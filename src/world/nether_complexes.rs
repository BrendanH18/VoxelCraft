//! Java's shared `nether_complexes` placement: 27/4 spacing, salt 30084232,
//! fortress weight 2, bastion weight 3. Coordinates use Java's 16-block chunks.
use crate::enchant::JavaRandom;
use glam::IVec2;

pub const REGION: i32 = 27 * 16;
pub const START_MAX: i32 = 22 * 16 + 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Complex {
    Fortress,
    Bastion,
}

fn next_long(rng: &mut JavaRandom) -> i64 {
    ((rng.next_int() as i64) << 32).wrapping_add(rng.next_int() as i64)
}

/// The candidate start and selected structure. Both generators call this,
/// so neither can consume a different random stream or claim the same region.
pub fn placement(seed: u64, region: IVec2) -> (IVec2, Complex) {
    let salt_seed = (region.x as i64)
        .wrapping_mul(341_873_128_712)
        .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541))
        .wrapping_add(seed as i64)
        .wrapping_add(30_084_232);
    let mut spread = JavaRandom::new(salt_seed);
    let chunk = region * 27 + IVec2::new(spread.next_bounded(23), spread.next_bounded(23));
    let mut large = JavaRandom::new(seed as i64);
    let (x, z) = (next_long(&mut large), next_long(&mut large));
    let mut choice = JavaRandom::new((chunk.x as i64).wrapping_mul(x) ^ (chunk.y as i64).wrapping_mul(z) ^ seed as i64);
    let kind = if choice.next_bounded(5) < 2 { Complex::Fortress } else { Complex::Bastion };
    (chunk * 16 + IVec2::splat(2), kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_uses_java_spacing_and_weighted_selection_in_negative_regions() {
        let mut fortresses = 0;
        for x in -50..50 {
            for z in -50..50 {
                let r = IVec2::new(x, z);
                let (p, kind) = placement(12345, r);
                let chunk = p.div_euclid(IVec2::splat(16));
                assert_eq!(chunk.div_euclid(IVec2::splat(27)), r);
                assert!(chunk.rem_euclid(IVec2::splat(27)).cmplt(IVec2::splat(23)).all());
                fortresses += (kind == Complex::Fortress) as usize;
            }
        }
        assert!((3800..4200).contains(&fortresses));
    }
}
