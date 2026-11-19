//! Generation 4 village templates (v0.15 Track G, FE-02).
//!
//! `jigsaw/village_v4.txt` lists every template location Java 26.3's village
//! pools name, with Java's size, job sites, beds, bells, loot, residents and
//! connectors, and Java's pools. This module draws an original template for
//! each line: houses and profession buildings with their job sites, beds and
//! chests, town centres with bells, streets, farms, pens and stables, lamps,
//! decorations, the residents' spawn markers and the feature elements (trees
//! and piles). The blocks are the Java materials each style's processor
//! lists act on (cobblestone mossifies, doors, torches and planks give way
//! in abandoned villages, wheat becomes other crops), so Java's processors
//! apply unchanged. Connectors keep the face Java gives them; where along
//! the face, and every block, is VoxelCraft's own design.

use std::sync::OnceLock;

use glam::IVec3;

use crate::world::block::{Block, Facing};
use crate::world::jigsaw::Library;
use crate::world::jigsaw::catalogue::{self, Conn, Draft, PieceSpec};
use crate::world::jigsaw::draw::{Frame, by_face, face_cell, face_len, mix, named, place_rest, spread};

/// The generation 4 village library, built once.
pub fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        let mut lib =
            catalogue::build(include_str!("../jigsaw/village_v4.txt"), &paint).unwrap_or_else(|e| panic!("{e}"));
        lib.expansion_hack = true;
        lib
    })
}

/// A village style's building materials (Java's own blocks, so the style's
/// processor lists find them).
#[derive(Clone, Copy)]
struct Mat {
    foundation: Block,
    floor: Block,
    wall: Block,
    beam: Block,
    roof: Block,
    slab: Block,
    window: Block,
    light: Block,
    fence: Block,
    path: Block,
    ground: Block,
    accent: Block,
    flat_roof: bool,
}

fn style_of(spec: &PieceSpec) -> &str {
    spec.location.split('/').nth(1).unwrap_or("plains")
}

fn mat(style: &str) -> Mat {
    match style {
        "desert" => Mat {
            foundation: named("sandstone"),
            floor: named("smooth sandstone"),
            wall: named("smooth sandstone"),
            beam: named("cut sandstone"),
            roof: named("smooth sandstone stairs"),
            slab: named("smooth sandstone slab"),
            window: named("glass pane"),
            light: Block::TORCH,
            fence: Block::OAK_FENCE,
            path: named("smooth sandstone"),
            ground: Block::SAND,
            accent: named("terracotta"),
            flat_roof: true,
        },
        "savanna" => Mat {
            foundation: named("terracotta"),
            floor: named("acacia planks"),
            wall: named("acacia planks"),
            beam: named("acacia log"),
            roof: named("acacia stairs"),
            slab: named("acacia slab"),
            window: named("glass pane"),
            light: Block::TORCH,
            fence: named("acacia fence"),
            path: Block::DIRT_PATH,
            ground: Block::GRASS,
            accent: named("orange terracotta"),
            flat_roof: false,
        },
        "taiga" => Mat {
            foundation: Block::COBBLESTONE,
            floor: named("spruce planks"),
            wall: named("spruce planks"),
            beam: named("spruce log"),
            roof: named("spruce stairs"),
            slab: named("spruce slab"),
            window: named("glass pane"),
            light: Block::TORCH,
            fence: named("spruce fence"),
            path: Block::DIRT_PATH,
            ground: Block::GRASS,
            accent: named("mossy cobblestone"),
            flat_roof: false,
        },
        "snowy" => Mat {
            foundation: named("stone bricks"),
            floor: named("spruce planks"),
            wall: named("spruce planks"),
            beam: named("stripped spruce log"),
            roof: named("spruce stairs"),
            slab: named("spruce slab"),
            window: named("glass pane"),
            light: named("lantern"),
            fence: named("spruce fence"),
            path: Block::DIRT_PATH,
            ground: Block::SNOWY_GRASS,
            accent: named("packed ice"),
            flat_roof: false,
        },
        _ => Mat {
            foundation: Block::COBBLESTONE,
            floor: named("oak planks"),
            wall: named("oak planks"),
            beam: named("stripped oak log"),
            roof: named("oak stairs"),
            slab: named("oak slab"),
            window: named("glass pane"),
            light: Block::TORCH,
            fence: Block::OAK_FENCE,
            path: Block::DIRT_PATH,
            ground: Block::GRASS,
            accent: named("white terracotta"),
            flat_roof: false,
        },
    }
}

/// Draws one piece (the catalogue's painter).
pub fn paint(spec: &PieceSpec, d: &mut Draft) {
    match spec.kind.as_str() {
        "feature" => feature(spec, d),
        "resident" => resident(spec, d),
        "center" => center(spec, d),
        "street" | "terminator" => street(spec, d),
        "house" => house(spec, d),
        "small_farm" | "large_farm" => farm(spec, d),
        "animal_pen" | "stable" => pen(spec, d),
        "meeting_point" | "accessory" => plaza(spec, d),
        "lamp" => lamp(spec, d),
        "decoration" => decoration(spec, d),
        "well_bottom" => well_bottom(spec, d),
        other => panic!("{}: no village painter for {other}", spec.location),
    }
}

fn is_entrance(c: &Conn) -> bool {
    c.side() && (c.name == "building_entrance" || c.name == "buidling_entrance")
}

/// Which way a piece's front faces: its building entrance's face.
fn front(spec: &PieceSpec) -> Facing {
    spec.conns.iter().find(|c| is_entrance(c)).and_then(|c| c.facing).unwrap_or(Facing::North)
}

