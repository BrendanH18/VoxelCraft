//! Generation 3 ancient cities (GEN-05, v0.16 track B).
//!
//! Java 26.3 references: `worldgen/structure/ancient_city.json` (jigsaw,
//! start pool `ancient_city/city_center`, start jigsaw `city_anchor`, start
//! height absolute -27, size 7, max distance 116, `beard_box`, empty spawn
//! overrides for every category), `structure_set/ancient_cities.json`
//! (random spread 24/8, salt 20083232), the `has_structure/ancient_city`
//! biome tag (deep dark), the three `ancient_city_*_degradation` processor
//! lists, the `ancient_city/sculk` pool and `sculk_patch_ancient_city`.
//! Templates are original (`jigsaw/ancient_city.txt`); see the
//! verification record for every difference.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::cave_blocks as cb;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::jigsaw::{self, Palette, Placed};
use super::noise::{hash_f, hash3};
use super::stone_blocks as sb;
use super::structure::{Bounds, Rng};
use super::terrain::{Biome, Dimension, Generation, Generator};

pub const SPACING: i32 = 24;
pub const SEPARATION: i32 = 8;
pub const SALT: u64 = 20_083_232;
/// `start_height`: the `city_anchor` jigsaw's height.
pub const START_Y: i32 = -27;
/// The start's floor course: the anchor sits 25 blocks above it (Java's
/// city centres have their walk level at -51).
pub const FLOOR_Y: i32 = -52;
/// `size`: the jigsaw depth.
pub const MAX_DEPTH: u8 = 7;
pub const MAX_DISTANCE: i32 = 116;
/// Terrain carved beside a piece's box, and filled under it (`beard_box`).
const MARGIN: i32 = 3;
const BEARD: i32 = 6;
/// The largest horizontal reach of a city's pieces from its start.
const REACH: i32 = MAX_DISTANCE + MARGIN + 2;
/// The lowest and highest cells a city touches.
const MIN_Y: i32 = FLOOR_Y - BEARD;
const MAX_Y: i32 = FLOOR_Y + 26;
const CACHE_LIMIT: usize = 64;
const PATCH_SALT: u64 = 0xAC17_5C01;

/// `#has_structure/ancient_city`.
pub fn allowed(b: Biome) -> bool {
    b == Biome::DeepDark
}

/// `#ancient_city_replaceable`, the block rot's target.
fn rottable(b: Block) -> bool {
    b == Block::DEEPSLATE
        || b == sb::DEEPSLATE_BRICKS
        || b == sb::DEEPSLATE_TILES
        || b == sb::CRACKED_DEEPSLATE_BRICKS
        || b == sb::CRACKED_DEEPSLATE_TILES
        || b == Block::COBBLED_DEEPSLATE
        || [
            "deepslate brick slab",
            "deepslate tile slab",
            "deepslate brick stairs",
            "deepslate tile wall",
            "deepslate brick wall",
            "gray wool",
        ]
        .contains(&b.name())
}

/// `#sculk_replaceable_world_gen` as it can occur in and under a city.
fn sculk_replaceable(b: Block) -> bool {
    matches!(
        b,
        Block::STONE
            | Block::DEEPSLATE
            | Block::GRANITE
            | Block::DIORITE
            | Block::ANDESITE
            | Block::TUFF
            | Block::CALCITE
            | Block::DIRT
            | Block::GRAVEL
            | Block::CLAY
            | Block::SAND
            | Block::COBBLED_DEEPSLATE
            | Block::POLISHED_DEEPSLATE
            | Block::SOUL_SAND
    ) || b == sb::DEEPSLATE_BRICKS
        || b == sb::DEEPSLATE_TILES
        || b == sb::CRACKED_DEEPSLATE_BRICKS
        || b == sb::CRACKED_DEEPSLATE_TILES
        || b == super::overworld_blocks::SMOOTH_BASALT
        || b == super::overworld_blocks::DRIPSTONE_BLOCK
}

