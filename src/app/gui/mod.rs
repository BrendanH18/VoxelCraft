//! The menu toolkit shared by the title screen, the pause menu and the
//! Options screens.
//!
//! A [`Screen`] lists its widgets for the current state and size each frame,
//! and one list serves drawing, mouse hit-testing and navigation. A [`Stack`]
//! of screens does the rest the same way everywhere: hover, keyboard and
//! controller focus, Escape and the controller's back button, sliders that
//! follow the mouse or step with the arrow keys, editable text fields, a
//! scrolling list region and tooltips. Screens decide what their widgets do
//! and answer with a [`Step`]: stay, open another screen, go back, or ask
//! the app for something ([`Host::Out`]).
//!
//! Layout follows Java's GUI in UI pixels: 20-pixel buttons, 150-pixel option
//! widgets in two columns 10 apart on 25-pixel rows, a 33-pixel header with
//! the title and a 33-pixel footer with Done.

mod clipboard;
mod dialog;
mod layout;
mod text_field;

use std::time::Instant;

use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

use crate::render::ui::{Color, Ui, WHITE};

pub(super) use clipboard::{Clipboard, SystemClipboard};
pub(super) use dialog::{Confirm, Notice};
pub(super) use layout::{FOOTER, HEADER, OptionList, centred_rows};
pub(super) use text_field::TextField;

/// x, y, width, height in UI pixels.
pub(super) type Rect = [f32; 4];
/// Screen width and height in UI pixels.
pub(super) type Size = (f32, f32);

pub(super) const BUTTON_W: f32 = 200.0;
pub(super) const BUTTON_H: f32 = 20.0;
pub(super) const GAP: f32 = 4.0;
/// Width of a slider's handle.
pub(super) const HANDLE_W: f32 = 8.0;

pub(super) const BUTTON: Color = [0.42, 0.42, 0.42, 1.0];
pub(super) const HOT: Color = [0.48, 0.52, 0.72, 1.0];
pub(super) const DISABLED: Color = [0.2, 0.2, 0.2, 1.0];
pub(super) const GREY: Color = [0.63, 0.63, 0.63, 1.0];
pub(super) const HOT_TEXT: Color = [1.0, 1.0, 0.63, 1.0];

/// What a widget is, which decides how it's drawn and what input it takes.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Kind {
    Button(String),
    /// A slider with its label drawn on it, its position (0..1) and how many
    /// steps the arrow keys and controller move it across the whole range.
    Slider {
        label: String,
        value: f32,
        steps: u32,
    },
    /// A text field the screen owns ([`Screen::field`]).
    Field,
    /// Drawn by the screen ([`Screen::draw_custom`]); focused and clicked
    /// like a button. List rows use this.
    Custom,
    /// Centred text that takes no input (a list's section heading).
    #[allow(dead_code, reason = "for the v0.11 options section headings")]
    Label(String),
}

#[derive(Clone, Debug)]
pub(super) struct Widget<Id> {
    pub id: Id,
    pub rect: Rect,
    pub kind: Kind,
    pub enabled: bool,
    pub tooltip: Option<String>,
    /// Lies in the screen's scrolling list ([`Screen::list`]).
    pub scrolls: bool,
}

impl<Id> Widget<Id> {
    pub fn new(id: Id, rect: Rect, kind: Kind) -> Self {
        Self { id, rect, kind, enabled: true, tooltip: None, scrolls: false }
    }
    pub fn button(id: Id, rect: Rect, label: impl Into<String>) -> Self {
        Self::new(id, rect, Kind::Button(label.into()))
    }
    /// `steps` is the number of steps across the range: one per value for
    /// whole-number options, 100 for a percentage.
    pub fn slider(id: Id, rect: Rect, label: impl Into<String>, value: f32, steps: u32) -> Self {
        Self::new(id, rect, Kind::Slider { label: label.into(), value: value.clamp(0.0, 1.0), steps: steps.max(1) })
    }
    pub fn field(id: Id, rect: Rect) -> Self {
        Self::new(id, rect, Kind::Field)
    }
    pub fn custom(id: Id, rect: Rect) -> Self {
        Self::new(id, rect, Kind::Custom)
    }
    #[allow(dead_code, reason = "for the v0.11 options section headings")]
    pub fn label(id: Id, rect: Rect, text: impl Into<String>) -> Self {
        Self::new(id, rect, Kind::Label(text.into()))
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
    pub fn scrolling(mut self) -> Self {
        self.scrolls = true;
        self
    }
    fn focusable(&self) -> bool {
        self.enabled && !matches!(self.kind, Kind::Label(_))
    }
}

/// How a widget should look this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Look {
    pub hovered: bool,
    /// Has keyboard or controller focus.
    pub focused: bool,
    pub enabled: bool,
    /// A focused slider the arrow keys adjust (Java's `canChangeValue`).
    pub editing: bool,
}

impl Look {
    pub fn hot(self) -> bool {
        self.enabled && (self.hovered || self.focused)
    }
}

