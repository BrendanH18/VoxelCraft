//! Candles and candle cakes: Java 26.3 `CandleBlock`, `CandleCakeBlock` and
//! `AbstractCandleBlock`.
//!
//! A candle block holds one to four candles of one colour, lit or not, dry
//! or waterlogged; each lit candle gives three light. A candle is placed
//! on a full uneaten cake to make a candle cake, which is lit the same way
//! and drops its candle when eaten from or broken.

use glam::{DVec3, IVec3};

use super::super::block::Block;
use super::super::shape::{Box16, Boxes};
use super::{CAKE_ORIGIN, CANDLE_COLOURS, CANDLE_ORIGIN, tex};
use crate::item::Item;

/// One candle block state. `colour` is 0 for the plain candle, then
/// `1 + DyeColor`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Candle {
    pub colour: u8,
    /// 1..=4.
    pub candles: u8,
    pub lit: bool,
    pub wet: bool,
}

/// The candle state with these properties.
pub const fn candle(colour: u8, candles: u8, lit: bool, wet: bool) -> Block {
    let n = if candles < 1 {
        1
    } else if candles > 4 {
        4
    } else {
        candles
    };
    Block(CANDLE_ORIGIN + colour as u16 * 16 + (n as u16 - 1) * 4 + lit as u16 * 2 + wet as u16)
}

pub const fn decode(id: u16) -> Option<Candle> {
    if id < CANDLE_ORIGIN || id >= CAKE_ORIGIN {
        return None;
    }
    let i = id - CANDLE_ORIGIN;
    let local = i % 16;
    Some(Candle { colour: (i / 16) as u8, candles: (local / 4) as u8 + 1, lit: local & 2 != 0, wet: local & 1 != 0 })
}

pub const fn encode(c: Candle) -> Block {
    candle(c.colour, c.candles, c.lit, c.wet)
}

/// The candle cake of candle colour `colour`.
pub const fn cake(colour: u8, lit: bool) -> Block {
    Block(CAKE_ORIGIN + colour as u16 * 2 + lit as u16)
}

/// Candle colour and lit state of a candle cake.
pub const fn decode_cake(id: u16) -> Option<(u8, bool)> {
    if id < CAKE_ORIGIN || id >= CAKE_ORIGIN + CANDLE_COLOURS * 2 {
        return None;
    }
    let i = id - CAKE_ORIGIN;
    Some(((i / 2) as u8, i % 2 == 1))
}

/// The candle colour of a candle item.
pub fn colour_of_item(item: Item) -> Option<u8> {
    item.block().and_then(|b| decode(b.0)).map(|c| c.colour)
}

const fn b(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}

/// Java's candle models (`template_candle` .. `template_four_candles`): each
/// candle's body, minimum corner and height.
const BODIES: [&[([u8; 2], u8)]; 4] = [
    &[([7, 7], 6)],
    &[([5, 7], 5), ([9, 6], 6)],
    &[([7, 9], 3), ([5, 7], 5), ([8, 6], 6)],
    &[([6, 8], 3), ([9, 8], 5), ([5, 5], 5), ([8, 5], 6)],
];

/// The bodies (2×2 columns), then a one-pixel wick on each. Java's wick is
/// two crossed planes centred on the body's middle; a box can't sit on a
/// half pixel, so it stands half a pixel toward +x and +z.
pub fn boxes(c: Candle) -> Boxes {
    let bodies = BODIES[c.candles as usize - 1];
    let mut out = Boxes::from_box(body(bodies[0]));
    for &body_at in &bodies[1..] {
        out.push(body(body_at));
    }
    for &body_at in bodies {
        out.push(wick(body_at, 0));
    }
    out
}

const fn body(([x, z], h): ([u8; 2], u8)) -> Box16 {
    b([x, 0, z], [x + 2, h, z + 2])
}

const fn wick(([x, z], h): ([u8; 2], u8), lift: u8) -> Box16 {
    b([x + 1, lift + h, z + 1], [x + 2, lift + h + 1, z + 2])
}

/// `CandleBlock.SHAPES`: one box around all the candles.
pub fn outline(c: Candle) -> Boxes {
    Boxes::from_box(match c.candles {
        1 => b([7, 0, 7], [9, 6, 9]),
        2 => b([5, 0, 6], [11, 6, 9]),
        3 => b([5, 0, 6], [10, 6, 11]),
        _ => b([5, 0, 5], [11, 6, 10]),
    })
}

/// The cake (drawn with the block's cake faces), the candle on top and its wick.
pub fn cake_boxes() -> Boxes {
    let mut out = Boxes::from_box(b([1, 0, 1], [15, 8, 15]));
    out.push(b([7, 8, 7], [9, 14, 9]));
    out.push(wick(([7, 7], 6), 8));
    out
}

/// `CandleCakeBlock.SHAPE`: the cake and the candle, without the wick.
pub fn cake_outline() -> Boxes {
    let mut out = Boxes::from_box(b([1, 0, 1], [15, 8, 15]));
    out.push(b([7, 8, 7], [9, 14, 9]));
    out
}

/// Layer of box `index` of [`boxes`] or [`cake_boxes`]: wax, then wicks.
pub fn box_layer(block: Block, index: usize) -> Option<u16> {
    let wick = |lit: bool| if lit { tex::WICK_LIT } else { tex::WICK };
    if let Some(c) = decode(block.0) {
        return Some(if index < c.candles as usize { tex::WAX + c.colour as u16 } else { wick(c.lit) });
    }
    let (colour, lit) = decode_cake(block.0)?;
    match index {
        0 => None,
        1 => Some(tex::WAX + colour as u16),
        _ => Some(wick(lit)),
    }
}

