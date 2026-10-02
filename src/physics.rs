//! Shared AABB-vs-voxel physics used by the player and mobs.
//!
//! Boxes are described by their feet position (bottom centre) plus a
//! [`Shape`]. Movement is resolved one axis at a time against solid blocks;
//! unloaded chunks count as solid so nothing ever falls out of the world.

use glam::{DVec3, IVec3};

use crate::world::World;
use crate::world::block::{Block, Facing, RenderKind};
use crate::world::shape;

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

/// Calls `f` with the (min, max) of every solid box in cell `p`: the whole
/// cell for unloaded chunks, a lowered box for low blocks (beds, slabs) and
/// a few smaller ones for shaped blocks (stairs, fences, doors).
fn cell_boxes<W: BlockSource + ?Sized>(world: &W, p: IVec3, mut f: impl FnMut(DVec3, DVec3)) {
    let lo = p.as_dvec3();
    match world.block(p) {
        None => f(lo, lo + DVec3::ONE),
        Some(b) if !b.is_solid() => {}
        Some(b) if b.kind() == RenderKind::Shaped => {
            let neighbour = |side: Facing| world.block(p + side.offset()).unwrap_or(Block::AIR);
            for bx in shape::collision(b, neighbour).as_slice() {
                let v = |c: [u8; 3]| DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64) / 16.0;
                f(lo + v(bx.min), lo + v(bx.max));
            }
        }
        Some(b) => f(lo, lo + DVec3::new(1.0, b.height(), 1.0)),
    }
}

/// Calls `f` with every solid box that could touch the box (min, max):
/// those in the cells it covers, and in the row below (fences reach up
/// into the next cell).
fn boxes_near<W: BlockSource + ?Sized>(world: &W, min: DVec3, max: DVec3, mut f: impl FnMut(DVec3, DVec3)) {
    let lo = (min + DVec3::splat(EPS)).floor().as_ivec3();
    let hi = (max - DVec3::splat(EPS)).floor().as_ivec3();
    for y in lo.y - 1..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                cell_boxes(world, IVec3::new(x, y, z), |bmin, bmax| {
                    if bmin.cmplt(max - DVec3::splat(EPS)).all() && bmax.cmpgt(min + DVec3::splat(EPS)).all() {
                        f(bmin, bmax);
                    }
                });
            }
        }
    }
}

/// Whether the box at `pos` overlaps any solid (or unloaded) block.
pub fn overlaps_solid<W: BlockSource + ?Sized>(world: &W, pos: DVec3, shape: Shape) -> bool {
    let (min, max) = shape.aabb(pos);
    let mut hit = false;
    boxes_near(world, min, max, |_, _| hit = true);
    hit
}

