//! Music & Sound Options: Java's `SoundOptionsScreen`. Master Volume
//! across the full width, then a slider for every other sound category, two
//! to a row in Java's order.
//!
//! Left out, on purpose: Show Subtitles (v0.20), Directional Audio and the
//! output device (the mixer has no HRTF and opens the system default), Music
//! Frequency and the Now Playing toast, and Java's Narrator/Voice slider
//! (nothing speaks).

use super::{OptionsHost, change, close, percent_label};
use crate::app::gui::{OptionList, Rect, Screen, Size, Step, Widget};
use crate::audio::SoundSource;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Volume(SoundSource),
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
        let others = &SoundSource::ALL[1..];
        for row in others.chunks(2) {
            list.pair(slider(row[0]), row.get(1).map(|&source| slider(source)));
        }
        list.done(Widget::button(Id::Done, [0.0; 4], "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Done => close(host),
            Id::Volume(_) => Step::Stay,
        }
    }

    fn slide(&mut self, host: &mut H, id: Id, t: f32) {
        if let Id::Volume(source) = id {
            let t = (t * 100.0).round() / 100.0;
            change(host, |s| s.set_sound_volume(source, t));
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}
