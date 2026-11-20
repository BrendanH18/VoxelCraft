//! The End city's templates (fidelity exception FE-02).
//!
//! [`TEMPLATES`] holds what Java 26.3's `structure/end_city/*.nbt`
//! templates define for play: the size, whether every cell of the box is
//! stored, and the data markers `EndCityPiece.handleDataMarker` turns into
//! loot chests, shulkers and the ship's elytra frame.
//! `tests/fixtures/java/26.3/end_cities.json` is checked against it in both
//! directions.
//!
//! The blocks are VoxelCraft's own design of each piece kind, in the
//! purpur, End stone brick, magenta glass and End rod palette Java's pieces
//! use (also listed in the fixture): storeys with a ladder shaft that lines
//! up through every floor and roof of a house, towers with a central post
//! and a ladder through each floor, five-wide bridges with a deck one
//! block up that meet the doorways bridge ends cut, fat towers, and a ship
//! with its stern cabin of chests and the elytra frame.
//!
//! Local frames: a house's storeys and roofs step one block out on -x and
//! -z per level (Java's `(-1, y, -1)` offsets), so the shaft is base-floor
//! cell (2, y, 1) everywhere; a tower's ladder climbs at (3, y, 2) on a post
//! at (3, y, 3), away from the four bridge doorways; bridges run toward -z.

use std::sync::OnceLock;

use glam::IVec3;

use super::super::block::{Block, Facing};
use super::super::end_blocks::{self as e, Dir};
use super::super::mansion::Rotation;

/// A data marker (`handleDataMarker`'s metadata prefix).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Marker {
    /// A loot chest one block below.
    Chest,
    /// A shulker.
    Sentry,
    /// The ship's item frame with an elytra.
    Elytra,
}

impl Marker {
    pub fn java(self) -> &'static str {
        match self {
            Marker::Chest => "Chest",
            Marker::Sentry => "Sentry",
            Marker::Elytra => "Elytra",
        }
    }
}

/// What Java's template of a name defines for play.
pub struct Facts {
    pub name: &'static str,
    pub size: [i32; 3],
    /// Whether every cell of the box is stored (air included).
    pub filled: bool,
    pub markers: &'static [(Marker, [i32; 3])],
}

/// From `end_cities.json`'s `templates`, in name order.
pub const TEMPLATES: [Facts; 20] = [
    Facts {
        name: "base_floor",
        size: [10, 4, 10],
        filled: true,
        markers: &[(Marker::Sentry, [3, 2, 9]), (Marker::Sentry, [6, 2, 9])],
    },
    Facts { name: "base_roof", size: [12, 2, 12], filled: true, markers: &[] },
    Facts { name: "bridge_end", size: [5, 6, 2], filled: true, markers: &[] },
    Facts { name: "bridge_gentle_stairs", size: [5, 7, 8], filled: true, markers: &[] },
    Facts { name: "bridge_piece", size: [5, 6, 4], filled: true, markers: &[] },
    Facts { name: "bridge_steep_stairs", size: [5, 7, 4], filled: true, markers: &[] },
    Facts { name: "fat_tower_base", size: [13, 4, 13], filled: true, markers: &[] },
    Facts {
        name: "fat_tower_middle",
        size: [13, 8, 13],
        filled: true,
        markers: &[
            (Marker::Sentry, [2, 2, 6]),
            (Marker::Sentry, [10, 2, 6]),
            (Marker::Sentry, [6, 6, 2]),
            (Marker::Sentry, [6, 6, 10]),
        ],
    },
    Facts {
        name: "fat_tower_top",
        size: [17, 6, 17],
        filled: true,
        markers: &[(Marker::Chest, [3, 2, 11]), (Marker::Chest, [5, 2, 13])],
    },
    Facts { name: "second_floor_1", size: [12, 8, 12], filled: true, markers: &[] },
    Facts { name: "second_floor_2", size: [12, 8, 12], filled: true, markers: &[(Marker::Sentry, [8, 5, 6])] },
    Facts { name: "second_roof", size: [14, 2, 14], filled: true, markers: &[] },
    Facts {
        name: "ship",
        size: [13, 24, 29],
        filled: true,
        markers: &[
            (Marker::Sentry, [6, 4, 8]),
            (Marker::Chest, [5, 5, 7]),
            (Marker::Elytra, [6, 5, 7]),
            (Marker::Chest, [7, 5, 7]),
            (Marker::Sentry, [8, 6, 27]),
            (Marker::Sentry, [4, 11, 27]),
        ],
    },
    Facts { name: "third_floor_1", size: [14, 8, 14], filled: true, markers: &[] },
    Facts {
        name: "third_floor_2",
        size: [14, 8, 14],
        filled: true,
        markers: &[(Marker::Sentry, [2, 5, 2]), (Marker::Sentry, [11, 5, 2]), (Marker::Chest, [6, 6, 2])],
    },
    Facts { name: "third_roof", size: [16, 2, 16], filled: true, markers: &[] },
    Facts { name: "tower_base", size: [7, 7, 7], filled: false, markers: &[] },
    Facts { name: "tower_floor", size: [7, 4, 7], filled: true, markers: &[] },
    Facts { name: "tower_piece", size: [7, 4, 7], filled: true, markers: &[] },
    Facts { name: "tower_top", size: [9, 5, 9], filled: true, markers: &[(Marker::Sentry, [4, 3, 4])] },
];

