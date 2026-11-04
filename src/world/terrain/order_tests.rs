//! GEN-07: a chunk generates the same whichever order chunks are asked for
//! in, whichever threads ask, and whatever the structure caches still hold.
//! Each generation runs a corpus of chunks four ways and compares every
//! chunk's blocks and structure features (block entities and loot seeds)
//! with a row-order run on a fresh generator:
//!
//! - shuffled, on another fresh generator;
//! - in parallel on several threads sharing one generator;
//! - in row order with caches emptied along the way: every few chunks the
//!   generator is replaced (dropping every cache, including ones structure
//!   code adds later), and in between only the terrain column cache, or
//!   only the temple, mineshaft and dungeon caches, are emptied, so a chunk
//!   can meet a structure rebuilt from scratch next to cached terrain and
//!   the reverse.
//!
//! Emptying stands in for the caches' own eviction (`CACHE_LIMIT`): that
//! drops whichever entry iterates first, so overflowing a cache can't
//! choose which structures go, and overflowing it with far-away structures
//! costs tens of seconds of terrain per round.

use super::{Dimension, Generation, Generator};
use crate::world::block::Block;
use crate::world::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData};
use crate::world::fortress::Feature;
use crate::world::temples::Kind;
use glam::{IVec2, IVec3};
use std::time::Instant;

const SEED: u64 = 12345;
const THREADS: usize = 6;
/// The evicting run empties caches on a cycle of this many chunks.
const EVICT_CYCLE: usize = 5;

type Features = Vec<(IVec3, Feature)>;

fn fresh(generation: Generation) -> Generator {
    Generator::with_generation(SEED, Dimension::Overworld, generation)
}

/// The chunks around block `p`, `r` chunk columns out and one row above
/// and below.
fn around(p: IVec3, r: i32) -> impl Iterator<Item = IVec3> {
    let c = p.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    (-r..=r).flat_map(move |x| (-r..=r).flat_map(move |z| (-1..=1).map(move |y| c + IVec3::new(x, y, z))))
}

/// Chunk borders around the origin (x and z from -2 to 1, every row: deep
/// dungeons, caves and the surface), the nearest village whole, every
/// temple-family structure within 1200 blocks and the nearest mineshaft, in
/// row order. Located on a throwaway generator so the runs start cold.
fn corpus(generation: Generation) -> Vec<IVec3> {
    let g = fresh(generation);
    let rows = Dimension::Overworld.chunk_rows();
    let (lo, hi) = (rows.start, rows.end);
    let mut chunks: Vec<_> =
        (-2..=1).flat_map(|x| (-2..=1).flat_map(move |z| (lo..hi).map(move |y| IVec3::new(x, y, z)))).collect();
    let village = g.villages.nearest(&g, IVec3::ZERO).expect("a village");
    chunks.extend(around(village, 3));
    for kind in Kind::ALL {
        if let Some(at) = g.temples.nearest(&g, kind, IVec2::ZERO, 1200) {
            chunks.extend(around(at, 1));
        }
    }
    if generation >= Generation::V3 {
        chunks.extend(around(g.ruined_portals.nearest(&g, IVec3::ZERO, 2).expect("a ruined portal"), 1));
        let treasure = g.buried_treasure.nearest_buried_treasure(&g, IVec3::ZERO, 400).expect("buried treasure");
        let treasure = g
            .buried_treasure
            .blocks(&g, IVec2::new(treasure.x.div_euclid(16), treasure.z.div_euclid(16)))
            .last()
            .unwrap()
            .0;
        chunks.extend(around(treasure, 0));
    }
    chunks.extend(around(g.mineshafts.nearest(IVec3::ZERO).expect("a mineshaft"), 1));
    chunks.retain(|p| rows.contains(&p.y));
    chunks.sort_by_key(|p| (p.z, p.x, p.y));
    chunks.dedup();
    chunks
}

fn blocks(chunk: &ChunkData) -> Vec<Block> {
    let mut out = Vec::with_capacity(CHUNK_VOLUME);
    chunk.for_each_block(|b| out.push(b));
    out
}

/// Generates `p` on `g` and describes how it differs from `reference`.
fn differs(g: &Generator, p: IVec3, reference: &(ChunkData, Features), run: &str) -> Option<String> {
    let chunk = g.generate(p);
    let features = g.structure_features(p);
    let (want, got) = (blocks(&reference.0), blocks(&chunk));
    if let Some(i) = (0..CHUNK_VOLUME).find(|&i| want[i] != got[i]) {
        return Some(format!("{run}: chunk {p} block {i} is {:?}, not {:?}", got[i], want[i]));
    }
    (features != reference.1).then(|| format!("{run}: chunk {p} features {features:?}, not {:?}", reference.1))
}

