//! GEN-07 (v0.16): golden block IDs and structure features of
//! [`Generation::V5`]. Every later world change goes in a new generation,
//! so this pin fails when one leaks into generation 5 worlds.
//!
//! On two seeds (the M3 corpus's, whose structure positions the corpus
//! table pins), each dimension hashes its chunks' block IDs and, as their
//! `Debug` text, the structure features (block entities, loot seeds,
//! spawners) the chunks register, with the located positions:
//!
//! - Overworld: the chunks around spawn (every row: dungeons, caves and the
//!   surface), the nearest village, pillager outpost, trail ruins, ancient
//!   city, trial chamber, woodland mansion and mineshaft, and generation 5's
//!   own content: the nearest stronghold, ocean monument, shipwreck and
//!   abandoned camp whole (Java's piece rules), a mesa mineshaft, a
//!   dappled forest of poplars and a whole sulfur spring in its sulfur cave.
//! - Nether: around spawn (nether gold ore), the nearest fortress (Java's
//!   pieces) and bastion, and a Nether fossil.
//! - End: the central island out to the gateways, an outer-island stretch
//!   with chorus, the nearest End city whole and the nearest one with a
//!   ship whole.
//!
//! The test also checks that the set holds the content it is meant to
//! pin, so a search that stops finding something fails here rather than
//! pinning less.

use std::collections::HashSet;

use glam::{IVec2, IVec3};

use super::order_tests::{chunks_of, sulfur_spring};
use super::{Biome, CHUNK_SIZE_I, Dimension, Generation, Generator};
use crate::entity::MobKind;
use crate::world::block::Block;
use crate::world::fortress::Feature;
use crate::world::gen5_blocks as g5;
use crate::world::temples::{Kind, LootTable};

const SEEDS: [u64; 2] = [12345, 42];

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
    }
}

/// The chunks around block `p`, `r` chunk columns out, in `rows`.
fn around(p: IVec3, r: i32, rows: std::ops::Range<i32>) -> impl Iterator<Item = IVec3> {
    let c = p.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    (-r..=r).flat_map(move |x| {
        let rows = rows.clone();
        (-r..=r).flat_map(move |z| rows.clone().map(move |y| IVec3::new(c.x + x, y, c.z + z)))
    })
}

/// The chunks around a located structure: `r` columns out and one row
/// above and below its anchor.
fn near(at: IVec3, r: i32) -> impl Iterator<Item = IVec3> {
    let y = at.y.div_euclid(CHUNK_SIZE_I);
    around(at, r, (y - 1)..(y + 2))
}

