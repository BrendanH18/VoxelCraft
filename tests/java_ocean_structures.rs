//! v0.16 Track F: shipwrecks, ocean monuments and the conduit against
//! `fixtures/java/26.3/ocean_structures.json`, which
//! `tools/extract_java_ocean_fixtures.py` reads from the pinned server jar.

use serde_json::Value;
use voxelcraft::item::Item;
use voxelcraft::world::biome::Biome;
use voxelcraft::world::block::{Block, Facing};
use voxelcraft::world::shipwreck::{self, templates};
use voxelcraft::world::{conduit, monument, worldgen_random};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/ocean_structures.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

fn facing(name: &str) -> Facing {
    match name {
        "north" => Facing::North,
        "south" => Facing::South,
        "east" => Facing::East,
        _ => Facing::West,
    }
}

#[test]
fn shipwreck_templates_match_javas_catalogue_both_ways() {
    let f = fixture();
    let java = f["shipwreck_templates"].as_object().unwrap();
    assert_eq!(java.len(), templates::TEMPLATES.len());
    for t in &templates::TEMPLATES {
        let j = &java[t.name];
        let size: Vec<i64> = j["size"].as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect();
        assert_eq!(size, t.size.map(i64::from).to_vec(), "{}", t.name);
        let mut want: Vec<(String, [i32; 3], Facing)> = j["markers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                let p: Vec<i32> = m["pos"].as_array().unwrap().iter().map(|n| n.as_i64().unwrap() as i32).collect();
                (
                    m["name"].as_str().unwrap().to_string(),
                    [p[0], p[1], p[2]],
                    facing(m["chest_facing"].as_str().unwrap()),
                )
            })
            .collect();
        let mut got: Vec<(String, [i32; 3], Facing)> =
            t.markers.iter().map(|(m, p, f)| (m.java().to_string(), *p, *f)).collect();
        want.sort_by_key(|m| m.1);
        got.sort_by_key(|m| m.1);
        assert_eq!(got, want, "{}", t.name);
    }
    for name in java.keys() {
        assert!(templates::template_index(name).is_some(), "{name}");
    }
}

#[test]
fn shipwreck_palettes_swap_javas_woods() {
    let f = fixture();
    let java = f["shipwreck_palettes"].as_array().unwrap();
    assert_eq!(java.len(), templates::PALETTES.len());
    for (j, (hull, trim)) in java.iter().zip(templates::PALETTES) {
        assert_eq!(j["oak"].as_str().unwrap(), hull);
        assert_eq!(j["spruce"].as_str().unwrap(), trim);
    }
}

