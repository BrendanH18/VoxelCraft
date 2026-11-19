//! Generation 4 template catalogues (v0.15 Track G, FE-02).
//!
//! Java's village, trial chamber and ancient city pools name hundreds of
//! templates. FE-02 keeps Java's pools, weights, connector graph, residents
//! and loot exact and lets the templates' geometry be original. A
//! catalogue file (`jigsaw/*_v4.txt`) therefore holds, for every template
//! location Java's pools name, one `piece` line with Java's facts — kind,
//! size, jigsaw connectors (the way each faces, name, target and pool),
//! job sites, beds, bells, loot tables and residents — and Java's pools,
//! whose elements are template locations with their processor list and
//! projection. A structure module supplies a painter that draws an
//! original template for each piece line at its size, placing every
//! connector where its own design wants it. `structure_catalogue_tests`
//! checks the lines and pools against the jar fixture both ways.
//!
//! Piece lines: `piece <location> <kind> <x> <y> <z> [key=value...]
//! [connector...]`, where a connector is `<face>:<name>[/<target>]><pool>
//! [^<placement>,<selection>][*count]`, `face` one of `n s e w` (the way it
//! faces out of the template in Java's frame, north being -z) or `u`/`d`
//! (up, down) followed by the connector's top (`n s e w`) and `!` when its
//! joint is aligned; the target defaults to the name and the priorities to
//! 0. `piece <location> missing` stands for a location the jar lacks (Java
//! places nothing for it).
//!
//! `alias <pool>` lines name pools that exist only as Java's
//! `pool_aliases` (trial chambers' spawner contents).
//!
//! Pool elements: `empty`; `feature:<name>` (Java's feature element: a
//! 1×1×1 box with a `bottom` connector facing down, drawn by the painter as
//! a piece whose `kind` is `feature`); `list:<a>+<b>...` (Java's list
//! element: every part placed together, connectors from the first); or a
//! location with `@<processor list>` and `~terrain` (terrain matching) when
//! Java's element has them.

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::super::block::{Block, Facing};
use super::{Base, Cell, Jigsaw, Library, Projection, Spec, Template};

/// A connector as Java's template has it (position left to the painter).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conn {
    /// `Some(facing)` for a side connector, `None` for up or down.
    pub facing: Option<Facing>,
    /// `1` up, `-1` down, `0` side.
    pub vertical: i8,
    pub name: String,
    pub target: String,
    pub pool: String,
    /// A vertical connector's top, and whether its joint is aligned.
    pub top: Facing,
    pub aligned: bool,
    pub placement: i32,
    pub selection: i32,
}

impl Conn {
    pub fn up(&self) -> bool {
        self.vertical == 1
    }

    pub fn down(&self) -> bool {
        self.vertical == -1
    }

    pub fn side(&self) -> bool {
        self.vertical == 0
    }

    /// The pool's last path segment, which names what it holds
    /// (`villagers`, `cats`, `decor`, `streets`...).
    pub fn pool_tail(&self) -> &str {
        self.pool.rsplit('/').next().unwrap_or(&self.pool)
    }
}

/// One `piece` line.
#[derive(Clone, Debug)]
pub struct PieceSpec {
    pub location: String,
    pub kind: String,
    pub size: IVec3,
    pub conns: Vec<Conn>,
    pub params: Vec<(String, String)>,
    pub missing: bool,
}

impl PieceSpec {
    /// A parameter's value.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|p| p.0 == key).map(|p| p.1.as_str())
    }

    /// Every value of a repeatable parameter.
    pub fn params_named<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.params.iter().filter(move |p| p.0 == key).map(|p| p.1.as_str())
    }

    /// A count parameter (`beds=2`), 0 when absent.
    pub fn count(&self, key: &str) -> i32 {
        self.param(key).and_then(|v| v.parse().ok()).unwrap_or(0)
    }

    /// `name*count` parameters expanded (`job=smoker*2`).
    pub fn counted<'a>(&'a self, key: &'a str) -> impl Iterator<Item = (&'a str, i32)> + 'a {
        self.params_named(key).map(|v| match v.rsplit_once('*') {
            Some((name, n)) => (name, n.parse().unwrap_or(1)),
            None => (v, 1),
        })
    }

    /// The last path segment of the location.
    pub fn base_name(&self) -> &str {
        self.location.rsplit('/').next().unwrap_or(&self.location)
    }
}

