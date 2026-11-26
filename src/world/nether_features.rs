//! Nether biome features, ported from Java's feature classes onto a small
//! [`Level`] interface so world generation and bone meal share them.
//!
//! Each feature reads and writes through `Level` and draws its randomness
//! from the `Rng` it is given, in Java's order where it matters for shape.
//! References: `HugeFungusFeature`, `WeepingVinesFeature`,
//! `TwistingVinesFeature`, `NetherForestVegetationFeature`,
//! `BasaltPillarFeature`, `DeltaFeature`, `BasaltColumnsFeature`,
//! `RandomPatchFeature` and the `nether_fossil` structure.

use glam::IVec3;

use super::block::Block;
use super::nether_biome_blocks::{self as nb, NetherWood, Vine};
use super::structure::Rng;

/// Java's Nether generation depth, build height and sea level.
pub const GEN_DEPTH: i32 = 128;
pub const BUILD_HEIGHT: i32 = 256;
pub const SEA_LEVEL: i32 = 32;

/// Where a feature reads blocks and puts them.
pub trait Level {
    fn block(&self, p: IVec3) -> Block;
    fn place(&mut self, p: IVec3, b: Block);

    fn empty(&self, p: IVec3) -> bool {
        self.block(p) == Block::AIR
    }

    /// Inclusive corners of the only blocks whose writes the caller keeps
    /// (terrain generation paints one chunk), when a feature may skip work
    /// that only decides writes elsewhere.
    fn write_bounds(&self) -> Option<(IVec3, IVec3)> {
        None
    }
}

const SIDES: [IVec3; 4] = [IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];
const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

fn float(rng: &mut Rng) -> f32 {
    rng.unit() as f32
}

/// Java's `random.nextInt(n)`.
fn int(rng: &mut Rng, n: i32) -> i32 {
    rng.below(n as u32) as i32
}

/// Java's `Mth.nextInt(random, lo, hi)` (inclusive).
fn between(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
    if lo >= hi { lo } else { lo + int(rng, hi - lo + 1) }
}

/// What a huge fungus may grow through: replaceable blocks, and for its
/// stem also the small plants it grows from (Java's replaceable predicate).
fn fungus_replaceable(b: Block, stem: bool) -> bool {
    b.is_replaceable()
        || (stem
            && (matches!(b, nb::CRIMSON_FUNGUS | nb::WARPED_FUNGUS) || b.is_sapling() || nb::Vine::of(b).is_some()))
}

/// `HugeFungusFeature.place`. `planted` (bone meal) never makes the rare
/// 3×3-stemmed giant and ignores the build-height check.
pub fn huge_fungus(level: &mut impl Level, rng: &mut Rng, origin: IVec3, wood: NetherWood, planted: bool) -> bool {
    if level.block(origin - IVec3::Y) != wood.nylium() {
        return false;
    }
    let mut height = between(rng, 4, 13);
    if int(rng, 12) == 0 {
        height *= 2;
    }
    if !planted && origin.y + height + 1 >= GEN_DEPTH {
        return false;
    }
    let huge = !planted && float(rng) < 0.06;
    level.place(origin, Block::AIR);
    place_stem(level, rng, origin, wood, height, huge, planted);
    place_hat(level, rng, origin, wood, height, huge, planted);
    true
}

fn place_stem(
    level: &mut impl Level,
    rng: &mut Rng,
    origin: IVec3,
    wood: NetherWood,
    h: i32,
    huge: bool,
    planted: bool,
) {
    let r = huge as i32;
    for dx in -r..=r {
        for dz in -r..=r {
            let corner = huge && dx.abs() == r && dz.abs() == r;
            for y in 0..h {
                let p = origin + IVec3::new(dx, y, dz);
                if !fungus_replaceable(level.block(p), true) {
                    continue;
                }
                if planted || !corner || float(rng) < 0.1 {
                    level.place(p, wood.stem());
                }
            }
        }
    }
}

fn place_hat(
    level: &mut impl Level,
    rng: &mut Rng,
    origin: IVec3,
    wood: NetherWood,
    h: i32,
    huge: bool,
    planted: bool,
) {
    let crimson = wood == NetherWood::Crimson;
    let hat_height = (int(rng, 1 + h / 3) + 5).min(h);
    let start = h - hat_height;
    for y in start..=h {
        let mut radius = if y < h - int(rng, 3) { 2 } else { 1 };
        if hat_height > 8 && y < start + 4 {
            radius = 3;
        }
        if huge {
            radius += 1;
        }
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let edge_x = dx == -radius || dx == radius;
                let edge_z = dz == -radius || dz == radius;
                let inside = !edge_x && !edge_z && y != h;
                let corner = edge_x && edge_z;
                let lower = y < start + 3;
                let p = origin + IVec3::new(dx, y, dz);
                if !fungus_replaceable(level.block(p), false) {
                    continue;
                }
                if planted && level.block(p - IVec3::Y) != Block::AIR && level.block(p) != Block::AIR {
                    level.place(p, Block::AIR);
                }
                if lower {
                    if !inside {
                        hat_drop_block(level, rng, p, wood, crimson);
                    }
                } else if inside {
                    hat_block(level, rng, p, wood, 0.1, 0.2, if crimson { 0.1 } else { 0.0 });
                } else if corner {
                    hat_block(level, rng, p, wood, 0.01, 0.7, if crimson { 0.083 } else { 0.0 });
                } else {
                    hat_block(level, rng, p, wood, 0.0005, 0.98, if crimson { 0.07 } else { 0.0 });
                }
            }
        }
    }
}

fn hat_block(level: &mut impl Level, rng: &mut Rng, p: IVec3, wood: NetherWood, decor: f32, hat: f32, vines: f32) {
    if float(rng) < decor {
        level.place(p, nb::SHROOMLIGHT);
    } else if float(rng) < hat {
        level.place(p, wood.wart());
        if float(rng) < vines {
            hat_vines(level, rng, p);
        }
    }
}

