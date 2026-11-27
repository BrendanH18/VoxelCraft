//! v1.0 gap Track 2: door breaking, the chase's goal scheduling, spear use
//! and the strider jockey's steering, against Java 26.3's goals.
use glam::{DVec3, IVec3};

use super::*;
use crate::entity::path::{Path, PathNode, PathType};
use crate::entity::{Entities, EntityEvent, MobSound, PlayerId, Target, illager::Cue};
use crate::physics::BlockSource;
use crate::physics::test_util::Grid;
use crate::simulation::difficulty::Difficulty;
use crate::simulation::lighting::SpawnLight;
use crate::world::block::{Block, Facing, Shaped};
use crate::world::terrain::Dimension;

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        players,
        daylight: 0.0,
        spawn_light: SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

/// A zombie that can break doors, pressed against the closed oak door in
/// cell (2, 0, 0)–(2, 1, 0) its path crosses.
fn zombie_at_door(difficulty: Difficulty) -> (Mob, Grid) {
    let mut world = Grid::flat(0);
    world.set(IVec3::new(2, 0, 0), Block::door(Facing::East, false, false));
    world.set(IVec3::new(2, 1, 0), Block::door(Facing::East, false, true));
    let mut m = Mob::new(MobKind::Zombie, DVec3::new(1.5, 0.0, 0.5), 0.0);
    m.attrs.can_break_doors = true;
    m.difficulty = difficulty;
    m.blocked = true;
    m.on_ground = true;
    let node = |x: i32, ty| PathNode { pos: IVec3::new(x, 0, 0), ty };
    m.nav.path = Some(Path {
        nodes: vec![node(1, PathType::Walkable), node(2, PathType::WalkableDoor), node(3, PathType::Walkable)],
        next: 1,
        target: IVec3::new(3, 0, 0),
        reached: true,
    });
    (m, world)
}

/// `BreakDoorGoal`: on Hard the zombie takes the door one above the path
/// node, shows Java's crack stages, knocks one tick in 20 on average and
/// removes the door at tick 240; on Normal, or without `mobGriefing`, it
/// never starts.
#[test]
fn zombies_break_doors_on_hard_with_cracks_and_knocks() {
    let (mut m, mut world) = zombie_at_door(Difficulty::Hard);
    let mut rng = Rng::new(7);
    let mut events = Vec::new();
    assert!(m.door_goal(&world, true, &mut rng, &mut events), "starts on a full update");
    assert_eq!(m.door_break.unwrap().cell, IVec3::new(2, 1, 0), "`doorPos` is the node's cell plus one");
    let mut broken_at = None;
    for tick in 2..=260u32 {
        let before = events.len();
        let running = m.door_goal(&world, tick % 2 == 0, &mut rng, &mut events);
        if let Some(goal) = m.door_break {
            assert_eq!(goal.progress, (goal.time as f32 / 240.0 * 10.0) as i32);
            if (1..240).contains(&goal.time) {
                let (cell, f) = m.door_crack().unwrap();
                assert_eq!(cell, IVec3::new(2, 1, 0));
                assert_eq!((f * 10.0) as i32, goal.progress, "crack stage");
            }
        }
        if events[before..]
            .iter()
            .any(|e| matches!(e, EntityEvent::DoorBroken { cell } if *cell == IVec3::new(2, 1, 0)))
        {
            broken_at = Some(tick);
            // What the world does with the event.
            world.set(IVec3::new(2, 0, 0), Block::AIR);
            world.set(IVec3::new(2, 1, 0), Block::AIR);
        }
        if !running {
            break;
        }
    }
    assert_eq!(broken_at, Some(240), "`breakTime == 240`");
    assert_eq!(m.door_break, None, "it stops past 240 on the next full update");
    let knocks = events
        .iter()
        .filter(|e| matches!(e, EntityEvent::Sound { sound: MobSound::Illager(Cue::DoorAttack), .. }))
        .count();
    assert!((4..=25).contains(&knocks), "about one tick in 20 knocks: {knocks}");

    for (difficulty, griefing) in [(Difficulty::Normal, true), (Difficulty::Easy, true), (Difficulty::Hard, false)] {
        let (mut m, world) = zombie_at_door(difficulty);
        m.griefing = griefing;
        assert!(!m.door_goal(&world, true, &mut rng, &mut Vec::new()), "{difficulty:?}, griefing {griefing}");
    }
    // Without `CanBreakDoors` there is no goal at all.
    let (mut m, world) = zombie_at_door(Difficulty::Hard);
    m.attrs.can_break_doors = false;
    assert!(!m.door_goal(&world, true, &mut rng, &mut Vec::new()));
}

