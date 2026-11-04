//! Generation 3 ruined portals. Java 26.3 placement/settings/processors with
//! thirteen original templates; layouts use deterministic terrain snapshots,
//! are cached once, and clip into chunks without reading loaded neighbours.
use super::biome::Biome;
use super::block::{Block, Facing};
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, chunk_of, index};
use super::fortress::Feature;
use super::jigsaw::{self, Canvas, Ground, Library, Palette, Placed};
use super::structure::Bounds;
use super::temples::LootTable;
use super::terrain::{Dimension, Generation, Generator, StructureColumn};
use crate::enchant::JavaRandom;
use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Land,
    Buried,
    Ocean,
    Mountain,
    Underground,
    Nether,
}
#[derive(Clone, Copy, Debug)]
struct Setup {
    placement: Placement,
    air: bool,
    moss: f32,
    cold: bool,
    vines: bool,
    overgrown: bool,
}

/// The Java structure settings mapped onto the available VoxelCraft biomes.
fn setup(b: Biome, nether: bool, rng: &mut JavaRandom) -> Setup {
    use Biome::*;
    let mut s = Setup { placement: Placement::Land, air: false, moss: 0.2, cold: true, vines: false, overgrown: false };
    if nether {
        s.placement = Placement::Nether;
        s.moss = 0.0;
        s.cold = false;
    } else if b == Desert {
        s.placement = Placement::Buried;
        s.moss = 0.0;
        s.cold = false;
    } else if b.is_jungle() {
        s.moss = 0.8;
        s.cold = false;
        s.vines = true;
        s.overgrown = true;
    } else if b.is_swamp() {
        s.placement = Placement::Ocean;
        s.moss = 0.5;
        s.cold = false;
        s.vines = true;
    } else if b.is_ocean() {
        s.placement = Placement::Ocean;
        s.moss = 0.8;
    } else {
        // Both standard and mountain sets have two equally weighted setups.
        s.air = rng.next_float() < 0.5;
        if s.air {
            s.placement = if (b.is_mountain() && b != Grove)
                || b.is_peak()
                || b.is_badlands()
                || matches!(
                    b,
                    WindsweptHills
                        | WindsweptGravellyHills
                        | WindsweptForest
                        | WindsweptSavanna
                        | SavannaPlateau
                        | StonyShore
                ) {
                Placement::Mountain
            } else {
                Placement::Underground
            };
        }
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Standard,
    Desert,
    Jungle,
    Swamp,
    Mountain,
    Ocean,
    Nether,
}
impl Variant {
    const ALL: [Self; 7] =
        [Self::Standard, Self::Desert, Self::Jungle, Self::Swamp, Self::Mountain, Self::Ocean, Self::Nether];
    fn representative(self) -> Biome {
        match self {
            Self::Standard | Self::Nether => Biome::Plains,
            Self::Desert => Biome::Desert,
            Self::Jungle => Biome::Jungle,
            Self::Swamp => Biome::Swamp,
            Self::Mountain => Biome::Meadow,
            Self::Ocean => Biome::Ocean,
        }
    }
    fn for_biome(b: Biome) -> Option<Self> {
        use Biome::*;
        if b == DeepDark {
            None
        } else if b == Desert {
            Some(Self::Desert)
        } else if b.is_jungle() {
            Some(Self::Jungle)
        } else if b.is_swamp() {
            Some(Self::Swamp)
        } else if b.is_ocean() {
            Some(Self::Ocean)
        } else if b.is_badlands()
            || (b.is_mountain() && b != Grove)
            || matches!(b, SavannaPlateau | WindsweptSavanna | StonyShore)
        {
            Some(Self::Mountain)
        } else {
            Some(Self::Standard)
        }
    }
}

/// Legacy WorldgenRandom.setLargeFeatureSeed, including Java signed nextLong.
fn next_long(r: &mut JavaRandom) -> i64 {
    ((r.next_int() as i64) << 32).wrapping_add(r.next_int() as i64)
}
fn piece_random(seed: u64, chunk: IVec2) -> JavaRandom {
    let mut r = JavaRandom::new(seed as i64);
    let (a, b) = (next_long(&mut r), next_long(&mut r));
    JavaRandom::new((chunk.x as i64).wrapping_mul(a) ^ (chunk.y as i64).wrapping_mul(b) ^ seed as i64)
}
fn between(r: &mut JavaRandom, lo: i32, hi: i32) -> i32 {
    if lo < hi { lo + r.next_bounded(hi - lo + 1) } else { hi }
}
fn double(r: &mut JavaRandom) -> f64 {
    // nextFloat cannot substitute for Java nextDouble (53 bits).
    let a = (r.next_int() as u32 >> 6) as u64;
    let b = (r.next_int() as u32 >> 5) as u64;
    ((a << 27) + b) as f64 / (1u64 << 53) as f64
}

/// FRONT_BACK mirrors x before rotation. Mirror the original library's
/// coordinates and directions once, then offset the placement back to Java's
/// signed x frame. This leaves the shared engine and other libraries unchanged.
fn library(mirror: bool) -> &'static Library {
    if !mirror {
        return jigsaw::ruined_portals();
    }
    static MIRROR: OnceLock<Library> = OnceLock::new();
    MIRROR.get_or_init(|| {
        let mut l = Library::parse(include_str!("jigsaw/ruined_portals.txt")).expect("portal templates");
        for t in &mut l.templates {
            let x = t.size.x - 1;
            let turn = |s: &mut jigsaw::Spec| {
                s.facing = s.facing.map(|f| match f {
                    Facing::East => Facing::West,
                    Facing::West => Facing::East,
                    f => f,
                });
            };
            for c in &mut t.cells {
                c.pos.x = x - c.pos.x;
                turn(&mut c.spec);
            }
            for (a, b, s) in &mut t.fills {
                let (lo, hi) = (x - b.x, x - a.x);
                a.x = lo;
                b.x = hi;
                turn(s);
            }
        }
        l
    })
}

