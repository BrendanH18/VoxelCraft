//! v0.15 Track F: woodland mansions (GEN-05i), in generation 4 only.
//!
//! Placement, layout and residents follow Java 26.3:
//!
//! - `StructureSets` places `woodland_mansions` with a triangular random
//!   spread of spacing 80, separation 20 and salt 10387319 (16-block chunks).
//! - `WoodlandMansionStructure.findGenerationPoint` starts at block (7, 7) of
//!   the chunk when a `has_structure/woodland_mansion` biome (dark forest or
//!   pale garden) can exist in that column, draws the rotation, and takes the
//!   lowest `WORLD_SURFACE_WG` height of four corners five blocks apart; a
//!   start below y 60 or outside those biomes is dropped.
//! - `WoodlandMansionPieces` builds the 11 × 11 `MansionGrid` (corridors,
//!   rooms of 1×1, 1×2 and 2×2 cells, secret rooms, the third floor and its
//!   stairs room) and places every piece exactly as `MansionPiecePlacer`
//!   does, from the same `WorldgenRandom` sequence (`setLargeFeatureSeed`).
//! - `afterPlace` fills cobblestone under the first floor down to the ground.
//!
//! The templates the pieces name are original (fidelity exception FE-02,
//! see [`rooms`]): each has the size of Java's template of that name and
//! carries Java's data markers (chests, evokers, vindicators, allays) and
//! fixed containers at Java's positions. Everything else in a room is our
//! own design of the same room kind.
//!
//! A layout is built once per start and cached by [`super::temples`], which
//! owns placement lookups, chests, residents and `/locate` for every
//! surface structure; see `docs/verification/2026-11-18-v0.15-mansions.md`.

mod rooms;
#[cfg(test)]
mod tests;

use glam::{IVec2, IVec3};

use super::block::{Block, Facing};
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::fortress::Feature;
use super::noise::hash3;
use super::structure::Bounds;
use super::terrain::{Biome, Dimension, Generation, Generator};
use crate::enchant::JavaRandom;
use crate::entity::MobKind;

pub use rooms::{FIXED, Fixed, Marker, TEMPLATES, Template, fixed_chest, template, template_index};

/// `StructureSets`' `woodland_mansions` placement, in 16-block chunks.
pub const SPACING: i32 = 80;
pub const SEPARATION: i32 = 20;
pub const SALT: u64 = 10_387_319;
/// `WoodlandMansionStructure.START_OFFSET_IN_CHUNK`.
pub const START_OFFSET: i32 = 7;
/// Starts lower than this are dropped (`findGenerationPoint`).
pub const MIN_START_Y: i32 = 60;
/// Furthest a mansion's blocks reach from its start, horizontally: the grid
/// spans eight cells of eight blocks west of the entrance and the entrance
/// porch eleven east, in any rotation.
pub const REACH: i32 = 80;

/// `#has_structure/woodland_mansion`.
pub fn allowed(biome: Biome) -> bool {
    matches!(biome, Biome::DarkForest | Biome::PaleGarden)
}

/// Java's `Rotation`, in its declaration order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

impl Rotation {
    pub const ALL: [Rotation; 4] =
        [Rotation::None, Rotation::Clockwise90, Rotation::Clockwise180, Rotation::Counterclockwise90];

    fn turns(self) -> u8 {
        self as u8
    }

    /// `Rotation.getRotated`.
    pub fn then(self, other: Rotation) -> Rotation {
        Self::ALL[((self.turns() + other.turns()) & 3) as usize]
    }

    /// `Rotation.rotate(Direction)`.
    pub fn facing(self, f: Facing) -> Facing {
        (0..self.turns()).fold(f, |f, _| f.clockwise())
    }

    /// `BlockPos.rotate`.
    pub fn pos(self, p: IVec3) -> IVec3 {
        match self {
            Rotation::None => p,
            Rotation::Clockwise90 => IVec3::new(-p.z, p.y, p.x),
            Rotation::Clockwise180 => IVec3::new(-p.x, p.y, -p.z),
            Rotation::Counterclockwise90 => IVec3::new(p.z, p.y, -p.x),
        }
    }

    fn inverse(self) -> Rotation {
        Self::ALL[((4 - self.turns()) & 3) as usize]
    }
}

/// Java's `Mirror`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Mirror {
    None,
    /// Flips z (north and south).
    LeftRight,
    /// Flips x (east and west).
    FrontBack,
}

impl Mirror {
    fn pos(self, p: IVec3) -> IVec3 {
        match self {
            Mirror::None => p,
            Mirror::LeftRight => IVec3::new(p.x, p.y, -p.z),
            Mirror::FrontBack => IVec3::new(-p.x, p.y, p.z),
        }
    }

    fn facing(self, f: Facing) -> Facing {
        match (self, f) {
            (Mirror::LeftRight, Facing::North | Facing::South) | (Mirror::FrontBack, Facing::East | Facing::West) => {
                f.opposite()
            }
            _ => f,
        }
    }
}

/// `StructureTemplate.transform` about the template origin: mirror, then rotate.
pub fn transform(p: IVec3, mirror: Mirror, rotation: Rotation) -> IVec3 {
    rotation.pos(mirror.pos(p))
}

/// A block facing in a placed template (`BlockState.mirror(..).rotate(..)`).
pub fn transform_facing(f: Facing, mirror: Mirror, rotation: Rotation) -> Facing {
    rotation.facing(mirror.facing(f))
}

/// Java's `WorldgenRandom.setLargeFeatureSeed` over a `LegacyRandomSource`.
pub fn large_feature_random(seed: u64, chunk: IVec2) -> JavaRandom {
    let mut r = JavaRandom::new(seed as i64);
    let next_long = |r: &mut JavaRandom| ((r.next_int() as i64) << 32).wrapping_add(r.next_int() as i64);
    let (a, b) = (next_long(&mut r), next_long(&mut r));
    JavaRandom::new((chunk.x as i64).wrapping_mul(a) ^ (chunk.y as i64).wrapping_mul(b) ^ seed as i64)
}

/// Where a mansion starts: its chunk, the first floor's corner and its rotation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Start {
    pub chunk: IVec2,
    pub origin: IVec3,
    pub rotation: Rotation,
}

