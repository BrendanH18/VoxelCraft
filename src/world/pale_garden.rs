//! Generation 3 pale garden features (Java 26.3): the `pale_oak` tree
//! (`DarkOakTrunkPlacer(6, 2, 1)`, `DarkOakFoliagePlacer(0, 0)`, the
//! `PaleMossDecorator(0.8, 0.15, 0.4)`), the `pale_oak_creaking` variant
//! that `pale_garden_vegetation` picks one time in ten and which adds
//! `CreakingHeartDecorator(1.0)`, and the closed eyeblossom placements
//! `pale_garden_flowers` and `flower_pale_garden`.
//!
//! Random draws come from the engine's positional SplitMix64 streams, not
//! Java's worldgen random sequence; counts, ranges, odds and shapes follow
//! the reference. Sources and differences: the 2026-11-05 creaking record.
use super::{
    creaking,
    noise::{hash3, splitmix64},
    overworld_blocks as ob,
    trees::Put,
};
use glam::IVec3;
use rustc_hash::FxHashSet;

struct Rng(u64);
impl Rng {
    fn int(&mut self, n: u32) -> i32 {
        (splitmix64(&mut self.0) % u64::from(n)) as i32
    }
    fn float(&mut self) -> f32 {
        (splitmix64(&mut self.0) >> 40) as f32 / (1u64 << 24) as f32
    }
}

const SIDES: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

/// A generation 3 pale oak on `ground` (the soil under its north-west trunk
/// column). `creaking` selects `pale_oak_creaking`. Blocks go to `put` in
/// placement order: logs, leaves, hanging moss, then the heart, which
/// replaces a log (callers must let it).
pub fn pale_oak(ground: IVec3, seed: u64, creaking: bool, put: Put) {
    let mut rng = Rng(seed ^ 0x9A1E_0A4C);
    let origin = ground + IVec3::Y;
    // `TrunkPlacer.getTreeHeight`: base + nextInt(a + 1) + nextInt(b + 1).
    let height = 6 + rng.int(3) + rng.int(2);
    let dir = [IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X][rng.int(4) as usize];
    let bend_at = height - rng.int(4);
    let mut bends = 2 - rng.int(3);
    let (mut x, mut z) = (origin.x, origin.z);
    let top = origin.y + height - 1;
    let mut logs: Vec<IVec3> = Vec::new();
    let mut taken: FxHashSet<IVec3> = FxHashSet::default();
    let mut log = |p: IVec3, logs: &mut Vec<IVec3>| {
        if taken.insert(p) {
            logs.push(p);
        }
    };
    for i in 0..height {
        if i >= bend_at && bends > 0 {
            x += dir.x;
            z += dir.z;
            bends -= 1;
        }
        let p = IVec3::new(x, origin.y + i, z);
        for d in [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z] {
            log(p + d, &mut logs);
        }
    }
    // Foliage attachments: the shifted double trunk, then each branch.
    let mut attachments = vec![(IVec3::new(x, top, z), true)];
    for dx in -1..=2 {
        for dz in -1..=2 {
            if (!(0..=1).contains(&dx) || !(0..=1).contains(&dz)) && rng.int(3) == 0 {
                let len = rng.int(3) + 2;
                for k in 0..len {
                    log(IVec3::new(origin.x + dx, top - k - 1, origin.z + dz), &mut logs);
                }
                attachments.push((IVec3::new(origin.x + dx, top, origin.z + dz), false));
            }
        }
    }
    let log_set: FxHashSet<IVec3> = logs.iter().copied().collect();
    let mut leaves: Vec<IVec3> = Vec::new();
    let mut leaf_set: FxHashSet<IVec3> = FxHashSet::default();
    for (at, large) in attachments {
        let rows: &[(i32, i32)] = if large { &[(2, -1), (3, 0), (2, 1)] } else { &[(2, -1), (1, 0)] };
        let mut place_rows = |rows: &[(i32, i32)], leaves: &mut Vec<IVec3>| {
            for &(range, y) in rows {
                leaves_row(at, range, y, large, &mut |p| {
                    if !log_set.contains(&p) && leaf_set.insert(p) {
                        leaves.push(p);
                    }
                });
            }
        };
        place_rows(rows, &mut leaves);
        if large && rng.int(2) == 1 {
            place_rows(&[(0, 2)], &mut leaves);
        }
    }
    for &p in &logs {
        put(p, ob::PALE_OAK_LOG);
    }
    for &p in &leaves {
        put(p, ob::PALE_OAK_LEAVES);
    }
    // `PaleMossDecorator`: its ground patch is left to the biome's moss
    // surface; trunk and leaf hangers follow the reference odds.
    let mut moss: FxHashSet<IVec3> = FxHashSet::default();
    let air = |p: IVec3, moss: &FxHashSet<IVec3>| {
        p.y > ground.y && !log_set.contains(&p) && !leaf_set.contains(&p) && !moss.contains(&p)
    };
    let mut shuffled = logs.clone();
    shuffle(&mut shuffled, &mut rng);
    let _ = rng.float(); // The ground patch roll.
    for (list, chance) in [(&logs, 0.4), (&leaves, 0.15)] {
        for &p in list {
            if rng.float() < chance && air(p - IVec3::Y, &moss) {
                let mut q = p - IVec3::Y;
                while air(q - IVec3::Y, &moss) && rng.float() >= 0.5 {
                    moss.insert(q);
                    put(q, ob::PALE_HANGING_MOSS);
                    q -= IVec3::Y;
                }
                moss.insert(q);
                put(q, ob::PALE_HANGING_MOSS_TIP);
            }
        }
    }
    if creaking && let Some(p) = heart_site(&logs, &log_set, &mut rng) {
        put(p, creaking::block(0, creaking::Activity::Dormant, true));
    }
}

