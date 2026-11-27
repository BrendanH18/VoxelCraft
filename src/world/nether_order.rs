//! Generation 7 (v1.0 gap Track 5, GEN-04a): the Nether's features in
//! Java's decoration order on decorated terrain.
//!
//! Java decorates one 16 × 16 chunk at a time
//! (`ChunkGenerator.applyBiomeDecoration`) over a `WorldGenRegion` of the
//! 3 × 3 chunks around it, which it may write (`FEATURES` has a block-state
//! write radius of 1; writes beyond it are dropped). Step by step, it runs
//! every placed feature that a biome of those nine chunks lists, in the
//! global order `FeatureSorter` gives the dimension's biomes ([`JAVA_ORDER`]).
//! Each placed feature runs its modifiers depth first as `FeaturePlacer`
//! does, reading the region as earlier features left it, so later features
//! see earlier ones: plants grow on deltas' magma, ores replace only the
//! netherrack blobs left, glowstone spreads from glowstone of either
//! feature, vines and fungi skip what fire or roots took.
//!
//! Here every Java chunk (a cell) is decorated once, over its own region
//! of undecorated terrain, and what it changed is kept in a bounded cache.
//! A VoxelCraft chunk takes the changes of the sixteen cells whose regions
//! reach it, in a fixed order (by z, then x) where the first cell to change
//! a block wins. That matches Java exactly inside each cell's region except
//! where two regions overlap: Java's result there depends on which chunk
//! the server decorated first, which depends on how the player moved, and
//! the later chunk sees the earlier one's blocks. Here neither cell sees
//! the other's, and the earlier cell's block is kept (a later Java feature
//! rarely replaces another's: ores take only netherrack, plants only air).
//!
//! Structures still paint after the features and ancient debris keeps its
//! own pass (`NetherGen::generate`), as in generation 6.

use std::cell::Cell;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, index};
use super::nether::{NetherGen, ROOF};
use super::nether_decoration::Probe;
use super::nether_features::{self as features, Level};
use super::nether_placed::{self as placed, Feature, Mod, PLACED, Placed};
use super::noise::hash3;
use super::structure::Rng;

/// A Java chunk.
const CELL: i32 = 16;
/// A cell's region: the 3 × 3 cells around it.
const SPAN: i32 = 3 * CELL;
/// Heights a cell writes: the Nether's 128, and the roof's top (mushrooms).
const HEIGHT: i32 = ROOF + 2;
/// The cache keeps at most this many changed blocks (8 bytes each).
const CACHE_LIMIT: usize = 6_000_000;
const SALT: u64 = 0xDEC0_0707;
/// A block the region hasn't read yet.
const UNREAD: Block = Block(u16::MAX);

/// The Nether's placed features in Java 26.3's decoration order: by step,
/// then `FeatureSorter.buildFeaturesPerStep` over the biome source's
/// biomes (wastes, soul sand valley, crimson, warped, basalt deltas, the
/// `nether` preset's order). `nether_order_tests` sorts the fixture's
/// biome lists the same way and compares.
pub const JAVA_ORDER: [&str; 37] = [
    "basalt_pillar",
    "delta",
    "small_basalt_columns",
    "large_basalt_columns",
    "basalt_blobs",
    "blackstone_blobs",
    "spring_delta",
    "spring_open",
    "patch_fire",
    "patch_soul_fire",
    "glowstone_extra",
    "glowstone",
    "patch_crimson_roots",
    "brown_mushroom_nether",
    "red_mushroom_nether",
    "ore_magma",
    "spring_closed_double",
    "ore_gold_deltas",
    "ore_quartz_deltas",
    "spring_closed",
    "ore_soul_sand",
    "ore_gravel_nether",
    "ore_blackstone",
    "ore_gold_nether",
    "ore_quartz_nether",
    "ore_ancient_debris_large",
    "ore_debris_small",
    "spring_lava",
    "brown_mushroom_normal",
    "red_mushroom_normal",
    "warped_fungi",
    "warped_forest_vegetation",
    "nether_sprouts",
    "twisting_vines",
    "weeping_vines",
    "crimson_fungi",
    "crimson_forest_vegetation",
];

/// [`JAVA_ORDER`] as indices into [`PLACED`].
fn order() -> &'static [usize] {
    static ORDER: OnceLock<Vec<usize>> = OnceLock::new();
    ORDER.get_or_init(|| {
        JAVA_ORDER
            .iter()
            .map(|n| PLACED.iter().position(|p| p.name == *n).unwrap_or_else(|| panic!("no placed feature {n}")))
            .collect()
    })
}

