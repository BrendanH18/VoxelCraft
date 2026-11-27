//! Nether portals: lighting an obsidian frame, breaking a portal that lost
//! its frame, and finding or building the portal at the far end of a trip.
//!
//! A frame is a rectangle of obsidian around an empty interior 2..=21 wide
//! and 3..=21 tall, standing along X or Z (corners optional, as in
//! Minecraft). Portal blocks fill the interior.

use glam::{DVec3, IVec3};

use super::World;
use super::block::Block;

const MIN_W: i32 = 2;
const MAX_W: i32 = 21;
const MIN_H: i32 = 3;
const MAX_H: i32 = 21;
const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
/// v0.16 Track G: `PortalForcer.NETHER_PORTAL_RADIUS` and
/// `OVERWORLD_PORTAL_RADIUS`, the existing-portal search into and out of
/// the Nether; `createPortal` spirals 16 out; its fallback ledge sits at y
/// 70 or more and 9 or more below the top.
pub const NETHER_PORTAL_RADIUS: i32 = 16;
pub const OVERWORLD_PORTAL_RADIUS: i32 = 128;
pub const CREATE_RADIUS: i32 = 16;
pub const LEDGE_MIN_Y: i32 = 70;
pub const LEDGE_BELOW_TOP: i32 = 9;
/// `BlockUtil.getLargestRectangleAround`'s reach each way, as portal
/// travel calls it (21 along the portal and up).
pub const RECTANGLE_LIMIT: i32 = 21;

/// v1.0 gap Track 5: Java's `WorldBorder` with its default settings
/// (`WorldBorder.Settings.DEFAULT`: centred on 0, 0 and 59,999,968 across).
/// In 26.3 every dimension keeps its own border, created with these, and
/// VoxelCraft has no `/worldborder` to move it, so every dimension's border
/// is this one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldBorder {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

pub const BORDER: WorldBorder =
    WorldBorder { min_x: -29_999_984.0, max_x: 29_999_984.0, min_z: -29_999_984.0, max_z: 29_999_984.0 };

impl WorldBorder {
    /// `isWithinBounds(x, z)`: the west and north edges are inside.
    pub fn contains(&self, x: f64, z: f64) -> bool {
        x >= self.min_x && x < self.max_x && z >= self.min_z && z < self.max_z
    }

    /// `isWithinBounds(BlockPos)`: the block's corner.
    pub fn contains_block(&self, p: IVec3) -> bool {
        self.contains(f64::from(p.x), f64::from(p.z))
    }

    /// `isWithinBounds(AABB)`: both corners, the far one just inside.
    pub fn contains_box(&self, min: DVec3, max: DVec3) -> bool {
        let inset = f64::from(1.0e-5_f32);
        self.contains(min.x, min.z) && self.contains(max.x - inset, max.z - inset)
    }

    /// `clampToBounds(x, y, z)`: the block holding the position moved
    /// inside the border.
    pub fn clamp(&self, p: DVec3) -> IVec3 {
        let inset = f64::from(1.0e-5_f32);
        DVec3::new(p.x.clamp(self.min_x, self.max_x - inset), p.y, p.z.clamp(self.min_z, self.max_z - inset))
            .floor()
            .as_ivec3()
    }
}

/// `BlockUtil.FoundRectangle` of a portal: its bottom corner on the
/// negative side, its width along the portal and its height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalRect {
    pub min: IVec3,
    pub width: i32,
    pub height: i32,
}

/// A trip through a Nether portal, as `NetherPortalBlock.getPortalDestination`
/// sets out: where to look for the exit (the scaled position clamped to the
/// border), the entry portal's axis, where in the entry portal the
/// traveller was (`PortalShape.getRelativePosition`) and their motion,
/// which they keep (`Relative.DELTA`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalTrip {
    pub target: IVec3,
    pub along_x: bool,
    pub offset: DVec3,
    pub vel: DVec3,
}

impl PortalTrip {
    /// A trip that didn't start in a portal block: Java's fallback, the
    /// middle of the exit portal's bottom (axis X, offset (0.5, 0, 0)).
    pub fn without_portal(target: IVec3) -> Self {
        Self { target, along_x: true, offset: DVec3::new(0.5, 0.0, 0.0), vel: DVec3::ZERO }
    }
}

