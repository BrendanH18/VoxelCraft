//! Chunk record tests (v1.0 Track 3b): what a chunk owns leaves memory with
//! it and comes back when it loads.
use super::*;
use crate::entity::MobKind;
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::block::Block;
use crate::world::terrain::Generator;
use glam::IVec2;
use std::sync::Arc;

const GROUND: i32 = 64;

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("voxelcraft-records-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn world(dir: &std::path::Path, owned: Option<FxHashSet<IVec3>>) -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
    let (store, damage) = RegionStore::open(dir, true).unwrap();
    assert!(damage.is_empty());
    w.attach_regions(store, owned, 0, true);
    w
}

fn count(w: &World, name: &str) -> usize {
    w.diagnostics().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no {name}")).1
}

/// A chest with diamonds, a spawner, a furnace, a sign, flowing water and
/// a fire, all in column x = 1.
fn furnish(w: &mut World) -> (IVec3, IVec3, IVec3) {
    let chest = IVec3::new(40, GROUND + 1, 5);
    assert!(w.set_block(chest, Block::CHEST));
    w.chest_mut(chest).unwrap().slots[3] = Some(Stack::new(Item::DIAMOND, 9));
    let spawner = IVec3::new(44, GROUND + 1, 5);
    w.set_block(spawner, Block::SPAWNER);
    w.set_spawner(spawner, MobKind::Zombie);
    assert!(w.set_block(IVec3::new(48, GROUND + 1, 5), Block::FURNACE));
    let water = IVec3::new(52, GROUND + 1, 9);
    w.set_block(water, Block::WATER);
    let fire = IVec3::new(56, GROUND + 1, 9);
    w.set_block(fire, Block::FIRE);
    (chest, spawner, water)
}

