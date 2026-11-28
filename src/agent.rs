//! Device-independent player commands and authoritative agent sessions.
//! Agents submit input, never positions or inventory edits (unless cheats are enabled).

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use crate::crafting;
use crate::entity::Entities;
use crate::inventory::{Inventory, Stack};
use crate::item::Item;
use crate::player::MoveInput;
use crate::rules::GameMode;
use crate::simulation::damage::Cause;
use crate::simulation::{self, TICK_SECONDS};
use crate::world::{World, block::Block};
use command::CartographyAction;

pub const HELP: &str = "observe [0..2] | catalog [query] (catalog tabs lists the creative tabs; catalog tab:<id> [query] one tab in Java's order) | hotbar save|load 1..9 (creative: the saved hotbars, as C/X + a number key) | players | look yaw pitch | move forward right ticks [jump sprint sneak] | wait ticks | mine ticks | eat (or drink, or hold a brush for 200 ticks) | draw [ticks] (hold use with a bow or crossbow: a bow shoots at that draw, a crossbow loads after 25 ticks; a charged crossbow fires at once) | sleep | place (throws a selected ender pearl, splash potion or eye of ender, puts the eye in a targeted End portal frame, or launches a firework rocket, which boosts a glide; a fresh jump in mid-air with elytra worn glides) | attack | select 1..9 | fly on/off | craft item | recipe id [max] (the recipe book's click on an unlocked recipe: fills the 2x2 grid, or an aimed crafting table's 3x3, from the inventory, one set or every set with max, and takes the results) | recipes [all|craftable] [query] (the unlocked recipes the book shows, craftable and new marked) | chest take/put slot (an aimed ender chest holds your own items) | bundle slot [stack] (take a stack out of the bundle in inventory slot 0..35, the front one or tooltip cell 0..11 as the wheel picks it) | crafter toggle 0..8 (turn an empty slot of an aimed crafter off or on; observe shows its slots, result and power) | sign x y z front|back [\"line\", ...] (write one side of a sign within reach) | furnace take/put input/fuel/output | enchanting 1..3 (an aimed enchanting table's offer for the held item) | anvil 1..9 (combine the held stack with that hotbar slot on an aimed anvil) | grindstone [1..9] (disenchant or combine held gear) | trade [1..10] (inspect or buy a targeted villager offer) | smithing (upgrade held diamond gear using a template and ingot) | stonecut item (cut the held stack at an aimed stonecutter) | loom pattern dye (weave onto the held banner at an aimed loom) | cartography copy|zoom|lock (the held map at an aimed cartography table) | book write [offhand] [\"page\", ...] | book sign [offhand] \"title\" [\"page\", ...] (the book and quill in the selected slot, or the off-hand) | lectern x y z page n|take | drop | respawn | leave. Cheats: give [@s|@p] item [count], clear, kill, summon mob [x y z], gamemode mode [@s|@p], tp|teleport [@s] x y z, tp @s x y z yaw pitch|facing x y z|facing entity @s [eyes|feet], rotate @s yaw pitch|facing x y z|facing entity @s [eyes|feet] (Java degrees: yaw 0 south, 90 west; pitch -90 up; ~ is relative), spawnpoint [x y z], setblock x y z block, time set/add/query, weather clear/rain/thunder, xp|experience add/set/query, effect give/clear, enchant name [level], say message. Host console only: difficulty, gamerule, seed, setworldspawn, locate structure|biome, dimension overworld/nether/end.";

/// Something an agent did that players nearby should hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Broke(IVec3, Block),
    Placed(IVec3, Block),
    Bucket(crate::survival_items::bucket::Effect),
    Chew,
    HoneyDrink,
    /// v1.0 gap Track 2h: `LivingEntity.spawnItemParticles` while eating
    /// `Item` (5 a chew, 16 on finishing; drinks have none).
    EatParticles(Item, u32),
    /// Drew, loaded, fired or shot a bow or crossbow.
    Shot(crate::session::ranged::Shot),
    /// Absorbed an experience orb; with the level-up chime volume when it
    /// reached a multiple of five levels.
    Xp(Option<f32>),
    /// Released an eye of ender from here.
    EyeThrown(DVec3),
    /// Put an eye in the End portal frame here; whether that opened the portal.
    FrameFilled(IVec3, bool),
    /// v1.0 gap track 6: threw a stack out of the bundle in use.
    BundleThrown(crate::session::bundles::Emptied),
    /// v1.0 final Track F4: a brush stroke (its sounds).
    Brushed(crate::session::Stroke),
    /// v1.0 final Track F4: blew a goat horn of this instrument from here.
    Horn(u8, DVec3),
}

/// Unheard events kept per agent (a host without audio never drains them).
const MAX_EVENTS: usize = 64;

/// Ticks to eat one food item (1.6 s).
pub const EAT_TICKS: u32 = 32;

pub mod command;
pub use command::{
    Anchor, Command, Coordinate, EffectChange, PositionSpec, TimeQuery, Turn, WeatherKind, XpChange, tab_complete,
};

pub struct Agent {
    pub session: crate::session::PlayerSession,
    pub previous_pos: DVec3,
    pub creative: bool,
    pub selected: usize,
    pub remaining: u32,
    /// Arm swings so far (placing, attacking, mining), for animation.
    pub swings: u32,
    /// Sounds to play, drained by the host.
    pub events: Vec<Event>,
    input: MoveInput,
    mining: bool,
    /// Holding "use" eats held food; `bite` counts the ticks chewed.
    eating: bool,
    bite: u32,
    /// Holding "use" with a bow or crossbow until `remaining` ticks run out.
    drawing: bool,
    cooldown: f64,
    /// The world's `keepInventory` rule, refreshed every tick so deaths
    /// outside `tick_rules` (mobs, pearls, `kill`) honour it too.
    keep_inventory: bool,
    /// The `ender_pearls_vanish_on_death` rule, as of the last rules tick.
    pearls_vanish: bool,
    /// The fall, fire and drowning rules as of the latest tick.
    immunity: crate::rules::DamageImmunity,
    /// v0.18 Track 5: the `limited_crafting` rule as of the last rules tick.
    limited_crafting: bool,
}

impl std::ops::Deref for Agent {
    type Target = crate::session::PlayerSession;
    fn deref(&self) -> &Self::Target {
        &self.session
    }
}
impl std::ops::DerefMut for Agent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

impl Agent {
    /// Creates an idle survival player with empty inventory at `pos`.
    pub fn new(pos: DVec3) -> Self {
        Self {
            session: crate::session::PlayerSession::new(crate::entity::PlayerId::default(), pos),
            previous_pos: pos,
            creative: false,
            selected: 0,
            remaining: 0,
            swings: 0,
            events: Vec::new(),
            input: MoveInput::default(),
            mining: false,
            eating: false,
            bite: 0,
            drawing: false,
            cooldown: 0.0,
            keep_inventory: false,
            pearls_vanish: true,
            immunity: Default::default(),
            limited_crafting: false,
        }
    }

    pub fn target(&self, world: &World) -> Option<(IVec3, IVec3)> {
        self.session.block_target(world)
    }

    /// The shared bucket/bottle step, after entity interactions. `None` means
    /// the held item is not a bucket or the targeted block's use comes first.
    /// Changes abilities together so command, host and controller paths agree.
    pub fn set_mode(&mut self, mode: GameMode) {
        self.session.set_mode(mode);
        self.creative = mode.is_creative();
    }

    /// Hardcore death has already dropped inventory and experience; revive
    /// only as a non-interacting spectator.
    pub fn hardcore_spectate(&mut self) {
        if self.session.vitals.is_dead() {
            self.session.vitals.respawn();
            self.set_mode(GameMode::Spectator);
            self.remaining = 0;
            self.session.sleeping = None;
        }
    }

