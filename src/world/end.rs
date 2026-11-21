//! Seeded End terrain: a central island, ten obsidian pillars, a void ring and outer islands.
//! Column shapes and structures use world coordinates so chunk order cannot create seams.
use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, index};
use super::noise::{Perlin, hash3};
use glam::{IVec2, IVec3};
use std::sync::{Arc, Mutex};

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
    /// Generation 5 (v0.16 Track G): Java's `EndSpikeFeature.placeSpike`
    /// shape (`dx² + dz² <= r² + 1`, and air above y 65 in the rest of
    /// its square) rather than the original round `dx² + dz² <= r²`.
    pub java: bool,
}

impl Pillar {
    /// Whether the column `x, z` is part of the pillar's obsidian.
    pub fn holds(&self, x: i32, z: i32) -> bool {
        let (dx, dz) = (x - self.x, z - self.z);
        let r2 = self.radius * self.radius + i32::from(self.java);
        dx.abs() <= self.radius && dz.abs() <= self.radius && dx * dx + dz * dz <= r2
    }

    /// The block `placeSpike` leaves at `p` (with the cage, bedrock cap and
    /// the cleared air above y 65 in its square), or `None` outside it.
    /// Generations before 5 only ever build the round pillar this way when
    /// the respawn ritual rebuilds it.
    pub fn block_at(&self, p: IVec3) -> Option<Block> {
        let (dx, dz) = (p.x - self.x, p.z - self.z);
        if self.caged(p.x, p.y, p.z) {
            return Some(Block::IRON_BARS);
        }
        if dx.abs() > self.radius || dz.abs() > self.radius || p.y > self.top + 10 {
            return None;
        }
        if dx == 0 && dz == 0 && p.y == self.top {
            return Some(Block::BEDROCK);
        }
        if self.holds(p.x, p.z) && p.y < self.top {
            return Some(Block::OBSIDIAN);
        }
        (p.y > 65).then_some(Block::AIR)
    }

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
/// Java's `EndSpikeFeature.SpikeCacheLoader` for a world seed: the cache
/// key is the first `nextLong` of the seed's `SingleThreadedRandomSource`
/// (low 16 bits), the sizes 0..10 shuffled by `Util.toShuffledList` with a
/// source seeded by that key, and the spikes at `floor(42 cos)`,
/// `floor(42 sin)` of `2(-π + π/10 i)`.
pub fn java_spikes(seed: u64) -> [Pillar; 10] {
    let key = crate::enchant::JavaRandom::new(seed as i64).next_long() & 0xFFFF;
    let mut sizes: [i32; 10] = std::array::from_fn(|i| i as i32);
    java_shuffle(&mut sizes, &mut crate::enchant::JavaRandom::new(key));
    std::array::from_fn(|i| {
        let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 10.0 * i as f64);
        let size = sizes[i];
        Pillar {
            x: (42.0 * a.cos()).floor() as i32,
            z: (42.0 * a.sin()).floor() as i32,
            radius: 2 + size / 3,
            top: 76 + size * 3,
            guarded: size == 1 || size == 2,
            java: true,
        }
    })
}

/// `Util.shuffle`: from the end, swap each slot with one drawn below it.
fn java_shuffle<T>(list: &mut [T], rng: &mut crate::enchant::JavaRandom) {
    for i in (2..=list.len()).rev() {
        let j = rng.next_bounded(i as i32) as usize;
        list.swap(i - 1, j);
    }
}

/// `EnderDragonFight.init`'s gateway list, `0..20` shuffled by the world
/// seed's `SingleThreadedRandomSource`, in the order kills use it
/// (`spawnNewGateway` takes from the end).
pub fn java_gateway_order(seed: u64) -> [usize; 20] {
    let mut list: [usize; 20] = std::array::from_fn(|i| i);
    java_shuffle(&mut list, &mut crate::enchant::JavaRandom::new(seed as i64));
    list.reverse();
    list
}

