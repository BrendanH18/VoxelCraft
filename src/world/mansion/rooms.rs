//! The woodland mansion's templates (fidelity exception FE-02).
//!
//! [`TEMPLATES`] holds what Java 26.3's templates of each name define for
//! play: the size, whether every cell of the box is stored (a stored air
//! cell clears what was there; an unstored one leaves it), and the data
//! markers `WoodlandMansionPiece.handleDataMarker` turns into chests,
//! evokers (`Mage`), vindicators (`Warrior`) and allays. [`FIXED`] holds the
//! containers whose contents the templates fix: the secret room's spider
//! spawner and four chests of set items. `tests/fixtures/java/26.3/mansions.json`
//! is checked against both tables in both directions.
//!
//! The blocks are VoxelCraft's own: each template below is an original
//! room of the same kind as Java's room of that name (its kind is checked
//! by the block families `mansions.json` lists, such as bookshelves for a
//! library or wool for a statue room), drawn around Java's markers.
//!
//! Local frames follow Java's: rooms open on the east at z 2..=4 (1×1 and
//! side-entrance rooms), on the south at x 2..=4 (front-entrance 1×2
//! rooms) or on the east of the north-east cell (2×2 rooms); the floor is
//! layer 0 and walls come from the wall pieces.

use std::sync::OnceLock;

use glam::IVec3;

use super::{Mirror, Rotation, transform_facing};
use crate::entity::MobKind;
use crate::world::block::{Block, Facing};

/// A data marker (`handleDataMarker`'s metadata).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Marker {
    ChestNorth,
    ChestSouth,
    ChestEast,
    ChestWest,
    /// An evoker.
    Mage,
    /// A vindicator.
    Warrior,
    /// One to three allays (`Group of Allays`).
    Allays,
}

impl Marker {
    /// The metadata string in Java's template.
    pub fn java(self) -> &'static str {
        match self {
            Marker::ChestNorth => "ChestNorth",
            Marker::ChestSouth => "ChestSouth",
            Marker::ChestEast => "ChestEast",
            Marker::ChestWest => "ChestWest",
            Marker::Mage => "Mage",
            Marker::Warrior => "Warrior",
            Marker::Allays => "Group of Allays",
        }
    }

    /// A chest marker's facing in the template frame.
    pub fn chest(self) -> Option<Facing> {
        match self {
            Marker::ChestNorth => Some(Facing::North),
            Marker::ChestSouth => Some(Facing::South),
            Marker::ChestEast => Some(Facing::East),
            Marker::ChestWest => Some(Facing::West),
            _ => None,
        }
    }

    /// What the marker leaves in the world: a chest turned by the piece's
    /// rotation only (Java ignores the mirror here), or air where a mob
    /// spawned.
    pub fn block(self, rotation: Rotation) -> Block {
        match self.chest() {
            Some(f) => Block::CHEST.with_facing(rotation.facing(f)),
            None => Block::AIR,
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

/// Generated from `mansions.json`'s `templates`, in name order.
pub const TEMPLATES: [Facts; 73] = [
    Facts { name: "1x1_a1", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_a2", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_a3", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_a4", size: [7, 8, 7], filled: true, markers: &[(Marker::ChestWest, [6, 6, 3])] },
    Facts { name: "1x1_a5", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_as1", size: [7, 8, 7], filled: true, markers: &[(Marker::ChestWest, [5, 1, 1])] },
    Facts { name: "1x1_as2", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_as3", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_as4", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "1x1_b1", size: [7, 11, 7], filled: true, markers: &[] },
    Facts { name: "1x1_b2", size: [7, 11, 7], filled: true, markers: &[] },
    Facts { name: "1x1_b3", size: [7, 11, 7], filled: true, markers: &[] },
    Facts { name: "1x1_b4", size: [7, 11, 7], filled: true, markers: &[] },
    Facts { name: "1x1_b5", size: [7, 11, 7], filled: true, markers: &[] },
    Facts {
        name: "1x2_a1",
        size: [7, 8, 15],
        filled: true,
        markers: &[(Marker::Warrior, [3, 1, 3]), (Marker::ChestSouth, [3, 1, 12])],
    },
    Facts { name: "1x2_a2", size: [7, 8, 15], filled: true, markers: &[] },
    Facts { name: "1x2_a3", size: [7, 8, 15], filled: true, markers: &[(Marker::Warrior, [3, 2, 8])] },
    Facts { name: "1x2_a4", size: [7, 8, 15], filled: true, markers: &[] },
    Facts { name: "1x2_a5", size: [7, 8, 15], filled: true, markers: &[] },
    Facts { name: "1x2_a6", size: [7, 8, 15], filled: true, markers: &[] },
    Facts { name: "1x2_a7", size: [7, 8, 15], filled: true, markers: &[] },
    Facts { name: "1x2_a8", size: [7, 8, 15], filled: true, markers: &[(Marker::Warrior, [2, 1, 2])] },
    Facts { name: "1x2_a9", size: [7, 8, 15], filled: true, markers: &[(Marker::Warrior, [5, 1, 1])] },
    Facts { name: "1x2_b1", size: [7, 8, 15], filled: true, markers: &[(Marker::Warrior, [3, 1, 13])] },
    Facts { name: "1x2_b2", size: [7, 8, 15], filled: true, markers: &[(Marker::Warrior, [5, 1, 11])] },
    Facts {
        name: "1x2_b3",
        size: [7, 8, 15],
        filled: true,
        markers: &[(Marker::ChestSouth, [3, 1, 0]), (Marker::Warrior, [3, 1, 4])],
    },
    Facts { name: "1x2_b4", size: [7, 8, 15], filled: true, markers: &[(Marker::ChestNorth, [4, 3, 9])] },
    Facts { name: "1x2_b5", size: [7, 8, 15], filled: true, markers: &[] },
    Facts { name: "1x2_c1", size: [7, 11, 15], filled: true, markers: &[] },
    Facts { name: "1x2_c2", size: [7, 11, 15], filled: true, markers: &[] },
    Facts {
        name: "1x2_c3",
        size: [7, 11, 15],
        filled: true,
        markers: &[(Marker::Warrior, [4, 1, 6]), (Marker::Warrior, [4, 1, 9]), (Marker::Warrior, [4, 1, 12])],
    },
    Facts { name: "1x2_c4", size: [7, 11, 15], filled: true, markers: &[] },
    Facts { name: "1x2_c_stairs", size: [7, 22, 15], filled: true, markers: &[] },
    Facts { name: "1x2_d1", size: [7, 11, 15], filled: true, markers: &[] },
    Facts { name: "1x2_d2", size: [7, 11, 15], filled: true, markers: &[(Marker::ChestWest, [6, 7, 12])] },
    Facts {
        name: "1x2_d3",
        size: [7, 11, 15],
        filled: true,
        markers: &[(Marker::Warrior, [2, 2, 10]), (Marker::Warrior, [4, 2, 10]), (Marker::Mage, [3, 3, 3])],
    },
    Facts { name: "1x2_d4", size: [7, 11, 15], filled: true, markers: &[] },
    Facts { name: "1x2_d5", size: [7, 11, 15], filled: true, markers: &[] },
    Facts { name: "1x2_d_stairs", size: [7, 22, 15], filled: true, markers: &[] },
    Facts { name: "1x2_s1", size: [7, 8, 15], filled: true, markers: &[(Marker::ChestSouth, [3, 1, 4])] },
    Facts { name: "1x2_s2", size: [7, 8, 15], filled: true, markers: &[] },
    Facts {
        name: "1x2_se1",
        size: [7, 11, 15],
        filled: true,
        markers: &[(Marker::ChestSouth, [3, 9, 1]), (Marker::ChestNorth, [3, 9, 13])],
    },
    Facts {
        name: "2x2_a1",
        size: [15, 8, 15],
        filled: true,
        markers: &[
            (Marker::Warrior, [12, 1, 3]),
            (Marker::Allays, [4, 2, 2]),
            (Marker::Allays, [2, 2, 8]),
            (Marker::Allays, [5, 2, 12]),
            (Marker::Allays, [12, 2, 12]),
        ],
    },
    Facts { name: "2x2_a2", size: [15, 8, 15], filled: true, markers: &[(Marker::Warrior, [9, 1, 2])] },
    Facts { name: "2x2_a3", size: [15, 8, 15], filled: true, markers: &[] },
    Facts { name: "2x2_a4", size: [15, 8, 15], filled: true, markers: &[] },
    Facts {
        name: "2x2_b1",
        size: [15, 11, 15],
        filled: true,
        markers: &[(Marker::Warrior, [4, 1, 4]), (Marker::Mage, [2, 1, 8]), (Marker::Warrior, [8, 1, 9])],
    },
    Facts {
        name: "2x2_b2",
        size: [15, 11, 15],
        filled: true,
        markers: &[(Marker::Warrior, [13, 1, 2]), (Marker::Mage, [2, 1, 7]), (Marker::Warrior, [9, 1, 8])],
    },
    Facts { name: "2x2_b3", size: [15, 11, 15], filled: true, markers: &[] },
    Facts {
        name: "2x2_b4",
        size: [15, 11, 15],
        filled: true,
        markers: &[(Marker::Warrior, [3, 1, 3]), (Marker::Mage, [3, 1, 8]), (Marker::Warrior, [4, 1, 11])],
    },
    Facts { name: "2x2_b5", size: [15, 11, 15], filled: true, markers: &[(Marker::ChestSouth, [7, 7, 0])] },
    Facts { name: "2x2_s1", size: [15, 11, 15], filled: true, markers: &[] },
    Facts { name: "carpet_east", size: [2, 1, 5], filled: false, markers: &[] },
    Facts { name: "carpet_north", size: [5, 1, 2], filled: false, markers: &[] },
    Facts { name: "carpet_south_1", size: [8, 8, 3], filled: false, markers: &[] },
    Facts { name: "carpet_south_2", size: [8, 11, 3], filled: false, markers: &[] },
    Facts { name: "carpet_west_1", size: [3, 8, 8], filled: false, markers: &[] },
    Facts { name: "carpet_west_2", size: [3, 11, 8], filled: false, markers: &[] },
    Facts { name: "corridor_floor", size: [7, 8, 7], filled: true, markers: &[] },
    Facts { name: "entrance", size: [21, 19, 16], filled: false, markers: &[] },
    Facts { name: "indoors_door_1", size: [1, 8, 8], filled: true, markers: &[] },
    Facts { name: "indoors_door_2", size: [1, 11, 8], filled: true, markers: &[] },
    Facts { name: "indoors_wall_1", size: [1, 8, 8], filled: true, markers: &[] },
    Facts { name: "indoors_wall_2", size: [1, 11, 8], filled: true, markers: &[] },
    Facts { name: "roof", size: [8, 1, 8], filled: true, markers: &[] },
    Facts { name: "roof_corner", size: [4, 4, 4], filled: false, markers: &[] },
    Facts { name: "roof_front", size: [4, 4, 8], filled: false, markers: &[] },
    Facts { name: "roof_inner_corner", size: [4, 4, 4], filled: false, markers: &[] },
    Facts { name: "small_wall", size: [2, 4, 8], filled: false, markers: &[] },
    Facts { name: "small_wall_corner", size: [2, 4, 2], filled: true, markers: &[] },
    Facts { name: "wall_corner", size: [9, 8, 2], filled: false, markers: &[] },
    Facts { name: "wall_flat", size: [2, 8, 8], filled: false, markers: &[] },
    Facts { name: "wall_window", size: [2, 8, 8], filled: true, markers: &[] },
];

/// The index of the template named `name` in [`TEMPLATES`].
pub fn template_index(name: &str) -> Option<u8> {
    TEMPLATES.iter().position(|t| t.name == name).map(|i| i as u8)
}

/// A container a template fills with set contents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fixed {
    Spawner(MobKind),
    /// A chest (`trapped` for a trapped chest) and its stacks as
    /// `(slot, item, count, Efficiency level)`.
    Chest {
        trapped: bool,
        items: &'static [(u8, &'static str, u8, u8)],
    },
}

pub struct FixedFacts {
    pub template: &'static str,
    pub pos: [i32; 3],
    pub contents: Fixed,
}

/// `mansions.json`'s `containers`.
pub const FIXED: [FixedFacts; 5] = [
    FixedFacts { template: "1x1_as2", pos: [4, 1, 3], contents: Fixed::Spawner(MobKind::Spider) },
    FixedFacts {
        template: "1x1_b5",
        pos: [6, 1, 6],
        contents: Fixed::Chest {
            trapped: false,
            items: &[
                (8, "allium", 1, 0),
                (10, "allium", 1, 0),
                (11, "allium", 1, 0),
                (12, "allium", 1, 0),
                (14, "allium", 1, 0),
                (15, "allium", 1, 0),
                (16, "allium", 1, 0),
                (18, "allium", 1, 0),
            ],
        },
    },
    FixedFacts {
        template: "1x2_a4",
        pos: [6, 1, 5],
        contents: Fixed::Chest {
            trapped: false,
            items: &[(4, "dark_oak_sapling", 9, 0), (10, "dark_oak_sapling", 10, 0), (23, "dark_oak_sapling", 9, 0)],
        },
    },
    FixedFacts {
        template: "1x2_a6",
        pos: [2, 1, 8],
        contents: Fixed::Chest { trapped: false, items: &[(10, "iron_axe", 1, 1)] },
    },
    FixedFacts {
        template: "1x2_s2",
        pos: [3, 4, 5],
        contents: Fixed::Chest { trapped: true, items: &[(11, "ender_pearl", 1, 0), (15, "ender_pearl", 1, 0)] },
    },
];

/// The chest [`FIXED`]`[i]` holds.
pub fn fixed_chest(i: u8) -> crate::world::chest::Chest {
    let mut chest = crate::world::chest::Chest::default();
    if let Some(FixedFacts { contents: Fixed::Chest { items, .. }, .. }) = FIXED.get(i as usize) {
        for &(slot, id, count, efficiency) in *items {
            let Some(item) = crate::java_registry::item(id, "") else { continue };
            let mut stack = crate::inventory::Stack::new(item, count);
            if efficiency > 0 {
                stack.enchants.set(crate::enchant::Enchantment::Efficiency, efficiency);
            }
            chest.slots[slot as usize] = Some(stack);
        }
    }
    chest
}

/// One stored template cell: a block and its facing in the template frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub block: Block,
    pub facing: Option<Facing>,
    /// An iron door half (`Some(upper)`).
    iron_door: Option<bool>,
    /// The Java registry id the cell was drawn with.
    pub id: &'static str,
}

