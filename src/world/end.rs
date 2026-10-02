//! Seeded End terrain: a central island, ten obsidian pillars, a void ring and outer islands.
//! Column shapes and structures use world coordinates so chunk order cannot create seams.
use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, index};
use super::noise::{Perlin, hash3};
use glam::IVec3;

pub const SPAWN: IVec3 = IVec3::new(100, 49, 0);
const OUTER_START: f64 = 1024.0;

struct Pillar {
    x: i32,
    z: i32,
    radius: i32,
    top: i32,
}
pub struct EndGen {
    shape: Perlin,
    detail: Perlin,
    seed: u64,
    pillars: [Pillar; 10],
}
impl EndGen {
    pub fn new(seed: u64) -> Self {
        let mut ranks: [usize; 10] = std::array::from_fn(|i| i);
        ranks.sort_unstable_by_key(|&i| hash3(i as i32, 0, 0, seed ^ 0x454E44));
        let pillars = std::array::from_fn(|i| {
            let angle = i as f64 * std::f64::consts::TAU / 10.0;
            let rank = ranks[i] as i32;
            Pillar {
                x: (angle.cos() * 42.0).round() as i32,
                z: (angle.sin() * 42.0).round() as i32,
                radius: 2 + rank / 3,
                top: 76 + rank * 3,
            }
        });
        Self { shape: Perlin::new(seed ^ 0x454E4401), detail: Perlin::new(seed ^ 0x454E4402), seed, pillars }
    }
    /// Surface and bottom of the floating island, or no land. No bedrock floor under the void.
    pub fn column(&self, x: i32, z: i32) -> Option<(i32, i32)> {
        let radius = (x as f64).hypot(z as f64);
        let rough = self.detail.noise2(x as f32 / 32.0, z as f32 / 32.0);
        let mass = if radius < 140.0 {
            1.0 - radius as f32 / 100.0 + rough * 0.10
        } else if radius < OUTER_START {
            return None;
        } else {
            let cell_x = x.div_euclid(128);
            let cell_z = z.div_euclid(128);
            let mut mass: f32 = -1.0;
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let (cx, cz) = (cell_x + dx, cell_z + dz);
                    let h = hash3(cx, 0, cz, self.seed ^ 0x151A);
                    if h.is_multiple_of(5) {
                        continue;
                    }
                    let center_x = cx as f64 * 128.0 + 24.0 + (h % 80) as f64;
                    let center_z = cz as f64 * 128.0 + 24.0 + ((h >> 8) % 80) as f64;
                    if center_x.hypot(center_z) < OUTER_START + 64.0 {
                        continue;
                    }
                    let size = 24.0 + ((h >> 16) % 48) as f32;
                    let d = (x as f64 - center_x).hypot(z as f64 - center_z) as f32;
                    mass = mass.max(1.0 - d / size + rough * 0.12);
                }
            }
            mass
        };
        if mass <= 0.0 {
            return None;
        }
        let noise = self.shape.noise2(x as f32 / 80.0, z as f32 / 80.0);
        let top = (56.0 + mass.min(1.0) * 10.0 + noise * 5.0).floor() as i32;
        let depth = (mass.sqrt() * 30.0 + rough * 3.0).max(2.0) as i32;
        Some((top, top - depth))
    }
    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        let base = cpos * CHUNK_SIZE_I;
        if base.y < 0 || base.y > 104 {
            return ChunkData::Uniform(Block::AIR);
        }
        let mut blocks = ChunkData::new_dense(Block::AIR);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let column = self.column(wx, wz);
                let pillar = self.pillars.iter().find(|p| {
                    let (dx, dz) = (wx - p.x, wz - p.z);
                    dx.abs() <= p.radius && dz.abs() <= p.radius && dx * dx + dz * dz <= p.radius * p.radius
                });
                for y in 0..CHUNK_SIZE {
                    let wy = base.y + y as i32;
                    let platform = (wx - SPAWN.x).abs() <= 2 && (wz - SPAWN.z).abs() <= 2;
                    let block = if platform && wy == SPAWN.y - 1 {
                        Block::OBSIDIAN
                    } else if platform && (SPAWN.y..SPAWN.y + 3).contains(&wy) {
                        Block::AIR
                    } else if pillar.is_some_and(|p| wy == p.top && wx == p.x && wz == p.z) {
                        Block::BEDROCK
                    } else if pillar.is_some_and(|p| (0..p.top).contains(&wy)) {
                        Block::OBSIDIAN
                    } else if column.is_some_and(|(top, bottom)| wy >= bottom && wy <= top) {
                        Block::END_STONE
                    } else {
                        Block::AIR
                    };
                    blocks[index(x, y, z)] = block;
                }
            }
        }
        ChunkData::from_dense(blocks)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn islands_void_platform_and_pillars() {
        let generator = EndGen::new(42);
        assert!(generator.column(0, 0).is_some());
        assert!(generator.column(500, 0).is_none());
        assert!(generator.generate(IVec3::new(15, 1, 0)).uniform() == Some(Block::AIR));
        assert!(generator.generate(IVec3::new(0, 4, 0)).uniform() == Some(Block::AIR));
        for z in -2..=2 {
            for x in 98..=102 {
                let pos = IVec3::new(x, 48, z);
                let chunk = generator.generate(super::super::chunk::chunk_of(pos));
                let local = super::super::chunk::local_of(pos);
                assert_eq!(chunk.get(local.x as usize, local.y as usize, local.z as usize), Block::OBSIDIAN);
            }
        }
        assert!(generator.pillars.iter().all(|p| (76..=103).contains(&p.top)));
        for p in &generator.pillars {
            let pos = IVec3::new(p.x, p.top, p.z);
            let local = super::super::chunk::local_of(pos);
            assert_eq!(
                generator.generate(super::super::chunk::chunk_of(pos)).get(
                    local.x as usize,
                    local.y as usize,
                    local.z as usize
                ),
                Block::BEDROCK
            );
        }
        assert!((1100..1600).step_by(16).any(|x| generator.column(x, 0).is_some()));
    }
    #[test]
    fn seeded_chunks_are_repeatable_in_any_order() {
        let a = EndGen::new(7);
        let b = EndGen::new(7);
        let c = EndGen::new(8);
        for pos in [IVec3::new(0, 1, 0), IVec3::new(-1, 2, 1), IVec3::new(38, 1, -1)] {
            let x = a.generate(pos);
            let y = b.generate(pos);
            for z in 0..CHUNK_SIZE {
                for i in 0..CHUNK_SIZE {
                    for j in 0..CHUNK_SIZE {
                        assert_eq!(x.get(i, j, z), y.get(i, j, z));
                    }
                }
            }
        }
        assert!((1100..1800).step_by(8).any(|x| a.column(x, 0) != c.column(x, 0)));
    }
}
