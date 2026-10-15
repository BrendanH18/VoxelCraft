//! Rebuild authoritative lighting from real saved chunk edits in a new process.
use glam::IVec3;
use rustc_hash::FxHashMap;
use std::{process::Command, sync::Arc};
use voxelcraft::world::{
    World,
    block::Block,
    chunk::ChunkData,
    storage::{LevelInfo, Storage},
    terrain::{Dimension, Generator},
};

#[test]
fn sky_and_torch_light_rebuild_after_overworld_and_end_process_restarts() {
    let root = std::env::temp_dir().join(format!(
        "voxelcraft-light-restart-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for dimension in ["overworld", "end"] {
        for phase in ["write", "read"] {
            let result = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "lighting_worker", "--nocapture"])
                .env("VOXELCRAFT_LIGHT_RESTART_DIR", root.join(dimension))
                .env("VOXELCRAFT_LIGHT_RESTART_DIMENSION", dimension)
                .env("VOXELCRAFT_LIGHT_RESTART_PHASE", phase)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{dimension} {phase}: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lighting_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_LIGHT_RESTART_DIR") else { return };
    let dimension = match std::env::var("VOXELCRAFT_LIGHT_RESTART_DIMENSION").unwrap().as_str() {
        "overworld" => Dimension::Overworld,
        "end" => Dimension::End,
        other => panic!("unexpected dimension {other}"),
    };
    let storage = Storage::new(path.clone());
    let _lock = storage.lock().unwrap();
    let mut world = World::new_remote(Arc::new(Generator::for_dimension(7, dimension)), 2);
    let shaft = IVec3::new(20, 63, 16);
    let probe = IVec3::new(24, 40, 16);
    if std::env::var("VOXELCRAFT_LIGHT_RESTART_PHASE").unwrap() == "write" {
        let mut chunks = FxHashMap::default();
        for z in -1..=1 {
            for x in -1..=1 {
                for y in dimension.chunk_rows() {
                    let pos = IVec3::new(x, y, z);
                    let data = Arc::new(ChunkData::Uniform(if y < 2 { Block::STONE } else { Block::AIR }));
                    world.receive_chunk(pos, Arc::clone(&data));
                    chunks.insert(pos, data);
                }
            }
        }
        for y in 40..=63 {
            assert!(world.set_block(shaft.with_y(y), Block::AIR));
        }
        for x in 21..=29 {
            for y in 40..=41 {
                assert!(world.set_block(IVec3::new(x, y, 16), Block::AIR));
            }
        }
        assert!(world.set_block(shaft, Block::GLASS));
        assert!(world.set_block(probe, Block::TORCH));
        world.update_block_light();
        assert_eq!(world.sky_light(probe), Some(11));
        assert_eq!(world.block_light(probe), 14);
        // Existing partial states are reconstructed from the unchanged disk format.
        let partials = [
            Block::slab_of(Block::STONE).unwrap(),
            Block::STONE_STAIRS,
            Block::DIRT_PATH,
            Block::FARMLAND,
            voxelcraft::world::overworld_blocks::snow_layer(7),
        ];
        for (i, b) in partials.into_iter().enumerate() {
            let p = IVec3::new(6 + i as i32, 80, 8);
            world.set_block(p - IVec3::Y, Block::STONE);
            world.set_block(p, b);
        }
        world.set_block(IVec3::new(5, 80, 8), Block::TORCH);
        std::fs::write(std::path::Path::new(&path).join("expected-light.bin"), centre_arrays(&world)).unwrap();
        chunks.extend(world.modified_chunks());
        let chunks: Vec<_> = chunks.into_iter().collect();
        storage.save(&LevelInfo { seed: 7, player: None, props: Default::default() }, &chunks).unwrap();
    } else {
        assert_eq!(storage.load_level().unwrap().seed, 7);
        for (pos, data) in storage.load_chunks().unwrap() {
            world.receive_chunk(pos, data);
        }
        world.update_block_light();
        let expected = std::fs::read(std::path::Path::new(&path).join("expected-light.bin")).unwrap();
        assert_eq!(centre_arrays(&world), expected, "two complete chunks of sky/block arrays after restart");
        assert_eq!(world.get_block(shaft), Some(Block::GLASS));
        assert_eq!(world.get_block(probe), Some(Block::TORCH));
        assert_eq!(world.sky_light(probe), Some(11));
        assert_eq!(world.block_light(probe), 14);
        // Subsequent edits must remove restored paths and relight surviving ones.
        assert!(world.set_block(probe, Block::AIR));
        assert_eq!(world.block_light(probe), 0);
        assert_eq!(world.sky_light(probe), Some(11));
        assert!(world.set_block(shaft, Block::STONE));
        assert_eq!(world.raw_brightness(probe, 0), Some(0));
        assert!(world.set_block(shaft, Block::AIR));
        assert_eq!(world.sky_light(probe), Some(11));
    }
}

/// Two complete centre chunks, containing both the underground tunnel and partial blocks.
fn centre_arrays(world: &World) -> Vec<u8> {
    (32..96)
        .flat_map(|y| {
            (0..32).flat_map(move |z| {
                (0..32).map(move |x| {
                    let p = IVec3::new(x, y, z);
                    world.sky_light(p).expect("fully loaded test columns") | world.block_light(p) << 4
                })
            })
        })
        .collect()
}
