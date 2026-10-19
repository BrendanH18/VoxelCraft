//! Two independent processes exercise the real disk save format, including interrupted work.
use glam::{DVec3, IVec3};
use std::{process::Command, sync::Arc};
use voxelcraft::{
    entity::{Entities, MobKind, PlayerId},
    inventory::{Inventory, Stack},
    item::Item,
    potion::Potion,
    session::PlayerSession,
    simulation::effects::Effect,
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
const T: IVec3 = IVec3::new(6, 145, 6);
const C: IVec3 = IVec3::new(8, 145, 4);
const P: IVec3 = IVec3::new(10, 145, 4);
const M: IVec3 = IVec3::new(14, 146, 8);
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
/// Runs only as a child of `persistence_worker`, which holds the save lock.
#[test]
fn lock_probe_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_LOCK_PROBE_DIR") else {
        return;
    };
    let err = Storage::new(path).lock().unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
    println!("lock probe: WouldBlock (pid {})", std::process::id());
}
#[test]
fn persistence_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_PERSISTENCE_DIR") else {
        return;
    };
    let storage = Storage::new(&path);
    let _lock = storage.lock().unwrap();
    // A genuinely separate writer process cannot lock the open world.
    let probe = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_probe_worker", "--nocapture"])
        .env("VOXELCRAFT_LOCK_PROBE_DIR", &path)
        .output()
        .unwrap();
    assert!(
        probe.status.success() && String::from_utf8_lossy(&probe.stdout).contains("lock probe: WouldBlock"),
        "lock probe: {}\n{}",
        String::from_utf8_lossy(&probe.stdout),
        String::from_utf8_lossy(&probe.stderr)
    );
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    if std::env::var("VOXELCRAFT_PERSISTENCE_PHASE").unwrap() == "write" {
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        w.set_block(F, Block::FURNACE);
        w.set_block(B, Block::BREWING_STAND);
        w.set_block(C, Block::COMPOSTER);
        w.set_block(T, Block::BREWING_STAND);
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
        let t = w.brewing_stand_mut(T).unwrap();
        t.bottles = [Some(Stack::new(Item::splash_potion(Potion::AWKWARD), 1)); 3];
        t.ingredient = Some(Stack::new(Item::TURTLE_HELMET, 1));
        t.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
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
        inventory.armor[0] = Some(Stack::new(Item::TURTLE_HELMET, 1));
        inventory.slots[2] = Some(Stack::new(Item::potion(Potion::from_id("strong_turtle_master").unwrap()), 1));
        let mut e = Entities::new(7);
        let mut session = PlayerSession::new(PlayerId(3), DVec3::new(12.5, 145., 4.5));
        session.player.yaw = 1.;
        session.player.on_ground = true;
        inventory.slots[3] = Some(Stack::new(Item::APPLE, 2));
        inventory.slots[4] =
            Some(Stack::new(Item::tool(voxelcraft::item::ToolKind::Pickaxe, voxelcraft::item::Tier::Iron), 1));
        session.inventory = inventory;
        session.vitals.hunger.food = 8.;
        session.spawn_point = Some(IVec3::new(15, 145, 5));
        // Absorption II with 2.5 of its 8 points spent.
        session.vitals.apply_effect(Effect::Absorption, 1, 2400);
        assert_eq!(session.vitals.damage(2.5, "test", false), 2.5);
        assert_eq!((session.vitals.health, session.vitals.effects.absorption()), (20., 5.5));
        for _ in 0..31 {
            assert!(session.tick_consumption(3, true, &mut e).unwrap().completed.is_none());
        }
        w.set_block(M, Block::STONE);
        for _ in 0..3 {
            assert!(matches!(
                session.tick_mining(4, &mut w, &mut e, &Default::default()).unwrap(),
                voxelcraft::session::mining::Outcome::Hit { .. }
            ));
        }
        e.spawn(MobKind::Donkey, DVec3::new(14.5, 145., 4.5));
        e.mobs[0].uid = 7;
        e.mobs[0].effects.add(Effect::Slowness, 3, 197);
        e.mobs[0].effects.add(Effect::Resistance, 2, 197);
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
            ("inventory", session.inventory.serialize()),
            ("effects", session.vitals.effects.serialize()),
            ("spawn_point", "15,145,5".into()),
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
        assert_eq!(inventory.armor[0], Some(Stack::new(Item::TURTLE_HELMET, 1)));
        assert_eq!(
            inventory.get(2),
            Some(Stack::new(Item::potion(Potion::from_id("strong_turtle_master").unwrap()), 1))
        );
        let mut e = Entities::new(7);
        let mut session = PlayerSession::from_parts(
            PlayerId(3),
            voxelcraft::player::Player::new(level.player.unwrap().0),
            inventory,
            Default::default(),
            voxelcraft::rules::GameMode::Survival,
        );
        session.vitals.hunger.food = 8.;
        session.vitals.effects = voxelcraft::simulation::effects::Effects::deserialize(&level.props["effects"]);
        let absorption = session.vitals.effects.get(Effect::Absorption).unwrap();
        assert_eq!((absorption.amplifier, absorption.ticks), (1, 2400));
        assert_eq!((session.vitals.effects.absorption(), session.vitals.effects.max_absorption()), (5.5, 8.));
        session.player.yaw = level.player.unwrap().1;
        session.player.pitch = level.player.unwrap().2;
        session.player.on_ground = true;
        let coords = level.props["spawn_point"].split(',').map(|s| s.parse::<i32>().unwrap()).collect::<Vec<_>>();
        session.spawn_point = Some(IVec3::new(coords[0], coords[1], coords[2]));
        assert_eq!(session.spawn_point, Some(IVec3::new(15, 145, 5)));
        assert_eq!(session.consumption_progress(), 0., "interrupted use is transient");
        for _ in 0..31 {
            assert!(session.tick_consumption(3, true, &mut e).unwrap().completed.is_none());
            assert_eq!(session.inventory.get(3), Some(Stack::new(Item::APPLE, 2)));
        }
        assert!(session.tick_consumption(3, true, &mut e).unwrap().completed.is_some());
        assert_eq!(session.inventory.get(3), Some(Stack::new(Item::APPLE, 1)));
        assert!(session.breaking(&w).is_none(), "mining progress is transient");
        assert_eq!(w.get_block(M), Some(Block::STONE));
        for _ in 0..7 {
            assert!(matches!(
                session.tick_mining(4, &mut w, &mut e, &Default::default()).unwrap(),
                voxelcraft::session::mining::Outcome::Hit { .. }
            ));
            assert_eq!(session.inventory.get(4).unwrap().damage, 0);
            assert_eq!(w.get_block(M), Some(Block::STONE));
        }
        assert!(matches!(
            session.tick_mining(4, &mut w, &mut e, &Default::default()).unwrap(),
            voxelcraft::session::mining::Outcome::Broken { .. }
        ));
        assert_eq!(session.inventory.get(4).unwrap().damage, 1);
        e.load_nether_mobs(&level.props["mobs"]);
        assert_eq!(e.mobs[0].effects.get(Effect::Resistance).unwrap().ticks, 197);
        assert_eq!(e.mobs[0].effects.get(Effect::Slowness).unwrap().amplifier, 3);
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
        assert!(
            w.brewing_stand(T)
                .unwrap()
                .bottles
                .iter()
                .all(|b| *b == Some(Stack::new(Item::splash_potion(Potion::from_id("turtle_master").unwrap()), 1)))
        );
        assert!(w.brewing_stand(T).unwrap().ingredient.is_none());
        assert_eq!(w.take_compost(C), Some(Stack::new(Item::BONE_MEAL, 1)));
    }
}
