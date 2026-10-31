//! Procedural textures for the v0.13 underground blocks (layers from
//! `cave_blocks::TEX`): original artwork for glow lichen and the sculk
//! family in the palette of Java's deep dark.

use super::{Rgba, SIZE, noisy, rnd, shade};
use crate::world::cave_blocks::t;

const CLEAR: Rgba = [0, 0, 0, 0];
/// Sculk's near-black teal and its glowing cyan specks.
const SCULK: [u8; 3] = [13, 30, 36];
const SCULK_DEEP: [u8; 3] = [7, 18, 24];
const GLOW: [u8; 3] = [40, 200, 210];
const BONE: [u8; 3] = [214, 208, 186];
const LICHEN: [u8; 3] = [112, 132, 112];
const LICHEN_GLOW: [u8; 3] = [236, 214, 148];

/// Sculk flesh: dark mottling with sparse glowing specks.
fn sculk(layer: u16, x: usize, y: usize) -> Rgba {
    let mottle = rnd(layer, x / 2, y / 2, 3);
    if rnd(layer, x, y, 4) < 0.05 {
        return shade(GLOW, 0.75 + rnd(layer, x, y, 5) * 0.35);
    }
    noisy(layer, x, y, if mottle < 0.45 { SCULK_DEEP } else { SCULK }, 0.18)
}

/// A pale bone frame around sculk, `w` pixels wide.
fn bone_rim(layer: u16, x: usize, y: usize, w: usize) -> Option<Rgba> {
    let edge = x < w || y < w || x >= SIZE - w || y >= SIZE - w;
    edge.then(|| shade(BONE, 0.78 + rnd(layer, x, y, 6) * 0.22))
}

/// Upper half of a sensor's side: four curling tendrils on clear texels.
fn tendrils(layer: u16, x: usize, y: usize, active: bool) -> Rgba {
    let column = matches!(x, 2 | 3 | 6 | 9 | 12 | 13);
    let curl = y < 3 && matches!(x, 1 | 4 | 5 | 8 | 10 | 11 | 14);
    if !(column || curl) {
        return CLEAR;
    }
    let tip = y < 2 || curl;
    let c = if tip && active {
        [120, 250, 240]
    } else if tip {
        GLOW
    } else {
        [26, 74, 84]
    };
    shade(c, 0.8 + rnd(layer, x, y, 7) * 0.3)
}

pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let r = rnd(layer, x, y, 0);
    let (fx, fy) = (x as f32 - 7.5, y as f32 - 7.5);
    match layer {
        t::GLOW_LICHEN => {
            // Irregular lichen crust with glowing knots; gaps show the wall.
            let crust = rnd(layer, x / 2, y / 2, 3) < 0.55 || rnd(layer, x, y, 4) < 0.3;
            if !crust {
                return CLEAR;
            }
            if rnd(layer, x, y, 5) < 0.14 { shade(LICHEN_GLOW, 0.9 + r * 0.15) } else { shade(LICHEN, 0.75 + r * 0.35) }
        }
        t::SCULK_VEIN => {
            // Branching dark veins with a few glowing nodes.
            let branch = (x + y / 2).is_multiple_of(5)
                || (x + 15 - y).is_multiple_of(6) && rnd(layer, x, y / 3, 3) < 0.7
                || rnd(layer, x / 3, y / 3, 4) < 0.18;
            if !branch {
                return CLEAR;
            }
            if rnd(layer, x, y, 5) < 0.08 { shade(GLOW, 0.85 + r * 0.2) } else { shade(SCULK, 0.85 + r * 0.4) }
        }
        t::SCULK => sculk(layer, x, y),
        t::CATALYST_SIDE | t::CATALYST_SIDE_BLOOM => {
            // Bone ribs over the lower half, sculk above; bloom brightens it.
            let rib = y >= 8 && (x % 4 == 1 || y == 8);
            if rib {
                shade(BONE, 0.75 + r * 0.25)
            } else if layer == t::CATALYST_SIDE_BLOOM && rnd(layer, x, y, 4) < 0.18 {
                shade(GLOW, 0.95)
            } else {
                sculk(layer, x, y)
            }
        }
        t::CATALYST_TOP | t::CATALYST_TOP_BLOOM => {
            // A bone ring around a glowing heart.
            let d = (fx * fx + fy * fy).sqrt();
            if let Some(rim) = bone_rim(layer, x, y, 2) {
                rim
            } else if d < if layer == t::CATALYST_TOP_BLOOM { 4.5 } else { 3.0 } {
                shade(GLOW, 0.8 + r * 0.3)
            } else {
                sculk(layer, x, y)
            }
        }
        t::CATALYST_BOTTOM | t::SENSOR_BOTTOM | t::SHRIEKER_BOTTOM => sculk(layer, x, y),
        t::SENSOR_SIDE | t::SENSOR_SIDE_ACTIVE => {
            if y < 8 {
                tendrils(layer, x, y, layer == t::SENSOR_SIDE_ACTIVE)
            } else if y == 8 {
                shade(BONE, 0.7 + r * 0.2)
            } else {
                sculk(layer, x, y)
            }
        }
        t::SENSOR_TOP | t::SENSOR_TOP_ACTIVE => {
            let d = (fx * fx + fy * fy).sqrt();
            let ring = (5.0..6.5).contains(&d);
            if ring {
                let c = if layer == t::SENSOR_TOP_ACTIVE { [120, 250, 240] } else { GLOW };
                shade(c, 0.8 + r * 0.3)
            } else {
                sculk(layer, x, y)
            }
        }
        t::SHRIEKER_SIDE => {
            if y < 1 {
                CLEAR
            } else if y < 8 {
                // The bony rim: upright teeth with gaps between them.
                if x % 3 != 2 || y > 5 { shade(BONE, 0.72 + r * 0.25) } else { CLEAR }
            } else {
                sculk(layer, x, y)
            }
        }
        t::SHRIEKER_TOP | t::SHRIEKER_TOP_CAN_SUMMON => {
            // The mouth: a dark pit, with a soul-like swirl when it can summon.
            let d = (fx * fx + fy * fy).sqrt();
            if let Some(rim) = bone_rim(layer, x, y, 1) {
                rim
            } else if layer == t::SHRIEKER_TOP_CAN_SUMMON && (d - 3.5).abs() < 0.8 {
                shade(GLOW, 0.85 + r * 0.2)
            } else {
                noisy(layer, x, y, SCULK_DEEP, 0.2)
            }
        }
        _ => [255, 0, 255, 255],
    }
}
