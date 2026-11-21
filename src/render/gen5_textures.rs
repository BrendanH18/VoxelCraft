//! Original procedural textures of v0.16 Track D (`world::wood::poplar::t`
//! and `world::gen5_blocks::t`, layers 749..=948): poplar bark and wood,
//! its three leaf colours, cinnabar and sulfur stones, potent sulfur,
//! sulfur spikes, nether gold ore, the red shrub, the golden dandelion and
//! the shelf mushroom. Every pattern is drawn here; nothing is copied.

use super::{Rgba, noisy, rnd, shade};
use crate::world::gen5_blocks::t as g;
use crate::world::wood::poplar::t as p;

const CLEAR: Rgba = [0, 0, 0, 0];
const BARK: [u8; 3] = [212, 208, 194];
const BARK_MARK: [u8; 3] = [74, 70, 62];
const STRIPPED: [u8; 3] = [230, 218, 186];
const PLANKS: [u8; 3] = [204, 194, 166];
const LEAVES: [[u8; 3]; 3] = [[226, 128, 38], [194, 54, 36], [234, 196, 54]];
const CINNABAR: [u8; 3] = [164, 38, 32];
const SULFUR: [u8; 3] = [220, 202, 74];

/// Whether `layer` is one of Track D's.
pub(super) fn owns(layer: u16) -> bool {
    (p::BASE..g::END).contains(&layer)
}

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        p::LOG_SIDE => bark(layer, x, y, r),
        p::LOG_TOP => log_top(layer, x, y, r),
        p::LOG_SIDE_TURNED => bark(layer, y, x, r),
        p::STRIPPED_SIDE => stripped(layer, x, y, r),
        p::STRIPPED_TOP => rings(layer, x, y, r, STRIPPED, STRIPPED),
        p::STRIPPED_SIDE_TURNED => stripped(layer, y, x, r),
        p::PLANKS => planks(layer, x, y, PLANKS),
        l if (p::LEAVES..p::LEAVES + 3).contains(&l) => leaves(layer, x, y, r, LEAVES[(l - p::LEAVES) as usize]),
        p::SAPLING => sapling(x, y, r),
        p::DOOR_BOTTOM | p::DOOR_TOP => door(layer, x, y, r, layer == p::DOOR_TOP),
        p::TRAPDOOR => trapdoor(x, y, r),
        p::SIGN_ICON => sign_icon(x, y, r),
        p::HANGING_SIGN_ICON => hanging_sign_icon(x, y, r),
        p::SHELF => shelf_body(layer, x, y),
        l if (p::SHELF_FRONTS..p::SHELF_FRONTS + 5).contains(&l) => shelf_front(layer, l - p::SHELF_FRONTS, x, y),
        p::POTTED_SAPLING => super::utility::potted_side(layer, crate::world::wood::poplar::SAPLING, x, y),
        l if (g::FULL..g::FULL + 8).contains(&l) => stone(layer, l - g::FULL, x, y, r),
        g::CHISELED_CINNABAR_TOP => chiseled_top(layer, x, y, CINNABAR),
        g::CHISELED_SULFUR_TOP => chiseled_top(layer, x, y, SULFUR),
        l if (g::POTENT..g::POTENT + 3).contains(&l) => potent(layer, l - g::POTENT, x, y, r),
        l if (g::SPIKE..g::SPIKE + 5).contains(&l) => spike(l - g::SPIKE, x, y, r),
        g::NETHER_GOLD_ORE => nether_gold_ore(layer, x, y, r),
        g::RED_SHRUB => red_shrub(layer, x, y, r),
        g::GOLDEN_DANDELION => golden_dandelion(x, y, r),
        g::POTTED_GOLDEN_DANDELION => {
            super::utility::potted_side(layer, crate::world::gen5_blocks::GOLDEN_DANDELION, x, y)
        }
        g::SHELF_MUSHROOM => shelf_mushroom(layer, x, y, r, false),
        g::SHELF_MUSHROOM_TOP => shelf_mushroom(layer, x, y, r, true),
        _ => [255, 0, 255, 255],
    }
}

