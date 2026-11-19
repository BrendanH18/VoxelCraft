//! Generation 4 trial chamber templates (v0.15 Track G, FE-02).
//!
//! `jigsaw/trial_chambers_v4.txt` lists every template location Java
//! 26.3's trial chamber pools name, with Java's size, connector graph (joint
//! types and priorities included), spawner configurations, vaults, loot
//! containers and decor, and Java's pools. This module draws an original
//! template for each: tuff and copper rooms whose doorways sit where the
//! structural connectors (corridors, hallways, chambers, entrance caps) are,
//! with the sub-pieces Java nests in them (chamber centres, slices and
//! quadrants, addons, reliefs, staircases, displays) given slots inside;
//! corridor slices stacked on floor plates; and the small pieces — spawner
//! plates and contents, vaults, supply chests, pots, candles, beds and
//! dispensers. Connectors keep Java's faces, tops and priorities; their
//! places along a face are VoxelCraft's design (face middles, so nested
//! pieces tile the way their sizes allow).

use std::sync::OnceLock;

use glam::IVec3;

use crate::world::block::{Block, Facing};
use crate::world::jigsaw::Library;
use crate::world::jigsaw::catalogue::{self, Conn, Draft, PieceSpec};
use crate::world::jigsaw::draw::{face_cell, face_len, mix, named, spread};

/// The generation 4 trial chamber library, built once.
pub fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        catalogue::build(include_str!("../jigsaw/trial_chambers_v4.txt"), &paint).unwrap_or_else(|e| panic!("{e}"))
    })
}

struct Mat {
    wall: Block,
    floor: Block,
    accent: Block,
    grate: Block,
    copper: Block,
    bulb: Block,
    stairs: Block,
    slab: Block,
}

fn mat() -> Mat {
    Mat {
        wall: named("tuff bricks"),
        floor: named("polished tuff"),
        accent: named("chiseled tuff"),
        grate: named("waxed copper grate"),
        copper: named("waxed cut copper"),
        bulb: named("waxed copper bulb"),
        stairs: named("tuff brick stairs"),
        slab: named("tuff brick slab"),
    }
}

/// Pools whose pieces fill a room's inside (sub-pieces and contents).
fn contents_pool(pool: &str) -> bool {
    ["reward/", "spawner/", "decor", "dispensers/", "chests/"].iter().any(|p| pool.contains(p))
}

/// Pools whose pieces continue the structure through a doorway.
fn structural_pool(pool: &str) -> bool {
    matches!(
        pool,
        "trial_chambers/corridor"
            | "trial_chambers/chambers/end"
            | "trial_chambers/hallway"
            | "trial_chambers/hallway/fallback"
            | "trial_chambers/chamber/entrance_cap"
    )
}

/// The connector a piece hangs from (Java's pools never send children
/// through it).
fn attach(c: &Conn) -> bool {
    c.pool == "empty" && c.name != "empty"
}

/// Rooms with a shell of their own (the rest sit inside one).
fn shell(kind: &str) -> bool {
    matches!(kind, "corridor_end" | "atrium" | "entrance" | "intersection" | "hallway" | "chamber")
}

/// Pieces whose connectors stand on a floor at height 1, inside walls
/// (shells, and corridor slices, whose side walls are their own).
fn roomy(kind: &str) -> bool {
    shell(kind) || kind == "corridor_slice"
}

/// Connectors that join floor plates: at floor level, so a corridor's floor
/// runs level with the rooms it joins.
fn plate_joint(c: &Conn) -> bool {
    c.pool == "trial_chambers/corridor"
        || attach(c)
            && matches!(c.name.as_str(), "first_corridor" | "second_corridor" | "intersection" | "atrium" | "entrance")
}

pub fn paint(spec: &PieceSpec, d: &mut Draft) {
    match spec.kind.as_str() {
        "decor" => decor(spec, d),
        "spawner" => spawner(spec, d),
        "spawner_connector" | "chest_connector" => plate_piece(spec, d),
        "vault" => vault(spec, d),
        "supply" => supply(spec, d),
        "dispenser" => dispenser(spec, d),
        "cap" => cap(spec, d),
        _ => room(spec, d),
    }
}