#[test]
fn shipwreck_placement_and_lists_match_java() {
    let f = fixture();
    let set = &f["structure_sets"]["shipwrecks"];
    let placement = &set["placement"];
    assert_eq!(placement["spacing"], shipwreck::SPACING);
    assert_eq!(placement["separation"], shipwreck::SEPARATION);
    assert_eq!(placement["salt"].as_u64().unwrap(), shipwreck::SALT);
    assert!(placement.get("spread_type").is_none(), "linear");
    let structures: Vec<(String, i64)> = set["structures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["structure"].as_str().unwrap().to_string(), s["weight"].as_i64().unwrap()))
        .collect();
    assert_eq!(structures, [("shipwreck".to_string(), 1), ("shipwreck_beached".to_string(), 1)]);
    assert_eq!(f["structures"]["shipwreck"]["is_beached"], false);
    assert_eq!(f["structures"]["shipwreck_beached"]["is_beached"], true);
    let pieces = &f["shipwreck_pieces"];
    let strip = |v: &Value| strings(v).into_iter().map(|s| s.trim_start_matches("shipwreck/").to_string()).collect();
    let beached: Vec<String> = strip(&pieces["beached"]);
    let ocean: Vec<String> = strip(&pieces["ocean"]);
    assert_eq!(beached, shipwreck::BEACHED.map(str::to_string).to_vec());
    assert_eq!(ocean, shipwreck::OCEAN.map(str::to_string).to_vec());
    let pivot: Vec<i64> = pieces["pivot"].as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect();
    assert_eq!(pivot, [shipwreck::PIVOT.x, shipwreck::PIVOT.y, shipwreck::PIVOT.z].map(i64::from).to_vec());
    assert_eq!(pieces["start_y"], shipwreck::START_Y);
    assert_eq!(pieces["max_region_size"], shipwreck::MAX_REGION_SIZE);
    // Every template fits a world-gen region, so the structure never
    // adjusts the height before `postProcess`.
    for t in &templates::TEMPLATES {
        assert!(t.size[0] <= shipwreck::MAX_REGION_SIZE && t.size[1] <= shipwreck::MAX_REGION_SIZE);
    }
}

#[test]
fn structure_indices_and_steps_match_javas_registry_order() {
    let f = fixture();
    let surface = strings(&f["surface_structures"]);
    let index = |name: &str| surface.iter().position(|s| s == name).unwrap() as i32;
    assert_eq!(index("monument"), monument::FEATURE_INDEX);
    assert_eq!(index("shipwreck"), shipwreck::FEATURE_INDEX);
    assert_eq!(index("shipwreck_beached"), shipwreck::FEATURE_INDEX_BEACHED);
    let steps = strings(&f["generation_steps"]);
    assert_eq!(
        steps.iter().position(|s| s == "surface_structures").unwrap() as i32,
        worldgen_random::SURFACE_STRUCTURES
    );
    for s in ["shipwreck", "shipwreck_beached", "monument"] {
        assert_eq!(f["structures"][s]["step"], "surface_structures");
    }
}

#[test]
fn monument_placement_box_and_biomes_match_java() {
    let f = fixture();
    let placement = &f["structure_sets"]["ocean_monuments"]["placement"];
    assert_eq!(placement["spacing"], monument::SPACING);
    assert_eq!(placement["separation"], monument::SEPARATION);
    assert_eq!(placement["salt"].as_u64().unwrap(), monument::SALT);
    assert_eq!(placement["spread_type"], "triangular");
    let c = &f["monument"]["constants"];
    assert_eq!(c["building_WIDTH"], monument::WIDTH);
    assert_eq!(c["building_DEPTH"], monument::WIDTH);
    assert_eq!(c["building_BIOME_RANGE_CHECK"], monument::BIOME_CHECK_RADIUS);
    assert_eq!(c["building_TOP_POSITION"], monument::BASE_Y + monument::HEIGHT - 1);
    assert_eq!(f["monument"]["start_offset_in_chunk"], monument::BIOME_CHECK_OFFSET);
    // The spawn overrides: guardians (2–4) over the full box, no axolotls or
    // underground water creatures.
    let o = &f["structures"]["monument"]["spawn_overrides"];
    assert_eq!(o["monster"]["bounding_box"], "full");
    assert_eq!(o["monster"]["spawns"][0]["type"], "guardian");
    assert_eq!(o["axolotls"]["spawns"].as_array().unwrap().len(), 0);
    assert_eq!(o["underground_water_creature"]["spawns"].as_array().unwrap().len(), 0);
    // The biome tags agree with VoxelCraft's biome groups.
    let tag = |t: &str| -> Vec<Biome> {
        strings(&f["biome_tags"][t]).iter().map(|n| Biome::from_name(n).unwrap_or_else(|| panic!("{n}"))).collect()
    };
    for b in Biome::ALL {
        assert_eq!(tag("has_structure/ocean_monument").contains(&b), b.is_deep_ocean(), "{b:?}");
        assert_eq!(tag("has_structure/shipwreck").contains(&b), b.is_ocean(), "{b:?}");
        assert_eq!(tag("has_structure/shipwreck_beached").contains(&b), b.is_beach(), "{b:?}");
        let surrounding = b.is_ocean() || matches!(b, Biome::River | Biome::FrozenRiver);
        assert_eq!(tag("required_ocean_monument_surrounding").contains(&b), surrounding, "{b:?}");
    }
}

#[test]
fn the_conduit_matches_its_block_entity_and_data() {
    let f = fixture();
    let c = &f["conduit"];
    let k = &c["constants"];
    assert_eq!(k["MIN_ACTIVE_SIZE"], conduit::MIN_ACTIVE_SIZE);
    assert_eq!(k["MIN_KILL_SIZE"], conduit::MIN_KILL_SIZE);
    assert_eq!(k["KILL_RANGE"].as_f64().unwrap(), conduit::KILL_RANGE);
    assert_eq!(k["ROTATION_SPEED"].as_f64().unwrap() as f32, conduit::ROTATION_SPEED);
    assert_eq!(c["shape_interval"], conduit::SHAPE_INTERVAL);
    assert_eq!(c["ambient_interval"], conduit::AMBIENT_INTERVAL);
    assert_eq!(c["effect_ticks"], conduit::EFFECT_TICKS);
    assert_eq!(c["attack_damage"].as_f64().unwrap() as f32, conduit::ATTACK_DAMAGE);
    assert_eq!(c["kill_range"].as_f64().unwrap(), conduit::KILL_RANGE);
    let (div, mul) = (c["range_divisor"].as_i64().unwrap() as i32, c["range_multiplier"].as_i64().unwrap() as i32);
    for n in 0..=42 {
        assert_eq!(conduit::range(n), n as i32 / div * mul);
    }
    assert_eq!(c["block_strength"][0].as_f64().unwrap() as f32, conduit::HARDNESS);
    assert_eq!(c["item_rarity"], "uncommon");
    assert!(Item::from_block(conduit::CONDUIT).is_uncommon());
    // `#conduit_effect_block`, both ways over every block.
    let tag: Vec<Block> =
        strings(&f["conduit_effect_block"]).iter().map(|n| Block::from_name(n).expect("a frame block")).collect();
    for b in &tag {
        assert!(conduit::is_frame(*b), "{}", b.name());
    }
    assert_eq!(Block::creative_palette().filter(|b| conduit::is_frame(*b)).count(), tag.len());
    // `recipe/conduit.json` and `loot_table/blocks/conduit.json`.
    assert_eq!(f["recipe"]["pattern"], serde_json::json!(["###", "#X#", "###"]));
    assert_eq!(f["recipe"]["key"]["#"], "nautilus_shell");
    assert_eq!(f["recipe"]["key"]["X"], "heart_of_the_sea");
    assert_eq!(f["loot_table"]["pools"][0]["entries"][0]["name"], "conduit");
    assert!(f["loot_table"]["pools"][0].get("conditions").is_none());
    assert_eq!(conduit::CONDUIT.drop(), Some(Item::from_block(conduit::CONDUIT)));
}
