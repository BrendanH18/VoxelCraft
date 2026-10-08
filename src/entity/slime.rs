//! Java slime-chunk selection. Coordinates are Java's 16-block chunks,
//! independent of the engine's chunk size. Preserve Java int overflow and
//! signed int-to-long promotion before seeding java.util.Random's 48-bit LCG.

pub fn slime_chunk(seed: u64, x: i32, z: i32) -> bool {
    let mixed = seed
        .wrapping_add(x.wrapping_mul(x).wrapping_mul(4_987_142) as i64 as u64)
        .wrapping_add(x.wrapping_mul(5_947_611) as i64 as u64)
        .wrapping_add((z.wrapping_mul(z) as i64).wrapping_mul(4_392_871) as u64)
        .wrapping_add(z.wrapping_mul(389_711) as i64 as u64)
        ^ 987_234_911;
    let mut state = (mixed ^ 0x5DEECE66D) & ((1 << 48) - 1);
    loop {
        state = state.wrapping_mul(0x5DEECE66D).wrapping_add(11) & ((1 << 48) - 1);
        let bits = (state >> 17) as i32;
        let value = bits % 10;
        if bits.wrapping_sub(value).wrapping_add(9) >= 0 {
            return value == 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_seed_vectors_include_negative_and_overflow_coordinates() {
        // Independent java.util.Random reference vectors.
        for (seed, x, z, expected) in [(0, 0, 0, false), (0, 0, 1, false), (1, 0, 0, false)] {
            assert_eq!(slime_chunk(seed, x, z), expected);
        }
        let count = (-50..50).flat_map(|x| (-50..50).map(move |z| slime_chunk(42, x, z))).filter(|&b| b).count();
        assert!((800..1200).contains(&count));
    }
}