/// A connector's height in a room: doorways and sub-pieces stand on the
/// floor (1), spawner and chest plates two up so their contents sit on it;
/// a staircase hallway's two ends differ by its rise.
fn side_height(spec: &PieceSpec, c: &Conn, shelled: bool) -> i32 {
    let base = if shelled { 1 } else { 0 };
    let tail = c.target.as_str();
    if matches!(tail, "spawner_connector" | "chest_connector") {
        // The plate two up, so its contents (two high) sit in the piece.
        return 2.min(spec.size.y - 1);
    }
    if plate_joint(c) {
        return 0;
    }
    // Staircases and the corridor connector shafts climb or drop by their
    // height less a hallway's (7): `_down` staircases and the lower shafts
    // (which pass under the corridor floor) enter high and leave low, the
    // rest enter low and leave high (the upper shaft passes over the
    // corridor's roof).
    let rise = (spec.size.y - 7).max(0);
    let name = spec.base_name();
    let climbs = name.contains("staircase") || name.contains("hallway_connector");
    if spec.kind == "hallway" && rise > 0 && climbs {
        let down = name.contains("down") || name.starts_with("lower") || name.starts_with("trapped");
        let upper = attach(c) == down;
        return if upper { base + rise } else { base };
    }
    if c.name.contains("upper") || c.target.contains("upper") {
        return (spec.size.y - 6).max(base);
    }
    base
}

/// Every room-like piece: an optional shell with doorways at its structural
/// connectors, a floor, lights, and slots for its sub-pieces and contents.
fn room(spec: &PieceSpec, d: &mut Draft) {
    let m = mat();
    let s = spec.size;
    let shelled = shell(&spec.kind);
    let seed = mix(&spec.location, s.x, s.z);
    match spec.kind.as_str() {
        "plate" => d.fill(IVec3::ZERO, IVec3::new(s.x - 1, 0, s.z - 1), m.floor),
        "corridor_slice" => {
            // Side walls, floor and roof; open where it meets the next slice.
            for z in 0..s.z {
                for y in 0..s.y {
                    for x in 0..s.x {
                        let p = IVec3::new(x, y, z);
                        let b = if y == 0 {
                            if x % 6 == 3 { m.accent } else { m.floor }
                        } else if y == s.y - 1 || x == 0 || x == s.x - 1 {
                            m.wall
                        } else if (y == 7 || y == 13) && (x <= 3 || x >= s.x - 4) {
                            m.grate
                        } else {
                            Block::AIR
                        };
                        d.block(p, b);
                    }
                }
                for x in [1, s.x - 2] {
                    if z == s.z / 2 {
                        d.block(IVec3::new(x, 4, z), m.bulb);
                        d.block(IVec3::new(x, 10, z), m.bulb);
                    }
                }
            }
        }
        _ if shelled => {
            for z in 0..s.z {
                for y in 0..s.y {
                    for x in 0..s.x {
                        let edge = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
                        let b = if y == 0 {
                            if !edge && (x + z) % 4 == 0 { m.accent } else { m.floor }
                        } else if y == s.y - 1 || edge {
                            if edge && y % 4 == 3 && (x + z) % 5 == 2 { m.bulb } else { m.wall }
                        } else {
                            Block::AIR
                        };
                        d.block(IVec3::new(x, y, z), b);
                    }
                }
            }
            // A copper band under the roof.
            if s.y >= 8 {
                for x in 1..s.x - 1 {
                    for z in [1, s.z - 2] {
                        d.block(IVec3::new(x, s.y - 2, z), m.copper);
                    }
                }
            }
        }
        _ => part(spec, d, &m, seed),
    }
    if spec.kind == "hallway" && s.y > 7 {
        stairs(spec, d, &m);
    }
    // Beds, chests and blocks Java fixes in these rooms.
    let mut slots = interior_slots(s, roomy(&spec.kind));
    for _ in 0..spec.count("bed") {
        if let Some(p) = slots.pop() {
            d.bed(p, Facing::East);
            slots.retain(|q| *q != p + IVec3::X);
        }
    }
    for (loot, n) in spec.counted("loot") {
        let (container, table) = loot.split_once(':').unwrap_or(("chest", loot));
        for _ in 0..n {
            let p = slots.pop().unwrap_or(IVec3::new(s.x / 2, 1.min(s.y - 1), s.z / 2));
            loot_at(d, p, container, table);
        }
    }
    let plain =
        |k: &str| spec.count(k) - spec.counted("loot").filter(|(l, _)| l.starts_with(k)).map(|(_, n)| n).sum::<i32>();
    for (block, n) in [("barrel", plain("barrel")), ("decorated pot", plain("decorated_pot"))] {
        for _ in 0..n {
            if let Some(p) = slots.pop() {
                d.block(p, named(block));
            }
        }
    }
    for (b, n) in spec.counted("block") {
        for _ in 0..n {
            if let Some(p) = slots.pop() {
                d.block(p, named(&b.replace('_', " ")));
            }
        }
    }
    connectors(spec, d, roomy(&spec.kind));
}

