//! Amethyst geodes: Java Edition 26.3's `minecraft:amethyst_geode`
//! configured feature (`GeodeFeature`) and placed feature.
//!
//! Reference data, extracted from the pinned official server artifact (see
//! `tests/fixtures/java/26.3/README.md`): `worldgen/feature/amethyst_geode.json`,
//! `worldgen/placed_feature/amethyst_geode.json`, the `geode_invalid_blocks`
//! and `features_cannot_replace` block tags, and the codec defaults and
//! `place` bytecode of `GeodeFeature`, `GeodeLayerSettings` and
//! `GeodeCrackSettings`. Every Overworld biome lists the feature in its
//! `local_modifications` step, so the biome filter passes everywhere here.
//!
//! Placement per Java 16×16 chunk: `rarity_filter` 1/24, `in_square`, then
//! a uniform height from `above_bottom` 6 (y = -58) to absolute 30. The
//! feature then follows `GeodeFeature.place` step for step: 3–4
//! distribution points at origin + uniform(4, 6) per axis, each with a
//! point offset of 1–2; more than one point in air or an invalid block
//! (bedrock, water, lava, ice) rejects the geode; cells in the
//! origin ±16 box are classified by summed inverse distances plus
//! 0.05 × normal noise into air filling (1.7), amethyst / budding amethyst
//! (2.2, alternate chance 0.083), calcite (3.2) and smooth basalt (4.2),
//! each threshold widened by points / 6. A crack (chance 0.95, base size
//! 2.0, point offset 2) opens air through the shell. Each budding block
//! becomes a bud site with chance 0.35 and grows a random bud or cluster
//! on its first free face in Java's `Direction` order.
//!
//! Chunks generate independently, so each geode is laid out once against
//! the pristine terrain ([`Generator::natural_block`] plus the exact
//! bedrock pattern), cached, and clipped into every chunk it touches.
//! Load order therefore cannot change or cut a geode. Seeds use this
//! engine's hashed per-cell RNG and Perlin noise, not Java's xoroshiro
//! decoration seeds and `NormalNoise`, so a vanilla seed does not
//! reproduce Java's coordinates (see the verification report).

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, WORLD_MAX_Y, WORLD_MIN_Y, index};
use super::noise::{Perlin, hash3};
use super::overworld_blocks as ob;
use super::structure::{Bounds, Rng};
use super::terrain::Generator;

const SALT: u64 = 0x0047_454F_4445_5331;
const NOISE_SALT: u64 = 0x0047_454F_4E4F_4953;
const CACHE_LIMIT: usize = 512;
const JAVA_CHUNK: i32 = 16;

/// `rarity_filter` chance.
pub const RARITY: u32 = 24;
/// `height_range`: `above_bottom` 6 through absolute 30.
pub const MIN_Y: i32 = WORLD_MIN_Y + 6;
pub const MAX_Y: i32 = 30;
/// `min_gen_offset` / `max_gen_offset`: the cube of cells considered.
pub const GEN_OFFSET: i32 = 16;

const OUTER_WALL_DISTANCE: (u32, u32) = (4, 6);
const DISTRIBUTION_POINTS: (u32, u32) = (3, 4);
const POINT_OFFSET: (u32, u32) = (1, 2);
const NOISE_MULTIPLIER: f64 = 0.05;
const INVALID_BLOCKS_THRESHOLD: u32 = 1;
const USE_POTENTIAL_PLACEMENTS_CHANCE: f64 = 0.35;
const USE_ALTERNATE_LAYER0_CHANCE: f64 = 0.083;
const PLACEMENTS_REQUIRE_LAYER0_ALTERNATE: bool = true;
/// `GeodeLayerSettings` defaults (the configured feature's `layers` is empty).
const FILLING: f64 = 1.7;
const INNER_LAYER: f64 = 2.2;
const MIDDLE_LAYER: f64 = 3.2;
const OUTER_LAYER: f64 = 4.2;
/// `GeodeCrackSettings`: configured chance, default size and point offset.
const GENERATE_CRACK_CHANCE: f64 = 0.95;
const BASE_CRACK_SIZE: f64 = 2.0;
const CRACK_POINT_OFFSET: i32 = 2;