/// Where the gateway numbered `i` (0..20) goes: `floor(96 cos)`,
/// `floor(96 sin)` of `2(-π + π/20 i)`, at y 75.
pub fn gateway_spot(i: usize) -> IVec3 {
    let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * i as f64);
    IVec3::new((96.0 * a.cos()).floor() as i32, 75, (96.0 * a.sin()).floor() as i32)
}

pub struct EndGen {
    shape: Perlin,
    detail: Perlin,
    seed: u64,
    pillars: [Pillar; 10],
    /// The generation the world was made with: generation 5 adds End
    /// biomes, chorus plants, and End cities and ships (v0.16 Track A).
    generation: super::terrain::Generation,
    /// v0.16 Track A: End cities and ships (generation 5).
    pub cities: super::end_city::Cities,
    /// Chorus plants grown per 16-block chunk, cached (generation 5).
    chorus: Mutex<rustc_hash::FxHashMap<IVec2, Plants>>,
}

/// The blocks of the chorus plants grown in one 16-block chunk.
type Plants = Arc<Vec<(IVec3, Block)>>;

/// v0.16 Track A: Java's End biomes (`TheEndBiomeSource`), generation 5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndBiome {
    TheEnd,
    Highlands,
    Midlands,
    SmallIslands,
    Barrens,
}

impl EndBiome {
    pub fn name(self) -> &'static str {
        match self {
            EndBiome::TheEnd => "the_end",
            EndBiome::Highlands => "end_highlands",
            EndBiome::Midlands => "end_midlands",
            EndBiome::SmallIslands => "small_end_islands",
            EndBiome::Barrens => "end_barrens",
        }
    }
}

/// `TheEndBiomeSource.getNoiseBiome`'s thresholds on the height value.
pub const HIGHLANDS_ABOVE: f64 = 0.25;
pub const MIDLANDS_FROM: f64 = -0.0625;
pub const SMALL_ISLANDS_BELOW: f64 = -0.21875;
/// Chunks within this squared distance (in chunks) of the centre are `the_end`.
pub const CENTRE_CHUNKS_SQUARED: i64 = 4096;
const CHORUS_CACHE: usize = 256;

impl EndGen {
    pub fn new(seed: u64) -> Self {
        Self::with_generation(seed, super::terrain::Generation::V4)
    }