/// `BlockUtil.getLargestRectangleAround(center, axis, 21, Y, 21, test)`:
/// the largest rectangle of cells passing `test` in the portal's plane
/// through `center` (along X or Z), reached from `center` without leaving
/// the run of passing cells above and below it.
pub fn largest_rectangle_around(center: IVec3, along_x: bool, test: impl Fn(IVec3) -> bool) -> PortalRect {
    let axis = if along_x { IVec3::X } else { IVec3::Z };
    let limit = |from: IVec3, dir: IVec3, limit: i32| {
        let mut n = 0;
        while n < limit && test(from + dir * (n + 1)) {
            n += 1;
        }
        n
    };
    let neg1 = limit(center, -axis, RECTANGLE_LIMIT);
    let pos1 = limit(center, axis, RECTANGLE_LIMIT);
    let columns = (neg1 + 1 + pos1) as usize;
    // Each column's reach down and up, never past its neighbour's nearer the centre.
    let mut bounds = vec![(0, 0); columns];
    let centre = neg1 as usize;
    bounds[centre] = (limit(center, IVec3::NEG_Y, RECTANGLE_LIMIT), limit(center, IVec3::Y, RECTANGLE_LIMIT));
    let centre2 = bounds[centre].0;
    for i in 1..=neg1 as usize {
        let last = bounds[centre - (i - 1)];
        let at = center - axis * i as i32;
        bounds[centre - i] = (limit(at, IVec3::NEG_Y, last.0), limit(at, IVec3::Y, last.1));
    }
    for i in 1..=pos1 as usize {
        let last = bounds[centre + i - 1];
        let at = center + axis * i as i32;
        bounds[centre + i] = (limit(at, IVec3::NEG_Y, last.0), limit(at, IVec3::Y, last.1));
    }
    let (mut min1, mut min2, mut size1, mut size2) = (0, 0, 0, 0);
    let mut heights = vec![0; columns];
    for i2 in (0..=centre2).rev() {
        for (i1, &(down, up)) in bounds.iter().enumerate() {
            let (lo, hi) = (centre2 - down, centre2 + up);
            heights[i1] = if i2 >= lo && i2 <= hi { hi + 1 - i2 } else { 0 };
        }
        let (start, end, height) = max_rectangle(&heights);
        let new1 = 1 + end - start;
        if new1 * height > size1 * size2 {
            (min1, min2, size1, size2) = (start, i2, new1, height);
        }
    }
    PortalRect { min: center + axis * (min1 - neg1) + IVec3::Y * (min2 - centre2), width: size1, height: size2 }
}

/// `BlockUtil.getMaxRectangleLocation`: the largest rectangle under a
/// histogram, as (first column, last column, height).
fn max_rectangle(columns: &[i32]) -> (i32, i32, i32) {
    let (mut start_max, mut end_max, mut height_max) = (0i32, 0i32, 0i32);
    let mut stack: Vec<usize> = vec![0];
    for column in 1..=columns.len() {
        let height = columns.get(column).copied().unwrap_or(0);
        while let Some(&top) = stack.last() {
            let stack_height = columns[top];
            if height >= stack_height {
                stack.push(column);
                break;
            }
            stack.pop();
            let start = stack.last().map_or(0, |&t| t + 1) as i32;
            if stack_height * (column as i32 - start) > height_max * (end_max - start_max) {
                end_max = column as i32;
                start_max = start;
                height_max = stack_height;
            }
        }
        if stack.is_empty() {
            stack.push(column);
        }
    }
    (start_max, end_max - 1, height_max)
}

/// `PortalShape.getRelativePosition`: where an entity of `width` by
/// `height` (Java's float dimensions) stands in the portal: across it and
/// up it as fractions of the room it has, and its distance from the
/// portal's centre plane.
pub fn relative_position(rect: PortalRect, along_x: bool, pos: DVec3, width: f64, height: f64) -> DVec3 {
    let room = f64::from(rect.width) - width;
    let headroom = f64::from(rect.height) - height;
    let min = rect.min.as_dvec3();
    let (across, forward, min_across, min_forward) =
        if along_x { (pos.x, pos.z, min.x, min.z) } else { (pos.z, pos.x, min.z, min.x) };
    let right = if room > 0.0 { ((across - (min_across + width / 2.0)) / room).clamp(0.0, 1.0) } else { 0.5 };
    let up = if headroom > 0.0 { ((pos.y - min.y) / headroom).clamp(0.0, 1.0) } else { 0.0 };
    DVec3::new(right, up, forward - (min_forward + 0.5))
}

