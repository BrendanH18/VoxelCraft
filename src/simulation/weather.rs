//! Weather: spells of rain between clear skies, like Minecraft. Rain and
//! thunder run on independent timers with Java's `ServerLevel` ranges. Rain falls
//! as snow in cold biomes and high up, and not at all in deserts, savannas
//! and badlands. It darkens and greys the sky, hides the sun, moon and
//! stars, waters farmland, keeps zombies and skeletons from burning, and
//! drums on the roof.

use glam::DVec3;

use crate::enchant::JavaRandom;
use crate::world::World;

/// `ServerLevel.RAIN_DELAY`, `RAIN_DURATION`, `THUNDER_DELAY` and
/// `THUNDER_DURATION`: `UniformInt`s in game ticks, inclusive (pinned by
/// the lighting fixture's `weather_ticks`). Rain and thunder cycle
/// independently.
pub const RAIN_DELAY: (i32, i32) = (12_000, 180_000);
pub const RAIN_DURATION: (i32, i32) = (12_000, 24_000);
pub const THUNDER_DELAY: (i32, i32) = (12_000, 180_000);
pub const THUNDER_DURATION: (i32, i32) = (3_600, 15_600);
/// Seconds for rain or thunder to fade fully in or out: Java moves each level
/// by 0.01 every tick, so a full change takes 100 ticks.
const FADE: f32 = 5.0;
/// `Level.isThundering`: `getThunderLevel(1) > 0.9`.
pub const THUNDERING_ABOVE: f64 = 0.9;
/// `Level.isRaining`: `getRainLevel(1) > 0.2`.
pub const RAINING_ABOVE: f64 = 0.2;
/// Salt for the weather random's seed (see [`Weather::new`]).
const SEED_SALT: i64 = 0x3EA7_4E12;

/// A timer in seconds as whole game ticks.
fn ticks(seconds: f64) -> i32 {
    (seconds * 20.0).round().max(0.0) as i32
}

pub struct Weather {
    pub raining: bool,
    /// Thunder target state; faded levels gate lightning and spawn light.
    pub thundering: bool,
    /// Seconds until rain starts or stops (Java's `rainTime`, in whole
    /// ticks); 0 draws a new spell on the next tick.
    pub timer: f64,
    /// Seconds until the storm flag flips (`thunderTime`); thunder only
    /// shows while it rains.
    pub thunder_timer: f64,
    /// Seconds of clear weather `/weather clear` holds (`clearWeatherTime`).
    pub clear_timer: f64,
    /// Rain strength 0..1, fading in and out: Java's `rainLevel`.
    pub strength: f32,
    /// Storm strength 0..1, fading in and out with `thundering`: Java's
    /// `thunderLevel` before it is scaled by the rain level.
    pub thunder: f32,
    /// Ticks of `dt` not yet run as whole ticks.
    remainder: f64,
    /// The level's `RandomSource` the spells are drawn from: Java's
    /// `LegacyRandomSource`, which is `java.util.Random`'s generator.
    rng: JavaRandom,
}

/// What falls from the sky in a column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

impl Weather {
    /// A new world's weather: Java's `WeatherData` starts with every timer
    /// at 0, so the first tick draws the thunder spell and then the rain
    /// spell. Java seeds the level's random from the clock; VoxelCraft
    /// seeds it from the world seed so a world's weather repeats.
    pub fn new(seed: u64) -> Self {
        Weather {
            raining: false,
            thundering: false,
            timer: 0.0,
            thunder_timer: 0.0,
            clear_timer: 0.0,
            strength: 0.0,
            thunder: 0.0,
            remainder: 0.0,
            rng: JavaRandom::new(seed as i64 ^ SEED_SALT),
        }
    }

    /// `IntProvider.sample` for a `UniformInt`: `Mth.randomBetweenInclusive`,
    /// `nextInt(max - min + 1) + min`.
    fn sample(&mut self, (min, max): (i32, i32)) -> i32 {
        self.rng.next_bounded(max - min + 1) + min
    }

    /// A spell of `range` ticks, in seconds.
    fn spell(&mut self, range: (i32, i32)) -> f64 {
        f64::from(self.sample(range)) / 20.0
    }

