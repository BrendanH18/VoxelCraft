//! v1.0 gap track 6: every registered item is drawn as Java 26.3 draws it,
//! a flat sprite or a 3D model (`tests/fixtures/java/26.3/item_models.json`).
use serde_json::Value;
use voxelcraft::{inventory::Stack, item, java_registry};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/item_models.json")).unwrap()
}

#[test]
fn report() {
    let reference: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let models = fixture();
    let mut lines = Vec::new();
    for (id, english) in reference["items"].as_object().unwrap() {
        let Some(it) = java_registry::item(id, english.as_str().unwrap()) else { continue };
        let java = &models["items"][id];
        let ours = item::model_block(&Stack::new(it, 1));
        let java_kind = java["kind"].as_str().unwrap_or("?");
        let our_kind = if ours.is_some() { "block" } else { "flat" };
        let same = match java_kind {
            "flat" => our_kind == "flat",
            "block" => our_kind == "block",
            _ => true,
        };
        if !same || java_kind == "special" {
            lines.push(format!(
                "{id}: java {java_kind} {} {} {}, ours {our_kind}",
                java["model"], java["special"], java["layer0"]
            ));
        }
    }
    println!("{}", lines.join("\n"));
}
