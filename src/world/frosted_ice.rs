//! v0.15 Track D: frosted ice and Frost Walker (Java 26.3
//! `FrostedIceBlock`, `ReplaceDisk` and `enchantment/frost_walker.json`).
//!
//! Frost Walker boots turn still water under a walker on the ground into
//! frosted ice of age 0 in a disk of radius `2 + level` (`linear(3, 1)`
//! clamped to 16). Each frosted ice block reschedules itself every 20 to 40
//! ticks (60 to 120 after it is placed); on a tick, one time in three or
//! when fewer than four frosted ice blocks touch it, it ages if the light
//! there is above `11 - age - 1`, and an aged-out block melts to water,
//! nudging its frosted neighbours to age too. A frosted ice block left with
//! fewer than two frosted neighbours by a neighbouring frosted ice change
//! melts at once. It never drops anything and has no item.
//!
//! States 12256..=12259 are ages 0 to 3. Ticks are persisted scheduled ticks
//! (the shared `redstone` queue), like the sniffer egg's.

use super::{
    World,
    block::{Block, RenderKind},
};
use glam::IVec3;

pub const FIRST: Block = Block(12256);
pub const LAST: u16 = 12259;
/// `FrostedIceBlock.MAX_AGE`.
pub const MAX_AGE: u8 = 3;
/// Texture layers 4000..4004: ice with growing cracks per age.
pub const TEX: u16 = 4000;
pub const TEX_END: u16 = TEX + 4;
/// `NEIGHBORS_TO_AGE` and `NEIGHBORS_TO_MELT`.
pub const NEIGHBORS_TO_AGE: u8 = 4;
pub const NEIGHBORS_TO_MELT: u8 = 2;
/// `onPlace`: `Mth.nextInt(random, 60, 120)`; `tick`: `nextInt(random, 20, 40)`.
pub const PLACE_DELAY: (u32, u32) = (60, 120);
pub const TICK_DELAY: (u32, u32) = (20, 40);
/// `Blocks.FROSTED_ICE`: `strength(0.5F)` and `friction(0.98F)`.
pub const HARDNESS: f32 = 0.5;
pub const FRICTION: f32 = 0.98;
/// The frosted ice state's light dampening (`getLightDampening`: a full,
/// non-occluding block that doesn't propagate skylight dims light by 1).
pub const LIGHT_DAMPENING: u8 = 1;

pub const fn owns(id: u16) -> bool {
    id >= FIRST.0 && id <= LAST
}

/// The frosted ice state of `age`.
pub const fn frosted_ice(age: u8) -> Block {
    Block(FIRST.0 + if age > MAX_AGE { MAX_AGE } else { age } as u16)
}

pub fn age(b: Block) -> Option<u8> {
    owns(b.0).then(|| (b.0 - FIRST.0) as u8)
}

pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    ("frosted ice", RenderKind::Translucent, [TEX + id - FIRST.0; 6])
}

/// Frost Walker's disk radius (`radius`: `clamped(linear(3, 1), 0, 16)`).
pub fn frost_walker_radius(level: u8) -> i32 {
    (3 + (i32::from(level) - 1)).clamp(0, 16)
}

impl World {
    /// A frosted ice change: `FrostedIceBlock.onPlace` schedules a tick for
    /// every new frosted ice state (an ageing included; a tick already
    /// pending stays), and frosted ice replaced by anything else tells its
    /// frosted neighbours (`neighborChanged` with the old block), which melt
    /// with fewer than two frosted neighbours left.
    pub(super) fn track_frosted_ice(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        if owns(new.0) {
            let delay = self.frost_delay(PLACE_DELAY);
            self.schedule_redstone(p, delay, 0);
        } else if owns(old.0) {
            for d in FACES {
                let q = p + d;
                if self.get_block(q).is_some_and(|b| owns(b.0)) && self.frosted_neighbours(q, NEIGHBORS_TO_MELT) {
                    self.melt_frosted_ice(q);
                }
            }
        }
    }