fn hat_drop_block(level: &mut impl Level, rng: &mut Rng, p: IVec3, wood: NetherWood, vines: bool) {
    if level.block(p - IVec3::Y) == wood.wart() {
        level.place(p, wood.wart());
    } else if float(rng) < 0.15 {
        level.place(p, wood.wart());
        if vines && int(rng, 11) == 0 {
            hat_vines(level, rng, p);
        }
    }
}

/// `HugeFungusFeature.tryPlaceWeepingVines`.
fn hat_vines(level: &mut impl Level, rng: &mut Rng, p: IVec3) {
    let below = p - IVec3::Y;
    if level.empty(below) {
        let mut length = between(rng, 1, 5);
        if int(rng, 7) == 0 {
            length *= 2;
        }
        weeping_column(level, rng, below, length, 23, 25);
    }
}

/// `WeepingVinesFeature.placeWeepingVinesColumn`: plant blocks down to a
/// head with an age in `min_age..=max_age`.
pub fn weeping_column(level: &mut impl Level, rng: &mut Rng, mut p: IVec3, length: i32, min_age: i32, max_age: i32) {
    for i in 0..=length {
        if level.empty(p) {
            if i == length || !level.empty(p - IVec3::Y) {
                level.place(p, Vine::Weeping.head(between(rng, min_age, max_age) as u8));
                break;
            }
            level.place(p, nb::WEEPING_VINES_PLANT);
        }
        p -= IVec3::Y;
    }
}

/// `TwistingVinesFeature.placeWeepingVinesColumn` (sic): the same, upward.
pub fn twisting_column(level: &mut impl Level, rng: &mut Rng, mut p: IVec3, length: i32, min_age: i32, max_age: i32) {
    for i in 1..=length {
        if level.empty(p) {
            if i == length || !level.empty(p + IVec3::Y) {
                level.place(p, Vine::Twisting.head(between(rng, min_age, max_age) as u8));
                break;
            }
            level.place(p, nb::TWISTING_VINES_PLANT);
        }
        p += IVec3::Y;
    }
}

fn roof(b: Block) -> bool {
    b == Block::NETHERRACK || b == nb::NETHER_WART_BLOCK
}

/// `WeepingVinesFeature`: a patch of nether wart blocks on the ceiling with
/// weeping vines hanging from it.
pub fn weeping_vines(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    if !level.empty(origin) || !roof(level.block(origin + IVec3::Y)) {
        return false;
    }
    level.place(origin, nb::NETHER_WART_BLOCK);
    for _ in 0..200 {
        let p = origin + IVec3::new(int(rng, 6) - int(rng, 6), int(rng, 2) - int(rng, 5), int(rng, 6) - int(rng, 6));
        if level.empty(p) {
            let mut touching = 0;
            for d in DIRECTIONS {
                if roof(level.block(p + d)) {
                    touching += 1;
                }
                if touching > 1 {
                    break;
                }
            }
            if touching == 1 {
                level.place(p, nb::NETHER_WART_BLOCK);
            }
        }
    }
    for _ in 0..100 {
        let p = origin + IVec3::new(int(rng, 8) - int(rng, 8), int(rng, 2) - int(rng, 7), int(rng, 8) - int(rng, 8));
        if level.empty(p) && roof(level.block(p + IVec3::Y)) {
            let mut length = between(rng, 1, 8);
            if int(rng, 6) == 0 {
                length *= 2;
            }
            if int(rng, 5) == 0 {
                length = 1;
            }
            weeping_column(level, rng, p, length, 17, 25);
        }
    }
    true
}

fn twisting_base_invalid(level: &impl Level, p: IVec3) -> bool {
    !level.empty(p)
        || !matches!(level.block(p - IVec3::Y), Block::NETHERRACK | nb::WARPED_NYLIUM | nb::WARPED_WART_BLOCK)
}

/// `TwistingVinesFeature` (spread width, spread height, max height).
pub fn twisting_vines(level: &mut impl Level, rng: &mut Rng, origin: IVec3, width: i32, height: i32, max: i32) -> bool {
    if twisting_base_invalid(level, origin) {
        return false;
    }
    for _ in 0..width * width {
        let mut p = origin
            + IVec3::new(between(rng, -width, width), between(rng, -height, height), between(rng, -width, width));
        // Java searches down through air to the ground, then steps back up.
        // Offsets inside solid terrain are rejected by the base check.
        loop {
            p -= IVec3::Y;
            if p.y < 0 || p.y >= BUILD_HEIGHT || !level.empty(p) {
                break;
            }
        }
        if p.y < 0 || p.y >= BUILD_HEIGHT {
            continue;
        }
        p += IVec3::Y;
        if !twisting_base_invalid(level, p) {
            let mut length = between(rng, 1, max);
            if int(rng, 6) == 0 {
                length *= 2;
            }
            if int(rng, 5) == 0 {
                length = 1;
            }
            twisting_column(level, rng, p, length, 17, 25);
        }
    }
    true
}

/// The plants a vegetation patch picks from (Java's weighted providers).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vegetation {
    /// Crimson roots 87, crimson fungus 11, warped fungus 1.
    Crimson,
    /// Warped roots 85, crimson roots 1, warped fungus 13, crimson fungus 1.
    Warped,
    NetherSprouts,
}

impl Vegetation {
    /// The weighted plants (sprouts: just sprouts).
    pub fn table(self) -> &'static [(Block, i32)] {
        match self {
            Vegetation::Crimson => &[(nb::CRIMSON_ROOTS, 87), (nb::CRIMSON_FUNGUS, 11), (nb::WARPED_FUNGUS, 1)],
            Vegetation::Warped => {
                &[(nb::WARPED_ROOTS, 85), (nb::CRIMSON_ROOTS, 1), (nb::WARPED_FUNGUS, 13), (nb::CRIMSON_FUNGUS, 1)]
            }
            Vegetation::NetherSprouts => &[(nb::NETHER_SPROUTS, 1)],
        }
    }

    fn pick(self, rng: &mut Rng) -> Block {
        if self == Vegetation::NetherSprouts {
            return nb::NETHER_SPROUTS;
        }
        let table = self.table();
        let mut r = int(rng, table.iter().map(|t| t.1).sum());
        for &(b, w) in table {
            if r < w {
                return b;
            }
            r -= w;
        }
        table[0].0
    }
}

