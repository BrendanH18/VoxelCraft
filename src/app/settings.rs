//! Player options, kept across worlds in `saves/options.txt` as
//! `key=value` lines. Unknown keys and bad values fall back to defaults.

use std::path::Path;

use crate::audio::SoundSource;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// In 32-block chunks.
    pub render_distance: i32,
    /// In Java 16-block chunks.
    pub simulation_distance: i32,
    /// Vertical field of view, degrees.
    pub fov: f32,
    /// Multiplier on the base mouse sensitivity.
    pub sensitivity: f32,
    /// Master volume, 0..1.
    pub volume: f32,
    /// Music category, multiplied by master volume.
    pub music_volume: f32,
    pub vsync: bool,
    pub enhanced_graphics: bool,
    pub show_fps: bool,
    pub view_bobbing: bool,
    pub particles: crate::particles::Setting,
    pub fullscreen: super::fullscreen::Mode,
    /// Ask GitHub for a newer release on the title screen.
    pub check_updates: bool,
    /// A release the player chose to skip.
    pub skip_update: Option<crate::update::Version>,
    /// The other sound categories, multiplied by master volume
    /// (`volume` and `music_volume` above are Master and Music).
    pub record_volume: f32,
    pub weather_volume: f32,
    pub block_volume: f32,
    pub hostile_volume: f32,
    pub neutral_volume: f32,
    pub player_volume: f32,
    pub ambient_volume: f32,
    pub ui_volume: f32,
    // Display, accessibility and controls (v0.11), under Java's option names.
    /// GUI Scale: 0 is Auto, otherwise a fixed scale (capped to what fits).
    pub gui_scale: u32,
    /// Max Framerate in fps, 10 to 260 in steps of 10; 260 is Unlimited.
    pub max_fps: u32,
    /// Java's gamma, "Brightness": 0 Moody, 0.5 Default, 1 Bright.
    pub brightness: f32,
    /// FOV Effects, 0..1: how much speed, flying and bows change the FOV.
    pub fov_effects: f32,
    /// Distortion Effects, 0..1: the nausea and portal warp.
    pub distortion: f32,
    /// Damage Tilt, 0..1: the camera roll when hurt.
    pub damage_tilt: f32,
    /// Sneak and Sprint: Toggle instead of Hold.
    pub toggle_sneak: bool,
    pub toggle_sprint: bool,
    pub invert_mouse_x: bool,
    pub invert_mouse_y: bool,
    /// Scroll Sensitivity: Java's `mouseWheelSensitivity`, 0.01 to 10.
    pub scroll_sensitivity: f64,
    /// Discrete Scrolling: every wheel event counts as one notch.
    pub discrete_scroll: bool,
    pub keys: super::keys::Bindings,
}

pub const RENDER_DISTANCE: (i32, i32) = (2, 32);
pub const FOV: (f32, f32) = (30.0, 110.0);
pub const SENSITIVITY: (f32, f32) = (0.25, 3.0);
/// Max Framerate's range; the top value is Unlimited.
pub const MAX_FPS: (u32, u32) = (10, 260);
/// Java's Scroll Sensitivity slider: 10^(n/100) for n in -200..=100.
pub const SCROLL_STEPS: (i32, i32) = (-200, 100);

impl Default for Settings {
    fn default() -> Self {
        Self {
            render_distance: 8,
            simulation_distance: crate::world::activity::DEFAULT_SIMULATION_DISTANCE,
            fov: 70.0,
            sensitivity: 1.0,
            volume: 1.0,
            music_volume: 1.0,
            vsync: true,
            enhanced_graphics: true,
            show_fps: true,
            view_bobbing: true,
            particles: Default::default(),
            fullscreen: Default::default(),
            check_updates: true,
            skip_update: None,
            record_volume: 1.0,
            weather_volume: 1.0,
            block_volume: 1.0,
            hostile_volume: 1.0,
            neutral_volume: 1.0,
            player_volume: 1.0,
            ambient_volume: 1.0,
            ui_volume: 1.0,
            gui_scale: 0,
            max_fps: 120,
            brightness: 0.5,
            fov_effects: 1.0,
            distortion: 1.0,
            damage_tilt: 1.0,
            toggle_sneak: false,
            toggle_sprint: false,
            invert_mouse_x: false,
            invert_mouse_y: false,
            scroll_sensitivity: 1.0,
            discrete_scroll: false,
            keys: Default::default(),
        }
    }
}

