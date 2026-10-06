//! End portals: putting an eye of ender in a frame, and opening the portal
//! once twelve frames with eyes ring a 3x3 hole, all facing in (Java's
//! `EndPortalFrameBlock` pattern).

use glam::IVec3;

use super::World;
use super::block::{Block, Facing};

/// The twelve frame cells around the portal centred on `center`, each
/// with the way it must face (toward the middle).
pub fn ring(center: IVec3) -> [(IVec3, Facing); 12] {
    let mut out = [(IVec3::ZERO, Facing::South); 12];
    for (i, d) in (-1..=1).enumerate() {
        out[i * 4] = (center + IVec3::new(d, 0, -2), Facing::South);
        out[i * 4 + 1] = (center + IVec3::new(d, 0, 2), Facing::North);
        out[i * 4 + 2] = (center + IVec3::new(-2, 0, d), Facing::East);
        out[i * 4 + 3] = (center + IVec3::new(2, 0, d), Facing::West);
    }
    out
}

/// Whether `b` is an End portal frame holding an eye.
pub fn has_eye(b: Block) -> bool {
    (204..=207).contains(&b.0)
}

/// The centre of a complete ring of eyed frames that includes the frame at
/// `frame`, if there is one.
pub fn find_portal(frame: IVec3, get: impl Fn(IVec3) -> Option<Block>) -> Option<IVec3> {
    centers(frame).find(|&c| ring(c).iter().all(|&(p, facing)| get(p).is_some_and(|b| fits(b, facing, false))))
}

/// Whether `b` is a frame facing `facing`, with an eye unless `eyeless_ok`.
fn fits(b: Block, facing: Facing, eyeless_ok: bool) -> bool {
    b.base() == Block::END_PORTAL_FRAME && (eyeless_ok || has_eye(b)) && b.oriented().is_some_and(|(_, f)| f == facing)
}

/// Centres of the rings the frame at `frame` could be part of.
fn centers(frame: IVec3) -> impl Iterator<Item = IVec3> {
    // The frame is on one side of the ring: its centre is within two cells.
    (-2..=2)
        .flat_map(move |dz| (-2..=2).map(move |dx| frame + IVec3::new(dx, 0, dz)))
        .filter(move |c| ring(*c).iter().any(|&(p, _)| p == frame))
}

/// Whether every ring an eye in `frame` could complete is fully loaded, so
/// [`find_portal`] sees it. Rings that a loaded cell already rules out (a
/// missing eye, wrong block or facing) don't count.
pub fn rings_loaded(frame: IVec3, get: impl Fn(IVec3) -> Option<Block>) -> bool {
    centers(frame).all(|c| {
        let cells = ring(c).map(|(p, facing)| (get(p), facing, p == frame));
        // The frame getting the eye only needs to face the right way.
        let possible = cells.iter().all(|&(b, facing, is_frame)| b.is_none_or(|b| fits(b, facing, is_frame)));
        !possible || cells.iter().all(|(b, ..)| b.is_some())
    })
}

