//! Player options, kept across worlds in `saves/options.txt` as
//! `key=value` lines. Unknown keys and bad values fall back to defaults.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// In 32-block chunks.
    pub render_distance: i32,
    /// Vertical field of view, degrees.
    pub fov: f32,
    /// Multiplier on the base mouse sensitivity.
    pub sensitivity: f32,
    /// Master volume, 0..1.
    pub volume: f32,
    pub vsync: bool,
}

pub const RENDER_DISTANCE: (i32, i32) = (2, 32);
pub const FOV: (f32, f32) = (30.0, 110.0);
pub const SENSITIVITY: (f32, f32) = (0.25, 3.0);

impl Default for Settings {
    fn default() -> Self {
        Self { render_distance: 8, fov: 70.0, sensitivity: 1.0, volume: 1.0, vsync: true }
    }
}

impl Settings {
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
                "fov" => s.fov = num().unwrap_or(s.fov),
                "sensitivity" => s.sensitivity = num().unwrap_or(s.sensitivity),
                "volume" => s.volume = num().unwrap_or(s.volume),
                "vsync" => s.vsync = value != "false",
                _ => {}
            }
        }
        s.clamped()
    }

    pub fn serialize(&self) -> String {
        format!(
            "render_distance={}\nfov={}\nsensitivity={:.2}\nvolume={:.2}\nvsync={}\n",
            self.render_distance, self.fov, self.sensitivity, self.volume, self.vsync
        )
    }

    /// Every value pulled into its valid range.
    pub fn clamped(mut self) -> Self {
        self.render_distance = self.render_distance.clamp(RENDER_DISTANCE.0, RENDER_DISTANCE.1);
        self.fov = self.fov.round().clamp(FOV.0, FOV.1);
        self.sensitivity = self.sensitivity.clamp(SENSITIVITY.0, SENSITIVITY.1);
        self.volume = self.volume.clamp(0.0, 1.0);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_clamps() {
        let s = Settings { render_distance: 12, fov: 90.0, sensitivity: 1.5, volume: 0.4, vsync: false };
        assert_eq!(Settings::parse(&s.serialize()), s);
        let wild = Settings::parse("render_distance=99\nfov=5\nvolume=nan\nsensitivity=abc\njunk\nunknown=1\n");
        assert_eq!(wild, Settings { render_distance: 32, fov: 30.0, ..Settings::default() });
        assert_eq!(Settings::parse(""), Settings::default());
    }
}
