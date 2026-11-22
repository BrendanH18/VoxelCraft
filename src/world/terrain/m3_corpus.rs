//! The M3 exit corpus (v0.16, 2026-11-21): every GEN-05 structure family
//! of generation 5, located on two seeds and checked against the Java 26.3
//! facts in `tests/fixtures/java/26.3/m3_corpus.json`
//! (`tools/extract_java_m3_corpus_fixtures.py`).
//!
//! For each family and seed the corpus
//!
//! - locates the nearest instance from the origin the way `/locate` (or,
//!   for features, the test discovery helpers) does, and pins its
//!   coordinates in [`CORPUS`], so a change to where a family generates is
//!   a visible change to this table;
//! - checks the instance against Java's placement for its structure set or
//!   placed feature, ported here from the fixture rather than read from
//!   VoxelCraft's constants: `RandomSpreadStructurePlacement`'s potential
//!   chunk (linear and triangular), the four frequency reducers, the
//!   outposts' exclusion zone around villages, the stronghold rings, and the
//!   feature counts and rarities of dungeons, fossils and wells;
//! - collects the block entities its chunks register
//!   ([`Generator::structure_features`]) and checks that every container's
//!   loot table is one of Java's chest tables and one VoxelCraft can roll,
//!   that the generated chunk holds the container or spawner the feature
//!   names, and that the family's residents and spawners are there.
//!
//! What a family has that Java doesn't (FE-01 terrain, FE-02 templates) is
//! in the [M3 exit record](../../../docs/verification/2026-11-21-v0.16-m3-exit.md).

use std::collections::{BTreeMap, BTreeSet};

use glam::{IVec2, IVec3};
use serde_json::Value;

use super::{Biome, Dimension, Generation, Generator};
use crate::enchant::JavaRandom;
use crate::entity::MobKind;
use crate::world::block::Block;
use crate::world::chunk::{chunk_of, local_of};
use crate::world::fortress::Feature;
use crate::world::java_pieces::{next_double, next_long};
use crate::world::structure::Bounds;
use crate::world::temples::Kind;

/// The corpus seeds: the order corpus's and the beard fixtures'.
const SEEDS: [u64; 2] = [12345, 42];

/// Where each family's located instance is, by seed: the `/locate` answer
/// (the start chunk's centre for temples, a structure's own anchor for the
/// others; see [`locate`]). Printed by the test when it differs.
const CORPUS: &[(&str, u64, [i32; 3])] = &[
    ("abandoned_camp", 42, [-264, 69, 1256]),
    ("abandoned_camp", 12345, [88, 77, 152]),
    ("ancient_city", 42, [144, -27, 176]),
    ("ancient_city", 12345, [960, -27, 192]),
    ("bastion_remnant", 42, [-346, 33, 729]),
    ("bastion_remnant", 12345, [167, 33, 336]),
    ("buried_treasure", 42, [-263, 0, -151]),
    ("buried_treasure", 12345, [-39, 0, 153]),
    ("desert_pyramid", 42, [-984, 74, -2504]),
    ("desert_pyramid", 12345, [-2904, 59, 840]),
    ("desert_well", 42, [3407, 69, -2298]),
    ("desert_well", 12345, [3313, 67, -2494]),
    ("dungeon", 42, [59, -22, -5]),
    ("dungeon", 12345, [44, 40, -32]),
    ("end_city", 42, [-569, 60, -905]),
    ("end_city", 12345, [695, 66, 1079]),
    ("fortress", 42, [9, 48, 38]),
    ("fortress", 12345, [771, 48, -169]),
    ("fossil", 42, [55, -8, -317]),
    ("fossil", 12345, [20, 49, -156]),
    ("igloo", 42, [280, 67, -2472]),
    ("igloo", 12345, [1064, 70, 24]),
    ("jungle_pyramid", 42, [-504, 80, 2072]),
    ("jungle_pyramid", 12345, [-1976, 69, 520]),
    ("mansion", 42, [1976, 63, 4584]),
    ("mansion", 12345, [4600, 69, -424]),
    ("mineshaft", 42, [72, 0, 25]),
    ("mineshaft", 12345, [70, -57, -5]),
    ("mineshaft_mesa", 42, [1174, 61, 483]),
    ("mineshaft_mesa", 12345, [-2836, 52, 996]),
    ("monument", 42, [24, 48, 712]),
    ("monument", 12345, [-392, 48, -360]),
    ("nether_fossil", 42, [-444, 36, 774]),
    ("nether_fossil", 12345, [1093, 52, 388]),
    ("ocean_ruin", 42, [-152, 49, -216]),
    ("ocean_ruin", 12345, [-248, 50, 88]),
    ("pillager_outpost", 42, [1544, 79, -2920]),
    ("pillager_outpost", 12345, [-2696, 85, -792]),
    ("ruined_portal", 42, [256, 64, 352]),
    ("ruined_portal", 12345, [16, 45, 0]),
    ("shipwreck", 42, [-120, 58, 120]),
    ("shipwreck", 12345, [-184, 52, -328]),
    ("stronghold", 42, [1636, 11, 660]),
    ("stronghold", 12345, [-620, 12, -1708]),
    ("swamp_hut", 42, [808, 66, -472]),
    ("swamp_hut", 12345, [648, 67, -1864]),
    ("trail_ruins", 42, [-397, 48, 109]),
    ("trail_ruins", 12345, [99, 74, 275]),
    ("trial_chambers", 42, [263, -33, 103]),
    ("trial_chambers", 12345, [-9, -22, 233]),
    ("village", 42, [-384, 72, -784]),
    ("village", 12345, [-208, 78, 400]),
];

