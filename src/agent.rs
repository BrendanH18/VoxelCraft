//! Device-independent player commands and authoritative agent sessions.
//! Agents submit input, never positions or inventory edits (unless cheats are enabled).

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use crate::crafting;
use crate::entity::Entities;
use crate::inventory::{Inventory, Stack};
use crate::item::Item;
use crate::mining;
use crate::player::{MoveInput, Player};
use crate::simulation::effects::Effect;
use crate::simulation::survival::{self, Vitals};
use crate::simulation::{self, TICK_SECONDS};
use crate::world::{
    World,
    block::{Block, RenderKind},
    terrain::Dimension,
};

pub const HELP: &str = "observe [0..2] | catalog [query] | players | look yaw pitch | move forward right ticks [jump sprint sneak] | wait ticks | mine ticks | eat (or drink) | sleep | place (throws a selected ender pearl or eye of ender, or puts the eye in a targeted End portal frame) | attack | select 1..9 | fly on/off | craft item | chest take/put slot | enchanting 1..3 (an aimed enchanting table's offer for the held item) | anvil 1..9 (combine the held stack with that hotbar slot on an aimed anvil) | drop | respawn | leave. Cheats: give item [count], gamemode creative/survival, tp x y z, setblock x y z block, time day/noon/night/0..1, weather clear/rain, xp add/set n [points/levels], xp query, effect give effect [seconds] [amplifier], effect clear [effect], enchant name [level], dimension overworld/nether/end (host console only).";

/// Something an agent did that players nearby should hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Broke(IVec3, Block),
    Placed(IVec3, Block),
    Chew,
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

pub enum Command {
    Help,
    Observe(i32),
    Players,
    Catalog(String),
    Look(f32, f32),
    Run(MoveInput, u32, bool),
    Eat,
    Sleep,
    Place,
    Attack,
    Select(usize),
    Fly(bool),
    Craft(Item),
    Chest(bool, usize),
    Drop,
    Respawn,
    Leave,
    Give(Item, u8),
    Mode(bool),
    Teleport(DVec3),
    SetBlock(IVec3, Block),
    Time(f64),
    Weather(bool),
    Dimension(Dimension),
    /// Java's `/xp`: add or set an amount, counted in levels or points.
    Xp(XpChange),
    XpQuery,
    /// Java's `/effect`.
    Effect(EffectChange),
    /// Java's `/enchant`: enchants the held item.
    Enchant(crate::enchant::Enchantment, u8),
    /// Takes offer 0..3 of the targeted enchanting table for the held item,
    /// paying levels and lapis from the inventory.
    Enchanting(usize),
    /// Combines the held stack with hotbar slot 0..9 on the targeted anvil.
    Anvil(usize),
}

/// `/effect give` (with seconds and amplifier) or `/effect clear`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectChange {
    Give(Effect, u32, u8),
    /// Removes one effect, or all of them.
    Clear(Option<Effect>),
}

