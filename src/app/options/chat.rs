//! v1.0 final F6: Chat Settings, Java 26.3's `ChatOptionsScreen` with the
//! options VoxelCraft's chat has, in Java's order, each under Java's label,
//! range, default and `options.txt` key. Chat Text Opacity, Line Spacing
//! and Chat Delay also sit in Accessibility Settings, as in Java.
//!
//! Left out: Web Links and Prompt on Links (no chat text is clickable), the
//! Narrator (nothing speaks), Command Suggestions, Hide Matched Names,
//! Reduced Debug Info, Only Show Secure Chat (no signed chat) and Save
//! Unsent Chats.

use super::{OptionsHost, change, close, frac};
use crate::app::gui::{OptionList, Rect, Screen, Size, Step, Widget, on_off};
use crate::app::settings::ChatOptions;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Visibility,
    Colors,
    Opacity,
    TextBackgroundOpacity,
    Scale,
    LineSpacing,
    Delay,
    Width,
    HeightFocused,
    HeightUnfocused,
    Done,
}

/// `Options.percentValueLabel`.
fn percent(name: &str, v: f32) -> String {
    format!("{name}: {}%", (v * 100.0).round() as i32)
}

/// Chat Text Opacity: the label shows the text alpha, `0.1 + 0.9 * v`.
pub(in crate::app) fn opacity_label(v: f32) -> String {
    percent("Chat Text Opacity", v * 0.9 + 0.1)
}

/// Chat Text Size: OFF at zero (`CommonComponents.optionStatus`).
pub(in crate::app) fn scale_label(v: f32) -> String {
    if v <= 0.0 { "Chat Text Size: OFF".into() } else { percent("Chat Text Size", v) }
}

/// `options.chat.delay_none` and `options.chat.delay` (`%.1f`).
pub(in crate::app) fn delay_label(v: f32) -> String {
    if v <= 0.0 { "Chat Delay: None".into() } else { format!("Chat Delay: {v:.1} second(s)") }
}

/// The sliders Chat and Accessibility Settings share.
pub(in crate::app) fn opacity_slider<I>(id: I, c: &ChatOptions) -> Widget<I> {
    Widget::slider(id, [0.0; 4], opacity_label(c.opacity), c.opacity, 100)
}

pub(in crate::app) fn line_spacing_slider<I>(id: I, c: &ChatOptions) -> Widget<I> {
    Widget::slider(id, [0.0; 4], percent("Line Spacing", c.line_spacing), c.line_spacing, 100)
}

pub(in crate::app) fn delay_slider<I>(id: I, c: &ChatOptions) -> Widget<I> {
    Widget::slider(id, [0.0; 4], delay_label(c.delay), frac(c.delay, (0.0, 6.0)), 60)
}

/// A unit slider's value, in hundredths.
pub(in crate::app) fn unit(t: f32) -> f32 {
    (t.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

/// Chat Delay at slider position `t`, in tenths of a second.
pub(in crate::app) fn delay_at(t: f32) -> f32 {
    (t.clamp(0.0, 1.0) * 60.0).round() / 10.0
}

pub(in crate::app) struct ChatSettings;

impl<H: OptionsHost> Screen<H> for ChatSettings {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Chat Settings".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let s = host.settings();
        let c = &s.chat;
        let z = [0.0; 4];
        let px = |name: &str, v: i32| format!("{name}: {v}px");
        let mut list = OptionList::new(size);
        list.pairs(vec![
            Widget::button(Id::Visibility, z, format!("Chat: {}", c.visibility.caption())),
            Widget::button(Id::Colors, z, format!("Colors: {}", on_off(c.colors))),
            opacity_slider(Id::Opacity, c),
            Widget::slider(
                Id::TextBackgroundOpacity,
                z,
                percent("Text Background Opacity", s.text_background_opacity),
                s.text_background_opacity,
                100,
            ),
            Widget::slider(Id::Scale, z, scale_label(c.scale), c.scale, 100),
            line_spacing_slider(Id::LineSpacing, c),
            delay_slider(Id::Delay, c),
            Widget::slider(Id::Width, z, px("Width", ChatOptions::width_px(c.width)), c.width, 100),
            Widget::slider(
                Id::HeightFocused,
                z,
                px("Focused Height", ChatOptions::height_px(c.height_focused)),
                c.height_focused,
                100,
            ),
            Widget::slider(
                Id::HeightUnfocused,
                z,
                px("Unfocused Height", ChatOptions::height_px(c.height_unfocused)),
                c.height_unfocused,
                100,
            ),
        ]);
        list.done(Widget::button(Id::Done, z, "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Visibility => change(host, |s| s.chat.visibility = s.chat.visibility.next()),
            Id::Colors => change(host, |s| s.chat.colors = !s.chat.colors),
            Id::Done => return close(host),
            _ => {}
        }
        Step::Stay
    }

    fn slide(&mut self, host: &mut H, id: Id, t: f32) {
        let u = unit(t);
        match id {
            Id::Opacity => change(host, |s| s.chat.opacity = u),
            Id::TextBackgroundOpacity => change(host, |s| s.text_background_opacity = u),
            Id::Scale => change(host, |s| s.chat.scale = u),
            Id::LineSpacing => change(host, |s| s.chat.line_spacing = u),
            Id::Delay => change(host, |s| s.chat.delay = delay_at(t)),
            Id::Width => change(host, |s| s.chat.width = u),
            Id::HeightFocused => change(host, |s| s.chat.height_focused = u),
            Id::HeightUnfocused => change(host, |s| s.chat.height_unfocused = u),
            _ => {}
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}
