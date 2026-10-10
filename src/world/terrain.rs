//! Procedural terrain generation.
//!
//! Every chunk is generated independently from the seed (no cross-chunk
//! state), so generation parallelises trivially across worker threads.
//! Features that straddle chunk borders, such as trees, are placed
//! deterministically from hashed world coordinates and each chunk writes
//! only its own part of them.
//!
//! The Overworld follows Java 1.18+: climate noise picks one of ~50 biomes
//! (see [`super::biome`]) and shapes a surface from y = -64 up to peaks
//! above y = 200 (see [`super::climate`]). Underground, noise caves,
//! ravines and aquifers open the rock (see [`super::caves`]), deepslate
//! fills everything below y = 0, and lush and dripstone caves grow under
//! humid and far-inland ground. Each biome then gets its surface, trees,
//! plants, ocean vegetation, ice and snow.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::caves::{Aquifer, Caves};
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, index};
use super::climate::ClimateNoise;
use super::noise::{Perlin, hash_f, hash3};
use super::overworld_blocks::{self as ob, Thickness};
use super::trees::{self, *};

pub use super::biome::{Biome, Climate};
pub use super::caves::LAVA_LEVEL;
pub use super::climate::SEA_LEVEL;

const TREE_CELL: i32 = 5;
const TREE_REACH: i32 = trees::REACH;
const TREE_TOP: i32 = trees::TOP;
/// Lowest Overworld block.
const BOTTOM: i32 = super::chunk::WORLD_MIN_Y;
/// Margin of columns kept around a chunk so slopes can be measured.
const M: usize = 1;
const W: usize = CHUNK_SIZE + 2 * M;

#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub height: i32,
    pub biome: Biome,
    /// Cold enough for the sea and rivers to freeze over.
    pub frozen: bool,
    pub climate: Climate,
}

/// Which world a generator, world or save belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dimension {
    #[default]
    Overworld,
    Nether,
    End,
}

impl Dimension {
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Overworld => "overworld",
            Dimension::Nether => "nether",
            Dimension::End => "end",
        }
    }

    pub fn from_name(name: &str) -> Option<Dimension> {
        [Dimension::Overworld, Dimension::Nether, Dimension::End].into_iter().find(|d| d.name() == name)
    }

    /// The other side of a Nether portal.
    pub fn other(self) -> Dimension {
        match self {
            Dimension::Overworld => Dimension::Nether,
            Dimension::Nether | Dimension::End => Dimension::Overworld,
        }
    }

    /// Whether the dimension has a sky (and with it skylight, weather and
    /// a day/night cycle).
    pub fn has_sky(self) -> bool {
        self == Dimension::Overworld
    }

    /// Lowest block y. Java's Overworld runs from -64; the Nether and the
    /// End start at 0.
    pub const fn min_y(self) -> i32 {
        match self {
            Dimension::Overworld => super::chunk::WORLD_MIN_Y,
            Dimension::Nether | Dimension::End => 0,
        }
    }

    /// One past the highest block y: 320 in the Overworld, 256 elsewhere.
    pub const fn max_y(self) -> i32 {
        match self {
            Dimension::Overworld => super::chunk::WORLD_MAX_Y,
            Dimension::Nether | Dimension::End => 256,
        }
    }

    /// Chunk rows of a column, bottom to top.
    pub const fn chunk_rows(self) -> std::ops::Range<i32> {
        (self.min_y() >> super::chunk::CHUNK_BITS)..(self.max_y() >> super::chunk::CHUNK_BITS)
    }

    /// How many chunks a column holds.
    pub const fn column_chunks(self) -> i32 {
        (self.max_y() - self.min_y()) >> super::chunk::CHUNK_BITS
    }

    /// Whether block height `y` is inside the world.
    pub const fn contains_y(self, y: i32) -> bool {
        y >= self.min_y() && y < self.max_y()
    }
}

/// Surface noise is independent of chunk Y. Keep a bounded set of column
/// snapshots shared by generation workers, rather than resampling all
/// twelve vertical chunks. Values are immutable and eviction never changes
/// output.
const COLUMN_CACHE_LIMIT: usize = 512;
struct ChunkColumns {
    /// Columns of the chunk and a one-block ring around it, `[z][x]`.
    cols: [[Column; W]; W],
    max_h: i32,
    min_h: i32,
}

impl ChunkColumns {
    #[inline]
    fn at(&self, x: usize, z: usize) -> &Column {
        &self.cols[z + M][x + M]
    }

    /// Largest height step to a neighbouring column (Java's steep rule).
    fn steepness(&self, x: usize, z: usize) -> i32 {
        let h = |dx: usize, dz: usize| self.cols[z + dz][x + dx].height;
        (h(2, 1) - h(0, 1)).abs().max((h(1, 2) - h(1, 0)).abs())
    }
}

pub struct Generator {
    pub seed: u64,
    pub dimension: Dimension,
    /// Set for the Nether, which generates from its own noise.
    nether: Option<super::nether::NetherGen>,
    end: Option<super::end::EndGen>,
    /// The overworld's strongholds.
    pub strongholds: super::stronghold::Strongholds,
    /// Monster rooms are terrain features and can cross chunk boundaries.
    pub dungeons: super::dungeon::Dungeons,
    /// Abandoned mineshafts, placed per Java 16×16 chunk.
    pub mineshafts: super::mineshaft::Mineshafts,
    pub villages: super::village::Villages,
    /// Temples, huts, igloos, outposts, shipwrecks, ruins and monuments.
    pub temples: super::temples::Temples,
    columns: Mutex<FxHashMap<IVec2, Arc<ChunkColumns>>>,
    climate: ClimateNoise,
    caves: Caves,
    /// Surface patches: podzol, gravel, coarse dirt, moss.
    patch: Perlin,
    /// Badlands hoodoos, icebergs and other column features.
    feature: Perlin,
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Badlands strata, bottom to top, as terracotta colours (see
/// [`Block::terracotta`]); the pattern repeats every `BANDS.len()` blocks.
const BANDS: [u8; 16] = [1, 1, 0, 2, 0, 0, 3, 1, 0, 4, 0, 5, 0, 1, 6, 0];

/// Natural rock that cave plants and surface rules may cover.
fn is_rock(b: Block) -> bool {
    matches!(b, Block::STONE | Block::DEEPSLATE | Block::GRANITE | Block::DIORITE | Block::ANDESITE | Block::TUFF)
        || b == Block::DIRT
        || b == Block::GRAVEL
}

impl Generator {
    pub fn new(seed: u64) -> Self {
        Self::for_dimension(seed, Dimension::Overworld)
    }

    pub fn for_dimension(seed: u64, dimension: Dimension) -> Self {
        let p = |salt: u64| Perlin::new(seed ^ salt.wrapping_mul(0x2545_F491_4F6C_DD1D));
        Self {
            seed,
            dimension,
            nether: (dimension == Dimension::Nether).then(|| super::nether::NetherGen::new(seed)),
            end: (dimension == Dimension::End).then(|| super::end::EndGen::new(seed)),
            strongholds: super::stronghold::Strongholds::new(seed),
            dungeons: super::dungeon::Dungeons::new(seed),
            mineshafts: super::mineshaft::Mineshafts::new(seed),
            villages: super::village::Villages::new(seed),
            temples: super::temples::Temples::new(seed),
            columns: Mutex::new(FxHashMap::default()),
            climate: ClimateNoise::new(seed),
            caves: Caves::new(seed),
            patch: p(31),
            feature: p(32),
        }
    }

    /// The End's layout (pillars, exit portal), in the End only.
    pub fn end(&self) -> Option<&super::end::EndGen> {
        self.end.as_ref()
    }

    /// The Nether biome of column `(x, z)`, when this is a Nether generator.
    pub fn nether_biome(&self, x: i32, z: i32) -> Option<super::nether_biome::NetherBiome> {
        Some(self.nether.as_ref()?.biomes.biome(x, z))
    }

    /// Nearest column of a Nether biome (`/locate biome` in the Nether).
    pub fn nearest_nether_biome(
        &self,
        origin: IVec3,
        target: super::nether_biome::NetherBiome,
        max_blocks: i32,
    ) -> Option<IVec3> {
        self.nether.as_ref()?.biomes.nearest(origin, target, max_blocks)
    }

    /// Nearest Nether fortress, when this is a Nether generator.
    pub fn nearest_fortress(&self, p: glam::IVec2) -> Option<IVec3> {
        self.nether.as_ref()?.fortresses.nearest(p)
    }

    /// The biome at a block: a cave biome underground where one grows,
    /// otherwise the column's surface biome.
    pub fn biome_at(&self, p: IVec3) -> Biome {
        let col = self.column(p.x, p.z);
        super::biome::cave(&col.climate, col.height - p.y).unwrap_or(col.biome)
    }

    /// Nearest column of `target`, searching outward from `origin` in 32-block
    /// steps (Java `/locate biome` uses a similar spiral; capped for speed).
    /// Cave biomes are found 40 blocks under the surface.
    pub fn nearest_biome(&self, origin: IVec2, target: Biome, max_blocks: i32) -> Option<IVec3> {
        if self.dimension != Dimension::Overworld {
            return None;
        }
        let here = |x, z| {
            let c = self.column(x, z);
            if target.is_cave() { super::biome::cave(&c.climate, 40) == Some(target) } else { c.biome == target }
        };
        let found = |x, z| {
            let h = self.column(x, z).height;
            IVec3::new(x, if target.is_cave() { h - 40 } else { h + 1 }, z)
        };
        if here(origin.x, origin.y) {
            return Some(found(origin.x, origin.y));
        }
        let steps = (max_blocks / 32).max(1);
        for ring in 1..=steps {
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let (x, z) = (origin.x + dx * 32, origin.y + dz * 32);
                    if here(x, z) {
                        return Some(found(x, z));
                    }
                }
            }
        }
        None
    }

