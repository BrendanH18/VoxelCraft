//! Java 26.3 fishing loot (`data/minecraft/loot_table/gameplay/fishing*.json`)
//! as typed tables. Entries keep the reference order. Item IDs are unchanged.
//!
//! `tests/fishing_loot.rs` checks every row against the extracted reference
//! fixture `tests/fixtures/java/26.3/fishing.json`.
use crate::item::{ArmorMaterial, ArmorPiece, Item};
use crate::potion::Potion;
use crate::world::{gadgets, overworld_blocks as ob};

/// An entry's `condition`, as used by the fishing tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Always,
    /// `entity_properties` on the hook: `in_open_water`.
    InOpenWater,
    /// `location_check` biomes: jungle, sparse jungle or bamboo jungle.
    JungleBiome,
}

/// The nested table a top-level `gameplay/fishing` entry rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Junk,
    Treasure,
    Fish,
}

/// One weighted pool entry. Java's effective weight is
/// `max(floor(weight + quality * luck), 0)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry<T> {
    pub value: T,
    pub weight: i32,
    pub quality: i32,
    pub condition: Condition,
}

/// An item entry with its loot functions, applied in this order:
/// `set_count`, `set_damage` (uniform remaining-durability fraction) and then
/// `enchant_with_levels` over `#minecraft:on_random_loot`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loot {
    pub item: Item,
    pub count: u8,
    /// `set_damage` with `uniform { min, max }`.
    pub damage: Option<(f32, f32)>,
    /// `enchant_with_levels` levels.
    pub enchant_levels: Option<u32>,
}

impl<T> Entry<T> {
    /// Java's `LootPoolSingletonContainer.getWeight`.
    pub fn weight(&self, luck: i32) -> i32 {
        (self.weight + self.quality * luck).max(0)
    }
}

const fn plain(item: Item, weight: i32) -> Entry<Loot> {
    Entry {
        value: Loot { item, count: 1, damage: None, enchant_levels: None },
        weight,
        quality: 0,
        condition: Condition::Always,
    }
}

const fn damaged(item: Item, weight: i32, max: f32) -> Entry<Loot> {
    Entry { value: Loot { damage: Some((0.0, max)), ..plain(item, weight).value }, ..plain(item, weight) }
}

const fn treasure_gear(item: Item) -> Entry<Loot> {
    let entry = damaged(item, 1, 0.25);
    Entry { value: Loot { enchant_levels: Some(30), ..entry.value }, ..entry }
}

/// `gameplay/fishing`: junk, treasure (open water only) and fish.
pub const CATEGORIES: [Entry<Category>; 3] = [
    Entry { value: Category::Junk, weight: 10, quality: -2, condition: Condition::Always },
    Entry { value: Category::Treasure, weight: 5, quality: 2, condition: Condition::InOpenWater },
    Entry { value: Category::Fish, weight: 85, quality: -1, condition: Condition::Always },
];

/// `gameplay/fishing/fish`.
pub const FISH: [Entry<Loot>; 4] =
    [plain(Item::COD, 60), plain(Item::SALMON, 25), plain(Item::TROPICAL_FISH, 2), plain(Item::PUFFERFISH, 13)];

/// `gameplay/fishing/junk`.
pub const JUNK: [Entry<Loot>; 13] = [
    plain(Item::from_block(ob::LILY_PAD), 17),
    damaged(Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather), 10, 0.9),
    plain(Item::LEATHER, 10),
    plain(Item::BONE, 10),
    plain(Item::potion(Potion::WATER), 10),
    plain(Item::STRING, 5),
    damaged(Item::FISHING_ROD, 2, 0.9),
    plain(Item::BOWL, 10),
    plain(Item::STICK, 5),
    Entry { value: Loot { count: 10, ..plain(Item::INK_SAC, 1).value }, ..plain(Item::INK_SAC, 1) },
    plain(Item::from_block(gadgets::HOOK), 10),
    plain(Item::ROTTEN_FLESH, 10),
    Entry { condition: Condition::JungleBiome, ..plain(Item::from_block(ob::BAMBOO), 10) },
];

/// `gameplay/fishing/treasure`. The book becomes an enchanted book.
pub const TREASURE: [Entry<Loot>; 6] = [
    plain(Item::NAME_TAG, 1),
    plain(Item::SADDLE, 1),
    treasure_gear(Item::BOW),
    treasure_gear(Item::FISHING_ROD),
    Entry { value: Loot { enchant_levels: Some(30), ..plain(Item::BOOK, 1).value }, ..plain(Item::BOOK, 1) },
    plain(Item::NAUTILUS_SHELL, 1),
];

impl Category {
    pub fn table(self) -> &'static [Entry<Loot>] {
        match self {
            Category::Junk => &JUNK,
            Category::Treasure => &TREASURE,
            Category::Fish => &FISH,
        }
    }
}
