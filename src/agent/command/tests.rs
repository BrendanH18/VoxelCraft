//! `/tp` rotations, `/rotate` and their `facing` forms.
use super::*;
use Coordinate::{Absolute, Relative};
use glam::Vec3;

fn parse(text: &str) -> Command {
    Command::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// Runs a parsed `/tp` or `/rotate` on a player at `pos` facing Java yaw 0, pitch 0.
fn run(text: &str, pos: DVec3) -> Player {
    let mut player = Player::new(pos);
    (player.yaw, player.pitch) = from_java_rotation(0.0, 0.0);
    match parse(text) {
        Command::Teleport(to, turn) => teleport(&mut player, to, turn).unwrap(),
        Command::Rotate(turn) => rotate(&mut player, turn).unwrap(),
        _ => panic!("{text} is not a turn"),
    }
    player
}

fn assert_angles(player: &Player, yaw: f64, pitch: f64) {
    let (y, p) = java_rotation(player.yaw, player.pitch);
    assert!(wrap_degrees(y - yaw).abs() < 1e-3, "yaw {y}, expected {yaw}");
    assert!((p - pitch).abs() < 1e-3, "pitch {p}, expected {pitch}");
}

fn assert_close(got: (f64, f64), want: (f64, f64)) {
    assert!((got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9, "{got:?}, expected {want:?}");
}

fn turn_of(text: &str) -> Option<Turn> {
    match parse(text) {
        Command::Teleport(_, turn) => turn,
        Command::Rotate(turn) => Some(turn),
        _ => None,
    }
}

#[test]
fn parses_rotation_and_facing_forms() {
    let Command::Teleport(pos, turn) = parse("/tp @s 10 64 -3 90 45") else { panic!() };
    // Whole x and z name block centres, as Java's `Vec3Argument` reads them.
    assert_eq!(pos, PositionSpec([Absolute(10.5), Absolute(64.0), Absolute(-2.5)]));
    assert_eq!(turn, Some(Turn::Rotation { yaw: Absolute(90.0), pitch: Absolute(45.0) }));
    let Command::Teleport(pos, turn) = parse("teleport @p ~ ~1 1.25 ~10 ~-5") else { panic!() };
    assert_eq!(pos, PositionSpec([Relative(0.0), Relative(1.0), Absolute(1.25)]));
    assert_eq!(turn, Some(Turn::Rotation { yaw: Relative(10.0), pitch: Relative(-5.0) }));
    assert!(matches!(parse("tp 0 64 0"), Command::Teleport(_, None)));
    assert!(matches!(parse("teleport @s 0 64 0"), Command::Teleport(_, None)));
    assert_eq!(
        turn_of("tp @s ~ ~ ~ facing 3 70 ~-2"),
        Some(Turn::FacingPos(PositionSpec([Absolute(3.5), Absolute(70.0), Relative(-2.0)])))
    );
    for (text, anchor) in [
        ("tp @s ~ ~ ~ facing entity @s", Anchor::Feet),
        ("tp @s ~ ~ ~ facing entity @p feet", Anchor::Feet),
        ("tp @s ~ ~ ~ facing entity @s eyes", Anchor::Eyes),
        ("rotate @s facing entity @s eyes", Anchor::Eyes),
    ] {
        assert_eq!(turn_of(text), Some(Turn::FacingSelf(anchor)), "{text}");
    }
    assert_eq!(turn_of("/rotate @s ~-90 30"), Some(Turn::Rotation { yaw: Relative(-90.0), pitch: Absolute(30.0) }));
    assert!(matches!(turn_of("rotate @p facing 0 64 0"), Some(Turn::FacingPos(_))));
    assert!(parse("rotate @s 0 0").cheat());
}

#[test]
fn rejects_malformed_turns() {
    for text in [
        // Java's target-less form takes no rotation.
        "tp 0 64 0 90 0",
        "tp @s 0 64 0 90",
        "tp @s 0 64 0 90 0 0",
        "tp @a 0 64 0 90 0",
        "tp @s 0 64 0 facing 1 2",
        "tp @s 0 64 0 facing entity @e",
        "tp @s 0 64 0 facing entity @s head",
        "tp @s 0 64 0 facing entity @s eyes extra",
        "rotate",
        "rotate @s",
        "rotate @s 90",
        "rotate 90 0",
        "rotate @a 90 0",
        "rotate @s ^ ^",
        "rotate @s 0 NaN",
        "rotate @s facing",
        "rotate @s facing entity",
    ] {
        assert!(Command::parse(text).is_err(), "{text}");
    }
}

#[test]
fn rotation_uses_java_angles() {
    let at = DVec3::new(0.5, 64.0, 0.5);
    // Java yaw 0 faces south (+Z), 90 west (-X); positive pitch looks down.
    let south = run("rotate @s 0 0", at);
    assert!((south.forward() - Vec3::Z).length() < 1e-3);
    let west = run("rotate @s 90 0", at);
    assert!((west.forward() + Vec3::X).length() < 1e-3);
    let down = run("rotate @s -90 45", at);
    assert_angles(&down, -90.0, 45.0);
    assert!(down.forward().y < -0.7 && down.forward().x > 0.7, "east and down");
    assert_angles(&run("rotate @s ~90 ~10", at), 90.0, 10.0);
    assert_angles(&run("rotate @s ~-90 ~", at), -90.0, 0.0);
    // Java clamps the pitch to 90 degrees; the camera stops at 89.
    assert_eq!(run("rotate @s 0 120", at).pitch, -PITCH_LIMIT);
    // `/tp` wraps the typed pitch first, so 270 is -90: straight up.
    assert_eq!(run("tp @s ~ ~ ~ 0 270", at).pitch, PITCH_LIMIT);
    let moved = run("tp @s 10 70 -4 180 -30", at);
    assert_eq!(moved.pos, DVec3::new(10.5, 70.0, -3.5));
    assert_angles(&moved, 180.0, -30.0);
    // A plain `/tp` keeps the facing.
    assert_angles(&run("tp @s 3 64 3", at), 0.0, 0.0);
}

#[test]
fn facing_looks_from_the_feet_after_the_teleport() {
    let at = DVec3::new(0.5, 64.0, 0.5);
    // Due north of the feet: Java yaw 180 (`Mth.wrapDegrees` gives -180).
    let north = run("rotate @s facing 0 64 -10", at);
    assert_angles(&north, 180.0, 0.0);
    assert!((north.forward() + Vec3::Z).length() < 1e-3);
    assert_close(java_look_at(at, at + DVec3::new(0.0, 0.0, -10.0)), (-180.0, 0.0));
    // Straight up: pitch -90, and atan2(0, 0) leaves Java yaw -90 (east).
    assert_close(java_look_at(at, at + DVec3::Y * 5.0), (-90.0, -90.0));
    let up = run("rotate @s facing 0 100 0", at);
    assert_eq!(up.pitch, PITCH_LIMIT);
    let limit = f64::from(PITCH_LIMIT).to_degrees();
    assert_angles(&up, -90.0, -limit);
    // 45 degrees down toward +X.
    assert_angles(&run("rotate @s facing 5 59 0", at), -90.0, 45.0);
    // `~` in the facing position counts from where the command ran: after
    // moving 10 blocks east the player looks back west at the old spot.
    let back = run("tp @s 10 64 ~ facing ~ ~ ~", at);
    assert_eq!(back.pos, DVec3::new(10.5, 64.0, 0.5));
    assert_angles(&back, 90.0, 0.0);
    // Facing the player's own eyes looks straight up; its feet, level east.
    assert_angles(&run("tp @s 5 70 5 facing entity @s eyes", at), -90.0, -limit);
    assert_angles(&run("rotate @s facing entity @s", at), -90.0, 0.0);
}

#[test]
fn a_bad_facing_leaves_the_player_in_place() {
    let start = DVec3::new(0.5, 64.0, 0.5);
    let mut player = Player::new(start);
    let (yaw, pitch) = (player.yaw, player.pitch);
    let Command::Teleport(to, turn) = parse("tp @s 10 64 10 facing 0 64 40000000") else { panic!() };
    assert!(teleport(&mut player, to, turn).is_err());
    assert_eq!((player.pos, player.yaw, player.pitch), (start, yaw, pitch));
}

#[test]
fn completes_turn_words() {
    assert_eq!(tab_complete("/rot").as_deref(), Some("/rotate"));
    assert_eq!(tab_complete("/rotate @s f").as_deref(), Some("/rotate @s facing"));
    assert_eq!(tab_complete("/tp @s 1 2 3 fa").as_deref(), Some("/tp @s 1 2 3 facing"));
    assert_eq!(tab_complete("/rotate @s facing e").as_deref(), Some("/rotate @s facing entity"));
    assert_eq!(tab_complete("/rotate @s facing entity @s e").as_deref(), Some("/rotate @s facing entity @s eyes"));
}
