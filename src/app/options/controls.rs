//! Controls and Mouse Settings: Java 26.3's screens with the options
//! VoxelCraft has. Controls leaves out Attack and Use (Hold/Toggle),
//! Auto-Jump, Sprint Window, quit shortcuts and the operator items tab;
//! Mouse Settings leaves out Allow Cursor Changes.

use super::{KeyBinds, OptionsHost, change, close, frac, lerp};
use crate::app::gui::{OptionList, Rect, Screen, Size, Step, Widget, on_off};
use crate::app::settings::{SCROLL_STEPS, SENSITIVITY, scroll_sensitivity, scroll_step};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum ControlsId {
    Mouse,
    KeyBinds,
    Sneak,
    Sprint,
    Done,
}

fn hold_toggle(name: &str, toggle: bool) -> String {
    format!("{name}: {}", if toggle { "Toggle" } else { "Hold" })
}

pub(in crate::app) struct Controls;

impl<H: OptionsHost> Screen<H> for Controls {
    type Id = ControlsId;

    fn title(&self, _: &H) -> String {
        "Controls".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<ControlsId>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<ControlsId>> {
        let s = host.settings();
        let z = [0.0; 4];
        let mut list = OptionList::new(size);
        list.pairs(vec![
            Widget::button(ControlsId::Mouse, z, "Mouse Settings..."),
            Widget::button(ControlsId::KeyBinds, z, "Key Binds..."),
            Widget::button(ControlsId::Sneak, z, hold_toggle("Sneak", s.toggle_sneak)),
            Widget::button(ControlsId::Sprint, z, hold_toggle("Sprint", s.toggle_sprint)),
        ]);
        list.done(Widget::button(ControlsId::Done, z, "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: ControlsId) -> Step<H> {
        match id {
            ControlsId::Mouse => return Step::push(MouseSettings),
            ControlsId::KeyBinds => return Step::push(KeyBinds::default()),
            ControlsId::Sneak => change(host, |s| s.toggle_sneak = !s.toggle_sneak),
            ControlsId::Sprint => change(host, |s| s.toggle_sprint = !s.toggle_sprint),
            ControlsId::Done => return close(host),
        }
        Step::Stay
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum MouseId {
    Sensitivity,
    Scroll,
    Discrete,
    InvertX,
    InvertY,
    Done,
}

pub(in crate::app) struct MouseSettings;

impl<H: OptionsHost> Screen<H> for MouseSettings {
    type Id = MouseId;

    fn title(&self, _: &H) -> String {
        "Mouse Settings".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<MouseId>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<MouseId>> {
        let s = host.settings();
        let z = [0.0; 4];
        let mut list = OptionList::new(size);
        // VoxelCraft's sensitivity is a multiplier on its own base, shown
        // as a percentage like Java's (100% is the default either way).
        let label = format!("Sensitivity: {}%", (s.sensitivity * 100.0).round() as i32);
        // Steps of 5%.
        let steps = ((SENSITIVITY.1 - SENSITIVITY.0) * 20.0).round() as u32;
        let (lo, hi) = SCROLL_STEPS;
        let scroll = scroll_step(s.scroll_sensitivity);
        list.pairs(vec![
            Widget::slider(MouseId::Sensitivity, z, label, frac(s.sensitivity, SENSITIVITY), steps),
            Widget::slider(
                MouseId::Scroll,
                z,
                format!("Scroll Sensitivity: {:.2}", s.scroll_sensitivity),
                (scroll - lo) as f32 / (hi - lo) as f32,
                (hi - lo) as u32,
            ),
            Widget::button(MouseId::Discrete, z, format!("Discrete Scrolling: {}", on_off(s.discrete_scroll))),
            Widget::button(MouseId::InvertX, z, format!("Invert Mouse X: {}", on_off(s.invert_mouse_x))),
            Widget::button(MouseId::InvertY, z, format!("Invert Mouse Y: {}", on_off(s.invert_mouse_y))),
        ]);
        list.done(Widget::button(MouseId::Done, z, "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: MouseId) -> Step<H> {
        match id {
            MouseId::Discrete => change(host, |s| s.discrete_scroll = !s.discrete_scroll),
            MouseId::InvertX => change(host, |s| s.invert_mouse_x = !s.invert_mouse_x),
            MouseId::InvertY => change(host, |s| s.invert_mouse_y = !s.invert_mouse_y),
            MouseId::Done => return close(host),
            MouseId::Sensitivity | MouseId::Scroll => {}
        }
        Step::Stay
    }

    fn slide(&mut self, host: &mut H, id: MouseId, t: f32) {
        match id {
            MouseId::Sensitivity => change(host, |s| s.sensitivity = (lerp(SENSITIVITY, t) * 20.0).round() / 20.0),
            MouseId::Scroll => {
                let (lo, hi) = SCROLL_STEPS;
                let step = lo + (t * (hi - lo) as f32).round() as i32;
                change(host, |s| s.scroll_sensitivity = scroll_sensitivity(step));
            }
            _ => {}
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}