/// Java's `Direction.values()` order, used for bud faces.
const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

const SIDE: i32 = 2 * GEN_OFFSET + 1;

/// `#minecraft:geode_invalid_blocks`.
pub fn invalid_block(b: Block) -> bool {
    b == Block::BEDROCK
        || b.is_water()
        || matches!(b.0, 37..=41)
        || b == Block::ICE
        || b == super::gadgets::PACKED_ICE
        || b == ob::BLUE_ICE
}

/// `#minecraft:features_cannot_replace` members this engine has
/// (reinforced deepslate, trial spawners and vaults do not exist yet).
pub fn cannot_replace(b: Block) -> bool {
    b == Block::BEDROCK || b == Block::SPAWNER || b.base() == Block::CHEST || b.base() == Block::END_PORTAL_FRAME
}

/// Java's geode noise: a one-octave `NormalNoise` with first octave -4,
/// that is two improved-Perlin samples at 1/16 frequency, the second
/// scaled by 1.0181…, summed and normalised by 1/6 ÷ 0.2.
struct GeodeNoise {
    a: Perlin,
    b: Perlin,
    offset: [[f64; 3]; 2],
}

impl GeodeNoise {
    fn new(seed: u64) -> Self {
        let mut rng = Rng(seed ^ NOISE_SALT);
        let mut offset = [[0.0; 3]; 2];
        for o in offset.iter_mut().flatten() {
            *o = rng.unit() * 256.0;
        }
        Self { a: Perlin::new(rng.next_u64()), b: Perlin::new(rng.next_u64()), offset }
    }

    fn sample(&self, p: IVec3) -> f64 {
        const INPUT: f64 = 1.0 / 16.0;
        const SECOND: f64 = 1.018_126_888_217_522_7;
        const VALUE: f64 = (1.0 / 6.0) / 0.2;
        // Perlin noise tiles every 256 lattice cells; wrapping first keeps
        // f32 precision at large coordinates.
        let at = |noise: &Perlin, scale: f64, o: [f64; 3]| {
            let w = |v: i32, o: f64| ((v as f64 * scale * INPUT + o).rem_euclid(256.0)) as f32;
            noise.noise3(w(p.x, o[0]), w(p.y, o[1]), w(p.z, o[2])) as f64
        };
        (at(&self.a, 1.0, self.offset[0]) + at(&self.b, SECOND, self.offset[1])) * VALUE
    }
}

/// One geode, laid out against pristine terrain: the final state of every
/// cell it writes, in Java's write order.
#[derive(Debug)]
pub struct Geode {
    pub origin: IVec3,
    pub bounds: Bounds,
    pub cells: Box<[(IVec3, Block)]>,
}

impl Geode {
    fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        if !self.bounds.intersects(&chunk) {
            return;
        }
        for &(p, block) in self.cells.iter() {
            if !chunk.contains(p) {
                continue;
            }
            let l = p - base;
            let i = index(l.x as usize, l.y as usize, l.z as usize);
            // `Feature.safeSetBlock`.
            if !cannot_replace(blocks[i]) {
                blocks[i] = block;
            }
        }
    }
}

/// The working cube of one geode: what each considered cell will hold.
struct Layout {
    origin: IVec3,
    cells: Vec<u16>,
    order: Vec<IVec3>,
}

impl Layout {
    const UNTOUCHED: u16 = u16::MAX;

