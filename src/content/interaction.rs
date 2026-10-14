//! Shared input priority metadata, migrated from the existing client policy.
use glam::IVec3;

use crate::{
    item::Item,
    world::{World, block::Block, chest, composter, furnace},
};
pub fn blocks_item_use(b: Block) -> bool {
    b == Block::CRAFTING_TABLE
        || b.is_bed()
        || (b.is_door() && b.base() != crate::world::redstone_blocks::IRON_DOOR)
        || b.is_gate()
        || furnace::is_furnace(b)
        || chest::is_chest(b)
        || b == Block::BREWING_STAND
        || b == Block::ENCHANTING_TABLE
        || b.is_anvil()
        || b == Block::SMITHING_TABLE
        || b.base() == Block::GRINDSTONE
}

/// Java 26.3 `ServerPlayerGameMode.useItemOn` runs the targeted block's
/// `useItemOn`/`useWithoutItem` before `Item.use`, skipping it only when the
/// player sneaks while holding an item. True when VoxelCraft's block use at
/// `pos` acts for `held`, so every input path leaves the held item unused.
pub fn block_use_precedes_item(world: &World, pos: IVec3, held: Option<Item>, sneaking: bool) -> bool {
    if sneaking && held.is_some() {
        return false;
    }
    let Some(b) = world.get_block(pos) else { return false };
    blocks_item_use(b)
        || b == Block::DRAGON_EGG
        || world.redstone_usable(pos)
        || composter::level(b).is_some_and(|level| level == 8 || held.is_some_and(|item| composter::chance(item) > 0.0))
}
