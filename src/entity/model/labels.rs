//! Camera-facing custom names and untextured lead ropes.
use super::*;
use crate::entity::leash::Leash;

fn quad(out: &mut Vec<EntityVertex>, points: [Vec3; 4], color: [u8; 3], light: [u8; 4], torch: u8) {
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
pub(super) fn attachments(m: &Mob, camera: DVec3, forward: Vec3, rel: Vec3, out: &mut Vec<EntityVertex>) {
    let light = [(m.sky_light.clamp(0.0, 1.0) * 255.0) as u8, 255, 0, 0];
    let torch = (m.block_light.clamp(0.0, 1.0) * 255.0) as u8;
    if let Some(anchor) = m.leash_pos {
        let a = rel + Vec3::Y * m.shape().height as f32 * 0.6;
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
#[cfg(feature = "client")]
fn nameplate(m: &Mob, forward: Vec3, rel: Vec3, out: &mut Vec<EntityVertex>) {
    let Some(text) = m.name.as_str() else { return };
    if rel.length_squared() > 32.0 * 32.0 || !m.alive() {
        return;
    }
    let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
    let pixel = 0.022;
    let origin =
        rel + Vec3::Y * (m.shape().height as f32 + 0.3) - right * (text.chars().count() as f32 * 8.0 * pixel * 0.5);
    let emissive = [255, 255, 0, 255];
    let w = text.chars().count() as f32 * 8.0 * pixel;
    let behind = forward * 0.003;
    quad(
        out,
        [
            origin - right * pixel + behind,
            origin + right * (w + pixel) + behind,
            origin + right * (w + pixel) + Vec3::Y * 9.0 * pixel + behind,
            origin - right * pixel + Vec3::Y * 9.0 * pixel + behind,
        ],
        [25, 25, 25],
        emissive,
        0,
    );
    for (n, ch) in text.chars().enumerate() {
        let code = if ch.is_ascii() { ch as usize } else { '?' as usize };
        let glyph = font8x8::legacy::BASIC_LEGACY[code];
        for (y, row) in glyph.into_iter().enumerate() {
            for x in 0..8 {
                if row & (1 << x) == 0 {
                    continue;
                }
                let p = origin + right * ((n * 8 + x) as f32 * pixel) + Vec3::Y * ((7 - y) as f32 * pixel);
                quad(
                    out,
                    [p, p + right * pixel, p + right * pixel + Vec3::Y * pixel, p + Vec3::Y * pixel],
                    [255; 3],
                    emissive,
                    0,
                );
            }
        }
    }
}
#[cfg(not(feature = "client"))]
fn nameplate(_: &Mob, _: Vec3, _: Vec3, _: &mut Vec<EntityVertex>) {}
