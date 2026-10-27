//! Original procedural textures of the v0.12 wood track (`world::wood::tex`):
//! stripped logs, quarter-turned log sides, trapdoors, sign item sprites,
//! shelves and the chiseled bookshelf.

use super::{Rgba, noisy, pixel as base_pixel, rnd, shade};
use crate::world::block::tex as block_tex;
use crate::world::overworld_blocks::t;
use crate::world::wood::tex;

const CLEAR: Rgba = [0, 0, 0, 0];

/// Planks colour of each `wood::Kind`.
const PLANKS: [[u8; 3]; 12] = [
    [162, 130, 78],
    [114, 84, 50],
    [196, 180, 124],
    [160, 114, 80],
    [170, 92, 50],
    [66, 43, 20],
    [117, 54, 48],
    [226, 178, 172],
    [228, 218, 214],
    [196, 176, 82],
    [101, 48, 70],
    [43, 104, 99],
];

/// Stripped wood colour of each `wood::Kind`.
const STRIPPED: [[u8; 3]; 12] = [
    [177, 144, 86],
    [115, 89, 52],
    [196, 176, 118],
    [171, 132, 84],
    [174, 92, 59],
    [72, 56, 36],
    [120, 54, 48],
    [215, 145, 148],
    [240, 232, 228],
    [195, 175, 80],
    [137, 57, 90],
    [57, 150, 147],
];

/// The existing log side of each of `wood::LOGS`.
const LOG_SIDES: [u16; 10] = [
    block_tex::LOG_SIDE,
    block_tex::SPRUCE_LOG_SIDE,
    block_tex::BIRCH_LOG_SIDE,
    block_tex::JUNGLE_LOG_SIDE,
    block_tex::ACACIA_LOG_SIDE,
    block_tex::DARK_OAK_LOG_SIDE,
    block_tex::MANGROVE_LOG_SIDE,
    block_tex::CHERRY_LOG_SIDE,
    t::PALE_OAK_LOG_SIDE,
    t::BAMBOO_BLOCK_SIDE,
];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        l if l < tex::STRIPPED_TOP => stripped_side(l, (l - tex::STRIPPED_SIDE) as usize, x, y, r),
        l if l < tex::LOG_SIDE_TURNED => stripped_top((l - tex::STRIPPED_TOP) as usize, x, y, r),
        // A quarter turn: the grain runs across instead of up.
        l if l < tex::STRIPPED_SIDE_TURNED => base_pixel(LOG_SIDES[(l - tex::LOG_SIDE_TURNED) as usize], y, x),
        l if l < tex::TRAPDOORS => pixel(tex::STRIPPED_SIDE + l - tex::STRIPPED_SIDE_TURNED, y, x),
        l if l < tex::SIGN_ICONS => trapdoor((l - tex::TRAPDOORS) as usize, x, y, r),
        l if l < tex::HANGING_SIGN_ICONS => sign_icon((l - tex::SIGN_ICONS) as usize, x, y, r),
        l if l < tex::SHELF => hanging_sign_icon((l - tex::HANGING_SIGN_ICONS) as usize, x, y, r),
        l if l < tex::SHELF_FRONTS => shelf_body(l, (l - tex::SHELF) as usize, x, y),
        l if l < tex::CHISELED_TOP => {
            let i = l - tex::SHELF_FRONTS;
            shelf_front(l, (i / 5) as usize, i % 5, x, y)
        }
        l if l == tex::CHISELED_TOP => chiseled_top(l, x, y, r),
        l if l == tex::CHISELED_SIDE => chiseled_side(l, x, y, r),
        l if l < tex::END => chiseled_front(l, (l - tex::CHISELED_FRONTS) as u8, x, y, r),
        _ => [255, 0, 255, 255],
    }
}

/// Bare wood with vertical grain; the bamboo block keeps its grooves and nodes.
fn stripped_side(layer: u16, kind: usize, x: usize, y: usize, r: f32) -> Rgba {
    let c = STRIPPED[kind];
    if kind == 9 {
        let groove = x.is_multiple_of(4);
        let node = y % 8 == 3;
        return shade(c, if groove || node { 0.8 } else { 0.94 + r * 0.1 });
    }
    let grain = (x + (rnd(layer, 0, y / 5, 3) * 2.0) as usize).is_multiple_of(5);
    let knot = rnd(layer, x / 3, y / 4, 7) > 0.97;
    shade(
        c,
        if knot {
            0.75
        } else if grain {
            0.84
        } else {
            0.94 + r * 0.1
        },
    )
}

