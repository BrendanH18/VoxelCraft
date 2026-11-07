//! Original warden: a towering, eyeless sculk-flesh body with a glowing
//! ribcage, long arms and two tendrils that twitch when it hears something.
//! Vertex-colour procedural texel art; nothing from Java's model or skin.
use super::*;
use crate::entity::warden as w;

const FLESH: [u8; 3] = [22, 46, 58];
const DEEP: [u8; 3] = [12, 28, 38];
const RIB: [u8; 3] = [126, 170, 168];
const SOUL: [u8; 3] = [52, 214, 226];
const TENDRIL: [u8; 3] = [64, 140, 150];

const LEG: &[Cuboid] = &[cube([-3.2, -12., -3.2], [3.2, 0., 3.2], DEEP, 25)];
const TORSO: &[Cuboid] = &[
    cube([-9.5, 12., -5.5], [9.5, 32., 5.5], FLESH, 30),
    // The ribcage, with souls glowing between the ribs.
    cube([-7., 18., 5.5], [7., 29., 6.3], RIB, 20),
    cube([-5., 20., 6.3], [-2., 23., 6.6], SOUL, 10),
    cube([2., 20., 6.3], [5., 23., 6.6], SOUL, 10),
    cube([-5., 25., 6.3], [-2., 28., 6.6], SOUL, 10),
    cube([2., 25., 6.3], [5., 28., 6.6], SOUL, 10),
    cube([-9.8, 28., -5.8], [9.8, 32.5, 5.8], DEEP, 25),
];
/// The heart, which swells with each beat.
const HEART: &[Cuboid] = &[cube([-1.5, -1.5, -0.5], [1.5, 1.5, 0.5], SOUL, 5)];
const HEAD: &[Cuboid] = &[
    cube([-8., 0., -6.], [8., 14., 5.], FLESH, 30),
    // A wide jaw with a pale lip and no eyes at all.
    cube([-6.5, 1., 5.], [6.5, 4., 5.6], RIB, 15),
    cube([-7., 8., 5.], [7., 11., 5.4], DEEP, 15),
];
const TENDRIL_L: &[Cuboid] = &[cube([-1., 0., -0.5], [1., 7., 0.5], TENDRIL, 20)];
const ARM: &[Cuboid] =
    &[cube([-2.8, -22., -2.8], [2.8, 0., 2.8], FLESH, 30), cube([-3., -22., -3.], [3., -17., 3.], DEEP, 20)];

pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let s = m.warden.as_deref();
    let state = s.map_or(w::STANDING, |s| s.pose);
    let elapsed = s.and_then(|s| s.action).map_or(0.0, |(_, t)| t as f32);
    // Rising from (or sinking into) the ground over the animation.
    let lower = match state {
        w::EMERGING => -46.0 * (1.0 - (elapsed / f32::from(w::EMERGE_TICKS)).clamp(0.0, 1.0)),
        w::DIGGING => -46.0 * (elapsed / f32::from(w::DIG_TICKS)).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let swing = m.limb_phase.sin() * m.limb_amp * 0.7;
    let attack = (m.attack_anim / 0.75).clamp(0.0, 1.0);
    let sonic = s.and_then(|s| s.sonic).map_or(0.0, |t| (t as f32 / 20.0).min(1.0));
    let (head_pitch, head_yaw) = match state {
        w::ROARING => (-0.6 + (time * 30.0).sin() * 0.05, 0.0),
        w::SNIFFING => (-0.2, (time * 3.0).sin() * 0.6),
        _ => (-m.head_pitch, -m.head_yaw),
    };
    let arm_raise = match state {
        w::ROARING => 0.9,
        w::EMERGING | w::DIGGING => 1.6,
        _ => attack * 2.2 + sonic * -0.6,
    };
    let twitch = s.map_or(0.0, |s| f32::from(s.tendril) / 10.0) * (time * 40.0).sin() * 0.5;
    let beat = s.map_or(0.0, |s| f32::from(s.heart) / 10.0);
    let head = Quat::from_rotation_y(head_yaw) * Quat::from_rotation_x(head_pitch);
    parts![
        part(LEG, [-5.5, 12. + lower, 0.], Quat::from_rotation_x(swing)),
        part(LEG, [5.5, 12. + lower, 0.], Quat::from_rotation_x(-swing)),
        part(TORSO, [0., lower, 0.], Quat::from_rotation_x(-sonic * 0.25)),
        part(HEART, [0., 24. + lower, 6.0 + beat * 1.2], Quat::IDENTITY),
        part(HEAD, [0., 32. + lower, 0.], head),
        part(TENDRIL_L, [-9., 39.5 + lower, 0.], Quat::from_rotation_z(0.5 + twitch)),
        part(TENDRIL_L, [9., 39.5 + lower, 0.], Quat::from_rotation_z(-0.5 - twitch)),
        part(ARM, [-12.3, 30.5 + lower, 0.], Quat::from_rotation_x(-swing - arm_raise)),
        part(ARM, [12.3, 30.5 + lower, 0.], Quat::from_rotation_x(swing - arm_raise))
    ]
}
