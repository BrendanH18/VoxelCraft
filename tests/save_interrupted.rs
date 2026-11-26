//! Interrupted and damaged saves (v1.0 Track 1, row UX-05b): a save cut
//! off mid-write, a level file lost or cut off, and a `chunks.bin` cut off
//! or corrupted each get the recovery `docs/saves.md` documents. None of
//! them opens as a fresh world or quietly drops what could still be read.
//! The game's screens and notices for these cases are tested beside them
//! (`app::title::tests`, `app::dimension`, `app::unreadable_levels_are_not_opened`).

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use glam::IVec3;
use voxelcraft::world::block::Block;
use voxelcraft::world::chunk::ChunkData;
use voxelcraft::world::storage::{LEVEL_FILE, LEVEL_OLD, LevelInfo, Storage};

fn temp(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxelcraft-save-interrupted-{what}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn level(name: &str) -> LevelInfo {
    let props: BTreeMap<String, String> =
        [("name".to_string(), name.to_string()), ("inventory".to_string(), format!("{name} things"))].into();
    LevelInfo { seed: 77, player: None, props }
}

/// Three chunks, each a different block, so a lost one shows.
fn chunks(top: Block) -> Vec<(IVec3, Arc<ChunkData>)> {
    let mut middle = ChunkData::Uniform(Block::DIRT);
    middle.set(1, 2, 3, Block::GLASS);
    vec![
        (IVec3::new(0, 0, 0), Arc::new(ChunkData::Uniform(Block::STONE))),
        (IVec3::new(1, 0, 0), Arc::new(middle)),
        (IVec3::new(2, 0, 0), Arc::new(ChunkData::Uniform(top))),
    ]
}

fn files(dir: &PathBuf, prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.unwrap().file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix))
        .collect();
    out.sort();
    out
}

