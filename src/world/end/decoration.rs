//! Generation 7 (v1.0 gap-closing Track 4b, GEN-04b): Java 26.3's biome
//! decoration of the outer End, as `ChunkGenerator.applyBiomeDecoration`
//! runs it for each chunk.
//!
//! The End biomes' features, by `GenerationStep.Decoration` step, with
//! their index in the step (`FeatureSorter.buildFeaturesPerStep` over
//! `TheEndBiomeSource`'s biomes):
//!
//! - step 0 (`RAW_GENERATION`), `small_end_islands`: `end_island_decorated`
//!   (index 0): `rarity_filter` 14, `count` 1 (weight 3) or 2 (weight 1),
//!   `in_square`, `height_range` uniform 55..=70, `biome`, then
//!   `EndIslandFeature`;
//! - step 4 (`SURFACE_STRUCTURES`), `end_highlands`: `end_gateway_return`
//!   (index 0; `end_spike` is index 1): `rarity_filter` 700, `in_square`,
//!   `heightmap` `MOTION_BLOCKING`, `offset` y uniform 3..=9, `biome`, then
//!   `EndGatewayFeature` with the exact exit (100, 50, 0);
//! - step 9 (`VEGETAL_DECORATION`), `end_highlands`: `chorus_plant`
//!   (index 0): `count` uniform 0..=4, `in_square`, `heightmap`
//!   `MOTION_BLOCKING`, `biome`, then `ChorusPlantFeature` (air with end
//!   stone below, `ChorusFlowerBlock.generatePlant(.., 8)`);
//! - step 10 (`TOP_LAYER_MODIFICATION`), `the_end`: `end_platform` at
//!   (100, 49, 0), which [`super::EndGen::generate`] has always built.
//!
//! Each feature draws from a `WorldgenRandom` (Xoroshiro) seeded with
//! `setDecorationSeed(world seed, chunk's block x, z)` then
//! `setFeatureSeed(decoration, index, step)`; `FeaturePlacer` runs the
//! modifiers depth first, so each copy a `count` makes is placed (and draws
//! the feature's own numbers) before the next is spread. The `biome` filter
//! reads the biome through `BiomeManager`'s fuzzy zoom
//! ([`crate::world::biome_zoom`]) at the placed block. `MOTION_BLOCKING`
//! counts the `#blocks_motion_in_heightmap` blocks (not chorus, nor the
//! gateway block), as they stand when the feature runs: the terrain, the
//! islands, the chunk's End city pieces (placed in step 4 before its
//! features) and, for chorus, the return gateway.
//!
//! Java decorates chunks in whatever order the server reaches them, and a
//! feature can reach into a neighbouring chunk; where two chunks' features
//! meet, which comes first is not fixed in Java either. Here each chunk's
//! features see every island, city and gateway around them (as if the
//! neighbours had decorated first), and its chorus sees the chorus of the
//! neighbours before it in a fixed order ([`decorates_before`]), each as
//! it grows alone.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{EndBiome, EndGen};
use crate::world::block::Block;
use crate::world::end_blocks::{CHORUS_PLANT, MAX_SPREAD, generate_plant};
use crate::world::end_city::City;
use crate::world::worldgen_random::{RAW_GENERATION, SURFACE_STRUCTURES, VEGETAL_DECORATION, WorldgenRandom};

/// `end_island_decorated`'s index in `RAW_GENERATION`.
const ISLAND_INDEX: i32 = 0;
/// `end_gateway_return`'s index in `SURFACE_STRUCTURES`.
const GATEWAY_INDEX: i32 = 0;
/// `chorus_plant`'s index in `VEGETAL_DECORATION`.
const CHORUS_INDEX: i32 = 0;
/// `end_gateway_return`'s exit (`ServerLevel.END_SPAWN_POINT`), exact.
pub const RETURN_EXIT: IVec3 = IVec3::new(100, 50, 0);
/// How far an `EndIslandFeature` island reaches from its origin.
const ISLAND_REACH: i32 = 6;
const CACHE_LIMIT: usize = 1024;

/// Generation 7's decoration caches, per 16-block chunk.
#[derive(Default)]
pub(super) struct Caches {
    islands: Mutex<FxHashMap<IVec2, Arc<Vec<IVec3>>>>,
    gateways: Mutex<FxHashMap<IVec2, Option<IVec3>>>,
    /// Each chunk's chorus grown alone.
    alone: Mutex<FxHashMap<IVec2, super::Plants>>,
}

