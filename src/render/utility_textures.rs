//! Procedural textures for the v0.12 utility blocks (layers from
//! `utilities::TEX`): original artwork for cauldrons, flower pots, potted
//! plants, lodestones, respawn anchors, campfires, coral wall fans, ender
//! and trapped chests, decorated pots and the double chest halves.
//!
//! Shaped blocks sample a face texture by position in the cell: a side
//! texel `(x, row)` is the cell's `y = 15 - row`, and a top texel `(x, y)`
//! is the cell's `(x, z)`. Each texture puts its parts where the model's
//! boxes are.

use super::{Rgba, noisy, pixel as base_pixel, rnd, shade};
use crate::world::block::tex;
use crate::world::utilities::{self as u, t};

const CLEAR: Rgba = [0, 0, 0, 0];
const IRON: [u8; 3] = [74, 74, 80];
const TERRACOTTA: [u8; 3] = [152, 84, 58];
const DIRT: [u8; 3] = [116, 84, 58];
const BARK: [u8; 3] = [92, 70, 44];
const WOOD: [u8; 3] = [168, 132, 84];
const CORAL: [[u8; 3]; 5] = [[50, 90, 210], [206, 80, 160], [160, 30, 190], [190, 36, 46], [216, 200, 66]];
const DEAD_CORAL: [u8; 3] = [130, 122, 118];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    // Double chest halves copy their single chest's texture without the
    // frame at the joint, each showing half the latch.
    if let Some((single, rows, hi)) = crate::world::double_chest::texture(layer) {
        use crate::world::double_chest::joint_source;
        return if rows {
            base_pixel(single, x, joint_source(y, hi))
        } else {
            base_pixel(single, joint_source(x, hi), y)
        };
    }
    let r = rnd(layer, x, y, 0);
    match layer {
        t::CAULDRON_SIDE => {
            // Riveted iron plates; the rim darkens the top rows.
            let rivet = (x == 2 || x == 13) && (y == 3 || y == 10);
            let seam = y == 7;
            let f = if rivet {
                1.35
            } else if seam || y < 1 {
                0.7
            } else {
                0.9 + r * 0.18
            };
            shade(IRON, f)
        }
        t::CAULDRON_BOTTOM => shade(IRON, if (x / 4 + y / 4).is_multiple_of(2) { 0.75 } else { 0.85 } + r * 0.1),
        t::CAULDRON_TOP..=t::CAULDRON_SNOW => {
            let rim = !(2..14).contains(&x) || !(2..14).contains(&y);
            if rim {
                return shade(IRON, if x == 0 || y == 0 || x == 15 || y == 15 { 0.75 } else { 1.05 + r * 0.12 });
            }
            match layer {
                t::CAULDRON_WATER => {
                    let wave = ((x as f32 * 0.9 + y as f32 * 0.5).sin() * 0.5 + 0.5) * 0.12;
                    shade([52, 96, 214], 0.88 + wave + r * 0.06)
                }
                t::CAULDRON_LAVA => {
                    let hot = rnd(layer, x / 2, y / 2, 4);
                    shade(if hot < 0.3 { [255, 196, 64] } else { [214, 92, 18] }, 0.9 + r * 0.15)
                }
                t::CAULDRON_SNOW => noisy(layer, x, y, [234, 244, 250], 0.03),
                // The empty floor deep inside.
                _ => shade([40, 40, 46], 0.85 + r * 0.2),
            }
        }
        t::POT_SIDE | t::POT_BOTTOM => pot(layer, y, r),
        t::POT_SOIL | t::POT_EMPTY => {
            let wall = (5..11).contains(&x) && (5..11).contains(&y) && !((6..10).contains(&x) && (6..10).contains(&y));
            if wall { shade(TERRACOTTA, 1.05 + r * 0.1) } else { noisy(layer, x, y, DIRT, 0.16) }
        }
        t::POTTED_CACTUS_TOP => {
            if (6..10).contains(&x) && (6..10).contains(&y) {
                base_pixel(tex::CACTUS_TOP, x, y)
            } else {
                pixel(t::POT_SOIL, x, y)
            }
        }
        l if (t::POTTED..t::POTTED + u::POTTED.len() as u16).contains(&l) => {
            potted(l, (l - t::POTTED) as usize, x, y, r)
        }
        t::LODESTONE_SIDE => {
            // Chiseled stone with an iron band around the middle.
            let band = (6..10).contains(&y);
            if band {
                let stud = x % 4 == 1 && y == 7;
                shade(IRON, if stud { 1.4 } else { 0.95 + r * 0.15 })
            } else {
                let mortar = y == 0 || y == 15 || y == 5 || y == 10 || (x == 0 || x == 15);
                shade([150, 150, 150], if mortar { 0.7 } else { 0.92 + r * 0.14 })
            }
        }
        t::LODESTONE_TOP => {
            let ring = x.min(y).min(15 - x).min(15 - y);
            let f = match ring {
                0 => 0.7,
                3 | 6 => 0.8,
                7 => 1.3,
                _ => 0.95 + r * 0.1,
            };
            shade(if ring >= 4 { IRON } else { [150, 150, 150] }, f)
        }
        l if (t::ANCHOR_SIDE..t::ANCHOR_SIDE + 5).contains(&l) => {
            // Crying obsidian with a glowstone gauge: one lit cell per charge.
            let charges = (l - t::ANCHOR_SIDE) as usize;
            let gauge = (3..13).contains(&x) && (3..13).contains(&y) && (x - 3) % 5 < 4 && (y - 3) % 5 < 4;
            if gauge {
                let cell = (y - 3).min(9) / 5 * 2 + (x - 3).min(9) / 5;
                return if cell < charges {
                    shade([255, 168, 60], 0.85 + r * 0.25)
                } else {
                    shade([40, 26, 58], 0.8 + r * 0.2)
                };
            }
            obsidian(layer, x, y, r)
        }
        t::ANCHOR_TOP_OFF | t::ANCHOR_TOP_ON => {
            let d = (x as f32 - 7.5).abs().max((y as f32 - 7.5).abs());
            if d < 3.0 {
                return if layer == t::ANCHOR_TOP_ON {
                    shade([186, 70, 236], 0.8 + r * 0.4)
                } else {
                    shade([20, 12, 30], 0.9 + r * 0.2)
                };
            }
            obsidian(layer, x, y, r)
        }
        t::ANCHOR_BOTTOM => obsidian(layer, x, y, r),
        l if (t::CAMPFIRE..t::CAMPFIRE + 9).contains(&l) => campfire(l - t::CAMPFIRE, x, y, r),
        l if (t::WALL_FAN..t::WALL_FAN + 40).contains(&l) => wall_fan(l - t::WALL_FAN, x, y, r),
        t::ENDER_FRONT | t::ENDER_SIDE | t::ENDER_TOP => {
            let border = x == 0 || y == 0 || x == 15 || y == 15;
            let lid = y == 5 || y == 6;
            let eye = layer == t::ENDER_FRONT && (6..10).contains(&x) && (4..8).contains(&y);
            if eye {
                let pupil = (7..9).contains(&x) && (5..7).contains(&y);
                return if pupil { shade([10, 40, 30], 1.0) } else { shade([80, 220, 160], 0.85 + r * 0.25) };
            }
            let c = if border || (layer != t::ENDER_TOP && lid) { [44, 64, 60] } else { [20, 30, 32] };
            shade(c, 0.85 + r * 0.3)
        }
        t::TRAPPED_FRONT => {
            // The chest front with a hook-red latch.
            if (7..9).contains(&x) && (4..8).contains(&y) {
                return shade([168, 44, 36], 0.9 + r * 0.15);
            }
            base_pixel(tex::CHEST_FRONT, x, y)
        }
        t::DECORATED_SIDE | t::DECORATED_CRACKED => {
            let band = y == 4 || y == 12;
            let mut p = shade(TERRACOTTA, if band { 0.72 } else { 0.92 + r * 0.12 });
            if layer == t::DECORATED_CRACKED && ((x * 3 + y * 2).is_multiple_of(11) || (x + 2 * y) % 13 == 1) {
                p = shade([60, 34, 24], 1.0);
            }
            p
        }
        t::DECORATED_TOP => {
            let d = (x as f32 - 7.5).abs().max((y as f32 - 7.5).abs());
            if d < 2.5 { shade([46, 26, 18], 1.0) } else { shade(TERRACOTTA, 0.9 + r * 0.12) }
        }
        _ => CLEAR,
    }
}

