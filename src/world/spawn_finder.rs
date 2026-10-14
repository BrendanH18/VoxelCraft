//! Java 26.3 `PlayerSpawnFinder`: where a player without a bed or spawn point
//! appears. Candidate columns inside the respawn radius are visited in a
//! shuffled order (a random start, then a fixed coprime stride), and the first
//! dry column with a solid top face and room for the player wins; otherwise
//! the suggested position is moved up out of blocks and down to the ground.

use glam::{DVec3, IVec3};

use crate::physics::{self, BlockSource};
use crate::world::block::RenderKind;

/// Java's `ABSOLUTE_MAX_ATTEMPTS`.
const MAX_ATTEMPTS: i64 = 1024;

/// Whether the player's box fits at the bottom centre of `cell` without
/// touching a block or a liquid (`noCollisionNoLiquid`).
fn fits<W: BlockSource + ?Sized>(world: &W, cell: IVec3) -> bool {
    let at = DVec3::new(cell.x as f64 + 0.5, cell.y as f64, cell.z as f64 + 0.5);
    !physics::overlaps_solid(world, at, crate::player::SHAPE)
        && !physics::touches_block(world, at, crate::player::SHAPE, |b| b.is_fluid() || b.is_waterlogged())
}

fn bottom_centre(cell: IVec3) -> DVec3 {
    DVec3::new(cell.x as f64 + 0.5, cell.y as f64, cell.z as f64 + 0.5)
}

/// `getLevelRespawnPos`: the cell above the topmost full-faced block of the
/// column, unless water covers it. `top` is the column's highest block.
fn column_spawn<W: BlockSource + ?Sized>(world: &W, x: i32, z: i32, top: i32, min_y: i32) -> Option<IVec3> {
    let mut y = top + 1;
    while y >= min_y {
        let block = world.block(IVec3::new(x, y, z))?;
        if block.is_fluid() || block.is_waterlogged() {
            return None;
        }
        if block.is_solid() && block.kind() != RenderKind::Shaped && block.height() >= 1.0 {
            return Some(IVec3::new(x, y + 1, z));
        }
        y -= 1;
    }
    None
}

/// `fixupSpawnHeight`: up out of any block, then down to the ground.
fn fixup_spawn_height<W: BlockSource + ?Sized>(world: &W, suggestion: IVec3, min_y: i32, max_y: i32) -> DVec3 {
    let mut cell = suggestion;
    while !fits(world, cell) && cell.y < max_y {
        cell.y += 1;
    }
    cell.y -= 1;
    while fits(world, cell) && cell.y > min_y {
        cell.y -= 1;
    }
    cell.y += 1;
    bottom_centre(cell)
}

/// Finds the spawn for `suggestion` (the world spawn) with the `spawnRadius`
/// game rule, starting the shuffled walk at `offset` (any value: Java draws
/// it randomly each time). `top` gives a column's highest block, if known;
/// `min_y..max_y` bound the world. Adventure-default worlds skip the search.
pub fn find_spawn<W: BlockSource + ?Sized>(
    world: &W,
    suggestion: IVec3,
    radius: i32,
    offset: u32,
    adventure: bool,
    top: impl Fn(i32, i32) -> Option<i32>,
    (min_y, max_y): (i32, i32),
) -> DVec3 {
    if !adventure {
        let radius = i64::from(radius.max(0));
        let width = radius * 2 + 1;
        let count = MAX_ATTEMPTS.min(width * width);
        let coprime = if count <= 16 { count - 1 } else { 17 };
        let start = i64::from(offset) % count;
        for i in 0..count {
            let index = (start + coprime * i) % count;
            let x = suggestion.x + (index % width) as i32 - radius as i32;
            let z = suggestion.z + (index / width) as i32 - radius as i32;
            if let Some(cell) = top(x, z).and_then(|top| column_spawn(world, x, z, top, min_y))
                && fits(world, cell)
            {
                return bottom_centre(cell);
            }
        }
    }
    fixup_spawn_height(world, suggestion, min_y, max_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;
    use crate::world::block::Block;

    fn top_of(grid: &Grid) -> impl Fn(i32, i32) -> Option<i32> + '_ {
        move |x, z| (-40..40).rev().find(|&y| grid.block(IVec3::new(x, y, z)).is_some_and(|b| b != Block::AIR))
    }

    #[test]
    fn radius_zero_spawns_in_the_suggested_column() {
        let grid = Grid::flat(10);
        let at = find_spawn(&grid, IVec3::new(5, 10, 7), 0, 3, false, top_of(&grid), (-40, 40));
        assert_eq!(at, DVec3::new(5.5, 10.0, 7.5));
    }

    #[test]
    fn the_walk_visits_each_candidate_once_and_stays_in_the_radius() {
        let grid = Grid::flat(10);
        let mut seen = std::collections::HashSet::new();
        for offset in 0..60 {
            let at = find_spawn(&grid, IVec3::ZERO, 3, offset, false, top_of(&grid), (-40, 40));
            assert!(at.x.abs() <= 3.5 && at.z.abs() <= 3.5 && at.y == 10.0, "{at}");
            seen.insert((at.x as i32, at.z as i32));
        }
        assert!(seen.len() > 20, "random starts spread over the 7x7 square: {}", seen.len());
        // 49 candidates and a stride of 17: the first candidate is the offset itself.
        let first = find_spawn(&grid, IVec3::ZERO, 3, 24, false, top_of(&grid), (-40, 40));
        assert_eq!((first.x, first.z), (0.5, 0.5));
    }

    #[test]
    fn flooded_columns_are_skipped() {
        let mut grid = Grid::flat(10);
        // Every column but (1, 0) is under water.
        for x in -1..=1 {
            for z in -1..=1 {
                if (x, z) != (1, 0) {
                    grid.set(IVec3::new(x, 10, z), Block::WATER);
                }
            }
        }
        for offset in 0..9 {
            let at = find_spawn(&grid, IVec3::ZERO, 1, offset, false, top_of(&grid), (-40, 40));
            assert_eq!(at, DVec3::new(1.5, 10.0, 0.5), "offset {offset}");
        }
    }

    #[test]
    fn a_roof_is_a_valid_floor_like_any_full_block() {
        let mut grid = Grid::flat(10);
        grid.set(IVec3::new(0, 11, 0), Block::STONE);
        let at = find_spawn(&grid, IVec3::ZERO, 0, 0, false, top_of(&grid), (-40, 40));
        assert_eq!(at, DVec3::new(0.5, 12.0, 0.5), "Java uses the motion-blocking top, not the cave floor");
    }

    #[test]
    fn with_no_dry_candidate_the_suggestion_is_lifted_out_of_blocks() {
        let mut grid = Grid::flat(10);
        let suggestion = IVec3::new(0, 4, 0);
        // Inside the ground: moves up until the player fits, landing on the surface.
        assert_eq!(find_spawn(&grid, suggestion, 0, 0, true, |_, _| None, (-40, 40)), DVec3::new(0.5, 10.0, 0.5));
        // Floating in air: falls to the ground.
        let high = IVec3::new(0, 30, 0);
        assert_eq!(find_spawn(&grid, high, 0, 0, true, |_, _| None, (-40, 40)), DVec3::new(0.5, 10.0, 0.5));
        grid.set(IVec3::new(0, 10, 0), Block::WATER);
        // Adventure never searches, even when the suggested column is fine.
        let adventure = find_spawn(&grid, IVec3::new(2, 10, 2), 5, 9, true, top_of(&grid), (-40, 40));
        assert_eq!(adventure, DVec3::new(2.5, 10.0, 2.5));
    }
}
