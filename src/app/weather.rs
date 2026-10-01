//! Weather: spells of rain between clear skies, like Minecraft. Rain falls
//! as snow in cold biomes and high up, and not at all in deserts, savannas
//! and badlands. It darkens and greys the sky, hides the sun, moon and
//! stars, waters farmland, keeps zombies and skeletons from burning, and
//! drums on the roof.

use glam::DVec3;

use crate::render::weather::WeatherVertex;
use crate::world::World;
use crate::world::noise::{hash3, splitmix64};

use super::DAY_LENGTH;

/// Clear spells last 1-5 days, rain half a day to a day and a half.
const CLEAR: (f64, f64) = (DAY_LENGTH, 5.0 * DAY_LENGTH);
const RAIN: (f64, f64) = (0.5 * DAY_LENGTH, 1.5 * DAY_LENGTH);
/// Seconds for rain to fade fully in or out.
const FADE: f32 = 10.0;
/// Rain sheets are drawn on columns this far from the camera...
const RADIUS: i32 = 10;
/// ...from this far below the camera to this far above it.
const HALF_HEIGHT: f64 = 14.0;
/// Precipitation falls as snow above this height.
const SNOW_LINE: i32 = 150;

pub(super) struct Weather {
    pub raining: bool,
    /// Seconds until the weather changes.
    pub timer: f64,
    /// Rain strength 0..1, fading in and out.
    pub strength: f32,
    rng: u64,
}

/// What falls from the sky in a column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Precipitation {
    None,
    Rain,
    Snow,
}

impl Weather {
    pub fn new(seed: u64) -> Self {
        let mut w = Weather { raining: false, timer: 0.0, strength: 0.0, rng: seed ^ 0x3EA7_4E12 };
        w.timer = w.spell(CLEAR);
        w
    }

    /// A random spell length in `range` seconds.
    fn spell(&mut self, range: (f64, f64)) -> f64 {
        let r = (splitmix64(&mut self.rng) >> 11) as f64 / (1u64 << 53) as f64;
        range.0 + (range.1 - range.0) * r
    }

    pub fn update(&mut self, dt: f64) {
        self.timer -= dt;
        if self.timer <= 0.0 {
            self.set(!self.raining, false);
        }
        let target = if self.raining { 1.0 } else { 0.0 };
        let step = dt as f32 / FADE;
        self.strength = if self.strength < target {
            (self.strength + step).min(target)
        } else {
            (self.strength - step).max(target)
        };
    }

    /// Starts or stops rain (`now`: without fading) with a fresh timer.
    pub fn set(&mut self, raining: bool, now: bool) {
        self.raining = raining;
        self.timer = self.spell(if raining { RAIN } else { CLEAR });
        if now {
            self.strength = if raining { 1.0 } else { 0.0 };
        }
    }

    /// `raining,seconds left` for the level file.
    pub fn serialize(&self) -> String {
        format!("{},{:.0}", self.raining as u8, self.timer)
    }

    pub fn deserialize(&mut self, text: &str) {
        let mut parts = text.split(',');
        if let (Some(r), Some(t)) = (parts.next(), parts.next().and_then(|t| t.parse::<f64>().ok())) {
            self.raining = r == "1";
            self.timer = t.max(1.0);
            self.strength = if self.raining { 1.0 } else { 0.0 };
        }
    }

    /// Sky and fog colour during rain: greyed and darkened.
    pub fn overcast(&self, c: [f32; 3]) -> [f32; 3] {
        let grey = (c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11) * 0.6;
        c.map(|v| v + (grey - v) * self.strength * 0.85)
    }

    /// Daylight (skylight multiplier) during rain.
    pub fn dim(&self, daylight: f32) -> f32 {
        daylight * (1.0 - 0.3 * self.strength)
    }
}

/// What falls in column (x, z), given the ground height there.
pub(super) fn precipitation(world: &World, x: i32, z: i32, ground: i32) -> Precipitation {
    match world.foliage_at(x, z) {
        // Deserts, savannas and badlands stay dry.
        Some(2) | None => Precipitation::None,
        Some(4) => Precipitation::Snow,
        _ if ground > SNOW_LINE => Precipitation::Snow,
        _ => Precipitation::Rain,
    }
}

/// How hard it rains on the player: the rain strength where they stand,
/// or 0 in a dry biome.
pub(super) fn rain_at(world: &World, weather: &Weather, pos: DVec3) -> f32 {
    let p = pos.floor().as_ivec3();
    let ground = world.surface_height(p.x, p.z).unwrap_or(p.y);
    match precipitation(world, p.x, p.z, ground) {
        Precipitation::Rain => weather.strength,
        _ => 0.0,
    }
}

/// Camera-facing rain and snow sheets on the columns around the camera,
/// stopping at the first light-blocking block in each.
pub(super) fn sheets(world: &World, camera: DVec3, strength: f32, out: &mut Vec<WeatherVertex>) {
    out.clear();
    if strength <= 0.0 {
        return;
    }
    let c = camera.floor().as_ivec3();
    for dz in -RADIUS..=RADIUS {
        for dx in -RADIUS..=RADIUS {
            let d2 = dx * dx + dz * dz;
            if d2 > RADIUS * RADIUS {
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
            let fade = 1.0 - (d2 as f32).sqrt() / (RADIUS as f32 + 1.0);
            let alpha = (strength * fade.sqrt() * 255.0) as u8;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_alternate_and_fade() {
        let mut w = Weather::new(1);
        assert!(!w.raining && (CLEAR.0..=CLEAR.1).contains(&w.timer));
        w.timer = 0.05;
        w.update(0.1);
        assert!(w.raining && w.strength < 0.02 && (RAIN.0..=RAIN.1).contains(&w.timer));
        for _ in 0..(FADE as usize * 10) {
            w.update(0.1);
        }
        assert!((w.strength - 1.0).abs() < 1e-4);
        assert!(w.dim(1.0) < 0.75 && w.overcast([0.5, 0.7, 1.0])[2] < 0.6);

        let mut loaded = Weather::new(2);
        loaded.deserialize(&w.serialize());
        assert!(loaded.raining && (loaded.timer - w.timer).abs() < 1.0);
    }
}
