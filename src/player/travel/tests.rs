//! Player movement against Java 26.3: tick-by-tick values derived from
//! `LivingEntity.travel`, `jumpFromGround` and `LocalPlayer.modifyInput`, and
//! the published Java speeds they produce (walking 4.317 m/s, sprinting
//! 5.612 m/s, sneaking 1.295 m/s, a 1.2522-block jump).

use glam::{DVec3, IVec3};

use super::super::{MoveInput, Player};
use super::modify_input;
use crate::entity::model::PlayerPose;
use crate::physics::test_util::Grid;
use crate::world::block::Block;

const FLOOR: i32 = 64;

fn ground() -> Grid {
    Grid::flat(FLOOR)
}

/// A player standing on the floor facing +X, settled onto the ground.
fn standing(world: &Grid) -> Player {
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64, 0.5));
    for _ in 0..3 {
        p.tick(MoveInput::default(), world);
    }
    assert!(p.on_ground, "settled on the floor");
    assert_eq!(p.pos.y, FLOOR as f64, "Java collision leaves the feet exactly on the block");
    p
}

fn forward() -> MoveInput {
    MoveInput { forward: 1.0, ..Default::default() }
}

/// Ticks `n` times and returns the last tick's displacement.
fn run(p: &mut Player, world: &Grid, input: MoveInput, n: usize) -> DVec3 {
    let mut last = DVec3::ZERO;
    for _ in 0..n {
        let before = p.pos;
        p.tick(input, world);
        last = p.pos - before;
    }
    last
}

fn near(a: f64, b: f64, tolerance: f64, what: &str) {
    assert!((a - b).abs() <= tolerance, "{what}: {a} vs {b}");
}

#[test]
fn walking_accelerates_like_java_and_tops_out_at_4_317_metres_a_second() {
    let world = ground();
    let mut p = standing(&world);
    // Tick 1: `0.1 * (0.21600002 / 0.6^3)` (floats) times the 0.98 key scale.
    let first = run(&mut p, &world, forward(), 1);
    let accel = (0.1_f32 * (0.21600002_f32 / (0.6_f32 * 0.6_f32 * 0.6_f32))) as f64 * 0.98_f32 as f64;
    near(first.x, accel, 1e-9, "first walking tick");
    // Tick 2 keeps 0.546 of it (`0.6 * 0.91` as floats).
    let second = run(&mut p, &world, forward(), 1);
    near(second.x, accel + accel * (0.6_f32 * 0.91_f32) as f64, 1e-9, "second walking tick");
    let top = run(&mut p, &world, forward(), 200);
    near(top.x * 20.0, 4.317, 0.001, "walking speed");
    assert!(top.z.abs() < 1e-12 && top.y == 0.0);
}

#[test]
fn sprinting_latches_and_sneaking_slows_to_java_speeds() {
    let world = ground();
    let mut p = standing(&world);
    let sprint = MoveInput { sprint: true, ..forward() };
    run(&mut p, &world, sprint, 1);
    assert!(p.sprinting);
    // Releasing the sprint key does not stop a Java sprint.
    let top = run(&mut p, &world, forward(), 200);
    assert!(p.sprinting, "sprint stays latched while moving forward");
    near(top.x * 20.0, 5.612, 0.001, "sprinting speed");
    // Letting go of forward ends it.
    run(&mut p, &world, MoveInput::default(), 1);
    assert!(!p.sprinting);

    let mut p = standing(&world);
    let sneak = MoveInput { descend: true, ..forward() };
    let top = run(&mut p, &world, sneak, 200);
    assert!(p.crouching && p.pose == PlayerPose::Crouching);
    near(top.x * 20.0, 1.295, 0.001, "sneaking speed");
}

