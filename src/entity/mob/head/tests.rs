//! v1.0 gap Track 2c: Java's look and body controls and look goals.
use glam::DVec3;

use super::super::{Mob, MobKind};
use super::*;
use crate::entity::{Ctx, PlayerId, Rng, Target};
use crate::physics::test_util::Grid;

fn ctx(players: &[DVec3]) -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players: players.iter().enumerate().map(|(i, &p)| Target::new(PlayerId(i as u32 + 1), p, true)).collect(),
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn mob(kind: MobKind) -> Mob {
    let mut m = Mob::new(kind, DVec3::new(0.5, 64.0, 0.5), 0.0);
    m.on_ground = true;
    m
}

/// The head relative to the body, degrees.
fn rel(m: &Mob) -> f32 {
    wrap_degrees(m.look.head - m.yrot())
}

/// `LookControl.tick`: a request turns the head by its speed for two ticks,
/// then it turns back by 10° a tick; the pitch restarts level each tick.
#[test]
fn the_look_control_turns_by_its_limits_then_back_to_the_body() {
    let mut m = mob(MobKind::Cow);
    let body = m.yrot();
    m.look_ready();
    // A point 90° to the left at eye height.
    let side = super::super::spear::view_vector(0.0, body + 90.0);
    let at = m.look_eye() + side * 4.0;
    m.look_toward(at);
    m.look_tick();
    assert!((rel(&m) - 10.0).abs() < 1e-3, "{}", rel(&m));
    m.look_tick();
    assert!((rel(&m) - 20.0).abs() < 1e-3, "{}", rel(&m));
    // The request has run out: back toward the body.
    m.look_tick();
    assert!((rel(&m) - 10.0).abs() < 1e-3, "{}", rel(&m));
    // A frog's head turns 35° a tick, at most 5° off the body while it
    // walks a path; a cow's at most 75°.
    let mut f = mob(MobKind::Frog);
    f.look_ready();
    let fb = f.yrot();
    let at = f.look_eye() + super::super::spear::view_vector(0.0, fb + 90.0) * 4.0;
    f.look_toward(at);
    f.look_tick();
    assert!((rel(&f) - 35.0).abs() < 1e-3, "{}", rel(&f));
    // Pitch: a point 45° below, limited to 40° (`getMaxHeadXRot`).
    let mut c = mob(MobKind::Cow);
    let below = c.look_eye() + super::super::spear::view_vector(45.0, c.yrot()) * 3.0;
    c.look_toward(below);
    c.look_tick();
    assert!((c.look.pitch - 40.0).abs() < 1e-3, "{}", c.look.pitch);
    c.look_tick();
    c.look_tick();
    assert_eq!(c.look.pitch, 0.0, "level once the request ends");
}

/// `BodyRotationControl`: a still body keeps within 75° of a head that
/// moved, then turns to face it from the 11th to the 20th still tick; a
/// moving body keeps the head within 75° of its heading.
#[test]
fn the_body_follows_a_still_head_and_holds_it_while_moving() {
    let mut m = mob(MobKind::Zombie);
    m.look_ready();
    let body = m.yrot();
    m.head.last_stable = m.look.head;
    m.look.head = body + 100.0;
    m.body_tick(DVec3::ZERO);
    assert!((wrap_degrees(m.yrot() - body) - 25.0).abs() < 1e-3, "pulled within 75°: {}", m.yrot() - body);
    for _ in 0..10 {
        m.body_tick(DVec3::ZERO);
    }
    assert!((wrap_degrees(m.yrot() - body) - 25.0).abs() < 1e-3, "settling: {}", m.yrot() - body);
    for _ in 0..10 {
        m.body_tick(DVec3::ZERO);
    }
    assert!(rel(&m).abs() < 1e-2, "faces the head: {}", rel(&m));
    // The drawn head follows.
    assert!(m.head_yaw.abs() < 1e-3);
    // Moving: the head is held within 75° of the body.
    m.look.head = m.yrot() - 120.0;
    m.body_tick(DVec3::X * 0.2);
    assert!((rel(&m) + 75.0).abs() < 1e-3, "{}", rel(&m));
    // Carrying a mob passenger, a still body doesn't turn.
    let mut s = mob(MobKind::Strider);
    s.look_ready();
    let sb = s.yrot();
    s.head.carrying_mob = true;
    s.look.head = sb + 100.0;
    for _ in 0..30 {
        s.body_tick(DVec3::ZERO);
    }
    assert_eq!(s.yrot(), sb);
}

