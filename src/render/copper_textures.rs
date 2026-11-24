//! The copper torch (`world::copper::t::TORCH`). The copper blocks' faces
//! in all four oxidation stages are drawn by `material_textures` (v0.18
//! Track 3); this module's test checks them through the shared `pixel`.

use super::{Rgba, shade};
use crate::world::copper::t;

const CLEAR: Rgba = [0, 0, 0, 0];
/// Bare copper, for the torch's band.
const METAL: [u8; 3] = [196, 112, 76];
const WOOD: [u8; 3] = [138, 106, 62];

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    // `material_textures` draws every stage's faces (v0.18 Track 3); the
    // copper torch is left here.
    if t::of(layer).is_some() { CLEAR } else { torch(x, y) }
}

/// The copper torch: a stick under a copper band and a green flame.
fn torch(x: usize, y: usize) -> Rgba {
    match (x, y) {
        (7..=8, 4) => [214, 255, 196, 255],
        (7..=8, 5) => [128, 230, 120, 255],
        (7..=8, 6) => shade(METAL, 1.05),
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
    use crate::world::copper::Stage;

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
