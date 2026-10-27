//! Big dripleaf tilting (`BigDripleafBlock`): a leaf someone stands on
//! turns unstable, then tilts partway after 10 ticks, fully after 10 more,
//! and springs back 100 ticks later; a redstone signal holds it level.

use glam::{DVec3, IVec3};

use super::{Tilt, big_dripleaf, dripleaf_of};
use crate::world::World;

/// `DELAY_UNTIL_NEXT_TILT_STATE`: ticks until the next tilt, if any.
pub const fn delay(tilt: Tilt) -> Option<u16> {
    match tilt {
        Tilt::None => None,
        Tilt::Unstable | Tilt::Partial => Some(10),
        Tilt::Full => Some(100),
    }
}

/// The tilt that follows `tilt` on its scheduled tick (`BigDripleafBlock.tick`).
pub const fn next(tilt: Tilt) -> Tilt {
    match tilt {
        Tilt::Unstable => Tilt::Partial,
        Tilt::Partial => Tilt::Full,
        Tilt::Full | Tilt::None => Tilt::None,
    }
}

impl World {
    /// `BigDripleafBlock.entityInside` for a player standing at `feet`: a
    /// level leaf under someone on the ground above 11/16 of its cell, with
    /// no redstone signal, turns unstable.
    pub fn dripleaf_step(&mut self, feet: DVec3, on_ground: bool) {
        if !on_ground {
            return;
        }
        let p = feet.floor().as_ivec3();
        let Some((facing, Tilt::None, wet)) = self.get_block(p).and_then(dripleaf_of) else { return };
        if feet.y <= p.y as f64 + 0.6875 || self.redstone_power(p) > 0 {
            return;
        }
        self.set_dripleaf_tilt(p, big_dripleaf(facing, Tilt::Unstable, wet));
    }

    fn set_dripleaf_tilt(&mut self, p: IVec3, state: crate::world::block::Block) {
        self.edit(p, state, true);
        self.utilities.tilts.retain(|&(q, _)| q != p);
        if let Some((_, tilt, _)) = dripleaf_of(state)
            && let Some(ticks) = delay(tilt)
        {
            self.utilities.tilts.push((p, ticks));
        }
    }

    /// `BigDripleafBlock.neighborChanged`: a signal levels a tilted leaf at
    /// once and cancels its timer (`resetTilt`).
    pub(in crate::world) fn dripleaf_neighbor_changed(&mut self, p: IVec3) {
        let Some((facing, tilt, wet)) = self.get_block(p).and_then(dripleaf_of) else { return };
        if tilt != Tilt::None && self.redstone_power(p) > 0 {
            self.set_dripleaf_tilt(p, big_dripleaf(facing, Tilt::None, wet));
        }
    }

    /// Scheduled tilt ticks.
    pub(super) fn tick_tilts(&mut self) {
        if self.utilities.tilts.is_empty() {
            return;
        }
        let mut due = Vec::new();
        // Scheduled ticks wait while their chunk isn't loaded and block-ticking.
        let ready: Vec<bool> = self
            .utilities
            .tilts
            .iter()
            .map(|&(p, _)| self.activity.contains(p, true) && self.get_block(p).is_some())
            .collect();
        let mut ready = ready.into_iter();
        self.utilities.tilts.retain_mut(|(p, ticks)| {
            if !ready.next().unwrap_or(false) {
                return true;
            }
            *ticks = ticks.saturating_sub(1);
            if *ticks == 0 {
                due.push(*p);
                false
            } else {
                true
            }
        });
        for p in due {
            let Some((facing, tilt, wet)) = self.get_block(p).and_then(dripleaf_of) else { continue };
            // `resetTilt` while powered, else the next stage.
            let next = if self.redstone_power(p) > 0 { Tilt::None } else { next(tilt) };
            self.set_dripleaf_tilt(p, big_dripleaf(facing, next, wet));
        }
    }
}
