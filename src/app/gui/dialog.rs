//! Java's confirm and alert screens: a question with two answers, and a
//! message with one button. Errors use [`Notice`] so the player sees what
//! happened and what they can do, instead of it only reaching the log.

use super::{BUTTON_H, BUTTON_W, GAP, GREY, Host, Rect, Screen, Size, Step, Widget, wrap};
use crate::render::ui::Ui;

const LINE: f32 = 10.0;

/// The message's lines and where the block starts, centred on screen.
fn block(lines: &[String], (sw, sh): Size) -> (Vec<String>, f32) {
    let wrapped: Vec<String> = lines.iter().flat_map(|l| wrap(l, (sw - 50.0).min(300.0))).collect();
    let total = LINE + 8.0 + wrapped.len() as f32 * LINE + 12.0 + BUTTON_H;
    (wrapped, ((sh - total) / 2.0).floor().max(4.0))
}

fn buttons_y(lines: usize, top: f32) -> f32 {
    top + LINE + 8.0 + lines as f32 * LINE + 12.0
}

fn draw_lines(ui: &mut Ui, lines: &[String], top: f32, sw: f32) {
    for (i, line) in lines.iter().enumerate() {
        let y = top + LINE + 8.0 + i as f32 * LINE;
        ui.text(((sw - Ui::text_width(line)) / 2.0).floor(), y, line, GREY);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Answer {
    Yes,
    No,
}

type Then<H> = Box<dyn FnOnce(&mut H) -> Step<H>>;

/// "Are you sure?": Yes runs `on_yes` on the screen below this one.
pub(in crate::app) struct Confirm<H: Host> {
    title: String,
    lines: Vec<String>,
    yes: String,
    no: String,
    on_yes: Option<Then<H>>,
}

impl<H: Host> Confirm<H> {
    pub fn new(
        title: impl Into<String>,
        lines: Vec<String>,
        yes: impl Into<String>,
        on_yes: impl FnOnce(&mut H) -> Step<H> + 'static,
    ) -> Self {
        Self { title: title.into(), lines, yes: yes.into(), no: "Cancel".into(), on_yes: Some(Box::new(on_yes)) }
    }

    /// The second answer's label (Java's Yes/No confirmations).
    pub fn no(mut self, label: impl Into<String>) -> Self {
        self.no = label.into();
        self
    }
}

impl<H: Host> Screen<H> for Confirm<H> {
    type Id = Answer;

    fn title(&self, _: &H) -> String {
        self.title.clone()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<Answer>> {
        let (lines, top) = block(&self.lines, size);
        let y = buttons_y(lines.len(), top);
        let w = 150.0;
        let x = (size.0 / 2.0).floor();
        let rect = |left: f32| -> Rect { [left, y, w, BUTTON_H] };
        vec![
            Widget::button(Answer::Yes, rect(x - w - GAP), self.yes.clone()),
            Widget::button(Answer::No, rect(x + GAP), self.no.clone()),
        ]
    }

    fn activate(&mut self, host: &mut H, id: Answer) -> Step<H> {
        match (id, self.on_yes.take()) {
            (Answer::Yes, Some(then)) => Step::Pop.then(then(host)),
            _ => Step::Pop,
        }
    }

    fn title_y(&self, size: Size, _: f32) -> f32 {
        block(&self.lines, size).1
    }

    fn background(&self, _: &H, ui: &mut Ui, (sw, sh): Size) {
        ui.rect(0.0, 0.0, sw, sh, [0.25, 0.0, 0.0, 0.75]);
    }

    fn draw(&self, _: &H, ui: &mut Ui, size: Size, _: f32) {
        let (lines, top) = block(&self.lines, size);
        draw_lines(ui, &lines, top, size.0);
    }
}

/// A message with one button, which goes back.
pub(in crate::app) struct Notice {
    title: String,
    lines: Vec<String>,
    button: String,
}

impl Notice {
    pub fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self { title: title.into(), lines, button: "Back".into() }
    }
}

impl<H: Host> Screen<H> for Notice {
    type Id = ();

    fn title(&self, _: &H) -> String {
        self.title.clone()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<()>> {
        let (lines, top) = block(&self.lines, size);
        let y = buttons_y(lines.len(), top);
        vec![Widget::button((), [((size.0 - BUTTON_W) / 2.0).floor(), y, BUTTON_W, BUTTON_H], self.button.clone())]
    }

    fn activate(&mut self, _: &mut H, _: ()) -> Step<H> {
        Step::Pop
    }

    fn initial_focus(&self) -> Option<()> {
        Some(())
    }

    fn title_y(&self, size: Size, _: f32) -> f32 {
        block(&self.lines, size).1
    }

    fn draw(&self, _: &H, ui: &mut Ui, size: Size, _: f32) {
        let (lines, top) = block(&self.lines, size);
        draw_lines(ui, &lines, top, size.0);
    }
}