fn fixture() -> &'static Value {
    static DATA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/m3_corpus.json")).expect("m3 corpus")
    })
}

fn placement(set: &str) -> &'static Value {
    let p = &fixture()["structure_sets"][set]["placement"];
    assert!(p.is_object(), "no structure set {set} in the fixture");
    p
}

fn int(v: &Value) -> i32 {
    v.as_i64().unwrap_or_else(|| panic!("not an integer: {v}")) as i32
}

/// `WorldgenRandom.setLargeFeatureWithSalt(seed, x, z, salt)`.
fn salted(seed: u64, x: i32, z: i32, salt: i64) -> JavaRandom {
    JavaRandom::new(
        i64::from(x)
            .wrapping_mul(341_873_128_712)
            .wrapping_add(i64::from(z).wrapping_mul(132_897_987_541))
            .wrapping_add(seed as i64)
            .wrapping_add(salt),
    )
}

/// `WorldgenRandom.setLargeFeatureSeed(seed, x, z)`.
fn large_feature(seed: u64, chunk: IVec2) -> JavaRandom {
    let mut r = JavaRandom::new(seed as i64);
    let (a, b) = (next_long(&mut r), next_long(&mut r));
    JavaRandom::new(i64::from(chunk.x).wrapping_mul(a) ^ i64::from(chunk.y).wrapping_mul(b) ^ seed as i64)
}

/// The placement region holding `chunk`.
fn region_of(set: &str, chunk: IVec2) -> IVec2 {
    chunk.div_euclid(IVec2::splat(int(&placement(set)["spacing"])))
}

/// `RandomSpreadStructurePlacement.getPotentialStructureChunk`.
fn potential(seed: u64, set: &str, region: IVec2) -> IVec2 {
    let p = placement(set);
    assert_eq!(p["type"], "random_spread", "{set}");
    let (spacing, separation) = (int(&p["spacing"]), int(&p["separation"]));
    let mut r = salted(seed, region.x, region.y, p["salt"].as_i64().unwrap());
    let limit = spacing - separation;
    let triangular = p["spread_type"] == "triangular";
    let mut spread = || {
        if triangular { (r.next_bounded(limit) + r.next_bounded(limit)) / 2 } else { r.next_bounded(limit) }
    };
    let (x, z) = (spread(), spread());
    region * spacing + IVec2::new(x, z)
}

/// `FrequencyReductionMethod.shouldGenerate` for the set's frequency.
fn frequency_passes(seed: u64, set: &str, chunk: IVec2) -> bool {
    let p = placement(set);
    let f = p["frequency"].as_f64().map_or(1.0, |f| f as f32);
    if f >= 1.0 {
        return true;
    }
    match p["frequency_reduction_method"].as_str().unwrap_or("default") {
        "legacy_type_1" => {
            let (cx, cz) = (chunk.x >> 4, chunk.y >> 4);
            let mut r = JavaRandom::new(i64::from(cx ^ (cz << 4)) ^ seed as i64);
            r.next_int();
            r.next_bounded((1.0 / f) as i32) == 0
        }
        "legacy_type_2" => salted(seed, chunk.x, chunk.y, 10_387_320).next_float() < f,
        "legacy_type_3" => next_double(&mut large_feature(seed, chunk)) < f64::from(f),
        // `setLargeFeatureWithSalt(seed, salt, x, z)`, Java's argument order.
        "default" => {
            let salt = int(&p["salt"]);
            salted(seed, salt, chunk.x, i64::from(chunk.y)).next_float() < f
        }
        other => panic!("unknown frequency reduction {other}"),
    }
}

