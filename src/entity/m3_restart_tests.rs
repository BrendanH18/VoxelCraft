//! The M3 exit's restart check (v0.16, 2026-11-21): cleared structure
//! containers, spawners and residents of a generation 5 world stay cleared
//! after a restart.
//!
//! Each case loads a real headless world around a generated structure,
//! clears it the way a player would (empties a loot chest, breaks a
//! spawner, kills the residents and takes the End ship's elytra), writes
//! the level with [`Storage`] in the save format the game uses (the
//! dimension's `chests` and `spawners`, the entities' properties and the
//! edited chunks), then reads it back into a fresh generator, whose
//! structure caches start empty, so every unedited chunk and every layout
//! is generated again. Nothing may be refilled, re-registered or respawned.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec2, IVec3};

use super::{Ctx, Entities, MobKind, PlayerId, Target};
use crate::simulation::difficulty::Difficulty;
use crate::simulation::lighting::SpawnLight;
use crate::world::World;
use crate::world::block::Block;
use crate::world::fortress::Feature;
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::{Dimension, Generation, Generator};

const SEED: u64 = 12345;
/// Chunks streamed around the player, each way (32-block chunks).
const DISTANCE: i32 = 2;

fn generator(dimension: Dimension) -> Arc<Generator> {
    Arc::new(Generator::with_generation(SEED, dimension, Generation::V5))
}

/// Streams the world around `at` until every chunk the cells need is in
/// and no generation job is pending.
fn load(world: &mut World, at: IVec3, cells: &[IVec3]) {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        world.update(at.as_dvec3());
        if cells.iter().all(|&c| world.get_block(c).is_some()) && world.pending_jobs() == 0 {
            return;
        }
        assert!(Instant::now() < deadline, "the world around {at} never loaded");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn ctx(at: IVec3, dimension: Dimension) -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), true)],
        daylight: 1.0,
        spawning: true,
        raining: false,
        dimension,
        spawn_light: SpawnLight::NIGHT,
    }
}

