//! Generation 2 underground content (GEN-02, v0.13 track 3): the deep dark
//! ([`super::deep_dark`]), glow lichen on cave walls and ceilings, and water
//! and lava springs. It runs once per chunk after carving, ores and the
//! cave biomes' decoration, reading the generated blocks rather than the
//! terrain noise, so it works with whatever shape the terrain has.
//!
//! Every decision is made per cell from hashed world coordinates and
//! noise, so features agree across chunk borders whatever order chunks
//! generate in. Attachments (lichen and vein faces, sensors, springs) need
//! their supporting blocks inside the chunk being generated; whether stone
//! is exposed across a border is read from the pristine terrain
//! ([`Generator::natural_block`]).
//!
//! Sources and differences: `docs/verification/2026-10-30-v0.13-underground.md`.

#[cfg(test)]
mod tests;

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::biome::Climate;
use super::block::{Block, Fluid};
use super::cave_blocks::{self as cb, DIRS, Multiface};
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, WORLD_MIN_Y, index};
use super::noise::{Perlin, hash_f, hash3};
use super::structure::Rng;
use super::terrain::{Column, Generator};

/// What the underground passes read about a chunk being generated.
pub(super) struct Ctx<'a> {
    generator: &'a Generator,
    pub seed: u64,
    pub base: IVec3,
    /// The chunk's blocks before these passes.
    pub snapshot: Box<[Block; CHUNK_VOLUME]>,
    heights: [i32; CHUNK_SIZE * CHUNK_SIZE],
    /// Columns whose climate allows the deep dark.
    deep_columns: [bool; CHUNK_SIZE * CHUNK_SIZE],
    /// Any cell of the chunk is deep dark.
    pub any_deep: bool,
    columns: FxHashMap<IVec2, Column>,
    nodes: FxHashMap<IVec3, [f32; 7]>,
}

impl Ctx<'_> {
    /// Surface height of local column `(x, z)`.
    pub fn height(&self, x: usize, z: usize) -> i32 {
        self.heights[x + z * CHUNK_SIZE]
    }

    /// Whether local cell `(x, y, z)` is in the deep dark.
    pub fn deep(&self, x: usize, y: usize, z: usize) -> bool {
        let wy = self.base.y + y as i32;
        self.deep_columns[x + z * CHUNK_SIZE]
            && wy <= super::deep_dark::TOP_Y
            && wy > WORLD_MIN_Y
            && self.height(x, z) - wy >= super::deep_dark::MIN_DEPTH
    }

    /// Whether the cell at world `p` is open cave (air or still water):
    /// this chunk's blocks inside it, the pristine terrain outside.
    pub fn open(&mut self, p: IVec3) -> bool {
        let l = p - self.base;
        if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
            return open(self.snapshot[index(l.x as usize, l.y as usize, l.z as usize)]);
        }
        open(self.generator.natural_block(p, &mut self.columns, &mut self.nodes))
    }
}

pub(super) fn open(b: Block) -> bool {
    b == Block::AIR || b == Block::WATER
}

/// Generation 2's underground pass over a generated chunk's `blocks`.
/// `column(x, z)` gives a local column's surface height and climate.
pub(super) fn decorate(
    generator: &Generator,
    blocks: &mut [Block; CHUNK_VOLUME],
    base: IVec3,
    column: impl Fn(usize, usize) -> (i32, Climate),
) {
    let mut heights = [0; CHUNK_SIZE * CHUNK_SIZE];
    let mut deep_columns = [false; CHUNK_SIZE * CHUNK_SIZE];
    let mut any_deep = false;
    let mut max_h = i32::MIN;
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let (h, climate) = column(x, z);
            heights[x + z * CHUNK_SIZE] = h;
            max_h = max_h.max(h);
            let deep = climate.erosion < super::deep_dark::MAX_EROSION;
            deep_columns[x + z * CHUNK_SIZE] = deep;
            any_deep |= deep
                && base.y <= super::deep_dark::TOP_Y
                && h - base.y >= super::deep_dark::MIN_DEPTH
                && base.y + CHUNK_SIZE_I > WORLD_MIN_Y + 1;
        }
    }
    // Nothing here is under ground by enough for lichen or springs.
    if base.y > max_h {
        return;
    }
    let mut ctx = Ctx {
        generator,
        seed: generator.seed,
        base,
        snapshot: Box::new(*blocks),
        heights,
        deep_columns,
        any_deep,
        columns: FxHashMap::default(),
        nodes: FxHashMap::default(),
    };
    super::deep_dark::decorate(&mut ctx, blocks);
    glow_lichen(&ctx, blocks);
    springs(&ctx, blocks);
}

