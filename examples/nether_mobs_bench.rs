//! Entity-update cost of the Nether mobs against zombies on a flat floor:
//! `cargo run --release --example nether_mobs_bench`.

use glam::{DVec3, IVec3};
use std::time::Instant;
use voxelcraft::entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, Target};
use voxelcraft::physics::BlockSource;
use voxelcraft::world::block::Block;
use voxelcraft::world::terrain::Dimension;

/// Stone below y = 10, air above, everything loaded.
struct Flat;

impl BlockSource for Flat {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(if p.y < 10 { Block::STONE } else { Block::AIR })
    }
}

impl MobWorld for Flat {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(9)
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
}

/// Median milliseconds per 50 ms update for `count` mobs from `kinds`.
fn run(kinds: &[MobKind], count: usize) -> f64 {
    let mut runs = Vec::new();
    for seed in 0..5 {
        let mut e = Entities::new(seed);
        for i in 0..count {
            let (x, z) = ((i % 20) as f64 * 3.0 - 30.0, (i / 20) as f64 * 3.0 - 30.0);
            e.spawn(kinds[i % kinds.len()], DVec3::new(x + 0.5, 10.0, z + 0.5));
        }
        // A player in gold at the centre: piglins stay calm, the rest give chase.
        let mut player = Target::new(PlayerId::HOST, DVec3::new(0.5, 10.0, 0.5), true);
        player.gold_armor = true;
        let ctx =
            Ctx { players: vec![player], daylight: 0.0, spawning: false, raining: false, dimension: Dimension::Nether };
        let start = Instant::now();
        for _ in 0..400 {
            std::hint::black_box(e.update(0.05, &Flat, &ctx));
        }
        runs.push(start.elapsed().as_secs_f64() * 1000.0 / 400.0);
    }
    runs.sort_by(f64::total_cmp);
    runs[runs.len() / 2]
}

fn main() {
    for count in [100, 300] {
        let zombies = run(&[MobKind::Zombie], count);
        let nether =
            run(&[MobKind::Piglin, MobKind::PiglinBrute, MobKind::Hoglin, MobKind::Zoglin, MobKind::Strider], count);
        println!("{count} mobs: zombies {zombies:.3} ms/update, nether mobs {nether:.3} ms/update");
    }
}