/// Pale bark with dark, diamond-shaped lenticels in staggered rows.
fn bark(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let row = y / 5;
    let offset = (row * 3 + (rnd(layer, 0, row, 4) * 4.0) as usize) % 16;
    let dx = (x as i32 - offset as i32).rem_euclid(8);
    let mark = y % 5 == 2 && (2..=4).contains(&dx) || y % 5 == 1 && dx == 3 || y % 5 == 3 && dx == 3;
    if mark {
        shade(BARK_MARK, 0.85 + r * 0.25)
    } else {
        let streak = (x + (rnd(layer, x / 3, 0, 2) * 3.0) as usize).is_multiple_of(6);
        shade(BARK, if streak { 0.86 } else { 0.94 + r * 0.08 })
    }
}

fn rings(layer: u16, x: usize, y: usize, r: f32, rim: [u8; 3], core: [u8; 3]) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
    let d = (dx * dx + dy * dy).sqrt().max(dx.abs().max(dy.abs()) * 0.98);
    if d > 7.0 {
        shade(rim, 0.86 + r * 0.1)
    } else {
        let ring = (d * 1.2) as i32 % 2 == 0;
        noisy(layer, x, y, core, 0.03).map(|v| if ring { (v as f32 * 0.9) as u8 } else { v })
    }
}

fn log_top(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    if x == 0 || y == 0 || x == 15 || y == 15 {
        return bark(layer, x, y, r);
    }
    rings(layer, x, y, r, STRIPPED, [222, 206, 168])
}

fn stripped(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let grain = (x + (rnd(layer, 0, y / 4, 3) * 2.0) as usize).is_multiple_of(4);
    shade(STRIPPED, if grain { 0.88 } else { 0.95 + r * 0.08 })
}

fn planks(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let board = y / 4;
    let seam = y % 4 == 3 || x == (board * 5 + 6) % 16;
    shade(c, if seam { 0.7 } else { 0.92 + rnd(layer, x, board, 2) * 0.12 })
}

/// Small rounded leaves with a few gaps.
fn leaves(layer: u16, x: usize, y: usize, r: f32, c: [u8; 3]) -> Rgba {
    if rnd(layer, x / 2, y / 2, 11) < 0.14 && rnd(layer, x, y, 12) < 0.7 {
        CLEAR
    } else {
        let vein = (x + y).is_multiple_of(5);
        shade(c, if vein { 0.78 } else { 0.82 + r * 0.32 })
    }
}

/// A slim sapling: a pale stem under a narrow, upright crown.
fn sapling(x: usize, y: usize, r: f32) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 6.0);
    if dx * dx * 2.2 + dy * dy * 0.7 < 14.0 && !(x * 3 + y).is_multiple_of(5) {
        shade(LEAVES[0], 0.8 + r * 0.3)
    } else if (7..=8).contains(&x) && y >= 9 {
        shade(BARK, 0.8 + r * 0.15)
    } else {
        CLEAR
    }
}

/// A door of vertical boards with a narrow window pair on the top half.
fn door(layer: u16, x: usize, y: usize, r: f32, top: bool) -> Rgba {
    let frame = x <= 1 || x >= 14 || (top && y <= 1) || (!top && y >= 14);
    if top && (4..=10).contains(&y) && matches!(x, 4..=6 | 9..=11) {
        return CLEAR;
    }
    let seam = x.is_multiple_of(4);
    let board = 0.92 + rnd(layer, x / 4, 0, 5) * 0.1 + r * 0.04;
    shade(
        PLANKS,
        if frame {
            0.78
        } else if seam {
            0.7
        } else {
            board
        },
    )
}

/// A trapdoor with a tall, narrow slot either side of a centre board.
fn trapdoor(x: usize, y: usize, r: f32) -> Rgba {
    let frame = x < 2 || y < 2 || x > 13 || y > 13;
    if !frame && matches!(x, 4..=5 | 10..=11) && (4..=11).contains(&y) {
        return CLEAR;
    }
    shade(PLANKS, if frame { 0.8 } else { 0.94 + r * 0.08 })
}

