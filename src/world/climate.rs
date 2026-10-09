//! The Overworld's climate noise and terrain shape.
//!
//! Five 2D noise fields stand in for Java 1.18's climate parameters
//! (temperature, humidity, continentalness, erosion, weirdness), scaled so
//! their values spread over Java's `-1..1` bands the way the vanilla
//! noise does. The surface height comes from the same parameters, like
//! Java's terrain splines: continentalness lifts land out of the ocean,
//! low erosion and high peaks-and-valleys raise mountains, mid erosion
//! makes plateaus, valleys sink toward rivers and high erosion flattens
//! swamps. Heights are Java's: sea level 63, peaks above 200.

use super::biome::Climate;
use super::noise::Perlin;

pub const SEA_LEVEL: i32 = 63;

pub struct ClimateNoise {
    temperature: Perlin,
    humidity: Perlin,
    continental: Perlin,
    erosion: Perlin,
    weirdness: Perlin,
    /// Warps sample positions a little so bands don't follow the lattice.
    shift: Perlin,
    detail: Perlin,
    jagged: Perlin,
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Piecewise-linear interpolation through `(x, y)` points sorted by x.
fn spline(points: &[(f32, f32)], x: f32) -> f32 {
    if x <= points[0].0 {
        return points[0].1;
    }
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x <= x1 {
            return lerp(y0, y1, (x - x0) / (x1 - x0));
        }
    }
    points[points.len() - 1].1
}

/// Ocean floors and coast heights by continentalness (Java's offset spline).
const CONTINENT: [(f32, f32); 10] = [
    (-1.2, 68.0),
    (-1.05, 64.0),
    (-1.0, 34.0),
    (-0.51, 32.0),
    (-0.44, 44.0),
    (-0.22, 49.0),
    (-0.13, 60.0),
    (-0.08, 65.0),
    (0.3, 70.0),
    (1.0, 78.0),
];

impl ClimateNoise {
    pub fn new(seed: u64) -> Self {
        let p = |salt: u64| Perlin::new(seed ^ salt.wrapping_mul(0x2545_F491_4F6C_DD1D));
        Self {
            temperature: p(21),
            humidity: p(22),
            continental: p(23),
            erosion: p(24),
            weirdness: p(25),
            shift: p(26),
            detail: p(27),
            jagged: p(28),
        }
    }

    /// The climate of column `(x, z)`.
    pub fn sample(&self, x: i32, z: i32) -> Climate {
        let (fx, fz) = (x as f32, z as f32);
        // Java shifts climate samples by up to four blocks.
        let sx = fx + self.shift.noise2(fx / 64.0, fz / 64.0) * 6.0;
        let sz = fz + self.shift.noise2(fz / 64.0 + 37.0, fx / 64.0 - 11.0) * 6.0;
        Climate {
            temperature: self.temperature.fbm2(sx / 2200.0, sz / 2200.0, 3) * 2.6,
            humidity: self.humidity.fbm2(sx / 1150.0, sz / 1150.0, 3) * 2.45,
            continentalness: self.continental.fbm2(sx / 1500.0, sz / 1500.0, 6) * 3.0 + 0.05,
            erosion: self.erosion.fbm2(sx / 1150.0, sz / 1150.0, 4) * 2.9,
            weirdness: self.weirdness.fbm2(sx / 760.0, sz / 760.0, 4) * 2.7,
        }
    }

    /// Surface height of a column with climate `c` (before biome rules).
    pub fn height(&self, c: &Climate, x: i32, z: i32) -> f32 {
        let (fx, fz) = (x as f32, z as f32);
        let cont = c.continentalness;
        let e = c.erosion;
        let pv = c.peaks_valleys();
        let mut h = spline(&CONTINENT, cont);
        let detail = self.detail.fbm2(fx / 48.0, fz / 48.0, 3);
        // How much the column belongs to the land, and how far inland.
        let land = smoothstep(-0.2, -0.1, cont);
        let inland = smoothstep(-0.11, 0.55, cont);
        // Low erosion is rugged; high erosion is flat.
        let rugged = 1.0 - smoothstep(-0.78, 0.45, e);
        // Hills and valleys follow peaks-and-valleys.
        let hills = pv * (3.0 + 24.0 * rugged) * (0.45 + 0.55 * inland);
        // Mountains: high peaks-and-valleys on rugged ground inland.
        let peak = (smoothstep(-0.2, 1.0, pv)).powf(1.7);
        let mountain = peak * rugged.powf(1.4) * (0.55 + 0.45 * inland);
        let jag = (1.0 - self.jagged.fbm2(fx / 22.0, fz / 22.0, 3).abs() * 2.2).max(0.0);
        let jagged = mountain * (0.4 + 0.6 * smoothstep(0.0, -0.4, c.weirdness)) * jag * 26.0;
        // Plateaus: mid erosion far enough inland, flat on top.
        let plateau_band = smoothstep(-0.5, -0.375, e) * (1.0 - smoothstep(-0.2225, -0.05, e));
        let plateau = plateau_band * smoothstep(0.0, 0.3, cont) * (1.0 - mountain);
        h += land * (hills * (1.0 - plateau * 0.6) + mountain * 160.0 + jagged + plateau * (32.0 + 22.0 * inland));
        // Windswept ground (erosion 0.45..0.55) is broken and lumpy.
        let shatter = smoothstep(0.38, 0.45, e) * (1.0 - smoothstep(0.55, 0.62, e)) * smoothstep(-0.3, 0.3, pv);
        h += land * shatter * (detail.abs() * 34.0 + 6.0) * (0.4 + 0.6 * inland);
        h += detail * (2.0 + 4.0 * rugged) * land;
        // Rivers: valleys cut down to just under the sea, except through
        // rugged mountain ground away from the coast.
        let valley = smoothstep(-0.55, -0.95, pv);
        let carves = smoothstep(-0.45, -0.375, e).max(1.0 - smoothstep(0.0, 0.05, cont));
        let river = valley * carves * land * (1.0 - smoothstep(0.5, 0.56, e));
        if river > 0.0 {
            let bed = SEA_LEVEL as f32 - 2.0 - 3.0 * smoothstep(-0.9, -1.0, pv);
            h = lerp(h, bed.min(h), river);
        }
        // Swamps: very high erosion inland flattens to just about sea level.
        let swampy = smoothstep(0.5, 0.6, e) * land * (1.0 - mountain);
        if swampy > 0.0 {
            h = lerp(h, SEA_LEVEL as f32 - 0.2 + detail * 1.6, swampy);
        }
        h.clamp(-50.0, 300.0)
    }
}

