//! Nether portals: lighting an obsidian frame, breaking a portal that lost
//! its frame, and finding or building the portal at the far end of a trip.
//!
//! A frame is a rectangle of obsidian around an empty interior 2..=21 wide
//! and 3..=21 tall, standing along X or Z (corners optional, as in
//! Minecraft). Portal blocks fill the interior.

use glam::IVec3;

use super::World;
use super::block::Block;

const MIN_W: i32 = 2;
const MAX_W: i32 = 21;
const MIN_H: i32 = 3;
const MAX_H: i32 = 21;
const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

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
    /// height, the nearest to `near` (Java's `distSqr`, lower first on a
    /// tie). Chunks that are neither loaded nor saved were never changed,
    /// so hold no portal. Returns the lowest portal block under the one
    /// found, where the traveller stands.
    pub fn find_portal(&self, near: IVec3, radius: i32) -> Option<IVec3> {
        let mut best: Option<(i64, i32, IVec3)> = None;
        let (min_y, max_y) = (self.min_y(), self.max_y());
        for cz in (near.z - radius).div_euclid(16)..=(near.z + radius).div_euclid(16) {
            for cx in (near.x - radius).div_euclid(16)..=(near.x + radius).div_euclid(16) {
                for cy in min_y.div_euclid(16)..=(max_y - 1).div_euclid(16) {
                    let Some(data) = self.section_data(IVec3::new(cx, cy, cz)) else { continue };
                    if !data.contains(Block::NETHER_PORTAL) {
                        continue;
                    }
                    for i in 0..super::chunk::CHUNK_VOLUME {
                        let (x, y, z) = (i % 16, (i / 16) % 16, i / 256);
                        if data.get(x, y, z) != Block::NETHER_PORTAL {
                            continue;
                        }
                        let p = IVec3::new(cx * 16 + x as i32, cy * 16 + y as i32, cz * 16 + z as i32);
                        if (p.x - near.x).abs() > radius || (p.z - near.z).abs() > radius {
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
        let (_, _, mut p) = best?;
        while self.get_block(p - IVec3::Y) == Some(Block::NETHER_PORTAL) {
            p.y -= 1;
        }
        Some(p)
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
    /// (`BlockPos.spiralAround`, east then south); in each, from the
    /// `MOTION_BLOCKING` height down, the bottom of every run of
    /// replaceable cells (a single cell or at least three high) that can
    /// hold the frame is a site; the nearest with room on both sides wins,
    /// else the nearest with room only for the frame. With none, an
    /// obsidian ledge is built at y 70 up to 9 below the top. Returns the
    /// lowest interior cell on the frame's negative side.
    pub fn create_portal(&mut self, origin: IVec3, along_x: bool) -> IVec3 {
        let dir = if along_x { IVec3::X } else { IVec3::Z };
        // `Direction.getClockWise`: east turns south, south turns west.
        let side = if along_x { IVec3::Z } else { IVec3::NEG_X };
        let max_placeable = self.portal_max_y();
        let min_y = self.min_y();
        let mut full: Option<(i64, IVec3)> = None;
        let mut partial: Option<(i64, IVec3)> = None;
        for column in spiral(origin, 16) {
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
                let low = (min_y + 1).max(70);
                let high = max_placeable - 9;
                let at = IVec3::new(origin.x, origin.y.clamp(low, high.max(low)), origin.z) - dir;
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
        at
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
        assert_eq!(world.find_portal(at + IVec3::new(-5, 3, 2), 8), Some(at));
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
        let at = world.create_portal(IVec3::new(2, 230, 2), true);
        assert!((at.x - 2).abs() <= 16 && (at.z - 2).abs() <= 16, "{at}");
        assert_eq!(world.get_block(at), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at + IVec3::new(1, 2, 0)), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at + IVec3::new(-1, 3, 0)), Some(Block::OBSIDIAN));
        assert_eq!(world.get_block(at - IVec3::Y), Some(Block::OBSIDIAN));
        assert!(world.get_block(at + IVec3::new(0, -1, 1)).unwrap().is_solid(), "ground to step onto");
        assert_eq!(world.find_portal(at + IVec3::new(0, 2, 0), 4), Some(at));
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
        let at = world.create_portal(IVec3::new(0, 225, 0), true);
        assert_eq!(at, IVec3::new(0, 220, 0));
        assert_eq!(world.get_block(at - IVec3::Y), Some(Block::OBSIDIAN));
        assert_eq!(world.get_block(at + IVec3::X), Some(Block::NETHER_PORTAL));
        // Along Z (an entry portal along Z) it turns.
        let at = world.create_portal(IVec3::new(4, 224, 4), false);
        assert_eq!(world.get_block(at + IVec3::Z), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at - IVec3::Z), Some(Block::OBSIDIAN));
        // The nearer portal is found from either side.
        assert_eq!(world.find_portal(IVec3::new(5, 222, 7), 128), Some(at + IVec3::Z));
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
