//! Java 26.3 poplar trees (GEN-03, v0.16 Track D): `orange_poplar`,
//! `red_poplar` and `yellow_poplar` (`PoplarTrunkPlacer(7, 4, 0)` with
//! `trunk_height_above_branches` 4 and one to four branches,
//! `PoplarFoliagePlacer` with radius 5/6/7/8 weighted 5:5:1:1, height 5..=6
//! and side-hole chance 0.15, and `ShelfMushroomDecorator(0.4)`), their
//! `_leaf_litter` variants (two `PlaceOnGroundDecorator`s of leaf litter),
//! and `fallen_poplar_tree` (`FallenTreeFeature`, log length 4..=7, brown
//! mushrooms on 10% of the logs and `ShelfMushroomDecorator(0.8)`).
//!
//! Random draws come from the engine's positional SplitMix64 streams, not
//! Java's worldgen random sequence (FE-01); counts, ranges, odds and shapes
//! follow the reference. Sources: `docs/verification/2026-11-20-v0.16-generation-5-overworld.md`.

use glam::IVec3;
use rustc_hash::FxHashSet;

use super::biome_plants as bp;
use super::block::{Block, Facing};
use super::gen5_blocks::shelf_mushroom;
use super::noise::splitmix64;
use super::trees::Put;
use super::wood::poplar;

struct Rng(u64);
impl Rng {
    fn int(&mut self, n: u32) -> i32 {
        (splitmix64(&mut self.0) % u64::from(n)) as i32
    }
    fn float(&mut self) -> f32 {
        (splitmix64(&mut self.0) >> 40) as f32 / (1u64 << 24) as f32
    }
    fn between(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.int((hi - lo + 1) as u32)
    }
    fn facing(&mut self) -> Facing {
        Facing::ALL[self.int(4) as usize]
    }
}

/// The log on `axis` (0 Y, 1 X, 2 Z).
fn log(axis: u16) -> Block {
    poplar::pillar(0, axis)
}

const fn axis_of(f: Facing) -> u16 {
    if f.along_x() { 1 } else { 2 }
}

/// Java's foliage rows: the rhombus test of `PoplarFoliagePlacer`.
fn within_rhombus(r: i32, dx: i32, dz: i32, corner: i32, extra: i32) -> bool {
    dx + dz <= r * 2 - (corner + extra)
}

fn corner_cut(dx: i32, dz: i32, r: i32, partial: bool, flip: bool) -> i32 {
    let small = if flip { (dx > 0 && dz > 0) || (dz < 0 && dx < 0) } else { (dx > 0 && dz < 0) || (dz > 0 && dx < 0) };
    if small {
        r - 1
    } else if partial {
        r + 1
    } else {
        r
    }
}

/// `PoplarTrunkPlacer`: base height, `height_rand_a` and
/// `trunk_height_above_branches`; branches one to [`MAX_BRANCHES`].
pub const TRUNK: (i32, i32, i32) = (7, 4, 4);
pub const MAX_BRANCHES: i32 = 4;
/// `PoplarFoliagePlacer`: radius weights, height range, side-hole chance.
pub const RADII: [(i32, i32); 4] = [(5, 5), (6, 5), (7, 1), (8, 1)];
pub const FOLIAGE_HEIGHT: (i32, i32) = (5, 6);
pub const SIDE_HOLE_CHANCE: f32 = 0.15;
/// `ShelfMushroomDecorator` chances: standing poplars, fallen ones.
pub const SHELF_CHANCE: f32 = 0.4;
pub const FALLEN_SHELF_CHANCE: f32 = 0.8;
/// `fallen_poplar_tree`: log length and the brown mushroom chance.
pub const FALLEN_LENGTH: (i32, i32) = (4, 7);
pub const FALLEN_MUSHROOM_CHANCE: f32 = 0.1;
/// `trees_dappled_forest`'s weighted selector, in Java's order.
pub const DAPPLED_SELECTOR: [(&str, u64); 5] = [
    ("red_poplar_leaf_litter", 200),
    ("orange_poplar_leaf_litter", 240),
    ("yellow_poplar_leaf_litter", 90),
    ("spruce_checked", 27),
    ("fallen_poplar_tree", 120),
];

