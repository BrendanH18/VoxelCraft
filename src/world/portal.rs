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

    /// The lowest portal block nearest `near` within `radius` blocks
    /// horizontally (loaded chunks only).
    pub fn find_portal(&self, near: IVec3, radius: i32, (min_y, max_y): (i32, i32)) -> Option<IVec3> {
        let mut best: Option<(i32, IVec3)> = None;
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                for y in min_y..=max_y {
                    let p = IVec3::new(near.x + dx, y, near.z + dz);
                    if self.is(p, Block::NETHER_PORTAL) && !self.is(p - IVec3::Y, Block::NETHER_PORTAL) {
                        let d = (p - near).length_squared();
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, p));
                        }
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// Builds a lit 2x3 portal along X near `near`, preferring a dry spot
    /// with ground to stand on in front and behind, and otherwise carving a
    /// space on an obsidian ledge. Returns its lowest interior cell on the
    /// -X side.
    pub fn build_portal(&mut self, near: IVec3, (min_y, max_y): (i32, i32)) -> IVec3 {
        let at = self.portal_site(near, 16, (min_y, max_y)).unwrap_or_else(|| {
            let p = IVec3::new(near.x, near.y.clamp(min_y, max_y), near.z);
            for dz in -1..=1 {
                for dx in -1..=2 {
                    for dy in 0..4 {
                        self.set_block(p + IVec3::new(dx, dy, dz), Block::AIR);
                    }
                    self.set_block(p + IVec3::new(dx, -1, dz), Block::OBSIDIAN);
                }
            }
            p
        });
        for dx in -1..=2 {
            self.set_block(at + IVec3::new(dx, -1, 0), Block::OBSIDIAN);
            self.set_block(at + IVec3::new(dx, 3, 0), Block::OBSIDIAN);
        }
        for dy in 0..3 {
            self.set_block(at + IVec3::new(-1, dy, 0), Block::OBSIDIAN);
            self.set_block(at + IVec3::new(2, dy, 0), Block::OBSIDIAN);
        }
        self.light_portal(at);
        at
    }

    /// A spot for a new portal: the 4x4x3 box from `p - (1, 0, 1)` is
    /// open (no fluids) and there's solid ground under the interior and in
    /// front of and behind it.
    fn portal_site(&self, near: IVec3, radius: i32, (min_y, max_y): (i32, i32)) -> Option<IVec3> {
        let fits = |p: IVec3| {
            let ground = (0..=1)
                .all(|dx| (-1..=1).all(|dz| self.get_block(p + IVec3::new(dx, -1, dz)).is_some_and(|b| b.is_opaque())));
            ground
                && (-1..=2)
                    .all(|dx| (-1..=1).all(|dz| (0..4).all(|dy| self.is(p + IVec3::new(dx, dy, dz), Block::AIR))))
        };
        let mut best: Option<(i32, IVec3)> = None;
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                for y in min_y..=max_y {
                    let p = IVec3::new(near.x + dx, y, near.z + dz);
                    let score = (y - near.y).abs() * 2 + dx.abs() + dz.abs();
                    if best.is_none_or(|(s, _)| score < s) && fits(p) {
                        best = Some((score, p));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
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
        assert_eq!(world.find_portal(at + IVec3::new(-5, 3, 2), 8, (200, 240)), Some(at));
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
        let at = world.build_portal(IVec3::new(2, 230, 2), (200, 240));
        assert_eq!(world.get_block(at), Some(Block::NETHER_PORTAL));
        assert_eq!(world.get_block(at + IVec3::new(1, 2, 0)), Some(Block::NETHER_PORTAL));
        assert!(world.get_block(at + IVec3::new(0, -1, 1)).unwrap().is_opaque(), "a ledge to step onto");
        assert_eq!(world.find_portal(at, 4, (200, 240)), Some(at));
    }
}
