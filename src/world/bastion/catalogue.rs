//! Generation 6 bastion remnants: original templates for Java 26.3's full
//! `bastion/*` pool catalogue (v1.0 Track 8, GEN-05g, FE-02).
//!
//! `jigsaw/bastion_v6.txt` holds Java's facts about each of the 167
//! template locations its 60 pools name (size, connectors and where they
//! sit, chests and their loot tables, the magma cube spawner, gold blocks,
//! gilded blackstone, lava and residents) and the pools themselves. The
//! painter here draws an original template for every piece line at its
//! size, by the piece's kind: ramparts and walls are blackstone masses
//! faced with polished bricks, with rooms and a crenellated walk on top;
//! housing units are brick floors and walls with windows and a stair; the
//! hoglin stables are railed pens and stair halls; the treasure room is a
//! lava basin with a spawner, gold-and-treasure pedestals, beams and
//! walkways; the bridge is an arched span on legs before a gate with an
//! original piglin face. Each connector sits where Java's does, so the
//! pieces tile as Java's do, and every counted block is placed exactly as
//! many times as Java's template holds it (`catalogue_tests` checks both).
//! Nothing of Java's geometry is copied.

use std::sync::OnceLock;

use glam::IVec3;
use rustc_hash::FxHashSet;

use crate::world::block::{Block, Facing};
use crate::world::jigsaw::catalogue::{self, Draft, PieceSpec};
use crate::world::jigsaw::draw::named;
use crate::world::jigsaw::{Base, Library};
use crate::world::noise::hash3;

/// The generation 6 bastion library, built once.
pub fn library() -> &'static Library {
    static LIB: OnceLock<Library> = OnceLock::new();
    LIB.get_or_init(|| {
        catalogue::build(include_str!("../jigsaw/bastion_v6.txt"), &paint).unwrap_or_else(|e| panic!("{e}"))
    })
}

const BRICKS: Block = Block::POLISHED_BLACKSTONE_BRICKS;
const BLACK: Block = Block::BLACKSTONE;
const POLISHED: Block = Block::POLISHED_BLACKSTONE;
const CHISELED: Block = Block::CHISELED_POLISHED_BLACKSTONE;
const PILLAR: Block = Block::POLISHED_BASALT;
const AIR: Block = Block::AIR;

fn v(x: i32, y: i32, z: i32) -> IVec3 {
    IVec3::new(x, y, z)
}

