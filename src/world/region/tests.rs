//! Region file tests (v1.0 Track 3b).
use super::*;
use crate::world::block::Block;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("voxelcraft-region-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn props(pairs: &[(&str, &str)]) -> Props {
    pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect()
}

fn chunk(fill: Block, marks: &[(usize, Block)]) -> Arc<ChunkData> {
    let mut data = ChunkData::Uniform(fill);
    for &(i, b) in marks {
        data.set(i % 32, i / 1024, i / 32 % 32, b);
    }
    Arc::new(data)
}

fn full(pos: IVec3) -> ChunkRecord {
    ChunkRecord {
        blocks: Some(chunk(Block::STONE, &[(pos.x.rem_euclid(1000) as usize, Block::GLASS), (9000, Block::CHEST)])),
        state: props(&[("chests", "1,2,3=xyz"), ("sculk", "[[[1,2,3],{\"k\":1}]]")]),
        water: vec![IVec3::new(1, 2, 3), IVec3::new(4, 5, 6)],
        lava: vec![IVec3::new(-7, 8, 9)],
        bubbles: vec![],
        fires: vec![(IVec3::new(10, 11, 12), 33)],
        entities: vec![props(&[("items", "a;b"), ("villagers", "{\"mobs\":[]}")]), props(&[("orbs", "o")])],
    }
}

fn same(a: &ChunkRecord, b: &ChunkRecord) {
    match (&a.blocks, &b.blocks) {
        (Some(x), Some(y)) => {
            for i in 0..32 * 32 * 32 {
                let (cx, cy, cz) = (i % 32, i / 1024, i / 32 % 32);
                assert_eq!(x.get(cx, cy, cz), y.get(cx, cy, cz), "cell {i}");
            }
        }
        (None, None) => {}
        _ => panic!("blocks differ in presence"),
    }
    assert_eq!(a.state, b.state);
    assert_eq!((&a.water, &a.lava, &a.bubbles), (&b.water, &b.lava, &b.bubbles));
    assert_eq!(a.fires, b.fires);
    assert_eq!(a.entities, b.entities);
}

#[test]
fn records_round_trip_and_reject_damage() {
    let pos = IVec3::new(-17, -2, 33);
    let record = full(pos);
    let payload = encode(pos, &record);
    same(&decode(pos, &payload).unwrap(), &record);
    assert_eq!(decode(IVec3::new(-17, -1, 33), &payload).err(), Some("a chunk record is in the wrong place"));
    for cut in [0, 1, 13, payload.len() / 2, payload.len() - 1] {
        assert!(decode(pos, &payload[..cut]).is_err(), "cut at {cut}");
    }
    let mut longer = payload.clone();
    longer.push(0);
    assert!(decode(pos, &longer).is_err(), "trailing bytes");
    let mut newer = payload;
    newer[0] = RECORD_VERSION + 1;
    assert_eq!(decode(pos, &newer).err(), Some("it is from a newer version"));
    // A record without blocks regenerates them.
    let state_only = ChunkRecord { blocks: None, ..full(pos) };
    same(&decode(pos, &encode(pos, &state_only)).unwrap(), &state_only);
    assert!(ChunkRecord::default().is_empty());
    assert!(!state_only.is_empty());
}

#[test]
fn every_row_of_every_dimension_has_its_own_slot() {
    use crate::world::terrain::Dimension;
    for dim in [Dimension::Overworld, Dimension::Nether, Dimension::End] {
        let mut seen = FxHashSet::default();
        for cy in dim.chunk_rows() {
            for cz in -16..32 {
                for cx in -16..32 {
                    let pos = IVec3::new(cx, cy, cz);
                    let region = region_of(pos);
                    assert!(seen.insert((region, slot_of(pos))), "{dim:?} {pos} shares a slot");
                    assert_eq!(slot_pos(region, slot_of(pos)), pos, "{dim:?}");
                }
            }
        }
    }
}

