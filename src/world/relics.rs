//! Generation 3 fossils and desert wells (GEN-05f). Original templates;
//! Java 26.3 placement/processor settings. See the dated verification record.
//!
//! Layouts sample pristine terrain once, never neighbouring generated chunks.
//! Each Java decoration chunk owns its rolls and every intersecting engine
//! chunk replays the same cells, in the same order, even after cache eviction.
use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, WORLD_MAX_Y, WORLD_MIN_Y, index};
use super::jigsaw::{self, Library, Palette, Placed};
use super::noise::hash3;
use super::structure::{Bounds, Rng};
use super::terrain::{Biome, Dimension, Generation, Generator};

const CACHE_LIMIT: usize = 512;
const FOSSIL_UPPER: u64 = 0x464F_5353_494C_0001;
const FOSSIL_LOWER: u64 = 0x464F_5353_494C_0002;
const WELL: u64 = 0x5745_4C4C_0000_0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    FossilUpper,
    FossilLower,
    Well,
}

impl Kind {
    fn settings(self) -> (u64, u32) {
        match self {
            Self::FossilUpper => (FOSSIL_UPPER, 64),
            Self::FossilLower => (FOSSIL_LOWER, 64),
            Self::Well => (WELL, 1000),
        }
    }
}

#[derive(Debug)]
pub struct Relic {
    pub kind: Kind,
    pub origin: IVec3,
    pub bounds: Bounds,
    pub cells: Vec<(IVec3, Block)>,
}

impl Relic {
    pub(super) fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let bounds = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        if !bounds.intersects(&self.bounds) {
            return;
        }
        for &(p, b) in &self.cells {
            if bounds.contains(p) {
                let l = p - base;
                let i = index(l.x as usize, l.y as usize, l.z as usize);
                if !super::geode::cannot_replace(blocks[i]) {
                    blocks[i] = b;
                }
            }
        }
    }
}

/// Resolve a whole original template before clipping it. Overlay cells
/// replace bone cells in a stable order, independently of chunk boundaries.
pub(super) fn bake(lib: &Library, pieces: &[Placed], palette: &dyn Palette) -> Vec<(IVec3, Block)> {
    let roles: Vec<_> = lib.roles.iter().map(|r| palette.role(r)).collect();
    let mut cells = FxHashMap::default();
    for p in pieces {
        let t = &lib.templates[p.template as usize];
        let mut put = |at, spec: &jigsaw::Spec| {
            let w = p.world(at);
            if let Some(b) = jigsaw::resolve(spec, &roles, palette, p, at).and_then(|b| palette.process(b, w, p)) {
                cells.insert(w, b);
            }
        };
        for &(a, b, spec) in &t.fills {
            for x in a.x..=b.x {
                for z in a.z..=b.z {
                    for y in a.y..=b.y {
                        put(IVec3::new(x, y, z), &spec);
                    }
                }
            }
        }
        for c in &t.cells {
            put(c.pos, &c.spec);
        }
    }
    let mut cells: Vec<_> = cells.into_iter().collect();
    cells.sort_by_key(|(p, _)| (p.x, p.z, p.y));
    cells
}

pub(super) struct RelicPalette {
    pub ore: Block,
    pub integrity: bool,
}

impl Palette for RelicPalette {
    fn role(&self, r: &str) -> Option<Block> {
        match r {
            "bone" => Some(super::nether_biome_blocks::BONE_BLOCK),
            "ore" => Some(self.ore),
            _ => None,
        }
    }

    fn process(&self, b: Block, at: IVec3, p: &Placed) -> Option<Block> {
        let ore = b == self.ore;
        let salt = if ore { 0x0AE } else { 0xB0AE };
        let mut rng = Rng(hash3(at.x, at.y, at.z, p.seed ^ salt));
        (!self.integrity || rng.unit() < if ore { 0.1 } else { 0.9 }).then_some(b)
    }

    fn clear_above(&self) -> i32 {
        0
    }
}

pub struct Relics {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Arc<Vec<Relic>>>>,
}