/// A stable seed per template location, for the painter's variety.
fn seed_of(location: &str) -> u64 {
    location.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn roll(seed: u64, p: IVec3) -> u64 {
    hash3(p.x, p.y, p.z, seed)
}

fn solid(d: &Draft, p: IVec3) -> bool {
    matches!(d.get(p), Some(s) if matches!(s.base, Base::Block(b) if b != AIR && !b.is_fluid()))
}

fn open(d: &Draft, p: IVec3) -> bool {
    match d.get(p) {
        None => true,
        Some(s) => matches!(s.base, Base::Block(AIR)),
    }
}

/// Draws one piece's original template.
pub fn paint(spec: &PieceSpec, d: &mut Draft) {
    let loc = spec.location.as_str();
    let size = spec.size;
    let seed = seed_of(loc);
    match spec.kind.as_str() {
        "mob" => return mob(spec, d),
        "block" => return block(spec, d),
        "start" => start(d, size, seed),
        _ => body(loc, d, size, seed),
    }
    connectors(spec, d);
    counted(spec, d, seed);
}

fn mob(spec: &PieceSpec, d: &mut Draft) {
    d.fill(IVec3::ZERO, spec.size - 1, AIR);
    for c in &spec.conns {
        d.connect(c.pos.expect("bastion connectors keep their position"), c);
    }
    for (m, n) in spec.counted("mob") {
        for _ in 0..n {
            d.mob(IVec3::ZERO, m);
        }
    }
}

fn block(spec: &PieceSpec, d: &mut Draft) {
    let b = if spec.location.ends_with("/gold") { Block::GOLD_BLOCK } else { AIR };
    d.block(IVec3::ZERO, b);
    for c in &spec.conns {
        d.connect(c.pos.expect("bastion connectors keep their position"), c);
    }
}

/// The air bases Java starts three bastion kinds from: an open court
/// cleared out of the rock, with rounded corners, that the units fill.
fn start(d: &mut Draft, s: IVec3, seed: u64) {
    let (cx, cz) = ((s.x - 1) as f32 / 2.0, (s.z - 1) as f32 / 2.0);
    for y in 0..s.y {
        for z in 0..s.z {
            for x in 0..s.x {
                let (dx, dz) = ((x as f32 - cx) / (cx + 0.5), (z as f32 - cz) / (cz + 0.5));
                // A squircle a little inside the box, ragged at the rim.
                let r = (dx.powi(4) + dz.powi(4)).powf(0.25);
                let ragged = (roll(seed, v(x, 0, z)) % 7) as f32 * 0.01;
                if r < 0.86 + ragged {
                    d.block(v(x, y, z), AIR);
                }
            }
        }
    }
}

fn body(loc: &str, d: &mut Draft, s: IVec3, seed: u64) {
    let has = |part: &str| loc.contains(part);
    if has("/bases/lava_basin") {
        basin(d, s);
    } else if has("/bases/centers/") {
        pedestal(d, s, seed);
    } else if has("/bridge_pieces/") {
        span(d, s);
    } else if has("/legs/") || has("/posts/") || has("/brains/") {
        column(d, s, has("/brains/"));
    } else if has("/center_pieces/") {
        courtyard(d, s);
    } else if has("/pathways/") {
        d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), BLACK);
        d.fill(v(0, 1, 0), s - 1, AIR);
    } else if has("_stables/") {
        pen(d, s, seed, has("/inner_"));
    } else if has("hoglin_stable/stairs/") || has("hoglin_stable/starting_pieces/") || has("treasure/stairs/") {
        stair_hall(d, s);
    } else if has("/extensions/") {
        extension(loc, d, s, seed);
    } else if has("/units/stages/")
        || has("/units/fillers/")
        || has("/units/edges/")
        || has("/units/wall_units/")
        || has("/treasure/entrances/")
    {
        house(d, s, seed);
    } else if s.y <= 2 || (s.x <= 2 && s.z <= 2) {
        beam(d, s);
    } else if has("/connectors/back_bridge") {
        ramp(d, s);
    } else if has("rampart_plates/") || has("/corners/edges/") || has("/starting_pieces/entrance_face") {
        plate(d, s, seed, has("entrance_face"));
    } else {
        bulwark(d, s, seed, has("/roofs/"));
    }
}

