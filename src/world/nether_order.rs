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
//! Structures are placed in their own steps as Java places them (v1.0 gap
//! Track 5b): before each step's features, the structures of that step,
//! in registry order ([`STRUCTURES`]), each piece clipped to the cell being
//! decorated (`getWritableArea`: its 16 × 16 columns from y 1); a ruined
//! portal goes whole into the cell holding its middle
//! (`RuinedPortalPiece.postProcess`). Structures read the region as earlier
//! steps left it (fortress pillars stop on basalt pillars, a portal's
//! processors see the bastion before it) and later features read them
//! (glowstone hangs from bastion blackstone, ores and fire skip their
//! bricks). Where regions overlap, a block a structure changed wins over
//! the other cells' changes: Java's result when the structure's chunk was
//! decorated after the others, as its pieces replace what they cover.
//! Ancient debris is the step 7 `scattered_ore` it is in Java, in order.

use std::cell::Cell;
use std::collections::VecDeque;
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
use super::ruined_portal::protected;
use super::structure::Rng;

/// A Java chunk.
const CELL: i32 = 16;
/// A cell's region: the 3 × 3 cells around it.
const SPAN: i32 = 3 * CELL;
/// Heights a cell writes: the Nether's 128, and the roof's top (mushrooms).
const HEIGHT: i32 = ROOF + 2;
/// The cache keeps at most this many changed blocks (8 bytes each, so 32
/// MB): a basalt delta cell changes about 17,000 (its blobs), a cell of
/// another biome 400 to 2,000.
const CACHE_LIMIT: usize = 4_000_000;
const SALT: u64 = 0xDEC0_0707;
/// A block the region hasn't read yet.
const UNREAD: Block = Block(u16::MAX);
/// Marks a change a structure made in a [`CellDiff`] key.
const CLAIMED: u32 = 1 << 31;

/// The Nether's structures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Structure {
    BastionRemnant,
    Fortress,
    NetherFossil,
    RuinedPortalNether,
}

/// The Nether's structures with their decoration steps, in the order
/// `applyBiomeDecoration` places a step's structures: the structure
/// registry's, which `RegistryLoadTask` registers sorted by id. Step 4 is
/// `surface_structures`, 7 `underground_decoration`.
pub const STRUCTURES: [(&str, u8, Structure); 4] = [
    ("bastion_remnant", 4, Structure::BastionRemnant),
    ("fortress", 7, Structure::Fortress),
    ("nether_fossil", 7, Structure::NetherFossil),
    ("ruined_portal_nether", 4, Structure::RuinedPortalNether),
];
/// The last decoration step (`top_layer_modification`).
const LAST_STEP: u8 = 10;

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
/// changed block as its position in the region ([`CLAIMED`] set where a
/// structure changed it) and its new block, where its lava springs went
/// and the chests of the ruined portals it placed.
pub(super) struct CellDiff {
    rows: [Vec<(u32, Block)>; 5],
    springs: Vec<IVec3>,
    chests: Vec<(IVec3, super::fortress::Feature)>,
}

impl CellDiff {
    fn len(&self) -> usize {
        self.rows.iter().map(Vec::len).sum::<usize>() + self.springs.len() + self.chests.len()
    }
}

/// Decorated cells shared by every chunk, the oldest dropped first once
/// they hold more than [`CACHE_LIMIT`] changes.
#[derive(Default)]
pub(super) struct CellCache {
    inner: Mutex<Cached>,
}

#[derive(Default)]
struct Cached {
    cells: FxHashMap<IVec2, Arc<CellDiff>>,
    added: VecDeque<IVec2>,
    total: usize,
}

fn key(l: IVec3) -> u32 {
    (l.y as u32) << 12 | (l.z as u32) << 6 | l.x as u32
}

fn unkey(k: u32) -> IVec3 {
    IVec3::new((k & 63) as i32, (k & !CLAIMED) as i32 >> 12, (k >> 6 & 63) as i32)
}

/// The first block of the region of cell `c`.
fn region_min(c: IVec2) -> IVec2 {
    (c - 1) * CELL
}

/// The cell decorated, from the cache or decorated now.
fn cell(nether: &NetherGen, c: IVec2) -> Arc<CellDiff> {
    if let Some(d) = nether.cells.inner.lock().unwrap().cells.get(&c) {
        return Arc::clone(d);
    }
    // Decorate outside the lock so workers fill different cells at once.
    let diff = Arc::new(decorate_cell(nether, c));
    let mut guard = nether.cells.inner.lock().unwrap();
    let cache = &mut *guard;
    if let Some(found) = cache.cells.get(&c) {
        return Arc::clone(found);
    }
    while cache.total + diff.len() > CACHE_LIMIT
        && let Some(old) = cache.added.pop_front()
    {
        if let Some(gone) = cache.cells.remove(&old) {
            cache.total -= gone.len();
        }
    }
    cache.total += diff.len();
    cache.added.push_back(c);
    Arc::clone(cache.cells.entry(c).or_insert(diff))
}

