//! Fire and lava ignition. Random ticks share the growth dispatch; active
//! fires also tick every 30..39 game ticks, as in Java Minecraft. Only
//! scheduled fires cost work, and age-only edits never rebuild meshes.
//!
//! Scheduled fire ticks follow Java 26.3 `LevelTicks`: each 16-block chunk
//! column keeps its own container, containers outside the block-ticking
//! region wait without costing anything, and the ticks that are due run in
//! Java's collection order (`LevelTicks.drainContainers`): each container
//! yields by trigger tick, then scheduling order (the sub-tick order), and the
//! containers take turns by the sub-tick order of their next tick, at most
//! [`MAX_TICKS_PER_TICK`] per game tick. Fire ticks are not saved: loading a
//! chunk reschedules every fire in it, so saves are unchanged.
//!
//! Reference: MinecraftForge's 1.21.1 FireBlock/LavaFluid patches at
//! https://github.com/MinecraftForge/MinecraftForge/tree/1.21.1/patches/minecraft/net/minecraft/world/level

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::{DVec3, IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::World;
use super::block::Block;
#[cfg(test)]
use super::chunk::chunk_of;
use super::chunk::{CHUNK_SIZE_I, ChunkData};

const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
const HORIZONTAL: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];
/// The `maxAllowedTicks` Java's `ServerLevel.tickBlocks` passes to
/// `LevelTicks.tick`; ticks beyond it stay scheduled for the next game tick.
const MAX_TICKS_PER_TICK: usize = 65_536;

/// A fire's schedule: when it is due and the order it was scheduled in
/// (Java's `subTickOrder`), which breaks ties between equal trigger ticks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Scheduled {
    due: u64,
    sequence: u64,
}

/// One container entry: trigger tick, sub-tick order and position.
type Entry = Reverse<(u64, u64, [i32; 3])>;

/// Java's chunk column of a block (`ChunkPos`), the key of a tick container.
fn java_chunk(p: IVec3) -> IVec2 {
    IVec2::new(p.x >> 4, p.z >> 4)
}

#[derive(Default)]
pub(super) struct FireState {
    /// Authoritative schedules; removed/replaced fires leave harmless stale
    /// container entries that disappear when they come due.
    pending: FxHashMap<IVec3, Scheduled>,
    /// `LevelTicks.allContainers`: each chunk column's scheduled fires, the
    /// next one due first.
    containers: FxHashMap<IVec2, BinaryHeap<Entry>>,
    tick: u64,
    remainder: f64,
    /// Java's `subTickCount`.
    sequence: u64,
}

impl World {
    fn schedule_fire(&mut self, p: IVec3) {
        if self.fire.pending.contains_key(&p) {
            return;
        }
        let due = self.fire.tick + 30 + self.roll() % 10;
        let sequence = self.fire.sequence;
        self.fire.sequence += 1;
        self.fire.pending.insert(p, Scheduled { due, sequence });
        self.fire.containers.entry(java_chunk(p)).or_default().push(Reverse((due, sequence, p.to_array())));
    }

    pub(super) fn track_fire(&mut self, p: IVec3, old: Block, new: Block) {
        // Only aging fire is scheduled; soul fire never changes on its own.
        if new.fire_age().is_some() {
            self.schedule_fire(p);
        } else if old.fire_age().is_some() {
            self.fire.pending.remove(&p);
        }
    }

    /// Generated terrain contains no fire. Scan saved chunks once on load
    /// to resume their fires, preserving ages stored in the block ids.
    pub(super) fn load_fires(&mut self, cpos: IVec3, data: &ChunkData) {
        if data.uniform().is_some_and(|b| b.fire_age().is_none()) {
            return;
        }
        let mut i = 0;
        data.for_each_block(|b| {
            if b.fire_age().is_some() {
                let l =
                    IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
                self.schedule_fire(cpos * CHUNK_SIZE_I + l);
            }
            i += 1;
        });
    }

    /// Advances scheduled fires one game tick at a time, like `LevelTicks`.
    /// Distant fires keep their ages until the player returns.
    pub fn tick_fire(&mut self, dt: f64, player: DVec3) {
        if dt <= 0.0 {
            return;
        }
        self.fire.remainder += dt * 20.0;
        let ticks = self.fire.remainder as u64;
        self.fire.remainder -= ticks as f64;
        self.simulation_player(player);
        for _ in 0..ticks {
            self.fire.tick += 1;
            self.run_fire_ticks();
        }
    }

