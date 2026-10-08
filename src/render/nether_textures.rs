//! Procedural textures for the Nether biome blocks (layers 1300..=1399),
//! original artwork in the palette of Java's crimson, warped, soul and bone
//! textures.

use super::{Rgba, SIZE, noisy, pixel as base_pixel, rnd, shade, wooden_door};
use crate::world::block::tex;

const CRIMSON: [u8; 3] = [130, 31, 31];
const WARPED: [u8; 3] = [43, 114, 101];
const CRIMSON_PLANKS: [u8; 3] = [101, 48, 70];
const WARPED_PLANKS: [u8; 3] = [43, 104, 99];
const CLEAR: Rgba = [0, 0, 0, 0];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        l if (tex::SOUL_FIRE_0..tex::SOUL_FIRE_0 + tex::FIRE_FRAMES as u16).contains(&l) => {
            soul_fire((l - tex::SOUL_FIRE_0) as usize, x, y, r)
        }
        tex::CRIMSON_NYLIUM_TOP | tex::WARPED_NYLIUM_TOP => nylium(layer, x, y, r),
        tex::CRIMSON_NYLIUM_SIDE | tex::WARPED_NYLIUM_SIDE => {
            // Netherrack with the moss hanging over the top edge.
            let fringe = 2 + (rnd(layer, x, 0, 7) * 3.0) as usize + (rnd(layer, x / 3, 1, 7) * 2.0) as usize;
            if y < fringe { nylium(layer - 1, x, y, r) } else { base_pixel(tex::NETHERRACK, x, y) }
        }
        l if (tex::NETHER_STEM_SIDE..tex::NETHER_STEM_SIDE + 4).contains(&l) => stem_side(l, x, y, r),
        l if (tex::NETHER_STEM_TOP..tex::NETHER_STEM_TOP + 4).contains(&l) => stem_top(l, x, y, r),
        tex::CRIMSON_PLANKS | tex::WARPED_PLANKS => {
            let colour = if layer == tex::CRIMSON_PLANKS { CRIMSON_PLANKS } else { WARPED_PLANKS };
            let board = y / 4;
            let seam = y % 4 == 3 || x == (board * 7 + 3) % SIZE;
            shade(colour, if seam { 0.66 } else { 0.9 + rnd(layer, x, board, 2) * 0.14 + r * 0.04 })
        }
        tex::NETHER_WART_BLOCK | tex::WARPED_WART_BLOCK => {
            let colour = if layer == tex::NETHER_WART_BLOCK { [114, 6, 6] } else { [22, 119, 121] };
            let lump = rnd(layer, x / 2, y / 2, 3);
            shade(
                colour,
                if lump < 0.2 {
                    0.7
                } else if lump > 0.85 {
                    1.25
                } else {
                    0.92 + r * 0.14
                },
            )
        }
        tex::SHROOMLIGHT => {
            // Warm glowing cells with darker seams.
            let seam = (x + (y / 4) * 2).is_multiple_of(5) || (y + x / 5).is_multiple_of(4);
            let hot = rnd(layer, x / 2, y / 2, 4) > 0.7;
            if seam {
                shade([206, 104, 44], 0.9 + r * 0.1)
            } else if hot {
                shade([255, 214, 140], 0.95 + r * 0.05)
            } else {
                shade([240, 150, 72], 0.92 + r * 0.1)
            }
        }
        tex::CRIMSON_FUNGUS => fungus(x, y, r, [168, 30, 40], [244, 196, 80]),
        tex::WARPED_FUNGUS => fungus(x, y, r, [20, 160, 140], [250, 130, 40]),
        tex::CRIMSON_ROOTS => roots(layer, x, y, r, [126, 8, 41]),
        tex::WARPED_ROOTS => roots(layer, x, y, r, [20, 150, 130]),
        tex::NETHER_SPROUTS => {
            // Short teal blades in two tufts.
            let tuft = |cx: usize| x.abs_diff(cx) <= 3 && y >= 9 + x.abs_diff(cx) * 2 && (x + y).is_multiple_of(2);
            if tuft(4) || tuft(11) { shade([20, 150, 130], 0.8 + r * 0.3) } else { CLEAR }
        }
        l if (tex::NETHER_VINES..tex::NETHER_VINES + 4).contains(&l) => vine(l - tex::NETHER_VINES, x, y, r),
        tex::SOUL_SOIL => {
            let pit = rnd(layer, x / 2, y / 2, 9) < 0.18;
            shade([75, 57, 46], if pit { 0.62 } else { 0.86 + r * 0.26 })
        }
        tex::SOUL_TORCH => match (x, y) {
            (7..=8, 4) => [200, 255, 255, 255],
            (7..=8, 5) => [93, 232, 236, 255],
            (7..=8, 6) => [40, 160, 200, 255],
            (7..=8, 7..=15) => shade([138, 106, 62], if x == 7 { 1.0 } else { 0.78 }),
            _ => CLEAR,
        },
        tex::BONE_BLOCK_SIDE => {
            let groove = x % 5 == 2;
            shade([229, 225, 207], if groove { 0.84 } else { 0.94 + r * 0.08 })
        }
        tex::BONE_BLOCK_TOP => {
            let ring = x.abs_diff(7).max(y.abs_diff(7));
            let c = if ring <= 2 { [196, 186, 150] } else { [229, 225, 207] };
            shade(c, if ring == 3 || ring == 7 { 0.82 } else { 0.94 + r * 0.08 })
        }
        l if (tex::NETHER_DOORS..tex::NETHER_DOORS + 4).contains(&l) => {
            let warped = l >= tex::NETHER_DOORS + 2;
            let bottom = if warped { tex::NETHER_DOORS + 2 } else { tex::NETHER_DOORS };
            let colour = if warped { WARPED_PLANKS } else { CRIMSON_PLANKS };
            wooden_door(layer, x, y, r, colour, bottom + 1, bottom)
        }
        l if (tex::NETHER_TRAPDOORS..=tex::NETHER_TRAPDOORS + 1).contains(&l) => {
            let colour = if layer == tex::NETHER_TRAPDOORS { CRIMSON_PLANKS } else { WARPED_PLANKS };
            let hole = matches!(x, 3..=5 | 10..=12) && matches!(y, 3..=5 | 10..=12);
            if hole { CLEAR } else { noisy(layer, x, y, colour, 0.12) }
        }
        _ => {
            // Unused reserved layers: the usual missing-texture checkerboard.
            if (x / 4 + y / 4).is_multiple_of(2) { [255, 0, 255, 255] } else { [0, 0, 0, 255] }
        }
    }
}