/// `CandleBlock.PARTICLE_OFFSETS` per candle count, in 1/16 block.
const FLAMES: [&[[f64; 3]]; 4] = [
    &[[8.0, 8.0, 8.0]],
    &[[6.0, 7.0, 8.0], [10.0, 8.0, 7.0]],
    &[[8.0, 5.0, 10.0], [6.0, 7.0, 8.0], [9.0, 8.0, 7.0]],
    &[[7.0, 5.0, 9.0], [10.0, 7.0, 9.0], [6.0, 7.0, 6.0], [9.0, 8.0, 6.0]],
];

/// Where a lit candle block's or candle cake's flames burn, within its cell
/// (`getParticleOffsets`).
pub fn flame_offsets(block: Block) -> &'static [[f64; 3]] {
    if let Some(c) = decode(block.0) {
        return FLAMES[c.candles as usize - 1];
    }
    if decode_cake(block.0).is_some() {
        return &[[8.0, 16.0, 8.0]];
    }
    &[]
}

/// Whether flint and steel or a fire charge lights this state
/// (`CandleBlock.canLight`, `CandleCakeBlock.canLight`).
pub fn can_light(block: Block) -> bool {
    if let Some(c) = decode(block.0) {
        return !c.lit && !c.wet;
    }
    matches!(decode_cake(block.0), Some((_, false)))
}

/// Whether the state is a lit candle or candle cake (`AbstractCandleBlock.isLit`).
pub fn is_lit(block: Block) -> bool {
    decode(block.0).is_some_and(|c| c.lit) || matches!(decode_cake(block.0), Some((_, true)))
}

/// The state with its `lit` property set (unchanged for other blocks).
pub fn with_lit(block: Block, lit: bool) -> Block {
    if let Some(c) = decode(block.0) {
        return encode(Candle { lit, ..c });
    }
    match decode_cake(block.0) {
        Some((colour, _)) => cake(colour, lit),
        None => block,
    }
}

/// `CandleBlock.canBeReplaced`: a candle of the held kind with room for
/// another takes it, unless the player sneaks.
pub fn takes_another(block: Block, held: Block, sneaking: bool) -> bool {
    match (decode(block.0), decode(held.0)) {
        (Some(c), Some(h)) => !sneaking && c.colour == h.colour && c.candles < 4,
        _ => false,
    }
}

/// `CandleBlock.getStateForPlacement` on a cell holding `existing`: one more
/// candle (keeping lit and water), or a new single candle.
pub fn placed(held: Block, existing: Block) -> Block {
    let h = decode(held.0).expect("a candle item");
    match decode(existing.0) {
        Some(c) if c.colour == h.colour => encode(Candle { candles: (c.candles + 1).min(4), ..c }),
        _ => candle(h.colour, 1, false, false),
    }
}

impl super::super::World {
    /// Flint and steel or a fire charge on `p`: lights an unlit dry candle
    /// or candle cake (`FlintAndSteelItem.useOn`). False if it can't be lit.
    pub fn light_candle(&mut self, p: IVec3) -> bool {
        match self.get_block(p) {
            Some(b) if can_light(b) => self.set_block(p, with_lit(b, true)),
            _ => false,
        }
    }

    /// `AbstractCandleBlock.extinguish`: puts a lit candle or candle cake
    /// out, with a puff of smoke over each flame.
    pub fn extinguish_candle(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p).filter(|&b| is_lit(b)) else { return false };
        for o in flame_offsets(b) {
            let pos = p.as_dvec3() + DVec3::from_array(*o) / 16.0;
            let mut smoke = crate::particles::Burst::new(crate::particles::Kind::Smoke, pos, 1);
            smoke.velocity = DVec3::new(0.0, 0.1, 0.0);
            self.particles.push(crate::particles::Request::Burst(smoke));
        }
        self.set_block(p, with_lit(b, false))
    }

    /// `CakeBlock.useItemOn` with a candle: an uneaten cake becomes a
    /// candle cake of that candle.
    pub fn add_candle_to_cake(&mut self, p: IVec3, held: Item) -> bool {
        let Some(colour) = colour_of_item(held) else { return false };
        if self.get_block(p) != Some(Block::cake(0)) {
            return false;
        }
        self.set_block(p, cake(colour, false))
    }

    /// `CandleCakeBlock.useWithoutItem` after a successful bite: the cake
    /// loses a slice (becoming a plain cake) and drops its candle.
    pub fn bite_candle_cake(&mut self, p: IVec3) -> bool {
        let Some((colour, _)) = self.get_block(p).and_then(|b| decode_cake(b.0)) else { return false };
        if !self.set_block(p, Block::cake(1)) {
            return false;
        }
        let stack = crate::inventory::Stack::new(Item::from_block(candle(colour, 1, false, false)), 1);
        self.drops.push((p, stack));
        true
    }
}

/// Whether a click at `click_y` (0..1 within the cell) hits a candle cake's
/// candle rather than its cake (`CandleCakeBlock.candleHit`).
pub fn candle_hit(click_y: f64) -> bool {
    click_y > 0.5
}