/// `NetherPortalBlock.createDimensionTransition`'s position: the same place
/// in the exit portal (along X if `along_x`) for an entity of `width` by
/// `height`, before `findCollisionFreePosition`.
pub fn exit_position(rect: PortalRect, along_x: bool, offset: DVec3, width: f64, height: f64) -> DVec3 {
    let right = width / 2.0 + (f64::from(rect.width) - width) * offset.x;
    let up = (f64::from(rect.height) - height) * offset.y;
    let forward = 0.5 + offset.z;
    let min = rect.min.as_dvec3();
    if along_x { min + DVec3::new(right, up, forward) } else { min + DVec3::new(forward, up, right) }
}

impl World {
    fn is(&self, p: IVec3, b: Block) -> bool {
        self.get_block(p) == Some(b)
    }

    /// Lights the obsidian frame around the empty cell `p`, filling it with
    /// portal blocks. Returns whether there was a frame to light.
    pub fn light_portal(&mut self, p: IVec3) -> bool {
        for axis in [IVec3::X, IVec3::Z] {
            if let Some((corner, w, h)) = self.find_frame(p, axis) {
                for dy in 0..h {
                    for i in 0..w {
                        // Straight in: the half-filled portal isn't whole yet.
                        self.edit(corner + axis * i + IVec3::Y * dy, Block::NETHER_PORTAL, true);
                    }
                }
                return true;
            }
        }
        false
    }

    /// The bottom corner, width and height of the interior of a frame along
    /// `axis` that contains `p`.
    fn find_frame(&self, p: IVec3, axis: IVec3) -> Option<(IVec3, i32, i32)> {
        let open = |q: IVec3| self.get_block(q).is_some_and(|b| b == Block::AIR || b.is_fire());
        let obsidian = |q: IVec3| self.is(q, Block::OBSIDIAN);
        if !open(p) {
            return None;
        }
        let mut bottom = p;
        while open(bottom - IVec3::Y) && p.y - bottom.y < MAX_H {
            bottom -= IVec3::Y;
        }
        let mut left = bottom;
        while open(left - axis) && obsidian(left - axis - IVec3::Y) && (bottom - left).abs().max_element() < MAX_W {
            left -= axis;
        }
        if !obsidian(left - IVec3::Y) || !obsidian(left - axis) {
            return None;
        }
        let mut w = 0;
        while w < MAX_W && open(left + axis * w) && obsidian(left + axis * w - IVec3::Y) {
            w += 1;
        }
        if w < MIN_W || !obsidian(left + axis * w) {
            return None;
        }
        let mut h = 0;
        while h < MAX_H
            && (0..w).all(|i| open(left + axis * i + IVec3::Y * h))
            && obsidian(left - axis + IVec3::Y * h)
            && obsidian(left + axis * w + IVec3::Y * h)
        {
            h += 1;
        }
        let roofed = (0..w).all(|i| obsidian(left + axis * i + IVec3::Y * h));
        (h >= MIN_H && roofed).then_some((left, w, h))
    }

    /// A portal block stays while it's held above and below, and on both
    /// sides along X or along Z, by portal or obsidian.
    fn portal_supported(&self, p: IVec3) -> bool {
        let holds = |d: IVec3| matches!(self.get_block(p + d), Some(Block::NETHER_PORTAL | Block::OBSIDIAN) | None);
        holds(IVec3::Y)
            && holds(IVec3::NEG_Y)
            && ((holds(IVec3::X) && holds(IVec3::NEG_X)) || (holds(IVec3::Z) && holds(IVec3::NEG_Z)))
    }

