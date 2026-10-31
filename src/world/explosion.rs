//! Java 26.3 `ServerExplosion`: which blocks a blast breaks, what they drop
//! and how much of an entity it reaches.
//!
//! `calculateExplodedPositions` casts a ray from the centre through each of
//! the 1,352 points on the surface of a 16 x 16 x 16 grid. Each ray starts
//! with `radius * (0.7..1.3)` intensity and steps 0.3 blocks at a time,
//! losing 0.225 per step and `(resistance + 0.3) * 0.3` in every cell that
//! holds a block or fluid; a cell it still has intensity in is exploded.
//! Blast resistance is each block's `explosionResistance` (the larger of the
//! block's and its fluid's, as `ExplosionDamageCalculator` takes).
//!
//! `interactWithBlocks` breaks the cells in a shuffled order, rolling each
//! block's loot table with an empty tool and, under `DESTROY_WITH_DECAY`, the
//! explosion radius: a `survives_explosion` drop stays with chance
//! `1 / radius`, and `explosion_decay` keeps each item with that chance.
//! Drops are merged into piles of up to 16 at the first cell that dropped
//! that item (`ServerExplosion.addOrAppendStack`). With fire (beds, respawn
//! anchors and ghast fireballs under `mob_griefing`), one in three exploded
//! cells that is now air over a solid block catches fire.
//!
//! The tables below are Java's per-block values, keyed by VoxelCraft block
//! name and checked for every registered state by `tests/java_block_mining.rs`
//! and `tests/java_block_loot.rs`.

use std::sync::LazyLock;