impl Settings {
    /// The volume of a sound category, 0..1.
    pub fn sound_volume(&self, source: SoundSource) -> f32 {
        *self.sound_volume_ref(source)
    }

    pub fn set_sound_volume(&mut self, source: SoundSource, volume: f32) {
        *self.sound_volume_mut(source) = volume;
    }

    fn sound_volume_ref(&self, source: SoundSource) -> &f32 {
        match source {
            SoundSource::Master => &self.volume,
            SoundSource::Music => &self.music_volume,
            SoundSource::Records => &self.record_volume,
            SoundSource::Weather => &self.weather_volume,
            SoundSource::Blocks => &self.block_volume,
            SoundSource::Hostile => &self.hostile_volume,
            SoundSource::Neutral => &self.neutral_volume,
            SoundSource::Players => &self.player_volume,
            SoundSource::Ambient => &self.ambient_volume,
            SoundSource::Ui => &self.ui_volume,
        }
    }

    fn sound_volume_mut(&mut self, source: SoundSource) -> &mut f32 {
        match source {
            SoundSource::Master => &mut self.volume,
            SoundSource::Music => &mut self.music_volume,
            SoundSource::Records => &mut self.record_volume,
            SoundSource::Weather => &mut self.weather_volume,
            SoundSource::Blocks => &mut self.block_volume,
            SoundSource::Hostile => &mut self.hostile_volume,
            SoundSource::Neutral => &mut self.neutral_volume,
            SoundSource::Players => &mut self.player_volume,
            SoundSource::Ambient => &mut self.ambient_volume,
            SoundSource::Ui => &mut self.ui_volume,
        }
    }

