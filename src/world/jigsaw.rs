//! Structure templates and the jigsaw assembler (GEN-05a, v0.14).
//!
//! Java builds villages, outposts, bastions, ancient cities and trail ruins
//! from templates joined by jigsaw connectors (`JigsawPlacement`). This
//! module is that system with VoxelCraft's own template data: templates are
//! text (`src/world/jigsaw/*.txt`, format described there), each holding
//! blocks, connectors and markers; pools pick templates by weight and name a
//! fallback pool.
//!
//! [`assemble`] follows `JigsawPlacement.Placer`: a start template at a
//! random rotation sits on the surface at its centre; connectors are taken
//! breadth first (every placement priority is 0), each connector's pool is
//! shuffled by weight with the fallback pool after it (only the fallback at
//! the maximum depth), an empty element stops the connector, rotations and
//! the candidate's connectors are shuffled, and a candidate fits when a
//! connector faces back at this one, its name equals this connector's
//! target, and its box lies in free space. Free space starts as the box
//! `max_distance` around the start minus the start; a connector pointing
//! into its own piece uses that piece's interior instead, and every piece
//! keeps using the space it was placed in. Rigid pieces line connectors up
//! exactly; a terrain-following piece, or a child of one, is set from the
//! surface at the connector. Not modelled: placement and selection
//! priorities other than 0, Java's random sequence (layouts are original,
//! so seeds can't match anyway), the "expansion hack" that grows a start's
//! box, processors other than the bridge rule, and Java's beardifier, which
//! is approximated by foundations below rigid pieces and clearing above.

use std::sync::OnceLock;

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::{Block, Facing};
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::noise::hash3;
use super::structure::{Bounds, Rng};

/// How a template meets the ground.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Projection {
    /// Placed as built (houses, wells).
    Rigid,
    /// Every column follows the surface (streets).
    Terrain,
}

/// What a template cell becomes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    Block(Block),
    /// A block the structure's palette chooses, by role index.
    Role(u16),
    Door,
    BedFoot,
    BedHead,
    /// A crop at a random stage, chosen by the palette.
    Crop,
    /// The piece's job site, chosen by the palette.
    Job,
    Chest,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spec {
    pub base: Base,
    /// Which way the block faces, in template directions.
    pub facing: Option<Facing>,
    /// Stairs: `facing` is the way they climb, not their front.
    pub climb: bool,
    /// A door's top half.
    pub upper: bool,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub pos: IVec3,
    pub spec: Spec,
}

#[derive(Clone, Debug)]
pub struct Jigsaw {
    pub pos: IVec3,
    /// The way the connector faces, out of its template.
    pub facing: Facing,
    pub name: String,
    pub target: String,
    /// The pool children come from (`empty` for none).
    pub pool: String,
}

#[derive(Clone, Debug)]
pub struct Template {
    pub name: String,
    pub size: IVec3,
    pub projection: Projection,
    pub cells: Vec<Cell>,
    pub jigsaws: Vec<Jigsaw>,
    /// Beds (head cell) and where their villager spawns.
    pub homes: Vec<(IVec3, IVec3)>,
}

#[derive(Clone, Debug)]
pub struct Pool {
    /// Template indices by weight; `None` is Java's empty element.
    pub elements: Vec<(Option<u16>, u32)>,
    pub fallback: Option<String>,
}

/// Every template and pool of one data file.
#[derive(Debug, Default)]
pub struct Library {
    pub templates: Vec<Template>,
    pub pools: FxHashMap<String, Pool>,
    /// Role names used by `$role` specs, indexed by [`Base::Role`].
    pub roles: Vec<String>,
}

fn parse_facing(s: &str) -> Result<Facing, String> {
    Ok(match s {
        "north" => Facing::North,
        "south" => Facing::South,
        "east" => Facing::East,
        "west" => Facing::West,
        _ => return Err(format!("unknown direction {s:?}")),
    })
}

