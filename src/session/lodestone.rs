//! Lodestone compasses: Java 26.3's `CompassItem.useOn`, `inventoryTick`
//! and `LodestoneTracker.tick`.

use glam::IVec3;

use super::PlayerSession;
use crate::entity::Entities;
use crate::inventory::{Lodestone, Stack};
use crate::item::Item;
use crate::world::World;

impl PlayerSession {
    /// `CompassItem.useOn`: a compass used on a lodestone points to it. A
    /// lone compass outside creative becomes the lodestone compass; otherwise
    /// one copy (keeping its name and enchantments) is split off into the
    /// inventory, or dropped when it doesn't fit. A lodestone compass can be
    /// locked again. `false` when this isn't a compass on a lodestone.
    pub fn lock_compass(&mut self, world: &World, entities: &mut Entities, slot: usize, pos: IVec3) -> bool {
        let Some(stack) = self.inventory.get(slot) else { return false };
        if stack.item != Item::COMPASS || world.get_block(pos) != Some(crate::world::utilities::LODESTONE) {
            return false;
        }
        let tracker = Some(Lodestone { target: Some((pos, world.generator.dimension)) });
        let creative = self.mode.is_creative();
        if !creative && stack.count == 1 {
            self.inventory.slots[slot] = Some(Stack { lodestone: tracker, ..stack });
            return true;
        }
        let compass = Stack { count: 1, lodestone: tracker, ..stack };
        if !creative {
            self.inventory.take_one(slot);
        }
        if self.inventory.add_stack(compass) > 0 {
            entities.throw(compass, self.player.eye(), self.player.forward().as_dvec3());
        }
        true
    }

    /// `LodestoneTracker.tick` for each lodestone compass carried: one whose
    /// lodestone in this dimension is gone loses its target and spins.
    /// Lodestones in unloaded chunks are kept, since Java's check reads the
    /// saved points of interest and nothing here can remove one unloaded.
    pub fn tick_lodestones(&mut self, world: &World) {
        let dimension = world.generator.dimension;
        let inventory = &mut self.inventory;
        for stack in inventory.slots.iter_mut().chain(inventory.armor.iter_mut()).flatten() {
            if let Some(Lodestone { target: Some((pos, d)) }) = stack.lodestone
                && d == dimension
                && world.is_loaded(pos)
                && world.get_block(pos) != Some(crate::world::utilities::LODESTONE)
            {
                stack.lodestone = Some(Lodestone { target: None });
            }
        }
    }
}