/// A spawn marker piece: a villager, zombie villager, cat, iron golem,
/// farm animals or a camel, standing on the cell above the parent's floor.
fn resident(spec: &PieceSpec, d: &mut Draft) {
    for (name, n) in spec.counted("mob") {
        for _ in 0..n {
            d.mob(IVec3::ZERO, name);
        }
    }
    for c in &spec.conns {
        d.connect(IVec3::ZERO, c);
    }
}

/// A free-cell allocator over a frame's floor plan.
struct Plan {
    w: i32,
    d: i32,
    used: Vec<bool>,
}

impl Plan {
    fn new(w: i32, d: i32) -> Plan {
        Plan { w, d, used: vec![false; (w * d).max(0) as usize] }
    }

    fn ok(&self, u: i32, v: i32) -> bool {
        u >= 0 && v >= 0 && u < self.w && v < self.d && !self.used[(v * self.w + u) as usize]
    }

    fn take(&mut self, u: i32, v: i32) {
        if u >= 0 && v >= 0 && u < self.w && v < self.d {
            self.used[(v * self.w + u) as usize] = true;
        }
    }

    /// The first free cell of `cells`, taken.
    fn first(&mut self, cells: &[(i32, i32)]) -> Option<(i32, i32)> {
        let c = cells.iter().copied().find(|&(u, v)| self.ok(u, v))?;
        self.take(c.0, c.1);
        Some(c)
    }
}

fn house(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let fr = Frame::new(front(spec), spec.size);
    let (w, dep, h) = (fr.w(), fr.d(), fr.h());
    let seed = mix(&spec.location, w, dep);
    let outdoor = spec.conns.iter().filter(|c| c.up() && matches!(c.pool_tail(), "decor" | "trees")).count();
    let porch = dep >= 7 || outdoor > 0 && dep >= 5;
    let v0 = porch as i32;
    let ud = w / 2;
    let yw = if h >= 10 { 5 } else { (h - 2).clamp(2, 3) };
    let edge = |u: i32, v: i32| u == 0 || u == w - 1 || v == v0 || v == dep - 1;
    // Foundation, floor and walls.
    for u in 0..w {
        if porch {
            d.block(fr.at(u, 0, 0), if u == ud { m.path } else { m.foundation });
        }
        for v in v0..dep {
            d.block(fr.at(u, 0, v), if edge(u, v) { m.foundation } else { m.floor });
            for y in 1..=yw {
                let corner = (u == 0 || u == w - 1) && (v == v0 || v == dep - 1);
                let b = if corner {
                    m.beam
                } else if edge(u, v) {
                    if y == 1 && style_of(spec) == "savanna" { m.accent } else { m.wall }
                } else {
                    Block::AIR
                };
                d.block(fr.at(u, y, v), b);
            }
        }
    }
    // Windows on the sides and back, every other cell.
    if yw >= 3 {
        for v in v0 + 1..dep - 1 {
            if (v - v0) % 2 == 0 {
                d.block(fr.at(0, 2, v), m.window);
                d.block(fr.at(w - 1, 2, v), m.window);
            }
        }
        for u in 1..w - 1 {
            if u % 2 == 1 && u != ud {
                d.block(fr.at(u, 2, dep - 1), m.window);
                d.block(fr.at(u, 2, v0), m.window);
            }
        }
    }
    d.door(fr.at(ud, 1, v0), fr.face(Facing::North));
    roof(d, &fr, &m, v0, yw, seed);
    // Furniture: beds, job sites, chests and barrels along the walls.
    let mut plan = Plan::new(w, dep);
    for u in 0..w {
        for v in 0..dep {
            if u == 0 || u == w - 1 || v <= v0 || v == dep - 1 {
                plan.take(u, v);
            }
        }
    }
    plan.take(ud, v0 + 1);
    let perimeter: Vec<(i32, i32)> = {
        let mut cells = Vec::new();
        for u in 1..w - 1 {
            cells.push((u, dep - 2));
        }
        for v in (v0 + 1..dep - 2).rev() {
            cells.push((1, v));
            cells.push((w - 2, v));
        }
        for u in 1..w - 1 {
            cells.push((u, v0 + 1));
        }
        cells
    };
    let anywhere: Vec<(i32, i32)> = (0..w).flat_map(|u| (0..dep).map(move |v| (u, v))).collect();
    for _ in 0..spec.count("bed") {
        bed(d, &fr, &mut plan, w, dep, v0);
    }
    let mut items: Vec<Block> = Vec::new();
    for (job, n) in spec.counted("job") {
        items.extend(std::iter::repeat_n(named(&job.replace('_', " ")), n as usize));
    }
    items.extend(std::iter::repeat_n(named("barrel"), spec.count("barrel") as usize));
    items.extend(std::iter::repeat_n(Block::BELL, spec.count("bell") as usize));
    for item in items {
        let (u, v) = slot(&mut plan, &perimeter, &anywhere);
        d.facing(fr.at(u, 1, v), item, fr.face(Facing::North));
    }
    for (loot, n) in spec.counted("loot") {
        let table = loot.split_once(':').map_or(loot, |(_, t)| t);
        for _ in 0..n {
            let (u, v) = slot(&mut plan, &perimeter, &anywhere);
            d.loot(fr.at(u, 1, v), table, fr.face(Facing::North));
        }
    }
    if let Some((u, v)) = plan.first(&perimeter) {
        d.block(fr.at(u, 1, v), m.light);
    }
    // Residents Java's template holds itself (the taiga armorer's armor
    // stands), standing in the room.
    for (name, n) in spec.counted("mob") {
        for _ in 0..n {
            let (u, v) = slot(&mut plan, &perimeter, &anywhere);
            d.mob(fr.at(u, 1, v), name);
        }
    }
    // Connectors: the entrance at the door, residents inside, decor on the porch.
    let mut done = Vec::new();
    let mut inner: Vec<IVec3> = Vec::new();
    for v in (v0 + 1..dep - 1).rev() {
        for u in 1..w - 1 {
            if plan.ok(u, v) {
                inner.push(fr.at(u, 0, v));
            }
        }
    }
    let mut outside: Vec<IVec3> = (0..w).filter(|&u| porch && u != ud).map(|u| fr.at(u, 0, 0)).collect();
    for (i, c) in spec.conns.iter().enumerate() {
        if is_entrance(c) && c.facing == Some(fr.front) && !done.iter().any(|&j| is_entrance(&spec.conns[j])) {
            d.connect(fr.at(ud, 1, 0), c);
            done.push(i);
        } else if c.up() {
            let from_outside = matches!(c.pool_tail(), "decor" | "trees");
            let list = if from_outside && !outside.is_empty() { &mut outside } else { &mut inner };
            if !list.is_empty() {
                d.connect(list.remove(0), c);
                done.push(i);
            } else if !outside.is_empty() {
                d.connect(outside.remove(0), c);
                done.push(i);
            }
        }
    }
    let mut floor = inner;
    floor.extend(outside);
    place_rest(spec, d, &done, 1, &mut floor);
}

