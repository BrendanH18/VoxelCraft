//! v0.16 Track H: sulfur springs in generation 5's sulfur caves (Java 26.3
//! `rooted_sulfur_spring`, a `root_system` feature around `sulfur_spring`).
//!
//! Placement follows the reference (`tests/fixtures/java/26.3/wave1_leftovers.json`):
//! per Java chunk, one or two tries (`count` uniform 1..=2, `in_square`, a
//! height from the bottom to 256); from an air cell an upward scan of at
//! most 12 finds a solid ceiling and the cell under it is the origin, which
//! must be in the sulfur caves (`biome`). `RootSystemFeature` then climbs
//! from the origin (at most 184 cells, never above the surface) to the
//! first air cell with five more air cells above it, a solid, non-lava
//! floor and level ground eight blocks out on four sides (two below solid,
//! two above open). There `sulfur_spring` picks a size (small 200, medium
//! 90, large 20, extra large 5): it scatters tuff over the solid ground
//! around (64–128 tries, trapezoid offsets, a downward scan of four) and
//! sets one of the size's templates seven blocks down, centred and turned
//! a random quarter (`TemplateFeature`). Finally the rock column between
//! the cave below and the spring turns to sulfur where it is
//! `#azalea_root_replaceable` (20 tries a layer within radius 3), and a
//! sulfur block hangs from the lower cave's ceiling at the origin.
//!
//! The ten templates are original (FE-02): generated basins with Java's
//! sizes, a pool whose surface is level with the cave floor, dormant
//! potent sulfur over magma at its bottom (two vents in the extra large
//! one), a sulfur rim with spikes, and rock of tuff, stone, granite,
//! cinnabar and sulfur below. Random draws are the engine's (FE-01), and
//! the layout is decided against the pristine terrain
//! (`Generator::natural_block`) and clipped into every chunk it reaches,
//! like geodes, so load order can't cut a spring.

use std::sync::{Arc, Mutex, OnceLock};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::biome::Biome;
use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, WORLD_MAX_Y, WORLD_MIN_Y, index};
use super::gen5_blocks::{self as g, Potent};
use super::noise::hash3;
use super::overworld_blocks::{self as ob, Thickness};
use super::structure::{Bounds, Rng};
use super::terrain::Generator;

const SALT: u64 = 0x5355_4C46_5350_5247;
const TEMPLATE_SALT: u64 = 0x5350_5249_4E47_5450;
const CACHE_LIMIT: usize = 512;
const JAVA_CHUNK: i32 = 16;

/// `rooted_sulfur_spring`'s placement: `count` 1..=2, heights from the
/// bottom to 256, a scan of at most 12 up through air.
pub const COUNT: (u32, u32) = (1, 2);
pub const MAX_Y: i32 = 256;
pub const CEILING_SCAN: i32 = 12;
/// `RootSystemFeature`'s settings.
pub const REQUIRED_SPACE: i32 = 5;
pub const LEVEL_TEST_DISTANCE: i32 = 8;
pub const MAX_LEVEL_DEVIATION: i32 = 2;
pub const ROOT_RADIUS: i32 = 3;
pub const ROOT_ATTEMPTS: u32 = 20;
pub const ROOT_COLUMN_MAX: i32 = 184;
/// `sulfur_spring`'s sizes: weight, tuff tries, tuff spread (x/z, y) and
/// the templates' names.
pub const SIZES: [(u32, u32, i32, i32, &[&str]); 4] = [
    (
        200,
        64,
        7,
        3,
        &[
            "spring/sulfur_spring_small_1",
            "spring/sulfur_spring_small_2",
            "spring/sulfur_spring_small_3",
            "spring/sulfur_spring_small_4",
        ],
    ),
    (
        90,
        80,
        8,
        3,
        &["spring/sulfur_spring_medium_1", "spring/sulfur_spring_medium_2", "spring/sulfur_spring_medium_3"],
    ),
    (20, 96, 9, 3, &["spring/sulfur_spring_large_1", "spring/sulfur_spring_large_2"]),
    (5, 128, 10, 3, &["spring/sulfur_spring_extra_large_1"]),
];
/// The templates sit seven blocks under the spring's origin.
pub const TEMPLATE_OFFSET_Y: i32 = -7;
/// The tuff scan's `max_steps`.
pub const TUFF_SCAN: i32 = 4;
/// Java's template sizes (x, y, z), in [`SIZES`] order.
pub const TEMPLATE_SIZES: [(&str, [i32; 3]); 10] = [
    ("spring/sulfur_spring_small_1", [8, 11, 9]),
    ("spring/sulfur_spring_small_2", [9, 11, 8]),
    ("spring/sulfur_spring_small_3", [9, 11, 9]),
    ("spring/sulfur_spring_small_4", [9, 11, 9]),
    ("spring/sulfur_spring_medium_1", [11, 11, 11]),
    ("spring/sulfur_spring_medium_2", [12, 11, 11]),
    ("spring/sulfur_spring_medium_3", [12, 11, 12]),
    ("spring/sulfur_spring_large_1", [13, 11, 12]),
    ("spring/sulfur_spring_large_2", [12, 11, 13]),
    ("spring/sulfur_spring_extra_large_1", [16, 11, 16]),
];