/// `NetherForestVegetationFeature`: plants scattered around a nylium spot.
pub fn vegetation(
    level: &mut impl Level,
    rng: &mut Rng,
    origin: IVec3,
    kind: Vegetation,
    width: i32,
    height: i32,
) -> bool {
    if !nb::is_nylium(level.block(origin - IVec3::Y)) || origin.y < 1 || origin.y + 1 >= BUILD_HEIGHT {
        return false;
    }
    let mut placed = false;
    for _ in 0..width * width {
        let p = origin
            + IVec3::new(
                int(rng, width) - int(rng, width),
                int(rng, height) - int(rng, height),
                int(rng, width) - int(rng, width),
            );
        let plant = kind.pick(rng);
        if level.empty(p) && p.y > 0 && plant.can_stay_on(level.block(p - IVec3::Y)) {
            level.place(p, plant);
            placed = true;
        }
    }
    placed
}

/// `BasaltPillarFeature`: a basalt column from a ceiling down to the floor,
/// with a ragged skirt where it lands.
pub fn basalt_pillar(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    if !level.empty(origin) || level.empty(origin + IVec3::Y) {
        return false;
    }
    let mut p = origin;
    while level.empty(p) {
        if p.y <= 0 {
            return true;
        }
        level.place(p, Block::BASALT);
        for d in SIDES {
            if int(rng, 10) != 0 {
                level.place(p + d, Block::BASALT);
            }
        }
        p -= IVec3::Y;
    }
    p += IVec3::Y;
    for d in SIDES {
        if int(rng, 2) == 0 {
            level.place(p + d, Block::BASALT);
        }
    }
    p -= IVec3::Y;
    for i in -3i32..4 {
        for j in -3i32..4 {
            let k = i.abs() * j.abs();
            if int(rng, 10) < 10 - k {
                let mut q = p + IVec3::new(i, 0, j);
                let mut steps = 3;
                while level.empty(q - IVec3::Y) {
                    q -= IVec3::Y;
                    steps -= 1;
                    if steps <= 0 {
                        break;
                    }
                }
                if !level.empty(q - IVec3::Y) {
                    level.place(q, Block::BASALT);
                }
            }
        }
    }
    true
}

/// Java's `DeltaFeature.CANNOT_REPLACE` (fortress blocks, chests, spawners).
fn delta_protected(b: Block) -> bool {
    matches!(b, Block::BEDROCK | Block::NETHER_BRICKS | Block::NETHER_BRICK_FENCE | Block::CHEST | Block::SPAWNER)
        || b.stairs_base() == Some(Block::NETHER_BRICKS)
        || b.wart_age().is_some()
}

/// A floor-top block a delta may flood: open above, enclosed elsewhere.
fn delta_clear(level: &impl Level, p: IVec3) -> bool {
    let b = level.block(p);
    if b == Block::LAVA || b == Block::AIR || delta_protected(b) {
        return false;
    }
    DIRECTIONS.iter().all(|&d| (level.block(p + d) == Block::AIR) == (d == IVec3::Y))
}

/// Whether `p + k * step` (k ≥ 0, non-negative steps) lies in the inclusive
/// horizontal box `lo..=hi` for some `k`.
fn chain_reaches(p: IVec3, step: (i32, i32), lo: IVec3, hi: IVec3) -> bool {
    debug_assert!(step.0 >= 0 && step.1 >= 0);
    // The k ≥ 0 keeping one axis inside its interval (`None`: never).
    let ks = |from: i32, step: i32, lo: i32, hi: i32| -> Option<(i32, i32)> {
        if step == 0 {
            return (lo..=hi).contains(&from).then_some((0, i32::MAX));
        }
        let first = (lo - from + step - 1).div_euclid(step);
        let last = (hi - from).div_euclid(step);
        (first.max(0) <= last).then_some((first.max(0), last))
    };
    match (ks(p.x, step.0, lo.x, hi.x), ks(p.z, step.1, lo.z, hi.z)) {
        (Some((a, b)), Some((c, d))) => a.max(c) <= b.min(d),
        _ => false,
    }
}

/// Offsets in a 15×15 square, nearest (Manhattan) first like Java's
/// `BlockPos.withinManhattan`.
fn manhattan_order() -> &'static [IVec3] {
    static ORDER: std::sync::OnceLock<Vec<IVec3>> = std::sync::OnceLock::new();
    ORDER.get_or_init(|| {
        let mut cells: Vec<IVec3> = (-7..=7).flat_map(|dx| (-7..=7).map(move |dz| IVec3::new(dx, 0, dz))).collect();
        cells.sort_by_key(|d| d.x.abs() + d.z.abs());
        cells
    })
}

/// `DeltaFeature`: a shallow lava pool set into the floor, rimmed with
/// magma (size 3..=7, rim 0..=2). `origin` is the floor's top block.
pub fn delta(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    let rimmed = rng.unit() < 0.9;
    let rx = if rimmed { between(rng, 0, 2) } else { 0 };
    let rz = if rimmed { between(rng, 0, 2) } else { 0 };
    let rim = rimmed && rx != 0 && rz != 0;
    let (sx, sz) = (between(rng, 3, 7), between(rng, 3, 7));
    let reach = sx.max(sz);
    let mut placed = false;
    let bounds = level.write_bounds();
    if bounds.is_some_and(|(lo, hi)| !(lo.y..=hi.y).contains(&origin.y)) {
        return false;
    }
    for &d in manhattan_order() {
        if d.x.abs() + d.z.abs() > reach {
            break;
        }
        if d.x.abs() > sx || d.z.abs() > sz {
            continue;
        }
        let p = origin + d;
        let q = p + IVec3::new(rx, 0, rz);
        // A cell writes `p` (rim) and `q` (pool), and its outcome depends on
        // whether an earlier cell already wrote `p`: the chain p, p + r, p + 2r…
        // must reach the kept blocks for the cell to matter.
        if bounds.is_some_and(|(lo, hi)| !chain_reaches(p, (rx, rz), lo, hi)) {
            continue;
        }
        if delta_clear(level, p) {
            if rim {
                level.place(p, Block::MAGMA);
                placed = true;
            }
            if delta_clear(level, q) {
                level.place(q, Block::LAVA);
                placed = true;
            }
        }
    }
    placed
}

