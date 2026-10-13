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
    /// The engine's only such states are live coral plants and fans.
    Coral,
    /// Not drained: the traversal does not continue through it.
    Skip,
}

fn drain(b: Block) -> Drain {
    match b {
        b if b.is_water() => Drain::Liquid,
        b if matches!(b.0, 1960..=1964) => Drain::Plant,
        b if matches!(b.0, 1990..=1994 | 2000..=2004) => Drain::Coral,
        // Sea pickles are `BucketPickup` in Java, but VoxelCraft has no
        // unwaterlogged pickle state to leave behind; they keep their water.
        _ => Drain::Skip,
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
        let mut coral = Vec::new();
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
                    Drain::Skip => continue,
                    Drain::Liquid => {
                        self.edit(q, Block::AIR, true);
                    }
                    Drain::Plant => {
                        self.spill_block(q, b);
                        self.edit(q, Block::AIR, true);
                    }
                    Drain::Coral => coral.push((q, b)),
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
        // A drained live coral is left without water. VoxelCraft has no dry
        // live coral state, so it takes Java's eventual result: it dies unless
        // water still touches it (Java waits 60-100 ticks before dying).
        let drained: FxHashSet<IVec3> = coral.iter().map(|&(q, _)| q).collect();
        for &(q, b) in &coral {
            let wet = DIRECTIONS
                .iter()
                .any(|&d| !drained.contains(&(q + d)) && self.get_block(q + d).is_some_and(Block::holds_water));
            if !wet && let Some((kind, colour, _)) = ob::coral_of(b) {
                self.edit(q, ob::coral(kind, colour, true), true);
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