#[test]
fn an_unloading_chunk_takes_what_it_owns_to_its_record_and_gets_it_back() {
    let dir = temp("unload");
    let mut w = world(&dir, Some(FxHashSet::default()));
    w.load_test_column(IVec2::new(0, 0), GROUND);
    w.load_test_column(IVec2::new(1, 0), GROUND);
    let (chest, spawner, water) = furnish(&mut w);
    let (chests, spawners, furnaces) = (w.chests_to_string(), w.spawners_to_string(), w.furnaces_to_string());
    assert!(count(&w, "fluid.pending") > 0 && count(&w, "fire.scheduled") > 0);

    w.unload_test_column(IVec2::new(1, 0));
    // Nothing of the column stays in memory.
    assert_eq!(count(&w, "chests"), 0);
    assert_eq!(count(&w, "spawners"), 0);
    assert_eq!(count(&w, "furnaces"), 0);
    assert_eq!(count(&w, "chunks.saved_unloaded"), 0, "the blocks are in the record, not in memory");
    assert_eq!(count(&w, "fluid.pending") + count(&w, "fluid.suspended"), 0);
    assert_eq!(count(&w, "fire.scheduled"), 0);
    assert_eq!(w.get_block(chest), None);
    w.flush_regions().unwrap();

    w.load_test_column(IVec2::new(1, 0), GROUND);
    assert_eq!(w.get_block(chest), Some(Block::CHEST));
    assert_eq!(w.chests_to_string(), chests);
    assert_eq!(w.spawners_to_string(), spawners);
    assert_eq!(w.furnaces_to_string(), furnaces);
    assert_eq!(w.chest(chest).unwrap().slots[3], Some(Stack::new(Item::DIAMOND, 9)));
    assert_eq!(w.get_block(spawner), Some(Block::SPAWNER));
    assert!(w.fluid_ticks_to_string().contains(&format!("w{},{},{}", water.x, water.y, water.z)), "fluid ticks too");
    assert!(count(&w, "fire.scheduled") > 0, "the fire's tick came back");
    drop(w);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_chunk_the_level_file_holds_ignores_its_records_older_state() {
    let dir = temp("owned");
    let mut w = world(&dir, Some(FxHashSet::default()));
    w.load_test_column(IVec2::new(1, 0), GROUND);
    let (chest, ..) = furnish(&mut w);
    w.unload_test_column(IVec2::new(1, 0));
    drop(w);
    // The level file names the chest's chunk: it holds that chunk's state
    // (here: an emptied chest), and the record's state is older.
    let owned: FxHashSet<IVec3> = [chunk_of(chest)].into_iter().collect();
    let mut w = world(&dir, Some(owned));
    let empty = crate::world::chest::Chest::default().serialize();
    w.load_chests(&format!("{},{},{}={empty}", chest.x, chest.y, chest.z));
    w.load_test_column(IVec2::new(1, 0), GROUND);
    assert_eq!(w.get_block(chest), Some(Block::CHEST), "the blocks still come from the record");
    assert!(w.chest(chest).unwrap().slots.iter().all(Option::is_none), "the level file's chest wins");
    assert_eq!(count(&w, "spawners"), 0, "the record's spawner state is older than the level file");
    // Once it unloads, its record holds it all again.
    w.unload_test_column(IVec2::new(1, 0));
    assert_eq!(w.region_chunks_to_string(), "");
    w.load_test_column(IVec2::new(1, 0), GROUND);
    assert!(w.chest(chest).unwrap().slots.iter().all(Option::is_none));
    drop(w);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_first_save_moves_the_level_files_unloaded_state_into_records() {
    let dir = temp("migrate");
    // A world from before region storage: chests in the level file for an
    // unloaded chunk, and a saved chunk from `chunks.bin`.
    let far = IVec3::new(200, GROUND + 1, 200);
    let mut saved: crate::world::storage::SavedChunks = Default::default();
    let mut data = World::flat_test_chunk(2, GROUND);
    let l = far.rem_euclid(IVec3::splat(32));
    Arc::make_mut(&mut data).set(l.x as usize, l.y as usize, l.z as usize, Block::CHEST);
    saved.insert(chunk_of(far), data);
    let mut w = World::new_headless(Arc::new(Generator::new(3)), saved, 2);
    let (store, _) = RegionStore::open(&dir, true).unwrap();
    w.attach_regions(store, None, 0, true);
    let mut held = crate::world::chest::Chest::default();
    held.slots[0] = Some(Stack::new(Item::DIAMOND, 5));
    w.load_chests(&format!("{},{},{}={}", far.x, far.y, far.z, held.serialize()));
    let text = w.chests_to_string();
    assert!(!text.is_empty());
    w.load_test_column(IVec2::new(0, 0), GROUND);
    assert!(w.keeps_entities(IVec3::new(9, 2, 9)), "the level file holds every chunk until the first save");
    let queued = w.save_records(Vec::new());
    assert!(queued >= 2, "{queued}");
    assert_eq!(w.chests_to_string(), "", "the unloaded chest left memory");
    assert_eq!(count(&w, "chunks.saved_unloaded"), 0);
    assert!(!w.keeps_entities(IVec3::new(9, 2, 9)));
    let owned = World::parse_region_chunks(&w.region_chunks_to_string());
    assert_eq!(owned.len(), w.generator.dimension.column_chunks() as usize, "the loaded column");
    w.flush_regions().unwrap();
    drop(w);
    // Reopened from the regions: the chest and its chunk are there.
    let mut w = world(&dir, Some(owned));
    w.load_test_column(IVec2::new(far.x.div_euclid(32), far.z.div_euclid(32)), GROUND);
    assert_eq!(w.get_block(far), Some(Block::CHEST));
    assert_eq!(w.chests_to_string(), text);
    drop(w);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Neon review N1: a generated chunk loading from its record gets its
/// structure chest from the record, and its loot isn't rolled again
/// (rolling would advance the loot sequence, number new explorer maps and
/// claim new chest identities on every reload).
#[test]
fn a_generated_chunk_reloading_from_its_record_rolls_no_structure_loot_again() {
    let dir = temp("features");
    let mut w = world(&dir, Some(FxHashSet::default()));
    let chest = IVec3::new(40, GROUND + 1, 5);
    let pos = chunk_of(chest);
    let generated = || {
        let mut data = Arc::unwrap_or_clone(World::flat_test_chunk(pos.y, GROUND));
        let l = chest.rem_euclid(IVec3::splat(32));
        data.set(l.x as usize, l.y as usize, l.z as usize, Block::CHEST);
        // Seed 0: the table's world loot sequence rolls it.
        let features = vec![(chest, crate::world::fortress::Feature::DungeonChest(0))];
        crate::world::GeneratedChunk {
            data,
            features,
            fluid_ticks: Vec::new(),
            bubble_seeds: Vec::new(),
            rolled: Vec::new(),
        }
    };
    w.insert_generated(pos, generated(), None);
    assert!(w.chest(chest).unwrap().slots.iter().any(Option::is_some), "the dungeon's loot");
    // The player empties it, and the chunk unloads with it in its record.
    w.chest_mut(chest).unwrap().slots.fill(None);
    let (sequences, identities) = (w.loot_sequences.serialize(), w.next_chest_identity);
    w.unload_test_column(IVec2::new(pos.x, pos.z));
    assert!(w.chest(chest).is_none());
    let record = w.regions().unwrap().read(pos);
    assert!(record.as_ref().is_some_and(|r| r.blocks.is_none()), "the blocks regenerate");
    w.insert_generated(pos, generated(), record);
    assert!(w.chest(chest).unwrap().slots.iter().all(Option::is_none), "still empty");
    assert_eq!(w.loot_sequences.serialize(), sequences, "nothing rolled again");
    assert_eq!(w.next_chest_identity, identities + 1, "one identity, the restored chest's");
    drop(w);
    std::fs::remove_dir_all(dir).unwrap();
}