/// A parsed pool before its template names are resolved: name, (template,
/// weight) elements and fallback.
type PendingPool = (String, Vec<(String, u32)>, Option<String>);

impl Library {
    /// Parses a template file; errors name the line.
    pub fn parse(text: &str) -> Result<Library, String> {
        let mut lib = Library::default();
        let mut names: FxHashMap<String, u16> = FxHashMap::default();
        let mut pending_pools: Vec<PendingPool> = Vec::new();
        let mut lines = text.lines().enumerate().peekable();
        while let Some((n, line)) = lines.next() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let err = |m: String| format!("line {}: {m}", n + 1);
            let words: Vec<&str> = line.split_whitespace().collect();
            match words[0] {
                "template" => {
                    let name = words.get(1).ok_or_else(|| err("template needs a name".into()))?.to_string();
                    let t = lib.parse_template(name.clone(), &mut lines)?;
                    if names.insert(name.clone(), lib.templates.len() as u16).is_some() {
                        return Err(err(format!("duplicate template {name}")));
                    }
                    lib.templates.push(t);
                }
                "pool" => {
                    let name = words.get(1).ok_or_else(|| err("pool needs a name".into()))?.to_string();
                    let fallback = match words.get(2..) {
                        Some(["fallback", f]) => Some(f.to_string()),
                        Some([]) => None,
                        _ => return Err(err("expected `pool <name> [fallback <pool>]`".into())),
                    };
                    let mut elements = Vec::new();
                    loop {
                        let Some((n, l)) = lines.next() else { return Err(err("pool without `end`".into())) };
                        let l = l.trim();
                        if l.is_empty() || l.starts_with('#') {
                            continue;
                        }
                        if l == "end" {
                            break;
                        }
                        let mut w = l.split_whitespace();
                        let weight = w.next().and_then(|s| s.parse::<u32>().ok());
                        match (weight, w.next(), w.next()) {
                            (Some(weight), Some(t), None) if weight > 0 => elements.push((t.to_string(), weight)),
                            _ => return Err(format!("line {}: expected `<weight> <template>`", n + 1)),
                        }
                    }
                    pending_pools.push((name, elements, fallback));
                }
                other => return Err(err(format!("unexpected {other:?}"))),
            }
        }
        for (name, elements, fallback) in pending_pools {
            let elements = elements
                .into_iter()
                .map(|(t, w)| match t.as_str() {
                    "empty" => Ok((None, w)),
                    _ => names.get(&t).map(|&i| (Some(i), w)).ok_or_else(|| format!("pool {name}: no template {t}")),
                })
                .collect::<Result<_, _>>()?;
            lib.pools.insert(name, Pool { elements, fallback });
        }
        for t in &lib.templates {
            for j in &t.jigsaws {
                if j.pool != "empty" && !lib.pools.contains_key(&j.pool) {
                    return Err(format!("{}: connector pool {} is missing", t.name, j.pool));
                }
            }
        }
        for (name, pool) in &lib.pools {
            if let Some(f) = &pool.fallback
                && !lib.pools.contains_key(f)
            {
                return Err(format!("pool {name}: fallback {f} is missing"));
            }
        }
        Ok(lib)
    }

    fn spec(&mut self, text: &str) -> Result<Spec, String> {
        let (text, upper) = match text.strip_suffix("/upper") {
            Some(t) => (t, true),
            None => (text, false),
        };
        let (name, facing, climb) = if let Some((n, f)) = text.split_once('@') {
            (n, Some(parse_facing(f)?), false)
        } else if let Some((n, f)) = text.split_once('^') {
            (n, Some(parse_facing(f)?), true)
        } else {
            (text, None, false)
        };
        let base = match name {
            "door" => Base::Door,
            "bed_foot" => Base::BedFoot,
            "bed_head" => Base::BedHead,
            "crop" => Base::Crop,
            "job" => Base::Job,
            "chest" => Base::Chest,
            role if role.starts_with('$') => {
                let role = &role[1..];
                let i = match self.roles.iter().position(|r| r == role) {
                    Some(i) => i,
                    None => {
                        self.roles.push(role.to_string());
                        self.roles.len() - 1
                    }
                };
                Base::Role(i as u16)
            }
            block => Base::Block(Block::from_name(block).ok_or_else(|| format!("unknown block {block:?}"))?),
        };
        Ok(Spec { base, facing, climb, upper })
    }

    fn parse_template<'a>(
        &mut self,
        name: String,
        lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
    ) -> Result<Template, String> {
        let mut size = None;
        let mut projection = Projection::Rigid;
        let mut keys: FxHashMap<char, Option<Spec>> = FxHashMap::default();
        keys.insert('.', None);
        keys.insert('-', Some(Spec { base: Base::Block(Block::AIR), facing: None, climb: false, upper: false }));
        let mut cells = Vec::new();
        let mut jigsaws = Vec::new();
        let mut homes = Vec::new();
        let mut layer: Option<(i32, i32)> = None;
        let mut layers_seen = Vec::new();
        loop {
            let Some((n, raw)) = lines.next() else { return Err(format!("{name}: template without `end`")) };
            let err = |m: String| format!("line {} ({name}): {m}", n + 1);
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') && layer.is_none() {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            let int = |s: &str| s.parse::<i32>().map_err(|_| err(format!("not a number: {s:?}")));
            let vec3 = |w: &[&str]| -> Result<IVec3, String> { Ok(IVec3::new(int(w[0])?, int(w[1])?, int(w[2])?)) };
            match words[0] {
                "end" => break,
                "size" if words.len() == 4 => size = Some(vec3(&words[1..4])?),
                "projection" => {
                    projection = match words.get(1) {
                        Some(&"rigid") => Projection::Rigid,
                        Some(&"terrain") => Projection::Terrain,
                        _ => return Err(err("projection is rigid or terrain".into())),
                    }
                }
                "key" if words.len() == 3 && words[1].chars().count() == 1 => {
                    let c = words[1].chars().next().unwrap();
                    let spec = self.spec(words[2]).map_err(&err)?;
                    if keys.insert(c, Some(spec)).is_some() {
                        return Err(err(format!("key {c:?} defined twice")));
                    }
                }
                "layer" if words.len() == 2 => {
                    let y = int(words[1])?;
                    if layers_seen.contains(&y) {
                        return Err(err(format!("layer {y} twice")));
                    }
                    layers_seen.push(y);
                    layer = Some((y, 0));
                }
                "jigsaw" if words.len() == 8 => jigsaws.push(Jigsaw {
                    pos: vec3(&words[1..4])?,
                    facing: parse_facing(words[4]).map_err(&err)?,
                    name: words[5].to_string(),
                    target: words[6].to_string(),
                    pool: words[7].to_string(),
                }),
                "home" if words.len() == 7 => homes.push((vec3(&words[1..4])?, vec3(&words[4..7])?)),
                _ if layer.is_some() && words.len() == 1 => {
                    let size = size.ok_or_else(|| err("`size` must come first".into()))?;
                    let (y, z) = layer.unwrap();
                    if z >= size.z || line.chars().count() != size.x as usize {
                        return Err(err(format!("row {z} of layer {y} doesn't fit size {size}")));
                    }
                    for (x, c) in line.chars().enumerate() {
                        let spec = keys.get(&c).ok_or_else(|| err(format!("no key for {c:?}")))?;
                        if let Some(spec) = spec {
                            cells.push(Cell { pos: IVec3::new(x as i32, y, z), spec: *spec });
                        }
                    }
                    layer = Some((y, z + 1));
                }
                _ => return Err(err(format!("unexpected {line:?}"))),
            }
        }
        let size = size.ok_or_else(|| format!("{name}: no size"))?;
        let inside = |p: IVec3| p.cmpge(IVec3::ZERO).all() && p.cmplt(size).all();
        for j in &jigsaws {
            if !inside(j.pos) {
                return Err(format!("{name}: connector {} outside the template", j.pos));
            }
        }
        for &(bed, spawn) in &homes {
            if !inside(bed) || !inside(spawn) {
                return Err(format!("{name}: home outside the template"));
            }
        }
        if layers_seen.iter().any(|&y| y < 0 || y >= size.y) {
            return Err(format!("{name}: layer outside the size"));
        }
        Ok(Template { name, size, projection, cells, jigsaws, homes })
    }

    pub fn template(&self, name: &str) -> Option<&Template> {
        self.templates.iter().find(|t| t.name == name)
    }
}