impl Relics {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    /// Rarity and in-square rolls of a Java 16-block decoration chunk.
    fn candidate(&self, c: IVec2, kind: Kind) -> Option<(IVec2, Rng)> {
        let (salt, rarity) = kind.settings();
        let mut rng = Rng(hash3(c.x, 0, c.y, self.seed ^ salt));
        if rng.unit() >= 1.0 / rarity as f64 {
            return None;
        }
        let at = c * 16 + IVec2::new(rng.below(16) as i32, rng.below(16) as i32);
        Some((at, rng))
    }

    fn fossil(&self, g: &Generator, c: IVec2, kind: Kind) -> Option<Relic> {
        let (at, mut rng) = self.candidate(c, kind)?;
        let initial_y = if kind == Kind::FossilUpper {
            rng.range(0, (WORLD_MAX_Y - 1) as u32) as i32
        } else {
            WORLD_MIN_Y + rng.below((-8 - WORLD_MIN_Y + 1) as u32) as i32
        };
        if !matches!(g.biome_at(IVec3::new(at.x, initial_y, at.y)), Biome::Desert | Biome::Swamp | Biome::MangroveSwamp)
        {
            return None;
        }
        let rotation = rng.below(4) as u8;
        let n = rng.below(8);
        let name = format!("fossil/{}_{}", if n < 4 { "spine" } else { "skull" }, 1 + n % 4);
        let lib = jigsaw::relics();
        let t = lib.template(&name).unwrap();
        let b = jigsaw::bounds_at(t, IVec3::ZERO, rotation);
        let size = b.max - b.min + IVec3::ONE;
        let low = at - IVec2::new(size.x / 2, size.z / 2);
        let mut lowest = initial_y;
        for x in 0..size.x {
            for z in 0..size.z {
                lowest = lowest.min(g.column(low.x + x, low.y + z).height + 1);
            }
        }
        let y = (lowest - 15 - rng.below(10) as i32).max(WORLD_MIN_Y + 10);
        let origin = IVec3::new(low.x, y, low.y) - b.min;
        let seed = rng.next_u64();
        let bone = jigsaw::single(lib, &name, origin, rotation, seed);
        let mut columns = FxHashMap::default();
        let mut nodes = FxHashMap::default();
        let mut empty = 0;
        for x in [bone.bounds.min.x, bone.bounds.max.x] {
            for y in [bone.bounds.min.y, bone.bounds.max.y] {
                for z in [bone.bounds.min.z, bone.bounds.max.z] {
                    let block = g.natural_block(IVec3::new(x, y, z), &mut columns, &mut nodes);
                    empty += (block == Block::AIR || block.is_fluid()) as usize;
                }
            }
        }
        if empty > 4 {
            return None;
        }
        let overlay = jigsaw::single(lib, &(name + "_coal"), origin, rotation, seed);
        let palette = RelicPalette {
            ore: if kind == Kind::FossilLower { Block::DEEPSLATE_DIAMOND_ORE } else { Block::COAL_ORE },
            integrity: true,
        };
        let mut cells = bake(lib, &[bone, overlay], &palette);
        // Java permits painting within the source chunk expanded by 16.
        let min = c * 16 - IVec2::splat(16);
        let max = c * 16 + IVec2::splat(31);
        cells.retain(|(p, _)| p.x >= min.x && p.x <= max.x && p.z >= min.y && p.z <= max.y);
        Some(Relic { kind, origin, bounds: bone.bounds, cells })
    }