#[test]
fn double_tapping_forward_starts_a_sprint() {
    let world = ground();
    let mut p = standing(&world);
    run(&mut p, &world, forward(), 1);
    run(&mut p, &world, MoveInput::default(), 2);
    assert!(!p.sprinting);
    run(&mut p, &world, forward(), 1);
    assert!(p.sprinting, "a second press inside the 7-tick window sprints");
}

#[test]
fn hunger_or_blindness_stops_a_sprint() {
    let world = ground();
    let mut p = standing(&world);
    run(&mut p, &world, MoveInput { sprint: true, ..forward() }, 5);
    assert!(p.sprinting);
    run(&mut p, &world, MoveInput { sprint_blocked: true, sprint: true, ..forward() }, 1);
    assert!(!p.sprinting);
}

#[test]
fn diagonal_keys_reach_full_input_length() {
    // `modifyInputSpeedForSquareMovement`: 0.98 straight, 1.0 diagonally.
    let (x, y) = modify_input(0.0, 1.0, false, false);
    assert!((x, y) == (0.0, 0.98));
    let d = std::f32::consts::FRAC_1_SQRT_2;
    let (x, y) = modify_input(d, d, false, false);
    near((x * x + y * y).sqrt() as f64, 1.0, 1e-6, "diagonal input length");
    // Sneaking scales before squaring: 0.98 * 0.3 straight.
    let (_, y) = modify_input(0.0, 1.0, false, true);
    near(y as f64, (0.98_f32 * 0.3_f32) as f64, 1e-9, "sneaking input");
    let (_, y) = modify_input(0.0, 1.0, true, false);
    near(y as f64, (0.98_f32 * 0.2_f32) as f64, 1e-9, "item use input");
}

#[test]
fn a_standing_jump_follows_javas_arc() {
    let world = ground();
    let mut p = standing(&world);
    let jump = MoveInput { jump: true, ..Default::default() };
    // Heights above the floor after each tick of a held jump (Java's
    // 0.42 take-off, 0.08 gravity, 0.98 drag).
    let expected = [
        0.41999998688697815,
        0.7531999805212017,
        1.0013359791121474,
        1.166109260938214,
        1.2491870787446813,
        1.2522033402537238,
    ];
    p.tick(jump, &world);
    assert!(p.jumped);
    near(p.pos.y - FLOOR as f64, expected[0], 1e-12, "tick 1");
    for (i, &h) in expected.iter().enumerate().skip(1) {
        p.tick(MoveInput::default(), &world);
        near(p.pos.y - FLOOR as f64, h, 1e-9, &format!("tick {}", i + 1));
    }
    let mut ticks = expected.len();
    while !p.on_ground {
        p.tick(MoveInput::default(), &world);
        ticks += 1;
    }
    assert_eq!(ticks, 12, "a jump lands on its twelfth tick");
    assert_eq!(p.pos.y, FLOOR as f64);
}

#[test]
fn holding_jump_waits_ten_ticks_between_jumps() {
    let world = ground();
    let mut p = standing(&world);
    let jump = MoveInput { jump: true, ..Default::default() };
    let mut takeoffs = Vec::new();
    for tick in 0..40 {
        p.tick(jump, &world);
        if p.jumped {
            takeoffs.push(tick);
        }
    }
    // Landing on tick 11 is inside the 10-tick `noJumpDelay`, so the next
    // jump waits for the ground, then for the delay.
    assert!(takeoffs.len() >= 3, "{takeoffs:?}");
    assert!(takeoffs.windows(2).all(|w| w[1] - w[0] >= 10), "{takeoffs:?}");
}

#[test]
fn a_sprint_jump_adds_a_fifth_of_a_block_forward() {
    let world = ground();
    let mut p = standing(&world);
    run(&mut p, &world, MoveInput { sprint: true, ..forward() }, 30);
    let before = p.vel.x / 20.0;
    p.tick(MoveInput { jump: true, ..forward() }, &world);
    // Ground friction applied to the pre-jump motion, plus the boost and
    // this tick's sprint acceleration, all spent on the move.
    assert!(p.jumped);
    let moved = p.vel.x / 20.0 / (0.6_f32 * 0.91_f32) as f64;
    assert!(moved > before + 0.2, "{moved} after {before}");
}

