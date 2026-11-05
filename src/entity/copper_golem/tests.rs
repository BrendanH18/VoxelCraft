use super::*;
#[test]
fn components_survive_partial_transfer() {
    let mut a = Stack::new(crate::item::Item::COPPER_INGOT, 16);
    a.entity_data = 77;
    let mut slots = [Some(Stack { count: 60, ..a }), Some(Stack::new(Block::STONE, 64))];
    let mut hand = Some(a);
    assert!(transfer(&mut slots, &mut hand));
    assert_eq!(slots[0].unwrap().count, 64);
    assert_eq!(hand.unwrap(), Stack { count: 12, ..a });
}
use crate::{
    entity::{Ctx, PlayerId, Target as PlayerTarget},
    item::Item,
    simulation::{difficulty::Difficulty, lighting::SpawnLight},
    world::{
        block::Facing,
        terrain::{Dimension, Generator},
    },
};
use std::sync::Arc;
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(91)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.load_test_column(glam::IVec2::new(x, z), 63);
        }
    }
    w
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![PlayerTarget::new(PlayerId::HOST, DVec3::new(8., 64., 8.), false)],
        daylight: 1.,
        spawn_light: SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn chest(w: &mut World, p: IVec3, kind: double_chest::Kind) {
    w.set_block(
        p,
        double_chest::block(double_chest::ChestState {
            kind,
            facing: Facing::North,
            ty: double_chest::ChestType::Single,
            wet: false,
        }),
    );
}
fn golem(e: &mut Entities, pos: DVec3) {
    e.spawn_copper_golem(pos, Stage::Unaffected, false);
    let m = e.mobs.last_mut().unwrap();
    assert!(m.persistent && m.uid != 0);
    m.on_ground = true;
    let s = m.copper_golem.as_mut().unwrap();
    s.initialized = true;
    s.cooldown = 0;
    s.spin = 200;
}
fn step(e: &mut Entities, w: &mut World, n: usize) {
    for _ in 0..n {
        e.clock_time += 1;
        w.maps.game_ticks += 1;
        e.update_world(0.05, w, &ctx(), Difficulty::Normal);
    }
}
fn cargo(e: &Entities) -> Option<Stack> {
    e.mobs.iter().find(|m| m.kind == MobKind::CopperGolem).unwrap().copper_golem.as_ref().unwrap().cargo
}
#[test]
fn legacy_indices_attributes_and_no_natural_spawn() {
    assert_eq!(MobKind::ALL[22], MobKind::IronGolem);
    assert_eq!(MobKind::ALL[23], MobKind::SnowGolem);
    assert_eq!(MobKind::ALL[67], MobKind::Ocelot);
    assert_eq!(MobKind::ALL[68], MobKind::CopperGolem);
    let m = Mob::new(MobKind::CopperGolem, DVec3::ZERO, 0.);
    assert_eq!(m.health, 12.);
    assert!(m.persistent && m.kind.leashable());
    assert!(!m.kind.is_hostile());
    assert_eq!(m.kind.category(), super::super::category::MobCategory::Misc);
    for d in [Dimension::Overworld, Dimension::Nether, Dimension::End] {
        assert!(!m.kind.spawns_in(d));
    }
    assert_eq!(m.kind.xp(&mut Rng::new(3)), 0);
    assert_eq!(m.kind.nav_caps(0.49, 0.98).max_path_length, 48.);
}
#[test]
fn simulation_moves_sixteen_from_copper_to_empty_then_matching_chest() {
    let mut w = world();
    let mut e = Entities::new(9);
    let source = IVec3::new(2, 64, 2);
    let dest = IVec3::new(7, 64, 2);
    chest(&mut w, source, double_chest::Kind::Copper(Stage::Weathered));
    chest(&mut w, dest, double_chest::Kind::Plain);
    let mut original = Stack::new(Item::COPPER_INGOT, 32);
    original.name = crate::inventory::StackName::new("cargo").unwrap();
    original.entity_data = 42;
    original.repair_cost = 7;
    original.damage = 12;
    original.enchants = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Unbreaking, 3);
    original.lodestone =
        Some(crate::inventory::Lodestone { target: Some((IVec3::new(91, 11, -5), Dimension::Nether)) });
    original.map_id = Some(77);
    w.set_chest_slot(source, 0, Some(original));
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    step(&mut e, &mut w, 61);
    assert_eq!(w.chest_slot(source, 0).unwrap().count, 16);
    assert_eq!(cargo(&e), Some(Stack { count: 16, ..original }));
    step(&mut e, &mut w, 1400);
    assert_eq!(w.chest_slot(dest, 0), Some(original));
    assert!(cargo(&e).is_none());
    assert!(w.chest_slot(source, 0).is_none());
    assert!(e.items.is_empty());
}
#[test]
fn queue_and_obstruction_prevent_opening_then_retry_without_loss() {
    let mut w = world();
    let mut e = Entities::new(4);
    let p = IVec3::new(2, 64, 2);
    chest(&mut w, p, double_chest::Kind::Copper(Stage::Unaffected));
    w.set_chest_slot(p, 0, Some(Stack::new(Item::DIAMOND, 24)));
    w.set_chest_viewers([p]);
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    step(&mut e, &mut w, 100);
    assert!(cargo(&e).is_none());
    assert_eq!(w.chest_slot(p, 0).unwrap().count, 24);
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().queued);
    w.set_chest_viewers([]);
    w.set_block(p + IVec3::Y, Block::STONE);
    step(&mut e, &mut w, 100);
    assert!(cargo(&e).is_none());
    w.set_block(p + IVec3::Y, Block::AIR);
    step(&mut e, &mut w, 400);
    assert_eq!(cargo(&e).unwrap().count, 16);
    assert_eq!(w.chest_slot(p, 0).unwrap().count, 8);
}
#[test]
fn wrong_full_and_invalid_destinations_keep_cargo_and_components() {
    for kind in [double_chest::Kind::Plain, double_chest::Kind::Trapped, double_chest::Kind::Copper(Stage::Unaffected)]
    {
        let mut w = world();
        let mut e = Entities::new(5);
        let p = IVec3::new(2, 64, 2);
        chest(&mut w, p, kind);
        for n in 0..27 {
            w.set_chest_slot(p, n, Some(Stack::new(Block::STONE, 64)));
        }
        golem(&mut e, DVec3::new(1.5, 64., 2.5));
        let a = Stack::new(Item::DIAMOND, 16);
        e.mobs[0].copper_golem.as_mut().unwrap().cargo = Some(a);
        step(&mut e, &mut w, 300);
        assert_eq!(cargo(&e), Some(a));
        assert!(e.items.is_empty());
    }
    let a = Stack::new(Item::DIAMOND, 16);
    let mut slots = [Some(Stack::new(Item::DIAMOND, 64)); 27];
    let mut hand = Some(a);
    assert!(transfer(&mut slots, &mut hand));
    assert_eq!(hand, Some(a));
    let mut tagged = a;
    tagged.entity_data = 99;
    let mut slots = [Some(Stack { count: 1, ..a }), None];
    let mut hand = Some(tagged);
    assert!(transfer(&mut slots, &mut hand));
    assert_eq!(slots[0].unwrap().count, 1);
    assert_eq!(slots[1], Some(tagged));
}
#[test]
fn empty_source_search_visits_ten_then_cools_140_and_retains_hand() {
    let mut w = world();
    let mut e = Entities::new(2);
    let p = IVec3::new(2, 64, 2);
    chest(&mut w, p, double_chest::Kind::Copper(Stage::Unaffected));
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    step(&mut e, &mut w, 65);
    assert!(cargo(&e).is_none());
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().cooldown > 0);
    let mut s = State {
        cargo: Some(Stack::new(Item::DIAMOND, 4)),
        visited: (0..11).map(|x| IVec3::new(x, 64, 0)).collect(),
        ..State::default()
    };
    s.rest();
    assert_eq!(s.cooldown, 140);
    assert!(s.visited.is_empty());
    assert_eq!(s.cargo.unwrap().count, 4);
}
#[test]
fn double_chest_uses_right_half_first_and_queues_both_halves() {
    let mut w = world();
    let mut e = Entities::new(1);
    let p = IVec3::new(2, 64, 2);
    let left = double_chest::ChestState {
        kind: double_chest::Kind::WaxedCopper(Stage::Exposed),
        facing: Facing::North,
        ty: double_chest::ChestType::Left,
        wet: false,
    };
    w.set_block(p, double_chest::block(left));
    w.set_block(
        p + IVec3::X,
        double_chest::block(double_chest::ChestState { ty: double_chest::ChestType::Right, ..left }),
    );
    assert!(w.chest_partner(p).is_some());
    w.set_chest_slot(p, 27, Some(Stack::new(Item::DIAMOND, 18)));
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    golem(&mut e, DVec3::new(1.5, 64., 3.5));
    step(&mut e, &mut w, 61);
    assert!(e.mobs.iter().any(|m| m.copper_golem.as_ref().unwrap().cargo.is_some()));
    let total: u16 = e.mobs.iter().filter_map(|m| m.copper_golem.as_ref()?.cargo).map(|s| u16::from(s.count)).sum();
    assert_eq!(total + (0..54).filter_map(|n| w.chest_slot(p, n)).map(|s| u16::from(s.count)).sum::<u16>(), 18);
    assert!(
        e.mobs.iter().any(|m| m.copper_golem.as_ref().unwrap().queued || m.copper_golem.as_ref().unwrap().pose > 0)
    );
}
#[test]
fn weather_wax_scrape_lightning_and_readiness_do_not_discard_state() {
    let mut s = State::default();
    let mut r = Rng::new(7);
    s.weather(100, true, &mut r);
    assert!((504100..=552100).contains(&s.next_weather_age));
    s.weather(s.next_weather_age, true, &mut r);
    assert_eq!(s.stage, Stage::Exposed);
    assert!(s.wax());
    let deadline = s.next_weather_age;
    s.weather(i64::MAX, true, &mut r);
    assert_eq!(s.next_weather_age, deadline);
    assert!(s.scrape());
    assert_eq!(s.stage, Stage::Exposed);
    assert!(s.scrape());
    assert_eq!(s.stage, Stage::Unaffected);
    assert!(!s.scrape());
    s.stage = Stage::Oxidized;
    s.wax();
    s.lightning(99);
    assert_eq!(s.stage, Stage::Weathered);
    assert!(!s.waxed());
    s.lightning(99);
    assert_eq!(s.stage, Stage::Weathered);
    s.stage = Stage::Oxidized;
    s.next_weather_age = 0;
    s.cargo = Some(Stack::new(Item::DIAMOND, 7));
    for _ in 0..10000 {
        s.weather(50, false, &mut r);
    }
    assert!(!s.statue_ready);
    for _ in 0..10000 {
        s.weather(50, true, &mut r);
    }
    assert!(s.statue_ready && s.cargo.is_some());
    let flower = Stack::new(Block::POPPY, 1);
    assert_eq!(s.equip_antenna(flower), Some(None));
    assert_eq!(s.equip_antenna(flower), None);
    assert_eq!(s.shear(), Some(flower));
}
#[test]
fn save_replica_and_unload_keep_full_cargo_deadline_equipment_and_identity() {
    let mut w = world();
    let mut e = Entities::new(4);
    golem(&mut e, DVec3::new(2.5, 64., 2.5));
    let mut stack = Stack::new(Item::DIAMOND, 12);
    stack.name = crate::inventory::StackName::new("precious").unwrap();
    stack.entity_data = 88;
    stack.repair_cost = 13;
    let m = &mut e.mobs[0];
    m.name = crate::inventory::StackName::new("Sorter").unwrap();
    let s = m.copper_golem.as_mut().unwrap();
    s.stage = Stage::Weathered;
    s.next_weather_age = 873221;
    s.cargo = Some(stack);
    s.antenna = Some(Stack::new(Block::POPPY, 1));
    s.pose = 3;
    s.interaction_ticks = 29;
    let mut replica = Mob::new(MobKind::CopperGolem, DVec3::ZERO, 0.);
    replica.apply_look(&m.look());
    let s = replica.copper_golem.as_ref().unwrap();
    assert_eq!((s.cargo, s.stage, s.pose, s.interaction_ticks), (Some(stack), Stage::Weathered, 3, 29));
    let saved = e.nether_mobs_to_string();
    let mut back = Entities::new(8);
    back.load_nether_mobs(&saved);
    assert_eq!(
        back.nether_mobs_to_string(),
        saved.replace("\"pose\":3", "\"pose\":0").replace("\"interaction_ticks\":29", "\"interaction_ticks\":0")
    );
    e.sync_chunk_entities(&mut w);
    w.unload_test_column(glam::IVec2::ZERO);
    e.sync_chunk_entities(&mut w);
    assert!(e.mobs.is_empty());
    assert_eq!(e.stored_entities(), 1);
    assert!(e.nether_mobs_to_string().contains("873221"));
    w.load_test_column(glam::IVec2::ZERO, 63);
    e.sync_chunk_entities(&mut w);
    assert_eq!(cargo(&e), Some(stack));
    assert_eq!(e.mobs[0].name.as_str(), Some("Sorter"));
}
#[test]
fn sorting_benchmark_many_golems_and_distant_containers() {
    let mut w = world();
    let mut e = Entities::new(31);
    for z in 0..8 {
        let p = IVec3::new(4, 64, z * 3);
        chest(&mut w, p, double_chest::Kind::Copper(Stage::Unaffected));
        w.set_chest_slot(p, 0, Some(Stack::new(Item::COPPER_INGOT, 64)));
        chest(&mut w, p + IVec3::X * 8, double_chest::Kind::Plain);
        golem(&mut e, DVec3::new(3.5, 64., f64::from(z * 3) + 0.5));
    }
    // Saved far-away chests exercise the spatial index without loading terrain.
    let empty = crate::world::chest::Chest::default().serialize();
    let distant = (0..4096).map(|n| format!("{},64,1000={}", 1000 + n, empty)).collect::<Vec<_>>().join("|");
    w.load_chests(&distant);
    let started = std::time::Instant::now();
    step(&mut e, &mut w, 800);
    let elapsed = started.elapsed();
    let delivered: u32 = (0..8)
        .flat_map(|z| {
            (0..27).filter_map({
                let w = &w;
                move |n| w.chest_slot(IVec3::new(12, 64, z * 3), n)
            })
        })
        .map(|s| u32::from(s.count))
        .sum();
    let carried: u32 = e.mobs.iter().filter_map(|m| m.copper_golem.as_ref()?.cargo).map(|s| u32::from(s.count)).sum();
    let source: u32 =
        (0..8).filter_map(|z| w.chest_slot(IVec3::new(4, 64, z * 3), 0)).map(|s| u32::from(s.count)).sum();
    assert_eq!(delivered + carried + source, 512);
    assert!(delivered > 0);
    println!(
        "copper sorting: 8 golems, 16 local + 4096 stored distant chests, 800 ticks: {elapsed:?}; delivered {delivered}, carried {carried}"
    );
}