/// `#azalea_root_replaceable` members this engine has.
pub fn root_replaceable(b: Block) -> bool {
    matches!(
        b,
        Block::STONE
            | Block::GRANITE
            | Block::DIORITE
            | Block::ANDESITE
            | Block::TUFF
            | Block::DEEPSLATE
            | Block::GRAVEL
            | Block::SAND
            | Block::RED_SAND
            | Block::CLAY
            | Block::TERRACOTTA
    ) || ob::dirt_like(b)
        || b == ob::MUD
        || b == ob::MOSS_BLOCK
        || b.terracotta_colour().is_some()
}

/// One original template: its size and every cell it writes (air too),
/// in template coordinates.
pub struct Template {
    pub name: &'static str,
    pub size: IVec3,
    pub cells: Vec<(IVec3, Block)>,
}

/// The ten generated templates, in [`TEMPLATE_SIZES`] order.
pub fn templates() -> &'static [Template] {
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        TEMPLATE_SIZES.iter().enumerate().map(|(i, &(name, size))| build_template(i, name, IVec3::from(size))).collect()
    })
}

/// An original spring basin: a rounded rock body seven layers deep, a pool
/// level with the floor over one or two dormant vents, a sulfur rim with
/// spikes and open air above.
fn build_template(i: usize, name: &'static str, size: IVec3) -> Template {
    let mut rng = Rng(hash3(i as i32, size.x, size.z, TEMPLATE_SALT));
    let (sx, sz) = (size.x as f64, size.z as f64);
    let centre = |x: i32, z: i32| ((x as f64 + 0.5 - sx / 2.0) / (sx / 2.0), (z as f64 + 0.5 - sz / 2.0) / (sz / 2.0));
    // Pool size and depth by class.
    let class = match size.x * size.z {
        a if a < 100 => 0,
        a if a < 140 => 1,
        a if a < 200 => 2,
        _ => 3,
    };
    let pool_r = [0.32, 0.42, 0.5, 0.62][class];
    let depth = [1, 1, 2, 2][class];
    let vents: Vec<(i32, i32)> = if class == 3 {
        vec![(size.x / 2 - 2, size.z / 2 - 2), (size.x / 2 + 2, size.z / 2)]
    } else {
        let jx = rng.below(3) as i32 - 1;
        let jz = rng.below(3) as i32 - 1;
        vec![(size.x / 2 + jx.clamp(-1, 0), size.z / 2 + jz.clamp(-1, 0))]
    };
    let mut cells: FxHashMap<IVec3, Block> = FxHashMap::default();
    let rock = |rng: &mut Rng, rim: bool| {
        let r = rng.below(100);
        if rim {
            match r {
                0..40 => g::SULFUR,
                40..62 => Block::TUFF,
                62..80 => g::CINNABAR,
                80..90 => Block::GRANITE,
                _ => Block::STONE,
            }
        } else {
            match r {
                0..36 => Block::TUFF,
                36..54 => Block::GRANITE,
                54..72 => Block::STONE,
                72..87 => g::CINNABAR,
                _ => g::SULFUR,
            }
        }
    };
    for z in 0..size.z {
        for x in 0..size.x {
            let (u, v) = centre(x, z);
            let d = (u * u + v * v).sqrt();
            // The body narrows downward: full width at the top layer.
            for y in 0..7 {
                let reach = 0.62 + 0.38 * (y as f64 / 6.0) + (rng.unit() - 0.5) * 0.12;
                if d <= reach {
                    cells.insert(IVec3::new(x, y, z), rock(&mut rng, y >= 5 && d > pool_r));
                }
            }
            if d > 1.02 {
                continue;
            }
            // Open air over the footprint.
            for y in 7..size.y {
                cells.insert(IVec3::new(x, y, z), Block::AIR);
            }
            // The pool: one or two layers of water, level with the floor
            // (its surface at the top body layer).
            if d <= pool_r {
                let deep = if d <= pool_r * 0.55 { depth } else { 1 };
                for k in 0..deep {
                    cells.insert(IVec3::new(x, 6 - k, z), Block::WATER);
                }
            } else if d <= pool_r + 0.3 && rng.below(10) < 7 {
                // The rim: sulfur crust a block up, now and then two.
                cells.insert(IVec3::new(x, 7, z), g::SULFUR);
                if rng.below(4) == 0 {
                    cells.insert(IVec3::new(x, 8, z), g::SULFUR);
                }
            }
        }
    }
    // Vents: dormant potent sulfur over magma under the deepest water.
    for &(vx, vz) in &vents {
        let top = 6 - depth;
        for k in 0..depth {
            cells.insert(IVec3::new(vx, 6 - k, vz), Block::WATER);
        }
        cells.insert(IVec3::new(vx, top, vz), g::potent(Potent::Dormant));
        cells.insert(IVec3::new(vx, top - 1, vz), Block::MAGMA);
    }
    // Spikes rise from the rim's top blocks, one to three long.
    let mut tops: Vec<IVec3> = cells
        .iter()
        .filter(|&(p, b)| *b == g::SULFUR && p.y >= 7 && cells.get(&(*p + IVec3::Y)) == Some(&Block::AIR))
        .map(|(p, _)| *p)
        .collect();
    tops.sort_by_key(|p| (p.x, p.y, p.z));
    let wanted = ((size.x * size.z) as f64 * 0.14) as usize + 2;
    let mut placed = 0;
    while placed < wanted && !tops.is_empty() {
        let p = tops.swap_remove(rng.below(tops.len() as u32) as usize);
        let len = 1 + rng.below(3) as i32;
        let run: Vec<IVec3> = (1..=len)
            .map(|k| p + IVec3::Y * k)
            .take_while(|q| q.y < size.y && cells.get(q) == Some(&Block::AIR))
            .collect();
        let n = run.len();
        for (k, q) in run.iter().enumerate() {
            let t = match k {
                k if k + 1 == n => Thickness::Tip,
                k if k + 2 == n => Thickness::Frustum,
                0 => Thickness::Base,
                _ => Thickness::Middle,
            };
            cells.insert(*q, g::spike(false, t, false));
        }
        placed += usize::from(n > 0);
    }
    let mut cells: Vec<(IVec3, Block)> = cells.into_iter().collect();
    cells.sort_by_key(|(p, _)| (p.y, p.z, p.x));
    Template { name, size, cells }
}