/// The ship's dragon head: its cell in the ship's frame (on the bow's
/// stand) and the way it faces. Java's ship holds a `dragon_wall_head` on
/// its bow; `City::paint` places Track C's head here.
pub const DRAGON_HEAD: ([i32; 3], Facing) = ([6, 7, 28], Facing::South);

pub fn template_index(name: &str) -> Option<u8> {
    TEMPLATES.iter().position(|t| t.name == name).map(|i| i as u8)
}

/// One stored template cell in the template frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cell {
    Air,
    Purpur,
    /// A purpur pillar along y (0), x (1) or z (2).
    Pillar(u16),
    /// Purpur stairs: the low step's side and whether upside down.
    Stairs(Facing, bool),
    /// A purpur slab, top half when true.
    Slab(bool),
    Bricks,
    Glass,
    Rod(Dir),
    /// A ladder facing away from its wall.
    Ladder(Facing),
    Chest(Facing),
    EnderChest(Facing),
    Brewing,
    Obsidian,
}

impl Cell {
    /// The block a piece turned `rotation` places for this cell.
    pub fn resolve(self, rotation: Rotation) -> Block {
        let turns = rotation as u8;
        let f = |f: Facing| rotation.facing(f);
        match self {
            Cell::Air => Block::AIR,
            Cell::Purpur => e::PURPUR_BLOCK,
            Cell::Pillar(axis) => e::rotated(Block(e::PURPUR_PILLAR.0 + axis), turns),
            Cell::Stairs(front, top) => {
                super::super::slab_stair_states::stairs_id(super::super::slab_stair_states::Stairs {
                    family: super::super::slab_stair_states::PURPUR_FAMILY as u8,
                    facing: f(front),
                    top,
                    shape: super::super::slab_stair_states::StairShape::Straight,
                    wet: false,
                })
            }
            Cell::Slab(top) => Block(e::PURPUR_SLAB.0 + if top { 2 } else { 0 }),
            Cell::Bricks => super::super::stone_blocks::END_STONE_BRICKS,
            Cell::Glass => magenta_glass(),
            Cell::Rod(d) => e::rotated(e::end_rod(d), turns),
            Cell::Ladder(facing) => Block::LADDER.with_facing(f(facing)),
            Cell::Chest(facing) => Block::CHEST.with_facing(f(facing)),
            Cell::EnderChest(facing) => super::super::utilities::ENDER_CHEST.with_facing(f(facing)),
            Cell::Brewing => Block::BREWING_STAND,
            Cell::Obsidian => Block::OBSIDIAN,
        }
    }
}

fn magenta_glass() -> Block {
    static GLASS: OnceLock<Block> = OnceLock::new();
    *GLASS.get_or_init(|| crate::java_registry::block("magenta_stained_glass", "").unwrap_or(Block::GLASS))
}

/// An original template: its stored cells and Java's markers.
pub struct Template {
    pub size: IVec3,
    cells: Vec<Option<Cell>>,
    markers: Vec<(Marker, IVec3)>,
}

