//! Nether biomes: Java 1.21's multi-noise biome source for the Nether.
//!
//! Java's Nether noise router samples two climate values per 4-block
//! "quart" column (Nether biomes have no vertical variation since 1.18):
//! temperature and vegetation, each a `NormalNoise` at a quarter of the
//! block scale, domain-warped by the `offset` shift noise. The biome is
//! the parameter point closest to the sampled climate, with each point's
//! offset added to its squared distance (`Climate.ParameterPoint.fitness`).
//!
//! Octave layouts, amplitudes, input/value factors and the parameter points
//! follow Java's `NoiseData`, `NormalNoise`, `PerlinNoise`, `NoiseRouterData`
//! and `MultiNoiseBiomeSourceParameterList.Preset.NETHER`. The gradient noise
//! is the same Improved Perlin noise, so climate values have Java's scale and
//! spread; the permutation tables come from our own seed hashing, so worlds
//! are not copies of Java seeds.

use glam::{DVec3, IVec2, IVec3};

use super::noise::{Perlin, hash3};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum NetherBiome {
    NetherWastes,
    SoulSandValley,
    CrimsonForest,
    WarpedForest,
    BasaltDeltas,
}

/// A mob in a Nether biome's spawn list (Java's `NetherBiomes`), mapped to
/// its `MobKind` by `entity::Entities::natural_spawn` (see `nether_spawn`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NetherMob {
    ZombifiedPiglin,
    Ghast,
    MagmaCube,
    Enderman,
    Skeleton,
    Piglin,
    Hoglin,
    Strider,
}

/// One spawn list entry: weight and group size range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spawn {
    pub mob: NetherMob,
    pub weight: u32,
    pub group: (u8, u8),
}

const fn spawn(mob: NetherMob, weight: u32, min: u8, max: u8) -> Spawn {
    Spawn { mob, weight, group: (min, max) }
}

/// Java's `NetherBiomes` monster lists.
const WASTES_MONSTERS: [Spawn; 5] = [
    spawn(NetherMob::Ghast, 50, 4, 4),
    spawn(NetherMob::ZombifiedPiglin, 100, 4, 4),
    spawn(NetherMob::MagmaCube, 2, 4, 4),
    spawn(NetherMob::Enderman, 1, 4, 4),
    spawn(NetherMob::Piglin, 15, 4, 4),
];
const SOUL_SAND_VALLEY_MONSTERS: [Spawn; 3] =
    [spawn(NetherMob::Skeleton, 20, 5, 5), spawn(NetherMob::Ghast, 50, 4, 4), spawn(NetherMob::Enderman, 1, 4, 4)];
const BASALT_DELTAS_MONSTERS: [Spawn; 2] = [spawn(NetherMob::Ghast, 40, 1, 1), spawn(NetherMob::MagmaCube, 100, 2, 5)];
const CRIMSON_FOREST_MONSTERS: [Spawn; 3] =
    [spawn(NetherMob::ZombifiedPiglin, 1, 2, 4), spawn(NetherMob::Hoglin, 9, 3, 4), spawn(NetherMob::Piglin, 5, 3, 4)];
const WARPED_FOREST_MONSTERS: [Spawn; 1] = [spawn(NetherMob::Enderman, 1, 4, 4)];
/// Every Nether biome's creature list: striders on the lava.
const CREATURES: [Spawn; 1] = [spawn(NetherMob::Strider, 60, 1, 2)];

/// Java's `AmbientParticleSettings`: the particle and the chance each
/// animate-tick sample emits it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ambient {
    CrimsonSpore,
    WarpedSpore,
    Ash,
    WhiteAsh,
}

impl NetherBiome {
    pub const ALL: [NetherBiome; 5] = [
        NetherBiome::NetherWastes,
        NetherBiome::SoulSandValley,
        NetherBiome::CrimsonForest,
        NetherBiome::WarpedForest,
        NetherBiome::BasaltDeltas,
    ];

