//! Procedurally generated 16x16 block textures, so the game ships no assets.
//!
//! Layer order must match `world::block::tex`.

use crate::world::block::tex;
use crate::world::noise::hash_f;

pub const SIZE: usize = 16;
pub const MIP_LEVELS: u32 = 5; // 16, 8, 4, 2, 1

type Rgba = [u8; 4];

/// Deterministic texture noise; `variation` selects a pattern within the layer.
/// This is a procedural graphics helper with no cryptographic purpose.
fn rnd(layer: u8, x: usize, y: usize, variation: i32) -> f32 {
    hash_f(x as i32, y as i32, variation, 0xB10C ^ layer as u64)
}

fn shade(c: [u8; 3], f: f32) -> Rgba {
    let s = |v: u8| (v as f32 * f).clamp(0.0, 255.0) as u8;
    [s(c[0]), s(c[1]), s(c[2]), 255]
}

fn noisy(layer: u8, x: usize, y: usize, c: [u8; 3], amount: f32) -> Rgba {
    shade(c, 1.0 - amount + rnd(layer, x, y, 0) * amount * 2.0)
}

/// Wrapping distance to the nearest two of a set of points (tileable Voronoi).
fn voronoi(x: usize, y: usize, pts: &[(f32, f32)]) -> (f32, f32, usize) {
    let (mut d1, mut d2, mut idx) = (f32::MAX, f32::MAX, 0);
    for (i, &(px, py)) in pts.iter().enumerate() {
        let dx = ((x as f32 + 0.5 - px).abs()).min(SIZE as f32 - (x as f32 + 0.5 - px).abs());
        let dy = ((y as f32 + 0.5 - py).abs()).min(SIZE as f32 - (y as f32 + 0.5 - py).abs());
        let d = (dx * dx + dy * dy).sqrt();
        if d < d1 {
            d2 = d1;
            d1 = d;
            idx = i;
        } else if d < d2 {
            d2 = d;
        }
    }
    (d1, d2, idx)
}

fn points(layer: u8, n: usize) -> Vec<(f32, f32)> {
    (0..n).map(|i| (rnd(layer, i, 0, 99) * SIZE as f32, rnd(layer, i, 1, 99) * SIZE as f32)).collect()
}

const STONE: [u8; 3] = [125, 125, 125];
const DIRT: [u8; 3] = [134, 96, 67];
const GRASS: [u8; 3] = [95, 159, 53];