use glam::{DVec3, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::{Block, STATE_CAPACITY};
use crate::inventory::Stack;
use crate::item::Item;

/// `ServerExplosion.calculateExplodedPositions`' ray step, `0.3f` widened.
const STEP: f64 = 0.300_000_011_920_928_96;
/// Intensity each step loses on top of the cell's resistance.
const STEP_DECAY: f32 = 0.225_000_01;
/// Fluids' `getExplosionResistance` (water and lava alike).
const FLUID_RESISTANCE: f32 = 100.0;
/// `StackCollector.tryMerge`'s cap on one pile.
const PILE: u8 = 16;

const RESISTANCE: &[(f32, &[&str])] = &[
    (0.0, &["air", "moving piston", "nether portal"]),
    (0.1, &["snow layer"]),
    (0.8, &["cut red sandstone slab", "quartz slab", "red sandstone slab"]),
    (1.0, &["dripstone block"]),
    (2.5, &["dried kelp block"]),
    (
        3.0,
        &[
            "acacia fence",
            "acacia fence gate",
            "acacia planks",
            "acacia shelf",
            "acacia slab",
            "acacia stairs",
            "bamboo fence",
            "bamboo fence gate",
            "bamboo mosaic",
            "bamboo mosaic slab",
            "bamboo mosaic stairs",
            "bamboo planks",
            "bamboo shelf",
            "bamboo slab",
            "bamboo stairs",
            "birch fence",
            "birch fence gate",
            "birch planks",
            "birch shelf",
            "birch slab",
            "birch stairs",
            "cherry fence",
            "cherry fence gate",
            "cherry planks",
            "cherry shelf",
            "cherry slab",
            "cherry stairs",
            "cocoa",
            "crimson fence",
            "crimson fence gate",
            "crimson planks",
            "crimson shelf",
            "crimson slab",
            "crimson stairs",
            "dark oak fence",
            "dark oak fence gate",
            "dark oak planks",
            "dark oak shelf",
            "dark oak slab",
            "dark oak stairs",
            "deepslate coal ore",
            "deepslate copper ore",
            "deepslate diamond ore",
            "deepslate emerald ore",
            "deepslate gold ore",
            "deepslate iron ore",
            "deepslate lapis lazuli ore",
            "deepslate redstone ore",
            "jungle fence",
            "jungle fence gate",
            "jungle planks",
            "jungle shelf",
            "jungle slab",
            "jungle stairs",
            "mangrove fence",
            "mangrove fence gate",
            "mangrove planks",
            "mangrove shelf",
            "mangrove slab",
            "mangrove stairs",
            "mud brick slab",
            "mud brick stairs",
            "mud brick wall",
            "mud bricks",
            "oak fence",
            "oak fence gate",
            "oak planks",
            "oak shelf",
            "oak slab",
            "oak stairs",
            "packed mud",
            "pale oak fence",
            "pale oak fence gate",
            "pale oak planks",
            "pale oak shelf",
            "pale oak slab",
            "pale oak stairs",
            "pointed dripstone",
            "spruce fence",
            "spruce fence gate",
            "spruce planks",
            "spruce shelf",
            "spruce slab",
            "spruce stairs",
            "warped fence",
            "warped fence gate",
            "warped planks",
            "warped shelf",
            "warped slab",
            "warped stairs",
        ],
    ),
    (
        4.2,
        &[
            "basalt",
            "black terracotta",
            "blue terracotta",
            "brown terracotta",
            "cyan terracotta",
            "gray terracotta",
            "green terracotta",
            "light blue terracotta",
            "light gray terracotta",
            "lime terracotta",
            "magenta terracotta",
            "orange terracotta",
            "pink terracotta",
            "polished basalt",
            "purple terracotta",
            "red terracotta",
            "smooth basalt",
            "terracotta",
            "white terracotta",
            "yellow terracotta",
        ],
    ),
    (4.8, &["hopper"]),
    (
        6.0,
        &[
            // VoxelCraft's name for Java's `iron_chain`.
            "chain",
            "andesite",
            "andesite slab",
            "andesite stairs",
            "andesite wall",
            "blackstone",
            "blackstone slab",
            "blackstone stairs",
            "blackstone wall",
            "block of coal",
            "block of copper",
            "block of diamond",
            "block of emerald",
            "block of gold",
            "block of iron",
            "block of raw copper",
            "block of raw gold",
            "block of raw iron",
            "block of redstone",
            "brain coral block",
            "brick slab",
            "brick stairs",
            "brick wall",
            "bricks",
            "bubble coral block",
            "chiseled copper",
            "chiseled deepslate",
            "chiseled nether bricks",
            "chiseled polished blackstone",
            "chiseled stone bricks",
            "chiseled tuff",
            "chiseled tuff bricks",
            "cobbled deepslate",
            "cobbled deepslate slab",
            "cobbled deepslate stairs",
            "cobbled deepslate wall",
            "cobblestone",
            "cobblestone slab",
            "cobblestone stairs",
            "cobblestone wall",
            "copper bars",
            "copper bulb",
            "copper chain",
            "copper chest",
            "copper door",
            "copper grate",
            "copper trapdoor",
            "cracked deepslate bricks",
            "cracked deepslate tiles",
            "cracked nether bricks",
            "cracked polished blackstone bricks",
            "cracked stone bricks",
            "cut copper",
            "cut copper slab",
            "cut copper stairs",
            "cut sandstone slab",
            "dark prismarine",
            "dark prismarine slab",
            "dark prismarine stairs",
            "dead brain coral block",
            "dead bubble coral block",
            "dead fire coral block",
            "dead horn coral block",
            "dead tube coral block",
            "deepslate",
            "deepslate brick slab",
            "deepslate brick stairs",
            "deepslate brick wall",
            "deepslate bricks",
            "deepslate tile slab",
            "deepslate tile stairs",
            "deepslate tile wall",
            "deepslate tiles",
            "diorite",
            "diorite slab",
            "diorite stairs",
            "diorite wall",
            "exposed chiseled copper",
            "exposed copper",
            "exposed copper bars",
            "exposed copper bulb",
            "exposed copper chain",
            "exposed copper chest",
            "exposed copper door",
            "exposed copper grate",
            "exposed copper trapdoor",
            "exposed cut copper",
            "exposed cut copper slab",
            "exposed cut copper stairs",
            "exposed lightning rod",
            "fire coral block",
            "gilded blackstone",
            "granite",
            "granite slab",
            "granite stairs",
            "granite wall",
            "grindstone",
            "horn coral block",
            "iron bars",
            "lightning rod",
            "mossy cobblestone",
            "mossy cobblestone slab",
            "mossy cobblestone stairs",
            "mossy cobblestone wall",
            "mossy stone brick slab",
            "mossy stone brick stairs",
            "mossy stone brick wall",
            "mossy stone bricks",
            "nether brick fence",
            "nether brick slab",
            "nether brick stairs",
            "nether brick wall",
            "nether bricks",
            "oxidized chiseled copper",
            "oxidized copper",
            "oxidized copper bars",
            "oxidized copper bulb",
            "oxidized copper chain",
            "oxidized copper chest",
            "oxidized copper door",
            "oxidized copper grate",
            "oxidized copper trapdoor",
            "oxidized cut copper",
            "oxidized cut copper slab",
            "oxidized cut copper stairs",
            "oxidized lightning rod",
            "polished andesite",
            "polished andesite slab",
            "polished andesite stairs",
            "polished blackstone",
            "polished blackstone brick slab",
            "polished blackstone brick stairs",
            "polished blackstone brick wall",
            "polished blackstone bricks",
            "polished blackstone slab",
            "polished blackstone stairs",
            "polished blackstone wall",
            "polished deepslate",
            "polished deepslate slab",
            "polished deepslate stairs",
            "polished deepslate wall",
            "polished diorite",
            "polished diorite slab",
            "polished diorite stairs",
            "polished granite",
            "polished granite slab",
            "polished granite stairs",
            "polished tuff",
            "polished tuff slab",
            "polished tuff stairs",
            "polished tuff wall",
            "prismarine",
            "prismarine brick slab",
            "prismarine brick stairs",
            "prismarine bricks",
            "prismarine slab",
            "prismarine stairs",
            "prismarine wall",
            "red nether brick slab",
            "red nether brick stairs",
            "red nether brick wall",
            "red nether bricks",
            "sandstone slab",
            "smooth quartz block",
            "smooth quartz slab",
            "smooth quartz stairs",
            "smooth red sandstone",
            "smooth red sandstone slab",
            "smooth red sandstone stairs",
            "smooth sandstone",
            "smooth sandstone slab",
            "smooth sandstone stairs",
            "smooth stone",
            "smooth stone slab",
            "stone",
            "stone brick slab",
            "stone brick stairs",
            "stone brick wall",
            "stone bricks",
            "stone slab",
            "stone stairs",
            "tube coral block",
            "tuff",
            "tuff brick slab",
            "tuff brick stairs",
            "tuff brick wall",
            "tuff bricks",
            "tuff slab",
            "tuff stairs",
            "tuff wall",
            "weathered chiseled copper",
            "weathered copper",
            "weathered copper bars",
            "weathered copper bulb",
            "weathered copper chain",
            "weathered copper chest",
            "weathered copper door",
            "weathered copper grate",
            "weathered copper trapdoor",
            "weathered cut copper",
            "weathered cut copper slab",
            "weathered cut copper stairs",
            "weathered lightning rod",
        ],
    ),
    (
        9.0,
        &[
            "dragon egg",
            "end stone",
            "end stone brick slab",
            "end stone brick stairs",
            "end stone brick wall",
            "end stone bricks",
        ],
    ),
    (100.0, &["lava", "water"]),
    (600.0, &["ender chest"]),
    (
        1200.0,
        &[
            "ancient debris",
            "anvil",
            "block of netherite",
            "chipped anvil",
            "crying obsidian",
            "damaged anvil",
            "enchanting table",
            "obsidian",
            "respawn anchor",
        ],
    ),
    (3_600_000.0, &["bedrock", "end gateway", "end portal", "end portal frame"]),
];
const DECAY: &[&str] = &[
    "leaf litter",
    "wildflowers",
    "acacia slab",
    "amethyst cluster",
    "andesite slab",
    "bamboo mosaic slab",
    "bamboo slab",
    "birch slab",
    "black candle",
    "black concrete slab",
    "black wool slab",
    "blackstone slab",
    "blue candle",
    "blue concrete slab",
    "blue wool slab",
    "bookshelf",
    "brick slab",
    "brown candle",
    "brown concrete slab",
    "brown mushroom block",
    "brown wool slab",
    "candle",
    "cherry slab",
    "clay",
    "coal ore",
    "cobbled deepslate slab",
    "cobblestone slab",
    "cocoa",
    "composter",
    "copper ore",
    "crimson slab",
    "cut copper slab",
    "cut red sandstone slab",
    "cut sandstone slab",
    "cyan candle",
    "cyan concrete slab",
    "cyan wool slab",
    "dark oak slab",
    "dark prismarine slab",
    "dead bush",
    "deepslate brick slab",
    "deepslate coal ore",
    "deepslate copper ore",
    "deepslate diamond ore",
    "deepslate emerald ore",
    "deepslate gold ore",
    "deepslate iron ore",
    "deepslate lapis lazuli ore",
    "deepslate redstone ore",
    "deepslate tile slab",
    "diamond ore",
    "diorite slab",
    "emerald ore",
    "end stone brick slab",
    "ender chest",
    "exposed cut copper slab",
    "fern",
    "glowstone",
    "gold ore",
    "granite slab",
    "gray candle",
    "gray concrete slab",
    "gray wool slab",
    "green candle",
    "green concrete slab",
    "green wool slab",
    "iron ore",
    "jungle slab",
    "lapis lazuli ore",
    "light blue candle",
    "light blue concrete slab",
    "light blue wool slab",
    "light gray candle",
    "light gray concrete slab",
    "light gray wool slab",
    "lime candle",
    "lime concrete slab",
    "lime wool slab",
    "magenta candle",
    "magenta concrete slab",
    "magenta wool slab",
    "mangrove leaves",
    "mangrove propagule",
    "mangrove slab",
    "melon",
    "mossy cobblestone slab",
    "mossy stone brick slab",
    "mud brick slab",
    "nether brick slab",
    "nether quartz ore",
    "nether wart",
    "oak slab",
    "orange candle",
    "orange concrete slab",
    "orange wool slab",
    "oxidized cut copper slab",
    "pale moss carpet",
    "pale oak slab",
    "pink candle",
    "pink concrete slab",
    "pink petals",
    "pink wool slab",
    "polished andesite slab",
    "polished blackstone brick slab",
    "polished blackstone slab",
    "polished deepslate slab",
    "polished diorite slab",
    "polished granite slab",
    "polished tuff slab",
    "prismarine brick slab",
    "prismarine slab",
    "purple candle",
    "purple concrete slab",
    "purple wool slab",
    "quartz slab",
    "red candle",
    "red concrete slab",
    "red mushroom block",
    "red nether brick slab",
    "red sandstone slab",
    "red wool slab",
    "redstone ore",
    "sandstone slab",
    "sea lantern",
    "sea pickle",
    "smooth quartz slab",
    "smooth red sandstone slab",
    "smooth sandstone slab",
    "smooth stone slab",
    "spruce slab",
    "stone brick slab",
    "stone slab",
    "sweet berry bush",
    "tuff brick slab",
    "tuff slab",
    "warped slab",
    "weathered cut copper slab",
    "wheat crops",
    // VoxelCraft names for Java's `carrots`, `potatoes` and `snow_block`.
    "carrot crops",
    "potato crops",
    "snow",
    "white candle",
    "white concrete slab",
    "white wool slab",
    "yellow candle",
    "yellow concrete slab",
    "yellow wool slab",
];
const KEEP: &[&str] = &[
    "bush",
    "short dry grass",
    "tall dry grass",
    "tall dry grass top",
    "cake with black candle",
    "cake with blue candle",
    "cake with brown candle",
    "cake with candle",
    "cake with cyan candle",
    "cake with gray candle",
    "cake with green candle",
    "cake with light blue candle",
    "cake with light gray candle",
    "cake with lime candle",
    "cake with magenta candle",
    "cake with orange candle",
    "cake with pink candle",
    "cake with purple candle",
    "cake with red candle",
    "cake with white candle",
    "cake with yellow candle",
    "cave vines",
    "cave vines plant",
    "decorated pot",
    "dragon egg",
    "snow layer",
    "twisting vines",
    "twisting vines plant",
    "weeping vines",
    "weeping vines plant",
];

/// How a block's loot table treats an explosion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loot {
    /// `survives_explosion`: each drop stays whole with chance `1 / radius`.
    Survives,
    /// `explosion_decay`: each item stays with chance `1 / radius`.
    Decay,
    /// Neither: the drops are those of an empty-handed break.
    Keep,
    /// Leaves: the sapling and apple must survive, the sticks decay.
    Leaves,
}