/// A blackstone mass faced with polished bricks: basalt corners, a course
/// of polished blackstone every six blocks, rooms inside the taller ones,
/// arrow slits and a crenellated walk on top.
fn bulwark(d: &mut Draft, s: IVec3, seed: u64, roof: bool) {
    d.fill(IVec3::ZERO, s - 1, BLACK);
    for y in 0..s.y {
        for z in 0..s.z {
            for x in 0..s.x {
                let face = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
                if face {
                    let b = if y % 6 == 5 { POLISHED } else { BRICKS };
                    d.block(v(x, y, z), b);
                }
            }
        }
    }
    for (x, z) in [(0, 0), (s.x - 1, 0), (0, s.z - 1), (s.x - 1, s.z - 1)] {
        d.fill(v(x, 0, z), v(x, s.y - 1, z), PILLAR);
    }
    if roof {
        // A thin roof: slab over open space, on a few pillars.
        d.fill(IVec3::ZERO, v(s.x - 1, s.y - 2, s.z - 1), AIR);
        d.fill(v(0, s.y - 1, 0), s - 1, BRICKS);
        for x in (0..s.x).step_by(4) {
            for z in (0..s.z).step_by(4) {
                d.fill(v(x, 0, z), v(x, s.y - 2, z), PILLAR);
            }
        }
        return;
    }
    // Rooms five high, one above another, with floors of brick.
    if s.x >= 6 && s.z >= 6 && s.y >= 9 {
        let mut y0 = 2;
        while y0 + 5 < s.y - 2 {
            d.fill(v(2, y0, 2), v(s.x - 3, y0 + 4, s.z - 3), AIR);
            d.fill(v(2, y0 - 1, 2), v(s.x - 3, y0 - 1, s.z - 3), BRICKS);
            // A stair up the room's west wall to a hole in its ceiling.
            let stairs = named("polished blackstone brick stairs");
            for i in 0..5 {
                let z = 3 + i;
                if z < s.z - 3 {
                    d.stairs(v(2, y0 + i, z), stairs, Facing::South);
                    d.fill(v(2, y0 + i + 1, z), v(2, y0 + 5, z), AIR);
                }
            }
            // Arrow slits on the long sides.
            for x in (3..s.x - 3).step_by(4) {
                d.fill(v(x, y0 + 1, 0), v(x, y0 + 2, 1), AIR);
                d.fill(v(x, y0 + 1, s.z - 2), v(x, y0 + 2, s.z - 1), AIR);
            }
            y0 += 7;
        }
    }
    // The walk along the top, behind merlons.
    if s.y >= 4 && s.x >= 4 && s.z >= 4 {
        d.fill(v(1, s.y - 1, 1), v(s.x - 2, s.y - 1, s.z - 2), AIR);
        for z in 0..s.z {
            for x in 0..s.x {
                let edge = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
                if edge && (x + z) % 2 == 1 && !roll(seed, v(x, 1, z)).is_multiple_of(5) {
                    d.block(v(x, s.y - 1, z), AIR);
                }
            }
        }
    }
}

/// A thin buttress or face: one brick wall across its widest side, with an
/// original piglin face (eyes, snout and tusks) on an entrance face.
fn plate(d: &mut Draft, s: IVec3, seed: u64, face: bool) {
    let along_x = s.x >= s.z;
    let (len, thick) = if along_x { (s.x, s.z) } else { (s.z, s.x) };
    let at = |u: i32, y: i32, t: i32| if along_x { v(u, y, t) } else { v(t, y, u) };
    for t in 0..thick {
        for y in 0..s.y {
            for u in 0..len {
                let edge = u == 0 || u == len - 1;
                let b = if edge {
                    PILLAR
                } else if y % 5 == 4 {
                    POLISHED
                } else if t == 0 || t == thick - 1 {
                    BRICKS
                } else {
                    BLACK
                };
                d.block(at(u, y, t), b);
            }
        }
    }
    if face && len >= 12 && s.y >= 12 {
        let mid = len / 2;
        let t = 0;
        let eyes = s.y * 2 / 3;
        for e in [mid - 5, mid + 3] {
            d.fill(at(e, eyes, t), at(e + 1, eyes + 1, t), CHISELED);
        }
        // The snout: a broad block with two nostrils.
        d.fill(at(mid - 3, eyes - 6, t), at(mid + 2, eyes - 2, t), POLISHED);
        d.block(at(mid - 2, eyes - 4, t), AIR);
        d.block(at(mid + 1, eyes - 4, t), AIR);
        for tusk in [mid - 5, mid + 4] {
            d.fill(at(tusk, eyes - 8, t), at(tusk, eyes - 5, t), CHISELED);
        }
    } else {
        // Slits through the wall.
        for u in (2..len - 2).step_by(3) {
            if roll(seed, at(u, 0, 0)).is_multiple_of(3) {
                let y = s.y / 2;
                d.fill(at(u, y, 0), at(u, y + 1, thick - 1), AIR);
            }
        }
    }
}