/// Whether chunk `a` decorates before its neighbour `b` here: Java
/// decorates in whatever order the server reaches chunks, and a feature
/// reaching into a chunk decorated earlier meets what is there. VoxelCraft
/// fixes the order west to east, then north to south (lower x, then lower
/// z, first), the order the fixture's server mostly took.
fn decorates_before(a: IVec2, b: IVec2) -> bool {
    (a.x, a.y) < (b.x, b.y)
}

fn cached<T: Clone>(cache: &Mutex<FxHashMap<IVec2, T>>, chunk: IVec2, make: impl FnOnce() -> T) -> T {
    if let Some(found) = cache.lock().unwrap().get(&chunk) {
        return found.clone();
    }
    let made = make();
    let mut cache = cache.lock().unwrap();
    if cache.len() >= CACHE_LIMIT
        && let Some(key) = cache.keys().next().copied()
    {
        cache.remove(&key);
    }
    cache.entry(chunk).or_insert(made).clone()
}

/// What decides a block before chorus grows: the terrain, the islands and
/// cities around, and (once the gateways are in) the return gateways.
struct View<'a> {
    end: &'a EndGen,
    islands: FxHashSet<IVec3>,
    cities: Vec<Arc<City>>,
    gateways: Vec<IVec3>,
}

impl<'a> View<'a> {
    /// The view around the 16-block chunk `chunk`, for blocks up to
    /// `reach` outside it; `gateways` adds the return gateways.
    fn new(end: &'a EndGen, chunk: IVec2, reach: i32, gateways: bool) -> Self {
        let lo = chunk * 16 - IVec2::splat(reach);
        let hi = chunk * 16 + IVec2::splat(15 + reach);
        let chunks = |r: i32| {
            let (a, b) = (
                (lo - IVec2::splat(r)).div_euclid(IVec2::splat(16)),
                (hi + IVec2::splat(r)).div_euclid(IVec2::splat(16)),
            );
            (a.y..=b.y).flat_map(move |z| (a.x..=b.x).map(move |x| IVec2::new(x, z)))
        };
        let islands =
            chunks(ISLAND_REACH).flat_map(|c| end.decorated_islands(c).iter().copied().collect::<Vec<_>>()).collect();
        let gateways = if gateways { chunks(1).filter_map(|c| end.return_gateway(c)).collect() } else { Vec::new() };
        Self { end, islands, cities: end.cities_near(lo, hi), gateways }
    }

    /// The block at `p` once the islands, cities and gateways are in.
    fn block(&self, p: IVec3) -> Block {
        for &g in self.gateways.iter().rev() {
            if let Some(b) = super::gateway_block(g, p) {
                return b;
            }
        }
        if let Some(b) = self.cities.iter().rev().find_map(|c| c.block_at(p)) {
            return b;
        }
        if self.islands.contains(&p) || self.end.solid(p) { Block::END_STONE } else { Block::AIR }
    }

    /// `MOTION_BLOCKING`: one above the highest `#blocks_motion_in_heightmap`
    /// block of the column, or 0 (the End's floor) if there is none.
    fn motion_blocking(&self, x: i32, z: i32) -> i32 {
        let mut top = self.end.column(x, z).map_or(-1, |(top, _)| top);
        for p in self.islands.iter().filter(|p| p.x == x && p.z == z) {
            top = top.max(p.y);
        }
        for g in &self.gateways {
            top = top.max(g.y + 2);
        }
        for c in &self.cities {
            if (c.bounds.min.x..=c.bounds.max.x).contains(&x) && (c.bounds.min.z..=c.bounds.max.z).contains(&z) {
                top = top.max(c.bounds.max.y);
            }
        }
        (0..=top)
            .rev()
            .find(|&y| crate::world::precipitation::motion_blocking(self.block(IVec3::new(x, y, z))))
            .map_or(0, |y| y + 1)
    }
}

impl EndGen {
    /// The `WorldgenRandom` the feature at `index` of `step` draws from
    /// while the chunk `chunk` decorates.
    fn feature_random(&self, chunk: IVec2, index: i32, step: i32) -> WorldgenRandom {
        WorldgenRandom::structure(self.seed, chunk.x * 16, chunk.y * 16, index, step)
    }

