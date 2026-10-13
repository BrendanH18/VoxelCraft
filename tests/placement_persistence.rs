//! Paired block placements and their spent inventory survive a real restart.
use glam::{DVec3, IVec3};
use std::{process::Command, sync::Arc};
use voxelcraft::{
    color::DyeColor,
    entity::PlayerId,
    inventory::{Inventory, Stack},
    item::Item,
    session::PlayerSession,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};
const DOOR: IVec3 = IVec3::new(6, 146, 4);
const FOOT: IVec3 = IVec3::new(7, 146, 8);

#[test]
fn placed_door_bed_and_spent_stacks_survive_a_process_restart() {
    let path = std::env::temp_dir().join(format!(
        "voxelcraft-place-restart-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for phase in ["write", "read"] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "placement_worker", "--nocapture"])
            .env("VOXELCRAFT_PLACE_RESTART_DIR", &path)
            .env("VOXELCRAFT_PLACE_RESTART_PHASE", phase)
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

#[test]
fn placement_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_PLACE_RESTART_DIR") else { return };
    let storage = Storage::new(path);
    let _lock = storage.lock().unwrap();
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    if std::env::var("VOXELCRAFT_PLACE_RESTART_PHASE").unwrap() == "write" {
        world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        world.set_block(DOOR - IVec3::Y, Block::STONE);
        world.set_block(DOOR + IVec3::X, Block::STONE);
        let mut session = PlayerSession::new(PlayerId::HOST, DVec3::new(4.5, 145., 4.5));
        session.inventory.slots[0] = Some(Stack::new(Item::from_name("bamboo_door").unwrap(), 1));
        session.place_held(0, &mut world, &[]).unwrap();
        for p in [FOOT - IVec3::Y, FOOT + IVec3::X - IVec3::Y] {
            world.set_block(p, Block::STONE);
        }
        session.player.pos.z = 8.5;
        session.player.pitch = -0.2;
        session.inventory.slots[1] = Some(Stack::new(DyeColor::Cyan.bed(), 1));
        session.place_held(1, &mut world, &[]).unwrap();
        assert!(session.inventory.get(0).is_none() && session.inventory.get(1).is_none());
        storage
            .save(
                &LevelInfo {
                    seed: 7,
                    player: None,
                    props: [("inventory".into(), session.inventory.serialize())].into_iter().collect(),
                },
                &world.modified_chunks(),
            )
            .unwrap();
    } else {
        let level = storage.load_level().unwrap();
        for (pos, data) in storage.load_chunks().unwrap() {
            world.receive_chunk(pos, data);
        }
        let inventory = Inventory::deserialize(&level.props["inventory"]).unwrap();
        assert!(inventory.get(0).is_none() && inventory.get(1).is_none());
        assert!(world.get_block(DOOR).unwrap().is_door());
        assert!(world.get_block(DOOR + IVec3::Y).unwrap().is_door_upper());
        assert_eq!(world.get_block(FOOT), Some(Block::colored_bed(DyeColor::Cyan, false)));
        assert_eq!(world.get_block(FOOT + IVec3::X), Some(Block::colored_bed(DyeColor::Cyan, true)));
        assert_eq!(world.bed_partner(FOOT, world.get_block(FOOT).unwrap()), Some(FOOT + IVec3::X));
    }
}