impl EffectChange {
    /// Applies the change; returns instant damage for the caller to deal
    /// and the feedback line.
    pub fn apply(self, vitals: &mut Vitals) -> (f32, String) {
        match self {
            EffectChange::Give(effect, secs, amp) => {
                let damage = vitals.apply_effect(effect, amp, secs.saturating_mul(20).max(1));
                (damage, format!("applied {} {}", effect.name(), crate::simulation::effects::level_name(amp)))
            }
            EffectChange::Clear(Some(effect)) => {
                let had = vitals.effects.remove(effect);
                (0.0, if had { format!("removed {}", effect.name()) } else { format!("no {}", effect.name()) })
            }
            EffectChange::Clear(None) => {
                vitals.effects.clear();
                (0.0, "removed every effect".into())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XpChange {
    pub set: bool,
    pub amount: i64,
    pub levels: bool,
}

impl XpChange {
    /// Applies the change, returning the level-up chime volume if any.
    pub fn apply(self, xp: &mut crate::simulation::experience::Experience) -> Option<f32> {
        match (self.set, self.levels) {
            (false, false) => xp.add_points(self.amount),
            (false, true) => xp.add_levels(self.amount),
            (true, true) => xp.add_levels(self.amount - xp.level as i64),
            (true, false) => {
                // Java only sets points within the current level.
                let cap = crate::simulation::experience::points_to_next(xp.level) as i64;
                xp.points = self.amount.clamp(0, cap - 1) as u32;
                None
            }
        }
    }
}

fn number(text: &str) -> Result<f64, String> {
    text.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| format!("invalid finite number: {text}"))
}

fn ticks(text: &str) -> Result<u32, String> {
    text.parse::<u32>().ok().filter(|n| (1..=200).contains(n)).ok_or_else(|| "ticks must be 1..200".into())
}

impl Command {
    /// Parse slash commands and CLI input identically, rejecting extra arguments and non-finite numbers.
    pub fn parse(text: &str) -> Result<Self, String> {
        let words: Vec<_> = text.trim().trim_start_matches('/').split_whitespace().collect();
        let bad = || "unknown command or arguments; use /help".to_string();
        let item = |name: &str| Item::from_name(name).ok_or_else(|| format!("unknown item: {name}"));
        let xyz = |x: &str, y: &str, z: &str| -> Result<DVec3, String> {
            let v = DVec3::new(number(x)?, number(y)?, number(z)?);
            if v.abs().max_element() > 30_000_000.0 {
                return Err("coordinates exceed world bounds".into());
            }
            Ok(v)
        };
        Ok(match words.as_slice() {
            ["help"] => Self::Help,
            ["observe"] => Self::Observe(0),
            ["observe", r] => Self::Observe(r.parse::<i32>().ok().filter(|r| (0..=2).contains(r)).ok_or_else(bad)?),
            ["players"] => Self::Players,
            ["catalog", query @ ..] => Self::Catalog(query.join(" ")),
            ["look", yaw, pitch] => {
                Self::Look(number(yaw)?.rem_euclid(360.0) as f32, number(pitch)?.clamp(-89.0, 89.0) as f32)
            }
            ["wait", n] => Self::Run(MoveInput::default(), ticks(n)?, false),
            ["mine", n] => Self::Run(MoveInput::default(), ticks(n)?, true),
            ["move", f, r, n, flags @ ..] => {
                if flags.iter().any(|f| !matches!(*f, "jump" | "sprint" | "sneak")) {
                    return Err(bad());
                }
                let forward = number(f)?;
                let right = number(r)?;
                if forward.abs() > 1.0 || right.abs() > 1.0 {
                    return Err("movement axes must be -1..1".into());
                }
                Self::Run(
                    MoveInput {
                        forward,
                        right,
                        jump: flags.contains(&"jump"),
                        sprint: flags.contains(&"sprint"),
                        descend: flags.contains(&"sneak"),
                    },
                    ticks(n)?,
                    false,
                )
            }
            ["eat"] => Self::Eat,
            ["sleep"] => Self::Sleep,
            ["place"] => Self::Place,
            ["attack"] => Self::Attack,
            ["select", n] => Self::Select(n.parse::<usize>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(bad)? - 1),
            ["fly", "on"] => Self::Fly(true),
            ["fly", "off"] => Self::Fly(false),
            ["craft", name] => Self::Craft(item(name)?),
            ["chest", action @ ("take" | "put"), slot] => {
                Self::Chest(*action == "take", slot.parse::<usize>().ok().filter(|n| *n < 27).ok_or_else(bad)?)
            }
            ["drop"] => Self::Drop,
            ["respawn"] => Self::Respawn,
            ["leave"] => Self::Leave,
            ["give", name] => Self::Give(item(name)?, 1),
            ["give", name, n] => Self::Give(item(name)?, n.parse::<u8>().ok().filter(|n| *n > 0).ok_or_else(bad)?),
            ["gamemode", "creative"] => Self::Mode(true),
            ["gamemode", "survival"] => Self::Mode(false),
            ["tp", x, y, z] => Self::Teleport(xyz(x, y, z)?),
            ["setblock", x, y, z, name] => {
                Self::SetBlock(xyz(x, y, z)?.floor().as_ivec3(), Block::from_name(name).ok_or_else(bad)?)
            }
            ["time", "day"] => Self::Time(0.0),
            ["time", "noon"] => Self::Time(0.25),
            ["time", "night"] => Self::Time(0.75),
            ["time", n] => Self::Time(number(n)?.rem_euclid(1.0)),
            ["weather", "clear"] => Self::Weather(false),
            ["weather", "rain"] => Self::Weather(true),
            ["dimension", name] => Self::Dimension(Dimension::from_name(name).ok_or_else(bad)?),
            ["xp", "query"] => Self::XpQuery,
            ["xp", op @ ("add" | "set"), n, unit @ ..] => {
                let amount = n.parse::<i64>().ok().filter(|n| n.abs() <= 1_000_000).ok_or_else(bad)?;
                let levels = match unit {
                    [] | ["points"] => false,
                    ["levels"] => true,
                    _ => return Err(bad()),
                };
                if *op == "set" && amount < 0 {
                    return Err(bad());
                }
                Self::Xp(XpChange { set: *op == "set", amount, levels })
            }
            ["effect", "clear"] => Self::Effect(EffectChange::Clear(None)),
            ["effect", "clear", name] => {
                Self::Effect(EffectChange::Clear(Some(Effect::from_id(name).ok_or_else(bad)?)))
            }
            ["effect", "give", name, rest @ ..] if rest.len() <= 2 => {
                let effect = Effect::from_id(name).ok_or_else(bad)?;
                // Java's defaults: 30 seconds, level I.
                let secs = rest.first().map_or(Ok(30), |n| {
                    n.parse::<u32>().ok().filter(|n| (1..=1_000_000).contains(n)).ok_or_else(bad)
                })?;
                let amp = rest.get(1).map_or(Ok(0), |n| n.parse::<u8>().map_err(|_| bad()))?;
                Self::Effect(EffectChange::Give(effect, secs, amp))
            }
            ["anvil", n] => Self::Anvil(n.parse::<usize>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(bad)? - 1),
            ["enchanting", n] => {
                Self::Enchanting(n.parse::<usize>().ok().filter(|n| (1..=3).contains(n)).ok_or_else(bad)? - 1)
            }
            ["enchant", name, rest @ ..] if rest.len() <= 1 => {
                let e = crate::enchant::Enchantment::from_name(name).ok_or_else(bad)?;
                let level = rest.first().map_or(Ok(1), |n| n.parse::<u8>().ok().filter(|&n| n > 0).ok_or_else(bad))?;
                Self::Enchant(e, level)
            }
            _ => return Err(bad()),
        })
    }

    /// Whether executing this command requires cheats to be enabled.
    pub fn cheat(&self) -> bool {
        matches!(
            self,
            Self::Give(..)
                | Self::Mode(..)
                | Self::Teleport(..)
                | Self::SetBlock(..)
                | Self::Time(..)
                | Self::Weather(..)
                | Self::Dimension(..)
                | Self::Xp(..)
                | Self::Effect(..)
                | Self::Enchant(..)
        )
    }
}

pub struct Agent {
    pub player: Player,
    pub previous_pos: DVec3,
    pub inventory: Inventory,
    /// Transient enchanting table / anvil inputs for a controller player.
    /// Saved as returned inventory and dropped on death with the other gear.
    pub work: [Option<Stack>; 2],
    pub vitals: Vitals,
    /// The host's stable ID for this player (owner of its thrown pearls).
    pub id: crate::entity::PlayerId,
    pub creative: bool,
    pub selected: usize,
    pub remaining: u32,
    /// Arm swings so far (placing, attacking, mining), for animation.
    pub swings: u32,
    /// Sounds to play, drained by the host.
    pub events: Vec<Event>,
    /// Seconds in bed (the host fades the view and skips the night once
    /// everyone has slept long enough). Acting or being hurt gets up.
    pub sleeping: Option<f32>,
    /// Foot of the Overworld bed last used, where a dead agent respawns.
    pub spawn_bed: Option<IVec3>,
    input: MoveInput,
    mining: bool,
    /// Holding "use" eats held food; `bite` counts the ticks chewed.
    eating: bool,
    bite: u32,
    /// Accumulated fraction broken, using the speed at each tick.
    breaking: Option<(IVec3, f64)>,
    cooldown: f64,
}

impl Agent {
    /// Creates an idle survival player with empty inventory at `pos`.
    pub fn new(pos: DVec3) -> Self {
        Self {
            player: Player::new(pos),
            previous_pos: pos,
            inventory: Inventory::default(),
            work: [None; 2],
            vitals: Vitals::default(),
            id: crate::entity::PlayerId::default(),
            creative: false,
            selected: 0,
            remaining: 0,
            swings: 0,
            events: Vec::new(),
            sleeping: None,
            spawn_bed: None,
            input: MoveInput::default(),
            mining: false,
            eating: false,
            bite: 0,
            breaking: None,
            cooldown: 0.0,
        }
    }

    pub fn target(&self, world: &World) -> Option<(IVec3, IVec3)> {
        world.raycast(self.player.eye(), self.player.forward().as_dvec3(), 6.0)
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
        if self.vitals.is_dead() && !matches!(command, Command::Respawn | Command::Observe(_) | Command::Help) {
            return Err("player is dead; respawn first".into());
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
            self.sleeping = None;
        }
        match command {
            Command::Run(input, n, mine) => {
                if mine
                    && self
                        .target(world)
                        .is_some_and(|(p, _)| world.get_block(p).is_some_and(|b| b.is_door() || b.is_bed()))
                {
                    return Err("multi-cell blocks require the desktop mining action".into());
                }
                self.input = input;
                self.remaining = n;
                self.mining = mine;
                self.eating = false;
                self.breaking = None;
            }
            Command::Eat => {
                let held = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                if held.item.as_potion().is_none() {
                    held.item.food().ok_or("selected item is not food or a potion")?;
                    if self.creative || !self.vitals.hunger.can_eat() {
                        return Err("not hungry".into());
                    }
                }
                self.input = MoveInput::default();
                self.remaining = EAT_TICKS;
                self.mining = false;
                self.eating = true;
                self.bite = 0;
            }
            Command::Look(yaw, pitch) => {
                self.player.yaw = yaw.to_radians();
                self.player.pitch = pitch.to_radians();
                self.breaking = None;
            }
            Command::Select(slot) => {
                self.selected = slot;
                self.breaking = None;
                self.bite = 0;
            }
            Command::Fly(on) => {
                if on && !self.creative {
                    return Err("flight requires creative mode".into());
                }
                self.player.flying = on;
                self.player.vel = DVec3::ZERO;
            }
            Command::Enchant(e, level) => {
                let slot = &mut self.inventory.slots[self.selected];
                *slot = Some(crate::enchant::command(*slot, e, level)?);
            }
            Command::Give(item, count) => {
                let mut inv = self.inventory.clone();
                if inv.add(item, count) > 0 {
                    return Err("inventory full".into());
                }
                self.inventory = inv;
            }
            Command::Xp(change) => {
                if let Some(chime) = change.apply(&mut self.vitals.xp) {
                    self.emit(Event::Xp(Some(chime)));
                }
            }
            Command::Effect(change) => {
                let (damage, _) = change.apply(&mut self.vitals);
                self.damage(damage, survival::CAUSE_MAGIC);
            }
            Command::Mode(creative) => {
                self.creative = creative;
                self.player.can_fly = creative;
                if !creative {
                    self.player.flying = false;
                }
            }
            Command::Teleport(pos) => {
                self.player.pos = pos;
                self.previous_pos = pos;
                self.player.vel = DVec3::ZERO;
                self.vitals.reset_fall();
            }
            Command::SetBlock(pos, block) => {
                if !world.set_block(pos, block) {
                    return Err("block is unchanged or unloaded".into());
                }
            }
            Command::Place if self.inventory.get(self.selected).is_some_and(|s| s.item == Item::ENDER_PEARL) => {
                if self.vitals.pearl_cooldown > 0.0 {
                    return Err("ender pearl cooling down".into());
                }
                self.vitals.pearl_cooldown = crate::entity::pearl::COOLDOWN;
                let p = &self.player;
                let carry = if p.on_ground { p.vel.with_y(0.0) } else { p.vel };
                entities.throw_pearl(self.id, p.eye(), p.forward().as_dvec3(), carry);
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Place if self.inventory.get(self.selected).is_some_and(|s| s.item == Item::EYE_OF_ENDER) => {
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let frame = self
                    .target(world)
                    .map(|(pos, _)| pos)
                    .filter(|&pos| world.get_block(pos).is_some_and(|b| b.base() == Block::END_PORTAL_FRAME));
                if let Some(pos) = frame {
                    let opened =
                        world.insert_eye(pos).ok_or("that frame already holds an eye, or its ring isn't loaded")?;
                    self.emit(Event::FrameFilled(pos, opened));
                } else {
                    if world.generator.dimension != crate::world::terrain::Dimension::Overworld {
                        return Err("eyes of ender only find strongholds in the overworld".into());
                    }
                    let from = self.player.pos + DVec3::Y * (crate::player::SHAPE.height * 0.5);
                    let target = world.generator.strongholds.nearest(from.floor().as_ivec3()).ok_or("no stronghold")?;
                    entities.release_eye(from, target.as_dvec3());
                    self.emit(Event::EyeThrown(from));
                }
                self.cooldown = 0.22;
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Place => {
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let (pos, normal) = self.target(world).ok_or("no block within reach")?;
                let held = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                let block = held.item.block().ok_or("selected item is not a block")?;
                // Complex multi-cell placements use client gameplay until the shared action boundary is extracted.
                if block.is_door()
                    || block.is_bed()
                    || block.is_ladder()
                    || block.kind() == RenderKind::Invisible
                    || block.is_fire()
                {
                    return Err("this block requires the desktop placement action".into());
                }
                let at = pos + normal;
                if !world.get_block(at).is_some_and(|b| b == Block::AIR || b.is_water() || b.is_lava()) {
                    return Err("destination occupied or unloaded".into());
                }
                if self.player.intersects_block(at) || others.iter().any(|&p| Player::new(p).intersects_block(at)) {
                    return Err("placement intersects a player".into());
                }
                if !world.set_block(at, block) {
                    return Err("placement failed".into());
                }
                self.emit(Event::Placed(at, block));
                self.cooldown = 0.22;
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Attack => {
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let distance = self
                    .target(world)
                    .map_or(6.0, |(p, _)| self.player.eye().distance(p.as_dvec3() + DVec3::splat(0.5)).min(6.0));
                let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
                let stack = self.inventory.get(self.selected);
                let held = stack.map(|s| s.item);
                let bonus = self.vitals.effects.attack_bonus();
                if let Some((hit, _)) = entities.fight_raycast(eye, dir, distance) {
                    let enchants = stack.map_or(Default::default(), |s| s.active_enchants());
                    let damage = (mining::attack_damage(held) + bonus).max(0.0)
                        + crate::enchant::damage_bonus(enchants, crate::enchant::Creature::Other);
                    entities.strike(hit, damage, self.id);
                } else {
                    let (i, _) = entities.raycast(eye, dir, distance).ok_or("no mob within reach")?;
                    let sweep = self.player.on_ground.then_some(self.player.pos);
                    entities.melee(i, dir, stack, bonus, false, sweep);
                }
                if !self.creative
                    && let Some(held) = held
                {
                    self.inventory.wear(self.selected, mining::wear(held, true));
                }
                self.cooldown = crate::entity::ATTACK_COOLDOWN;
                self.swings += 1;
            }
            Command::Craft(item) => self.craft(item, world)?,
            Command::Chest(take, slot) => {
                let (pos, _) = self.target(world).ok_or("no chest within reach")?;
                let chest = world.chest_mut(pos).ok_or("target is not a chest")?;
                if take {
                    let stack = chest.slots[slot].ok_or("chest slot empty")?;
                    let left = self.inventory.add_stack(stack);
                    chest.slots[slot] = (left > 0).then_some(Stack { count: left, ..stack });
                    if left == stack.count {
                        return Err("inventory full".into());
                    }
                } else {
                    let stack = self.inventory.get(self.selected).ok_or("selected slot empty")?;
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
                    self.inventory.slots[self.selected] =
                        (stack.count > n).then_some(Stack { count: stack.count - n, ..stack });
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
                let left = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                let right = self.inventory.get(slot);
                let r = crate::enchant::anvil_any_cost(left, right, self.creative).ok_or("those don't combine")?;
                if !self.creative && r.cost >= crate::enchant::TOO_EXPENSIVE {
                    return Err("too expensive".into());
                }
                if !self.creative && self.vitals.xp.level < r.cost {
                    return Err(format!("needs level {}", r.cost));
                }
                self.inventory.slots[self.selected] = Some(r.output);
                self.inventory.slots[slot] = match (r.uses, right) {
                    (Some(n), Some(s)) if s.count > n => Some(Stack { count: s.count - n, ..s }),
                    _ => None,
                };
                if !self.creative {
                    self.vitals.xp.add_levels(-(r.cost as i64));
                    crate::enchant::wear_anvil(world, pos);
                }
            }
            Command::Drop => {
                let stack = self.inventory.slots[self.selected].take().ok_or("selected slot empty")?;
                entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
            }
            Command::Respawn => {
                if !self.vitals.is_dead() {
                    return Err("player is alive".into());
                }
                let overworld = world.generator.dimension == Dimension::Overworld;
                let bed = self.spawn_bed.filter(|&b| overworld && world.get_block(b) == Some(Block::BED_FOOT));
                if overworld && bed.is_none() {
                    self.spawn_bed = None; // broken: forget it
                }
                let at = match bed {
                    Some(b) => DVec3::new(b.x as f64 + 0.5, b.y as f64 + Block::BED_FOOT.height(), b.z as f64 + 0.5),
                    None => world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5),
                };
                self.player = Player::new(at);
                self.player.can_fly = self.creative;
                self.vitals = Vitals::default();
            }
            Command::Observe(_) | Command::Help | Command::XpQuery => {}
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
        self.inventory = inventory;
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
        let mut inventory = self.inventory.clone();
        for i in 0..grid.cells.len() {
            if grid.cells[i].is_none() {
                continue;
            }
            let options = recipe.alternatives(i).unwrap();
            let slot = inventory.slots.iter().position(|s| s.is_some_and(|s| options.contains(&s.item)))?;
            inventory.take_one(slot);
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
        self.previous_pos = self.player.pos;
        // Movement alone returning early is not enough: survival, mining,
        // pickups and timed commands must also wait for local terrain.
        let feet = self.player.pos.floor().as_ivec3();
        if !world.is_loaded(feet) || !world.is_loaded(feet - IVec3::Y) {
            return;
        }
        self.cooldown = (self.cooldown - TICK_SECONDS).max(0.0);
        if self.vitals.is_dead() {
            self.remaining = 0;
            self.sleeping = None;
            return;
        }
        let mut input = if self.remaining > 0 { self.input } else { MoveInput::default() };
        input.sprint &= self.creative || self.vitals.hunger.can_sprint();
        let hurts = simulation::tick_player(
            &mut self.player,
            world,
            &mut self.vitals,
            &self.inventory.armor,
            input,
            self.creative,
        )
        .hurts;
        for (damage, cause) in [
            (hurts.fall, "hit the ground too hard"),
            (hurts.drown, "drowned"),
            (hurts.lava, "tried to swim in lava"),
            (hurts.fire + hurts.burn, "burned to death"),
            (hurts.starve, "starved to death"),
        ] {
            if damage > 0.0 && self.damage(damage, cause) > 0.0 {
                self.sleeping = None;
            }
        }
        if self.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
            return;
        }
        if self.remaining > 0
            && self.mining
            && self.cooldown <= 0.0
            && let Some((pos, _)) = self.target(world)
            && let Some(block) = world.get_block(pos)
        {
            let held = self.inventory.get(self.selected).map(|s| s.item);
            let digger = self.digger(world);
            let progress = self.breaking.filter(|(p, _)| *p == pos).map_or(0.0, |(_, n)| n)
                + TICK_SECONDS / mining::dig_time(block, digger).max(1e-3) as f64;
            self.breaking = Some((pos, progress));
            self.swings += 1;
            if block != Block::BEDROCK && !block.is_door() && !block.is_bed() && (self.creative || progress >= 1.0) {
                world.set_block(pos, Block::AIR);
                self.emit(Event::Broke(pos, block));
                if !self.creative {
                    if mining::can_harvest(block, held) {
                        let tool = digger.held.map_or(Default::default(), |s| s.active_enchants());
                        world.spill_mined(pos, block, tool);
                        entities.drop_mined_xp(block, pos, tool);
                    }
                    if let Some(held) = held {
                        self.inventory.wear(self.selected, mining::wear(held, false));
                    }
                    self.vitals.hunger.exhaust(simulation::survival::EXHAUST_MINE);
                }
                self.breaking = None;
                self.cooldown = 0.15;
            }
        } else {
            self.breaking = None;
        }
        self.chew();
        self.remaining = self.remaining.saturating_sub(1);
        entities.items.retain_mut(|item| {
            if item.pickup_delay > 0.0 || !item.touches_player(self.player.pos) {
                return true;
            }
            item.stack.count = self.inventory.add_stack(item.stack);
            item.stack.count > 0
        });
        if let Some(chime) = crate::entity::orb::absorb(
            &mut entities.orbs,
            self.player.pos,
            &mut self.vitals.xp,
            &mut self.inventory,
            self.selected,
        ) {
            self.emit(Event::Xp(chime));
        }
    }

    /// Arrives where this agent's ender pearl landed, taking the landing's
    /// 5 damage. Returns whether it teleported (dead or asleep agents don't).
    pub fn pearl_teleport(&mut self, pos: DVec3, entities: &mut Entities) -> bool {
        if self.vitals.is_dead() || self.sleeping.is_some() {
            return false;
        }
        self.player.pos = pos;
        self.previous_pos = pos;
        self.player.vel = DVec3::ZERO;
        self.vitals.reset_fall();
        self.damage(crate::entity::pearl::DAMAGE, "fell from a high place");
        if self.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
        }
        true
    }

    /// A dead survival agent's inventory and some of its experience spill
    /// where it died.
    fn drop_everything(&mut self, entities: &mut Entities) {
        let stacks = self.inventory.take_all().into_iter().chain(self.work.iter_mut().filter_map(Option::take));
        for stack in stacks.filter(|s| !s.active_enchants().has(crate::enchant::Enchantment::VanishingCurse)) {
            entities.scatter(stack, self.player.pos);
        }
        let xp = self.vitals.xp.die();
        entities.spawn_xp(self.player.pos, xp);
    }

    /// One tick of eating: a bite finishes after [`EAT_TICKS`] of holding
    /// the same food, like Java. Switching slots restarts it via `select`.
    fn chew(&mut self) {
        let held = self.inventory.get(self.selected).map(|s| s.item);
        let potion = held.and_then(Item::as_potion);
        let food = held.and_then(|i| i.food()).filter(|_| !self.creative && self.vitals.hunger.can_eat());
        if self.remaining == 0 || !self.eating || (potion.is_none() && food.is_none()) {
            self.bite = 0;
            return;
        }
        self.bite += 1;
        // Chewing sounds four times a second.
        if self.bite % 5 == 1 {
            self.emit(Event::Chew);
        }
        if self.bite >= EAT_TICKS {
            self.bite = 0;
            if let Some(potion) = potion {
                let damage = potion.drink(&mut self.vitals);
                self.damage(damage, survival::CAUSE_MAGIC);
                if !self.creative {
                    self.inventory.slots[self.selected] = Some(Stack::new(Item::GLASS_BOTTLE, 1));
                }
            } else if let Some((hunger, saturation)) = food {
                self.inventory.take_one(self.selected);
                self.vitals.hunger.eat(hunger, saturation);
                if let Some((effect, amp, ticks)) = held.and_then(Item::food_effect) {
                    self.vitals.apply_effect(effect, amp, ticks);
                }
            }
            // A timed `eat` command stops after one bite.
            if self.remaining <= 1 {
                self.eating = false;
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

    /// The targeted enchanting table and its offers for the held item.
    fn table_offers(&self, world: &World) -> Option<(IVec3, [crate::enchant::Offer; 3])> {
        let (pos, _) = self.target(world)?;
        let held = self.inventory.get(self.selected)?;
        (world.get_block(pos) == Some(Block::ENCHANTING_TABLE) && crate::enchant::table_accepts(held)).then(|| {
            (pos, crate::enchant::offers(self.vitals.xp.seed, held.item, crate::enchant::bookshelves(world, pos)))
        })
    }

    /// Java's enchanting table for an agent: one of the held item gets
    /// offer `i`, for `i + 1` levels and lapis (none in creative).
    fn enchant_at_table(&mut self, i: usize, world: &World) -> Result<(), String> {
        let (_, offers) =
            self.table_offers(world).ok_or("not aiming at an enchanting table with an enchantable item")?;
        let offer = offers[i];
        let held = self.inventory.get(self.selected).ok_or("selected slot empty")?;
        let lapis = self
            .inventory
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == Item::LAPIS_LAZULI)
            .map(|s| s.count as usize)
            .sum::<usize>();
        let level = self.vitals.xp.level;
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
        for (e, l) in crate::enchant::offer_enchants(self.vitals.xp.seed, held.item, i, offer.cost) {
            out.enchants.set(e, l);
        }
        let mut inv = self.inventory.clone();
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
            self.vitals.xp.add_levels(-(i as i64 + 1));
        }
        self.inventory = inv;
        self.vitals.xp.seed = (crate::enchant::roll() * u32::MAX as f32) as u32 as i32;
        Ok(())
    }

    /// Damage through protection enchantments (armor points aside).
    fn damage(&mut self, amount: f32, cause: &str) -> f32 {
        let amount = crate::enchant::protect(amount, &self.inventory.armor, cause);
        self.vitals.damage(amount, cause, self.creative)
    }

    /// Armored damage from a mob, arrow or explosion. Knockback only lands
    /// with damage (hurt immunity also stops repeated shoves), and a survival
    /// agent killed this way drops everything. Returns the damage taken.
    pub fn hurt(&mut self, amount: f32, cause: &str, knockback: DVec3, entities: &mut Entities) -> f32 {
        let reduced = simulation::survival::armor_reduce(amount, self.inventory.armor_points());
        let taken = self.damage(reduced, cause);
        if taken <= 0.0 {
            return 0.0;
        }
        self.player.vel += knockback;
        self.inventory.wear_armor(amount);
        self.sleeping = None;
        if self.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
        }
        taken
    }

    /// Held device input for the next tick (a local controller). Unlike a
    /// timed `move` or `mine`, this keeps mining progress on the same block.
    pub fn hold(&mut self, input: MoveInput, mining: bool, using: bool) {
        self.input = input;
        self.mining = mining;
        self.eating = using;
        self.remaining = 1;
    }

    /// The block being mined and the fraction broken (for crack overlays).
    pub fn breaking(&self, world: &World) -> Option<(IVec3, f32)> {
        let (pos, progress) = self.breaking?;
        world.get_block(pos)?;
        Some((pos, (progress as f32).min(1.0)))
    }

    /// What this agent mines with, and where it stands (Java's penalties).
    fn digger(&self, world: &World) -> mining::Digger {
        mining::Digger {
            held: self.inventory.get(self.selected),
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(world),
            on_ground: self.player.on_ground || self.player.flying,
        }
    }

    /// Whether hostile mobs may attack this agent.
    pub fn targetable(&self) -> bool {
        !self.creative && !self.vitals.is_dead()
    }

    /// Structured observation includes loaded status, target, inventory and optional nearby cells.
    pub fn observe(&self, world: &World, radius: i32) -> Value {
        let center = self.player.pos.floor().as_ivec3();
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
                    json!({"slot":slot+1,"item":s.item.name(),"count":s.count,"damage":s.damage,"enchantments":enchants})
                })
            })
            .collect();
        let offers = self
            .table_offers(world)
            .map(|(_, o)| o.map(|o| json!({"cost":o.cost,"clue":o.clue.map(|(e, l)| e.describe(l))})).to_vec());
        let target = self.target(world).map(|(p, n)| {
            json!({"position":p.to_array(),"face":n.to_array(),"block":world.get_block(p).map(|b|b.name()),"enchanting_offers":offers})
        });
        json!({"position":self.player.pos.to_array(),"yaw":self.player.yaw.to_degrees(),"pitch":self.player.pitch.to_degrees(),"loaded":world.is_loaded(center),"dimension":world.generator.dimension.name(),"health":self.vitals.health,"food":self.vitals.hunger.food,"level":self.vitals.xp.level,"xp_progress":self.vitals.xp.progress(),"dead":self.vitals.is_dead(),"creative":self.creative,"flying":self.player.flying,"sleeping":self.sleeping.is_some(),"spawn_bed":self.spawn_bed.map(|p|p.to_array()),"selected":self.selected+1,"inventory":inventory,"target":target,"blocks":blocks})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunk::ChunkData, terrain::Generator};
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
        a.hold(MoveInput::default(), true, false);
        a.tick(&mut world, &mut entities);
        let after = a.breaking(&world).unwrap();
        let expected = before.1 + (TICK_SECONDS / mining::dig_time(Block::STONE, a.digger(&world)) as f64) as f32;
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
        a.execute(Command::Mode(true), &mut world, &mut entities, &[]).unwrap();
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
        assert_eq!(a.work, [None; 2]);
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
}
