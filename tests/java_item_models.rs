//! v1.0 gap track 6: every registered item is drawn the way Java 26.3 draws
//! it, as a flat (extruded) sprite or as a 3D model, from the GUI leaf of
//! its item model definition (`tests/fixtures/java/26.3/item_models.json`,
//! `tools/extract_java_item_model_fixtures.py`). The items listed below
//! still differ and are recorded in
//! `docs/verification/2026-11-27-v1.0-gap-track-06-items-utilities.md`.
use std::collections::BTreeSet;

use serde_json::Value;
use voxelcraft::{inventory::Stack, item, java_registry};

/// Block items Java draws as a sprite, which the game draws in 3D from their
/// world shape: it has no such sprites yet (Java's are item textures of
/// their own, or for the lever, redstone torch and tripwire hook a block
/// texture the game's boxes don't share).
const STILL_3D: &[&str] = &[
    "bamboo",
    "bell",
    "brewing_stand",
    "campfire",
    "cauldron",
    "comparator",
    "flower_pot",
    "hopper",
    "leaf_litter",
    "lever",
    "redstone_torch",
    "pink_petals",
    "repeater",
    "sniffer_egg",
    "soul_campfire",
    "tripwire_hook",
    "turtle_egg",
    "wildflowers",
];
/// Families of [`STILL_3D`]: every door, lantern and chain.
const STILL_3D_SUFFIXES: &[&str] = &["_door", "lantern", "_chain"];
/// Items Java draws in 3D that the game draws as a sprite: beds, two
/// plants, and the banner and shield (Java's special renderers).
const STILL_FLAT: &[&str] = &["small_dripleaf", "spore_blossom", "shield"];
const STILL_FLAT_SUFFIXES: &[&str] = &["_bed", "_banner"];

fn listed(id: &str, ids: &[&str], suffixes: &[&str]) -> bool {
    ids.contains(&id) || suffixes.iter().any(|s| id.ends_with(s))
}

#[test]
fn every_item_is_flat_or_3d_as_in_java() {
    let reference: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let models: Value = serde_json::from_str(include_str!("fixtures/java/26.3/item_models.json")).unwrap();
    let mut differ = BTreeSet::new();
    let mut checked = 0;
    for (id, english) in reference["items"].as_object().unwrap() {
        let Some(it) = java_registry::item(id, english.as_str().unwrap()).filter(|_| id != "air") else { continue };
        let java = &models["items"][id];
        let java_3d = match java["kind"].as_str() {
            Some("flat") => false,
            Some("block" | "special") => true,
            other => panic!("{id}: unexpected Java model kind {other:?}"),
        };
        let ours_3d = item::model_block(&Stack::new(it, 1)).is_some();
        checked += 1;
        if java_3d != ours_3d {
            differ.insert(id.as_str());
            let expected = if java_3d {
                listed(id, STILL_FLAT, STILL_FLAT_SUFFIXES)
            } else {
                listed(id, STILL_3D, STILL_3D_SUFFIXES)
            };
            assert!(expected, "{id}: Java draws it {}, the game {}", kind(java_3d), kind(ours_3d));
        }
    }
    assert!(checked > 1600, "only {checked} items resolved");
    // Every listed difference still exists (a fixed one comes off the list).
    for id in STILL_3D.iter().chain(STILL_FLAT) {
        assert!(differ.contains(id), "{id} now matches Java: take it off the list");
    }
    println!("{checked} items checked, {} still drawn differently", differ.len());
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
