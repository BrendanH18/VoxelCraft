use super::*;
use crate::item::Item;
use crate::world::block::tex;
use glam::IVec2;

/// Flat stone to y = 63 over 3 × 3 columns (lit sky), with farmland at
/// `P` on dirt-free ground and nothing near it.
fn world() -> World {
    crate::world::lighting::flat_test_world((-1..=1).flat_map(|x| (-1..=1).map(move |z| IVec2::new(x, z))), 63)
}
const P: IVec3 = IVec3::new(16, 63, 16);

#[test]
fn moisture_states_keep_the_old_ids_at_zero_and_seven() {
    assert_eq!((farmland(0), farmland(7)), (Block::FARMLAND, Block::WET_FARMLAND), "57 and 58 as before");
    assert_eq!((farmland(1).0, farmland(6).0), (13968, 13973), "Track 4's reserved 13968-14015");
    assert_eq!(farmland(9), Block::WET_FARMLAND);
    assert!(!owns(FIRST - 1) && !owns(LAST + 1) && !is_farmland(Block(LAST + 1)));
    for m in 0..=MAX_MOISTURE {
        let b = farmland(m);
        assert_eq!(moisture(b), Some(m));
        assert!(b.is_farmland());
        // Java draws `farmland_moist` only at 7.
        let top = if m == MAX_MOISTURE { tex::WET_FARMLAND } else { tex::FARMLAND };
        assert_eq!(b.info().tex[2], top, "moisture {m}");
        assert_eq!(b.kind(), RenderKind::Cutout);
        assert_eq!(b.drop(), Some(Item::from_block(Block::DIRT)));
        assert_eq!(b.hardness(), Block::FARMLAND.hardness());
        assert_eq!(b.height(), Block::FARMLAND.height(), "15/16 high");
        assert_eq!(b.light_opacity(), Block::FARMLAND.light_opacity());
        assert!(b.uses_shape_for_light_occlusion(), "its 15/16 shape occludes light, as 57's does");
        assert!(!crate::world::light_shape::full_face(b, 2), "no full top face");
        assert!(crate::world::light_shape::full_face(b, 3), "a full bottom face");
        assert_eq!(crate::session::interact::picked_item(b), Item::from_block(Block::DIRT));
        assert_eq!(crate::content::mining::reference_name(b.base()), "farmland");
        assert!(Block::wheat(0).can_stay_on(b));
        if (1..MAX_MOISTURE).contains(&m) {
            assert_eq!((b.name(), b.base()), ("farmland", Block::FARMLAND));
        }
    }
    assert_eq!(Block::from_name("farmland"), Some(Block::FARMLAND));
}

