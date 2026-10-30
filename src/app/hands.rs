//! One player's hands on the desktop host: attack, mine, use, eat and draw a
//! bow through the shared session, with sounds and the hand swing.
//!
//! An [`Actor`] borrows the acting player's own session, [`Body`] and hand
//! animation alongside the shared world, so the host, controller seats and
//! LAN peers run the same code without swapping state. What only the owning
//! adapter can do (screens, beds, popups, damage) comes back as [`Request`]s.
use glam::DVec3;

use crate::audio::{
    Audio, SoundSource,
    sounds::{Material, Sound},
};
use crate::entity::Entities;
use crate::item::Item;
use crate::world::World;
use crate::world::block::Block;
use voxelcraft::rules::GameRules;
use voxelcraft::session::{
    PlayerSession,
    interact::{UseContext, Used},
    ranged::Shot,
};

use super::gamepad::Body;
use super::hand::HandAnim;
use super::{ACTION_REPEAT, EAT_TIME, bucket, capitalize, items};

/// Something the acting player's adapter must handle.
#[derive(Debug, PartialEq)]
pub(super) enum Request {
    /// A message about this player's hands.
    Popup(String),
    /// A screen, bed, vehicle store or merchant to open for this player.
    Open(Used),
    /// Damage from something consumed (a harming potion), as magic.
    MagicDamage(f32),
    /// An attack's extra knockback stopped this player's sprint (`setSprinting(false)`).
    StopSprint,
}

pub(super) struct Actor<'a> {
    pub session: &'a mut PlayerSession,
    pub body: &'a mut Body,
    pub hand: &'a mut HandAnim,
    pub world: &'a mut World,
    pub entities: &'a mut Entities,
    pub audio: &'a mut Audio,
    pub gamerules: &'a GameRules,
    /// Sneaking builds against containers and beds instead of using them.
    pub sneaking: bool,
    pub sprinting: bool,
    /// Other players' positions, which block placement.
    pub others: &'a [DVec3],
    pub requests: Vec<Request>,
}