#[test]
fn steps_up_slabs_but_not_full_blocks() {
    let mut world = ground();
    world.set(IVec3::new(2, FLOOR, 0), Block::STONE_SLAB);
    let mut p = standing(&world);
    // Six ticks of walking reach the slab and step onto it in one move.
    run(&mut p, &world, forward(), 6);
    assert!(p.pos.y == FLOOR as f64 && p.pos.x < 1.7);
    run(&mut p, &world, forward(), 1);
    near(p.pos.y, FLOOR as f64 + 0.5, 1e-9, "stepped onto the slab");
    assert!(!p.horizontal_collision(), "the step carried the whole move");

    let mut world = ground();
    world.set(IVec3::new(2, FLOOR, 0), Block::STONE);
    let mut p = standing(&world);
    run(&mut p, &world, forward(), 30);
    assert_eq!(p.pos.y, FLOOR as f64);
    // Java's float width: half of `0.6f`.
    near(p.pos.x, 2.0 - super::super::HALF_WIDTH, 1e-12, "stopped flush against the block");
    assert!(p.horizontal_collision());
}

#[test]
fn sneaking_never_walks_off_an_edge() {
    let mut world = ground();
    // A pillar one block wide: stone at x = 0 only, a drop everywhere else.
    for x in -3..=3 {
        for z in -3..=3 {
            if (x, z) != (0, 0) {
                world.set(IVec3::new(x, FLOOR - 1, z), Block::AIR);
                world.set(IVec3::new(x, FLOOR - 2, z), Block::AIR);
            }
        }
    }
    let mut p = standing(&world);
    run(&mut p, &world, MoveInput { descend: true, ..forward() }, 60);
    assert!(p.on_ground);
    assert_eq!(p.pos.y, FLOOR as f64);
    // The box may overhang until only its edge is over the block.
    assert!(p.pos.x <= 1.0 + 0.3 + 1e-9, "{}", p.pos.x);
    assert!(p.pos.x > 1.2, "walks right up to the edge: {}", p.pos.x);
}

#[test]
fn ladders_climb_at_2_35_and_slide_at_3_metres_a_second() {
    let mut world = ground();
    for y in FLOOR..FLOOR + 20 {
        world.set(IVec3::new(0, y, 0), Block::LADDER);
    }
    let mut p = standing(&world);
    let jump = MoveInput { jump: true, ..Default::default() };
    let climb = run(&mut p, &world, jump, 40);
    // Climbing sets 0.2, then gravity and drag: (0.2 - 0.08) * 0.98 a tick.
    let rate = (0.2 - 0.08) * 0.98_f32 as f64;
    near(climb.y, rate, 1e-9, "climb displacement");
    near(climb.y * 20.0, 2.35, 0.01, "climb speed");
    near(p.vel.y / 20.0, rate, 1e-9, "climb motion");
    let slide = run(&mut p, &world, MoveInput::default(), 20);
    near(slide.y, -0.15_f32 as f64, 1e-9, "slide");
    // Sneaking holds on.
    run(&mut p, &world, MoveInput { descend: true, ..Default::default() }, 2);
    let held = run(&mut p, &world, MoveInput { descend: true, ..Default::default() }, 5);
    assert_eq!(held.y, 0.0);
    assert!(p.climbing);
}

