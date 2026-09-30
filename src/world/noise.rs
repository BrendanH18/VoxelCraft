//! Seeded Perlin noise and integer hashing used by world generation.

pub struct Perlin {
    perm: [u8; 512],
}

#[inline(always)]
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic hash of an integer coordinate, for placement decisions.
#[inline(always)]
pub fn hash3(x: i32, y: i32, z: i32, seed: u64) -> u64 {
    let mut h = seed
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as u32 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^ (h >> 33)
}

/// Hash mapped to `[0, 1)`.
#[inline(always)]
pub fn hash_f(x: i32, y: i32, z: i32, seed: u64) -> f32 {
    (hash3(x, y, z, seed) >> 40) as f32 / (1u64 << 24) as f32
}

#[inline(always)]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline(always)]
fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

#[inline(always)]
fn grad2(h: u8, x: f32, y: f32) -> f32 {
    match h & 7 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        _ => -y,
    }
}

#[inline(always)]
fn grad3(h: u8, x: f32, y: f32, z: f32) -> f32 {
    match h & 15 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        11 => -y - z,
        12 => x + y,
        13 => -y + z,
        14 => -x + y,
        _ => -y - z,
    }
}

impl Perlin {
    pub fn new(seed: u64) -> Self {
        let mut p: [u8; 256] = core::array::from_fn(|i| i as u8);
        let mut s = seed;
        for i in (1..256).rev() {
            let j = (splitmix64(&mut s) % (i as u64 + 1)) as usize;
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        for (i, v) in perm.iter_mut().enumerate() {
            *v = p[i & 255];
        }
        Self { perm }
    }

    #[inline(always)]
    fn p(&self, i: usize) -> usize {
        self.perm[i] as usize
    }

    /// 2D gradient noise, roughly in `[-1, 1]`.
    pub fn noise2(&self, x: f32, y: f32) -> f32 {
        let (xf, yf) = (x.floor(), y.floor());
        let xi = (xf as i32 & 255) as usize;
        let yi = (yf as i32 & 255) as usize;
        let (x, y) = (x - xf, y - yf);
        let (u, v) = (fade(x), fade(y));
        let a = self.p(xi) + yi;
        let b = self.p(xi + 1) + yi;
        let perm = &self.perm;
        lerp(
            v,
            lerp(u, grad2(perm[a], x, y), grad2(perm[b], x - 1.0, y)),
            lerp(u, grad2(perm[a + 1], x, y - 1.0), grad2(perm[b + 1], x - 1.0, y - 1.0)),
        )
    }

    /// 3D gradient noise, roughly in `[-1, 1]`.
    pub fn noise3(&self, x: f32, y: f32, z: f32) -> f32 {
        let (xf, yf, zf) = (x.floor(), y.floor(), z.floor());
        let xi = (xf as i32 & 255) as usize;
        let yi = (yf as i32 & 255) as usize;
        let zi = (zf as i32 & 255) as usize;
        let (x, y, z) = (x - xf, y - yf, z - zf);
        let (u, v, w) = (fade(x), fade(y), fade(z));
        let a = self.p(xi) + yi;
        let aa = self.p(a) + zi;
        let ab = self.p(a + 1) + zi;
        let b = self.p(xi + 1) + yi;
        let ba = self.p(b) + zi;
        let bb = self.p(b + 1) + zi;
        let perm = &self.perm;
        lerp(
            w,
            lerp(
                v,
                lerp(u, grad3(perm[aa], x, y, z), grad3(perm[ba], x - 1.0, y, z)),
                lerp(u, grad3(perm[ab], x, y - 1.0, z), grad3(perm[bb], x - 1.0, y - 1.0, z)),
            ),
            lerp(
                v,
                lerp(u, grad3(perm[aa + 1], x, y, z - 1.0), grad3(perm[ba + 1], x - 1.0, y, z - 1.0)),
                lerp(u, grad3(perm[ab + 1], x, y - 1.0, z - 1.0), grad3(perm[bb + 1], x - 1.0, y - 1.0, z - 1.0)),
            ),
        )
    }

    /// Fractal Brownian motion over `noise2`, normalised to roughly `[-1, 1]`.
    pub fn fbm2(&self, x: f32, y: f32, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for i in 0..octaves {
            // Offset each octave so lattice artefacts don't line up.
            let o = i as f32 * 17.31;
            sum += amp * self.noise2(x * freq + o, y * freq - o);
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let a = Perlin::new(42);
        let b = Perlin::new(42);
        for i in 0..1000 {
            let x = i as f32 * 0.37 - 150.0;
            let y = i as f32 * 0.11 + 3.0;
            let n = a.noise2(x, y);
            assert_eq!(n, b.noise2(x, y));
            assert!((-1.5..=1.5).contains(&n));
            assert!((-1.5..=1.5).contains(&a.noise3(x, y, -x)));
        }
    }
}
