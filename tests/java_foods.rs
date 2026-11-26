//! FOO-01a: every Java 26.3 food against VoxelCraft's items, both ways.
//! `tests/fixtures/java/26.3/foods.json` comes from the pinned server jar
//! (`tools/extract_java_food_fixtures.py`): `Foods`, `Consumables` and the
//! `Items` registrations that call `Item$Properties.food`.
use serde_json::Value;
use std::collections::BTreeSet;

use voxelcraft::{content::consumption, item::Item, java_registry, simulation::effects::Effect};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/foods.json")).unwrap()
}

/// The Java item id's English name, for [`java_registry::item`].
fn english(id: &str) -> String {
    let names: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    names["items"][id].as_str().unwrap_or_default().to_owned()
}

fn resolve(id: &str) -> Item {
    java_registry::item(id, &english(id)).unwrap_or_else(|| panic!("Java food {id} has no VoxelCraft item"))
}

#[test]
fn every_java_food_has_its_nutrition_saturation_and_consumable() {
    let fixture = fixture();
    let foods = fixture["foods"].as_array().unwrap();
    assert_eq!(foods.len(), 40, "Java 26.3 registers 40 foods");
    for food in foods {
        let id = food["item"].as_str().unwrap();
        let item = resolve(id);
        let nutrition = food["nutrition"].as_u64().unwrap() as u8;
        // The fixture's saturation is Java's float `nutrition * modifier * 2.0F`.
        let saturation = food["saturation"].as_f64().unwrap() as f32;
        assert_eq!(item.food(), Some((nutrition, saturation)), "{id}: nutrition and saturation");
        assert_eq!(
            voxelcraft::item::saturation_by_modifier(nutrition, food["saturation_modifier"].as_f64().unwrap() as f32),
            saturation,
            "{id}: FoodConstants.saturationByModifier"
        );
        assert_eq!(item.always_edible(), food["always_edible"].as_bool().unwrap(), "{id}: alwaysEdible");
        assert_eq!(consumption::ticks(item), Some(food["consume_ticks"].as_u64().unwrap() as u32), "{id}: seconds");
        assert_eq!(item.is_drink(), food["animation"] == "drink", "{id}: eat or drink");
        assert_eq!(u64::from(item.max_stack()), food["max_stack"].as_u64().unwrap(), "{id}: stack size");
        assert_eq!(item.remainder(), food["converts_to"].as_str().map(resolve), "{id}: usingConvertsTo");
        let effects: Vec<_> = food["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    Effect::from_id(e["effect"].as_str().unwrap()).unwrap(),
                    e["amplifier"].as_u64().unwrap() as u8,
                    e["ticks"].as_u64().unwrap() as u32,
                )
            })
            .collect();
        assert_eq!(item.food_effects(), effects.as_slice(), "{id}: ApplyStatusEffectsConsumeEffect");
        if !effects.is_empty() {
            let chance = food["effect_chance"].as_f64().unwrap() as f32;
            assert_eq!(item.food_effect_chance(), chance, "{id}: effect chance");
        }
        // The other consume effects are item-specific code paths; pin which
        // item each belongs to.
        let removes: Vec<&str> = food["removes_effects"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(removes, if item == Item::HONEY_BOTTLE { vec!["poison"] } else { vec![] }, "{id}: removes");
        assert!(!food["clears_effects"].as_bool().unwrap(), "{id}: only milk clears, and it isn't food");
        assert_eq!(
            food["teleports"].as_bool().unwrap(),
            item == voxelcraft::content::chorus::CHORUS_FRUIT,
            "{id}: TeleportRandomlyConsumeEffect"
        );
    }
}

#[test]
fn every_voxelcraft_food_is_a_java_food() {
    let java: BTreeSet<u16> =
        fixture()["foods"].as_array().unwrap().iter().map(|f| resolve(f["item"].as_str().unwrap()).0).collect();
    let ours: BTreeSet<u16> =
        Item::all_items().chain(Item::creative_palette()).filter(|item| item.food().is_some()).map(|i| i.0).collect();
    let extra: Vec<_> = ours.difference(&java).map(|&i| Item(i).name()).collect();
    assert!(extra.is_empty(), "foods Java 26.3 doesn't register: {extra:?}");
    let missing: Vec<_> = java.difference(&ours).map(|&i| Item(i).name()).collect();
    assert!(missing.is_empty(), "Java foods VoxelCraft's item list doesn't reach: {missing:?}");
}
