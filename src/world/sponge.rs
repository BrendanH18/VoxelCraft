//! Java 26.3 sponge traversal (`SpongeBlock.removeWaterBreadthFirstSearch`):
//! six connected steps, at most 64 accepted water cells.
use super::{World, block::Block, overworld_blocks as ob, terrain::Dimension};
use glam::IVec3;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

/// What the sponge does to one water-holding cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drain {
    /// `LiquidBlock`: source (bucket pickup) or flowing water becomes air.
    Liquid,
    /// Kelp and seagrass: dropped as items, then air.
    Plant,
    /// A `BucketPickup` waterlogged block keeps its block and loses its water.
    Pickup,
}

fn drain(b: Block) -> Drain {
    match b {
        b if b.is_water() => Drain::Liquid,
        b if super::waterlog::wet(b.0) => Drain::Pickup,
        _ => Drain::Plant,
    }
}

impl World {
    pub(super) fn update_sponges(&mut self, changed: IVec3) {
        // `WetSpongeBlock.onPlace`: a wet sponge dries only when it is placed
        // where water evaporates, not when a neighbour changes.
        if self.get_block(changed) == Some(ob::WET_SPONGE) && self.generator.dimension == Dimension::Nether {
            self.edit(changed, ob::SPONGE, true);
        }
        for p in std::iter::once(changed).chain(DIRECTIONS.map(|d| changed + d)) {
            if self.get_block(p) == Some(ob::SPONGE) {
                self.absorb_water(p);
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
                match drain(b) {
                    Drain::Liquid => {
                        self.edit(q, Block::AIR, true);
                    }
                    // `setBlockAndUpdate(pos, AIR)`: the plant's water goes too.
                    Drain::Plant => {
                        self.spill_block(q, b);
                        self.edit_exact(q, Block::AIR, true);
                    }
                    Drain::Pickup => {
                        self.drain_waterlogged(q);
                    }
                }
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
