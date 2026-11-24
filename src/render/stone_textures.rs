//! Original procedural textures for the v0.12 stone and minerals blocks
//! (`world::stone_blocks::t`), in the noisy style of [`super::pixel`].

use super::{Rgba, noisy, rnd, shade};
use crate::world::stone_blocks::t;

const QUARTZ: [u8; 3] = [236, 230, 222];

/// Running-bond bricks `w` by `h` pixels: whether (x, y) is mortar, and
/// the brick it belongs to.
fn brick(x: usize, y: usize, w: usize, h: usize) -> (bool, usize, usize) {
    let row = y / h;
    let shift = if row.is_multiple_of(2) { 0 } else { w / 2 };
    let col = (x + shift) / w;
    let mortar = y % h == h - 1 || (x + shift) % w == w - 1;
    (mortar, col, row)
}

/// A brick face: mortar `joint` times darker, each brick slightly varied
/// and lit on its top-left edge.
fn bricks(layer: u16, x: usize, y: usize, w: usize, h: usize, c: [u8; 3], joint: f32) -> Rgba {
    let (mortar, col, row) = brick(x, y, w, h);
    let r = rnd(layer, x, y, 0);
    if mortar {
        return shade(c, joint + r * 0.06);
    }
    let lit = y.is_multiple_of(h) || (x + if (y / h).is_multiple_of(2) { 0 } else { w / 2 }).is_multiple_of(w);
    shade(c, 0.86 + rnd(layer, col, row, 1) * 0.14 + r * 0.06 + if lit { 0.08 } else { 0.0 })
}

/// Concentric carved squares around the centre: 0 on the rim, rising inward.
fn ring(x: usize, y: usize) -> usize {
    let dx = if x < 8 { x } else { 15 - x };
    let dy = if y < 8 { y } else { 15 - y };
    dx.min(dy)
}

/// Pillar grooves running along `along_y` (vertical in the texture) or across.
fn pillar(layer: u16, x: usize, y: usize, along_y: bool) -> Rgba {
    let across = if along_y { x } else { y };
    let groove = across % 5 == 0;
    let rim = across == 15;
    shade(
        QUARTZ,
        if groove {
            0.84
        } else if rim {
            0.88
        } else {
            0.96 + rnd(layer, x, y, 0) * 0.04
        },
    )
}

/// A faceted gem block: rim, alternating facets and a few sparkles.
fn gem(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let rim = x == 0 || y == 0 || x == 15 || y == 15;
    let (cx, cy) = (x as i32 - 8, y as i32 - 8);
    let facet = (cx + cy).rem_euclid(8) < 4;
    let spark = rnd(layer, x, y, 9) > 0.93;
    if spark && !rim {
        return shade([250, 255, 255], 0.95);
    }
    shade(
        c,
        if rim {
            0.68
        } else if facet {
            1.0
        } else {
            0.86 + rnd(layer, x, y, 0) * 0.06
        },
    )
}

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        t::QUARTZ_SIDE | t::QUARTZ_TOP => {
            let rim = layer == t::QUARTZ_TOP && (x == 0 || y == 0 || x == 15 || y == 15);
            shade(QUARTZ, if rim { 0.9 } else { 0.96 + rnd(layer, x / 2, y / 2, 3) * 0.04 })
        }
        t::QUARTZ_BOTTOM => noisy(layer, x, y, QUARTZ, 0.02),
        t::QUARTZ_BRICKS => bricks(layer, x, y, 8, 4, QUARTZ, 0.85),
        t::CHISELED_QUARTZ => {
            let band = !(2..=13).contains(&y);
            let scroll = !band && ((x + y).is_multiple_of(6) || (x + 15 - y).is_multiple_of(6)) && (3..13).contains(&x);
            shade(QUARTZ, if band || scroll { 0.84 } else { 0.97 + r * 0.03 })
        }
        t::CHISELED_QUARTZ_TOP => shade(QUARTZ, if matches!(ring(x, y), 0 | 3 | 6) { 0.84 } else { 0.97 + r * 0.03 }),
        t::QUARTZ_PILLAR_SIDE => pillar(layer, x, y, true),
        t::QUARTZ_PILLAR_SIDE_TURNED => pillar(t::QUARTZ_PILLAR_SIDE, y, x, true),
        t::QUARTZ_PILLAR_TOP => shade(QUARTZ, if matches!(ring(x, y), 0 | 4) { 0.84 } else { 0.97 + r * 0.03 }),
        t::COAL_BLOCK => {
            let (cx, cy) = (x as i32 - 8, y as i32 - 8);
            let facet = (cx * 2 + cy).rem_euclid(9) < 3;
            shade([30, 30, 32], if facet { 1.35 + r * 0.2 } else { 0.85 + r * 0.25 })
        }
        t::DIAMOND_BLOCK => gem(layer, x, y, [98, 226, 220]),
        t::TINTED_GLASS => {
            let rim = x == 0 || y == 0 || x == 15 || y == 15;
            let streak = (x + y == 5 || x + y == 6) && (1..6).contains(&x);
            let mut p = shade(
                [46, 38, 56],
                if rim {
                    1.15
                } else if streak {
                    1.6
                } else {
                    0.9 + r * 0.1
                },
            );
            p[3] = if rim { 255 } else { 205 };
            p
        }
        t::SLIME_BLOCK => {
            // A clear outer jelly around a denser core (Java's model has an
            // inner cube from 3 to 13).
            let rim = x == 0 || y == 0 || x == 15 || y == 15;
            let core = (3..13).contains(&x) && (3..13).contains(&y);
            let core_edge = core && (x == 3 || y == 3 || x == 12 || y == 12);
            let mut p = shade(
                [112, 190, 90],
                if rim {
                    1.1
                } else if core_edge {
                    0.7
                } else if core {
                    0.88 + r * 0.08
                } else {
                    1.0 + r * 0.06
                },
            );
            p[3] = if rim || core { 230 } else { 160 };
            p
        }
        _ => [255, 0, 255, 255],
    }
}
