use super::*;
use crate::{
    entity::{Mob, PlayerId, Target},
    inventory::Stack,
    item::Item,
    simulation::lighting::SpawnLight,
    world::{
        chunk::ChunkData,
        creaking as c, shape,
        terrain::{Dimension, Generator},
    },
};
use std::sync::Arc;
const P: IVec3 = IVec3::new(16, 140, 16);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.insert_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
    }
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, 136, z), Block::STONE);
        }
    }
    w.set_block(P - IVec3::Y, crate::world::overworld_blocks::PALE_OAK_LOG);
    w.set_block(P + IVec3::Y, crate::world::overworld_blocks::PALE_OAK_LOG);
    w.set_block(P, c::HEART);
    w
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, P.as_dvec3() + DVec3::new(6., -3., 0.), true)],
        daylight: 0.,
        spawn_light: SpawnLight::NIGHT,
        spawning: true,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn step(e: &mut Entities, w: &mut World, c: &Ctx, n: usize) {
    for _ in 0..n {
        e.update_world(0.05, w, c, Difficulty::Normal);
    }
}
#[test]
fn creaking_reserved_states_shapes_and_items() {
    assert_eq!(MobKind::ALL[69], MobKind::Creaking);
    for axis in 0..3 {
        for a in [c::Activity::Uprooted, c::Activity::Dormant, c::Activity::Awake] {
            for natural in [false, true] {
                let b = c::block(axis, a, natural);
                assert_eq!(c::heart(b), Some((axis, a, natural)));
                assert_eq!(b.base(), c::HEART);
                assert_eq!(b.hardness(), 10.);
                assert_eq!(b.name(), "creaking heart");
            }
        }
    }
    for wet in [false, true] {
        for f in 1..64 {
            let b = faces::multiface(Multiface::ResinClump, f, wet);
            assert_eq!(b.base(), c::RESIN_CLUMP);
            assert_eq!(b.is_waterlogged(), wet);
            assert_eq!(b.with_water(!wet), faces::multiface(Multiface::ResinClump, f, !wet));
            assert!(!b.is_solid());
            assert!(shape::collision(b, |_| Block::AIR, Block::AIR).as_slice().is_empty());
        }
    }
    for b in [c::HEART, c::RESIN_CLUMP, c::RESIN_BLOCK, c::RESIN_BRICKS, c::CHISELED, c::STAIRS, c::SLAB, c::WALL] {
        assert_eq!(Block::from_name(b.name()), Some(b));
        assert!(Block::creative_palette().any(|p| p == b));
        assert!(Item::from_block(b).is_valid());
    }
    assert_eq!(crate::world::furnace::smelt(c::RESIN_CLUMP.into()), Some(Item::RESIN_BRICK));
    assert_eq!(crate::world::furnace::smelt_xp(Item::RESIN_BRICK), 0.3);
    assert!(!MobKind::Creaking.spawns_in(Dimension::Overworld));
    assert_eq!(MobKind::Creaking.max_health(), 1.);
}
#[test]
fn creaking_player_heart_spawns_once_binds_emits_and_dies() {
    let mut w = world();
    let mut e = Entities::new(263);
    let c = ctx();
    step(&mut e, &mut w, &c, 100);
    assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Creaking).count(), 1);
    let i = e.mobs.iter().position(|m| m.kind == MobKind::Creaking).unwrap();
    assert!(!e.mobs[i].persistent);
    assert_eq!(e.mobs[i].creaking.as_ref().unwrap().home, Some(P));
    assert_eq!(e.attack(i, DVec3::X, 100.), None);
    assert_eq!(e.mobs[i].health, 1.);
    step(&mut e, &mut w, &c, 1);
    let resin = |w: &World| {
        (-3..=3)
            .flat_map(|x| (-3..=3).flat_map(move |z| (-3..=3).map(move |y| P + IVec3::new(x, y, z))))
            .filter_map(|p| w.get_block(p))
            .filter_map(faces::multiface_of)
            .filter(|(k, _, _)| *k == Multiface::ResinClump)
            .map(|(_, f, _)| f.count_ones())
            .sum::<u32>()
    };
    let first = resin(&w);
    assert!((2..=3).contains(&first), "{first}");
    for _ in 0..10 {
        e.attack(i, DVec3::X, 100.);
        step(&mut e, &mut w, &c, 1);
    }
    assert_eq!(resin(&w), first, "100-tick resin cooldown");
    step(&mut e, &mut w, &c, 89);
    e.attack(i, DVec3::X, 100.);
    step(&mut e, &mut w, &c, 1);
    assert!(resin(&w) > first, "the next emission is available on tick 100");
    assert!(w.container_signal(P).is_some());
    w.set_block(P, Block::AIR);
    step(&mut e, &mut w, &c, 50);
    assert!(!e.mobs.iter().any(|m| m.kind == MobKind::Creaking && m.alive()));
}
#[test]
fn creaking_day_and_reload_binding() {
    let mut w = world();
    let mut e = Entities::new(18);
    let c = ctx();
    step(&mut e, &mut w, &c, 1);
    let m = e.mobs.iter().find(|m| m.kind == MobKind::Creaking).unwrap();
    let id = m.uid;
    let saved = super::super::nether::save_mob(m);
    let loaded = super::super::nether::load_mob(&saved).unwrap();
    assert_eq!(loaded.creaking.as_ref().unwrap().home, Some(P));
    assert_eq!(loaded.uid, id);
    let h = w.creaking_hearts_to_string();
    let mut other = world();
    other.load_creaking_hearts(&h);
    assert_eq!(other.creaking_hearts[&P.to_array()].protector, Some(id));
    e.mobs = vec![loaded];
    w.set_bee_clock(Dimension::Overworld, 6000);
    step(&mut e, &mut w, &c, 50);
    assert!(!e.mobs.iter().any(|m| m.kind == MobKind::Creaking && m.alive()));
}
#[test]
fn creaking_watches_any_player_and_cannot_attack_when_watched() {
    let mut w = world();
    let mut c = ctx();
    let pos = IVec3::new(10, 137, 16).as_dvec3() + DVec3::new(0.5, 0., 0.5);
    let mut m = Mob::new(MobKind::Creaking, pos, 0.);
    let mut rng = Rng::new(1);
    let mut events = Vec::new();
    c.players[0].pos = pos + DVec3::new(2., 0., 0.);
    c.players[0].look = DVec3::NEG_X;
    m.update(0.05, &w, &c, &mut rng, &mut events);
    assert!(!m.creaking.as_ref().unwrap().can_move);
    assert!(m.creaking.as_ref().unwrap().active);
    assert!(!events.iter().any(|e| matches!(e, super::super::EntityEvent::PlayerHit { .. })));
    let mut second = c.players[0];
    second.id = PlayerId(1);
    c.players[0].look = DVec3::X;
    c.players.push(second);
    m.update(0.05, &w, &c, &mut rng, &mut events);
    assert!(!m.creaking.as_ref().unwrap().can_move);
    c.players[1].look = DVec3::X;
    m.update(0.05, &w, &c, &mut rng, &mut events);
    assert!(m.creaking.as_ref().unwrap().can_move);
    c.players[0].look = DVec3::NEG_X;
    c.players[1].look = DVec3::NEG_X;
    for y in 137..141 {
        w.set_block(IVec3::new(11, y, 16), Block::STONE);
    }
    m.update(0.05, &w, &c, &mut rng, &mut events);
    assert!(m.creaking.as_ref().unwrap().can_move, "occluded players do not freeze it");
}
#[test]
fn creaking_loot_and_resin_face_count() {
    let mut w = world();
    w.spill_mined(P, c::HEART, Default::default());
    assert!((1..=3).contains(&w.drops.last().unwrap().1.count));
    assert_eq!(w.drops.last().unwrap().1.item, Item::from_block(c::RESIN_CLUMP));
    let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
    for _ in 0..50 {
        let xp = crate::mining::mined_xp(c::block(0, c::Activity::Dormant, true), silk, &mut Rng::new(12));
        assert!((20..=24).contains(&xp));
    }
    assert_eq!(crate::mining::mined_xp(c::HEART, silk, &mut Rng::new(1)), 0);
    w.spill_mined(P, c::HEART, silk);
    assert_eq!(w.drops.last().unwrap().1, Stack::new(c::HEART, 1));
    w.spill_mined(P, faces::multiface(Multiface::ResinClump, 63, true), silk);
    assert_eq!(w.drops.last().unwrap().1, Stack::new(c::RESIN_CLUMP, 6));
    for id in 14792..14877 {
        let b = Block(id);
        let item_block = if id < 14872 { c::STAIRS } else { c::SLAB };
        assert_eq!(b.base(), item_block, "placed state #{id} picks the dry item");
        assert_eq!(b.drop(), Some(Item::from_block(item_block)));
        w.spill_mined(P, b, Default::default());
        assert_eq!(w.drops.last().unwrap().1, Stack::new(item_block, if id == 14876 { 2 } else { 1 }));
    }
}
