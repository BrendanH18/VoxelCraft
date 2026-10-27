//! Procedural textures for v0.12's colour track (layers from
//! `colour_blocks::tex::TEX`): original candle wax and wicks, banner cloth
//! and pole, flat candle and banner icons, and the straw bed.

use super::{Rgba, SIZE, noisy, rnd, shade};
use crate::color::DyeColor;
use crate::world::colour_blocks::{CANDLE_COLOURS, tex};

const CLEAR: Rgba = [0, 0, 0, 0];
/// The plain candle's beeswax.
const BEESWAX: [u8; 3] = [226, 196, 140];
const WICK: [u8; 3] = [52, 44, 38];
const POLE: [u8; 3] = [148, 110, 66];
const STRAW: [u8; 3] = [214, 178, 84];

/// Wax colour of candle colour `c` (plain, then the dyes): the dye softened
/// toward the colour of wax, as candles are paler than wool.
fn wax(c: u16) -> [u8; 3] {
    if c == 0 {
        return BEESWAX;
    }
    let dye = DyeColor::ALL[c as usize - 1].rgb();
    let mix = |d: u8, w: u8| ((d as u16 * 3 + w as u16) / 4) as u8;
    [mix(dye[0], 236), mix(dye[1], 226), mix(dye[2], 206)]
}

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        l if (tex::WAX..tex::WAX + CANDLE_COLOURS).contains(&l) => {
            // Smooth wax with faint vertical runs, lit from the top left.
            let run = rnd(layer, x, 0, 3) < 0.25 && rnd(layer, x, y / 3, 4) < 0.6;
            let edge = if x.is_multiple_of(2) { 1.04 } else { 0.94 };
            shade(wax(l - tex::WAX), edge * if run { 1.06 } else { 0.97 + r * 0.04 })
        }
        tex::WICK => shade(WICK, 0.85 + r * 0.3),
        tex::WICK_LIT => {
            let hot = rnd(layer, x, y, 2) < 0.5;
            shade(if hot { [255, 236, 150] } else { [250, 170, 60] }, 0.9 + r * 0.1)
        }
        l if (tex::CANDLE_ICON..tex::CANDLE_ICON + CANDLE_COLOURS).contains(&l) => candle_icon(l, x, y),
        l if (tex::CLOTH..tex::CLOTH + 16).contains(&l) => cloth(l, x, y),
        tex::POLE => {
            // Wood with a grain running along each piece.
            let grain = (x + y / 5).is_multiple_of(4);
            shade(POLE, if grain { 0.78 } else { 0.92 + r * 0.12 })
        }
        l if (tex::BANNER_ICON..tex::BANNER_ICON + 16).contains(&l) => banner_icon(l, x, y),
        tex::STRAW_TOP | tex::STRAW_SIDE | tex::STRAW_BOTTOM | tex::STRAW_PILLOW => straw(layer, x, y),
        _ => noisy(layer, x, y, [255, 0, 255], 0.0),
    }
}

/// A candle standing in the middle of the icon, its wick above.
fn candle_icon(layer: u16, x: usize, y: usize) -> Rgba {
    let c = wax(layer - tex::CANDLE_ICON);
    match (x, y) {
        (7, 3..=4) => shade(WICK, 1.0),
        (6..=9, 5..=14) => {
            let f = match x {
                6 => 1.12,
                9 => 0.72,
                _ => 0.95,
            };
            // The rim of the top catches the light; a drip runs down one side.
            let top = if y == 5 { 1.12 } else { 1.0 };
            let drip = x == 8 && (6..=8).contains(&y);
            shade(c, f * top * if drip { 1.08 } else { 1.0 })
        }
        _ => CLEAR,
    }
}

/// Plain woven cloth, tiling in both directions.
fn cloth(layer: u16, x: usize, y: usize) -> Rgba {
    let rgb = DyeColor::ALL[(layer - tex::CLOTH) as usize].rgb();
    let weave = if (x + y).is_multiple_of(2) { 1.0 } else { 0.93 };
    let thread = if y % 4 == 3 { 0.95 } else { 1.0 };
    shade(rgb, weave * thread * (0.96 + rnd(layer, x, y, 1) * 0.06))
}

/// A banner on its pole: the crossbar, the pole and the hanging cloth.
fn banner_icon(layer: u16, x: usize, y: usize) -> Rgba {
    let rgb = DyeColor::ALL[(layer - tex::BANNER_ICON) as usize].rgb();
    match (x, y) {
        (2..=13, 1) => shade(POLE, if x == 2 || x == 13 { 0.8 } else { 1.0 }),
        (3..=12, 2..=12) => {
            let f = match x {
                3 => 1.1,
                12 => 0.78,
                _ => 0.97,
            };
            let hem = if y == 12 { 0.85 } else { 1.0 };
            shade(rgb, f * hem * (0.97 + rnd(layer, x, y, 1) * 0.05))
        }
        (7..=8, 0 | 13..=15) => shade(POLE, if x == 7 { 1.05 } else { 0.8 }),
        _ => CLEAR,
    }
}

/// Bundled straw: strands on top tied in bands, packed layers on the
/// sides, and a pale bundle of finer straw for the pillow.
fn straw(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        tex::STRAW_TOP => {
            let strand = rnd(layer, x, 0, 5) < 0.3;
            let tie = y % 8 == 2 && (1..SIZE - 1).contains(&x);
            shade(
                STRAW,
                if tie {
                    0.66
                } else if strand {
                    0.82
                } else {
                    0.95 + r * 0.12
                },
            )
        }
        tex::STRAW_SIDE => shade(STRAW, if y.is_multiple_of(3) { 0.8 } else { 0.92 + r * 0.14 }),
        tex::STRAW_PILLOW => {
            let weave = if (x + y).is_multiple_of(2) { 1.0 } else { 0.9 };
            shade([236, 216, 150], weave * (0.95 + r * 0.08))
        }
        _ => shade([168, 132, 60], 0.9 + r * 0.12),
    }
}