/// `WoodlandMansionStructure.findGenerationPoint` followed by
/// `Structure.isValidBiome`, at a start chunk the placement chose.
pub fn start(g: &Generator, chunk: IVec2) -> Option<Start> {
    if g.generation < Generation::V4 || g.dimension != Dimension::Overworld {
        return None;
    }
    let (x, z) = (chunk.x * 16 + START_OFFSET, chunk.y * 16 + START_OFFSET);
    // `couldValidBiomeExistInTerrainColumn`: no randomness is drawn first.
    if !allowed(g.column(x, z).biome) {
        return None;
    }
    let mut rng = large_feature_random(g.seed, chunk);
    let rotation = Rotation::ALL[rng.next_bounded(4) as usize];
    // `getLowestYIn5by5Box`.
    let (ox, oz) = match rotation {
        Rotation::None => (5, 5),
        Rotation::Clockwise90 => (-5, 5),
        Rotation::Clockwise180 => (-5, -5),
        Rotation::Counterclockwise90 => (5, -5),
    };
    let first_occupied = |x: i32, z: i32| g.structure_column(x, z).top(false) + 1;
    let y = [(x, z), (x, z + oz), (x + ox, z), (x + ox, z + oz)]
        .into_iter()
        .map(|(x, z)| first_occupied(x, z))
        .min()
        .unwrap();
    if y < MIN_START_Y {
        return None;
    }
    let origin = IVec3::new(x, y, z);
    allowed(g.biome_at(origin)).then_some(Start { chunk, origin, rotation })
}

/// One template placed by `MansionPiecePlacer`: its name (an index into
/// [`TEMPLATES`]), the world position of its template origin, rotation and mirror.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Piece {
    pub template: u8,
    pub pos: IVec3,
    pub rotation: Rotation,
    pub mirror: Mirror,
}

impl Piece {
    pub fn name(&self) -> &'static str {
        TEMPLATES[self.template as usize].name
    }

    /// `StructureTemplate.getBoundingBox` for the piece.
    pub fn bounds(&self) -> Bounds {
        let size = TEMPLATES[self.template as usize].size;
        let a = transform(IVec3::ZERO, self.mirror, self.rotation);
        let b = transform(IVec3::from(size) - IVec3::ONE, self.mirror, self.rotation);
        Bounds { min: self.pos + a.min(b), max: self.pos + a.max(b) }
    }

    /// World position of template cell `local`.
    pub fn world(&self, local: IVec3) -> IVec3 {
        self.pos + transform(local, self.mirror, self.rotation)
    }

    /// Template cell at world position `world` (the inverse of [`Piece::world`]).
    pub fn local(&self, world: IVec3) -> IVec3 {
        self.mirror.pos(self.rotation.inverse().pos(world - self.pos))
    }
}

/// Every piece of the mansion at `start`, in Java's order.
pub fn pieces(seed: u64, start: &Start) -> Vec<Piece> {
    let mut rng = large_feature_random(seed, start.chunk);
    // The rotation `findGenerationPoint` drew.
    rng.next_bounded(4);
    let grid = Grid::new(&mut rng);
    let mut placer = Placer { rng: &mut rng, start: IVec2::ZERO, pieces: Vec::new() };
    placer.create(start.origin, start.rotation, &grid);
    placer.pieces
}

// `MansionGrid` cell values and room flags.
const CLEAR: i32 = 0;
const CORRIDOR: i32 = 1;
const ROOM: i32 = 2;
const START_ROOM: i32 = 3;
const TEST_ROOM: i32 = 4;
const BLOCKED: i32 = 5;
const ROOM_1X1: i32 = 0x1_0000;
const ROOM_1X2: i32 = 0x2_0000;
const ROOM_2X2: i32 = 0x4_0000;
const ROOM_ORIGIN_FLAG: i32 = 0x10_0000;
const ROOM_DOOR_FLAG: i32 = 0x20_0000;
const ROOM_STAIRS_FLAG: i32 = 0x40_0000;
const ROOM_CORRIDOR_FLAG: i32 = 0x80_0000;
const ROOM_TYPE_MASK: i32 = 0xF_0000;
const ROOM_ID_MASK: i32 = 0xFFFF;

/// `Direction.from2DDataValue` order.
const BY_2D: [Facing; 4] = [Facing::South, Facing::West, Facing::North, Facing::East];
/// `Direction.Plane.HORIZONTAL` order.
const HORIZONTAL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

fn step(f: Facing) -> IVec2 {
    let o = f.offset();
    IVec2::new(o.x, o.z)
}

/// `WoodlandMansionPieces.SimpleGrid`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleGrid {
    width: i32,
    height: i32,
    outside: i32,
    cells: Vec<i32>,
}

impl SimpleGrid {
    fn new(width: i32, height: i32, outside: i32) -> Self {
        Self { width, height, outside, cells: vec![0; (width * height) as usize] }
    }

    fn set(&mut self, x: i32, y: i32, v: i32) {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.cells[(x * self.height + y) as usize] = v;
        }
    }

    fn set_box(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, v: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, v);
            }
        }
    }

    pub fn get(&self, x: i32, y: i32) -> i32 {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.cells[(x * self.height + y) as usize]
        } else {
            self.outside
        }
    }

    fn set_if(&mut self, x: i32, y: i32, if_value: i32, v: i32) {
        if self.get(x, y) == if_value {
            self.set(x, y, v);
        }
    }

    fn edges_to(&self, x: i32, y: i32, v: i32) -> bool {
        self.get(x - 1, y) == v || self.get(x + 1, y) == v || self.get(x, y + 1) == v || self.get(x, y - 1) == v
    }
}

fn is_house(grid: &SimpleGrid, x: i32, y: i32) -> bool {
    matches!(grid.get(x, y), CORRIDOR | ROOM | START_ROOM | TEST_ROOM)
}

/// `WoodlandMansionPieces.MansionGrid`.
pub struct Grid {
    pub base: SimpleGrid,
    pub third: SimpleGrid,
    pub rooms: [SimpleGrid; 3],
    entrance: IVec2,
}

