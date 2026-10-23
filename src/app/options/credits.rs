//! Credits & Attribution: Java's screen of that name, with its Credits
//! button rolling VoxelCraft's credits (the end screen's scroll, without the
//! respawn that follows it). Java's Attribution and Licenses buttons open
//! Mojang web pages and are left out.

use std::cell::Cell;
use std::time::Instant;

use super::OptionsHost;
use crate::app::credits;
use crate::app::gui::{BUTTON_H, BUTTON_W, GAP, Screen, Size, Step, Widget};
use crate::render::ui::Ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Credits,
    Done,
}

pub(in crate::app) struct CreditsAndAttribution;

impl<H: OptionsHost> Screen<H> for CreditsAndAttribution {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Credits and Attribution".into()
    }

    fn widgets(&self, _: &H, (sw, sh): Size) -> Vec<Widget<Id>> {
        let x = ((sw - BUTTON_W) / 2.0).floor();
        let y = (sh / 2.0 - BUTTON_H - GAP / 2.0).floor();
        vec![
            Widget::button(Id::Credits, [x, y, BUTTON_W, BUTTON_H], "Credits"),
            Widget::button(Id::Done, [x, y + BUTTON_H + GAP * 2.0, BUTTON_W, BUTTON_H], "Done"),
        ]
    }

    fn activate(&mut self, _: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Credits => Step::push(Roll::default()),
            Id::Done => Step::Pop,
        }
    }

    fn title_y(&self, _: Size, first: f32) -> f32 {
        first - 20.0
    }
}

/// The credits scrolling up a black screen until they end or Escape.
pub(in crate::app) struct Roll {
    started: Instant,
    /// The screen height last drawn at, which sets when the scroll ends.
    height: Cell<f32>,
}

impl Default for Roll {
    fn default() -> Self {
        Self { started: Instant::now(), height: Cell::new(240.0) }
    }
}

#[cfg(test)]
impl Roll {
    /// A roll that has scrolled to its end.
    pub fn finished() -> Self {
        Self { started: Instant::now() - std::time::Duration::from_secs(3600), ..Self::default() }
    }
}

impl<H: OptionsHost> Screen<H> for Roll {
    type Id = ();

    fn title(&self, _: &H) -> String {
        String::new()
    }

    fn widgets(&self, _: &H, _: Size) -> Vec<Widget<()>> {
        Vec::new()
    }

    fn activate(&mut self, _: &mut H, _: ()) -> Step<H> {
        Step::Stay
    }

    fn tick(&mut self, _: &mut H) -> Step<H> {
        let t = self.started.elapsed().as_secs_f32();
        if t * credits::SPEED > credits::total_height(self.height.get()) { Step::Pop } else { Step::Stay }
    }

    fn background(&self, _: &H, ui: &mut Ui, (sw, sh): Size) {
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 1.0]);
    }

    fn draw(&self, _: &H, ui: &mut Ui, (_, sh): Size, _: f32) {
        self.height.set(sh);
        credits::draw(ui, self.started.elapsed().as_secs_f32());
    }
}
