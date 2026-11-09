//! Inventory of unresolved names in runtime name-keyed content tables.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use voxelcraft::{entity::villager::trade_item_names, item::Item};
fn collect<'a>(v: &'a Value, names: &mut BTreeSet<&'a str>) {
    match v {
        Value::Object(o) => {
            if o.get("type").and_then(Value::as_str).is_some_and(|t| t.trim_start_matches("minecraft:") == "item")
                && let Some(n) = o.get("name").and_then(Value::as_str)
            {
                names.insert(n);
            }
            for child in o.values() {
                collect(child, names);
            }
        }
        Value::Array(a) => {
            for child in a {
                collect(child, names);
            }
        }
        _ => {}
    }
}
fn main() {
    let trades: BTreeSet<_> = trade_item_names().collect();
    let missing_trades: Vec<_> = trades.iter().filter(|n| Item::from_name(n).is_none()).collect();
    let old_trades: BTreeSet<_> = trades
        .iter()
        .map(|n| match *n {
            "cod" => "raw cod",
            "raw rabbit" => "rabbit",
            "salmon" => "raw salmon",
            "turtle scute" => "scute",
            "bucket of cod" => "cod bucket",
            n => n,
        })
        .collect();
    let missing_before: Vec<_> = old_trades.iter().filter(|n| Item::from_name(n).is_none()).collect();
    let wandering: BTreeSet<_> = voxelcraft::entity::wandering_trader::trade_item_names().collect();
    let original_wandering: BTreeSet<_> =
        wandering.iter().map(|n| if *n == "slimeball" { "slime ball" } else { n }).collect();
    let missing_original_wandering: Vec<_> =
        original_wandering.iter().filter(|n| Item::from_name(n).is_none()).collect();
    let missing_wandering: Vec<_> = wandering.iter().filter(|n| Item::from_name(n).is_none()).collect();
    let facts: Value = serde_json::from_str(include_str!("../src/world/loot_tables.json")).unwrap();
    let mut names = BTreeSet::new();
    collect(&facts["tables"], &mut names);
    let missing_loot: Vec<_> = names
        .iter()
        .filter(|n| voxelcraft::world::loot::java_item(n.trim_start_matches("minecraft:")).is_none())
        .collect();
    println!("{}", serde_json::to_string_pretty(&json!({"trade_names":trades.len(),"unresolved_original_trade_names":missing_before,"wandering_names":wandering.len(),"unresolved_wandering_names":missing_wandering,"unresolved_original_wandering_names":missing_original_wandering,"unresolved_trade_names":missing_trades,"loot_names":names.len(),"unresolved_loot_names":missing_loot})).unwrap());
}
