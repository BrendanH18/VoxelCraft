//! Original Nether bone fossils, Generation 3 only. Java's random-spread
//! grid and downward generation-point scan; four designs instead of fourteen.
use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME};
use super::jigsaw;
use super::nether::NetherGen;
use super::nether_biome::NetherBiome;
use super::noise::hash3;
use super::relics::{Relic, RelicPalette, bake};
use super::structure::Rng;
use super::terrain::Generation;
use super::trail_ruins::spread;

pub const SALT: u64 = 14_357_921;
const CACHE_LIMIT: usize = 512;

pub struct NetherFossils {
    seed: u64,
    generation: Generation,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Relic>>>>,
}

impl NetherFossils {
    pub fn new(seed: u64, generation: Generation) -> Self {
        Self { seed, generation, cache: Mutex::new(FxHashMap::default()) }
    }

    pub fn start_chunk(&self, region: IVec2) -> IVec2 {
        spread(self.seed, region, 2, 1, SALT)
    }

    pub fn get(&self, g: &NetherGen, region: IVec2) -> Option<Arc<Relic>> {
        if self.generation < Generation::V3 {
            return None;
        }
        if let Some(v) = self.cache.lock().unwrap().get(&region).cloned() {
            return v;
        }
        let c = self.start_chunk(region);
        let mut rng = Rng(hash3(c.x, 0, c.y, self.seed ^ SALT));
        let at = c * 16 + IVec2::new(rng.below(16) as i32, rng.below(16) as i32);
        // Nether noise settings have generation height 128: below_top 2
        // resolves to 125 (the dimension's build height is 256).
        let initial_y = rng.range(32, 125) as i32;
        let mut y = if g.biomes.biome(at.x, at.y) == NetherBiome::SoulSandValley { initial_y } else { 32 };
        while y > 32 {
            let current = g.base_block_at(IVec3::new(at.x, y, at.y));
            y -= 1;
            let below = g.base_block_at(IVec3::new(at.x, y, at.y));
            if current == Block::AIR && (below == Block::SOUL_SAND || (below.is_solid() && !below.is_fluid())) {
                break;
            }
        }
        let found = if y > 32 && g.biomes.biome(at.x, at.y) == NetherBiome::SoulSandValley {
            let lib = jigsaw::relics();
            let rotation = rng.below(4) as u8;
            let name = format!("nether_fossil/relic_{}", 1 + rng.below(4));
            let origin = IVec3::new(at.x, y, at.y);
            let p = jigsaw::single(lib, &name, origin, rotation, rng.next_u64());
            Some(Arc::new(Relic {
                kind: super::relics::Kind::FossilUpper,
                origin,
                bounds: p.bounds,
                cells: bake(lib, &[p], &RelicPalette { ore: Block::COAL_ORE, integrity: false }),
            }))
        } else {
            None
        };
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.entry(region).or_insert(found).clone()
    }

    pub fn nearest(&self, g: &NetherGen, origin: IVec2, max_blocks: i32) -> Option<IVec3> {
        if self.generation < Generation::V3 {
            return None;
        }
        let centre = origin.div_euclid(IVec2::splat(32));
        let mut best = None;
        let mut distance = (max_blocks as i64).pow(2) + 1;
        for radius in 0..=max_blocks.div_euclid(32) + 1 {
            // Any unvisited region is at least this far along one axis.
            let lower_bound = ((radius - 1) * 32).max(0) as i64;
            if best.is_some() && lower_bound * lower_bound > distance {
                break;
            }
            for z in -radius..=radius {
                for x in -radius..=radius {
                    if x.abs().max(z.abs()) != radius {
                        continue;
                    }
                    if let Some(r) = self.get(g, centre + IVec2::new(x, z)) {
                        let d = IVec2::new(r.origin.x, r.origin.z).as_i64vec2() - origin.as_i64vec2();
                        let d = d.length_squared();
                        if d < distance {
                            distance = d;
                            best = Some(r.origin);
                        }
                    }
                }
            }
        }
        best
    }

