//! Missing reference names must have an owner and disappear from the ledger as they land.
use serde_json::Value;
use std::collections::BTreeSet;
use voxelcraft::{item::Item, java_registry, potion::Potion};
const ALLOWED: &[&str] = &["v0.17", "v0.18", "v0.19", "v0.20", "deferred", "none"];
fn gaps() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_gaps.json")).unwrap()
}
#[test]
fn reference_registry_gaps_are_complete_current_and_assigned() {
    let reference: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let brewing: Value = serde_json::from_str(include_str!("fixtures/java/26.3/brewing.json")).unwrap();
    let g = gaps();
    for (kind, resolve) in
        [("blocks", java_registry::resolves_block as fn(&str, &str) -> bool), ("items", java_registry::resolves_item)]
    {
        let names = reference[kind].as_object().unwrap();
        let entries = g[kind].as_object().unwrap();
        for (id, english) in names {
            assert_eq!(
                !resolve(id, english.as_str().unwrap()),
                entries.contains_key(id),
                "{kind}/{id}: missing owner or stale gap"
            );
        }
        for id in entries.keys() {
            assert!(names.contains_key(id), "invented {kind} gap {id}");
        }
        println!("registry {kind}: {} reference names, {} owned gaps", names.len(), entries.len());
    }
    let potions: BTreeSet<_> = brewing["potions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    for id in &potions {
        assert_eq!(Potion::from_id(id).is_none(), g["potions"].get(id).is_some(), "potion {id}");
    }
    for kind in ["blocks", "items", "potions"] {
        for (id, entry) in g[kind].as_object().unwrap() {
            assert!(ALLOWED.contains(&entry["release"].as_str().unwrap()), "{kind}/{id}: bad release");
            assert!(!entry["reason"].as_str().unwrap().trim().is_empty(), "{kind}/{id}: no reason");
            if kind == "potions" {
                assert!(potions.contains(id.as_str()));
            }
        }
    }
}
fn owned_item(name: &str, g: &Value, reference: &Value) -> bool {
    let name = name.trim_start_matches("minecraft:");
    let alias = match name {
        "bottle o enchanting" => "experience_bottle",
        _ => name,
    };
    let id = alias.replace(' ', "_");
    g["items"].get(&id).is_some()
        || reference["items"].as_object().unwrap().iter().any(|(id, english)| {
            english.as_str().is_some_and(|s| s.to_lowercase() == alias) && g["items"].get(id).is_some()
        })
}
fn loot_names<'a>(v: &'a Value, names: &mut BTreeSet<&'a str>) {
    match v {
        Value::Object(o) => {
            if o.get("type").and_then(Value::as_str).is_some_and(|t| t.trim_start_matches("minecraft:") == "item")
                && let Some(n) = o.get("name").and_then(Value::as_str)
            {
                names.insert(n);
            }
            for child in o.values() {
                loot_names(child, names);
            }
        }
        Value::Array(a) => {
            for child in a {
                loot_names(child, names);
            }
        }
        _ => {}
    }
}
#[test]
fn runtime_table_names_resolve_or_have_explicit_registry_gaps() {
    let g = gaps();
    let r: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let trades: BTreeSet<_> = voxelcraft::entity::villager::trade_item_names().collect();
    let mut pending = 0;
    for name in &trades {
        if Item::from_name(name).is_none() {
            pending += 1;
            assert!(owned_item(name, &g, &r), "unowned trade name {name}");
        }
    }
    println!("trades: {} names, {pending} unresolved (all owned)", trades.len());
    let wandering: BTreeSet<_> = voxelcraft::entity::wandering_trader::trade_item_names().collect();
    pending = 0;
    for name in &wandering {
        if Item::from_name(name).is_none() {
            pending += 1;
            assert!(owned_item(name, &g, &r), "unowned wandering-trader name {name}");
        }
    }
    println!("wandering trader: {} names, {pending} unresolved (all owned)", wandering.len());
    let loot: Value = serde_json::from_str(include_str!("../src/world/loot_tables.json")).unwrap();
    let mut names = BTreeSet::new();
    loot_names(&loot["tables"], &mut names);
    pending = 0;
    for name in &names {
        if voxelcraft::world::loot::java_item(name.trim_start_matches("minecraft:")).is_none() {
            pending += 1;
            assert!(owned_item(name, &g, &r), "unowned loot name {name}");
        }
    }
    println!("structure loot: {} names, {pending} unresolved (all owned)", names.len());
}

#[test]
fn identity_aliases_do_not_conflate_short_and_tall_grass_or_skull_items_and_blocks() {
    use voxelcraft::world::block::Block;
    assert_eq!(java_registry::block("grass_block", ""), Some(Block::GRASS));
    assert_eq!(java_registry::block("short_grass", ""), Some(Block::TALL_GRASS));
    assert_ne!(java_registry::block("short_grass", ""), java_registry::block("tall_grass", ""));
    assert_eq!(java_registry::item("potion", ""), Some(Item::potion(Potion::WATER)));
    assert_eq!(java_registry::item("splash_potion", ""), Some(Item::splash_potion(Potion::WATER)));
    assert_eq!(voxelcraft::world::loot::java_item("cooked_beef"), Item::from_name("steak"));
    assert_eq!(voxelcraft::world::loot::java_item("quartz"), Some(Item::NETHER_QUARTZ));
    assert!(java_registry::resolves_item("wither_skeleton_skull", ""));
    assert!(!java_registry::resolves_block("wither_skeleton_skull", ""));
}