impl Grid {
    fn new(rng: &mut JavaRandom) -> Self {
        let e = IVec2::new(7, 4);
        let mut base = SimpleGrid::new(11, 11, BLOCKED);
        base.set_box(e.x, e.y, e.x + 1, e.y + 1, START_ROOM);
        base.set_box(e.x - 1, e.y, e.x - 1, e.y + 1, ROOM);
        base.set_box(e.x + 2, e.y - 2, e.x + 3, e.y + 3, BLOCKED);
        base.set_box(e.x + 1, e.y - 2, e.x + 1, e.y - 1, CORRIDOR);
        base.set_box(e.x + 1, e.y + 2, e.x + 1, e.y + 3, CORRIDOR);
        base.set(e.x - 1, e.y - 1, CORRIDOR);
        base.set(e.x - 1, e.y + 2, CORRIDOR);
        base.set_box(0, 0, 11, 1, BLOCKED);
        base.set_box(0, 9, 11, 11, BLOCKED);
        corridor(rng, &mut base, e.x, e.y - 2, Facing::West, 6);
        corridor(rng, &mut base, e.x, e.y + 3, Facing::West, 6);
        corridor(rng, &mut base, e.x - 2, e.y - 1, Facing::West, 3);
        corridor(rng, &mut base, e.x - 2, e.y + 2, Facing::West, 3);
        while clean_edges(&mut base) {}
        let mut rooms =
            [SimpleGrid::new(11, 11, BLOCKED), SimpleGrid::new(11, 11, BLOCKED), SimpleGrid::new(11, 11, BLOCKED)];
        identify_rooms(rng, &base, &mut rooms[0]);
        identify_rooms(rng, &base, &mut rooms[1]);
        rooms[0].set_box(e.x + 1, e.y, e.x + 1, e.y + 1, ROOM_CORRIDOR_FLAG);
        rooms[1].set_box(e.x + 1, e.y, e.x + 1, e.y + 1, ROOM_CORRIDOR_FLAG);
        let mut grid = Grid { third: SimpleGrid::new(base.width, base.height, BLOCKED), base, rooms, entrance: e };
        grid.setup_third_floor(rng);
        let third = grid.third.clone();
        identify_rooms(rng, &third, &mut grid.rooms[2]);
        grid
    }

    fn is_room_id(&self, x: i32, y: i32, floor: usize, id: i32) -> bool {
        (self.rooms[floor].get(x, y) & ROOM_ID_MASK) == id
    }

    fn room_1x2_direction(&self, x: i32, y: i32, floor: usize, id: i32) -> Option<Facing> {
        HORIZONTAL.into_iter().find(|&d| {
            let s = step(d);
            self.is_room_id(x + s.x, y + s.y, floor, id)
        })
    }

    fn setup_third_floor(&mut self, rng: &mut JavaRandom) {
        let mut potential = Vec::new();
        for y in 0..self.third.height {
            for x in 0..self.third.width {
                let data = self.rooms[1].get(x, y);
                if data & ROOM_TYPE_MASK == ROOM_1X2 && data & ROOM_DOOR_FLAG == ROOM_DOOR_FLAG {
                    potential.push(IVec2::new(x, y));
                }
            }
        }
        let (w, h) = (self.third.width, self.third.height);
        if potential.is_empty() {
            self.third.set_box(0, 0, w, h, BLOCKED);
            return;
        }
        let room = potential[rng.next_bounded(potential.len() as i32) as usize];
        let data = self.rooms[1].get(room.x, room.y);
        self.rooms[1].set(room.x, room.y, data | ROOM_STAIRS_FLAG);
        let dir = self.room_1x2_direction(room.x, room.y, 1, data & ROOM_ID_MASK).expect("a 1x2 room has two cells");
        let end = room + step(dir);
        for y in 0..h {
            for x in 0..w {
                if !is_house(&self.base, x, y) {
                    self.third.set(x, y, BLOCKED);
                } else if x == room.x && y == room.y {
                    self.third.set(x, y, START_ROOM);
                } else if x == end.x && y == end.y {
                    self.third.set(x, y, START_ROOM);
                    self.rooms[2].set(x, y, ROOM_CORRIDOR_FLAG);
                }
            }
        }
        let corridors: Vec<Facing> = HORIZONTAL
            .into_iter()
            .filter(|&d| {
                let s = end + step(d);
                self.third.get(s.x, s.y) == CLEAR
            })
            .collect();
        if corridors.is_empty() {
            self.third.set_box(0, 0, w, h, BLOCKED);
            self.rooms[1].set(room.x, room.y, data);
        } else {
            let d = corridors[rng.next_bounded(corridors.len() as i32) as usize];
            let s = end + step(d);
            corridor(rng, &mut self.third, s.x, s.y, d, 4);
            while clean_edges(&mut self.third) {}
        }
    }
}

/// `MansionGrid.recursiveCorridor`.
fn corridor(rng: &mut JavaRandom, grid: &mut SimpleGrid, x: i32, y: i32, heading: Facing, depth: i32) {
    if depth <= 0 {
        return;
    }
    let h = step(heading);
    grid.set(x, y, CORRIDOR);
    grid.set_if(x + h.x, y + h.y, CLEAR, CORRIDOR);
    for _ in 0..8 {
        let next = BY_2D[rng.next_bounded(4) as usize];
        if next != heading.opposite() && (next != Facing::East || !rng.next_boolean()) {
            let (nx, ny) = (x + h.x, y + h.y);
            let n = step(next);
            if grid.get(nx + n.x, ny + n.y) == CLEAR && grid.get(nx + n.x * 2, ny + n.y * 2) == CLEAR {
                corridor(rng, grid, x + h.x + n.x, y + h.y + n.y, next, depth - 1);
                break;
            }
        }
    }
    let (cw, ccw) = (step(heading.clockwise()), step(heading.counter_clockwise()));
    grid.set_if(x + cw.x, y + cw.y, CLEAR, ROOM);
    grid.set_if(x + ccw.x, y + ccw.y, CLEAR, ROOM);
    grid.set_if(x + h.x + cw.x, y + h.y + cw.y, CLEAR, ROOM);
    grid.set_if(x + h.x + ccw.x, y + h.y + ccw.y, CLEAR, ROOM);
    grid.set_if(x + h.x * 2, y + h.y * 2, CLEAR, ROOM);
    grid.set_if(x + cw.x * 2, y + cw.y * 2, CLEAR, ROOM);
    grid.set_if(x + ccw.x * 2, y + ccw.y * 2, CLEAR, ROOM);
}

/// `MansionGrid.cleanEdges`.
fn clean_edges(grid: &mut SimpleGrid) -> bool {
    let mut touched = false;
    for y in 0..grid.height {
        for x in 0..grid.width {
            if grid.get(x, y) != CLEAR {
                continue;
            }
            let house = |dx: i32, dy: i32| is_house(grid, x + dx, y + dy) as i32;
            let direct = house(1, 0) + house(-1, 0) + house(0, 1) + house(0, -1);
            if direct >= 3 {
                grid.set(x, y, ROOM);
                touched = true;
            } else if direct == 2 {
                let diagonal = house(1, 1) + house(-1, 1) + house(1, -1) + house(-1, -1);
                if diagonal <= 1 {
                    grid.set(x, y, ROOM);
                    touched = true;
                }
            }
        }
    }
    touched
}

