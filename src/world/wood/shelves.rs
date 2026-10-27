//! Shelves of the twelve woods (Java 26.3 `ShelfBlock`, `ShelfBlockEntity`,
//! `SideChainPartBlock`, `SelectableSlotContainer`): three item slots taken
//! one held stack at a time, and powered shelves chained side by side (up
//! to three) that swap their slots with the hotbar.
//!
//! Each [`Kind`] has 64 ids from `18560 + 64 * kind`:
//! `32 * waterlogged + 16 * powered + 4 * side_chain + facing`, with
//! `facing` in Java's order ([`super::FACINGS`]: north, south, west, east)
//! and `side_chain` in Java's order (unconnected, right, centre, left).
//! The default state, the item's, faces north as in Java.

use glam::{DVec3, IVec3};

use super::super::block::{Block, Facing, RenderKind};
use super::super::shape::{Box16, Boxes};
use super::{Kind, Props};
use crate::item::{Item, ToolKind};

pub const FIRST: u16 = 18560;
pub const STRIDE: u16 = 64;
pub const END: u16 = FIRST + STRIDE * 12;
/// Java's `getMaxChainLength`.
pub const MAX_CHAIN: usize = 3;

/// Java's `SideChainPart`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chain {
    Unconnected,
    /// The right end: chained to the shelf on its left.
    Right,
    Centre,
    /// The left end: chained to the shelf on its right.
    Left,
}

impl Chain {
    const ALL: [Chain; 4] = [Chain::Unconnected, Chain::Right, Chain::Centre, Chain::Left];

