//! Generation 4 ancient city templates (v0.15 Track G, FE-02).
//!
//! `jigsaw/ancient_city_v4.txt` lists every template location Java 26.3's
//! ancient city pools name — the three city centres, the ten pieces that
//! wall the centre in, the entrance paths, the city walls (the corners,
//! intersections, stairs, passages, the bridge and the ruined walls, and
//! the `no_corners` pool, whose `intact_horizontal_wall_stairs_5` the jar
//! lacks), the structures (barracks, chambers, sauna, statue, ruins, tall
//! ruins, pillars, the three camps and the ice box) and the sculk patch
//! feature — with Java's sizes, connectors, chests and their loot. This
//! module draws an original template for each: deepslate, reinforced
//! deepslate, basalt, wool and soul light, with the portal frame and a
//! hidden room (lectern and, in one centre, a chest) in every city centre.
//! Every floor is the template's bottom layer and every horizontal
//! connector stands on it, so the whole city keeps one walk level.

use std::sync::OnceLock;

use glam::IVec3;

use crate::world::block::{Block, Facing};
use crate::world::jigsaw::Library;
use crate::world::jigsaw::catalogue::{self, Conn, Draft, PieceSpec};
use crate::world::jigsaw::draw::{face_cell, face_len, mix, named, spread};

/// The city anchor's height above the centre's floor (Java's anchor at
/// -27 over a walk level of -51).
pub const ANCHOR_Y: i32 = 25;

/// The generation 4 ancient city library, built once.
pub fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        catalogue::build(include_str!("../jigsaw/ancient_city_v4.txt"), &paint).unwrap_or_else(|e| panic!("{e}"))
    })
}

struct Mat {
    bricks: Block,
    tiles: Block,
    polished: Block,
    cobbled: Block,
    chiseled: Block,
    brick_stairs: Block,
    tile_slab: Block,
    wall: Block,
    basalt: Block,
    wool: Block,
    reinforced: Block,
    lantern: Block,
    dark_planks: Block,
    dark_fence: Block,
}

fn mat() -> Mat {
    Mat {
        bricks: named("deepslate bricks"),
        tiles: named("deepslate tiles"),
        polished: named("polished deepslate"),
        cobbled: named("cobbled deepslate"),
        chiseled: named("chiseled deepslate"),
        brick_stairs: named("deepslate brick stairs"),
        tile_slab: named("deepslate tile slab"),
        wall: named("deepslate brick wall"),
        basalt: named("polished basalt"),
        wool: named("gray wool"),
        reinforced: crate::world::jukebox::REINFORCED_DEEPSLATE,
        lantern: crate::world::mount_states::lantern_block(true, false, false),
        dark_planks: named("dark oak planks"),
        dark_fence: named("dark oak fence"),
    }
}

pub fn paint(spec: &PieceSpec, d: &mut Draft) {
    let m = mat();
    match spec.kind.as_str() {
        "feature" => {
            // The sculk patch: grown when the city is laid out
            // (`City::build`); the template itself places nothing.
            for c in &spec.conns {
                d.connect(IVec3::ZERO, c);
            }
            return;
        }
        "center" => center(spec, d, &m),
        "center_wall" => center_wall(spec, d, &m),
        "entrance" => entrance(spec, d, &m),
        "wall" => wall(spec, d, &m),
        "structure" => structure(spec, d, &m),
        other => panic!("{}: no ancient city painter for {other}", spec.location),
    }
    connectors(spec, d);
}

/// A tiled floor over the whole box, with chiseled inlays.
fn floor(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    let s = spec.size;
    for x in 0..s.x {
        for z in 0..s.z {
            let b = if (x * 7 + z * 3) % 11 == 0 {
                m.chiseled
            } else if (x + z) % 2 == 0 {
                m.tiles
            } else {
                m.polished
            };
            d.block(IVec3::new(x, 0, z), b);
        }
    }
}

