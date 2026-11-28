//! v1.0 gap Track 8b: llama spit flying on through players and mobs
//! (`LlamaSpit.onHitEntity` never discards it), the mob an End crystal's
//! blast names (`EndCrystal.hurtServer`: `explosion(crystal,
//! source.getEntity())`), and `checkWalls` on bubble columns.

use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::Dragon;
use crate::entity::thrown::{Kind, Thrown};
use crate::entity::{Ctx, Entities, EntityEvent, Mob, MobKind, PlayerId, Rng, Target};
use crate::physics::test_util::Grid;
use crate::simulation::damage::DamageType;
use crate::world::block::Block;

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players,
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn llama(e: &mut Entities, uid: u32) {
    let mut llama = Mob::new(MobKind::Llama, DVec3::new(-3.0, 11.0, 8.5), 0.0);
    llama.uid = uid;
    e.mobs.push(llama);
}

/// Spit hurts the mob and the player in its way once each, by 1, and
/// flies on past both; with its llama gone it hurts neither.
#[test]
fn llama_spit_flies_on_through_mobs_and_players() {
    let world = Grid::flat(10);
    for llama_alive in [true, false] {
        let mut e = Entities::new(1);
        if llama_alive {
            llama(&mut e, 7);
        }
        let mut cow = Mob::new(MobKind::Cow, DVec3::new(0.5, 10.0, 0.5), 0.0);
        cow.uid = 8;
        e.mobs.push(cow);
        let player = Target::new(PlayerId::HOST, DVec3::new(4.5, 10.0, 0.5), true);
        let from = DVec3::new(-3.0, 11.0, 0.5);
        e.thrown.push(Thrown::spit(from, DVec3::new(8.5, 10.0, 0.5), 7, &mut Rng::new(2)));
        let mut hits = 0;
        for _ in 0..8 {
            for ev in e.update(0.05, &world, &ctx(vec![player])) {
                if let EntityEvent::PlayerHit { player: PlayerId::HOST, damage, .. } = ev {
                    assert_eq!(damage, 1.0);
                    hits += 1;
                }
            }
        }
        let cow = e.mobs.iter().find(|m| m.uid == 8).unwrap();
        let lost = MobKind::Cow.max_health() - cow.health;
        assert_eq!(lost, if llama_alive { 1.0 } else { 0.0 }, "llama alive: {llama_alive}");
        assert_eq!(hits, u32::from(llama_alive), "llama alive: {llama_alive}");
        assert!(
            e.thrown.iter().any(|t| t.kind == Kind::LlamaSpit && t.pos.x > 5.5),
            "the spit flies on past both (llama alive: {llama_alive})"
        );
    }
}