/// Floor cells along a room's walls, for fixed furniture.
fn interior_slots(s: IVec3, shelled: bool) -> Vec<IVec3> {
    let (lo, y) = if shelled { (2, 1) } else { (1, 0) };
    let mut out = Vec::new();
    for x in lo..s.x - lo {
        if s.z > 2 * lo {
            out.push(IVec3::new(x, y.min(s.y - 1), lo.min(s.z - 1)));
        }
    }
    out.reverse();
    out
}

fn loot_at(d: &mut Draft, p: IVec3, container: &str, table: &str) {
    match container {
        "chest" => d.loot(p, table, Facing::South),
        "dispenser" => d.loot_in(p, table, crate::world::redstone_blocks::dispenser(5, false), Facing::South),
        other => d.loot_in(p, table, named(&other.replace('_', " ")), Facing::South),
    }
}

/// A staircase hallway: a ramp of steps from its low doorway to its high one.
fn stairs(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    let s = spec.size;
    let rise = s.y - 7;
    let run = s.z.max(s.x) - 2;
    let along_z = s.z >= s.x;
    for i in 0..run {
        let h = 1 + (i * rise) / run.max(1);
        for w in 1..(if along_z { s.x } else { s.z }) - 1 {
            for y in 1..=h {
                let p = if along_z { IVec3::new(w, y, i + 1) } else { IVec3::new(i + 1, y, w) };
                d.block(p, if y == h { m.stairs } else { m.wall });
            }
        }
    }
}