/// The app side of a stack of screens: a game with a world open, or the
/// title screen.
pub(super) trait Host: 'static {
    /// What a screen can ask the app to do beyond changing screens.
    type Out;
    /// The UI click sound.
    fn click(&mut self);
    /// Whether a world is open behind the menus.
    fn in_world(&self) -> bool;
}

/// What a screen asks the stack to do after an input.
pub(super) enum Step<H: Host> {
    Stay,
    Push(Box<dyn Dyn<H>>),
    /// Back to the screen below (closing the stack at its last screen).
    Pop,
    /// The top screen swapped for another.
    Replace(Box<dyn Dyn<H>>),
    /// Close every screen.
    Close,
    Out(H::Out),
    /// The first step, then the second.
    Then(Box<Step<H>>, Box<Step<H>>),
}

impl<H: Host> Step<H> {
    pub fn push(screen: impl Screen<H>) -> Self {
        Step::Push(Box::new(screen))
    }
    pub fn replace(screen: impl Screen<H>) -> Self {
        Step::Replace(Box::new(screen))
    }
    pub fn then(self, next: Step<H>) -> Self {
        Step::Then(Box::new(self), Box::new(next))
    }
}

/// A key or mouse press a screen can take before the stack does
/// ([`Screen::raw`]), as the key binding screen does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Raw {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Menu input from a controller (or a test): the D-pad or left stick, A and B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Nav {
    Up,
    Down,
    Left,
    Right,
    Activate,
    Back,
}

impl Nav {
    /// A controller button's menu meaning.
    pub fn from_pad(button: gilrs::Button) -> Option<Self> {
        use gilrs::Button as B;
        Some(match button {
            B::DPadUp => Nav::Up,
            B::DPadDown => Nav::Down,
            B::DPadLeft => Nav::Left,
            B::DPadRight => Nav::Right,
            B::South => Nav::Activate,
            B::East => Nav::Back,
            _ => return None,
        })
    }
}

/// A key press as menus see it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Key {
    pub code: KeyCode,
    /// Printable text it types, if any (none while Ctrl or Cmd is held).
    pub text: Option<String>,
    pub repeat: bool,
    pub shift: bool,
    /// Ctrl, or Cmd on macOS: Java's "control" for shortcuts.
    pub command: bool,
    pub alt: bool,
}

impl Key {
    #[cfg(test)]
    pub fn new(code: KeyCode) -> Self {
        Self { code, text: None, repeat: false, shift: false, command: false, alt: false }
    }

    /// A press from winit; `None` for releases and keys without a code.
    pub fn from_event(event: &KeyEvent, mods: ModifiersState) -> Option<Self> {
        if !event.state.is_pressed() {
            return None;
        }
        let PhysicalKey::Code(code) = event.physical_key else { return None };
        let command = if cfg!(target_os = "macos") { mods.super_key() } else { mods.control_key() };
        let text = event
            .text
            .as_ref()
            .filter(|_| !command && !mods.control_key())
            .map(|t| t.chars().filter(|c| text_field::allowed(*c)).collect::<String>())
            .filter(|t| !t.is_empty());
        Some(Self { code, text, repeat: event.repeat, shift: mods.shift_key(), command, alt: mods.alt_key() })
    }

    #[cfg(test)]
    pub fn typed(text: &str) -> Self {
        Self { text: Some(text.into()), ..Self::new(KeyCode::KeyA) }
    }
}

/// One menu screen. `H` is the app it runs in; screens shared by the title
/// screen and the game (Options) are generic over it.
pub(super) trait Screen<H: Host>: 'static {
    type Id: Copy + PartialEq + std::fmt::Debug;

    fn title(&self, host: &H) -> String;
    /// Every widget, in Tab order, for this state and screen size.
    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Self::Id>>;
    /// A button, custom widget or text field (Enter) was activated.
    fn activate(&mut self, host: &mut H, id: Self::Id) -> Step<H>;
    /// A widget was clicked with the mouse. List rows select on a click and
    /// act on Enter or a double click; everything else activates.
    fn click(&mut self, host: &mut H, id: Self::Id) -> Step<H> {
        self.activate(host, id)
    }
    /// Keyboard, controller or mouse focus moved to a widget, as Java's
    /// lists select the focused row and tab bars show the focused tab.
    fn focus(&mut self, _host: &mut H, _id: Self::Id) {}
    /// A slider was moved to `t` (0..1).
    fn slide(&mut self, _host: &mut H, _id: Self::Id, _t: f32) {}
    fn field(&mut self, _id: Self::Id) -> Option<&mut TextField> {
        None
    }
    fn field_ref(&self, _id: Self::Id) -> Option<&TextField> {
        None
    }
    /// Escape or the controller's back button.
    fn back(&mut self, _host: &mut H) -> Step<H> {
        Step::Pop
    }
    /// Every key and mouse press, before the stack handles it; `Some`
    /// consumes it.
    fn raw(&mut self, _host: &mut H, _input: Raw) -> Option<Step<H>> {
        None
    }
    /// Once a frame while this screen is on top.
    fn tick(&mut self, _host: &mut H) -> Step<H> {
        Step::Stay
    }
    /// The region the scrolling widgets show in.
    fn list(&self, _host: &H, _size: Size) -> Option<Rect> {
        None
    }
    /// How far one mouse wheel notch scrolls the list.
    fn scroll_step(&self) -> f32 {
        layout::ROW
    }
    /// The widget focused when the screen opens (a text field, as in Java).
    fn initial_focus(&self) -> Option<Self::Id> {
        None
    }
    /// Where the title goes, given the top of the first widget.
    fn title_y(&self, _size: Size, _first_widget: f32) -> f32 {
        layout::TITLE_Y
    }
    fn background(&self, host: &H, ui: &mut Ui, (sw, sh): Size) {
        // Blending is in linear space: it takes a high alpha to look dark.
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, if host.in_world() { 0.78 } else { 0.6 }]);
    }
    /// Anything the widgets don't draw, under them. `scroll` is how far the
    /// list is scrolled, in UI pixels.
    fn draw(&self, _host: &H, _ui: &mut Ui, _size: Size, _scroll: f32) {}
    fn draw_custom(&self, _host: &H, _ui: &mut Ui, _id: Self::Id, _rect: Rect, _look: Look) {}
}

