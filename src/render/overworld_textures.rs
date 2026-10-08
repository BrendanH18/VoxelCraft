//! Procedural textures for the v0.6 Overworld blocks (layers from
//! `overworld_blocks::TEX`): original artwork in the palette of Java's
//! moss, dripstone, amethyst, ocean, coral, new woods and flowers.

use super::{Rgba, SIZE, noisy, pixel as base_pixel, rnd, shade};
use crate::world::block::tex;
use crate::world::overworld_blocks::t;

const CLEAR: Rgba = [0, 0, 0, 0];
const DIRT: [u8; 3] = [134, 96, 67];
const MOSS: [u8; 3] = [89, 109, 45];
const PALE_MOSS: [u8; 3] = [108, 118, 104];
const DRIPSTONE: [u8; 3] = [134, 107, 92];
const AMETHYST: [u8; 3] = [133, 97, 191];
const PRISMARINE: [u8; 3] = [99, 156, 151];
const STEM: [u8; 3] = [64, 120, 40];
const CORAL: [[u8; 3]; 5] = [[50, 90, 210], [206, 80, 160], [160, 30, 190], [190, 36, 46], [216, 200, 66]];
const DEAD_CORAL: [u8; 3] = [130, 122, 118];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    match layer {
        t::PODZOL_TOP => {
            let c = if rnd(layer, x / 2, y / 2, 3) < 0.35 { [92, 62, 28] } else { [122, 87, 40] };
            noisy(layer, x, y, c, 0.16)
        }
        t::PODZOL_SIDE => fringe(layer, x, y, [106, 74, 34]),
        t::COARSE_DIRT => {
            let pebble = rnd(layer, x / 2, y / 2, 4) < 0.22;
            if pebble { shade([110, 104, 96], 0.85 + r * 0.25) } else { noisy(layer, x, y, [119, 85, 59], 0.16) }
        }
        t::ROOTED_DIRT => {
            let root = (x + (y / 3) * 5).is_multiple_of(7) && rnd(layer, x, y / 2, 5) < 0.7;
            if root { shade([170, 130, 92], 0.9 + r * 0.15) } else { noisy(layer, x, y, [144, 104, 76], 0.14) }
        }
        t::MYCELIUM_TOP => {
            let speck = rnd(layer, x, y, 6);
            let c = if speck < 0.15 {
                [150, 128, 150]
            } else if speck > 0.85 {
                [96, 82, 96]
            } else {
                [114, 98, 112]
            };
            shade(c, 0.9 + r * 0.15)
        }
        t::MYCELIUM_SIDE => fringe(layer, x, y, [114, 98, 112]),
        t::MUD => noisy(layer, x, y, [60, 57, 60], 0.12),
        t::PACKED_MUD => {
            let straw = rnd(layer, x, y / 2, 7) < 0.12;
            if straw { shade([180, 150, 92], 0.85 + r * 0.2) } else { noisy(layer, x, y, [142, 107, 80], 0.1) }
        }
        t::MUD_BRICKS => bricks(layer, x, y, [137, 103, 79], [116, 88, 66]),
        t::MOSS => mossy(layer, x, y, MOSS),
        t::PALE_MOSS => mossy(layer, x, y, PALE_MOSS),
        t::BLUE_ICE => {
            let crack = rnd(layer, x / 3, y / 3, 8) < 0.2 && (x + y).is_multiple_of(3);
            let c = if crack { [176, 214, 255] } else { [116, 168, 253] };
            shade(c, 0.92 + r * 0.1)
        }
        t::SNOW_LAYER => noisy(layer, x, y, [240, 246, 250], 0.03),
        t::DRIPSTONE => {
            let band = (y + (rnd(layer, x / 4, 0, 2) * 3.0) as usize).is_multiple_of(5);
            shade(DRIPSTONE, if band { 0.8 } else { 0.92 + r * 0.14 })
        }
        l if (t::POINTED..t::POINTED + 10).contains(&l) => pointed(l - t::POINTED, x, y, r),
        t::AMETHYST => {
            let facet = super::voronoi(x, y, &super::points(layer, 6)).2;
            shade(AMETHYST, 0.82 + (facet as f32 * 0.07) + r * 0.08)
        }
        t::BUDDING_AMETHYST => {
            let (d1, _, _) = super::voronoi(x, y, &super::points(layer, 5));
            if d1 < 1.6 { shade([200, 160, 240], 0.95 + r * 0.05) } else { shade(AMETHYST, 0.8 + r * 0.12) }
        }
        l if (t::BUDS..t::BUDS + 8).contains(&l) => {
            let i = l - t::BUDS;
            bud(i % 4, i >= 4, x, y, r)
        }
        t::SMOOTH_BASALT => noisy(layer, x, y, [72, 72, 78], 0.08),
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
        t::AZALEA_LEAVES | t::FLOWERING_AZALEA_LEAVES => {
            if rnd(layer, x, y, 11) < 0.18 {
                CLEAR
            } else if layer == t::FLOWERING_AZALEA_LEAVES && rnd(layer, x / 2, y / 2, 12) < 0.22 {
                shade([214, 120, 200], 0.9 + r * 0.2)
            } else {
                shade([90, 122, 44], 0.75 + r * 0.4)
            }
        }
        l if (t::CAVE_VINES..t::CAVE_VINES + 4).contains(&l) => {
            let i = l - t::CAVE_VINES;
            let lit = i % 2 == 1;
            let wave = ((fy * 0.6).sin() * 1.5) as i32;
            let on = (x as i32 - 7 - wave).abs() <= 1;
            let leaf = (x as i32 - 7 - wave).abs() <= 3 && (y % 5 == 1 || y % 5 == 2);
            let berry = lit
                && ((x as i32 - 10).pow(2) + (y as i32 - 9).pow(2) <= 3
                    || (x as i32 - 4).pow(2) + (y as i32 - 4).pow(2) <= 2);
            if berry {
                shade([255, 186, 70], 0.9 + r * 0.15)
            } else if on || leaf {
                shade([84, 116, 40], 0.8 + r * 0.3)
            } else {
                CLEAR
            }
        }
        t::SPORE_BLOSSOM => {
            let (dx, dy) = (fx - 8.0, fy - 3.0);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 2.0 {
                shade([90, 140, 50], 0.9 + r * 0.1)
            } else if y > 3 && y < 12 && (dx.abs() * 2.0 < (12 - y) as f32) && !(x + y).is_multiple_of(3) {
                shade([226, 120, 200], 0.85 + r * 0.25)
            } else {
                CLEAR
            }
        }
        t::HANGING_ROOTS => {
            let strand = [3usize, 6, 9, 12].iter().any(|&c| x == c + (y / 6) % 2) && y < 12 + (x % 4);
            if strand { shade([160, 118, 84], 0.85 + r * 0.2) } else { CLEAR }
        }
        t::BIG_DRIPLEAF_TOP => {
            let vein = x == 8 || (x as i32 - 8).abs() == (y as i32 - 8).abs();
            shade([112, 160, 40], if vein { 0.8 } else { 0.95 + r * 0.12 })
        }
        t::BIG_DRIPLEAF_SIDE => shade([96, 140, 34], 0.9 + r * 0.1),
        t::BIG_DRIPLEAF_STEM => {
            if (7..=8).contains(&x) {
                shade([96, 140, 34], 0.85 + r * 0.2)
            } else {
                CLEAR
            }
        }
        t::SMALL_DRIPLEAF | t::SMALL_DRIPLEAF_TOP => {
            let stalk = (7..=8).contains(&x) && (layer == t::SMALL_DRIPLEAF || y > 6);
            let leaf = layer == t::SMALL_DRIPLEAF_TOP && y < 7 && (x as i32 - 8).abs() <= 6 - y as i32 / 2;
            if stalk || leaf { shade([96, 150, 40], 0.85 + r * 0.25) } else { CLEAR }
        }
        t::KELP | t::KELP_PLANT => {
            let wave = ((fy * 0.5 + if layer == t::KELP { 0.0 } else { 1.0 }).sin() * 2.0) as i32;
            let c = x as i32 - 8 - wave;
            let blade = c.abs() <= 1 || (y % 4 < 2 && c.abs() <= 4 && (c > 0) == (y % 8 < 4));
            let top = layer == t::KELP && y < 3;
            if blade && !top { shade([66, 120, 40], 0.75 + r * 0.35) } else { CLEAR }
        }
        t::SEAGRASS | t::TALL_SEAGRASS_BOTTOM | t::TALL_SEAGRASS_TOP => {
            let start = if layer == t::SEAGRASS {
                3
            } else if layer == t::TALL_SEAGRASS_TOP {
                1
            } else {
                0
            };
            let blade = [2usize, 5, 8, 11, 14].iter().any(|&c| {
                let lean = (SIZE - y) / 5;
                x == (c + lean) % SIZE && y >= start + c % 3
            });
            if blade { shade([50, 132, 40], 0.75 + r * 0.35) } else { CLEAR }
        }
        l if (t::SEA_PICKLES..t::SEA_PICKLES + 4).contains(&l) => {
            let n = (l - t::SEA_PICKLES) as usize + 1;
            const SPOTS: [(usize, usize); 4] = [(7, 10), (3, 11), (11, 9), (6, 5)];
            let hit = SPOTS[..n].iter().any(|&(cx, cy)| x.abs_diff(cx) <= 1 && y >= cy && y <= cy + 5);
            let glow = SPOTS[..n].iter().any(|&(cx, cy)| x.abs_diff(cx) <= 1 && y + 1 == cy);
            if glow {
                shade([230, 250, 140], 1.0)
            } else if hit {
                shade([100, 120, 40], 0.85 + r * 0.25)
            } else {
                CLEAR
            }
        }
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
        t::PRISMARINE_BRICKS => bricks(layer, x, y, [99, 171, 158], [76, 140, 128]),
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
        l if (t::CORAL..t::CORAL + 30).contains(&l) => coral_pixel(l - t::CORAL, x, y, r),
        t::DARK_OAK_LEAVES => leaves(layer, x, y, r, [44, 92, 24], 0.16),
        t::MANGROVE_LEAVES => leaves(layer, x, y, r, [70, 128, 36], 0.2),
        t::CHERRY_LEAVES => {
            if rnd(layer, x, y, 11) < 0.15 {
                CLEAR
            } else {
                let deep = rnd(layer, x / 2, y / 2, 12) < 0.3;
                shade(if deep { [220, 132, 180] } else { [240, 180, 210] }, 0.88 + r * 0.16)
            }
        }
        t::PALE_OAK_LEAVES => leaves(layer, x, y, r, [140, 150, 134], 0.2),
        t::DARK_OAK_SAPLING => sapling(x, y, r, [44, 92, 24], [70, 50, 30]),
        t::CHERRY_SAPLING => sapling(x, y, r, [238, 170, 206], [70, 40, 46]),
        t::PALE_OAK_SAPLING => sapling(x, y, r, [140, 150, 134], [200, 196, 188]),
        t::MANGROVE_PROPAGULE => {
            let pod = (7..=8).contains(&x) && (6..15).contains(&y);
            let leaf = y < 6 && (x as i32 - 8).abs() <= (y as i32 + 1) / 2 + 1;
            if leaf {
                shade([90, 140, 50], 0.85 + r * 0.2)
            } else if pod {
                shade([100, 120, 40], 0.85 + r * 0.2)
            } else {
                CLEAR
            }
        }
        t::PALE_OAK_LOG_SIDE => {
            let stripe = (x + (rnd(layer, 0, y / 4, 3) * 2.0) as usize).is_multiple_of(4);
            shade([84, 76, 72], if stripe { 0.75 } else { 0.9 + r * 0.15 })
        }
        t::PALE_OAK_LOG_TOP => {
            let d = ((fx - 8.0).powi(2) + (fy - 8.0).powi(2)).sqrt();
            if d > 6.6 {
                shade([84, 76, 72], 0.9 + r * 0.1)
            } else {
                shade(if (d * 1.1) as i32 % 2 == 0 { [232, 226, 220] } else { [210, 200, 196] }, 0.95 + r * 0.06)
            }
        }
        t::PALE_OAK_PLANKS => planks(layer, x, y, [228, 218, 214]),
        t::BAMBOO_PLANKS => planks(layer, x, y, [196, 176, 82]),
        t::MANGROVE_ROOTS_SIDE | t::MANGROVE_ROOTS_TOP => {
            let root = (x + y).is_multiple_of(5) || (x + 16 - y).is_multiple_of(6);
            if root { shade([90, 70, 48], 0.85 + r * 0.25) } else { CLEAR }
        }
        t::MUDDY_ROOTS_SIDE | t::MUDDY_ROOTS_TOP => {
            let root = (x + y).is_multiple_of(5) || (x + 16 - y).is_multiple_of(6);
            if root { shade([96, 74, 50], 0.85 + r * 0.25) } else { noisy(layer, x, y, [60, 57, 60], 0.12) }
        }
        t::BAMBOO | t::BAMBOO_SMALL_LEAVES | t::BAMBOO_LARGE_LEAVES => {
            // The post samples columns 6..10.
            let node = y.is_multiple_of(8);
            let leaf = match layer {
                t::BAMBOO_SMALL_LEAVES => y < 4,
                t::BAMBOO_LARGE_LEAVES => y < 8,
                _ => false,
            };
            if leaf && !(x + y).is_multiple_of(3) {
                shade([80, 150, 40], 0.85 + r * 0.25)
            } else {
                shade([110, 160, 40], if node { 0.72 } else { 0.9 + r * 0.12 })
            }
        }
        t::BAMBOO_SAPLING => {
            let shoot = (7..=8).contains(&x) && y > 8;
            let leaf = (5..11).contains(&y) && (x as i32 - 8).abs() <= (11 - y as i32) / 2 && x.is_multiple_of(2);
            if shoot || leaf { shade([96, 150, 40], 0.85 + r * 0.25) } else { CLEAR }
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
        t::PINK_PETALS => {
            let petal = rnd(layer, x / 2, y / 2, 4) < 0.45;
            if petal { shade([246, 168, 210], 0.88 + r * 0.16) } else { CLEAR }
        }
        t::PALE_HANGING_MOSS | t::PALE_HANGING_MOSS_TIP => {
            let len = if layer == t::PALE_HANGING_MOSS_TIP { 10 + x % 4 } else { SIZE };
            let strand = !x.is_multiple_of(3) && y < len;
            if strand { shade(PALE_MOSS, 0.8 + r * 0.35) } else { CLEAR }
        }
        l if (t::FLOWERS..t::FLOWERS + 9).contains(&l) => flower(l - t::FLOWERS, x, y, r),
        l if (t::DOUBLE..t::DOUBLE + 12).contains(&l) => {
            let i = l - t::DOUBLE;
            double_plant(i / 2, i % 2 == 1, x, y, r)
        }
        t::LILY_PAD => {
            let d = ((fx - 8.0).powi(2) + (fy - 8.0).powi(2)).sqrt();
            let notch = fx > 8.0 && (fy - 8.0).abs() < (fx - 8.0) * 0.3;
            if d < 7.5 && !notch { shade([32, 128, 48], 0.85 + r * 0.2) } else { CLEAR }
        }
        t::VINE => {
            let stem = (x + y / 2).is_multiple_of(5) || (x + 16 - y / 3).is_multiple_of(7);
            let leaf = rnd(layer, x / 2, y / 2, 3) < 0.35;
            if stem || leaf { shade([62, 110, 30], 0.75 + r * 0.35) } else { CLEAR }
        }
        l if (t::BERRY_BUSH..t::BERRY_BUSH + 4).contains(&l) => {
            let age = (l - t::BERRY_BUSH) as usize;
            let height = 6 + age * 3;
            let bush = y + height >= SIZE && rnd(l, x, y, 3) < 0.7;
            let berry = age >= 2 && rnd(l, x / 2, y / 2, 4) < if age == 3 { 0.25 } else { 0.12 };
            if bush && berry {
                shade([186, 20, 40], 0.9 + r * 0.15)
            } else if bush {
                shade([40, 90, 50], 0.8 + r * 0.3)
            } else {
                CLEAR
            }
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

/// Dirt with a coloured layer hanging over its top edge.
fn fringe(layer: u16, x: usize, y: usize, top: [u8; 3]) -> Rgba {
    let edge = 3 + (rnd(layer, x, 0, 7) * 2.2) as usize;
    if y < edge { noisy(layer, x, y, top, 0.14) } else { noisy(tex::DIRT, x, y, DIRT, 0.14) }
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

fn bricks(layer: u16, x: usize, y: usize, c: [u8; 3], mortar: [u8; 3]) -> Rgba {
    let row = y / 4;
    let offset = if row.is_multiple_of(2) { 0 } else { 4 };
    if y % 4 == 3 || (x + offset) % 8 == 7 {
        shade(mortar, 0.9 + rnd(layer, x, y, 1) * 0.1)
    } else {
        noisy(layer, x, y, c, 0.08)
    }
}

fn planks(layer: u16, x: usize, y: usize, colour: [u8; 3]) -> Rgba {
    let board = y / 4;
    let seam = y % 4 == 3 || x == (board * 7 + 3) % SIZE;
    shade(colour, if seam { 0.68 } else { 0.92 + rnd(layer, x, board, 2) * 0.12 })
}

fn leaves(layer: u16, x: usize, y: usize, r: f32, c: [u8; 3], holes: f32) -> Rgba {
    if rnd(layer, x, y, 11) < holes { CLEAR } else { shade(c, 0.7 + r * 0.45) }
}

fn sapling(x: usize, y: usize, r: f32, leaf: [u8; 3], trunk: [u8; 3]) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 6.0);
    if dx * dx + dy * dy * 1.4 < 20.0 && !(x + y).is_multiple_of(4) {
        shade(leaf, 0.8 + r * 0.3)
    } else if (7..=8).contains(&x) && y >= 8 {
        shade(trunk, 0.85 + r * 0.2)
    } else {
        CLEAR
    }
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

fn coral_pixel(i: u16, x: usize, y: usize, r: f32) -> Rgba {
    let (kind, dead, colour) = (i / 10, i % 10 >= 5, (i % 5) as usize);
    let c = if dead { DEAD_CORAL } else { CORAL[colour] };
    match kind {
        0 => {
            let pattern = match colour {
                0 => (x + y / 2).is_multiple_of(4),
                1 => ((x as i32 - 8).pow(2) + (y as i32 - 8).pow(2)) % 9 < 3,
                2 => rnd(200 + i, x / 2, y / 2, 3) < 0.3,
                3 => (x * 3 + y) % 7 < 2,
                _ => (x + y).is_multiple_of(5) || (x + 16 - y).is_multiple_of(5),
            };
            shade(c, if pattern { 0.75 } else { 0.92 + r * 0.14 })
        }
        1 => {
            // Branching coral: a few stalks from the bottom.
            let stalk = [4usize, 8, 12].iter().any(|&cx| {
                let lean = (SIZE - y) / 4;
                (x as i32
                    - cx as i32
                    - if cx == 4 {
                        -(lean as i32)
                    } else if cx == 12 {
                        lean as i32
                    } else {
                        0
                    })
                .abs()
                    <= 1
                    && y >= 3 + cx % 5
            });
            if stalk { shade(c, 0.85 + r * 0.25) } else { CLEAR }
        }
        _ => {
            // A fan spreading from the bottom middle.
            let (dx, dy) = (x as f32 - 7.5, SIZE as f32 - y as f32);
            let inside = dy > 1.0 && dy < 12.0 && dx.abs() < dy * 0.75;
            let rib = ((dx.atan2(dy) * 6.0).round() as i32 - (dx.atan2(dy) * 6.0) as i32) == 0 && x.is_multiple_of(2);
            if inside { shade(c, if rib { 0.8 } else { 0.92 + r * 0.14 }) } else { CLEAR }
        }
    }
}

fn flower(i: u16, x: usize, y: usize, r: f32) -> Rgba {
    const PETALS: [[u8; 3]; 9] = [
        [70, 110, 220],
        [240, 240, 240],
        [236, 236, 230],
        [220, 228, 240],
        [190, 120, 230],
        [210, 40, 30],
        [236, 120, 30],
        [236, 236, 236],
        [240, 170, 200],
    ];
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 5.0);
    let d = (dx * dx + dy * dy * 1.3).sqrt();
    let stem = (7..=8).contains(&x) && y >= 7 && x == 7 + (y / 5) % 2;
    let leaf = (y == 10 && (4..7).contains(&x)) || (y == 12 && (9..12).contains(&x));
    let head = match i {
        // Lily of the valley: little bells along an arched stem.
        1 => [(5usize, 5usize), (8, 3), (11, 5)].iter().any(|&(cx, cy)| x.abs_diff(cx) <= 1 && y.abs_diff(cy) <= 1),
        // Tulips: a cup.
        5..=8 => d < 3.0 && y >= 3,
        // Allium: a round purple ball.
        4 => d < 3.6,
        _ => d < 3.2,
    };
    let centre = matches!(i, 2 | 3) && d < 1.2;
    if centre {
        shade([236, 200, 50], 1.0)
    } else if head {
        shade(PETALS[i as usize], 0.85 + r * 0.2)
    } else if stem || leaf {
        shade(STEM, 0.85 + r * 0.2)
    } else {
        CLEAR
    }
}

fn double_plant(kind: u16, top: bool, x: usize, y: usize, r: f32) -> Rgba {
    let stem = (7..=8).contains(&x);
    match (kind, top) {
        // Sunflower: a big yellow face up top.
        (0, true) => {
            let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 8.0).powi(2)).sqrt();
            if d < 2.5 {
                shade([90, 60, 20], 0.9 + r * 0.2)
            } else if d < 5.5 {
                shade([250, 210, 40], 0.9 + r * 0.15)
            } else if stem && y > 12 {
                shade(STEM, 0.9)
            } else {
                CLEAR
            }
        }
        // Lilac, rose bush, peony: leafy below, blooms above.
        (1..=3, true) => {
            let bloom = [[200, 160, 220], [196, 30, 40], [236, 180, 220]][kind as usize - 1];
            let d = ((x as f32 - 7.5).powi(2) * 0.8 + (y as f32 - 7.0).powi(2)).sqrt();
            if d < 6.0 && rnd(300 + kind, x, y, 3) < 0.8 {
                shade(if rnd(301 + kind, x / 2, y / 2, 3) < 0.6 { bloom } else { [60, 120, 40] }, 0.85 + r * 0.2)
            } else {
                CLEAR
            }
        }
        // Large fern and tall grass: blades in both halves.
        (4 | 5, _) => {
            let blades = if kind == 4 { [1usize, 4, 8, 11, 14] } else { [2, 5, 7, 10, 13] };
            let start = if top { 3 } else { 0 };
            let blade = blades.iter().any(|&c| x == (c + (SIZE - y) / 6) % SIZE && y >= start + c % 3);
            if blade { shade([72, 140, 50], 0.75 + r * 0.35) } else { CLEAR }
        }
        // Lower halves of the flowering plants: stems and leaves.
        (_, false) => {
            let leaf = (y % 5 < 2) && (x as i32 - 8).abs() <= 4 && rnd(310 + kind, x, y, 3) < 0.7;
            if stem || leaf { shade([62, 124, 40], 0.8 + r * 0.25) } else { CLEAR }
        }
        _ => {
            let _ = base_pixel;
            CLEAR
        }
    }
}
