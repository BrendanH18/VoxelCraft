//! Every Java 26.3 compostable that VoxelCraft registers has Java's chance,
//! and nothing else composts. See `compost.json`.
use glam::IVec3;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use voxelcraft::{
    item::Item,
    world::{
        World,
        block::{Block, STATE_CAPACITY},
        chunk::ChunkData,
        composter,
        terrain::{Dimension, Generator},
    },
};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Java items VoxelCraft names after a different block or state.
const ALIASES: &[(&str, &str)] =
    &[("hay_block", "hay bale"), ("short_grass", "tall grass"), ("tall_grass", "tall grass plant"), ("vine", "vines")];

/// The registered item for a Java ID: by ID, alias or English name.
fn registered(names: &Value, id: &str) -> Option<Item> {
    let id = id.strip_prefix("minecraft:").unwrap_or(id);
    if let Some((_, alias)) = ALIASES.iter().find(|(java, _)| *java == id) {
        return Item::from_name(alias);
    }
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
fn every_registered_compostable_has_java_26_3_chance_and_nothing_else_composts() {
    let f = fixture("compost.json");
    let names = fixture("registry_names.json");
    assert_eq!(f["empty_composter_always_adds_a_layer"], true);
    let mut expected = BTreeMap::new();
    let mut unregistered = Vec::new();
    let mut failures = Vec::new();
    for case in f["compost"].as_array().unwrap() {
        let id = case["item"].as_str().unwrap();
        let percent = case["chance_percent"].as_u64().unwrap() as f32;
        let Some(item) = registered(&names, id) else {
            unregistered.push(id);
            continue;
        };
        expected.insert(item.0, id);
        if composter::chance(item) != percent / 100.0 {
            failures.push(format!("{id}: {} (Java {percent}%)", composter::chance(item)));
        }
    }
    let extra: BTreeSet<&str> = all_items()
        .into_iter()
        .filter(|i| composter::chance(*i) > 0.0 && !expected.contains_key(&i.0))
        .map(|i| i.name())
        .collect();
    println!(
        "compared {} registered compostables; {} unregistered: {unregistered:?}",
        expected.len(),
        unregistered.len()
    );
    assert!(extra.is_empty(), "these compost but aren't Java 26.3 compostables: {extra:?}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Java's providers add the first layer to an empty composter every time.
#[test]
fn an_empty_composter_takes_the_first_layer_of_every_compostable() {
    let names = fixture("registry_names.json");
    let mut w = World::new_remote(Arc::new(Generator::for_dimension(7, Dimension::Overworld)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let p = IVec3::new(3, 130, 3);
    for case in fixture("compost.json")["compost"].as_array().unwrap() {
        let Some(item) = registered(&names, case["item"].as_str().unwrap()) else { continue };
        w.set_block(p, Block::COMPOSTER);
        assert!(w.compost(p, item), "{}", case["item"]);
        assert_eq!(w.get_block(p), Some(composter::state(1)), "{}", case["item"]);
    }
}