struct Tables {
    resistance: Box<[f32]>,
    loot: Box<[Loot]>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut resistance: FxHashMap<&str, f32> = FxHashMap::default();
    for &(value, names) in RESISTANCE {
        for &name in names {
            resistance.insert(name, value);
        }
    }
    let mut loot: FxHashMap<&str, Loot> = FxHashMap::default();
    loot.extend(DECAY.iter().map(|&n| (n, Loot::Decay)));
    loot.extend(KEEP.iter().map(|&n| (n, Loot::Keep)));
    let mut tables = Tables {
        resistance: vec![0.0; STATE_CAPACITY].into_boxed_slice(),
        loot: vec![Loot::Survives; STATE_CAPACITY].into_boxed_slice(),
    };
    for i in 0..STATE_CAPACITY {
        let b = Block(i as u16);
        let name = b.name();
        tables.resistance[i] = resistance.get(name).copied().unwrap_or_else(|| b.hardness());
        tables.loot[i] = match loot.get(name) {
            Some(&kind) => kind,
            None if b.is_leaves() => Loot::Leaves,
            None => Loot::Survives,
        };
    }
    tables
});

/// Builds the per-state tables now (about 15 ms once) instead of on the
/// first blast.
pub fn prepare() {
    LazyLock::force(&TABLES);
}

