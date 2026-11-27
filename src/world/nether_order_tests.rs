//! Generation 7's decoration order (`world::nether_order`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use glam::{IVec2, IVec3};
use serde_json::Value;

use super::*;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::nether::LAVA_SEA;
use crate::world::nether_biome::NetherBiome;
use crate::world::nether_biome_blocks::{self as nb, Vine};
use crate::world::terrain::Generation;

fn fixture() -> &'static Value {
    static DATA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/nether_generation.json"))
            .expect("nether generation fixture")
    })
}

/// `FeatureSorter.buildFeaturesPerStep` over the biomes' feature lists, in
/// the biome source's order: (step, name) in decoration order.
fn feature_sorter(biomes: &[&str]) -> Vec<(usize, String)> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    // Nodes keyed (step, index), each with the set of the next features.
    let mut edges: BTreeMap<(usize, usize), BTreeSet<(usize, usize)>> = BTreeMap::new();
    for biome in biomes {
        let steps = fixture()["biomes"][*biome]["features"].as_array().unwrap();
        let mut list = Vec::new();
        for (step, features) in steps.iter().enumerate() {
            for f in features.as_array().unwrap() {
                let f = f.as_str().unwrap().to_string();
                let next = index.len();
                let i = *index.entry(f.clone()).or_insert_with(|| {
                    names.push(f.clone());
                    next
                });
                list.push((step, i));
            }
        }
        for (k, node) in list.iter().enumerate() {
            let children = edges.entry(*node).or_default();
            if let Some(next) = list.get(k + 1) {
                children.insert(*next);
            }
        }
    }
    // `Graph.depthFirstSearch` from each node in key order; post order, reversed.
    fn visit(
        node: (usize, usize),
        edges: &BTreeMap<(usize, usize), BTreeSet<(usize, usize)>>,
        discovered: &mut BTreeSet<(usize, usize)>,
        visiting: &mut BTreeSet<(usize, usize)>,
        out: &mut Vec<(usize, usize)>,
    ) {
        if discovered.contains(&node) {
            return;
        }
        assert!(!visiting.contains(&node), "feature order cycle");
        visiting.insert(node);
        for &next in edges.get(&node).into_iter().flatten() {
            visit(next, edges, discovered, visiting, out);
        }
        visiting.remove(&node);
        discovered.insert(node);
        out.push(node);
    }
    let (mut discovered, mut visiting, mut out) = (BTreeSet::new(), BTreeSet::new(), Vec::new());
    for &node in edges.keys() {
        visit(node, &edges, &mut discovered, &mut visiting, &mut out);
    }
    out.reverse();
    // Each step's features in that order.
    let steps = out.iter().map(|n| n.0).max().unwrap();
    (0..=steps)
        .flat_map(|s| out.iter().filter(move |n| n.0 == s).map(|n| (n.0, names[n.1].clone())).collect::<Vec<_>>())
        .collect()
}

#[test]
fn features_run_in_javas_feature_sorter_order() {
    // The `nether` preset's biomes, in its order (`tools/JavaNetherVectors.java`
    // prints them so: wastes, soul sand valley, crimson, warped, deltas).
    let java =
        feature_sorter(&["nether_wastes", "soul_sand_valley", "crimson_forest", "warped_forest", "basalt_deltas"]);
    let names: Vec<&str> = java.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, JAVA_ORDER, "Java's order");
    for (i, (step, name)) in java.iter().enumerate() {
        let entry = &PLACED[order()[i]];
        assert_eq!((entry.name, usize::from(entry.step)), (name.as_str(), *step));
    }
    assert_eq!(order().iter().collect::<BTreeSet<_>>().len(), PLACED.len(), "every placed feature once");
}

/// Every block of the 2 x 2 x 4 chunks at a spot well inside `biome`.
fn area(g: &NetherGen, biome: NetherBiome) -> HashMap<IVec3, Block> {
    let inside = |p: IVec3| {
        [IVec3::ZERO, IVec3::X * 48, IVec3::Z * 48, IVec3::new(48, 0, 48)]
            .iter()
            .all(|&d| g.biomes.biome(p.x + d.x, p.z + d.z) == biome)
    };
    let at = (0..4000)
        .map(|i| IVec3::new((i % 63 - 31) * 64, 64, (i / 63 - 31) * 64))
        .find(|&p| inside(p))
        .expect("a large patch of the biome");
    let c = IVec3::new(at.x.div_euclid(CHUNK_SIZE_I), 0, at.z.div_euclid(CHUNK_SIZE_I));
    let mut blocks = HashMap::new();
    for dx in 0..2 {
        for dz in 0..2 {
            for cy in 0..4 {
                let cpos = c + IVec3::new(dx, cy, dz);
                let chunk = g.generate(cpos);
                for z in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            let p = cpos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32);
                            blocks.insert(p, chunk.get(x, y, z));
                        }
                    }
                }
            }
        }
    }
    blocks
}

