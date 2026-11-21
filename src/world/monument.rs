//! v0.16 Track F: ocean monuments (GEN-05e), in generation 5.
//!
//! A port of Java 26.3's `OceanMonumentStructure` and `OceanMonumentPieces`:
//!
//! - placement is the `ocean_monuments` set (triangular spread, spacing
//!   32, separation 5, salt 10387313; `temples::Kind::Monument`);
//! - `findGenerationPoint`: every biome within 29 blocks of the start
//!   chunk's block (9, 9) at sea level, on the four-block biome grid, must
//!   be in `#required_ocean_monument_surrounding` (oceans and rivers), then
//!   the start chunk's centre on the ocean floor must be in
//!   `#has_structure/ocean_monument` (the deep oceans);
//! - `MonumentBuilding`: the 58 × 23 × 58 building at y 39, 29 blocks west
//!   and north of the start chunk, facing a random horizontal direction; its
//!   room graph (5 × 5 × 3 grid, the entry, the core room's eight cells,
//!   two closed openings per room where the source stays reachable) and the
//!   room fitters in Java's order, all from the start's
//!   `setLargeFeatureSeed` random, so a seed lays out the rooms Java's would;
//! - `postProcess` runs once per 16 × 16 Java chunk the building's box
//!   crosses, clipped to that chunk, with the chunk's own decoration random
//!   (`setDecorationSeed`, then `setFeatureSeed` with the monument's index in
//!   `surface_structures`): Java gates each wall, wing and arch on whether
//!   the chunk crosses it, and the simple rooms' centre pillars and the top
//!   rooms' wet sponges draw from that random, so both follow the chunk
//!   grid exactly as Java's do.
//!
//! Java's `postProcess` reads the world it writes into (water boxes keep
//! ice and water, `generateBoxOnFillOnly` writes only over water, the legs
//! fill down through air and fluid); here the reads see the monument's own
//! earlier writes over the base terrain (`Generator::column` heights, the
//! exact column where the sea could be frozen, `solid_below` for the legs).
//! The building is laid out once per start and painted like every other
//! temple. See `docs/verification/2026-11-21-v0.16-ocean-structures.md`.

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::biome::Biome;
use super::block::{Block, Facing};
use super::mansion::large_feature_random;
use super::overworld_blocks as ob;
use super::structure::Bounds;
use super::terrain::{Generator, SEA_LEVEL};
use super::worldgen_random::{SURFACE_STRUCTURES, WorldgenRandom};
use crate::entity::MobKind;

/// `OceanMonumentPiece.BASE_GRAY`, `BASE_LIGHT`, `BASE_BLACK`, `LAMP_BLOCK`.
const GRAY: Block = ob::PRISMARINE;
const LIGHT: Block = ob::PRISMARINE_BRICKS;
const BLACK: Block = ob::DARK_PRISMARINE;
const LAMP: Block = ob::SEA_LANTERN;

/// `MonumentBuilding`'s box: 58 wide and deep, 23 high, from y 39.
pub const WIDTH: i32 = 58;
pub const HEIGHT: i32 = 23;
pub const BASE_Y: i32 = 39;
/// `findGenerationPoint`'s biome check: block (9, 9) of the start chunk and
/// 29 blocks around it.
pub const BIOME_CHECK_OFFSET: i32 = 9;
pub const BIOME_CHECK_RADIUS: i32 = 29;
/// The monument's index in `surface_structures` (registry order), which
/// seeds its decoration random.
pub const FEATURE_INDEX: i32 = 24;
/// Farthest a monument's blocks reach from its start chunk's centre
/// horizontally (the building, its water skirt five blocks out).
pub const REACH: i32 = 29 + 5 + 8 + 1;

// `Direction.get3DDataValue()`.
const DOWN: usize = 0;
const UP: usize = 1;
const NORTH: usize = 2;
const SOUTH: usize = 3;
const WEST: usize = 4;
const EAST: usize = 5;
const OPPOSITE: [usize; 6] = [UP, DOWN, SOUTH, NORTH, EAST, WEST];
const STEP: [[i32; 3]; 6] = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];

/// `getRoomIndex`.
const fn room_index(x: i32, y: i32, z: i32) -> i32 {
    y * 25 + z * 5 + x
}
const SOURCE_INDEX: i32 = room_index(2, 0, 0);
const TOP_CONNECT_INDEX: i32 = room_index(2, 2, 0);
const LEFTWING_CONNECT_INDEX: i32 = room_index(0, 1, 0);
const RIGHTWING_CONNECT_INDEX: i32 = room_index(4, 1, 0);

/// `RoomDefinition`.
#[derive(Clone, Debug)]
pub struct Room {
    pub index: i32,
    conn: [Option<usize>; 6],
    open: [bool; 6],
    claimed: bool,
    is_source: bool,
    scan: i32,
}

impl Room {
    fn new(index: i32) -> Room {
        Room { index, conn: [None; 6], open: [false; 6], claimed: false, is_source: false, scan: 0 }
    }

    fn count_openings(&self) -> usize {
        self.open.iter().filter(|o| **o).count()
    }
}

struct Graph {
    rooms: Vec<Room>,
}

impl Graph {
    fn add(&mut self, index: i32) -> usize {
        self.rooms.push(Room::new(index));
        self.rooms.len() - 1
    }

    fn connect(&mut self, a: usize, d: usize, b: usize) {
        self.rooms[a].conn[d] = Some(b);
        self.rooms[b].conn[OPPOSITE[d]] = Some(a);
    }

    fn conn(&self, a: usize, d: usize) -> usize {
        self.rooms[a].conn[d].expect("a monument room connection")
    }

    fn update_openings(&mut self, a: usize) {
        let r = &mut self.rooms[a];
        for i in 0..6 {
            r.open[i] = r.conn[i].is_some();
        }
    }

    /// `findSource`.
    fn find_source(&mut self, a: usize, scan: i32) -> bool {
        if self.rooms[a].is_source {
            return true;
        }
        self.rooms[a].scan = scan;
        for i in 0..6 {
            if let Some(c) = self.rooms[a].conn[i]
                && self.rooms[a].open[i]
                && self.rooms[c].scan != scan
                && self.find_source(c, scan)
            {
                return true;
            }
        }
        false
    }
}

/// The kind of each child piece, with what its `postProcess` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Entry,
    Core,
    DoubleX,
    DoubleXY,
    DoubleY,
    DoubleYZ,
    DoubleZ,
    /// `mainDesign` 0–2.
    Simple(u8),
    SimpleTop,
    /// `mainDesign` 0–1.
    Wing(u8),
    Penthouse,
}

/// One child piece: its kind, room (for the room pieces) and box.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub kind: PieceKind,
    pub room: Option<usize>,
    pub frame: Frame,
}

/// A `StructurePiece` frame: its box and orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub min: IVec3,
    pub max: IVec3,
    pub orient: Facing,
}

impl Frame {
    /// `StructurePiece.makeBoundingBox(x, y, z, direction, width, height, depth)`.
    fn make(x: i32, y: i32, z: i32, orient: Facing, w: i32, h: i32, d: i32) -> Frame {
        let size =
            if matches!(orient, Facing::North | Facing::South) { IVec3::new(w, h, d) } else { IVec3::new(d, h, w) };
        let min = IVec3::new(x, y, z);
        Frame { min, max: min + size - IVec3::ONE, orient }
    }

    /// `getWorldX/Y/Z`.
    pub fn world(&self, x: i32, y: i32, z: i32) -> IVec3 {
        let wx = match self.orient {
            Facing::North | Facing::South => self.min.x + x,
            Facing::West => self.max.x - z,
            Facing::East => self.min.x + z,
        };
        let wz = match self.orient {
            Facing::North => self.max.z - z,
            Facing::South => self.min.z + z,
            Facing::West | Facing::East => self.min.z + x,
        };
        IVec3::new(wx, self.min.y + y, wz)
    }

    fn shifted(self, d: IVec3) -> Frame {
        Frame { min: self.min + d, max: self.max + d, ..self }
    }

    pub fn bounds(&self) -> Bounds {
        Bounds { min: self.min, max: self.max }
    }
}

/// `BoundingBox.fromCorners`.
fn corners(a: IVec3, b: IVec3, orient: Facing) -> Frame {
    Frame { min: a.min(b), max: a.max(b), orient }
}

/// A monument's pieces: the building frame, the room graph and the
/// children `MonumentBuilding` made.
pub struct Layout {
    pub building: Frame,
    rooms: Vec<Room>,
    pub pieces: Vec<Piece>,
    /// The start chunk.
    pub chunk: IVec2,
}

/// The biome rules of `findGenerationPoint` and `isValidBiome` (the centre
/// column is checked first only because it is the cheapest; every rule must
/// hold either way).
pub fn biomes_allow(g: &Generator, chunk: IVec2) -> bool {
    let (cx, cz) = (chunk.x * 16 + 8, chunk.y * 16 + 8);
    if !g.column(cx, cz).biome.is_deep_ocean() {
        return false;
    }
    let (x, z) = (chunk.x * 16 + BIOME_CHECK_OFFSET, chunk.y * 16 + BIOME_CHECK_OFFSET);
    // `BiomeSource.getBiomesWithin`: the quart grid from (x - r) >> 2 to
    // (x + r) >> 2 at sea level must hold oceans and rivers only.
    let (lo, hi) = (
        IVec2::new(x - BIOME_CHECK_RADIUS, z - BIOME_CHECK_RADIUS).div_euclid(IVec2::splat(4)),
        IVec2::new(x + BIOME_CHECK_RADIUS, z + BIOME_CHECK_RADIUS).div_euclid(IVec2::splat(4)),
    );
    for qz in lo.y..=hi.y {
        for qx in lo.x..=hi.x {
            let b = g.biome_at(IVec3::new(qx * 4, SEA_LEVEL, qz * 4));
            if !(b.is_ocean() || matches!(b, Biome::River | Biome::FrozenRiver)) {
                return false;
            }
        }
    }
    // `onTopOfChunkCenter(OCEAN_FLOOR_WG)`, then `isValidBiome` at the stub.
    let floor = g.structure_column(cx, cz).top(true);
    g.biome_at(IVec3::new(cx, floor, cz)).is_deep_ocean()
}

