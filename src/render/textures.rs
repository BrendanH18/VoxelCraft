//! Procedurally generated 16x16 block textures, so the game ships no assets.
//!
//! Layer order must match `world::block::tex`.

use crate::world::block::tex;
use crate::world::noise::hash_f;

#[path = "archaeology_textures.rs"]
mod archaeology;
#[path = "bee_textures.rs"]
mod bees;
#[path = "cave_textures.rs"]
mod cave;
#[path = "colour_textures.rs"]
mod colour;
#[path = "copper_textures.rs"]
mod copper;
#[path = "copper_golem_statue_textures.rs"]
mod copper_golem_statue;
#[path = "gen5_textures.rs"]
mod gen5;
#[path = "material_textures.rs"]
mod material;
#[path = "mob_textures.rs"]
mod mob;
#[path = "nether_textures.rs"]
mod nether;
#[path = "overworld_textures.rs"]
mod overworld;
#[path = "stone_textures.rs"]
mod stone;
#[path = "utility_textures.rs"]
mod utility;
#[path = "wood_textures.rs"]
mod wood;

pub const SIZE: usize = 16;
pub const MIP_LEVELS: u32 = 5; // 16, 8, 4, 2, 1

type Rgba = [u8; 4];

/// Deterministic texture noise; `variation` selects a pattern within the layer.
/// This is a procedural graphics helper with no cryptographic purpose.
fn rnd(layer: u16, x: usize, y: usize, variation: i32) -> f32 {
    hash_f(x as i32, y as i32, variation, 0xB10C ^ layer as u64)
}

fn shade(c: [u8; 3], f: f32) -> Rgba {
    let s = |v: u8| (v as f32 * f).clamp(0.0, 255.0) as u8;
    [s(c[0]), s(c[1]), s(c[2]), 255]
}

fn noisy(layer: u16, x: usize, y: usize, c: [u8; 3], amount: f32) -> Rgba {
    shade(c, 1.0 - amount + rnd(layer, x, y, 0) * amount * 2.0)
}

