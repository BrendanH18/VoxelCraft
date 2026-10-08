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
        // Up to the first air above the ground.
        loop {
            p += IVec3::Y;
            if p.y >= BUILD_HEIGHT || level.empty(p) {
                break;
            }
        }
        if p.y < BUILD_HEIGHT && !twisting_base_invalid(level, p) {
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
    fn pick(self, rng: &mut Rng) -> Block {
        let table: &[(Block, i32)] = match self {
            Vegetation::Crimson => &[(nb::CRIMSON_ROOTS, 87), (nb::CRIMSON_FUNGUS, 11), (nb::WARPED_FUNGUS, 1)],
            Vegetation::Warped => {
                &[(nb::WARPED_ROOTS, 85), (nb::CRIMSON_ROOTS, 1), (nb::WARPED_FUNGUS, 13), (nb::CRIMSON_FUNGUS, 1)]
            }
            Vegetation::NetherSprouts => return nb::NETHER_SPROUTS,
        };
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
    for &d in manhattan_order() {
        if d.x.abs() + d.z.abs() > reach {
            break;
        }
        if d.x.abs() > sx || d.z.abs() > sz {
            continue;
        }
        let p = origin + d;
        if delta_clear(level, p) {
            if rim {
                level.place(p, Block::MAGMA);
                placed = true;
            }
            let q = p + IVec3::new(rx, 0, rz);
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
    for dx in -reach..=reach {
        for dz in -reach..=reach {
            let p = center + IVec3::new(dx, 0, dz);
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
