//! Java's structure processor lists as data (v0.15 Track G, FE-02).
//!
//! The generation 4 catalogues name a processor list per pool element, as
//! Java's `worldgen/template_pool` files do. [`LISTS`] copies the lists the
//! village, trial chamber and ancient city pools name from
//! `worldgen/processor_list/*.json`; `structure_catalogue_tests` checks it
//! against the jar fixture both ways. [`Compiled::apply`] runs them the way
//! `StructureProcessor.processBlock` does: each step in order on the block
//! the previous step produced; `RuleProcessor` and `BlockRotProcessor` each
//! seed a fresh `RandomSource.create(Mth.getSeed(pos))`; a rule's
//! `random_block_match` draws a float only when its block matches, and the
//! first rule whose input and location tests pass wins.
//!
//! VoxelCraft names some blocks differently (see [`local_name`]); outputs'
//! properties are applied where VoxelCraft has them (lit copper bulbs, unlit
//! campfires) and ignored otherwise (glass pane connections, which are drawn
//! from neighbours here).

use std::sync::OnceLock;

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::super::block::{Block, STATE_CAPACITY};

/// A `RuleTest` (input or location predicate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Test {
    /// `always_true`.
    Always,
    /// `block_match`.
    Block(&'static str),
    /// `random_block_match`: the block, then a float below the probability.
    Random(&'static str, f32),
    /// `tag_match`.
    Tag(&'static str),
    /// `blockstate_match`: one state of a block.
    State(&'static str, &'static [(&'static str, &'static str)]),
}

/// A `ProcessorRule`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub input: Test,
    pub location: Test,
    pub output: &'static str,
    pub props: &'static [(&'static str, &'static str)],
    /// The rule's `position_predicate` (`None`: `always_true`).
    pub pos: Option<PosTest>,
}

/// A `PosRuleTest` (a rule's position predicate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PosTest {
    /// `axis_aligned_linear_pos` on the y axis: a float at most the chance
    /// lerped from `min_chance` to `max_chance` as the distance along y from
    /// the piece's position goes from `min_dist` to `max_dist`.
    LinearY { min_chance: f32, max_chance: f32, min_dist: i32, max_dist: i32 },
}

impl PosTest {
    /// `AxisAlignedLinearPosTest.test`: `at` is the block's world position,
    /// `reference` the piece's.
    fn test(&self, at: IVec3, reference: IVec3, random: &mut crate::enchant::JavaRandom) -> bool {
        match *self {
            PosTest::LinearY { min_chance, max_chance, min_dist, max_dist } => {
                let dist = (at.y - reference.y).abs();
                let rnd = random.next_float();
                // `Mth.clampedLerp(Mth.inverseLerp(dist, min, max), lo, hi)`.
                let t = (dist - min_dist) as f32 / (max_dist - min_dist) as f32;
                let chance = if t < 0.0 {
                    min_chance
                } else if t > 1.0 {
                    max_chance
                } else {
                    min_chance + t * (max_chance - min_chance)
                };
                rnd <= chance
            }
        }
    }
}

