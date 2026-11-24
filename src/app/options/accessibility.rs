//! Accessibility Settings: Java 26.3's screen with the options VoxelCraft
//! has, in Java's order. Narrator, high contrast, chat, menu blur, notification
//! time, darkness, glint, lightning flash, panorama, splash and minecart
//! options are left out until VoxelCraft has what they change. v0.18 Track 9
//! adds Closed Captions and the text background pair the captions use.

use super::{Controls, OptionsHost, change, close, percent_label};
use crate::app::gui::{OptionList, Rect, Screen, Size, Step, Widget, on_off};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Controls,
    Subtitles,
    TextBackgroundOpacity,
    TextBackground,
    ViewBobbing,
    Distortion,
    FovEffects,
    DamageTilt,
    Done,
}

pub(in crate::app) struct Accessibility;

impl<H: OptionsHost> Screen<H> for Accessibility {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Accessibility Settings".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let s = host.settings();
        let z = [0.0; 4];
        let mut list = OptionList::new(size);
        // Java's first row is Narrator and a link to Controls.
        list.pair(Widget::button(Id::Controls, z, "Controls..."), None);
        list.pairs(vec![
            Widget::button(Id::Subtitles, z, format!("Closed Captions: {}", on_off(s.show_subtitles)))
                .tooltip("Enables captions for sounds played in the game."),
            // `Options.percentValueLabel`: no OFF at zero.
            Widget::slider(
                Id::TextBackgroundOpacity,
                z,
                format!("Text Background Opacity: {}%", (s.text_background_opacity * 100.0).round() as i32),
                s.text_background_opacity,
                100,
            ),
            Widget::button(
                Id::TextBackground,
                z,
                format!("Text Background: {}", if s.background_for_chat_only { "Chat" } else { "Everywhere" }),
            ),
            Widget::button(Id::ViewBobbing, z, format!("View Bobbing: {}", on_off(s.view_bobbing))),
            Widget::slider(Id::Distortion, z, percent_label("Distortion Effects", s.distortion), s.distortion, 100)
                .tooltip(
                    "Strength of Nausea and Nether Portal screen distortion effects.\nAt lower values, the Nausea effect is replaced with a green overlay.",
                ),
            // Java's slider moves the square root of the value.
            Widget::slider(Id::FovEffects, z, percent_label("FOV Effects", s.fov_effects), s.fov_effects.sqrt(), 100)
                .tooltip("Controls how much the field of view can change with gameplay effects."),
            Widget::slider(Id::DamageTilt, z, percent_label("Damage Tilt", s.damage_tilt), s.damage_tilt, 100)
                .tooltip("The amount of camera shake caused by being hurt."),
        ]);
        list.done(Widget::button(Id::Done, z, "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Controls => return Step::push(Controls),
            Id::ViewBobbing => change(host, |s| s.view_bobbing = !s.view_bobbing),
            Id::Subtitles => change(host, |s| s.show_subtitles = !s.show_subtitles),
            Id::TextBackground => change(host, |s| s.background_for_chat_only = !s.background_for_chat_only),
            Id::Done => return close(host),
            _ => {}
        }
        Step::Stay
    }

    fn slide(&mut self, host: &mut H, id: Id, t: f32) {
        let t = (t * 100.0).round() / 100.0;
        match id {
            Id::Distortion => change(host, |s| s.distortion = t),
            Id::FovEffects => change(host, |s| s.fov_effects = t * t),
            Id::DamageTilt => change(host, |s| s.damage_tilt = t),
            Id::TextBackgroundOpacity => change(host, |s| s.text_background_opacity = t),
            _ => {}
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}