/// The horizontal connectors stand on the floor at the middle of their
/// face; equal connectors share a cell. The city anchor sits over the
/// centre, the sculk patch connectors on the floor.
fn connectors(spec: &PieceSpec, d: &mut Draft) {
    let s = spec.size;
    let mut faces: Vec<(Facing, Vec<&Conn>)> = Vec::new();
    for c in spec.conns.iter().filter(|c| c.side()) {
        let f = c.facing.unwrap();
        match faces.iter_mut().find(|(g, _)| *g == f) {
            Some((_, v)) => v.push(c),
            None => faces.push((f, vec![c])),
        }
    }
    for (f, cs) in faces {
        let len = face_len(s, f);
        let mut kinds: Vec<&str> = cs.iter().map(|c| c.pool.as_str()).collect();
        kinds.sort();
        kinds.dedup();
        let spots = spread(len, kinds.len());
        for c in cs {
            let k = kinds.iter().position(|k| *k == c.pool).unwrap();
            let t = if kinds.len() == 1 { len / 2 } else { spots[k] };
            d.connect(face_cell(s, f, t, 1), c);
        }
    }
    let sculk: Vec<&Conn> = spec.conns.iter().filter(|c| c.up()).collect();
    let spots = spread((s.x - 2).max(1), sculk.len());
    for (c, x) in sculk.into_iter().zip(spots) {
        d.connect(IVec3::new(1 + x, 0, s.z / 2 + (x % 3) - 1), c);
    }
    for c in spec.conns.iter().filter(|c| c.down()) {
        d.connect(IVec3::new(s.x / 2, ANCHOR_Y.min(s.y - 1), s.z / 2), c);
    }
}

/// A city centre: the plaza, the reinforced deepslate portal frame on its
/// stepped platform, and the hidden room inside the platform.
fn center(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    floor(spec, d, m);
    let s = spec.size;
    let cx = s.x / 2;
    let cz = s.z / 2;
    let variant = spec.base_name().chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as i32;
    // The platform: three courses, stepped, 12 by 21 at the base.
    let (hw, hd) = (6, 10);
    for step in 0..3 {
        for x in cx - hw + step..=cx + hw - step {
            for z in cz - hd + step..=cz + hd - step {
                let edge = x == cx - hw + step || x == cx + hw - step || z == cz - hd + step || z == cz + hd - step;
                d.block(IVec3::new(x, 1 + step, z), if edge { m.bricks } else { m.polished });
            }
        }
    }
    // Steps up the front and back of the platform.
    for step in 0..3 {
        for x in cx - 1..=cx + 1 {
            d.stairs(IVec3::new(x, 1 + step, cz - hd + step), m.brick_stairs, Facing::South);
            d.stairs(IVec3::new(x, 1 + step, cz + hd - step), m.brick_stairs, Facing::North);
        }
    }
    // The hidden room inside the platform: walls of bricks, a lectern, a
    // chest where Java has one, soul light, reached through one block of
    // wool in the platform's side.
    let room_lo = IVec3::new(cx - 3, 1, cz - 4);
    let room_hi = IVec3::new(cx + 3, 3, cz + 4);
    for x in room_lo.x..=room_hi.x {
        for z in room_lo.z..=room_hi.z {
            for y in room_lo.y..=room_hi.y {
                let edge = x == room_lo.x || x == room_hi.x || z == room_lo.z || z == room_hi.z || y == room_hi.y;
                d.block(IVec3::new(x, y, z), if edge { m.bricks } else { Block::AIR });
            }
        }
    }
    d.block(IVec3::new(cx - 2, 1, cz), named("lectern"));
    d.block(IVec3::new(cx, 2, cz + 3), m.lantern);
    for _ in 0..spec.count("chest") {
        d.facing(IVec3::new(cx + 2, 1, cz + 2), Block::CHEST, Facing::West);
    }
    d.block(IVec3::new(room_lo.x, 1, cz), m.wool);
    d.block(IVec3::new(room_lo.x, 2, cz), m.wool);
    // The portal frame: reinforced deepslate, 3 + 2·variant wide inside.
    let top = 3 + 1;
    let inner = 3 + variant;
    let half = inner / 2 + 1;
    for dx in -half..=half {
        for y in top..top + inner + 2 {
            let edge = dx.abs() == half || y == top || y == top + inner + 1;
            if edge {
                d.block(IVec3::new(cx + dx, y, cz), m.reinforced);
            }
        }
    }
    // Soul lanterns on posts round the plaza, and basalt pillars.
    for (x, z) in [(1, 1), (s.x - 2, 1), (1, s.z - 2), (s.x - 2, s.z - 2)] {
        for y in 1..6 {
            d.block(IVec3::new(x, y, z), m.basalt);
        }
        d.block(IVec3::new(x, 6, z), m.lantern);
    }
}