pub struct Portal {
    pub bounds: Bounds,
    pub placement: Placement,
    pub giant: bool,
    pub origin: IVec3,
    blocks: Vec<(IVec3, Block)>,
    features: Vec<(IVec3, Feature)>,
}
type Cache = FxHashMap<IVec2, Option<Arc<Portal>>>;
pub struct RuinedPortals {
    seed: u64,
    cache: Mutex<Cache>,
}
impl RuinedPortals {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }
    pub fn start(&self, region: IVec2) -> IVec2 {
        let seed = self
            .seed
            .wrapping_add((region.x as i64).wrapping_mul(341_873_128_712) as u64)
            .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541) as u64)
            .wrapping_add(34_222_645);
        let mut r = JavaRandom::new(seed as i64);
        region * 40 + IVec2::new(r.next_bounded(25), r.next_bounded(25))
    }
    fn enabled(g: &Generator) -> bool {
        g.generation >= Generation::V3 && matches!(g.dimension, Dimension::Overworld | Dimension::Nether)
    }
    pub fn get(&self, g: &Generator, region: IVec2) -> Option<Arc<Portal>> {
        if !Self::enabled(g) {
            return None;
        }
        if let Some(p) = self.cache.lock().unwrap().get(&region) {
            return p.clone();
        }
        let p = self.build(g, self.start(region)).map(Arc::new);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 256
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        cache.entry(region).or_insert(p).clone()
    }
    pub fn nearest(&self, g: &Generator, from: IVec3, max_rings: i32) -> Option<IVec3> {
        if !Self::enabled(g) {
            return None;
        }
        let home = IVec2::new(from.x, from.z).div_euclid(IVec2::splat(640));
        // Java's first valid start on a ring, as in Temples::locate_rings.
        for ring in 0..=max_rings {
            for x in -ring..=ring {
                for z in -ring..=ring {
                    if x.abs() != ring && z.abs() != ring {
                        continue;
                    }
                    if let Some(p) = self.get(g, home + IVec2::new(x, z)) {
                        return Some(p.origin);
                    }
                }
            }
        }
        None
    }
    fn around(&self, g: &Generator, base: IVec3) -> Vec<Arc<Portal>> {
        if !Self::enabled(g) {
            return Vec::new();
        }
        let min = IVec2::new(base.x - 48, base.z - 48).div_euclid(IVec2::splat(640));
        let max = IVec2::new(base.x + CHUNK_SIZE_I + 48, base.z + CHUNK_SIZE_I + 48).div_euclid(IVec2::splat(640));
        let mut out = Vec::new();
        for x in min.x..=max.x {
            for z in min.y..=max.y {
                let region = IVec2::new(x, z);
                let start = self.start(region) * 16;
                // Avoid building portals for unrelated chunks in the same region.
                if start.x < base.x - 48
                    || start.x > base.x + CHUNK_SIZE_I + 48
                    || start.y < base.z - 48
                    || start.y > base.z + CHUNK_SIZE_I + 48
                {
                    continue;
                }
                if let Some(p) = self.get(g, region)
                    && p.bounds.min.y < base.y + CHUNK_SIZE_I
                    && p.bounds.max.y >= base.y
                {
                    out.push(p);
                }
            }
        }
        out
    }
    /// Keep uniform Nether chunks uniform when they do not meet a portal.
    pub(super) fn paint_generated(
        &self,
        g: &Generator,
        chunk: super::chunk::ChunkData,
        cpos: IVec3,
    ) -> super::chunk::ChunkData {
        let base = cpos * CHUNK_SIZE_I;
        let portals = self.around(g, base);
        if portals.is_empty() {
            return chunk;
        }
        let mut blocks = super::chunk::ChunkData::new_dense(Block::AIR);
        let mut i = 0;
        chunk.for_each_block(|b| {
            blocks[i] = b;
            i += 1;
        });
        for p in portals {
            Self::paint_portal(&p, &mut blocks, base);
        }
        super::chunk::ChunkData::from_dense(blocks)
    }
    fn paint_portal(p: &Portal, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        for &(at, b) in &p.blocks {
            let l = at - base;
            if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                let i = index(l.x as usize, l.y as usize, l.z as usize);
                if !protected(blocks[i]) {
                    blocks[i] = b;
                }
            }
        }
    }
    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        for p in self.around(g, base) {
            Self::paint_portal(&p, blocks, base);
        }
    }
    pub fn features(&self, g: &Generator, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        self.around(g, cpos * CHUNK_SIZE_I)
            .iter()
            .flat_map(|p| p.features.iter().copied())
            .filter(|(p, _)| chunk_of(*p) == cpos)
            .collect()
    }
    fn build(&self, g: &Generator, chunk: IVec2) -> Option<Portal> {
        // ChunkGenerator.createStructures: remove a failed weighted option
        // without replacement. All seven portal variants have weight one.
        let mut options = Variant::ALL.to_vec();
        let mut selection = piece_random(self.seed, chunk);
        while !options.is_empty() {
            let i = selection.next_bounded(options.len() as i32) as usize;
            let variant = options.remove(i);
            if let Some(portal) = self.build_variant(g, chunk, variant) {
                return Some(portal);
            }
        }
        None
    }
    fn build_variant(&self, g: &Generator, chunk: IVec2, variant: Variant) -> Option<Portal> {
        let nether = g.dimension == Dimension::Nether;
        if (variant == Variant::Nether) != nether {
            return None;
        }
        let mut r = piece_random(self.seed, chunk);
        let start = chunk * 16;
        let biome = variant.representative();
        let mut s = setup(biome, nether, &mut r);
        let giant = r.next_float() < 0.05;
        let number = r.next_bounded(if giant { 3 } else { 10 }) + 1;
        let name = format!("ruined_portal/{}portal_{number}", if giant { "giant_" } else { "" });
        let rotation = r.next_bounded(4) as u8;
        let mirror = r.next_float() >= 0.5;
        let lib = library(mirror);
        let t = lib.templates.iter().find(|t| t.name == name).unwrap();
        let pivot = IVec3::new(t.size.x / 2, 0, t.size.z / 2);
        let corner = IVec3::new(start.x, 0, start.y) + pivot
            - jigsaw::rotate(pivot + IVec3::X * if mirror { t.size.x - 1 } else { 0 }, rotation);
        let mut piece = jigsaw::single(lib, &name, corner, rotation, self.seed);
        let centre = piece.bounds.min + (piece.bounds.max - piece.bounds.min + IVec3::ONE) / 2;
        let center_col = g.structure_column(centre.x, centre.z);
        let surface = center_col.top(s.placement == Placement::Ocean);
        let min_y = if nether { 15 } else { g.dimension.min_y() + 15 };
        let height = t.size.y;
        let mut y = match s.placement {
            Placement::Nether => {
                if s.air {
                    between(&mut r, 32, 100)
                } else if r.next_float() < 0.5 {
                    between(&mut r, 27, 29)
                } else {
                    between(&mut r, 29, 100)
                }
            }
            Placement::Mountain => between(&mut r, 70, surface - height),
            Placement::Underground => between(&mut r, min_y, surface - height),
            Placement::Buried => surface - height + between(&mut r, 2, 8),
            _ => surface,
        };
        let corners = [
            (piece.bounds.min.x, piece.bounds.min.z),
            (piece.bounds.max.x, piece.bounds.min.z),
            (piece.bounds.min.x, piece.bounds.max.z),
            (piece.bounds.max.x, piece.bounds.max.z),
        ]
        .map(|(x, z)| g.structure_column(x, z));
        while y > min_y {
            if corners
                .iter()
                .filter(|c| {
                    let b = c.get(y);
                    b != Block::AIR && (s.placement != Placement::Ocean || b.is_solid())
                })
                .count()
                >= 3
            {
                break;
            }
            y -= 1;
        }
        piece.origin.y = y;
        piece.bounds.min.y += y;
        piece.bounds.max.y += y;
        let cold_biome = if nether { biome } else { g.biome_at(IVec3::new(start.x, y, start.y)) };
        // Structure.generate validates the biome at GenerationStub.position,
        // after findSuitableY; in particular deep-dark underground starts fail.
        if !nether && Variant::for_biome(cold_biome) != Some(variant) {
            return None;
        }
        s.cold = s.cold && (cold_biome.is_cold() || cold_biome.temperature() - (y - 80).max(0) as f32 / 800.0 < 0.15);
        // A local base-column canvas also makes submerged lava processing and
        // spread/drips independent of the chunk being painted.
        let centre = piece.bounds.min + (piece.bounds.max - piece.bounds.min + IVec3::ONE) / 2;
        let lo = piece.bounds.min.min(centre - IVec3::new(14, 0, 14)) - IVec3::new(1, 9, 1);
        let hi = piece.bounds.max.max(centre + IVec3::new(14, 0, 14)) + IVec3::ONE;
        let mut columns = FxHashMap::default();
        // Placement above used Java's base columns. Piece processors operate
        // on the level instead: retain actual cave air, ores, decorations and
        // older structures. These local snapshots exclude portals/treasure,
        // so rebuilding a layout cannot recurse or depend on generation order.
        let mut terrain = FxHashMap::default();
        for x in lo.x..=hi.x {
            for z in lo.z..=hi.z {
                let min_y = g.dimension.min_y();
                let blocks = (min_y..g.dimension.max_y())
                    .map(|y| {
                        let at = IVec3::new(x, y, z);
                        let c = chunk_of(at);
                        let data = terrain.entry(c).or_insert_with(|| g.portal_terrain(c));
                        let l = at - c * CHUNK_SIZE_I;
                        data.get(l.x as usize, l.y as usize, l.z as usize)
                    })
                    .collect();
                columns.insert(IVec2::new(x, z), StructureColumn { min_y, blocks });
            }
        }
        let palette = PortalPalette { setup: s, nether, columns: &columns };
        let surface = |x, z| columns[&IVec2::new(x, z)].top(false);
        let solid = |x, z| columns[&IVec2::new(x, z)].top(true);
        let ground = Ground { surface: &surface, solid: &solid };
        let mut changed = FxHashMap::default();
        let untouched = Block(u16::MAX);
        for x in piece.bounds.min.x..=piece.bounds.max.x {
            for z in piece.bounds.min.z..=piece.bounds.max.z {
                let mut blocks = vec![untouched; t.size.y as usize];
                jigsaw::paint_canvas(
                    lib,
                    &[piece],
                    &palette,
                    &ground,
                    &mut Canvas {
                        blocks: &mut blocks,
                        base: IVec3::new(x, piece.bounds.min.y, z),
                        top: IVec3::new(x, piece.bounds.max.y, z),
                    },
                );
                for (dy, b) in blocks.into_iter().enumerate() {
                    if b != untouched {
                        changed.insert(IVec3::new(x, piece.bounds.min.y + dy as i32, z), b);
                    }
                }
            }
        }
        let get = |p: IVec3, map: &FxHashMap<IVec3, Block>| {
            map.get(&p)
                .copied()
                .unwrap_or_else(|| columns.get(&IVec2::new(p.x, p.z)).map_or(Block::AIR, |c| c.get(p.y)))
        };
        let can_replace =
            |b: Block| b != Block::AIR && b != Block::OBSIDIAN && !protected(b) && (nether || !b.is_lava());
        let rack = |r: &mut JavaRandom| if !s.cold && r.next_float() < 0.07 { Block::MAGMA } else { Block::NETHERRACK };
        let drip = |at: IVec3, map: &mut FxHashMap<IVec3, Block>, r: &mut JavaRandom| {
            let mut p = at;
            for remaining in (0..=8).rev() {
                if can_replace(get(p, map)) {
                    map.insert(p, rack(r));
                }
                if remaining == 0 || r.next_float() >= 0.5 {
                    break;
                }
                p.y -= 1;
            }
        };
        let probabilities = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.9, 0.9, 0.8, 0.7, 0.6, 0.4, 0.2];
        let width = (t.size.x + t.size.z) / 2;
        let adjustment = r.next_bounded((8 - width / 2).max(1));
        for x in centre.x - 14..=centre.x + 14 {
            for z in centre.z - 14..=centre.z + 14 {
                let distance = (x - centre.x).abs() + (z - centre.z).abs() + adjustment;
                if distance >= 14 || double(&mut r) >= probabilities[distance as usize] {
                    continue;
                }
                let col = &columns[&IVec2::new(x, z)];
                // Java reads the heightmap after template painting.
                let top = (col.min_y..col.min_y + col.blocks.len() as i32)
                    .rev()
                    .find(|&y| {
                        let b = get(IVec3::new(x, y, z), &changed);
                        b != Block::AIR && (s.placement != Placement::Ocean || b.is_solid())
                    })
                    .unwrap_or(col.min_y - 1);
                let y = if matches!(s.placement, Placement::Land | Placement::Ocean) {
                    top
                } else {
                    top.min(piece.bounds.min.y)
                };
                let p = IVec3::new(x, y, z);
                if (y - piece.bounds.min.y).abs() <= 3 && can_replace(get(p, &changed)) {
                    changed.insert(p, rack(&mut r));
                    if s.overgrown
                        && r.next_float() < 0.5
                        && get(p, &changed) == Block::NETHERRACK
                        && get(p + IVec3::Y, &changed) == Block::AIR
                    {
                        changed.insert(p + IVec3::Y, Block::JUNGLE_LEAVES);
                    }
                    drip(p - IVec3::Y, &mut changed, &mut r);
                }
            }
        }
        for x in piece.bounds.min.x + 1..piece.bounds.max.x {
            for z in piece.bounds.min.z + 1..piece.bounds.max.z {
                let p = IVec3::new(x, piece.bounds.min.y, z);
                if get(p, &changed) == Block::NETHERRACK {
                    drip(p - IVec3::Y, &mut changed, &mut r);
                }
            }
        }
        if s.vines || s.overgrown {
            // BlockPos.betweenClosedStream: x changes fastest, then y, then z.
            for z in piece.bounds.min.z..=piece.bounds.max.z {
                for y in piece.bounds.min.y..=piece.bounds.max.y {
                    for x in piece.bounds.min.x..=piece.bounds.max.x {
                        let p = IVec3::new(x, y, z);
                        let b = get(p, &changed);
                        if s.vines && b != Block::AIR && super::overworld_blocks::vine_wall(b).is_none() {
                            let f =
                                [Facing::North, Facing::East, Facing::South, Facing::West][r.next_bounded(4) as usize];
                            let n = p + f.offset();
                            if full_collision_face(b, f) && get(n, &changed) == Block::AIR {
                                changed.insert(n, super::overworld_blocks::vine(f.opposite()));
                            }
                        }
                        if s.overgrown
                            && r.next_float() < 0.5
                            && get(p, &changed) == Block::NETHERRACK
                            && get(p + IVec3::Y, &changed) == Block::AIR
                        {
                            changed.insert(p + IVec3::Y, Block::JUNGLE_LEAVES);
                        }
                    }
                }
            }
        }
        let features = jigsaw::markers(lib, &[piece], &surface)
            .into_iter()
            .filter_map(|m| match m {
                jigsaw::Marker::Loot(p, "ruined_portal", seed) if super::chest::is_chest(get(p, &changed)) => {
                    Some((p, Feature::TempleChest(seed, LootTable::RuinedPortal)))
                }
                _ => None,
            })
            .collect();
        let mut blocks: Vec<_> = changed.into_iter().collect();
        blocks.sort_unstable_by_key(|(p, _)| (p.y, p.z, p.x));
        let bounds = Bounds {
            min: blocks.iter().fold(piece.bounds.min, |a, (p, _)| a.min(*p)),
            max: blocks.iter().fold(piece.bounds.max, |a, (p, _)| a.max(*p)),
        };
        Some(Portal {
            bounds,
            placement: s.placement,
            giant,
            origin: IVec3::new(start.x, y, start.y),
            blocks,
            features,
        })
    }
}