impl Actor<'_> {
    fn held(&self) -> Option<Item> {
        self.session.inventory.get(self.body.actions.selected).map(|s| s.item)
    }

    fn popup(&mut self, text: String) {
        self.requests.push(Request::Popup(text));
    }

    fn tool_broke(&mut self, item: Item) {
        self.popup(format!("{} broke", capitalize(item.name())));
        let eye = self.session.player.eye();
        // Java plays the item-break sound as the player (`PLAYERS`).
        self.audio.play_in(SoundSource::Players, Sound::Break(Material::Wood), Some(eye), 0.8, (1.3, 1.5));
    }

    /// Attack/mine button pressed or released. `hit_player` is whether the
    /// press already struck another player (LAN PvP), which the host resolves.
    pub fn attack_button(&mut self, pressed: bool, hit_player: bool) {
        if !self.session.mode.can_interact() {
            return;
        }
        self.body.left_held = pressed;
        if pressed {
            self.hand.swing();
        }
        if !pressed {
            self.body.actions.breaking = None;
            self.session.cancel_mining();
            self.body.attack_held = false;
        } else if hit_player || self.attack() {
            self.body.attack_held = true;
            self.body.actions.breaking = None;
            self.session.cancel_mining();
        } else {
            if let Some((p, _)) = self.session.block_target(self.world) {
                self.world.strike_note(p);
            }
            self.body.mine_pressed = true;
        }
    }

    /// Melee through the shared session transaction. Returns whether something was hit.
    fn attack(&mut self) -> bool {
        use voxelcraft::session::combat::{Outcome, Target as Hit};
        let slot = self.body.actions.selected;
        let result = self.session.attack(slot, self.world, self.entities, self.sprinting);
        let Ok(Outcome::Hit { target, at, critical, tool_broken, sprint_cancelled, .. }) = result else { return false };
        if sprint_cancelled {
            self.requests.push(Request::StopSprint);
        }
        let (volume, pitch) = match target {
            Hit::FenceKnot => (0.0, (1.0, 1.0)),
            Hit::Cart => (0.7, (0.8, 1.0)),
            Hit::Decor(effect) => {
                if let Some(effect) = effect {
                    super::utilities::decor_feedback(self.audio, at, effect);
                }
                (0.0, (1.0, 1.0))
            }
            Hit::Fireball => (0.7, (0.6, 0.8)),
            Hit::Fight => (0.8, (0.9, 1.1)),
            Hit::Mob(_) if critical => (0.8, (1.25, 1.4)),
            Hit::Mob(_) => (0.8, (0.9, 1.1)),
        };
        if volume > 0.0 {
            self.audio.play(Sound::Hit, Some(at), volume, pitch);
        }
        if let Some(item) = tool_broken {
            self.tool_broke(item);
        }
        true
    }

    /// Use button pressed or released.
    pub fn use_button(&mut self, pressed: bool) {
        if !self.session.mode.can_interact() {
            return;
        }
        self.body.right_held = pressed;
        self.body.used_once = false;
        if pressed && !self.equip_held() && !self.start_draw() {
            if self.held().is_none_or(|i| i.food().is_none()) {
                self.hand.swing();
            }
            let did = self.use_held();
            // A click nothing answered raises a shield held in either hand.
            if !did && self.session.consumable_in_hands(self.body.actions.selected).is_none() {
                self.session.start_blocking(self.body.actions.selected);
            }
            self.body.action_cooldown = ACTION_REPEAT;
        } else if !pressed {
            self.session.stop_blocking();
            self.session.cancel_consumption();
            self.body.actions.eat_timer = 0.0;
            if let Some(shot) = self.session.release_draw(self.body.actions.selected, self.entities) {
                self.present_shot(shot);
            }
        }
    }

    /// Held buttons for one fixed tick: mining, repeated placing, eating and
    /// drawing a bow. `aiming_at_target` is whether a mob, fight part or
    /// player is under the crosshair, which suppresses mining.
    pub fn act(&mut self, acting: bool, dt: f64, aiming_at_target: bool) {
        let mine_pressed = std::mem::take(&mut self.body.mine_pressed);
        self.body.action_cooldown -= dt;
        if acting && (self.body.left_held || mine_pressed) {
            // Shared session timing, including Java's destroy delay, in every mode.
            if self.body.attack_held || aiming_at_target {
                self.session.cancel_mining();
                self.body.actions.breaking = None;
            } else {
                self.mine(dt);
            }
            if self.body.actions.breaking.is_some() || self.session.mining_delay() > 0 {
                self.hand.swing();
            }
        } else if acting
            && self.body.action_cooldown <= 0.0
            && self.body.right_held
            && !self.session.drawing()
            && !self.session.shield_raised()
            // Buckets, snowballs, eggs and rods act once per click, in either hand.
            && !self.held().is_some_and(once_per_click)
            && !self.body.used_once
        {
            self.use_held();
            self.hand.swing();
            self.body.action_cooldown = ACTION_REPEAT;
        }
        if !self.body.left_held {
            self.body.actions.breaking = None;
            self.session.cancel_mining();
        }
        self.session.tick_blocking(self.body.actions.selected, acting && self.body.right_held);
        self.eat(acting, dt);
        if let Some(shot) = self.session.tick_draw(self.body.actions.selected, acting) {
            self.present_shot(shot);
        }
    }

    fn mine(&mut self, dt: f64) {
        use voxelcraft::session::mining::Outcome;
        let slot = self.body.actions.selected;
        match self.session.tick_mining(slot, self.world, self.entities, self.gamerules) {
            Ok(Outcome::Hit { cell, block, .. }) => self.audio.block_hit(block, cell, dt),
            Ok(Outcome::Broken { cell, block, tool_broken, .. }) => {
                self.audio.block_break(block, cell);
                if let Some(item) = tool_broken {
                    self.popup(format!("{} broke", capitalize(item.name())));
                }
            }
            Ok(Outcome::TeleportedEgg { destination, .. }) => {
                if let Some(to) = destination {
                    self.audio.play_in(SoundSource::Blocks, Sound::Teleport, Some(to.as_dvec3()), 0.6, (0.9, 1.1));
                }
            }
            // The session's destroy delay replaces the former adapter repeat cooldowns.
            Ok(Outcome::Delayed { .. }) => {}
            Err(_) => {}
        }
        self.body.actions.breaking = self.session.breaking(self.world);
    }

    /// Held use advances the shared fixed-tick consumption clock.
    fn eat(&mut self, acting: bool, dt: f64) {
        let held = self.session.consumable_in_hands(self.body.actions.selected).or(self.held());
        let drink = held.is_some_and(Item::is_drink);
        let food = held.and_then(|i| i.food());
        let hungry = held.is_some_and(|item| self.session.can_eat(item));
        // Right-clicking a composter fills it instead, as in Java.
        let composting = !self.sneaking
            && self
                .session
                .block_target(self.world)
                .and_then(|(pos, _)| self.world.get_block(pos))
                .is_some_and(|b| crate::world::composter::level(b).is_some());
        let using = acting && self.body.right_held && (drink || (food.is_some() && hungry && !composting));
        if !using {
            self.session.cancel_consumption();
            self.body.actions.eat_timer = 0.0;
            return;
        }
        let before = self.session.consumption_elapsed();
        let result = self.session.tick_consumption_either(self.body.actions.selected, using, self.entities);
        let Ok(step) = result else {
            self.session.cancel_consumption();
            self.body.actions.eat_timer = 0.0;
            return;
        };
        // This field is presentation only; progress belongs to the session.
        self.body.actions.eat_timer = f64::from(step.progress) * EAT_TIME;
        if (before / 0.25).floor() != ((before + dt) / 0.25).floor() {
            let eye = self.session.player.eye();
            if held == Some(Item::HONEY_BOTTLE) {
                self.audio.play(Sound::HoneyDrink, Some(eye), 0.7, (0.9, 1.1));
            } else {
                self.audio.play(Sound::Step(Material::Snow), Some(eye), 0.7, (1.4, 1.7));
            }
        }
        if let Some(consumed) = step.completed {
            self.requests.push(Request::MagicDamage(consumed.magic_damage));
        }
    }

    /// Right-click with armor puts it on (swapping out the worn piece),
    /// unless aimed at a container. Returns whether it did.
    fn equip_held(&mut self) -> bool {
        let slot = self.body.actions.selected;
        if self.session.block_takes_use(slot, self.world, self.sneaking) {
            return false;
        }
        let s = &mut *self.session;
        if !voxelcraft::survival_items::equip_armor(&s.player, &mut s.inventory, slot, s.mode, self.world) {
            return false;
        }
        let eye = self.session.player.eye();
        self.audio.play_in(SoundSource::Players, Sound::Place(Material::Wood), Some(eye), 0.6, (1.4, 1.6));
        true
    }

    fn start_draw(&mut self) -> bool {
        let shot = self.session.start_draw(self.body.actions.selected, self.world, self.entities);
        shot.inspect(|s| self.present_shot(*s)).is_some()
    }

    fn present_shot(&mut self, shot: Shot) {
        items::shot_sound(self.audio, shot.ranged);
        if let Some(item) = shot.tool_broken {
            self.popup(format!("{} broke", capitalize(item.name())));
        }
    }

    /// The shared right-click chain, with its sounds; screens and beds are requested.
    fn use_held(&mut self) -> bool {
        let ctx = UseContext { sneaking: self.sneaking, others: self.others };
        let slot = self.body.actions.selected;
        let off = self.session.inventory.offhand.map(|s| s.item);
        let Ok(Some(done)) = self.session.use_held(slot, self.world, self.entities, &ctx) else { return false };
        if done.off_hand && off.is_some_and(once_per_click) {
            self.body.used_once = true;
        }
        if let Some(item) = done.tool_broken {
            self.tool_broke(item);
        }
        let centre = |cell: glam::IVec3| cell.as_dvec3() + DVec3::splat(0.5);
        // Java's throw is the bow sound, pitched well down.
        let throw = (0.5, (0.42, 0.62));
        let cushion = |id: u32| self.entities.cart(id).is_some_and(|c| c.kind.cushion().is_some());
        let sat_on_cushion = matches!(done.used, Used::Mounted(id) if cushion(id));
        let audio = &mut *self.audio;
        match done.used {
            Used::OpenVehicle(_)
            | Used::OpenMerchant(_)
            | Used::Screen { .. }
            | Used::Bed { .. }
            | Used::EditSign { .. } => {
                self.requests.push(Request::Open(done.used));
            }
            // Dyes, ink sacs and glow ink sacs on signs; shelves and chiseled
            // bookshelves (nearest existing sounds).
            Used::SignInked { cell, .. } => {
                audio.play(Sound::Place(Material::Wood), Some(centre(cell)), 0.6, (1.3, 1.5))
            }
            Used::Shelf { cell, used } => {
                use crate::world::wood::ShelfUse;
                let pitch = match used {
                    ShelfUse::Placed | ShelfUse::BookIn { .. } => (1.0, 1.1),
                    ShelfUse::Swapped { .. } | ShelfUse::BookOut { .. } => (1.2, 1.3),
                    ShelfUse::Hotbar => (0.8, 0.9),
                    ShelfUse::Nothing => return true,
                };
                audio.play(Sound::Place(Material::Wood), Some(centre(cell)), 0.8, pitch);
            }
            Used::Bucket(effect) => bucket::feedback(audio, effect),
            // Pearls, snowballs, eggs and fishing rods play in `NEUTRAL`, splash potions in `PLAYERS`.
            Used::PearlThrown { from } | Used::ProjectileThrown { from } | Used::Rod { from, broke: false } => {
                audio.play_in(SoundSource::Neutral, Sound::Bow, Some(from), throw.0, throw.1)
            }
            Used::PotionThrown { from } => audio.play(Sound::Bow, Some(from), throw.0, throw.1),
            Used::EyeReleased { from } => items::eye_thrown_sound(audio, from),
            Used::EyeInserted { cell, opened } => items::frame_filled_sounds(audio, cell, opened),
            Used::EggTeleported { destination: Some(to) } => {
                audio.play_in(SoundSource::Blocks, Sound::Teleport, Some(to.as_dvec3()), 0.6, (0.9, 1.1));
            }
            Used::Harvested { cell } => audio.play(Sound::Break(Material::Grass), Some(centre(cell)), 0.8, (1.0, 1.2)),
            Used::Door { cell, open: Some(open) } => {
                audio.play(Sound::Door(open), Some(centre(cell)), 0.8, (0.9, 1.1));
            }
            // TNT's fuse, flint and steel and fire are `BLOCKS` in Java.
            Used::TntPrimed { cell } => {
                audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.95, 1.05))
            }
            Used::Ignited { cell, portal } => {
                audio.play(Sound::Place(Material::Stone), Some(cell.as_dvec3()), 0.7, (1.6, 1.9));
                if portal {
                    audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.5, 0.6));
                }
            }
            Used::Stripped { cell } => audio.play(Sound::Place(Material::Wood), Some(centre(cell)), 1.0, (0.8, 1.0)),
            // Nearest shared sound for `LODESTONE_COMPASS_LOCK` (`PLAYERS`).
            Used::CompassLocked { cell } => {
                audio.play_in(SoundSource::Players, Sound::Place(Material::Stone), Some(centre(cell)), 1.0, (1.4, 1.6))
            }
            // Nearest shared sound for `item.axe.scrape`: a high, scratchy stone step.
            Used::Scraped { cell } => audio.play(Sound::Step(Material::Stone), Some(centre(cell)), 1.0, (1.5, 1.8)),
            // `CANDLE_EXTINGUISH` and `CAKE_ADD_CANDLE`, from the shared sound set.
            Used::CandleExtinguished { cell } => {
                audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(centre(cell)), 0.3, (1.8, 2.0))
            }
            Used::CandlePlacedOnCake { cell } => {
                audio.play_in(SoundSource::Blocks, Sound::Place(Material::Snow), Some(centre(cell)), 1.0, (1.0, 1.0))
            }
            // `CUSHION_PLACE` (0.75, 0.8) and `CUSHION_SIT`, from the shared wool set.
            Used::CushionPlaced { at } => {
                audio.play_in(SoundSource::Blocks, Sound::Place(Material::Snow), Some(at), 0.75, (0.8, 0.8))
            }
            Used::Mounted(_) if sat_on_cushion => {
                let at = self.session.player.pos;
                audio.play_in(SoundSource::Neutral, Sound::Step(Material::Snow), Some(at), 1.0, (0.8, 0.9))
            }
            Used::Tilled { cell } => {
                audio.play_in(SoundSource::Blocks, Sound::Step(Material::Gravel), Some(centre(cell)), 1.0, (0.8, 0.9))
            }
            Used::Carved { cell } => audio.play(Sound::Break(Material::Wood), Some(centre(cell)), 0.8, (0.9, 1.1)),
            Used::BoneMeal { cell } => audio.play(Sound::Place(Material::Grass), Some(centre(cell)), 0.8, (1.2, 1.4)),
            Used::Utility { cell, effect } => {
                super::utilities::feedback(audio, cell, effect);
                self.requests.push(Request::Open(done.used));
            }
            Used::Decor { at, effect } => super::utilities::decor_feedback(audio, at, effect),
            Used::Placed(placement) => {
                use voxelcraft::session::placement::{Kind, Outcome};
                match placement {
                    Outcome::Placed { cell, block, kind, .. } => {
                        audio.block_place(if kind == Kind::Bed { Block::WOOL } else { block }, cell);
                        // `SignBlock.setPlacedBy`: the placer edits the front.
                        if crate::world::wood::signs::sign(block).is_some() {
                            self.requests.push(Request::Open(Used::EditSign { cell, front: true }));
                        }
                    }
                    Outcome::WaterEvaporated { cell, .. } => {
                        audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 0.6, (1.6, 1.8));
                    }
                }
            }
            _ => {}
        }
        true
    }

    /// Swap-item-with-offhand: the selected stack and the off-hand trade places.
    pub fn swap_hands(&mut self) -> bool {
        let swapped = self.session.swap_hands(self.body.actions.selected);
        if swapped {
            self.body.actions.eat_timer = 0.0;
            let eye = self.session.player.eye();
            self.audio.play_in(SoundSource::Players, Sound::Place(Material::Wood), Some(eye), 0.4, (1.2, 1.4));
        }
        swapped
    }
}

/// Buckets, snowballs, eggs and rods act once per click, not again while
/// the button is held.
fn once_per_click(i: Item) -> bool {
    (i != Item::GLASS_BOTTLE && voxelcraft::survival_items::bucket::handles(i))
        || matches!(i, Item::SNOWBALL | Item::EGG | Item::FISHING_ROD)
}
