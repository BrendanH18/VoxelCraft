//! Device-free engine smoke run, not a network server.
//! cargo run --release --no-default-features --example headless
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::DVec3;
use voxelcraft::entity::{Ctx, Entities};
use voxelcraft::player::{MoveInput, Player};
use voxelcraft::simulation::{self, TICK_SECONDS, survival::Vitals, weather::Weather};
use voxelcraft::world::{
    World,
    terrain::{Dimension, Generator},
};

fn main() {
    let dimension = match std::env::args().nth(1).as_deref() {
        None => Dimension::Overworld,
        Some("--nether") => Dimension::Nether,
        _ => panic!("usage: headless [--nether]"),
    };
    let loading = Instant::now();
    let generator = Arc::new(Generator::for_dimension(12345, dimension));
    let spawn = generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let mut world = World::new_headless(generator, Default::default(), 2);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        world.update(spawn);
        if world.loaded_chunks() > 0 && world.is_idle() {
            break;
        }
        assert!(Instant::now() < deadline, "terrain streaming timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("{} streaming: {:.2} s", dimension.name(), loading.elapsed().as_secs_f64());
    let mut player = Player::new(spawn);
    let mut vitals = Vitals::default();
    let mut weather = Weather::new(12345);
    let mut entities = Entities::new(12345);
    let started = Instant::now();
    for _ in 0..200 {
        if dimension.has_sky() {
            weather.update(TICK_SECONDS);
        }
        world.raining = weather.raining;
        // Creative avoids damage; this smoke run exercises movement, world
        // systems and entity AI without duplicating the client's event handling.
        simulation::tick_player(&mut player, &world, &mut vitals, MoveInput::default(), true);
        simulation::tick_world(&mut world, player.pos);
        let ctx = Ctx {
            player_pos: player.pos,
            player_targetable: false,
            daylight: 1.0,
            raining: weather.raining,
            spawning: true,
            nether: dimension == Dimension::Nether,
        };
        entities.update(TICK_SECONDS, &world, &ctx);
        world.update(player.pos);
        assert!(world.mesh_uploads.is_empty() && world.mesh_removals.is_empty());
    }
    println!(
        "200 ticks (10 game seconds), {} loaded chunks, {} mobs, no render meshes; {:.2} ms/tick",
        world.loaded_chunks(),
        entities.mobs.len(),
        started.elapsed().as_secs_f64() * 1000.0 / 200.0,
    );
}
