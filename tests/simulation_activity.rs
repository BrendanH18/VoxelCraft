//! Loaded-region and fixed-tick regression fixtures (Java Edition 26.3).
use glam::{DVec3, IVec3};
use std::{sync::Arc, time::Instant};
use voxelcraft::{
    entity::{Ctx, Entities, MobKind, PlayerId, Target},
    inventory::Stack,
    item::Item,
    simulation::{self, TICK_SECONDS},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        terrain::{Dimension, Generator},
    },
};

fn fixture() -> (World, Entities, Ctx) {
    let mut world = World::new_remote(Arc::new(Generator::new(42)), 8);
    for z in -4..=4 {
        for x in -4..=4 {
            let mut data = ChunkData::Uniform(Block::AIR);
            for lz in 0..32 {
                for lx in 0..32 {
                    data.set(lx, 15, lz, Block::STONE);
                }
            }
            world.receive_chunk(IVec3::new(x, 4, z), Arc::new(data));
        }
    }
    for z in -4..=4 {
        for x in -4..=4 {
            let p = IVec3::new(x * 32 + 4, 144, z * 32 + 4);
            world.set_block(p, Block::FURNACE);
            let f = world.furnace_mut(p).unwrap();
            f.input = Some(Stack::new(Item::RAW_IRON, 64));
            f.fuel = Some(Stack::new(Item::COAL, 64));
        }
    }
    let mut entities = Entities::new(42);
    for n in 0..160 {
        entities.spawn(MobKind::Cow, DVec3::new((n % 16) as f64 * 4. + 0.5, 144., (n / 16) as f64 * 4. + 0.5));
        entities.mobs.last_mut().unwrap().persistent = true;
    }
    let ctx = context(DVec3::new(0.5, 144., 0.5));
    (world, entities, ctx)
}

fn context(pos: DVec3) -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, pos, false)],
        daylight: 0.,
        spawn_light: simulation::lighting::SpawnLight::NIGHT,
        raining: false,
        spawning: false,
        dimension: Dimension::Overworld,
    }
}

