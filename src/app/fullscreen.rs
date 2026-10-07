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

    /// Puts the window into this mode on its current monitor. Exclusive picks
    /// the monitor's largest mode, then its highest refresh rate, and falls
    /// back to borderless when the platform reports no video modes.
    pub fn apply(self, window: &Window) {
        if Self::current(window) == self {
            return;
        }
        let monitor = window.current_monitor();
        let fullscreen = match self {
            Self::Off => None,
            Self::Borderless => Some(Fullscreen::Borderless(monitor)),
            Self::Exclusive => match monitor.as_ref().and_then(|m| native_mode(m.video_modes())) {
                Some(mode) => Some(Fullscreen::Exclusive(mode)),
                None => Some(Fullscreen::Borderless(monitor)),
            },
        };
        window.set_fullscreen(fullscreen);
    }
}

fn native_mode(modes: impl Iterator<Item = VideoModeHandle>) -> Option<VideoModeHandle> {
    modes.max_by_key(|m| {
        let size = m.size();
        (size.width as u64 * size.height as u64, m.refresh_rate_millihertz(), m.bit_depth())
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
    }
}
