//! Shared AABB-vs-voxel physics used by the player and mobs.
//!
//! Boxes are described by their feet position (bottom centre) plus a
//! [`Shape`]. Movement is resolved one axis at a time against solid blocks;
//! unloaded chunks count as solid so nothing ever falls out of the world.

use glam::{DVec3, IVec3};

use crate::world::World;
use crate::world::block::Block;

/// Anything that can answer block queries (the world, or a test grid).
pub trait BlockSource {
    /// Block at a position; `None` if it isn't loaded.
    fn block(&self, p: IVec3) -> Option<Block>;
}

impl BlockSource for World {
    #[inline]
    fn block(&self, p: IVec3) -> Option<Block> {
        self.get_block(p)
    }
}

/// Axis-aligned box size: half width on X/Z, full height on Y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub half_width: f64,
    pub height: f64,
}

impl Shape {
    pub const fn new(half_width: f64, height: f64) -> Self {
        Self { half_width, height }
    }

    /// (min, max) corners of the box standing at `pos`.
    pub fn aabb(self, pos: DVec3) -> (DVec3, DVec3) {
        (
            pos - DVec3::new(self.half_width, 0.0, self.half_width),
            pos + DVec3::new(self.half_width, self.height, self.half_width),
        )
    }
}

const EPS: f64 = 1e-5;

/// Which axes were blocked by a [`move_box`] call.
#[derive(Clone, Copy, Default, Debug)]
pub struct Collision {
    /// Landed on (or is resting on) something while moving down.
    pub on_ground: bool,
    /// Blocked moving along X or Z.
    pub horizontal: bool,
    /// Bumped a ceiling.
    pub ceiling: bool,
}

/// Whether the box at `pos` overlaps any solid (or unloaded) block.
pub fn overlaps_solid<W: BlockSource + ?Sized>(world: &W, pos: DVec3, shape: Shape) -> bool {
    let (min, max) = shape.aabb(pos);
    let lo = (min + DVec3::splat(EPS)).floor().as_ivec3();
    let hi = (max - DVec3::splat(EPS)).floor().as_ivec3();
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let block = world.block(IVec3::new(x, y, z));
                // Low blocks (beds) only reach part way up their cell.
                if block.is_none_or(|b| b.is_solid() && min.y + EPS < y as f64 + b.height()) {
                    return true;
                }
            }
        }
    }
    false
}

/// If the box at `pos` overlaps solid blocks, returns `pos` pushed back
/// against them along `axis` (the direction of travel is the sign of `dir`).
pub fn collide<W: BlockSource + ?Sized>(world: &W, pos: DVec3, shape: Shape, axis: usize, dir: f64) -> Option<DVec3> {
    let (min, max) = shape.aabb(pos);
    let lo = (min + DVec3::splat(EPS)).floor().as_ivec3();
    let hi = (max - DVec3::splat(EPS)).floor().as_ivec3();
    let mut hit: Option<f64> = None;
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let b = IVec3::new(x, y, z);
                // Unloaded chunks act solid so we never fall out of the world.
                let block = world.block(b);
                let solid = block.is_none_or(|b| b.is_solid());
                let height = block.map_or(1.0, |b| b.height());
                if !solid || min.y + EPS >= b.y as f64 + height {
                    continue;
                }
                let far = if axis == 1 { height } else { 1.0 };
                let edge = if dir > 0.0 { b[axis] as f64 } else { b[axis] as f64 + far };
                hit = Some(match hit {
                    Some(h) if dir > 0.0 => h.min(edge),
                    Some(h) => h.max(edge),
                    None => edge,
                });
            }
        }
    }
    let edge = hit?;
    let mut out = pos;
    out[axis] = match axis {
        1 if dir > 0.0 => edge - shape.height - EPS,
        1 => edge + EPS,
        _ if dir > 0.0 => edge - shape.half_width - EPS,
        _ => edge + shape.half_width + EPS,
    };
    Some(out)
}

/// Moves a box by `delta`, one axis at a time (Y first so landing takes
/// priority), zeroing the velocity on each blocked axis.
pub fn move_box<W: BlockSource + ?Sized>(
    world: &W,
    pos: &mut DVec3,
    vel: &mut DVec3,
    delta: DVec3,
    shape: Shape,
) -> Collision {
    let mut out = Collision::default();
    for axis in [1usize, 0, 2] {
        if delta[axis] == 0.0 {
            continue;
        }
        let mut next = *pos;
        next[axis] += delta[axis];
        if let Some(resolved) = collide(world, next, shape, axis, delta[axis]) {
            match axis {
                1 if delta[axis] < 0.0 => out.on_ground = true,
                1 => out.ceiling = true,
                _ => out.horizontal = true,
            }
            *pos = resolved;
            vel[axis] = 0.0;
        } else {
            *pos = next;
        }
    }
    out
}

/// Whether the cell containing `p` holds water or lava.
pub fn is_fluid_at<W: BlockSource + ?Sized>(world: &W, p: DVec3) -> bool {
    world.block(p.floor().as_ivec3()).is_some_and(|b| b.is_fluid())
}

