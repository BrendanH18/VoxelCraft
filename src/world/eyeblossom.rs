//! Java 26.3 `EyeblossomBlock`: open and closed eyeblossoms and their potted
//! forms, in Track A's reservation (see `docs/architecture.md`, "v0.15 final
//! slice reservations", and the dated creaking verification record).
//!
//! A random or scheduled tick compares the flower with the Overworld day
//! timeline's `gameplay/eyeblossom_open` attribute (open from tick 12600,
//! closed from 23401). A flower that changes schedules every other flower of
//! its old kind within ±3/±2/±3 blocks to tick after
//! `nextIntBetweenInclusive(5 * d, 10 * d)` ticks, `d` being the distance,
//! so whole patches ripple open or shut. Outside the Overworld the attribute
//! is undefined and the flowers keep their state.
use super::{
    World,
    block::{Block, RenderKind},
};
use glam::IVec3;

pub const OPEN: Block = Block(14888);
pub const CLOSED: Block = Block(14889);
pub const POTTED_OPEN: Block = Block(14890);
pub const POTTED_CLOSED: Block = Block(14891);
/// Texture layers: open and closed flowers, then their potted sides.
pub const TEX: u16 = super::creaking::TEX + 13;
pub const TEX_END: u16 = TEX + 4;

pub const fn owns(id: u16) -> bool {
    id >= OPEN.0 && id <= POTTED_CLOSED.0
}

/// Open or closed for an eyeblossom, potted or not.
pub const fn is_open(b: Block) -> Option<bool> {
    match b.0 {
        14888 | 14890 => Some(true),
        14889 | 14891 => Some(false),
        _ => None,
    }
}

pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    match id {
        14888 => ("open eyeblossom", RenderKind::Cross, [TEX; 6]),
        14889 => ("closed eyeblossom", RenderKind::Cross, [TEX + 1; 6]),
        14890 => ("potted open eyeblossom", RenderKind::Shaped, pot(TEX + 2)),
        14891 => ("potted closed eyeblossom", RenderKind::Shaped, pot(TEX + 3)),
        _ => ("unknown", RenderKind::Invisible, [0; 6]),
    }
}

const fn pot(side: u16) -> [u16; 6] {
    use super::utilities::t;
    [side, side, t::POT_SOIL, t::POT_BOTTOM, side, side]
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [OPEN.0, CLOSED.0].into_iter()
}

/// `FlowerBlock` survival: Java's `#supports_vegetation` soils.
pub fn can_stay_on(b: Block, below: Block) -> Option<bool> {
    matches!(b, OPEN | CLOSED).then(|| super::overworld_blocks::dirt_like(below))
}

/// The potted form of a flower and the flower in a pot.
pub const fn potted(plant: Block) -> Option<Block> {
    match plant {
        OPEN => Some(POTTED_OPEN),
        CLOSED => Some(POTTED_CLOSED),
        _ => None,
    }
}
pub const fn potted_plant(b: Block) -> Option<Block> {
    match b {
        POTTED_OPEN => Some(OPEN),
        POTTED_CLOSED => Some(CLOSED),
        _ => None,
    }
}

/// The flower drawn above the pot on a potted texture layer.
pub const fn pot_layer_plant(layer: u16) -> Option<Block> {
    match layer.wrapping_sub(TEX) {
        2 => Some(OPEN),
        3 => Some(CLOSED),
        _ => None,
    }
}

/// The flower of the other kind.
pub const fn flipped(b: Block) -> Block {
    match b {
        OPEN => CLOSED,
        _ => OPEN,
    }
}

/// Java's `gameplay/eyeblossom_open` timeline keyframes (Overworld only).
pub const fn open_at(day_ticks: u32) -> bool {
    let t = day_ticks % 24000;
    t >= 12600 && t < 23401
}

impl World {
    /// `EyeblossomBlock.tryChangingState`, for a random or scheduled tick.
    pub(super) fn tick_eyeblossom(&mut self, p: IVec3, b: Block) -> bool {
        let Some(open) = self.eyeblossom_open else { return false };
        if !matches!(b, OPEN | CLOSED) || (b == OPEN) == open {
            return false;
        }
        self.edit(p, flipped(b), true);
        for dy in -2..=2 {
            for dz in -3..=3 {
                for dx in -3..=3 {
                    let q = p + IVec3::new(dx, dy, dz);
                    if q == p || self.get_block(q) != Some(b) {
                        continue;
                    }
                    let d = f64::from(dx * dx + dy * dy + dz * dz).sqrt();
                    let (lo, hi) = ((d * 5.) as u64, (d * 10.) as u64);
                    let delay = lo + self.roll() % (hi - lo + 1);
                    self.schedule_redstone(q, delay, 0);
                }
            }
        }
        true
    }
}

/// Original sprites: a pale stem with a shut grey bud, or an open bloom whose
/// orange iris stares out of pale petals. Pots reuse the shared pot artwork.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let open = (layer - TEX) % 2 == 0;
    let n = ((x * 7 + y * 13 + x * y) % 9) as u8;
    let (cx, cy) = (x as i32 - 8, y as i32 - 5);
    let r2 = cx * cx + cy * cy;
    let stem = (x == 7 || x == 8) && y >= 7;
    let leaf = y >= 10 && ((x as i32 - 7 + (y as i32 - 13)).abs() <= 1 && x < 7 || (x as i32 - 8 - (13 - y as i32)).abs() <= 1 && x > 8);
    if open {
        if r2 <= 2 {
            return [24, 16, 12, 255];
        }
        if r2 <= 6 {
            return [236, 120 + n * 3, 28, 255];
        }
        if r2 <= 16 && (x + y) % 5 != 0 {
            return [196 + n, 190 + n, 176 + n, 255];
        }
    } else if (2..=8).contains(&y) && (cx.abs() <= 2 - i32::from(y < 4) - i32::from(y > 7)) {
        return [118 + n, 116 + n, 110 + n, 255];
    }
    if stem || leaf { [96 + n, 104 + n, 86, 255] } else { [0; 4] }
}

#[cfg(test)]
mod tests;
