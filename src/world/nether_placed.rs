//! Java 26.3's Nether placed features, as data, for generation 6 (v1.0
//! Track 8, GEN-04a).
//!
//! [`PLACED`] lists every placed feature the five Nether biomes name
//! (`worldgen/biome/*.json`), each with its placement modifiers copied
//! from `worldgen/placed_feature/<name>.json` and the configured feature
//! it places (`worldgen/feature`). `nether_decoration` runs the modifiers
//! the way `PlacedFeature.placeWithContext` does (each modifier maps every
//! position to zero or more positions, depth first) and places the feature
//! at each position that survives. `nether_generation_tests` checks the
//! table against `tests/fixtures/java/26.3/nether_generation.json` both
//! ways: every biome's lists by step and in order, every modifier and
//! every configured feature's parameters.
//!
//! Under FE-01 the terrain and biome noise are VoxelCraft's, and so is the
//! randomness: each modifier stage draws from a seed of its own position
//! in the stream (so a chunk that skips a branch never shifts another),
//! not from Java's shared decoration random.

use glam::IVec3;

use super::block::Block;
use super::nether_biome::NetherBiome;
use super::nether_biome_blocks::{self as nb, NetherWood};
use super::structure::Rng;

/// The Nether's generation depth: `above_bottom` and `below_top` anchors
/// resolve against `0..=127` (the noise settings' height 128).
const BOTTOM: i32 = 0;
const TOP: i32 = 127;

/// An `IntProvider`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IntP {
    Const(i32),
    /// `uniform`, inclusive.
    Uniform(i32, i32),
    /// `trapezoid` with its plateau.
    Trapezoid(i32, i32, i32),
    /// `biased_to_bottom`, inclusive.
    BiasedToBottom(i32, i32),
}

impl IntP {
    pub fn sample(self, rng: &mut Rng) -> i32 {
        let between = |rng: &mut Rng, lo: i32, hi: i32| lo + rng.below((hi - lo + 1) as u32) as i32;
        match self {
            IntP::Const(v) => v,
            IntP::Uniform(lo, hi) => between(rng, lo, hi),
            IntP::Trapezoid(min, max, plateau) => {
                // `TrapezoidInt.sample`.
                if plateau == 0 && max == -min {
                    return rng.below((max + 1) as u32) as i32 - rng.below((max + 1) as u32) as i32;
                }
                let range = max - min;
                if plateau == range {
                    return between(rng, min, max);
                }
                let start = (range - plateau) / 2;
                let end = range - start;
                min + between(rng, 0, end) + between(rng, 0, start)
            }
            IntP::BiasedToBottom(lo, hi) => {
                let n = rng.below((hi - lo + 1) as u32) as i32 + 1;
                lo + rng.below(n as u32) as i32
            }
        }
    }

    /// The smallest and largest value.
    pub fn bounds(self) -> (i32, i32) {
        match self {
            IntP::Const(v) => (v, v),
            IntP::Uniform(lo, hi) | IntP::Trapezoid(lo, hi, _) | IntP::BiasedToBottom(lo, hi) => (lo, hi),
        }
    }
}

/// A `VerticalAnchor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
}

impl Anchor {
    pub fn resolve(self) -> i32 {
        match self {
            Anchor::Absolute(y) => y,
            Anchor::AboveBottom(n) => BOTTOM + n,
            Anchor::BelowTop(n) => TOP - n,
        }
    }
}

/// A `HeightProvider`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    Uniform(Anchor, Anchor),
    /// `trapezoid` with no plateau.
    Trapezoid(Anchor, Anchor),
    /// `very_biased_to_bottom` with its `inner`.
    VeryBiased(Anchor, Anchor, i32),
}