pub(super) fn pixel(layer: u8, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        tex::STONE => {
            let streak = rnd(layer, x / 3, y, 5) < 0.12;
            shade(STONE, if streak { 0.82 } else { 0.9 + r * 0.18 })
        }
        tex::DIRT => noisy(layer, x, y, DIRT, 0.14),
        tex::GRASS_TOP => noisy(layer, x, y, GRASS, 0.13),
        tex::GRASS_SIDE | tex::SNOWY_GRASS_SIDE => {
            let edge = 3 + (rnd(layer, x, 0, 7) * 2.2) as usize;
            if y < edge {
                if layer == tex::GRASS_SIDE {
                    noisy(layer, x, y, GRASS, 0.13)
                } else {
                    noisy(layer, x, y, [240, 245, 250], 0.03)
                }
            } else {
                noisy(tex::DIRT, x, y, DIRT, 0.14)
            }
        }
        tex::SAND => noisy(layer, x, y, [219, 207, 163], 0.06),
        tex::WATER => {
            let c = noisy(layer, x, y, [44, 90, 200], 0.08);
            [c[0], c[1], c[2], 170]
        }
        tex::LOG_SIDE | tex::SPRUCE_LOG_SIDE | tex::JUNGLE_LOG_SIDE | tex::ACACIA_LOG_SIDE => {
            let bark = match layer {
                tex::LOG_SIDE => [104, 82, 51],
                tex::SPRUCE_LOG_SIDE => [70, 50, 30],
                tex::JUNGLE_LOG_SIDE => [88, 70, 32],
                _ => [104, 96, 88],
            };
            let stripe = (x + (rnd(layer, 0, y / 4, 3) * 2.0) as usize).is_multiple_of(4);
            // Jungle bark is mottled with moss.
            if layer == tex::JUNGLE_LOG_SIDE && rnd(layer, x / 2, y / 3, 6) < 0.18 {
                return shade([84, 100, 40], 0.85 + r * 0.2);
            }
            shade(bark, if stripe { 0.72 } else { 0.9 + r * 0.15 })
        }
        tex::BIRCH_LOG_SIDE => {
            // White bark with short dark horizontal marks.
            let mark = rnd(layer, x / 3, y, 4) < 0.16 && rnd(layer, x, y, 5) < 0.85;
            if mark { shade([50, 46, 40], 0.8 + r * 0.3) } else { shade([216, 214, 204], 0.9 + r * 0.12) }
        }
        tex::LOG_TOP | tex::SPRUCE_LOG_TOP | tex::BIRCH_LOG_TOP | tex::JUNGLE_LOG_TOP | tex::ACACIA_LOG_TOP => {
            let (bark, light, dark) = match layer {
                tex::LOG_TOP => ([104, 82, 51], [176, 142, 88], [150, 118, 70]),
                tex::SPRUCE_LOG_TOP => ([70, 50, 30], [128, 96, 58], [106, 78, 46]),
                tex::BIRCH_LOG_TOP => ([216, 214, 204], [200, 182, 128], [178, 160, 108]),
                tex::JUNGLE_LOG_TOP => ([88, 70, 32], [170, 124, 86], [146, 104, 70]),
                _ => ([104, 96, 88], [176, 96, 54], [150, 80, 44]),
            };
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let d = (dx * dx + dy * dy).sqrt();
            if d > 6.6 {
                shade(bark, 0.9 + r * 0.1)
            } else {
                let ring = (d * 1.1) as i32 % 2 == 0;
                shade(if ring { light } else { dark }, 0.95 + r * 0.08)
            }
        }
        tex::LEAVES | tex::SPRUCE_LEAVES | tex::BIRCH_LEAVES | tex::JUNGLE_LEAVES | tex::ACACIA_LEAVES => {
            let (base, holes) = match layer {
                tex::LEAVES => ([58, 128, 38], 0.2),
                tex::SPRUCE_LEAVES => ([44, 92, 56], 0.2),
                tex::BIRCH_LEAVES => ([100, 148, 62], 0.22),
                tex::JUNGLE_LEAVES => ([44, 136, 28], 0.12),
                _ => ([88, 124, 40], 0.24),
            };
            if rnd(layer, x, y, 11) < holes { [0, 0, 0, 0] } else { shade(base, 0.7 + r * 0.45) }
        }
        tex::PLANKS | tex::SPRUCE_PLANKS | tex::BIRCH_PLANKS | tex::JUNGLE_PLANKS | tex::ACACIA_PLANKS => {
            let colour = match layer {
                tex::PLANKS => [162, 130, 78],
                tex::SPRUCE_PLANKS => [114, 84, 50],
                tex::BIRCH_PLANKS => [196, 180, 124],
                tex::JUNGLE_PLANKS => [160, 114, 80],
                _ => [170, 92, 50],
            };
            let board = y / 4;
            let seam_x = (board * 7 + 3) % SIZE;
            let seam = y % 4 == 3 || x == seam_x;
            shade(colour, if seam { 0.68 } else { 0.92 + rnd(layer, x, board, 2) * 0.12 })
        }
        tex::SKIN => noisy(layer, x, y, [196, 141, 110], 0.05),
        l if (tex::FIRE_0..tex::FIRE_0 + tex::FIRE_FRAMES).contains(&l) => {
            // Pixel flames rise from a solid base into separate tongues.
            // Each frame changes the tips and hot inner cores.
            let frame = (l - tex::FIRE_0) as usize;
            let sway = (rnd(tex::FIRE_0, y / 3, frame, 2) * 3.0) as usize;
            let column = (x + sway) % SIZE;
            let tip = (rnd(tex::FIRE_0, column / 2, frame, 3) * 9.0) as usize;
            if y < tip || (y < 10 && rnd(l, x, y, 4) < 0.16) {
                [0, 0, 0, 0]
            } else {
                let heat = ((y - tip) as f32 / (SIZE - tip) as f32 + r * 0.2).min(1.0);
                [255, (75.0 + 180.0 * heat) as u8, (15.0 + 105.0 * heat * heat) as u8, 255]
            }
        }
        tex::LADDER => {
            // Two rails with a rung every four pixels; see-through between.
            let rail = matches!(x, 1 | 2 | 13 | 14);
            let rung = y % 4 == 1 && (3..=12).contains(&x);
            if rail || rung {
                let edge = x == 2 || x == 14 || (rung && !rail);
                shade([124, 94, 56], if edge { 0.8 } else { 0.95 + r * 0.1 })
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::DOOR_TOP | tex::DOOR_BOTTOM => {
            // A frame of vertical boards around raised panels; the upper
            // half has two windows.
            let frame =
                x <= 1 || x >= 14 || (layer == tex::DOOR_TOP && y <= 1) || (layer == tex::DOOR_BOTTOM && y >= 14);
            let window = layer == tex::DOOR_TOP && (3..=6).contains(&y) && matches!(x, 3..=6 | 9..=12);
            let rail = y == 8 || y == 9;
            let inset = matches!(x, 3 | 12) || (layer == tex::DOOR_BOTTOM && matches!(y, 2 | 12));
            if window {
                [0, 0, 0, 0]
            } else {
                let board = 0.92 + rnd(layer, x / 3, 0, 5) * 0.12 + r * 0.05;
                let f = if frame || rail {
                    0.8
                } else if inset {
                    0.7
                } else {
                    board
                };
                shade([150, 116, 68], f)
            }
        }
        tex::RED_SAND => noisy(layer, x, y, [190, 102, 36], 0.07),
        l if (tex::TERRACOTTA..tex::TERRACOTTA + 7).contains(&l) => {
            const COLOURS: [[u8; 3]; 7] = [
                [152, 94, 67],
                [162, 84, 38],
                [186, 134, 36],
                [144, 62, 48],
                [78, 52, 36],
                [210, 178, 160],
                [136, 108, 98],
            ];
            noisy(layer, x, y, COLOURS[(l - tex::TERRACOTTA) as usize], 0.05)
        }
        tex::CLAY => {
            let spot = rnd(layer, x / 2, y / 2, 3) < 0.15;
            shade([160, 166, 180], if spot { 0.9 } else { 0.97 + r * 0.06 })
        }
        tex::ICE => {
            // Pale blue and see-through, with bright fracture lines.
            let crack =
                (x + 2 * y).is_multiple_of(11) && rnd(layer, x, y, 4) < 0.7 || (3 * x + y).is_multiple_of(13) && y < 9;
            let c = if crack { [230, 240, 255] } else { [150, 186, 246] };
            let s = shade(c, 0.95 + r * 0.08);
            [s[0], s[1], s[2], if crack { 220 } else { 160 }]
        }
        tex::PUMPKIN_SIDE | tex::MELON_SIDE => {
            // Vertical ribs (pumpkin) or stripes (melon).
            let pumpkin = layer == tex::PUMPKIN_SIDE;
            let rib = if pumpkin { x.is_multiple_of(4) } else { (x + (y / 5) % 2) % 5 < 2 };
            let c = match (pumpkin, rib) {
                (true, true) => [190, 110, 20],
                (true, false) => [226, 140, 28],
                (false, true) => [58, 98, 22],
                (false, false) => [110, 156, 36],
            };
            shade(c, 0.92 + r * 0.12)
        }
        tex::PUMPKIN_TOP | tex::MELON_TOP => {
            let pumpkin = layer == tex::PUMPKIN_TOP;
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let d = (dx * dx + dy * dy).sqrt();
            if pumpkin && d < 1.6 {
                shade([110, 86, 40], 0.9 + r * 0.1) // the stalk
            } else {
                let base = if pumpkin { [214, 130, 26] } else { [104, 146, 34] };
                shade(base, 0.9 - (d / 12.0).min(0.2) + r * 0.1)
            }
        }
        tex::SUGAR_CANE => {
            // Three stalks with pale joints, and a leaf or two.
            let stalk = [3, 8, 12].iter().any(|&sx| x == sx || x == sx + 1);
            let joint = (y + x / 4 * 3).is_multiple_of(6);
            let leaf = (y == 4 && (4..8).contains(&x)) || (y == 11 && (9..12).contains(&x));
            if stalk {
                shade(if joint { [196, 222, 140] } else { [140, 192, 84] }, 0.85 + r * 0.2)
            } else if leaf {
                shade([110, 170, 60], 0.85 + r * 0.2)
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::FERN => {
            // Arching fronds: a stem per frond with leaflets on both sides.
            let fronds = [(7.5f32, 0.0f32), (4.0, -0.35), (11.0, 0.35)];
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let on = fronds.iter().any(|&(base, lean)| {
                let top = 2.0 + (lean.abs() * 8.0);
                if fy < top {
                    return false;
                }
                let cx = base + lean * (16.0 - fy) * 0.6;
                let half = 0.6 + ((fy - top) / 3.0).min(2.6) * if y.is_multiple_of(2) { 1.0 } else { 0.5 };
                (fx - cx).abs() < half
            });
            if on { shade([82, 140, 52], 0.75 + r * 0.35) } else { [0, 0, 0, 0] }
        }
        tex::BLUE_ORCHID => {
            let heads = [(5.5f32, 4.0f32), (10.5, 6.0), (7.5, 8.5)];
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            if heads.iter().any(|&(hx, hy)| (fx - hx).powi(2) + (fy - hy).powi(2) < 3.2) {
                let centre = heads.iter().any(|&(hx, hy)| (fx - hx).powi(2) + (fy - hy).powi(2) < 0.6);
                if centre { [200, 220, 255, 255] } else { shade([50, 150, 230], 0.85 + r * 0.25) }
            } else if (x == 7 || x == 8) && y >= 9 || (y == 12 && (4..7).contains(&x)) {
                shade([60, 125, 40], 0.85 + r * 0.2)
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::COBBLESTONE => {
            let pts = points(layer, 9);
            let (d1, d2, i) = voronoi(x, y, &pts);
            if d2 - d1 < 1.1 { shade(STONE, 0.55) } else { shade(STONE, 0.8 + rnd(layer, i, 0, 4) * 0.35 - d1 * 0.03) }
        }
        tex::GLASS => {
            let border = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            let streak = (x + y == 9 || x + y == 10) && (3..8).contains(&x);
            if border {
                [205, 225, 235, 255]
            } else if streak {
                [240, 250, 255, 255]
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::BEDROCK => shade([85, 85, 85], 0.45 + r * 0.8),
        tex::GRAVEL => {
            let c = if rnd(layer, x, y, 6) < 0.3 { [140, 120, 105] } else { [130, 126, 124] };
            shade(c, 0.7 + r * 0.45)
        }
        tex::SNOW => noisy(layer, x, y, [240, 245, 252], 0.03),
        tex::COAL_ORE | tex::IRON_ORE | tex::GOLD_ORE | tex::DIAMOND_ORE => {
            let ore = match layer {
                tex::COAL_ORE => [40, 40, 40],
                tex::IRON_ORE => [216, 175, 147],
                tex::GOLD_ORE => [250, 220, 70],
                _ => [95, 230, 225],
            };
            let pts = points(layer, 5);
            let (d1, _, _) = voronoi(x, y, &pts);
            if d1 < 1.5 { shade(ore, 0.85 + r * 0.25) } else { pixel(tex::STONE, x, y) }
        }
        tex::CACTUS_SIDE => {
            let line = x % 4 == 1;
            let spike = rnd(layer, x, y, 8) < 0.05;
            if spike { [230, 230, 190, 255] } else { shade([76, 140, 48], if line { 0.75 } else { 0.95 + r * 0.1 }) }
        }
        tex::CACTUS_TOP => {
            let border = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            shade([88, 150, 58], if border { 0.75 } else { 0.95 + r * 0.1 })
        }
        tex::BRICKS => {
            let row = y / 4;
            let offset = if row.is_multiple_of(2) { 0 } else { 4 };
            if y % 4 == 3 || (x + offset) % 8 == 7 {
                shade([200, 195, 185], 0.9 + r * 0.1)
            } else {
                shade([150, 72, 56], 0.85 + rnd(layer, (x + offset) / 8, row, 1) * 0.2 + r * 0.06)
            }
        }
        tex::SANDSTONE_SIDE => {
            let band = matches!(y, 3 | 4 | 10);
            shade([216, 203, 155], if band { 0.88 } else { 0.96 + r * 0.06 })
        }
        tex::SANDSTONE_TOP => noisy(layer, x, y, [222, 210, 162], 0.04),
        tex::GLOWSTONE => {
            let pts = points(layer, 7);
            let (d1, d2, i) = voronoi(x, y, &pts);
            if d2 - d1 < 0.9 { [150, 110, 60, 255] } else { shade([250, 215, 120], 0.8 + rnd(layer, i, 0, 4) * 0.25) }
        }
        tex::TALL_GRASS => {
            // Blades rising from the bottom edge to varying heights, leaning a little.
            let blade = |bx: usize| {
                let top = 2 + (rnd(layer, bx, 0, 12) * 9.0) as usize;
                let lean = (rnd(layer, bx, 1, 12) - 0.5) * 0.5;
                let at = (bx as f32 + lean * (SIZE - y) as f32).round() as usize;
                y >= top && at == x
            };
            if (0..SIZE).any(|bx| rnd(layer, bx, 2, 12) < 0.7 && blade(bx)) {
                shade(GRASS, 0.75 + r * 0.25 + (y as f32 / SIZE as f32) * -0.15)
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::DANDELION | tex::POPPY => flower(layer, x, y, r),
        tex::DEAD_BUSH => {
            // A trunk forking into thin bare twigs.
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let twig = |x0: f32, y0: f32, x1: f32, y1: f32| {
                let (dx, dy) = (x1 - x0, y1 - y0);
                let t = (((fx - x0) * dx + (fy - y0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                let (px, py) = (x0 + dx * t - fx, y0 + dy * t - fy);
                px * px + py * py < 0.36
            };
            let branches = [
                (8.0, 16.0, 8.0, 10.0),
                (8.0, 10.0, 3.0, 4.0),
                (8.0, 10.0, 13.0, 3.0),
                (8.0, 12.0, 12.0, 8.0),
                (6.0, 8.0, 7.0, 2.0),
                (5.0, 6.0, 1.0, 5.0),
                (11.0, 6.0, 15.0, 7.0),
            ];
            if branches.iter().any(|&(a, b, c, d)| twig(a, b, c, d)) {
                shade([148, 102, 52], 0.75 + r * 0.3)
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::TORCH => match (x, y) {
            (7..=8, 5) => [255, 250, 210, 255],
            (7..=8, 4) => [255, 216, 90, 255],
            (7..=8, 6) => [255, 160, 40, 255],
            (7..=8, 7..=15) => shade([138, 106, 62], if x == 7 { 1.0 } else { 0.78 }),
            _ => [0, 0, 0, 0],
        },
        tex::LAVA => {
            // Molten cells: bright yellow-orange centres, darker red crust between.
            let pts = points(layer, 8);
            let (d1, d2, _) = voronoi(x, y, &pts);
            let edge = ((d2 - d1) / 3.0).min(1.0);
            let heat = edge * 0.7 + r * 0.3;
            let c = [255, (90.0 + heat * 140.0) as u8, (20.0 + heat * 40.0) as u8];
            shade(c, 0.75 + heat * 0.3)
        }
        tex::WOOL => {
            // Soft weave: alternating diagonal ridges.
            let ridge = (x + y) % 4 < 2;
            shade([234, 234, 228], if ridge { 0.96 + r * 0.06 } else { 0.86 + r * 0.06 })
        }
        tex::BED_TOP_FOOT => {
            // Red blanket with a lighter hem around the edge.
            let hem = x == 1 || y == 1 || x == SIZE - 2 || y == SIZE - 2;
            let weave = (x + y) % 4 < 2;
            shade(
                [178, 34, 34],
                if hem {
                    1.18
                } else if weave {
                    0.96 + r * 0.06
                } else {
                    0.88 + r * 0.06
                },
            )
        }
        tex::BED_TOP_HEAD => {
            // A white pillow framed by the blanket.
            if (3..SIZE - 3).contains(&x) && (3..SIZE - 3).contains(&y) {
                let edge = x == 3 || y == 3 || x == SIZE - 4 || y == SIZE - 4;
                shade([236, 236, 230], if edge { 0.86 } else { 0.97 + r * 0.05 })
            } else {
                pixel(tex::BED_TOP_FOOT, x, y)
            }
        }
        tex::BED_SIDE_FOOT | tex::BED_SIDE_HEAD => {
            // Only the bottom 9 rows show (the bed is 9/16 tall): blanket
            // (a pillow on the head half), a wooden frame, then corner legs.
            match y {
                0..=9 if layer == tex::BED_SIDE_HEAD => shade([236, 236, 230], 0.92 + r * 0.06),
                0..=9 => shade([178, 34, 34], if y == 7 { 1.15 } else { 0.9 + r * 0.06 }),
                10..=12 => pixel(tex::PLANKS, x, y),
                _ if !(3..SIZE - 3).contains(&x) => shade([104, 74, 44], 0.85 + r * 0.1),
                _ => [0, 0, 0, 0],
            }
        }
        tex::TABLE_TOP => {
            // Planks framed by a dark border, with a 2x2 grid scored in the middle.
            let border = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            let grid = (3..13).contains(&x)
                && (3..13).contains(&y)
                && (x == 3 || x == 8 || x == 12 || y == 3 || y == 8 || y == 12);
            if border || grid { shade([104, 74, 44], 0.85 + r * 0.1) } else { pixel(tex::PLANKS, x, y) }
        }
        tex::TABLE_SIDE => {
            // Planks with a saw blade (left) and hammer (right) hung on them.
            let top_band = y < 3;
            let saw = (2..7).contains(&x) && (5..12).contains(&y);
            let saw_teeth = x == 2 && y.is_multiple_of(2) && (5..12).contains(&y);
            let handle = (9..11).contains(&x) && (6..14).contains(&y);
            let head = (8..13).contains(&x) && (4..6).contains(&y);
            if top_band {
                shade([104, 74, 44], 0.85 + r * 0.1)
            } else if saw && !saw_teeth {
                shade([170, 170, 176], 0.9 + r * 0.15)
            } else if head {
                shade([110, 110, 116], 0.9 + r * 0.1)
            } else if handle {
                shade([96, 64, 36], 0.9 + r * 0.1)
            } else {
                pixel(tex::PLANKS, x, y)
            }
        }
        tex::FURNACE_TOP => noisy(layer, x, y, [118, 118, 118], 0.1),
        tex::FURNACE_SIDE => {
            // Smooth stone with a darker band at the top and bottom.
            let band = !(2..SIZE - 2).contains(&y);
            noisy(layer, x, y, if band { [96, 96, 96] } else { [128, 128, 128] }, 0.08)
        }
        tex::CHEST_TOP | tex::CHEST_SIDE | tex::CHEST_FRONT => {
            // Planks in a dark frame; sides have the lid seam, the front a latch.
            let edge = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            let seam = layer != tex::CHEST_TOP && (y == 5 || y == 6);
            let latch = layer == tex::CHEST_FRONT && (7..9).contains(&x) && (4..9).contains(&y);
            let board = y / 4;
            if latch {
                let rim = x == 7 && y == 4 || y == 8;
                shade([196, 196, 204], if rim { 0.7 } else { 1.0 + r * 0.1 })
            } else if edge || seam {
                shade([76, 52, 30], 0.85 + r * 0.15)
            } else {
                let grain = if (x + board * 5).is_multiple_of(7) { 0.8 } else { 0.95 + rnd(layer, x, board, 2) * 0.1 };
                shade([170, 120, 60], grain)
            }
        }
        tex::FURNACE_FRONT | tex::FURNACE_LIT => {
            // Cobbled face with a dark mouth; a lit furnace shows flames in it.
            let mouth = (4..12).contains(&x) && (8..14).contains(&y);
            let rim = (3..13).contains(&x) && (7..15).contains(&y) && !mouth;
            if mouth {
                let flame = layer == tex::FURNACE_LIT && y as f32 > 9.0 + 2.5 * rnd(layer, x, 0, 4);
                if flame {
                    let hot = (y as f32 - 9.0) / 5.0;
                    [255, (200.0 - hot * 110.0) as u8, (60.0 - hot * 40.0) as u8, 255]
                } else {
                    shade([24, 22, 22], 0.9 + r * 0.2)
                }
            } else if rim {
                shade([80, 80, 80], 0.9 + r * 0.15)
            } else {
                pixel(tex::COBBLESTONE, x, y)
            }
        }
        tex::FARMLAND | tex::WET_FARMLAND => {
            // Tilled dirt: furrows across, darker and richer when wet.
            let wet = if layer == tex::WET_FARMLAND { 0.62 } else { 1.0 };
            let furrow = match y % 4 {
                0 => 0.72,
                1 => 1.12,
                _ => 0.95 + r * 0.12,
            };
            shade([134, 96, 64], furrow * wet)
        }
        l if (tex::WHEAT_0..tex::WHEAT_0 + 8).contains(&l) => wheat(l - tex::WHEAT_0, x, y),
        l if (tex::NETHER_WART_0..tex::NETHER_WART_0 + 3).contains(&l) => nether_wart(l - tex::NETHER_WART_0, x, y),
        tex::OAK_SAPLING | tex::SPRUCE_SAPLING | tex::BIRCH_SAPLING | tex::JUNGLE_SAPLING | tex::ACACIA_SAPLING => {
            let (px, py) = (x as f32 - 7.5, y as f32);
            let stem = (x == 7 || x == 8) && y >= 10;
            let round = px * px + (py - 6.5) * (py - 6.5) <= 22.0 && rnd(layer, x, y, 5) > 0.15;
            let (crown, leaf, bark) = match layer {
                // Stacked triangles.
                tex::SPRUCE_SAPLING => (
                    y < 12 && px.abs() <= ((y % 4) as f32 + 1.0 + (y / 4) as f32 * 0.8).min(6.0),
                    [46, 92, 50],
                    [110, 80, 46],
                ),
                tex::BIRCH_SAPLING => (round, [104, 156, 64], [220, 218, 208]),
                // Broad, dark leaves fanning out.
                tex::JUNGLE_SAPLING => (
                    y < 11 && (px.abs() < 1.5 + py * 0.5 || (py - 7.0).abs() < 1.5) && rnd(layer, x, y, 5) > 0.1,
                    [40, 128, 26],
                    [110, 84, 40],
                ),
                // A flat-topped umbrella.
                tex::ACACIA_SAPLING => {
                    ((3..7).contains(&y) && px.abs() < 6.5 - (y as f32 - 3.0), [90, 126, 40], [120, 110, 100])
                }
                _ => (round, [72, 146, 44], [110, 80, 46]),
            };
            let stem = stem || (layer == tex::ACACIA_SAPLING && y >= 6 && x == 7 + (y < 10) as usize);
            if crown {
                shade(leaf, 0.75 + r * 0.45)
            } else if stem {
                shade(bark, if x == 7 { 1.0 } else { 0.8 })
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::SPAWNER => {
            // An iron cage: dark bars on a 4-pixel grid with a rim, open
            // in between so the inside shows.
            let bar = x.is_multiple_of(5) || y.is_multiple_of(5) || x == SIZE - 1 || y == SIZE - 1;
            if bar {
                let rivet = x.is_multiple_of(5) && y.is_multiple_of(5);
                shade([46, 56, 66], if rivet { 1.3 } else { 0.8 + r * 0.3 })
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::BREWING_SIDE | tex::BREWING_TOP => {
            // The rod down the middle, stone plates below (sides) or all
            // around it (top).
            let rod = (7..=8).contains(&x)
                && (layer == tex::BREWING_TOP && (7..=8).contains(&y) || layer == tex::BREWING_SIDE && y >= 2);
            let plate = layer == tex::BREWING_TOP || y >= 14;
            if rod {
                shade([128, 98, 60], if x == 7 { 1.1 } else { 0.85 } * (0.9 + r * 0.15))
            } else if plate {
                let edge = layer == tex::BREWING_SIDE && y == 14;
                shade([104, 104, 108], if edge { 1.15 } else { 0.8 + r * 0.3 })
            } else {
                [0, 0, 0, 0]
            }
        }
        tex::END_STONE => {
            let pit = rnd(layer, x / 2, y / 2, 31);
            shade([220, 224, 164], if pit < 0.22 { 0.74 + r * 0.08 } else { 0.91 + r * 0.14 })
        }
        tex::NETHERRACK => {
            // Lumpy dark red rock with darker cracks between the lumps.
            let pts = points(layer, 9);
            let (d1, d2, i) = voronoi(x, y, &pts);
            let crack = d2 - d1 < 0.8;
            let lump = 0.8 + rnd(layer, i, 0, 3) * 0.3;
            shade([111, 54, 52], if crack { 0.62 } else { lump + r * 0.14 })
        }
        tex::SOUL_SAND => {
            // Brown sand with a few dark, face-like hollows.
            let (cx, cy) = (x % 8, y % 8);
            let hollow = ((cx == 2 || cx == 5) && cy == 2) || ((2..=5).contains(&cx) && cy == 5);
            shade([84, 64, 51], if hollow { 0.55 } else { 0.85 + r * 0.3 })
        }
        tex::QUARTZ_ORE => {
            let pts = points(layer, 6);
            let (d1, _, _) = voronoi(x, y, &pts);
            if d1 < 1.3 { shade([235, 228, 220], 0.9 + r * 0.12) } else { pixel(tex::NETHERRACK, x, y) }
        }
        tex::NETHER_BRICKS => {
            // Small dark bricks, half-offset every row.
            let row = y / 4;
            let offset = if row.is_multiple_of(2) { 0 } else { 4 };
            if y % 4 == 3 || (x + offset) % 8 == 7 {
                shade([30, 15, 18], 1.0 + r * 0.2)
            } else {
                shade([68, 34, 40], 0.85 + rnd(layer, (x + offset) / 8, row, 1) * 0.2 + r * 0.1)
            }
        }
        tex::PORTAL => {
            // Swirling violet, see-through at the dark streaks.
            let (fx, fy) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);
            let tau = std::f32::consts::TAU;
            let swirl = (fx * tau + (fy * tau).sin() * 1.5).sin() * 0.5 + (fy * tau * 2.0 + fx * tau).cos() * 0.5;
            let k = (swirl * 0.5 + 0.5) * 0.7 + r * 0.3;
            let c = [(90.0 + 110.0 * k) as u8, (20.0 + 50.0 * k) as u8, (160.0 + 90.0 * k) as u8];
            [c[0], c[1], c[2], (150.0 + 90.0 * k) as u8]
        }
        tex::TNT_SIDE => {
            // Red paper with a white band across the middle and a fuse-dark
            // "TNT" stencilled on it.
            const LETTERS: [&str; 6] =
                ["### #  # ###", " #  ## #  # ", " #  ## #  # ", " #  # ##  # ", " #  # ##  # ", " #  #  #  # "];
            if (4..=11).contains(&y) {
                let ink = LETTERS.get(y.wrapping_sub(5)).and_then(|row| row.as_bytes().get(x.wrapping_sub(2)));
                if ink == Some(&b'#') { shade([40, 30, 30], 1.0) } else { shade([228, 226, 220], 0.92 + r * 0.08) }
            } else {
                let stripe = x.is_multiple_of(4);
                shade([200, 50, 38], if stripe { 0.78 } else { 0.9 + r * 0.15 })
            }
        }
        tex::TNT_TOP | tex::TNT_BOTTOM => {
            // Ends of the sticks, with a fuse in the middle on top.
            let (cx, cy) = (x % 4, y % 4);
            let edge = cx == 0 || cy == 0;
            let fuse = layer == tex::TNT_TOP && (7..=8).contains(&x) && (7..=8).contains(&y);
            if fuse { shade([60, 60, 60], 1.0) } else { shade([200, 50, 38], if edge { 0.7 } else { 0.95 + r * 0.1 }) }
        }
        tex::OBSIDIAN => {
            let speck = rnd(layer, x, y, 13) < 0.08;
            let c = if speck { [80, 60, 110] } else { [22, 16, 34] };
            shade(c, 0.85 + r * 0.3)
        }
        tex::HEART_FULL | tex::HEART_HALF | tex::HEART_EMPTY => heart(layer, x, y),
        tex::FOOD_FULL | tex::FOOD_HALF | tex::FOOD_EMPTY => drumstick(layer, x, y),
        tex::BUBBLE => {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let d = (dx * dx + dy * dy).sqrt();
            if (5.0..6.5).contains(&d) {
                [60, 110, 200, 255]
            } else if d < 5.0 {
                let shine = dx < -1.0 && dy < -1.0 && d > 2.0 && d < 4.0;
                if shine { [230, 245, 255, 255] } else { [120, 180, 250, 200] }
            } else {
                [0, 0, 0, 0]
            }
        }
        l if (tex::CRACK_0..tex::CRACK_0 + tex::CRACK_STAGES).contains(&l) => crack(l - tex::CRACK_0, x, y),
        l if let Some((base, group)) = tex::untinted(l) => tint_foliage(pixel(base, x, y), group),
        _ => {
            // Missing texture: magenta checkerboard.
            if (x / 4 + y / 4).is_multiple_of(2) { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        }
    }
}

/// Recolours the green parts of a grass or leaf pixel for a biome's
/// foliage group (see `tex::tinted`); dirt and transparent pixels stay.
fn tint_foliage(p: Rgba, group: u8) -> Rgba {
    let (r, g, b) = (p[0], p[1], p[2]);
    if p[3] == 0 || g <= r || g < b {
        return p;
    }
    let f: [f32; 3] = match group {
        1 => [0.85, 0.66, 0.75], // swamp: dark and murky
        2 => [1.35, 0.98, 0.85], // savanna and badlands: dry, yellow
        3 => [0.7, 1.08, 0.62],  // jungle: vivid
        _ => [0.92, 0.95, 1.45], // taiga and snow: cool, blue
    };
    let c = |v: u8, k: f32| (v as f32 * k).clamp(0.0, 255.0) as u8;
    [c(r, f[0]), c(g, f[1]), c(b, f[2]), p[3]]
}

/// A flower: stem with two leaves, and a yellow (dandelion) or red (poppy) head.
/// Wheat at growth `stage` 0..8: stalks rise and turn from green to gold;
/// ripe wheat carries grain heads.
fn wheat(stage: u8, x: usize, y: usize) -> Rgba {
    const STALKS: [usize; 5] = [2, 5, 8, 11, 14];
    let Some(i) = STALKS.iter().position(|&sx| sx == x || sx + 1 == x && x.is_multiple_of(3)) else {
        return [0, 0, 0, 0];
    };
    let height = 3 + stage as usize * 12 / 7 - (i % 2) * (1 + stage as usize / 3);
    if y + height < 16 {
        return [0, 0, 0, 0];
    }
    let ripe = stage as f32 / 7.0;
    let green = [72.0, 150.0, 40.0];
    let gold = [206.0, 176.0, 70.0];
    let c: [u8; 3] = std::array::from_fn(|k| (green[k] + (gold[k] - green[k]) * ripe * ripe) as u8);
    let top = 16 - height;
    if stage == 7 && y < top + 4 {
        // Grain head: wider, golden, notched.
        let notch = (x + y).is_multiple_of(2);
        return shade([224, 190, 84], if notch { 0.82 } else { 1.05 });
    }
    shade(c, 0.85 + rnd(tex::WHEAT_0 + stage, x, y, 9) * 0.25)
}

/// Nether wart at look `stage` 0..3: dark red shoots that thicken and,
/// once ripe, carry knobbly bulbs.
fn nether_wart(stage: u8, x: usize, y: usize) -> Rgba {
    const STALKS: [usize; 4] = [2, 6, 10, 13];
    let r = rnd(tex::NETHER_WART_0 + stage, x, y, 11);
    let width = if stage == 0 { 1 } else { 2 };
    // Ripe bulbs swell a pixel past the stalk on either side.
    let reach = if stage == 2 { 1 } else { 0 };
    let Some(i) = STALKS.iter().position(|&sx| x + reach >= sx && x < sx + width + reach) else {
        return [0, 0, 0, 0];
    };
    let height = [5, 8, 11][stage as usize] - (i % 2) * 2;
    let top = SIZE - height;
    let on_stalk = x >= STALKS[i] && x < STALKS[i] + width;
    if stage == 2 && y + 1 >= top && y < top + 4 {
        let knob = rnd(tex::NETHER_WART_0, x, y, 12) > 0.2;
        return if knob { shade([182, 38, 46], 0.75 + r * 0.45) } else { [0, 0, 0, 0] };
    }
    if !on_stalk || y < top {
        return [0, 0, 0, 0];
    }
    let tip = y < top + 2 + stage as usize;
    if tip { shade([164, 30, 38], 0.85 + r * 0.35) } else { shade([108, 18, 28], 0.8 + r * 0.3) }
}

fn flower(layer: u8, x: usize, y: usize, r: f32) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 5.0);
    let d = (dx * dx + dy * dy * 1.3).sqrt();
    let (petal, centre) =
        if layer == tex::DANDELION { ([245, 210, 40], [220, 170, 20]) } else { ([210, 30, 30], [40, 30, 20]) };
    if layer == tex::POPPY && d < 1.0 {
        shade(centre, 1.0)
    } else if d < if layer == tex::DANDELION { 2.6 } else { 3.3 } {
        shade(petal, 0.85 + r * 0.25)
    } else if (7..=8).contains(&x) && y >= 7 && x == 7 + (y / 5) % 2 {
        shade([60, 125, 40], 0.85 + r * 0.2)
    } else if (y == 10 && (4..7).contains(&x)) || (y == 12 && (9..12).contains(&x)) {
        shade([70, 140, 45], 0.85 + r * 0.2)
    } else {
        [0, 0, 0, 0]
    }
}

/// Hunger icon: a drumstick (meat upper left, bone lower right); full,
/// half (left side) or an empty outline.
fn drumstick(layer: u8, x: usize, y: usize) -> Rgba {
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let meat = |px: f32, py: f32| (px - 6.5).powi(2) + (py - 6.5).powi(2) * 1.2 < 22.0;
    let bone = |px: f32, py: f32| {
        // A shaft along the diagonal with a knob at the end.
        let t = ((px + py) / 2.0).clamp(8.0, 13.0);
        let shaft = (px - t).powi(2) + (py - t).powi(2) < 2.0;
        shaft || (px - 13.0).powi(2) + (py - 12.0).powi(2) < 2.5 || (px - 12.0).powi(2) + (py - 13.0).powi(2) < 2.5
    };
    let inside = |px: f32, py: f32| meat(px, py) || bone(px, py);
    if !inside(fx, fy) {
        return [0, 0, 0, 0];
    }
    let edge = !(inside(fx - 1.0, fy) && inside(fx + 1.0, fy) && inside(fx, fy - 1.0) && inside(fx, fy + 1.0));
    if edge {
        return [40, 20, 10, 255];
    }
    let filled = match layer {
        tex::FOOD_FULL => true,
        tex::FOOD_HALF => x < 8,
        _ => false,
    };
    if !filled {
        [60, 40, 30, 200]
    } else if meat(fx, fy) {
        if x < 6 && (3..6).contains(&y) { [240, 150, 90, 255] } else { [196, 96, 44, 255] }
    } else {
        [236, 228, 210, 255]
    }
}

/// Heart icon: full, half (left side filled) or empty outline.
fn heart(layer: u8, x: usize, y: usize) -> Rgba {
    // Implicit heart curve (x²+y²-1)³ - x²y³ <= 0, mapped onto the tile.
    let inside = |px: f32, py: f32| {
        let (hx, hy) = ((px - 7.5) / 6.2, (8.0 - py) / 6.2);
        (hx * hx + hy * hy - 1.0).powi(3) - hx * hx * hy.powi(3) <= 0.0
    };
    let (fx, fy) = (x as f32, y as f32);
    if !inside(fx, fy) {
        return [0, 0, 0, 0];
    }
    let edge = !(inside(fx - 1.0, fy) && inside(fx + 1.0, fy) && inside(fx, fy - 1.0) && inside(fx, fy + 1.0));
    if edge {
        return [30, 10, 10, 255];
    }
    let filled = match layer {
        tex::HEART_FULL => true,
        tex::HEART_HALF => x < 8,
        _ => false,
    };
    if !filled {
        [60, 30, 30, 200]
    } else if x < 6 && (3..6).contains(&y) {
        [255, 170, 170, 255] // highlight
    } else {
        [220, 30, 30, 255]
    }
}

/// Crack overlay used with multiplicative blending: mid-grey (linear 0.5)
/// leaves the block unchanged, darker pixels are cracks. Each stage shows
/// more of the same crack pattern.
fn crack(stage: u8, x: usize, y: usize) -> Rgba {
    const NEUTRAL: u8 = 188; // sRGB for linear 0.5
    // Crack pixels are laid down by random walks from the centre; each
    // pixel records the walk step at which it appears.
    static ORDER: std::sync::OnceLock<[u8; SIZE * SIZE]> = std::sync::OnceLock::new();
    let order = ORDER.get_or_init(|| {
        let mut order = [u8::MAX; SIZE * SIZE];
        for branch in 0..7 {
            let (mut px, mut py) = (7.5f32, 7.5f32);
            let angle = branch as f32 / 7.0 * std::f32::consts::TAU + rnd(99, branch, 0, 1) * 0.8;
            for step in 0..14u8 {
                let wobble = (rnd(99, branch, step as usize, 2) - 0.5) * 1.4;
                px += (angle + wobble).cos();
                py += (angle + wobble).sin();
                let (ix, iy) = (px as isize, py as isize);
                if !(0..SIZE as isize).contains(&ix) || !(0..SIZE as isize).contains(&iy) {
                    break;
                }
                let i = iy as usize * SIZE + ix as usize;
                order[i] = order[i].min(step);
            }
        }
        order
    });
    let visible_steps = (stage as u32 + 1) * 14 / tex::CRACK_STAGES as u32;
    if (order[y * SIZE + x] as u32) < visible_steps { [40, 40, 40, 255] } else { [NEUTRAL, NEUTRAL, NEUTRAL, 255] }
}

/// A pixel of any layer a renderer can name: block layers below
/// `tex::ITEM_BASE`, item icons from it on.
pub fn texel(layer: u16, x: usize, y: usize) -> Rgba {
    use crate::simulation::effects::Effect;
    let icons = crate::item::icon_count() as u16;
    match tex::item_index(layer) {
        Some(index) if index >= icons => Effect::ALL
            .get((index - icons) as usize)
            .map_or([0, 0, 0, 0], |&e| super::item_sprites::effect_pixel(e, x, y)),
        Some(index) => {
            crate::item::sprite_for_layer(index).map_or([0, 0, 0, 0], |s| super::item_sprites::pixel(s, x, y))
        }
        None => pixel(layer as u8, x, y),
    }
}

/// RGBA8 data for every mip level of the block texture array; each level
/// contains all layers back to back, ready for `write_texture`.
pub fn generate_mips() -> Vec<Vec<u8>> {
    mips_of(tex::COUNT as usize, |l, x, y| pixel(l as u8, x, y))
}

/// Layers of the item icon array: every item's icon, then the status
/// effect icons.
pub fn item_layers() -> u32 {
    crate::item::icon_count() + crate::simulation::effects::Effect::ALL.len() as u32
}

/// Mip levels of the item icon array (see [`generate_mips`]).
pub fn generate_item_mips() -> Vec<Vec<u8>> {
    mips_of(item_layers() as usize, |l, x, y| texel(tex::item_layer(l as u16), x, y))
}

fn mips_of(layers: usize, pixel: impl Fn(usize, usize, usize) -> Rgba) -> Vec<Vec<u8>> {
    let mut level: Vec<Rgba> = Vec::with_capacity(SIZE * SIZE * layers);
    for l in 0..layers {
        for y in 0..SIZE {
            for x in 0..SIZE {
                level.push(pixel(l, x, y));
            }
        }
    }

    let mut mips = vec![level.concat()];
    let mut size = SIZE;
    for _ in 1..MIP_LEVELS {
        let half = size / 2;
        let mut next = Vec::with_capacity(half * half * layers);
        for l in 0..layers {
            let src = &level[l * size * size..(l + 1) * size * size];
            for y in 0..half {
                for x in 0..half {
                    let px = [
                        src[(2 * y) * size + 2 * x],
                        src[(2 * y) * size + 2 * x + 1],
                        src[(2 * y + 1) * size + 2 * x],
                        src[(2 * y + 1) * size + 2 * x + 1],
                    ];
                    // Alpha-weighted average so transparent texels don't
                    // darken the colour of cutout edges.
                    let a: u32 = px.iter().map(|p| p[3] as u32).sum();
                    let mut out = [0u8; 4];
                    for (c, o) in out.iter_mut().take(3).enumerate() {
                        let w: u32 = px.iter().map(|p| p[c] as u32 * p[3] as u32).sum();
                        *o = w.checked_div(a).unwrap_or(0) as u8;
                    }
                    out[3] = (a / 4) as u8;
                    next.push(out);
                }
            }
        }
        mips.push(next.concat());
        level = next;
        size = half;
    }
    mips
}