/// `canContinueToUse`: an opened door, or the zombie 2 blocks from the
/// door cell's centre, stops the goal on the next full update.
#[test]
fn the_door_goal_stops_when_the_door_opens_or_the_zombie_leaves() {
    let mut rng = Rng::new(3);
    let (mut m, mut world) = zombie_at_door(Difficulty::Hard);
    assert!(m.door_goal(&world, true, &mut rng, &mut Vec::new()));
    world.set(IVec3::new(2, 1, 0), Block::door(Facing::East, true, true));
    assert!(m.door_goal(&world, false, &mut rng, &mut Vec::new()), "only full updates check");
    assert!(!m.door_goal(&world, true, &mut rng, &mut Vec::new()), "open");
    let (mut m, world) = zombie_at_door(Difficulty::Hard);
    assert!(m.door_goal(&world, true, &mut rng, &mut Vec::new()));
    m.pos = DVec3::new(-0.5, 0.0, 0.5);
    m.blocked = false;
    assert!(!m.door_goal(&world, true, &mut rng, &mut Vec::new()), "too far");
    assert!(matches!(world.block(IVec3::new(2, 1, 0)).unwrap().shaped(), Some(Shaped::Door { .. })));
}

/// A door breaker's navigation goes through closed wooden doors
/// (`setCanOpenDoors`); an ordinary zombie's doesn't.
#[test]
fn door_breakers_path_through_closed_doors() {
    let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    assert!(!m.caps(true).can_open_doors);
    m.attrs.can_break_doors = true;
    assert!(m.caps(true).can_open_doors);
    let mut husk = Mob::new(MobKind::Husk, DVec3::ZERO, 0.0);
    husk.attrs.can_break_doors = true;
    assert!(husk.caps(true).can_open_doors && husk.has_door_goal());
    assert!(!Mob::new(MobKind::Skeleton, DVec3::ZERO, 0.0).has_door_goal());
}

/// A zombie chasing a player behind a wooden door on Hard walks into it
/// and breaks it within the goal's 240 ticks.
#[test]
fn a_hard_zombie_breaks_through_to_its_target() {
    let mut world = Grid::flat(0);
    for z in -6..=6 {
        for y in 0..3 {
            world.set(IVec3::new(4, y, z), Block::STONE);
        }
    }
    world.set(IVec3::new(4, 0, 0), Block::door(Facing::West, false, false));
    world.set(IVec3::new(4, 1, 0), Block::door(Facing::West, false, true));
    let mut e = Entities::new(11);
    e.spawn(MobKind::Zombie, DVec3::new(1.5, 0.0, 0.5));
    e.mobs[0].attrs.can_break_doors = true;
    e.mobs[0].persistent = true;
    // It can't see the player through the door, so it is sent after them
    // (a reinforcement's `setTarget`), as one already chasing would be.
    e.mobs[0].reinforce_target = Some(PlayerId(1));
    let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(7.5, 0.0, 0.5), true)]);
    let mut broke = None;
    for tick in 0..900 {
        let events = e.update_difficulty(0.05, &world, &c, Difficulty::Hard);
        if events.iter().any(|ev| matches!(ev, EntityEvent::DoorBroken { cell } if cell.x == 4)) {
            broke = Some(tick);
            break;
        }
    }
    assert!(broke.is_some(), "the door was broken (zombie at {:?})", e.mobs[0].pos);
}

