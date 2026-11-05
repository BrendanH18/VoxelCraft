//! Original procedural statue metal, face and cap. UVs use block coordinates,
//! so the eyes occur on the head, above the torso. No reference art is imported.
use super::{Rgba, noisy, shade};
use crate::world::copper_golem_statue::{TEXTURE_END, TEXTURE_FIRST};
pub fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    debug_assert!((TEXTURE_FIRST..TEXTURE_END).contains(&layer));
    let local = layer - TEXTURE_FIRST;
    let stage = (local / 3) as usize;
    let rgb = [[192, 112, 70], [158, 125, 85], [83, 148, 117], [55, 161, 143]][stage];
    let rivet = matches!(x, 4 | 11) && matches!(y, 3 | 8 | 12);
    let seam = y == 8 || x == 4 || x == 11;
    // Both sitting and upright heads intersect this eye line.
    if local % 3 == 1 && matches!(x, 5 | 10) && y == 6 {
        return [36, 46, 42, 255];
    }
    if rivet {
        return shade(rgb, 1.22);
    }
    if seam {
        return shade(rgb, 0.72);
    }
    if local % 3 == 2 && (6..=9).contains(&x) && (6..=9).contains(&y) {
        return shade(rgb, 0.65);
    }
    noisy(layer, x, y, rgb, 0.065)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_layers_are_opaque_distinct_and_fit_the_reservation() {
        for layer in TEXTURE_FIRST..TEXTURE_END {
            assert!((0..16).all(|y| (0..16).all(|x| pixel(layer, x, y)[3] == 255)));
        }
        for stage in 0..3 {
            assert_ne!(pixel(TEXTURE_FIRST + stage * 3, 7, 7), pixel(TEXTURE_FIRST + (stage + 1) * 3, 7, 7));
        }
        assert_ne!(pixel(TEXTURE_FIRST, 5, 6), pixel(TEXTURE_FIRST + 1, 5, 6));
    }
}
