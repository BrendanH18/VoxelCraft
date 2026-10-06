//! Seeded End terrain: a central island, ten obsidian pillars, a void ring and outer islands.
//! Column shapes and structures use world coordinates so chunk order cannot create seams.
use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, index};
use super::noise::{Perlin, hash3};
use glam::IVec3;

pub const SPAWN: IVec3 = IVec3::new(100, 49, 0);
const OUTER_START: f64 = 1024.0;

/// One of the ten obsidian spikes (Java's `SpikeFeature.EndSpike`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pillar {
    pub x: i32,
    pub z: i32,
    pub radius: i32,
    /// The bedrock cap's height; an End crystal sits on it.
    pub top: i32,
    /// Ringed by an iron bar cage (the second and third shortest).
    pub guarded: bool,
}

impl Pillar {
    /// Where the pillar's End crystal stands (feet), above the bedrock cap.
    pub fn crystal(&self) -> glam::DVec3 {
        glam::DVec3::new(self.x as f64 + 0.5, (self.top + 1) as f64, self.z as f64 + 0.5)
    }

    /// The cage's iron bars: the sides of a 5x5 box four blocks high
    /// standing on the pillar, with a lid.
    fn caged(&self, x: i32, y: i32, z: i32) -> bool {
        let (dx, dy, dz) = (x - self.x, y - self.top, z - self.z);
        self.guarded
            && dx.abs() <= 2
            && dz.abs() <= 2
            && (0..=3).contains(&dy)
            && (dx.abs() == 2 || dz.abs() == 2 || dy == 3)
    }
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
                guarded: rank == 1 || rank == 2,
            }
        });
        Self { shape: Perlin::new(seed ^ 0x454E4401), detail: Perlin::new(seed ^ 0x454E4402), seed, pillars }
    }

    pub fn pillars(&self) -> &[Pillar; 10] {
        &self.pillars
    }

    /// The exit portal's centre: the cell above the central island's
    /// surface at 0, 0 (Java's `EndPodiumFeature` origin).
    pub fn podium(&self) -> IVec3 {
        let top = self.column(0, 0).map_or(SPAWN.y, |(top, _)| top);
        IVec3::new(0, top + 1, 0)
    }
    /// The 20 spots around the central island where a gateway appears for
    /// each dragon killed, in the order they're used (Java shuffles them by
    /// the world seed).
    pub fn gateways(&self) -> [IVec3; 20] {
        let mut order: [usize; 20] = std::array::from_fn(|i| i);
        order.sort_unstable_by_key(|&i| hash3(i as i32, 1, 0, self.seed ^ 0x4741_5445));
        order.map(|i| {
            let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * i as f64);
            IVec3::new((96.0 * a.cos()).floor() as i32, 75, (96.0 * a.sin()).floor() as i32)
        })
    }

    /// Where the gateway at `gateway` leads, found like Java's
    /// `TheEndGatewayBlockEntity.findOrCreateValidTeleportPos`: 1024 blocks
    /// straight out from the centre, moved to the first chunk with land
    /// (up to 16 chunks either way), then ten blocks above the highest
    /// block within 16. Returns the exit gateway's cell, and whether no
    /// land was found, so a small island must be built under it.
    pub fn gateway_exit(&self, gateway: IVec3) -> (IVec3, bool) {
        let dir = glam::DVec2::new(gateway.x as f64, gateway.z as f64).normalize_or(glam::DVec2::X);
        let mut at = dir * 1024.0;
        let chunk_land = |at: glam::DVec2| {
            let (cx, cz) = ((at.x.floor() as i32) >> 4 << 4, (at.y.floor() as i32) >> 4 << 4);
            (0..16)
                .flat_map(|z| (0..16).map(move |x| (cx + x, cz + z)))
                .filter_map(|(x, z)| self.column(x, z).map(|(top, _)| (IVec3::new(x, top, z), cx, cz)))
                .min_by_key(|(p, cx, cz)| (p.x - cx - 8).pow(2) + (p.z - cz - 8).pow(2))
                .map(|(p, ..)| p)
        };
        for _ in 0..16 {
            if chunk_land(at).is_none() {
                break;
            }
            at -= dir * 16.0;
        }
        for _ in 0..16 {
            if chunk_land(at).is_some() {
                break;
            }
            at += dir * 16.0;
        }
        let Some(land) = chunk_land(at) else {
            return (IVec3::new((at.x + 0.5).floor() as i32, 75 + 10, (at.y + 0.5).floor() as i32), true);
        };
        // The tallest land within 16, the land itself excluded like Java.
        let mut best = land;
        for dz in -16..=16 {
            for dx in -16..=16 {
                if (dx, dz) == (0, 0) {
                    continue;
                }
                if let Some((top, _)) = self.column(land.x + dx, land.z + dz)
                    && top > best.y
                {
                    best = IVec3::new(land.x + dx, top, land.z + dz);
                }
            }
        }
        (best + IVec3::Y * 10, false)
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
        let podium = self.podium();
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
                    } else if pillar.is_some_and(|p| p.caged(wx, wy, wz)) {
                        Block::IRON_BARS
                    } else if let Some(b) = podium_block(podium, IVec3::new(wx, wy, wz), false) {
                        b
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
/// An End gateway at `origin` (Java's `EndGatewayFeature`): the gateway in
/// a gap one block high, capped above and below by bedrock pluses with a
/// bedrock tip at each end.
pub fn gateway_block(origin: IVec3, p: IVec3) -> Option<Block> {
    let d = p - origin;
    if d.x.abs() > 1 || d.z.abs() > 1 || d.y.abs() > 2 {
        return None;
    }
    let (on_x, on_z) = (d.x == 0, d.z == 0);
    Some(match d.y.abs() {
        0 if on_x && on_z => Block::END_GATEWAY,
        2 if on_x && on_z => Block::BEDROCK,
        1 if on_x || on_z => Block::BEDROCK,
        _ => Block::AIR,
    })
}

/// The exit portal around `origin` at `p`, if it has a block there (Java's
/// `EndPodiumFeature`): a bedrock bowl with an end stone rim beneath, a
/// four-high bedrock pillar in the middle, and the portal itself once the
/// dragon is dead (`active`).
pub fn podium_block(origin: IVec3, p: IVec3, active: bool) -> Option<Block> {
    let d = p - origin;
    if d.x.abs() > 4 || d.z.abs() > 4 || !(-1..=32).contains(&d.y) {
        return None;
    }
    if d.x == 0 && d.z == 0 && (0..4).contains(&d.y) {
        return Some(Block::BEDROCK);
    }
    // Java compares squared distances between block corners.
    let d2 = d.length_squared() as f64;
    let inner = d2 < 2.5 * 2.5;
    if !inner && d2 >= 3.5 * 3.5 {
        return None;
    }
    Some(match d.y {
        y if y < 0 && inner => Block::BEDROCK,
        y if y < 0 => Block::END_STONE,
        y if y > 0 => Block::AIR,
        _ if !inner => Block::BEDROCK,
        _ if active => Block::END_PORTAL,
        _ => Block::AIR,
    })
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
    fn gateways_ring_the_island_and_lead_far_out() {
        let generator = EndGen::new(42);
        let gateways = generator.gateways();
        let mut sorted = gateways.to_vec();
        sorted.sort_by_key(|p| (p.x, p.z));
        sorted.dedup();
        assert_eq!(sorted.len(), 20);
        for g in gateways {
            assert!(((g.x * g.x + g.z * g.z) as f64).sqrt().round() as i32 - 96 <= 1);
            assert_eq!(g.y, 75);
        }
        assert_ne!(EndGen::new(43).gateways(), gateways);
        for g in &gateways[..5] {
            let (exit, island) = generator.gateway_exit(*g);
            let r = ((exit.x * exit.x + exit.z * exit.z) as f64).sqrt();
            assert!((768.0..=1300.0).contains(&r), "exit {exit} at {r}");
            if !island {
                assert_eq!(generator.column(exit.x, exit.z).map(|(top, _)| top + 10), Some(exit.y));
            }
        }
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
