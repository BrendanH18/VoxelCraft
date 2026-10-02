//! Weather: spells of rain between clear skies, like Minecraft. Rain falls
//! as snow in cold biomes and high up, and not at all in deserts, savannas
//! and badlands. It darkens and greys the sky, hides the sun, moon and
//! stars, waters farmland, keeps zombies and skeletons from burning, and
//! drums on the roof.

use glam::DVec3;

use crate::world::World;
use crate::world::noise::splitmix64;

use super::DAY_LENGTH;

/// Clear spells last 1-5 days, rain half a day to a day and a half.
const CLEAR: (f64, f64) = (DAY_LENGTH, 5.0 * DAY_LENGTH);
const RAIN: (f64, f64) = (0.5 * DAY_LENGTH, 1.5 * DAY_LENGTH);
/// Seconds for rain to fade fully in or out.
const FADE: f32 = 10.0;
/// Precipitation falls as snow above this height.
const SNOW_LINE: i32 = 150;

pub struct Weather {
    pub raining: bool,
    /// Seconds until the weather changes.
    pub timer: f64,
    /// Rain strength 0..1, fading in and out.
    pub strength: f32,
    rng: u64,
}

/// What falls from the sky in a column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

impl Weather {
    /// Start a clear spell using a deterministic weather RNG derived from the world seed.
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

    /// Advance the spell timer and rain fade by `dt` game seconds.
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

    /// Restore rain and remaining seconds from a level-file entry, snapping the rain fade.
    /// Leave state unchanged if the entry cannot be parsed.
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
pub fn precipitation(world: &World, x: i32, z: i32, ground: i32) -> Precipitation {
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
pub fn rain_at(world: &World, weather: &Weather, pos: DVec3) -> f32 {
    let p = pos.floor().as_ivec3();
    let ground = world.surface_height(p.x, p.z).unwrap_or(p.y);
    match precipitation(world, p.x, p.z, ground) {
        Precipitation::Rain => weather.strength,
        _ => 0.0,
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