/// A widget with its screen id erased, as the stack sees it.
#[derive(Clone, Debug)]
pub(super) struct Slot {
    pub rect: Rect,
    pub focusable: bool,
    /// On screen (not scrolled out of the list).
    pub shown: bool,
    /// In the scrolling list.
    pub scrolls: bool,
    /// Slider position and steps.
    pub slider: Option<(f32, u32)>,
    pub field: bool,
}

pub(super) struct Shape {
    pub slots: Vec<Slot>,
    pub list: Option<Rect>,
    pub max_scroll: f32,
}

/// What the stack tells a screen about drawing.
pub(super) struct Frame {
    pub hovered: Option<usize>,
    /// Keyboard or controller focus, or a focused text field.
    pub focus: Option<usize>,
    pub drag: Option<usize>,
    pub editing: bool,
    pub caret: bool,
    pub scroll: f32,
    /// Whose tooltip to show, and where.
    pub tooltip: Option<(usize, (f32, f32))>,
}

/// A screen behind the stack's back: [`Screen`] with its ids turned into
/// widget indices. Every screen gets it from the blanket impl below.
pub(super) trait Dyn<H: Host> {
    fn shape(&self, host: &H, size: Size, scroll: f32) -> Shape;
    fn activate_at(&mut self, host: &mut H, size: Size, index: usize) -> Step<H>;
    fn click_at(&mut self, host: &mut H, size: Size, index: usize) -> Step<H>;
    fn focus_at(&mut self, host: &mut H, size: Size, index: usize);
    fn slide_at(&mut self, host: &mut H, size: Size, index: usize, t: f32);
    fn field_at(&mut self, host: &H, size: Size, index: usize) -> Option<&mut TextField>;
    fn back_dyn(&mut self, host: &mut H) -> Step<H>;
    fn raw_dyn(&mut self, host: &mut H, input: Raw) -> Option<Step<H>>;
    fn tick_dyn(&mut self, host: &mut H) -> Step<H>;
    fn scroll_step_dyn(&self) -> f32;
    fn initial_focus_at(&self, host: &H, size: Size) -> Option<usize>;
    fn draw_dyn(&self, host: &H, ui: &mut Ui, frame: &Frame);
}

/// Scrolling widgets moved by `scroll` and hidden outside the list.
fn place<Id>(widgets: &mut [Widget<Id>], list: Option<Rect>, scroll: f32) -> (Vec<bool>, f32) {
    let Some([_, top, _, h]) = list else { return (vec![true; widgets.len()], 0.0) };
    let bottom = widgets.iter().filter(|w| w.scrolls).map(|w| w.rect[1] + w.rect[3]).fold(top, f32::max);
    let max_scroll = (bottom + GAP - (top + h)).max(0.0);
    let scroll = scroll.clamp(0.0, max_scroll);
    let shown = widgets
        .iter_mut()
        .map(|w| {
            if !w.scrolls {
                return true;
            }
            w.rect[1] -= scroll;
            w.rect[1] >= top - 0.5 && w.rect[1] + w.rect[3] <= top + h + 0.5
        })
        .collect();
    (shown, max_scroll)
}

impl<H: Host, S: Screen<H>> Dyn<H> for S {
    fn shape(&self, host: &H, size: Size, scroll: f32) -> Shape {
        let mut widgets = self.widgets(host, size);
        let list = self.list(host, size);
        let (shown, max_scroll) = place(&mut widgets, list, scroll);
        let slots = widgets
            .into_iter()
            .zip(shown)
            .map(|(w, shown)| Slot {
                rect: w.rect,
                focusable: w.focusable(),
                shown,
                scrolls: w.scrolls,
                slider: match w.kind {
                    Kind::Slider { value, steps, .. } => Some((value, steps)),
                    _ => None,
                },
                field: w.kind == Kind::Field,
            })
            .collect();
        Shape { slots, list, max_scroll }
    }

    fn activate_at(&mut self, host: &mut H, size: Size, index: usize) -> Step<H> {
        match self.widgets(host, size).get(index) {
            Some(w) if w.focusable() => self.activate(host, w.id),
            _ => Step::Stay,
        }
    }

