use super::*;
use crate::{
    enchant::{Enchantment, Enchants},
    entity::{Target, bee},
    inventory::{StackName, stack_from_str, stack_to_string},
    item::{Item, Tier, ToolKind},
    world::{
        chunk::ChunkData,
        terrain::{Biome, Dimension, Generator},
    },
};
use std::sync::Arc;
const P: IVec3 = IVec3::new(12, 145, 12);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    for y in 4..6 {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, P.as_dvec3() + glam::DVec3::new(5., 0., 0.), true)],
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        daylight: 1.,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn occupant(nectar: bool, ticks: u32) -> Occupant {
    let mut m = crate::entity::Mob::new(MobKind::Bee, P.as_dvec3(), 0.);
    m.age = -2000;
    m.baby = true;
    m.health = 8.;
    m.name = StackName::new("Buzzy").unwrap();
    m.bee.as_mut().unwrap().nectar = nectar;
    m.bee.as_mut().unwrap().flower = Some([4, 145, 4]);
    Occupant { ticks, ..Occupant::from_mob(&m) }
}

#[test]
fn registry_states_food_and_recipes_are_reference_values() {
    for id in FIRST..LAST {
        let b = Block(id);
        let (nest, f, honey) = hive(b).unwrap();
        assert_eq!(block(nest, f, honey), b);
        assert_eq!(b.base(), if nest { NEST } else { HIVE });
        assert_eq!(b.with_facing(Facing::East), block(nest, Facing::East, honey));
        assert_eq!(b.hardness(), if nest { 0.3 } else { 0.6 });
        assert!(b.is_solid());
    }
    assert_eq!(MobKind::ALL.last(), Some(&MobKind::Bee));
    assert_eq!(MobKind::from_name("minecraft:bee"), Some(MobKind::Bee));
    assert_eq!(MobKind::Bee.max_health(), 10.);
    assert_eq!(MobKind::Bee.shape().half_width, 0.35);
    assert!(MobKind::Bee.is_breedable());
    assert!(!MobKind::Bee.spawns_in(Dimension::Overworld));
    let mut g = crate::crafting::Grid::new(3);
    for i in [0, 1, 2, 6, 7, 8] {
        g.cells[i] = Some(Stack::new(Block::PLANKS, 1));
    }
    for i in [3, 4, 5] {
        g.cells[i] = Some(Stack::new(Item::HONEYCOMB, 1));
    }
    assert_eq!(g.result(), Some(Stack::new(HIVE, 1)));
}
#[test]
fn occupants_wait_for_day_dry_weather_open_exit_and_minimum_stay() {
    let mut w = world();
    let mut e = Entities::new(4);
    w.set_block(P, HIVE);
    assert!(w.bee_enter(P, occupant(false, 599)));
    let mut c = ctx();
    w.tick_beehives(&mut e, 0.05, &c);
    assert!(e.mobs.is_empty(), "strictly more than 600 ticks");
    c.raining = true;
    w.tick_beehives(&mut e, 2., &c);
    assert!(e.mobs.is_empty());
    c.raining = false;
    c.daylight = 0.;
    w.tick_beehives(&mut e, 2., &c);
    assert!(e.mobs.is_empty());
    c.daylight = 1.;
    w.set_block(P + IVec3::Z, Block::STONE);
    w.tick_beehives(&mut e, 2., &c);
    assert!(e.mobs.is_empty());
    w.set_block(P + IVec3::Z, Block::AIR);
    w.tick_beehives(&mut e, 0.05, &c);
    assert_eq!(e.mobs.len(), 1);
    assert_eq!(e.mobs[0].name.as_str(), Some("Buzzy"));
    assert!(e.mobs[0].age > -2000);
    assert!(!e.mobs[0].bee.unwrap().nectar);
}
#[test]
fn nectar_delivery_fills_honey_after_2400_ticks_without_duplication() {
    let mut w = world();
    let mut e = Entities::new(8);
    w.set_block(P, HIVE);
    w.bee_enter(P, occupant(true, 2399));
    w.tick_beehives(&mut e, 0.05, &ctx());
    assert!(e.mobs.is_empty());
    w.tick_beehives(&mut e, 0.05, &ctx());
    assert_eq!(e.mobs.len(), 1);
    assert!((1..=2).contains(&hive(w.get_block(P).unwrap()).unwrap().2));
    assert!(!e.mobs[0].bee.unwrap().nectar);
    for _ in 0..20 {
        w.tick_beehives(&mut e, 0.05, &ctx());
    }
    assert_eq!(e.mobs.len(), 1);
}
#[test]
fn saved_occupants_and_silk_item_round_trip_and_replace() {
    let mut w = world();
    let mut e = Entities::new(3);
    w.set_block(P, block(true, Facing::West, 5));
    w.bee_enter(P, occupant(true, 123));
    let text = w.beehives_to_string();
    let mut loaded = world();
    loaded.set_block(P, block(true, Facing::West, 5));
    loaded.load_beehives(&text);
    assert_eq!(w.beehive(P), loaded.beehive(P));
    let tool = Stack {
        enchants: Enchants::NONE.with(Enchantment::SilkTouch, 1),
        ..Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1)
    };
    assert!(loaded.mine_beehive(&mut e, P, loaded.get_block(P).unwrap(), Some(tool), PlayerId::HOST, true));
    assert!(e.mobs.is_empty());
    let s = loaded.drops[0].1;
    assert_eq!(s.item, Item::from_block(NEST));
    assert_eq!(s.hive.unwrap().honey, 5);
    let decoded = stack_from_str(&stack_to_string(Some(s))).unwrap().unwrap();
    assert_eq!(decoded, s);
    loaded.set_block(P, Block::AIR);
    loaded.set_block(P, NEST);
    loaded.restore_hive_item(P, decoded.hive);
    assert_eq!(hive(loaded.get_block(P).unwrap()).unwrap().2, 5);
    assert_eq!(loaded.beehive(P).unwrap().bees[0].as_ref(), Some(&occupant(true, 123)));
    assert_eq!(stack_from_str("283:3"), Some(Some(Stack::new(Item::MELON_SLICE, 3))));
}
#[test]
fn smoke_prevents_harvest_anger_but_unsmoked_harvest_releases_bees() {
    let mut w = world();
    let mut e = Entities::new(3);
    w.set_block(P, block(false, Facing::South, 5));
    w.bee_enter(P, occupant(false, 0));
    let c =
        super::super::utilities::Campfire { soul: false, facing: Facing::South, lit: true, wet: false, signal: false };
    w.set_block(P - IVec3::Y * 3, super::super::utilities::campfire(c));
    assert!(w.hive_smoked(P));
    assert!(w.harvest_beehive(&mut e, P, PlayerId::HOST));
    assert!(e.mobs.is_empty());
    assert!(w.beehive(P).unwrap().bees[0].is_some());
    w.set_block(P - IVec3::Y * 3, Block::AIR);
    w.set_block(P, block(false, Facing::South, 5));
    assert!(w.harvest_beehive(&mut e, P, PlayerId::HOST));
    assert_eq!(e.mobs.len(), 1);
    assert!(e.mobs[0].bee.unwrap().anger >= 400);
    assert_eq!(hive(w.get_block(P).unwrap()).unwrap().2, 0);
}
#[test]
fn bees_pollinate_and_apply_bounded_crop_boosts() {
    let mut w = world();
    let p = P + IVec3::X * 3;
    w.set_block(p, Block::DANDELION);
    let mut e = Entities::new(2);
    e.spawn(MobKind::Bee, p.as_dvec3() + glam::DVec3::new(0.5, 0.6, 0.5));
    let m = &mut e.mobs[0];
    m.bee.as_mut().unwrap().flower = Some(p.to_array());
    let mut events = Vec::new();
    let mut rng = crate::entity::Rng::new(4);
    for _ in 0..430 {
        bee::steer(&mut e.mobs[0], 0.05, &w, &ctx(), &mut rng, &mut events);
        if e.mobs[0].bee.unwrap().nectar {
            break;
        }
    }
    assert!(e.mobs[0].bee.unwrap().nectar);
    let b = Block::crop(super::super::block::Crop::Beetroot, 2);
    w.set_block(P - IVec3::Y, Block::WET_FARMLAND);
    w.set_block(P, b);
    assert!(w.bee_grow_crop(P));
    assert_eq!(w.get_block(P).unwrap().crop_stage(), Some(3));
    assert!(!w.bee_grow_crop(P));
}
#[test]
fn sting_requires_success_and_stung_bees_eventually_die() {
    let mut e = Entities::new(3);
    e.spawn(MobKind::Bee, P.as_dvec3());
    let id = e.mobs[0].uid;
    e.confirm_bee_sting(id, false);
    assert!(!e.mobs[0].bee.unwrap().stung);
    e.confirm_bee_sting(id, true);
    assert!(e.mobs[0].bee.unwrap().stung);
    let w = world();
    let mut rng = crate::entity::Rng::new(2);
    let mut ev = vec![];
    for _ in 0..1200 {
        bee::upkeep(&mut e.mobs[0], 0.05, &mut rng, &mut ev);
        bee::steer(&mut e.mobs[0], 0.05, &w, &ctx(), &mut rng, &mut ev);
        if e.mobs[0].health <= 0. {
            break;
        }
    }
    assert!(e.mobs[0].health <= 0.);
    assert!(!e.mobs[0].alive());
}
#[test]
fn tree_decorator_meadow_is_guaranteed_and_generated_nests_have_two_or_three_bees() {
    let mut tree = vec![];
    for y in 140..146 {
        tree.push((IVec3::new(12, y, 12), Block::BIRCH_LOG));
    }
    tree.push((IVec3::new(12, 145, 13), Block::BIRCH_LEAVES));
    for seed in 0..20 {
        let (p, b) = decorate_tree(Biome::Meadow, &tree, seed).unwrap();
        assert_eq!(hive(b), Some((true, Facing::South, 0)));
        assert_eq!(p.y, 144);
        let mut w = world();
        let mut data = ChunkData::Uniform(Block::AIR);
        let local = p - IVec3::new(0, 4, 0) * CHUNK_SIZE_I;
        data.set(local.x as usize, local.y as usize, local.z as usize, b);
        w.register_beehives(IVec3::new(0, 4, 0), &data, false);
        assert!((2..=3).contains(&w.beehive(p).unwrap().bees.iter().flatten().count()));
    }
}