#[test]
fn death_drops_cargo_before_save_and_exactly_once() {
    let mut w = world();
    let mut e = Entities::new(5);
    golem(&mut e, DVec3::new(2.5, 64., 2.5));
    let mut stack = Stack::new(Item::DIAMOND, 11);
    stack.entity_data = 456;
    stack.enchants = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::VanishingCurse, 1);
    e.mob_loot = false;
    e.mobs[0].copper_golem.as_mut().unwrap().cargo = Some(stack);
    e.mobs[0].copper_golem.as_mut().unwrap().antenna = Some(Stack::new(Block::POPPY, 1));
    golem(&mut e, DVec3::new(2.5, 64., 2.5));
    let second = Stack { count: 7, entity_data: 457, ..stack };
    e.mobs[1].copper_golem.as_mut().unwrap().cargo = Some(second);
    assert!(e.mobs[0].damage(20., None, &mut e.rng));
    assert!(e.mobs[1].damage(20., None, &mut e.rng));
    e.drop_loot(MobKind::CopperGolem, e.mobs[0].pos);
    e.drop_loot(MobKind::CopperGolem, e.mobs[1].pos);
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().cargo.is_none());
    assert!(e.items.iter().any(|i| i.stack == stack));
    let mut reload = Entities::new(9);
    reload.load_items(&e.items_to_string());
    assert!(reload.items.iter().any(|i| i.stack == stack));
    assert!(reload.items.iter().any(|i| i.stack == second));
    step(&mut e, &mut w, 30);
    assert_eq!(
        e.items.iter().filter(|i| i.stack.item == Item::DIAMOND).map(|i| u32::from(i.stack.count)).sum::<u32>(),
        18
    );
}