/// The crystal's blast names the mob behind the blow, a `PLAYER_EXPLOSION`
/// ("was blown up by" it): a llama's spit, a skeleton's arrow. An ownerless
/// projectile's blast stays the crystal's own.
#[test]
fn a_crystal_broken_by_a_mob_blames_that_mob() {
    use crate::entity::Arrow;
    let world = Grid::flat(10);
    let blast = |events: &[EntityEvent]| {
        events.iter().find_map(|e| match *e {
            EntityEvent::Explosion { cause, credit_player, .. } => Some((cause, credit_player)),
            _ => None,
        })
    };
    // Spit.
    let mut e = Entities::new(1);
    llama(&mut e, 7);
    e.crystals.place(DVec3::new(0.5, 11.0, 0.5));
    e.thrown.push(Thrown::spit(DVec3::new(-3.0, 12.0, 0.5), DVec3::new(0.5, 12.0, 0.5), 7, &mut Rng::new(2)));
    let events: Vec<_> = (0..5).flat_map(|_| e.update(0.05, &world, &ctx(Vec::new()))).collect();
    let (cause, credit) = blast(&events).expect("the crystal blew up");
    assert_eq!((cause.kind, cause.text, credit), (DamageType::PlayerExplosion, "was blown up by a llama", false));
    // A skeleton's arrow.
    let mut e = Entities::new(1);
    let mut skeleton = Mob::new(MobKind::Skeleton, DVec3::new(-6.0, 11.0, 8.5), 0.0);
    skeleton.uid = 4;
    e.mobs.push(skeleton);
    e.crystals.place(DVec3::new(0.5, 11.0, 0.5));
    let cause = crate::simulation::damage::Cause::SKELETON_ARROW;
    e.arrows.push(Arrow::mob_shot(DVec3::new(-4.0, 12.0, 0.5), DVec3::X * 30.0, cause, 4));
    let events: Vec<_> = (0..5).flat_map(|_| e.update(0.05, &world, &ctx(Vec::new()))).collect();
    let (cause, _) = blast(&events).expect("the crystal blew up");
    assert_eq!((cause.kind, cause.text), (DamageType::PlayerExplosion, "was blown up by a skeleton"));
    // A dispenser's arrow has no owner.
    let mut e = Entities::new(1);
    e.crystals.place(DVec3::new(0.5, 11.0, 0.5));
    e.arrows.push(Arrow::dispensed(DVec3::new(-4.0, 12.0, 0.5), DVec3::X * 30.0));
    let events: Vec<_> = (0..5).flat_map(|_| e.update(0.05, &world, &ctx(Vec::new()))).collect();
    let (cause, _) = blast(&events).expect("the crystal blew up");
    assert_eq!(cause, crate::simulation::damage::Cause::END_CRYSTAL);
}

/// `checkWalls` on a bubble column: `removeBlock` leaves its fluid's
/// legacy block, a water source, so the cell counts as broken (level event
/// 2008); with `mob_griefing` off it is a wall. The world turns the cell to
/// water and the column re-forms on the next bubble tick.
#[test]
fn the_dragon_turns_bubble_columns_to_water_and_they_re_form() {
    use crate::world::World;
    use crate::world::bubble::{UP, is_column};
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    let d = Dragon::new(DVec3::new(8.5, 40.0, 8.5));
    let (_, min, max) = d.parts[0];
    let cell = ((min + max) / 2.0).floor().as_ivec3();
    let mut grid = Grid::flat(0);
    grid.set(cell, UP);
    for griefing in [true, false] {
        let mut d = Dragon::new(DVec3::new(8.5, 40.0, 8.5));
        let mut events = Vec::new();
        d.check_walls(&grid, griefing, &mut Rng::new(4), &mut events);
        let broke = events.iter().any(|e| matches!(e, EntityEvent::BreakBlock { cell: c } if *c == cell));
        let smashed = events.iter().any(|e| matches!(e, EntityEvent::DragonSmashed { .. }));
        assert_eq!((broke, smashed, d.in_wall), (griefing, griefing, !griefing), "griefing {griefing}");
    }
    // Still water is no wall and nothing breaks.
    grid.set(cell, Block::WATER);
    let mut d = Dragon::new(DVec3::new(8.5, 40.0, 8.5));
    let mut events = Vec::new();
    d.check_walls(&grid, true, &mut Rng::new(4), &mut events);
    assert!(events.is_empty() && !d.in_wall);

    // The world's side.
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    let base = IVec3::new(4, 140, 8);
    w.edit(base, Block::SOUL_SAND, false);
    for y in 1..=4 {
        w.edit(base + IVec3::Y * y, Block::WATER, false);
    }
    for _ in 0..8 {
        w.tick_fluids(0.25);
    }
    let at = base + IVec3::Y * 2;
    assert!(is_column(w.get_block(at).unwrap()), "the column stands");
    w.break_blocks(&[at]);
    assert_eq!(w.get_block(at), Some(Block::WATER), "a water source, as `removeBlock` leaves");
    w.tick_fluids(0.25);
    assert!(is_column(w.get_block(at).unwrap()), "and the column re-forms through it");
}