impl Cell {
    /// The block placed for this cell by a piece turned `rotation` after `mirror`.
    pub fn resolve(self, mirror: Mirror, rotation: Rotation) -> Block {
        let facing = self.facing.map(|f| transform_facing(f, mirror, rotation));
        match (self.iron_door, facing) {
            (Some(upper), f) => crate::world::redstone_blocks::iron_door(f.unwrap_or(Facing::North), false, upper),
            (None, Some(f)) => self.block.with_facing(f),
            (None, None) => self.block,
        }
    }
}

/// An original template: its stored cells (`None` leaves the world as it
/// is) and Java's markers.
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

/// Java ids whose VoxelCraft block names differ from the id.
const NAMES: [(&str, &str); 3] =
    [("diamond_block", "block of diamond"), ("lapis_block", "block of lapis lazuli"), ("vine", "vines")];

/// Ids no block resolved (left as air); `rooms_resolve_every_block` keeps it empty.
static UNRESOLVED: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

fn blk(id: &'static str) -> Block {
    let english = NAMES.iter().find(|n| n.0 == id).map_or("", |n| n.1);
    crate::java_registry::block(id, english).unwrap_or_else(|| {
        log::error!("mansion template block {id} is unknown");
        UNRESOLVED.lock().unwrap().push(id);
        Block::AIR
    })
}

/// Every template's ids that resolved to no block.
#[cfg(test)]
pub fn unresolved() -> Vec<&'static str> {
    for i in 0..TEMPLATES.len() {
        template(i as u8);
    }
    UNRESOLVED.lock().unwrap().clone()
}

/// A block with no facing.
fn c(id: &'static str) -> Cell {
    Cell { block: blk(id), facing: None, iron_door: None, id }
}

/// A block facing `f`.
fn cf(id: &'static str, f: Facing) -> Cell {
    Cell { block: blk(id), facing: Some(f), iron_door: None, id }
}

/// Stairs climbing toward `f` (the game turns stairs by their low side).
fn stairs(id: &'static str, f: Facing) -> Cell {
    cf(id, f.opposite())
}

/// Half of an iron door facing `f`.
fn iron_door(f: Facing, upper: bool) -> Cell {
    Cell { block: Block::AIR, facing: Some(f), iron_door: Some(upper), id: "iron_door" }
}

const AIR: Cell = Cell { block: Block::AIR, facing: None, iron_door: None, id: "air" };
const WATER: Cell = Cell { block: Block::WATER, facing: None, iron_door: None, id: "water" };
const LAVA: Cell = Cell { block: Block::LAVA, facing: None, iron_door: None, id: "lava" };

