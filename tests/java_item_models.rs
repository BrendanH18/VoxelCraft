//! v1.0 gap track 6: every registered item is drawn the way Java 26.3 draws
//! it, as a flat (extruded) sprite or as a 3D model, from the GUI leaf of
//! its item model definition (`tests/fixtures/java/26.3/item_models.json`,
//! `tools/extract_java_item_model_fixtures.py`). Since v1.0 gap track 6b
//! (`docs/verification/2026-11-27-v1.0-gap-track-06b-item-drawing.md`)
//! there are no exceptions: block items Java draws as sprites have item
//! sprites (`content::block_item_sprites`), and beds, banners, the shield,
//! the small dripleaf and the spore blossom have 3D models
//! (`content::item_models`).
use serde_json::Value;
use voxelcraft::{inventory::Stack, item, java_registry};

#[test]
fn every_item_is_flat_or_3d_as_in_java() {
    let reference: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let models: Value = serde_json::from_str(include_str!("fixtures/java/26.3/item_models.json")).unwrap();
    let mut checked = 0;
    for (id, english) in reference["items"].as_object().unwrap() {
        let Some(it) = java_registry::item(id, english.as_str().unwrap()).filter(|_| id != "air") else { continue };
        let java = &models["items"][id];
        let java_3d = match java["kind"].as_str() {
            Some("flat") => false,
            Some("block" | "special") => true,
            other => panic!("{id}: unexpected Java model kind {other:?}"),
        };
        let ours_3d = item::drawn_3d(&Stack::new(it, 1));
        checked += 1;
        // v1.0 gap track 6b: no exceptions are left.
        assert_eq!(java_3d, ours_3d, "{id}: Java draws it {}, the game {}", kind(java_3d), kind(ours_3d));
    }
    assert!(checked > 1600, "only {checked} items resolved");
    println!("{checked} items checked, every one drawn as Java draws it");
}

fn kind(three_d: bool) -> &'static str {
    if three_d { "in 3D" } else { "as a sprite" }
}

#[test]
fn panes_bars_and_flat_blocks_show_their_texture_as_a_sprite() {
    // Java's `item/generated` with the block's own texture: these items are
    // flat, drawn with their block's first face layer.
    for (id, english) in
        [("glass_pane", "Glass Pane"), ("iron_bars", "Iron Bars"), ("vine", "Vines"), ("frogspawn", "Frogspawn")]
    {
        let it = java_registry::item(id, english).unwrap();
        let block = it.block().unwrap();
        assert!(block.flat_icon(), "{id}");
        assert!(item::model_block(&Stack::new(it, 1)).is_none(), "{id}");
    }
}
