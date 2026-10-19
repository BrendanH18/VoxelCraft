//! Java 26.3 `DismountHelper.findSafeDismountLocation` for the player and
//! `AbstractBedBlock.findStandUpPosition`: where a player stands up from a bed,
//! both when waking (`LivingEntity.stopSleeping`) and when respawning at it
//! (`ServerPlayer.findRespawnAndUseSpawnBlock`).
//!
//! Known differences: unloaded cells count as solid, as in the rest of
//! `physics`; the world border is not modelled; lit campfires, wither roses,
//! powder snow and lava cauldrons (part of `EntityType.isBlockDangerous`) are
//! not blocks in VoxelCraft.

use glam::{DVec3, IVec3};

use super::World;
use super::block::{Block, Facing};
use crate::physics::{self, BlockSource};
use crate::simulation::lighting::{mth_cos, mth_sin};

/// Java's `PLAYER` entity type is 0.6 × 1.8 whatever pose the player is in.
const BODY: physics::Shape = crate::player::SHAPE;

/// `EntityType.isBlockDangerous` for an entity that is neither fire immune
/// nor immune to any block: burning blocks (fire, soul fire, lava, magma),
/// sweet berry bushes and cacti.
pub fn is_block_dangerous(b: Block) -> bool {
    b.is_fire() || b.is_lava() || b == Block::MAGMA || b == Block::CACTUS || b.name() == "sweet berry bush"
}

/// `BlockTags.INVALID_SPAWN_INSIDE`.
fn invalid_spawn_inside(b: Block) -> bool {
    matches!(b, Block::END_PORTAL | Block::END_GATEWAY)
}

/// `DismountHelper.nonClimbableShape(...).max(Y)`: the top of the cell's
/// collision relative to its floor, `None` for an empty shape (open trapdoors
/// and climbable blocks count as empty).
fn non_climbable_top<W: BlockSource + ?Sized>(world: &W, p: IVec3) -> Option<f64> {
    use super::redstone_blocks::{Component, component};
    let b = world.block(p)?;
    if b.is_climbable() || matches!(component(b), Some(Component::Trapdoor { open: true, .. })) {
        return None;
    }
    physics::collision_top(world, p)
}

/// `BlockGetter.getBlockFloorHeight`: how high above the cell's floor a
/// player standing in `p` stands, or `None` when there is nothing to stand on
/// (`-Infinity`) or the floor reaches the next cell (`isBlockFloorValid`).
fn floor_height<W: BlockSource + ?Sized>(world: &W, p: IVec3) -> Option<f64> {
    let height = match non_climbable_top(world, p) {
        Some(top) => top,
        None => non_climbable_top(world, p - IVec3::Y).filter(|&below| below >= 1.0)? - 1.0,
    };
    (height < 1.0).then_some(height)
}

/// `DismountHelper.findSafeDismountLocation(PLAYER, level, pos, checkDangerous)`:
/// where in cell `pos` a standing player fits, if anywhere.
pub fn find_safe_dismount_location<W: BlockSource + ?Sized>(
    world: &W,
    pos: IVec3,
    check_dangerous: bool,
) -> Option<DVec3> {
    let here = world.block(pos)?;
    if check_dangerous && is_block_dangerous(here) {
        return None;
    }
    let height = floor_height(world, pos)?;
    if check_dangerous && height <= 0.0 && is_block_dangerous(world.block(pos - IVec3::Y)?) {
        return None;
    }
    let at = DVec3::new(pos.x as f64 + 0.5, pos.y as f64 + height, pos.z as f64 + 0.5);
    let (min, max) = BODY.aabb(at);
    if physics::box_overlaps_solid(world, min, max) {
        return None;
    }
    if invalid_spawn_inside(here) || invalid_spawn_inside(world.block(pos + IVec3::Y)?) {
        return None;
    }
    Some(at)
}

/// `Direction.isFacingAngle`: whether the horizontal direction is within a
/// quarter turn of the Java `yaw` (degrees, 0 faces south, +Z).
pub fn is_facing_angle(facing: Facing, java_yaw: f32) -> bool {
    let radians = f64::from(java_yaw * 0.017453292);
    let (sin, cos) = (-mth_sin(radians), mth_cos(radians));
    let step = facing.offset();
    step.x as f32 * sin + step.z as f32 * cos > 0.0
}