    /// Surface height and biome of a world column.
    pub fn column(&self, x: i32, z: i32) -> Column {
        if let Some(end) = &self.end {
            return Column {
                height: end.column(x, z).map_or(-1, |(top, _)| top),
                biome: Biome::Plains,
                frozen: false,
                climate: Climate::default(),
            };
        }
        let climate = self.climate.sample(x, z);
        let mut h = self.climate.height(&climate, x, z);
        let biome = super::biome::pick(&climate);
        let (fx, fz) = (x as f32, z as f32);
        if biome.is_badlands() && h > SEA_LEVEL as f32 + 6.0 {
            // Badlands: terraced cliffs, and in eroded badlands spires.
            let t = h / 7.0;
            let terraced = (t.floor() + smoothstep(0.6, 0.9, t.fract())) * 7.0;
            h = h + (terraced - h) * 0.8;
            if biome == Biome::ErodedBadlands {
                let n = self.feature.fbm2(fx / 11.0, fz / 11.0, 2);
                h += ((n - 0.18) * 70.0).clamp(0.0, 16.0);
            }
        }
        let height = (h as i32).clamp(BOTTOM + 6, 300);
        Column { height, biome, frozen: biome.is_cold(), climate }
    }

    /// Foliage colour group (see [`Biome::foliage`]) of every column in
    /// chunk column `(cx, cz)`, indexed `x + z * CHUNK_SIZE`. Each column
    /// samples the biome a few blocks off at random, so colours dither
    /// into each other across biome borders instead of changing in a line.
    pub fn foliage(&self, cx: i32, cz: i32) -> Box<[u8; CHUNK_SIZE * CHUNK_SIZE]> {
        if !self.dimension.has_sky() {
            return Box::new([0; CHUNK_SIZE * CHUNK_SIZE]);
        }
        let mut out = Box::new([0u8; CHUNK_SIZE * CHUNK_SIZE]);
        for (i, f) in out.iter_mut().enumerate() {
            let (x, z) = (cx * CHUNK_SIZE_I + (i % CHUNK_SIZE) as i32, cz * CHUNK_SIZE_I + (i / CHUNK_SIZE) as i32);
            let h = hash3(x, 7, z, self.seed ^ 0xF01);
            let (dx, dz) = ((h % 9) as i32 - 4, ((h >> 8) % 9) as i32 - 4);
            *f = self.climate_biome(x + dx, z + dz).foliage();
        }
        out
    }

    /// Where snow starts in every column of chunk column `(cx, cz)`,
    /// indexed `x + z * CHUNK_SIZE`: Java's biome temperature falls below
    /// 0.15 from this height up (it cools above y = 80). `i16::MIN` means
    /// snow at any height, `i16::MAX` a dry biome with no precipitation.
    pub fn snow_lines(&self, cx: i32, cz: i32) -> Box<[i16; CHUNK_SIZE * CHUNK_SIZE]> {
        let mut out = Box::new([i16::MAX; CHUNK_SIZE * CHUNK_SIZE]);
        if !self.dimension.has_sky() {
            return out;
        }
        for (i, line) in out.iter_mut().enumerate() {
            let (x, z) = (cx * CHUNK_SIZE_I + (i % CHUNK_SIZE) as i32, cz * CHUNK_SIZE_I + (i / CHUNK_SIZE) as i32);
            let biome = self.climate_biome(x, z);
            *line = if biome.is_dry() {
                i16::MAX
            } else if biome.is_cold() {
                i16::MIN
            } else {
                // temperature_at(y) = t - (y - 80) * 0.05 / 40 < 0.15
                (80.0 + (biome.temperature() - 0.15) * 800.0).min(i16::MAX as f32 - 1.0) as i16
            };
        }
        out
    }

    /// The biome alone, without the height (cheaper than [`Generator::column`]).
    fn climate_biome(&self, x: i32, z: i32) -> Biome {
        super::biome::pick(&self.climate.sample(x, z))
    }

    /// Whether a column under water gets a patch of clay on its floor.
    fn clay_patch(&self, x: i32, z: i32) -> bool {
        hash3(x >> 2, 0, z >> 2, self.seed ^ 0xC1A).is_multiple_of(7) && hash_f(x, 1, z, self.seed ^ 0xC1B) < 0.8
    }

    fn patch_at(&self, x: i32, z: i32) -> f32 {
        self.patch.fbm2(x as f32 / 18.0, z as f32 / 18.0, 2)
    }

    /// The block at the very top of a column, before caves and decoration.
    /// `steep` is the largest height step to a neighbour.
    fn surface_block(&self, col: Column, x: i32, z: i32, steep: i32) -> Block {
        use Biome::*;
        let y = col.height;
        let wet = y < SEA_LEVEL;
        let patch = self.patch_at(x, z);
        if wet && col.biome.is_watery() && y >= SEA_LEVEL - 6 && self.clay_patch(x, z) {
            return Block::CLAY;
        }
        if wet {
            return match col.biome {
                WarmOcean | LukewarmOcean | DeepLukewarmOcean | Beach | MushroomFields => Block::SAND,
                b if b.is_deep_ocean() => Block::GRAVEL,
                b if b.is_ocean() => {
                    if y >= SEA_LEVEL - 10 && patch > -0.2 {
                        Block::SAND
                    } else {
                        Block::GRAVEL
                    }
                }
                River | FrozenRiver => {
                    if y < SEA_LEVEL - 3 && patch < 0.0 {
                        Block::GRAVEL
                    } else {
                        Block::SAND
                    }
                }
                Swamp => Block::DIRT,
                MangroveSwamp => ob::MUD,
                Desert => Block::SAND,
                b if b.is_badlands() => Block::RED_SAND,
                _ => {
                    if y >= SEA_LEVEL - 3 {
                        Block::SAND
                    } else {
                        Block::GRAVEL
                    }
                }
            };
        }
        let mountain = col.biome.is_mountain() || col.biome.is_peak();
        if steep >= 4 && (mountain || col.biome == StonyShore) {
            return if col.biome == FrozenPeaks { super::gadgets::PACKED_ICE } else { Block::STONE };
        }
        match col.biome {
            Beach | Desert => Block::SAND,
            SnowyBeach => Block::SAND,
            StonyShore => {
                if patch > 0.3 {
                    Block::GRAVEL
                } else {
                    Block::STONE
                }
            }
            b if b.is_badlands() => {
                if b == WoodedBadlands && y > 96 {
                    if patch > 0.0 { ob::COARSE_DIRT } else { Block::GRASS }
                } else if y < SEA_LEVEL + 14 || hash_f(x, 2, z, self.seed ^ 0xBAD) < 0.3 {
                    Block::RED_SAND
                } else {
                    self.stratum(y)
                }
            }
            MushroomFields => ob::MYCELIUM,
            MangroveSwamp => ob::MUD,
            JaggedPeaks | SnowySlopes => Block::SNOW,
            FrozenPeaks => {
                if patch > 0.1 {
                    super::gadgets::PACKED_ICE
                } else {
                    Block::SNOW
                }
            }
            StonyPeaks => {
                if patch > 0.25 {
                    Block::CALCITE
                } else {
                    Block::STONE
                }
            }
            WindsweptGravellyHills => {
                if patch > -0.1 {
                    Block::GRAVEL
                } else if patch > -0.4 {
                    Block::STONE
                } else {
                    Block::GRASS
                }
            }
            WindsweptHills if patch > 0.35 => Block::STONE,
            OldGrowthPineTaiga | OldGrowthSpruceTaiga => {
                if patch > 0.25 {
                    ob::COARSE_DIRT
                } else if patch > -0.15 {
                    ob::PODZOL
                } else {
                    Block::GRASS
                }
            }
            BambooJungle if patch > 0.2 => ob::PODZOL,
            PaleGarden if patch > 0.35 => ob::PALE_MOSS_BLOCK,
            _ if col.biome.temperature_at(y) < 0.15 => Block::SNOWY_GRASS,
            _ => Block::GRASS,
        }
    }