    /// `LevelTicks.tick` for the current game tick: collect the due ticks of
    /// the containers in the block-ticking region in global order (up to the
    /// per-tick limit), then run them.
    fn run_fire_ticks(&mut self) {
        let now = self.fire.tick;
        // `sortContainersToTick`: containers whose next tick is due and that
        // may tick. Others cost one peek and keep waiting.
        // Keyed by the sub-tick order, as `CONTAINER_DRAIN_ORDER` compares
        // the containers' next ticks by `INTRA_TICK_DRAIN_ORDER`.
        let mut heads: BinaryHeap<Reverse<(u64, [i32; 2])>> = BinaryHeap::new();
        for (&chunk, container) in &self.fire.containers {
            if let Some(&Reverse((due, sequence, _))) = container.peek()
                && due <= now
                && self.activity.contains(IVec3::new(chunk.x << 4, 0, chunk.y << 4), true)
            {
                heads.push(Reverse((sequence, chunk.to_array())));
            }
        }
        // `drainContainers`: the container with the lowest next sub-tick order
        // goes first; after each tick the heads are compared again, which is
        // what `drainFromCurrentContainer` does.
        let mut due_ticks = Vec::new();
        while due_ticks.len() < MAX_TICKS_PER_TICK {
            let Some(Reverse((_, chunk))) = heads.pop() else { break };
            let chunk = IVec2::from_array(chunk);
            let Some(container) = self.fire.containers.get_mut(&chunk) else { continue };
            let Some(Reverse(next)) = container.pop() else { continue };
            due_ticks.push(next);
            if let Some(&Reverse((due, sequence, _))) = container.peek()
                && due <= now
            {
                heads.push(Reverse((sequence, chunk.to_array())));
            }
            if container.is_empty() {
                self.fire.containers.remove(&chunk);
            }
        }
        for (due, sequence, xyz) in due_ticks {
            let p = IVec3::from_array(xyz);
            if self.fire.pending.get(&p) != Some(&Scheduled { due, sequence }) {
                continue;
            }
            self.fire.pending.remove(&p);
            // An unloaded fire reschedules when its chunk loads again.
            if !self.is_loaded(p) || !self.get_block(p).is_some_and(|b| b.fire_age().is_some()) {
                continue;
            }
            // `FireBlock.tick` reschedules itself before `canSpreadFireAround`.
            if self.fire_may_spread(p) {
                self.random_tick(p);
            }
            if self.get_block(p).is_some_and(|b| b.fire_age().is_some()) {
                self.schedule_fire(p);
            }
        }
    }

    fn fire_fuel(&self, p: IVec3) -> u8 {
        SIDES.iter().filter_map(|&d| self.get_block(p + d)).map(|b| b.fire_odds().0).max().unwrap_or(0)
    }

    fn fire_supported(&self, p: IVec3) -> bool {
        self.get_block(p - IVec3::Y).is_some_and(|b| b.supports_fire()) || self.fire_fuel(p) > 0
    }

    /// Java's `BaseFireBlock.getState`: soul fire over soul sand and soul
    /// soil, ordinary fire of `age` anywhere else.
    pub(super) fn fire_state(&self, p: IVec3, age: u8) -> Block {
        if self.get_block(p - IVec3::Y).is_some_and(super::nether_biome_blocks::soul_fire_base) {
            super::nether_biome_blocks::SOUL_FIRE
        } else {
            Block::fire(age)
        }
    }

    /// Soul fire only survives on its soul base; it never spreads or ages.
    fn soul_fire_supported(&self, p: IVec3) -> bool {
        self.get_block(p - IVec3::Y).is_some_and(super::nether_biome_blocks::soul_fire_base)
    }

    fn rain_near_fire(&self, p: IVec3) -> bool {
        self.rains_on(p) || HORIZONTAL.iter().any(|&d| self.rains_on(p + d))
    }

    pub(super) fn extinguish_unsupported_fire(&mut self, p: IVec3) {
        for q in std::iter::once(p).chain(SIDES.map(|d| p + d)) {
            let Some(fire) = self.get_block(q).filter(|b| b.is_fire()) else { continue };
            let supported = if fire == super::nether_biome_blocks::SOUL_FIRE {
                self.soul_fire_supported(q)
            } else {
                self.fire_supported(q)
            };
            if SIDES.iter().all(|&d| self.get_block(q + d).is_some()) && !supported {
                self.edit(q, Block::AIR, false);
            }
        }
    }

    /// Flint and steel lights a portal first, otherwise a supported empty
    /// cell. Failed uses neither place fire nor spend tool durability.
    pub fn ignite(&mut self, p: IVec3) -> bool {
        if !self.contains_y(p.y) {
            return false;
        }
        self.light_portal(p)
            || (self.get_block(p) == Some(Block::AIR)
                && self.fire_supported(p)
                && self.set_block(p, self.fire_state(p, 0)))
    }