impl Template {
    pub fn cell(&self, p: IVec3) -> Option<Cell> {
        if p.cmplt(IVec3::ZERO).any() || p.cmpge(self.size).any() {
            return None;
        }
        self.cells[((p.y * self.size.z + p.z) * self.size.x + p.x) as usize]
    }

    pub fn markers(&self) -> &[(Marker, IVec3)] {
        &self.markers
    }
}

/// Template `i` of [`TEMPLATES`], built on first use.
pub fn template(i: u8) -> &'static Template {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    &ALL.get_or_init(|| TEMPLATES.iter().map(build).collect())[i as usize]
}

type P = (i32, i32, i32);

struct B {
    size: IVec3,
    cells: Vec<Option<Cell>>,
}

impl B {
    fn inside(&self, (x, y, z): P) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.size.x && y < self.size.y && z < self.size.z
    }

    fn put(&mut self, p: P, cell: Cell) {
        if self.inside(p) {
            let (x, y, z) = p;
            self.cells[((y * self.size.z + z) * self.size.x + x) as usize] = Some(cell);
        }
    }

    fn void(&mut self, p: P) {
        if self.inside(p) {
            let (x, y, z) = p;
            self.cells[((y * self.size.z + z) * self.size.x + x) as usize] = None;
        }
    }

    fn fill(&mut self, a: P, b: P, cell: Cell) {
        for y in a.1.min(b.1)..=a.1.max(b.1) {
            for z in a.2.min(b.2)..=a.2.max(b.2) {
                for x in a.0.min(b.0)..=a.0.max(b.0) {
                    self.put((x, y, z), cell);
                }
            }
        }
    }

    /// The outline of the box `(x0, z0)..=(x1, z1)` from `y0` to `y1`.
    fn walls(&mut self, (x0, z0): (i32, i32), (x1, z1): (i32, i32), y0: i32, y1: i32, cell: Cell) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.put((x, y, z0), cell);
                self.put((x, y, z1), cell);
            }
            for z in z0..=z1 {
                self.put((x0, y, z), cell);
                self.put((x1, y, z), cell);
            }
        }
    }

    /// Pillars up the four corners of the box.
    fn corners(&mut self, (x0, z0): (i32, i32), (x1, z1): (i32, i32), y0: i32, y1: i32) {
        for (x, z) in [(x0, z0), (x0, z1), (x1, z0), (x1, z1)] {
            self.fill((x, y0, z), (x, y1, z), Cell::Pillar(0));
        }
    }

    /// Glass panes of windows two wide in the middle of each wall at `y`.
    fn windows(&mut self, (x0, z0): (i32, i32), (x1, z1): (i32, i32), y: i32) {
        let (mx, mz) = ((x0 + x1) / 2, (z0 + z1) / 2);
        for d in [0, 1] {
            self.put((mx + d, y, z0), Cell::Glass);
            self.put((mx + d, y, z1), Cell::Glass);
            self.put((x0, y, mz + d), Cell::Glass);
            self.put((x1, y, mz + d), Cell::Glass);
        }
    }

    /// The house's ladder shaft at local `(x, z)` (base-floor cell (2, 1)
    /// shifted into this frame) from `y0` to `y1`, on a pillar to its west,
    /// cutting through any floor on the way.
    fn shaft(&mut self, (x, z): (i32, i32), y0: i32, y1: i32) {
        for y in y0..=y1 {
            self.put((x - 1, y, z), Cell::Pillar(0));
            self.put((x, y, z), Cell::Ladder(Facing::East));
        }
    }

    /// A tower's ladder at `(x, z)` climbing a post one block south.
    fn tower_shaft(&mut self, (x, z): (i32, i32), y0: i32, y1: i32) {
        for y in y0..=y1 {
            self.put((x, y, z + 1), Cell::Pillar(0));
            self.put((x, y, z), Cell::Ladder(Facing::North));
        }
    }

    /// A roof of slabs over the box with stairs round the rim and End rods
    /// standing at the corners, and the shaft's hole.
    fn roof(&mut self, hole: (i32, i32)) {
        let (mx, mz) = (self.size.x - 1, self.size.z - 1);
        self.fill((0, 0, 0), (mx, 0, mz), Cell::Purpur);
        for x in 1..mx {
            self.put((x, 1, 0), Cell::Stairs(Facing::North, false));
            self.put((x, 1, mz), Cell::Stairs(Facing::South, false));
        }
        for z in 1..mz {
            self.put((0, 1, z), Cell::Stairs(Facing::West, false));
            self.put((mx, 1, z), Cell::Stairs(Facing::East, false));
        }
        for (x, z) in [(0, 0), (0, mz), (mx, 0), (mx, mz)] {
            self.put((x, 1, z), Cell::Rod(Dir::Up));
        }
        self.put((hole.0, 0, hole.1), Cell::Ladder(Facing::East));
        self.put((hole.0 - 1, 0, hole.1), Cell::Pillar(0));
    }

    /// A storey of a house: its floor at `y` with the shaft's hole, walls
    /// above, windows, corner pillars and End rods lighting the room.
    fn storey(&mut self, y: i32, shaft: (i32, i32)) {
        let (mx, mz) = (self.size.x - 1, self.size.z - 1);
        self.fill((0, y, 0), (mx, y, mz), Cell::Bricks);
        self.walls((0, 0), (mx, mz), y, y, Cell::Purpur);
        self.walls((0, 0), (mx, mz), y + 1, y + 3, Cell::Purpur);
        self.corners((0, 0), (mx, mz), y, y + 3);
        self.windows((0, 0), (mx, mz), y + 2);
        self.put((mx - 1, y + 3, mz - 1), Cell::Rod(Dir::Down));
        self.put((mx - 1, y + 3, 1), Cell::Rod(Dir::Down));
        self.shaft(shaft, y, y + 3);
    }
}

