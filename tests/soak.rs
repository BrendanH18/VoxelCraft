//! v1.0 Track 3: a few simulated minutes of the long-session soak
//! (`voxelcraft::soak`), through every leg of its itinerary, with the
//! bounds the multi-hour run on Boat holds
//! (`docs/verification/2026-11-25-v1.0-soak.md`).
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};
use voxelcraft::soak::{self, LEGS, Options, Sample};
use voxelcraft::world::World;
use voxelcraft::world::block::Block;
use voxelcraft::world::terrain::Generator;

#[test]
fn a_short_soak_keeps_every_queue_and_cache_bounded() {
    let dir = std::env::temp_dir().join(format!("voxelcraft-soak-{}", std::process::id()));
    let options = Options {
        render_distance: 3,
        hours: 0.1,
        leg_minutes: 0.75,
        sample_seconds: 15.0,
        autosave_seconds: 45.0,
        data_dir: dir.join("world"),
        csv: None,
        ..Options::default()
    };
    let mut samples: Vec<Sample> = Vec::new();
    let summary = soak::run(&options, |s| samples.push(s.clone())).expect("the soak ran");
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!("{summary:?}");
    assert_eq!(summary.ticks, 7200);
    assert_eq!(summary.load_timeouts, 0, "the walker never waited out a load");
    for leg in LEGS {
        assert!(samples.iter().any(|s| s.leg == leg), "no sample on the {leg} leg");
    }
    assert!(summary.dimension_changes >= 3 && summary.reopens >= 2, "{summary:?}");
    for s in &samples {
        let v = |k: &str| s.get(k).unwrap_or_else(|| panic!("no {k}"));
        // Events the game drains every tick are empty between ticks.
        for (name, value) in &s.values {
            if name.starts_with("w.events.") && name != "w.events.particles" {
                assert_eq!(*value, 0.0, "{name} left over at {}", v("tick"));
            }
        }
        let cap = |name: &str, max: f64| assert!(v(name) <= max, "{name} = {} > {max} at {}", v(name), v("tick"));
        // Streaming holds what's in range, and its queues stay short.
        let side = f64::from(2 * (options.render_distance + 3) + 1);
        cap("w.chunks.loaded", side * side * 16.0);
        cap("w.chunks.columns", side * side);
        cap("w.stream.gen_in_flight", 64.0);
        cap("w.stream.load_list", side * side * 16.0);
        cap("w.light.block_flood_raw", 65536.0);
        cap("w.light.sky_flood_raw", 65536.0);
        cap("w.redstone.order.trace", 0.0);
        cap("w.fire.scheduled", 4096.0);
        // Cells of unloaded chunks leave the fluid queue (they used to pile up
        // behind the walker and be scanned every fluid tick).
        cap("w.fluid.pending", 16384.0);
        cap("e.entities.mobs", 400.0);
        cap("e.entities.items", 600.0);
        cap("e.entities.pending_sounds", 0.0);
        cap("e.entities.mob_paths", 400.0 * 64.0);
        cap("w.events.particles", 256.0);
        cap("e.entities.particles", 256.0);
        // v1.0 Track 3b: unloaded chunks live in region files. Memory holds
        // no unloaded chunk's blocks. Stored entities wait in memory only
        // while their chunk loads or the level file holds them (after a
        // reopen, until the next save): no more than the loaded area holds.
        cap("w.chunks.saved_unloaded", 0.0);
        cap("e.stored.mobs", 400.0);
        cap("e.stored.items", 600.0);
        cap("e.stored.entities", 1100.0);
        cap("w.regions.pending", 4096.0);
        cap("w.fluid.suspended", 16384.0);
        // Block entities are the loaded area's (the base's chests, furnaces
        // and spawner, and the structures in range).
        cap("w.chests", 400.0);
        cap("w.spawners", 200.0);
    }
    let last = samples.last().unwrap();
    assert!(last.get("w.regions.written").unwrap() > 0.0, "chunks were written to region files");
}

/// The soak's first finding: a cell rewritten with the state it already
/// holds marked its chunk player-modified, so every chunk the fluid,
/// redstone and growth ticks touched near the player was saved and kept in
/// memory for the rest of the session. Java's `LevelChunk.setBlockState`
/// changes nothing then; only a real change marks the chunk now.
#[test]
fn rewriting_a_cell_with_its_own_state_leaves_its_chunk_unmodified() {
    let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    let at = DVec3::new(8.5, 100.0, 8.5);
    let deadline = Instant::now() + Duration::from_secs(60);
    while !(world.is_loaded(at.floor().as_ivec3()) && world.pending_jobs() == 0) {
        world.update(at);
        assert!(Instant::now() < deadline, "the world did not load");
        std::thread::sleep(Duration::from_millis(1));
    }
    let ground = (world.min_y()..world.max_y())
        .rev()
        .map(|y| IVec3::new(8, y, 8))
        .find(|&p| world.get_block(p).is_some_and(|b| b != Block::AIR))
        .expect("ground");
    let before = world.modified_chunks().len();
    let block = world.get_block(ground).unwrap();
    world.set_block(ground, block);
    assert_eq!(world.modified_chunks().len(), before, "rewriting {block:?} with itself modified its chunk");
    let other = if block == Block::STONE { Block::DIRT } else { Block::STONE };
    world.set_block(ground, other);
    let chunk = IVec3::new(0, ground.y.div_euclid(32), 0);
    assert!(world.modified_chunks().iter().any(|(p, _)| *p == chunk), "a real change marks the chunk");
}