/// Block.isFaceFull on the collision shape (unlike the support shape,
/// leaves can carry a vine). Union coverage handles the two stair boxes.
fn full_collision_face(b: Block, f: Facing) -> bool {
    let d = f.offset();
    let axis = if d.x != 0 { 0 } else { 2 };
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut rows = [0u16; 16];
    for bx in super::shape::collision(b, |_| Block::AIR, Block::AIR).as_slice() {
        if if d[axis] > 0 { bx.max[axis] >= 16 } else { bx.min[axis] == 0 } {
            for y in bx.min[v]..bx.max[v].min(16) {
                for x in bx.min[u]..bx.max[u].min(16) {
                    rows[y as usize] |= 1 << x;
                }
            }
        }
    }
    rows.iter().all(|&row| row == u16::MAX)
}

/// The represented members of Java's features_cannot_replace tag.
fn protected(b: Block) -> bool {
    b == Block::BEDROCK || b == Block::SPAWNER || b.base() == Block::CHEST || b.base() == Block::END_PORTAL_FRAME
}

struct PortalPalette<'a> {
    setup: Setup,
    nether: bool,
    columns: &'a FxHashMap<IVec2, StructureColumn>,
}
/// Mth.getSeed, as StructurePlaceSettings.getRandom(pos) uses it.
fn position_random(p: IVec3) -> JavaRandom {
    let n = (p.x.wrapping_mul(3_129_871) as i64) ^ (p.z as i64).wrapping_mul(116_129_781) ^ p.y as i64;
    JavaRandom::new(n.wrapping_mul(n).wrapping_mul(42_317_861).wrapping_add(n.wrapping_mul(11)) >> 16)
}
impl Palette for PortalPalette<'_> {
    fn role(&self, _: &str) -> Option<Block> {
        None
    }
    fn clear_above(&self) -> i32 {
        0
    }
    fn process(&self, mut b: Block, p: IVec3, _: &Placed) -> Option<Block> {
        if b == Block::AIR && !self.setup.air {
            return None;
        }
        let mut r = position_random(p);
        if b == Block::GOLD_BLOCK && r.next_float() < 0.3 {
            b = Block::AIR;
        } else if b == Block::LAVA {
            b = if self.setup.placement == Placement::Ocean {
                Block::MAGMA
            } else if self.setup.cold {
                Block::NETHERRACK
            } else if r.next_float() < 0.2 {
                Block::MAGMA
            } else {
                b
            };
        } else if b == Block::NETHERRACK && !self.setup.cold && r.next_float() < 0.07 {
            b = Block::MAGMA;
        }
        b = age(b, p, self.setup.moss);
        let original = self.columns[&IVec2::new(p.x, p.z)].get(p.y);
        if protected(original) {
            return None;
        }
        if original.is_lava() {
            let full = b != Block::AIR
                && b.base() != Block::CHEST
                && !b.is_fluid()
                && super::shape::outline(b, |_| Block::AIR, Block::AIR)
                    .as_slice()
                    .iter()
                    .any(|b| b.min == [0; 3] && b.max == [16; 3]);
            if !full {
                b = Block::LAVA;
            }
        }
        if self.nether {
            b = blackstone(b);
        }
        // StructureTemplate's APPLY_WATERLOGGING pass happens after all
        // piece processors, using the fluid that occupied the cell.
        if original.is_water() {
            b = b.with_water(true);
        }
        Some(b)
    }
}
fn family(base: Block) -> u8 {
    super::slab_stair_states::FAMILIES.iter().position(|f| f.base == base && f.stairs != Block::AIR).unwrap() as u8
}
fn age(mut b: Block, p: IVec3, moss: f32) -> Block {
    use super::slab_stair_states as ss;
    let mut r = position_random(p);
    let mossy = family(Block::MOSSY_STONE_BRICKS);
    if matches!(b, Block::STONE | Block::STONE_BRICKS | super::stone_blocks::CHISELED_STONE_BRICKS) {
        if r.next_float() < 0.5 {
            let stairs = |r: &mut JavaRandom, f| {
                ss::stairs_id(ss::Stairs {
                    family: f,
                    facing: [Facing::North, Facing::East, Facing::South, Facing::West][r.next_bounded(4) as usize]
                        .opposite(),
                    top: r.next_bounded(2) == 0,
                    shape: ss::StairShape::Straight,
                    wet: false,
                })
            };
            let non = [Block::CRACKED_STONE_BRICKS, stairs(&mut r, family(Block::STONE_BRICKS))];
            let yes = [Block::MOSSY_STONE_BRICKS, stairs(&mut r, mossy)];
            b = if r.next_float() < moss { yes } else { non }[r.next_bounded(2) as usize];
        }
    } else if let Some(s) = ss::stairs(b) {
        if r.next_float() < 0.5 {
            let non = [Block::STONE_SLAB, Block::STONE_BRICK_SLAB];
            let yes = [ss::stairs_id(ss::Stairs { family: mossy, ..s }), ss::FAMILIES[mossy as usize].slab];
            b = if r.next_float() < moss { yes } else { non }[r.next_bounded(2) as usize];
        }
    } else if let Some(s) = ss::slab(b) {
        if r.next_float() < moss {
            b = ss::slab_id(ss::Slab { family: mossy, ..s });
        }
    } else if b.wall_base().is_some() {
        if r.next_float() < moss {
            b = super::wall_states::with_up(super::stone_blocks::wall(7), super::wall_states::up(b).unwrap_or(true))
                .with_water(b.is_waterlogged());
        }
    } else if b == Block::OBSIDIAN && r.next_float() < 0.15 {
        b = Block::CRYING_OBSIDIAN;
    }
    b
}
fn blackstone(b: Block) -> Block {
    use super::slab_stair_states as ss;
    let replace = |b| match b {
        Block::COBBLESTONE | Block::MOSSY_COBBLESTONE => Block::BLACKSTONE,
        Block::STONE => Block::POLISHED_BLACKSTONE,
        Block::IRON_BARS => Block::CHAIN,
        Block::STONE_BRICKS | Block::MOSSY_STONE_BRICKS => Block::POLISHED_BLACKSTONE_BRICKS,
        super::stone_blocks::CHISELED_STONE_BRICKS => Block::CHISELED_POLISHED_BLACKSTONE,
        Block::CRACKED_STONE_BRICKS => Block::CRACKED_POLISHED_BLACKSTONE_BRICKS,
        b => b,
    };
    if let Some(s) = ss::stairs(b) {
        let old = ss::FAMILIES[s.family as usize].base;
        let new = replace(old);
        if new != old {
            return ss::stairs_id(ss::Stairs { family: family(new), shape: ss::StairShape::Straight, wet: false, ..s });
        }
    } else if let Some(s) = ss::slab(b) {
        let old = ss::FAMILIES[s.family as usize].base;
        let new = if old == Block::SMOOTH_STONE { Block::POLISHED_BLACKSTONE } else { replace(old) };
        if new != old {
            return ss::slab_id(ss::Slab { family: family(new), wet: false, ..s });
        }
    } else if let Some(old) = b.wall_base() {
        return match replace(old) {
            Block::BLACKSTONE => super::nether_blocks::shape_id(0, 5),
            Block::POLISHED_BLACKSTONE_BRICKS => super::nether_blocks::shape_id(2, 5),
            _ => b,
        };
    }
    replace(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_seeded_starts_match_reference_coordinates() {
        // Independently evaluated with the 48-bit LCG and Java rejection loop;
        // includes signed regions (not truncating division).
        for (seed, region, want) in [
            (0, (0, 0), (20, 9)),
            (0, (-3, 2), (-97, 82)),
            (0, (5, -7), (222, -270)),
            (1, (0, 0), (19, 18)),
            (1, (-3, 2), (-119, 99)),
            (1, (5, -7), (222, -280)),
            (42, (0, 0), (16, 22)),
            (42, (-3, 2), (-115, 95)),
            (42, (5, -7), (204, -280)),
            (12345, (0, 0), (1, 0)),
            (12345, (-3, 2), (-104, 88)),
            (12345, (5, -7), (212, -280)),
        ] {
            assert_eq!(RuinedPortals::new(seed).start(IVec2::from(region)), IVec2::from(want));
        }
    }
    #[test]
    fn thirteen_original_templates_and_mirrors() {
        assert_eq!(library(false).templates.len(), 13);
        for (a, b) in library(false).templates.iter().zip(&library(true).templates) {
            assert_eq!(a.size, b.size);
            assert!(a.cells.iter().any(|c| matches!(c.spec.base, jigsaw::Base::Loot(_))));
            for (c, m) in a.cells.iter().zip(&b.cells) {
                assert_eq!(m.pos, IVec3::new(a.size.x - 1 - c.pos.x, c.pos.y, c.pos.z));
            }
        }
    }
    #[test]
    fn setup_weights_and_giant_probability_follow_java() {
        let mut giants = 0;
        let mut underground = 0;
        let mut choices = [0; 13];
        for seed in 0..20000 {
            let mut r = piece_random(seed, IVec2::new(-18, 31));
            let s = setup(Biome::Plains, false, &mut r);
            underground += usize::from(s.air);
            let giant = r.next_float() < 0.05;
            giants += usize::from(giant);
            let n = r.next_bounded(if giant { 3 } else { 10 }) as usize;
            choices[if giant { 10 + n } else { n }] += 1;
        }
        assert!((920..1080).contains(&giants), "{giants}");
        assert!((9700..10300).contains(&underground), "{underground}");
        assert!(choices.iter().all(|&n| n > 0));
    }
    #[test]
    fn processors_preserve_cold_air_lava_and_age_materials() {
        let at = IVec3::ZERO;
        let piece = jigsaw::single(library(false), "ruined_portal/portal_1", at, 0, 1);
        let columns = FxHashMap::from_iter([(IVec2::ZERO, StructureColumn { min_y: 0, blocks: vec![Block::AIR; 1] })]);
        let s = setup(Biome::FrozenOcean, false, &mut JavaRandom::new(0));
        let palette = PortalPalette { setup: Setup { cold: true, ..s }, nether: false, columns: &columns };
        assert_eq!(palette.process(Block::AIR, at, &piece), None);
        assert_eq!(palette.process(Block::LAVA, at, &piece), Some(Block::MAGMA));
        assert_eq!(palette.process(Block::NETHERRACK, at, &piece), Some(Block::NETHERRACK));
        let cold = PortalPalette { setup: Setup { placement: Placement::Land, ..palette.setup }, ..palette };
        assert_eq!(cold.process(Block::LAVA, at, &piece), Some(Block::NETHERRACK));
        let air = PortalPalette { setup: Setup { air: true, ..cold.setup }, ..cold };
        assert_eq!(air.process(Block::AIR, at, &piece), Some(Block::AIR));
        let mut crying = 0;
        let mut aged = 0;
        for x in 0..10000 {
            let p = IVec3::new(x, 70, -37);
            crying += usize::from(age(Block::OBSIDIAN, p, 0.2) == Block::CRYING_OBSIDIAN);
            aged += usize::from(age(Block::STONE_BRICKS, p, 0.2) != Block::STONE_BRICKS);
        }
        assert!((1350..1650).contains(&crying), "{crying}");
        assert!((4800..5200).contains(&aged), "{aged}");
        assert_eq!(blackstone(Block::STONE_BRICKS), Block::POLISHED_BLACKSTONE_BRICKS);
        assert_eq!(blackstone(Block::CRACKED_STONE_BRICKS), Block::CRACKED_POLISHED_BLACKSTONE_BRICKS);
        let wall = super::super::wall_states::with_up(Block::COBBLESTONE_WALL, false).with_water(true);
        let aged = age(wall, at, 1.0);
        assert_eq!(aged.wall_base(), Some(Block::MOSSY_STONE_BRICKS));
        assert_eq!(super::super::wall_states::up(aged), Some(false));
        assert!(aged.is_waterlogged());
    }
    #[test]
    fn gold_magma_lava_rates_and_submerged_shapes_follow_java_rules() {
        let columns = (0..10000)
            .map(|x| {
                (IVec2::new(x, 0), StructureColumn { min_y: 0, blocks: vec![Block::AIR, Block::LAVA, Block::WATER] })
            })
            .collect();
        let warm = PortalPalette {
            setup: setup(Biome::Jungle, false, &mut JavaRandom::new(0)),
            nether: false,
            columns: &columns,
        };
        let piece = jigsaw::single(library(false), "ruined_portal/portal_1", IVec3::ZERO, 0, 0);
        let (mut gold, mut magma, mut lava) = (0, 0, 0);
        for x in 0..10000 {
            let p = IVec3::new(x, 0, 0);
            gold += usize::from(warm.process(Block::GOLD_BLOCK, p, &piece) == Some(Block::AIR));
            magma += usize::from(warm.process(Block::NETHERRACK, p, &piece) == Some(Block::MAGMA));
            lava += usize::from(warm.process(Block::LAVA, p, &piece) == Some(Block::MAGMA));
        }
        assert!((2800..3200).contains(&gold), "{gold}");
        assert!((600..800).contains(&magma), "{magma}");
        assert!((1800..2200).contains(&lava), "{lava}");
        let p = IVec3::Y;
        assert_eq!(warm.process(Block::CHEST, p, &piece), Some(Block::LAVA));
        assert_eq!(warm.process(Block::STONE_BRICK_SLAB, p, &piece), Some(Block::LAVA));
        assert_eq!(warm.process(Block::GOLD_BLOCK, p, &piece), Some(Block::GOLD_BLOCK));
        let wet = warm.process(Block::STONE_BRICK_SLAB, IVec3::Y * 2, &piece).unwrap();
        assert!(wet.is_waterlogged(), "underwater template slabs retain the water source");
        assert!(protected(Block::CHEST));
        assert!(protected(Block::SPAWNER));
        assert!(protected(Block::END_PORTAL_FRAME));
    }

    #[test]
    fn every_vertical_placement_and_nether_paint_across_borders() {
        let g = Generator::new(12345);
        let mut found = Vec::new();
        for x in -12..=12 {
            for z in -12..=12 {
                let region = IVec2::new(x, z);
                let start = g.ruined_portals.start(region);
                let mut r = piece_random(g.seed, start);
                let b = g.column(start.x * 16 + 8, start.y * 16 + 8).biome;
                let s = setup(b, false, &mut r);
                if found.iter().any(|(placement, _)| *placement == s.placement) {
                    continue;
                }
                let Some(portal) = g.ruined_portals.get(&g, region) else {
                    continue;
                };
                if found.iter().any(|(placement, _)| *placement == portal.placement) {
                    continue;
                }
                assert!(!portal.blocks.is_empty());
                found.push((portal.placement, region));
            }
        }
        assert_eq!(found.len(), 5, "{found:?}");
        for dim in [Dimension::Overworld, Dimension::Nether] {
            let g = Generator::for_dimension(12345, dim);
            let regions =
                if dim == Dimension::Nether { vec![IVec2::ZERO] } else { found.iter().map(|(_, r)| *r).collect() };
            for region in regions {
                let p = g.ruined_portals.get(&g, region).unwrap();
                if dim == Dimension::Nether {
                    assert_eq!(p.placement, Placement::Nether);
                }
                let mut chunks = FxHashMap::default();
                for &(at, b) in &p.blocks {
                    let c = chunk_of(at);
                    let blocks = chunks.entry(c).or_insert_with(|| {
                        let mut blocks = Box::new([Block::STONE; CHUNK_VOLUME]);
                        g.ruined_portals.paint(&g, &mut blocks, c * CHUNK_SIZE_I);
                        blocks
                    });
                    let l = at - c * CHUNK_SIZE_I;
                    assert_eq!(blocks[index(l.x as usize, l.y as usize, l.z as usize)], b);
                }
                g.ruined_portals.cache.lock().unwrap().clear();
                let q = g.ruined_portals.get(&g, region).unwrap();
                assert_eq!(p.blocks, q.blocks);
                assert_eq!(p.features, q.features);
                eprintln!(
                    "{dim:?} {:?} portal at {} (giant {}), {} blocks, {} chests",
                    p.placement,
                    p.origin,
                    p.giant,
                    p.blocks.len(),
                    p.features.len()
                );
            }
        }
    }
    #[test]
    fn both_structure_chests_register_and_looted_saves_stay_empty() {
        for dim in [Dimension::Overworld, Dimension::Nether] {
            let g = Arc::new(Generator::for_dimension(42, dim));
            let mut features = Vec::new();
            for x in -2..=2 {
                if let Some(p) = g.ruined_portals.get(&g, IVec2::new(x, 0)) {
                    features.extend(p.features.iter().copied());
                }
            }
            if dim == Dimension::Overworld {
                let p = g.buried_treasure.nearest_buried_treasure(&g, IVec3::ZERO, 400).expect("treasure");
                let c = IVec2::new(p.x.div_euclid(16), p.z.div_euclid(16));
                let at = g.buried_treasure.blocks(&g, c).last().unwrap().0;
                features.extend(g.buried_treasure.features(&g, chunk_of(at)));
            }
            assert!(!features.is_empty());
            let mut checked_treasure = false;
            let mut checked_portal = false;
            for (at, f) in features {
                let Feature::TempleChest(_, table) = f else { continue };
                if matches!(table, LootTable::BuriedTreasure) {
                    checked_treasure = true;
                } else {
                    checked_portal = true;
                }
                let c = chunk_of(at);
                let data = g.generate(c);
                let mut world = super::super::World::new_headless(g.clone(), Default::default(), 1);
                world.register_structure_features(c, &data);
                assert!(world.chest(at).is_some(), "{dim:?} {at} {table:?}");
                assert!(world.chest(at).unwrap().slots.iter().any(Option::is_some));
                world.chest_mut(at).unwrap().slots.fill(None);
                let saved = world.chests_to_string();
                let mut loaded = super::super::World::new_headless(g.clone(), Default::default(), 1);
                loaded.load_chests(&saved);
                loaded.register_structure_features(c, &data);
                assert!(loaded.chest(at).unwrap().slots.iter().all(Option::is_none));
            }
            assert!(checked_portal);
            assert!(dim == Dimension::Nether || checked_treasure);
        }
    }
    #[test]
    fn generations_one_and_two_never_add_portals_or_treasure() {
        for gen_version in [Generation::V1, Generation::V2] {
            for dim in [Dimension::Overworld, Dimension::Nether] {
                let g = Generator::with_generation(42, dim, gen_version);
                assert!(g.ruined_portals.nearest(&g, IVec3::ZERO, 1).is_none());
                assert!(g.ruined_portals.features(&g, IVec3::ZERO).is_empty());
                assert!(g.buried_treasure.nearest_buried_treasure(&g, IVec3::ZERO, 1).is_none());
            }
        }
    }
}