impl Height {
    pub fn sample(self, rng: &mut Rng) -> i32 {
        let between =
            |rng: &mut Rng, lo: i32, hi: i32| if lo >= hi { lo } else { lo + rng.below((hi - lo + 1) as u32) as i32 };
        match self {
            Height::Uniform(lo, hi) => between(rng, lo.resolve(), hi.resolve()),
            Height::Trapezoid(lo, hi) => {
                // `TrapezoidHeight.sample` with plateau 0.
                let (lo, hi) = (lo.resolve(), hi.resolve());
                let half = (hi - lo) / 2;
                lo + between(rng, 0, hi - lo - half) + between(rng, 0, half)
            }
            Height::VeryBiased(lo, hi, inner) => {
                let (lo, hi) = (lo.resolve(), hi.resolve());
                let a = between(rng, lo + inner, hi);
                let b = between(rng, lo, a - 1);
                between(rng, lo, b - 1 + inner)
            }
        }
    }

    pub fn bounds(self) -> (i32, i32) {
        match self {
            Height::Uniform(lo, hi) | Height::Trapezoid(lo, hi) | Height::VeryBiased(lo, hi, _) => {
                (lo.resolve(), hi.resolve())
            }
        }
    }
}

/// A `BlockPredicate`, over Java block ids and tags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pred {
    /// `matching_block_tag` at an offset.
    Tag(&'static str, IVec3),
    /// `matching_blocks` at an offset.
    Blocks(&'static [&'static str], IVec3),
    Not(&'static Pred),
    All(&'static [Pred]),
}

/// The VoxelCraft block of a Java block id these features name.
pub fn java_block(id: &str) -> Block {
    match id {
        "netherrack" => Block::NETHERRACK,
        "basalt" => Block::BASALT,
        "blackstone" => Block::BLACKSTONE,
        "nether_wart_block" => nb::NETHER_WART_BLOCK,
        "warped_nylium" => nb::WARPED_NYLIUM,
        "warped_wart_block" => nb::WARPED_WART_BLOCK,
        "soul_soil" => nb::SOUL_SOIL,
        "soul_sand" => Block::SOUL_SAND,
        "gravel" => Block::GRAVEL,
        "magma_block" => Block::MAGMA,
        "nether_gold_ore" => super::gen5_blocks::NETHER_GOLD_ORE,
        "nether_quartz_ore" => Block::QUARTZ_ORE,
        "glowstone" => Block::GLOWSTONE,
        other => panic!("{other} is not one of the Nether features' blocks"),
    }
}

impl Pred {
    pub fn test(&self, block: &impl Fn(IVec3) -> Block, p: IVec3) -> bool {
        match *self {
            Pred::Tag(tag, o) => {
                let b = block(p + o);
                match tag {
                    "air" => b == Block::AIR,
                    "nylium" => nb::is_nylium(b),
                    other => panic!("block tag {other} is not modelled"),
                }
            }
            Pred::Blocks(ids, o) => {
                let b = block(p + o);
                ids.iter().any(|id| java_block(id) == b)
            }
            Pred::Not(inner) => !inner.test(block, p),
            Pred::All(all) => all.iter().all(|q| q.test(block, p)),
        }
    }
}

/// A `PlacementModifier`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mod {
    Count(IntP),
    CountOnEveryLayer(u32),
    InSquare,
    Height(Height),
    /// `heightmap` `MOTION_BLOCKING`.
    MotionBlocking,
    Biome,
    Rarity(u32),
    Offset(IntP, IntP, IntP),
    Filter(Pred),
    /// `environment_scan` down for `target`, at most this many steps.
    ScanDown(i32, Pred),
}

/// The plants a `simple_block` feature places.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Simple {
    Fire,
    SoulFire,
    BrownMushroom,
    RedMushroom,
    CrimsonRoots,
    NetherSprouts,
    /// `crimson_forest_vegetation`: roots 87, crimson fungus 11, warped fungus 1.
    CrimsonVegetation,
    /// `warped_forest_vegetation`: roots 85, crimson roots 1, warped fungus 13,
    /// crimson fungus 1.
    WarpedVegetation,
}

