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
use crate::simulation::survival;
use crate::simulation::{self, TICK_SECONDS};
use crate::world::{World, block::Block};

pub const HELP: &str = "observe [0..2] | catalog [query] | players | look yaw pitch | move forward right ticks [jump sprint sneak] | wait ticks | mine ticks | eat (or drink) | draw [ticks] (hold use with a bow or crossbow: a bow shoots at that draw, a crossbow loads after 25 ticks; a charged crossbow fires at once) | sleep | place (throws a selected ender pearl, splash potion or eye of ender, or puts the eye in a targeted End portal frame) | attack | select 1..9 | fly on/off | craft item | chest take/put slot | furnace take/put input/fuel/output | enchanting 1..3 (an aimed enchanting table's offer for the held item) | anvil 1..9 (combine the held stack with that hotbar slot on an aimed anvil) | grindstone [1..9] (disenchant or combine held gear) | trade [1..10] (inspect or buy a targeted villager offer) | smithing (upgrade held diamond gear using a template and ingot) | drop | respawn | leave. Cheats: give [@s|@p] item [count], clear, kill, summon mob [x y z], gamemode mode [@s|@p], tp [~] x y z, spawnpoint [x y z], setblock x y z block, time set/add/query, weather clear/rain/thunder, xp|experience add/set/query, effect give/clear, enchant name [level], say message. Host console only: difficulty, gamerule, seed, setworldspawn, locate structure|biome, dimension overworld/nether/end.";

/// Something an agent did that players nearby should hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Broke(IVec3, Block),
    Placed(IVec3, Block),
    Bucket(crate::survival_items::bucket::Effect),
    Chew,
    /// Drew, loaded, fired or shot a bow or crossbow.
    Shot(crate::session::ranged::Shot),
    /// Absorbed an experience orb; with the level-up chime volume when it
    /// reached a multiple of five levels.
    Xp(Option<f32>),
    /// Released an eye of ender from here.
    EyeThrown(DVec3),
    /// Put an eye in the End portal frame here; whether that opened the portal.
    FrameFilled(IVec3, bool),
}

/// Unheard events kept per agent (a host without audio never drains them).
const MAX_EVENTS: usize = 64;

/// Ticks to eat one food item (1.6 s).
pub const EAT_TICKS: u32 = 32;

