//! Nether terrain: netherrack caverns between a bedrock floor and roof,
//! a lava sea below y = 31, quartz ore, ancient debris (Java
//! `scattered_ore`), and glowstone hanging from the ceilings. Each 4×4
//! quart column has a Nether biome (`world::nether_biome`) whose surface
//! rules (Java's `SurfaceRuleData.nether`) dress floors and ceilings:
//! nylium and wart blocks in the forests, soul sand and soul soil in soul
//! sand valleys, basalt and blackstone in basalt deltas, and the old soul
//! sand and gravel shores in the wastes. Biome features follow
//! (`world::nether_decoration`).
//!
//! Like the overworld, every chunk is a pure function of the seed. Solidity
//! comes from two octaves of 3D noise, biased solid towards the floor and the
//! roof, sampled on a coarse grid and interpolated (like overworld caves).
//! Full-height grid columns (density and biome) are shared between chunks
//! in a bounded cache, so the four chunks of a column and the features that
//! reach across chunk borders sample each one once.
//! Fortresses (see `world::fortress`) are painted over the terrain.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, index};
use super::fortress::Fortresses;
use super::nether_biome::{NetherBiome, NetherBiomeSource};
use super::nether_biome_blocks as nb;
use super::noise::{Perlin, hash_f, hash3};
use super::structure::Rng;

/// The bedrock roof; nothing generates above it.
pub const ROOF: i32 = 127;
/// Open space at or below this height fills with lava.
pub const LAVA_SEA: i32 = 31;
const STEP: usize = 4;
const GRID: usize = CHUNK_SIZE / STEP + 1;
/// Java horizontal decoration cell (`in_square` placement).
const JAVA_CELL: i32 = 16;
/// Maximum offset for our size-3 scattered feature (candidate index 2).
const DEBRIS_SPREAD: i32 = 2;
const SALT_DEBRIS_LARGE: u64 = 0xDE_B1_01;
const SALT_DEBRIS_SMALL: u64 = 0xDE_B1_02;
const SALT_BLOB: u64 = 0xB1_0B;
const SALT_SURFACE: u64 = 0x5E_EF;
/// Grid levels y = 0, 4, ..., 132 (the top one is above the roof, so
/// interpolation in the top cell has a level above it).
const LEVELS: usize = (ROOF as usize + 1) / STEP + 2;
/// Shared grid columns kept between chunks (about 700 bytes each).
const COLUMN_CACHE_LIMIT: usize = 8_192;
/// Below this density magnitude the fast column scan evaluates every block
/// exactly, so it always agrees with [`interpolate`].
const EXACT_BAND: f32 = 1e-3;

/// One 4×4 quart column: densities at every grid level, the hashed
/// basalt-delta blob values there, the quart's biome, and the solidity of
/// its 16 block columns once someone needs them.
pub(super) struct GridColumn {
    density: [f32; LEVELS],
    blob: [u8; LEVELS],
    pub(super) biome: NetherBiome,
    bits: [OnceLock<u128>; STEP * STEP],
}

/// The four grid columns around a block column and its position between them.
pub(super) struct Corners<'a> {
    pub(super) c: [&'a GridColumn; 4],
    pub(super) t: [f32; 2],
}