/// A housing unit: brick floor, walls with windows, a basalt post at each
/// corner, a ceiling with a stairwell; low units are an open roof terrace.
fn house(d: &mut Draft, s: IVec3, seed: u64) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), BRICKS);
    if s.y <= 3 {
        for z in 0..s.z {
            for x in 0..s.x {
                let edge = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
                if edge && (x + z) % 3 != 1 {
                    d.block(v(x, 1, z), named("polished blackstone brick wall"));
                }
            }
        }
        return;
    }
    let top = s.y - 1;
    for y in 1..top {
        for z in 0..s.z {
            for x in 0..s.x {
                let edge = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
                if edge {
                    let window = (2..=3).contains(&y) && (x + z) % 3 == 1 && x > 0 && x < s.x - 1;
                    d.block(v(x, y, z), if window { AIR } else { BRICKS });
                }
            }
        }
    }
    d.fill(v(0, top, 0), v(s.x - 1, top, s.z - 1), BRICKS);
    for (x, z) in [(0, 0), (s.x - 1, 0), (0, s.z - 1), (s.x - 1, s.z - 1)] {
        d.fill(v(x, 0, z), v(x, top, z), PILLAR);
    }
    // A stair along the north wall to a well in the ceiling.
    let stairs = named("polished blackstone brick stairs");
    let flight = (top - 1).min(s.x - 3);
    for i in 0..flight {
        d.stairs(v(1 + i, 1 + i, 1), stairs, Facing::East);
    }
    if flight > 0 {
        d.fill(v(flight - 1, top, 1), v(flight, top, 2.min(s.z - 2)), AIR);
    }
    // Some units keep a basalt-and-soul-sand hearth.
    if roll(seed, IVec3::ZERO).is_multiple_of(3) && s.x >= 6 && s.z >= 6 {
        let c = v(s.x / 2, 0, s.z / 2);
        d.block(c, Block::SOUL_SAND);
        d.block(c + IVec3::Y, Block::nether_wart(3));
    }
}

/// The units' centre: a brick court around a patch of nether wart in soul
/// sand, ringed by chiseled blackstone and a low broken wall.
fn courtyard(d: &mut Draft, s: IVec3) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), BRICKS);
    let (cx, cz) = (s.x / 2, s.z / 2);
    d.fill(v(cx - 3, 0, cz - 3), v(cx + 3, 0, cz + 3), CHISELED);
    d.fill(v(cx - 2, 0, cz - 2), v(cx + 2, 0, cz + 2), Block::SOUL_SAND);
    d.fill(v(cx - 2, 1, cz - 2), v(cx + 2, 1, cz + 2), Block::nether_wart(3));
    for z in 0..s.z {
        for x in 0..s.x {
            let edge = x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1;
            if edge && (x - cx).abs() > 1 && (z - cz).abs() > 1 {
                d.block(v(x, 1, z), named("blackstone wall"));
            }
        }
    }
}

/// A hoglin pen: blackstone floor, rails of blackstone wall, basalt posts;
/// the inner pens carry the next level's floor as a ceiling.
fn pen(d: &mut Draft, s: IVec3, seed: u64, inner: bool) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), BLACK);
    let rail = named("blackstone wall");
    for x in 0..s.x {
        for z in [0, s.z - 1] {
            d.block(v(x, 1, z), rail);
            if x % 4 == 0 {
                d.fill(v(x, 1, z), v(x, s.y - 1, z), PILLAR);
            }
        }
    }
    // Feed: a strip of crimson roots... here a soul sand trough.
    let tz = 1 + (roll(seed, IVec3::ONE) % (s.z.max(3) as u64 - 2)) as i32;
    d.fill(v(2, 0, tz), v(s.x - 3, 0, tz), Block::SOUL_SAND);
    if inner {
        d.fill(v(0, s.y - 1, 0), v(s.x - 1, s.y - 1, s.z - 1), BRICKS);
    }
}

