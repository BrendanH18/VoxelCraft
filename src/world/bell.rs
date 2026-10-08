//! Bell use and redstone edges, delivered through the shared entity bridge.
use super::{World, block::Block};
use glam::IVec3;
impl World {
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
