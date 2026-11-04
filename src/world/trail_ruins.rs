//! Generation 3 buried jigsaw settlements. Placement and processor settings
//! from Java 26.3; original, reduced template catalogue (GEN-05f / DEF-02).
use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME};
use super::jigsaw::{self, Palette, Placed};
use super::noise::hash3;
use super::overworld_blocks as ob;
use super::structure::Rng;
use super::terrain::{Biome, Dimension, Generation, Generator};
use crate::enchant::JavaRandom;

pub const SPACING: i32 = 34;
pub const SEPARATION: i32 = 8;
pub const SALT: u64 = 83_469_867;
const REACH: i32 = 96;
const CACHE_LIMIT: usize = 256;

/// Java random-spread linear placement, including signed region coordinates.
pub(super) fn spread(seed: u64, region: IVec2, spacing: i32, separation: i32, salt: u64) -> IVec2 {
    let seed = seed
        .wrapping_add((region.x as i64).wrapping_mul(341_873_128_712) as u64)
        .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541) as u64)
        .wrapping_add(salt);
    let mut rng = JavaRandom::new(seed as i64);
    region * spacing + IVec2::new(rng.next_bounded(spacing - separation), rng.next_bounded(spacing - separation))
}

pub fn allowed(b: Biome) -> bool {
    matches!(
        b,
        Biome::Taiga
            | Biome::SnowyTaiga
            | Biome::OldGrowthPineTaiga
            | Biome::OldGrowthSpruceTaiga
            | Biome::OldGrowthBirchForest
            | Biome::Jungle
    )
}

struct RuinPalette;

impl Palette for RuinPalette {
    fn role(&self, _r: &str) -> Option<Block> {
        None
    }

    fn process(&self, b: Block, at: IVec3, p: &Placed) -> Option<Block> {
        // Tower tops have only the capped archaeology processor; with
        // DEF-02's gravel fallback it has no observable block change.
        let name = &jigsaw::trail_ruins().templates[p.template as usize].name;
        if name == "trail_ruins/tower/crown" {
            return Some(b);
        }
        let mut rng = Rng(hash3(at.x, at.y, at.z, p.seed));
        Some(if b == Block::GRAVEL && rng.unit() < 0.2 {
            Block::DIRT
        } else if b == Block::GRAVEL && rng.unit() < 0.1 {
            ob::COARSE_DIRT
        } else if b == ob::MUD_BRICKS && rng.unit() < 0.1 {
            ob::PACKED_MUD
        } else {
            b
        })
    }

    fn clear_above(&self) -> i32 {
        0
    }
}

#[derive(Debug)]
pub struct Ruin {
    pub at: IVec3,
    pub pieces: Vec<Placed>,
}

pub struct TrailRuins {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Ruin>>>>,
}

impl TrailRuins {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    pub fn start_chunk(&self, region: IVec2) -> IVec2 {
        spread(self.seed, region, SPACING, SEPARATION, SALT)
    }