/// Java's `BasaltColumnsFeature.CANNOT_PLACE_ON`.
fn column_blocked(b: Block) -> bool {
    b.is_lava() || matches!(b, Block::MAGMA | Block::SOUL_SAND) || delta_protected(b)
}

fn air_or_lava_ocean(level: &impl Level, p: IVec3) -> bool {
    let b = level.block(p);
    b == Block::AIR || (b.is_lava() && p.y <= SEA_LEVEL)
}

fn column_base(level: &impl Level, p: IVec3) -> bool {
    air_or_lava_ocean(level, p) && {
        let below = level.block(p - IVec3::Y);
        below != Block::AIR && !column_blocked(below)
    }
}

/// `BasaltColumnsFeature`: small (`large == false`: reach 1, height 1..=4)
/// or large (reach 2..=3, height 5..=10) clusters of basalt columns rising
/// from the floor or out of the lava sea.
pub fn basalt_columns(level: &mut impl Level, rng: &mut Rng, origin: IVec3, large: bool) -> bool {
    if !column_base(level, origin) {
        return false;
    }
    let height = if large { between(rng, 5, 10) } else { between(rng, 1, 4) };
    let narrow = float(rng) < 0.9;
    let spread = height.min(if narrow { 5 } else { 8 });
    let tries = if narrow { 50 } else { 15 };
    let mut placed = false;
    for _ in 0..tries {
        let p = origin + IVec3::new(between(rng, -spread, spread), 0, between(rng, -spread, spread));
        let left = height - (p.x - origin.x).abs() - (p.z - origin.z).abs();
        if left >= 0 {
            let reach = if large { between(rng, 2, 3) } else { 1 };
            placed |= column(level, p, left, reach);
        }
    }
    placed
}

fn column(level: &mut impl Level, center: IVec3, height: i32, reach: i32) -> bool {
    let mut placed = false;
    let bounds = level.write_bounds();
    for dx in -reach..=reach {
        for dz in -reach..=reach {
            let p = center + IVec3::new(dx, 0, dz);
            // Each cell only reads and writes its own vertical column.
            if bounds.is_some_and(|(lo, hi)| !(lo.x..=hi.x).contains(&p.x) || !(lo.z..=hi.z).contains(&p.z)) {
                continue;
            }
            let distance = dx.abs() + dz.abs();
            let start = if air_or_lava_ocean(level, p) {
                // Down to the surface.
                let mut q = p;
                let mut left = distance;
                let mut found = None;
                while q.y > 1 && left > 0 {
                    left -= 1;
                    if column_base(level, q) {
                        found = Some(q);
                        break;
                    }
                    q -= IVec3::Y;
                }
                found
            } else {
                // Up out of the ground.
                let mut q = p;
                let mut left = distance;
                let mut found = None;
                while q.y < BUILD_HEIGHT && left > 0 {
                    left -= 1;
                    let b = level.block(q);
                    if column_blocked(b) {
                        break;
                    }
                    if b == Block::AIR {
                        found = Some(q);
                        break;
                    }
                    q += IVec3::Y;
                }
                found
            };
            let Some(mut q) = start else { continue };
            let mut left = height - distance / 2;
            while left >= 0 {
                if air_or_lava_ocean(level, q) {
                    level.place(q, Block::BASALT);
                    placed = true;
                } else if level.block(q) != Block::BASALT {
                    break;
                }
                q += IVec3::Y;
                left -= 1;
            }
        }
    }
    placed
}

/// `RandomPatchFeature` of soul fire on soul soil (64 tries, spread 7 × 3).
pub fn soul_fire_patch(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    let mut placed = false;
    for _ in 0..64 {
        let p = origin + IVec3::new(int(rng, 8) - int(rng, 8), int(rng, 4) - int(rng, 4), int(rng, 8) - int(rng, 8));
        if level.empty(p) && level.block(p - IVec3::Y) == nb::SOUL_SOIL {
            level.place(p, nb::SOUL_FIRE);
            placed = true;
        }
    }
    placed
}

/// A Nether fossil: a bone-block spine and ribcage half sunk into the
/// floor. Java builds these from 14 hand-made templates; this draws a
/// procedural skeleton of similar size instead. `origin` is the air block
/// above the floor.
pub fn fossil(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    let below = level.block(origin - IVec3::Y);
    if !level.empty(origin) || !(below.is_opaque() || below == Block::SOUL_SAND) {
        return false;
    }
    let along_x = int(rng, 2) == 0;
    let length = between(rng, 5, 12);
    let sink = between(rng, 0, 2);
    let rib_height = between(rng, 2, 4);
    let at = |i: i32, side: i32, up: i32| {
        let o = if along_x { IVec3::new(i, up, side) } else { IVec3::new(side, up, i) };
        origin + o - IVec3::Y * sink
    };
    let mut writes = Vec::new();
    for i in 0..length {
        writes.push(at(i, 0, rib_height));
        if i % 2 == 1 && i + 1 < length {
            let half = 1 + (i < length - 3) as i32;
            for side in [-1, 1] {
                for w in 1..=half {
                    writes.push(at(i, side * w, rib_height));
                }
                for up in 0..rib_height {
                    writes.push(at(i, side * (half + 1), up));
                }
            }
        }
    }
    // A skull at one end of the spine.
    for d in [IVec3::ZERO, IVec3::Y] {
        writes.push(at(-1, 0, rib_height) + d);
    }
    let mut placed = false;
    for p in writes {
        let b = level.block(p);
        if p.y > 0 && p.y < GEN_DEPTH - 1 && (b == Block::AIR || b == Block::SOUL_SAND || b == nb::SOUL_SOIL) {
            level.place(p, nb::BONE_BLOCK);
            placed = true;
        }
    }
    placed
}

