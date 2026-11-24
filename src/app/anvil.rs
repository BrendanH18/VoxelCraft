//! The anvil screen, as Java has it: two inputs and a result that repairs
//! gear with its material, merges two of the same item or adds an enchanted
//! book's enchantments, for levels (see `enchant::anvil`), and (v0.18
//! Track 2) the name box that renames the result. Each use may chip the
//! anvil.

use glam::IVec3;
use winit::event::KeyEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::enchant::AnvilResult;
use crate::inventory::Stack;
use crate::render::ui::Ui;

use super::gui::{Key, TextField, style};
use super::{Container, Game};

/// `AnvilMenu.MAX_NAME_LENGTH`.
pub(super) const NAME_LENGTH: usize = crate::enchant::ANVIL_MAX_NAME_LENGTH;
/// `AnvilScreen`'s name box: its sprite at (59, 20), 110 x 16, and the
/// text at (62, 24), 103 wide, from the panel's corner.
pub(super) const NAME_BOX: [f32; 4] = [59.0, 20.0, 110.0, 16.0];
pub(super) const NAME_TEXT: (f32, f32, f32) = (62.0, 24.0, 103.0);

/// The desktop's name box: the text field and the left-hand stack it was
/// last reset for (`AnvilScreen.slotChanged`).
#[derive(Clone, Debug)]
pub(super) struct NameBox {
    pub field: TextField,
    pub item: Option<Stack>,
}

impl Default for NameBox {
    fn default() -> Self {
        Self { field: TextField::new(NAME_LENGTH), item: None }
    }
}

impl Game {
    /// Right-click on an anvil.
    pub(super) fn open_anvil(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.body.container = Container::Anvil(pos);
        self.toggle_inventory();
    }

    pub(super) fn open_grindstone(&mut self, pos: IVec3) {
        if !self.inventory_open {
            self.body.container = Container::Grindstone(pos);
            self.toggle_inventory();
        }
    }

    /// What the host's inputs make (see `screen::anvil_preview`).
    pub(super) fn anvil_preview(&self) -> Option<(AnvilResult, bool)> {
        super::screen::anvil_preview(self.body.container, &self.session)
    }
}

impl Game {
    /// Once a frame: when the anvil's left input changes, the name box
    /// shows the new item's name (or empties), as `AnvilScreen.slotChanged`
    /// does; closing the screen forgets it.
    pub(super) fn anvil_frame(&mut self) {
        let left = match self.body.container {
            Container::Anvil(_) if self.inventory_open => self.session.work[0],
            _ => {
                if self.anvil_name.item.take().is_some() {
                    self.session.clear_anvil_name();
                }
                return;
            }
        };
        if left != self.anvil_name.item {
            self.anvil_name.item = left;
            self.anvil_name.field.set(left.as_ref().map_or("", |s| s.display_name()));
        }
    }

    /// A key on the anvil screen: while the left input holds an item, the
    /// name box takes every key but Escape (`AnvilScreen.keyPressed` with
    /// `canConsumeInput`), so E and the number keys type.
    pub(super) fn anvil_key(&mut self, event: &KeyEvent) -> bool {
        if !self.inventory_open || !matches!(self.body.container, Container::Anvil(_)) {
            return false;
        }
        self.anvil_frame();
        if self.session.work[0].is_none() {
            return false;
        }
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            return false;
        }
        let Some(key) = Key::from_event(event, self.modifiers) else { return true };
        let before = self.anvil_name.field.text().to_string();
        let mut clipboard = super::gui::SystemClipboard;
        self.anvil_name.field.key(&key, &mut clipboard);
        if self.anvil_name.field.text() != before {
            let text = self.anvil_name.field.text().to_string();
            self.rename_in_anvil(&text);
        }
        true
    }

    /// The name box's text changed: `AnvilScreen.onNameChanged`. The item's
    /// own name typed back counts as no name; a LAN guest also tells the
    /// host (`ServerboundRenameItemPacket`).
    fn rename_in_anvil(&mut self, text: &str) {
        let Some(left) = self.session.work[0] else { return };
        let name = if left.name.as_str().is_none() && text == left.display_name() { "" } else { text };
        if self.session.set_anvil_name(name) && self.lan.client.is_some() {
            self.lan_command(&rename_command(name));
        }
    }

    /// Draws the name box over the anvil panel at `(px, py)`.
    pub(super) fn anvil_name_ui(&self, ui: &mut Ui, px: f32, py: f32) {
        let [x, y, w, h] = NAME_BOX;
        let editable = self.session.work[0].is_some();
        name_box(ui, [px + x, py + y, w, h], editable);
        if editable {
            let (tx, ty, tw) = NAME_TEXT;
            let caret = (self.started.elapsed().as_millis() / 300).is_multiple_of(2);
            // Java's box: white text, no border of its own, always focused.
            self.anvil_name.field.draw_plain(ui, [px + tx, py + ty, tw, 8.0], caret);
        }
    }
}

/// The name box's frame: a dark field in a 1-pixel inset frame, greyed and
/// empty while there's nothing to name.
pub(super) fn name_box(ui: &mut Ui, [x, y, w, h]: [f32; 4], editable: bool) {
    style::inset(ui, x, y, w, h, if editable { 0x000000 } else { style::token::PANEL_SHADE });
}

/// The LAN command that types `name` into a guest's anvil name box.
pub(super) fn rename_command(name: &str) -> String {
    format!("anvil_name {name}")
}

/// The name a LAN guest's `anvil_name` command carries.
pub(super) fn parse_rename(command: &str) -> Option<&str> {
    let rest = command.strip_prefix("anvil_name")?;
    if rest.is_empty() { Some("") } else { rest.strip_prefix(' ') }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_commands_round_trip_names_with_spaces_and_empty_ones() {
        for name in ["", "Sword", "The Sword of Doom", " padded "] {
            assert_eq!(parse_rename(&rename_command(name)), Some(name));
        }
        assert_eq!(parse_rename("anvil_name"), Some(""));
        assert_eq!(parse_rename("anvil_names x"), None);
        assert_eq!(parse_rename("anvil"), None);
    }

    #[test]
    fn the_name_box_sits_where_javas_does() {
        assert_eq!(NAME_BOX, [59.0, 20.0, 110.0, 16.0]);
        assert_eq!(NAME_LENGTH, 50);
        // The box ends inside the 176-wide panel, right of the title's start.
        const { assert!(NAME_BOX[0] + NAME_BOX[2] <= 176.0 - 7.0) };
    }
}
