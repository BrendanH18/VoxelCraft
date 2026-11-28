//! Original breeze: a pale-violet head over three spinning tiers of wind
//! and three rods circling it. Inhaling and shooting puff the head.
use super::*;
const HEAD: &[Cuboid] = &[
    cube([-4., 20., -4.5], [4., 28., 4.5], [202, 196, 230], 14),
    cube([-3., 23., 4.51], [-1., 25., 4.6], [92, 228, 238], 0),
    cube([1., 23., 4.51], [3., 25., 4.6], [92, 228, 238], 0),
    cube([-2.5, 21., 4.51], [2.5, 21.8, 4.6], [128, 120, 170], 0),
    // v0.18 Track 6: swept brows and pale eye glints.
    cube([-3.5, 25.3, 4.51], [-0.8, 25.9, 4.6], [120, 110, 170], 0),
    cube([0.8, 25.3, 4.51], [3.5, 25.9, 4.6], [120, 110, 170], 0),
    cube([-2.6, 24.2, 4.6], [-2.1, 24.7, 4.65], [230, 252, 255], 0),
    cube([1.4, 24.2, 4.6], [1.9, 24.7, 4.65], [230, 252, 255], 0),
];
// v0.18 Track 6: a paler gust band round each tier.
const TIER_TOP: &[Cuboid] = &[
    cube([-5., 14., -5.], [5., 19., 5.], [176, 200, 228], 22),
    cube([-5.05, 16., -5.05], [5.05, 16.6, 5.05], [214, 228, 244], 0),
];
const TIER_MID: &[Cuboid] = &[
    cube([-3.5, 8., -3.5], [3.5, 14., 3.5], [160, 188, 222], 22),
    cube([-3.55, 10.5, -3.55], [3.55, 11.1, 3.55], [206, 222, 240], 0),
];
const TIER_LOW: &[Cuboid] = &[cube([-2., 0., -2.], [2., 8., 2.], [146, 178, 214], 22)];
const ROD: &[Cuboid] = &[cube([-0.7, 0., -0.7], [0.7, 8., 0.7], [224, 230, 244], 10)];
/// `BreezeAnimation.IDLE`'s length in seconds.
const IDLE: f32 = 2.0;

/// A point along a closed path of `(time, x, z)` keyframes (linear).
fn path(t: f32, keys: &[(f32, f32, f32)]) -> (f32, f32) {
    let i = keys.iter().rposition(|k| k.0 <= t).unwrap_or(0).min(keys.len() - 2);
    let ((t0, x0, z0), (t1, x1, z1)) = (keys[i], keys[i + 1]);
    let k = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    (x0 + (x1 - x0) * k, z0 + (z1 - z0) * k)
}

/// v1.0 gap Track 7b: `BreezeModel` running `BreezeAnimation.IDLE`, the
/// 2-second loop `Breeze` starts on its first tick (from its own
/// `tickCount`): the rods turn three times round per loop and sink a pixel
/// at its middle while the head rises one; the top and middle wind tiers
/// each trace a square half a pixel out, a quarter loop apart. Java's wind
/// texture scrolls where this model's tiers turn. Inhaling and shooting
/// keep this game's own head tilt.
pub(super) fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
    use crate::entity::breeze::Pose;
    let pose = m.breeze.as_ref().map_or(Pose::Standing, |s| s.pose);
    let spin = time * 6.;
    let head = match pose {
        Pose::Inhaling | Pose::Shooting => Quat::from_rotation_x(-0.25 + (time * 18.).sin() * 0.05),
        _ => Quat::IDENTITY,
    };
    let t = m.loop_time(m.anim.idle_start, IDLE, alpha).unwrap_or(0.0) * IDLE;
    let mid = 1.0 - (t - 1.0).abs();
    let turn = -t / IDLE * 3.0 * std::f32::consts::TAU;
    // Java's keyframe x and z, z turned to this model's front.
    let (tx, tz) = path(
        t,
        &[(0.0, 0.5, 0.0), (0.25, 0.5, -0.5), (0.75, -0.5, -0.5), (1.25, -0.5, 0.5), (1.75, 0.5, 0.5), (2.0, 0.5, 0.0)],
    );
    let (mx, mz) = path(t, &[(0.0, 0.5, -0.5), (0.5, -0.5, -0.5), (1.0, -0.5, 0.5), (1.5, 0.5, 0.5), (2.0, 0.5, -0.5)]);
    let [r0, r1, r2] = [0., 2.094, 4.189].map(|a| {
        let a = a + turn;
        part(ROD, [a.cos() * 7., 14. - mid, a.sin() * 7.], Quat::from_rotation_z(0.25 * a.sin()))
    });
    parts![
        part(HEAD, [0., mid, 0.], head),
        part(TIER_TOP, [tx, 0., -tz], Quat::from_rotation_y(spin)),
        part(TIER_MID, [mx, 0., -mz], Quat::from_rotation_y(-spin * 1.3)),
        part(TIER_LOW, [0., 0., 0.], Quat::from_rotation_y(spin * 1.7)),
        r0,
        r1,
        r2
    ]
}