#[test]
fn lethal_projectiles_and_uncredited_blasts_preserve_equipment_in_immediate_saves() {
    for projectile in [true, false] {
        let mut w = world();
        let mut e = Entities::new(5);
        e.mob_loot = false;
        let pos = DVec3::new(2.5, 64., 2.5);
        golem(&mut e, pos);
        let cargo = Stack {
            entity_data: 456,
            name: crate::inventory::StackName::new("Preserved parcel").unwrap(),
            enchants: crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::VanishingCurse, 1),
            ..Stack::new(Item::DIAMOND, 11)
        };
        let antenna = Stack { entity_data: 789, ..Stack::new(Block::POPPY, 1) };
        let state = e.mobs[0].copper_golem.as_mut().unwrap();
        state.cargo = Some(cargo);
        state.antenna = Some(antenna);
        if projectile {
            e.mobs[0].health = 1.;
            e.arrows.push(super::super::Arrow::shot(pos + DVec3::new(-1., 0.5, 0.), DVec3::X, 1., false));
            let events = e.update_world(0.05, &mut w, &ctx(), Difficulty::Normal);
            assert!(events.iter().any(|event| matches!(
                event,
                super::super::EntityEvent::MobShot { kind: MobKind::CopperGolem, killed: true, .. }
            )));
        } else {
            e.explode(&w, pos, 3.);
        }
        assert!(!e.mobs[0].alive());
        let mut props = Default::default();
        e.save_props(&mut props);
        let mut reloaded = Entities::new(9);
        reloaded.load_props(&props);
        assert!(reloaded.mobs.is_empty(), "dead mobs are omitted from saves");
        assert_eq!(reloaded.items.len(), 2, "projectile={projectile}: equipment must already be dropped");
        for stack in [cargo, antenna] {
            assert_eq!(reloaded.items.iter().filter(|item| item.stack == stack).count(), 1);
        }
        assert!(reloaded.orbs.is_empty());
        step(&mut e, &mut w, 30);
        assert_eq!(e.items.len(), 2, "corpse cleanup must not duplicate either stack");
    }
}

