//! Overworld caves in the style of Java 1.18: noise caves, ravines and
//! aquifers.
//!
//! - **Cheese caves** are big open caverns where a low-frequency noise is
//!   high, larger deeper down and kept under a roof near the surface.
//! - **Spaghetti caves** are winding tunnels where two noises are both
//!   near zero; they can break through the surface as entrances.
//! - **Noodle caves** are the same, thinner and switched on and off by a
//!   third noise.
//! - **Ravines** (Java's canyon carver) are rare, deep, narrow cuts.
//! - **Aquifers** decide what fills a carved cell: the world is divided
//!   into cells, each with its own water (or, deep down, lava) level or
//!   none, and walls of stone are kept where neighbouring aquifers
//!   disagree. Near the surface, below sea level, caves flood with the
//!   sea; below y = -54 everything open fills with lava.
//!
//! Noise is sampled on a coarse grid and interpolated (like Java's noise
//! cells): fine values every 4 blocks, cavern values every 8.

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I};
use super::climate::SEA_LEVEL;
use super::noise::{Perlin, hash3};

/// Java's lava level: open space below it fills with lava.
pub const LAVA_LEVEL: i32 = -54;
/// Nothing is carved below this (the bedrock layers start at -64).
pub const CARVE_FLOOR: i32 = -59;

const STEP: usize = 4;
const GRID: usize = CHUNK_SIZE / STEP + 1;
const COARSE: usize = 8;
const CGRID: usize = CHUNK_SIZE / COARSE + 1;

/// Fine values: spaghetti a, spaghetti b, noodle a, noodle b.
type Fine = [f32; 4];
/// Coarse values: cheese, spaghetti rarity, noodle switch.
type Coarse = [f32; 3];

pub struct Caves {
    seed: u64,
    cheese: Perlin,
    spag_a: Perlin,
    spag_b: Perlin,
    rarity: Perlin,
    noodle_a: Perlin,
    noodle_b: Perlin,
    noodle_on: Perlin,
    flood: Perlin,
    spread: Perlin,
    lava: Perlin,
}

/// Interpolated cave noise for one chunk.
pub struct Field {
    fine: Box<[Fine; GRID * GRID * GRID]>,
    coarse: Box<[Coarse; CGRID * CGRID * CGRID]>,
}

