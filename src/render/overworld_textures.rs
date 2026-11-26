//! Procedural textures for the v0.6 Overworld blocks (layers from
//! `overworld_blocks::TEX`): original artwork in the palette of Java's
//! moss, dripstone, amethyst, ocean blocks and new woods. The plants,
//! corals and sea pickles are drawn by the material generator (v1.0
//! Tracks 10 and 10b).

use super::{Rgba, SIZE, noisy, rnd, shade};
use crate::world::overworld_blocks::t;

const CLEAR: Rgba = [0, 0, 0, 0];
const MOSS: [u8; 3] = [89, 109, 45];
const PALE_MOSS: [u8; 3] = [108, 118, 104];
const DRIPSTONE: [u8; 3] = [134, 107, 92];
const PRISMARINE: [u8; 3] = [99, 156, 151];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    match layer {
        t::MOSS => mossy(layer, x, y, MOSS),
        t::PALE_MOSS => mossy(layer, x, y, PALE_MOSS),
        t::DRIPSTONE => {
            let band = (y + (rnd(layer, x / 4, 0, 2) * 3.0) as usize).is_multiple_of(5);
            shade(DRIPSTONE, if band { 0.8 } else { 0.92 + r * 0.14 })
        }
        l if (t::POINTED..t::POINTED + 10).contains(&l) => pointed(l - t::POINTED, x, y, r),
        l if (t::BUDS..t::BUDS + 8).contains(&l) => {
            let i = l - t::BUDS;
            bud(i % 4, i >= 4, x, y, r)
        }
        t::AZALEA_TOP | t::FLOWERING_AZALEA_TOP => {
            let flowers = layer == t::FLOWERING_AZALEA_TOP && rnd(layer, x / 2, y / 2, 3) < 0.25;
            if flowers { shade([206, 112, 196], 0.9 + r * 0.2) } else { shade([96, 126, 46], 0.8 + r * 0.3) }
        }
        t::AZALEA_SIDE | t::FLOWERING_AZALEA_SIDE => {
            // Leaves on top, a short trunk below.
            if y < 9 {
                let flowers = layer == t::FLOWERING_AZALEA_SIDE && rnd(layer, x / 2, y / 2, 3) < 0.2;
                if flowers { shade([206, 112, 196], 0.9 + r * 0.2) } else { shade([96, 126, 46], 0.8 + r * 0.3) }
            } else if (6..=9).contains(&x) {
                shade([96, 74, 44], 0.85 + r * 0.2)
            } else {
                CLEAR
            }
        }
        t::BIG_DRIPLEAF_TOP => {
            let vein = x == 8 || (x as i32 - 8).abs() == (y as i32 - 8).abs();
            shade([112, 160, 40], if vein { 0.8 } else { 0.95 + r * 0.12 })
        }
        t::BIG_DRIPLEAF_SIDE => shade([96, 140, 34], 0.9 + r * 0.1),
        t::DRIED_KELP_SIDE => {
            let strap = y.is_multiple_of(4);
            shade([58, 64, 36], if strap { 0.7 } else { 0.9 + r * 0.15 })
        }
        t::DRIED_KELP_TOP => {
            let ring = (x as i32 - 8).abs().max((y as i32 - 8).abs());
            shade([66, 72, 40], if ring % 3 == 0 { 0.75 } else { 0.95 + r * 0.1 })
        }
        t::PRISMARINE => {
            let tint = (rnd(layer, x / 3, y / 3, 3) * 3.0) as usize;
            let c = [PRISMARINE, [86, 140, 150], [110, 170, 140]][tint];
            shade(c, 0.88 + r * 0.16)
        }
        t::DARK_PRISMARINE => {
            let seam = x.is_multiple_of(8) || y.is_multiple_of(8);
            shade([51, 92, 75], if seam { 0.7 } else { 0.9 + r * 0.12 })
        }
        t::SEA_LANTERN => {
            let (cx, cy) = (x as i32 - 8, y as i32 - 8);
            let core = cx.abs() + cy.abs() < 6;
            let frame = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            let c = if frame {
                [140, 176, 166]
            } else if core {
                [236, 246, 240]
            } else {
                [196, 220, 212]
            };
            shade(c, 0.95 + r * 0.06)
        }
        t::SPONGE | t::WET_SPONGE => {
            let hole = rnd(layer, x, y, 9) < 0.18;
            let c = if layer == t::SPONGE { [196, 192, 74] } else { [170, 170, 60] };
            shade(c, if hole { 0.62 } else { 0.9 + r * 0.15 })
        }
        l if (t::TURTLE_EGGS..t::TURTLE_EGGS + 4).contains(&l) => {
            let n = (l - t::TURTLE_EGGS) as usize + 1;
            const EGGS: [(f32, f32); 4] = [(8.0, 12.0), (4.0, 13.0), (12.0, 13.0), (8.5, 7.0)];
            let inside = EGGS[..n].iter().find(|&&(cx, cy)| (fx - cx).powi(2) + ((fy - cy) * 0.8).powi(2) < 6.0);
            match inside {
                Some(_) if rnd(l, x, y, 3) < 0.22 => shade([100, 150, 90], 0.9),
                Some(_) => shade([236, 232, 212], 0.92 + r * 0.08),
                None => CLEAR,
            }
        }
        t::BAMBOO_PLANKS => planks(layer, x, y, [196, 176, 82]),
        t::MANGROVE_ROOTS_SIDE | t::MANGROVE_ROOTS_TOP => {
            let root = (x + y).is_multiple_of(5) || (x + 16 - y).is_multiple_of(6);
            if root { shade([90, 70, 48], 0.85 + r * 0.25) } else { CLEAR }
        }
        t::MUDDY_ROOTS_SIDE | t::MUDDY_ROOTS_TOP => {
            let root = (x + y).is_multiple_of(5) || (x + 16 - y).is_multiple_of(6);
            if root { shade([96, 74, 50], 0.85 + r * 0.25) } else { noisy(layer, x, y, [60, 57, 60], 0.12) }
        }
        t::BAMBOO_BLOCK_SIDE => {
            let groove = x.is_multiple_of(4);
            let node = y % 8 == 3;
            shade([126, 140, 46], if groove || node { 0.78 } else { 0.92 + r * 0.12 })
        }
        t::BAMBOO_BLOCK_TOP => {
            let ring = (x.is_multiple_of(8) || y.is_multiple_of(8)) as u8;
            shade([170, 168, 80], if ring == 1 { 0.8 } else { 0.95 + r * 0.08 })
        }
        t::BAMBOO_MOSAIC => {
            let tile = ((x / 4) + (y / 8) * 2).is_multiple_of(2);
            let seam = x.is_multiple_of(4) || y.is_multiple_of(8);
            shade(
                [196, 176, 82],
                if seam {
                    0.7
                } else if tile {
                    0.95
                } else {
                    0.85
                },
            )
        }
        l if (t::COCOA..t::COCOA + 3).contains(&l) => {
            let age = l - t::COCOA;
            let c = [[110, 130, 40], [170, 110, 50], [140, 76, 36]][age as usize];
            let ridge = x.is_multiple_of(3);
            shade(c, if ridge { 0.78 } else { 0.92 + r * 0.12 })
        }
        t::RED_MUSHROOM_BLOCK => {
            let spot = rnd(layer, x / 3, y / 3, 5) < 0.25 && rnd(layer, x, y, 6) < 0.8;
            if spot { shade([230, 224, 218], 0.95) } else { shade([180, 30, 30], 0.88 + r * 0.14) }
        }
        t::BROWN_MUSHROOM_BLOCK => noisy(layer, x, y, [150, 112, 82], 0.12),
        t::MUSHROOM_STEM => {
            let groove = x % 4 == 1;
            shade([206, 198, 186], if groove { 0.88 } else { 0.95 + r * 0.06 })
        }
        _ => {
            if (x / 4 + y / 4).is_multiple_of(2) {
                [255, 0, 255, 255]
            } else {
                [0, 0, 0, 255]
            }
        }
    }
}

