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
use super::structure::Rng;

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
}

impl<'a> Probe<'a> {
    pub(super) fn new(nether: &'a NetherGen, base: IVec3) -> Self {
        let lo = (IVec2::new(base.x, base.z) - WINDOW).div_euclid(IVec2::splat(STEP));
        let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I + WINDOW).div_euclid(IVec2::splat(STEP)) + 1;
        let width = hi.x - lo.x + 1;
        Self { nether, g0: lo, width, grid: (0..width * width).map(|_| OnceCell::new()).collect() }
    }

    fn column(&self, g: IVec2) -> Option<&GridColumn> {
        let l = g - self.g0;
        if l.x < 0 || l.y < 0 || l.x >= self.width || l.y >= self.width {
            return None;
        }
        Some(self.grid[(l.y * self.width + l.x) as usize].get_or_init(|| self.nether.grid_column(g)))
    }

    fn with_corners<T>(&self, x: i32, z: i32, f: impl FnOnce(&Corners) -> T) -> T {
        let g = IVec2::new(x, z).div_euclid(IVec2::splat(STEP));
        let t = [(x - g.x * STEP) as f32 / STEP as f32, (z - g.y * STEP) as f32 / STEP as f32];
        let at = [g, g + IVec2::X, g + IVec2::Y, g + IVec2::ONE];
        if let [Some(a), Some(b), Some(c), Some(d)] = at.map(|g| self.column(g)) {
            return f(&Corners { c: [a, b, c, d], t });
        }
        // Outside the window (never for in-reach features): fetch directly.
        let c = at.map(|g| self.nether.grid_column(g));
        f(&Corners { c: [&c[0], &c[1], &c[2], &c[3]], t })
    }

    fn bits(&self, x: i32, z: i32) -> u128 {
        self.with_corners(x, z, |c| c.cached_bits())
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
        self.with_corners(p.x, p.z, |c| self.nether.classify(p.x, p.y, p.z, c.cached_bits(), c))
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
}

impl Level for Canvas<'_, '_> {
    fn block(&self, p: IVec3) -> Block {
        self.own.get(&p).copied().unwrap_or_else(|| self.probe.terrain(p))
    }
    fn place(&mut self, p: IVec3, b: Block) {
        self.own.insert(p, b);
    }
}

/// Paints every biome feature that reaches the chunk at `base`.
pub(super) fn decorate(nether: &NetherGen, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    let probe = Probe::new(nether, base);
    let lo = (IVec2::new(base.x, base.z) - REACH).div_euclid(IVec2::splat(CELL));
    let hi = (IVec2::new(base.x, base.z) + CHUNK_SIZE_I - 1 + REACH).div_euclid(IVec2::splat(CELL));
    let mut painter = Painter {
        base,
        blocks,
        touched: vec![0u64; CHUNK_VOLUME / 64],
        canvas: Canvas { probe: &probe, own: FxHashMap::default() },
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
            canvas: Canvas { probe: &probe, own: FxHashMap::default() },
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
        let g = NetherGen::new(77);
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
        let b = NetherGen::new(77).generate(cpos);
        let same = |a: &ChunkData, b: &ChunkData| {
            (0..CHUNK_SIZE).all(|z| (0..CHUNK_SIZE).all(|y| (0..CHUNK_SIZE).all(|x| a.get(x, y, z) == b.get(x, y, z))))
        };
        assert!(same(&a, &b));
    }

    #[test]
    fn probe_terrain_matches_generated_chunks_where_nothing_was_painted() {
        let g = NetherGen::new(5);
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