    fn slot(&self, p: IVec3) -> Option<usize> {
        let l = p - self.origin + IVec3::splat(GEN_OFFSET);
        (l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(SIDE)).all())
            .then(|| (l.x + SIDE * (l.y + SIDE * l.z)) as usize)
    }

    fn get(&self, p: IVec3) -> Option<Block> {
        self.slot(p).map(|i| self.cells[i]).filter(|&b| b != Self::UNTOUCHED).map(Block)
    }

    fn set(&mut self, p: IVec3, b: Block) {
        if let Some(i) = self.slot(p) {
            if self.cells[i] == Self::UNTOUCHED {
                self.order.push(p);
            }
            self.cells[i] = b.0;
        }
    }
}

pub struct Geodes {
    seed: u64,
    noise: GeodeNoise,
    cache: Mutex<FxHashMap<IVec2, Arc<Vec<Arc<Geode>>>>>,
}

impl Geodes {
    pub fn new(seed: u64) -> Self {
        Self { seed, noise: GeodeNoise::new(seed), cache: Mutex::new(FxHashMap::default()) }
    }

    /// The placement origin of the Java 16×16 chunk `c`, if its rarity roll
    /// passes, and the random stream the feature continues with.
    pub fn candidate(&self, c: IVec2) -> Option<(IVec3, Rng)> {
        let mut rng = Rng(hash3(c.x, 0, c.y, self.seed ^ SALT));
        // `rarity_filter`: `nextFloat() < 1 / chance`.
        if rng.unit() >= 1.0 / RARITY as f64 {
            return None;
        }
        let x = c.x * JAVA_CHUNK + rng.below(JAVA_CHUNK as u32) as i32;
        let z = c.y * JAVA_CHUNK + rng.below(JAVA_CHUNK as u32) as i32;
        let y = MIN_Y + rng.range(0, (MAX_Y - MIN_Y) as u32) as i32;
        Some((IVec3::new(x, y, z), rng))
    }

    /// `GeodeFeature.place`, against pristine terrain. `None` when too many
    /// distribution points are open or invalid.
    pub fn build(&self, generator: &Generator, origin: IVec3, mut rng: Rng) -> Option<Geode> {
        let mut heights = FxHashMap::default();
        let mut nodes = FxHashMap::default();
        let mut terrain = |p: IVec3| {
            if generator.bedrock_at(p) {
                return Block::BEDROCK;
            }
            // `natural_block` approximates the bedrock band; the exact
            // pattern is above, so treat its remainder as rock.
            match generator.natural_block(p, &mut heights, &mut nodes) {
                Block::BEDROCK => Block::DEEPSLATE,
                b => b,
            }
        };

        let n = rng.range(DISTRIBUTION_POINTS.0, DISTRIBUTION_POINTS.1);
        let spread = n as f64 / OUTER_WALL_DISTANCE.1 as f64;
        let filling = 1.0 / FILLING.sqrt();
        let inner = 1.0 / (INNER_LAYER + spread).sqrt();
        let middle = 1.0 / (MIDDLE_LAYER + spread).sqrt();
        let outer = 1.0 / (OUTER_LAYER + spread).sqrt();
        let crack_size = 1.0 / (BASE_CRACK_SIZE + rng.unit() / 2.0 + if n > 3 { spread } else { 0.0 }).sqrt();
        let crack = rng.unit() < GENERATE_CRACK_CHANCE;

        let mut invalid = 0;
        let mut points: Vec<(IVec3, i32)> = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let wall = |rng: &mut Rng| rng.range(OUTER_WALL_DISTANCE.0, OUTER_WALL_DISTANCE.1) as i32;
            let p = origin + IVec3::new(wall(&mut rng), wall(&mut rng), wall(&mut rng));
            let b = terrain(p);
            if b == Block::AIR || invalid_block(b) {
                invalid += 1;
                if invalid > INVALID_BLOCKS_THRESHOLD {
                    return None;
                }
            }
            points.push((p, rng.range(POINT_OFFSET.0, POINT_OFFSET.1) as i32));
        }

        let mut cracks: Vec<IVec3> = Vec::new();
        if crack {
            let o = n as i32 * 2 + 1;
            let (dx, dz) = match rng.below(4) {
                0 => (o, 0),
                1 => (0, o),
                2 => (o, o),
                _ => (0, 0),
            };
            cracks.extend([7, 5, 1].map(|dy| origin + IVec3::new(dx, dy, dz)));
        }