    /// `LevelReader.getBiome` at block `p`, as the `biome` filter reads it:
    /// the biome of the quart `BiomeManager`'s zoom picks.
    pub fn biome_at(&self, p: IVec3) -> EndBiome {
        let (qx, _, qz) = self.zoom.quart(p.x, p.y, p.z);
        self.biome(qx * 4, qz * 4)
    }

    /// Whether `biome` is in the chunk `chunk` or one beside it: the zoom
    /// reads no farther, so a `biome` filter for anything else fails.
    fn near_biome(&self, chunk: IVec2, biome: EndBiome) -> bool {
        (-1..=1).any(|dz| (-1..=1).any(|dx| self.biome((chunk.x + dx) * 16, (chunk.y + dz) * 16) == biome))
    }

    /// `end_island_decorated` in the chunk `chunk`: the end stone of its
    /// islands (generation 7).
    pub fn decorated_islands(&self, chunk: IVec2) -> Arc<Vec<IVec3>> {
        cached(&self.decoration.islands, chunk, || {
            let mut r = self.feature_random(chunk, ISLAND_INDEX, RAW_GENERATION);
            let mut out = Vec::new();
            if r.next_float() < 1.0f32 / 14.0 && self.near_biome(chunk, EndBiome::SmallIslands) {
                // `weighted_list`: 1 with weight 3, 2 with weight 1.
                let count = if r.next_int(4) < 3 { 1 } else { 2 };
                for _ in 0..count {
                    let x = r.next_int(16) + chunk.x * 16;
                    let z = r.next_int(16) + chunk.y * 16;
                    let y = r.next_int(70 - 55 + 1) + 55;
                    let origin = IVec3::new(x, y, z);
                    if self.biome_at(origin) == EndBiome::SmallIslands {
                        out.extend(crate::world::end_portal::island_with(origin, &mut |n| r.next_int(n)));
                    }
                }
            }
            Arc::new(out)
        })
    }

    /// `end_gateway_return` in the chunk `chunk`: its gateway, if it has
    /// one (generation 7).
    pub fn return_gateway(&self, chunk: IVec2) -> Option<IVec3> {
        cached(&self.decoration.gateways, chunk, || {
            let mut r = self.feature_random(chunk, GATEWAY_INDEX, SURFACE_STRUCTURES);
            if r.next_float() >= 1.0f32 / 700.0 || !self.near_biome(chunk, EndBiome::Highlands) {
                return None;
            }
            let x = r.next_int(16) + chunk.x * 16;
            let z = r.next_int(16) + chunk.y * 16;
            let height = View::new(self, chunk, 0, false).motion_blocking(x, z);
            if height <= 0 {
                return None;
            }
            let p = IVec3::new(x, height + r.next_int(9 - 3 + 1) + 3, z);
            (self.biome_at(p) == EndBiome::Highlands).then_some(p)
        })
    }

    /// Generation 7's return gateway in the chunk `chunk`, if any.
    pub fn return_gateway_at_chunk(&self, chunk: IVec2) -> Option<IVec3> {
        self.noise.as_ref()?;
        self.return_gateway(chunk)
    }

    /// Whether `cell` holds a return gateway generation 7 placed: its
    /// exit, exact.
    pub fn return_gateway_at(&self, cell: IVec3) -> Option<IVec3> {
        let chunk = IVec2::new(cell.x >> 4, cell.z >> 4);
        (self.noise.is_some() && self.return_gateway(chunk) == Some(cell)).then_some(RETURN_EXIT)
    }

    /// `chorus_plant` in the chunk `chunk` (generation 7), grown after the
    /// chunks before it (see [`decorates_before`]): their chorus, as each
    /// grows alone, fills cells its plants then can't use.
    pub(super) fn chorus_v7(&self, chunk: IVec2) -> Vec<(IVec3, Block)> {
        let mut taken = FxHashSet::default();
        for dz in -1..=1 {
            for dx in -1..=0 {
                let n = chunk + IVec2::new(dx, dz);
                if decorates_before(n, chunk) {
                    taken.extend(self.chorus_alone(n).iter().map(|&(p, _)| p));
                }
            }
        }
        if taken.is_empty() { (*self.chorus_alone(chunk)).clone() } else { self.grow_chorus(chunk, &taken) }
    }