    pub(super) fn tick_fire_block(&mut self, p: IVec3, age: u8) {
        if SIDES.iter().any(|&d| self.get_block(p + d).is_none()) {
            return;
        }
        if !self.fire_supported(p) {
            self.edit(p, Block::AIR, false);
            return;
        }
        let below = self.get_block(p - IVec3::Y).unwrap_or(Block::AIR);
        let eternal = infiniburn(below, self.generator.dimension);
        if !eternal && self.rain_near_fire(p) && self.roll() % 100 < 20 + age as u64 * 3 {
            self.edit(p, Block::AIR, false);
            return;
        }
        let next_age = (age + (self.roll() % 3 / 2) as u8).min(15);
        if next_age != age {
            self.edit(p, Block::fire(next_age), false);
        }
        if !eternal {
            if self.fire_fuel(p) == 0 {
                if !below.supports_fire() || age > 3 {
                    self.edit(p, Block::AIR, false);
                }
                return;
            }
            if age == 15 && below.fire_odds().0 == 0 && self.one_in(4) {
                self.edit(p, Block::AIR, false);
                return;
            }
        }

        let humid = matches!(self.foliage_at(p.x, p.z), Some(1 | 3));
        // Direct neighbours are consumed, faster vertically. TNT primes
        // instead of dropping as an item; burned blocks never drop loot.
        for d in SIDES {
            let q = p + d;
            let Some(b) = self.get_block(q) else { continue };
            let chance = if d.y == 0 { 300 } else { 250 } - if humid { 50 } else { 0 };
            if b.fire_odds().1 == 0 || self.roll() % chance >= b.fire_odds().1 as u64 {
                continue;
            }
            let new = if b != Block::TNT && self.roll() % (age as u64 + 10) < 5 && !self.rains_on(q) {
                let age = (age + (self.roll() % 5 / 4) as u8).min(15);
                self.fire_state(q, age)
            } else {
                Block::AIR
            };
            if self.edit(q, new, false) {
                if b == Block::TNT && self.rules.tnt_explodes {
                    self.primed_tnt.push((q, false));
                }
                self.settle(q);
            }
        }

        // Fire can leap one block horizontally and up to four upward,
        // provided the empty destination is beside fuel. The world difficulty
        // adds 7 encouragement per level; humid biomes halve the spread chance.
        for dy in -1..=4 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let q = p + IVec3::new(dx, dy, dz);
                    if q == p || self.get_block(q) != Some(Block::AIR) || !self.contains_y(q.y) {
                        continue;
                    }
                    let fuel = self.fire_fuel(q);
                    if fuel == 0 {
                        continue;
                    }
                    let odds = spread_odds(fuel as u64, age as u64, self.rules.difficulty, humid);
                    let bound = 100 + (dy - 1).max(0) as u64 * 100;
                    if odds > 0 && self.roll() % bound <= odds && !self.rain_near_fire(q) {
                        let age = (age + (self.roll() % 5 / 4) as u8).min(15);
                        if !self.light_portal(q) {
                            self.edit(q, self.fire_state(q, age), false);
                        }
                    }
                }
            }
        }
    }

    /// `LightningBolt.spawnFire`: on Normal or Hard (with fire ticking
    /// allowed) a strike at `p` lights that cell if it is empty and a fire
    /// can stand there, then tries `extra` more cells within one block of it.
    /// Returns how many fires were lit. Nothing in the game strikes lightning
    /// yet; this is the rule it will call.
    pub fn lightning_fire(&mut self, p: IVec3, extra: u32) -> u32 {
        if self.rules.difficulty < 2 || self.rules.fire_spread_radius == 0 {
            return 0;
        }
        let mut lit = 0;
        for i in 0..=extra {
            let q = if i == 0 {
                p
            } else {
                p + IVec3::new((self.roll() % 3) as i32 - 1, (self.roll() % 3) as i32 - 1, (self.roll() % 3) as i32 - 1)
            };
            if self.contains_y(q.y) && self.get_block(q) == Some(Block::AIR) && self.fire_supported(q) {
                self.edit(q, self.fire_state(q, 0), false);
                lit += 1;
            }
        }
        lit
    }

    /// Minecraft's lava random tick: walk upward 1..2 cells, looking for
    /// air beside fuel, or try three nearby blocks at the lava's height.
    /// Unloaded chunks and solid blocks stop the upward walk.
    pub(super) fn tick_lava_fire(&mut self, p: IVec3) {
        let steps = self.roll() % 3;
        if steps > 0 {
            let mut q = p;
            for _ in 0..steps {
                q += IVec3::new((self.roll() % 3) as i32 - 1, 1, (self.roll() % 3) as i32 - 1);
                match self.get_block(q) {
                    Some(Block::AIR) if q.y < self.max_y() => {
                        if SIDES.iter().any(|&d| self.get_block(q + d).is_some_and(|b| b.ignited_by_lava())) {
                            if !self.light_portal(q) {
                                self.edit(q, self.fire_state(q, 0), false);
                            }
                            return;
                        }
                    }
                    None => return,
                    Some(b) if b.is_solid() => return,
                    _ => {}
                }
            }
        } else {
            for _ in 0..3 {
                let q = p + IVec3::new((self.roll() % 3) as i32 - 1, 0, (self.roll() % 3) as i32 - 1);
                let above = q + IVec3::Y;
                if above.y < self.max_y()
                    && self.get_block(q).is_some_and(|b| b.ignited_by_lava())
                    && self.get_block(above) == Some(Block::AIR)
                    && !self.light_portal(above)
                {
                    self.edit(above, self.fire_state(above, 0), false);
                }
            }
        }
    }
}

/// The `infiniburn_overworld`, `infiniburn_nether` and `infiniburn_end` block
/// tags: fire on netherrack and magma blocks never ages out or rains out, and
/// bedrock joins them in the End (the crystals' flames).
pub fn infiniburn(below: Block, dimension: super::terrain::Dimension) -> bool {
    below == Block::NETHERRACK
        || below == Block::MAGMA
        || (below == Block::BEDROCK && dimension == super::terrain::Dimension::End)
}

