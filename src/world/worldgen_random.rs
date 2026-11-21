//! v0.16 Track F: Java 26.3's `WorldgenRandom` over a `XoroshiroRandomSource`,
//! the random `ChunkGenerator.applyBiomeDecoration` hands each structure's
//! `postProcess` (`setDecorationSeed`, then `setFeatureSeed` with the
//! structure's index in its generation step).
//!
//! `WorldgenRandom` extends `LegacyRandomSource` but draws its bits from the
//! wrapped source: `next(bits)` is the top `bits` of a Xoroshiro128++
//! `nextLong`, and `nextInt(bound)`, `nextLong` and `nextBoolean` are
//! `BitRandomSource`'s defaults over that `next`.

use super::loot::Xoroshiro;

/// `GenerationStep.Decoration.SURFACE_STRUCTURES.ordinal()`.
pub const SURFACE_STRUCTURES: i32 = 4;

/// `RandomSupport.mixStafford13`.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

pub struct WorldgenRandom(Xoroshiro);

impl WorldgenRandom {
    pub fn new(seed: i64) -> Self {
        let mut r = WorldgenRandom(Xoroshiro::from_state(0, 0));
        r.set_seed(seed);
        r
    }

    /// `XoroshiroRandomSource.setSeed` (`RandomSupport.upgradeSeedTo128bit`).
    pub fn set_seed(&mut self, seed: i64) {
        let lo = seed as u64 ^ 0x6a09_e667_f3bc_c909;
        let hi = lo.wrapping_add(0x9e37_79b9_7f4a_7c15);
        self.0 = Xoroshiro::from_state(mix(lo), mix(hi));
    }

    /// `WorldgenRandom.next`.
    fn next(&mut self, bits: u32) -> i32 {
        (self.0.next_long() >> (64 - bits)) as i32
    }

    /// `BitRandomSource.nextLong`.
    pub fn next_long(&mut self) -> i64 {
        let upper = i64::from(self.next(32));
        let lower = i64::from(self.next(32));
        (upper << 32).wrapping_add(lower)
    }

    /// `BitRandomSource.nextInt(bound)`.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0);
        if bound.isolate_lowest_one() == bound {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let sample = self.next(31);
            let modulo = sample % bound;
            if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                return modulo;
            }
        }
    }

    pub fn next_bool(&mut self) -> bool {
        self.next(1) != 0
    }

    /// `setDecorationSeed(seed, blockX, blockZ)`, returning the decoration seed.
    pub fn set_decoration_seed(&mut self, seed: i64, x: i32, z: i32) -> i64 {
        self.set_seed(seed);
        let xs = self.next_long() | 1;
        let zs = self.next_long() | 1;
        let result = (i64::from(x).wrapping_mul(xs)).wrapping_add(i64::from(z).wrapping_mul(zs)) ^ seed;
        self.set_seed(result);
        result
    }

    /// `setFeatureSeed(decorationSeed, index, step)`.
    pub fn set_feature_seed(&mut self, decoration: i64, index: i32, step: i32) {
        self.set_seed(decoration.wrapping_add(i64::from(index)).wrapping_add(10_000 * i64::from(step)));
    }

    /// The random a structure at `index` of `step` gets while the chunk
    /// whose lowest corner is block `(x, z)` decorates.
    pub fn structure(seed: u64, x: i32, z: i32, index: i32, step: i32) -> Self {
        let mut r = WorldgenRandom::new(0);
        let decoration = r.set_decoration_seed(seed as i64, x, z);
        r.set_feature_seed(decoration, index, step);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values from Java 26.3: `new WorldgenRandom(new XoroshiroRandomSource(0))`
    /// draws its first `nextLong` from Xoroshiro128++ seeded with
    /// `upgradeSeedTo128bit(0)`, whose state is the Stafford mixes of the
    /// golden-ratio constants.
    #[test]
    fn seeds_like_xoroshiro_random_source() {
        let mut r = WorldgenRandom::new(0);
        let mut x = Xoroshiro::from_state(
            mix(0x6a09_e667_f3bc_c909),
            mix(0x6a09_e667_f3bc_c909_u64.wrapping_add(0x9e37_79b9_7f4a_7c15)),
        );
        let a = x.next_long();
        let b = x.next_long();
        assert_eq!(r.next_long(), (((a >> 32) as i32 as i64) << 32).wrapping_add((b >> 32) as i32 as i64));
    }

    #[test]
    fn bounded_draws_stay_in_range() {
        let mut r = WorldgenRandom::structure(12345, -64, 48, 24, SURFACE_STRUCTURES);
        for bound in [1, 2, 3, 4, 6, 7, 100] {
            for _ in 0..100 {
                assert!((0..bound).contains(&r.next_int(bound)));
            }
        }
    }
}