impl World {
    /// Builds an End gateway centred on `origin`.
    pub fn build_gateway(&mut self, origin: IVec3) {
        for dy in -2..=2 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let p = origin + IVec3::new(dx, dy, dz);
                    if let Some(b) = super::end::gateway_block(origin, p)
                        && self.get_block(p) != Some(b)
                    {
                        self.set_block(p, b);
                    }
                }
            }
        }
    }

    /// A small end stone island hanging from `top` (Java's `EndIslandFeature`),
    /// for gateways that lead out over the void.
    pub fn build_end_island(&mut self, top: IVec3) {
        let mut radius = (self.roll() % 3) as f32 + 4.0;
        let mut y = 0;
        while radius > 0.5 {
            let r = radius.ceil() as i32;
            for dz in -r..=r {
                for dx in -r..=r {
                    if ((dx * dx + dz * dz) as f32) <= (radius + 1.0) * (radius + 1.0) {
                        self.set_block(top + IVec3::new(dx, y, dz), Block::END_STONE);
                    }
                }
            }
            radius -= (self.roll() % 2) as f32 + 0.5;
            y -= 1;
        }
    }

    /// Where someone coming out of the gateway at `gateway` stands: on the
    /// highest full block (not bedrock) within five of it, like Java's
    /// `findExitPosition`, or on the gateway's cap if there's none.
    pub fn gateway_arrival(&self, gateway: IVec3) -> glam::DVec3 {
        let from = gateway + IVec3::Y * 2;
        let mut best: Option<IVec3> = None;
        for dz in -5..=5 {
            for dx in -5..=5 {
                if (dx, dz) == (0, 0) {
                    continue;
                }
                let floor = best.map_or(0, |b| b.y);
                for y in (floor + 1..super::chunk::WORLD_HEIGHT).rev() {
                    let p = IVec3::new(from.x + dx, y, from.z + dz);
                    let b = self.get_block(p).unwrap_or(Block::AIR);
                    if b.is_opaque() && b != Block::BEDROCK {
                        best = Some(p);
                        break;
                    }
                }
            }
        }
        let stand = best.unwrap_or(from) + IVec3::Y;
        stand.as_dvec3() + glam::DVec3::new(0.5, 0.0, 0.5)
    }

    /// Hitting or using the dragon egg makes it jump to a random empty cell
    /// up to 15 blocks away (Java's `DragonEggBlock.teleport`). Returns
    /// where it went.
    pub fn teleport_egg(&mut self, pos: IVec3) -> Option<IVec3> {
        if self.get_block(pos) != Some(Block::DRAGON_EGG) {
            return None;
        }
        for _ in 0..1000 {
            let mut d = || (self.roll() % 16) as i32 - (self.roll() % 16) as i32;
            let (x, z) = (d(), d());
            let y = (self.roll() % 8) as i32 - (self.roll() % 8) as i32;
            let to = pos + IVec3::new(x, y, z);
            if to != pos && self.get_block(to) == Some(Block::AIR) {
                self.set_block(pos, Block::AIR);
                self.set_block(to, Block::DRAGON_EGG);
                return Some(to);
            }
        }
        None
    }

    /// Opens the exit portal in the End's podium once the dragon is dead,
    /// with the dragon egg on its pillar after the first kill.
    pub fn open_exit_portal(&mut self, egg: bool) {
        let Some(origin) = self.generator.end().map(|e| e.podium()) else { return };
        for dy in -1..=3 {
            for dz in -4..=4 {
                for dx in -4..=4 {
                    let p = origin + IVec3::new(dx, dy, dz);
                    if let Some(b) = super::end::podium_block(origin, p, true)
                        && self.get_block(p) != Some(b)
                    {
                        self.set_block(p, b);
                    }
                }
            }
        }
        if egg {
            self.set_block(origin + IVec3::Y * 4, Block::DRAGON_EGG);
        }
    }

    /// Puts an eye of ender in the empty frame at `pos`. Returns `None` if
    /// there's no empty frame there (or part of its ring hasn't loaded, so
    /// the last eye can't be spent without opening the portal), else
    /// whether this opened a portal.
    pub fn insert_eye(&mut self, pos: IVec3) -> Option<bool> {
        let frame = self.get_block(pos).filter(|b| b.base() == Block::END_PORTAL_FRAME && !has_eye(*b))?;
        if !rings_loaded(pos, |p| self.get_block(p)) {
            return None;
        }
        self.set_block(pos, Block(frame.0 + 4));
        let Some(center) = find_portal(pos, |p| self.get_block(p)) else { return Some(false) };
        for dz in -1..=1 {
            for dx in -1..=1 {
                self.set_block(center + IVec3::new(dx, 0, dz), Block::END_PORTAL);
            }
        }
        Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;

    fn eyed(facing: Facing) -> Block {
        Block(Block::END_PORTAL_FRAME.0 + 4).with_facing(facing)
    }

    #[test]
    fn a_full_inward_ring_opens_from_any_frame() {
        let center = IVec3::new(10, 30, -4);
        let mut blocks: FxHashMap<IVec3, Block> = ring(center).iter().map(|&(p, f)| (p, eyed(f))).collect();
        for &(p, _) in &ring(center) {
            assert_eq!(find_portal(p, |q| blocks.get(&q).copied()), Some(center));
        }
        // Not from somewhere that isn't on the ring.
        assert_eq!(find_portal(center, |q| blocks.get(&q).copied()), None);

        // One frame turned outward, or missing its eye, keeps it shut.
        let (p, f) = ring(center)[5];
        blocks.insert(p, eyed(f.opposite()));
        assert_eq!(find_portal(ring(center)[0].0, |q| blocks.get(&q).copied()), None);
        blocks.insert(p, Block::END_PORTAL_FRAME.with_facing(f));
        assert_eq!(find_portal(ring(center)[0].0, |q| blocks.get(&q).copied()), None);
    }

    #[test]
    fn unloaded_ring_cells_hold_off_the_last_eye() {
        let center = IVec3::new(0, 64, 0);
        let (frame, facing) = ring(center)[0];
        // Eleven eyed frames and an empty one, all facing in.
        let mut blocks: FxHashMap<IVec3, Block> = ring(center).iter().map(|&(p, f)| (p, eyed(f))).collect();
        blocks.insert(frame, Block::END_PORTAL_FRAME.with_facing(facing));
        let get = |q: IVec3| Some(blocks.get(&q).copied().unwrap_or(Block::AIR));
        assert!(rings_loaded(frame, get));

        // Part of the ring unloaded: wait for it.
        let far = ring(center)[7].0;
        assert!(!rings_loaded(frame, |q| if q == far { None } else { get(q) }));

        // Another candidate ring crossing unloaded cells doesn't matter
        // once a loaded cell rules it out (here, plain air).
        let other = center + IVec3::new(-1, 0, 0);
        assert!(ring(other).iter().any(|&(p, _)| p == frame));
        let unloaded = ring(other).iter().map(|&(p, _)| p).find(|p| p.x == -3).unwrap();
        assert!(rings_loaded(frame, |q| if q == unloaded { None } else { get(q) }));
    }

    #[test]
    fn ring_frames_face_the_middle() {
        let center = IVec3::new(0, 0, 0);
        for (p, f) in ring(center) {
            let inside = p + f.offset();
            assert!(inside.x.abs() <= 1 && inside.z.abs() <= 1, "{p} facing {f:?}");
        }
    }
}
