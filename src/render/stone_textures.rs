//! Original procedural textures for the v0.12 stone and minerals blocks
//! (`world::stone_blocks::t`), in the noisy style of [`super::pixel`].

use super::{Rgba, rnd, shade};
use crate::world::stone_blocks::t;

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    match layer {
        t::TINTED_GLASS => {
            let rim = x == 0 || y == 0 || x == 15 || y == 15;
            let streak = (x + y == 5 || x + y == 6) && (1..6).contains(&x);
            let mut p = shade(
                [46, 38, 56],
                if rim {
                    1.15
                } else if streak {
                    1.6
                } else {
                    0.9 + r * 0.1
                },
            );
            p[3] = if rim { 255 } else { 205 };
            p
        }
        t::SLIME_BLOCK => {
            // A clear outer jelly around a denser core (Java's model has an
            // inner cube from 3 to 13).
            let rim = x == 0 || y == 0 || x == 15 || y == 15;
            let core = (3..13).contains(&x) && (3..13).contains(&y);
            let core_edge = core && (x == 3 || y == 3 || x == 12 || y == 12);
            let mut p = shade(
                [112, 190, 90],
                if rim {
                    1.1
                } else if core_edge {
                    0.7
                } else if core {
                    0.88 + r * 0.08
                } else {
                    1.0 + r * 0.06
                },
            );
            p[3] = if rim || core { 230 } else { 160 };
            p
        }
        _ => [255, 0, 255, 255],
    }
}
