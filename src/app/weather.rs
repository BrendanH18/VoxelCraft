//! Camera-facing rain and snow geometry for the desktop client.

use crate::render::weather::WeatherVertex;
pub(super) use crate::simulation::weather::{Precipitation, Weather, precipitation, rain_at};
use crate::world::World;
use crate::world::noise::hash3;
use glam::DVec3;

const HALF_HEIGHT: f64 = 14.0;

/// Camera-facing rain and snow sheets on the columns around the camera,
/// stopping at the first light-blocking block in each, out to Weather Effect
/// Radius `radius` blocks (`weatherRadius`, v1.0 final F6; Java's 3 to 10).
pub(super) fn sheets(world: &World, camera: DVec3, strength: f32, radius: i32, out: &mut Vec<WeatherVertex>) {
    let radius = radius.max(1);
    out.clear();
    if strength <= 0.0 {
        return;
    }
    let c = camera.floor().as_ivec3();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = dx * dx + dz * dz;
            if d2 > radius * radius {
                continue;
            }
            let (x, z) = (c.x + dx, c.z + dz);
            let Some(ground) = world.surface_height(x, z) else { continue };
            let kind = precipitation(world, x, z, ground);
            if kind == Precipitation::None {
                continue;
            }
            let bottom = ((ground + 1) as f64).max(camera.y - HALF_HEIGHT);
            let top = camera.y + HALF_HEIGHT;
            if bottom >= top {
                continue;
            }
            // Face the camera, turning about the vertical axis.
            let centre = DVec3::new(x as f64 + 0.5, 0.0, z as f64 + 0.5);
            let to_cam = DVec3::new(camera.x - centre.x, 0.0, camera.z - centre.z);
            let side = if to_cam.length_squared() > 1e-6 {
                DVec3::new(-to_cam.z, 0.0, to_cam.x).normalize() * 0.5
            } else {
                DVec3::X * 0.5
            };
            // Java's `WeatherEffectRenderer.prepareInstances`: from full
            // (snow 0.8) at the camera to half at the weather radius.
            let max = if kind == Precipitation::Snow { 0.8 } else { 1.0 };
            let near = (d2 as f32 / (radius * radius) as f32).min(1.0);
            let alpha = ((max + (0.5 - max) * near) * strength * 255.0) as u8;
            let snow = if kind == Precipitation::Snow { 255 } else { 0 };
            let seed = (hash3(x, 0, z, 0x5EED) % 1000) as f32 / 10.0;
            let rel = |p: DVec3| (p - camera).as_vec3().to_array();
            let corner = |s: f64, y: f64, u: f32| WeatherVertex {
                pos: rel(centre + side * s + DVec3::Y * y),
                uv: [u, y as f32],
                seed,
                params: [snow, 255, alpha, 0],
            };
            let (a, b) = (corner(-1.0, bottom, 0.0), corner(1.0, bottom, 1.0));
            let (cc, d) = (corner(1.0, top, 1.0), corner(-1.0, top, 0.0));
            out.extend([a, b, cc, a, cc, d]);
        }
    }
}