        let mut layout =
            Layout { origin, cells: vec![Layout::UNTOUCHED; (SIDE * SIDE * SIDE) as usize], order: Vec::new() };
        let in_world = |p: IVec3| (WORLD_MIN_Y..WORLD_MAX_Y).contains(&p.y);
        // Java's `safeSetBlock` against terrain (the geode is the first
        // writer, so only bedrock among the protected blocks can be there).
        let safe_set = |layout: &mut Layout, p: IVec3, b: Block| {
            if in_world(p) && !generator.bedrock_at(p) {
                layout.set(p, b);
            }
        };
        let inv_dist = |a: IVec3, b: IVec3, offset: i32| 1.0 / ((a - b).length_squared() as f64 + offset as f64).sqrt();
        let mut sites = Vec::new();
        // `BlockPos.betweenClosed` order: x fastest, then y, then z.
        for dz in -GEN_OFFSET..=GEN_OFFSET {
            for dy in -GEN_OFFSET..=GEN_OFFSET {
                for dx in -GEN_OFFSET..=GEN_OFFSET {
                    let p = origin + IVec3::new(dx, dy, dz);
                    // Each point's term adds the noise (Java adds it inside the sum).
                    let near: f64 = points.iter().map(|&(q, offset)| inv_dist(p, q, offset)).sum();
                    // The noise is bounded well below this margin, so far
                    // cells skip the sample without changing any outcome.
                    if near + n as f64 * NOISE_MULTIPLIER * 2.0 < outer {
                        continue;
                    }
                    let s = self.noise.sample(p) * NOISE_MULTIPLIER;
                    let d = near + n as f64 * s;
                    if d < outer {
                        continue;
                    }
                    if d >= filling {
                        safe_set(&mut layout, p, Block::AIR);
                        continue;
                    }
                    let d2: f64 = cracks.iter().map(|&c| inv_dist(p, c, CRACK_POINT_OFFSET) + s).sum();
                    if crack && d2 >= crack_size {
                        // Java also schedules fluid ticks beside crack air.
                        safe_set(&mut layout, p, Block::AIR);
                    } else if d >= inner {
                        let alternate = rng.unit() < USE_ALTERNATE_LAYER0_CHANCE;
                        safe_set(&mut layout, p, if alternate { ob::BUDDING_AMETHYST } else { ob::AMETHYST_BLOCK });
                        if (!PLACEMENTS_REQUIRE_LAYER0_ALTERNATE || alternate)
                            && rng.unit() < USE_POTENTIAL_PLACEMENTS_CHANCE
                        {
                            sites.push(p);
                        }
                    } else if d >= middle {
                        safe_set(&mut layout, p, Block::CALCITE);
                    } else {
                        safe_set(&mut layout, p, ob::SMOOTH_BASALT);
                    }
                }
            }
        }

        for p in sites {
            let size = rng.below(4) as u16;
            for dir in DIRECTIONS {
                let q = p + dir;
                let here = match layout.get(q) {
                    Some(b) => b,
                    None if !in_world(q) => Block::AIR,
                    None => terrain(q),
                };
                // `BuddingAmethystBlock.canClusterGrowAtState`: air or a
                // full water source.
                if here == Block::AIR || here == Block::WATER {
                    safe_set(&mut layout, q, ob::amethyst_bud_facing(size, dir));
                    break;
                }
            }
        }

