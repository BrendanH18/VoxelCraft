//! v0.18 Track 4: the default player in the inventory screens
//! (`InventoryScreen.extractEntityInInventoryFollowsMouse`), turning toward
//! the pointer. The figure is VoxelCraft's own default skin
//! (`entity::player_model::skin_pixels`, Java's 64 x 64 layout), drawn as
//! one flat quad per skin texel of the base layer: an orthographic view
//! like Java's GUI, faces sorted back to front and lit a little by facing.
//!
//! Java's numbers (`tests/fixtures/java/26.3/creative_layout.json`): the
//! box's centre minus the pointer, over 40, through `atan`, times 20 gives
//! degrees; the body turns by the horizontal one, the head turns as much
//! again and pitches by the vertical one, and the whole figure tilts by it.
//! `size` is GUI pixels per block; the feet stand `(height / 2 + offset_y)`
//! blocks below the box's centre. Worn armour, held items and the outer
//! skin layer are not drawn (they are in Java); custom skins stay deferred.

use std::sync::OnceLock;

use glam::Vec3;

use crate::render::ui::{SOLID, Ui};

/// `Avatar`'s 0.9375 model scale: 32 model pixels make 1.875 blocks.
const MODEL_SCALE: f32 = 0.9375;
/// The player's 1.8-block box (`EntityType.PLAYER`).
const HEIGHT: f32 = 1.8;

/// A box of the humanoid model: its corner and size in model pixels (feet
/// at y 0, facing +z, toward the viewer), the skin texture's corner and
/// whether it turns with the head.
struct Part {
    min: [f32; 3],
    size: [f32; 3],
    uv: [usize; 2],
    head: bool,
}

const PARTS: [Part; 6] = [
    Part { min: [-4.0, 24.0, -4.0], size: [8.0, 8.0, 8.0], uv: [0, 0], head: true },
    Part { min: [-4.0, 12.0, -2.0], size: [8.0, 12.0, 4.0], uv: [16, 16], head: false },
    // The right arm and leg are on the viewer's left.
    Part { min: [-8.0, 12.0, -2.0], size: [4.0, 12.0, 4.0], uv: [40, 16], head: false },
    Part { min: [4.0, 12.0, -2.0], size: [4.0, 12.0, 4.0], uv: [32, 48], head: false },
    Part { min: [-4.0, 0.0, -2.0], size: [4.0, 12.0, 4.0], uv: [0, 16], head: false },
    Part { min: [0.0, 0.0, -2.0], size: [4.0, 12.0, 4.0], uv: [16, 48], head: false },
];

fn skin() -> &'static [u8] {
    static SKIN: OnceLock<Vec<u8>> = OnceLock::new();
    SKIN.get_or_init(crate::entity::player_model::skin_pixels)
}

/// Java's angles for a pointer at `mouse` over the box `(x0, y0, x1, y1)`:
/// (body yaw, head yaw relative to it, head pitch up, figure tilt), radians.
pub(super) fn angles((x0, y0, x1, y1): (f32, f32, f32, f32), mouse: (f32, f32)) -> (f32, f32, f32, f32) {
    let x = ((x0 + x1) / 2.0 - mouse.0) / 40.0;
    let y = ((y0 + y1) / 2.0 - mouse.1) / 40.0;
    let (yaw, pitch) = (x.atan() * 20f32.to_radians(), y.atan() * 20f32.to_radians());
    (yaw, yaw, pitch, pitch)
}

