use super::*;
use crate::{
    crafting::Grid,
    inventory::Stack,
    world::{
        chunk::ChunkData,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};
use std::sync::Arc;
const P: IVec3 = IVec3::new(12, 145, 12);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w.set_block(P - IVec3::Y, Block::WET_FARMLAND);
    w.set_block(P + IVec3::X, Block::GLOWSTONE);
    w
}
#[test]
fn bone_meal_growth_ceiling_and_upper_target_match_java() {
    let mut w = world();
    w.set_block(P, TORCH_CROP);
    assert!(w.apply_bone_meal(P));
    assert_eq!(w.get_block(P), Some(Block(TORCH_CROP.0 + 1)));
    assert!(w.apply_bone_meal(P));
    assert_eq!(w.get_block(P), Some(TORCHFLOWER));
    assert!(!w.apply_bone_meal(P));
    w.set_block(P, pitcher(0, false));
    for age in 1..=2 {
        assert!(w.apply_bone_meal(P));
        assert_eq!(w.get_block(P), Some(pitcher(age, false)));
        assert_eq!(w.get_block(P + IVec3::Y), Some(Block::AIR));
    }
    w.set_block(P + IVec3::Y, Block::STONE);
    assert!(!w.apply_bone_meal(P));
    assert_eq!(w.get_block(P), Some(pitcher(2, false)));
    w.set_block(P + IVec3::Y, Block::AIR);
    assert!(w.apply_bone_meal(P));
    assert_eq!(w.get_block(P + IVec3::Y), Some(pitcher(3, true)));
    assert!(w.apply_bone_meal(P + IVec3::Y));
    assert_eq!(w.get_block(P), Some(pitcher(4, false)));
    assert_eq!(w.get_block(P + IVec3::Y), Some(pitcher(4, true)));
    assert!(!w.apply_bone_meal(P + IVec3::Y));
    w.drops.clear();
    w.set_block(P + IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(P), Some(Block::AIR));
    assert_eq!(w.drops, vec![(P, Stack::new(PITCHER_PLANT, 1))]);
}
#[test]
fn random_ticks_light_soil_loot_and_names() {
    let mut w = world();
    for seed in [TORCH_CROP, pitcher(0, false)] {
        w.set_block(P + IVec3::Y, Block::AIR);
        w.set_block(P, seed);
        for _ in 0..1000 {
            w.random_tick(P);
        }
        assert_eq!(w.get_block(P), Some(if seed == TORCH_CROP { TORCHFLOWER } else { pitcher(4, false) }));
        w.set_block(P - IVec3::Y, Block::STONE);
        assert_eq!(w.get_block(P), Some(Block::AIR));
        w.set_block(P - IVec3::Y, Block::WET_FARMLAND);
    }
    assert_eq!(Block::from_name("torchflower"), Some(TORCHFLOWER));
    assert_eq!(Block::from_name("pitcher_plant"), Some(PITCHER_PLANT));
    assert!(crate::entity::bee::flower(TORCHFLOWER));
    for age in 0..=4 {
        assert_eq!(pitcher(age, false).drop(), Some(if age == 4 { PITCHER_PLANT.into() } else { Item::PITCHER_POD }));
        assert_eq!(pitcher(age, true).drop(), None);
    }
    assert_eq!(Item::TORCHFLOWER_SEEDS.places(), Some(TORCH_CROP));
    assert_eq!(Item::PITCHER_POD.places(), Some(pitcher(0, false)));
}
#[test]
fn recipes_compost_pots_and_stew() {
    use crate::color::DyeColor;
    for (plant, dye, count) in [(TORCHFLOWER, DyeColor::Orange, 1), (PITCHER_PLANT, DyeColor::Cyan, 2)] {
        let mut g = Grid::new(2);
        g.cells[0] = Some(Stack::new(plant, 1));
        assert_eq!(g.result(), Some(Stack::new(dye.dye(), count)));
        assert_eq!(crate::content::compost::chance(plant.into()), 0.85);
    }
    for seed in [Item::TORCHFLOWER_SEEDS, Item::PITCHER_POD] {
        assert_eq!(crate::content::compost::chance(seed), 0.3);
        assert_eq!(Item::from_name(seed.name()), Some(seed));
    }
    assert_eq!(super::super::utilities::potted(TORCHFLOWER), Some(POTTED_TORCHFLOWER));
    assert_eq!(super::super::utilities::potted_plant(POTTED_TORCHFLOWER), Some(TORCHFLOWER));
    let mut g = Grid::new(2);
    for (i, item) in
        [Item::BOWL, Block::RED_MUSHROOM.into(), Block::RED_MUSHROOM.into(), TORCHFLOWER.into()].into_iter().enumerate()
    {
        g.cells[i] = Some(Stack::new(item, 1));
    }
    let stew = g.result().unwrap();
    assert_eq!(stew.item, Item::SUSPICIOUS_STEW);
    assert_eq!(stew.entity_data, 2);
}
#[test]
fn all_states_round_trip_chunk_storage() {
    let mut w = world();
    let at = |id| IVec3::new(2 + (id - FIRST) as i32, 148, 8);
    for id in FIRST..=super::super::sniffer_egg::LAST {
        w.edit_exact(at(id), Block(id), false);
        assert_ne!(Block(id).name(), "unknown");
    }
    let dir = std::env::temp_dir().join(format!("vc-sniffer-plants-{}", std::process::id()));
    let storage = Storage::new(&dir);
    storage.save(&LevelInfo { seed: 7, player: None, props: Default::default() }, &w.modified_chunks()).unwrap();
    let mut loaded = world();
    for (p, data) in storage.load_chunks().unwrap() {
        loaded.insert_chunk(p, data, true);
    }
    for id in FIRST..=super::super::sniffer_egg::LAST {
        assert_eq!(loaded.get_block(at(id)), Some(Block(id)));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pitcher_collision_and_potted_drops_use_shared_paths() {
    let mut w = world();
    w.set_block(P, pitcher(0, false));
    assert_eq!(crate::physics::collision_top(&w, P), Some(3. / 16.));
    w.set_block(P, pitcher(2, false));
    assert_eq!(crate::physics::collision_top(&w, P), Some(5. / 16.));
    w.drops.clear();
    w.spill_block(P, POTTED_TORCHFLOWER);
    assert_eq!(w.drops, vec![(P, Stack::new(super::super::utilities::FLOWER_POT, 1)), (P, Stack::new(TORCHFLOWER, 1))]);
}

#[test]
fn pitcher_growth_requires_upper_cell_inside_build_height_at_every_age() {
    let mut w = world();
    let p = IVec3::new(12, w.max_y() - 1, 12);
    w.insert_chunk(IVec3::new(0, p.y.div_euclid(32), 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w.set_block(p - IVec3::Y, Block::WET_FARMLAND);
    w.set_block(p + IVec3::X, Block::GLOWSTONE);
    w.set_block(p, pitcher(0, false));
    assert!(!w.apply_bone_meal(p));
    assert_eq!(w.get_block(p), Some(pitcher(0, false)));
}