// ---------------------------------------------------------------------------
// Java 26.3's data-driven Nether features (generation 6, v1.0 Track 8).
// ---------------------------------------------------------------------------

use super::nether_placed::{IntP, Simple};

/// `glowstone_extra`: attempts and the x/z and y offsets of each try.
pub const GLOWSTONE_SPREAD: (i32, IntP, IntP) = (1500, IntP::Trapezoid(-7, 7, 0), IntP::Uniform(-11, 0));
/// `weeping_vines`: the wart spread's attempts and offsets...
pub const WART_SPREAD: (i32, IntP, IntP) = (200, IntP::Trapezoid(-5, 5, 0), IntP::Trapezoid(-4, 1, 3));
/// ...then the columns' count and offsets...
pub const VINE_PLACEMENT: (i32, IntP, IntP) = (100, IntP::Trapezoid(-7, 7, 0), IntP::Trapezoid(-4, 1, 3));
/// ...and each column's plant layer (weighted heights); twisting vines'
/// likewise. Heads are aged [`VINE_HEAD_AGE`].
pub const WEEPING_LENGTHS: [(IntP, i32); 3] =
    [(IntP::Uniform(1, 8), 10), (IntP::Uniform(2, 17), 2), (IntP::Const(1), 3)];
pub const TWISTING_LENGTHS: [(IntP, i32); 3] =
    [(IntP::Uniform(0, 7), 10), (IntP::Uniform(1, 15), 2), (IntP::Const(0), 3)];
pub const VINE_HEAD_AGE: (i32, i32) = (17, 25);

/// `SpringFeature.place`: lava set into rock with exactly `rock` of its
/// four sides and floor in `valid` and `hole` of them open.
pub fn spring(
    level: &mut impl Level,
    origin: IVec3,
    valid: &[Block],
    requires_below: bool,
    rock: u8,
    hole: u8,
) -> bool {
    let is_valid = |b: Block| valid.contains(&b);
    if !is_valid(level.block(origin + IVec3::Y)) || (requires_below && !is_valid(level.block(origin - IVec3::Y))) {
        return false;
    }
    let here = level.block(origin);
    if here != Block::AIR && !is_valid(here) {
        return false;
    }
    let around = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_Y];
    let rocks = around.iter().filter(|&&d| is_valid(level.block(origin + d))).count();
    let holes = around.iter().filter(|&&d| level.empty(origin + d)).count();
    if rocks == rock as usize && holes == hole as usize {
        level.place(origin, Block::LAVA);
        return true;
    }
    false
}

/// `SimpleBlockFeature` of a Nether plant or fire, if it can stay there
/// (`canSurvive`; mushrooms ignore light, which worldgen hasn't lit yet).
pub fn simple(level: &mut impl Level, rng: &mut Rng, p: IVec3, what: Simple) -> bool {
    let below = level.block(p - IVec3::Y);
    let b = match what {
        Simple::Fire => {
            if !below.is_opaque() {
                return false;
            }
            Block::fire(0)
        }
        Simple::SoulFire => {
            if !matches!(below, Block::SOUL_SAND | nb::SOUL_SOIL) {
                return false;
            }
            nb::SOUL_FIRE
        }
        Simple::BrownMushroom | Simple::RedMushroom => {
            // `MushroomBlock.canSurvive`: the override tag, or a solid block.
            if !(below.is_opaque() || nb::is_nylium(below)) {
                return false;
            }
            if what == Simple::BrownMushroom { Block::BROWN_MUSHROOM } else { Block::RED_MUSHROOM }
        }
        Simple::CrimsonRoots => nb::CRIMSON_ROOTS,
        Simple::NetherSprouts => Vegetation::NetherSprouts.pick(rng),
        Simple::CrimsonVegetation => Vegetation::Crimson.pick(rng),
        Simple::WarpedVegetation => Vegetation::Warped.pick(rng),
    };
    let plant = !matches!(what, Simple::Fire | Simple::SoulFire | Simple::BrownMushroom | Simple::RedMushroom);
    if plant && !b.can_stay_on(below) {
        return false;
    }
    level.place(p, b);
    true
}

/// `RandomNeighborSpreadFeature` of glowstone (`glowstone_extra`): the
/// origin, then 1500 tries at offsets (trapezoid -7..7, uniform -11..0,
/// trapezoid -7..7) that touch exactly one glowstone. Only this feature's
/// own blocks are glowstone, so neighbours are kept in a small grid.
pub fn glowstone(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    const W: i32 = 17;
    let index = |d: IVec3| ((d.y + 12) * W * W + (d.z + 8) * W + (d.x + 8)) as usize;
    let mut grid = vec![false; (W * W * 14) as usize];
    level.place(origin, Block::GLOWSTONE);
    grid[index(IVec3::ZERO)] = true;
    let (attempts, xz, y) = GLOWSTONE_SPREAD;
    for _ in 0..attempts {
        let d = IVec3::new(xz.sample(rng), y.sample(rng), xz.sample(rng));
        if grid[index(d)] {
            continue;
        }
        let mut touching = 0;
        for o in DIRECTIONS {
            if grid[index(d + o)] {
                touching += 1;
                if touching > 1 {
                    break;
                }
            }
        }
        if touching == 1 && level.empty(origin + d) {
            grid[index(d)] = true;
            level.place(origin + d, Block::GLOWSTONE);
        }
    }
    true
}