/// `OceanMonumentStructure.createTopPiece` and `MonumentBuilding`'s
/// constructor from the start's random.
pub fn layout(seed: u64, chunk: IVec2) -> Layout {
    let mut rng = large_feature_random(seed, chunk);
    let (west, north) = (chunk.x * 16 - 29, chunk.y * 16 - 29);
    // `Plane.HORIZONTAL.getRandomDirection`: north, east, south, west.
    let orient = [Facing::North, Facing::East, Facing::South, Facing::West][rng.next_bounded(4) as usize];
    let building = Frame::make(west, BASE_Y, north, orient, WIDTH, HEIGHT, WIDTH);

    // `generateRoomGraph`.
    let mut g = Graph { rooms: Vec::new() };
    let mut grid: [Option<usize>; 75] = [None; 75];
    for (y, xs, zs) in [(0, 0..5, 0..4), (1, 0..5, 0..4), (2, 1..4, 0..2)] {
        for x in xs {
            for z in zs.clone() {
                let i = room_index(x, y, z);
                grid[i as usize] = Some(g.add(i));
            }
        }
    }
    let source = grid[SOURCE_INDEX as usize].unwrap();
    for x in 0..5 {
        for z in 0..5 {
            for y in 0..3 {
                let Some(here) = grid[room_index(x, y, z) as usize] else { continue };
                for (d, s) in STEP.iter().enumerate() {
                    let (nx, ny, nz) = (x + s[0], y + s[1], z + s[2]);
                    if !(0..5).contains(&nx) || !(0..5).contains(&nz) || !(0..3).contains(&ny) {
                        continue;
                    }
                    if let Some(n) = grid[room_index(nx, ny, nz) as usize] {
                        let dir = if nz == z { d } else { OPPOSITE[d] };
                        g.connect(here, dir, n);
                    }
                }
            }
        }
    }
    let roof = g.add(1003);
    let left = g.add(1001);
    let right = g.add(1002);
    g.connect(grid[TOP_CONNECT_INDEX as usize].unwrap(), UP, roof);
    g.connect(grid[LEFTWING_CONNECT_INDEX as usize].unwrap(), SOUTH, left);
    g.connect(grid[RIGHTWING_CONNECT_INDEX as usize].unwrap(), SOUTH, right);
    for r in [roof, left, right] {
        g.rooms[r].claimed = true;
    }
    g.rooms[source].is_source = true;
    let core = grid[room_index(rng.next_bounded(4), 0, 2) as usize].unwrap();
    let (e, n, u) = (g.conn(core, EAST), g.conn(core, NORTH), g.conn(core, UP));
    let (en, eu, nu) = (g.conn(e, NORTH), g.conn(e, UP), g.conn(n, UP));
    let enu = g.conn(en, UP);
    for r in [core, e, n, en, u, eu, nu, enu] {
        g.rooms[r].claimed = true;
    }
    let mut defs: Vec<usize> = Vec::new();
    for r in grid.iter().flatten() {
        g.update_openings(*r);
        defs.push(*r);
    }
    g.update_openings(roof);
    // `Util.shuffle`.
    for i in (2..=defs.len()).rev() {
        let to = rng.next_bounded(i as i32) as usize;
        defs.swap(i - 1, to);
    }
    let mut scan = 1;
    for &def in &defs {
        let (mut closed, mut attempts) = (0, 0);
        while closed < 2 && attempts < 5 {
            attempts += 1;
            let f = rng.next_bounded(6) as usize;
            if !g.rooms[def].open[f] {
                continue;
            }
            let of = OPPOSITE[f];
            let other = g.conn(def, f);
            g.rooms[def].open[f] = false;
            g.rooms[other].open[of] = false;
            let a = g.find_source(def, scan);
            scan += 1;
            let ok = a && {
                let b = g.find_source(other, scan);
                scan += 1;
                b
            };
            if ok {
                closed += 1;
            } else {
                g.rooms[def].open[f] = true;
                g.rooms[other].open[of] = true;
            }
        }
    }
    defs.extend([roof, left, right]);

    // The constructor: the entry, the core and the fitted rooms.
    g.rooms[source].claimed = true;
    let mut pieces =
        vec![room_piece(&g, PieceKind::Entry, source, orient), room_piece(&g, PieceKind::Core, core, orient)];
    for &def in &defs {
        if g.rooms[def].claimed || g.rooms[def].index >= 75 {
            continue;
        }
        pieces.push(fit(&mut g, def, orient, &mut rng));
    }
    let offset = building.world(9, 0, 22);
    for p in &mut pieces {
        p.frame = p.frame.shifted(offset);
    }
    let left_wing = corners(building.world(1, 1, 1), building.world(23, 8, 21), orient);
    let right_wing = corners(building.world(34, 1, 1), building.world(56, 8, 21), orient);
    let penthouse = corners(building.world(22, 13, 22), building.world(35, 17, 35), orient);
    let wing = rng.next_int();
    pieces.push(Piece { kind: PieceKind::Wing((wing & 1) as u8), room: None, frame: left_wing });
    pieces.push(Piece { kind: PieceKind::Wing((wing.wrapping_add(1) & 1) as u8), room: None, frame: right_wing });
    pieces.push(Piece { kind: PieceKind::Penthouse, room: None, frame: penthouse });
    Layout { building, rooms: g.rooms, pieces, chunk }
}

/// `OceanMonumentPiece(type, genDepth, orientation, definition, w, h, d)`'s box.
fn room_piece(g: &Graph, kind: PieceKind, room: usize, orient: Facing) -> Piece {
    let (w, h, d) = match kind {
        PieceKind::Core => (2, 2, 2),
        PieceKind::DoubleX => (2, 1, 1),
        PieceKind::DoubleXY => (2, 2, 1),
        PieceKind::DoubleY => (1, 2, 1),
        PieceKind::DoubleYZ => (1, 2, 2),
        PieceKind::DoubleZ => (1, 1, 2),
        _ => (1, 1, 1),
    };
    let index = g.rooms[room].index;
    let (rx, rz, ry) = (index % 5, index / 5 % 5, index / 25);
    let f = Frame::make(0, 0, 0, orient, w * 8, h * 4, d * 8);
    let shift = match orient {
        Facing::North => IVec3::new(rx * 8, ry * 4, -(rz + d) * 8 + 1),
        Facing::South => IVec3::new(rx * 8, ry * 4, rz * 8),
        Facing::West => IVec3::new(-(rz + d) * 8 + 1, ry * 4, rx * 8),
        Facing::East => IVec3::new(rz * 8, ry * 4, rx * 8),
    };
    Piece { kind, room: Some(room), frame: f.shifted(shift) }
}

/// The `MonumentRoomFitter`s in Java's order: the first that fits claims
/// its rooms and makes the piece.
fn fit(g: &mut Graph, def: usize, orient: Facing, rng: &mut crate::enchant::JavaRandom) -> Piece {
    let open = |g: &Graph, r: usize, d: usize| g.rooms[r].open[d] && !g.rooms[g.conn(r, d)].claimed;
    if open(g, def, EAST) && open(g, def, UP) && open(g, g.conn(def, EAST), UP) {
        let (e, u) = (g.conn(def, EAST), g.conn(def, UP));
        let eu = g.conn(e, UP);
        for r in [def, e, u, eu] {
            g.rooms[r].claimed = true;
        }
        return room_piece(g, PieceKind::DoubleXY, def, orient);
    }
    if open(g, def, NORTH) && open(g, def, UP) && open(g, g.conn(def, NORTH), UP) {
        let (n, u) = (g.conn(def, NORTH), g.conn(def, UP));
        let nu = g.conn(n, UP);
        for r in [def, n, u, nu] {
            g.rooms[r].claimed = true;
        }
        return room_piece(g, PieceKind::DoubleYZ, def, orient);
    }
    if open(g, def, NORTH) {
        let n = g.conn(def, NORTH);
        g.rooms[def].claimed = true;
        g.rooms[n].claimed = true;
        return room_piece(g, PieceKind::DoubleZ, def, orient);
    }
    if open(g, def, EAST) {
        let e = g.conn(def, EAST);
        g.rooms[def].claimed = true;
        g.rooms[e].claimed = true;
        return room_piece(g, PieceKind::DoubleX, def, orient);
    }
    if open(g, def, UP) {
        let u = g.conn(def, UP);
        g.rooms[def].claimed = true;
        g.rooms[u].claimed = true;
        return room_piece(g, PieceKind::DoubleY, def, orient);
    }
    g.rooms[def].claimed = true;
    let o = g.rooms[def].open;
    if !o[WEST] && !o[EAST] && !o[NORTH] && !o[SOUTH] && !o[UP] {
        return room_piece(g, PieceKind::SimpleTop, def, orient);
    }
    let design = rng.next_bounded(3) as u8;
    room_piece(g, PieceKind::Simple(design), def, orient)
}

/// What a laid-out monument writes, and its elder guardians.
pub struct Built {
    /// The building's box (`StructureStart.getBoundingBox`, the spawn
    /// override's `full` box).
    pub bounds: Bounds,
    /// Every cell the monument writes, in its final state.
    pub blocks: Vec<(IVec3, Block)>,
    pub elders: Vec<IVec3>,
}