/// A temporary save folder, removed when dropped.
struct Folder(std::path::PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        Self(std::env::temp_dir().join(format!("voxelcraft-m3-{name}-{}-{nanos}", std::process::id())))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Saves `world` and `entities` as the game does and reads them back on a
/// fresh generator; returns the reloaded world (not yet streamed) and
/// entities.
fn restart(folder: &Folder, world: &World, entities: &Entities, dimension: Dimension) -> (World, Entities) {
    let mut props = BTreeMap::new();
    props.insert("chests".to_string(), world.chests_to_string());
    props.insert("spawners".to_string(), world.spawners_to_string());
    entities.save_props(&mut props);
    let storage = Storage::new(&folder.0);
    storage.save(&LevelInfo { seed: SEED, player: None, props }, &world.modified_chunks()).unwrap();
    let level = storage.load_level().unwrap();
    let saved = storage.load_chunks().unwrap();
    let mut world = World::new_headless(generator(dimension), saved, DISTANCE);
    world.load_chests(&level.props["chests"]);
    world.load_spawners(&level.props["spawners"]);
    let mut entities = Entities::new(SEED);
    entities.load_props(&level.props);
    (world, entities)
}

/// The first `want` feature of the chunks around `at` (searching `rings`
/// chunk columns out over every row of the dimension).
fn find(g: &Generator, at: IVec3, rings: i32, want: impl Fn(&Feature) -> bool) -> Option<IVec3> {
    let c = crate::world::chunk::chunk_of(at);
    let rows = g.dimension.chunk_rows();
    (0..=rings).find_map(|r| {
        (-r..=r)
            .flat_map(|z| (-r..=r).map(move |x| IVec2::new(x, z)))
            .filter(|p| p.abs().max_element() == r)
            .flat_map(|p| rows.clone().map(move |y| IVec3::new(c.x + p.x, y, c.z + p.y)))
            .flat_map(|cpos| g.structure_features(cpos))
            .find(|(_, f)| want(f))
            .map(|(p, _)| p)
    })
}

/// Empties the chest at `at` (a player taking everything) and returns how
/// many stacks it held.
fn loot(world: &mut World, at: IVec3) -> usize {
    let chest = world.chest_mut(at).unwrap_or_else(|| panic!("no chest registered at {at}"));
    let held = chest.slots.iter().flatten().count();
    chest.slots.fill(None);
    held
}

fn empty(world: &World, at: IVec3) -> bool {
    world.chest(at).is_some_and(|c| c.slots.iter().all(Option::is_none))
}

/// Overworld: a shipwreck chest, a stronghold chest, its silverfish spawner
/// broken, an abandoned camp chest, and an ocean monument's three elder
/// guardians killed.
#[test]
fn generation_5_overworld_structures_stay_cleared_after_a_restart() {
    use crate::world::temples::{Kind, LootTable};
    let g = generator(Dimension::Overworld);
    let wreck = g.temples.nearest(&g, Kind::Shipwreck, IVec2::ZERO, 6400).expect("a shipwreck");
    let wreck_chest = find(&g, wreck, 2, |f| {
        matches!(f, Feature::TempleChest(_, LootTable::ShipSupply | LootTable::ShipTreasure | LootTable::ShipMap))
    })
    .expect("a shipwreck chest");
    let camp = g.temples.nearest(&g, Kind::AbandonedCamp, IVec2::ZERO, 6400).expect("an abandoned camp");
    let camp_chest =
        find(&g, camp, 2, |f| matches!(f, Feature::StructureLoot(_, t) if t.starts_with("abandoned_camp")))
            .expect("an abandoned camp chest");
    let stronghold = g.strongholds.nearest(IVec3::ZERO).expect("a stronghold");
    let cage = find(&g, stronghold, 4, |f| matches!(f, Feature::Spawner(MobKind::Silverfish))).expect("a portal room");
    let library = find(&g, stronghold, 4, |f| matches!(f, Feature::StrongholdChest(_))).expect("a stronghold chest");
    let monument = g.temples.nearest(&g, Kind::Monument, IVec2::ZERO, 12_800).expect("a monument");

    let folder = Folder::new("overworld");
    let mut world = World::new_headless(Arc::clone(&g), Default::default(), DISTANCE);
    let mut entities = Entities::new(SEED);
    for (at, cells) in
        [(wreck_chest, vec![wreck_chest]), (camp_chest, vec![camp_chest]), (library, vec![library]), (cage, vec![cage])]
    {
        load(&mut world, at, &cells);
    }
    for chest in [wreck_chest, camp_chest, library] {
        assert!(loot(&mut world, chest) > 0, "the chest at {chest} was generated empty");
    }
    assert_eq!(world.spawner(cage), Some(MobKind::Silverfish));
    assert!(world.set_block(cage, Block::AIR), "mining the portal room's spawner");
    assert_eq!(world.spawner(cage), None);
    // The monument populates once a player is near and its cells are in.
    load(&mut world, monument, &[monument]);
    let near = ctx(monument, Dimension::Overworld);
    let deadline = Instant::now() + Duration::from_secs(120);
    while entities.count(MobKind::ElderGuardian) < 3 {
        world.update(monument.as_dvec3());
        entities.populate_structures(1.0, &world, &near, Difficulty::Normal);
        assert!(Instant::now() < deadline, "the monument at {monument} never populated");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(entities.count(MobKind::ElderGuardian), 3);
    entities.mobs.retain(|m| m.kind != MobKind::ElderGuardian);

    let (mut world, mut entities) = restart(&folder, &world, &entities, Dimension::Overworld);
    for (at, cells) in [
        (wreck_chest, vec![wreck_chest]),
        (camp_chest, vec![camp_chest]),
        (library, vec![library]),
        (cage, vec![cage]),
        (monument, vec![monument]),
    ] {
        load(&mut world, at, &cells);
    }
    for chest in [wreck_chest, camp_chest, library] {
        assert!(empty(&world, chest), "the chest at {chest} refilled after the restart");
    }
    assert_eq!(world.get_block(cage), Some(Block::AIR), "the broken spawner came back");
    assert_eq!(world.spawner(cage), None);
    assert!(world.spawners().iter().all(|(p, _)| *p != cage));
    for _ in 0..5 {
        world.update(monument.as_dvec3());
        entities.populate_structures(1.0, &world, &near, Difficulty::Normal);
    }
    assert_eq!(entities.count(MobKind::ElderGuardian), 0, "elder guardians respawned after the restart");
}

/// The Nether: a fortress chest looted and a blaze spawner broken.
#[test]
fn generation_5_fortress_stays_cleared_after_a_restart() {
    let g = generator(Dimension::Nether);
    let fortress = g.nearest_fortress(IVec2::ZERO).expect("a fortress");
    let chest = find(&g, fortress, 6, |f| matches!(f, Feature::Chest(_))).expect("a fortress chest");
    let cage = find(&g, fortress, 6, |f| matches!(f, Feature::Spawner(MobKind::Blaze))).expect("a blaze spawner");
    let folder = Folder::new("nether");
    let mut world = World::new_headless(Arc::clone(&g), Default::default(), DISTANCE);
    load(&mut world, chest, &[chest]);
    load(&mut world, cage, &[cage]);
    assert!(loot(&mut world, chest) > 0, "the fortress chest at {chest} was generated empty");
    assert_eq!(world.spawner(cage), Some(MobKind::Blaze));
    assert!(world.set_block(cage, Block::AIR));
    let (mut world, _) = restart(&folder, &world, &Entities::new(SEED), Dimension::Nether);
    load(&mut world, chest, &[chest]);
    load(&mut world, cage, &[cage]);
    assert!(empty(&world, chest), "the fortress chest refilled after the restart");
    assert_eq!(world.get_block(cage), Some(Block::AIR));
    assert_eq!(world.spawner(cage), None, "the blaze spawner came back");
}

/// The End: an End city's chest looted, its shulkers killed and the ship's
/// elytra taken from its frame.
#[test]
fn generation_5_end_city_stays_cleared_after_a_restart() {
    let g = generator(Dimension::End);
    let end = g.end().unwrap();
    let city = (1..=40)
        .flat_map(|r| (-r..=r).flat_map(move |z| (-r..=r).map(move |x| IVec2::new(x, z) * 320)))
        .flat_map(|p| end.cities_near(p, p))
        .find(|c| c.elytra_frame.is_some())
        .expect("an End city with a ship");
    let frame = city.elytra_frame.unwrap().cell;
    let chest = city
        .features
        .iter()
        .find(|(p, f)| matches!(f, Feature::TempleChest(..)) && (*p - frame).abs().max_element() <= 32)
        .or_else(|| city.features.iter().find(|(_, f)| matches!(f, Feature::TempleChest(..))))
        .expect("an End city chest")
        .0;
    let sentries: Vec<_> = city.sentries.iter().copied().filter(|s| (*s - frame).abs().max_element() <= 64).collect();
    assert!(!sentries.is_empty(), "shulkers near the ship");
    let folder = Folder::new("end");
    let mut world = World::new_headless(Arc::clone(&g), Default::default(), DISTANCE);
    let mut entities = Entities::new(SEED);
    load(&mut world, chest, &[chest]);
    load(&mut world, frame, &[frame]);
    for &s in &sentries {
        load(&mut world, frame, &[s]);
    }
    assert!(loot(&mut world, chest) > 0, "the End city chest at {chest} was generated empty");
    let near = ctx(frame, Dimension::End);
    for _ in 0..3 {
        entities.populate_end_cities(1.0, &world, &near);
    }
    let shulkers = entities.count(MobKind::Shulker);
    assert!(shulkers >= sentries.len(), "{shulkers} shulkers for {} sentries", sentries.len());
    assert_eq!(entities.decor.len(), 1, "the ship's elytra frame");
    entities.mobs.retain(|m| m.kind != MobKind::Shulker);
    // Taking the elytra: the frame stays, empty.
    entities.decor.clear();

    let (mut world, mut entities) = restart(&folder, &world, &entities, Dimension::End);
    load(&mut world, chest, &[chest]);
    load(&mut world, frame, &[frame]);
    for &s in &sentries {
        load(&mut world, frame, &[s]);
    }
    assert!(empty(&world, chest), "the End city chest refilled after the restart");
    for _ in 0..3 {
        entities.populate_end_cities(1.0, &world, &near);
    }
    assert_eq!(entities.count(MobKind::Shulker), 0, "shulkers respawned after the restart");
    assert!(entities.decor.is_empty(), "the elytra frame was hung again");
}
