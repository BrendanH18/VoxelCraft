//! The chiseled bookshelf (Java 26.3 `ChiseledBookShelfBlock`,
//! `ChiseledBookShelfBlockEntity`): six book slots in two rows of three on
//! its front, each with a `slot_N_occupied` state.
//!
//! Ids `19328 + 64 * facing + slots`, `facing` in Java's order
//! ([`super::FACINGS`]: north first, the default state) and
//! bit `i` of `slots` set while slot `i` (0 top left, as seen from the
//! front) holds a book.

use glam::{DVec3, IVec3};

use super::super::block::{Block, Facing, RenderKind};
use super::Props;
use crate::item::{Item, ToolKind};

pub const FIRST: u16 = 19328;
pub const END: u16 = FIRST + 256;
pub const CHISELED_BOOKSHELF: Block = Block(FIRST);

pub const fn state(facing: Facing, slots: u8) -> Block {
    Block(FIRST + super::facing_index(facing) * 64 + (slots & 63) as u16)
}

/// The facing and filled slots of a chiseled bookshelf state.
pub const fn shelf(b: Block) -> Option<(Facing, u8)> {
    if b.0 < FIRST || b.0 >= END {
        return None;
    }
    let i = b.0 - FIRST;
    Some((super::FACINGS[(i / 64) as usize], (i % 64) as u8))
}

pub const fn slots(b: Block) -> Option<u8> {
    match shelf(b) {
        Some((_, s)) => Some(s),
        None => None,
    }
}

/// Swaps slots 0 and 2, and 3 and 5: the front texture of a face whose
/// planar texture runs right to left (+X and -Z, see `chunk.wgsl`).
const fn mirror(slots: u8) -> u8 {
    let mut out = slots & 0b010010;
    out |= (slots & 1) << 2 | (slots >> 2) & 1;
    out |= (slots & 8) << 2 | (slots >> 2) & 8;
    out
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some((facing, slots)) = shelf(Block(id)) else { return None };
    let shown = if matches!(facing, Facing::East | Facing::North) { mirror(slots) } else { slots };
    let mut t = [super::tex::CHISELED_SIDE; 6];
    t[2] = super::tex::CHISELED_TOP;
    t[3] = super::tex::CHISELED_TOP;
    t[facing.face()] = super::tex::CHISELED_FRONTS + shown as u16;
    Some(("chiseled bookshelf", RenderKind::Opaque, t))
}

pub fn base(id: u16) -> Option<Block> {
    shelf(Block(id)).map(|_| CHISELED_BOOKSHELF)
}

/// Java: strength 1.5, axe; drops only with silk touch (its books spill
/// from the block entity); not flammable, but lava ignites it.
pub fn props(b: Block) -> Option<Props> {
    shelf(b)?;
    Some(Props { hardness: 1.5, tool: Some(ToolKind::Axe), fire: (0, 0), lava: true, drop: None })
}

pub fn silk_drop(b: Block) -> Option<Item> {
    shelf(b).map(|_| Item::from_block(CHISELED_BOOKSHELF))
}

pub fn with_facing(b: Block, facing: Facing) -> Option<Block> {
    shelf(b).map(|(_, slots)| state(facing, slots))
}

/// Java's `getHitSlot` with two rows of three: the slot under a click on
/// the front face.
pub fn hit_slot(facing: Facing, cell: IVec3, face: IVec3, point: DVec3) -> Option<usize> {
    if Facing::from_offset(face) != Some(facing) {
        return None;
    }
    let local = point - (cell + face).as_dvec3();
    let x = match facing {
        Facing::North => 1.0 - local.x,
        Facing::South => local.x,
        Facing::West => local.z,
        Facing::East => 1.0 - local.z,
    };
    let section = |v: f64, n: f64| ((v * 16.0 / (16.0 / n)).floor() as i64).clamp(0, n as i64 - 1) as usize;
    Some(section(x, 3.0) + section(1.0 - local.y, 2.0) * 3)
}

/// Java's `#bookshelf_books` among VoxelCraft's items.
pub fn is_book(item: Item) -> bool {
    matches!(item, Item::BOOK | Item::ENCHANTED_BOOK)
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [FIRST].into_iter()
}
