//! Cauldrons (`AbstractCauldronBlock`, `LayeredCauldronBlock`,
//! `LavaCauldronBlock`, `CauldronInteractions`): bucket and bottle
//! exchanges, filling from rain and snow, and from dripstone above.

use glam::IVec3;

use super::{CAULDRON, Contents, cauldron, cauldron_of};
use crate::item::Item;
use crate::potion::Potion;
use crate::world::{World, block::Block, overworld_blocks as ob};

/// What a held item does to a cauldron (`CauldronInteraction`s): the new
/// contents and what the hand gets back (`None`: nothing happens).
pub fn exchange(contents: Contents, held: Item) -> Option<(Contents, Item)> {
    use Contents::*;
    // `addDefaultInteractions`: any cauldron takes a full bucket.
    match held {
        Item::WATER_BUCKET => return Some((Water(3), Item::BUCKET)),
        Item::LAVA_BUCKET => return Some((Lava, Item::BUCKET)),
        Item::POWDER_SNOW_BUCKET => return Some((PowderSnow(3), Item::BUCKET)),
        _ => {}
    }
    let water_bottle = Item::potion(Potion::WATER);
    match (contents, held) {
        // `EMPTY`: a water bottle pours one level.
        (Empty, h) if h == water_bottle => Some((Water(1), Item::GLASS_BOTTLE)),
        // `WATER`: a bucket takes a full cauldron; bottles take and give a level.
        (Water(3), Item::BUCKET) => Some((Empty, Item::WATER_BUCKET)),
        (Water(level), Item::GLASS_BOTTLE) => Some((Water(level - 1), water_bottle)),
        (Water(level), h) if h == water_bottle && level < 3 => Some((Water(level + 1), Item::GLASS_BOTTLE)),
        (Lava, Item::BUCKET) => Some((Empty, Item::LAVA_BUCKET)),
        (PowderSnow(3), Item::BUCKET) => Some((Empty, Item::POWDER_SNOW_BUCKET)),
        _ => None,
    }
}

/// Whether `held` does something to the cauldron `b` (so block use
/// precedes the item's own use, as Java's `useItemOn` does).
pub fn takes(b: Block, held: Option<Item>) -> bool {
    match (cauldron_of(b), held) {
        (Some(contents), Some(item)) => exchange(contents, item).is_some(),
        _ => false,
    }
}

/// `CauldronBlock.handlePrecipitation` and `LayeredCauldronBlock`'s: rain
/// fills an empty or water cauldron a level 5% of the time, snow a powder
/// snow one 10% of the time.
pub fn precipitation_fill(contents: Contents, snow: bool, roll: f32) -> Option<Contents> {
    use Contents::*;
    match (contents, snow) {
        (Empty, false) if roll < 0.05 => Some(Water(1)),
        (Empty, true) if roll < 0.1 => Some(PowderSnow(1)),
        (Water(level), false) if level < 3 && roll < 0.05 => Some(Water(level + 1)),
        (PowderSnow(level), true) if level < 3 && roll < 0.1 => Some(PowderSnow(level + 1)),
        _ => None,
    }
}

/// What a drip of `lava` (else water) does to a cauldron
/// (`canReceiveStalactiteDrip`/`receiveStalactiteDrip`).
pub fn drip_fill(contents: Contents, lava: bool) -> Option<Contents> {
    use Contents::*;
    match (contents, lava) {
        (Empty, true) => Some(Lava),
        (Empty, false) => Some(Water(1)),
        (Water(level), false) if level < 3 => Some(Water(level + 1)),
        _ => None,
    }
}

impl World {
    /// Sets the cauldron at `p` to hold `contents`.
    pub fn set_cauldron(&mut self, p: IVec3, contents: Contents) -> bool {
        self.get_block(p).and_then(cauldron_of).is_some() && self.set_block(p, cauldron(contents))
    }

