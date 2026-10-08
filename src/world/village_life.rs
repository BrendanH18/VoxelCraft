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
        if may_hold_poi(data) {
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

/// Whether a chunk's distinct states include a POI, so most chunks skip the
/// per-block scan. Paletted chunks check their palette and byte chunks the
/// set of bytes present; dense chunks are always scanned.
fn may_hold_poi(data: &ChunkData) -> bool {
    match data {
        ChunkData::Uniform(b) => is_poi(*b),
        ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|&b| is_poi(b)),
        ChunkData::Bytes(bytes) => {
            let mut present = [false; 256];
            for &v in bytes.iter() {
                present[v as usize] = true;
            }
            present.iter().enumerate().any(|(id, &p)| p && is_poi(Block(id as u16)))
        }
        ChunkData::Dense(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poi_precheck_skips_only_chunks_without_poi_states() {
        assert!(!may_hold_poi(&ChunkData::Uniform(Block::STONE)));
        let mut blocks = ChunkData::new_dense(Block::STONE);
        blocks[7] = Block::DIRT;
        let plain = ChunkData::from_dense(blocks.clone());
        assert!(!may_hold_poi(&plain));
        blocks[9] = Block::BELL;
        let village = ChunkData::from_dense(blocks.clone());
        assert!(may_hold_poi(&village));
        blocks[9] = Block::BED_HEAD;
        assert!(may_hold_poi(&ChunkData::from_dense(blocks)));
    }
}
