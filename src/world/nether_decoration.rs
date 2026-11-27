//! Nether biome features painted into generated chunks.
//!
//! Java decorates each 16×16 chunk with its biomes' placed features. Here
//! every feature start is a pure function of the seed and its Java cell, and
//! every feature reads only the undecorated terrain ([`Probe`]), so the
//! chunks a feature crosses each rebuild it identically and paint their own
//! part. Features run in one global order and the first write to a block
//! wins, which keeps overlapping features consistent across chunk borders.
//! Structures are painted afterwards and cut through features.
//!
//! Placement modifiers follow Java's `NetherPlacements` and
//! `VegetationPlacements`: `count_on_every_layer` (a new random column per
//! attempt and floor layer, until a layer finds no floor) or a count of
//! uniformly random heights. Each attempt draws its feature's randomness
//! from its own seed, so features that cannot reach a chunk are skipped
//! without disturbing the others.

use std::cell::OnceCell;
use std::sync::Arc;

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::nether::{Corners, GridColumn, NetherGen, ROOF};
use super::nether_biome::NetherBiome;
use super::nether_biome_blocks::NetherWood;
use super::nether_features::{self as features, Level, Vegetation};
use super::noise::hash3;
use super::structure::{Bounds, Rng};

const CELL: i32 = 16;
const STEP: i32 = 4;
/// The farthest any feature writes from its origin horizontally.
const REACH: i32 = 14;
/// Probe window around the chunk: feature origins up to `REACH` outside
/// it, reading up to `REACH + 1` from their origin.
const WINDOW: i32 = 2 * REACH + 1;
const SALT_PLACE: u64 = 0xDEC0_0001;
const SALT_FEATURE: u64 = 0xDEC0_0002;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Delta,
    SmallColumns,
    LargeColumns,
    Fossil,
    BasaltPillar,
    CrimsonFungi,
    CrimsonVegetation,
    WeepingVines,
    WarpedFungi,
    WarpedVegetation,
    NetherSprouts,
    TwistingVines,
    SoulFire,
}

/// How a feature's starts are chosen in each 16×16 cell.
enum Placement {
    /// `CountOnEveryLayerPlacement`: per floor layer, this many columns.
    EveryLayer(u32),
    /// `CountPlacement` + `InSquarePlacement` + uniform height in `lo..=hi`;
    /// the count itself is uniform in `count.0..=count.1`.
    Uniform { count: (u32, u32), lo: u32, hi: u32 },
    /// One start per 2×2 cells (the `nether_fossil` structure's spread).
    Fossil,
}

impl Kind {
    /// Painting order. Java's steps put surface structures (deltas,
    /// columns) and fossils before vegetation; within vegetation, the tall
    /// features go first here so a later plant never cuts a fungus or vine
    /// (Java's later features see the earlier ones; ours see terrain only).
    const ALL: [Kind; 13] = [
        Kind::Delta,
        Kind::SmallColumns,
        Kind::LargeColumns,
        Kind::Fossil,
        Kind::BasaltPillar,
        Kind::CrimsonFungi,
        Kind::WarpedFungi,
        Kind::WeepingVines,
        Kind::TwistingVines,
        Kind::CrimsonVegetation,
        Kind::WarpedVegetation,
        Kind::NetherSprouts,
        Kind::SoulFire,
    ];

    fn biome(self) -> NetherBiome {
        match self {
            Kind::Delta | Kind::SmallColumns | Kind::LargeColumns => NetherBiome::BasaltDeltas,
            Kind::Fossil | Kind::BasaltPillar | Kind::SoulFire => NetherBiome::SoulSandValley,
            Kind::CrimsonFungi | Kind::CrimsonVegetation | Kind::WeepingVines => NetherBiome::CrimsonForest,
            _ => NetherBiome::WarpedForest,
        }
    }

    fn placement(self) -> Placement {
        let full = Placement::Uniform { count: (10, 10), lo: 0, hi: 127 };
        match self {
            Kind::Delta => Placement::EveryLayer(40),
            Kind::SmallColumns => Placement::EveryLayer(4),
            Kind::LargeColumns => Placement::EveryLayer(2),
            Kind::Fossil => Placement::Fossil,
            Kind::WeepingVines | Kind::TwistingVines => full,
            // RANGE_10_10.
            Kind::BasaltPillar => Placement::Uniform { count: (10, 10), lo: 10, hi: 117 },
            Kind::CrimsonFungi | Kind::WarpedFungi => Placement::EveryLayer(8),
            Kind::CrimsonVegetation => Placement::EveryLayer(6),
            Kind::WarpedVegetation => Placement::EveryLayer(5),
            Kind::NetherSprouts => Placement::EveryLayer(4),
            Kind::SoulFire => Placement::Uniform { count: (0, 5), lo: 4, hi: 123 },
        }
    }

    /// Where the feature can write, relative to its start: horizontal
    /// radius and lowest and highest y offset.
    fn reach(self) -> (i32, i32, i32) {
        match self {
            Kind::Delta => (9, -1, -1),
            Kind::SmallColumns => (9, -6, 6),
            Kind::LargeColumns => (11, -6, 16),
            Kind::Fossil => (REACH, -3, 7),
            Kind::BasaltPillar => (4, -ROOF, 1),
            Kind::CrimsonFungi | Kind::WarpedFungi => (4, -12, 28),
            Kind::CrimsonVegetation | Kind::WarpedVegetation | Kind::NetherSprouts | Kind::SoulFire => (7, -3, 3),
            Kind::WeepingVines => (7, -23, 2),
            // Each attempt searches down from its offset to the first floor.
            Kind::TwistingVines => (8, -ROOF, 20),
        }
    }

