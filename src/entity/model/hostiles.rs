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

// ---------------------------------------------------------------- endermite
// Original colours over four segments with `EndermiteModel`'s proportions
// (width, height, depth in pixels, head first): 4x3x2, 6x4x5, 3x3x1, 1x2x1.
const MITE_SHELL: Rgb = [44, 32, 58];
const MITE_GLOW: Rgb = [112, 64, 150];
const fn mite(w: f32, h: f32, d: f32, color: Rgb, noise: u8) -> Cuboid {
    cube([-w / 2.0, 0.0, -d / 2.0], [w / 2.0, h, d / 2.0], color, noise)
}
const MITE_SEGMENTS: [&[Cuboid]; 4] = [
    &[mite(4.0, 3.0, 2.0, MITE_SHELL, 40)],
    &[mite(6.0, 4.0, 5.0, MITE_SHELL, 52), cube([-2.0, 4.0, -1.5], [2.0, 4.01, 1.5], MITE_GLOW, 70)],
    &[mite(3.0, 3.0, 1.0, MITE_SHELL, 44)],
    &[mite(1.0, 2.0, 1.0, MITE_GLOW, 36)],
];
/// Segment centres along the body, front to back: Java places them at
/// -3.5, 0, 3 and 4 (its models face -Z; these face +Z).
const MITE_Z: [f32; 4] = [3.5, 0.0, -3.0, -4.0];

/// The endermite's segments sway side to side (`EndermiteModel.setupAnim`:
/// phase `ageInTicks * 0.9 + i * 0.15π`, turning `π/100 * (1 + |i - 2|)` and
/// shifting `π/10 * |i - 2|` pixels). It has no head of its own: every
/// segment follows the body yaw.
pub(super) fn endermite(time: f32) -> Parts {
    let age = time * 20.0;
    MITE_SEGMENTS
        .iter()
        .zip(MITE_Z)
        .enumerate()
        .map(|(i, (&boxes, z))| {
            let phase = age * 0.9 + i as f32 * 0.15 * PI;
            let k = (i as f32 - 2.0).abs();
            let turn = phase.cos() * PI * 0.01 * (1.0 + k);
            part(boxes, [phase.sin() * PI * 0.1 * k, 0.0, z], Quat::from_rotation_y(turn))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(m: &Mob, time: f32) -> Vec<[f32; 3]> {
        let mut out = EntityMesh::flat();
        build(std::slice::from_ref(m), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, time, 1.0, &mut out);
        out.iter().map(|v| v.pos).collect()
    }

    #[test]
    fn the_endermite_turns_as_one_body_and_its_segments_sway() {
        let mut m = Mob::new(MobKind::Endermite, DVec3::new(6.0, 64.0, 0.0), 0.4);
        let still = mesh(&m, 1.0);
        assert!(!still.is_empty());
        // `Endermite.tick` keeps the body on its own rotation and the model
        // draws no head: looking around changes nothing.
        m.head_yaw = 1.1;
        m.head_pitch = -0.4;
        assert_eq!(mesh(&m, 1.0), still);
        // Turning the body turns every segment.
        m.yaw += 1.0;
        assert_ne!(mesh(&m, 1.0), still);
        // `setupAnim` sways the segments over time.
        m.yaw -= 1.0;
        assert_ne!(mesh(&m, 1.3), still);
        // It sits on the ground, 4 pixels tall at the middle segment.
        let (lo, hi) = still.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
        assert!(lo.abs() < 1e-4 && (hi - 4.01 / 16.0).abs() < 1e-3, "{lo} {hi}");
    }
}