/// The cells whose regions reach the chunk at `base`, in the order their
/// changes apply.
fn cells_reaching(base: IVec3) -> impl Iterator<Item = IVec2> {
    let lo = IVec2::new(base.x, base.z).div_euclid(IVec2::splat(CELL)) - 1;
    let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I - 1).div_euclid(IVec2::splat(CELL)) + 1;
    (lo.y..=hi.y).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec2::new(x, z)))
}

/// Paints the features and structures of every cell that reaches the
/// chunk at `base`: first the blocks structures changed, then the rest.
pub(super) fn decorate(nether: &NetherGen, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    let row = base.y.div_euclid(CHUNK_SIZE_I);
    if !(0..5).contains(&row) {
        return;
    }
    let mut touched = vec![0u64; CHUNK_VOLUME / 64];
    let cells: Vec<(IVec2, Arc<CellDiff>)> = cells_reaching(base).map(|c| (c, cell(nether, c))).collect();
    for claimed in [true, false] {
        for (c, diff) in &cells {
            apply(&diff.rows[row as usize], *c, base, blocks, &mut touched, claimed);
        }
    }
}

/// Paints the changes of cell `c` in `entries` (those a structure made if
/// `claimed`, else the others) where no earlier change landed.
fn apply(
    entries: &[(u32, Block)],
    c: IVec2,
    base: IVec3,
    blocks: &mut [Block; CHUNK_VOLUME],
    touched: &mut [u64],
    claimed: bool,
) {
    let min = region_min(c);
    let offset = IVec3::new(min.x - base.x, -base.y, min.y - base.z);
    for &(k, block) in entries {
        if (k & CLAIMED != 0) != claimed {
            continue;
        }
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

/// The loot chests of the ruined portals placed in the chunk at `cpos`
/// (generation 7: they are placed with the cell decorations).
pub fn structure_chests(nether: &NetherGen, cpos: IVec3) -> Vec<(IVec3, super::fortress::Feature)> {
    let base = cpos * CHUNK_SIZE_I;
    let mut out = Vec::new();
    if !(0..5).contains(&cpos.y) {
        return out;
    }
    for c in cells_reaching(base) {
        for &(p, f) in &cell(nether, c).chests {
            let l = p - base;
            if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                out.push((p, f));
            }
        }
    }
    out
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
    /// Which blocks a structure changed.
    claimed: Vec<u64>,
    springs: Vec<IVec3>,
    chests: Vec<(IVec3, super::fortress::Feature)>,
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

    /// A structure sets block `p` (in the region) to `b`.
    fn claim(&mut self, p: IVec3, b: Block) {
        let Some(l) = self.local(p) else { return };
        let i = Self::slot(l);
        self.claimed[i / 64] |= 1 << (i % 64);
        self.place(p, b);
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
        claimed: vec![0; volume.div_ceil(64)],
        springs: Vec::new(),
        chests: Vec::new(),
    };
    let origin = IVec3::new(c.x * CELL, 0, c.y * CELL);
    let features = order();
    let mut next = 0;
    for step in 0..=LAST_STEP {
        // `applyBiomeDecoration`: a step's structures, then its features.
        for &(_, at, structure) in &STRUCTURES {
            if at == step {
                place_structure(nether, &mut region, c, structure);
            }
        }
        while let Some(&i) = features.get(next).filter(|&&i| PLACED[i].step == step) {
            next += 1;
            let entry = &PLACED[i];
            let runs = entry.feature.runs() || matches!(entry.feature, Feature::Debris { .. });
            if !runs || !entry.biomes.iter().any(|&b| present[b as usize]) {
                continue;
            }
            let mut rng = Rng(hash3(c.x, i32::from(entry.step) << 8 | i as i32, c.y, nether.seed() ^ SALT));
            place_with_modifiers(&mut region, entry, origin, &mut rng);
        }
    }
    debug_assert_eq!(next, features.len(), "JAVA_ORDER runs by step");
    let mut rows: [Vec<(u32, Block)>; 5] = Default::default();
    let mut written = std::mem::take(&mut region.written);
    written.sort_unstable_by_key(|&(k, _)| k);
    for (k, before) in written {
        let l = unkey(k);
        let i = Region::slot(l);
        let now = region.blocks[i].get();
        if now != before {
            let claimed = region.claimed[i / 64] >> (i % 64) & 1 != 0;
            rows[(l.y / CHUNK_SIZE_I) as usize].push((if claimed { k | CLAIMED } else { k }, now));
        }
    }
    CellDiff { rows, springs: region.springs, chests: region.chests }
}

/// Places `structure`'s pieces in cell `c` (`StructureStart.placeInChunk`
/// with the cell's writable area), reading and writing the region.
fn place_structure(nether: &NetherGen, region: &mut Region, c: IVec2, structure: Structure) {
    // `getWritableArea`: the chunk's columns, from one above the bottom.
    let base = IVec3::new(c.x * CELL, 1, c.y * CELL);
    let top = IVec3::new(base.x + CELL - 1, HEIGHT - 1, base.z + CELL - 1);
    let (lo, hi) = (IVec2::new(base.x, base.z), IVec2::new(top.x, top.z));
    match structure {
        Structure::BastionRemnant => {
            let bastions = &nether.fortresses.bastions;
            if !bastions.near(lo, hi).is_empty() {
                paint_clipped(region, base, top, |blocks| bastions.paint_region(blocks, base, top));
            }
        }
        Structure::Fortress => {
            if !nether.fortresses.near(lo, hi).is_empty() {
                // No piece stands above the region, so pillars start in it.
                let open = |_, _, _, _| false;
                paint_clipped(region, base, top, |blocks| nether.fortresses.paint_box(blocks, base, top, &open));
            }
        }
        Structure::NetherFossil => {
            let fossils = nether.fossils.meeting(nether, base, top);
            if !fossils.is_empty() {
                paint_clipped(region, base, top, |blocks| {
                    for f in &fossils {
                        f.paint_box(blocks, base, top);
                    }
                });
            }
        }
        Structure::RuinedPortalNether => {
            let dim = super::terrain::Dimension::Nether;
            for layout in nether.ruined_portals.nether_layouts(nether, lo, hi) {
                if !(base.y..=top.y).contains(&layout.centre().y) {
                    continue;
                }
                let portal = (*layout).clone().process(dim.min_y(), dim.max_y(), |p| region.block(p));
                for &(p, b) in portal.cells() {
                    if !protected(region.block(p)) {
                        region.claim(p, b);
                    }
                }
                let chests: Vec<_> =
                    portal.chests().iter().filter(|(p, _)| region.local(*p).is_some()).copied().collect();
                region.chests.extend(chests);
            }
        }
    }
}

/// Runs `paint` over the box from `base` to `top` (held X fastest, then Z,
/// then Y) as the region holds it, and claims what it changed.
fn paint_clipped(region: &mut Region, base: IVec3, top: IVec3, paint: impl FnOnce(&mut [Block])) {
    let size = top - base + IVec3::ONE;
    let mut blocks = Vec::with_capacity((size.x * size.y * size.z) as usize);
    for y in base.y..=top.y {
        for z in base.z..=top.z {
            for x in base.x..=top.x {
                blocks.push(region.block(IVec3::new(x, y, z)));
            }
        }
    }
    let before = blocks.clone();
    paint(&mut blocks);
    for (i, (&was, &now)) in before.iter().zip(&blocks).enumerate() {
        if was != now {
            let i = i as i32;
            let l = IVec3::new(i % size.x, i / (size.x * size.z), i / size.x % size.z);
            region.claim(base + l, now);
        }
    }
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
        Feature::Debris { size } => {
            scattered_debris(region, rng, p, size);
        }
        Feature::Inert => {}
    }
}