/// Where the ground is under `(x, z)` for the leaf-litter decorators: the
/// top solid block's height, if any.
pub type Ground<'a> = &'a dyn Fn(i32, i32) -> Option<i32>;

/// A poplar of `colour` (0 orange, 1 red, 2 yellow) on `ground` (the soil
/// under its trunk). With `litter` it is the `_leaf_litter` variant and
/// scatters leaf litter on the ground around it. Blocks go to `put`:
/// logs, leaves, then decorations.
pub fn poplar(colour: u16, ground: IVec3, seed: u64, litter: Option<Ground>, put: Put) {
    let mut rng = Rng(seed ^ 0x0090_71A5);
    let origin = ground + IVec3::Y;
    // `TreeFeature.doPlace`: trunk height, foliage height, then radius.
    let tree_height = TRUNK.0 + rng.int(TRUNK.1 as u32 + 1);
    let foliage_height = rng.between(FOLIAGE_HEIGHT.0, FOLIAGE_HEIGHT.1);
    let total: i32 = RADII.iter().map(|r| r.1).sum();
    let mut pick = rng.int(total as u32);
    let mut leaf_radius = RADII[0].0;
    for (radius, weight) in RADII {
        if pick < weight {
            leaf_radius = radius;
            break;
        }
        pick -= weight;
    }
    let leaves = poplar::LEAVES[colour.min(2) as usize];
    let mut logs: Vec<(IVec3, Block)> = Vec::new();
    // `PoplarTrunkPlacer.placeTrunk`: branches one block out of the trunk,
    // `trunk_height_above_branches` (4) below the top.
    let up_to_branches = tree_height - TRUNK.2;
    for y in 0..tree_height {
        logs.push((origin + IVec3::Y * y, log(0)));
        if y == up_to_branches - 1 {
            let mut dirs = Facing::ALL;
            for i in (1..4).rev() {
                dirs.swap(i, rng.int(i as u32 + 1) as usize);
            }
            let branches = 1 + rng.int(MAX_BRANCHES as u32);
            for &f in dirs.iter().take(branches as usize) {
                logs.push((origin + IVec3::Y * y + f.offset(), log(axis_of(f))));
            }
        }
    }
    let log_cells: FxHashSet<IVec3> = logs.iter().map(|&(p, _)| p).collect();
    // `PoplarFoliagePlacer.createFoliage` at the one attachment.
    let at = origin + IVec3::Y * up_to_branches;
    let r = leaf_radius - 1;
    let flip = rng.int(2) == 1;
    let fh = foliage_height;
    let partial = |y: i32| fh - 1 == y || fh - 2 == y;
    let mut leaf_cells: Vec<IVec3> = Vec::new();
    let mut leaf_set: FxHashSet<IVec3> = FxHashSet::default();
    let row = |radius: i32, y: i32, rng: &mut Rng, cells: &mut Vec<IVec3>, set: &mut FxHashSet<IVec3>| {
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let (ax, az) = (dx.abs(), dz.abs());
                let corner = corner_cut(dx, dz, radius, partial(y), flip);
                let edge = ax == radius || az == radius;
                if partial(y) && edge {
                    continue;
                }
                let extra = i32::from(rng.float() <= SIDE_HOLE_CHANCE);
                if !within_rhombus(radius, ax, az, corner, extra) {
                    continue;
                }
                let p = at + IVec3::new(dx, y, dz);
                if !log_cells.contains(&p) && set.insert(p) {
                    cells.push(p);
                }
            }
        }
    };
    row(r - 2, fh - 1, &mut rng, &mut leaf_cells, &mut leaf_set);
    row(r - 1, fh - 2, &mut rng, &mut leaf_cells, &mut leaf_set);
    row(r - 1, fh - 3, &mut rng, &mut leaf_cells, &mut leaf_set);
    for y in (1..=fh - 4).rev() {
        row(r, y, &mut rng, &mut leaf_cells, &mut leaf_set);
    }
    // `replaceLeavesWithLog`: the long arms of the widest row become logs.
    let y = fh - 4;
    let mut branch_logs: Vec<(IVec3, Block)> = Vec::new();
    for dx in -r..=r {
        for dz in -r..=r {
            let (ax, az) = (dx.abs(), dz.abs());
            let corner = corner_cut(dx, dz, r, partial(y), flip);
            if within_rhombus(r, ax, az, corner, 2) && ((az == 0 && r - ax >= 4) || (ax == 0 && r - az >= 4)) {
                let p = at + IVec3::new(dx, y, dz);
                if leaf_set.remove(&p) {
                    branch_logs.push((p, log(if az == 0 { 1 } else { 2 })));
                }
            }
        }
    }
    row(r - 1, 0, &mut rng, &mut leaf_cells, &mut leaf_set);
    row((r - 2).clamp(1, 2), -1, &mut rng, &mut leaf_cells, &mut leaf_set);
    logs.extend(branch_logs);
    for &(p, b) in &logs {
        put(p, b);
    }
    for p in leaf_cells {
        if leaf_set.contains(&p) {
            put(p, leaves);
        }
    }
    if let Some(ground_at) = litter {
        // Two `place_on_ground` decorators around the trunk's base: 96
        // tries in radius 4 (one to three segments), then 150 in radius 2
        // (one to four), both two blocks up and down.
        for (tries, radius, segments) in [(96, 4, 3), (150, 2, 4)] {
            for _ in 0..tries {
                let dx = rng.between(-radius, radius);
                let dy = rng.between(-2, 2);
                let dz = rng.between(-radius, radius);
                let p = origin + IVec3::new(dx, dy, dz);
                let n = 1 + rng.int(segments);
                if ground_at(p.x, p.z) == Some(p.y) && !log_cells.contains(&(p + IVec3::Y)) {
                    put(p + IVec3::Y, bp::with_segments(bp::LEAF_LITTER, n as u8));
                }
            }
        }
    }
    // `ShelfMushroomDecorator(0.4)` on the standing trunk.
    if rng.float() < SHELF_CHANCE {
        let first = rng.facing();
        let dirs = [first, first.clockwise()];
        let mut sorted = logs.clone();
        sorted.sort_by_key(|(p, _)| p.y);
        let mut placed: FxHashSet<IVec3> = FxHashSet::default();
        for (p, _) in sorted {
            let dy = p.y - origin.y;
            if !(1..=4).contains(&dy) {
                continue;
            }
            for f in dirs {
                if rng.float() > 0.25 {
                    continue;
                }
                let q = p + f.offset();
                if log_cells.contains(&q) || leaf_set.contains(&q) || placed.contains(&(q - IVec3::Y)) {
                    continue;
                }
                placed.insert(q);
                put(q, shelf_mushroom(f, rng.int(2) as u16));
                break;
            }
        }
    }
}