#[test]
fn obstructed_and_removed_destination_keep_hand_then_retry_after_reopening() {
    let mut w = world();
    let mut e = Entities::new(91);
    let p = IVec3::new(6, 64, 2);
    chest(&mut w, p, double_chest::Kind::Plain);
    for side in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y] {
        w.set_block(p + side, Block::STONE);
    }
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    let hand = Stack::new(Item::DIAMOND, 16);
    e.mobs[0].copper_golem.as_mut().unwrap().cargo = Some(hand);
    step(&mut e, &mut w, 250);
    assert_eq!(cargo(&e), Some(hand));
    assert!(w.chest_slot(p, 0).is_none());
    w.set_block(p, Block::BARREL);
    step(&mut e, &mut w, 200);
    assert_eq!(cargo(&e), Some(hand));
    assert!(e.items.is_empty());
    chest(&mut w, p, double_chest::Kind::Plain);
    for side in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y] {
        w.set_block(p + side, Block::AIR);
    }
    step(&mut e, &mut w, 800);
    assert!(cargo(&e).is_none());
    assert_eq!(w.chest_slot(p, 0), Some(hand));
}
#[test]
fn chest_entity_identity_survives_weathering_but_changes_on_replacement() {
    let mut w = world();
    let p = IVec3::new(2, 64, 2);
    chest(&mut w, p, double_chest::Kind::Copper(Stage::Unaffected));
    let id = w.transport_chest_identity(p);
    assert!(id.is_some());
    chest(&mut w, p, double_chest::Kind::Copper(Stage::Weathered));
    assert_eq!(w.transport_chest_identity(p), id);
    w.set_block(p, Block::AIR);
    chest(&mut w, p, double_chest::Kind::Copper(Stage::Weathered));
    assert_ne!(w.transport_chest_identity(p), id);
}