/// Java's `explosionResistance` for a block state.
pub fn resistance(b: Block) -> f32 {
    TABLES.resistance.get(b.0 as usize).copied().unwrap_or(0.0)
}

/// `ExplosionDamageCalculator.getBlockExplosionResistance`: nothing for an
/// empty cell, else the larger of the block's and its fluid's resistance.
fn resistance_against(b: Block) -> Option<f32> {
    if b == Block::AIR {
        return None;
    }
    let fluid = if b.is_fluid() || b.holds_water() { FLUID_RESISTANCE } else { 0.0 };
    Some(resistance(b).max(fluid))
}

impl World {
    /// `ServerExplosion.calculateExplodedPositions`, shuffled as
    /// `interactWithBlocks` breaks them. Rays stop at the world's top and
    /// bottom and at unloaded chunks. Exploded cells may be air.
    pub fn exploded_cells(&mut self, center: DVec3, radius: f32) -> Vec<IVec3> {
        let mut cells = FxHashSet::default();
        let edge = |i: i32| i == 0 || i == 15;
        let axis = |i: i32| f64::from(i as f32 / 15.0 * 2.0 - 1.0);
        for i in 0..16 {
            for j in 0..16 {
                for k in 0..16 {
                    if !(edge(i) || edge(j) || edge(k)) {
                        continue;
                    }
                    let d = DVec3::new(axis(i), axis(j), axis(k));
                    let d = d / d.length();
                    let mut intensity = radius * (0.7 + self.next_float() * 0.6);
                    let mut pos = center;
                    while intensity > 0.0 {
                        let cell = pos.floor().as_ivec3();
                        if !self.contains_y(cell.y) {
                            break;
                        }
                        let Some(b) = self.get_block(cell) else { break };
                        if let Some(r) = resistance_against(b) {
                            intensity -= (r + 0.3) * 0.3;
                        }
                        if intensity > 0.0 {
                            cells.insert(cell);
                        }
                        pos += d * STEP;
                        intensity -= STEP_DECAY;
                    }
                }
            }
        }
        let mut cells: Vec<IVec3> = cells.into_iter().collect();
        cells.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        // `Util.shuffle`: Fisher-Yates from the end.
        for i in (1..cells.len()).rev() {
            let j = (self.roll() % (i as u64 + 1)) as usize;
            cells.swap(i, j);
        }
        cells
    }

