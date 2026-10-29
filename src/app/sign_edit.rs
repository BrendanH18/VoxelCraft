//! Java's sign edit screen (`AbstractSignEditScreen`, `SignEditScreen` and
//! `HangingSignEditScreen`) on the menu toolkit: four lines on the sign's
//! board, each as wide as the sign allows (90 font pixels, 60 on hanging
//! signs), and Done. Up and Down move between lines; Done or Escape keeps
//! the text, as Java's `onClose` sends it. Like every menu here it pauses
//! an unshared world (Java's sign screen doesn't pause).

use glam::IVec3;

use super::Game;
use super::gui::{BUTTON_H, Screen, Size, Step, TextField, Widget};
use crate::render::ui::Ui;
use crate::world::wood::entities::MAX_LINE_CHARS;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SignId {
    Line(usize),
    Done,
}

pub(super) struct SignEdit {
    cell: IVec3,
    front: bool,
    hanging: bool,
    /// The board's colour, from the sign's planks.
    board: [f32; 4],
    lines: [TextField; 4],
}

impl SignEdit {
    fn max_width(&self) -> f32 {
        if self.hanging { 60.0 } else { 90.0 }
    }

    /// The lines as typed, cut to the sign's width.
    fn text(&self) -> [String; 4] {
        std::array::from_fn(|i| self.lines[i].text().to_string())
    }
}

impl Screen<Game> for SignEdit {
    type Id = SignId;

    fn title(&self, _: &Game) -> String {
        if self.hanging { "Edit Hanging Sign Message".into() } else { "Edit Sign Message".into() }
    }

    fn widgets(&self, _: &Game, (sw, sh): Size) -> Vec<Widget<SignId>> {
        let cx = (sw / 2.0).floor();
        let w = self.max_width() + 8.0;
        let top = (sh / 2.0 - 34.0).floor();
        let mut out: Vec<Widget<SignId>> = (0..4)
            .map(|i| Widget::field(SignId::Line(i), [cx - (w / 2.0).floor(), top + i as f32 * 14.0, w, 13.0]))
            .collect();
        // Java: Done at (width / 2 - 100, height / 4 + 144).
        let y = ((sh / 4.0 + 144.0).min(sh - BUTTON_H - 8.0)).floor();
        out.push(Widget::button(SignId::Done, [cx - 100.0, y, 200.0, BUTTON_H], "Done"));
        out
    }

    fn activate(&mut self, game: &mut Game, id: SignId) -> Step<Game> {
        match id {
            SignId::Done => {
                game.finish_sign_edit(self.cell, self.front, self.text());
                Step::Close
            }
            SignId::Line(_) => Step::Stay,
        }
    }

    fn back(&mut self, game: &mut Game) -> Step<Game> {
        game.finish_sign_edit(self.cell, self.front, self.text());
        Step::Close
    }

    fn field(&mut self, id: SignId) -> Option<&mut TextField> {
        match id {
            SignId::Line(i) => self.lines.get_mut(i),
            SignId::Done => None,
        }
    }

    fn field_ref(&self, id: SignId) -> Option<&TextField> {
        match id {
            SignId::Line(i) => self.lines.get(i),
            SignId::Done => None,
        }
    }

    fn initial_focus(&self) -> Option<SignId> {
        Some(SignId::Line(0))
    }

    /// Lines stop at the sign's width (Java's `SignEditScreen` refuses
    /// characters past it); the screen closes if the sign goes.
    fn tick(&mut self, game: &mut Game) -> Step<Game> {
        if game.world.sign_at(self.cell).is_none() {
            return Step::Close;
        }
        let max = self.max_width();
        for field in &mut self.lines {
            if Ui::text_width(field.text()) > max {
                let mut text = field.text().to_string();
                while Ui::text_width(&text) > max {
                    text.pop();
                }
                field.set(&text);
            }
        }
        Step::Stay
    }

    fn background(&self, _: &Game, ui: &mut Ui, (sw, sh): Size) {
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]);
        // The board behind the lines.
        let cx = (sw / 2.0).floor();
        let w = self.max_width() + 24.0;
        let top = (sh / 2.0 - 34.0).floor() - 8.0;
        ui.rect(cx - (w / 2.0).floor(), top, w, 4.0 * 14.0 + 14.0, self.board);
        if !self.hanging {
            ui.rect(cx - 3.0, top + 70.0, 6.0, 40.0, self.board.map(|c| c * 0.8));
        }
    }
}

impl Game {
    /// Opens the editor for one side of the sign at `cell` (Java's
    /// `openTextEdit`). A LAN client opens it when the host asks. `false`
    /// when there is no sign there.
    pub(super) fn open_sign_editor(&mut self, cell: IVec3, front: bool) -> bool {
        let Some((sign, entity)) = self.world.sign_at(cell) else { return false };
        let side = entity.side(front);
        let board = match sign.kind {
            crate::world::wood::Kind::Crimson => [0.40, 0.19, 0.27, 1.0],
            crate::world::wood::Kind::Warped => [0.17, 0.41, 0.39, 1.0],
            crate::world::wood::Kind::Cherry | crate::world::wood::Kind::PaleOak | crate::world::wood::Kind::Birch => {
                [0.80, 0.70, 0.55, 1.0]
            }
            crate::world::wood::Kind::DarkOak | crate::world::wood::Kind::Spruce => [0.30, 0.21, 0.12, 1.0],
            _ => [0.60, 0.47, 0.29, 1.0],
        };
        let lines = std::array::from_fn(|i| TextField::with_text(&side.lines[i], MAX_LINE_CHARS));
        let editor = SignEdit { cell, front, hanging: sign.hanging(), board, lines };
        if self.inventory_open {
            self.toggle_inventory();
        }
        self.menu.close();
        self.menu.open(editor);
        self.set_grab(false);
        self.keys.clear();
        self.body.left_held = false;
        self.body.right_held = false;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
        true
    }

    /// The edit screen closed: the text goes to the sign through the
    /// shared session rule (reach and build checks).
    pub(super) fn finish_sign_edit(&mut self, cell: IVec3, front: bool, lines: [String; 4]) {
        if self.lan.client.is_some() {
            // The host applies it through the same rule (`ServerboundSignUpdatePacket`).
            let side = if front { "front" } else { "back" };
            let lines = serde_json::to_string(&lines).unwrap_or_default();
            self.lan_command(&format!("sign {} {} {} {side} {lines}", cell.x, cell.y, cell.z));
            return;
        }
        if let Err(e) = self.session.edit_sign(&mut self.world, cell, front, lines) {
            log::info!("sign edit at {cell} not applied: {e}");
        }
    }
}