#[test]
fn free_bees_save_state_and_breed_with_flowers_through_shared_goals() {
    let mut w = world();
    let mut e = Entities::new(6);
    e.spawn(MobKind::Bee, P.as_dvec3() + glam::DVec3::new(0.5, 1., 0.5));
    e.spawn(MobKind::Bee, P.as_dvec3() + glam::DVec3::new(1.1, 1., 0.5));
    for i in 0..2 {
        assert_eq!(e.use_animal(i, Some(Stack::new(Block::DANDELION, 1)), PlayerId::HOST), Some(true));
    }
    for _ in 0..100 {
        e.update_world(0.05, &mut w, &ctx(), crate::simulation::difficulty::Difficulty::Normal);
        if e.mobs.iter().any(|m| m.baby) {
            break;
        }
    }
    assert!(e.mobs.iter().any(|m| m.kind == MobKind::Bee && m.baby), "shared bee breeding produced no baby");
    let s = e.mobs[0].bee.as_mut().unwrap();
    s.nectar = true;
    s.cannot_enter = 123;
    s.anger = 567;
    s.target = Some(0);
    s.flower = Some([1, 2, 3]);
    s.hive = Some(P.to_array());
    let before = e.mobs[0].bee;
    let text = e.nether_mobs_to_string();
    let mut saved = Entities::new(1);
    saved.load_nether_mobs(&text);
    assert_eq!(saved.mobs[0].kind, MobKind::Bee);
    assert_eq!(saved.mobs[0].bee, before);
    let look = e.mobs[0].look();
    let mut replica = crate::entity::Mob::new(MobKind::Bee, P.as_dvec3(), 0.);
    replica.apply_look(&look);
    assert_eq!(replica.bee, before);
}