/// The walls round the centre: a rampart on the outer side (away from the
/// connector that holds the piece), a walkway on top.
fn center_wall(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    floor(spec, d, m);
    let s = spec.size;
    let held = spec.conns.iter().find(|c| c.side() && c.pool == "empty").and_then(|c| c.facing).unwrap_or(Facing::East);
    let corner = s.x == s.z;
    let height = 12.min(s.y - 1);
    for x in 0..s.x {
        for z in 0..s.z {
            let p = IVec3::new(x, 0, z);
            let outer = [Facing::North, Facing::South, Facing::East, Facing::West].into_iter().any(|f| {
                let on = match f {
                    Facing::North => z <= 1,
                    Facing::South => z >= s.z - 2,
                    Facing::West => x <= 1,
                    Facing::East => x >= s.x - 2,
                };
                on && if corner { f != held } else { f == held.opposite() }
            });
            if !outer {
                continue;
            }
            for y in 1..=height {
                let b = if y == height {
                    m.tile_slab
                } else if (x + z + y) % 9 == 0 {
                    m.chiseled
                } else {
                    m.bricks
                };
                d.block(p.with_y(y), b);
            }
            if (x + z) % 6 == 0 {
                d.block(p.with_y(height + 1), m.lantern);
            }
        }
    }
}

/// Entrance paths: a broad avenue between low walls, lit by soul lanterns.
fn entrance(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    floor(spec, d, m);
    let s = spec.size;
    let cz = s.z / 2;
    for x in 0..s.x {
        for z in [cz - 5, cz + 5] {
            for y in 1..=3 {
                d.block(IVec3::new(x, y, z), if y == 3 { m.wall } else { m.bricks });
            }
            if x % 6 == 2 {
                d.block(IVec3::new(x, 4, z), m.lantern);
            }
        }
    }
    if spec.base_name() == "entrance_connector" {
        // An archway where the avenue leaves the centre's walls.
        for z in cz - 5..=cz + 5 {
            for y in 6..8 {
                d.block(IVec3::new(s.x - 2, y, z), m.bricks);
            }
        }
        for y in 1..8 {
            d.block(IVec3::new(s.x - 2, y, cz - 5), m.bricks);
            d.block(IVec3::new(s.x - 2, y, cz + 5), m.bricks);
        }
    }
}

/// City walls: a deepslate wall along each pair of opposite connectors,
/// with stairs over it, a passage through it, a raised bridge, or a broken
/// top for the ruined ones.
fn wall(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    floor(spec, d, m);
    let s = spec.size;
    let name = spec.base_name();
    let ruined = name.starts_with("ruined");
    let height = if ruined { 6 } else { 9 };
    let (cx, cz) = (s.x / 2, s.z / 2);
    let faces: Vec<Facing> = spec.conns.iter().filter(|c| c.name == "connect_wall").filter_map(|c| c.facing).collect();
    let has = |f: Facing| faces.contains(&f);
    let line = |d: &mut Draft, p: IVec3, i: i32, along_x: bool| {
        let h = if ruined { height - (mix(&spec.location, p.x, p.z) % 4) as i32 } else { height };
        let next = if along_x { IVec3::Z } else { IVec3::X };
        for y in 1..=h {
            let b = if y == h {
                m.tile_slab
            } else if (i + y) % 7 == 0 {
                m.chiseled
            } else {
                m.bricks
            };
            d.block(p.with_y(y), b);
            d.block(p.with_y(y) + next, if y == h { m.tile_slab } else { m.bricks });
        }
    };
    let x_wall = has(Facing::East) || has(Facing::West) || faces.is_empty();
    let z_wall = has(Facing::North) || has(Facing::South);
    if x_wall {
        let from = if has(Facing::West) || faces.is_empty() || !z_wall { 0 } else { cx };
        let to = if has(Facing::East) || faces.is_empty() || !z_wall { s.x - 1 } else { cx };
        for x in from..=to {
            line(d, IVec3::new(x, 0, cz - 1), x, true);
        }
    }
    if z_wall {
        let from = if has(Facing::North) || !x_wall { 0 } else { cz };
        let to = if has(Facing::South) || !x_wall { s.z - 1 } else { cz };
        for z in from..=to {
            line(d, IVec3::new(cx - 1, 0, z), z, false);
        }
    }
    if name.contains("stairs") {
        // Stairs up both sides and over the top.
        for i in 0..height {
            for x in cx - 1..=cx + 1 {
                d.stairs(IVec3::new(x, 1 + i, cz - 2 - (height - 1 - i)), m.brick_stairs, Facing::South);
                d.stairs(IVec3::new(x, 1 + i, cz + 1 + (height - 1 - i)), m.brick_stairs, Facing::North);
            }
        }
    } else if name.contains("passage") {
        for x in cx - 1..=cx + 1 {
            for y in 1..=3 {
                d.air(IVec3::new(x, y, cz - 1));
                d.air(IVec3::new(x, y, cz));
            }
        }
    } else if name.contains("bridge") {
        for x in 0..s.x {
            for z in cz - 2..=cz + 2 {
                d.block(IVec3::new(x, height + 1, z), m.polished);
            }
            if x % 4 == 0 {
                d.block(IVec3::new(x, height + 2, cz - 2), m.wall);
                d.block(IVec3::new(x, height + 2, cz + 2), m.wall);
            }
        }
    }
    for x in (2..s.x - 2).step_by(5) {
        d.block(IVec3::new(x, height + 1, cz), m.lantern);
    }
}