/// The open pieces that sit inside a room: chamber centres, slices,
/// quadrants, arms, addons, reliefs and staircases.
fn part(spec: &PieceSpec, d: &mut Draft, m: &Mat, seed: u64) {
    let s = spec.size;
    let name = spec.base_name();
    let top = s.y - 1;
    if name.contains("relief") {
        // A carved wall: chiseled tuff in a pattern on the back face.
        for y in 0..s.y {
            for z in 0..s.z {
                let b = if (y + z + seed as i32 % 3) % 3 == 0 { m.accent } else { m.wall };
                d.block(IVec3::new(s.x - 1, y, z), b);
            }
        }
    } else if name.contains("staircase") {
        let run = s.z.max(s.x);
        for i in 0..run {
            let h = (i * top / run.max(1)).min(top);
            for w in 0..s.x.min(s.z) {
                let p = if s.z >= s.x { IVec3::new(w, h, i) } else { IVec3::new(i, h, w) };
                d.block(p, m.stairs);
                for y in 0..h {
                    d.block(p.with_y(y), m.wall);
                }
            }
        }
    } else if name.starts_with("hanging") || name.contains("chandelier") || name.contains("head_upper") {
        // Hung from above: a chain and a lamp.
        let c = IVec3::new(s.x / 2, 0, s.z / 2);
        for y in 1..s.y {
            d.block(c.with_y(y), named("chain"));
        }
        d.block(c, m.bulb);
    } else if name.contains("column") {
        for y in 0..s.y {
            d.block(IVec3::new(s.x / 2, y, s.z / 2), if y % 4 == 3 { m.bulb } else { m.wall });
        }
    } else if name.contains("ladder") {
        for y in 0..s.y {
            d.facing(IVec3::new(s.x / 2, y, s.z / 2), Block::LADDER, Facing::North);
        }
    } else if name.starts_with("display") {
        let c = IVec3::new(s.x / 2, 0, s.z / 2);
        d.block(c, m.accent);
        d.block(c + IVec3::Y, Block::from_name("decorated pot").unwrap_or(m.accent));
    } else if name.contains("wall") && !name.contains("walkway") {
        d.fill(IVec3::ZERO, IVec3::new(s.x - 1, top, 0), m.wall);
    } else if spec.kind == "chamber_addon" || spec.kind == "corridor_addon" {
        // A walkway: a grate deck along the top with a tuff edge, a ladder
        // up one side.
        for x in 0..s.x {
            for z in 0..s.z {
                let edge = x == 0 || x == s.x - 1;
                d.block(IVec3::new(x, top, z), if edge { m.wall } else { m.grate });
            }
        }
        if top > 1 {
            for y in 0..top {
                d.facing(IVec3::new(s.x / 2, y, 0), Block::LADDER, Facing::South);
            }
        }
    } else {
        // Chamber floor pieces: a patterned floor, pillars at the corners
        // and a light.
        for x in 0..s.x {
            for z in 0..s.z {
                let b = if (x + z) % 3 == (seed % 3) as i32 { m.accent } else { m.floor };
                d.block(IVec3::new(x, 0, z), b);
            }
        }
        if s.x >= 5 && s.z >= 5 && s.y >= 6 {
            for (x, z) in [(0, 0), (s.x - 1, 0), (0, s.z - 1), (s.x - 1, s.z - 1)] {
                if mix(&spec.location, x, z).is_multiple_of(3) {
                    for y in 1..4 {
                        d.block(IVec3::new(x, y, z), if y == 3 { m.bulb } else { m.wall });
                    }
                }
            }
        }
        if name.contains("ramp") || name.contains("arm") {
            for z in 0..s.z {
                let h = (z * top / s.z.max(1)).min(top);
                for x in 0..s.x {
                    d.block(IVec3::new(x, h, z), m.slab);
                }
            }
        }
    }
}