/// `MansionGrid.identifyRooms`.
fn identify_rooms(rng: &mut JavaRandom, from: &SimpleGrid, rooms: &mut SimpleGrid) {
    let mut cells = Vec::new();
    for y in 0..from.height {
        for x in 0..from.width {
            if from.get(x, y) == ROOM {
                cells.push(IVec2::new(x, y));
            }
        }
    }
    // `Util.shuffle`.
    for i in (2..=cells.len()).rev() {
        let j = rng.next_bounded(i as i32) as usize;
        cells.swap(i - 1, j);
    }
    let mut id = 10;
    for c in cells {
        let (x, y) = (c.x, c.y);
        if rooms.get(x, y) != CLEAR {
            continue;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (x, x, y, y);
        let mut kind = ROOM_1X1;
        let free = |rooms: &SimpleGrid, dx: i32, dy: i32| rooms.get(x + dx, y + dy) == CLEAR;
        let room = |dx: i32, dy: i32| from.get(x + dx, y + dy) == ROOM;
        if free(rooms, 1, 0) && free(rooms, 0, 1) && free(rooms, 1, 1) && room(1, 0) && room(0, 1) && room(1, 1) {
            x1 = x + 1;
            y1 = y + 1;
            kind = ROOM_2X2;
        } else if free(rooms, -1, 0)
            && free(rooms, 0, 1)
            && free(rooms, -1, 1)
            && room(-1, 0)
            && room(0, 1)
            && room(-1, 1)
        {
            x0 = x - 1;
            y1 = y + 1;
            kind = ROOM_2X2;
        } else if free(rooms, -1, 0)
            && free(rooms, 0, -1)
            && free(rooms, -1, -1)
            && room(-1, 0)
            && room(0, -1)
            && room(-1, -1)
        {
            x0 = x - 1;
            y0 = y - 1;
            kind = ROOM_2X2;
        } else if free(rooms, 1, 0) && room(1, 0) {
            x1 = x + 1;
            kind = ROOM_1X2;
        } else if free(rooms, 0, 1) && room(0, 1) {
            y1 = y + 1;
            kind = ROOM_1X2;
        } else if free(rooms, -1, 0) && room(-1, 0) {
            x0 = x - 1;
            kind = ROOM_1X2;
        } else if free(rooms, 0, -1) && room(0, -1) {
            y0 = y - 1;
            kind = ROOM_1X2;
        }
        let mut door_x = if rng.next_boolean() { x0 } else { x1 };
        let mut door_y = if rng.next_boolean() { y0 } else { y1 };
        let mut door_flag = ROOM_DOOR_FLAG;
        let other = |v: i32, a: i32, b: i32| if v == a { b } else { a };
        if !from.edges_to(door_x, door_y, CORRIDOR) {
            door_x = other(door_x, x0, x1);
            door_y = other(door_y, y0, y1);
            if !from.edges_to(door_x, door_y, CORRIDOR) {
                door_y = other(door_y, y0, y1);
                if !from.edges_to(door_x, door_y, CORRIDOR) {
                    door_x = other(door_x, x0, x1);
                    door_y = other(door_y, y0, y1);
                    if !from.edges_to(door_x, door_y, CORRIDOR) {
                        door_flag = 0;
                        door_x = x0;
                        door_y = y0;
                    }
                }
            }
        }
        for ry in y0..=y1 {
            for rx in x0..=x1 {
                if rx == door_x && ry == door_y {
                    rooms.set(rx, ry, ROOM_ORIGIN_FLAG | door_flag | kind | id);
                } else {
                    rooms.set(rx, ry, kind | id);
                }
            }
        }
        id += 1;
    }
}

/// Which room templates a floor draws from (`FirstFloorRoomCollection`,
/// `SecondFloorRoomCollection`; the third floor uses the second's).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Floor {
    First,
    Upper,
}

impl Floor {
    fn room_1x1(self, rng: &mut JavaRandom) -> String {
        match self {
            Floor::First => format!("1x1_a{}", rng.next_bounded(5) + 1),
            Floor::Upper => format!("1x1_b{}", rng.next_bounded(5) + 1),
        }
    }

    fn secret_1x1(self, rng: &mut JavaRandom) -> String {
        format!("1x1_as{}", rng.next_bounded(4) + 1)
    }

    fn side_entrance(self, rng: &mut JavaRandom, stairs: bool) -> String {
        match self {
            Floor::First => format!("1x2_a{}", rng.next_bounded(9) + 1),
            Floor::Upper if stairs => "1x2_c_stairs".into(),
            Floor::Upper => format!("1x2_c{}", rng.next_bounded(4) + 1),
        }
    }

    fn front_entrance(self, rng: &mut JavaRandom, stairs: bool) -> String {
        match self {
            Floor::First => format!("1x2_b{}", rng.next_bounded(5) + 1),
            Floor::Upper if stairs => "1x2_d_stairs".into(),
            Floor::Upper => format!("1x2_d{}", rng.next_bounded(5) + 1),
        }
    }

    fn secret_1x2(self, rng: &mut JavaRandom) -> String {
        match self {
            Floor::First => format!("1x2_s{}", rng.next_bounded(2) + 1),
            Floor::Upper => format!("1x2_se{}", rng.next_bounded(1) + 1),
        }
    }

    fn room_2x2(self, rng: &mut JavaRandom) -> String {
        match self {
            Floor::First => format!("2x2_a{}", rng.next_bounded(4) + 1),
            Floor::Upper => format!("2x2_b{}", rng.next_bounded(5) + 1),
        }
    }
}

/// `PlacementData`: where the wall walk stands, which way it faces and
/// which wall template it lays.
struct Walk {
    pos: IVec3,
    rotation: Rotation,
    wall: &'static str,
}

/// A door direction, or `UP` for a room without a corridor beside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Door {
    Side(Facing),
    Up,
}

/// `MansionPiecePlacer`.
struct Placer<'a> {
    rng: &'a mut JavaRandom,
    /// `startX`, `startY`.
    start: IVec2,
    pieces: Vec<Piece>,
}

fn rel(p: IVec3, f: Facing, n: i32) -> IVec3 {
    p + f.offset() * n
}