/// If the box at `pos` overlaps solid blocks, returns `pos` pushed back
/// against them along `axis` (the direction of travel is the sign of `dir`).
pub fn collide<W: BlockSource + ?Sized>(world: &W, pos: DVec3, shape: Shape, axis: usize, dir: f64) -> Option<DVec3> {
    let (min, max) = shape.aabb(pos);
    let mut hit: Option<f64> = None;
    boxes_near(world, min, max, |bmin, bmax| {
        let edge = if dir > 0.0 { bmin[axis] } else { bmax[axis] };
        hit = Some(match hit {
            Some(h) if dir > 0.0 => h.min(edge),
            Some(h) => h.max(edge),
            None => edge,
        });
    });
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

/// Like [`move_box`], but a box on the ground that walks into something at
/// most `step` tall (a slab, a stair) steps up onto it, like Minecraft's
/// 0.6-block step height.
pub fn move_box_stepping<W: BlockSource + ?Sized>(
    world: &W,
    pos: &mut DVec3,
    vel: &mut DVec3,
    delta: DVec3,
    shape: Shape,
    step: f64,
) -> Collision {
    let (start, start_vel) = (*pos, *vel);
    let out = move_box(world, pos, vel, delta, shape);
    if !out.horizontal || delta.y > 0.0 {
        return out;
    }
    // Retry: rise, move across, then settle back down onto whatever is there.
    let (mut p, mut v) = (start, start_vel);
    move_box(world, &mut p, &mut v, DVec3::new(0.0, step, 0.0), shape);
    let across = move_box(world, &mut p, &mut v, DVec3::new(delta.x, 0.0, delta.z), shape);
    let fall = DVec3::new(0.0, start.y - p.y + delta.y.min(0.0), 0.0);
    let down = move_box(world, &mut p, &mut v, fall, shape);
    let gained = |q: DVec3| (q.x - start.x).powi(2) + (q.z - start.z).powi(2);
    if !down.on_ground || p.y <= start.y + EPS || gained(p) <= gained(*pos) + 1e-9 {
        return out;
    }
    *pos = p;
    vel.x = v.x;
    vel.z = v.z;
    vel.y = 0.0;
    Collision { on_ground: true, horizontal: across.horizontal, ceiling: false }
}

/// Whether the cell containing `p` holds water or lava.
pub fn is_fluid_at<W: BlockSource + ?Sized>(world: &W, p: DVec3) -> bool {
    world.block(p.floor().as_ivec3()).is_some_and(|b| b.is_fluid())
}

/// Whether any part of an entity's box touches a block matching `test`.
/// The epsilon excludes cells whose face is merely flush with the box.
pub fn touches_block<W: BlockSource + ?Sized>(world: &W, pos: DVec3, shape: Shape, test: fn(Block) -> bool) -> bool {
    let (min, max) = shape.aabb(pos);
    let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
    (lo.y..=hi.y)
        .any(|y| (lo.z..=hi.z).any(|z| (lo.x..=hi.x).any(|x| world.block(IVec3::new(x, y, z)).is_some_and(test))))
}

/// Slab test: distance along `dir` (not necessarily normalised; the result
/// is in units of `dir`) at which the ray enters the box, or `None` if it
/// misses. A ray starting inside the box hits at 0.
pub fn ray_aabb(origin: DVec3, dir: DVec3, min: DVec3, max: DVec3) -> Option<f64> {
    ray_aabb_face(origin, dir, min, max).map(|(t, _)| t)
}

/// Like [`ray_aabb`], plus the outward normal of the face the ray enters
/// through (zero if it starts inside).
pub fn ray_aabb_face(origin: DVec3, dir: DVec3, min: DVec3, max: DVec3) -> Option<(f64, IVec3)> {
    let mut t_near = 0.0f64;
    let mut t_far = f64::INFINITY;
    let mut normal = IVec3::ZERO;
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
        if t0 > t_near {
            t_near = t0;
            normal = IVec3::ZERO;
            normal[axis] = if d > 0.0 { -1 } else { 1 };
        }
        t_far = t_far.min(t1);
        if t_near > t_far {
            return None;
        }
    }
    Some((t_near, normal))
}