/// `isStructureChunk` without the exclusion zone: the potential chunk of
/// its region and the frequency.
fn structure_chunk(seed: u64, set: &str, chunk: IVec2) -> bool {
    potential(seed, set, region_of(set, chunk)) == chunk && frequency_passes(seed, set, chunk)
}

/// One located instance of a family.
struct Found {
    /// `/locate`'s answer (or the discovery helper's).
    at: IVec3,
    /// The structure's start chunk, checked against its set's placement.
    start: Option<IVec2>,
    /// The blocks the structure occupies, for its block entities.
    bounds: Bounds,
    /// Mobs the structure spawns once (temples' residents and the like).
    residents: Vec<MobKind>,
}

impl Found {
    fn new(at: IVec3, start: Option<IVec2>, bounds: Bounds) -> Self {
        Self { at, start, bounds, residents: Vec::new() }
    }
}

/// The Java (16-block) chunk holding `p`; VoxelCraft's chunks are 32³.
fn chunk16(p: IVec3) -> IVec2 {
    IVec2::new(p.x.div_euclid(16), p.z.div_euclid(16))
}

fn around_bounds(at: IVec3, r: i32, y: (i32, i32)) -> Bounds {
    Bounds { min: IVec3::new(at.x - r, y.0, at.z - r), max: IVec3::new(at.x + r, y.1, at.z + r) }
}

fn overworld(seed: u64) -> Generator {
    Generator::with_generation(seed, Dimension::Overworld, Generation::V5)
}

/// The family's structure set (`None` for features).
fn set_of(family: &str) -> Option<&'static str> {
    Some(match family {
        "village" => "villages",
        "mineshaft" | "mineshaft_mesa" => "mineshafts",
        "desert_pyramid" => "desert_pyramids",
        "jungle_pyramid" => "jungle_temples",
        "swamp_hut" => "swamp_huts",
        "igloo" => "igloos",
        "pillager_outpost" => "pillager_outposts",
        "shipwreck" => "shipwrecks",
        "ocean_ruin" => "ocean_ruins",
        "monument" => "ocean_monuments",
        "buried_treasure" => "buried_treasures",
        "stronghold" => "strongholds",
        "fortress" | "bastion_remnant" => "nether_complexes",
        "ruined_portal" => "ruined_portals",
        "trail_ruins" => "trail_ruins",
        "end_city" => "end_cities",
        "mansion" => "woodland_mansions",
        "ancient_city" => "ancient_cities",
        "trial_chambers" => "trial_chambers",
        "abandoned_camp" => "abandoned_camp",
        "nether_fossil" => "nether_fossils",
        _ => return None,
    })
}

const TEMPLES: [(&str, Kind, i32); 10] = [
    ("desert_pyramid", Kind::DesertPyramid, 6400),
    ("jungle_pyramid", Kind::JungleTemple, 6400),
    ("swamp_hut", Kind::SwampHut, 6400),
    ("igloo", Kind::Igloo, 6400),
    ("pillager_outpost", Kind::Outpost, 12800),
    ("shipwreck", Kind::Shipwreck, 6400),
    ("ocean_ruin", Kind::OceanRuin, 6400),
    ("monument", Kind::Monument, 12800),
    ("mansion", Kind::Mansion, 25600),
    ("abandoned_camp", Kind::AbandonedCamp, 6400),
];

/// The families, in the record's order.
const FAMILIES: [&str; 26] = [
    "village",
    "dungeon",
    "mineshaft",
    "mineshaft_mesa",
    "desert_pyramid",
    "jungle_pyramid",
    "swamp_hut",
    "igloo",
    "pillager_outpost",
    "shipwreck",
    "ocean_ruin",
    "monument",
    "buried_treasure",
    "stronghold",
    "fortress",
    "bastion_remnant",
    "ruined_portal",
    "trail_ruins",
    "end_city",
    "mansion",
    "ancient_city",
    "trial_chambers",
    "abandoned_camp",
    "fossil",
    "desert_well",
    "nether_fossil",
];