/// `LookAtPlayerGoal(Player, 6)` at the default 0.02 chance per full
/// update: a cow looks at a player 5 blocks off for 20 to 39 full updates,
/// and never at one 7 blocks off.
#[test]
fn a_cow_looks_at_a_nearby_player_for_javas_time() {
    let w = Grid::flat(64);
    // Off to the side (the cow faces +X).
    let near = ctx(&[DVec3::new(0.5, 64.0, 5.5)]);
    let far = ctx(&[DVec3::new(0.5, 64.0, 7.5)]);
    for seed in 0..10 {
        let mut rng = Rng::new(seed);
        let mut m = mob(MobKind::Cow);
        for _ in 0..400 {
            m.head_goals(&w, &far, &mut rng, true);
            assert!(!matches!(m.head.running, Some((_, Running::LookAt { .. }))), "too far");
        }
        let mut ticks = 0;
        while !matches!(m.head.running, Some((_, Running::LookAt { .. }))) {
            m.head_goals(&w, &near, &mut rng, true);
            ticks += 1;
            assert!(ticks < 5000, "seed {seed}");
        }
        assert!(m.look.looking(), "it asks the look control");
        let yaw = yaw_toward(DVec3::new(0.0, 0.0, 5.0)).unwrap();
        m.look_tick();
        assert!(wrap_degrees(m.look.head - yaw).abs() < wrap_degrees(m.yrot() - yaw).abs());
        let mut held = 0;
        while matches!(m.head.running, Some((_, Running::LookAt { .. }))) {
            m.head_goals(&w, &near, &mut rng, true);
            held += 1;
            assert!(held < 200);
        }
        // `adjustedTickDelay(40 + nextInt(40))` full updates, every other tick.
        assert!((38..=84).contains(&held), "seed {seed}: {held}");
    }
}

/// `RandomLookAroundGoal` holds MOVE: it never starts while a better goal
/// moves the mob, starts at 2% a full update when it stands, and an
/// attack goal (LOOK) stops it.
#[test]
fn random_looks_wait_for_a_still_mob_and_yield_to_attacks() {
    let w = Grid::flat(64);
    let none = ctx(&[]);
    let mut rng = Rng::new(3);
    let mut m = mob(MobKind::Sheep);
    for _ in 0..2000 {
        m.head_goals(&w, &none, &mut rng, true);
        assert!(m.head.running.is_none());
    }
    let mut ticks = 0;
    while !matches!(m.head.running, Some((_, Running::Random { .. }))) {
        m.head_goals(&w, &none, &mut rng, false);
        ticks += 1;
        assert!(ticks < 5000);
    }
    assert!(m.look.looking());
    // Something else asks the look control (an attack): the random look stops.
    let at = m.look_eye() + DVec3::X;
    m.set_look_at(at, ATTACK_LOOK, ATTACK_LOOK);
    m.head_goals(&w, &none, &mut rng, false);
    assert!(m.head.running.is_none());
}

/// `Mob.updateControlFlags`: a strider carrying a zombified piglin loses
/// LOOK, so its look goals stop; the rider keeps its own.
#[test]
fn a_mob_steered_strider_stops_its_look_goals_and_the_rider_keeps_its_own() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(3.5, 64.0, 0.5)]);
    let mut rng = Rng::new(9);
    let mut s = mob(MobKind::Strider);
    s.head.carrying_mob = true;
    let mut rider = mob(MobKind::ZombifiedPiglin);
    rider.riding = Some(s.uid | crate::entity::mounts::MOB_VEHICLE);
    let mut looked = false;
    for _ in 0..3000 {
        s.head_goals(&w, &near, &mut rng, false);
        assert!(s.head.running.is_none());
        rider.head_goals(&w, &near, &mut rng, false);
        looked |= rider.head.running.is_some();
    }
    assert!(looked, "the rider's LookAtPlayerGoal and RandomLookAroundGoal run");
}

