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

fn pixel(layer: u8, x: usize, y: usize) -> Rgba {
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
        tex::LOG_SIDE => {
            let stripe = (x + (rnd(layer, 0, y / 4, 3) * 2.0) as usize).is_multiple_of(4);
            shade([104, 82, 51], if stripe { 0.72 } else { 0.9 + r * 0.15 })
        }
        tex::LOG_TOP => {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let d = (dx * dx + dy * dy).sqrt();
            if d > 6.6 {
                shade([104, 82, 51], 0.9 + r * 0.1)
            } else {
                let ring = (d * 1.1) as i32 % 2 == 0;
                shade(if ring { [176, 142, 88] } else { [150, 118, 70] }, 0.95 + r * 0.08)
            }
        }
        tex::LEAVES | tex::SPRUCE_LEAVES => {
            let base = if layer == tex::LEAVES { [58, 128, 38] } else { [44, 92, 56] };
            if rnd(layer, x, y, 11) < 0.2 { [0, 0, 0, 0] } else { shade(base, 0.7 + r * 0.45) }
        }
        tex::PLANKS => {
            let board = y / 4;
            let seam_x = (board * 7 + 3) % SIZE;
            let seam = y % 4 == 3 || x == seam_x;
            shade([162, 130, 78], if seam { 0.68 } else { 0.92 + rnd(layer, x, board, 2) * 0.12 })
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
        tex::OAK_SAPLING | tex::SPRUCE_SAPLING => {
            let spruce = layer == tex::SPRUCE_SAPLING;
            let (px, py) = (x as f32 - 7.5, y as f32);
            let stem = (x == 7 || x == 8) && y >= 10;
            let crown = if spruce {
                // Stacked triangles.
                y < 12 && px.abs() <= ((y % 4) as f32 + 1.0 + (y / 4) as f32 * 0.8).min(6.0)
            } else {
                px * px + (py - 6.5) * (py - 6.5) <= 22.0 && rnd(layer, x, y, 5) > 0.15
            };
            if crown {
                let c = if spruce { [46, 92, 50] } else { [72, 146, 44] };
                shade(c, 0.75 + r * 0.45)
            } else if stem {
                shade([110, 80, 46], if x == 7 { 1.0 } else { 0.8 })
            } else {
                [0, 0, 0, 0]
            }
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
        l if l >= tex::ITEM_0 => match crate::item::sprite_for_layer(l - tex::ITEM_0) {
            Some(sprite) => super::item_sprites::pixel(sprite, x, y),
            None => [0, 0, 0, 0],
        },
        _ => {
            // Missing texture: magenta checkerboard.
            if (x / 4 + y / 4).is_multiple_of(2) { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        }
    }
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

/// Returns RGBA8 data for every mip level; each level contains all layers
/// back to back, ready for `write_texture`.
pub fn generate_mips() -> Vec<Vec<u8>> {
    let layers = tex::COUNT as usize;
    let mut level: Vec<Rgba> = Vec::with_capacity(SIZE * SIZE * layers);
    for l in 0..layers {
        for y in 0..SIZE {
            for x in 0..SIZE {
                level.push(pixel(l as u8, x, y));
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