    fn click_at(&mut self, host: &mut H, size: Size, index: usize) -> Step<H> {
        match self.widgets(host, size).get(index) {
            Some(w) if w.focusable() => self.click(host, w.id),
            _ => Step::Stay,
        }
    }

    fn focus_at(&mut self, host: &mut H, size: Size, index: usize) {
        if let Some(w) = self.widgets(host, size).get(index).filter(|w| w.focusable()) {
            self.focus(host, w.id);
        }
    }

    fn slide_at(&mut self, host: &mut H, size: Size, index: usize, t: f32) {
        if let Some(w) = self.widgets(host, size).get(index).filter(|w| w.enabled) {
            self.slide(host, w.id, t.clamp(0.0, 1.0));
        }
    }

    fn field_at(&mut self, host: &H, size: Size, index: usize) -> Option<&mut TextField> {
        let w = self.widgets(host, size).into_iter().nth(index).filter(|w| w.kind == Kind::Field && w.enabled)?;
        self.field(w.id)
    }

    fn back_dyn(&mut self, host: &mut H) -> Step<H> {
        self.back(host)
    }

    fn raw_dyn(&mut self, host: &mut H, input: Raw) -> Option<Step<H>> {
        self.raw(host, input)
    }

    fn tick_dyn(&mut self, host: &mut H) -> Step<H> {
        self.tick(host)
    }

    fn scroll_step_dyn(&self) -> f32 {
        self.scroll_step()
    }

    fn initial_focus_at(&self, host: &H, size: Size) -> Option<usize> {
        let id = self.initial_focus()?;
        self.widgets(host, size).iter().position(|w| w.id == id)
    }

    fn draw_dyn(&self, host: &H, ui: &mut Ui, frame: &Frame) {
        let size = ui.size();
        let mut widgets = self.widgets(host, size);
        let list = self.list(host, size);
        let (shown, max_scroll) = place(&mut widgets, list, frame.scroll);
        self.background(host, ui, size);
        let title = self.title(host);
        let first = widgets.iter().zip(&shown).find(|(_, s)| **s).map_or(size.1 / 2.0, |(w, _)| w.rect[1]);
        ui.text(((size.0 - Ui::text_width(&title)) / 2.0).floor(), self.title_y(size, first), &title, WHITE);
        self.draw(host, ui, size, frame.scroll.clamp(0.0, max_scroll));
        for (i, w) in widgets.iter().enumerate().filter(|(i, _)| shown[*i]) {
            let look = Look {
                hovered: frame.hovered == Some(i) || frame.drag == Some(i),
                focused: frame.focus == Some(i),
                enabled: w.enabled,
                editing: frame.focus == Some(i) && frame.editing,
            };
            match &w.kind {
                Kind::Button(label) => draw_button(ui, w.rect, label, look),
                Kind::Slider { label, value, .. } => draw_slider(ui, w.rect, label, *value, look),
                Kind::Field => {
                    if let Some(field) = self.field_ref(w.id) {
                        field.draw(ui, w.rect, look.focused, frame.caret && look.focused, w.enabled);
                    }
                }
                Kind::Custom => self.draw_custom(host, ui, w.id, w.rect, look),
                Kind::Label(text) => label_text(ui, w.rect, text, 0.0, WHITE),
            }
        }
        if let (Some([x, y, w, h]), true) = (list, max_scroll > 0.0) {
            // Java's scroll bar: a dark track at the list's right edge with a
            // grey thumb sized to the visible part.
            let bar = [x + w - 6.0, y, 6.0, h];
            ui.rect(bar[0], bar[1], bar[2], bar[3], [0.0, 0.0, 0.0, 1.0]);
            let thumb = (h * h / (h + max_scroll)).clamp(32.0_f32.min(h), h);
            let ty = y + (h - thumb) * (frame.scroll.clamp(0.0, max_scroll) / max_scroll);
            ui.rect(bar[0], ty, 6.0, thumb, [0.5, 0.5, 0.5, 1.0]);
            ui.rect(bar[0], ty, 5.0, thumb - 1.0, [0.75, 0.75, 0.75, 1.0]);
        }
        if let Some((i, at)) = frame.tooltip
            && let Some(text) = widgets.get(i).filter(|_| shown[i]).and_then(|w| w.tooltip.as_deref())
        {
            draw_tooltip(ui, text, at);
        }
    }
}

struct Entry<H: Host> {
    screen: Box<dyn Dyn<H>>,
    focus: Option<usize>,
    scroll: f32,
    /// The initial focus is looked up the first time the screen gets input
    /// or is drawn, when the size is known.
    fresh: bool,
}

/// The open screens, top last, with the pointer and focus state they share.
pub(super) struct Stack<H: Host> {
    entries: Vec<Entry<H>>,
    cursor: (f32, f32),
    /// A slider held down by the mouse.
    drag: Option<usize>,
    /// The last input came from the keyboard or a controller: show focus.
    keyboard: bool,
    /// Left and Right adjust the focused slider instead of moving focus, as
    /// in Java: on when the mouse or Tab focuses a slider, toggled by Enter,
    /// Space or A, and off when the arrow keys or D-pad move onto it.
    editing: bool,
    started: Instant,
    clipboard: Box<dyn Clipboard>,
}