    /// `ServerLevel.tickPrecipitation` for cauldrons: each block-ticking
    /// 16x16 column has a 1 in 16 chance per tick that its surface block
    /// is offered the weather falling on it.
    pub(super) fn cauldron_precipitation(&mut self) {
        if !self.raining || !self.generator.dimension.has_sky() {
            return;
        }
        let columns: Vec<glam::IVec2> = match self.activity.block_columns() {
            Some(bounds) => {
                let mut out = Vec::new();
                for (lo, hi) in bounds {
                    for x in lo.x..=hi.x {
                        for z in lo.y..=hi.y {
                            out.push(glam::IVec2::new(x, z));
                        }
                    }
                }
                out
            }
            None => self.columns.keys().copied().collect(),
        };
        let size = crate::world::chunk::CHUNK_SIZE_I;
        for c in columns {
            // Four Java chunks per storage column.
            for q in 0..4 {
                if !self.roll().is_multiple_of(16) {
                    continue;
                }
                let r = self.roll();
                let x = c.x * size + (q % 2) * 16 + (r & 15) as i32;
                let z = c.y * size + (q / 2) * 16 + (r >> 4 & 15) as i32;
                // Java's `MOTION_BLOCKING` surface: the column heightmap
                // holds light-blocking blocks, and a cauldron blocks none,
                // so look for the highest solid block a little above it.
                let Some(ground) = self.surface_height(x, z) else { continue };
                let top = (ground + 1..ground + 24)
                    .rev()
                    .find(|&y| self.get_block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid() || b.is_fluid()))
                    .unwrap_or(ground);
                let p = IVec3::new(x, top, z);
                let Some(contents) = self.get_block(p).and_then(cauldron_of) else { continue };
                let Some(line) = self.snow_line_at(x, z) else { continue };
                if line == i16::MAX {
                    continue;
                }
                let snow = p.y >= line as i32;
                let roll = (self.roll() % 1_000_000) as f32 / 1_000_000.0;
                if let Some(next) = precipitation_fill(contents, snow, roll) {
                    self.set_block(p, cauldron(next));
                }
            }
        }
    }

    /// `PointedDripstoneBlock.maybeTransferFluid` on a random tick of the
    /// top of a stalactite: water drips 17.6% of the time and lava 5.9%
    /// from a source on the block it hangs from, down to the first cauldron below the tip
    /// that takes it, arriving `50 + fall` ticks later. Mud above drips
    /// into the air and dries into clay instead.
    pub(super) fn dripstone_transfer(&mut self, p: IVec3, b: Block) {
        if !matches!(ob::dripstone_of(b.canonical()), Some((true, _)))
            || self.get_block(p + IVec3::Y).is_some_and(|a| ob::dripstone_of(a.canonical()).is_some())
        {
            return;
        }
        let roll = (self.roll() % 1_000_000) as f32 / 1_000_000.0;
        // `getFluidAboveStalactite`: the fluid sits on the block the
        // stalactite hangs from (`findRootBlock`, the first cell above that
        // isn't pointed dripstone), and mud there counts as water.
        let above = p + IVec3::Y * 2;
        let Some(source) = self.get_block(above) else { return };
        let nether = self.generator.dimension == crate::world::terrain::Dimension::Nether;
        let lava = if source == Block::LAVA {
            true
        } else if crate::world::waterlog::holds_source(source) || (source == ob::MUD && !nether) {
            false
        } else {
            return;
        };
        if roll >= if lava { 0.058_593_75 } else { 0.175_781_25 } {
            return;
        }
        // The tip: down the run of downward dripstone, at most 11 blocks.
        let mut tip = p;
        for _ in 0..11 {
            match self.get_block(tip - IVec3::Y) {
                Some(n) if matches!(ob::dripstone_of(n.canonical()), Some((true, _))) => tip -= IVec3::Y,
                _ => break,
            }
        }
        if !matches!(
            self.get_block(tip).and_then(|t| ob::dripstone_of(t.canonical())),
            Some((true, ob::Thickness::Tip))
        ) {
            return;
        }
        if source == ob::MUD {
            self.set_block(above, Block::CLAY);
            return;
        }
        // `findFillableCauldronBelowStalactiteTip`: through air and
        // non-colliding blocks, at most 11 blocks down.
        let mut at = tip;
        for _ in 0..11 {
            at -= IVec3::Y;
            let Some(n) = self.get_block(at) else { return };
            if let Some(contents) = cauldron_of(n) {
                if drip_fill(contents, lava).is_some() {
                    let delay = 50 + (tip.y - at.y) as u16;
                    if !self.utilities.drips.iter().any(|&(q, _)| q == at) {
                        self.utilities.drips.push((at, delay));
                    }
                }
                return;
            }
            if n.is_solid() || n.is_fluid() {
                return;
            }
        }
    }

    /// `AbstractCauldronBlock.tick`: the scheduled drip arrives if a
    /// stalactite with fluid above still points at the cauldron.
    pub(super) fn tick_drips(&mut self) {
        if self.utilities.drips.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.utilities.drips.retain_mut(|(p, ticks)| {
            *ticks = ticks.saturating_sub(1);
            if *ticks == 0 {
                due.push(*p);
                false
            } else {
                true
            }
        });
        for p in due {
            let Some(contents) = self.get_block(p).and_then(cauldron_of) else { continue };
            let Some(lava) = self.drip_fluid_above(p) else { continue };
            if let Some(next) = drip_fill(contents, lava) {
                self.set_block(p, cauldron(next));
            }
        }
    }

    /// `findStalactiteTipAboveCauldron` and `getCauldronFillFluidType`: the
    /// fluid (lava or water) over the stalactite whose tip is above `p`.
    fn drip_fluid_above(&self, p: IVec3) -> Option<bool> {
        let mut at = p;
        for _ in 0..11 {
            at += IVec3::Y;
            let b = self.get_block(at)?;
            if matches!(ob::dripstone_of(b.canonical()), Some((true, ob::Thickness::Tip))) {
                let mut root = at;
                while self
                    .get_block(root + IVec3::Y)
                    .is_some_and(|a| matches!(ob::dripstone_of(a.canonical()), Some((true, _))))
                {
                    root += IVec3::Y;
                }
                // The fluid sits on the block the stalactite hangs from.
                let source = self.get_block(root + IVec3::Y * 2)?;
                return if source == Block::LAVA {
                    Some(true)
                } else if crate::world::waterlog::holds_source(source) {
                    Some(false)
                } else {
                    None
                };
            }
            if b.is_solid() || b.is_fluid() {
                return None;
            }
        }
        None
    }

    /// Whether `p` holds an empty cauldron (tests and commands).
    pub fn is_empty_cauldron(&self, p: IVec3) -> bool {
        self.get_block(p) == Some(CAULDRON)
    }
}
