//! Java 26.3 gameplay sky brightness, separate from visual sky colour.
//! The day timeline uses linear float interpolation, then weather alpha blends.
//! Sources: timeline/day.json, KeyframeTrack codec, WeatherAttributes and
//! Level.updateSkyBrightness in the pinned official server artifact.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnLight {
    /// Level.skyDarken for raw brightness (including swamp slime checks).
    pub sky_darken: u8,
    /// Monster.isDarkEnoughToSpawn uses a fixed darkening of ten while
    /// `Level.isThundering()`: the faded thunder level exceeds 0.9.
    pub monster_darken: u8,
}

impl SpawnLight {
    pub const DAY: Self = Self { sky_darken: 0, monster_darken: 0 };
    pub const NIGHT: Self = Self { sky_darken: 11, monster_darken: 11 };

    /// Evaluate the reference day clock and explicitly supplied weather levels:
    /// `rain` is `getRainLevel(1)` and `thunder` is `getThunderLevel(1)` (the
    /// storm level already scaled by rain), as Java's `WeatherAccess` reads
    /// them. Thundering is derived from that faded level, as
    /// `Level.isThundering` does, never from the weather's target state.
    /// Callers own their clock and weather transitions; visual daylight is not
    /// sufficient to reconstruct these inputs. Only dimensions with weather
    /// and the day timeline (the Overworld) use this; see [`Self::for_dimension`].
    pub fn at(ticks: u32, rain: f32, thunder: f32) -> Self {
        const KEYS: [(u32, f32); 4] = [(133, 1.), (11867, 1.), (13670, 0.26666668), (22330, 0.26666668)];
        let tick = ticks % 24000;
        let (lo, hi, sample) = if tick < KEYS[0].0 {
            (KEYS[3], (KEYS[0].0 + 24000, KEYS[0].1), tick + 24000)
        } else {
            let i = KEYS.iter().rposition(|&(t, _)| t <= tick).unwrap();
            let next = KEYS.get(i + 1).copied().unwrap_or((KEYS[0].0 + 24000, KEYS[0].1));
            (KEYS[i], next, tick)
        };
        let alpha = (sample - lo.0) as f32 / (hi.0 - lo.0) as f32;
        let day = lo.1 + alpha * (hi.1 - lo.1);
        let weather_level = |strength: f32| if strength.is_finite() { strength.clamp(0., 1.) } else { 0. };
        let mut level = 15. * day;
        let thunder = weather_level(thunder);
        let rain_only = weather_level(rain) - thunder;
        // WeatherAttributes blends the rain-only layer (rain minus thunder)
        // first, then thunder. Each layer applies its modifier before the
        // state-change lerp; preserve both float rounding steps.
        let lerp = |alpha: f32, from: f32, to: f32| from + alpha * (to - from);
        if rain_only > 0. {
            level = lerp(rain_only, level, lerp(0.3125, level, 4.));
        }
        if thunder > 0. {
            level = lerp(thunder, level, lerp(0.52734375, level, 4.));
        }
        let sky_darken = (15. - level).clamp(0., 15.) as u8;
        let thundering = f64::from(thunder) > crate::simulation::weather::THUNDERING_ABOVE;
        Self { sky_darken, monster_darken: if thundering { 10 } else { sky_darken } }
    }

    /// Gameplay sky darkening of a dimension. Only the Overworld has the day
    /// timeline and weather layers (`Level.canHaveWeather`); the End's sky
    /// light level keeps its default of 15, and the Nether has no skylight.
    pub fn for_dimension(dimension: crate::world::terrain::Dimension, ticks: u32, rain: f32, thunder: f32) -> Self {
        if dimension == crate::world::terrain::Dimension::Overworld {
            Self::at(ticks, rain, thunder)
        } else {
            Self::DAY
        }
    }
}

