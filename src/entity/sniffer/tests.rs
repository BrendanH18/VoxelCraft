use super::*;
use crate::{
    entity::PlayerId,
    world::{chunk::ChunkData, terrain::Generator},
};
use std::sync::Arc;
const P: IVec3 = IVec3::new(12, 145, 12);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w.set_block(P - IVec3::Y, Block::DIRT);
    w
}
fn ctx() -> Ctx {
    Ctx {
        players: Vec::new(),
        daylight: 1.,
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
#[test]
fn attributes_food_egg_breeding_and_wire_reservation() {
    assert_eq!(MobKind::Sniffer.max_health(), 14.);
    assert_eq!(MobKind::Sniffer.lan_index(), 71);
    assert_eq!(MobKind::from_lan_index(71), Some(MobKind::Sniffer));
    assert_eq!(MobKind::from_name("sniffer"), Some(MobKind::Sniffer));
    assert!(!MobKind::Sniffer.spawns_in(Dimension::Overworld));
    assert!(MobKind::Sniffer.loot().is_empty());
    for seed in [Item::TORCHFLOWER_SEEDS, Item::PITCHER_POD] {
        assert!(MobKind::Chicken.breeding_food(seed));
        assert!(super::super::animals::seeds(seed));
    }
    let mut e = Entities::new(7);
    e.spawn(MobKind::Sniffer, P.as_dvec3());
    e.spawn(MobKind::Sniffer, P.as_dvec3() + DVec3::X);
    for i in 0..2 {
        assert_eq!(e.use_animal(i, Some(Stack::new(Item::TORCHFLOWER_SEEDS, 1)), PlayerId::HOST), Some(true));
    }
    e.ensure_mob_ids();
    let uid = e.mobs[1].uid;
    e.mobs[0].animal.as_mut().unwrap().breed_now = Some(uid);
    e.resolve_breeding();
    assert_eq!(e.mobs.len(), 2);
    assert_eq!(e.items.len(), 1);
    assert_eq!(e.items[0].stack.item, Item::SNIFFER_EGG);
    assert!(e.mobs.iter().all(|m| m.age == 6000));
}
#[test]
fn digging_drops_once_persists_and_remembers_dimension() {
    let mut w = world();
    let mut e = Entities::new(7);
    e.spawn(MobKind::Sniffer, P.as_dvec3());
    e.mob_loot = false; // Gifts are not death loot in Java.
    let c = ctx();
    let s = e.mobs[0].sniffer.as_mut().unwrap();
    s.pose = DIGGING;
    s.elapsed = 119;
    s.duration = 160;
    s.target = Some(P - IVec3::Y);
    e.tick_sniffers(0.05, &mut w, &c);
    assert_eq!(e.items.len(), 1);
    assert!([Item::TORCHFLOWER_SEEDS, Item::PITCHER_POD].contains(&e.items[0].stack.item));
    let saved = super::super::nether::save_mob(&e.mobs[0]);
    e.mobs[0] = super::super::nether::load_mob(&saved).unwrap();
    assert!(e.mobs[0].sniffer.as_ref().unwrap().dropped);
    for _ in 0..100 {
        e.tick_sniffers(0.05, &mut w, &c);
    }
    assert_eq!(e.items.len(), 1);
    let s = e.mobs[0].sniffer.as_ref().unwrap();
    assert_eq!(s.explored, vec![(0, P - IVec3::Y)]);
    assert!(s.cooldown > 9500);
    let state = s.save();
    let mut loaded = State::default();
    loaded.load(&state);
    assert_eq!(loaded.explored, s.explored);
    assert!(!valid(&w, &loaded, P - IVec3::Y, 0));
    assert!(valid(&w, &loaded, P - IVec3::Y, 1));
}
#[test]
fn egg_hatching_boost_timing_scheduled_save_and_snifflet_age() {
    use crate::world::sniffer_egg as egg;
    let mut w = world();
    let mut e = Entities::new(7);
    w.set_block(P - IVec3::Y, crate::world::overworld_blocks::MOSS_BLOCK);
    w.set_block(P, egg::EGG);
    let schedule = w.redstone_to_string();
    let entry = schedule.split('|').find(|s| s.starts_with("t,12,145,12,")).unwrap();
    let delay = entry.split(',').nth(4).unwrap().parse::<u64>().unwrap();
    assert!((4000..4300).contains(&delay));
    for hatch in 0..3 {
        w.load_redstone(&format!("v1,0,1|t,12,145,12,1,0,0,{}", 15120 + hatch));
        w.tick_redstone();
    }
    e.tick_sniffers(0.05, &mut w, &ctx());
    assert_eq!(e.mobs.len(), 1);
    assert!(e.mobs[0].baby);
    assert_eq!(e.mobs[0].age, -48000);
    for hatch in 0..=2 {
        assert_eq!(Block(egg::EGG.0 + hatch).drop(), Some(Item::SNIFFER_EGG));
    }
}
#[test]
fn independent_reference_data_and_look_round_trip() {
    let facts: Value = serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/sniffers.json")).unwrap();
    assert_eq!(MobKind::Sniffer.max_health(), facts["constants"]["sniffer_health"].as_f64().unwrap() as f32);
    for (name, count) in [("torchflower_crop", 2), ("pitcher_crop", 10), ("pitcher_plant", 2), ("sniffer_egg", 3)] {
        assert_eq!(facts["blockstate_keys"][name].as_array().unwrap().len(), count);
    }
    let mut m = Mob::new(MobKind::Sniffer, DVec3::ZERO, 0.);
    let s = m.sniffer.as_mut().unwrap();
    s.pose = DIGGING;
    s.elapsed = 123;
    assert!((m.shape().height - 1.35).abs() < 1e-6);
    let mut copy = Mob::new(MobKind::Sniffer, DVec3::ZERO, 0.);
    copy.apply_look(&m.look());
    assert_eq!(copy.sniffer.as_ref().unwrap().pose, DIGGING);
    assert_eq!(copy.sniffer.as_ref().unwrap().elapsed, 123);
    assert!(!diggable(crate::world::overworld_blocks::MYCELIUM));
    assert!(!diggable(Block::SAND));
    assert!(!diggable(Block::FARMLAND));
}
#[test]
fn saved_scheduled_cracking_resumes_and_replacement_cancels_hatch() {
    use crate::world::sniffer_egg as egg;
    let mut w = world();
    w.set_block(P, egg::EGG);
    let pending = w.redstone_to_string();
    let entry = pending.split('|').find(|s| s.starts_with("t,12,145,12,")).unwrap();
    let delay = entry.split(',').nth(4).unwrap().parse::<u64>().unwrap();
    assert!((8000..8300).contains(&delay));
    w.load_redstone("v1,0,1|t,12,145,12,2,0,0,15120");
    w.tick_redstone();
    let pending = w.redstone_to_string();
    let mut loaded = world();
    loaded.set_block(P, egg::EGG);
    loaded.load_redstone(&pending);
    loaded.tick_redstone();
    assert_eq!(loaded.get_block(P), Some(Block(15121)));
    assert!(loaded.redstone_to_string().contains("t,12,145,12,"));
    loaded.set_block(P, Block::STONE);
    loaded.load_redstone("v1,0,1|t,12,145,12,1,0,0,15120");
    loaded.tick_redstone();
    assert!(loaded.hatched_sniffers.is_empty());
    assert_eq!(loaded.get_block(P), Some(Block::STONE));
}