#[test]
#[ignore = "manual CPU tick benchmark; run without concurrent builds"]
fn representative_tick_cost() {
    let (mut world, mut entities, ctx) = fixture();
    for _ in 0..100 {
        simulation::tick_world(&mut world, ctx.players[0].pos);
        entities.update(TICK_SECONDS, &world, &ctx);
    }
    let mut samples = Vec::new();
    for _ in 0..1000 {
        let start = Instant::now();
        simulation::tick_world(&mut world, ctx.players[0].pos);
        world.tick_automation_entities(&mut entities);
        entities.update(TICK_SECONDS, &world, &ctx);
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    samples.sort_by(f64::total_cmp);
    eprintln!(
        "81 loaded platform chunks, 81 burning furnaces, 160 cows: mean {mean:.4} ms; p99 {:.4} ms; max {:.4} ms",
        samples[989], samples[999]
    );
}

fn small_world() -> World {
    let mut world = World::new_remote(Arc::new(Generator::new(42)), 8);
    for x in -8..=8 {
        let mut data = ChunkData::Uniform(Block::AIR);
        for z in 0..32 {
            for x in 0..32 {
                data.set(x, 15, z, Block::STONE);
            }
        }
        world.receive_chunk(IVec3::new(x, 4, 0), Arc::new(data));
    }
    world.set_simulation_distance(5);
    world.set_simulation_players([DVec3::new(0.5, 144., 0.5)]);
    world
}

#[test]
fn java_ticket_tiers_split_storage_chunks_and_use_floor_for_negative_positions() {
    let mut world = small_world();
    for (x, entity, block) in [
        (-97, false, false),
        (-96, false, true),
        (-81, false, true),
        (-80, true, true),
        (95, true, true),
        (96, false, true),
        (111, false, true),
        (112, false, false),
    ] {
        let p = IVec3::new(x, 144, 4);
        assert!(world.is_loaded(p));
        assert_eq!(world.is_entity_ticking(p), entity, "entity x={x}");
        assert_eq!(world.is_block_ticking(p), block, "block x={x}");
    }
    world.set_render_distance(32);
    assert!(!world.is_block_ticking(IVec3::new(112, 144, 4)));
    world.set_simulation_players([DVec3::new(-0.01, 144., 0.5)]);
    assert!(world.is_entity_ticking(IVec3::new(-96, 144, 4)));
    assert!(!world.is_entity_ticking(IVec3::new(80, 144, 4)));
    world.set_simulation_distance(10);
    assert_eq!(world.simulation_distance(), 10);
    assert!(world.is_entity_ticking(IVec3::new(159, 144, 4)));
    assert!(!world.is_entity_ticking(IVec3::new(160, 144, 4)));
}

fn furnace(world: &mut World, at: IVec3) {
    world.set_block(at, Block::FURNACE);
    let f = world.furnace_mut(at).unwrap();
    f.input = Some(Stack::new(Item::RAW_IRON, 1));
    f.fuel = Some(Stack::new(Item::COAL, 1));
}

#[test]
fn rendering_only_chunks_freeze_containers_mobs_items_fluids_fire_and_falling_blocks() {
    use voxelcraft::{potion::Potion, simulation::effects::Effect, world::redstone_blocks as r};
    let mut world = small_world();
    let active = IVec3::new(100, 144, 4); // block-only ring
    let sleeping = IVec3::new(116, 144, 4); // same native storage chunk
    for p in [active, sleeping] {
        furnace(&mut world, p);
        world.set_block(p + IVec3::Z * 2, Block::BREWING_STAND);
        let b = world.brewing_stand_mut(p + IVec3::Z * 2).unwrap();
        b.bottles[0] = Some(Stack::new(Item::potion(Potion::AWKWARD), 1));
        b.ingredient = Some(Stack::new(Item::PUFFERFISH, 1));
        b.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
        world.set_block(p + IVec3::Z * 4, r::hopper(0, false));
        world.set_block(p + IVec3::Z * 4 + IVec3::Y, Block::CHEST);
        world.chest_mut(p + IVec3::Z * 4 + IVec3::Y).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 8));
    }
    let frozen_furnace = world.furnace(sleeping).unwrap().serialize();
    let frozen_brew = world.brewing_stand(sleeping + IVec3::Z * 2).unwrap().serialize();
    world.set_block(sleeping + IVec3::Z * 6, Block::WATER);
    world.set_block(sleeping + IVec3::Z * 8 - IVec3::Y, Block::NETHERRACK);
    world.set_block(sleeping + IVec3::Z * 8, Block::FIRE);
    let fire = world.get_block(sleeping + IVec3::Z * 8);
    world.set_block(sleeping + IVec3::Y * 8, Block::SAND);
    let falling = world.falling_to_string();
    let mut entities = Entities::new(42);
    for kind in [MobKind::Turtle, MobKind::Cow, MobKind::Piglin, MobKind::Villager, MobKind::Llama] {
        entities.spawn(kind, sleeping.as_dvec3() + DVec3::new(0.5, 0., 0.5));
        let m = entities.mobs.last_mut().unwrap();
        m.persistent = true;
        m.effects.add(Effect::Slowness, 0, 123);
        if let Some(a) = &mut m.animal {
            a.love = 10.;
        }
    }
    entities.mobs[0].baby = true;
    entities.mobs[0].age = -24000;
    entities.scatter(Stack::new(Item::EMERALD, 1), sleeping.as_dvec3() + DVec3::Y);
    entities.spawn_xp(sleeping.as_dvec3() + DVec3::Y, 5);
    entities.prime_tnt(sleeping + IVec3::Z * 10, false);
    let items = entities.items_to_string();
    let orbs = entities.orbs_to_string();
    let tnt = entities.tnt[0].pos;
    let ctx = context(DVec3::new(0.5, 144., 0.5));
    for _ in 0..420 {
        simulation::tick_world(&mut world, ctx.players[0].pos);
        world.tick_automation_entities(&mut entities);
        assert!(
            entities
                .update(TICK_SECONDS, &world, &ctx)
                .iter()
                .all(|e| !matches!(e, voxelcraft::entity::EntityEvent::Explosion { .. }))
        );
    }
    assert_eq!(world.furnace(active).unwrap().output, Some(Stack::new(Item::IRON_INGOT, 1)));
    assert!(world.brewing_stand(active + IVec3::Z * 2).unwrap().ingredient.is_none());
    assert!(world.chest(active + IVec3::Z * 4 + IVec3::Y).unwrap().slots[0].is_none());
    assert_eq!(world.furnace(sleeping).unwrap().serialize(), frozen_furnace);
    assert_eq!(world.brewing_stand(sleeping + IVec3::Z * 2).unwrap().serialize(), frozen_brew);
    assert_eq!(world.chest(sleeping + IVec3::Z * 4 + IVec3::Y).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 8)));
    assert_eq!(world.get_block(sleeping + IVec3::Z * 6 + IVec3::X), Some(Block::AIR));
    assert_eq!(world.get_block(sleeping + IVec3::Z * 8), fire);
    assert_eq!(world.falling_to_string(), falling);
    assert_eq!(entities.items_to_string(), items);
    assert_eq!(entities.orbs_to_string(), orbs);
    assert_eq!(entities.tnt[0].pos, tnt);
    assert_eq!(entities.mobs.len(), 5);
    for mob in &entities.mobs {
        assert_eq!(mob.pos, sleeping.as_dvec3() + DVec3::new(0.5, 0., 0.5));
        assert_eq!(mob.effects.get(Effect::Slowness).unwrap().ticks, 123);
        if let Some(a) = &mob.animal {
            assert_eq!(a.love, 10.);
        }
    }
    assert_eq!(entities.mobs[0].age, -24000);
    assert_eq!(entities.mobs[0].grow, 0.);
    // A returning player wakes work; two players in one region don't double it.
    world.set_simulation_players([ctx.players[0].pos, sleeping.as_dvec3()]);
    simulation::tick_world(&mut world, ctx.players[0].pos);
    entities.update(TICK_SECONDS, &world, &ctx);
    assert_ne!(world.furnace(sleeping).unwrap().serialize(), frozen_furnace);
    assert_eq!(entities.mobs[0].effects.get(Effect::Slowness).unwrap().ticks, 122);
}