/// The structures between the walls.
fn structure(spec: &PieceSpec, d: &mut Draft, m: &Mat) {
    let s = spec.size;
    let name = spec.base_name();
    floor(spec, d, m);
    let chests: Vec<String> = spec
        .counted("loot")
        .flat_map(|(l, n)| std::iter::repeat_n(l.split_once(':').map_or(l, |x| x.1).to_string(), n as usize))
        .collect();
    let mut chest_spots: Vec<IVec3> = Vec::new();
    if name.contains("pillar") {
        let c = IVec3::new(s.x / 2, 0, s.z / 2);
        let r = if name.starts_with("large") { 2 } else { 1 };
        for y in 1..s.y {
            for dx in -r + 1..r {
                for dz in -r + 1..r {
                    d.block(c + IVec3::new(dx, y, dz), if y % 5 == 0 { m.chiseled } else { m.basalt });
                }
            }
        }
        d.block(c.with_y(s.y - 1), m.lantern);
    } else if name == "small_statue" {
        // A hunched deepslate figure on a plinth.
        let c = IVec3::new(s.x / 2, 0, s.z / 2);
        d.fill(c + IVec3::new(-2, 1, -2), c + IVec3::new(2, 1, 2), m.polished);
        d.fill(c + IVec3::new(-1, 2, -1), c + IVec3::new(1, 5, 1), m.cobbled);
        d.fill(c + IVec3::new(-2, 4, 0), c + IVec3::new(2, 4, 0), m.cobbled);
        d.block(c + IVec3::new(0, 6, 0), m.chiseled);
        d.block(c + IVec3::new(0, 6, -1), named("sculk sensor"));
    } else if name.starts_with("camp") {
        // A wool tent beside a soul campfire.
        let c = IVec3::new(s.x / 2, 0, s.z / 2);
        d.block(c + IVec3::Y, named("soul campfire"));
        for i in 0..3 {
            for x in c.x - 2..=c.x + 2 {
                d.block(IVec3::new(x, 1 + i, c.z - 4 + i), m.wool);
                d.block(IVec3::new(x, 1 + i, c.z - 4 - i), m.wool);
            }
        }
        d.block(c + IVec3::new(3, 1, 0), m.dark_planks);
        d.block(c + IVec3::new(-3, 1, 0), m.dark_planks);
    } else if name.contains("ruin") {
        // Broken walls, taller and with a chest for the tall ruins.
        let tall = name.starts_with("tall");
        let h = if tall { s.y - 3 } else { s.y - 1 };
        for x in 0..s.x {
            for z in 0..s.z {
                let edge = x == 1 || z == 1 || x == s.x - 2 || z == s.z - 2;
                if !edge {
                    continue;
                }
                let top = (h - (mix(&spec.location, x, z) % (h as u64).max(1)) as i32).max(1);
                for y in 1..=top {
                    d.block(IVec3::new(x, y, z), if y % 4 == 0 { m.cobbled } else { m.bricks });
                }
            }
        }
        if tall {
            for y in [6, 12, 18].into_iter().filter(|y| *y < s.y - 2) {
                d.fill(IVec3::new(2, y, 2), IVec3::new(s.x - 3, y, s.z - 3), m.polished);
                d.air(IVec3::new(s.x / 2, y, 3));
            }
            for y in 1..s.y - 3 {
                d.facing(IVec3::new(s.x / 2, y, 2), Block::LADDER, Facing::South);
            }
            for i in 0..chests.len() as i32 {
                chest_spots.push(IVec3::new(3 + i * 2, 7, s.z - 4));
            }
        }
    } else {
        // Buildings: barracks, chambers, the sauna and the ice box.
        let ice = name.starts_with("ice_box");
        let h = (s.y - 1).min(7);
        for x in 0..s.x {
            for z in 0..s.z {
                let edge = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
                for y in 1..=h {
                    let b = if y == h {
                        m.tiles
                    } else if edge {
                        if ice && y == 1 { named("packed ice") } else { m.bricks }
                    } else if name.starts_with("sauna") && y == 1 && (x + z) % 3 == 0 {
                        named("basalt")
                    } else {
                        Block::AIR
                    };
                    d.block(IVec3::new(x, y, z), b);
                }
            }
        }
        // Doorways in the middle of each side.
        for f in [Facing::North, Facing::South, Facing::East, Facing::West] {
            let len = face_len(s, f);
            for dt in -1..=1 {
                for y in 1..=3 {
                    d.air(face_cell(s, f, len / 2 + dt, y));
                }
            }
        }
        if ice {
            for x in 2..s.x - 2 {
                d.block(IVec3::new(x, 1, s.z / 2), named("blue ice"));
                d.block(IVec3::new(x, 1, s.z / 2 + 1), Block::SNOW);
            }
        }
        if name == "barracks" {
            for z in (2..s.z - 2).step_by(3) {
                d.block(IVec3::new(1, 1, z), m.wool);
                d.block(IVec3::new(2, 1, z), m.wool);
                d.block(IVec3::new(s.x - 2, 1, z), m.dark_fence);
            }
        }
        d.block(IVec3::new(s.x / 2, h - 1, s.z / 2), m.lantern);
        for i in 0..chests.len() as i32 {
            chest_spots.push(IVec3::new(2 + i * 3, 1, s.z - 2));
        }
    }
    for (i, table) in chests.iter().enumerate() {
        let at = chest_spots.get(i).copied().unwrap_or(IVec3::new(1 + i as i32, 1, 1));
        d.loot(at, table, Facing::North);
    }
}