/// Regions in rings out from region (0, 0).
fn regions(rings: i32) -> impl Iterator<Item = IVec2> {
    (0..=rings).flat_map(|r| {
        (-r..=r).flat_map(move |z| (-r..=r).map(move |x| IVec2::new(x, z))).filter(move |p| p.abs().max_element() == r)
    })
}

/// Locates the first mesa mineshaft start around the nearest badlands.
pub(super) fn mesa_mineshaft(g: &Generator) -> (IVec3, IVec2, Bounds) {
    let badlands = g.nearest_biome(IVec2::ZERO, Biome::Badlands, 12_800).expect("badlands");
    let home = chunk16(badlands);
    let (start, shaft) = regions(64)
        .find_map(|d| g.mineshafts.get(g, home + d).filter(|s| s.is_mesa()).map(|s| (home + d, s)))
        .unwrap_or_else(|| panic!("no mesa mineshaft near the badlands at {badlands} on {}", g.seed));
    let b = shaft.bounds;
    let at = IVec3::new((b.min.x + b.max.x) / 2, b.min.y, (b.min.z + b.max.z) / 2);
    (at, start, b)
}

/// Locates `family` on `g` (a generation 5 generator of its dimension).
fn locate(family: &str, g: &Generator) -> Found {
    let seed = g.seed;
    if let Some(&(_, kind, radius)) = TEMPLES.iter().find(|t| t.0 == family) {
        let at = g.temples.nearest(g, kind, IVec2::ZERO, radius).unwrap_or_else(|| panic!("{family} on {seed}"));
        let start = (IVec2::new(at.x, at.z) - IVec2::splat(8)).div_euclid(IVec2::splat(16));
        let built = g
            .temples
            .near(g, at, 16)
            .into_iter()
            .find(|b| b.kind == kind)
            .unwrap_or_else(|| panic!("{family} on {seed}: nothing built at {at}"));
        let mut found = Found::new(at, Some(start), built.bounds);
        found.residents = built.residents.iter().map(|r| r.0).collect();
        return found;
    }
    let rows = |d: Dimension| (d.min_y(), d.max_y() - 1);
    match family {
        "village" => {
            let at = g.villages.nearest(g, IVec3::ZERO).expect("a village");
            let region = region_of("villages", chunk16(at));
            let v = g.villages.get(g, region).expect("the located village");
            let start = g.villages.candidate(region).div_euclid(IVec2::splat(16));
            Found::new(at, Some(start), v.bounds)
        }
        "dungeon" => {
            // The first dungeon chest from the origin outwards, by chunk.
            for c in regions(8) {
                for y in Dimension::Overworld.chunk_rows() {
                    let cpos = IVec3::new(c.x, y, c.y);
                    if let Some(&(at, _)) =
                        g.dungeons.features(g, cpos).iter().find(|(_, f)| matches!(f, Feature::Spawner(_)))
                    {
                        return Found::new(at, None, around_bounds(at, 8, (at.y - 4, at.y + 8)));
                    }
                }
            }
            panic!("no dungeon near the origin on {seed}")
        }
        "mineshaft" => {
            let at = g.mineshafts.nearest(g, IVec3::ZERO).unwrap_or_else(|| panic!("{family} on {seed}"));
            // The located shaft's start: the chunk whose start VoxelCraft
            // accepts nearest the located centre.
            let here = chunk16(at);
            let start = (-6..=6)
                .flat_map(|z| (-6..=6).map(move |x| here + IVec2::new(x, z)))
                .filter(|&c| g.mineshafts.starts_at(g, c))
                .min_by_key(|&c| (c - here).length_squared())
                .expect("the located shaft's start");
            assert!(!g.mineshafts.get(g, start).unwrap().is_mesa());
            Found::new(at, Some(start), around_bounds(at, 96, rows(Dimension::Overworld)))
        }
        "mineshaft_mesa" => {
            let (at, start, bounds) = mesa_mineshaft(g);
            Found::new(at, Some(start), bounds)
        }
        "buried_treasure" => {
            let at = g.buried_treasure.nearest_buried_treasure(g, IVec3::ZERO, 400).expect("buried treasure");
            let chunk = chunk16(at);
            Found::new(at, Some(chunk), around_bounds(at, 4, rows(Dimension::Overworld)))
        }
        "stronghold" => {
            let at = g.strongholds.nearest(IVec3::ZERO).expect("a stronghold");
            let sh = g
                .strongholds
                .near(IVec2::new(at.x, at.z), IVec2::new(at.x, at.z))
                .into_iter()
                .next()
                .expect("the located stronghold");
            Found::new(at, Some(chunk16(at)), sh.bounds)
        }
        "fortress" | "bastion_remnant" => {
            let n = g.nether.as_ref().expect("a Nether generator");
            for region in regions(4) {
                let start = crate::world::nether_complexes::placement(seed, region).0.div_euclid(IVec2::splat(16));
                if family == "fortress" {
                    if let Some(f) = n.fortresses.get(region) {
                        let at = IVec3::new(
                            (f.bounds.min.x + f.bounds.max.x) / 2,
                            f.bounds.min.y,
                            (f.bounds.min.z + f.bounds.max.z) / 2,
                        );
                        return Found::new(at, Some(start), f.bounds);
                    }
                } else if let Some(b) = n.fortresses.bastions.get(region) {
                    let at = IVec3::new(
                        (b.bounds.min.x + b.bounds.max.x) / 2,
                        b.bounds.min.y,
                        (b.bounds.min.z + b.bounds.max.z) / 2,
                    );
                    let mut found = Found::new(at, Some(start), b.bounds);
                    found.residents = b.template_residents.iter().flatten().map(|r| r.kind).collect();
                    return found;
                }
            }
            panic!("no {family} near the origin on {seed}")
        }
        "ruined_portal" => {
            let at = g.ruined_portals.nearest(g, IVec3::ZERO, 10).expect("a ruined portal");
            let region = region_of("ruined_portals", chunk16(at));
            let portal = (-1..=1)
                .flat_map(|z| (-1..=1).map(move |x| region + IVec2::new(x, z)))
                .filter_map(|r| g.ruined_portals.get(g, r))
                .find(|p| p.bounds.contains(at) || p.origin == at)
                .expect("the located portal");
            Found::new(at, Some(chunk16(portal.origin)), portal.bounds)
        }
        "trail_ruins" => {
            let at = g.trail_ruins.nearest(g, IVec2::ZERO, 6400).expect("trail ruins");
            let ruin = g.trail_ruins.get(g, region_of("trail_ruins", chunk16(at))).expect("the ruins");
            let bounds = ruin.pieces.iter().fold(ruin.pieces[0].bounds, |b, p| b.union(&p.bounds));
            Found::new(at, Some(chunk16(ruin.at)), bounds)
        }
        "ancient_city" => {
            let at = g.ancient_cities.nearest(g, IVec2::ZERO, 6400).expect("an ancient city");
            let city = g.ancient_cities.get(g, region_of("ancient_cities", chunk16(at))).expect("the city");
            Found::new(at, Some(chunk16(city.at)), city.bounds)
        }
        "trial_chambers" => {
            let at = g.trial_chambers.nearest(g, IVec2::ZERO, 6400).expect("trial chambers");
            let chamber = g.trial_chambers.get(g, region_of("trial_chambers", chunk16(at))).expect("the chamber");
            Found::new(at, Some(chunk16(chamber.at)), chamber.bounds())
        }
        "end_city" => {
            let end = g.end().expect("an End generator");
            let at = end.nearest_city(IVec2::ZERO, 12_800).expect("an End city");
            let city = end
                .cities_near(IVec2::new(at.x, at.z), IVec2::new(at.x, at.z))
                .into_iter()
                .find(|c| c.start.origin == at)
                .expect("the located city");
            let mut found = Found::new(at, Some(city.start.chunk), city.bounds);
            found.residents = city.sentries.iter().map(|_| MobKind::Shulker).collect();
            found
        }
        "fossil" | "desert_well" => {
            use crate::world::relics::Kind as R;
            let (kind, from) = if family == "fossil" {
                (R::FossilUpper, IVec2::ZERO)
            } else {
                let desert = g.nearest_biome(IVec2::ZERO, Biome::Desert, 16_000).expect("a desert");
                (R::Well, IVec2::new(desert.x, desert.z))
            };
            let at = g.relics.find(g, kind, from, 160).unwrap_or_else(|| panic!("{family} on {seed}"));
            Found::new(at, None, around_bounds(at, 12, (at.y - 12, at.y + 12)))
        }
        "nether_fossil" => {
            let at = g.nearest_nether_fossil(IVec2::ZERO, 6400).expect("a Nether fossil");
            Found::new(at, Some(chunk16(at)), around_bounds(at, 12, (at.y - 4, at.y + 16)))
        }
        other => panic!("no family {other}"),
    }
}

