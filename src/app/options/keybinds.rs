//! Key Binds: Java 26.3's `KeyBindsScreen` and `KeyBindsList`. Every action
//! in the binding table, under its category, with the key it's on and a
//! Reset button. Clicking a binding and then pressing a key or mouse button
//! rebinds it; Escape unbinds it, as in Java. Bindings shared with another
//! action show in yellow brackets with a stripe beside them, and their
//! tooltip names the other actions.

use winit::keyboard::KeyCode;

use super::{OptionsHost, change, close};
use crate::app::gui::{self, BUTTON_H, FOOTER, HEADER, Look, Raw, Rect, Screen, Size, Step, Widget};
use crate::app::keys::{Action, Category, Input};
use crate::render::ui::{Color, Ui, WHITE};

/// Java's `KeyBindsList` row: 340 wide, 20 high.
const ROW_W: f32 = 340.0;
const ROW_H: f32 = 20.0;
const CHANGE_W: f32 = 75.0;
const RESET_W: f32 = 50.0;
const FOOTER_W: f32 = 150.0;
/// `ChatFormatting.YELLOW`.
const YELLOW: Color = [1.0, 1.0, 1.0 / 3.0, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Category(Category),
    Name(Action),
    Change(Action),
    Reset(Action),
    ResetAll,
    Done,
}

pub(in crate::app) struct KeyBinds {
    /// Java's order: by category, then name.
    order: Vec<Action>,
    /// The binding waiting for a key (`KeyBindsScreen.selectedKey`).
    selected: Option<Action>,
}

impl Default for KeyBinds {
    fn default() -> Self {
        Self { order: Action::sorted(), selected: None }
    }
}

/// The rows' left edge and width: Java's 340, narrower in a small window.
fn row_span(sw: f32) -> (f32, f32) {
    let w = ROW_W.min(sw - 20.0);
    (((sw - w) / 2.0).floor(), w)
}

impl KeyBinds {
    /// "This key is also used for:" and the other actions, if any.
    fn conflict_tooltip<H: OptionsHost>(host: &H, action: Action) -> Option<String> {
        let names: Vec<&str> = host.settings().keys.conflicts(action).map(|a| a.label()).collect();
        (!names.is_empty()).then(|| format!("This key is also used for:\n{}", names.join(", ")))
    }
}