/// `CreakingHeartDecorator.place` with probability 1: the first of the
/// shuffled logs whose six neighbours are all logs (`#logs`).
fn heart_site(logs: &[IVec3], log_set: &FxHashSet<IVec3>, rng: &mut Rng) -> Option<IVec3> {
    let _ = rng.float(); // `nextFloat() >= probability` never rejects at 1.0.
    let mut order = logs.to_vec();
    shuffle(&mut order, rng);
    order.into_iter().find(|p| SIDES.iter().all(|d| log_set.contains(&(*p + *d))))
}

fn shuffle(v: &mut [IVec3], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        v.swap(i, rng.int(i as u32 + 1) as usize);
    }
}

/// `FoliagePlacer.placeLeavesRow` with `DarkOakFoliagePlacer`'s skips.
fn leaves_row(at: IVec3, range: i32, y: i32, large: bool, place: &mut dyn FnMut(IVec3)) {
    let extra = i32::from(large);
    for dx in -range..=range + extra {
        for dz in -range..=range + extra {
            if !skip_signed(dx, y, dz, range, large) {
                place(at + IVec3::new(dx, y, dz));
            }
        }
    }
}

fn skip_signed(dx: i32, y: i32, dz: i32, range: i32, large: bool) -> bool {
    if y == 0 && large && (dx == -range || dx >= range) && (dz == -range || dz >= range) {
        return true;
    }
    let (x, z) =
        if large { (dx.abs().min((dx - 1).abs()), dz.abs().min((dz - 1).abs())) } else { (dx.abs(), dz.abs()) };
    match y {
        -1 if !large => x == range && z == range,
        1 => x + z > range * 2 - 2,
        _ => false,
    }
}

/// `pale_garden_vegetation`: one tree in ten is `pale_oak_creaking`.
pub fn creaking_tree(seed: u64) -> bool {
    let mut s = seed ^ 0xC2EA_4B1E;
    (splitmix64(&mut s) >> 40) as f32 / ((1u64 << 24) as f32) < 0.1
}