    fn place(self, level: &mut impl Level, rng: &mut Rng, origin: IVec3) {
        match self {
            Kind::Delta => {
                features::delta(level, rng, origin - IVec3::Y);
            }
            Kind::SmallColumns | Kind::LargeColumns => {
                features::basalt_columns(level, rng, origin, self == Kind::LargeColumns);
            }
            Kind::Fossil => {
                features::fossil(level, rng, origin);
            }
            Kind::BasaltPillar => {
                features::basalt_pillar(level, rng, origin);
            }
            Kind::CrimsonFungi | Kind::WarpedFungi => {
                let wood = if self == Kind::CrimsonFungi { NetherWood::Crimson } else { NetherWood::Warped };
                features::huge_fungus(level, rng, origin, wood, false);
            }
            Kind::CrimsonVegetation => {
                features::vegetation(level, rng, origin, Vegetation::Crimson, 8, 4);
            }
            Kind::WarpedVegetation => {
                features::vegetation(level, rng, origin, Vegetation::Warped, 8, 4);
            }
            Kind::NetherSprouts => {
                features::vegetation(level, rng, origin, Vegetation::NetherSprouts, 8, 4);
            }
            Kind::WeepingVines => {
                features::weeping_vines(level, rng, origin);
            }
            Kind::TwistingVines => {
                features::twisting_vines(level, rng, origin, 8, 4, 8);
            }
            Kind::SoulFire => {
                features::soul_fire_patch(level, rng, origin);
            }
        }
    }
}

/// The undecorated terrain around one chunk: the grid columns of a window
/// around it, fetched from the shared cache on first use, and
/// [`NetherGen::classify`]. Block-column solidity is cached in the shared
/// grid columns, so the four chunks of a column scan each one once.
pub(super) struct Probe<'a> {
    nether: &'a NetherGen,
    /// Grid coordinates of the window's first grid column.
    g0: IVec2,
    width: i32,
    grid: Vec<OnceCell<Arc<GridColumn>>>,
    /// Every generation 3 fossil box whose beard reaches the window: features
    /// see the adapted terrain, as Java's run after the noise and beardifier.
    boxes: Vec<Bounds>,
    /// Generation 6: the chunk being painted, undecorated, so reads inside
    /// it skip the terrain function (it is the same terrain).
    chunk: Option<(IVec3, Box<[Block; CHUNK_VOLUME]>)>,
}

impl<'a> Probe<'a> {
    pub(super) fn new(nether: &'a NetherGen, base: IVec3) -> Self {
        let lo = (IVec2::new(base.x, base.z) - WINDOW).div_euclid(IVec2::splat(STEP));
        let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I + WINDOW).div_euclid(IVec2::splat(STEP)) + 1;
        Self::grid(nether, lo, hi)
    }

    /// The terrain of block columns `min..=max` (and a margin of two).
    pub(super) fn around(nether: &'a NetherGen, min: IVec2, max: IVec2) -> Self {
        let lo = (min - 2).div_euclid(IVec2::splat(STEP));
        let hi = (max + 2).div_euclid(IVec2::splat(STEP)) + 1;
        Self::grid(nether, lo, hi)
    }

    fn grid(nether: &'a NetherGen, lo: IVec2, hi: IVec2) -> Self {
        let width = (hi.x - lo.x).max(hi.y - lo.y) + 1;
        let boxes = nether.beard_boxes(lo * STEP, (lo + width) * STEP);
        Self { nether, g0: lo, width, grid: (0..width * width).map(|_| OnceCell::new()).collect(), boxes, chunk: None }
    }

    fn column(&self, g: IVec2) -> Option<&GridColumn> {
        let l = g - self.g0;
        if l.x < 0 || l.y < 0 || l.x >= self.width || l.y >= self.width {
            return None;
        }
        Some(self.grid[(l.y * self.width + l.x) as usize].get_or_init(|| self.nether.grid_column(g)))
    }

    /// Column `(x, z)`'s terrain solidity (fossil beards included) and corners.
    fn with_corners<T>(&self, x: i32, z: i32, f: impl FnOnce(u128, &Corners) -> T) -> T {
        let g = IVec2::new(x, z).div_euclid(IVec2::splat(STEP));
        let t = [(x - g.x * STEP) as f32 / STEP as f32, (z - g.y * STEP) as f32 / STEP as f32];
        let at = [g, g + IVec2::X, g + IVec2::Y, g + IVec2::ONE];
        if let [Some(a), Some(b), Some(c), Some(d)] = at.map(|g| self.column(g)) {
            let corners = Corners { c: [a, b, c, d], t };
            return f(self.nether.terrain_bits(x, z, &corners, &self.boxes), &corners);
        }
        // Outside the window (never for in-reach features): fetch directly.
        let c = at.map(|g| self.nether.grid_column(g));
        let corners = Corners { c: [&c[0], &c[1], &c[2], &c[3]], t };
        let boxes = self.nether.beard_boxes(IVec2::new(x, z), IVec2::new(x, z));
        f(self.nether.terrain_bits(x, z, &corners, &boxes), &corners)
    }

    fn bits(&self, x: i32, z: i32) -> u128 {
        self.with_corners(x, z, |bits, _| bits)
    }

    pub(super) fn biome(&self, x: i32, z: i32) -> NetherBiome {
        let g = IVec2::new(x, z).div_euclid(IVec2::splat(STEP));
        self.column(g).map_or_else(|| self.nether.grid_column(g).biome, |c| c.biome)
    }

    pub(super) fn terrain(&self, p: IVec3) -> Block {
        if p.y < 0 {
            return Block::BEDROCK;
        }
        if p.y > ROOF {
            return Block::AIR;
        }
        if let Some((base, blocks)) = &self.chunk {
            let l = p - *base;
            if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                return blocks[index(l.x as usize, l.y as usize, l.z as usize)];
            }
        }
        self.with_corners(p.x, p.z, |bits, c| self.nether.classify(p.x, p.y, p.z, bits, c))
    }

    /// Java's `MOTION_BLOCKING` heightmap: one above the column's top
    /// non-air block (above the bedrock roof in the Nether).
    fn motion_blocking(&self, x: i32, z: i32) -> i32 {
        (0..=ROOF).rev().find(|&y| self.terrain(IVec3::new(x, y, z)) != Block::AIR).map_or(0, |y| y + 1)
    }

    /// The `layer`-th floor from the top of column `(x, z)`: the open (air or
    /// lava) block above rock that isn't bedrock, as Java's
    /// `CountOnEveryLayerPlacement` finds it.
    fn floor(&self, x: i32, z: i32, layer: u32) -> Option<i32> {
        let bits = self.bits(x, z);
        // Open blocks with rock below, under the roof.
        let mut floors = !bits & (bits << 1) & ((1u128 << ROOF) - 1);
        let mut seen = 0;
        while floors != 0 {
            let y = 127 - floors.leading_zeros() as i32;
            floors &= !(1u128 << y);
            if self.nether.bedrock(x, y - 1, z) {
                continue;
            }
            if seen == layer {
                return Some(y);
            }
            seen += 1;
        }
        None
    }
}

