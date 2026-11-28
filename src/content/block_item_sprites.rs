//! v1.0 gap track 6b: block items Java 26.3 draws as flat sprites of their
//! own (`item/generated` models with an `item/...` layer, or for the lever,
//! redstone torch and tripwire hook a `block/...` sprite), which the game
//! drew in 3D from their world shape. Each gets an original 16 × 16 item
//! sprite (`render::block_item_sprites`) in the item icon array, after the
//! open bundles, and is drawn flat in a slot and extruded when held,
//! dropped or framed, as Java draws them.
//!
//! The oak door is the one item here that isn't a block item: it places
//! its halves, and drew its own squeezed door before.
//!
//! Waxed copper items share their unwaxed sprite, as Java's item models do;
//! each item still has its own layer, so a layer names one entry here.

use std::sync::OnceLock;

use rustc_hash::FxHashMap;

use crate::item::Item;
use crate::world::block::Block;

/// What a sprite shows. Doors, lanterns and chains take their colours (and
/// doors their whole face) from the item's own block textures, so the icon
/// matches the placed block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    /// A door: its two halves' faces, side by side in one sprite.
    Door,
    /// A lantern: its cage and flame, with a loop handle.
    Lantern,
    /// A chain: links hanging on a diagonal.
    Chain,
    Cauldron,
    Hopper,
    BrewingStand,
    Comparator,
    Repeater,
    Campfire {
        soul: bool,
    },
    FlowerPot,
    Bell,
    SnifferEgg,
    TurtleEgg,
    PinkPetals,
    Wildflowers,
    LeafLitter,
    Bamboo,
    Lever,
    RedstoneTorch,
    TripwireHook,
}

/// Java ids (with English names for the resolver) of every item drawn with
/// one of these sprites, in icon order. Append only: the order is the icon
/// layer order.
pub const ITEMS: &[(&str, &str, Art)] = &[
    ("oak_door", "Oak Door", Art::Door),
    ("spruce_door", "Spruce Door", Art::Door),
    ("birch_door", "Birch Door", Art::Door),
    ("jungle_door", "Jungle Door", Art::Door),
    ("acacia_door", "Acacia Door", Art::Door),
    ("cherry_door", "Cherry Door", Art::Door),
    ("dark_oak_door", "Dark Oak Door", Art::Door),
    ("pale_oak_door", "Pale Oak Door", Art::Door),
    ("mangrove_door", "Mangrove Door", Art::Door),
    ("bamboo_door", "Bamboo Door", Art::Door),
    ("crimson_door", "Crimson Door", Art::Door),
    ("warped_door", "Warped Door", Art::Door),
    ("poplar_door", "Poplar Door", Art::Door),
    ("iron_door", "Iron Door", Art::Door),
    ("copper_door", "Copper Door", Art::Door),
    ("exposed_copper_door", "Exposed Copper Door", Art::Door),
    ("weathered_copper_door", "Weathered Copper Door", Art::Door),
    ("oxidized_copper_door", "Oxidized Copper Door", Art::Door),
    ("waxed_copper_door", "Waxed Copper Door", Art::Door),
    ("waxed_exposed_copper_door", "Waxed Exposed Copper Door", Art::Door),
    ("waxed_weathered_copper_door", "Waxed Weathered Copper Door", Art::Door),
    ("waxed_oxidized_copper_door", "Waxed Oxidized Copper Door", Art::Door),
    ("lantern", "Lantern", Art::Lantern),
    ("soul_lantern", "Soul Lantern", Art::Lantern),
    ("copper_lantern", "Copper Lantern", Art::Lantern),
    ("exposed_copper_lantern", "Exposed Copper Lantern", Art::Lantern),
    ("weathered_copper_lantern", "Weathered Copper Lantern", Art::Lantern),
    ("oxidized_copper_lantern", "Oxidized Copper Lantern", Art::Lantern),
    ("waxed_copper_lantern", "Waxed Copper Lantern", Art::Lantern),
    ("waxed_exposed_copper_lantern", "Waxed Exposed Copper Lantern", Art::Lantern),
    ("waxed_weathered_copper_lantern", "Waxed Weathered Copper Lantern", Art::Lantern),
    ("waxed_oxidized_copper_lantern", "Waxed Oxidized Copper Lantern", Art::Lantern),
    ("iron_chain", "Iron Chain", Art::Chain),
    ("copper_chain", "Copper Chain", Art::Chain),
    ("exposed_copper_chain", "Exposed Copper Chain", Art::Chain),
    ("weathered_copper_chain", "Weathered Copper Chain", Art::Chain),
    ("oxidized_copper_chain", "Oxidized Copper Chain", Art::Chain),
    ("waxed_copper_chain", "Waxed Copper Chain", Art::Chain),
    ("waxed_exposed_copper_chain", "Waxed Exposed Copper Chain", Art::Chain),
    ("waxed_weathered_copper_chain", "Waxed Weathered Copper Chain", Art::Chain),
    ("waxed_oxidized_copper_chain", "Waxed Oxidized Copper Chain", Art::Chain),
    ("cauldron", "Cauldron", Art::Cauldron),
    ("hopper", "Hopper", Art::Hopper),
    ("brewing_stand", "Brewing Stand", Art::BrewingStand),
    ("comparator", "Redstone Comparator", Art::Comparator),
    ("repeater", "Redstone Repeater", Art::Repeater),
    ("campfire", "Campfire", Art::Campfire { soul: false }),
    ("soul_campfire", "Soul Campfire", Art::Campfire { soul: true }),
    ("flower_pot", "Flower Pot", Art::FlowerPot),
    ("bell", "Bell", Art::Bell),
    ("sniffer_egg", "Sniffer Egg", Art::SnifferEgg),
    ("turtle_egg", "Turtle Egg", Art::TurtleEgg),
    ("pink_petals", "Pink Petals", Art::PinkPetals),
    ("wildflowers", "Wildflowers", Art::Wildflowers),
    ("leaf_litter", "Leaf Litter", Art::LeafLitter),
    ("bamboo", "Bamboo", Art::Bamboo),
    ("lever", "Lever", Art::Lever),
    ("redstone_torch", "Redstone Torch", Art::RedstoneTorch),
    ("tripwire_hook", "Tripwire Hook", Art::TripwireHook),
];

