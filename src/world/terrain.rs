//! Procedural terrain generation.
//!
//! Every chunk is generated independently from the seed (no cross-chunk
//! state), so generation parallelises trivially across worker threads.
//! Features that straddle chunk borders, such as trees, are placed
//! deterministically from hashed world coordinates and each chunk writes
//! only its own part of them.
//!
//! Each column gets a height from layered noise (continents, erosion,
//! mountain ridges, detail), reshaped by the climate: rivers carve valleys
//! down to sea level, swamps flatten into shallow pools and badlands rise
//! into terraced plateaus. Temperature and humidity then pick the biome.

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::{Block, Wood};
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, index};
use super::noise::{Perlin, hash_f, hash3};

pub const SEA_LEVEL: i32 = 62;
/// Caves carved at or below this height fill with lava.
pub const LAVA_LEVEL: i32 = 10;
const TREE_CELL: i32 = 5;
/// How far a tree's leaves can extend from its trunk (mega jungle trees).
const TREE_REACH: i32 = 5;
/// How far above the ground the tallest tree reaches.
const TREE_TOP: i32 = 32;
const CAVE_STEP: usize = 4;
const CAVE_GRID: usize = CHUNK_SIZE / CAVE_STEP + 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Ocean,
    Beach,
    River,
    Plains,
    Forest,
    BirchForest,
    Swamp,
    Desert,
    Badlands,
    Savanna,
    Jungle,
    Mountains,
    Snowy,
    Taiga,
}

impl Biome {
    pub const ALL: [Biome; 14] = [
        Biome::Ocean,
        Biome::Beach,
        Biome::River,
        Biome::Plains,
        Biome::Forest,
        Biome::BirchForest,
        Biome::Swamp,
        Biome::Desert,
        Biome::Badlands,
        Biome::Savanna,
        Biome::Jungle,
        Biome::Mountains,
        Biome::Snowy,
        Biome::Taiga,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Biome::Ocean => "ocean",
            Biome::Beach => "beach",
            Biome::River => "river",
            Biome::Plains => "plains",
            Biome::Forest => "forest",
            Biome::BirchForest => "birch_forest",
            Biome::Swamp => "swamp",
            Biome::Desert => "desert",
            Biome::Badlands => "badlands",
            Biome::Savanna => "savanna",
            Biome::Jungle => "jungle",
            Biome::Mountains => "mountains",
            Biome::Snowy => "snowy",
            Biome::Taiga => "taiga",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Self::ALL.into_iter().find(|biome| biome.name() == name)
    }