/// Empties some of `g`'s caches, by `step` of [`EVICT_CYCLE`].
fn evict(g: &mut Generator, generation: Generation, step: usize) {
    match step {
        // Everything, whatever caches the generator holds.
        0 => *g = fresh(generation),
        // Terrain columns only.
        2 => g.columns.lock().unwrap().clear(),
        // Temples, mineshafts and dungeons, keeping terrain and villages.
        3 => {
            g.temples = crate::world::temples::Temples::new(SEED);
            g.ruined_portals = crate::world::ruined_portal::RuinedPortals::new(SEED);
            g.buried_treasure = crate::world::buried_treasure::BuriedTreasure::new(SEED);
            g.mineshafts = crate::world::mineshaft::Mineshafts::new(SEED);
            g.dungeons = crate::world::dungeon::Dungeons::new(SEED);
        }
        _ => {}
    }
}

fn generation_ignores_order_threads_and_caches(generation: Generation) {
    let started = Instant::now();
    let chunks = corpus(generation);

    // (a) Row order on a fresh generator: the reference.
    let g = fresh(generation);
    let reference: Vec<(ChunkData, Features)> =
        chunks.iter().map(|&p| (g.generate(p), g.structure_features(p))).collect();
    drop(g);
    let all: Vec<_> = reference.iter().flat_map(|(_, f)| f.iter().map(|(_, f)| *f)).collect();
    let has = |f: fn(&Feature) -> bool| all.iter().any(f);
    assert!(has(|f| matches!(f, Feature::VillageHome(_) | Feature::VillageChest(..))), "{generation:?}: no village");
    assert!(has(|f| matches!(f, Feature::TempleChest(..))), "{generation:?}: no temple-family chest");
    assert!(has(|f| matches!(f, Feature::DungeonChest(_))), "{generation:?}: no dungeon");
    assert!(has(|f| matches!(f, Feature::MineshaftChest(_))), "{generation:?}: no mineshaft chest");
    if generation >= Generation::V3 {
        assert!(
            has(|f| matches!(f, Feature::TempleChest(_, crate::world::temples::LootTable::RuinedPortal))),
            "no ruined portal chest"
        );
        assert!(
            has(|f| matches!(f, Feature::TempleChest(_, crate::world::temples::LootTable::BuriedTreasure))),
            "no buried treasure"
        );
    }
    let mut failures = Vec::new();

    // (b) Shuffled, on another fresh generator.
    let mut order: Vec<usize> = (0..chunks.len()).collect();
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for i in (1..order.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        order.swap(i, (state % (i as u64 + 1)) as usize);
    }
    let g = fresh(generation);
    failures.extend(order.iter().filter_map(|&i| differs(&g, chunks[i], &reference[i], "shuffled")));
    drop(g);

    // (c) In parallel: threads take interleaved chunks, so they build the
    // same structures at the same time.
    let g = fresh(generation);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let (g, chunks, reference) = (&g, &chunks, &reference);
                s.spawn(move || {
                    (t..chunks.len())
                        .step_by(THREADS)
                        .filter_map(|i| differs(g, chunks[i], &reference[i], "parallel"))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            failures.extend(h.join().unwrap());
        }
    });
    drop(g);

    // (d) Row order with the caches emptied along the way.
    let mut g = fresh(generation);
    for (n, (&p, reference)) in chunks.iter().zip(&reference).enumerate() {
        evict(&mut g, generation, n % EVICT_CYCLE);
        failures.extend(differs(&g, p, reference, "evicted"));
    }

    assert!(
        failures.is_empty(),
        "{generation:?}: {} of {} chunks differ:\n{}",
        failures.len(),
        chunks.len(),
        failures.join("\n")
    );
    eprintln!("{generation:?}: {} chunks four ways in {:.2?}", chunks.len(), started.elapsed());
}

#[test]
fn generation_1_ignores_order_threads_and_caches() {
    generation_ignores_order_threads_and_caches(Generation::V1);
}

#[test]
fn generation_2_ignores_order_threads_and_caches() {
    generation_ignores_order_threads_and_caches(Generation::V2);
}

#[test]
fn generation_3_ignores_order_threads_and_caches() {
    generation_ignores_order_threads_and_caches(Generation::V3);
}

/// Nether portals cross both horizontal and vertical chunk borders; their base
/// columns and processor decisions must remain pure alongside Nether caches.
#[test]
fn generation_3_nether_portals_ignore_order_threads_and_caches() {
    let fresh = || Generator::with_generation(SEED, Dimension::Nether, Generation::V3);
    let g = fresh();
    let at = g.ruined_portals.nearest(&g, IVec3::ZERO, 1).unwrap();
    let chunks: Vec<_> = around(at, 1).filter(|p| Dimension::Nether.chunk_rows().contains(&p.y)).collect();
    let reference: Vec<_> = chunks.iter().map(|&p| (g.generate(p), g.structure_features(p))).collect();
    let g = fresh();
    for (p, r) in chunks.iter().zip(&reference).rev() {
        assert_eq!(differs(&g, *p, r, "reverse"), None);
    }
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (g, chunks, reference) = (&g, &chunks, &reference);
            s.spawn(move || {
                for i in (t..chunks.len()).step_by(THREADS) {
                    assert_eq!(differs(g, chunks[i], &reference[i], "parallel Nether"), None);
                }
            });
        }
    });
    for (p, r) in chunks.iter().zip(&reference) {
        assert_eq!(differs(&fresh(), *p, r, "cold Nether"), None);
    }
}
