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
//!   only the structure caches (including ancient cities and trial chambers)
//!   are emptied, so a chunk
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
use crate::world::structure::Bounds;
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

/// Every chunk intersecting an inclusive structure box, including partial
/// chunks at its edges. Both fixtures must exercise negative coordinates
/// and horizontal/vertical chunk borders, rather than just a start chunk.
fn underground_chunks(bounds: Bounds) -> impl Iterator<Item = IVec3> {
    let lo = bounds.min.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    let hi = bounds.max.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    assert!(lo.x < 0 && lo.z < 0, "negative underground fixture: {bounds:?}");
    assert!(lo.cmplt(hi).all(), "underground fixture crosses chunk borders on every axis: {bounds:?}");
    (lo.z..=hi.z).flat_map(move |z| (lo.x..=hi.x).flat_map(move |x| (lo.y..=hi.y).map(move |y| IVec3::new(x, y, z))))
}

/// Chunk borders around the origin (x and z from -2 to 1, every row: deep
/// dungeons, caves and the surface), the nearest village whole, every
/// temple-family structure within 1200 blocks and the nearest mineshaft, in
/// row order. Generation 3 adds a buried trail ruin and both fossils plus
/// a well near desert terrain, plus a whole ancient city and trial chamber
/// at negative coordinates. Located on a throwaway generator so the runs
/// start cold.
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
        let ocean = g.temples.nearest(&g, Kind::OceanRuin, IVec2::ZERO, 6400).expect("ocean ruin");
        chunks.extend(around(ocean, 2));
        let outpost = g.temples.nearest(&g, Kind::Outpost, IVec2::ZERO, 20_000).expect("an outpost");
        chunks.extend(around(outpost, 2));
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
    if generation >= Generation::V3 {
        // Bounded, deterministic search in negative placement regions. Pick
        // a city with both loot variants and a chamber crossing a vertical
        // chunk border; the complete boxes cover their outlying pieces too.
        let regions = || (1..=16).flat_map(|z| (1..=16).map(move |x| IVec2::new(-x, -z)));
        let city = regions()
            .filter_map(|r| g.ancient_cities.get(&g, r))
            .find(|c| c.chests.iter().any(|&(_, _, ice)| ice) && c.chests.iter().any(|&(_, _, ice)| !ice))
            .expect("a negative ancient city with ordinary and ice-box chests");
        // Include the carving, foundations and sculk patches outside pieces.
        let city_chunks: Vec<_> = underground_chunks(Bounds {
            min: city.bounds.min - IVec3::splat(8),
            max: city.bounds.max + IVec3::splat(8),
        })
        .collect();
        let chamber = regions()
            .filter_map(|r| g.trial_chambers.get(&g, r))
            .find(|c| c.bounds().min.y.div_euclid(CHUNK_SIZE_I) < c.bounds().max.y.div_euclid(CHUNK_SIZE_I))
            .expect("a negative trial chamber crossing a vertical chunk border");
        let chamber_chunks: Vec<_> = underground_chunks(chamber.bounds()).collect();
        eprintln!(
            "GEN-07: city at {} ({} chunks), trial chamber at {} ({} chunks)",
            city.at,
            city_chunks.len(),
            chamber.at,
            chamber_chunks.len()
        );
        chunks.extend(city_chunks);
        chunks.extend(chamber_chunks);
        let ruins = g.trail_ruins.nearest(&g, IVec2::ZERO, 6400).expect("trail ruins");
        chunks.extend(around(ruins, 3));
        let desert = g.nearest_biome(IVec2::ZERO, super::Biome::Desert, 16000).expect("desert");
        for kind in [
            crate::world::relics::Kind::FossilUpper,
            crate::world::relics::Kind::FossilLower,
            crate::world::relics::Kind::Well,
        ] {
            let at = g.relics.find(&g, kind, IVec2::new(desert.x, desert.z), 160).expect("desert relic");
            chunks.extend(around(at, 1));
        }
    }
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
        // Structure caches, keeping terrain and villages.
        3 => {
            g.temples = crate::world::temples::Temples::new(SEED);
            g.ruined_portals = crate::world::ruined_portal::RuinedPortals::new(SEED);
            g.buried_treasure = crate::world::buried_treasure::BuriedTreasure::new(SEED);
            g.mineshafts = crate::world::mineshaft::Mineshafts::new(SEED);
            g.dungeons = crate::world::dungeon::Dungeons::new(SEED);
            g.relics = crate::world::relics::Relics::new(SEED);
            g.trail_ruins = crate::world::trail_ruins::TrailRuins::new(SEED);
            g.ancient_cities = crate::world::ancient_city::AncientCities::new(SEED);
            g.trial_chambers = crate::world::trial_chambers::TrialChambers::new(SEED);
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
    if generation >= Generation::V3 {
        assert!(has(|f| matches!(f, Feature::Archaeology(..))), "no ocean ruin archaeology");
        assert!(
            has(|f| matches!(f, Feature::TempleChest(_, crate::world::temples::LootTable::Outpost))),
            "generation 3: no outpost chest"
        );
    }
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
        for ice in [false, true] {
            assert!(
                all.iter().any(|f| matches!(f, Feature::AncientCityChest(_, i) if *i == ice)),
                "generation 3: no ancient-city chest (ice box: {ice})"
            );
        }
        assert!(has(|f| matches!(f, Feature::TrialLoot(..))), "generation 3: no trial loot");
        assert!(has(|f| matches!(f, Feature::TrialSpawner(_))), "generation 3: no trial spawner");
        assert!(has(|f| matches!(f, Feature::TrialVault)), "generation 3: no trial vault");

        // Features alone could pass even if painting stopped. Check their
        // backing blocks in the generated reference, not just layout data.
        let mut vaults = [false; 2];
        for (&cpos, (chunk, features)) in chunks.iter().zip(&reference) {
            for &(at, feature) in features {
                let p = at - cpos * CHUNK_SIZE_I;
                let block = chunk.get(p.x as usize, p.y as usize, p.z as usize);
                match feature {
                    Feature::AncientCityChest(..) => assert_eq!(block.base(), Block::CHEST, "city chest at {at}"),
                    Feature::TrialLoot(..) => assert!(crate::world::chest::is_chest(block), "trial loot at {at}"),
                    Feature::TrialSpawner(_) => {
                        assert!(crate::world::trial::spawner(block).is_some(), "trial spawner at {at}")
                    }
                    Feature::TrialVault => {
                        let (_, _, ominous) = crate::world::trial::vault(block).expect("trial vault block");
                        vaults[usize::from(ominous)] = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(vaults.into_iter().all(|covered| covered), "generation 3: normal and ominous trial vaults");
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

/// GEN-07's Nether counterpart includes every chunk intersecting a treasure
/// bastion, including its magma-cube spawner and a negative region border.
#[test]
fn generation_3_bastion_ignores_order_threads_and_caches() {
    let bastions = crate::world::bastion::Bastions::with_generation(SEED, Generation::V3);
    let bastion = (-4..=0)
        .flat_map(|z| (-4..=0).map(move |x| IVec2::new(x, z)))
        .filter_map(|r| bastions.get(r))
        .find(|b| b.kind == crate::world::bastion::Kind::Treasure)
        .expect("treasure bastion");
    let lo = crate::world::chunk::chunk_of(bastion.bounds.min);
    let hi = crate::world::chunk::chunk_of(bastion.bounds.max);
    let chunks: Vec<_> = (lo.x..=hi.x)
        .flat_map(|x| (lo.z..=hi.z).flat_map(move |z| (lo.y..=hi.y).map(move |y| IVec3::new(x, y, z))))
        .collect();
    let fresh = || Generator::with_generation(SEED, Dimension::Nether, Generation::V3);
    let g = fresh();
    let reference: Vec<_> = chunks.iter().map(|&p| (g.generate(p), g.structure_features(p))).collect();
    assert!(
        reference
            .iter()
            .any(|(_, fs)| fs.iter().any(|(_, f)| matches!(f, Feature::Spawner(crate::entity::MobKind::MagmaCube))))
    );
    let g = fresh();
    for i in (0..chunks.len()).rev() {
        assert_eq!(differs(&g, chunks[i], &reference[i], "reverse"), None);
    }
    let g = fresh();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let (g, chunks, reference) = (&g, &chunks, &reference);
                s.spawn(move || {
                    for i in (t..chunks.len()).step_by(THREADS) {
                        assert_eq!(differs(g, chunks[i], &reference[i], "parallel"), None);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    });
    for (&p, want) in chunks.iter().zip(&reference) {
        assert_eq!(differs(&fresh(), p, want, "fresh caches"), None);
    }
}

#[test]
fn generation_3_nether_fossils_ignore_order_threads_and_caches() {
    let probe = crate::world::nether::NetherGen::new(SEED);
    let valley = probe
        .biomes
        .nearest(IVec3::ZERO, crate::world::nether_biome::NetherBiome::SoulSandValley, 20000)
        .expect("soul sand valley");
    let region = IVec2::new(valley.x, valley.z).div_euclid(IVec2::splat(32));
    let mut starts = Vec::new();
    'search: for z in -20..=20 {
        for x in -20..=20 {
            if let Some(r) = probe.fossils.get(&probe, region + IVec2::new(x, z)) {
                starts.push(r.origin);
                if starts.len() == 3 {
                    break 'search;
                }
            }
        }
    }
    assert_eq!(starts.len(), 3);
    let mut chunks: Vec<_> = starts.into_iter().flat_map(|p| around(p, 1)).filter(|p| (0..4).contains(&p.y)).collect();
    chunks.sort_by_key(|p| (p.z, p.x, p.y));
    chunks.dedup();
    let fresh = || Generator::with_generation(SEED, Dimension::Nether, Generation::V3);
    let g = fresh();
    let reference: Vec<_> = chunks.iter().map(|&p| (g.generate(p), g.structure_features(p))).collect();
    assert!(reference.iter().any(|(c, _)| blocks(c).contains(&crate::world::nether_biome_blocks::BONE_BLOCK)));
    let g = fresh();
    for (i, &p) in chunks.iter().enumerate().rev() {
        assert!(differs(&g, p, &reference[i], "nether reversed").is_none());
    }
    let g = fresh();
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (chunks, reference, g) = (&chunks, &reference, &g);
            s.spawn(move || {
                for i in (t..chunks.len()).step_by(THREADS) {
                    assert!(differs(g, chunks[i], &reference[i], "nether parallel").is_none());
                }
            });
        }
    });
    for (i, &p) in chunks.iter().enumerate() {
        assert!(differs(&fresh(), p, &reference[i], "nether cold").is_none());
    }
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

/// Surface-beard footprints and their outside ring, including chunks whose
/// only structure influence comes from across a chunk boundary.
#[test]
fn generation_3_sloped_structure_beards_ignore_order_threads_and_caches() {
    let seed = 42;
    let fresh = || Generator::with_generation(seed, Dimension::Overworld, Generation::V3);
    let probe = fresh();
    let mut chunks = Vec::new();
    for (_, ps) in super::beard::tests::fixtures(&probe) {
        for p in ps {
            let lo = (p.bounds.min - glam::IVec3::splat(12)).div_euclid(IVec3::splat(CHUNK_SIZE_I));
            let hi = (p.bounds.max + glam::IVec3::splat(12)).div_euclid(IVec3::splat(CHUNK_SIZE_I));
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    for y in lo.y..=hi.y {
                        chunks.push(IVec3::new(x, y, z));
                    }
                }
            }
        }
    }
    chunks.sort_by_key(|p| (p.z, p.x, p.y));
    chunks.dedup();
    let g = fresh();
    let reference: Vec<_> = chunks.iter().map(|&p| (g.generate(p), g.structure_features(p))).collect();
    let mut g = fresh();
    for (i, p) in chunks.iter().enumerate().rev() {
        assert_eq!(differs(&g, *p, &reference[i], "reverse"), None);
        if i % 3 == 0 {
            g.adapted_columns.lock().unwrap().clear();
        }
        if i % 5 == 0 {
            g.columns.lock().unwrap().clear();
        }
        if i % 7 == 0 {
            g.villages = crate::world::village::Villages::new(seed);
            g.temples = crate::world::temples::Temples::new(seed);
        }
    }
    let g = fresh();
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (g, chunks, reference) = (&g, &chunks, &reference);
            s.spawn(move || {
                for i in (t..chunks.len()).step_by(THREADS) {
                    assert_eq!(differs(g, chunks[i], &reference[i], "parallel"), None);
                }
            });
        }
    });
    for (p, want) in chunks.iter().zip(reference.iter()) {
        assert_eq!(differs(&fresh(), *p, want, "cold"), None);
    }
}