/// Runs `postProcess` in every Java chunk the building's box crosses.
pub fn build(g: &Generator, layout: &Layout) -> Built {
    let mut level =
        Level { g, cells: FxHashMap::default(), heights: FxHashMap::default(), frozen: FxHashMap::default() };
    let mut elders = Vec::new();
    let b = layout.building;
    for cz in b.min.z.div_euclid(16)..=b.max.z.div_euclid(16) {
        for cx in b.min.x.div_euclid(16)..=b.max.x.div_euclid(16) {
            let rng = WorldgenRandom::structure(g.seed, cx * 16, cz * 16, FEATURE_INDEX, SURFACE_STRUCTURES);
            let mut run = Run {
                level: &mut level,
                lo: IVec2::new(cx * 16, cz * 16),
                hi: IVec2::new(cx * 16 + 15, cz * 16 + 15),
                rng,
                elders: &mut elders,
                rooms: &layout.rooms,
            };
            run.building(layout);
        }
    }
    let mut blocks: Vec<(IVec3, Block)> = level.cells.into_iter().collect();
    blocks.sort_unstable_by_key(|(p, _)| (p.y, p.z, p.x));
    Built { bounds: b.bounds(), blocks, elders }
}

/// The world `postProcess` reads and writes: the monument's own writes over
/// the base terrain.
struct Level<'a> {
    g: &'a Generator,
    cells: FxHashMap<IVec3, Block>,
    heights: FxHashMap<IVec2, i32>,
    frozen: FxHashMap<IVec2, Option<Block>>,
}

impl Level<'_> {
    fn get(&mut self, p: IVec3) -> Block {
        if let Some(b) = self.cells.get(&p) {
            return *b;
        }
        let xz = IVec2::new(p.x, p.z);
        let g = self.g;
        let height = *self.heights.entry(xz).or_insert_with(|| g.column(p.x, p.z).height);
        if p.y <= height {
            return Block::STONE;
        }
        if p.y >= SEA_LEVEL {
            return Block::AIR;
        }
        if p.y == SEA_LEVEL - 1 {
            // Only frozen seas can hold ice at the surface: read the exact
            // base column there.
            let top = *self.frozen.entry(xz).or_insert_with(|| {
                let col = g.column(p.x, p.z);
                col.frozen.then(|| g.structure_column(p.x, p.z).get(SEA_LEVEL - 1))
            });
            if let Some(b) = top {
                return b;
            }
        }
        Block::WATER
    }
}

/// One Java chunk's `postProcess`.
struct Run<'a, 'b> {
    level: &'a mut Level<'b>,
    lo: IVec2,
    hi: IVec2,
    rng: WorldgenRandom,
    elders: &'a mut Vec<IVec3>,
    rooms: &'a [Room],
}

/// `FILL_KEEP`.
fn fill_keep(b: Block) -> bool {
    b == Block::WATER || b == Block::ICE || b == super::gadgets::PACKED_ICE || b == ob::BLUE_ICE
}

/// `isReplaceableByStructures`.
fn replaceable(b: Block) -> bool {
    b == Block::AIR
        || b.is_fluid()
        || b == super::cave_blocks::GLOW_LICHEN
        || b == ob::SEAGRASS
        || b == ob::TALL_SEAGRASS
        || b == ob::TALL_SEAGRASS_TOP
}

impl Run<'_, '_> {
    fn inside(&self, p: IVec3) -> bool {
        (self.lo.x..=self.hi.x).contains(&p.x) && (self.lo.y..=self.hi.y).contains(&p.z)
    }

    /// `chunkIntersects`.
    fn crosses(&self, f: &Frame, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let (a, b) = (f.world(x0, 0, z0), f.world(x1, 0, z1));
        let (min, max) = (a.min(b), a.max(b));
        min.x <= self.hi.x && max.x >= self.lo.x && min.z <= self.hi.y && max.z >= self.lo.y
    }

    fn crosses_box(&self, b: &Bounds) -> bool {
        b.min.x <= self.hi.x && b.max.x >= self.lo.x && b.min.z <= self.hi.y && b.max.z >= self.lo.y
    }

    /// `placeBlock`.
    fn place(&mut self, f: &Frame, b: Block, x: i32, y: i32, z: i32) {
        let p = f.world(x, y, z);
        if self.inside(p) {
            self.level.cells.insert(p, b);
        }
    }

    /// `getBlock`: air outside the chunk.
    fn get(&mut self, f: &Frame, x: i32, y: i32, z: i32) -> Block {
        let p = f.world(x, y, z);
        if self.inside(p) { self.level.get(p) } else { Block::AIR }
    }

    /// Whether the world box of local `[x0..x1] × [z0..z1]` misses the chunk.
    fn misses(&self, f: &Frame, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        x0 > x1 || z0 > z1 || !self.crosses(f, x0, z0, x1, z1)
    }