    fn filler_block(&self, col: Column, y: i32) -> Block {
        use Biome::*;
        match col.biome {
            Beach | SnowyBeach | WarmOcean | LukewarmOcean | DeepLukewarmOcean => Block::SAND,
            b if b.is_ocean() || b.is_river() => {
                if col.height >= SEA_LEVEL - 10 {
                    Block::SAND
                } else {
                    Block::GRAVEL
                }
            }
            Desert => {
                if y > col.height - 4 {
                    Block::SAND
                } else {
                    Block::SANDSTONE
                }
            }
            b if b.is_badlands() => {
                if y > col.height - 2 && col.height < SEA_LEVEL + 14 {
                    Block::RED_SAND
                } else {
                    self.stratum(y)
                }
            }
            MangroveSwamp => ob::MUD,
            JaggedPeaks | SnowySlopes | FrozenPeaks if y > col.height - 3 => Block::SNOW,
            StonyShore | StonyPeaks | JaggedPeaks | FrozenPeaks | SnowySlopes | WindsweptGravellyHills => Block::STONE,
            _ => Block::DIRT,
        }
    }

    /// The terracotta band at height `y` in badlands.
    fn stratum(&self, y: i32) -> Block {
        let shift = (self.seed % BANDS.len() as u64) as i32;
        Block::terracotta(BANDS[(y + shift).rem_euclid(BANDS.len() as i32) as usize])
    }

    /// How deep the surface and filler layers go before stone.
    fn soil_depth(col: Column) -> i32 {
        match col.biome {
            Biome::Desert => 7,
            // Strata run down to below sea level so whole cliffs are banded.
            b if b.is_badlands() => (col.height - SEA_LEVEL + 6).max(4),
            _ => 4,
        }
    }

    /// Block entities generated structures put in the chunk at `cpos`
    /// (fortress spawners and loot chests).
    pub fn structure_features(&self, cpos: IVec3) -> Vec<(IVec3, super::fortress::Feature)> {
        match (&self.nether, self.dimension) {
            (Some(n), _) => n.fortresses.features(cpos),
            (None, Dimension::Overworld) => {
                let mut features = self.strongholds.features(cpos);
                features.extend(self.dungeons.features(self, cpos));
                features.extend(self.mineshafts.features(cpos));
                features.extend(self.villages.features(self, cpos));
                features.extend(self.temples.features(self, cpos));
                features
            }
            _ => Vec::new(),
        }
    }

    /// Whether `p` lies inside a generated structure where its own mobs
    /// spawn (Nether fortresses).
    pub fn in_fortress(&self, p: IVec3) -> bool {
        self.nether.as_ref().is_some_and(|n| n.fortresses.inside(p))
    }

    /// Bastion remnants within `r` blocks of `p` (none outside the Nether).
    pub fn bastions_near(&self, p: IVec3, r: i32) -> Vec<Arc<super::bastion::Bastion>> {
        self.nether.as_ref().map_or(Vec::new(), |n| n.fortresses.bastions.around(IVec2::new(p.x, p.z), r))
    }