fn sign_icon(x: usize, y: usize, r: f32) -> Rgba {
    let board = (1..=14).contains(&x) && (2..=10).contains(&y);
    let post = (7..=8).contains(&x) && (11..=15).contains(&y);
    if board {
        let edge = x == 1 || x == 14 || y == 2 || y == 10;
        shade(PLANKS, if edge { 0.72 } else { 0.95 + r * 0.1 })
    } else if post {
        shade(PLANKS, 0.64 + r * 0.08)
    } else {
        CLEAR
    }
}

fn hanging_sign_icon(x: usize, y: usize, r: f32) -> Rgba {
    let chain = matches!(x, 3 | 12) && y <= 5;
    let board = (1..=14).contains(&x) && (6..=14).contains(&y);
    if chain {
        shade([96, 98, 108], if y.is_multiple_of(2) { 0.8 } else { 1.0 })
    } else if board {
        let edge = x == 1 || x == 14 || y == 6 || y == 14;
        shade(STRIPPED, if edge { 0.74 } else { 0.95 + r * 0.1 })
    } else {
        CLEAR
    }
}

fn shelf_body(layer: u16, x: usize, y: usize) -> Rgba {
    let grain = (y + (rnd(layer, x / 5, 0, 3) * 2.0) as usize).is_multiple_of(4);
    noisy(layer, x, y, if grain { [200, 190, 162] } else { STRIPPED }, 0.06)
}

/// The front, unpowered or lit along its panel as the chain runs (variants
/// as in `wood::tex::SHELF_FRONTS`).
fn shelf_front(layer: u16, variant: u16, x: usize, y: usize) -> Rgba {
    if y <= 3 || y >= 12 {
        let edge = y == 0 || y == 3 || y == 12 || y == 15;
        return noisy(layer, x, y, STRIPPED, 0.05).map(|v| if edge { (v as f32 * 0.82) as u8 } else { v });
    }
    let panel = shade(STRIPPED, 0.64 + rnd(layer, x, y, 1) * 0.06);
    let (from, to) = match variant {
        0 => return panel,
        1 => (2, 13),
        2 => (2, 15),
        3 => (0, 15),
        _ => (0, 13),
    };
    if (6..=9).contains(&y) && (from..=to).contains(&x) {
        shade([250, 210, 130], 0.92 + rnd(layer, x, y, 2) * 0.08)
    } else {
        panel
    }
}

/// Cinnabar (red with darker veins) and sulfur (yellow, crumbly), each as
/// the raw stone, polished, bricks and chiseled.
fn stone(layer: u16, i: u16, x: usize, y: usize, r: f32) -> Rgba {
    let c = if i < 4 { CINNABAR } else { SULFUR };
    match i % 4 {
        0 => {
            let vein = rnd(layer, x / 2, y / 3, 6) > 0.82;
            let speck = rnd(layer, x, y, 7) > 0.93;
            shade(
                c,
                if vein {
                    0.7
                } else if speck {
                    1.15
                } else {
                    0.86 + r * 0.2
                },
            )
        }
        1 => {
            let edge = x == 0 || y == 0 || x == 15 || y == 15;
            shade(c, if edge { 0.78 } else { 0.98 + r * 0.06 })
        }
        2 => {
            let row = y / 4;
            let mortar = y % 4 == 3 || (x + if row.is_multiple_of(2) { 0 } else { 4 }) % 8 == 7;
            shade(c, if mortar { 0.66 } else { 0.92 + r * 0.12 })
        }
        _ => {
            let frame = x <= 1 || y <= 1 || x >= 14 || y >= 14;
            let (dx, dy) = ((x as i32 - 8).abs(), (y as i32 - 8).abs());
            let carved = dx + dy == 4 || dx + dy == 5;
            shade(
                c,
                if frame {
                    0.8
                } else if carved {
                    0.62
                } else {
                    0.95 + r * 0.06
                },
            )
        }
    }
}

