//! Generation 7 (v1.0 gap-closing Track 4, GEN-04b): the End's terrain as
//! Java 26.3 generates it, ported from the jar's End noise router
//! (`NoiseRouterData.end`, `noise_settings/end.json`).
//!
//! The final density of a block is
//!
//! ```text
//! squeeze(interpolated_8x4x8(0.64 * lerp(gradient_y(4 -> 32, 0 -> 1), -0.234375,
//!     lerp(gradient_y(56 -> 312, 1 -> 0), -23.4375, islands + base_3d_noise))))
//! ```
//!
//! plus the beardifier (zero: End cities have no terrain adaptation), and a
//! block is end stone where it is above zero. `islands` is `end/islands`:
//! the main island's falloff `(clamp(100 - distance to the origin, -100,
//! 80) - 8) / 128` at y 0, max `EndIslandFunction` (the outer islands,
//! `DensityFunctions.endOuterIslands`). `base_3d_noise` is
//! `end/base_3d_noise`, `BlendedNoise(0.25, 0.25, 80, 160, 4)`.
//!
//! Everything is computed in the jar's own arithmetic (its floats where it
//! uses floats, doubles where it uses doubles, the same operation order),
//! and the interpolation fills a cell the way `InterpolatedFunction`'s
//! volume sampler does (z, then x, then stepping up in y), so chunk columns
//! match `NoiseBasedChunkGenerator` block for block; the fixture
//! `tests/fixtures/java/26.3/end_islands.json` checks it against the jar.

use crate::enchant::JavaRandom;

/// `GradientNoise.GRADIENT`.
const GRADIENT: [[i32; 3]; 16] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
    [1, 1, 0],
    [0, -1, 1],
    [-1, 1, 0],
    [0, -1, -1],
];

/// The End's noise height (`noise_settings/end.json`: min y 0, height 128).
pub const HEIGHT: i32 = 128;
const CELL_XZ: i32 = 8;
const CELL_Y: i32 = 4;
/// Cell corners up a column: y 0, 4, .. 128.
const CORNERS_Y: usize = (HEIGHT / CELL_Y) as usize + 1;

/// `GradientNoise`'s permutation table and offsets.
struct Gradient {
    perms: [u8; 256],
    offset: [f64; 3],
}

impl Gradient {
    fn new(rng: &mut JavaRandom, offset_scale: f64) -> Self {
        let offset =
            [rng.next_double() * offset_scale, rng.next_double() * offset_scale, rng.next_double() * offset_scale];
        let mut perms: [u8; 256] = std::array::from_fn(|i| i as u8);
        for i in 0..256 {
            let j = rng.next_bounded(256 - i as i32) as usize + i;
            perms.swap(i, j);
        }
        Self { perms, offset }
    }

    fn permute(&self, x: i32) -> i32 {
        i32::from(self.perms[(x & 0xFF) as usize])
    }

    fn grad(&self, x: i32) -> [i32; 3] {
        GRADIENT[(self.permute(x) & 15) as usize]
    }
}

/// `GradientNoise.wrap`.
fn wrap(x: f64) -> f64 {
    // `Math.nextDown(1.6777216E7)`.
    let half = f64::from_bits(16_777_216.0f64.to_bits() - 1);
    if x >= -half && x < half { x } else { x - (x / 3.3554432E7 + 0.5).floor() * 3.3554432E7 }
}

fn lerp(alpha: f32, a: f32, b: f32) -> f32 {
    a + alpha * (b - a)
}

/// `Mth.lerp3`.
#[allow(clippy::too_many_arguments)]
fn lerp3(a1: f32, a2: f32, a3: f32, v: [f32; 8]) -> f32 {
    let [x000, x100, x010, x110, x001, x101, x011, x111] = v;
    let lerp2 = |x00, x10, x01, x11| lerp(a2, lerp(a1, x00, x10), lerp(a1, x01, x11));
    lerp(a3, lerp2(x000, x100, x010, x110), lerp2(x001, x101, x011, x111))
}

fn smoothstep(x: f32) -> f32 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// Java's `SimplexNoise`, two-dimensional `get` only.
struct Simplex(Gradient);