    fn chunk_columns(&self, pos: IVec2) -> Arc<ChunkColumns> {
        if let Some(found) = self.columns.lock().unwrap().get(&pos) {
            return Arc::clone(found);
        }
        // Do noise work outside the lock, so unrelated columns run in parallel.
        let base = IVec3::new(pos.x * CHUNK_SIZE_I, 0, pos.y * CHUNK_SIZE_I) - IVec3::new(M as i32, 0, M as i32);
        let blank = Column { height: 0, biome: Biome::Plains, frozen: false, climate: Climate::default() };
        let mut cols = [[blank; W]; W];
        let mut max_h = i32::MIN;
        let mut min_h = i32::MAX;
        for (z, row) in cols.iter_mut().enumerate() {
            for (x, c) in row.iter_mut().enumerate() {
                *c = self.column(base.x + x as i32, base.z + z as i32);
                let inside = (M..M + CHUNK_SIZE).contains(&x) && (M..M + CHUNK_SIZE).contains(&z);
                if inside {
                    max_h = max_h.max(c.height);
                    min_h = min_h.min(c.height);
                }
            }
        }
        let found = Arc::new(ChunkColumns { cols, max_h, min_h });
        let mut cache = self.columns.lock().unwrap();
        if cache.len() >= COLUMN_CACHE_LIMIT
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        Arc::clone(cache.entry(pos).or_insert(found))
    }

    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        if let Some(end) = &self.end {
            return end.generate(cpos);
        }
        if let Some(nether) = &self.nether {
            return nether.generate(cpos);
        }
        let base = cpos * CHUNK_SIZE_I;
        let columns = self.chunk_columns(IVec2::new(cpos.x, cpos.z));
        let (max_h, min_h) = (columns.max_h, columns.min_h);
        let top = base.y + CHUNK_SIZE_I - 1;
        // Open sky: nothing (not even tree canopies or ice spikes) reaches this chunk.
        if base.y > (max_h + TREE_TOP).max(SEA_LEVEL + 40) {
            return ChunkData::Uniform(Block::AIR);
        }
        let mut blocks = ChunkData::new_dense(Block::AIR);
        self.fill(&mut blocks, base, &columns);
        if base.y <= max_h {
            self.carve(&mut blocks, base, &columns);
            // Vein origins can lie outside the chunk: use their real biome
            // so both chunks agree on the vein.
            let biome_at = |x: i32, z: i32| {
                let (lx, lz) = (x - base.x, z - base.z);
                if (0..CHUNK_SIZE_I).contains(&lx) && (0..CHUNK_SIZE_I).contains(&lz) {
                    columns.at(lx as usize, lz as usize).biome
                } else {
                    self.climate_biome(x, z)
                }
            };
            super::ore::paint(self.seed, blocks.as_mut(), base, biome_at);
            if base.y < max_h - 15 {
                self.decorate_caves(&mut blocks, base, &columns);
            }
        }
        if base.y <= max_h + TREE_TOP && top >= min_h - 40 {
            self.column_features(&mut blocks, base, &columns);
            self.place_trees(&mut blocks, base);
            self.place_plants(&mut blocks, base, &columns);
            self.freeze(&mut blocks, base, &columns);
        }
        if base.y < SEA_LEVEL {
            self.strongholds.paint(&mut blocks, base);
        }
        self.dungeons.paint(self, &mut blocks, base);
        self.mineshafts.paint(&mut blocks, base);
        self.villages.paint(self, &mut blocks, base);
        self.temples.paint(self, &mut blocks, base);
        ChunkData::from_dense(blocks)
    }

    /// Bedrock, stone and deepslate, soil, surface and the sea.
    fn fill(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        let top = base.y + CHUNK_SIZE_I - 1;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = *columns.at(x, z);
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let fill_top = col.height.max(SEA_LEVEL - 1).min(top);
                let depth = Self::soil_depth(col);
                let surface = (col.height >= base.y && col.height <= top)
                    .then(|| self.surface_block(col, wx, wz, columns.steepness(x, z)));
                for wy in base.y..=fill_top {
                    let y = (wy - base.y) as usize;
                    let below_bedrock = wy - BOTTOM;
                    let b = if below_bedrock == 0
                        || (below_bedrock < 5 && hash_f(wx, wy, wz, self.seed) < (5 - below_bedrock) as f32 / 5.0)
                    {
                        Block::BEDROCK
                    } else if wy < col.height - depth {
                        if wy < 0 || (wy < 8 && hash_f(wx, wy, wz, self.seed ^ 0xDEE) < (8 - wy) as f32 / 8.0) {
                            Block::DEEPSLATE
                        } else {
                            Block::STONE
                        }
                    } else if wy < col.height {
                        self.filler_block(col, wy)
                    } else if wy == col.height {
                        surface.unwrap_or(Block::GRASS)
                    } else if wy == SEA_LEVEL - 1 && col.frozen && self.ice_sheet(wx, wz, col.biome) {
                        Block::ICE
                    } else {
                        Block::WATER
                    };
                    blocks[index(x, y, z)] = b;
                }
            }
        }
    }

    /// Frozen oceans have holes in their ice; other frozen water freezes over.
    fn ice_sheet(&self, x: i32, z: i32, biome: Biome) -> bool {
        !biome.is_ocean() || self.feature.noise2(x as f32 / 23.0, z as f32 / 23.0) > -0.3
    }

    /// Noise caves, ravines and aquifers.
    fn carve(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        let field = self.caves.field(base);
        let ravines = self.caves.ravines(base);
        let mut aquifer = Aquifer::new(&self.caves, |x, z| {
            let (lx, lz) = (x - base.x, z - base.z);
            if (-1..=CHUNK_SIZE_I).contains(&lx) && (-1..=CHUNK_SIZE_I).contains(&lz) {
                columns.cols[(lz + 1) as usize][(lx + 1) as usize].height
            } else {
                self.column(x, z).height
            }
        });
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = columns.at(x, z);
                let limit = (col.height - base.y).min(CHUNK_SIZE_I - 1);
                if limit < 0 {
                    continue;
                }
                let profile = Caves::profile(&field, x, z);
                for y in 0..=limit as usize {
                    let wy = base.y + y as i32;
                    let i = index(x, y, z);
                    let b = blocks[i];
                    if b == Block::BEDROCK || b == Block::WATER || b == Block::ICE {
                        continue;
                    }
                    let ravine = ravines.as_ref().is_some_and(|m| m[i]) && wy > super::caves::CARVE_FLOOR;
                    if !ravine && !Caves::carved(&profile, y, wy, col.height - wy) {
                        continue;
                    }
                    // Keep beds under rivers and seas from draining into caves
                    // through a one-block floor.
                    if col.height < SEA_LEVEL && wy >= col.height - 1 {
                        continue;
                    }
                    if let Some(fill) = aquifer.fill(IVec3::new(base.x + x as i32, wy, base.z + z as i32)) {
                        blocks[i] = fill;
                    }
                }
            }
        }
    }

    /// Lush and dripstone cave floors, ceilings and pools.
    fn decorate_caves(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = columns.at(x, z);
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                for y in 1..CHUNK_SIZE - 1 {
                    let wy = base.y + y as i32;
                    // Keep flammable moss and plants clear of the lava sea, or
                    // its flames would burn through the cave as it loads.
                    if wy <= LAVA_LEVEL + 4 {
                        continue;
                    }
                    let Some(biome) = super::biome::cave(&col.climate, col.height - wy) else { continue };
                    let here = blocks[index(x, y, z)];
                    let below = blocks[index(x, y - 1, z)];
                    let above = blocks[index(x, y + 1, z)];
                    let roll = hash_f(wx, wy, wz, self.seed ^ 0xCA7E);
                    match (biome, here) {
                        (Biome::LushCaves, Block::AIR) => {
                            if is_rock(below) && roll < 0.85 {
                                blocks[index(x, y - 1, z)] = ob::MOSS_BLOCK;
                                let plant = hash_f(wx, wy, wz, self.seed ^ 0xF100);
                                blocks[index(x, y, z)] = if plant < 0.22 {
                                    ob::MOSS_CARPET
                                } else if plant < 0.32 {
                                    Block::TALL_GRASS
                                } else if plant < 0.35 {
                                    ob::AZALEA
                                } else if plant < 0.36 {
                                    ob::FLOWERING_AZALEA
                                } else if plant < 0.38 {
                                    ob::SMALL_DRIPLEAF
                                } else {
                                    Block::AIR
                                };
                            }
                            if is_rock(above) && roll > 0.4 {
                                blocks[index(x, y + 1, z)] = ob::MOSS_BLOCK;
                                if roll > 0.9 {
                                    self.hang_cave_vines(blocks, x, y, z, wx, wy, wz);
                                } else if roll > 0.885 {
                                    blocks[index(x, y, z)] = ob::SPORE_BLOSSOM;
                                }
                            }
                        }
                        (Biome::LushCaves, Block::WATER) if is_rock(below) => {
                            blocks[index(x, y - 1, z)] = Block::CLAY;
                            if above == Block::AIR && roll < 0.08 && y + 2 < CHUNK_SIZE {
                                blocks[index(x, y, z)] = ob::BIG_DRIPLEAF_STEM;
                                blocks[index(x, y + 1, z)] = ob::BIG_DRIPLEAF;
                            }
                        }
                        (Biome::DripstoneCaves, Block::AIR) => {
                            if is_rock(below) {
                                if roll < 0.25 {
                                    blocks[index(x, y - 1, z)] = ob::DRIPSTONE_BLOCK;
                                }
                                if roll < 0.08 {
                                    let len = 1 + (hash3(wx, wy, wz, self.seed ^ 0xD71) % 4) as usize;
                                    Self::dripstone(blocks, x, y, z, len, false);
                                }
                            }
                            if is_rock(above) {
                                if roll > 0.7 {
                                    blocks[index(x, y + 1, z)] = ob::DRIPSTONE_BLOCK;
                                }
                                if roll > 0.9 {
                                    let len = 1 + (hash3(wx, wy, wz, self.seed ^ 0xD72) % 5) as usize;
                                    Self::dripstone(blocks, x, y, z, len, true);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Cave vines down from the ceiling over `(x, y, z)`, within the chunk.
    #[allow(clippy::too_many_arguments)]
    fn hang_cave_vines(
        &self,
        blocks: &mut [Block; CHUNK_VOLUME],
        x: usize,
        y: usize,
        z: usize,
        wx: i32,
        wy: i32,
        wz: i32,
    ) {
        let len = 1 + (hash3(wx, wy, wz, self.seed ^ 0xC1E) % 6) as usize;
        let mut placed = 0;
        for d in 0..len {
            if y < d || blocks[index(x, y - d, z)] != Block::AIR {
                break;
            }
            placed = d + 1;
        }
        for d in 0..placed {
            let lit = hash_f(wx, wy - d as i32, wz, self.seed ^ 0xBE4) < 0.11;
            let head = d + 1 == placed;
            blocks[index(x, y - d, z)] = match (head, lit) {
                (true, false) => ob::CAVE_VINES,
                (true, true) => ob::CAVE_VINES_LIT,
                (false, false) => ob::CAVE_VINES_PLANT,
                (false, true) => ob::CAVE_VINES_PLANT_LIT,
            };
        }
    }

    /// A stalagmite (up from the floor under `y`) or stalactite (down from
    /// the ceiling over `y`) of up to `len` pointed dripstone.
    fn dripstone(blocks: &mut [Block; CHUNK_VOLUME], x: usize, y: usize, z: usize, len: usize, down: bool) {
        let mut cells = Vec::with_capacity(len);
        for d in 0..len {
            let ly = if down { y.checked_sub(d) } else { Some(y + d).filter(|&v| v < CHUNK_SIZE) };
            match ly {
                Some(ly) if blocks[index(x, ly, z)] == Block::AIR => cells.push(ly),
                _ => break,
            }
        }
        let n = cells.len();
        for (i, ly) in cells.into_iter().enumerate() {
            // i = 0 sits against the rock (the base); the last is the tip.
            let from_tip = n - 1 - i;
            let t = match from_tip {
                0 => Thickness::Tip,
                1 => Thickness::Frustum,
                _ if i == 0 => Thickness::Base,
                _ => Thickness::Middle,
            };
            blocks[index(x, ly, z)] = ob::pointed_dripstone(down, t);
        }
    }

    /// Features decided column by column, so they agree across chunk
    /// seams: ocean vegetation and coral, icebergs, ice spikes, lily pads.
    fn column_features(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        use Biome::*;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = *columns.at(x, z);
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let mut put = |wy: i32, b: Block, only: fn(Block) -> bool| {
                    let ly = wy - base.y;
                    if (0..CHUNK_SIZE_I).contains(&ly) {
                        let i = index(x, ly as usize, z);
                        if only(blocks[i]) {
                            blocks[i] = b;
                        }
                    }
                };
                let roll = hash_f(wx, 3, wz, self.seed ^ 0x5EA);
                let depth = SEA_LEVEL - 1 - col.height;
                let is_water = |b: Block| b == Block::WATER;
                if depth >= 1 && !col.frozen && (col.biome.is_ocean() || col.biome.is_river() || col.biome.is_swamp()) {
                    let floor = col.height;
                    let warm = matches!(col.biome, WarmOcean);
                    let reef = warm && self.feature.fbm2(wx as f32 / 14.0, wz as f32 / 14.0, 2) > 0.05;
                    if reef && depth >= 3 {
                        let colour = (hash3(wx >> 3, 1, wz >> 3, self.seed ^ 0xC0) % 5) as u16;
                        let tall = 1 + (hash3(wx, 2, wz, self.seed ^ 0xC1) % 3) as i32;
                        for dy in 1..=tall.min(depth - 1) {
                            put(floor + dy, ob::coral(0, colour, false), is_water);
                        }
                        let crown = hash_f(wx, 4, wz, self.seed ^ 0xC2);
                        let plant = if crown < 0.35 {
                            ob::coral(1, (colour + (crown * 10.0) as u16) % 5, false)
                        } else if crown < 0.6 {
                            ob::coral(2, (colour + 2) % 5, false)
                        } else if crown < 0.65 {
                            ob::sea_pickles(1 + (crown * 100.0) as u8 % 4)
                        } else {
                            Block::WATER
                        };
                        put(floor + tall.min(depth - 1) + 1, plant, is_water);
                        continue;
                    }
                    let kelpy = !matches!(col.biome, WarmOcean | FrozenOcean | DeepFrozenOcean)
                        && col.biome.is_ocean()
                        && self.feature.fbm2(wx as f32 / 40.0 + 9.0, wz as f32 / 40.0, 2) > 0.1;
                    if kelpy && roll < 0.22 && depth >= 4 {
                        let height = (2 + (hash3(wx, 5, wz, self.seed ^ 0x6E1) % 20) as i32).min(depth - 1);
                        for dy in 1..=height {
                            put(floor + dy, if dy == height { ob::KELP } else { ob::KELP_PLANT }, is_water);
                        }
                    } else if roll < 0.45 && col.biome != MangroveSwamp || roll < 0.15 {
                        if roll < 0.12 && depth >= 2 {
                            put(floor + 1, ob::TALL_SEAGRASS, is_water);
                            put(floor + 2, ob::TALL_SEAGRASS_TOP, is_water);
                        } else {
                            put(floor + 1, ob::SEAGRASS, is_water);
                        }
                    } else if matches!(col.biome, LukewarmOcean | DeepLukewarmOcean) && roll > 0.985 {
                        put(floor + 1, ob::sea_pickles(1 + (roll * 1000.0) as u8 % 4), is_water);
                    }
                    if col.biome == Swamp && depth <= 2 && roll > 0.95 {
                        put(SEA_LEVEL, ob::LILY_PAD, |b| b == Block::AIR);
                    }
                }
                // Icebergs: packed ice mounds rising from frozen seas.
                if matches!(col.biome, FrozenOcean | DeepFrozenOcean) {
                    let n = self.feature.fbm2(wx as f32 / 30.0 - 50.0, wz as f32 / 30.0, 3);
                    if n > 0.38 {
                        let up = ((n - 0.38) * 70.0) as i32;
                        let down = up * 2 + 2;
                        let core = n > 0.5 && hash_f(wx, 6, wz, self.seed) < 0.3;
                        for wy in (SEA_LEVEL - down).max(col.height + 1)..=SEA_LEVEL - 1 + up {
                            let b = if core { ob::BLUE_ICE } else { super::gadgets::PACKED_ICE };
                            put(wy, b, |b| b == Block::WATER || b == Block::AIR || b == Block::ICE);
                        }
                        put(SEA_LEVEL + up, Block::SNOW, |b| b == Block::AIR);
                    }
                }
                // Ice spikes: tall needles of packed ice.
                if col.biome == IceSpikes {
                    let spike = self.spike_height(wx, wz);
                    for dy in 1..=spike {
                        put(col.height + dy, super::gadgets::PACKED_ICE, |b| b == Block::AIR);
                    }
                }
            }
        }
    }

    /// Height of the ice spike over a column (0 for none): spikes stand on
    /// a 7-block grid of jittered centres and narrow toward their tips.
    fn spike_height(&self, x: i32, z: i32) -> i32 {
        let (cx, cz) = (x.div_euclid(7), z.div_euclid(7));
        let mut best = 0;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let h = hash3(cx + dx, 9, cz + dz, self.seed ^ 0x1CE);
                if !h.is_multiple_of(3) {
                    continue;
                }
                let centre = IVec2::new((cx + dx) * 7 + (h >> 8) as i32 % 7, (cz + dz) * 7 + (h >> 16) as i32 % 7);
                let tall = 6 + (h >> 24) as i32 % if (h >> 30).is_multiple_of(8) { 30 } else { 10 };
                let radius = 1.0 + tall as f32 / 12.0;
                let d = ((x - centre.x).pow(2) + (z - centre.y).pow(2)) as f32;
                if d <= radius * radius {
                    let here = (tall as f32 * (1.0 - d.sqrt() / (radius + 0.5))) as i32;
                    best = best.max(here);
                }
            }
        }
        best
    }

    /// Snow on cold surfaces and ice on cold still water, like Java's
    /// `freeze_top_layer`. Only columns whose top lies inside this chunk.
    fn freeze(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = columns.at(x, z);
                if col.biome.is_dry() {
                    continue;
                }
                let Some(y) = (0..CHUNK_SIZE - 1).rev().find(|&y| blocks[index(x, y, z)] != Block::AIR) else {
                    continue;
                };
                if blocks[index(x, y + 1, z)] != Block::AIR {
                    continue;
                }
                let wy = base.y + y as i32;
                if wy < col.height - 1 || col.biome.temperature_at(wy + 1) >= 0.15 {
                    continue;
                }
                let b = blocks[index(x, y, z)];
                if b == Block::WATER {
                    blocks[index(x, y, z)] = Block::ICE;
                } else if b.is_opaque() || b.is_leaves() {
                    blocks[index(x, y + 1, z)] = ob::SNOW_LAYER;
                }
            }
        }
    }

    /// Unmodified terrain at one point, used to validate a monster room
    /// without depending on which adjacent chunks were generated first.
    /// The caches make the repeated cave and height samples cheap.
    pub(super) fn natural_block(
        &self,
        p: IVec3,
        columns: &mut FxHashMap<IVec2, Column>,
        nodes: &mut FxHashMap<IVec3, [f32; 7]>,
    ) -> Block {
        if !self.dimension.contains_y(p.y) {
            return Block::AIR;
        }
        let col = *columns.entry(IVec2::new(p.x, p.z)).or_insert_with(|| self.column(p.x, p.z));
        if p.y > col.height {
            return if p.y < SEA_LEVEL { Block::WATER } else { Block::AIR };
        }
        if p.y <= BOTTOM + 4 {
            return Block::BEDROCK;
        }
        if col.height < SEA_LEVEL && p.y >= col.height - 1 {
            return Block::STONE;
        }
        if self.caves.carved_at(p, col.height - p.y, nodes) {
            if p.y < LAVA_LEVEL { Block::LAVA } else { Block::AIR }
        } else {
            Block::STONE
        }
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
                let roll = ((h >> 16) & 0xFFFF) as f32 / 65536.0;
                // Cheap rejection before the full column: most biomes are sparse.
                let biome = self.climate_biome(tx, tz);
                if roll >= tree_density(biome) {
                    continue;
                }
                let col = self.column(tx, tz);
                // Nothing this chunk could hold: skip the rest of the work.
                if col.height + TREE_TOP < base.y || col.height > base.y + CHUNK_SIZE_I {
                    continue;
                }
                let mangrove = col.biome == Biome::MangroveSwamp;
                if col.height < SEA_LEVEL - if mangrove { 3 } else { 0 } || (col.height == SEA_LEVEL - 1 && !mangrove) {
                    continue;
                }
                let variant = (h >> 32) as u32;
                let pick = (h >> 24) % 100;
                let ground = IVec3::new(tx, col.height, tz);
                // The same steep rule as `fill`, so trees skip bare cliffs.
                let h = |dx: i32, dz: i32| self.column(tx + dx, tz + dz).height;
                let steep = (h(1, 0) - h(-1, 0)).abs().max((h(0, 1) - h(0, -1)).abs());
                let surface = self.surface_block(col, tx, tz, steep);
                let mut tree_blocks: Vec<(IVec3, Block)> = Vec::new();
                let collect = &mut |p, b| tree_blocks.push((p, b));
                if !grow_tree_for(col.biome, surface, ground, variant, pick, collect) {
                    continue;
                }
                let put = &mut |p, b| Self::put(blocks, base, p, b);
                for &(p, b) in &tree_blocks {
                    put(p, b);
                }
                match col.biome {
                    Biome::Swamp | Biome::MangroveSwamp => hang_vines(&tree_blocks, variant, put),
                    b if b.is_jungle() && pick < 60 => {
                        hang_vines(&tree_blocks, variant, put);
                        if tree_blocks.iter().any(|&(_, b)| b == Block::JUNGLE_LOG) {
                            let trunk = tree_blocks
                                .iter()
                                .filter(|&&(p, b)| b == Block::JUNGLE_LOG && p.x == tx && p.z == tz)
                                .count();
                            cocoa_pods(ground, trunk as i32, variant, put);
                        }
                    }
                    _ => {}
                }
                // Old growth spruces turn the ground around them to podzol.
                if matches!(col.biome, Biome::OldGrowthSpruceTaiga | Biome::OldGrowthPineTaiga) && pick < 40 {
                    for dz in -2..=3 {
                        for dx in -2..=3 {
                            let g = ground + IVec3::new(dx, 0, dz);
                            let l = g - base;
                            if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                                let i = index(l.x as usize, l.y as usize, l.z as usize);
                                if blocks[i] == Block::GRASS {
                                    blocks[i] = ob::PODZOL;
                                }
                            }
                        }
                    }
                }
                // Azalea trees mark lush caves: their roots reach down.
                if self.lush_below(&col) && pick < 50 {
                    for dy in 0..12 {
                        let g = ground - IVec3::Y * dy;
                        let l = g - base;
                        if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                            let i = index(l.x as usize, l.y as usize, l.z as usize);
                            if is_rock(blocks[i]) || blocks[i] == Block::GRASS {
                                blocks[i] = ob::ROOTED_DIRT;
                            } else if blocks[i] == Block::AIR {
                                blocks[i] = ob::HANGING_ROOTS;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fn lush_below(&self, col: &Column) -> bool {
        col.climate.humidity >= 0.7
    }

    /// Scatters grass, flowers, ferns, bushes, cane, pumpkins, bamboo and
    /// more on the untouched surface (after trees, so trunks keep their
    /// spot). Only columns whose ground and the cell above both lie in this
    /// chunk get plants.
    fn place_plants(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        use Biome::*;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = *columns.at(x, z);
                let y = col.height - base.y;
                if !(0..CHUNK_SIZE_I - 2).contains(&y) || col.height < SEA_LEVEL {
                    continue;
                }
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let (ground, above) = (index(x, y as usize, z), index(x, y as usize + 1, z));
                let soil = blocks[ground];
                if blocks[above] != Block::AIR || soil != self.surface_block(col, wx, wz, columns.steepness(x, z)) {
                    continue; // carved by a cave, or covered by a tree
                }
                let roll = hash_f(wx, 0, wz, self.seed ^ 0x9A5);
                let grassy = matches!(soil, Block::GRASS | Block::SNOWY_GRASS) || soil == ob::PODZOL;

                // Sugar cane on the banks of rivers, lakes and the sea.
                if matches!(soil, Block::GRASS | Block::SAND | Block::RED_SAND | Block::DIRT)
                    && hash_f(wx, 1, wz, self.seed ^ 0xCA5) < 0.12
                    && Self::water_beside(blocks, x, y as usize, z)
                {
                    let tall = 1 + (hash3(wx, 2, wz, self.seed ^ 0xCA5) % 3) as usize;
                    for dy in 1..=tall.min(CHUNK_SIZE - 1 - y as usize) {
                        blocks[index(x, y as usize + dy, z)] = Block::SUGAR_CANE;
                    }
                    continue;
                }
                // Bamboo grows in clumps in bamboo jungles, rarely in jungles.
                let clump = self.feature.fbm2(wx as f32 / 9.0, wz as f32 / 9.0, 2);
                if grassy
                    && ((col.biome == BambooJungle && clump > -0.05 && roll < 0.55)
                        || (col.biome.is_jungle() && clump > 0.45 && roll < 0.3))
                {
                    let tall = 4 + (hash3(wx, 3, wz, self.seed ^ 0xBA3) % 12) as usize;
                    let tall = tall.min(CHUNK_SIZE - 2 - y as usize);
                    for dy in 1..=tall {
                        let b = if dy == tall {
                            ob::BAMBOO_LARGE_LEAVES
                        } else if dy + 1 == tall {
                            ob::BAMBOO_SMALL_LEAVES
                        } else {
                            ob::BAMBOO
                        };
                        blocks[index(x, y as usize + dy, z)] = b;
                    }
                    continue;
                }

                // Flowers grow in patches: a coarse cell decides the kind.
                let patch = hash3(wx >> 3, 0, wz >> 3, self.seed ^ 0xF10);
                let pumpkins = hash3(wx >> 4, 1, wz >> 4, self.seed ^ 0x9C1).is_multiple_of(24);
                let flower = self.flower(col.biome, patch, wx, wz);
                let flowery = patch % 5 < 2;
                let tall_grass = |b: &[Block; CHUNK_VOLUME]| {
                    y as usize + 2 < CHUNK_SIZE && b[index(x, y as usize + 2, z)] == Block::AIR
                };
                let plant: Option<(Block, Option<Block>)> = match (col.biome, grassy) {
                    (_, true) if pumpkins && roll > 0.97 && !col.biome.is_cold() => Some((Block::PUMPKIN, None)),
                    (Plains | SunflowerPlains | Meadow, true) if flowery && roll < 0.07 => Some((flower, None)),
                    (SunflowerPlains, true) if roll < 0.12 && tall_grass(blocks) => {
                        Some((ob::SUNFLOWER, Some(Block(ob::SUNFLOWER.0 + 1))))
                    }
                    (Meadow, true) if roll < 0.12 => Some((flower, None)),
                    (Plains | SunflowerPlains | Meadow, true) if roll < 0.04 && tall_grass(blocks) => {
                        Some((ob::DOUBLE_TALL_GRASS, Some(Block(ob::DOUBLE_TALL_GRASS.0 + 1))))
                    }
                    (Plains | SunflowerPlains | Meadow, true) if roll < 0.35 => Some((Block::TALL_GRASS, None)),
                    (FlowerForest, true) if roll < 0.3 => Some((flower, None)),
                    (Forest | FlowerForest | DarkForest, true) if roll < 0.012 && tall_grass(blocks) => {
                        let lower = [ob::LILAC, ob::ROSE_BUSH, ob::PEONY][(patch % 3) as usize];
                        Some((lower, Some(Block(lower.0 + 1))))
                    }
                    (Forest | BirchForest | OldGrowthBirchForest | DarkForest, true) if flowery && roll < 0.03 => {
                        Some((flower, None))
                    }
                    (DarkForest, true) if roll < 0.04 => {
                        Some((if patch.is_multiple_of(2) { Block::RED_MUSHROOM } else { Block::BROWN_MUSHROOM }, None))
                    }
                    (Forest | FlowerForest | BirchForest | OldGrowthBirchForest | DarkForest, true) if roll < 0.15 => {
                        Some((Block::TALL_GRASS, None))
                    }
                    (PaleGarden, true) if roll < 0.3 => Some((ob::PALE_MOSS_CARPET, None)),
                    (CherryGrove, true) if roll < 0.35 && !patch.is_multiple_of(3) => Some((ob::PINK_PETALS, None)),
                    (CherryGrove, true) if roll < 0.5 => Some((Block::TALL_GRASS, None)),
                    (b, true) if b.is_savanna() && roll < 0.05 && tall_grass(blocks) => {
                        Some((ob::DOUBLE_TALL_GRASS, Some(Block(ob::DOUBLE_TALL_GRASS.0 + 1))))
                    }
                    (b, true) if b.is_savanna() && roll < 0.45 => Some((Block::TALL_GRASS, None)),
                    (b, true) if b.is_jungle() && roll < 0.006 => Some((Block::MELON, None)),
                    (b, true) if b.is_jungle() && roll < 0.05 && tall_grass(blocks) => {
                        Some((ob::LARGE_FERN, Some(Block(ob::LARGE_FERN.0 + 1))))
                    }
                    (b, true) if b.is_jungle() && roll < 0.2 => Some((Block::FERN, None)),
                    (b, true) if b.is_jungle() && roll < 0.5 => Some((Block::TALL_GRASS, None)),
                    (Swamp, true) if roll < 0.03 => Some((Block::BLUE_ORCHID, None)),
                    (Swamp, true) if roll < 0.2 => Some((Block::TALL_GRASS, None)),
                    (b, true) if b.is_taiga() && roll < 0.01 => Some((ob::berry_bush(3), None)),
                    (OldGrowthPineTaiga | OldGrowthSpruceTaiga, true) if roll < 0.04 => {
                        Some((if patch.is_multiple_of(2) { Block::BROWN_MUSHROOM } else { Block::RED_MUSHROOM }, None))
                    }
                    (b, true) if b.is_taiga() && roll < 0.06 && tall_grass(blocks) => {
                        Some((ob::LARGE_FERN, Some(Block(ob::LARGE_FERN.0 + 1))))
                    }
                    (b, true) if b.is_taiga() && roll < 0.18 => Some((Block::FERN, None)),
                    (b, true) if b.is_taiga() && roll < 0.25 => Some((Block::TALL_GRASS, None)),
                    (WindsweptHills | WindsweptForest | Grove, true) if roll < 0.08 => Some((Block::TALL_GRASS, None)),
                    (Desert, false) if soil == Block::SAND && roll < 0.008 => Some((Block::DEAD_BUSH, None)),
                    (b, false) if b.is_badlands() && soil == Block::RED_SAND && roll < 0.015 => {
                        Some((Block::DEAD_BUSH, None))
                    }
                    (MushroomFields, false) if soil == ob::MYCELIUM && roll < 0.01 => {
                        Some((if patch.is_multiple_of(2) { Block::RED_MUSHROOM } else { Block::BROWN_MUSHROOM }, None))
                    }
                    (MangroveSwamp, false) if soil == ob::MUD && roll < 0.03 => Some((Block::TALL_GRASS, None)),
                    _ => None,
                };
                if let Some((lower, upper)) = plant
                    && lower.can_stay_on(soil)
                {
                    blocks[above] = lower;
                    if let Some(upper) = upper {
                        blocks[index(x, y as usize + 2, z)] = upper;
                    }
                }
            }
        }
    }

    /// Which flower a patch grows in a biome (Java's per-biome flower lists).
    fn flower(&self, biome: Biome, patch: u64, x: i32, z: i32) -> Block {
        use Biome::*;
        let pick = (patch >> 8) as usize;
        match biome {
            FlowerForest => {
                const ALL: [Block; 12] = [
                    Block::DANDELION,
                    Block::POPPY,
                    ob::ALLIUM,
                    ob::AZURE_BLUET,
                    ob::RED_TULIP,
                    ob::ORANGE_TULIP,
                    ob::WHITE_TULIP,
                    ob::PINK_TULIP,
                    ob::OXEYE_DAISY,
                    ob::CORNFLOWER,
                    ob::LILY_OF_THE_VALLEY,
                    ob::ALLIUM,
                ];
                // Flower forests sort their flowers in bands by noise.
                let n = self.patch.noise2(x as f32 / 48.0, z as f32 / 48.0);
                ALL[(((n + 1.0) * 6.0) as usize).min(11)]
            }
            Plains | SunflowerPlains => {
                const P: [Block; 9] = [
                    Block::DANDELION,
                    Block::POPPY,
                    ob::AZURE_BLUET,
                    ob::OXEYE_DAISY,
                    ob::CORNFLOWER,
                    ob::RED_TULIP,
                    ob::ORANGE_TULIP,
                    ob::WHITE_TULIP,
                    ob::PINK_TULIP,
                ];
                P[pick % P.len()]
            }
            Meadow => {
                const P: [Block; 6] =
                    [Block::DANDELION, Block::POPPY, ob::AZURE_BLUET, ob::OXEYE_DAISY, ob::CORNFLOWER, ob::ALLIUM];
                P[pick % P.len()]
            }
            Forest | DarkForest | BirchForest | OldGrowthBirchForest => {
                [Block::DANDELION, Block::POPPY, ob::LILY_OF_THE_VALLEY][pick % 3]
            }
            _ => [Block::DANDELION, Block::POPPY][pick % 2],
        }
    }

    /// Whether water touches the side of the block at local (x, y, z),
    /// looking only inside this chunk.
    fn water_beside(blocks: &[Block; CHUNK_VOLUME], x: usize, y: usize, z: usize) -> bool {
        let n = CHUNK_SIZE - 1;
        let at = |x: usize, z: usize| blocks[index(x, y, z)] == Block::WATER;
        (x > 0 && at(x - 1, z)) || (x < n && at(x + 1, z)) || (z > 0 && at(x, z - 1)) || (z < n && at(x, z + 1))
    }

    /// Writes a block if it falls inside this chunk. Trunks may replace
    /// leaves; leaves only fill air. Mangrove roots and logs may also grow
    /// into water and mud. This makes overlapping trees resolve the same
    /// way no matter which chunk (and order) places them.
    #[inline]
    fn put(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, p: IVec3, b: Block) {
        let l = p - base;
        if l.cmplt(IVec3::ZERO).any() || l.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
            return;
        }
        let slot = &mut blocks[index(l.x as usize, l.y as usize, l.z as usize)];
        let mangrove = b == ob::MANGROVE_ROOTS || b == Block::MANGROVE_LOG;
        if *slot == Block::AIR
            || (!b.is_leaves() && slot.is_leaves())
            || (mangrove && (*slot == Block::WATER || *slot == ob::MUD || slot.is_replaceable()))
            || (b.is_log() && slot.kind() == super::block::RenderKind::Cross)
        {
            *slot = b;
        }
    }

    /// Finds a dry-land spawn point near the origin.
    pub fn find_spawn(&self) -> IVec3 {
        if self.end.is_some() {
            return super::end::SPAWN;
        }
        for r in 0..96 {
            for i in 0..(r * 8).max(1) {
                let a = i as f32 / (r * 8).max(1) as f32 * std::f32::consts::TAU;
                let x = (a.cos() * r as f32 * 16.0) as i32;
                let z = (a.sin() * r as f32 * 16.0) as i32;
                let c = self.column(x, z);
                if c.height > SEA_LEVEL && c.height < 120 && !c.biome.is_watery() && !c.biome.is_peak() {
                    return IVec3::new(x, c.height + 1, z);
                }
            }
        }
        IVec3::new(0, self.column(0, 0).height.max(SEA_LEVEL) + 1, 0)
    }
}