/// A located place to pin: its name (hashed with its position) and chunks.
type Place = (&'static str, Option<IVec3>, Vec<IVec3>);

fn located(name: &'static str, at: Option<IVec3>, chunks: impl FnOnce(IVec3) -> Vec<IVec3>) -> Place {
    (name, at, at.map(chunks).unwrap_or_default())
}

fn overworld(g: &Generator) -> Vec<Place> {
    let temple = |name, kind, radius, whole: bool| {
        let at = g.temples.nearest(g, kind, IVec2::ZERO, radius);
        located(name, at, |at| {
            if !whole {
                return near(at, 1).collect();
            }
            let built = g.temples.near(g, at, 16).into_iter().find(|b| b.kind == kind).expect("the located build");
            chunks_of(built.bounds, 8).collect()
        })
    };
    let stronghold = g.strongholds.nearest(IVec3::ZERO);
    let badlands = g.nearest_biome(IVec2::ZERO, Biome::Badlands, 12_800).expect("badlands");
    let spring = sulfur_spring(g);
    vec![
        located("village", g.villages.nearest(g, IVec3::ZERO), |at| near(at, 2).collect()),
        temple("outpost", Kind::Outpost, 12_800, false),
        located("trail ruins", g.trail_ruins.nearest(g, IVec2::ZERO, 6400), |at| near(at, 1).collect()),
        located("ancient city", g.ancient_cities.nearest(g, IVec2::ZERO, 6400), |at| near(at, 1).collect()),
        located("trial chamber", g.trial_chambers.nearest(g, IVec2::ZERO, 6400), |at| near(at, 1).collect()),
        temple("mansion", Kind::Mansion, 25_600, false),
        located("mineshaft", g.mineshafts.nearest(g, IVec3::ZERO), |at| near(at, 1).collect()),
        located("stronghold", stronghold, |at| {
            let column = IVec2::new(at.x, at.z);
            let built = g.strongholds.near(column, column).into_iter().next().expect("the located stronghold");
            chunks_of(built.bounds, 4).collect()
        }),
        temple("monument", Kind::Monument, 12_800, true),
        temple("shipwreck", Kind::Shipwreck, 6400, true),
        temple("abandoned camp", Kind::AbandonedCamp, 6400, true),
        located("mesa mineshaft", g.mineshafts.nearest(g, badlands), |at| near(at, 1).collect()),
        located("dappled forest", g.nearest_biome(IVec2::ZERO, Biome::DappledForest, 12_800), |at| {
            let y = g.column(at.x, at.z).height.div_euclid(CHUNK_SIZE_I);
            around(at, 1, (y - 1)..(y + 2)).collect()
        }),
        located("sulfur spring", Some(spring.origin), |_| chunks_of(spring.bounds, 8).collect()),
    ]
}

fn nether(g: &Generator) -> Vec<Place> {
    let bastion = g.nether.as_ref().and_then(|n| n.fortresses.bastions.nearest(IVec2::ZERO));
    vec![
        located("fortress", g.nearest_fortress(IVec2::ZERO), |at| near(at, 2).collect()),
        located("bastion", bastion, |at| near(at, 2).collect()),
        located("nether fossil", g.nearest_nether_fossil(IVec2::ZERO, 6400), |at| near(at, 1).collect()),
    ]
}

fn end(g: &Generator) -> Vec<Place> {
    let end = g.end().expect("an End generator");
    let city = end.nearest_city(IVec2::ZERO, 12_800);
    let whole = |at: IVec3| {
        let column = IVec2::new(at.x, at.z);
        let city = end.cities_near(column, column).into_iter().find(|c| c.start.origin == at).expect("the city");
        chunks_of(city.bounds, 0).collect::<Vec<_>>()
    };
    // Rings of 320 blocks, as the order test finds a city with a ship.
    let ship = (1..=40)
        .flat_map(|r| (-r..=r).flat_map(move |z| (-r..=r).map(move |x| IVec2::new(x, z) * 320)))
        .flat_map(|p| end.cities_near(p, p))
        .find(|c| c.elytra_frame.is_some())
        .map(|c| c.start.origin);
    vec![located("end city", city, whole), located("end city with a ship", ship, whole)]
}

/// What the set must hold, from its blocks and features, by dimension.
fn missing(dimension: Dimension, blocks: &HashSet<Block>, features: &[Feature]) -> Vec<&'static str> {
    let has = |f: &dyn Fn(&Feature) -> bool| features.iter().any(f);
    let block = |p: &dyn Fn(Block) -> bool| blocks.iter().any(|&b| p(b));
    let checks: Vec<(&str, bool)> = match dimension {
        Dimension::Overworld => vec![
            (
                "village",
                has(&|f| matches!(f, Feature::VillageBed | Feature::VillageResident(_) | Feature::VillageChest(..))),
            ),
            ("outpost chest", has(&|f| matches!(f, Feature::TempleChest(_, LootTable::Outpost)))),
            ("dungeon chest", has(&|f| matches!(f, Feature::DungeonChest(_)))),
            ("mineshaft chest", has(&|f| matches!(f, Feature::MineshaftChest(_)))),
            ("ancient city chest", has(&|f| matches!(f, Feature::AncientCityChest(..)))),
            ("trial spawner", has(&|f| matches!(f, Feature::TrialSpawner(_)))),
            ("stronghold chest", has(&|f| matches!(f, Feature::StrongholdChest(_)))),
            ("silverfish spawner", has(&|f| matches!(f, Feature::Spawner(MobKind::Silverfish)))),
            (
                "shipwreck chest",
                has(&|f| {
                    matches!(
                        f,
                        Feature::TempleChest(_, LootTable::ShipSupply | LootTable::ShipTreasure | LootTable::ShipMap)
                    )
                }),
            ),
            (
                "abandoned camp chest",
                has(&|f| matches!(f, Feature::StructureLoot(_, t) if t.starts_with("abandoned_camp"))),
            ),
            ("monument bricks", blocks.contains(&crate::world::overworld_blocks::PRISMARINE_BRICKS)),
            ("poplar logs", block(&crate::world::wood::poplar::is_log)),
            ("cinnabar", blocks.contains(&g5::CINNABAR)),
            ("sulfur", blocks.contains(&g5::SULFUR)),
            ("potent sulfur vents", block(&|b| g5::potent_state(b).is_some())),
        ],
        Dimension::Nether => vec![
            ("blaze spawner", has(&|f| matches!(f, Feature::Spawner(MobKind::Blaze)))),
            ("bastion chest", has(&|f| matches!(f, Feature::BastionChest(..)))),
            ("nether gold ore", blocks.contains(&g5::NETHER_GOLD_ORE)),
        ],
        Dimension::End => vec![
            ("end city chest", has(&|f| matches!(f, Feature::TempleChest(..)))),
            ("purpur", blocks.contains(&crate::world::end_blocks::PURPUR_BLOCK)),
            ("chorus", blocks.contains(&crate::world::end_blocks::CHORUS_PLANT)),
        ],
    };
    checks.into_iter().filter(|(_, ok)| !ok).map(|(name, _)| name).collect()
}

