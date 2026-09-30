//! Headless benchmark: generates and meshes a full render-distance area,
//! single-threaded and then across all cores.

use std::sync::Arc;
use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::mesh::{self, Neighborhood};
use crate::world::chunk::{ChunkData, WORLD_HEIGHT_CHUNKS};
use crate::world::terrain::Generator;
use crate::world::World;

pub fn run(seed: u64, rd: i32) {
    let generator = Generator::new(seed);

    // Single-threaded throughput on a fixed column set.
    let r = 3;
    let positions: Vec<IVec3> = (-r..=r)
        .flat_map(|x| (-r..=r).flat_map(move |z| (0..WORLD_HEIGHT_CHUNKS).map(move |y| IVec3::new(x, y, z))))
        .collect();
    let t = Instant::now();
    let chunks: rustc_hash::FxHashMap<IVec3, Arc<ChunkData>> =
        positions.iter().map(|&p| (p, Arc::new(generator.generate(p)))).collect();
    let gen_time = t.elapsed();
    let dense = chunks.values().filter(|c| c.uniform().is_none()).count();
    println!(
        "generate (1 thread): {} chunks in {:.1} ms -> {:.3} ms/chunk ({} dense, {} uniform)",
        chunks.len(),
        gen_time.as_secs_f64() * 1e3,
        gen_time.as_secs_f64() * 1e3 / chunks.len() as f64,
        dense,
        chunks.len() - dense
    );

    let mut scratch = mesh::new_padded();
    let (mut meshed, mut quads) = (0, 0u64);
    let t = Instant::now();
    for (&p, c) in &chunks {
        if p.x.abs() == r || p.z.abs() == r || c.uniform().is_some() {
            continue;
        }
        let mut n: Neighborhood = Default::default();
        for (i, slot) in n.iter_mut().enumerate() {
            let i = i as i32;
            let o = IVec3::new(i % 3 - 1, i / 9 - 1, (i / 3) % 3 - 1);
            *slot = chunks.get(&(p + o)).cloned();
        }
        let m = mesh::mesh_neighborhood(&n, &mut scratch);
        quads += (m.vertices.len() / 4) as u64;
        meshed += 1;
    }
    let mesh_time = t.elapsed();
    println!(
        "mesh (1 thread): {} dense chunks in {:.1} ms -> {:.3} ms/chunk, {} quads ({:.0} bytes/chunk GPU)",
        meshed,
        mesh_time.as_secs_f64() * 1e3,
        mesh_time.as_secs_f64() * 1e3 / meshed.max(1) as f64,
        quads,
        quads as f64 * 16.0 / meshed.max(1) as f64
    );

    // Full streaming pipeline on the worker pool.
    let mut world = World::new(Arc::new(generator), Default::default(), rd);
    let t = Instant::now();
    let mut uploads = 0usize;
    loop {
        world.update(DVec3::new(0.0, 80.0, 0.0));
        uploads += world.mesh_uploads.drain(..).count();
        if world.pending_jobs() == 0 && uploads > 0 {
            // One more tick to make sure nothing new was scheduled.
            world.update(DVec3::new(0.0, 80.0, 0.0));
            uploads += world.mesh_uploads.drain(..).count();
            if world.pending_jobs() == 0 {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    println!(
        "stream rd={rd} on {} workers: {} chunks loaded, {} meshed in {:.2} s",
        world.worker_threads(),
        world.loaded_chunks(),
        uploads,
        t.elapsed().as_secs_f64()
    );
}