/// `TemplateFeature.getRotatedOffset` and `StructureTemplate`'s rotation
/// about the placement corner, for quarter turns clockwise `turns`.
pub fn placed_cell(origin: IVec3, size: IVec3, turns: u8, local: IVec3) -> IVec3 {
    let rot = |v: IVec3| match turns % 4 {
        0 => v,
        1 => IVec3::new(-v.z, v.y, v.x),
        2 => IVec3::new(-v.x, v.y, -v.z),
        _ => IVec3::new(v.z, v.y, -v.x),
    };
    // `rotation.rotate(WEST) * sizeX / 2` and `rotate(NORTH) * sizeZ / 2`.
    let corner = origin + rot(IVec3::new(-(size.x / 2), 0, 0)) + rot(IVec3::new(0, 0, -(size.z / 2)));
    corner + rot(local)
}

/// Why a try placed no spring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reject {
    /// The column's climate can never be sulfur caves.
    Climate,
    /// `environment_scan`: the start isn't air, or no ceiling within 12.
    StartNotAir,
    NoCeiling,
    /// The origin isn't in the sulfur caves (`biome`).
    Biome,
    /// The root system's climb reached the surface, met a liquid or
    /// unsolid floor, or found no room within 184 blocks.
    Surface,
    Floor,
    NoRoom,
    Template,
}

