//! v1.0 gap track 6e: the explicit face UVs of Java 26.3's door, trapdoor
//! and glass pane models, as the world mesh draws them.
//!
//! Java maps a face without explicit UVs by `FaceBakery.defaultFaceUV`,
//! which the shader's planar `face_uv` already is (track 6d). These models
//! give faces explicit `uv` rectangles and `rotation`s, and their
//! blockstates turn them without `uvlock`, so a face's texture follows the
//! model: a door's edges show the strip at the texture's left columns, a
//! trapdoor's rims its bottom rows upside down, a pane's top and bottom its
//! edge texture's middle columns, and an orientable trapdoor's broad faces
//! turn with its facing.
//!
//! Every such face's Java UVs are the planar ones under one of the eight
//! symmetries of the square (`M`) plus an offset in texels, so a face's
//! *op* is a byte: bits 0-2 the symmetry (identity, mirror u, mirror v,
//! half turn, transpose, quarter turns and anti-transpose; the shader's
//! `JAVA_UV_M`), bits 3-5 the offset (`JAVA_UV_OFFSET`), bit 6 set when the
//! face lies on the cell's edge. The tables come from the jar's models by
//! `tools/extract_java_model_uvs.py`, keyed by Java's blockstate and the
//! face's world-space rectangle (sixteenths), so a face the game draws but
//! Java's model lacks keeps the default.
//!
//! In a detail quad (see the module docs of [`crate::mesh`]) a face on the
//! cell's edge stores its symmetry in the plane code (17 + `M`; the depth
//! is the edge the face points to) and its offset in the three spare bound
//! bits; any other face stores its symmetry there with no offset (the
//! tables hold no inner face with one).

use crate::world::block::{Block, Facing, Shaped};

#[derive(Clone, Copy, Debug)]
pub(super) struct Op {
    face: u8,
    lo: [u8; 3],
    hi: [u8; 3],
    op: u8,
}

const fn op(face: u8, lo: [u8; 3], hi: [u8; 3], op: u8) -> Op {
    Op { face, lo, hi, op }
}

include!("java_uv_tables.rs");

/// The op's symmetry, `JAVA_UV_M`'s index.
pub(super) const fn symmetry(op: u8) -> u32 {
    (op & 7) as u32
}
/// The op's offset, `JAVA_UV_OFFSET`'s index.
pub(super) const fn offset(op: u8) -> u32 {
    (op >> 3 & 7) as u32
}
/// Whether the op's face lies on the cell's edge.
pub(super) const fn on_edge(op: u8) -> bool {
    op & 64 != 0
}

/// Java's facing index in the tables (south, north, east, west).
pub(super) fn facing_index(f: Facing) -> usize {
    match f {
        Facing::South => 0,
        Facing::North => 1,
        Facing::East => 2,
        Facing::West => 3,
    }
}

/// The door's row of [`DOORS`]: Java's facing (the game's facing is its
/// opposite, see `shape::open_door_side`), half, hinge and open. An upper
/// half reads its facing, open state and hinge from the lower half below.
fn door_row(b: Block, below: Block) -> Option<usize> {
    let Some(Shaped::Door { facing, open, upper }) = b.shaped() else { return None };
    let (facing, open, hinge_right) = if upper {
        let (facing, open) = match below.shaped() {
            Some(Shaped::Door { facing, open, upper: false }) => (facing, open),
            _ => (facing, open),
        };
        (facing, open, crate::world::mount_states::is_right_hinge(below))
    } else {
        (facing, open, crate::world::mount_states::is_right_hinge(b))
    };
    let java = facing.clockwise().clockwise();
    Some(facing_index(java) * 8 + usize::from(upper) * 4 + usize::from(hinge_right) * 2 + usize::from(open))
}

/// Java's wood trapdoors whose models turn with their facing
/// (`template_orientable_trapdoor_*`); oak, dark oak, iron and copper
/// trapdoors use `template_trapdoor_*`, whose closed models don't turn.
pub(super) fn orientable_trapdoor(b: Block) -> bool {
    const ORIENTABLE: [&str; 11] = [
        "acacia", "bamboo", "birch", "cherry", "crimson", "jungle", "mangrove", "pale oak", "poplar", "spruce",
        "warped",
    ];
    let name = b.canonical().name();
    ORIENTABLE.iter().any(|w| name.strip_prefix(w).is_some_and(|rest| rest.starts_with(' ')))
}

/// The trapdoor's table and row: Java's facing (the game's), half, open.
fn trapdoor_row(b: Block) -> Option<(&'static [&'static [Op]; 16], usize)> {
    let Some(crate::world::redstone_blocks::Component::Trapdoor { facing, open, top, .. }) =
        crate::world::redstone_blocks::component(b)
    else {
        return None;
    };
    let table = if orientable_trapdoor(b) { &ORIENTABLE_TRAPDOORS } else { &TRAPDOORS };
    Some((table, facing_index(facing) * 4 + usize::from(top) * 2 + usize::from(open)))
}

/// The Java UV op of the face `face` (0 +X .. 5 -Z) of `b` covering the
/// box rectangle `lo`..`hi` (sixteenths, flat on the face's axis); 0 (the
/// default UVs) for any face Java's model maps by default. `below` is the
/// block under `b` (a door's upper half reads it).
pub(super) fn face_op(b: Block, below: Block, face: usize, lo: [u8; 3], hi: [u8; 3]) -> u8 {
    let ops: &[Op] = if let Some(row) = door_row(b, below) {
        DOORS[row]
    } else if let Some((table, row)) = trapdoor_row(b) {
        table[row]
    } else if b.canonical().is_glass_pane() {
        &PANES
    } else {
        return 0;
    };
    ops.iter().find(|o| usize::from(o.face) == face && o.lo == lo && o.hi == hi).map_or(0, |o| o.op)
}

/// Whether `b` is one of the families [`face_op`] maps.
pub(super) fn has_ops(b: Block) -> bool {
    b.is_door()
        || matches!(
            crate::world::redstone_blocks::component(b),
            Some(crate::world::redstone_blocks::Component::Trapdoor { .. })
        )
        || b.canonical().is_glass_pane()
}
