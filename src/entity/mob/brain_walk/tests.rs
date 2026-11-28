//! v1.0 gap Track 2e: the brains' idle movement `RunOne`s and
//! `MoveToTargetSink`.
use super::super::head::Seen;
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

fn seen(uid: u32, kind: MobKind, at: DVec3) -> Seen {
    Seen { uid, kind, eye: at + DVec3::Y, pos: at, breedable: false }
}

/// The idle `RunOne` sends a villager to a villager it sees within 8
/// (`InteractWith`, speed 0.5, 2 blocks short) and looks at it; other picks
/// stroll, follow the look target or do nothing, never the old wandering.
#[test]
fn idle_villagers_walk_to_the_villagers_they_see() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let (mut met, mut strolled, mut waited) = (0, 0, 0);
    for seed in 0..60 {
        let mut rng = Rng::new(seed);
        let mut v = mob(MobKind::Villager);
        v.walking.in_village = true;
        v.head.seen = vec![seen(78, MobKind::Villager, DVec3::new(5.5, 64.0, 0.5))];
        v.head_goals(&w, &c, &mut rng, false);
        match v.walking.walk {
            Some(Walk { to: WalkTo::Mob(78, _), speed, close }) => {
                assert_eq!((speed, close), (0.5, 2));
                assert!(matches!(v.head.target, Some(Tracked::Mob(78, _))));
                // The core sink starts on the next tick.
                assert!(v.head.fresh_target);
                met += 1;
            }
            Some(Walk { to: WalkTo::Point(p), .. }) => {
                assert!((p - v.pos).with_y(0.0).abs().max_element() <= 10.5, "{p}");
                strolled += 1;
            }
            Some(other) => panic!("{other:?}"),
            None => waited += 1,
        }
        assert_ne!(v.ai, Ai::Wander);
    }
    assert!(met > 0 && strolled > 0 && waited > 0, "met {met}, strolled {strolled}, waited {waited}");
}

/// `MoveToTargetSink`: a reached walk target is erased (`distManhattan` to
/// the target block within `closeEnoughDist`), and a set one is walked.
#[test]
fn the_walk_sink_walks_then_erases_its_target() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut rng = Rng::new(3);
    let mut v = mob(MobKind::Villager);
    v.set_walk(Some(Walk { to: WalkTo::Point(DVec3::new(1.5, 64.0, 0.5)), speed: 0.5, close: 1 }));
    assert!(v.walk_sink(&w, &c, &mut rng, (150, 250)).is_none());
    assert!(v.walking.walk.is_none(), "one block off is close enough");
    v.set_walk(Some(Walk { to: WalkTo::Point(DVec3::new(8.5, 64.0, 0.5)), speed: 0.5, close: 0 }));
    let (dir, speed) = v.walk_sink(&w, &c, &mut rng, (150, 250)).expect("it walks");
    assert!(dir.is_some_and(|d| d.x > 0.5), "{dir:?}");
    assert!((speed - v.goal_speed(0.5)).abs() < 1e-9);
}

/// A piglin's idle movement: strolls within 10 blocks, walks to piglins it
/// sees, or waits; a celebrating piglin strolls only within 2.
#[test]
fn piglins_stroll_and_celebrate_close_by() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut kinds = (0, 0);
    for seed in 0..80 {
        let mut rng = Rng::new(seed);
        let mut p = mob(MobKind::Piglin);
        p.head.seen = vec![seen(90, MobKind::Piglin, DVec3::new(4.5, 64.0, 0.5))];
        for _ in 0..3 {
            p.head_goals(&w, &c, &mut rng, false);
        }
        match p.walking.walk {
            // `InteractWith` (2 short) or `SetWalkTargetFromLookTarget` (3).
            Some(Walk { to: WalkTo::Mob(90, _), speed, close }) => {
                assert!(speed == 0.6 && (2..=3).contains(&close), "{speed} {close}");
                kinds.0 += 1;
            }
            Some(Walk { to: WalkTo::Point(at), speed, .. }) => {
                assert_eq!(speed, 0.6);
                assert!((at - p.pos).with_y(0.0).abs().max_element() <= 10.5);
                kinds.1 += 1;
            }
            _ => {}
        }
        let mut q = mob(MobKind::Piglin);
        q.nether.as_mut().unwrap().celebrate_left = 10.0;
        for _ in 0..40 {
            q.head_goals(&w, &c, &mut rng, false);
            if let Some(Walk { to: WalkTo::Point(at), .. }) = q.walking.walk {
                assert!((at - q.pos).with_y(0.0).abs().max_element() <= 2.5, "{at}");
                q.walking.walk = None;
            }
        }
    }
    assert!(kinds.0 > 0 && kinds.1 > 0, "{kinds:?}");
}

/// The sniffer's idle `RunOne` starts its scent and its sniff (the sniff
/// waits out `SNIFF_COOLDOWN`; the scent doesn't) and holds while they run.
#[test]
fn the_sniffers_run_one_starts_scents_and_sniffs() {
    use crate::entity::sniffer::{SCENTING, SNIFFING};
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut rng = Rng::new(9);
    let (mut scents, mut sniffs) = (0, 0);
    for cooldown in [0, 9000] {
        let mut s = mob(MobKind::Sniffer);
        s.sniffer.as_mut().unwrap().cooldown = cooldown;
        for _ in 0..400 {
            s.head_goals(&w, &c, &mut rng, false);
            s.walking.walk = None;
            let st = s.sniffer.as_mut().unwrap();
            match st.pose {
                SCENTING => {
                    scents += 1;
                    st.stop_early();
                }
                SNIFFING => {
                    assert_eq!(cooldown, 0, "no sniff during the cooldown");
                    sniffs += 1;
                    st.stop_early();
                }
                _ => {}
            }
        }
    }
    assert!(scents > 0 && sniffs > 0, "scents {scents}, sniffs {sniffs}");
}

/// `JumpOnBed`: a baby villager at play near a bed walks to it and jumps on
/// it three to six times, five ticks apart.
#[test]
fn baby_villagers_jump_on_beds() {
    let mut w = Grid::flat(64);
    let bed = IVec3::new(0, 64, 0);
    w.set(bed, Block::colored_bed(crate::color::DyeColor::Blue, true));
    let c = ctx(&[]);
    let mut jumped = false;
    for seed in 0..60 {
        let mut rng = Rng::new(seed);
        let mut v = mob(MobKind::Villager);
        v.age = -24000;
        v.baby = true;
        v.villager.as_mut().unwrap().activity = crate::entity::villager::Activity::Play;
        v.walking.nearest_bed = Some(bed);
        let mut ticks = 0;
        while !matches!(v.walking.hold, Some(Hold::JumpOnBed(_))) && ticks < 400 {
            v.head_goals(&w, &c, &mut rng, false);
            v.walking.walk = None;
            ticks += 1;
        }
        let mut jumps = 0;
        while matches!(v.walking.hold, Some(Hold::JumpOnBed(_))) {
            v.head_goals(&w, &c, &mut rng, false);
            if std::mem::take(&mut v.jump) {
                jumps += 1;
            }
        }
        if jumps > 0 {
            assert!((3..=6).contains(&jumps), "{jumps}");
            jumped = true;
        }
    }
    assert!(jumped);
}