    pub fn name(self) -> &'static str {
        match self {
            NetherBiome::NetherWastes => "nether_wastes",
            NetherBiome::SoulSandValley => "soul_sand_valley",
            NetherBiome::CrimsonForest => "crimson_forest",
            NetherBiome::WarpedForest => "warped_forest",
            NetherBiome::BasaltDeltas => "basalt_deltas",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Self::ALL.into_iter().find(|b| b.name() == name)
    }

    /// Java's parameter point: (temperature, humidity/vegetation, offset).
    /// Continentalness, erosion, depth and weirdness are zero for every
    /// Nether biome and in the Nether router, so they never contribute.
    pub const fn point(self) -> (f32, f32, f32) {
        match self {
            NetherBiome::NetherWastes => (0.0, 0.0, 0.0),
            NetherBiome::SoulSandValley => (0.0, -0.5, 0.0),
            NetherBiome::CrimsonForest => (0.4, 0.0, 0.0),
            NetherBiome::WarpedForest => (0.0, 0.5, 0.375),
            NetherBiome::BasaltDeltas => (-0.5, 0.0, 0.175),
        }
    }

    /// The biome whose parameter point is nearest the climate `(t, h)`.
    pub fn select(t: f32, h: f32) -> NetherBiome {
        let fitness = |b: NetherBiome| {
            let (pt, ph, offset) = b.point();
            (t - pt) * (t - pt) + (h - ph) * (h - ph) + offset * offset
        };
        let mut best = NetherBiome::NetherWastes;
        let mut score = f32::INFINITY;
        for b in Self::ALL {
            let f = fitness(b);
            if f < score {
                (best, score) = (b, f);
            }
        }
        best
    }

    /// Java's biome `fog_color` (and sky colour, which matches in the Nether).
    pub const fn fog_rgb(self) -> u32 {
        match self {
            NetherBiome::NetherWastes => 0x330808,
            NetherBiome::SoulSandValley => 0x1B4745,
            NetherBiome::CrimsonForest => 0x330303,
            NetherBiome::WarpedForest => 0x1A051A,
            NetherBiome::BasaltDeltas => 0x685F70,
        }
    }

    pub fn fog_color(self) -> [f32; 3] {
        let c = self.fog_rgb();
        [(c >> 16 & 255) as f32 / 255.0, (c >> 8 & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
    }

    /// Ambient particles and their per-sample probability (`NetherBiomes`).
    pub const fn ambient(self) -> Option<(Ambient, f32)> {
        match self {
            NetherBiome::NetherWastes => None,
            NetherBiome::SoulSandValley => Some((Ambient::Ash, 0.006_25)),
            NetherBiome::CrimsonForest => Some((Ambient::CrimsonSpore, 0.025)),
            NetherBiome::WarpedForest => Some((Ambient::WarpedSpore, 0.014_28)),
            NetherBiome::BasaltDeltas => Some((Ambient::WhiteAsh, 0.118_093_334)),
        }
    }

    /// Java's monster spawn list for the biome.
    pub const fn monsters(self) -> &'static [Spawn] {
        match self {
            NetherBiome::NetherWastes => &WASTES_MONSTERS,
            NetherBiome::SoulSandValley => &SOUL_SAND_VALLEY_MONSTERS,
            NetherBiome::CrimsonForest => &CRIMSON_FOREST_MONSTERS,
            NetherBiome::WarpedForest => &WARPED_FOREST_MONSTERS,
            NetherBiome::BasaltDeltas => &BASALT_DELTAS_MONSTERS,
        }
    }

    /// Java's creature spawn list: striders in every Nether biome.
    pub const fn creatures(self) -> &'static [Spawn] {
        &CREATURES
    }

    /// The spawn entry for `mob` here, and the chance an attempt for it goes
    /// ahead: its weight against the heaviest monster in the biome.
    pub fn monster_spawn(self, mob: NetherMob) -> Option<(Spawn, f32)> {
        let list = self.monsters();
        let heaviest = list.iter().map(|s| s.weight).max()?;
        list.iter().find(|s| s.mob == mob).map(|s| (*s, s.weight as f32 / heaviest as f32))
    }

