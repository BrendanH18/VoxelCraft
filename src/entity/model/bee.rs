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
];
const WING: &[Cuboid] = &[cube([0., 0., -3.], [7., 0.3, 4.], [199, 229, 233], 4)];
const STING: &[Cuboid] = &[cube([-0.5, 5., -7.], [0.5, 6., -4.], [33, 28, 24], 0)];
const NECTAR: &[Cuboid] = &[
    cube([-4.3, 3., -3.], [-3.8, 6., 1.], [246, 197, 52], 10),
    cube([3.8, 3., -3.], [4.3, 6., 1.], [246, 197, 52], 10),
];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let flap = (time * 110.).sin() * 0.7;
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
    p
}
