//! Procedural textures for the Nether biome blocks (layers 1300..=1399),
//! original artwork in the palette of Java's crimson, warped, soul and bone
//! textures. The fungi, roots, sprouts, vines, doors and trapdoors are drawn
//! by the material generator (v1.0 Track 10).

use super::{Rgba, SIZE, rnd, shade};
use crate::world::block::tex;

const CLEAR: Rgba = [0, 0, 0, 0];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        l if (tex::SOUL_FIRE_0..tex::SOUL_FIRE_0 + tex::FIRE_FRAMES as u16).contains(&l) => {
            soul_fire((l - tex::SOUL_FIRE_0) as usize, x, y, r)
        }
        tex::SOUL_TORCH => match (x, y) {
            (7..=8, 4) => [200, 255, 255, 255],
            (7..=8, 5) => [93, 232, 236, 255],
            (7..=8, 6) => [40, 160, 200, 255],
            (7..=8, 7..=15) => shade([138, 106, 62], if x == 7 { 1.0 } else { 0.78 }),
            _ => CLEAR,
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_nether_layer_is_drawn_and_cross_sprites_have_holes() {
        // Through the shared `pixel`: `material_textures` draws the nylium,
        // wart blocks, shroomlight and soul soil (v0.18 Track 3).
        use super::super::pixel;
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
