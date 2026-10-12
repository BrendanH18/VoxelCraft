//! Independent expectations normalized from the official Java 26.3 artifact.
use serde_json::Value;
use voxelcraft::{
    inventory::Stack,
    item::Item,
    potion::Potion,
    world::{
        block::Block,
        brewing,
        furnace::{Furnace, FurnaceKind},
    },
};

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/item_integrations.json")).unwrap()
}

fn item(id: &str) -> Item {
    let name = id.strip_prefix("minecraft:").unwrap_or(id);
    if name == "bamboo_block" {
        return Item::from_block(voxelcraft::world::overworld_blocks::BAMBOO_BLOCK);
    }
    Item::from_name(name).unwrap_or_else(|| panic!("unregistered fixture item {id}"))
}

#[test]
fn fuels_match_java_26_3_values_and_exceptions() {
    let fixture = reference();
    let mut failures = Vec::new();
    for case in fixture["fuel_cases"].as_array().unwrap() {
        let name = case["item"].as_str().unwrap();
        let item = item(name);
        let expected = case["ticks"].as_u64().map(|ticks| ticks as f32 / 20.0);
        let actual = voxelcraft::world::furnace::burn_time(item);
        if actual != expected {
            failures.push(format!("{name}: expected {expected:?}, got {actual:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pufferfish_brews_java_26_3_water_breathing_in_both_bottle_types() {
    for case in reference()["brewing"].as_array().unwrap() {
        let input = Potion::from_id(case["input"].as_str().unwrap()).unwrap();
        let output = Potion::from_id(case["output"].as_str().unwrap()).unwrap();
        let ingredient = item(case["ingredient"].as_str().unwrap());
        let splash = case["bottle"] == "minecraft:splash_potion";
        let bottle = if splash { Item::splash_potion } else { Item::potion };
        assert_eq!(brewing::brew_item(bottle(input), ingredient), Some(bottle(output)));
        assert!(brewing::is_ingredient(ingredient));
    }
    assert_eq!(brewing::brew(Potion::WATER, Item::PUFFERFISH), None);
}

#[test]
fn new_fuels_finish_smelt_after_an_interrupted_save() {
    for name in ["bamboo_mosaic", "dried_kelp_block"] {
        let mut furnace = Furnace {
            input: Some(Stack::new(Item::from_block(Block::IRON_ORE), 2)),
            fuel: Some(Stack::new(Item::from_name(name).unwrap(), 1)),
            ..Default::default()
        };
        furnace.tick(4.0);
        assert!(furnace.is_lit(), "{name} must actually start cooking");
        let mut restored = Furnace::deserialize(&furnace.serialize()).unwrap();
        assert_eq!(restored.cook, 4.0);
        assert!(restored.fuel.is_none(), "the fuel was consumed before saving");
        restored.tick(6.0);
        assert_eq!(restored.output, Some(Stack::new(Item::IRON_INGOT, 1)));
        assert_eq!(restored.input.unwrap().count, 1);
        assert!(restored.burn_left > 0.0);
    }
}

#[test]
fn fast_furnaces_apply_java_integer_fuel_reduction() {
    let normal_ticks = reference()["fuel_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["item"] == "minecraft:dried_kelp_block")
        .unwrap()["ticks"]
        .as_u64()
        .unwrap();
    for kind in [FurnaceKind::Normal, FurnaceKind::Smoker, FurnaceKind::Blast] {
        let mut furnace = Furnace {
            kind,
            input: Some(Stack::new(if kind == FurnaceKind::Smoker { Item::COD } else { Item::RAW_IRON }, 1)),
            fuel: Some(Stack::new(Item::from_name("dried_kelp_block").unwrap(), 1)),
            ..Default::default()
        };
        furnace.tick(0.05);
        let effective_ticks = if kind == FurnaceKind::Normal { normal_ticks } else { normal_ticks / 2 * 2 };
        assert_eq!(furnace.burn_total, effective_ticks as f32 / 20.0);
    }
}

#[test]
fn pufferfish_brewing_resumes_after_save_without_consuming_twice() {
    let mut stand = brewing::BrewingStand::default();
    stand.bottles = [Some(Stack::new(Item::potion(Potion::from_id("awkward").unwrap()), 1)); 3];
    stand.ingredient = Some(Stack::new(Item::PUFFERFISH, 2));
    stand.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
    stand.tick(0.0);
    for _ in 0..200 {
        stand.tick(0.05);
    }
    assert!(stand.is_brewing());
    let mut restored = brewing::BrewingStand::deserialize(&stand.serialize()).unwrap();
    for _ in 0..200 {
        restored.tick(0.05);
    }
    let output = Item::potion(Potion::from_id("water_breathing").unwrap());
    assert!(restored.bottles.iter().all(|slot| *slot == Some(Stack::new(output, 1))));
    assert_eq!(restored.ingredient, Some(Stack::new(Item::PUFFERFISH, 1)));
    assert_eq!(restored.fuel_left, brewing::FUEL_USES - 1);
}