/// The village templates and pools, parsed once.
pub fn villages() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/village.txt")).expect("village templates"))
}

/// `v` turned `r` quarter turns clockwise seen from above, like Java's
/// `Rotation` about the template origin.
pub fn rotate(v: IVec3, r: u8) -> IVec3 {
    match r & 3 {
        0 => v,
        1 => IVec3::new(-v.z, v.y, v.x),
        2 => IVec3::new(-v.x, v.y, -v.z),
        _ => IVec3::new(v.z, v.y, -v.x),
    }
}

pub fn rotate_facing(f: Facing, r: u8) -> Facing {
    (0..r & 3).fold(f, |f, _| f.clockwise())
}

/// The world box of a template placed at `origin` turned `r` times.
fn bounds_of(t: &Template, origin: IVec3, r: u8) -> Bounds {
    let a = rotate(IVec3::ZERO, r);
    let b = rotate(t.size - IVec3::ONE, r);
    Bounds { min: origin + a.min(b), max: origin + a.max(b) }
}

/// One template in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub template: u16,
    pub origin: IVec3,
    pub rotation: u8,
    pub bounds: Bounds,
    pub depth: u8,
    /// Seeds the piece's random choices (job site, crops, chest loot).
    pub seed: u64,
}

impl Placed {
    /// World position of template cell `p` (rigid pieces).
    pub fn world(&self, p: IVec3) -> IVec3 {
        self.origin + rotate(p, self.rotation)
    }
}