    /// Bastion remnants generate everywhere but basalt deltas
    /// (`#has_structure/bastion_remnant`). Fortresses generate in every
    /// Nether biome.
    pub const fn has_bastions(self) -> bool {
        !matches!(self, NetherBiome::BasaltDeltas)
    }
}

/// The fog colour around a camera, blended like Java's `FogRenderer`:
/// `CubicSampler.gaussianSampleVec3` over the 6×6 quarts around
/// `(camera - 2) / 4` with kernel weights 1, 4, 6, 4, 1. The 36 biome
/// colours are kept until the camera crosses into another quart.
pub struct FogSampler {
    cell: Option<IVec2>,
    colors: [[f32; 3]; 36],
}

impl Default for FogSampler {
    fn default() -> Self {
        Self { cell: None, colors: [[0.0; 3]; 36] }
    }
}

impl FogSampler {
    const KERNEL: [f64; 7] = [0.0, 1.0, 4.0, 6.0, 4.0, 1.0, 0.0];

    /// `biome` gives the biome of a quart column.
    pub fn sample(&mut self, camera: DVec3, biome: impl Fn(i32, i32) -> NetherBiome) -> [f32; 3] {
        let p = (camera - DVec3::splat(2.0)) * 0.25;
        let cell = IVec2::new(p.x.floor() as i32, p.z.floor() as i32);
        if self.cell != Some(cell) {
            self.cell = Some(cell);
            for (i, c) in self.colors.iter_mut().enumerate() {
                *c = biome(cell.x - 2 + (i % 6) as i32, cell.y - 2 + (i / 6) as i32).fog_color();
            }
        }
        let (fx, fz) = (p.x - cell.x as f64, p.z - cell.y as f64);
        let weight = |f: f64, l: usize| Self::KERNEL[l + 1] + (Self::KERNEL[l] - Self::KERNEL[l + 1]) * f;
        let (mut sum, mut total) = ([0.0f64; 3], 0.0);
        for (i, c) in self.colors.iter().enumerate() {
            let w = weight(fx, i % 6) * weight(fz, i / 6);
            total += w;
            for (s, v) in sum.iter_mut().zip(c) {
                *s += *v as f64 * w;
            }
        }
        sum.map(|s| (s / total) as f32)
    }
}

/// Java's `PerlinNoise`: octaves from `first_octave` with per-octave
/// amplitudes; zero-amplitude octaves are skipped.
struct Octaves {
    /// (noise, random lattice offset, input factor, amplitude * value factor)
    levels: Vec<(Perlin, [f64; 3], f64, f64)>,
}

impl Octaves {
    fn new(seed: u64, first_octave: i32, amplitudes: &[f64]) -> Self {
        let n = amplitudes.len() as i32;
        let mut input = 2f64.powi(first_octave);
        let mut value = 2f64.powi(n - 1) / (2f64.powi(n) - 1.0);
        let mut levels = Vec::new();
        for (i, &amp) in amplitudes.iter().enumerate() {
            if amp != 0.0 {
                let s = seed ^ (i as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407);
                let offset = |k: i32| (hash3(i as i32, k, 0, s) >> 11) as f64 / (1u64 << 53) as f64 * 256.0;
                levels.push((Perlin::new(s), [offset(0), offset(1), offset(2)], input, amp * value));
            }
            input *= 2.0;
            value /= 2.0;
        }
        Self { levels }
    }

    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        // Our lattice repeats every 256 units, so wrapping keeps f32 precision
        // far from the origin without changing the value.
        let wrap = |v: f64| (v - (v * (1.0 / 256.0)).floor() * 256.0) as f32;
        self.levels
            .iter()
            .map(|(noise, o, input, weight)| {
                let n = noise.noise3(wrap(x * input + o[0]), wrap(y * input + o[1]), wrap(z * input + o[2]));
                n as f64 * weight
            })
            .sum()
    }
}

