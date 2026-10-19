//! Weather: spells of rain between clear skies, like Minecraft. Rain and
//! thunder run on independent timers with Java's `ServerLevel` ranges. Rain falls
//! as snow in cold biomes and high up, and not at all in deserts, savannas
//! and badlands. It darkens and greys the sky, hides the sun, moon and
//! stars, waters farmland, keeps zombies and skeletons from burning, and
//! drums on the roof.

use glam::DVec3;

use crate::world::World;
use crate::world::noise::splitmix64;

/// `ServerLevel.RAIN_DELAY`/`RAIN_DURATION`/`THUNDER_DELAY`/`THUNDER_DURATION`
/// in seconds (Java's uniform tick ranges 12000..180000, 12000..24000,
/// 12000..180000 and 3600..15600 at 20 ticks a second; pinned by the lighting
/// fixture's `weather_ticks`). Rain and thunder cycle independently.
const RAIN_DELAY: (f64, f64) = (600.0, 9000.0);
const RAIN_DURATION: (f64, f64) = (600.0, 1200.0);
const THUNDER_DELAY: (f64, f64) = (600.0, 9000.0);
const THUNDER_DURATION: (f64, f64) = (180.0, 780.0);
/// Seconds for rain or thunder to fade fully in or out: Java moves each level
/// by 0.01 every tick, so a full change takes 100 ticks.
const FADE: f32 = 5.0;
/// `Level.isThundering`: `getThunderLevel(1) > 0.9`.
pub const THUNDERING_ABOVE: f64 = 0.9;
/// `Level.isRaining`: `getRainLevel(1) > 0.2`.
pub const RAINING_ABOVE: f64 = 0.2;

