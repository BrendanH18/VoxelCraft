//! Nether generation cost per biome patch:
//! `cargo run --release --example nether_patch_bench`.
//!
//! Seven fresh generators each generate 5×5 columns (all four vertical
//! chunks) around the block centre of a patch of each biome for seed 12345.

use glam::IVec3;
use std::{hint::black_box, time::Instant};
use voxelcraft::world::nether::NetherGen;

const PATCHES: [(&str, i32, i32); 5] = [
    ("wastes", 0, 0),
    ("soul sand valley", -2560, -4480),
    ("crimson forest", -4096, -4480),
    ("warped forest", -4480, -4480),
    ("basalt deltas", -1024, -4480),
];

fn main() {
    for (name, x, z) in PATCHES {
        let mut runs = Vec::new();
        for _ in 0..7 {
            let generator = NetherGen::new(12345);
            let (cx, cz) = (x.div_euclid(32), z.div_euclid(32));
            let start = Instant::now();
            for dx in -2..=2 {
                for dz in -2..=2 {
                    for y in 0..4 {
                        black_box(generator.generate(IVec3::new(cx + dx, y, cz + dz)));
                    }
                }
            }
            runs.push(start.elapsed().as_secs_f64() * 1000.0 / 100.0);
        }
        runs.sort_by(f64::total_cmp);
        println!(
            "{name}: {:.3} ms/chunk (median of 7 fresh generators, best {:.3}; 100 chunks)",
            runs[runs.len() / 2],
            runs[0]
        );
    }
}
