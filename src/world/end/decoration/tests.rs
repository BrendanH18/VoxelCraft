//! Generation 7's End decoration against Java 26.3's own
//! (`tests/fixtures/java/26.3/end_decoration.json`, written by
//! `tools/extract_java_end_decoration_fixtures.py`, which runs the pinned
//! server and reads its region files back).

use glam::{IVec2, IVec3};
use serde_json::Value;

use super::super::EndGen;
use crate::world::block::Block;
use crate::world::chunk::CHUNK_SIZE_I;
use crate::world::end_blocks::{CHORUS_PLANT, flower_age};
use crate::world::terrain::Generation;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../tests/fixtures/java/26.3/end_decoration.json")).unwrap()
}

/// FNV-1a 64 over `items` in order, as the extractor hashes.
fn fnv<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for item in items {
        for b in item.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

fn ivec(v: &Value) -> IVec3 {
    let a = v.as_array().unwrap();
    IVec3::new(a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32, a[2].as_i64().unwrap() as i32)
}

/// The blocks of the 16-block chunk column `chunk` as generation 7 builds
/// them, up to y 192 (VoxelCraft's sections are 32 blocks).
fn column(
    end: &EndGen,
    sections: &mut rustc_hash::FxHashMap<IVec3, crate::world::chunk::ChunkData>,
    chunk: IVec2,
) -> Vec<(IVec3, Block)> {
    let mut out = Vec::new();
    let (lo, hi) = (chunk * 16, chunk * 16 + IVec2::splat(15));
    for cy in 0..192 / CHUNK_SIZE_I {
        let cpos = IVec3::new(lo.x.div_euclid(CHUNK_SIZE_I), cy, lo.y.div_euclid(CHUNK_SIZE_I));
        let data = sections.entry(cpos).or_insert_with(|| end.generate(cpos));
        if data.uniform() == Some(Block::AIR) {
            continue;
        }
        let base = cpos * CHUNK_SIZE_I;
        for y in 0..CHUNK_SIZE_I {
            for z in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    let l = IVec3::new(x, y, z) - IVec3::new(base.x, 0, base.z);
                    let b = data.get(l.x as usize, y as usize, l.z as usize);
                    if b != Block::AIR {
                        out.push((IVec3::new(x, base.y + y, z), b));
                    }
                }
            }
        }
    }
    out
}

fn chorus_name(b: Block) -> Option<String> {
    if b == CHORUS_PLANT {
        return Some("chorus_plant".into());
    }
    flower_age(b).map(|age| format!("chorus_flower:{age}"))
}

/// The chunks whose chorus may meet a neighbour's (a plant grown alone in
/// one comes within a block of one grown alone in the other), and the
/// chunks around them, which those plants reach into. Which grew first
/// changes both (a plant that meets another stops, and every later draw of
/// its chunk moves), and Java grows them in whatever order its server
/// decorates chunks.
fn order_dependent(end: &EndGen, chunks: &[IVec2]) -> rustc_hash::FxHashSet<IVec2> {
    let mut out = rustc_hash::FxHashSet::default();
    for &c in chunks {
        let mine = end.chorus_alone(c);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let n = c + IVec2::new(dx, dz);
                if n == c {
                    continue;
                }
                let theirs = end.chorus_alone(n);
                let meet = mine.iter().any(|&(p, _)| theirs.iter().any(|&(q, _)| (p - q).abs().max_element() <= 1));
                if meet {
                    for m in [c, n] {
                        out.extend((-1..=1).flat_map(|z| (-1..=1).map(move |x| m + IVec2::new(x, z))));
                    }
                }
            }
        }
    }
    out
}

