//! Generation 7 (v1.0 gap Track 5, GEN-04a): the Nether's surfaces from
//! Java 26.3's surface rules, `worldgen/material_rule/nether.json` (26.3
//! calls them material rules), as `MaterialSystem.buildSurface` applies
//! them to every stone block of a column.
//!
//! The rules read a block's depth below the open space above it
//! (`stoneDepthAbove`: lava doesn't end the run, so the floor under the lava
//! sea is a floor) and above the open space below it (`stoneDepthBelow`:
//! lava or air ends it), the column's surface depth
//! (`MaterialSystem.getSurfaceDepth`: the `surface` noise × 2.75 + 3 plus a
//! random quarter, truncated; a hole where it is 0 or less), its biome and
//! five 2D noises. That gives the edges by the lava sea: gravel on floors
//! whose surface reaches y 30–34 (in the wastes on the top block from y 31,
//! and under the floor in soul sand valleys and basalt deltas), soul sand
//! under the wastes' floors there, and lava instead of the floor's top
//! block below y 32 where the surface depth makes a hole.
//!
//! Under FE-01 the noises are VoxelCraft's (permutations from the seed),
//! with Java's octaves and amplitudes: 26.3 stores each as the 1.21
//! layout's parity parameters (`NormalNoise.createParity`), checked in
//! `nether_surface_tests`.

use super::block::Block;
use super::nether::{LAVA_SEA, ROOF};
use super::nether_biome::{NetherBiome, NormalNoise};
use super::nether_biome_blocks as nb;
use super::noise::hash3;

/// Java's `Noises` the Nether's rules read, as the legacy layout 26.3's
/// parity parameters encode: first octave and amplitudes.
pub(super) const SURFACE: (i32, &[f64]) = (-6, &[1.0, 1.0, 1.0]);
pub(super) const SOUL_SAND_LAYER: (i32, &[f64]) =
    (-8, &[1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.013_333_333_333_333_334]);
pub(super) const GRAVEL_LAYER: (i32, &[f64]) = SOUL_SAND_LAYER;
pub(super) const PATCH: (i32, &[f64]) = (-5, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.013_333_333_333_333_334]);
pub(super) const NETHERRACK: (i32, &[f64]) = (-3, &[1.0, 0.0, 0.0, 0.35]);
pub(super) const NETHER_WART: (i32, &[f64]) = (-3, &[1.0, 0.0, 0.0, 0.9]);
pub(super) const NETHER_STATE_SELECTOR: (i32, &[f64]) = (-4, &[1.0]);

const SALT: u64 = 0x5EA7_0007;

/// The surface rules' noises for one seed.
pub(super) struct SurfaceNoises {
    seed: u64,
    surface: NormalNoise,
    soul_sand_layer: NormalNoise,
    gravel_layer: NormalNoise,
    patch: NormalNoise,
    netherrack: NormalNoise,
    nether_wart: NormalNoise,
    state_selector: NormalNoise,
}

/// What a block column's rules need: its surface depth and the two noises
/// its biome's rules read (see [`SurfaceNoises::column`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SurfaceColumn {
    pub depth: i32,
    pub noise: [f64; 2],
}

impl SurfaceNoises {
    pub(super) fn new(seed: u64) -> Self {
        let n = |i: u64, (first, amplitudes): (i32, &[f64])| {
            NormalNoise::new(seed ^ SALT ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15), first, amplitudes)
        };
        Self {
            seed,
            surface: n(1, SURFACE),
            soul_sand_layer: n(2, SOUL_SAND_LAYER),
            gravel_layer: n(3, GRAVEL_LAYER),
            patch: n(4, PATCH),
            netherrack: n(5, NETHERRACK),
            nether_wart: n(6, NETHER_WART),
            state_selector: n(7, NETHER_STATE_SELECTOR),
        }
    }

    /// `MaterialSystem.getSurfaceDepth`: `(int)` truncates toward zero.
    pub(super) fn depth(&self, x: i32, z: i32) -> i32 {
        let random = (hash3(x, 0, z, self.seed ^ SALT) >> 11) as f64 / (1u64 << 53) as f64;
        (self.surface.sample(f64::from(x), 0.0, f64::from(z)) * 2.75 + 3.0 + random * 0.25) as i32
    }

    /// A named noise at `(x, 0, z)`, as `NoiseThresholdCondition` samples it.
    pub(super) fn noise(&self, name: &str, x: i32, z: i32) -> f64 {
        let n = match name {
            "surface" => &self.surface,
            "soul_sand_layer" => &self.soul_sand_layer,
            "gravel_layer" => &self.gravel_layer,
            "patch" => &self.patch,
            "netherrack" => &self.netherrack,
            "nether_wart" => &self.nether_wart,
            "nether_state_selector" => &self.state_selector,
            other => panic!("the Nether's surface rules read no noise {other}"),
        };
        n.sample(f64::from(x), 0.0, f64::from(z))
    }

    /// The column's depth and its biome's two noises: the wastes' soul sand
    /// and gravel layers, the valleys' and deltas' patch and state
    /// selector, the forests' netherrack and wart noises.
    pub(super) fn column(&self, biome: NetherBiome, x: i32, z: i32) -> SurfaceColumn {
        let names = match biome {
            NetherBiome::NetherWastes => ["soul_sand_layer", "gravel_layer"],
            NetherBiome::SoulSandValley | NetherBiome::BasaltDeltas => ["patch", "nether_state_selector"],
            NetherBiome::CrimsonForest | NetherBiome::WarpedForest => ["netherrack", "nether_wart"],
        };
        SurfaceColumn { depth: self.depth(x, z), noise: names.map(|n| self.noise(n, x, z)) }
    }
}