/// A configured feature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Feature {
    /// `spring_feature` of falling lava.
    Spring {
        valid: &'static [&'static str],
        requires_below: bool,
        rock: u8,
        hole: u8,
    },
    SimpleBlock(Simple),
    /// `random_neighbor_spread` of glowstone (`glowstone_extra`).
    Glowstone,
    /// `ore` replacing netherrack.
    Ore {
        size: i32,
        state: &'static str,
    },
    /// `scattered_ore` of ancient debris: placed by `NetherGen`'s own pass.
    Debris {
        size: i32,
    },
    /// `netherrack_replace_blobs`.
    Blobs {
        state: &'static str,
        radius: (i32, i32),
    },
    HugeFungus(NetherWood),
    WeepingVines,
    TwistingVines,
    Delta,
    Columns {
        large: bool,
    },
    BasaltPillar,
    /// The Overworld's `spring_lava_overworld`: Java lists it for four
    /// Nether biomes, but none of its valid blocks (stone, dirt...) occur
    /// in the Nether, so it never places.
    Inert,
}

/// A placed feature: its name, generation step, biomes (in Java's order
/// of the biome lists) and modifiers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub name: &'static str,
    pub step: u8,
    pub biomes: &'static [NetherBiome],
    pub feature: Feature,
    pub mods: &'static [Mod],
}

use Anchor::{AboveBottom, Absolute, BelowTop};
use NetherBiome::{BasaltDeltas as D, CrimsonForest as C, NetherWastes as W, SoulSandValley as S, WarpedForest as F};

const ALL: &[NetherBiome] = &[W, S, C, F, D];
const AIR: Pred = Pred::Tag("air", IVec3::ZERO);
const PATCH: [Mod; 3] = [
    Mod::Count(IntP::Const(96)),
    Mod::Offset(IntP::Trapezoid(-7, 7, 0), IntP::Trapezoid(-3, 3, 0), IntP::Trapezoid(-7, 7, 0)),
    Mod::Filter(AIR),
];
const VEGETATION: [Mod; 5] = [
    Mod::Biome,
    Mod::Filter(Pred::Tag("nylium", IVec3::NEG_Y)),
    Mod::Count(IntP::Const(64)),
    Mod::Offset(IntP::Trapezoid(-7, 7, 0), IntP::Trapezoid(-3, 3, 0), IntP::Trapezoid(-7, 7, 0)),
    Mod::Filter(AIR),
];
const NETHERRACK: &[&str] = &["netherrack"];
const GLOW_ROOF: Pred = Pred::All(&[AIR, Pred::Blocks(&["netherrack", "basalt", "blackstone"], IVec3::Y)]);
const TWIST_BASE: Pred =
    Pred::All(&[AIR, Pred::Blocks(&["netherrack", "warped_nylium", "warped_wart_block"], IVec3::NEG_Y)]);
const SPRING_DELTA: &[&str] = &["netherrack", "soul_sand", "gravel", "magma_block", "blackstone"];

const fn uniform(lo: Anchor, hi: Anchor) -> Mod {
    Mod::Height(Height::Uniform(lo, hi))
}

