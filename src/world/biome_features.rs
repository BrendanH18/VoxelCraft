//! Generation 2 biome features (GEN-03), a child module of `terrain` so it
//! shares the generator's private surface helpers: mossy boulders and fallen
//! logs, which can cross chunk borders and so are placed from hashed world
//! coordinates exactly like trees, and the column-by-column floor plants
//! (leaf litter, wildflowers, bushes, firefly bushes, dry grass, melon
//! patches). Generation 1 never calls any of it.
//! Biome table and sources: `docs/verification/2026-10-30-v0.13-trees.md`.

use glam::IVec3;

use super::{Biome, Block, CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkColumns, Generator, SEA_LEVEL, ob};
use crate::world::biome_plants as bp;
use crate::world::block::Wood;
use crate::world::chunk::index;
use crate::world::noise::{hash_f, hash3};
use crate::world::trees;

/// Blocks a boulder or log reaches from its hashed origin.
const REACH: i32 = 5;

/// Cell sizes of the hashed placement grids.
const BOULDER_CELL: i32 = 11;
const LOG_CELL: i32 = 13;

/// Share of boulder cells that hold a boulder.
fn boulder_density(biome: Biome) -> f32 {
    use Biome::*;
    match biome {
        OldGrowthPineTaiga | OldGrowthSpruceTaiga => 0.55,
        Taiga | SnowyTaiga => 0.12,
        _ => 0.0,
    }
}

/// Share of log cells that hold a fallen log, and the wood of its tree.
fn log_density(biome: Biome) -> Option<(f32, Wood)> {
    use Biome::*;
    Some(match biome {
        Forest | FlowerForest => (0.3, Wood::Oak),
        BirchForest | OldGrowthBirchForest => (0.35, Wood::Birch),
        DarkForest => (0.4, Wood::DarkOak),
        Taiga | SnowyTaiga | OldGrowthPineTaiga | OldGrowthSpruceTaiga => (0.3, Wood::Spruce),
        Jungle | SparseJungle | BambooJungle => (0.3, Wood::Jungle),
        Swamp => (0.2, Wood::Oak),
        _ => return None,
    })
}

impl Generator {
    /// Boulders and fallen logs whose hashed origin lies near this chunk.
    /// Each chunk writes only its part, so a feature is the same whichever
    /// chunk is generated first.
    pub(super) fn place_ground_features(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        for (kind, cell, salt) in [(0, BOULDER_CELL, 0xB017u64), (1, LOG_CELL, 0xF10Cu64)] {
            let range = |lo: i32, hi: i32| (lo - REACH).div_euclid(cell)..=(hi + REACH).div_euclid(cell);
            for cz in range(base.z, base.z + CHUNK_SIZE_I) {
                for cx in range(base.x, base.x + CHUNK_SIZE_I) {
                    let h = hash3(cx, kind, cz, self.seed ^ salt);
                    let (ox, oz) = (cx * cell + (h % cell as u64) as i32, cz * cell + ((h >> 8) % cell as u64) as i32);
                    if ox < base.x - REACH
                        || ox >= base.x + CHUNK_SIZE_I + REACH
                        || oz < base.z - REACH
                        || oz >= base.z + CHUNK_SIZE_I + REACH
                    {
                        continue;
                    }
                    let roll = ((h >> 16) & 0xFFFF) as f32 / 65536.0;
                    let biome = self.climate_biome(ox, oz);
                    let chance =
                        if kind == 0 { boulder_density(biome) } else { log_density(biome).map_or(0.0, |d| d.0) };
                    if roll >= chance {
                        continue;
                    }
                    let col = self.column(ox, oz);
                    if col.height < SEA_LEVEL || col.height + 8 < base.y || col.height > base.y + CHUNK_SIZE_I {
                        continue;
                    }
                    let variant = (h >> 32) as u32;
                    let ground = IVec3::new(ox, col.height, oz);
                    let steep = {
                        let h = |dx: i32, dz: i32| self.column(ox + dx, oz + dz).height;
                        (h(1, 0) - h(-1, 0)).abs().max((h(0, 1) - h(0, -1)).abs())
                    };
                    let soil = self.surface_block(col, ox, oz, steep);
                    if !(matches!(soil, Block::GRASS | Block::DIRT) || soil == ob::PODZOL || soil == ob::COARSE_DIRT) {
                        continue;
                    }
                    let put = &mut |p, b| Self::put(blocks, base, p, b);
                    if kind == 0 {
                        trees::mossy_boulder(ground, variant, put);
                    } else if let Some((_, wood)) = log_density(col.biome) {
                        // A log lies on level ground only: every cell it covers
                        // shares the origin's height and is dry soil.
                        let along_x = (variant >> 3) & 1 == 0;
                        let step = if along_x { IVec3::X } else { IVec3::Z };
                        let len = 3 + (variant % 3) as i32;
                        let level = (0..len).all(|i| {
                            let c = self.column(ox + step.x * i, oz + step.z * i);
                            c.height == col.height && c.biome == col.biome
                        });
                        if level {
                            trees::fallen_log(ground, variant, wood, put);
                        }
                    }
                }
            }
        }
    }