    pub const fn connected(self) -> bool {
        !matches!(self, Chain::Unconnected)
    }
    /// `isConnectionTowards`.
    pub const fn towards(self, end: Chain) -> bool {
        matches!(self, Chain::Centre) || self as u8 == end as u8
    }
    /// `isChainEnd`.
    pub const fn chain_end(self) -> bool {
        !matches!(self, Chain::Centre)
    }
    pub const fn connect_right(self) -> Chain {
        match self {
            Chain::Unconnected | Chain::Left => Chain::Left,
            Chain::Right | Chain::Centre => Chain::Centre,
        }
    }
    pub const fn connect_left(self) -> Chain {
        match self {
            Chain::Unconnected | Chain::Right => Chain::Right,
            Chain::Centre | Chain::Left => Chain::Centre,
        }
    }
    pub const fn disconnect_right(self) -> Chain {
        match self {
            Chain::Unconnected | Chain::Left => Chain::Unconnected,
            Chain::Right | Chain::Centre => Chain::Right,
        }
    }
    pub const fn disconnect_left(self) -> Chain {
        match self {
            Chain::Unconnected | Chain::Right => Chain::Unconnected,
            Chain::Centre | Chain::Left => Chain::Left,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shelf {
    pub kind: Kind,
    pub facing: Facing,
    pub powered: bool,
    pub chain: Chain,
    pub wet: bool,
}

impl Shelf {
    pub const fn block(self) -> Block {
        Block(
            FIRST
                + self.kind.index() * STRIDE
                + self.wet as u16 * 32
                + self.powered as u16 * 16
                + self.chain as u16 * 4
                + super::facing_index(self.facing),
        )
    }

    /// The shelf's left and right, as seen from its front (Java's
    /// `getClockWise` and `getCounterClockWise` of the facing).
    pub fn left(self) -> IVec3 {
        self.facing.clockwise().offset()
    }
    pub fn right(self) -> IVec3 {
        -self.left()
    }
}

pub const fn shelf(b: Block) -> Option<Shelf> {
    let id = b.0;
    if id < FIRST || id >= END {
        return None;
    }
    let local = (id - FIRST) % STRIDE;
    Some(Shelf {
        kind: Kind::of((id - FIRST) / STRIDE),
        facing: super::FACINGS[(local % 4) as usize],
        chain: Chain::ALL[(local / 4 % 4) as usize],
        powered: local % 32 >= 16,
        wet: local >= 32,
    })
}

/// Faces whose planar texture runs right to left as seen from outside
/// (+X and -Z, see `chunk.wgsl`) show the mirrored left and right fronts.
const fn mirrored(facing: Facing) -> bool {
    matches!(facing, Facing::East | Facing::North)
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some(s) = shelf(Block(id)) else { return None };
    const NAMES: [&str; 12] = [
        "oak shelf",
        "spruce shelf",
        "birch shelf",
        "jungle shelf",
        "acacia shelf",
        "dark oak shelf",
        "mangrove shelf",
        "cherry shelf",
        "pale oak shelf",
        "bamboo shelf",
        "crimson shelf",
        "warped shelf",
    ];
    let k = s.kind.index();
    let variant = if !s.powered {
        0
    } else {
        match s.chain {
            Chain::Unconnected => 1,
            Chain::Centre => 3,
            Chain::Left => {
                if mirrored(s.facing) {
                    4
                } else {
                    2
                }
            }
            Chain::Right => {
                if mirrored(s.facing) {
                    2
                } else {
                    4
                }
            }
        }
    };
    let mut t = [super::tex::SHELF + k; 6];
    t[s.facing.face()] = super::tex::SHELF_FRONTS + k * 5 + variant;
    Some((NAMES[k as usize], RenderKind::Shaped, t))
}

pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    match shelf(Block(id)) {
        Some(s) => Some((Shelf { wet: !s.wet, ..s }.block().0, s.wet)),
        None => None,
    }
}

/// One item per kind: facing north (Java's default), unpowered, unconnected, dry.
pub fn base(id: u16) -> Option<Block> {
    shelf(Block(id))
        .map(|s| Shelf { facing: Facing::North, powered: false, chain: Chain::Unconnected, wet: false, ..s }.block())
}

/// Java: strength 2.0, axe, fire 30/20 and lava for the burning woods.
pub fn props(b: Block) -> Option<Props> {
    let s = shelf(b)?;
    Some(Props {
        hardness: 2.0,
        tool: Some(ToolKind::Axe),
        fire: if s.kind.burns() { (30, 20) } else { (0, 0) },
        lava: s.kind.burns(),
        drop: base(b.0).map(Item::from_block),
    })
}

pub fn with_facing(b: Block, facing: Facing) -> Option<Block> {
    shelf(b).map(|s| Shelf { facing, ..s }.block())
}

/// Java's `SHAPES`: the back board and the top and bottom shelves, drawn here
/// for a shelf facing south.
pub fn boxes(b: Block) -> Option<Boxes> {
    let s = shelf(b)?;
    let mut out = Boxes::new();
    for bx in [
        Box16 { min: [0, 0, 0], max: [16, 16, 3] },
        Box16 { min: [0, 0, 3], max: [16, 4, 5] },
        Box16 { min: [0, 12, 3], max: [16, 16, 5] },
    ] {
        out.push(bx.turned(s.facing));
    }
    Some(out)
}

/// Java's `getHitSlot` with one row of three: the slot under a click on the
/// shelf's front face, 0 on the left as seen from the front.
pub fn hit_slot(s: Shelf, cell: IVec3, face: IVec3, point: DVec3) -> Option<usize> {
    if Facing::from_offset(face) != Some(s.facing) {
        return None;
    }
    let local = point - (cell + face).as_dvec3();
    let x = match s.facing {
        Facing::North => 1.0 - local.x,
        Facing::South => local.x,
        Facing::West => local.z,
        Facing::East => 1.0 - local.z,
    };
    Some(((x * 16.0 / (16.0 / 3.0)).floor() as i64).clamp(0, 2) as usize)
}

/// Where the item in `slot` sits, relative to the cell (Java's
/// `ShelfRenderer`: a quarter-size item a quarter in front of the centre,
/// 5/16 apart), and the yaw it faces in degrees.
pub fn item_anchor(s: Shelf, slot: usize) -> (DVec3, f32) {
    let yaw = super::signs::facing_yaw(s.facing);
    let offset = (slot as f64 - 1.0) * 0.3125;
    // Model x runs to the right as seen from the front; model -z is the back.
    let right = s.right().as_dvec3();
    let front = s.facing.offset().as_dvec3();
    (DVec3::splat(0.5) + right * offset - front * 0.25, yaw)
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    Kind::ALL.into_iter().map(|kind| {
        Shelf { kind, facing: Facing::North, powered: false, chain: Chain::Unconnected, wet: false }.block().0
    })
}

/// Comparator output from behind a shelf: bit `i` set while slot `i` holds
/// an item (Java's `getAnalogOutputSignal`), 0 from any other side.
pub fn comparator_signal(s: Shelf, items: &[Option<crate::inventory::Stack>; 3], toward: IVec3) -> u8 {
    if toward != -s.facing.offset() {
        return 0;
    }
    items.iter().enumerate().filter(|(_, i)| i.is_some()).map(|(n, _)| 1 << n).sum()
}