/// A free furniture cell: along the walls first, else anywhere in the plan
/// (a wall cell at worst).
fn slot(plan: &mut Plan, perimeter: &[(i32, i32)], anywhere: &[(i32, i32)]) -> (i32, i32) {
    if let Some(c) = plan.first(perimeter) {
        return c;
    }
    if let Some(c) = plan.first(anywhere) {
        return c;
    }
    (anywhere.len() as i32 % plan.w.max(1), 0)
}

/// A bed's foot and head cells and the way its head points.
type BedSpot = ((i32, i32), (i32, i32), Facing);

/// A bed against the back wall, else along a side, else anywhere it fits.
fn bed(d: &mut Draft, fr: &Frame, plan: &mut Plan, w: i32, dep: i32, v0: i32) {
    let back = (1..w - 1).map(|u| ((u, dep - 3), (u, dep - 2), Facing::South));
    let sides =
        (v0 + 1..dep - 2).flat_map(|v| [((2, v), (1, v), Facing::West), ((w - 3, v), (w - 2, v), Facing::East)]);
    let all: Vec<BedSpot> = back.chain(sides).collect();
    for (foot, head, f) in all {
        if plan.ok(foot.0, foot.1) && plan.ok(head.0, head.1) && foot.1 > v0 && head.1 > v0 {
            plan.take(foot.0, foot.1);
            plan.take(head.0, head.1);
            d.bed(fr.at(foot.0, 1, foot.1), fr.face(f));
            return;
        }
    }
    // A cramped house: the bed goes where two cells are free, walls included.
    for v in (0..dep - 1).rev() {
        for u in 0..w {
            if plan.ok(u, v) && plan.ok(u, v + 1) {
                plan.take(u, v);
                plan.take(u, v + 1);
                d.bed(fr.at(u, 1, v), fr.face(Facing::South));
                return;
            }
        }
    }
    d.bed(fr.at(w / 2, 1, 0), fr.face(Facing::West));
}

/// A gable roof across the front (stairs climbing in from both sides), or a
/// flat one for desert houses and low pieces.
fn roof(d: &mut Draft, fr: &Frame, m: &Mat, v0: i32, yw: i32, seed: u64) {
    let (w, dep, h) = (fr.w(), fr.d(), fr.h());
    if m.flat_roof || h - yw - 1 < 2 {
        for u in 0..w {
            for v in v0..dep {
                let rim = u == 0 || u == w - 1 || v == v0 || v == dep - 1;
                d.block(fr.at(u, yw + 1, v), if rim && m.flat_roof { m.beam } else { m.slab_or(m.wall, seed) });
            }
        }
        return;
    }
    let mut k = 0;
    loop {
        let y = yw + 1 + k;
        let (a, b) = (k, w - 1 - k);
        if y >= h || a > b {
            break;
        }
        for v in v0..dep {
            if a == b {
                d.block(fr.at(a, y, v), m.slab);
                continue;
            }
            d.stairs(fr.at(a, y, v), m.roof, fr.face(Facing::East));
            d.stairs(fr.at(b, y, v), m.roof, fr.face(Facing::West));
            for u in a + 1..b {
                let gable = v == v0 || v == dep - 1;
                let top = y == h - 1;
                d.block(
                    fr.at(u, y, v),
                    if top {
                        m.slab
                    } else if gable {
                        m.wall
                    } else {
                        Block::AIR
                    },
                );
            }
        }
        k += 1;
    }
}

impl Mat {
    /// The roof slab, or the wall block for variety.
    fn slab_or(&self, wall: Block, seed: u64) -> Block {
        if seed.is_multiple_of(3) { wall } else { self.slab }
    }
}