// ---- Glow lichen --------------------------------------------------------

const LICHEN_SALT: u64 = 0x611C_4E01;
/// Java's `surface_relative_threshold_filter`: at least 13 blocks down.
const LICHEN_DEPTH: i32 = 13;
/// Lichen grows in clusters where this noise is high...
const LICHEN_CLUSTER: f32 = 0.42;
/// ...on each wall or ceiling face with this chance.
const LICHEN_FACE: f32 = 0.7;

/// The stone glow lichen grows on (Java's `glow_lichen` feature:
/// stone, andesite, diorite, granite, dripstone, calcite, tuff, deepslate).
fn lichen_rock(b: Block) -> bool {
    matches!(
        b,
        Block::STONE
            | Block::ANDESITE
            | Block::DIORITE
            | Block::GRANITE
            | Block::CALCITE
            | Block::TUFF
            | Block::DEEPSLATE
    ) || b == super::overworld_blocks::DRIPSTONE_BLOCK
}

/// Glow lichen in clusters on cave walls and ceilings (never floors), at
/// least [`LICHEN_DEPTH`] blocks under the surface, outside the deep dark.
fn glow_lichen(ctx: &Ctx, blocks: &mut [Block; CHUNK_VOLUME]) {
    let noise = Perlin::new(ctx.seed ^ LICHEN_SALT);
    for y in 0..CHUNK_SIZE {
        let wy = ctx.base.y + y as i32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if ctx.height(x, z) - wy < LICHEN_DEPTH {
                    continue;
                }
                let i = index(x, y, z);
                let here = ctx.snapshot[i];
                // Only cells nothing else has taken this pass.
                if !open(here) || blocks[i] != here || ctx.deep(x, y, z) {
                    continue;
                }
                let p = ctx.base + IVec3::new(x as i32, y as i32, z as i32);
                let f = p.as_vec3();
                if noise.noise3(f.x / 9.0, f.y / 6.0, f.z / 9.0) < LICHEN_CLUSTER {
                    continue;
                }
                let mut faces = 0u8;
                // Up, north, south, west and east: walls and the ceiling.
                for (face, &d) in DIRS.iter().enumerate().skip(1) {
                    let q = IVec3::new(x as i32, y as i32, z as i32) + d;
                    if q.cmplt(IVec3::ZERO).any() || q.cmpge(IVec3::splat(CHUNK_SIZE_I)).any() {
                        continue;
                    }
                    let j = index(q.x as usize, q.y as usize, q.z as usize);
                    if lichen_rock(ctx.snapshot[j])
                        && blocks[j] == ctx.snapshot[j]
                        && hash_f(p.x * 6 + face as i32, p.y, p.z, ctx.seed ^ LICHEN_SALT) < LICHEN_FACE
                    {
                        faces |= 1 << face;
                    }
                }
                if faces != 0 {
                    blocks[i] = cb::multiface(Multiface::GlowLichen, faces, here == Block::WATER);
                }
            }
        }
    }
}

// ---- Springs ------------------------------------------------------------

const JAVA_CELL: i32 = 16;
const WATER_SALT: u64 = 0x5791_0001;
const LAVA_SALT: u64 = 0x5791_0002;
/// `spring_water`: 25 tries per Java chunk, uniform from the bottom to 192.
const WATER_TRIES: u32 = 25;
const WATER_TOP: i32 = 192;
/// `spring_lava`: 20 tries, `very_biased_to_bottom` from the bottom to 8
/// under the top with inner 8.
const LAVA_TRIES: u32 = 20;
const LAVA_TOP: i32 = super::chunk::WORLD_MAX_Y - 1 - 8;
const LAVA_INNER: i32 = 8;

/// `spring_feature`'s `valid_blocks` (the frozen variant's snow and ice
/// aside).
fn spring_rock(b: Block) -> bool {
    matches!(
        b,
        Block::STONE
            | Block::GRANITE
            | Block::DIORITE
            | Block::ANDESITE
            | Block::DEEPSLATE
            | Block::TUFF
            | Block::CALCITE
            | Block::DIRT
            | Block::SNOW
    )
}

/// Java's `Mth.nextInt(random, lo, hi)`: uniform in `lo..=hi` (`lo` when
/// the range is empty).
fn between(rng: &mut Rng, lo: i32, hi: i32) -> i32 {
    if lo >= hi { lo } else { lo + rng.below((hi - lo + 1) as u32) as i32 }
}

