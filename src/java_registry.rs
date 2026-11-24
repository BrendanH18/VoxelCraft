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
        "sulfur_cube_bucket" => "bucket of sulfur cube",
        "turtle_scute" => "turtle scute",
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

/// VoxelCraft state names that are a state of a differently named Java
/// block (lit and wet forms, upper halves, flowing fluids), beyond
/// [`block_alias`]. Bed halves are matched by colour in [`state_java_id`].
pub const STATE_ALIASES: &[(&str, &str)] = &[
    ("snowy grass", "grass_block"),
    ("tall dry grass top", "tall_dry_grass"),
    ("lit redstone ore", "redstone_ore"),
    ("lit deepslate redstone ore", "deepslate_redstone_ore"),
    ("lit furnace", "furnace"),
    ("lit smoker", "smoker"),
    ("lit blast furnace", "blast_furnace"),
    ("wet farmland", "farmland"),
    ("snow layer", "snow"),
    ("snow", "snow_block"),
    ("wool", "white_wool"),
    ("vines", "vine"),
    ("hay bale", "hay_block"),
    ("grass", "grass_block"),
    ("tall grass", "short_grass"),
    ("tall grass plant", "tall_grass"),
    ("tall grass plant top", "tall_grass"),
    ("sunflower top", "sunflower"),
    ("lilac top", "lilac"),
    ("rose bush top", "rose_bush"),
    ("peony top", "peony"),
    ("large fern top", "large_fern"),
    ("small dripleaf top", "small_dripleaf"),
    ("tall seagrass top", "tall_seagrass"),
    ("pale hanging moss plant", "pale_hanging_moss"),
    ("hanging mangrove propagule", "mangrove_propagule"),
    ("carrot crops", "carrots"),
    ("potato crops", "potatoes"),
    ("wheat crops", "wheat"),
    ("chain", "iron_chain"),
    ("legacy bone block", "bone_block"),
    ("flowing water", "water"),
    ("falling water", "water"),
    ("flowing lava", "lava"),
    ("falling lava", "lava"),
    ("bed foot", "red_bed"),
    ("bed head", "red_bed"),
];

/// VoxelCraft state name -> Java block ID, from Java's (ID, English name)
/// pairs. Cold path: callers build a table once.
pub fn state_java_ids<'a>(
    ids: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> std::collections::HashMap<String, String> {
    let ids: Vec<(&str, &str)> = ids.into_iter().collect();
    let mut by_name = std::collections::HashMap::new();
    for &(id, _) in &ids {
        by_name.insert(id.replace('_', " "), id.to_string());
        // Beds register a named foot and head per colour.
        if let Some(colour) = id.strip_suffix("_bed") {
            for half in ["foot", "head"] {
                by_name.insert(format!("{} bed {half}", colour.replace('_', " ")), id.to_string());
            }
        }
    }
    for &(id, english) in &ids {
        // Wall torches are states of VoxelCraft's torches: the torch keeps the name.
        if let Some(alias) = block_alias(id).filter(|_| !id.ends_with("wall_torch")) {
            by_name.insert(alias.to_string(), id.to_string());
        }
        by_name.entry(english.to_lowercase()).or_insert_with(|| id.to_string());
    }
    for (alias, id) in STATE_ALIASES {
        by_name.insert(alias.to_string(), id.to_string());
    }
    by_name
}