    fn furnace_transfer(
        &mut self,
        take: bool,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
    ) -> Result<(), String> {
        if slot > 2 {
            return Err("furnace slot out of range".into());
        }
        let (pos, _) = self.target(world).ok_or("no furnace within reach")?;
        let f = world.furnace(pos).ok_or("target is not a furnace")?;
        let before_output = f.output.map_or(0, |s| s.count);
        let existing = [f.input, f.fuel, f.output][slot];
        let next = if take {
            let stack = existing.ok_or("furnace slot empty")?;
            let left = self.session.inventory.add_stack(stack);
            if left == stack.count {
                return Err("inventory full".into());
            }
            (left > 0).then_some(Stack { count: left, ..stack })
        } else {
            if slot == 2 {
                return Err("furnace output only permits taking items".into());
            }
            let stack = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
            if slot == 1 && crate::world::furnace::burn_time(stack.item).is_none() {
                return Err("item is not furnace fuel".into());
            }
            let room = match existing {
                None => stack.max(),
                Some(s) if s.stacks_with(&stack) => s.max() - s.count,
                _ => 0,
            };
            let count = room.min(stack.count);
            if count == 0 {
                return Err("furnace slot full or incompatible".into());
            }
            self.session.inventory.slots[self.selected] =
                (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
            Some(Stack { count: existing.map_or(0, |s| s.count) + count, ..stack })
        };
        let f = world.furnace_mut(pos).unwrap();
        match slot {
            0 => {
                // Putting items in is Java's `setItem` (a different item
                // restarts cooking); taking some out isn't.
                if take {
                    f.input = next;
                } else {
                    f.set_input(next);
                }
            }
            1 => f.fuel = next,
            _ => f.output = next,
        }
        if take && slot == 2 {
            let furnace = world.furnace_mut(pos).unwrap();
            let (kind, used): (_, Vec<Item>) = (furnace.kind, furnace.recipes_used.iter().map(|r| r.recipe).collect());
            if let Some(xp) = furnace.take_output_xp(before_output, || entities.roll()) {
                // v0.18 Track 5: the take teaches the recipes the tally holds.
                self.session.recipe_book.cooked(kind, &used);
                entities.spawn_xp(self.session.player.pos, xp);
            }
        }
        Ok(())
    }

    /// [`Agent::execute`] with the world's rules as they are now. A request
    /// can arrive before this profile's first rules tick (a restored or new
    /// player), and crafting must still honour `limited_crafting`.
    pub fn execute_with_rules(
        &mut self,
        command: Command,
        rules: &crate::rules::GameRules,
        world: &mut World,
        entities: &mut Entities,
        others: &[DVec3],
    ) -> Result<(), String> {
        self.limited_crafting = rules.bool("limited_crafting");
        self.execute(command, world, entities, others)
    }

    /// Execute immediate actions on the authority; timed inputs complete after real game ticks.
    /// Container transfers and crafting validate capacity before committing either side.
    pub fn execute(
        &mut self,
        command: Command,
        world: &mut World,
        entities: &mut Entities,
        others: &[DVec3],
    ) -> Result<(), String> {
        if self.session.vitals.is_dead() && !matches!(command, Command::Respawn | Command::Observe(_) | Command::Help) {
            return Err("player is dead; respawn first".into());
        }
        if self.session.mode == GameMode::Spectator
            && !matches!(
                command,
                Command::Help
                    | Command::Observe(_)
                    | Command::Players
                    | Command::Catalog(_)
                    | Command::Recipes(..)
                    | Command::Look(..)
                    | Command::Run(..)
                    | Command::Fly(..)
                    | Command::Mode(..)
                    | Command::Teleport(..)
                    | Command::Rotate(..)
                    | Command::SpawnPoint(..)
                    | Command::Summon(..)
                    | Command::SummonLightning(..)
                    | Command::Clear
                    | Command::Kill
                    | Command::Say(..)
                    | Command::Leave
            )
        {
            return Err("spectators cannot interact".into());
        }
        let resting = match &command {
            Command::Observe(_)
            | Command::Help
            | Command::Players
            | Command::Catalog(_)
            | Command::Recipes(..)
            | Command::Sleep
            | Command::XpQuery => true,
            Command::Run(input, _, mine) => !mine && input.forward == 0.0 && input.right == 0.0 && !input.jump,
            _ => false,
        };
        if !resting {
            self.session.sleeping = None;
        }
        match command {
            Command::Run(input, n, mine) => {
                if mine && !self.session.mode.can_build() {
                    return Err("this game mode cannot break blocks".into());
                }
                self.input = input;
                self.remaining = n;
                self.mining = mine;
                self.eating = false;
                self.session.cancel_brushing();
                self.session.cancel_consumption();
                self.session.cancel_mining();
            }
            Command::SwapHands => {
                if !self.session.swap_hands(self.selected) {
                    return Err("cannot swap hands now".into());
                }
                self.bite = 0;
                self.eating = false;
                self.session.cancel_brushing();
            }
            Command::Block(duration) => {
                if self.session.shield_cooldown() > 0 {
                    return Err("the shield is disabled".into());
                }
                if !self.session.start_blocking(self.selected) {
                    return Err("hold a shield in either hand".into());
                }
                self.input = MoveInput::default();
                self.remaining = duration;
                self.mining = false;
                self.eating = true;
                self.bite = 0;
            }
            Command::Eat => {
                let duration = if self.session.start_brushing(self.selected, world) {
                    200
                } else if self.session.start_bundle_use(self.selected, entities) {
                    // v1.0 gap track 6: holding use empties a bundle.
                    crate::session::bundles::USE_TICKS
                } else {
                    self.session.consumption_duration_either(self.selected)?
                };
                self.input = MoveInput::default();
                self.remaining = duration;
                self.mining = false;
                self.eating = true;
                self.bite = 0;
                self.session.cancel_consumption();
            }
            Command::Draw(duration) => {
                if !self.session.mode.can_interact() {
                    return Err("this game mode cannot use items".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                self.session.cancel_brushing();
                let Some(shot) = self.session.start_draw(self.selected, world, entities) else {
                    return Err("hold a bow or crossbow with an arrow, away from a block that takes use".into());
                };
                self.emit(Event::Shot(shot));
                if matches!(
                    shot.ranged,
                    crate::session::ranged::Ranged::Drawing | crate::session::ranged::Ranged::Charging { .. }
                ) {
                    self.input = MoveInput::default();
                    self.remaining = duration;
                    self.mining = false;
                    self.eating = false;
                    self.drawing = true;
                    self.session.cancel_consumption();
                    self.session.cancel_mining();
                } else {
                    // A charged crossbow fired at once.
                    self.cooldown = 0.22;
                    self.swings += 1;
                }
            }
            Command::Look(yaw, pitch) => {
                self.session.player.yaw = yaw.to_radians();
                self.session.player.pitch = pitch.to_radians();
            }
            Command::Select(slot) => {
                // Java keeps progress while the held stack is the same item and components.
                self.selected = slot;
                self.bite = 0;
                self.session.cancel_consumption();
                self.drawing = false;
                self.session.cancel_draw();
            }
            Command::Fly(on) => {
                if on && !self.session.mode.can_fly() {
                    return Err("flight requires Creative or Spectator mode".into());
                }
                self.session.player.flying = on;
                self.session.player.vel = DVec3::ZERO;
            }
            Command::Enchant(e, level) => {
                let slot = &mut self.session.inventory.slots[self.selected];
                *slot = Some(crate::enchant::command(*slot, e, level)?);
            }
            Command::Give(item, count) => {
                let mut inv = self.session.inventory.clone();
                if inv.add(item, count) > 0 {
                    return Err("inventory full".into());
                }
                self.session.inventory = inv;
            }
            Command::Clear => {
                self.session.inventory.take_all();
                self.session.work.fill(None);
            }
            Command::Kill => {
                if !self.session.vitals.is_dead() {
                    self.session.vitals.damage(f32::MAX, Cause::GENERIC_KILL, false);
                    self.drop_everything(entities);
                    self.remaining = 0;
                }
            }
            Command::SummonLightning(pos) => entities.summon_lightning(pos.resolve(self.session.player.pos)?, false),
            Command::Summon(kind, pos) => {
                entities.spawn(kind, pos.resolve(self.session.player.pos)?);
            }
            Command::Xp(change) => {
                if let Some(chime) = change.apply(&mut self.session.vitals.xp) {
                    self.emit(Event::Xp(Some(chime)));
                }
            }
            Command::Effect(change) => {
                let (damage, _) = change.apply(&mut self.session.vitals, "Player")?;
                self.damage(damage, Cause::MAGIC);
            }
            Command::Mode(mode) => self.set_mode(mode),
            Command::Teleport(pos, turn) => {
                command::teleport(&mut self.session.player, pos, turn)?;
                self.previous_pos = self.session.player.pos;
                self.session.player.vel = DVec3::ZERO;
                self.session.vitals.reset_fall();
            }
            Command::Rotate(turn) => command::rotate(&mut self.session.player, turn)?,
            Command::SpawnPoint(pos) => {
                let pos = pos.resolve(self.session.player.pos)?.floor().as_ivec3();
                self.session.spawn_bed = None;
                self.session.spawn_anchor = None;
                self.session.spawn_point = Some(pos);
            }
            Command::SetBlock(pos, block) => {
                if !world.set_block(pos, block) {
                    return Err("block is unchanged or unloaded".into());
                }
            }
            Command::Place
                if self.session.inventory.get(self.selected).is_some_and(|s| s.item.equip_piece().is_some())
                    && !self.session.block_takes_use(self.selected, world, self.session.player.sneaking) =>
            {
                if !crate::survival_items::equip_armor(
                    &self.session.player,
                    &mut self.session.inventory,
                    self.selected,
                    self.session.mode,
                    world,
                ) {
                    return Err("cannot equip held armor here".into());
                }
                self.swings += 1;
            }
            Command::Place => {
                use crate::session::interact::{UseContext, Used};
                if !self.session.mode.can_interact() {
                    return Err("this game mode cannot interact".into());
                }
                let held = self.session.inventory.get(self.selected).map(|s| s.item);
                // Pearls keep their own cooldown; splash potions, buckets and bottles have none.
                let bucket = held.is_some_and(crate::survival_items::bucket::handles);
                let cooled = bucket
                    || matches!(held, Some(Item::ENDER_PEARL))
                    || held.is_some_and(|i| i.as_splash_potion().is_some());
                if !cooled && self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let ctx = UseContext { sneaking: self.session.player.sneaking, others };
                let done = self.session.use_held(self.selected, world, entities, &ctx)?;
                let Some(done) = done else {
                    return Err(if bucket && !self.session.mode.can_build() && held != Some(Item::GLASS_BOTTLE) {
                        "this game mode cannot use buckets here"
                    } else if bucket {
                        "no bucket or bottle interaction within reach"
                    } else if self.session.player.vehicle.is_some() {
                        "cannot place while riding"
                    } else if !self.session.mode.can_build() {
                        "this game mode cannot place blocks"
                    } else {
                        "no block within reach"
                    }
                    .into());
                };
                match done.used {
                    Used::Placed(crate::session::placement::Outcome::Placed { cell, block, .. }) => {
                        self.emit(Event::Placed(cell, block));
                    }
                    Used::Bucket(effect) => self.emit(Event::Bucket(effect)),
                    Used::EyeInserted { cell, opened } => self.emit(Event::FrameFilled(cell, opened)),
                    Used::EyeReleased { from } => self.emit(Event::EyeThrown(from)),
                    // The agent protocol has no screens or beds: use its container,
                    // trade and sleep commands, or sneak to build against the block.
                    Used::Screen { .. } => return Err("that block opens a screen; use its command or sneak".into()),
                    Used::Bed { .. } => return Err("use the sleep command for beds".into()),
                    Used::EditSign { .. } => return Err("signs are written with: sign x y z front|back [lines]".into()),
                    Used::OpenVehicle(_) => return Err("that vehicle has storage; use chest take/put".into()),
                    Used::OpenMerchant(_) => return Err("use trade to see this villager's offers".into()),
                    Used::Utility { effect: crate::world::utilities::interact::Effect::EnderChest, .. } => {
                        return Err("that is your ender chest; use chest take/put".into());
                    }
                    _ => {}
                }
                if let Used::HornPlayed { instrument, from } = done.used {
                    self.emit(Event::Horn(instrument, from));
                }
                if !matches!(done.used, Used::PearlThrown { .. } | Used::PotionThrown { .. } | Used::Bucket(_)) {
                    self.cooldown = 0.22;
                }
                self.swings += 1;
            }
            Command::Attack => {
                if !self.session.mode.can_interact() {
                    return Err("this game mode cannot attack".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let sprinting = self.session.player.sprinting;
                let outcome = self.session.attack(self.selected, world, entities, sprinting)?;
                // Java has no attack cooldown; damage scales with the charge instead.
                self.cooldown = TICK_SECONDS;
                self.swings += 1;
                // Wind Burst's block interaction, as the desktop hands apply it.
                if let crate::session::combat::Outcome::Hit { smash: Some(smash), .. } = &outcome
                    && let Some(center) = smash.wind_burst
                {
                    crate::weapons::mace::wind_burst_blocks(world, center);
                }
                match outcome {
                    crate::session::combat::Outcome::Missed { .. } => {
                        return Err("nothing to attack within reach".into());
                    }
                    // Java's `causeExtraKnockback` stops the sprint until it is asked for again.
                    crate::session::combat::Outcome::Hit { sprint_cancelled: true, .. } => self.stop_sprint(),
                    _ => {}
                }
            }
            Command::Craft(item) => self.craft(item, world, entities)?,
            Command::Bundle(slot, pick) => self.bundle_take(slot, pick, entities)?,
            Command::Recipe(id, max) => self.craft_recipe(id, max, world, entities)?,
            Command::Furnace(take, slot) => self.furnace_transfer(take, slot, world, entities)?,
            Command::Chest(take, slot) => {
                let eye = self.session.player.eye();
                let dir = self.session.player.forward().as_dvec3();
                let reach = crate::entity::minecart::interaction_reach(world, eye, dir, self.session.entity_reach());
                let vehicle = self
                    .player
                    .vehicle
                    .filter(|id| entities.vehicle_slots(*id).is_some_and(|s| !s.is_empty()))
                    .or_else(|| entities.vehicle_container(eye, dir, reach));
                if let Some(id) = vehicle {
                    if !entities.vehicle_in_reach(id, eye, self.session.entity_screen_range()) {
                        return Err("vehicle out of reach".into());
                    }
                    if take {
                        let stack = entities
                            .vehicle_slots(id)
                            .and_then(|slots| slots.get(slot).copied().flatten())
                            .ok_or("vehicle slot empty or out of range")?;
                        let left = self.session.inventory.add_stack(stack);
                        entities.vehicle_slots_mut(id).unwrap()[slot] =
                            (left > 0).then_some(Stack { count: left, ..stack });
                        if left == stack.count {
                            return Err("inventory full".into());
                        }
                    } else {
                        let stack = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
                        if !entities.vehicle_accepts(id, slot, stack.item) {
                            return Err("item does not fit this mount slot".into());
                        }
                        let dest = &mut entities.vehicle_slots_mut(id).unwrap()[slot];
                        let room = match dest {
                            None => stack.max(),
                            Some(s) if s.stacks_with(&stack) => s.max() - s.count,
                            _ => 0,
                        };
                        let n = room.min(stack.count);
                        if n == 0 {
                            return Err("vehicle slot full or incompatible".into());
                        }
                        *dest = Some(Stack { count: dest.map_or(0, |s| s.count) + n, ..stack });
                        self.session.inventory.slots[self.selected] =
                            (stack.count > n).then_some(Stack { count: stack.count - n, ..stack });
                    }
                    return Ok(());
                }
                let (pos, _) = self.target(world).ok_or("no chest within reach")?;
                // An ender chest holds each player's own inventory
                // (`PlayerEnderChestContainer`); the table entry under an open
                // one is the host's, not this player's.
                let ender = world.lent_chest() == Some(pos)
                    || world.get_block(pos).is_some_and(|b| crate::world::utilities::ender_chest_of(b).is_some());
                if ender && !crate::world::utilities::ender_chest_opens(world, pos) {
                    return Err("the ender chest is blocked from above".into());
                }
                if !ender
                    && world.get_block(pos).and_then(crate::world::double_chest::state).is_some()
                    && !world.chest_opens(pos)
                {
                    return Err("the chest is blocked from above".into());
                }
                // v0.16 Track B: a shulker box needs room for its lid, and
                // takes no shulker box.
                if !ender && world.get_block(pos).is_some_and(|b| crate::world::shulker_box::owns(b.0)) {
                    if !world.shulker_box_opens(pos) {
                        return Err("the shulker box's lid is blocked".into());
                    }
                    if !take
                        && self.session.inventory.get(self.selected).is_some_and(|s| !world.container_accepts(pos, &s))
                    {
                        return Err("a shulker box can't hold a shulker box".into());
                    }
                }
                let slots = if ender { crate::world::chest::SLOTS } else { world.container_slots(pos) };
                if slot >= slots {
                    return Err("container slot out of range".into());
                }
                // v0.17 Track C1: `CrafterSlot.mayPlace`.
                if !take && !ender && world.crafter_slot_disabled(pos, slot) {
                    return Err("that crafter slot is disabled".into());
                }
                let (inventory, selected) = (&mut self.session.inventory, self.selected);
                if ender {
                    chest_transfer(&mut self.session.ender.slots[slot], inventory, selected, take)?;
                } else {
                    // Both halves of a double chest, as one container.
                    world
                        .with_chest_slots(pos, |slots| chest_transfer(&mut slots[slot], inventory, selected, take))
                        .ok_or("target is not a chest")??;
                }
                if world.get_block(pos).is_some_and(crate::entity::nether::guarded_by_piglins) {
                    entities.piglins_notice(self.session.id, self.session.player.pos, true, world);
                }
            }
            Command::Enchanting(i) => self.enchant_at_table(i, world)?,
            command @ (Command::Anvil(_) | Command::AnvilName(_)) => {
                let (pos, _) = self.target(world).ok_or("no anvil within reach")?;
                if !world.get_block(pos).is_some_and(Block::is_anvil) {
                    return Err("target is not an anvil".into());
                }
                let (slot, name) = match &command {
                    Command::Anvil(slot) => (Some(*slot), None),
                    Command::AnvilName(name) => (None, Some(name.as_str())),
                    _ => unreachable!(),
                };
                if slot == Some(self.selected) {
                    return Err("pick a different slot for the second item".into());
                }
                let left = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
                let right = slot.and_then(|slot| self.session.inventory.get(slot));
                let r = crate::enchant::anvil_menu(left, right, name, self.creative).ok_or("those don't combine")?;
                if !self.creative && r.cost >= crate::enchant::TOO_EXPENSIVE {
                    return Err("too expensive".into());
                }
                if !self.creative && self.session.vitals.xp.level < r.cost {
                    return Err(format!("needs level {}", r.cost));
                }
                self.session.inventory.slots[self.selected] = Some(r.output);
                if let Some(slot) = slot {
                    self.session.inventory.slots[slot] = match (r.uses, right) {
                        (Some(n), Some(s)) if s.count > n => Some(Stack { count: s.count - n, ..s }),
                        _ => None,
                    };
                }
                if !self.creative {
                    self.session.vitals.xp.add_levels(-(r.cost as i64));
                    crate::enchant::wear_anvil(world, pos);
                }
            }
            Command::Trade(index) => {
                let id = entities
                    .target_merchant(
                        world,
                        self.session.player.eye(),
                        self.session.player.forward().as_dvec3(),
                        self.session.entity_reach(),
                    )
                    .ok_or("no villager within reach")?;
                entities.prepare_merchant_maps(world, id);
                if let Some(i) = index {
                    let xp = entities.trade_for(id, i, &mut self.session.inventory, self.session.id)?;
                    entities.spawn_xp(self.session.player.pos, xp);
                }
            }
            Command::Grindstone(slot) => {
                let (pos, _) = self.target(world).ok_or("no grindstone within reach")?;
                if world.get_block(pos).is_none_or(|b| b.base() != Block::GRINDSTONE) {
                    return Err("target is not a grindstone".into());
                }
                if slot == Some(self.selected) {
                    return Err("pick a different second slot".into());
                }
                let a = self.session.inventory.get(self.selected);
                let b = slot.and_then(|i| self.session.inventory.get(i));
                let result = crate::grindstone::result(a, b).ok_or("those don't grind")?;
                let xp = crate::grindstone::xp(a, b, crate::enchant::roll());
                self.session.inventory.slots[self.selected] = Some(result.output);
                if let Some(i) = slot {
                    self.session.inventory.slots[i] = None;
                }
                entities.spawn_xp(pos.as_dvec3() + DVec3::splat(0.5), xp);
            }
            Command::Smithing => {
                let (pos, _) = self.target(world).ok_or("no smithing table within reach")?;
                if world.get_block(pos) != Some(Block::SMITHING_TABLE) {
                    return Err("target is not a smithing table".into());
                }
                let base = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
                // v0.15 Track D: an armor trim template and a trim material
                // work too; the upgrade template is tried first.
                let (inv, selected) = (&self.session.inventory, self.selected);
                let slots = |want: fn(Item) -> bool| {
                    (0..inv.slots.len()).filter(move |&i| i != selected && inv.get(i).is_some_and(|s| want(s.item)))
                };
                let found = slots(crate::smithing::is_template)
                    .flat_map(|t| slots(crate::smithing::is_addition).map(move |a| (t, a)))
                    .find_map(|(t, a)| {
                        let (template, addition) = (inv.get(t).unwrap(), inv.get(a).unwrap());
                        crate::smithing::upgrade(template, base, addition).map(|r| (t, a, r))
                    });
                if !inv.slots.iter().flatten().any(|s| crate::smithing::is_template(s.item)) {
                    return Err("no smithing template".into());
                }
                let (template_slot, addition_slot, result) =
                    found.ok_or("no template and addition in the inventory fit the selected item")?;
                self.session.inventory.take_one(template_slot);
                self.session.inventory.take_one(addition_slot);
                self.session.inventory.slots[self.selected] = Some(result);
            }
            Command::Sign(pos, front, lines) => self.session.edit_sign(world, pos, front, lines)?,
            Command::Stonecut(output) => self.stonecut(output, world)?,
            Command::Loom(pattern, dye) => self.loom(&pattern, dye, world)?,
            Command::Cartography(action) => self.cartography(action, world)?,
            Command::Beacon(primary, secondary) => self.beacon(primary, secondary, world)?,
            Command::CrafterToggle(slot) => {
                let pos = self.target(world).map(|(p, _)| p).filter(|&p| world.crafter(p).is_some());
                let pos = pos.ok_or("no crafter within reach")?;
                if world.chest_slot(pos, slot).is_some() {
                    return Err("only an empty crafter slot can be turned off or on".into());
                }
                let enabled = world.crafter_slot_disabled(pos, slot);
                world.set_crafter_slot_enabled(pos, slot, enabled);
            }
            Command::Book { pages, title, offhand } => {
                let slot = if offhand { crate::session::books::OFFHAND_SLOT } else { self.selected };
                match title {
                    Some(title) => self.session.sign_book(slot, &pages, &title, "Player")?,
                    None => self.session.edit_book(slot, &pages)?,
                }
            }
            Command::Lectern(pos, Some(page)) => {
                self.session.turn_lectern_page(world, pos, page)?;
            }
            Command::Lectern(pos, None) => {
                if let Some(rest) = self.session.take_lectern_book(world, pos)? {
                    entities.throw(rest, self.session.player.eye(), self.session.player.forward().as_dvec3());
                }
            }
            Command::Drop => {
                let stack = self.session.inventory.slots[self.selected].take().ok_or("selected slot empty")?;
                entities.throw(stack, self.session.player.eye(), self.session.player.forward().as_dvec3());
            }
            Command::Respawn => {
                if !self.session.vitals.is_dead() {
                    return Err("player is alive".into());
                }
                let personal = self.session.personal_spawn(world);
                let at = personal
                    .position
                    .unwrap_or_else(|| world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                self.session.respawn(at, personal.yaw, self.keep_inventory);
            }
            Command::Observe(_) | Command::Help | Command::XpQuery | Command::Say(_) | Command::Recipes(..) => {}
            _ => return Err("command requires host console".into()),
        }
        Ok(())
    }

    fn craft(&mut self, item: Item, world: &World, entities: &mut Entities) -> Result<(), String> {
        let at_table = self.at_crafting_table(world);
        let (inventory, remainders, _, grid) = crafting::recipes()
            .iter()
            .filter(|r| r.result.item == item)
            .find_map(|r| self.crafted(r, at_table))
            .ok_or("missing ingredients, inventory space, targeted crafting table or unlocked recipe")?;
        self.session.inventory = inventory;
        // v0.18 Track 5: taking the result teaches the recipe.
        self.session.recipe_book.crafted(&grid);
        for remainder in remainders {
            entities.throw(remainder, self.session.player.eye(), self.session.player.forward().as_dvec3());
        }
        Ok(())
    }

    /// The targeted block, when its base is `base`.
    fn targeted(&self, world: &World, base: Block, what: &str) -> Result<IVec3, String> {
        let (pos, _) = self.target(world).ok_or_else(|| format!("no {what} within reach"))?;
        if world.get_block(pos).is_none_or(|b| b.base() != base) {
            return Err(format!("target is not a {what}"));
        }
        Ok(pos)
    }

    /// Uses up one item from each of `inputs` and adds `result`, all or
    /// nothing: refused when the result wouldn't fit.
    fn exchange(&mut self, inputs: &[usize], result: Stack) -> Result<(), String> {
        let mut inventory = self.session.inventory.clone();
        for &i in inputs {
            inventory.take_one(i);
        }
        if inventory.add_stack(result) > 0 {
            return Err("no room for the result".into());
        }
        self.session.inventory = inventory;
        Ok(())
    }

    /// v0.15 Track B `stonecut <item>`: one held stack cut into `output`.
    fn stonecut(&mut self, output: Item, world: &World) -> Result<(), String> {
        self.targeted(world, Block::STONECUTTER, "stonecutter")?;
        let input = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
        let &(item, count) = crate::crafting::stonecutting::menu(input.item)
            .iter()
            .find(|(o, _)| *o == output)
            .ok_or("the stonecutter can't make that from the held item")?;
        self.exchange(&[self.selected], Stack::new(item, count))
    }

    /// `loom <pattern> <dye>`: the held banner gains a layer.
    fn loom(&mut self, pattern: &str, dye: crate::color::DyeColor, world: &World) -> Result<(), String> {
        use crate::content::banner_patterns as bp;
        self.targeted(world, Block::LOOM, "loom")?;
        let banner = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
        bp::banner_color(banner.item).ok_or("the held item is not a banner")?;
        let pattern = crate::inventory::components::BannerPattern::from_name(pattern).ok_or("unknown pattern")?;
        let pattern_item = bp::every_item().find(|&i| bp::provides(i) == Some(pattern));
        if let Some(item) = pattern_item {
            self.session.inventory.find(item).ok_or("that pattern needs its banner pattern item")?;
        } else if !bp::selectable(None).contains(&pattern) {
            return Err("the loom can't weave that pattern".into());
        }
        if bp::patterns(&banner).layers.len() >= bp::MAX_LAYERS {
            return Err("the banner already has six layers".into());
        }
        let dye_slot = self.session.inventory.find(dye.dye()).ok_or("no dye of that colour")?;
        let result = bp::loom_result(banner, Stack::new(dye.dye(), 1), pattern).ok_or("that can't be woven")?;
        self.exchange(&[dye_slot, self.selected], result)
    }

    /// `cartography copy|zoom|lock`: the held map at the targeted table.
    fn cartography(&mut self, action: CartographyAction, world: &mut World) -> Result<(), String> {
        self.targeted(world, Block::CARTOGRAPHY_TABLE, "cartography table")?;
        let map = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
        let extra = match action {
            CartographyAction::Copy => Item::MAP,
            CartographyAction::Zoom => Item::PAPER,
            CartographyAction::Lock => Item::from_block(Block::GLASS_PANE),
        };
        let extra_slot = self.session.inventory.find(extra).ok_or_else(|| format!("no {}", extra.name()))?;
        let result = world
            .maps
            .cartography_result(Some(map), Some(Stack::new(extra, 1)))
            .ok_or("the table can't do that to the held map")?;
        let mut check = self.session.inventory.clone();
        check.take_one(extra_slot);
        check.take_one(self.selected);
        if check.add_stack(result) > 0 {
            return Err("no room for the result".into());
        }
        let mut made = result;
        world.maps.post_process(&mut made);
        self.exchange(&[extra_slot, self.selected], made)
    }

    /// v0.16 Track C `beacon <primary> [secondary]`: the beacon screen's
    /// Done (`BeaconMenu.updateEffects`) with the first payment item in the
    /// inventory, refused when the pyramid doesn't allow the powers.
    fn beacon(
        &mut self,
        primary: crate::simulation::effects::Effect,
        secondary: Option<crate::simulation::effects::Effect>,
        world: &mut World,
    ) -> Result<(), String> {
        let pos = self.targeted(world, crate::world::beacon::BEACON, "beacon")?;
        let slot = (0..crate::inventory::SLOTS)
            .find(|&i| self.session.inventory.get(i).is_some_and(|s| crate::world::beacon::is_payment(s.item)))
            .ok_or("no iron ingot, gold ingot, emerald, diamond or netherite ingot to pay with")?;
        if !world.set_beacon_powers(pos, Some(primary), secondary) {
            return Err("the beacon's pyramid doesn't allow those powers".into());
        }
        self.session.inventory.take_one(slot);
        Ok(())
    }

    fn at_crafting_table(&self, world: &World) -> bool {
        self.target(world).is_some_and(|(p, _)| world.get_block(p).is_some_and(|b| b.base() == Block::CRAFTING_TABLE))
    }

    /// The inventory after crafting `recipe` once, if the ingredients and
    /// room are there (3x3 recipes need a targeted crafting table).
    fn crafted(
        &self,
        recipe: &crafting::Recipe,
        at_table: bool,
    ) -> Option<(Inventory, Vec<Stack>, Stack, crafting::Grid)> {
        let mut grid = recipe.preview();
        if grid.size == 3 && !at_table {
            return None;
        }
        let mut inventory = self.session.inventory.clone();
        let mut remainders = Vec::new();
        for i in 0..grid.cells.len() {
            if grid.cells[i].is_none() {
                continue;
            }
            let options = recipe.alternatives(i).unwrap();
            let slot = inventory.slots.iter().position(|s| s.is_some_and(|s| options.contains(&s.item)))?;
            let used = inventory.get(slot)?;
            grid.cells[i] = Some(Stack { count: 1, ..used });
            inventory.take_one(slot);
            if let Some(rest) = used.item.remainder() {
                remainders.push(Stack::new(rest, 1));
            }
        }
        // v0.18 Track 5: `limited_crafting` hides recipes the player doesn't know.
        if !self.session.recipe_book.allows(&grid, self.limited_crafting) {
            return None;
        }
        let result = grid.result()?;
        if inventory.add_stack(result) != 0 {
            return None;
        }
        remainders.retain_mut(|stack| {
            stack.count = inventory.add_stack(*stack);
            stack.count != 0
        });
        Some((inventory, remainders, result, grid))
    }

    /// v0.18 Track 5 `recipe <id> [max]`: the recipe book's click on a
    /// known recipe. Java's placement fills a grid (the 2x2, or the
    /// targeted crafting table's 3x3) from the inventory, one set or every
    /// set with `max` (shift), and the results are taken through the shared
    /// grid as a shift-click on the result would; what's left in the grid
    /// goes back to the inventory.
    fn craft_recipe(
        &mut self,
        id: crafting::book::RecipeId,
        max: bool,
        world: &World,
        entities: &mut Entities,
    ) -> Result<(), String> {
        use crafting::book::place::{self, Outcome};
        let recipe = id.recipe().ok_or("VoxelCraft can't make that recipe yet")?;
        if !self.session.recipe_book.knows(id) {
            return Err("that recipe isn't unlocked".into());
        }
        if !recipe.kind.is_crafting() {
            return Err("only crafting recipes; furnaces take their input with furnace put".into());
        }
        let size = if self.at_crafting_table(world) { 3 } else { 2 };
        if !recipe.fits(size) {
            return Err("that recipe needs a targeted crafting table".into());
        }
        let mut grid = crafting::Grid::new(size);
        let creative = self.session.mode.is_creative();
        match place::place_in_grid(recipe, &mut grid, &mut self.session.inventory, self.selected, max, creative) {
            Outcome::Filled => {}
            Outcome::Ghost => return Err("missing ingredients".into()),
            Outcome::Unchanged => return Err("no room to place the recipe".into()),
        }
        let mut made = 0;
        while self.session.recipe_book.allows(&grid, self.limited_crafting)
            && let Some(result) = grid.result()
        {
            let mut inventory = self.session.inventory.clone();
            if inventory.add_stack(result) != 0 {
                break;
            }
            self.session.inventory = inventory;
            self.session.recipe_book.crafted(&grid);
            for rest in grid.consume_with_remainders() {
                let left = self.session.inventory.add_stack(rest);
                if left > 0 {
                    let (eye, forward) = (self.session.player.eye(), self.session.player.forward().as_dvec3());
                    entities.throw(Stack { count: left, ..rest }, eye, forward);
                }
            }
            made += 1;
            if !max {
                break;
            }
        }
        for stack in grid.take_all() {
            place::place_back(&mut self.session.inventory, stack, self.selected);
        }
        // What doesn't fit drops, as Java's `placeItemBackInInventory` does.
        let (eye, forward) = (self.session.player.eye(), self.session.player.forward().as_dvec3());
        for stack in self.session.inventory.take_spill() {
            entities.throw(stack, eye, forward);
        }
        if made == 0 { Err("no room for the result".into()) } else { Ok(()) }
    }

    /// v0.18 Track 5 `recipes [all|craftable] [query]`: the known recipes the
    /// book would show for the 2x2 grid (or the targeted crafting table's),
    /// as the search tab lists them, with whether each can be made now and
    /// whether it is new. Craftable only with `craftable`, or when the
    /// book's crafting filter is on and neither is given.
    pub fn recipe_list(&self, world: &World, filter: Option<bool>, query: &str) -> Value {
        use crafting::book::{self, BookType, stacked::Stacked};
        let size = if self.at_crafting_table(world) { 3 } else { 2 };
        let mut book = self.session.recipe_book.clone();
        if let Some(filtering) = filter {
            book.set_filtering(BookType::Crafting, filtering);
        }
        let available = Stacked::of_inventory(&self.session.inventory);
        let page = book::page(&book, BookType::Crafting, None, Some(size), query, &available);
        let filtering = book.settings(BookType::Crafting).filtering;
        let entries: Vec<Value> = page
            .iter()
            .flat_map(|shown| shown.entries(filtering).iter().map(move |&id| (id, shown.craftable.contains(&id))))
            .map(|(id, craftable)| {
                let r = id.recipe().unwrap();
                json!({"id":format!("minecraft:{}", id.name()),"result":r.result.item.name(),"count":r.result.count,
                    "category":r.category.id(),"craftable":craftable,"new":self.session.recipe_book.highlighted(id)})
            })
            .collect();
        json!({"grid":size,"filtering":filtering,"known":self.session.recipe_book.known_count(),"recipes":entries})
    }

    /// v0.18 Track 5: the recipe book's search tab as one list, for the
    /// controller's crafting tab: each recipe the book shows for this
    /// player's grid (the 2x2, or a targeted table's 3x3), with its result
    /// and whether it can be made now. Craftable only while the crafting
    /// book filters.
    pub fn book_entries(&self, world: &World) -> Vec<(crafting::book::RecipeId, Stack, bool)> {
        use crafting::book::{self, BookType, stacked::Stacked};
        let size = if self.at_crafting_table(world) { 3 } else { 2 };
        let book = &self.session.recipe_book;
        let available = Stacked::of_inventory(&self.session.inventory);
        let filtering = book.settings(BookType::Crafting).filtering;
        book::page(book, BookType::Crafting, None, Some(size), "", &available)
            .iter()
            .flat_map(|shown| {
                shown
                    .entries(filtering)
                    .iter()
                    .map(|&id| (id, id.recipe().unwrap().result, shown.craftable.contains(&id)))
            })
            .collect()
    }

    /// v0.18 Track 5: what `observe` reports of the recipe book.
    fn recipe_book_state(&self) -> Value {
        use crafting::book::BookType;
        let book = &self.session.recipe_book;
        let settings: serde_json::Map<String, Value> = BookType::ALL
            .into_iter()
            .flat_map(|b| {
                let (open, filtering) = b.settings_keys();
                let s = book.settings(b);
                [(open.to_string(), json!(s.open)), (filtering.to_string(), json!(s.filtering))]
            })
            .collect();
        json!({"known":book.known_count(),"new":book.highlighted_count(),"settings":settings})
    }

    /// Every result that `craft` would make right now, once each, in recipe
    /// book order.
    pub fn craftable(&self, world: &World) -> Vec<Stack> {
        let at_table = self.at_crafting_table(world);
        let mut out: Vec<Stack> = Vec::new();
        for recipe in crafting::recipes() {
            if !out.iter().any(|s| s.item == recipe.result.item)
                && let Some((_, _, result, _)) = self.crafted(recipe, at_table)
            {
                out.push(result);
            }
        }
        out
    }

    /// Advance physics/survival/mining once. All sessions tick before the shared world systems.
    pub fn tick(&mut self, world: &mut World, entities: &mut Entities) {
        self.tick_difficulty(world, entities, crate::simulation::difficulty::Difficulty::Normal);
    }

    /// [`Agent::tick`] using the host world's difficulty.
    pub fn tick_difficulty(
        &mut self,
        world: &mut World,
        entities: &mut Entities,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) {
        self.tick_rules(world, entities, difficulty, &crate::rules::GameRules::default());
    }

    /// [`Agent::tick_difficulty`] with natural regeneration toggled.
    pub fn tick_rules(
        &mut self,
        world: &mut World,
        entities: &mut Entities,
        difficulty: crate::simulation::difficulty::Difficulty,
        rules: &crate::rules::GameRules,
    ) {
        self.session.settle_wake(world);
        self.session.tick_lodestones(world);
        self.session.tick_recipe_book(world);
        for stack in [self.session.inventory.get(self.selected), self.session.inventory.offhand].into_iter().flatten() {
            world.tick_map(stack, self.session.player.pos, self.session.id.0 as u64);
        }
        self.previous_pos = self.session.player.pos;
        self.keep_inventory = rules.bool("keep_inventory");
        self.pearls_vanish = rules.bool("ender_pearls_vanish_on_death");
        // The neutral-mob rules, as the host's mob update hands them over.
        entities.neutral_forgive_dead_players = rules.bool("forgive_dead_players");
        entities.neutral_universal_anger = rules.bool("universal_anger");
        self.immunity = rules.damage_immunity();
        self.limited_crafting = rules.bool("limited_crafting");
        // Movement alone returning early is not enough: survival, mining,
        // pickups and timed commands must also wait for local terrain.
        let feet = self.session.player.pos.floor().as_ivec3();
        if !world.is_loaded(feet) || !world.is_loaded(feet - IVec3::Y) {
            return;
        }
        // Every agent (CLI, controller and LAN hands) charges here once per tick.
        self.session.tick_attack_strength(self.selected);
        if let Some(shot) = self.session.tick_weapons(entities) {
            self.emit(Event::Shot(shot));
        }
        // Java keeps the hook while a rod is in either hand.
        let holding_rod = [self.session.inventory.get(self.selected), self.session.inventory.offhand]
            .iter()
            .any(|s| s.is_some_and(|s| s.item == Item::FISHING_ROD));
        if self.session.vitals.is_dead() || !holding_rod {
            entities.drop_bobber(self.session.id);
        }
        self.cooldown = (self.cooldown - TICK_SECONDS).max(0.0);
        if self.session.vitals.is_dead() {
            // Effects count down while dead, as on the desktop (`Vitals::tick_rules`).
            let _ = self.session.vitals.effects.tick(TICK_SECONDS as f32);
            self.session.cancel_consumption();
            self.session.cancel_mining();
            entities.dismount(world, self.session.id);
            self.session.player.vehicle = None;
            self.remaining = 0;
            self.session.sleeping = None;
            return;
        }
        let mut input = self.movement_input();
        input.sprint_blocked = !self.session.vitals.may_sprint(self.creative || self.session.mode.invulnerable());
        input.using_item = self.bite > 0 || self.session.using_item();
        let before = self.session.player.pos;
        if self.session.player.vehicle.is_some()
            && input.descend
            && let Some(pos) = entities.dismount(world, self.session.id)
        {
            self.session.player.pos = pos;
            self.session.player.vehicle = None;
            self.session.player.vel = DVec3::ZERO;
        }
        // `LivingEntity.canFreeze` reads the current armour, riding or not.
        self.session.player.powder_snow_equipment(&self.session.inventory.armor, self.session.vitals.ticks_frozen);
        if self.session.player.vehicle.is_none() {
            self.session.player.apply_effects(&self.session.vitals.effects);
            self.session.player.wear_boots(crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::DepthStrider,
            ));
            self.session.player.tick(input, world);
            world.powder_snow_fire(&self.session.player, self.session.vitals.burning());
            world.dripleaf_step(self.session.player.pos, self.session.player.on_ground);
            crate::particles::movement_effects(&self.session.player, before, world);
            world.player_moved(
                self.session.id,
                &self.session.player,
                before,
                self.session.mode == crate::rules::GameMode::Spectator,
                self.creative || self.session.mode.is_creative(),
            );
        } else {
            entities.mount_input(self.session.id, &self.session.player, input, &self.session.inventory, self.selected);
            entities.seat_player(&mut self.session.player, self.session.id);
            self.session.player.refresh_riding_contact(world);
        }
        // Damage, death and armor changes as game events.
        world.player_vitals(
            self.session.id,
            self.session.player.pos,
            self.session.vitals.health,
            self.session.inventory.armor.map(|s| s.map(|s| s.item)),
            self.session.event_source(),
        );
        world.player_totems(
            self.session.id,
            self.session.player.pos,
            self.session.vitals.totems_used,
            self.session.event_source(),
        );
        let moved = (self.session.player.pos - before).with_y(0.0).length();
        let env = simulation::survival::Env {
            respiration: crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::Respiration,
            ),
            frost_walker: crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::FrostWalker,
            ) > 0,
            turtle_helmet: self.session.inventory.armor[0].is_some_and(|s| s.item == Item::TURTLE_HELMET),
            stalactite: std::mem::take(&mut self.session.stalactite),
            ..simulation::player_environment(&self.session.player, world, moved)
        };
        let burning = self.session.vitals.burning();
        let was_alive = !self.session.vitals.is_dead();
        let hurts = self.session.vitals.tick_rules(
            TICK_SECONDS as f32,
            &env,
            self.creative || self.session.mode.invulnerable(),
            difficulty,
            rules.bool("natural_health_regeneration"),
        );
        if self.session.sleeping.is_some() {
            self.session.vitals.time_since_rest = 0;
        }
        // v1.0 Track 4: `FarmlandBlock.fallOn`, as the host's `Game::trample`.
        let source = self.session.event_source();
        world.player_landed(self.session.player.pos, self.session.player.collision_shape(), hurts.landed, source);
        // Wither damage lands inside the rule tick, not through `damage`.
        if was_alive && self.session.vitals.is_dead() {
            self.session.totem_saves(self.selected);
        }
        if burning {
            let top = self.session.player.collision_shape().aabb(self.session.player.pos).1.y;
            world.douse_in_cauldron(self.session.player.pos, top);
        }
        // In the host's order (`Game::tick`); armor takes its share of the
        // types outside `#bypasses_armor` (lava, fire, magma, campfires and
        // stalactites), as `damage_player_armored` does for the host.
        for (damage, cause) in [
            (hurts.freeze, Cause::FREEZE),
            (hurts.fall, Cause::FALL_HARD),
            (hurts.drown, Cause::DROWN),
            (hurts.lava, Cause::LAVA),
            (hurts.stalactite, Cause::FALLING_STALACTITE),
            (hurts.stalagmite, Cause::STALAGMITE),
            (hurts.kinetic, Cause::FLY_INTO_WALL),
            (hurts.fire, Cause::IN_FIRE),
            (hurts.hot_floor, Cause::HOT_FLOOR),
            (hurts.campfire, Cause::CAMPFIRE),
            (hurts.burn, Cause::ON_FIRE),
            (hurts.starve, Cause::STARVE),
        ] {
            if damage > 0.0 && self.hurt(damage, cause, DVec3::ZERO, entities) > 0.0 {
                self.session.sleeping = None;
            }
        }
        // v0.15 Track D: silverfish from hits taken under Infested, and
        // Frost Walker's ice.
        self.session.settle_effect_hooks(entities);
        self.session.frost_walk(world, entities);
        self.session.tick_soul_speed(world);
        self.session.tick_elytra();
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
            return;
        }
        if self.remaining == 0 && self.mining {
            self.session.cancel_mining();
        }
        // The session owns Java's destroy delay; no adapter repeat cooldown applies.
        if self.remaining > 0 && self.mining {
            match self.session.tick_mining(self.selected, world, entities, rules) {
                Ok(crate::session::mining::Outcome::Broken { cell, block, .. }) => {
                    self.swings += 1;
                    self.emit(Event::Broke(cell, block));
                }
                Ok(_) => self.swings += 1,
                Err(_) => {}
            }
        }
        self.draw(world, entities);
        self.session.tick_blocking_unless_ticked(self.selected, self.eating && self.remaining > 0);
        self.session.tick_tooting(self.selected, self.eating && self.remaining > 0);
        if self.session.bundle_in_use() {
            let using = self.eating && self.remaining > 0;
            if let Some(out) = self.session.tick_bundle_use(self.selected, using, entities) {
                self.swings += 1;
                self.emit(Event::BundleThrown(out));
            }
        } else if self.session.brushing() && self.eating && self.remaining > 0 {
            if let Some(stroke) = self.session.tick_brushing(self.selected, true, world) {
                self.emit(Event::Brushed(stroke));
            }
        } else {
            self.chew(world, entities);
        }
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
            self.session.cancel_consumption();
            self.remaining = 0;
            return;
        }
        self.remaining = self.remaining.saturating_sub(1);
        if self.eating && self.remaining == 0 {
            self.session.cancel_brushing();
        }
        entities.items.retain_mut(|item| {
            if item.pickup_delay > 0.0 || !item.touches_player(self.session.player.pos) {
                return true;
            }
            item.stack.count = self.session.inventory.add_stack(item.stack);
            item.stack.count > 0
        });
        let (id, feet, shape) = (self.session.id, self.session.player.pos, self.session.player.collision_shape());
        let creative = self.session.mode.is_creative();
        let inventory = &mut self.session.inventory;
        entities.collect_tridents(id, feet, shape, creative, |stack| inventory.add_stack(stack) == 0);
        for stack in entities.collect_arrow_stacks(self.session.player.pos) {
            let left = self.session.inventory.add_stack(stack);
            if left > 0 {
                let mut stack = stack;
                stack.count = left;
                entities.scatter(stack, self.session.player.pos);
            }
        }
        if let Some(chime) = crate::entity::orb::absorb(
            &mut entities.orbs,
            self.session.player.pos,
            &mut self.session.vitals.xp,
            &mut self.session.inventory,
            self.selected,
        ) {
            self.emit(Event::Xp(chime));
        }
    }

    /// Arrives where this agent's ender pearl landed, taking the landing's
    /// 5 damage. Returns whether it teleported (dead or asleep agents don't).
    pub fn pearl_teleport(&mut self, pos: DVec3, entities: &mut Entities) -> bool {
        if self.session.vitals.is_dead() || self.session.sleeping.is_some() {
            return false;
        }
        self.session.player.pos = pos;
        self.previous_pos = pos;
        self.session.player.vel = DVec3::ZERO;
        self.session.vitals.reset_fall();
        self.damage(crate::entity::pearl::DAMAGE, Cause::ENDER_PEARL);
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
        }
        true
    }