/// What one spring writes, in Java's order.
#[derive(Debug)]
pub struct Spring {
    pub origin: IVec3,
    /// The cave floor cell the spring stands on (the root system's tree
    /// position).
    pub spring: IVec3,
    pub bounds: Bounds,
    /// Tuff and template cells: written as given.
    pub cells: Box<[(IVec3, Block)]>,
    /// Root cells: sulfur where the chunk holds a replaceable block then.
    pub roots: Box<[IVec3]>,
    /// The hanging sulfur under the lower cave's ceiling, if it fits.
    pub hanging: Option<IVec3>,
}

impl Spring {
    fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        if !self.bounds.intersects(&chunk) {
            return;
        }
        let at = |p: IVec3| {
            let l = p - base;
            index(l.x as usize, l.y as usize, l.z as usize)
        };
        for &(p, b) in self.cells.iter() {
            if chunk.contains(p) && blocks[at(p)] != Block::BEDROCK {
                blocks[at(p)] = b;
            }
        }
        for &p in self.roots.iter() {
            if chunk.contains(p) && root_replaceable(blocks[at(p)]) {
                blocks[at(p)] = g::SULFUR;
            }
        }
        if let Some(p) = self.hanging.filter(|p| chunk.contains(*p))
            && blocks[at(p)] == Block::AIR
        {
            blocks[at(p)] = g::SULFUR;
        }
    }
}

/// Generation 5's sulfur springs, laid out once per 32-block column and
/// cached.
pub struct SulfurSprings {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Arc<Vec<Arc<Spring>>>>>,
}

impl SulfurSprings {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    /// The tries of Java chunk `c`: positions and the random stream each
    /// continues with.
    fn candidates(&self, c: IVec2) -> Vec<(IVec3, Rng)> {
        let mut rng = Rng(hash3(c.x, 0, c.y, self.seed ^ SALT));
        let n = rng.range(COUNT.0, COUNT.1);
        (0..n)
            .map(|_| {
                let x = c.x * JAVA_CHUNK + rng.below(JAVA_CHUNK as u32) as i32;
                let z = c.y * JAVA_CHUNK + rng.below(JAVA_CHUNK as u32) as i32;
                let y = WORLD_MIN_Y + rng.below((MAX_Y - WORLD_MIN_Y + 1) as u32) as i32;
                (IVec3::new(x, y, z), Rng(rng.next_u64()))
            })
            .collect()
    }

    /// One try: the placement filters, the root system's climb and the
    /// spring, against pristine terrain.
    pub fn build(&self, generator: &Generator, start: IVec3, rng: Rng) -> Option<Spring> {
        self.try_build(generator, start, rng).ok()
    }