/// Java's `visual/sun_angle` in degrees for the Overworld day timeline:
/// keyframes 0 then 360 at tick 6000 of a 24,000-tick period, eased by
/// `cubic_bezier [0.362, 0.241, 0.638, 0.759]` and lerped as floats
/// (`AttributeTypes.ANGLE_DEGREES` keyframes use `LerpFunction.ofFloat`).
/// Dimensions without the day timeline keep the attribute default of 0.
pub fn sun_angle(ticks: u32) -> f32 {
    const KEY: u32 = 6000;
    const PERIOD: u32 = 24000;
    let since = (ticks % PERIOD + PERIOD - KEY) % PERIOD;
    if since == 0 {
        // `sample` returns the segment's start value at its first tick.
        return 0.0;
    }
    let alpha = since as f32 / PERIOD as f32;
    let eased = CubicBezier::new(0.362, 0.241, 0.638, 0.759).apply(alpha);
    0.0 + eased * (360.0 - 0.0)
}

/// Java's `EasingType.CubicBezier`: four Newton-Raphson steps, then bisection,
/// in float precision exactly as the reference orders its operations.
struct CubicBezier {
    x: [f32; 3],
    y: [f32; 3],
}

impl CubicBezier {
    fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        let curve = |c1: f32, c2: f32| [3.0 * c1 - 3.0 * c2 + 1.0, -6.0 * c1 + 3.0 * c2, 3.0 * c1];
        Self { x: curve(x1, x2), y: curve(y1, y2) }
    }

    fn sample(c: [f32; 3], t: f32) -> f32 {
        ((c[0] * t + c[1]) * t + c[2]) * t
    }

    fn gradient(c: [f32; 3], t: f32) -> f32 {
        (3.0 * c[0] * t + 2.0 * c[1]) * t + c[2]
    }

    fn apply(&self, x: f32) -> f32 {
        Self::sample(self.y, self.solve_t(x))
    }

    fn solve_t(&self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..4 {
            let err = Self::sample(self.x, t) - x;
            if err.abs() < 1e-5 {
                return t;
            }
            let g = Self::gradient(self.x, t);
            if g < 1e-5 {
                break;
            }
            t -= (err / g).clamp(-0.25, 0.25);
        }
        self.bisect(x, t)
    }

    fn bisect(&self, x: f32, mut t: f32) -> f32 {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        // The reference loops while lo < hi; float halving ends well within
        // this bound for the timeline's monotonic curve.
        for _ in 0..200 {
            if lo >= hi {
                break;
            }
            let err = Self::sample(self.x, t) - x;
            if err.abs() < 1e-5 {
                return t;
            }
            if err < 0.0 {
                lo = t;
            } else {
                hi = t;
            }
            t = (hi + lo) / 2.0;
        }
        t
    }
}

/// Java's `Mth.cos(double)`: a 65,536-entry float sine table indexed by
/// `(long) (x * 10430.378350470453 + 16384.0) & 65535`.
pub fn mth_cos(x: f64) -> f32 {
    const SCALE: f64 = 10430.378350470453;
    let i = (x * SCALE + 16384.0) as i64 & 65535;
    (i as f64 / SCALE).sin() as f32
}

