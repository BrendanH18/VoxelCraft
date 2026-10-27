//! Original procedural textures for the v0.12 stone and minerals blocks
//! (`world::stone_blocks::t`), in the noisy style of [`super::pixel`].

use super::{Rgba, noisy, rnd, shade};
use crate::world::stone_blocks::t;

const TUFF: [u8; 3] = [108, 109, 102];
const DEEPSLATE: [u8; 3] = [72, 72, 78];
const QUARTZ: [u8; 3] = [236, 230, 222];
const SANDSTONE: [u8; 3] = [216, 203, 155];
const RED_SANDSTONE: [u8; 3] = [186, 99, 29];
const NETHER_BRICK: [u8; 3] = [68, 34, 40];
const RED_NETHER_BRICK: [u8; 3] = [112, 14, 18];
const END_STONE: [u8; 3] = [220, 224, 164];
const STONE_BRICK: [u8; 3] = [122, 121, 122];

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

/// A jagged crack across a brick or tile face.
fn cracked(layer: u16, x: usize, y: usize) -> bool {
    let wander = |v: usize| (rnd(layer, v / 3, 0, 7) * 3.0) as usize;
    let first = (x + 3 + wander(y)) % 16 == 8 && (2..14).contains(&y);
    let second = (y + 2 + wander(x)) % 16 == 11 && (1..9).contains(&x);
    first || second
}

/// Concentric carved squares around the centre: 0 on the rim, rising inward.
fn ring(x: usize, y: usize) -> usize {
    let dx = if x < 8 { x } else { 15 - x };
    let dy = if y < 8 { y } else { 15 - y };
    dx.min(dy)
}

/// A smooth, bevelled slab of stone: lighter top-left rim, darker bottom-right.
fn polished(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let r = rnd(layer, x / 2, y / 2, 4);
    let f = if x == 0 || y == 0 {
        1.08
    } else if x == 15 || y == 15 {
        0.78
    } else {
        0.94 + r * 0.08
    };
    shade(c, f)
}

/// Sandstone-like side: soft layered bands.
fn layered(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let band = matches!(y, 3 | 4 | 10);
    shade(c, if band { 0.88 } else { 0.96 + rnd(layer, x, y, 0) * 0.06 })
}

/// Cut sandstone: smooth face with two cut lines and a rim.
fn cut(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let line = y == 1 || y == 14;
    let rim = y == 0 || y == 15;
    shade(
        c,
        if rim {
            0.84
        } else if line {
            0.9
        } else {
            0.97 + rnd(layer, x, y, 0) * 0.05
        },
    )
}

/// Chiseled sandstone: bands top and bottom around a carved diamond.
fn chiseled_sand(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    if !(3..=12).contains(&y) {
        return shade(c, if y == 2 || y == 13 { 0.82 } else { 0.96 + rnd(layer, x, y, 0) * 0.05 });
    }
    let (cx, cy) = (x as i32 * 2 - 15, y as i32 * 2 - 15);
    let d = cx.abs() + cy.abs();
    let groove = (9..=11).contains(&d) || d <= 2;
    shade(c, if groove { 0.74 } else { 0.95 + rnd(layer, x, y, 0) * 0.05 })
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
        t::POLISHED_TUFF => polished(layer, x, y, [118, 120, 112]),
        t::TUFF_BRICKS => bricks(layer, x, y, 8, 4, TUFF, 0.62),
        t::CHISELED_TUFF => {
            let band = !(3..=12).contains(&y);
            let frame = !band && (x == 2 || x == 13 || y == 4 || y == 11);
            shade(
                TUFF,
                if band {
                    0.88 + r * 0.1
                } else if frame {
                    0.72
                } else {
                    0.98 + r * 0.08
                },
            )
        }
        t::CHISELED_TUFF_TOP => shade(TUFF, if ring(x, y).is_multiple_of(3) { 0.72 } else { 0.95 + r * 0.08 }),
        t::CHISELED_TUFF_BRICKS => {
            if !(4..=11).contains(&y) {
                bricks(layer, x, y, 8, 4, TUFF, 0.62)
            } else {
                let carve = ring(x, y) == 5 || (ring(x, y) > 5 && (x + y).is_multiple_of(3));
                shade([118, 120, 112], if carve { 0.7 } else { 0.96 + r * 0.06 })
            }
        }
        t::CHISELED_TUFF_BRICKS_TOP => {
            let k = ring(x, y);
            shade([118, 120, 112], if k == 0 || k == 4 { 0.68 } else { 0.94 + r * 0.08 })
        }
        t::DEEPSLATE_BRICKS => bricks(layer, x, y, 8, 4, DEEPSLATE, 0.5),
        t::CRACKED_DEEPSLATE_BRICKS => {
            if cracked(layer, x, y) {
                shade(DEEPSLATE, 0.38)
            } else {
                bricks(t::DEEPSLATE_BRICKS, x, y, 8, 4, DEEPSLATE, 0.5)
            }
        }
        t::DEEPSLATE_TILES => {
            let joint = x % 8 == 7 || y % 8 == 7;
            shade([58, 58, 64], if joint { 0.55 } else { 0.9 + rnd(layer, x / 8, y / 8, 1) * 0.12 + r * 0.06 })
        }
        t::CRACKED_DEEPSLATE_TILES => {
            if cracked(layer, x, y) {
                shade([58, 58, 64], 0.4)
            } else {
                pixel(t::DEEPSLATE_TILES, x, y)
            }
        }
        t::CHISELED_DEEPSLATE => shade(DEEPSLATE, if ring(x, y).is_multiple_of(2) { 0.62 } else { 0.95 + r * 0.08 }),
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
        t::RED_SANDSTONE => layered(layer, x, y, RED_SANDSTONE),
        t::RED_SANDSTONE_TOP => noisy(layer, x, y, [192, 104, 34], 0.04),
        t::RED_SANDSTONE_BOTTOM => noisy(layer, x, y, [180, 95, 28], 0.08),
        t::CUT_RED_SANDSTONE => cut(layer, x, y, RED_SANDSTONE),
        t::CHISELED_RED_SANDSTONE => chiseled_sand(layer, x, y, RED_SANDSTONE),
        t::CUT_SANDSTONE => cut(layer, x, y, SANDSTONE),
        t::CHISELED_SANDSTONE => chiseled_sand(layer, x, y, SANDSTONE),
        t::RED_NETHER_BRICKS => bricks(layer, x, y, 8, 4, RED_NETHER_BRICK, 0.45),
        t::CHISELED_NETHER_BRICKS => {
            let k = ring(x, y);
            if k < 2 {
                bricks(layer, x, y, 8, 4, NETHER_BRICK, 0.45)
            } else {
                shade(NETHER_BRICK, if k == 2 || k == 5 { 0.55 } else { 0.95 + r * 0.12 })
            }
        }
        t::CRACKED_NETHER_BRICKS => {
            if cracked(layer, x, y) {
                shade(NETHER_BRICK, 0.4)
            } else {
                bricks(layer, x, y, 8, 4, NETHER_BRICK, 0.45)
            }
        }
        t::END_STONE_BRICKS => bricks(layer, x, y, 8, 4, END_STONE, 0.72),
        t::CHISELED_STONE_BRICKS => {
            let k = ring(x, y);
            let f = match k {
                0 => 0.62,
                1 => 1.08,
                3 | 6 => 0.72,
                _ => 0.94 + r * 0.08,
            };
            shade(STONE_BRICK, f)
        }
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