/// Town centres: a plaza with a fountain, well or bell post, its streets
/// leaving from the middle of each side.
fn center(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let s = spec.size;
    let c = IVec3::new(s.x / 2, 0, s.z / 2);
    for x in 0..s.x {
        for z in 0..s.z {
            d.block(IVec3::new(x, 0, z), m.path);
            for y in 1..s.y {
                d.air(IVec3::new(x, y, z));
            }
        }
    }
    let mut taken: Vec<IVec3> = Vec::new();
    let well = spec.conns.iter().any(|c| c.down());
    let fountain = spec.base_name().contains("fountain");
    // The centrepiece: a basin (fountain), a well (a down connector to the
    // well bottom), or a raised platform.
    let r = if fountain { 2 } else { 1 };
    for x in c.x - r..=c.x + r {
        for z in c.z - r..=c.z + r {
            let rim = (x - c.x).abs() == r || (z - c.z).abs() == r;
            let at = IVec3::new(x, 0, z);
            taken.push(at);
            if rim {
                d.block(at, m.foundation);
                d.block(at + IVec3::Y, if fountain || well { m.foundation } else { m.fence });
            } else {
                d.block(at, if fountain || well { Block::WATER } else { m.foundation });
            }
        }
    }
    if s.y >= 5 && (well || !fountain) {
        // Posts and a roof over the centrepiece.
        for (dx, dz) in [(-r, -r), (r, -r), (-r, r), (r, r)] {
            for y in 2..s.y - 1 {
                d.block(c + IVec3::new(dx, y, dz), m.fence);
            }
        }
        for x in c.x - r..=c.x + r {
            for z in c.z - r..=c.z + r {
                d.block(IVec3::new(x, s.y - 1, z), m.slab);
            }
        }
        d.block(c + IVec3::new(0, s.y - 2, 0), m.light);
    }
    // Bells beside the centrepiece.
    let bells = spec.count("bell");
    for i in 0..bells {
        let off =
            [IVec3::new(0, 0, r + 1), IVec3::new(0, 0, -r - 1), IVec3::new(r + 1, 0, 0), IVec3::new(-r - 1, 0, 0)]
                [i as usize % 4];
        let at = (c + off).clamp(IVec3::ZERO, s - IVec3::ONE);
        d.block(at.with_y(0), m.foundation);
        d.block(at.with_y(1), Block::BELL);
        taken.push(at.with_y(0));
    }
    // Corner lights.
    if s.x >= 7 && s.z >= 7 {
        for (x, z) in [(1, 1), (s.x - 2, 1), (1, s.z - 2), (s.x - 2, s.z - 2)] {
            let at = IVec3::new(x, 0, z);
            d.block(at + IVec3::Y, m.fence);
            if s.y >= 4 {
                d.block(at + IVec3::new(0, 2, 0), m.light);
            }
            taken.push(at);
        }
    }
    let mut done = Vec::new();
    for (f, cs) in by_face(&spec.conns, |c| c.name == "street") {
        let len = face_len(s, f);
        for (c, t) in cs.iter().zip(spread(len, cs.len())) {
            d.connect(face_cell(s, f, t, 0), c);
            let i = spec.conns.iter().position(|x| std::ptr::eq(x, *c)).unwrap();
            done.push(i);
        }
    }
    if let Some(i) = spec.conns.iter().position(|c| c.down()) {
        d.connect(c, &spec.conns[i]);
        done.push(i);
    }
    // Residents stand round the plaza, off the street lanes.
    let mut floor: Vec<IVec3> = Vec::new();
    for ring in 1..s.x.max(s.z) {
        for x in 0..s.x {
            for z in 0..s.z {
                let at = IVec3::new(x, 0, z);
                if (x - c.x).abs().max((z - c.z).abs()) == r + ring
                    && !taken.contains(&at)
                    && x != c.x
                    && z != c.z
                    && x > 0
                    && z > 0
                    && x < s.x - 1
                    && z < s.z - 1
                {
                    floor.push(at);
                }
            }
        }
    }
    place_rest(spec, d, &done, 0, &mut floor);
}

/// Streets and their ends (terrain matching): a three-wide path from every
/// street connector to the middle, and a spur to every building entrance.
fn street(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let s = spec.size;
    let c = IVec3::new(s.x / 2, 0, s.z / 2);
    let half = if s.x.min(s.z) >= 7 { 1 } else { 0 };
    let mut path = vec![false; (s.x * s.z) as usize];
    let lay = |path: &mut Vec<bool>, x: i32, z: i32| {
        if x >= 0 && z >= 0 && x < s.x && z < s.z {
            path[(z * s.x + x) as usize] = true;
        }
    };
    let streets = by_face(&spec.conns, |c| c.name == "street");
    let mut done = Vec::new();
    for (f, cs) in &streets {
        let len = face_len(s, *f);
        for (conn, t) in cs.iter().zip(spread(len, cs.len())) {
            let at = face_cell(s, *f, t, 0);
            d.connect(at, conn);
            done.push(spec.conns.iter().position(|x| std::ptr::eq(x, *conn)).unwrap());
            // Inward to the middle line, then along it to the middle.
            let inward = f.opposite().offset();
            let mut p = at;
            let steps = if f.along_x() { (c.x - at.x).abs() } else { (c.z - at.z).abs() };
            for _ in 0..=steps {
                for o in -half..=half {
                    let side = if f.along_x() { IVec3::new(0, 0, o) } else { IVec3::new(o, 0, 0) };
                    lay(&mut path, p.x + side.x, p.z + side.z);
                }
                p += inward;
            }
            let (from, to) = if f.along_x() { (at.z.min(c.z), at.z.max(c.z)) } else { (at.x.min(c.x), at.x.max(c.x)) };
            for t in from..=to {
                for o in -half..=half {
                    if f.along_x() {
                        lay(&mut path, c.x + o, t);
                    } else {
                        lay(&mut path, t, c.z + o);
                    }
                }
            }
        }
    }
    if streets.is_empty() {
        for x in 0..s.x {
            for z in 0..s.z {
                lay(&mut path, x, z);
            }
        }
    }
    // Building entrances: spread along their face, a spur in to the path.
    for (f, cs) in by_face(&spec.conns, is_entrance) {
        let len = face_len(s, f);
        let lane = streets.iter().find(|(g, _)| *g == f).map(|(_, v)| spread(len, v.len()));
        let mut spots: Vec<i32> =
            (0..len).filter(|t| lane.as_ref().is_none_or(|l| l.iter().all(|x| (x - t).abs() > half + 1))).collect();
        if spots.is_empty() {
            spots = (0..len).collect();
        }
        let picks = spread(spots.len() as i32, cs.len());
        for (conn, i) in cs.iter().zip(picks) {
            let t = spots[i as usize];
            let at = face_cell(s, f, t, 0);
            d.connect(at, conn);
            done.push(spec.conns.iter().position(|x| std::ptr::eq(x, *conn)).unwrap());
            let mut p = at;
            for _ in 0..s.x.max(s.z) {
                if p.x < 0 || p.z < 0 || p.x >= s.x || p.z >= s.z || path[(p.z * s.x + p.x) as usize] && p != at {
                    break;
                }
                lay(&mut path, p.x, p.z);
                p += f.opposite().offset();
            }
        }
    }
    for x in 0..s.x {
        for z in 0..s.z {
            if path[(z * s.x + x) as usize] {
                d.block(IVec3::new(x, 0, z), m.path);
                for y in 1..s.y {
                    d.air(IVec3::new(x, y, z));
                }
            }
        }
    }
    // Decor stands on the verges beside the path.
    let mut verge = Vec::new();
    for x in 1..s.x - 1 {
        for z in 1..s.z - 1 {
            let on = |x: i32, z: i32| x >= 0 && z >= 0 && x < s.x && z < s.z && path[(z * s.x + x) as usize];
            if !on(x, z) && (on(x - 1, z) || on(x + 1, z) || on(x, z - 1) || on(x, z + 1)) {
                verge.push(IVec3::new(x, 0, z));
            }
        }
    }
    let ups = spec.conns.iter().filter(|c| c.up()).count();
    let mut floor: Vec<IVec3> = if verge.is_empty() {
        Vec::new()
    } else {
        spread(verge.len() as i32, ups.max(1)).into_iter().map(|i| verge[i as usize]).collect()
    };
    place_rest(spec, d, &done, 0, &mut floor);
}