#[test]
fn one_chunk_rewrites_without_touching_the_others_and_sectors_are_reused() {
    let dir = temp("rewrite");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(file_name(IVec2::ZERO));
    let (mut file, damage) = RegionFile::open(&path, true).unwrap().unwrap();
    assert!(damage.is_none());
    let positions: Vec<IVec3> = (0..8).map(|i| IVec3::new(i, 2, i * 2)).collect();
    for &p in &positions {
        file.write(slot_of(p), Some(&encode(p, &full(p)))).unwrap();
    }
    let len = fs::metadata(&path).unwrap().len();
    // Rewriting one chunk many times reuses its sectors: the file doesn't grow.
    for _ in 0..20 {
        let p = positions[3];
        file.write(slot_of(p), Some(&encode(p, &full(p)))).unwrap();
    }
    assert!(fs::metadata(&path).unwrap().len() <= len + 2 * SECTOR, "rewrites reuse freed sectors");
    // A bigger record moves; removing one frees its slot.
    let big = ChunkRecord { entities: vec![props(&[("items", &"x".repeat(20_000))])], ..full(positions[1]) };
    file.write(slot_of(positions[1]), Some(&encode(positions[1], &big))).unwrap();
    file.write(slot_of(positions[5]), None).unwrap();
    drop(file);
    let (mut file, damage) = RegionFile::open(&path, false).unwrap().unwrap();
    assert!(damage.is_none(), "every entry is whole");
    for &p in &positions {
        let read = file.read(slot_of(p)).unwrap();
        match p {
            p if p == positions[5] => assert!(read.is_none()),
            p if p == positions[1] => same(&decode(p, &read.unwrap()).unwrap(), &big),
            p => same(&decode(p, &read.unwrap()).unwrap(), &full(p)),
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn patches_merge_in_order_and_partial_ones_keep_the_rest() {
    let pos = IVec3::new(1, 3, 1);
    let mut record = full(pos);
    // Blocks only (an autosave of a loaded chunk): state and entities stay.
    let blocks = chunk(Block::DIRT, &[]);
    Patch { blocks: Some(Some(blocks.clone())), ..Default::default() }.apply(&mut record);
    assert_eq!(record.blocks.as_ref().unwrap().get(0, 0, 0), Block::DIRT);
    assert_eq!(record.state, full(pos).state);
    assert_eq!(record.entities.len(), 2);
    // Unloading replaces everything but appends; entities stored after it join.
    let mut unload = Patch::replace(ChunkRecord { blocks: None, ..Default::default() });
    unload.then(Patch { append: vec![props(&[("items", "late")])], ..Default::default() });
    let mut r = full(pos);
    unload.apply(&mut r);
    assert!(r.blocks.is_none() && r.state.is_empty() && r.fires.is_empty());
    assert_eq!(r.entities, vec![props(&[("items", "late")])]);
    // An append queued before a replacement is replaced with the rest.
    let mut early = Patch { append: vec![props(&[("items", "early")])], ..Default::default() };
    early.then(Patch::replace(ChunkRecord::default()));
    let mut r = full(pos);
    early.apply(&mut r);
    assert!(r.is_empty());
    assert!(early.is_total());
}

#[test]
fn the_store_reads_what_it_queued_and_writes_it_in_the_background() {
    let dir = temp("store");
    let (store, damage) = RegionStore::open(&dir, true).unwrap();
    assert!(damage.is_empty() && store.stored_at_open().is_empty());
    let a = IVec3::new(0, 1, 0);
    let b = IVec3::new(40, -2, -3);
    store.queue([(a, Patch::replace(full(a))), (b, Patch::replace(full(b)))]);
    // Read your writes, whether or not the I/O thread got to them yet.
    same(&store.read(a).unwrap(), &full(a));
    store.queue([(a, Patch { append: vec![props(&[("orbs", "more")])], ..Default::default() })]);
    assert_eq!(store.read(a).unwrap().entities.len(), 3);
    let (tx, rx) = std::sync::mpsc::channel();
    store.after_writes(move || {
        tx.send(()).unwrap();
        Ok(())
    });
    store.flush().unwrap();
    rx.try_recv().expect("the work after the writes ran");
    assert_eq!(store.stats().0, 0, "nothing waits");
    // Partial patches build on what's on disk.
    store.queue([(b, Patch { blocks: Some(None), ..Default::default() })]);
    store.flush().unwrap();
    let read = store.read(b).unwrap();
    assert!(read.blocks.is_none());
    assert_eq!(read.state, full(b).state);
    // An emptied record leaves no slot behind.
    store.queue([(b, Patch::replace(ChunkRecord::default()))]);
    assert!(store.read(b).is_none());
    drop(store);
    let (store, damage) = RegionStore::open(&dir, true).unwrap();
    assert!(damage.is_empty());
    assert_eq!(store.stored_at_open().iter().copied().collect::<Vec<_>>(), vec![a], "b was removed");
    let read = store.read(a).unwrap();
    assert_eq!(read.entities.last().unwrap(), &props(&[("orbs", "more")]));
    assert!(dir.join("r.0.0.vxr").is_file() && dir.join("r.2.-1.vxr").is_file());
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_full_queue_waits_for_the_disk_instead_of_growing() {
    let dir = temp("bounded");
    let (store, _) = RegionStore::open(&dir, true).unwrap();
    let small = |_: IVec3| Patch::replace(ChunkRecord { state: props(&[("spawners", "x")]), ..Default::default() });
    let many: Vec<(IVec3, Patch)> =
        (0..MAX_PENDING as i32 * 2).map(|i| (IVec3::new(i % 64, 0, i / 64), small(IVec3::ZERO))).collect();
    store.queue(many);
    assert!(store.stats().0 <= MAX_PENDING * 2);
    store.flush().unwrap();
    assert_eq!(store.stats().1, MAX_PENDING as u64 * 2, "every record was written");
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_damaged_record_regenerates_and_is_reported_once_with_a_copy() {
    let dir = temp("corrupt");
    let (store, _) = RegionStore::open(&dir, true).unwrap();
    let (a, b) = (IVec3::new(1, 0, 1), IVec3::new(2, 0, 1));
    store.queue([(a, Patch::replace(full(a))), (b, Patch::replace(full(b)))]);
    store.flush().unwrap();
    drop(store);
    // Flip a byte inside b's record.
    let path = dir.join(file_name(IVec2::ZERO));
    let mut bytes = fs::read(&path).unwrap();
    let at = SECTOR as usize + slot_of(b) * 8;
    let off = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize * SECTOR as usize;
    bytes[off + 40] ^= 0xff;
    fs::write(&path, &bytes).unwrap();
    let (store, damage) = RegionStore::open(&dir, true).unwrap();
    assert!(damage.is_empty(), "the table is whole");
    same(&store.read(a).unwrap(), &full(a));
    assert!(store.read(b).is_none(), "b regenerates");
    let notices = store.take_notices();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("regenerates") && notices[0].contains("damaged-"), "{}", notices[0]);
    let kept: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".damaged-"))
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(fs::read(kept[0].path()).unwrap(), bytes, "the copy is the damaged file");
    // Writing b again works, and the file reads whole afterwards.
    store.queue([(b, Patch::replace(full(b)))]);
    store.flush().unwrap();
    drop(store);
    let (store, damage) = RegionStore::open(&dir, true).unwrap();
    assert!(damage.is_empty());
    same(&store.read(b).unwrap(), &full(b));
    assert!(store.take_notices().is_empty());
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_read_only_store_never_writes() {
    let dir = temp("readonly");
    let (store, _) = RegionStore::open(&dir, true).unwrap();
    let a = IVec3::new(3, 1, 3);
    store.queue([(a, Patch::replace(full(a)))]);
    drop(store);
    let path = dir.join(file_name(IVec2::ZERO));
    let mut bytes = fs::read(&path).unwrap();
    // Into the record itself (the sector's tail is padding).
    bytes.truncate(bytes.len() - SECTOR as usize + 30);
    fs::write(&path, &bytes).unwrap();
    let (store, damage) = RegionStore::open(&dir, false).unwrap();
    assert_eq!(damage.len(), 1);
    assert!(damage[0].kept_as.is_none());
    assert!(store.read(a).is_none());
    drop(store);
    assert_eq!(fs::read(&path).unwrap(), bytes, "untouched");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no copy");
    fs::remove_dir_all(dir).unwrap();
}