/// A feature's view: the terrain plus its own writes, which it records.
struct Canvas<'p, 'a> {
    probe: &'p Probe<'a>,
    own: FxHashMap<IVec3, Block>,
    /// The chunk being painted: the only blocks whose writes are kept.
    chunk: Option<(IVec3, IVec3)>,
}

impl Level for Canvas<'_, '_> {
    fn block(&self, p: IVec3) -> Block {
        self.own.get(&p).copied().unwrap_or_else(|| self.probe.terrain(p))
    }
    fn place(&mut self, p: IVec3, b: Block) {
        self.own.insert(p, b);
    }
    fn write_bounds(&self) -> Option<(IVec3, IVec3)> {
        self.chunk
    }
}

/// Paints every biome feature that reaches the chunk at `base`.
pub(super) fn decorate(nether: &NetherGen, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    decorate_within(nether, blocks, base, true);
}

/// [`decorate`], where `cull` lets features skip work that only decides
/// blocks outside the chunk (off, every feature runs in full: the same chunk).
fn decorate_within(nether: &NetherGen, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, cull: bool) {
    if nether.generation >= super::terrain::Generation::V7 {
        super::nether_order::decorate(nether, blocks, base);
        return;
    }
    if nether.generation >= super::terrain::Generation::V6 {
        v6::decorate(nether, blocks, base, cull, None);
        return;
    }
    let probe = Probe::new(nether, base);
    let lo = (IVec2::new(base.x, base.z) - REACH).div_euclid(IVec2::splat(CELL));
    let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I - 1 + REACH).div_euclid(IVec2::splat(CELL));
    let mut painter = Painter {
        base,
        blocks,
        touched: vec![0u64; CHUNK_VOLUME / 64],
        canvas: Canvas {
            probe: &probe,
            own: FxHashMap::default(),
            chunk: cull.then_some((base, base + IVec3::splat(CHUNK_SIZE_I - 1))),
        },
    };
    for cz in lo.y..=hi.y {
        for cx in lo.x..=hi.x {
            let cell = IVec2::new(cx, cz);
            // Skip features whose biome is nowhere in the cell.
            let mut present = [false; 5];
            for qz in 0..CELL / STEP {
                for qx in 0..CELL / STEP {
                    present[probe.biome(cx * CELL + qx * STEP, cz * CELL + qz * STEP) as usize] = true;
                }
            }
            for kind in Kind::ALL {
                if kind == Kind::Fossil && nether.generation >= super::terrain::Generation::V3 {
                    continue;
                }
                if present[kind.biome() as usize] {
                    painter.cell(nether.seed(), cell, kind);
                }
            }
        }
    }
}

struct Painter<'b, 'p, 'a> {
    base: IVec3,
    blocks: &'b mut [Block; CHUNK_VOLUME],
    /// Blocks a feature already wrote (first write wins).
    touched: Vec<u64>,
    canvas: Canvas<'p, 'a>,
}