#[test]
fn soul_sand_slows_walking_to_two_fifths() {
    let mut world = ground();
    for x in -2..40 {
        world.set(IVec3::new(x, FLOOR - 1, 0), Block::SOUL_SAND);
    }
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64, 0.5));
    for _ in 0..5 {
        p.tick(MoveInput::default(), &world);
    }
    // Soul sand's collision is 14/16 tall.
    near(p.pos.y, FLOOR as f64 - 0.125, 1e-9, "sunk into soul sand");
    let top = run(&mut p, &world, forward(), 200);
    // Each move is d = v + a for the walking acceleration a, and the motion
    // kept is v = 0.4 * 0.546 * d.
    let a = (0.1_f32 * (0.21600002_f32 / (0.6_f32 * 0.6_f32 * 0.6_f32))) as f64 * 0.98_f32 as f64;
    let f = (0.6_f32 * 0.91_f32) as f64;
    near(top.x, a / (1.0 - 0.4_f32 as f64 * f), 1e-9, "soul sand displacement");
}

#[test]
fn ice_keeps_momentum() {
    let mut world = ground();
    for x in -2..60 {
        world.set(IVec3::new(x, FLOOR - 1, 0), Block::ICE);
    }
    let mut p = standing(&world);
    run(&mut p, &world, forward(), 100);
    let mut glide = 0;
    while p.vel.x.abs() > 0.0 && glide < 400 {
        p.tick(MoveInput::default(), &world);
        glide += 1;
    }
    // `0.98 * 0.91` a tick takes walking speed under the 0.003 cut-off in
    // about 37 ticks; stone's `0.6 * 0.91` takes 7.
    assert!((35..=40).contains(&glide), "{glide}");
    let world = ground();
    let mut p = standing(&world);
    run(&mut p, &world, forward(), 100);
    let mut stop = 0;
    while p.vel.x.abs() > 0.0 {
        p.tick(MoveInput::default(), &world);
        stop += 1;
    }
    assert!(stop <= 8, "{stop}");
}

#[test]
fn cobwebs_hold_a_falling_player() {
    let mut world = ground();
    world.set(IVec3::new(0, FLOOR + 2, 0), Block::COBWEB);
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 4.0, 0.5));
    let mut ticks = 0;
    while !p.on_ground && ticks < 400 {
        p.tick(MoveInput::default(), &world);
        ticks += 1;
    }
    assert!(ticks > 40, "the web slows the fall to a crawl: {ticks} ticks");
}

#[test]
fn still_water_sinks_at_half_a_block_a_second() {
    let mut world = ground();
    for y in FLOOR..FLOOR + 40 {
        world.set(IVec3::new(0, y, 0), Block::WATER);
    }
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 30.0, 0.5));
    let sink = run(&mut p, &world, MoveInput::default(), 60);
    assert!(p.in_water);
    // y' = 0.8 y - 0.08 / 16.
    near(sink.y, -0.025, 1e-6, "sinking");
    // Holding jump adds 0.04 a tick before the move, and y' = 0.8 (y + 0.04)
    // - 0.005 after it, so each move rises y + 0.04 = 0.175.
    let rise = run(&mut p, &world, MoveInput { jump: true, ..Default::default() }, 40);
    near(rise.y, 0.175, 1e-4, "swimming up");
}

#[test]
fn flowing_water_pushes_the_player_downstream() {
    let mut world = ground();
    world.set(IVec3::new(0, FLOOR, 0), Block::WATER);
    for x in 1..=7 {
        world.set(IVec3::new(x, FLOOR, 0), Block(23 + x as u16));
        for z in [-1, 1] {
            world.set(IVec3::new(x, FLOOR, z), Block::STONE);
        }
    }
    let mut p = standing(&world);
    p.pos.x = 2.5;
    run(&mut p, &world, MoveInput::default(), 20);
    assert!(p.pos.x > 2.6, "carried along +X: {}", p.pos.x);
}

#[test]
fn a_one_block_gap_forces_a_slow_crawl() {
    let mut world = ground();
    for x in -2..20 {
        world.set(IVec3::new(x, FLOOR + 1, 0), Block::STONE);
    }
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64, 0.5));
    p.tick(MoveInput::default(), &world);
    assert_eq!(p.pose, PlayerPose::Swimming);
    let top = run(&mut p, &world, forward(), 200);
    assert!(p.pose.crawling(p.in_water));
    near(top.x * 20.0, 1.295, 0.001, "crawling speed");
}