    pub fn with_generation(seed: u64, generation: super::terrain::Generation) -> Self {
        let pillars = if generation >= super::terrain::Generation::V5 {
            java_spikes(seed)
        } else {
            let mut ranks: [usize; 10] = std::array::from_fn(|i| i);
            ranks.sort_unstable_by_key(|&i| hash3(i as i32, 0, 0, seed ^ 0x454E44));
            std::array::from_fn(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 10.0;
                let rank = ranks[i] as i32;
                Pillar {
                    x: (angle.cos() * 42.0).round() as i32,
                    z: (angle.sin() * 42.0).round() as i32,
                    radius: 2 + rank / 3,
                    top: 76 + rank * 3,
                    guarded: rank == 1 || rank == 2,
                    java: false,
                }
            })
        };
        Self {
            shape: Perlin::new(seed ^ 0x454E4401),
            detail: Perlin::new(seed ^ 0x454E4402),
            seed,
            pillars,
            generation,
            cities: super::end_city::Cities::new(seed),
            chorus: Mutex::new(rustc_hash::FxHashMap::default()),
        }
    }

    fn v5(&self) -> bool {
        self.generation >= super::terrain::Generation::V5
    }

    pub fn pillars(&self) -> &[Pillar; 10] {
        &self.pillars
    }

    /// The exit portal's centre (Java's `EndPodiumFeature` origin): the
    /// cell above the central island's surface at 0, 0. Generation 5 (v0.16
    /// Track G) puts it where `spawnExitPortal` does, on the surface block
    /// itself (`MOTION_BLOCKING_NO_LEAVES` heightmap, one down).
    pub fn podium(&self) -> IVec3 {
        let top = self.column(0, 0).map_or(SPAWN.y, |(top, _)| top);
        IVec3::new(0, top + i32::from(!self.v5()), 0)
    }
    /// The 20 spots around the central island where a gateway appears for
    /// each dragon killed, in the order they're used (Java shuffles them by
    /// the world seed).
    ///
    /// Generation 5 (v0.16 Track G) uses Java's own order for the seed
    /// ([`java_gateway_order`]); older worlds keep the original order so
    /// their saved gateway counts still name the same spots.
    pub fn gateways(&self) -> [IVec3; 20] {
        if self.v5() {
            return java_gateway_order(self.seed).map(gateway_spot);
        }
        let mut order: [usize; 20] = std::array::from_fn(|i| i);
        order.sort_unstable_by_key(|&i| hash3(i as i32, 1, 0, self.seed ^ 0x4741_5445));
        order.map(gateway_spot)
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
            self.outer_mass(x, z, rough)
        };
        if mass <= 0.0 {
            return None;
        }
        let noise = self.shape.noise2(x as f32 / 80.0, z as f32 / 80.0);
        let top = (56.0 + mass.min(1.0) * 10.0 + noise * 5.0).floor() as i32;
        let depth = (mass.sqrt() * 30.0 + rough * 3.0).max(2.0) as i32;
        Some((top, top - depth))
    }

    /// The outer islands' mass at `(x, z)`: above 0 is land. Islands sit
    /// on a 128-block grid, four cells in five holding one.
    fn outer_mass(&self, x: i32, z: i32, rough: f32) -> f32 {
        {
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
        }
    }

    /// v0.16 Track A: the End biome at block `(x, z)` (generation 5).
    /// Like `TheEndBiomeSource`, biomes are per 16-block chunk: chunks within
    /// 64 of the centre are `the_end`, others are chosen from a height value
    /// sampled at the chunk's middle. Java's value is `EndIslandFunction`'s
    /// `(doffs - 8) / 128`; VoxelCraft's islands are original (FE-01), so
    /// the same thresholds read `doffs` as 100 times the island mass
    /// (clamped to Java's -100..80 like `doffs`).
    pub fn biome(&self, x: i32, z: i32) -> EndBiome {
        let (cx, cz) = (x >> 4, z >> 4);
        if i64::from(cx) * i64::from(cx) + i64::from(cz) * i64::from(cz) <= CENTRE_CHUNKS_SQUARED {
            return EndBiome::TheEnd;
        }
        let (bx, bz) = ((cx * 2 + 1) * 8, (cz * 2 + 1) * 8);
        let rough = self.detail.noise2(bx as f32 / 32.0, bz as f32 / 32.0);
        let doffs = (f64::from(self.outer_mass(bx, bz, rough)) * 100.0).clamp(-100.0, 80.0);
        let value = (doffs - 8.0) / 128.0;
        if value > HIGHLANDS_ABOVE {
            EndBiome::Highlands
        } else if value >= MIDLANDS_FROM {
            EndBiome::Midlands
        } else if value < SMALL_ISLANDS_BELOW {
            EndBiome::SmallIslands
        } else {
            EndBiome::Barrens
        }
    }

    /// The chorus plants Java's `chorus_plant` placed feature grows in the
    /// 16-block chunk `chunk` (generation 5): `count` uniform 0..=4,
    /// `in_square`, `heightmap MOTION_BLOCKING`, End highlands only; each
    /// plant (`ChorusPlantFeature`) needs air with End stone below and grows
    /// with `ChorusFlowerBlock.generatePlant(.., 8)`. The draws come from a
    /// seeded `JavaRandom` per chunk, not Java's decoration seed (the
    /// terrain is original, so Java's positions can't carry over).
    pub fn chorus_plants(&self, chunk: IVec2) -> Arc<Vec<(IVec3, Block)>> {
        if let Some(found) = self.chorus.lock().unwrap().get(&chunk) {
            return Arc::clone(found);
        }
        let mut out: Vec<(IVec3, Block)> = Vec::new();
        if self.biome(chunk.x * 16, chunk.y * 16) == EndBiome::Highlands {
            let salt = hash3(chunk.x, 0x0C40, chunk.y, self.seed ^ 0x0C40_5E50);
            let mut rng = crate::enchant::JavaRandom::new(salt as i64);
            let count = rng.next_bounded(5);
            for _ in 0..count {
                let x = chunk.x * 16 + rng.next_bounded(16);
                let z = chunk.y * 16 + rng.next_bounded(16);
                let Some((top, _)) = self.column(x, z) else { continue };
                let origin = IVec3::new(x, top + 1, z);
                if out.iter().any(|(p, _)| *p == origin) {
                    continue;
                }
                let placed = out.clone();
                let empty = |p: IVec3| {
                    self.column(p.x, p.z).is_none_or(|(top, bottom)| p.y > top || p.y < bottom)
                        && !placed.iter().any(|(q, _)| *q == p)
                };
                let mut int = |n: u32| rng.next_bounded(n as i32) as u32;
                out.extend(super::end_blocks::generate_plant(origin, super::end_blocks::MAX_SPREAD, &empty, &mut int));
            }
        }
        let found = Arc::new(out);
        let mut cache = self.chorus.lock().unwrap();
        if cache.len() >= CHORUS_CACHE
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        Arc::clone(cache.entry(chunk).or_insert(found))
    }

    /// The End cities whose blocks may reach the blocks `min..=max` (x, z).
    pub fn cities_near(&self, min: IVec2, max: IVec2) -> Vec<Arc<super::end_city::City>> {
        if !self.v5() {
            return Vec::new();
        }
        self.cities.around(self, min, max)
    }

    /// Chests generation 5's End cities put in the chunk at `cpos`.
    pub fn structure_features(&self, cpos: IVec3) -> Vec<(IVec3, super::fortress::Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let min = IVec2::new(base.x, base.z);
        let max = min + IVec2::splat(CHUNK_SIZE_I - 1);
        self.cities_near(min, max).iter().flat_map(|c| c.features_in(cpos).collect::<Vec<_>>()).collect()
    }

    /// `/locate structure end_city`.
    pub fn nearest_city(&self, origin: IVec2, max_blocks: i32) -> Option<IVec3> {
        if !self.v5() {
            return None;
        }
        self.cities.nearest(self, origin, max_blocks)
    }

    /// Generation 5's additions to the chunk at `base`: chorus plants, then
    /// End cities over them.
    fn paint_v5(&self, blocks: &mut [Block], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let reach = super::end_blocks::MAX_SPREAD;
        let lo = IVec2::new(base.x - reach, base.z - reach).div_euclid(IVec2::splat(16));
        let hi = IVec2::new(top.x + reach, top.z + reach).div_euclid(IVec2::splat(16));
        for cz in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                for &(p, b) in self.chorus_plants(IVec2::new(cx, cz)).iter() {
                    if p.cmpge(base).all() && p.cmple(top).all() {
                        let l = p - base;
                        blocks[index(l.x as usize, l.y as usize, l.z as usize)] = b;
                    }
                }
            }
        }
        for city in self.cities_near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            city.paint(blocks, base);
        }
    }
    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        let base = cpos * CHUNK_SIZE_I;
        if base.y < 0 || base.y > 104 {
            if self.v5() && base.y >= 0 {
                let mut blocks = ChunkData::new_dense(Block::AIR);
                self.paint_v5(&mut blocks[..], base);
                return ChunkData::from_dense(blocks);
            }
            return ChunkData::Uniform(Block::AIR);
        }
        let mut blocks = ChunkData::new_dense(Block::AIR);
        let podium = self.podium();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let column = self.column(wx, wz);
                let pillar = self.pillars.iter().find(|p| p.holds(wx, wz));
                // Generation 5: `placeSpike` also clears its square above y 65.
                let square = self
                    .pillars
                    .iter()
                    .find(|p| p.java && (wx - p.x).abs() <= p.radius && (wz - p.z).abs() <= p.radius);
                // Cages reach past the round pillar, to its 5x5 square.
                let cage = self.pillars.iter().find(|p| p.guarded && (wx - p.x).abs() <= 2 && (wz - p.z).abs() <= 2);
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
                    } else if cage.is_some_and(|p| p.caged(wx, wy, wz)) {
                        Block::IRON_BARS
                    } else if square.is_some_and(|p| wy > 65 && wy <= p.top + 10) {
                        Block::AIR
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
        if self.v5() {
            self.paint_v5(&mut blocks[..], base);
        }
        ChunkData::from_dense(blocks)
    }
}
impl super::end_city::Terrain for EndGen {
    fn city_biome(&self, x: i32, z: i32) -> bool {
        matches!(self.biome(x, z), EndBiome::Highlands | EndBiome::Midlands)
    }