/// `OreFeature.place` replacing netherrack with `state` (the Nether's ores
/// never discard on air). The ocean-floor heightmap check always passes
/// under the Nether's bedrock roof. Spheres outside the write bounds are
/// skipped: the shape draws all its randomness first.
pub fn ore(level: &mut impl Level, rng: &mut Rng, origin: IVec3, size: i32, state: Block) -> bool {
    use std::f32::consts::PI;
    let dir = float(rng) * PI;
    let spread = size as f32 / 8.0;
    let max_radius = ((size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil() as i32;
    let (o, s) = (origin.as_dvec3(), f64::from(spread));
    let (x0, x1) = (o.x + f64::from(dir.sin()) * s, o.x - f64::from(dir.sin()) * s);
    let (z0, z1) = (o.z + f64::from(dir.cos()) * s, o.z - f64::from(dir.cos()) * s);
    let y0 = f64::from(origin.y + int(rng, 3) - 2);
    let y1 = f64::from(origin.y + int(rng, 3) - 2);
    let reach = spread.ceil() as i32 + max_radius;
    let start = origin - IVec3::new(reach, 2 + max_radius, reach);
    let mut data = Vec::with_capacity(size as usize);
    for i in 0..size {
        let step = i as f32 / size as f32;
        let lerp = |a: f64, b: f64| a + f64::from(step) * (b - a);
        let ss = rng.unit() * f64::from(size) / 16.0;
        let r = ((f64::from((PI * step).sin()) + 1.0) * ss + 1.0) / 2.0;
        data.push([lerp(x0, x1), lerp(y0, y1), lerp(z0, z1), r]);
    }
    for i in 0..data.len().saturating_sub(1) {
        if data[i][3] <= 0.0 {
            continue;
        }
        for j in i + 1..data.len() {
            if data[j][3] <= 0.0 {
                continue;
            }
            let (a, b) = (data[i], data[j]);
            let dr = a[3] - b[3];
            if dr * dr > (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2) {
                if dr > 0.0 {
                    data[j][3] = -1.0;
                } else {
                    data[i][3] = -1.0;
                }
            }
        }
    }
    let bounds = level.write_bounds();
    let mut placed = false;
    for [xx, yy, zz, r] in data {
        if r < 0.0 {
            continue;
        }
        let lo = IVec3::new((xx - r).floor() as i32, (yy - r).floor() as i32, (zz - r).floor() as i32).max(start);
        let hi = IVec3::new((xx + r).floor() as i32, (yy + r).floor() as i32, (zz + r).floor() as i32).max(lo);
        let (lo, hi) = match bounds {
            Some((a, b)) => (lo.max(a), hi.min(b)),
            None => (lo, hi),
        };
        for x in lo.x..=hi.x {
            let dx = (f64::from(x) + 0.5 - xx) / r;
            if dx * dx >= 1.0 {
                continue;
            }
            for y in lo.y..=hi.y {
                let dy = (f64::from(y) + 0.5 - yy) / r;
                if dx * dx + dy * dy >= 1.0 || !(0..BUILD_HEIGHT).contains(&y) {
                    continue;
                }
                for z in lo.z..=hi.z {
                    let dz = (f64::from(z) + 0.5 - zz) / r;
                    let p = IVec3::new(x, y, z);
                    if dx * dx + dy * dy + dz * dz < 1.0 && level.block(p) == Block::NETHERRACK {
                        level.place(p, state);
                        placed = true;
                    }
                }
            }
        }
    }
    placed
}

/// `ReplaceBlobsFeature` (`netherrack_replace_blobs`): from the first
/// netherrack at or below the origin, every netherrack within a box of
/// three sampled radii and the largest radius's Manhattan distance.
pub fn blobs(level: &mut impl Level, rng: &mut Rng, origin: IVec3, state: Block, radius: (i32, i32)) -> bool {
    let mut c = IVec3::new(origin.x, origin.y.clamp(1, BUILD_HEIGHT - 1), origin.z);
    loop {
        if c.y <= 1 {
            return false;
        }
        if level.block(c) == Block::NETHERRACK {
            break;
        }
        c.y -= 1;
    }
    let r = IntP::Uniform(radius.0, radius.1);
    let (rx, ry, rz) = (r.sample(rng), r.sample(rng), r.sample(rng));
    let max = rx.max(ry).max(rz);
    let (mut lo, mut hi) = (c - IVec3::new(rx, ry, rz), c + IVec3::new(rx, ry, rz));
    if let Some((a, b)) = level.write_bounds() {
        (lo, hi) = (lo.max(a), hi.min(b));
    }
    let mut placed = false;
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let p = IVec3::new(x, y, z);
                let d = (p - c).abs();
                if d.x + d.y + d.z <= max && level.block(p) == Block::NETHERRACK {
                    level.place(p, state);
                    placed = true;
                }
            }
        }
    }
    placed
}

/// A `weighted_list` int provider of uniform or constant entries.
fn weighted(rng: &mut Rng, entries: &[(IntP, i32)]) -> i32 {
    let mut r = int(rng, entries.iter().map(|e| e.1).sum());
    for &(p, w) in entries {
        if r < w {
            return p.sample(rng);
        }
        r -= w;
    }
    unreachable!("weights cover the roll")
}

/// `BlockColumnFeature` of a vine: a plant layer of `plants` blocks and a
/// one-block head aged 17..=25, growing along `dir`, cut to the run of air
/// past its origin, plant layer first (`prioritize_tip`).
fn vine_column(level: &mut impl Level, rng: &mut Rng, origin: IVec3, dir: IVec3, plants: i32, vine: Vine) {
    let total = plants + 1;
    let mut heights = [plants, 1];
    let mut next = origin + dir;
    for y in 0..total {
        if !level.empty(next) {
            let mut remove = total - y;
            for h in &mut heights {
                let cut = (*h).min(remove);
                *h -= cut;
                remove -= cut;
            }
            break;
        }
        next += dir;
    }
    let plant = if vine == Vine::Weeping { nb::WEEPING_VINES_PLANT } else { nb::TWISTING_VINES_PLANT };
    let mut p = origin;
    for _ in 0..heights[0] {
        level.place(p, plant);
        p += dir;
    }
    for _ in 0..heights[1] {
        level.place(p, vine.head(between(rng, VINE_HEAD_AGE.0, VINE_HEAD_AGE.1) as u8));
        p += dir;
    }
}