fn glazed_pixel(color: usize, rot: u16, x: usize, y: usize) -> Rgba {
    let (x, y) = match rot {
        1 => (y, SIZE - 1 - x),
        2 => (SIZE - 1 - x, SIZE - 1 - y),
        3 => (SIZE - 1 - y, x),
        _ => (x, y),
    };
    let rgb = crate::color::DyeColor::ALL[color].rgb();
    let (cx, cy) = (x as i32 - 8, y as i32 - 8);
    let mark = match color {
        0 => (cx + cy).unsigned_abs().is_multiple_of(6),
        1 => (cx.abs() + cy.abs()).unsigned_abs().is_multiple_of(5),
        2 => (x / 4 + y / 4).is_multiple_of(2),
        3 => cx.abs() <= 1 || cy.abs() <= 1,
        4 => (cx * cx + cy * cy) % 18 < 8,
        5 => (x + 2 * y).is_multiple_of(5),
        6 => (cx.abs() - cy.abs()).unsigned_abs() < 2,
        7 => (x.max(y) - x.min(y) < 3) && (x + y).is_multiple_of(2),
        8 => (x % 5 < 2) != (y % 5 < 2),
        9 => (cx * 3 + cy * 2).unsigned_abs().is_multiple_of(7),
        10 => cx.abs() == cy.abs() || cx == 0 || cy == 0,
        11 => (x / 2).is_multiple_of(2) != (y / 3).is_multiple_of(2),
        12 => (cx + 2 * cy).unsigned_abs() % 8 < 3,
        13 => (x as i32 - y as i32).unsigned_abs().is_multiple_of(4),
        14 => (cx.abs() + 2 * cy.abs()) % 6 < 3,
        _ => (x + y * 3).is_multiple_of(7) || x == 8 || y == 8,
    };
    shade(rgb, if mark { 1.02 } else { 0.7 })
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

fn points(layer: u16, n: usize) -> Vec<(f32, f32)> {
    (0..n).map(|i| (rnd(layer, i, 0, 99) * SIZE as f32, rnd(layer, i, 1, 99) * SIZE as f32)).collect()
}

const STONE: [u8; 3] = [125, 125, 125];

/// v1.0 gap Track 2c: a mob material pattern's texel (layers
/// `entity::model::PATTERN_TEX`..), for the mount screen's preview, which
/// draws without the GPU's entity shader.
pub fn mob_pattern_pixel(layer: u16, x: usize, y: usize) -> Rgba {
    mob::pixel(layer, x % SIZE, y % SIZE)
}

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    // v0.18 Track 3: the materials, workstations and fixtures it redrew.
    // Stone (layer 0) stays the authored asset; nothing here draws it.
    if let Some(pixel) = material::pixel(layer, x, y) {
        return pixel;
    }
    // v0.18 Track 6: the mob material patterns.
    if crate::entity::model::Pattern::from_layer(layer).is_some() {
        return mob::pixel(layer, x, y);
    }
    // v0.18 Track 9: a barrel's open top, while it is in use.
    // v0.16 Track C: heads, the wither rose and its pot, the beacon.
    if (crate::world::skulls::TEX..crate::world::skulls::TEX_END).contains(&layer) {
        return crate::world::skulls::pixel(layer, x, y);
    }
    // The wither rose itself is drawn by `material` (v1.0 Track 10).
    if let Some(plant) = crate::world::wither_rose::pot_layer_plant(layer) {
        return utility::potted_side(layer, plant, x, y);
    }
    if (crate::world::conduit::SHELL_TEX..crate::world::conduit::TEX_END).contains(&layer) {
        return crate::world::conduit::pixel(layer, x, y);
    }
    if (crate::world::beacon::GLASS_TEX..crate::world::beacon::TEX_END).contains(&layer) {
        return crate::world::beacon::pixel(layer, x, y);
    }
    // v0.17 Track C1: the crafter.
    if (crate::world::crafter::TEX..crate::world::crafter::TEX_END).contains(&layer) {
        return crate::world::crafter::pixel(layer, x, y);
    }
    if (crate::world::camp_maps::TEX..crate::world::camp_maps::TEX_END).contains(&layer) {
        return crate::world::camp_maps::pixel(layer, x, y);
    }
    // v0.16 Track G: the End crystal's icon.
    if (crate::entity::crystal::ICON_TEX..crate::entity::crystal::TEX_END).contains(&layer) {
        return crate::entity::crystal::icon_pixel(x, y);
    }
    if gen5::owns(layer) {
        return gen5::pixel(layer, x, y);
    }
    // The eyeblossoms themselves are drawn by `material` (v1.0 Track 10).
    if let Some(plant) = crate::world::eyeblossom::pot_layer_plant(layer) {
        return utility::potted_side(layer, plant, x, y);
    }
    if (crate::world::creaking::TEX..crate::world::creaking::TEX_END).contains(&layer) {
        return crate::world::creaking::pixel(layer, x, y);
    }
    if (crate::world::frog_blocks::tex::BASE..crate::world::frog_blocks::tex::END).contains(&layer) {
        return crate::world::frog_blocks::pixel(layer, x, y);
    }
    if (2956..=2959).contains(&layer) {
        return super::item_sprites::map_decoration_pixel(layer - 2956, x, y);
    }
    if layer == 2955 {
        let color = (y * 16 + x) as u8;
        return if color / 4 == 0 { [229, 213, 164, 255] } else { crate::world::maps::rgba(color) };
    }
    if (2944..=2954).contains(&layer) {
        return super::item_sprites::pixel(crate::item::Sprite::Map(layer > 2945), x, y);
    }
    if let Some(pixel) = voxelcraft::assets::texture_pixel(layer, x, y) {
        return pixel;
    }
    if (crate::world::wood::tex::BASE..crate::world::wood::tex::END).contains(&layer) {
        return wood::pixel(layer, x, y);
    }
    if (crate::world::colour_blocks::tex::TEX..crate::world::colour_blocks::tex::TEX_END).contains(&layer) {
        return colour::pixel(layer, x, y);
    }
    if (tex::HONEY_SIDE..=tex::HONEY_BOTTOM).contains(&layer) {
        // Original amber shell: streaks and small trapped air pockets.
        let edge = x == 0 || y == 0 || x == 15 || y == 15;
        let bubble = ((x * 7 + y * 11 + layer as usize) % 41) < 3;
        let streak = layer == tex::HONEY_SIDE && (x + 3 * (y / 5)).is_multiple_of(7);
        let mut p = noisy(
            layer,
            x,
            y,
            if edge {
                [239, 177, 53]
            } else if bubble {
                [255, 207, 93]
            } else if streak {
                [205, 127, 20]
            } else {
                [231, 153, 28]
            },
            0.06,
        );
        p[3] = if edge { 220 } else { 165 };
        return p;
    }
    let r = rnd(layer, x, y, 0);
    if layer == tex::SCAFFOLD_TOP {
        let slat = x % 4 < 2 || y % 4 < 2;
        return if slat { noisy(layer, x, y, [186, 166, 91], 0.10) } else { [0, 0, 0, 0] };
    }
    if layer == tex::SCAFFOLD_SIDE {
        return noisy(layer, x, y, if y % 7 < 2 { [121, 112, 61] } else { [195, 180, 102] }, 0.12);
    }
    if layer == tex::BUBBLE_UP || layer == tex::BUBBLE_DOWN {
        // Water with pale bubbles; the magma column is a shade darker.
        let down = layer == tex::BUBBLE_DOWN;
        let c = noisy(layer, x, y, if down { [38, 78, 176] } else { [44, 90, 200] }, 0.08);
        let bubble = rnd(layer, x, y, 7) < 0.09;
        return if bubble { [205, 228, 255, 200] } else { [c[0], c[1], c[2], 170] };
    }
    if (tex::VILLAGE..tex::VILLAGE + 45).contains(&layer) {
        // `material` draws every village block's faces (v0.18 Track 3 and
        // v1.0 Track 10); the unused face slots show the missing texture.
        return [255, 0, 255, 255];
    }
    if (1139..1155).contains(&layer) {
        return special_rail_pixel(layer, x, y);
    }
    if (tex::SOUL_FIRE_0..=1399).contains(&layer) {
        return nether::pixel(layer, x, y);
    }
    if (1750..=1752).contains(&layer) {
        let speckle = rnd(layer, x, y, 1) < 0.14;
        let crack = layer > 1750 && ((x + y * 2).is_multiple_of(13) || layer == 1752 && (x * 3 + y).is_multiple_of(11));
        return shade(
            if crack {
                [69, 83, 60]
            } else if speckle {
                [100, 150, 90]
            } else {
                [236, 232, 212]
            },
            0.92 + rnd(layer, x, y, 0) * 0.08,
        );
    }
    if (crate::world::overworld_blocks::TEX..crate::world::overworld_blocks::TEX_END).contains(&layer) {
        return overworld::pixel(layer, x, y);
    }
    if (crate::world::stone_blocks::t::TEX..crate::world::stone_blocks::t::END).contains(&layer) {
        return stone::pixel(layer, x, y);
    }
    if (crate::world::archaeology::t::BASE..crate::world::archaeology::t::END).contains(&layer) {
        return archaeology::pixel(layer, x, y);
    }
    if (crate::world::copper_golem_statue::TEXTURE_FIRST..crate::world::copper_golem_statue::TEXTURE_END)
        .contains(&layer)
    {
        return copper_golem_statue::pixel(layer, x, y);
    }
    if (crate::world::copper::t::BASE..crate::world::copper::t::END).contains(&layer) {
        return copper::pixel(layer, x, y);
    }
    if (crate::world::utilities::TEX..crate::world::utilities::TEX_END).contains(&layer) {
        return utility::pixel(layer, x, y);
    }
    if (crate::world::bees::t::BASE..crate::world::bees::t::END).contains(&layer) {
        return bees::pixel(layer, x, y);
    }
    if (crate::world::jukebox::t::BASE..crate::world::jukebox::t::END).contains(&layer) {
        return crate::world::jukebox::pixel(layer, x, y);
    }
    if (crate::world::trial::TEX..crate::world::trial::TEX_END).contains(&layer) {
        return crate::world::trial::pixel(layer, x, y);
    }
    if (crate::world::explorer_maps::TEX..crate::world::explorer_maps::TEX_END).contains(&layer) {
        return crate::world::explorer_maps::pixel(layer, x, y);
    }
    if (crate::world::sniffer_egg::TEX..crate::world::sniffer_egg::TEXTURE_END).contains(&layer) {
        return crate::world::sniffer_egg::pixel(layer, x, y);
    }
    if (crate::world::workstations::TEX..crate::world::workstations::TEX_END).contains(&layer) {
        return crate::world::workstations::pixel(layer, x, y);
    }
    if (crate::world::heavy_core::TEX..crate::world::heavy_core::TEX_END).contains(&layer) {
        return crate::world::heavy_core::pixel(layer, x, y);
    }
    if (crate::world::end_blocks::TEX..crate::world::end_blocks::TEX_END).contains(&layer) {
        return crate::world::end_blocks::pixel(layer, x, y);
    }
    if (crate::world::dried_ghast::TEX..crate::world::dried_ghast::TEX_END).contains(&layer) {
        return crate::world::dried_ghast::pixel(layer, x, y);
    }
    // v0.16 Track B: shulker boxes.
    if (crate::world::shulker_box::TEX..crate::world::shulker_box::TEX_END).contains(&layer) {
        return crate::world::shulker_box::pixel(layer, x, y);
    }
    if layer == voxelcraft::content::elytra::WING_TEX {
        return voxelcraft::content::elytra::wing_pixel(x, y);
    }
    if (crate::world::cave_blocks::TEX..crate::world::cave_blocks::TEX_END).contains(&layer) {
        return cave::pixel(layer, x, y);
    }
    if (crate::world::cave_blocks::CAL_TEX..crate::world::cave_blocks::CAL_TEX_END).contains(&layer) {
        return cave::pixel(layer, x, y);
    }
    if (tex::STAINED_GLASS..tex::STAINED_GLASS + 16).contains(&layer) {
        let rgb = crate::color::DyeColor::ALL[(layer - tex::STAINED_GLASS) as usize].rgb();
        let border = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
        let pane = x == 1 || y == 1 || x == SIZE - 2 || y == SIZE - 2;
        if border {
            return shade(rgb, 0.45 + r * 0.1);
        }
        if pane {
            return shade(rgb, 0.7);
        }
        let mut p = shade(rgb, 0.85 + r * 0.08);
        p[3] = 150;
        return p;
    }
    if (tex::GLAZED..tex::GLAZED + 64).contains(&layer) {
        let i = layer - tex::GLAZED;
        return glazed_pixel((i / 4) as usize, i % 4, x, y);
    }
    if (tex::COLORED_BED..tex::COLORED_BED + 64).contains(&layer) {
        let color = crate::color::DyeColor::ALL[((layer - tex::COLORED_BED) / 4) as usize];
        let part = (layer - tex::COLORED_BED) % 4;
        let base = [tex::BED_TOP_FOOT, tex::BED_TOP_HEAD, tex::BED_SIDE_FOOT, tex::BED_SIDE_HEAD][part as usize];
        let mut p = pixel(base, x, y);
        // Recolour only the blanket, retaining the pillow and wooden frame.
        if p[0] / 2 > p[1] && p[0] / 2 > p[2] {
            let rgb = color.rgb();
            p = shade(rgb, p[0] as f32 / 178.0);
        }
        return p;
    }
    match layer {
        1100..=1115 => {
            let p = (layer - 1100) as f32 / 15.0;
            noisy(
                layer,
                x,
                y,
                [
                    (if p == 0.0 { 0.3 } else { p * 0.6 + 0.4 } * 255.0) as u8,
                    ((p * p * 0.7 - 0.5).max(0.0) * 255.0) as u8,
                    ((p * p * 0.6 - 0.7).max(0.0) * 255.0) as u8,
                ],
                0.1,
            )
        }
        1116..=1117 => {
            if y < 9 {
                noisy(layer, x, y, if layer == 1116 { [255, 48, 20] } else { [90, 20, 12] }, 0.1)
            } else {
                noisy(layer, x, y, [125, 90, 45], 0.2)
            }
        }
        1118..=1119 => {
            if x == 7 || x == 8 {
                noisy(layer, x, y, if layer == 1119 { [240, 35, 15] } else { [105, 20, 12] }, 0.1)
            } else {
                noisy(layer, x, y, [170, 165, 155], 0.1)
            }
        }
        1120..=1121 => {
            // Java's lamp: a dark rim, a 4x4 lattice of glass panes split by
            // thin frame lines, and a lighter core in each pane.
            let lit = layer == 1121;
            let rim = x == 0 || y == 0 || x == 15 || y == 15;
            let bar = x.is_multiple_of(5) || y.is_multiple_of(5);
            let core = (2..=3).contains(&(x % 5)) && (2..=3).contains(&(y % 5));
            let rgb = match (rim, bar, core, lit) {
                (true, ..) => [72, 44, 26],
                (_, true, _, false) => [96, 60, 34],
                (_, true, _, true) => [150, 98, 50],
                (_, _, true, false) => [150, 104, 60],
                (_, _, true, true) => [255, 246, 214],
                (.., false) => [122, 80, 45],
                (.., true) => [246, 196, 112],
            };
            noisy(layer, x, y, rgb, 0.08)
        }
        1125..=1126 => noisy(
            layer,
            x,
            y,
            if x.is_multiple_of(4) || y.is_multiple_of(4) {
                [92, 66, 38]
            } else if layer == 1125 {
                [218, 211, 176]
            } else {
                [98, 121, 153]
            },
            0.08,
        ),
        1130 => noisy(layer, x, y, if !(3..=12).contains(&x) { [96, 98, 95] } else { [167, 128, 73] }, 0.2),
        1131 | 1132 => noisy(
            layer,
            x,
            y,
            if x == 0 || x == 15 || y == 0 || y == 15 {
                [101, 100, 95]
            } else if layer == 1132 {
                [112, 167, 72]
            } else {
                [180, 147, 95]
            },
            0.16,
        ),
        1133 => noisy(
            layer,
            x,
            y,
            if ((4..7).contains(&x) || (10..13).contains(&x)) && (4..7).contains(&y) || (4..13).contains(&x) && y == 11
            {
                [36, 37, 36]
            } else {
                [154, 157, 151]
            },
            0.1,
        ),
        1134 | 1135 => noisy(
            layer,
            x,
            y,
            if (5..11).contains(&x) && (5..11).contains(&y) {
                if layer == 1135 { [240, 42, 18] } else { [92, 15, 13] }
            } else {
                [92, 94, 89]
            },
            0.1,
        ),
        1136 | 1137 => noisy(
            layer,
            x,
            y,
            if (4..12).contains(&x) && (4..12).contains(&y) && (layer == 1136 || x > 7 || y > 7) {
                [23, 24, 23]
            } else {
                [126, 128, 122]
            },
            0.18,
        ),
        1138 => noisy(layer, x, y, [64, 67, 68], 0.12),
        1129 => noisy(layer, x, y, if y < 13 { [128, 94, 46] } else { [92, 94, 90] }, 0.2),
        1128 => noisy(layer, x, y, if y == 3 || y == 12 { [162, 67, 29] } else { [203, 171, 58] }, 0.2),
        1127 => {
            let d = x.abs_diff(7).max(y.abs_diff(7));
            noisy(layer, x, y, if matches!(d,0..=1|4..=5) { [180, 32, 26] } else { [235, 224, 194] }, 0.06)
        }
        tex::STONE => {
            let streak = rnd(layer, x / 3, y, 5) < 0.12;
            shade(STONE, if streak { 0.82 } else { 0.9 + r * 0.18 })
        }
        tex::WATER => {
            let c = noisy(layer, x, y, [44, 90, 200], 0.08);
            [c[0], c[1], c[2], 170]
        }
        tex::SKIN => noisy(layer, x, y, [196, 141, 110], 0.05),
        l if (tex::FIRE_0..tex::FIRE_0 + tex::FIRE_FRAMES as u16).contains(&l) => {
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
        tex::CARVED_PUMPKIN | tex::JACK_O_LANTERN => {
            // The ribbed skin with triangular eyes and a jagged grin cut out of it.
            let (fx, fy) = (x as i32, y as i32);
            let eye = |cx: i32| (4..7).contains(&fy) && (fx - cx).abs() <= fy - 3 && (fx - cx).abs() <= 2;
            let mouth = (9..13).contains(&fy) && (3..13).contains(&fx) && ((fx + fy) % 4 != 0 || fy == 10 || fy == 11);
            if eye(4) || eye(11) || mouth {
                if layer == tex::JACK_O_LANTERN {
                    shade([255, 214, 90], 0.92 + r * 0.12)
                } else {
                    shade([64, 38, 14], 0.85 + r * 0.2)
                }
            } else {
                let rib = x.is_multiple_of(4);
                shade(if rib { [190, 110, 20] } else { [226, 140, 28] }, 0.92 + r * 0.12)
            }
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
        tex::MOSSY_COBBLESTONE => {
            let stone = pixel(tex::COBBLESTONE, x, y);
            let moss = rnd(layer, x / 2, y / 2, 1) * 0.7 + rnd(layer, x, y, 2) * 0.3;
            if moss > 0.47 && y > 2 { shade([65, 105, 45], 0.75 + r * 0.35) } else { stone }
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
        tex::CACTUS_SIDE => {
            let line = x % 4 == 1;
            let spike = rnd(layer, x, y, 8) < 0.05;
            if spike { [230, 230, 190, 255] } else { shade([76, 140, 48], if line { 0.75 } else { 0.95 + r * 0.1 }) }
        }
        tex::CACTUS_TOP => {
            let border = x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1;
            shade([88, 150, 58], if border { 0.75 } else { 0.95 + r * 0.1 })
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
        tex::CAKE_TOP => {
            if !(1..15).contains(&x) || !(1..15).contains(&y) {
                [0, 0, 0, 0]
            } else if (x + y).is_multiple_of(6) {
                shade([196, 48, 42], 1.0)
            } else {
                shade([248, 248, 244], 0.95 + r * 0.08)
            }
        }
        tex::CAKE_SIDE => {
            if y < 7 {
                shade([244, 244, 240], 1.0)
            } else {
                shade([156, 96, 52], 0.9 + r * 0.1)
            }
        }
        tex::CAKE_BOTTOM => shade([140, 84, 44], 0.9 + r * 0.1),
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
        tex::RAIL | tex::RAIL_EW | tex::RAIL_SE | tex::RAIL_SW | tex::RAIL_NW | tex::RAIL_NE => {
            rail_pixel(layer, x, y, r)
        }
        tex::COBWEB => {
            // Threads from the centre and rings around it, see-through.
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let ring = (((dx * dx + dy * dy).sqrt() + 0.5) as usize).is_multiple_of(4);
            let spoke = dx.abs() < 0.6 || dy.abs() < 0.6 || (dx.abs() - dy.abs()).abs() < 0.8;
            if (ring || spoke) && rnd(layer, x, y, 3) > 0.15 { [235, 235, 240, 220] } else { [0, 0, 0, 0] }
        }
        tex::FRAME_TOP | tex::FRAME_EYE_TOP => {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            let eye = layer == tex::FRAME_EYE_TOP && (4..12).contains(&x) && (4..12).contains(&y);
            if eye {
                end_eye(x, y, r)
            } else {
                // Teal stone with a pale ring inset round the socket.
                let d = dx.abs().max(dy.abs());
                let c = if (4.5..5.5).contains(&d) { [196, 214, 168] } else { [56, 104, 92] };
                shade(c, 0.85 + r * 0.25)
            }
        }
        tex::FRAME_SIDE | tex::FRAME_EYE_SIDE => {
            if y < 3 {
                // Above the 13/16 frame: only the eye shows here.
                if layer == tex::FRAME_EYE_SIDE && (4..12).contains(&x) { end_eye(x, y + 6, r) } else { [0, 0, 0, 0] }
            } else if y < 6 {
                shade([56, 104, 92], 0.85 + r * 0.25)
            } else {
                pixel(tex::END_STONE, x, y)
            }
        }
        tex::END_PORTAL => {
            // A starfield: specks of teal and green on near black.
            let star = rnd(layer, x, y, 7);
            if star > 0.94 {
                shade([90, 200, 170], 0.8 + r * 0.4)
            } else if star > 0.88 {
                shade([40, 90, 110], 0.9 + r * 0.3)
            } else {
                shade([8, 12, 18], 0.8 + r * 0.4)
            }
        }
        tex::ENCHANT_TOP => {
            // Red cloth on an obsidian rim, with a diamond at each corner.
            let edge = x.min(y).min(SIZE - 1 - x).min(SIZE - 1 - y);
            let corner = (x <= 2 || x >= SIZE - 3) && (y <= 2 || y >= SIZE - 3);
            if corner && (x == 1 || x == SIZE - 2) && (y == 1 || y == SIZE - 2) {
                shade([95, 230, 225], 0.9 + r * 0.2)
            } else if edge < 2 || corner {
                pixel(tex::OBSIDIAN, x, y)
            } else {
                let weave = if (x + y).is_multiple_of(2) { 0.95 } else { 0.85 };
                shade([170, 32, 36], weave * (0.9 + r * 0.15))
            }
        }
        tex::ENCHANT_SIDE => {
            // The cloth hangs over the top of the side (rows 4-6, a gold
            // hem at 7), obsidian below; rows 0-3 sit above the table.
            match y {
                0..=3 => [0, 0, 0, 0],
                4..=6 => shade([170, 32, 36], 0.85 + r * 0.15),
                7 => shade([230, 190, 60], 0.9 + r * 0.15),
                _ => pixel(tex::OBSIDIAN, x, y),
            }
        }
        tex::DRAGON_EGG => {
            // Near-black scales with violet flecks.
            let fleck = rnd(layer, x, y, 13);
            if fleck > 0.9 {
                shade([96, 40, 120], 0.85 + r * 0.3)
            } else if fleck > 0.75 {
                shade([40, 20, 52], 0.9 + r * 0.2)
            } else {
                shade([16, 10, 20], 0.8 + r * 0.4)
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
        tex::HEART_FULL
        | tex::HEART_HALF
        | tex::HEART_EMPTY
        | tex::HEART_ABSORBING_FULL
        | tex::HEART_ABSORBING_HALF
        | tex::HEART_FROZEN_FULL
        | tex::HEART_FROZEN_HALF => heart(layer, x, y),
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
        l if (tex::CRACK_0..tex::CRACK_0 + tex::CRACK_STAGES as u16).contains(&l) => {
            crack((l - tex::CRACK_0) as u8, x, y)
        }
        l if let Some((base, group)) = tex::untinted(l) => tint_foliage(pixel(base, x, y), group),
        _ => {
            // Missing texture: magenta checkerboard.
            if (x / 4 + y / 4).is_multiple_of(2) { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        }
    }
}

fn special_rail_pixel(layer: u16, x: usize, y: usize) -> Rgba {
    if layer >= 1151 {
        return match layer {
            1151 => {
                let edge = !(2..=13).contains(&x) || !(2..=13).contains(&y);
                let lattice = (x + y).is_multiple_of(4) || (x + 16 - y).is_multiple_of(4);
                let grain = 0.92 + rnd(layer, x, y / 2, 17) * 0.16;
                shade(
                    if edge {
                        [116, 78, 44]
                    } else if lattice {
                        [71, 45, 26]
                    } else {
                        [148, 103, 58]
                    },
                    grain,
                )
            }
            1152 => shade([176, 176, 180], if x < 4 { 0.7 } else { 1.0 }),
            1154 => shade([224, 218, 195], if x == 0 || y == 0 || x == 15 || y == 15 { 0.85 } else { 1.0 }),
            _ => shade([210, 210, 210], 1.0),
        };
    }
    let i = layer - 1139;
    let kind = i / 4;
    let powered = (i / 2) % 2 == 1;
    let east_west = i % 2 == 1;
    let (across, along) = if east_west { (y, x) } else { (x, y) };
    let rail = matches!(across, 4 | 5 | 10 | 11);
    let sleeper = along % 4 <= 1 && (3..=12).contains(&across);
    let circuit = (7..=8).contains(&across) && (kind != 1 || (3..=12).contains(&along));
    if rail {
        shade(
            if kind == 0 { [224, 177, 59] } else { [169, 169, 178] },
            if across == 4 || across == 10 { 1.0 } else { 0.75 },
        )
    } else if circuit {
        shade(if powered { [235, 57, 34] } else { [98, 22, 16] }, 1.0)
    } else if kind == 1 && (3..=12).contains(&along) && (6..=9).contains(&across) {
        shade([155, 155, 162], 0.8)
    } else if sleeper {
        shade([112, 78, 46], 0.9)
    } else {
        [0, 0, 0, 0]
    }
}

fn rail_pixel(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let iron = |lit: bool| shade([148, 148, 156], if lit { 1.18 } else { 0.78 } * (0.92 + r * 0.12));
    let wood = shade([110, 78, 46], 0.88 + r * 0.18);
    let ns = |x: usize, y: usize| {
        let bar = x == 4 || x == 5 || x == 10 || x == 11;
        let sleeper = y % 4 <= 1 && (3..=12).contains(&x);
        (bar, sleeper, x == 4 || x == 10)
    };
    let ew = |x: usize, y: usize| {
        let bar = y == 4 || y == 5 || y == 10 || y == 11;
        let sleeper = x % 4 <= 1 && (3..=12).contains(&y);
        (bar, sleeper, y == 4 || y == 10)
    };
    let (bar, sleeper, lit) = match layer {
        tex::RAIL => ns(x, y),
        tex::RAIL_EW => ew(x, y),
        tex::RAIL_SE => {
            let n = ns(x, y);
            let e = ew(x, y);
            ((n.0 && y >= 7) || (e.0 && x >= 7), (n.1 && y >= 7) || (e.1 && x >= 7), n.2 || e.2)
        }
        tex::RAIL_SW => {
            let n = ns(x, y);
            let e = ew(x, y);
            ((n.0 && y >= 7) || (e.0 && x <= 8), (n.1 && y >= 7) || (e.1 && x <= 8), n.2 || e.2)
        }
        tex::RAIL_NW => {
            let n = ns(x, y);
            let e = ew(x, y);
            ((n.0 && y <= 8) || (e.0 && x <= 8), (n.1 && y <= 8) || (e.1 && x <= 8), n.2 || e.2)
        }
        _ => {
            let n = ns(x, y);
            let e = ew(x, y);
            ((n.0 && y <= 8) || (e.0 && x >= 7), (n.1 && y <= 8) || (e.1 && x >= 7), n.2 || e.2)
        }
    };
    if bar {
        iron(lit)
    } else if sleeper {
        wood
    } else {
        [0, 0, 0, 0]
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

/// The eye of ender set in a frame: green with a dark slit pupil, `y`
/// from 4 at its top.
fn end_eye(x: usize, y: usize, r: f32) -> Rgba {
    let pupil = (7..=8).contains(&x) && (5..=10).contains(&y);
    if pupil { shade([18, 40, 30], 1.0) } else { shade([60, 150, 110], 0.85 + r * 0.3) }
}

/// Hunger icon: a drumstick (meat upper left, bone lower right); full,
/// half (left side) or an empty outline.
fn drumstick(layer: u16, x: usize, y: usize) -> Rgba {
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
fn heart(layer: u16, x: usize, y: usize) -> Rgba {
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
        tex::HEART_FULL | tex::HEART_ABSORBING_FULL | tex::HEART_FROZEN_FULL => true,
        tex::HEART_HALF | tex::HEART_ABSORBING_HALF | tex::HEART_FROZEN_HALF => x < 8,
        _ => false,
    };
    // Absorption's hearts are gold, drawn over the empty container.
    let golden = matches!(layer, tex::HEART_ABSORBING_FULL | tex::HEART_ABSORBING_HALF);
    if filled && matches!(layer, tex::HEART_FROZEN_FULL | tex::HEART_FROZEN_HALF) {
        return if x < 6 && (3..6).contains(&y) { [220, 252, 255, 255] } else { [93, 190, 233, 255] };
    }
    if !filled {
        [60, 30, 30, 200]
    } else if x < 6 && (3..6).contains(&y) {
        if golden { [255, 245, 170, 255] } else { [255, 170, 170, 255] } // highlight
    } else if golden {
        [232, 178, 24, 255]
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
    let clock = icons + crate::item::COMPASS_FRAMES;
    let recovery = clock + crate::item::CLOCK_FRAMES;
    let effects = recovery + crate::item::RECOVERY_FRAMES;
    match tex::item_index(layer) {
        Some(index) if index >= crate::item::open_bundle_icon_base() => {
            crate::item::open_bundle_sprite(index).map_or([0, 0, 0, 0], |s| super::item_sprites::pixel(s, x, y))
        }
        Some(index) if index >= effects => Effect::ALL
            .get((index - effects) as usize)
            .map_or([0, 0, 0, 0], |&e| super::item_sprites::effect_pixel(e, x, y)),
        Some(index) if index >= recovery => super::item_sprites::compass_face(
            index - recovery,
            crate::item::RECOVERY_FRAMES,
            x as i32,
            y as i32,
            [38, 52, 58],
            [78, 214, 196],
        )
        .unwrap_or([0, 0, 0, 0]),
        Some(index) if index >= clock => super::item_sprites::compass_face(
            index - clock,
            crate::item::CLOCK_FRAMES,
            x as i32,
            y as i32,
            [236, 214, 150],
            [250, 196, 48],
        )
        .unwrap_or([0, 0, 0, 0]),
        Some(index) if index >= icons => super::item_sprites::compass_face(
            index - icons,
            crate::item::COMPASS_FRAMES,
            x as i32,
            y as i32,
            [62, 86, 112],
            [176, 40, 36],
        )
        .unwrap_or([0, 0, 0, 0]),
        Some(index) => {
            crate::item::sprite_for_layer(index).map_or([0, 0, 0, 0], |s| super::item_sprites::pixel(s, x, y))
        }
        None => pixel(layer, x, y),
    }
}

/// RGBA8 data for every mip level of the block texture array; each level
/// contains all layers back to back, ready for `write_texture`.
pub fn generate_mips() -> Vec<Vec<u8>> {
    mips_of(tex::COUNT as usize, |l, x, y| pixel(l as u16, x, y))
}

/// Layers of the item icon array: every item's icon, the compass and clock
/// frames, the status effect icons, then the open bundles.
pub fn item_layers() -> u32 {
    crate::item::icon_count()
        + crate::item::animated_icons()
        + crate::simulation::effects::Effect::ALL.len() as u32
        + crate::item::OPEN_BUNDLE_ICONS
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

// Original procedural artwork matching each workstation's material and face.
#[cfg(test)]
mod tests {
    use super::*;

    /// v1.0 Track 10's layers, in review-sheet order: wooden doors and
    /// trapdoors, chests, the village and farm blocks, then the plants.
    fn final_art_layers() -> Vec<u16> {
        let mut out = vec![tex::DOOR_TOP, tex::DOOR_BOTTOM];
        out.extend(tex::DARK_OAK_DOOR_BOTTOM..=tex::CHERRY_DOOR_TOP);
        out.extend(tex::BUILDING_DOORS..tex::BUILDING_DOORS + 4);
        out.extend(tex::NETHER_DOORS..tex::NETHER_DOORS + 4);
        out.extend([760, 761, 1124]);
        out.extend(1816..=1822);
        out.extend([tex::BUILDING_TRAPDOORS, tex::BUILDING_TRAPDOORS + 1, tex::NETHER_TRAPDOORS]);
        out.extend([tex::NETHER_TRAPDOORS + 1, 762]);
        out.extend([tex::CHEST_TOP, tex::CHEST_SIDE, tex::CHEST_FRONT, crate::world::utilities::t::TRAPPED_FRONT]);
        out.extend([tex::VILLAGE, tex::VILLAGE + 1, tex::VILLAGE + 2, tex::VILLAGE + 3, tex::VILLAGE + 4]);
        out.extend([tex::VILLAGE + 42, tex::VILLAGE + 43, tex::VILLAGE + 44]);
        out.extend(tex::ANVIL..tex::ANVIL_TOP + 3);
        out.extend([tex::TNT_SIDE, tex::TNT_TOP, tex::TNT_BOTTOM]);
        // Flowers, tall flowers, saplings, grasses.
        out.extend([tex::DANDELION, tex::POPPY, tex::BLUE_ORCHID]);
        out.extend(1578..=1586);
        out.extend([706, 3442, 792, 3373, 3374, 2845]);
        out.extend(1587..=1594);
        out.extend([3454, 3455]);
        out.extend([tex::OAK_SAPLING, tex::SPRUCE_SAPLING, tex::BIRCH_SAPLING, tex::JUNGLE_SAPLING]);
        out.extend([tex::ACACIA_SAPLING, 1556, 1557, 1558, 1559, 1570, 759]);
        out.extend([tex::TALL_GRASS, tex::FERN]);
        out.extend(1595..=1598);
        out.extend([tex::DEAD_BUSH]);
        out.extend(2840..=2844);
        // Crops.
        out.extend(tex::WHEAT_0..tex::WHEAT_0 + 8);
        out.extend(tex::CARROT_0..tex::CARROT_0 + 8);
        out.extend(tex::POTATO_0..tex::POTATO_0 + 8);
        out.extend(3004..=3007);
        out.extend(2992..=3003);
        out.extend([3440, 3441]);
        out.extend(3444..=3453);
        out.extend(tex::NETHER_WART_0..tex::NETHER_WART_0 + 3);
        out.extend(1601..=1604);
        // Mushrooms, fungi and the other cross-model plants.
        out.extend([tex::BROWN_MUSHROOM, tex::RED_MUSHROOM, 1324, 1325]);
        out.extend([tex::SUGAR_CANE, 1326, 1327, 1328, 1329, 1330, 1331, 1332]);
        out.extend(1501..=1505);
        out.extend(1490..=1495);
        out.extend([1498, 1499, 1500, 1576, 1577, 791]);
        out
    }

    /// Writes v1.0 Track 10's layers as a review sheet (each tile 8x, and a
    /// 2 x 2 repeat at 4x, over a blue-grey that shows the see-through
    /// texels) to `$VC_FINAL_ART_SHEET`; run with `--ignored --nocapture`.
    /// The layer of each cell is printed in sheet order.
    #[test]
    #[ignore]
    fn final_art_sheet() {
        let Ok(path) = std::env::var("VC_FINAL_ART_SHEET") else { return };
        write_sheet(&path, &final_art_layers());
    }

    /// v1.0 Track 10b's layers, in review-sheet order: the flat ground
    /// plants, vines and glow lichen, the corals (blocks, plants and fans
    /// alive then dead, then the wall fans) and sea pickles, bamboo, the
    /// potted torchflower and the ender chest.
    fn final_art_b_layers() -> Vec<u16> {
        use crate::world::overworld_blocks::t as ob;
        use crate::world::utilities::t as ut;
        let mut out = vec![ob::LILY_PAD, ob::PINK_PETALS];
        out.extend(crate::world::biome_plants::TEX..crate::world::biome_plants::TEX + 8);
        out.extend([ob::VINE, crate::world::cave_blocks::t::GLOW_LICHEN]);
        out.extend(ob::CORAL..ob::CORAL + 30);
        out.extend(ut::WALL_FAN..ut::WALL_FAN + 40);
        out.extend(ob::SEA_PICKLES..ob::SEA_PICKLES + 4);
        out.extend([ob::BAMBOO, ob::BAMBOO_SMALL_LEAVES, ob::BAMBOO_LARGE_LEAVES]);
        out.push(crate::world::sniffer_plants::t::POTTED);
        out.extend([ut::ENDER_FRONT, ut::ENDER_SIDE, ut::ENDER_TOP]);
        out
    }

    /// Writes v1.0 Track 10b's layers as a review sheet, as
    /// [`final_art_sheet`] does, to `$VC_FINAL_ART_B_SHEET`.
    #[test]
    #[ignore]
    fn final_art_b_sheet() {
        let Ok(path) = std::env::var("VC_FINAL_ART_B_SHEET") else { return };
        write_sheet(&path, &final_art_b_layers());
    }

    /// Writes v1.0 gap Track 7's layers (the amethyst buds and cluster
    /// standing then hanging, the bamboo's two leaf sprites, the potted
    /// bamboo and its leaf) as a review sheet to `$VC_FINAL_ART_C_SHEET`.
    #[test]
    #[ignore]
    fn final_art_c_sheet() {
        let Ok(path) = std::env::var("VC_FINAL_ART_C_SHEET") else { return };
        use crate::world::overworld_blocks::t as ob;
        let mut layers: Vec<u16> = (ob::BUDS..ob::BUDS + 8).collect();
        layers.extend([ob::BAMBOO_SMALL_LEAVES, ob::BAMBOO_LARGE_LEAVES]);
        layers.push(crate::world::utilities::t::POTTED + crate::world::utilities::POTTED_BAMBOO as u16);
        layers.push(crate::world::utilities::POTTED_BAMBOO_LEAF);
        write_sheet(&path, &layers);
    }

    /// Writes one FNV-1a checksum per block layer (every layer below
    /// `tex::COUNT`, as `pixel` draws it) to `$VC_LAYER_DIGESTS`, one
    /// `layer checksum` line each: comparing two commits' files shows which
    /// layers a change touched; run with `--ignored`.
    #[test]
    #[ignore]
    fn layer_digests() {
        let Ok(path) = std::env::var("VC_LAYER_DIGESTS") else { return };
        let mut out = String::new();
        for layer in 0..tex::COUNT as u16 {
            let h = (0..SIZE * SIZE)
                .flat_map(|i| pixel(layer, i % SIZE, i / SIZE))
                .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
            out.push_str(&format!("{layer} {h:016x}\n"));
        }
        std::fs::write(path, out).unwrap();
    }

    /// Writes `layers` as a review sheet to `path` (see [`final_art_sheet`]).
    fn write_sheet(path: &str, layers: &[u16]) {
        let (cols, cell_w, cell_h) = (12, 16 * 8 + 4 + 32 * 4 + 12, 16 * 8 + 12);
        let rows = layers.len().div_ceil(cols);
        let (w, h) = (cols * cell_w, rows * cell_h);
        let mut img = vec![24u8; w * h * 4];
        for (i, &layer) in layers.iter().enumerate() {
            let (ox, oy) = ((i % cols) * cell_w + 6, (i / cols) * cell_h + 6);
            let mut put = |x: usize, y: usize, p: Rgba| {
                let k = ((oy + y) * w + ox + x) * 4;
                img[k..k + 4].copy_from_slice(&if p[3] == 0 { [92, 108, 132, 255] } else { [p[0], p[1], p[2], 255] });
            };
            for y in 0..16 * 8 {
                for x in 0..16 * 8 {
                    put(x, y, pixel(layer, x / 8, y / 8));
                }
            }
            for y in 0..32 * 4 {
                for x in 0..32 * 4 {
                    put(16 * 8 + 4 + x, y, pixel(layer, (x / 4) % 16, (y / 4) % 16));
                }
            }
            println!("sheet {i}: layer {layer}");
        }
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
    }

    /// Golden absorption hearts are a full tile and a left-half tile, distinct
    /// from the red hearts they sit beside, with the same outline shape.
    #[test]
    fn absorption_hearts_are_golden_full_and_half() {
        let tile = |layer| (0..SIZE).flat_map(move |y| (0..SIZE).map(move |x| pixel(layer, x, y))).collect::<Vec<_>>();
        let (full, half, red) =
            (tile(tex::HEART_ABSORBING_FULL), tile(tex::HEART_ABSORBING_HALF), tile(tex::HEART_FULL));
        assert_ne!(full, red);
        assert_ne!(full, half);
        let golden = |p: &Rgba| p[3] == 255 && p[0] > 200 && p[1] > 150 && p[2] < 80;
        assert!(full.iter().any(golden) && half.iter().any(golden));
        assert!(!red.iter().any(golden));
        let right_golden = |tile: &[Rgba]| (0..SIZE).any(|y| (8..SIZE).any(|x| golden(&tile[y * SIZE + x])));
        assert!(right_golden(&full) && !right_golden(&half), "the half heart fills only its left side");
        // Same silhouette as the red heart.
        let outline = |tile: &[Rgba]| tile.iter().map(|p| p[3] > 0).collect::<Vec<_>>();
        assert_eq!(outline(&full), outline(&red));
        assert!((tex::HEART_ABSORBING_HALF as u32) < tex::COUNT);
    }

    #[test]
    fn absorption_and_the_enchanted_golden_apple_have_icons() {
        use crate::simulation::effects::Effect;
        let drawn = |draw: &dyn Fn(usize, usize) -> Rgba| {
            (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| draw(x, y)[3] > 0).count()
        };
        assert!(drawn(&|x, y| super::super::item_sprites::effect_pixel(Effect::Absorption, x, y)) > 20);
        let apple = crate::item::Item::ENCHANTED_GOLDEN_APPLE.icon_layer().unwrap();
        let plain = crate::item::Item::GOLDEN_APPLE.icon_layer().unwrap();
        assert_ne!(apple, plain);
        let differs = (0..16).any(|y| (0..16).any(|x| texel(apple, x, y) != texel(plain, x, y)));
        assert!(differs, "the enchanted apple is drawn with violet sparkles");
        assert!(drawn(&|x, y| texel(apple, x, y)) > 40);
    }
}