/// Fire's flame tongues in cyan and blue.
fn soul_fire(frame: usize, x: usize, y: usize, r: f32) -> Rgba {
    let sway = (rnd(tex::SOUL_FIRE_0, y / 3, frame, 2) * 3.0) as usize;
    let column = (x + sway) % SIZE;
    let tip = (rnd(tex::SOUL_FIRE_0, column / 2, frame, 3) * 9.0) as usize;
    if y < tip || (y < 10 && rnd(tex::SOUL_FIRE_0 + frame as u16, x, y, 4) < 0.16) {
        CLEAR
    } else {
        let heat = ((y - tip) as f32 / (SIZE - tip) as f32 + r * 0.2).min(1.0);
        [(40.0 + 120.0 * heat * heat) as u8, (170.0 + 85.0 * heat) as u8, (215.0 + 40.0 * heat) as u8, 255]
    }
}

fn nylium(top: u16, x: usize, y: usize, r: f32) -> Rgba {
    let (base, dark, bright) = if top == tex::CRIMSON_NYLIUM_TOP {
        (CRIMSON, [92, 16, 16], [176, 52, 42])
    } else {
        (WARPED, [22, 80, 71], [78, 160, 138])
    };
    let speck = rnd(top, x, y, 5);
    let c = if speck < 0.18 {
        dark
    } else if speck > 0.9 {
        bright
    } else {
        base
    };
    shade(c, 0.9 + r * 0.18)
}

/// Bark (or stripped wood) with vertical veins.
fn stem_side(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let kind = layer - tex::NETHER_STEM_SIDE;
    let (bark, vein) = match kind {
        0 => ([92, 25, 29], [148, 54, 74]),
        1 => ([58, 58, 77], [22, 150, 140]),
        2 => ([137, 57, 90], [168, 80, 112]),
        _ => ([57, 150, 147], [84, 176, 168]),
    };
    let vein_x = (x + (rnd(layer, 0, y / 5, 3) * 2.0) as usize).is_multiple_of(5);
    let glint = kind < 2 && rnd(layer, x, y, 6) > 0.95;
    if glint {
        shade(if kind == 0 { [232, 132, 52] } else { [120, 240, 220] }, 0.9 + r * 0.1)
    } else if vein_x {
        shade(vein, 0.88 + r * 0.16)
    } else {
        shade(bark, 0.86 + r * 0.2)
    }
}

