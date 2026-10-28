//! Time `World::explode` on generated terrain: single TNT-sized blasts at the
//! surface and underground, and a 64-block TNT cube going off one after
//! another as a chain does. Uses only the `World::explode` API, so the same
//! file measures older builds too.
//!
//! `cargo run --release --no-default-features --example explosion_bench`
use glam::{DVec3, IVec3};
use std::{sync::Arc, time::Instant};
use voxelcraft::world::{World, block::Block, terrain::Generator};

const SEED: u64 = 263;

fn loaded() -> World {
    let generator = Arc::new(Generator::new(SEED));
    let mut world = World::new_remote(generator.clone(), 4);
    for x in -3..3 {
        for z in -3..3 {
            for y in generator.dimension.chunk_rows() {
                let pos = IVec3::new(x, y, z);
                world.receive_chunk(pos, Arc::new(generator.generate(pos)));
            }
        }
    }
    world
}

fn surface(world: &World, x: i32, z: i32) -> i32 {
    (world.min_y()..world.max_y())
        .rev()
        .find(|&y| world.get_block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid()))
        .unwrap()
}

fn report(name: &str, times: &mut [f64], removed: usize) {
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "{name}: {} blasts, mean {mean:.3} ms, median {:.3} ms, max {:.3} ms, {removed} blocks removed",
        times.len(),
        times[times.len() / 2],
        times[times.len() - 1]
    );
}

fn main() {
    // The per-state resistance and loot tables build on first use.
    let start = Instant::now();
    std::hint::black_box(Block::STONE.blast_resistance());
    println!("tables: {:.3} ms", start.elapsed().as_secs_f64() * 1000.0);
    for (name, depth) in [("surface", 0), ("underground", 24)] {
        let mut world = loaded();
        let (mut times, mut removed) = (Vec::new(), 0);
        for i in 0..64 {
            let (x, z) = (-24 + (i % 8) * 6, -24 + (i / 8) * 6);
            let y = surface(&world, x, z) + 1 - depth;
            let start = Instant::now();
            removed += world.explode(DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5), 4.0);
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            world.drops.clear();
            world.primed_tnt.clear();
        }
        report(name, &mut times, removed);
    }
    let mut world = loaded();
    let base = surface(&world, 0, 0) + 1;
    let cells: Vec<IVec3> = (0..64).map(|i| IVec3::new(i % 4, base + i / 16, (i / 4) % 4)).collect();
    for &c in &cells {
        world.set_block(c, Block::TNT);
    }
    let (mut times, mut removed) = (Vec::new(), 0);
    let start_all = Instant::now();
    for &c in &cells {
        let start = Instant::now();
        removed += world.explode(c.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), 4.0);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        world.drops.clear();
        world.primed_tnt.clear();
    }
    let total = start_all.elapsed().as_secs_f64() * 1000.0;
    report("64-TNT cube chain", &mut times, removed);
    println!("64-TNT cube chain total: {total:.1} ms");
}
