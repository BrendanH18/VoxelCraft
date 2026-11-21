//! The shipwreck templates (fidelity exception FE-02).
//!
//! [`TEMPLATES`] holds what Java 26.3's `structure/shipwreck/*.nbt`
//! templates define for play: the size, and each data marker with the
//! facing of the chest one block below it. [`PALETTES`] are the eight wood
//! pairs every template's palettes swap in for the first palette's oak (the
//! hull) and spruce (the trim). `tests/fixtures/java/26.3/ocean_structures
//! .json` is checked against both in both directions.
//!
//! The blocks are VoxelCraft's own ship, drawn here once in Java's frame
//! (x across the beam, y up, z from bow to stern): a keel and a rounded
//! bilge, a hold two blocks high under a trim deck with hatches over its
//! chests, railings, and a stern cabin a step down from the deck with a
//! roof walk and windows; the masted ship adds a mast with two yards and a
//! crow's nest. Each catalogue entry is that ship as Java's entry of the
//! same name lies: right side up, upside down (turned over end to end
//! across its beam) or on its side (rolled onto its port side), whole or
//! broken to its front or back part with a ragged break, and the degraded
//! ones with planks missing. The chests sit at Java's marker cells, so the
//! loot is where Java's is.

use std::sync::OnceLock;

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::super::block::{Block, Facing};
use super::super::jigsaw::draw::named;
use super::super::mansion::Rotation;
use super::super::noise::hash3;
use super::super::slab_stair_states::{self as ss, SlabType, StairShape};

/// A data marker (`ShipwreckPieces.MARKERS_TO_LOOT`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Marker {
    Map,
    Supply,
    Treasure,
}

impl Marker {
    pub fn java(self) -> &'static str {
        match self {
            Marker::Map => "map_chest",
            Marker::Supply => "supply_chest",
            Marker::Treasure => "treasure_chest",
        }
    }
}

/// What Java's template of a name defines for play.
pub struct Facts {
    pub name: &'static str,
    pub size: [i32; 3],
    /// Each marker, its cell and the facing of the chest under it.
    pub markers: &'static [(Marker, [i32; 3], Facing)],
}

