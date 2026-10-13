//! Java 26.3 fishing/fish loot weights; item IDs are unchanged.
use crate::item::Item;
pub const FISH: [(Item, i32); 4] =
    [(Item::COD, 60), (Item::SALMON, 25), (Item::TROPICAL_FISH, 2), (Item::PUFFERFISH, 13)];