fn letter(c: char) -> Option<Facing> {
    Some(match c {
        'n' => Facing::North,
        's' => Facing::South,
        'e' => Facing::East,
        'w' => Facing::West,
        _ => return None,
    })
}

/// A face token: `n s e w`, or `u`/`d` with the connector's top letter and
/// `!` for an aligned joint (`un!`).
fn face(s: &str) -> Result<(Option<Facing>, i8, Facing, bool), String> {
    let err = || format!("unknown connector face {s:?}");
    let mut chars = s.chars();
    let first = chars.next().ok_or_else(err)?;
    if let Some(f) = letter(first) {
        return if s.len() == 1 { Ok((Some(f), 0, Facing::North, false)) } else { Err(err()) };
    }
    let vertical = match first {
        'u' => 1,
        'd' => -1,
        _ => return Err(err()),
    };
    let top = chars.next().and_then(letter).ok_or_else(err)?;
    let aligned = match chars.next() {
        None => false,
        Some('!') => true,
        _ => return Err(err()),
    };
    Ok((None, vertical, top, aligned))
}

fn conns(token: &str) -> Result<Vec<Conn>, String> {
    let (token, count) = match token.rsplit_once('*') {
        Some((t, n)) => (t, n.parse::<usize>().map_err(|_| format!("bad count in {token:?}"))?),
        None => (token, 1),
    };
    let (f, rest) = token.split_once(':').ok_or_else(|| format!("bad connector {token:?}"))?;
    let (names, pool) = rest.split_once('>').ok_or_else(|| format!("connector {token:?} has no pool"))?;
    let (pool, priorities) = match pool.split_once('^') {
        Some((p, pr)) => {
            let (a, b) = pr.split_once(',').unwrap_or((pr, "0"));
            let n = |v: &str| v.parse::<i32>().map_err(|_| format!("bad priority in {token:?}"));
            (p, (n(a)?, n(b)?))
        }
        None => (pool, (0, 0)),
    };
    let (name, target) = names.split_once('/').unwrap_or((names, names));
    let (facing, vertical, top, aligned) = face(f)?;
    let c = Conn {
        facing,
        vertical,
        name: name.into(),
        target: target.into(),
        pool: pool.into(),
        top,
        aligned,
        placement: priorities.0,
        selection: priorities.1,
    };
    Ok(vec![c; count])
}

/// Parses a catalogue's piece lines; the rest of the text is returned for
/// the pool parser.
pub fn parse_pieces(text: &str) -> Result<(Vec<PieceSpec>, String), String> {
    let mut pieces = Vec::new();
    let mut rest = String::new();
    for (n, line) in text.lines().enumerate() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.first() != Some(&"piece") {
            rest.push_str(line);
            rest.push('\n');
            continue;
        }
        rest.push('\n');
        let err = |m: String| format!("line {}: {m}", n + 1);
        if words.len() == 3 && words[2] == "missing" {
            pieces.push(PieceSpec {
                location: words[1].into(),
                kind: "missing".into(),
                size: IVec3::ONE,
                conns: Vec::new(),
                params: Vec::new(),
                missing: true,
            });
            continue;
        }
        if words.len() < 6 {
            return Err(err("expected `piece <location> <kind> <x> <y> <z> ...`".into()));
        }
        let int = |s: &str| s.parse::<i32>().map_err(|_| err(format!("not a number: {s:?}")));
        let size = IVec3::new(int(words[3])?, int(words[4])?, int(words[5])?);
        let mut spec = PieceSpec {
            location: words[1].into(),
            kind: words[2].into(),
            size,
            conns: Vec::new(),
            params: Vec::new(),
            missing: false,
        };
        for w in &words[6..] {
            if w.contains('>') {
                spec.conns.extend(conns(w).map_err(err)?);
            } else if let Some((k, v)) = w.split_once('=') {
                spec.params.push((k.into(), v.into()));
            } else {
                return Err(err(format!("unexpected {w:?}")));
            }
        }
        pieces.push(spec);
    }
    Ok((pieces, rest))
}