/// One processor of a list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// `block_rot`: blocks of the tag survive with probability `integrity`.
    Rot { integrity: f32, rottable: &'static str },
    /// `rule`.
    Rules(&'static [Rule]),
    /// `protected_blocks`: the world's blocks of this tag are never replaced.
    Protected(&'static str),
}

// Copied from `worldgen/processor_list/<name>.json` (Java 26.3).
#[rustfmt::skip]
pub static LISTS: &[(&str, &[Step])] = &[
    (
        "ancient_city_generic_degradation",
        &[
            Step::Rot { integrity: 0.95, rottable: "ancient_city_replaceable" },
            Step::Rules(&[
                Rule { input: Test::Random("deepslate_bricks", 0.3), location: Test::Always, output: "cracked_deepslate_bricks", props: &[], pos: None },
                Rule { input: Test::Random("deepslate_tiles", 0.3), location: Test::Always, output: "cracked_deepslate_tiles", props: &[], pos: None },
                Rule { input: Test::Random("soul_lantern", 0.05), location: Test::Always, output: "air", props: &[], pos: None },
            ]),
            Step::Protected("features_cannot_replace"),
        ],
    ),
    (
        "ancient_city_start_degradation",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("deepslate_bricks", 0.3), location: Test::Always, output: "cracked_deepslate_bricks", props: &[], pos: None },
                Rule { input: Test::Random("deepslate_tiles", 0.3), location: Test::Always, output: "cracked_deepslate_tiles", props: &[], pos: None },
                Rule { input: Test::Random("soul_lantern", 0.05), location: Test::Always, output: "air", props: &[], pos: None },
            ]),
            Step::Protected("features_cannot_replace"),
        ],
    ),
    (
        "ancient_city_walls_degradation",
        &[
            Step::Rot { integrity: 0.95, rottable: "ancient_city_replaceable" },
            Step::Rules(&[
                Rule { input: Test::Random("deepslate_bricks", 0.3), location: Test::Always, output: "cracked_deepslate_bricks", props: &[], pos: None },
                Rule { input: Test::Random("deepslate_tiles", 0.3), location: Test::Always, output: "cracked_deepslate_tiles", props: &[], pos: None },
                Rule { input: Test::Random("deepslate_tile_slab", 0.3), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("soul_lantern", 0.05), location: Test::Always, output: "air", props: &[], pos: None },
            ]),
            Step::Protected("features_cannot_replace"),
        ],
    ),
    (
        "farm_desert",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("wheat", 0.2), location: Test::Always, output: "beetroots", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "melon_stem", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "farm_plains",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("wheat", 0.3), location: Test::Always, output: "carrots", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.2), location: Test::Always, output: "potatoes", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "beetroots", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "farm_savanna",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "melon_stem", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "farm_snowy",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "carrots", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.8), location: Test::Always, output: "potatoes", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "farm_taiga",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("wheat", 0.3), location: Test::Always, output: "pumpkin_stem", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.2), location: Test::Always, output: "potatoes", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "mossify_10_percent",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("cobblestone", 0.1), location: Test::Always, output: "mossy_cobblestone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "mossify_20_percent",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("cobblestone", 0.2), location: Test::Always, output: "mossy_cobblestone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "mossify_70_percent",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("cobblestone", 0.7), location: Test::Always, output: "mossy_cobblestone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "street_plains",
        &[
            Step::Rules(&[
                Rule { input: Test::Block("dirt_path"), location: Test::Block("water"), output: "oak_planks", props: &[], pos: None },
                Rule { input: Test::Random("dirt_path", 0.1), location: Test::Always, output: "grass_block", props: &[], pos: None },
                Rule { input: Test::Block("grass_block"), location: Test::Block("water"), output: "water", props: &[], pos: None },
                Rule { input: Test::Block("dirt"), location: Test::Block("water"), output: "water", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "street_savanna",
        &[
            Step::Rules(&[
                Rule { input: Test::Block("dirt_path"), location: Test::Block("water"), output: "acacia_planks", props: &[], pos: None },
                Rule { input: Test::Random("dirt_path", 0.2), location: Test::Always, output: "grass_block", props: &[], pos: None },
                Rule { input: Test::Block("grass_block"), location: Test::Block("water"), output: "water", props: &[], pos: None },
                Rule { input: Test::Block("dirt"), location: Test::Block("water"), output: "water", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "street_snowy_or_taiga",
        &[
            Step::Rules(&[
                Rule { input: Test::Block("dirt_path"), location: Test::Block("water"), output: "spruce_planks", props: &[], pos: None },
                Rule { input: Test::Block("dirt_path"), location: Test::Block("ice"), output: "spruce_planks", props: &[], pos: None },
                Rule { input: Test::Random("dirt_path", 0.2), location: Test::Always, output: "grass_block", props: &[], pos: None },
                Rule { input: Test::Block("grass_block"), location: Test::Block("water"), output: "water", props: &[], pos: None },
                Rule { input: Test::Block("dirt"), location: Test::Block("water"), output: "water", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "trial_chambers_copper_bulb_degradation",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("waxed_copper_bulb", 0.1), location: Test::Always, output: "waxed_oxidized_copper_bulb", props: &[("lit", "true"), ("powered", "false")], pos: None },
                Rule { input: Test::Random("waxed_copper_bulb", 0.33333334), location: Test::Always, output: "waxed_weathered_copper_bulb", props: &[("lit", "true"), ("powered", "false")], pos: None },
                Rule { input: Test::Random("waxed_copper_bulb", 0.5), location: Test::Always, output: "waxed_exposed_copper_bulb", props: &[("lit", "true"), ("powered", "false")], pos: None },
            ]),
            Step::Protected("features_cannot_replace"),
        ],
    ),
    (
        "zombie_desert",
        &[
            Step::Rules(&[
                Rule { input: Test::Tag("doors"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("wall_torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("smooth_sandstone", 0.08), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("cut_sandstone", 0.1), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("terracotta", 0.08), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("smooth_sandstone_stairs", 0.08), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("smooth_sandstone_slab", 0.08), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.2), location: Test::Always, output: "beetroots", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "melon_stem", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "zombie_plains",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("cobblestone", 0.8), location: Test::Always, output: "mossy_cobblestone", props: &[], pos: None },
                Rule { input: Test::Tag("doors"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("wall_torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("cobblestone", 0.07), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("mossy_cobblestone", 0.07), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("white_terracotta", 0.07), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("oak_log", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("oak_planks", 0.1), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("oak_stairs", 0.1), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("stripped_oak_log", 0.02), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("glass_pane", 0.5), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")], pos: None },
                Rule { input: Test::Random("wheat", 0.3), location: Test::Always, output: "carrots", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.2), location: Test::Always, output: "potatoes", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "beetroots", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "zombie_savanna",
        &[
            Step::Rules(&[
                Rule { input: Test::Tag("doors"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("wall_torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("acacia_planks", 0.2), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("acacia_stairs", 0.2), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("acacia_log", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("acacia_wood", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("orange_terracotta", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("yellow_terracotta", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("red_terracotta", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("glass_pane", 0.5), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")], pos: None },
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "melon_stem", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "zombie_snowy",
        &[
            Step::Rules(&[
                Rule { input: Test::Tag("doors"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("wall_torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("lantern"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("spruce_planks", 0.2), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("spruce_slab", 0.4), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("stripped_spruce_log", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("stripped_spruce_wood", 0.05), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("glass_pane", 0.5), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")], pos: None },
                Rule { input: Test::Random("wheat", 0.1), location: Test::Always, output: "carrots", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.8), location: Test::Always, output: "potatoes", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "zombie_taiga",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("cobblestone", 0.8), location: Test::Always, output: "mossy_cobblestone", props: &[], pos: None },
                Rule { input: Test::Tag("doors"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("wall_torch"), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Block("campfire"), location: Test::Always, output: "campfire", props: &[("facing", "north"), ("lit", "false"), ("signal_fire", "false"), ("waterlogged", "false")], pos: None },
                Rule { input: Test::Random("cobblestone", 0.08), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("spruce_log", 0.08), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::Random("glass_pane", 0.5), location: Test::Always, output: "cobweb", props: &[], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "false"), ("north", "true"), ("south", "true"), ("waterlogged", "false"), ("west", "false")], pos: None },
                Rule { input: Test::State("glass_pane", &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")]), location: Test::Always, output: "brown_stained_glass_pane", props: &[("east", "true"), ("north", "false"), ("south", "false"), ("waterlogged", "false"), ("west", "true")], pos: None },
                Rule { input: Test::Random("wheat", 0.3), location: Test::Always, output: "pumpkin_stem", props: &[], pos: None },
                Rule { input: Test::Random("wheat", 0.2), location: Test::Always, output: "potatoes", props: &[], pos: None },
            ]),
        ],
    ),
    // Bastion remnants (v1.0 Track 8, GEN-05g).
    (
        "bastion_generic_degradation",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.0001), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("gold_block", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "bottom_rampart",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("magma_block", 0.75), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("cracked_polished_blackstone_bricks", 0.15), location: Test::Always, output: "polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "bridge",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.0001), location: Test::Always, output: "air", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "entrance_replacement",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("chiseled_polished_blackstone", 0.5), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("gold_block", 0.6), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "high_rampart",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("gold_block", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Always, location: Test::Always, output: "air", props: &[], pos: Some(PosTest::LinearY { min_chance: 0.0, max_chance: 0.05, min_dist: 0, max_dist: 100 }) },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "high_wall",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.01), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("polished_blackstone_bricks", 0.5), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("polished_blackstone_bricks", 0.3), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "housing",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.0001), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "rampart_degradation",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.4), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("polished_blackstone_bricks", 0.0001), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.0001), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("gold_block", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "roof",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.3), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("polished_blackstone_bricks", 0.15), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("polished_blackstone_bricks", 0.3), location: Test::Always, output: "blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "side_wall_degradation",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("chiseled_polished_blackstone", 0.5), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("gold_block", 0.1), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "stable_degradation",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.1), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.0001), location: Test::Always, output: "air", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
    (
        "treasure_rooms",
        &[
            Step::Rules(&[
                Rule { input: Test::Random("polished_blackstone_bricks", 0.35), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("chiseled_polished_blackstone", 0.1), location: Test::Always, output: "cracked_polished_blackstone_bricks", props: &[], pos: None },
                Rule { input: Test::Random("gilded_blackstone", 0.5), location: Test::Always, output: "blackstone", props: &[], pos: None },
                Rule { input: Test::Random("blackstone", 0.01), location: Test::Always, output: "gilded_blackstone", props: &[], pos: None },
            ]),
        ],
    ),
];

/// VoxelCraft's name for a Java block id where it differs (the rest swap
/// underscores for spaces).
pub fn local_name(java: &str) -> String {
    match java {
        "grass_block" => "grass".into(),
        "wheat" => "wheat crops".into(),
        "carrots" => "carrot crops".into(),
        "potatoes" => "potato crops".into(),
        // VoxelCraft's torch block covers wall torches.
        "wall_torch" => "torch".into(),
        "gold_block" => "block of gold".into(),
        other => other.replace('_', " "),
    }
}

/// Every state of each block name, built once.
fn states() -> &'static FxHashMap<&'static str, Vec<u16>> {
    static STATES: OnceLock<FxHashMap<&'static str, Vec<u16>>> = OnceLock::new();
    STATES.get_or_init(|| {
        let mut out: FxHashMap<&'static str, Vec<u16>> = FxHashMap::default();
        for id in 0..STATE_CAPACITY as u16 {
            out.entry(Block(id).name()).or_default().push(id);
        }
        out
    })
}

/// The states of a Java block id (empty if VoxelCraft lacks the block).
pub fn states_of(java: &str) -> &'static [u16] {
    let name = local_name(java);
    if name == "air" {
        return &[0];
    }
    states().get(name.as_str()).map_or(&[], Vec::as_slice)
}

/// The block a rule outputs, with the properties VoxelCraft models.
pub fn output_block(java: &str, props: &[(&str, &str)]) -> Option<Block> {
    if java == "air" {
        return Some(Block::AIR);
    }
    let base = Block::from_name(&local_name(java))?;
    let prop = |k: &str| props.iter().find(|p| p.0 == k).map(|p| p.1);
    if let Some(c) = super::super::copper::state(base)
        && prop("lit") == Some("true")
    {
        return Some(super::super::copper::encode(super::super::copper::Copper { local: 1, ..c }));
    }
    if let Some(c) = super::super::utilities::campfire_of(base)
        && let Some(lit) = prop("lit")
    {
        return Some(super::super::utilities::campfire(super::super::utilities::Campfire { lit: lit == "true", ..c }));
    }
    Some(base)
}

#[derive(Debug)]
enum Matcher {
    States(Vec<u16>),
    Doors,
    AncientCityReplaceable,
    FeaturesCannotReplace,
}

impl Matcher {
    fn of_block(java: &str) -> Matcher {
        let mut s = states_of(java).to_vec();
        s.sort_unstable();
        Matcher::States(s)
    }

    fn of_tag(tag: &str) -> Matcher {
        match tag {
            "doors" => Matcher::Doors,
            "ancient_city_replaceable" => Matcher::AncientCityReplaceable,
            "features_cannot_replace" => Matcher::FeaturesCannotReplace,
            other => panic!("processor tag {other} is not modelled"),
        }
    }

    fn matches(&self, b: Block) -> bool {
        match self {
            Matcher::States(s) => s.binary_search(&b.0).is_ok(),
            Matcher::Doors => b.is_door(),
            Matcher::AncientCityReplaceable => super::super::ancient_city::rottable(b),
            Matcher::FeaturesCannotReplace => {
                b == Block::BEDROCK
                    || b == Block::SPAWNER
                    || b.base() == Block::CHEST
                    || b.base() == Block::END_PORTAL_FRAME
                    || b == super::super::jukebox::REINFORCED_DEEPSLATE
                    || super::super::trial::spawner(b).is_some()
                    || super::super::trial::vault(b).is_some()
            }
        }
    }
}

#[derive(Debug)]
enum CTest {
    Always,
    Is(Matcher),
    Random(Matcher, f32),
}

impl CTest {
    fn of(t: &Test) -> CTest {
        match *t {
            Test::Always => CTest::Always,
            Test::Block(b) => CTest::Is(Matcher::of_block(b)),
            Test::Random(b, p) => CTest::Random(Matcher::of_block(b), p),
            Test::Tag(t) => CTest::Is(Matcher::of_tag(t)),
            // Glass panes have no connection states here; a pane matches.
            Test::State(b, _) => CTest::Is(Matcher::of_block(b)),
        }
    }

    fn test(&self, b: Block, random: &mut crate::enchant::JavaRandom) -> bool {
        match self {
            CTest::Always => true,
            CTest::Is(m) => m.matches(b),
            CTest::Random(m, p) => m.matches(b) && random.next_float() < *p,
        }
    }
}

#[derive(Debug)]
enum CStep {
    Rot(f32, Matcher),
    Rules(Vec<(CTest, CTest, Option<PosTest>, Block)>),
    Protected(Matcher),
}

/// A processor list ready to apply.
#[derive(Debug)]
pub struct Compiled {
    steps: Vec<CStep>,
}

impl Compiled {
    fn of(steps: &[Step]) -> Compiled {
        let steps = steps
            .iter()
            .map(|s| match *s {
                Step::Rot { integrity, rottable } => CStep::Rot(integrity, Matcher::of_tag(rottable)),
                Step::Rules(rules) => CStep::Rules(
                    rules
                        .iter()
                        .map(|r| {
                            let out = output_block(r.output, r.props)
                                .unwrap_or_else(|| panic!("processor output {} is not a block", r.output));
                            (CTest::of(&r.input), CTest::of(&r.location), r.pos, out)
                        })
                        .collect(),
                ),
                Step::Protected(tag) => CStep::Protected(Matcher::of_tag(tag)),
            })
            .collect();
        Compiled { steps }
    }

    /// The block placed at `at` for template block `block` over the
    /// world's `existing` block (`None` keeps the world's block).
    pub fn apply(&self, block: Block, at: IVec3, existing: Block) -> Option<Block> {
        self.apply_in(block, at, existing, at)
    }

    /// [`Compiled::apply`] for a piece placed at `reference` (the world
    /// position position predicates measure from).
    pub fn apply_in(&self, block: Block, at: IVec3, existing: Block, reference: IVec3) -> Option<Block> {
        let mut block = block;
        for step in &self.steps {
            match step {
                CStep::Rot(integrity, rottable) => {
                    if rottable.matches(block) && super::processor_random(at).next_float() > *integrity {
                        return None;
                    }
                }
                CStep::Rules(rules) => {
                    let mut random = super::processor_random(at);
                    for (input, location, pos, out) in rules {
                        if input.test(block, &mut random)
                            && location.test(existing, &mut random)
                            && pos.is_none_or(|p| p.test(at, reference, &mut random))
                        {
                            block = *out;
                            break;
                        }
                    }
                }
                CStep::Protected(tag) => {
                    if tag.matches(existing) {
                        return None;
                    }
                }
            }
        }
        Some(block)
    }
}

/// The compiled list of a name in [`LISTS`].
pub fn list(name: &str) -> Option<&'static Compiled> {
    static COMPILED: OnceLock<FxHashMap<&'static str, Compiled>> = OnceLock::new();
    COMPILED.get_or_init(|| LISTS.iter().map(|(n, steps)| (*n, Compiled::of(steps))).collect()).get(name)
}
