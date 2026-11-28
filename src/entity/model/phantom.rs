//! Original night-blue ray silhouette, articulated wings and pale green eyes.
use super::*;
const BODY: &[Cuboid] = &[
    cube([-3., 0., -5.], [3., 5., 4.], [51, 64, 100], 35),
    cube([-2.5, 1., 4.], [2.5, 7., 7.], [65, 79, 117], 28),
    cube([-2., 4., 7.], [-0.5, 5.5, 7.2], [153, 217, 101], 0),
    cube([0.5, 4., 7.], [2., 5.5, 7.2], [153, 217, 101], 0),
    // v0.18 Track 6: a ragged mouth with teeth and a ridge down the back.
    cube([-1.8, 1.6, 7.], [1.8, 2.0, 7.05], [24, 30, 50], 0),
    cube([-1.2, 1.3, 7.], [-0.8, 1.6, 7.05], [214, 220, 200], 0),
    cube([0.8, 1.3, 7.], [1.2, 1.6, 7.05], [214, 220, 200], 0),
    cube([-0.4, 5., -4.], [0.4, 5.5, 3.], [80, 96, 136], 0),
];
/// v1.0 gap Track 7c: the tail on `PhantomModel`'s two joints, its base
/// at the body's back and its tip halfway along (the same box at rest).
const TAIL_BASE: &[Cuboid] = &[cube([-1., -1., -2.5], [1., 1., 0.], [56, 69, 105], 24)];
const TAIL_TIP: &[Cuboid] = &[cube([-1., -1., -2.5], [1., 1., 0.], [56, 69, 105], 24)];
/// v1.0 gap Track 7c: each wing in Java's two parts, the base and the tip
/// on its own joint 7 pixels out (the same boxes at rest).
const WING_BASE: &[Cuboid] =
    &[cube([0., -0.5, -4.], [7., 0.5, 3.], [45, 57, 91], 35), cube([0., 0.5, 1.], [7., 1., 2.], [93, 111, 143], 16)];
const WING_TIP: &[Cuboid] = &[
    cube([0., -0.5, -4.], [5., 0.5, 3.], [45, 57, 91], 35),
    cube([0., -0.5, -6.], [11., 0.5, 1.], [58, 71, 108], 38),
    cube([0., 0.5, 1.], [9., 1., 2.], [93, 111, 143], 16),
];
/// v1.0 gap Track 7b: `PhantomModel.setupAnim`: both wings beat together,
/// `cos(flapTime * 7.448 degrees) * 16 degrees`, where `flapTime` is
/// `ageInTicks` plus the phantom's own offset (`getUniqueFlapTickOffset`,
/// three ticks per entity id; here its uid, kept small for the float).
/// v1.0 gap Track 7c: each wing tip beats by the same angle again on the
/// base's (`left_wing_tip.zRot` as `left_wing_base.zRot`), and the tail's
/// two joints each droop `5 + cos(2 * anim) * 5` degrees.
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let flap_time = (m.uid % 4096) as f32 * 3.0 + age_ticks(time);
    let anim = (flap_time * 7.448_451).to_radians();
    let flap = anim.cos() * 16f32.to_radians();
    let droop = Quat::from_rotation_x(-(5.0 + (anim * 2.0).cos() * 5.0).to_radians());
    let mut p = Parts::new();
    p.push(part(BODY, [0.; 3], Quat::IDENTITY));
    for side in [Quat::IDENTITY, Quat::from_rotation_y(PI)] {
        let base = side * Quat::from_rotation_z(-flap);
        let root = side * Vec3::new(3., 3., 0.);
        p.push(part(WING_BASE, root.to_array(), base));
        let tip = root + base * Vec3::new(7., 0., 0.);
        p.push(part(WING_TIP, tip.to_array(), base * Quat::from_rotation_z(-flap)));
    }
    let tail_at = Vec3::new(0., 2., -5.);
    p.push(part(TAIL_BASE, tail_at.to_array(), droop));
    p.push(part(TAIL_TIP, (tail_at + droop * Vec3::new(0., 0., -2.5)).to_array(), droop * droop));
    p
}
