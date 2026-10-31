use std::sync::Arc;

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::*;
use crate::world::biome::Biome;
use crate::world::cave_blocks as cb;
use crate::world::chunk::chunk_of;
use crate::world::overworld_blocks as ob;
use crate::world::terrain::{Dimension, Generation};

fn generator(seed: u64, generation: Generation) -> Generator {
    Generator::with_generation(seed, Dimension::Overworld, generation)
}

/// Generated chunks of a box of chunk positions, with a block lookup.
struct Area(FxHashMap<IVec3, ChunkData>);

impl Area {
    fn new(g: &Generator, chunks: impl IntoIterator<Item = IVec3>) -> Self {
        Area(chunks.into_iter().map(|c| (c, g.generate(c))).collect())
    }

    fn get(&self, p: IVec3) -> Option<Block> {
        let l = p - chunk_of(p) * CHUNK_SIZE_I;
        self.0.get(&chunk_of(p)).map(|c| c.get(l.x as usize, l.y as usize, l.z as usize))
    }

    fn cells(&self) -> impl Iterator<Item = (IVec3, Block)> + '_ {
        self.0.iter().flat_map(|(&c, data)| {
            (0..CHUNK_VOLUME).map(move |i| {
                let l = IVec3::new((i % 32) as i32, (i / 1024) as i32, (i / 32 % 32) as i32);
                (c * CHUNK_SIZE_I + l, data.get(l.x as usize, l.y as usize, l.z as usize))
            })
        })
    }
}