    fn well(&self, g: &Generator, c: IVec2) -> Option<Relic> {
        let (at, mut rng) = self.candidate(c, Kind::Well)?;
        if g.surface_biome(at.x, at.y) != Biome::Desert || g.top_block_at_v2(at.x, at.y) != Block::SAND {
            return None;
        }
        let y = g.column(at.x, at.y).height;
        if y < super::terrain::SEA_LEVEL - 1 {
            return None;
        }
        let mut columns = FxHashMap::default();
        let mut nodes = FxHashMap::default();
        for x in -2..=2 {
            for z in -2..=2 {
                for dy in [-2, -1] {
                    if g.natural_block(IVec3::new(at.x + x, y + dy, at.y + z), &mut columns, &mut nodes) == Block::AIR {
                        return None;
                    }
                }
            }
        }
        let rotation = rng.below(4) as u8;
        let lib = jigsaw::relics();
        let t = lib.template("desert_well/well").unwrap();
        let b = jigsaw::bounds_at(t, IVec3::ZERO, rotation);
        let centre = (b.min + b.max) / 2;
        let origin = IVec3::new(at.x - centre.x, y - 2, at.y - centre.z);
        let p = jigsaw::single(lib, "desert_well/well", origin, rotation, rng.next_u64());
        let mut cells = bake(lib, &[p], &RelicPalette { ore: Block::COAL_ORE, integrity: false });
        // Java 26.3 overlays one archaeological cell on each of these
        // levels, independently choosing centre or a cardinal neighbour.
        // DEF-02 replaces suspicious sand with ordinary sand and no loot.
        let offsets = [IVec2::ZERO, IVec2::NEG_X, IVec2::Y, IVec2::X, IVec2::NEG_Y];
        for dy in [-1, -2] {
            let offset = offsets[rng.below(5) as usize];
            let sand = IVec3::new(at.x + offset.x, y + dy, at.y + offset.y);
            cells.retain(|(p, _)| *p != sand);
            cells.push((sand, Block::SAND));
        }
        Some(Relic { kind: Kind::Well, origin, bounds: p.bounds, cells })
    }

