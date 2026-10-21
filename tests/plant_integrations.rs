//! Regression expectations from the selected official Java 26.3 artifact.
use glam::IVec3;
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    enchant::{Enchantment, Enchants},
    inventory::Stack,
    item::Item,
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        overworld_blocks as ob,
        terrain::{Dimension, Generator},
    },
};
const P: IVec3 = IVec3::new(12, 145, 12);
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/plant_integrations.json")).unwrap()
}
fn world(dimension: Dimension) -> World {
    let generator = Generator::for_dimension(7, dimension);
    let mut w = World::new_remote(Arc::new(generator), 2);
    for y in 4..6 {
        w.receive_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    }
    w
}
fn reference_item(name: &str) -> Item {
    let name = name.trim_start_matches("minecraft:");
    let alias = match name {
        "hay_block" => "hay_bale",
        "short_grass" => "tall_grass",
        "tall_grass" => "tall_grass_plant",
        "vine" => "vines",
        _ => name,
    };
    Item::from_name(alias).unwrap_or_else(|| panic!("missing fixture item {name}"))
}
#[test]
fn all_supported_compostables_match_reference_probabilities_and_first_level() {
    let mut w = world(Dimension::Overworld);
    for case in fixture()["compost"].as_array().unwrap() {
        let item = reference_item(case["item"].as_str().unwrap());
        let expected = case["chance_percent"].as_u64().unwrap() as f32 / 100.0;
        assert_eq!(voxelcraft::world::composter::chance(item), expected, "{}", case["item"]);
        w.set_block(P, Block::COMPOSTER);
        assert!(w.compost(P, item));
        assert_eq!(w.get_block(P), Some(voxelcraft::world::composter::state(1)));
    }
    for item in [Item::POISONOUS_POTATO, Item::IRON_INGOT, Item::from_block(ob::BAMBOO_PLANKS)] {
        assert_eq!(voxelcraft::world::composter::chance(item), 0.0);
    }
}
#[test]
fn cocoa_is_three_when_ripe_and_budding_amethyst_never_drops() {
    let mut w = world(Dimension::Overworld);
    for fortune in 0..=3 {
        for age in 0..=2 {
            for face in Facing::ALL {
                for _ in 0..20 {
                    w.drops.clear();
                    w.spill_mined(P, ob::cocoa(age, face), Enchants::NONE.with(Enchantment::Fortune, fortune));
                    assert_eq!(w.drops, vec![(P, Stack::new(Item::COCOA_BEANS, if age == 2 { 3 } else { 1 }))]);
                }
            }
        }
    }
    for enchants in
        [Enchants::NONE, Enchants::NONE.with(Enchantment::SilkTouch, 1), Enchants::NONE.with(Enchantment::Fortune, 3)]
    {
        w.drops.clear();
        w.spill_mined(P, ob::BUDDING_AMETHYST, enchants);
        assert!(w.drops.is_empty());
    }
}
#[test]
fn fish_weights_include_tropical_and_pufferfish() {
    for (actual, expected) in voxelcraft::content::fishing::FISH.iter().zip(fixture()["fish"].as_array().unwrap()) {
        assert_eq!(actual.value.item, reference_item(expected["item"].as_str().unwrap()));
        assert_eq!(actual.weight, expected["weight"].as_i64().unwrap() as i32);
    }
}
#[test]
fn sponge_absorbs_connected_water_six_steps_and_respects_walls_and_limit() {
    let mut w = world(Dimension::Overworld);
    for x in 1..=7 {
        w.set_block(P + IVec3::X * x, Block::WATER);
    }
    w.set_block(P - IVec3::X, Block::STONE);
    w.set_block(P - IVec3::X * 2, Block::WATER);
    w.set_block(P, ob::SPONGE);
    assert_eq!(w.get_block(P), Some(ob::WET_SPONGE));
    for x in 1..=6 {
        assert_eq!(w.get_block(P + IVec3::X * x), Some(Block::AIR));
    }
    assert_eq!(w.get_block(P + IVec3::X * 7), Some(Block::WATER));
    assert_eq!(w.get_block(P - IVec3::X * 2), Some(Block::WATER));
    let mut w = world(Dimension::Overworld);
    for x in -5..=5 {
        for y in -5..=5 {
            for z in -5..=5 {
                w.set_block(P + IVec3::new(x, y, z), Block::WATER);
            }
        }
    }
    w.set_block(P, ob::SPONGE);
    let removed = (-5..=5)
        .flat_map(|x| (-5..=5).flat_map(move |y| (-5..=5).map(move |z| IVec3::new(x, y, z))))
        .filter(|d| w.get_block(P + *d) == Some(Block::AIR))
        .count();
    assert_eq!(removed, 64);
    let mut nether = world(Dimension::Nether);
    nether.set_block(P, ob::WET_SPONGE);
    assert_eq!(nether.get_block(P), Some(ob::SPONGE));
}
/// Java 26.3 `SpongeBlock.removeWaterBreadthFirstSearch` tries
/// `BucketPickup.pickupBlock` first: a waterlogged block loses its water,
/// counts toward the 64 and the search continues through it.
#[test]
fn sponge_drains_waterlogged_coral_and_continues_through_it() {
    let mut w = world(Dimension::Overworld);
    let fan = ob::coral(2, 0, false);
    let plant = ob::coral(1, 3, false);
    for x in 2..4 {
        w.set_block(P + IVec3::X * x - IVec3::Y, Block::STONE);
    }
    w.set_block(P + IVec3::X, Block::WATER);
    w.set_block(P + IVec3::X * 2, fan);
    w.set_block(P + IVec3::X * 3, plant);
    w.set_block(P + IVec3::X * 4, Block::WATER);
    w.set_block(P, ob::SPONGE);
    assert_eq!(w.get_block(P), Some(ob::WET_SPONGE));
    assert_eq!(w.get_block(P + IVec3::X), Some(Block::AIR));
    // `SimpleWaterloggedBlock.pickupBlock` leaves dry live coral, which
    // dies later on its scheduled tick.
    assert_eq!(w.get_block(P + IVec3::X * 2), Some(fan.with_water(false)));
    assert_eq!(w.get_block(P + IVec3::X * 3), Some(plant.with_water(false)));
    assert!(!fan.with_water(false).is_waterlogged() && fan.with_water(false) != fan);
    assert_eq!(w.get_block(P + IVec3::X * 4), Some(Block::AIR), "the search continues past waterlogged blocks");
    assert!(w.drops.is_empty(), "drained coral is not broken");
}
/// `WetSpongeBlock.onPlace` dries a wet sponge only when it is placed where
/// water evaporates; neighbour changes do not.
#[test]
fn nether_wet_sponge_dries_only_when_placed() {
    let generator = Generator::for_dimension(7, Dimension::Nether);
    let mut w = World::new_remote(Arc::new(generator), 2);
    w.receive_chunk(IVec3::new(0, 3, 0), Arc::new(ChunkData::Uniform(ob::WET_SPONGE)));
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let top = IVec3::new(12, 127, 12);
    assert_eq!(w.get_block(top), Some(ob::WET_SPONGE));
    w.set_block(top + IVec3::Y, Block::STONE);
    w.set_block(top + IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(top), Some(ob::WET_SPONGE), "a neighbour change does not dry it");
    w.set_block(top + IVec3::Y, ob::WET_SPONGE);
    assert_eq!(w.get_block(top + IVec3::Y), Some(ob::SPONGE), "placing one dries it");
    assert_eq!(w.get_block(top), Some(ob::WET_SPONGE));
}
#[test]
fn dry_sponge_responds_to_water_placed_after_it() {
    let mut w = world(Dimension::Overworld);
    w.set_block(P, ob::SPONGE);
    assert_eq!(w.get_block(P), Some(ob::SPONGE));
    w.set_block(P + IVec3::X, Block::WATER);
    assert_eq!(w.get_block(P), Some(ob::WET_SPONGE));
    assert_eq!(w.get_block(P + IVec3::X), Some(Block::AIR));
}
#[test]
fn dark_oak_requires_four_saplings_and_mangrove_propagules_mature_under_leaves() {
    let mut w = world(Dimension::Overworld);
    for d in [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
        w.set_block(P + d - IVec3::Y, Block::DIRT);
    }
    w.set_block(P, ob::DARK_OAK_SAPLING);
    assert!(!w.grow_tree(P));
    assert_eq!(w.get_block(P), Some(ob::DARK_OAK_SAPLING));
    for d in [IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
        w.set_block(P + d, ob::DARK_OAK_SAPLING);
    }
    assert!(w.grow_tree(P + IVec3::X + IVec3::Z));
    for d in [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
        assert!(w.get_block(P + d).unwrap().is_log());
    }
    let mut w = world(Dimension::Overworld);
    w.set_block(P + IVec3::Y, ob::MANGROVE_LEAVES);
    assert!(w.apply_bone_meal(P + IVec3::Y));
    assert_eq!(w.get_block(P), Some(ob::hanging_propagule(0)));
    assert!(!w.apply_bone_meal(P + IVec3::Y));
    for age in 0..4 {
        w.drops.clear();
        w.spill_block(P, ob::hanging_propagule(age));
        assert!(w.drops.is_empty());
        assert!(w.apply_bone_meal(P));
        assert_eq!(w.get_block(P), Some(ob::hanging_propagule(age + 1)));
    }
    assert!(!w.apply_bone_meal(P));
    w.set_block(P + IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    assert_eq!(w.drops, vec![(P, Stack::new(ob::MANGROVE_PROPAGULE, 1))]);
}
/// Java 26.3 `TreeGrower.PALE_OAK` has only `PALE_OAK_BONEMEAL` (a 2x2 tree);
/// `SaplingBlock.isValidBonemealTarget` uses `TreeGrower.canGrow`, and
/// `findTwoByTwoSaplingPos` tries x then z offsets from 0 down to -1.
#[test]
fn pale_and_dark_oak_need_a_two_by_two_and_lone_ones_keep_bone_meal() {
    for sapling in [ob::DARK_OAK_SAPLING, ob::PALE_OAK_SAPLING] {
        let mut w = world(Dimension::Overworld);
        for x in -1..=2 {
            for z in -1..=2 {
                w.set_block(P + IVec3::new(x, -1, z), Block::DIRT);
            }
        }
        w.set_block(P, sapling);
        for _ in 0..64 {
            assert!(!w.apply_bone_meal(P), "a lone sapling is not a bone-meal target");
            assert!(!w.grow_tree(P));
        }
        assert_eq!(w.get_block(P), Some(sapling));
        // A 3x3 of saplings: the corner at offset (0, 0) wins over (-1, -1).
        for x in -1..=1 {
            for z in -1..=1 {
                w.set_block(P + IVec3::new(x, 0, z), sapling);
            }
        }
        assert!(w.grow_tree(P));
        for d in [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
            assert!(w.get_block(P + d).unwrap().is_log(), "{d}");
        }
        for d in [IVec3::new(-1, 0, -1), IVec3::new(-1, 0, 0), IVec3::new(0, 0, -1), IVec3::new(-1, 0, 1)] {
            assert_eq!(w.get_block(P + d), Some(sapling), "{d}");
        }
    }
    let mut w = world(Dimension::Overworld);
    w.set_block(P - IVec3::Y, Block::DIRT);
    w.set_block(P, Block::OAK_SAPLING);
    assert!(w.apply_bone_meal(P), "a lone oak sapling consumes bone meal");
}

/// `SaplingBlock.advanceTree`: stage 0 becomes stage 1, and only a stage-1
/// sapling grows its tree. Bone meal (45% to succeed) and random ticks both
/// call it. Stage-1 states are appended at 2508..=2515 and behave as the plain
/// sapling for items, drops and soil.
#[test]
fn saplings_advance_through_stage_one_before_growing() {
    let mut w = world(Dimension::Overworld);
    for x in -1..=2 {
        for z in -1..=2 {
            w.set_block(P + IVec3::new(x, -1, z), Block::DIRT);
        }
    }
    let saplings = [
        Block::OAK_SAPLING,
        Block::SPRUCE_SAPLING,
        Block::BIRCH_SAPLING,
        Block::JUNGLE_SAPLING,
        Block::ACACIA_SAPLING,
        ob::CHERRY_SAPLING,
    ];
    for sapling in saplings {
        w.set_block(P, sapling);
        assert!(!w.advance_tree(P), "{} only advances its stage first", sapling.name());
        let staged = w.get_block(P).unwrap();
        assert_ne!(staged, sapling, "{}", sapling.name());
        assert!((2508..=2515).contains(&staged.0), "{staged:?}");
        assert_eq!((staged.name(), staged.base(), staged.is_sapling()), (sapling.name(), sapling, true));
        assert_eq!(staged.info().tex, sapling.info().tex);
        assert_eq!(staged.drop(), Some(sapling.into()), "{}", sapling.name());
        assert!(staged.can_stay_on(Block::DIRT) && !staged.can_stay_on(Block::STONE));
        // Bone meal on the stage-1 sapling eventually grows the tree.
        let mut grown = false;
        for _ in 0..200 {
            w.apply_bone_meal(P);
            if w.get_block(P).is_some_and(|b| !b.is_sapling()) {
                grown = true;
                break;
            }
        }
        assert!(grown, "{} never grew from stage 1", sapling.name());
        assert!(w.get_block(P).unwrap().is_log(), "{}", sapling.name());
        // Clear the tree for the next sapling.
        for dy in 0..30 {
            for dz in -6..=6 {
                for dx in -6..=6 {
                    w.set_block(P + IVec3::new(dx, dy, dz), Block::AIR);
                }
            }
        }
    }
    // Dark and pale oak need their 2x2 at either stage.
    for sapling in [ob::DARK_OAK_SAPLING, ob::PALE_OAK_SAPLING] {
        for d in [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
            w.set_block(P + d, sapling);
        }
        assert!(!w.advance_tree(P));
        assert_eq!(w.get_block(P).unwrap().base(), sapling);
        assert_ne!(w.get_block(P), Some(sapling), "the corner advanced to stage 1");
        assert!(w.advance_tree(P), "mixed stages still count as one sapling kind");
        for d in [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
            assert!(w.get_block(P + d).unwrap().is_log(), "{d}");
        }
        for dy in 0..30 {
            for dz in -8..=8 {
                for dx in -8..=8 {
                    w.set_block(P + IVec3::new(dx, dy, dz), Block::AIR);
                }
            }
        }
    }
}