/// One column's noise values at every grid height of a chunk.
pub struct Profile {
    fine: [Fine; GRID],
    coarse: [Coarse; CGRID],
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_n<const N: usize>(a: &[f32; N], b: &[f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

impl Caves {
    pub fn new(seed: u64) -> Self {
        let p = |salt: u64| Perlin::new(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        Self {
            seed,
            cheese: p(41),
            spag_a: p(42),
            spag_b: p(43),
            rarity: p(44),
            noodle_a: p(45),
            noodle_b: p(46),
            noodle_on: p(47),
            flood: p(48),
            spread: p(49),
            lava: p(50),
        }
    }

    fn fine_at(&self, x: f32, y: f32, z: f32) -> Fine {
        [
            self.spag_a.noise3(x / 54.0, y / 34.0, z / 54.0),
            self.spag_b.noise3(x / 54.0, y / 34.0, z / 54.0),
            self.noodle_a.noise3(x / 24.0, y / 24.0, z / 24.0),
            self.noodle_b.noise3(x / 24.0, y / 24.0, z / 24.0),
        ]
    }

    fn coarse_at(&self, x: f32, y: f32, z: f32) -> Coarse {
        let cheese =
            self.cheese.noise3(x / 90.0, y / 55.0, z / 90.0) + 0.5 * self.cheese.noise3(x / 38.0, y / 26.0, z / 38.0);
        [
            cheese / 1.5,
            self.rarity.noise3(x / 160.0, y / 110.0, z / 160.0),
            self.noodle_on.noise3(x / 100.0, y / 70.0, z / 100.0),
        ]
    }

    /// Samples the cave noise for the chunk whose minimum corner is `base`.
    pub fn field(&self, base: IVec3) -> Field {
        let mut fine = Box::new([[0.0; 4]; GRID * GRID * GRID]);
        for gy in 0..GRID {
            for gz in 0..GRID {
                for gx in 0..GRID {
                    let p = base + IVec3::new(gx as i32, gy as i32, gz as i32) * STEP as i32;
                    fine[gx + gz * GRID + gy * GRID * GRID] = self.fine_at(p.x as f32, p.y as f32, p.z as f32);
                }
            }
        }
        let mut coarse = Box::new([[0.0; 3]; CGRID * CGRID * CGRID]);
        for gy in 0..CGRID {
            for gz in 0..CGRID {
                for gx in 0..CGRID {
                    let p = base + IVec3::new(gx as i32, gy as i32, gz as i32) * COARSE as i32;
                    coarse[gx + gz * CGRID + gy * CGRID * CGRID] = self.coarse_at(p.x as f32, p.y as f32, p.z as f32);
                }
            }
        }
        Field { fine, coarse }
    }

    /// Whether a cell carves open: `v` holds the interpolated noises,
    /// `depth` is how far under the column's surface the cell is.
    #[inline]
    fn carves(fine: &Fine, coarse: &Coarse, y: i32, depth: i32) -> bool {
        if y < CARVE_FLOOR || depth < 0 {
            return false;
        }
        // Caverns: bigger with depth, sealed under at least 10 blocks of rock.
        let deep = ((24 - y) as f32 / 90.0).clamp(0.0, 1.0);
        let roof = ((14 - depth) as f32 * 0.06).max(0.0);
        let cheese = coarse[0] > 0.31 - deep * 0.12 + roof;
        // Tunnels: thicker where the rarity noise is high; they may open
        // onto the surface.
        let rarity = 1.0 + 0.9 * (coarse[1] * 1.6).clamp(-0.5, 0.5);
        let tunnel = fine[0] * fine[0] + fine[1] * fine[1] < 0.0038 * rarity;
        // Noodles: thin passages, switched on in patches, not at the surface.
        let noodle = depth > 3 && coarse[2] > 0.05 && fine[2] * fine[2] + fine[3] * fine[3] < 0.0011;
        cheese || tunnel || noodle
    }

    /// The column profile of interpolated noise at local `(x, z)`.
    pub fn profile(field: &Field, x: usize, z: usize) -> Profile {
        let (gx, tx) = (x / STEP, (x % STEP) as f32 / STEP as f32);
        let (gz, tz) = (z / STEP, (z % STEP) as f32 / STEP as f32);
        let f = |gy: usize, dx: usize, dz: usize| &field.fine[(gx + dx) + (gz + dz) * GRID + gy * GRID * GRID];
        let fine = std::array::from_fn(|gy| {
            let a = lerp_n(f(gy, 0, 0), f(gy, 1, 0), tx);
            let b = lerp_n(f(gy, 0, 1), f(gy, 1, 1), tx);
            lerp_n(&a, &b, tz)
        });
        let (cx, ctx) = (x / COARSE, (x % COARSE) as f32 / COARSE as f32);
        let (cz, ctz) = (z / COARSE, (z % COARSE) as f32 / COARSE as f32);
        let c = |gy: usize, dx: usize, dz: usize| &field.coarse[(cx + dx) + (cz + dz) * CGRID + gy * CGRID * CGRID];
        let coarse = std::array::from_fn(|gy| {
            let a = lerp_n(c(gy, 0, 0), c(gy, 1, 0), ctx);
            let b = lerp_n(c(gy, 0, 1), c(gy, 1, 1), ctx);
            lerp_n(&a, &b, ctz)
        });
        Profile { fine, coarse }
    }

    /// Whether local height `y` (world height `wy`) of a profiled column carves.
    #[inline]
    pub fn carved(profile: &Profile, y: usize, wy: i32, depth: i32) -> bool {
        let (gy, ty) = (y / STEP, (y % STEP) as f32 / STEP as f32);
        let fine = lerp_n(&profile.fine[gy], &profile.fine[gy + 1], ty);
        let (cy, cty) = (y / COARSE, (y % COARSE) as f32 / COARSE as f32);
        let coarse = lerp_n(&profile.coarse[cy], &profile.coarse[cy + 1], cty);
        Self::carves(&fine, &coarse, wy, depth)
    }

    /// [`Caves::carved`] at one world point, with grid nodes cached in
    /// `nodes` (for features that must agree with any chunk's carving).
    pub fn carved_at(&self, p: IVec3, depth: i32, nodes: &mut FxHashMap<IVec3, [f32; 7]>) -> bool {
        if p.y < CARVE_FLOOR || depth < 0 {
            return false;
        }
        let mut node = |q: IVec3| -> [f32; 7] {
            *nodes.entry(q).or_insert_with(|| {
                let (x, y, z) = (q.x as f32, q.y as f32, q.z as f32);
                let f = self.fine_at(x, y, z);
                let c = self.coarse_at(x, y, z);
                [f[0], f[1], f[2], f[3], c[0], c[1], c[2]]
            })
        };
        let trilinear = |step: i32, node: &mut dyn FnMut(IVec3) -> [f32; 7]| {
            let lo = IVec3::new(p.x.div_euclid(step), p.y.div_euclid(step), p.z.div_euclid(step)) * step;
            let t = (p - lo).as_vec3() / step as f32;
            let at = |n: &mut dyn FnMut(IVec3) -> [f32; 7], dx: i32, dz: i32| {
                let lo_y = n(lo + IVec3::new(dx, 0, dz) * step);
                let hi_y = n(lo + IVec3::new(dx, 1, dz) * step);
                lerp_n(&lo_y, &hi_y, t.y)
            };
            let a = lerp_n(&at(node, 0, 0), &at(node, 1, 0), t.x);
            let b = lerp_n(&at(node, 0, 1), &at(node, 1, 1), t.x);
            lerp_n(&a, &b, t.z)
        };
        let fine = trilinear(STEP as i32, &mut node);
        let coarse = trilinear(COARSE as i32, &mut node);
        Self::carves(&[fine[0], fine[1], fine[2], fine[3]], &[coarse[4], coarse[5], coarse[6]], p.y, depth)
    }

    /// Ravine cells in the chunk at `base`, as a mask indexed like chunk blocks.
    pub fn ravines(&self, base: IVec3) -> Option<Box<[bool]>> {
        let mut mask: Option<Box<[bool]>> = None;
        let chunk16 = IVec3::new(base.x.div_euclid(16), 0, base.z.div_euclid(16));
        const REACH: i32 = 8;
        for cz in chunk16.z - REACH..=chunk16.z + REACH + 1 {
            for cx in chunk16.x - REACH..=chunk16.x + REACH + 1 {
                let h = hash3(cx, 7, cz, self.seed ^ 0xCA_7E);
                // Java's canyon carver: one 16x16 chunk in fifty.
                if !h.is_multiple_of(50) {
                    continue;
                }
                carve_ravine(h, cx, cz, base, &mut mask);
            }
        }
        mask
    }

    /// The fluid status of the aquifer cell `cell`, given the surface
    /// height at its centre column.
    fn status(&self, cell: IVec3, surface: &mut dyn FnMut(i32, i32) -> i32) -> (IVec3, Status) {
        let h = hash3(cell.x, cell.y, cell.z, self.seed ^ 0xA9_F1);
        let centre = IVec3::new(
            cell.x * 16 + (h % 10) as i32,
            cell.y * 12 + ((h >> 8) % 9) as i32,
            cell.z * 16 + ((h >> 16) % 10) as i32,
        );
        let top = surface(centre.x, centre.z);
        let (x, y, z) = (centre.x as f32, centre.y as f32, centre.z as f32);
        let status = if centre.y > top - 10 {
            Status::Global
        } else {
            let flood =
                self.flood.noise3(x / 90.0, y / 60.0, z / 90.0) + 0.4 * self.flood.noise3(x / 37.0, y / 25.0, z / 37.0);
            if flood > 0.52 {
                Status::Global
            } else if flood > -0.12 {
                let spread = self.spread.noise3(x / 40.0, y / 28.0, z / 40.0);
                let level = (centre.y - 3 + (spread * 14.0) as i32).min(top - 6);
                let lava =
                    level < LAVA_LEVEL || (level <= -10 && self.lava.noise3(x / 64.0, y / 40.0, z / 64.0).abs() > 0.32);
                Status::Local { level, lava }
            } else {
                Status::Dry
            }
        };
        (centre, status)
    }
}

/// What an aquifer cell holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// The sea: water below sea level, lava below the lava level.
    Global,
    /// Its own level: fluid below `level`.
    Local {
        level: i32,
        lava: bool,
    },
    Dry,
}

impl Status {
    fn fluid_at(self, y: i32) -> Block {
        match self {
            Status::Global if y < LAVA_LEVEL => Block::LAVA,
            Status::Global if y < SEA_LEVEL => Block::WATER,
            Status::Local { level, lava } if y < level => {
                if lava {
                    Block::LAVA
                } else {
                    Block::WATER
                }
            }
            _ => Block::AIR,
        }
    }
}

/// Resolves what fills carved cells, caching aquifer cells.
pub struct Aquifer<'a, F: FnMut(i32, i32) -> i32> {
    caves: &'a Caves,
    surface: F,
    cells: FxHashMap<IVec3, (IVec3, Status)>,
}

impl<'a, F: FnMut(i32, i32) -> i32> Aquifer<'a, F> {
    pub fn new(caves: &'a Caves, surface: F) -> Self {
        Self { caves, surface, cells: FxHashMap::default() }
    }

    fn cell(&mut self, c: IVec3) -> (IVec3, Status) {
        if let Some(&found) = self.cells.get(&c) {
            return found;
        }
        let found = self.caves.status(c, &mut self.surface);
        self.cells.insert(c, found);
        found
    }

    /// What a carved cell at `p` becomes: air, water or lava, or `None`
    /// for a barrier of stone between two aquifers that disagree.
    pub fn fill(&mut self, p: IVec3) -> Option<Block> {
        // Java: open space at the bottom of the world is a sea of lava.
        if p.y < LAVA_LEVEL {
            return Some(Block::LAVA);
        }
        let (x0, y0, z0) = ((p.x - 5).div_euclid(16), (p.y + 1).div_euclid(12), (p.z - 5).div_euclid(16));
        let mut best = [(i32::MAX, Status::Dry); 2];
        for dy in -1..=1 {
            for dz in 0..=1 {
                for dx in 0..=1 {
                    let (centre, status) = self.cell(IVec3::new(x0 + dx, y0 + dy, z0 + dz));
                    let d = (centre - p).length_squared();
                    if d < best[0].0 {
                        best[1] = best[0];
                        best[0] = (d, status);
                    } else if d < best[1].0 {
                        best[1] = (d, status);
                    }
                }
            }
        }
        let a = best[0].1.fluid_at(p.y);
        let b = best[1].1.fluid_at(p.y);
        if a != b && (best[1].0 as f32).sqrt() - (best[0].0 as f32).sqrt() < 2.2 {
            return None;
        }
        Some(a)
    }
}

/// Carves one ravine that starts in 16x16 chunk `(cx, cz)` into `mask`
/// where it crosses the chunk at `base`.
fn carve_ravine(h: u64, cx: i32, cz: i32, base: IVec3, mask: &mut Option<Box<[bool]>>) {
    let mut rng = h;
    let mut next = || {
        rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (rng >> 40) as f32 / (1u64 << 24) as f32
    };
    let mut pos =
        glam::Vec3::new((cx * 16) as f32 + next() * 16.0, 10.0 + next() * 57.0, (cz * 16) as f32 + next() * 16.0);
    let mut yaw = next() * std::f32::consts::TAU;
    let mut pitch = (next() - 0.5) * 0.25;
    let width = (next() * 2.0 + next()) * 2.0;
    let length = 84 + (next() * 28.0) as i32;
    let (mut yaw_v, mut pitch_v) = (0.0f32, 0.0f32);
    let lo = base.as_vec3();
    let hi = lo + glam::Vec3::splat(CHUNK_SIZE as f32);
    for i in 0..length {
        let radius = 1.5 + (i as f32 * std::f32::consts::PI / length as f32).sin() * width;
        let tall = radius * 3.0;
        pos += glam::Vec3::new(pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin());
        pitch *= 0.7;
        pitch += pitch_v * 0.05;
        yaw += yaw_v * 0.05;
        pitch_v = pitch_v * 0.8 + (next() - next()) * next() * 2.0;
        yaw_v = yaw_v * 0.5 + (next() - next()) * next() * 4.0;
        if pos.x + radius < lo.x || pos.x - radius > hi.x || pos.z + radius < lo.z || pos.z - radius > hi.z {
            continue;
        }
        if pos.y + tall < lo.y || pos.y - tall > hi.y {
            continue;
        }
        let m = mask.get_or_insert_with(|| vec![false; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into_boxed_slice());
        let min = (pos - glam::Vec3::new(radius, tall, radius) - lo).floor().max(glam::Vec3::ZERO).as_ivec3();
        let max = (pos + glam::Vec3::new(radius, tall, radius) - lo)
            .ceil()
            .min(glam::Vec3::splat(CHUNK_SIZE_I as f32 - 1.0))
            .as_ivec3();
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let d = (glam::Vec3::new(x as f32, y as f32, z as f32) + lo + 0.5 - pos)
                        / glam::Vec3::new(radius, tall, radius);
                    // Java flattens the floor of canyons a little.
                    if d.x * d.x + d.z * d.z + d.y * d.y < 1.0 && d.y > -0.7 {
                        m[super::chunk::index(x as usize, y as usize, z as usize)] = true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_samples_agree_with_chunk_fields() {
        let caves = Caves::new(77);
        let base = IVec3::new(64, -32, -96);
        let field = caves.field(base);
        let mut nodes = FxHashMap::default();
        let mut agree = 0;
        let mut total = 0;
        for z in (0..CHUNK_SIZE).step_by(3) {
            for x in (0..CHUNK_SIZE).step_by(3) {
                let profile = Caves::profile(&field, x, z);
                for y in (0..CHUNK_SIZE).step_by(2) {
                    let p = base + IVec3::new(x as i32, y as i32, z as i32);
                    let a = Caves::carved(&profile, y, p.y, 40);
                    let b = caves.carved_at(p, 40, &mut nodes);
                    total += 1;
                    agree += (a == b) as i32;
                }
            }
        }
        assert!(agree as f32 / total as f32 > 0.99, "{agree} of {total}");
    }

    #[test]
    fn caves_open_up_a_fair_share_of_the_underground() {
        let caves = Caves::new(5);
        let mut open = 0;
        let mut total = 0;
        for cx in -3..3 {
            for cz in -3..3 {
                let base = IVec3::new(cx * 32, -32, cz * 32);
                let field = caves.field(base);
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        let profile = Caves::profile(&field, x, z);
                        for y in 0..CHUNK_SIZE {
                            total += 1;
                            open += Caves::carved(&profile, y, base.y + y as i32, 80) as i32;
                        }
                    }
                }
            }
        }
        let share = open as f32 / total as f32;
        assert!((0.04..0.25).contains(&share), "{share}");
    }

    #[test]
    fn aquifers_flood_deep_space_with_lava_and_shallow_with_the_sea() {
        let caves = Caves::new(9);
        let mut aquifer = Aquifer::new(&caves, |_, _| 70);
        // Just under the surface, below sea level: the sea.
        assert_eq!(aquifer.fill(IVec3::new(0, 62, 0)).unwrap_or(Block::WATER), Block::WATER);
        let mut lava = 0;
        for x in 0..40 {
            if aquifer.fill(IVec3::new(x * 7, -58, x * 3)) == Some(Block::LAVA) {
                lava += 1;
            }
        }
        assert!(lava > 10, "{lava}");
    }

    #[test]
    fn ravines_are_rare_and_deterministic() {
        let caves = Caves::new(3);
        let mut found = 0;
        for cx in -10..10 {
            for cz in -10..10 {
                let base = IVec3::new(cx * 32, 0, cz * 32);
                let a = caves.ravines(base);
                assert_eq!(a.is_some(), caves.ravines(base).is_some());
                found += a.is_some() as i32;
            }
        }
        assert!(found > 0 && found < 200, "{found}");
    }
}