/// The generator a family lives in.
fn generator(family: &str, seed: u64) -> Generator {
    let dimension = match family {
        "fortress" | "bastion_remnant" | "nether_fossil" => Dimension::Nether,
        "end_city" => Dimension::End,
        _ => Dimension::Overworld,
    };
    Generator::with_generation(seed, dimension, Generation::V5)
}

/// How far (in chunks) a family's start may be from its set's potential
/// chunk: none where the corpus knows the start chunk itself, the start
/// piece's offset where it reads a piece position.
fn start_tolerance(family: &str) -> i32 {
    match family {
        "ruined_portal" | "trail_ruins" | "ancient_city" | "trial_chambers" | "nether_fossil" => 1,
        _ => 0,
    }
}

/// Every block entity the chunks under `bounds` register.
fn features_in(g: &Generator, bounds: Bounds) -> Vec<(IVec3, Feature)> {
    let rows = g.dimension.chunk_rows();
    let lo = chunk_of(bounds.min);
    let hi = chunk_of(bounds.max);
    let mut out = Vec::new();
    for z in lo.z..=hi.z {
        for x in lo.x..=hi.x {
            for y in lo.y.max(rows.start)..=hi.y.min(rows.end - 1) {
                out.extend(g.structure_features(IVec3::new(x, y, z)));
            }
        }
    }
    out.sort_by_key(|(p, _)| (p.x, p.y, p.z));
    out.dedup_by_key(|(p, _)| *p);
    out
}

