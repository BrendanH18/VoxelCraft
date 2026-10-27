//! The straw bed: Java 26.3 `StrawBedBlock`, a two-cell bed of straw with
//! its own `straw_bed_rule` (sleep when dark but never set a respawn point,
//! and fall apart on waking; in the Nether and the End it falls apart when
//! used instead of exploding).

use super::super::block::{Block, Facing};
use super::super::shape::{Box16, Boxes};
use super::STRAW_BED_ORIGIN;

/// The item's state: the foot, facing south.
pub const ITEM_BLOCK: Block = Block(STRAW_BED_ORIGIN);

/// A half of a straw bed. `facing` is Java's `FACING`: from the foot toward
/// the head.
pub const fn half(facing: Facing, head: bool) -> Block {
    Block(STRAW_BED_ORIGIN + head as u16 * 4 + facing as u16)
}

pub const fn decode(id: u16) -> Option<(Facing, bool)> {
    if id < STRAW_BED_ORIGIN || id >= STRAW_BED_ORIGIN + 8 {
        return None;
    }
    let i = id - STRAW_BED_ORIGIN;
    Some((Facing::ALL[(i % 4) as usize], i >= 4))
}

/// The other half's cell offset from this half.
pub fn partner_offset(facing: Facing, head: bool) -> glam::IVec3 {
    if head { -facing.offset() } else { facing.offset() }
}

/// `box` drawn for a bed facing south, turned to face `facing`.
fn turned(min: [u8; 3], max: [u8; 3], facing: Facing) -> Box16 {
    let (x0, z0, x1, z1) = match facing {
        Facing::South => (min[0], min[2], max[0], max[2]),
        Facing::North => (16 - max[0], 16 - max[2], 16 - min[0], 16 - min[2]),
        // South (+Z) turned to east (+X): (x, z) -> (z, 16 - x).
        Facing::East => (min[2], 16 - max[0], max[2], 16 - min[0]),
        Facing::West => (16 - max[2], min[0], 16 - min[2], max[0]),
    };
    Box16 { min: [x0, min[1], z0], max: [x1, max[1], z1] }
}

/// `StrawBedBlock.getShape`: a 4-high mattress; the head adds a 5-high
/// pillow on its far half. These are also the model's main boxes (the
/// loose straw strands of Java's model are left out).
pub fn boxes(facing: Facing, head: bool) -> Boxes {
    if !head {
        return Boxes::from_box(Box16 { min: [0; 3], max: [16, 4, 16] });
    }
    let mut out = Boxes::from_box(turned([0, 0, 0], [16, 4, 8], facing));
    out.push(turned([0, 0, 8], [16, 5, 16], facing));
    out
}

/// The head cell of the straw bed whose foot is at `foot`, if it is whole.
pub fn head_of(world: &super::super::World, foot: glam::IVec3) -> Option<glam::IVec3> {
    let (facing, false) = decode(world.get_block(foot)?.0)? else { return None };
    let head = foot + facing.offset();
    (world.get_block(head) == Some(half(facing, true))).then_some(head)
}
