//! Original timber, nest fibres and honeycomb, all generated here.
use super::{Rgba, rnd, shade};
use crate::world::bees::t;
pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let v = 0.84 + rnd(layer, x, y, 0) * 0.25;
    if layer == t::COMB {
        let gx = (x + (y / 4 % 2) * 3) % 6;
        let gy = y % 4;
        return shade(if gx == 0 || gy == 0 { [168, 101, 20] } else { [234, 166, 39] }, v);
    }
    let nest = layer < t::HIVE_SIDE;
    let local = if nest { layer - t::NEST_SIDE } else { layer - t::HIVE_SIDE };
    if (local == 2 || local == 3) && (4..=11).contains(&x) && (8..=10).contains(&y) {
        return shade(if local == 3 { [238, 169, 30] } else { [46, 30, 15] }, v);
    }
    let c = if nest {
        if (y + x / 5).is_multiple_of(4) { [153, 109, 51] } else { [196, 158, 90] }
    } else if y.is_multiple_of(5) {
        [112, 72, 34]
    } else {
        [173, 126, 65]
    };
    shade(c, v)
}
