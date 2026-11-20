//! Shared content tags and integration metadata, independent of client devices.
//! Start with planks and furnace fuels; other integrations migrate with fixtures.
use crate::{item::Item, world::block::Block};

pub mod banner_patterns;
pub mod books;
pub mod bundle;
pub mod chorus;
pub mod compost;
pub mod consumption;
pub mod farm_variant;
pub mod fishing;
pub mod fuel;
pub mod harness;
pub mod interaction;
pub mod mining;
pub mod placement;
pub mod spyglass;
pub mod trim;
// v0.16 Track B: flight and storage.
pub mod elytra;
pub mod fireworks;
pub mod shulker_box;

/// All implemented plank ingredients. Oak stays first for recipe-book previews.
/// Nether planks are valid ingredients but explicitly are not furnace fuels.
pub const PLANKS: [Item; 12] = [
    Item::from_block(Block::PLANKS),
    Item::from_block(Block::SPRUCE_PLANKS),
    Item::from_block(Block::BIRCH_PLANKS),
    Item::from_block(Block::JUNGLE_PLANKS),
    Item::from_block(Block::ACACIA_PLANKS),
    Item::from_block(Block::DARK_OAK_PLANKS),
    Item::from_block(Block::MANGROVE_PLANKS),
    Item::from_block(Block::CHERRY_PLANKS),
    Item::from_block(crate::world::nether_biome_blocks::CRIMSON_PLANKS),
    Item::from_block(crate::world::nether_biome_blocks::WARPED_PLANKS),
    Item::from_block(crate::world::overworld_blocks::PALE_OAK_PLANKS),
    Item::from_block(crate::world::overworld_blocks::BAMBOO_PLANKS),
];

pub fn burnable_planks(item: Item) -> bool {
    PLANKS.contains(&item)
        && !matches!(
            item.block(),
            Some(crate::world::nether_biome_blocks::CRIMSON_PLANKS | crate::world::nether_biome_blocks::WARPED_PLANKS)
        )
}
