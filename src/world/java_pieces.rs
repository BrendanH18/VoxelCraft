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

/// Generation 5 layouts against the vectors the 26.3 jar's own piece
/// generators printed (`tests/fixtures/java/26.3/classic_structures.json`,
/// from `tools/extract_java_classic_structure_fixtures.py`): every piece's
/// kind, facing and box, in Java's order.
#[cfg(test)]
mod layout_tests {
    use glam::{IVec2, IVec3};
    use serde_json::Value;

    use super::super::block::Facing;
    use super::super::structure::Bounds;

    /// Plains everywhere and the fixture's mesa surface.
    struct Surface(i32);

    impl super::super::mineshaft::ShaftTerrain for Surface {
        fn biome(&self, _: IVec3) -> super::super::biome::Biome {
            super::super::biome::Biome::Plains
        }

        fn surface(&self, _: i32, _: i32) -> i32 {
            self.0
        }
    }

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/classic_structures.json")).unwrap()
    }

    fn facing(name: &str) -> Option<Facing> {
        match name {
            "north" => Some(Facing::North),
            "south" => Some(Facing::South),
            "west" => Some(Facing::West),
            "east" => Some(Facing::East),
            _ => None,
        }
    }

    /// Java's piece class as VoxelCraft's kind name.
    fn kind(structure: &str, class: &str) -> &'static str {
        match (structure, class) {
            ("fortress", "StartPiece" | "BridgeCrossing") => "BridgeCrossing",
            ("fortress", "BridgeEndFiller") => "BridgeEnd",
            ("fortress", "BridgeStraight") => "BridgeStraight",
            ("fortress", "RoomCrossing") => "RoomCrossing",
            ("fortress", "StairsRoom") => "StairsRoom",
            ("fortress", "MonsterThrone") => "Throne",
            ("fortress", "CastleEntrance") => "Entrance",
            ("fortress", "CastleSmallCorridorPiece") => "Corridor",
            ("fortress", "CastleSmallCorridorCrossingPiece") => "CorridorCrossing",
            ("fortress", "CastleSmallCorridorRightTurnPiece") => "RightTurn",
            ("fortress", "CastleSmallCorridorLeftTurnPiece") => "LeftTurn",
            ("fortress", "CastleCorridorStairsPiece") => "CorridorStairs",
            ("fortress", "CastleCorridorTBalconyPiece") => "Balcony",
            ("fortress", "CastleStalkRoom") => "StalkRoom",
            ("stronghold", "StartPiece" | "StairsDown") => "StairsDown",
            ("stronghold", c) => match c {
                "Straight" => "Straight",
                "PrisonHall" => "PrisonHall",
                "LeftTurn" => "LeftTurn",
                "RightTurn" => "RightTurn",
                "RoomCrossing" => "RoomCrossing",
                "StraightStairsDown" => "StraightStairsDown",
                "FiveCrossing" => "FiveCrossing",
                "ChestCorridor" => "ChestCorridor",
                "Library" => "Library",
                "PortalRoom" => "PortalRoom",
                "FillerCorridor" => "FillerCorridor",
                _ => panic!("unknown stronghold piece {c}"),
            },
            (_, "MineShaftRoom") => "Room",
            (_, "MineShaftCorridor") => "Corridor",
            (_, "MineShaftCrossing") => "Crossing",
            (_, "MineShaftStairs") => "Stairs",
            _ => panic!("unknown {structure} piece {class}"),
        }
    }

    #[test]
    fn generation_5_layouts_match_javas_piece_generators() {
        let java = fixture();
        let mesa_surface = java["mesa_surface"].as_i64().unwrap() as i32;
        for layout in java["layouts"].as_array().unwrap() {
            let structure = layout["structure"].as_str().unwrap();
            let seed = layout["seed"].as_i64().unwrap() as u64;
            let chunk =
                IVec2::new(layout["chunk"][0].as_i64().unwrap() as i32, layout["chunk"][1].as_i64().unwrap() as i32);
            let ours: Vec<(String, Facing, Bounds)> = match structure {
                "fortress" => super::super::fortress::Fortress::generate_java(seed, chunk)
                    .pieces
                    .iter()
                    .map(|p| (format!("{:?}", p.kind), p.facing, p.bounds))
                    .collect(),
                "stronghold" => super::super::stronghold::Stronghold::generate_java(seed, chunk)
                    .pieces
                    .iter()
                    .map(|p| (format!("{:?}", p.kind), p.facing, p.bounds))
                    .collect(),
                _ => {
                    let mesa = structure == "mineshaft_mesa";
                    let (shaft, _) =
                        super::super::mineshaft::Mineshaft::generate_java(seed, chunk, mesa, &Surface(mesa_surface));
                    shaft.pieces.iter().map(|p| (format!("{:?}", p.kind), p.facing, p.bounds)).collect()
                }
            };
            let theirs = layout["pieces"].as_array().unwrap();
            let what = format!("{structure} seed {seed} chunk {chunk}");
            for (i, (ours, theirs)) in ours.iter().zip(theirs).enumerate() {
                let row: Vec<i64> = theirs.as_array().unwrap()[3..].iter().map(|v| v.as_i64().unwrap()).collect();
                let bounds = Bounds {
                    min: IVec3::new(row[0] as i32, row[1] as i32, row[2] as i32),
                    max: IVec3::new(row[3] as i32, row[4] as i32, row[5] as i32),
                };
                let class = theirs[0].as_str().unwrap();
                assert_eq!(ours.0, kind(structure, class), "{what}: piece {i} kind");
                assert_eq!(ours.2, bounds, "{what}: piece {i} ({class}) box");
                if let Some(f) = facing(theirs[1].as_str().unwrap()) {
                    assert_eq!(ours.1, f, "{what}: piece {i} ({class}) facing");
                }
            }
            assert_eq!(ours.len(), theirs.len(), "{what}: piece count");
        }
    }

    #[test]
    fn placements_and_infested_blocks_match_the_jar() {
        let java = fixture();
        let complexes = &java["placements"]["nether_complexes"];
        assert_eq!(complexes["salt"], 30_084_232);
        assert_eq!(complexes["spacing"], 27);
        assert_eq!(complexes["separation"], 4);
        let rings = &java["placements"]["strongholds"];
        assert_eq!(
            (rings["count"].as_i64(), rings["distance"].as_i64(), rings["spread"].as_i64()),
            (Some(128), Some(32), Some(3))
        );
        let shafts = &java["placements"]["mineshafts"];
        assert_eq!(shafts["frequency_reduction_method"], "legacy_type_3");
        assert_eq!(shafts["frequency"].as_f64().unwrap() as f32, 0.004);
        let fortress = &java["spawn_overrides"]["fortress"]["monster"];
        assert_eq!(fortress["bounding_box"], "piece");
        for (name, facts) in java["infested"].as_object().unwrap() {
            let block =
                super::super::block::Block::from_name(&name.replace('_', " ")).unwrap_or_else(|| panic!("{name}"));
            let host = super::super::infested::host(block).unwrap();
            assert_eq!(host.name().replace(' ', "_"), facts["host"].as_str().unwrap(), "{name}");
            assert_eq!(block.hardness(), facts["hardness"].as_f64().unwrap() as f32, "{name}");
            assert_eq!(super::super::infested::RESISTANCE, facts["blast_resistance"].as_f64().unwrap() as f32);
        }
        let ore = &java["ore_infested"];
        assert_eq!(ore["size"], 9);
        let biomes: Vec<&str> = ore["biomes"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
        for b in super::super::biome::Biome::ALL {
            assert_eq!(b.is_mountain(), biomes.contains(&b.name()), "{b:?} and ore_infested");
        }
    }
}