impl Simplex {
    fn get(&self, x: f64, y: f64) -> f32 {
        let sqrt3 = 3.0f64.sqrt();
        let f2 = 0.5 * (sqrt3 - 1.0);
        let g2 = (3.0 - sqrt3) / 6.0;
        let xin = x + self.0.offset[0];
        let yin = y + self.0.offset[1];
        let s = (xin + yin) * f2;
        let i = (xin + s).floor() as i32;
        let j = (yin + s).floor() as i32;
        let t = f64::from(i.wrapping_add(j)) * g2;
        let x0 = xin - (f64::from(i) - t);
        let y0 = yin - (f64::from(j) - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - f64::from(i1) + g2;
        let y1 = y0 - f64::from(j1) + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;
        let (ii, jj) = (i & 0xFF, j & 0xFF);
        let p = |v| self.0.permute(v);
        let gi0 = p(ii + p(jj)) % 12;
        let gi1 = p(ii + i1 + p(jj + j1)) % 12;
        let gi2 = p(ii + 1 + p(jj + 1)) % 12;
        let corner = |g: i32, x: f64, y: f64| {
            let t = 0.5 - x * x - y * y - 0.0 * 0.0;
            if t < 0.0 {
                0.0
            } else {
                let t = t * t;
                let [gx, gy, gz] = GRADIENT[g as usize];
                t * t * (f64::from(gx) * x + f64::from(gy) * y + f64::from(gz) * 0.0)
            }
        };
        (70.0 * (corner(gi0, x0, y0) + corner(gi1, x1, y1) + corner(gi2, x2, y2))) as f32
    }
}

/// One `SmearedPerlinNoise` octave of `BlendedNoise`.
struct Smeared {
    noise: Gradient,
    fudge_scale: f64,
    /// The octave's frequency and amplitude in its `NoiseStack`.
    frequency: f64,
    amplitude: f32,
}

impl Smeared {
    /// `computeFudgeY`.
    fn fudge(&self, original_y: f64, relative_y: f64) -> f64 {
        let limit = if original_y >= 0.0 && original_y < relative_y { original_y } else { relative_y };
        f64::from((limit / self.fudge_scale + f64::from(1.0E-7f32)).floor() as i32) * self.fudge_scale
    }

    /// `SmearedPerlinNoise.addToVolume` for one point.
    fn sample(&self, (bx, by, bz): (i32, i32, i32), xz_scale: f64, y_scale: f64, amplitude: f32) -> f32 {
        let n = &self.noise;
        let z = wrap(f64::from(bz) * xz_scale) + n.offset[2];
        let floor_z = z.floor() as i32;
        let rz = (z - f64::from(floor_z)) as f32;
        let alpha_z = smoothstep(rz);
        let x = wrap(f64::from(bx) * xz_scale) + n.offset[0];
        let floor_x = x.floor() as i32;
        let rx = (x - f64::from(floor_x)) as f32;
        let x0 = n.permute(floor_x);
        let x1 = n.permute(floor_x.wrapping_add(1));
        let alpha_x = smoothstep(rx);
        let original_y = f64::from(by) * y_scale;
        let y = wrap(original_y) + n.offset[1];
        let floor_y = y.floor() as i32;
        let relative_y = y - f64::from(floor_y);
        let alpha_y = smoothstep(relative_y as f32);
        let xy00 = n.permute(x0 + floor_y);
        let xy01 = n.permute(x0 + floor_y + 1);
        let xy10 = n.permute(x1 + floor_y);
        let xy11 = n.permute(x1 + floor_y + 1);
        let dot_xz = |g: [i32; 3], x: f32, z: f32| g[0] as f32 * x + g[2] as f32 * z;
        let fy = (relative_y - self.fudge(original_y, relative_y)) as f32;
        let corner = |hash: i32, x: f32, z: f32, y: f32| {
            let g = n.grad(hash);
            dot_xz(g, x, z) + g[1] as f32 * y
        };
        let v = [
            corner(xy00 + floor_z, rx, rz, fy),
            corner(xy10 + floor_z, rx - 1.0, rz, fy),
            corner(xy01 + floor_z, rx, rz, fy - 1.0),
            corner(xy11 + floor_z, rx - 1.0, rz, fy - 1.0),
            corner(xy00 + floor_z + 1, rx, rz - 1.0, fy),
            corner(xy10 + floor_z + 1, rx - 1.0, rz - 1.0, fy),
            corner(xy01 + floor_z + 1, rx, rz - 1.0, fy - 1.0),
            corner(xy11 + floor_z + 1, rx - 1.0, rz - 1.0, fy - 1.0),
        ];
        amplitude * lerp3(alpha_x, alpha_y, alpha_z, v)
    }
}

/// `BlendedNoise.createFbm`: octaves from frequency 1 down, the amplitude
/// doubling as the frequency halves.
fn fbm(rng: &mut JavaRandom, first_octave: i32, smear_y: f64, value_factor: f64) -> Vec<Smeared> {
    let octaves = -first_octave + 1;
    let mut factor = 1.0f64;
    let mut value = value_factor / (2.0f64.powi(octaves) - 1.0);
    (0..octaves)
        .map(|_| {
            let noise = Gradient::new(rng, 256.0);
            let layer = Smeared { noise, fudge_scale: smear_y * factor, frequency: factor, amplitude: value as f32 };
            factor /= 2.0;
            value *= 2.0;
            layer
        })
        .collect()
}

/// `NoiseFunction.Sampler.sampleVolume` over a `NoiseStack` at one point:
/// the octaves added in order onto zero.
fn stack(layers: &[Smeared], p: (i32, i32, i32), xz_scale: f64, y_scale: f64) -> f32 {
    let mut out = 0.0f32;
    for l in layers {
        out += l.sample(p, xz_scale * l.frequency, y_scale * l.frequency, 1.0 * l.amplitude);
    }
    out
}

/// Java 26.3's End terrain noise for one world seed.
pub struct EndNoise {
    islands: Simplex,
    min_limit: Vec<Smeared>,
    max_limit: Vec<Smeared>,
    main: Vec<Smeared>,
}

/// `BlendedNoise` for the End: `xz_scale` and `y_scale` 0.25, `xz_factor`
/// 80, `y_factor` 160, `smear_scale_multiplier` 4.
const XZ_MULTIPLIER: f64 = 684.412 * 0.25;
const Y_MULTIPLIER: f64 = 684.412 * 0.25;

impl EndNoise {
    pub fn new(seed: u64) -> Self {
        // `createEndIslandRandom`: a legacy source on the seed, 17292 ints in.
        let mut rng = JavaRandom::new(seed as i64);
        for _ in 0..17292 {
            rng.next_int();
        }
        let islands = Simplex(Gradient::new(&mut rng, 0.0));
        // `createRandom(BlendedNoise.NOISE_SEED)` with legacy random sources.
        let mut rng = JavaRandom::new(seed as i64);
        let limit_smear = Y_MULTIPLIER * 4.0;
        let main_smear = limit_smear / 160.0;
        let limit = f64::from(0.99998474f32);
        let min_limit = fbm(&mut rng, -15, limit_smear, limit);
        let max_limit = fbm(&mut rng, -15, limit_smear, limit);
        let main = fbm(&mut rng, -7, main_smear, 12.75);
        Self { islands, min_limit, max_limit, main }
    }