pub struct Weather {
    pub raining: bool,
    /// Thunderstorms currently share rain rendering; retained for command
    /// state, saving, and future lightning.
    pub thundering: bool,
    /// Seconds until rain starts or stops.
    pub timer: f64,
    /// Seconds until the storm flag flips; thunder only shows while it rains.
    pub thunder_timer: f64,
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
            thunder_timer: 0.0,
            strength: 0.0,
            thunder: 0.0,
            rng: seed ^ 0x3EA7_4E12,
        };
        w.timer = w.spell(RAIN_DELAY);
        w.thunder_timer = w.spell(THUNDER_DELAY);
        w
    }

    /// A random spell length in `range` seconds.
    fn spell(&mut self, range: (f64, f64)) -> f64 {
        let lo = (range.0 * 20.0) as u64;
        let hi = (range.1 * 20.0) as u64;
        (lo + splitmix64(&mut self.rng) % (hi - lo + 1)) as f64 / 20.0
    }

    /// Advance the spell timers and the rain fade by `dt` game seconds.
    pub fn update(&mut self, dt: f64) {
        self.update_rules(dt, true);
    }

    /// As [`Weather::update`] under the `advance_weather` rule. Java's
    /// `ServerLevel.advanceWeatherCycle` freezes only the spell timers when
    /// it is off: rain and thunder keep fading toward their current state.
    pub fn update_rules(&mut self, dt: f64, advance: bool) {
        if advance {
            self.advance_spells(dt);
        }
        let step = dt as f32 / FADE;
        let fade = |level: f32, on: bool| {
            let target = if on { 1.0 } else { 0.0 };
            if level < target { (level + step).min(target) } else { (level - step).max(target) }
        };
        self.strength = fade(self.strength, self.raining);
        self.thunder = fade(self.thunder, self.thundering);
    }

    fn advance_spells(&mut self, dt: f64) {
        // Java decrements positive timers and flips the flag at zero. It
        // samples the next spell on the following tick, not the toggle tick.
        // Keep the public/save seconds interface, with tick-quantized timers.
        let expired = |timer: &mut f64, on: &mut bool| {
            if *timer <= 0.0 {
                return true;
            }
            *timer = ((*timer * 20.0).round() - dt * 20.0).max(0.0) / 20.0;
            if *timer == 0.0 {
                *on = !*on;
            }
            false
        };
        if expired(&mut self.thunder_timer, &mut self.thundering) {
            self.thunder_timer = self.spell(if self.thundering { THUNDER_DURATION } else { THUNDER_DELAY });
        }
        if expired(&mut self.timer, &mut self.raining) {
            self.timer = self.spell(if self.raining { RAIN_DURATION } else { RAIN_DELAY });
        }
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
        self.timer = self.spell(if raining { RAIN_DURATION } else { RAIN_DELAY });
        self.thunder_timer = self.spell(THUNDER_DELAY);
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
        self.thunder_timer = self.spell(THUNDER_DURATION);
        if now {
            self.thunder = 1.0;
        }
    }

    /// `raining,rain seconds left,thundering,thunder seconds left` for the level file.
    pub fn serialize(&self) -> String {
        format!("{},{:.0},{},{:.0}", self.raining as u8, self.timer, self.thundering as u8, self.thunder_timer)
    }

    /// Restore rain and remaining seconds from a level-file entry, snapping the rain fade.
    /// Leave state unchanged if the entry cannot be parsed.
    pub fn deserialize(&mut self, text: &str) {
        let mut parts = text.split(',');
        if let (Some(r), Some(t)) = (parts.next(), parts.next().and_then(|t| t.parse::<f64>().ok())) {
            self.raining = r == "1";
            self.timer = t.max(1.0);
            self.thundering = parts.next() == Some("1");
            // Older saves have no thunder timer: start a fresh spell.
            self.thunder_timer = match parts.next().and_then(|t| t.parse::<f64>().ok()) {
                Some(t) => t.max(1.0),
                None => self.spell(if self.thundering { THUNDER_DURATION } else { THUNDER_DELAY }),
            };
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
    use super::super::DAY_LENGTH;
    use super::*;

    #[test]
    fn spells_alternate_and_fade() {
        let mut w = Weather::new(1);
        assert!(!w.raining && (RAIN_DELAY.0..=RAIN_DELAY.1).contains(&w.timer));
        w.timer = 0.05;
        w.thunder_timer = 1.0e6;
        w.update(0.1);
        assert!(w.raining && w.strength < 0.03 && w.timer == 0.0);
        w.update(0.0);
        assert!((RAIN_DURATION.0..=RAIN_DURATION.1).contains(&w.timer));
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
        // Java adds 0.01 per tick, so 91 ticks (4.5 s) take the level past 0.9.
        assert!(w.thunder_level() > 0.9 && (45..=46).contains(&steps), "{steps}");
        // Clearing fades both out; the storm stops counting below 0.9.
        w.set(false, false);
        w.update(0.1);
        assert!(!w.thundering && !w.is_thundering() && w.thunder_level() > 0.0);
        w.thunder_timer = 1.0e6;
        w.set_thunder(true);
        assert!(w.is_thundering() && w.rain_level() == 1.0);

        let mut loaded = Weather::new(2);
        loaded.deserialize(&w.serialize());
        assert!(loaded.raining && (loaded.timer - w.timer).abs() < 1.0);
        assert!(loaded.thundering && loaded.is_thundering(), "a saved storm reloads at full strength");
    }

    #[test]
    fn advance_weather_off_freezes_the_spell_timers_but_not_the_fade() {
        // `ServerLevel.advanceWeatherCycle` gates the timers behind the rule
        // and always moves the levels toward the current state.
        let mut w = Weather::new(3);
        w.set(true, false);
        w.timer = 100.0;
        w.thunder_timer = 100.0;
        w.update_rules(1.0, false);
        assert_eq!((w.timer, w.thunder_timer), (100.0, 100.0));
        assert!((w.strength - 0.2).abs() < 1e-6, "rain keeps fading in: {}", w.strength);
        for _ in 0..30 {
            w.update_rules(1.0, false);
        }
        assert_eq!((w.strength, w.timer), (1.0, 100.0));
        w.update_rules(1.0, true);
        assert_eq!(w.timer, 99.0);
        // Expired timers do not flip the weather while the rule is off.
        w.timer = 0.05;
        w.update_rules(1.0, false);
        assert!(w.raining && w.timer == 0.05);
    }

    #[test]
    fn durations_and_fades_follow_the_reference_tick_ranges() {
        let facts: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/lighting_spawning.json")).unwrap();
        let range = |name: &str| {
            let r = facts["weather_ticks"][name].as_array().unwrap();
            (r[0].as_f64().unwrap() / 20.0, r[1].as_f64().unwrap() / 20.0)
        };
        assert_eq!(RAIN_DELAY, range("rain_delay"));
        assert_eq!(RAIN_DURATION, range("rain_duration"));
        assert_eq!(THUNDER_DELAY, range("thunder_delay"));
        assert_eq!(THUNDER_DURATION, range("thunder_duration"));
        // 0.01 per tick at 20 ticks a second: a full fade is 5 seconds.
        let step = facts["weather_ticks"]["level_step_per_tick"].as_f64().unwrap();
        assert!((1.0 / (step * 20.0) - f64::from(FADE)).abs() < 1e-9);
        // A day is Java's 24000 ticks, 20 minutes.
        assert_eq!(DAY_LENGTH * 20.0, 24_000.0);
        let mut w = Weather::new(263);
        for _ in 0..1000 {
            for range in [RAIN_DELAY, RAIN_DURATION, THUNDER_DELAY, THUNDER_DURATION] {
                let t = w.spell(range);
                assert!((range.0..=range.1).contains(&t));
                assert!((t * 20.0 - (t * 20.0).round()).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn rain_and_thunder_cycle_on_independent_timers() {
        let mut w = Weather::new(9);
        assert!((THUNDER_DELAY.0..=THUNDER_DELAY.1).contains(&w.thunder_timer));
        w.timer = 1.0e6;
        w.thunder_timer = 0.5;
        w.update(1.0);
        assert!(w.thundering && !w.raining, "the storm flag flips without rain");
        assert_eq!(w.thunder_timer, 0.0);
        w.update(0.0);
        assert!((THUNDER_DURATION.0..=THUNDER_DURATION.1).contains(&w.thunder_timer));
        for _ in 0..200 {
            w.update(0.1);
        }
        assert!(!w.is_thundering(), "thunder shows only while it rains");
        w.timer = 0.05;
        w.update(0.1);
        assert!(w.raining && w.thundering);
        let mut loaded = Weather::new(2);
        loaded.deserialize(&w.serialize());
        assert!(loaded.thundering && (loaded.thunder_timer - w.thunder_timer).abs() < 1.0);
        let mut old = Weather::new(3);
        old.deserialize("1,500,0");
        assert!(old.raining && !old.thundering && old.timer == 500.0);
        assert!((THUNDER_DELAY.0..=THUNDER_DELAY.1).contains(&old.thunder_timer), "older saves get a fresh spell");
    }
}
