//! Java 26.3 movement branches and literal stuck multipliers.
use super::*;
use crate::{physics::test_util::Grid, world::scaffolding as sc};
#[test]
fn scaffolding_sneak_descends_where_ladders_hold() {
    let mut w = Grid::flat(0);
    let mut p = Player::new(DVec3::new(0.5, 0.5, 0.5));
    p.sneaking = true;
    w.set(IVec3::ZERO, sc::state(0, false, false));
    assert_eq!(
        p.handle_on_climbable(&w, DVec3::new(1.0, -1.0, 1.0)),
        DVec3::splat(f64::from(0.15_f32)).with_y(-f64::from(0.15_f32))
    );
    w.set(IVec3::ZERO, Block::LADDER);
    assert_eq!(p.handle_on_climbable(&w, DVec3::NEG_Y).y, 0.0);
}
#[test]
fn powder_snow_stuck_is_feet_only_and_resets_fall_distance() {
    let mut w = Grid::flat(-10);
    w.set(IVec3::ZERO, sc::POWDER_SNOW);
    let mut p = Player::new(DVec3::new(0.5, 0.5, 0.5));
    p.motion.fall_distance = 9.0;
    p.apply_effects_from_blocks(&w, &mut p.vel.clone());
    assert!(p.in_powder_snow);
    assert_eq!(p.motion.fall_distance, 0.0);
    assert_eq!(p.motion.stuck, DVec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32)));
    let before = p.pos;
    let mut dm = DVec3::new(0.1, -0.08, 0.1);
    p.move_entity(&w, dm, &mut dm);
    assert!((p.pos - before - DVec3::new(0.1 * f64::from(0.9_f32), -0.12, 0.1 * f64::from(0.9_f32))).length() < 1e-12);
    assert_eq!(dm, DVec3::ZERO);
    let mut p = Player::new(DVec3::new(1.1, 0.5, 0.5));
    p.apply_effects_from_blocks(&w, &mut p.vel.clone());
    assert!(p.in_powder_snow, "body contact freezes");
    assert_eq!(p.motion.stuck, DVec3::ZERO, "feet outside do not slow living entities");
}
#[test]
fn powder_snow_boots_walk_then_sneak_sinks_and_jump_climbs_out() {
    let mut w = Grid::flat(-10);
    w.set(IVec3::ZERO, sc::POWDER_SNOW);
    let mut boots = Player::new(DVec3::new(0.5, 1.0, 0.5));
    boots.leather_boots = true;
    for _ in 0..10 {
        boots.tick(MoveInput::default(), &w);
    }
    assert_eq!(boots.pos.y, 1.0);
    assert!(boots.on_ground);
    for _ in 0..5 {
        boots.tick(MoveInput { descend: true, ..Default::default() }, &w);
    }
    assert!(boots.pos.y < 1.0);
    assert!(boots.in_powder_snow);
    // The exact `wasInPowderSnow && canEntityWalkOnPowderSnow` branch after move.
    boots.was_in_powder_snow = true;
    let mut dm = DVec3::ZERO;
    assert_eq!(boots.relative_friction_movement(&w, DVec3::ZERO, 0.6, true, &mut dm).y, 0.2);
    boots.leather_boots = false;
    dm = DVec3::ZERO;
    assert_ne!(boots.relative_friction_movement(&w, DVec3::ZERO, 0.6, true, &mut dm).y, 0.2);
}
#[test]
fn powder_snow_long_fall_stops_at_nine_tenths_then_sinks_without_damage() {
    let mut w = Grid::flat(-10);
    w.set(IVec3::ZERO, sc::POWDER_SNOW);
    let mut p = Player::new(DVec3::new(0.5, 2.0, 0.5));
    p.motion.fall_distance = 3.0;
    let mut dm = DVec3::new(0.0, -2.0, 0.0);
    p.move_entity(&w, dm, &mut dm);
    assert_eq!(p.pos.y, f64::from(0.9_f32));
    assert!(p.on_ground);
    p.apply_effects_from_blocks(&w, &mut p.vel.clone());
    assert_eq!(p.motion.fall_distance, 0.0);
    for _ in 0..5 {
        p.tick(MoveInput::default(), &w);
    }
    assert!(p.pos.y < f64::from(0.9_f32));
}

#[test]
fn leather_boots_only_walk_from_feet_slot_and_any_leather_piece_prevents_freeze() {
    use crate::{
        inventory::Stack,
        item::{ArmorMaterial, ArmorPiece, Item},
    };
    let mut p = Player::new(DVec3::ZERO);
    for (slot, piece) in ArmorPiece::ALL.into_iter().enumerate() {
        let mut armor = [None; 4];
        armor[slot] = Some(Stack::new(Item::armor(piece, ArmorMaterial::Leather), 1));
        p.powder_snow_equipment(&armor, 140);
        assert!(p.freeze_immune);
        assert_eq!(p.leather_boots, piece == ArmorPiece::Boots);
        assert_eq!(p.movement_speed(), 0.05);
    }
    let mut armor = [None; 4];
    armor[0] = Some(Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather), 1));
    p.powder_snow_equipment(&armor, 0);
    assert!(!p.leather_boots);
}

