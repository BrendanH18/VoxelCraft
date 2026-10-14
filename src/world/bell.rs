//! Bell use and redstone edges, delivered through the shared entity bridge.
use super::{World, block::Block};
use glam::IVec3;
impl World {
    /// Whether a click along `eye` + `dir` rings the bell at `pos`. Java's
    /// `BellBlock.isProperHit` rejects hits on the top or bottom face and on
    /// the beam above 0.8124 of the block. VoxelCraft has a single bell state
    /// (no facing or attachment), so the side-face axis rules are not modelled
    /// and the ray tests the whole cell.
    pub fn bell_hit_rings(&self, pos: IVec3, eye: glam::DVec3, dir: glam::DVec3) -> bool {
        let min = pos.as_dvec3();
        let Some((t, normal)) = crate::physics::ray_aabb_face(eye, dir, min, min + glam::DVec3::ONE) else {
            return false;
        };
        normal.y == 0 && eye.y + dir.y * t - min.y <= 0.812_399_983_406_066_9
    }

    pub fn ring_bell(&mut self, pos: IVec3) -> bool {
        if self.get_block(pos).is_none_or(|b| b.base() != Block::BELL) {
            return false;
        }
        if !self.bell_rings.contains(&pos) {
            self.bell_rings.push(pos);
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Entities, MobKind},
        world::{chunk::ChunkData, redstone_blocks as r, terrain::Generator},
    };
    use std::sync::Arc;
    #[test]
    fn only_side_hits_below_the_beam_ring_the_bell() {
        use glam::DVec3;
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        let p = IVec3::new(4, 145, 8);
        w.set_block(p, Block::BELL);
        let c = p.as_dvec3();
        // Level with the bell body from the west: rings (hit height 0.5).
        assert!(w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.5, 0.5), DVec3::X));
        // Just below the 0.8124 beam line: rings. Above it: passes.
        assert!(w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.81, 0.5), DVec3::X));
        assert!(!w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.82, 0.5), DVec3::X));
        // Top and bottom faces never ring.
        assert!(!w.bell_hit_rings(p, c + DVec3::new(0.5, 3.0, 0.5), DVec3::NEG_Y));
        assert!(!w.bell_hit_rings(p, c + DVec3::new(0.5, -2.0, 0.5), DVec3::Y));
        // A miss does not ring.
        assert!(!w.bell_hit_rings(p, c + DVec3::new(-2.0, 0.5, 3.5), DVec3::X));
    }

    #[test]
    fn use_and_redstone_rising_edges_send_residents_home() {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        let p = IVec3::new(4, 145, 8);
        w.set_block(p, Block::BELL);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Villager, p.as_dvec3() + glam::DVec3::X * 4.0);
        let home = p + IVec3::Z * 5;
        e.mobs[0].villager.as_mut().unwrap().home = Some(home);
        e.spawn(MobKind::Villager, p.as_dvec3() + glam::DVec3::X * 33.0);
        assert!(w.use_redstone(p));
        w.tick_automation_entities(&mut e);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().bell_hide, 15.0);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().bell_hide, 0.0);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().goal, Some(home.as_dvec3() + glam::DVec3::new(0.5, 0.6, 0.5)));
        w.set_block(p + IVec3::X, r::REDSTONE_BLOCK);
        w.tick_redstone();
        assert_eq!(w.bell_rings.len(), 1);
        w.bell_rings.clear();
        for _ in 0..5 {
            w.tick_redstone();
        }
        assert!(w.bell_rings.is_empty());
        w.set_block(p + IVec3::X, Block::AIR);
        w.tick_redstone();
        w.set_block(p + IVec3::X, r::REDSTONE_BLOCK);
        w.tick_redstone();
        assert_eq!(w.bell_rings.len(), 1);
    }
}
