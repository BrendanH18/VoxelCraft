//! Cold-path reference inventory; emits unsupported registrations instead of hiding them.
use serde_json::{Value, json};
use voxelcraft::{content::fuel, item::Item};

fn main() {
    let reference: Value =
        serde_json::from_str(include_str!("../tests/fixtures/java/26.3/fuel_integrations.json")).unwrap();
    let mut implemented = Vec::new();
    let mut unregistered = Vec::new();
    let mut failures = Vec::new();
    for case in reference["fuels"].as_array().unwrap() {
        let id = case["item"].as_str().unwrap();
        if let Some(item) = Item::from_name(id) {
            let expected = case["ticks"].as_u64().unwrap() as u32;
            let actual = fuel::ticks(item);
            implemented.push(json!({"reference_id":id,"saved_item_id":item.0,"name":item.name(),"expected_ticks":expected,"actual_ticks":actual}));
            if actual != Some(expected) {
                failures.push(json!({"reference_id":id,"expected":expected,"actual":actual}));
            }
        } else {
            unregistered.push(id);
        }
    }
    let mut non_fuels = Vec::new();
    for id in reference["non_fuels"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()) {
        if let Some(item) = Item::from_name(id) {
            non_fuels.push(json!({"reference_id":id,"saved_item_id":item.0,"actual_ticks":fuel::ticks(item)}));
            if fuel::ticks(item).is_some() {
                failures.push(json!({"reference_id":id,"expected":null,"actual":fuel::ticks(item)}));
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&json!({
        "reference":reference["reference"],"server_sha1":reference["server_sha1"],
        "reference_fuel_count":reference["fuels"].as_array().unwrap().len(),
        "implemented_fuel_count":implemented.len(),"unregistered_fuel_count":unregistered.len(),
        "implemented":implemented,"unregistered":unregistered,"checked_non_fuels":non_fuels,"failures":failures,
        "scope":"Fuel integration for implemented registrations; this is not acquisition, player or full content acceptance."
    })).unwrap());
    if !failures.is_empty() {
        std::process::exit(1);
    }
}
