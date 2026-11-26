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
    /// The compact FPS counter. v0.18 Track 2: off unless a saved
    /// `show_fps` line says otherwise; files that have one keep it.
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
    /// v0.18 Track 9: Java's Closed Captions (`showSubtitles`, off).
    pub show_subtitles: bool,
    /// Text Background Opacity (`textBackgroundOpacity`, 0..1, 0.5): the
    /// text backing where Text Background is Everywhere.
    pub text_background_opacity: f32,
    /// Text Background (`backgroundForChatOnly`): Chat (true, the default)
    /// keeps each overlay's own backing; Everywhere uses the opacity above.
    pub background_for_chat_only: bool,
    /// v0.18 Track 3: Java's Mipmap Levels (`mipmapLevels`, 0..=4, 4): how
    /// many of the block textures' smaller levels distant surfaces may use.
    pub mipmap_levels: u32,
    /// v0.18 Track 2: Menu Background Blur (`menuBackgroundBlurriness`,
    /// 0 to 10, default 5; 0 is OFF): the blur radius behind menus.
    pub menu_blur: u32,
    /// v0.18 Track 7: Panorama Scroll Speed (`panoramaScrollSpeed`, 0..1,
    /// 1): how fast the title panorama turns.
    pub panorama_speed: f64,
    /// v0.18 Track 8: Java's Clouds (`renderClouds`: Fancy).
    pub clouds: voxelcraft::atmosphere::Clouds,
    /// Cloud Distance (`cloudRange`) in Java chunks, 2..128 (64, Java's
    /// Fancy preset; v1.0 Track 2).
    pub cloud_range: u32,
    /// Hide Sky Flashes (`hideLightningFlashes`, off): lightning no longer
    /// lights up the sky; the bolts still show.
    pub hide_lightning_flashes: bool,
    /// v0.18 Track 6: Java's Entity Shadows (`entityShadows`, on).
    pub entity_shadows: bool,
    /// v1.0 Track 9: Java's Attack/Destroy and Use Item/Place Block
    /// (`toggleAttack`, `toggleUse`, both Hold): Toggle makes a press flip
    /// the button's held state, as `ToggleKeyMapping` does.
    pub toggle_attack: bool,
    pub toggle_use: bool,
    /// Auto-Jump (`autoJump`, off in Java 26.3): `LocalPlayer.updateAutoJump`.
    pub auto_jump: bool,
    /// Attack Indicator (`attackIndicator`, Crosshair).
    pub attack_indicator: AttackIndicator,
    pub keys: super::keys::Bindings,
}

/// Java's `AttackIndicatorStatus`, saved as its id (0, 1, 2) like Java's
/// `LEGACY_CODEC`; an id out of range wraps (`ByIdMap.OutOfBoundsStrategy.WRAP`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttackIndicator {
    Off,
    #[default]
    Crosshair,
    Hotbar,
}

impl AttackIndicator {
    pub const ALL: [Self; 3] = [Self::Off, Self::Crosshair, Self::Hotbar];

    pub fn id(self) -> i32 {
        self as i32
    }

    pub fn from_id(id: i32) -> Self {
        Self::ALL[id.rem_euclid(Self::ALL.len() as i32) as usize]
    }

    /// The option button's cycle (`OptionInstance.Enum`): the next value.
    pub fn next(self) -> Self {
        Self::from_id(self.id() + 1)
    }

    /// `options.off`, `options.attack.crosshair`, `options.attack.hotbar`.
    pub fn caption(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Crosshair => "Crosshair",
            Self::Hotbar => "Hotbar",
        }
    }
}