#[test]
fn a_bed_bounces_a_falling_player_unless_sneaking() {
    let mut world = ground();
    world.set(IVec3::new(0, FLOOR, 0), Block::BED_FOOT);
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 5.0, 0.5));
    let mut bounced = false;
    for _ in 0..60 {
        let was_falling = p.vel.y < 0.0;
        p.tick(MoveInput::default(), &world);
        bounced |= was_falling && p.vel.y > 0.0;
    }
    assert!(bounced);

    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 5.0, 0.5));
    let sneak = MoveInput { descend: true, ..Default::default() };
    for _ in 0..60 {
        p.tick(sneak, &world);
        assert!(p.vel.y <= 0.0, "no bounce while sneaking");
    }
}

#[test]
fn creative_flight_moves_like_java() {
    let world = ground();
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 10.0, 0.5));
    p.can_fly = true;
    p.flying = true;
    let up = run(&mut p, &world, MoveInput { jump: true, ..Default::default() }, 60);
    // Each move is y + 0.15 with y' = 0.6 (y + 0.15): 0.375 a tick.
    near(up.y, 0.375, 1e-6, "flying up");
    run(&mut p, &world, MoveInput::default(), 60);
    let level = run(&mut p, &world, forward(), 200);
    // x' = 0.91 (x + 0.05 * 0.98).
    near(level.x, 0.05_f32 as f64 * 0.98_f32 as f64 / (1.0 - 0.91_f32 as f64), 1e-6, "flying speed");
}

#[test]
fn a_one_and_a_half_block_gap_forces_a_crouch_without_the_sneak_key() {
    // A bottom slab floor under a stone ceiling leaves exactly 1.5 blocks.
    let mut world = ground();
    for x in -2..20 {
        world.set(IVec3::new(x, FLOOR, 0), Block::STONE_SLAB);
        world.set(IVec3::new(x, FLOOR + 2, 0), Block::STONE);
    }
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 0.5, 0.5));
    p.tick(MoveInput::default(), &world);
    assert_eq!(p.pose, PlayerPose::Crouching, "standing doesn't fit, crouching does");
    // `LocalPlayer.crouching` follows on the next tick, and with it the 0.3 input.
    let top = run(&mut p, &world, forward(), 200);
    assert!(p.crouching);
    near(top.x * 20.0, 1.295, 0.001, "crouch-walking speed");
}

#[test]
fn sprinting_under_water_swims_with_the_low_pose() {
    let mut world = ground();
    for x in -2..40 {
        for y in FLOOR..FLOOR + 6 {
            world.set(IVec3::new(x, y, 0), Block::WATER);
        }
    }
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 + 2.0, 0.5));
    run(&mut p, &world, MoveInput::default(), 3);
    assert!(p.head_in_water(&world));
    run(&mut p, &world, MoveInput { sprint: true, ..forward() }, 3);
    assert!(p.sprinting && p.swimming);
    assert_eq!(p.pose, PlayerPose::Swimming);
    assert!(!p.pose.crawling(p.in_water), "swimming, not crawling");
    // Letting go of forward off the ground ends the swim sprint, and the
    // pose stands back up.
    run(&mut p, &world, MoveInput::default(), 2);
    assert!(!p.sprinting && !p.swimming);
    assert_eq!(p.pose, PlayerPose::Standing);
}

