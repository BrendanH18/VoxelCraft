//! Original asymmetric bark creature with amber eyes and procedural grain.
use super::*;
const BARK: Rgb = [111, 111, 96];
const BODY: &[Cuboid] = &[
    cube([-3.5, 16., -2.], [3.5, 32., 2.], BARK, 67),
    cube([-2., 23., 2.], [0., 28., 2.4], [68, 69, 58], 30),
    cube([1., 30., -1.], [3., 38., 1.], BARK, 60),
    // v0.18 Track 6 round 4: resin glowing in cracks of the trunk.
    cube([0.5, 18., 2.], [1.2, 22., 2.05], [255, 150, 30], 0),
    cube([-2.5, 26., -2.05], [-1.8, 30., -2.], [255, 150, 30], 0),
    cube([-3.55, 20., -0.5], [-3.5, 23., 0.3], [220, 120, 24], 0),
];
const HEAD: &[Cuboid] = &[
    cube([-3., 0., -2.], [3., 7., 2.], BARK, 66),
    cube([-1.2, 1.5, 2.], [0.4, 3., 4.8], BARK, 52),
    cube([-2.5, 3., 2.], [-1., 4., 2.15], [255, 167, 31], 0),
    cube([0., 4., 2.], [1.5, 5., 2.15], [255, 167, 31], 0),
    cube([1.8, 1.5, 2.], [2.6, 2.5, 2.15], [255, 167, 31], 0),
    // A split mouth and moss on the crown.
    cube([-2.4, 0.6, 2.], [0.6, 1.0, 2.05], [52, 52, 44], 0),
    cube([-3.05, 6.0, -2.05], [1.0, 7.05, 2.05], [86, 104, 56], 0),
];
const LEG: &[Cuboid] = &[cube([-1.3, -17., -1.3], [1.3, 0., 1.3], BARK, 55)];
const ARM: &[Cuboid] =
    &[cube([-1.1, -19., -1.1], [1.1, 0., 1.1], BARK, 60), cube([-1.5, -20., -1.8], [1.7, -17., 1.8], [87, 87, 73], 60)];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let s = m.creaking.as_ref().unwrap();
    // v1.0 gap Track 7: `CreakingModel` plays a keyframe walk; these limbs keep
    // its loop and strength (`applyWalk`).
    let (phase, scale) = super::motion::keyframe_phase(m.limb_phase, m.limb_amp, super::motion::CREAKING_WALK);
    let swing = if s.can_move { phase.sin() * 0.6 * scale } else { 0. };
    let twitch = if s.teardown > 0. || s.hit_cooldown > 0. { (time * 49.).sin() * 0.13 } else { 0. };
    parts![
        part(BODY, [0.; 3], Quat::from_rotation_z(twitch)),
        part(HEAD, [-0.5, 34., 0.], Quat::from_rotation_y(-m.head_yaw + twitch)),
        part(LEG, [-2., 17., 0.], Quat::from_rotation_x(swing)),
        part(LEG, [2., 17., 0.], Quat::from_rotation_x(-swing)),
        part(ARM, [-5., 31., 0.], Quat::from_rotation_x(-swing + twitch)),
        part(ARM, [4.5, 29., 0.], Quat::from_rotation_x(swing - twitch))
    ]
}
