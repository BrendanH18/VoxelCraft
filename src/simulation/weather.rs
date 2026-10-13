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
/// `Level.isThundering`: `getThunderLevel(1) > 0.9`.
pub const THUNDERING_ABOVE: f64 = 0.9;
/// `Level.isRaining`: `getRainLevel(1) > 0.2`.
pub const RAINING_ABOVE: f64 = 0.2;

pub struct Weather {
    pub raining: bool,
    /// Thunderstorms currently share rain rendering; retained for command
    /// state, saving, and future lightning.
    pub thundering: bool,
    /// Seconds until the weather changes.
    pub timer: f64,
    /// Rain strength 0..1, fading in and out: Java's `rainLevel`.
    pub strength: f32,
    /// Storm strength 0..1, fading in and out with `thundering`: Java's
    /// `thunderLevel` before it is scaled by the rain level.
    pub thunder: f32,
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
        let mut w = Weather {
            raining: false,
            thundering: false,
            timer: 0.0,
            strength: 0.0,
            thunder: 0.0,
            rng: seed ^ 0x3EA7_4E12,
        };
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
        let step = dt as f32 / FADE;
        let fade = |level: f32, on: bool| {
            let target = if on { 1.0 } else { 0.0 };
            if level < target { (level + step).min(target) } else { (level - step).max(target) }
        };
        self.strength = fade(self.strength, self.raining);
        self.thunder = fade(self.thunder, self.thundering);
    }

    /// Java's `Level.getRainLevel(1)`.
    pub fn rain_level(&self) -> f32 {
        self.strength
    }

    /// Java's `Level.isRaining()` for a dimension with weather: the faded rain
    /// level exceeds 0.2 (double comparison).
    pub fn is_raining(&self) -> bool {
        f64::from(self.strength) > RAINING_ABOVE
    }

    /// Java's `Level.getThunderLevel(1)`: the storm level scaled by rain.
    pub fn thunder_level(&self) -> f32 {
        self.thunder * self.strength
    }

    /// Java's `Level.isThundering()` for a dimension with weather: the
    /// faded thunder level, not the target state, exceeds 0.9 (compared in
    /// double precision, so a float 0.9 does not count).
    pub fn is_thundering(&self) -> bool {
        f64::from(self.thunder_level()) > THUNDERING_ABOVE
    }

    /// Starts or stops rain (`now`: without fading) with a fresh timer.
    pub fn set(&mut self, raining: bool, now: bool) {
        self.raining = raining;
        self.thundering = false;
        self.timer = self.spell(if raining { RAIN } else { CLEAR });
        if now {
            self.strength = if raining { 1.0 } else { 0.0 };
            self.thunder = 0.0;
        }
    }

    /// Starts a thunderstorm; rendering and precipitation match rain until
    /// lightning is implemented.
    pub fn set_thunder(&mut self, now: bool) {
        self.set(true, now);
        self.thundering = true;
        if now {
            self.thunder = 1.0;
        }
    }

    /// `raining,seconds left,thundering` for the level file.
    pub fn serialize(&self) -> String {
        format!("{},{:.0},{}", self.raining as u8, self.timer, self.thundering as u8)
    }

    /// Restore rain and remaining seconds from a level-file entry, snapping the rain fade.
    /// Leave state unchanged if the entry cannot be parsed.
    pub fn deserialize(&mut self, text: &str) {
        let mut parts = text.split(',');
        if let (Some(r), Some(t)) = (parts.next(), parts.next().and_then(|t| t.parse::<f64>().ok())) {
            self.raining = r == "1";
            self.timer = t.max(1.0);
            self.thundering = parts.next() == Some("1") && self.raining;
            self.strength = if self.raining { 1.0 } else { 0.0 };
            self.thunder = if self.thundering { 1.0 } else { 0.0 };
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
    match world.snow_line_at(x, z) {
        // Deserts, savannas and badlands stay dry.
        Some(i16::MAX) | None => Precipitation::None,
        Some(line) if ground >= line as i32 => Precipitation::Snow,
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
        assert!(!w.is_raining(), "rain that is still fading in does not count yet");
        for _ in 0..(FADE as usize * 10) {
            w.update(0.1);
        }
        assert!((w.strength - 1.0).abs() < 1e-4);
        assert!(w.dim(1.0) < 0.75 && w.overcast([0.5, 0.7, 1.0])[2] < 0.6);

        // A storm starting in full rain fades in: Java's isThundering waits
        // until thunderLevel * rainLevel exceeds 0.9.
        w.set_thunder(false);
        assert!(w.thundering && !w.is_thundering() && w.thunder_level() == 0.0);
        assert!(w.is_raining());
        let mut steps = 0;
        while !w.is_thundering() {
            w.update(0.1);
            steps += 1;
        }
        assert!(w.thunder_level() > 0.9 && (90..=91).contains(&steps), "{steps}");
        // Clearing fades both out; the storm stops counting below 0.9.
        w.set(false, false);
        w.update(0.1);
        assert!(!w.thundering && !w.is_thundering() && w.thunder_level() > 0.0);
        w.set_thunder(true);
        assert!(w.is_thundering() && w.rain_level() == 1.0);

        let mut loaded = Weather::new(2);
        loaded.deserialize(&w.serialize());
        assert!(loaded.raining && (loaded.timer - w.timer).abs() < 1.0);
        assert!(loaded.thundering && loaded.is_thundering(), "a saved storm reloads at full strength");
    }
}