/// Closed eyeblossom attempts that may land in the chunk column at
/// `(chunk_x, chunk_z)` (block coordinates of its corner): every
/// `pale_garden_flowers` patch (one chunk in eight, 96 tries spread by
/// trapezoid offsets of ±7/±3/±7) and `flower_pale_garden` (one chunk in 32,
/// a single flower) from this and the neighbouring chunks. `surface` gives
/// the column height used as the `MOTION_BLOCKING_NO_LEAVES` heightmap and
/// whether the column is in a pale garden. Each returned position still needs
/// air there and a soil below (the feature's `simple_block` survival test).
pub fn eyeblossom_attempts(
    seed: u64,
    chunk_x: i32,
    chunk_z: i32,
    surface: &mut dyn FnMut(i32, i32) -> Option<i32>,
    out: &mut Vec<IVec3>,
) {
    for cz in -1..=1 {
        for cx in -1..=1 {
            let (bx, bz) = (chunk_x + cx * 16, chunk_z + cz * 16);
            let mut rng = Rng(hash3(bx >> 4, 0x3B, bz >> 4, seed ^ 0xE7EB));
            // `pale_garden_flowers`: rarity 8, in_square, heightmap, biome, count 96.
            if rng.int(8) == 0 {
                let (x, z) = (bx + rng.int(16), bz + rng.int(16));
                if let Some(h) = surface(x, z) {
                    let tri = |rng: &mut Rng, half: u32| rng.int(half + 1) + rng.int(half + 1) - half as i32;
                    for _ in 0..96 {
                        let (dx, dy, dz) = (tri(&mut rng, 7), tri(&mut rng, 3), tri(&mut rng, 7));
                        let p = IVec3::new(x + dx, h + 1 + dy, z + dz);
                        if (p.x - chunk_x) as u32 >= 16 || (p.z - chunk_z) as u32 >= 16 {
                            continue;
                        }
                        out.push(p);
                    }
                }
            }
            // `flower_pale_garden`: rarity 32, in_square, heightmap, biome.
            if cx == 0 && cz == 0 && rng.int(32) == 0 {
                let (x, z) = (bx + rng.int(16), bz + rng.int(16));
                if let Some(h) = surface(x, z) {
                    out.push(IVec3::new(x, h + 1, z));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    fn grow(seed: u64, creaking: bool) -> Vec<(IVec3, Block)> {
        let mut out = Vec::new();
        pale_oak(IVec3::new(0, 64, 0), seed, creaking, &mut |p, b| out.push((p, b)));
        out
    }

    #[test]
    fn pale_oak_matches_the_dark_oak_placers() {
        for seed in 0..200 {
            let tree = grow(seed, false);
            let logs: Vec<_> = tree.iter().filter(|(_, b)| *b == ob::PALE_OAK_LOG).map(|(p, _)| *p).collect();
            // A 2x2 trunk of 6..=9 rows, bent at most two blocks.
            let base: Vec<_> = logs.iter().filter(|p| p.y == 65).collect();
            assert!(base.len() >= 4, "{seed}");
            let top = logs.iter().map(|p| p.y).max().unwrap();
            assert!((70..=73).contains(&top), "{seed}: {top}");
            for (p, _) in &tree {
                assert!((p.x).abs() <= super::super::trees::REACH && (p.z).abs() <= super::super::trees::REACH);
                assert!(p.y - 64 <= super::super::trees::TOP);
            }
            assert!(tree.iter().any(|(_, b)| *b == ob::PALE_OAK_LEAVES));
            assert!(!tree.iter().any(|(_, b)| creaking::heart(*b).is_some()));
        }
    }

    #[test]
    fn creaking_trees_hold_an_enclosed_natural_dormant_heart_when_one_fits() {
        let mut hearts = 0;
        for seed in 0..400 {
            let tree = grow(seed, true);
            let logs: FxHashSet<_> = tree.iter().filter(|(_, b)| *b == ob::PALE_OAK_LOG).map(|(p, _)| *p).collect();
            for (p, b) in &tree {
                if let Some((axis, activity, natural)) = creaking::heart(*b) {
                    hearts += 1;
                    assert_eq!((axis, activity, natural), (0, creaking::Activity::Dormant, true));
                    assert!(SIDES.iter().all(|d| logs.contains(&(*p + *d))), "{seed}");
                }
            }
        }
        // Branches enclose a trunk cell often but not always.
        assert!((40..400).contains(&hearts), "{hearts}");
        let creaking = (0..10_000u64).filter(|&s| creaking_tree(s)).count();
        assert!((900..1100).contains(&creaking), "{creaking}");
    }

    #[test]
    fn eyeblossom_attempts_are_chunk_local_and_order_independent() {
        let mut flat = |_: i32, _: i32| Some(70);
        let mut total = 0;
        for c in -8..8 {
            let mut a = Vec::new();
            eyeblossom_attempts(5, c * 16, 32, &mut flat, &mut a);
            let mut b = Vec::new();
            eyeblossom_attempts(5, c * 16, 32, &mut flat, &mut b);
            assert_eq!(a, b);
            for p in &a {
                assert!((p.x - c * 16) as u32 <= 15 && (p.z - 32) as u32 <= 15);
                assert!((68..=74).contains(&p.y));
            }
            total += a.len();
        }
        assert!(total > 0);
    }
    /// A generation 3 pale garden holds natural creaking hearts and closed
    /// eyeblossoms; generation 2 at the same place holds neither.
    fn pale_garden_chunks(generation: crate::world::terrain::Generation) -> (usize, usize, f64) {
        use crate::world::terrain::{Biome, Dimension, Generator};
        let g = Generator::with_generation(42, Dimension::Overworld, generation);
        let centre = (0..400)
            .flat_map(|i| (0..400).map(move |j| (i * 64 - 12800, j * 64 - 12800)))
            .find(|&(x, z)| g.column(x, z).biome == Biome::PaleGarden)
            .expect("seed 42 has a pale garden");
        let (cx, cz) = (centre.0 >> 5, centre.1 >> 5);
        let cy = g.column(centre.0, centre.1).height >> 5;
        let (mut hearts, mut flowers) = (0, 0);
        let start = std::time::Instant::now();
        let mut n = 0;
        for x in cx - 3..=cx + 3 {
            for z in cz - 3..=cz + 3 {
                for y in cy - 1..=cy + 1 {
                    n += 1;
                    let data = g.generate(IVec3::new(x, y, z));
                    for i in 0..super::super::chunk::CHUNK_VOLUME {
                        let s = super::super::chunk::CHUNK_SIZE;
                        let b = data.get(i % s, i / (s * s), i / s % s);
                        hearts += usize::from(creaking::heart(b).is_some());
                        flowers += usize::from(b == super::super::eyeblossom::CLOSED);
                    }
                }
            }
        }
        (hearts, flowers, start.elapsed().as_secs_f64() * 1000. / n as f64)
    }

    #[test]
    fn generation_3_pale_gardens_grow_hearts_and_eyeblossoms() {
        let (hearts, flowers, _) = pale_garden_chunks(crate::world::terrain::Generation::V3);
        assert!(hearts > 0 && flowers > 0, "{hearts} hearts, {flowers} eyeblossoms");
        let (hearts, flowers, _) = pale_garden_chunks(crate::world::terrain::Generation::V2);
        assert_eq!((hearts, flowers), (0, 0));
    }

    /// `cargo test --release pale_garden_generation_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn pale_garden_generation_cost() {
        use crate::world::terrain::Generation;
        for _ in 0..2 {
            for generation in [Generation::V2, Generation::V3] {
                let (h, f, ms) = pale_garden_chunks(generation);
                println!("pale garden {generation:?}: {ms:.3} ms/chunk, {h} hearts, {f} eyeblossoms");
            }
        }
    }
}