    /// The shared death step: the inventory and some experience spill where
    /// the agent died, unless `keepInventory` is on or it is a spectator.
    fn drop_everything(&mut self, entities: &mut Entities) {
        self.session.die(entities, self.keep_inventory, Vec::new());
        // `ThrownEnderpearl.tick` sees the dead owner before any respawn.
        if self.pearls_vanish {
            let id = self.session.id;
            entities.discard_pearls_of_dead(|owner| owner == id);
        }
    }

    /// Advance the shared consumption clock; item-specific duration comes from
    /// the reference content definition. Switching slots restarts the bite.
    fn chew(&mut self, world: &World, entities: &mut Entities) {
        // Controller/LAN hands drive their session's consumption through the desktop
        // adapter; an idle CLI input must not cancel that other adapter's current tick.
        if !self.eating || self.remaining == 0 {
            self.bite = 0;
            return;
        }
        let result = self.session.tick_consumption_either(self.selected, true, entities);
        let Ok(step) = result else {
            self.bite = 0;
            self.session.cancel_consumption();
            // Held use with nothing to eat raises a shield, as a right-click does.
            self.session.start_blocking(self.selected);
            return;
        };
        self.bite = (f64::from(step.progress) * EAT_TICKS as f64) as u32;
        // v1.0 gap Track 2h: Java's rhythm (`shouldEmitParticlesAndSounds`).
        if step.effects {
            let held = self.session.consumable_in_hands(self.selected);
            self.emit(if held == Some(Item::HONEY_BOTTLE) { Event::HoneyDrink } else { Event::Chew });
            if let Some(item) = held.filter(|i| !i.is_drink()) {
                self.emit(Event::EatParticles(item, 5));
            }
        }
        if let Some(consumed) = step.completed {
            if !consumed.item.is_drink() {
                self.emit(Event::EatParticles(consumed.item, 16));
            }
            if consumed.item == crate::content::chorus::CHORUS_FRUIT {
                self.session.chorus_teleport(world, entities);
            }
            self.damage(consumed.magic_damage, Cause::MAGIC);
            if self.remaining <= 1 {
                self.eating = false;
            }
        }
    }