fn build(f: &Facts) -> Template {
    let size = IVec3::from(f.size);
    let fill = if f.filled { Some(Cell::Air) } else { None };
    let mut b = B { size, cells: vec![fill; (size.x * size.y * size.z) as usize] };
    design(f.name, &mut b);
    let markers: Vec<(Marker, IVec3)> = f.markers.iter().map(|&(m, p)| (m, IVec3::from(p))).collect();
    // A marker's own cell is not placed: the structure block is ignored.
    for &(_, p) in &markers {
        b.void((p.x, p.y, p.z));
    }
    Template { size, cells: b.cells, markers }
}

use Facing::{East as E, North as N, South as S};

fn design(name: &str, b: &mut B) {
    let (mx, my, mz) = (b.size.x - 1, b.size.y - 1, b.size.z - 1);
    match name {
        // The ground storey: an End stone brick floor, purpur walls, the
        // doorway the bridges arrive at on the south side (z 9, between
        // the two sentries' cells) and the foot of the shaft.
        "base_floor" => {
            b.fill((0, 0, 0), (mx, 0, mz), Cell::Bricks);
            b.walls((0, 0), (mx, mz), 0, 0, Cell::Purpur);
            b.walls((0, 0), (mx, mz), 1, 3, Cell::Purpur);
            b.corners((0, 0), (mx, mz), 0, 3);
            b.windows((0, 0), (mx, mz), 2);
            b.fill((4, 1, mz), (5, 3, mz), Cell::Air);
            b.put((4, 0, mz), Cell::Stairs(S, false));
            b.put((5, 0, mz), Cell::Stairs(S, false));
            b.put((7, 3, 7), Cell::Rod(Dir::Down));
            b.shaft((2, 1), 1, 3);
        }
        // Upper storeys stand on the base floor's walls; their lower half
        // is open (only the corner posts are stored as blocks).
        "second_floor_1" | "second_floor_2" => {
            b.corners((0, 0), (mx, mz), 0, 3);
            b.shaft((3, 2), 0, 3);
            b.storey(4, (3, 2));
            b.put((6, 5, 6), Cell::Purpur);
            b.put((6, 6, 6), Cell::Rod(Dir::Up));
        }
        "third_floor_1" | "third_floor_2" => {
            b.corners((0, 0), (mx, mz), 0, 3);
            b.storey(4, (4, 3));
            if name == "third_floor_2" {
                // A table for the loot chest (marker at (6, 6, 2)) and an
                // Ender chest beside it.
                b.put((6, 5, 2), Cell::Chest(S));
                b.put((7, 5, 2), Cell::EnderChest(S));
            }
        }
        "base_roof" => b.roof((3, 2)),
        "second_roof" => b.roof((4, 3)),
        "third_roof" => b.roof((5, 4)),
        // Only the floor, walls, post and ladder are stored: the base sinks
        // into the roof below without clearing it. The doorway and floor
        // holes let the house's shaft out wherever the tower stands.
        "tower_base" => {
            b.fill((3, 0, 3), (3, 6, 3), Cell::Pillar(0));
            b.fill((0, 3, 0), (6, 3, 6), Cell::Purpur);
            b.walls((0, 0), (6, 6), 4, 6, Cell::Purpur);
            b.corners((0, 0), (6, 6), 3, 6);
            b.tower_shaft((3, 2), 3, 6);
            b.fill((1, 3, 0), (2, 3, 1), Cell::Air);
            b.fill((1, 4, 0), (2, 5, 0), Cell::Air);
            b.fill((3, 4, 6), (3, 5, 6), Cell::Air);
        }
        "tower_piece" | "tower_floor" => {
            b.fill((0, 0, 0), (6, 0, 6), if name == "tower_floor" { Cell::Bricks } else { Cell::Purpur });
            b.walls((0, 0), (6, 6), 1, 3, Cell::Purpur);
            b.corners((0, 0), (6, 6), 0, 3);
            for (x, z) in [(3, 0), (0, 3), (6, 3), (3, 6)] {
                b.put((x, 2, z), Cell::Glass);
            }
            b.tower_shaft((3, 2), 0, 3);
        }
        "tower_top" => {
            b.fill((0, 0, 0), (8, 0, 8), Cell::Purpur);
            b.fill((1, 0, 1), (7, 0, 7), Cell::Bricks);
            b.walls((0, 0), (8, 8), 1, 1, Cell::Slab(false));
            b.corners((0, 0), (8, 8), 1, 3);
            for (x, z) in [(0, 0), (0, 8), (8, 0), (8, 8)] {
                b.put((x, 4, z), Cell::Rod(Dir::Up));
            }
            b.fill((4, 0, 4), (4, 2, 4), Cell::Pillar(0));
            b.tower_shaft((4, 3), 0, 2);
            b.fill((1, 4, 1), (7, 4, 7), Cell::Purpur);
            b.walls((1, 1), (7, 7), 4, 4, Cell::Stairs(N, true));
            b.fill((3, 4, 3), (5, 4, 5), Cell::Glass);
        }
        // A five-wide deck one block up with low rails and a centre beam.
        "bridge_piece" => {
            b.fill((0, 1, 0), (4, 1, mz), Cell::Purpur);
            b.fill((2, 0, 0), (2, 0, mz), Cell::Pillar(2));
            for z in 0..=mz {
                b.put((0, 2, z), Cell::Slab(false));
                b.put((4, 2, z), Cell::Slab(false));
            }
        }
        // The arch where a bridge meets a wall: the deck, posts each side,
        // a lintel with End rods, and the doorway cut through.
        "bridge_end" => {
            b.fill((0, 0, 0), (4, 0, 1), Cell::Purpur);
            b.fill((0, 1, 0), (4, 1, 1), Cell::Purpur);
            b.fill((0, 2, 0), (0, 4, 0), Cell::Pillar(0));
            b.fill((4, 2, 0), (4, 4, 0), Cell::Pillar(0));
            b.fill((0, 5, 0), (4, 5, 0), Cell::Purpur);
            b.put((0, 5, 1), Cell::Rod(Dir::South));
            b.put((4, 5, 1), Cell::Rod(Dir::South));
            b.put((2, 5, 1), Cell::Slab(true));
        }
        // Four steps up over four blocks toward -z.
        "bridge_steep_stairs" => {
            for z in 0..=mz {
                let top = 1 + (mz + 1 - z);
                b.fill((1, 1, z), (3, top - 1, z), Cell::Purpur);
                b.fill((1, top, z), (3, top, z), Cell::Stairs(S, false));
                b.fill((0, top - 1, z), (0, top, z), Cell::Purpur);
                b.fill((4, top - 1, z), (4, top, z), Cell::Purpur);
            }
            b.put((2, 0, mz), Cell::Pillar(0));
        }
        // Four steps up over eight blocks toward -z, a landing between each.
        "bridge_gentle_stairs" => {
            for z in 0..=mz {
                let top = 1 + (mz + 1 - z + 1) / 2;
                b.fill((1, 1, z), (3, top - 1, z), Cell::Purpur);
                let step = if (mz - z) % 2 == 0 { Cell::Stairs(S, false) } else { Cell::Purpur };
                b.fill((1, top, z), (3, top, z), step);
                b.put((0, top + 1, z), Cell::Slab(false));
                b.put((4, top + 1, z), Cell::Slab(false));
                b.put((0, top, z), Cell::Purpur);
                b.put((4, top, z), Cell::Purpur);
            }
            for z in [1, mz - 1] {
                b.put((2, 0, z), Cell::Pillar(0));
            }
        }
        "fat_tower_base" => {
            b.fill((0, 0, 0), (mx, 0, mz), Cell::Purpur);
            b.fill((2, 0, 2), (mx - 2, 0, mz - 2), Cell::Bricks);
            b.walls((0, 0), (mx, mz), 1, 3, Cell::Pillar(0));
            b.windows((0, 0), (mx, mz), 2);
            b.fill((6, 0, 6), (6, 3, 6), Cell::Pillar(0));
            b.tower_shaft((6, 5), 0, 3);
            for (x, z) in [(0, 0), (0, mz), (mx, 0), (mx, mz)] {
                b.put((x, my, z), Cell::Rod(Dir::Up));
            }
        }
        "fat_tower_middle" => {
            for y in [0, 4] {
                b.fill((0, y, 0), (mx, y, mz), Cell::Purpur);
                b.walls((0, 0), (mx, mz), y + 1, y + 3, Cell::Pillar(0));
                b.windows((0, 0), (mx, mz), y + 2);
                b.put((1, y + 3, 1), Cell::Rod(Dir::Down));
                b.put((mx - 1, y + 3, mz - 1), Cell::Rod(Dir::Down));
            }
            b.fill((6, 0, 6), (6, 7, 6), Cell::Pillar(0));
            b.tower_shaft((6, 5), 0, 7);
        }
        // The treasure loft: a wide floor, a hipped roof of stairs, the
        // two loot chests by the west wall (markers at y 2) and a lamp.
        "fat_tower_top" => {
            b.fill((0, 0, 0), (mx, 0, mz), Cell::Purpur);
            b.fill((2, 0, 2), (mx - 2, 0, mz - 2), Cell::Bricks);
            b.walls((1, 1), (mx - 1, mz - 1), 1, 3, Cell::Purpur);
            b.corners((1, 1), (mx - 1, mz - 1), 1, 3);
            b.windows((1, 1), (mx - 1, mz - 1), 2);
            b.walls((0, 0), (mx, mz), 1, 1, Cell::Slab(false));
            b.fill((2, 4, 2), (mx - 2, 4, mz - 2), Cell::Purpur);
            b.walls((1, 1), (mx - 1, mz - 1), 4, 4, Cell::Stairs(N, false));
            b.fill((5, 5, 5), (mx - 5, 5, mz - 5), Cell::Slab(false));
            b.fill((8, 0, 8), (8, 4, 8), Cell::Pillar(0));
            b.tower_shaft((8, 7), 0, 3);
            b.put((3, 1, 11), Cell::Chest(E));
            b.put((5, 1, 13), Cell::Chest(N));
            b.put((8, 3, 12), Cell::Rod(Dir::Down));
        }
        "ship" => ship(b),
        _ => {}
    }
}