    fn nearby(&self, g: &NetherGen, base: IVec3, width: i32) -> Vec<Arc<Relic>> {
        if self.generation < Generation::V3 {
            return Vec::new();
        }
        let min = (IVec2::new(base.x, base.z) - IVec2::splat(16)).div_euclid(IVec2::splat(32));
        let max = (IVec2::new(base.x, base.z) + IVec2::splat(width + 15)).div_euclid(IVec2::splat(32));
        let mut out = Vec::new();
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                if let Some(r) = self.get(g, IVec2::new(x, z)) {
                    out.push(r);
                }
            }
        }
        out
    }

    pub fn paint(&self, g: &NetherGen, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        for r in self.nearby(g, base, CHUNK_SIZE_I) {
            r.paint(blocks, base);
        }
    }

    /// Exposure checks must see the same bones as full chunk generation.
    pub(super) fn column_at(
        &self,
        g: &NetherGen,
        base: IVec3,
        mut blocks: [Block; CHUNK_SIZE_I as usize],
    ) -> [Block; CHUNK_SIZE_I as usize] {
        for r in self.nearby(g, base, 1) {
            for &(p, b) in &r.cells {
                if p.x == base.x && p.z == base.z && (base.y..base.y + CHUNK_SIZE_I).contains(&p.y) {
                    let y = (p.y - base.y) as usize;
                    if !super::geode::cannot_replace(blocks[y]) {
                        blocks[y] = b;
                    }
                }
            }
        }
        blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_java_linear_and_legacy_generations_have_no_fossils() {
        let g = NetherGen::new(12345);
        for r in [IVec2::ZERO, IVec2::splat(-1), IVec2::new(2, -3), IVec2::new(-17, 23)] {
            // spacing - separation = 1, hence both Java offsets are zero.
            assert_eq!(g.fossils.start_chunk(r), r * 2);
        }
        for generation in [Generation::V1, Generation::V2] {
            let old = NetherGen::with_generation(12345, generation);
            assert!(old.fossils.get(&old, IVec2::ZERO).is_none());
        }
    }

    #[test]
    fn fossils_find_valley_floors_above_sea_and_match_columns_and_orders() {
        let g = NetherGen::new(12345);
        let valley = g.biomes.nearest(IVec3::ZERO, NetherBiome::SoulSandValley, 20000).expect("soul sand valley");
        let region = IVec2::new(valley.x, valley.z).div_euclid(IVec2::splat(32));
        let mut seen = 0;
        for z in -20..=20 {
            for x in -20..=20 {
                let c = region + IVec2::new(x, z);
                let Some(r) = g.fossils.get(&g, c) else { continue };
                assert!(r.origin.y > 32 && r.origin.y < 126);
                assert_eq!(g.biomes.biome(r.origin.x, r.origin.z), NetherBiome::SoulSandValley);
                assert_ne!(g.base_block_at(r.origin), Block::AIR);
                assert_eq!(g.base_block_at(r.origin + IVec3::Y), Block::AIR);
                if seen == 0 {
                    eprintln!("nether_fossil: {}", r.origin);
                }
                let base = r.bounds.min.div_euclid(IVec3::splat(CHUNK_SIZE_I)) * CHUNK_SIZE_I;
                for shift in [IVec3::ZERO, IVec3::ONE, IVec3::new(17, 9, 27)] {
                    let base = base + shift;
                    let mut blocks = Box::new([Block::AIR; CHUNK_VOLUME]);
                    g.fossils.paint(&g, &mut blocks, base);
                    for z in 0..CHUNK_SIZE_I {
                        for x in 0..CHUNK_SIZE_I {
                            let p = base + IVec3::new(x, 0, z);
                            let col = g.fossils.column_at(&g, p, [Block::AIR; CHUNK_SIZE_I as usize]);
                            for y in 0..CHUNK_SIZE_I {
                                assert_eq!(
                                    blocks[super::super::chunk::index(x as usize, y as usize, z as usize)],
                                    col[y as usize]
                                );
                            }
                        }
                    }
                }
                g.fossils.cache.lock().unwrap().clear();
                assert_eq!(g.fossils.get(&g, c).unwrap().cells, r.cells);
                seen += 1;
                if seen == 3 {
                    return;
                }
            }
        }
        panic!("only {seen} Nether fossils");
    }
}