    /// Which colour grass and leaves take on here (see `block::tex::tinted`):
    /// 0 temperate green, 1 murky swamp, 2 dry and yellow, 3 lush jungle,
    /// 4 cold and blue.
    pub fn foliage(self) -> u8 {
        match self {
            Biome::Swamp => 1,
            Biome::Savanna | Biome::Desert | Biome::Badlands => 2,
            Biome::Jungle => 3,
            Biome::Taiga | Biome::Snowy => 4,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub height: i32,
    pub biome: Biome,
    /// Cold enough for the sea and rivers to freeze over.
    pub frozen: bool,
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
    continent: Perlin,
    erosion: Perlin,
    ridge: Perlin,
    detail: Perlin,
    temperature: Perlin,
    humidity: Perlin,
    cave_a: Perlin,
    cave_b: Perlin,
    cavern: Perlin,
    /// Picks variants within a climate: birch woods, badlands.
    weird: Perlin,
    river: Perlin,
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Badlands strata, bottom to top, as terracotta colours (see
/// [`Block::terracotta`]); the pattern repeats every `BANDS.len()` blocks.
const BANDS: [u8; 16] = [1, 1, 0, 2, 0, 0, 3, 1, 0, 4, 0, 5, 0, 1, 6, 0];

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
            continent: p(1),
            erosion: p(2),
            ridge: p(3),
            detail: p(4),
            temperature: p(5),
            humidity: p(6),
            cave_a: p(7),
            cave_b: p(8),
            cavern: p(9),
            weird: p(10),
            river: p(11),
        }
    }

    /// The End's layout (pillars, exit portal), in the End only.
    pub fn end(&self) -> Option<&super::end::EndGen> {
        self.end.as_ref()
    }

    /// Nearest Nether fortress, when this is a Nether generator.
    pub fn nearest_fortress(&self, p: glam::IVec2) -> Option<IVec3> {
        self.nether.as_ref()?.fortresses.nearest(p)
    }

    /// Nearest column of `target`, searching outward from `origin` in 32-block
    /// steps (Java `/locate biome` uses a similar spiral; capped for speed).
    pub fn nearest_biome(&self, origin: IVec2, target: Biome, max_blocks: i32) -> Option<IVec3> {
        if self.dimension != Dimension::Overworld {
            return None;
        }
        let here = |x, z| self.column(x, z).biome == target;
        if here(origin.x, origin.y) {
            return Some(IVec3::new(origin.x, self.column(origin.x, origin.y).height + 1, origin.y));
        }
        let steps = (max_blocks / 32).max(1);
        for ring in 1..=steps {
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let x = origin.x + dx * 32;
                    let z = origin.y + dz * 32;
                    if here(x, z) {
                        return Some(IVec3::new(x, self.column(x, z).height + 1, z));
                    }
                }
            }
        }
        None
    }

    /// Surface height and biome of a world column.
    pub fn column(&self, x: i32, z: i32) -> Column {
        if let Some(end) = &self.end {
            return Column { height: end.column(x, z).map_or(-1, |(top, _)| top), biome: Biome::Plains, frozen: false };
        }
        let (fx, fz) = (x as f32, z as f32);
        let cont = self.continent.fbm2(fx / 900.0, fz / 900.0, 5) * 1.8;
        let erosion = self.erosion.fbm2(fx / 500.0, fz / 500.0, 3) * 1.6;
        let ridge = 1.0 - self.ridge.fbm2(fx / 260.0, fz / 260.0, 5).abs() * 2.2;
        let ridge = ridge.max(0.0).powi(2);
        let detail = self.detail.fbm2(fx / 60.0, fz / 60.0, 4);
        let climate = self.temperature.fbm2(fx / 1100.0, fz / 1100.0, 2) * 2.0;
        let humid = self.humidity.fbm2(fx / 900.0, fz / 900.0, 3) * 1.8;
        let weird = self.weird.fbm2(fx / 700.0, fz / 700.0, 3) * 1.8;

        // Land rises gently out of the ocean; mountains only appear inland
        // where erosion is low.
        let base = SEA_LEVEL as f32 + 4.0 + cont * 34.0;
        let mountain = smoothstep(0.05, 0.45, cont) * smoothstep(0.1, -0.4, erosion);
        let hills = smoothstep(-0.2, 0.4, cont) * 10.0;
        let mut h = base + detail * (4.0 + hills) + ridge * mountain * 110.0;
        let inland = smoothstep(-0.08, 0.12, cont) * (1.0 - mountain);

        // Badlands: hot, dry and odd. The land lifts into plateaus whose
        // terraced cliffs expose the strata.
        let badland = smoothstep(0.2, 0.35, climate)
            * smoothstep(0.05, -0.1, humid)
            * smoothstep(0.1, 0.3, weird)
            * smoothstep(0.0, 0.2, cont)
            * (1.0 - mountain);
        if badland > 0.0 {
            let raised = h + badland * (14.0 + detail.max(-0.5) * 16.0 + smoothstep(0.35, 0.7, weird) * 14.0);
            let t = raised / 7.0;
            let terraced = (t.floor() + smoothstep(0.6, 0.9, t.fract())) * 7.0;
            h = lerp(raised, terraced, smoothstep(0.2, 0.6, badland));
        }

        // Swamps: temperate, soaking wet lowland pressed flat around sea
        // level, with shallow pools where the ground dips.
        let temperate = 1.0 - smoothstep(0.25, 0.4, climate.abs());
        let swampy = smoothstep(0.25, 0.45, humid)
            * temperate
            * inland
            * smoothstep(SEA_LEVEL as f32 + 16.0, SEA_LEVEL as f32 + 6.0, h);
        if swampy > 0.0 {
            let pools = self.weird.noise2(fx / 14.0, fz / 14.0) * 2.5 + detail * 2.0;
            h = lerp(h, SEA_LEVEL as f32 + 0.3 + pools, swampy);
        }

        // Rivers wind along the zero line of their own noise and cut a
        // valley down to just below sea level.
        let rv = self.river.fbm2(fx / 700.0, fz / 700.0, 3).abs();
        let valley =
            smoothstep(0.05, 0.011, rv) * smoothstep(-0.05, 0.1, cont) * (1.0 - smoothstep(0.3, 0.6, mountain));
        let bed = SEA_LEVEL as f32 - 1.0 - 3.0 * smoothstep(0.011, 0.0, rv);
        if h > bed {
            h = lerp(h, bed, valley);
        }

        let height = (h as i32).clamp(4, 240);
        let temp = climate - (height - SEA_LEVEL).max(0) as f32 / 160.0;
        let frozen = temp < -0.3;

        let biome = if valley > 0.75 && height < SEA_LEVEL {
            Biome::River
        } else if swampy > 0.5 && height >= SEA_LEVEL - 3 {
            Biome::Swamp
        } else if height < SEA_LEVEL - 1 {
            Biome::Ocean
        } else if height <= SEA_LEVEL + 1 && mountain < 0.2 && valley < 0.3 && badland < 0.3 {
            Biome::Beach
        } else if height > 125 || mountain > 0.55 {
            Biome::Mountains
        } else if badland > 0.3 {
            Biome::Badlands
        } else if temp < -0.3 {
            if humid > 0.0 { Biome::Taiga } else { Biome::Snowy }
        } else if temp > 0.22 {
            if humid < -0.05 {
                Biome::Desert
            } else if humid < 0.25 {
                Biome::Savanna
            } else {
                Biome::Jungle
            }
        } else if humid > 0.05 {
            if weird > 0.25 { Biome::BirchForest } else { Biome::Forest }
        } else {
            Biome::Plains
        };
        Column { height, biome, frozen }
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
            *f = self.column(x + dx, z + dz).biome.foliage();
        }
        out
    }

    /// Whether a column under water gets a patch of clay on its floor.
    fn clay_patch(&self, x: i32, z: i32) -> bool {
        hash3(x >> 2, 0, z >> 2, self.seed ^ 0xC1A).is_multiple_of(7) && hash_f(x, 1, z, self.seed ^ 0xC1B) < 0.8
    }

    /// The block at the very top of a column, before caves and decoration.
    fn surface_block(&self, col: Column, x: i32, z: i32) -> Block {
        let y = col.height;
        let wet = y < SEA_LEVEL;
        match col.biome {
            Biome::Ocean | Biome::River | Biome::Swamp if wet && y >= SEA_LEVEL - 6 && self.clay_patch(x, z) => {
                Block::CLAY
            }
            Biome::Ocean => {
                if y >= SEA_LEVEL - 4 {
                    Block::SAND
                } else {
                    Block::GRAVEL
                }
            }
            Biome::River => {
                if y < SEA_LEVEL - 3 {
                    Block::GRAVEL
                } else {
                    Block::SAND
                }
            }
            Biome::Swamp if wet => Block::DIRT,
            Biome::Beach | Biome::Desert => Block::SAND,
            Biome::Badlands => {
                // Red sand on the low ground, bare strata up high.
                if y < SEA_LEVEL + 14 || hash_f(x, 2, z, self.seed ^ 0xBAD) < 0.3 {
                    Block::RED_SAND
                } else {
                    self.stratum(y)
                }
            }
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
            _ => Block::GRASS,
        }
    }

    fn filler_block(&self, col: Column, y: i32) -> Block {
        match col.biome {
            Biome::Beach | Biome::Ocean | Biome::River => Block::SAND,
            Biome::Desert => {
                if y > col.height - 3 {
                    Block::SAND
                } else {
                    Block::SANDSTONE
                }
            }
            Biome::Badlands => {
                if y > col.height - 2 && col.height < SEA_LEVEL + 14 {
                    Block::RED_SAND
                } else {
                    self.stratum(y)
                }
            }
            Biome::Mountains if col.height > 135 => Block::STONE,
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
            Biome::Desert => 6,
            // Strata run down to below sea level so whole cliffs are banded.
            Biome::Badlands => (col.height - SEA_LEVEL + 6).max(4),
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

    pub fn generate(&self, cpos: IVec3) -> ChunkData {
        if let Some(end) = &self.end {
            return end.generate(cpos);
        }
        if let Some(nether) = &self.nether {
            return nether.generate(cpos);
        }
        let base = cpos * CHUNK_SIZE_I;
        let mut cols = [[Column { height: 0, biome: Biome::Plains, frozen: false }; CHUNK_SIZE]; CHUNK_SIZE];
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
        if base.y > (max_h + TREE_TOP).max(SEA_LEVEL) {
            return ChunkData::Uniform(Block::AIR);
        }

        let caves = if base.y < max_h { Some(self.cave_field(base)) } else { None };
        let mut blocks = ChunkData::new_dense(Block::AIR);

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = cols[z][x];
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let fill_top = col.height.max(SEA_LEVEL).min(top);
                let depth = Self::soil_depth(col);
                for wy in base.y..=fill_top {
                    let y = (wy - base.y) as usize;
                    // Solid bedrock at y=0, thinning out randomly up to y=3.
                    let mut b = if wy == 0 || (wy < 4 && hash_f(wx, wy, wz, self.seed) < (4 - wy) as f32 / 4.0) {
                        Block::BEDROCK
                    } else if wy < col.height - depth {
                        self.ore_or_stone(wx, wy, wz)
                    } else if wy < col.height {
                        self.filler_block(col, wy)
                    } else if wy == col.height {
                        self.surface_block(col, wx, wz)
                    } else if wy == SEA_LEVEL && col.frozen {
                        Block::ICE
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
                            // Deep caves flood with lava, like Minecraft's lava level.
                            b = if wy <= LAVA_LEVEL { Block::LAVA } else { Block::AIR };
                        }
                    }
                    blocks[index(x, y, z)] = b;
                }
            }
        }

        if base.y <= max_h + TREE_TOP && top >= min_h {
            self.place_trees(&mut blocks, base);
            self.place_plants(&mut blocks, base, &cols);
        }
        if base.y < SEA_LEVEL {
            self.strongholds.paint(&mut blocks, base);
        }
        self.dungeons.paint(self, &mut blocks, base);
        ChunkData::from_dense(blocks)
    }

    /// Unmodified terrain at one point, used to validate a monster room
    /// without depending on which adjacent chunks were generated first.
    /// The caches make the repeated cave and height samples cheap.
    pub(super) fn natural_block(
        &self,
        p: IVec3,
        columns: &mut FxHashMap<IVec2, Column>,
        nodes: &mut FxHashMap<IVec3, [f32; 3]>,
    ) -> Block {
        if p.y < 0 || p.y >= 256 {
            return Block::AIR;
        }
        let col = *columns.entry(IVec2::new(p.x, p.z)).or_insert_with(|| self.column(p.x, p.z));
        if p.y > col.height {
            return if p.y <= SEA_LEVEL {
                if p.y == SEA_LEVEL && col.frozen { Block::ICE } else { Block::WATER }
            } else {
                Block::AIR
            };
        }
        if p.y <= 4 {
            return Block::STONE;
        }
        let carve_limit = if col.height < SEA_LEVEL + 2 { col.height - 6 } else { col.height };
        if p.y > carve_limit {
            return Block::STONE;
        }
        let lo = IVec3::new(p.x.div_euclid(4) * 4, p.y.div_euclid(4) * 4, p.z.div_euclid(4) * 4);
        let t = (p - lo).as_vec3() / 4.0;
        let mut v = [0.0f32; 3];
        for dy in 0..=1 {
            for dz in 0..=1 {
                for dx in 0..=1 {
                    let q = lo + IVec3::new(dx * 4, dy * 4, dz * 4);
                    let at = *nodes.entry(q).or_insert_with(|| {
                        let (x, y, z) = (q.x as f32, q.y as f32, q.z as f32);
                        [
                            self.cave_a.noise3(x / 48.0, y / 32.0, z / 48.0),
                            self.cave_b.noise3(x / 48.0, y / 32.0, z / 48.0),
                            self.cavern.noise3(x / 90.0, y / 45.0, z / 90.0),
                        ]
                    });
                    let w = (if dx == 0 { 1.0 - t.x } else { t.x })
                        * (if dy == 0 { 1.0 - t.y } else { t.y })
                        * (if dz == 0 { 1.0 - t.z } else { t.z });
                    for i in 0..3 {
                        v[i] += at[i] * w;
                    }
                }
            }
        }
        let tunnel = v[0] * v[0] + v[1] * v[1] < 0.0045;
        let cavern = p.y < 48 && v[2] > 0.42 - (48 - p.y) as f32 * 0.002;
        if tunnel || cavern { if p.y <= LAVA_LEVEL { Block::LAVA } else { Block::AIR } } else { Block::STONE }
    }

    fn ore_or_stone(&self, x: i32, y: i32, z: i32) -> Block {
        // Ores form small clusters: pick a 2x2x2 cell, then thin it out.
        let cell = hash3(x >> 1, y >> 1, z >> 1, self.seed ^ 0x0E5) % 1000;
        let ore = match cell {
            0..=11 if y < 128 => Block::COAL_ORE,
            12..=18 if y < 64 => Block::IRON_ORE,
            19..=21 if y < 32 => Block::GOLD_ORE,
            22..=23 if y < 16 => Block::DIAMOND_ORE,
            24..=25 if y < 32 => Block::LAPIS_ORE,
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
                // Nothing this chunk could hold: skip the rest of the work.
                if col.height + TREE_TOP < base.y || col.height > base.y + CHUNK_SIZE_I {
                    continue;
                }
                let density = match col.biome {
                    Biome::Jungle => 0.95,
                    Biome::Forest => 0.75,
                    Biome::BirchForest => 0.7,
                    Biome::Taiga => 0.6,
                    Biome::Swamp => 0.3,
                    Biome::Savanna => 0.14,
                    Biome::Plains => 0.06,
                    Biome::Snowy => 0.08,
                    Biome::Mountains if col.height < 125 => 0.12,
                    Biome::Desert => 0.1,
                    Biome::Badlands => 0.05,
                    _ => 0.0,
                };
                let roll = ((h >> 16) & 0xFFFF) as f32 / 65536.0;
                if roll >= density || col.height <= SEA_LEVEL {
                    continue;
                }
                let variant = (h >> 32) as u32;
                let pick = (h >> 24) % 100;
                let ground = IVec3::new(tx, col.height, tz);
                let put = &mut |p, b| Self::put(blocks, base, p, b);
                match (col.biome, self.surface_block(col, tx, tz)) {
                    (Biome::Desert, Block::SAND) | (Biome::Badlands, Block::RED_SAND) => cactus(ground, variant, put),
                    (Biome::Taiga | Biome::Snowy, _) => spruce(ground, variant, put),
                    (biome, Block::GRASS) => match biome {
                        Biome::BirchForest => birch(ground, variant, put),
                        Biome::Forest if pick < 20 => birch(ground, variant, put),
                        Biome::Jungle if pick < 15 => mega_jungle(ground, variant, put),
                        Biome::Jungle if pick < 55 => jungle(ground, variant, put),
                        Biome::Jungle => jungle_bush(ground, variant, put),
                        Biome::Savanna if pick < 80 => acacia(ground, variant, put),
                        Biome::Swamp => swamp_oak(ground, variant, put),
                        _ => oak(ground, variant, put),
                    },
                    _ => {}
                }
            }
        }
    }

    /// Scatters grass, flowers, ferns, dead bushes, sugar cane, pumpkins
    /// and melons on the untouched surface (after trees, so trunks keep
    /// their spot). Only columns whose ground and the cell above both lie
    /// in this chunk get plants.
    fn place_plants(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, cols: &[[Column; CHUNK_SIZE]; CHUNK_SIZE]) {
        for (z, row) in cols.iter().enumerate() {
            for (x, col) in row.iter().enumerate() {
                let y = col.height - base.y;
                if !(0..CHUNK_SIZE_I - 1).contains(&y) || col.height < SEA_LEVEL {
                    continue;
                }
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let (ground, above) = (index(x, y as usize, z), index(x, y as usize + 1, z));
                if blocks[above] != Block::AIR || blocks[ground] != self.surface_block(*col, wx, wz) {
                    continue; // carved by a cave, or covered by a tree
                }
                let roll = hash_f(wx, 0, wz, self.seed ^ 0x9A5);

                // Sugar cane on the banks of rivers, lakes and the sea.
                let soil = blocks[ground];
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

                // Flowers grow in patches: a coarse cell decides the colour.
                let patch = hash3(wx >> 3, 0, wz >> 3, self.seed ^ 0xF10);
                let flower = if patch.is_multiple_of(2) { Block::DANDELION } else { Block::POPPY };
                let flower_chance = if patch % 5 < 2 { 0.06 } else { 0.0 };
                // Pumpkins are rare, and come in small clusters.
                let pumpkins = hash3(wx >> 4, 1, wz >> 4, self.seed ^ 0x9C1).is_multiple_of(24);
                use Biome::*;
                let plant = match (col.biome, soil) {
                    (Plains | Forest | BirchForest, Block::GRASS) if pumpkins && roll > 0.96 => Block::PUMPKIN,
                    (Plains, Block::GRASS) if roll < flower_chance => flower,
                    (Plains, Block::GRASS) if roll < 0.3 => Block::TALL_GRASS,
                    (Forest | BirchForest, Block::GRASS) if roll < flower_chance * 0.5 => flower,
                    (Forest | BirchForest, Block::GRASS) if roll < 0.15 => Block::TALL_GRASS,
                    (Savanna, Block::GRASS) if roll < 0.45 => Block::TALL_GRASS,
                    (Jungle, Block::GRASS) if roll < 0.006 => Block::MELON,
                    (Jungle, Block::GRASS) if roll < 0.2 => Block::FERN,
                    (Jungle, Block::GRASS) if roll < 0.5 => Block::TALL_GRASS,
                    (Swamp, Block::GRASS) if roll < 0.03 => Block::BLUE_ORCHID,
                    (Swamp, Block::GRASS) if roll < 0.2 => Block::TALL_GRASS,
                    (Mountains, Block::GRASS) if roll < 0.08 => Block::TALL_GRASS,
                    (Taiga, Block::SNOWY_GRASS) if roll < 0.06 => Block::FERN,
                    (Taiga, Block::SNOWY_GRASS) if roll < 0.1 => Block::TALL_GRASS,
                    (Desert, Block::SAND) if roll < 0.01 => Block::DEAD_BUSH,
                    (Badlands, Block::RED_SAND) if roll < 0.015 => Block::DEAD_BUSH,
                    _ => continue,
                };
                blocks[above] = plant;
            }
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
    /// leaves; leaves only fill air. This makes overlapping trees resolve the
    /// same way no matter which chunk (and order) places them.
    #[inline]
    fn put(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, p: IVec3, b: Block) {
        let l = p - base;
        if l.cmplt(IVec3::ZERO).any() || l.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
            return;
        }
        let slot = &mut blocks[index(l.x as usize, l.y as usize, l.z as usize)];
        if *slot == Block::AIR || (!b.is_leaves() && slot.is_leaves()) {
            *slot = b;
        }
    }

    /// Finds a dry-land spawn point near the origin.
    pub fn find_spawn(&self) -> IVec3 {
        if self.end.is_some() {
            return super::end::SPAWN;
        }
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

type Put<'a> = &'a mut dyn FnMut(IVec3, Block);

const DIRS: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

/// A tree of `wood` grown from a sapling: blocks handed to `put`, leaves
/// before logs. `v` picks the variant.
pub fn tree(wood: Wood, ground: IVec3, v: u32, put: Put) {
    match wood {
        Wood::Oak => oak(ground, v, put),
        Wood::Spruce => spruce(ground, v, put),
        Wood::Birch => birch(ground, v, put),
        Wood::Jungle => jungle(ground, v, put),
        Wood::Acacia => acacia(ground, v, put),
    }
}

/// The classic round-topped tree: a trunk of `height` with two wide
/// leaf layers (radius `wide`) under two narrow ones.
fn round_tree(ground: IVec3, height: i32, wood: Wood, wide: i32, v: u32, put: Put) {
    let top = ground.y + height;
    for dy in -2..=1 {
        let r: i32 = if dy >= 0 { 1 } else { wide };
        for dz in -r..=r {
            for dx in -r..=r {
                // Trim corners randomly for a less boxy canopy.
                let corner = dx.abs() == r && dz.abs() == r;
                if corner && (dy == 1 || r > 2 || (v >> ((dx + dz * 3 + dy * 7) & 15)) & 1 == 0) {
                    continue;
                }
                put(IVec3::new(ground.x + dx, top + dy, ground.z + dz), wood.leaves());
            }
        }
    }
    for y in ground.y + 1..top {
        put(IVec3::new(ground.x, y, ground.z), wood.log());
    }
}

pub fn oak(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 4 + (v % 3) as i32, Wood::Oak, 2, v, put);
}

/// Taller and slimmer than an oak, with white bark.
fn birch(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 5 + (v % 3) as i32, Wood::Birch, 2, v, put);
}

