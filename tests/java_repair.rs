//! Anvil repair materials against Java 26.3's `repairable` component: each
//! registered repairable item takes exactly its Java materials, and every
//! other item takes none. See `repair.json`.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use voxelcraft::{
    enchant,
    item::Item,
    world::block::{Block, STATE_CAPACITY},
};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The registered item for a Java ID: by ID or English name.
fn registered(names: &Value, id: &str) -> Option<Item> {
    let id = id.strip_prefix("minecraft:").unwrap_or(id);
    let english = names["items"][id].as_str();
    Item::from_name(id).or_else(|| english.and_then(|e| Item::from_name(&e.to_lowercase())))
}

/// Every item a player can hold, block items included.
fn all_items() -> Vec<Item> {
    let blocks = (0..STATE_CAPACITY as u16).map(Block).filter(|b| b.base() == *b && !b.name().is_empty());
    let mut items: Vec<Item> = blocks.filter_map(|b| Item::from_name(b.name())).chain(Item::all_items()).collect();
    items.sort_by_key(|i| i.0);
    items.dedup();
    items
}

#[test]
fn every_repairable_item_takes_exactly_its_java_26_3_materials() {
    let f = fixture("repair.json");
    let names = fixture("registry_names.json");
    let items = all_items();
    let mut repairable = BTreeMap::new();
    let mut unregistered = Vec::new();
    let mut failures = Vec::new();
    for case in f["repairs"].as_array().unwrap() {
        let id = case["item"].as_str().unwrap();
        let materials: Vec<&str> = match (case.get("tag"), case.get("material")) {
            (Some(tag), None) => {
                f["tags"][tag.as_str().unwrap()].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect()
            }
            (None, Some(material)) => vec![material.as_str().unwrap()],
            _ => panic!("{id}: unknown repair form"),
        };
        let Some(item) = registered(&names, id) else {
            unregistered.push(id);
            continue;
        };
        let want: BTreeSet<u16> = materials.iter().filter_map(|m| registered(&names, m)).map(|m| m.0).collect();
        repairable.insert(item.0, id);
        for &material in &items {
            if enchant::repairs(item, material) != want.contains(&material.0) {
                failures.push(format!("{id} with {}: {}", material.name(), enchant::repairs(item, material)));
            }
        }
    }
    for &item in items.iter().filter(|i| !repairable.contains_key(&i.0)) {
        for &material in &items {
            if enchant::repairs(item, material) {
                failures.push(format!("{} isn't repairable in Java but takes {}", item.name(), material.name()));
            }
        }
    }
    println!(
        "compared {} registered repairable items; {} unregistered: {unregistered:?}",
        repairable.len(),
        unregistered.len()
    );
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}