/// A template being drawn in code. Later writes to a cell replace earlier
/// ones; connectors are added as the painter places them.
pub struct Draft<'a> {
    pub lib: &'a mut Library,
    pub size: IVec3,
    cells: FxHashMap<IVec3, Spec>,
    order: Vec<IVec3>,
    pub jigsaws: Vec<Jigsaw>,
    pub mobs: Vec<(IVec3, String)>,
    pub overhang: i32,
}

impl Draft<'_> {
    pub fn new(lib: &mut Library, size: IVec3) -> Draft<'_> {
        Draft {
            lib,
            size,
            cells: FxHashMap::default(),
            order: Vec::new(),
            jigsaws: Vec::new(),
            mobs: Vec::new(),
            overhang: 0,
        }
    }

    pub fn inside(&self, p: IVec3) -> bool {
        p.cmpge(IVec3::splat(-self.overhang)).all()
            && p.cmplt(self.size + IVec3::new(self.overhang, self.overhang, self.overhang)).all()
    }

    /// Sets a cell to a spec (ignored outside the template).
    pub fn spec(&mut self, p: IVec3, spec: Spec) {
        if !self.inside(p) {
            return;
        }
        if self.cells.insert(p, spec).is_none() {
            self.order.push(p);
        }
    }

    pub fn block(&mut self, p: IVec3, b: Block) {
        self.spec(p, Spec { base: Base::Block(b), facing: None, climb: false, upper: false });
    }

    /// A block turned to face `f` (template directions).
    pub fn facing(&mut self, p: IVec3, b: Block, f: Facing) {
        self.spec(p, Spec { base: Base::Block(b), facing: Some(f), climb: false, upper: false });
    }

    /// Stairs climbing toward `f`.
    pub fn stairs(&mut self, p: IVec3, b: Block, f: Facing) {
        self.spec(p, Spec { base: Base::Block(b), facing: Some(f), climb: true, upper: false });
    }

    pub fn role(&mut self, p: IVec3, role: &str) {
        let r = self.lib.role_index(role);
        self.spec(p, Spec { base: Base::Role(r), facing: None, climb: false, upper: false });
    }

    /// A role block climbing (stairs) toward `f`.
    pub fn role_stairs(&mut self, p: IVec3, role: &str, f: Facing) {
        let r = self.lib.role_index(role);
        self.spec(p, Spec { base: Base::Role(r), facing: Some(f), climb: true, upper: false });
    }

    /// A role block facing `f`.
    pub fn role_facing(&mut self, p: IVec3, role: &str, f: Facing) {
        let r = self.lib.role_index(role);
        self.spec(p, Spec { base: Base::Role(r), facing: Some(f), climb: false, upper: false });
    }

    pub fn air(&mut self, p: IVec3) {
        self.block(p, Block::AIR);
    }

    /// Both halves of a door at `p` whose front faces `f`.
    pub fn door(&mut self, p: IVec3, f: Facing) {
        self.spec(p, Spec { base: Base::Door, facing: Some(f), climb: false, upper: false });
        self.spec(p + IVec3::Y, Spec { base: Base::Door, facing: Some(f), climb: false, upper: true });
    }

    /// A bed with its foot at `foot` and its head toward `f`.
    pub fn bed(&mut self, foot: IVec3, f: Facing) {
        self.spec(foot, Spec { base: Base::BedFoot, facing: Some(f), climb: false, upper: false });
        self.spec(foot + f.offset(), Spec { base: Base::BedHead, facing: Some(f), climb: false, upper: false });
    }

    /// A container filled from a loot table, facing `f`.
    pub fn loot(&mut self, p: IVec3, table: &str, f: Facing) {
        let i = self.lib.loot_index(table);
        self.spec(p, Spec { base: Base::Loot(i), facing: Some(f), climb: false, upper: false });
    }

    /// A container block filled from a loot table, facing `f`.
    pub fn loot_in(&mut self, p: IVec3, table: &str, container: Block, f: Facing) {
        let i = self.lib.loot_index(table);
        self.spec(p, Spec { base: Base::LootIn(i, container), facing: Some(f), climb: false, upper: false });
    }

    /// A spawner of a named configuration.
    pub fn spawner(&mut self, p: IVec3, name: &str) {
        let i = self.lib.mob_index(name);
        self.spec(p, Spec { base: Base::Spawner(i), facing: None, climb: false, upper: false });
    }

    pub fn mob(&mut self, p: IVec3, name: &str) {
        self.mobs.push((p, name.to_string()));
    }

    /// Fills an inclusive box with a block.
    pub fn fill(&mut self, a: IVec3, b: IVec3, block: Block) {
        let (lo, hi) = (a.min(b), a.max(b));
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    self.block(IVec3::new(x, y, z), block);
                }
            }
        }
    }

    /// Fills an inclusive box with a role.
    pub fn fill_role(&mut self, a: IVec3, b: IVec3, role: &str) {
        let (lo, hi) = (a.min(b), a.max(b));
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    self.role(IVec3::new(x, y, z), role);
                }
            }
        }
    }

    /// The spec at a cell, if one is set.
    pub fn get(&self, p: IVec3) -> Option<Spec> {
        self.cells.get(&p).copied()
    }

    /// Places a connector at `p`.
    pub fn connect(&mut self, p: IVec3, c: &Conn) {
        self.jigsaws.push(Jigsaw {
            pos: p,
            facing: c.facing.unwrap_or(Facing::North),
            vertical: c.vertical,
            name: c.name.clone(),
            target: c.target.clone(),
            pool: c.pool.clone(),
            top: c.top,
            aligned: c.aligned,
            placement: c.placement,
            selection: c.selection,
        });
    }

    fn finish(self, name: String, projection: Projection) -> Template {
        let cells = self.order.iter().map(|p| Cell { pos: *p, spec: self.cells[p] }).collect();
        Template {
            name,
            size: self.size,
            projection,
            cells,
            jigsaws: self.jigsaws,
            homes: Vec::new(),
            fills: Vec::new(),
            mobs: self.mobs,
            processors: None,
            overhang: self.overhang,
        }
    }
}