/// Places every connector: structural and own connectors on their face
/// (cutting a doorway in a shell), contents and sub-pieces inside, floor
/// pieces on the floor and hanging pieces under the roof.
fn connectors(spec: &PieceSpec, d: &mut Draft, shelled: bool) {
    let s = spec.size;
    let inner = |c: &Conn| !attach(c) && (contents_pool(&c.pool) || shelled && !structural_pool(&c.pool));
    // Side connectors on faces, grouped so equal connectors share a cell.
    let mut faces: Vec<(Facing, Vec<&Conn>)> = Vec::new();
    for c in spec.conns.iter().filter(|c| c.side() && !inner(c)) {
        let f = c.facing.unwrap();
        match faces.iter_mut().find(|(g, _)| *g == f) {
            Some((_, v)) => v.push(c),
            None => faces.push((f, vec![c])),
        }
    }
    for (f, cs) in faces {
        let len = face_len(s, f);
        let mut kinds: Vec<(&str, &str, &str)> =
            cs.iter().map(|c| (c.name.as_str(), c.target.as_str(), c.pool.as_str())).collect();
        kinds.dedup();
        kinds.sort();
        kinds.dedup();
        let lo = if shelled { 2 } else { 0 };
        let span = (len - 2 * lo).max(1);
        let spots: Vec<i32> = spread(span, kinds.len()).into_iter().map(|t| t + lo).collect();
        for c in cs {
            let k = kinds.iter().position(|k| *k == (c.name.as_str(), c.target.as_str(), c.pool.as_str())).unwrap();
            let t = if kinds.len() == 1 { len / 2 } else { spots[k] };
            let y = side_height(spec, c, shelled);
            let at = face_cell(s, f, t, y.min(s.y - 1));
            d.connect(at, c);
            if shelled && s.y > 4 {
                // A doorway three wide and three high.
                for dt in -1..=1 {
                    for dy in 0..3 {
                        let p = face_cell(s, f, (t + dt).clamp(1, len - 2), (y.max(1) + dy).min(s.y - 2));
                        d.air(p);
                    }
                }
            }
        }
    }
    // Inner side connectors: against the wall behind them, facing in.
    let lo = if shelled { 1 } else { 0 };
    let mut used: Vec<IVec3> = Vec::new();
    let inners: Vec<&Conn> = spec.conns.iter().filter(|c| c.side() && inner(c)).collect();
    for f in [Facing::North, Facing::South, Facing::East, Facing::West] {
        let group: Vec<&&Conn> = inners.iter().filter(|c| c.facing == Some(f)).collect();
        if group.is_empty() {
            continue;
        }
        // The wall the connectors stand against is the face opposite `f`.
        let back = f.opposite();
        let len = face_len(s, back);
        let span = (len - 2 * lo).max(1);
        let spots = spread(span, group.len());
        for (c, t) in group.into_iter().zip(spots) {
            let y = side_height(spec, c, shelled);
            let mut at = face_cell(s, back, t + lo, y.min(s.y - 1));
            if shelled {
                at += f.offset();
            }
            if used.contains(&at) && s.y > y + 4 {
                at.y += 4;
            }
            used.push(at);
            d.connect(at, c);
        }
    }
    // Up and down connectors.
    let centre = IVec3::new(s.x / 2, 0, s.z / 2);
    let ups: Vec<&Conn> = spec.conns.iter().filter(|c| c.up()).collect();
    let downs: Vec<&Conn> = spec.conns.iter().filter(|c| c.down()).collect();
    let floor_y = 0;
    let roof_y = s.y - 1;
    let grid = |n: usize, y: i32, edge: bool| -> Vec<IVec3> {
        let inset = if shelled { 2 } else { 0 };
        let w = (s.x - 2 * inset).max(1);
        let dd = (s.z - 2 * inset).max(1);
        if edge {
            // Spread round the walls.
            let mut ring = Vec::new();
            for x in 0..w {
                ring.push(IVec3::new(inset + x, y, inset));
            }
            for z in 1..dd {
                ring.push(IVec3::new(inset + w - 1, y, inset + z));
            }
            for x in (0..w - 1).rev() {
                ring.push(IVec3::new(inset + x, y, inset + dd - 1));
            }
            for z in (1..dd - 1).rev() {
                ring.push(IVec3::new(inset, y, inset + z));
            }
            return spread(ring.len() as i32, n).into_iter().map(|i| ring[i as usize]).collect();
        }
        let cols = (n as f64).sqrt().ceil() as i32;
        let rows = (n as i32 + cols - 1) / cols.max(1);
        let xs = spread(w, cols as usize);
        let zs = spread(dd, rows as usize);
        let mut out = Vec::new();
        for z in &zs {
            for x in &xs {
                out.push(IVec3::new(inset + x, y, inset + z));
            }
        }
        out.truncate(n);
        out
    };
    let mut by_pool: Vec<(&str, Vec<&Conn>)> = Vec::new();
    for c in ups.iter().chain(downs.iter()) {
        let key = c.pool.as_str();
        match by_pool.iter_mut().find(|(k, v)| *k == key && v[0].vertical == c.vertical && attach(v[0]) == attach(c)) {
            Some((_, v)) => v.push(c),
            None => by_pool.push((key, vec![c])),
        }
    }
    for (pool, cs) in by_pool {
        let c0 = cs[0];
        let positions: Vec<IVec3> = if attach(c0) {
            // The piece's own vertical connector: bottom (it stands on its
            // parent) or top (it hangs from it), centred.
            let y = if c0.up() { roof_y } else { floor_y };
            vec![centre.with_y(y); cs.len()]
        } else if c0.up() {
            let y = if shelled || spec.kind == "plate" { floor_y } else { roof_y };
            let y = if spec.kind == "corridor_slice" { floor_y } else { y };
            if pool.ends_with("/slices") {
                // Slices stack along the plate, one per five blocks.
                (0..cs.len() as i32).map(|i| IVec3::new(s.x / 2, y, 2 + 5 * i)).collect()
            } else if cs.len() == 1 && !pool.contains("disposal") {
                vec![centre.with_y(y)]
            } else {
                grid(cs.len(), y, pool.contains("disposal"))
            }
        } else {
            grid(cs.len(), roof_y, pool.contains("dispensers"))
        };
        for (c, p) in cs.into_iter().zip(positions) {
            d.connect(p, c);
        }
    }
}

