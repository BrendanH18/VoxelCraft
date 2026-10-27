//! Falling stalactites (`PointedDripstoneBlock.spawnFallingStalactite`):
//! pointed dripstone hanging from nothing falls as falling blocks, the tip
//! hurting what it lands on, and breaks into items where it lands.

use glam::{DVec3, IVec3};

use crate::world::{World, block::Block, falling::FallingBlock, overworld_blocks as ob};

/// Java's `fallDamageMax` for a stalactite.
const MAX_DAMAGE: f32 = 40.0;

/// The thickness of a downward pointed dripstone.
fn stalactite(b: Block) -> Option<ob::Thickness> {
    match ob::dripstone_of(b.canonical()) {
        Some((true, t)) => Some(t),
        _ => None,
    }
}

/// `FallingBlockEntity.causeFallDamage`: `min(floor(ceil(distance - 1) *
/// per_block), max)`.
pub fn fall_damage(distance: f64, per_block: f32) -> f32 {
    let blocks = (distance - 1.0).ceil().max(0.0) as f32;
    (blocks * per_block).floor().min(MAX_DAMAGE)
}

impl World {
    /// `spawnFallingStalactite`: every block from `top` down to the tip
    /// falls; the tip hurts by the stalactite's length (at least 6) per
    /// block fallen.
    pub(in crate::world) fn drop_stalactite(&mut self, top: IVec3) {
        let mut run = Vec::new();
        let mut at = top;
        while let Some(b) = self.get_block(at).filter(|&b| stalactite(b).is_some()) {
            let tip = stalactite(b) == Some(ob::Thickness::Tip);
            run.push((at, b));
            if tip || run.len() > 64 {
                break;
            }
            at -= IVec3::Y;
        }
        let length = run.len() as i32;
        for &(p, b) in &run {
            let mut f = FallingBlock::new(b.canonical(), p.as_dvec3(), 0.0);
            if stalactite(b) == Some(ob::Thickness::Tip) {
                f.hurt = length.max(6) as f32;
            }
            self.falling.push(f);
        }
        // From the tip up, so each block still hangs from the next as it goes.
        for &(p, _) in run.iter().rev() {
            self.edit(p, Block::AIR, true);
        }
    }

    /// A falling block landed at `p`: pointed dripstone breaks into its
    /// item instead of settling, and a stalactite's tip hurts whatever is
    /// in that cell. Returns whether it was dripstone.
    pub(in crate::world) fn stalactite_landed(&mut self, p: IVec3, fell: &FallingBlock) -> bool {
        if ob::dripstone_of(fell.block.canonical()).is_none() {
            return false;
        }
        if self.tile_drops {
            self.spill_block(p, fell.block);
        }
        if fell.hurt > 0.0 {
            let damage = fall_damage(fell.from_y - p.y as f64, fell.hurt);
            if damage > 0.0 {
                self.utilities.stalactite_hits.push((p, damage));
            }
        }
        true
    }

    pub(in crate::world) fn clear_stalactite_hits(&mut self) {
        self.utilities.stalactite_hits.clear();
    }

    /// The damage of a stalactite that landed this tick in a cell the box
    /// `min`..`max` overlaps (Java hurts every entity in the falling
    /// block's box as it lands), or 0.
    pub fn stalactite_damage(&self, min: DVec3, max: DVec3) -> f32 {
        self.utilities
            .stalactite_hits
            .iter()
            .filter(|(p, _)| {
                let (lo, hi) = (p.as_dvec3(), p.as_dvec3() + DVec3::ONE);
                lo.cmplt(max).all() && hi.cmpgt(min).all()
            })
            .map(|&(_, d)| d)
            .fold(0.0, f32::max)
    }

    /// Cells where stalactites landed this tick, with their damage.
    pub fn stalactite_hits(&self) -> &[(IVec3, f32)] {
        &self.utilities.stalactite_hits
    }
}
