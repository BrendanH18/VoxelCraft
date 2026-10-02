//! These use the public engine API and run with --no-default-features.
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};
use voxelcraft::entity::{Ctx, Entities, EntityEvent};
use voxelcraft::inventory::Stack;
use voxelcraft::item::Item;
use voxelcraft::player::{MoveInput, Player};
use voxelcraft::simulation::{self, FixedClock, TICK_SECONDS, survival::Vitals};
use voxelcraft::world::World;
use voxelcraft::world::block::Block;
use voxelcraft::world::chunk::ChunkData;
use voxelcraft::world::terrain::Generator;

fn settle(world: &mut World, at: DVec3) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        world.update(at);
        assert!(world.mesh_uploads.is_empty());
        assert!(world.mesh_removals.is_empty());
        if world.loaded_chunks() > 0 && world.is_idle() {
            return;
        }
        assert!(Instant::now() < deadline, "headless streaming did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn platform() -> World {
    let mut data = ChunkData::Uniform(Block::AIR);
    for z in 0..32 {
        for x in 0..32 {
            data.set(x, 21, z, Block::STONE); // y=149
        }
    }
    let saved = [(IVec3::new(0, 4, 0), Arc::new(data))].into_iter().collect();
    let mut world = World::new_headless(Arc::new(Generator::new(7)), saved, 2);
    settle(&mut world, DVec3::new(2.5, 150.0, 2.5));
    world
}

#[test]
fn headless_world_runs_gameplay_without_render_meshes() {
    let mut world = platform();
    let at = DVec3::new(2.5, 150.0, 2.5);
    let furnace = IVec3::new(12, 150, 12);
    assert!(world.set_block(furnace, Block::FURNACE));
    let contents = world.furnace_mut(furnace).unwrap();
    contents.input = Some(Stack::new(Item::from_block(Block::IRON_ORE), 1));
    contents.fuel = Some(Stack::new(Item::COAL, 1));
    world.set_block(IVec3::new(14, 155, 14), Block::SAND);
    world.set_block(IVec3::new(18, 149, 18), Block::NETHERRACK);
    world.set_block(IVec3::new(18, 150, 18), Block::FIRE);
    world.set_block(IVec3::new(24, 150, 24), Block::WATER);

    let mut entities = Entities::new(7);
    entities.scatter(Stack::new(Item::DIAMOND, 1), DVec3::new(10.5, 153.0, 10.5));
    entities.prime_tnt(IVec3::new(28, 150, 28), false);
    let ctx =
        Ctx { player_pos: at, player_targetable: false, daylight: 1.0, raining: false, spawning: false, nether: false };
    let mut explosions = 0;
    for _ in 0..201 {
        simulation::tick_world(&mut world, at);
        for event in entities.update(TICK_SECONDS, &world, &ctx) {
            if let EntityEvent::Explosion { .. } = event {
                explosions += 1;
            }
        }
        world.update(at);
        assert!(world.mesh_uploads.is_empty());
        assert!(world.mesh_removals.is_empty());
    }
    assert_eq!(world.block_light(furnace), 13, "lit furnaces illuminate without mesh jobs");
    assert_eq!(world.block_light(IVec3::new(18, 150, 18)), 15);
    assert_eq!(world.furnace(furnace).unwrap().output, Some(Stack::new(Item::IRON_INGOT, 1)));
    assert_eq!(world.get_block(IVec3::new(14, 150, 14)), Some(Block::SAND));
    assert!(world.get_block(IVec3::new(18, 150, 18)).unwrap().is_fire());
    assert!(world.get_block(IVec3::new(25, 150, 24)).unwrap().is_water());
    assert_eq!(entities.items.len(), 1);
    assert!((entities.items[0].pos.y - 150.0).abs() < 0.02);
    assert_eq!(explosions, 1);
    assert!(entities.tnt.is_empty());
    assert_eq!(world.pending_jobs(), 0);

    // Unloading keeps edits without generating renderer removal messages.
    settle(&mut world, at + DVec3::X * 640.0);
    assert!(
        world
            .modified_chunks()
            .iter()
            .any(|(p, data)| { *p == IVec3::new(0, 4, 0) && data.get(14, 22, 14) == Block::SAND })
    );
}

#[test]
fn headless_explosion_removes_light_before_returning() {
    let mut world = platform();
    let lamp = IVec3::new(4, 150, 4);
    world.set_block(lamp, Block::GLOWSTONE);
    assert_eq!(world.block_light(lamp + IVec3::X), 14);
    assert!(world.explode(lamp.as_dvec3() + DVec3::splat(0.5), 2.0) > 0);
    assert_eq!(world.block_light(lamp + IVec3::X), 0, "explosions clear light immediately");
}

#[test]
fn fixed_steps_preserve_movement_and_survival_across_frame_rates() {
    let world = platform();
    let mut expected = None;
    for frame_ms in [1, 10, 25, 50, 100, 200] {
        let mut player = Player::new(DVec3::new(2.5, 150.0, 2.5));
        let mut vitals = Vitals::default();
        let mut clock = FixedClock::default();
        let input = MoveInput { forward: 1.0, sprint: true, ..Default::default() };
        for _ in 0..1000 / frame_ms {
            for _ in 0..clock.advance(Duration::from_millis(frame_ms), false) {
                let step = simulation::tick_player(&mut player, &world, &mut vitals, input, false);
                assert_eq!(step.hurts.fall, 0.0);
                assert_eq!(step.hurts.fire, 0.0);
            }
        }
        assert_eq!(clock.ticks(), 20);
        assert!(player.on_ground);
        assert!((7.5..8.5).contains(&player.pos.x), "sprinting accelerates toward six blocks/sec: {:?}", player.pos);
        assert!((player.vel.x - 6.0).abs() < 0.01);
        let state = (player.pos, player.vel, vitals.health, vitals.hunger);
        if let Some(expected) = expected {
            assert_eq!(state, expected);
        } else {
            expected = Some(state);
        }
    }
}