/// The stables' stair halls (and the treasure room's lower stairs): a
/// flight of brick stairs climbing along x between walls.
fn stair_hall(d: &mut Draft, s: IVec3) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), BLACK);
    let stairs = named("polished blackstone brick stairs");
    let (z0, z1) = (s.z / 2 - 2, s.z / 2 + 1);
    for x in 0..s.x {
        let y = (x * (s.y - 1) / s.x.max(1)).clamp(0, s.y - 1);
        d.fill(v(x, 0, z0), v(x, y, z1), BRICKS);
        if y + 1 < s.y {
            for z in z0..=z1 {
                d.stairs(v(x, y + 1, z), stairs, Facing::East);
            }
        }
        d.fill(v(x, 0, z0 - 1), v(x, (y + 2).min(s.y - 1), z0 - 1), BRICKS);
        d.fill(v(x, 0, z1 + 1), v(x, (y + 2).min(s.y - 1), z1 + 1), BRICKS);
    }
}

/// Posts, legs and the treasure room's brain: a basalt column cased in
/// bricks, chained at the brain.
fn column(d: &mut Draft, s: IVec3, chained: bool) {
    d.fill(IVec3::ZERO, s - 1, BRICKS);
    for y in 0..s.y {
        let b = if chained && y % 3 == 1 { Block::CHAIN } else { PILLAR };
        d.fill(v(0, y, 0), v((s.x - 1).min(1), y, (s.z - 1).min(1)), b);
    }
    if s.x >= 3 && s.z >= 3 {
        d.fill(v(1, 0, 1), v(1, s.y - 1, 1), PILLAR);
    }
}

/// A one-block beam of polished bricks.
fn beam(d: &mut Draft, s: IVec3) {
    d.fill(IVec3::ZERO, s - 1, BRICKS);
    if s.x > 2 {
        for x in (0..s.x).step_by(3) {
            d.block(v(x, 0, 0), POLISHED);
        }
    }
}

/// A walkway falling along z from the top of the piece to its foot.
fn ramp(d: &mut Draft, s: IVec3) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    for z in 0..s.z {
        let y = (s.y - 3) - z * (s.y - 3) / s.z.max(1);
        d.fill(v(1, 0, z), v(s.x - 2, y, z), BLACK);
        d.fill(v(1, y, z), v(s.x - 2, y, z), BRICKS);
        d.fill(v(0, 0, z), v(0, y + 1, z), BRICKS);
        d.fill(v(s.x - 1, 0, z), v(s.x - 1, y + 1, z), BRICKS);
    }
}

/// The bridge: a deck along x two blocks under the top connector, on two
/// arches, with railings and lamps of basalt.
fn span(d: &mut Draft, s: IVec3) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    let deck = s.y - 7;
    d.fill(v(0, deck, 1), v(s.x - 1, deck, s.z - 2), BRICKS);
    d.fill(v(0, deck - 1, 2), v(s.x - 1, deck - 1, s.z - 3), BLACK);
    for x in 0..s.x {
        let rail = if x % 5 == 0 { PILLAR } else { named("polished blackstone brick wall") };
        d.block(v(x, deck + 1, 1), rail);
        d.block(v(x, deck + 1, s.z - 2), rail);
    }
    // Arches spring from the legs' feet.
    for x0 in [4, s.x - 9] {
        for x in x0..x0 + 5 {
            let rise = 2 - (x - x0 - 2).abs();
            d.fill(v(x, 0, 3), v(x, deck - 2 - rise, s.z - 4), BRICKS);
        }
    }
}

/// The treasure basin: a magma floor under a sunken pool (filled with lava
/// by the counted pass), a brick rim, and a pillar for the spawner.
fn basin(d: &mut Draft, s: IVec3) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), Block::MAGMA);
    d.fill(v(0, 1, 0), v(s.x - 1, 3, s.z - 1), BRICKS);
    for z in 0..s.z {
        for x in 0..s.x {
            if x == 0 || z == 0 || x == s.x - 1 || z == s.z - 1 {
                d.fill(v(x, 0, z), v(x, 4, z), if (x + z) % 4 == 0 { PILLAR } else { BRICKS });
            }
        }
    }
    let c = v(s.x / 2, 0, s.z / 2);
    d.fill(c + v(-1, 1, -1), c + v(1, 6, 1), POLISHED);
}