/// Spawner and chest plates: no blocks, a connector each way.
fn plate_piece(spec: &PieceSpec, d: &mut Draft) {
    let s = spec.size;
    for c in &spec.conns {
        if c.side() {
            d.connect(face_cell(s, c.facing.unwrap(), s.x / 2, 0), c);
        } else {
            d.connect(IVec3::new(s.x / 2, 0, s.z / 2), c);
        }
    }
}

/// Spawner contents: a floor, the trial spawner of Java's configuration in
/// the middle, and Java's cobwebs round it for the spiders.
fn spawner(spec: &PieceSpec, d: &mut Draft) {
    let m = mat();
    let s = spec.size;
    d.fill(IVec3::ZERO, IVec3::new(s.x - 1, 0, s.z - 1), m.floor);
    let c = IVec3::new(s.x / 2, 1, s.z / 2);
    for config in spec.params_named("trial_spawner_config") {
        d.spawner(c, config);
    }
    let webs =
        [IVec3::new(0, 1, 0), IVec3::new(2, 1, 2), IVec3::new(0, 1, 2), IVec3::new(2, 1, 0), IVec3::new(1, 1, 0)];
    for p in webs.into_iter().take(spec.count("cobweb") as usize) {
        d.block(p, Block::COBWEB);
    }
    for conn in &spec.conns {
        d.connect(c, conn);
    }
}

/// A vault on a small dais, facing out of its connector side.
fn vault(spec: &PieceSpec, d: &mut Draft) {
    let m = mat();
    let s = spec.size;
    let ominous = spec.params_named("vault_loot").any(|v| v.ends_with(":ominous"));
    let at = IVec3::new(s.x / 2, if s.y >= 4 { 1 } else { 0 }, s.z / 2);
    if at.y == 1 {
        d.block(at.with_y(0), m.accent);
    }
    d.facing(at, if ominous { crate::world::trial::OMINOUS_VAULT } else { crate::world::trial::VAULT }, Facing::North);
    for c in &spec.conns {
        d.connect(face_cell(s, c.facing.unwrap_or(Facing::North), s.x / 2, 0), c);
    }
}

/// The supply chest under its plate.
fn supply(spec: &PieceSpec, d: &mut Draft) {
    let m = mat();
    let s = spec.size;
    d.fill(IVec3::ZERO, IVec3::new(s.x - 1, 0, s.z - 1), m.floor);
    let c = IVec3::new(s.x / 2, 1, s.z / 2);
    for (loot, _) in spec.counted("loot") {
        let (container, table) = loot.split_once(':').unwrap();
        loot_at(d, c, container, table);
    }
    for conn in &spec.conns {
        d.connect(c, conn);
    }
}

/// Dispensers: in the roof facing down, or in a wall or floor facing out.
fn dispenser(spec: &PieceSpec, d: &mut Draft) {
    let s = spec.size;
    let hanging = spec.conns.iter().any(|c| c.up());
    let at = if hanging { IVec3::new(s.x / 2, 0, s.z / 2) } else { IVec3::new(0, 0, 0) };
    for (loot, _) in spec.counted("loot") {
        let (_, table) = loot.split_once(':').unwrap();
        let facing = if hanging { 5 } else { 1 };
        d.loot_in(at, table, crate::world::redstone_blocks::dispenser(facing, false), Facing::North);
    }
    for c in &spec.conns {
        if c.side() {
            d.connect(face_cell(s, c.facing.unwrap(), 0, 0), c);
        } else {
            d.connect(IVec3::new(s.x / 2, s.y - 1, s.z / 2), c);
        }
    }
}