    /// The chunk's chorus grown with nothing of its neighbours' around.
    fn chorus_alone(&self, chunk: IVec2) -> super::Plants {
        cached(&self.decoration.alone, chunk, || Arc::new(self.grow_chorus(chunk, &FxHashSet::default())))
    }

    /// `chorus_plant` in the chunk `chunk`, with the cells in `taken`
    /// already holding chorus.
    fn grow_chorus(&self, chunk: IVec2, taken: &FxHashSet<IVec3>) -> Vec<(IVec3, Block)> {
        let mut r = self.feature_random(chunk, CHORUS_INDEX, VEGETAL_DECORATION);
        let count = r.next_int(5);
        let mut out: Vec<(IVec3, Block)> = Vec::new();
        if count == 0 || !self.near_biome(chunk, EndBiome::Highlands) {
            return out;
        }
        let view = View::new(self, chunk, MAX_SPREAD, true);
        for _ in 0..count {
            let x = r.next_int(16) + chunk.x * 16;
            let z = r.next_int(16) + chunk.y * 16;
            let height = view.motion_blocking(x, z);
            if height <= 0 {
                continue;
            }
            let origin = IVec3::new(x, height, z);
            if self.biome_at(origin) != EndBiome::Highlands {
                continue;
            }
            let placed = out.clone();
            let empty =
                |p: IVec3| view.block(p) == Block::AIR && !taken.contains(&p) && !placed.iter().any(|(q, _)| *q == p);
            // `#supports_chorus_plant` is end stone alone.
            if !empty(origin) || view.block(origin - IVec3::Y) != Block::END_STONE {
                continue;
            }
            let mut int = |n: u32| r.next_int(n as i32) as u32;
            let plant = generate_plant(origin, MAX_SPREAD, &empty, &mut int);
            debug_assert_eq!(plant.first().map(|&(p, b)| (p, b)), Some((origin, CHORUS_PLANT)));
            for (p, b) in plant {
                match out.iter_mut().find(|(q, _)| *q == p) {
                    Some(c) => c.1 = b,
                    None => out.push((p, b)),
                }
            }
        }
        out
    }

    /// Generation 7's decoration of the section at `base`: the islands,
    /// End cities, return gateways and chorus around it, in Java's step
    /// order.
    pub(super) fn paint_v7(&self, blocks: &mut [Block], base: IVec3) {
        use crate::world::chunk::{CHUNK_SIZE_I, index};
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let inside = |p: IVec3| p.cmpge(base).all() && p.cmple(top).all();
        let mut set = |p: IVec3, b: Block| {
            if inside(p) {
                let l = p - base;
                blocks[index(l.x as usize, l.y as usize, l.z as usize)] = b;
            }
        };
        let chunks = |reach: i32| {
            let lo = IVec2::new(base.x - reach, base.z - reach).div_euclid(IVec2::splat(16));
            let hi = IVec2::new(top.x + reach, top.z + reach).div_euclid(IVec2::splat(16));
            (lo.y..=hi.y).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec2::new(x, z)))
        };
        if base.y <= 70 {
            for c in chunks(ISLAND_REACH) {
                for &p in self.decorated_islands(c).iter() {
                    set(p, Block::END_STONE);
                }
            }
        }
        for city in self.cities_near(IVec2::new(base.x, base.z), IVec2::new(top.x, top.z)) {
            city.paint(blocks, base);
        }
        let mut set = |p: IVec3, b: Block| {
            if inside(p) {
                let l = p - base;
                blocks[index(l.x as usize, l.y as usize, l.z as usize)] = b;
            }
        };
        for c in chunks(1) {
            if let Some(g) = self.return_gateway(c) {
                for dy in -2..=2 {
                    for dz in -1..=1 {
                        for dx in -1..=1 {
                            let p = g + IVec3::new(dx, dy, dz);
                            if let Some(b) = super::gateway_block(g, p) {
                                set(p, b);
                            }
                        }
                    }
                }
            }
        }
        for c in chunks(MAX_SPREAD) {
            for &(p, b) in self.chorus_plants(c).iter() {
                set(p, b);
            }
        }
    }
}

#[cfg(test)]
mod tests;
