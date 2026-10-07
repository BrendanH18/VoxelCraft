//! Nether terrain: netherrack caverns between a bedrock floor and roof,
//! a lava sea below y = 31, soul sand and gravel shores, quartz ore,
//! ancient debris (Java `scattered_ore`), and glowstone hanging from the ceilings.
//!
//! Like the overworld, every chunk is a pure function of the seed. Solidity
//! comes from two octaves of 3D noise, biased solid towards the floor and the
//! roof, sampled on a coarse grid and interpolated (like overworld caves).
//! Fortresses (see `world::fortress`) are painted over the terrain.

use std::cell::RefCell;

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, index};
use super::fortress::Fortresses;
use super::noise::{Perlin, hash_f, hash3};
use super::structure::Rng;

/// The bedrock roof; nothing generates above it.
pub const ROOF: i32 = 127;
/// Open space at or below this height fills with lava.
pub const LAVA_SEA: i32 = 31;
const STEP: usize = 4;
const GRID: usize = CHUNK_SIZE / STEP + 1;
/// Extra samples above the chunk so ceilings just above it are known.
const GRID_Y: usize = GRID + 1;
/// Java horizontal decoration cell (`in_square` placement).
const JAVA_CELL: i32 = 16;
/// Maximum offset for our size-3 scattered feature (candidate index 2).
const DEBRIS_SPREAD: i32 = 2;
const SALT_DEBRIS_LARGE: u64 = 0xDE_B1_01;
const SALT_DEBRIS_SMALL: u64 = 0xDE_B1_02;

#[derive(Default)]
struct ExposureCache {
    densities: FxHashMap<IVec3, f32>,
    columns: FxHashMap<IVec3, [Block; CHUNK_SIZE]>,
}

