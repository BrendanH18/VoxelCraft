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
//!
//! Besides the format documented in `jigsaw/village.txt`, templates take
//! `fill x0 y0 z0 x1 y1 z1 <spec>` (an inclusive box of a block, role or
//! crop, painted before the layers so layer cells override it),
//! `cell x y z <spec>` or `put x y z <spec>` (one cell of any spec, painted with the layers),
//! `chest:<table>@dir` (a chest filled from a named loot table),
//! `spawner:<mob>` (a mob spawner), `mob x y z <name>` (a resident placed
//! with the structure, see [`markers`]) and connectors facing `up` or
//! `down`: a vertical connector meets one facing the other way, at any
//! rotation. [`Palette::process`] is the structure-processor hook and
//! [`Palette::clear_above`] turns clearing off for buried structures;
//! [`paint_canvas`] paints into a chunk or a single column.

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
    /// A chest filled from a named loot table ([`Library::loot`]).
    Loot(u16),
    /// A mob spawner of a named mob ([`Library::mobs`]).
    Spawner(u16),
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
    /// The way the connector faces, out of its template (unused when
    /// [`Jigsaw::vertical`] is set).
    pub facing: Facing,
    /// `1` for a connector facing up, `-1` facing down, `0` horizontal.
    pub vertical: i8,
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
    /// Boxes painted before the cells: inclusive corners and what fills them.
    pub fills: Vec<(IVec3, IVec3, Spec)>,
    /// Resident mobs placed with the structure, by name.
    pub mobs: Vec<(IVec3, String)>,
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
    /// Loot table names used by `chest:<table>` specs ([`Base::Loot`]).
    pub loot: Vec<String>,
    /// Mob names used by `spawner:<mob>` specs ([`Base::Spawner`]).
    pub mobs: Vec<String>,
}

fn intern(names: &mut Vec<String>, name: &str) -> u16 {
    match names.iter().position(|r| r == name) {
        Some(i) => i as u16,
        None => {
            names.push(name.to_string());
            names.len() as u16 - 1
        }
    }
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
            role if role.starts_with('$') => Base::Role(intern(&mut self.roles, &role[1..])),
            loot if loot.starts_with("chest:") => Base::Loot(intern(&mut self.loot, &loot[6..])),
            mob if mob.starts_with("spawner:") => Base::Spawner(intern(&mut self.mobs, &mob[8..])),
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
        let mut fills = Vec::new();
        let mut mobs = Vec::new();
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
                    let spec = self.spec(words[2]).map_err(err)?;
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
                "jigsaw" if words.len() == 8 => {
                    let (facing, vertical) = match words[4] {
                        "up" => (Facing::North, 1),
                        "down" => (Facing::North, -1),
                        f => (parse_facing(f).map_err(err)?, 0),
                    };
                    jigsaws.push(Jigsaw {
                        pos: vec3(&words[1..4])?,
                        facing,
                        vertical,
                        name: words[5].to_string(),
                        target: words[6].to_string(),
                        pool: words[7].to_string(),
                    })
                }
                "home" if words.len() == 7 => homes.push((vec3(&words[1..4])?, vec3(&words[4..7])?)),
                "fill" if words.len() == 8 => {
                    let (a, b) = (vec3(&words[1..4])?, vec3(&words[4..7])?);
                    let spec = self.spec(words[7]).map_err(err)?;
                    if !matches!(spec.base, Base::Block(_) | Base::Role(_) | Base::Crop) {
                        return Err(err("fills take blocks, roles or crops".into()));
                    }
                    fills.push((a.min(b), a.max(b), spec));
                }
                "mob" if words.len() == 5 => mobs.push((vec3(&words[1..4])?, words[4].to_string())),
                // One cell of any spec (chests and spawners included), without a layer grid.
                "cell" | "put" if words.len() == 5 => {
                    let spec = self.spec(words[4]).map_err(err)?;
                    cells.push(Cell { pos: vec3(&words[1..4])?, spec });
                }
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
        if fills.iter().any(|&(a, b, _)| !inside(a) || !inside(b)) {
            return Err(format!("{name}: fill outside the template"));
        }
        if mobs.iter().any(|(p, _)| !inside(*p)) {
            return Err(format!("{name}: mob outside the template"));
        }
        if cells.iter().any(|c| !inside(c.pos)) {
            return Err(format!("{name}: cell outside the template"));
        }
        if layers_seen.iter().any(|&y| y < 0 || y >= size.y) {
            return Err(format!("{name}: layer outside the size"));
        }
        Ok(Template { name, size, projection, cells, jigsaws, homes, fills, mobs })
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

/// Pillager outpost templates and pools (v0.14).
pub fn outposts() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/outpost.txt")).expect("outpost templates"))
}

