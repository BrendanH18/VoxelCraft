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

/// Shelves and chiseled bookshelves act only for a click on their front
/// (`getHitSlot`); any other face returns `PASS` and the held item is used.
fn front_face_use(world: &World, pos: IVec3, b: Block, aim: (glam::DVec3, glam::DVec3)) -> bool {
    use crate::world::wood::{bookshelf, shelves};
    let Some(facing) = shelves::shelf(b).map(|s| s.facing).or_else(|| bookshelf::shelf(b).map(|(f, _)| f)) else {
        return false;
    };
    crate::physics::ray_shape(world, pos, aim.0, aim.1).is_some_and(|(_, normal)| normal == facing.offset())
}

/// Java 26.3 `ServerPlayerGameMode.useItemOn` runs the targeted block's
/// `useItemOn`/`useWithoutItem` before `Item.use`, skipping it only when the
/// player sneaks while holding an item. True when VoxelCraft's block use at
/// `pos` acts for `held`, so every input path leaves the held item unused.
pub fn block_use_precedes_item(
    world: &World,
    pos: IVec3,
    held: Option<Item>,
    sneaking: bool,
    aim: (glam::DVec3, glam::DVec3),
) -> bool {
    if sneaking && held.is_some() {
        return false;
    }
    let Some(b) = world.get_block(pos) else { return false };
    blocks_item_use(b)
        || crate::world::utilities::interact::takes_use(world, pos, held)
        || b == Block::DRAGON_EGG
        || crate::world::wood::signs::sign(b).is_some()
        || front_face_use(world, pos, b, aim)
        || (world.redstone_usable(pos) && (b.base() != Block::BELL || world.bell_hit_rings(pos, aim.0, aim.1)))
        || composter::level(b).is_some_and(|level| level == 8 || held.is_some_and(|item| composter::chance(item) > 0.0))
}
