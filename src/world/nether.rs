//! Nether terrain: netherrack caverns between a bedrock floor and roof,
//! a lava sea below y = 31, soul sand and gravel shores, quartz ore and
//! glowstone hanging from the ceilings.
//!
//! Like the overworld, every chunk is a pure function of the seed. Solidity
//! comes from two octaves of 3D noise, biased solid towards the floor and the
//! roof, sampled on a coarse grid and interpolated (like overworld caves).
//! Fortresses (see `world::fortress`) are painted over the terrain.

use std::cell::RefCell;

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, index};
use super::fortress::Fortresses;
use super::noise::{Perlin, hash_f};

/// The bedrock roof; nothing generates above it.
pub const ROOF: i32 = 127;
/// Open space at or below this height fills with lava.
pub const LAVA_SEA: i32 = 31;
const STEP: usize = 4;
const GRID: usize = CHUNK_SIZE / STEP + 1;
/// Extra samples above the chunk so ceilings just above it are known.
const GRID_Y: usize = GRID + 1;

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
}
