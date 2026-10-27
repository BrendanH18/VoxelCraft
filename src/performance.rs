//! Frame-time statistics with the roadmap's explicit 1% low definition, and
//! the validity rules a live benchmark run must meet before its statistics count.
#[derive(Debug)]
pub struct FrameStats {
    pub frames: usize,
    pub average_fps: f64,
    pub low_one_percent_fps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub worst_ms: f64,
    pub hitches_over_50ms: usize,
}
impl FrameStats {
    pub fn from_milliseconds(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() || samples.iter().any(|x| !x.is_finite() || *x <= 0.) {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let percentile = |percent: usize| sorted[(n * percent).div_ceil(100).saturating_sub(1).min(n - 1)];
        let slow = n.div_ceil(100);
        let slow_mean = sorted[n - slow..].iter().sum::<f64>() / slow as f64;
        Some(Self {
            frames: n,
            average_fps: 1000. / (sorted.iter().sum::<f64>() / n as f64),
            low_one_percent_fps: 1000. / slow_mean,
            p50_ms: percentile(50),
            p95_ms: percentile(95),
            p99_ms: percentile(99),
            worst_ms: sorted[n - 1],
            hitches_over_50ms: sorted.iter().filter(|&&x| x > 50.).count(),
        })
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"frames":self.frames,"average_fps":self.average_fps,"one_percent_low_fps":self.low_one_percent_fps,"p50_ms":self.p50_ms,"p95_ms":self.p95_ms,"p99_ms":self.p99_ms,"worst_ms":self.worst_ms,"hitches_over_50ms":self.hitches_over_50ms})
    }
}

/// Live benchmark scene names accepted by `--benchmark-scene`, matching `benchmarks/scenes.json`.
pub const SCENES: [&str; 8] =
    ["survival", "building", "entities", "automation", "materials", "streaming", "menus", "title"];
/// Scenes that cycle through menu screens. Most of them pause a single-player
/// game, as Java's do, so their runs don't count simulation ticks.
pub const MENU_SCENES: [&str; 2] = ["menus", "title"];
/// Seconds each screen of a menu scene stays open before the next.
pub const MENU_SCREEN_SECONDS: f64 = 2.0;

/// The menus scene's screens, in order: `--open-menu` names, and the inventory.
pub const MENUS_SCREENS: [&str; 9] =
    ["pause", "options", "video", "sound", "controls", "mouse", "keybinds", "accessibility", "inventory"];
/// The title scene's screens, in order: title `--open-menu` names, then the
/// Options screens opened from the title menu.
pub const TITLE_SCREENS: [&str; 12] = [
    "title",
    "select",
    "create",
    "multiplayer",
    "direct",
    "options",
    "video",
    "sound",
    "controls",
    "mouse",
    "keybinds",
    "accessibility",
];

/// The screen a menu scene shows `seconds` after it is ready.
pub fn menu_screen(screens: &[&'static str], seconds: f64) -> &'static str {
    screens[(seconds / MENU_SCREEN_SECONDS).max(0.0) as usize % screens.len()]
}

/// Whether `scene` has a world open; the title scene draws only the sky.
pub fn scene_has_world(scene: &str) -> bool {
    scene != "title"
}

/// Whether `scene`'s runs must show unpaused 20 Hz simulation.
pub fn scene_measures_simulation(scene: &str) -> bool {
    !MENU_SCENES.contains(&scene)
}
/// Mobs the entity scene places; its population must stay near this count.
pub const ENTITY_SCENE_MOBS: usize = 160;
/// Allowed relative drift of the entity scene's population during sampling.
pub const POPULATION_TOLERANCE: f64 = 0.05;
/// Fixed-step simulation rate the sample window must sustain.
pub const TICKS_PER_SECOND: f64 = 20.0;
/// Relative completed-tick tolerance (at least two ticks for window boundaries).
pub const TICK_TOLERANCE: f64 = 0.005;

pub fn parse_scene(text: &str) -> Result<String, String> {
    SCENES.contains(&text).then(|| text.to_string()).ok_or_else(|| format!("unknown benchmark scene '{text}'"))
}

/// Measured seconds after warm-up: 1..=3600.
pub fn parse_duration(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && (1.0..=3600.0).contains(n))
        .ok_or_else(|| "benchmark duration must be 1..3600 seconds".into())
}

/// Warm-up seconds after the scene is ready: 0..=600.
pub fn parse_warmup(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && (0.0..=600.0).contains(n))
        .ok_or_else(|| "benchmark warm-up must be 0..600 seconds".into())
}

/// Whether `ticks` is the 20 Hz count expected for `seconds` of unpaused sampling.
pub fn ticks_within_tolerance(ticks: u64, seconds: f64) -> bool {
    let expected = seconds * TICKS_PER_SECOND;
    (ticks as f64 - expected).abs() <= (expected * TICK_TOLERANCE).max(2.0)
}

