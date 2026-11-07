//! Java 26.3 `HeavyCoreBlock` (v0.16 Track D): an 8×8×8 core resting on the
//! floor, waterloggable, the mace's base. `Blocks.HEAVY_CORE`: hardness 10,
//! explosion resistance 1200, `PushReaction.PUSH_PULL`, metal map colour,
//! snare instrument, mineable with a pickaxe (`#mineable/pickaxe`) but not
//! needing one to drop (no `requiresCorrectToolForDrops`), and its loot
//! table drops itself when it survives an explosion. Not pathfindable.
//!
//! State ids 15744 (dry) and 15745 (waterlogged) come from Track D's
//! reservation 15744–15807; texture layers 3856–3858 from 3856–3951.
//! The art is original.
use super::{
    block::{Block, RenderKind},
    shape::{Box16, Boxes},
};

pub const FIRST: u16 = 15744;
pub const HEAVY_CORE: Block = Block(FIRST);
pub const WATERLOGGED: Block = Block(FIRST + 1);
pub const LAST: u16 = FIRST + 1;

pub const TEX: u16 = 3856;
const TOP: u16 = TEX;
const SIDE: u16 = TEX + 1;
const BOTTOM: u16 = TEX + 2;
pub const TEX_END: u16 = TEX + 3;
// Inside Track D's reservations: states 15744–15807, layers 3856–3951.
const _: () = assert!(FIRST >= 15744 && LAST <= 15807 && TEX >= 3856 && TEX_END <= 3952);

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// The other half of the waterlogged pair, and whether `id` is the wet one.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if owns(id) { Some((id ^ 1, id == LAST)) } else { None }
}

pub const fn registry(_id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    ("heavy core", RenderKind::Shaped, [SIDE, SIDE, TOP, BOTTOM, SIDE, SIDE])
}

/// `Block.column(8.0, 0.0, 8.0)`: the shape, outline and collision.
pub fn boxes() -> Boxes {
    Boxes::from_box(Box16 { min: [4, 0, 4], max: [12, 8, 12] })
}

pub const HARDNESS: f32 = 10.0;
pub const BLAST_RESISTANCE: f32 = 1200.0;

/// Original texture: dark riveted iron with a cold blue seam. The block
/// shows the middle 8×8 of each 16×16 layer.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let (x, y) = (x as i32, y as i32);
    let edge = !(5..=10).contains(&x) || !(5..=10).contains(&y);
    let rivet = matches!((x, y), (5, 5) | (10, 5) | (5, 10) | (10, 10));
    let grain = ((x * 7 + y * 13 + layer as i32 * 5) % 11) as u8;
    let c: [u8; 3] = match layer {
        TOP | BOTTOM if !edge && (x == 7 || x == 8) && (y == 7 || y == 8) => [118, 156, 196],
        SIDE if y == 7 || y == 8 => [92, 118, 150],
        _ if rivet => [146, 146, 154],
        _ if edge => [52, 52, 60],
        _ => [70 + grain, 70 + grain, 80 + grain],
    };
    [c[0], c[1], c[2], 255]
}