/// How many icon layers these sprites take.
pub const ICONS: u32 = ITEMS.len() as u32;

struct Table {
    by_item: FxHashMap<Item, u16>,
    items: Vec<Option<Item>>,
    blocks: FxHashMap<Block, u16>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let items: Vec<Option<Item>> =
            ITEMS.iter().map(|&(id, english, _)| crate::java_registry::item(id, english)).collect();
        let mut by_item = FxHashMap::default();
        let mut blocks = FxHashMap::default();
        for (i, item) in items.iter().enumerate() {
            let Some(item) = *item else { continue };
            by_item.insert(item, i as u16);
            if let Some(b) = item.block() {
                blocks.insert(b, i as u16);
            }
        }
        Table { by_item, items, blocks }
    })
}

/// The entry of `item` in [`ITEMS`], if it is drawn with one of these sprites.
pub fn index(item: Item) -> Option<u16> {
    table().by_item.get(&item).copied()
}

/// Whether `block` is the block of one of [`ITEMS`]' items (so its item is
/// drawn flat).
pub fn is_item_block(block: Block) -> bool {
    table().blocks.contains_key(&block)
}

/// The item of entry `i`, as the game resolves its Java id.
pub fn item(i: u16) -> Option<Item> {
    table().items.get(i as usize).copied().flatten()
}

/// The art of entry `i`.
pub fn art(i: u16) -> Option<Art> {
    ITEMS.get(i as usize).map(|&(_, _, art)| art)
}

/// The Java id of entry `i`.
pub fn java_id(i: u16) -> Option<&'static str> {
    ITEMS.get(i as usize).map(|&(id, _, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_resolves_to_its_own_block_item() {
        let mut seen = std::collections::HashSet::new();
        for (i, &(id, _, _)) in ITEMS.iter().enumerate() {
            let it = item(i as u16).unwrap_or_else(|| panic!("{id} doesn't resolve"));
            assert!(seen.insert(it), "{id} resolves to an item listed twice");
            assert_eq!(index(it), Some(i as u16), "{id}");
            // The oak door is the one item here that isn't a block item
            // (it places its two halves).
            match it.block() {
                Some(b) => assert!(is_item_block(b), "{id}"),
                None => assert_eq!(it, Item::OAK_DOOR, "{id} is a block item"),
            }
        }
    }
}