/// A cut end: a thin rim of the stripped surface around rings.
fn stripped_top(kind: usize, x: usize, y: usize, r: f32) -> Rgba {
    let c = STRIPPED[kind];
    if kind == 9 {
        let ring = x.is_multiple_of(8) || y.is_multiple_of(8);
        return shade(c, if ring { 0.82 } else { 1.0 + r * 0.06 });
    }
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
    let d = (dx * dx + dy * dy).sqrt().max(dx.abs().max(dy.abs()) * 0.98);
    if d > 7.0 {
        shade(c, 0.88 + r * 0.08)
    } else {
        let ring = (d * 1.1) as i32 % 2 == 0;
        shade(c, if ring { 1.06 } else { 0.96 + r * 0.06 })
    }
}

/// Seven original trapdoor designs, one per wood family.
fn trapdoor(family: usize, x: usize, y: usize, r: f32) -> Rgba {
    let kind = family + 1; // `wood::Kind` index: spruce is 1
    let c = PLANKS[kind];
    let frame = x < 2 || y < 2 || x > 13 || y > 13;
    let open = match family {
        // Spruce: two horizontal slits.
        0 => matches!(y, 5 | 10) && (4..=11).contains(&x),
        // Birch: a ring of small holes.
        1 => (x.is_multiple_of(4) && y % 4 == 2 || y.is_multiple_of(4) && x % 4 == 2) && !frame,
        // Jungle: a grid of square holes.
        2 => (3..=12).contains(&x) && (3..=12).contains(&y) && x % 5 != 2 && y % 5 != 2,
        // Acacia: a diamond window between vertical bars.
        3 => (x as i32 - 8).abs() + (y as i32 - 8).abs() < 5 && !x.is_multiple_of(3),
        // Dark oak: solid boards with four corner holes.
        4 => matches!((x, y), (3..=4, 3..=4) | (11..=12, 3..=4) | (3..=4, 11..=12) | (11..=12, 11..=12)),
        // Mangrove: horizontal slats.
        5 => y.is_multiple_of(3) && !frame,
        // Cherry: a four-petal cut-out.
        _ => {
            let (dx, dy) = ((x as i32 - 8).abs(), (y as i32 - 8).abs());
            (dx <= 1 && dy <= 4) || (dy <= 1 && dx <= 4)
        }
    };
    if open && !frame {
        return CLEAR;
    }
    let seam = !frame && x % 4 == 3;
    shade(
        c,
        if frame {
            0.8 + r * 0.06
        } else if seam {
            0.74
        } else {
            0.93 + r * 0.1
        },
    )
}

/// The sign item: a board on a post.
fn sign_icon(kind: usize, x: usize, y: usize, r: f32) -> Rgba {
    let c = PLANKS[kind];
    let board = (1..=14).contains(&x) && (2..=10).contains(&y);
    let post = (7..=8).contains(&x) && (11..=15).contains(&y);
    if board {
        let edge = x == 1 || x == 14 || y == 2 || y == 10;
        shade(c, if edge { 0.7 } else { 0.95 + r * 0.1 })
    } else if post {
        shade(c, 0.62 + r * 0.08)
    } else {
        CLEAR
    }
}

/// The hanging sign item: a board under two chains.
fn hanging_sign_icon(kind: usize, x: usize, y: usize, r: f32) -> Rgba {
    let c = STRIPPED[kind];
    let chain = matches!(x, 3 | 12) && y <= 5;
    let link = matches!(x, 2 | 4 | 11 | 13) && y <= 5 && y % 2 == 1;
    let board = (1..=14).contains(&x) && (6..=14).contains(&y);
    if chain || link {
        shade([96, 98, 108], if y.is_multiple_of(2) { 0.8 } else { 1.0 })
    } else if board {
        let edge = x == 1 || x == 14 || y == 6 || y == 14;
        shade(c, if edge { 0.72 } else { 0.95 + r * 0.1 })
    } else {
        CLEAR
    }
}