    /// `EndIslandFunction.getHeightValue` for the 8-block section `sx, sz`.
    fn height_value(&self, sx: i32, sz: i32) -> f32 {
        let (chunk_x, chunk_z) = (sx / 2, sz / 2);
        let (sub_x, sub_z) = (sx % 2, sz % 2);
        let mut doffs = -100.0f32;
        for xo in -12..=12 {
            for zo in -12..=12 {
                let total_x = i64::from(chunk_x + xo);
                let total_z = i64::from(chunk_z + zo);
                if total_x * total_x + total_z * total_z > 4096
                    && self.islands.get(total_x as f64, total_z as f64) < -0.9f32
                {
                    let size = ((total_x as f32).abs() * 3439.0 + (total_z as f32).abs() * 147.0) % 13.0 + 9.0;
                    let xd = (sub_x - xo * 2) as f32;
                    let zd = (sub_z - zo * 2) as f32;
                    let d = (100.0 - (xd * xd + zd * zd).sqrt() * size).clamp(-100.0, 80.0);
                    doffs = doffs.max(d);
                }
            }
        }
        doffs
    }

    /// `EndIslandFunction` (`DensityFunctions.endOuterIslands`) at block
    /// `x, z`: the outer islands' height value.
    pub fn outer_islands(&self, x: i32, z: i32) -> f32 {
        (self.height_value(x / 8, z / 8) - 8.0) / 128.0
    }

    /// `end/islands` (the End router's `erosion`, which `TheEndBiomeSource`
    /// reads): the main island's falloff, max the outer islands.
    pub fn islands(&self, x: i32, z: i32) -> f32 {
        let (dx, dz) = ((0i32.wrapping_sub(x)) as f32, (0i32.wrapping_sub(z)) as f32);
        let distance = ((dx * dx + 0.0 * 0.0 + dz * dz) as f64).sqrt() as f32;
        let main = ((100.0f32 - distance).clamp(-100.0, 80.0) + -8.0f32) * 0.0078125f32;
        main.max(self.outer_islands(x, z))
    }