    pub fn source(&self, g: &Generator, c: IVec2) -> Arc<Vec<Relic>> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return Arc::new(Vec::new());
        }
        if let Some(v) = self.cache.lock().unwrap().get(&c).cloned() {
            return v;
        }
        let mut found = Vec::new();
        for kind in [Kind::FossilUpper, Kind::FossilLower] {
            if let Some(f) = self.fossil(g, c, kind) {
                found.push(f);
            }
        }
        if let Some(w) = self.well(g, c) {
            found.push(w);
        }
        let found = Arc::new(found);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        Arc::clone(cache.entry(c).or_insert(found))
    }

    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        if g.generation < Generation::V3 {
            return;
        }
        let min = (IVec2::new(base.x, base.z) - IVec2::splat(16)).div_euclid(IVec2::splat(16));
        let max = (IVec2::new(base.x, base.z) + IVec2::splat(CHUNK_SIZE_I + 15)).div_euclid(IVec2::splat(16));
        for z in min.y..=max.y {
            for x in min.x..=max.x {
                for r in self.source(g, IVec2::new(x, z)).iter() {
                    r.paint(blocks, base);
                }
            }
        }
    }

    /// Test/benchmark discovery; features are deliberately not `/locate` structures.
    pub fn find(&self, g: &Generator, kind: Kind, origin: IVec2, radius: i32) -> Option<IVec3> {
        let c = origin.div_euclid(IVec2::splat(16));
        for r in 0..=radius {
            for z in -r..=r {
                for x in -r..=r {
                    if x.abs().max(z.abs()) != r || self.candidate(c + IVec2::new(x, z), kind).is_none() {
                        continue;
                    }
                    if let Some(r) = self.source(g, c + IVec2::new(x, z)).iter().find(|r| r.kind == kind) {
                        return Some(r.origin);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fossil_integrity_and_overlay_rates_match_processors() {
        let lib = jigsaw::relics();
        let mut bones = 0;
        let mut ores = 0;
        let mut total = 0;
        for seed in 0..400 {
            for name in ["spine_1", "spine_2", "spine_3", "spine_4", "skull_1", "skull_2", "skull_3", "skull_4"] {
                let n = format!("fossil/{name}");
                let a = jigsaw::single(lib, &n, IVec3::ZERO, (seed % 4) as u8, seed);
                let b = jigsaw::single(lib, &(n + "_coal"), IVec3::ZERO, a.rotation, seed);
                let raw = bake(lib, &[a], &RelicPalette { ore: Block::COAL_ORE, integrity: false });
                let cells = bake(lib, &[a, b], &RelicPalette { ore: Block::COAL_ORE, integrity: true });
                total += raw.len();
                bones += cells.iter().filter(|(_, b)| *b == super::super::nether_biome_blocks::BONE_BLOCK).count();
                ores += cells.iter().filter(|(_, b)| *b == Block::COAL_ORE).count();
                let diamond = bake(lib, &[a, b], &RelicPalette { ore: Block::DEEPSLATE_DIAMOND_ORE, integrity: true });
                assert!(diamond.iter().all(|(_, b)| *b != Block::COAL_ORE));
            }
        }
        // 90% bone retention followed by independent 10% overlay leaves
        // 81% bone, 10% ore and 9% holes, not 90% bone + 10% ore.
        assert!((bones as f64 / total as f64 - 0.81).abs() < 0.008);
        assert!((ores as f64 / total as f64 - 0.10).abs() < 0.005);
    }

    fn all_cells(g: &Generator, r: &Relic, shift: IVec3) -> FxHashMap<IVec3, Block> {
        let min = (r.bounds.min - shift).div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let max = (r.bounds.max - shift).div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let mut out = FxHashMap::default();
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                for y in min.y..=max.y {
                    let base = IVec3::new(x, y, z) * CHUNK_SIZE_I + shift;
                    let mut blocks = Box::new([Block::STONE; CHUNK_VOLUME]);
                    g.relics.paint(g, &mut blocks, base);
                    for &(p, _) in &r.cells {
                        let l = p - base;
                        if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                            out.insert(p, blocks[index(l.x as usize, l.y as usize, l.z as usize)]);
                        }
                    }
                }
            }
        }
        out
    }

    #[test]
    fn fossils_and_wells_are_buried_or_on_desert_sand_and_clip_identically() {
        let g = Generator::new(12345);
        let desert = g.nearest_biome(IVec2::ZERO, Biome::Desert, 16000).unwrap();
        let centre = IVec2::new(desert.x, desert.z);
        for kind in [Kind::FossilUpper, Kind::FossilLower, Kind::Well] {
            let at = g.relics.find(&g, kind, centre, 160).expect("a relic in the desert");
            let c = if kind == Kind::Well { IVec2::new(at.x + 3, at.z + 3) } else { IVec2::new(at.x, at.z) }
                .div_euclid(IVec2::splat(16));
            let relics: Vec<_> = (-1..=1)
                .flat_map(|z| (-1..=1).map(move |x| c + IVec2::new(x, z)))
                .map(|c| g.relics.source(&g, c))
                .collect();
            let r = relics.iter().flat_map(|r| r.iter()).find(|r| r.origin == at && r.kind == kind).unwrap();
            eprintln!("{kind:?}: {} bounds {:?}", r.origin, r.bounds);
            if kind == Kind::Well {
                let at = (r.bounds.min + r.bounds.max) / 2;
                assert_eq!(g.surface_biome(at.x, at.z), Biome::Desert);
                assert_eq!(g.top_block_at_v2(at.x, at.z), Block::SAND);
                assert_eq!(r.origin.y + 2, g.column(at.x, at.z).height);
            } else {
                assert!(r.bounds.min.y >= WORLD_MIN_Y + 10);
                if kind == Kind::FossilLower {
                    assert!((-54..=-23).contains(&r.origin.y));
                }
                for &(p, _) in &r.cells {
                    assert!(p.y < g.column(p.x, p.z).height - 8);
                }
            }
            // A player edit to a generated relic is stored, while its
            // untouched neighbour still regenerates from generation 3.
            let cell = r.cells.iter().map(|(p, _)| *p).next().unwrap();
            let cp = cell.div_euclid(IVec3::splat(CHUNK_SIZE_I));
            let local = cell.rem_euclid(IVec3::splat(CHUNK_SIZE_I));
            let mut edited = g.generate(cp);
            edited.set(local.x as usize, local.y as usize, local.z as usize, Block::AIR);
            let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/relics/persistence")
                .join(format!("{}-{kind:?}", std::process::id()));
            let storage = super::super::storage::Storage::new(&dir);
            let mut props = std::collections::BTreeMap::new();
            props.insert("worldgen".into(), "3".into());
            storage
                .save(
                    &super::super::storage::LevelInfo { seed: g.seed, player: None, props },
                    &[(cp, Arc::new(edited))],
                )
                .unwrap();
            let level = storage.load_level().unwrap();
            assert_eq!(Generation::from_prop(level.props.get("worldgen").map(String::as_str)), Generation::V3);
            let reloaded = storage.load_chunks().unwrap();
            assert_eq!(reloaded[&cp].get(local.x as usize, local.y as usize, local.z as usize), Block::AIR);
            std::fs::remove_dir_all(dir).unwrap();
            let a = all_cells(&g, r, IVec3::ZERO);
            for shift in [IVec3::ONE, IVec3::new(17, 13, 29)] {
                assert_eq!(a, all_cells(&g, r, shift));
            }
            let g2 = Generator::new(12345);
            let b = all_cells(&g2, r, IVec3::ZERO);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn many_fossils_keep_their_depth_and_empty_corner_limit() {
        let g = Generator::new(12345);
        let p = g.nearest_biome(IVec2::ZERO, Biome::Desert, 16000).unwrap();
        let c = IVec2::new(p.x, p.z).div_euclid(IVec2::splat(16));
        let mut count = [0; 2];
        'samples: for radius in 0i32..=256 {
            for z in -radius..=radius {
                for x in -radius..=radius {
                    if x.abs().max(z.abs()) != radius {
                        continue;
                    }
                    for (i, kind) in [Kind::FossilUpper, Kind::FossilLower].into_iter().enumerate() {
                        if count[i] >= 24 {
                            continue;
                        }
                        let Some(r) = g.relics.fossil(&g, c + IVec2::new(x, z), kind) else { continue };
                        assert!(r.origin.y >= WORLD_MIN_Y + 10);
                        for (p, _) in &r.cells {
                            assert!(p.y <= g.column(p.x, p.z).height - 9);
                        }
                        if i == 1 {
                            assert!((-54..=-23).contains(&r.origin.y));
                        }
                        let mut columns = FxHashMap::default();
                        let mut nodes = FxHashMap::default();
                        let mut empty = 0;
                        for x in [r.bounds.min.x, r.bounds.max.x] {
                            for y in [r.bounds.min.y, r.bounds.max.y] {
                                for z in [r.bounds.min.z, r.bounds.max.z] {
                                    let b = g.natural_block(IVec3::new(x, y, z), &mut columns, &mut nodes);
                                    empty += (b == Block::AIR || b.is_fluid()) as usize;
                                }
                            }
                        }
                        assert!(empty <= 4);
                        count[i] += 1;
                    }
                    if count == [24, 24] {
                        break 'samples;
                    }
                }
            }
        }
        assert_eq!(count, [24, 24]);
    }

    #[test]
    fn rarity_is_per_java_chunk_and_protected_blocks_survive() {
        let f = Relics::new(123);
        for kind in [Kind::FossilUpper, Kind::FossilLower, Kind::Well] {
            let hits = (0..100_000).filter(|&x| f.candidate(IVec2::new(x, -10), kind).is_some()).count();
            let expected = 100_000.0 / kind.settings().1 as f64;
            assert!((hits as f64 - expected).abs() < 5.0 * expected.sqrt());
        }
        let r = Relic {
            kind: Kind::FossilUpper,
            origin: IVec3::ZERO,
            bounds: Bounds { min: IVec3::ZERO, max: IVec3::ONE },
            cells: vec![(IVec3::ZERO, Block::COAL_ORE)],
        };
        for protected in [Block::BEDROCK, Block::CHEST, Block::SPAWNER] {
            let mut blocks = Box::new([protected; CHUNK_VOLUME]);
            r.paint(&mut blocks, IVec3::ZERO);
            assert!(blocks.iter().all(|b| *b == protected));
        }
    }
}