#[test]
fn search_bounds_include_edges_exclude_outside_and_shrink_for_passengers() {
    let mut w = world();
    let mut e = Entities::new(44);
    let inside = IVec3::new(34, 72, 2);
    let outside = IVec3::new(2, 73, 2);
    for p in [inside, outside] {
        chest(&mut w, p, double_chest::Kind::Copper(Stage::Unaffected));
    }
    golem(&mut e, DVec3::new(2.5, 64., 2.5));
    e.tick_copper_golems(0.05, &mut w);
    assert_eq!(e.mobs[0].copper_golem.as_ref().unwrap().target.unwrap().pos, inside);
    let s = e.mobs[0].copper_golem.as_mut().unwrap();
    s.interrupt();
    s.clear_memories();
    s.cooldown = 0;
    e.mobs[0].riding = Some(999);
    e.tick_copper_golems(0.05, &mut w);
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().target.is_none());
    let near = IVec3::new(3, 65, 2);
    chest(&mut w, near, double_chest::Kind::WaxedCopper(Stage::Oxidized));
    e.mobs[0].copper_golem.as_mut().unwrap().cooldown = 0;
    e.tick_copper_golems(0.05, &mut w);
    assert_eq!(e.mobs[0].copper_golem.as_ref().unwrap().target.unwrap().pos, near);
}
#[test]
fn copper_golem_survives_falls_and_breathes_under_water_and_lightning_uses_base_damage() {
    let mut w = world();
    let mut e = Entities::new(5);
    golem(&mut e, DVec3::new(2.5, 78., 2.5));
    e.mobs[0].on_ground = false;
    e.mobs[0].copper_golem.as_mut().unwrap().wax();
    step(&mut e, &mut w, 100);
    assert_eq!(e.mobs[0].health, 12.);
    assert!((e.mobs[0].pos.y - 64.).abs() < 0.1);
    for x in 0..=4 {
        for z in 0..=4 {
            for y in 64..=66 {
                w.set_block(IVec3::new(x, y, z), Block::WATER);
            }
        }
    }
    e.mobs[0].pos = DVec3::new(2.5, 64., 2.5);
    step(&mut e, &mut w, 400);
    assert!(e.mobs[0].in_water);
    assert_eq!(e.mobs[0].health, 12.);
    e.mobs[0].copper_golem.as_mut().unwrap().stage = Stage::Weathered;
    assert!(!e.mobs[0].thunder_transform(55, 2));
    assert_eq!(e.mobs[0].copper_golem.as_ref().unwrap().stage, Stage::Exposed);
    assert!(!e.mobs[0].copper_golem.as_ref().unwrap().waxed());
}