/// Every Nether placed feature, in VoxelCraft's global order: by step, then
/// an order consistent with every biome's list (Java's `FeatureSorter`).
pub static PLACED: &[Placed] = &[
    Placed {
        name: "basalt_pillar",
        step: 2,
        biomes: &[S],
        feature: Feature::BasaltPillar,
        mods: &[
            Mod::Count(IntP::Const(10)),
            Mod::InSquare,
            uniform(AboveBottom(0), BelowTop(0)),
            Mod::Filter(Pred::All(&[AIR, Pred::Not(&Pred::Tag("air", IVec3::Y))])),
            Mod::Biome,
        ],
    },
    Placed {
        name: "delta",
        step: 4,
        biomes: &[D],
        feature: Feature::Delta,
        mods: &[Mod::CountOnEveryLayer(40), Mod::Biome],
    },
    Placed {
        name: "small_basalt_columns",
        step: 4,
        biomes: &[D],
        feature: Feature::Columns { large: false },
        mods: &[Mod::CountOnEveryLayer(4), Mod::Biome],
    },
    Placed {
        name: "large_basalt_columns",
        step: 4,
        biomes: &[D],
        feature: Feature::Columns { large: true },
        mods: &[Mod::CountOnEveryLayer(2), Mod::Biome],
    },
    Placed {
        name: "basalt_blobs",
        step: 7,
        biomes: &[D],
        feature: Feature::Blobs { state: "basalt", radius: (3, 7) },
        mods: &[Mod::Count(IntP::Const(75)), Mod::InSquare, uniform(AboveBottom(0), BelowTop(0)), Mod::Biome],
    },
    Placed {
        name: "blackstone_blobs",
        step: 7,
        biomes: &[D],
        feature: Feature::Blobs { state: "blackstone", radius: (3, 7) },
        mods: &[Mod::Count(IntP::Const(25)), Mod::InSquare, uniform(AboveBottom(0), BelowTop(0)), Mod::Biome],
    },
    Placed {
        name: "spring_delta",
        step: 7,
        biomes: &[D],
        feature: Feature::Spring { valid: SPRING_DELTA, requires_below: true, rock: 4, hole: 1 },
        mods: &[Mod::Count(IntP::Const(16)), Mod::InSquare, uniform(AboveBottom(4), BelowTop(4)), Mod::Biome],
    },
    Placed {
        name: "spring_open",
        step: 7,
        biomes: &[W, S, C, F],
        feature: Feature::Spring { valid: NETHERRACK, requires_below: false, rock: 4, hole: 1 },
        mods: &[Mod::Count(IntP::Const(8)), Mod::InSquare, uniform(AboveBottom(4), BelowTop(4)), Mod::Biome],
    },
    Placed {
        name: "patch_fire",
        step: 7,
        biomes: ALL,
        feature: Feature::SimpleBlock(Simple::Fire),
        mods: &[
            Mod::Count(IntP::Uniform(0, 5)),
            Mod::InSquare,
            uniform(AboveBottom(4), BelowTop(4)),
            Mod::Biome,
            PATCH[0],
            PATCH[1],
            Mod::Filter(Pred::All(&[AIR, Pred::Blocks(&["netherrack"], IVec3::NEG_Y)])),
        ],
    },
    Placed {
        name: "patch_soul_fire",
        step: 7,
        biomes: &[W, S, F, D],
        feature: Feature::SimpleBlock(Simple::SoulFire),
        mods: &[
            Mod::Count(IntP::Uniform(0, 5)),
            Mod::InSquare,
            uniform(AboveBottom(4), BelowTop(4)),
            Mod::Biome,
            PATCH[0],
            PATCH[1],
            Mod::Filter(Pred::All(&[AIR, Pred::Blocks(&["soul_soil"], IVec3::NEG_Y)])),
        ],
    },
    Placed {
        name: "glowstone_extra",
        step: 7,
        biomes: ALL,
        feature: Feature::Glowstone,
        mods: &[
            Mod::Count(IntP::BiasedToBottom(0, 9)),
            Mod::InSquare,
            uniform(AboveBottom(4), BelowTop(4)),
            Mod::Filter(GLOW_ROOF),
            Mod::Biome,
        ],
    },
    Placed {
        name: "glowstone",
        step: 7,
        biomes: ALL,
        feature: Feature::Glowstone,
        mods: &[
            Mod::Count(IntP::Const(10)),
            Mod::InSquare,
            uniform(AboveBottom(0), BelowTop(0)),
            Mod::Filter(GLOW_ROOF),
            Mod::Biome,
        ],
    },
    Placed {
        name: "brown_mushroom_nether",
        step: 7,
        biomes: &[W, D],
        feature: Feature::SimpleBlock(Simple::BrownMushroom),
        mods: &[
            Mod::Rarity(2),
            Mod::InSquare,
            uniform(AboveBottom(0), BelowTop(0)),
            Mod::Biome,
            PATCH[0],
            PATCH[1],
            PATCH[2],
        ],
    },
    Placed {
        name: "red_mushroom_nether",
        step: 7,
        biomes: &[W, D],
        feature: Feature::SimpleBlock(Simple::RedMushroom),
        mods: &[
            Mod::Rarity(2),
            Mod::InSquare,
            uniform(AboveBottom(0), BelowTop(0)),
            Mod::Biome,
            PATCH[0],
            PATCH[1],
            PATCH[2],
        ],
    },
    Placed {
        name: "patch_crimson_roots",
        step: 7,
        biomes: &[S],
        feature: Feature::SimpleBlock(Simple::CrimsonRoots),
        mods: &[uniform(AboveBottom(0), BelowTop(0)), Mod::Biome, PATCH[0], PATCH[1], PATCH[2]],
    },
    Placed {
        name: "ore_magma",
        step: 7,
        biomes: ALL,
        feature: Feature::Ore { size: 33, state: "magma_block" },
        mods: &[Mod::Count(IntP::Const(4)), Mod::InSquare, uniform(Absolute(27), Absolute(36)), Mod::Biome],
    },
    Placed {
        name: "spring_closed",
        step: 7,
        biomes: &[W, S, C, F],
        feature: Feature::Spring { valid: NETHERRACK, requires_below: false, rock: 5, hole: 0 },
        mods: &[Mod::Count(IntP::Const(16)), Mod::InSquare, uniform(AboveBottom(10), BelowTop(10)), Mod::Biome],
    },
    Placed {
        name: "spring_closed_double",
        step: 7,
        biomes: &[D],
        feature: Feature::Spring { valid: NETHERRACK, requires_below: false, rock: 5, hole: 0 },
        mods: &[Mod::Count(IntP::Const(32)), Mod::InSquare, uniform(AboveBottom(10), BelowTop(10)), Mod::Biome],
    },
    Placed {
        name: "ore_soul_sand",
        step: 7,
        biomes: &[S],
        feature: Feature::Ore { size: 12, state: "soul_sand" },
        mods: &[Mod::Count(IntP::Const(12)), Mod::InSquare, uniform(AboveBottom(0), Absolute(31)), Mod::Biome],
    },
    Placed {
        name: "ore_gravel_nether",
        step: 7,
        biomes: &[W, S, C, F],
        feature: Feature::Ore { size: 33, state: "gravel" },
        mods: &[Mod::Count(IntP::Const(2)), Mod::InSquare, uniform(Absolute(5), Absolute(41)), Mod::Biome],
    },
    Placed {
        name: "ore_blackstone",
        step: 7,
        biomes: &[W, S, C, F],
        feature: Feature::Ore { size: 33, state: "blackstone" },
        mods: &[Mod::Count(IntP::Const(2)), Mod::InSquare, uniform(Absolute(5), Absolute(31)), Mod::Biome],
    },
    Placed {
        name: "ore_gold_nether",
        step: 7,
        biomes: &[W, S, C, F],
        feature: Feature::Ore { size: 10, state: "nether_gold_ore" },
        mods: &[Mod::Count(IntP::Const(10)), Mod::InSquare, uniform(AboveBottom(10), BelowTop(10)), Mod::Biome],
    },
    Placed {
        name: "ore_gold_deltas",
        step: 7,
        biomes: &[D],
        feature: Feature::Ore { size: 10, state: "nether_gold_ore" },
        mods: &[Mod::Count(IntP::Const(20)), Mod::InSquare, uniform(AboveBottom(10), BelowTop(10)), Mod::Biome],
    },
    Placed {
        name: "ore_quartz_nether",
        step: 7,
        biomes: &[W, S, C, F],
        feature: Feature::Ore { size: 14, state: "nether_quartz_ore" },
        mods: &[Mod::Count(IntP::Const(16)), Mod::InSquare, uniform(AboveBottom(10), BelowTop(10)), Mod::Biome],
    },
    Placed {
        name: "ore_quartz_deltas",
        step: 7,
        biomes: &[D],
        feature: Feature::Ore { size: 14, state: "nether_quartz_ore" },
        mods: &[Mod::Count(IntP::Const(32)), Mod::InSquare, uniform(AboveBottom(10), BelowTop(10)), Mod::Biome],
    },
    Placed {
        name: "ore_ancient_debris_large",
        step: 7,
        biomes: ALL,
        feature: Feature::Debris { size: 3 },
        mods: &[Mod::InSquare, Mod::Height(Height::Trapezoid(Absolute(8), Absolute(24))), Mod::Biome],
    },
    Placed {
        name: "ore_debris_small",
        step: 7,
        biomes: ALL,
        feature: Feature::Debris { size: 2 },
        mods: &[Mod::InSquare, uniform(AboveBottom(8), BelowTop(8)), Mod::Biome],
    },
    Placed {
        name: "spring_lava",
        step: 9,
        biomes: &[W, S, C, F],
        feature: Feature::Inert,
        mods: &[
            Mod::Count(IntP::Const(20)),
            Mod::InSquare,
            Mod::Height(Height::VeryBiased(AboveBottom(0), BelowTop(8), 8)),
            Mod::Biome,
        ],
    },
    Placed {
        name: "brown_mushroom_normal",
        step: 9,
        biomes: &[W, C, F],
        feature: Feature::SimpleBlock(Simple::BrownMushroom),
        mods: &[Mod::Rarity(256), Mod::InSquare, Mod::MotionBlocking, Mod::Biome, PATCH[0], PATCH[1], PATCH[2]],
    },
    Placed {
        name: "red_mushroom_normal",
        step: 9,
        biomes: &[W, C, F],
        feature: Feature::SimpleBlock(Simple::RedMushroom),
        mods: &[Mod::Rarity(512), Mod::InSquare, Mod::MotionBlocking, Mod::Biome, PATCH[0], PATCH[1], PATCH[2]],
    },
    Placed {
        name: "weeping_vines",
        step: 9,
        biomes: &[C],
        feature: Feature::WeepingVines,
        mods: &[
            Mod::Count(IntP::Const(10)),
            Mod::InSquare,
            uniform(AboveBottom(0), BelowTop(0)),
            Mod::Biome,
            Mod::Filter(Pred::All(&[AIR, Pred::Blocks(&["netherrack", "nether_wart_block"], IVec3::Y)])),
        ],
    },
    Placed {
        name: "crimson_fungi",
        step: 9,
        biomes: &[C],
        feature: Feature::HugeFungus(NetherWood::Crimson),
        mods: &[Mod::CountOnEveryLayer(8), Mod::Biome],
    },
    Placed {
        name: "crimson_forest_vegetation",
        step: 9,
        biomes: &[C],
        feature: Feature::SimpleBlock(Simple::CrimsonVegetation),
        mods: &[Mod::CountOnEveryLayer(6), VEGETATION[0], VEGETATION[1], VEGETATION[2], VEGETATION[3], VEGETATION[4]],
    },
    Placed {
        name: "warped_fungi",
        step: 9,
        biomes: &[F],
        feature: Feature::HugeFungus(NetherWood::Warped),
        mods: &[Mod::CountOnEveryLayer(8), Mod::Biome],
    },
    Placed {
        name: "warped_forest_vegetation",
        step: 9,
        biomes: &[F],
        feature: Feature::SimpleBlock(Simple::WarpedVegetation),
        mods: &[Mod::CountOnEveryLayer(5), VEGETATION[0], VEGETATION[1], VEGETATION[2], VEGETATION[3], VEGETATION[4]],
    },
    Placed {
        name: "nether_sprouts",
        step: 9,
        biomes: &[F],
        feature: Feature::SimpleBlock(Simple::NetherSprouts),
        mods: &[Mod::CountOnEveryLayer(4), VEGETATION[0], VEGETATION[1], VEGETATION[2], VEGETATION[3], VEGETATION[4]],
    },
    Placed {
        name: "twisting_vines",
        step: 9,
        biomes: &[F],
        feature: Feature::TwistingVines,
        mods: &[
            Mod::Count(IntP::Const(10)),
            Mod::InSquare,
            uniform(AboveBottom(0), BelowTop(0)),
            Mod::Biome,
            Mod::Filter(TWIST_BASE),
            Mod::Count(IntP::Const(64)),
            Mod::Offset(IntP::Uniform(-8, 8), IntP::Uniform(-4, 4), IntP::Uniform(-8, 8)),
            Mod::Offset(IntP::Const(0), IntP::Const(-1), IntP::Const(0)),
            Mod::ScanDown(32, Pred::Not(&AIR)),
            Mod::Offset(IntP::Const(0), IntP::Const(1), IntP::Const(0)),
            Mod::Filter(TWIST_BASE),
        ],
    },
];