    /// Reads the options file; a missing or unreadable one gives defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|s| Self::parse(&s)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.serialize())
    }

    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            let num = || value.parse::<f32>().ok().filter(|v| v.is_finite());
            match key.trim() {
                "render_distance" => {
                    if let Ok(v) = value.parse::<i32>() {
                        s.render_distance = v;
                    }
                }
                "simulation_distance" => {
                    if let Ok(v) = value.parse::<i32>() {
                        s.simulation_distance = v;
                    }
                }
                "fov" => s.fov = num().unwrap_or(s.fov),
                "sensitivity" => s.sensitivity = num().unwrap_or(s.sensitivity),
                key if let Some(source) = SoundSource::from_option_key(key) => {
                    s.set_sound_volume(source, num().unwrap_or(s.sound_volume(source)))
                }
                "view_bobbing" if value == "true" => s.view_bobbing = true,
                "view_bobbing" if value == "false" => s.view_bobbing = false,
                "vsync" => s.vsync = value != "false",
                "show_fps" if value == "true" => s.show_fps = true,
                "show_fps" if value == "false" => s.show_fps = false,
                "graphics" if value == "classic" => s.enhanced_graphics = false,
                "graphics" if value == "enhanced" => s.enhanced_graphics = true,
                "particles" if value == "all" => s.particles = crate::particles::Setting::All,
                "particles" if value == "decreased" => s.particles = crate::particles::Setting::Decreased,
                "particles" if value == "minimal" => s.particles = crate::particles::Setting::Minimal,
                "fullscreen" => s.fullscreen = super::fullscreen::Mode::parse(value).unwrap_or(s.fullscreen),
                "check_updates" => s.check_updates = value != "false",
                "skip_update" => s.skip_update = crate::update::Version::parse(value),
                key => {
                    s.parse_display_controls(key, value);
                }
            }
        }
        s.clamped()
    }

    /// The display, accessibility and control lines; false for other keys.
    fn parse_display_controls(&mut self, key: &str, value: &str) -> bool {
        let num = || value.parse::<f64>().ok().filter(|v| v.is_finite());
        let flag = |v: &mut bool| match value {
            "true" => *v = true,
            "false" => *v = false,
            _ => {}
        };
        match key {
            "guiScale" => self.gui_scale = value.parse().unwrap_or(self.gui_scale),
            "maxFps" => self.max_fps = value.parse().unwrap_or(self.max_fps),
            "gamma" => self.brightness = num().map_or(self.brightness, |v| v as f32),
            "fovEffectScale" => self.fov_effects = num().map_or(self.fov_effects, |v| v as f32),
            "screenEffectScale" => self.distortion = num().map_or(self.distortion, |v| v as f32),
            "damageTiltStrength" => self.damage_tilt = num().map_or(self.damage_tilt, |v| v as f32),
            "toggleCrouch" => flag(&mut self.toggle_sneak),
            "toggleSprint" => flag(&mut self.toggle_sprint),
            "invertXMouse" => flag(&mut self.invert_mouse_x),
            "invertYMouse" => flag(&mut self.invert_mouse_y),
            "mouseWheelSensitivity" => self.scroll_sensitivity = num().unwrap_or(self.scroll_sensitivity),
            "discrete_mouse_scroll" => flag(&mut self.discrete_scroll),
            _ => return self.keys.parse_line(key, value),
        }
        true
    }

    fn serialize_display_controls(&self, out: &mut String) {
        out.push_str(&format!(
            "guiScale={}\nmaxFps={}\ngamma={}\nfovEffectScale={}\nscreenEffectScale={}\ndamageTiltStrength={}\ntoggleCrouch={}\ntoggleSprint={}\ninvertXMouse={}\ninvertYMouse={}\nmouseWheelSensitivity={:.4}\ndiscrete_mouse_scroll={}\n",
            self.gui_scale,
            self.max_fps,
            self.brightness,
            self.fov_effects,
            self.distortion,
            self.damage_tilt,
            self.toggle_sneak,
            self.toggle_sprint,
            self.invert_mouse_x,
            self.invert_mouse_y,
            self.scroll_sensitivity,
            self.discrete_scroll,
        ));
        self.keys.serialize(out);
    }

    pub fn serialize(&self) -> String {
        let volumes: String = SoundSource::ALL
            .into_iter()
            .filter(|&source| !matches!(source, SoundSource::Master | SoundSource::Music))
            .map(|source| format!("{}={:.2}\n", source.option_key(), self.sound_volume(source)))
            .collect();
        let mut out = format!(
            "render_distance={}\nsimulation_distance={}\nfov={}\nsensitivity={:.2}\nvolume={:.2}\nmusic_volume={:.2}\nvsync={}\ngraphics={}\nshow_fps={}\nview_bobbing={}\nparticles={}\nfullscreen={}\ncheck_updates={}\nskip_update={}\n{volumes}",
            self.render_distance,
            self.simulation_distance,
            self.fov,
            self.sensitivity,
            self.volume,
            self.music_volume,
            self.vsync,
            if self.enhanced_graphics { "enhanced" } else { "classic" },
            self.show_fps,
            self.view_bobbing,
            self.particles.name().to_ascii_lowercase(),
            self.fullscreen.name().to_ascii_lowercase(),
            self.check_updates,
            self.skip_update.map_or(String::new(), |v| v.to_string())
        );
        self.serialize_display_controls(&mut out);
        out
    }

    /// Every value pulled into its valid range.
    pub fn clamped(mut self) -> Self {
        self.render_distance = self.render_distance.clamp(RENDER_DISTANCE.0, RENDER_DISTANCE.1);
        let (lo, hi) = crate::world::activity::SIMULATION_DISTANCE_RANGE;
        self.simulation_distance = self.simulation_distance.clamp(lo, hi);
        self.fov = self.fov.round().clamp(FOV.0, FOV.1);
        self.sensitivity = self.sensitivity.clamp(SENSITIVITY.0, SENSITIVITY.1);
        for source in SoundSource::ALL {
            self.set_sound_volume(source, self.sound_volume(source).clamp(0.0, 1.0));
        }
        self.gui_scale = self.gui_scale.min(MAX_GUI_SCALE);
        // Java stores the framerate in steps of ten.
        self.max_fps = (self.max_fps.clamp(MAX_FPS.0, MAX_FPS.1) + 5) / 10 * 10;
        for v in [&mut self.brightness, &mut self.fov_effects, &mut self.distortion, &mut self.damage_tilt] {
            *v = v.clamp(0.0, 1.0);
        }
        let (lo, hi) = SCROLL_STEPS;
        self.scroll_sensitivity = self.scroll_sensitivity.clamp(scroll_sensitivity(lo), scroll_sensitivity(hi));
        self
    }

    /// The Max Framerate cap, or `None` for Unlimited.
    pub fn frame_cap(&self) -> Option<u32> {
        (self.max_fps < MAX_FPS.1).then_some(self.max_fps)
    }
}