    /// [`SulfurSprings::build`], saying which filter refused a try.
    pub fn try_build(&self, generator: &Generator, start: IVec3, mut rng: Rng) -> Result<Spring, Reject> {
        // Only columns whose climate allows sulfur caves can pass the
        // biome filter; skip the terrain probes elsewhere.
        let column = generator.column(start.x, start.z);
        if super::biome::cave_in(&column.climate, 40, true) != Some(Biome::SulfurCaves) {
            return Err(Reject::Climate);
        }
        let mut heights = FxHashMap::default();
        let mut nodes = FxHashMap::default();
        let mut cache: FxHashMap<IVec3, Block> = FxHashMap::default();
        let mut terrain = |p: IVec3| -> Block {
            if !(WORLD_MIN_Y..WORLD_MAX_Y).contains(&p.y) {
                return Block::AIR;
            }
            *cache.entry(p).or_insert_with(|| {
                if generator.bedrock_at(p) {
                    return Block::BEDROCK;
                }
                match generator.natural_block(p, &mut heights, &mut nodes) {
                    Block::BEDROCK => Block::DEEPSLATE,
                    b => b,
                }
            })
        };
        // `environment_scan` up through air to a solid ceiling, then one down.
        if terrain(start) != Block::AIR {
            return Err(Reject::StartNotAir);
        }
        let mut p = start;
        let mut found = false;
        for _ in 0..=CEILING_SCAN {
            if terrain(p).is_solid() {
                found = true;
                break;
            }
            p.y += 1;
            if p.y >= WORLD_MAX_Y || terrain(p) != Block::AIR && !terrain(p).is_solid() {
                break;
            }
        }
        if !found {
            return Err(Reject::NoCeiling);
        }
        let origin = p - IVec3::Y;
        if generator.biome_at(origin) != Biome::SulfurCaves || terrain(origin) != Block::AIR {
            return Err(Reject::Biome);
        }
        // `placeDirtAndTree`: climb to a cave floor with room above.
        let surface = column.height.max(super::climate::SEA_LEVEL - 1);
        let mut work = origin;
        let mut spot = None;
        for y in 0..ROOT_COLUMN_MAX {
            work.y += 1;
            // `getHeight(WORLD_SURFACE)` is one over the top block.
            if surface + 1 < work.y {
                return Err(Reject::Surface);
            }
            if terrain(work) != Block::AIR || !(1..=REQUIRED_SPACE).all(|i| terrain(work + IVec3::Y * i) == Block::AIR)
            {
                continue;
            }
            // `levelTestDistance`: SOUTH, WEST, NORTH, EAST.
            let level = [IVec3::Z, IVec3::NEG_X, IVec3::NEG_Z, IVec3::X].iter().all(|&d| {
                let corner = work + d * LEVEL_TEST_DISTANCE;
                terrain(corner - IVec3::Y * MAX_LEVEL_DEVIATION) != Block::AIR
                    && terrain(corner + IVec3::Y * MAX_LEVEL_DEVIATION) == Block::AIR
            });
            if !level {
                continue;
            }
            let floor = terrain(work - IVec3::Y);
            if floor.is_lava() || !floor.is_solid() {
                return Err(Reject::Floor);
            }
            spot = Some((work, y));
            break;
        }
        let (spring, climbed) = spot.ok_or(Reject::NoRoom)?;
        // `sulfur_spring`: a size by weight.
        let total: u32 = SIZES.iter().map(|s| s.0).sum();
        let mut pick = rng.below(total);
        let mut size = SIZES[0];
        for s in SIZES {
            if pick < s.0 {
                size = s;
                break;
            }
            pick -= s.0;
        }
        let (_, tries, spread, rise, names) = size;
        let mut cells: Vec<(IVec3, Block)> = Vec::new();
        let mut written: FxHashMap<IVec3, Block> = FxHashMap::default();
        // The tuff scatter.
        let trapezoid = |rng: &mut Rng, half: i32| -> i32 {
            // `TrapezoidInt` with plateau 0: the sum of two uniform halves.
            let range = 2 * half;
            let end = range - range / 2;
            -half + rng.range(0, (range - end) as u32) as i32 + rng.range(0, end as u32) as i32
        };
        for _ in 0..tries {
            let dx = trapezoid(&mut rng, spread);
            let dy = trapezoid(&mut rng, rise);
            let dz = trapezoid(&mut rng, spread);
            let mut q = spring + IVec3::new(dx, dy, dz);
            let get = |q: IVec3, written: &FxHashMap<IVec3, Block>, terrain: &mut dyn FnMut(IVec3) -> Block| {
                written.get(&q).copied().unwrap_or_else(|| terrain(q))
            };
            let mut hit = None;
            for step in 0..=TUFF_SCAN {
                if get(q, &written, &mut terrain).is_solid() {
                    hit = Some(q);
                    break;
                }
                if step == TUFF_SCAN {
                    break;
                }
                q.y -= 1;
                if q.y < WORLD_MIN_Y {
                    break;
                }
            }
            if let Some(q) = hit.filter(|q| get(*q, &written, &mut terrain) != Block::BEDROCK) {
                written.insert(q, Block::TUFF);
                cells.push((q, Block::TUFF));
            }
        }
        // The template, seven down, centred and turned.
        let name = names[rng.below(names.len() as u32) as usize];
        let template = templates().iter().find(|t| t.name == name).ok_or(Reject::Template)?;
        let turns = rng.below(4) as u8;
        let corner = spring + IVec3::Y * TEMPLATE_OFFSET_Y;
        for &(local, b) in &template.cells {
            let q = placed_cell(corner, template.size, turns, local);
            if (WORLD_MIN_Y..WORLD_MAX_Y).contains(&q.y) {
                cells.push((q, b));
            }
        }
        // `placeDirt`: the rock between the lower cave and the spring.
        let mut roots = Vec::new();
        for y in origin.y..origin.y + climbed {
            for _ in 0..ROOT_ATTEMPTS {
                let r = ROOT_RADIUS as u32;
                let dx = rng.below(r) as i32 - rng.below(r) as i32;
                let dz = rng.below(r) as i32 - rng.below(r) as i32;
                roots.push(IVec3::new(origin.x + dx, y, origin.z + dz));
            }
        }
        // `placeRoots`: one try at radius and span 1, so the origin itself,
        // under a ceiling with a sturdy underside.
        let hanging = terrain(origin + IVec3::Y).is_opaque().then_some(origin);
        let mut bounds = Bounds { min: origin, max: origin };
        for p in cells.iter().map(|c| c.0).chain(roots.iter().copied()) {
            bounds.min = bounds.min.min(p);
            bounds.max = bounds.max.max(p);
        }
        Ok(Spring { origin, spring, bounds, cells: cells.into(), roots: roots.into(), hanging })
    }