fn farm(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let fr = Frame::new(front(spec), spec.size);
    let (w, dep) = (fr.w(), fr.d());
    let farmland = Block::WET_FARMLAND;
    // Water channels every four cells keep all the farmland hydrated.
    let channel = |u: i32| u > 0 && u < w - 1 && (u % 5 == 3 || w <= 6 && u == w / 2);
    let mut border = Vec::new();
    for u in 0..w {
        for v in 0..dep {
            let rim = u == 0 || u == w - 1 || v == 0 || v == dep - 1;
            let at = fr.at(u, 0, v);
            if rim {
                d.block(at, m.beam);
                border.push((u, v));
            } else if channel(u) {
                d.block(at, Block::WATER);
                d.air(at + IVec3::Y);
            } else {
                d.block(at, farmland);
                let age = (mix(&spec.location, u, v) % 8) as u8;
                d.block(at + IVec3::Y, Block::wheat(age));
            }
            for y in 2..fr.h().min(4) {
                d.air(fr.at(u, y, v));
            }
        }
    }
    // Composters (the farmer's job site) on the rim, hay at a corner.
    let rim_slots: Vec<(i32, i32)> = (1..dep - 1).flat_map(|v| [(0, v), (w - 1, v)]).collect();
    let jobs: Vec<(Block, i32)> = spec.counted("job").map(|(j, n)| (named(&j.replace('_', " ")), n)).collect();
    let mut i = 0;
    for (b, n) in jobs {
        for _ in 0..n {
            let (u, v) = rim_slots.get((i * 3 + 1) % rim_slots.len().max(1)).copied().unwrap_or((0, 0));
            d.block(fr.at(u, 1, v), b);
            i += 1;
        }
    }
    d.block(fr.at(w - 1, 1, dep - 1), named("hay bale"));
    let mut done = Vec::new();
    if let Some(i) = spec.conns.iter().position(is_entrance) {
        d.connect(fr.at(w / 2, 1, 0), &spec.conns[i]);
        done.push(i);
    }
    let mut floor: Vec<IVec3> = border.iter().map(|&(u, v)| fr.at(u, 0, v)).collect();
    place_rest(spec, d, &done, 1, &mut floor);
}

/// Animal pens and stables: a fenced yard with a gate on the entrance side,
/// hay and a trough; stables are roofed. Animals stand on the yard.
fn pen(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let fr = Frame::new(front(spec), spec.size);
    let (w, dep, h) = (fr.w(), fr.d(), fr.h());
    let stable = spec.kind == "stable";
    for u in 0..w {
        for v in 0..dep {
            let rim = u == 0 || u == w - 1 || v == 0 || v == dep - 1;
            d.block(fr.at(u, 0, v), if stable { m.floor } else { m.ground });
            for y in 1..h.min(5) {
                d.air(fr.at(u, y, v));
            }
            if rim {
                d.block(fr.at(u, 1, v), if stable && (u == 0 || u == w - 1) && v % 3 == 0 { m.beam } else { m.fence });
            }
        }
    }
    let gate = named("oak fence gate");
    d.facing(fr.at(w / 2, 1, 0), gate, fr.face(Facing::North));
    d.block(fr.at(1, 1, dep - 2), named("hay bale"));
    d.block(fr.at(w - 2, 1, dep - 2), named("water cauldron"));
    if stable && h >= 5 {
        for u in 0..w {
            for v in 0..dep {
                if (u == 0 || u == w - 1) && (v % 3 == 0 || v == dep - 1) {
                    for y in 2..4 {
                        d.block(fr.at(u, y, v), m.beam);
                    }
                }
                d.block(fr.at(u, 4, v), m.slab);
            }
        }
        d.block(fr.at(w / 2, 3, dep / 2), m.light);
    }
    let mut done = Vec::new();
    if let Some(i) = spec.conns.iter().position(is_entrance) {
        d.connect(fr.at(w / 2, 1, 0), &spec.conns[i]);
        done.push(i);
    }
    let mut floor = Vec::new();
    for v in (1..dep - 1).rev() {
        for u in 1..w - 1 {
            if (u, v) != (1, dep - 2) && (u, v) != (w - 2, dep - 2) && (u, v) != (w / 2, 1) {
                floor.push(fr.at(u, 0, v));
            }
        }
    }
    // Spread residents across the yard rather than packing one corner.
    let n = spec.conns.iter().filter(|c| c.up()).count().max(1);
    let picks: Vec<IVec3> = if floor.is_empty() {
        Vec::new()
    } else {
        spread(floor.len() as i32, n).into_iter().map(|i| floor[i as usize]).collect()
    };
    let mut floor = picks;
    place_rest(spec, d, &done, 1, &mut floor);
}