/// Free space: a box minus the boxes placed in it (Java keeps a voxel
/// shape; a box fits when it lies in the container and meets no piece).
struct Free {
    container: Bounds,
    taken: Vec<Bounds>,
}

impl Free {
    fn fits(&self, b: &Bounds) -> bool {
        b.min.cmpge(self.container.min).all()
            && b.max.cmple(self.container.max).all()
            && !self.taken.iter().any(|t| t.intersects(b))
    }
}

fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        v.swap(i, rng.below(i as u32 + 1) as usize);
    }
}

/// A pool's elements repeated by weight and shuffled (Java's
/// `getShuffledTemplates`).
fn shuffled(pool: &Pool, rng: &mut Rng) -> Vec<Option<u16>> {
    let mut out: Vec<_> = pool.elements.iter().flat_map(|&(t, w)| std::iter::repeat_n(t, w as usize)).collect();
    shuffle(&mut out, rng);
    out
}

/// Settings of one jigsaw structure.
pub struct Settings<'a> {
    pub start_pool: &'a str,
    pub max_depth: u8,
    pub max_distance: i32,
}

/// Builds a structure from `settings.start_pool` around column `at`.
/// `surface(x, z)` is the y of the top block there (water included).
pub fn assemble(
    lib: &Library,
    settings: &Settings,
    at: IVec2,
    surface: &dyn Fn(i32, i32) -> i32,
    seed: u64,
) -> Vec<Placed> {
    let mut rng = Rng(seed);
    let Some(start_pool) = lib.pools.get(settings.start_pool) else { return Vec::new() };
    let start = pick(start_pool, &mut rng);
    let Some(start) = start else { return Vec::new() };
    let rotation = rng.below(4) as u8;
    let t = &lib.templates[start as usize];
    let mut origin = IVec3::new(at.x, 0, at.y);
    let b = bounds_of(t, origin, rotation);
    let centre = (b.min + b.max) / 2;
    // Layer 0 replaces the top block, like Java's ground-level delta of 1.
    let ground = surface(centre.x, centre.z);
    origin.y = ground;
    let bounds = bounds_of(t, origin, rotation);
    let d = settings.max_distance;
    let container = Bounds {
        min: IVec3::new(centre.x - d, ground - d, centre.z - d),
        max: IVec3::new(centre.x + d, ground + d, centre.z + d),
    };
    let mut frees = vec![Free { container, taken: vec![bounds] }];
    let mut pieces = vec![Placed { template: start, origin, rotation, bounds, depth: 0, seed: rng.next_u64() }];
    // Each piece's free space, and its own interior once something is put in it.
    let mut space = vec![0usize];
    let mut inner: Vec<Option<usize>> = vec![None];
    let mut queue = std::collections::VecDeque::from([0usize]);
    while let Some(i) = queue.pop_front() {
        let piece = pieces[i];
        let t = &lib.templates[piece.template as usize];
        let mut order: Vec<usize> = (0..t.jigsaws.len()).collect();
        shuffle(&mut order, &mut rng);
        'connectors: for j in order {
            // Java reads the surface at most once per connector.
            let mut ground_at = None;
            let jig = &t.jigsaws[j];
            let Some(pool) = lib.pools.get(&jig.pool) else { continue };
            let p = piece.world(jig.pos);
            let facing = rotate_facing(jig.facing, piece.rotation);
            let target = p + facing.offset();
            let free = if piece.bounds.contains(target) {
                *inner[i].get_or_insert_with(|| {
                    frees.push(Free { container: piece.bounds, taken: Vec::new() });
                    frees.len() - 1
                })
            } else {
                space[i]
            };
            let mut candidates = if piece.depth != settings.max_depth { shuffled(pool, &mut rng) } else { Vec::new() };
            if let Some(fallback) = pool.fallback.as_ref().and_then(|f| lib.pools.get(f)) {
                candidates.extend(shuffled(fallback, &mut rng));
            }
            let rigid = t.projection == Projection::Rigid;
            for candidate in candidates {
                let Some(c) = candidate else { break };
                let ct = &lib.templates[c as usize];
                let mut rotations = [0u8, 1, 2, 3];
                shuffle(&mut rotations, &mut rng);
                for r in rotations {
                    let mut joints: Vec<usize> = (0..ct.jigsaws.len()).collect();
                    shuffle(&mut joints, &mut rng);
                    for k in joints {
                        let cj = &ct.jigsaws[k];
                        if rotate_facing(cj.facing, r) != facing.opposite() || cj.name != jig.target {
                            continue;
                        }
                        let local = rotate(cj.pos, r);
                        let mut origin = target - local;
                        let y = if rigid && ct.projection == Projection::Rigid {
                            p.y - local.y
                        } else {
                            *ground_at.get_or_insert_with(|| surface(p.x, p.z) + 1) - local.y
                        };
                        origin.y = y;
                        let bounds = bounds_of(ct, origin, r);
                        if !frees[free].fits(&bounds) {
                            continue;
                        }
                        frees[free].taken.push(bounds);
                        let depth = piece.depth + 1;
                        pieces.push(Placed { template: c, origin, rotation: r, bounds, depth, seed: rng.next_u64() });
                        space.push(free);
                        inner.push(None);
                        if depth <= settings.max_depth {
                            queue.push_back(pieces.len() - 1);
                        }
                        continue 'connectors;
                    }
                }
            }
        }
    }
    pieces
}

