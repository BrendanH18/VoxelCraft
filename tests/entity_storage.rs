//! Per-chunk entity storage with real streaming and real saves (Java 26.3
//! `PersistentEntitySectionManager`): entities leave the level with their
//! chunk, come back when it loads, and saves keep them all.
use glam::{DVec3, IVec3};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use voxelcraft::{
    entity::{Ctx, Entities, Mob, MobKind, PlayerId, Target, leash::Leash},
    inventory::{Stack, StackName},
    item::Item,
    simulation::{self, TICK_SECONDS},
    world::{
        World,
        storage::{LevelInfo, Storage},
        terrain::{Dimension, Generator},
    },
};

const FIXTURE: &str = include_str!("fixtures/entity_storage/level-77a6885.txt");

/// Streams the world around `at` until generation and lighting are done.
fn settle(world: &mut World, at: DVec3) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        world.update(at);
        if world.loaded_chunks() > 0 && world.pending_jobs() == 0 && world.light_settled() {
            return;
        }
        assert!(Instant::now() < deadline, "headless streaming did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn ctx(pos: DVec3) -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, pos, false)],
        daylight: 1.0,
        spawn_light: simulation::lighting::SpawnLight::DAY,
        raining: false,
        spawning: false,
        dimension: Dimension::Overworld,
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "voxelcraft-entity-storage-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ))
}

/// Save properties with each entity list in a fixed order.
fn normalized(props: &BTreeMap<String, String>) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for key in ["items", "orbs", "minecarts"] {
        let mut parts: Vec<_> = props[key].split(';').filter(|s| !s.is_empty()).map(str::to_string).collect();
        parts.sort();
        out.insert(key.to_string(), parts);
    }
    for key in ["villagers", "nether_mobs"] {
        let root: serde_json::Value = serde_json::from_str(&props[key]).unwrap();
        let mut mobs: Vec<String> = root["mobs"].as_array().unwrap().iter().map(|m| m.to_string()).collect();
        mobs.sort();
        out.insert(key.to_string(), mobs);
    }
    out
}