impl Placer<'_> {
    fn add(&mut self, name: &str, pos: IVec3, rotation: Rotation, mirror: Mirror) {
        let template = template_index(name).unwrap_or_else(|| panic!("no mansion template {name}"));
        self.pieces.push(Piece { template, pos, rotation, mirror });
    }

    /// The north-west corner of grid cell `(x, y)` on the floor at `origin`.
    fn cell(&self, origin: IVec3, rotation: Rotation, x: i32, y: i32) -> IVec3 {
        let p = rel(origin, rotation.facing(Facing::South), 8 + (y - self.start.y) * 8);
        rel(p, rotation.facing(Facing::East), (x - self.start.x) * 8)
    }

    fn create(&mut self, origin: IVec3, rotation: Rotation, grid: &Grid) {
        let east = rotation.facing(Facing::East);
        let south = rotation.facing(Facing::South);
        let mut walk = Walk { pos: origin, rotation, wall: "wall_flat" };
        // `entrance`.
        self.add("entrance", rel(walk.pos, rotation.facing(Facing::West), 9), rotation, Mirror::None);
        walk.pos = rel(walk.pos, south, 16);
        let mut second = Walk { pos: walk.pos + IVec3::Y * 8, rotation, wall: "wall_window" };
        self.start = grid.entrance + IVec2::ONE;
        let end = IVec2::new(grid.entrance.x + 1, grid.entrance.y);
        let start = self.start;
        self.outer_walls(&mut walk, &grid.base, start, end);
        self.outer_walls(&mut second, &grid.base, start, end);
        let mut third = Walk { pos: walk.pos + IVec3::Y * 19, rotation, wall: "wall_window" };
        'found: for y in 0..grid.third.height {
            for x in (0..grid.third.width).rev() {
                if is_house(&grid.third, x, y) {
                    third.pos = rel(third.pos, south, 8 + (y - self.start.y) * 8);
                    third.pos = rel(third.pos, east, (x - self.start.x) * 8);
                    self.wall_piece(&mut third);
                    self.outer_walls(&mut third, &grid.third, IVec2::new(x, y), IVec2::new(x, y));
                    break 'found;
                }
            }
        }
        self.roof(origin + IVec3::Y * 16, rotation, &grid.base, Some(&grid.third));
        self.roof(origin + IVec3::Y * 27, rotation, &grid.third, None);
        let floors = [Floor::First, Floor::Upper, Floor::Upper];
        for (floor, &collection) in floors.iter().enumerate() {
            let floor_origin = origin + IVec3::Y * (8 * floor as i32 + if floor == 2 { 3 } else { 0 });
            let rooms = &grid.rooms[floor];
            let g = if floor == 2 { &grid.third } else { &grid.base };
            let carpet_south = if floor == 0 { "carpet_south_1" } else { "carpet_south_2" };
            let carpet_west = if floor == 0 { "carpet_west_1" } else { "carpet_west_2" };
            let joins = |x: i32, y: i32| g.get(x, y) == CORRIDOR || rooms.get(x, y) & ROOM_CORRIDOR_FLAG != 0;
            for y in 0..g.height {
                for x in 0..g.width {
                    if g.get(x, y) != CORRIDOR {
                        continue;
                    }
                    let pos = self.cell(floor_origin, rotation, x, y);
                    self.add("corridor_floor", pos, rotation, Mirror::None);
                    if joins(x, y - 1) {
                        self.add("carpet_north", rel(pos, east, 1) + IVec3::Y, rotation, Mirror::None);
                    }
                    if joins(x + 1, y) {
                        self.add("carpet_east", rel(rel(pos, south, 1), east, 5) + IVec3::Y, rotation, Mirror::None);
                    }
                    if joins(x, y + 1) {
                        let p = rel(rel(pos, south, 5), rotation.facing(Facing::West), 1);
                        self.add(carpet_south, p, rotation, Mirror::None);
                    }
                    if joins(x - 1, y) {
                        let p = rel(rel(pos, rotation.facing(Facing::West), 1), rotation.facing(Facing::North), 1);
                        self.add(carpet_west, p, rotation, Mirror::None);
                    }
                }
            }
            let wall = if floor == 0 { "indoors_wall_1" } else { "indoors_wall_2" };
            let door_piece = if floor == 0 { "indoors_door_1" } else { "indoors_door_2" };
            for y in 0..g.height {
                for x in 0..g.width {
                    let third_start = floor == 2 && g.get(x, y) == START_ROOM;
                    if g.get(x, y) != ROOM && !third_start {
                        continue;
                    }
                    let data = rooms.get(x, y);
                    let kind = data & ROOM_TYPE_MASK;
                    let id = data & ROOM_ID_MASK;
                    let third_start = third_start && data & ROOM_CORRIDOR_FLAG == ROOM_CORRIDOR_FLAG;
                    let mut doors = Vec::new();
                    if data & ROOM_DOOR_FLAG == ROOM_DOOR_FLAG {
                        for d in HORIZONTAL {
                            let s = step(d);
                            if g.get(x + s.x, y + s.y) == CORRIDOR {
                                doors.push(d);
                            }
                        }
                    }
                    let door = if !doors.is_empty() {
                        Some(Door::Side(doors[self.rng.next_bounded(doors.len() as i32) as usize]))
                    } else if data & ROOM_ORIGIN_FLAG == ROOM_ORIGIN_FLAG {
                        Some(Door::Up)
                    } else {
                        None
                    };
                    let room = rel(self.cell(floor_origin, rotation, x, y), east, -1);
                    let pick = |d: Facing| if door == Some(Door::Side(d)) { door_piece } else { wall };
                    if is_house(g, x - 1, y) && !grid.is_room_id(x - 1, y, floor, id) {
                        self.add(pick(Facing::West), room, rotation, Mirror::None);
                    }
                    if g.get(x + 1, y) == CORRIDOR && !third_start {
                        self.add(pick(Facing::East), rel(room, east, 8), rotation, Mirror::None);
                    }
                    let turned = rotation.then(Rotation::Clockwise90);
                    if is_house(g, x, y + 1) && !grid.is_room_id(x, y + 1, floor, id) {
                        self.add(pick(Facing::South), rel(rel(room, south, 7), east, 7), turned, Mirror::None);
                    }
                    if g.get(x, y - 1) == CORRIDOR && !third_start {
                        let p = rel(rel(room, rotation.facing(Facing::North), 1), east, 7);
                        self.add(pick(Facing::North), p, turned, Mirror::None);
                    }
                    match (kind, door) {
                        (ROOM_1X1, _) => self.room_1x1(room, rotation, door, collection),
                        (ROOM_1X2, Some(door)) => {
                            let dir = grid.room_1x2_direction(x, y, floor, id).expect("a 1x2 room has two cells");
                            let stairs = data & ROOM_STAIRS_FLAG == ROOM_STAIRS_FLAG;
                            self.room_1x2(room, rotation, dir, door, collection, stairs);
                        }
                        (ROOM_2X2, Some(Door::Side(door))) => {
                            let mut dir = door.clockwise();
                            let s = step(dir);
                            if !grid.is_room_id(x + s.x, y + s.y, floor, id) {
                                dir = dir.opposite();
                            }
                            self.room_2x2(room, rotation, dir, door, collection);
                        }
                        (ROOM_2X2, Some(Door::Up)) => {
                            let name = collection.room_2x2_secret();
                            self.add(name, rel(room, east, 1), rotation, Mirror::None);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// `traverseOuterWalls`.
    fn outer_walls(&mut self, walk: &mut Walk, grid: &SimpleGrid, start: IVec2, end: IVec2) {
        let mut at = start;
        let mut dir = Facing::South;
        let first = dir;
        loop {
            let s = step(dir);
            let ahead = at + s;
            if !is_house(grid, ahead.x, ahead.y) {
                self.turn(walk);
                dir = dir.clockwise();
                if at != end || first != dir {
                    self.wall_piece(walk);
                }
            } else {
                let left = ahead + step(dir.counter_clockwise());
                if is_house(grid, left.x, left.y) {
                    self.inner_turn(walk);
                    at = ahead;
                    dir = dir.counter_clockwise();
                } else {
                    at = ahead;
                    if at != end || first != dir {
                        self.wall_piece(walk);
                    }
                }
            }
            if at == end && first == dir {
                break;
            }
        }
    }

    /// `traverseWallPiece`.
    fn wall_piece(&mut self, walk: &mut Walk) {
        let east = walk.rotation.facing(Facing::East);
        self.add(walk.wall, rel(walk.pos, east, 7), walk.rotation, Mirror::None);
        walk.pos = rel(walk.pos, walk.rotation.facing(Facing::South), 8);
    }

    /// `traverseTurn`.
    fn turn(&mut self, walk: &mut Walk) {
        let south = walk.rotation.facing(Facing::South);
        walk.pos = rel(walk.pos, south, -1);
        self.add("wall_corner", walk.pos, walk.rotation, Mirror::None);
        walk.pos = rel(walk.pos, south, -7);
        walk.pos = rel(walk.pos, walk.rotation.facing(Facing::West), -6);
        walk.rotation = walk.rotation.then(Rotation::Clockwise90);
    }

    /// `traverseInnerTurn`.
    fn inner_turn(&mut self, walk: &mut Walk) {
        walk.pos = rel(walk.pos, walk.rotation.facing(Facing::South), 6);
        walk.pos = rel(walk.pos, walk.rotation.facing(Facing::East), 8);
        walk.rotation = walk.rotation.then(Rotation::Counterclockwise90);
    }

    /// `createRoof`.
    fn roof(&mut self, origin: IVec3, rotation: Rotation, grid: &SimpleGrid, above: Option<&SimpleGrid>) {
        let (east, south, west, north) = (
            rotation.facing(Facing::East),
            rotation.facing(Facing::South),
            rotation.facing(Facing::West),
            rotation.facing(Facing::North),
        );
        let house = |x: i32, y: i32| is_house(grid, x, y);
        let r = |turn: Rotation| rotation.then(turn);
        let none = Mirror::None;
        for y in 0..grid.height {
            for x in 0..grid.width {
                let p = self.cell(origin, rotation, x, y);
                let covered = above.is_some_and(|a| is_house(a, x, y));
                if !house(x, y) || covered {
                    continue;
                }
                self.add("roof", p + IVec3::Y * 3, rotation, none);
                if !house(x + 1, y) {
                    self.add("roof_front", rel(p, east, 6), rotation, none);
                }
                if !house(x - 1, y) {
                    self.add("roof_front", rel(rel(p, east, 0), south, 7), r(Rotation::Clockwise180), none);
                }
                if !house(x, y - 1) {
                    self.add("roof_front", rel(p, west, 1), r(Rotation::Counterclockwise90), none);
                }
                if !house(x, y + 1) {
                    self.add("roof_front", rel(rel(p, east, 6), south, 6), r(Rotation::Clockwise90), none);
                }
            }
        }
        if let Some(above) = above {
            for y in 0..grid.height {
                for x in 0..grid.width {
                    let p = self.cell(origin, rotation, x, y);
                    if !house(x, y) || !is_house(above, x, y) {
                        continue;
                    }
                    if !house(x + 1, y) {
                        self.add("small_wall", rel(p, east, 7), rotation, none);
                    }
                    if !house(x - 1, y) {
                        self.add("small_wall", rel(rel(p, west, 1), south, 6), r(Rotation::Clockwise180), none);
                    }
                    if !house(x, y - 1) {
                        self.add("small_wall", rel(rel(p, west, 0), north, 1), r(Rotation::Counterclockwise90), none);
                    }
                    if !house(x, y + 1) {
                        self.add("small_wall", rel(rel(p, east, 6), south, 7), r(Rotation::Clockwise90), none);
                    }
                    if !house(x + 1, y) {
                        if !house(x, y - 1) {
                            self.add("small_wall_corner", rel(rel(p, east, 7), north, 2), rotation, none);
                        }
                        if !house(x, y + 1) {
                            let q = rel(rel(p, east, 8), south, 7);
                            self.add("small_wall_corner", q, r(Rotation::Clockwise90), none);
                        }
                    }
                    if !house(x - 1, y) {
                        if !house(x, y - 1) {
                            let q = rel(rel(p, west, 2), north, 1);
                            self.add("small_wall_corner", q, r(Rotation::Counterclockwise90), none);
                        }
                        if !house(x, y + 1) {
                            let q = rel(rel(p, west, 1), south, 8);
                            self.add("small_wall_corner", q, r(Rotation::Clockwise180), none);
                        }
                    }
                }
            }
        }
        for y in 0..grid.height {
            for x in 0..grid.width {
                let p = self.cell(origin, rotation, x, y);
                let covered = above.is_some_and(|a| is_house(a, x, y));
                if !house(x, y) || covered {
                    continue;
                }
                if !house(x + 1, y) {
                    let p2 = rel(p, east, 6);
                    if !house(x, y + 1) {
                        self.add("roof_corner", rel(p2, south, 6), rotation, none);
                    } else if house(x + 1, y + 1) {
                        self.add("roof_inner_corner", rel(p2, south, 5), rotation, none);
                    }
                    if !house(x, y - 1) {
                        self.add("roof_corner", p2, r(Rotation::Counterclockwise90), none);
                    } else if house(x + 1, y - 1) {
                        let q = rel(rel(p, east, 9), north, 2);
                        self.add("roof_inner_corner", q, r(Rotation::Clockwise90), none);
                    }
                }
                if !house(x - 1, y) {
                    let p2 = rel(rel(p, east, 0), south, 0);
                    if !house(x, y + 1) {
                        self.add("roof_corner", rel(p2, south, 6), r(Rotation::Clockwise90), none);
                    } else if house(x - 1, y + 1) {
                        let q = rel(rel(p2, south, 8), west, 3);
                        self.add("roof_inner_corner", q, r(Rotation::Counterclockwise90), none);
                    }
                    if !house(x, y - 1) {
                        self.add("roof_corner", p2, r(Rotation::Clockwise180), none);
                    } else if house(x - 1, y - 1) {
                        self.add("roof_inner_corner", rel(p2, south, 1), r(Rotation::Clockwise180), none);
                    }
                }
            }
        }
    }

    /// `addRoom1x1`.
    fn room_1x1(&mut self, room: IVec3, rotation: Rotation, door: Option<Door>, floor: Floor) {
        let mut turn = Rotation::None;
        let mut name = floor.room_1x1(self.rng);
        match door {
            Some(Door::Side(Facing::East)) => {}
            Some(Door::Side(Facing::North)) => turn = turn.then(Rotation::Counterclockwise90),
            Some(Door::Side(Facing::West)) => turn = turn.then(Rotation::Clockwise180),
            Some(Door::Side(Facing::South)) => turn = turn.then(Rotation::Clockwise90),
            _ => name = floor.secret_1x1(self.rng),
        }
        let zero = zero_position(IVec3::new(1, 0, 0), Mirror::None, turn, 7, 7);
        let turn = turn.then(rotation);
        let zero = rotation.pos(zero);
        self.add(&name, room + IVec3::new(zero.x, 0, zero.z), turn, Mirror::None);
    }

    /// `addRoom1x2`.
    fn room_1x2(&mut self, room: IVec3, rotation: Rotation, dir: Facing, door: Door, floor: Floor, stairs: bool) {
        use Facing::*;
        let (east, south, north, west) =
            (rotation.facing(East), rotation.facing(South), rotation.facing(North), rotation.facing(West));
        let r = |turn: Rotation| rotation.then(turn);
        let (kind, pos, rot, mirror) = match (door, dir) {
            (Door::Side(East), South) => (0, rel(room, east, 1), rotation, Mirror::None),
            (Door::Side(East), North) => (0, rel(rel(room, east, 1), south, 6), rotation, Mirror::LeftRight),
            (Door::Side(West), North) => {
                (0, rel(rel(room, east, 7), south, 6), r(Rotation::Clockwise180), Mirror::None)
            }
            (Door::Side(West), South) => (0, rel(room, east, 7), rotation, Mirror::FrontBack),
            (Door::Side(South), East) => (0, rel(room, east, 1), r(Rotation::Clockwise90), Mirror::LeftRight),
            (Door::Side(South), West) => (0, rel(room, east, 7), r(Rotation::Clockwise90), Mirror::None),
            (Door::Side(North), West) => {
                (0, rel(rel(room, east, 7), south, 6), r(Rotation::Clockwise90), Mirror::FrontBack)
            }
            (Door::Side(North), East) => {
                (0, rel(rel(room, east, 1), south, 6), r(Rotation::Counterclockwise90), Mirror::None)
            }
            (Door::Side(South), North) => (1, rel(rel(room, east, 1), north, 8), rotation, Mirror::None),
            (Door::Side(North), South) => {
                (1, rel(rel(room, east, 7), south, 14), r(Rotation::Clockwise180), Mirror::None)
            }
            (Door::Side(West), East) => (1, rel(room, east, 15), r(Rotation::Clockwise90), Mirror::None),
            (Door::Side(East), West) => {
                (1, rel(rel(room, west, 7), south, 6), r(Rotation::Counterclockwise90), Mirror::None)
            }
            (Door::Up, East) => (2, rel(room, east, 15), r(Rotation::Clockwise90), Mirror::None),
            (Door::Up, South) => (2, rel(rel(room, east, 1), north, 0), rotation, Mirror::None),
            _ => return,
        };
        let name = match kind {
            0 => floor.side_entrance(self.rng, stairs),
            1 => floor.front_entrance(self.rng, stairs),
            _ => floor.secret_1x2(self.rng),
        };
        self.add(&name, pos, rot, mirror);
    }

    /// `addRoom2x2`.
    fn room_2x2(&mut self, room: IVec3, rotation: Rotation, dir: Facing, door: Facing, floor: Floor) {
        use Facing::*;
        let r = |turn: Rotation| rotation.then(turn);
        let (east, south, rot, mirror) = match (door, dir) {
            (East, South) => (-7, 0, rotation, Mirror::None),
            (East, North) => (-7, 6, rotation, Mirror::LeftRight),
            (North, East) => (1, 14, r(Rotation::Counterclockwise90), Mirror::None),
            (North, West) => (7, 14, r(Rotation::Counterclockwise90), Mirror::LeftRight),
            (South, West) => (7, -8, r(Rotation::Clockwise90), Mirror::None),
            (South, East) => (1, -8, r(Rotation::Clockwise90), Mirror::LeftRight),
            (West, North) => (15, 6, r(Rotation::Clockwise180), Mirror::None),
            (West, South) => (15, 0, rotation, Mirror::FrontBack),
            _ => (0, 0, rotation, Mirror::None),
        };
        let pos = rel(rel(room, rotation.facing(East), east), rotation.facing(South), south);
        let name = floor.room_2x2(self.rng);
        self.add(&name, pos, rot, mirror);
    }
}

impl Floor {
    fn room_2x2_secret(self) -> &'static str {
        "2x2_s1"
    }
}

/// `StructureTemplate.getZeroPositionWithTransform`.
fn zero_position(zero: IVec3, mirror: Mirror, rotation: Rotation, size_x: i32, size_z: i32) -> IVec3 {
    let (sx, sz) = (size_x - 1, size_z - 1);
    let mx = if mirror == Mirror::FrontBack { sx } else { 0 };
    let mz = if mirror == Mirror::LeftRight { sz } else { 0 };
    zero + match rotation {
        Rotation::Counterclockwise90 => IVec3::new(mz, 0, sx - mx),
        Rotation::Clockwise90 => IVec3::new(sz - mz, 0, mx),
        Rotation::Clockwise180 => IVec3::new(sx - mx, 0, sz - mz),
        Rotation::None => IVec3::new(mx, 0, mz),
    }
}

/// One placed piece and its box.
#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub piece: Piece,
    pub bounds: Bounds,
}

/// A built mansion: its pieces, the cobblestone under its first floor,
/// its chests and its residents.
pub struct Layout {
    pub start: Start,
    pub pieces: Vec<Placed>,
    /// The pieces' box, grown down over the foundation.
    pub bounds: Bounds,
    /// The first floor (`afterPlace`'s `yStart`, the pieces' lowest y).
    pub floor_y: i32,
    /// Per column of `bounds`, x fastest: the lowest y filled with
    /// cobblestone, or `i16::MAX` where nothing is.
    foundation: Vec<i16>,
    pub features: Vec<(IVec3, Feature)>,
    pub residents: Vec<(MobKind, IVec3)>,
}

impl Layout {
    pub fn build(g: &Generator, start: Start) -> Layout {
        let pieces: Vec<Placed> =
            pieces(g.seed, &start).into_iter().map(|piece| Placed { piece, bounds: piece.bounds() }).collect();
        let mut bounds = pieces[0].bounds;
        for p in &pieces {
            bounds = bounds.union(&p.bounds);
        }
        let floor_y = bounds.min.y;
        // `afterPlace`: a column gets a foundation when the block at the
        // first floor is not air once every piece is placed.
        let width = bounds.max.x - bounds.min.x + 1;
        let depth = bounds.max.z - bounds.min.z + 1;
        let mut floor = vec![false; (width * depth) as usize];
        for p in pieces.iter().filter(|p| p.bounds.min.y <= floor_y) {
            let t = template(p.piece.template);
            for z in p.bounds.min.z..=p.bounds.max.z {
                for x in p.bounds.min.x..=p.bounds.max.x {
                    let w = IVec3::new(x, floor_y, z);
                    let i = ((x - bounds.min.x) + (z - bounds.min.z) * width) as usize;
                    if let Some(block) = marker_block(p, w).or_else(|| t.cell(p.piece.local(w)).map(|c| c.block)) {
                        floor[i] = block != Block::AIR;
                    }
                }
            }
        }
        let mut foundation = vec![i16::MAX; floor.len()];
        let mut lowest = floor_y;
        for (i, &solid) in floor.iter().enumerate() {
            if !solid {
                continue;
            }
            let (x, z) = (bounds.min.x + i as i32 % width, bounds.min.z + i as i32 / width);
            let ground = g.solid_below(x, z, floor_y);
            if ground < floor_y - 1 {
                foundation[i] = (ground + 1) as i16;
                lowest = lowest.min(ground + 1);
            }
        }
        bounds.min.y = lowest;
        let (features, residents) = residents_and_features(g.seed, &start, &pieces);
        Layout { start, pieces, bounds, floor_y, foundation, features, residents }
    }

    /// Paints the part of the mansion inside the chunk at `base`.
    pub fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        let chunk = Bounds { min: base, max: top };
        if !self.bounds.intersects(&chunk) {
            return;
        }
        for p in self.pieces.iter().filter(|p| p.bounds.intersects(&chunk)) {
            let t = template(p.piece.template);
            let (lo, hi) = (p.bounds.min.max(base), p.bounds.max.min(top));
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        let w = IVec3::new(x, y, z);
                        if let Some(cell) = t.cell(p.piece.local(w)) {
                            let l = w - base;
                            blocks[index(l.x as usize, l.y as usize, l.z as usize)] =
                                cell.resolve(p.piece.mirror, p.piece.rotation);
                        }
                    }
                }
            }
            // Data markers: the structure block is skipped, then a chest
            // replaces it or a mob spawned there leaves air.
            for &(marker, local) in t.markers() {
                let w = p.piece.world(local);
                if chunk.contains(w) {
                    let l = w - base;
                    blocks[index(l.x as usize, l.y as usize, l.z as usize)] = marker.block(p.piece.rotation);
                }
            }
        }
        let width = self.bounds.max.x - self.bounds.min.x + 1;
        for z in base.z.max(self.bounds.min.z)..=top.z.min(self.bounds.max.z) {
            for x in base.x.max(self.bounds.min.x)..=top.x.min(self.bounds.max.x) {
                let low = self.foundation[((x - self.bounds.min.x) + (z - self.bounds.min.z) * width) as usize];
                if low == i16::MAX {
                    continue;
                }
                for y in (low as i32).max(base.y)..=(self.floor_y - 1).min(top.y) {
                    let l = IVec3::new(x, y, z) - base;
                    blocks[index(l.x as usize, l.y as usize, l.z as usize)] = Block::COBBLESTONE;
                }
            }
        }
    }

    /// The cobblestone column under `(x, z)`, as `(lowest, highest)` y.
    pub fn foundation_at(&self, x: i32, z: i32) -> Option<(i32, i32)> {
        if !(self.bounds.min.x..=self.bounds.max.x).contains(&x)
            || !(self.bounds.min.z..=self.bounds.max.z).contains(&z)
        {
            return None;
        }
        let width = self.bounds.max.x - self.bounds.min.x + 1;
        let low = self.foundation[((x - self.bounds.min.x) + (z - self.bounds.min.z) * width) as usize];
        (low != i16::MAX).then_some((low as i32, self.floor_y - 1))
    }
}

