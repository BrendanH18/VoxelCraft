//! Save compatibility (v1.0 Track 1, rows UX-05b and UX-04a): a world saved
//! by every tagged release from v0.8.0 to v0.18, each made with that tag's
//! own game on Boat (`tools/save_fixtures/`), opens with this build with its
//! blocks, inventory, chest, player, agent player and entities, then saves
//! and reloads without losing anything. The policy these check is
//! `docs/saves.md`.
//!
//! Each fixture's `manifest.json` says what its run put in the world. The
//! fixture's `chunks.bin` keeps the run's own chunk records for the sky
//! chunks it built in (chunk y 4) byte for byte; the terrain chunks the
//! simulation touched were left out to keep the fixture small.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::entity::{Entities, MobKind};
use voxelcraft::inventory::{Inventory, Stack};
use voxelcraft::item::Item;
use voxelcraft::world::World;
use voxelcraft::world::block::Block;
use voxelcraft::world::chunk::ChunkData;
use voxelcraft::world::region::{REGION_DIR, RegionStore};
use voxelcraft::world::storage::{LevelInfo, SavedChunks, Storage, VERSION_KEY};
use voxelcraft::world::terrain::Generator;

/// Every tagged release whose saves 1.0 opens (`docs/saves.md`).
const RELEASES: [&str; 11] =
    ["v0.8.0", "v0.9", "v0.10", "v0.11", "v0.12", "v0.13", "v0.14", "v0.15", "v0.16", "v0.17", "v0.18"];

/// The checked-in fixtures.
fn checked_in() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/saves")
}

fn manifest(root: &Path, tag: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root.join(tag).join("manifest.json")).unwrap()).unwrap()
}

/// The version a release's saves name: none before v0.11, which added it.
fn saving_version(tag: &str) -> Option<String> {
    let cargo = format!("{}.0", tag.trim_start_matches('v').trim_end_matches(".0"));
    (!["v0.8.0", "v0.9", "v0.10"].contains(&tag)).then_some(cargo)
}

/// A copy of the fixture's world in a temporary folder, so saving never
/// touches the checked-in files.
fn copy_world(root: &Path, tag: &str, what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxelcraft-save-compat-{what}-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(root.join(tag).join("world")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
    dir
}

fn pos(v: &Value) -> IVec3 {
    let a: Vec<i32> = v.as_array().unwrap().iter().map(|n| n.as_i64().unwrap() as i32).collect();
    IVec3::new(a[0], a[1], a[2])
}

fn block_at(chunks: &SavedChunks, p: IVec3) -> Option<Block> {
    let chunk = chunks.get(&p.div_euclid(IVec3::splat(32)))?;
    let l = p.rem_euclid(IVec3::splat(32));
    Some(chunk.get(l.x as usize, l.y as usize, l.z as usize))
}

fn item(name: &str) -> Item {
    Item::from_name(name).unwrap_or_else(|| panic!("no item {name}"))
}

/// `name[,count]` as the fixture runs' `--give` and `--drop` take it.
fn named_stack(text: &str) -> (Item, u8) {
    let (name, count) = text.split_once(',').unwrap_or((text, "1"));
    (item(name), count.parse().unwrap())
}

fn holds(inventory: &Inventory, want: Item, count: u8) -> bool {
    inventory.slots.iter().flatten().any(|s| s.item == want && s.count == count)
}

/// Everything a fixture's level file holds that this build reads back,
/// in this build's own text: what a resave must keep.
fn reread(level: &LevelInfo, chunks: &SavedChunks) -> BTreeMap<&'static str, String> {
    let mut world = World::new_headless(Arc::new(Generator::new(level.seed)), chunks.clone(), 2);
    if let Some(c) = level.props.get("chests") {
        world.load_chests(c);
    }
    if let Some(f) = level.props.get("furnaces") {
        world.load_furnaces(f);
    }
    let mut entities = Entities::new(level.seed);
    entities.load_props(&level.props);
    let mut saved = BTreeMap::new();
    entities.save_props(&mut saved);
    let mut out = BTreeMap::new();
    let inventory = Inventory::deserialize(&level.props["inventory"]).expect("inventory");
    out.insert("inventory", inventory.serialize());
    // Containers are kept by position in a hash map: compare them sorted.
    let sorted = |text: String| {
        let mut entries: Vec<&str> = text.split('|').collect();
        entries.sort_unstable();
        entries.join("|")
    };
    out.insert("chests", sorted(world.chests_to_string()));
    out.insert("furnaces", sorted(world.furnaces_to_string()));
    out.insert("villagers", saved["villagers"].clone());
    out.insert("nether_mobs", saved["nether_mobs"].clone());
    out.insert("items", saved["items"].clone());
    out
}

