//! Original striped bee, antennae, legs, stinger and beating wings.
use super::*;
const BODY: &[Cuboid] = &[
    cube([-4., 2., -5.], [4., 10., 5.], [225, 170, 40], 20),
    cube([-4.1, 2., -3.], [4.1, 10.1, -1.], [49, 37, 23], 10),
    cube([-4.1, 2., 1.], [4.1, 10.1, 3.], [49, 37, 23], 10),
    cube([-3.8, 5., 5.], [-1.8, 8., 5.2], [27, 24, 25], 0),
    cube([1.8, 5., 5.], [3.8, 8., 5.2], [27, 24, 25], 0),
    cube([-3., 10., 3.], [-2.5, 12., 4.], [45, 33, 21], 0),
    cube([2.5, 10., 3.], [3., 12., 4.], [45, 33, 21], 0),
    cube([-3., 0., -3.], [-2.4, 3., 3.], [50, 34, 20], 0),
    cube([2.4, 0., -3.], [3., 3., 3.], [50, 34, 20], 0),
    // v0.18 Track 6: eye glints, a mouth and a fuzzy pale collar.
    cube([-3.4, 7.0, 5.2], [-2.8, 7.6, 5.25], [226, 222, 214], 0),
    cube([2.8, 7.0, 5.2], [3.4, 7.6, 5.25], [226, 222, 214], 0),
    cube([-0.8, 3.6, 5.0], [0.8, 4.0, 5.05], [96, 62, 28], 0),
    cube([-4.05, 8.6, 3.0], [4.05, 10.05, 4.0], [240, 214, 120], 0),
];
const WING: &[Cuboid] = &[cube([0., 0., -3.], [7., 0.3, 4.], [199, 229, 233], 4)];
const STING: &[Cuboid] = &[cube([-0.5, 5., -7.], [0.5, 6., -4.], [33, 28, 24], 0)];
const NECTAR: &[Cuboid] = &[
    cube([-4.3, 3., -3.], [-3.8, 6., 1.], [246, 197, 52], 10),
    cube([3.8, 3., -3.], [4.3, 6., 1.], [246, 197, 52], 10),
];
/// v1.0 gap Track 7b: `BeeModel.setupAnim`. Flying (not resting on the
/// ground) the wings beat `cos(ageInTicks * 120.32 degrees) * 0.15 pi`
/// and, unless angry, the bee bobs and pitches on `cos(ageInTicks *
/// 0.18)`; an angry bee at its target tumbles forward by `rollAmount`
/// (`Bee.updateRollAmount`'s state) toward 177 degrees.
pub(super) fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
    let age = age_ticks(time);
    let flying = !(m.on_ground && m.limb_amp < 1e-3);
    let flap = if flying { (age * 120.321_13_f32.to_radians()).cos() * PI * 0.15 } else { 0.0 };
    let angry = m.bee.is_some_and(|s| s.anger > 0);
    let (mut pitch, mut rise) = (0.0, 0.0);
    if flying && !angry {
        let speed = (age * 0.18).cos();
        (pitch, rise) = (0.1 + speed * PI * 0.025, speed * 0.9);
    }
    let roll = crate::entity::mob_anim::lerp(alpha, m.anim.o_roll, m.anim.roll);
    if roll > 0.0 {
        pitch += (3.091_592_8 - pitch) * roll;
    }
    let mut p = parts![
        part(BODY, [0.; 3], Quat::IDENTITY),
        part(WING, [3., 9., 0.], Quat::from_rotation_z(flap)),
        part(WING, [-3., 9., 0.], Quat::from_rotation_z(std::f32::consts::PI - flap))
    ];
    if !m.bee.is_some_and(|s| s.stung) {
        p.push(part(STING, [0.; 3], Quat::IDENTITY));
    }
    if m.bee.is_some_and(|s| s.nectar) {
        p.push(part(NECTAR, [0.; 3], Quat::IDENTITY));
    }
    // The whole bee (Java's `bone`, pivoted 5 pixels up) pitches and rises.
    p.turn_about(Vec3::new(0.0, 5.0, 0.0), Quat::from_rotation_x(pitch));
    p.transform(Vec3::new(0.0, rise, 0.0), Quat::IDENTITY);
    p
}
