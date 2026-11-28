//! One player's hands on the desktop host: attack, mine, use, eat and draw a
//! bow through the shared session, with sounds and the hand swing.
//!
//! An [`Actor`] borrows the acting player's own session, [`Body`] and hand
//! animation alongside the shared world, so the host, controller seats and
//! LAN peers run the same code without swapping state. What only the owning
//! adapter can do (screens, beds, popups, damage) comes back as [`Request`]s.
use glam::DVec3;
use voxelcraft::world::vibration::GameEvent as ItemUse;

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

#[cfg(test)]
mod tests;

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
        // v1.0 gap Track 2g: `breakItem`'s five item particles.
        let forward = self.session.player.forward().as_dvec3();
        let seed = eye.x.to_bits() ^ eye.z.to_bits().rotate_left(21) ^ u64::from(item.0);
        for p in crate::particles::living_item_particles(item, eye, forward, 5, seed) {
            self.world.particles.push(p);
        }
    }

    /// v1.0 gap Track 2g: `Consumable.emitParticlesAndSounds`' item particles
    /// for food (drinks have none).
    fn eating_particles(&mut self, item: Item, count: u32, seed: u64) {
        let eye = self.session.player.eye();
        let forward = self.session.player.forward().as_dvec3();
        for p in crate::particles::living_item_particles(item, eye, forward, count, seed ^ eye.x.to_bits()) {
            self.world.particles.push(p);
        }
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
                // `NoteBlock.attack`: the striking player is the event source.
                let source =
                    crate::world::vibration::EventSource::player(self.session.id, self.session.player.sneaking);
                self.world.strike_note_by(p, source);
            }
            self.body.mine_pressed = true;
        }
    }

    /// Melee through the shared session transaction. Returns whether something was hit.
    fn attack(&mut self) -> bool {
        use voxelcraft::session::combat::{Outcome, Target as Hit};
        let slot = self.body.actions.selected;
        let spear = self.held().is_some_and(voxelcraft::weapons::is_spear);
        let result = self.session.attack(slot, self.world, self.entities, self.sprinting);
        if let Ok(Outcome::Jab { tool_broken, attack, hit, lunge, from, .. }) = result {
            super::items::weapon_sound(self.audio, attack, from);
            if let Some(hit) = hit {
                super::items::weapon_sound(self.audio, hit, from);
            }
            if let Some(level) = lunge {
                super::items::weapon_sound(self.audio, voxelcraft::weapons::Cue::lunge(level), from);
            }
            if let Some(item) = tool_broken {
                self.tool_broke(item);
            }
            return true;
        }
        // A spear never starts mining (`Minecraft.continueAttack` skips piercing weapons).
        if spear {
            return true;
        }
        let Ok(Outcome::Hit { target, at, critical, tool_broken, sprint_cancelled, smash, .. }) = result else {
            return false;
        };
        if let Some(smash) = smash {
            super::items::weapon_sound(self.audio, smash.cue, smash.at);
            if let Some(center) = smash.wind_burst {
                if smash.cue != voxelcraft::weapons::Cue::WindBurst {
                    super::items::weapon_sound(self.audio, voxelcraft::weapons::Cue::WindBurst, center);
                }
                voxelcraft::weapons::mace::wind_burst_blocks(self.world, center);
            }
        }
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
        self.body.main_used = false;
        if pressed && !self.equip_held() && !self.start_draw() {
            if self.held().is_none_or(|i| i.food().is_none()) {
                self.hand.swing();
            }
            let mut did = self.use_held();
            // v1.0 final track F1: a bow or crossbow in the off-hand draws
            // when the main hand's use passed.
            if !did
                && let Some(shot) =
                    self.session.start_offhand_draw(self.body.actions.selected, self.world, self.entities)
            {
                self.present_shot(shot);
                did = true;
            }
            // A click nothing answered raises a spyglass (`SpyglassItem.use`)
            // or a shield held in either hand.
            if !did && self.session.start_scoping(self.body.actions.selected) {
                let eye = self.session.player.eye();
                self.audio.play_in(SoundSource::Players, Sound::Click, Some(eye), 0.4, (1.6, 1.7));
                self.session.use_vibration(self.entities, ItemUse::ItemInteractStart);
            } else if !did
                && self.session.consumable_in_hands(self.body.actions.selected).is_none()
                && self.session.start_blocking(self.body.actions.selected)
            {
                // v1.0 Track 9: raising a shield is `startUsingItem`.
                self.session.use_vibration(self.entities, ItemUse::ItemInteractStart);
            }
            self.body.action_cooldown = ACTION_REPEAT;
        } else if !pressed {
            if self.session.stop_scoping() {
                let eye = self.session.player.eye();
                self.audio.play_in(SoundSource::Players, Sound::Click, Some(eye), 0.4, (1.2, 1.3));
                self.session.use_vibration(self.entities, ItemUse::ItemInteractFinish);
            }
            // v1.0 Track 9: lowering a shield is `stopUsingItem`.
            if self.session.shield_raised() {
                self.session.use_vibration(self.entities, ItemUse::ItemInteractFinish);
            }
            self.session.stop_blocking();
            self.session.cancel_consumption();
            self.body.actions.eat_timer = 0.0;
            if let Some(shot) = self.session.release_draw(self.body.actions.selected, self.world, self.entities) {
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
            && !self.session.scoping()
            && !self.session.consuming()
            && !self.session.brushing()
            && !self.session.bundle_in_use()
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
        if let Some(stroke) =
            self.session.tick_brushing(self.body.actions.selected, acting && self.body.right_held, self.world)
        {
            self.hand.swing();
            brush_sound(self.audio, stroke);
        }
        // v1.0 gap track 6: held use throws a bundle's contents out.
        if let Some(out) =
            self.session.tick_bundle_use(self.body.actions.selected, acting && self.body.right_held, self.entities)
        {
            bundle_thrown(self.hand, self.audio, out);
        }
        self.session.tick_blocking(self.body.actions.selected, acting && self.body.right_held);
        self.session.tick_scoping(self.body.actions.selected, acting && self.body.right_held);
        self.session.tick_tooting(self.body.actions.selected, acting && self.body.right_held);
        self.eat(acting);
        if let Some(shot) = self.session.tick_draw(self.body.actions.selected, acting, self.world, self.entities) {
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
                    self.tool_broke(item);
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
    fn eat(&mut self, acting: bool) {
        let slot = self.body.actions.selected;
        // Java uses the off-hand only when the main hand's use passes: a
        // click that placed a block doesn't also eat off-hand food.
        let off_allowed =
            !self.body.main_used || self.session.consuming() || self.session.consumption_duration(slot).is_ok();
        let held = if off_allowed { self.session.consumable_in_hands(slot).or(self.held()) } else { self.held() };
        let drink = held.is_some_and(Item::is_drink);
        let food = held.and_then(|i| i.food());
        let hungry = held.is_some_and(|item| self.session.can_eat(item));
        let targeted = self.session.block_target(self.world).and_then(|(pos, _)| self.world.get_block(pos));
        // Right-clicking a composter fills it instead, as in Java. A statue
        // consumes block use before food or drink, except when sneaking.
        // Looking at a statue during an existing bite does not interrupt it.
        let composting = !self.sneaking && targeted.is_some_and(|b| crate::world::composter::level(b).is_some());
        let statue_use = !self.session.consuming()
            && self.session.player.vehicle.is_none()
            && !self.sneaking
            && targeted.and_then(crate::world::copper_golem_statue::decode).is_some();
        let using =
            acting && self.body.right_held && !statue_use && (drink || (food.is_some() && hungry && !composting));
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
        // v1.0 gap Track 2h: on Java's rhythm (`shouldEmitParticlesAndSounds`).
        if step.effects {
            let eye = self.session.player.eye();
            if held == Some(Item::HONEY_BOTTLE) {
                self.audio.play(Sound::HoneyDrink, Some(eye), 0.7, (0.9, 1.1));
            } else {
                self.audio.play(Sound::Step(Material::Snow), Some(eye), 0.7, (1.4, 1.7));
            }
            if let Some(item) = held.filter(|_| !drink) {
                self.eating_particles(item, 5, before.to_bits());
            }
        }
        if let Some(consumed) = step.completed {
            if !drink {
                self.eating_particles(consumed.item, 16, before.to_bits() ^ 0x16);
            }
            // v0.16 Track A: chorus fruit teleports (`CHORUS_FRUIT_TELEPORT`, `PLAYERS`).
            if consumed.item == voxelcraft::content::chorus::CHORUS_FRUIT
                && let Some(from) = self.session.chorus_teleport(self.world, self.entities)
            {
                let to = self.session.player.pos;
                for at in [from, to] {
                    self.audio.play_in(
                        crate::audio::SoundSource::Players,
                        Sound::Teleport,
                        Some(at + DVec3::Y),
                        1.0,
                        (0.9, 1.1),
                    );
                }
            }
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
        let elytra = s.inventory.get(slot).is_some_and(|s| s.item == voxelcraft::content::elytra::ELYTRA);
        if !voxelcraft::survival_items::equip_armor(&s.player, &mut s.inventory, slot, s.mode, self.world) {
            return false;
        }
        let eye = self.session.player.eye();
        if elytra {
            // v0.16 Track B: `ARMOR_EQUIP_ELYTRA`.
            let sound = Sound::Flight(voxelcraft::content::fireworks::Cue::EquipElytra);
            self.audio.play_in(SoundSource::Players, sound, Some(eye), 1.0, (1.0, 1.0));
            return true;
        }
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
            self.tool_broke(item);
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
        self.body.main_used |= !done.off_hand;
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
            | Used::EditSign { .. }
            | Used::Lectern { .. }
            | Used::OpenBook { .. }
            | Used::LecternBook { .. } => {
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
            Used::HornPlayed { instrument, from } => super::items::horn_sound(audio, instrument, from),
            Used::WindChargeThrown { from } => {
                let cue = Sound::Breeze(voxelcraft::entity::breeze::Cue::Throw);
                audio.play_in(SoundSource::Neutral, cue, Some(from), 0.5, throw.1)
            }
            Used::Vault { cell, unlocked, rewarded, sound } => {
                use voxelcraft::entity::breeze::Cue;
                let cue = if unlocked {
                    Cue::VaultInsert
                } else if rewarded {
                    Cue::VaultReject
                } else {
                    Cue::VaultFail
                };
                if sound {
                    audio.play_in(SoundSource::Blocks, Sound::Breeze(cue), Some(centre(cell)), 1.0, (0.9, 1.1));
                }
            }
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
            // v1.0 gap Track 8f: a fire charge's `FIRECHARGE_USE` (the
            // blaze's shot, `mob/ghast/fireball4` in Java's sound set), pitch
            // `(r - r) * 0.2 + 1`.
            Used::Ignited { cell, portal, charge: true } => {
                audio.play_in(SoundSource::Blocks, Sound::Fireball, Some(cell.as_dvec3()), 1.0, (0.8, 1.2));
                if portal {
                    audio.play_in(SoundSource::Blocks, Sound::Fuse, Some(cell.as_dvec3()), 1.0, (0.5, 0.6));
                }
            }
            Used::Ignited { cell, portal, charge: false } => {
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
            Used::Waxed { cell } => audio.play(Sound::Place(Material::Snow), Some(centre(cell)), 1.0, (0.9, 1.1)),
            Used::Scraped { cell } => audio.play(Sound::Step(Material::Stone), Some(centre(cell)), 1.0, (1.5, 1.8)),
            Used::Statue { .. } | Used::BrushStarted | Used::BundleStarted => {}
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
            // v1.0 gap Track 2h: level event 1505's sound is the world's.
            Used::BoneMeal { .. } => {}
            Used::Utility { cell, effect } => {
                super::utilities::feedback(audio, cell, effect);
                self.requests.push(Request::Open(done.used));
            }
            Used::Decor { at, effect } => super::utilities::decor_feedback(audio, at, effect),
            // `EndCrystalItem` plays no sound (only `ENTITY_PLACE`).
            Used::CrystalPlaced { .. } => {}
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
/// Java `BrushItem.onUseTick` plays the brushed block's brush sound each
/// stroke (`BrushableBlock.getBrushSound`, else `BRUSH_GENERIC`; `BLOCKS`,
/// volume and pitch 1), and level event 3008 its completed sound
/// (`PLAYERS`, volume and pitch 1) as the find comes free.
pub(super) fn brush_sound(audio: &mut Audio, stroke: voxelcraft::session::Stroke) {
    use crate::audio::horns::BrushCue;
    let at = Some(stroke.pos.as_dvec3() + DVec3::splat(0.5));
    let gravel = voxelcraft::world::archaeology::decode(stroke.block).map(|(gravel, _)| gravel);
    let cue = match gravel {
        Some(true) => BrushCue::Gravel,
        Some(false) => BrushCue::Sand,
        None => BrushCue::Generic,
    };
    audio.play_in(SoundSource::Blocks, Sound::Brush(cue), at, 1.0, (1.0, 1.0));
    if stroke.completed {
        let done = if gravel == Some(true) { BrushCue::GravelComplete } else { BrushCue::SandComplete };
        audio.play_in(SoundSource::Players, Sound::Brush(done), at, 1.0, (1.0, 1.0));
    }
}

fn once_per_click(i: Item) -> bool {
    (i != Item::GLASS_BOTTLE && voxelcraft::survival_items::bucket::handles(i))
        || matches!(i, Item::SNOWBALL | Item::FISHING_ROD)
        || voxelcraft::content::farm_variant::is_egg(i)
}

/// v1.0 gap track 6: a stack thrown out of a bundle in use. `Player.drop`
/// swings the main hand; `removeOneItemFromBundle` plays the remove sound
/// at the player and `dropContent` the drop-contents sound at the player's
/// block.
pub(super) fn bundle_thrown(hand: &mut HandAnim, audio: &mut Audio, out: voxelcraft::session::bundles::Emptied) {
    use voxelcraft::content::bundle::Cue;
    hand.swing();
    items::bundle_sound(audio, Cue::RemoveOne, Some(out.at));
    let block = out.at.floor() + DVec3::splat(0.5);
    items::bundle_sound(audio, Cue::DropContents, Some(block));
}