impl Painter<'_, '_, '_> {
    fn cell(&mut self, seed: u64, cell: IVec2, kind: Kind) {
        let probe = self.canvas.probe;
        let mut rng = Rng(hash3(cell.x, kind as i32, cell.y, seed ^ SALT_PLACE));
        let column =
            |rng: &mut Rng| IVec2::new(cell.x * CELL + rng.below(16) as i32, cell.y * CELL + rng.below(16) as i32);
        match kind.placement() {
            Placement::EveryLayer(count) => {
                for layer in 0.. {
                    let mut found = false;
                    for _ in 0..count {
                        let c = column(&mut rng);
                        if let Some(y) = probe.floor(c.x, c.y, layer) {
                            found = true;
                            self.start(seed, kind, IVec3::new(c.x, y, c.y));
                        }
                    }
                    if !found {
                        break;
                    }
                }
            }
            Placement::Uniform { count, lo, hi } => {
                for _ in 0..rng.range(count.0, count.1) {
                    let c = column(&mut rng);
                    let y = rng.range(lo, hi) as i32;
                    self.start(seed, kind, IVec3::new(c.x, y, c.y));
                }
            }
            Placement::Fossil => {
                if cell.x.rem_euclid(2) != 0 || cell.y.rem_euclid(2) != 0 {
                    return;
                }
                let c = column(&mut rng);
                // Down from a random height to a floor above the lava sea.
                let top = rng.range(33, 125) as i32;
                let floor = (34..=top).rev().find(|&y| {
                    let below = probe.terrain(IVec3::new(c.x, y - 1, c.y));
                    probe.terrain(IVec3::new(c.x, y, c.y)) == Block::AIR
                        && (below == Block::SOUL_SAND || below.is_opaque())
                });
                if let Some(y) = floor {
                    self.start(seed, kind, IVec3::new(c.x, y, c.y));
                }
            }
        }
    }

    /// Runs one feature start if its biome matches and it can reach the chunk.
    fn start(&mut self, seed: u64, kind: Kind, origin: IVec3) {
        let (r, y_lo, y_hi) = kind.reach();
        let b = self.base;
        let reaches = origin.x + r >= b.x
            && origin.x - r < b.x + CHUNK_SIZE_I
            && origin.z + r >= b.z
            && origin.z - r < b.z + CHUNK_SIZE_I
            && origin.y + y_hi >= b.y
            && origin.y + y_lo < b.y + CHUNK_SIZE_I;
        if !reaches || self.canvas.probe.biome(origin.x, origin.z) != kind.biome() {
            return;
        }
        let mut rng = Rng(hash3(origin.x, origin.y, origin.z, seed ^ SALT_FEATURE ^ (kind as u64) << 40));
        self.canvas.own.clear();
        kind.place(&mut self.canvas, &mut rng, origin);
        self.paint_feature();
    }

    fn paint_feature(&mut self) {
        let b = self.base;
        // A feature may overwrite its own blocks (deltas replace magma with
        // lava). First-write precedence applies between features; within one
        // feature, paint the final state from its canvas.
        for (&p, &block) in &self.canvas.own {
            let l = p - b;
            if block == Block::AIR || l.cmplt(IVec3::ZERO).any() || l.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
                continue;
            }
            let i = index(l.x as usize, l.y as usize, l.z as usize);
            if self.touched[i / 64] >> (i % 64) & 1 == 0 {
                self.touched[i / 64] |= 1 << (i % 64);
                self.blocks[i] = block;
            }
        }
    }
}

/// Generation 6: Java 26.3's placed features ([`super::nether_placed`]).
pub(super) mod v6 {
    use super::*;
    use crate::world::nether_placed::{self as placed, Feature, Mod, PLACED, Placed};

    /// The springs a generated chunk holds, to schedule their first fluid
    /// tick (Java's `SpringFeature` schedules one as it places the lava):
    /// the springs replayed over the undecorated chunk, where the generated
    /// chunk still holds their lava.
    pub fn spring_ticks(nether: &NetherGen, cpos: IVec3, data: &crate::world::chunk::ChunkData) -> Vec<IVec3> {
        let base = cpos * CHUNK_SIZE_I;
        if base.y > ROOF || base.y + CHUNK_SIZE_I <= 0 {
            return Vec::new();
        }
        let mut terrain = nether.undecorated(cpos);
        let mut springs = Vec::new();
        decorate(nether, &mut terrain, base, true, Some(&mut springs));
        springs
            .into_iter()
            .filter(|p| {
                let l = *p - base;
                data.get(l.x as usize, l.y as usize, l.z as usize) == Block::LAVA
            })
            .collect()
    }