pub const RENDER_DISTANCE: (i32, i32) = (2, 32);
pub const FOV: (f32, f32) = (30.0, 110.0);
pub const SENSITIVITY: (f32, f32) = (0.25, 3.0);
/// Max Framerate's range; the top value is Unlimited.
pub const MAX_FPS: (u32, u32) = (10, 260);
/// Menu Background Blur's range and default (`Options.menuBackgroundBlurriness`).
pub const MENU_BLUR: (u32, u32) = (0, 10);
pub const MENU_BLUR_DEFAULT: u32 = 5;
/// Java's Scroll Sensitivity slider: 10^(n/100) for n in -200..=100.
pub const SCROLL_STEPS: (i32, i32) = (-200, 100);
/// Mipmap Levels' range (Java 26.3; `tests/fixtures/java/26.3/mipmap_levels.json`).
pub const MIPMAP_LEVELS: (u32, u32) = (0, 4);

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
            show_fps: false,
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
            show_subtitles: false,
            text_background_opacity: 0.5,
            background_for_chat_only: true,
            mipmap_levels: MIPMAP_LEVELS.1,
            menu_blur: MENU_BLUR_DEFAULT,
            panorama_speed: voxelcraft::title::options::PANORAMA_SPEED_DEFAULT,
            clouds: voxelcraft::atmosphere::Clouds::Fancy,
            cloud_range: voxelcraft::atmosphere::DEFAULT_CLOUD_RANGE,
            hide_lightning_flashes: false,
            entity_shadows: voxelcraft::entity::shadow::OPTION_DEFAULT,
            toggle_attack: false,
            toggle_use: false,
            auto_jump: false,
            attack_indicator: AttackIndicator::Crosshair,
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
                // Java's key is `bobView`; files before v1.0 wrote `view_bobbing`.
                "view_bobbing" | "bobView" if value == "true" => s.view_bobbing = true,
                "view_bobbing" | "bobView" if value == "false" => s.view_bobbing = false,
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
        s.keys.migrate(text);
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
            "showSubtitles" => flag(&mut self.show_subtitles),
            "textBackgroundOpacity" => {
                self.text_background_opacity = num().map_or(self.text_background_opacity, |v| v as f32)
            }
            "backgroundForChatOnly" => flag(&mut self.background_for_chat_only),
            "mipmapLevels" => self.mipmap_levels = value.parse().unwrap_or(self.mipmap_levels),
            "menuBackgroundBlurriness" => self.menu_blur = value.parse().unwrap_or(self.menu_blur),
            "panoramaScrollSpeed" => self.panorama_speed = num().unwrap_or(self.panorama_speed),
            // Java also reads a boolean here (true: Fancy, false: OFF).
            "renderClouds" => {
                self.clouds = voxelcraft::atmosphere::Clouds::parse(value).unwrap_or(self.clouds);
            }
            "cloudRange" => self.cloud_range = value.parse().unwrap_or(self.cloud_range),
            "hideLightningFlashes" => flag(&mut self.hide_lightning_flashes),
            "entityShadows" => flag(&mut self.entity_shadows),
            "toggleAttack" => flag(&mut self.toggle_attack),
            "toggleUse" => flag(&mut self.toggle_use),
            "autoJump" => flag(&mut self.auto_jump),
            "attackIndicator" => {
                if let Ok(id) = value.parse::<i32>() {
                    self.attack_indicator = AttackIndicator::from_id(id);
                }
            }
            _ => return self.keys.parse_line(key, value),
        }
        true
    }

    fn serialize_display_controls(&self, out: &mut String) {
        out.push_str(&format!(
            "guiScale={}\nmaxFps={}\ngamma={}\nfovEffectScale={}\nscreenEffectScale={}\ndamageTiltStrength={}\ntoggleCrouch={}\ntoggleSprint={}\ninvertXMouse={}\ninvertYMouse={}\nmouseWheelSensitivity={:.4}\ndiscrete_mouse_scroll={}\nshowSubtitles={}\ntextBackgroundOpacity={}\nbackgroundForChatOnly={}\nmipmapLevels={}\nmenuBackgroundBlurriness={}\npanoramaScrollSpeed={}\nrenderClouds={}\ncloudRange={}\nhideLightningFlashes={}\nentityShadows={}\ntoggleAttack={}\ntoggleUse={}\nautoJump={}\nattackIndicator={}\nbobView={}\n",
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
            self.show_subtitles,
            self.text_background_opacity,
            self.background_for_chat_only,
            self.mipmap_levels,
            self.menu_blur,
            self.panorama_speed,
            self.clouds.saved(),
            self.cloud_range,
            self.hide_lightning_flashes,
            self.entity_shadows,
            self.toggle_attack,
            self.toggle_use,
            self.auto_jump,
            self.attack_indicator.id(),
            self.view_bobbing,
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
        self.mipmap_levels = self.mipmap_levels.clamp(MIPMAP_LEVELS.0, MIPMAP_LEVELS.1);
        self.menu_blur = self.menu_blur.clamp(MENU_BLUR.0, MENU_BLUR.1);
        // Java stores the framerate in steps of ten.
        self.max_fps = (self.max_fps.clamp(MAX_FPS.0, MAX_FPS.1) + 5) / 10 * 10;
        for v in [
            &mut self.brightness,
            &mut self.fov_effects,
            &mut self.distortion,
            &mut self.damage_tilt,
            &mut self.text_background_opacity,
        ] {
            *v = v.clamp(0.0, 1.0);
        }
        self.panorama_speed = self.panorama_speed.clamp(0.0, 1.0);
        let (lo, hi) = SCROLL_STEPS;
        self.scroll_sensitivity = self.scroll_sensitivity.clamp(scroll_sensitivity(lo), scroll_sensitivity(hi));
        let (lo, hi) = voxelcraft::atmosphere::CLOUD_RANGE;
        self.cloud_range = self.cloud_range.clamp(lo, hi);
        self
    }

    /// The Max Framerate cap, or `None` for Unlimited.
    /// `Options.getBackgroundColor(defaultOpacity)`: the backing alpha of
    /// an overlay whose own opacity is `default`.
    pub fn background_opacity(&self, default: f32) -> f32 {
        if self.background_for_chat_only { default } else { self.text_background_opacity }
    }

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
pub const MAX_GUI_SCALE: u32 = voxelcraft::capture::MAX_GUI_SCALE;

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

    /// v1.0 Track 1: the options file each release from v0.8.0 to v0.18
    /// wrote (beside its save fixture, `tests/fixtures/saves`) loads with
    /// its values, and saving it again changes nothing this build reads.
    #[test]
    fn every_release_options_file_loads_and_resaves_stably() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/saves");
        let mut seen = 0;
        for entry in std::fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path().join("options.txt");
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let s = Settings::parse(&text);
            // Each run used `--rd 2` and changed nothing else.
            assert_eq!((s.render_distance, s.simulation_distance, s.fov), (2, 10, 70.0), "{}", path.display());
            assert_eq!(s.show_fps, text.contains("show_fps=true"), "{}: a saved choice is kept", path.display());
            assert_eq!(Settings::parse(&s.serialize()), s, "{}", path.display());
            seen += 1;
        }
        assert!(seen >= 10, "{seen} options files");
    }

    #[test]
    fn cloud_and_sky_flash_options_save_load_and_default_like_java() {
        use voxelcraft::atmosphere::Clouds;
        // An options file from before v0.18 has none of them: Java's defaults.
        let old = Settings::parse("render_distance=12\nshowSubtitles=true\n");
        assert_eq!((old.clouds, old.cloud_range, old.hide_lightning_flashes), (Clouds::Fancy, 64, false));
        // A value saved by v0.18 (whose default was 128) stays the player's.
        assert_eq!(Settings::parse("cloudRange=128\n").cloud_range, 128);
        for clouds in Clouds::ALL {
            let s = Settings { clouds, cloud_range: 37, hide_lightning_flashes: true, ..Settings::default() };
            let text = s.serialize();
            assert!(text.contains(&format!("renderClouds={}\n", clouds.saved())), "{text}");
            assert_eq!(Settings::parse(&text), s);
        }
        // Java's legacy booleans, and the range's ends.
        assert_eq!(Settings::parse("renderClouds=false").clouds, Clouds::Off);
        assert_eq!(Settings::parse("renderClouds=true").clouds, Clouds::Fancy);
        assert_eq!(Settings::parse("renderClouds=fast").clouds, Clouds::Fast);
        assert_eq!(Settings::parse("renderClouds=junk").clouds, Clouds::Fancy);
        assert_eq!(Settings::parse("cloudRange=1").cloud_range, 2);
        assert_eq!(Settings::parse("cloudRange=500").cloud_range, 128);
        assert!(Settings::parse("hideLightningFlashes=true").hide_lightning_flashes);
    }

    /// v1.0 Track 9: Java 26.3's control options under its `options.txt`
    /// keys and defaults (Hold, Hold, Auto-Jump off, Crosshair), kept across a
    /// save; an attack indicator id out of range wraps like Java's codec.
    #[test]
    fn control_toggles_auto_jump_and_attack_indicator_use_javas_keys() {
        let d = Settings::default();
        assert_eq!((d.toggle_attack, d.toggle_use, d.auto_jump), (false, false, false));
        assert_eq!(d.attack_indicator, AttackIndicator::Crosshair);
        assert_eq!(Settings::parse("render_distance=8\n"), d, "files without the keys get the defaults");
        let text = d.serialize();
        for line in
            ["toggleAttack=false\n", "toggleUse=false\n", "autoJump=false\n", "attackIndicator=1\n", "bobView=true\n"]
        {
            assert!(text.contains(line), "{line:?} in {text}");
        }
        for indicator in AttackIndicator::ALL {
            let s =
                Settings { toggle_attack: true, toggle_use: true, auto_jump: true, attack_indicator: indicator, ..d };
            assert_eq!(Settings::parse(&s.serialize()), s);
        }
        assert_eq!(Settings::parse("attackIndicator=2").attack_indicator, AttackIndicator::Hotbar);
        assert_eq!(Settings::parse("attackIndicator=0").attack_indicator, AttackIndicator::Off);
        assert_eq!(Settings::parse("attackIndicator=4").attack_indicator, AttackIndicator::Crosshair, "wraps");
        assert_eq!(Settings::parse("attackIndicator=-1").attack_indicator, AttackIndicator::Hotbar, "wraps");
        assert_eq!(Settings::parse("attackIndicator=junk").attack_indicator, AttackIndicator::Crosshair);
        assert!(Settings::parse("autoJump=true").auto_jump);
        assert!(!Settings::parse("autoJump=maybe").auto_jump);
        // Java's `bobView` and the older `view_bobbing` both load.
        assert!(!Settings::parse("bobView=false").view_bobbing);
        assert!(!Settings::parse("view_bobbing=false").view_bobbing);
        assert_eq!(
            [AttackIndicator::Off, AttackIndicator::Crosshair, AttackIndicator::Hotbar].map(AttackIndicator::next),
            [AttackIndicator::Crosshair, AttackIndicator::Hotbar, AttackIndicator::Off]
        );
    }

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
            show_fps: true,
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

    /// v0.18 Track 2: the FPS counter is off for a new options file only;
    /// a saved true or false loads unchanged and survives a save.
    #[test]
    fn fps_counter_defaults_off_only_without_a_saved_preference() {
        assert!(!Settings::default().show_fps);
        assert!(!Settings::parse("").show_fps, "no options file yet");
        assert!(!Settings::parse("fov=80\nvsync=true\n").show_fps, "a file without the line");
        for saved in [true, false] {
            let s = Settings::parse(&format!("render_distance=8\nshow_fps={saved}\n"));
            assert_eq!(s.show_fps, saved, "a saved {saved} is kept");
            assert_eq!(Settings::parse(&s.serialize()).show_fps, saved, "and saved again");
        }
        // A file written by v0.17, whose default was on, keeps the counter.
        let v017 = "render_distance=8\nsimulation_distance=10\nfov=70\nsensitivity=1.00\nvolume=1.00\nmusic_volume=1.00\nvsync=true\ngraphics=enhanced\nshow_fps=true\nview_bobbing=true\n";
        assert!(Settings::parse(v017).show_fps);
        assert!(Settings::default().serialize().contains("show_fps=false\n"));
    }

    /// Menu Background Blur: Java's key, range 0 to 10 and default 5,
    /// kept across a save; out-of-range and junk values clamp or fall back.
    #[test]
    fn menu_blur_round_trips_with_javas_range() {
        assert_eq!(Settings::default().menu_blur, 5);
        assert_eq!(Settings::parse("").menu_blur, 5, "old files get the default");
        for v in 0..=10 {
            let s = Settings { menu_blur: v, ..Settings::default() };
            assert!(s.serialize().contains(&format!("menuBackgroundBlurriness={v}\n")));
            assert_eq!(Settings::parse(&s.serialize()).menu_blur, v);
        }
        assert_eq!(Settings::parse("menuBackgroundBlurriness=99").menu_blur, 10);
        assert_eq!(Settings::parse("menuBackgroundBlurriness=junk").menu_blur, 5);
        assert_eq!(Settings::parse("menuBackgroundBlurriness=-3").menu_blur, 5, "not a u32");
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

    /// v0.18 Track 9: Closed Captions and the text background options keep
    /// Java's keys and defaults, survive a save, and old files get the defaults.
    #[test]
    fn subtitle_and_text_background_options_round_trip() {
        let d = Settings::default();
        assert!(!d.show_subtitles && d.background_for_chat_only);
        assert_eq!(d.text_background_opacity, 0.5);
        assert_eq!(d.background_opacity(0.8), 0.8, "Chat keeps the overlay's own backing");
        let s = Settings {
            show_subtitles: true,
            text_background_opacity: 0.25,
            background_for_chat_only: false,
            ..Settings::default()
        };
        let text = s.serialize();
        assert!(text.contains("\nshowSubtitles=true\n") && text.contains("\nbackgroundForChatOnly=false\n"));
        let back = Settings::parse(&text);
        assert_eq!(back, s);
        assert_eq!(back.background_opacity(0.8), 0.25, "Everywhere uses the opacity");
        assert!(!Settings::parse("fov=80\n").show_subtitles);
        assert_eq!(Settings::parse("textBackgroundOpacity=7\n").text_background_opacity, 1.0);
    }

    /// v0.18 Track 6: Entity Shadows keeps Java's key and default (on),
    /// survives a save, and files from before it existed get the default.
    #[test]
    fn entity_shadows_round_trip() {
        assert!(Settings::default().entity_shadows);
        let off = Settings { entity_shadows: false, ..Settings::default() };
        let text = off.serialize();
        assert!(text.contains(&format!("\n{}=false\n", voxelcraft::entity::shadow::OPTION_KEY)));
        assert_eq!(Settings::parse(&text), off);
        assert!(Settings::parse(&Settings::default().serialize()).entity_shadows);
        assert!(Settings::parse("fov=80\nshowSubtitles=true\n").entity_shadows, "an older file");
        assert!(Settings::parse("entityShadows=maybe\n").entity_shadows, "a bad value keeps the default");
    }

    /// v0.18 Track 7: Panorama Scroll Speed keeps Java's key, default and
    /// 0..1 range, survives a save, and old files get the default.
    #[test]
    fn panorama_speed_round_trips_with_javas_key_and_default() {
        use voxelcraft::title::options::{PANORAMA_SPEED_DEFAULT, PANORAMA_SPEED_KEY};
        assert_eq!(Settings::default().panorama_speed, PANORAMA_SPEED_DEFAULT);
        assert_eq!(PANORAMA_SPEED_KEY, "panoramaScrollSpeed");
        let s = Settings { panorama_speed: 0.37, ..Settings::default() };
        let text = s.serialize();
        assert!(text.contains("\npanoramaScrollSpeed=0.37\n"), "{text}");
        assert_eq!(Settings::parse(&text), s);
        assert_eq!(Settings::parse("fov=80\n").panorama_speed, 1.0, "old files keep the default");
        assert_eq!(Settings::parse("panoramaScrollSpeed=4\n").panorama_speed, 1.0);
        assert_eq!(Settings::parse("panoramaScrollSpeed=-1\n").panorama_speed, 0.0);
        assert_eq!(Settings::parse("panoramaScrollSpeed=nan\n").panorama_speed, 1.0);
        assert_eq!(Settings::parse("panoramaScrollSpeed=0\n").panorama_speed, 0.0, "zero stops it");
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

    /// v0.18 Track 3: Mipmap Levels has Java's key, range and default, and
    /// an options file from before it loads with the default.
    #[test]
    fn mipmap_levels_match_java_and_persist() {
        let java: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/mipmap_levels.json")).unwrap();
        let m = &java["mipmapLevels"];
        assert_eq!(m["key"], "mipmapLevels");
        assert_eq!((m["min"].as_u64().unwrap() as u32, m["max"].as_u64().unwrap() as u32), MIPMAP_LEVELS);
        assert_eq!(Settings::default().mipmap_levels as u64, m["default"].as_u64().unwrap());
        let s = Settings { mipmap_levels: 2, ..Settings::default() };
        assert!(s.serialize().contains("mipmapLevels=2\n"));
        assert_eq!(Settings::parse(&s.serialize()), s);
        assert_eq!(Settings::parse("render_distance=12\n").mipmap_levels, 4, "older files get the default");
        assert_eq!(Settings::parse("mipmapLevels=9").mipmap_levels, 4);
        assert_eq!(Settings::parse("mipmapLevels=-1").mipmap_levels, 4);
        assert_eq!(Settings::parse("mipmapLevels=0").mipmap_levels, 0);
    }
}