/// What decorating one cell changed: per 32-block row of heights, each
/// changed block as its position in the region and its new block, and
/// where its lava springs went.
pub(super) struct CellDiff {
    rows: [Vec<(u32, Block)>; 5],
    springs: Vec<IVec3>,
}

impl CellDiff {
    fn len(&self) -> usize {
        self.rows.iter().map(Vec::len).sum::<usize>() + self.springs.len()
    }
}

/// Decorated cells shared by every chunk, bounded by [`CACHE_LIMIT`].
#[derive(Default)]
pub(super) struct CellCache {
    inner: Mutex<(FxHashMap<IVec2, Arc<CellDiff>>, usize)>,
}

fn key(l: IVec3) -> u32 {
    (l.y as u32) << 12 | (l.z as u32) << 6 | l.x as u32
}

fn unkey(k: u32) -> IVec3 {
    IVec3::new((k & 63) as i32, (k >> 12) as i32, (k >> 6 & 63) as i32)
}

/// The first block of the region of cell `c`.
fn region_min(c: IVec2) -> IVec2 {
    (c - 1) * CELL
}

/// The cell decorated, from the cache or decorated now.
fn cell(nether: &NetherGen, c: IVec2) -> Arc<CellDiff> {
    if let Some(d) = nether.cells.inner.lock().unwrap().0.get(&c) {
        return Arc::clone(d);
    }
    // Decorate outside the lock so workers fill different cells at once.
    let diff = Arc::new(decorate_cell(nether, c));
    let mut guard = nether.cells.inner.lock().unwrap();
    let (map, total) = &mut *guard;
    if let Some(found) = map.get(&c) {
        return Arc::clone(found);
    }
    while *total + diff.len() > CACHE_LIMIT
        && let Some(k) = map.keys().next().copied()
    {
        let gone = map.remove(&k).expect("a cached cell");
        *total -= gone.len();
    }
    *total += diff.len();
    Arc::clone(map.entry(c).or_insert(diff))
}

/// The cells whose regions reach the chunk at `base`, in the order their
/// changes apply.
fn cells_reaching(base: IVec3) -> impl Iterator<Item = IVec2> {
    let lo = IVec2::new(base.x, base.z).div_euclid(IVec2::splat(CELL)) - 1;
    let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I - 1).div_euclid(IVec2::splat(CELL)) + 1;
    (lo.y..=hi.y).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec2::new(x, z)))
}

/// Paints the features of every cell that reaches the chunk at `base`.
pub(super) fn decorate(nether: &NetherGen, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    let row = base.y.div_euclid(CHUNK_SIZE_I);
    if !(0..5).contains(&row) {
        return;
    }
    let mut touched = vec![0u64; CHUNK_VOLUME / 64];
    for c in cells_reaching(base) {
        let diff = cell(nether, c);
        let min = region_min(c);
        let offset = IVec3::new(min.x - base.x, -base.y, min.y - base.z);
        for &(k, block) in &diff.rows[row as usize] {
            let l = unkey(k) + offset;
            if l.cmplt(IVec3::ZERO).any() || l.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
                continue;
            }
            let i = index(l.x as usize, l.y as usize, l.z as usize);
            if touched[i / 64] >> (i % 64) & 1 == 0 {
                touched[i / 64] |= 1 << (i % 64);
                blocks[i] = block;
            }
        }
    }
}

/// The springs a generated chunk holds, to schedule their first fluid tick
/// (Java's `SpringFeature` schedules one as it places the lava).
pub fn spring_ticks(nether: &NetherGen, cpos: IVec3, data: &ChunkData) -> Vec<IVec3> {
    let base = cpos * CHUNK_SIZE_I;
    if !(0..5).contains(&cpos.y) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in cells_reaching(base) {
        for &p in &cell(nether, c).springs {
            let l = p - base;
            if l.cmpge(IVec3::ZERO).all()
                && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all()
                && data.get(l.x as usize, l.y as usize, l.z as usize) == Block::LAVA
                && !out.contains(&p)
            {
                out.push(p);
            }
        }
    }
    out
}

/// One cell's `WorldGenRegion`: the terrain of its 3 × 3 cells, read as
/// needed, with the features' writes.
struct Region<'a> {
    probe: Probe<'a>,
    min: IVec2,
    /// The region's blocks as read or written so far ([`UNREAD`] before).
    blocks: Vec<Cell<Block>>,
    /// Which blocks a feature wrote...
    dirty: Vec<u64>,
    /// ...and, in order, what each held before its first write.
    written: Vec<(u32, Block)>,
    springs: Vec<IVec3>,
}

