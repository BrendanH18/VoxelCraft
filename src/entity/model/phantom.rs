//! Original night-blue ray silhouette, articulated wings and pale green eyes.
use super::*;
const BODY: &[Cuboid] = &[
    cube([-3., 0., -5.], [3., 5., 4.], [51, 64, 100], 35),
    cube([-2.5, 1., 4.], [2.5, 7., 7.], [65, 79, 117], 28),
    cube([-2., 4., 7.], [-0.5, 5.5, 7.2], [153, 217, 101], 0),
    cube([0.5, 4., 7.], [2., 5.5, 7.2], [153, 217, 101], 0),
    cube([-1., 1., -10.], [1., 3., -5.], [56, 69, 105], 24),
];
const WING: &[Cuboid] = &[
    cube([0., -0.5, -4.], [12., 0.5, 3.], [45, 57, 91], 35),
    cube([7., -0.5, -6.], [18., 0.5, 1.], [58, 71, 108], 38),
    cube([0., 0.5, 1.], [16., 1., 2.], [93, 111, 143], 16),
];
pub(super) fn pose(time: f32) -> Parts {
    let flap = (time * 7.).sin() * 0.45;
    parts![
        part(BODY, [0.; 3], Quat::IDENTITY),
        part(WING, [3., 3., 0.], Quat::from_rotation_z(flap)),
        part(WING, [-3., 3., 0.], Quat::from_rotation_y(PI) * Quat::from_rotation_z(-flap))
    ]
}