/// A squat oak with a broad, drooping canopy.
fn swamp_oak(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 5 + (v % 3) as i32, Wood::Oak, 3, v, put);
}

/// A tall, thin jungle tree (what a single jungle sapling grows into).
fn jungle(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 7 + (v % 5) as i32, Wood::Jungle, 2, v, put);
}

/// A jungle floor shrub: one log under a mound of oak leaves.
fn jungle_bush(ground: IVec3, v: u32, put: Put) {
    for dy in 1..=2i32 {
        let r = 3 - dy;
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() == r && dz.abs() == r && (v >> ((dx + dz * 5) & 15)) & 1 == 0 {
                    continue;
                }
                put(ground + IVec3::new(dx, dy, dz), Block::LEAVES);
            }
        }
    }
    put(ground + IVec3::Y, Block::JUNGLE_LOG);
}

/// A giant jungle tree: a 2x2 trunk up to 28 blocks tall under a wide
/// dome, with a couple of leafy side branches.
fn mega_jungle(ground: IVec3, v: u32, put: Put) {
    let height = 18 + (v % 10) as i32;
    let top = ground.y + height;
    let leaves = Wood::Jungle.leaves();
    for (dy, r) in [(-2, 4.5f32), (-1, 4.2), (0, 3.4), (1, 2.3)] {
        for dz in -4..=5 {
            for dx in -4..=5 {
                let (cx, cz) = (dx as f32 - 0.5, dz as f32 - 0.5);
                if cx * cx + cz * cz <= r * r {
                    put(IVec3::new(ground.x + dx, top + dy, ground.z + dz), leaves);
                }
            }
        }
    }
    let mut logs = Vec::new();
    for i in 0..2u32 {
        let y = top - 6 - i as i32 * 5 - ((v >> (4 + i * 2)) & 3) as i32;
        let dir = DIRS[((v >> (10 + i * 2)) & 3) as usize];
        // Start from the trunk block on that side.
        let start = IVec3::new(ground.x + (dir.x > 0) as i32, y, ground.z + (dir.z > 0) as i32);
        let end = start + dir * 2 + IVec3::Y;
        logs.extend([start + dir, start + dir * 2, end]);
        for dz in -1..=1 {
            for dx in -1..=1 {
                for dy in 0..=1 {
                    if dy == 1 && dx != 0 && dz != 0 {
                        continue;
                    }
                    put(end + IVec3::new(dx, dy, dz), leaves);
                }
            }
        }
    }
    for y in ground.y - 1..top {
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(IVec3::new(ground.x + dx, y, ground.z + dz), Wood::Jungle.log());
        }
    }
    for p in logs {
        put(p, Wood::Jungle.log());
    }
}