/// An entrance cap: a tuff frame round a closed copper door.
fn cap(spec: &PieceSpec, d: &mut Draft) {
    let m = mat();
    let s = spec.size;
    for x in 0..s.x {
        for y in 0..s.y {
            let edge = x == 0 || x == s.x - 1 || y == 0 || y == s.y - 1;
            d.block(IVec3::new(x, y, 0), if edge { m.wall } else { m.grate });
        }
    }
    for c in &spec.conns {
        d.connect(IVec3::new(s.x / 2, 1, 0), c);
    }
}

/// Decor: pots (with Java's corridor loot where Java has it), candles,
/// barrels and beds.
fn decor(spec: &PieceSpec, d: &mut Draft) {
    let name = spec.base_name();
    let at = IVec3::ZERO;
    if let Some((loot, _)) = spec.counted("loot").next() {
        let (container, table) = loot.split_once(':').unwrap();
        loot_at(d, at, container, table);
    } else if name.ends_with("_bed") {
        d.bed(at, Facing::East);
    } else if let Some(n) = name.strip_prefix("candle_").and_then(|n| n.parse::<u8>().ok()) {
        d.block(at, crate::world::colour_blocks::candle::candle(0, n, false, false));
    } else if name == "barrel" {
        d.block(at, named("barrel"));
    } else if name == "dead_bush_pot" {
        d.block(at, named("potted dead bush"));
    } else if name == "empty_pot" {
        d.block(at, named("flower pot"));
    } else if name == "disposal" {
        let s = spec.size;
        d.block(IVec3::new(0, 0, s.z / 2), named("barrel"));
        for y in 1..s.y - 1 {
            d.block(IVec3::new(0, y, s.z / 2), mat().grate);
        }
    } else {
        d.block(at, named("decorated pot"));
    }
    let s = spec.size;
    for c in &spec.conns {
        if c.side() {
            d.connect(face_cell(s, c.facing.unwrap(), if c.facing.unwrap().along_x() { 0 } else { s.x - 1 }, 0), c);
        } else {
            d.connect(IVec3::new(0, if c.up() { s.y - 1 } else { 0 }, s.z / 2), c);
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::{IVec2, IVec3};

    use super::*;
    use crate::world::fortress::Feature;
    use crate::world::terrain::{Dimension, Generation, Generator};
    use crate::world::trial_chambers::SPACING;

    #[test]
    fn a_generation_4_chamber_holds_javas_spawners_vaults_and_loot() {
        let g = Generator::with_generation(12345, Dimension::Overworld, Generation::V4);
        let at = g.trial_chambers.nearest(&g, IVec2::ZERO, 6400).expect("a chamber within 6400 blocks");
        let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * SPACING));
        let c = g.trial_chambers.get(&g, region).unwrap();
        assert!(c.v4);
        let lib = library();
        let names: Vec<&str> = c.pieces.iter().map(|p| lib.templates[p.template as usize].name.as_str()).collect();
        println!("generation 4 trial chamber at {at}: {} pieces", c.pieces.len());
        assert!(names[0].starts_with("trial_chambers/corridor/end_"));
        assert!(c.pieces.len() >= 30, "{} pieces", c.pieces.len());
        // Pieces overlap only where one sits inside the other (Java's
        // nested chamber parts and room contents).
        for (i, a) in c.pieces.iter().enumerate() {
            for b in &c.pieces[i + 1..] {
                let inside = |x: &crate::world::jigsaw::Placed, y: &crate::world::jigsaw::Placed| {
                    x.bounds.min.cmpge(y.bounds.min).all() && x.bounds.max.cmple(y.bounds.max).all()
                };
                assert!(!a.bounds.intersects(&b.bounds) || inside(a, b) || inside(b, a), "{a:?} {b:?}");
            }
        }
        let bounds = c.bounds();
        let (mut spawners, mut vaults, mut loot) = (0, 0, 0);
        for cx in bounds.min.x.div_euclid(32)..=bounds.max.x.div_euclid(32) {
            for cy in bounds.min.y.div_euclid(32)..=bounds.max.y.div_euclid(32) {
                for cz in bounds.min.z.div_euclid(32)..=bounds.max.z.div_euclid(32) {
                    for (_, f) in g.trial_chambers.features(&g, IVec3::new(cx, cy, cz)) {
                        match f {
                            Feature::TrialSpawner(_) => spawners += 1,
                            Feature::TrialVault => vaults += 1,
                            Feature::TrialLoot(..) | Feature::StructureLoot(..) => loot += 1,
                            _ => {}
                        }
                    }
                }
            }
        }
        println!("{spawners} spawners, {vaults} vaults, {loot} containers");
        let mut kinds = std::collections::BTreeMap::new();
        for n in &names {
            let key: String =
                n.trim_start_matches("trial_chambers/").splitn(3, '/').take(2).collect::<Vec<_>>().join("/");
            *kinds.entry(key).or_insert(0) += 1;
        }
        println!("{kinds:?}");
        assert!(spawners >= 3 && vaults >= 1 && loot >= 1);
        assert!(names.iter().any(|n| n.starts_with("trial_chambers/chamber/")), "no chamber");
    }

    /// `Aliases::pool` against the structure's `pool_aliases` in the jar
    /// fixture: the same aliases, the same targets in Java's order (one
    /// random group shared by ranged and slow ranged).
    #[test]
    fn spawner_aliases_match_javas_pool_aliases() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/structure_catalogues.json")).unwrap();
        let aliases = f["structures"]["trial_chambers"]["pool_aliases"].as_array().unwrap();
        let strip = |s: &serde_json::Value| s.as_str().unwrap().strip_prefix("minecraft:").unwrap().to_string();
        let mut declared: Vec<String> = library().aliases.clone();
        let mut java = Vec::new();
        for a in aliases {
            match a["type"].as_str().unwrap() {
                "minecraft:random_group" => {
                    for (i, group) in a["groups"].as_array().unwrap().iter().enumerate() {
                        let choice =
                            crate::world::trial_chambers::Aliases { ranged: i as u8, melee: 0, small_melee: 0 };
                        for d in group["data"].as_array().unwrap() {
                            let alias = strip(&d["alias"]);
                            assert_eq!(choice.pool(&alias), strip(&d["target"]), "{alias} {i}");
                            if i == 0 {
                                java.push(alias);
                            }
                        }
                    }
                }
                "minecraft:random" => {
                    let alias = strip(&a["alias"]);
                    for (i, t) in a["targets"].as_array().unwrap().iter().enumerate() {
                        let choice = if alias.ends_with("/small_melee") {
                            crate::world::trial_chambers::Aliases { ranged: 0, melee: 0, small_melee: i as u8 }
                        } else {
                            crate::world::trial_chambers::Aliases { ranged: 0, melee: i as u8, small_melee: 0 }
                        };
                        assert_eq!(choice.pool(&alias), strip(&t["data"]), "{alias} {i}");
                    }
                    java.push(alias);
                }
                other => panic!("alias type {other}"),
            }
        }
        declared.sort();
        java.sort();
        assert_eq!(declared, java);
    }

    #[test]
    fn spawner_aliases_name_real_pools() {
        let lib = library();
        for ranged in 0..3 {
            for melee in 0..3 {
                for small_melee in 0..4 {
                    let a = crate::world::trial_chambers::Aliases { ranged, melee, small_melee };
                    for kind in ["ranged", "slow_ranged", "melee", "small_melee", "breeze"] {
                        let pool = a.pool(&format!("trial_chambers/spawner/contents/{kind}")).to_string();
                        assert!(lib.pools.contains_key(&pool), "{pool}");
                    }
                }
            }
        }
    }
}
