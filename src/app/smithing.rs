//! Smithing table screen: upgrade template + diamond base + Netherite
//! ingot -> matching Netherite gear.

use glam::IVec3;

use crate::inventory::Stack;

use super::{Container, Game};

impl Game {
    pub(super) fn open_smithing(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.body.container = Container::Smithing(pos);
        self.toggle_inventory();
    }

    pub(super) fn smithing_result(&self) -> Option<Stack> {
        super::screen::smithing_result(&self.session)
    }
}