impl<H: Host> Default for Stack<H> {
    fn default() -> Self {
        Self::with_clipboard(Box::new(SystemClipboard))
    }
}

impl<H: Host> Stack<H> {
    pub fn with_clipboard(clipboard: Box<dyn Clipboard>) -> Self {
        Self {
            entries: Vec::new(),
            cursor: (-1.0, -1.0),
            drag: None,
            keyboard: false,
            editing: false,
            started: Instant::now(),
            clipboard,
        }
    }

    pub fn is_open(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn open(&mut self, screen: impl Screen<H>) {
        self.push(Box::new(screen));
    }

    pub fn close(&mut self) {
        self.entries.clear();
        self.drag = None;
    }

    fn push(&mut self, screen: Box<dyn Dyn<H>>) {
        self.entries.push(Entry { screen, focus: None, scroll: 0.0, fresh: true });
        self.drag = None;
        self.editing = false;
    }

    /// The top screen, with its initial focus settled.
    fn top(&mut self, host: &H, size: Size) -> Option<&mut Entry<H>> {
        top(&mut self.entries, host, size)
    }

    /// Does what a screen would ask for, from outside the stack (the app
    /// opening a world or a connection on the player's behalf).
    pub fn run(&mut self, step: Step<H>) -> Option<H::Out> {
        self.apply(step)
    }

    fn apply(&mut self, step: Step<H>) -> Option<H::Out> {
        match step {
            Step::Stay => None,
            Step::Push(screen) => {
                self.push(screen);
                None
            }
            Step::Pop => {
                self.entries.pop();
                self.drag = None;
                self.editing = false;
                None
            }
            Step::Replace(screen) => {
                self.entries.pop();
                self.push(screen);
                None
            }
            Step::Close => {
                self.close();
                None
            }
            Step::Out(out) => Some(out),
            Step::Then(first, next) => {
                let a = self.apply(*first);
                let b = self.apply(*next);
                a.or(b)
            }
        }
    }

    fn hovered(&self, shape: &Shape) -> Option<usize> {
        shape.slots.iter().position(|s| s.shown && inside(s.rect, self.cursor))
    }

    pub fn mouse_moved(&mut self, host: &mut H, pos: (f32, f32), size: Size) {
        if pos != self.cursor {
            self.keyboard = false;
        }
        self.cursor = pos;
        let Some(i) = self.drag else { return };
        let Some(entry) = self.top(host, size) else { return };
        let shape = entry.screen.shape(host, size, entry.scroll);
        if let Some(slot) = shape.slots.get(i) {
            let t = slider_t(slot.rect, pos.0);
            entry.screen.slide_at(host, size, i, t);
        }
    }

    pub fn mouse_button(&mut self, host: &mut H, button: MouseButton, pressed: bool, size: Size) -> Option<H::Out> {
        if !pressed {
            if button == MouseButton::Left {
                self.drag = None;
            }
            return None;
        }
        let entry = self.top(host, size)?;
        if let Some(step) = entry.screen.raw_dyn(host, Raw::Mouse(button)) {
            return self.apply(step);
        }
        if button != MouseButton::Left {
            return None;
        }
        let shape = entry.screen.shape(host, size, entry.scroll);
        let i = self.hovered(&shape)?;
        let slot = &shape.slots[i];
        if !slot.focusable {
            return None;
        }
        self.keyboard = false;
        self.editing = true;
        let entry = self.entries.last_mut()?;
        if entry.focus != Some(i) {
            entry.focus = Some(i);
            entry.screen.focus_at(host, size, i);
        }
        if slot.field {
            let x = self.cursor.0 - slot.rect[0];
            if let Some(field) = entry.screen.field_at(host, size, i) {
                field.click(x);
            }
            return None;
        }
        host.click();
        if slot.slider.is_some() {
            self.drag = Some(i);
            entry.screen.slide_at(host, size, i, slider_t(slot.rect, self.cursor.0));
            return None;
        }
        let step = entry.screen.click_at(host, size, i);
        self.apply(step)
    }

    /// Mouse wheel: positive `lines` scroll up.
    pub fn wheel(&mut self, host: &mut H, lines: f32, size: Size) {
        let Some(entry) = self.top(host, size) else { return };
        let shape = entry.screen.shape(host, size, entry.scroll);
        let step = entry.screen.scroll_step_dyn();
        entry.scroll = (entry.scroll - lines * step).clamp(0.0, shape.max_scroll);
    }

    pub fn key(&mut self, host: &mut H, key: &Key, size: Size) -> Option<H::Out> {
        let entry = top(&mut self.entries, host, size)?;
        if !key.repeat
            && let Some(step) = entry.screen.raw_dyn(host, Raw::Key(key.code))
        {
            return self.apply(step);
        }
        let shape = entry.screen.shape(host, size, entry.scroll);
        let focus = entry.focus.filter(|&i| shape.slots.get(i).is_some_and(|s| s.focusable));
        if let Some(i) = focus.filter(|&i| shape.slots[i].field)
            && !matches!(key.code, KeyCode::Escape | KeyCode::Tab | KeyCode::ArrowUp | KeyCode::ArrowDown)
            && let Some(field) = entry.screen.field_at(host, size, i)
        {
            if matches!(key.code, KeyCode::Enter | KeyCode::NumpadEnter) {
                if key.repeat {
                    return None;
                }
                let step = entry.screen.activate_at(host, size, i);
                return self.apply(step);
            }
            field.key(key, self.clipboard.as_mut());
            return None;
        }
        match key.code {
            KeyCode::Escape if !key.repeat => self.nav(host, Nav::Back, size),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if !key.repeat => {
                self.nav(host, Nav::Activate, size)
            }
            KeyCode::Tab => {
                self.keyboard = true;
                self.editing = true;
                let entry = self.entries.last_mut()?;
                entry.focus = cycle(&shape, focus, if key.shift { -1 } else { 1 });
                reveal(entry, &shape);
                notify_focus(entry, host, size, focus);
                None
            }
            KeyCode::ArrowUp => self.nav(host, Nav::Up, size),
            KeyCode::ArrowDown => self.nav(host, Nav::Down, size),
            KeyCode::ArrowLeft => self.nav(host, Nav::Left, size),
            KeyCode::ArrowRight => self.nav(host, Nav::Right, size),
            _ => None,
        }
    }

    /// Controller (or arrow key) navigation.
    pub fn nav(&mut self, host: &mut H, nav: Nav, size: Size) -> Option<H::Out> {
        self.keyboard = true;
        let entry = top(&mut self.entries, host, size)?;
        let shape = entry.screen.shape(host, size, entry.scroll);
        let focus = entry.focus.filter(|&i| shape.slots.get(i).is_some_and(|s| s.focusable));
        match nav {
            Nav::Back => {
                let step = entry.screen.back_dyn(host);
                self.apply(step)
            }
            // Java's sliders take Enter, Space or A as "adjust me" (or "stop").
            Nav::Activate if focus.is_some_and(|i| shape.slots[i].slider.is_some()) => {
                self.editing = !self.editing;
                None
            }
            Nav::Activate => {
                let i = focus?;
                host.click();
                let entry = self.entries.last_mut()?;
                let step = entry.screen.activate_at(host, size, i);
                self.apply(step)
            }
            Nav::Left | Nav::Right if self.editing && focus.is_some_and(|i| shape.slots[i].slider.is_some()) => {
                let i = focus?;
                let (value, steps) = shape.slots[i].slider?;
                let dir = if nav == Nav::Left { -1.0 } else { 1.0 };
                let t = ((value * steps as f32).round() + dir) / steps as f32;
                entry.screen.slide_at(host, size, i, t);
                None
            }
            _ => {
                self.editing = false;
                entry.focus = match focus {
                    None => first(&shape),
                    Some(i) => neighbour(&shape, i, nav).or(Some(i)),
                };
                reveal(entry, &shape);
                notify_focus(entry, host, size, focus);
                None
            }
        }
    }

    /// How many screens are open, for tests.
    #[cfg(test)]
    pub fn depth(&self) -> usize {
        self.entries.len()
    }

    /// The top screen's widgets and focus, for tests.
    #[cfg(test)]
    pub fn peek(&self, host: &H, size: Size) -> (Shape, Option<usize>) {
        let entry = self.entries.last().expect("a screen");
        (entry.screen.shape(host, size, entry.scroll), entry.focus)
    }

    pub fn tick(&mut self, host: &mut H, size: Size) -> Option<H::Out> {
        let entry = self.top(host, size)?;
        let step = entry.screen.tick_dyn(host);
        self.apply(step)
    }

    pub fn draw(&self, host: &H, ui: &mut Ui) {
        let size = ui.size();
        let (cursor, keyboard) = (self.cursor, self.keyboard);
        let caret = (self.started.elapsed().as_millis() / 300).is_multiple_of(2);
        let Some(entry) = self.entries.last() else { return };
        let shape = entry.screen.shape(host, size, entry.scroll);
        let focus = if entry.fresh { entry.screen.initial_focus_at(host, size) } else { entry.focus };
        let hovered = shape.slots.iter().position(|s| s.shown && inside(s.rect, cursor));
        let focus = focus.filter(|&i| shape.slots.get(i).is_some_and(|s| s.focusable && s.shown));
        let field_focus = focus.filter(|&i| shape.slots[i].field);
        let tooltip = match (keyboard, focus, hovered) {
            (true, Some(i), _) => {
                let [x, y, _, h] = shape.slots[i].rect;
                Some((i, (x, y + h + 2.0)))
            }
            (false, _, Some(i)) => Some((i, (cursor.0 + 10.0, cursor.1 - 14.0))),
            _ => None,
        };
        let frame = Frame {
            hovered: if keyboard { None } else { hovered },
            focus: if keyboard { focus } else { field_focus },
            drag: self.drag,
            editing: self.editing,
            caret,
            scroll: entry.scroll,
            tooltip,
        };
        entry.screen.draw_dyn(host, ui, &frame);
    }
}

/// The top screen, with its initial focus settled.
fn top<'a, H: Host>(entries: &'a mut [Entry<H>], host: &H, size: Size) -> Option<&'a mut Entry<H>> {
    let entry = entries.last_mut()?;
    if std::mem::take(&mut entry.fresh) {
        entry.focus = entry.screen.initial_focus_at(host, size);
    }
    Some(entry)
}

