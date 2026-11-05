//! Original six-legged moss-backed sniffer, vertex-colour procedural texel art.
use super::*;
use crate::entity::sniffer as s;
const BODY: &[Cuboid] = &[
    cube([-12., 7., -15.], [12., 22., 12.], [124, 63, 43], 35),
    cube([-12.3, 18., -15.3], [12.3, 25., 12.3], [70, 108, 42], 45),
    cube([-10., 25., -12.], [9., 27., 10.], [89, 126, 49], 35),
];
const HEAD: &[Cuboid] = &[
    cube([-9., -5., -3.], [9., 6., 12.], [133, 72, 48], 30),
    cube([-10., -3., 10.], [10., 3., 16.], [204, 171, 75], 30),
    cube([-7.5, 3., 10.], [-5.5, 5., 12.5], [28, 28, 22], 0),
    cube([5.5, 3., 10.], [7.5, 5., 12.5], [28, 28, 22], 0),
    cube([-8., -1., 16.], [-5., 1., 16.2], [65, 49, 33], 0),
    cube([5., -1., 16.], [8., 1., 16.2], [65, 49, 33], 0),
];
const LEG: &[Cuboid] = &[cube([-2., -7., -2.], [2., 0., 2.], [75, 49, 34], 20)];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let state = m.sniffer.as_ref().map_or(0, |s| s.pose);
    let swing = m.limb_phase.sin() * m.limb_amp * 0.6;
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
