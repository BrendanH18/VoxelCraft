//! v0.16 Track E: what Java 26.3's piece-by-piece structures share
//! (`StructurePiece`, `StructurePiecesBuilder`, `WorldgenRandom`), for the
//! generation 5 layouts of fortresses, strongholds and mineshafts. Each
//! layout draws from Java's `LegacyRandomSource` seeded by
//! `setLargeFeatureSeed`, in Java's order, so a start chunk lays out the
//! pieces Java's does.

use glam::{IVec2, IVec3};

use super::block::Facing;
use super::structure::Bounds;
use crate::enchant::JavaRandom;

/// `Direction.Plane.HORIZONTAL`'s order: north, east, south, west.
pub const HORIZONTAL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

/// `StructurePiece.getRandomHorizontalDirection`.
pub fn random_horizontal(rng: &mut JavaRandom) -> Facing {
    HORIZONTAL[rng.next_bounded(4) as usize]
}

/// `RandomSource.nextLong`.
pub fn next_long(rng: &mut JavaRandom) -> i64 {
    ((rng.next_int() as i64) << 32).wrapping_add(rng.next_int() as i64)
}

/// `RandomSource.nextDouble`.
pub fn next_double(rng: &mut JavaRandom) -> f64 {
    let hi = (rng.next_int() as u32 >> 6) as i64;
    let lo = (rng.next_int() as u32 >> 5) as i64;
    ((hi << 27) + lo) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// `Mth.nextInt(random, min, max)`: inclusive, `min` when `max <= min`.
pub fn next_int_between(rng: &mut JavaRandom, min: i32, max: i32) -> i32 {
    if min >= max { min } else { rng.next_bounded(max - min + 1) + min }
}

/// `Structure.GenerationContext.makeRandom`: `setLargeFeatureSeed` over a
/// `LegacyRandomSource`.
pub fn large_feature_random(seed: u64, chunk: IVec2) -> JavaRandom {
    super::mansion::large_feature_random(seed, chunk)
}

/// `StructurePiece.makeBoundingBox`.
pub fn make_box(corner: IVec3, facing: Facing, width: i32, height: i32, depth: i32) -> Bounds {
    let size = if matches!(facing, Facing::North | Facing::South) {
        IVec3::new(width, height, depth)
    } else {
        IVec3::new(depth, height, width)
    };
    Bounds { min: corner, max: corner + size - IVec3::ONE }
}

/// `StructurePiece.findCollisionPiece`: whether `b` meets any of `boxes`.
pub fn collides<'a>(mut boxes: impl Iterator<Item = &'a Bounds>, b: &Bounds) -> bool {
    boxes.any(|o| o.intersects(b))
}

/// The doorway of a child grown straight ahead (`generateChildForward`,
/// `generateSmallDoorChildForward`): `x_off` across, `y_off` up.
pub fn forward(b: &Bounds, facing: Facing, x_off: i32, y_off: i32) -> (IVec3, Facing) {
    let y = b.min.y + y_off;
    let p = match facing {
        Facing::North => IVec3::new(b.min.x + x_off, y, b.min.z - 1),
        Facing::South => IVec3::new(b.min.x + x_off, y, b.max.z + 1),
        Facing::West => IVec3::new(b.min.x - 1, y, b.min.z + x_off),
        Facing::East => IVec3::new(b.max.x + 1, y, b.min.z + x_off),
    };
    (p, facing)
}

/// `generateChildLeft`: west of a north or south piece, north of an east
/// or west one, `z_off` along the world axis (not mirrored).
pub fn left(b: &Bounds, facing: Facing, y_off: i32, z_off: i32) -> (IVec3, Facing) {
    let y = b.min.y + y_off;
    match facing {
        Facing::North | Facing::South => (IVec3::new(b.min.x - 1, y, b.min.z + z_off), Facing::West),
        Facing::West | Facing::East => (IVec3::new(b.min.x + z_off, y, b.min.z - 1), Facing::North),
    }
}

/// `generateChildRight`: east of a north or south piece, south of an east
/// or west one.
pub fn right(b: &Bounds, facing: Facing, y_off: i32, z_off: i32) -> (IVec3, Facing) {
    let y = b.min.y + y_off;
    match facing {
        Facing::North | Facing::South => (IVec3::new(b.max.x + 1, y, b.min.z + z_off), Facing::East),
        Facing::West | Facing::East => (IVec3::new(b.min.x + z_off, y, b.max.z + 1), Facing::South),
    }
}

/// `StructurePiecesBuilder.moveBelowSeaLevel`: the shift that puts the
/// layout's top below `sea_level - offset`.
pub fn below_sea_level_shift(bounds: &Bounds, sea_level: i32, min_y: i32, rng: &mut JavaRandom, offset: i32) -> i32 {
    let max_y = sea_level - offset;
    let mut y1 = bounds.max.y - bounds.min.y + 1 + min_y + 1;
    if y1 < max_y {
        y1 += rng.next_bounded(max_y - y1);
    }
    y1 - bounds.max.y
}

/// `StructurePiecesBuilder.moveInsideHeights`: the shift that puts the
/// layout's bottom between `lowest` and `highest`.
pub fn inside_heights_shift(bounds: &Bounds, rng: &mut JavaRandom, lowest: i32, highest: i32) -> i32 {
    let span = highest - lowest + 1 - (bounds.max.y - bounds.min.y + 1);
    let y0 = if span > 1 { lowest + rng.next_bounded(span) } else { lowest };
    y0 - bounds.min.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_double_and_long_match_java_util_random() {
        // `new java.util.Random(42)`: nextLong() = -5025562857975149833,
        // and a fresh one's nextDouble() = 0.7275636800328681.
        let mut r = JavaRandom::new(42);
        assert_eq!(next_long(&mut r), -5_025_562_857_975_149_833);
        let mut r = JavaRandom::new(42);
        assert_eq!(next_double(&mut r), 0.727_563_680_032_868_1);
    }

    #[test]
    fn children_follow_javas_world_offsets() {
        let b = Bounds { min: IVec3::new(0, 64, 0), max: IVec3::new(4, 70, 4) };
        assert_eq!(forward(&b, Facing::North, 1, 0), (IVec3::new(1, 64, -1), Facing::North));
        assert_eq!(left(&b, Facing::South, 0, 1), (IVec3::new(-1, 64, 1), Facing::West));
        assert_eq!(left(&b, Facing::East, 0, 1), (IVec3::new(1, 64, -1), Facing::North));
        assert_eq!(right(&b, Facing::North, 0, 1), (IVec3::new(5, 64, 1), Facing::East));
        assert_eq!(right(&b, Facing::West, 0, 1), (IVec3::new(1, 64, 5), Facing::South));
        assert_eq!(make_box(IVec3::ZERO, Facing::East, 3, 2, 5).max, IVec3::new(4, 1, 2));
    }
}