/// The Java chest table a container feature fills from.
fn table_of(g: &Generator, at: IVec3, f: &Feature) -> Option<&'static str> {
    crate::world::spawner::chest_loot(g, Block::CHEST, at, f).map(|(name, _)| name)
}

/// What each family must have among its residents, spawners and containers.
fn requirements(family: &str) -> (&'static [MobKind], &'static [MobKind], bool) {
    use MobKind as M;
    // (residents, spawner mobs, has a loot container)
    match family {
        "village" => (&[], &[], true),
        "dungeon" => (&[], &[], true),
        "mineshaft" | "mineshaft_mesa" => (&[], &[], true),
        "desert_pyramid" | "jungle_pyramid" | "shipwreck" | "buried_treasure" | "ruined_portal" => (&[], &[], true),
        "swamp_hut" => (&[M::Witch], &[], false),
        "igloo" => (&[], &[], false),
        "pillager_outpost" => (&[M::Pillager], &[], true),
        "ocean_ruin" => (&[], &[], true),
        "monument" => (&[M::ElderGuardian], &[], false),
        "stronghold" => (&[], &[M::Silverfish], true),
        "fortress" => (&[], &[M::Blaze], false),
        "bastion_remnant" => (&[M::Piglin], &[], true),
        "trail_ruins" | "fossil" | "desert_well" | "nether_fossil" => (&[], &[], false),
        "end_city" => (&[M::Shulker], &[], true),
        "mansion" => (&[M::Vindicator, M::Evoker], &[], true),
        "ancient_city" => (&[], &[], true),
        "trial_chambers" => (&[], &[], true),
        "abandoned_camp" => (&[], &[], true),
        other => panic!("no family {other}"),
    }
}

