//! Java 26.3 sponge traversal: six connected steps, at most 64 water cells.
use super::{World, block::Block, overworld_blocks as ob, terrain::Dimension};
use glam::IVec3;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

impl World {
    pub(super) fn update_sponges(&mut self, changed: IVec3) {
        for p in std::iter::once(changed).chain(DIRECTIONS.map(|d| changed + d)) {
            match self.get_block(p) {
                Some(ob::WET_SPONGE) if self.generator.dimension == Dimension::Nether => {
                    self.edit(p, ob::SPONGE, true);
                }
                Some(ob::SPONGE) => {
                    self.absorb_water(p);
                }
                _ => {}
            }
        }
    }

    fn absorb_water(&mut self, origin: IVec3) {
        let mut queue = VecDeque::from([(origin, 0u8)]);
        let mut seen = FxHashSet::default();
        seen.insert(origin);
        let mut removed = Vec::with_capacity(64);
        while let Some((p, depth)) = queue.pop_front() {
            if depth == 6 {
                continue;
            }
            for d in DIRECTIONS {
                let q = p + d;
                if !seen.insert(q) {
                    continue;
                }
                let Some(b) = self.get_block(q).filter(|b| b.holds_water()) else { continue };
                // The current waterlogged catalogue consists of aquatic plants.
                // Keep this explicit so future solid waterlogging retains its block.
                if !b.is_fluid() && !matches!(b.0, 1960..=1964) {
                    continue;
                }
                if !b.is_fluid() {
                    self.spill_block(q, b);
                }
                self.edit(q, Block::AIR, true);
                removed.push(q);
                queue.push_back((q, depth + 1));
                if removed.len() == 64 {
                    break;
                }
            }
            if removed.len() == 64 {
                break;
            }
        }
        if !removed.is_empty() {
            self.edit(origin, ob::WET_SPONGE, true);
            for q in removed {
                self.wake_fluids(q);
                self.update_overworld_plants(q);
            }
        }
    }
}