#[test]
fn smoke_collision_and_flower_adjacent_sapling_nests() {
    let mut w = world();
    w.set_block(P, HIVE);
    w.set_block(
        P - IVec3::Y * 3,
        crate::world::utilities::campfire(crate::world::utilities::Campfire {
            soul: false,
            lit: true,
            signal: false,
            wet: false,
            facing: Facing::South,
        }),
    );
    assert!(w.hive_smoked(P));
    w.set_block(P - IVec3::Y, Block::GLASS);
    assert!(!w.hive_smoked(P));
    w.set_block(P - IVec3::Y, Block::AIR);
    let mut nests = 0;
    for i in 0..100 {
        let mut w = world();
        for _ in 0..i {
            w.roll();
        }
        w.set_block(P - IVec3::Y, Block::DIRT);
        w.set_block(P, Block::BIRCH_SAPLING);
        w.set_block(P + IVec3::X, Block::POPPY);
        assert!(w.grow_tree(P));
        nests += w.beehives.hives.values().filter(|h| h.bees.iter().flatten().count() >= 2).count();
    }
    assert!(nests > 0, "flower-adjacent saplings can supply nests in survival");
}

#[test]
fn bees_rest_at_the_pinned_day_timeline_boundaries() {
    assert!(!rest_time(Dimension::Overworld, 12541));
    assert!(rest_time(Dimension::Overworld, 12542));
    assert!(rest_time(Dimension::Overworld, 23459));
    assert!(!rest_time(Dimension::Overworld, 23460));
    assert!(!rest_time(Dimension::Nether, 18000));
    let mut w = world();
    w.set_bee_clock(Dimension::Overworld, 18000);
    assert!(w.bee_night(1.), "the timeline, not visual daylight");
    w.set_bee_clock(Dimension::Overworld, 6000);
    assert!(!w.bee_night(0.));
}