/// From `ocean_structures.json`'s `shipwreck_templates`, in name order.
#[rustfmt::skip]
pub const TEMPLATES: [Facts; 20] = [
    Facts { name: "rightsideup_backhalf", size: [9, 9, 16], markers: &[(Marker::Map, [5, 3, 6], Facing::West), (Marker::Treasure, [6, 5, 12], Facing::North)] },
    Facts { name: "rightsideup_backhalf_degraded", size: [9, 9, 16], markers: &[(Marker::Map, [5, 3, 6], Facing::West), (Marker::Treasure, [6, 5, 12], Facing::North)] },
    Facts { name: "rightsideup_fronthalf", size: [9, 9, 24], markers: &[(Marker::Supply, [4, 3, 8], Facing::South)] },
    Facts { name: "rightsideup_fronthalf_degraded", size: [9, 9, 24], markers: &[(Marker::Supply, [4, 3, 8], Facing::South)] },
    Facts { name: "rightsideup_full", size: [9, 9, 28], markers: &[(Marker::Supply, [4, 3, 8], Facing::South), (Marker::Map, [5, 3, 18], Facing::West), (Marker::Treasure, [6, 5, 24], Facing::North)] },
    Facts { name: "rightsideup_full_degraded", size: [9, 9, 28], markers: &[(Marker::Supply, [4, 3, 8], Facing::South), (Marker::Map, [5, 3, 18], Facing::West), (Marker::Treasure, [6, 5, 24], Facing::North)] },
    Facts { name: "sideways_backhalf", size: [9, 9, 17], markers: &[(Marker::Treasure, [3, 3, 13], Facing::North), (Marker::Map, [6, 4, 8], Facing::North)] },
    Facts { name: "sideways_backhalf_degraded", size: [9, 9, 17], markers: &[(Marker::Treasure, [3, 3, 13], Facing::North), (Marker::Map, [6, 4, 8], Facing::North)] },
    Facts { name: "sideways_fronthalf", size: [9, 9, 24], markers: &[(Marker::Supply, [5, 4, 8], Facing::South)] },
    Facts { name: "sideways_fronthalf_degraded", size: [9, 9, 24], markers: &[(Marker::Supply, [5, 4, 8], Facing::South)] },
    Facts { name: "sideways_full", size: [9, 9, 28], markers: &[(Marker::Treasure, [3, 3, 24], Facing::North), (Marker::Supply, [5, 4, 8], Facing::South), (Marker::Map, [6, 4, 19], Facing::North)] },
    Facts { name: "sideways_full_degraded", size: [9, 9, 28], markers: &[(Marker::Treasure, [3, 3, 24], Facing::North), (Marker::Supply, [5, 4, 8], Facing::South), (Marker::Map, [6, 4, 19], Facing::North)] },
    Facts { name: "upsidedown_backhalf", size: [9, 9, 16], markers: &[(Marker::Treasure, [2, 3, 12], Facing::North), (Marker::Map, [3, 6, 5], Facing::East)] },
    Facts { name: "upsidedown_backhalf_degraded", size: [9, 9, 16], markers: &[(Marker::Treasure, [2, 3, 12], Facing::North), (Marker::Map, [3, 6, 5], Facing::East)] },
    Facts { name: "upsidedown_fronthalf", size: [9, 9, 22], markers: &[(Marker::Supply, [4, 6, 8], Facing::South), (Marker::Map, [3, 6, 17], Facing::East)] },
    Facts { name: "upsidedown_fronthalf_degraded", size: [9, 9, 22], markers: &[(Marker::Supply, [4, 6, 8], Facing::South), (Marker::Map, [3, 6, 17], Facing::East)] },
    Facts { name: "upsidedown_full", size: [9, 9, 28], markers: &[(Marker::Treasure, [2, 3, 24], Facing::North), (Marker::Supply, [4, 6, 8], Facing::South), (Marker::Map, [3, 6, 17], Facing::East)] },
    Facts { name: "upsidedown_full_degraded", size: [9, 9, 28], markers: &[(Marker::Treasure, [2, 3, 24], Facing::North), (Marker::Supply, [4, 6, 8], Facing::South), (Marker::Map, [3, 6, 17], Facing::East)] },
    Facts { name: "with_mast", size: [9, 21, 28], markers: &[(Marker::Supply, [4, 3, 9], Facing::South), (Marker::Map, [5, 3, 18], Facing::West), (Marker::Treasure, [6, 5, 24], Facing::North)] },
    Facts { name: "with_mast_degraded", size: [9, 21, 28], markers: &[(Marker::Supply, [4, 3, 9], Facing::South), (Marker::Map, [5, 3, 18], Facing::West), (Marker::Treasure, [6, 5, 24], Facing::North)] },
];

/// The eight palettes: the wood replacing the first palette's oak (hull)
/// and spruce (trim), in palette order.
pub const PALETTES: [(&str, &str); 8] = [
    ("oak", "spruce"),
    ("jungle", "spruce"),
    ("dark_oak", "jungle"),
    ("dark_oak", "spruce"),
    ("spruce", "oak"),
    ("spruce", "jungle"),
    ("spruce", "dark_oak"),
    ("oak", "birch"),
];

pub fn template_index(name: &str) -> Option<u8> {
    TEMPLATES.iter().position(|t| t.name == name).map(|i| i as u8)
}

/// Which of a palette's two woods a cell takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wood {
    Hull,
    Trim,
}

/// One stored template cell in the template frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cell {
    Planks(Wood),
    /// Stairs: the side of the full half, and whether upside down.
    Stairs(Wood, Facing, bool),
    /// A slab, top half when true.
    Slab(Wood, bool),
    /// A log along y (0), x (1) or z (2).
    Log(Wood, u8),
    Fence(Wood),
    Chest(Facing),
}

/// The ship's half beam at `z` (bow at 0, stern at 27).
fn half_beam(z: i32) -> i32 {
    match z {
        0 => 1,
        1 => 2,
        2 | 3 => 3,
        4..=24 => 4,
        _ => 3,
    }
}

/// The first cabin row; the main deck runs ahead of it.
const CABIN: i32 = 21;