/// Spring origins whose cells lie in the chunk at `base`, with their fluid:
/// tries per Java 16×16 cell at hashed positions, Java's counts and height
/// providers.
fn spring_origins(seed: u64, base: IVec3) -> Vec<(IVec3, Fluid)> {
    let mut out = Vec::new();
    let (lo_cx, hi_cx) = (base.x.div_euclid(JAVA_CELL), (base.x + CHUNK_SIZE_I - 1).div_euclid(JAVA_CELL));
    let (lo_cz, hi_cz) = (base.z.div_euclid(JAVA_CELL), (base.z + CHUNK_SIZE_I - 1).div_euclid(JAVA_CELL));
    let inside = |p: IVec3| (p.y - base.y).clamp(0, CHUNK_SIZE_I - 1) == p.y - base.y;
    for cz in lo_cz..=hi_cz {
        for cx in lo_cx..=hi_cx {
            for (fluid, salt, tries) in [(Fluid::Water, WATER_SALT, WATER_TRIES), (Fluid::Lava, LAVA_SALT, LAVA_TRIES)]
            {
                let mut rng = Rng(hash3(cx, cz, salt as i32, seed ^ salt));
                for _ in 0..tries {
                    let x = cx * JAVA_CELL + rng.below(JAVA_CELL as u32) as i32;
                    let z = cz * JAVA_CELL + rng.below(JAVA_CELL as u32) as i32;
                    let y = match fluid {
                        Fluid::Water => between(&mut rng, WORLD_MIN_Y, WATER_TOP),
                        Fluid::Lava => {
                            let l = between(&mut rng, WORLD_MIN_Y + LAVA_INNER, LAVA_TOP);
                            let m = between(&mut rng, WORLD_MIN_Y, l - 1);
                            between(&mut rng, WORLD_MIN_Y, m - 1 + LAVA_INNER)
                        }
                    };
                    let p = IVec3::new(x, y, z);
                    if inside(p) {
                        out.push((p, fluid));
                    }
                }
            }
        }
    }
    out
}

/// `SpringFeature.place`: stone above and below, the origin stone or air,
/// and of the four sides and the block below exactly four stone and one
/// open (air). Every cell checked lies in the chunk.
fn spring_fits(blocks: &[Block; CHUNK_VOLUME], local: IVec3) -> bool {
    let reach = IVec3::ONE;
    if local.cmplt(reach).any() || local.cmpge(IVec3::splat(CHUNK_SIZE_I) - reach).any() {
        return false;
    }
    let at = |d: IVec3| {
        let q = local + d;
        blocks[index(q.x as usize, q.y as usize, q.z as usize)]
    };
    if !spring_rock(at(IVec3::Y)) || !spring_rock(at(IVec3::NEG_Y)) {
        return false;
    }
    let here = at(IVec3::ZERO);
    if here != Block::AIR && !spring_rock(here) {
        return false;
    }
    let sides = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::NEG_Y];
    let rock = sides.iter().filter(|&&d| spring_rock(at(d))).count();
    let holes = sides.iter().filter(|&&d| at(d) == Block::AIR).count();
    rock == 4 && holes == 1
}

/// Water and lava springs: single sources set into cave walls (and hill
/// sides) that flow once the chunk is simulated (see [`spring_ticks`]).
fn springs(ctx: &Ctx, blocks: &mut [Block; CHUNK_VOLUME]) {
    for (p, fluid) in spring_origins(ctx.seed, ctx.base) {
        let local = p - ctx.base;
        if spring_fits(blocks, local) {
            blocks[index(local.x as usize, local.y as usize, local.z as usize)] = fluid.source();
        }
    }
}

/// The springs in a generated chunk, to schedule their first fluid tick
/// (Java's `scheduleTick` when the feature places one).
pub fn spring_ticks(generator: &Generator, cpos: IVec3, data: &ChunkData) -> Vec<IVec3> {
    let base = cpos * CHUNK_SIZE_I;
    spring_origins(generator.seed, base)
        .into_iter()
        .filter(|&(p, fluid)| {
            let l = p - base;
            data.get(l.x as usize, l.y as usize, l.z as usize) == fluid.source()
                && [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z].iter().any(|&d| {
                    let q = l + d;
                    q.cmpge(IVec3::ZERO).all()
                        && q.cmplt(IVec3::splat(CHUNK_SIZE_I)).all()
                        && data.get(q.x as usize, q.y as usize, q.z as usize) == Block::AIR
                })
        })
        .map(|(p, _)| p)
        .collect()
}
