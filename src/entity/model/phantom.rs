//! Original night-blue ray silhouette, articulated wings and pale green eyes.
use super::*;
const BODY: &[Cuboid] = &[
    cube([-3., 0., -5.], [3., 5., 4.], [51, 64, 100], 35),
    cube([-2.5, 1., 4.], [2.5, 7., 7.], [65, 79, 117], 28),
    cube([-2., 4., 7.], [-0.5, 5.5, 7.2], [153, 217, 101], 0),
    cube([0.5, 4., 7.], [2., 5.5, 7.2], [153, 217, 101], 0),
    cube([-1., 1., -10.], [1., 3., -5.], [56, 69, 105], 24),
    // v0.18 Track 6: a ragged mouth with teeth and a ridge down the back.
    cube([-1.8, 1.6, 7.], [1.8, 2.0, 7.05], [24, 30, 50], 0),
    cube([-1.2, 1.3, 7.], [-0.8, 1.6, 7.05], [214, 220, 200], 0),
    cube([0.8, 1.3, 7.], [1.2, 1.6, 7.05], [214, 220, 200], 0),
    cube([-0.4, 5., -4.], [0.4, 5.5, 3.], [80, 96, 136], 0),
];
const WING: &[Cuboid] = &[
    cube([0., -0.5, -4.], [12., 0.5, 3.], [45, 57, 91], 35),
    cube([7., -0.5, -6.], [18., 0.5, 1.], [58, 71, 108], 38),
    cube([0., 0.5, 1.], [16., 1., 2.], [93, 111, 143], 16),
];
/// v1.0 gap Track 7b: `PhantomModel.setupAnim`: both wings beat together,
/// `cos(flapTime * 7.448 degrees) * 16 degrees`, where `flapTime` is
/// `ageInTicks` plus the phantom's own offset (`getUniqueFlapTickOffset`,
/// three ticks per entity id; here its uid, kept small for the float).
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let flap_time = (m.uid % 4096) as f32 * 3.0 + age_ticks(time);
    let flap = ((flap_time * 7.448_451).to_radians()).cos() * 16f32.to_radians();
    parts![
        part(BODY, [0.; 3], Quat::IDENTITY),
        part(WING, [3., 3., 0.], Quat::from_rotation_z(-flap)),
        part(WING, [-3., 3., 0.], Quat::from_rotation_y(PI) * Quat::from_rotation_z(-flap))
    ]
}
