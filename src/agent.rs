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
use crate::simulation::{self, TICK_SECONDS, survival::Vitals};
use crate::world::{
    World,
    block::{Block, RenderKind},
    terrain::Dimension,
};

pub const HELP: &str = "observe [0..2] | catalog [query] | players | look yaw pitch | move forward right ticks [jump sprint sneak] | wait ticks | mine ticks | eat | place | attack | select 1..9 | fly on/off | craft item | chest take/put slot | drop | respawn | leave. Cheats: give item [count], gamemode creative/survival, tp x y z, setblock x y z block, time day/noon/night/0..1, weather clear/rain, dimension overworld/nether/end (host console only).";

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
            _ => return Err(bad()),
        })
    }

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
        )
    }
}

pub struct Agent {
    pub player: Player,
    pub previous_pos: DVec3,
    pub inventory: Inventory,
    pub vitals: Vitals,
    pub creative: bool,
    pub selected: usize,
    pub remaining: u32,
    input: MoveInput,
    mining: bool,
    /// Holding "use" eats held food; `bite` counts the ticks chewed.
    eating: bool,
    bite: u32,
    breaking: Option<(IVec3, f64)>,
    cooldown: f64,
}

impl Agent {
    pub fn new(pos: DVec3) -> Self {
        Self {
            player: Player::new(pos),
            previous_pos: pos,
            inventory: Inventory::default(),
            vitals: Vitals::default(),
            creative: false,
            selected: 0,
            remaining: 0,
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
                held.item.food().ok_or("selected item is not food")?;
                if self.creative || !self.vitals.hunger.can_eat() {
                    return Err("not hungry".into());
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
            Command::Give(item, count) => {
                let mut inv = self.inventory.clone();
                if inv.add(item, count) > 0 {
                    return Err("inventory full".into());
                }
                self.inventory = inv;
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
                self.cooldown = 0.22;
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
                let (i, _) = entities
                    .raycast(self.player.eye(), self.player.forward().as_dvec3(), distance)
                    .ok_or("no mob within reach")?;
                let held = self.inventory.get(self.selected).map(|s| s.item);
                if let Some(kind) = entities.attack(i, self.player.forward().as_dvec3(), mining::attack_damage(held)) {
                    entities.drop_loot(kind, entities.mobs[i].pos);
                }
                if !self.creative
                    && let Some(held) = held
                {
                    self.inventory.wear(self.selected, mining::wear(held, true));
                }
                self.cooldown = crate::entity::ATTACK_COOLDOWN;
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
            Command::Drop => {
                let stack = self.inventory.slots[self.selected].take().ok_or("selected slot empty")?;
                entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
            }
            Command::Respawn => {
                if !self.vitals.is_dead() {
                    return Err("player is alive".into());
                }
                self.player = Player::new(world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                self.player.can_fly = self.creative;
                self.vitals = Vitals::default();
            }
            Command::Observe(_) | Command::Help => {}
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
            return;
        }
        let mut input = if self.remaining > 0 { self.input } else { MoveInput::default() };
        input.sprint &= self.creative || self.vitals.hunger.can_sprint();
        let hurts = simulation::tick_player(&mut self.player, world, &mut self.vitals, input, self.creative).hurts;
        for (damage, cause) in [
            (hurts.fall, "hit the ground too hard"),
            (hurts.drown, "drowned"),
            (hurts.lava, "tried to swim in lava"),
            (hurts.fire + hurts.burn, "burned to death"),
            (hurts.starve, "starved to death"),
        ] {
            if damage > 0.0 {
                self.vitals.damage(damage, cause, self.creative);
            }
        }
        if self.vitals.is_dead() {
            for stack in self.inventory.take_all() {
                entities.scatter(stack, self.player.pos);
            }
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
            let progress = self.breaking.filter(|(p, _)| *p == pos).map_or(0.0, |(_, n)| n) + TICK_SECONDS;
            self.breaking = Some((pos, progress));
            if block != Block::BEDROCK
                && !block.is_door()
                && !block.is_bed()
                && (self.creative || progress >= mining::break_time(block, held) as f64)
            {
                world.set_block(pos, Block::AIR);
                if !self.creative {
                    if mining::can_harvest(block, held) {
                        world.spill_block(pos, block);
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
    }

    /// One tick of eating: a bite finishes after [`EAT_TICKS`] of holding
    /// the same food, like Java. Switching slots restarts it via `select`.
    fn chew(&mut self) {
        let food = self.inventory.get(self.selected).and_then(|s| s.item.food());
        let Some((hunger, saturation)) =
            food.filter(|_| self.remaining > 0 && self.eating && !self.creative && self.vitals.hunger.can_eat())
        else {
            self.bite = 0;
            return;
        };
        self.bite += 1;
        if self.bite >= EAT_TICKS {
            self.bite = 0;
            self.inventory.take_one(self.selected);
            self.vitals.hunger.eat(hunger, saturation);
            // A timed `eat` command stops after one bite.
            if self.remaining <= 1 {
                self.eating = false;
            }
        }
    }

    /// Fraction of the current bite chewed, for the HUD.
    pub fn eating(&self) -> f32 {
        self.bite as f32 / EAT_TICKS as f32
    }

    /// Armored damage from a mob, arrow or explosion. Knockback only lands
    /// with damage (hurt immunity also stops repeated shoves), and a survival
    /// agent killed this way drops everything. Returns the damage taken.
    pub fn hurt(&mut self, amount: f32, cause: &str, knockback: DVec3, entities: &mut Entities) -> f32 {
        let reduced = simulation::survival::armor_reduce(amount, self.inventory.armor_points());
        let taken = self.vitals.damage(reduced, cause, self.creative);
        if taken <= 0.0 {
            return 0.0;
        }
        self.player.vel += knockback;
        self.inventory.wear_armor(amount);
        if self.vitals.is_dead() {
            for stack in self.inventory.take_all() {
                entities.scatter(stack, self.player.pos);
            }
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
        let (pos, seconds) = self.breaking?;
        let block = world.get_block(pos)?;
        let held = self.inventory.get(self.selected).map(|s| s.item);
        Some((pos, (seconds as f32 / mining::break_time(block, held).max(1e-3)).min(1.0)))
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
                s.map(|s| json!({"slot":slot+1,"item":s.item.name(),"count":s.count,"damage":s.damage}))
            })
            .collect();
        let target = self.target(world).map(
            |(p, n)| json!({"position":p.to_array(),"face":n.to_array(),"block":world.get_block(p).map(|b|b.name())}),
        );
        json!({"position":self.player.pos.to_array(),"yaw":self.player.yaw.to_degrees(),"pitch":self.player.pitch.to_degrees(),"loaded":world.is_loaded(center),"dimension":world.generator.dimension.name(),"health":self.vitals.health,"food":self.vitals.hunger.food,"dead":self.vitals.is_dead(),"creative":self.creative,"flying":self.player.flying,"selected":self.selected+1,"inventory":inventory,"target":target,"blocks":blocks})
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
        ] {
            assert!(Command::parse(s).is_err(), "{s}");
        }
        assert!(matches!(Command::parse("/give dirt 64").unwrap(), Command::Give(_, 64)));
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
        a.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
        assert_eq!(world.get_block(IVec3::new(3, 151, 1)), Some(Block::STONE));
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
    }
}