fn chiseled_top(layer: u16, x: usize, y: usize, c: [u8; 3]) -> Rgba {
    let frame = x <= 1 || y <= 1 || x >= 14 || y >= 14;
    let ring = {
        let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
        ((dx * dx + dy * dy).sqrt() - 3.5).abs() < 0.8
    };
    shade(
        c,
        if frame {
            0.8
        } else if ring {
            0.64
        } else {
            0.94 + rnd(layer, x, y, 1) * 0.08
        },
    )
}

/// Potent sulfur: dense golden sulfur, pocked with vents that darken when
/// dry and shine when water stands over it (`state` 0 dry, 1 wet, 2 active).
fn potent(layer: u16, state: u16, x: usize, y: usize, r: f32) -> Rgba {
    let (dx, dy) = ((x as i32 * 5 + 3) % 8 - 4, (y as i32 * 3 + 1) % 7 - 3);
    let vent = dx * dx + dy * dy < 3 && rnd(layer, x / 4, y / 4, 9) > 0.35;
    if vent {
        return match state {
            0 => shade([110, 84, 26], 0.9 + r * 0.1),
            1 => shade([96, 196, 150], 0.9 + r * 0.15),
            _ => shade([240, 250, 210], 0.95 + r * 0.05),
        };
    }
    shade([232, 176, 42], 0.84 + r * 0.24)
}

/// A sulfur spike pointing up, `thickness` 0 tip, 1 tip merge, 2 frustum,
/// 3 middle, 4 base (down-pointing spikes draw the same faces).
fn spike(thickness: u16, x: usize, y: usize, r: f32) -> Rgba {
    let half = match thickness {
        0 => 1.0 + (16 - y) as f32 * 0.15,
        1 => 1.5,
        2 => 2.0 + y as f32 * 0.12,
        3 => 3.5,
        _ => 4.5 + y as f32 * 0.1,
    };
    let half = if thickness == 0 && y < 4 { 0.0 } else { half };
    let dx = (x as f32 - 7.5).abs();
    if dx <= half {
        let ridge = dx + 0.6 > half;
        shade(SULFUR, if ridge { 0.76 } else { 0.9 + r * 0.2 })
    } else {
        CLEAR
    }
}

/// Netherrack with flecks of gold.
fn nether_gold_ore(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let fleck = rnd(layer, x / 2, y / 2, 5) > 0.8 && rnd(layer, x, y, 6) > 0.35;
    if fleck { shade([244, 198, 62], 0.9 + r * 0.2) } else { shade([112, 46, 44], 0.78 + r * 0.32) }
}

/// A low, round bush of small red leaves.
fn red_shrub(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 10.0);
    if dx * dx + dy * dy * 1.6 < 40.0 && rnd(layer, x, y, 3) > 0.18 {
        shade([176, 40, 34], 0.72 + r * 0.4)
    } else if (7..=8).contains(&x) && y >= 14 {
        shade([86, 54, 34], 0.9)
    } else {
        CLEAR
    }
}

/// A dandelion head of gold leaf on a green stem.
fn golden_dandelion(x: usize, y: usize, r: f32) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 5.5);
    let d = dx * dx + dy * dy;
    if d < 7.5 {
        let glint = (x + y).is_multiple_of(3);
        shade([246, 204, 60], if glint { 1.12 } else { 0.86 + r * 0.2 })
    } else if (7..=8).contains(&x) && y >= 8 {
        shade([82, 138, 48], 0.85 + r * 0.2)
    } else if y >= 11 && (x == 5 || x == 10) {
        shade([82, 138, 48], 0.8)
    } else {
        CLEAR
    }
}

/// A bracket fungus: tan bands with a pale rim.
fn shelf_mushroom(layer: u16, x: usize, y: usize, r: f32, top: bool) -> Rgba {
    if top {
        let (dx, dy) = (x as f32 - 7.5, y as f32);
        let band = ((dx * dx + dy * dy).sqrt() as usize).is_multiple_of(3);
        return shade(if band { [150, 96, 48] } else { [190, 134, 70] }, 0.9 + r * 0.15);
    }
    let rim = y.is_multiple_of(4);
    noisy(layer, x, y, if rim { [226, 196, 140] } else { [176, 118, 60] }, 0.08)
}
