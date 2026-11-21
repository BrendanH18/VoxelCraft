//! v0.16 Track H: Java 26.3's shared `SpeleothemBlock` rules for pointed
//! dripstone and sulfur spikes (`#speleothems`), on top of the falling
//! stalactites in `utilities::dripstone`:
//!
//! - `updateShape`: when the cell above or below a speleothem changes, its
//!   `thickness` is recomputed (`calculateSpeleothemThickness`; a merged tip
//!   stays merged while it can); this is the shared code path, so pointed
//!   dripstone follows it too;
//! - `getStateForPlacement` for sulfur spikes: the thickness against the
//!   neighbours (tips merge unless sneaking) and the cell's water;
//! - `randomTick` growth for sulfur spikes (`SulfurSpikeBlock`,
//!   `getMaxGrowthLength` 2, growing on sulfur): a hanging run of at most
//!   two whose top is under sulfur grows its free tip down, or a stalagmite
//!   up from the floor within ten blocks, with Java's 0.011377778 chance a
//!   random tick and an even split.
//!
//! In 26.3 only `PointedDripstoneBlock` overrides `fallOn` (landing on an
//! upward tip doubles the fall and adds 2.5 blocks), so sulfur spikes hurt
//! nothing that lands on them, as in Java; the dripstone's own landing
//! damage is a different code path and is not part of this module.

use glam::IVec3;

use super::World;
use super::block::Block;
use super::gen5_blocks as g;
use super::overworld_blocks::{self as ob, Thickness};

/// `GROWTH_PROBABILITY_PER_RANDOM_TICK`.
pub const GROWTH_CHANCE: f32 = 0.011_377_778;
/// `SulfurSpikeBlock.getMaxGrowthLength`.
pub const SPIKE_MAX_GROWTH_LENGTH: i32 = 2;
/// `MAX_STALAGMITE_SEARCH_RANGE_WHEN_GROWING`.
pub const STALAGMITE_SEARCH: i32 = 10;

/// Which speleothem block a state belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dripstone,
    SulfurSpike,
}

/// One speleothem state: its block, `vertical_direction` (down or up),
/// `thickness` and `waterlogged`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Speleothem {
    pub kind: Kind,
    pub down: bool,
    pub thickness: Thickness,
    pub wet: bool,
}

/// The speleothem `b` is, if any (`#speleothems`).
pub fn of(b: Block) -> Option<Speleothem> {
    if let Some((down, thickness, wet)) = g::spike_of(b) {
        return Some(Speleothem { kind: Kind::SulfurSpike, down, thickness, wet });
    }
    let (down, thickness) = ob::dripstone_of(b.canonical())?;
    Some(Speleothem { kind: Kind::Dripstone, down, thickness, wet: b.is_waterlogged() })
}

/// The block of a speleothem state.
pub fn block(s: Speleothem) -> Block {
    match s.kind {
        Kind::SulfurSpike => g::spike(s.down, s.thickness, s.wet),
        Kind::Dripstone => ob::pointed_dripstone(s.down, s.thickness).with_water(s.wet),
    }
}

/// `isSpeleothemWithDirection`: any `#speleothems` block pointing `down`.
fn with_direction(b: Block, down: bool) -> bool {
    of(b).is_some_and(|s| s.down == down)
}

/// `calculateSpeleothemThickness` for a `kind` speleothem pointing `down`
/// at a cell with `front` in its tip's direction and `behind` at its base.
pub fn thickness(kind: Kind, down: bool, merge: bool, front: Block, behind: Block) -> Thickness {
    if let Some(f) = of(front).filter(|f| f.down != down && f.kind == kind) {
        return if !merge && f.thickness != Thickness::TipMerge { Thickness::Tip } else { Thickness::TipMerge };
    }
    match of(front).filter(|f| f.down == down) {
        None => Thickness::Tip,
        Some(f) if matches!(f.thickness, Thickness::Tip | Thickness::TipMerge) => Thickness::Frustum,
        Some(_) => {
            if with_direction(behind, down) {
                Thickness::Middle
            } else {
                Thickness::Base
            }
        }
    }
}

/// `isValidSpeleothemPlacement`: a sturdy face at the base, or a
/// speleothem of the same block pointing the same way.
fn valid_placement(kind: Kind, down: bool, behind: Block) -> bool {
    let face = if down { IVec3::NEG_Y } else { IVec3::Y };
    super::mount_states::face_sturdy(behind, face)
        || of(behind).is_some_and(|b| b.kind == kind && b.down == down)
        // The engine's dripstone support also counts these.
        || kind == Kind::Dripstone && behind.is_opaque()
}

/// The state a held sulfur spike places at `at` (`getStateForPlacement`),
/// or `None` where it can't stand or hang.
pub fn placed_spike(at: IVec3, look_up: bool, sneaking: bool, get: impl Fn(IVec3) -> Block) -> Option<Block> {
    // `getNearestLookingVerticalDirection().getOpposite()`.
    let first_down = look_up;
    for down in [first_down, !first_down] {
        let behind = if down { at + IVec3::Y } else { at - IVec3::Y };
        if valid_placement(Kind::SulfurSpike, down, get(behind)) {
            let front = if down { at - IVec3::Y } else { at + IVec3::Y };
            let t = thickness(Kind::SulfurSpike, down, !sneaking, get(front), get(behind));
            let wet = get(at) == Block::WATER;
            return Some(g::spike(down, t, wet));
        }
    }
    None
}