/// Whether a measured population stays within [`POPULATION_TOLERANCE`] of `expected`.
pub fn population_within_tolerance(expected: usize, count: usize) -> bool {
    (count as f64 - expected as f64).abs() <= expected as f64 * POPULATION_TOLERANCE
}

/// Everything a scene run must satisfy before its frame statistics may count.
#[derive(Clone, Debug)]
pub struct RunEvidence {
    /// A world is open, so the view distance counts.
    pub world: bool,
    /// The simulation must run unpaused at 20 Hz (not in menu scenes).
    pub simulation: bool,
    pub resolution: (u32, u32),
    pub render_distance_chunks: i32,
    pub warmup_seconds: f64,
    pub requested_seconds: f64,
    pub sample_seconds: f64,
    pub enhanced_graphics: bool,
    /// The resolved present mode does not wait for display refresh.
    pub uncapped_presentation: bool,
    pub completed_ticks: u64,
    pub paused_ms: f64,
    /// Sample intervals in which no surface image was acquired (hidden or occluded window).
    pub unpresented_frames: u64,
    /// Expected population and the counts at sample start and end, for scenes that fix one.
    pub population: Option<(usize, usize, usize)>,
}

impl RunEvidence {
    /// Reasons the reference configuration was not used; empty when eligible.
    pub fn configuration_problems(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.resolution != (1920, 1080) {
            out.push("resolution is not 1920x1080");
        }
        if self.world && self.render_distance_chunks != 8 {
            out.push("view distance is not 256 blocks");
        }
        if self.requested_seconds < 60.0 || self.warmup_seconds < 10.0 {
            out.push("sample or warm-up period is shorter than the protocol");
        }
        if !self.enhanced_graphics {
            out.push("graphics are not enhanced");
        }
        if !self.uncapped_presentation {
            out.push("presentation is synchronized to the display");
        }
        out
    }