/// Checks one family on one seed; returns its located position and a
/// summary line for the record.
fn check(family: &str, seed: u64, failures: &mut Vec<String>) -> (IVec3, String) {
    let g = generator(family, seed);
    let found = locate(family, &g);
    let mut fail = |m: String| failures.push(format!("{family} on {seed}: {m}"));

    // Placement against Java's structure set.
    if let (Some(set), Some(start)) = (set_of(family), found.start) {
        let p = placement(set);
        if p["type"] == "random_spread" {
            let region = region_of(set, start);
            let java = (-1..=1)
                .flat_map(|z| (-1..=1).map(move |x| region + IVec2::new(x, z)))
                .map(|r| potential(seed, set, r))
                .min_by_key(|c| (*c - start).abs().max_element())
                .unwrap();
            let off = (java - start).abs().max_element();
            if off > start_tolerance(family) {
                fail(format!("start chunk {start} is {off} chunks from Java's potential chunk {java}"));
            }
            if !frequency_passes(seed, set, java) {
                fail(format!("Java's frequency reducer refuses chunk {java}"));
            }
            if let Some(zone) = p.get("exclusion_zone") {
                let other = zone["other_set"].as_str().unwrap();
                let n = int(&zone["chunk_count"]);
                for z in -n..=n {
                    for x in -n..=n {
                        if structure_chunk(seed, other, java + IVec2::new(x, z)) {
                            fail(format!("{other} placement within {n} chunks of {java}"));
                        }
                    }
                }
            }
        }
    }

    // Block entities: containers, spawners and their blocks.
    let features = features_in(&g, found.bounds);
    let java_tables: BTreeSet<&str> = ["chest_loot_tables", "other_container_loot_tables"]
        .iter()
        .flat_map(|k| fixture()[*k].as_array().unwrap())
        .map(|v| v.as_str().unwrap())
        .collect();
    let ours: Value = serde_json::from_str(include_str!("../loot_tables.json")).unwrap();
    let mut tables = BTreeSet::new();
    let mut spawners = BTreeSet::new();
    let mut residents: BTreeSet<_> = found.residents.iter().map(|k| k.name()).collect();
    let (mut found_container, mut misses) = (false, Vec::new());
    for (at, f) in &features {
        let block = || {
            let c = chunk_of(*at);
            let l = local_of(*at);
            g.generate(c).get(l.x as usize, l.y as usize, l.z as usize)
        };
        if let Some(table) = table_of(&g, *at, f) {
            if !java_tables.contains(table) {
                fail(format!("{table} at {at} is not a Java container table"));
            }
            if !ours["tables"][table].is_object() {
                fail(format!("{table} at {at} has no VoxelCraft loot data"));
            }
            // A container feature whose piece couldn't place it (a mesa
            // mineshaft corridor over open air, as Java's `createChest`
            // needs a floor) leaves no block entity: check that one of the
            // first few holds a container.
            if !found_container
                && misses.len() < 8
                && !matches!(f, Feature::TempleChest(_, crate::world::temples::LootTable::Dispenser))
            {
                let b = block();
                if crate::world::chest::is_chest(b) || b.base() == Block::BARREL {
                    found_container = true;
                } else {
                    misses.push(format!("{table} at {at} is {}", b.name()));
                }
            }
            tables.insert(table);
        }
        match f {
            Feature::Spawner(kind) => {
                let b = block();
                if b != Block::SPAWNER {
                    fail(format!("{} spawner at {at} is {}", kind.name(), b.name()));
                }
                spawners.insert(kind.name());
            }
            Feature::TrialSpawner(_) => {
                spawners.insert("trial spawner");
            }
            Feature::VillageResident(_) => {
                residents.insert("villager");
            }
            Feature::MansionChest(_) => {
                tables.insert("woodland_mansion (fixed)");
            }
            _ => {}
        }
    }
    let (want_residents, want_spawners, container) = requirements(family);
    for k in want_residents {
        if !residents.contains(k.name()) {
            fail(format!("no {} among residents {residents:?}", k.name()));
        }
    }
    for k in want_spawners {
        if !spawners.contains(k.name()) {
            fail(format!("no {} spawner among {spawners:?}", k.name()));
        }
    }
    if !found_container && !misses.is_empty() {
        fail(format!("no generated container: {}", misses.join(", ")));
    }
    if container && tables.is_empty() {
        fail("no loot container".into());
    }
    let summary = format!(
        "{family:16} {seed:>6} at {:>6} {:>4} {:>6}  start {:?}  tables {:?}  spawners {:?}  residents {:?}",
        found.at.x, found.at.y, found.at.z, found.start, tables, spawners, residents
    );
    (found.at, summary)
}

