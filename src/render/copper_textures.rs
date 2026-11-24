//! Procedural textures for the v0.12 copper blocks (layers from
//! `world::copper::t::BASE`): original artwork, in four oxidation stages
//! from bright copper through brown and green patina to teal.

use super::{Rgba, noisy, rnd, shade};
use crate::world::copper::{Stage, t};

const CLEAR: Rgba = [0, 0, 0, 0];
/// Bare metal per stage, then its patina.
const METAL: [[u8; 3]; 4] = [[196, 112, 76], [168, 126, 104], [116, 150, 114], [84, 160, 132]];
const PATINA: [[u8; 3]; 4] = [[176, 98, 66], [112, 152, 124], [88, 166, 132], [70, 140, 116]];
/// How much of the surface the patina covers.
const COVER: [f32; 4] = [0.0, 0.3, 0.6, 0.9];
const WOOD: [u8; 3] = [138, 106, 62];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let Some((stage, kind)) = t::of(layer) else { return torch(x, y) };
    let r = rnd(layer, x, y, 0);
    let rim = x == 0 || y == 0 || x == 15 || y == 15;
    match kind {
        t::DOOR_TOP | t::DOOR_BOTTOM => door(stage, layer, x, y, kind == t::DOOR_TOP),
        t::TRAPDOOR => {
            let frame = x <= 1 || y <= 1 || x >= 14 || y >= 14;
            let hole = !frame && (3..=12).contains(&x) && (3..=12).contains(&y) && (x % 3 == 1) && (y % 3 == 1);
            if hole { CLEAR } else { metal(stage, layer, x, y, if frame { 0.82 } else { 0.98 + r * 0.06 }) }
        }
        t::BARS => {
            let bar = x % 4 == 1 || x % 4 == 2;
            let rail = (1..=2).contains(&y) || (13..=14).contains(&y);
            if bar || rail {
                let lit = if x % 4 == 1 || y == 1 || y == 13 { 1.12 } else { 0.8 };
                metal(stage, layer, x, y, lit * (0.94 + r * 0.1))
            } else {
                CLEAR
            }
        }
        t::CHAIN => {
            // Links down the middle columns (the chain's boxes span x 6..9).
            let gap = matches!(y % 8, 3 | 4) && (7..=8).contains(&x);
            if gap { CLEAR } else { metal(stage, layer, x, y, if y % 8 == 2 || y % 8 == 5 { 0.75 } else { 1.0 }) }
        }
        t::LANTERN => {
            // A copper cage around the green flame of copper fire.
            let caged = x % 3 == 2 || y.is_multiple_of(7) || y % 7 == 6 || matches!(x, 5 | 10);
            if caged {
                metal(stage, layer, x, y, 0.85 + rnd(layer, x, y, 3) * 0.2)
            } else {
                shade([142, 232, 128], 0.85 + rnd(layer, x, y, 5) * 0.3)
            }
        }
        t::CHEST_TOP => metal(
            stage,
            layer,
            x,
            y,
            if rim {
                0.7
            } else if x == 1 || y == 1 {
                1.1
            } else {
                0.95 + r * 0.06
            },
        ),
        t::CHEST_SIDE | t::CHEST_FRONT => {
            // The lid's seam five pixels down, and a latch on the front.
            let seam = y == 5;
            let latch = kind == t::CHEST_FRONT && (7..=8).contains(&x) && (3..=7).contains(&y);
            if latch {
                shade([62, 58, 56], if y == 3 { 1.3 } else { 1.0 })
            } else {
                metal(stage, layer, x, y, if rim || seam { 0.7 } else { 0.95 + r * 0.06 })
            }
        }
        t::ROD => {
            // Drawn on the rod's 2-pixel columns and its tip: lit on one side.
            let f = if x % 8 == 7 || y < 4 && x % 4 == 1 { 1.18 } else { 0.9 + r * 0.08 };
            metal(stage, layer, x, y, f)
        }
        _ => CLEAR,
    }
}

/// Copper of `stage` at brightness `f`, its patina in blotches.
fn metal(stage: Stage, layer: u16, x: usize, y: usize, f: f32) -> Rgba {
    let s = stage as usize;
    let blotch = rnd(0x0C0B, x / 2, y / 2, s as i32) * 0.6 + rnd(0x0C0B, x, y, 7 + s as i32) * 0.4;
    let c = if blotch < COVER[s] { PATINA[s] } else { METAL[s] };
    let mut p = noisy(layer, x, y, c, 0.04);
    for v in &mut p[..3] {
        *v = (*v as f32 * f).clamp(0.0, 255.0) as u8;
    }
    p
}

/// A riveted metal door: frame, a cross rail, and a window in the top half.
fn door(stage: Stage, layer: u16, x: usize, y: usize, top: bool) -> Rgba {
    let frame = x <= 1 || x >= 14 || (top && y <= 1) || (!top && y >= 14);
    let window = top && (3..=7).contains(&y) && matches!(x, 3..=6 | 9..=12);
    if window {
        return CLEAR;
    }
    let rail = (!top && (y == 1 || y == 2)) || (top && (y == 9 || y == 10));
    let rivet = (x == 3 || x == 12) && (y == 3 || y == 12);
    let f = if rivet {
        1.25
    } else if frame || rail {
        0.8
    } else {
        0.95 + rnd(layer, x / 4, y, 6) * 0.08
    };
    metal(stage, layer, x, y, f)
}

/// The copper torch: a stick under a copper band and a green flame.
fn torch(x: usize, y: usize) -> Rgba {
    match (x, y) {
        (7..=8, 4) => [214, 255, 196, 255],
        (7..=8, 5) => [128, 230, 120, 255],
        (7..=8, 6) => shade(METAL[0], 1.05),
        (7..=8, 7..=15) => shade(WOOD, if x == 7 { 1.0 } else { 0.78 }),
        _ => CLEAR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Through the shared `pixel`: `material_textures` draws the block, cut,
    // chiseled, grate and bulb faces (v0.18 Track 3).
    use super::super::pixel;

    fn opaque(layer: u16) -> usize {
        (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| pixel(layer, x, y)[3] == 255).count()
    }

    #[test]
    fn every_copper_layer_is_drawn_and_the_stages_differ() {
        for stage in Stage::ALL {
            for kind in 0..t::PER_STAGE {
                let layer = t::layer(stage, kind);
                assert!(opaque(layer) > 0, "{stage:?} {kind}");
                let see_through = matches!(kind, t::GRATE | t::DOOR_TOP | t::TRAPDOOR | t::BARS | t::CHAIN);
                assert_eq!(opaque(layer) < 256, see_through, "{stage:?} {kind}");
                // The same kind looks different at each stage.
                if let Some(next) = stage.next() {
                    let all = |l: u16| (0..256).map(|i| pixel(l, i % 16, i / 16)).collect::<Vec<_>>();
                    assert_ne!(all(layer), all(t::layer(next, kind)), "{stage:?} {kind}");
                }
            }
        }
        assert!(opaque(t::TORCH) > 0 && opaque(t::TORCH) < 40);
        // Lit and unlit bulbs differ in the middle; powered ones on the frame.
        let bulb = |state| pixel(t::layer(Stage::Exposed, t::BULB + state), 8, 8);
        assert_ne!(bulb(0), bulb(1));
        assert_ne!(pixel(t::layer(Stage::Exposed, t::BULB), 7, 1), pixel(t::layer(Stage::Exposed, t::BULB + 2), 7, 1));
    }
}