/// Draws the default player standing in `bounds`, `size` GUI pixels per
/// block, its feet `offset_y` blocks below the box's half height.
pub(super) fn player(ui: &mut Ui, bounds: (f32, f32, f32, f32), size: f32, offset_y: f32, mouse: (f32, f32)) {
    let (yaw, head_yaw, pitch, tilt) = angles(bounds, mouse);
    let (x0, y0, x1, y1) = bounds;
    let centre_x = (x0 + x1) / 2.0;
    let feet_y = (y0 + y1) / 2.0 + (HEIGHT / 2.0 + offset_y) * size;
    let px = size / 16.0 * MODEL_SCALE;
    let turn_y = |p: Vec3, a: f32| Vec3::new(p.x * a.cos() - p.z * a.sin(), p.y, p.x * a.sin() + p.z * a.cos());
    let turn_x = |p: Vec3, a: f32| Vec3::new(p.x, p.y * a.cos() + p.z * a.sin(), -p.y * a.sin() + p.z * a.cos());
    let neck = Vec3::new(0.0, 24.0, 0.0);
    let middle = Vec3::new(0.0, 16.0, 0.0);
    let place = |p: Vec3, head: bool| {
        let p = if head { turn_y(turn_x(p - neck, pitch), head_yaw) + neck } else { p };
        turn_x(turn_y(p, yaw) - middle, tilt) + middle
    };
    // Faces: (corner, u axis, v axis, texture rectangle (u, v, w, h), outward normal, light).
    let mut faces: Vec<(f32, [Vec3; 4], [usize; 4], f32)> = Vec::with_capacity(36);
    for part in &PARTS {
        let [w, h, d] = part.size;
        let m = Vec3::from(part.min);
        let (wi, hi, di) = (w as usize, h as usize, d as usize);
        let [u, v] = part.uv;
        // Each face: its top-left corner as seen from outside, the u and v
        // directions, and its skin rectangle.
        let list = [
            // Top: u along +x, v toward the front.
            (m + Vec3::new(0.0, h, 0.0), Vec3::X * w, Vec3::Z * d, [u + di, v, wi, di], 1.0),
            // Bottom.
            (m + Vec3::new(0.0, 0.0, d), Vec3::X * w, -Vec3::Z * d, [u + di + wi, v, wi, di], 0.5),
            // Right side (-x), seen from the viewer's left: back to front.
            (m + Vec3::new(0.0, h, 0.0), Vec3::Z * d, -Vec3::Y * h, [u, v + di, di, hi], 0.7),
            // Front (+z).
            (m + Vec3::new(0.0, h, d), Vec3::X * w, -Vec3::Y * h, [u + di, v + di, wi, hi], 0.9),
            // Left side (+x): front to back.
            (m + Vec3::new(w, h, d), -Vec3::Z * d, -Vec3::Y * h, [u + di + wi, v + di, di, hi], 0.7),
            // Back (-z): right to left as seen from behind.
            (m + Vec3::new(w, h, 0.0), -Vec3::X * w, -Vec3::Y * h, [u + 2 * di + wi, v + di, wi, hi], 0.6),
        ];
        for (corner, du, dv, rect, light) in list {
            let c = [corner, corner + du, corner + du + dv, corner + dv].map(|p| place(p, part.head));
            // Facing the viewer (+z) in screen orientation (y down): keep it.
            let normal = (c[1] - c[0]).cross(c[3] - c[0]);
            if normal.z <= 0.0 {
                continue;
            }
            let depth = c.iter().map(|p| p.z).sum::<f32>() / 4.0;
            faces.push((depth, c, rect, light));
        }
    }
    faces.sort_by(|a, b| a.0.total_cmp(&b.0));
    let skin = skin();
    let screen = |p: Vec3| [centre_x + p.x * px, feet_y - p.y * px];
    for (_, c, [u, v, w, h], light) in faces {
        let du = (c[1] - c[0]) / w as f32;
        let dv = (c[3] - c[0]) / h as f32;
        for ty in 0..h {
            for tx in 0..w {
                let i = ((v + ty) * 64 + u + tx) * 4;
                let Some(texel) = skin.get(i..i + 4) else { continue };
                if texel[3] < 128 {
                    continue;
                }
                let a = c[0] + du * tx as f32 + dv * ty as f32;
                let colour = [
                    f32::from(texel[0]) / 255.0 * light,
                    f32::from(texel[1]) / 255.0 * light,
                    f32::from(texel[2]) / 255.0 * light,
                    1.0,
                ];
                ui.quad([screen(a), screen(a + du), screen(a + du + dv), screen(a + dv)], [[0.0; 2]; 4], SOLID, colour);
            }
        }
    }
}

/// `entity.wgsl`'s `hash2`: the per-texel brightness noise of mob boxes.
fn hash2(x: f32, y: f32) -> f32 {
    let fract = |v: f32| v - v.floor();
    let (qx, qy) = (fract(x * 123.34), fract(y * 456.21));
    let d = qx * (qx + 45.32) + qy * (qy + 45.32);
    fract((qx + d) * (qy + d))
}

/// sRGB to linear and back, as the entity shader's `pow(c, 2.2)`.
fn linear(c: f32) -> f32 {
    c.max(0.0).powf(2.2)
}
fn gamma(c: f32) -> f32 {
    c.max(0.0).powf(1.0 / 2.2).min(1.0)
}