/// `NearestAttackableTargetGoal` and `TargetGoal`: the search runs only on
/// full updates and one in five of those, and the chosen player is kept
/// when a nearer one appears.
#[test]
fn targets_are_found_on_javas_schedule_and_kept() {
    let world = Grid::flat(0);
    let mut first = Vec::new();
    for seed in 0..40 {
        let mut e = Entities::new(seed);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5));
        e.mobs[0].persistent = true;
        let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(8.5, 0.0, 0.5), true)]);
        let found = (0..200).position(|_| {
            e.update(0.05, &world, &c);
            e.mobs[0].chase_target.is_some()
        });
        first.push(found.expect("found within 200 ticks"));
    }
    let mean = first.iter().sum::<usize>() as f64 / first.len() as f64;
    // One full update in two ticks, one in five of those: about 10 ticks.
    assert!((4.0..20.0).contains(&mean), "mean wait {mean}");
    assert!(first.iter().any(|&t| t > 2), "not every tick");

    let mut e = Entities::new(5);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[0].persistent = true;
    let far = Target::new(PlayerId(1), DVec3::new(12.5, 0.0, 0.5), true);
    let c = ctx(vec![far]);
    while e.mobs[0].chase_target.is_none() {
        e.update(0.05, &world, &c);
    }
    let near = Target::new(PlayerId(2), DVec3::new(2.5, 0.0, 2.5), true);
    let c = ctx(vec![far, near]);
    for _ in 0..40 {
        e.update(0.05, &world, &c);
    }
    assert_eq!(e.mobs[0].chase_target, Some(PlayerId(1)), "the first target is kept");
}

/// `TargetGoal.canContinueToUse`: a target out of sight for more than
/// `reducedTickDelay(60)` full updates is dropped (about 62 ticks).
#[test]
fn an_unseen_target_is_dropped_after_sixty_ticks() {
    let mut world = Grid::flat(0);
    let mut e = Entities::new(9);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[0].persistent = true;
    let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(6.5, 0.0, 0.5), true)]);
    while e.mobs[0].chase_target.is_none() {
        e.update(0.05, &world, &c);
    }
    // Box the zombie in so it neither sees nor reaches the player.
    let at = e.mobs[0].pos.floor().as_ivec3();
    for dx in -1..=1 {
        for dz in -1..=1 {
            for y in 0..4 {
                if dx != 0 || dz != 0 || y >= 2 {
                    world.set(at + IVec3::new(dx, y, dz), Block::STONE);
                }
            }
        }
    }
    let kept = (0..200).take_while(|_| {
        e.update(0.05, &world, &c);
        e.mobs[0].chase_target.is_some()
    });
    let n = kept.count();
    assert!((58..=66).contains(&n), "dropped after {n} ticks");
}

/// `MeleeAttackGoal`: in reach and in sight it hits at once when the goal
/// starts, then every 20 ticks.
#[test]
fn melee_hits_every_twenty_ticks() {
    let world = Grid::flat(0);
    let mut e = Entities::new(2);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[0].persistent = true;
    let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(1.6, 0.0, 0.5), true)]);
    let mut hits = Vec::new();
    for tick in 0..120 {
        let events = e.update(0.05, &world, &c);
        if events.iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { .. })) {
            hits.push(tick);
        }
    }
    assert!(hits.len() >= 3, "{hits:?}");
    assert!(hits.windows(2).all(|w| w[1] - w[0] >= 20), "{hits:?}");
}

/// The zombie goal table: a held spear gives `SpearUseGoal` (priority 2)
/// the move and look flags over `ZombieAttackGoal` (3); it lowers the spear
/// within 10 blocks for `reducedTickDelay(delay + damage window)` updates.
#[test]
fn spear_zombies_use_the_spear_goal() {
    let world = Grid::flat(0);
    let mut e = Entities::new(4);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[0].persistent = true;
    e.mobs[0].equipment.set(
        crate::entity::equipment::Slot::MainHand,
        Some(crate::inventory::Stack::new(crate::weapons::spear(crate::item::Tier::Iron), 1)),
    );
    let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(14.5, 0.0, 0.5), true)]);
    let mut lowered = None;
    for tick in 0..400 {
        e.update(0.05, &world, &c);
        let m = &e.mobs[0];
        if m.chase_target.is_some() {
            assert!(!m.melee.running, "the spear goal holds MOVE and LOOK");
        }
        if lowered.is_none() && m.spear_use.is_some() {
            lowered = Some((tick, m.pos.distance(DVec3::new(14.5, 0.0, 0.5))));
        }
    }
    let (_, dist) = lowered.expect("the spear was lowered");
    assert!(dist <= SPEAR_APPROACH + 0.5, "lowered within 10 blocks: {dist}");
    let iron = spear::kinetic(crate::item::Tier::Iron);
    assert_eq!(selector::reduced((iron.delay_ticks + iron.damage.max_ticks) as i32), 119);
}

