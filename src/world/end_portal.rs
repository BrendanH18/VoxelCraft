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
    // The frame is on one side of the ring: its centre is within two cells.
    (-2..=2)
        .flat_map(|dz| (-2..=2).map(move |dx| frame + IVec3::new(dx, 0, dz)))
        .filter(|c| ring(*c).iter().any(|&(p, _)| p == frame))
        .find(|&c| {
            ring(c).iter().all(|&(p, facing)| {
                get(p).is_some_and(|b| has_eye(b) && b.oriented().is_some_and(|(_, f)| f == facing))
            })
        })
}

impl World {
    /// Puts an eye of ender in the empty frame at `pos`. Returns `None` if
    /// there's no empty frame there, else whether this opened a portal.
    pub fn insert_eye(&mut self, pos: IVec3) -> Option<bool> {
        let frame = self.get_block(pos).filter(|b| b.base() == Block::END_PORTAL_FRAME && !has_eye(*b))?;
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
    fn ring_frames_face_the_middle() {
        let center = IVec3::new(0, 0, 0);
        for (p, f) in ring(center) {
            let inside = p + f.offset();
            assert!(inside.x.abs() <= 1 && inside.z.abs() <= 1, "{p} facing {f:?}");
        }
    }
}