#[test]
fn every_release_has_a_fixture_made_by_its_own_build() {
    for tag in RELEASES {
        let m = manifest(&checked_in(), tag);
        assert_eq!(m["tag"], tag);
        assert_eq!(m["commit"].as_str().map(str::len), Some(40), "{tag}: the commit the fixture was built from");
        assert_eq!(m["chest"]["filled"], true, "{tag}: the run's agent player filled the chest");
        assert!(m["reopen_loaded_chunks"].as_u64().is_some(), "{tag}: the run reopened its world once");
        let size: u64 = ["world/level.txt", "world/chunks.bin"]
            .iter()
            .map(|f| std::fs::metadata(checked_in().join(tag).join(f)).unwrap().len())
            .sum();
        assert!(size < 200_000, "{tag}: the fixture stays small ({size} bytes)");
    }
}

#[test]
fn every_release_fixture_opens_with_its_blocks_inventory_chest_player_and_entities() {
    for tag in RELEASES {
        check_opens(&checked_in(), tag, saving_version(tag).as_deref());
    }
}

#[test]
fn every_release_fixture_saves_and_reloads_without_loss() {
    for tag in RELEASES {
        check_resaves(&checked_in(), tag);
    }
}

/// v1.0 Track 3b: the first save with region storage moves a release's
/// `chunks.bin` and the level file's state for unloaded chunks into region
/// files, keeps the replaced files, and the world reopens from them with
/// its blocks, chest and entities.
#[test]
fn every_release_fixture_migrates_into_region_files_and_reopens() {
    for tag in RELEASES {
        check_migrates(&checked_in(), tag);
    }
}

/// UX-04a's Linux run of this build (`tools/save_fixtures/cycle-current.sh`
/// on Boat): the game started, built and saved a world, quit when its
/// window was closed, and opened and quit again. The world it saved passes
/// the same checks as the releases'. Needs `VC_SAVE_FIXTURES`, the folder
/// the script collected the run into.
#[test]
#[ignore = "needs a live run of the game; see tools/save_fixtures/cycle-current.sh"]
fn live_run_of_this_build_starts_saves_quits_and_reopens() {
    let root = PathBuf::from(std::env::var("VC_SAVE_FIXTURES").expect("VC_SAVE_FIXTURES"));
    let m = manifest(&root, "current");
    assert_eq!((m["create_exit"].as_u64(), m["reopen_exit"].as_u64()), (Some(0), Some(0)), "both runs quit cleanly");
    assert_eq!(m["chest"]["filled"], true);
    let saved = m["create_saved_chunks"].as_u64().expect("the first run saved on quit");
    assert_eq!(m["reopen_loaded_chunks"].as_u64(), Some(saved), "the reopened world loaded every saved chunk");
    assert!(m["reopen_saved_chunks"].as_u64().is_some(), "the reopened world saved on quit");
    check_opens(&root, "current", Some(env!("CARGO_PKG_VERSION")));
    check_resaves(&root, "current");
}