impl World {
    /// `SpeleothemBlock.updateShape` for the speleothems above and below a
    /// changed cell: a new thickness for those whose support is still there
    /// (unsupported ones fall or break through the plant rules).
    pub(super) fn speleothem_neighbours(&mut self, p: IVec3) {
        for (q, from_above) in [(p - IVec3::Y, true), (p + IVec3::Y, false)] {
            let Some(b) = self.get_block(q) else { continue };
            let Some(s) = of(b) else { continue };
            let (front, behind) = if s.down { (q - IVec3::Y, q + IVec3::Y) } else { (q + IVec3::Y, q - IVec3::Y) };
            // The changed cell is at the base: the support check owns it.
            let base_changed = (from_above && s.down) || (!from_above && !s.down);
            let (Some(front_block), Some(behind_block)) = (self.get_block(front), self.get_block(behind)) else {
                continue;
            };
            if base_changed && !valid_placement(s.kind, s.down, behind_block) {
                continue;
            }
            let merge = s.thickness == Thickness::TipMerge;
            let t = thickness(s.kind, s.down, merge, front_block, behind_block);
            if t != s.thickness {
                self.edit_exact(q, block(Speleothem { thickness: t, ..s }), true);
            }
        }
    }

    /// `SpeleothemBlock.randomTick` for a sulfur spike: growth from the top
    /// of a hanging run under sulfur.
    pub(super) fn speleothem_random_tick(&mut self, p: IVec3, b: Block) {
        let Some(s) = of(b).filter(|s| s.kind == Kind::SulfurSpike) else { return };
        if !self.loot_chance(GROWTH_CHANCE) || !s.down {
            return;
        }
        // `isStalactiteStartPos`, then `canGrow` (sulfur above).
        let above = self.get_block(p + IVec3::Y);
        if above.is_some_and(|a| g::spike_of(a).is_some()) || above != Some(g::SULFUR) {
            return;
        }
        // `findTip(state, level, pos, getMaxGrowthLength(), false)`.
        // The start itself, then (`findBlockVertical`) one step down: the
        // tip test (any `#speleothems` tip) comes before the path test
        // (this block, pointing down).
        let tip = if s.thickness == Thickness::Tip {
            Some(p)
        } else {
            let below = p - IVec3::Y;
            self.get_block(below).and_then(of).filter(|b| b.thickness == Thickness::Tip).map(|_| below)
        };
        const _: () = assert!(SPIKE_MAX_GROWTH_LENGTH == 2);
        let Some(tip) = tip else { return };
        let Some(t) = self.get_block(tip).and_then(of) else { return };
        // `isFreeHangingStalactite` and `canTipGrow`.
        if !t.down || t.wet || !self.tip_can_grow(tip, true) {
            return;
        }
        if self.roll() & 1 == 0 {
            self.grow_speleothem(tip, true);
        } else {
            self.grow_stalagmite_below(tip);
        }
    }

    /// `canTipGrow`: the cell past the tip holds no fluid and is air or an
    /// unmerged sulfur tip pointing back.
    fn tip_can_grow(&self, tip: IVec3, down: bool) -> bool {
        let next = if down { tip - IVec3::Y } else { tip + IVec3::Y };
        let Some(b) = self.get_block(next) else { return false };
        if b.is_fluid() || b.is_waterlogged() {
            return false;
        }
        b == Block::AIR || unmerged_spike_tip(b, !down)
    }

    /// `grow(level, from, direction)`: a new tip past `from`, or merged tips
    /// where an opposite tip waits.
    fn grow_speleothem(&mut self, from: IVec3, down: bool) {
        let target = if down { from - IVec3::Y } else { from + IVec3::Y };
        let Some(existing) = self.get_block(target) else { return };
        if unmerged_spike_tip(existing, !down) {
            // `createMergedTips`.
            let (stalactite, stalagmite) = if down { (target + IVec3::Y, target) } else { (target, target - IVec3::Y) };
            let wet = |w: &World, p: IVec3| w.get_block(p).is_some_and(Block::holds_water);
            let (a, b) = (wet(self, stalactite), wet(self, stalagmite));
            self.edit_exact(stalactite, g::spike(true, Thickness::TipMerge, a), true);
            self.edit_exact(stalagmite, g::spike(false, Thickness::TipMerge, b), true);
        } else if existing == Block::AIR || existing.is_water() {
            // `getFluidState(pos).is(Fluids.WATER)`: a source.
            let wet = existing == Block::WATER;
            self.edit_exact(target, g::spike(down, Thickness::Tip, wet), true);
        }
    }

    /// `growStalagmiteBelow`: the first floor within ten blocks under the
    /// tip grows a stalagmite (or an upward tip there grows on).
    fn grow_stalagmite_below(&mut self, above: IVec3) {
        let mut p = above;
        for _ in 0..STALAGMITE_SEARCH {
            p -= IVec3::Y;
            let Some(b) = self.get_block(p) else { return };
            if b.is_fluid() || b.is_waterlogged() {
                return;
            }
            if unmerged_spike_tip(b, false) && self.tip_can_grow(p, false) {
                self.grow_speleothem(p, false);
                return;
            }
            let below = self.get_block(p - IVec3::Y);
            if below.is_some_and(|under| valid_placement(Kind::SulfurSpike, false, under))
                && !below.is_some_and(Block::holds_water)
            {
                self.grow_speleothem(p - IVec3::Y, false);
                return;
            }
            // `canDripThrough`: a solid block ends the search.
            if b != Block::AIR && b.is_solid() {
                return;
            }
        }
    }
}

/// `isUnmergedTipWithDirection` for sulfur spikes.
fn unmerged_spike_tip(b: Block, down: bool) -> bool {
    matches!(g::spike_of(b), Some((d, Thickness::Tip, _)) if d == down)
}

#[cfg(test)]
mod tests;