/// Bastion remnant templates and pools (v0.14).
pub fn bastions() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/bastion.txt")).expect("bastion templates"))
}

/// Trail ruin templates and pools (v0.14).
pub fn trail_ruins() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/trail_ruins.txt")).expect("trail ruin templates"))
}

/// Single templates placed without connectors: fossils, Nether fossils
/// and desert wells (v0.14).
pub fn relics() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/relics.txt")).expect("relic templates"))
}

/// Ruined portal templates, placed without connectors (v0.14).
pub fn ruined_portals() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/ruined_portals.txt")).expect("ruined portal templates"))
}

/// Ancient city templates and pools (v0.16).
pub fn ancient_cities() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/ancient_city.txt")).expect("ancient city templates"))
}

/// Trial chamber templates and pools (v0.16 Track C).
pub fn trial_chambers() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| Library::parse(include_str!("jigsaw/trial_chambers.txt")).expect("trial chamber templates"))
}

/// Every template library, for tests that check them all.
pub fn all() -> [(&'static str, &'static Library); 8] {
    [
        ("ancient_city", ancient_cities()),
        ("village", villages()),
        ("outpost", outposts()),
        ("bastion", bastions()),
        ("trail_ruins", trail_ruins()),
        ("relics", relics()),
        ("ruined_portals", ruined_portals()),
        ("trial_chambers", trial_chambers()),
    ]
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
    let place = |t: &Template, rotation: u8| {
        let mut origin = IVec3::new(at.x, 0, at.y);
        let b = bounds_of(t, origin, rotation);
        let centre = (b.min + b.max) / 2;
        // Layer 0 replaces the top block, like Java's ground-level delta of 1.
        origin.y = surface(centre.x, centre.z);
        origin
    };
    assemble_with(lib, settings, &place, surface, seed)
}