#[test]
fn every_gen_05_family_matches_the_java_corpus_on_two_seeds() {
    let mut failures = Vec::new();
    let mut located = BTreeMap::new();
    let mut lines = Vec::new();
    for family in FAMILIES {
        for seed in SEEDS {
            let started = std::time::Instant::now();
            // One family's panic (nothing located) mustn't hide the others.
            let mut found = Vec::new();
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(family, seed, &mut found)));
            failures.extend(found);
            match run {
                Ok((at, line)) => {
                    lines.push(format!("{line}  ({:.1?})", started.elapsed()));
                    located.insert((family, seed), at);
                }
                Err(e) => {
                    let why =
                        e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()));
                    failures.push(format!("{family} on {seed}: panicked: {}", why.unwrap_or_default()));
                }
            }
        }
    }
    for line in &lines {
        eprintln!("M3 corpus: {line}");
    }
    let pinned: BTreeMap<_, _> = CORPUS.iter().map(|&(f, s, p)| ((f, s), IVec3::from_array(p))).collect();
    for (key, at) in &located {
        if pinned.get(key) != Some(at) {
            failures.push(format!("{} on {}: located at {at}, pinned {:?}", key.0, key.1, pinned.get(key)));
        }
    }
    if pinned.len() != located.len() {
        failures.push(format!("CORPUS pins {} instances, the corpus locates {}", pinned.len(), located.len()));
        let table: Vec<_> =
            located.iter().map(|((f, s), p)| format!("    (\"{f}\", {s}, [{}, {}, {}]),", p.x, p.y, p.z)).collect();
        eprintln!("CORPUS:\n{}", table.join("\n"));
    }
    assert!(failures.is_empty(), "{} corpus failures:\n{}", failures.len(), failures.join("\n"));
}

/// The placements the corpus can't check from one instance: the stronghold
/// rings (all 128), and the counts, rarities and heights of the structures
/// Java places as features.
#[test]
fn ring_and_feature_placements_match_java() {
    // Strongholds: `ConcentricRingsStructurePlacement`'s ring of each start.
    let p = placement("strongholds");
    assert_eq!(p["type"], "concentric_rings");
    let (count, distance, spread) = (int(&p["count"]), int(&p["distance"]), int(&p["spread"]));
    for seed in SEEDS {
        let g = overworld(seed);
        let starts = g.strongholds.starts();
        assert_eq!(starts.len(), count as usize, "seed {seed}");
        let (mut ring, mut in_ring) = (0, 0);
        let mut spread_now = spread;
        for (i, s) in starts.iter().enumerate() {
            let chunk = s.div_euclid(IVec2::splat(16)).as_vec2();
            let centre = f64::from(4 * distance + distance * ring * 6);
            let half = f64::from(distance) * 1.25 + 1.5;
            let d = f64::from(chunk.length());
            assert!((d - centre).abs() <= half, "seed {seed}: stronghold {i} at {s} is {d} chunks out, ring {ring}");
            in_ring += 1;
            if in_ring == spread_now {
                ring += 1;
                in_ring = 0;
                spread_now += 2 * spread_now / (ring + 1);
                spread_now = spread_now.min(count - i as i32);
            }
        }
        // Eight rings: the last has room for ten and holds the nine left.
        assert_eq!((ring, in_ring), (7, 9), "seed {seed}: 128 strongholds make eight rings");
    }

    // Dungeons, fossils and desert wells.
    let features = &fixture()["placed_features"];
    let modifier = |name: &str, kind: &str| -> Value {
        features[name]["placement"].as_array().unwrap().iter().find(|m| m["type"] == kind).cloned().unwrap()
    };
    assert_eq!(features["monster_room"]["feature"], "monster_room");
    assert_eq!(int(&modifier("monster_room", "count")["count"]), crate::world::dungeon::ATTEMPTS as i32);
    assert_eq!(int(&modifier("monster_room_deep", "count")["count"]), crate::world::dungeon::DEEP_ATTEMPTS as i32);
    let deep = modifier("monster_room_deep", "height_range");
    let bottom = Dimension::Overworld.min_y();
    assert_eq!(
        (
            bottom + int(&deep["height"]["min_inclusive"]["above_bottom"]),
            int(&deep["height"]["max_inclusive"]["absolute"])
        ),
        crate::world::dungeon::DEEP_BAND
    );
    let full = modifier("monster_room", "height_range");
    assert_eq!(
        (
            int(&full["height"]["min_inclusive"]["absolute"]),
            Dimension::Overworld.max_y() - 1 - int(&full["height"]["max_inclusive"]["below_top"])
        ),
        crate::world::dungeon::FULL_BAND
    );
    use crate::world::relics::Kind as R;
    for (name, kind) in [("fossil_upper", R::FossilUpper), ("fossil_lower", R::FossilLower), ("desert_well", R::Well)] {
        assert_eq!(int(&modifier(name, "rarity_filter")["chance"]) as u32, kind.rarity(), "{name}");
    }
}