pub struct NetherGen {
    seed: u64,
    shape: Perlin,
    detail: Perlin,
    patches: Perlin,
    glow: Perlin,
    pub fortresses: Fortresses,
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Trilinear interpolation of corner densities `c[dx][dy][dz]` at the
/// fractions `t` (x, y, z) of a grid cell. Chunks and single columns share
/// it so both agree to the bit.
#[inline]
fn interpolate(c: [[[f32; 2]; 2]; 2], t: [f32; 3]) -> f32 {
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(c[0][0][0], c[1][0][0], t[0]);
    let x10 = lerp(c[0][1][0], c[1][1][0], t[0]);
    let x01 = lerp(c[0][0][1], c[1][0][1], t[0]);
    let x11 = lerp(c[0][1][1], c[1][1][1], t[0]);
    lerp(lerp(x00, x10, t[1]), lerp(x01, x11, t[1]), t[2])
}

impl NetherGen {
    pub fn new(seed: u64) -> Self {
        let p = |salt: u64| Perlin::new(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        Self {
            seed: seed ^ 0x4E45_5448,
            shape: p(21),
            detail: p(22),
            patches: p(23),
            glow: p(24),
            fortresses: Fortresses::new(seed),
        }
    }

    /// Bedrock floor and roof, thinning out over the four layers next to them.
    fn bedrock(&self, x: i32, y: i32, z: i32) -> bool {
        let h = |salt: u64| hash_f(x, y, z, self.seed ^ salt);
        y == 0
            || y == ROOF
            || (y <= 4 && h(1) < (5 - y) as f32 / 5.0)
            || (y >= ROOF - 4 && h(2) < (y - (ROOF - 5)) as f32 / 5.0)
    }

    /// Whether the terrain leaves the column at `x, z` open (air or lava)
    /// from `top` down to `bottom`, exactly as chunks generate it. Grid
    /// densities are memoized in `cache`, which neighbouring columns share.
    pub fn column_open(&self, x: i32, z: i32, top: i32, bottom: i32, cache: &mut FxHashMap<IVec3, f32>) -> bool {
        let s = STEP as i32;
        let (gx, gz) = (x.div_euclid(s) * s, z.div_euclid(s) * s);
        let (tx, tz) = ((x - gx) as f32 / STEP as f32, (z - gz) as f32 / STEP as f32);
        let mut level = i32::MIN;
        let mut c = [[[0f32; 2]; 2]; 2];
        for y in (bottom..=top).rev() {
            if y <= 0 || y >= ROOF || self.bedrock(x, y, z) {
                return false;
            }
            let gy = y.div_euclid(s) * s;
            if gy != level {
                level = gy;
                for (dx, plane) in c.iter_mut().enumerate() {
                    for (dy, row) in plane.iter_mut().enumerate() {
                        for (dz, d) in row.iter_mut().enumerate() {
                            let p = IVec3::new(gx + dx as i32 * s, gy + dy as i32 * s, gz + dz as i32 * s);
                            *d = *cache.entry(p).or_insert_with(|| self.density(p.x, p.y, p.z));
                        }
                    }
                }
            }
            if interpolate(c, [tx, (y - gy) as f32 / STEP as f32, tz]) > 0.0 {
                return false;
            }
        }
        true
    }

    /// Above zero is netherrack, below is open space.
    fn density(&self, x: i32, y: i32, z: i32) -> f32 {
        let (fx, fy, fz) = (x as f32, y as f32, z as f32);
        let n = self.shape.noise3(fx / 52.0, fy / 30.0, fz / 52.0) * 0.7
            + self.detail.noise3(fx / 17.0, fy / 13.0, fz / 17.0) * 0.3;
        // Solid towards the floor and the roof, mostly open in between.
        let floor = smoothstep(33.0, 4.0, fy) * 1.4;
        let roof = smoothstep(92.0, 124.0, fy) * 1.5;
        n + floor + roof - 0.12
    }

    #[inline]
    fn contains(base: IVec3, wx: i32, wy: i32, wz: i32) -> bool {
        (base.x..base.x + CHUNK_SIZE_I).contains(&wx)
            && (base.y..base.y + CHUNK_SIZE_I).contains(&wy)
            && (base.z..base.z + CHUNK_SIZE_I).contains(&wz)
    }

    #[inline]
    fn local(base: IVec3, wx: i32, wy: i32, wz: i32) -> (usize, usize, usize) {
        ((wx - base.x) as usize, (wy - base.y) as usize, (wz - base.z) as usize)
    }

    fn interpolated_density(&self, wx: i32, wy: i32, wz: i32, cache: &mut FxHashMap<IVec3, f32>) -> f32 {
        let s = STEP as i32;
        let (gx, gy, gz) = (wx.div_euclid(s) * s, wy.div_euclid(s) * s, wz.div_euclid(s) * s);
        let t = [(wx - gx) as f32 / STEP as f32, (wy - gy) as f32 / STEP as f32, (wz - gz) as f32 / STEP as f32];
        let mut c = [[[0f32; 2]; 2]; 2];
        for (dx, plane) in c.iter_mut().enumerate() {
            for (dy, row) in plane.iter_mut().enumerate() {
                for (dz, d) in row.iter_mut().enumerate() {
                    let p = IVec3::new(gx + dx as i32 * s, gy + dy as i32 * s, gz + dz as i32 * s);
                    *d = *cache.entry(p).or_insert_with(|| self.density(p.x, p.y, p.z));
                }
            }
        }
        interpolate(c, t)
    }

    /// Air/solid/fluid terrain classification used by exposure and pillar stops.
    fn exposure_terrain(&self, p: IVec3, cache: &mut FxHashMap<IVec3, f32>) -> Block {
        let IVec3 { x, y, z } = p;
        if !(0..=ROOF).contains(&y) {
            return Block::AIR;
        }
        if self.bedrock(x, y, z) {
            return Block::BEDROCK;
        }
        if self.interpolated_density(x, y, z, cache) > 0.0 {
            return Block::NETHERRACK;
        }
        if y <= LAVA_SEA {
            return Block::LAVA;
        }
        if y > 60
            && self.glow.noise3(x as f32 / 6.0, y as f32 / 6.0, z as f32 / 6.0) > 0.5
            && (1..=3).any(|k| self.interpolated_density(x, y + k, z, cache) > 0.0)
        {
            return Block::GLOWSTONE;
        }
        Block::AIR
    }

    /// Tests actual terrain and fortress carving across chunk boundaries.
    fn air_for_exposure(
        &self,
        p: IVec3,
        base: IVec3,
        blocks: &[Block; CHUNK_VOLUME],
        cache: &mut ExposureCache,
    ) -> bool {
        if Self::contains(base, p.x, p.y, p.z) {
            let (x, y, z) = Self::local(base, p.x, p.y, p.z);
            return blocks[index(x, y, z)] == Block::AIR;
        }
        // Native cavities below the sea are lava, and fortress rooms stay above it.
        if p.y <= LAVA_SEA {
            return false;
        }
        let key = IVec3::new(p.x, p.y.div_euclid(CHUNK_SIZE_I) * CHUNK_SIZE_I, p.z);
        let column = cache.columns.entry(key).or_insert_with(|| {
            let terrain =
                std::array::from_fn(|y| self.exposure_terrain(key + IVec3::Y * y as i32, &mut cache.densities));
            let densities = RefCell::new(&mut cache.densities);
            let open = |x, z, top, bottom| self.column_open(x, z, top, bottom, &mut densities.borrow_mut());
            let terrain = self.fortresses.column_at(key.x, key.z, key.y, terrain, &open);
            self.fortresses.bastions.column_at(key, terrain)
        });
        column[(p.y - key.y) as usize] == Block::AIR
    }

    fn adjacent_to_air(
        &self,
        p: IVec3,
        base: IVec3,
        blocks: &[Block; CHUNK_VOLUME],
        cache: &mut ExposureCache,
    ) -> bool {
        [IVec3::X, -IVec3::X, IVec3::Y, -IVec3::Y, IVec3::Z, -IVec3::Z]
            .into_iter()
            .any(|d| self.air_for_exposure(p + d, base, blocks, cache))
    }

    /// Java's scattered-ore count, triangular offsets and height distributions.
    /// Cell seeds use our terrain RNG; they do not reproduce vanilla world seeds.
    fn debris_candidates(&self, cx: i32, cz: i32, large: bool) -> impl Iterator<Item = IVec3> {
        let salt = if large { SALT_DEBRIS_LARGE } else { SALT_DEBRIS_SMALL };
        let mut rng = Rng(hash3(cx, cz, 0, self.seed ^ salt));
        let x = cx * JAVA_CELL + rng.range(0, 15) as i32;
        let z = cz * JAVA_CELL + rng.range(0, 15) as i32;
        let y = if large { 8 + rng.range(0, 8) as i32 + rng.range(0, 8) as i32 } else { rng.range(8, 119) as i32 };
        let origin = IVec3::new(x, y, z);
        let count = rng.range(0, if large { 3 } else { 2 });
        (0..count).map(move |i| {
            // Java Math.round(float) rounds ties toward positive infinity.
            // Draw all six floats even for candidate zero (whose spread is zero).
            let mut axis = || {
                let a = (rng.next_u64() >> 40) as f32 / (1u32 << 24) as f32;
                let b = (rng.next_u64() >> 40) as f32 / (1u32 << 24) as f32;
                ((a - b) * i.min(7) as f32 + 0.5).floor() as i32
            };
            origin + IVec3::new(axis(), axis(), axis())
        })
    }

    /// Ancient debris from every Java 16×16 cell that can reach this chunk (seams included).
    fn paint_ancient_debris(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let (x0, x1) = (base.x, base.x + CHUNK_SIZE_I - 1);
        let (z0, z1) = (base.z, base.z + CHUNK_SIZE_I - 1);
        let min_cx = (x0 - DEBRIS_SPREAD).div_euclid(JAVA_CELL);
        let max_cx = (x1 + DEBRIS_SPREAD).div_euclid(JAVA_CELL);
        let min_cz = (z0 - DEBRIS_SPREAD).div_euclid(JAVA_CELL);
        let max_cz = (z1 + DEBRIS_SPREAD).div_euclid(JAVA_CELL);

        let mut exposure_cache = ExposureCache::default();
        for cz in min_cz..=max_cz {
            for cx in min_cx..=max_cx {
                for large in [true, false] {
                    for p in self.debris_candidates(cx, cz, large) {
                        if !Self::contains(base, p.x, p.y, p.z) {
                            continue;
                        }
                        let (x, y, z) = Self::local(base, p.x, p.y, p.z);
                        if blocks[index(x, y, z)] == Block::NETHERRACK
                            && !self.adjacent_to_air(p, base, blocks, &mut exposure_cache)
                        {
                            blocks[index(x, y, z)] = Block::ANCIENT_DEBRIS;
                        }
                    }
                }
            }
        }
    }

    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        let base = cpos * CHUNK_SIZE_I;
        if base.y > ROOF || base.y + CHUNK_SIZE_I <= 0 {
            return ChunkData::Uniform(Block::AIR);
        }
        // Density on a coarse grid, trilinearly interpolated per cell.
        let mut grid = [[[0f32; GRID]; GRID_Y]; GRID];
        for (gx, plane) in grid.iter_mut().enumerate() {
            for (gy, row) in plane.iter_mut().enumerate() {
                for (gz, d) in row.iter_mut().enumerate() {
                    let p = base + IVec3::new(gx as i32, gy as i32, gz as i32) * STEP as i32;
                    *d = self.density(p.x, p.y, p.z);
                }
            }
        }
        let solid = |x: usize, y: usize, z: usize| -> bool {
            let wy = base.y + y as i32;
            if wy <= 0 || wy >= ROOF {
                return true;
            }
            let (gx, gy, gz) = (x / STEP, y / STEP, z / STEP);
            let t = [(x % STEP) as f32 / STEP as f32, (y % STEP) as f32 / STEP as f32, (z % STEP) as f32 / STEP as f32];
            let at = |dx: usize, dy: usize, dz: usize| grid[gx + dx][(gy + dy).min(GRID_Y - 1)][gz + dz];
            let c = [
                [[at(0, 0, 0), at(0, 0, 1)], [at(0, 1, 0), at(0, 1, 1)]],
                [[at(1, 0, 0), at(1, 0, 1)], [at(1, 1, 0), at(1, 1, 1)]],
            ];
            interpolate(c, t) > 0.0
        };

        let mut blocks = ChunkData::new_dense(Block::AIR);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let patch = self.patches.noise2(wx as f32 / 22.0, wz as f32 / 22.0);
                let gravel = self.patches.noise2(wz as f32 / 15.0 + 50.0, wx as f32 / 15.0);
                for y in 0..CHUNK_SIZE {
                    let wy = base.y + y as i32;
                    if wy > ROOF {
                        break;
                    }
                    let h = |salt: u64| hash_f(wx, wy, wz, self.seed ^ salt);
                    let block = if self.bedrock(wx, wy, wz) {
                        Block::BEDROCK
                    } else if solid(x, y, z) {
                        // Depth below the nearest open space above (up to 4).
                        let depth = (1..=4).find(|&k| y + k <= CHUNK_SIZE + STEP && !solid(x, y + k, z)).unwrap_or(5);
                        if depth <= 3 && wy < 90 && patch > 0.32 {
                            Block::SOUL_SAND
                        } else if depth <= 2 && (LAVA_SEA - 4..LAVA_SEA + 6).contains(&wy) && gravel > 0.3 {
                            Block::GRAVEL
                        } else if hash_f(wx >> 1, wy >> 1, wz >> 1, self.seed ^ 3) < 0.035 && h(4) < 0.55 {
                            Block::QUARTZ_ORE
                        } else {
                            Block::NETHERRACK
                        }
                    } else if wy <= LAVA_SEA {
                        Block::LAVA
                    } else if wy > 60
                        && (1..=3).any(|k| solid(x, y + k, z))
                        && self.glow.noise3(wx as f32 / 6.0, wy as f32 / 6.0, wz as f32 / 6.0) > 0.5
                    {
                        Block::GLOWSTONE
                    } else {
                        continue;
                    };
                    blocks[index(x, y, z)] = block;
                }
            }
        }
        let cache = RefCell::new(FxHashMap::default());
        let open = |x, z, top, bottom| self.column_open(x, z, top, bottom, &mut cache.borrow_mut());
        self.fortresses.paint(&mut blocks, base, &open);
        self.fortresses.bastions.paint(&mut blocks, base);
        self.paint_ancient_debris(&mut blocks, base);
        ChunkData::from_dense(blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(chunks: &[ChunkData], x: usize, z: usize) -> Vec<Block> {
        (0..=ROOF as usize).map(|y| chunks[y / CHUNK_SIZE].get(x, y % CHUNK_SIZE, z)).collect()
    }

    #[test]
    fn caverns_lie_between_bedrock_and_a_lava_sea() {
        let g = NetherGen::new(5);
        let mut counts = std::collections::HashMap::new();
        for cx in 0..2 {
            for cz in 0..2 {
                let chunks: Vec<ChunkData> = (0..4).map(|cy| g.generate(IVec3::new(cx, cy, cz))).collect();
                assert!(matches!(g.generate(IVec3::new(cx, 4, cz)), ChunkData::Uniform(Block::AIR)));
                for x in (0..CHUNK_SIZE).step_by(3) {
                    for z in (0..CHUNK_SIZE).step_by(3) {
                        let col = column(&chunks, x, z);
                        assert_eq!((col[0], col[ROOF as usize]), (Block::BEDROCK, Block::BEDROCK));
                        for (y, &b) in col.iter().enumerate() {
                            *counts.entry(b).or_insert(0) += 1;
                            if b == Block::LAVA {
                                assert!(y as i32 <= LAVA_SEA, "lava above the sea at {y}");
                            }
                            if b == Block::AIR {
                                assert!(y as i32 > LAVA_SEA, "air below the sea at {y}");
                            }
                        }
                    }
                }
            }
        }
        let n = |b| counts.get(&b).copied().unwrap_or(0);
        let total: usize = counts.values().sum();
        assert!(n(Block::NETHERRACK) > total / 4, "{counts:?}");
        assert!(n(Block::AIR) > total / 6, "open caverns: {counts:?}");
        for b in [Block::LAVA, Block::SOUL_SAND, Block::QUARTZ_ORE, Block::GLOWSTONE] {
            assert!(n(b) > 0, "no {} in {counts:?}", b.name());
        }
    }

    fn world_block(
        g: &NetherGen,
        cache: &mut std::collections::HashMap<IVec3, ChunkData>,
        wx: i32,
        wy: i32,
        wz: i32,
    ) -> Block {
        let cpos = IVec3::new(wx.div_euclid(CHUNK_SIZE_I), wy.div_euclid(CHUNK_SIZE_I), wz.div_euclid(CHUNK_SIZE_I));
        let chunk = cache.entry(cpos).or_insert_with(|| g.generate(cpos));
        let lx = wx.rem_euclid(CHUNK_SIZE_I) as usize;
        let ly = wy.rem_euclid(CHUNK_SIZE_I) as usize;
        let lz = wz.rem_euclid(CHUNK_SIZE_I) as usize;
        chunk.get(lx, ly, lz)
    }

    fn debris_in_chunk(chunk: &ChunkData) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if chunk.get(x, y, z) == Block::ANCIENT_DEBRIS {
                        out.push((x, y, z));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn ancient_debris_is_deterministic_per_chunk() {
        let g = NetherGen::new(0xDEAD_BEEF);
        let pos = IVec3::new(-2, 1, 3);
        let a = g.generate(pos);
        let b = g.generate(pos);
        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    assert_eq!(a.get(x, y, z), b.get(x, y, z), "({x},{y},{z})");
                }
            }
        }
    }

    #[test]
    fn debris_scatter_counts_and_cross_chunk_candidates_match_java_rules() {
        let g = NetherGen::new(90210);
        for large in [true, false] {
            let mut counts = [0usize; 4];
            let mut seam_candidates = 0;
            for cx in -32..=32 {
                for cz in -32..=32 {
                    let candidates: Vec<_> = g.debris_candidates(cx, cz, large).collect();
                    counts[candidates.len()] += 1;
                    if let Some(&origin) = candidates.first() {
                        assert!((8..=if large { 24 } else { 119 }).contains(&origin.y));
                        assert_eq!(origin.x.div_euclid(JAVA_CELL), cx);
                        assert_eq!(origin.z.div_euclid(JAVA_CELL), cz);
                        for (i, &p) in candidates.iter().enumerate() {
                            assert!((p - origin).abs().max_element() <= i as i32);
                            if p.x.div_euclid(JAVA_CELL) != cx || p.z.div_euclid(JAVA_CELL) != cz {
                                seam_candidates += 1;
                            }
                        }
                    }
                }
            }
            for count in counts.iter().take(if large { 4 } else { 3 }) {
                assert!(*count > 900 && *count < 1700, "uniform count including zero: {counts:?}");
            }
            assert!(seam_candidates > 20, "scattering must cross cell boundaries");
        }

        // A filled low chunk accepts every incoming candidate, including candidates
        // from source cells just outside it. Its neighbours cannot be air below lava sea.
        let base = IVec3::ZERO;
        let mut blocks = ChunkData::new_dense(Block::NETHERRACK);
        g.paint_ancient_debris(&mut blocks, base);
        let expected: std::collections::HashSet<_> = (-1..=2)
            .flat_map(|cx| (-1..=2).flat_map(move |cz| [true, false].map(move |large| (cx, cz, large))))
            .flat_map(|(cx, cz, large)| g.debris_candidates(cx, cz, large))
            .filter(|p| p.y < LAVA_SEA && NetherGen::contains(base, p.x, p.y, p.z))
            .collect();
        let actual: std::collections::HashSet<_> = debris_in_chunk(&ChunkData::from_dense(blocks))
            .into_iter()
            .map(|(x, y, z)| IVec3::new(x as i32, y as i32, z as i32))
            .filter(|p| p.y < LAVA_SEA)
            .collect();
        assert!(!expected.is_empty());
        assert_eq!(actual, expected);
    }

    #[test]
    fn ancient_debris_stays_underground_and_rare() {
        let g = NetherGen::new(7);
        let mut debris = 0usize;
        let mut netherrack = 0usize;
        for cx in -2..=2 {
            for cz in -2..=2 {
                for cy in 0..4 {
                    let base = IVec3::new(cx, cy, cz) * CHUNK_SIZE_I;
                    let chunk = g.generate(IVec3::new(cx, cy, cz));
                    for z in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_SIZE {
                            for x in 0..CHUNK_SIZE {
                                let wy = base.y + y as i32;
                                match chunk.get(x, y, z) {
                                    Block::ANCIENT_DEBRIS => {
                                        debris += 1;
                                        assert!((6..=120).contains(&wy), "debris at y={wy}");
                                    }
                                    Block::NETHERRACK => netherrack += 1,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(debris > 20, "expected non-vacuous debris count, got {debris}");
        assert!(debris * 1000 < netherrack, "debris should be rare: {debris} vs {netherrack} netherrack");
    }

    #[test]
    fn ancient_debris_rejects_air_exposure() {
        let g = NetherGen::new(4242);
        let mut cache = std::collections::HashMap::new();
        for cx in -2..=2 {
            for cz in -2..=2 {
                for cy in 0..4 {
                    let cpos = IVec3::new(cx, cy, cz);
                    cache.insert(cpos, g.generate(cpos));
                }
            }
        }
        let debris: Vec<IVec3> = cache
            .iter()
            .flat_map(|(cpos, chunk)| {
                let base = *cpos * CHUNK_SIZE_I;
                debris_in_chunk(chunk)
                    .into_iter()
                    .map(move |(x, y, z)| IVec3::new(base.x + x as i32, base.y + y as i32, base.z + z as i32))
            })
            .collect();
        for p in debris {
            for d in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let n = p + IVec3::new(d.0, d.1, d.2);
                assert_ne!(world_block(&g, &mut cache, n.x, n.y, n.z), Block::AIR, "debris at {p} touches air");
            }
        }
    }

    #[test]
    fn ancient_debris_large_cluster_favors_mid_heights() {
        let g = NetherGen::new(1337);
        let mut bands = [0u32; 3];
        for cx in -32..=32 {
            for cz in -32..=32 {
                for p in g.debris_candidates(cx, cz, true) {
                    bands[if p.y < 13 {
                        0
                    } else if p.y <= 19 {
                        1
                    } else {
                        2
                    }] += 1;
                }
            }
        }
        assert!(bands[0] > 100 && bands[2] > 100);
        assert!(bands[1] > bands[0] * 2 && bands[1] > bands[2] * 2, "triangle bias: {bands:?}");
    }

    #[test]
    fn exposure_samples_match_generated_terrain_and_fortress_carving() {
        let g = NetherGen::new(42);
        let fortress = g.fortresses.get(glam::IVec2::ZERO).unwrap();
        let mut cache = ExposureCache::default();
        let mut found_air = 0;
        let mut found_solid = 0;
        let blocks = ChunkData::new_dense(Block::NETHERRACK);
        for piece in fortress.pieces.iter().take(8) {
            let min = piece.bounds.min - IVec3::ONE;
            let max = piece.bounds.max + IVec3::ONE;
            let mut chunks = std::collections::HashMap::new();
            for y in (min.y..=max.y).step_by(2) {
                for z in (min.z..=max.z).step_by(2) {
                    for x in (min.x..=max.x).step_by(2) {
                        let expected = world_block(&g, &mut chunks, x, y, z) == Block::AIR;
                        // Force the neighbour-sampling path rather than reading the chunk.
                        let base = IVec3::new(x + CHUNK_SIZE_I, y, z);
                        assert_eq!(
                            g.air_for_exposure(IVec3::new(x, y, z), base, &blocks, &mut cache),
                            expected,
                            "point sample at ({x},{y},{z})"
                        );
                        if expected {
                            found_air += 1;
                        } else {
                            found_solid += 1;
                        }
                    }
                }
            }
        }
        assert!(found_air > 100 && found_solid > 100, "exercise carving and walls");
    }
}