/// v1.0 gap Track 3b: `extractEntityInInventoryFollowsMouse` for a mob
/// (the mount screens' `size` 17 and `offsetY` 0.25): the mob's model as
/// the world draws it (`entity::model::build`, its boxes and equipment),
/// turned to face the viewer, then by the pointer as the player is.
/// v1.0 gap Track 2c: drawn textured as the player preview is, one quad per
/// model pixel: the box colour with the entity shader's per-texel noise
/// (`hash2` of the texel, scaled by the vertex's noise strength) or its
/// material pattern (material 6: the pattern's grey, neutral at linear
/// 0.5), lit by face. The world's sky and block light aren't applied (the
/// screen lights it evenly, as Java's GUI lighting does).
#[allow(clippy::too_many_arguments)]
pub(super) fn mob(
    ui: &mut Ui,
    m: &crate::entity::Mob,
    bounds: (f32, f32, f32, f32),
    size: f32,
    offset_y: f32,
    mouse: (f32, f32),
) {
    use crate::entity::model::{EntityMesh, build};
    let (yaw, _, _, tilt) = angles(bounds, mouse);
    let (x0, y0, x1, y1) = bounds;
    let height = m.shape().height as f32;
    let centre_x = (x0 + x1) / 2.0;
    let feet_y = (y0 + y1) / 2.0 + (height / 2.0 + offset_y) * size;
    // Vertices come relative to the mob's feet, in blocks.
    let mut mesh = EntityMesh::flat();
    let forward = glam::Vec3::new(m.yaw.cos(), 0.0, m.yaw.sin());
    // v1.0 gap Track 7b: the mob animates on its own clock (`tickCount`).
    build(std::slice::from_ref(m), m.pos, forward, 64.0, 1.0, &mut mesh);
    // Its front toward the viewer (+z), then the pointer's turn and tilt
    // about its middle.
    let face = glam::Quat::from_rotation_arc(forward, Vec3::Z);
    let middle = Vec3::Y * (height / 2.0);
    let turn_y = |p: Vec3, a: f32| Vec3::new(p.x * a.cos() - p.z * a.sin(), p.y, p.x * a.sin() + p.z * a.cos());
    let turn_x = |p: Vec3, a: f32| Vec3::new(p.x, p.y * a.cos() + p.z * a.sin(), -p.y * a.sin() + p.z * a.cos());
    let place = |p: Vec3| turn_x(turn_y(face * p, yaw) - middle, tilt) + middle;
    let mut tris: Vec<(f32, usize)> = Vec::with_capacity(mesh.len() / 3);
    let chunks = mesh.as_chunks::<3>().0;
    for (i, t) in chunks.iter().enumerate() {
        // Every triangle, far to near: the boxes are convex, so the nearer
        // faces cover the far ones.
        let z = t.iter().map(|v| place(Vec3::from(v.pos)).z).sum::<f32>() / 3.0;
        tris.push((z, i));
    }
    tris.sort_by(|a, b| a.0.total_cmp(&b.0));
    let screen = |p: Vec3| [centre_x + p.x * size, feet_y - p.y * size];
    for (_, i) in tris {
        let t = &chunks[i];
        let shade = f32::from(t[0].light[1]) / 255.0;
        let [r, g, b, noise] = t[0].color;
        let base = [r, g, b].map(|c| linear(f32::from(c) / 255.0));
        let strength = f32::from(noise) / 255.0;
        let pattern = (t[0].torch[1] == crate::entity::model::PATTERN_MATERIAL)
            .then(|| u16::from_le_bytes([t[0].torch[2], t[0].torch[3]]));
        let colour_at = |tx: f32, ty: f32| -> [f32; 4] {
            let lit = if let Some(layer) = pattern {
                let px = crate::render::textures::mob_pattern_pixel(
                    layer,
                    tx.rem_euclid(16.0) as usize,
                    ty.rem_euclid(16.0) as usize,
                );
                let grey = linear(f32::from(px[0]) / 255.0) * 2.0;
                base.map(|c| c * grey)
            } else {
                let n = hash2(tx + 0.5, ty + 0.5) - 0.5;
                base.map(|c| c * (1.0 + n * strength))
            };
            let [r, g, b] = lit.map(|c| gamma(c) * shade);
            [r, g, b, 1.0]
        };
        // The affine map from the face's texel coordinates to the model.
        let p = t.map(|v| Vec3::from(v.pos));
        let uv = t.map(|v| glam::Vec2::from(v.uv));
        let (e1, e2, f1, f2) = (p[1] - p[0], p[2] - p[0], uv[1] - uv[0], uv[2] - uv[0]);
        let det = f1.x * f2.y - f1.y * f2.x;
        if det.abs() < 1e-6 {
            // No texel grid on this face: draw it whole in its colour.
            let c = p.map(|q| screen(place(q)));
            ui.quad([c[0], c[1], c[2], c[2]], [[0.0; 2]; 4], SOLID, colour_at(uv[0].x.floor(), uv[0].y.floor()));
            continue;
        }
        let du = (e1 * f2.y - e2 * f1.y) / det;
        let dv = (e2 * f1.x - e1 * f2.x) / det;
        let at = |u: f32, v: f32| screen(place(p[0] + du * (u - uv[0].x) + dv * (v - uv[0].y)));
        let lo = uv[0].min(uv[1]).min(uv[2]).floor();
        let hi = uv[0].max(uv[1]).max(uv[2]).ceil();
        // Barycentric test on each texel's centre, so the two triangles of
        // a face share its texels without drawing one twice.
        let inside = |q: glam::Vec2| {
            let d = q - uv[0];
            let s = (d.x * f2.y - d.y * f2.x) / det;
            let r = (f1.x * d.y - f1.y * d.x) / det;
            s >= -1e-4 && r >= -1e-4 && s + r <= 1.0 + 1e-4
        };
        let mut ty = lo.y;
        while ty < hi.y {
            let mut tx = lo.x;
            while tx < hi.x {
                if inside(glam::Vec2::new(tx + 0.5, ty + 0.5)) {
                    let quad = [at(tx, ty), at(tx + 1.0, ty), at(tx + 1.0, ty + 1.0), at(tx, ty + 1.0)];
                    ui.quad(quad, [[0.0; 2]; 4], SOLID, colour_at(tx, ty));
                }
                tx += 1.0;
            }
            ty += 1.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_turns_toward_the_pointer() {
        let bounds = (26.0, 8.0, 75.0, 78.0);
        // Pointer on the centre: straight ahead.
        assert_eq!(angles(bounds, (50.5, 43.0)), (0.0, 0.0, 0.0, 0.0));
        // Left of and above the centre: turns left (positive) and looks up.
        let (yaw, head, pitch, _) = angles(bounds, (10.5, 3.0));
        assert!((yaw - 1f32.atan() * 20f32.to_radians()).abs() < 1e-6);
        assert_eq!(head, yaw);
        assert!(pitch > 0.0);
        // Far away the turn approaches 20 x 90 degrees' worth of atan: 31.4 degrees.
        let (far, ..) = angles(bounds, (-10_000.0, 43.0));
        assert!(far.to_degrees() < 31.5 && far.to_degrees() > 31.0);
    }

    /// Headless cost of the preview's vertices (reported, not enforced).
    #[test]
    fn preview_cost_is_reported() {
        let mut ui = Ui::with_scale(400.0, 300.0, 1.0);
        let start = std::time::Instant::now();
        let runs = 2000;
        for i in 0..runs {
            ui.verts.clear();
            player(&mut ui, (26.0, 8.0, 75.0, 78.0), 30.0, 0.0625, (i as f32 % 80.0, 40.0));
        }
        let per = start.elapsed().as_secs_f64() / f64::from(runs) * 1e6;
        println!("preview: {} vertices, {per:.1} us per draw", ui.verts.len());
        assert!(ui.verts.len() < 12_000);
    }

    /// v1.0 gap Track 2c: the mount preview is drawn a model pixel at a
    /// time (many more quads than the model has triangles), with the face
    /// noise varying the colour across a face, inside the screen's box.
    #[test]
    fn the_mount_preview_is_textured_per_model_pixel() {
        use crate::entity::{Mob, MobKind};
        let m = Mob::new(MobKind::Horse, glam::DVec3::new(0.5, 64.0, 0.5), 0.0);
        let mut mesh = crate::entity::model::EntityMesh::flat();
        let forward = glam::Vec3::new(m.yaw.cos(), 0.0, m.yaw.sin());
        crate::entity::model::build(std::slice::from_ref(&m), m.pos, forward, 64.0, 1.0, &mut mesh);
        let triangles = mesh.len() / 3;
        let mut ui = Ui::with_scale(400.0, 300.0, 1.0);
        mob(&mut ui, &m, (26.0, 18.0, 78.0, 70.0), 17.0, 0.25, (52.0, 44.0), 0.0);
        let quads = ui.verts.len() / 6;
        assert!(quads > triangles * 2, "{quads} quads for {triangles} triangles");
        let colours: std::collections::HashSet<[u32; 3]> = ui
            .verts
            .iter()
            .map(|v| v.color[..3].iter().map(|c| c.to_bits()).collect::<Vec<_>>().try_into().unwrap())
            .collect();
        assert!(colours.len() > 40, "textured, not flat: {} colours", colours.len());
        for v in &ui.verts {
            let (x, y) = ((v.pos[0] + 1.0) / 2.0 * 400.0, (1.0 - v.pos[1]) / 2.0 * 300.0);
            assert!((0.0..=110.0).contains(&x) && (0.0..=100.0).contains(&y), "{x} {y}");
        }
    }

    #[test]
    fn the_preview_draws_inside_a_reasonable_area() {
        let mut ui = Ui::with_scale(400.0, 300.0, 1.0);
        player(&mut ui, (26.0, 8.0, 75.0, 78.0), 30.0, 0.0625, (50.5, 43.0));
        assert!(!ui.verts.is_empty());
        // Every vertex lands within a few pixels of Java's box.
        for v in &ui.verts {
            let (x, y) = ((v.pos[0] + 1.0) / 2.0 * 400.0, (1.0 - v.pos[1]) / 2.0 * 300.0);
            assert!((20.0..=82.0).contains(&x) && (8.0..=80.0).contains(&y), "{x} {y}");
        }
    }
}