/// Half the hull's width at `z` (the stern at z 0, the bow at z 28).
fn hull(z: i32) -> i32 {
    match z {
        0 => 3,
        1..=2 => 4,
        3..=20 => 5,
        21..=23 => 4,
        24..=25 => 3,
        26 => 2,
        27 => 1,
        _ => 0,
    }
}

/// The End ship: a purpur hull on an obsidian keel, the main deck at
/// y 3, the stern cabin (z 3–9) with the two loot chests either side of
/// the elytra frame on its back wall, a raised forecastle and a lookout
/// over the bow, a mast with a sail, and the dragon head's stand.
fn ship(b: &mut B) {
    for z in 0..=28 {
        let w = hull(z);
        for x in 6 - w..=6 + w {
            let edge = (x - 6).abs() == w;
            // Keel, bilge and sides narrowing toward the bottom.
            if (x - 6).abs() <= (w - 2).max(0) || z == 0 {
                b.put((x, 0, z), Cell::Purpur);
            }
            if (x - 6).abs() < w {
                b.put((x, 1, z), if edge { Cell::Purpur } else { Cell::Air });
            }
            if edge {
                b.fill((x, 1, z), (x, 3, z), Cell::Purpur);
                b.put((x, 4, z), Cell::Slab(false));
            } else {
                b.put((x, 3, z), Cell::Purpur);
            }
        }
        if (2..=26).contains(&z) {
            b.put((6, 0, z), Cell::Obsidian);
        }
    }
    b.put((6, 0, 1), Cell::Obsidian);
    // The stern cabin: walls round z 3..=9, its back wall (z 6) holding
    // the elytra frame between the chests, a door forward.
    b.walls((2, 3), (10, 9), 4, 7, Cell::Purpur);
    b.corners((2, 3), (10, 9), 4, 7);
    b.fill((2, 8, 3), (10, 8, 9), Cell::Purpur);
    b.walls((2, 3), (10, 9), 8, 8, Cell::Stairs(N, false));
    b.fill((3, 4, 6), (9, 7, 6), Cell::Purpur);
    b.put((4, 6, 6), Cell::Glass);
    b.put((8, 6, 6), Cell::Glass);
    b.fill((6, 4, 9), (6, 5, 9), Cell::Air);
    b.put((5, 4, 7), Cell::Chest(S));
    b.put((7, 4, 7), Cell::Chest(S));
    b.put((3, 4, 8), Cell::Brewing);
    b.put((6, 7, 8), Cell::Rod(Dir::Down));
    b.put((2, 5, 6), Cell::Glass);
    b.put((10, 5, 6), Cell::Glass);
    // The mast and its sail.
    b.fill((6, 4, 15), (6, 22, 15), Cell::Pillar(0));
    b.fill((1, 18, 15), (11, 18, 15), Cell::Pillar(1));
    b.fill((2, 10, 16), (10, 17, 16), Cell::Purpur);
    b.fill((3, 11, 16), (9, 16, 16), Cell::Glass);
    b.put((6, 23, 15), Cell::Rod(Dir::Up));
    b.put((1, 19, 15), Cell::Rod(Dir::Up));
    b.put((11, 19, 15), Cell::Rod(Dir::Up));
    // The forecastle (deck at y 5) with a ladder up from the main deck.
    for z in 22..=27 {
        let w = hull(z);
        b.fill((6 - w + 1, 4, z), (6 + w - 1, 5, z), Cell::Purpur);
        b.put((6 - w, 5, z), Cell::Purpur);
        b.put((6 + w, 5, z), Cell::Purpur);
        b.put((6 - w, 6, z), Cell::Slab(false));
        b.put((6 + w, 6, z), Cell::Slab(false));
    }
    b.fill((5, 4, 21), (7, 4, 21), Cell::Stairs(N, false));
    // The lookout: posts up from the bow to a platform at y 10.
    b.fill((5, 6, 26), (5, 9, 26), Cell::Pillar(0));
    b.fill((7, 6, 26), (7, 9, 26), Cell::Pillar(0));
    b.fill((3, 10, 25), (9, 10, 27), Cell::Purpur);
    b.fill((4, 6, 25), (4, 9, 25), Cell::Ladder(S));
    b.put((4, 10, 25), Cell::Air);
    b.put((4, 5, 25), Cell::Purpur);
    b.walls((3, 25), (9, 27), 11, 11, Cell::Slab(false));
    b.put((9, 11, 27), Cell::Rod(Dir::Up));
    // The bow's stand (the head goes on its south face, see `DRAGON_HEAD`).
    b.fill((6, 4, 28), (6, 6, 28), Cell::Purpur);
    b.put((6, 3, 28), Cell::Purpur);
    b.put((6, 6, 27), Cell::Purpur);
    b.put((6, 11, 26), Cell::Rod(Dir::Up));
    // A ledge under the bow sentry.
    b.put((8, 5, 27), Cell::Purpur);
}