/// The block a data marker leaves at world `w` of piece `p`, if one sits there.
fn marker_block(p: &Placed, w: IVec3) -> Option<Block> {
    let local = p.piece.local(w);
    template(p.piece.template).markers().iter().find(|m| m.1 == local).map(|m| m.0.block(p.piece.rotation))
}

/// The mobs a mansion's markers spawn, by kind and block.
type Residents = Vec<(MobKind, IVec3)>;

/// Chests, the spawner and fixed containers, and the mobs Java's data
/// markers spawn (`WoodlandMansionPiece.handleDataMarker`).
fn residents_and_features(seed: u64, start: &Start, pieces: &[Placed]) -> (Vec<(IVec3, Feature)>, Residents) {
    let mut features = Vec::new();
    let mut residents = Vec::new();
    let salt = seed ^ (start.chunk.x as u64).wrapping_mul(0x9E37_79B9) ^ (start.chunk.y as u64) << 32;
    for p in pieces {
        let t = template(p.piece.template);
        for &(marker, local) in t.markers() {
            let w = p.piece.world(local);
            match marker {
                Marker::Mage => residents.push((MobKind::Evoker, w)),
                Marker::Warrior => residents.push((MobKind::Vindicator, w)),
                // `level.getRandom().nextInt(3) + 1` allays: the world's
                // random, so a per-position roll stands in for it. Java
                // spawns them on one block; here each takes the next block
                // up the cell (residents are remembered by position).
                Marker::Allays => {
                    let n = 1 + hash3(w.x, w.y, w.z, salt ^ 0xA11A) % 3;
                    residents.extend((0..n as i32).map(|i| (MobKind::Allay, w + IVec3::Y * i)));
                }
                _ => features.push((
                    w,
                    Feature::TempleChest(hash3(w.x, w.y, w.z, salt ^ 0x3A45), super::temples::LootTable::Mansion),
                )),
            }
        }
        for (i, f) in FIXED.iter().enumerate() {
            if f.template != p.piece.name() {
                continue;
            }
            let w = p.piece.world(IVec3::from(f.pos));
            features.push((
                w,
                match f.contents {
                    Fixed::Spawner(kind) => Feature::Spawner(kind),
                    _ => Feature::MansionChest(i as u8),
                },
            ));
        }
    }
    (features, residents)
}