struct B {
    size: IVec3,
    cells: Vec<Option<Cell>>,
}

impl B {
    fn put(&mut self, (x, y, z): P, cell: Cell) {
        if x < 0 || y < 0 || z < 0 || x >= self.size.x || y >= self.size.y || z >= self.size.z {
            return;
        }
        self.cells[((y * self.size.z + z) * self.size.x + x) as usize] = Some(cell);
    }

    fn void(&mut self, (x, y, z): P) {
        if x < 0 || y < 0 || z < 0 || x >= self.size.x || y >= self.size.y || z >= self.size.z {
            return;
        }
        self.cells[((y * self.size.z + z) * self.size.x + x) as usize] = None;
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

    fn max(&self) -> P {
        (self.size.x - 1, self.size.y - 1, self.size.z - 1)
    }

    /// A floor of birch planks, as every room has.
    fn floor(&mut self) {
        let (mx, _, mz) = self.max();
        self.fill((0, 0, 0), (mx, 0, mz), c("birch_planks"));
    }

    /// The cells of the box's outline at height `y` (x and z edges).
    fn ring(&mut self, y: i32, a: (i32, i32), b: (i32, i32), cell: Cell) {
        for x in a.0..=b.0 {
            self.put((x, y, a.1), cell);
            self.put((x, y, b.1), cell);
        }
        for z in a.1..=b.1 {
            self.put((a.0, y, z), cell);
            self.put((b.0, y, z), cell);
        }
    }

    /// A small table: a fence post under a pressure-free top of `top`.
    fn table(&mut self, (x, z): (i32, i32), top: &'static str) {
        self.put((x, 1, z), c("oak_fence"));
        self.put((x, 2, z), c(top));
    }

    /// A lamp: a dark oak fence post with a torch on it.
    fn lamp(&mut self, (x, z): (i32, i32)) {
        self.put((x, 1, z), c("dark_oak_fence"));
        self.put((x, 2, z), c("torch"));
    }
}

fn build(f: &Facts) -> Template {
    let size = IVec3::from(f.size);
    let fill = if f.filled { Some(AIR) } else { None };
    let mut b = B { size, cells: vec![fill; (size.x * size.y * size.z) as usize] };
    design(f.name, &mut b);
    let markers: Vec<(Marker, IVec3)> = f.markers.iter().map(|&(m, p)| (m, IVec3::from(p))).collect();
    // A marker's own cell is not stored: Java skips the structure block.
    for &(_, p) in &markers {
        b.void((p.x, p.y, p.z));
    }
    Template { size, cells: b.cells, markers }
}

use Facing::{East as E, North as N, South as S, West as W};

fn design(name: &str, b: &mut B) {
    let (mx, my, mz) = b.max();
    match name {
        // Structure: corridors and their carpets.
        "corridor_floor" => {
            b.floor();
            runner(b, (1, 1), (5, 5));
        }
        "carpet_north" => {
            b.fill((1, 0, 0), (3, 0, 1), c("red_carpet"));
            b.put((0, 0, 1), c("white_carpet"));
            b.put((4, 0, 1), c("white_carpet"));
        }
        "carpet_east" => {
            b.fill((0, 0, 1), (1, 0, 3), c("red_carpet"));
            b.put((0, 0, 0), c("white_carpet"));
            b.put((0, 0, 4), c("white_carpet"));
        }
        "carpet_south_1" | "carpet_south_2" => {
            // An arch over the wall line (z 2) between two corridor cells.
            b.fill((1, 1, 0), (mx, my - 1, 2), AIR);
            b.fill((0, 0, 0), (mx, 0, 2), c("birch_planks"));
            b.fill((3, 1, 0), (5, 1, 2), c("red_carpet"));
            for x in [2, 6] {
                b.put((x, 1, 0), c("white_carpet"));
                b.put((x, 1, 2), c("white_carpet"));
            }
            b.fill((0, 1, 2), (0, my - 1, 2), c("dark_oak_planks"));
            b.fill((0, my, 2), (mx, my, 2), c("dark_oak_planks"));
            for x in [1, 7] {
                b.lamp((x, 1));
            }
            if my > 7 {
                b.put((1, my - 1, 2), stairs("dark_oak_stairs", W));
                b.put((7, my - 1, 2), stairs("dark_oak_stairs", E));
            }
        }
        "carpet_west_1" | "carpet_west_2" => {
            b.fill((0, 1, 1), (2, my - 1, mz), AIR);
            b.fill((0, 0, 0), (2, 0, mz), c("birch_planks"));
            b.fill((0, 1, 3), (2, 1, 5), c("red_carpet"));
            for z in [2, 6] {
                b.put((0, 1, z), c("white_carpet"));
                b.put((2, 1, z), c("white_carpet"));
            }
            b.fill((0, 1, 0), (0, my - 1, 0), c("dark_oak_planks"));
            b.fill((0, my, 0), (0, my, mz), c("dark_oak_planks"));
            for z in [1, 7] {
                b.lamp((1, z));
            }
            if my > 7 {
                b.put((0, my - 1, 1), stairs("dark_oak_stairs", N));
                b.put((0, my - 1, 7), stairs("dark_oak_stairs", S));
            }
        }
        "indoors_wall_1" | "indoors_wall_2" | "indoors_door_1" | "indoors_door_2" => {
            b.fill((0, 1, 0), (0, my, 6), c("dark_oak_planks"));
            b.fill((0, 1, 7), (0, my, 7), c("dark_oak_log"));
            b.fill((0, 0, 0), (0, 0, 7), c("birch_planks"));
            if name.contains("door") {
                b.fill((0, 1, 2), (0, 3, 4), AIR);
                b.put((0, 4, 3), AIR);
                b.put((0, 4, 2), stairs("dark_oak_stairs", N));
                b.put((0, 4, 4), stairs("dark_oak_stairs", S));
            }
        }
        // The outer walls, x 0 on the wall line and x 1 outside.
        "wall_flat" | "wall_window" => {
            b.fill((0, 0, 0), (1, 0, 7), c("cobblestone"));
            b.fill((0, 1, 0), (0, 7, 6), c("dark_oak_planks"));
            b.fill((0, 1, 7), (1, 7, 7), c("dark_oak_log"));
            b.fill((1, 1, 0), (1, 1, 6), stairs("cobblestone_stairs", W));
            if name == "wall_window" {
                b.fill((0, 2, 2), (0, 4, 5), c("glass_pane"));
                b.fill((1, 2, 1), (1, 2, 6), AIR);
                b.put((1, 1, 1), c("cobblestone_wall"));
                b.put((1, 1, 6), c("cobblestone_wall"));
                b.fill((1, 5, 2), (1, 5, 5), c("dark_oak_fence"));
                b.fill((1, 6, 2), (1, 6, 5), c("dark_oak_planks"));
            } else {
                b.put((1, 2, 0), c("cobblestone_wall"));
                b.put((1, 2, 6), c("cobblestone_wall"));
                b.fill((1, 6, 0), (1, 6, 6), stairs("dark_oak_stairs", W));
            }
        }
        "wall_corner" => {
            b.fill((7, 0, 0), (8, 6, 1), c("cobblestone"));
            b.fill((7, 7, 0), (8, 7, 1), c("cobblestone_wall"));
        }
        "roof" => b.fill((0, 0, 0), (mx, 0, mz), c("dark_oak_planks")),
        "roof_front" => {
            // An eave sloping down and out over the wall (+x).
            for z in 0..=mz {
                for x in 0..=3 {
                    let y = 3 - x;
                    b.put((x, y, z), stairs("dark_oak_stairs", W));
                    for below in 0..y {
                        b.put((x, below, z), c("dark_oak_planks"));
                    }
                }
                b.put((3, 0, z), stairs("cobblestone_stairs", W));
                b.put((2, 0, z), c("cobblestone"));
            }
        }
        "roof_corner" => {
            for z in 0..=3 {
                for x in 0..=3 {
                    let y = 3 - x.max(z);
                    let top = if x == z {
                        c("dark_oak_planks")
                    } else if x > z {
                        stairs("dark_oak_stairs", W)
                    } else {
                        stairs("dark_oak_stairs", N)
                    };
                    b.put((x, y, z), top);
                    if y == 0 {
                        b.put((x, y, z), c("cobblestone"));
                    }
                }
            }
        }
        "roof_inner_corner" => {
            for z in 0..=3 {
                for x in 0..=3 {
                    let y = 3 - x.min(z);
                    b.put((x, y, z), if x == z { c("dark_oak_planks") } else { stairs("dark_oak_stairs", W) });
                    for below in 0..y {
                        b.put((x, below, z), c("dark_oak_planks"));
                    }
                    if y == 0 {
                        b.put((x, 0, z), c("cobblestone"));
                    }
                }
            }
        }
        "small_wall" => {
            b.fill((0, 0, 0), (0, 3, 6), c("dark_oak_planks"));
            b.fill((0, 0, 7), (0, 3, 7), c("dark_oak_log"));
            b.fill((1, 0, 0), (1, 0, 7), stairs("cobblestone_stairs", W));
            b.fill((1, 1, 0), (1, 3, 7), AIR);
            b.put((1, 1, 0), c("cobblestone"));
            b.put((1, 1, 7), c("cobblestone"));
        }
        "small_wall_corner" => {
            b.fill((0, 0, 0), (1, 2, 1), c("cobblestone"));
            b.fill((0, 3, 0), (1, 3, 1), c("cobblestone_wall"));
        }
        "entrance" => entrance(b),
        _ => room(name, b),
    }
}

/// A 5×5 carpet with a red centre at y 1.
fn runner(b: &mut B, a: (i32, i32), z: (i32, i32)) {
    for zz in a.1..=z.1 {
        for x in a.0..=z.0 {
            let edge = x == a.0 || x == z.0 || zz == a.1 || zz == z.1;
            b.put((x, 1, zz), c(if edge { "white_carpet" } else { "red_carpet" }));
        }
    }
}

/// The entrance hall: the front (+x) porch and door, a double-height
/// hall with a staircase to the second floor, and walls on the sides no
/// room piece closes (x 0 and z 15 west of the corridor).
fn entrance(b: &mut B) {
    // Cells: x 1..=7 and 9..=15, z 0..=6 and 8..=14; walls x 0, 8, 16 and z 7, 15.
    b.fill((0, 0, 0), (16, 0, 15), c("birch_planks"));
    b.fill((1, 0, 0), (15, 18, 15), AIR);
    b.fill((1, 0, 0), (15, 0, 15), c("dark_oak_planks"));
    b.fill((2, 0, 1), (14, 0, 14), c("birch_planks"));
    // The second floor, open over the stairs.
    b.fill((1, 8, 0), (15, 8, 15), c("birch_planks"));
    b.fill((3, 8, 2), (5, 8, 10), AIR);
    for z in 2..=10 {
        b.put((2, 9, z), c("dark_oak_fence"));
        b.put((6, 9, z), c("dark_oak_fence"));
    }
    b.fill((2, 9, 1), (6, 9, 1), c("dark_oak_fence"));
    // Stairs up to the south (z grows as they climb).
    for step in 0..8 {
        let z = 3 + step;
        b.fill((3, step + 1, z), (5, step + 1, z), stairs("cobblestone_stairs", S));
        b.fill((3, 1, z), (5, step, z), c("cobblestone"));
    }
    b.fill((3, 8, 11), (5, 8, 11), c("cobblestone"));
    // Red carpet from the door to the stairs, on both floors.
    b.fill((9, 1, 7), (15, 1, 8), c("red_carpet"));
    b.fill((6, 1, 7), (8, 1, 8), c("red_carpet"));
    b.fill((9, 9, 6), (15, 9, 9), c("red_carpet"));
    b.fill((9, 9, 5), (15, 9, 5), c("white_carpet"));
    b.fill((9, 9, 10), (15, 9, 10), c("white_carpet"));
    // West wall (x 0) and the south wall west of the corridor, both floors.
    b.fill((0, 1, 0), (0, 17, 15), c("dark_oak_planks"));
    b.fill((0, 1, 7), (0, 17, 7), c("dark_oak_log"));
    b.fill((0, 1, 15), (8, 17, 15), c("dark_oak_planks"));
    b.fill((8, 1, 15), (8, 17, 15), c("dark_oak_log"));
    b.fill((0, 18, 0), (16, 18, 15), c("dark_oak_planks"));
    // The facade (x 16) with its door and windows.
    b.fill((16, 0, 0), (16, 17, 15), c("dark_oak_planks"));
    for z in [0, 7, 8, 15] {
        b.fill((16, 0, z), (16, 17, z), c("dark_oak_log"));
    }
    b.fill((16, 0, 0), (16, 0, 15), c("cobblestone"));
    b.fill((16, 1, 6), (16, 4, 9), AIR);
    b.fill((16, 5, 6), (16, 5, 9), c("dark_oak_log"));
    for z0 in [2, 11] {
        b.fill((16, 2, z0), (16, 4, z0 + 2), c("glass_pane"));
        b.fill((16, 10, z0), (16, 13, z0 + 2), c("glass_pane"));
    }
    // The porch.
    b.fill((17, 0, 4), (20, 0, 11), c("cobblestone"));
    b.fill((20, 0, 5), (20, 0, 10), stairs("cobblestone_stairs", W));
    for z in [4, 11] {
        b.fill((17, 1, z), (17, 1, z), c("cobblestone_wall"));
        b.fill((19, 1, z), (19, 5, z), c("dark_oak_log"));
        b.put((19, 6, z), c("dark_oak_planks"));
    }
    b.fill((17, 6, 4), (20, 6, 11), c("dark_oak_planks"));
    b.fill((20, 7, 4), (20, 7, 11), stairs("dark_oak_stairs", W));
    b.fill((17, 7, 4), (19, 7, 11), c("cobblestone_slab"));
    b.put((18, 5, 5), c("torch"));
    b.put((18, 5, 10), c("torch"));
    b.put((18, 1, 4), c("birch_fence"));
    b.put((18, 1, 11), c("birch_fence"));
    // Lamps in the hall.
    for (x, z) in [(10, 2), (14, 2), (10, 13), (14, 13)] {
        b.lamp((x, z));
    }
    b.fill((9, 1, 0), (15, 1, 0), c("white_carpet"));
    // The second floor's lamps.
    b.put((10, 9, 2), c("dark_oak_fence"));
    b.put((10, 10, 2), c("torch"));
    b.put((14, 9, 13), c("dark_oak_fence"));
    b.put((14, 10, 13), c("torch"));
}

/// Pots of the given plants on top of `base` at `(x, z)`, raised `y`.
fn pots(b: &mut B, y: i32, spots: &[((i32, i32), &'static str)]) {
    for &((x, z), pot) in spots {
        b.put((x, y, z), c(pot));
    }
}

/// Empty storage chests along a run of z at `x`, on a plank shelf.
fn chest_rack(b: &mut B, x: i32, y: i32, z: (i32, i32), facing: Facing) {
    for zz in z.0..=z.1 {
        b.put((x, y, zz), cf("chest", facing));
        b.put((x, y + 1, zz), c("oak_slab"));
    }
}

/// A wool statue of `shape` cells (`(x, y, z)` relative to `at`).
fn statue(b: &mut B, at: P, cells: &[(P, &'static str)]) {
    for &((x, y, z), id) in cells {
        b.put((at.0 + x, at.1 + y, at.2 + z), c(id));
    }
}

fn room(name: &str, b: &mut B) {
    let (mx, _, mz) = b.max();
    b.floor();
    match name {
        // First floor, 1×1 (door east, z 2..=4).
        "1x1_a1" => {
            // Flower room: stone planters along north and south.
            for z in [0, 6] {
                b.fill((0, 1, z), (5, 1, z), c("cobblestone"));
                b.fill((0, 2, z), (5, 2, z), c("cobblestone_wall"));
            }
            pots(
                b,
                2,
                &[
                    ((1, 0), "potted_allium"),
                    ((3, 0), "potted_oxeye_daisy"),
                    ((1, 6), "potted_blue_orchid"),
                    ((3, 6), "potted_azure_bluet"),
                ],
            );
            b.fill((0, 1, 2), (0, 1, 4), stairs("cobblestone_stairs", W));
        }
        "1x1_a2" => {
            // Pumpkin shrine: a rail ring round a lit pumpkin, ladders up the back.
            for x in 1..=5 {
                b.put((x, 1, 1), c("rail"));
                b.put((x, 1, 5), c("rail"));
            }
            for z in 2..=4 {
                b.put((1, 1, z), c("rail"));
                b.put((5, 1, z), c("rail"));
            }
            b.fill((2, 1, 2), (4, 1, 4), c("cobblestone"));
            b.fill((3, 2, 3), (3, 2, 3), stairs("dark_oak_stairs", W));
            b.put((3, 3, 3), cf("carved_pumpkin", E));
            b.fill((0, 1, 1), (0, 6, 5), cf("ladder", E));
            b.fill((0, 7, 0), (6, 7, 6), stairs("cobblestone_stairs", W));
        }
        "1x1_a3" => {
            // Tea room: a table with chairs, tulips and carpet.
            b.fill((1, 1, 1), (5, 1, 5), c("light_gray_carpet"));
            b.put((2, 1, 3), c("oak_planks"));
            b.put((2, 2, 3), c("potted_red_tulip"));
            b.put((1, 1, 3), stairs("oak_stairs", W));
            b.put((2, 1, 2), stairs("oak_stairs", N));
            b.put((2, 1, 4), stairs("oak_stairs", S));
            b.fill((0, 1, 0), (0, 2, 0), c("cobblestone"));
            b.fill((0, 1, 6), (0, 2, 6), c("cobblestone"));
            pots(b, 3, &[((0, 0), "potted_red_tulip"), ((0, 6), "potted_red_tulip")]);
        }
        "1x1_a4" => {
            // Study with a loft over the door holding a chest.
            for (x, z) in [(0, 0), (0, 6), (6, 0), (6, 6)] {
                b.fill((x, 1, z), (x, 7, z), c("dark_oak_log"));
            }
            b.fill((4, 5, 1), (6, 5, 5), c("dark_oak_planks"));
            b.fill((3, 6, 1), (3, 6, 5), c("oak_fence"));
            b.fill((1, 1, 1), (5, 1, 5), c("gray_carpet"));
            b.fill((2, 1, 2), (4, 1, 4), c("light_gray_carpet"));
            b.fill((0, 1, 1), (0, 4, 5), c("oak_fence"));
            b.fill((3, 5, 1), (3, 5, 1), c("oak_fence"));
        }
        "1x1_a5" => {
            // Hearth room: a cobblestone fireplace and benches.
            b.fill((0, 1, 1), (1, 4, 5), c("cobblestone"));
            b.fill((1, 1, 2), (1, 2, 4), AIR);
            b.put((0, 1, 3), c("dark_oak_planks"));
            b.fill((1, 5, 1), (1, 5, 5), c("cobblestone_slab"));
            b.fill((4, 1, 1), (4, 1, 5), stairs("dark_oak_stairs", E));
            b.put((5, 1, 0), c("dark_oak_planks"));
            b.put((5, 2, 0), c("potted_white_tulip"));
        }
        // Secret 1×1 rooms (no door).
        "1x1_as1" => {
            b.fill((0, 1, 0), (mx, 6, mz), c("cobblestone"));
            b.fill((2, 1, 1), (5, 3, 2), AIR);
            b.fill((1, 1, 1), (1, 2, 1), AIR);
        }
        "1x1_as2" => {
            for y in 1..=6 {
                for z in 0..=mz {
                    for x in 0..=mx {
                        if (x * 3 + y * 5 + z * 7) % 4 != 0 {
                            b.put((x, y, z), c("cobweb"));
                        }
                    }
                }
            }
            b.put((4, 1, 3), c("spawner"));
        }
        "1x1_as3" => {
            b.fill((2, 1, 2), (4, 3, 4), c("obsidian"));
            b.fill((3, 2, 2), (3, 2, 4), AIR);
            b.fill((2, 2, 3), (4, 2, 3), AIR);
            b.put((3, 2, 3), c("diamond_block"));
        }
        "1x1_as4" => {
            for (x, z) in [(0, 0), (0, 6), (6, 0), (6, 6), (3, 0), (3, 6), (0, 3), (6, 3)] {
                b.fill((x, 1, z), (x, 6, z), c("birch_planks"));
            }
            b.fill((0, 7, 0), (6, 7, 6), c("birch_planks"));
        }
        // Upper floors, 1×1.
        "1x1_b1" => {
            // Sitting room: birch benches round a slab table.
            b.fill((1, 1, 1), (1, 1, 5), stairs("birch_stairs", W));
            b.fill((2, 1, 1), (4, 1, 1), stairs("birch_stairs", N));
            b.fill((2, 1, 5), (4, 1, 5), stairs("birch_stairs", S));
            b.put((3, 1, 3), c("birch_fence"));
            b.put((3, 2, 3), c("birch_slab"));
            b.put((3, 3, 3), c("potted_azure_bluet"));
            b.put((5, 1, 3), c("pink_carpet"));
            b.fill((0, 9, 0), (6, 10, 6), c("birch_planks"));
            b.fill((1, 9, 1), (5, 9, 5), AIR);
        }
        "1x1_b2" => {
            // Writing room: a desk with a lamp.
            b.fill((1, 1, 1), (5, 1, 5), c("light_gray_carpet"));
            b.fill((1, 1, 2), (1, 1, 4), c("dark_oak_planks"));
            b.fill((1, 2, 2), (1, 2, 4), c("dark_oak_planks"));
            b.put((2, 1, 3), stairs("oak_stairs", E));
            b.put((1, 3, 2), c("dark_oak_fence"));
            b.put((1, 4, 2), c("torch"));
            b.put((0, 1, 0), c("black_carpet"));
            b.put((0, 1, 6), c("black_carpet"));
            b.fill((0, 1, 0), (0, 3, 0), c("oak_planks"));
        }
        "1x1_b3" => {
            // Wool bed corner under a little tree.
            b.fill((1, 1, 1), (2, 1, 2), c("red_wool"));
            b.fill((1, 1, 1), (1, 1, 2), c("white_wool"));
            b.fill((0, 1, 4), (0, 3, 4), c("dark_oak_fence"));
            b.put((0, 4, 4), c("dark_oak_leaves"));
            b.put((0, 4, 5), c("oak_slab"));
            b.fill((4, 1, 0), (6, 1, 0), c("cobblestone"));
            b.fill((4, 2, 0), (6, 2, 0), c("cobblestone_slab"));
        }
        "1x1_b4" => {
            // Reading nook: bookshelves on two walls.
            b.fill((0, 1, 0), (0, 3, 6), c("bookshelf"));
            b.fill((0, 1, 0), (4, 3, 0), c("bookshelf"));
            b.put((2, 1, 3), stairs("dark_oak_stairs", W));
            b.put((3, 1, 3), c("oak_planks"));
            b.put((3, 2, 3), c("potted_allium"));
            b.fill((0, 4, 0), (4, 4, 0), c("oak_slab"));
        }
        "1x1_b5" => {
            // Greenhouse: hanging planters, a water cauldron, a chest of alliums.
            b.fill((0, 1, 0), (0, 4, 6), c("oak_fence"));
            b.fill((0, 5, 0), (6, 5, 0), c("oak_planks"));
            pots(b, 1, &[((1, 1), "potted_allium"), ((1, 5), "potted_allium"), ((3, 1), "potted_allium")]);
            b.put((1, 1, 3), c("water_cauldron"));
            b.put((0, 4, 3), cf("vine", E));
            b.put((6, 1, 6), cf("chest", W));
        }
        // First floor, side-entrance 1×2 (door east, z 2..=4).
        "1x2_a1" => {
            // Guard room: cobblestone benches and a banner.
            b.fill((0, 1, 6), (1, 1, 14), c("cobblestone"));
            b.fill((0, 2, 6), (0, 2, 14), c("cobblestone_slab"));
            b.fill((5, 1, 9), (6, 1, 14), stairs("cobblestone_stairs", E));
            b.put((0, 3, 10), cf("gray_wall_banner", E));
        }
        "1x2_a2" => {
            // Wheat farm on either side of a lily-pad channel.
            b.fill((0, 0, 6), (6, 0, 14), c("farmland"));
            b.fill((3, 0, 6), (3, 0, 14), WATER);
            b.fill((3, 1, 6), (3, 1, 14), c("lily_pad"));
            for x in [0, 1, 2, 4, 5, 6] {
                b.fill((x, 1, 6), (x, 1, 14), c("wheat"));
            }
            for (x, z) in [(0, 5), (6, 5)] {
                b.lamp((x, z));
            }
            b.put((5, 1, 3), c("white_carpet"));
        }
        "1x2_a3" => {
            // Smithy: an andesite floor, a lava hearth and an anvil on a dais.
            b.fill((0, 0, 0), (6, 0, 14), c("polished_andesite"));
            b.fill((2, 1, 7), (4, 1, 9), c("polished_andesite"));
            b.fill((0, 1, 11), (6, 2, 14), c("polished_andesite"));
            b.fill((2, 2, 12), (3, 2, 12), LAVA);
            b.fill((0, 3, 11), (6, 3, 14), c("smooth_stone_slab"));
            b.fill((0, 1, 0), (1, 1, 6), c("smooth_stone_slab"));
            b.put((5, 1, 8), c("damaged_anvil"));
        }
        "1x2_a4" => {
            // Sapling nursery with a loft reached by ladder.
            for x in [0, 2, 4] {
                b.fill((x, 1, 6), (x, 1, 14), c("oak_planks"));
                b.fill((x, 0, 7), (x, 0, 13), c("dirt"));
                b.fill((x, 1, 7), (x, 1, 13), c("dark_oak_sapling"));
            }
            b.fill((1, 1, 6), (1, 1, 14), c("green_carpet"));
            b.fill((3, 1, 6), (3, 1, 14), c("green_carpet"));
            b.fill((6, 1, 8), (6, 5, 8), cf("ladder", W));
            b.fill((4, 6, 9), (6, 6, 14), c("oak_planks"));
            b.put((6, 1, 5), cf("chest", W));
        }
        "1x2_a5" => {
            // Wool mural of waves along the west wall.
            for z in 0..=14i32 {
                let crest = 2 + ((z % 6) - 3).abs() / 2;
                for y in 1..=6 {
                    let id = if y < crest {
                        "blue_wool"
                    } else if y == crest {
                        "cyan_wool"
                    } else {
                        "light_blue_wool"
                    };
                    b.put((0, y, z), c(id));
                }
            }
            b.fill((1, 1, 6), (1, 1, 14), c("light_blue_wool"));
        }
        "1x2_a6" => {
            // An indoor dark oak tree; a chest of tools beside it.
            b.fill((2, 0, 9), (4, 0, 13), c("dirt"));
            b.fill((3, 1, 10), (3, 4, 12), c("dark_oak_log"));
            b.fill((1, 5, 8), (5, 6, 14), c("dark_oak_leaves"));
            b.fill((3, 5, 10), (3, 6, 12), c("dark_oak_log"));
            b.fill((0, 7, 9), (6, 7, 13), c("dark_oak_planks"));
            b.put((2, 1, 8), cf("chest", N));
        }
        "1x2_a7" => {
            // Mushroom cellar: coarse dirt beds on plank shelves.
            for x in [0, 5] {
                b.fill((x, 1, 6), (x + 1, 1, 14), c("oak_planks"));
                b.fill((x, 2, 6), (x + 1, 2, 14), c("coarse_dirt"));
                for z in (6..=14).step_by(2) {
                    b.put((x, 3, z), c(if z % 4 == 0 { "red_mushroom" } else { "brown_mushroom" }));
                }
            }
            b.fill((2, 1, 14), (4, 1, 14), c("oak_fence"));
            b.put((3, 1, 13), cf("chest", N));
        }
        "1x2_a8" => {
            // Pumpkin and melon patch round a water line.
            b.fill((0, 0, 6), (6, 0, 14), c("farmland"));
            b.fill((3, 0, 6), (3, 0, 14), WATER);
            for z in (6..=14).step_by(2) {
                b.put((2, 1, z), cf("attached_pumpkin_stem", W));
                b.put((1, 1, z), c("pumpkin"));
                b.put((4, 1, z), cf("attached_melon_stem", E));
                b.put((5, 1, z), c("melon"));
            }
            b.lamp((0, 4));
        }
        "1x2_a9" => {
            // Storeroom: racks of empty chests.
            for x in [0, 3] {
                chest_rack(b, x, 2, (3, 11), if x == 0 { E } else { W });
                b.fill((x, 1, 3), (x, 1, 11), c("oak_planks"));
            }
            b.fill((0, 4, 3), (0, 4, 11), cf("chest", E));
            b.fill((0, 5, 3), (0, 5, 11), c("oak_slab"));
        }
        // First floor, front-entrance 1×2 (door south, x 2..=4).
        "1x2_b1" | "1x2_b2" => {
            // A cell behind iron doors, opened by a lever.
            b.fill((0, 1, 0), (6, 4, 6), c("cobblestone"));
            b.fill((1, 1, 1), (5, 3, 5), AIR);
            b.fill((1, 1, 1), (5, 1, 5), c("brown_carpet"));
            b.put((3, 1, 6), iron_door(S, false));
            b.put((3, 2, 6), iron_door(S, true));
            b.put((1, 1, 2), c("cauldron"));
            b.fill((4, 1, 7), (4, 1, 9), c("redstone_wire"));
            b.put((4, 1, 10), c("cobblestone"));
            b.put((4, 2, 10), cf("lever", S));
            b.fill((0, 1, 7), (0, 2, 14), c("cobblestone_wall"));
            if name == "1x2_b1" {
                b.fill((6, 1, 7), (6, 1, 12), stairs("cobblestone_stairs", E));
            } else {
                b.fill((6, 1, 7), (6, 1, 9), stairs("oak_stairs", E));
            }
        }
        "1x2_b3" => {
            // Log hall: pillars down both sides, a chest at the far end.
            for z in (1..=13).step_by(3) {
                b.fill((0, 1, z), (0, 7, z), c("dark_oak_log"));
                b.fill((6, 1, z), (6, 7, z), c("dark_oak_log"));
            }
            b.fill((0, 7, 0), (6, 7, 14), c("dark_oak_log"));
            b.fill((1, 7, 1), (5, 7, 13), AIR);
        }
        "1x2_b4" => {
            // A stage of dark oak with slab steps, a chest on its back shelf.
            b.fill((0, 1, 3), (6, 2, 12), c("dark_oak_planks"));
            b.fill((0, 1, 13), (6, 1, 13), c("oak_slab"));
            b.fill((1, 3, 3), (5, 6, 4), c("dark_oak_planks"));
            b.fill((2, 3, 4), (4, 4, 4), AIR);
            b.fill((0, 7, 0), (6, 7, 2), c("oak_slab"));
            b.fill((1, 3, 9), (1, 3, 9), c("oak_slab"));
        }
        "1x2_b5" => {
            // A sheep statue in grey wool.
            statue(
                b,
                (2, 1, 4),
                &[
                    ((0, 0, 0), "gray_wool"),
                    ((2, 0, 0), "gray_wool"),
                    ((0, 0, 4), "gray_wool"),
                    ((2, 0, 4), "gray_wool"),
                ],
            );
            b.fill((2, 1, 4), (4, 3, 8), c("light_gray_wool"));
            b.fill((3, 1, 5), (3, 1, 7), AIR);
            b.fill((2, 2, 3), (4, 4, 3), c("white_wool"));
            b.put((2, 3, 2), c("black_wool"));
            b.put((4, 3, 2), c("black_wool"));
            b.put((3, 2, 2), c("green_wool"));
            b.fill((2, 1, 9), (4, 1, 9), c("dark_oak_planks"));
        }
        // Upper floors, side-entrance 1×2 (door east, z 2..=4).
        "1x2_c1" => {
            // Bedroom: a canopy bed of wool, a desk, carpet.
            b.fill((1, 1, 1), (5, 1, 13), c("white_carpet"));
            b.fill((0, 1, 10), (1, 1, 13), c("white_carpet"));
            b.fill((0, 1, 11), (1, 1, 12), c("oak_planks"));
            b.fill((0, 2, 11), (1, 2, 12), c("oak_slab"));
            for (x, z) in [(0, 10), (0, 13), (2, 10), (2, 13)] {
                b.fill((x, 1, z), (x, 3, z), c("dark_oak_fence"));
            }
            b.fill((0, 4, 10), (2, 4, 13), c("oak_slab"));
            b.put((5, 1, 12), stairs("oak_stairs", E));
            b.put((6, 1, 12), c("oak_planks"));
            b.put((6, 2, 12), c("potted_oxeye_daisy"));
            b.lamp((6, 7));
            b.put((1, 1, 6), c("gray_carpet"));
        }
        "1x2_c2" | "1x2_c3" => {
            // Twin beds of wool under coloured carpet.
            let (carpet, pot) = if name == "1x2_c2" {
                ("purple_carpet", "potted_allium")
            } else {
                ("blue_carpet", "potted_azure_bluet")
            };
            b.fill((1, 1, 1), (5, 1, 13), c(carpet));
            for z in [1, 13] {
                b.fill((0, 1, z), (2, 1, z), c("white_wool"));
                b.put((0, 2, z), c("white_wool"));
            }
            b.put((6, 1, 13), c("oak_planks"));
            b.put((6, 2, 13), c(pot));
            b.put((0, 1, 7), stairs("oak_stairs", W));
            if name == "1x2_c2" {
                b.fill((2, 1, 6), (2, 1, 8), c("pink_carpet"));
            } else {
                b.fill((5, 1, 6), (5, 1, 12), c("light_blue_carpet"));
            }
        }
        "1x2_c4" => {
            // Library: shelves along the west wall, reading desks.
            b.fill((0, 1, 0), (0, 4, 14), c("bookshelf"));
            b.fill((0, 1, 6), (0, 2, 8), AIR);
            for z in [4, 10] {
                b.table((3, z), "oak_planks");
                b.put((2, 1, z), stairs("oak_stairs", W));
                b.put((4, 1, z), stairs("oak_stairs", E));
            }
            pots(b, 3, &[((3, 4), "potted_oxeye_daisy"), ((3, 10), "potted_red_tulip")]);
            b.put((6, 1, 13), c("oak_planks"));
            pots(b, 2, &[((6, 13), "potted_poppy")]);
            b.put((6, 1, 7), c("oak_planks"));
            pots(b, 2, &[((6, 7), "potted_dandelion")]);
        }
        // Upper floors, front-entrance 1×2 (door south, x 2..=4).
        "1x2_d1" => {
            // Audience room: banners on the walls, a bookshelf, a carpet aisle.
            b.fill((2, 1, 0), (4, 1, 14), c("red_carpet"));
            for z in [3, 7, 11] {
                b.put((0, 4, z), cf("light_gray_wall_banner", E));
            }
            b.fill((5, 1, 1), (6, 3, 1), c("bookshelf"));
            b.fill((0, 1, 0), (1, 1, 1), c("white_wool"));
            b.put((6, 1, 4), c("oak_planks"));
            b.put((6, 2, 4), c("potted_oxeye_daisy"));
            b.put((6, 1, 8), c("oak_planks"));
            b.put((6, 2, 8), c("potted_birch_sapling"));
            b.put((1, 1, 5), stairs("oak_stairs", W));
        }
        "1x2_d2" => {
            // Bedroom with a ladder to a loft (the chest sits on the loft).
            b.fill((4, 6, 9), (6, 6, 14), c("oak_planks"));
            b.fill((3, 7, 9), (3, 7, 14), c("oak_fence"));
            b.fill((4, 1, 9), (4, 5, 9), c("dark_oak_log"));
            b.fill((4, 1, 8), (4, 5, 8), cf("ladder", N));
            b.fill((0, 1, 1), (2, 1, 2), c("white_wool"));
            b.fill((0, 1, 5), (2, 1, 6), c("white_wool"));
            b.fill((3, 1, 1), (5, 1, 6), c("red_carpet"));
            b.fill((4, 7, 10), (6, 7, 11), c("cyan_carpet"));
            b.put((1, 1, 13), c("light_blue_carpet"));
            b.put((0, 1, 9), c("oak_planks"));
            b.put((0, 2, 9), c("potted_red_tulip"));
        }
        "1x2_d3" => {
            // Illager altar: a raised dais and a pedestal for the evoker.
            b.fill((0, 1, 9), (6, 1, 11), c("cobblestone"));
            b.fill((0, 1, 12), (6, 1, 12), stairs("cobblestone_stairs", N));
            b.fill((2, 1, 2), (4, 2, 4), c("cobblestone"));
            b.fill((2, 3, 2), (4, 3, 4), AIR);
            b.fill((1, 1, 1), (5, 1, 1), stairs("cobblestone_stairs", S));
            b.fill((2, 2, 9), (4, 2, 9), c("white_carpet"));
            b.put((1, 2, 10), c("black_carpet"));
            for x in [1, 5] {
                b.put((x, 4, 0), cf("black_wall_banner", S));
            }
            b.fill((0, 1, 0), (0, 5, 0), c("cobblestone_wall"));
            b.fill((6, 1, 0), (6, 5, 0), c("cobblestone_wall"));
        }
        "1x2_d4" => {
            // A spider statue in black wool with white eyes.
            b.fill((1, 1, 4), (5, 3, 10), c("black_wool"));
            b.fill((2, 1, 5), (4, 2, 9), AIR);
            for z in [4, 6, 8, 10] {
                b.put((0, 1, z), c("black_wool"));
                b.put((6, 1, z), c("black_wool"));
            }
            b.put((2, 3, 3), c("white_wool"));
            b.put((4, 3, 3), c("white_wool"));
            b.put((3, 2, 3), c("lime_wool"));
        }
        "1x2_d5" => {
            // A chicken statue in white wool with an orange beak.
            b.fill((2, 1, 5), (4, 4, 9), c("white_wool"));
            b.fill((2, 5, 4), (4, 7, 6), c("white_wool"));
            b.fill((3, 5, 3), (3, 5, 3), c("orange_wool"));
            b.fill((2, 1, 9), (2, 1, 9), c("orange_wool"));
            b.fill((4, 1, 9), (4, 1, 9), c("orange_wool"));
            b.put((2, 6, 3), c("gray_wool"));
            b.put((4, 6, 3), c("gray_wool"));
        }
        "1x2_c_stairs" | "1x2_d_stairs" => stairs_room(name, b),
        // Secret 1×2 rooms.
        "1x2_s1" => {
            b.fill((0, 1, 0), (mx, 6, mz), c("birch_planks"));
            b.fill((2, 1, 2), (4, 3, 12), AIR);
            b.lamp((2, 10));
            b.lamp((4, 10));
            b.put((2, 1, 7), c("dark_oak_fence"));
            b.put((2, 2, 7), c("torch"));
        }
        "1x2_s2" => {
            // A trap: the chest of pearls sits over TNT behind false stone.
            b.fill((0, 1, 0), (mx, 6, mz), c("cobblestone"));
            b.fill((1, 1, 2), (5, 3, 12), AIR);
            b.fill((2, 1, 4), (4, 3, 6), c("cobblestone"));
            b.fill((3, 1, 5), (3, 2, 5), c("tnt"));
            b.put((3, 4, 5), cf("trapped_chest", S));
            b.fill((1, 1, 10), (5, 1, 12), c("green_wool"));
            b.fill((1, 1, 2), (5, 1, 3), c("orange_wool"));
            b.fill((0, 1, 7), (0, 1, 9), stairs("cobblestone_stairs", W));
        }
        "1x2_se1" => {
            // A hidden loft room: chests on a gallery up under the roof.
            b.fill((0, 1, 0), (mx, 8, mz), c("dark_oak_planks"));
            b.fill((1, 1, 1), (5, 7, 13), AIR);
            for z in [1, 13] {
                b.fill((0, 1, z), (0, 8, z), c("dark_oak_log"));
                b.fill((6, 1, z), (6, 8, z), c("dark_oak_log"));
            }
            b.fill((1, 8, 0), (5, 8, 14), c("dark_oak_planks"));
            b.fill((2, 8, 2), (4, 8, 12), AIR);
            b.fill((1, 1, 6), (1, 7, 6), stairs("dark_oak_stairs", N));
        }
        // First floor, 2×2 (door east, z 2..=4, x 14).
        "2x2_a1" => jail(b),
        "2x2_a2" => {
            // Great storeroom: four racks of empty chests, a carpet aisle.
            for x in [0, 6, 8, 14] {
                b.fill((x, 1, 5), (x, 1, 13), c("oak_planks"));
                chest_rack(b, x, 2, (5, 13), if x == 0 || x == 8 { E } else { W });
            }
            b.fill((9, 1, 5), (13, 1, 13), c("red_carpet"));
            b.fill((1, 1, 5), (5, 1, 13), c("red_carpet"));
            pots(b, 1, &[((1, 1), "potted_blue_orchid"), ((5, 1), "potted_blue_orchid")]);
        }
        "2x2_a3" => {
            // A giant's head in grey wool with lapis eyes.
            b.fill((3, 1, 3), (11, 7, 11), c("light_gray_wool"));
            b.fill((4, 1, 4), (10, 6, 10), AIR);
            b.fill((5, 4, 2), (5, 4, 2), c("lapis_block"));
            b.fill((9, 4, 2), (9, 4, 2), c("lapis_block"));
            b.fill((5, 4, 3), (5, 4, 3), c("black_wool"));
            b.fill((9, 4, 3), (9, 4, 3), c("black_wool"));
            b.fill((6, 2, 3), (8, 2, 3), c("brown_wool"));
            b.fill((3, 7, 3), (11, 7, 4), c("gray_wool"));
            b.fill((7, 3, 2), (7, 3, 2), c("white_wool"));
            b.put((3, 1, 2), c("yellow_wool"));
            b.put((11, 1, 2), c("green_wool"));
        }
        "2x2_a4" => {
            // Indoor pond with a bush island and stone paths.
            b.fill((2, 0, 2), (12, 0, 12), c("polished_andesite"));
            b.fill((3, 0, 3), (11, 0, 11), WATER);
            b.fill((6, 0, 6), (8, 0, 8), c("dirt"));
            b.put((7, 1, 7), c("dark_oak_log"));
            b.put((7, 2, 7), c("dark_oak_log"));
            b.fill((6, 3, 6), (8, 3, 8), c("dark_oak_leaves"));
            b.fill((6, 1, 6), (8, 2, 8), c("dark_oak_leaves"));
            b.fill((7, 1, 7), (7, 2, 7), c("dark_oak_log"));
            b.fill((7, 0, 3), (7, 0, 5), c("smooth_stone_slab"));
            b.fill((9, 0, 7), (13, 0, 7), c("smooth_stone_slab"));
            b.ring(1, (2, 2), (12, 12), c("smooth_stone_slab"));
            b.fill((12, 1, 6), (12, 1, 8), AIR);
            b.fill((7, 1, 2), (7, 1, 2), AIR);
        }
        // Upper floors, 2×2.
        "2x2_b1" | "2x2_b2" | "2x2_b4" => council(name, b),
        "2x2_b3" => {
            // Grand library: shelves round the walls and in stacks.
            b.fill((0, 1, 0), (0, 6, 14), c("bookshelf"));
            b.fill((0, 1, 0), (14, 6, 0), c("bookshelf"));
            b.fill((0, 1, 14), (10, 6, 14), c("bookshelf"));
            for x in [3, 7, 11] {
                b.fill((x, 1, 4), (x, 4, 11), c("bookshelf"));
                b.fill((x, 5, 4), (x, 5, 11), c("oak_slab"));
            }
            b.fill((0, 7, 0), (14, 7, 14), c("oak_planks"));
            b.fill((2, 7, 2), (12, 7, 12), AIR);
            b.fill((13, 1, 9), (13, 1, 12), stairs("oak_stairs", S));
            b.fill((1, 1, 2), (13, 1, 2), stairs("oak_stairs", N));
        }
        "2x2_b5" => {
            // Hay loft: a gallery reached by ladder, fenced pens below.
            b.fill((0, 6, 0), (14, 6, 6), c("oak_planks"));
            b.fill((0, 7, 7), (14, 7, 7), c("oak_fence"));
            b.fill((6, 7, 7), (8, 7, 7), AIR);
            b.fill((7, 1, 6), (7, 5, 6), c("dark_oak_log"));
            b.fill((7, 1, 7), (7, 5, 7), cf("ladder", S));
            for x in [3, 7, 11] {
                b.fill((x, 1, 9), (x, 2, 14), c("dark_oak_fence"));
            }
            b.fill((1, 1, 9), (13, 1, 9), c("dark_oak_fence"));
            b.put((5, 1, 9), cf("dark_oak_fence_gate", S));
            b.fill((0, 1, 0), (5, 1, 4), c("smooth_stone_slab"));
        }
        "2x2_s1" => {
            // Treasure vault: a diamond block on obsidian over a lava pool under glass.
            b.fill((1, 1, 1), (13, 1, 13), c("obsidian"));
            b.fill((2, 1, 2), (12, 1, 12), LAVA);
            b.fill((2, 2, 2), (12, 2, 12), c("glass"));
            b.fill((6, 2, 6), (8, 4, 8), c("obsidian"));
            b.put((7, 5, 7), c("diamond_block"));
            b.fill((0, 1, 0), (14, 7, 0), c("obsidian"));
            b.fill((0, 1, 14), (14, 7, 14), c("obsidian"));
        }
        _ => panic!("no mansion room design for {name}"),
    }
}

/// The stairs room under the third floor: a flight from the second floor
/// (layer 0) up to the third (layer 11), a landing and its carpet.
fn stairs_room(name: &str, b: &mut B) {
    // Front-entrance rooms open south, side-entrance ones east, so the
    // flight climbs north from the door end or along the west wall.
    let front = name == "1x2_d_stairs";
    b.fill((0, 11, 0), (6, 11, 14), c("birch_planks"));
    for step in 0..11 {
        let z = if front { 12 - step } else { 1 + step };
        b.fill((1, step + 1, z), (2, step + 1, z), stairs("cobblestone_stairs", if front { N } else { S }));
        b.fill((1, 1, z), (2, step, z), c("cobblestone"));
        b.fill((1, 11, z), (2, 11, z), AIR);
    }
    let (lo, hi) = if front { (2, 12) } else { (1, 11) };
    b.fill((3, 12, lo), (3, 12, hi), c("cobblestone_wall"));
    b.fill((4, 1, 1), (5, 1, 13), c("red_carpet"));
    b.fill((3, 12, 0), (6, 12, 0), c("red_carpet"));
    // A wool tapestry on the stairwell wall.
    b.fill((0, 4, 3), (0, 8, 11), c("light_gray_wool"));
    b.fill((0, 5, 5), (0, 7, 9), c("gray_wool"));
    b.put((0, 6, 7), c("black_wool"));
    b.put((0, 6, 6), c("white_wool"));
    b.put((0, 6, 8), c("green_wool"));
    b.fill((4, 12, 4), (5, 12, 10), c("red_carpet"));
}

/// The allays' jail: four barred cells on raised floors and a guard post
/// by the door.
fn jail(b: &mut B) {
    b.fill((0, 1, 0), (14, 6, 14), c("cobblestone"));
    // The guard room and corridor (door at x 14, z 2..=4).
    b.fill((8, 1, 1), (14, 4, 5), AIR);
    b.fill((1, 1, 6), (13, 4, 8), AIR);
    b.fill((8, 1, 1), (13, 1, 5), c("brown_carpet"));
    // Cells around the markers: (4, 2, 2), (2, 2, 8), (5, 2, 12), (12, 2, 12).
    let cells = [((2, 1), (6, 4)), ((1, 7), (3, 9)), ((3, 10), (7, 13)), ((10, 10), (13, 13))];
    for ((x0, z0), (x1, z1)) in cells {
        b.fill((x0, 1, z0), (x1, 1, z1), c("cobblestone_slab"));
        b.fill((x0, 2, z0), (x1, 4, z1), AIR);
    }
    // Bars and iron doors into the corridor.
    b.fill((2, 2, 5), (6, 3, 5), c("iron_bars"));
    b.put((4, 2, 5), iron_door(S, false));
    b.put((4, 3, 5), iron_door(S, true));
    b.fill((3, 2, 10), (7, 3, 10), c("iron_bars"));
    b.put((5, 2, 10), iron_door(N, false));
    b.put((5, 3, 10), iron_door(N, true));
    b.fill((10, 2, 10), (13, 3, 10), c("iron_bars"));
    b.put((12, 2, 10), iron_door(N, false));
    b.put((12, 3, 10), iron_door(N, true));
    b.fill((1, 1, 6), (1, 1, 6), c("cobblestone"));
    // The guard's cauldrons, levers and banner.
    for (x, z) in [(9, 1), (13, 5), (2, 6), (13, 6)] {
        b.put((x, 1, z), c("cauldron"));
    }
    for (x, z) in [(6, 6), (8, 9), (11, 9), (3, 6)] {
        b.put((x, 2, z), c("polished_andesite"));
        b.put((x, 3, z), cf("lever", S));
    }
    b.put((8, 3, 1), cf("gray_wall_banner", S));
    b.fill((0, 7, 0), (14, 7, 14), c("cobblestone_wall"));
    b.fill((1, 7, 1), (13, 7, 13), AIR);
}

/// Council rooms: a long table with chairs where the evoker sits among
/// vindicators.
fn council(name: &str, b: &mut B) {
    let (carpet, plant) = match name {
        "2x2_b1" => ("magenta_carpet", "potted_birch_sapling"),
        "2x2_b2" => ("white_carpet", "potted_white_tulip"),
        _ => ("lime_carpet", "potted_oxeye_daisy"),
    };
    b.fill((1, 1, 1), (13, 1, 13), c("white_carpet"));
    b.fill((5, 1, 5), (9, 1, 9), c(carpet));
    // The table runs along z through the middle of the west cells.
    for z in 3..=11 {
        b.table((6, z), "oak_planks");
    }
    for z in (4..=10).step_by(2) {
        b.put((5, 1, z), stairs("oak_stairs", W));
        b.put((7, 1, z), stairs("oak_stairs", E));
    }
    for (x, z) in [(1, 1), (13, 13), (1, 13), (13, 6)] {
        b.lamp((x, z));
    }
    pots(b, 3, &[((6, 3), plant), ((6, 11), plant)]);
    if name == "2x2_b4" {
        // A map room: shelves on the north wall and coloured rugs.
        b.fill((0, 1, 0), (14, 3, 0), c("bookshelf"));
        b.fill((10, 1, 9), (12, 1, 12), c("blue_carpet"));
        b.fill((10, 1, 1), (12, 1, 3), c("yellow_carpet"));
    }
}
