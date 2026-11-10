//! Device-free `/locate` for every Overworld structure of the latest world
//! generation, so a scripted capture can be pointed at each one:
//! `cargo run --release --no-default-features --example locate_structures -- [seed]`
//! prints the nearest of each to the origin, one per line.
use glam::{IVec2, IVec3};
use voxelcraft::world::{temples::Kind, terrain::Generator};

fn main() {
    let seed = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(42);
    let g = Generator::new(seed);
    let (flat, origin) = (IVec2::ZERO, IVec3::ZERO);
    let mut found: Vec<(&str, Option<IVec3>)> = vec![
        ("village", g.villages.nearest(&g, origin)),
        ("ancient_city", g.ancient_cities.nearest(&g, flat, 6400)),
        ("trial_chambers", g.trial_chambers.nearest(&g, flat, 6400)),
        ("trail_ruins", g.trail_ruins.nearest(&g, flat, 6400)),
        ("ruined_portal", g.ruined_portals.nearest(&g, origin, 10)),
        ("buried_treasure", g.buried_treasure.nearest_buried_treasure(&g, origin, 400)),
        ("ocean_ruin_warm", g.temples.nearest_ocean_ruin(&g, flat, 6400, true)),
        ("ocean_ruin_cold", g.temples.nearest_ocean_ruin(&g, flat, 6400, false)),
        ("stronghold", g.strongholds.nearest(origin)),
        ("mineshaft", g.mineshafts.nearest(origin)),
    ];
    found.extend(Kind::ALL.into_iter().map(|k| (k.name(), g.temples.nearest(&g, k, flat, 6400))));
    println!("seed {seed}, generation {:?}", g.generation);
    for (name, at) in found {
        match at {
            Some(p) => println!("{name} {} {} {}", p.x, p.y, p.z),
            None => println!("{name} none within range"),
        }
    }
}