    pub fn get(&self, g: &Generator, region: IVec2) -> Option<Arc<Ruin>> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        if let Some(v) = self.cache.lock().unwrap().get(&region).cloned() {
            return v;
        }
        let at = self.start_chunk(region) * 16;
        // Java projects the jigsaw target to first-air height minus 15,
        // then subtracts SinglePoolElement's ground-level delta of 1.
        let surface = |x, z| g.column(x, z).height.max(crate::world::terrain::SEA_LEVEL - 1) - 15;
        let pieces = jigsaw::assemble(
            jigsaw::trail_ruins(),
            &jigsaw::Settings { start_pool: "trail_ruins/tower", max_depth: 7, max_distance: 80 },
            at,
            &surface,
            hash3(at.x, 0, at.y, self.seed ^ SALT),
        );
        // Jigsaw's generation stub is the rotated start box's centre,
        // which can lie across a biome boundary from its chunk corner.
        let found = pieces.first().copied().and_then(|p| {
            let mut at = (p.bounds.min + p.bounds.max) / 2;
            at.y = p.origin.y + 1;
            allowed(g.surface_biome(at.x, at.z)).then(|| Arc::new(Ruin { at, pieces }))
        });
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.entry(region).or_insert(found).clone()
    }

    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        if g.generation < Generation::V3 {
            return;
        }
        let min = (IVec2::new(base.x, base.z) - IVec2::splat(REACH)).div_euclid(IVec2::splat(16 * SPACING));
        let max =
            (IVec2::new(base.x, base.z) + IVec2::splat(CHUNK_SIZE_I + REACH)).div_euclid(IVec2::splat(16 * SPACING));
        let ground = |x, z| g.column(x, z).height;
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                if let Some(r) = self.get(g, IVec2::new(x, z)) {
                    jigsaw::paint(
                        jigsaw::trail_ruins(),
                        &r.pieces,
                        &RuinPalette,
                        &jigsaw::Ground { surface: &ground, solid: &ground },
                        blocks,
                        base,
                    );
                }
            }
        }
    }

    pub fn nearest(&self, g: &Generator, origin: IVec2, max_blocks: i32) -> Option<IVec3> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        let centre = origin.div_euclid(IVec2::splat(16 * SPACING));
        let rings = max_blocks.div_euclid(16 * SPACING) + 1;
        let mut best = None;
        let mut distance = (max_blocks as i64).pow(2) + 1;
        for z in -rings..=rings {
            for x in -rings..=rings {
                if let Some(r) = self.get(g, centre + IVec2::new(x, z)) {
                    let d = IVec2::new(r.at.x, r.at.z).as_i64vec2() - origin.as_i64vec2();
                    let d = d.length_squared();
                    if d < distance {
                        distance = d;
                        best = Some(r.at);
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::super::chunk::index;
    use super::*;

    #[test]
    fn placement_matches_independent_java_random_starts() {
        // Independently calculated with a Python 48-bit java.util.Random
        // implementation; seed 12345, linear offsets in [0, 26).
        let ruins = TrailRuins::new(12345);
        for (r, c) in [((0, 0), (6, 17)), ((-1, -1), (-25, -13)), ((2, -3), (91, -90)), ((-17, 23), (-578, 791))] {
            assert_eq!(ruins.start_chunk(IVec2::new(r.0, r.1)), IVec2::new(c.0, c.1));
        }
    }

    fn painted(g: &Generator, r: &Ruin, shift: IVec3) -> FxHashMap<IVec3, Block> {
        let cells = super::super::relics::bake(jigsaw::trail_ruins(), &r.pieces, &RuinPalette);
        let bounds = r.pieces.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b)).unwrap();
        let min = (bounds.min - shift).div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let max = (bounds.max - shift).div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let mut out = FxHashMap::default();
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                for y in min.y..=max.y {
                    let base = IVec3::new(x, y, z) * CHUNK_SIZE_I + shift;
                    let mut blocks = Box::new([Block::STONE; CHUNK_VOLUME]);
                    g.trail_ruins.paint(g, &mut blocks, base);
                    for &(p, b) in &cells {
                        let l = p - base;
                        if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                            let got = blocks[index(l.x as usize, l.y as usize, l.z as usize)];
                            assert_eq!(got, b, "missing template cell {p}");
                            out.insert(p, got);
                        }
                    }
                    // No foundations or clearing: cells outside the piece
                    // boxes (including above/below) retain the world stone.
                    for (i, b) in blocks.iter().enumerate() {
                        if *b == Block::STONE {
                            continue;
                        }
                        let p = base + IVec3::new((i % 32) as i32, (i / 1024) as i32, (i / 32 % 32) as i32);
                        assert!(r.pieces.iter().any(|piece| piece.bounds.contains(p)), "out of box: {p}");
                    }
                }
            }
        }
        out
    }

    #[test]
    fn ruins_keep_the_surface_and_clip_across_chunks_and_cache_rebuilds() {
        let g = Generator::new(12345);
        let at = g.trail_ruins.nearest(&g, IVec2::ZERO, 6400).unwrap();
        eprintln!("trail_ruins: {at}");
        let c = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * SPACING));
        let r = g.trail_ruins.get(&g, c).unwrap();
        assert!(r.pieces.len() >= 5);
        let start = r.pieces[0];
        let centre = (start.bounds.min + start.bounds.max) / 2;
        assert_eq!(
            start.origin.y + 1,
            g.column(centre.x, centre.z).height.max(crate::world::terrain::SEA_LEVEL - 1) + 1 - 15
        );
        assert_eq!(r.at.y, start.origin.y + 1);
        assert!(allowed(g.surface_biome(centre.x, centre.z)));
        let a = painted(&g, &r, IVec3::ZERO);
        assert!(
            a.keys().any(|p| p.x.div_euclid(32) != at.x.div_euclid(32) || p.z.div_euclid(32) != at.z.div_euclid(32))
        );
        for shift in [IVec3::ONE, IVec3::new(17, 5, 27)] {
            assert_eq!(a, painted(&g, &r, shift));
        }
        g.trail_ruins.cache.lock().unwrap().clear();
        assert_eq!(g.trail_ruins.get(&g, c).unwrap().pieces, r.pieces);
        assert_eq!(a, painted(&Generator::new(12345), &r, IVec3::ZERO));
        let old = Generator::with_generation(12345, Dimension::Overworld, Generation::V2);
        assert!(old.trail_ruins.nearest(&old, IVec2::ZERO, 6400).is_none());
        assert!(g.trail_ruins.nearest(&g, IVec2::ZERO, 1).is_none());
    }

    #[test]
    fn processor_frequencies_and_biome_tag_match_reference() {
        let mut dirt = 0;
        let mut coarse = 0;
        let mut packed = 0;
        let piece = jigsaw::single(jigsaw::trail_ruins(), "trail_ruins/roads/section_1", IVec3::ZERO, 0, 123);
        for x in 0..100_000 {
            let p = IVec3::new(x, 0, 0);
            let b = RuinPalette.process(Block::GRAVEL, p, &piece).unwrap();
            dirt += (b == Block::DIRT) as i32;
            coarse += (b == ob::COARSE_DIRT) as i32;
            packed += (RuinPalette.process(ob::MUD_BRICKS, p, &piece) == Some(ob::PACKED_MUD)) as i32;
        }
        assert!((dirt - 20000).abs() < 700);
        assert!((coarse - 8000).abs() < 500);
        assert!((packed - 10000).abs() < 500);
        assert!(!allowed(Biome::BambooJungle));
        assert!(!allowed(Biome::SparseJungle));
        assert!(!allowed(Biome::BirchForest));
        for b in [
            Biome::Taiga,
            Biome::SnowyTaiga,
            Biome::OldGrowthPineTaiga,
            Biome::OldGrowthSpruceTaiga,
            Biome::OldGrowthBirchForest,
            Biome::Jungle,
        ] {
            assert!(allowed(b));
        }
    }
}
