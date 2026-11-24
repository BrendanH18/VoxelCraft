//! v0.16 Track C: an original box model of the Wither at full size (Java's
//! model is drawn at twice its pixels): three skulls on a shoulder bar,
//! a swaying spine with ribs and a tail. Vertex colours only. Also its
//! skulls in flight.
use super::*;

const BONE: Rgb = [46, 46, 50];
const BONE_LIGHT: Rgb = [64, 64, 70];
const SOCKET: Rgb = [16, 16, 18];
const GLINT: Rgb = [210, 210, 220];

/// The centre head (a 16-pixel cube) with its face on +Z.
const CENTER_HEAD: &[Cuboid] = &[
    cube([-8., -8., -8.], [8., 8., 8.], BONE, 26),
    cube([-6., 0., 8.], [-2., 3., 8.4], SOCKET, 0),
    cube([2., 0., 8.], [6., 3., 8.4], SOCKET, 0),
    cube([-4.5, 1., 8.2], [-3.5, 2., 8.6], GLINT, 0),
    cube([3.5, 1., 8.2], [4.5, 2., 8.6], GLINT, 0),
    cube([-5., -6., 8.], [5., -4., 8.4], SOCKET, 0),
];
/// A side head (12 pixels).
const SIDE_HEAD: &[Cuboid] = &[
    cube([-6., -6., -6.], [6., 6., 6.], BONE, 26),
    cube([-4.5, 0., 6.], [-1.5, 2.5, 6.4], SOCKET, 0),
    cube([1.5, 0., 6.], [4.5, 2.5, 6.4], SOCKET, 0),
    cube([-3.5, -4.5, 6.], [3.5, -3., 6.4], SOCKET, 0),
];
const SHOULDERS: &[Cuboid] = &[cube([-20., 34., -1.], [20., 40., 5.], BONE_LIGHT, 22)];
/// The spine hangs from the shoulders (pivot at its top) with three ribs.
const RIBCAGE: &[Cuboid] = &[
    cube([-3., -20., -3.], [3., 0., 3.], BONE, 22),
    cube([-11., -6., -2.], [11., -2., 2.], BONE_LIGHT, 18),
    cube([-11., -11., -2.], [11., -7., 2.], BONE_LIGHT, 18),
    cube([-11., -16., -2.], [11., -12., 2.], BONE_LIGHT, 18),
];
const TAIL: &[Cuboid] = &[cube([-3., -12., -3.], [3., 0., 3.], BONE, 22)];

/// The Wither's parts: the ribcage and tail sway (`ageInTicks * 0.1`),
/// the centre head follows the body's head turn and the side heads their
/// own targets.
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let anim = (time * 20.0 * 0.1).cos();
    let rib = (0.065 + 0.05 * anim) * PI;
    let tail = (0.265 + 0.1 * anim) * PI;
    let spine_top = Vec3::new(0.0, 34.0, 2.0);
    let tail_at = spine_top + Quat::from_rotation_x(rib) * Vec3::new(0.0, -20.0, 0.0);
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let (left, right) = m.wither.as_ref().map_or(((0.0, 0.0), (0.0, 0.0)), |s| (s.head_rot[0], s.head_rot[1]));
    let body_yrot = m.yaw.to_degrees() - 90.0;
    let side = |(yrot, xrot): (f32, f32)| {
        Quat::from_rotation_y(-(yrot - body_yrot).to_radians()) * Quat::from_rotation_x(xrot.to_radians())
    };
    parts![
        part(SHOULDERS, [0.0; 3], Quat::IDENTITY),
        part(RIBCAGE, spine_top.to_array(), Quat::from_rotation_x(rib)),
        part(TAIL, tail_at.to_array(), Quat::from_rotation_x(tail)),
        part(CENTER_HEAD, [0.0, 48.0, 0.0], head),
        part(SIDE_HEAD, [-20.8, 35.2, 0.0], side(left)),
        part(SIDE_HEAD, [20.8, 35.2, 0.0], side(right)),
    ]
}

/// Skulls in flight: a dark cube, blue-white for a dangerous one, turning
/// along its path.
pub fn build_wither_skulls(
    skulls: &[super::super::wither::Skull],
    camera: DVec3,
    time: f32,
    alpha: f64,
    out: &mut EntityMesh,
) {
    const DARK: &[Cuboid] = &[
        cube([-4., -4., -4.], [4., 4., 4.], [40, 40, 44], 30),
        cube([-3., 0., 4.], [-1., 2., 4.3], [12, 12, 14], 0),
        cube([1., 0., 4.], [3., 2., 4.3], [12, 12, 14], 0),
    ];
    const BLUE: &[Cuboid] = &[
        cube([-4., -4., -4.], [4., 4., 4.], [120, 170, 230], 30),
        cube([-3., 0., 4.], [-1., 2., 4.3], [30, 40, 70], 0),
        cube([1., 0., 4.], [3., 2., 4.3], [30, 40, 70], 0),
    ];
    for (i, k) in skulls.iter().enumerate() {
        let rel = (k.previous_pos.lerp(k.pos, alpha) - camera).as_vec3();
        let rot = Quat::from_rotation_arc(Vec3::Z, k.heading()) * Quat::from_rotation_z(time * 4.0 + i as f32);
        let model = if k.dangerous { BLUE } else { DARK };
        // An 8-pixel skull at 0.6: about the skull's 0.3125-block box.
        for (j, c) in model.iter().enumerate() {
            push_cuboid(
                out,
                c,
                &|v: Vec3| rel + rot * v * 0.6 / 16.0,
                rot,
                ([255, 255, 0, if k.dangerous { 120 } else { 0 }], 0),
                (FIRE, 0.0),
                j as f32,
            );
        }
    }
}

/// v0.16 Track C: beacon beams (`BeaconRenderer`): per section a spinning
/// inner column of the section's colour and a wider translucent glow,
/// both drawn at full brightness; the last section reaches the top of the
/// world. Only beacons within `max_dist` horizontally are drawn.
pub fn build_beacon_beams(world: &crate::world::World, camera: DVec3, time: f32, max_dist: f32, out: &mut EntityMesh) {
    let top = world.max_y() as f32;
    for (pos, sections) in world.beacon_beams() {
        let base = pos.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        if (base - camera).with_y(0.0).length() > f64::from(max_dist) + 1.0 {
            continue;
        }
        let rel = (base - camera).as_vec3();
        let spin = Quat::from_rotation_y(time * 2.25 * 0.1);
        let mut y = 0.0f32;
        for (i, s) in sections.iter().enumerate() {
            let last = i + 1 == sections.len();
            let height = if last { (top - pos.y as f32 - y).max(s.height as f32) } else { s.height as f32 };
            // Model pixels: 16 to a block.
            let (lo, hi) = (y * 16.0, (y + height) * 16.0);
            let inner = Cuboid {
                min: [-3.2, lo, -3.2],
                max: [3.2, hi, 3.2],
                color: s.color,
                noise: 10,
                translucent: true,
                pattern: 0,
            };
            let glow = Cuboid {
                min: [-4.0, lo, -4.0],
                max: [4.0, hi, 4.0],
                color: s.color,
                noise: 0,
                translucent: true,
                pattern: 0,
            };
            for (j, c) in [inner, glow].iter().enumerate() {
                push_cuboid(
                    out,
                    c,
                    &|v: Vec3| rel + spin * v / 16.0,
                    spin,
                    ([255, 255, 0, 255], 255),
                    (FIRE, 0.0),
                    (i * 2 + j) as f32,
                );
            }
            y += height;
        }
    }
}
