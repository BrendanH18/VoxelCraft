//! Procedurally generated 16x16 block textures, so the game ships no assets.
//!
//! Layer order must match `world::block::tex`.

use crate::world::block::tex;
use crate::world::noise::hash_f;

pub const SIZE: usize = 16;
pub const MIP_LEVELS: u32 = 5; // 16, 8, 4, 2, 1

type Rgba = [u8; 4];

fn rnd(layer: u8, x: usize, y: usize, salt: i32) -> f32 {
    hash_f(x as i32, y as i32, salt, 0xB10C ^ layer as u64)
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
    (0..n)
        .map(|i| (rnd(layer, i, 0, 99) * SIZE as f32, rnd(layer, i, 1, 99) * SIZE as f32))
        .collect()
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
            if rnd(layer, x, y, 11) < 0.2 {
                [0, 0, 0, 0]
            } else {
                shade(base, 0.7 + r * 0.45)
            }
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
            if d2 - d1 < 1.1 {
                shade(STONE, 0.55)
            } else {
                shade(STONE, 0.8 + rnd(layer, i, 0, 4) * 0.35 - d1 * 0.03)
            }
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
            if spike {
                [230, 230, 190, 255]
            } else {
                shade([76, 140, 48], if line { 0.75 } else { 0.95 + r * 0.1 })
            }
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
            if d2 - d1 < 0.9 {
                [150, 110, 60, 255]
            } else {
                shade([250, 215, 120], 0.8 + rnd(layer, i, 0, 4) * 0.25)
            }
        }
        _ => {
            // Missing texture: magenta checkerboard.
            if (x / 4 + y / 4).is_multiple_of(2) { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        }
    }
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
