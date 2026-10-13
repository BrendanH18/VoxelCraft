//! Two independent processes exercise the real disk save format, including interrupted work.
use glam::{DVec3, IVec3};
use std::{process::Command, sync::Arc};
use voxelcraft::{
    entity::{Entities, MobKind, PlayerId},
    inventory::{Inventory, Stack},
    item::Item,
    potion::Potion,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        forms, overworld_blocks as ob,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};
const F: IVec3 = IVec3::new(4, 145, 4);
const B: IVec3 = IVec3::new(6, 145, 4);
const C: IVec3 = IVec3::new(8, 145, 4);
const P: IVec3 = IVec3::new(10, 145, 4);
#[test]
fn saved_interrupted_work_and_v07_entities_survive_a_process_restart() {
    let path = std::env::temp_dir().join(format!(
        "voxelcraft-process-save-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for phase in ["write", "read"] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "persistence_worker", "--nocapture"])
            .env("VOXELCRAFT_PERSISTENCE_DIR", &path)
            .env("VOXELCRAFT_PERSISTENCE_PHASE", phase)
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
fn persistence_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_PERSISTENCE_DIR") else {
        return;
    };
    let storage = Storage::new(path);
    let _lock = storage.lock().unwrap();
    assert_eq!(storage.lock().unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    if std::env::var("VOXELCRAFT_PERSISTENCE_PHASE").unwrap() == "write" {
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        w.set_block(F, Block::FURNACE);
        w.set_block(B, Block::BREWING_STAND);
        w.set_block(C, Block::COMPOSTER);
        w.set_block(P + IVec3::Y, ob::MANGROVE_LEAVES);
        w.apply_bone_meal(P + IVec3::Y);
        w.apply_bone_meal(P);
        w.apply_bone_meal(P);
        w.set_block(P + IVec3::Z, forms::wood_id(9, 0));
        w.set_block(P + IVec3::Z * 2, forms::stone_id(15, 4));
        let f = w.furnace_mut(F).unwrap();
        f.input = Some(Stack::new(Item::RAW_IRON, 1));
        f.fuel = Some(Stack::new(ob::BAMBOO_MOSAIC, 1));
        let b = w.brewing_stand_mut(B).unwrap();
        b.bottles = [Some(Stack::new(Item::potion(Potion::from_id("awkward").unwrap()), 1)); 3];
        b.ingredient = Some(Stack::new(Item::PUFFERFISH, 1));
        b.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
        for _ in 0..80 {
            w.tick_furnaces(0.05);
            w.tick_brewing(0.05);
        }
        for _ in 0..7 {
            assert!(w.compost(C, Item::CAKE));
        }
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        inventory.slots[1] = Some(Stack::new(forms::wood_id(10, 14), 3));
        let mut e = Entities::new(7);
        e.spawn(MobKind::Donkey, DVec3::new(14.5, 145., 4.5));
        e.mobs[0].uid = 7;
        let mount = e.mobs[0].mount.as_mut().unwrap();
        mount.tame = true;
        mount.owner = Some(PlayerId(3));
        mount.chest = true;
        mount.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        mount.slots[2] = Some(Stack::new(Item::DIAMOND, 16));
        e.spawn(MobKind::Fox, DVec3::new(15.5, 145., 4.5));
        e.mobs[1].animal.as_mut().unwrap().mouth = Some(Stack::new(Item::SWEET_BERRIES, 4));
        e.spawn(MobKind::Turtle, DVec3::new(16.5, 145., 4.5));
        e.mobs[2].age = -12000;
        let props = [
            ("inventory", inventory.serialize()),
            ("furnaces", w.furnaces_to_string()),
            ("brewing", w.brewing_stands_to_string()),
            ("redstone", w.redstone_to_string()),
            ("mobs", e.nether_mobs_to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        storage
            .save(
                &LevelInfo { seed: 7, player: Some((DVec3::new(12.5, 145., 4.5), 1., 0.)), props },
                &w.modified_chunks(),
            )
            .unwrap();
    } else {
        let level = storage.load_level().unwrap();
        assert_eq!(level.seed, 7);
        assert_eq!(level.player.unwrap().0, DVec3::new(12.5, 145., 4.5));
        for (pos, data) in storage.load_chunks().unwrap() {
            w.receive_chunk(pos, data);
        }
        w.load_furnaces(&level.props["furnaces"]);
        w.load_brewing_stands(&level.props["brewing"]);
        w.load_redstone(&level.props["redstone"]);
        assert_eq!(w.get_block(P), Some(ob::hanging_propagule(2)));
        assert_eq!(w.get_block(P + IVec3::Z), Some(forms::wood_id(9, 0)));
        assert_eq!(w.get_block(P + IVec3::Z * 2), Some(forms::stone_id(15, 4)));
        let inventory = Inventory::deserialize(&level.props["inventory"]).unwrap();
        assert_eq!(inventory.get(0), Some(Stack::new(Item::SADDLE, 1)));
        assert_eq!(inventory.get(1), Some(Stack::new(forms::wood_id(10, 14), 3)));
        let mut e = Entities::new(7);
        e.load_nether_mobs(&level.props["mobs"]);
        let mount = e.mobs[0].mount.as_ref().unwrap();
        assert!(mount.tame && mount.chest);
        assert_eq!(mount.owner, Some(PlayerId(3)));
        assert_eq!(mount.slots[2], Some(Stack::new(Item::DIAMOND, 16)));
        assert_eq!(e.mobs[1].animal.as_ref().unwrap().mouth, Some(Stack::new(Item::SWEET_BERRIES, 4)));
        assert_eq!(e.mobs[2].age, -12000);
        for _ in 0..340 {
            w.tick_furnaces(0.05);
            w.tick_brewing(0.05);
            w.tick_redstone();
        }
        assert_eq!(w.furnace(F).unwrap().output, Some(Stack::new(Item::IRON_INGOT, 1)));
        assert!(w.furnace(F).unwrap().input.is_none());
        assert!(
            w.brewing_stand(B)
                .unwrap()
                .bottles
                .iter()
                .all(|b| *b == Some(Stack::new(Item::potion(Potion::from_id("water_breathing").unwrap()), 1)))
        );
        assert!(w.brewing_stand(B).unwrap().ingredient.is_none());
        assert_eq!(w.take_compost(C), Some(Stack::new(Item::BONE_MEAL, 1)));
    }
}