/// Where a ray first hits the boxes of the shaped block at `cell`: the
/// distance (in units of `dir`) and the face normal.
pub fn ray_shape<W: BlockSource + ?Sized>(world: &W, cell: IVec3, origin: DVec3, dir: DVec3) -> Option<(f64, IVec3)> {
    let b = world.block(cell)?;
    let neighbour = |side: Facing| world.block(cell + side.offset()).unwrap_or(Block::AIR);
    let lo = cell.as_dvec3();
    let v = |c: [u8; 3]| DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64) / 16.0;
    shape::shape(b, neighbour)
        .as_slice()
        .iter()
        .filter_map(|bx| ray_aabb_face(origin, dir, lo + v(bx.min), lo + v(bx.max)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
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
    fn fire_contact_checks_the_whole_body_and_excludes_flush_faces() {
        let mut world = Grid::flat(0);
        let shape = Shape::new(0.3, 1.8);
        let feet = DVec3::new(0.5, 0.0, 0.5);
        world.set(IVec3::new(0, 1, 0), Block::FIRE);
        assert!(touches_block(&world, feet, shape, Block::is_fire), "head touches fire");
        assert!(!touches_block(&world, feet, Shape::new(0.3, 1.0), Block::is_fire), "flush below");
        assert!(!touches_block(&world, feet + DVec3::X, shape, Block::is_fire));
        assert!(touches_block(&world, feet + DVec3::X * 0.7, shape, Block::is_fire), "body edge touches");
        assert!(!touches_block(&world, feet + DVec3::X * 0.8, shape, Block::is_fire), "flush beside");
    }
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

    #[test]
    fn walking_steps_up_slabs_and_stairs_but_not_blocks() {
        use crate::world::block::Facing;
        let shape = Shape::new(0.3, 1.8);
        for (block, rise) in [
            (Block::slab_of(Block::STONE).unwrap(), Some(0.5)),
            // Stairs climb in two half steps.
            (Block::STONE_STAIRS.with_facing(Facing::West), Some(1.0)),
            (Block::STONE, None),
        ] {
            let mut grid = Grid::flat(10);
            grid.set(IVec3::new(1, 10, 0), block);
            let (mut pos, mut vel) = (DVec3::new(0.5, 10.0, 0.5), DVec3::ZERO);
            for _ in 0..20 {
                move_box_stepping(&grid, &mut pos, &mut vel, DVec3::new(0.05, -0.01, 0.0), shape, 0.6);
            }
            match rise {
                Some(r) => assert!((pos.y - 10.0 - r).abs() < 1e-3 && pos.x > 1.0, "{}: {pos}", block.name()),
                None => assert!(pos.y < 10.01 && pos.x < 0.71, "{}: {pos}", block.name()),
            }
        }
    }

    #[test]
    fn fences_are_too_tall_to_jump_but_open_gates_let_you_by() {
        use crate::world::block::Facing;
        let shape = Shape::new(0.3, 1.8);
        let mut grid = Grid::flat(10);
        grid.set(IVec3::new(1, 10, 0), Block::OAK_FENCE);
        // Even 1.2 blocks up (the top of a jump), the fence is in the way.
        assert!(overlaps_solid(&grid, DVec3::new(1.5, 11.2, 0.5), shape));
        assert!(!overlaps_solid(&grid, DVec3::new(1.5, 11.6, 0.5), shape));
        // Beside the thin post there's room.
        assert!(!overlaps_solid(&grid, DVec3::new(1.5, 10.0, 1.5), shape));
        grid.set(IVec3::new(1, 10, 0), Block::gate(Facing::South, false));
        assert!(overlaps_solid(&grid, DVec3::new(1.5, 10.0, 0.5), shape));
        grid.set(IVec3::new(1, 10, 0), Block::gate(Facing::South, true));
        assert!(!overlaps_solid(&grid, DVec3::new(1.5, 10.0, 0.5), shape));
    }

    #[test]
    fn rays_pass_through_the_open_part_of_shaped_blocks() {
        use crate::world::block::Facing;
        let mut grid = Grid::flat(0);
        // A closed door facing south: the panel is on the south side.
        let door = IVec3::new(0, 5, 0);
        grid.set(door, Block::door(Facing::South, false, false));
        let hit = ray_shape(&grid, door, DVec3::new(0.5, 5.5, 3.0), DVec3::NEG_Z).unwrap();
        assert!((hit.0 - 2.0).abs() < 1e-9 && hit.1 == IVec3::Z, "{hit:?}");
        // From the side, a ray just north of the panel misses it.
        assert!(ray_shape(&grid, door, DVec3::new(-1.0, 5.5, 0.5), DVec3::X).is_none());
        assert!(ray_shape(&grid, door, DVec3::new(-1.0, 5.5, 0.9), DVec3::X).is_some());
    }
}