/// Saves `settings` to `path`; a failure is logged and returned as the
/// line an error notice shows.
pub fn save_or_report(settings: &Settings, path: &Path) -> Result<(), String> {
    settings.save(path).map_err(|e| {
        log::error!("failed to save options to {}: {e}", path.display());
        format!("{}: {e}", path.display())
    })
}

/// The largest fixed GUI Scale kept in the file (Java's encodable maximum is
/// far larger; no display reaches this).
pub const MAX_GUI_SCALE: u32 = 64;

/// Java's `logMouse`: a Scroll Sensitivity slider step to its value.
pub fn scroll_sensitivity(step: i32) -> f64 {
    10f64.powf(step as f64 / 100.0)
}

/// Java's `unlogMouse`, rounded rather than floored so a saved step reads
/// back as itself.
pub fn scroll_step(value: f64) -> i32 {
    (value.log10() * 100.0).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_clamps() {
        let s = Settings {
            render_distance: 12,
            simulation_distance: 7,
            fov: 90.0,
            sensitivity: 1.5,
            volume: 0.4,
            music_volume: 0.25,
            vsync: false,
            enhanced_graphics: false,
            show_fps: false,
            view_bobbing: false,
            particles: crate::particles::Setting::Decreased,
            fullscreen: super::super::fullscreen::Mode::Exclusive,
            check_updates: false,
            skip_update: Some(crate::update::Version(0, 11, 0)),
            record_volume: 0.1,
            weather_volume: 0.2,
            block_volume: 0.3,
            hostile_volume: 0.55,
            neutral_volume: 0.65,
            player_volume: 0.75,
            ambient_volume: 0.85,
            ui_volume: 0.0,
            ..Settings::default()
        };
        assert_eq!(Settings::parse(&s.serialize()), s);
        let wild = Settings::parse("render_distance=99\nfov=5\nvolume=nan\nsensitivity=abc\njunk\nunknown=1\n");
        assert_eq!(wild, Settings { render_distance: 32, fov: 30.0, ..Settings::default() });
        assert_eq!(Settings::parse("music_volume=3").music_volume, 1.0);
        assert_eq!(Settings::parse("music_volume=-1").music_volume, 0.0);
        assert_eq!(Settings::parse("music_volume=nan").music_volume, 1.0);
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::parse("skip_update=junk").skip_update, None);
        assert!(Settings::parse("check_updates=junk").check_updates);
    }

    #[test]
    fn every_sound_category_persists_and_old_files_still_load() {
        // A file from before the categories existed: master and music only.
        let old = Settings::parse("volume=0.4\nmusic_volume=0.25\nfov=80\n");
        assert_eq!((old.volume, old.music_volume, old.fov), (0.4, 0.25, 80.0));
        for source in SoundSource::ALL.into_iter().skip(2) {
            assert_eq!(old.sound_volume(source), 1.0, "{source:?} defaults to full volume");
        }
        // Each category has its own key, and each one round-trips alone.
        let text = Settings::default().serialize();
        for source in SoundSource::ALL {
            assert!(
                text.contains(&format!("\n{}=1.00\n", source.option_key())) || text.starts_with(source.option_key())
            );
            let mut s = Settings::default();
            s.set_sound_volume(source, 0.37);
            let back = Settings::parse(&s.serialize());
            assert_eq!(back, s, "{source:?}");
            for other in SoundSource::ALL.into_iter().filter(|&o| o != source) {
                assert_eq!(back.sound_volume(other), 1.0, "{source:?} leaves {other:?} alone");
            }
        }
        // Wild values are pulled into range, one category at a time.
        let wild = Settings::parse("volume_ui=9\nvolume_block=-2\nvolume_weather=nan\nvolume_hostile=0.5\n");
        assert_eq!(wild.ui_volume, 1.0);
        assert_eq!(wild.block_volume, 0.0);
        assert_eq!(wild.weather_volume, 1.0);
        assert_eq!(wild.hostile_volume, 0.5);
        assert_eq!(Settings { ui_volume: 4.0, record_volume: -1.0, ..Settings::default() }.clamped().ui_volume, 1.0);
    }

    #[test]
    fn effect_sliders_keep_their_exact_value_across_a_save() {
        // FOV Effects stores the slider position squared (0.25 -> 0.0625).
        let s = Settings {
            fov_effects: 0.0625,
            distortion: 0.3025,
            damage_tilt: 0.01,
            brightness: 0.37,
            ..Settings::default()
        };
        assert_eq!(Settings::parse(&s.serialize()), s);
    }

    #[test]
    fn display_and_control_options_round_trip() {
        use super::super::keys::{Action, Input};
        let mut keys = super::super::keys::Bindings::default();
        keys.set(Action::Jump, Input::Mouse(winit::event::MouseButton::Back));
        keys.set(Action::Drop, Input::Unbound);
        let s = Settings {
            gui_scale: 3,
            max_fps: 60,
            brightness: 1.0,
            fov_effects: 0.25,
            distortion: 0.0,
            damage_tilt: 0.5,
            toggle_sneak: true,
            toggle_sprint: true,
            invert_mouse_x: true,
            invert_mouse_y: true,
            scroll_sensitivity: scroll_sensitivity(-37),
            discrete_scroll: true,
            keys,
            ..Settings::default()
        };
        let text = s.serialize();
        assert!(text.contains("key_key.jump=key.mouse.4\n") && text.contains("guiScale=3\n"));
        let read = Settings::parse(&text);
        assert_eq!(scroll_step(read.scroll_sensitivity), -37);
        assert_eq!(read, Settings { scroll_sensitivity: read.scroll_sensitivity, ..s });
        assert_eq!(read.frame_cap(), Some(60));
        assert_eq!(Settings::default().frame_cap(), Some(120), "Java's default is 120 fps");
        assert_eq!(Settings { max_fps: 260, ..s }.frame_cap(), None, "260 is Unlimited");
    }

    #[test]
    fn old_files_load_with_new_defaults_and_bad_values_are_clamped() {
        // A v0.10 options.txt: none of the new keys.
        let old = "render_distance=12\nsimulation_distance=8\nfov=90\nsensitivity=1.50\nvolume=0.40\nmusic_volume=0.25\nvsync=false\ngraphics=classic\nshow_fps=false\nview_bobbing=false\nparticles=decreased\nfullscreen=off\ncheck_updates=true\nskip_update=\n";
        let s = Settings::parse(old);
        assert_eq!((s.render_distance, s.fov, s.vsync, s.view_bobbing), (12, 90.0, false, false));
        let d = Settings::default();
        assert_eq!((s.gui_scale, s.max_fps, s.brightness, s.keys), (d.gui_scale, d.max_fps, d.brightness, d.keys));
        let wild = Settings::parse(
            "maxFps=5000\ngamma=7\nguiScale=-1\nfovEffectScale=nan\nmouseWheelSensitivity=1e9\ntoggleCrouch=maybe\nkey_key.forward=nonsense\n",
        );
        assert_eq!(wild.max_fps, 260);
        assert_eq!(wild.brightness, 1.0);
        assert_eq!(wild.gui_scale, 0);
        assert_eq!(wild.fov_effects, 1.0);
        assert_eq!(wild.scroll_sensitivity, 10.0);
        assert!(!wild.toggle_sneak);
        assert_eq!(wild.keys, d.keys);
        assert_eq!(Settings::parse("maxFps=33").max_fps, 30, "steps of ten");
    }
}
