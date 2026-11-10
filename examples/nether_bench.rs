//! Headless Nether chunk generation timing: 16×16 chunk columns, four rows
//! each (1,024 chunks), on a fresh generator per run.
//!
//! ```text
//! cargo run --release --example nether_bench                         # seed 12345 around the origin
//! cargo run --release --example nether_bench -- --seed 9 --at 716,141  # around seed 9's ledge fossil
//! ```
use glam::{IVec2, IVec3};
use std::{hint::black_box, time::Instant};
use voxelcraft::world::nether::NetherGen;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    let seed: u64 = value("--seed").map_or(12345, |s| s.parse().expect("--seed N"));
    let at = value("--at").map_or(IVec2::ZERO, |s| {
        let (x, z) = s.split_once(',').expect("--at X,Z");
        IVec2::new(x.parse().expect("X"), z.parse().expect("Z"))
    });
    let centre = at.div_euclid(IVec2::splat(16));
    for run in 0..5 {
        let generator = NetherGen::new(seed);
        let start = Instant::now();
        for x in -8..8 {
            for z in -8..8 {
                for y in 0..4 {
                    black_box(generator.generate(IVec3::new(centre.x + x, y, centre.y + z)));
                }
            }
        }
        println!(
            "Nether run {run}: {:.3} ms/chunk (1024 chunks, seed {seed}, around {},{})",
            start.elapsed().as_secs_f64() * 1000.0 / 1024.0,
            at.x,
            at.y
        );
    }
}