    fn first_free(&self, x: i32, z: i32) -> i32 {
        self.column(x, z).map_or(0, |(top, _)| top + 1)
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
    fn guarded_pillars_are_fully_caged() {
        let generator = EndGen::new(42);
        let guarded: Vec<_> = generator.pillars.iter().filter(|p| p.guarded).collect();
        assert_eq!(guarded.len(), 2);
        for p in guarded {
            for dy in 0..=3 {
                for dz in -2..=2 {
                    for dx in -2..=2 {
                        let pos = IVec3::new(p.x + dx, p.top + dy, p.z + dz);
                        let local = super::super::chunk::local_of(pos);
                        let block = generator.generate(super::super::chunk::chunk_of(pos)).get(
                            local.x as usize,
                            local.y as usize,
                            local.z as usize,
                        );
                        let bars = dx.abs() == 2 || dz.abs() == 2 || dy == 3;
                        if bars {
                            assert_eq!(block, Block::IRON_BARS, "{pos}");
                        }
                    }
                }
            }
        }
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
            let found = super::super::end_portal::gateway_exit(&generator, &|_| None, *g, 256);
            let exit = found.exit;
            let r = ((exit.x * exit.x + exit.z * exit.z) as f64).sqrt();
            assert!((768.0..=1300.0).contains(&r), "exit {exit} at {r}");
            if found.island.is_none() {
                // Ten above the tallest land within 16 (from the original
                // islands' columns).
                let tallest = (-16..=16)
                    .flat_map(|dx| (-16..=16).map(move |dz| (dx, dz)))
                    .filter_map(|(dx, dz)| generator.column(exit.x + dx, exit.z + dz).map(|(top, _)| top))
                    .max();
                assert!(tallest.is_some_and(|t| t + 10 <= exit.y), "exit {exit} over {tallest:?}");
            }
        }
    }

