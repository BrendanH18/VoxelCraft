//! Original Nether bone fossils, Generation 3 only. Java's random-spread
//! grid and downward generation-point scan; four designs instead of fourteen.
//! Like Java's (`"terrain_adaptation": "beard_thin"`), a fossil raises the
//! ground under its box: see `NetherGen::terrain_bits`.
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
use super::structure::{Bounds, Rng};
use super::terrain::Generation;
use super::trail_ruins::spread;

pub const SALT: u64 = 14_357_921;
const CACHE_LIMIT: usize = 512;
/// Margin that covers every region whose fossil's beard can reach a column:
/// a start lies in its region's first chunk, a design reaches at most 12
/// blocks from it (checked by a test) and the kernel 11 more, so a beard
/// stays within blocks -23 to 38 of the region's 32-block span.
const BEARD_REGION_REACH: i32 = 40;

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
        // Java scans `getBaseColumn`: the noise column alone (netherrack,
        // lava below sea level, air), without surface rules, glowstone or
        // any structure's beard. Above sea level 32 nothing is fluid, so
        // `isAir` is open noise and every solid cell has a sturdy top face.
        let bits = if y > 32 { g.base_bits(at.x, at.y) } else { 0 };
        let solid = |y: i32| bits >> y & 1 == 1;
        while y > 32 {
            let current = solid(y);
            y -= 1;
            if !current && solid(y) {
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

    /// The boxes of the fossils whose `beard_thin` reaches a column from
    /// `min` to `max` (inclusive), in region order (z, then x), so a sum
    /// over them adds in the same order whichever area asked. Empty before
    /// generation 3.
    pub(super) fn beard_boxes(&self, g: &NetherGen, min: IVec2, max: IVec2) -> Vec<Bounds> {
        if self.generation < Generation::V3 {
            return Vec::new();
        }
        let lo = (min - IVec2::splat(BEARD_REGION_REACH)).div_euclid(IVec2::splat(32));
        let hi = (max + IVec2::splat(BEARD_REGION_REACH)).div_euclid(IVec2::splat(32));
        let mut out = Vec::new();
        for z in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let Some(r) = self.get(g, IVec2::new(x, z)) else { continue };
                let b = r.bounds;
                // The kernel reaches 11 blocks beyond the box.
                if b.min.x - 11 <= max.x && b.max.x + 11 >= min.x && b.min.z - 11 <= max.y && b.max.z + 11 >= min.y {
                    out.push(b);
                }
            }
        }
        out
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
                // Java's scan: open noise above a solid noise cell.
                let bits = g.base_bits(r.origin.x, r.origin.z);
                assert_eq!(bits >> r.origin.y & 3, 1, "solid start under open noise");
                assert_ne!(g.base_block_at(r.origin), Block::AIR);
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

    /// Defect 1 of the 2026-11-10 captures: seed 9's fossil starts on a
    /// soul-soil ledge over the lava lake and hung in the air. Java's
    /// `beard_thin` raises ground under the box: the kernel just below a
    /// footprint cell (+0.557, then +0.49) beats any squeezed density (at
    /// most 0.458 in magnitude), so the two layers under the floor plane are
    /// rock whatever the noise.
    #[test]
    fn fossil_on_a_ledge_stands_on_bearded_ground() {
        let g = NetherGen::new(9);
        let r = g.fossils.get(&g, IVec2::new(716, 141).div_euclid(IVec2::splat(32))).expect("seed 9 fossil");
        assert_eq!(r.origin, IVec3::new(716, 60, 141));
        let (b, ground) = (r.bounds, r.bounds.min.y);
        assert_eq!(ground, r.origin.y, "ground-level delta 0");
        let footprint: Vec<_> = (b.min.z..=b.max.z).flat_map(|z| (b.min.x..=b.max.x).map(move |x| (x, z))).collect();
        // The unadapted noise leaves the plane unsupported: the overhang.
        let hanging = footprint
            .iter()
            .filter(|&&(x, z)| {
                let bits = g.base_bits(x, z);
                (1..=2).any(|d| bits >> (ground - d) & 1 == 0)
            })
            .count();
        eprintln!("seed 9 fossil: {hanging} of {} footprint columns unsupported before adaptation", footprint.len());
        assert!(hanging > 0, "the start is on a ledge");
        let mut chunks = FxHashMap::default();
        let mut block = |p: IVec3| {
            let c = p.div_euclid(IVec3::splat(CHUNK_SIZE_I));
            let l = p - c * CHUNK_SIZE_I;
            chunks.entry(c).or_insert_with(|| g.generate(c)).get(l.x as usize, l.y as usize, l.z as usize)
        };
        for &(x, z) in &footprint {
            for d in 1..=2 {
                let below = block(IVec3::new(x, ground - d, z));
                assert!(below.is_solid() && !below.is_fluid(), "{below:?} at {x},{},{z}", ground - d);
            }
        }
        // The bones are still painted, on the raised ground.
        assert!(r.cells.iter().filter(|(p, _)| p.y == ground).all(|&(p, bone)| block(p) == bone));
    }

    #[test]
    fn fossil_boxes_sit_on_their_start_within_the_beard_margin() {
        let g = NetherGen::new(12345);
        let valley = g.biomes.nearest(IVec3::ZERO, NetherBiome::SoulSandValley, 20000).expect("soul sand valley");
        let region = IVec2::new(valley.x, valley.z).div_euclid(IVec2::splat(32));
        let mut seen = 0;
        for z in -12..=12 {
            for x in -12..=12 {
                let c = region + IVec2::new(x, z);
                let Some(r) = g.fossils.get(&g, c) else { continue };
                seen += 1;
                let b = r.bounds;
                assert_eq!(b.min.y, r.origin.y);
                let (span_min, span_max) = (c * 32 - IVec2::splat(12), c * 32 + IVec2::splat(15 + 12));
                assert!(
                    b.min.x >= span_min.x && b.max.x <= span_max.x && b.min.z >= span_min.y && b.max.z <= span_max.y
                );
                // Every column the beard reaches finds this box.
                for (px, pz) in
                    [(b.min.x - 11, b.min.z - 11), (b.max.x + 11, b.max.z + 11), (b.min.x - 11, b.max.z + 11)]
                {
                    let p = IVec2::new(px, pz);
                    assert!(g.beard_boxes(p, p).contains(&b), "{p} misses {b:?}");
                }
                let p = IVec2::new(b.min.x - 12, b.min.z);
                assert!(!g.beard_boxes(p, p).contains(&b));
            }
        }
        assert!(seen >= 5, "only {seen} fossils");
        for generation in [Generation::V1, Generation::V2] {
            let old = NetherGen::with_generation(9, generation);
            assert!(old.beard_boxes(IVec2::new(700, 120), IVec2::new(740, 160)).is_empty());
        }
    }
}