impl Region<'_> {
    fn local(&self, p: IVec3) -> Option<IVec3> {
        let l = IVec3::new(p.x - self.min.x, p.y, p.z - self.min.y);
        (l.cmpge(IVec3::ZERO).all() && l.x < SPAN && l.z < SPAN && l.y < HEIGHT).then_some(l)
    }

    fn slot(l: IVec3) -> usize {
        ((l.y * SPAN + l.z) * SPAN + l.x) as usize
    }

    /// Block `l` (world position `p`), read from the terrain the first time.
    fn read(&self, l: IVec3, p: IVec3) -> Block {
        let cell = &self.blocks[Self::slot(l)];
        let b = cell.get();
        if b != UNREAD {
            return b;
        }
        let b = self.probe.terrain(p);
        cell.set(b);
        b
    }

    /// Java's `MOTION_BLOCKING` heightmap: one above the highest block that
    /// blocks motion or holds a fluid (above the bedrock roof here).
    fn motion_blocking(&mut self, x: i32, z: i32) -> i32 {
        (0..HEIGHT)
            .rev()
            .find(|&y| {
                let b = self.block(IVec3::new(x, y, z));
                b.is_solid() || b.is_fluid()
            })
            .map_or(0, |y| y + 1)
    }

    /// `CountOnEveryLayerPlacement.findOnGroundYPosition`: down from `top`,
    /// the `layer`-th open (air or lava) block above a block that is
    /// neither open nor bedrock.
    fn on_ground(&mut self, x: i32, top: i32, z: i32, layer: u32) -> Option<i32> {
        let open = |b: Block| b == Block::AIR || b.is_lava();
        let mut seen = 0;
        let mut current = self.block(IVec3::new(x, top, z));
        for y in (1..=top).rev() {
            let below = self.block(IVec3::new(x, y - 1, z));
            if !open(below) && open(current) && below != Block::BEDROCK {
                if seen == layer {
                    return Some(y);
                }
                seen += 1;
            }
            current = below;
        }
        None
    }
}

impl Level for Region<'_> {
    fn block(&self, p: IVec3) -> Block {
        match self.local(p) {
            Some(l) => self.read(l, p),
            None if p.y > ROOF => Block::AIR,
            None => self.probe.terrain(p),
        }
    }

    fn place(&mut self, p: IVec3, b: Block) {
        // Java drops writes outside the region (`ensureCanWrite`).
        let Some(l) = self.local(p) else { return };
        let i = Self::slot(l);
        if self.dirty[i / 64] >> (i % 64) & 1 == 0 {
            self.dirty[i / 64] |= 1 << (i % 64);
            let before = self.read(l, p);
            self.written.push((key(l), before));
        }
        self.blocks[i].set(b);
    }

    fn write_bounds(&self) -> Option<(IVec3, IVec3)> {
        Some((
            IVec3::new(self.min.x, 0, self.min.y),
            IVec3::new(self.min.x + SPAN - 1, HEIGHT - 1, self.min.y + SPAN - 1),
        ))
    }
}

/// Decorates cell `c` as Java decorates a chunk.
fn decorate_cell(nether: &NetherGen, c: IVec2) -> CellDiff {
    let min = region_min(c);
    let probe = Probe::around(nether, min, min + SPAN - 1);
    // The biomes of the nine chunks (Java's `possibleBiomes`).
    let mut present = [false; 5];
    for qz in (min.y..min.y + SPAN).step_by(4) {
        for qx in (min.x..min.x + SPAN).step_by(4) {
            present[probe.biome(qx, qz) as usize] = true;
        }
    }
    let volume = (SPAN * SPAN * HEIGHT) as usize;
    let mut region = Region {
        probe,
        min,
        blocks: (0..volume).map(|_| Cell::new(UNREAD)).collect(),
        dirty: vec![0; volume.div_ceil(64)],
        written: Vec::new(),
        springs: Vec::new(),
    };
    let origin = IVec3::new(c.x * CELL, 0, c.y * CELL);
    for &i in order() {
        let entry = &PLACED[i];
        if !entry.feature.runs() || !entry.biomes.iter().any(|&b| present[b as usize]) {
            continue;
        }
        let mut rng = Rng(hash3(c.x, i32::from(entry.step) << 8 | i as i32, c.y, nether.seed() ^ SALT));
        place_with_modifiers(&mut region, entry, origin, &mut rng);
    }
    let mut rows: [Vec<(u32, Block)>; 5] = Default::default();
    let mut written = std::mem::take(&mut region.written);
    written.sort_unstable_by_key(|&(k, _)| k);
    for (k, before) in written {
        let l = unkey(k);
        let now = region.blocks[Region::slot(l)].get();
        if now != before {
            rows[(l.y / CHUNK_SIZE_I) as usize].push((k, now));
        }
    }
    CellDiff { rows, springs: region.springs }
}