#[test]
fn a_save_cut_off_mid_write_leaves_the_last_complete_save() {
    let dir = temp("mid-write");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    // The next save stopped while writing its temporary files: neither
    // rename happened, so the last save is untouched.
    std::fs::write(dir.join("chunks.bin.tmp"), b"VXC2\x03\x00").unwrap();
    std::fs::write(dir.join("level.txt.tmp"), "seed=77\nname=sec").unwrap();
    assert_eq!(storage.load_level().unwrap().props["name"], "first");
    let loaded = storage.load_chunks_recovering().unwrap();
    assert!(loaded.1.is_none(), "nothing is damaged");
    assert_eq!(loaded.0[&IVec3::new(2, 0, 0)].uniform(), Some(Block::GOLD_BLOCK));
    // The next save writes over the leftovers.
    storage.save(&level("second"), &chunks(Block::IRON_BLOCK)).unwrap();
    assert_eq!(storage.load_level().unwrap().props["name"], "second");
    assert_eq!(storage.load_chunks().unwrap()[&IVec3::new(2, 0, 0)].uniform(), Some(Block::IRON_BLOCK));
    assert_eq!(files(&dir, "").iter().filter(|n| n.ends_with(".tmp")).count(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn each_save_keeps_the_level_file_it_replaces() {
    let dir = temp("level-old");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    assert!(!dir.join(LEVEL_OLD).exists(), "nothing to keep on the first save");
    storage.save(&level("second"), &chunks(Block::GOLD_BLOCK)).unwrap();
    assert_eq!(storage.load_level_old().unwrap().props["name"], "first");
    storage.save_level(&level("third")).unwrap();
    assert_eq!(storage.load_level_old().unwrap().props["name"], "second");
    storage.set_props(&[("name", "renamed")]).unwrap();
    assert_eq!(storage.load_level().unwrap().props["name"], "renamed");
    assert_eq!(storage.load_level_old().unwrap().props["name"], "third");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_crash_between_the_level_renames_restores_from_the_kept_copy() {
    let dir = temp("between-renames");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    storage.save(&level("second"), &chunks(Block::GOLD_BLOCK)).unwrap();
    // A third save wrote chunks.bin and its temporary level file, moved
    // level.txt ("second") to level.txt_old, and stopped before the last
    // rename: after the whole save, put "third" back where it was then.
    storage.save(&level("third"), &chunks(Block::IRON_BLOCK)).unwrap();
    std::fs::rename(dir.join(LEVEL_FILE), dir.join("level.txt.tmp")).unwrap();

    // Still a world, not an empty folder a new world could take.
    assert!(storage.exists());
    let e = storage.load_level().err().expect("an error");
    assert_eq!((e.kind(), e.to_string().as_str()), (ErrorKind::NotFound, "level.txt is missing"));
    assert_eq!(storage.load_level_old().unwrap().props["name"], "second");
    assert_eq!(storage.restore_level_from_old("2026-11-25_12.00.00").unwrap(), None, "no unreadable file to move");
    assert_eq!(storage.load_level().unwrap().props["name"], "second");
    assert!(!dir.join(LEVEL_OLD).exists(), "Java moves the kept copy into place");
    // The chunks are the newer save's.
    assert_eq!(storage.load_chunks().unwrap()[&IVec3::new(2, 0, 0)].uniform(), Some(Block::IRON_BLOCK));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_cut_off_or_empty_level_file_is_refused_then_restored_from_its_kept_copy() {
    let dir = temp("cut-level");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    storage.save(&level("second"), &chunks(Block::GOLD_BLOCK)).unwrap();
    let text = std::fs::read_to_string(dir.join(LEVEL_FILE)).unwrap();
    for (damaged, why) in [(&text[..text.len() - 4], "level.txt is cut off"), ("", "level.txt is empty")] {
        std::fs::write(dir.join(LEVEL_FILE), damaged).unwrap();
        let e = storage.load_level().err().expect("an error");
        assert_eq!((e.kind(), e.to_string()), (ErrorKind::InvalidData, why.to_string()));
    }
    std::fs::write(dir.join(LEVEL_FILE), &text[..text.len() - 4]).unwrap();
    let kept = storage.restore_level_from_old("2026-11-25_12.00.00").unwrap().unwrap();
    assert_eq!(kept, dir.join("level.txt_corrupted_2026-11-25_12.00.00"));
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), &text[..text.len() - 4], "the damaged file is kept as it was");
    assert_eq!(storage.load_level().unwrap().props["name"], "first");
    // A second restore in the same second never replaces the first's file.
    storage.save_level(&level("third")).unwrap();
    std::fs::write(dir.join(LEVEL_FILE), "seed=junk\n").unwrap();
    let again = storage.restore_level_from_old("2026-11-25_12.00.00").unwrap().unwrap();
    assert_eq!(again, dir.join("level.txt_corrupted_2026-11-25_12.00.00-1"));
    assert_eq!(files(&dir, "level.txt_corrupted_").len(), 2);
    // With no readable kept copy there is nothing to restore, and nothing moves.
    std::fs::write(dir.join(LEVEL_FILE), "seed=junk\n").unwrap();
    assert!(storage.restore_level_from_old("later").is_err());
    assert_eq!(std::fs::read_to_string(dir.join(LEVEL_FILE)).unwrap(), "seed=junk\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_level_file_beside_saved_chunks_is_not_an_empty_folder() {
    let dir = temp("missing-level");
    let storage = Storage::new(&dir);
    assert!(!storage.exists() && !storage.has_chunks_without_level(), "an empty folder is a new world");
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    std::fs::remove_file(dir.join(LEVEL_FILE)).unwrap();
    assert!(!storage.exists());
    assert!(storage.has_chunks_without_level(), "the game refuses it rather than starting over it");
    assert!(storage.restore_level_from_old("x").is_err(), "nothing kept to restore from");
    assert_eq!(storage.load_chunks().unwrap().len(), 3, "the chunks are untouched");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_truncated_chunks_file_keeps_every_intact_chunk_and_a_copy_of_the_damage() {
    let dir = temp("cut-chunks");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    let whole = std::fs::read(dir.join("chunks.bin")).unwrap();
    // The third record's runs are the last 4 bytes; its header the 16 before.
    let cases: [(usize, usize, &str); 5] = [
        (whole.len() - 1, 2, "it is cut off"),
        (whole.len() - 4 - 10, 2, "it is cut off"),
        (10, 0, "it is cut off"),
        (6, 0, "it ends inside its header"),
        (0, 0, "its header is unreadable"),
    ];
    for (keep, recovered, reason) in cases {
        let damaged = &whole[..keep];
        std::fs::write(dir.join("chunks.bin"), damaged).unwrap();
        assert_eq!(
            storage.load_chunks().err().expect("an error").kind(),
            ErrorKind::InvalidData,
            "the strict read refuses it"
        );
        let (loaded, damage) = storage.load_chunks_recovering().unwrap();
        let damage = damage.expect("damage is reported");
        assert_eq!((damage.recovered, damage.reason.as_str()), (recovered, reason), "cut at {keep}");
        assert_eq!(loaded.len(), recovered);
        if recovered == 2 {
            assert_eq!(damage.saved, 3);
            assert_eq!(loaded[&IVec3::new(0, 0, 0)].uniform(), Some(Block::STONE));
            assert_eq!(loaded[&IVec3::new(1, 0, 0)].get(1, 2, 3), Block::GLASS);
        }
        let kept = damage.kept_as.expect("a copy is kept");
        assert!(kept.file_name().unwrap().to_str().unwrap().starts_with("chunks.bin.damaged-"));
        assert_eq!(std::fs::read(&kept).unwrap(), damaged, "the copy is the damaged file as found");
        assert_eq!(std::fs::read(dir.join("chunks.bin")).unwrap(), damaged, "loading changes nothing");
        // The game's next save writes what was recovered; the copy stays.
        let list: Vec<_> = loaded.into_iter().collect();
        storage.save(&level("after"), &list).unwrap();
        assert_eq!(storage.load_chunks().unwrap().len(), recovered);
        assert!(kept.exists());
        assert!(files(&dir, "chunks.bin.unknown-").is_empty(), "damage isn't mistaken for a newer format");
    }
    assert_eq!(files(&dir, "chunks.bin.damaged-").len(), 5, "one copy per damaged load");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_malformed_chunk_record_is_skipped_and_the_rest_load() {
    let dir = temp("bad-record");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    let mut bin = std::fs::read(dir.join("chunks.bin")).unwrap();
    // The first record (the uniform stone chunk) is one run: make it a
    // zero-length run, which no chunk can be.
    let run = 8 + 16;
    bin[run..run + 2].copy_from_slice(&0u16.to_le_bytes());
    std::fs::write(dir.join("chunks.bin"), &bin).unwrap();
    let (loaded, damage) = storage.load_chunks_recovering().unwrap();
    let damage = damage.unwrap();
    assert_eq!((damage.saved, damage.recovered, damage.reason.as_str()), (3, 2, "a chunk record is malformed"));
    assert!(!loaded.contains_key(&IVec3::new(0, 0, 0)), "that chunk regenerates");
    assert_eq!(loaded[&IVec3::new(2, 0, 0)].uniform(), Some(Block::GOLD_BLOCK));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_newer_versions_chunks_are_kept_not_treated_as_damage() {
    let dir = temp("newer-chunks");
    let storage = Storage::new(&dir);
    storage.save(&level("first"), &chunks(Block::GOLD_BLOCK)).unwrap();
    let mut bin = std::fs::read(dir.join("chunks.bin")).unwrap();
    bin[..4].copy_from_slice(b"VXC9");
    std::fs::write(dir.join("chunks.bin"), &bin).unwrap();
    assert_eq!(storage.load_chunks_recovering().err().expect("an error").kind(), ErrorKind::InvalidData);
    assert!(files(&dir, "chunks.bin.damaged-").is_empty());
    storage.save(&level("after"), &[]).unwrap();
    let kept = files(&dir, "chunks.bin.unknown-");
    assert_eq!(kept.len(), 1, "the newer build's file is moved aside, not overwritten");
    assert_eq!(std::fs::read(dir.join(&kept[0])).unwrap(), bin);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unknown_block_states_are_kept_and_unknown_items_emptied() {
    // A state id this build doesn't register (the integrator's reserved
    // range, docs/architecture.md) and an item id past every registered one.
    let unknown = Block(14_335);
    assert_eq!((unknown.name(), unknown.is_solid()), ("unknown", false), "a placeholder");
    let dir = temp("unknown-ids");
    let storage = Storage::new(&dir);
    let mut chunk = ChunkData::Uniform(Block::STONE);
    chunk.set(4, 5, 6, unknown);
    let mut props = level("ids").props;
    let inventory =
        format!("2199:3,{}:5{}", voxelcraft::item::Item::DIAMOND.0, ",-".repeat(voxelcraft::inventory::SLOTS - 2));
    props.insert("inventory".into(), inventory);
    storage.save(&LevelInfo { seed: 1, player: None, props }, &[(IVec3::ZERO, Arc::new(chunk))]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    assert_eq!(loaded[&IVec3::ZERO].get(4, 5, 6), unknown, "the raw state id is kept");
    let list: Vec<_> = loaded.into_iter().collect();
    storage.save(&storage.load_level().unwrap(), &list).unwrap();
    assert_eq!(storage.load_chunks().unwrap()[&IVec3::ZERO].get(4, 5, 6), unknown, "and saved again unchanged");
    // Java's `ItemStack.OPTIONAL_CODEC`: an item it can't read is an empty slot.
    let level = storage.load_level().unwrap();
    let parsed = voxelcraft::inventory::Inventory::deserialize(&level.props["inventory"]).unwrap();
    assert_eq!(parsed.slots[0], None, "the unknown item's slot is empty");
    assert_eq!(parsed.slots[1].map(|s| s.count), Some(5), "the rest of the inventory loads");
    std::fs::remove_dir_all(dir).unwrap();
}

/// v1.0 Track 3b: a region file cut off (a crash or a full disk while the
/// I/O thread wrote it) keeps every chunk whose record is whole; the rest
/// regenerate, the player is told how many, and the damaged file is kept,
/// as `chunks.bin` damage is handled. Writing again repairs the table.
#[test]
fn a_cut_off_region_file_keeps_every_intact_chunk_and_a_copy_of_the_damage() {
    use voxelcraft::world::region::{ChunkRecord, Patch, RegionStore, SECTOR};
    let dir = temp("cut-region");
    let regions = dir.join("region");
    let record = |b: Block| ChunkRecord {
        blocks: Some(Arc::new(ChunkData::Uniform(b))),
        state: vec![("spawners".to_string(), "1,2,3=zombie".to_string())],
        ..Default::default()
    };
    let blocks = [Block::STONE, Block::DIRT, Block::GOLD_BLOCK];
    {
        let (store, _) = RegionStore::open(&regions, true).unwrap();
        // A big middle record, so cuts land inside it and inside the last.
        let mut big = record(blocks[1]);
        big.entities.push(vec![("items".to_string(), "x".repeat(3 * SECTOR as usize))]);
        store.queue([
            (IVec3::new(0, 0, 0), Patch::replace(record(blocks[0]))),
            (IVec3::new(1, 0, 0), Patch::replace(big)),
            (IVec3::new(2, 0, 0), Patch::replace(record(blocks[2]))),
        ]);
        store.flush().unwrap();
    }
    let path = regions.join("r.0.0.vxr");
    let whole = std::fs::read(&path).unwrap();
    let header = 9 * SECTOR as usize;
    // Records follow the table in the order they were written: 1 sector,
    // 4 sectors, 1 sector.
    let cases: [(usize, usize, &str); 4] = [
        (whole.len() - SECTOR as usize + 20, 2, "it is cut off"),
        (header + SECTOR as usize + 100, 1, "it is cut off"),
        (header - 100, 0, "it is cut off"),
        (6, 0, "it is cut off"),
    ];
    for (keep, kept, reason) in cases {
        let damaged = &whole[..keep];
        std::fs::write(&path, damaged).unwrap();
        let (store, damage) = RegionStore::open(&regions, true).unwrap();
        assert_eq!(damage.len(), 1, "cut at {keep}");
        let d = &damage[0];
        assert_eq!((d.kept, d.reason), (kept, reason), "cut at {keep}");
        assert!(d.saved == 3 || keep < header, "the table listed three chunks");
        let notice = d.notice("overworld");
        assert!(notice.contains("regenerate") && notice.contains("r.0.0.vxr"), "{notice}");
        let copy = d.kept_as.clone().expect("a copy is kept");
        assert!(copy.file_name().unwrap().to_str().unwrap().starts_with("r.0.0.vxr.damaged-"));
        assert_eq!(std::fs::read(&copy).unwrap(), damaged, "the copy is the damaged file as found");
        assert_eq!(std::fs::read(&path).unwrap(), damaged, "opening changes nothing");
        let read: Vec<Option<Block>> =
            (0..3).map(|x| store.read(IVec3::new(x, 0, 0)).and_then(|r| r.blocks?.uniform())).collect();
        let whole_ones = read.iter().filter(|b| b.is_some()).count();
        assert_eq!(whole_ones, kept, "cut at {keep}: {read:?}");
        for (x, b) in read.iter().enumerate() {
            assert!(b.is_none() || *b == Some(blocks[x]), "an intact chunk reads as it was");
        }
        // The game writes the lost chunks again (here, all three): the file
        // is whole afterwards.
        store.queue((0..3).map(|x| (IVec3::new(x, 0, 0), Patch::replace(record(blocks[x as usize])))));
        store.flush().unwrap();
        drop(store);
        let (store, damage) = RegionStore::open(&regions, true).unwrap();
        assert!(damage.is_empty(), "cut at {keep}: repaired");
        for x in 0..3 {
            let r = store.read(IVec3::new(x, 0, 0)).unwrap();
            assert_eq!(r.blocks.unwrap().uniform(), Some(blocks[x as usize]));
            assert_eq!(r.state, record(Block::AIR).state);
        }
    }
    let copies = std::fs::read_dir(&regions)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("r.0.0.vxr.damaged-"))
        .count();
    assert_eq!(copies, 4, "one copy per damaged open");
    std::fs::remove_dir_all(dir).unwrap();
}