/// Java's `NormalNoise`: two octave sets, the second sampled at a slightly
/// different scale, normalised by the expected deviation of their span.
struct NormalNoise {
    first: Octaves,
    second: Octaves,
    value_factor: f64,
}

impl NormalNoise {
    const INPUT_FACTOR: f64 = 1.018_126_888_217_522_7;

    fn new(seed: u64, first_octave: i32, amplitudes: &[f64]) -> Self {
        let lo = amplitudes.iter().position(|&a| a != 0.0).unwrap_or(0);
        let hi = amplitudes.iter().rposition(|&a| a != 0.0).unwrap_or(0);
        let expected_deviation = 0.1 * (1.0 + 1.0 / (hi - lo + 1) as f64);
        Self {
            first: Octaves::new(seed, first_octave, amplitudes),
            second: Octaves::new(seed ^ 0x5DEE_CE66_D1CE_4E5B, first_octave, amplitudes),
            value_factor: 1.0 / 6.0 / expected_deviation,
        }
    }

    fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let f = Self::INPUT_FACTOR;
        (self.first.sample(x, y, z) + self.second.sample(x * f, y * f, z * f)) * self.value_factor
    }
}

/// The Nether's climate noises. Cheap to clone the seed into; construct one
/// per generator (or per structure set that filters on biomes).
pub struct NetherBiomeSource {
    temperature: NormalNoise,
    vegetation: NormalNoise,
    shift: NormalNoise,
}