    /// One tick of holding use with a bow or crossbow: a crossbow loads when
    /// its charge completes; a bow shoots when the requested ticks have passed.
    fn draw(&mut self, world: &World, entities: &mut Entities) {
        if !self.drawing {
            return;
        }
        if self.remaining == 0 || self.session.vitals.is_dead() {
            self.drawing = false;
            self.session.cancel_draw();
            return;
        }
        if let Some(shot) = self.session.tick_draw(self.selected, true, world, entities) {
            self.emit(Event::Shot(shot));
        }
        if !self.session.drawing() {
            // Loaded, or the draw ended (the slot changed, the arrows ran out).
            self.drawing = false;
        } else if self.remaining <= 1 {
            self.drawing = false;
            if let Some(shot) = self.session.release_draw(self.selected, world, entities) {
                self.emit(Event::Shot(shot));
                self.swings += 1;
            }
        }
    }

    fn emit(&mut self, event: Event) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        }
    }

    /// Fraction of the current bite chewed, for the HUD.
    pub fn eating(&self) -> f32 {
        self.bite as f32 / EAT_TICKS as f32
    }

    /// Shows a remote player's eating progress (0..1) on this copy (LAN clients).
    pub fn show_eating(&mut self, fraction: f32) {
        self.bite = (fraction.clamp(0.0, 1.0) * EAT_TICKS as f32) as _;
    }

    /// The targeted enchanting table and its offers for the held item.
    fn table_offers(&self, world: &World) -> Option<(IVec3, [crate::enchant::Offer; 3])> {
        let (pos, _) = self.target(world)?;
        let held = self.session.inventory.get(self.selected)?;
        (world.get_block(pos) == Some(Block::ENCHANTING_TABLE) && crate::enchant::table_accepts(held)).then(|| {
            (
                pos,
                crate::enchant::offers(self.session.vitals.xp.seed, held.item, crate::enchant::bookshelves(world, pos)),
            )
        })
    }

    /// Java's enchanting table for an agent: one of the held item gets
    /// offer `i`, for `i + 1` levels and lapis (none in creative).
    fn enchant_at_table(&mut self, i: usize, world: &World) -> Result<(), String> {
        let (_, offers) =
            self.table_offers(world).ok_or("not aiming at an enchanting table with an enchantable item")?;
        let offer = offers[i];
        let held = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
        let lapis = self
            .inventory
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == Item::LAPIS_LAZULI)
            .map(|s| s.count as usize)
            .sum::<usize>();
        let level = self.session.vitals.xp.level;
        if offer.cost == 0 {
            return Err("no offer in that slot".into());
        }
        if !self.creative && (lapis <= i || level < offer.cost || level <= i as u32) {
            return Err(format!("needs {} lapis and level {}", i + 1, offer.cost));
        }
        let mut out = Stack { count: 1, ..held };
        if out.item == Item::BOOK {
            out.item = Item::ENCHANTED_BOOK;
        }
        for (e, l) in crate::enchant::offer_enchants(self.session.vitals.xp.seed, held.item, i, offer.cost) {
            out.enchants.set(e, l);
        }
        let mut inv = self.session.inventory.clone();
        inv.take_one(self.selected);
        if !self.creative {
            for _ in 0..=i {
                let slot = inv.find(Item::LAPIS_LAZULI).ok_or("no lapis")?;
                inv.take_one(slot);
            }
        }
        if inv.slots[self.selected].is_none() {
            inv.slots[self.selected] = Some(out);
        } else if inv.add_stack(out) > 0 {
            return Err("inventory full".into());
        }
        // Java takes the levels in every game mode (`onEnchantmentPerformed`).
        self.session.vitals.xp.add_levels(-(i as i64 + 1));
        self.session.inventory = inv;
        self.session.vitals.xp.seed = crate::enchant::new_seed();
        Ok(())
    }

    /// Damage through protection enchantments (armor points aside).
    fn damage(&mut self, amount: f32, cause: impl Into<Cause>) -> f32 {
        self.damage_through(amount, cause.into(), false).1
    }

    /// [`Agent::damage`], through armor points too when `armored`: the hurt
    /// cooldown sees `amount` before armor and enchantments, as Java's
    /// `hurtServer` does (v1.0 gap Track 8d). Returns what armor wears by
    /// and the damage taken.
    fn damage_through(&mut self, amount: f32, cause: Cause, armored: bool) -> (f32, f32) {
        if self.immunity.blocks(cause) {
            return (0.0, 0.0);
        }
        let invulnerable = self.creative || self.session.mode.invulnerable();
        let inv = &self.session.inventory;
        let (points, toughness) = (inv.armor_points(), inv.armor_toughness());
        let mut worn = false;
        let (landed, taken) = self.session.vitals.hurt(amount, cause, invulnerable, |a| {
            let a = if armored {
                let (reduced, w) = simulation::damage::after_armor(a, cause, points, toughness);
                worn = w;
                reduced
            } else {
                a
            };
            crate::enchant::protect(a, &inv.armor, cause)
        });
        if taken > 0.0 && self.session.vitals.is_dead() {
            self.session.totem_saves(self.selected);
        }
        // A stronger hit inside the immunity window keeps the hurt direction, as Java skips it.
        if taken > 0.0 && self.session.vitals.last_hit_fresh() {
            self.session.player.animation.hurt_direction = 0.0;
        }
        (if worn { landed } else { 0.0 }, taken)
    }

    /// Damage from a mob, arrow, explosion or the world, through armor
    /// unless its type is `#bypasses_armor`. Knockback only lands
    /// with damage (hurt immunity also stops repeated shoves), and a survival
    /// agent killed this way drops everything. Returns the damage taken.
    pub fn hurt(&mut self, amount: f32, cause: impl Into<Cause>, knockback: DVec3, entities: &mut Entities) -> f32 {
        let cause = cause.into();
        if knockback.with_y(0.0).length_squared() > 0.0 && self.shielded(amount, -knockback) {
            return 0.0;
        }
        self.hurt_through(amount, cause, true, knockback, None, entities)
    }

    /// A raised shield facing `toward` (horizontally, from this agent to the
    /// attacker) stops the hit.
    fn shielded(&mut self, amount: f32, toward: DVec3) -> bool {
        self.session.shield_intercept(self.selected, amount, toward).blocked
    }

    /// Java's `setSprinting(false)` after a hit with extra knockback: held input stops
    /// sprinting until it is asked for again.
    pub fn stop_sprint(&mut self) {
        self.input.sprint = false;
        self.session.player.sprinting = false;
    }

    /// [`Agent::hurt`] delivering Java's knockback sequence instead of an impulse.
    pub fn hurt_knocked(
        &mut self,
        amount: f32,
        cause: impl Into<Cause>,
        hit: &crate::knockback::Hit,
        entities: &mut Entities,
    ) -> f32 {
        let cause = cause.into();
        if let Some(away) = hit.away()
            && self.shielded(amount, -away)
        {
            return 0.0;
        }
        self.hurt_through(amount, cause, true, DVec3::ZERO, Some(hit), entities)
    }

    /// Magic damage from something consumed (a harming potion): no armor or
    /// knockback, with protection and death/drop rules.
    pub fn hurt_magic(&mut self, amount: f32, entities: &mut Entities) -> f32 {
        self.hurt_magic_by(amount, Cause::MAGIC, entities)
    }

    /// v1.0 gap Track 8g: [`Agent::hurt_magic`] from `cause` (a thrown
    /// potion's `indirectMagic`).
    pub fn hurt_magic_by(&mut self, amount: f32, cause: Cause, entities: &mut Entities) -> f32 {
        self.hurt_through(amount, cause, false, DVec3::ZERO, None, entities)
    }

    /// Mounted falls bypass armor points, retaining protection and death/drop rules.
    pub fn hurt_fall(&mut self, amount: f32, entities: &mut Entities) {
        self.damage(amount, Cause::FALL);
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
        }
    }

    /// A warden's sonic boom (`SonicBoom.tick`): armor and enchantments don't
    /// reduce it, a shield raised toward the warden at `from` stops it, and it
    /// pushes by `push` (blocks per tick, before knockback resistance).
    pub fn hurt_sonic_boom(&mut self, damage: f32, push: DVec3, from: DVec3, entities: &mut Entities) -> f32 {
        if self.shielded(damage, from - self.session.player.pos) {
            return 0.0;
        }
        self.hurt_through(damage, Cause::SONIC_BOOM, false, push * crate::knockback::TICK_RATE, None, entities)
    }

    /// v1.0 gap Track 8d: `GuardianAttackGoal.tick`'s two hits,
    /// `indirectMagic` then the attack through armor, which the hurt
    /// cooldown lets in only by its excess.
    pub fn hurt_beam(&mut self, physical: f32, magic: f32, entities: &mut Entities) -> f32 {
        let first = self.hurt_through(magic, Cause::GUARDIAN_MAGIC, false, DVec3::ZERO, None, entities);
        first + self.hurt_through(physical, Cause::GUARDIAN_BEAM, true, DVec3::ZERO, None, entities)
    }

    fn hurt_through(
        &mut self,
        amount: f32,
        cause: Cause,
        armored: bool,
        knockback: DVec3,
        java: Option<&crate::knockback::Hit>,
        entities: &mut Entities,
    ) -> f32 {
        if self.session.vitals.is_dead() {
            return 0.0;
        }
        let (wear, taken) = self.damage_through(amount, cause, armored);
        if taken <= 0.0 {
            return 0.0;
        }
        if let Some(hit) = java {
            let mut jitter = || f64::from(entities.roll_range(0.0, 1.0));
            self.session.knocked_back(hit, &mut jitter);
        } else if self.session.vitals.last_hit_fresh() {
            // Only a full hit takes `hurtServer`'s push (v1.0 gap Track 8d).
            self.session.player.hurt_from(knockback);
            self.session.player.vel +=
                simulation::survival::knockback_taken(knockback, self.session.inventory.knockback_resistance());
        }
        // `doHurtEquipment` wears nothing for `#bypasses_armor` damage (no
        // `hurtArmor` call) or a zero hit.
        if wear > 0.0 {
            self.session.inventory.wear_armor(wear);
        }
        self.session.sleeping = None;
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
        }
        taken
    }

    /// Held device input for the next tick (a local controller). Unlike a
    /// timed `move` or `mine`, this keeps mining progress on the same block.
    pub fn hold(&mut self, input: MoveInput, mining: bool, using: bool) {
        if self.mining && !mining {
            self.session.cancel_mining();
        }
        if self.eating && !using {
            self.session.cancel_consumption();
        }
        self.input = input;
        self.mining = mining;
        self.eating = using;
        self.remaining = 1;
    }

    /// The block being mined and the fraction broken (for crack overlays).
    pub fn breaking(&self, world: &World) -> Option<(IVec3, f32)> {
        self.session.breaking(world)
    }

    /// Held movement for the current command or controller tick.
    pub fn movement_input(&self) -> MoveInput {
        if self.remaining > 0 { self.input } else { MoveInput::default() }
    }

    /// What this agent mines with, and where it stands (Java's penalties).
    #[cfg(test)]
    fn digger(&self, world: &World) -> crate::mining::Digger {
        crate::mining::Digger {
            held: self.session.inventory.get(self.selected),
            helmet: self.session.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.session.player.head_in_water(world),
            on_ground: self.session.player.on_ground || self.session.player.flying,
        }
    }

    /// Whether hostile mobs may attack this agent.
    pub fn targetable(&self) -> bool {
        !self.creative && self.session.mode.targetable() && !self.session.vitals.is_dead()
    }

    /// Structured observation includes loaded status, target, inventory and optional nearby cells.
    pub fn observe(&self, world: &World, radius: i32) -> Value {
        let center = self.session.player.pos.floor().as_ivec3();
        let mut blocks = Vec::new();
        if radius > 0 {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    for x in -radius..=radius {
                        let p = center + IVec3::new(x, y, z);
                        blocks.push(json!([p.x, p.y, p.z, world.get_block(p).map(|b| b.name())]));
                    }
                }
            }
        }
        let inventory: Vec<_> = self
            .inventory
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, s)| {
                s.map(|s| {
                    let enchants: Vec<String> = s.enchants.lines().into_iter().map(|(l, _)| l).collect();
                    json!({"slot":slot+1,"item":s.item.name(),"display_name":s.display_name(),"count":s.count,"damage":s.damage,"enchantments":enchants})
                })
            })
            .collect();
        let offers = self
            .table_offers(world)
            .map(|(_, o)| o.map(|o| json!({"cost":o.cost,"clue":o.clue.map(|(e, l)| e.describe(l))})).to_vec());
        let target = self.target(world).map(|(p, n)| {
            // v0.17 Track C1: a crafter's slots, toggles, power and result.
            let crafter = world.crafter(p).map(|c| {
                let slots: Vec<_> = (0..crate::world::crafter::SLOTS)
                    .map(|i| world.chest_slot(p, i).map(|s| json!({"item":s.item.name(),"count":s.count})))
                    .collect();
                let disabled: Vec<usize> = (0..crate::world::crafter::SLOTS).filter(|&i| c.disabled(i)).collect();
                let state = world.get_block(p).and_then(crate::world::crafter::decode);
                json!({"slots":slots,"disabled":disabled,"triggered":state.is_some_and(|s|s.triggered),
                    "crafting":state.is_some_and(|s|s.crafting),
                    "result":world.crafter_result(p).map(|s| json!({"item":s.item.name(),"count":s.count}))})
            });
            json!({"position":p.to_array(),"face":n.to_array(),"block":world.get_block(p).map(|b|b.name()),"enchanting_offers":offers,"crafter":crafter})
        });
        json!({"position":self.session.player.pos.to_array(),"yaw":self.session.player.yaw.to_degrees(),"pitch":self.session.player.pitch.to_degrees(),"loaded":world.is_loaded(center),"dimension":world.generator.dimension.name(),"health":self.session.vitals.health,"absorption":self.session.vitals.effects.absorption(),"food":self.session.vitals.hunger.food,"level":self.session.vitals.xp.level,"xp_progress":self.session.vitals.xp.progress(),"dead":self.session.vitals.is_dead(),"mode":self.session.mode.name(),"creative":self.creative,"flying":self.session.player.flying,"gliding":self.session.player.fall_flying,"sleeping":self.session.sleeping.is_some(),"spawn_bed":self.session.spawn_bed.map(|p|p.to_array()),"spawn_point":self.session.spawn_point.map(|p|p.to_array()),"selected":self.selected+1,"inventory":inventory,"target":target,"blocks":blocks,"recipe_book":self.recipe_book_state()})
    }
}