pub mod command;
pub use command::{Command, Coordinate, EffectChange, PositionSpec, TimeQuery, WeatherKind, XpChange, tab_complete};

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
                // Reuse desktop/controller input-change timer handling.
                let mut cursor = next;
                f.input = None;
                f.click_input(&mut cursor, false);
                f.cook = 0.0;
            }
            1 => f.fuel = next,
            _ => f.output = next,
        }
        if take && slot == 2 {
            let roll = entities.roll();
            if let Some(xp) = world.furnace_mut(pos).unwrap().take_output_xp(before_output, roll) {
                entities.spawn_xp(self.session.player.pos, xp);
            }
        }
        Ok(())
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
                    | Command::Look(..)
                    | Command::Run(..)
                    | Command::Fly(..)
                    | Command::Mode(..)
                    | Command::Teleport(..)
                    | Command::SpawnPoint(..)
                    | Command::Summon(..)
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
                self.session.cancel_consumption();
                self.session.cancel_mining();
            }
            Command::Eat => {
                let duration = self.session.consumption_duration(self.selected)?;
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
                let Some(shot) = self.session.start_draw(self.selected, world, entities) else {
                    return Err("hold a bow or crossbow with an arrow, away from a block that takes use".into());
                };
                self.emit(Event::Shot(shot));
                if shot.ranged == crate::session::ranged::Ranged::Drawing {
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
                    self.session.vitals.damage(f32::MAX, "was killed", false);
                    self.drop_everything(entities);
                    self.remaining = 0;
                }
            }
            Command::Summon(kind, pos) => {
                entities.spawn(kind, pos.resolve(self.session.player.pos)?);
            }
            Command::Xp(change) => {
                if let Some(chime) = change.apply(&mut self.session.vitals.xp) {
                    self.emit(Event::Xp(Some(chime)));
                }
            }
            Command::Effect(change) => {
                let (damage, _) = change.apply(&mut self.session.vitals);
                self.damage(damage, survival::CAUSE_MAGIC);
            }
            Command::Mode(mode) => self.set_mode(mode),
            Command::Teleport(pos) => {
                let pos = pos.resolve(self.session.player.pos)?;
                self.session.player.pos = pos;
                self.previous_pos = pos;
                self.session.player.vel = DVec3::ZERO;
                self.session.vitals.reset_fall();
            }
            Command::SpawnPoint(pos) => {
                let pos = pos.resolve(self.session.player.pos)?.floor().as_ivec3();
                self.session.spawn_bed = None;
                self.session.spawn_point = Some(pos);
            }
            Command::SetBlock(pos, block) => {
                if !world.set_block(pos, block) {
                    return Err("block is unchanged or unloaded".into());
                }
            }
            Command::Place
                if self.session.inventory.get(self.selected).is_some_and(|s| s.item.as_armor().is_some())
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
                    Used::OpenVehicle(_) => return Err("that vehicle has storage; use chest take/put".into()),
                    Used::OpenMerchant(_) => return Err("use trade to see this villager's offers".into()),
                    _ => {}
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
                let sprinting = self.movement_input().sprint && self.session.vitals.may_sprint(self.creative);
                let outcome = self.session.attack(self.selected, world, entities, sprinting)?;
                // Java has no attack cooldown; damage scales with the charge instead.
                self.cooldown = TICK_SECONDS;
                self.swings += 1;
                if let crate::session::combat::Outcome::Missed { .. } = outcome {
                    return Err("nothing to attack within reach".into());
                }
            }
            Command::Craft(item) => self.craft(item, world)?,
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
                if slot >= world.container_slots(pos) {
                    return Err("container slot out of range".into());
                }
                let chest = world.chest_mut(pos).ok_or("target is not a chest")?;
                if take {
                    let stack = chest.slots[slot].ok_or("chest slot empty")?;
                    let left = self.session.inventory.add_stack(stack);
                    chest.slots[slot] = (left > 0).then_some(Stack { count: left, ..stack });
                    if left == stack.count {
                        return Err("inventory full".into());
                    }
                } else {
                    let stack = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
                    let dest = &mut chest.slots[slot];
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
                    self.session.inventory.slots[self.selected] =
                        (stack.count > n).then_some(Stack { count: stack.count - n, ..stack });
                }
                if world.get_block(pos).is_some_and(crate::entity::nether::guarded_by_piglins) {
                    entities.piglins_notice(self.session.id, self.session.player.pos, true, world);
                }
            }
            Command::Enchanting(i) => self.enchant_at_table(i, world)?,
            Command::Anvil(slot) => {
                let (pos, _) = self.target(world).ok_or("no anvil within reach")?;
                if !world.get_block(pos).is_some_and(Block::is_anvil) {
                    return Err("target is not an anvil".into());
                }
                if slot == self.selected {
                    return Err("pick a different slot for the second item".into());
                }
                let left = self.session.inventory.get(self.selected).ok_or("selected slot empty")?;
                let right = self.session.inventory.get(slot);
                let r = crate::enchant::anvil_any_cost(left, right, self.creative).ok_or("those don't combine")?;
                if !self.creative && r.cost >= crate::enchant::TOO_EXPENSIVE {
                    return Err("too expensive".into());
                }
                if !self.creative && self.session.vitals.xp.level < r.cost {
                    return Err(format!("needs level {}", r.cost));
                }
                self.session.inventory.slots[self.selected] = Some(r.output);
                self.session.inventory.slots[slot] = match (r.uses, right) {
                    (Some(n), Some(s)) if s.count > n => Some(Stack { count: s.count - n, ..s }),
                    _ => None,
                };
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
                let template_slot =
                    self.session.inventory.find(Item::NETHERITE_UPGRADE).ok_or("no upgrade template")?;
                let addition_slot = self.session.inventory.find(Item::NETHERITE_INGOT).ok_or("no Netherite ingot")?;
                let result = crate::smithing::upgrade(
                    self.session.inventory.get(template_slot).unwrap(),
                    base,
                    self.session.inventory.get(addition_slot).unwrap(),
                )
                .ok_or("selected item is not diamond gear")?;
                self.session.inventory.take_one(template_slot);
                self.session.inventory.take_one(addition_slot);
                self.session.inventory.slots[self.selected] = Some(result);
            }
            Command::Drop => {
                let stack = self.session.inventory.slots[self.selected].take().ok_or("selected slot empty")?;
                entities.throw(stack, self.session.player.eye(), self.session.player.forward().as_dvec3());
            }
            Command::Respawn => {
                if !self.session.vitals.is_dead() {
                    return Err("player is alive".into());
                }
                let at = self
                    .session
                    .personal_spawn(world)
                    .position
                    .unwrap_or_else(|| world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                self.session.respawn(at);
            }
            Command::Observe(_) | Command::Help | Command::XpQuery | Command::Say(_) => {}
            _ => return Err("command requires host console".into()),
        }
        Ok(())
    }

    fn craft(&mut self, item: Item, world: &World) -> Result<(), String> {
        let at_table = self.at_crafting_table(world);
        let inventory = crafting::recipes()
            .iter()
            .filter(|r| r.result.item == item)
            .find_map(|r| self.crafted(r, at_table))
            .ok_or("missing ingredients, inventory space or targeted crafting table")?;
        self.session.inventory = inventory;
        Ok(())
    }

    fn at_crafting_table(&self, world: &World) -> bool {
        self.target(world).is_some_and(|(p, _)| world.get_block(p).is_some_and(|b| b.base() == Block::CRAFTING_TABLE))
    }

    /// The inventory after crafting `recipe` once, if the ingredients and
    /// room are there (3x3 recipes need a targeted crafting table).
    fn crafted(&self, recipe: &crafting::Recipe, at_table: bool) -> Option<Inventory> {
        let grid = recipe.preview();
        if grid.size == 3 && !at_table {
            return None;
        }
        let mut inventory = self.session.inventory.clone();
        for i in 0..grid.cells.len() {
            if grid.cells[i].is_none() {
                continue;
            }
            let options = recipe.alternatives(i).unwrap();
            let slot = inventory.slots.iter().position(|s| s.is_some_and(|s| options.contains(&s.item)))?;
            let used = inventory.get(slot)?.item;
            inventory.take_one(slot);
            if let Some(rest) = used.remainder()
                && inventory.add(rest, 1) != 0
            {
                return None;
            }
        }
        (inventory.add_stack(recipe.result) == 0).then_some(inventory)
    }

    /// Every result that `craft` would make right now, once each, in recipe
    /// book order.
    pub fn craftable(&self, world: &World) -> Vec<Stack> {
        let at_table = self.at_crafting_table(world);
        let mut out: Vec<Stack> = Vec::new();
        for recipe in crafting::recipes() {
            if !out.iter().any(|s| s.item == recipe.result.item) && self.crafted(recipe, at_table).is_some() {
                out.push(recipe.result);
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
        self.previous_pos = self.session.player.pos;
        self.keep_inventory = rules.bool("keepInventory");
        // Movement alone returning early is not enough: survival, mining,
        // pickups and timed commands must also wait for local terrain.
        let feet = self.session.player.pos.floor().as_ivec3();
        if !world.is_loaded(feet) || !world.is_loaded(feet - IVec3::Y) {
            return;
        }
        // Every agent (CLI, controller and LAN hands) charges here once per tick.
        self.session.tick_attack_strength(self.selected);
        let holding_rod = self.session.inventory.get(self.selected).is_some_and(|s| s.item == Item::FISHING_ROD);
        if self.session.vitals.is_dead() || !holding_rod {
            entities.drop_bobber(self.session.id);
        }
        self.cooldown = (self.cooldown - TICK_SECONDS).max(0.0);
        if self.session.vitals.is_dead() {
            self.session.cancel_consumption();
            self.session.cancel_mining();
            entities.dismount(world, self.session.id);
            self.session.player.vehicle = None;
            self.remaining = 0;
            self.session.sleeping = None;
            return;
        }
        let mut input = self.movement_input();
        input.sprint &= self.session.vitals.may_sprint(self.creative || self.session.mode.invulnerable());
        let before = self.session.player.pos;
        if self.session.player.vehicle.is_some()
            && input.descend
            && let Some(pos) = entities.dismount(world, self.session.id)
        {
            self.session.player.pos = pos;
            self.session.player.vehicle = None;
            self.session.player.vel = DVec3::ZERO;
        }
        if self.session.player.vehicle.is_none() {
            self.session.player.apply_effects(&self.session.vitals.effects);
            self.session.player.wear_boots(crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::DepthStrider,
            ));
            self.session.player.update(TICK_SECONDS, input, world);
            crate::particles::water_entry(&self.session.player, before, world);
        } else {
            entities.mount_input(self.session.id, &self.session.player, input, &self.session.inventory, self.selected);
            entities.seat_player(&mut self.session.player, self.session.id);
        }
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
            ..simulation::player_environment(&self.session.player, world, input, moved)
        };
        let hurts = self.session.vitals.tick_rules(
            TICK_SECONDS as f32,
            &env,
            self.creative || self.session.mode.invulnerable(),
            difficulty,
            rules.bool("naturalRegeneration"),
        );
        for (damage, cause) in [
            (hurts.fall * rules.bool("fallDamage") as u8 as f32, "hit the ground too hard"),
            (hurts.drown * rules.bool("drowningDamage") as u8 as f32, "drowned"),
            (hurts.lava * rules.bool("fireDamage") as u8 as f32, "tried to swim in lava"),
            ((hurts.fire + hurts.burn) * rules.bool("fireDamage") as u8 as f32, "burned to death"),
            (hurts.starve, "starved to death"),
        ] {
            if damage > 0.0 && self.damage(damage, cause) > 0.0 {
                self.session.sleeping = None;
            }
        }
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
        self.draw(entities);
        self.chew(entities);
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
            self.session.cancel_consumption();
            self.remaining = 0;
            return;
        }
        self.remaining = self.remaining.saturating_sub(1);
        entities.items.retain_mut(|item| {
            if item.pickup_delay > 0.0 || !item.touches_player(self.session.player.pos) {
                return true;
            }
            item.stack.count = self.session.inventory.add_stack(item.stack);
            item.stack.count > 0
        });
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
        self.damage(crate::entity::pearl::DAMAGE, "fell from a high place");
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
    }

    /// Advance the shared consumption clock; item-specific duration comes from
    /// the reference content definition. Switching slots restarts the bite.
    fn chew(&mut self, entities: &mut Entities) {
        // Controller/LAN hands drive their session's consumption through the desktop
        // adapter; an idle CLI input must not cancel that other adapter's current tick.
        if !self.eating || self.remaining == 0 {
            self.bite = 0;
            return;
        }
        let before = self.session.consumption_elapsed();
        let result = self.session.tick_consumption(self.selected, true, entities);
        let Ok(step) = result else {
            self.bite = 0;
            self.session.cancel_consumption();
            return;
        };
        self.bite = (f64::from(step.progress) * EAT_TICKS as f64) as u32;
        if ((before / TICK_SECONDS).round() as u32).is_multiple_of(5) {
            self.emit(Event::Chew);
        }
        if let Some(consumed) = step.completed {
            self.damage(consumed.magic_damage, survival::CAUSE_MAGIC);
            if self.remaining <= 1 {
                self.eating = false;
            }
        }
    }

    /// One tick of holding use with a bow or crossbow: a crossbow loads when
    /// its charge completes; a bow shoots when the requested ticks have passed.
    fn draw(&mut self, entities: &mut Entities) {
        if !self.drawing {
            return;
        }
        if self.remaining == 0 || self.session.vitals.is_dead() {
            self.drawing = false;
            self.session.cancel_draw();
            return;
        }
        if let Some(shot) = self.session.tick_draw(self.selected, true) {
            self.emit(Event::Shot(shot));
        }
        if !self.session.drawing() {
            // Loaded, or the draw ended (the slot changed, the arrows ran out).
            self.drawing = false;
        } else if self.remaining <= 1 {
            self.drawing = false;
            if let Some(shot) = self.session.release_draw(self.selected, entities) {
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
        if !self.creative {
            self.session.vitals.xp.add_levels(-(i as i64 + 1));
        }
        self.session.inventory = inv;
        self.session.vitals.xp.seed = (crate::enchant::roll() * u32::MAX as f32) as u32 as i32;
        Ok(())
    }

    /// Damage through protection enchantments (armor points aside).
    fn damage(&mut self, amount: f32, cause: &str) -> f32 {
        let amount = crate::enchant::protect(amount, &self.session.inventory.armor, cause);
        let taken = self.session.vitals.damage(amount, cause, self.creative || self.session.mode.invulnerable());
        if taken > 0.0 {
            self.session.player.animation.hurt_direction = 0.0;
        }
        taken
    }

    /// Armored damage from a mob, arrow or explosion. Knockback only lands
    /// with damage (hurt immunity also stops repeated shoves), and a survival
    /// agent killed this way drops everything. Returns the damage taken.
    pub fn hurt(&mut self, amount: f32, cause: &str, knockback: DVec3, entities: &mut Entities) -> f32 {
        let inv = &self.session.inventory;
        let reduced = simulation::survival::armor_reduce(amount, inv.armor_points(), inv.armor_toughness());
        self.hurt_after_armor(reduced, amount, cause, knockback, entities)
    }

    /// Magic damage from something consumed (a harming potion): no armor or
    /// knockback, with protection and death/drop rules.
    pub fn hurt_magic(&mut self, amount: f32, entities: &mut Entities) -> f32 {
        self.hurt_after_armor(amount, 0.0, simulation::survival::CAUSE_MAGIC, DVec3::ZERO, entities)
    }

    /// Mounted falls bypass armor points, retaining protection and death/drop rules.
    pub fn hurt_fall(&mut self, amount: f32, entities: &mut Entities) {
        self.damage(amount, simulation::survival::CAUSE_FALL);
        if self.session.vitals.is_dead() {
            self.drop_everything(entities);
        }
    }

    pub fn hurt_beam(&mut self, physical: f32, magic: f32, entities: &mut Entities) -> f32 {
        let inv = &self.session.inventory;
        let reduced = simulation::survival::armor_reduce(physical, inv.armor_points(), inv.armor_toughness());
        self.hurt_after_armor(reduced + magic, physical, "was slain by a guardian", DVec3::ZERO, entities)
    }

    fn hurt_after_armor(
        &mut self,
        reduced: f32,
        wear: f32,
        cause: &str,
        knockback: DVec3,
        entities: &mut Entities,
    ) -> f32 {
        let taken = self.damage(reduced, cause);
        if taken <= 0.0 {
            return 0.0;
        }
        self.session.player.hurt_from(knockback);
        self.session.player.vel +=
            simulation::survival::knockback_taken(knockback, self.session.inventory.knockback_resistance());
        self.session.inventory.wear_armor(wear);
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
            json!({"position":p.to_array(),"face":n.to_array(),"block":world.get_block(p).map(|b|b.name()),"enchanting_offers":offers})
        });
        json!({"position":self.session.player.pos.to_array(),"yaw":self.session.player.yaw.to_degrees(),"pitch":self.session.player.pitch.to_degrees(),"loaded":world.is_loaded(center),"dimension":world.generator.dimension.name(),"health":self.session.vitals.health,"food":self.session.vitals.hunger.food,"level":self.session.vitals.xp.level,"xp_progress":self.session.vitals.xp.progress(),"dead":self.session.vitals.is_dead(),"mode":self.session.mode.name(),"creative":self.creative,"flying":self.session.player.flying,"sleeping":self.session.sleeping.is_some(),"spawn_bed":self.session.spawn_bed.map(|p|p.to_array()),"spawn_point":self.session.spawn_point.map(|p|p.to_array()),"selected":self.selected+1,"inventory":inventory,"target":target,"blocks":blocks})
    }
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
    fn depth_strider_bonus_is_halved_off_the_ground() {
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        world.set_block(IVec3::new(1, 150, 1), Block::WATER);
        let speed = |world: &World, grounded, level| {
            let mut p = Player::new(DVec3::new(1.5, 150.0, 1.5));
            p.on_ground = grounded;
            p.wear_boots(level);
            p.update(1.0 / 120.0, MoveInput { forward: 1.0, ..Default::default() }, world);
            p.vel.x
        };
        let grounded_bonus = speed(&world, true, 3) - speed(&world, true, 0);
        let airborne_bonus = speed(&world, false, 3) - speed(&world, false, 0);
        assert!(grounded_bonus > 0.0);
        assert!((airborne_bonus * 2.0 - grounded_bonus).abs() < 1e-9);
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
        assert_eq!(bare.vitals.death.as_deref(), Some("was blown up by a creeper"));
        assert!(bare.inventory.get(0).is_none());
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 2);
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
        rules.set("keepInventory", "true").unwrap();
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
        world.set_block(bed - IVec3::Y, Block::STONE);
        world.set_block(bed, Block::BED_FOOT);
        let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        agent.spawn_bed = Some(bed);
        let mut die_and_respawn = |agent: &mut Agent, world: &mut World| {
            agent.vitals.damage(100.0, "test", false);
            agent.execute(Command::Respawn, world, &mut entities, &[]).unwrap();
        };
        die_and_respawn(&mut agent, &mut world);
        assert_eq!(agent.player.pos, DVec3::new(1.5, 150.0 + Block::BED_FOOT.height(), 1.5));
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
            Command::Effect(EffectChange::Give(Effect::Strength, 90, 1))
        ));
        assert!(matches!(
            Command::parse("effect give regeneration").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Regeneration, 30, 0))
        ));
        assert!(matches!(Command::parse("effect clear").unwrap(), Command::Effect(EffectChange::Clear(None))));
        let mut vitals = Vitals::default();
        vitals.health = 10.0;
        assert_eq!(EffectChange::Give(Effect::InstantDamage, 1, 0).apply(&mut vitals).0, 6.0);
        EffectChange::Give(Effect::InstantHealth, 1, 1).apply(&mut vitals);
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
        assert!(a.player.pos.x > 8.0);
        assert_eq!(a.remaining, 0);
        assert_eq!(b.player.pos.x, 4.5);
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
        assert!(restored.furnace(p).unwrap().xp > 0.0);
        restored.furnace_mut(p).unwrap().xp = 3.0; // whole points, independent of the fraction roll
        a.execute(Command::Furnace(true, 2), &mut restored, &mut e, &[]).unwrap();
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::IRON_INGOT, 1)));
        assert_eq!(restored.furnace(p).unwrap().xp, 0.0);
        assert!(!e.orbs.is_empty());
        // A full player cannot drain slots or earn stored XP.
        a.inventory.slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
        restored.furnace_mut(p).unwrap().output = Some(Stack::new(Item::IRON_INGOT, 1));
        restored.furnace_mut(p).unwrap().xp = 4.0;
        assert!(a.execute(Command::Furnace(true, 2), &mut restored, &mut e, &[]).is_err());
        assert_eq!(restored.furnace(p).unwrap().xp, 4.0);
        assert_eq!(restored.furnace(p).unwrap().output, Some(Stack::new(Item::IRON_INGOT, 1)));
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
}