    /// `end/base_3d_noise` at block `p`: the main noise picks between the
    /// two limit noises.
    fn base_3d(&self, p: (i32, i32, i32)) -> f32 {
        let min = stack(&self.min_limit, p, XZ_MULTIPLIER, Y_MULTIPLIER);
        let max = stack(&self.max_limit, p, XZ_MULTIPLIER, Y_MULTIPLIER);
        let main = stack(&self.main, p, XZ_MULTIPLIER / 80.0, Y_MULTIPLIER / 160.0);
        let alpha = (main + 0.5f32).clamp(0.0, 1.0);
        if alpha == 0.0 {
            min
        } else if alpha == 1.0 {
            max
        } else {
            lerp(alpha, min, max)
        }
    }

    /// The final density's input at a cell corner, given the column's
    /// `islands` value: the sloped cheese faded to its floor and ceiling
    /// targets and scaled by 0.64.
    fn corner(&self, islands: f32, p: (i32, i32, i32)) -> f32 {
        let y = p.1;
        // `yClampedGradient`s: `from + (clamp(y) - from_y) * factor`.
        let top = 1.0f32 + (y.clamp(56, 312) - 56) as f32 * (-1.0f32 / 256.0);
        let bottom = 0.0f32 + (y.clamp(4, 32) - 4) as f32 * (1.0f32 / 28.0);
        let sloped = islands + self.base_3d(p);
        let inner = if top == 0.0 {
            -23.4375
        } else if top == 1.0 {
            sloped
        } else {
            lerp(top, -23.4375, sloped)
        };
        let outer = if bottom == 0.0 {
            -0.234375
        } else if bottom == 1.0 {
            inner
        } else {
            lerp(bottom, -0.234375, inner)
        };
        outer * 0.64f32
    }

    /// The cell corners of the column at block `x, z` (a multiple of 8).
    fn corner_column(&self, x: i32, z: i32) -> [f32; CORNERS_Y] {
        let islands = self.islands(x, z);
        std::array::from_fn(|i| self.corner(islands, (x, i as i32 * CELL_Y, z)))
    }

    /// Which blocks of the 16 x 16 chunk `cx, cz` are end stone: bit `y` of
    /// `[z * 16 + x]`, for y 0..128. `NoiseBasedChunkGenerator.doFill` over
    /// the chunk's volume: each cell filled by z, then x, then y stepped
    /// up from its bottom corners, then squeezed.
    pub fn chunk(&self, cx: i32, cz: i32) -> [u128; 256] {
        let (x0, z0) = (cx * 16, cz * 16);
        let corners: [[[f32; CORNERS_Y]; 3]; 3] =
            std::array::from_fn(|k| std::array::from_fn(|i| self.corner_column(x0 + i as i32 * 8, z0 + k as i32 * 8)));
        let mut out = [0u128; 256];
        let inv_xz = 1.0f32 / CELL_XZ as f32;
        let inv_y = 1.0f32 / CELL_Y as f32;
        for cell_z in 0..2 {
            for cell_x in 0..2 {
                for cell_y in 0..(CORNERS_Y - 1) {
                    let c = |dx: usize, dy: usize, dz: usize| corners[cell_z + dz][cell_x + dx][cell_y + dy];
                    let (v000, v100, v010, v110) = (c(0, 0, 0), c(1, 0, 0), c(0, 1, 0), c(1, 1, 0));
                    let (v001, v101, v011, v111) = (c(0, 0, 1), c(1, 0, 1), c(0, 1, 1), c(1, 1, 1));
                    for z in 0..CELL_XZ {
                        let az = z as f32 * inv_xz;
                        let v00 = lerp(az, v000, v001);
                        let v01 = lerp(az, v010, v011);
                        let v10 = lerp(az, v100, v101);
                        let v11 = lerp(az, v110, v111);
                        for x in 0..CELL_XZ {
                            let ax = x as f32 * inv_xz;
                            let v0 = lerp(ax, v00, v10);
                            let v1 = lerp(ax, v01, v11);
                            let step = (v1 - v0) * inv_y;
                            let mut value = v0 + step * 0.0;
                            let column = (cell_z as i32 * 8 + z) * 16 + cell_x as i32 * 8 + x;
                            for y in 0..CELL_Y {
                                if squeeze(value) > 0.0 {
                                    out[column as usize] |= 1u128 << (cell_y as i32 * CELL_Y + y);
                                }
                                value += step;
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

/// `UnaryFunction.SqueezeSampler.apply`.
fn squeeze(input: f32) -> f32 {
    let c = input.clamp(-1.0, 1.0);
    c / 2.0 - c * c * c / 24.0
}

#[cfg(test)]
mod tests;
