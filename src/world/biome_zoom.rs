//! Java's `BiomeManager` fuzzy zoom. Biomes are stored per 4×4×4 quart;
//! `getBiome(x, y, z)` does not read the quart containing the block but
//! the nearest of the eight surrounding quart centres after jittering each
//! with a hash of the world seed, which roughens straight borders. Bytecode
//! (`getBiome`, `getFiddledDistance`, `getFiddle`) and
//! `LinearCongruentialGenerator.next`; the seed is
//! `Hashing.sha256().hashLong(seed).asLong()`.

/// The jitter source of one world.
#[derive(Clone, Copy, Debug)]
pub struct BiomeZoom {
    seed: i64,
}

impl BiomeZoom {
    /// `BiomeManager.obfuscateSeed`: SHA-256 of the seed as eight little-endian
    /// bytes, its first eight bytes read as a little-endian `long`.
    pub fn new(world_seed: i64) -> Self {
        let digest = sha256_one_block(&world_seed.to_le_bytes());
        Self { seed: i64::from_le_bytes(digest[..8].try_into().unwrap()) }
    }

    pub fn seed(self) -> i64 {
        self.seed
    }

    /// The quart whose biome `getBiome(x, y, z)` returns for block `(x, y, z)`.
    pub fn quart(self, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
        let (i, j, k) = (x - 2, y - 2, z - 2);
        let (qx, qy, qz) = (i >> 2, j >> 2, k >> 2);
        let frac = |v: i32| f64::from(v & 3) / 4.0;
        let (dx, dy, dz) = (frac(i), frac(j), frac(k));
        let (mut best, mut least) = (0, f64::INFINITY);
        for corner in 0..8 {
            let (hx, hy, hz) = (corner & 4 == 0, corner & 2 == 0, corner & 1 == 0);
            let pick = |low: bool, q: i32| if low { q } else { q + 1 };
            let rel = |low: bool, d: f64| if low { d } else { d - 1.0 };
            let d =
                self.fiddled_distance(pick(hx, qx), pick(hy, qy), pick(hz, qz), rel(hx, dx), rel(hy, dy), rel(hz, dz));
            if least > d {
                best = corner;
                least = d;
            }
        }
        (
            if best & 4 == 0 { qx } else { qx + 1 },
            if best & 2 == 0 { qy } else { qy + 1 },
            if best & 1 == 0 { qz } else { qz + 1 },
        )
    }

    fn fiddled_distance(self, x: i32, y: i32, z: i32, xr: f64, yr: f64, zr: f64) -> f64 {
        let mut l = self.seed;
        for v in [x, y, z, x, y, z] {
            l = lcg_next(l, i64::from(v));
        }
        let d = fiddle(l);
        l = lcg_next(l, self.seed);
        let e = fiddle(l);
        l = lcg_next(l, self.seed);
        let f = fiddle(l);
        (zr + f).powi(2) + (yr + e).powi(2) + (xr + d).powi(2)
    }
}

/// `LinearCongruentialGenerator.next`.
fn lcg_next(seed: i64, add: i64) -> i64 {
    seed.wrapping_mul(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)).wrapping_add(add)
}

/// `BiomeManager.getFiddle`.
fn fiddle(l: i64) -> f64 {
    let d = (l >> 24).rem_euclid(1024) as f64 / 1024.0;
    (d - 0.5) * 0.9
}

/// SHA-256 of a message short enough for one 64-byte block.
fn sha256_one_block(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    assert!(message.len() <= 55, "one block only");
    let mut block = [0u8; 64];
    block[..message.len()].copy_from_slice(message);
    block[message.len()] = 0x80;
    block[56..].copy_from_slice(&((message.len() as u64) * 8).to_be_bytes());
    let mut w = [0u32; 64];
    for (slot, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
        *slot = u32::from_be_bytes(*word);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut v = h;
    for i in 0..64 {
        let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
        let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
        let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
        let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
        let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
        let t2 = s0.wrapping_add(maj);
        v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
    }
    for (a, b) in h.iter_mut().zip(v) {
        *a = a.wrapping_add(b);
    }
    let mut out = [0u8; 32];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = word.to_be_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_vectors() {
        let hex = |d: [u8; 32]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(sha256_one_block(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hex(sha256_one_block(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(
            hex(sha256_one_block(&12345i64.to_le_bytes())),
            "e1543551249113046932741cc28f36b4bbcc542233eb5094874072d3167f160a"
        );
    }

    /// Values from a Java transcription of `BiomeManager` (`obfuscateSeed` with
    /// Guava's SHA-256 `hashLong`, `getFiddledDistance`, the eight-corner search)
    /// run on a JDK for these seeds and positions.
    #[test]
    fn fuzzy_quarts_match_the_java_bytecode_transcription() {
        let points =
            [(0, 64, 0), (3, 70, 5), (-1, -1, -1), (100, 33, -250), (-4097, 120, 4097), (17, 0, 17), (18, 127, -19)];
        type Quart = (i32, i32, i32);
        let cases: [(i64, i64, [Quart; 7]); 2] = [
            (
                12345,
                293737985876514017,
                [(-1, 16, -1), (0, 17, 1), (-1, 0, -1), (24, 8, -63), (-1025, 29, 1024), (4, 0, 4), (4, 31, -5)],
            ),
            (
                -987654321,
                739882241652834894,
                [(-1, 16, 0), (0, 17, 1), (-1, -1, 0), (25, 7, -63), (-1025, 29, 1023), (4, -1, 4), (4, 32, -5)],
            ),
        ];
        for (seed, zoom_seed, expected) in cases {
            let zoom = BiomeZoom::new(seed);
            assert_eq!(zoom.seed(), zoom_seed);
            for (p, want) in points.iter().zip(expected) {
                assert_eq!(zoom.quart(p.0, p.1, p.2), want, "seed {seed} at {p:?}");
            }
        }
    }
}