/// Shelf sides, tops and boards: stripped wood with horizontal grain.
fn shelf_body(layer: u16, kind: usize, x: usize, y: usize) -> Rgba {
    let c = STRIPPED[kind];
    let grain = (y + (rnd(layer, x / 5, 0, 3) * 2.0) as usize).is_multiple_of(4);
    noisy(layer, x, y, if grain { [c[0] / 8 * 7, c[1] / 8 * 7, c[2] / 8 * 7] } else { c }, 0.06)
}

/// A shelf's front: the boards' edges top and bottom, and between them the
/// back panel. Powered shelves light a strip along the panel that runs on
/// into the shelves they are chained to (`variant`: 0 unpowered, 1
/// unconnected, 2 left, 3 centre, 4 right).
fn shelf_front(layer: u16, kind: usize, variant: u16, x: usize, y: usize) -> Rgba {
    let c = STRIPPED[kind];
    if y <= 3 || y >= 12 {
        let edge = y == 0 || y == 3 || y == 12 || y == 15;
        return noisy(layer, x, y, c, 0.05).map(|v| if edge { (v as f32 * 0.82) as u8 } else { v });
    }
    let panel = shade(c, 0.66 + rnd(layer, x, y, 1) * 0.06);
    if variant == 0 {
        return panel;
    }
    let (from, to) = match variant {
        1 => (2, 13),
        2 => (2, 15),
        3 => (0, 15),
        _ => (0, 13),
    };
    let strip = (6..=9).contains(&y) && (from..=to).contains(&x);
    if strip {
        let core = (7..=8).contains(&y);
        shade(if core { [255, 214, 128] } else { [214, 150, 72] }, 0.92 + rnd(layer, x, y, 2) * 0.08)
    } else {
        panel
    }
}

fn chiseled_top(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let c = PLANKS[0];
    let rim = x == 0 || y == 0 || x == 15 || y == 15;
    let inset = (x == 3 || x == 12) && (3..=12).contains(&y) || (y == 3 || y == 12) && (3..=12).contains(&x);
    let _ = layer;
    shade(
        c,
        if rim {
            0.72
        } else if inset {
            0.8
        } else {
            0.94 + r * 0.1
        },
    )
}

fn chiseled_side(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let c = PLANKS[0];
    let board = y / 4;
    let seam = y % 4 == 3 || x == (board * 7 + 3) % 16;
    let rim = x == 0 || x == 15;
    let _ = layer;
    shade(
        c,
        if rim {
            0.72
        } else if seam {
            0.7
        } else {
            0.93 + r * 0.1
        },
    )
}

/// The front of a chiseled bookshelf holding books in the slots of `mask`:
/// two rows of three slots, slot 0 top left as seen from the front.
fn chiseled_front(layer: u16, mask: u8, x: usize, y: usize, r: f32) -> Rgba {
    let c = PLANKS[0];
    let row = match y {
        1..=6 => Some(0),
        9..=14 => Some(1),
        _ => None,
    };
    let column = match x {
        1..=4 => Some(0),
        6..=9 => Some(1),
        11..=14 => Some(2),
        _ => None,
    };
    let (Some(row), Some(column)) = (row, column) else {
        return shade(c, 0.78 + r * 0.08);
    };
    let slot = row * 3 + column;
    if mask & (1 << slot) == 0 {
        return shade([46, 32, 18], 0.9 + r * 0.1);
    }
    // Book spines: each pixel column a book, coloured and sized by slot.
    const COVERS: [[u8; 3]; 6] =
        [[150, 40, 34], [42, 92, 150], [56, 120, 50], [126, 84, 44], [120, 50, 120], [196, 160, 64]];
    let first = if row == 0 { 1 } else { 9 };
    let book = (x + slot * 2) % 6;
    let height = 4 + (rnd(tex::CHISELED_FRONTS, x, slot, 4) * 2.0) as usize;
    if y - first >= 6 - height {
        let band = y - first == 6 - height + 1;
        shade(COVERS[book], if band { 1.15 } else { 0.85 + rnd(layer, x, 0, 6) * 0.15 })
    } else {
        shade([46, 32, 18], 0.9)
    }
}