/// Tells the screen its focus moved away from `before`.
fn notify_focus<H: Host>(entry: &mut Entry<H>, host: &mut H, size: Size, before: Option<usize>) {
    if let Some(i) = entry.focus.filter(|&i| Some(i) != before) {
        entry.screen.focus_at(host, size, i);
    }
}

/// Moves the list so the focused widget shows.
fn reveal<H: Host>(entry: &mut Entry<H>, shape: &Shape) {
    let (Some(i), Some([_, top, _, h])) = (entry.focus, shape.list) else { return };
    let slot = &shape.slots[i];
    // Rects are already scrolled by `entry.scroll`.
    let [_, y, _, sh] = slot.rect;
    if y < top {
        entry.scroll = (entry.scroll - (top - y)).max(0.0);
    } else if y + sh > top + h {
        entry.scroll = (entry.scroll + (y + sh - top - h)).min(shape.max_scroll);
    }
}

fn first(shape: &Shape) -> Option<usize> {
    shape.slots.iter().position(|s| s.focusable)
}

/// Tab order, wrapping around.
fn cycle(shape: &Shape, from: Option<usize>, dir: isize) -> Option<usize> {
    let n = shape.slots.len() as isize;
    let start = from.map_or(if dir > 0 { -1 } else { n }, |i| i as isize);
    (1..=n).map(|k| (start + k * dir).rem_euclid(n) as usize).find(|&i| shape.slots[i].focusable)
}

