//! Java 26.3 gameplay sky brightness, separate from visual sky colour.
//! The day timeline uses linear float interpolation, then weather alpha blends.
//! Sources: timeline/day.json, KeyframeTrack codec, WeatherAttributes and
//! Level.updateSkyBrightness in the pinned official server artifact.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnLight {
    /// Level.skyDarken for raw brightness (including swamp slime checks).
    pub sky_darken: u8,
    /// Monster.isDarkEnoughToSpawn uses a fixed darkening during thunder.
    pub monster_darken: u8,
}

impl SpawnLight {
    pub const DAY: Self = Self { sky_darken: 0, monster_darken: 0 };
    pub const NIGHT: Self = Self { sky_darken: 11, monster_darken: 11 };

    /// Evaluate the reference day clock and explicitly supplied weather levels.
    /// Callers own their clock and weather transitions; visual daylight is not
    /// sufficient to reconstruct these inputs.
    pub fn at(ticks: u32, rain: f32, thunder: f32, thundering: bool) -> Self {
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
        Self { sky_darken, monster_darken: if thundering { 10 } else { sky_darken } }
    }
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
            assert_eq!(SpawnLight::at(tick, 0., 0., false).sky_darken, expected);
        }
        for case in fixture["sky_darken"]["cases"].as_array().unwrap() {
            let got = SpawnLight::at(
                case["tick"].as_u64().unwrap() as u32,
                case["rain"].as_f64().unwrap() as f32,
                case["thunder"].as_f64().unwrap() as f32,
                case["thundering"].as_bool().unwrap(),
            );
            assert_eq!(got.sky_darken, case["sky_darken"].as_u64().unwrap() as u8, "{case}");
            assert_eq!(got.monster_darken, case["monster_darken"].as_u64().unwrap() as u8, "{case}");
        }
        assert_eq!(SpawnLight::at(6000, 0., 0., false), SpawnLight::DAY);
        assert_eq!(SpawnLight::at(18000, 0., 0., false), SpawnLight::NIGHT);
        assert_eq!(SpawnLight::at(6000, 1., 0., false).sky_darken, 3);
        assert_eq!(SpawnLight::at(6000, 1., 1., true), SpawnLight { sky_darken: 5, monster_darken: 10 });
        assert_eq!(SpawnLight::at(18000, 1., 1., true), SpawnLight { sky_darken: 11, monster_darken: 10 });
        // Midpoints of both linear transitions, including midnight wrap.
        assert_eq!(SpawnLight::at(12768, 0., 0., false).sky_darken, 5);
        assert_eq!(SpawnLight::at(23231, 0., 0., false).sky_darken, 5);
        for ticks in 0..24000 {
            assert_eq!(SpawnLight::at(ticks, 0., 0., false), SpawnLight::at(ticks + 24000, 0., 0., false));
        }
    }
}
