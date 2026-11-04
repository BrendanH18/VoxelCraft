//! Original frost-cloth stray and moss/mushroom bogged geometry. Procedural
//! face noise follows the existing skeleton's material; no Mojang assets.
use super::*;
const FROST: &[Cuboid] = &[
    cube([-4.4, 23.5, -2.5], [4.4, 24.5, 2.5], [109, 139, 148], 42),
    cube([-4.1, 14., -2.1], [4.1, 23.5, 2.1], [130, 158, 163], 48),
    cube([-3.8, 9., -2.3], [-1.2, 14., 2.3], [113, 142, 151], 56),
    cube([0.2, 10.5, -2.3], [3.9, 14., 2.3], [113, 142, 151], 56),
];
const MOSS: &[Cuboid] = &[
    cube([-4.3, 14., -2.3], [4.3, 24., 2.3], [78, 103, 60], 76),
    cube([-3.5, 11., 2.2], [-0.8, 17., 2.6], [104, 125, 63], 72),
    cube([1., 10., -2.6], [3.8, 16., -2.1], [58, 91, 62], 64),
];
const MUSHROOMS: &[Cuboid] = &[
    cube([-3., 7., -1.], [-2., 10., 0.], [191, 179, 137], 12),
    cube([-4.2, 9.5, -2.2], [-0.8, 10.0, 1.2], [164, 66, 46], 45),
    cube([1.5, 7., 0.5], [2.3, 9.5, 1.3], [191, 179, 137], 12),
    cube([0.4, 9., -0.5], [3.4, 10., 2.4], [126, 94, 61], 45),
];
pub(super) fn pose(m: &Mob, swing: f32, head: Quat) -> Parts {
    let rx = Quat::from_rotation_x;
    let arm = |s: f32| if m.ai == Ai::Chase { rx(-FRAC_PI_2 - m.head_pitch) } else { rx(-swing * s * 0.6) };
    let mut p = parts![
        part(SKELETON_BODY, [0.; 3], Quat::IDENTITY),
        part(SKELETON_LEG, [-2., 12., 0.], rx(swing)),
        part(SKELETON_LEG, [2., 12., 0.], rx(-swing)),
        part(SKELETON_ARM, [-5., 22., 0.], arm(1.)),
        part(SKELETON_BOW_ARM, [5., 22., 0.], arm(-1.)),
        part(SKELETON_HEAD, [0., 24., 0.], head),
    ];
    p.push(part(if m.kind == MobKind::Stray { FROST } else { MOSS }, [0.; 3], Quat::IDENTITY));
    if m.kind == MobKind::Bogged && !m.sheared {
        p.push(part(MUSHROOMS, [0., 24., 0.], head));
    }
    p
}
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    if c.color == BONE {
        c.color = if m.kind == MobKind::Stray { [172, 194, 197] } else { [119, 137, 96] };
        c.noise = if m.kind == MobKind::Stray { 26 } else { 62 };
    }
}
