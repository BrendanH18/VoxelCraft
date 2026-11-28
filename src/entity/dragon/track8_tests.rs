//! v1.0 gap Track 8: llama spit on End crystals (`LlamaSpit.onHitEntity`),
//! arrows set alight by a perched dragon (`AbstractDragonSittingPhase.onHurt`),
//! `checkWalls`' water and level event 2008, every dragon's parts under
//! every dragon's fireballs, and the `flap` game event.

use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::{Dragon, DragonFireball, Fight, Part, Phase, TICK, Wild};
use crate::entity::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, Rng};
use crate::physics::test_util::Grid;
use crate::world::block::Block;

fn ctx() -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players: Vec::new(),
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn run(e: &mut Entities, world: &Grid, ticks: u32) -> Vec<EntityEvent> {
    (0..ticks).flat_map(|_| e.update(0.05, world, &ctx())).collect()
}

/// The centre of the first wild dragon's body as it stands.
fn body(e: &Entities) -> DVec3 {
    let (_, min, max) = e.wild_dragons.dragons[0].dragon.parts.iter().find(|p| p.0 == Part::Body).copied().unwrap();
    (min + max) / 2.0
}

/// A llama's spit breaks an End crystal and flies on; with its llama gone
/// (`getOwner` null) it does nothing to it.
#[test]
fn llama_spit_breaks_end_crystals_and_flies_on() {
    use crate::entity::thrown::{Kind, Thrown};
    let world = Grid::flat(10);
    for llama_alive in [true, false] {
        let mut e = Entities::new(1);
        if llama_alive {
            let mut llama = Mob::new(MobKind::Llama, DVec3::new(-3.0, 11.0, 8.5), 0.0);
            llama.uid = 7;
            e.mobs.push(llama);
        }
        let id = e.crystals.place(DVec3::new(0.5, 11.0, 0.5));
        let from = DVec3::new(-3.0, 12.0, 0.5);
        e.thrown.push(Thrown::spit(from, DVec3::new(0.5, 12.0, 0.5), 7, &mut Rng::new(2)));
        run(&mut e, &world, 4);
        assert_eq!(e.crystals.get(id).is_none(), llama_alive, "llama alive: {llama_alive}");
        assert!(
            e.thrown.iter().any(|t| t.kind == Kind::LlamaSpit && t.pos.x > 1.5),
            "the spit flies on past the crystal (llama alive: {llama_alive})"
        );
    }
}

/// An arrow off a perched dragon's scales burns for a second and bounces
/// back; one that hits a flying dragon lands; a dying dragon's refusal
/// doesn't light it.
#[test]
fn a_perched_dragon_sets_arrows_alight() {
    use crate::entity::Arrow;
    let world = Grid::flat(10);
    for (phase, bounces, burns) in [
        (Phase::SittingScanning, true, true),
        (Phase::SittingAttacking, true, true),
        (Phase::Hovering, false, false),
        (Phase::Dying, true, false),
    ] {
        let mut e = Entities::new(1);
        e.wild_dragons.spawn(DVec3::new(0.5, 40.0, 0.5), 0.0);
        run(&mut e, &world, 1);
        e.wild_dragons.dragons[0].dragon.phase = phase;
        let aim = body(&e);
        e.arrows.push(Arrow::shot(aim - DVec3::new(6.0, 0.0, 0.0), DVec3::X, 1.0, true));
        run(&mut e, &world, 2);
        assert_eq!(e.arrows.len(), usize::from(bounces), "{phase:?}");
        if let Some(a) = e.arrows.first() {
            assert!(a.velocity().x < 0.0, "{phase:?}: bounced back");
            assert_eq!(a.burning(), burns, "{phase:?}");
        }
    }
    // The fire lasts a second (`igniteForSeconds(1)`).
    let mut e = Entities::new(1);
    e.wild_dragons.spawn(DVec3::new(0.5, 40.0, 0.5), 0.0);
    run(&mut e, &world, 1);
    e.wild_dragons.dragons[0].dragon.phase = Phase::SittingScanning;
    let aim = body(&e);
    e.arrows.push(Arrow::shot(aim - DVec3::new(6.0, 0.0, 0.0), DVec3::X, 1.0, true));
    let mut lit = 0;
    for _ in 0..30 {
        run(&mut e, &world, 1);
        let Some(a) = e.arrows.first() else { break };
        lit += u32::from(a.burning());
    }
    assert!((19..=20).contains(&lit), "{lit} ticks alight");
}

