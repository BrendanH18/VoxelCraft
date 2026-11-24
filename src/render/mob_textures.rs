//! v0.18 Track 6: original material patterns for mob boxes, in texture
//! layers 1280–1289 (`entity::model::PATTERN_TEX`). Each is a tileable 16 x 16
//! grey modulation: the entity shader multiplies a box's colour by twice
//! the sampled (linear) value, so a texel of linear 0.5 leaves the colour
//! as authored. One texel covers one model pixel, and the mipmaps fade the
//! pattern to its neutral mean with distance.

use super::{Rgba, points, rnd, voronoi};
use crate::entity::model::Pattern;

/// A modulation factor (1 = the box colour) as an sRGB grey texel.
fn grey(m: f32) -> Rgba {
    let lin = (m * 0.5).clamp(0.0, 1.0);
    let s = if lin <= 0.003_130_8 { lin * 12.92 } else { 1.055 * lin.powf(1.0 / 2.4) - 0.055 };
    let v = (s * 255.0).round() as u8;
    [v, v, v, 255]
}

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let jitter = |amount: f32, v: i32| (rnd(layer, x, y, v) - 0.5) * 2.0 * amount;
    let m = match Pattern::from_layer(layer) {
        // Rounded tufts in offset rows: a light crown, a darker fold between.
        Some(Pattern::Wool) => {
            let row = y / 4;
            let (lx, ly) = ((x + (row % 2) * 2) % 4, y % 4);
            let d = (lx as f32 - 1.5).abs() + (ly as f32 - 1.2).abs();
            1.12 - d * 0.075 + jitter(0.04, 1)
        }
        // Short vertical strands, each a few texels long, staggered by column.
        Some(Pattern::Fur) => {
            let strand = (y + (x * 7) % 5) / 3;
            let tone = rnd(layer, x, strand, 2);
            0.9 + tone * 0.2 + if (x + strand).is_multiple_of(5) { -0.06 } else { 0.0 }
        }
        // Smooth skin with a few pores.
        Some(Pattern::Hide) => {
            let pore = rnd(layer, x, y, 3) > 0.93;
            1.0 + jitter(0.035, 4) - if pore { 0.07 } else { 0.0 }
        }
        // Overlapping feathers: rows of scallops, light at the tip, a dark lower edge.
        Some(Pattern::Feather) => {
            let row = y / 3;
            let lx = (x + (row % 2) * 2) % 4;
            let edge = y % 3 == 2 && lx != 0;
            let tip = y.is_multiple_of(3) && (1..=2).contains(&lx);
            1.0 + if edge {
                -0.12
            } else if tip {
                0.07
            } else {
                0.0
            } + jitter(0.025, 5)
        }
        // Irregular blotches in three tones.
        Some(Pattern::Mottle) => {
            let (d1, d2, idx) = voronoi(x, y, &points(layer, 9));
            let tone = [0.74, 1.0, 1.18, 0.88, 1.08][idx % 5];
            let border = if d2 - d1 < 0.6 { -0.05 } else { 0.0 };
            tone + border + jitter(0.05, 6)
        }
        // Dry bone: faint growth lines across, a hairline crack here and there.
        Some(Pattern::Bone) => {
            let line = rnd(layer, 0, y, 7) > 0.72;
            let crack = rnd(layer, x / 3, y, 8) > 0.9;
            1.03 + jitter(0.03, 9) - if line { 0.05 } else { 0.0 } - if crack { 0.12 } else { 0.0 }
        }
        // Dark ground with sparse, faintly bright specks.
        Some(Pattern::Speckle) => {
            let speck = rnd(layer, x, y, 10) > 0.94;
            1.0 + jitter(0.06, 11) + if speck { 0.55 } else { 0.0 }
        }
        // Coarse hair: short light bristles on a dark ground.
        Some(Pattern::Bristle) => {
            let bristle = (x + 2 * y).is_multiple_of(5) && rnd(layer, x, y / 2, 12) > 0.45;
            0.94 + jitter(0.07, 13) + if bristle { 0.4 } else { 0.0 }
        }
        // A plain weave with a darker seam every four rows.
        Some(Pattern::Cloth) => {
            let weave = if (x + y).is_multiple_of(2) { 0.045 } else { -0.045 };
            let seam = if y % 8 == 7 { -0.08 } else { 0.0 };
            1.0 + weave + seam + jitter(0.03, 14)
        }
        // Mottled flesh with darker rotten patches.
        Some(Pattern::Rot) => {
            let (d1, _, idx) = voronoi(x, y, &points(layer, 6));
            let patch = idx % 3 == 0 && d1 < 2.2;
            1.02 + jitter(0.06, 15) - if patch { 0.2 } else { 0.0 }
        }
        None => 1.0,
    };
    grey(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::textures::SIZE;

    fn linear(t: Rgba) -> f32 {
        let s = t[0] as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    }

    /// Each pattern averages close to neutral (the box keeps its colour from
    /// afar) yet has visible texture up close, and no two are alike.
    #[test]
    fn patterns_are_neutral_on_average_and_distinct() {
        let mut seen = Vec::new();
        for p in Pattern::ALL {
            let layer = p.layer();
            let texels: Vec<f32> = (0..SIZE * SIZE).map(|i| linear(pixel(layer, i % SIZE, i / SIZE)) * 2.0).collect();
            let mean = texels.iter().sum::<f32>() / texels.len() as f32;
            let spread = texels.iter().map(|v| (v - mean).abs()).sum::<f32>() / texels.len() as f32;
            assert!((0.9..=1.1).contains(&mean), "{p:?} mean {mean}");
            assert!((0.015..=0.2).contains(&spread), "{p:?} spread {spread}");
            assert!(!seen.contains(&texels), "{p:?} repeats another pattern");
            seen.push(texels);
        }
    }
}
