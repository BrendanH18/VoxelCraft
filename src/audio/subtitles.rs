//! v0.18 Track 9: Java 26.3's subtitle overlay (`SubtitleOverlay`).
//!
//! Every sound played while Closed Captions is on adds its Java subtitle
//! (the `subtitle` of its sound event in `sounds.json`, in English) or
//! refreshes the line already showing that text. A line remembers where
//! each of its sounds played and when; a sound is forgotten 3 seconds after
//! it played (Notification Time's 1x), and a line with none left goes.
//! A line shows while the listener is within its range of the sound
//! (`Sound.getAttenuationDistance`, 16 blocks times the volume when that is
//! louder; sounds at the listener, Java's relative ones, are heard
//! anywhere). Its text fades from white to grey over the 3 seconds (a
//! looping sound stays white for its first half second), and an arrow
//! points to a sound more than 60 degrees off the view direction.
//!
//! Drawing is the HUD's ([`crate::app`]): lines stack up from 35 pixels
//! above the bottom-right corner, each on a backing of Text Background's
//! colour (`getBackgroundColor(0.8)`), as wide as the widest line plus the
//! arrows.

use std::collections::HashMap;
use std::sync::OnceLock;

use glam::DVec3;

use super::sounds::Sound;

/// `SubtitleOverlay.DISPLAY_TIME`, in seconds.
pub const DISPLAY_TIME: f64 = 3.0;
/// `LOOPING_DISPLAY_FULL_BRIGHTNESS_TIME`, in seconds.
pub const LOOPING_FULL_BRIGHTNESS: f64 = 0.5;
/// The most lines kept (Java has no limit; sounds come in bursts).
const MOST: usize = 64;

struct Texts {
    /// Per [`Sound::key`], the subtitle text index and attenuation distance.
    by_sound: Vec<Option<(u16, f32)>>,
    texts: Vec<&'static str>,
}

fn texts() -> &'static Texts {
    static TEXTS: OnceLock<Texts> = OnceLock::new();
    TEXTS.get_or_init(|| {
        let fixture = super::block_sounds::fixture();
        let mut texts: Vec<&'static str> = Vec::new();
        let mut index: HashMap<&'static str, u16> = HashMap::new();
        let mut by_sound = vec![None; Sound::COUNT];
        for sound in Sound::all() {
            let Some(event) = sound.java_event() else { continue };
            let Some(key) = fixture["subtitles"][&event].as_str() else { continue };
            // Keys without English text show the key itself, as Java's do.
            let text: &'static str = fixture["subtitle_text"][key].as_str().unwrap_or(key);
            let i = *index.entry(text).or_insert_with(|| {
                texts.push(text);
                texts.len() as u16 - 1
            });
            let range = fixture["attenuation_distances"][&event].as_f64().unwrap_or(16.0) as f32;
            by_sound[sound.key()] = Some((i, range));
        }
        Texts { by_sound, texts }
    })
}

/// The subtitle `sound` shows, if its Java event has one.
pub fn subtitle(sound: Sound) -> Option<&'static str> {
    let t = texts();
    t.by_sound.get(sound.key()).copied().flatten().map(|(i, _)| t.texts[i as usize])
}

/// Where and when one of a line's sounds played (`SoundPlayedAt`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Played {
    at: DVec3,
    time: f64,
}

/// One caption (`SubtitleOverlay.Subtitle`).
#[derive(Clone, Debug, PartialEq)]
struct Line {
    text: u16,
    /// Blocks; infinite for sounds at the listener.
    range: f32,
    looping: bool,
    played: Vec<Played>,
}

impl Line {
    /// `getBestSubtitleCandidate`: the only sound, else the latest (then
    /// nearest) one within range.
    fn best(&self, ear: DVec3) -> Option<Played> {
        if self.played.len() == 1 {
            return self.played.first().copied();
        }
        let range = self.range as f64;
        self.played.iter().filter(|p| p.at.distance_squared(ear) < range * range).copied().max_by(|a, b| {
            a.time.total_cmp(&b.time).then_with(|| a.at.distance_squared(ear).total_cmp(&b.at.distance_squared(ear)))
        })
    }

    /// `isAudibleFrom`.
    fn audible(&self, ear: DVec3) -> bool {
        if self.range.is_infinite() {
            return true;
        }
        let range = self.range as f64;
        self.best(ear).is_some_and(|p| p.at.distance_squared(ear) < range * range)
    }
}

/// Which way to point a caption's arrow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arrow {
    None,
    Left,
    Right,
}

/// A caption to draw this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub text: &'static str,
    pub arrow: Arrow,
    /// Text grey level, 75..=255.
    pub brightness: u8,
}

/// The captions on screen and the sounds behind them.
#[derive(Default)]
pub struct Subtitles {
    lines: Vec<Line>,
}