#[test]
fn a_save_in_the_v0_8_layout_loads_with_every_entity_and_reloads_each_chunk() {
    // Written by the serializers at 77a6885, before per-chunk storage.
    let dir = temp_dir("fixture");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("level.txt"), FIXTURE).unwrap();
    let level = Storage::new(&dir).load_level().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let props = level.props;
    let fixture = normalized(&props);

    let mut world = World::new_headless(Arc::new(Generator::new(level.seed)), Default::default(), 2);
    let mut e = Entities::new(level.seed);
    e.load_props(&props);
    world.load_falling(&props["falling"]);
    let total = e.mobs.len() + e.items.len() + e.orbs.len() + e.minecarts.len();
    assert_eq!((e.mobs.len(), e.items.len(), e.minecarts.len()), (8, 2, 3));
    settle(&mut world, DVec3::new(0.5, 90.0, 0.5));
    e.sync_chunk_entities(&mut world);
    let live = e.mobs.len() + e.items.len() + e.orbs.len() + e.minecarts.len();
    assert!(live > 0 && e.stored_entities() > 0, "{live} live, {} stored", e.stored_entities());
    assert_eq!(live + e.stored_entities(), total, "none lost or duplicated");
    assert_eq!(world.falling_blocks().len() + world.stored_falling_blocks(), 1);
    // Saving now, with most entities in unloaded chunks, writes the same entities.
    let mut resaved = BTreeMap::new();
    e.save_props(&mut resaved);
    assert_eq!(normalized(&resaved), fixture);

    // Near the origin: the tamed wolf, the baby cow and the chest cart are live.
    let wolf = e.mobs.iter().find(|m| m.kind == MobKind::Wolf).expect("wolf");
    assert_eq!((wolf.name.as_str(), wolf.health), (Some("Rex"), 13.0));
    let tame = wolf.animal.as_ref().unwrap();
    assert_eq!((tame.owner, tame.sitting), (Some(PlayerId::HOST), true));
    let cow = e.mobs.iter().find(|m| m.kind == MobKind::Cow).unwrap();
    assert!(cow.baby && cow.age == -12_000);
    let chest_cart = e.minecarts.iter().find(|c| c.id == 1).expect("chest cart");
    assert_eq!(chest_cart.slots[4], Some(Stack::new(Item::DIAMOND, 5)));

    // Visiting each saved spot brings its entities back with their state.
    let spots = [
        (300.5, 300.5),
        (-500.5, -500.5),
        (1000.5, 1000.5),
        (-130.5, -130.5),
        (-70.5, 100.5),
        (-200.0, 333.0),
        (-64.5, 64.5),
    ];
    for (i, spot) in spots.into_iter().chain([(-300.0, 40.0)]).enumerate() {
        let at = DVec3::new(spot.0, 90.0, spot.1);
        settle(&mut world, at);
        e.sync_chunk_entities(&mut world);
        let near = |p: DVec3| (p - at).with_y(0.0).length() < 40.0;
        assert!(!e.stored_mobs().any(|m| near(m.pos)), "mobs near {spot:?} returned");
        assert_eq!(e.mobs.len() + e.items.len() + e.orbs.len() + e.minecarts.len() + e.stored_entities(), total);
        let find = |kind| e.mobs.iter().find(|m| m.kind == kind && near(m.pos)).unwrap_or_else(|| panic!("{kind:?}"));
        match i {
            0 => {
                let s = find(MobKind::Donkey).mount.as_ref().unwrap();
                assert!(s.tame && s.chest && s.owner == Some(PlayerId::HOST));
                assert_eq!(s.slots[2], Some(Stack::new(Item::DIAMOND, 7)));
            }
            1 => assert!(find(MobKind::Piglin).persistent),
            2 => {
                let villager = find(MobKind::Villager);
                assert_eq!(villager.name, StackName::new("Bob").unwrap());
                let v = villager.villager.as_ref().unwrap();
                assert_eq!(
                    (v.home, v.job, v.xp),
                    (Some(IVec3::new(1002, 70, 1000)), Some(IVec3::new(998, 70, 1001)), 12)
                );
                assert!(find(MobKind::IronGolem).built);
            }
            3 => assert!(e.minecarts.iter().any(|c| near(c.pos)
                && c.slots[0] == Some(Stack::new(Item::from(voxelcraft::world::block::Block::STONE), 64)))),
            4 => {
                let sheep = find(MobKind::Sheep);
                assert!(sheep.sheared);
                assert_eq!(sheep.leash, Some(Leash::Fence(IVec3::new(-72, 64, 100))));
            }
            5 => assert!(e.items.iter().any(|item| near(item.pos) && item.stack == Stack::new(Item::DIAMOND, 3))),
            6 => {
                assert_eq!(find(MobKind::Pig).riding, Some(3), "the pig came back in its cart");
                assert!(e.minecarts.iter().any(|c| c.id == 3));
            }
            _ => assert_eq!(world.falling_blocks().len(), 1, "the falling sand loaded with its chunk"),
        }
    }
    let mut resaved = BTreeMap::new();
    e.save_props(&mut resaved);
    assert_eq!(normalized(&resaved), fixture);
}