/// `SetEntityLookTargetSometimes(PLAYER, 6, 30..60)` and
/// `LookAtTargetSink(45, 90)`: a goat looks at a player within 6 blocks
/// once its ticker comes round, for 46 to 91 ticks.
#[test]
fn a_goat_looks_at_a_player_sometimes_for_the_sinks_time() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(4.5, 64.0, 0.5)]);
    for seed in 0..10 {
        let mut rng = Rng::new(seed);
        let mut m = mob(MobKind::Goat);
        let mut ticks = 0;
        while m.head.target.is_none() {
            m.head_goals(&w, &near, &mut rng, false);
            ticks += 1;
            assert!(ticks <= 60, "seed {seed}");
        }
        assert!(ticks >= 30, "seed {seed}: {ticks}");
        let mut held = 0;
        while m.head.target.is_some() {
            m.head_goals(&w, &near, &mut rng, false);
            held += 1;
            assert!(held < 200);
        }
        assert!((46..=92).contains(&held), "seed {seed}: {held}");
        // Goats turn their heads at most 15° off the body while walking.
        assert_eq!(m.max_head_y_rot(), 15.0);
    }
}

/// The riding baby piglin (`initRideHoglinActivity`): each tick a shuffled
/// one-shot picks a player, a piglin or anything within 8 blocks, or
/// nothing; an idle piglin's `RunOne` may instead do nothing for 30 to 60.
#[test]
fn a_riding_baby_piglin_looks_around_from_its_hoglin() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(4.5, 64.0, 0.5)]);
    let mut rng = Rng::new(5);
    let mut p = mob(MobKind::Piglin);
    p.baby = true;
    let hoglin = mob(MobKind::Hoglin);
    p.riding = Some(hoglin.uid | crate::entity::mounts::MOB_VEHICLE);
    let mut ticks = 0;
    while p.head.target.is_none() {
        p.head_goals(&w, &near, &mut rng, false);
        ticks += 1;
        assert!(ticks < 20, "a one-shot fires most ticks: {ticks}");
    }
    assert!(matches!(p.head.target, Some(Tracked::Player(_))));
}

/// A villager's full look behaviour weighs cats 8 to players' 2: with a
/// cat in sight it mostly looks at the cat.
#[test]
fn villagers_mostly_look_at_cats() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(3.5, 64.0, 0.5)]);
    let (mut cats, mut players) = (0, 0);
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut v = mob(MobKind::Villager);
        let cat = mob(MobKind::Cat);
        v.head.seen = vec![Seen {
            uid: 77,
            kind: MobKind::Cat,
            eye: cat.look_eye() + DVec3::X * 2.0,
            pos: DVec3::new(2.5, 64.0, 0.5),
        }];
        let mut ticks = 0;
        while v.head.target.is_none() && ticks < 200 {
            v.head_goals(&w, &near, &mut rng, false);
            ticks += 1;
        }
        match v.head.target {
            Some(Tracked::Mob(77, _)) => cats += 1,
            Some(Tracked::Player(_)) => players += 1,
            _ => {}
        }
    }
    assert!(cats > players * 2, "cats {cats}, players {players}");
}

/// `SmoothSwimmingLookControl(10)`: a dolphin whose head is more than 10°
/// off its body swings the body 4° a tick toward it.
#[test]
fn a_smooth_swimmer_swings_its_body_toward_its_head() {
    let mut d = mob(MobKind::Dolphin);
    d.look_ready();
    let body = d.yrot();
    d.look.head = body + 30.0;
    d.look_tick();
    assert!((wrap_degrees(d.yrot() - body) - 4.0).abs() < 1e-3, "{}", d.yrot() - body);
}