/// Every chunk of the fixture's grids (out past the void, across all four
/// outer biomes): the end stone (the terrain and `end_island_decorated`)
/// is Java's block for block everywhere, and so is the chorus
/// (`chorus_plant`, with its bases) in every chunk whose plants can't meet
/// a neighbour's. Where they can, Java's result depends on the order its
/// server decorated the two chunks; most of those match anyway under
/// VoxelCraft's fixed order, and the test reports how many. No End city
/// stands in the grids.
#[test]
fn outer_end_chunks_are_decorated_as_javas() {
    let f = fixture();
    let mut all_failures = Vec::new();
    let (mut free, mut dependent, mut dependent_differ) = (0, 0, 0);
    for (seed, s) in f["seeds"].as_object().unwrap() {
        let end = EndGen::with_generation(seed.parse::<i64>().unwrap() as u64, Generation::V7);
        let grid: Vec<IVec2> = s["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| IVec2::new(c[0].as_i64().unwrap() as i32, c[1].as_i64().unwrap() as i32))
            .collect();
        let tangled = order_dependent(&end, &grid);
        let mut bases = Vec::new();
        let mut failures = Vec::new();
        let mut sections = rustc_hash::FxHashMap::default();
        for c in s["chunks"].as_array().unwrap() {
            let c = c.as_array().unwrap();
            let chunk = IVec2::new(c[0].as_i64().unwrap() as i32, c[1].as_i64().unwrap() as i32);
            assert!(!c[7].as_bool().unwrap(), "a city in {chunk}");
            let blocks = column(&end, &mut sections, chunk);
            let mut stone: Vec<IVec3> =
                blocks.iter().filter(|(_, b)| *b == Block::END_STONE).map(|&(p, _)| p).collect();
            stone.sort_by_key(|p| (p.x, p.y, p.z));
            let stone_text: Vec<String> = stone.iter().map(|p| format!("{} {} {};", p.x, p.y, p.z)).collect();
            let mut chorus: Vec<(IVec3, String)> =
                blocks.iter().filter_map(|&(p, b)| chorus_name(b).map(|n| (p, n))).collect();
            chorus.sort_by(|a, b| (a.0.x, a.0.y, a.0.z, &a.1).cmp(&(b.0.x, b.0.y, b.0.z, &b.1)));
            let chorus_text: Vec<String> =
                chorus.iter().map(|(p, n)| format!("{} {} {} {n};", p.x, p.y, p.z)).collect();
            let got = (
                stone.len() as u64,
                fnv(stone_text.iter().map(String::as_str)),
                chorus.len() as u64,
                fnv(chorus_text.iter().map(String::as_str)),
            );
            let want = (
                c[3].as_u64().unwrap(),
                c[4].as_str().unwrap().to_string(),
                c[5].as_u64().unwrap(),
                c[6].as_str().unwrap().to_string(),
            );
            if got.0 != want.0 || got.1 != want.1 {
                failures.push(format!("seed {seed} chunk {chunk} ({}): end stone {got:?} != {want:?}", c[2]));
            }
            if tangled.contains(&chunk) {
                dependent += 1;
                dependent_differ += usize::from(got != want);
                continue;
            }
            free += 1;
            if got.2 != want.2 || got.3 != want.3 {
                failures.push(format!("seed {seed} chunk {chunk} ({}): chorus {got:?} != {want:?}", c[2]));
            }
            let stone_at: rustc_hash::FxHashSet<IVec3> = stone.iter().copied().collect();
            for (p, n) in &chorus {
                if n == "chorus_plant" && stone_at.contains(&(*p - IVec3::Y)) {
                    bases.push(*p);
                }
            }
        }
        let mut want: Vec<IVec3> = s["chorus_bases"]
            .as_array()
            .unwrap()
            .iter()
            .map(ivec)
            .filter(|p| !tangled.contains(&IVec2::new(p.x >> 4, p.z >> 4)))
            .collect();
        want.sort_by_key(|p| (p.x, p.y, p.z));
        bases.sort_by_key(|p| (p.x, p.y, p.z));
        all_failures.extend(failures);
        if bases != want {
            all_failures.push(format!("seed {seed}: chorus bases {bases:?} != {want:?}"));
        }
    }
    eprintln!(
        "{free} chunks whose chorus can't meet a neighbour's; {dependent} whose can, {dependent_differ} of them differing"
    );
    assert!(all_failures.is_empty(), "{} differ:\n{}", all_failures.len(), all_failures.join("\n"));
    assert!(free > 2000 && dependent_differ * 10 < dependent, "{free} {dependent} {dependent_differ}");
}

/// `end_gateway_return`: every return gateway in the grids and in the
/// chunks within 128 of the centre where its rarity filter passes is
/// Java's, at Java's cell, and none is placed where Java has none.
#[test]
fn return_gateways_are_javas() {
    let f = fixture();
    for (seed, s) in f["seeds"].as_object().unwrap() {
        let end = EndGen::with_generation(seed.parse::<i64>().unwrap() as u64, Generation::V7);
        let want: Vec<IVec3> = s["gateways"].as_array().unwrap().iter().map(ivec).collect();
        assert!(want.len() >= 10, "seed {seed}: {}", want.len());
        let mut chunks: Vec<IVec2> = s["gateway_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| IVec2::new(c[0].as_i64().unwrap() as i32, c[1].as_i64().unwrap() as i32))
            .collect();
        let grid = s["grid"].as_array().unwrap();
        let (lo, hi) = (ivec2(&grid[0]), ivec2(&grid[1]));
        chunks.extend((lo.x..=hi.x).flat_map(|x| (lo.y..=hi.y).map(move |z| IVec2::new(x, z))));
        let mut got: Vec<IVec3> = chunks.iter().filter_map(|&c| end.return_gateway(c)).collect();
        got.sort_by_key(|p| (p.x, p.y, p.z));
        let mut want = want;
        want.sort_by_key(|p| (p.x, p.y, p.z));
        assert_eq!(got, want, "seed {seed}");
        for g in &want {
            assert_eq!(end.return_gateway_at(*g), Some(super::RETURN_EXIT));
            let l = crate::world::chunk::local_of(*g);
            let data = end.generate(crate::world::chunk::chunk_of(*g));
            assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize), Block::END_GATEWAY, "{g}");
        }
    }
}

fn ivec2(v: &Value) -> IVec2 {
    IVec2::new(v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32)
}