    /// `Mth.nextInt(random, lo, hi)`.
    fn frost_delay(&mut self, (lo, hi): (u32, u32)) -> u64 {
        u64::from(lo) + self.roll() % u64::from(hi - lo + 1)
    }

    /// `FrostedIceBlock.fewerNeigboursThan(pos, limit)`.
    fn frosted_neighbours(&self, p: IVec3, limit: u8) -> bool {
        let mut n = 0;
        for d in FACES {
            if self.get_block(p + d).is_some_and(|b| owns(b.0)) {
                n += 1;
                if n >= limit {
                    return false;
                }
            }
        }
        true
    }

    /// `FrostedIceBlock.tick`.
    pub(super) fn frosted_ice_tick(&mut self, p: IVec3) {
        let Some(age) = self.get_block(p).and_then(age) else { return };
        if self.roll().is_multiple_of(3) || self.frosted_neighbours(p, NEIGHBORS_TO_AGE) {
            // The End reads block light alone; elsewhere
            // `getMaxLocalRawBrightness`.
            let brightness = if self.generator.dimension == super::terrain::Dimension::End {
                self.update_block_light();
                self.block_light(p)
            } else {
                self.growth_light(p, true)
            };
            if i32::from(brightness) > 11 - i32::from(age) - i32::from(LIGHT_DAMPENING) && self.slightly_melt(p) {
                for d in FACES {
                    let q = p + d;
                    if self.get_block(q).is_some_and(|b| owns(b.0)) && !self.slightly_melt(q) {
                        let delay = self.frost_delay(TICK_DELAY);
                        self.schedule_redstone(q, delay, 0);
                    }
                }
                return;
            }
        }
        let delay = self.frost_delay(TICK_DELAY);
        self.schedule_redstone(p, delay, 0);
    }

    /// `slightlyMelt`: ages the block, or melts it at age 3 (returns true).
    fn slightly_melt(&mut self, p: IVec3) -> bool {
        let Some(age) = self.get_block(p).and_then(age) else { return false };
        if age < MAX_AGE {
            self.edit(p, frosted_ice(age + 1), false);
            false
        } else {
            self.melt_frosted_ice(p);
            true
        }
    }

    /// `IceBlock.melt`: water, or nothing where water evaporates (the Nether).
    fn melt_frosted_ice(&mut self, p: IVec3) {
        let into =
            if self.generator.dimension == super::terrain::Dimension::Nether { Block::AIR } else { Block::WATER };
        self.set_block(p, into);
    }

    /// Frost Walker's `replace_disk` around a walker whose feet are at
    /// `feet`, at enchantment `level`: still water with air above, under the
    /// feet's layer, within the radius horizontally of the feet and not
    /// overlapping an entity (`blocked`), becomes frosted ice of age 0.
    pub fn frost_walk(&mut self, feet: glam::DVec3, level: u8, blocked: impl Fn(IVec3) -> bool) -> usize {
        let dist = frost_walker_radius(level);
        let center = feet.floor().as_ivec3() - IVec3::Y;
        let mut placed = 0;
        for x in -dist..=dist {
            for z in -dist..=dist {
                let p = center + IVec3::new(x, 0, z);
                // `distToCenterSqr(position.x, pos.y + 0.5, position.z)`.
                let (dx, dz) = (f64::from(p.x) + 0.5 - feet.x, f64::from(p.z) + 0.5 - feet.z);
                if dx * dx + dz * dz >= f64::from(dist * dist) {
                    continue;
                }
                if self.get_block(p + IVec3::Y) == Some(Block::AIR)
                    && self.get_block(p) == Some(Block::WATER)
                    && !blocked(p)
                    && self.set_block(p, frosted_ice(0))
                {
                    placed += 1;
                }
            }
        }
        placed
    }
}

const FACES: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

#[cfg(test)]
mod tests;
