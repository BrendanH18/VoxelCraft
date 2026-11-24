//! v0.18 Track 9 against Java 26.3's `sounds.json` fixture (from
//! `tools/extract_java_sound_fixtures.py`): every registered block state
//! finds its Java block and so its `SoundType`, and chest lids move at
//! `ChestLidController`'s speed. The game's own subtitle and sound-type
//! checks are unit tests of the client (`src/audio/tests.rs`).
use std::collections::BTreeSet;
use std::sync::Arc;

use glam::{IVec2, IVec3};
use serde_json::Value;
use voxelcraft::java_registry::state_java_ids;
use voxelcraft::world::World;
use voxelcraft::world::block::{Block, STATE_CAPACITY};
use voxelcraft::world::terrain::Generator;

fn fixture() -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/sounds.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn block_states_find_their_java_sound_types() {
    let java = fixture();
    let names = java["block_names"].as_object().unwrap();
    let ids = state_java_ids(names.iter().map(|(id, n)| (id.as_str(), n.as_str().unwrap())));
    let types = java["sound_types"].as_object().unwrap();
    let mut unresolved = BTreeSet::new();
    let mut resolved = 0;
    for i in 0..STATE_CAPACITY as u16 {
        let name = Block(i).name();
        if name.is_empty() {
            continue;
        }
        match ids.get(name) {
            Some(id) => {
                let t = java["block_sound_types"][id].as_str().unwrap_or_else(|| panic!("{id} has no sound type"));
                assert!(types.contains_key(t), "{id}: {t} is in the fixture");
                resolved += 1;
            }
            None => {
                unresolved.insert(name);
            }
        }
    }
    println!("{resolved} states resolve; {} names don't:\n{unresolved:#?}", unresolved.len());
    assert!(resolved > 2000, "{resolved}");
    // Every Java block VoxelCraft has by name is a resolved Java ID.
    for (id, english) in names {
        let english = english.as_str().unwrap().to_lowercase();
        if let Some(b) = Block::from_name(&english) {
            assert!(ids.contains_key(b.name()), "{id} ({english})");
        }
    }
}

#[test]
fn sound_types_and_subtitles_are_complete() {
    let java = fixture();
    // Every type a block uses has its five events and Java's volume and pitch.
    for t in java["block_sound_types"].as_object().unwrap().values() {
        let t = &java["sound_types"][t.as_str().unwrap()];
        for key in ["break", "step", "place", "hit", "fall", "volume", "pitch"] {
            assert!(!t[key].is_null(), "{t} lacks {key}");
        }
    }
    assert_eq!(java["sound_types"]["METAL"]["pitch"], 1.5);
    assert_eq!(java["block_sound_types"]["decorated_pot"], "DECORATED_POT");
    // Every subtitle key has English text but the one Java leaves untranslated.
    let text = java["subtitle_text"].as_object().unwrap();
    let missing: BTreeSet<&str> = java["subtitles"]
        .as_object()
        .unwrap()
        .values()
        .filter_map(Value::as_str)
        .filter(|k| !text.contains_key(*k))
        .collect();
    assert_eq!(missing, BTreeSet::from(["subtitles.entity.baby_chicken.hurt"]));
    // The overlay and its option, as the client sets them.
    assert_eq!(java["options"]["showSubtitles"]["label"], "Closed Captions");
    assert_eq!(java["options"]["showSubtitles"]["default"], false);
    assert_eq!(java["overlay"]["display_time_ms"], 3000);
}

#[test]
fn chest_lids_open_and_shut_at_javas_speed() {
    let java = fixture();
    let step = java["lids"]["chest_lid_step_per_tick"].as_f64().unwrap() as f32;
    assert_eq!(step, voxelcraft::world::lids::LID_STEP);
    let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    w.load_test_column(IVec2::ZERO, 0);
    let p = IVec3::new(2, 10, 2);
    w.set_block(p, Block::CHEST);
    w.set_container_viewers(&[p]);
    let ticks_to = |w: &mut World, target: f32| {
        let mut n = 0;
        while (w.chest_lid(p, 1.0) - target).abs() > 1e-4 && n < 40 {
            w.tick_lids();
            n += 1;
        }
        n
    };
    assert_eq!(ticks_to(&mut w, 1.0), (1.0 / step).round() as i32, "opening takes ten ticks");
    w.set_container_viewers(&[]);
    assert_eq!(ticks_to(&mut w, 0.0), (1.0 / step).round() as i32, "closing takes ten ticks");
    let cues = w.take_lid_cues();
    assert_eq!(cues.iter().map(|c| c.open).collect::<Vec<_>>(), [true, false]);
}