/// Meeting points off the streets (with a bell) and small accessories.
fn plaza(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let fr = Frame::new(front(spec), spec.size);
    let (w, dep, h) = (fr.w(), fr.d(), fr.h());
    for u in 0..w {
        for v in 0..dep {
            let rim = u == 0 || u == w - 1 || v == dep - 1;
            d.block(fr.at(u, 0, v), if rim { m.ground } else { m.path });
            for y in 1..h.min(5) {
                d.air(fr.at(u, y, v));
            }
        }
    }
    let mid = (w / 2, dep / 2);
    if spec.count("bell") > 0 {
        d.block(fr.at(mid.0, 0, mid.1), m.foundation);
        d.block(fr.at(mid.0, 1, mid.1), Block::BELL);
        if h >= 5 {
            for du in [-1, 1] {
                for y in 1..4 {
                    d.block(fr.at(mid.0 + du, y, mid.1), m.fence);
                }
            }
            for du in -1..=1 {
                d.block(fr.at(mid.0 + du, 4, mid.1), m.slab);
            }
        }
    } else {
        // An accessory: a bench and a planter.
        d.stairs(fr.at(mid.0, 1, mid.1), m.roof, fr.face(Facing::South));
        d.block(fr.at(mid.0, 1, (mid.1 + 1).min(dep - 1)), named("flower pot"));
    }
    for (u, v) in [(1, dep - 2), (w - 2, dep - 2)] {
        if u != mid.0 && w >= 5 && dep >= 5 {
            d.stairs(fr.at(u, 1, v), m.roof, fr.face(Facing::South));
        }
    }
    let mut done = Vec::new();
    if let Some(i) = spec.conns.iter().position(is_entrance) {
        d.connect(fr.at(w / 2, 1, 0), &spec.conns[i]);
        done.push(i);
    }
    let mut floor = Vec::new();
    for v in 1..dep - 1 {
        for u in 1..w - 1 {
            if (u - mid.0).abs() > 1 || (v - mid.1).abs() > 1 {
                floor.push(fr.at(u, 0, v));
            }
        }
    }
    let n = spec.conns.iter().filter(|c| c.up()).count().max(1);
    let mut floor: Vec<IVec3> = if floor.is_empty() {
        Vec::new()
    } else {
        spread(floor.len() as i32, n).into_iter().map(|i| floor[i as usize]).collect()
    };
    place_rest(spec, d, &done, 1, &mut floor);
}

/// Lamps: a post with a light, arms for the wider ones.
fn lamp(spec: &PieceSpec, d: &mut Draft) {
    let m = mat(style_of(spec));
    let s = spec.size;
    let base = IVec3::new(s.x / 2, 0, s.z / 2);
    let post = if style_of(spec) == "desert" { m.beam } else { m.fence };
    for y in 0..s.y - 1 {
        d.block(base.with_y(y), post);
    }
    d.block(base.with_y(s.y - 1), if s.y <= 2 { m.light } else { m.beam });
    if s.y > 2 {
        let mut lit = false;
        for f in [Facing::North, Facing::South, Facing::East, Facing::West] {
            let at = base.with_y(s.y - 2) + f.offset();
            if d.inside(at) && at.cmpge(IVec3::ZERO).all() && at.cmplt(s).all() {
                d.block(at, m.light);
                lit = true;
            }
        }
        if !lit {
            d.block(base.with_y(s.y - 1), m.light);
        }
    }
    for c in &spec.conns {
        d.connect(base, c);
    }
}

/// Taiga decorations: log piles, pumpkins, a campfire.
fn decoration(spec: &PieceSpec, d: &mut Draft) {
    let s = spec.size;
    let n = spec.base_name().chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1);
    for x in 0..s.x {
        for z in 0..s.z {
            let at = IVec3::new(x, 0, z);
            let b = match n {
                1 => named("spruce log"),
                2 => named("hay bale"),
                3 | 4 => {
                    if z == 0 {
                        Block::PUMPKIN
                    } else {
                        named("lantern")
                    }
                }
                5 => Block::PUMPKIN,
                _ => {
                    if x == s.x / 2 && z == s.z / 2 {
                        named("campfire")
                    } else {
                        named("spruce log")
                    }
                }
            };
            d.block(at, b);
            if s.y > 1 && n == 1 && (x + z) % 2 == 0 {
                d.block(at + IVec3::Y, named("spruce log"));
            }
        }
    }
    for c in &spec.conns {
        d.connect(IVec3::new(s.x / 2, 0, s.z / 2), c);
    }
}