        let cells: Box<[(IVec3, Block)]> = layout.order.iter().map(|&p| (p, layout.get(p).unwrap())).collect();
        let mut bounds = Bounds { min: origin, max: origin };
        for &(p, _) in cells.iter() {
            bounds.min = bounds.min.min(p);
            bounds.max = bounds.max.max(p);
        }
        Some(Geode { origin, bounds, cells })
    }

    /// The geodes placed from the Java chunks of one 32×32 column.
    pub fn source(&self, generator: &Generator, column: IVec2) -> Arc<Vec<Arc<Geode>>> {
        if let Some(found) = self.cache.lock().unwrap().get(&column) {
            return found.clone();
        }
        let mut geodes = Vec::new();
        let per = CHUNK_SIZE_I / JAVA_CHUNK;
        for sub_z in 0..per {
            for sub_x in 0..per {
                let c = column * per + IVec2::new(sub_x, sub_z);
                if let Some((origin, rng)) = self.candidate(c)
                    && let Some(geode) = self.build(generator, origin, rng)
                {
                    geodes.push(Arc::new(geode));
                }
            }
        }
        let found = Arc::new(geodes);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(k) = cache.keys().next().copied()
        {
            cache.remove(&k);
        }
        cache.entry(column).or_insert(found).clone()
    }

    /// Geodes touching the chunk at `base`, in a fixed order (by origin) so
    /// overlapping geodes resolve the same way whichever chunk loads first.
    pub fn near(&self, generator: &Generator, base: IVec3) -> Vec<Arc<Geode>> {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        if top.y < MIN_Y - GEN_OFFSET || base.y > MAX_Y + GEN_OFFSET {
            return Vec::new();
        }
        let chunk = Bounds { min: base, max: top };
        let lo = IVec2::new(base.x - GEN_OFFSET, base.z - GEN_OFFSET).div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let hi = IVec2::new(top.x + GEN_OFFSET, top.z + GEN_OFFSET).div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let mut out = Vec::new();
        for cz in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                out.extend(
                    self.source(generator, IVec2::new(cx, cz)).iter().filter(|g| g.bounds.intersects(&chunk)).cloned(),
                );
            }
        }
        out.sort_by_key(|g| (g.origin.x, g.origin.y, g.origin.z));
        out
    }

    pub fn paint(&self, generator: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        for geode in self.near(generator, base) {
            geode.paint(blocks, base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every geode laid out for a seed across a square of Java chunks.
    fn sample(seed: u64, radius: i32) -> (Generator, Vec<Arc<Geode>>, u32) {
        let g = Generator::new(seed);
        let mut out = Vec::new();
        let mut candidates = 0;
        for cz in -radius..radius {
            for cx in -radius..radius {
                let column = IVec2::new(cx, cz);
                let per = CHUNK_SIZE_I / JAVA_CHUNK;
                for sz in 0..per {
                    for sx in 0..per {
                        candidates += g.geodes.candidate(column * per + IVec2::new(sx, sz)).is_some() as u32;
                    }
                }
                out.extend(g.geodes.source(&g, column).iter().cloned());
            }
        }
        (g, out, candidates)
    }

    #[test]
    fn noise_stays_inside_the_skip_margin() {
        // The far-cell skip assumes |noise| < 2; Java's normalised noise
        // peaks near 1.7 and ours shares the same construction.
        let noise = GeodeNoise::new(7);
        let mut peak = 0.0f64;
        for x in -40..40 {
            for y in -40..40 {
                for z in -40..40 {
                    peak = peak.max(noise.sample(IVec3::new(x * 3, y * 3, z * 3)).abs());
                }
            }
        }
        assert!(peak < 2.0 && peak > 0.3, "noise peak {peak}");
    }

    #[test]
    fn placement_rate_and_heights_follow_the_placed_feature() {
        let (_, geodes, candidates) = sample(20_261_013, 24);
        // 48×48 columns of four Java chunks each: expect 9216 / 24 = 384.
        let java_chunks = (48 * 48 * 4) as f64;
        let rate = candidates as f64 / java_chunks;
        assert!((rate - 1.0 / 24.0).abs() < 0.01, "candidate rate {rate}");
        assert!(!geodes.is_empty() && geodes.len() <= candidates as usize);
        // Placement validity rejects geodes whose points sit in caves,
        // water or bedrock; most still survive underground.
        assert!(geodes.len() * 3 > candidates as usize, "{} of {candidates}", geodes.len());
        for g in &geodes {
            assert!((MIN_Y..=MAX_Y).contains(&g.origin.y), "{}", g.origin);
            // Every write stays inside Java's ±16 box.
            assert!((g.bounds.min - g.origin).cmpge(IVec3::splat(-GEN_OFFSET)).all());
            assert!((g.bounds.max - g.origin).cmple(IVec3::splat(GEN_OFFSET + 1)).all());
        }
    }

    /// Distance from the geode's air core along a ray must pass through
    /// the layers in order: air, amethyst, calcite, smooth basalt.
    #[test]
    fn layers_nest_air_amethyst_calcite_basalt() {
        let (_, geodes, _) = sample(4242, 10);
        assert!(geodes.len() >= 5, "{} geodes", geodes.len());
        let rank = |b: Block| match b {
            Block::AIR => Some(0),
            ob::AMETHYST_BLOCK | ob::BUDDING_AMETHYST => Some(1),
            Block::CALCITE => Some(2),
            ob::SMOOTH_BASALT => Some(3),
            _ => None,
        };
        let mut checked = 0;
        let (mut amethyst_calcite, mut amethyst_basalt) = (0, 0);
        let mut reached_basalt = 0;
        for g in &geodes {
            let map: FxHashMap<IVec3, Block> = g.cells.iter().copied().collect();
            let counts = |b: Block| g.cells.iter().filter(|c| c.1 == b).count();
            assert!(counts(ob::SMOOTH_BASALT) > counts(Block::CALCITE));
            assert!(counts(Block::CALCITE) > 0 && counts(ob::AMETHYST_BLOCK) > 0);
            // Amethyst faces calcite; the thin layers sampled on a grid let a
            // diagonal step skip calcite only rarely.
            for (&p, &b) in &map {
                if rank(b) != Some(1) {
                    continue;
                }
                for d in DIRECTIONS {
                    match map.get(&(p + d)).copied().and_then(rank) {
                        Some(2) => amethyst_calcite += 1,
                        Some(3) => amethyst_basalt += 1,
                        _ => {}
                    }
                }
            }
            // Rays from the centroid of the filling outward, skipping cracks.
            let air: Vec<IVec3> = g.cells.iter().filter(|c| c.1 == Block::AIR).map(|c| c.0).collect();
            if air.is_empty() {
                continue;
            }
            let centre = air.iter().fold(IVec3::ZERO, |a, &p| a + p) / air.len() as i32;
            if map.get(&centre) != Some(&Block::AIR) {
                continue;
            }
            for dir in DIRECTIONS {
                let mut last = 0;
                let mut p = centre;
                let mut monotone = true;
                for _ in 0..GEN_OFFSET {
                    p += dir;
                    let Some(&b) = map.get(&p) else { break };
                    let Some(r) = rank(b) else { continue };
                    if r < last {
                        monotone = false;
                        break;
                    }
                    last = r;
                }
                // Cracks reopen air through the shell; only crack-free rays
                // must be strictly nested and reach the basalt.
                // A ray of only air left through a crack.
                if monotone && last > 0 {
                    checked += 1;
                    reached_basalt += (last == 3) as u32;
                    assert!(last >= 2, "ray {dir} from {centre} leaves through amethyst");
                }
            }
        }
        assert!(checked > 20, "{checked} rays");
        // Smooth basalt is under a block thick; the odd ray slips through a gap.
        assert!(reached_basalt * 10 >= checked * 8, "{reached_basalt} of {checked} rays reach basalt");
        assert!(amethyst_basalt * 20 < amethyst_calcite, "{amethyst_basalt} vs {amethyst_calcite}");
    }

    #[test]
    fn buds_grow_out_of_budding_amethyst_into_open_cells() {
        let (_, geodes, _) = sample(99, 12);
        let mut buds = 0;
        let mut facings = std::collections::HashSet::new();
        let mut sizes = [0; 4];
        for g in &geodes {
            let map: FxHashMap<IVec3, Block> = g.cells.iter().copied().collect();
            for &(p, b) in g.cells.iter() {
                let Some((size, facing)) = ob::bud_facing(b) else { continue };
                buds += 1;
                sizes[size as usize] += 1;
                facings.insert(facing.to_array());
                // The supporting block sits behind the bud.
                assert_eq!(map.get(&(p - facing)), Some(&ob::BUDDING_AMETHYST), "bud at {p} facing {facing}");
            }
        }
        assert!(buds > 20, "{buds} buds");
        assert_eq!(facings.len(), 6, "all six facings appear: {facings:?}");
        assert!(sizes.iter().all(|&n| n > 0), "{sizes:?}");
    }

    #[test]
    fn budding_share_matches_the_alternate_chance() {
        let (_, geodes, _) = sample(31, 12);
        let (mut inner, mut budding) = (0, 0);
        for g in &geodes {
            for &(_, b) in g.cells.iter() {
                inner += (b == ob::AMETHYST_BLOCK || b == ob::BUDDING_AMETHYST) as u32;
                budding += (b == ob::BUDDING_AMETHYST) as u32;
            }
        }
        let share = budding as f64 / inner as f64;
        assert!((share - USE_ALTERNATE_LAYER0_CHANCE).abs() < 0.015, "{budding} of {inner}: {share}");
    }

    #[test]
    fn rejected_when_points_are_open() {
        let g = Generator::new(5);
        // Far above the surface every distribution point is air.
        let origin = IVec3::new(0, 400, 0);
        assert!(g.geodes.build(&g, origin, Rng(1)).is_none());
    }

    #[test]
    fn chunks_paint_identically_in_any_generation_order() {
        let seed = 4242;
        let (g, geodes, _) = sample(seed, 6);
        let geode = geodes.iter().max_by_key(|g| g.cells.len()).expect("a geode");
        // The chunks a geode spans, generated forward by a fresh generator
        // and in reverse by another, must match block for block.
        let lo = geode.bounds.min.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let hi = geode.bounds.max.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let mut positions = Vec::new();
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    positions.push(IVec3::new(x, y, z));
                }
            }
        }
        let forward = Generator::new(seed);
        let backward = Generator::new(seed);
        let a: Vec<_> = positions.iter().map(|&p| forward.generate(p)).collect();
        let b: Vec<_> = positions.iter().rev().map(|&p| backward.generate(p)).collect();
        let mut seen = 0;
        for (i, p) in positions.iter().enumerate() {
            let (ca, cb) = (&a[i], &b[positions.len() - 1 - i]);
            for z in 0..CHUNK_SIZE_I as usize {
                for y in 0..CHUNK_SIZE_I as usize {
                    for x in 0..CHUNK_SIZE_I as usize {
                        assert_eq!(ca.get(x, y, z), cb.get(x, y, z), "{p} {x} {y} {z}");
                    }
                }
            }
            // The geode's own cells reach the generated chunks.
            for &(q, block) in geode.cells.iter() {
                let l = q - *p * CHUNK_SIZE_I;
                if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                    let got = ca.get(l.x as usize, l.y as usize, l.z as usize);
                    // Later structures (strongholds, mineshafts, dungeons)
                    // and overlapping geodes may overwrite a few cells.
                    seen += (got == block) as u32;
                }
            }
        }
        assert!(seen as usize * 10 >= geode.cells.len() * 9, "{seen} of {}", geode.cells.len());
        // The layout itself is a pure function of the seed.
        let again = Generator::new(seed);
        let column = IVec2::new(geode.origin.x, geode.origin.z).div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let rebuilt = again.geodes.source(&again, column);
        let twin = rebuilt.iter().find(|r| r.origin == geode.origin).expect("same geode");
        assert_eq!(twin.cells, geode.cells);
        drop(g);
    }
}