/// A weighted pick from `pool` (the start piece).
fn pick(pool: &Pool, rng: &mut Rng) -> Option<u16> {
    let total: u32 = pool.elements.iter().map(|e| e.1).sum();
    let mut roll = rng.below(total.max(1));
    for &(t, w) in &pool.elements {
        if roll < w {
            return t;
        }
        roll -= w;
    }
    None
}

/// What a structure's blocks are made of.
pub trait Palette {
    /// The block for a `$role` (`None` leaves the world as it is).
    fn role(&self, role: &str) -> Option<Block>;
    /// The job site of a piece.
    fn job(&self, piece: &Placed) -> Block;
    /// A crop for a cell, from a per-cell hash.
    fn crop(&self, hash: u64) -> Block;
    /// A terrain-following block placed on water (Java's bridge processor).
    fn on_water(&self, block: Block) -> Block {
        block
    }
}

/// The ground a structure is painted onto.
pub struct Ground<'a> {
    /// Top block y, water included.
    pub surface: &'a dyn Fn(i32, i32) -> i32,
    /// Top solid block y (the bottom of any water).
    pub solid: &'a dyn Fn(i32, i32) -> i32,
}

/// How far above a rigid piece terrain and leaves are cleared.
const CLEAR_ABOVE: i32 = 30;