/// The features that paint before every other: the pillars, deltas and
/// columns of steps 2 and 4, then the huge fungi and the vines.
pub const PAINT_FIRST: [&str; 8] = [
    "basalt_pillar",
    "delta",
    "small_basalt_columns",
    "large_basalt_columns",
    "crimson_fungi",
    "warped_fungi",
    "weeping_vines",
    "twisting_vines",
];

/// The order features paint in. First write wins and every feature reads
/// the undecorated terrain (so a chunk never depends on its neighbours), so
/// the tall features that must stay whole paint first, as generation 5's
/// do: Java's fungus stems replace the vines placed before them, and
/// Java's vines skip a spot an earlier plant or fire took, which here
/// keeps the plant or fire off the vine instead. The rest follow Java.
pub fn paint_order() -> Vec<usize> {
    let at = |n: &str| PLACED.iter().position(|p| p.name == n).unwrap();
    let mut order: Vec<usize> = PAINT_FIRST.iter().map(|n| at(n)).collect();
    order.extend((0..PLACED.len()).filter(|i| !PAINT_FIRST.contains(&PLACED[*i].name)));
    order
}

/// Whether `biome` lists this placed feature (Java's `BiomeFilter`).
pub fn in_biome(p: &Placed, biome: NetherBiome) -> bool {
    p.biomes.contains(&biome)
}

