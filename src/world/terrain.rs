//! Procedural terrain generation.
//!
//! Every chunk is generated independently from the seed (no cross-chunk
//! state), so generation parallelises trivially across worker threads.
//! Features that straddle chunk borders, such as trees, are placed
//! deterministically from hashed world coordinates and each chunk writes
//! only its own part of them.

use glam::IVec3;

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, index};
use super::noise::{Perlin, hash_f, hash3};

pub const SEA_LEVEL: i32 = 62;
const TREE_CELL: i32 = 5;
/// How far a tree's leaves can extend from its trunk.
const TREE_REACH: i32 = 3;
const CAVE_STEP: usize = 4;
const CAVE_GRID: usize = CHUNK_SIZE / CAVE_STEP + 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Ocean,
    Beach,
    Plains,
    Forest,
    Desert,
    Mountains,
    Snowy,
    Taiga,
}

#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub height: i32,
    pub biome: Biome,
}

pub struct Generator {
    pub seed: u64,
    continent: Perlin,
    erosion: Perlin,
    ridge: Perlin,
    detail: Perlin,
    temperature: Perlin,
    humidity: Perlin,
    cave_a: Perlin,
    cave_b: Perlin,
    cavern: Perlin,
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Generator {
    pub fn new(seed: u64) -> Self {
        let p = |salt: u64| Perlin::new(seed ^ salt.wrapping_mul(0x2545_F491_4F6C_DD1D));
        Self {
            seed,
            continent: p(1),
            erosion: p(2),
            ridge: p(3),
            detail: p(4),
            temperature: p(5),
            humidity: p(6),
            cave_a: p(7),
            cave_b: p(8),
            cavern: p(9),
        }
    }

    /// Surface height and biome of a world column.
    pub fn column(&self, x: i32, z: i32) -> Column {
        let (fx, fz) = (x as f32, z as f32);
        let cont = self.continent.fbm2(fx / 900.0, fz / 900.0, 5) * 1.8;
        let erosion = self.erosion.fbm2(fx / 500.0, fz / 500.0, 3) * 1.6;
        let ridge = 1.0 - self.ridge.fbm2(fx / 260.0, fz / 260.0, 5).abs() * 2.2;
        let ridge = ridge.max(0.0).powi(2);
        let detail = self.detail.fbm2(fx / 60.0, fz / 60.0, 4);

        // Land rises gently out of the ocean; mountains only appear inland
        // where erosion is low.
        let base = SEA_LEVEL as f32 + 4.0 + cont * 34.0;
        let mountain = smoothstep(0.05, 0.45, cont) * smoothstep(0.1, -0.4, erosion);
        let hills = smoothstep(-0.2, 0.4, cont) * 10.0;
        let h = base + detail * (4.0 + hills) + ridge * mountain * 110.0;
        let height = (h as i32).clamp(4, 240);

        let temp =
            self.temperature.fbm2(fx / 1100.0, fz / 1100.0, 3) * 1.8 - (height - SEA_LEVEL).max(0) as f32 / 160.0;
        let humid = self.humidity.fbm2(fx / 900.0, fz / 900.0, 3) * 1.8;

        let biome = if height < SEA_LEVEL - 1 {
            Biome::Ocean
        } else if height <= SEA_LEVEL + 2 && mountain < 0.2 {
            Biome::Beach
        } else if height > 125 || mountain > 0.55 {
            Biome::Mountains
        } else if temp < -0.35 {
            if humid > 0.0 { Biome::Taiga } else { Biome::Snowy }
        } else if temp > 0.35 && humid < 0.1 {
            Biome::Desert
        } else if humid > 0.05 {
            Biome::Forest
        } else {
            Biome::Plains
        };
        Column { height, biome }
    }

    /// The block at the very top of a column, before caves and decoration.
    fn surface_block(col: Column, y: i32) -> Block {
        match col.biome {
            Biome::Ocean => {
                if col.height >= SEA_LEVEL - 4 {
                    Block::SAND
                } else {
                    Block::GRAVEL
                }
            }
            Biome::Beach | Biome::Desert => Block::SAND,
            Biome::Mountains => {
                if y > 165 {
                    Block::SNOW
                } else if y > 135 {
                    Block::STONE
                } else {
                    Block::GRASS
                }
            }
            Biome::Snowy | Biome::Taiga => Block::SNOWY_GRASS,
            Biome::Plains | Biome::Forest => Block::GRASS,
        }
    }

    fn filler_block(col: Column, y: i32) -> Block {
        match col.biome {
            Biome::Beach | Biome::Ocean => Block::SAND,
            Biome::Desert => {
                if y > col.height - 3 {
                    Block::SAND
                } else {
                    Block::SANDSTONE
                }
            }
            Biome::Mountains if col.height > 135 => Block::STONE,
            _ => Block::DIRT,
        }
    }

    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        let base = cpos * CHUNK_SIZE_I;
        let mut cols = [[Column { height: 0, biome: Biome::Plains }; CHUNK_SIZE]; CHUNK_SIZE];
        let mut max_h = i32::MIN;
        let mut min_h = i32::MAX;
        for (z, row) in cols.iter_mut().enumerate() {
            for (x, c) in row.iter_mut().enumerate() {
                *c = self.column(base.x + x as i32, base.z + z as i32);
                max_h = max_h.max(c.height);
                min_h = min_h.min(c.height);
            }
        }

        let top = base.y + CHUNK_SIZE_I - 1;
        // Open sky: nothing (not even tree canopies) reaches this chunk.
        if base.y > (max_h + 12).max(SEA_LEVEL) {
            return ChunkData::Uniform(Block::AIR);
        }

        let caves = if base.y < max_h { Some(self.cave_field(base)) } else { None };
        let mut blocks = ChunkData::new_dense(Block::AIR);

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = cols[z][x];
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let fill_top = col.height.max(SEA_LEVEL).min(top);
                let depth = if col.biome == Biome::Desert { 6 } else { 4 };
                for wy in base.y..=fill_top {
                    let y = (wy - base.y) as usize;
                    // Solid bedrock at y=0, thinning out randomly up to y=3.
                    let mut b = if wy == 0 || (wy < 4 && hash_f(wx, wy, wz, self.seed) < (4 - wy) as f32 / 4.0) {
                        Block::BEDROCK
                    } else if wy < col.height - depth {
                        self.ore_or_stone(wx, wy, wz)
                    } else if wy < col.height {
                        Self::filler_block(col, wy)
                    } else if wy == col.height {
                        Self::surface_block(col, wy)
                    } else {
                        Block::WATER
                    };

                    if let Some(field) = &caves {
                        let carve_limit = if col.height < SEA_LEVEL + 2 {
                            col.height - 6 // keep sea floors sealed
                        } else {
                            col.height
                        };
                        if wy > 4 && wy <= carve_limit && b != Block::WATER && Self::is_cave(&field[..], x, y, z, wy) {
                            b = Block::AIR;
                        }
                    }
                    blocks[index(x, y, z)] = b;
                }
            }
        }

        if base.y <= max_h + 12 && top >= min_h {
            self.place_trees(&mut blocks, base);
        }
        ChunkData::from_dense(blocks)
    }

    fn ore_or_stone(&self, x: i32, y: i32, z: i32) -> Block {
        // Ores form small clusters: pick a 2x2x2 cell, then thin it out.
        let cell = hash3(x >> 1, y >> 1, z >> 1, self.seed ^ 0x0E5) % 1000;
        let ore = match cell {
            0..=11 if y < 128 => Block::COAL_ORE,
            12..=18 if y < 64 => Block::IRON_ORE,
            19..=21 if y < 32 => Block::GOLD_ORE,
            22..=23 if y < 16 => Block::DIAMOND_ORE,
            _ => return Block::STONE,
        };
        if !hash3(x, y, z, self.seed ^ 0x0E6).is_multiple_of(3) { ore } else { Block::STONE }
    }

    /// Samples the cave noises on a coarse grid; per-block values are
    /// trilinearly interpolated, which is ~60x cheaper than sampling each block.
    fn cave_field(&self, base: IVec3) -> Box<[[f32; 3]; CAVE_GRID * CAVE_GRID * CAVE_GRID]> {
        let mut field = Box::new([[0.0f32; 3]; CAVE_GRID * CAVE_GRID * CAVE_GRID]);
        for gy in 0..CAVE_GRID {
            for gz in 0..CAVE_GRID {
                for gx in 0..CAVE_GRID {
                    let x = (base.x + (gx * CAVE_STEP) as i32) as f32;
                    let y = (base.y + (gy * CAVE_STEP) as i32) as f32;
                    let z = (base.z + (gz * CAVE_STEP) as i32) as f32;
                    field[gx + gz * CAVE_GRID + gy * CAVE_GRID * CAVE_GRID] = [
                        self.cave_a.noise3(x / 48.0, y / 32.0, z / 48.0),
                        self.cave_b.noise3(x / 48.0, y / 32.0, z / 48.0),
                        self.cavern.noise3(x / 90.0, y / 45.0, z / 90.0),
                    ];
                }
            }
        }
        field
    }

    #[inline]
    fn is_cave(field: &[[f32; 3]], x: usize, y: usize, z: usize, wy: i32) -> bool {
        let (gx, gy, gz) = (x / CAVE_STEP, y / CAVE_STEP, z / CAVE_STEP);
        let (tx, ty, tz) = (
            (x % CAVE_STEP) as f32 / CAVE_STEP as f32,
            (y % CAVE_STEP) as f32 / CAVE_STEP as f32,
            (z % CAVE_STEP) as f32 / CAVE_STEP as f32,
        );
        let at = |dx: usize, dy: usize, dz: usize| {
            field[(gx + dx) + (gz + dz) * CAVE_GRID + (gy + dy) * CAVE_GRID * CAVE_GRID]
        };
        let mut v = [0.0f32; 3];
        for (i, out) in v.iter_mut().enumerate() {
            let c00 = at(0, 0, 0)[i] + (at(1, 0, 0)[i] - at(0, 0, 0)[i]) * tx;
            let c10 = at(0, 1, 0)[i] + (at(1, 1, 0)[i] - at(0, 1, 0)[i]) * tx;
            let c01 = at(0, 0, 1)[i] + (at(1, 0, 1)[i] - at(0, 0, 1)[i]) * tx;
            let c11 = at(0, 1, 1)[i] + (at(1, 1, 1)[i] - at(0, 1, 1)[i]) * tx;
            let c0 = c00 + (c10 - c00) * ty;
            let c1 = c01 + (c11 - c01) * ty;
            *out = c0 + (c1 - c0) * tz;
        }
        // "Spaghetti" tunnels run where two noise fields are both near zero.
        let tunnel = v[0] * v[0] + v[1] * v[1] < 0.0045;
        // Large caverns deep underground.
        let cavern = wy < 48 && v[2] > 0.42 - (48 - wy) as f32 * 0.002;
        tunnel || cavern
    }

    fn place_trees(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let cell_min_x = (base.x - TREE_REACH).div_euclid(TREE_CELL);
        let cell_max_x = (base.x + CHUNK_SIZE_I + TREE_REACH).div_euclid(TREE_CELL);
        let cell_min_z = (base.z - TREE_REACH).div_euclid(TREE_CELL);
        let cell_max_z = (base.z + CHUNK_SIZE_I + TREE_REACH).div_euclid(TREE_CELL);

        for cz in cell_min_z..=cell_max_z {
            for cx in cell_min_x..=cell_max_x {
                let h = hash3(cx, 0, cz, self.seed ^ 0x7EE);
                let tx = cx * TREE_CELL + (h % TREE_CELL as u64) as i32;
                let tz = cz * TREE_CELL + ((h >> 8) % TREE_CELL as u64) as i32;
                if tx < base.x - TREE_REACH
                    || tx >= base.x + CHUNK_SIZE_I + TREE_REACH
                    || tz < base.z - TREE_REACH
                    || tz >= base.z + CHUNK_SIZE_I + TREE_REACH
                {
                    continue;
                }
                let col = self.column(tx, tz);
                let density = match col.biome {
                    Biome::Forest => 0.75,
                    Biome::Taiga => 0.6,
                    Biome::Plains => 0.06,
                    Biome::Snowy => 0.08,
                    Biome::Mountains if col.height < 125 => 0.12,
                    Biome::Desert => 0.1,
                    _ => 0.0,
                };
                let roll = ((h >> 16) & 0xFFFF) as f32 / 65536.0;
                if roll >= density || col.height <= SEA_LEVEL {
                    continue;
                }
                let variant = (h >> 32) as u32;
                let ground = IVec3::new(tx, col.height, tz);
                match (col.biome, Self::surface_block(col, col.height)) {
                    (Biome::Desert, Block::SAND) => Self::cactus(blocks, base, ground, variant),
                    (Biome::Taiga | Biome::Snowy, _) => Self::spruce(blocks, base, ground, variant),
                    (_, Block::GRASS) => Self::oak(blocks, base, ground, variant),
                    _ => {}
                }
            }
        }
    }

    /// Writes a block if it falls inside this chunk. Trunks may replace
    /// leaves; leaves only fill air. This makes overlapping trees resolve the
    /// same way no matter which chunk (and order) places them.
    #[inline]
    fn put(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, p: IVec3, b: Block) {
        let l = p - base;
        if l.cmplt(IVec3::ZERO).any() || l.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
            return;
        }
        let slot = &mut blocks[index(l.x as usize, l.y as usize, l.z as usize)];
        let is_leaf = matches!(b, Block::LEAVES | Block::SPRUCE_LEAVES);
        if *slot == Block::AIR || (!is_leaf && matches!(*slot, Block::LEAVES | Block::SPRUCE_LEAVES)) {
            *slot = b;
        }
    }

    fn oak(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, ground: IVec3, v: u32) {
        let height = 4 + (v % 3) as i32;
        let top = ground.y + height;
        for dy in -2..=1 {
            let r: i32 = if dy >= 0 { 1 } else { 2 };
            for dz in -r..=r {
                for dx in -r..=r {
                    // Trim corners randomly for a less boxy canopy.
                    let corner = dx.abs() == r && dz.abs() == r;
                    if corner && (dy == 1 || (v >> ((dx + dz * 3 + dy * 7) & 15)) & 1 == 0) {
                        continue;
                    }
                    Self::put(blocks, base, IVec3::new(ground.x + dx, top + dy, ground.z + dz), Block::LEAVES);
                }
            }
        }
        for y in ground.y + 1..top {
            Self::put(blocks, base, IVec3::new(ground.x, y, ground.z), Block::LOG);
        }
    }

    fn spruce(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, ground: IVec3, v: u32) {
        let height = 6 + (v % 4) as i32;
        let top = ground.y + height;
        Self::put(blocks, base, IVec3::new(ground.x, top + 1, ground.z), Block::SPRUCE_LEAVES);
        for i in 0..height - 2 {
            let y = top - i;
            let r = match i {
                0 => 0,
                _ if i % 2 == 1 => 1,
                _ => (i / 2).min(TREE_REACH) - (i / 6),
            };
            for dz in -r..=r {
                for dx in -r..=r {
                    if r > 1 && dx.abs() == r && dz.abs() == r {
                        continue;
                    }
                    Self::put(blocks, base, IVec3::new(ground.x + dx, y, ground.z + dz), Block::SPRUCE_LEAVES);
                }
            }
        }
        for y in ground.y + 1..top {
            Self::put(blocks, base, IVec3::new(ground.x, y, ground.z), Block::LOG);
        }
    }

    fn cactus(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, ground: IVec3, v: u32) {
        for y in 1..=1 + (v % 3) as i32 {
            Self::put(blocks, base, ground + IVec3::new(0, y, 0), Block::CACTUS);
        }
    }

    /// Finds a dry-land spawn point near the origin.
    pub fn find_spawn(&self) -> IVec3 {
        for r in 0..64 {
            for i in 0..(r * 8).max(1) {
                let a = i as f32 / (r * 8).max(1) as f32 * std::f32::consts::TAU;
                let x = (a.cos() * r as f32 * 16.0) as i32;
                let z = (a.sin() * r as f32 * 16.0) as i32;
                let c = self.column(x, z);
                if c.height > SEA_LEVEL + 1 && c.height < 110 {
                    return IVec3::new(x, c.height + 1, z);
                }
            }
        }
        IVec3::new(0, self.column(0, 0).height.max(SEA_LEVEL) + 1, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let g = Generator::new(1234);
        for p in [IVec3::new(0, 1, 0), IVec3::new(-3, 2, 5), IVec3::new(7, 0, -2)] {
            let a = g.generate(p);
            let b = g.generate(p);
            let (mut va, mut vb) = (Vec::new(), Vec::new());
            a.for_each_block(|x| va.push(x));
            b.for_each_block(|x| vb.push(x));
            assert!(va == vb);
        }
    }

    #[test]
    fn terrain_has_varied_heights_and_biomes() {
        let g = Generator::new(99);
        let mut heights = Vec::new();
        let mut biomes = std::collections::HashSet::new();
        for i in -60..60 {
            for j in -60..60 {
                let c = g.column(i * 64, j * 64);
                heights.push(c.height);
                biomes.insert(format!("{:?}", c.biome));
            }
        }
        let min = *heights.iter().min().unwrap();
        let max = *heights.iter().max().unwrap();
        assert!(min < SEA_LEVEL && max > 110, "height range {min}..{max}");
        assert!(biomes.len() >= 5, "biomes: {biomes:?}");
    }
}