/// The nearest focusable widget in a direction: one overlapping this one
/// across the direction if any, else the closest by distance. Within a
/// scrolling list, rows scrolled out of view count too, so they're reached
/// in order; between the list and the rest of the screen only rows on screen
/// count.
fn neighbour(shape: &Shape, from: usize, nav: Nav) -> Option<usize> {
    let f = &shape.slots[from];
    let vertical = matches!(nav, Nav::Up | Nav::Down);
    shape
        .slots
        .iter()
        .enumerate()
        .filter(|&(i, s)| i != from && s.focusable && (s.shown || (f.scrolls && s.scrolls)))
        .filter_map(|(i, s)| {
            let [fx, fy, fw, fh] = f.rect;
            let [x, y, w, h] = s.rect;
            let (fcx, fcy, cx, cy) = (fx + fw / 2.0, fy + fh / 2.0, x + w / 2.0, y + h / 2.0);
            let ahead = match nav {
                Nav::Up => fcy - cy,
                Nav::Down => cy - fcy,
                Nav::Left => fcx - cx,
                Nav::Right => cx - fcx,
                _ => return None,
            };
            if ahead < 1.0 {
                return None;
            }
            let overlaps = if vertical { x < fx + fw && fx < x + w } else { y < fy + fh && fy < y + h };
            let across = if vertical { (cx - fcx).abs() } else { (cy - fcy).abs() };
            // Rows ahead in the list come before anything outside it, so Down
            // from a wide row doesn't skip to a centred Done below the list.
            let leaves_list = f.scrolls && !s.scrolls;
            Some((i, ((leaves_list, !overlaps), ahead + across * 2.0)))
        })
        .min_by(|a, b| a.1.0.cmp(&b.1.0).then(a.1.1.total_cmp(&b.1.1)))
        .map(|(i, _)| i)
}

pub(super) fn inside([x, y, w, h]: Rect, (mx, my): (f32, f32)) -> bool {
    mx >= x && mx < x + w && my >= y && my < y + h
}

/// Slider position for a mouse x over a slider.
fn slider_t([x, _, w, _]: Rect, mx: f32) -> f32 {
    ((mx - x - HANDLE_W / 2.0) / (w - HANDLE_W)).clamp(0.0, 1.0)
}

/// A filled box with a black outline and (if `raised`) light top-left and
/// dark bottom-right edges.
pub(super) fn bevel(ui: &mut Ui, [x, y, w, h]: Rect, fill: Color, raised: bool) {
    ui.rect(x, y, w, h, [0.0, 0.0, 0.0, 1.0]);
    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, fill);
    if raised {
        let light = [fill[0] + 0.25, fill[1] + 0.25, fill[2] + 0.25, 1.0];
        let dark = [fill[0] * 0.55, fill[1] * 0.55, fill[2] * 0.55, 1.0];
        ui.rect(x + 1.0, y + 1.0, w - 2.0, 1.0, light);
        ui.rect(x + 1.0, y + 1.0, 1.0, h - 2.0, light);
        ui.rect(x + 1.0, y + h - 3.0, w - 2.0, 2.0, dark);
        ui.rect(x + w - 2.0, y + 1.0, 1.0, h - 2.0, dark);
    }
}