/// Java `ScaffoldingBlock.getCollisionShape` through whole player ticks: the
/// top plate holds a standing player, sneaking drops through it, the sides are
/// passable from inside, and the column is climbed by jumping.
#[test]
fn scaffolding_holds_from_above_lets_sneakers_through_and_climbs() {
    let mut w = Grid::flat(0);
    for y in 0..3 {
        w.set(IVec3::new(0, y, 0), sc::state(0, false, false));
    }
    let mut p = Player::new(DVec3::new(0.5, 3.4, 0.5));
    for _ in 0..30 {
        p.tick(MoveInput::default(), &w);
    }
    assert!(p.on_ground);
    assert_eq!(p.pos.y, 3.0, "stands on the top plate");
    for _ in 0..90 {
        p.tick(MoveInput { descend: true, ..Default::default() }, &w);
    }
    assert!(p.on_ground);
    assert_eq!(p.pos.y, 0.0, "sneaking descends through the column to the floor");
    assert!(p.climbing);
    let mut before = p.pos.y;
    for _ in 0..8 {
        p.tick(MoveInput { jump: true, ..Default::default() }, &w);
        assert!(p.pos.y > before, "jumping climbs scaffolding");
        before = p.pos.y;
    }
    for _ in 0..40 {
        if p.pos.y >= 3.0 {
            break;
        }
        p.tick(MoveInput { jump: true, ..Default::default() }, &w);
    }
    assert!(p.pos.y >= 3.0, "climbs out over the plate: {}", p.pos.y);
    for _ in 0..30 {
        p.tick(MoveInput::default(), &w);
    }
    assert_eq!((p.pos.y, p.on_ground), (3.0, true), "and stands on it");
    // From the side at floor level, the unsupported-from-above shape is empty.
    let mut side = Player::new(DVec3::new(-1.0, 0.0, 0.5));
    for _ in 0..12 {
        side.tick(MoveInput { forward: 1.0, ..Default::default() }, &w);
    }
    assert!(side.pos.x > 0.5, "walked into the column: {}", side.pos);
}

#[test]
fn powder_snow_steady_sink_speed_is_gravity_times_one_and_a_half() {
    let mut w = Grid::flat(-10);
    for y in -4..=5 {
        w.set(IVec3::new(0, y, 0), sc::POWDER_SNOW);
    }
    let mut p = Player::new(DVec3::new(0.5, 5.2, 0.5));
    for _ in 0..4 {
        p.tick(MoveInput::default(), &w);
    }
    let before = p.pos.y;
    p.tick(MoveInput::default(), &w);
    // Each tick's movement is `(-0.08 * 0.98) * 1.5`: the stuck multiplier
    // also zeroes the carried motion.
    let expected = -0.08 * 0.98_f32 as f64 * 1.5;
    assert!((p.pos.y - before - expected).abs() < 1e-9, "{}", p.pos.y - before);
    assert!(p.in_powder_snow && p.motion.fall_distance == 0.0);
}
#[test]
fn a_riding_player_refreshes_powder_snow_contact() {
    // Riding skips `Player::tick`; Java's rider still runs `baseTick`.
    let mut w = Grid::flat(-10);
    w.set(IVec3::ZERO, sc::POWDER_SNOW);
    let mut p = Player::new(DVec3::new(0.5, 0.2, 0.5));
    p.refresh_riding_contact(&w);
    assert!(p.in_powder_snow);
    p.pos.x = 5.5;
    p.refresh_riding_contact(&w);
    assert!(!p.in_powder_snow && p.was_in_powder_snow);
}
#[test]
fn redstone_stands_on_sturdy_tops_like_java() {
    use crate::world::redstone_blocks::{self as r, HOPPER, WIRE};
    // `RedStoneWireBlock.canSurviveOn`: a sturdy top (scaffolding's planks,
    // a top slab, glass) or a hopper, but not leaves or a bottom slab.
    for below in [Block::STONE, Block::GLASS, sc::SCAFFOLDING, HOPPER] {
        assert!(WIRE.can_stay_on(below), "{}", below.name());
    }
    for below in [Block::LEAVES, Block::STONE_SLAB, Block::AIR] {
        assert!(!WIRE.can_stay_on(below), "{}", below.name());
    }
    let repeater = r::repeater(crate::world::block::Facing::North, 1, false);
    assert!(repeater.can_stay_on(sc::SCAFFOLDING));
    assert!(!repeater.can_stay_on(Block::LEAVES));
}