impl Agent {
    /// v1.0 gap track 6b: `bundle <slot> [stack]`, the agent's
    /// `BundleMouseActions` pick and right click: the picked stack (a cell
    /// of the contents tooltip, `shown_count` of them; the front stack
    /// without a pick) comes out of the bundle onto the cursor, and from
    /// there into the inventory (what doesn't fit is dropped).
    fn bundle_take(&mut self, slot: usize, pick: Option<usize>, entities: &mut Entities) -> Result<(), String> {
        use crate::content::bundle;
        let inv = &mut self.session.inventory;
        let mut cell = inv.get(slot).filter(|s| bundle::is_bundle(s.item));
        let Some(stack) = cell else { return Err("no bundle in that slot".into()) };
        let shown = bundle::shown_count(stack.bundle_contents().len());
        if stack.bundle_contents().is_empty() {
            return Err("the bundle is empty".into());
        }
        if pick.is_some_and(|p| p >= shown) {
            return Err(format!("the bundle shows {shown} stacks"));
        }
        let mut cursor = None;
        let ((), _) = bundle::clicking(pick, || {
            bundle::click(&mut cell, &mut cursor, true);
        });
        inv.slots[slot] = cell;
        if let Some(out) = cursor {
            let left = inv.add_stack(out);
            if left > 0 {
                crate::session::storage::toss(&self.session, entities, crate::inventory::Stack { count: left, ..out });
            }
        }
        Ok(())
    }
}