    /// Breaks the exploded `cells` (`interactWithBlocks`, then `createFire`
    /// with `fire`). `decay` is `DESTROY_WITH_DECAY`: drops are rolled with
    /// the explosion `radius`; without it every drop stays. TNT is primed
    /// instead of dropped. Returns how many blocks were destroyed.
    pub fn detonate(&mut self, cells: &[IVec3], radius: f32, drop_blocks: bool, decay: bool, fire: bool) -> usize {
        let mut piles: Vec<(IVec3, Stack)> = Vec::new();
        let mut removed = Vec::new();
        for &p in cells {
            let Some(b) = self.get_block(p).filter(|&b| b != Block::AIR) else { continue };
            if b == Block::TNT {
                // `TntBlock.wasExploded` primes it with `tnt_explodes`;
                // `dropFromExplosion` is false either way.
                if self.edit(p, Block::AIR, false) {
                    removed.push(p);
                    if self.rules.tnt_explodes {
                        self.primed_tnt.push((p, true));
                    }
                }
                continue;
            }
            let start = self.drops.len();
            if drop_blocks {
                self.spill_exploded(p, b, decay.then_some(radius));
            }
            let dropped: Vec<Stack> = self.drops.drain(start..).map(|(_, s)| s).collect();
            if self.edit(p, Block::AIR, false) {
                removed.push(p);
                for stack in dropped {
                    add_or_append(&mut piles, stack, p);
                }
            }
        }
        self.drops.extend(piles);
        for &p in &removed {
            self.settle(p);
        }
        if fire {
            for &p in cells {
                if self.roll().is_multiple_of(3)
                    && self.get_block(p) == Some(Block::AIR)
                    && self.get_block(p - IVec3::Y).is_some_and(Block::is_opaque)
                {
                    let flame = self.fire_state(p, 0);
                    self.edit(p, flame, false);
                }
            }
        }
        self.update_block_light();
        removed.len()
    }