#[test]
fn entities_saved_in_unloaded_chunks_survive_a_restart() {
    let dir = temp_dir("restart");
    let generator = Arc::new(Generator::new(42));
    let home = DVec3::new(0.5, 90.0, 0.5);
    let far = DVec3::new(900.5, 90.0, 0.5);
    let fingerprint = |m: &Mob| {
        format!(
            "{} {:?} {:?} {} {:?} {:?} {}",
            m.uid,
            m.kind,
            m.pos,
            m.health,
            m.name,
            m.animal.as_ref().map(|a| a.owner),
            m.age
        )
    };
    let before = {
        let mut world = World::new_headless(generator.clone(), Default::default(), 2);
        settle(&mut world, home);
        let mut e = Entities::new(42);
        let ground = world.surface_height(5, 5).unwrap() as f64 + 1.0;
        e.spawn(MobKind::Wolf, DVec3::new(5.5, ground, 5.5));
        let m = e.mobs.last_mut().unwrap();
        m.animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
        m.name = StackName::new("Scout").unwrap();
        m.health = 9.5;
        e.spawn_baby(MobKind::Sheep, DVec3::new(7.5, ground, 5.5));
        e.ensure_mob_ids();
        e.drop_from_block(Stack::new(Item::DIAMOND, 4), IVec3::new(6, ground as i32, 6));
        e.sync_chunk_entities(&mut world);
        let before: Vec<String> = e.mobs.iter().filter(|m| m.uid <= 2).map(fingerprint).collect();
        // Fly away: the entities leave the level, then the game saves.
        settle(&mut world, far);
        e.sync_chunk_entities(&mut world);
        assert!(e.stored_mobs().filter(|m| m.uid <= 2).count() == 2 && e.items.iter().all(|i| i.pos.x > 500.0));
        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        props.insert("falling".into(), world.falling_to_string());
        Storage::new(&dir).save_level(&LevelInfo { seed: 42, player: Some((far, 0.0, 0.0)), props }).unwrap();
        before
    };
    // A new process: the save loads far from the entities, which wait until the player returns.
    let level = Storage::new(&dir).load_level().unwrap();
    let mut world = World::new_headless(generator, Default::default(), 2);
    let mut e = Entities::new(42);
    e.load_props(&level.props);
    settle(&mut world, far);
    e.sync_chunk_entities(&mut world);
    assert_eq!(e.stored_mobs().filter(|m| m.uid <= 2).count(), 2);
    settle(&mut world, home);
    e.sync_chunk_entities(&mut world);
    let after: Vec<String> = e.mobs.iter().filter(|m| m.uid <= 2).map(fingerprint).collect();
    assert_eq!(after, before);
    assert!(e.items.iter().any(|i| i.stack == Stack::new(Item::DIAMOND, 4)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn flying_away_keeps_only_loaded_entities_live_and_never_regenerates_herds() {
    let mut world = World::new_headless(Arc::new(Generator::new(20261017)), Default::default(), 2);
    let mut e = Entities::new(7);
    // Storage chunks within the keep radius (render distance + 3) of the player stay loaded.
    let keep = (2.0 + 3.0) * 32.0 * std::f64::consts::SQRT_2 + 64.0;
    let visit = |e: &mut Entities, world: &mut World, at: DVec3| {
        settle(world, at);
        let c = ctx(at);
        for _ in 0..5 {
            simulation::tick_world(world, at);
            e.update(TICK_SECONDS, world, &c);
            e.sync_chunk_entities(world);
        }
        for m in &e.mobs {
            let away = (m.pos - at).with_y(0.0).length();
            assert!(away <= keep, "{:?} still live {away:.0} blocks away", m.kind);
        }
        (e.mobs.len(), e.mobs.len() + e.stored_mobs().count())
    };
    let origin = DVec3::new(0.5, 120.0, 0.5);
    let (_, start_total) = visit(&mut e, &mut world, origin);
    let near = |e: &Entities| -> BTreeSet<u32> {
        e.mobs.iter().filter(|m| (m.pos - origin).with_y(0.0).length() < 48.0).map(|m| m.uid).collect()
    };
    e.ensure_mob_ids();
    let first = near(&e);
    let mut peak_live = 0;
    let mut out_total = start_total;
    for step in 1..=8 {
        let at = origin + DVec3::X * (step as f64 * 192.0);
        let (live, total) = visit(&mut e, &mut world, at);
        e.ensure_mob_ids();
        eprintln!("x = {:>5}: {live} live mobs, {total} in all, {} stored entities", at.x, e.stored_entities());
        peak_live = peak_live.max(live);
        out_total = total;
    }
    assert!(out_total > start_total, "the flight populated new chunks ({start_total} -> {out_total})");
    assert!(e.stored_entities() > 0);
    assert!(
        peak_live < out_total,
        "live mobs stay a loaded-area population ({peak_live}) while the total grows ({out_total})"
    );
    for step in (0..8).rev() {
        let at = origin + DVec3::X * (step as f64 * 192.0);
        let (_, total) = visit(&mut e, &mut world, at);
        assert_eq!(total, out_total, "returning over populated chunks adds no herds (step {step})");
    }
    assert_eq!(near(&e), first, "the mobs that were here came back");
}

#[test]
#[ignore = "manual CPU benchmark; run without concurrent builds"]
fn chunk_entity_sync_cost() {
    let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    let at = DVec3::new(0.5, 120.0, 0.5);
    settle(&mut world, at);
    let mut e = Entities::new(42);
    for n in 0..500 {
        let p = DVec3::new((n % 25) as f64 * 4.0 - 48.0, 100.0, (n / 25) as f64 * 4.0 - 40.0);
        e.spawn(MobKind::Cow, p);
        e.drop_from_block(Stack::new(Item::DIAMOND, 1), p.floor().as_ivec3());
    }
    e.sync_chunk_entities(&mut world);
    assert_eq!((e.mobs.len(), e.items.len(), e.stored_entities()), (500, 500, 0));
    let mut samples = Vec::new();
    for _ in 0..2000 {
        let start = Instant::now();
        e.sync_chunk_entities(&mut world);
        samples.push(start.elapsed().as_secs_f64() * 1e6);
    }
    samples.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    eprintln!(
        "sync with 500 live mobs and 500 items, nothing to move: mean {mean:.2} us, p99 {:.2} us, max {:.2} us",
        samples[1979], samples[1999]
    );
    // Flying away: everything leaves at once.
    settle(&mut world, DVec3::new(2000.5, 120.0, 0.5));
    let start = Instant::now();
    e.sync_chunk_entities(&mut world);
    let store = start.elapsed().as_secs_f64() * 1e6;
    assert_eq!((e.mobs.len(), e.stored_entities()), (0, 1000));
    settle(&mut world, at);
    let start = Instant::now();
    e.sync_chunk_entities(&mut world);
    let restore = start.elapsed().as_secs_f64() * 1e6;
    assert_eq!(e.stored_entities(), 0);
    eprintln!("storing 1000 entities: {store:.1} us; restoring them: {restore:.1} us");
}

/// Streams for the host plus agent and split-screen players until settled.
fn settle_players(world: &mut World, host: DVec3, agents: &[DVec3], viewers: &[DVec3]) {
    world.set_viewers(viewers);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        world.update_players(host, agents);
        if world.loaded_chunks() > 0 && world.pending_jobs() == 0 && world.light_settled() {
            return;
        }
        assert!(Instant::now() < deadline, "headless streaming did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn agents_and_split_screen_viewers_keep_their_entities_in_the_level() {
    let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    let host = DVec3::new(0.5, 120.0, 0.5);
    let agent = DVec3::new(1000.5, 120.0, 0.5);
    let viewer = DVec3::new(0.5, 120.0, 1000.5);
    let mut e = Entities::new(42);
    e.spawn(MobKind::Cow, agent + DVec3::new(10.0, 0.0, 0.0));
    e.spawn(MobKind::Sheep, viewer + DVec3::new(0.0, 0.0, 10.0));
    e.spawn(MobKind::Pig, DVec3::new(-1000.5, 120.0, 0.5));
    settle_players(&mut world, host, &[agent], &[viewer]);
    e.sync_chunk_entities(&mut world);
    let live = |e: &Entities| e.mobs.iter().map(|m| m.kind).collect::<Vec<_>>();
    assert_eq!(live(&e), [MobKind::Cow, MobKind::Sheep], "only the pig is far from every player");
    // The agent disconnects and the split-screen view closes.
    settle_players(&mut world, host, &[], &[]);
    e.sync_chunk_entities(&mut world);
    assert!(e.mobs.is_empty());
    assert_eq!(e.stored_entities(), 3);
}