/// Terracotta pot sides: the pot fills the bottom six rows (`y` 0..6).
fn pot(layer: u16, y: usize, r: f32) -> Rgba {
    if y < 10 && layer == t::POT_SIDE {
        return CLEAR;
    }
    let lip = y == 10;
    shade(TERRACOTTA, if lip { 1.1 } else { 0.88 + r * 0.14 })
}

/// A potted plant's side: the pot, and above it the plant's own sprite
/// squeezed onto the cross planes (columns 3..13, `y` 4..16).
fn potted(layer: u16, i: usize, x: usize, y: usize, r: f32) -> Rgba {
    let in_pot = y >= 10 && (5..11).contains(&x);
    if in_pot || y >= 12 {
        return pot(t::POT_BOTTOM, y, r);
    }
    let (plant, _) = u::POTTED[i];
    match i {
        u::POTTED_CACTUS => {
            let spine = (x + y).is_multiple_of(4) && (x == 6 || x == 9);
            return shade([76, 128, 48], if spine { 1.4 } else { 0.85 + r * 0.2 });
        }
        u::POTTED_BAMBOO => {
            let node = y.is_multiple_of(5);
            return shade([96, 148, 50], if node { 0.7 } else { 0.9 + r * 0.15 });
        }
        _ => {}
    }
    if !(3..13).contains(&x) {
        return CLEAR;
    }
    let sx = ((x - 3) * 16 / 10).min(15);
    let sy = (y * 16 / 12).min(15);
    let _ = layer;
    base_pixel(plant.info().tex[0], sx, sy)
}