fn resolve(spec: &Spec, roles: &[Option<Block>], palette: &dyn Palette, piece: &Placed, pos: IVec3) -> Option<Block> {
    let face = |f: Facing| rotate_facing(f, piece.rotation);
    let block = match spec.base {
        Base::Block(b) => b,
        Base::Role(r) => roles[r as usize]?,
        Base::Door => return Some(Block::door(face(spec.facing.unwrap_or(Facing::North)), false, spec.upper)),
        Base::BedFoot => Block::BED_FOOT,
        Base::BedHead => Block::BED_HEAD,
        Base::Crop => palette.crop(hash3(pos.x, pos.y, pos.z, piece.seed)),
        Base::Job => palette.job(piece),
        Base::Chest => Block::CHEST,
    };
    Some(match spec.facing {
        Some(f) if spec.climb => block.with_facing(face(f).opposite()),
        Some(f) => block.with_facing(face(f)),
        None => block,
    })
}

fn chunk_index(p: IVec3, base: IVec3) -> Option<usize> {
    let l = p - base;
    (l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all())
        .then(|| index(l.x as usize, l.y as usize, l.z as usize))
}

fn meets_columns(b: &Bounds, base: IVec3) -> bool {
    b.min.x < base.x + CHUNK_SIZE_I && b.max.x >= base.x && b.min.z < base.z + CHUNK_SIZE_I && b.max.z >= base.z
}

