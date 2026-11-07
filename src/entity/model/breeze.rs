//! Original breeze: a pale-violet head over three spinning tiers of wind
//! and three rods circling it. Inhaling and shooting puff the head.
use super::*;
const HEAD: &[Cuboid] = &[
    cube([-4., 20., -4.5], [4., 28., 4.5], [202, 196, 230], 14),
    cube([-3., 23., 4.51], [-1., 25., 4.6], [92, 228, 238], 0),
    cube([1., 23., 4.51], [3., 25., 4.6], [92, 228, 238], 0),
    cube([-2.5, 21., 4.51], [2.5, 21.8, 4.6], [128, 120, 170], 0),
];
const TIER_TOP: &[Cuboid] = &[cube([-5., 14., -5.], [5., 19., 5.], [176, 200, 228], 22)];
const TIER_MID: &[Cuboid] = &[cube([-3.5, 8., -3.5], [3.5, 14., 3.5], [160, 188, 222], 22)];
const TIER_LOW: &[Cuboid] = &[cube([-2., 0., -2.], [2., 8., 2.], [146, 178, 214], 22)];
const ROD: &[Cuboid] = &[cube([-0.7, 0., -0.7], [0.7, 8., 0.7], [224, 230, 244], 10)];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    use crate::entity::breeze::Pose;
    let pose = m.breeze.as_ref().map_or(Pose::Standing, |s| s.pose);
    let spin = time * 6.;
    let head = match pose {
        Pose::Inhaling | Pose::Shooting => Quat::from_rotation_x(-0.25 + (time * 18.).sin() * 0.05),
        _ => Quat::IDENTITY,
    };
    let bob = (time * 3.).sin() * 0.6;
    let [r0, r1, r2] = [0., 2.094, 4.189].map(|a| {
        let a = a + time * 2.4;
        part(ROD, [a.cos() * 7., 14. + bob + (a * 2.).sin(), a.sin() * 7.], Quat::from_rotation_z(0.25 * a.sin()))
    });
    parts![
        part(HEAD, [0., bob, 0.], head),
        part(TIER_TOP, [0., bob, 0.], Quat::from_rotation_y(spin)),
        part(TIER_MID, [0., bob * 0.5, 0.], Quat::from_rotation_y(-spin * 1.3)),
        part(TIER_LOW, [0., 0., 0.], Quat::from_rotation_y(spin * 1.7)),
        r0,
        r1,
        r2
    ]
}