/// `chest take` or `chest put` on one container slot: the slot's stack
/// into the inventory, or as much of the selected stack as the slot takes.
fn chest_transfer(
    dest: &mut Option<Stack>,
    inventory: &mut crate::inventory::Inventory,
    selected: usize,
    take: bool,
) -> Result<(), String> {
    if take {
        let stack = dest.ok_or("chest slot empty")?;
        let left = inventory.add_stack(stack);
        *dest = (left > 0).then_some(Stack { count: left, ..stack });
        if left == stack.count {
            return Err("inventory full".into());
        }
    } else {
        let stack = inventory.get(selected).ok_or("selected slot empty")?;
        let room = match dest {
            None => stack.max(),
            Some(s) if s.stacks_with(&stack) => s.max() - s.count,
            _ => 0,
        };
        let n = room.min(stack.count);
        if n == 0 {
            return Err("chest slot full or incompatible".into());
        }
        *dest = Some(Stack { count: dest.map_or(0, |s| s.count) + n, ..stack });
        inventory.slots[selected] = (stack.count > n).then_some(Stack { count: stack.count - n, ..stack });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        chunk::ChunkData,
        terrain::{Dimension, Generator},
    };
    use crate::{player::Player, simulation::survival::Vitals};
    use crate::{simulation::effects::Effect, world::terrain::Biome};
    use std::sync::Arc;
    fn world() -> World {
        let mut saved = rustc_hash::FxHashMap::default();
        saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
        for _ in 0..10000 {
            world.update(DVec3::new(1.0, 150.0, 1.0));
            if world.is_loaded(IVec3::new(1, 150, 1)) {
                return world;
            }
            std::thread::yield_now();
        }
        panic!("world failed to load");
    }
    #[test]
    fn milk_clears_effects_at_full_hunger_and_stew_returns_a_bowl() {
        let mut w = world();
        let mut e = Entities::new(6);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        w.set_block(IVec3::new(1, 149, 1), Block::STONE);
        a.inventory.slots[0] = Some(Stack::new(Item::MILK_BUCKET, 1));
        a.vitals.apply_effect(Effect::Poison, 0, 600);
        a.vitals.apply_effect(Effect::Speed, 1, 600);
        a.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
        for _ in 0..EAT_TICKS {
            a.tick(&mut w, &mut e);
        }
        assert!(!a.vitals.effects.has(Effect::Poison));
        assert!(!a.vitals.effects.has(Effect::Speed));
        assert_eq!(a.inventory.get(0).unwrap().item, Item::BUCKET);
        a.inventory.slots[0] = Some(Stack::new(Item::MUSHROOM_STEW, 1));
        a.vitals.hunger.food = 10.0;
        a.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
        for _ in 0..EAT_TICKS {
            a.tick(&mut w, &mut e);
        }
        assert_eq!(a.inventory.get(0).unwrap().item, Item::BOWL);
        assert_eq!(a.vitals.hunger.food, 16.0);
    }

    /// Issue #92: a LAN guest's book opened from the off-hand is saved and
    /// signed there, with the selected hotbar slot empty and left alone.
    #[test]
    fn book_commands_edit_the_offhand_book_when_named() {
        use crate::content::books;
        use crate::session::books::OFFHAND_SLOT;
        let mut w = world();
        let mut e = Entities::new(7);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.offhand = Some(Stack::new(books::WRITABLE_BOOK, 1));
        let write = Command::parse(&Command::book_text(OFFHAND_SLOT, &["draft".into()], None)).unwrap();
        a.execute(write, &mut w, &mut e, &[]).unwrap();
        assert_eq!(books::pages(&a.inventory.offhand.unwrap()), ["draft"]);
        assert!(a.inventory.slots[a.selected].is_none());
        let sign = Command::parse(&Command::book_text(OFFHAND_SLOT, &["final".into()], Some("Notes"))).unwrap();
        a.execute(sign, &mut w, &mut e, &[]).unwrap();
        let signed = a.inventory.offhand.unwrap();
        assert_eq!((signed.item, books::pages(&signed)), (books::WRITTEN_BOOK, vec!["final".to_string()]));
        assert!(a.inventory.slots[a.selected].is_none());
        // Without `offhand` the command still names the selected slot, which
        // is empty here, so it's refused rather than touching the off-hand.
        let hotbar = Command::parse(&Command::book_text(a.selected, &["x".into()], None)).unwrap();
        assert!(a.execute(hotbar, &mut w, &mut e, &[]).is_err());
        assert_eq!(a.inventory.offhand.unwrap().item, books::WRITTEN_BOOK);
    }

    #[test]
    fn replacing_a_targeted_block_keeps_progress_at_the_new_rate_until_it_is_air() {
        let mut world = world();
        let at = IVec3::new(3, 151, 1);
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        world.set_block(at, Block::DIRT);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let mut e = Entities::new(1);
        for _ in 0..10 {
            a.hold(MoveInput::default(), true, false);
            a.tick(&mut world, &mut e);
        }
        let (_, before) = a.breaking(&world).unwrap();
        // Java keys progress on position and held item, not the block state.
        world.set_block(at, Block::STONE);
        assert_eq!(a.breaking(&world), Some((at, before)));
        a.hold(MoveInput::default(), true, false);
        a.tick(&mut world, &mut Entities::new(1));
        assert_eq!(world.get_block(at), Some(Block::STONE));
        let delta = crate::mining::dig_progress(Block::STONE, a.digger(&world), 1.0);
        assert_eq!(a.breaking(&world), Some((at, before + delta)));
        world.set_block(at, Block::AIR);
        assert!(a.breaking(&world).is_none(), "cracks disappear once the cell is air");
    }

    #[test]
    fn sprint_attacks_do_not_sweep_nearby_mobs() {
        let mut world = world();
        for sprint in [false, true] {
            let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
            a.player.on_ground = true;
            a.inventory.slots[0] =
                Some(Stack::new(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Wood), 1));
            // Java sweeps and sprint knockback need a charged (> 0.9) attack.
            for _ in 0..13 {
                a.tick(&mut world, &mut Entities::new(1));
            }
            assert!(a.session.attack_strength(0, 0.5) > 0.9);
            // The test world is open air: put the charged player back on its spot.
            a.player.pos = DVec3::new(1.5, 150.0, 1.5);
            a.player.vel = DVec3::ZERO;
            a.player.on_ground = true;
            a.hold(MoveInput { forward: 1.0, sprint, ..Default::default() }, false, false);
            // The sprint key latches Java's sprint on the next movement tick.
            a.player.sprinting = sprint;
            let mut e = Entities::new(1);
            e.spawn(crate::entity::MobKind::Zombie, DVec3::new(3.0, 150.0, 1.5));
            e.spawn(crate::entity::MobKind::Zombie, DVec3::new(3.0, 150.0, 2.3));
            a.execute(Command::Attack, &mut world, &mut e, &[]).unwrap();
            assert!(e.mobs[0].health < crate::entity::MobKind::Zombie.max_health());
            assert_eq!(e.mobs[1].health, crate::entity::MobKind::Zombie.max_health() - if sprint { 0.0 } else { 1.0 });
        }
    }

    #[test]
    fn mining_speed_changes_only_affect_future_progress() {
        use crate::enchant::Enchantment;
        let mut world = world();
        for x in 0..5 {
            for z in 0..3 {
                world.set_block(IVec3::new(x, 149, z), Block::STONE);
            }
        }
        let at = IVec3::new(3, 151, 1);
        world.set_block(at, Block::STONE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots[0] = Some(Stack::new(Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Wood), 1));
        let mut entities = Entities::new(1);
        for _ in 0..3 {
            a.hold(MoveInput::default(), true, false);
            a.tick(&mut world, &mut entities);
        }
        let before = a.breaking(&world).unwrap();
        assert_eq!(before.0, at);
        a.inventory.slots[0].as_mut().unwrap().enchants =
            crate::enchant::Enchants::NONE.with(Enchantment::Efficiency, 5);
        assert_eq!(a.breaking(&world).unwrap(), before, "changing tools cannot change existing progress");
        // Java `sameDestroyTarget` compares item components: the enchanted
        // pickaxe restarts without progress this tick, then accrues its rate.
        a.hold(MoveInput::default(), true, false);
        a.tick(&mut world, &mut entities);
        assert!(a.breaking(&world).is_none(), "a different held stack restarts the dig");
        a.hold(MoveInput::default(), true, false);
        a.tick(&mut world, &mut entities);
        let after = a.breaking(&world).unwrap();
        let expected = (TICK_SECONDS / crate::mining::dig_time(Block::STONE, a.digger(&world)) as f64) as f32;
        assert!((after.1 - expected).abs() < 1e-6);
        assert_eq!(world.get_block(at), Some(Block::STONE), "a faster tool does not retroactively finish the dig");
    }

    #[test]
    fn a_riding_player_freezes_or_not_by_current_armour() {
        // `LivingEntity.canFreeze` reads the armour every tick, mounted too.
        use crate::item::{ArmorMaterial, ArmorPiece};
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.session.player.vehicle = Some(999);
        a.tick(&mut world, &mut entities);
        assert!(!a.session.player.freeze_immune);
        a.session.inventory.armor[0] = Some(Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather), 1));
        a.tick(&mut world, &mut entities);
        assert!(a.session.player.freeze_immune, "equipping leather while riding counts");
    }

    #[test]
    fn depth_strider_bonus_is_halved_off_the_ground() {
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        world.set_block(IVec3::new(1, 150, 1), Block::WATER);
        let speed = |world: &World, grounded, level| {
            let mut p = Player::new(DVec3::new(1.5, 150.0, 1.5));
            p.on_ground = grounded;
            p.wear_boots(level);
            p.tick(MoveInput { forward: 1.0, ..Default::default() }, world);
            p.vel.x / 20.0
        };
        // `travelInWater`: efficiency e moves the 0.8 slow-down toward
        // 0.546 and the 0.02 speed toward the walking speed; off the ground
        // e is halved. From rest, one tick keeps speed * 0.98 * slow-down.
        let expected = |e: f32| {
            let slow = 0.8_f32 + (0.54600006 - 0.8) * e;
            let speed = 0.02_f32 + (0.1 - 0.02) * e;
            speed as f64 * 0.98_f32 as f64 * slow as f64
        };
        let full = 0.33333334_f32 * 3.0;
        assert!((speed(&world, true, 3) - expected(full)).abs() < 1e-9);
        assert!((speed(&world, false, 3) - expected(full * 0.5)).abs() < 1e-9);
        assert!((speed(&world, true, 0) - expected(0.0)).abs() < 1e-9);
        world.set_block(IVec3::new(1, 150, 1), Block::LAVA);
        assert_eq!(speed(&world, true, 3), speed(&world, true, 0));
    }

    #[test]
    fn hurt_applies_armor_knockback_immunity_and_death_drops() {
        let mut entities = Entities::new(1);
        let mut bare = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        let mut armored = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        let chest = Item::armor(crate::item::ArmorPiece::Chestplate, crate::item::ArmorMaterial::Iron);
        armored.inventory.armor[1] = Some(Stack::new(chest, 1));
        let push = DVec3::new(6.0, 5.0, 0.0);
        assert_eq!(bare.hurt(4.0, "was slain by a zombie", push, &mut entities), 4.0);
        assert_eq!(bare.player.vel, push);
        assert!(armored.hurt(4.0, "was slain by a zombie", push, &mut entities) < 4.0, "armor absorbs some");
        assert!(armored.inventory.armor[1].unwrap().damage > 0, "armor wears");
        // Hurt immunity: an equal hit right after doesn't land or shove.
        assert_eq!(bare.hurt(4.0, "was slain by a zombie", push, &mut entities), 0.0);
        assert_eq!(bare.player.vel, push);
        // Full Netherite: toughness and 0.4 knockback resistance.
        let mut netherite = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        for piece in crate::item::ArmorPiece::ALL {
            let item = Item::armor(piece, crate::item::ArmorMaterial::Netherite);
            netherite.inventory.armor[piece as usize] = Some(Stack::new(item, 1));
        }
        let taken = netherite.hurt(20.0, "was slain by a zombie", push, &mut entities);
        assert!((taken - 20.0 * 9.0 / 25.0).abs() < 1e-4, "{taken}");
        assert!((netherite.player.vel - push * 0.6).length() < 1e-6);

        let mut creative = Agent::new(DVec3::ZERO);
        creative.creative = true;
        assert!(!creative.targetable());
        assert_eq!(creative.hurt(30.0, "was blown up by a creeper", push, &mut entities), 0.0);

        bare.inventory.add(Item::DIAMOND, 2);
        bare.vitals.health = 1.0;
        bare.vitals.reset_fall();
        bare.hurt(30.0, "was blown up by a creeper", DVec3::ZERO, &mut entities);
        assert!(bare.vitals.is_dead() && !bare.targetable());
        // v1.0 gap Track 8g: Java names the mob by its type ("Creeper").
        assert_eq!(bare.vitals.death.as_deref(), Some("was blown up by Creeper"));
        assert!(bare.inventory.get(0).is_none());
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 2);
    }
    #[test]
    fn device_hands_keep_a_raised_shield_up() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        a.inventory.offhand = Some(Stack::new(Item::SHIELD, 1));
        assert!(a.session.start_blocking(0));
        for _ in 0..crate::session::hands::SHIELD_DELAY + 1 {
            // A controller or LAN adapter: held input, then its shared hands tick the shield.
            a.hold(MoveInput::default(), false, false);
            a.session.tick_blocking(0, true);
            a.tick(&mut world, &mut entities);
        }
        assert!(a.session.blocking(), "the agent's own tick must not lower a device-held shield");

        // No shared hands (a headless host, or a controller menu open): the
        // agent's own tick raises the shield while use is held and lowers it on release.
        let mut b = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        b.inventory.offhand = Some(Stack::new(Item::SHIELD, 1));
        assert!(b.session.start_blocking(0));
        for _ in 0..crate::session::hands::SHIELD_DELAY + 1 {
            b.hold(MoveInput::default(), false, true);
            b.tick(&mut world, &mut entities);
        }
        assert!(b.session.blocking());
        b.hold(MoveInput::default(), false, false);
        b.tick(&mut world, &mut entities);
        assert!(!b.session.shield_raised());
    }

    #[test]
    fn an_offhand_rod_keeps_its_bobber() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        a.inventory.offhand = Some(Stack::new(Item::FISHING_ROD, 1));
        entities.cast_bobber(a.session.id, a.session.player.eye(), DVec3::X, 0, 0);
        a.tick(&mut world, &mut entities);
        assert!(entities.has_bobber(a.session.id));
        a.inventory.offhand = None;
        a.tick(&mut world, &mut entities);
        assert!(!entities.has_bobber(a.session.id));
    }

    #[test]
    fn a_totem_saves_from_wither() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        a.inventory.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
        a.vitals.health = 1.0;
        a.vitals.effects.add(crate::simulation::effects::Effect::Wither, 1, 200);
        for _ in 0..60 {
            a.tick_rules(&mut world, &mut entities, Default::default(), &Default::default());
        }
        assert!(!a.vitals.is_dead(), "{:?}", a.vitals.death);
        assert_eq!(a.inventory.offhand, None, "the totem is used up");
    }

    #[test]
    fn every_death_path_follows_keep_inventory() {
        let mut world = world();
        let mut entities = Entities::new(1);
        // `kill` drops everything, like other deaths.
        let mut killed = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        killed.inventory.add(Item::DIAMOND, 3);
        killed.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
        assert!(killed.vitals.is_dead() && killed.inventory.get(0).is_none());
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 3);

        // With keepInventory, mob damage and `kill` keep items and experience.
        let mut rules = crate::rules::GameRules::default();
        rules.set("keep_inventory", "true").unwrap();
        let mut kept = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        kept.tick_rules(&mut world, &mut entities, Default::default(), &rules);
        kept.inventory.add(Item::DIAMOND, 2);
        kept.vitals.xp.add_points(50);
        let xp = kept.vitals.xp;
        kept.hurt(100.0, "was slain by a zombie", DVec3::ZERO, &mut entities);
        assert!(kept.vitals.is_dead());
        assert!(kept.inventory.get(0).is_some(), "keepInventory keeps the hotbar");
        assert_eq!(kept.vitals.xp, xp, "keepInventory keeps experience");
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 3, "nothing new dropped");
    }

    #[test]
    fn drinking_a_potion_applies_it_and_leaves_a_bottle() {
        use crate::potion::Potion;
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        // Not hungry, but potions drink anyway.
        a.inventory.add(Item::potion(Potion::from_id("swiftness").unwrap()), 1);
        a.execute(Command::Eat, &mut world, &mut entities, &[]).unwrap();
        for _ in 0..EAT_TICKS {
            a.tick(&mut world, &mut entities);
        }
        assert_eq!(a.vitals.effects.get(Effect::Speed).map(|e| e.amplifier), Some(0));
        assert_eq!(a.inventory.get(0).map(|s| s.item), Some(Item::GLASS_BOTTLE));
    }

    #[test]
    fn eating_takes_a_full_bite_and_held_use_keeps_chewing() {
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        assert!(a.execute(Command::Eat, &mut world, &mut entities, &[]).is_err(), "nothing held");
        a.inventory.add(Item::BREAD, 2);
        assert!(a.execute(Command::Eat, &mut world, &mut entities, &[]).is_err(), "not hungry");
        a.vitals.hunger = crate::simulation::survival::Hunger::restore(10.0, 0.0, 0.0);
        a.execute(Command::Eat, &mut world, &mut entities, &[]).unwrap();
        for _ in 0..EAT_TICKS - 1 {
            a.tick(&mut world, &mut entities);
        }
        assert_eq!(a.inventory.get(0).unwrap().count, 2, "bite unfinished");
        a.tick(&mut world, &mut entities);
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
        assert_eq!(a.vitals.hunger.food, 15.0);
        assert_eq!(a.remaining, 0);
        // A controller holding use chews until released; letting go restarts the bite.
        for _ in 0..EAT_TICKS / 2 {
            a.hold(MoveInput::default(), false, true);
            a.tick(&mut world, &mut entities);
        }
        a.hold(MoveInput::default(), false, false);
        a.tick(&mut world, &mut entities);
        assert_eq!(a.eating(), 0.0);
        for _ in 0..EAT_TICKS {
            a.hold(MoveInput::default(), false, true);
            a.tick(&mut world, &mut entities);
        }
        assert!(a.inventory.get(0).is_none());
        assert_eq!(a.vitals.hunger.food, 20.0);
    }
    #[test]
    fn resting_keeps_an_agent_in_bed_and_acting_or_hurt_wakes_it() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let mut run = |agent: &mut Agent, text: &str| {
            agent.execute(Command::parse(text).unwrap(), &mut world, &mut entities, &[]).unwrap();
        };
        assert!(matches!(Command::parse("sleep"), Ok(Command::Sleep)));
        agent.sleeping = Some(1.0);
        for rest in ["wait 5", "observe", "help", "move 0 0 3 sneak"] {
            run(&mut agent, rest);
            assert!(agent.sleeping.is_some(), "{rest} keeps sleeping");
        }
        run(&mut agent, "select 2");
        assert!(agent.sleeping.is_none(), "acting gets up");
        agent.sleeping = Some(1.0);
        run(&mut agent, "move 1 0 3");
        assert!(agent.sleeping.is_none(), "walking gets up");
        agent.sleeping = Some(1.0);
        agent.hurt(1.0, "test", DVec3::ZERO, &mut entities);
        assert!(agent.sleeping.is_none(), "being hurt wakes");
    }

    #[test]
    fn respawns_at_its_bed_while_the_bed_stands() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let bed = IVec3::new(1, 150, 1);
        for x in 0..4 {
            for z in 0..3 {
                world.set_block(IVec3::new(x, 149, z), Block::STONE);
            }
        }
        world.set_block(bed, Block::BED_FOOT);
        world.set_block(bed + IVec3::X, Block::BED_HEAD);
        let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        agent.spawn_bed = Some(bed);
        let mut die_and_respawn = |agent: &mut Agent, world: &mut World| {
            agent.vitals.damage(100.0, "test", false);
            agent.execute(Command::Respawn, world, &mut entities, &[]).unwrap();
        };
        die_and_respawn(&mut agent, &mut world);
        // Beside the head on the side a stored angle of 0 steers to (north).
        assert_eq!(agent.player.pos, DVec3::new(2.5, 150.0, 0.5));
        world.set_block(bed, Block::AIR);
        die_and_respawn(&mut agent, &mut world);
        assert_eq!(agent.spawn_bed, None, "a broken bed is forgotten");
        assert_ne!(agent.player.pos.floor().as_ivec3(), bed);
    }

    #[test]
    fn parsing_bounds() {
        for s in [
            "tp NaN 0 0",
            "look inf 0",
            "move 2 0 1",
            "wait 0",
            "mine 201",
            "select 0",
            "time NaN",
            "give dirt 0",
            "observe 3",
            "place extra",
            "effect give nope",
            "effect give speed 0",
            "effect give speed 10 300",
            "effect clear speed extra",
        ] {
            assert!(Command::parse(s).is_err(), "{s}");
        }
        assert!(matches!(
            Command::parse("effect give minecraft:strength 90 1").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Strength, 1800, 1, true))
        ));
        assert!(matches!(
            Command::parse("effect give regeneration").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Regeneration, 600, 0, true))
        ));
        // `EffectCommands.computeDurationInTicks`: instantaneous effects
        // count ticks and default to one.
        assert!(matches!(
            Command::parse("effect give saturation 5").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Saturation, 5, 0, true))
        ));
        assert!(matches!(
            Command::parse("effect give instant_health").unwrap(),
            Command::Effect(EffectChange::Give(Effect::InstantHealth, 1, 0, true))
        ));
        assert!(matches!(Command::parse("effect clear").unwrap(), Command::Effect(EffectChange::Clear(None))));
        // Java's forms with a target, as players type them.
        assert!(matches!(
            Command::parse("/effect give @s resistance 600 4").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Resistance, 12000, 4, true))
        ));
        assert!(matches!(Command::parse("effect clear @p").unwrap(), Command::Effect(EffectChange::Clear(None))));
        assert!(matches!(
            Command::parse("/enchant @s loyalty 3").unwrap(),
            Command::Enchant(crate::enchant::Enchantment::Loyalty, 3)
        ));
        assert!(matches!(Command::parse("xp add @s 30 levels"), Ok(Command::Xp(XpChange { levels: true, .. }))));
        assert!(Command::parse("effect give @a speed").is_err(), "other targets stay rejected");
        let mut vitals = Vitals::default();
        vitals.health = 10.0;
        assert_eq!(EffectChange::Give(Effect::InstantDamage, 1, 0, true).apply(&mut vitals, "Player").unwrap().0, 6.0);
        EffectChange::Give(Effect::InstantHealth, 1, 1, true).apply(&mut vitals, "Player").unwrap();
        assert_eq!(vitals.health, 18.0);
        assert!(matches!(Command::parse("/give dirt 64").unwrap(), Command::Give(_, 64)));
        assert!(matches!(
            Command::parse("/enchant silk_touch").unwrap(),
            Command::Enchant(crate::enchant::Enchantment::SilkTouch, 1)
        ));
        assert!(Command::parse("/enchant sharpness 0").is_err() && Command::parse("/enchant speed").is_err());
        let xp = |text: &str| match Command::parse(text) {
            Ok(Command::Xp(c)) => Some((c.set, c.amount, c.levels)),
            _ => None,
        };
        assert_eq!(xp("/xp add 30 levels"), Some((false, 30, true)));
        assert_eq!(xp("xp add -5"), Some((false, -5, false)));
        assert_eq!(xp("xp set 3 points"), Some((true, 3, false)));
        assert!(xp("xp set -1 levels").is_none() && xp("xp add 5 hearts").is_none() && xp("xp add").is_none());
        assert!(matches!(Command::parse("xp query"), Ok(Command::XpQuery)));
        assert!(matches!(Command::parse("/xp query @s levels"), Ok(Command::XpQuery)));
        assert!(Command::parse("xp add 1").unwrap().cheat());
        assert!(matches!(Command::parse("gamerule keepInventory").unwrap(), Command::GameRule { .. }));
        assert!(matches!(Command::parse("time query daytime").unwrap(), Command::TimeQuery(TimeQuery::Daytime)));
        assert!(matches!(Command::parse("weather thunder").unwrap(), Command::Weather(WeatherKind::Thunder)));
        assert!(matches!(Command::parse("locate biome plains").unwrap(), Command::LocateBiome(Biome::Plains)));
        assert!(matches!(
            Command::parse("locate biome minecraft:crimson_forest").unwrap(),
            Command::LocateNetherBiome(crate::world::nether_biome::NetherBiome::CrimsonForest)
        ));
        assert!(tab_complete("/gamemode surv").unwrap().starts_with("/gamemode survival"));
    }

    #[test]
    fn adventure_and_spectator_enforce_player_abilities() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.execute(Command::Mode(GameMode::Adventure), &mut world, &mut entities, &[]).unwrap();
        assert!(a.execute(Command::parse("mine 1").unwrap(), &mut world, &mut entities, &[]).is_err());
        assert!(a.targetable());

        a.execute(Command::Mode(GameMode::Spectator), &mut world, &mut entities, &[]).unwrap();
        assert!(a.player.flying && a.player.noclip && !a.targetable());
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.hurt(20.0, "test", DVec3::ZERO, &mut entities), 0.0);
    }

    #[test]
    fn hardcore_death_becomes_spectator() {
        let mut a = Agent::new(DVec3::ZERO);
        a.vitals.damage(100.0, "test", false);
        a.hardcore_spectate();
        assert!(!a.vitals.is_dead());
        assert_eq!(a.mode, GameMode::Spectator);
    }

    #[test]
    fn agent_wolf_armor_dye_crafting_uses_real_components_and_reports_the_actual_result() {
        use crate::{color::DyeColor, entity::wolf_armor};
        let mut world = world();
        let mut entities = Entities::new(91);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let mut armor =
            wolf_armor::dye(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Red]).unwrap().with_name("Cedar").unwrap();
        armor.damage = 17;
        armor.repair_cost = 3;
        a.inventory.slots[0] = Some(armor);
        a.inventory.slots[1] = Some(Stack::new(DyeColor::Blue.dye(), 5));
        let expected = wolf_armor::dye(armor, &[DyeColor::Blue]).unwrap();
        assert_eq!(a.craftable(&world).iter().find(|s| s.item == Item::WOLF_ARMOR).copied(), Some(expected));
        a.execute(Command::Craft(Item::WOLF_ARMOR), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0), Some(expected));
        assert_eq!(a.inventory.get(1).unwrap().count, 4);
    }

    #[test]
    fn independent_players_and_atomic_crafting() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let b = Agent::new(DVec3::new(4.5, 150.0, 1.5));
        a.inventory.add(Block::LOG, 1);
        a.execute(Command::Craft(Item::from_block(Block::PLANKS)), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().count, 4);
        let before = a.inventory.clone();
        assert!(
            a.execute(
                Command::Craft(Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Diamond)),
                &mut world,
                &mut entities,
                &[]
            )
            .is_err()
        );
        assert_eq!(a.inventory, before);
        assert!(b.inventory.get(0).is_none());
        a.execute(Command::Mode(GameMode::Creative), &mut world, &mut entities, &[]).unwrap();
        a.execute(Command::Fly(true), &mut world, &mut entities, &[]).unwrap();
        a.execute(Command::parse("move 1 0 20").unwrap(), &mut world, &mut entities, &[]).unwrap();
        for _ in 0..20 {
            a.tick(&mut world, &mut entities);
        }
        // Java's creative flight from rest covers about six blocks in a second.
        assert!(a.player.pos.x > 7.0, "{}", a.player.pos.x);
        assert_eq!(a.remaining, 0);
        assert_eq!(b.player.pos.x, 4.5);
    }
    #[test]
    fn tipped_arrow_inventory_crafting_keeps_potion_and_requires_table() {
        let mut a = Agent::new(DVec3::ZERO);
        let recipe = crafting::recipes().iter().find(|r| r.result.item == Item::TIPPED_ARROW).unwrap();
        let potion = crate::potion::Potion::from_id("poison").unwrap();
        a.inventory.slots[0] = Some(Stack::lingering_potion(potion));
        a.inventory.slots[1] = Some(Stack::new(Item::ARROW, 7));
        let before = a.inventory.clone();
        assert!(a.crafted(recipe, true).is_none());
        assert_eq!(a.inventory, before);
        a.inventory.slots[2] = Some(Stack::new(Item::ARROW, 1));
        assert!(a.crafted(recipe, false).is_none());
        let (inventory, remainders, result, _) = a.crafted(recipe, true).unwrap();
        assert!(remainders.is_empty());
        assert_eq!(result, Stack::tipped_arrow(potion, 8));
        assert_eq!(inventory.slots.iter().flatten().copied().collect::<Vec<_>>(), vec![result]);
    }
    #[test]
    fn death_drops_work_inputs_and_destroys_vanishing_gear() {
        let mut a = Agent::new(DVec3::ZERO);
        let sword = Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Iron);
        a.work[0] = Some(Stack {
            enchants: crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::VanishingCurse, 1),
            ..Stack::new(sword, 1)
        });
        a.work[1] = Some(Stack::new(Item::DIAMOND, 2));
        let mut entities = Entities::new(1);
        a.hurt(30.0, "was slain by a zombie", DVec3::ZERO, &mut entities);
        assert!(a.vitals.is_dead());
        assert_eq!(a.work, [None; 3]);
        assert_eq!(entities.items.len(), 1);
        assert_eq!(entities.items[0].stack, Stack::new(Item::DIAMOND, 2));
    }

    #[test]
    fn enchanting_can_use_the_slot_freed_by_lapis() {
        let mut world = world();
        world.set_block(IVec3::new(4, 151, 1), Block::ENCHANTING_TABLE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots.fill(Some(Stack::new(Item::STICK, 64)));
        a.inventory.slots[0] = Some(Stack::new(Item::BOOK, 2));
        a.inventory.slots[1] = Some(Stack::new(Item::LAPIS_LAZULI, 1));
        a.vitals.xp.add_levels(30);
        a.enchant_at_table(0, &world).unwrap();
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::BOOK, 1)));
        let book = a.inventory.get(1).unwrap();
        assert_eq!(book.item, Item::ENCHANTED_BOOK);
        assert!(!book.enchants.is_empty());
        assert_eq!(a.vitals.xp.level, 29);
    }

    #[test]
    fn agents_enchant_at_a_table_for_levels_and_lapis() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let pos = IVec3::new(4, 151, 1);
        world.set_block(pos, Block::ENCHANTING_TABLE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let pick = Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Iron);
        a.inventory.slots[0] = Some(Stack::new(pick, 1));
        a.inventory.slots[1] = Some(Stack::new(Item::LAPIS_LAZULI, 2));
        let offers = a.observe(&world, 0)["target"]["enchanting_offers"].clone();
        assert_eq!(offers.as_array().map(Vec::len), Some(3), "{offers}");
        let cost = offers[1]["cost"].as_u64().unwrap() as u32;
        assert!(a.execute(Command::Enchanting(1), &mut world, &mut entities, &[]).is_err(), "no levels yet");
        a.vitals.xp.add_levels(cost as i64);
        let seed = a.vitals.xp.seed;
        a.execute(Command::Enchanting(1), &mut world, &mut entities, &[]).unwrap();
        let held = a.inventory.get(0).unwrap();
        assert!(!held.enchants.is_empty());
        assert_eq!(a.vitals.xp.level, cost - 2, "the second offer costs two levels");
        assert_eq!(a.inventory.get(1), None, "and two lapis");
        assert_ne!(a.vitals.xp.seed, seed, "a new seed rolls new offers");
        assert!(a.execute(Command::Enchanting(0), &mut world, &mut entities, &[]).is_err(), "already enchanted");
    }

    #[test]
    fn agents_combine_on_an_anvil() {
        use crate::enchant::{Enchantment, Enchants};
        let mut world = world();
        let mut entities = Entities::new(1);
        world.set_block(IVec3::new(4, 150, 1), Block::STONE);
        world.set_block(IVec3::new(4, 151, 1), Block::ANVIL);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let sword = Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Iron);
        let book = Enchants::NONE.with(Enchantment::Sharpness, 2);
        a.inventory.slots[0] = Some(Stack::new(sword, 1));
        a.inventory.slots[1] = Some(Stack { enchants: book, ..Stack::new(Item::ENCHANTED_BOOK, 1) });
        let short = a.execute(Command::Anvil(1), &mut world, &mut entities, &[]);
        assert_eq!(short, Err("needs level 2".to_string()));
        a.vitals.xp.add_levels(3);
        a.execute(Command::Anvil(1), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().enchants.level(Enchantment::Sharpness), 2);
        assert_eq!(a.inventory.get(1), None, "the book is used up");
        assert_eq!(a.vitals.xp.level, 1);
    }

    #[test]
    fn agents_rename_on_an_anvil_for_a_level() {
        let mut world = world();
        let mut entities = Entities::new(1);
        world.set_block(IVec3::new(4, 150, 1), Block::STONE);
        world.set_block(IVec3::new(4, 151, 1), Block::ANVIL);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots[0] = Some(Stack::new(Item::DIAMOND, 5));
        a.vitals.xp.add_levels(1);
        a.execute(Command::parse("anvil name Shiny rocks").unwrap(), &mut world, &mut entities, &[]).unwrap();
        let named = a.inventory.get(0).unwrap();
        assert_eq!(named.display_name(), "Shiny rocks");
        assert_eq!((named.count, named.repair_cost), (5, 0), "a rename alone adds no prior-work penalty");
        assert_eq!(a.vitals.xp.level, 0);
        assert!(Command::parse(&format!("anvil name {}", "x".repeat(51))).is_err(), "Java's 50-character limit");
    }

    #[test]
    fn agents_upgrade_held_diamond_gear_at_a_smithing_table() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::item::{Tier, ToolKind};
        let mut world = world();
        let mut entities = Entities::new(1);
        world.set_block(IVec3::new(4, 151, 1), Block::SMITHING_TABLE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let base = Stack {
            damage: 75,
            enchants: Enchants::NONE.with(Enchantment::Efficiency, 4),
            repair_cost: 7,
            ..Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Diamond), 1).with_name("Deep Delver").unwrap()
        };
        a.inventory.slots[0] = Some(base);
        a.inventory.slots[1] = Some(Stack::new(Item::NETHERITE_UPGRADE, 2));
        a.inventory.slots[2] = Some(Stack::new(Item::NETHERITE_INGOT, 3));
        a.execute(Command::Smithing, &mut world, &mut entities, &[]).unwrap();
        let result = a.inventory.get(0).unwrap();
        assert_eq!(result.item, Item::tool(ToolKind::Pickaxe, Tier::Netherite));
        assert_eq!((result.damage, result.enchants, result.repair_cost), (75, base.enchants, 7));
        assert_eq!(result.display_name(), "Deep Delver");
        assert_eq!(a.inventory.get(1).unwrap().count, 1);
        assert_eq!(a.inventory.get(2).unwrap().count, 2);
        assert!(
            a.execute(Command::Smithing, &mut world, &mut entities, &[]).is_err(),
            "Netherite cannot upgrade again"
        );
    }

    #[test]
    fn guarded_container_transfers_and_mining_anger_piglins_at_the_agent() {
        use crate::entity::{Ctx, EntityEvent, MobKind, PlayerId, Target};
        for mining in [false, true] {
            let mut world = world();
            for x in 0..=6 {
                for z in 0..=4 {
                    world.set_block(IVec3::new(x, 149, z), Block::STONE);
                }
            }
            let pos = IVec3::new(4, 151, 1);
            world.set_block(pos, if mining { Block::GOLD_BLOCK } else { Block::CHEST });
            let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
            a.id = PlayerId(7);
            let mut entities = Entities::new(1);
            entities.spawn(MobKind::Piglin, DVec3::new(1.5, 150.0, 2.5));
            entities.mobs[0].baby = false;
            if mining {
                a.inventory.slots[0] =
                    Some(Stack::new(Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Diamond), 1));
                a.execute(Command::Run(MoveInput::default(), 100, true), &mut world, &mut entities, &[]).unwrap();
                for _ in 0..100 {
                    a.tick(&mut world, &mut entities);
                }
                assert_eq!(world.get_block(pos), Some(Block::AIR));
            } else {
                world.chest_mut(pos).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 1));
                a.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).unwrap();
            }
            let ctx = Ctx {
                spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
                players: vec![Target { gold_armor: true, ..Target::new(a.id, a.player.pos, true) }],
                spawning: false,
                dimension: Dimension::Nether,
                daylight: 0.0,
                raining: false,
            };
            assert!(
                entities
                    .update(0.05, &world, &ctx)
                    .iter()
                    .any(|event| { matches!(event, EntityEvent::PlayerHit { player, .. } if *player == a.id) }),
                "the angry piglin attacks even a player wearing gold; mining={mining}"
            );
        }
    }

    #[test]
    fn furnace_commands_validate_slots_fuels_capacity_xp_and_saved_processing() {
        for (slot, index) in [("input", 0), ("fuel", 1), ("output", 2)] {
            assert!(
                matches!(Command::parse(&format!("furnace take {slot}")).unwrap(), Command::Furnace(true, i) if i == index)
            );
        }
        assert!(Command::parse("furnace put 27").is_err());
        let mut w = world();
        let mut e = Entities::new(1);
        let p = IVec3::new(4, 151, 1);
        w.set_block(p, Block::FURNACE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots[0] = Some(Stack::new(Item::DIAMOND, 3));
        assert!(a.execute(Command::Furnace(false, 1), &mut w, &mut e, &[]).is_err());
        assert!(a.execute(Command::Furnace(false, 2), &mut w, &mut e, &[]).is_err());
        assert!(w.furnace(p).unwrap().fuel.is_none());
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::DIAMOND, 3)));
        a.inventory.slots[0] = Some(Stack::new(Item::RAW_IRON, 2));
        a.execute(Command::Furnace(false, 0), &mut w, &mut e, &[]).unwrap();
        a.inventory.slots[0] = Some(Stack::new(Item::COAL, 1));
        a.execute(Command::Furnace(false, 1), &mut w, &mut e, &[]).unwrap();
        w.tick_furnaces(5.0);
        let save = w.furnaces_to_string();
        let mut restored = world();
        restored.set_block(p, Block::FURNACE);
        restored.load_furnaces(&save);
        restored.tick_furnaces(5.0);
        assert!(restored.furnace(p).unwrap().stored_xp() > 0.0);
        // Whole points, independent of the fraction roll.
        restored.furnace_mut(p).unwrap().recipes_used =
            vec![crate::world::furnace::RecipeUse { recipe: Item::IRON_INGOT, xp: 3.0, count: 1 }];
        a.execute(Command::Furnace(true, 2), &mut restored, &mut e, &[]).unwrap();
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::IRON_INGOT, 1)));
        assert_eq!(restored.furnace(p).unwrap().stored_xp(), 0.0);
        assert!(!e.orbs.is_empty());
        // A full player cannot drain slots or earn stored XP.
        a.inventory.slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
        restored.furnace_mut(p).unwrap().output = Some(Stack::new(Item::IRON_INGOT, 1));
        restored.furnace_mut(p).unwrap().recipes_used =
            vec![crate::world::furnace::RecipeUse { recipe: Item::IRON_INGOT, xp: 4.0, count: 1 }];
        assert!(a.execute(Command::Furnace(true, 2), &mut restored, &mut e, &[]).is_err());
        assert_eq!(restored.furnace(p).unwrap().stored_xp(), 4.0);
        assert_eq!(restored.furnace(p).unwrap().output, Some(Stack::new(Item::IRON_INGOT, 1)));
    }

    #[test]
    fn compasses_lock_onto_lodestones_and_lose_them_when_they_go() {
        use crate::inventory::{stack_from_str, stack_to_string};
        let mut world = world();
        let mut entities = Entities::new(1);
        let pos = IVec3::new(4, 151, 1);
        world.set_block(pos, crate::world::utilities::LODESTONE);
        // `CompassItem.useOn`: one of a stack of three splits off, keeping its name.
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots[0] = Stack::new(Item::COMPASS, 3).with_name("Home");
        a.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).map(|s| (s.count, s.lodestone)), Some((2, None)));
        let locked = *a.inventory.slots.iter().flatten().find(|s| s.lodestone.is_some()).unwrap();
        assert_eq!((locked.count, locked.display_name()), (1, "Home"));
        assert_eq!(locked.lodestone.unwrap().target, Some((pos, Dimension::Overworld)));
        assert!(!locked.stacks_with(&a.inventory.get(0).unwrap()));
        assert_eq!(stack_from_str(&stack_to_string(Some(locked))), Some(Some(locked)));
        // A lone compass becomes the lodestone compass itself.
        let mut b = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        b.inventory.add(Item::COMPASS, 1);
        b.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        let lone = b.inventory.get(0).unwrap();
        assert_eq!((lone.count, lone.display_name()), (1, "lodestone compass"));
        assert!(b.inventory.slots[1..].iter().all(Option::is_none));
        // `LodestoneTracker.tick`: with the lodestone gone it loses its target.
        world.set_block(pos, Block::AIR);
        b.tick_lodestones(&world);
        let lost = b.inventory.get(0).unwrap();
        assert_eq!(lost.lodestone.map(|l| l.target), Some(None));
        assert_eq!(stack_from_str(&stack_to_string(Some(lost))), Some(Some(lost)));
    }

    #[test]
    fn the_bundle_command_takes_the_picked_stack_out() {
        use crate::content::bundle;
        use crate::inventory::components::Payload;
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let held = vec![Stack::new(Item::DIAMOND, 3), Stack::new(Item::STICK, 5), Stack::new(Item::APPLE, 2)];
        a.inventory.slots[4] = Some(Stack::new(bundle::BUNDLE, 1).with_payload(Payload::Bundle(held.clone())));
        assert!(matches!(Command::parse("bundle 4 1"), Ok(Command::Bundle(4, Some(1)))));
        assert!(matches!(Command::parse("bundle 4"), Ok(Command::Bundle(4, None))));
        assert!(Command::parse("bundle 36").is_err() && Command::parse("bundle 4 12").is_err());
        // The picked cell (1, the stack after the front) comes out first...
        a.execute(Command::Bundle(4, Some(1)), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(4).unwrap().bundle_contents(), [held[0], held[2]]);
        assert_eq!(a.inventory.slots[0], Some(held[1]));
        // ...then with no pick, the front one.
        a.execute(Command::Bundle(4, None), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(4).unwrap().bundle_contents(), [held[2]]);
        assert_eq!(a.inventory.slots[1], Some(held[0]));
        // A pick past what the tooltip shows, an empty bundle or no bundle
        // is refused.
        assert!(a.execute(Command::Bundle(4, Some(1)), &mut world, &mut entities, &[]).is_err());
        a.execute(Command::Bundle(4, None), &mut world, &mut entities, &[]).unwrap();
        assert!(a.execute(Command::Bundle(4, None), &mut world, &mut entities, &[]).is_err());
        assert!(a.execute(Command::Bundle(0, None), &mut world, &mut entities, &[]).is_err());
    }

    #[test]
    fn the_sign_command_writes_one_side_of_a_sign_in_reach() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let pos = IVec3::new(4, 151, 1);
        let sign = crate::world::wood::signs::Sign {
            kind: crate::world::wood::Kind::Oak,
            mount: crate::world::wood::signs::Mount::Standing { rotation: 0 },
            wet: false,
        };
        world.set_block(pos, sign.block());
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let command = Command::parse(r#"sign 4 151 1 back ["Hello  there", "", "line 3"]"#).unwrap();
        a.execute(command, &mut world, &mut entities, &[]).unwrap();
        let (_, entity) = world.sign_at(pos).unwrap();
        assert_eq!(entity.side(false).lines, ["Hello  there", "", "line 3", ""].map(String::from));
        assert!(entity.side(true).lines.iter().all(String::is_empty));
        assert!(Command::parse("sign 4 151 1 up []").is_err());
        assert!(Command::parse(r#"sign 4 151 1 front ["1","2","3","4","5"]"#).is_err());
        // Out of reach.
        let mut far = Agent::new(DVec3::new(30.5, 150.0, 1.5));
        assert!(
            far.execute(Command::parse(r#"sign 4 151 1 front ["x"]"#).unwrap(), &mut world, &mut entities, &[])
                .is_err()
        );
    }

    #[test]
    fn shared_chest_transfer_cannot_duplicate_items() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let pos = IVec3::new(4, 151, 1);
        world.set_block(pos, Block::CHEST);
        world.chest_mut(pos).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 1));
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let mut b = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).unwrap();
        assert!(b.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::DIAMOND, 1)));
        assert!(world.chest(pos).unwrap().slots[0].is_none());
        assert!(b.inventory.slots.iter().all(Option::is_none));
        a.execute(Command::Chest(false, 0), &mut world, &mut entities, &[]).unwrap();
        assert!(a.inventory.get(0).is_none());
        assert_eq!(world.chest(pos).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        a.inventory.slots.fill(Some(Stack::new(Item::STICK, 64)));
        assert!(a.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).is_err());
        assert_eq!(world.chest(pos).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
    }

    #[test]
    fn placement_validates_reach_occupancy_and_spends_once() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.add(Block::STONE, 2);
        world.set_block(IVec3::new(4, 151, 1), Block::STONE);
        let blocker = DVec3::new(3.5, 150.0, 1.5);
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[blocker]).is_err());
        assert_eq!(a.inventory.get(0).unwrap().count, 2);
        assert_eq!(a.swings, 0, "failed placements don't swing");
        a.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
        assert_eq!(a.swings, 1);
        assert_eq!(a.events, [Event::Placed(IVec3::new(3, 151, 1), Block::STONE)]);
        assert_eq!(world.get_block(IVec3::new(3, 151, 1)), Some(Block::STONE));
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
    }

    #[test]
    fn controller_and_cli_agents_ride_carts_and_sneak_to_dismount() {
        // Controller seats and CLI agents are agents: the shared use path mounts a rideable
        // cart and sneaking steps out to a cell Java's dismount search accepts.
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let kind = crate::entity::minecart::CartKind::from_item(crate::item::Item::MINECART).unwrap();
        for x in 0..7 {
            for z in 0..4 {
                world.set_block(IVec3::new(x, 150, z), Block::STONE);
            }
        }
        let cart = entities.spawn_cart(kind, DVec3::new(3.5, 151.0, 1.5));
        a.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.session.player.vehicle, Some(cart));
        assert!(entities.cart(cart).unwrap().seat_for(a.session.id).is_some());
        a.input.descend = true;
        a.remaining = 1;
        a.tick(&mut world, &mut entities);
        assert_eq!(a.session.player.vehicle, None);
        assert!(entities.cart(cart).unwrap().seat_for(a.session.id).is_none());
        let off = a.session.player.pos - DVec3::new(3.5, 151.0, 1.5);
        assert!(off.x.hypot(off.z) >= 0.9, "the player steps out beside the cart");
    }

    #[test]
    fn cart_placement_off_rails_or_while_riding_reports_an_error() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.add(crate::item::Item::MINECART, 1);
        world.set_block(IVec3::new(4, 151, 1), Block::STONE);
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err(), "no rail");
        a.player.vehicle = Some(1);
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err(), "riding");
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
        assert_eq!(a.swings, 0);
    }
}