/// Generation 5 (v0.16) must never change: every later world change goes
/// in a new generation. See the module notes for what the set covers.
#[test]
fn generation_5_chunk_hashes_stay_identical() {
    let mut got = Vec::new();
    let mut failures = Vec::new();
    for seed in SEEDS {
        for dimension in [Dimension::Overworld, Dimension::Nether, Dimension::End] {
            let g = Generator::with_generation(seed, dimension, Generation::V5);
            let rows = dimension.chunk_rows();
            let (mut blocks, mut features) = (0xcbf2_9ce4_8422_2325u64, 0xcbf2_9ce4_8422_2325u64);
            let mut positions: Vec<IVec3> = match dimension {
                // Out to the gateways (96 blocks), and an outer stretch.
                Dimension::End => around(IVec3::ZERO, 3, rows.clone())
                    .chain(around(IVec3::new(1200, 0, 300), 2, rows.clone()))
                    .collect(),
                _ => around(IVec3::ZERO, 2, rows.clone()).collect(),
            };
            let places = match dimension {
                Dimension::Overworld => overworld(&g),
                Dimension::Nether => nether(&g),
                Dimension::End => end(&g),
            };
            for (name, at, chunks) in places {
                // A place the seed lacks is pinned as missing.
                fnv(&mut features, format!("{name} {at:?}").as_bytes());
                positions.extend(chunks);
            }
            positions.retain(|p| rows.contains(&p.y));
            let mut seen = HashSet::new();
            positions.retain(|p| seen.insert(*p));

            let (mut present, mut all) = (HashSet::new(), Vec::new());
            for &p in &positions {
                g.generate(p).for_each_block(|block| {
                    fnv(&mut blocks, &block.0.to_le_bytes());
                    present.insert(block);
                });
                let found = g.structure_features(p);
                fnv(&mut features, format!("{p:?} {found:?}").as_bytes());
                all.extend(found.into_iter().map(|(_, f)| f));
            }
            for what in missing(dimension, &present, &all) {
                failures.push(format!("seed {seed} {}: no {what}", dimension.name()));
            }
            println!("seed {seed} {}: {} chunks", dimension.name(), positions.len());
            got.push((seed, dimension.name(), blocks, features));
        }
    }
    for got in &got {
        println!("PIN ({}, \"{}\", {:#x}, {:#x}),", got.0, got.1, got.2, got.3);
    }
    assert!(failures.is_empty(), "the pinned set lacks content:\n{}", failures.join("\n"));
    let expected: [(u64, &str, u64, u64); 6] = [
        (12345, "overworld", 0xec3c_c524_fb66_b005, 0x58bf_619e_0463_375d),
        (12345, "nether", 0x5724_95a5_1925_c9df, 0x032a_89dd_4274_d036),
        (12345, "end", 0x1e25_f271_38e6_1cc6, 0x53b5_ed1f_fed3_fac9),
        (42, "overworld", 0xabe3_f8fb_0191_bb7c, 0x48f5_6a75_5601_72f7),
        (42, "nether", 0x5d16_5681_bc8a_e36f, 0xcd54_3cb0_7702_2693),
        (42, "end", 0x360b_40d6_6887_c9ad, 0x14ce_6254_3838_2d7a),
    ];
    for (got, expected) in got.iter().zip(expected) {
        assert_eq!(*got, expected, "generation 5 changed: {got:#x?}");
    }
}