    /// The drops of `block` at `p` caught in an explosion: an empty-handed
    /// break's loot, thinned by its table's explosion rules when `radius` is
    /// given (`DESTROY_WITH_DECAY`). Pushed to [`World::drops`] unmerged.
    pub fn spill_exploded(&mut self, p: IVec3, block: Block, radius: Option<f32>) {
        let start = self.drops.len();
        self.spill_with_item(p, block, None);
        let Some(radius) = radius else { return };
        let chance = 1.0 / radius;
        let kind = TABLES.loot[block.canonical().0 as usize];
        // One loot pool gives one stack (a campfire's two charcoal survive
        // together); VoxelCraft may hand out its items in parts.
        let mut mined: Vec<(IVec3, Stack)> = Vec::new();
        for (at, stack) in self.drops.drain(start..).collect::<Vec<_>>() {
            match mined.iter_mut().find(|(_, s)| s.stacks_with(&stack)) {
                Some((_, s)) => s.count += stack.count,
                None => mined.push((at, stack)),
            }
        }
        for (at, mut stack) in mined {
            let rule = match kind {
                Loot::Leaves if stack.item == Item::STICK => Loot::Decay,
                Loot::Leaves => Loot::Survives,
                // A full composter's bone meal comes from a pool with no
                // explosion rule; the composter itself decays.
                Loot::Decay if stack.item == Item::BONE_MEAL && super::composter::level(block).is_some() => Loot::Keep,
                kind => kind,
            };
            match rule {
                Loot::Survives if self.next_float() > chance => continue,
                Loot::Decay => {
                    stack.count = (0..stack.count).filter(|_| self.next_float() <= chance).count() as u8;
                    if stack.count == 0 {
                        continue;
                    }
                }
                _ => {}
            }
            self.drops.push((at, stack));
        }
    }
}

/// `ServerExplosion.addOrAppendStack`: tops up earlier piles of the same
/// item (`ItemEntity.areMergable`: only when the two fit in one stack) to
/// at most 16, then starts a pile at `p` with what is left.
fn add_or_append(piles: &mut Vec<(IVec3, Stack)>, mut stack: Stack, p: IVec3) {
    let max = stack.item.max_stack();
    for (_, pile) in piles.iter_mut() {
        if pile.stacks_with(&stack) && u32::from(pile.count) + u32::from(stack.count) <= u32::from(max) {
            let moved = max.min(PILE).saturating_sub(pile.count).min(stack.count);
            pile.count += moved;
            stack.count -= moved;
            if stack.count == 0 {
                return;
            }
        }
    }
    piles.push((p, stack));
}

/// `ServerExplosion.getSeenPercent`: the share of points spread over the box
/// `min..max` with a clear line to `center`, clipping collision shapes and
/// ignoring fluids.
pub fn seen_percent<W: crate::physics::BlockSource + ?Sized>(world: &W, center: DVec3, min: DVec3, max: DVec3) -> f32 {
    let size = max - min;
    let step = DVec3::ONE / (size * 2.0 + DVec3::ONE);
    if step.min_element() < 0.0 {
        return 0.0;
    }
    let offset_x = (1.0 - (1.0 / step.x).floor() * step.x) / 2.0;
    let offset_z = (1.0 - (1.0 / step.z).floor() * step.z) / 2.0;
    let (mut seen, mut total) = (0u32, 0u32);
    let mut fx = 0.0;
    while fx <= 1.0 {
        let mut fy = 0.0;
        while fy <= 1.0 {
            let mut fz = 0.0;
            while fz <= 1.0 {
                let x = min.x + fx * size.x;
                let y = min.y + fy * size.y;
                let z = min.z + fz * size.z;
                let from = DVec3::new(x + offset_x, y, z + offset_z);
                if crate::entity::clear_line(world, from, center) {
                    seen += 1;
                }
                total += 1;
                fz += step.z;
            }
            fy += step.y;
        }
        fx += step.x;
    }
    seen as f32 / total as f32
}

#[cfg(test)]
#[path = "explosion_tests.rs"]
mod tests;