/// Java 26.3 Blocks.HONEY_BLOCK factors are applied to retained velocity,
/// not directly to displacement (steady walking therefore is not 4.317 * 0.4).
#[test]
fn honey_speed_and_jump_follow_java_factors() {
    let mut world = ground();
    for x in -2..80 {
        for z in -2..3 {
            world.set(IVec3::new(x, FLOOR - 1, z), Block::HONEY_BLOCK);
        }
    }
    let mut p = Player::new(DVec3::new(0.5, FLOOR as f64 - 0.0625, 0.5));
    run(&mut p, &world, MoveInput::default(), 3);
    assert!(p.on_ground && p.on_honey(&world));
    let first = run(&mut p, &world, forward(), 1);
    let drag = f64::from(0.6_f32 * 0.91_f32);
    near(p.vel.x / 20.0, first.x * f64::from(0.4_f32) * drag, 1e-9, "honey retained velocity");
    let top = run(&mut p, &world, forward(), 200);
    let acceleration = f64::from(0.1_f32 * (0.21600002_f32 / (0.6_f32 * 0.6_f32 * 0.6_f32))) * f64::from(0.98_f32);
    near(top.x, acceleration / (1.0 - f64::from(0.4_f32) * drag), 1e-8, "Java honey walking");
    let base = p.pos.y;
    p.vel = DVec3::ZERO;
    p.tick(MoveInput { jump: true, ..Default::default() }, &world);
    near(p.pos.y - base, f64::from(0.42_f32 * 0.5_f32), 1e-9, "honey jump impulse");
    let mut peak = p.pos.y;
    for _ in 0..20 {
        p.tick(MoveInput::default(), &world);
        peak = peak.max(p.pos.y);
    }
    near(peak - base, 0.3838519838586434, 1e-7, "honey jump height");
    assert!(p.on_ground);
}

#[test]
fn honey_wall_slides_at_java_post_gravity_speed_and_resets_fall_distance() {
    let mut world = ground();
    for y in FLOOR..FLOOR + 30 {
        world.set(IVec3::new(1, y, 0), Block::HONEY_BLOCK);
    }
    let mut p = Player::new(DVec3::new(0.7625, (FLOOR + 20) as f64, 0.5));
    p.vel = DVec3::new(0.0, -20.0, 0.0);
    for _ in 0..10 {
        p.tick(MoveInput::default(), &world);
        assert!(p.honey_sliding);
        near(p.vel.y, (-0.05 - 0.08) * f64::from(0.98_f32) * 20.0, 1e-12, "honey slide");
        assert_eq!(p.motion.fall_distance, 0.0);
    }
    p.pos.x = 0.5;
    p.tick(MoveInput::default(), &world);
    assert!(!p.honey_sliding, "moving away leaves the wall");
    assert!(p.vel.y < -2.548);
}

#[test]
fn honey_slide_thresholds_scale_horizontal_motion_and_honey_does_not_bounce() {
    let mut p = Player::new(DVec3::new(0.7625, 65.0, 0.5));
    p.motion.fall_distance = 20.0;
    let mut dm = DVec3::new(0.2, (-0.5 - 0.08) * f64::from(0.98_f32), 0.1);
    p.honey_slide(IVec3::new(1, 65, 0), &mut dm);
    near(dm.x, 0.02, 1e-12, "slide x reduction");
    near(dm.z, 0.01, 1e-12, "slide z reduction");
    assert!(p.honey_sliding);
    p.honey_sliding = false;
    dm.y = (-0.08 - 0.08) * f64::from(0.98_f32);
    p.honey_slide(IVec3::new(1, 65, 0), &mut dm);
    assert!(!p.honey_sliding, "old Y at threshold does not slide");
    let mut world = ground();
    world.set(IVec3::new(0, FLOOR - 1, 0), Block::HONEY_BLOCK);
    p = Player::new(DVec3::new(0.5, (FLOOR + 10) as f64, 0.5));
    for _ in 0..80 {
        p.tick(MoveInput::default(), &world);
        if p.on_ground {
            near(p.pos.y, FLOOR as f64 - 0.0625, 1e-12, "honey collision top");
            assert!(p.vel.y <= 0.0, "SUPPRESSES_BOUNCE");
            return;
        }
    }
    panic!("never landed on honey");
}