/// A Java yaw in degrees from VoxelCraft's yaw, which is a quarter turn
/// ahead (VoxelCraft yaw 0 faces +X, Java yaw 0 faces +Z).
pub fn java_yaw_degrees(yaw: f32) -> f32 {
    (yaw - std::f32::consts::FRAC_PI_2).to_degrees()
}

/// `AbstractBedBlock.bedSurroundStandUpOffsets(facing, side)`, as (x, z)
/// steps from the head block, in Java's order.
fn surround_offsets(facing: Facing, side: Facing) -> [(i32, i32); 10] {
    let (f, s) = (facing.offset(), side.offset());
    let at = |sf: i32, ff: i32| (s.x * sf + f.x * ff, s.z * sf + f.z * ff);
    [at(1, 0), at(1, -1), at(1, -2), at(0, -2), at(-1, -2), at(-1, -1), at(-1, 0), at(-1, 1), at(0, 1), at(1, 1)]
}

/// `AbstractBedBlock.bedAboveStandUpOffsets(facing)`: on the head, then on
/// the foot.
fn above_offsets(facing: Facing) -> [(i32, i32); 2] {
    let f = facing.offset();
    [(0, 0), (-f.x, -f.z)]
}

/// `AbstractBedBlock.findStandUpPositionAtOffset`: the first offset (from
/// `pos`) that has a safe dismount location.
fn first_at<W: BlockSource + ?Sized>(
    world: &W,
    pos: IVec3,
    offsets: &[(i32, i32)],
    check_dangerous: bool,
) -> Option<DVec3> {
    offsets.iter().find_map(|&(x, z)| find_safe_dismount_location(world, pos + IVec3::new(x, 0, z), check_dangerous))
}

/// `AbstractBedBlock.findStandUpPosition(PLAYER, level, head, facing, yaw)`.
/// `head` is the bed's head block, `facing` points from the foot to the head
/// and `java_yaw` (degrees) is the player's own angle, which picks the side
/// to try first.
pub fn find_stand_up_position<W: BlockSource + ?Sized>(
    world: &W,
    head: IVec3,
    facing: Facing,
    java_yaw: f32,
) -> Option<DVec3> {
    let clockwise = facing.clockwise();
    let side = if is_facing_angle(clockwise, java_yaw) { clockwise.opposite() } else { clockwise };
    let surround = surround_offsets(facing, side);
    let above = above_offsets(facing);
    let below = head - IVec3::Y;
    if world.block(below).is_some_and(Block::is_bed) {
        // A bunk bed: beside either bed, then on top, safe spots first.
        return [true, false].into_iter().find_map(|check| {
            first_at(world, head, &surround, check)
                .or_else(|| first_at(world, below, &surround, check))
                .or_else(|| first_at(world, head, &above, check))
        });
    }
    let offsets: Vec<(i32, i32)> = surround.into_iter().chain(above).collect();
    first_at(world, head, &offsets, true).or_else(|| first_at(world, head, &offsets, false))
}

/// VoxelCraft yaw that looks from `from` at the bottom centre of the bed's
/// head block, as `RespawnPosAngle.of` and `LivingEntity.stopSleeping` do.
pub fn look_at_bed(from: DVec3, head: IVec3) -> f32 {
    let look = head.as_dvec3() + DVec3::new(0.5, 0.0, 0.5) - from;
    look.z.atan2(look.x) as f32
}

/// Where a player stands up from the bed whose head block is `head` (the
/// foot lies behind it, opposite `facing`) and the yaw they face there, or
/// `None` when the bed is gone or boxed in.
pub fn bed_stand_up(world: &World, head: IVec3, facing: Facing, java_yaw: f32) -> Option<(DVec3, f32)> {
    world.get_block(head).filter(|b| b.is_bed_head())?;
    let stand = find_stand_up_position(world, head, facing, java_yaw)?;
    Some((stand, look_at_bed(stand, head)))
}