/// `FeaturePlacer.place`: each modifier maps a position to zero or more,
/// all of them before any is taken further; positions go on depth first,
/// in order, and the feature places at each that passes every modifier.
fn place_with_modifiers(region: &mut Region, entry: &Placed, origin: IVec3, rng: &mut Rng) {
    let mut stack = vec![(origin, 0usize)];
    let mut out = Vec::new();
    while let Some((p, k)) = stack.pop() {
        out.clear();
        modify(region, entry, entry.mods[k], p, rng, &mut out);
        if k + 1 < entry.mods.len() {
            stack.extend(out.iter().rev().map(|&q| (q, k + 1)));
        } else {
            for &q in &out {
                place(region, entry.feature, q, rng);
            }
        }
    }
}

fn modify(region: &mut Region, entry: &Placed, m: Mod, p: IVec3, rng: &mut Rng, out: &mut Vec<IVec3>) {
    match m {
        Mod::Count(n) => {
            for _ in 0..n.sample(rng).max(0) {
                out.push(p);
            }
        }
        Mod::CountOnEveryLayer(count) => {
            for layer in 0.. {
                let mut found = false;
                for _ in 0..count {
                    let x = rng.below(16) as i32 + p.x;
                    let z = rng.below(16) as i32 + p.z;
                    let top = region.motion_blocking(x, z);
                    if let Some(y) = region.on_ground(x, top, z, layer) {
                        out.push(IVec3::new(x, y, z));
                        found = true;
                    }
                }
                if !found {
                    break;
                }
            }
        }
        Mod::InSquare => {
            let x = rng.below(16) as i32 + p.x;
            let z = rng.below(16) as i32 + p.z;
            out.push(IVec3::new(x, p.y, z));
        }
        Mod::Height(h) => out.push(IVec3::new(p.x, h.sample(rng), p.z)),
        Mod::MotionBlocking => out.push(IVec3::new(p.x, region.motion_blocking(p.x, p.z), p.z)),
        Mod::Biome => {
            if placed::in_biome(entry, region.probe.biome(p.x, p.z)) {
                out.push(p);
            }
        }
        Mod::Rarity(n) => {
            if (rng.unit() as f32) < 1.0 / n as f32 {
                out.push(p);
            }
        }
        Mod::Offset(x, y, z) => {
            let d = IVec3::new(x.sample(rng), y.sample(rng), z.sample(rng));
            out.push(p + d);
        }
        Mod::Filter(pred) => {
            if pred.test(&|q| region.block(q), p) {
                out.push(p);
            }
        }
        Mod::ScanDown(steps, target) => {
            // `EnvironmentScanPlacement` with no search condition.
            let block = |q: IVec3| region.block(q);
            let mut q = p;
            for _ in 0..steps {
                if target.test(&block, q) {
                    out.push(q);
                    return;
                }
                q.y -= 1;
                if q.y < 0 {
                    return;
                }
            }
            if target.test(&block, q) {
                out.push(q);
            }
        }
    }
}

fn place(region: &mut Region, feature: Feature, p: IVec3, rng: &mut Rng) {
    match feature {
        Feature::Spring { valid, requires_below, rock, hole } => {
            let valid: Vec<Block> = valid.iter().map(|id| placed::java_block(id)).collect();
            if features::spring(region, p, &valid, requires_below, rock, hole) && region.local(p).is_some() {
                region.springs.push(p);
            }
        }
        Feature::SimpleBlock(what) => {
            features::simple(region, rng, p, what);
        }
        Feature::Glowstone => {
            features::glowstone_spread(region, rng, p);
        }
        Feature::Ore { size, state } => {
            features::ore(region, rng, p, size, placed::java_block(state));
        }
        Feature::Blobs { state, radius } => {
            features::blobs(region, rng, p, placed::java_block(state), radius);
        }
        Feature::HugeFungus(wood) => {
            features::huge_fungus(region, rng, p, wood, false);
        }
        Feature::WeepingVines => {
            features::weeping_vines_26(region, rng, p);
        }
        Feature::TwistingVines => {
            features::twisting_vines_26(region, rng, p);
        }
        Feature::Delta => {
            features::delta(region, rng, p - IVec3::Y);
        }
        Feature::Columns { large } => {
            features::basalt_columns(region, rng, p, large);
        }
        Feature::BasaltPillar => {
            features::basalt_pillar(region, rng, p);
        }
        Feature::Debris { .. } | Feature::Inert => {}
    }
}

#[cfg(test)]
#[path = "nether_order_tests.rs"]
mod nether_order_tests;