/// A treasure pedestal: two layers of brick with a chiseled crown.
fn pedestal(d: &mut Draft, s: IVec3, seed: u64) {
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 1, s.z - 1), BRICKS);
    for z in 0..s.z {
        for x in 0..s.x {
            if (x == 0 || x == s.x - 1) && (z == 0 || z == s.z - 1) {
                d.fill(v(x, 2, z), v(x, 3, z), named("polished blackstone wall"));
            }
        }
    }
    if roll(seed, IVec3::ZERO).is_multiple_of(2) {
        d.fill(v(1, 1, 1), v(s.x - 2, 1, s.z - 2), CHISELED);
    }
}

/// Treasure extensions: bridges, a fire room, small houses and empty
/// landings.
fn extension(loc: &str, d: &mut Draft, s: IVec3, seed: u64) {
    if loc.ends_with("/fire_room") {
        d.fill(IVec3::ZERO, s - 1, AIR);
        d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), Block::NETHERRACK);
        for z in 0..s.z {
            for x in 0..s.x {
                if s.y > 1 && roll(seed, v(x, 0, z)).is_multiple_of(4) {
                    d.block(v(x, 1, z), Block::fire(0));
                }
            }
        }
        return;
    }
    if loc.contains("/house_") {
        return house(d, s, seed);
    }
    d.fill(IVec3::ZERO, s - 1, AIR);
    d.fill(IVec3::ZERO, v(s.x - 1, 0, s.z - 1), BRICKS);
    if s.y > 1 {
        let rail = named("polished blackstone brick wall");
        let along_x = s.x >= s.z;
        for i in 0..s.x.max(s.z) {
            let (a, b) = if along_x { (v(i, 1, 0), v(i, 1, s.z - 1)) } else { (v(0, 1, i), v(s.x - 1, 1, i)) };
            d.block(a, rail);
            d.block(b, rail);
        }
    }
}

/// Places every connector where Java's template holds it and shapes the
/// cells around it: a floor under residents and loose blocks, a doorway
/// at each side connector.
fn connectors(spec: &PieceSpec, d: &mut Draft) {
    for c in &spec.conns {
        let p = c.pos.expect("bastion connectors keep their position");
        if c.side() {
            let f = c.facing.expect("side connector");
            let across = if f.offset().x != 0 { IVec3::Z } else { IVec3::X };
            for dy in 0..3 {
                for w in -1..=1 {
                    let q = p + IVec3::Y * dy + across * w;
                    if d.inside(q) && q.y > 0 {
                        d.block(q, AIR);
                    }
                }
            }
            if p.y == 0 || !solid(d, p - IVec3::Y) {
                d.block(p, BRICKS);
            } else {
                d.block(p, AIR);
            }
        } else {
            d.block(p, BRICKS);
        }
        d.connect(p, c);
    }
}

