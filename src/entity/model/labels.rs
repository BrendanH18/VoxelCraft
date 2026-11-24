//! Camera-facing custom names and untextured lead ropes.
use super::*;
use crate::entity::leash::Leash;

fn quad(out: &mut EntityMesh, points: [Vec3; 4], color: [u8; 3], light: [u8; 4], torch: u8) {
    // Both windings: rope and glyphs remain visible from either side.
    for i in [0, 1, 2, 0, 2, 3, 2, 1, 0, 3, 2, 0] {
        out.push(EntityVertex {
            pos: points[i].to_array(),
            uv: [0.0; 2],
            color: [color[0], color[1], color[2], 0],
            light,
            torch: [torch, 0, 0, 0],
        });
    }
}
pub(super) fn attachments(m: &Mob, camera: DVec3, forward: Vec3, rel: Vec3, out: &mut EntityMesh) {
    let light = [(m.sky_light.clamp(0.0, 1.0) * 255.0) as u8, 255, 0, 0];
    let torch = (m.block_light.clamp(0.0, 1.0) * 255.0) as u8;
    if let Some(anchor) = m.leash_pos {
        let height = if m.kind == MobKind::CopperGolem { 0.75 * 0.8125 } else { m.shape().height as f32 * 0.6 };
        let a = rel + Vec3::Y * height;
        let b = (anchor - camera).as_vec3();
        let delta = b - a;
        let sag = (delta.length() * 0.08).min(0.7);
        let at = |t: f32| a + delta * t - Vec3::Y * (4.0 * t * (1.0 - t) * sag);
        for i in 0..24 {
            let p = at(i as f32 / 24.0);
            let q = at((i + 1) as f32 / 24.0);
            let color = if i % 2 == 0 { [117, 87, 50] } else { [91, 66, 38] };
            for width in [Vec3::Y * 0.012, Vec3::new(delta.z, 0.0, -delta.x).normalize_or(Vec3::X) * 0.012] {
                quad(out, [p - width, q - width, q + width, p + width], color, light, torch);
            }
        }
        if let Some(Leash::Fence(_)) = m.leash {
            let knot = cube([-2.0, -2.0, -2.0], [2.0, 2.0, 2.0], [117, 87, 50], 10);
            push_cuboid(out, &knot, &|v| b + v / 16.0, Quat::IDENTITY, (light, torch), ([0.0; 3], 0.0), 0.0);
        }
    }
    nameplate(m, forward, rel, out);
}
fn nameplate(m: &Mob, forward: Vec3, rel: Vec3, out: &mut EntityMesh) {
    let Some(text) = m.name.as_str() else { return };
    if rel.length_squared() > 32.0 * 32.0 || !m.alive() {
        return;
    }
    label(text, rel + Vec3::Y * (m.selection_shape().height as f32 + 0.3), forward, out);
}

/// Draws `text` centred above `bottom` (camera-relative), facing a camera looking along `forward`,
/// as Java's dark-backed white name tag, in the shared font (`crate::font`).
#[cfg(feature = "client")]
pub(crate) fn label(text: &str, bottom: Vec3, forward: Vec3, out: &mut EntityMesh) {
    use crate::font;
    let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
    let pixel = 0.022;
    // Java centres on `-font.width(text) / 2` in whole font pixels.
    let width = font::width(text);
    let origin = bottom - right * ((width / 2) as f32 * pixel);
    let emissive = [255, 255, 0, 255];
    // Font rows run down from the line's top: row `r` spans heights
    // `8 - r` to `9 - r` pixels, so row 8, under the descenders, sits on
    // `bottom`. The backing covers rows -1 to 8, as Java's.
    let at = |x: f32, row: f32| origin + right * (x * pixel) + Vec3::Y * ((8.0 - row) * pixel);
    let behind = forward * 0.003;
    let (left, right_edge) = (-1.0, font::ink_width(text) as f32 + 1.0);
    quad(
        out,
        [at(left, 8.0) + behind, at(right_edge, 8.0) + behind, at(right_edge, -2.0) + behind, at(left, -2.0) + behind],
        [25, 25, 25],
        emissive,
        0,
    );
    let mut pen = 0.0;
    for c in text.chars() {
        let glyph = font::glyph(c);
        for (x, y, n) in glyph.runs() {
            let (x0, x1, row) = (pen + x as f32, pen + (x + n) as f32, y as f32);
            quad(out, [at(x0, row), at(x1, row), at(x1, row - 1.0), at(x0, row - 1.0)], [255; 3], emissive, 0);
        }
        pen += glyph.advance() as f32;
    }
}
#[cfg(not(feature = "client"))]
pub(crate) fn label(_: &str, _: Vec3, _: Vec3, _: &mut EntityMesh) {}