#[cfg(test)]
mod village_agent_tests {
    use super::*;
    use crate::entity::{MobKind, villager::Profession};
    use crate::world::{chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;
    #[test]
    fn agent_trades_targeted_villager_and_cannot_pay_through_walls() {
        let mut saved = rustc_hash::FxHashMap::default();
        saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        let mut w = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        for _ in 0..10000 {
            w.update(a.player.pos);
            if w.is_loaded(IVec3::new(1, 150, 1)) {
                break;
            }
            std::thread::yield_now();
        }
        assert!(w.is_loaded(IVec3::new(1, 150, 1)));
        a.player.yaw = 0.0;
        a.player.pitch = 0.0;
        let mut e = Entities::new(42);
        e.spawn(MobKind::Villager, DVec3::new(3.5, 150.0, 1.5));
        let v = e.mobs[0].villager.as_mut().unwrap();
        v.set_profession(Profession::Farmer);
        let o = v.offers[0].unwrap();
        a.inventory.slots[0] = Some(o.price());
        a.execute(Command::parse("trade").unwrap(), &mut w, &mut e, &[]).unwrap();
        a.execute(Command::parse("trade 1").unwrap(), &mut w, &mut e, &[]).unwrap();
        assert!(a.inventory.slots.iter().flatten().any(|s| s.item == o.output.item && s.count >= o.output.count));
        assert!(!e.orbs.is_empty());
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().offers[0].unwrap().uses, 1);
        w.set_block(IVec3::new(2, 151, 1), Block::STONE);
        assert!(a.execute(Command::parse("trade").unwrap(), &mut w, &mut e, &[]).is_err());
        assert!(Command::parse("trade 11").is_err());
    }

