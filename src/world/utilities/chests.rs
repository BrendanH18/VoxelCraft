//! Decorated pots (`DecoratedPotBlock`), which keep one stack; trapped
//! chests (`TrappedChestBlock`), whose redstone signal is the number of
//! players looking in; and ender chests (`EnderChestBlock`), whose
//! contents belong to the player.

use glam::IVec3;

use super::trapped_chest_of;
use crate::inventory::Stack;
use crate::world::{World, block::Block};

impl World {
    /// What the decorated pot at `p` holds.
    pub fn decorated_pot_item(&self, p: IVec3) -> Option<Stack> {
        self.utilities.decorated.get(&p).copied()
    }

    /// Adds one item to the decorated pot at `p` (`DecoratedPotBlock.useItemOn`).
    pub fn decorated_pot_insert(&mut self, p: IVec3, one: Stack) -> bool {
        if !self.get_block(p).is_some_and(|b| super::decorated_pot_of(b).is_some()) {
            return false;
        }
        match self.utilities.decorated.get_mut(&p) {
            None => {
                self.utilities.decorated.insert(p, one);
                true
            }
            Some(s) if s.stacks_with(&one) && s.count < s.item.max_stack() => {
                s.count += 1;
                true
            }
            Some(_) => false,
        }
    }

    /// Puts `contents` in the chest table at `p` for a screen (an ender
    /// chest showing its player's inventory).
    pub fn lend_chest(&mut self, p: IVec3, contents: crate::world::chest::Chest) {
        self.chests.insert(p, contents);
        self.utilities.lent = Some(p);
    }

    /// Takes back what [`World::lend_chest`] put at `p`, even if the ender
    /// chest has since been mined; `None` for any other chest.
    pub fn take_lent_chest(&mut self, p: IVec3) -> Option<crate::world::chest::Chest> {
        if self.utilities.lent != Some(p) {
            return None;
        }
        self.utilities.lent = None;
        let lent = self.chests.remove(&p);
        // A real chest placed there meanwhile keeps (empty) storage of its own.
        if self.get_block(p).is_some_and(crate::world::chest::is_chest) {
            self.chests.entry(p).or_default();
        }
        lent
    }

    /// The cell whose chest table entry is a lent ender inventory.
    pub fn lent_chest(&self) -> Option<IVec3> {
        self.utilities.lent
    }

    /// A player opened the chest at `p`; trapped chests start signalling.
    pub fn chest_opened(&mut self, p: IVec3) {
        if self.get_block(p).is_some_and(|b| trapped_chest_of(b).is_some()) {
            let n = self.utilities.viewers.entry(p).or_default();
            *n = n.saturating_add(1);
            self.redstone_changed(p);
        }
    }

    /// A player closed the chest at `p`.
    pub fn chest_closed(&mut self, p: IVec3) {
        if let Some(n) = self.utilities.viewers.get_mut(&p) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.utilities.viewers.remove(&p);
            }
            self.redstone_changed(p);
        }
    }

    /// How many players look into the chest at `p`.
    pub fn chest_viewers(&self, p: IVec3) -> u8 {
        self.utilities.viewers.get(&p).copied().unwrap_or(0)
    }

    /// `TrappedChestBlock.getSignal` and `getDirectSignal`: the viewer
    /// count (at most 15) weakly to every side and strongly into the block
    /// below. `None` for other blocks.
    pub(crate) fn trapped_signal(&self, p: IVec3, b: Block, direction: IVec3, strong: bool) -> Option<u8> {
        trapped_chest_of(b)?;
        let power = self.chest_viewers(p).min(15);
        Some(if !strong || direction == IVec3::NEG_Y { power } else { 0 })
    }
}