/// A savanna acacia: a trunk that leans off to one side and ends in a
/// flat, umbrella-like canopy, sometimes with a second smaller branch.
fn acacia(ground: IVec3, v: u32, put: Put) {
    let (log, leaves) = (Wood::Acacia.log(), Wood::Acacia.leaves());
    let rise = 2 + (v % 3) as i32;
    let dir = DIRS[((v >> 3) & 3) as usize];
    let lean = 1 + ((v >> 5) & 1) as i32;
    let mut logs = Vec::new();
    let mut p = ground;
    for _ in 0..rise {
        p += IVec3::Y;
        logs.push(p);
    }
    let fork = p;
    for _ in 0..lean {
        p += dir + IVec3::Y;
        logs.push(p);
    }
    let canopy = |c: IVec3, wide: i32, put: Put| {
        for dz in -wide..=wide {
            for dx in -wide..=wide {
                if dx.abs() + dz.abs() <= wide + 1 && !(dx.abs() == wide && dz.abs() == wide) {
                    put(c + IVec3::new(dx, 1, dz), leaves);
                }
                if dx.abs() <= 1 && dz.abs() <= 1 && wide > 2 {
                    put(c + IVec3::new(dx, 2, dz), leaves);
                }
            }
        }
    };
    canopy(p, 3, put);
    if (v >> 7) & 1 == 1 {
        // A second branch the other way, with its own small canopy.
        let other = -dir;
        let mut q = fork - IVec3::Y;
        for _ in 0..2 {
            q += other + IVec3::Y;
            logs.push(q);
        }
        canopy(q, 2, put);
    }
    for p in logs {
        put(p, log);
    }
}