/// The shaft under a town-centre well.
fn well_bottom(spec: &PieceSpec, d: &mut Draft) {
    let s = spec.size;
    for x in 0..s.x {
        for z in 0..s.z {
            for y in 0..s.y {
                let rim = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1 || y == 0;
                d.block(IVec3::new(x, y, z), if rim { Block::COBBLESTONE } else { Block::WATER });
            }
        }
    }
    for c in &spec.conns {
        d.connect(IVec3::new(s.x / 2, s.y - 1, s.z / 2), c);
    }
}

/// Feature elements: a 1×1×1 box whose tree or pile spills round it.
fn feature(spec: &PieceSpec, d: &mut Draft) {
    let name = spec.location.strip_prefix("feature:").unwrap_or(&spec.location);
    let o = IVec3::ZERO;
    let pile = |d: &mut Draft, b: Block, r: i32| {
        for x in -r..=r {
            for z in -r..=r {
                let h = if x.abs() + z.abs() <= 1 {
                    2
                } else if x.abs() + z.abs() <= r {
                    1
                } else {
                    0
                };
                for y in 0..h {
                    d.block(o + IVec3::new(x, y, z), b);
                }
            }
        }
    };
    let tree = |d: &mut Draft, log: Block, leaves: Block, height: i32, r: i32, cone: bool| {
        for y in 0..height {
            d.block(o.with_y(y), log);
        }
        for y in height - 3..=height {
            let rr = if cone {
                (height - y).clamp(0, r)
            } else if y == height {
                r - 1
            } else {
                r
            };
            for x in -rr..=rr {
                for z in -rr..=rr {
                    if (x != 0 || z != 0 || y >= height) && x.abs() + z.abs() <= rr + 1 {
                        d.block(o + IVec3::new(x, y, z), leaves);
                    }
                }
            }
        }
        if cone {
            d.block(o.with_y(height + 1), leaves);
        }
    };
    match name {
        "oak" => {
            d.overhang = 3;
            tree(d, Block::LOG, Block::LEAVES, 5, 2, false);
        }
        "spruce" | "pine" => {
            d.overhang = 3;
            tree(d, Block::SPRUCE_LOG, Block::SPRUCE_LEAVES, if name == "pine" { 7 } else { 6 }, 2, true);
        }
        "acacia" => {
            d.overhang = 3;
            tree(d, Block::ACACIA_LOG, named("acacia leaves"), 5, 2, false);
        }
        "pile_hay" => {
            d.overhang = 2;
            pile(d, named("hay bale"), 2);
        }
        "pile_melon" => {
            d.overhang = 1;
            pile(d, named("melon"), 1);
        }
        "pile_pumpkin" => {
            d.overhang = 1;
            pile(d, Block::PUMPKIN, 1);
        }
        "pile_snow" => {
            d.overhang = 2;
            pile(d, Block::SNOW, 2);
        }
        "pile_ice" => {
            d.overhang = 2;
            pile(d, named("packed ice"), 2);
        }
        // Plant patches keep to their own cell, on the ground the connector
        // stands on: a patch spread round it could float or bury itself on
        // a slope (Java's patches place each plant on the surface).
        "patch_cactus" => {
            d.block(o, Block::CACTUS);
            d.block(o + IVec3::Y, Block::CACTUS);
        }
        "flower_plain" => d.block(o, Block::DANDELION),
        "patch_berry_bush" => d.block(o, named("sweet berry bush")),
        "patch_taiga_grass" => d.block(o, named("fern")),
        other => panic!("no village feature drawing for {other}"),
    }
    for c in &spec.conns {
        d.connect(o, c);
    }
}

#[cfg(test)]
mod tests {
    use glam::{IVec2, IVec3};

    use super::*;
    use crate::world::fortress::Feature;
    use crate::world::jigsaw::{self, Base, Marker};
    use crate::world::terrain::{Dimension, Generation, Generator};
    use crate::world::village::{Resident, Style};

    const STYLES: [Style; 5] = [Style::Plains, Style::Desert, Style::Savanna, Style::Taiga, Style::Snowy];

    /// Java's job site blocks, one per profession (the fisherman's barrel
    /// is counted apart from the job sites in the piece lines).
    const JOBS: [&str; 13] = [
        "composter",
        "blast_furnace",
        "smoker",
        "cartography_table",
        "brewing_stand",
        "barrel",
        "fletching_table",
        "cauldron",
        "lectern",
        "stonecutter",
        "loom",
        "smithing_table",
        "grindstone",
    ];

