//! Device-free structure terrain timing; fresh generators, all vertical rows.
use glam::{IVec2, IVec3};
use std::{hint::black_box, time::Instant};
use voxelcraft::world::terrain::{Dimension, Generator};
fn main() {
    if std::env::args().any(|a| a == "--probe-village") {
        probe_village();
        return;
    }
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
    // Pin the same scene across placement changes, e.g. --village=-208,400.
    let village = std::env::args()
        .find_map(|a| {
            let (x, z) = a.strip_prefix("--village=")?.split_once(',')?;
            let (x, z) = (x.parse::<i32>().ok()?, z.parse::<i32>().ok()?);
            Some(IVec3::new(x, g.column(x, z).height + 1, z))
        })
        .unwrap_or_else(|| g.villages.nearest(&g, IVec3::ZERO).unwrap());
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

/// A block-based regression probe usable on the parent generator too. Seed 42's
/// nearest plains village has wood roofs, so the natural-ground predicate does
/// not mistake sandstone/terracotta buildings for terrain.
fn probe_village() {
    use voxelcraft::world::{
        block::Block,
        chunk::CHUNK_SIZE_I,
        jigsaw::{self, Projection},
        village::SPACING,
    };
    for seed in [42, 12345, 2024] {
        let g = Generator::new(seed);
        let at = g.villages.nearest(&g, IVec3::ZERO).unwrap();
        let r = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * SPACING));
        let village = g.villages.get(&g, r).unwrap();
        let mut chunks = std::collections::HashMap::new();
        let mut building = std::collections::HashSet::new();
        for p in &village.placed {
            let t = &jigsaw::villages().templates[p.template as usize];
            if t.projection != Projection::Rigid {
                continue;
            }
            for c in &t.cells {
                if c.pos.y > 0 && c.spec.base != jigsaw::Base::Block(Block::AIR) {
                    building.insert(p.world(c.pos));
                }
            }
            for &(a, b, spec) in &t.fills {
                if spec.base == jigsaw::Base::Block(Block::AIR) {
                    continue;
                }
                for y in a.y.max(1)..=b.y {
                    for z in a.z..=b.z {
                        for x in a.x..=b.x {
                            building.insert(p.world(IVec3::new(x, y, z)));
                        }
                    }
                }
            }
        }
        let mut ring = 0;
        let mut worst = (0, 0, 0, 0);
        let mut checked = 0;
        for p in &village.placed {
            if jigsaw::villages().templates[p.template as usize].projection != Projection::Rigid {
                continue;
            }
            let b = p.bounds;
            for z in b.min.z - 1..=b.max.z + 1 {
                for x in b.min.x - 1..=b.max.x + 1 {
                    if x >= b.min.x && x <= b.max.x && z >= b.min.z && z <= b.max.z {
                        continue;
                    }
                    let h = (-64..=300)
                        .rev()
                        .find(|&y| {
                            if building.contains(&IVec3::new(x, y, z)) {
                                return false;
                            }
                            let c = IVec3::new(x, y, z).div_euclid(IVec3::splat(CHUNK_SIZE_I));
                            let l = IVec3::new(x, y, z).rem_euclid(IVec3::splat(CHUNK_SIZE_I));
                            let b = chunks.entry(c).or_insert_with(|| g.generate(c)).get(
                                l.x as usize,
                                l.y as usize,
                                l.z as usize,
                            );
                            matches!(
                                b,
                                Block::GRASS
                                    | Block::SNOWY_GRASS
                                    | Block::DIRT
                                    | Block::STONE
                                    | Block::SAND
                                    | Block::GRAVEL
                                    | Block::DIRT_PATH
                                    | Block::FARMLAND
                                    | Block::WET_FARMLAND
                                    | Block::RED_SAND
                                    | Block::SANDSTONE
                                    | Block::SNOW
                            ) || b.terracotta_colour().is_some()
                                || b == voxelcraft::world::overworld_blocks::PODZOL
                                || b == voxelcraft::world::overworld_blocks::COARSE_DIRT
                        })
                        .unwrap();
                    if (h - p.origin.y).abs() > ring {
                        ring = (h - p.origin.y).abs();
                        worst = (x, z, h, p.origin.y);
                    }
                    checked += 1;
                }
            }
        }
        println!(
            "seed {seed} village {at}: {} pieces, {checked} generated first-ring columns, max floor error {ring}",
            village.placed.len()
        );
        println!("worst ring (x,z,height,floor) {worst:?}");
        assert!(checked > 0 && ring <= 3, "generated village ring error {ring}");
    }
}
