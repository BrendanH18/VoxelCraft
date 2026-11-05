//! Original hand-designed procedural ancient flowers. No Mojang pixels.
use super::{Rgba, rnd, shade};
use crate::world::sniffer_plants::t;
pub(super) fn pixel(layer: u16, x: usize, y: usize) -> Rgba {
    let n = 0.88 + rnd(layer, x, y, 0) * 0.22;
    if layer == t::POTTED {
        // Existing pot models use cell-space UVs for both clay and plant planes.
        if y >= 10 && (5..11).contains(&x) {
            return shade([145, 74, 47], n);
        }
        return torch(2, x, y, n);
    }
    if layer < t::PITCHER {
        return torch((layer - t::TORCH) as usize, x, y, n);
    }
    let (age, upper) = if layer >= t::FLOWER {
        (4, layer > t::FLOWER)
    } else {
        let i = (layer - t::PITCHER) as usize;
        (i % 5, i >= 5)
    };
    if upper && age < 3 {
        return [0; 4];
    }
    let yy = y + if upper {
        0
    } else if age >= 3 {
        16
    } else {
        0
    };
    if age >= 3 {
        // Paired pink pitchers on a green stalk, cyan lips and a central bud.
        if upper && ((3..=6).contains(&x) || (10..=13).contains(&x)) && (7..=14).contains(&y) {
            return shade(if y == 7 { [68, 183, 183] } else { [166, 72, 112] }, n);
        }
        if upper && (6..=9).contains(&x) && (3..=6).contains(&y) && age == 4 {
            return shade([210, 137, 162], n);
        }
        let stem = (7..=8).contains(&x) && yy >= if age == 4 { 5 } else { 9 };
        let leaf = yy >= 20 && (yy as i32 - (26 - (x as i32 - 8).abs() / 2)).abs() <= 1;
        if stem || leaf {
            return shade([65, 137, 85], n);
        }
    } else {
        let h = 3 + age * 4;
        if y >= 16 - h && (6..=9).contains(&x) {
            return shade(if age == 0 { [142, 78, 73] } else { [69, 134, 83] }, n);
        }
        if age > 0 && y >= 13 - age && y <= 14 && (3..=12).contains(&x) {
            return shade([78, 154, 91], n);
        }
    }
    [0; 4]
}
fn torch(age: usize, x: usize, y: usize, n: f32) -> Rgba {
    let h = 6 + 4 * age;
    if (7..=8).contains(&x) && y >= 16 - h {
        return shade([63, 138, 60], n);
    }
    if (12..=14).contains(&y) && (3..=12).contains(&x) && x.abs_diff(8) + y <= 21 {
        return shade([75, 151, 53], n);
    }
    if age == 2 && (3..=11).contains(&x) && (1..=7).contains(&y) {
        let flame = (x as i32 - 7).abs() + (y as i32 - 5).abs() / 2 <= 3;
        if flame {
            return shade(if y <= 3 { [239, 166, 30] } else { [218, 83, 27] }, n);
        }
    }
    [0; 4]
}