#[cfg(test)]
mod tests {
    use glam::IVec2;

    use super::*;
    use crate::world::terrain::{Dimension, Generation, Generator};

    #[test]
    fn a_generation_4_city_has_its_centre_hidden_room_walls_and_structures() {
        let g = Generator::with_generation(12345, Dimension::Overworld, Generation::V4);
        let at = g.ancient_cities.nearest(&g, IVec2::ZERO, 8000).expect("an ancient city within 8000 blocks");
        let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * crate::world::ancient_city::SPACING));
        let city = g.ancient_cities.get(&g, region).unwrap();
        let lib = library();
        let names: Vec<&str> = city.pieces.iter().map(|p| lib.templates[p.template as usize].name.as_str()).collect();
        println!("generation 4 ancient city at {at}: {} pieces", names.len());
        assert!(names[0].starts_with("ancient_city/city_center/city_center_"));
        // The anchor lands on the start position: walk level -51.
        assert_eq!(city.pieces[0].origin.y, crate::world::ancient_city::START_Y - ANCHOR_Y);
        assert!(names.iter().any(|n| n.starts_with("ancient_city/city_center/walls/")));
        assert!(names.iter().any(|n| n.starts_with("ancient_city/walls/")));
        assert!(names.iter().any(|n| n.starts_with("ancient_city/structures/")));
        assert!(names.len() >= 20, "{names:?}");
        // Pieces overlap only where a sculk patch sits inside its structure.
        let inside = |x: &crate::world::jigsaw::Placed, y: &crate::world::jigsaw::Placed| {
            x.bounds.min.cmpge(y.bounds.min).all() && x.bounds.max.cmple(y.bounds.max).all()
        };
        for (i, a) in city.pieces.iter().enumerate() {
            for b in &city.pieces[i + 1..] {
                assert!(!a.bounds.intersects(&b.bounds) || inside(a, b) || inside(b, a), "{a:?} overlaps {b:?}");
            }
        }
        // The hidden room's lectern and the portal frame are in the centre.
        let centre = &lib.templates[city.pieces[0].template as usize];
        let lectern = named("lectern");
        assert!(centre.cells.iter().any(|c| c.spec.base == crate::world::jigsaw::Base::Block(lectern)));
        assert!(
            centre
                .cells
                .iter()
                .any(|c| c.spec.base == crate::world::jigsaw::Base::Block(crate::world::jukebox::REINFORCED_DEEPSLATE))
        );
        assert!(!city.chests.is_empty());
    }
}