fn mossy(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let tuft = rnd(layer, x / 2, y / 2, 3);
    shade(
        c,
        if tuft < 0.2 {
            0.78
        } else if tuft > 0.85 {
            1.15
        } else {
            0.92 + rnd(layer, x, y, 0) * 0.14
        },
    )
}

fn planks(layer: u16, x: usize, y: usize, colour: [u8; 3]) -> Rgba {
    let board = y / 4;
    let seam = y % 4 == 3 || x == (board * 7 + 3) % SIZE;
    shade(colour, if seam { 0.68 } else { 0.92 + rnd(layer, x, board, 2) * 0.12 })
}

/// Pointed dripstone: `i` 0..5 points up (tip, tip merge, frustum,
/// middle, base), 5..10 the same pointing down.
fn pointed(i: u16, x: usize, y: usize, r: f32) -> Rgba {
    let down = i >= 5;
    // Distance from the tip end of the cell: an upward tip is at the top.
    let from_tip = if down { SIZE - 1 - y } else { y } as f32;
    let half = match i % 5 {
        0 => from_tip * 0.22,
        1 => (from_tip * 0.22).min(2.5),
        2 => 1.5 + from_tip * 0.18,
        3 => 3.5,
        _ => 3.5 + from_tip * 0.12,
    };
    if (x as f32 + 0.5 - 8.0).abs() < half.max(0.6) {
        let groove = x.is_multiple_of(3);
        shade(DRIPSTONE, if groove { 0.8 } else { 0.92 + r * 0.14 })
    } else {
        CLEAR
    }
}

fn bud(size: u16, down: bool, x: usize, y: usize, r: f32) -> Rgba {
    let tall = [4.0, 7.0, 10.0, 13.0][size as usize];
    let up_y = if down { y } else { SIZE - 1 - y } as f32;
    let spikes: &[(f32, f32)] = match size {
        3 => &[(8.0, 1.0), (4.5, 0.75), (11.5, 0.8)],
        2 => &[(8.0, 1.0), (5.0, 0.7)],
        _ => &[(8.0, 1.0)],
    };
    for &(cx, scale) in spikes {
        let h = tall * scale;
        if up_y < h {
            let half = 1.5 + (h - up_y) * 0.25;
            if (x as f32 + 0.5 - cx).abs() < half.min(2.6) {
                return shade([180, 140, 230], 0.85 + r * 0.25);
            }
        }
    }
    CLEAR
}