/// A spruce standing on `ground`: a cone of alternating wide and narrow
/// leaf rings.
pub fn spruce(ground: IVec3, v: u32, put: Put) {
    let height = 6 + (v % 4) as i32;
    let top = ground.y + height;
    let leaves = Wood::Spruce.leaves();
    put(IVec3::new(ground.x, top + 1, ground.z), leaves);
    for i in 0..height - 2 {
        let y = top - i;
        let r = match i {
            0 => 0,
            _ if i % 2 == 1 => 1,
            _ => (i / 2).min(3) - (i / 6),
        };
        for dz in -r..=r {
            for dx in -r..=r {
                if r > 1 && dx.abs() == r && dz.abs() == r {
                    continue;
                }
                put(IVec3::new(ground.x + dx, y, ground.z + dz), leaves);
            }
        }
    }
    for y in ground.y + 1..top {
        put(IVec3::new(ground.x, y, ground.z), Wood::Spruce.log());
    }
}

fn cactus(ground: IVec3, v: u32, put: Put) {
    for y in 1..=1 + (v % 3) as i32 {
        put(ground + IVec3::new(0, y, 0), Block::CACTUS);
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
        assert!(biomes.len() >= 12, "biomes: {biomes:?}");
    }

    #[test]
    fn surface_grows_grass_and_flowers() {
        let g = Generator::new(99);
        let mut counts = std::collections::HashMap::new();
        for cx in -8..8 {
            for cz in -8..8 {
                for cy in 1..4 {
                    g.generate(IVec3::new(cx, cy, cz)).for_each_block(|b| {
                        if b.kind() == crate::world::block::RenderKind::Cross {
                            *counts.entry(b).or_insert(0) += 1;
                        }
                    });
                }
            }
        }
        assert!(counts.get(&Block::TALL_GRASS).copied().unwrap_or(0) > 100, "{counts:?}");
        assert!(counts.contains_key(&Block::DANDELION) || counts.contains_key(&Block::POPPY), "{counts:?}");
    }

    #[test]
    fn badlands_are_banded_and_rivers_run_below_sea_level() {
        let g = Generator::new(99);
        let (mut bands, mut river_depths) = (std::collections::HashSet::new(), Vec::new());
        for i in -150..150 {
            for j in -150..150 {
                let (x, z) = (i * 16, j * 16);
                let c = g.column(x, z);
                match c.biome {
                    Biome::Badlands => {
                        for y in SEA_LEVEL..c.height {
                            bands.insert(g.filler_block(c, y));
                        }
                    }
                    Biome::River => river_depths.push(c.height),
                    _ => {}
                }
            }
        }
        assert!(bands.iter().filter(|b| b.terracotta_colour().is_some()).count() >= 5, "{bands:?}");
        assert!(!river_depths.is_empty() && river_depths.iter().all(|&h| h < SEA_LEVEL));
    }

    #[test]
    fn every_tree_has_a_trunk_of_its_wood_under_its_leaves() {
        for wood in Wood::ALL {
            for v in 0..20u32 {
                let mut blocks = Vec::new();
                tree(wood, IVec3::ZERO, v.wrapping_mul(0x9E37_79B9), &mut |p, b| blocks.push((p, b)));
                assert!(blocks.contains(&(IVec3::Y, wood.log())), "{wood:?} trunk starts on the ground");
                assert!(blocks.iter().any(|&(_, b)| b == wood.leaves()), "{wood:?} has leaves");
                for &(p, _) in &blocks {
                    assert!(p.x.abs() <= TREE_REACH && p.z.abs() <= TREE_REACH && p.y <= TREE_TOP, "{wood:?} {p}");
                }
            }
        }
        // Worldgen-only trees also stay within reach.
        for v in 0..50u32 {
            let v = v.wrapping_mul(0x9E37_79B9);
            for grow in [mega_jungle, jungle_bush, swamp_oak, acacia] {
                let mut blocks = Vec::new();
                grow(IVec3::ZERO, v, &mut |p, b| blocks.push((p, b)));
                assert!(
                    blocks.iter().all(|&(p, _)| p.x.abs() <= TREE_REACH && p.z.abs() <= TREE_REACH && p.y <= TREE_TOP)
                );
            }
        }
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
        // A jungle column well inside the biome (see the biome map).
        let (cx, cz) = (112 >> 5, -1024 >> 5);
        let f = g.foliage(cx, cz);
        let lush = f.iter().filter(|&&group| group == Biome::Jungle.foliage()).count();
        assert!(lush > f.len() / 2, "{lush} of {} columns lush", f.len());
        assert_eq!(*g.foliage(cx, cz), *f, "deterministic");
    }

    #[test]
    fn deep_caves_hold_lava() {
        let g = Generator::new(99);
        let mut lava_heights = Vec::new();
        for cx in -6..6 {
            for cz in -6..6 {
                let base = IVec3::new(cx, 0, cz) * CHUNK_SIZE_I;
                let data = g.generate(IVec3::new(cx, 0, cz));
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            if data.get(x, y, z) == Block::LAVA {
                                lava_heights.push(base.y + y as i32);
                            }
                        }
                    }
                }
            }
        }
        assert!(!lava_heights.is_empty(), "no lava generated");
        assert!(lava_heights.iter().all(|&y| y <= LAVA_LEVEL));
    }
}