/// Java's `DaylightDetectorBlock.updateSignalStrength`. `effective_sky` is
/// `getEffectiveSkyBrightness`: stored skylight minus the shared
/// `Level.skyDarken` (the same value spawning uses). Unlike spawning, the
/// normal detector also scales by the cosine of the sun angle pulled 20%
/// toward noon; the inverted detector outputs `15 - effective_sky`.
pub fn daylight_detector_power(effective_sky: u8, sun_angle_degrees: f32, inverted: bool) -> u8 {
    let mut power = i32::from(effective_sky);
    if inverted {
        power = 15 - power;
    } else if power > 0 {
        let mut angle = sun_angle_degrees * 0.017453292;
        // The reference's float literals 3.1415927f and 6.2831855f.
        let target = if angle < std::f32::consts::PI { 0.0 } else { std::f32::consts::TAU };
        angle += (target - angle) * 0.2;
        // Java's Math.round(float): floor(x + 1/2), computed exactly.
        power = (f64::from(power as f32 * mth_cos(f64::from(angle))) + 0.5).floor() as i32;
    }
    power.clamp(0, 15) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_night_weather_and_twilight_use_reference_inputs() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/lighting_spawning.json")).unwrap();
        for key in fixture["sky_darken"]["day_multiplier_keyframes"].as_array().unwrap() {
            let tick = key[0].as_u64().unwrap() as u32;
            let multiplier = key[1].as_f64().unwrap() as f32;
            let expected = (15. - 15. * multiplier) as u8;
            assert_eq!(SpawnLight::at(tick, 0., 0.).sky_darken, expected);
        }
        for case in fixture["sky_darken"]["cases"].as_array().unwrap() {
            let thunder = case["thunder"].as_f64().unwrap() as f32;
            let got =
                SpawnLight::at(case["tick"].as_u64().unwrap() as u32, case["rain"].as_f64().unwrap() as f32, thunder);
            // The reference derives thundering from the faded level.
            let thundering = case["thundering"].as_bool().unwrap();
            assert_eq!(f64::from(thunder) > crate::simulation::weather::THUNDERING_ABOVE, thundering, "{case}");
            assert_eq!(got.sky_darken, case["sky_darken"].as_u64().unwrap() as u8, "{case}");
            assert_eq!(got.monster_darken, case["monster_darken"].as_u64().unwrap() as u8, "{case}");
        }
        assert_eq!(SpawnLight::at(6000, 0., 0.), SpawnLight::DAY);
        assert_eq!(SpawnLight::at(18000, 0., 0.), SpawnLight::NIGHT);
        assert_eq!(SpawnLight::at(6000, 1., 0.).sky_darken, 3);
        assert_eq!(SpawnLight::at(6000, 1., 1.), SpawnLight { sky_darken: 5, monster_darken: 10 });
        assert_eq!(SpawnLight::at(18000, 1., 1.), SpawnLight { sky_darken: 11, monster_darken: 10 });
        // Midpoints of both linear transitions, including midnight wrap.
        assert_eq!(SpawnLight::at(12768, 0., 0.).sky_darken, 5);
        assert_eq!(SpawnLight::at(23231, 0., 0.).sky_darken, 5);
        // A storm that is still fading in does not count as thundering, nor
        // does the target state alone: Java compares the faded level.
        assert_eq!(SpawnLight::at(18000, 1., 0.9).monster_darken, 11);
        assert_eq!(SpawnLight::at(18000, 1., 0.900_001).monster_darken, 10);
        assert_eq!(SpawnLight::at(6000, 1., 0.5).monster_darken, SpawnLight::at(6000, 1., 0.5).sky_darken);
        let fixture_threshold = fixture["monster_light"]["thundering_above"].as_f64().unwrap();
        assert_eq!(fixture_threshold, crate::simulation::weather::THUNDERING_ABOVE);
        use crate::world::terrain::Dimension;
        assert_eq!(
            SpawnLight::for_dimension(Dimension::End, 18000, 1., 1.),
            SpawnLight::DAY,
            "no day timeline or weather"
        );
        assert_eq!(SpawnLight::for_dimension(Dimension::Overworld, 18000, 0., 0.), SpawnLight::NIGHT);
        for ticks in 0..24000 {
            assert_eq!(SpawnLight::at(ticks, 0., 0.), SpawnLight::at(ticks + 24000, 0., 0.));
        }
    }

    #[test]
    fn daylight_detectors_match_the_reference_sun_angle_and_cosine_table() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/lighting_spawning.json")).unwrap();
        let cases = fixture["daylight_detector"]["cases"].as_array().unwrap();
        assert!(cases.len() >= 100);
        for case in cases {
            let tick = case["tick"].as_u64().unwrap() as u32;
            let angle = sun_angle(tick);
            assert_eq!(angle, case["sun_angle"].as_f64().unwrap() as f32, "{case}");
            let power = daylight_detector_power(
                case["effective_sky"].as_u64().unwrap() as u8,
                angle,
                case["inverted"].as_bool().unwrap(),
            );
            assert_eq!(u64::from(power), case["power"].as_u64().unwrap(), "{case}");
        }
        // The table cosine is exact at the quarter turns.
        assert_eq!(mth_cos(0.0), 1.0);
        assert_eq!(mth_cos(std::f64::consts::PI), -1.0);
        // The sun angle wraps with the day and rises monotonically after noon.
        let mut last = -1.0;
        for tick in 6000..30000 {
            let a = sun_angle(tick % 24000);
            assert!(a >= last, "{tick}");
            last = a;
        }
    }
}
