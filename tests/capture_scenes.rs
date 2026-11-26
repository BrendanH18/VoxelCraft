//! The comparison scenes in `tools/capture_scenes.json` name only blocks,
//! items, mobs, effects and capture settings the game accepts, so a capture
//! run never stops on a typo. `tools/test_capture_scenes.py` covers how the
//! runner expands them into command lines.

use serde_json::Value;
use voxelcraft::entity::MobKind;
use voxelcraft::item::Item;
use voxelcraft::world::block::Block;

fn scenes() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tools/capture_scenes.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn block(name: &str) -> Block {
    Block::from_name(name).unwrap_or_else(|| panic!("unknown block {name:?}"))
}

/// A `--place` block: a name, or a raw state id for oriented states (as the
/// game's own `--place` parsing accepts), which must be a registered state.
fn placed(name: &str) -> Block {
    match name.parse::<u16>() {
        Ok(id) => {
            let b = Block(id);
            assert!(
                (id as usize) < voxelcraft::world::block::STATE_CAPACITY && !b.name().is_empty(),
                "unregistered block state {id}"
            );
            b
        }
        Err(_) => block(name),
    }
}

/// The value after each `flag` in a scene's extra arguments.
fn flag_values<'a>(args: &'a [Value], flag: &str) -> Vec<&'a str> {
    args.windows(2).filter(|w| w[0] == flag).map(|w| w[1].as_str().unwrap()).collect()
}

#[test]
fn every_scene_names_things_the_game_knows() {
    let data = scenes();
    let defaults = &data["defaults"];
    let mut names = std::collections::BTreeSet::new();
    for scene in data["scenes"].as_array().unwrap() {
        let name = scene["name"].as_str().unwrap();
        assert!(names.insert(name), "two scenes are named {name}");
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "scene names are file names: {name}"
        );
        for fill in scene["fill"].as_array().into_iter().flatten() {
            let fill = fill.as_array().unwrap();
            assert_eq!(fill.len(), 7, "{name}: fills are x1,y1,z1,x2,y2,z2,block");
            block(fill[6].as_str().unwrap());
        }
        for cell in scene["place"].as_array().into_iter().flatten() {
            let parts: Vec<&str> = cell.as_str().unwrap().split(',').collect();
            assert_eq!(parts.len(), 4, "{name}: {cell}");
            placed(parts[3]);
        }
        for mob in scene["spawn"].as_array().into_iter().flatten() {
            let kind = mob.as_str().unwrap().split(',').next().unwrap();
            assert!(MobKind::from_name(kind).is_some(), "{name}: unknown mob {kind}");
        }
        let args = scene["args"].as_array().cloned().unwrap_or_default();
        for flag in ["--give", "--drop", "--wear"] {
            for value in flag_values(&args, flag) {
                let item = value.split(',').next().unwrap();
                assert!(Item::from_name(item).is_some(), "{name}: unknown item {item}");
            }
        }
        for effect in flag_values(&args, "--effect") {
            voxelcraft::simulation::effects::Active::parse_option(effect).unwrap();
        }
        for tab in flag_values(&args, "--creative-tab") {
            voxelcraft::capture::parse_creative_tab(tab).unwrap();
        }
        let setting = |key: &str| if scene[key].is_null() { &defaults[key] } else { &scene[key] };
        voxelcraft::capture::parse_gui_scale(&match setting("gui_scale") {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap();
        voxelcraft::capture::parse_weather(setting("weather").as_str().unwrap()).unwrap();
        for rule in setting("gamerules").as_array().unwrap() {
            voxelcraft::capture::parse_gamerule(rule.as_str().unwrap()).unwrap();
        }
        for scale in scene["matrix"]["gui_scale"].as_array().into_iter().flatten() {
            let text = scale.as_str().map_or_else(|| scale.to_string(), str::to_string);
            voxelcraft::capture::parse_gui_scale(&text).unwrap();
        }
    }
}

#[test]
fn scenes_cover_the_v018_comparison_list() {
    let data = scenes();
    let groups: std::collections::BTreeSet<&str> =
        data["scenes"].as_array().unwrap().iter().filter_map(|s| s["group"].as_str()).collect();
    for group in ["materials", "mobs", "ui", "title", "atmosphere"] {
        assert!(groups.contains(group), "no {group} scenes");
    }
    // The UI scenes include both reference resolutions with Auto and a fixed scale.
    let ui = data["scenes"].as_array().unwrap().iter().find(|s| s["name"] == "inventory").unwrap();
    let sizes: Vec<_> = ui["matrix"]["size"].as_array().unwrap().iter().map(|s| s.to_string()).collect();
    assert_eq!(sizes, ["[1920,1080]", "[1280,720]"]);
}