/// Which processor list a piece uses (`ancient_city/*` pool elements).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Processors {
    /// `ancient_city_start_degradation`.
    Start,
    /// `ancient_city_generic_degradation`.
    Generic,
    /// `ancient_city_walls_degradation`.
    Walls,
    /// The ice box element's empty processor list.
    None,
}

fn processors(name: &str) -> Processors {
    if name.starts_with("ancient_city/city_center/city_center_") {
        Processors::Start
    } else if name.starts_with("ancient_city/walls/") {
        Processors::Walls
    } else if name == "ancient_city/structures/ice_box_1" {
        Processors::None
    } else {
        Processors::Generic
    }
}

pub(super) struct CityPalette;

impl Palette for CityPalette {
    fn role(&self, role: &str) -> Option<Block> {
        use super::colour_blocks::candle::candle;
        Some(match role {
            "shrieker" => cb::sculk_shrieker(false, true, false),
            "candle2" => candle(0, 2, false, false),
            "candle3" => candle(0, 3, false, false),
            "candle4" => candle(0, 4, false, false),
            "hanging_lantern" => super::mount_states::lantern_block(true, true, false),
            _ => return None,
        })
    }

    /// The pieces' processor lists. `BlockRotProcessor` and `RuleProcessor`
    /// each seed a fresh random from `Mth.getSeed(pos)`; a rule's
    /// `random_block_match` draws only when its block matches.
    fn process(&self, block: Block, at: IVec3, piece: &Placed) -> Option<Block> {
        let name = &jigsaw::ancient_cities().templates[piece.template as usize].name;
        let list = processors(name);
        if list == Processors::None {
            return Some(block);
        }
        if list != Processors::Start && rottable(block) && jigsaw::processor_random(at).next_float() > 0.95 {
            return None;
        }
        let soul_lantern = super::mount_states::lantern_of(block).is_some_and(|l| l.soul);
        let tile_slab = block.name() == "deepslate tile slab";
        let mut rng = jigsaw::processor_random(at);
        if block == sb::DEEPSLATE_BRICKS {
            if rng.next_float() < 0.3 {
                return Some(sb::CRACKED_DEEPSLATE_BRICKS);
            }
        } else if block == sb::DEEPSLATE_TILES {
            if rng.next_float() < 0.3 {
                return Some(sb::CRACKED_DEEPSLATE_TILES);
            }
        } else if list == Processors::Walls && tile_slab {
            if rng.next_float() < 0.3 {
                return Some(Block::AIR);
            }
        } else if soul_lantern && rng.next_float() < 0.05 {
            return Some(Block::AIR);
        }
        Some(block)
    }

    fn clear_above(&self) -> i32 {
        0
    }
}

/// What a sculk patch does to one cell, decided when the city is laid out
/// and applied to the generated terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatchOp {
    /// Sculk, if the block there is `#sculk_replaceable_world_gen`.
    Sculk,
    /// A sculk vein on these faces, if the cell is air.
    Vein(u8),
    /// A catalyst or a summoning shrieker, if the cell is air.
    Place(Block),
}

/// One generated ancient city.
#[derive(Debug)]
pub struct City {
    /// Java's generation stub: the start box's centre at the start height.
    pub at: IVec3,
    pub pieces: Vec<Placed>,
    /// Every piece's box (`bounding_box: full` of the spawn overrides).
    pub bounds: Bounds,
    /// Chests: position, loot seed, whether it's the ice box's.
    pub chests: Vec<(IVec3, u64, bool)>,
    cells: FxHashMap<IVec3, Vec<(u16, Block)>>,
    ops: FxHashMap<IVec3, Vec<(u16, PatchOp)>>,
}

fn chunk_key(p: IVec3) -> (IVec3, u16) {
    let c = p.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    let l = p - c * CHUNK_SIZE_I;
    (c, index(l.x as usize, l.y as usize, l.z as usize) as u16)
}

/// A piece's box widened by the carved margin (`beard_box`).
fn carved(b: &Bounds) -> Bounds {
    Bounds { min: b.min - IVec3::new(MARGIN, 0, MARGIN), max: b.max + IVec3::new(MARGIN, 1, MARGIN) }
}

