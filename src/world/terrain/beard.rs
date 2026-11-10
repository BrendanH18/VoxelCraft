//! Generation 3 surface-structure density adaptation.
//!
//! Shape follows Java 26.3 `Beardifier`: rigid boxes, paired junctions,
//! 0.8/0.4 weights and the [-12, 11] kernel. The native field measures
//! distance in blocks, so it is normalized before addition (see the record).
//! Structure discovery reads *base* columns only: never generated neighbours
//! or these adapted masks. Layout caches are bounded per placement region.
use super::*;
use crate::world::{
    jigsaw::{self, Placed, Projection},
    structure::Bounds,
};
use std::sync::OnceLock;

const HEIGHT: usize = (crate::world::chunk::WORLD_MAX_Y - BOTTOM) as usize;
const WORDS: usize = HEIGHT / 64;
#[derive(Clone)]
pub(super) struct Adaptation {
    solid: Box<[[u64; WORDS]]>,
    /// Positive beard density protects the supporting soil from later carvers.
    support: Box<[[u64; WORDS]]>,
    vegetation: Box<[bool]>,
    heights: Box<[i32]>,
}
impl Adaptation {
    pub fn solid(&self, lx: i32, y: i32, lz: i32) -> bool {
        if y < BOTTOM {
            return true;
        }
        if y >= BOTTOM + HEIGHT as i32 {
            return false;
        }
        Self::bit(&self.solid, lx, y, lz)
    }
    pub fn steepness(&self, lx: i32, lz: i32) -> i32 {
        let h = |x: i32, z: i32| self.heights[(z + 1) as usize * W + (x + 1) as usize];
        (h(lx + 1, lz) - h(lx - 1, lz)).abs().max((h(lx, lz + 1) - h(lx, lz - 1)).abs())
    }
    pub fn vegetation(&self, lx: i32, lz: i32) -> bool {
        self.vegetation[(lz + 1) as usize * W + (lx + 1) as usize]
    }
    pub fn support(&self, lx: i32, y: i32, lz: i32) -> bool {
        (BOTTOM..BOTTOM + HEIGHT as i32).contains(&y) && Self::bit(&self.support, lx, y, lz)
    }
    fn bit(bits: &[[u64; WORDS]], lx: i32, y: i32, lz: i32) -> bool {
        let i = (y - BOTTOM) as usize;
        bits[(lz + 1) as usize * W + (lx + 1) as usize][i / 64] >> (i % 64) & 1 != 0
    }
}
#[derive(Clone, Copy)]
enum Mode {
    Thin,
    Junction,
}
#[derive(Clone, Copy)]
struct Term {
    bounds: Bounds,
    ground: i32,
    weight: f32,
    mode: Mode,
}
impl Term {
    fn vertical_range(&self) -> (i32, i32) {
        match self.mode {
            Mode::Thin | Mode::Junction => (self.ground - 12, self.ground + 11),
        }
    }
}
fn distance(v: i32, lo: i32, hi: i32) -> i32 {
    (lo - v).max(v - hi).max(0)
}