    /// `generateBox` with the same edge and fill block.
    fn bx(&mut self, f: &Frame, [x0, y0, z0, x1, y1, z1]: [i32; 6], b: Block) {
        if self.misses(f, x0, z0, x1, z1) {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    self.place(f, b, x, y, z);
                }
            }
        }
    }

    /// `generateWaterBox`.
    fn water(&mut self, f: &Frame, [x0, y0, z0, x1, y1, z1]: [i32; 6]) {
        if self.misses(f, x0, z0, x1, z1) {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    let b = self.get(f, x, y, z);
                    if fill_keep(b) {
                        continue;
                    }
                    let air = f.world(x, y, z).y >= SEA_LEVEL && b != Block::WATER;
                    self.place(f, if air { Block::AIR } else { Block::WATER }, x, y, z);
                }
            }
        }
    }

    /// `generateBoxOnFillOnly`.
    fn fill_only(&mut self, f: &Frame, [x0, y0, z0, x1, y1, z1]: [i32; 6], b: Block) {
        if self.misses(f, x0, z0, x1, z1) {
            return;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if self.get(f, x, y, z) == Block::WATER {
                        self.place(f, b, x, y, z);
                    }
                }
            }
        }
    }

    /// `fillColumnDown`.
    fn column_down(&mut self, f: &Frame, b: Block, x: i32, y: i32, z: i32) {
        let mut p = f.world(x, y, z);
        if !self.inside(p) {
            return;
        }
        let min_y = super::chunk::WORLD_MIN_Y;
        while p.y > min_y + 1 {
            let here = match self.level.cells.get(&p) {
                Some(b) => *b,
                // Below the sea floor the base column decides.
                None if p.y <= self.level.g.solid_below(p.x, p.z, p.y + 1) => Block::STONE,
                None => self.level.get(p),
            };
            if !replaceable(here) {
                break;
            }
            self.level.cells.insert(p, b);
            p.y -= 1;
        }
    }

    /// `generateDefaultFloor`.
    fn default_floor(&mut self, f: &Frame, xo: i32, zo: i32, down_opening: bool) {
        if down_opening {
            self.bx(f, [xo, 0, zo, xo + 2, 0, zo + 7], GRAY);
            self.bx(f, [xo + 5, 0, zo, xo + 7, 0, zo + 7], GRAY);
            self.bx(f, [xo + 3, 0, zo, xo + 4, 0, zo + 2], GRAY);
            self.bx(f, [xo + 3, 0, zo + 5, xo + 4, 0, zo + 7], GRAY);
            self.bx(f, [xo + 3, 0, zo + 2, xo + 4, 0, zo + 2], LIGHT);
            self.bx(f, [xo + 3, 0, zo + 5, xo + 4, 0, zo + 5], LIGHT);
            self.bx(f, [xo + 2, 0, zo + 3, xo + 2, 0, zo + 4], LIGHT);
            self.bx(f, [xo + 5, 0, zo + 3, xo + 5, 0, zo + 4], LIGHT);
        } else {
            self.bx(f, [xo, 0, zo, xo + 7, 0, zo + 7], GRAY);
        }
    }

    /// `spawnElder`.
    fn elder(&mut self, f: &Frame, x: i32, y: i32, z: i32) {
        let p = f.world(x, y, z);
        if self.inside(p) {
            self.elders.push(p);
        }
    }

    fn open(&self, r: usize, d: usize) -> bool {
        self.rooms[r].open[d]
    }

    fn conn(&self, r: usize, d: usize) -> Option<usize> {
        self.rooms[r].conn[d]
    }

    /// `MonumentBuilding.postProcess`.
    fn building(&mut self, layout: &Layout) {
        let f = &layout.building;
        if !self.crosses_box(&f.bounds()) {
            return;
        }
        let water_height = SEA_LEVEL.max(64) - f.min.y;
        self.water(f, [0, 0, 0, 58, water_height, 58]);
        self.wing(f, false, 0);
        self.wing(f, true, 33);
        self.entrance_archs(f);
        self.entrance_wall(f);
        self.roof_piece(f);
        self.lower_wall(f);
        self.middle_wall(f);
        self.upper_wall(f);
        for px in 0..7 {
            let mut pz = 0;
            while pz < 7 {
                if pz == 0 && px == 3 {
                    pz = 6;
                }
                let (bx, bz) = (px * 9, pz * 9);
                for w in 0..4 {
                    for d in 0..4 {
                        self.place(f, LIGHT, bx + w, 0, bz + d);
                        self.column_down(f, LIGHT, bx + w, -1, bz + d);
                    }
                }
                if px != 0 && px != 6 {
                    pz += 6;
                } else {
                    pz += 1;
                }
            }
        }
        for i in 0..5 {
            self.water(f, [-1 - i, i * 2, -1 - i, -1 - i, 23, 58 + i]);
            self.water(f, [58 + i, i * 2, -1 - i, 58 + i, 23, 58 + i]);
            self.water(f, [-i, i * 2, -1 - i, 57 + i, 23, -1 - i]);
            self.water(f, [-i, i * 2, 58 + i, 57 + i, 23, 58 + i]);
        }
        for p in &layout.pieces {
            if self.crosses_box(&p.frame.bounds()) {
                self.piece(p);
            }
        }
    }

    fn wing(&mut self, f: &Frame, flipped: bool, xo: i32) {
        if !self.crosses(f, xo, 0, xo + 23, 20) {
            return;
        }
        self.bx(f, [xo, 0, 0, xo + 24, 0, 20], GRAY);
        self.water(f, [xo, 1, 0, xo + 24, 10, 20]);
        for i in 0..4 {
            self.bx(f, [xo + i, i + 1, i, xo + i, i + 1, 20], LIGHT);
            self.bx(f, [xo + i + 7, i + 5, i + 7, xo + i + 7, i + 5, 20], LIGHT);
            self.bx(f, [xo + 17 - i, i + 5, i + 7, xo + 17 - i, i + 5, 20], LIGHT);
            self.bx(f, [xo + 24 - i, i + 1, i, xo + 24 - i, i + 1, 20], LIGHT);
            self.bx(f, [xo + i + 1, i + 1, i, xo + 23 - i, i + 1, i], LIGHT);
            self.bx(f, [xo + i + 8, i + 5, i + 7, xo + 16 - i, i + 5, i + 7], LIGHT);
        }
        self.bx(f, [xo + 4, 4, 4, xo + 6, 4, 20], GRAY);
        self.bx(f, [xo + 7, 4, 4, xo + 17, 4, 6], GRAY);
        self.bx(f, [xo + 18, 4, 4, xo + 20, 4, 20], GRAY);
        self.bx(f, [xo + 11, 8, 11, xo + 13, 8, 20], GRAY);
        self.place(f, LIGHT, xo + 12, 9, 12);
        self.place(f, LIGHT, xo + 12, 9, 15);
        self.place(f, LIGHT, xo + 12, 9, 18);
        let left = xo + if flipped { 19 } else { 5 };
        let right = xo + if flipped { 5 } else { 19 };
        for z in (5..=20).rev().step_by(3) {
            self.place(f, LIGHT, left, 5, z);
        }
        for z in (7..=19).rev().step_by(3) {
            self.place(f, LIGHT, right, 5, z);
        }
        for i in 0..4 {
            let pos = if flipped { xo + 24 - (17 - i * 3) } else { xo + 17 - i * 3 };
            self.place(f, LIGHT, pos, 5, 5);
        }
        self.place(f, LIGHT, right, 5, 5);
        self.bx(f, [xo + 11, 1, 12, xo + 13, 7, 12], GRAY);
        self.bx(f, [xo + 12, 1, 11, xo + 12, 7, 13], GRAY);
    }

    fn entrance_archs(&mut self, f: &Frame) {
        if !self.crosses(f, 22, 5, 35, 17) {
            return;
        }
        self.water(f, [25, 0, 0, 32, 8, 20]);
        for i in 0..4 {
            let z = 5 + i * 4;
            self.bx(f, [24, 2, z, 24, 4, z], LIGHT);
            self.bx(f, [22, 4, z, 23, 4, z], LIGHT);
            self.place(f, LIGHT, 25, 5, z);
            self.place(f, LIGHT, 26, 6, z);
            self.place(f, LAMP, 26, 5, z);
            self.bx(f, [33, 2, z, 33, 4, z], LIGHT);
            self.bx(f, [34, 4, z, 35, 4, z], LIGHT);
            self.place(f, LIGHT, 32, 5, z);
            self.place(f, LIGHT, 31, 6, z);
            self.place(f, LAMP, 31, 5, z);
            self.bx(f, [27, 6, z, 30, 6, z], GRAY);
        }
    }

    fn entrance_wall(&mut self, f: &Frame) {
        if !self.crosses(f, 15, 20, 42, 21) {
            return;
        }
        self.bx(f, [15, 0, 21, 42, 0, 21], GRAY);
        self.water(f, [26, 1, 21, 31, 3, 21]);
        self.bx(f, [21, 12, 21, 36, 12, 21], GRAY);
        self.bx(f, [17, 11, 21, 40, 11, 21], GRAY);
        self.bx(f, [16, 10, 21, 41, 10, 21], GRAY);
        self.bx(f, [15, 7, 21, 42, 9, 21], GRAY);
        self.bx(f, [16, 6, 21, 41, 6, 21], GRAY);
        self.bx(f, [17, 5, 21, 40, 5, 21], GRAY);
        self.bx(f, [21, 4, 21, 36, 4, 21], GRAY);
        self.bx(f, [22, 3, 21, 26, 3, 21], GRAY);
        self.bx(f, [31, 3, 21, 35, 3, 21], GRAY);
        self.bx(f, [23, 2, 21, 25, 2, 21], GRAY);
        self.bx(f, [32, 2, 21, 34, 2, 21], GRAY);
        self.bx(f, [28, 4, 20, 29, 4, 21], LIGHT);
        self.place(f, LIGHT, 27, 3, 21);
        self.place(f, LIGHT, 30, 3, 21);
        self.place(f, LIGHT, 26, 2, 21);
        self.place(f, LIGHT, 31, 2, 21);
        self.place(f, LIGHT, 25, 1, 21);
        self.place(f, LIGHT, 32, 1, 21);
        for i in 0..7 {
            self.place(f, BLACK, 28 - i, 6 + i, 21);
            self.place(f, BLACK, 29 + i, 6 + i, 21);
        }
        for i in 0..4 {
            self.place(f, BLACK, 28 - i, 9 + i, 21);
            self.place(f, BLACK, 29 + i, 9 + i, 21);
        }
        self.place(f, BLACK, 28, 12, 21);
        self.place(f, BLACK, 29, 12, 21);
        for i in 0..3 {
            self.place(f, BLACK, 22 - i * 2, 8, 21);
            self.place(f, BLACK, 22 - i * 2, 9, 21);
            self.place(f, BLACK, 35 + i * 2, 8, 21);
            self.place(f, BLACK, 35 + i * 2, 9, 21);
        }
        self.water(f, [15, 13, 21, 42, 15, 21]);
        self.water(f, [15, 1, 21, 15, 6, 21]);
        self.water(f, [16, 1, 21, 16, 5, 21]);
        self.water(f, [17, 1, 21, 20, 4, 21]);
        self.water(f, [21, 1, 21, 21, 3, 21]);
        self.water(f, [22, 1, 21, 22, 2, 21]);
        self.water(f, [23, 1, 21, 24, 1, 21]);
        self.water(f, [42, 1, 21, 42, 6, 21]);
        self.water(f, [41, 1, 21, 41, 5, 21]);
        self.water(f, [37, 1, 21, 40, 4, 21]);
        self.water(f, [36, 1, 21, 36, 3, 21]);
        self.water(f, [33, 1, 21, 34, 1, 21]);
        self.water(f, [35, 1, 21, 35, 2, 21]);
    }

    fn roof_piece(&mut self, f: &Frame) {
        if !self.crosses(f, 21, 21, 36, 36) {
            return;
        }
        self.bx(f, [21, 0, 22, 36, 0, 36], GRAY);
        self.water(f, [21, 1, 22, 36, 23, 36]);
        for i in 0..4 {
            self.bx(f, [21 + i, 13 + i, 21 + i, 36 - i, 13 + i, 21 + i], LIGHT);
            self.bx(f, [21 + i, 13 + i, 36 - i, 36 - i, 13 + i, 36 - i], LIGHT);
            self.bx(f, [21 + i, 13 + i, 22 + i, 21 + i, 13 + i, 35 - i], LIGHT);
            self.bx(f, [36 - i, 13 + i, 22 + i, 36 - i, 13 + i, 35 - i], LIGHT);
        }
        self.bx(f, [25, 16, 25, 32, 16, 32], GRAY);
        self.bx(f, [25, 17, 25, 25, 19, 25], LIGHT);
        self.bx(f, [32, 17, 25, 32, 19, 25], LIGHT);
        self.bx(f, [25, 17, 32, 25, 19, 32], LIGHT);
        self.bx(f, [32, 17, 32, 32, 19, 32], LIGHT);
        self.place(f, LIGHT, 26, 20, 26);
        self.place(f, LIGHT, 27, 21, 27);
        self.place(f, LAMP, 27, 20, 27);
        self.place(f, LIGHT, 26, 20, 31);
        self.place(f, LIGHT, 27, 21, 30);
        self.place(f, LAMP, 27, 20, 30);
        self.place(f, LIGHT, 31, 20, 31);
        self.place(f, LIGHT, 30, 21, 30);
        self.place(f, LAMP, 30, 20, 30);
        self.place(f, LIGHT, 31, 20, 26);
        self.place(f, LIGHT, 30, 21, 27);
        self.place(f, LAMP, 30, 20, 27);
        self.bx(f, [28, 21, 27, 29, 21, 27], GRAY);
        self.bx(f, [27, 21, 28, 27, 21, 29], GRAY);
        self.bx(f, [28, 21, 30, 29, 21, 30], GRAY);
        self.bx(f, [30, 21, 28, 30, 21, 29], GRAY);
    }

    fn lower_wall(&mut self, f: &Frame) {
        if self.crosses(f, 0, 21, 6, 58) {
            self.bx(f, [0, 0, 21, 6, 0, 57], GRAY);
            self.water(f, [0, 1, 21, 6, 7, 57]);
            self.bx(f, [4, 4, 21, 6, 4, 53], GRAY);
            for i in 0..4 {
                self.bx(f, [i, i + 1, 21, i, i + 1, 57 - i], LIGHT);
            }
            for z in (23..53).step_by(3) {
                self.place(f, LIGHT, 5, 5, z);
            }
            self.place(f, LIGHT, 5, 5, 52);
            for i in 0..4 {
                self.bx(f, [i, i + 1, 21, i, i + 1, 57 - i], LIGHT);
            }
            self.bx(f, [4, 1, 52, 6, 3, 52], GRAY);
            self.bx(f, [5, 1, 51, 5, 3, 53], GRAY);
        }
        if self.crosses(f, 51, 21, 58, 58) {
            self.bx(f, [51, 0, 21, 57, 0, 57], GRAY);
            self.water(f, [51, 1, 21, 57, 7, 57]);
            self.bx(f, [51, 4, 21, 53, 4, 53], GRAY);
            for i in 0..4 {
                self.bx(f, [57 - i, i + 1, 21, 57 - i, i + 1, 57 - i], LIGHT);
            }
            for z in (23..53).step_by(3) {
                self.place(f, LIGHT, 52, 5, z);
            }
            self.place(f, LIGHT, 52, 5, 52);
            self.bx(f, [51, 1, 52, 53, 3, 52], GRAY);
            self.bx(f, [52, 1, 51, 52, 3, 53], GRAY);
        }
        if self.crosses(f, 0, 51, 57, 57) {
            self.bx(f, [7, 0, 51, 50, 0, 57], GRAY);
            self.water(f, [7, 1, 51, 50, 10, 57]);
            for i in 0..4 {
                self.bx(f, [i + 1, i + 1, 57 - i, 56 - i, i + 1, 57 - i], LIGHT);
            }
        }
    }

    fn middle_wall(&mut self, f: &Frame) {
        if self.crosses(f, 7, 21, 13, 50) {
            self.bx(f, [7, 0, 21, 13, 0, 50], GRAY);
            self.water(f, [7, 1, 21, 13, 10, 50]);
            self.bx(f, [11, 8, 21, 13, 8, 53], GRAY);
            for i in 0..4 {
                self.bx(f, [i + 7, i + 5, 21, i + 7, i + 5, 54], LIGHT);
            }
            for z in (21..=45).step_by(3) {
                self.place(f, LIGHT, 12, 9, z);
            }
        }
        if self.crosses(f, 44, 21, 50, 54) {
            self.bx(f, [44, 0, 21, 50, 0, 50], GRAY);
            self.water(f, [44, 1, 21, 50, 10, 50]);
            self.bx(f, [44, 8, 21, 46, 8, 53], GRAY);
            for i in 0..4 {
                self.bx(f, [50 - i, i + 5, 21, 50 - i, i + 5, 54], LIGHT);
            }
            for z in (21..=45).step_by(3) {
                self.place(f, LIGHT, 45, 9, z);
            }
        }
        if self.crosses(f, 8, 44, 49, 54) {
            self.bx(f, [14, 0, 44, 43, 0, 50], GRAY);
            self.water(f, [14, 1, 44, 43, 10, 50]);
            for x in (12..=45).step_by(3) {
                self.place(f, LIGHT, x, 9, 45);
                self.place(f, LIGHT, x, 9, 52);
                if matches!(x, 12 | 18 | 24 | 33 | 39 | 45) {
                    self.place(f, LIGHT, x, 9, 47);
                    self.place(f, LIGHT, x, 9, 50);
                    self.place(f, LIGHT, x, 10, 45);
                    self.place(f, LIGHT, x, 10, 46);
                    self.place(f, LIGHT, x, 10, 51);
                    self.place(f, LIGHT, x, 10, 52);
                    self.place(f, LIGHT, x, 11, 47);
                    self.place(f, LIGHT, x, 11, 50);
                    self.place(f, LIGHT, x, 12, 48);
                    self.place(f, LIGHT, x, 12, 49);
                }
            }
            for i in 0..3 {
                self.bx(f, [8 + i, 5 + i, 54, 49 - i, 5 + i, 54], GRAY);
            }
            self.bx(f, [11, 8, 54, 46, 8, 54], LIGHT);
            self.bx(f, [14, 8, 44, 43, 8, 53], GRAY);
        }
    }

    fn upper_wall(&mut self, f: &Frame) {
        if self.crosses(f, 14, 21, 20, 43) {
            self.bx(f, [14, 0, 21, 20, 0, 43], GRAY);
            self.water(f, [14, 1, 22, 20, 14, 43]);
            self.bx(f, [18, 12, 22, 20, 12, 39], GRAY);
            self.bx(f, [18, 12, 21, 20, 12, 21], LIGHT);
            for i in 0..4 {
                self.bx(f, [i + 14, i + 9, 21, i + 14, i + 9, 43 - i], LIGHT);
            }
            for z in (23..=39).step_by(3) {
                self.place(f, LIGHT, 19, 13, z);
            }
        }
        if self.crosses(f, 37, 21, 43, 43) {
            self.bx(f, [37, 0, 21, 43, 0, 43], GRAY);
            self.water(f, [37, 1, 22, 43, 14, 43]);
            self.bx(f, [37, 12, 22, 39, 12, 39], GRAY);
            self.bx(f, [37, 12, 21, 39, 12, 21], LIGHT);
            for i in 0..4 {
                self.bx(f, [43 - i, i + 9, 21, 43 - i, i + 9, 43 - i], LIGHT);
            }
            for z in (23..=39).step_by(3) {
                self.place(f, LIGHT, 38, 13, z);
            }
        }
        if self.crosses(f, 15, 37, 42, 43) {
            self.bx(f, [21, 0, 37, 36, 0, 43], GRAY);
            self.water(f, [21, 1, 37, 36, 14, 43]);
            self.bx(f, [21, 12, 37, 36, 12, 39], GRAY);
            for i in 0..4 {
                self.bx(f, [15 + i, i + 9, 43 - i, 42 - i, i + 9, 43 - i], LIGHT);
            }
            for x in (21..=36).step_by(3) {
                self.place(f, LIGHT, x, 13, 38);
            }
        }
    }

    fn piece(&mut self, p: &Piece) {
        let f = &p.frame;
        match p.kind {
            PieceKind::Entry => self.entry(f, p.room.unwrap()),
            PieceKind::Core => self.core(f),
            PieceKind::DoubleX => self.double_x(f, p.room.unwrap()),
            PieceKind::DoubleXY => self.double_xy(f, p.room.unwrap()),
            PieceKind::DoubleY => self.double_y(f, p.room.unwrap()),
            PieceKind::DoubleYZ => self.double_yz(f, p.room.unwrap()),
            PieceKind::DoubleZ => self.double_z(f, p.room.unwrap()),
            PieceKind::Simple(design) => self.simple(f, p.room.unwrap(), design),
            PieceKind::SimpleTop => self.simple_top(f, p.room.unwrap()),
            PieceKind::Wing(design) => self.wing_room(f, design),
            PieceKind::Penthouse => self.penthouse(f),
        }
    }

    fn core(&mut self, f: &Frame) {
        self.fill_only(f, [1, 8, 0, 14, 8, 14], GRAY);
        self.bx(f, [0, 7, 0, 0, 7, 15], LIGHT);
        self.bx(f, [15, 7, 0, 15, 7, 15], LIGHT);
        self.bx(f, [1, 7, 0, 15, 7, 0], LIGHT);
        self.bx(f, [1, 7, 15, 14, 7, 15], LIGHT);
        for y in 1..=6 {
            let b = if y == 2 || y == 6 { GRAY } else { LIGHT };
            for x in [0, 15] {
                self.bx(f, [x, y, 0, x, y, 1], b);
                self.bx(f, [x, y, 6, x, y, 9], b);
                self.bx(f, [x, y, 14, x, y, 15], b);
            }
            self.bx(f, [1, y, 0, 1, y, 0], b);
            self.bx(f, [6, y, 0, 9, y, 0], b);
            self.bx(f, [14, y, 0, 14, y, 0], b);
            self.bx(f, [1, y, 15, 14, y, 15], b);
        }
        self.bx(f, [6, 3, 6, 9, 6, 9], BLACK);
        self.bx(f, [7, 4, 7, 8, 5, 8], Block::GOLD_BLOCK);
        for y in [3, 6] {
            for x in [6, 9] {
                self.place(f, LAMP, x, y, 6);
                self.place(f, LAMP, x, y, 9);
            }
        }
        for b in [
            [5, 1, 6, 5, 2, 6],
            [5, 1, 9, 5, 2, 9],
            [10, 1, 6, 10, 2, 6],
            [10, 1, 9, 10, 2, 9],
            [6, 1, 5, 6, 2, 5],
            [9, 1, 5, 9, 2, 5],
            [6, 1, 10, 6, 2, 10],
            [9, 1, 10, 9, 2, 10],
            [5, 2, 5, 5, 6, 5],
            [5, 2, 10, 5, 6, 10],
            [10, 2, 5, 10, 6, 5],
            [10, 2, 10, 10, 6, 10],
            [5, 7, 1, 5, 7, 6],
            [10, 7, 1, 10, 7, 6],
            [5, 7, 9, 5, 7, 14],
            [10, 7, 9, 10, 7, 14],
            [1, 7, 5, 6, 7, 5],
            [1, 7, 10, 6, 7, 10],
            [9, 7, 5, 14, 7, 5],
            [9, 7, 10, 14, 7, 10],
            [2, 1, 2, 2, 1, 3],
            [3, 1, 2, 3, 1, 2],
            [13, 1, 2, 13, 1, 3],
            [12, 1, 2, 12, 1, 2],
            [2, 1, 12, 2, 1, 13],
            [3, 1, 13, 3, 1, 13],
            [13, 1, 12, 13, 1, 13],
            [12, 1, 13, 12, 1, 13],
        ] {
            self.bx(f, b, LIGHT);
        }
    }

    fn entry(&mut self, f: &Frame, r: usize) {
        self.bx(f, [0, 3, 0, 2, 3, 7], LIGHT);
        self.bx(f, [5, 3, 0, 7, 3, 7], LIGHT);
        self.bx(f, [0, 2, 0, 1, 2, 7], LIGHT);
        self.bx(f, [6, 2, 0, 7, 2, 7], LIGHT);
        self.bx(f, [0, 1, 0, 0, 1, 7], LIGHT);
        self.bx(f, [7, 1, 0, 7, 1, 7], LIGHT);
        self.bx(f, [0, 1, 7, 7, 3, 7], LIGHT);
        self.bx(f, [1, 1, 0, 2, 3, 0], LIGHT);
        self.bx(f, [5, 1, 0, 6, 3, 0], LIGHT);
        if self.open(r, NORTH) {
            self.water(f, [3, 1, 7, 4, 2, 7]);
        }
        if self.open(r, WEST) {
            self.water(f, [0, 1, 3, 1, 2, 4]);
        }
        if self.open(r, EAST) {
            self.water(f, [6, 1, 3, 7, 2, 4]);
        }
    }

    fn double_x(&mut self, f: &Frame, west: usize) {
        let east = self.conn(west, EAST).unwrap();
        if self.rooms[west].index / 25 > 0 {
            self.default_floor(f, 8, 0, self.open(east, DOWN));
            self.default_floor(f, 0, 0, self.open(west, DOWN));
        }
        if self.conn(west, UP).is_none() {
            self.fill_only(f, [1, 4, 1, 7, 4, 6], GRAY);
        }
        if self.conn(east, UP).is_none() {
            self.fill_only(f, [8, 4, 1, 14, 4, 6], GRAY);
        }
        self.bx(f, [0, 3, 0, 0, 3, 7], LIGHT);
        self.bx(f, [15, 3, 0, 15, 3, 7], LIGHT);
        self.bx(f, [1, 3, 0, 15, 3, 0], LIGHT);
        self.bx(f, [1, 3, 7, 14, 3, 7], LIGHT);
        self.bx(f, [0, 2, 0, 0, 2, 7], GRAY);
        self.bx(f, [15, 2, 0, 15, 2, 7], GRAY);
        self.bx(f, [1, 2, 0, 15, 2, 0], GRAY);
        self.bx(f, [1, 2, 7, 14, 2, 7], GRAY);
        self.bx(f, [0, 1, 0, 0, 1, 7], LIGHT);
        self.bx(f, [15, 1, 0, 15, 1, 7], LIGHT);
        self.bx(f, [1, 1, 0, 15, 1, 0], LIGHT);
        self.bx(f, [1, 1, 7, 14, 1, 7], LIGHT);
        self.bx(f, [5, 1, 0, 10, 1, 4], LIGHT);
        self.bx(f, [6, 2, 0, 9, 2, 3], GRAY);
        self.bx(f, [5, 3, 0, 10, 3, 4], LIGHT);
        self.place(f, LAMP, 6, 2, 3);
        self.place(f, LAMP, 9, 2, 3);
        self.doors_x(f, west, east, 1);
    }

    /// The doorways of a two-cell-wide room's level at `y0` (1 or 5).
    fn doors_x(&mut self, f: &Frame, west: usize, east: usize, y0: i32) {
        let y1 = y0 + 1;
        if self.open(west, SOUTH) {
            self.water(f, [3, y0, 0, 4, y1, 0]);
        }
        if self.open(west, NORTH) {
            self.water(f, [3, y0, 7, 4, y1, 7]);
        }
        if self.open(west, WEST) {
            self.water(f, [0, y0, 3, 0, y1, 4]);
        }
        if self.open(east, SOUTH) {
            self.water(f, [11, y0, 0, 12, y1, 0]);
        }
        if self.open(east, NORTH) {
            self.water(f, [11, y0, 7, 12, y1, 7]);
        }
        if self.open(east, EAST) {
            self.water(f, [15, y0, 3, 15, y1, 4]);
        }
    }

    fn double_xy(&mut self, f: &Frame, west: usize) {
        let east = self.conn(west, EAST).unwrap();
        let west_up = self.conn(west, UP).unwrap();
        let east_up = self.conn(east, UP).unwrap();
        if self.rooms[west].index / 25 > 0 {
            self.default_floor(f, 8, 0, self.open(east, DOWN));
            self.default_floor(f, 0, 0, self.open(west, DOWN));
        }
        if self.conn(west_up, UP).is_none() {
            self.fill_only(f, [1, 8, 1, 7, 8, 6], GRAY);
        }
        if self.conn(east_up, UP).is_none() {
            self.fill_only(f, [8, 8, 1, 14, 8, 6], GRAY);
        }
        for y in 1..=7 {
            let b = if y == 2 || y == 6 { GRAY } else { LIGHT };
            self.bx(f, [0, y, 0, 0, y, 7], b);
            self.bx(f, [15, y, 0, 15, y, 7], b);
            self.bx(f, [1, y, 0, 15, y, 0], b);
            self.bx(f, [1, y, 7, 14, y, 7], b);
        }
        for b in [
            [2, 1, 3, 2, 7, 4],
            [3, 1, 2, 4, 7, 2],
            [3, 1, 5, 4, 7, 5],
            [13, 1, 3, 13, 7, 4],
            [11, 1, 2, 12, 7, 2],
            [11, 1, 5, 12, 7, 5],
            [5, 1, 3, 5, 3, 4],
            [10, 1, 3, 10, 3, 4],
            [5, 7, 2, 10, 7, 5],
            [5, 5, 2, 5, 7, 2],
            [10, 5, 2, 10, 7, 2],
            [5, 5, 5, 5, 7, 5],
            [10, 5, 5, 10, 7, 5],
        ] {
            self.bx(f, b, LIGHT);
        }
        self.place(f, LIGHT, 6, 6, 2);
        self.place(f, LIGHT, 9, 6, 2);
        self.place(f, LIGHT, 6, 6, 5);
        self.place(f, LIGHT, 9, 6, 5);
        self.bx(f, [5, 4, 3, 6, 4, 4], LIGHT);
        self.bx(f, [9, 4, 3, 10, 4, 4], LIGHT);
        self.place(f, LAMP, 5, 4, 2);
        self.place(f, LAMP, 5, 4, 5);
        self.place(f, LAMP, 10, 4, 2);
        self.place(f, LAMP, 10, 4, 5);
        self.doors_x(f, west, east, 1);
        self.doors_x(f, west_up, east_up, 5);
    }

    fn double_y(&mut self, f: &Frame, r: usize) {
        if self.rooms[r].index / 25 > 0 {
            self.default_floor(f, 0, 0, self.open(r, DOWN));
        }
        let above = self.conn(r, UP).unwrap();
        if self.conn(above, UP).is_none() {
            self.fill_only(f, [1, 8, 1, 6, 8, 6], GRAY);
        }
        for b in [
            [0, 4, 0, 0, 4, 7],
            [7, 4, 0, 7, 4, 7],
            [1, 4, 0, 6, 4, 0],
            [1, 4, 7, 6, 4, 7],
            [2, 4, 1, 2, 4, 2],
            [1, 4, 2, 1, 4, 2],
            [5, 4, 1, 5, 4, 2],
            [6, 4, 2, 6, 4, 2],
            [2, 4, 5, 2, 4, 6],
            [1, 4, 5, 1, 4, 5],
            [5, 4, 5, 5, 4, 6],
            [6, 4, 5, 6, 4, 5],
        ] {
            self.bx(f, b, LIGHT);
        }
        let mut def = r;
        for y in [1, 5] {
            if self.open(def, SOUTH) {
                self.bx(f, [2, y, 0, 2, y + 2, 0], LIGHT);
                self.bx(f, [5, y, 0, 5, y + 2, 0], LIGHT);
                self.bx(f, [3, y + 2, 0, 4, y + 2, 0], LIGHT);
            } else {
                self.bx(f, [0, y, 0, 7, y + 2, 0], LIGHT);
                self.bx(f, [0, y + 1, 0, 7, y + 1, 0], GRAY);
            }
            if self.open(def, NORTH) {
                self.bx(f, [2, y, 7, 2, y + 2, 7], LIGHT);
                self.bx(f, [5, y, 7, 5, y + 2, 7], LIGHT);
                self.bx(f, [3, y + 2, 7, 4, y + 2, 7], LIGHT);
            } else {
                self.bx(f, [0, y, 7, 7, y + 2, 7], LIGHT);
                self.bx(f, [0, y + 1, 7, 7, y + 1, 7], GRAY);
            }
            if self.open(def, WEST) {
                self.bx(f, [0, y, 2, 0, y + 2, 2], LIGHT);
                self.bx(f, [0, y, 5, 0, y + 2, 5], LIGHT);
                self.bx(f, [0, y + 2, 3, 0, y + 2, 4], LIGHT);
            } else {
                self.bx(f, [0, y, 0, 0, y + 2, 7], LIGHT);
                self.bx(f, [0, y + 1, 0, 0, y + 1, 7], GRAY);
            }
            if self.open(def, EAST) {
                self.bx(f, [7, y, 2, 7, y + 2, 2], LIGHT);
                self.bx(f, [7, y, 5, 7, y + 2, 5], LIGHT);
                self.bx(f, [7, y + 2, 3, 7, y + 2, 4], LIGHT);
            } else {
                self.bx(f, [7, y, 0, 7, y + 2, 7], LIGHT);
                self.bx(f, [7, y + 1, 0, 7, y + 1, 7], GRAY);
            }
            def = above;
        }
    }

    fn double_yz(&mut self, f: &Frame, south: usize) {
        let north = self.conn(south, NORTH).unwrap();
        let north_up = self.conn(north, UP).unwrap();
        let south_up = self.conn(south, UP).unwrap();
        if self.rooms[south].index / 25 > 0 {
            self.default_floor(f, 0, 8, self.open(north, DOWN));
            self.default_floor(f, 0, 0, self.open(south, DOWN));
        }
        if self.conn(south_up, UP).is_none() {
            self.fill_only(f, [1, 8, 1, 6, 8, 7], GRAY);
        }
        if self.conn(north_up, UP).is_none() {
            self.fill_only(f, [1, 8, 8, 6, 8, 14], GRAY);
        }
        for y in 1..=7 {
            let b = if y == 2 || y == 6 { GRAY } else { LIGHT };
            self.bx(f, [0, y, 0, 0, y, 15], b);
            self.bx(f, [7, y, 0, 7, y, 15], b);
            self.bx(f, [1, y, 0, 6, y, 0], b);
            self.bx(f, [1, y, 15, 6, y, 15], b);
        }
        for y in 1..=7 {
            let b = if y == 2 || y == 6 { LAMP } else { BLACK };
            self.bx(f, [3, y, 7, 4, y, 8], b);
        }
        if self.open(south, SOUTH) {
            self.water(f, [3, 1, 0, 4, 2, 0]);
        }
        if self.open(south, EAST) {
            self.water(f, [7, 1, 3, 7, 2, 4]);
        }
        if self.open(south, WEST) {
            self.water(f, [0, 1, 3, 0, 2, 4]);
        }
        if self.open(north, NORTH) {
            self.water(f, [3, 1, 15, 4, 2, 15]);
        }
        if self.open(north, WEST) {
            self.water(f, [0, 1, 11, 0, 2, 12]);
        }
        if self.open(north, EAST) {
            self.water(f, [7, 1, 11, 7, 2, 12]);
        }
        if self.open(south_up, SOUTH) {
            self.water(f, [3, 5, 0, 4, 6, 0]);
        }
        if self.open(south_up, EAST) {
            self.water(f, [7, 5, 3, 7, 6, 4]);
            self.bx(f, [5, 4, 2, 6, 4, 5], LIGHT);
            self.bx(f, [6, 1, 2, 6, 3, 2], LIGHT);
            self.bx(f, [6, 1, 5, 6, 3, 5], LIGHT);
        }
        if self.open(south_up, WEST) {
            self.water(f, [0, 5, 3, 0, 6, 4]);
            self.bx(f, [1, 4, 2, 2, 4, 5], LIGHT);
            self.bx(f, [1, 1, 2, 1, 3, 2], LIGHT);
            self.bx(f, [1, 1, 5, 1, 3, 5], LIGHT);
        }
        if self.open(north_up, NORTH) {
            self.water(f, [3, 5, 15, 4, 6, 15]);
        }
        if self.open(north_up, WEST) {
            self.water(f, [0, 5, 11, 0, 6, 12]);
            self.bx(f, [1, 4, 10, 2, 4, 13], LIGHT);
            self.bx(f, [1, 1, 10, 1, 3, 10], LIGHT);
            self.bx(f, [1, 1, 13, 1, 3, 13], LIGHT);
        }
        if self.open(north_up, EAST) {
            self.water(f, [7, 5, 11, 7, 6, 12]);
            self.bx(f, [5, 4, 10, 6, 4, 13], LIGHT);
            self.bx(f, [6, 1, 10, 6, 3, 10], LIGHT);
            self.bx(f, [6, 1, 13, 6, 3, 13], LIGHT);
        }
    }

    fn double_z(&mut self, f: &Frame, south: usize) {
        let north = self.conn(south, NORTH).unwrap();
        if self.rooms[south].index / 25 > 0 {
            self.default_floor(f, 0, 8, self.open(north, DOWN));
            self.default_floor(f, 0, 0, self.open(south, DOWN));
        }
        if self.conn(south, UP).is_none() {
            self.fill_only(f, [1, 4, 1, 6, 4, 7], GRAY);
        }
        if self.conn(north, UP).is_none() {
            self.fill_only(f, [1, 4, 8, 6, 4, 14], GRAY);
        }
        for (y, b) in [(3, LIGHT), (2, GRAY), (1, LIGHT)] {
            self.bx(f, [0, y, 0, 0, y, 15], b);
            self.bx(f, [7, y, 0, 7, y, 15], b);
            self.bx(f, [1, y, 0, 7, y, 0], b);
            self.bx(f, [1, y, 15, 6, y, 15], b);
        }
        for b in [
            [1, 1, 1, 1, 1, 2],
            [6, 1, 1, 6, 1, 2],
            [1, 3, 1, 1, 3, 2],
            [6, 3, 1, 6, 3, 2],
            [1, 1, 13, 1, 1, 14],
            [6, 1, 13, 6, 1, 14],
            [1, 3, 13, 1, 3, 14],
            [6, 3, 13, 6, 3, 14],
            [2, 1, 6, 2, 3, 6],
            [5, 1, 6, 5, 3, 6],
            [2, 1, 9, 2, 3, 9],
            [5, 1, 9, 5, 3, 9],
            [3, 2, 6, 4, 2, 6],
            [3, 2, 9, 4, 2, 9],
            [2, 2, 7, 2, 2, 8],
            [5, 2, 7, 5, 2, 8],
        ] {
            self.bx(f, b, LIGHT);
        }
        self.place(f, LAMP, 2, 2, 5);
        self.place(f, LAMP, 5, 2, 5);
        self.place(f, LAMP, 2, 2, 10);
        self.place(f, LAMP, 5, 2, 10);
        self.place(f, LIGHT, 2, 3, 5);
        self.place(f, LIGHT, 5, 3, 5);
        self.place(f, LIGHT, 2, 3, 10);
        self.place(f, LIGHT, 5, 3, 10);
        if self.open(south, SOUTH) {
            self.water(f, [3, 1, 0, 4, 2, 0]);
        }
        if self.open(south, EAST) {
            self.water(f, [7, 1, 3, 7, 2, 4]);
        }
        if self.open(south, WEST) {
            self.water(f, [0, 1, 3, 0, 2, 4]);
        }
        if self.open(north, NORTH) {
            self.water(f, [3, 1, 15, 4, 2, 15]);
        }
        if self.open(north, WEST) {
            self.water(f, [0, 1, 11, 0, 2, 12]);
        }
        if self.open(north, EAST) {
            self.water(f, [7, 1, 11, 7, 2, 12]);
        }
    }

    fn simple(&mut self, f: &Frame, r: usize, design: u8) {
        if self.rooms[r].index / 25 > 0 {
            self.default_floor(f, 0, 0, self.open(r, DOWN));
        }
        if self.conn(r, UP).is_none() {
            self.fill_only(f, [1, 4, 1, 6, 4, 6], GRAY);
        }
        let pillar = design != 0
            && self.rng.next_bool()
            && !self.open(r, DOWN)
            && !self.open(r, UP)
            && self.rooms[r].count_openings() > 1;
        match design {
            0 => {
                for (b, g1, g2, lamp) in [
                    ([0, 0, 2, 2], [0, 0, 0, 2], [1, 0, 2, 0], (1, 1)),
                    ([5, 0, 7, 2], [7, 0, 7, 2], [5, 0, 6, 0], (6, 1)),
                    ([0, 5, 2, 7], [0, 5, 0, 7], [1, 7, 2, 7], (1, 6)),
                    ([5, 5, 7, 7], [7, 5, 7, 7], [5, 7, 6, 7], (6, 6)),
                ] {
                    // Each corner: light courses at 1 and 3, gray on its two
                    // outer sides at 2, a lamp in the middle.
                    self.bx(f, [b[0], 1, b[1], b[2], 1, b[3]], LIGHT);
                    self.bx(f, [b[0], 3, b[1], b[2], 3, b[3]], LIGHT);
                    self.bx(f, [g1[0], 2, g1[1], g1[2], 2, g1[3]], GRAY);
                    self.bx(f, [g2[0], 2, g2[1], g2[2], 2, g2[3]], GRAY);
                    self.place(f, LAMP, lamp.0, 2, lamp.1);
                }
                if self.open(r, SOUTH) {
                    self.bx(f, [3, 3, 0, 4, 3, 0], LIGHT);
                } else {
                    self.bx(f, [3, 3, 0, 4, 3, 1], LIGHT);
                    self.bx(f, [3, 2, 0, 4, 2, 0], GRAY);
                    self.bx(f, [3, 1, 0, 4, 1, 1], LIGHT);
                }
                if self.open(r, NORTH) {
                    self.bx(f, [3, 3, 7, 4, 3, 7], LIGHT);
                } else {
                    self.bx(f, [3, 3, 6, 4, 3, 7], LIGHT);
                    self.bx(f, [3, 2, 7, 4, 2, 7], GRAY);
                    self.bx(f, [3, 1, 6, 4, 1, 7], LIGHT);
                }
                if self.open(r, WEST) {
                    self.bx(f, [0, 3, 3, 0, 3, 4], LIGHT);
                } else {
                    self.bx(f, [0, 3, 3, 1, 3, 4], LIGHT);
                    self.bx(f, [0, 2, 3, 0, 2, 4], GRAY);
                    self.bx(f, [0, 1, 3, 1, 1, 4], LIGHT);
                }
                if self.open(r, EAST) {
                    self.bx(f, [7, 3, 3, 7, 3, 4], LIGHT);
                } else {
                    self.bx(f, [6, 3, 3, 7, 3, 4], LIGHT);
                    self.bx(f, [7, 2, 3, 7, 2, 4], GRAY);
                    self.bx(f, [6, 1, 3, 7, 1, 4], LIGHT);
                }
            }
            1 => {
                self.bx(f, [2, 1, 2, 2, 3, 2], LIGHT);
                self.bx(f, [2, 1, 5, 2, 3, 5], LIGHT);
                self.bx(f, [5, 1, 5, 5, 3, 5], LIGHT);
                self.bx(f, [5, 1, 2, 5, 3, 2], LIGHT);
                self.place(f, LAMP, 2, 2, 2);
                self.place(f, LAMP, 2, 2, 5);
                self.place(f, LAMP, 5, 2, 5);
                self.place(f, LAMP, 5, 2, 2);
                for b in [
                    [0, 1, 0, 1, 3, 0],
                    [0, 1, 1, 0, 3, 1],
                    [0, 1, 7, 1, 3, 7],
                    [0, 1, 6, 0, 3, 6],
                    [6, 1, 7, 7, 3, 7],
                    [7, 1, 6, 7, 3, 6],
                    [6, 1, 0, 7, 3, 0],
                    [7, 1, 1, 7, 3, 1],
                ] {
                    self.bx(f, b, LIGHT);
                }
                for (x, z) in [(1, 0), (0, 1), (1, 7), (0, 6), (6, 7), (7, 6), (6, 0), (7, 1)] {
                    self.place(f, GRAY, x, 2, z);
                }
                if !self.open(r, SOUTH) {
                    self.bx(f, [1, 3, 0, 6, 3, 0], LIGHT);
                    self.bx(f, [1, 2, 0, 6, 2, 0], GRAY);
                    self.bx(f, [1, 1, 0, 6, 1, 0], LIGHT);
                }
                if !self.open(r, NORTH) {
                    self.bx(f, [1, 3, 7, 6, 3, 7], LIGHT);
                    self.bx(f, [1, 2, 7, 6, 2, 7], GRAY);
                    self.bx(f, [1, 1, 7, 6, 1, 7], LIGHT);
                }
                if !self.open(r, WEST) {
                    self.bx(f, [0, 3, 1, 0, 3, 6], LIGHT);
                    self.bx(f, [0, 2, 1, 0, 2, 6], GRAY);
                    self.bx(f, [0, 1, 1, 0, 1, 6], LIGHT);
                }
                if !self.open(r, EAST) {
                    self.bx(f, [7, 3, 1, 7, 3, 6], LIGHT);
                    self.bx(f, [7, 2, 1, 7, 2, 6], GRAY);
                    self.bx(f, [7, 1, 1, 7, 1, 6], LIGHT);
                }
            }
            _ => {
                self.ring(f);
                if self.open(r, SOUTH) {
                    self.water(f, [3, 1, 0, 4, 2, 0]);
                }
                if self.open(r, NORTH) {
                    self.water(f, [3, 1, 7, 4, 2, 7]);
                }
                if self.open(r, WEST) {
                    self.water(f, [0, 1, 3, 0, 2, 4]);
                }
                if self.open(r, EAST) {
                    self.water(f, [7, 1, 3, 7, 2, 4]);
                }
            }
        }
        if pillar {
            self.bx(f, [3, 1, 3, 4, 1, 4], LIGHT);
            self.bx(f, [3, 2, 3, 4, 2, 4], GRAY);
            self.bx(f, [3, 3, 3, 4, 3, 4], LIGHT);
        }
    }

    /// The walls `mainDesign` 2 and the top room share: light, dark and
    /// light courses with dark doorway frames.
    fn ring(&mut self, f: &Frame) {
        for (y, b) in [(1, LIGHT), (2, BLACK), (3, LIGHT)] {
            self.bx(f, [0, y, 0, 0, y, 7], b);
            self.bx(f, [7, y, 0, 7, y, 7], b);
            self.bx(f, [1, y, 0, 6, y, 0], b);
            self.bx(f, [1, y, 7, 6, y, 7], b);
        }
        self.bx(f, [0, 1, 3, 0, 2, 4], BLACK);
        self.bx(f, [7, 1, 3, 7, 2, 4], BLACK);
        self.bx(f, [3, 1, 0, 4, 2, 0], BLACK);
        self.bx(f, [3, 1, 7, 4, 2, 7], BLACK);
    }

    fn simple_top(&mut self, f: &Frame, r: usize) {
        if self.rooms[r].index / 25 > 0 {
            self.default_floor(f, 0, 0, self.open(r, DOWN));
        }
        if self.conn(r, UP).is_none() {
            self.fill_only(f, [1, 4, 1, 6, 4, 6], GRAY);
        }
        for x in 1..=6 {
            for z in 1..=6 {
                if self.rng.next_int(3) != 0 {
                    let y0 = 2 + i32::from(self.rng.next_int(4) != 0);
                    self.bx(f, [x, y0, z, x, 3, z], ob::WET_SPONGE);
                }
            }
        }
        self.ring(f);
        if self.open(r, SOUTH) {
            self.water(f, [3, 1, 0, 4, 2, 0]);
        }
    }

    fn wing_room(&mut self, f: &Frame, design: u8) {
        if design == 0 {
            for i in 0..4 {
                self.bx(f, [10 - i, 3 - i, 20 - i, 12 + i, 3 - i, 20], LIGHT);
            }
            for b in [
                [7, 0, 6, 15, 0, 16],
                [6, 0, 6, 6, 3, 20],
                [16, 0, 6, 16, 3, 20],
                [7, 1, 7, 7, 1, 20],
                [15, 1, 7, 15, 1, 20],
                [7, 1, 6, 9, 3, 6],
                [13, 1, 6, 15, 3, 6],
                [8, 1, 7, 9, 1, 7],
                [13, 1, 7, 14, 1, 7],
                [9, 0, 5, 13, 0, 5],
            ] {
                self.bx(f, b, LIGHT);
            }
            self.bx(f, [10, 0, 7, 12, 0, 7], BLACK);
            self.bx(f, [8, 0, 10, 8, 0, 12], BLACK);
            self.bx(f, [14, 0, 10, 14, 0, 12], BLACK);
            for z in (7..=18).rev().step_by(3) {
                self.place(f, LAMP, 6, 3, z);
                self.place(f, LAMP, 16, 3, z);
            }
            self.place(f, LAMP, 10, 0, 10);
            self.place(f, LAMP, 12, 0, 10);
            self.place(f, LAMP, 10, 0, 12);
            self.place(f, LAMP, 12, 0, 12);
            self.place(f, LAMP, 8, 3, 6);
            self.place(f, LAMP, 14, 3, 6);
            for (x, z) in [(4, 4), (18, 4), (4, 18), (18, 18)] {
                self.place(f, LIGHT, x, 2, z);
                self.place(f, LAMP, x, 1, z);
                self.place(f, LIGHT, x, 0, z);
            }
            self.place(f, LIGHT, 9, 7, 20);
            self.place(f, LIGHT, 13, 7, 20);
            self.bx(f, [6, 0, 21, 7, 4, 21], LIGHT);
            self.bx(f, [15, 0, 21, 16, 4, 21], LIGHT);
            self.elder(f, 11, 2, 16);
        } else {
            self.bx(f, [9, 3, 18, 13, 3, 20], LIGHT);
            self.bx(f, [9, 0, 18, 9, 2, 18], LIGHT);
            self.bx(f, [13, 0, 18, 13, 2, 18], LIGHT);
            for x in [9, 13] {
                self.place(f, LIGHT, x, 6, 20);
                self.place(f, LAMP, x, 5, 20);
                self.place(f, LIGHT, x, 4, 20);
            }
            self.bx(f, [7, 3, 7, 15, 3, 14], LIGHT);
            for x in [10, 12] {
                self.bx(f, [x, 0, 10, x, 6, 10], LIGHT);
                self.bx(f, [x, 0, 12, x, 6, 12], LIGHT);
                self.place(f, LAMP, x, 0, 10);
                self.place(f, LAMP, x, 0, 12);
                self.place(f, LAMP, x, 4, 10);
                self.place(f, LAMP, x, 4, 12);
            }
            for x in [8, 14] {
                self.bx(f, [x, 0, 7, x, 2, 7], LIGHT);
                self.bx(f, [x, 0, 14, x, 2, 14], LIGHT);
            }
            self.bx(f, [8, 3, 8, 8, 3, 13], BLACK);
            self.bx(f, [14, 3, 8, 14, 3, 13], BLACK);
            self.elder(f, 11, 5, 13);
        }
    }

    fn penthouse(&mut self, f: &Frame) {
        self.bx(f, [2, -1, 2, 11, -1, 11], LIGHT);
        self.bx(f, [0, -1, 0, 1, -1, 11], GRAY);
        self.bx(f, [12, -1, 0, 13, -1, 11], GRAY);
        self.bx(f, [2, -1, 0, 11, -1, 1], GRAY);
        self.bx(f, [2, -1, 12, 11, -1, 13], GRAY);
        self.bx(f, [0, 0, 0, 0, 0, 13], LIGHT);
        self.bx(f, [13, 0, 0, 13, 0, 13], LIGHT);
        self.bx(f, [1, 0, 0, 12, 0, 0], LIGHT);
        self.bx(f, [1, 0, 13, 12, 0, 13], LIGHT);
        for i in (2..=11).step_by(3) {
            self.place(f, LAMP, 0, 0, i);
            self.place(f, LAMP, 13, 0, i);
            self.place(f, LAMP, i, 0, 0);
        }
        self.bx(f, [2, 0, 3, 4, 0, 9], LIGHT);
        self.bx(f, [9, 0, 3, 11, 0, 9], LIGHT);
        self.bx(f, [4, 0, 9, 9, 0, 11], LIGHT);
        self.place(f, LIGHT, 5, 0, 8);
        self.place(f, LIGHT, 8, 0, 8);
        self.place(f, LIGHT, 10, 0, 10);
        self.place(f, LIGHT, 3, 0, 10);
        self.bx(f, [3, 0, 3, 3, 0, 7], BLACK);
        self.bx(f, [10, 0, 3, 10, 0, 7], BLACK);
        self.bx(f, [6, 0, 10, 7, 0, 10], BLACK);
        for x in [3, 10] {
            for z in (2..=8).step_by(3) {
                self.bx(f, [x, 0, z, x, 2, z], LIGHT);
            }
        }
        self.bx(f, [5, 0, 10, 5, 2, 10], LIGHT);
        self.bx(f, [8, 0, 10, 8, 2, 10], LIGHT);
        self.bx(f, [6, -1, 7, 7, -1, 8], BLACK);
        self.water(f, [6, -1, 3, 7, -1, 4]);
        self.elder(f, 6, 1, 6);
    }
}

/// The residents a built monument houses.
pub fn residents(built: &Built) -> Vec<(MobKind, IVec3)> {
    built.elders.iter().map(|p| (MobKind::ElderGuardian, *p)).collect()
}

#[cfg(test)]
mod tests;
