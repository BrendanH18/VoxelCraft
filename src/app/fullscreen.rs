//! Window modes: windowed, borderless fullscreen and exclusive fullscreen at
//! the monitor's native resolution. On macOS borderless uses the system's
//! native fullscreen Space, like other Mac games; exclusive switches the
//! display mode, like Java's "Fullscreen Resolution" option.

use winit::monitor::VideoModeHandle;
use winit::window::{Fullscreen, Window};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Borderless,
    Exclusive,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Borderless => "Borderless",
            Self::Exclusive => "Exclusive",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Borderless,
            Self::Borderless => Self::Exclusive,
            Self::Exclusive => Self::Off,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Off, Self::Borderless, Self::Exclusive].into_iter().find(|m| m.name().eq_ignore_ascii_case(value))
    }

    /// The mode the window is actually in (the user may also use the macOS
    /// green button or the system shortcut).
    pub fn current(window: &Window) -> Self {
        match window.fullscreen() {
            None => Self::Off,
            Some(Fullscreen::Borderless(_)) => Self::Borderless,
            Some(Fullscreen::Exclusive(_)) => Self::Exclusive,
        }
    }

    /// The mode a benchmark's `--benchmark-window-mode` names
    /// (`voxelcraft::performance::WINDOW_MODES`).
    pub fn from_benchmark(name: &str) -> Self {
        match name {
            "borderless" => Self::Borderless,
            "exclusive" => Self::Exclusive,
            _ => Self::Off,
        }
    }

    /// Puts the window into this mode on its current monitor. Exclusive picks
    /// the monitor's largest mode, then its highest refresh rate, and falls
    /// back to borderless when the platform reports no video modes.
    pub fn apply(self, window: &Window) {
        self.apply_preferring(window, None);
    }

    /// As [`Mode::apply`], but exclusive fullscreen first looks for a video
    /// mode of `size` physical pixels (benchmarks ask for 1920 x 1080).
    pub fn apply_preferring(self, window: &Window, size: Option<(u32, u32)>) {
        if Self::current(window) == self {
            return;
        }
        let monitor = window.current_monitor();
        let fullscreen = match self {
            Self::Off => None,
            Self::Borderless => Some(Fullscreen::Borderless(monitor)),
            Self::Exclusive => match monitor.as_ref().and_then(|m| native_mode(m.video_modes(), size)) {
                Some(mode) => Some(Fullscreen::Exclusive(mode)),
                None => Some(Fullscreen::Borderless(monitor)),
            },
        };
        window.set_fullscreen(fullscreen);
    }
}

fn native_mode(modes: impl Iterator<Item = VideoModeHandle>, prefer: Option<(u32, u32)>) -> Option<VideoModeHandle> {
    best_mode(
        modes,
        |m| {
            let size = m.size();
            ((size.width, size.height), m.refresh_rate_millihertz(), m.bit_depth())
        },
        prefer,
    )
}

/// The mode whose size is `prefer`, if any, else the largest; then the
/// highest refresh rate and bit depth. `key` gives (size, millihertz, depth).
fn best_mode<T>(
    modes: impl Iterator<Item = T>,
    key: impl Fn(&T) -> ((u32, u32), u32, u16),
    prefer: Option<(u32, u32)>,
) -> Option<T> {
    modes.max_by_key(|m| {
        let (size, refresh, depth) = key(m);
        (prefer == Some(size), u64::from(size.0) * u64::from(size.1), refresh, depth)
    })
}

/// The window's mode and size and its monitor, for benchmark reports.
/// `video_modes` adds the monitor's distinct modes as `WxH@Hz`.
pub fn describe(window: &Window, video_modes: bool) -> serde_json::Value {
    let inner = window.inner_size();
    let monitor = window.current_monitor().map(|m| {
        let size = m.size();
        let modes = video_modes.then(|| {
            let mut modes: Vec<String> = m
                .video_modes()
                .map(|v| {
                    format!("{}x{}@{:.3}", v.size().width, v.size().height, v.refresh_rate_millihertz() as f64 / 1000.)
                })
                .collect();
            modes.sort();
            modes.dedup();
            modes
        });
        serde_json::json!({
            "name": m.name(),
            "physical": [size.width, size.height],
            "scale_factor": m.scale_factor(),
            "refresh_millihertz": m.refresh_rate_millihertz(),
            "video_modes": modes,
        })
    });
    serde_json::json!({
        "mode": Mode::current(window).name(),
        "window_physical": [inner.width, inner.height],
        "monitor": monitor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_and_parse() {
        let mut m = Mode::Off;
        for _ in 0..3 {
            assert_eq!(Mode::parse(m.name()), Some(m));
            m = m.next();
        }
        assert_eq!(m, Mode::Off);
        assert_eq!(Mode::parse("EXCLUSIVE"), Some(Mode::Exclusive));
        assert_eq!(Mode::parse("windowed"), None);
        assert_eq!(Mode::from_benchmark("windowed"), Mode::Off);
        assert_eq!(Mode::from_benchmark("borderless"), Mode::Borderless);
        assert_eq!(Mode::from_benchmark("exclusive"), Mode::Exclusive);
    }

    #[test]
    fn exclusive_mode_prefers_the_requested_size() {
        let modes = [((3024, 1964), 120_000, 32), ((1920, 1080), 60_000, 32), ((1920, 1080), 120_000, 32)];
        let pick = |prefer| best_mode(modes.into_iter(), |m| *m, prefer).unwrap();
        assert_eq!(pick(None), modes[0], "largest without a preference");
        assert_eq!(pick(Some((1920, 1080))), modes[2], "the requested size at its highest refresh");
        assert_eq!(pick(Some((2560, 1440))), modes[0], "largest when the size isn't offered");
        assert!(best_mode(std::iter::empty::<((u32, u32), u32, u16)>(), |m| *m, None).is_none());
    }
}