/// `fallen_poplar_tree`: a stump on `ground`, and two to five logs lying
/// two or three blocks away in a random direction, on level ground
/// (`ground_at` gives the ground height of each column), decorated with
/// brown mushrooms on top (10%) and shelf mushrooms on their sides (80% of
/// trees, a quarter of the cells).
pub fn fallen_poplar(ground: IVec3, seed: u64, ground_at: Ground, put: Put) {
    let mut rng = Rng(seed ^ 0x00FA_11E7);
    let stump = ground + IVec3::Y;
    put(stump, log(0));
    let dir = rng.facing();
    let length = rng.between(FALLEN_LENGTH.0, FALLEN_LENGTH.1) - 2;
    let start = stump + dir.offset() * (2 + rng.int(2));
    // `setGroundHeightForFallenLogStartPos` and `canPlaceEntireFallenLog`:
    // the log lies on the ground at its start, with at most two cells of
    // gap under it.
    let Some(h) = ground_at(start.x, start.z) else { return };
    if (h - ground.y).abs() > 5 {
        return;
    }
    let y = h + 1;
    let mut gap = 0;
    for i in 0..length {
        let p = start + dir.offset() * i;
        match ground_at(p.x, p.z) {
            Some(g) if g == y - 1 => gap = 0,
            Some(g) if g < y - 1 => {
                gap += 1;
                if gap > 2 {
                    return;
                }
            }
            _ => return,
        }
    }
    let cells: Vec<IVec3> = (0..length).map(|i| IVec3::new(start.x, y, start.z) + dir.offset() * i).collect();
    for &p in &cells {
        put(p, log(axis_of(dir)));
    }
    // `attached_to_logs`: a brown mushroom on top of a tenth of the logs.
    for &p in &cells {
        if rng.float() < FALLEN_MUSHROOM_CHANCE {
            put(p + IVec3::Y, Block::BROWN_MUSHROOM);
        }
    }
    // `ShelfMushroomDecorator(0.8)` on a fallen log: both sides across it.
    if rng.float() < FALLEN_SHELF_CHANCE {
        let sides = [dir.clockwise(), dir.clockwise().opposite()];
        let mut placed: FxHashSet<IVec3> = FxHashSet::default();
        let near = |q: IVec3, placed: &FxHashSet<IVec3>| Facing::ALL.iter().any(|f| placed.contains(&(q + f.offset())));
        for &p in &cells {
            for f in sides {
                if rng.float() > 0.25 {
                    continue;
                }
                let q = p + f.offset();
                if !near(q, &placed) && !near(p, &placed) {
                    placed.insert(q);
                    put(q, shelf_mushroom(f, rng.int(2) as u16));
                }
            }
        }
    }
}