/// Java 26.3's `weeping_vines`: an overlay of a nether wart spread on the
/// ceiling (200 tries, trapezoid -5..5 across and -4..1 with plateau 3 up)
/// and a hundred vine columns hung where air meets netherrack or wart.
pub fn weeping_vines_26(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    let (attempts, xz, y) = WART_SPREAD;
    level.place(origin, nb::NETHER_WART_BLOCK);
    for _ in 0..attempts {
        let p = origin + IVec3::new(xz.sample(rng), y.sample(rng), xz.sample(rng));
        if !level.empty(p) {
            continue;
        }
        let mut touching = 0;
        for d in DIRECTIONS {
            if roof(level.block(p + d)) {
                touching += 1;
                if touching > 1 {
                    break;
                }
            }
        }
        if touching == 1 {
            level.place(p, nb::NETHER_WART_BLOCK);
        }
    }
    let (count, xz, y) = VINE_PLACEMENT;
    for _ in 0..count {
        let p = origin + IVec3::new(xz.sample(rng), y.sample(rng), xz.sample(rng));
        if level.empty(p) && roof(level.block(p + IVec3::Y)) {
            let plants = weighted(rng, &WEEPING_LENGTHS);
            vine_column(level, rng, p, IVec3::NEG_Y, plants, Vine::Weeping);
        }
    }
    true
}