/// Share of tree cells that grow a tree, per biome.
fn tree_density(biome: Biome) -> f32 {
    use Biome::*;
    match biome {
        Jungle | DarkForest => 0.95,
        PaleGarden => 0.85,
        BambooJungle => 0.45,
        Forest | FlowerForest | BirchForest | OldGrowthBirchForest => 0.75,
        OldGrowthPineTaiga | OldGrowthSpruceTaiga => 0.8,
        Taiga | SnowyTaiga => 0.6,
        MangroveSwamp => 0.7,
        WindsweptForest => 0.5,
        Grove => 0.45,
        Swamp => 0.3,
        CherryGrove => 0.3,
        SparseJungle => 0.25,
        WoodedBadlands => 0.25,
        Savanna | SavannaPlateau => 0.14,
        WindsweptSavanna => 0.08,
        Desert => 0.1,
        Plains | SunflowerPlains => 0.05,
        Meadow => 0.03,
        WindsweptHills => 0.08,
        SnowyPlains => 0.03,
        MushroomFields => 0.06,
        Badlands | ErodedBadlands => 0.05,
        WindsweptGravellyHills => 0.03,
        _ => 0.0,
    }
}

/// Grows a biome's tree on `ground` (whose block is `soil`). Returns false
/// when nothing grows there.
fn grow_tree_for(biome: Biome, soil: Block, ground: IVec3, v: u32, pick: u64, put: Put) -> bool {
    use Biome::*;
    let grass = matches!(soil, Block::GRASS | Block::SNOWY_GRASS) || soil == ob::PODZOL || soil == ob::COARSE_DIRT;
    match biome {
        Desert if soil == Block::SAND => cactus(ground, v, put),
        Badlands | ErodedBadlands if soil == Block::RED_SAND => cactus(ground, v, put),
        WoodedBadlands if grass => oak(ground, v % 2, put),
        MushroomFields if soil == ob::MYCELIUM => huge_mushroom(ground, v, pick < 50, put),
        MangroveSwamp if soil == ob::MUD || soil == Block::WATER || grass => mangrove(ground, v, put),
        _ if !grass => return false,
        Taiga | SnowyTaiga | Grove | SnowyPlains if pick < 33 => pine(ground, v, put),
        Taiga | SnowyTaiga | Grove | SnowyPlains => spruce(ground, v, put),
        OldGrowthSpruceTaiga if pick < 40 => mega_spruce(ground, v, false, put),
        OldGrowthPineTaiga if pick < 40 => mega_spruce(ground, v, true, put),
        OldGrowthSpruceTaiga | OldGrowthPineTaiga if pick < 70 => spruce(ground, v, put),
        OldGrowthSpruceTaiga | OldGrowthPineTaiga => pine(ground, v, put),
        WindsweptForest | WindsweptHills | WindsweptGravellyHills if pick < 60 => spruce(ground, v, put),
        WindsweptForest | WindsweptHills | WindsweptGravellyHills => oak(ground, v, put),
        BirchForest => birch(ground, v, put),
        OldGrowthBirchForest => tall_birch(ground, v, put),
        Forest | FlowerForest if pick < 20 => birch(ground, v, put),
        Forest | FlowerForest if pick < 28 => fancy_oak(ground, v, put),
        DarkForest if pick < 5 => huge_mushroom(ground, v, pick < 3, put),
        DarkForest if pick < 72 => dark_oak(ground, v, put),
        DarkForest if pick < 85 => birch(ground, v, put),
        PaleGarden if pick < 90 => pale_oak(ground, v, put),
        CherryGrove => cherry(ground, v, put),
        Jungle | BambooJungle | SparseJungle if pick < 12 && biome != SparseJungle => mega_jungle(ground, v, put),
        Jungle | BambooJungle | SparseJungle if pick < 50 => jungle(ground, v, put),
        Jungle | BambooJungle | SparseJungle if pick < 90 => jungle_bush(ground, v, put),
        Savanna | SavannaPlateau | WindsweptSavanna if pick < 80 => acacia(ground, v, put),
        Swamp => swamp_oak(ground, v, put),
        Plains | SunflowerPlains | Meadow if pick < 15 => fancy_oak(ground, v, put),
        Meadow if pick < 50 => birch(ground, v, put),
        _ if biome.is_cold() => spruce(ground, v, put),
        _ => oak(ground, v, put),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden block IDs of the generator. The Overworld values were
    /// re-captured for the 1.18-style generator (v0.6); the Nether and End
    /// values are unchanged.
    #[test]
    fn generated_chunk_hashes_stay_identical() {
        for (dimension, seed, expected) in [
            (Dimension::Overworld, 12345, 0xd652_ef08_5c4e_7a1au64),
            (Dimension::Overworld, 99, 0x496d_1ab6_6454_aa98u64),
            (Dimension::Nether, 12345, 0xb0c9_36c6_81b7_bfd5u64),
            (Dimension::End, 12345, 0xc5aa_2549_4a63_5b2bu64),
        ] {
            let g = Generator::for_dimension(seed, dimension);
            let mut hash = 0xcbf2_9ce4_8422_2325u64;
            let (lo, hi) = (dimension.chunk_rows().start, dimension.chunk_rows().end);
            let mut positions: Vec<_> =
                (-3..=3).flat_map(|x| (-3..=3).flat_map(move |z| (lo..hi).map(move |y| IVec3::new(x, y, z)))).collect();
            positions.extend([IVec3::new(-31, 0, 17), IVec3::new(17, 1, -23), IVec3::new(4, 4, 39)]);
            if dimension != Dimension::Overworld {
                positions.retain(|p| (0..8).contains(&p.y));
            }
            for p in positions {
                g.generate(p).for_each_block(|block| {
                    for byte in block.0.to_le_bytes() {
                        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                    }
                });
            }
            assert_eq!(hash, expected, "{dimension:?} seed {seed}: {hash:#x}");
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let g = Generator::new(1234);
        for p in [IVec3::new(0, 1, 0), IVec3::new(-3, 2, 5), IVec3::new(7, -1, -2)] {
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
                let c = g.column(i * 96, j * 96);
                heights.push(c.height);
                biomes.insert(c.biome);
            }
        }
        let min = *heights.iter().min().unwrap();
        let max = *heights.iter().max().unwrap();
        assert!(min < 40 && max > 170, "height range {min}..{max}");
        assert!(biomes.len() >= 35, "{} biomes", biomes.len());
    }

    #[test]
    fn surface_grows_grass_and_flowers() {
        let g = Generator::new(99);
        let mut counts = std::collections::HashMap::new();
        for cx in -8..8 {
            for cz in -8..8 {
                for cy in 1..5 {
                    g.generate(IVec3::new(cx, cy, cz)).for_each_block(|b| {
                        if b.kind() == crate::world::block::RenderKind::Cross {
                            *counts.entry(b).or_insert(0) += 1;
                        }
                    });
                }
            }
        }
        assert!(counts.get(&Block::TALL_GRASS).copied().unwrap_or(0) > 100, "{counts:?}");
    }

    #[test]
    fn bedrock_floor_deepslate_and_deep_lava() {
        let g = Generator::new(99);
        let mut lava = 0;
        let mut deepslate = 0;
        for cx in -3..3 {
            for cz in -3..3 {
                let data = g.generate(IVec3::new(cx, -2, cz));
                for z in 0..CHUNK_SIZE {
                    for x in 0..CHUNK_SIZE {
                        assert_eq!(data.get(x, 0, z), Block::BEDROCK, "bedrock at y=-64");
                        for y in 0..CHUNK_SIZE {
                            match data.get(x, y, z) {
                                Block::LAVA => {
                                    lava += 1;
                                    assert!(y as i32 + BOTTOM < LAVA_LEVEL + 1 || y > 0);
                                }
                                Block::DEEPSLATE => deepslate += 1,
                                Block::STONE => panic!("stone below y=-32"),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        assert!(lava > 0 && deepslate > 100_000, "lava {lava}, deepslate {deepslate}");
    }

    #[test]
    fn nearest_biome_finds_a_matching_column() {
        let g = Generator::new(42);
        let origin = IVec2::new(0, 0);
        let here = g.column(origin.x, origin.y).biome;
        assert_eq!(g.nearest_biome(origin, here, 256).map(|p| IVec2::new(p.x, p.z)), Some(origin));
        assert!(g.nearest_biome(origin, Biome::Ocean, 12_800).is_some());
    }

    #[test]
    fn foliage_follows_the_biomes() {
        let g = Generator::new(99);
        let f = g.foliage(3, -7);
        assert_eq!(*g.foliage(3, -7), *f, "deterministic");
    }

    #[test]
    fn spawn_is_on_dry_land() {
        let g = Generator::new(7);
        let s = g.find_spawn();
        let c = g.column(s.x, s.z);
        assert!(c.height >= SEA_LEVEL && !c.biome.is_watery(), "{s} {:?}", c.biome);
    }
}