#[test]
fn random_ticks_respect_the_java_section_boundary_and_overlapping_players() {
    // Nether wart needs no skylight and has no support/hydration randomness.
    fn run(players: &[DVec3]) -> (Vec<Block>, Vec<Block>) {
        let mut world = small_world();
        let cells: Vec<_> = [96, 112]
            .into_iter()
            .flat_map(|x| (0..16).flat_map(move |dx| (0..16).map(move |z| IVec3::new(x + dx, 144, z))))
            .collect();
        for &p in &cells {
            world.set_block(p - IVec3::Y, Block::SOUL_SAND);
            world.set_block(p, Block::nether_wart(0));
        }
        world.set_simulation_players(players.iter().copied());
        for _ in 0..200 {
            world.tick_random_speed(TICK_SECONDS, players[0], 300);
        }
        let blocks: Vec<_> = cells.iter().map(|&p| world.get_block(p).unwrap()).collect();
        (blocks[..256].to_vec(), blocks[256..].to_vec())
    }
    let at = DVec3::new(0.5, 144., 0.5);
    let (active, sleeping) = run(&[at]);
    assert!(active.iter().any(|&b| b != Block::nether_wart(0)));
    assert!(sleeping.iter().all(|&b| b == Block::nether_wart(0)));
    assert_eq!(run(&[at, at + DVec3::X]), (active, sleeping));
}

#[test]
fn seeded_world_outcomes_match_frame_pacing_and_pause_produces_no_ticks() {
    use std::time::Duration;
    let mut expected = None;
    for frames in [vec![5; 2000], vec![50; 200], vec![200; 50], [13, 37, 80, 20, 50].repeat(50)] {
        let mut world = small_world();
        let at = IVec3::new(4, 144, 4);
        furnace(&mut world, at);
        world.set_block(at + IVec3::Y * 8, Block::SAND);
        let mut entities = Entities::new(42);
        entities.spawn(MobKind::Cow, DVec3::new(10.5, 144., 10.5));
        entities.scatter(Stack::new(Item::DIAMOND, 1), DVec3::new(15.5, 148., 10.5));
        let ctx = context(at.as_dvec3());
        let mut clock = simulation::FixedClock::default();
        for ms in frames {
            for _ in 0..clock.advance(Duration::from_millis(ms), false) {
                simulation::tick_world(&mut world, ctx.players[0].pos);
                entities.update(TICK_SECONDS, &world, &ctx);
            }
        }
        assert_eq!(clock.ticks(), 200);
        let before = (
            world.furnaces_to_string(),
            world.falling_to_string(),
            entities.items_to_string(),
            entities.mobs[0].pos,
            entities.mobs[0].vel,
        );
        for _ in 0..100 {
            assert_eq!(clock.advance(Duration::from_millis(200), true), 0);
        }
        assert_eq!(clock.ticks(), 200);
        let after = (
            world.furnaces_to_string(),
            world.falling_to_string(),
            entities.items_to_string(),
            entities.mobs[0].pos,
            entities.mobs[0].vel,
        );
        assert_eq!(before, after);
        if let Some(expected) = &expected {
            assert_eq!(&after, expected);
        } else {
            expected = Some(after);
        }
    }
}

/// Java's `PLAYER_SIMULATION` ticket only simulates; it never loads chunks.
/// Only the view (render) distance streams terrain in.
#[test]
fn simulation_distance_does_not_load_chunks_beyond_the_render_distance() {
    let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    world.set_simulation_distance(10);
    let at = DVec3::new(0.5, 150.0, 0.5);
    let deadline = Instant::now() + std::time::Duration::from_secs(20);
    while !world.is_loaded(at.floor().as_ivec3()) || world.pending_jobs() > 0 {
        world.update(at);
        assert!(Instant::now() < deadline, "streaming did not settle");
        std::thread::yield_now();
    }
    // Java chunk 9 is inside the entity-ticking distance, but 4.5 storage chunks out.
    let far = IVec3::new(150, 150, 0);
    assert!(!world.is_loaded(far), "the simulation ticket loaded terrain");
    assert!(!world.is_entity_ticking(far));
}