/// Java 26.3's `twisting_vines` feature: one column up from `origin`.
pub fn twisting_vines_26(level: &mut impl Level, rng: &mut Rng, origin: IVec3) -> bool {
    let plants = weighted(rng, &TWISTING_LENGTHS);
    vine_column(level, rng, origin, IVec3::Y, plants, Vine::Twisting);
    true
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rustc_hash::FxHashMap;

    /// A flat test world: solid below `floor`, air above, optional ceiling.
    pub(crate) struct Flat {
        pub blocks: FxHashMap<IVec3, Block>,
        pub floor: i32,
        pub ceiling: i32,
        pub ground: Block,
    }

    impl Flat {
        pub(crate) fn new(floor: i32, ceiling: i32, ground: Block) -> Self {
            Self { blocks: FxHashMap::default(), floor, ceiling, ground }
        }
    }

    impl Level for Flat {
        fn block(&self, p: IVec3) -> Block {
            if let Some(&b) = self.blocks.get(&p) {
                return b;
            }
            match p.y {
                y if y == self.floor - 1 => self.ground,
                y if y < self.floor - 1 || y >= self.ceiling => Block::NETHERRACK,
                _ => Block::AIR,
            }
        }
        fn place(&mut self, p: IVec3, b: Block) {
            self.blocks.insert(p, b);
        }
    }

    fn count(level: &Flat, f: impl Fn(Block) -> bool) -> usize {
        level.blocks.values().filter(|&&b| f(b)).count()
    }

    #[test]
    fn huge_fungi_need_their_nylium_and_stay_within_java_bounds() {
        for wood in NetherWood::ALL {
            let mut grown = 0;
            for seed in 0..200 {
                let mut level = Flat::new(40, 200, wood.nylium());
                let origin = IVec3::new(0, 40, 0);
                if !huge_fungus(&mut level, &mut Rng(seed), origin, wood, false) {
                    continue;
                }
                grown += 1;
                let stems = count(&level, |b| b == wood.stem());
                assert!(stems >= 4, "seed {seed}: a stem at least four tall");
                for (&p, &b) in &level.blocks {
                    let d = p - origin;
                    // Stem height <= 26, hat radius <= 4, vines hang <= 10 under the hat.
                    assert!(d.x.abs() <= 4 && d.z.abs() <= 4, "{b:?} at {d}");
                    assert!((-11..=26).contains(&d.y), "{b:?} at {d}");
                    assert!(
                        b == Block::AIR
                            || b == wood.stem()
                            || b == wood.wart()
                            || b == nb::SHROOMLIGHT
                            || Vine::of(b).is_some_and(|(v, _)| v == Vine::Weeping && wood == NetherWood::Crimson),
                        "{} in a {wood:?} fungus",
                        b.name()
                    );
                }
                assert!(count(&level, |b| b == wood.wart()) > 10, "a hat");
            }
            assert!(grown > 150, "{wood:?} fungi grew {grown} times");
            let mut wrong = Flat::new(40, 200, Block::NETHERRACK);
            assert!(!huge_fungus(&mut wrong, &mut Rng(1), IVec3::new(0, 40, 0), wood, true));
        }
    }

    #[test]
    fn planted_fungi_are_never_giant_and_respect_the_build_limit_only_in_worldgen() {
        let mut level = Flat::new(124, 400, nb::CRIMSON_NYLIUM);
        let origin = IVec3::new(0, 124, 0);
        assert!(!huge_fungus(&mut level, &mut Rng(3), origin, NetherWood::Crimson, false), "too close to the roof");
        for seed in 0..100 {
            let mut level = Flat::new(40, 400, nb::WARPED_NYLIUM);
            assert!(huge_fungus(&mut level, &mut Rng(seed), IVec3::new(0, 40, 0), NetherWood::Warped, true));
            let stem_columns: std::collections::HashSet<_> =
                level.blocks.iter().filter(|(_, b)| **b == nb::WARPED_STEM).map(|(p, _)| (p.x, p.z)).collect();
            assert_eq!(stem_columns.len(), 1, "planted fungi have a single stem");
        }
    }

    #[test]
    fn vines_grow_into_air_only_and_end_in_an_aged_head() {
        let mut level = Flat::new(40, 60, Block::NETHERRACK);
        weeping_column(&mut level, &mut Rng(5), IVec3::new(0, 59, 0), 8, 17, 25);
        let heads: Vec<_> = level.blocks.iter().filter(|(_, b)| matches!(Vine::of(**b), Some((_, Some(_))))).collect();
        assert_eq!(heads.len(), 1);
        let (head, b) = heads[0];
        assert_eq!(head.y, 51);
        assert!(matches!(Vine::of(*b), Some((Vine::Weeping, Some(17..=25)))));
        assert_eq!(count(&level, |b| b == nb::WEEPING_VINES_PLANT), 8);
        // Stops at the floor.
        let mut short = Flat::new(57, 60, Block::NETHERRACK);
        weeping_column(&mut short, &mut Rng(5), IVec3::new(0, 59, 0), 8, 17, 25);
        assert_eq!(short.blocks.len(), 3);
        assert!(short.blocks[&IVec3::new(0, 57, 0)] != nb::WEEPING_VINES_PLANT);
        let mut up = Flat::new(40, 44, Block::NETHERRACK);
        twisting_column(&mut up, &mut Rng(6), IVec3::new(0, 40, 0), 9, 17, 25);
        assert_eq!(up.blocks.len(), 4, "stops under the ceiling");
        assert!(matches!(Vine::of(up.blocks[&IVec3::new(0, 43, 0)]), Some((Vine::Twisting, Some(_)))));
    }

    #[test]
    fn ceiling_vines_need_a_netherrack_roof_and_spread_wart() {
        let mut level = Flat::new(40, 60, Block::NETHERRACK);
        assert!(!weeping_vines(&mut level, &mut Rng(1), IVec3::new(0, 50, 0)), "not under a ceiling");
        assert!(weeping_vines(&mut level, &mut Rng(1), IVec3::new(0, 59, 0)));
        assert!(count(&level, |b| b == nb::NETHER_WART_BLOCK) > 3);
        assert!(count(&level, |b| Vine::of(b).is_some()) > 5);
        let mut warped = Flat::new(40, 60, nb::WARPED_NYLIUM);
        assert!(twisting_vines(&mut warped, &mut Rng(2), IVec3::new(0, 40, 0), 8, 4, 8));
        assert!(count(&warped, |b| matches!(Vine::of(b), Some((Vine::Twisting, _)))) > 5);
    }

    #[test]
    fn twisting_vines_find_ground_from_air_offsets_and_never_dig_out_of_rock() {
        let origin = IVec3::new(0, 40, 0);
        // With no vertical spread all attempts begin in air at floor height.
        // Searching up skips every attempt; searching down finds the floor.
        let mut floor = Flat::new(40, 100, nb::WARPED_NYLIUM);
        assert!(twisting_vines(&mut floor, &mut Rng(2), origin, 3, 0, 2));
        assert!(count(&floor, |b| Vine::of(b).is_some()) > 0);
        assert!(floor.blocks.keys().all(|p| p.y >= 40));
        // A neighbouring ledge above the start is solid at every sampled y.
        // Java cannot grow through it to the air on its upper surface.
        let mut ledge = Flat::new(40, 100, nb::WARPED_NYLIUM);
        for x in -8..=8 {
            for z in -8..=8 {
                if x == 0 && z == 0 {
                    continue;
                }
                for y in 40..=48 {
                    ledge.blocks.insert(IVec3::new(x, y, z), Block::NETHERRACK);
                }
            }
        }
        assert!(twisting_vines(&mut ledge, &mut Rng(2), origin, 8, 4, 8));
        assert!(!ledge.blocks.iter().any(|(p, b)| (p.x != 0 || p.z != 0) && Vine::of(*b).is_some()));
    }

    #[test]
    fn vegetation_only_grows_from_nylium() {
        let mut level = Flat::new(40, 60, nb::CRIMSON_NYLIUM);
        assert!(vegetation(&mut level, &mut Rng(9), IVec3::new(0, 40, 0), Vegetation::Crimson, 8, 4));
        assert!(count(&level, |b| b == nb::CRIMSON_ROOTS) > count(&level, |b| b == nb::CRIMSON_FUNGUS));
        for p in level.blocks.keys() {
            assert_eq!(p.y, 40, "plants sit on the floor");
        }
        let mut bare = Flat::new(40, 60, Block::NETHERRACK);
        assert!(!vegetation(&mut bare, &mut Rng(9), IVec3::new(0, 40, 0), Vegetation::Warped, 8, 4));
    }

    #[test]
    fn chain_reaches_matches_stepping_one_block_at_a_time() {
        let (lo, hi) = (IVec3::new(0, 0, 0), IVec3::new(31, 31, 31));
        for px in -40..72 {
            for pz in (-40..72).step_by(3) {
                for step in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 1), (1, 2), (2, 2), (2, 0), (0, 2)] {
                    let by_stepping = (0..=60).any(|k| {
                        let (x, z) = (px + k * step.0, pz + k * step.1);
                        (0..=31).contains(&x) && (0..=31).contains(&z)
                    });
                    assert_eq!(chain_reaches(IVec3::new(px, 5, pz), step, lo, hi), by_stepping, "{px},{pz} {step:?}");
                }
            }
        }
    }

    #[test]
    fn pillars_deltas_and_columns() {
        let mut level = Flat::new(40, 60, Block::SOUL_SAND);
        assert!(basalt_pillar(&mut level, &mut Rng(3), IVec3::new(0, 59, 0)));
        for y in 40..60 {
            assert_eq!(level.block(IVec3::new(0, y, 0)), Block::BASALT, "a column from roof to floor at {y}");
        }
        let mut floor = Flat::new(40, 60, Block::BASALT);
        assert!((0..20).any(|s| delta(&mut floor, &mut Rng(s), IVec3::new(0, 39, 0))));
        assert!(count(&floor, |b| b == Block::LAVA) > 4);
        assert!(floor.blocks.keys().all(|p| p.y == 39), "pools sit in the floor's top layer");
        let mut cols = Flat::new(40, 100, Block::BASALT);
        assert!(basalt_columns(&mut cols, &mut Rng(4), IVec3::new(0, 40, 0), true));
        let top = cols.blocks.keys().map(|p| p.y).max().unwrap();
        assert!((44..=51).contains(&top), "large columns rise 5..=10: {top}");
        let mut soul = Flat::new(40, 60, nb::SOUL_SOIL);
        assert!(soul_fire_patch(&mut soul, &mut Rng(8), IVec3::new(0, 40, 0)));
        assert!(soul.blocks.iter().all(|(p, b)| *b == nb::SOUL_FIRE && p.y == 40));
        let mut bones = Flat::new(40, 60, nb::SOUL_SOIL);
        assert!(fossil(&mut bones, &mut Rng(2), IVec3::new(0, 40, 0)));
        assert!(count(&bones, |b| b == nb::BONE_BLOCK) > 8);
    }
}