    /// Leaf litter, wildflowers, bushes, firefly bushes, dry grass and melon
    /// patches on the untouched surface, after the other plants so they only
    /// take cells that are still empty.
    pub(super) fn place_biome_plants(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, columns: &ChunkColumns) {
        use Biome::*;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let col = *columns.at(x, z);
                let y = col.height - base.y;
                // The ground may be the top cell of the chunk below (y = -1):
                // then only the cell above is ours, and the soil is taken to
                // be the column's untouched surface.
                if !(-1..CHUNK_SIZE_I - 1).contains(&y) || col.height < SEA_LEVEL {
                    continue;
                }
                let (wx, wz) = (base.x + x as i32, base.z + z as i32);
                let above = index(x, (y + 1) as usize, z);
                let surface = self.surface_block(col, wx, wz, columns.steepness(x, z));
                let ground = (y >= 0).then(|| index(x, y as usize, z));
                let soil = ground.map_or(surface, |g| blocks[g]);
                if blocks[above] != Block::AIR || soil != surface {
                    continue;
                }
                let roll = hash_f(wx, 0, wz, self.seed ^ 0x6E3);
                let pick = hash3(wx, 1, wz, self.seed ^ 0x6E3);
                let patch =
                    |salt: i32, one_in: u64| hash3(wx >> 3, salt, wz >> 3, self.seed ^ 0x6E4).is_multiple_of(one_in);
                let grassy = matches!(soil, Block::GRASS | Block::SNOWY_GRASS) || soil == ob::PODZOL;
                let segments = 1 + (pick >> 8) as u8 % 4;
                let near_water = || {
                    let h = |dx: usize, dz: usize| columns.cols[z + dz][x + dx].height;
                    [h(0, 1), h(2, 1), h(1, 0), h(1, 2)].into_iter().any(|n| n < SEA_LEVEL)
                };
                let plant = match col.biome {
                    Forest | BirchForest | OldGrowthBirchForest | DarkForest | WoodedBadlands
                        if grassy && patch(2, 3) && roll < 0.4 =>
                    {
                        Some(bp::with_segments(bp::LEAF_LITTER, segments))
                    }
                    Meadow if grassy && roll < 0.1 => Some(bp::with_segments(bp::WILDFLOWERS, segments)),
                    BirchForest | OldGrowthBirchForest if grassy && patch(4, 4) && roll < 0.2 => {
                        Some(bp::with_segments(bp::WILDFLOWERS, segments))
                    }
                    Savanna | SavannaPlateau | WindsweptSavanna if grassy && roll < 0.012 => Some(bp::BUSH),
                    WoodedBadlands if grassy && roll < 0.01 => Some(bp::BUSH),
                    Plains | SunflowerPlains if grassy && roll < 0.004 => Some(bp::BUSH),
                    Swamp | MangroveSwamp if roll < 0.15 && near_water() => Some(bp::FIREFLY_BUSH),
                    Desert if soil == Block::SAND && roll < 0.03 => Some(bp::SHORT_DRY_GRASS),
                    Desert if soil == Block::SAND && roll < 0.04 && y + 2 < CHUNK_SIZE_I => Some(bp::TALL_DRY_GRASS),
                    b if b.is_badlands() && soil == Block::RED_SAND && roll < 0.025 => Some(bp::SHORT_DRY_GRASS),
                    b if b.is_badlands() && soil == Block::RED_SAND && roll < 0.035 && y + 2 < CHUNK_SIZE_I => {
                        Some(bp::TALL_DRY_GRASS)
                    }
                    b if b.is_jungle() && grassy && patch(3, 14) && roll < 0.25 => Some(Block::MELON),
                    _ => None,
                };
                let Some(plant) = plant else { continue };
                if !plant.can_stay_on(soil) {
                    continue;
                }
                if plant == bp::TALL_DRY_GRASS {
                    let top = index(x, (y + 2) as usize, z);
                    if blocks[top] != Block::AIR {
                        continue;
                    }
                    blocks[top] = bp::TALL_DRY_GRASS_TOP;
                }
                blocks[above] = plant;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::terrain::Generation;
    use glam::IVec2;

    fn generator(seed: u64) -> Generator {
        Generator::with_generation(seed, crate::world::terrain::Dimension::Overworld, Generation::V2)
    }

    /// A chunk-sized block array of air at `base`, with the ground features
    /// of this generator written into it.
    fn features_at(g: &Generator, base: IVec3) -> Vec<Block> {
        let mut blocks = Box::new([Block::AIR; CHUNK_VOLUME]);
        g.place_ground_features(&mut blocks, base);
        blocks.to_vec()
    }

    fn find(g: &Generator, biome: Biome) -> IVec3 {
        let at = g.nearest_biome(IVec2::ZERO, biome, 20_000).unwrap_or_else(|| panic!("no {biome:?} near the origin"));
        let col = g.column(at.x, at.z);
        IVec3::new(at.x, col.height, at.z)
    }

    #[test]
    fn boulders_and_logs_are_deterministic_and_continuous_across_chunk_borders() {
        let g = generator(77);
        let mut seen_boulder = false;
        let mut seen_log = false;
        for biome in [Biome::OldGrowthSpruceTaiga, Biome::BirchForest] {
            let spot = find(&g, biome);
            // Two overlapping windows whose borders cut through the spot: the
            // blocks they share must agree, so a feature crossing a chunk
            // border is whole whichever side is generated first.
            for (dx, dz, dy) in [
                (0, 0, 0),
                (11, 5, 32),
                (-9, 17, 0),
                (40, 0, 32),
                (-40, -20, 0),
                (0, 40, 0),
                (30, -30, 32),
                (0, 0, -32),
            ] {
                let a = IVec3::new(
                    (spot.x + dx).div_euclid(32) * 32,
                    (spot.y - 8).div_euclid(32) * 32 + dy,
                    (spot.z + dz).div_euclid(32) * 32,
                );
                for shift in [IVec3::new(16, 0, 0), IVec3::new(0, 0, 16), IVec3::new(16, 0, 16)] {
                    let b = a + shift;
                    let (fa, fb) = (features_at(&g, a), features_at(&g, b));
                    assert_eq!(fa, features_at(&g, a), "deterministic");
                    for z in 0..32 {
                        for y in 0..32 {
                            for x in 0..32 {
                                let p = b + IVec3::new(x, y, z);
                                let l = p - a;
                                if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(32)).all() {
                                    let (ba, bb) = (
                                        fa[index(l.x as usize, l.y as usize, l.z as usize)],
                                        fb[index(x as usize, y as usize, z as usize)],
                                    );
                                    assert_eq!(ba, bb, "{p} differs between windows at {a} and {b}");
                                    seen_boulder |= ba == Block::MOSSY_COBBLESTONE;
                                    seen_log |= ba.is_log();
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(seen_boulder, "an old growth taiga near the origin holds a mossy boulder");
        assert!(seen_log, "a forest holds a fallen log");
    }

    #[test]
    fn generation_one_never_places_the_new_features() {
        let g1 = Generator::with_generation(2024, crate::world::terrain::Dimension::Overworld, Generation::V1);
        let spot = find(&g1, Biome::OldGrowthSpruceTaiga);
        let base = IVec3::new(spot.x.div_euclid(32), (spot.y - 8).div_euclid(32), spot.z.div_euclid(32));
        for dz in -2..=2 {
            for dx in -2..=2 {
                g1.generate(base + IVec3::new(dx, 0, dz)).for_each_block(|b| {
                    assert!(!crate::world::biome_plants::owns(b.0), "{}", b.name());
                });
            }
        }
    }

    /// Every chunk of a 5x5 area around `spot` and the rows above its surface.
    fn area(g: &Generator, spot: IVec3, radius: i32) -> Vec<(IVec3, crate::world::chunk::ChunkData)> {
        let c = IVec3::new(spot.x.div_euclid(32), spot.y.div_euclid(32), spot.z.div_euclid(32));
        let mut out = Vec::new();
        for dy in -1..=1 {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    let p = c + IVec3::new(dx, dy, dz);
                    out.push((p, g.generate(p)));
                }
            }
        }
        out
    }

    fn count(chunks: &[(IVec3, crate::world::chunk::ChunkData)], mut test: impl FnMut(Block) -> bool) -> usize {
        let mut n = 0;
        for (_, c) in chunks {
            c.for_each_block(|b| n += test(b) as usize);
        }
        n
    }

    #[test]
    fn old_growth_taigas_grow_giant_spruces_on_podzol_with_boulders() {
        let g = generator(77);
        let spot = find(&g, Biome::OldGrowthSpruceTaiga);
        let chunks = area(&g, spot, 3);
        assert!(count(&chunks, |b| b == Block::SPRUCE_LOG) > 0);
        assert!(count(&chunks, |b| b == ob::PODZOL) > 50, "podzol around the trunks");
        assert!(count(&chunks, |b| b == Block::MOSSY_COBBLESTONE) > 0, "mossy boulders");
        // A 2x2 trunk somewhere: four logs in a square over four more.
        let mut found = false;
        for (_, c) in &chunks {
            for y in 0..31 {
                for z in 0..31 {
                    for x in 0..31 {
                        let log = |dx: usize, dy: usize, dz: usize| c.get(x + dx, y + dy, z + dz) == Block::SPRUCE_LOG;
                        found |= log(0, 0, 0)
                            && log(1, 0, 0)
                            && log(0, 0, 1)
                            && log(1, 0, 1)
                            && log(0, 1, 0)
                            && log(1, 1, 1);
                    }
                }
            }
        }
        assert!(found, "a 2x2 spruce trunk");
    }

    #[test]
    fn new_floor_plants_grow_in_their_biomes() {
        use crate::world::biome_plants as bp;
        let g = generator(31);
        for (biome, wanted) in [
            (Biome::Desert, vec![bp::SHORT_DRY_GRASS, bp::CACTUS_FLOWER]),
            (Biome::DarkForest, vec![bp::LEAF_LITTER]),
            (Biome::Meadow, vec![bp::WILDFLOWERS]),
            (Biome::Swamp, vec![bp::FIREFLY_BUSH]),
        ] {
            let spot = find(&g, biome);
            let chunks = area(&g, spot, 4);
            for b in wanted {
                let n = count(&chunks, |x| x.base() == b);
                assert!(n > 0, "{biome:?} should hold {}", b.name());
            }
        }
    }

    /// Cost of the generation-2 features: the same surface chunks made by
    /// both generations of one generator seed. Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "timing measurement"]
    fn generation_two_features_cost_little_chunk_time() {
        let time = |generation| {
            let g = Generator::with_generation(5, crate::world::terrain::Dimension::Overworld, generation);
            let start = std::time::Instant::now();
            let mut chunks = 0;
            for z in -12..12 {
                for x in -12..12 {
                    for y in 0..4 {
                        g.generate(IVec3::new(x, y, z));
                        chunks += 1;
                    }
                }
            }
            (start.elapsed().as_secs_f64() * 1000.0 / f64::from(chunks), g)
        };
        let _ = time(Generation::V1); // warm caches and code
        let (v1, _) = time(Generation::V1);
        let (v2, _) = time(Generation::V2);
        eprintln!("ms per chunk: generation 1 {v1:.3}, generation 2 {v2:.3}");
        assert!(v2 < v1 * 1.5, "the features should not slow generation by half");
    }
}