/// Cut end: a bark rim around a ringed core.
fn stem_top(layer: u16, x: usize, y: usize, r: f32) -> Rgba {
    let kind = layer - tex::NETHER_STEM_TOP;
    let ring = x.abs_diff(7).max(y.abs_diff(7)).min(x.abs_diff(8).max(y.abs_diff(8)));
    if ring >= 7 && kind < 2 {
        return stem_side(tex::NETHER_STEM_SIDE + kind, x, y, r);
    }
    let core = match kind {
        0 | 2 => [123, 57, 82],
        _ => [56, 120, 117],
    };
    shade(core, if ring.is_multiple_of(2) { 0.8 } else { 0.95 + r * 0.08 })
}

/// A capped mushroom on a short stem.
fn fungus(x: usize, y: usize, r: f32, cap: [u8; 3], spot: [u8; 3]) -> Rgba {
    let (dx, dy) = (x as f32 - 7.5, y as f32 - 6.0);
    let in_cap = y <= 8 && dx * dx / 36.0 + dy * dy / 9.0 < 1.0;
    if in_cap {
        let spotted = (x * 3 + y * 5).is_multiple_of(7);
        return if spotted { shade(spot, 0.9 + r * 0.1) } else { shade(cap, 0.85 + r * 0.2) };
    }
    if (7..=8).contains(&x) && y > 8 {
        return shade([214, 170, 150], if x == 7 { 0.95 } else { 0.78 });
    }
    CLEAR
}

/// Thin roots curling up from the ground.
fn roots(layer: u16, x: usize, y: usize, r: f32, colour: [u8; 3]) -> Rgba {
    let strand = |i: usize| {
        let base = 2 + i * 4;
        let top = 3 + (rnd(layer, i, 0, 8) * 6.0) as usize;
        let curl = ((y as f32 / 3.0 + i as f32).sin() * 1.4).round() as i32;
        y >= top && (base as i32 + curl) as usize == x
    };
    if (0..4).any(strand) { shade(colour, 0.75 + r * 0.35) } else { CLEAR }
}

/// Weeping (red, hanging) and twisting (teal, climbing) vines: head (with a
/// bulb at its tip) or plant (strands the full height).
fn vine(which: u16, x: usize, y: usize, r: f32) -> Rgba {
    let weeping = which < 2;
    let head = which.is_multiple_of(2);
    let colour = if weeping { [150, 20, 24] } else { [20, 140, 125] };
    // Distance from where the vine grows out of its support.
    let along = if weeping { y } else { SIZE - 1 - y };
    let strand = |cx: usize, wiggle: usize| {
        let sway = ((along / 3 + wiggle) % 3) as i32 - 1;
        (cx as i32 + sway) as usize == x
    };
    let length = if head { 11 } else { SIZE };
    if along < length && (strand(5, 0) || strand(10, 1) || (along % 4 == 1 && (6..=9).contains(&x))) {
        return shade(colour, 0.8 + r * 0.3);
    }
    let tip = head && (11..=14).contains(&along) && x.abs_diff(7) + along.abs_diff(12) <= 2;
    if tip { shade(if weeping { [190, 40, 40] } else { [40, 180, 160] }, 0.9 + r * 0.15) } else { CLEAR }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_nether_layer_is_drawn_and_cross_sprites_have_holes() {
        for layer in tex::SOUL_FIRE_0..=tex::NETHER_BIOME_LAST {
            let px: Vec<Rgba> = (0..SIZE * SIZE).map(|i| pixel(layer, i % SIZE, i / SIZE)).collect();
            assert!(!px.contains(&[255, 0, 255, 255]), "layer {layer} is missing");
            assert!(px.iter().any(|p| p[3] == 255), "layer {layer} is empty");
        }
        for layer in [tex::CRIMSON_FUNGUS, tex::WARPED_ROOTS, tex::NETHER_SPROUTS, tex::NETHER_VINES, tex::SOUL_TORCH] {
            assert!((0..SIZE * SIZE).any(|i| pixel(layer, i % SIZE, i / SIZE)[3] == 0), "{layer} is a sprite");
        }
        // Soul fire is blue where ordinary fire is orange.
        let p = pixel(tex::SOUL_FIRE_0, 8, 15);
        assert!(p[2] > p[0] && p[3] == 255, "{p:?}");
    }
}
