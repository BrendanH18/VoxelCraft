//! Device-free `/locate` for every Overworld structure of the latest world
//! generation, the nearest desert well, and the Nether's fossils and
//! bastions, so a scripted capture can be pointed at each one:
//! `cargo run --release --no-default-features --example locate_structures -- [seed]`
//! prints the nearest of each to the origin, one per line.
use glam::{IVec2, IVec3};
use voxelcraft::world::{
    relics,
    temples::Kind,
    terrain::{Dimension, Generator},
    trail_ruins,
};

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
        ("desert_well", g.relics.find(&g, relics::Kind::Well, flat, 160)),
    ];
    found.extend(Kind::ALL.into_iter().map(|k| (k.name(), g.temples.nearest(&g, k, flat, 6400))));
    println!("seed {seed}, generation {:?}", g.generation);
    for (name, at) in &found {
        match at {
            Some(p) => println!("{name} {} {} {}", p.x, p.y, p.z),
            None => println!("{name} none within range"),
        }
    }
    // The buried treasure chest's depth, which `/locate` doesn't give.
    if let Some(at) = found.iter().find(|(n, _)| *n == "buried_treasure").and_then(|f| f.1) {
        let chunk = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16));
        for (p, b) in g.buried_treasure.blocks(&g, chunk) {
            if b.name().contains("chest") {
                println!("buried_treasure_chest {} {} {}", p.x, p.y, p.z);
            }
        }
    }
    // The buried trail ruin's pieces, for cut-away poses.
    if let Some(at) = found.iter().find(|(n, _)| *n == "trail_ruins").and_then(|f| f.1) {
        let home = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * trail_ruins::SPACING));
        let regions = (-1..=1).flat_map(|z| (-1..=1).map(move |x| home + IVec2::new(x, z)));
        if let Some(ruin) = regions.filter_map(|r| g.trail_ruins.get(&g, r)).find(|r| r.at == at) {
            for p in &ruin.pieces {
                let (lo, hi) = (p.bounds.min, p.bounds.max);
                let ground = g.column((lo.x + hi.x) / 2, (lo.z + hi.z) / 2).height;
                println!("trail_ruins_piece {} {} {} to {} {} {} ground {ground}", lo.x, lo.y, lo.z, hi.x, hi.y, hi.z);
            }
        }
    }
    // The Nether is a separate generator. Bastions print their kind and the
    // centre of their bounds at floor level, nearest first.
    let nether = Generator::for_dimension(seed, Dimension::Nether);
    match nether.nearest_nether_fossil(flat, 3200) {
        Some(p) => println!("nether_fossil {} {} {}", p.x, p.y, p.z),
        None => println!("nether_fossil none within range"),
    }
    let mut bastions = nether.bastions_near(origin, 1500);
    let centre = |b: &voxelcraft::world::bastion::Bastion| (b.bounds.min + b.bounds.max) / 2;
    bastions.sort_by_key(|b| IVec2::new(centre(b).x, centre(b).z).as_i64vec2().length_squared());
    for b in bastions.iter().take(8) {
        let (c, kind) = (centre(b), format!("{:?}", b.kind).to_lowercase());
        println!("bastion_{kind} {} {} {}", c.x, b.bounds.min.y, c.z);
    }
}