    /// The springs placed from the Java chunks of one 32×32 column.
    pub fn source(&self, generator: &Generator, column: IVec2) -> Arc<Vec<Arc<Spring>>> {
        if let Some(found) = self.cache.lock().unwrap().get(&column) {
            return found.clone();
        }
        let mut springs = Vec::new();
        let per = CHUNK_SIZE_I / JAVA_CHUNK;
        for sub_z in 0..per {
            for sub_x in 0..per {
                let c = column * per + IVec2::new(sub_x, sub_z);
                for (start, rng) in self.candidates(c) {
                    if let Some(s) = self.build(generator, start, rng) {
                        springs.push(Arc::new(s));
                    }
                }
            }
        }
        let found = Arc::new(springs);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(k) = cache.keys().next().copied()
        {
            cache.remove(&k);
        }
        cache.entry(column).or_insert(found).clone()
    }

    /// Springs touching the chunk at `base`, ordered by origin.
    pub fn near(&self, generator: &Generator, base: IVec3) -> Vec<Arc<Spring>> {
        // Templates reach 8, tuff 10 and roots 2 blocks from their column.
        const REACH: i32 = 20;
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        let lo = IVec2::new(base.x - REACH, base.z - REACH).div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let hi = IVec2::new(top.x + REACH, top.z + REACH).div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let mut out = Vec::new();
        for cz in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                out.extend(
                    self.source(generator, IVec2::new(cx, cz)).iter().filter(|s| s.bounds.intersects(&chunk)).cloned(),
                );
            }
        }
        out.sort_by_key(|s| (s.origin.x, s.origin.y, s.origin.z));
        out
    }

    /// Paints the springs reaching the chunk at `base` (generation 5 only;
    /// the caller checks).
    pub fn paint(&self, generator: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        for spring in self.near(generator, base) {
            spring.paint(blocks, base);
        }
    }
}

#[cfg(test)]
mod tests;