/// Whether the cell containing `p` holds lava.
pub fn is_lava_at<W: BlockSource + ?Sized>(world: &W, p: DVec3) -> bool {
    world.block(p.floor().as_ivec3()).is_some_and(|b| b.is_lava())
}

/// Slab test: distance along `dir` (not necessarily normalised; the result
/// is in units of `dir`) at which the ray enters the box, or `None` if it
/// misses. A ray starting inside the box hits at 0.
pub fn ray_aabb(origin: DVec3, dir: DVec3, min: DVec3, max: DVec3) -> Option<f64> {
    let mut t_near = 0.0f64;
    let mut t_far = f64::INFINITY;
    for axis in 0..3 {
        let (o, d) = (origin[axis], dir[axis]);
        if d.abs() < 1e-12 {
            if o < min[axis] || o > max[axis] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d;
        let (mut t0, mut t1) = ((min[axis] - o) * inv, (max[axis] - o) * inv);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        t_near = t_near.max(t0);
        t_far = t_far.min(t1);
        if t_near > t_far {
            return None;
        }
    }
    Some(t_near)
}

#[cfg(test)]
pub mod test_util {
    //! A tiny in-memory block grid for physics tests.

    use super::*;
    use rustc_hash::FxHashMap;

    /// Everything below `floor_y` (exclusive) is stone, above is air,
    /// plus explicit overrides.
    pub struct Grid {
        pub floor_y: i32,
        pub blocks: FxHashMap<IVec3, Block>,
    }

    impl Grid {
        pub fn flat(floor_y: i32) -> Self {
            Self { floor_y, blocks: FxHashMap::default() }
        }

        pub fn set(&mut self, p: IVec3, b: Block) {
            self.blocks.insert(p, b);
        }
    }

    impl BlockSource for Grid {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(self.blocks.get(&p).copied().unwrap_or(if p.y < self.floor_y { Block::STONE } else { Block::AIR }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::Grid;
    use super::*;

    #[test]
    fn ray_hits_box_from_outside_and_inside() {
        let (min, max) = (DVec3::new(1.0, 0.0, -0.5), DVec3::new(2.0, 1.0, 0.5));
        let t = ray_aabb(DVec3::new(0.0, 0.5, 0.0), DVec3::X, min, max).unwrap();
        assert!((t - 1.0).abs() < 1e-9);
        // Diagonal ray, unnormalised direction: t is in units of `dir`.
        let t = ray_aabb(DVec3::new(0.0, 0.5, -1.0), DVec3::new(2.0, 0.0, 1.0), min, max).unwrap();
        assert!((t - 0.5).abs() < 1e-9, "t = {t}");
        assert_eq!(ray_aabb(DVec3::new(1.5, 0.5, 0.0), DVec3::Y, min, max), Some(0.0), "inside");
        assert_eq!(ray_aabb(DVec3::new(0.0, 0.5, 0.0), DVec3::NEG_X, min, max), None, "behind");
        assert_eq!(ray_aabb(DVec3::new(0.0, 1.5, 0.0), DVec3::X, min, max), None, "above");
        assert_eq!(ray_aabb(DVec3::new(0.0, 0.5, 2.0), DVec3::X, min, max), None, "parallel, outside");
    }

    #[test]
    fn box_lands_and_is_stopped_by_walls() {
        let mut grid = Grid::flat(10);
        grid.set(IVec3::new(3, 10, 0), Block::STONE);
        let shape = Shape::new(0.3, 1.8);
        let mut pos = DVec3::new(0.5, 12.0, 0.5);
        let mut vel = DVec3::ZERO;
        let mut c = Collision::default();
        for _ in 0..50 {
            c = move_box(&grid, &mut pos, &mut vel, DVec3::new(0.0, -0.1, 0.0), shape);
        }
        assert!(c.on_ground);
        assert!((pos.y - 10.0).abs() < 1e-3);
        for _ in 0..50 {
            c = move_box(&grid, &mut pos, &mut vel, DVec3::new(0.1, 0.0, 0.0), shape);
        }
        assert!(c.horizontal);
        assert!((pos.x - 2.7).abs() < 1e-3, "x = {}", pos.x);
        assert!(!overlaps_solid(&grid, pos, shape));
        assert!(overlaps_solid(&grid, pos + DVec3::X * 0.1, shape));
    }

    #[test]
    fn beds_are_stood_on_at_their_height() {
        let mut grid = Grid::flat(10);
        grid.set(IVec3::new(0, 10, 0), Block::BED_FOOT);
        let shape = Shape::new(0.3, 1.8);
        let (mut pos, mut vel) = (DVec3::new(0.5, 12.0, 0.5), DVec3::ZERO);
        for _ in 0..50 {
            move_box(&grid, &mut pos, &mut vel, DVec3::new(0.0, -0.1, 0.0), shape);
        }
        assert!((pos.y - 10.5625).abs() < 1e-3, "y = {}", pos.y);
        // Walking off the bed isn't blocked by the bed itself.
        let c = move_box(&grid, &mut pos, &mut vel, DVec3::new(1.0, 0.0, 0.0), shape);
        assert!(!c.horizontal);
    }
}