fn count(blocks: &HashMap<IVec3, Block>, f: impl Fn(Block) -> bool) -> usize {
    blocks.values().filter(|&&b| f(b)).count()
}

/// Inside a cell later features see earlier ones: glowstone grows only
/// from glowstone (either feature's), vegetation and fungi only on nylium
/// left free, and vines only from wart or rock.
#[test]
fn later_features_build_on_earlier_ones() {
    let g = NetherGen::with_generation(2024, Generation::V7);
    let sides = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    for biome in NetherBiome::ALL {
        let blocks = area(&g, biome);
        let at = |p: IVec3| blocks.get(&p).copied();
        // Glowstone: every block touches glowstone or hangs from rock.
        let glow: Vec<_> = blocks.iter().filter(|(_, b)| **b == Block::GLOWSTONE).map(|(p, _)| *p).collect();
        let held = glow
            .iter()
            .filter(|&&p| sides.iter().any(|&d| at(p + d).is_none_or(|b| b == Block::GLOWSTONE || b.is_opaque())))
            .count();
        assert!(held * 1000 >= glow.len() * 998, "{biome:?}: {held} of {} glowstone held", glow.len());
        // Plants stand on what they need.
        let plants: Vec<_> = blocks
            .iter()
            .filter(|(_, b)| matches!(**b, nb::CRIMSON_ROOTS | nb::WARPED_ROOTS | nb::NETHER_SPROUTS))
            .map(|(p, b)| (*p, *b))
            .collect();
        let standing = plants.iter().filter(|(p, b)| at(*p - IVec3::Y).is_none_or(|s| b.can_stay_on(s))).count();
        assert!(standing * 1000 >= plants.len() * 998, "{biome:?}: {standing} of {} plants stand", plants.len());
        // Vines hang from (or stand on) more vine, wart or rock.
        let vines: Vec<_> = blocks.iter().filter_map(|(p, b)| Vine::of(*b).map(|(v, _)| (*p, v))).collect();
        let held = vines
            .iter()
            .filter(|(p, v)| {
                at(*p - v.grows()).is_none_or(|s| s.is_opaque() || Vine::of(s).is_some_and(|(w, _)| w == *v))
            })
            .count();
        assert!(held * 1000 >= vines.len() * 998, "{biome:?}: {held} of {} vines held", vines.len());
    }
}

/// Generation 7's own blocks per biome: Java's surface rules dress the
/// floors and the features follow.
#[test]
fn generation_7_dresses_each_biome_like_java() {
    let g = NetherGen::new(2024);
    assert_eq!(g.generation, Generation::V7);
    let crimson = area(&g, NetherBiome::CrimsonForest);
    for b in [nb::CRIMSON_NYLIUM, nb::CRIMSON_STEM, nb::NETHER_WART_BLOCK, nb::CRIMSON_ROOTS, Block::GLOWSTONE] {
        assert!(count(&crimson, |c| c == b) > 0, "crimson forest has {}", b.name());
    }
    assert!(count(&crimson, |b| matches!(Vine::of(b), Some((Vine::Weeping, _)))) > 0, "weeping vines");
    let warped = area(&g, NetherBiome::WarpedForest);
    for b in [nb::WARPED_NYLIUM, nb::WARPED_STEM, nb::WARPED_WART_BLOCK, nb::WARPED_ROOTS, nb::NETHER_SPROUTS] {
        assert!(count(&warped, |c| c == b) > 0, "warped forest has {}", b.name());
    }
    assert!(count(&warped, |b| matches!(Vine::of(b), Some((Vine::Twisting, _)))) > 0, "twisting vines");
    let soul = area(&g, NetherBiome::SoulSandValley);
    for b in [nb::SOUL_SOIL, Block::SOUL_SAND, nb::BONE_BLOCK, nb::SOUL_FIRE, Block::BASALT] {
        assert!(count(&soul, |c| c == b) > 0, "soul sand valley has {}", b.name());
    }
    assert_eq!(count(&soul, nb::is_nylium), 0, "no nylium in soul sand valleys");
    let deltas = area(&g, NetherBiome::BasaltDeltas);
    for b in [Block::BASALT, Block::BLACKSTONE, Block::MAGMA, Block::QUARTZ_ORE] {
        assert!(count(&deltas, |c| c == b) > 100, "basalt deltas have {}", b.name());
    }
    let pools = deltas.iter().filter(|(p, b)| **b == Block::LAVA && p.y > LAVA_SEA).count();
    assert!(pools > 10, "lava delta pools above the sea: {pools}");
    let wastes = area(&g, NetherBiome::NetherWastes);
    for b in [Block::GLOWSTONE, Block::QUARTZ_ORE, Block::MAGMA, Block::SOUL_SAND] {
        assert!(count(&wastes, |c| c == b) > 0, "nether wastes have {}", b.name());
    }
}