    #[test]
    fn generation_5_uses_javas_spikes_and_gateway_order() {
        // `EndSpikeFeature`'s spike positions are fixed; sizes and cages
        // come from the seed's shuffle.
        let spikes = java_spikes(0);
        let xz: Vec<(i32, i32)> = spikes.iter().map(|p| (p.x, p.z)).collect();
        assert_eq!(
            xz,
            vec![
                (42, 0),
                (33, 24),
                (12, 39),
                (-13, 39),
                (-34, 24),
                (-42, -1),
                (-34, -25),
                (-13, -40),
                (12, -40),
                (33, -25)
            ]
        );
        let mut sizes: Vec<i32> = spikes.iter().map(|p| (p.top - 76) / 3).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, (0..10).collect::<Vec<_>>());
        assert!(
            spikes.iter().all(|p| p.radius == 2 + (p.top - 76) / 9 && p.guarded == matches!((p.top - 76) / 3, 1 | 2))
        );
        let v5 = EndGen::with_generation(0, super::super::terrain::Generation::V5);
        assert_eq!(v5.pillars(), &spikes);
        // The gateway list is every spot once, used from its shuffled end.
        let order = java_gateway_order(0);
        let mut sorted = order.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_eq!(v5.gateways()[0], gateway_spot(order[0]));
        assert_eq!(gateway_spot(0), IVec3::new(96, 75, 0));
        assert_eq!(gateway_spot(5), IVec3::new(-1, 75, 96));
        // Generation 4 keeps its own order and round pillars.
        assert!(EndGen::new(0).pillars().iter().all(|p| !p.java));
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