/// The whole ship, right side up, in the full templates' frame.
fn ship(mast: bool) -> FxHashMap<IVec3, Cell> {
    use Cell::*;
    use Wood::*;
    let mut m = FxHashMap::default();
    let mut put = |x: i32, y: i32, z: i32, c: Cell| {
        m.insert(IVec3::new(x, y, z), c);
    };
    for z in 0..28 {
        let w = half_beam(z);
        let (port, starboard) = (4 - w, 4 + w);
        // Keel and bottom.
        if (1..=26).contains(&z) {
            put(4, 0, z, Log(Hull, 2));
        }
        for x in 4 - (w - 3).max(0)..=4 + (w - 3).max(0) {
            if x != 4 {
                put(x, 0, z, Planks(Hull));
            }
        }
        // The bilge curves up to the sides.
        for x in port + 1..starboard {
            put(x, 1, z, Planks(Hull));
        }
        put(port, 1, z, Stairs(Hull, Facing::East, false));
        put(starboard, 1, z, Stairs(Hull, Facing::West, false));
        if z < CABIN {
            for y in 2..=3 {
                put(port, y, z, Planks(Hull));
                put(starboard, y, z, Planks(Hull));
            }
            for x in port + 1..starboard {
                put(x, 4, z, Planks(Trim));
            }
            put(port, 4, z, Planks(Hull));
            put(starboard, 4, z, Planks(Hull));
            put(port, 5, z, Fence(Trim));
            put(starboard, 5, z, Fence(Trim));
        } else {
            // The cabin: floor one below the deck, walls, a roof walk.
            put(port, 2, z, Planks(Hull));
            put(starboard, 2, z, Planks(Hull));
            for x in port..=starboard {
                put(x, 3, z, if x == port || x == starboard { Planks(Hull) } else { Planks(Trim) });
            }
            for y in 4..=6 {
                put(port, y, z, Planks(Hull));
                put(starboard, y, z, Planks(Hull));
            }
            for x in port..=starboard {
                put(x, 7, z, Slab(Trim, true));
            }
            put(port, 8, z, Fence(Trim));
            put(starboard, 8, z, Fence(Trim));
            if z == CABIN || z == 27 {
                for x in port + 1..starboard {
                    for y in 4..=6 {
                        // A doorway forward, windows aft.
                        let opening = if z == CABIN { x == 4 } else { y == 5 && (x == 3 || x == 5) };
                        if !opening {
                            put(x, y, z, Planks(Hull));
                        } else if z == 27 {
                            put(x, y, z, Fence(Trim));
                        }
                    }
                    put(x, 8, z, Fence(Trim));
                }
            }
        }
    }
    // The stem closes the bow, with a bowsprit post.
    for y in 1..=4 {
        put(4, y, 0, Planks(Hull));
    }
    put(4, 5, 0, Log(Hull, 0));
    put(4, 6, 0, Fence(Trim));
    // The cabin roof meets the deck in a step.
    for x in 1..=7 {
        put(x, 4, CABIN - 1, Planks(Trim));
    }
    put(4, 5, CABIN - 1, Stairs(Trim, Facing::South, false));
    // Hatches over the hold's chests.
    for p in [IVec3::new(4, 4, 8), IVec3::new(4, 4, 9), IVec3::new(5, 4, 18)] {
        m.remove(&p);
    }
    if mast {
        let mut put = |x: i32, y: i32, z: i32, c: Cell| {
            m.insert(IVec3::new(x, y, z), c);
        };
        for y in 2..=19 {
            put(4, y, 12, Log(Hull, 0));
        }
        for y in [12, 16] {
            for x in 1..=7 {
                if x != 4 {
                    put(x, y, 12, Fence(Trim));
                }
            }
            put(1, y - 1, 12, Fence(Trim));
            put(7, y - 1, 12, Fence(Trim));
        }
        for x in 3..=5 {
            for z in 11..=13 {
                if (x, z) != (4, 12) {
                    put(x, 18, z, Slab(Trim, false));
                }
            }
        }
        put(4, 20, 12, Fence(Trim));
    }
    m
}

/// A cell turned over end to end across the beam: x and y mirrored.
fn flipped(c: Cell) -> Cell {
    match c {
        Cell::Stairs(w, f, top) => Cell::Stairs(w, if f.along_x() { f.opposite() } else { f }, !top),
        Cell::Slab(w, top) => Cell::Slab(w, !top),
        c => c,
    }
}