impl Subtitles {
    /// `onPlaySound`: a sound played at `at` (the listener's position for
    /// one with no position, which is then heard anywhere) at `time`
    /// seconds, `gain` its volume.
    pub fn play(&mut self, sound: Sound, at: DVec3, relative: bool, gain: f32, time: f64) {
        let Some((text, range)) = texts().by_sound.get(sound.key()).copied().flatten() else { return };
        if let Some(line) = self.lines.iter_mut().find(|l| l.text == text) {
            // `refresh`: one entry per position, the newest last.
            line.played.retain(|p| p.at != at);
            line.played.push(Played { at, time });
            return;
        }
        if self.lines.len() >= MOST {
            return;
        }
        let range = if relative { f32::INFINITY } else { range * gain.max(1.0) };
        self.lines.push(Line { text, range, looping: sound.is_loop(), played: vec![Played { at, time }] });
    }

    /// Clears every caption (leaving a world, or Closed Captions turned off).
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// `extractRenderState`'s bookkeeping and choices: forgets sounds older
    /// than the display time and returns the captions heard at `ear`,
    /// facing `forward` (with `right` to its right), oldest first.
    pub fn shown(&mut self, ear: DVec3, forward: DVec3, right: DVec3, time: f64) -> Vec<Shown> {
        let texts = &texts().texts;
        let mut out = Vec::new();
        for line in &mut self.lines {
            line.played.retain(|p| time - p.time <= DISPLAY_TIME);
        }
        self.lines.retain(|l| !l.played.is_empty());
        for line in &self.lines {
            if !line.audible(ear) {
                continue;
            }
            let Some(best) = line.best(ear) else { continue };
            // `Vec3.normalize` gives zero for a sound at the ear: no arrow.
            let delta = best.at - ear;
            let delta = if delta.length() < 1e-4 { DVec3::ZERO } else { delta.normalize() };
            let rightness = right.dot(delta);
            let in_view = forward.dot(delta) > 0.5;
            let arrow = if in_view || rightness == 0.0 {
                Arrow::None
            } else if rightness > 0.0 {
                Arrow::Right
            } else {
                Arrow::Left
            };
            out.push(Shown {
                text: texts[line.text as usize],
                arrow,
                brightness: brightness(line.looping, time - best.time),
            });
        }
        out
    }
}

/// `Subtitle.getBrightness`: 255 down to 75 over the display time, after a
/// looping sound's half second at full brightness.
pub fn brightness(looping: bool, age: f64) -> u8 {
    let hold = if looping { LOOPING_FULL_BRIGHTNESS } else { 0.0 };
    let t = ((age - hold).max(0.0) / (DISPLAY_TIME - hold)).clamp(0.0, 1.0);
    (255.0 + (75.0 - 255.0) * t).floor() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sounds::{Material, Sound};

    const FORWARD: DVec3 = DVec3::NEG_Z;
    const RIGHT: DVec3 = DVec3::X;

    #[test]
    fn captions_merge_by_text_fade_and_expire_after_three_seconds() {
        let mut s = Subtitles::default();
        let ear = DVec3::ZERO;
        // Two different sounds with the same subtitle share a line.
        s.play(Sound::Break(Material::Stone), DVec3::new(0.0, 0.0, -4.0), false, 1.0, 0.0);
        s.play(Sound::Break(Material::Wood), DVec3::new(0.0, 0.0, -5.0), false, 1.0, 1.0);
        let shown = s.shown(ear, FORWARD, RIGHT, 1.0);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].text, "Block broken");
        assert_eq!(shown[0].arrow, Arrow::None, "in view");
        assert_eq!(shown[0].brightness, 255, "the newest sound is just played");
        // 1.5 s after the newest: halfway through the fade.
        assert_eq!(s.shown(ear, FORWARD, RIGHT, 2.5)[0].brightness, 165);
        // The first sound expires at 3 s, the line at 4 s.
        assert_eq!(s.shown(ear, FORWARD, RIGHT, 3.9).len(), 1);
        assert!(s.shown(ear, FORWARD, RIGHT, 4.01).is_empty());
        assert!(s.is_empty());
    }

    #[test]
    fn arrows_point_to_sounds_out_of_view_and_range_hides_far_ones() {
        let mut s = Subtitles::default();
        s.play(Sound::Explosion, DVec3::new(8.0, 0.0, 0.0), false, 1.0, 0.0);
        s.play(Sound::Bell, DVec3::new(-8.0, 0.0, 0.0), false, 1.0, 0.0);
        s.play(Sound::Click, DVec3::ZERO, true, 1.0, 0.0);
        let shown = s.shown(DVec3::ZERO, FORWARD, RIGHT, 0.1);
        let arrow = |t: &str| shown.iter().find(|x| x.text == t).map(|x| x.arrow);
        assert_eq!(arrow("Explosion"), Some(Arrow::Right));
        assert_eq!(arrow("Bell rings"), Some(Arrow::Left));
        // `ui.button.click` has no subtitle in Java.
        assert_eq!(shown.len(), 2);
        // Walking 20 blocks away leaves the bell out of range (16 blocks).
        let far = s.shown(DVec3::new(30.0, 0.0, 0.0), FORWARD, RIGHT, 0.2);
        assert!(far.iter().all(|x| x.text != "Bell rings"));
    }

    #[test]
    fn looping_sounds_stay_bright_for_half_a_second() {
        assert_eq!(brightness(true, 0.4), 255);
        assert_eq!(brightness(false, 0.4), 231);
        assert_eq!(brightness(false, 3.0), 75);
    }
}
