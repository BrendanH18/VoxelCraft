//! Device-free structure terrain timing; fresh generators, all vertical rows.
use glam::{IVec2, IVec3};
use std::{hint::black_box, time::Instant};
use voxelcraft::world::terrain::{Dimension, Generator};
fn main() {
    if std::env::args().any(|a| a == "--locations") {
        use voxelcraft::world::{temples::Kind, village::Style};
        for (seed, style) in [(7, Style::Desert), (42, Style::Taiga)] {
            let g = Generator::new(seed);
            let at = g.villages.locate_style(&g, IVec3::ZERO, style, 24).unwrap();
            let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * voxelcraft::world::village::SPACING));
            let v = g.villages.get(&g, region).unwrap();
            println!("{style:?} seed {seed} locate {at}, centre {}, bounds {:?}", v.center, v.bounds);
        }
        let g = Generator::new(42);
        let p = g.temples.nearest(&g, Kind::Outpost, IVec2::ZERO, 20_000).unwrap();
        println!("outpost seed 42 locate {p}");
        for b in g.temples.near(&g, p, 0).iter().filter(|b| b.kind == Kind::Outpost) {
            println!("outpost bounds {:?}", b.bounds);
        }
        return;
    }
    let seed = 12345;
    let g = Generator::new(seed);
    let village = g.villages.nearest(&g, IVec3::ZERO).unwrap();
    println!("seed {seed}, village {village}");
    for (name, at) in [("open", IVec3::ZERO), ("village", village)] {
        let centre = IVec2::new(at.x.div_euclid(32), at.z.div_euclid(32));
        for run in 0..5 {
            let g = Generator::new(seed);
            let start = Instant::now();
            let mut n = 0;
            for z in -2..=2 {
                for x in -2..=2 {
                    for y in Dimension::Overworld.chunk_rows() {
                        black_box(g.generate(IVec3::new(centre.x + x, y, centre.y + z)));
                        n += 1;
                    }
                }
            }
            println!("{name} run {run}: {n} chunks, {:.3} ms/chunk", start.elapsed().as_secs_f64() * 1000.0 / n as f64);
        }
    }
}
