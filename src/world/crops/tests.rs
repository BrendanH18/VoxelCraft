use super::*;
use crate::{
    crafting::Grid,
    enchant::{Enchantment, Enchants},
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
    w
}
#[test]
fn crop_registry_seeds_shapes_and_food_match_reference() {
    for id in FIRST..=LAST {
        let b = Block(id);
        assert_ne!(b.name(), "unknown");
        assert_eq!(b.kind(), RenderKind::Cross);
        assert!(!b.is_solid());
        assert_eq!(b.hardness(), 0.0);
        assert!(b.can_stay_on(Block::WET_FARMLAND));
        assert!(!b.can_stay_on(Block::DIRT));
        assert!(b.info().tex.iter().all(|&l| (t::BASE..t::END).contains(&l)));
        assert!(super::outline(b).is_some());
        assert!(super::super::shape::collision(b, |_| Block::AIR, Block::WET_FARMLAND).is_empty());
    }
    for fruit in [Fruit::Melon, Fruit::Pumpkin] {
        assert_eq!(fruit.seed().places(), Some(stem_block(fruit, 0)));
        for age in 0..8 {
            assert_eq!(stem(stem_block(fruit, age)), Some((fruit, age, None)));
        }
        for facing in Facing::ALL {
            assert_eq!(stem(attached(fruit, facing)), Some((fruit, 7, Some(facing))));
        }
    }
    assert_eq!(Item::BEETROOT_SEEDS.places(), Some(Block::crop(Crop::Beetroot, 0)));
    assert_eq!(Item::BEETROOT_SOUP.name(), "beetroot soup");
    assert_eq!(Item::BEETROOT_SOUP.max_stack(), 1);
    assert!(
        matches!(Item::BEETROOT_SOUP.info().kind,crate::item::ItemKind::Food {hunger:6,saturation} if saturation == 7.2)
    );
    assert_eq!(Item::BEETROOT_SOUP.remainder(), Some(Item::BOWL));
    assert_eq!(Item::from_name("beetroot_soup"), Some(Item::BEETROOT_SOUP));
}
#[test]
fn growth_speed_matches_java_soil_and_spacing_examples() {
    let mut w = world();
    let b = stem_block(Fruit::Melon, 0);
    w.set_block(P, b);
    w.set_block(P - IVec3::Y, Block::FARMLAND);
    assert_eq!(w.crop_growth_speed(P, b), 2.0);
    w.set_block(P - IVec3::Y, Block::WET_FARMLAND);
    assert_eq!(w.crop_growth_speed(P, b), 4.0);
    for dx in -1..=1 {
        for dz in -1..=1 {
            w.set_block(P + IVec3::new(dx, -1, dz), Block::WET_FARMLAND);
        }
    }
    assert_eq!(w.crop_growth_speed(P, b), 10.0);
    w.set_block(P + IVec3::X, stem_block(Fruit::Melon, 6));
    assert_eq!(w.crop_growth_speed(P, b), 10.0, "a straight row is not crowded");
    w.set_block(P + IVec3::Z, stem_block(Fruit::Pumpkin, 6));
    assert_eq!(w.crop_growth_speed(P, b), 10.0, "other species don't crowd");
    w.set_block(P + IVec3::Z, stem_block(Fruit::Melon, 2));
    assert_eq!(w.crop_growth_speed(P, b), 5.0);
    w.set_block(P + IVec3::Z, Block::AIR);
    w.set_block(P + IVec3::X + IVec3::Z, stem_block(Fruit::Melon, 4));
    assert_eq!(w.crop_growth_speed(P, b), 5.0, "diagonal halves once");
    w.set_block(P + IVec3::X + IVec3::Z, attached(Fruit::Melon, Facing::South));
    assert_eq!(w.crop_growth_speed(P, b), 10.0, "attached stems are a different block");
}
#[test]
fn stem_fruiting_attachment_and_support_follow_java() {
    for fruit in [Fruit::Melon, Fruit::Pumpkin] {
        let mut w = world();
        for dx in -1..=1 {
            for dz in -1..=1 {
                w.set_block(P + IVec3::new(dx, -1, dz), Block::DIRT);
            }
        }
        w.set_block(P - IVec3::Y, Block::WET_FARMLAND);
        w.set_block(P, stem_block(fruit, 0));
        // Raw brightness must be supplied in this single loaded chunk fixture.
        w.set_block(P + IVec3::Y, Block::GLOWSTONE);
        for _ in 0..2000 {
            w.random_tick(P);
        }
        let (_, age, Some(facing)) = stem(w.get_block(P).unwrap()).unwrap() else {
            panic!("no fruit");
        };
        assert_eq!(age, 7);
        let q = P + facing.offset();
        assert_eq!(w.get_block(q), Some(fruit.block()));
        for _ in 0..100 {
            w.random_tick(P);
        }
        assert_eq!(w.get_block(q), Some(fruit.block()));
        assert!(!w.apply_bone_meal(P));
        w.set_block(q, Block::AIR);
        assert_eq!(w.get_block(P), Some(stem_block(fruit, 7)));
        assert!(!w.apply_bone_meal(P), "age 7 unattached cannot be fertilised");
        w.set_block(P - IVec3::Y, Block::DIRT);
        assert_eq!(w.get_block(P), Some(Block::AIR), "lost farmland pops the stem");
    }
    use super::super::overworld_blocks as ob;
    for b in [Block::DIRT, Block::GRASS, Block::WET_FARMLAND, ob::ROOTED_DIRT, ob::MOSS_BLOCK, ob::MUD, ob::PODZOL] {
        assert!(supports_fruit(b));
    }
    for b in [Block::STONE, Block::SAND, Block::DIRT_PATH, ob::PACKED_MUD] {
        assert!(!supports_fruit(b));
    }
}
#[test]
fn beetroot_random_ticks_use_two_thirds_and_bone_meal_can_do_nothing() {
    let mut w = world();
    w.set_block(P - IVec3::Y, Block::WET_FARMLAND);
    w.set_block(P + IVec3::Y, Block::GLOWSTONE);
    let mut grows = 0;
    let mut gains = [0; 2];
    for _ in 0..12000 {
        let b = Block::crop(Crop::Beetroot, 0);
        w.edit(P, b, false);
        w.random_tick(P);
        grows += (w.get_block(P) != Some(b)) as i32;
        w.edit(P, b, false);
        assert!(w.apply_bone_meal(P));
        let age = w.get_block(P).unwrap().crop_stage().unwrap();
        assert!(age <= 1);
        gains[age as usize] += 1;
    }
    // Isolated wet soil: floor(25/4)+1=7, so 2/3 * 1/7.
    assert!((1000..1300).contains(&grows), "expected about 1143, got {grows}");
    assert!((2700..3300).contains(&gains[0]), "2/3/4/5 divided by 3 -> 25% zero: {gains:?}");
    w.set_block(P, Block::crop(Crop::Beetroot, 3));
    assert!(!w.apply_bone_meal(P));
}
#[test]
fn crop_loot_and_acquisition_match_java_pools() {
    let mut w = world();
    let fortune = Enchants::NONE.with(Enchantment::Fortune, 3);
    for age in 0..8 {
        let mut total = 0;
        for _ in 0..3000 {
            w.drops.clear();
            w.spill_mined(P, stem_block(Fruit::Melon, age), fortune);
            for (_, s) in &w.drops {
                assert_eq!(s.item, Item::MELON_SEEDS);
                assert!(s.count <= 3);
                total += s.count as i32;
            }
        }
        let expected = 3000.0 * 3.0 * (age as f32 + 1.0) / 15.0;
        assert!((total as f32 - expected).abs() < 180.0, "age {age}: {total}, expected {expected}");
    }
    for _ in 0..300 {
        w.drops.clear();
        w.spill_mined(P, Block::crop(Crop::Beetroot, 3), fortune);
        assert_eq!(w.drops[0].1, Stack::new(Item::BEETROOT, 1));
        assert_eq!(w.drops[1].1.item, Item::BEETROOT_SEEDS);
        assert!((1..=7).contains(&w.drops[1].1.count));
        w.drops.clear();
        w.spill_block(P, Block::MELON);
        assert_eq!(w.drops[0].1.item, Item::MELON_SLICE);
        assert_eq!(w.drops.len(), 1);
        assert!((3..=7).contains(&w.drops[0].1.count));
        w.drops.clear();
        w.spill_block(P, Block::TALL_GRASS);
        assert!(w.drops.iter().all(|(_, s)| s.item == Item::WHEAT_SEEDS));
    }
    w.drops.clear();
    w.spill_block(P, Block::crop(Crop::Beetroot, 2));
    assert_eq!(w.drops, vec![(P, Stack::new(Item::BEETROOT_SEEDS, 1))]);
    w.drops.clear();
    w.spill_mined(P, Block::MELON, Enchants::NONE.with(Enchantment::SilkTouch, 1));
    assert_eq!(w.drops, vec![(P, Stack::new(Block::MELON, 1))]);
    for name in ["melon_seeds", "pumpkin_seeds", "beetroot_seeds", "beetroot_soup"] {
        assert!(super::super::loot::java_item(name).is_some(), "existing chest table item {name} must resolve");
    }
}
#[test]
fn crop_recipes_match_java() {
    let mut g = Grid::new(2);
    g.cells[3] = Some(Stack::new(Item::MELON_SLICE, 1));
    assert_eq!(g.result(), Some(Stack::new(Item::MELON_SEEDS, 1)));
    g.cells[3] = Some(Stack::new(Block::PUMPKIN, 1));
    assert_eq!(g.result(), Some(Stack::new(Item::PUMPKIN_SEEDS, 4)));
    let mut g = Grid::new(3);
    for i in 0..6 {
        g.cells[i] = Some(Stack::new(Item::BEETROOT, 1));
    }
    g.cells[8] = Some(Stack::new(Item::BOWL, 1));
    assert_eq!(g.result(), Some(Stack::new(Item::BEETROOT_SOUP, 1)));
    g.cells[5] = None;
    assert_eq!(g.result(), None);
}
#[test]
fn every_crop_state_survives_chunk_storage_round_trip() {
    let mut w = world();
    let at = |id| IVec3::new(2 + (id - FIRST) as i32, 145, 8);
    for id in FIRST..=LAST {
        w.edit_exact(at(id), Block(id), false);
    }
    let dir = std::env::temp_dir().join(format!("voxelcraft-crops-{}", std::process::id()));
    let storage = Storage::new(&dir);
    storage.save(&LevelInfo { seed: 7, player: None, props: Default::default() }, &w.modified_chunks()).unwrap();
    let mut loaded = world();
    for (pos, data) in storage.load_chunks().unwrap() {
        loaded.insert_chunk(pos, data, true);
    }
    for id in FIRST..=LAST {
        assert_eq!(loaded.get_block(at(id)), Some(Block(id)));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