#[test]
fn sitting_cat_on_short_chest_lid_blocks_transport() {
    let mut w = world();
    let mut e = Entities::new(13);
    let p = IVec3::new(2, 64, 2);
    chest(&mut w, p, double_chest::Kind::Copper(Stage::Unaffected));
    w.set_chest_slot(p, 0, Some(Stack::new(Item::DIAMOND, 16)));
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    e.spawn(MobKind::Cat, DVec3::new(2.5, 64.875, 2.5));
    e.mobs[1].animal.as_mut().unwrap().sitting = true;
    e.tick_copper_golems(4., &mut w);
    assert!(cargo(&e).is_none());
    assert_eq!(w.chest_slot(p, 0).unwrap().count, 16);
    e.mobs[1].pos += DVec3::X * 10.;
    e.tick_copper_golems(12., &mut w);
    assert_eq!(cargo(&e).unwrap().count, 16);
    assert!(w.chest_slot(p, 0).is_none());
}

#[test]
fn simulation_partial_delivery_retains_exact_remainder_across_save() {
    let mut w = world();
    let mut e = Entities::new(8);
    let p = IVec3::new(2, 64, 2);
    chest(&mut w, p, double_chest::Kind::Plain);
    for n in 0..27 {
        w.set_chest_slot(p, n, Some(Stack::new(Block::STONE, 64)));
    }
    let mut stack = Stack::new(Item::DIAMOND, 16);
    stack.name = crate::inventory::StackName::new("parcel").unwrap();
    stack.entity_data = 93;
    w.set_chest_slot(p, 0, Some(Stack { count: 60, ..stack }));
    golem(&mut e, DVec3::new(1.5, 64., 2.5));
    e.mobs[0].copper_golem.as_mut().unwrap().cargo = Some(stack);
    step(&mut e, &mut w, 61);
    assert_eq!(w.chest_slot(p, 0), Some(Stack { count: 64, ..stack }));
    assert_eq!(cargo(&e), Some(Stack { count: 12, ..stack }));
    let mut back = Entities::new(9);
    back.load_nether_mobs(&e.nether_mobs_to_string());
    assert_eq!(cargo(&back), cargo(&e));
    let mut restored = world();
    restored.load_chests(&w.chests_to_string());
    assert_eq!(restored.chest_slot(p, 0), w.chest_slot(p, 0));
    assert!(e.items.is_empty());
}

#[test]
fn visited_and_unreachable_brain_memory_survives_saving_with_loaded_tick_ttl() {
    let mut s = State {
        visited: vec![IVec3::new(1, 64, 2)],
        unreachable: vec![IVec3::new(3, 64, 4)],
        visited_ttl: 5932,
        unreachable_ttl: 2331,
        ..State::default()
    };
    let value = s.save();
    s = State::default();
    s.load(&value);
    assert_eq!(s.visited, vec![IVec3::new(1, 64, 2)]);
    assert_eq!(s.unreachable, vec![IVec3::new(3, 64, 4)]);
    assert_eq!((s.visited_ttl, s.unreachable_ttl), (5932, 2331));
    assert!(s.look()["visited"].is_null());
}
