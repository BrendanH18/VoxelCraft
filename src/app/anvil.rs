//! The anvil screen, as Java has it (without renaming): two inputs and a
//! result that repairs gear with its material, merges two of the same item
//! or adds an enchanted book's enchantments, for levels (see
//! `enchant::anvil`). Each use may chip the anvil.

use glam::IVec3;

use crate::enchant::AnvilResult;

use super::{Container, Game};

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