/// [`assemble`] with the start piece's origin chosen by `place` from the
/// picked start template and rotation (Java's `start_jigsaw_name` puts a
/// named point of the start at the structure's start position; see
/// `ancient_city`). The random draws are those of [`assemble`].
pub fn assemble_with(
    lib: &Library,
    settings: &Settings,
    place: &dyn Fn(&Template, u8) -> IVec3,
    surface: &dyn Fn(i32, i32) -> i32,
    seed: u64,
) -> Vec<Placed> {
    let mut rng = Rng(seed);
    let Some(start_pool) = lib.pools.get(settings.start_pool) else { return Vec::new() };
    let start = pick(start_pool, &mut rng);
    let Some(start) = start else { return Vec::new() };
    let rotation = rng.below(4) as u8;
    let t = &lib.templates[start as usize];
    let origin = place(t, rotation);
    let bounds = bounds_of(t, origin, rotation);
    let centre = (bounds.min + bounds.max) / 2;
    let ground = origin.y;
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
            let target = p + jig_offset(jig, piece.rotation);
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
                        let faces_back = match jig.vertical {
                            0 => cj.vertical == 0 && rotate_facing(cj.facing, r) == facing.opposite(),
                            v => cj.vertical == -v,
                        };
                        if !faces_back || cj.name != jig.target {
                            continue;
                        }
                        let local = rotate(cj.pos, r);
                        let mut origin = target - local;
                        // A vertical join sits on/below its parent, including
                        // terrain-projected plates. Projecting again would move
                        // the child into the parent's already occupied box.
                        let y = if jig.vertical != 0 || rigid && ct.projection == Projection::Rigid {
                            target.y - local.y
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

/// Where a connector of a piece turned `rotation` times leads, from its cell.
pub fn jig_offset(j: &Jigsaw, rotation: u8) -> IVec3 {
    match j.vertical {
        0 => rotate_facing(j.facing, rotation).offset(),
        v => IVec3::Y * v as i32,
    }
}

/// One template placed on its own (ruined portals, fossils, wells): turned
/// `rotation` times about its origin corner at `origin`.
pub fn single(lib: &Library, name: &str, origin: IVec3, rotation: u8, seed: u64) -> Placed {
    let template = lib.templates.iter().position(|t| t.name == name).unwrap_or_else(|| panic!("no template {name}"));
    let bounds = bounds_of(&lib.templates[template], origin, rotation);
    Placed { template: template as u16, origin, rotation, bounds, depth: 0, seed }
}

/// The world box a template would take at `origin`, turned `rotation` times.
pub fn bounds_at(t: &Template, origin: IVec3, rotation: u8) -> Bounds {
    bounds_of(t, origin, rotation)
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
    fn job(&self, _piece: &Placed) -> Block {
        Block::AIR
    }
    /// A crop for a cell, from a per-cell hash.
    fn crop(&self, _hash: u64) -> Block {
        Block::AIR
    }
    /// A terrain-following block placed on water (Java's bridge processor).
    fn on_water(&self, block: Block) -> Block {
        block
    }
    /// A structure processor: the block placed at `at` (`None` keeps the
    /// world's block), after the template and roles chose `block`.
    fn process(&self, block: Block, _at: IVec3, _piece: &Placed) -> Option<Block> {
        Some(block)
    }
    /// How far above a rigid piece's floor terrain is cleared (0: buried
    /// structures keep the ground around them).
    fn clear_above(&self) -> i32 {
        CLEAR_ABOVE
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

pub fn resolve(
    spec: &Spec,
    roles: &[Option<Block>],
    palette: &dyn Palette,
    piece: &Placed,
    pos: IVec3,
) -> Option<Block> {
    let face = |f: Facing| rotate_facing(f, piece.rotation);
    let block = match spec.base {
        Base::Block(b) => b,
        Base::Role(r) => roles[r as usize]?,
        Base::Door => return Some(Block::door(face(spec.facing.unwrap_or(Facing::North)), false, spec.upper)),
        Base::BedFoot => Block::BED_FOOT,
        Base::BedHead => Block::BED_HEAD,
        Base::Crop => palette.crop(hash3(pos.x, pos.y, pos.z, piece.seed)),
        Base::Job => palette.job(piece),
        Base::Chest | Base::Loot(_) => Block::CHEST,
        Base::Spawner(_) => Block::SPAWNER,
    };
    Some(match spec.facing {
        Some(f) if spec.climb => block.with_facing(face(f).opposite()),
        Some(f) => block.with_facing(face(f)),
        None => block,
    })
}

/// Java's position-seeded random used by structure rule/rot processors
/// (`Mth.getSeed` and `StructurePlaceSettings.getRandom`).
pub fn processor_random(at: IVec3) -> crate::enchant::JavaRandom {
    let seed = i64::from(at.x.wrapping_mul(3_129_871)) ^ i64::from(at.z).wrapping_mul(116_129_781) ^ i64::from(at.y);
    let seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861).wrapping_add(seed.wrapping_mul(11)) >> 16;
    crate::enchant::JavaRandom::new(seed)
}

/// A box of the world being generated: a chunk, or one column of a chunk
/// (the Nether's exposure checks).
pub struct Canvas<'a> {
    pub blocks: &'a mut [Block],
    pub base: IVec3,
    pub top: IVec3,
}

impl Canvas<'_> {
    fn index(&self, p: IVec3) -> Option<usize> {
        if p.cmplt(self.base).any() || p.cmpgt(self.top).any() {
            return None;
        }
        let l = p - self.base;
        Some(if self.blocks.len() == CHUNK_VOLUME {
            index(l.x as usize, l.y as usize, l.z as usize)
        } else {
            l.y as usize
        })
    }
}

fn meets_columns(b: &Bounds, base: IVec3, top: IVec3) -> bool {
    b.min.x <= top.x && b.max.x >= base.x && b.min.z <= top.z && b.max.z >= base.z
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
    let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
    paint_canvas(lib, pieces, palette, ground, &mut Canvas { blocks, base, top });
}

/// [`paint`] into any [`Canvas`].
pub fn paint_canvas(lib: &Library, pieces: &[Placed], palette: &dyn Palette, ground: &Ground, canvas: &mut Canvas) {
    let (base, top) = (canvas.base, canvas.top);
    let roles: Vec<Option<Block>> = lib.roles.iter().map(|r| palette.role(r)).collect();
    let near: Vec<&Placed> = pieces.iter().filter(|p| meets_columns(&p.bounds, base, top)).collect();
    let clear = palette.clear_above();
    // Clear above rigid pieces first so later pieces aren't cut away.
    for p in &near {
        if clear == 0 || lib.templates[p.template as usize].projection != Projection::Rigid {
            continue;
        }
        let lo = (p.bounds.max.y + 1).max(base.y);
        let hi = (p.bounds.min.y + clear).min(top.y);
        for x in p.bounds.min.x.max(base.x)..=p.bounds.max.x.min(top.x) {
            for z in p.bounds.min.z.max(base.z)..=p.bounds.max.z.min(top.z) {
                for y in lo..=hi {
                    let i = canvas.index(IVec3::new(x, y, z)).unwrap();
                    canvas.blocks[i] = Block::AIR;
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
        let terrain = t.projection == Projection::Terrain;
        let put = |canvas: &mut Canvas, cell: IVec3, spec: &Spec| {
            let mut at = p.world(cell);
            if at.x < base.x || at.x > top.x || at.z < base.z || at.z > top.z {
                return;
            }
            let mut wet = false;
            if terrain {
                let surface = (ground.surface)(at.x, at.z);
                wet = cell.y == 0 && (ground.solid)(at.x, at.z) < surface;
                at.y = surface + cell.y;
            }
            let Some(i) = canvas.index(at) else { return };
            if let Some(mut b) = resolve(spec, &roles, palette, p, cell) {
                if wet {
                    b = palette.on_water(b);
                }
                if let Some(b) = palette.process(b, at, p) {
                    canvas.blocks[i] = b;
                }
            }
        };
        for (a, b, spec) in &t.fills {
            // Only the part of the box in this canvas, in template cells.
            let (wa, wb) = (p.world(*a), p.world(*b));
            let (lo, hi) = (wa.min(wb), wa.max(wb));
            let (lo_y, hi_y) = if terrain { (lo.y, hi.y) } else { (lo.y.max(base.y), hi.y.min(top.y)) };
            if lo_y > hi_y || hi.x < base.x || lo.x > top.x || hi.z < base.z || lo.z > top.z {
                continue;
            }
            for x in lo.x.max(base.x)..=hi.x.min(top.x) {
                for z in lo.z.max(base.z)..=hi.z.min(top.z) {
                    for y in lo_y..=hi_y {
                        let cell = unrotate(IVec3::new(x, y, z) - p.origin, p.rotation);
                        put(canvas, cell, spec);
                    }
                }
            }
        }
        for cell in &t.cells {
            put(canvas, cell.pos, &cell.spec);
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
        let bottom = t.cells.iter().filter(|c| c.pos.y == 0).map(|c| (c.pos, c.spec));
        let filled = t.fills.iter().filter(|f| f.0.y == 0).flat_map(|&(a, b, spec)| {
            (a.x..=b.x).flat_map(move |x| (a.z..=b.z).map(move |z| (IVec3::new(x, 0, z), spec)))
        });
        for (cell, spec) in filled.chain(bottom) {
            let block = resolve(&spec, &roles, palette, p, cell);
            if !block.is_some_and(|b| b != Block::AIR && !b.is_fluid()) {
                continue;
            }
            let at = p.world(cell);
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
                let i = canvas.index(IVec3::new(at.x, y, at.z)).unwrap();
                if canvas.blocks[i] != Block::AIR && !canvas.blocks[i].is_fluid() {
                    break;
                }
                canvas.blocks[i] = foundation;
                y -= 1;
            }
        }
    }
}

/// The template cell at offset `v` from a piece's origin (the inverse of
/// [`rotate`]).
fn unrotate(v: IVec3, r: u8) -> IVec3 {
    rotate(v, (4 - (r & 3)) & 3)
}

/// A chest, spawner or resident of a placed template, in the world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Marker<'a> {
    /// A chest and the name of its loot table; the seed is per chest.
    Loot(IVec3, &'a str, u64),
    Spawner(IVec3, &'a str),
    Mob(IVec3, &'a str),
}

/// Every [`Marker`] of `pieces`. Terrain pieces' markers sit on the
/// surface given by `surface`.
pub fn markers<'a>(lib: &'a Library, pieces: &'a [Placed], surface: &dyn Fn(i32, i32) -> i32) -> Vec<Marker<'a>> {
    let mut out = Vec::new();
    for p in pieces {
        let t = &lib.templates[p.template as usize];
        let at = |c: IVec3| {
            let mut w = p.world(c);
            if t.projection == Projection::Terrain {
                w.y = surface(w.x, w.z) + c.y;
            }
            w
        };
        for c in &t.cells {
            match c.spec.base {
                Base::Loot(i) => {
                    let w = at(c.pos);
                    out.push(Marker::Loot(w, &lib.loot[i as usize], hash3(w.x, w.y, w.z, p.seed)));
                }
                Base::Spawner(i) => out.push(Marker::Spawner(at(c.pos), &lib.mobs[i as usize])),
                _ => {}
            }
        }
        for (c, name) in &t.mobs {
            out.push(Marker::Mob(at(*c), name));
        }
    }
    out
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
        assert!(villages().templates.len() >= 12);
        for (file, lib) in all() {
            every_connector_can_attach(file, lib);
        }
    }

    fn every_connector_can_attach(file: &str, lib: &Library) {
        for t in &lib.templates {
            for j in &t.jigsaws {
                // A connector must sit on the template's edge facing out.
                let out = j.pos + jig_offset(j, 0);
                let inside = out.cmpge(IVec3::ZERO).all() && out.cmplt(t.size).all();
                assert!(!inside, "{file} {}: connector {} faces into the template", t.name, j.pos);
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
                    "{file} {}: nothing in {} answers {}",
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

    const TOWER: &str = "
template t/base
size 5 1 5
projection rigid
fill 0 0 0 4 0 4 cobblestone
key c chest:test_loot@south
layer 0
.....
.....
..c..
.....
.....
jigsaw 2 0 2 up up up t/tops
mob 1 0 1 pillager
end
template t/top
size 3 4 3
projection rigid
fill 0 0 0 2 3 2 stone
key s spawner:zombie
layer 3
...
.s.
...
jigsaw 1 0 1 down up up empty
end
pool t/starts
1 t/base
end
pool t/tops
1 t/top
end
";

    #[test]
    fn fills_vertical_connectors_and_markers() {
        // An up connector joins a down one directly above it.
        let lib = Library::parse(TOWER).unwrap();
        let settings = Settings { start_pool: "t/starts", max_depth: 2, max_distance: 40 };
        for seed in 0..8 {
            let pieces = assemble(&lib, &settings, IVec2::new(37, -21), &flat, seed);
            assert_eq!(pieces.len(), 2, "seed {seed}");
            let (base, top) = (pieces[0], pieces[1]);
            // The top stands on the base: its down connector is one above the
            // base's up connector, whatever the rotations.
            let up = base.world(IVec3::new(2, 0, 2));
            assert_eq!(top.world(IVec3::new(1, 0, 1)), up + IVec3::Y);
            let markers = markers(&lib, &pieces, &flat);
            assert!(markers.contains(&Marker::Mob(base.world(IVec3::new(1, 0, 1)), "pillager")));
            assert!(markers.iter().any(|m| matches!(m, Marker::Loot(p, "test_loot", _) if *p == up)));
            assert!(markers.contains(&Marker::Spawner(top.world(IVec3::new(1, 3, 1)), "zombie")));
            // Painting a chunk and painting each column alone agree, and the
            // fill sits under the layer's chest.
            struct Plain;
            impl Palette for Plain {
                fn role(&self, _: &str) -> Option<Block> {
                    None
                }
            }
            let ground = Ground { surface: &flat, solid: &flat };
            let base_corner = IVec3::new(32, 64, -32);
            let mut chunk = Box::new([Block::AIR; CHUNK_VOLUME]);
            paint(&lib, &pieces, &Plain, &ground, &mut chunk, base_corner);
            assert_eq!(
                chunk[index((up.x - base_corner.x) as usize, 0, (up.z - base_corner.z) as usize)],
                Block::CHEST.with_facing(rotate_facing(Facing::South, base.rotation))
            );
            let corner = base.world(IVec3::ZERO);
            assert_eq!(
                chunk[index((corner.x - base_corner.x) as usize, 0, (corner.z - base_corner.z) as usize)],
                Block::COBBLESTONE
            );
            for x in 0..CHUNK_SIZE_I {
                for z in 0..CHUNK_SIZE_I {
                    let col_base = base_corner + IVec3::new(x, 0, z);
                    let mut column = [Block::AIR; CHUNK_SIZE_I as usize];
                    paint_canvas(
                        &lib,
                        &pieces,
                        &Plain,
                        &ground,
                        &mut Canvas {
                            blocks: &mut column,
                            base: col_base,
                            top: col_base + IVec3::Y * (CHUNK_SIZE_I - 1),
                        },
                    );
                    for y in 0..CHUNK_SIZE_I {
                        assert_eq!(
                            column[y as usize],
                            chunk[index(x as usize, y as usize, z as usize)],
                            "seed {seed} at {x} {y} {z}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_vertical_child_sits_above_its_terrain_projected_plate() {
        let lib = Library::parse(
            r#"
template base
size 1 1 1
jigsaw 0 0 0 east base plate plates
end
template plate
size 1 1 1
projection terrain
jigsaw 0 0 0 west plate base empty
jigsaw 0 0 0 up plate top tops
end
template top
size 1 1 1
jigsaw 0 0 0 down top plate empty
end
pool starts
1 base
end
pool plates
1 plate
end
pool tops
1 top
end
"#,
        )
        .unwrap();
        for seed in 0..16 {
            let pieces = assemble(
                &lib,
                &Settings { start_pool: "starts", max_depth: 7, max_distance: 80 },
                IVec2::ZERO,
                &|_, _| 70,
                seed,
            );
            assert_eq!(pieces.len(), 3);
            assert_eq!(pieces[1].origin.y, 71);
            assert_eq!(pieces[2].origin.y, 72);
            assert!(!pieces[1].bounds.intersects(&pieces[2].bounds));
        }
    }

    #[test]
    fn fills_must_fit_and_take_plain_blocks() {
        let e = Library::parse("template a\nsize 2 1 2\nfill 0 0 0 2 0 0 stone\nend\n").unwrap_err();
        assert!(e.contains("fill outside"), "{e}");
        let e = Library::parse("template a\nsize 2 1 2\nfill 0 0 0 1 0 0 chest\nend\n").unwrap_err();
        assert!(e.contains("fills take"), "{e}");
    }
}