/// `FireBlock.tick`'s chance for fire to leap to a neighbouring cell:
/// `(fuel + 40 + 7 x difficulty id) / (age + 30)`, halved in humid biomes.
fn spread_odds(fuel: u64, age: u64, difficulty: u8, humid: bool) -> u64 {
    (fuel + 40 + 7 * u64::from(difficulty)) / (age + 30) / if humid { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::IVec2;

    use super::*;
    use crate::world::chunk::CHUNK_SIZE;
    use crate::world::terrain::Generator;

    const AT: IVec3 = IVec3::new(8, 145, 8);

    /// Empty loaded column with a known rainy biome; no terrain generation
    /// or mesh jobs are needed to exercise the simulation.
    fn world() -> World {
        let mut world = World::new(Arc::new(Generator::new(7)), Default::default(), 2);
        for y in world.generator.dimension.chunk_rows() {
            world.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        let col = world.columns.get_mut(&IVec2::ZERO).unwrap();
        col.foliage = Some(Box::new([0; CHUNK_SIZE * CHUNK_SIZE]));
        // A plains-like snow line: rain below y = 160.
        col.snow = Some(Box::new([160; CHUNK_SIZE * CHUNK_SIZE]));
        world
    }

    fn light(world: &mut World, p: IVec3, base: Block) {
        world.edit(p - IVec3::Y, base, false);
        assert!(world.ignite(p));
    }

    /// The (encouragement, flammability) a block family has in Java's public
    /// flammability table (the wiki's "Fire" page), by registry name; `None`
    /// for names the table does not mention (checked to burn nothing).
    fn documented(name: &str) -> Option<(u8, u8)> {
        const WOODS: [&str; 10] =
            ["oak", "spruce", "birch", "jungle", "acacia", "dark oak", "mangrove", "cherry", "pale oak", "bamboo"];
        let wood = WOODS.iter().find_map(|w| name.strip_prefix(w).and_then(|rest| rest.strip_prefix(' ')));
        if let Some(kind) = wood {
            return match kind {
                "planks" | "slab" | "stairs" | "fence" | "fence gate" | "mosaic" | "roots" => Some((5, 20)),
                "log" | "wood" | "block" => Some((5, 5)),
                "leaves" => Some((30, 60)),
                "shelf" => Some((30, 20)),
                _ => None,
            };
        }
        let stripped = name.strip_prefix("stripped ").unwrap_or(name);
        if stripped.ends_with(" log") || stripped.ends_with(" wood") || name == "block of stripped bamboo" {
            return Some((5, 5));
        }
        if name == "block of bamboo" {
            return Some((5, 5));
        }
        Some(match name {
            "azalea leaves" | "flowering azalea leaves" => (30, 60),
            "wool" => (30, 60),
            n if n.ends_with(" wool") => (30, 60),
            n if n.ends_with(" carpet") && !n.contains("moss") => (60, 20),
            "bookshelf" | "lectern" => (30, 20),
            "composter" => (5, 20),
            "hay bale" => (60, 20),
            "tnt" => (15, 100),
            "vines" | "glow lichen" => (15, 100),
            "leaf litter" | "wildflowers" | "bush" | "firefly bush" | "short dry grass" | "tall dry grass" => (60, 100),
            "dried kelp block" => (30, 60),
            "bamboo" | "scaffolding" => (60, 60),
            "tall grass" | "fern" | "dead bush" | "large fern" | "sweet berry bush" | "dandelion" | "poppy"
            | "blue orchid" | "allium" | "azure bluet" | "red tulip" | "orange tulip" | "white tulip"
            | "pink tulip" | "oxeye daisy" | "cornflower" | "lily of the valley" | "sunflower" | "lilac"
            | "rose bush" | "peony" => (60, 100),
            _ => return None,
        })
    }

    /// Every registered state, family by family, against the documented table.
    #[test]
    fn every_flammable_family_matches_the_documented_flammability_table() {
        let mut checked = std::collections::BTreeSet::new();
        for id in 0..crate::world::block::STATE_CAPACITY as u16 {
            let b = Block(id);
            if b.name() == "unknown" || b.is_waterlogged() || b.waterloggable() && b.with_water(false) != b {
                continue;
            }
            // Wool stairs and slabs are VoxelCraft's, burning as the wool they are cut from.
            if b.name().ends_with("wool slab") || b.name().ends_with("wool stairs") {
                assert_eq!(b.fire_odds(), (30, 60), "{}", b.name());
                continue;
            }
            // Top halves of tall plants answer as their bottoms.
            let name = b.name().strip_suffix(" top").unwrap_or(b.name());
            if let Some(odds) = documented(name) {
                assert_eq!(b.fire_odds(), odds, "{} ({id})", b.name());
                checked.insert(name.to_string());
            } else if !matches!(
                name,
                "moss block"
                    | "moss carpet"
                    | "pale moss block"
                    | "pale moss carpet"
                    | "azalea"
                    | "flowering azalea"
                    | "cave vines"
                    | "cave vines plant"
                    | "spore blossom"
                    | "hanging roots"
                    | "pink petals"
                    | "pale hanging moss"
                    | "pale hanging moss plant"
                    | "tall grass plant"
                    | "straw bed"
            ) {
                assert_eq!(b.fire_odds(), (0, 0), "{} ({id}) is not in the flammability table", b.name());
            }
        }
        // The sweep reached every family the table names.
        for family in [
            "oak planks",
            "spruce slab",
            "birch stairs",
            "jungle fence",
            "acacia fence gate",
            "oak log",
            "stripped oak log",
            "oak wood",
            "mangrove roots",
            "dark oak leaves",
            "azalea leaves",
            "bookshelf",
            "lectern",
            "composter",
            "hay bale",
            "tnt",
            "vines",
            "dried kelp block",
            "bamboo",
            "scaffolding",
            "wool",
            "red carpet",
            "dead bush",
            "sunflower",
            "sweet berry bush",
            "bamboo mosaic",
            "block of bamboo",
            "block of stripped bamboo",
            "pale oak shelf",
        ] {
            assert!(checked.contains(family), "no state of {family} was checked");
        }
    }

    #[test]
    fn doors_saplings_mushrooms_and_furniture_without_an_entry_do_not_burn_by_fire() {
        for name in [
            "oak door",
            "acacia door",
            "bamboo door",
            "oak sapling",
            "mangrove propagule",
            "red mushroom",
            "lily pad",
            "oak trapdoor",
            "oak button",
            "oak pressure plate",
            "oak sign",
            "chiseled bookshelf",
            "crafting table",
            "ladder",
            "chest",
            "crimson planks",
            "warped fence",
            "white banner",
        ] {
            if let Some(b) = Block::from_name(name) {
                assert_eq!(b.fire_odds(), (0, 0), "{name}");
            }
        }
        // Wooden doors, ladders and containers still catch from lava.
        for name in ["oak door", "acacia door", "ladder", "chest", "crafting table", "white banner"] {
            if let Some(b) = Block::from_name(name) {
                assert!(b.ignited_by_lava(), "{name}");
            }
        }
    }

    #[test]
    fn ignition_needs_empty_space_and_support() {
        let mut w = world();
        assert!(!w.ignite(AT), "air has no support");
        light(&mut w, AT, Block::STONE);
        assert_eq!(w.get_block(AT), Some(Block::FIRE));
        assert_eq!(w.fire.pending.len(), 1);
        assert!(!w.ignite(AT), "already lit");
        assert!(!w.ignite(AT - IVec3::Y), "occupied");
        assert!(!w.ignite(IVec3::new(8, w.max_y(), 8)));
        assert!(!w.ignite(IVec3::new(8, w.min_y() - 1, 8)));
        assert!(!w.ignite(AT + IVec3::X * CHUNK_SIZE_I), "unloaded chunk");
        w.set_block(AT - IVec3::Y, Block::AIR);
        assert_eq!(w.get_block(AT), Some(Block::AIR), "support updates extinguish immediately");
        assert!(w.fire.pending.is_empty());

        w.edit(AT, Block::PLANKS, false);
        assert!(w.ignite(AT + IVec3::X), "can cling to flammable sides");
        w.set_block(AT, Block::AIR);
        assert_eq!(w.get_block(AT + IVec3::X), Some(Block::AIR));
        light(&mut w, AT, Block::GLASS);
        assert_eq!(w.get_block(AT), Some(Block::FIRE), "glass has a full top face");
    }

    #[test]
    fn ignition_fills_portal_frames_even_with_existing_fire_inside() {
        let mut w = world();
        for dx in -1..=2 {
            w.edit(AT + IVec3::new(dx, -1, 0), Block::OBSIDIAN, false);
            w.edit(AT + IVec3::new(dx, 3, 0), Block::OBSIDIAN, false);
        }
        for y in 0..3 {
            for dx in [-1, 2] {
                w.edit(AT + IVec3::new(dx, y, 0), Block::OBSIDIAN, false);
            }
        }
        w.edit(AT, Block::fire(8), false);
        assert!(w.ignite(AT));
        for y in 0..3 {
            for dx in 0..2 {
                assert_eq!(w.get_block(AT + IVec3::new(dx, y, 0)), Some(Block::NETHER_PORTAL));
            }
        }
        assert!(w.fire.pending.is_empty());
    }

    #[test]
    fn netherrack_burns_forever_but_stone_burns_out() {
        let mut w = world();
        let stone = AT + IVec3::X * 8;
        light(&mut w, AT, Block::NETHERRACK);
        light(&mut w, stone, Block::STONE);
        for _ in 0..200 {
            w.random_tick(AT);
            w.random_tick(stone);
        }
        assert_eq!(w.get_block(AT), Some(Block::fire(15)));
        assert_eq!(w.get_block(stone), Some(Block::AIR));
        assert!(w.drops.is_empty());
        w.raining = true;
        assert!(w.rains_on(AT));
        for _ in 0..100 {
            w.random_tick(AT);
        }
        assert_eq!(w.get_block(AT), Some(Block::fire(15)), "even rain leaves netherrack lit");
    }

    #[test]
    fn magma_blocks_burn_forever_too_and_the_end_adds_bedrock() {
        use crate::world::terrain::Dimension::{End, Nether, Overworld};
        let mut w = world();
        light(&mut w, AT, Block::MAGMA);
        w.raining = true;
        for _ in 0..300 {
            w.random_tick(AT);
        }
        assert_eq!(w.get_block(AT), Some(Block::fire(15)), "magma is infiniburn in every dimension, rain or not");
        for d in [Overworld, Nether, End] {
            assert!(infiniburn(Block::NETHERRACK, d) && infiniburn(Block::MAGMA, d));
            assert!(!infiniburn(Block::STONE, d) && !infiniburn(Block::SOUL_SAND, d));
            assert_eq!(infiniburn(Block::BEDROCK, d), d == End);
        }
    }

    #[test]
    fn lightning_lights_its_cell_and_neighbours_only_from_normal_difficulty() {
        let mut w = world();
        w.edit(AT - IVec3::Y, Block::STONE, false);
        for difficulty in [0, 1] {
            w.rules.difficulty = difficulty;
            assert_eq!(w.lightning_fire(AT, 3), 0, "Peaceful and Easy: lightning starts no fires");
        }
        w.rules.difficulty = 2;
        w.rules.fire_spread_radius = 0;
        assert_eq!(w.lightning_fire(AT, 3), 0, "fire ticking off");
        w.rules.fire_spread_radius = 128;
        assert!(w.lightning_fire(AT, 0) == 1 && w.get_block(AT) == Some(Block::FIRE));
        assert_eq!(w.lightning_fire(AT, 0), 0, "the cell is no longer empty");
        // Unsupported cells (nothing below, nothing to burn) stay dark.
        assert_eq!(w.lightning_fire(AT + IVec3::Y * 20, 0), 0);
        // The extra tries stay within one block of the strike.
        let before: Vec<_> = (-1..=1).flat_map(|x| (-1..=1).map(move |z| AT + IVec3::new(x, 0, z))).collect();
        w.edit(AT, Block::AIR, false);
        for dx in -1..=1 {
            for dz in -1..=1 {
                w.edit(AT + IVec3::new(dx, -1, dz), Block::STONE, false);
            }
        }
        let lit = w.lightning_fire(AT, 3);
        assert!((1..=4).contains(&lit));
        let burning = before.iter().filter(|&&p| w.get_block(p) == Some(Block::FIRE)).count();
        assert!(burning >= 1 && burning as u32 <= lit);
    }

    #[test]
    fn rain_reaches_fire_and_adjacent_cells_but_not_roofs_or_dry_biomes() {
        let mut w = world();
        light(&mut w, AT, Block::STONE);
        w.raining = true;
        // A roof over just the fire doesn't shelter the adjacent cells.
        w.edit(AT + IVec3::Y * 2, Block::STONE, false);
        assert!(!w.rains_on(AT));
        assert!(w.rain_near_fire(AT));
        for d in HORIZONTAL {
            w.edit(AT + d + IVec3::Y * 2, Block::STONE, false);
        }
        assert!(!w.rain_near_fire(AT));
        for d in std::iter::once(IVec3::ZERO).chain(HORIZONTAL) {
            w.edit(AT + d + IVec3::Y * 2, Block::AIR, false);
        }
        for _ in 0..20 {
            w.random_tick(AT);
        }
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        for line in [i16::MAX, i16::MIN] {
            w.columns.get_mut(&IVec2::ZERO).unwrap().snow = Some(Box::new([line; CHUNK_SIZE * CHUNK_SIZE]));
            assert!(!w.rains_on(AT), "dry/snowy biome {line}");
        }
        w.columns.get_mut(&IVec2::ZERO).unwrap().snow = Some(Box::new([160; CHUNK_SIZE * CHUNK_SIZE]));
        let high = AT.with_y(170);
        w.edit(high - IVec3::Y, Block::STONE, false);
        assert!(!w.rains_on(high), "snow on high terrain");
        // Rain is a per-column effect like the rendered weather sheets;
        // flying high above a low plain doesn't turn its rain into snow.
        assert!(!w.rains_on(high + IVec3::X), "no known ground: judged by the cell");
        w.edit(AT.with_y(100) + IVec3::X, Block::STONE, false);
        assert!(w.rains_on(high + IVec3::X));
    }

    #[test]
    fn fire_consumes_fuel_without_loot_and_primes_tnt_with_a_full_fuse() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        for (d, b) in [(IVec3::X, Block::TNT), (IVec3::Z, Block::WOOL), (IVec3::NEG_X, Block::LEAVES)] {
            w.edit(AT + d, b, false);
        }
        w.edit(AT + IVec3::Y, Block::STONE, false);
        for _ in 0..100 {
            w.random_tick(AT);
        }
        assert_ne!(w.get_block(AT + IVec3::Z), Some(Block::WOOL));
        assert_ne!(w.get_block(AT - IVec3::X), Some(Block::LEAVES));
        assert_eq!(w.get_block(AT + IVec3::Y), Some(Block::STONE));
        assert_eq!(w.primed_tnt, vec![(AT + IVec3::X, false)]);
        assert!(w.drops.is_empty());
    }

    #[test]
    fn fire_spreads_more_readily_on_harder_worlds() {
        // Peaceful, Easy, Normal, Hard are ids 0..3.
        assert_eq!([0, 1, 2, 3].map(|id| spread_odds(66, 0, id, false)), [3, 3, 4, 4]);
        assert_eq!([0, 1, 2, 3].map(|id| spread_odds(89, 0, id, false)), [4, 4, 4, 5]);
        assert_eq!(spread_odds(89, 0, 3, true), 2, "humid biomes halve it");
        assert_eq!(spread_odds(89, 30, 3, false), 2, "older fire spreads less");
        assert_eq!(crate::world::WorldRules::default().difficulty, 2, "Normal until the game says otherwise");
    }

    #[test]
    fn fire_burning_tnt_primes_nothing_without_tnt_explodes() {
        let mut w = world();
        w.rules.tnt_explodes = false;
        light(&mut w, AT, Block::NETHERRACK);
        w.edit(AT + IVec3::X, Block::TNT, false);
        for _ in 0..100 {
            w.random_tick(AT);
        }
        assert!(w.primed_tnt.is_empty());
    }

    #[test]
    fn fire_spread_radius_decides_which_fires_tick() {
        // Fire on bare stone burns out on its own once it ticks.
        let out_after = |radius: i32, player: DVec3| {
            let mut w = world();
            w.rules.fire_spread_radius = radius;
            w.set_simulation_players([player]);
            light(&mut w, AT, Block::STONE);
            for _ in 0..40 {
                w.tick_fire(1.0, player);
            }
            w.get_block(AT) != Some(Block::FIRE)
        };
        let here = AT.as_dvec3();
        assert!(out_after(128, here), "default radius: fire beside the player ticks");
        assert!(!out_after(0, here), "radius 0 stops fire like doFireTick=false");
        // 150 blocks is still inside the default 10-chunk simulation distance.
        assert!(out_after(-1, here + DVec3::X * 150.0), "-1 ticks fire at any distance the world simulates");
        assert!(!out_after(128, here + DVec3::X * 150.0), "beyond the radius fire is frozen");
        assert!(out_after(128, here + DVec3::X * 100.0), "inside the radius");
        assert!(!out_after(16, here + DVec3::X * 17.0));
        assert!(out_after(16, here + DVec3::X * 15.0));
    }

    #[test]
    fn spectators_do_not_keep_fire_ticking() {
        let mut w = world();
        w.rules.fire_spread_radius = 128;
        let here = AT.as_dvec3();
        w.set_simulation_players([here]);
        assert!(w.fire_may_spread(AT));
        w.set_simulation_spectators([true]);
        assert!(!w.fire_may_spread(AT), "`ChunkMap.playerIsCloseEnoughTo` skips Spectators");
        w.set_simulation_players([here + DVec3::X * 500.0, here]);
        w.set_simulation_spectators([false, true]);
        assert!(!w.fire_may_spread(AT));
        w.set_simulation_spectators([true, false]);
        assert!(w.fire_may_spread(AT), "a second player who is not spectating counts");
        w.rules.fire_spread_radius = -1;
        w.set_simulation_spectators([true, true]);
        assert!(w.fire_may_spread(AT), "-1 needs no player");
    }

    #[test]
    fn fire_spreads_upward_into_air_beside_fuel() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        let destination = AT + IVec3::new(1, 1, 1);
        w.edit(destination + IVec3::Y, Block::WOOL, false);
        let mut spread = false;
        for _ in 0..400 {
            w.random_tick(AT);
            spread |= w.get_block(destination).is_some_and(|b| b.is_fire());
        }
        assert!(spread, "fire leaps to nearby fuel without consuming it first");
    }

    #[test]
    fn lava_random_ticks_ignite_nearby_wood_and_respect_solid_roofs() {
        let mut w = world();
        for dz in -1..=1 {
            for dx in -1..=1 {
                w.edit(AT + IVec3::new(dx, 0, dz), Block::PLANKS, false);
            }
        }
        w.edit(AT, Block::LAVA, false);
        for _ in 0..40 {
            w.random_tick(AT);
        }
        assert!(
            (-1..=1)
                .any(|dz| { (-1..=1).any(|dx| w.get_block(AT + IVec3::new(dx, 1, dz)).is_some_and(|b| b.is_fire())) })
        );
        let enclosed = AT + IVec3::X * 8;
        for dz in -1..=1 {
            for dx in -1..=1 {
                w.edit(enclosed + IVec3::new(dx, 0, dz), Block::PLANKS, false);
                w.edit(enclosed + IVec3::new(dx, 1, dz), Block::STONE, false);
            }
        }
        w.edit(enclosed, Block::LAVA, false);
        for _ in 0..40 {
            w.random_tick(enclosed);
        }
        assert!((-2..=2).all(|dz| {
            (-2..=2).all(|dx| {
                !(1..=2).any(|dy| w.get_block(enclosed + IVec3::new(dx, dy, dz)).is_some_and(|b| b.is_fire()))
            })
        }));
    }

    #[test]
    fn water_extinguishes_fire_without_dropping_it() {
        let mut w = world();
        for dz in -2..=2 {
            for dx in -2..=2 {
                w.edit(AT + IVec3::new(dx, -1, dz), Block::STONE, false);
            }
        }
        assert!(w.ignite(AT));
        w.set_block(AT + IVec3::Y, Block::WATER);
        for _ in 0..8 {
            w.tick_fluids(0.25);
        }
        assert!(w.get_block(AT).unwrap().is_water());
        assert!(!w.fire.pending.contains_key(&AT));
        assert!(w.drops.is_empty());
    }

    #[test]
    fn scheduled_fire_pauses_at_distance_and_resumes_after_chunk_reload() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        let due = w.fire.pending[&AT].due;
        assert!((30..=39).contains(&due));
        w.tick_fire(1.49, AT.as_dvec3());
        assert_eq!(w.fire.pending[&AT].due, due);
        w.tick_fire(0.51, AT.as_dvec3());
        assert!(w.fire.pending[&AT].due > due);
        w.edit(AT, Block::fire(9), false);
        w.tick_fire(20.0, (AT + IVec3::X * 256).as_dvec3());
        assert_eq!(w.get_block(AT), Some(Block::fire(9)));

        let cpos = chunk_of(AT);
        w.remove_chunk(cpos);
        w.tick_fire(2.0, AT.as_dvec3());
        assert!(!w.fire.pending.contains_key(&AT));
        let data = w.saved.remove(&cpos).unwrap();
        w.insert_chunk(cpos, data, true);
        assert_eq!(w.get_block(AT), Some(Block::fire(9)));
        assert!(w.fire.pending.contains_key(&AT), "saved fire reschedules on load");
        let deadline = w.fire.pending[&AT].due;
        w.tick_fire(2.0, AT.as_dvec3());
        assert!(w.fire.pending[&AT].due > deadline);
        let overdue = w.fire.pending[&AT].due;
        w.fire.tick = overdue;
        w.tick_fire(0.0, AT.as_dvec3());
        assert_eq!(w.fire.pending[&AT].due, overdue, "menus pause even an overdue update backlog");
        w.tick_fire(0.05, AT.as_dvec3());
        assert!(w.fire.pending[&AT].due > overdue);
    }

    /// The active chunk 1 of the test column ticks around a player in Java
    /// chunk 7 (simulation distance 5: block ticking out to 6 chunks);
    /// chunk 0 sleeps, 7 chunks away.
    const HERE: DVec3 = DVec3::new(120.5, 150.0, 8.5);

    #[test]
    fn thousands_of_sleeping_fires_never_delay_an_active_one() {
        let mut w = world();
        w.set_simulation_distance(5);
        let active = IVec3::new(24, 145, 8);
        light(&mut w, active, Block::NETHERRACK);
        // The most delayed fire possible: due last, scheduled last.
        let last = Scheduled { due: 39, sequence: u64::MAX };
        w.fire.pending.insert(active, last);
        w.fire.containers.entry(java_chunk(active)).or_default().push(Reverse((
            last.due,
            last.sequence,
            active.to_array(),
        )));
        let mut sleepers = 0;
        for y in (100..196).step_by(2) {
            for z in 0..16 {
                for x in 0..16 {
                    w.edit(IVec3::new(x, y, z), Block::STONE, false);
                    w.edit(IVec3::new(x, y + 1, z), Block::fire(0), false);
                    sleepers += 1;
                }
            }
        }
        assert!(sleepers > 12_000);
        assert!(w.fire.pending.len() > sleepers, "{} pending for {sleepers}", w.fire.pending.len());
        for _ in 0..40 {
            w.tick_fire(0.05, HERE);
        }
        assert!(w.fire.tick >= 40);
        assert!(w.fire.pending[&active].due > last.due, "the active fire ticked on schedule");
        let asleep = w.fire.pending.iter().filter(|(p, _)| p.x < 16);
        assert_eq!(asleep.clone().count(), sleepers, "sleeping fires wait with their original deadlines");
        assert!(asleep.clone().all(|(_, s)| s.due <= 39));
        assert_eq!(w.get_block(IVec3::new(0, 101, 0)), Some(Block::fire(0)), "and do not age");
        // Back in range the whole backlog runs in one game tick.
        w.tick_fire(0.05, DVec3::new(8.5, 150.0, 8.5));
        assert!(w.fire.pending.values().all(|s| s.due > w.fire.tick), "no overdue fire is left behind");
    }

    #[test]
    fn at_most_65536_due_ticks_are_collected_per_game_tick_in_container_order() {
        let mut w = world();
        w.set_simulation_distance(5);
        w.simulation_player(HERE);
        // Distinct fires in chunk 1 with every due time in 30..=39 and
        // sequences interleaved the other way, so due time orders first.
        let total = MAX_TICKS_PER_TICK + 4_464;
        let mut order = Vec::new();
        for i in 0..total {
            let p = IVec3::new(16 + (i % 16) as i32, -60 + (i / 256) as i32, ((i / 16) % 16) as i32);
            let due = 39 - (i % 10) as u64;
            let sequence = (total - i) as u64;
            w.fire.pending.insert(p, Scheduled { due, sequence });
            w.fire.containers.entry(java_chunk(p)).or_default().push(Reverse((due, sequence, p.to_array())));
            order.push((due, sequence, p));
        }
        order.sort_unstable_by_key(|&(due, sequence, _)| (due, sequence));
        w.fire.tick = 99;
        w.tick_fire(0.05, HERE);
        assert_eq!(w.fire.pending.len(), 4_464, "the limit leaves the rest for the next tick");
        for &(_, _, p) in &order[MAX_TICKS_PER_TICK..] {
            assert!(w.fire.pending.contains_key(&p), "{p}: the last in (trigger, sub-tick) order waits");
        }
        w.tick_fire(0.05, HERE);
        assert!(w.fire.pending.is_empty());
        assert!(w.fire.containers.is_empty());
    }

    /// `CONTAINER_DRAIN_ORDER` compares containers by their next tick's
    /// sub-tick order alone, so a late-due tick scheduled first in one
    /// chunk is collected before an earlier-due one scheduled later in
    /// another. Only the limit makes that visible.
    #[test]
    fn containers_take_turns_by_the_sub_tick_order_of_their_next_tick() {
        let mut w = world();
        w.set_simulation_distance(5);
        w.simulation_player(HERE);
        let first = IVec3::new(16, 100, 16); // chunk (1, 1)
        let put = |w: &mut World, p: IVec3, due: u64, sequence: u64| {
            w.fire.pending.insert(p, Scheduled { due, sequence });
            w.fire.containers.entry(java_chunk(p)).or_default().push(Reverse((due, sequence, p.to_array())));
        };
        // Chunk (1, 1): one tick, due last, scheduled first.
        put(&mut w, first, 39, 0);
        // Chunk (1, 0): a full tick's worth, due earlier, scheduled later.
        let mut last = IVec3::ZERO;
        for i in 0..MAX_TICKS_PER_TICK {
            last = IVec3::new(16 + (i % 16) as i32, -60 + (i / 256) as i32, ((i / 16) % 16) as i32);
            put(&mut w, last, 30, 1 + i as u64);
        }
        w.fire.tick = 99;
        w.tick_fire(0.05, HERE);
        assert!(!w.fire.pending.contains_key(&first), "collected first, despite the later due time");
        assert!(w.fire.pending.contains_key(&last), "so the limit leaves the other chunk's last tick");
        assert_eq!(w.fire.pending.len(), 1);
    }

    #[test]
    fn age_changes_preserve_mesh_versions_and_copy_on_write_snapshots() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        let cpos = chunk_of(AT);
        let slot = &w.chunks[&cpos];
        let version = slot.version;
        let snapshot = slot.data.clone();
        w.edit(AT, Block::fire(15), false);
        assert_eq!(w.chunks[&cpos].version, version);
        assert_eq!(snapshot.get(8, 17, 8), Block::FIRE);
        assert_eq!(w.get_block(AT), Some(Block::fire(15)));
        assert!(w.chunks[&cpos].modified);
    }
}
