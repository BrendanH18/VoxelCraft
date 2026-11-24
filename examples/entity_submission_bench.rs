//! v0.18 Track 6: the CPU side of entity submission per frame, headless.
//!
//! A scene like the `entities` benchmark scene (160 cows, sheep, pigs,
//! chickens, zombies, skeletons, wolves and horses on generated terrain,
//! plus dropped items), with the camera among them. For each of 2,000
//! frames it times `Entities::mesh` (the per-frame model build), the
//! partition the renderer used to run over the finished triangles to put
//! blended ones last (a scan, and with any blended triangle a full copy),
//! and `Entities::shadows` (the ground shadows), and prints the median,
//! 99th percentile and mean of each in microseconds, with the geometry sizes.
//!
//! `cargo run --profile ci --example entity_submission_bench`

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, Vec3};
use voxelcraft::entity::model::EntityVertex;
use voxelcraft::entity::{Ctx, Entities, MobKind, PlayerId, Target};
use voxelcraft::inventory::Stack;
use voxelcraft::simulation::TICK_SECONDS;
use voxelcraft::world::World;
use voxelcraft::world::terrain::{Dimension, Generator};

const FRAMES: usize = 2000;

fn stats(name: &str, mut us: Vec<f64>) {
    us.sort_by(f64::total_cmp);
    let mean = us.iter().sum::<f64>() / us.len() as f64;
    let at = |q: f64| us[((us.len() - 1) as f64 * q).round() as usize];
    println!("{name:<34} median {:>8.1} us   p99 {:>8.1} us   mean {:>8.1} us", at(0.5), at(0.99), mean);
}

/// The renderer's partition before this track (`Renderer::set_entities`
/// at 39fe5bc): blended triangles (material 4) moved after the rest.
fn old_partition(verts: &[EntityVertex], scratch: &mut Vec<EntityVertex>) -> usize {
    if verts.iter().any(|v| v.torch[1] >= 4) {
        scratch.clear();
        scratch.extend(verts.iter().filter(|v| v.torch[1] < 4));
        let opaque = scratch.len();
        scratch.extend(verts.iter().filter(|v| v.torch[1] >= 4));
        opaque
    } else {
        verts.len()
    }
}

fn main() {
    let generator = Arc::new(Generator::for_dimension(12345, Dimension::Overworld));
    let spawn = generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let mut world = World::new_headless(generator.clone(), Default::default(), 2);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        world.update(spawn);
        if world.loaded_chunks() > 0 && world.pending_jobs() == 0 {
            break;
        }
        assert!(Instant::now() < deadline, "terrain streaming timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    // Spread the skylight the shadows read (the game does this as it streams).
    world.update_block_light();
    let kinds = [
        MobKind::Cow,
        MobKind::Sheep,
        MobKind::Pig,
        MobKind::Chicken,
        MobKind::Zombie,
        MobKind::Skeleton,
        MobKind::Wolf,
        MobKind::Horse,
    ];
    let mut entities = Entities::new(12345);
    for i in 0..160 {
        let (x, z) = ((i % 16) as i32 * 2 - 16, (i / 16) as i32 * 2 - 10);
        let (wx, wz) = (spawn.x as i32 + x, spawn.z as i32 + z);
        let y = generator.column(wx, wz).height + 1;
        entities.spawn(kinds[i % kinds.len()], DVec3::new(wx as f64 + 0.5, y as f64, wz as f64 + 0.5));
    }
    for i in 0..24 {
        let (wx, wz) = (spawn.x as i32 + (i % 6) * 2 - 6, spawn.z as i32 - 4 - (i / 6));
        let y = generator.column(wx, wz).height + 1;
        let stack = Stack::new(voxelcraft::item::Item::APPLE, 1 + i as u8);
        entities.throw(stack, DVec3::new(wx as f64 + 0.5, y as f64 + 1.5, wz as f64 + 0.5), DVec3::NEG_Y);
    }
    let ctx = Ctx {
        spawn_light: voxelcraft::simulation::lighting::SpawnLight::DAY,
        players: vec![Target::new(PlayerId::HOST, spawn + DVec3::new(0.0, 0.0, -40.0), false)],
        daylight: 1.0,
        raining: false,
        spawning: false,
        dimension: Dimension::Overworld,
    };
    // Let them land and spread out.
    for _ in 0..40 {
        entities.snapshot_positions();
        entities.update(TICK_SECONDS, &world, &ctx);
    }
    let ground = spawn.y.max(f64::from(generator.column(spawn.x as i32, spawn.z as i32).height));
    let mut scratch = Vec::new();
    let (mut mesh_us, mut old_us, mut old_shadow_us, mut shadow_us) = (vec![], vec![], vec![], vec![]);
    let (mut boxes, mut verts, mut blended) = (0, 0, 0);
    for frame in 0..FRAMES {
        // A slow sweep around the herd, a few blocks up, as the scene's camera does.
        let a = frame as f64 / FRAMES as f64 * std::f64::consts::TAU;
        let camera = spawn + DVec3::new(a.cos() * 10.0, ground - spawn.y + 4.0, a.sin() * 10.0 - 4.0);
        let forward = (spawn - camera).as_vec3().normalize_or(Vec3::X);
        let alpha = (frame % 7) as f64 / 7.0;
        let t = Instant::now();
        let mesh = entities.mesh(camera, forward, 128.0, 3.0, alpha);
        mesh_us.push(t.elapsed().as_secs_f64() * 1e6);
        let t = Instant::now();
        std::hint::black_box(old_partition(mesh, &mut scratch));
        old_us.push(t.elapsed().as_secs_f64() * 1e6);
        let t = Instant::now();
        let mesh = entities.shadows(&world, camera, alpha, &[], true);
        shadow_us.push(t.elapsed().as_secs_f64() * 1e6);
        // What the old partition would have cost with the shadows among the triangles.
        let mut joined: Vec<EntityVertex> = mesh.to_vec();
        joined.extend_from_slice(mesh.translucent());
        let t = Instant::now();
        std::hint::black_box(old_partition(&joined, &mut scratch));
        old_shadow_us.push(t.elapsed().as_secs_f64() * 1e6);
        boxes += mesh.boxes().len();
        verts += mesh.len();
        blended += mesh.translucent().len();
    }
    println!(
        "{} mobs, {} items; per frame: {} boxes, {} triangle vertices, {} blended (shadow) vertices",
        entities.mobs.len(),
        entities.items.len(),
        boxes / FRAMES,
        verts / FRAMES,
        blended / FRAMES
    );
    stats("Entities::mesh (model build)", mesh_us);
    stats("old partition, no shadows", old_us);
    stats("old partition, with shadows", old_shadow_us);
    stats("Entities::shadows (casters, quads)", shadow_us);
    println!("new partition: none (blended triangles are their own list)");
}