    /// Advance the spell timers and the rain fade by `dt` game seconds.
    pub fn update(&mut self, dt: f64) {
        self.update_rules(dt, true);
    }

    /// As [`Weather::update`] under the `advance_weather` rule. Java's
    /// `ServerLevel.advanceWeatherCycle` freezes only the spell timers when
    /// it is off: rain and thunder keep fading toward their current state.
    pub fn update_rules(&mut self, dt: f64, advance: bool) {
        self.remainder += dt.max(0.0) * 20.0;
        let whole = (self.remainder + 1e-9).floor();
        self.remainder = (self.remainder - whole).max(0.0);
        if advance {
            for _ in 0..whole as u32 {
                self.advance_tick();
            }
        }
        let step = dt as f32 / FADE;
        let fade = |level: f32, on: bool| {
            let target = if on { 1.0 } else { 0.0 };
            if level < target { (level + step).min(target) } else { (level - step).max(target) }
        };
        self.strength = fade(self.strength, self.raining);
        self.thunder = fade(self.thunder, self.thundering);
    }

    /// One tick of `ServerLevel.advanceWeatherCycle`'s timers. While clear
    /// weather is held, each tick sets the thunder and rain timers to 0 if
    /// that weather was on and 1 if not, and turns both off; so when the
    /// hold ends, the next tick flips both on (Java's order, kept). After
    /// that each timer counts down and flips its weather at 0, and a timer
    /// at 0 draws the next spell: thunder first, then rain.
    fn advance_tick(&mut self) {
        let (mut clear, mut thunder, mut rain) =
            (ticks(self.clear_timer), ticks(self.thunder_timer), ticks(self.timer));
        if clear > 0 {
            clear -= 1;
            thunder = if self.thundering { 0 } else { 1 };
            rain = if self.raining { 0 } else { 1 };
            self.thundering = false;
            self.raining = false;
        } else {
            if thunder > 0 {
                thunder -= 1;
                if thunder == 0 {
                    self.thundering = !self.thundering;
                }
            } else {
                thunder = self.sample(if self.thundering { THUNDER_DURATION } else { THUNDER_DELAY });
            }
            if rain > 0 {
                rain -= 1;
                if rain == 0 {
                    self.raining = !self.raining;
                }
            } else {
                rain = self.sample(if self.raining { RAIN_DURATION } else { RAIN_DELAY });
            }
        }
        self.clear_timer = f64::from(clear) / 20.0;
        self.thunder_timer = f64::from(thunder) / 20.0;
        self.timer = f64::from(rain) / 20.0;
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

    /// `/weather clear` (`raining` false) or `/weather rain` with no
    /// duration, as `MinecraftServer.setWeatherParameters` sets them:
    /// clear holds for a `RAIN_DELAY` sample with both timers at 0; rain
    /// runs for a `RAIN_DURATION` sample, the thunder timer set to the same
    /// ticks. `now` skips the fade (VoxelCraft's commands snap the sky).
    pub fn set(&mut self, raining: bool, now: bool) {
        if raining {
            let spell = self.spell(RAIN_DURATION);
            self.set_parameters(0.0, spell, true, false);
        } else {
            let hold = self.spell(RAIN_DELAY);
            self.set_parameters(hold, 0.0, false, false);
        }
        if now {
            self.strength = if raining { 1.0 } else { 0.0 };
            self.thunder = 0.0;
        }
    }

    /// `/weather thunder` with no duration: rain and thunder for a
    /// `THUNDER_DURATION` sample; faded levels gate the per-chunk bolt rolls.
    pub fn set_thunder(&mut self, now: bool) {
        let spell = self.spell(THUNDER_DURATION);
        self.set_parameters(0.0, spell, true, true);
        if now {
            self.strength = 1.0;
            self.thunder = 1.0;
        }
    }

    /// `MinecraftServer.setWeatherParameters(clearTime, rainTime, raining,
    /// thundering)`: the rain time is the thunder time too.
    fn set_parameters(&mut self, clear: f64, time: f64, raining: bool, thundering: bool) {
        self.clear_timer = clear;
        self.timer = time;
        self.thunder_timer = time;
        self.raining = raining;
        self.thundering = thundering;
    }

    /// `ServerLevel.resetWeatherCycle`, when everyone wakes from sleep in
    /// the rain: both off with their timers at 0, so the next tick draws
    /// fresh delays. The levels fade out as usual.
    pub fn reset_cycle(&mut self) {
        self.timer = 0.0;
        self.raining = false;
        self.thunder_timer = 0.0;
        self.thundering = false;
    }

    /// `raining,rain seconds left,thundering,thunder seconds left,clear
    /// seconds left` for the level file; timers keep whole ticks.
    pub fn serialize(&self) -> String {
        format!(
            "{},{:.2},{},{:.2},{:.2}",
            self.raining as u8, self.timer, self.thundering as u8, self.thunder_timer, self.clear_timer
        )
    }

    /// Restore rain and remaining seconds from a level-file entry, snapping the rain fade.
    /// Leave state unchanged if the entry cannot be parsed. Older saves
    /// have no thunder timer (a fresh spell then) or clear timer (none).
    pub fn deserialize(&mut self, text: &str) {
        let mut parts = text.split(',');
        if let (Some(r), Some(t)) = (parts.next(), parts.next().and_then(|t| t.parse::<f64>().ok())) {
            let timer = |t: f64| f64::from(ticks(t)) / 20.0;
            self.raining = r == "1";
            self.timer = timer(t);
            self.thundering = parts.next() == Some("1");
            self.thunder_timer = match parts.next().and_then(|t| t.parse::<f64>().ok()) {
                Some(t) => timer(t),
                None => self.spell(if self.thundering { THUNDER_DURATION } else { THUNDER_DELAY }),
            };
            self.clear_timer = parts.next().and_then(|t| t.parse::<f64>().ok()).map_or(0.0, timer);
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

    /// `n` ticks of weather with the timers advancing.
    fn run(w: &mut Weather, n: u32) {
        for _ in 0..n {
            w.update(0.05);
        }
    }

    #[test]
    fn spells_come_from_javas_random_in_javas_order() {
        // Independent reference: `new java.util.Random(263 ^ 0x3EA74E12)`
        // drawing `nextInt(max - min + 1) + min` for THUNDER_DELAY,
        // RAIN_DELAY, THUNDER_DURATION and RAIN_DELAY (JDK 25 on Boat).
        let mut w = Weather::new(263);
        assert_eq!((w.timer, w.thunder_timer), (0.0, 0.0), "a new world's timers start at 0");
        run(&mut w, 1);
        assert_eq!(ticks(w.thunder_timer), 95_682, "the first tick draws the thunder delay first");
        assert_eq!(ticks(w.timer), 169_548);
        assert!(!w.raining && !w.thundering);
        w.set_thunder(true);
        assert_eq!((ticks(w.timer), ticks(w.thunder_timer)), (9_778, 9_778), "one THUNDER_DURATION for both");
        assert!(w.raining && w.thundering && w.is_thundering());
        w.set(false, true);
        assert_eq!(ticks(w.clear_timer), 120_200, "clear holds for a RAIN_DELAY sample");
        assert_eq!((w.timer, w.thunder_timer), (0.0, 0.0));
    }

    #[test]
    fn timers_count_down_flip_at_zero_and_draw_on_the_next_tick() {
        let mut w = Weather::new(1);
        run(&mut w, 1);
        w.timer = 0.1;
        w.thunder_timer = 1.0e5;
        run(&mut w, 1);
        assert!(!w.raining && ticks(w.timer) == 1);
        run(&mut w, 1);
        assert!(w.raining && w.timer == 0.0, "flips as the timer reaches 0");
        assert!(w.strength < 0.02, "and fades in from there");
        run(&mut w, 1);
        assert!((RAIN_DURATION.0..=RAIN_DURATION.1).contains(&ticks(w.timer)), "a raining spell is a duration");
        assert!(!w.is_raining(), "rain that is still fading in does not count yet");
        run(&mut w, 100);
        assert!((w.strength - 1.0).abs() < 1e-4);
        assert!(w.dim(1.0) < 0.75 && w.overcast([0.5, 0.7, 1.0])[2] < 0.6);
        // Thunder runs on its own timer, raining or not.
        w.thunder_timer = 0.05;
        run(&mut w, 1);
        assert!(w.thundering && w.thunder_timer == 0.0);
        run(&mut w, 1);
        assert!((THUNDER_DURATION.0..=THUNDER_DURATION.1).contains(&ticks(w.thunder_timer)));
        // Java adds 0.01 per tick, so about 91 ticks (counting the flip's,
        // which fades too) take the level past 0.9.
        let mut steps = 1;
        while !w.is_thundering() {
            run(&mut w, 1);
            steps += 1;
        }
        assert!(w.thunder_level() > 0.9 && (90..=92).contains(&steps), "{steps}");
    }

    #[test]
    fn clear_weather_holds_then_turns_both_on() {
        // `advanceWeatherCycle` while `clearWeatherTime > 0`: thunder and
        // rain timers go to 1 (they were off), so the tick after the hold
        // ends flips both on, as in Java.
        let mut w = Weather::new(5);
        w.set_thunder(true);
        w.set(false, true);
        assert!(!w.raining && !w.thundering && w.strength == 0.0);
        w.clear_timer = 0.1;
        run(&mut w, 1);
        assert_eq!((ticks(w.clear_timer), ticks(w.timer), ticks(w.thunder_timer)), (1, 1, 1));
        run(&mut w, 1);
        assert_eq!(w.clear_timer, 0.0);
        assert!(!w.raining && !w.thundering, "still held on the hold's last tick");
        run(&mut w, 1);
        assert!(w.raining && w.thundering, "both flip on once the hold is over");
    }

    #[test]
    fn rain_command_shares_its_spell_and_sleep_resets_the_cycle() {
        let mut w = Weather::new(6);
        w.set(true, true);
        assert!(w.raining && !w.thundering && w.timer == w.thunder_timer);
        assert!((RAIN_DURATION.0..=RAIN_DURATION.1).contains(&ticks(w.timer)));
        // `resetWeatherCycle`: off, timers at 0, a fresh draw next tick.
        w.reset_cycle();
        assert!(!w.raining && w.timer == 0.0 && w.thunder_timer == 0.0);
        assert_eq!(w.strength, 1.0, "the rain fades out rather than stopping at once");
        run(&mut w, 1);
        assert!((RAIN_DELAY.0..=RAIN_DELAY.1).contains(&ticks(w.timer)));
        assert!((THUNDER_DELAY.0..=THUNDER_DELAY.1).contains(&ticks(w.thunder_timer)));
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
            (r[0].as_i64().unwrap() as i32, r[1].as_i64().unwrap() as i32)
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
        let (mut low, mut high) = (i32::MAX, i32::MIN);
        for _ in 0..20_000 {
            let t = w.sample(THUNDER_DURATION);
            assert!((THUNDER_DURATION.0..=THUNDER_DURATION.1).contains(&t));
            (low, high) = (low.min(t), high.max(t));
        }
        assert!(low < THUNDER_DURATION.0 + 50 && high > THUNDER_DURATION.1 - 50, "{low}..{high}");
    }

    #[test]
    fn weather_and_its_clear_hold_survive_save_and_reload() {
        let mut w = Weather::new(9);
        w.set_thunder(true);
        run(&mut w, 7);
        let mut loaded = Weather::new(2);
        loaded.deserialize(&w.serialize());
        assert!(
            loaded.raining && loaded.thundering && loaded.is_thundering(),
            "a saved storm reloads at full strength"
        );
        assert_eq!((loaded.timer, loaded.thunder_timer, loaded.clear_timer), (w.timer, w.thunder_timer, 0.0));
        w.set(false, true);
        run(&mut w, 3);
        let mut loaded = Weather::new(2);
        loaded.deserialize(&w.serialize());
        assert!(!loaded.raining && loaded.clear_timer == w.clear_timer && loaded.clear_timer > 0.0);
        // Older saves: no clear hold, and before that no thunder timer.
        let mut old = Weather::new(3);
        old.deserialize("1,500,1,300");
        assert!(
            old.raining && old.thundering && (old.timer, old.thunder_timer, old.clear_timer) == (500.0, 300.0, 0.0)
        );
        let mut older = Weather::new(3);
        older.deserialize("1,500,0");
        assert!(older.raining && !older.thundering && older.timer == 500.0);
        assert!((THUNDER_DELAY.0..=THUNDER_DELAY.1).contains(&ticks(older.thunder_timer)), "a fresh spell");
    }
}