fn obsidian(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let tear = rnd(layer, x / 2, y / 3, 7) < 0.12;
    if tear { shade([120, 40, 200], 0.8 + r * 0.3) } else { shade([22, 14, 34], 0.8 + r * 0.4) }
}

/// Campfire faces: `set` 0 lit, 3 unlit, 6 lit soul; within a set, log
/// ends, log lengths, then the top.
fn campfire(i: u16, x: usize, y: usize, r: f32) -> Rgba {
    let (set, part) = (i / 3, i % 3);
    let lit = set != 1;
    let soul = set == 2;
    let glow = if soul { [90, 230, 240] } else { [255, 170, 40] };
    if part == 2 {
        // The top: log bark with glowing cracks when lit.
        let crack = lit && (x * 5 + y * 3).is_multiple_of(9);
        return if crack { shade(glow, 0.9 + r * 0.2) } else { shade(BARK, 0.8 + r * 0.25) };
    }
    // Rows 0..9 are above the logs (`y` 7..16): the flames.
    if y < 9 {
        if !lit || !(1..15).contains(&x) {
            return CLEAR;
        }
        let (dx, h) = (x as f32 - 7.5, 9.0 - y as f32);
        let width = 6.5 - h * 0.6 + (rnd(i, x, y / 2, 3) - 0.5) * 2.0;
        if dx.abs() > width {
            return CLEAR;
        }
        let core = dx.abs() < width * 0.45;
        let c = match (soul, core) {
            (false, true) => [255, 230, 120],
            (false, false) => [240, 120, 30],
            (true, true) => [200, 255, 255],
            (true, false) => [60, 200, 220],
        };
        return shade(c, 0.85 + r * 0.25);
    }
    // Rows 9..13 (`y` 3..7) hold the upper logs, rows 12..16 the lower.
    let upper = y < 13;
    let ring = |x: usize, lo: usize| {
        let (cx, cy) = (x as f32 - (lo as f32 + 1.5), y as f32 - if upper { 10.5 } else { 13.5 });
        let d = (cx * cx + cy * cy).sqrt();
        if d > 1.6 { shade(BARK, 0.85 + r * 0.2) } else { shade(WOOD, if d < 0.6 { 0.8 } else { 1.0 + r * 0.1 }) }
    };
    let end_part = (part == 0) != upper;
    if end_part {
        // Log ends at columns 1..5 and 11..15.
        if (1..5).contains(&x) {
            return ring(x, 1);
        }
        if (11..15).contains(&x) {
            return ring(x, 11);
        }
        // Between the lower logs: the ember bed.
        if !upper && y == 15 {
            return if lit { shade(glow, 0.7 + r * 0.4) } else { shade([60, 56, 52], 0.8 + r * 0.3) };
        }
        return shade([40, 30, 22], 0.9);
    }
    let crack = lit && (x * 7 + y).is_multiple_of(11);
    if crack { shade(glow, 0.9 + r * 0.2) } else { shade(BARK, if y.is_multiple_of(4) { 0.75 } else { 0.9 + r * 0.2 }) }
}

/// A coral fan seen from above, spreading out from its wall: `i` is
/// `(dead * 5 + colour) * 4 + facing` in `utilities::FACINGS` order.
fn wall_fan(i: u16, x: usize, y: usize, r: f32) -> Rgba {
    let (kind, facing) = (i / 4, i % 4);
    let c = if kind >= 5 { DEAD_CORAL } else { CORAL[kind as usize] };
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    // Distance out from the wall, and across.
    let (d, l) = match facing {
        0 => (16.0 - fy, fx - 8.0),
        1 => (fx, fy - 8.0),
        2 => (fy, fx - 8.0),
        _ => (16.0 - fx, fy - 8.0),
    };
    if d < 0.5 || d > 11.0 || l.abs() > 1.5 + d * 0.62 {
        return CLEAR;
    }
    let rib = ((l / (d + 1.0)) * 6.0).fract().abs() < 0.2;
    shade(c, if rib { 0.78 } else { 0.92 + r * 0.14 })
}