    /// Paints every generation 6 feature that reaches the chunk. With
    /// `springs`, places only the springs and records where they went.
    pub fn decorate(
        nether: &NetherGen,
        blocks: &mut [Block; CHUNK_VOLUME],
        base: IVec3,
        cull: bool,
        mut springs: Option<&mut Vec<IVec3>>,
    ) {
        let mut probe = Probe::new(nether, base);
        if base.y <= ROOF {
            probe.chunk = Some((base, Box::new(*blocks)));
        }
        let lo = (IVec2::new(base.x, base.z) - REACH).div_euclid(IVec2::splat(CELL));
        let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I - 1 + REACH).div_euclid(IVec2::splat(CELL));
        let mut painter = Painter {
            base,
            blocks,
            touched: vec![0u64; CHUNK_VOLUME / 64],
            canvas: Canvas {
                probe: &probe,
                own: FxHashMap::default(),
                chunk: cull.then_some((base, base + IVec3::splat(CHUNK_SIZE_I - 1))),
            },
        };
        let order = placed::paint_order();
        let slack: Vec<Vec<Slack>> = PLACED.iter().map(slack_of).collect();
        for cz in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                let cell = IVec2::new(cx, cz);
                let mut present = [false; 5];
                for qz in 0..CELL / STEP {
                    for qx in 0..CELL / STEP {
                        present[probe.biome(cx * CELL + qx * STEP, cz * CELL + qz * STEP) as usize] = true;
                    }
                }
                for &i in &order {
                    let entry = &PLACED[i];
                    let is_spring = matches!(entry.feature, Feature::Spring { .. });
                    if !entry.feature.runs() || (springs.is_some() && !is_spring) {
                        continue;
                    }
                    if !entry.biomes.iter().any(|&b| present[b as usize]) {
                        continue;
                    }
                    let seed = hash3(cell.x, i as i32, cell.y, nether.seed() ^ SALT_PLACE_V6);
                    let origin = IVec3::new(cell.x * CELL, 0, cell.y * CELL);
                    let mut run =
                        Run { painter: &mut painter, entry, slack: &slack[i], springs: springs.as_deref_mut() };
                    run.walk(0, origin, seed);
                }
            }
        }
    }

    const SALT_PLACE_V6: u64 = 0xDEC0_0006;
    const SALT_FEATURE_V6: u64 = 0xDEC0_0007;

    /// How far the rest of a modifier list and the feature can move and
    /// write from the current position: x/z and y ranges (`None`: a height
    /// modifier still decides y).
    #[derive(Clone, Copy, Debug)]
    struct Slack {
        xz: (i32, i32),
        y: Option<(i32, i32)>,
    }

    fn slack_of(p: &Placed) -> Vec<Slack> {
        let (r, ylo, yhi) = placed::reach(p.feature);
        let mut out = vec![Slack { xz: (-r, r), y: Some((ylo, yhi)) }];
        for m in p.mods.iter().rev() {
            let next = *out.last().unwrap();
            let s = match *m {
                Mod::InSquare | Mod::CountOnEveryLayer(_) => Slack {
                    xz: (next.xz.0, next.xz.1 + CELL - 1),
                    y: if matches!(m, Mod::CountOnEveryLayer(_)) { None } else { next.y },
                },
                Mod::Height(_) | Mod::MotionBlocking => Slack { xz: next.xz, y: None },
                Mod::Offset(x, y, z) => {
                    let (xl, xh) = x.bounds();
                    let (zl, zh) = z.bounds();
                    let (yl, yh) = y.bounds();
                    Slack {
                        xz: (next.xz.0 + xl.min(zl), next.xz.1 + xh.max(zh)),
                        y: next.y.map(|(a, b)| (a + yl, b + yh)),
                    }
                }
                Mod::ScanDown(steps, _) => Slack { xz: next.xz, y: next.y.map(|(a, b)| (a - steps, b)) },
                _ => next,
            };
            out.push(s);
        }
        out.reverse();
        out
    }

    fn child(seed: u64, i: u64) -> u64 {
        let mut s = seed ^ (i + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        crate::world::noise::splitmix64(&mut s)
    }

    struct Run<'r, 'b, 'p, 'a> {
        painter: &'r mut Painter<'b, 'p, 'a>,
        entry: &'r Placed,
        slack: &'r [Slack],
        springs: Option<&'r mut Vec<IVec3>>,
    }

    impl Run<'_, '_, '_, '_> {
        fn reaches(&self, k: usize, p: IVec3) -> bool {
            let s = self.slack[k];
            let b = self.painter.base;
            let top = b + IVec3::splat(CHUNK_SIZE_I - 1);
            let xz = p.x + s.xz.1 >= b.x && p.x + s.xz.0 <= top.x && p.z + s.xz.1 >= b.z && p.z + s.xz.0 <= top.z;
            xz && s.y.is_none_or(|(lo, hi)| p.y + hi >= b.y && p.y + lo <= top.y)
        }

        fn walk(&mut self, k: usize, p: IVec3, seed: u64) {
            if !self.reaches(k, p) {
                return;
            }
            let Some(&m) = self.entry.mods.get(k) else {
                self.place(p, seed);
                return;
            };
            let probe = self.painter.canvas.probe;
            let mut rng = Rng(seed);
            match m {
                Mod::Count(n) => {
                    for i in 0..n.sample(&mut rng).max(0) {
                        self.walk(k + 1, p, child(seed, i as u64));
                    }
                }
                Mod::CountOnEveryLayer(count) => {
                    for layer in 0.. {
                        let mut found = false;
                        for j in 0..count {
                            let (x, z) = (p.x + rng.below(16) as i32, p.z + rng.below(16) as i32);
                            if let Some(y) = probe.floor(x, z, layer) {
                                found = true;
                                self.walk(k + 1, IVec3::new(x, y, z), child(seed, u64::from(layer * count + j)));
                            }
                        }
                        if !found {
                            break;
                        }
                    }
                }
                Mod::InSquare => {
                    let q = p + IVec3::new(rng.below(16) as i32, 0, rng.below(16) as i32);
                    self.walk(k + 1, q, child(seed, 0));
                }
                Mod::Height(h) => self.walk(k + 1, IVec3::new(p.x, h.sample(&mut rng), p.z), child(seed, 0)),
                Mod::MotionBlocking => self.walk(k + 1, IVec3::new(p.x, probe.motion_blocking(p.x, p.z), p.z), seed),
                Mod::Biome => {
                    if placed::in_biome(self.entry, probe.biome(p.x, p.z)) {
                        self.walk(k + 1, p, seed);
                    }
                }
                Mod::Rarity(n) => {
                    if (rng.unit() as f32) < 1.0 / n as f32 {
                        self.walk(k + 1, p, child(seed, 0));
                    }
                }
                Mod::Offset(x, y, z) => {
                    let q = p + IVec3::new(x.sample(&mut rng), y.sample(&mut rng), z.sample(&mut rng));
                    self.walk(k + 1, q, child(seed, 0));
                }
                Mod::Filter(pred) => {
                    if pred.test(&|q| probe.terrain(q), p) {
                        self.walk(k + 1, p, seed);
                    }
                }
                Mod::ScanDown(steps, target) => {
                    let block = |q: IVec3| probe.terrain(q);
                    let mut q = p;
                    for _ in 0..steps {
                        if target.test(&block, q) {
                            self.walk(k + 1, q, seed);
                            return;
                        }
                        q.y -= 1;
                        if q.y < 0 {
                            return;
                        }
                    }
                    if target.test(&block, q) {
                        self.walk(k + 1, q, seed);
                    }
                }
            }
        }

        fn place(&mut self, p: IVec3, seed: u64) {
            let mut rng = Rng(seed ^ SALT_FEATURE_V6);
            let level = &mut self.painter.canvas;
            level.own.clear();
            let placed = match self.entry.feature {
                Feature::Spring { valid, requires_below, rock, hole } => {
                    let valid: Vec<Block> = valid.iter().map(|id| placed::java_block(id)).collect();
                    features::spring(level, p, &valid, requires_below, rock, hole)
                }
                Feature::SimpleBlock(what) => features::simple(level, &mut rng, p, what),
                Feature::Glowstone => features::glowstone(level, &mut rng, p),
                Feature::Ore { size, state } => features::ore(level, &mut rng, p, size, placed::java_block(state)),
                Feature::Blobs { state, radius } => {
                    features::blobs(level, &mut rng, p, placed::java_block(state), radius)
                }
                Feature::HugeFungus(wood) => features::huge_fungus(level, &mut rng, p, wood, false),
                Feature::WeepingVines => features::weeping_vines_26(level, &mut rng, p),
                Feature::TwistingVines => features::twisting_vines_26(level, &mut rng, p),
                Feature::Delta => features::delta(level, &mut rng, p - IVec3::Y),
                Feature::Columns { large } => features::basalt_columns(level, &mut rng, p, large),
                Feature::BasaltPillar => features::basalt_pillar(level, &mut rng, p),
                Feature::Debris { .. } | Feature::Inert => false,
            };
            if let Some(springs) = self.springs.as_deref_mut() {
                let local = p - self.painter.base;
                if placed && local.cmpge(IVec3::ZERO).all() && local.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                    springs.push(p);
                }
                return;
            }
            self.painter.paint_feature();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::{CHUNK_SIZE, ChunkData};
    use crate::world::nether_biome_blocks::{self as nb, Vine};
    use std::collections::HashMap;

    /// Every block of the 2×2×4 chunks at a spot well inside `biome`.
    fn area(g: &NetherGen, biome: NetherBiome) -> (IVec3, HashMap<IVec3, Block>) {
        let inside = |p: IVec3| {
            [IVec3::ZERO, IVec3::X * 48, IVec3::Z * 48, IVec3::new(48, 0, 48)]
                .iter()
                .all(|&d| g.biomes.biome(p.x + d.x, p.z + d.z) == biome)
        };
        let at = (0..4000)
            .map(|i| IVec3::new((i % 63 - 31) * 64, 64, (i / 63 - 31) * 64))
            .find(|&p| inside(p))
            .expect("a large patch of the biome");
        let c = IVec3::new(at.x.div_euclid(CHUNK_SIZE_I), 0, at.z.div_euclid(CHUNK_SIZE_I));
        let mut blocks = HashMap::new();
        for dx in 0..2 {
            for dz in 0..2 {
                for cy in 0..4 {
                    let cpos = c + IVec3::new(dx, cy, dz);
                    let chunk = g.generate(cpos);
                    for z in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_SIZE {
                            for x in 0..CHUNK_SIZE {
                                let p = cpos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32);
                                blocks.insert(p, chunk.get(x, y, z));
                            }
                        }
                    }
                }
            }
        }
        (c * CHUNK_SIZE_I, blocks)
    }

    fn count(blocks: &HashMap<IVec3, Block>, f: impl Fn(Block) -> bool) -> usize {
        blocks.values().filter(|&&b| f(b)).count()
    }

    #[test]
    fn delta_painting_keeps_lava_that_replaced_its_own_magma_rim() {
        use super::super::nether_features::tests::Flat;

        struct Recorded<'c, 'p, 'a> {
            flat: Flat,
            canvas: &'c mut Canvas<'p, 'a>,
        }
        impl Level for Recorded<'_, '_, '_> {
            fn block(&self, p: IVec3) -> Block {
                self.flat.block(p)
            }
            fn place(&mut self, p: IVec3, b: Block) {
                self.flat.place(p, b);
                self.canvas.place(p, b);
            }
        }

        let g = NetherGen::new(1);
        let base = IVec3::new(-16, 32, -16);
        let probe = Probe::new(&g, base);
        let mut blocks = ChunkData::new_dense(Block::BASALT);
        let mut painter = Painter {
            base,
            blocks: &mut blocks,
            touched: vec![0u64; CHUNK_VOLUME / 64],
            canvas: Canvas {
                probe: &probe,
                own: FxHashMap::default(),
                chunk: Some((base, base + IVec3::splat(CHUNK_SIZE_I - 1))),
            },
        };
        let mut recorded = Recorded { flat: Flat::new(40, 100, Block::BASALT), canvas: &mut painter.canvas };
        // Seed 2 replaces 28 earlier magma writes with lava inside one delta.
        assert!(features::delta(&mut recorded, &mut Rng(2), IVec3::new(0, 39, 0)));
        let expected = recorded.flat.blocks;
        painter.paint_feature();
        for (&p, &block) in &expected {
            let l = p - base;
            assert_eq!(painter.blocks[index(l.x as usize, l.y as usize, l.z as usize)], block, "delta at {p}");
        }
        // A later feature still cannot override this delta.
        painter.canvas.own.clear();
        for (&p, &block) in &expected {
            if block == Block::LAVA {
                painter.canvas.place(p, Block::MAGMA);
            }
        }
        painter.paint_feature();
        for (&p, &block) in &expected {
            let l = p - base;
            assert_eq!(painter.blocks[index(l.x as usize, l.y as usize, l.z as usize)], block, "precedence at {p}");
        }
    }

    #[test]
    fn each_biome_dresses_its_caverns_like_java() {
        let g = NetherGen::new(2024);
        let (_, crimson) = area(&g, NetherBiome::CrimsonForest);
        for b in [nb::CRIMSON_NYLIUM, nb::CRIMSON_STEM, nb::NETHER_WART_BLOCK, nb::CRIMSON_ROOTS] {
            assert!(count(&crimson, |c| c == b) > 0, "crimson forest has {}", b.name());
        }
        assert!(count(&crimson, |b| matches!(Vine::of(b), Some((Vine::Weeping, _)))) > 0, "weeping vines");
        let (_, warped) = area(&g, NetherBiome::WarpedForest);
        for b in [nb::WARPED_NYLIUM, nb::WARPED_STEM, nb::WARPED_WART_BLOCK, nb::WARPED_ROOTS, nb::NETHER_SPROUTS] {
            assert!(count(&warped, |c| c == b) > 0, "warped forest has {}", b.name());
        }
        let (_, soul) = area(&g, NetherBiome::SoulSandValley);
        for b in [nb::SOUL_SOIL, Block::SOUL_SAND, nb::BONE_BLOCK] {
            assert!(count(&soul, |c| c == b) > 0, "soul sand valley has {}", b.name());
        }
        assert_eq!(count(&soul, nb::is_nylium), 0, "no nylium in soul sand valleys");
        let (_, deltas) = area(&g, NetherBiome::BasaltDeltas);
        for b in [Block::BASALT, Block::BLACKSTONE, Block::MAGMA] {
            assert!(count(&deltas, |c| c == b) > 100, "basalt deltas have {}", b.name());
        }
        let pools = deltas.iter().filter(|(p, b)| **b == Block::LAVA && p.y > super::super::nether::LAVA_SEA).count();
        assert!(pools > 10, "lava delta pools above the sea: {pools}");
    }

    #[test]
    fn features_cross_chunk_borders_without_seams_and_regardless_of_order() {
        // Generation 6's first-write order; generation 7's chunks are
        // checked by `nether_order_tests` and `order_tests`.
        let v6 = || NetherGen::with_generation(77, crate::world::terrain::Generation::V6);
        let g = v6();
        for biome in [NetherBiome::CrimsonForest, NetherBiome::WarpedForest] {
            let (_, blocks) = area(&g, biome);
            let mut vines = 0;
            for (&p, &b) in &blocks {
                let Some((vine, _)) = Vine::of(b) else { continue };
                vines += 1;
                // Every vine hangs from (or stands on) more vine or something solid,
                // including across the chunk borders inside the area.
                let support = p - vine.grows();
                if let Some(&s) = blocks.get(&support) {
                    assert!(
                        s.is_opaque() || Vine::of(s).is_some_and(|(v, _)| v == vine),
                        "{} at {p} held by {}",
                        b.name(),
                        s.name()
                    );
                }
            }
            assert!(vines > 0, "{biome:?} has vines");
        }
        // A fresh generator (cold caches) paints the same chunk.
        let cpos = IVec3::new(3, 1, -2);
        let a = g.generate(cpos);
        let b = v6().generate(cpos);
        let same = |a: &ChunkData, b: &ChunkData| {
            (0..CHUNK_SIZE).all(|z| (0..CHUNK_SIZE).all(|y| (0..CHUNK_SIZE).all(|x| a.get(x, y, z) == b.get(x, y, z))))
        };
        assert!(same(&a, &b));
    }

    /// Skipping work that only decides blocks outside the chunk must leave every
    /// painted block as it was, in the biomes with the heaviest culling (deltas,
    /// whose pools and columns straddle chunk borders) and in the others.
    #[test]
    fn culling_features_to_the_chunk_changes_nothing() {
        use crate::world::terrain::Generation;
        for generation in [Generation::V5, Generation::V6] {
            let g = NetherGen::with_generation(12345, generation);
            let mut compared = 0;
            for (cx, cz) in [(-32, -140), (-33, -141), (-128, -140), (-140, -140), (0, 0), (-80, -140), (36, -111)] {
                for dx in 0..2 {
                    for cy in 0..4 {
                        let cpos = IVec3::new(cx + dx, cy, cz);
                        let base = cpos * CHUNK_SIZE_I;
                        // Decoration paints over the chunk's own terrain.
                        let terrain = g.undecorated(cpos);
                        let paint = |cull| {
                            let mut blocks = terrain.clone();
                            decorate_within(&g, &mut blocks, base, cull);
                            blocks
                        };
                        let (culled, full) = (paint(true), paint(false));
                        for i in 0..CHUNK_VOLUME {
                            assert_eq!(
                                culled[i],
                                full[i],
                                "{generation:?} {base} cell {i}: {} vs {}",
                                culled[i].name(),
                                full[i].name()
                            );
                        }
                        compared += (0..CHUNK_VOLUME).filter(|&i| full[i] != terrain[i]).count();
                    }
                }
            }
            assert!(compared > 2000, "{generation:?}: only {compared} feature blocks compared");
        }
    }

    /// Generation 6 places Java 26.3's Nether features: ores, glowstone,
    /// fire, springs, mushrooms, blobs, and the 26.3 vegetation and vines.
    #[test]
    fn generation_6_places_javas_nether_features_in_their_biomes() {
        use crate::world::gen5_blocks::NETHER_GOLD_ORE;
        let g = NetherGen::with_generation(2024, crate::world::terrain::Generation::V6);
        let sea = super::super::nether::LAVA_SEA;
        let springs =
            |blocks: &HashMap<IVec3, Block>| blocks.iter().filter(|(p, b)| **b == Block::LAVA && p.y > sea + 1).count();
        let (_, wastes) = area(&g, NetherBiome::NetherWastes);
        for b in [Block::GLOWSTONE, Block::QUARTZ_ORE, NETHER_GOLD_ORE, Block::MAGMA, Block::GRAVEL, Block::BLACKSTONE]
        {
            assert!(count(&wastes, |c| c == b) > 0, "nether wastes have {}", b.name());
        }
        assert!(count(&wastes, |b| b.fire_age().is_some()) > 0, "fire on netherrack");
        assert!(springs(&wastes) > 0, "lava springs above the sea");
        let (_, deltas) = area(&g, NetherBiome::BasaltDeltas);
        // Blobs replace netherrack deep inside the rock as well as at floors.
        let deep_basalt = deltas
            .iter()
            .filter(|(p, b)| **b == Block::BASALT && deltas.get(&(**p + IVec3::Y)) == Some(&Block::NETHERRACK))
            .count();
        assert!(deep_basalt > 50, "basalt blobs under netherrack: {deep_basalt}");
        for b in [Block::QUARTZ_ORE, NETHER_GOLD_ORE, Block::BLACKSTONE] {
            assert!(count(&deltas, |c| c == b) > 0, "basalt deltas have {}", b.name());
        }
        let (_, soul) = area(&g, NetherBiome::SoulSandValley);
        assert!(count(&soul, |c| c == nb::SOUL_FIRE) > 0, "soul fire on soul soil");
        assert!(count(&soul, |c| c == Block::SOUL_SAND) > 0);
        let (_, crimson) = area(&g, NetherBiome::CrimsonForest);
        for b in [nb::CRIMSON_STEM, nb::CRIMSON_ROOTS, nb::NETHER_WART_BLOCK, Block::GLOWSTONE] {
            assert!(count(&crimson, |c| c == b) > 0, "crimson forest has {}", b.name());
        }
        // 26.3's vines: every head is aged 17..=25 and hangs below plants or the roof.
        let heads: Vec<_> = crimson.values().filter_map(|&b| Vine::of(b).and_then(|(_, age)| age)).collect();
        assert!(!heads.is_empty() && heads.iter().all(|a| (17..=25).contains(a)), "{heads:?}");
        let (_, warped) = area(&g, NetherBiome::WarpedForest);
        for b in [nb::WARPED_STEM, nb::WARPED_ROOTS, nb::NETHER_SPROUTS] {
            assert!(count(&warped, |c| c == b) > 0, "warped forest has {}", b.name());
        }
        assert!(count(&warped, |b| matches!(Vine::of(b), Some((Vine::Twisting, _)))) > 0, "twisting vines");
        // No noise glowstone remains: glowstone grows from ceilings, each
        // block touching another or the rock (an earlier feature may take
        // a block of the chain, rarely).
        let glow: Vec<_> = wastes.iter().filter(|(_, b)| **b == Block::GLOWSTONE).map(|(p, _)| *p).collect();
        let held = glow
            .iter()
            .filter(|&&p| {
                [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z]
                    .iter()
                    .any(|&d| wastes.get(&(p + d)).is_some_and(|b| *b == Block::GLOWSTONE || b.is_opaque()))
            })
            .count();
        assert!(held * 100 >= glow.len() * 99, "{held} of {} glowstone held", glow.len());
    }

    /// Java's overworld mushrooms (heightmap placement) land on the bedrock
    /// roof, in the chunk above it.
    #[test]
    fn generation_6_grows_mushrooms_on_the_nether_roof() {
        use crate::world::terrain::Generation;
        for generation in [Generation::V6, Generation::V7] {
            roof_mushrooms(NetherGen::with_generation(31, generation));
        }
        // Generation 5 keeps the roof bare.
        let old = NetherGen::with_generation(31, Generation::V5);
        assert!(matches!(old.generate(IVec3::new(0, 4, 0)), ChunkData::Uniform(Block::AIR)));
    }

    fn roof_mushrooms(g: NetherGen) {
        let mut found = 0;
        for cz in -8..8 {
            for cx in -8..8 {
                let cpos = IVec3::new(cx, 4, cz);
                let chunk = g.generate(cpos);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let b = chunk.get(x, 0, z);
                        if b == Block::BROWN_MUSHROOM || b == Block::RED_MUSHROOM {
                            found += 1;
                            let p = cpos * CHUNK_SIZE_I + IVec3::new(x as i32, -1, z as i32);
                            assert!(g.bedrock(p.x, p.y, p.z), "a roof mushroom stands on bedrock at {p}");
                        }
                        // Nothing else lands up here.
                        assert!(matches!(b, Block::AIR | Block::BROWN_MUSHROOM | Block::RED_MUSHROOM), "{}", b.name());
                    }
                }
            }
        }
        assert!(found > 0, "{:?}: no roof mushrooms in 1024 cells", g.generation);
    }

    #[test]
    fn generation_6_springs_are_scheduled_to_flow() {
        let g = NetherGen::with_generation(8, crate::world::terrain::Generation::V6);
        let mut open = 0;
        let mut total = 0;
        for cz in -2..2 {
            for cx in -2..2 {
                for cy in 0..4 {
                    let cpos = IVec3::new(cx, cy, cz);
                    let data = g.generate(cpos);
                    for p in v6::spring_ticks(&g, cpos, &data) {
                        total += 1;
                        let l = p - cpos * CHUNK_SIZE_I;
                        assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize), Block::LAVA);
                        let air = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::NEG_Y].iter().any(|&d| {
                            let q = l + d;
                            q.cmpge(IVec3::ZERO).all()
                                && q.cmplt(IVec3::splat(CHUNK_SIZE_I)).all()
                                && data.get(q.x as usize, q.y as usize, q.z as usize) == Block::AIR
                        });
                        open += usize::from(air);
                    }
                }
            }
        }
        assert!(total > 20 && open > 0, "{total} springs, {open} open");
    }

    #[test]
    fn probe_terrain_matches_generated_chunks_where_nothing_was_painted() {
        let g = NetherGen::with_generation(5, crate::world::terrain::Generation::V5);
        let cpos = IVec3::new(-1, 1, 2);
        let base = cpos * CHUNK_SIZE_I;
        let chunk = g.generate(cpos);
        let probe = Probe::new(&g, base);
        let mut same = 0;
        for z in (0..CHUNK_SIZE).step_by(3) {
            for y in (0..CHUNK_SIZE).step_by(2) {
                for x in (0..CHUNK_SIZE).step_by(3) {
                    let p = base + IVec3::new(x as i32, y as i32, z as i32);
                    let (generated, terrain) = (chunk.get(x, y, z), probe.terrain(p));
                    // Features only replace air, floor tops (deltas) or soul blocks (fossils).
                    if generated == terrain {
                        same += 1;
                    } else {
                        assert!(
                            terrain == Block::AIR || terrain.is_opaque(),
                            "{} became {} at {p}",
                            terrain.name(),
                            generated.name()
                        );
                    }
                }
            }
        }
        assert!(same > 1500, "{same}");
    }
}