/// `checkWalls` on a waterlogged block leaves its water
/// (`Level.removeBlock` sets `fluidState.createLegacyBlock()`), and each
/// part that broke something posts one level event 2008 inside its box.
#[test]
fn smashed_blocks_leave_their_water_and_post_level_event_2008() {
    use crate::world::World;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    let wet = crate::world::heavy_core::HEAVY_CORE.with_water(true);
    assert!(wet.is_waterlogged());
    let d = Dragon::new(DVec3::new(8.5, 40.0, 8.5));
    let (_, min, max) = d.parts[0];
    let cell = ((min + max) / 2.0).floor().as_ivec3();
    let mut grid = Grid::flat(0);
    grid.set(cell, wet);
    let mut d = d;
    let mut events = Vec::new();
    d.check_walls(&grid, true, &mut Rng::new(4), &mut events);
    assert!(events.iter().any(|e| matches!(e, EntityEvent::BreakBlock { cell: c } if *c == cell)));
    let posted: Vec<IVec3> = events
        .iter()
        .filter_map(|e| if let EntityEvent::DragonSmashed { cell } = *e { Some(cell) } else { None })
        .collect();
    assert_eq!(posted.len(), 1, "only the body broke anything");
    let (lo, hi) = (min.floor().as_ivec3(), max.floor().as_ivec3());
    assert!(posted[0].cmpge(lo).all() && posted[0].cmple(hi).all(), "{} in {lo}..{hi}", posted[0]);
    assert!(!events.iter().any(|e| matches!(e, EntityEvent::Sound { .. })), "level event 2008 has no sound");

    // The world's side: the dragon's break leaves the water source.
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    let at = IVec3::new(4, 145, 8);
    assert!(w.edit(at, wet, false));
    assert!(w.edit(at + IVec3::X, Block::STONE, false));
    w.break_blocks(&[at, at + IVec3::X]);
    assert_eq!(w.get_block(at), Some(Block::WATER));
    assert_eq!(w.get_block(at + IVec3::X), Some(Block::AIR));
}

/// The fight's fireballs burst on a dragon no fight owns (every
/// `EnderDragonPart` is pickable), and a wild dragon's on the fight's.
#[test]
fn fireballs_burst_on_any_dragons_parts() {
    let end = crate::world::end::EndGen::new(5);
    let world = Grid::flat(0);
    let mut rng = Rng::new(1);
    for beside in [true, false] {
        let mut fight = Fight::new(&end);
        let mut crystals = crate::entity::crystal::Crystals::default();
        let mut wild = Wild::default();
        wild.spawn(DVec3::new(60.5, 90.0, 300.5), 0.0);
        let (_, min, max) = wild.dragons[0].dragon.parts[0];
        let aim = (min + max) / 2.0;
        fight.fireballs.push(DragonFireball::new(aim - DVec3::new(6.0, 0.0, 0.0), DVec3::X));
        let mut events = Vec::new();
        for _ in 0..20 {
            if beside {
                fight.update_beside(TICK, &world, &ctx(), &mut rng, &mut crystals, &wild, &mut events);
            } else {
                fight.update(TICK, &world, &ctx(), &mut rng, &mut crystals, &mut events);
            }
        }
        assert_eq!(fight.fireballs.is_empty(), beside, "beside the wild dragon: {beside}");
        assert_eq!(fight.clouds.iter().any(|c| c.pos.distance(aim) < 8.0), beside);
    }
    // And the other way round.
    let mut fight = Fight::new(&end);
    let dragon = fight.dragon.as_mut().unwrap();
    dragon.phase = Phase::Hovering;
    let (_, min, max) = dragon.parts[0];
    let aim = (min + max) / 2.0;
    let mut wild = Wild::default();
    wild.spawn(aim + DVec3::new(0.0, 0.0, 200.0), 0.0);
    wild.fireballs.push(DragonFireball::new(aim - DVec3::new(6.0, 0.0, 0.0), DVec3::X));
    let crystals = crate::entity::crystal::Crystals::default();
    let mut events = Vec::new();
    for _ in 0..20 {
        wild.update(TICK, &world, &ctx(), &mut rng, &crystals, Some(&fight), &mut events);
    }
    assert!(wild.fireballs.is_empty());
}

/// Each wing beat the dragon sounds also posts `flap` at the dragon.
#[test]
fn the_dragons_wing_beats_post_flap() {
    let world = Grid::flat(10);
    let mut e = Entities::new(1);
    e.wild_dragons.spawn(DVec3::new(0.5, 40.0, 0.5), 0.0);
    let events = run(&mut e, &world, 60);
    let flaps: Vec<DVec3> = events
        .iter()
        .filter_map(|e| match *e {
            EntityEvent::Sound { sound: MobSound::DragonFlap, pos } => Some(pos),
            _ => None,
        })
        .collect();
    assert!(!flaps.is_empty());
    let posted: Vec<DVec3> = e
        .sculk
        .events
        .iter()
        .filter(|&&(_, event, ..)| event == crate::world::vibration::GameEvent::Flap)
        .map(|&(pos, ..)| pos)
        .collect();
    assert_eq!(posted, flaps);
}
