//! Saving in the middle of piston motion and repeater delays, then loading
//! the save in a separate process, changes nothing: the reloaded world
//! produces the same blocks tick by tick as the one that kept running, with
//! no moved block duplicated or lost.
use glam::IVec3;
use std::{process::Command, sync::Arc};
use voxelcraft::world::{
    World,
    block::{Block, Facing},
    chunk::ChunkData,
    redstone_blocks as r,
    storage::{LevelInfo, Storage},
    terrain::Generator,
};

/// A sticky piston saved mid-push, one saved mid-pull and a 4-tick
/// repeater saved with its pulse in flight.
const PUSH: IVec3 = IVec3::new(4, 145, 4);
const PULL: IVec3 = IVec3::new(4, 145, 12);
const DELAY: IVec3 = IVec3::new(4, 145, 20);
const EAST: u8 = 2;
const TICKS_AFTER_SAVE: usize = 12;

#[test]
fn piston_and_repeater_motion_survive_a_mid_motion_process_restart() {
    let path = std::env::temp_dir().join(format!(
        "voxelcraft-redstone-motion-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for phase in ["write", "read"] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "redstone_motion_worker", "--nocapture"])
            .env("VOXELCRAFT_MOTION_DIR", &path)
            .env("VOXELCRAFT_MOTION_PHASE", phase)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{phase}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}

/// Every block in the test area, as one line.
fn snapshot(w: &World) -> String {
    let mut ids = Vec::new();
    for z in 0..24 {
        for y in 144..148 {
            for x in 0..12 {
                ids.push(w.get_block(IVec3::new(x, y, z)).map_or(u16::MAX, |b| b.0).to_string());
            }
        }
    }
    ids.join(",")
}

fn count(w: &World, block: Block) -> usize {
    let mut n = 0;
    for z in 0..24 {
        for y in 144..148 {
            for x in 0..12 {
                n += (w.get_block(IVec3::new(x, y, z)) == Some(block)) as usize;
            }
        }
    }
    n
}

fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_redstone();
    }
}

/// Runs only as a child of the test above.
#[test]
fn redstone_motion_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_MOTION_DIR") else {
        return;
    };
    let storage = Storage::new(&path);
    let _lock = storage.lock().unwrap();
    let expected_path = std::path::Path::new(&path).join("expected.txt");
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    if std::env::var("VOXELCRAFT_MOTION_PHASE").unwrap() == "write" {
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        // Mid-pull: extend with a block in front, then cut the power.
        w.set_block(PULL, r::piston(EAST, true, false));
        w.set_block(PULL + IVec3::X, Block::IRON_BLOCK);
        w.set_block(PULL - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 3);
        assert_eq!(w.get_block(PULL + IVec3::X * 2), Some(Block::IRON_BLOCK));
        w.set_block(PULL - IVec3::X, Block::AIR);
        // Mid-push and the repeater start on the same tick.
        w.set_block(PUSH, r::piston(EAST, true, false));
        w.set_block(PUSH + IVec3::X, Block::GOLD_BLOCK);
        w.set_block(DELAY - IVec3::Y, Block::STONE);
        w.set_block(DELAY, r::repeater(Facing::East, 4, false));
        w.set_block(DELAY + IVec3::X, r::LAMP);
        w.set_block(PUSH - IVec3::X, r::REDSTONE_BLOCK);
        w.set_block(DELAY - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(PUSH + IVec3::X * 2), Some(r::MOVING), "saved mid-push");
        assert_eq!(w.get_block(PULL + IVec3::X), Some(r::MOVING), "saved mid-pull");
        assert_eq!(w.get_block(DELAY + IVec3::X), Some(r::LAMP), "saved before the repeater fires");
        assert_eq!(w.moving_piston_blocks(0.0).count(), 4, "two heads and two blocks in motion");
        let props = [("redstone", w.redstone_to_string()), ("automation", w.automation_to_string())]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect();
        storage.save(&LevelInfo { seed: 7, player: None, props }, &w.modified_chunks()).unwrap();
        // The reference: the same world, kept running without a reload.
        let mut expected = Vec::new();
        for _ in 0..TICKS_AFTER_SAVE {
            ticks(&mut w, 1);
            expected.push(snapshot(&w));
        }
        assert_eq!(w.get_block(PUSH + IVec3::X * 2), Some(Block::GOLD_BLOCK));
        assert_eq!(w.get_block(PULL + IVec3::X), Some(Block::IRON_BLOCK));
        assert_ne!(w.get_block(DELAY + IVec3::X), Some(r::LAMP), "the lamp lit within the test window");
        std::fs::write(expected_path, expected.join("\n")).unwrap();
    } else {
        let level = storage.load_level().unwrap();
        for (pos, data) in storage.load_chunks().unwrap() {
            w.receive_chunk(pos, data);
        }
        w.load_redstone(&level.props["redstone"]);
        w.load_automation(&level.props["automation"]);
        assert_eq!(w.moving_piston_blocks(0.0).count(), 4);
        let expected = std::fs::read_to_string(expected_path).unwrap();
        for (tick, line) in expected.lines().enumerate() {
            ticks(&mut w, 1);
            assert!(snapshot(&w) == line, "tick {} after the reload differs from the running world", tick + 1);
        }
        assert_eq!(count(&w, Block::GOLD_BLOCK), 1, "pushed block neither lost nor duplicated");
        assert_eq!(count(&w, Block::IRON_BLOCK), 1, "pulled block neither lost nor duplicated");
        assert_eq!(count(&w, r::MOVING), 0, "no motion left behind");
        assert_eq!(w.get_block(PUSH + IVec3::X), Some(r::piston_head(EAST, true)));
        assert_eq!(w.get_block(PULL), Some(r::piston(EAST, true, false)));
    }
}
