//! Consumption durations in fixed gameplay ticks (Java 26.3 Consumables).
use crate::item::Item;

pub fn ticks(item: Item) -> Option<u32> {
    if !item.is_drink() && item.food().is_none() {
        return None;
    }
    Some(match item {
        Item::DRIED_KELP => 16,
        Item::HONEY_BOTTLE => 40,
        _ => 32,
    })
}