/// The world `tag`'s run saved loads with everything its manifest lists.
fn check_opens(root: &Path, tag: &str, saving_version: Option<&str>) {
    let m = manifest(root, tag);
    let dir = copy_world(root, tag, "open");
    let storage = Storage::new(&dir);
    assert!(storage.exists());
    let level = storage.load_level().unwrap_or_else(|e| panic!("{tag}: {e}"));
    assert_eq!(level.seed, 20261125, "{tag}");
    // From v0.11 every save names the version that wrote it.
    let version = level.props.get(VERSION_KEY).map(String::as_str);
    assert_eq!(version, saving_version, "{tag}: the saving version");

    // The player where the run left it.
    let (p, yaw, pitch) = level.player.expect("player");
    assert!(p.distance(DVec3::new(8.5, 152.0, 8.5)) < 1e-6, "{tag}: player at {p}");
    assert!((yaw - 45f32.to_radians()).abs() < 1e-4 && (pitch - 10f32.to_radians()).abs() < 1e-4, "{tag}");

    // Every block the run placed keeps its state: the id the tag gave
    // the name is the one saved, and this build gives the name that id.
    let chunks = storage.load_chunks().unwrap_or_else(|e| panic!("{tag}: {e}"));
    assert_eq!(chunks.len() as u64, m["chunks"]["kept"].as_u64().unwrap(), "{tag}");
    let placed = m["placed"].as_array().unwrap();
    assert!(placed.len() >= 60, "{tag}: {} blocks", placed.len());
    for b in placed {
        let (name, id, at) = (b["name"].as_str().unwrap(), b["id"].as_u64().unwrap() as u16, pos(&b["pos"]));
        assert_eq!(Block::from_name(name), Some(Block(id)), "{tag}: {name} still names state {id}");
        // A block may change state while the run plays (a daylight
        // detector's power follows the sun), never what block it is.
        let saved = block_at(&chunks, at).unwrap_or_else(|| panic!("{tag}: {name} at {at} not saved"));
        assert_eq!(saved.base(), Block(id).base(), "{tag}: {name} at {at} is {}", saved.name());
        if saved != Block(id) {
            eprintln!("{tag}: {name} placed as state {id} was saved as state {}", saved.0);
        }
    }
    for at in m["platform"].as_array().unwrap() {
        assert_eq!(block_at(&chunks, pos(at)), Some(Block::STONE), "{tag}");
    }
    let chest_at = pos(&m["chest"]["pos"]);
    assert_eq!(block_at(&chunks, chest_at).map(|b| b.base()), Some(Block::CHEST.base()), "{tag}");

    // The chest the agent player filled.
    let mut world = World::new_headless(Arc::new(Generator::new(level.seed)), chunks.clone(), 2);
    world.load_chests(&level.props["chests"]);
    let chest = world.chest(chest_at).unwrap_or_else(|| panic!("{tag}: no chest record"));
    for (slot, want) in m["chest"]["slots"].as_object().unwrap() {
        let want = Some(Stack::new(item(want[0].as_str().unwrap()), want[1].as_u64().unwrap() as u8));
        assert_eq!(chest.slots[slot.parse::<usize>().unwrap()], want, "{tag}: chest slot {slot}");
    }
    assert_eq!(chest.slots.iter().flatten().count(), 2, "{tag}");

    // The player's inventory and armour.
    let inventory = Inventory::deserialize(&level.props["inventory"]).unwrap_or_else(|| panic!("{tag}: inventory"));
    for g in m["give"].as_array().unwrap() {
        let (want, count) = named_stack(g.as_str().unwrap());
        assert!(holds(&inventory, want, count), "{tag}: holds {g}");
    }
    assert_eq!(inventory.armor[0].map(|s| s.item), Some(item("iron_helmet")), "{tag}: helmet");
    assert_eq!(inventory.armor[3].map(|s| s.item), Some(item("leather_boots")), "{tag}: boots");

    // The agent player's profile and what it holds.
    let agents: Value = serde_json::from_str(&level.props["agents"]).unwrap();
    let keeper = agents.as_array().unwrap().iter().find(|a| a["name"] == "keeper").expect("agent profile");
    let theirs = Inventory::deserialize(keeper["inventory"].as_str().unwrap()).unwrap();
    assert!(holds(&theirs, item("oak_log"), 20), "{tag}: the agent's logs");

    // The mobs the run spawned, and the dropped stack.
    let mut entities = Entities::new(level.seed);
    entities.load_props(&level.props);
    for s in m["spawn"].as_array().unwrap() {
        let parts: Vec<&str> = s.as_str().unwrap().split(',').collect();
        let kind = MobKind::from_name(parts[0]).unwrap();
        let x: f64 = parts[1].parse().unwrap();
        // Within 24 blocks of where it was spawned: mobs wander while the run plays.
        let near = entities.mobs.iter().any(|mob| mob.kind == kind && (mob.pos.x - x).hypot(mob.pos.z - 12.0) < 24.0);
        assert!(near, "{tag}: the {} spawned at x {x}", parts[0]);
    }
    for d in m["drop"].as_array().unwrap() {
        let (want, count) = named_stack(d.as_str().unwrap());
        assert!(
            entities.items.iter().any(|e| e.stack.item == want && e.stack.count == count),
            "{tag}: the dropped {d}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

/// The world `tag`'s run saved saves again with this build and reloads
/// without losing anything.
fn check_resaves(root: &Path, tag: &str) {
    let dir = copy_world(root, tag, "resave");
    let storage = Storage::new(&dir);
    let level = storage.load_level().unwrap();
    let chunks = storage.load_chunks().unwrap();
    let before = reread(&level, &chunks);

    // Save as this build does: every chunk and property, then read it all again.
    let list: Vec<(IVec3, Arc<ChunkData>)> = chunks.iter().map(|(p, c)| (*p, c.clone())).collect();
    storage.save(&level, &list).unwrap();
    let again = storage.load_level().unwrap();
    let reloaded = storage.load_chunks().unwrap();
    assert_eq!(again.seed, level.seed, "{tag}");
    assert_eq!(again.player, level.player, "{tag}");
    assert_eq!(again.props[VERSION_KEY], env!("CARGO_PKG_VERSION"), "{tag}: now saved by this version");
    let mut old = level.props.clone();
    let mut new = again.props.clone();
    old.remove(VERSION_KEY);
    new.remove(VERSION_KEY);
    assert_eq!(new, old, "{tag}: every property survives");
    assert_eq!(reloaded.len(), chunks.len(), "{tag}");
    for (p, c) in &chunks {
        let r = &reloaded[p];
        for i in 0..32 * 32 * 32 {
            let (x, y, z) = (i % 32, i / 32 % 32, i / 1024);
            assert_eq!(r.get(x, y, z), c.get(x, y, z), "{tag}: chunk {p} cell {x},{y},{z}");
        }
    }
    // The level file the save replaced is kept beside it.
    assert_eq!(storage.load_level_old().unwrap().props, level.props, "{tag}: level.txt_old");

    // What this build reads from the world is the same after the
    // resave, and once rewritten in this build's text it stays put.
    assert_eq!(reread(&again, &reloaded), before, "{tag}");
    let mut rewritten = again;
    for (key, text) in &before {
        rewritten.props.insert(key.to_string(), text.clone());
    }
    storage.save(&rewritten, &list).unwrap();
    let third = storage.load_level().unwrap();
    assert_eq!(reread(&third, &storage.load_chunks().unwrap()), before, "{tag}: stable in this build's text");
    std::fs::remove_dir_all(dir).unwrap();
}

/// The fixture opened as the game opens a world with region storage (the
/// level file is from before it, so it holds every chunk's state), saved
/// once, then opened again from what that save wrote.
fn check_migrates(root: &Path, tag: &str) {
    let m = manifest(root, tag);
    let dir = copy_world(root, tag, "migrate");
    let storage = Storage::new(&dir);
    let level = storage.load_level().unwrap();
    let chunks = storage.load_chunks().unwrap();
    let original_chunks = std::fs::read(dir.join("chunks.bin")).unwrap();
    let open = |props: &BTreeMap<String, String>, chunks: SavedChunks| {
        let mut world = World::new_headless(Arc::new(Generator::new(level.seed)), chunks, 1);
        let (store, damage) = RegionStore::open(dir.join(REGION_DIR), true).unwrap();
        assert!(damage.is_empty(), "{tag}");
        let owned = props.get(voxelcraft::world::REGION_KEY).map(|t| World::parse_region_chunks(t));
        world.attach_regions(store, owned, true);
        if let Some(c) = props.get("chests") {
            world.load_chests(c);
        }
        let mut entities = Entities::new(level.seed);
        entities.load_props(props);
        if let Some(t) = props.get("stored_index") {
            entities.load_stored_index(t);
        }
        entities.sync_chunk_entities(&mut world);
        (world, entities)
    };

    // The first save: nothing is loaded, so everything moves to records.
    let (mut world, mut entities) = open(&level.props, chunks.clone());
    let stored = entities.take_stored_batches(&world);
    assert!(!stored.is_empty(), "{tag}: the mobs and the dropped stack go to their chunks' records");
    world.save_records(stored);
    let mut props = level.props.clone();
    props.insert("chests".into(), world.chests_to_string());
    entities.save_props(&mut props);
    props.insert(voxelcraft::world::REGION_KEY.into(), world.region_chunks_to_string());
    props.insert("stored_index".into(), entities.stored_index_to_string());
    let saved = LevelInfo { seed: level.seed, player: level.player, props };
    let writer = Storage::new(&dir);
    world.regions().unwrap().after_writes(move || {
        writer.save_level(&saved)?;
        writer.retire_chunks_file()
    });
    world.flush_regions().unwrap();
    drop(world);
    assert!(!dir.join("chunks.bin").exists(), "{tag}: chunks.bin moved aside");
    assert_eq!(std::fs::read(dir.join("chunks.bin_old")).unwrap(), original_chunks, "{tag}: and kept unchanged");
    assert!(storage.has_regions(), "{tag}");
    assert_eq!(storage.load_level_old().unwrap().props, level.props, "{tag}: the old level file is kept");
    let migrated = storage.load_level().unwrap();
    assert_eq!(migrated.props["chests"], "", "{tag}: no chest is left in the level file");
    assert_eq!(migrated.props[voxelcraft::world::REGION_KEY], "", "{tag}: the level file holds no chunk");
    assert!(storage.load_chunks().unwrap().is_empty(), "{tag}");

    // Reopened: the chunks load from their records with everything in them.
    let (mut world, mut entities) = open(&migrated.props, Default::default());
    let mut columns: Vec<(i32, i32)> = chunks.keys().map(|c| (c.x, c.z)).collect();
    columns.extend((-2..=2).flat_map(|x| (-1..=2).map(move |z| (x, z))));
    columns.sort_unstable();
    columns.dedup();
    for (x, z) in columns {
        world.load_test_column(glam::IVec2::new(x, z), 60);
    }
    entities.sync_chunk_entities(&mut world);
    for (p, c) in &chunks {
        for i in (0..32 * 32 * 32).step_by(7) {
            let (x, y, z) = (i % 32, i / 1024, i / 32 % 32);
            let at = *p * 32 + IVec3::new(x as i32, y as i32, z as i32);
            assert_eq!(world.get_block(at), Some(c.get(x, y, z)), "{tag}: {at}");
        }
    }
    let chest_at = pos(&m["chest"]["pos"]);
    let chest = world.chest(chest_at).unwrap_or_else(|| panic!("{tag}: the chest came back"));
    for (slot, want) in m["chest"]["slots"].as_object().unwrap() {
        let want = Some(Stack::new(item(want[0].as_str().unwrap()), want[1].as_u64().unwrap() as u8));
        assert_eq!(chest.slots[slot.parse::<usize>().unwrap()], want, "{tag}: chest slot {slot}");
    }
    for s in m["spawn"].as_array().unwrap() {
        let parts: Vec<&str> = s.as_str().unwrap().split(',').collect();
        let kind = MobKind::from_name(parts[0]).unwrap();
        let x: f64 = parts[1].parse().unwrap();
        let near = entities.mobs.iter().any(|mob| mob.kind == kind && (mob.pos.x - x).hypot(mob.pos.z - 12.0) < 24.0);
        assert!(near, "{tag}: the {} spawned at x {x} came back", parts[0]);
    }
    for d in m["drop"].as_array().unwrap() {
        let (want, count) = named_stack(d.as_str().unwrap());
        assert!(
            entities.items.iter().any(|e| e.stack.item == want && e.stack.count == count),
            "{tag}: the dropped {d} came back"
        );
    }
    drop(world);
    std::fs::remove_dir_all(dir).unwrap();
}
