//! Complete Java 26.3 cooking-fuel registrations, with missing content kept visible.
use serde_json::Value;
use voxelcraft::{content::fuel, item::Item};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/fuel_integrations.json")).unwrap()
}
/// The registered item for a Java ID, matched by ID or by English name
/// (VoxelCraft keeps names such as "block of coal" for `coal_block`).
fn registered(name: &str) -> Option<Item> {
    let names: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let english = names["items"][name.strip_prefix("minecraft:").unwrap_or(name)].as_str();
    Item::from_name(name).or_else(|| english.and_then(|e| Item::from_name(&e.to_lowercase())))
}

/// Every VoxelCraft item a player can hold, block items included (blocks
/// placed by another item, such as door halves, resolve to that item).
fn all_items() -> Vec<Item> {
    use voxelcraft::world::block::{Block, STATE_CAPACITY};
    let blocks = (0..STATE_CAPACITY as u16).map(Block).filter(|b| b.base() == *b && !b.name().is_empty());
    let blocks = blocks.filter_map(|b| Item::from_name(b.name()));
    let mut items: Vec<Item> = blocks.chain(Item::all_items()).collect();
    items.sort_by_key(|i| i.0);
    items.dedup();
    items
}

/// No item burns unless Java 26.3 registers it as a fuel.
#[test]
fn only_java_fuels_burn() {
    let f = fixture();
    let fuels: std::collections::BTreeSet<u16> = f["fuels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|case| registered(case["item"].as_str().unwrap()))
        .map(|i| i.0)
        .collect();
    let extra: Vec<&str> = all_items()
        .into_iter()
        // A charged crossbow is Java's crossbow with a loaded projectile.
        .filter(|i| fuel::ticks(*i).is_some() && !fuels.contains(&i.0) && *i != Item::CHARGED_CROSSBOW)
        .map(|i| i.name())
        .collect();
    assert!(extra.is_empty(), "these burn but aren't Java 26.3 fuels: {extra:?}");
}
#[test]
fn every_registered_reference_fuel_and_nonfuel_has_its_exact_tick_value() {
    let f = fixture();
    let mut checked = 0;
    let mut missing = Vec::new();
    let mut failures = Vec::new();
    for case in f["fuels"].as_array().unwrap() {
        let name = case["item"].as_str().unwrap();
        let Some(item) = registered(name) else {
            missing.push(name);
            continue;
        };
        checked += 1;
        let expected = Some(case["ticks"].as_u64().unwrap() as u32);
        if fuel::ticks(item) != expected {
            failures.push(format!("{name}: expected {expected:?}, got {:?}", fuel::ticks(item)));
        }
    }
    for name in f["non_fuels"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()) {
        if let Some(item) = registered(name) {
            assert_eq!(fuel::ticks(item), None, "reference non-fuel {name}");
        }
    }
    // This scope tests existing registrations; missing reference content is not blessed.
    // The inventory example emits every unresolved name for the M0 catalogue.
    eprintln!(
        "checked {checked} implemented fuel registrations; {} reference items remain unregistered: {missing:?}",
        missing.len()
    );
    assert!(checked > 100, "the reference inventory must cover the existing families");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn loaded_crossbow_retains_the_same_fuel_as_its_unloaded_item() {
    let f = fixture();
    let expected = f["fuels"].as_array().unwrap().iter().find(|v| v["item"] == "minecraft:crossbow").unwrap()["ticks"]
        .as_u64()
        .unwrap() as u32;
    assert_eq!(fuel::ticks(Item::CROSSBOW), Some(expected));
    assert_eq!(fuel::ticks(Item::CHARGED_CROSSBOW), Some(expected));
}

#[test]
fn every_implemented_fuel_can_finish_an_interrupted_furnace_job() {
    use voxelcraft::{inventory::Stack, world::furnace::Furnace};
    for case in fixture()["fuels"].as_array().unwrap() {
        let name = case["item"].as_str().unwrap();
        let Some(item) = registered(name) else { continue };
        let ticks = case["ticks"].as_u64().unwrap() as u32;
        let count = 200u32.div_ceil(ticks) as u8;
        assert!(count <= item.max_stack());
        let mut furnace = Furnace {
            input: Some(Stack::new(Item::RAW_IRON, 1)),
            fuel: Some(Stack::new(item, count)),
            ..Default::default()
        };
        furnace.tick(4.0);
        assert_eq!(furnace.cook, 4.0, "{name}");
        let mut restored = Furnace::deserialize(&furnace.serialize()).unwrap();
        restored.tick(6.0);
        assert_eq!(restored.output, Some(Stack::new(Item::IRON_INGOT, 1)), "{name}");
        assert!(restored.input.is_none(), "{name}");
        assert_eq!(restored.fuel.map(|s| s.item), (item == Item::LAVA_BUCKET).then_some(Item::BUCKET), "{name}");
    }
}

#[test]
fn odd_carpet_ticks_round_down_in_fast_cooking_and_nonfuels_remain_untouched() {
    use voxelcraft::{
        inventory::Stack,
        world::furnace::{Furnace, FurnaceKind},
    };
    let f = fixture();
    let ticks = f["fuels"].as_array().unwrap().iter().find(|v| v["item"] == "minecraft:white_carpet").unwrap()["ticks"]
        .as_u64()
        .unwrap() as u32;
    let carpet = registered("minecraft:white_carpet").unwrap();
    for kind in [FurnaceKind::Normal, FurnaceKind::Blast, FurnaceKind::Smoker] {
        let mut furnace = Furnace {
            kind,
            input: Some(Stack::new(if kind == FurnaceKind::Smoker { Item::COD } else { Item::RAW_IRON }, 1)),
            fuel: Some(Stack::new(carpet, 4)),
            ..Default::default()
        };
        furnace.tick(0.05);
        let effective = if kind == FurnaceKind::Normal { ticks } else { ticks / 2 * 2 };
        assert_eq!(furnace.burn_total, effective as f32 / 20.0);
    }
    for name in f["non_fuels"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()) {
        let Some(item) = registered(name) else { continue };
        let mut furnace = Furnace {
            input: Some(Stack::new(Item::RAW_IRON, 1)),
            fuel: Some(Stack::new(item, 1)),
            ..Default::default()
        };
        let before = furnace.serialize();
        furnace.tick(1.0);
        assert_eq!(furnace.serialize(), before, "{name}");
    }
}