/// The surface rules' edges by the lava sea: floors that become lava below
/// y 32 where the surface depth makes a hole, and gravel where floors meet
/// the sea's level (y 30 to 34).
#[test]
fn floors_by_the_lava_sea_get_javas_gravel_and_lava_edges() {
    let g = NetherGen::with_generation(12345, Generation::V7);
    let (mut lava_floor, mut gravel_low, mut gravel_high, mut soul_high) = (0, 0, 0, 0);
    for z in (-1024..1024).step_by(7) {
        for x in (-1024..1024).step_by(5) {
            let bits = g.base_bits(x, z);
            let column = g.structure_column(x, z);
            for y in 0..=ROOF {
                let b = column.blocks[y as usize];
                let solid = bits >> y & 1 == 1;
                if solid && b == Block::LAVA {
                    // Only the top block of a floor below y 32.
                    assert!(y < 32 && bits >> (y + 1) & 1 == 0, "lava rock at {x} {y} {z}");
                    lava_floor += 1;
                }
                if b == Block::GRAVEL {
                    // A floor's surface at y 30..=34 (its top block in the wastes).
                    if y < 35 {
                        gravel_low += 1;
                    } else {
                        gravel_high += 1;
                    }
                }
                if b == Block::SOUL_SAND && g.biomes.biome(x, z) == NetherBiome::NetherWastes && y >= 35 {
                    soul_high += 1;
                }
            }
        }
    }
    assert!(lava_floor > 20, "lava edges {lava_floor}");
    assert!(gravel_low > 100, "gravel by the sea {gravel_low}");
    // Gravel under floors reaches a few blocks below a surface at 34.
    assert_eq!(gravel_high, 0, "gravel above y 34");
    assert_eq!(soul_high, 0, "the wastes' soul sand is only by the sea");
}

/// A cell changes only blocks of its own 3 x 3 region, and decorating it
/// again gives the same changes.
#[test]
fn cells_change_only_their_region_and_are_pure() {
    let g = NetherGen::with_generation(77, Generation::V7);
    for c in [IVec2::new(0, 0), IVec2::new(-9, 4), IVec2::new(-64, -280)] {
        let a = decorate_cell(&g, c);
        let b = decorate_cell(&NetherGen::with_generation(77, Generation::V7), c);
        assert_eq!(a.rows, b.rows, "cell {c}");
        assert_eq!(a.springs, b.springs, "cell {c}");
        assert!(a.len() > 0, "cell {c} has features");
        for (row, entries) in a.rows.iter().enumerate() {
            for &(k, _) in entries {
                let l = unkey(k);
                assert!(l.x < SPAN && l.z < SPAN && l.y < HEIGHT && l.y / CHUNK_SIZE_I == row as i32, "{l}");
            }
        }
    }
}

/// Springs are scheduled where the generated chunk holds their lava.
#[test]
fn generation_7_springs_are_scheduled_to_flow() {
    let g = NetherGen::with_generation(8, Generation::V7);
    let mut total = 0;
    for cz in -2..2 {
        for cx in -2..2 {
            for cy in 0..4 {
                let cpos = IVec3::new(cx, cy, cz);
                let data = g.generate(cpos);
                for p in spring_ticks(&g, cpos, &data) {
                    let l = p - cpos * CHUNK_SIZE_I;
                    assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize), Block::LAVA);
                    total += 1;
                }
            }
        }
    }
    assert!(total > 20, "{total} springs");
}