    fn style_name(s: Style) -> &'static str {
        s.start_pool().split('/').nth(1).unwrap()
    }

    /// Every profession has a building with its fixed job site in every
    /// style's houses pool (the leatherworker's cauldron is Java's water
    /// cauldron, the fisherman's barrel a barrel).
    #[test]
    fn every_style_has_a_building_for_every_profession() {
        let lib = library();
        for style in STYLES {
            let pool = &lib.pools[&format!("village/{}/houses", style_name(style))];
            let mut found = std::collections::BTreeSet::new();
            for t in pool.elements.iter().filter_map(|e| e.0) {
                for c in &lib.templates[t as usize].cells {
                    if let Base::Block(b) = c.spec.base {
                        for job in JOBS {
                            let java = if job == "cauldron" { "water_cauldron" } else { job };
                            if jigsaw::rules::states_of(java).contains(&b.0)
                                || jigsaw::rules::states_of(job).contains(&b.0)
                            {
                                found.insert(job);
                            }
                        }
                    }
                }
            }
            assert_eq!(found.len(), JOBS.len(), "{style:?}: {found:?}");
        }
    }

    /// The abandoned variant: Java's four zombie town centres weigh 1 each
    /// against 50 for each ordinary one, and lead only to zombie pools.
    #[test]
    fn abandoned_villages_start_at_javas_two_percent_and_hold_zombie_villagers() {
        let lib = library();
        for style in STYLES {
            let pool = &lib.pools[style.start_pool()];
            let total: u32 = pool.elements.iter().map(|e| e.1).sum();
            let zombie: u32 = pool
                .elements
                .iter()
                .filter(|e| e.0.is_some_and(|t| lib.templates[t as usize].name.contains("/zombie/")))
                .map(|e| e.1)
                .sum();
            let share = zombie as f64 / total as f64;
            assert!((0.019..0.021).contains(&share), "{style:?}: {zombie}/{total}");
        }
        // Find an abandoned plains village on flat ground and check it.
        let settings = jigsaw::Settings { start_pool: Style::Plains.start_pool(), max_depth: 6, max_distance: 80 };
        let flat = |_: i32, _: i32| 70;
        let mut tried = 0;
        let pieces = (0u64..)
            .map(|seed| {
                tried += 1;
                jigsaw::assemble(lib, &settings, IVec2::ZERO, &flat, seed)
            })
            .find(|p| lib.templates[p[0].template as usize].name.contains("/zombie/"))
            .unwrap();
        assert!(tried < 2000);
        let mut zombies = 0;
        for m in jigsaw::markers(lib, &pieces, &flat) {
            if let Marker::Mob(_, name) = m {
                match Resident::parse(name).unwrap() {
                    Resident::Villager { .. } => panic!("a villager in an abandoned village"),
                    Resident::ZombieVillager { .. } => zombies += 1,
                    _ => {}
                }
            }
        }
        assert!(zombies > 0);
        // Every piece is processed as abandoned (or is a street or spawn piece).
        for p in &pieces {
            let t = &lib.templates[p.template as usize];
            let list = t.processors.map(|i| lib.processor_lists[i as usize].as_str());
            assert!(matches!(list, None | Some("zombie_plains" | "street_plains")), "{} with {list:?}", t.name);
        }
    }

    #[test]
    fn generation_4_villages_hold_job_sites_bells_residents_and_loot_in_every_style() {
        let g = Generator::with_generation(42, Dimension::Overworld, Generation::V4);
        let mut residents = std::collections::BTreeMap::new();
        for style in STYLES {
            let at = g.villages.locate_style(&g, IVec3::ZERO, style, 24).unwrap_or_else(|| panic!("{style:?}"));
            let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * crate::world::village::SPACING));
            let v = g.villages.get(&g, region).unwrap();
            assert!(v.v4 && v.placed.len() >= 8, "{style:?}: {} pieces", v.placed.len());
            for (i, a) in v.placed.iter().enumerate() {
                for b in &v.placed[i + 1..] {
                    let (ta, tb) =
                        (&library().templates[a.template as usize], &library().templates[b.template as usize]);
                    assert!(
                        !a.bounds.intersects(&b.bounds) || ta.overhang > 0 || tb.overhang > 0 || is_inside(a, b),
                        "{style:?}"
                    );
                }
            }
            let chunks: std::collections::BTreeSet<_> = v
                .placed
                .iter()
                .flat_map(|p| {
                    let lo = p.bounds.min.div_euclid(IVec3::splat(32));
                    let hi = p.bounds.max.div_euclid(IVec3::splat(32));
                    (lo.x..=hi.x)
                        .flat_map(move |x| (lo.y..=hi.y).flat_map(move |y| (lo.z..=hi.z).map(move |z| (x, y, z))))
                })
                .collect();
            let (mut jobs, mut bells, mut loot) = (0, 0, 0);
            for (x, y, z) in chunks {
                for (_, f) in g.villages.features(&g, IVec3::new(x, y, z)) {
                    match f {
                        Feature::VillageWorkstation(b) if b == Block::BELL => bells += 1,
                        Feature::VillageWorkstation(_) => jobs += 1,
                        Feature::StructureLoot(..) => loot += 1,
                        Feature::VillageResident(r) => *residents.entry(format!("{r:?}")).or_insert(0) += 1,
                        _ => {}
                    }
                }
            }
            println!("{style:?}: {} pieces, {jobs} job sites, {bells} bells, {loot} chests", v.placed.len());
            assert!(bells >= 1 && jobs >= 1, "{style:?}");
        }
        println!("{residents:?}");
        assert!(residents.keys().any(|r| r.starts_with("Villager")));
        assert!(residents.keys().any(|r| r.starts_with("Cat")));
        assert!(residents.keys().any(|r| r.starts_with("IronGolem")));
    }

    /// Children placed inside their parent (residents in a house) share its box.
    fn is_inside(a: &jigsaw::Placed, b: &jigsaw::Placed) -> bool {
        let inside = |x: &jigsaw::Placed, y: &jigsaw::Placed| {
            x.bounds.min.cmpge(y.bounds.min).all() && x.bounds.max.cmple(y.bounds.max).all()
        };
        inside(a, b) || inside(b, a)
    }

    #[test]
    fn generations_before_4_keep_their_village_templates() {
        let g = Generator::with_generation(42, Dimension::Overworld, Generation::V3);
        let at = g.villages.nearest(&g, IVec3::ZERO).unwrap();
        let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * crate::world::village::SPACING));
        let v = g.villages.get(&g, region).unwrap();
        assert!(!v.v4);
        assert!(std::ptr::eq(v.library(), jigsaw::villages()));
    }
}