impl Corners<'_> {
    /// [`Corners::solid_bits`], computed once per block column and shared.
    pub(super) fn cached_bits(&self) -> u128 {
        let [x, z] = self.t.map(|t| (t * STEP as f32) as usize);
        *self.c[0].bits[z * STEP + x].get_or_init(|| self.solid_bits())
    }

    /// Which of the column's blocks (bit y, 0..=127) are solid rock, from
    /// the same trilinear interpolation as [`interpolate`]. Between grid
    /// levels the interpolation is linear in y, so only levels whose ends
    /// straddle zero need every block evaluated.
    pub(super) fn solid_bits(&self) -> u128 {
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let [c00, c10, c01, c11] = self.c;
        let [tx, tz] = self.t;
        let a: [f32; LEVELS] = std::array::from_fn(|k| lerp(c00.density[k], c10.density[k], tx));
        let b: [f32; LEVELS] = std::array::from_fn(|k| lerp(c01.density[k], c11.density[k], tx));
        let mut bits = 0u128;
        for k in 0..(ROOF as usize).div_ceil(STEP) {
            let (lo, hi) = (lerp(a[k], b[k], tz), lerp(a[k + 1], b[k + 1], tz));
            let level = (k * STEP) as u32;
            if lo > EXACT_BAND && hi > EXACT_BAND {
                bits |= ((1u128 << STEP) - 1) << level;
            } else if lo < -EXACT_BAND && hi < -EXACT_BAND {
                continue;
            } else {
                for i in 0..STEP {
                    let ty = i as f32 / STEP as f32;
                    if lerp(lerp(a[k], a[k + 1], ty), lerp(b[k], b[k + 1], ty), tz) > 0.0 {
                        bits |= 1 << (level + i as u32);
                    }
                }
            }
        }
        // The floor and roof layers are always rock (bedrock or not).
        bits | 1 | 1 << ROOF
    }

    /// Hashed blob value at height `y`, trilinear between grid levels.
    fn blob(&self, y: i32) -> f32 {
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let (k, ty) = ((y / STEP as i32) as usize, (y % STEP as i32) as f32 / STEP as f32);
        let [c00, c10, c01, c11] = self.c;
        let level = |k: usize| {
            let v = |c: &GridColumn| c.blob[k] as f32 / 255.0;
            lerp(lerp(v(c00), v(c10), self.t[0]), lerp(v(c01), v(c11), self.t[0]), self.t[1])
        };
        lerp(level(k), level(k + 1), ty)
    }
}