impl City {
    fn build(pieces: Vec<Placed>, seed: u64) -> City {
        let lib = jigsaw::ancient_cities();
        let palette = CityPalette;
        let roles: Vec<_> = lib.roles.iter().map(|r| palette.role(r)).collect();
        let mut baked: FxHashMap<IVec3, Block> = FxHashMap::default();
        for p in &pieces {
            let t = &lib.templates[p.template as usize];
            let mut put = |at: IVec3, spec: &jigsaw::Spec| {
                let w = p.world(at);
                if let Some(b) = jigsaw::resolve(spec, &roles, &palette, p, at).and_then(|b| palette.process(b, w, p)) {
                    baked.insert(w, b);
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
        let bounds = pieces.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b)).expect("city start");
        let carved_boxes: Vec<Bounds> = pieces.iter().map(|p| carved(&p.bounds)).collect();
        // Open: cleared by the carving and not a template block (or air).
        let cleared = |q: IVec3| carved_boxes.iter().any(|b| b.contains(q) && q.y > b.min.y);
        let floor = |q: IVec3| carved_boxes.iter().any(|b| b.contains(q) && q.y == b.min.y);
        let open = |q: IVec3| match baked.get(&q) {
            Some(&b) => b == Block::AIR,
            None => cleared(q),
        };
        let sturdy = |q: IVec3| match baked.get(&q) {
            Some(&b) => b.is_opaque(),
            None => floor(q) && !cleared(q),
        };
        let mut ops: Vec<(IVec3, PatchOp)> = Vec::new();
        for p in &pieces {
            if lib.templates[p.template as usize].name != "ancient_city/sculk_patch" {
                continue;
            }
            let o = p.origin;
            let mut rng = Rng(hash3(o.x, o.y, o.z, seed ^ PATCH_SALT));
            // `sculk_patch` (10 charges of 32): sculk over exposed stone and
            // city blocks, thinning out towards the edge.
            let radius = 4 + rng.below(3) as i32;
            let mut sculk = rustc_hash::FxHashSet::default();
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    for dz in -radius..=radius {
                        let d = IVec3::new(dx, dy, dz);
                        let r = (d.length_squared() as f32).sqrt();
                        if r > radius as f32 {
                            continue;
                        }
                        let q = o + d;
                        let replaceable = match baked.get(&q) {
                            Some(&b) => sculk_replaceable(b),
                            None => floor(q) && !cleared(q),
                        };
                        if !replaceable || !cb::DIRS.iter().any(|&n| open(q + n)) {
                            continue;
                        }
                        if hash_f(q.x, q.y, q.z, seed ^ PATCH_SALT) < 1.0 - 0.7 * r / (radius as f32 + 1.0) {
                            sculk.insert(q);
                            ops.push((q, PatchOp::Sculk));
                        }
                    }
                }
            }
            // Veins on the faces round the patch's edge.
            for dx in -radius - 1..=radius + 1 {
                for dy in -2..=2 {
                    for dz in -radius - 1..=radius + 1 {
                        let q = o + IVec3::new(dx, dy, dz);
                        if !open(q) {
                            continue;
                        }
                        let mut faces = 0u8;
                        for (i, &n) in cb::DIRS.iter().enumerate() {
                            let s = q + n;
                            if !sculk.contains(&s)
                                && sturdy(s)
                                && hash_f(s.x * 6 + i as i32, s.y, s.z, seed ^ PATCH_SALT ^ 1) < 0.3
                            {
                                faces |= 1 << i;
                            }
                        }
                        if faces != 0 {
                            ops.push((q, PatchOp::Vein(faces)));
                        }
                    }
                }
            }
            // The overlay: a catalyst half the time, then one to three
            // summoning shriekers within two blocks, each on a sturdy floor.
            if rng.unit() < 0.5 && open(o) && sturdy(o - IVec3::Y) {
                ops.push((o, PatchOp::Place(cb::sculk_catalyst(false))));
            }
            for _ in 0..1 + rng.below(3) {
                let off =
                    IVec3::new(rng.range(0, 4) as i32 - 2, rng.range(0, 4) as i32 - 2, rng.range(0, 4) as i32 - 2);
                let q = o + off;
                if open(q) && sturdy(q - IVec3::Y) && q != o {
                    ops.push((q, PatchOp::Place(cb::sculk_shrieker(false, true, false))));
                }
            }
        }
        let mut cells: FxHashMap<IVec3, Vec<(u16, Block)>> = FxHashMap::default();
        let mut sorted: Vec<_> = baked.into_iter().collect();
        sorted.sort_by_key(|(p, _)| (p.x, p.y, p.z));
        for (p, b) in sorted {
            let (c, i) = chunk_key(p);
            cells.entry(c).or_default().push((i, b));
        }
        let mut grouped: FxHashMap<IVec3, Vec<(u16, PatchOp)>> = FxHashMap::default();
        for (p, op) in ops {
            let (c, i) = chunk_key(p);
            grouped.entry(c).or_default().push((i, op));
        }
        let chests = jigsaw::markers(lib, &pieces, &|_, _| FLOOR_Y)
            .into_iter()
            .filter_map(|m| match m {
                jigsaw::Marker::Loot(at, name, seed) => Some((at, seed, name == "ancient_city_ice_box")),
                _ => None,
            })
            .collect();
        let start = pieces[0].bounds;
        let at = IVec3::new((start.min.x + start.max.x) / 2, START_Y, (start.min.z + start.max.z) / 2);
        City { at, pieces, bounds, chests, cells, ops: grouped }
    }

    /// Paints the city's part of the chunk at `base`: carve every piece's
    /// widened box above its floor, fill the gaps under it, then the
    /// template blocks and the sculk patches.
    fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let wide = Bounds {
            min: self.bounds.min - IVec3::new(MARGIN, BEARD, MARGIN),
            max: self.bounds.max + IVec3::new(MARGIN, 1, MARGIN),
        };
        if !wide.intersects(&chunk) {
            return;
        }
        let local = |p: IVec3| index((p.x - base.x) as usize, (p.y - base.y) as usize, (p.z - base.z) as usize);
        for p in &self.pieces {
            let b = carved(&p.bounds);
            let lo = b.min.max(base);
            let hi = b.max.min(top);
            if lo.x > hi.x || lo.z > hi.z {
                continue;
            }
            for x in lo.x..=hi.x {
                for z in lo.z..=hi.z {
                    for y in (b.min.y + 1).max(base.y)..=hi.y {
                        blocks[local(IVec3::new(x, y, z))] = Block::AIR;
                    }
                    for y in (b.min.y - BEARD).max(base.y)..=b.min.y.min(top.y) {
                        let i = local(IVec3::new(x, y, z));
                        if blocks[i] == Block::AIR || blocks[i].is_fluid() {
                            blocks[i] = Block::DEEPSLATE;
                        }
                    }
                }
            }
        }
        let c = base.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        if let Some(cells) = self.cells.get(&c) {
            for &(i, b) in cells {
                blocks[i as usize] = b;
            }
        }
        if let Some(ops) = self.ops.get(&c) {
            for &(i, op) in ops {
                let here = &mut blocks[i as usize];
                match op {
                    PatchOp::Sculk if sculk_replaceable(*here) => *here = cb::SCULK,
                    PatchOp::Vein(faces) if *here == Block::AIR => {
                        *here = cb::multiface(cb::Multiface::SculkVein, faces, false)
                    }
                    PatchOp::Place(b) if *here == Block::AIR || cb::multiface_of(*here).is_some() => *here = b,
                    _ => {}
                }
            }
        }
    }
}