/// A bed's head block and the direction from its foot to it.
pub fn bed_orientation(foot: IVec3, head: IVec3) -> Option<(IVec3, Facing)> {
    Facing::from_offset(head - foot).map(|facing| (head, facing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;

    /// A bed along +X with its foot at (0, 0, 0), on a stone floor at y = -1.
    fn bed() -> (Grid, IVec3) {
        let mut grid = Grid::flat(0);
        grid.set(IVec3::ZERO, Block::BED_FOOT);
        grid.set(IVec3::X, Block::BED_HEAD);
        (grid, IVec3::X)
    }

    fn stand(grid: &Grid, head: IVec3, yaw: f32) -> Option<DVec3> {
        find_stand_up_position(grid, head, Facing::East, yaw)
    }

    #[test]
    fn the_offset_tables_match_java() {
        // Facing east (step +1, 0), side north (0, -1).
        assert_eq!(
            surround_offsets(Facing::East, Facing::North),
            [(0, -1), (-1, -1), (-2, -1), (-2, 0), (-2, 1), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1)]
        );
        assert_eq!(above_offsets(Facing::East), [(0, 0), (-1, 0)]);
        // Facing south (step 0, +1), side west (-1, 0).
        assert_eq!(
            surround_offsets(Facing::South, Facing::West),
            [(-1, 0), (-1, -1), (-1, -2), (0, -2), (1, -2), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1)]
        );
    }

    #[test]
    fn the_player_angle_picks_the_side_to_try_first() {
        // East's clockwise side is south (+Z); Java yaw 0 faces south, so the
        // player is steered to the opposite side.
        assert!(is_facing_angle(Facing::South, 0.0));
        assert!(!is_facing_angle(Facing::North, 0.0));
        assert!(!is_facing_angle(Facing::South, 180.0));
        assert!(is_facing_angle(Facing::East, -90.0) && is_facing_angle(Facing::West, 90.0));
        // A quarter turn off is decided by the sign of the float table's
        // `Mth.cos(1.5707964f)` = +1.2e-16, so south still counts at 90.
        assert!(is_facing_angle(Facing::South, 90.0) && !is_facing_angle(Facing::North, 90.0));
        let (grid, head) = bed();
        assert_eq!(stand(&grid, head, 0.0), Some(DVec3::new(1.5, 0.0, -0.5)), "north of the head");
        assert_eq!(stand(&grid, head, 180.0), Some(DVec3::new(1.5, 0.0, 1.5)), "south of the head");
        assert_eq!(java_yaw_degrees(std::f32::consts::FRAC_PI_2), 0.0);
    }

    #[test]
    fn blocked_sides_fall_through_to_the_next_offset_in_java_order() {
        let (mut grid, head) = bed();
        // Yaw 0 tries north first: beside the head, then beside the foot, and
        // so on round the bed.
        let order = [(1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1), (2, 1), (2, 0), (2, -1)];
        for (blocked, &(x, z)) in order.iter().enumerate() {
            let expected = DVec3::new(x as f64 + 0.5, 0.0, z as f64 + 0.5);
            assert_eq!(stand(&grid, head, 0.0), Some(expected), "after blocking {blocked}");
            grid.set(IVec3::new(x, 0, z), Block::STONE);
            grid.set(IVec3::new(x, 1, z), Block::STONE);
        }
        // With every side stone the player ends up on the head itself.
        assert_eq!(stand(&grid, head, 0.0), Some(DVec3::new(1.5, Block::BED_HEAD.height(), 0.5)));
    }

    #[test]
    fn a_boxed_in_bed_has_no_stand_up_position() {
        let (mut grid, head) = bed();
        for z in -1..=1 {
            for x in -1..=2 {
                if z != 0 || !(0..=1).contains(&x) {
                    grid.set(IVec3::new(x, 0, z), Block::STONE);
                    grid.set(IVec3::new(x, 1, z), Block::STONE);
                }
            }
        }
        // Standing on the head or foot needs two clear blocks above them.
        grid.set(IVec3::new(0, 1, 0), Block::STONE);
        grid.set(IVec3::new(1, 1, 0), Block::STONE);
        assert_eq!(stand(&grid, head, 0.0), None);
        assert_eq!(stand(&grid, head, 180.0), None);
    }

    #[test]
    fn dangerous_floors_are_a_second_choice() {
        let (mut grid, head) = bed();
        // North of the head has a magma floor: the next cell round the bed
        // wins even though north is tried first.
        grid.set(IVec3::new(1, -1, -1), Block::MAGMA);
        assert_eq!(stand(&grid, head, 0.0), Some(DVec3::new(0.5, 0.0, -0.5)), "beside the foot instead");
        // With every other cell blocked, and no headroom on the bed, the
        // magma spot is taken in the second pass.
        for &(x, z) in &[(0, -1), (-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1), (2, 0), (2, -1), (2, 1)] {
            grid.set(IVec3::new(x, 0, z), Block::STONE);
            grid.set(IVec3::new(x, 1, z), Block::STONE);
        }
        grid.set(IVec3::new(0, 1, 0), Block::STONE);
        grid.set(IVec3::new(1, 1, 0), Block::STONE);
        assert_eq!(stand(&grid, head, 0.0), Some(DVec3::new(1.5, 0.0, -0.5)));
        // A dangerous block in the cell itself is refused the same way.
        grid.set(IVec3::new(1, 0, -1), Block::CACTUS);
        assert_ne!(stand(&grid, head, 0.0), Some(DVec3::new(1.5, 0.0, -0.5)));
    }

    #[test]
    fn a_bunk_bed_is_left_beside_either_bed_before_standing_on_the_top() {
        let (mut grid, head) = bed();
        // Raise the bed one block: a lower bed underneath it.
        let lift = IVec3::Y;
        grid.set(IVec3::ZERO, Block::BED_FOOT);
        grid.set(IVec3::X, Block::BED_HEAD);
        grid.set(lift, Block::BED_FOOT);
        grid.set(IVec3::X + lift, Block::BED_HEAD);
        grid.set(IVec3::new(0, -1, 0), Block::STONE);
        let top_head = head + lift;
        // Nothing beside the upper bed has a floor, so the lower bed's sides
        // are tried next.
        let first = stand(&grid, top_head, 0.0).unwrap();
        assert_eq!(first, DVec3::new(1.5, 0.0, -0.5), "beside the lower bed");
        // Put a floor beside the top bed and it is used first.
        grid.set(IVec3::new(1, 0, -1), Block::STONE);
        assert_eq!(stand(&grid, top_head, 0.0), Some(DVec3::new(1.5, 1.0, -0.5)));
        // Enclose both levels: the top bed itself (offset 0 of the above table).
        for z in -1..=1 {
            for x in -1..=2 {
                if z != 0 || !(0..=1).contains(&x) {
                    for y in 0..=3 {
                        grid.set(IVec3::new(x, y, z), Block::STONE);
                    }
                }
            }
        }
        assert_eq!(stand(&grid, top_head, 0.0), Some(DVec3::new(1.5, 1.0 + Block::BED_HEAD.height(), 0.5)));
    }

    #[test]
    fn climbable_cells_and_high_floors_are_rejected() {
        let mut grid = Grid::flat(0);
        assert_eq!(find_safe_dismount_location(&grid, IVec3::ZERO, true), Some(DVec3::new(0.5, 0.0, 0.5)));
        // A fence post is 1.5 high: standing in its cell is out, but its top
        // is a floor half a block up in the cell above.
        grid.set(IVec3::new(2, 0, 0), Block::OAK_FENCE);
        assert_eq!(find_safe_dismount_location(&grid, IVec3::new(2, 0, 0), true), None);
        assert_eq!(find_safe_dismount_location(&grid, IVec3::new(2, 1, 0), true), Some(DVec3::new(2.5, 1.5, 0.5)));
        // No floor at all (the cell below is air): `-Infinity`.
        assert_eq!(find_safe_dismount_location(&grid, IVec3::new(4, 2, 0), true), None);
        // A low ceiling does not fit the 1.8 high body.
        grid.set(IVec3::new(6, 1, 0), Block::STONE);
        assert_eq!(find_safe_dismount_location(&grid, IVec3::new(6, 0, 0), true), None);
        // Portals are not valid spawn cells.
        grid.set(IVec3::new(8, 0, 0), Block::END_PORTAL);
        assert_eq!(find_safe_dismount_location(&grid, IVec3::new(8, 0, 0), false), None);
    }
}