/// Places the template's chests, spawners, gold, gilded blackstone and
/// lava, exactly as many of each as Java's template holds, on cells no
/// connector uses.
fn counted(spec: &PieceSpec, d: &mut Draft, seed: u64) {
    let s = spec.size;
    let mut reserved: FxHashSet<IVec3> = spec.conns.iter().filter_map(|c| c.pos).collect();
    let cells: Vec<IVec3> =
        (0..s.y).flat_map(|y| (0..s.z).flat_map(move |z| (0..s.x).map(move |x| v(x, y, z)))).collect();
    let centre = (s - 1).as_vec3() / 2.0;
    let mut by_hash = cells.clone();
    by_hash.sort_by_key(|&p| (roll(seed, p), p.x, p.y, p.z));

    // Lava first, as pools: from the bottom up and the middle out, solid
    // cells whose sides and floor are solid, so the lava stays put.
    let lava = spec.count("lava");
    if lava > 0 {
        let mut order = cells.clone();
        order.sort_by(|a, b| {
            let da = (a.as_vec3() - centre).truncate().length_squared();
            let db = (b.as_vec3() - centre).truncate().length_squared();
            (a.y, (da * 16.0) as i64, a.x, a.z).cmp(&(b.y, (db * 16.0) as i64, b.x, b.z))
        });
        let held = |d: &Draft, q: IVec3, chosen: &FxHashSet<IVec3>| !d.inside(q) || solid(d, q) || chosen.contains(&q);
        let mut chosen: FxHashSet<IVec3> = FxHashSet::default();
        let mut placed = 0;
        for pass in 0..2 {
            for &p in &order {
                if placed == lava {
                    break;
                }
                if chosen.contains(&p) || reserved.contains(&p) || !solid(d, p) {
                    continue;
                }
                let sealed = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::NEG_Y]
                    .iter()
                    .all(|&o| held(d, p + o, &chosen) && d.inside(p + o));
                if pass == 0 && !sealed {
                    continue;
                }
                chosen.insert(p);
                placed += 1;
            }
        }
        // Too few solid cells: lava in the open (tiny pieces only).
        for &p in &order {
            if placed == lava {
                break;
            }
            if !chosen.contains(&p) && !reserved.contains(&p) {
                chosen.insert(p);
                placed += 1;
            }
        }
        for &p in &chosen {
            d.block(p, Block::LAVA);
            reserved.insert(p);
        }
    }

    // Chests stand on a floor with room above; treasure tables first.
    for (loot, n) in spec.counted("loot") {
        let (_, table) = loot.split_once(':').expect("loot=<container>:<table>");
        for _ in 0..n {
            let p = pick(d, &by_hash, &reserved, |d, p| {
                open(d, p) && p.y > 0 && solid(d, p - IVec3::Y) && (!d.inside(p + IVec3::Y) || open(d, p + IVec3::Y))
            })
            .or_else(|| pick(d, &by_hash, &reserved, |_, _| true))
            .expect("a cell for every chest");
            d.loot(p, table, Facing::South);
            reserved.insert(p);
        }
    }
    for (mob, n) in spec.counted("spawner_mob") {
        for _ in 0..n {
            let mut order = by_hash.clone();
            order.sort_by_key(|p| ((p.as_vec3() - centre).truncate().length_squared() * 16.0) as i64);
            let p = pick(d, &order, &reserved, |d, p| open(d, p) && p.y > 0 && solid(d, p - IVec3::Y))
                .or_else(|| pick(d, &order, &reserved, |_, _| true))
                .expect("a cell for the spawner");
            d.spawner(p, mob);
            reserved.insert(p);
        }
    }
    // Gold and gilded blackstone replace facing blocks where they show.
    for (block, n) in
        [(Block::GOLD_BLOCK, spec.count("gold_block")), (Block::GILDED_BLACKSTONE, spec.count("gilded_blackstone"))]
    {
        for _ in 0..n {
            let shows = |d: &Draft, p: IVec3| {
                solid(d, p)
                    && [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z]
                        .iter()
                        .any(|&o| !d.inside(p + o) || open(d, p + o))
            };
            let p = pick(d, &by_hash, &reserved, shows)
                .or_else(|| pick(d, &by_hash, &reserved, solid))
                .or_else(|| pick(d, &by_hash, &reserved, |_, _| true))
                .expect("a cell for every counted block");
            d.block(p, block);
            reserved.insert(p);
        }
    }
}

fn pick(d: &Draft, order: &[IVec3], reserved: &FxHashSet<IVec3>, ok: impl Fn(&Draft, IVec3) -> bool) -> Option<IVec3> {
    order.iter().copied().find(|p| !reserved.contains(p) && ok(d, *p))
}