/// Precomputed full beard contribution: exact inverse sqrt rather than Java's
/// fast inverse sqrt; the Gaussian uses f64 then rounds to f32 as Java does.
fn kernel(x: i32, y: i32, z: i32) -> f32 {
    if !(-12..12).contains(&x) || !(-12..12).contains(&y) || !(-12..12).contains(&z) {
        return 0.0;
    }
    static KERNEL: OnceLock<Box<[f32]>> = OnceLock::new();
    let k = KERNEL.get_or_init(|| {
        let mut k = vec![0.0; 24 * 24 * 24];
        for z in -12i32..12 {
            for x in -12i32..12 {
                for y in -12i32..12 {
                    let dy = y as f64 + 0.5;
                    let r2 = (x * x + z * z) as f64 + dy * dy;
                    let gaussian = (-r2 / 16.0).exp() as f32;
                    let sign = -(dy as f32) * (1.0 / (r2 / 2.0).sqrt()) as f32 / 2.0;
                    k[((z + 12) * 24 * 24 + (x + 12) * 24 + y + 12) as usize] = sign * gaussian;
                }
            }
        }
        k.into_boxed_slice()
    });
    k[((z + 12) * 24 * 24 + (x + 12) * 24 + y + 12) as usize]
}
fn add(terms: &mut Vec<Term>, lib: &jigsaw::Library, pieces: &[Placed], base: IVec3, mode: Mode) {
    let near = |b: Bounds| {
        b.max.x + 12 >= base.x - 1
            && b.min.x - 12 <= base.x + CHUNK_SIZE_I
            && b.max.z + 12 >= base.z - 1
            && b.min.z - 12 <= base.z + CHUNK_SIZE_I
    };
    for p in pieces {
        if lib.templates[p.template as usize].projection == Projection::Rigid && near(p.bounds) {
            terms.push(Term { bounds: p.bounds, ground: p.ground_y, weight: 0.8, mode });
        }
        if let Some(js) = p.junctions {
            for at in js {
                let bounds = Bounds { min: at, max: at };
                if near(bounds) {
                    terms.push(Term { bounds, ground: at.y, weight: 0.4, mode: Mode::Junction });
                }
            }
        }
    }
}
impl Generator {
    /// Placement oracle on the *combined* finite density field. A candidate
    /// must preserve every affected rigid piece, not just fit its own raw
    /// height envelope. Reads seed-only columns and never adapted caches.
    pub(in crate::world) fn beard_fit(&self, lib: &jigsaw::Library, pieces: &[Placed], candidate: &Placed) -> bool {
        let all = pieces.iter().chain(std::iter::once(candidate));
        let mut terms = Vec::new();
        for p in all.clone() {
            if lib.templates[p.template as usize].projection == Projection::Rigid {
                terms.push(Term { bounds: p.bounds, ground: p.ground_y, weight: 0.8, mode: Mode::Thin });
            }
            if let Some(junctions) = p.junctions {
                for at in junctions {
                    terms.push(Term {
                        bounds: Bounds { min: at, max: at },
                        ground: at.y,
                        weight: 0.4,
                        mode: Mode::Junction,
                    });
                }
            }
        }
        let mut heights = FxHashMap::default();
        let mut height = |x: i32, z: i32| {
            *heights.entry((x, z)).or_insert_with(|| {
                let c = self.chunk_columns(IVec2::new(x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
                let (lx, lz) = (x.rem_euclid(CHUNK_SIZE_I), z.rem_euclid(CHUNK_SIZE_I));
                let raw = c.at(lx as usize, lz as usize).height;
                let r = c.relief.as_ref().unwrap();
                let local: Vec<_> = terms
                    .iter()
                    .filter_map(|t| {
                        let (dx, dz) = match t.mode {
                            Mode::Thin => (
                                distance(x, t.bounds.min.x, t.bounds.max.x),
                                distance(z, t.bounds.min.z, t.bounds.max.z),
                            ),
                            Mode::Junction => (x - t.bounds.min.x, z - t.bounds.min.z),
                        };
                        ((-12..12).contains(&dx) && (-12..12).contains(&dz)).then_some((t, dx, dz))
                    })
                    .collect();
                let top = local.iter().map(|(t, _, _)| t.ground - 1).max().unwrap_or(raw).max(raw).min(300);
                (BOTTOM..=top)
                    .rev()
                    .find(|&y| {
                        let beard: f32 =
                            local.iter().map(|(t, dx, dz)| t.weight * kernel(*dx, y - t.ground, *dz)).sum();
                        if beard == 0.0 {
                            r.solid(lx, y, lz)
                        } else {
                            (r.density(lx, y, lz) / 32.0).clamp(-0.25, 0.25) + beard > 0.0
                        }
                    })
                    .unwrap_or(BOTTOM)
            })
        };
        for p in all {
            if lib.templates[p.template as usize].projection != Projection::Rigid {
                continue;
            }
            let b = p.bounds;
            let n = candidate.bounds;
            let dx = (b.min.x - n.max.x).max(n.min.x - b.max.x).max(0);
            let dz = (b.min.z - n.max.z).max(n.min.z - b.max.z).max(0);
            // A directed junction can be one block outside this box;
            // signed kernel reach is 12, plus the two-block sample ring.
            if dx > 15 || dz > 15 {
                continue;
            }
            let floor = p.origin.y;
            for z in b.min.z - 1..=b.max.z + 1 {
                for x in b.min.x - 1..=b.max.x + 1 {
                    let h = height(x, z);
                    let outside = x < b.min.x || x > b.max.x || z < b.min.z || z > b.max.z;
                    if outside && (h - floor).abs() > 3 {
                        return false;
                    }
                    if !outside && self.column(x, z).height >= SEA_LEVEL - 1 && floor - 1 - h > 2 {
                        return false;
                    }
                    if (h - height(x + 1, z)).abs() > 3 || (h - height(x, z + 1)).abs() > 3 {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub(super) fn terrain_vegetation(&self, x: i32, z: i32) -> bool {
        if self.generation < Generation::V3 {
            return true;
        }
        let c = self.terrain_columns(IVec2::new(x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
        c.relief.as_ref().and_then(|r| r.adaptation.as_ref()).is_none_or(|a| {
            a.vegetation(x.rem_euclid(CHUNK_SIZE_I), z.rem_euclid(CHUNK_SIZE_I)) && self.terrain_surface_supported(x, z)
        })
    }

    /// Reject a whole tree rather than clipping its crown into raised soil.
    /// Cache at most the few horizontal columns its canopy visits. Terrain
    /// masks are seed-only; existing trees and generated neighbours are irrelevant.
    #[cfg_attr(test, inline(never))]
    pub(super) fn tree_terrain_clear(&self, ground: IVec3, blocks: &[(IVec3, Block)]) -> bool {
        if self.generation < Generation::V3 || self.dimension != Dimension::Overworld {
            return true;
        }
        let mut columns = FxHashMap::default();
        !blocks.iter().any(|(p, _)| {
            if p.y <= ground.y {
                return false;
            } // intentional embedded roots
            let pos = IVec2::new(p.x.div_euclid(CHUNK_SIZE_I), p.z.div_euclid(CHUNK_SIZE_I));
            let c = columns.entry(pos).or_insert_with(|| self.terrain_columns(pos));
            c.relief.as_ref().is_some_and(|r| {
                r.adaptation.is_some() && r.solid(p.x.rem_euclid(CHUNK_SIZE_I), p.y, p.z.rem_euclid(CHUNK_SIZE_I))
            })
        })
    }

    /// Point support for cross-chunk feature origins. Density heights precede
    /// carvers: a cave/ravine entrance must not acquire a floating tree or plant.
    pub(super) fn terrain_surface_supported(&self, x: i32, z: i32) -> bool {
        if self.generation < Generation::V3 || self.dimension != Dimension::Overworld {
            return true;
        }
        let c = self.terrain_columns(IVec2::new(x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
        let Some(a) = c.relief.as_ref().and_then(|r| r.adaptation.as_ref()) else { return true };
        let (lx, lz) = (x.rem_euclid(CHUNK_SIZE_I), z.rem_euclid(CHUNK_SIZE_I));
        let y = c.at(lx as usize, lz as usize).height;
        if a.support(lx, y, lz) {
            return true;
        }
        let at = IVec3::new(x, y, z);
        if !self.natural_block_v2(at, &mut FxHashMap::default()).is_solid() {
            return false;
        }
        let base = at.div_euclid(IVec3::splat(CHUNK_SIZE_I)) * CHUNK_SIZE_I;
        self.caves
            .ravines(base)
            .is_none_or(|mask| !mask[index(lx as usize, y.rem_euclid(CHUNK_SIZE_I) as usize, lz as usize)])
    }

    /// Actual pre-decoration terrain; placement deliberately keeps `column()`.
    pub(in crate::world) fn terrain_column(&self, x: i32, z: i32) -> Column {
        if self.generation < Generation::V3 || self.dimension != Dimension::Overworld {
            return self.column(x, z);
        }
        let c = self.terrain_columns(IVec2::new(x.div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
        *c.at(x.rem_euclid(CHUNK_SIZE_I) as usize, z.rem_euclid(CHUNK_SIZE_I) as usize)
    }
    pub(super) fn terrain_columns(&self, pos: IVec2) -> Arc<ChunkColumns> {
        if self.generation < Generation::V3 || self.dimension != Dimension::Overworld {
            return self.chunk_columns(pos);
        }
        if let Some(c) = self.adapted_columns.lock().unwrap().get(&pos) {
            return Arc::clone(c);
        }
        let base = IVec3::new(pos.x * CHUNK_SIZE_I, 0, pos.y * CHUNK_SIZE_I);
        let mut terms = Vec::new();
        self.villages.terrain_pieces(self, base, |p| add(&mut terms, jigsaw::villages(), p, base, Mode::Thin));
        self.temples.terrain_pieces(self, base, |p| add(&mut terms, jigsaw::outposts(), p, base, Mode::Thin));
        let c = if terms.is_empty() {
            self.chunk_columns(pos)
        } else {
            // Placement has already sampled these seed-only columns. Copy
            // the cached field instead of sampling its noise a second time.
            let mut c = self.chunk_columns(pos).as_ref().clone();
            let r = c.relief.as_mut().unwrap();
            let mut solid = vec![[0u64; WORDS]; W * W].into_boxed_slice();
            let mut support = vec![[0u64; WORDS]; W * W].into_boxed_slice();
            for (sz, row) in c.cols.iter_mut().enumerate() {
                for (sx, col) in row.iter_mut().enumerate() {
                    let (lx, lz) = (sx as i32 - 1, sz as i32 - 1);
                    let (wx, wz) = (base.x + lx, base.z + lz);
                    let local: Vec<_> = terms
                        .iter()
                        .filter_map(|t| {
                            let (dx, dz) = if matches!(t.mode, Mode::Junction) {
                                (wx - t.bounds.min.x, wz - t.bounds.min.z)
                            } else {
                                (
                                    distance(wx, t.bounds.min.x, t.bounds.max.x),
                                    distance(wz, t.bounds.min.z, t.bounds.max.z),
                                )
                            };
                            ((-12..12).contains(&dx) && (-12..12).contains(&dz)).then_some((t, dx, dz))
                        })
                        .collect();
                    let slot = sz * W + sx;
                    for (word, mask) in solid[slot].iter_mut().enumerate() {
                        *mask = r.mask(lx, lz, BOTTOM + word as i32 * 64);
                    }
                    if let (Some(lo), Some(hi)) = (
                        local.iter().map(|(t, _, _)| t.vertical_range().0).min(),
                        local.iter().map(|(t, _, _)| t.vertical_range().1).max(),
                    ) {
                        for y in lo.max(BOTTOM + 6)..=hi.min(300) {
                            let beard: f32 = local
                                .iter()
                                .map(|(t, dx, dz)| match t.mode {
                                    Mode::Thin | Mode::Junction => t.weight * kernel(*dx, y - t.ground, *dz),
                                })
                                .sum();
                            if beard == 0.0 {
                                continue;
                            }
                            // FE-01: native distances become bounded density before
                            // adding Java's kernel; unchanged signs outside its reach.
                            let b = (r.density(lx, y, lz) / 32.0).clamp(-0.25, 0.25) + beard > 0.0;
                            let i = (y - BOTTOM) as usize;
                            let bit = 1 << (i % 64);
                            solid[slot][i / 64] = (solid[slot][i / 64] & !bit) | ((b as u64) << (i % 64));
                            support[slot][i / 64] |= ((beard > 0.0) as u64) << (i % 64);
                        }
                    }
                    let height = (0..WORDS)
                        .rev()
                        .find_map(|i| {
                            (solid[slot][i] != 0)
                                .then(|| BOTTOM + i as i32 * 64 + 63 - solid[slot][i].leading_zeros() as i32)
                        })
                        .unwrap_or(BOTTOM);
                    col.height = height;
                }
            }
            let vegetation = (0..W * W)
                .map(|i| {
                    let (wx, wz) = (base.x + i as i32 % W as i32 - 1, base.z + i as i32 / W as i32 - 1);
                    !terms.iter().any(|t| {
                        matches!(t.mode, Mode::Thin)
                            && t.weight == 0.8
                            && distance(wx, t.bounds.min.x, t.bounds.max.x) <= TREE_REACH
                            && distance(wz, t.bounds.min.z, t.bounds.max.z) <= TREE_REACH
                    })
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let heights =
                c.cols.iter().flat_map(|row| row.iter().map(|c| c.height)).collect::<Vec<_>>().into_boxed_slice();
            r.adaptation = Some(Adaptation { solid, support, vegetation, heights });
            c.max_h = c.cols[1..W - 1].iter().flat_map(|row| row[1..W - 1].iter()).map(|c| c.height).max().unwrap();
            c.min_h = c.cols[1..W - 1].iter().flat_map(|row| row[1..W - 1].iter()).map(|c| c.height).min().unwrap();
            Arc::new(c)
        };
        let mut cache = self.adapted_columns.lock().unwrap();
        if cache.len() >= COLUMN_CACHE_LIMIT {
            cache.clear();
        }
        Arc::clone(cache.entry(pos).or_insert(c))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::world::temples::Kind;

    #[test]
    fn benchmark_origin_has_no_surface_beard() {
        let g = Generator::new(12345);
        for z in -2..=2 {
            for x in -2..=2 {
                assert!(g.terrain_columns(IVec2::new(x, z)).relief.as_ref().unwrap().adaptation.is_none());
            }
        }
    }

    #[test]
    fn tree_canopies_do_not_clip_into_blended_slopes() {
        let g = Generator::new(42);
        let at = g.villages.nearest(&g, IVec3::ZERO).unwrap();
        let mut slope = None;
        'find: for z in at.z - 48..=at.z + 48 {
            for x in at.x - 48..=at.x + 48 {
                let low = g.terrain_column(x, z).height;
                let high = g.terrain_column(x + 1, z).height;
                if high <= low || high - low > TREE_TOP {
                    continue;
                }
                let c = g.terrain_columns(IVec2::new((x + 1).div_euclid(CHUNK_SIZE_I), z.div_euclid(CHUNK_SIZE_I)));
                if c.relief.as_ref().unwrap().adaptation.is_some() {
                    slope = Some((IVec3::new(x, low, z), IVec3::new(x + 1, high, z)));
                    break 'find;
                }
            }
        }
        let (ground, blocked) = slope.expect("a blended slope");
        assert!(!g.tree_terrain_clear(ground, &[(blocked, Block::LEAVES)]));
        assert!(g.tree_terrain_clear(ground, &[(blocked + IVec3::Y * (TREE_TOP + 1), Block::LEAVES)]));
    }

    #[test]
    fn blended_surfaces_and_small_plants_keep_their_top_rules() {
        use crate::world::village::{SPACING, Style};
        for (seed, style) in [(7, Style::Desert), (42, Style::Taiga)] {
            let g = Generator::new(seed);
            let at = g.villages.locate_style(&g, IVec3::ZERO, style, 24).unwrap();
            let r = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * SPACING));
            let v = g.villages.get(&g, r).unwrap();
            let mut chunks = FxHashMap::default();
            let mut block = |x: i32, y: i32, z: i32| {
                let at = IVec3::new(x, y, z);
                let c = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
                let l = at.rem_euclid(IVec3::splat(CHUNK_SIZE_I));
                chunks.entry(c).or_insert_with(|| g.generate(c)).get(l.x as usize, l.y as usize, l.z as usize)
            };
            let mut changed = 0;
            for z in v.bounds.min.z - 12..=v.bounds.max.z + 12 {
                for x in v.bounds.min.x - 12..=v.bounds.max.x + 12 {
                    if v.placed.iter().any(|p| {
                        x >= p.bounds.min.x && x <= p.bounds.max.x && z >= p.bounds.min.z && z <= p.bounds.max.z
                    }) {
                        continue;
                    }
                    let col = g.terrain_column(x, z);
                    if col.height <= SEA_LEVEL || col.height == g.column(x, z).height {
                        continue;
                    }
                    let expected = match col.biome {
                        Biome::Desert => Block::SAND,
                        Biome::Taiga | Biome::Plains | Biome::Forest | Biome::Savanna => Block::GRASS,
                        Biome::SnowyTaiga => Block::SNOWY_GRASS,
                        _ => continue,
                    };
                    // Carvers may remove a surface after its material rules.
                    // Only surviving ground is a top-material assertion.
                    let top = block(x, col.height, z);
                    if matches!(
                        top,
                        Block::AIR | Block::WATER | Block::LAVA | Block::COBBLESTONE | Block::MOSSY_COBBLESTONE
                    ) || top.is_log()
                    {
                        continue;
                    }
                    assert_eq!(block(x, col.height, z), expected, "seed {seed} at {x},{z}");
                    for y in col.height - 8..=col.height + 8 {
                        let b = block(x, y, z);
                        if matches!(
                            b,
                            Block::TALL_GRASS | Block::FERN | Block::DANDELION | Block::POPPY | Block::DEAD_BUSH
                        ) {
                            assert_eq!(y, col.height + 1, "floating/buried {b:?} seed {seed} at {x},{y},{z}");
                        }
                    }
                    changed += 1;
                }
            }
            println!("seed {seed} {style:?}: {changed} changed surface columns outside templates checked");
            assert!(changed >= 8);
        }
    }

    #[test]
    fn kernel_has_java_sign_weights_and_finite_support() {
        assert!((kernel(0, -1, 0) - 0.696144).abs() < 0.00001);
        assert_eq!(kernel(0, -1, 0), -kernel(0, 0, 0));
        assert!(kernel(-12, 0, 0) < 0.0, "the signed negative boundary is included");
        for p in [(12, 0, 0), (0, 12, 0), (0, 0, 12), (-13, 0, 0)] {
            assert_eq!(kernel(p.0, p.1, p.2), 0.0);
        }
        // At the first outside ring a single piece overwhelms normalized
        // native density +/-0.25 at floor-2 and floor+1. Thus its surface
        // is in [floor-2, floor] (two blocks, independent of slope height).
        assert!(0.8 * kernel(1, -2, 0) > 0.25);
        assert!(0.8 * kernel(1, 1, 0) < -0.25);
    }

    pub(in crate::world::terrain) fn fixtures(g: &Generator) -> Vec<(&'static str, Vec<Placed>)> {
        let at = g.villages.nearest(g, IVec3::ZERO).unwrap();
        let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * crate::world::village::SPACING));
        let village = g.villages.get(g, region).unwrap().placed.clone();
        let at = g.temples.nearest(g, Kind::Outpost, IVec2::ZERO, 20_000).unwrap();
        let mut outpost = Vec::new();
        g.temples.terrain_pieces(g, at.div_euclid(IVec3::splat(32)) * 32, |ps| outpost.extend_from_slice(ps));
        assert!(!outpost.is_empty());
        vec![("village", village), ("outpost", outpost)]
    }

    /// Final generated blocks, including surface rules, caves, decoration,
    /// structure painting and fallback foundations (not predicted heights).
    #[test]
    fn sloped_villages_and_outposts_meet_blended_soil() {
        let mut failures = Vec::new();
        for seed in [42, 12345, 2024] {
            let g = Generator::new(seed);
            let mut chunks = FxHashMap::default();
            let mut terrain_chunks = FxHashMap::default();
            // Generate the actual pre-structure terrain blocks, including
            // carvers, to distinguish natural terracotta from identical
            // fallback material. Heights alone cannot identify a pillar.
            let mut terrain = |x: i32, y: i32, z: i32| {
                let at = IVec3::new(x, y, z);
                let c = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
                let l = at.rem_euclid(IVec3::splat(CHUNK_SIZE_I));
                let blocks = terrain_chunks.entry(c).or_insert_with(|| {
                    let base = c * CHUNK_SIZE_I;
                    let cols = g.terrain_columns(IVec2::new(c.x, c.z));
                    let mut blocks = ChunkData::new_dense(Block::AIR);
                    let masks = g.fill_v2(&mut blocks, base, &cols);
                    g.carve_v2(&mut blocks, base, &cols, &masks);
                    blocks
                });
                blocks[index(l.x as usize, l.y as usize, l.z as usize)]
            };
            for (name, ps) in fixtures(&g) {
                let foundation = if name == "village" {
                    let r = IVec2::new(ps[0].origin.x, ps[0].origin.z)
                        .div_euclid(IVec2::splat(16 * crate::world::village::SPACING));
                    jigsaw::Palette::role(&g.villages.get(&g, r).unwrap().style, "foundation").unwrap()
                } else {
                    Block::COBBLESTONE
                };
                let mut block = |x: i32, y: i32, z: i32| {
                    let at = IVec3::new(x, y, z);
                    let c = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
                    let p = at.rem_euclid(IVec3::splat(CHUNK_SIZE_I));
                    chunks.entry(c).or_insert_with(|| g.generate(c)).get(p.x as usize, p.y as usize, p.z as usize)
                };
                let mut before = 0;
                let mut ring_error = 0;
                let mut pillar = 0;
                let mut cut = 0;
                let mut checked = 0;
                let mut worst_ring = (0, 0, 0, 0);
                let mut worst_step = (0, 0, 0, 0);
                let lib = if name == "village" { jigsaw::villages() } else { jigsaw::outposts() };
                // Sandstone/terracotta walls are building material, not terrain.
                // Exclude explicit upper template cells, not arbitrary block types.
                let mut building = rustc_hash::FxHashSet::default();
                for p in &ps {
                    let t = &lib.templates[p.template as usize];
                    if t.projection != Projection::Rigid {
                        continue;
                    }
                    for c in &t.cells {
                        if c.pos.y > 0 && c.spec.base != jigsaw::Base::Block(Block::AIR) {
                            building.insert(p.world(c.pos));
                        }
                    }
                    for &(a, b, spec) in &t.fills {
                        if spec.base == jigsaw::Base::Block(Block::AIR) {
                            continue;
                        }
                        for y in a.y.max(1)..=b.y {
                            for z in a.z..=b.z {
                                for x in a.x..=b.x {
                                    building.insert(p.world(IVec3::new(x, y, z)));
                                }
                            }
                        }
                    }
                }
                for p in ps.iter().filter(|p| lib.templates[p.template as usize].projection == Projection::Rigid) {
                    let b = p.bounds;
                    let floor = p.origin.y;
                    let mut heights = FxHashMap::default();
                    for z in b.min.z - 2..=b.max.z + 2 {
                        for x in b.min.x - 2..=b.max.x + 2 {
                            let h = (BOTTOM..=(floor + 12).min(300))
                                .rev()
                                .find(|&y| {
                                    if building.contains(&IVec3::new(x, y, z)) {
                                        return false;
                                    }
                                    let b = block(x, y, z);
                                    matches!(
                                        b,
                                        Block::STONE
                                            | Block::DIRT
                                            | Block::GRASS
                                            | Block::SNOWY_GRASS
                                            | Block::SAND
                                            | Block::SANDSTONE
                                            | Block::GRAVEL
                                            | Block::RED_SAND
                                            | Block::SNOW
                                            | Block::DIRT_PATH
                                            | Block::FARMLAND
                                            | Block::WET_FARMLAND
                                            | Block::TERRACOTTA
                                    ) || b == ob::PODZOL
                                        || b == ob::COARSE_DIRT
                                        || b.terracotta_colour().is_some()
                                })
                                .unwrap_or(BOTTOM);
                            heights.insert((x, z), h);
                        }
                    }
                    for z in b.min.z - 1..=b.max.z + 1 {
                        for x in b.min.x - 1..=b.max.x + 1 {
                            let raw = g.column(x, z).height;
                            before = before.max((raw - floor).abs());
                            let h = heights[&(x, z)];
                            let outside = x < b.min.x || x > b.max.x || z < b.min.z || z > b.max.z;
                            if outside && (h - floor).abs() > ring_error {
                                ring_error = (h - floor).abs();
                                worst_ring = (x, z, h, floor);
                            } else if raw >= SEA_LEVEL - 1 {
                                // Count actual fallback material, stopping at soil.
                                let mut length = 0;
                                for y in (BOTTOM..b.min.y).rev() {
                                    let before = terrain(x, y, z);
                                    if block(x, y, z) != foundation || (before != Block::AIR && !before.is_fluid()) {
                                        break;
                                    }
                                    length += 1;
                                }
                                pillar = pillar.max(length);
                            }
                            for (nx, nz) in [(x + 1, z), (x, z + 1)] {
                                let other = heights[&(nx, nz)];
                                if (h - other).abs() > cut {
                                    cut = (h - other).abs();
                                    worst_step = (x, z, h, other);
                                }
                            }
                            checked += 1;
                        }
                    }
                }
                println!(
                    "seed {seed} {name}: {} pieces, {checked} columns, raw delta {before}, ring {ring_error}, foundation {pillar}, step {cut}, anchor {:?}",
                    ps.len(),
                    ps[0].origin
                );
                assert!(checked > 0);
                println!("worst ring (x,z,height,floor) {worst_ring:?}; step (x,z,height,neighbor) {worst_step:?}");
                if pillar > 2 || ring_error > 3 || cut > 3 {
                    failures.push(format!("seed {seed} {name}: pillar {pillar}, ring {ring_error}, step {cut}"));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