#[cfg(test)]
mod tests {
    use super::super::biome::{self, Biome};
    use super::*;

    /// Spot check that the climate spread gives Java-like proportions:
    /// some ocean, mostly land, a mix of every temperature.
    #[test]
    fn climate_bands_have_javas_proportions() {
        let n = ClimateNoise::new(99);
        let mut ocean = 0;
        let mut cold = 0;
        let mut hot = 0;
        let mut total = 0;
        for i in -80..80 {
            for j in -80..80 {
                let c = n.sample(i * 97, j * 97);
                total += 1;
                ocean += (c.continentalness < -0.19) as i32;
                cold += (c.temperature < -0.45) as i32;
                hot += (c.temperature > 0.55) as i32;
            }
        }
        let share = |v: i32| v as f32 / total as f32;
        assert!((0.2..0.5).contains(&share(ocean)), "ocean {}", share(ocean));
        assert!((0.05..0.3).contains(&share(cold)), "cold {}", share(cold));
        assert!((0.05..0.3).contains(&share(hot)), "hot {}", share(hot));
    }

    /// Prints percentiles of every parameter and the biome mix (run with
    /// `--ignored --nocapture` when tuning).
    #[test]
    #[ignore]
    fn print_distributions() {
        let n = ClimateNoise::new(12345);
        let mut v: [Vec<f32>; 6] = Default::default();
        let mut counts = std::collections::HashMap::new();
        for i in -150..150 {
            for j in -150..150 {
                let (x, z) = (i * 61, j * 61);
                let c = n.sample(x, z);
                for (k, val) in
                    [c.temperature, c.humidity, c.continentalness, c.erosion, c.weirdness, n.height(&c, x, z)]
                        .into_iter()
                        .enumerate()
                {
                    v[k].push(val);
                }
                *counts.entry(biome::pick(&c)).or_insert(0) += 1;
            }
        }
        for (name, mut vals) in ["temp", "humid", "cont", "erosion", "weird", "height"].into_iter().zip(v) {
            vals.sort_by(f32::total_cmp);
            let q = |p: f32| vals[((vals.len() - 1) as f32 * p) as usize];
            println!(
                "{name:8} p1 {:7.2} p10 {:7.2} p50 {:7.2} p90 {:7.2} p99 {:7.2}",
                q(0.01),
                q(0.1),
                q(0.5),
                q(0.9),
                q(0.99)
            );
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by_key(|&(_, c)| std::cmp::Reverse(c));
        for (b, c) in counts {
            println!("{:28} {:5.2}%", b.name(), c as f32 / 900.0);
        }
    }

    #[test]
    fn heights_span_deep_ocean_to_high_peaks() {
        let n = ClimateNoise::new(99);
        let (mut lo, mut hi) = (i32::MAX, i32::MIN);
        let mut biomes = std::collections::HashSet::new();
        for i in -60..60 {
            for j in -60..60 {
                let (x, z) = (i * 131, j * 131);
                let c = n.sample(x, z);
                let h = n.height(&c, x, z) as i32;
                lo = lo.min(h);
                hi = hi.max(h);
                biomes.insert(biome::pick(&c));
            }
        }
        assert!(lo < 40 && hi > 180, "heights {lo}..{hi}");
        assert!(biomes.len() >= 35, "{} biomes: {biomes:?}", biomes.len());
        assert!(biomes.contains(&Biome::CherryGrove) || biomes.contains(&Biome::Meadow));
    }
}