/// How far a feature can write from the position it is placed at:
/// horizontal radius and lowest and highest y offset.
pub fn reach(f: Feature) -> (i32, i32, i32) {
    match f {
        Feature::Spring { .. } | Feature::SimpleBlock(_) | Feature::Inert | Feature::Debris { .. } => (0, 0, 0),
        // `OreFeature`: spread size/8 plus the largest sphere radius.
        Feature::Ore { size, .. } => {
            let r = (size as f32 / 8.0).ceil() as i32 + ((size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil() as i32 + 1;
            (r, -(2 + r), 2 + r)
        }
        Feature::Glowstone => (8, -12, 1),
        Feature::Blobs { radius, .. } => (radius.1, -super::nether::ROOF, radius.1),
        Feature::Delta => (9, -1, -1),
        Feature::Columns { large: false } => (9, -6, 6),
        Feature::Columns { large: true } => (11, -6, 16),
        Feature::BasaltPillar => (4, -super::nether::ROOF, 1),
        Feature::HugeFungus(_) => (4, -12, 28),
        // Wart spread (5 out, -4..1) and columns hung from 100 offsets
        // (7 out, -4..1) up to 17 + 1 long.
        Feature::WeepingVines => (8, -24, 2),
        Feature::TwistingVines => (0, 0, 17),
    }
}

impl Feature {
    /// Every placed feature's own debris pass stays `NetherGen`'s; the
    /// interpreter skips these and the inert spring.
    pub fn runs(self) -> bool {
        !matches!(self, Feature::Debris { .. } | Feature::Inert)
    }
}

#[cfg(test)]
#[path = "nether_placed_tests.rs"]
mod nether_generation_tests;