pub struct AncientCities {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<City>>>>,
}

impl AncientCities {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    /// The Java chunk a region's city would start in (random spread).
    pub fn start_chunk(&self, region: IVec2) -> IVec2 {
        super::trail_ruins::spread(self.seed, region, SPACING, SEPARATION, SALT)
    }

    /// Where a region's city would stand (its generation stub), checking
    /// the biome without laying the city out. Every assembly places its
    /// start, so this is exactly where [`AncientCities::get`] finds one.
    pub fn stub(&self, g: &Generator, region: IVec2) -> Option<IVec3> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        let at = self.start_chunk(region) * 16;
        let stub = IVec3::new(at.x, START_Y, at.y);
        allowed(g.biome_at(stub)).then_some(stub)
    }

    /// The city of a placement region, if one generates there.
    pub fn get(&self, g: &Generator, region: IVec2) -> Option<Arc<City>> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        if let Some(v) = self.cache.lock().unwrap().get(&region).cloned() {
            return v;
        }
        let at = self.start_chunk(region) * 16;
        // The start box is centred on the anchor, so Java's biome check at
        // the stub (the start box's centre, at the start height) can run
        // before the assembly.
        let found = self.stub(g, region).map(|_| {
            let lib = jigsaw::ancient_cities();
            let place = |t: &jigsaw::Template, r: u8| {
                let centre = jigsaw::rotate(IVec3::new(t.size.x / 2, 0, t.size.z / 2), r);
                IVec3::new(at.x - centre.x, FLOOR_Y, at.y - centre.z)
            };
            let settings = jigsaw::Settings {
                start_pool: "ancient_city/city_center",
                max_depth: MAX_DEPTH,
                max_distance: MAX_DISTANCE,
            };
            let seed = hash3(at.x, 0, at.y, self.seed ^ SALT);
            let pieces = jigsaw::assemble_with(lib, &settings, &place, &|_, _| FLOOR_Y, seed);
            Arc::new(City::build(pieces, seed))
        });
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.entry(region).or_insert(found).clone()
    }

    fn regions_near(&self, min: IVec2, max: IVec2) -> impl Iterator<Item = IVec2> {
        let size = IVec2::splat(16 * SPACING);
        let lo = (min - IVec2::splat(REACH)).div_euclid(size);
        let hi = (max + IVec2::splat(REACH)).div_euclid(size);
        (lo.y..=hi.y).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec2::new(x, z)))
    }

    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return;
        }
        if base.y + CHUNK_SIZE_I - 1 < MIN_Y || base.y > MAX_Y {
            return;
        }
        let min = IVec2::new(base.x, base.z);
        for region in self.regions_near(min, min + IVec2::splat(CHUNK_SIZE_I - 1)) {
            if let Some(city) = self.get(g, region) {
                city.paint(blocks, base);
            }
        }
    }

    /// The chests a chunk's cities put in it.
    pub fn features(&self, g: &Generator, cpos: IVec3) -> Vec<(IVec3, super::fortress::Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let mut out = Vec::new();
        if base.y + CHUNK_SIZE_I - 1 < MIN_Y || base.y > MAX_Y {
            return out;
        }
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        let min = IVec2::new(base.x, base.z);
        for region in self.regions_near(min, min + IVec2::splat(CHUNK_SIZE_I - 1)) {
            if let Some(city) = self.get(g, region) {
                for &(p, seed, ice) in &city.chests {
                    if chunk.contains(p) {
                        out.push((p, super::fortress::Feature::AncientCityChest(seed, ice)));
                    }
                }
            }
        }
        out
    }

    /// Whether `p` lies in a city's full bounding box, where no mob spawns
    /// naturally (the empty `spawn_overrides`).
    pub fn contains(&self, g: &Generator, p: IVec3) -> bool {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld || p.y < MIN_Y || p.y > MAX_Y {
            return false;
        }
        let at = IVec2::new(p.x, p.z);
        self.regions_near(at, at).any(|r| self.get(g, r).is_some_and(|c| c.bounds.contains(p)))
    }

    /// The nearest city's stub within `max_blocks` (`/locate`).
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
                if let Some(at) = self.stub(g, centre + IVec2::new(x, z)) {
                    let d = (IVec2::new(at.x, at.z).as_i64vec2() - origin.as_i64vec2()).length_squared();
                    if d < distance {
                        distance = d;
                        best = Some(at);
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests;
