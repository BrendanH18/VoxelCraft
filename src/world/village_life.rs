//! Loaded POIs and natural residents; generation is queried only when chunks arrive.
use super::{
    World,
    block::Block,
    chunk::{CHUNK_SIZE_I, ChunkData, local_of},
    fortress::Feature,
};
use crate::entity::villager::is_poi;
use glam::IVec3;
impl World {
    pub(super) fn track_village_poi(&mut self, p: IVec3, b: Block) {
        if is_poi(b) {
            self.village_pois.insert(p, b);
        } else {
            self.village_pois.remove(&p);
        }
    }
    pub(super) fn register_village_life(&mut self, cpos: IVec3, data: &ChunkData) {
        if let ChunkData::Uniform(b) = data
            && !is_poi(*b)
        {
            return;
        }
        let base = cpos * CHUNK_SIZE_I;
        for y in 0..CHUNK_SIZE_I {
            for z in 0..CHUNK_SIZE_I {
                for x in 0..CHUNK_SIZE_I {
                    let b = data.get(x as usize, y as usize, z as usize);
                    if is_poi(b) {
                        self.village_pois.insert(base + IVec3::new(x, y, z), b);
                    }
                }
            }
        }
        for (p, f) in self.generator.villages.features(&self.generator, cpos) {
            if let Feature::VillageHome(spawn) = f {
                let l = local_of(p);
                if data.get(l.x as usize, l.y as usize, l.z as usize) == Block::BED_HEAD {
                    self.village_homes.insert(p, spawn);
                }
            }
        }
    }
    pub fn visit_village_pois(&self, visit: &mut dyn FnMut(IVec3, Block)) {
        for (&p, &b) in &self.village_pois {
            if self.is_loaded(p) {
                visit(p, b)
            }
        }
    }
    pub fn visit_village_homes(&self, visit: &mut dyn FnMut(IVec3, IVec3)) {
        for (&p, &spawn) in &self.village_homes {
            if self.get_block(p) == Some(Block::BED_HEAD) && self.is_loaded(spawn) {
                visit(p, spawn)
            }
        }
    }
}
