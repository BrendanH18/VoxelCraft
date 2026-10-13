//! Shared input priority metadata, migrated from the existing client policy.
use crate::world::{block::Block, chest, furnace};
pub fn blocks_item_use(b: Block) -> bool {
    b == Block::CRAFTING_TABLE
        || b.is_bed()
        || b.is_door()
        || b.is_gate()
        || furnace::is_furnace(b)
        || chest::is_chest(b)
        || b == Block::BREWING_STAND
        || b == Block::ENCHANTING_TABLE
        || b.is_anvil()
        || b == Block::SMITHING_TABLE
}