    /// Reasons the run's samples are invalid regardless of configuration; empty when valid.
    pub fn run_problems(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.simulation && self.paused_ms > 0.0 {
            out.push("simulation was paused during sampling");
        }
        if self.simulation && !ticks_within_tolerance(self.completed_ticks, self.sample_seconds) {
            out.push("completed simulation ticks differ from 20 per sampled second");
        }
        if self.unpresented_frames > 0 {
            out.push("frames were skipped because the window was hidden or occluded");
        }
        if let Some((expected, start, end)) = self.population
            && !(population_within_tolerance(expected, start) && population_within_tolerance(expected, end))
        {
            out.push("scene population drifted beyond tolerance");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eligible() -> RunEvidence {
        RunEvidence {
            world: true,
            simulation: true,
            resolution: (1920, 1080),
            render_distance_chunks: 8,
            warmup_seconds: 10.0,
            requested_seconds: 60.0,
            sample_seconds: 60.004,
            enhanced_graphics: true,
            uncapped_presentation: true,
            completed_ticks: 1200,
            paused_ms: 0.0,
            unpresented_frames: 0,
            population: Some((160, 160, 158)),
        }
    }

    #[test]
    fn complete_runs_are_eligible() {
        let run = eligible();
        assert!(run.configuration_problems().is_empty());
        assert!(run.run_problems().is_empty());
        assert!(RunEvidence { population: None, ..run }.run_problems().is_empty());
    }

    #[test]
    fn paused_or_short_simulation_is_ineligible() {
        // The recorded M5 streaming run 3: 180 sampled seconds and no ticks.
        let paused = RunEvidence { sample_seconds: 180.01, completed_ticks: 0, paused_ms: 180_010.0, ..eligible() };
        assert_eq!(paused.run_problems().len(), 2);
        let starved = RunEvidence { completed_ticks: 1150, ..eligible() };
        assert_eq!(starved.run_problems(), ["completed simulation ticks differ from 20 per sampled second"]);
        let pause_only = RunEvidence { paused_ms: 1.0, ..eligible() };
        assert_eq!(pause_only.run_problems(), ["simulation was paused during sampling"]);
        assert!(ticks_within_tolerance(1199, 60.0) && ticks_within_tolerance(1202, 60.0));
        assert!(ticks_within_tolerance(3618, 180.0) && !ticks_within_tolerance(3581, 180.0));
    }

    #[test]
    fn population_drift_and_hidden_windows_are_ineligible() {
        // The recorded entity runs ended at 148 of 160 fixture mobs.
        let drifted = RunEvidence { population: Some((160, 160, 148)), ..eligible() };
        assert_eq!(drifted.run_problems(), ["scene population drifted beyond tolerance"]);
        let late = RunEvidence { population: Some((160, 120, 160)), ..eligible() };
        assert_eq!(late.run_problems().len(), 1);
        let hidden = RunEvidence { unpresented_frames: 3, ..eligible() };
        assert_eq!(hidden.run_problems().len(), 1);
    }

    #[test]
    fn configuration_requires_reference_options() {
        let classic = RunEvidence { enhanced_graphics: false, ..eligible() };
        assert_eq!(classic.configuration_problems(), ["graphics are not enhanced"]);
        let vsync = RunEvidence { uncapped_presentation: false, resolution: (2560, 1440), ..eligible() };
        assert_eq!(vsync.configuration_problems().len(), 2);
        let quick = RunEvidence { requested_seconds: 5.0, warmup_seconds: 1.0, ..eligible() };
        assert_eq!(quick.configuration_problems().len(), 1);
    }

    #[test]
    fn menu_scenes_skip_the_simulation_and_the_title_the_view_distance() {
        // The pause menu stops the game, so a menus run has paused time and few ticks.
        let menus = RunEvidence { simulation: false, completed_ticks: 300, paused_ms: 45_000.0, ..eligible() };
        assert!(menus.run_problems().is_empty());
        let title = RunEvidence { world: false, render_distance_chunks: 0, ..menus.clone() };
        assert!(title.configuration_problems().is_empty());
        assert_eq!(RunEvidence { unpresented_frames: 1, ..title.clone() }.run_problems().len(), 1);
        assert_eq!(RunEvidence { resolution: (1600, 900), ..title }.configuration_problems().len(), 1);
        for scene in SCENES {
            assert_eq!(scene_measures_simulation(scene), !MENU_SCENES.contains(&scene), "{scene}");
            assert_eq!(scene_has_world(scene), scene != "title", "{scene}");
        }
    }

    #[test]
    fn specification_matches_the_runtime_scenes() {
        let spec: serde_json::Value = serde_json::from_str(include_str!("../benchmarks/scenes.json")).unwrap();
        let names: Vec<&str> = spec["scenes"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert_eq!(names, SCENES);
        for scene in spec["scenes"].as_array().unwrap() {
            let name = scene["name"].as_str().unwrap();
            assert_eq!(scene["simulation"].as_bool().unwrap_or(true), scene_measures_simulation(name), "{name}");
            assert_eq!(scene["world"].as_bool().unwrap_or(true), scene_has_world(name), "{name}");
        }
        let scene = |name: &str| spec["scenes"].as_array().unwrap().iter().find(|s| s["name"] == name).unwrap();
        let screens = |name: &str| -> Vec<&str> {
            scene(name)["screens"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect()
        };
        assert_eq!(screens("menus"), MENUS_SCREENS);
        assert_eq!(screens("title"), TITLE_SCREENS);
        assert_eq!(menu_screen(&TITLE_SCREENS, 0.0), "title");
        assert_eq!(menu_screen(&TITLE_SCREENS, 2.5), "select");
        assert_eq!(menu_screen(&MENUS_SCREENS, 18.0), "pause", "the cycle repeats");
        let entities = &scene("entities")["fixture"];
        assert_eq!(entities["population"].as_u64(), Some(ENTITY_SCENE_MOBS as u64));
        assert_eq!(entities["population_tolerance"].as_f64(), Some(POPULATION_TOLERANCE));
        assert_eq!(spec["graphics"], "enhanced");
    }

    #[test]
    fn benchmark_flags_validate_names_and_ranges() {
        for scene in SCENES {
            assert_eq!(parse_scene(scene).unwrap(), scene);
        }
        for bad in ["", "Survival", "nether", "survival "] {
            assert!(parse_scene(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_duration("1"), Ok(1.0));
        assert_eq!(parse_duration("3600"), Ok(3600.0));
        for bad in ["0.5", "3601", "-5", "nan", "inf", "", "sixty"] {
            assert!(parse_duration(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_warmup("0"), Ok(0.0));
        assert_eq!(parse_warmup("600"), Ok(600.0));
        for bad in ["-0.1", "600.5", "NaN", "x"] {
            assert!(parse_warmup(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn slowest_one_percent_is_an_average_not_a_percentile() {
        let mut times = vec![2.; 198];
        times.extend([10., 30.]);
        let stats = FrameStats::from_milliseconds(&times).unwrap();
        assert_eq!(stats.low_one_percent_fps, 50.);
        assert_eq!(stats.p99_ms, 2.);
        assert_eq!(stats.worst_ms, 30.);
    }
    #[test]
    fn malformed_samples_do_not_become_performance_claims() {
        for samples in [vec![], vec![0.], vec![-1.], vec![f64::NAN], vec![f64::INFINITY]] {
            assert!(FrameStats::from_milliseconds(&samples).is_none());
        }
    }
}
