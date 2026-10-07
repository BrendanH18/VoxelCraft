use glam::IVec3;
use std::{hint::black_box, time::Instant};
use voxelcraft::world::nether::NetherGen;

fn main() {
    for run in 0..5 {
        let generator = NetherGen::new(12345);
        let start = Instant::now();
        for x in -8..8 {
            for z in -8..8 {
                for y in 0..4 {
                    black_box(generator.generate(IVec3::new(x, y, z)));
                }
            }
        }
        println!(
            "Nether run {run}: {:.3} ms/chunk (1024 chunks, seed 12345)",
            start.elapsed().as_secs_f64() * 1000.0 / 1024.0
        );
    }
}