/// One `trees_dappled_forest` attempt: Java's weighted selector of red
/// (200), orange (240) and yellow (90) leaf-litter poplars, a spruce (27)
/// and a fallen poplar (120).
pub fn dappled_forest_tree(ground: IVec3, seed: u64, ground_at: Ground, put: Put) {
    let total: u64 = DAPPLED_SELECTOR.iter().map(|e| e.1).sum();
    let mut pick = (seed >> 40) % total;
    let mut choice = 0;
    for (i, &(_, weight)) in DAPPLED_SELECTOR.iter().enumerate() {
        if pick < weight {
            choice = i;
            break;
        }
        pick -= weight;
    }
    match choice {
        0 => poplar(1, ground, seed, Some(ground_at), put),
        1 => poplar(0, ground, seed, Some(ground_at), put),
        2 => poplar(2, ground, seed, Some(ground_at), put),
        3 => super::trees::spruce(ground, seed as u32, put),
        _ => fallen_poplar(ground, seed, ground_at, put),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constants above against `tests/fixtures/java/26.3/gen5_overworld.json`
    /// (`tools/extract_java_gen5_fixtures.py`).
    #[test]
    fn poplar_constants_match_the_java_26_3_features() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/gen5_overworld.json")).unwrap();
        for colour in ["orange", "red", "yellow"] {
            for suffix in ["", "_leaf_litter"] {
                let t = &f["poplar_trees"][format!("{colour}_poplar{suffix}")];
                let trunk = &t["trunk_placer"];
                assert_eq!(trunk["base_height"], TRUNK.0);
                assert_eq!(trunk["height_rand_a"], TRUNK.1);
                assert_eq!(trunk["height_rand_b"], 0);
                assert_eq!(trunk["trunk_height_above_branches"], TRUNK.2);
                assert_eq!(trunk["branch_amount"]["min_inclusive"], 1);
                assert_eq!(trunk["branch_amount"]["max_inclusive"], MAX_BRANCHES);
                let foliage = &t["foliage_placer"];
                let radii: Vec<(i64, i64)> = foliage["radius"]["distribution"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| (e["data"].as_i64().unwrap(), e["weight"].as_i64().unwrap()))
                    .collect();
                assert_eq!(radii, RADII.map(|(r, w)| (i64::from(r), i64::from(w))));
                assert_eq!(foliage["height"]["min_inclusive"], FOLIAGE_HEIGHT.0);
                assert_eq!(foliage["height"]["max_inclusive"], FOLIAGE_HEIGHT.1);
                assert_eq!(foliage["side_hole_chance"].as_f64().unwrap() as f32, SIDE_HOLE_CHANCE);
                assert_eq!(foliage["offset"], 0);
                assert_eq!(t["foliage"], format!("{colour}_poplar_leaves"));
                let shelf = t["decorators"].as_array().unwrap().iter().find(|d| d["type"] == "shelf_mushroom").unwrap();
                assert_eq!(shelf["probability"].as_f64().unwrap() as f32, SHELF_CHANCE);
                let litter = t["decorators"].as_array().unwrap().iter().filter(|d| d["type"] == "place_on_ground");
                let litter: Vec<_> = litter.map(|d| (d["tries"].clone(), d["radius"].clone())).collect();
                if suffix.is_empty() {
                    assert!(litter.is_empty());
                } else {
                    assert_eq!(litter.len(), 2);
                    assert_eq!(litter[0], (96.into(), 4.into()));
                    assert_eq!(litter[1].0, 150);
                }
            }
        }
        let fallen = &f["fallen_poplar_tree"];
        assert_eq!(fallen["log_length"]["min_inclusive"], FALLEN_LENGTH.0);
        assert_eq!(fallen["log_length"]["max_inclusive"], FALLEN_LENGTH.1);
        let decorators = fallen["log_decorators"].as_array().unwrap();
        assert_eq!(decorators[0]["probability"].as_f64().unwrap() as f32, FALLEN_MUSHROOM_CHANCE);
        assert_eq!(decorators[1]["probability"].as_f64().unwrap() as f32, FALLEN_SHELF_CHANCE);
        let selector: Vec<(String, u64)> = f["trees_dappled_forest"]["selector"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["feature"].as_str().unwrap().to_owned(), e["weight"].as_u64().unwrap()))
            .collect();
        assert_eq!(selector, DAPPLED_SELECTOR.map(|(n, w)| (n.to_owned(), w)));
        assert_eq!(f["trees_dappled_forest"]["count"], 6);
    }

    fn grow(colour: u16, seed: u64) -> Vec<(IVec3, Block)> {
        let mut out = Vec::new();
        poplar(colour, IVec3::new(0, 64, 0), seed, None, &mut |p, b| out.push((p, b)));
        out
    }

    #[test]
    fn poplars_follow_the_java_placers() {
        for seed in 0..200 {
            let tree = grow((seed % 3) as u16, seed);
            let trunk: Vec<_> =
                tree.iter().filter(|(p, b)| *b == poplar::LOG && p.x == 0 && p.z == 0).map(|(p, _)| p.y).collect();
            let height = trunk.len() as i32;
            assert!((7..=11).contains(&height), "trunk height {height}");
            assert_eq!(trunk.iter().min(), Some(&65));
            // Every block stays within the tree reach the terrain allows.
            for (p, _) in &tree {
                assert!(p.x.abs() <= super::super::trees::REACH && p.z.abs() <= super::super::trees::REACH);
                assert!(p.y - 64 <= super::super::trees::TOP);
            }
            let colour = poplar::LEAVES[(seed % 3) as usize];
            assert!(tree.iter().any(|(_, b)| *b == colour), "{seed}");
            assert!(tree.iter().all(|(_, b)| poplar::is_leaves(*b) == (*b == colour) || !poplar::is_leaves(*b)));
            // Branch logs lie on their side.
            assert!(tree.iter().any(|(p, b)| poplar::axis(*b).is_some_and(|a| a != 0) && p.y > 65));
        }
    }

    #[test]
    fn fallen_poplars_lie_on_level_ground() {
        let mut found = false;
        for seed in 0..50 {
            let mut out = Vec::new();
            fallen_poplar(IVec3::new(0, 64, 0), seed, &|_, _| Some(64), &mut |p, b| out.push((p, b)));
            assert_eq!(out[0], (IVec3::new(0, 65, 0), poplar::LOG));
            let lying: Vec<_> = out.iter().filter(|(_, b)| poplar::axis(*b).is_some_and(|a| a != 0)).collect();
            assert!((2..=5).contains(&lying.len()), "{}", lying.len());
            assert!(lying.iter().all(|(p, _)| p.y == 65));
            found |= out.iter().any(|(_, b)| super::super::gen5_blocks::shelf_of(*b).is_some());
        }
        assert!(found, "shelf mushrooms grow on fallen poplars");
    }
}
