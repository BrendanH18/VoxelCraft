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

/// v1.0 gap track 6f: one face of a Java model drawn whole from the jar's
/// element layout (bars, hoppers, end rods, lanterns): the face, its flat rect in
/// sixteenths, the symmetry (`JAVA_UV_M`'s index) and whole-texel offset
/// taking the planar UVs to Java's, its texture slot (the model's texture
/// variables in the order [`model_layer`] reads them), its cullface turned
/// with the model (a face index, 6 for none) and whether it is one of
/// Java's planes just inside the cell's edge (0.001 or 15.999) facing into
/// the cell. Such faces are detail quads with plane code 29, lit flat (see
/// [`crate::mesh`]): every one of these models is drawn without ambient
/// occlusion in Java, by its `ambientocclusion` or its light emission.
#[derive(Clone, Copy, Debug)]
pub(super) struct ModelFace {
    pub(super) face: u8,
    pub(super) lo: [u8; 3],
    pub(super) hi: [u8; 3],
    pub(super) sym: u8,
    pub(super) offset: [u8; 2],
    pub(super) tex: u8,
    pub(super) cull: u8,
    pub(super) inset: bool,
}

#[allow(clippy::too_many_arguments)]
const fn mf(face: u8, lo: [u8; 3], hi: [u8; 3], sym: u8, offset: [u8; 2], tex: u8, cull: u8, inset: bool) -> ModelFace {
    ModelFace { face, lo, hi, sym, offset, tex, cull, inset }
}

include!("java_uv_tables.rs");

/// v1.0 gap track 6f: whether `b` (canonical) is iron or copper bars, whose
/// mesh is Java's `template_bars_*` layout ([`BARS`]) rather than boxes.
pub(super) fn is_bars(b: Block) -> bool {
    matches!(b.shaped(), Some(Shaped::Pane)) && !b.is_glass_pane()
}

/// v1.0 gap track 6f: the faces of Java's model for `b` (canonical), with
/// a bars block's sides joined as `joined` says, when the game draws its
/// whole layout from the jar ([`ModelFace`]): iron and copper bars
/// (`template_bars_*`), hoppers (`hopper`, `hopper_side`), end rods
/// (`end_rod`) and lanterns (`template_lantern`, `template_hanging_lantern`,
/// without the handle's turned planes, which the game's quads can't draw).
pub(super) fn model_faces(b: Block, joined: impl Fn(Facing) -> bool) -> Option<&'static [ModelFace]> {
    if is_bars(b) {
        let bits = [Facing::North, Facing::East, Facing::South, Facing::West]
            .into_iter()
            .enumerate()
            .fold(0, |acc, (i, f)| acc | usize::from(joined(f)) << i);
        return Some(BARS[bits]);
    }
    if let Some(crate::world::redstone_blocks::Component::Hopper { facing, .. }) =
        crate::world::redstone_blocks::component(b)
    {
        // Rows: down, north, south, east, west; the game's facing 4 is down.
        let row = match facing {
            4 => 0,
            f => match Facing::ALL[usize::from(f)] {
                Facing::North => 1,
                Facing::South => 2,
                Facing::East => 3,
                Facing::West => 4,
            },
        };
        return Some(HOPPERS[row]);
    }
    if let Some(dir) = crate::world::end_blocks::rod_facing(b) {
        return Some(END_RODS[dir as usize]);
    }
    if let Some(hanging) = lantern_hanging(b) {
        return Some(LANTERNS[usize::from(hanging)]);
    }
    None
}

/// v1.0 gap track 6f: whether a lantern (iron, soul or copper) hangs;
/// `None` for anything else.
pub(super) fn lantern_hanging(b: Block) -> Option<bool> {
    if let Some(l) = crate::world::mount_states::lantern_of(b) {
        return Some(l.hanging);
    }
    match crate::world::copper::state(b) {
        Some(c) if c.family == crate::world::copper::Family::Lantern => Some(c.local >= 2),
        _ => None,
    }
}

/// v1.0 gap track 6f: the texture layer of a [`ModelFace`]'s slot on `b`,
/// given the layer the block's face would use (`layer`): the bars' `#bars`
/// and `#edge` are both the bars texture; a hopper's `#side`, `#top` and
/// `#inside` are its outside, rim and inside textures; an end rod's one
/// texture is the rod's layer, drawn in Java's `end_rod` layout; a
/// lantern's its own layer, drawn in Java's lantern layout.
pub(super) fn model_layer(b: Block, tex: u8, layer: u32) -> u32 {
    if is_bars(b) {
        return layer;
    }
    if crate::world::end_blocks::is_end_rod(b) {
        return u32::from(crate::world::end_blocks::ROD_LAYER);
    }
    match tex {
        1 => u32::from(crate::content::item_models::HOPPER_TOP),
        2 => u32::from(crate::content::item_models::HOPPER_INSIDE),
        _ => layer,
    }
}

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

/// v1.0 gap track 6f: the Java UV transform (symmetry and texel offset)
/// of a face of `b`'s item model, `None` for the default UVs. A trapdoor's
/// item is the unturned `*_trapdoor_bottom` block model (the blockstate's
/// north-facing, bottom, closed row, whatever the item's state); an end
/// rod's the unturned `end_rod` (facing up). Door and pane items are flat
/// sprites.
pub(super) fn item_face_transform(b: Block, face: usize, lo: [u8; 3], hi: [u8; 3]) -> Option<(u32, [f32; 2])> {
    if let Some((table, _)) = trapdoor_row(b) {
        let row = facing_index(Facing::North) * 4;
        let op = table[row].iter().find(|o| usize::from(o.face) == face && o.lo == lo && o.hi == hi)?.op;
        let o = OFFSET[offset(op) as usize];
        return Some((symmetry(op), o));
    }
    if crate::world::end_blocks::is_end_rod(b) {
        let up = crate::world::end_blocks::Dir::Up as usize;
        let f = END_RODS[up].iter().find(|m| usize::from(m.face) == face && m.lo == lo && m.hi == hi)?;
        return Some((u32::from(f.sym), f.offset.map(f32::from)));
    }
    None
}

/// The shader's `JAVA_UV_M` rows (identity, mirror u, mirror v, half turn,
/// transpose, the quarter turns, anti-transpose).
const M: [[f32; 4]; 8] = [
    [1., 0., 0., 1.],
    [-1., 0., 0., 1.],
    [1., 0., 0., -1.],
    [-1., 0., 0., -1.],
    [0., 1., 1., 0.],
    [0., -1., 1., 0.],
    [0., 1., -1., 0.],
    [0., -1., -1., 0.],
];
/// The shader's `JAVA_UV_OFFSET` texels.
const OFFSET: [[f32; 2]; 8] = [[0., 0.], [3., 0.], [13., 0.], [0., 3.], [0., 13.], [0., 7.], [0., 9.], [1., 0.]];

/// v1.0 gap track 6f: a symmetry and texel offset applied to a planar UV
/// (block units), as the shader's `java_uv` does.
pub(super) fn apply(sym: u32, offset: [f32; 2], uv: [f32; 2]) -> [f32; 2] {
    let [a, b, c, d] = M[sym as usize];
    [a * uv[0] + b * uv[1] + offset[0] / 16.0, c * uv[0] + d * uv[1] + offset[1] / 16.0]
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
