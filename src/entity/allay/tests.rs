use super::*;
#[test]
fn identity_and_payload_round_trip() {
    let mut e = Entities::new(1);
    e.spawn(MobKind::Allay, DVec3::Y * 10.);
    let s = e.mobs[0].allay.as_mut().unwrap();
    s.liked = Some(Stack::new(crate::item::Item::DIAMOND, 1));
    s.cargo = Some(Stack::new(crate::item::Item::DIAMOND, 13));
    s.liked.as_mut().unwrap().name = crate::inventory::StackName::new("Keeper").unwrap();
    s.cargo.as_mut().unwrap().name = s.liked.unwrap().name;
    s.ground_speed = Some(f64::from(0.3_f32));
    s.ticks = 29;
    s.remainder = 0.025;
    s.vibration = Some((IVec3::new(-3, 10, 6), 3));
    s.give_ticks = 12;
    s.player = Some(PlayerId(42));
    s.note = Some(IVec3::new(-3, 10, 6));
    s.note_ticks = 411;
    s.duplication_ticks = 2000;
    s.pickup_ticks = 29;
    s.dancing = true;
    s.jukebox = Some(IVec3::ZERO);
    let saved = e.nether_mobs_to_string();
    let mut loaded = Entities::new(2);
    loaded.load_nether_mobs(&saved);
    assert_eq!(loaded.mobs.len(), 1);
    assert_eq!(loaded.mobs[0].allay, e.mobs[0].allay);
    let mut replica = Mob::new(MobKind::Allay, DVec3::ZERO, 0.);
    replica.apply_look(&e.mobs[0].look());
    assert_eq!(replica.allay, e.mobs[0].allay);
    assert_eq!(MobKind::Allay.lan_index(), 70);
    assert_eq!(MobKind::from_lan_index(70), Some(MobKind::Allay));
    assert_eq!(MobKind::from_lan_index(69), None);
    assert_eq!(MobKind::from_lan_index(68), Some(MobKind::CopperGolem));
    assert_eq!(MobKind::from_name("minecraft:allay"), Some(MobKind::Allay));
}
use crate::{
    entity::Target,
    item::Item,
    physics::test_util::Grid,
    simulation::{difficulty::Difficulty, lighting::SpawnLight},
    world::terrain::{Dimension, Generator},
};
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(2., 10., 0.), false)],
        daylight: 1.,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
        spawn_light: SpawnLight::DAY,
    }
}
fn collector() -> Entities {
    let mut e = Entities::new(1);
    e.spawn(MobKind::Allay, DVec3::new(0., 10., 0.));
    e.mobs[0].allay.as_mut().unwrap().liked = Some(Stack::new(Item::DIAMOND, 1));
    e
}
fn ticks(e: &mut Entities, world: &impl MobWorld, ctx: &Ctx, n: usize) {
    for _ in 0..n {
        e.tick_allays(0.05, world, ctx, &mut Vec::new());
    }
}
#[test]
fn java_attributes_regeneration_flight_and_no_loot_or_natural_spawn() {
    let world = Grid::flat(-64);
    let mut e = collector();
    let m = &e.mobs[0];
    assert_eq!(m.shape(), MobKind::Allay.shape());
    assert_eq!(m.health, 20.);
    assert!(m.persistent && m.kind.leashable() && !m.kind.is_hostile());
    for d in [Dimension::Overworld, Dimension::Nether, Dimension::End] {
        assert!(!m.kind.spawns_in(d));
    }
    assert!(m.kind.loot().is_empty());
    assert_eq!(m.kind.xp(&mut Rng::new(1)), 0);
    e.mobs[0].health = 7.;
    ticks(&mut e, &world, &ctx(), 9);
    assert_eq!(e.mobs[0].health, 7.);
    ticks(&mut e, &world, &ctx(), 1);
    assert_eq!(e.mobs[0].health, 8.);
    e.mobs[0].simulation_active = false;
    ticks(&mut e, &world, &ctx(), 100);
    assert_eq!(e.mobs[0].health, 8.);
    let m = &mut e.mobs[0];
    for _ in 0..100 {
        travel(m, 0.05, &world, Some(DVec3::X), 1.);
    }
    // FlyingMoveControl sets .1 forward input and .1 speed; .98 input,
    // .91 drag. Steady displacement per second = .0098/.09*20.
    assert!((m.vel.x - (0.0098 / 0.09 * 0.91 * 20.)).abs() < 0.001);
    assert!((m.pos.y - 10.).abs() < 0.0001, "no gravity");
    assert_eq!(m.health, 8., "no fall damage");
}
#[test]
fn pickup_matches_java_item_equality_respects_delays_capacity_and_griefing() {
    let world = Grid::flat(-64);
    let mut e = collector();
    let pos = e.mobs[0].pos;
    // The liked sample is a plain diamond: Java's allayConsidersItemEqual
    // ignores names, so renamed diamonds are wanted too.
    let mut renamed = Stack::new(Item::DIAMOND, 20);
    renamed.name = crate::inventory::StackName::new("different").unwrap();
    e.items.push(ItemEntity::new(renamed, pos, DVec3::ZERO, 0., &mut e.rng));
    e.items.push(ItemEntity::new(Stack::new(Item::DIAMOND, 64), pos, DVec3::ZERO, 2., &mut e.rng));
    e.items.push(ItemEntity::new(Stack::new(Item::EMERALD, 5), pos, DVec3::ZERO, 0., &mut e.rng));
    e.villager_griefing = false;
    ticks(&mut e, &world, &ctx(), 1);
    assert!(e.mobs[0].allay.as_ref().unwrap().cargo.is_none(), "mob griefing gates pickup");
    e.villager_griefing = true;
    // Plain cargo cannot accept the renamed stack (SimpleContainer compares
    // components), and the delayed plain stack is not collectible yet.
    e.mobs[0].allay.as_mut().unwrap().cargo = Some(Stack::new(Item::DIAMOND, 60));
    ticks(&mut e, &world, &ctx(), 1);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().cargo.unwrap().count, 60);
    assert_eq!(e.items[0].stack, renamed);
    e.items[1].pickup_delay = 0.;
    ticks(&mut e, &world, &ctx(), 1);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().cargo.unwrap().count, 64);
    assert_eq!(e.items[1].stack.count, 60);
    assert_eq!(e.items[2].stack.count, 5, "unliked items stay");
    // An empty slot takes the renamed stack whole.
    e.mobs[0].allay.as_mut().unwrap().cargo = None;
    ticks(&mut e, &world, &ctx(), 1);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().cargo.unwrap(), renamed);
}
#[test]
fn item_equality_ignores_components_except_potion_contents() {
    use super::considers_equal;
    let sword = |t| Item::tool(crate::item::ToolKind::Sword, t);
    let mut named = Stack::new(sword(crate::item::Tier::Diamond), 1);
    named.name = crate::inventory::StackName::new("x").unwrap();
    named.damage = 7;
    assert!(considers_equal(Stack::new(sword(crate::item::Tier::Diamond), 1), named));
    assert!(!considers_equal(named, Stack::new(sword(crate::item::Tier::Iron), 1)));
    let arrow = |p| Stack { entity_data: p, ..Stack::new(Item::TIPPED_ARROW, 1) };
    assert!(considers_equal(arrow(3), arrow(3)));
    assert!(!considers_equal(arrow(3), arrow(4)));
    let lingering = |p| Stack { entity_data: p, ..Stack::new(Item::LINGERING_POTION, 1) };
    assert!(!considers_equal(lingering(1), lingering(2)));
    let p = crate::potion::Potion(1);
    assert!(considers_equal(Stack::new(Item::potion(p), 1), Stack::new(Item::potion(p), 1)));
    assert!(!considers_equal(Stack::new(Item::potion(p), 1), Stack::new(Item::splash_potion(p), 1)));
}
#[test]
fn sensor_range_player_range_and_throw_cooldown_match_reference() {
    let mut world = Grid::flat(-64);
    let mut e = collector();
    let pos = e.mobs[0].pos;
    e.items.push(ItemEntity::new(Stack::new(Item::DIAMOND, 1), pos + DVec3::X * 33., DVec3::ZERO, 0., &mut e.rng));
    e.items.push(ItemEntity::new(Stack::new(Item::DIAMOND, 1), pos + DVec3::X * 31., DVec3::ZERO, 0., &mut e.rng));
    ticks(&mut e, &world, &ctx(), 1);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().wanted, Some(pos + DVec3::X * 31.));
    world.set(IVec3::new(15, 10, 0), Block::STONE);
    e.mobs[0].allay.as_mut().unwrap().sensor = 0;
    ticks(&mut e, &world, &ctx(), 1);
    assert!(e.mobs[0].allay.as_ref().unwrap().wanted.is_none());
    let s = e.mobs[0].allay.as_mut().unwrap();
    s.player = Some(PlayerId::HOST);
    s.cargo = Some(Stack::new(Item::DIAMOND, 2));
    let mut c = ctx();
    c.players[0].pos.x = 64.;
    ticks(&mut e, &world, &c, 1);
    assert!(e.mobs[0].allay.as_ref().unwrap().deposit.is_none());
    c.players[0].pos.x = 2.;
    ticks(&mut e, &world, &c, 2);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().cargo.unwrap().count, 1);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().pickup_ticks, 60);
    assert_eq!(e.items.last().unwrap().stack.count, 1);
    assert_eq!(e.items.last().unwrap().pickup_delay, 0.5);
    assert!(e.items.last().unwrap().vel.x > 0. && e.items.last().unwrap().vel.y > 0.);
    e.mobs[0].allay.as_mut().unwrap().cargo = None;
    ticks(&mut e, &world, &c, 59);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().pickup_ticks, 1);
    ticks(&mut e, &world, &c, 1);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().pickup_ticks, 0);
}
#[test]
fn notes_prefer_first_refresh_expire_and_fall_back_to_player() {
    let mut world = Grid::flat(-64);
    let mut e = collector();
    let a = IVec3::new(1, 10, 0);
    let b = IVec3::new(2, 10, 0);
    world.set(a, crate::world::gadgets::NOTE);
    world.set(b, crate::world::gadgets::NOTE);
    let s = e.mobs[0].allay.as_mut().unwrap();
    s.player = Some(PlayerId::HOST);
    s.hear_note(a);
    s.hear_note(b);
    assert_eq!(s.note, Some(a));
    ticks(&mut e, &world, &ctx(), 599);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().note_ticks, 1);
    let s = e.mobs[0].allay.as_mut().unwrap();
    s.hear_note(a);
    assert_eq!(s.note_ticks, 600);
    ticks(&mut e, &world, &ctx(), 600);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().note, None);
    assert!(e.mobs[0].allay.as_ref().unwrap().deposit.unwrap().distance(ctx().players[0].pos + DVec3::Y * 1.62) < 1e-6);
    e.mobs[0].allay.as_mut().unwrap().hear_note(a);
    world.set(a, Block::AIR);
    ticks(&mut e, &world, &ctx(), 1);
    assert!(e.mobs[0].allay.as_ref().unwrap().note.is_none());
}
#[test]
fn vibration_delay_radius_wool_and_redstone_playback_share_the_hook() {
    use std::sync::Arc;
    let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    let p = IVec3::new(8, 10, 8);
    w.set_block(p, crate::world::gadgets::NOTE);
    let mut e = collector();
    e.mobs[0].pos = DVec3::new(13.5, 10., 8.5);
    assert!(w.strike_note(p));
    e.hear_allay_notes(&mut w);
    assert!(e.mobs[0].allay.as_ref().unwrap().vibration.is_some());
    ticks(&mut e, &w, &ctx(), 4);
    assert!(e.mobs[0].allay.as_ref().unwrap().note.is_none());
    ticks(&mut e, &w, &ctx(), 2);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().note, Some(p));
    assert!(!occluded(&w, p, e.mobs[0].pos));
    for y in 9..=12 {
        for z in 6..=10 {
            w.set_block(IVec3::new(10, y, z), Block::WOOL);
        }
    }
    assert!(occluded(&w, p, e.mobs[0].pos));
    e.mobs[0].allay.as_mut().unwrap().note = None;
    w.strike_note(p);
    e.hear_allay_notes(&mut w);
    assert!(e.mobs[0].allay.as_ref().unwrap().vibration.is_none());
    e.mobs[0].pos.x = 25.;
    w.strike_note(p);
    e.hear_allay_notes(&mut w);
    assert!(e.mobs[0].allay.as_ref().unwrap().vibration.is_none());
    // Removing the wall and powering the note goes through play_note too.
    for y in 9..=12 {
        for z in 6..=10 {
            w.set_block(IVec3::new(10, y, z), Block::AIR);
        }
    }
    e.mobs[0].pos.x = 13.5;
    w.set_block(p - IVec3::X, crate::world::redstone_blocks::REDSTONE_BLOCK);
    w.tick_redstone();
    e.hear_allay_notes(&mut w);
    assert!(e.mobs[0].allay.as_ref().unwrap().vibration.is_some());
}
#[test]
fn duplication_cooldown_and_newborn_ownership_are_independent() {
    let world = Grid::flat(-64);
    let mut e = collector();
    assert!(!e.duplicate_allay(0));
    e.mobs[0].allay.as_mut().unwrap().set_jukebox_playing(IVec3::new(0, 10, 0), true);
    e.mobs[0].allay.as_mut().unwrap().player = Some(PlayerId::HOST);
    assert!(e.duplicate_allay(0));
    assert_eq!(e.mobs.len(), 2);
    assert_eq!(e.mobs[0].allay.as_ref().unwrap().duplication_ticks, 6000);
    assert_eq!(e.mobs[1].allay.as_ref().unwrap().duplication_ticks, 6000);
    assert!(e.mobs[1].allay.as_ref().unwrap().liked.is_none());
    assert!(e.mobs[1].allay.as_ref().unwrap().player.is_none());
    assert!(!e.duplicate_allay(0));
    ticks(&mut e, &world, &ctx(), 6000);
    assert!(e.duplicate_allay(0));
    assert!(e.mobs[0].persistent && e.mobs[1].persistent);
}
#[test]
fn liked_player_cannot_melee_or_fire_aspect_the_allay_and_death_drops_once() {
    let mut e = collector();
    e.mobs[0].allay.as_mut().unwrap().player = Some(PlayerId::HOST);
    let mut sword = Stack::new(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Diamond), 1);
    sword.enchants = sword.enchants.with(crate::enchant::Enchantment::FireAspect, 2);
    e.player_melee(
        0,
        super::super::PlayerMelee {
            held: Some(sword),
            bonus: 0.,
            strength: 1.,
            critical: false,
            sprint_knockback: false,
            origin: DVec3::ZERO,
            yaw: 0.,
            sweep: false,
            owner: PlayerId::HOST,
        },
    );
    assert_eq!(e.mobs[0].health, 20.);
    assert!(!e.mobs[0].burning);
    e.mobs[0].allay.as_mut().unwrap().cargo = Some(Stack::new(Item::DIAMOND, 19));
    e.mobs[0].damage(100., None, &mut e.rng);
    e.mob_loot = false;
    e.drop_loot(MobKind::Allay, e.mobs[0].pos);
    assert_eq!(e.items.len(), 2);
    assert!(e.orbs.is_empty());
    e.drop_loot(MobKind::Allay, e.mobs[0].pos);
    assert_eq!(e.items.len(), 2);
}
#[test]
fn loaded_ai_retrieves_and_delivers_through_a_flying_path() {
    let world = Grid::flat(0);
    let mut e = collector();
    let mut c = ctx();
    c.players[0].pos = DVec3::new(0., 8., 0.);
    let pos = DVec3::new(12., 10., 0.);
    e.items.push(ItemEntity::new(Stack::new(Item::DIAMOND, 3), pos, DVec3::ZERO, 0., &mut e.rng));
    e.mobs[0].allay.as_mut().unwrap().player = Some(PlayerId::HOST);
    let mut saw_cargo = false;
    let mut delivered = false;
    for _ in 0..800 {
        e.tick_allays(0.05, &world, &c, &mut Vec::new());
        let m = &mut e.mobs[0];
        let (wish, speed) = m.allay_steer(0.05, &world, &c, &mut e.rng);
        travel(m, 0.05, &world, wish, speed);
        saw_cargo |= m.allay.as_ref().unwrap().cargo.is_some();
        if saw_cargo && m.allay.as_ref().unwrap().cargo.is_none() {
            delivered = true;
            break;
        }
    }
    assert!(saw_cargo, "did not retrieve item: {:?}", e.mobs[0].pos);
    assert!(delivered, "did not deliver: {:?}", e.mobs[0].pos);
    assert_eq!(e.items.iter().map(|i| i.stack.count as usize).sum::<usize>(), 3);
}
#[test]
#[ignore = "manual local timing; no wall-clock assertion"]
fn allay_sensor_cost() {
    let world = Grid::flat(-64);
    let c = ctx();
    let mut e = Entities::new(1);
    for i in 0..40 {
        e.spawn(MobKind::Allay, DVec3::new((i % 8) as f64 * 8., 10., (i / 8) as f64 * 8.));
        e.mobs.last_mut().unwrap().allay.as_mut().unwrap().liked = Some(Stack::new(Item::DIAMOND, 1));
    }
    for i in 0..1000 {
        e.items.push(ItemEntity::new(
            Stack::new(Item::DIAMOND, 64),
            DVec3::new((i % 50) as f64 * 2., 2., (i / 50) as f64 * 2.),
            DVec3::ZERO,
            0.,
            &mut e.rng,
        ));
    }
    let start = std::time::Instant::now();
    ticks(&mut e, &world, &c, 400);
    eprintln!("allay_sensor_cost: 40 allays / 1000 items: {:.3} ms/tick", start.elapsed().as_secs_f64() * 1000. / 400.);
}
#[test]
fn outpost_residents_spawn_once_persist_and_do_not_resurrect() {
    use crate::physics::BlockSource;
    use std::sync::Arc;
    struct StructureWorld {
        grid: Grid,
        structure: Arc<crate::world::temples::Built>,
    }
    impl BlockSource for StructureWorld {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.grid.block(p)
        }
    }
    impl MobWorld for StructureWorld {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn structures_near(&self, _: IVec3, _: i32) -> Vec<Arc<crate::world::temples::Built>> {
            vec![self.structure.clone()]
        }
    }
    let generator = Generator::with_generation(42, Dimension::Overworld, crate::world::terrain::Generation::V3);
    let at =
        generator.temples.nearest(&generator, crate::world::temples::Kind::Outpost, glam::IVec2::ZERO, 20_000).unwrap();
    let b = (0..100)
        .flat_map(|i| generator.temples.near(&generator, at + IVec3::new(i * 600, 0, 0), 700))
        .find(|b| b.residents.iter().any(|r| r.0 == MobKind::Allay))
        .expect("a cage in deterministic nearby outposts");
    let world = StructureWorld { grid: Grid::flat(-64), structure: b };
    let mut e = Entities::new(1);
    let c = ctx();
    e.populate_structures(1., &world, &c, Difficulty::Peaceful);
    assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Allay).count(), 2);
    assert!(e.mobs.iter().filter(|m| m.kind == MobKind::Allay).all(|m| m.persistent
        && m.health == 10.
        && m.allay.as_ref().unwrap().ground_speed == Some(f64::from(0.3_f32))));
    let mut loaded = Entities::new(2);
    loaded.load_nether_mobs(&e.nether_mobs_to_string());
    loaded.populate_structures(1., &world, &c, Difficulty::Peaceful);
    assert_eq!(loaded.mobs.iter().filter(|m| m.kind == MobKind::Allay).count(), 2);
    loaded.mobs.retain(|m| m.kind != MobKind::Allay);
    loaded.populate_structures(1., &world, &c, Difficulty::Peaceful);
    assert!(loaded.mobs.iter().all(|m| m.kind != MobKind::Allay));
}
#[test]
fn allay_unload_save_restart_reload_keeps_inventory_and_memory() {
    use std::sync::Arc;
    let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    world.load_test_column(glam::IVec2::ZERO, 0);
    let mut e = collector();
    e.mobs[0].pos = DVec3::new(40.5, 10., 0.5);
    let s = e.mobs[0].allay.as_mut().unwrap();
    s.cargo = Some(Stack::new(Item::DIAMOND, 31));
    s.player = Some(PlayerId(123));
    s.hear_note(IVec3::new(42, 10, 1));
    s.duplication_ticks = 999;
    let expected = s.save();
    e.sync_chunk_entities(&mut world);
    assert!(e.mobs.is_empty());
    assert_eq!(e.stored_mobs().count(), 1);
    let mut loaded = Entities::new(2);
    loaded.load_nether_mobs(&e.nether_mobs_to_string());
    loaded.sync_chunk_entities(&mut world);
    assert_eq!(loaded.stored_mobs().count(), 1);
    world.load_test_column(glam::IVec2::new(1, 0), 0);
    loaded.sync_chunk_entities(&mut world);
    assert_eq!(loaded.mobs.len(), 1);
    assert_eq!(loaded.mobs[0].allay.as_ref().unwrap().save(), expected);
}
#[test]
fn collected_items_throw_toward_the_note_instead_of_the_linked_player() {
    let mut world = Grid::flat(-64);
    let mut e = collector();
    let mut c = ctx();
    c.players[0].pos.x = -2.;
    let note = IVec3::new(1, 10, 0);
    world.set(note, crate::world::gadgets::NOTE);
    let s = e.mobs[0].allay.as_mut().unwrap();
    s.player = Some(PlayerId::HOST);
    s.hear_note(note);
    s.cargo = Some(Stack::new(Item::DIAMOND, 1));
    ticks(&mut e, &world, &c, 2);
    assert!(e.mobs[0].allay.as_ref().unwrap().cargo.is_none());
    assert_eq!(e.items.len(), 1);
    assert!(e.items[0].vel.x > 0. && e.items[0].vel.z > 0., "note is east/south, player is west");
}