/// The blocks below height `n` (all of them from 128).
fn below(n: i32) -> u128 {
    if n >= 128 { u128::MAX } else { (1u128 << n.max(0)) - 1 }
}

/// `stoneDepthAbove` of solid block `y` in a column whose rock is `bits`:
/// the rock from `y` up to the first air (open above the lava sea, or
/// above the roof), counting `y`; lava doesn't end the run.
pub(super) fn stone_depth_above(bits: u128, y: i32) -> i32 {
    let air = !bits & !below(LAVA_SEA + 1) & !below(y + 1);
    let top = if air == 0 { 128 } else { air.trailing_zeros() as i32 };
    (bits & below(top) & !below(y)).count_ones() as i32
}

/// `stoneDepthBelow`: the rock from `y` down to the first air or lava (or
/// the bottom of the world), counting `y`.
pub(super) fn stone_depth_below(bits: u128, y: i32) -> i32 {
    (bits << (127 - y)).leading_ones() as i32
}

/// `worldgen/material_rule/nether.json` for a stone block at height `y`
/// that isn't bedrock and is below the five netherrack layers under the
/// roof, given its stone depths and its column.
pub(super) fn rule(biome: NetherBiome, y: i32, above: i32, below: i32, column: SurfaceColumn) -> Block {
    debug_assert!(y < ROOF - 5);
    let depth = column.depth;
    let [a, b] = column.noise;
    let hole = depth <= 0;
    let on_floor = above <= 1;
    let under_floor = above <= 1 + depth;
    let under_ceiling = below <= 1 + depth;
    // `y_above` with `add_stone_depth`: the surface's height.
    let start = y + above;
    // The gravel rule of the valleys and deltas: `patch` at or above
    // -0.012 where the surface is at y 30..=34.
    let gravel = a >= -0.012 && (30..35).contains(&start);
    match biome {
        NetherBiome::BasaltDeltas => {
            if under_ceiling {
                return Block::BASALT;
            }
            if under_floor {
                return if gravel {
                    Block::GRAVEL
                } else if b >= 0.0 {
                    Block::BASALT
                } else {
                    Block::BLACKSTONE
                };
            }
        }
        NetherBiome::SoulSandValley => {
            if under_ceiling {
                return if b >= 0.0 { Block::SOUL_SAND } else { nb::SOUL_SOIL };
            }
            if under_floor {
                return if gravel {
                    Block::GRAVEL
                } else if b >= 0.0 {
                    Block::SOUL_SAND
                } else {
                    nb::SOUL_SOIL
                };
            }
        }
        _ => {}
    }
    if on_floor {
        if y < 32 && hole {
            return Block::LAVA;
        }
        let wood = match biome {
            NetherBiome::WarpedForest => Some(nb::NetherWood::Warped),
            NetherBiome::CrimsonForest => Some(nb::NetherWood::Crimson),
            _ => None,
        };
        if let Some(wood) = wood
            && a < 0.54
            && y >= 31
        {
            return if b >= 1.17 { wood.wart() } else { wood.nylium() };
        }
    }
    if biome == NetherBiome::NetherWastes {
        if under_floor && a >= -0.012 {
            return if !hole && (30..35).contains(&start) { Block::SOUL_SAND } else { Block::NETHERRACK };
        }
        if on_floor && y >= 31 && start < 35 && b >= -0.012 && (y >= 32 || !hole) {
            return Block::GRAVEL;
        }
    }
    Block::NETHERRACK
}

#[cfg(test)]
#[path = "nether_surface_tests.rs"]
mod nether_surface_tests;