#[test]
fn every_moisture_survives_save_and_reload() {
    use crate::world::chunk::ChunkData;
    use crate::world::storage::{LevelInfo, Storage};
    let dir = std::env::temp_dir().join(format!("voxelcraft-farmland-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let mut chunk = ChunkData::Uniform(Block::STONE);
    for m in 0..=MAX_MOISTURE {
        chunk.set(m as usize, 5, 3, farmland(m));
    }
    let pos = IVec3::new(1, 2, -3);
    storage
        .save(&LevelInfo { seed: 1, player: None, props: Default::default() }, &[(pos, std::sync::Arc::new(chunk))])
        .unwrap();
    let loaded = storage.load_chunks().unwrap();
    for m in 0..=MAX_MOISTURE {
        assert_eq!(loaded[&pos].get(m as usize, 5, 3), farmland(m), "moisture {m}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn water_wets_to_seven_and_dry_farmland_loses_one_level_a_tick() {
    let mut w = world();
    w.set_block(P, farmland(0));
    // Water 4 across and one up counts (`pos.offset(-4, 0, -4)` to `(4, 1, 4)`).
    let water = P + IVec3::new(4, 1, -4);
    w.set_block(water, Block::WATER);
    w.random_tick(P);
    assert_eq!(w.get_block(P), Some(farmland(7)));
    w.set_block(water, Block::AIR);
    for m in (0..MAX_MOISTURE).rev() {
        w.random_tick(P);
        assert_eq!(w.get_block(P), Some(farmland(m)), "dries one level per random tick");
    }
    // At 0 with nothing growing above, the next tick turns it to dirt.
    w.random_tick(P);
    assert_eq!(w.get_block(P), Some(Block::DIRT));
}

#[test]
fn water_out_of_reach_does_not_wet_it_and_waterlogged_blocks_do() {
    let mut w = world();
    for (at, wet) in [
        (IVec3::new(5, 0, 0), false),
        (IVec3::new(0, 2, 0), false),
        (IVec3::new(0, -1, 1), false),
        (IVec3::new(-4, 0, 4), true),
    ] {
        w.set_block(P, farmland(3));
        w.set_block(P + at, Block::WATER);
        w.random_tick(P);
        assert_eq!(w.get_block(P), Some(farmland(if wet { 7 } else { 2 })), "water at {at}");
        w.set_block(P + at, if at.y < 0 { Block::STONE } else { Block::AIR });
    }
    let slab = Block::slab_of(Block::STONE).unwrap().with_water(true);
    assert!(slab.holds_water());
    w.set_block(P + IVec3::X, slab);
    w.set_block(P, farmland(1));
    w.random_tick(P);
    assert_eq!(w.get_block(P), Some(farmland(7)), "a waterlogged slab's water counts");
}

#[test]
fn rain_on_the_block_above_wets_it() {
    let mut w = world();
    w.set_block(P, farmland(2));
    // A rainy (not snowy, not dry) column, and rain.
    let line = Some(Box::new([i16::MAX - 1; crate::world::chunk::CHUNK_SIZE * crate::world::chunk::CHUNK_SIZE]));
    w.columns.get_mut(&IVec2::ZERO).unwrap().snow = line;
    w.raining = true;
    w.random_tick(P);
    assert_eq!(w.get_block(P), Some(farmland(7)));
    // Under a roof, rain doesn't reach it.
    w.set_block(P + IVec3::Y * 3, Block::STONE);
    w.random_tick(P);
    assert_eq!(w.get_block(P), Some(farmland(6)));
}

#[test]
fn crops_and_the_maintaining_blocks_keep_dry_farmland() {
    let mut w = world();
    for above in [
        Block::wheat(0),
        Block::crop(crate::world::block::Crop::Beetroot, 1),
        crate::world::crops::stem_block(crate::world::crops::Fruit::Melon, 3),
        crate::world::sniffer_plants::TORCHFLOWER,
        Block::gate(crate::world::block::Facing::North, false),
    ] {
        assert!(maintains_farmland(above), "{above:?}");
        w.set_block(P, farmland(0));
        w.set_block(P + IVec3::Y, above);
        assert_eq!(w.get_block(P + IVec3::Y), Some(above));
        for _ in 0..3 {
            w.random_tick(P);
        }
        assert_eq!(w.get_block(P), Some(farmland(0)), "{above:?} keeps it");
        w.set_block(P + IVec3::Y, Block::AIR);
    }
    assert!(!maintains_farmland(Block::OAK_SAPLING) && !maintains_farmland(Block::STONE));
}

#[test]
fn a_solid_block_above_turns_it_to_dirt_but_a_fence_gate_does_not() {
    let mut w = world();
    w.set_block(P, farmland(5));
    w.set_block(P + IVec3::Y, Block::gate(crate::world::block::Facing::East, false));
    assert_eq!(w.get_block(P), Some(farmland(5)), "fence gates maintain farmland");
    w.set_block(P + IVec3::Y, Block::TORCH);
    assert_eq!(w.get_block(P), Some(farmland(5)), "a torch isn't solid");
    w.take_block_pushes();
    w.set_block(P + IVec3::Y, Block::STONE);
    assert_eq!(w.get_block(P), Some(Block::DIRT));
    assert_eq!(w.take_block_pushes(), vec![(P, 15. / 16., 1.)], "what stood on it is lifted");
}

#[test]
fn moisture_above_zero_is_moist_for_crop_growth() {
    let java: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/growth.json")).unwrap();
    let speed = &java["crop_growth_speed"];
    let mut w = world();
    let crop = P + IVec3::Y;
    w.set_block(P, farmland(0));
    w.set_block(crop, Block::wheat(0));
    let base = speed["base"].as_f64().unwrap() as f32;
    assert_eq!(w.crop_growth_speed(crop, Block::wheat(0)), base + speed["soil"].as_f64().unwrap() as f32);
    for m in 1..=MAX_MOISTURE {
        w.set_block(P, farmland(m));
        assert_eq!(
            w.crop_growth_speed(crop, Block::wheat(0)),
            base + speed["moist_soil"].as_f64().unwrap() as f32,
            "moisture {m}"
        );
    }
    w.set_block(P + IVec3::X, farmland(1));
    let neighbour = speed["moist_soil"].as_f64().unwrap() as f32 / speed["neighbour_divisor"].as_f64().unwrap() as f32;
    assert_eq!(w.crop_growth_speed(crop, Block::wheat(0)), base + 3.0 + neighbour);
}

#[test]
fn trampling_needs_the_fall_and_a_big_enough_body() {
    let mut w = world();
    w.set_block(P, farmland(7));
    w.set_block(P + IVec3::Y, Block::wheat(5));
    // `nextFloat() < fallDistance - 0.5`: never at half a block.
    for _ in 0..50 {
        assert!(!w.trample_farmland(P, 0.5, 0.6, 1.8, true));
    }
    // A chicken (0.4 × 0.7: 0.112 cubic blocks) never tramples; nor does
    // a mob without `mob_griefing`.
    assert!(!w.trample_farmland(P, 5.0, 0.4, 0.7, true));
    assert!(!w.trample_farmland(P, 5.0, 0.6, 1.95, false));
    w.drops.clear();
    // Falling 1.5 blocks or more always passes the roll.
    assert!(w.trample_farmland(P, 1.5, 0.6, 1.8, true));
    assert_eq!(w.get_block(P), Some(Block::DIRT));
    assert_eq!(w.get_block(P + IVec3::Y), Some(Block::AIR), "the crop pops off");
    assert!(w.drops.iter().any(|&(_, s)| s.item == Item::WHEAT_SEEDS), "and drops");
    // A player landing on it from 3 blocks: the feet sit at its 15/16 top.
    w.set_block(P, farmland(0));
    let feet = P.as_dvec3() + glam::DVec3::new(0.5, 15. / 16., 0.5);
    assert!(w.player_landed(feet, crate::player::SHAPE, 3.0));
    assert_eq!(w.get_block(P), Some(Block::DIRT));
}

#[test]
fn trampling_is_a_chance_of_the_fall_less_half_a_block() {
    let mut w = world();
    let mut trampled = 0;
    for _ in 0..2000 {
        w.set_block(P, farmland(0));
        w.set_block(P + IVec3::Y, Block::wheat(0));
        trampled += w.trample_farmland(P, 0.75, 0.6, 1.8, true) as u32;
    }
    assert!((400..600).contains(&trampled), "about a quarter: {trampled}");
}