/// Paints the part of `pieces` inside the chunk at `base`. Every chunk
/// computes its blocks from the layout and the ground alone, so chunks
/// agree whatever order they generate in.
pub fn paint(
    lib: &Library,
    pieces: &[Placed],
    palette: &dyn Palette,
    ground: &Ground,
    blocks: &mut [Block; CHUNK_VOLUME],
    base: IVec3,
) {
    let roles: Vec<Option<Block>> = lib.roles.iter().map(|r| palette.role(r)).collect();
    let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
    let near: Vec<&Placed> = pieces.iter().filter(|p| meets_columns(&p.bounds, base)).collect();
    // Clear above rigid pieces first so later pieces aren't cut away.
    for p in &near {
        if lib.templates[p.template as usize].projection != Projection::Rigid {
            continue;
        }
        let lo = (p.bounds.max.y + 1).max(base.y);
        let hi = (p.bounds.min.y + CLEAR_ABOVE).min(top.y);
        for x in p.bounds.min.x.max(base.x)..=p.bounds.max.x.min(top.x) {
            for z in p.bounds.min.z.max(base.z)..=p.bounds.max.z.min(top.z) {
                for y in lo..=hi {
                    blocks[chunk_index(IVec3::new(x, y, z), base).unwrap()] = Block::AIR;
                }
            }
        }
    }
    // Terrain pieces first: following the ground can take a street outside
    // the box its placement was checked with, and it must not cut through
    // a building's chest, bed or job site.
    let terrain_first = near.iter().filter(|p| lib.templates[p.template as usize].projection == Projection::Terrain);
    let rigid = near.iter().filter(|p| lib.templates[p.template as usize].projection == Projection::Rigid);
    for p in terrain_first.chain(rigid) {
        let t = &lib.templates[p.template as usize];
        for cell in &t.cells {
            let mut at = p.world(cell.pos);
            if at.x < base.x || at.x > top.x || at.z < base.z || at.z > top.z {
                continue;
            }
            let mut wet = false;
            if t.projection == Projection::Terrain {
                let surface = (ground.surface)(at.x, at.z);
                wet = cell.pos.y == 0 && (ground.solid)(at.x, at.z) < surface;
                at.y = surface + cell.pos.y;
            }
            let Some(i) = chunk_index(at, base) else { continue };
            if let Some(mut b) = resolve(&cell.spec, &roles, palette, p, cell.pos) {
                if wet {
                    b = palette.on_water(b);
                }
                blocks[i] = b;
            }
        }
    }
    // Foundations: pillars under a rigid piece's bottom layer, down
    // through air and water to the ground (Java's beardifier fills the
    // terrain instead).
    let Some(foundation) = palette.role("foundation") else { return };
    for p in &near {
        let t = &lib.templates[p.template as usize];
        if t.projection != Projection::Rigid {
            continue;
        }
        for cell in t.cells.iter().filter(|c| c.pos.y == 0) {
            let block = resolve(&cell.spec, &roles, palette, p, cell.pos);
            if !block.is_some_and(|b| b != Block::AIR && !b.is_fluid()) {
                continue;
            }
            let at = p.world(cell.pos);
            if at.x < base.x || at.x > top.x || at.z < base.z || at.z > top.z || at.y - 1 < base.y {
                continue;
            }
            let mut y = at.y - 1;
            if y > top.y {
                // Only reach into this chunk if the ground is below it.
                if (ground.solid)(at.x, at.z) > top.y {
                    continue;
                }
                y = top.y;
            }
            while y > 1 && y >= base.y {
                let i = chunk_index(IVec3::new(at.x, y, at.z), base).unwrap();
                if blocks[i] != Block::AIR && !blocks[i].is_fluid() {
                    break;
                }
                blocks[i] = foundation;
                y -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_turn_clockwise_like_java() {
        // Java CLOCKWISE_90 maps (x, z) to (-z, x) and north to east.
        assert_eq!(rotate(IVec3::new(1, 2, 3), 1), IVec3::new(-3, 2, 1));
        assert_eq!(rotate(IVec3::new(1, 2, 3), 2), IVec3::new(-1, 2, -3));
        assert_eq!(rotate(IVec3::new(1, 2, 3), 3), IVec3::new(3, 2, -1));
        assert_eq!(rotate_facing(Facing::North, 1), Facing::East);
        for f in Facing::ALL {
            for r in 0..4 {
                let turned = rotate(f.offset(), r);
                assert_eq!(rotate_facing(f, r).offset(), turned, "{f:?} {r}");
            }
        }
    }

    #[test]
    fn the_village_file_parses_and_every_connector_can_attach() {
        let lib = villages();
        assert!(lib.templates.len() >= 12);
        for t in &lib.templates {
            for j in &t.jigsaws {
                // A connector must sit on the template's edge facing out.
                let out = j.pos + j.facing.offset();
                let inside = out.cmpge(IVec3::ZERO).all() && out.cmplt(t.size).all();
                assert!(!inside, "{}: connector {} faces into the template", t.name, j.pos);
                if j.pool == "empty" {
                    continue;
                }
                // Some element of its pool has a connector named after its target.
                let pool = &lib.pools[&j.pool];
                assert!(
                    pool.elements
                        .iter()
                        .filter_map(|e| e.0)
                        .any(|c| { lib.templates[c as usize].jigsaws.iter().any(|cj| cj.name == j.target) }),
                    "{}: nothing in {} answers {}",
                    t.name,
                    j.pool,
                    j.target
                );
            }
        }
    }

    #[test]
    fn parse_errors_name_the_line() {
        let e = Library::parse("template a\nsize 1 1 1\nlayer 0\nZ\nend\n").unwrap_err();
        assert!(e.contains("line 4"), "{e}");
        let e = Library::parse("pool p\n1 nothing\nend\n").unwrap_err();
        assert!(e.contains("no template nothing"), "{e}");
        let e = Library::parse("template a\nsize 2 1 1\nlayer 0\nx\nend\n").unwrap_err();
        assert!(e.contains("doesn't fit"), "{e}");
    }

    fn flat(_: i32, _: i32) -> i32 {
        64
    }

    #[test]
    fn assembly_is_deterministic_and_pieces_never_overlap() {
        let lib = villages();
        let settings = Settings { start_pool: "village/town_centers", max_depth: 6, max_distance: 80 };
        let hilly = |x: i32, z: i32| 64 + ((x / 7 + z / 5) % 5);
        for seed in 0..40u64 {
            for surface in [&flat as &dyn Fn(i32, i32) -> i32, &hilly] {
                let a = assemble(lib, &settings, IVec2::new(100, -300), surface, seed);
                let b = assemble(lib, &settings, IVec2::new(100, -300), surface, seed);
                assert_eq!(a, b);
                assert!(a.len() > 1, "seed {seed}: only the start");
                for (i, p) in a.iter().enumerate() {
                    assert!(p.depth <= settings.max_depth + 1);
                    assert!((p.bounds.min.x - 100).abs() <= 95 && (p.bounds.max.z + 300).abs() <= 95);
                    for q in &a[i + 1..] {
                        // Pieces in the same free space never share a block.
                        assert!(!p.bounds.intersects(&q.bounds), "seed {seed}: {p:?} {q:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn connected_pieces_meet_at_their_connectors() {
        let lib = villages();
        let settings = Settings { start_pool: "village/town_centers", max_depth: 6, max_distance: 80 };
        let pieces = assemble(lib, &settings, IVec2::ZERO, &flat, 7);
        // Every piece but the start has a connector facing back into a
        // connector of an earlier piece.
        for (i, p) in pieces.iter().enumerate().skip(1) {
            let t = &lib.templates[p.template as usize];
            let joined = t.jigsaws.iter().any(|j| {
                let at = p.world(j.pos);
                let out = rotate_facing(j.facing, p.rotation);
                pieces[..i].iter().any(|q| {
                    lib.templates[q.template as usize].jigsaws.iter().any(|k| {
                        let mut there = q.world(k.pos);
                        let mut here = at;
                        // Terrain pieces' y comes from the ground; compare columns.
                        there.y = 0;
                        here.y = 0;
                        there + rotate_facing(k.facing, q.rotation).offset() == here
                            && rotate_facing(k.facing, q.rotation) == out.opposite()
                            && k.target == j.name
                    })
                })
            });
            assert!(joined, "piece {i} ({}) isn't joined", t.name);
        }
    }

    #[test]
    fn the_last_depth_only_takes_fallbacks() {
        let lib = villages();
        let settings = Settings { start_pool: "village/town_centers", max_depth: 1, max_distance: 80 };
        for seed in 0..20 {
            let pieces = assemble(lib, &settings, IVec2::ZERO, &flat, seed);
            for p in pieces.iter().filter(|p| p.depth == 2) {
                let name = &lib.templates[p.template as usize].name;
                assert!(name == "village/street_end", "seed {seed}: {name} past the last depth");
            }
        }
    }
}