/// Number of solid blocks from `y` upward (`up`) or downward before an open
/// one, at most 8: 1 means `y` is the top of a floor (or bottom of a ceiling).
pub(super) fn solid_run(bits: u128, y: i32, up: bool) -> u32 {
    if !(0..128).contains(&y) || bits >> y & 1 == 0 {
        return 0;
    }
    let run = if up { (bits >> y).trailing_ones() } else { (bits << (127 - y)).leading_ones() };
    run.min(8)
}

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
    /// Java's multi-noise Nether biomes (see `world::nether_biome`).
    pub biomes: NetherBiomeSource,
    columns: Mutex<FxHashMap<IVec2, Arc<GridColumn>>>,
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
            biomes: NetherBiomeSource::new(seed),
            columns: Mutex::new(FxHashMap::default()),
        }
    }

    /// The salted seed every Nether feature and placement hashes from.
    pub(super) fn seed(&self) -> u64 {
        self.seed
    }

    /// Bedrock floor and roof, thinning out over the four layers next to them.
    pub(super) fn bedrock(&self, x: i32, y: i32, z: i32) -> bool {
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

    /// The shared grid column at grid position `g` (multiples of 4 blocks).
    pub(super) fn grid_column(&self, g: IVec2) -> Arc<GridColumn> {
        if let Some(c) = self.columns.lock().unwrap().get(&g) {
            return Arc::clone(c);
        }
        // Sample outside the lock so workers fill different columns in parallel.
        let s = STEP as i32;
        let (x, z) = (g.x * s, g.y * s);
        let column = Arc::new(GridColumn {
            density: std::array::from_fn(|k| self.density(x, k as i32 * s, z)),
            blob: std::array::from_fn(|k| (hash3(x, k as i32, z, self.seed ^ SALT_BLOB) >> 56) as u8),
            biome: self.biomes.quart(g.x, g.y),
            bits: Default::default(),
        });
        let mut cache = self.columns.lock().unwrap();
        // Another worker may have filled this column while we sampled it.
        if let Some(found) = cache.get(&g) {
            return Arc::clone(found);
        }
        if cache.len() >= COLUMN_CACHE_LIMIT
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        Arc::clone(cache.entry(g).or_insert(column))
    }

    /// What the terrain and its biome's surface rules put at height `wy`
    /// of block column `(wx, wz)`, before structures and features.
    pub(super) fn classify(&self, wx: i32, wy: i32, wz: i32, bits: u128, corners: &Corners) -> Block {
        if self.bedrock(wx, wy, wz) {
            return Block::BEDROCK;
        }
        let solid = |y: i32| (0..128).contains(&y) && bits >> y & 1 == 1;
        if !solid(wy) {
            return if wy <= LAVA_SEA {
                Block::LAVA
            } else if wy > 60
                && (1..=3).any(|k| solid(wy + k))
                && self.glow.noise3(wx as f32 / 6.0, wy as f32 / 6.0, wz as f32 / 6.0) > 0.5
            {
                Block::GLOWSTONE
            } else {
                Block::AIR
            };
        }
        let biome = corners.c[0].biome;
        let floor = solid_run(bits, wy, true);
        let ceiling = solid_run(bits, wy, false);
        // Java's surface depth: a few blocks, varying by column.
        let depth = || 3 + (hash3(wx, 0, wz, self.seed ^ SALT_SURFACE) % 3) as u32;
        // The five layers under the roof are always netherrack.
        if wy < ROOF - 5 {
            let selector = || self.patches.noise2(wx as f32 / 16.0 + 31.7, wz as f32 / 16.0 - 47.3) > 0.0;
            match biome {
                NetherBiome::BasaltDeltas => {
                    if ceiling <= depth() {
                        return Block::BASALT;
                    }
                    if floor <= depth() {
                        return if selector() { Block::BLACKSTONE } else { Block::BASALT };
                    }
                    // Java's basalt and blackstone blobs through the rock.
                    let blob = corners.blob(wy);
                    if blob > 0.62 {
                        return Block::BASALT;
                    }
                    if blob < 0.3 {
                        return Block::BLACKSTONE;
                    }
                }
                NetherBiome::SoulSandValley if ceiling.min(floor) <= depth() => {
                    return if selector() { Block::SOUL_SAND } else { nb::SOUL_SOIL };
                }
                NetherBiome::CrimsonForest | NetherBiome::WarpedForest if floor == 1 && wy > LAVA_SEA => {
                    let bare = self.glow.noise2(wx as f32 / 8.0 + 13.1, wz as f32 / 8.0 + 71.9) > 0.42;
                    if !bare {
                        let wart = self.patches.noise2(wx as f32 / 8.0 - 91.3, wz as f32 / 8.0 + 5.7) > 0.5;
                        let wood = if biome == NetherBiome::CrimsonForest {
                            nb::NetherWood::Crimson
                        } else {
                            nb::NetherWood::Warped
                        };
                        return if wart { wood.wart() } else { wood.nylium() };
                    }
                }
                NetherBiome::NetherWastes if floor <= 3 => {
                    if wy < 90 && self.patches.noise2(wx as f32 / 22.0, wz as f32 / 22.0) > 0.32 {
                        return Block::SOUL_SAND;
                    }
                    if floor <= 2
                        && (LAVA_SEA - 4..LAVA_SEA + 6).contains(&wy)
                        && self.patches.noise2(wz as f32 / 15.0 + 50.0, wx as f32 / 15.0) > 0.3
                    {
                        return Block::GRAVEL;
                    }
                }
                _ => {}
            }
        }
        // Basalt deltas carry twice the quartz (Java's ore_quartz_deltas).
        let h = |salt: u64| hash_f(wx, wy, wz, self.seed ^ salt);
        let quartz = |salt: u64| hash_f(wx >> 1, wy >> 1, wz >> 1, self.seed ^ salt) < 0.035;
        if (quartz(3) || (biome == NetherBiome::BasaltDeltas && quartz(5))) && h(4) < 0.55 {
            Block::QUARTZ_ORE
        } else {
            Block::NETHERRACK
        }
    }

    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        let base = cpos * CHUNK_SIZE_I;
        if base.y > ROOF || base.y + CHUNK_SIZE_I <= 0 {
            return ChunkData::Uniform(Block::AIR);
        }
        let g0 = IVec2::new(base.x, base.z) / STEP as i32;
        let grid: [[Arc<GridColumn>; GRID]; GRID] =
            std::array::from_fn(|gz| std::array::from_fn(|gx| self.grid_column(g0 + IVec2::new(gx as i32, gz as i32))));
        let mut blocks = ChunkData::new_dense(Block::AIR);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let (gx, gz) = (x / STEP, z / STEP);
                let corners = Corners {
                    c: [&grid[gz][gx], &grid[gz][gx + 1], &grid[gz + 1][gx], &grid[gz + 1][gx + 1]],
                    t: [(x % STEP) as f32 / STEP as f32, (z % STEP) as f32 / STEP as f32],
                };
                let bits = corners.cached_bits();
                for y in 0..CHUNK_SIZE {
                    let wy = base.y + y as i32;
                    if wy > ROOF {
                        break;
                    }
                    let block = self.classify(wx, wy, wz, bits, &corners);
                    if block != Block::AIR {
                        blocks[index(x, y, z)] = block;
                    }
                }
            }
        }
        super::nether_decoration::decorate(self, &mut blocks, base);
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
                        let (wx, wz) = (cx * CHUNK_SIZE_I + x as i32, cz * CHUNK_SIZE_I + z as i32);
                        let deltas = g.biomes.biome(wx, wz) == NetherBiome::BasaltDeltas;
                        for (y, &b) in col.iter().enumerate() {
                            *counts.entry(b).or_insert(0) += 1;
                            if b == Block::LAVA && !deltas {
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
        let surfaces = [Block::SOUL_SAND, Block::BASALT, nb::SOUL_SOIL, nb::CRIMSON_NYLIUM, nb::WARPED_NYLIUM];
        assert!(surfaces.iter().any(|&b| n(b) > 0), "a biome surface in {counts:?}");
        for b in [Block::LAVA, Block::QUARTZ_ORE, Block::GLOWSTONE] {
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
    fn grid_column_cache_keeps_warm_columns_when_full() {
        let g = NetherGen::new(12345);
        let warm: Vec<_> = (0..COLUMN_CACHE_LIMIT).map(|x| g.grid_column(IVec2::new(x as i32, 0))).collect();
        assert_eq!(g.columns.lock().unwrap().len(), COLUMN_CACHE_LIMIT);
        // A hit at capacity must reuse the sampled column without evicting.
        assert!(Arc::ptr_eq(&warm[0], &g.grid_column(IVec2::ZERO)));
        let added = g.grid_column(IVec2::new(-1, 0));
        let cache = g.columns.lock().unwrap();
        assert_eq!(cache.len(), COLUMN_CACHE_LIMIT);
        assert!(Arc::ptr_eq(&added, cache.get(&IVec2::new(-1, 0)).unwrap()));
        let retained = warm
            .iter()
            .enumerate()
            .filter(|(x, column)| cache.get(&IVec2::new(*x as i32, 0)).is_some_and(|c| Arc::ptr_eq(c, column)))
            .count();
        assert_eq!(retained, COLUMN_CACHE_LIMIT - 1, "only one warm column is evicted");
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
                        let generated = world_block(&g, &mut chunks, x, y, z);
                        let expected = generated == Block::AIR;
                        // Force the neighbour-sampling path rather than reading the chunk.
                        let base = IVec3::new(x + CHUNK_SIZE_I, y, z);
                        let sampled = g.air_for_exposure(IVec3::new(x, y, z), base, &blocks, &mut cache);
                        // Biome features only ever fill air, so the undecorated
                        // sample may see air where a plant or vine now stands.
                        let feature = nb::NetherWood::of(generated).is_some()
                            || nb::Vine::of(generated).is_some()
                            || generated == nb::SHROOMLIGHT
                            || generated == Block::BASALT
                            || generated == nb::BONE_BLOCK
                            || generated == nb::SOUL_FIRE;
                        assert!(sampled == expected || (sampled && feature), "point sample at ({x},{y},{z})");
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