    /// After an edit at `p`: portal blocks next to it that lost their frame
    /// vanish, and with them the rest of their portal.
    pub(super) fn break_unsupported_portals(&mut self, p: IVec3) {
        let mut queue: Vec<IVec3> = SIDES.iter().map(|&d| p + d).collect();
        let mut budget = 4 * MAX_W * MAX_H;
        while let Some(q) = queue.pop() {
            if self.is(q, Block::NETHER_PORTAL) && !self.portal_supported(q) {
                self.edit(q, Block::AIR, true);
                queue.extend(SIDES.iter().map(|&d| q + d));
                budget -= 1;
                if budget == 0 {
                    return;
                }
            }
        }
    }

    /// v0.16 Track G: `PortalForcer.findClosestPortalPosition`. Every
    /// Nether portal block is a point of interest; of those within `radius`
    /// blocks horizontally (16 into the Nether, 128 out of it) at any
    /// height and inside the world border (v1.0 gap Track 5), the nearest
    /// to `near` (Java's `distSqr`, lower first on a tie). Chunks that are
    /// neither loaded nor saved were never changed, so hold no portal.
    pub fn find_portal(&self, near: IVec3, radius: i32) -> Option<IVec3> {
        use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I as S};
        let mut best: Option<(i64, i32, IVec3)> = None;
        let (min_y, max_y) = (self.min_y(), self.max_y());
        for cz in (near.z - radius).div_euclid(S)..=(near.z + radius).div_euclid(S) {
            for cx in (near.x - radius).div_euclid(S)..=(near.x + radius).div_euclid(S) {
                for cy in min_y.div_euclid(S)..=(max_y - 1).div_euclid(S) {
                    let Some(data) = self.section_data(IVec3::new(cx, cy, cz)) else { continue };
                    if !data.contains(Block::NETHER_PORTAL) {
                        continue;
                    }
                    for i in 0..super::chunk::CHUNK_VOLUME {
                        let (x, y, z) = (i % CHUNK_SIZE, (i / CHUNK_SIZE) % CHUNK_SIZE, i / (CHUNK_SIZE * CHUNK_SIZE));
                        if data.get(x, y, z) != Block::NETHER_PORTAL {
                            continue;
                        }
                        let p = IVec3::new(cx * S + x as i32, cy * S + y as i32, cz * S + z as i32);
                        if (p.x - near.x).abs() > radius || (p.z - near.z).abs() > radius || !BORDER.contains_block(p) {
                            continue;
                        }
                        let d = (p - near).as_i64vec3().length_squared();
                        if best.is_none_or(|(bd, by, _)| (d, p.y) < (bd, by)) {
                            best = Some((d, p.y, p));
                        }
                    }
                }
            }
        }
        best.map(|(_, _, p)| p)
    }

    /// Whether the portal at `p` stands along X (portal or obsidian on
    /// both sides along X); VoxelCraft's portal blocks don't store Java's
    /// `axis`.
    pub fn portal_along_x(&self, p: IVec3) -> bool {
        let holds = |q: IVec3| matches!(self.get_block(q), Some(Block::NETHER_PORTAL | Block::OBSIDIAN));
        holds(p + IVec3::X) && holds(p - IVec3::X)
    }

    /// The portal around portal block `p` as travel measures it
    /// (`getLargestRectangleAround` of portal blocks, 21 each way), and
    /// whether it stands along X.
    pub fn portal_rect(&self, p: IVec3) -> (PortalRect, bool) {
        let along_x = self.portal_along_x(p);
        (largest_rectangle_around(p, along_x, |q| self.get_block(q) == Some(Block::NETHER_PORTAL)), along_x)
    }

    /// v1.0 gap Track 5: `PortalShape.findCollisionFreePosition` for an
    /// entity of `width` by `height` whose feet are at `bottom`: the
    /// nearest place to it, at most half its width across and one block
    /// up, where no block's collision box (each grown by half the entity's
    /// size) holds its centre; `bottom` itself if there is none or the
    /// entity is over 4 blocks either way.
    pub fn collision_free_position(&self, bottom: DVec3, width: f64, height: f64) -> DVec3 {
        if width > 4.0 || height > 4.0 {
            return bottom;
        }
        let half = DVec3::new(width / 2.0, height / 2.0, width / 2.0);
        let center = bottom + DVec3::Y * half.y;
        let e = 1.0e-6;
        let allowed =
            (center - DVec3::new(half.x + e, e, half.z + e), center + DVec3::new(half.x + e, 1.0 + e, half.z + e));
        let size = DVec3::new(width, height, width);
        let mut colliders = Vec::new();
        crate::physics::colliders(self, allowed.0 - size, allowed.1 + size, &mut colliders);
        let obstacles: Vec<(DVec3, DVec3)> = colliders
            .iter()
            .filter(|c| BORDER.contains_box(c.min, c.max))
            .map(|c| (c.min - half, c.max + half))
            .collect();
        // The free space is the allowed box less the obstacles, cut at
        // every edge (as `Shapes.join` cuts it into cells); the nearest
        // point of a free cell to the centre.
        let cuts = |axis: usize| {
            let (lo, hi) = (allowed.0[axis], allowed.1[axis]);
            let mut v = vec![lo, hi];
            for (a, b) in &obstacles {
                v.extend([a[axis], b[axis]].into_iter().filter(|&c| c > lo && c < hi));
            }
            v.sort_by(f64::total_cmp);
            v.dedup();
            v
        };
        let (xs, ys, zs) = (cuts(0), cuts(1), cuts(2));
        let mut best: Option<(f64, DVec3)> = None;
        for x in xs.windows(2) {
            for y in ys.windows(2) {
                for z in zs.windows(2) {
                    let (lo, hi) = (DVec3::new(x[0], y[0], z[0]), DVec3::new(x[1], y[1], z[1]));
                    let mid = (lo + hi) / 2.0;
                    if obstacles.iter().any(|(a, b)| mid.cmpgt(*a).all() && mid.cmplt(*b).all()) {
                        continue;
                    }
                    let p = center.clamp(lo, hi);
                    let d = p.distance_squared(center);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, p));
                    }
                }
            }
        }
        best.map_or(bottom, |(_, p)| p - DVec3::Y * half.y)
    }

    /// `canPortalReplaceBlock`: replaceable and holding no fluid.
    fn portal_can_replace(&self, p: IVec3) -> bool {
        self.get_block(p).is_some_and(|b| b.is_replaceable() && !b.is_fluid() && !b.holds_water())
    }

    /// `canHostFrame`: the frame's footprint (four along `dir`, `offset`
    /// to the side) has solid ground under it and five replaceable cells up.
    fn portal_can_host(&self, at: IVec3, dir: IVec3, side: IVec3, offset: i32) -> bool {
        (-1..3).all(|w| {
            (-1..4).all(|h| {
                let p = at + dir * w + side * offset + IVec3::Y * h;
                if h < 0 { self.get_block(p).is_some_and(|b| b.is_solid()) } else { self.portal_can_replace(p) }
            })
        })
    }

    /// The highest y a portal's frame may reach (`getMaxY`, or the
    /// Nether's logical height of 128).
    fn portal_max_y(&self) -> i32 {
        match self.generator.dimension {
            super::terrain::Dimension::Nether => (self.max_y() - 1).min(127),
            _ => self.max_y() - 1,
        }
    }

    /// v0.16 Track G: `PortalForcer.createPortal` at `origin`, along X or
    /// Z as the portal travelled through. Columns spiral out to 16 blocks
    /// (`BlockPos.spiralAround`, east then south), skipping a column that
    /// is (or whose next block along the portal is) outside the world
    /// border; in each, from the `MOTION_BLOCKING` height down, the bottom
    /// of every run of replaceable cells (a single cell or at least three
    /// high) that can hold the frame is a site; the nearest with room on
    /// both sides wins, else the nearest with room only for the frame.
    /// With none, an obsidian ledge is built at y 70 up to 9 below the
    /// top, clamped to the border; with no room for that, no portal
    /// (`None`). Returns the new portal: its lowest interior cell on the
    /// negative side, 2 wide and 3 high.
    pub fn create_portal(&mut self, origin: IVec3, along_x: bool) -> Option<PortalRect> {
        let dir = if along_x { IVec3::X } else { IVec3::Z };
        // `Direction.getClockWise`: east turns south, south turns west.
        let side = if along_x { IVec3::Z } else { IVec3::NEG_X };
        let max_placeable = self.portal_max_y();
        let min_y = self.min_y();
        let mut full: Option<(i64, IVec3)> = None;
        let mut partial: Option<(i64, IVec3)> = None;
        for column in spiral(origin, CREATE_RADIUS) {
            if !BORDER.contains_block(column) || !BORDER.contains_block(column + dir) {
                continue;
            }
            let height = max_placeable.min(self.motion_blocking_height(column.x, column.z));
            let mut y = height;
            while y >= min_y {
                let here = IVec3::new(column.x, y, column.z);
                if self.portal_can_replace(here) {
                    let first_empty = y;
                    while y > min_y && self.portal_can_replace(IVec3::new(column.x, y - 1, column.z)) {
                        y -= 1;
                    }
                    let gap = first_empty - y;
                    if y + 4 <= max_placeable && (gap <= 0 || gap >= 3) {
                        let at = IVec3::new(column.x, y, column.z);
                        if self.portal_can_host(at, dir, side, 0) {
                            let d = (at - origin).as_i64vec3().length_squared();
                            if self.portal_can_host(at, dir, side, -1)
                                && self.portal_can_host(at, dir, side, 1)
                                && full.is_none_or(|(fd, _)| fd > d)
                            {
                                full = Some((d, at));
                            }
                            if full.is_none() && partial.is_none_or(|(pd, _)| pd > d) {
                                partial = Some((d, at));
                            }
                        }
                    }
                }
                y -= 1;
            }
        }
        let at = match full.or(partial) {
            Some((_, at)) => at,
            None => {
                let low = (min_y + 1).max(LEDGE_MIN_Y);
                let high = max_placeable - LEDGE_BELOW_TOP;
                if high < low {
                    return None;
                }
                let at = BORDER.clamp((IVec3::new(origin.x, origin.y.clamp(low, high), origin.z) - dir).as_dvec3());
                for b in -1..2 {
                    for w in 0..2 {
                        for h in -1..3 {
                            let block = if h < 0 { Block::OBSIDIAN } else { Block::AIR };
                            self.set_block(at + dir * w + side * b + IVec3::Y * h, block);
                        }
                    }
                }
                at
            }
        };
        for w in -1..3 {
            for h in -1..4 {
                if w == -1 || w == 2 || h == -1 || h == 3 {
                    self.set_block(at + dir * w + IVec3::Y * h, Block::OBSIDIAN);
                }
            }
        }
        for w in 0..2 {
            for h in 0..3 {
                self.edit(at + dir * w + IVec3::Y * h, Block::NETHER_PORTAL, true);
            }
        }
        Some(PortalRect { min: at, width: 2, height: 3 })
    }

    /// `Heightmap.Types.MOTION_BLOCKING`: one above the highest block that
    /// blocks motion or holds a fluid in the column.
    fn motion_blocking_height(&self, x: i32, z: i32) -> i32 {
        let mut y = self.max_y() - 1;
        while y >= self.min_y() {
            if self.get_block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid() || b.is_fluid() || b.holds_water()) {
                return y + 1;
            }
            y -= 1;
        }
        self.min_y()
    }
}

