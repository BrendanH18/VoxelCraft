//! Cold-path registry audit: Java 26.3 block and item names VoxelCraft doesn't register,
//! and registered names Java doesn't have. Name matching is a probe, not proof of parity:
//! a name can resolve while its states or behaviour are incomplete.
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use voxelcraft::{
    item::Item,
    world::block::{Block, STATE_CAPACITY},
};

/// Blocks resolve only as blocks, so a same-named item (such as the wither skeleton skull
/// material) doesn't hide a missing placeable block; items resolve as either.
fn resolves(id: &str, name: &str, find: fn(&str) -> bool) -> bool {
    let name = name.to_lowercase();
    find(id) || (!name.is_empty() && find(&name))
}

fn missing(names: &Map<String, Value>, find: fn(&str) -> bool) -> Vec<&str> {
    names.iter().filter(|(id, name)| !resolves(id, name.as_str().unwrap(), find)).map(|(id, _)| id.as_str()).collect()
}

fn main() {
    let reference: Value =
        serde_json::from_str(include_str!("../tests/fixtures/java/26.3/registry_names.json")).unwrap();
    let blocks = reference["blocks"].as_object().unwrap();
    let items = reference["items"].as_object().unwrap();
    let mut known = BTreeSet::new();
    for (id, name) in blocks.iter().chain(items) {
        known.insert(id.replace('_', " "));
        known.insert(name.as_str().unwrap().to_lowercase());
    }
    let block_names: BTreeSet<&str> =
        (0..STATE_CAPACITY as u16).map(|i| Block(i).name()).filter(|n| !n.is_empty()).collect();
    let extra_blocks: Vec<&str> = block_names.iter().copied().filter(|n| !known.contains(*n)).collect();
    let extra_items: BTreeSet<&str> =
        Item::all_items().map(|i| i.name()).filter(|n| !known.contains(*n) && !block_names.contains(n)).collect();
    // Beds register each half under its own name; the bed item places them.
    let missing_blocks = missing(blocks, |n| {
        Block::from_name(n).is_some() || Item::from_name(n).is_some_and(|i| i.bed_color().is_some())
    });
    let missing_items = missing(items, |n| Block::from_name(n).is_some() || Item::from_name(n).is_some());
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "reference": reference["reference"], "client_sha1": reference["client_sha1"],
            "reference_blocks": blocks.len(), "reference_items": items.len(),
            "missing_block_count": missing_blocks.len(), "missing_item_count": missing_items.len(),
            "missing_blocks": missing_blocks, "missing_items": missing_items,
            "unmatched_block_names": extra_blocks, "unmatched_item_names": extra_items,
            "scope": "Name-level probe. Unmatched names are internal states, renamed entries or blocks Java lacks; review each by hand.",
        }))
        .unwrap()
    );
}