/// A cell rolled onto its side (the y axis becomes x): stairs and slabs
/// can't lie on their sides, so they become planks.
fn rolled(c: Cell) -> Cell {
    match c {
        Cell::Stairs(w, ..) | Cell::Slab(w, _) => Cell::Planks(w),
        Cell::Log(w, 0) => Cell::Log(w, 1),
        Cell::Log(w, 1) => Cell::Log(w, 0),
        c => c,
    }
}

/// Template `index`'s cells, built once.
pub fn cells(index: u8) -> &'static [(IVec3, Cell)] {
    static CELLS: OnceLock<Vec<Vec<(IVec3, Cell)>>> = OnceLock::new();
    &CELLS.get_or_init(|| (0..TEMPLATES.len() as u8).map(build).collect())[index as usize]
}

fn build(index: u8) -> Vec<(IVec3, Cell)> {
    let t = &TEMPLATES[index as usize];
    let name = t.name;
    let size = IVec3::from(t.size);
    let whole = ship(name.starts_with("with_mast"));
    // The part of the 28-long ship this template keeps.
    let from = if name.contains("backhalf") { 28 - size.z } else { 0 };
    let mut out: FxHashMap<IVec3, Cell> = FxHashMap::default();
    for (&p, &c) in &whole {
        if p.z < from || p.z >= from + size.z {
            continue;
        }
        let (q, c) = if name.starts_with("upsidedown") {
            (IVec3::new(8 - p.x, 8 - p.y, p.z - from), flipped(c))
        } else if name.starts_with("sideways") {
            (IVec3::new(8 - p.y, p.x, p.z - from), rolled(c))
        } else {
            (IVec3::new(p.x, p.y, p.z - from), c)
        };
        // A ragged break where a half parted from the rest.
        let edge = if name.contains("fronthalf") {
            size.z - 1 - q.z
        } else if name.contains("backhalf") {
            q.z
        } else {
            i32::MAX
        };
        let h = hash3(q.x, q.y, q.z, 0x5817 + u64::from(index));
        if edge < 3 && h % 4 < (3 - edge) as u64 {
            continue;
        }
        if name.ends_with("_degraded") && h % 100 < 14 {
            continue;
        }
        out.insert(q, c);
    }
    for &(_, at, facing) in t.markers {
        let at = IVec3::from(at);
        out.remove(&at);
        out.insert(at - IVec3::Y, Cell::Chest(facing));
    }
    let mut v: Vec<(IVec3, Cell)> =
        out.into_iter().filter(|(p, _)| p.cmplt(size).all() && p.cmpge(IVec3::ZERO).all()).collect();
    v.sort_unstable_by_key(|(p, _)| (p.y, p.z, p.x));
    v
}

/// A cell's block in `palette`, turned by `rotation`.
pub fn resolve(c: Cell, palette: u8, rotation: Rotation) -> Block {
    let (hull, trim) = PALETTES[palette as usize];
    let wood = |w: Wood| if w == Wood::Hull { hull } else { trim };
    match c {
        Cell::Planks(w) => named(&format!("{}_planks", wood(w))),
        Cell::Stairs(w, f, top) => {
            let base = named(&format!("{}_stairs", wood(w)));
            match ss::stairs(base) {
                Some(s) => ss::stairs_id(ss::Stairs {
                    facing: rotation.facing(f),
                    top,
                    shape: StairShape::Straight,
                    wet: false,
                    ..s
                }),
                None => base,
            }
        }
        Cell::Slab(w, top) => {
            let base = named(&format!("{}_slab", wood(w)));
            match ss::slab(base) {
                Some(s) => {
                    ss::slab_id(ss::Slab { kind: if top { SlabType::Top } else { SlabType::Bottom }, wet: false, ..s })
                }
                None => base,
            }
        }
        Cell::Log(w, axis) => {
            let base = named(&format!("{}_log", wood(w)));
            let quarter = matches!(rotation, Rotation::Clockwise90 | Rotation::Counterclockwise90);
            let normal = match (axis, quarter) {
                (0, _) => IVec3::Y,
                (1, false) | (2, true) => IVec3::X,
                _ => IVec3::Z,
            };
            super::super::wood::axis_placed(base, normal).unwrap_or(base)
        }
        Cell::Fence(w) => named(&format!("{}_fence", wood(w))),
        Cell::Chest(f) => Block::CHEST.with_facing(rotation.facing(f)),
    }
}
