//! Original six-legged moss-backed sniffer, vertex-colour procedural texel art.
use super::*;
use crate::entity::sniffer as s;
const BODY: &[Cuboid] = &[
    cube([-12., 7., -15.], [12., 22., 12.], [124, 63, 43], 35),
    cube([-12.3, 18., -15.3], [12.3, 25., 12.3], [70, 108, 42], 45),
    cube([-10., 25., -12.], [9., 27., 10.], [89, 126, 49], 35),
    // v0.18 Track 6: a few flowers in the moss.
    cube([-6., 27., -4.], [-5., 27.6, -3.], [214, 96, 150], 0),
    cube([3., 27., 2.], [4., 27.6, 3.], [236, 206, 70], 0),
    cube([-1., 27., 6.], [0., 27.6, 7.], [214, 96, 150], 0),
];
const HEAD: &[Cuboid] = &[
    cube([-9., -5., -3.], [9., 6., 12.], [133, 72, 48], 30),
    cube([-10., -3., 10.], [10., 3., 16.], [204, 171, 75], 30),
    cube([-7.5, 3., 10.], [-5.5, 5., 12.5], [28, 28, 22], 0),
    cube([5.5, 3., 10.], [7.5, 5., 12.5], [28, 28, 22], 0),
    cube([-8., -1., 16.], [-5., 1., 16.2], [65, 49, 33], 0),
    cube([5., -1., 16.], [8., 1., 16.2], [65, 49, 33], 0),
    // v0.18 Track 6: eye glints, a mouth line and two teeth.
    cube([-7., 4.2, 12.5], [-6.4, 4.8, 12.55], [214, 208, 190], 0),
    cube([6.4, 4.2, 12.5], [7., 4.8, 12.55], [214, 208, 190], 0),
    cube([-8., -2.4, 16.], [8., -2.0, 16.05], [120, 90, 40], 0),
    cube([-2.5, -3.0, 16.], [-1.0, -2.4, 16.1], [236, 230, 210], 0),
    cube([1.0, -3.0, 16.], [2.5, -2.4, 16.1], [236, 230, 210], 0),
];
const LEG: &[Cuboid] = &[cube([-2., -7., -2.], [2., 0., 2.], [75, 49, 34], 20)];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let state = m.sniffer.as_ref().map_or(0, |s| s.pose);
    // v1.0 gap Track 7: `SnifferModel` plays a keyframe walk; these limbs keep
    // its loop and strength (`applyWalk`).
    let (phase, scale) = super::motion::keyframe_phase(m.limb_phase, m.limb_amp, super::motion::SNIFFER_WALK);
    let swing = phase.sin() * 0.6 * scale;
    let digging = state == s::DIGGING;
    let lower = if digging {
        -3.
    } else if state == s::RISING {
        -3. * (1. - m.sniffer.as_ref().unwrap().elapsed as f32 / 40.).clamp(0., 1.)
    } else {
        0.
    };
    let nose = match state {
        s::DIGGING => 0.4 + (time * 9.).sin() * 0.1,
        s::SNIFFING => 0.25 + (time * 5.).sin() * 0.08,
        s::SCENTING => -0.3,
        s::HAPPY => (time * 4.).sin() * 0.15,
        _ => -m.head_pitch,
    };
    parts![
        part(BODY, [0., lower, 0.], Quat::IDENTITY),
        part(HEAD, [0., 14. + lower, 12.], Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(nose)),
        part(LEG, [-9., 7., -10.], Quat::from_rotation_x(swing)),
        part(LEG, [9., 7., -10.], Quat::from_rotation_x(-swing)),
        part(LEG, [-9., 7., 0.], Quat::from_rotation_x(-swing)),
        part(LEG, [9., 7., 0.], Quat::from_rotation_x(swing)),
        part(LEG, [-9., 7., 10.], Quat::from_rotation_x(swing)),
        part(LEG, [9., 7., 10.], Quat::from_rotation_x(-swing))
    ]
}