impl<H: OptionsHost> Screen<H> for KeyBinds {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Key Binds".into()
    }

    fn list(&self, _: &H, (sw, sh): Size) -> Option<Rect> {
        let (left, w) = row_span(sw);
        Some([left - 4.0, HEADER, w + 18.0, (sh - HEADER - FOOTER).max(ROW_H)])
    }

    fn scroll_step(&self) -> f32 {
        ROW_H
    }

    fn widgets(&self, host: &H, (sw, sh): Size) -> Vec<Widget<Id>> {
        let keys = &host.settings().keys;
        let (left, w) = row_span(sw);
        let right = left + w;
        let mut y = HEADER + 4.0;
        let mut out = Vec::new();
        let mut category = None;
        for &action in &self.order {
            if category != Some(action.category()) {
                category = Some(action.category());
                out.push(
                    Widget::label(Id::Category(action.category()), [left, y, w, ROW_H], action.category().label())
                        .scrolling(),
                );
                y += ROW_H;
            }
            let reset_x = right - RESET_W;
            let change_x = reset_x - 5.0 - CHANGE_W;
            out.push(
                Widget::custom(Id::Name(action), [left, y, change_x - 8.0 - left, ROW_H]).enabled(false).scrolling(),
            );
            let mut change = Widget::custom(Id::Change(action), [change_x, y, CHANGE_W, BUTTON_H]).scrolling();
            if let Some(tip) = Self::conflict_tooltip(host, action) {
                change = change.tooltip(tip);
            }
            out.push(change);
            out.push(
                Widget::button(Id::Reset(action), [reset_x, y, RESET_W, BUTTON_H], "Reset")
                    .enabled(!keys.is_default(action))
                    .scrolling(),
            );
            y += ROW_H;
        }
        let fy = (sh - FOOTER + (FOOTER - BUTTON_H) / 2.0).floor();
        let fx = ((sw - 2.0 * FOOTER_W - 8.0) / 2.0).floor();
        out.push(Widget::button(Id::ResetAll, [fx, fy, FOOTER_W, BUTTON_H], "Reset Keys").enabled(!keys.all_default()));
        out.push(Widget::button(Id::Done, [fx + FOOTER_W + 8.0, fy, FOOTER_W, BUTTON_H], "Done"));
        out
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Change(action) => self.selected = Some(action),
            Id::Reset(action) => change(host, |s| s.keys.reset(action)),
            Id::ResetAll => change(host, |s| s.keys = Default::default()),
            Id::Done => {
                self.selected = None;
                return close(host);
            }
            Id::Category(_) | Id::Name(_) => {}
        }
        Step::Stay
    }

    /// While a binding waits, the next key or mouse button goes to it.
    fn raw(&mut self, host: &mut H, input: Raw) -> Option<Step<H>> {
        let action = self.selected?;
        let input = match input {
            Raw::Key(KeyCode::Escape) => Input::Unbound,
            Raw::Key(code) => Input::Key(code),
            Raw::Mouse(button) => Input::Mouse(button),
        };
        if input.bindable() {
            change(host, |s| s.keys.set(action, input));
            self.selected = None;
        }
        Some(Step::Stay)
    }

    /// The controller's back button lets go of a waiting binding first.
    fn back(&mut self, host: &mut H) -> Step<H> {
        if self.selected.take().is_some() {
            return Step::Stay;
        }
        close(host)
    }

    fn draw_custom(&self, host: &H, ui: &mut Ui, id: Id, rect: Rect, look: Look) {
        let keys = &host.settings().keys;
        match id {
            Id::Name(action) => {
                let [x, y, _, h] = rect;
                ui.text(x, y + ((h - 8.0) / 2.0).floor(), action.label(), WHITE);
            }
            Id::Change(action) => {
                gui::draw_button(ui, rect, "", look);
                let key = keys.get(action).display();
                let conflict = keys.conflicts(action).next().is_some();
                let main = gui::text_color(look);
                let parts: Vec<(&str, Color)> = if self.selected == Some(action) {
                    vec![("> ", YELLOW), (&key, WHITE), (" <", YELLOW)]
                } else if conflict {
                    vec![("[ ", YELLOW), (&key, WHITE), (" ]", YELLOW)]
                } else {
                    vec![(&key, main)]
                };
                let width: f32 = parts.iter().map(|(t, _)| Ui::text_width(t)).sum();
                let [x, y, w, h] = rect;
                if width > w - 8.0 {
                    // Too wide for the button ("Left Control"): one colour,
                    // scrolling inside it like any other long label.
                    let text: String = parts.iter().map(|(t, _)| *t).collect();
                    gui::label_text(ui, rect, &text, 4.0, parts[parts.len() / 2].1);
                    if conflict {
                        ui.rect(x - 6.0, y + 1.0, 3.0, h - 1.0, YELLOW);
                    }
                    return;
                }
                let mut pen = (x + (w - width) / 2.0).floor();
                let ty = y + ((h - 8.0) / 2.0).floor();
                for (i, (text, colour)) in parts.iter().enumerate() {
                    ui.text(pen, ty, text, *colour);
                    if i == 1 && self.selected == Some(action) {
                        // Underlined while it waits for a key.
                        ui.rect(pen, ty + 9.0, Ui::text_width(text) - 1.0, 1.0, WHITE);
                    }
                    pen += Ui::text_width(text);
                }
                if conflict {
                    ui.rect(x - 6.0, y + 1.0, 3.0, h - 1.0, YELLOW);
                }
            }
            _ => {}
        }
    }
}
