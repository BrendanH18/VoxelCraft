//! v0.16 Track F: an active conduit's model (`ConduitRenderer`'s parts, in
//! original shapes and colours): a prismarine cage turning about the
//! tilted axis Java turns it about and bobbing with the tick count, and an
//! eye that faces the viewer, sea-blue, red-ringed while the conduit hunts.
//! The shell itself is the block (it stays inside the cage).

use super::*;

const CAGE: Rgb = [92, 170, 152];
const EYE_WHITE: Rgb = [214, 236, 240];
const IRIS: Rgb = [40, 150, 196];
const HUNT: Rgb = [196, 40, 36];
const PUPIL: Rgb = [16, 24, 32];

/// The cage's twelve edges, 8 model pixels to a side, 1 thick.
fn cage() -> [Cuboid; 12] {
    let (a, b) = (-4.0f32, 4.0f32);
    let bar = |min: [f32; 3], max: [f32; 3]| cube(min, max, CAGE, 30);
    [
        bar([a, a, a], [b, a + 1., a + 1.]),
        bar([a, b - 1., a], [b, b, a + 1.]),
        bar([a, a, b - 1.], [b, a + 1., b]),
        bar([a, b - 1., b - 1.], [b, b, b]),
        bar([a, a, a], [a + 1., b, a + 1.]),
        bar([b - 1., a, a], [b, b, a + 1.]),
        bar([a, a, b - 1.], [a + 1., b, b]),
        bar([b - 1., a, b - 1.], [b, b, b]),
        bar([a, a, a], [a + 1., a + 1., b]),
        bar([b - 1., a, a], [b, a + 1., b]),
        bar([a, b - 1., a], [a + 1., b, b]),
        bar([b - 1., b - 1., a], [b, b, b]),
    ]
}

/// Every active conduit within Java's 64 blocks (v1.0 Track 2).
pub fn build_conduits(
    world: &crate::world::World,
    camera: DVec3,
    view: impl Into<crate::entity::draw_distance::DrawDistance>,
    out: &mut EntityMesh,
) {
    let view = view.into();
    let light = ([255, 255, 0, 255], 255);
    for (pos, c) in world.loaded_conduits() {
        if !c.active {
            continue;
        }
        let t = c.tick_count as f32;
        // `ConduitRenderer`: the parts bob with the tick count.
        let bob = (t * 0.1).sin() / 2.0 + 0.5;
        let bob = bob * bob + bob;
        let centre = pos.as_dvec3() + DVec3::new(0.5, 0.3 + f64::from(bob) * 0.2, 0.5);
        // `BlockEntityRenderer.shouldRender` measures from the cell's centre.
        if !view.conduit(pos.as_dvec3() + DVec3::splat(0.5), camera) {
            continue;
        }
        let rel = (centre - camera).as_vec3();
        let axis = Vec3::new(0.5, 1.0, 0.5).normalize();
        let spin = Quat::from_axis_angle(axis, c.active_rotation(0.0).to_radians());
        for (i, bar) in cage().iter().enumerate() {
            push_cuboid(out, bar, &|v: Vec3| rel + spin * v / 16.0, spin, light, ([0.0; 3], 0.0), i as f32);
        }
        // The eye turns to the viewer.
        let to_camera = -rel;
        let yaw = to_camera.x.atan2(to_camera.z);
        let pitch = -(to_camera.y).atan2(to_camera.x.hypot(to_camera.z));
        let face = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        let iris = if c.hunting { HUNT } else { IRIS };
        for (j, (min, max, colour)) in [
            ([-2.0, -2.0, 0.0], [2.0, 2.0, 0.5], EYE_WHITE),
            ([-1.2, -1.2, 0.5], [1.2, 1.2, 0.8], iris),
            ([-0.5, -0.5, 0.8], [0.5, 0.5, 1.0], PUPIL),
        ]
        .into_iter()
        .enumerate()
        {
            let part = cube(min, max, colour, 0);
            push_cuboid(
                out,
                &part,
                &|v: Vec3| rel + face * (v + Vec3::Z * 3.2) / 16.0,
                face,
                light,
                ([0.0; 3], 0.0),
                20.0 + j as f32,
            );
        }
    }
}