/// A white frame around a focused widget, as Java draws keyboard focus.
pub(super) fn focus_frame(ui: &mut Ui, [x, y, w, h]: Rect) {
    ui.rect(x, y, w, 1.0, WHITE);
    ui.rect(x, y + h - 1.0, w, 1.0, WHITE);
    ui.rect(x, y, 1.0, h, WHITE);
    ui.rect(x + w - 1.0, y, 1.0, h, WHITE);
}

pub(super) fn text_color(look: Look) -> Color {
    match (look.enabled, look.hot()) {
        (false, _) => GREY,
        (true, true) => HOT_TEXT,
        (true, false) => WHITE,
    }
}

pub(super) fn draw_button(ui: &mut Ui, rect: Rect, label: &str, look: Look) {
    let fill = if !look.enabled {
        DISABLED
    } else if look.hot() {
        HOT
    } else {
        BUTTON
    };
    bevel(ui, rect, fill, look.enabled);
    if look.focused && look.enabled {
        focus_frame(ui, rect);
    }
    label_text(ui, rect, label, 4.0, text_color(look));
}

/// A widget's label, centred, or scrolling back and forth when it's wider
/// than the widget (Java's `renderScrollingString`).
pub(super) fn label_text(ui: &mut Ui, [x, y, w, h]: Rect, label: &str, margin: f32, color: Color) {
    let ty = y + ((h - 8.0) / 2.0).floor();
    let (min_x, max_x) = (x + margin, x + w - margin);
    let width = Ui::text_width(label);
    let overflow = width - (max_x - min_x);
    if overflow <= 0.0 {
        ui.text((x + (w - width) / 2.0).floor(), ty, label, color);
        return;
    }
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let period = (overflow as f64 * 0.5).max(3.0);
    let phase = (std::f64::consts::FRAC_PI_2 * (std::f64::consts::TAU * t / period).cos()).sin() / 2.0 + 0.5;
    let offset = (phase * overflow as f64) as f32;
    ui.text_clipped((min_x - offset).floor(), ty, label, color, (min_x, max_x));
}

pub(super) fn draw_slider(ui: &mut Ui, rect: Rect, label: &str, value: f32, look: Look) {
    let [x, y, w, h] = rect;
    // Minecraft-style: a dark track with a button-like handle.
    bevel(ui, rect, [0.16, 0.16, 0.16, 1.0], false);
    let hx = x + (w - HANDLE_W) * value.clamp(0.0, 1.0);
    // The handle lights up while the mouse is over it or the keys adjust it.
    let fill = if !look.enabled {
        DISABLED
    } else if look.hovered || look.editing {
        HOT
    } else {
        BUTTON
    };
    bevel(ui, [hx, y, HANDLE_W, h], fill, look.enabled);
    if look.focused && look.enabled {
        focus_frame(ui, rect);
    }
    label_text(ui, rect, label, 4.0, text_color(look));
}

/// A tooltip box near `at`, kept on screen, its text wrapped like Java's.
pub(super) fn draw_tooltip(ui: &mut Ui, text: &str, (ax, ay): (f32, f32)) {
    let (sw, sh) = ui.size();
    let lines: Vec<String> = text.lines().flat_map(|l| wrap(l, 200.0)).collect();
    let w = lines.iter().map(|l| Ui::text_width(l)).fold(0.0, f32::max) + 8.0;
    let h = lines.len() as f32 * 10.0 + 6.0;
    let x = ax.min(sw - w - 2.0).max(2.0);
    let y = if ay + h > sh - 2.0 { (ay - h - 24.0).max(2.0) } else { ay.max(2.0) };
    ui.rect(x, y, w, h, [0.31, 0.0, 1.0, 0.6]);
    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [0.06, 0.0, 0.06, 0.94]);
    for (i, line) in lines.iter().enumerate() {
        ui.text(x + 4.0, y + 4.0 + i as f32 * 10.0, line, WHITE);
    }
}

/// `text`, cut short with "..." if it's wider than `width` UI pixels.
pub(super) fn fit(text: &str, width: f32) -> String {
    if Ui::text_width(text) <= width {
        return text.to_string();
    }
    let mut cut = text.to_string();
    while !cut.is_empty() && Ui::text_width(&format!("{cut}...")) > width {
        cut.pop();
    }
    format!("{}...", cut.trim_end())
}

/// `text` broken into lines no wider than `width` UI pixels, at spaces
/// where possible. Leading spaces (list indentation) are kept.
pub(super) fn wrap(text: &str, width: f32) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let mut lines = Vec::new();
    let mut line = indent.clone();
    for word in text.split_whitespace() {
        let candidate = if line.trim().is_empty() { format!("{line}{word}") } else { format!("{line} {word}") };
        if Ui::text_width(&candidate) <= width || line.trim().is_empty() {
            line = candidate;
        } else {
            lines.push(fit(&line, width));
            line = format!("{indent}  {word}");
        }
    }
    lines.push(fit(&line, width));
    lines
}

/// "ON" or "OFF", as Java labels its toggles.
pub(super) fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

#[cfg(test)]
pub(super) mod tests;