impl NetherBiomeSource {
    pub fn new(seed: u64) -> Self {
        let s = |salt: u64| seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x4E42_494F_4D45;
        Self {
            // NoiseData: temperature (-10; 1.5, 0, 1, 0, 0, 0),
            // vegetation (-8; 1, 1, 0, 0, 0, 0), offset (-3; 1, 1, 1, 0).
            temperature: NormalNoise::new(s(1), -10, &[1.5, 0.0, 1.0, 0.0, 0.0, 0.0]),
            vegetation: NormalNoise::new(s(2), -8, &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
            shift: NormalNoise::new(s(3), -3, &[1.0, 1.0, 1.0, 0.0]),
        }
    }

    /// Temperature and vegetation at block column `(x, z)`, as Java's
    /// `shiftedNoise2d(shiftX, shiftZ, 0.25, noise)`: `shift_a` samples the
    /// offset noise at (x/4, 0, z/4) and `shift_b` at (z/4, x/4, 0), each × 4.
    pub fn climate(&self, x: i32, z: i32) -> (f32, f32) {
        let (qx, qz) = (x as f64 * 0.25, z as f64 * 0.25);
        let sx = self.shift.sample(qx, 0.0, qz) * 4.0;
        let sz = self.shift.sample(qz, qx, 0.0) * 4.0;
        let (px, pz) = (qx + sx, qz + sz);
        (self.temperature.sample(px, 0.0, pz) as f32, self.vegetation.sample(px, 0.0, pz) as f32)
    }

    /// The biome of quart column `(qx, qz)` (4×4 blocks), sampled at its
    /// corner block like Java's `Climate.Sampler`.
    pub fn quart(&self, qx: i32, qz: i32) -> NetherBiome {
        let (t, h) = self.climate(qx << 2, qz << 2);
        NetherBiome::select(t, h)
    }

    /// The biome of the block column `(x, z)`.
    pub fn biome(&self, x: i32, z: i32) -> NetherBiome {
        self.quart(x >> 2, z >> 2)
    }

    /// Biomes of the quart grid covering `[x0, x0 + 4 * N)` × `[z0, z0 + 4 * N)`
    /// (`x0`, `z0` quart-aligned), indexed `[qz][qx]`.
    pub fn quarts<const N: usize>(&self, x0: i32, z0: i32) -> [[NetherBiome; N]; N] {
        std::array::from_fn(|qz| std::array::from_fn(|qx| self.quart((x0 >> 2) + qx as i32, (z0 >> 2) + qz as i32)))
    }

    /// Nearest column of `target`, searching rings of 32-block steps like
    /// Java's `/locate biome` horizontal step. The result keeps `origin.y`.
    pub fn nearest(&self, origin: IVec3, target: NetherBiome, max_blocks: i32) -> Option<IVec3> {
        let at = |p: IVec2| (self.biome(p.x, p.y) == target).then_some(IVec3::new(p.x, origin.y, p.y));
        let o = IVec2::new(origin.x, origin.z);
        if let Some(p) = at(o) {
            return Some(p);
        }
        for ring in 1..=(max_blocks / 32).max(1) {
            let mut best: Option<(i64, IVec3)> = None;
            for dz in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let p = o + IVec2::new(dx, dz) * 32;
                    if let Some(found) = at(p) {
                        let d = (p - o).as_i64vec2().length_squared();
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, found));
                        }
                    }
                }
            }
            if let Some((_, p)) = best {
                return Some(p);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_points_select_their_own_biome() {
        for b in NetherBiome::ALL {
            let (t, h, _) = b.point();
            assert_eq!(NetherBiome::select(t, h), b, "{b:?}");
            assert_eq!(NetherBiome::from_name(b.name()), Some(b));
            assert_eq!(NetherBiome::from_name(&format!("minecraft:{}", b.name())), Some(b));
        }
        // Java fitness: warped's 0.375 offset loses to wastes until vegetation passes ~0.39.
        assert_eq!(NetherBiome::select(0.0, 0.3), NetherBiome::NetherWastes);
        assert_eq!(NetherBiome::select(0.0, 0.45), NetherBiome::WarpedForest);
        assert_eq!(NetherBiome::select(0.25, 0.0), NetherBiome::CrimsonForest);
        // Basalt's 0.175 offset moves its border to t = -0.28.
        assert_eq!(NetherBiome::select(-0.27, 0.0), NetherBiome::NetherWastes);
        assert_eq!(NetherBiome::select(-0.29, 0.0), NetherBiome::BasaltDeltas);
        assert_eq!(NetherBiome::select(0.0, -0.26), NetherBiome::SoulSandValley);
        assert_eq!(NetherBiome::from_name("plains"), None);
    }

    #[test]
    fn lookup_is_deterministic_and_seed_dependent() {
        let (a, b, c) = (NetherBiomeSource::new(42), NetherBiomeSource::new(42), NetherBiomeSource::new(43));
        let mut differs = 0;
        for i in 0..400 {
            let (x, z) = (i * 97 - 20_000, i * 61 - 9_000);
            assert_eq!(a.climate(x, z), b.climate(x, z));
            assert_eq!(a.biome(x, z), b.biome(x, z));
            differs += (a.biome(x, z) != c.biome(x, z)) as u32;
        }
        assert!(differs > 40, "seeds should change the layout: {differs}");
        // Whole quarts share one biome; far coordinates stay finite and valid.
        assert_eq!(a.biome(13, -7), a.biome(12, -8));
        assert_eq!(a.biome(15, -5), a.biome(12, -8));
        let (t, h) = a.climate(29_999_000, -29_999_000);
        assert!(t.is_finite() && h.is_finite());
        let grid = a.quarts::<8>(32, -64);
        assert_eq!(grid[3][5], a.biome(32 + 5 * 4, -64 + 3 * 4));
    }

    #[test]
    fn every_biome_appears_in_java_like_proportions_and_region_sizes() {
        let src = NetherBiomeSource::new(12345);
        let mut counts = [0usize; 5];
        let mut changes = 0usize;
        // Java's temperature noise starts at octave -10 on a quarter-block
        // scale (4096-block wavelength), so sample a wide area.
        let n = 400;
        let step = 64;
        for zi in 0..n {
            let mut prev = None;
            for xi in 0..n {
                let b = src.biome(xi * step - 12_800, zi * step - 12_800);
                counts[b as usize] += 1;
                changes += (prev.is_some_and(|p| p != b)) as usize;
                prev = Some(b);
            }
        }
        let total = (n * n) as usize;
        for (b, &c) in NetherBiome::ALL.iter().zip(&counts) {
            let share = c as f64 / total as f64;
            assert!((0.02..0.6).contains(&share), "{b:?} covers {share:.3}: {counts:?}");
        }
        // Wastes are the most common; warped forests (offset 0.375) the rarest.
        assert_eq!(counts.iter().max(), Some(&counts[NetherBiome::NetherWastes as usize]));
        assert_eq!(counts.iter().min(), Some(&counts[NetherBiome::WarpedForest as usize]));
        // Mean run length along x: regions are hundreds of blocks to a few kilometres wide.
        let run = (n * (n - 1)) as f64 * step as f64 / changes.max(1) as f64;
        assert!((200.0..3000.0).contains(&run), "mean biome run {run:.0} blocks");
    }

    #[test]
    fn spawn_lists_follow_java() {
        let chance = |b: NetherBiome, m| b.monster_spawn(m).map(|(_, c)| c);
        assert_eq!(chance(NetherBiome::NetherWastes, NetherMob::ZombifiedPiglin), Some(1.0));
        assert_eq!(chance(NetherBiome::NetherWastes, NetherMob::Ghast), Some(0.5));
        assert_eq!(chance(NetherBiome::NetherWastes, NetherMob::MagmaCube), Some(0.02));
        assert_eq!(chance(NetherBiome::SoulSandValley, NetherMob::Skeleton), Some(0.4));
        assert_eq!(chance(NetherBiome::SoulSandValley, NetherMob::ZombifiedPiglin), None);
        assert_eq!(chance(NetherBiome::BasaltDeltas, NetherMob::MagmaCube), Some(1.0));
        assert_eq!(NetherBiome::BasaltDeltas.monster_spawn(NetherMob::MagmaCube).unwrap().0.group, (2, 5));
        // Hoglins dominate the crimson forest's list.
        assert_eq!(chance(NetherBiome::CrimsonForest, NetherMob::ZombifiedPiglin), Some(1.0 / 9.0));
        assert_eq!(chance(NetherBiome::WarpedForest, NetherMob::Enderman), Some(1.0));
        for b in NetherBiome::ALL {
            assert_eq!(b.creatures(), &[spawn(NetherMob::Strider, 60, 1, 2)]);
        }
    }

    #[test]
    fn fog_blends_smoothly_between_biome_colours() {
        let mut fog = FogSampler::default();
        // Crimson for negative x, warped for positive: a soft blend across x = 0.
        let split = |qx: i32, _: i32| if qx < 0 { NetherBiome::CrimsonForest } else { NetherBiome::WarpedForest };
        let far_left = fog.sample(DVec3::new(-100.0, 64.0, 0.0), split);
        assert_eq!(far_left.map(|c| (c * 255.0).round() as u32), [0x33, 0x03, 0x03]);
        let far_right = fog.sample(DVec3::new(100.0, 64.0, 0.0), split);
        assert_eq!(far_right.map(|c| (c * 255.0).round() as u32), [0x1A, 0x05, 0x1A]);
        let mut previous = fog.sample(DVec3::new(-12.0, 64.0, 0.0), split);
        for step in 1..=96 {
            let x = -12.0 + step as f64 * 0.25;
            let c = fog.sample(DVec3::new(x, 64.0, 0.0), split);
            assert!(c[0] <= previous[0] + 1e-6, "red fades monotonically at {x}");
            assert!((c[0] - previous[0]).abs() < 0.01, "no jumps at {x}");
            previous = c;
        }
        assert!((previous[0] - far_right[0]).abs() < 1e-6);
    }

    #[test]
    fn locate_finds_the_nearest_column() {
        let src = NetherBiomeSource::new(7);
        let origin = IVec3::new(100, 70, -40);
        for b in NetherBiome::ALL {
            let p = src.nearest(origin, b, 6400).expect("every biome within 6400 blocks");
            assert_eq!(src.biome(p.x, p.z), b);
            assert_eq!(p.y, 70);
        }
        let here = src.biome(origin.x, origin.z);
        assert_eq!(src.nearest(origin, here, 64), Some(origin));
    }
}