/// `ScatteredOreFeature` of ancient debris (`ore_ancient_debris_large` and
/// `_small`): up to `size` tries, each `round((a - b) × min(i, 7))` from
/// `origin` on every axis for two random floats `a` and `b`, replacing
/// `#base_stone_nether` (netherrack, basalt, blackstone) with no air beside
/// it (`discard_chance_on_air_exposure` 1, so no random draw for it).
pub(super) fn scattered_debris(level: &mut impl Level, rng: &mut Rng, origin: IVec3, size: i32) -> u32 {
    let mut placed = 0;
    for i in 0..rng.below(size as u32 + 1) {
        let reach = i.min(7) as f32;
        // `Math.round(float)`: half up.
        let mut axis = || ((rng.unit() as f32 - rng.unit() as f32) * reach + 0.5).floor() as i32;
        let p = origin + IVec3::new(axis(), axis(), axis());
        let stone = matches!(level.block(p).base(), Block::NETHERRACK | Block::BASALT | Block::BLACKSTONE);
        if stone && !DIRECTIONS.iter().any(|&d| level.block(p + d) == Block::AIR) {
            level.place(p, Block::ANCIENT_DEBRIS);
            placed += 1;
        }
    }
    placed
}

const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

#[cfg(test)]
#[path = "nether_order_tests.rs"]
mod nether_order_tests;