/// `KineticWeapon.damageEntities` for a mob: a lowered iron spear charging
/// at 4 blocks a second into a standing player hits for the base 3 plus
/// `floor(4 × 0.95)`, once per 10-tick contact cooldown, and not before
/// the 12-tick delay.
#[test]
fn a_charging_spear_stabs_the_player_in_front() {
    let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5), 0.0);
    m.equipment.set(
        crate::entity::equipment::Slot::MainHand,
        Some(crate::inventory::Stack::new(crate::weapons::spear(crate::item::Tier::Iron), 1)),
    );
    m.chase_target = Some(PlayerId(1));
    m.vel = DVec3::new(4.0, 0.0, 0.0);
    m.spear_use = Some(SpearCharge::default());
    let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(2.0, 0.0, 0.5), true)]);
    let mut hits = Vec::new();
    for tick in 0..40u32 {
        m.goals.ticks = tick;
        let mut events = Vec::new();
        m.spear_charge_tick(&c, &mut events);
        for ev in events {
            if let EntityEvent::PlayerHit { damage, .. } = ev {
                hits.push((tick, damage));
            }
        }
    }
    assert_eq!(hits.first().map(|h| h.0), Some(12), "after the delay");
    assert!(hits.iter().all(|&(_, d)| d == 3.0 + 3.0), "{hits:?}");
    assert!(hits.windows(2).all(|w| w[1].0 - w[0].0 >= 10), "{hits:?}");
}

/// `LandRandomPos.getPosAway`: spots lie away from the avoided point,
/// within the requested ring.
#[test]
fn land_positions_away_lie_beyond_the_target() {
    let world = Grid::flat(0);
    let m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5), 0.0);
    let c = ctx(Vec::new());
    let mut rng = Rng::new(1);
    // Java's ring (`lerp(√r, 6, 7) × √2`) must fit inside the 7-block
    // square and land on a stable cell, so many tries find nothing, as in
    // Java (the goal then keeps charging).
    let found: Vec<DVec3> = (0..200)
        .filter_map(|_| m.land_pos_away(&world, &c, &mut rng, 6.0, 7.0, 7, DVec3::new(-3.0, 0.0, 0.5)))
        .collect();
    assert!(found.len() > 20, "{} found", found.len());
    for p in found {
        assert!(p.x > 0.5, "away from the target: {p:?}");
        let flat = (p - m.pos).with_y(0.0);
        assert!(flat.x.abs() <= 8.0 && flat.z.abs() <= 8.0, "{p:?}");
        assert!(flat.length() >= 6.0 * std::f64::consts::SQRT_2 - 1.5, "{p:?}");
    }
}

/// A strider jockey: the zombified piglin controls the strider, which stops
/// running its own goals and goes for the piglin's target.
#[test]
fn a_zombified_piglin_steers_its_strider() {
    let world = Grid::flat(0);
    let mut e = Entities::new(8);
    e.monster_spawning = true;
    e.spawn(MobKind::Strider, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[0].baby = false;
    e.spawn_strider_jockey(0, MobKind::ZombifiedPiglin);
    let player = Target::new(PlayerId(1), DVec3::new(10.5, 0.0, 0.5), true);
    let c = ctx(vec![player]);
    e.update(0.05, &world, &c);
    assert!(e.mobs[0].mob_steered, "the piglin controls");
    assert_eq!(e.mobs[0].hunt, None, "a calm piglin strolls");
    // Angered, the piglin's `ZombieAttackGoal` walks the strider at the player.
    e.mobs[1].provoked = 20.0;
    e.mobs[1].angry_player = Some(PlayerId(1));
    let start = e.mobs[0].pos;
    for _ in 0..60 {
        e.update(0.05, &world, &c);
    }
    assert_eq!(e.mobs[0].hunt, Some(player.pos), "the strider heads for the piglin's target");
    assert!(e.mobs[0].pos.x > start.x + 0.5, "it moved toward the player: {:?}", e.mobs[0].pos);
    // A baby strider rider leaves the adult to its own goals.
    let mut e = Entities::new(8);
    e.spawn(MobKind::Strider, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[0].baby = false;
    e.spawn_strider_jockey(0, MobKind::Strider);
    e.update(0.05, &world, &c);
    assert!(!e.mobs[0].mob_steered);
}