    /// v1.0 gap Track 7f: `Villager.mobInteract`'s `setUnhappy`: a villager
    /// with no offers refuses with its head shake, a child shakes its head
    /// at any use, a wandering trader never does, and `Villager.tick`
    /// counts the 40 ticks down.
    #[test]
    fn refused_trades_and_pestered_children_shake_their_heads() {
        use crate::entity::villager::UNHAPPY_TICKS;
        let mut saved = rustc_hash::FxHashMap::default();
        saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        let mut w = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        for _ in 0..10000 {
            w.update(a.player.pos);
            if w.is_loaded(IVec3::new(1, 150, 1)) {
                break;
            }
            std::thread::yield_now();
        }
        a.player.yaw = 0.0;
        a.player.pitch = 0.0;
        let (eye, dir) = (a.player.eye(), a.player.forward().as_dvec3());
        let mut e = Entities::new(42);
        e.spawn(MobKind::Villager, DVec3::new(3.5, 150.0, 1.5));
        let unhappy = |e: &Entities| e.mobs[0].villager.as_ref().unwrap().unhappy;
        // A jobless adult has no offers: it refuses.
        let id = e.target_merchant(&w, eye, dir, 4.5).expect("the villager in view");
        assert!(!e.pester_child(&w, eye, dir, 4.5));
        e.refuse_trade(id);
        assert_eq!(unhappy(&e), UNHAPPY_TICKS);
        // `Villager.tick` counts it down to the end of the shake.
        let ctx = crate::entity::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        let mut rng = crate::entity::Rng::new(1);
        for _ in 0..UNHAPPY_TICKS {
            e.mobs[0].update(0.05, &w, &ctx, &mut rng, &mut Vec::new());
        }
        assert_eq!(unhappy(&e), 0);
        // A child: no trading screen, a head shake; not while asleep.
        e.mobs[0].age = -24000;
        e.mobs[0].baby = true;
        // Looking down at the child's middle, where it stands now.
        e.mobs[0].pos = DVec3::new(3.5, 150.0, 1.5);
        let dir = (DVec3::new(3.5, 150.4, 1.5) - eye).normalize();
        assert!(e.target_merchant(&w, eye, dir, 4.5).is_none());
        assert!(e.pester_child(&w, eye, dir, 4.5));
        assert_eq!(unhappy(&e), UNHAPPY_TICKS);
        e.mobs[0].villager.as_mut().unwrap().unhappy = 0;
        e.mobs[0].villager.as_mut().unwrap().sleeping = true;
        assert!(!e.pester_child(&w, eye, dir, 4.5));
        assert_eq!(unhappy(&e), 0);
        // A wandering trader with nothing to sell never shakes its head.
        let mut e = Entities::new(43);
        e.spawn(MobKind::WanderingTrader, DVec3::new(3.5, 150.0, 1.5));
        if let Some(id) = e.target_merchant(&w, eye, dir, 4.5) {
            e.refuse_trade(id);
        }
        assert!(e.mobs.iter().all(|m| m.villager.as_ref().is_none_or(|v| v.unhappy == 0)));
    }
}