fn cube(center: IVec3, r: i32) -> Vec<IVec3> {
    let c = chunk_of(center);
    let mut out = Vec::new();
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let p = c + IVec3::new(dx, dy, dz);
                if p.y >= -2 {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// A deep-dark spot of a generation 2 world with plenty of open cave.
fn deep_dark_spot(g: &Generator) -> IVec3 {
    let mut origin = IVec2::ZERO;
    for _ in 0..12 {
        let at = g.nearest_biome(origin, Biome::DeepDark, 12_800).expect("generation 2 has a deep dark");
        let data = g.generate(chunk_of(at));
        let open = (0..CHUNK_VOLUME).filter(|&i| data.get(i % 32, i / 1024, i / 32 % 32) == Block::AIR).count();
        if open > 1500 {
            return at;
        }
        origin = IVec2::new(at.x + 700, at.z + 300);
    }
    panic!("no open deep dark found");
}

#[test]
fn deep_dark_is_a_generation_2_cave_biome_far_under_the_surface() {
    let g = generator(7, Generation::V2);
    let at = g.nearest_biome(IVec2::ZERO, Biome::DeepDark, 12_800).expect("/locate biome deep_dark finds it");
    assert_eq!(g.biome_at(at), Biome::DeepDark);
    assert!(at.y <= crate::world::deep_dark::TOP_Y, "{at}");
    let col = g.column(at.x, at.z);
    assert!(col.height - at.y >= crate::world::deep_dark::MIN_DEPTH);
    assert!(col.climate.erosion < crate::world::deep_dark::MAX_EROSION);
    assert_ne!(g.biome_at(IVec3::new(at.x, col.height - 30, at.z)), Biome::DeepDark, "not near the surface");
    assert_eq!(Biome::from_name("minecraft:deep_dark"), Some(Biome::DeepDark));
    assert!(Biome::DeepDark.is_cave());
    let old = generator(7, Generation::V1);
    assert_eq!(old.nearest_biome(IVec2::ZERO, Biome::DeepDark, 12_800), None, "generation 1 has none");
    assert_ne!(old.biome_at(at), Biome::DeepDark);
}

#[test]
fn deep_dark_caves_grow_sculk_and_no_other_plants() {
    let g = generator(7, Generation::V2);
    let at = deep_dark_spot(&g);
    let area = Area::new(&g, cube(at, 1));
    let mut columns = FxHashMap::default();
    let mut counts: FxHashMap<&str, usize> = FxHashMap::default();
    for (p, b) in area.cells() {
        let col = *columns.entry(IVec2::new(p.x, p.z)).or_insert_with(|| g.column(p.x, p.z));
        let deep = crate::world::deep_dark::under(&col.climate, col.height, p.y);
        let name = b.name();
        let sculky = name.starts_with("sculk");
        if deep {
            assert!(
                !matches!(
                    name,
                    "glow lichen" | "moss block" | "moss carpet" | "cave vines" | "spore blossom" | "azalea"
                ) && ob::dripstone_of(b).is_none(),
                "{name} at {p} in the deep dark"
            );
            if sculky {
                *counts.entry(name).or_default() += 1;
            }
        } else {
            assert!(!sculky, "{name} at {p} outside the deep dark");
        }
    }
    eprintln!("deep dark at {at}: {counts:?}");
    assert!(counts.get("sculk").copied().unwrap_or(0) > 300, "{counts:?}");
    assert!(counts.get("sculk vein").copied().unwrap_or(0) > 30, "{counts:?}");
    assert!(counts.get("sculk sensor").copied().unwrap_or(0) > 0, "{counts:?}");
}

#[test]
fn attachments_rest_on_real_support() {
    let g = generator(7, Generation::V2);
    let at = deep_dark_spot(&g);
    // The deep dark, and ordinary caves high above it.
    let mut chunks = cube(at, 1);
    chunks.extend(cube(at + IVec3::new(0, 64, 0), 1));
    chunks.sort_by_key(|c| (c.x, c.y, c.z));
    chunks.dedup();
    let area = Area::new(&g, chunks);
    let (mut lichen, mut veins, mut growths) = (0, 0, 0);
    for (p, b) in area.cells() {
        if let Some((kind, faces, wet)) = cb::multiface_of(b) {
            for (i, &d) in cb::DIRS.iter().enumerate() {
                if faces & (1 << i) != 0
                    && let Some(n) = area.get(p + d)
                {
                    assert!(cb::supports(n, i), "{kind:?} at {p} face {i} on {}", n.name());
                }
            }
            match kind {
                cb::Multiface::GlowLichen => {
                    lichen += 1;
                    assert_eq!(faces & cb::DOWN, 0, "glow lichen grows on walls and ceilings only");
                    let col = g.column(p.x, p.z);
                    assert!(col.height - p.y >= LICHEN_DEPTH, "lichen at {p} under {}", col.height);
                }
                cb::Multiface::SculkVein => veins += 1,
            }
            assert_eq!(wet, area.get(p).is_some_and(|b| b.is_waterlogged()));
        }
        if cb::sensor_of(b).is_some() || cb::shrieker_of(b).is_some() {
            growths += 1;
            if let Some(below) = area.get(p - IVec3::Y) {
                assert_eq!(below, cb::SCULK, "sensors and shriekers stand on sculk, {p}");
            }
            if let Some((_, can_summon, _)) = cb::shrieker_of(b) {
                assert!(can_summon, "generated shriekers can summon (inert until v0.16)");
            }
        }
    }
    eprintln!("lichen {lichen}, veins {veins}, sensors and shriekers {growths}");
    assert!(lichen > 0 && veins > 0 && growths > 0);
}

#[test]
fn chunks_match_whatever_order_generates_them() {
    let g = generator(7, Generation::V2);
    let at = deep_dark_spot(&g);
    let chunks = cube(at, 1);
    let forward = Area::new(&g, chunks.clone());
    let other = generator(7, Generation::V2);
    for c in chunks.iter().rev() {
        let (a, b) = (other.generate(*c), &forward.0[c]);
        for i in 0..CHUNK_VOLUME {
            let (x, y, z) = (i % 32, i / 1024, i / 32 % 32);
            assert_eq!(a.get(x, y, z), b.get(x, y, z), "chunk {c} differs by generation order at {x},{y},{z}");
        }
    }
}

#[test]
fn sculk_continues_across_chunk_borders() {
    let g = generator(7, Generation::V2);
    let at = deep_dark_spot(&g);
    let area = Area::new(&g, cube(at, 2));
    let mut columns = FxHashMap::default();
    let (mut border_sculk, mut border_exposed, mut inner_sculk, mut inner_exposed) = (0, 0, 0, 0);
    let mut predicted = 0;
    // Exposed stone-or-sculk cells of every chunk whose neighbours are all
    // generated: on its six faces (where exposure can lie across a border)
    // and inside.
    let surrounded = |c: IVec3| cb::DIRS.iter().all(|&d| area.0.contains_key(&(c + d)));
    for (p, b) in area.cells().filter(|(p, _)| surrounded(chunk_of(*p))) {
        let inner = chunk_of(p);
        if !(b == cb::SCULK || matches!(b, Block::STONE | Block::DEEPSLATE | Block::TUFF)) {
            continue;
        }
        let col = *columns.entry(IVec2::new(p.x, p.z)).or_insert_with(|| g.column(p.x, p.z));
        if !crate::world::deep_dark::under(&col.climate, col.height, p.y) {
            continue;
        }
        let l = p - inner * CHUNK_SIZE_I;
        let outward: Vec<IVec3> = cb::DIRS
            .iter()
            .copied()
            .filter(|&d| chunk_of(p + d) != inner && area.get(p + d).is_some_and(|n| n == Block::AIR))
            .collect();
        let inward = cb::DIRS.iter().any(|&d| chunk_of(p + d) == inner && area.get(p + d) == Some(Block::AIR));
        let on_border = l.cmpeq(IVec3::ZERO).any() || l.cmpeq(IVec3::splat(31)).any();
        if on_border && !outward.is_empty() && !inward {
            // The generating chunk can't see its neighbour's blocks; it
            // reads the pristine terrain there.
            let (mut cols, mut nodes) = (FxHashMap::default(), FxHashMap::default());
            predicted += usize::from(outward.iter().any(|&d| open(g.natural_block(p + d, &mut cols, &mut nodes))));
            border_exposed += 1;
            border_sculk += usize::from(b == cb::SCULK);
        } else if !on_border && inward {
            inner_exposed += 1;
            inner_sculk += usize::from(b == cb::SCULK);
        }
    }
    let share = |n: usize, d: usize| n as f64 / d.max(1) as f64;
    eprintln!("border {border_sculk}/{border_exposed} (predicted {predicted}), inner {inner_sculk}/{inner_exposed}");
    assert!(border_exposed > 20, "chunk faces open onto their neighbours: {border_exposed}");
    // Stone exposed only across a border is seen as exposed, so it can
    // turn to sculk like stone exposed inside the chunk: no bare seams.
    assert!(predicted * 20 >= border_exposed * 19, "{predicted} of {border_exposed} border cells seen as exposed");
    // The rates differ only by where the patch noise falls (the border
    // cells are a few planes): both are a large share.
    assert!(share(border_sculk, border_exposed) > 0.3 && share(inner_sculk, inner_exposed) > 0.3);
}

#[test]
fn springs_sit_in_walls_and_start_flowing() {
    let g = generator(3, Generation::V2);
    let (mut water, mut lava) = (0, 0);
    let mut flowing = None;
    let mut lava_chunk = None;
    'search: for cx in -2..2 {
        for cz in -2..2 {
            for cy in -2..5 {
                let cpos = IVec3::new(cx, cy, cz);
                let data = g.generate(cpos);
                let ticks = spring_ticks(&g, cpos, &data);
                for p in &ticks {
                    let l = *p - cpos * CHUNK_SIZE_I;
                    let b = data.get(l.x as usize, l.y as usize, l.z as usize);
                    let at = |d: IVec3| {
                        let q = l + d;
                        data.get(q.x as usize, q.y as usize, q.z as usize)
                    };
                    assert!(spring_rock(at(IVec3::Y)) && spring_rock(at(IVec3::NEG_Y)), "spring at {p}");
                    if b == Block::WATER {
                        water += 1;
                    } else {
                        assert_eq!(b, Block::LAVA);
                        lava += 1;
                        if lava_chunk.is_none() {
                            lava_chunk = Some((cpos, data.clone(), *p));
                        }
                    }
                }
                if flowing.is_none() && !ticks.is_empty() {
                    flowing = Some((cpos, data));
                }
                if water > 3 && lava > 3 {
                    break 'search;
                }
            }
        }
    }
    eprintln!("water springs {water}, lava springs {lava}");
    assert!(water > 0 && lava > 0);
    let (cpos, data) = flowing.unwrap();
    let mut world = crate::world::World::new_headless(Arc::new(g), Default::default(), 2);
    assert_eq!(world.active_fluids(), 0);
    world.insert_chunk(cpos, Arc::new(data), false);
    assert!(world.active_fluids() > 0, "a generated spring is scheduled to flow");
    // Lava springs get lava ticks: the chunk is installed before its
    // fluids wake, so the wake sees the lava.
    let (cpos, data, spring) = lava_chunk.expect("a lava spring");
    let mut world = crate::world::World::new_headless(Arc::new(generator(3, Generation::V2)), Default::default(), 2);
    world.insert_chunk(cpos, Arc::new(data), false);
    assert!(world.fluids.lava_pending.contains(&spring), "lava spring at {spring} is scheduled");
}

#[test]
fn spring_origins_follow_javas_height_providers() {
    let mut water = Vec::new();
    let mut lava = Vec::new();
    for cy in -2..10 {
        for (p, fluid) in spring_origins(9, IVec3::new(0, cy * CHUNK_SIZE_I, 0)) {
            if fluid == Fluid::Water { water.push(p.y) } else { lava.push(p.y) }
        }
    }
    // Four Java cells per chunk column: 100 water and 80 lava tries.
    assert_eq!((water.len(), lava.len()), (100, 80));
    assert!(water.iter().all(|&y| (WORLD_MIN_Y..=WATER_TOP).contains(&y)));
    let below_zero = lava.iter().filter(|&&y| y < 0).count();
    assert!(below_zero * 2 > lava.len(), "lava springs are biased to the bottom: {lava:?}");
}

#[test]
fn generation_1_has_none_of_it() {
    let g = generator(7, Generation::V1);
    let at = deep_dark_spot(&generator(7, Generation::V2));
    let area = Area::new(&g, cube(at, 1));
    for (p, b) in area.cells() {
        assert!(!cb::owns(b.0), "{} at {p} in generation 1", b.name());
        assert!(!matches!(b, Block::RAW_COPPER_BLOCK | Block::RAW_IRON_BLOCK), "ore vein at {p} in generation 1");
    }
}

/// Chunk generation time with and without generation 2's underground
/// passes, over a deep-dark area and an ordinary one (run with
/// `--ignored --nocapture`; timing, not a pass/fail check).
#[test]
#[ignore]
fn generation_2_underground_cost() {
    let spot = deep_dark_spot(&generator(7, Generation::V2));
    for (label, center) in [("deep dark", spot), ("ordinary", IVec3::new(0, 0, 0))] {
        let mut chunks = Vec::new();
        for cy in -2..4 {
            for cz in -3..3 {
                for cx in -3..3 {
                    chunks.push(chunk_of(center) * IVec3::new(1, 0, 1) + IVec3::new(cx, cy, cz));
                }
            }
        }
        for generation in [Generation::V1, Generation::V2, Generation::V1, Generation::V2] {
            let g = generator(7, generation);
            let start = std::time::Instant::now();
            for &c in &chunks {
                std::hint::black_box(g.generate(c));
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / chunks.len() as f64;
            eprintln!("{label} {generation:?}: {ms:.3} ms per chunk over {} chunks", chunks.len());
        }
    }
}