#[test]
fn hive_block_and_occupants_survive_real_storage_and_chunk_unload() {
    use crate::world::storage::{LevelInfo, Storage};
    let mut w = world();
    let b = block(true, Facing::East, 4);
    w.set_block(P, b);
    w.bee_enter(P, occupant(true, 320));
    let mut props = std::collections::BTreeMap::new();
    props.insert("beehives".into(), w.beehives_to_string());
    let dir = std::env::temp_dir().join(format!("voxelcraft-bees-{}", std::process::id()));
    let store = Storage::new(&dir);
    store.save(&LevelInfo { seed: 7, player: None, props }, &w.modified_chunks()).unwrap();
    let mut loaded = world();
    for (p, data) in store.load_chunks().unwrap() {
        loaded.insert_chunk(p, data, true);
    }
    loaded.load_beehives(&store.load_level().unwrap().props["beehives"]);
    assert_eq!(loaded.get_block(P), Some(b));
    assert_eq!(loaded.beehive(P), w.beehive(P));
    loaded.unload_beehives(super::super::chunk::chunk_of(P));
    let mut e = Entities::new(7);
    loaded.tick_beehives(&mut e, 500., &ctx());
    assert!(e.mobs.is_empty());
    assert_eq!(loaded.beehive(P), w.beehive(P), "unloaded nests do not advance");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_flying_bee_completes_pollination_entry_and_honey_delivery() {
    let mut w = world();
    let mut e = Entities::new(42);
    let home = P + IVec3::X * 6;
    w.set_block(home, HIVE);
    w.set_block(P, Block::POPPY);
    e.spawn(MobKind::Bee, P.as_dvec3() + glam::DVec3::new(0.5, 0.6, 0.5));
    e.mobs[0].bee.as_mut().unwrap().hive = Some(home.to_array());
    e.mobs[0].bee.as_mut().unwrap().flower = Some(P.to_array());
    let start = std::time::Instant::now();
    let mut entered = false;
    for _ in 0..6500 {
        e.update_world(0.05, &mut w, &ctx(), crate::simulation::difficulty::Difficulty::Normal);
        entered |= w.beehive(home).unwrap().bees.iter().any(Option::is_some);
        if hive(w.get_block(home).unwrap()).unwrap().2 > 0 {
            break;
        }
    }
    eprintln!("bee pollination/delivery simulation: {:?}", start.elapsed());
    assert!(entered, "nectar-carrying bee must reach and enter its hive");
    assert!(hive(w.get_block(home).unwrap()).unwrap().2 > 0, "nectar becomes honey after its stay");
}

#[test]
fn bees_pending_emergency_release_survive_an_intervening_save() {
    let mut w = world();
    w.set_block(P, NEST);
    w.bee_enter(P, occupant(false, 0));
    w.set_block(P, Block::AIR);
    let mut loaded = world();
    loaded.load_beehives(&w.beehives_to_string());
    let mut e = Entities::new(2);
    loaded.tick_beehives(&mut e, 0.05, &ctx());
    assert_eq!(e.mobs.len(), 1);
    assert_eq!(e.mobs[0].name.as_str(), Some("Buzzy"));
    loaded.tick_beehives(&mut e, 0.05, &ctx());
    assert_eq!(e.mobs.len(), 1);
}

#[test]
fn empty_harvest_is_safe_but_smoke_does_not_protect_mining() {
    let mut w = world();
    let mut e = Entities::new(8);
    w.set_block(P, block(false, Facing::South, 5));
    e.spawn(MobKind::Bee, P.as_dvec3() + glam::DVec3::X * 2.);
    assert!(w.harvest_beehive(&mut e, P, PlayerId::HOST));
    assert_eq!(e.mobs[0].bee.unwrap().anger, 0, "empty hives do not alert bees on harvest");
    let fire =
        super::super::utilities::Campfire { soul: false, facing: Facing::South, lit: true, wet: false, signal: false };
    w.set_block(P - IVec3::Y * 3, super::super::utilities::campfire(fire));
    assert!(w.hive_smoked(P));
    assert!(w.mine_beehive(&mut e, P, HIVE, None, PlayerId::HOST, true));
    assert!(e.mobs[0].bee.unwrap().anger > 0, "pinned playerDestroy always alerts nearby bees");
}
