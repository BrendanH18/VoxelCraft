//! Cold-path Java registry-name resolution shared by inventory audits and tests.
//! Aliases establish identity, never behavior or block-state parity.
use crate::{item::Item, world::block::Block};

/// Explicit Java block identities whose registered VoxelCraft names differ.
pub fn block_alias(id: &str) -> Option<&'static str> {
    Some(match id {
        "grass_block" => "grass",
        "short_grass" => "tall grass",
        "tall_grass" => "tall grass plant",
        "snow_block" => "snow",
        "snow" => "snow layer",
        "wheat" => "wheat crops",
        "carrots" => "carrot crops",
        "potatoes" => "potato crops",
        "wall_torch" => "torch",
        "soul_wall_torch" => "soul torch",
        "redstone_wall_torch" => "redstone torch",
        "iron_chain" => "chain",
        _ => return None,
    })
}

pub fn block(id: &str, english: &str) -> Option<Block> {
    let id = id.trim_start_matches("minecraft:");
    Block::from_name(block_alias(id).unwrap_or(id)).or_else(|| Block::from_name(&english.to_lowercase()))
}

pub fn item(id: &str, english: &str) -> Option<Item> {
    let id = id.trim_start_matches("minecraft:");
    let alias = match id {
        "potion" => return Some(Item::potion(crate::potion::Potion::WATER)),
        "splash_potion" => return Some(Item::splash_potion(crate::potion::Potion::WATER)),
        "goat_horn" => return Some(Item::GOAT_HORN),
        "buried_trial_chambers_map" => return Some(crate::world::trial::TRIAL_CHAMBERS_MAP),
        "cooked_beef" => "steak",
        "quartz" => "nether quartz",
        "cod_bucket" => "bucket of cod",
        "salmon_bucket" => "bucket of salmon",
        "tropical_fish_bucket" => "bucket of tropical fish",
        "pufferfish_bucket" => "bucket of pufferfish",
        "turtle_scute" => "turtle scute",
        "experience_bottle" => "bottle o' enchanting",
        // The one alias whose Java block and item differ: the crop and its harvest.
        "wheat" => "wheat",
        _ => block_alias(id).unwrap_or(id),
    };
    Item::from_name(alias).or_else(|| Item::from_name(&english.to_lowercase()))
}

pub fn resolves_block(id: &str, english: &str) -> bool {
    // Beds register named halves, while their item identifies the Java block.
    block(id, english).is_some() || item(id, english).is_some_and(|i| i.bed_color().is_some())
}

pub fn resolves_item(id: &str, english: &str) -> bool {
    item(id, english).is_some()
}