/// `BlockPos.spiralAround(center, radius, EAST, SOUTH)`: the centre, then
/// legs east, south, west and north, each pair one longer.
pub fn spiral(center: IVec3, radius: i32) -> Vec<IVec3> {
    let dirs = [IVec3::X, IVec3::Z, IVec3::NEG_X, IVec3::NEG_Z];
    let legs = 4 * radius;
    let (mut leg, mut leg_size, mut leg_index) = (-1i32, 0i32, 0i32);
    let mut last = center + IVec3::Z;
    let mut out = Vec::with_capacity(((2 * radius + 1) * (2 * radius + 1)) as usize);
    loop {
        let cursor = last + dirs[((leg + 4) % 4) as usize];
        last = cursor;
        if leg_index >= leg_size {
            if leg >= legs {
                return out;
            }
            leg += 1;
            leg_index = 0;
            leg_size = leg / 2 + 1;
        }
        leg_index += 1;
        out.push(cursor);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use glam::DVec3;

    use super::*;
    use crate::world::terrain::Generator;

    /// A loaded world with a hollow sky high above the terrain to build in.
    fn world() -> World {
        let mut world = World::new(Arc::new(Generator::new(3)), Default::default(), 2);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !(world.loaded_chunks() > 0 && world.pending_jobs() == 0) {
            world.update(DVec3::new(0.0, 230.0, 0.0));
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        world
    }

    fn frame(world: &mut World, at: IVec3, w: i32, h: i32) {
        for i in -1..=w {
            world.set_block(at + IVec3::new(i, -1, 0), Block::OBSIDIAN);
            world.set_block(at + IVec3::new(i, h, 0), Block::OBSIDIAN);
        }
        for y in 0..h {
            world.set_block(at + IVec3::new(-1, y, 0), Block::OBSIDIAN);
            world.set_block(at + IVec3::new(w, y, 0), Block::OBSIDIAN);
        }
    }

    #[test]
    fn frames_light_and_portals_break_with_them() {
        let mut world = world();
        let at = IVec3::new(4, 220, 4);
        frame(&mut world, at, 2, 3);
        assert!(world.light_portal(at + IVec3::new(1, 2, 0)), "lit from the top corner");
        for (dx, dy) in [(0, 0), (1, 0), (0, 2), (1, 2)] {
            assert_eq!(world.get_block(at + IVec3::new(dx, dy, 0)), Some(Block::NETHER_PORTAL));
        }
        // The nearest portal block, and the portal around it.
        assert_eq!(world.find_portal(at + IVec3::new(-5, 3, 2), 8), Some(at + IVec3::new(0, 2, 0)));
        assert_eq!(world.portal_rect(at + IVec3::new(0, 2, 0)), (PortalRect { min: at, width: 2, height: 3 }, true));
        // Knocking out one side of the frame takes the whole portal down.
        world.set_block(at + IVec3::new(2, 1, 0), Block::AIR);
        for dy in 0..3 {
            assert_eq!(world.get_block(at + IVec3::new(0, dy, 0)), Some(Block::AIR));
        }

        // Unfinished or wrongly sized frames don't light.
        let at = IVec3::new(-8, 220, -8);
        frame(&mut world, at, 2, 3);
        world.set_block(at + IVec3::new(-1, 1, 0), Block::AIR);
        assert!(!world.light_portal(at));
        let at = IVec3::new(-8, 220, 8);
        frame(&mut world, at, 1, 3);
        assert!(!world.light_portal(at));
    }

    #[test]
    fn missing_portals_are_built_with_somewhere_to_stand() {
        let mut world = world();
        // High in the sky the nearest site is on the ground below, within
        // 16 blocks across.
        let rect = world.create_portal(IVec3::new(2, 230, 2), true).expect("room for a portal");
        let at = rect.min;
        assert_eq!((rect.width, rect.height), (2, 3));
        assert!((at.x - 2).abs() <= 16 && (at.z - 2).abs() <= 16, "{at}");
        assert_eq!(world.get_block(at), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at + IVec3::new(1, 2, 0)), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at + IVec3::new(-1, 3, 0)), Some(Block::OBSIDIAN));
        assert_eq!(world.get_block(at - IVec3::Y), Some(Block::OBSIDIAN));
        assert!(world.get_block(at + IVec3::new(0, -1, 1)).unwrap().is_solid(), "ground to step onto");
        assert_eq!(world.find_portal(at + IVec3::new(0, 2, 0), 4), Some(at + IVec3::new(0, 2, 0)));
        assert_eq!(world.portal_rect(at + IVec3::new(1, 1, 0)), (rect, true));
    }

    #[test]
    fn new_portals_pick_the_nearest_site_with_room_on_both_sides() {
        let mut world = world();
        // A floor at y 220 with room above it: the frame goes on it, at the
        // cell nearest the origin.
        for z in -8..=8 {
            for x in -8..=8 {
                world.set_block(IVec3::new(x, 219, z), Block::STONE);
            }
        }
        let at = world.create_portal(IVec3::new(0, 225, 0), true).unwrap().min;
        assert_eq!(at, IVec3::new(0, 220, 0));
        assert_eq!(world.get_block(at - IVec3::Y), Some(Block::OBSIDIAN));
        assert_eq!(world.get_block(at + IVec3::X), Some(Block::NETHER_PORTAL));
        // Along Z (an entry portal along Z) it turns.
        let at = world.create_portal(IVec3::new(4, 224, 4), false).unwrap().min;
        assert_eq!(world.get_block(at + IVec3::Z), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at - IVec3::Z), Some(Block::OBSIDIAN));
        // The nearer portal is found from either side.
        let found = world.find_portal(IVec3::new(5, 222, 7), 128).unwrap();
        assert_eq!(world.portal_rect(found), (PortalRect { min: at, width: 2, height: 3 }, false));
    }

    #[test]
    fn a_saved_unloaded_portal_is_found() {
        let mut world = world();
        let at = IVec3::new(4, 220, 4);
        frame(&mut world, at, 2, 3);
        assert!(world.light_portal(at + IVec3::new(1, 2, 0)));
        // Unload the portal's chunk: it is now only in the saved sections,
        // which `get_block` doesn't read.
        world.remove_chunk(crate::world::chunk::chunk_of(at));
        assert_eq!(world.get_block(at), None);
        // From above, the nearest portal cell is in the top row (the
        // arrival waits for it to load, then measures the portal).
        assert_eq!(world.find_portal(at + IVec3::new(0, 6, 0), 16), Some(at + IVec3::new(0, 2, 0)));
    }

    /// `findCollisionFreePosition`: an arrival overlapping a wall moves
    /// clear of it (at most half its width across, one block up); one in
    /// the open stays put.
    #[test]
    fn arrivals_move_clear_of_blocks_like_java() {
        let mut world = world();
        let at = IVec3::new(4, 220, 4);
        frame(&mut world, at, 2, 3);
        assert!(world.light_portal(at + IVec3::new(1, 2, 0)));
        let rect = PortalRect { min: at, width: 2, height: 3 };
        let (w, h) = (f64::from(0.6f32), f64::from(1.8f32));
        let open = exit_position(rect, true, DVec3::new(0.5, 0.0, 0.0), w, h);
        assert_eq!(world.collision_free_position(open, w, h), open);
        // A wall just in front of the portal: an arrival 0.45 forward of the
        // portal's middle overlaps it and backs off to touch it.
        for x in -1..=2 {
            for y in 0..4 {
                world.set_block(at + IVec3::new(x, y, 1), Block::STONE);
            }
        }
        let pos = exit_position(rect, true, DVec3::new(0.5, 0.0, 0.45), w, h);
        let free = world.collision_free_position(pos, w, h);
        let want = at.as_dvec3() + DVec3::new(1.0, 0.0, 1.0 - w / 2.0);
        assert!((free - want).abs().max_element() < 1e-5, "{free} not {want}");
        // Too deep in the wall to back out: Java keeps the position.
        let deep = exit_position(rect, true, DVec3::new(0.5, 0.0, 0.8), w, h);
        assert_eq!(world.collision_free_position(deep, w, h), deep);
    }

    /// The world border: a new portal's fallback ledge is clamped inside it.
    #[test]
    fn new_portals_stay_inside_the_world_border() {
        let mut world = world();
        // Nothing is loaded out there, so there are no sites: the ledge.
        let rect = world.create_portal(IVec3::new(29_999_990, 80, 5), true).expect("a ledge");
        assert_eq!(rect.min, IVec3::new(29_999_983, 80, 5));
        let rect = world.create_portal(IVec3::new(-40_000_000, 400, -40_000_000), false).expect("a ledge");
        assert_eq!(rect.min, IVec3::new(-29_999_984, world.portal_max_y() - LEDGE_BELOW_TOP, -29_999_984));
    }

    #[test]
    fn the_portal_search_spirals_out_like_java() {
        let s = spiral(IVec3::ZERO, 1);
        assert_eq!(
            s,
            vec![
                IVec3::new(0, 0, 0),
                IVec3::new(1, 0, 0),
                IVec3::new(1, 0, 1),
                IVec3::new(0, 0, 1),
                IVec3::new(-1, 0, 1),
                IVec3::new(-1, 0, 0),
                IVec3::new(-1, 0, -1),
                IVec3::new(0, 0, -1),
                IVec3::new(1, 0, -1),
            ]
        );
        let mut all = spiral(IVec3::ZERO, 16);
        assert_eq!(all.len(), 33 * 33);
        all.sort_by_key(|p| (p.x, p.z));
        all.dedup();
        assert_eq!(all.len(), 33 * 33);
    }
}

#[cfg(test)]
#[path = "portal_java_tests.rs"]
mod java_tests;