/// Draws one piece's original template into a draft. The painter must place
/// every connector of `spec.conns` (checked when the library is built).
pub type Painter = dyn Fn(&PieceSpec, &mut Draft);

/// A pool element token: its locations with their processor list, and
/// whether it follows the terrain.
struct Element<'a> {
    parts: Vec<(&'a str, Option<&'a str>)>,
    terrain: bool,
    feature: Option<&'a str>,
}

fn element(token: &str) -> Element<'_> {
    if let Some(f) = token.strip_prefix("feature:") {
        return Element { parts: Vec::new(), terrain: false, feature: Some(f) };
    }
    let (token, terrain) = match token.strip_suffix("~terrain") {
        Some(t) => (t, true),
        None => (token, false),
    };
    let list = token.strip_prefix("list:");
    let parts = list
        .unwrap_or(token)
        .split('+')
        .map(|p| match p.split_once('@') {
            Some((l, proc_)) => (l, Some(proc_)),
            None => (p, None),
        })
        .collect();
    Element { parts, terrain, feature: None }
}

/// Builds a catalogue's library: a template for every pool element token,
/// drawn by `paint`, then the pools.
pub fn build(text: &str, paint: &Painter) -> Result<Library, String> {
    let (pieces, pools) = parse_pieces(text)?;
    let specs: FxHashMap<&str, &PieceSpec> = pieces.iter().map(|p| (p.location.as_str(), p)).collect();
    let mut lib = Library::default();
    // `alias <pool>`: a pool name Java's structure resolves by its aliases.
    let mut rest = String::new();
    for line in pools.lines() {
        match line.strip_prefix("alias ") {
            Some(name) => lib.aliases.push(name.trim().to_string()),
            None => {
                rest.push_str(line);
                rest.push('\n');
            }
        }
    }
    let pools = rest;
    let mut names: FxHashMap<String, u16> = FxHashMap::default();
    for line in pools.lines() {
        let mut w = line.split_whitespace();
        let (Some(weight), Some(token), None) = (w.next(), w.next(), w.next()) else { continue };
        if weight.parse::<u32>().is_err() || token == "empty" || names.contains_key(token) {
            continue;
        }
        let e = element(token);
        let template = if let Some(feature) = e.feature {
            let spec = specs
                .get(format!("feature:{feature}").as_str())
                .ok_or_else(|| format!("no piece line for feature {feature}"))?;
            let mut d = Draft::new(&mut lib, IVec3::ONE);
            paint(spec, &mut d);
            d.finish(token.to_string(), Projection::Rigid)
        } else {
            let mut parts = Vec::new();
            for &(location, processors) in &e.parts {
                let spec = specs.get(location).ok_or_else(|| format!("no piece line for {location}"))?;
                let mut d = Draft::new(&mut lib, spec.size);
                if !spec.missing {
                    paint(spec, &mut d);
                }
                let placed = d.jigsaws.clone();
                let mut have = placed
                    .iter()
                    .map(|j| {
                        let facing = (j.vertical == 0).then_some(j.facing);
                        let top = (j.vertical != 0).then_some((j.top, j.aligned));
                        (
                            facing,
                            j.vertical,
                            top,
                            j.name.clone(),
                            j.target.clone(),
                            j.pool.clone(),
                            j.placement,
                            j.selection,
                        )
                    })
                    .collect::<Vec<_>>();
                let mut want = spec
                    .conns
                    .iter()
                    .map(|c| {
                        let top = (c.vertical != 0).then_some((c.top, c.aligned));
                        (
                            c.facing,
                            c.vertical,
                            top,
                            c.name.clone(),
                            c.target.clone(),
                            c.pool.clone(),
                            c.placement,
                            c.selection,
                        )
                    })
                    .collect::<Vec<_>>();
                have.sort_by_key(|c| format!("{c:?}"));
                want.sort_by_key(|c| format!("{c:?}"));
                if have != want {
                    return Err(format!("{location}: painted connectors {have:?} differ from {want:?}"));
                }
                let mut t =
                    d.finish(location.to_string(), if e.terrain { Projection::Terrain } else { Projection::Rigid });
                t.processors = processors.map(|p| lib.processor_index(p));
                parts.push(t);
            }
            let mut t = parts.remove(0);
            for p in parts {
                // Java's list element: every part at the same origin, the
                // first part's connectors.
                t.size = t.size.max(p.size);
                t.cells.extend(p.cells);
                t.mobs.extend(p.mobs);
                t.name = format!("{}+{}", t.name, p.name);
            }
            if token.starts_with("list:") {
                t.name = format!("list:{}", t.name);
            }
            t
        };
        names.insert(token.to_string(), lib.templates.len() as u16);
        lib.templates.push(template);
    }
    Library::parse_onto(lib, names, &pools)
}

#[cfg(test)]
#[path = "catalogue_tests.rs"]
mod structure_catalogue_tests;
