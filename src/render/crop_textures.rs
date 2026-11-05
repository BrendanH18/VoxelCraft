//! Original procedural crop art, no reference assets copied.
use super::{Rgba, rnd, shade};
use crate::world::crops::t;
pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let noise = 0.82 + rnd(layer, x, y, 0) * 0.3;
    if layer >= t::BEETROOT {
        let age = (layer - t::BEETROOT) as usize;
        let height = 2 + 2 * age;
        if y < 16 - height {
            return [0; 4];
        }
        let leaf = ((x + age) % 5 <= 1 && y >= 16 - height + x % 3) || (y >= 14 && (3..=12).contains(&x));
        if age == 3 && y >= 14 && ((x as i32 - 4).abs() <= 1 || (x as i32 - 11).abs() <= 1) {
            return shade([155, 35, 65], noise);
        }
        return if leaf { shade([54, 136, 45], noise) } else { [0; 4] };
    }
    let attached = layer >= t::ATTACHED;
    let age = if attached { 7 } else { (layer - t::STEM) as usize };
    let height = 2 + age * 2;
    let stem = (7..=8).contains(&x) && y >= 16 - height;
    let branch = if attached {
        let right = (layer - t::ATTACHED).is_multiple_of(2);
        (3..=6).contains(&y)
            && if right { x >= 8 && y == 3 + x.abs_diff(8) / 3 } else { x <= 7 && y == 3 + x.abs_diff(7) / 3 }
    } else {
        age >= 2 && (x == 6 || x == 9) && y == 17 - height
    };
    if stem || branch { shade(if age >= 6 { [166, 158, 43] } else { [70, 156, 40] }, noise) } else { [0; 4] }
}
