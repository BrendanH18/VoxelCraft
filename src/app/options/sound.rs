//! Music & Sound Options: Java's `SoundOptionsScreen`. Master Volume
//! across the full width, then a slider for every other sound category, two
//! to a row in Java's order.
//!
//! v0.18 Track 9: Closed Captions sits where Java's does, after the output
//! device, alone in its row. v1.0 final F6 adds Java's Narrator/Voice slider
//! between Ambient/Environment and UI (`soundCategory_voice`; only Java's
//! narrator plays in it, and VoxelCraft has none). Left out: the output
//! Device (the audio thread opens the system default once, for the whole
//! run), Directional Audio (the mixer pans in stereo and has no HRTF), Music
//! Frequency and the Now Playing toast.

use super::{OptionsHost, change, close, percent_label};
use crate::app::gui::{OptionList, Rect, Screen, Size, Step, Widget, on_off};
use crate::audio::SoundSource;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Volume(SoundSource),
    Voice,
    Subtitles,
    Done,
}

pub(in crate::app) struct MusicAndSounds;

impl<H: OptionsHost> Screen<H> for MusicAndSounds {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Music & Sound Options".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let s = host.settings();
        let slider = |source: SoundSource| {
            let v = s.sound_volume(source);
            Widget::slider(Id::Volume(source), [0.0; 4], percent_label(source.label(), v), v, 100)
        };
        let mut list = OptionList::new(size);
        list.wide(slider(SoundSource::Master));
        // Java's `SoundSource` order puts Voice before UI.
        let mut others: Vec<_> = SoundSource::ALL[1..].iter().map(|&source| slider(source)).collect();
        let voice = s.voice_volume;
        others.insert(
            others.len() - 1,
            Widget::slider(Id::Voice, [0.0; 4], percent_label("Narrator/Voice", voice), voice, 100),
        );
        list.pairs(others);
        list.pair(
            Widget::button(Id::Subtitles, [0.0; 4], format!("Closed Captions: {}", on_off(s.show_subtitles)))
                .tooltip("Enables captions for sounds played in the game."),
            None,
        );
        list.done(Widget::button(Id::Done, [0.0; 4], "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Done => close(host),
            Id::Subtitles => {
                change(host, |s| s.show_subtitles = !s.show_subtitles);
                Step::Stay
            }
            Id::Volume(_) | Id::Voice => Step::Stay,
        }
    }

    fn slide(&mut self, host: &mut H, id: Id, t: f32) {
        let t = (t * 100.0).round() / 100.0;
        match id {
            Id::Volume(source) => change(host, |s| s.set_sound_volume(source, t)),
            Id::Voice => change(host, |s| s.voice_volume = t),
            _ => {}
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}
