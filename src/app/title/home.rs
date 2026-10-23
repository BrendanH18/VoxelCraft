//! Java's title screen (`TitleScreen`): the logo, Singleplayer, Multiplayer,
//! then Options... and Quit Game side by side, with the version in the
//! bottom-left corner. VoxelCraft has no Realms, so its row and the icon row
//! under it (Friends, Language, Accessibility) are left out and Options...
//! moves up beneath Multiplayer. The update offer sits in the bottom-right
//! corner, where Java puts its copyright line.

use super::{TitleHost, create, select, worlds};
use crate::app::gui::{BUTTON_H, BUTTON_W, Screen, Size, Step, Widget};
use crate::app::options;
use crate::render::ui::{Ui, WHITE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Singleplayer,
    Multiplayer,
    Options,
    Quit,
    Update,
}

/// Java's half-width buttons (Options... and Quit Game).
const HALF_W: f32 = 98.0;
const UPDATE_W: f32 = 100.0;
const LOGO: &str = "VoxelCraft";

pub(in crate::app) struct Home;

impl Home {
    /// Java's `height / 4 + 48`, raised on short windows so the buttons
    /// stay clear of the version line and the update button.
    fn top((_, sh): Size) -> f32 {
        (sh / 4.0 + 48.0).min(sh - 112.0).max(64.0).floor()
    }

    /// The logo's scale: Java's 256-pixel logo is about 4x the font.
    fn logo_scale(sw: f32) -> f32 {
        if Ui::text_width(LOGO) * 4.0 <= sw - 16.0 { 4.0 } else { 3.0 }
    }
}

impl<H: TitleHost> Screen<H> for Home {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        String::new()
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let (sw, sh) = size;
        let cx = (sw / 2.0).floor();
        let y = Self::top(size);
        let mut out = vec![
            Widget::button(Id::Singleplayer, [cx - 100.0, y, BUTTON_W, BUTTON_H], "Singleplayer"),
            Widget::button(Id::Multiplayer, [cx - 100.0, y + 24.0, BUTTON_W, BUTTON_H], "Multiplayer"),
            Widget::button(Id::Options, [cx - 100.0, y + 60.0, HALF_W, BUTTON_H], "Options..."),
            Widget::button(Id::Quit, [cx + 2.0, y + 60.0, HALF_W, BUTTON_H], "Quit Game"),
        ];
        if let Some(label) = host.update_offer() {
            out.push(Widget::button(Id::Update, [sw - UPDATE_W - 2.0, sh - BUTTON_H - 2.0, UPDATE_W, BUTTON_H], label));
        }
        out
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            // Java goes straight to Create New World while there are no worlds.
            Id::Singleplayer if worlds::list(host.saves_dir()).is_empty() => Step::push(create::CreateWorld::new()),
            Id::Singleplayer => Step::push(select::SelectWorld::new(host)),
            Id::Multiplayer => host.multiplayer(),
            Id::Options => Step::push(options::Options),
            Id::Quit => Step::Out(super::Action::Quit),
            Id::Update => host.updates(),
        }
    }

    /// Escape does nothing on Java's title screen.
    fn back(&mut self, _: &mut H) -> Step<H> {
        Step::Stay
    }

    fn background(&self, _: &H, ui: &mut Ui, (sw, sh): Size) {
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.25]);
    }

    fn draw(&self, host: &H, ui: &mut Ui, (sw, sh): Size, _: f32) {
        let k = Self::logo_scale(sw);
        ui.text_scaled(((sw - Ui::text_width(LOGO) * k) / 2.0).floor(), 30.0, LOGO, WHITE, k);
        let version = format!("VoxelCraft {}", crate::update::Version::current().short());
        ui.text(2.0, sh - 10.0, &version, WHITE);
        if let Some(notice) = host.update_notice() {
            let room = sw - UPDATE_W - 12.0;
            ui.text(2.0, sh - 22.0, &crate::app::gui::fit(&notice, room), [0.6, 1.0, 0.6, 1.0]);
        }
    }
}
