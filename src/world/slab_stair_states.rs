//! v0.9 slab types and stair halves, shapes and waterlogged states
//! (BLD-04a/b; Java 26.3 `SlabBlock` and `StairBlock`), ids 4096–7679.
//!
//! Every stair and slab family keeps its existing ids for the states it
//! already had: the four facings of bottom, straight, dry stairs and the
//! bottom, dry slab. The other Java states of the 39 families are laid out
//! here:
//!
//! - 4096..=7059: stairs, 76 ids per family in [`FAMILIES`] order. Within a
//!   family, `local = ((top * 5 + shape) * 2 + wet) * 4 + facing` (facing in
//!   [`Facing::ALL`] order, shape in Java's `StairsShape` order) runs over
//!   0..80; locals 0..4 are the existing ids and local `n` >= 4 is id
//!   `origin + n - 4`.
//! - 7060..=7215: slabs, four ids per family: bottom wet, top dry, top wet
//!   and double (never waterlogged).
//! - 7216..=7679 stay unregistered.
//!
//! A family's states share its name, textures, mining, sounds and item:
//! they drop, and are picked as, the existing bottom slab or south-facing
//! stairs. Facings are the codebase's fronts (`Shaped::Stairs`), so Java's
//! `facing` (the side of the tall half) is the opposite.

use super::block::{Block, Facing, RenderKind, Shaped};
use super::forms;

/// Families laid out in this module's range (4096–7679).
pub const LEGACY_COUNT: usize = 39;
/// Stair and slab families, each with one stairs and one slab block. The
/// families from [`LEGACY_COUNT`] on (v0.12) keep all their states in
/// their own module's range; see [`super::stone_blocks`].
pub const FAMILY_COUNT: usize = LEGACY_COUNT + super::stone_blocks::SHAPES.len();
pub const STAIRS_ORIGIN: u16 = 4096;
const STAIRS_STRIDE: u16 = 76;
pub const SLABS_ORIGIN: u16 = STAIRS_ORIGIN + LEGACY_COUNT as u16 * STAIRS_STRIDE;
const SLABS_STRIDE: u16 = 4;
/// One past the last registered id.
pub const END: u16 = SLABS_ORIGIN + LEGACY_COUNT as u16 * SLABS_STRIDE;
/// Last id of the reserved range (docs/architecture.md, "Release ID reservations").
pub const RESERVED_LAST: u16 = 7679;
const _: () = assert!(END - 1 <= RESERVED_LAST);
// `PACKED` keeps the family in six bits.
const _: () = assert!(FAMILY_COUNT <= 64);

/// One family: its full block and the existing ids of its south-facing
/// bottom straight stairs and bottom slab. A slab-only family's `stairs`
/// is air.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Family {
    pub base: Block,
    pub stairs: Block,
    pub slab: Block,
}

/// Legacy stairs/slab materials (`Block::SLAB_BASES`), the 17 stone forms
/// with stairs (`forms` 0..=12 and 15..=18), the 12 wood forms and the
/// three blackstone sets.
pub const FAMILIES: [Family; FAMILY_COUNT] = {
    let mut out = [Family { base: Block::AIR, stairs: Block::AIR, slab: Block::AIR }; FAMILY_COUNT];
    let mut i = 0;
    while i < FAMILY_COUNT {
        out[i] = match i {
            0..=6 => Family {
                base: Block::SLAB_BASES[i],
                stairs: if i < 6 { Block(112 + i as u16 * 4) } else { Block::STONE_BRICK_STAIRS },
                slab: if i < 6 { Block(106 + i as u16) } else { Block::STONE_BRICK_SLAB },
            },
            7..=23 => {
                let index = if i < 20 { i as u16 - 7 } else { i as u16 - 20 + 15 };
                Family {
                    base: forms::stone_base(index),
                    stairs: forms::stone_id(index, 0),
                    slab: forms::stone_id(index, 4),
                }
            }
            24..=35 => {
                let index = i as u16 - 24;
                Family {
                    base: forms::wood_planks(index),
                    stairs: forms::wood_id(index, 0),
                    slab: forms::wood_id(index, 4),
                }
            }
            36..=38 => {
                let index = i - 36;
                Family {
                    base: super::nether_blocks::SHAPE_BASES[index],
                    stairs: super::nether_blocks::shape_id(index, 0),
                    slab: super::nether_blocks::shape_id(index, 4),
                }
            }
            _ => super::stone_blocks::SHAPES[i - LEGACY_COUNT],
        };
        i += 1;
    }
    out
};

/// Java's `StairsShape`, in its declaration order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StairShape {
    Straight,
    InnerLeft,
    InnerRight,
    OuterLeft,
    OuterRight,
}

impl StairShape {
    pub const ALL: [StairShape; 5] = [
        StairShape::Straight,
        StairShape::InnerLeft,
        StairShape::InnerRight,
        StairShape::OuterLeft,
        StairShape::OuterRight,
    ];
}

/// Java's `SlabType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlabType {
    Top,
    Bottom,
    Double,
}

/// A stairs state. `facing` is the front (the low step's side); `top` is
/// Java's `half=top` (upside down).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stairs {
    pub family: u8,
    pub facing: Facing,
    pub top: bool,
    pub shape: StairShape,
    pub wet: bool,
}

/// A slab state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slab {
    pub family: u8,
    pub kind: SlabType,
    pub wet: bool,
}

/// The family index of a stone `forms` index with stairs and a slab.
const fn stone_family(index: u16) -> u8 {
    if index < 13 { 7 + index as u8 } else { 20 + (index - 15) as u8 }
}

/// Family and facing index of an existing (bottom, straight, dry) stairs id.
const fn legacy_stairs(id: u16) -> Option<(u8, u16)> {
    match id {
        112..=135 => return Some((((id - 112) / 4) as u8, (id - 112) % 4)),
        189..=192 => return Some((6, id - 189)),
        _ => {}
    }
    if let Some(forms::StoneForm::Stairs { index, facing }) = forms::stone_form(id) {
        return Some((stone_family(index), facing as u16));
    }
    if let Some(forms::WoodForm::Stairs { index, facing }) = forms::wood_form(id) {
        return Some((24 + index as u8, facing as u16));
    }
    match super::nether_blocks::form(id) {
        Some((i, f @ 0..=3)) => Some((36 + i as u8, f)),
        _ => None,
    }
}

/// Family of an existing (bottom, dry) slab id.
const fn legacy_slab(id: u16) -> Option<u8> {
    match id {
        106..=111 => return Some((id - 106) as u8),
        193 => return Some(6),
        _ => {}
    }
    if let Some(forms::StoneForm::Slab { index }) = forms::stone_form(id) {
        return Some(stone_family(index));
    }
    if let Some(forms::WoodForm::Slab { index }) = forms::wood_form(id) {
        return Some(24 + index as u8);
    }
    match super::nether_blocks::form(id) {
        Some((i, 4)) => Some(36 + i as u8),
        _ => None,
    }
}

/// The stairs state at `local` (see the module notes) of `family`.
const fn stairs_from_local(family: u8, local: u16) -> Stairs {
    let rest = local / 4;
    Stairs {
        family,
        facing: Facing::ALL[(local % 4) as usize],
        top: rest >= 10,
        shape: StairShape::ALL[(rest / 2 % 5) as usize],
        wet: rest % 2 == 1,
    }
}

/// Decodes any stairs state, existing or new.
pub const fn stairs_const(id: u16) -> Option<Stairs> {
    if id >= STAIRS_ORIGIN && id < SLABS_ORIGIN {
        let i = id - STAIRS_ORIGIN;
        return Some(stairs_from_local((i / STAIRS_STRIDE) as u8, i % STAIRS_STRIDE + 4));
    }
    if let Some((f, local)) = super::stone_blocks::stairs_local(id) {
        return Some(stairs_from_local(LEGACY_COUNT as u8 + f as u8, local));
    }
    match legacy_stairs(id) {
        Some((family, f)) => Some(Stairs {
            family,
            facing: Facing::ALL[f as usize],
            top: false,
            shape: StairShape::Straight,
            wet: false,
        }),
        None => None,
    }
}

/// Decodes any slab state, existing or new.
pub const fn slab_const(id: u16) -> Option<Slab> {
    if id >= SLABS_ORIGIN && id < END {
        let i = id - SLABS_ORIGIN;
        let (kind, wet) = match i % SLABS_STRIDE {
            0 => (SlabType::Bottom, true),
            1 => (SlabType::Top, false),
            2 => (SlabType::Top, true),
            _ => (SlabType::Double, false),
        };
        return Some(Slab { family: (i / SLABS_STRIDE) as u8, kind, wet });
    }
    if let Some((f, local)) = super::stone_blocks::slab_local(id) {
        let (kind, wet) = match local {
            0 => (SlabType::Bottom, false),
            1 => (SlabType::Bottom, true),
            2 => (SlabType::Top, false),
            3 => (SlabType::Top, true),
            _ => (SlabType::Double, false),
        };
        return Some(Slab { family: LEGACY_COUNT as u8 + f as u8, kind, wet });
    }
    match legacy_slab(id) {
        Some(family) => Some(Slab { family, kind: SlabType::Bottom, wet: false }),
        None => None,
    }
}

/// The id of a stairs state.
pub const fn stairs_id(s: Stairs) -> Block {
    let shape = s.shape as u16;
    let local = ((s.top as u16 * 5 + shape) * 2 + s.wet as u16) * 4 + s.facing as u16;
    if s.family as usize >= LEGACY_COUNT {
        return super::stone_blocks::stairs_at(s.family as u16 - LEGACY_COUNT as u16, local);
    }
    if local < 4 {
        Block(FAMILIES[s.family as usize].stairs.0 + local)
    } else {
        Block(STAIRS_ORIGIN + s.family as u16 * STAIRS_STRIDE + local - 4)
    }
}

/// The id of a slab state. A double slab is never waterlogged.
pub const fn slab_id(s: Slab) -> Block {
    if s.family as usize >= LEGACY_COUNT {
        let local = match (s.kind, s.wet) {
            (SlabType::Bottom, false) => 0,
            (SlabType::Bottom, true) => 1,
            (SlabType::Top, false) => 2,
            (SlabType::Top, true) => 3,
            (SlabType::Double, _) => 4,
        };
        return super::stone_blocks::slab_at(s.family as u16 - LEGACY_COUNT as u16, local);
    }
    let local = match (s.kind, s.wet) {
        (SlabType::Bottom, false) => return FAMILIES[s.family as usize].slab,
        (SlabType::Bottom, true) => 0,
        (SlabType::Top, false) => 1,
        (SlabType::Top, true) => 2,
        (SlabType::Double, _) => 3,
    };
    Block(SLABS_ORIGIN + s.family as u16 * SLABS_STRIDE + local)
}

/// Per-state decode for hot paths: 0 for other blocks, else bit 15 (stairs)
/// or 14 (slab), the family in bits 0..6, then the properties.
static PACKED: [u16; super::block::STATE_CAPACITY] = {
    let mut arr = [0; super::block::STATE_CAPACITY];
    let mut i = 0;
    while i < arr.len() {
        arr[i] = if let Some(s) = stairs_const(i as u16) {
            1 << 15
                | s.family as u16
                | (s.facing as u16) << 6
                | (s.top as u16) << 8
                | (s.shape as u16) << 9
                | (s.wet as u16) << 12
        } else if let Some(s) = slab_const(i as u16) {
            1 << 14 | s.family as u16 | (s.kind as u16) << 6 | (s.wet as u16) << 12
        } else {
            0
        };
        i += 1;
    }
    arr
};

/// The stairs state of `block`, if it is stairs.
#[inline]
pub fn stairs(block: Block) -> Option<Stairs> {
    let p = *PACKED.get(block.0 as usize)?;
    (p & 1 << 15 != 0).then(|| Stairs {
        family: (p & 63) as u8,
        facing: Facing::ALL[(p >> 6 & 3) as usize],
        top: p >> 8 & 1 == 1,
        shape: StairShape::ALL[(p >> 9 & 7) as usize],
        wet: p >> 12 & 1 == 1,
    })
}

/// The slab state of `block`, if it is a slab.
#[inline]
pub fn slab(block: Block) -> Option<Slab> {
    let p = *PACKED.get(block.0 as usize)?;
    (p & 1 << 14 != 0).then(|| Slab {
        family: (p & 63) as u8,
        kind: [SlabType::Top, SlabType::Bottom, SlabType::Double][(p >> 6 & 3) as usize],
        wet: p >> 12 & 1 == 1,
    })
}

/// The slab type of `block`, if it is a slab.
#[inline]
pub fn slab_type(block: Block) -> Option<SlabType> {
    slab(block).map(|s| s.kind)
}

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet
/// one, for slabs and stairs. See [`super::waterlog::pair`].
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if let Some(s) = stairs_const(id) {
        let other = stairs_id(Stairs { wet: !s.wet, ..s });
        return Some((other.0, s.wet));
    }
    match slab_const(id) {
        Some(Slab { kind: SlabType::Double, .. }) | None => None,
        Some(s) => Some((slab_id(Slab { wet: !s.wet, ..s }).0, s.wet)),
    }
}

/// For a new id: the existing id whose name and textures it shares, and its
/// render kind. Stairs are shaped; a top slab is a shaped half; a bottom
/// slab stays a low cutout block; a double slab is a full opaque cube.
pub const fn registry(id: u16) -> Option<(u16, RenderKind)> {
    if id < STAIRS_ORIGIN || id >= END {
        return None;
    }
    if let Some(s) = stairs_const(id) {
        return Some((FAMILIES[s.family as usize].stairs.0, RenderKind::Shaped));
    }
    match slab_const(id) {
        Some(s) => Some((
            FAMILIES[s.family as usize].slab.0,
            match s.kind {
                SlabType::Bottom => RenderKind::Cutout,
                SlabType::Top => RenderKind::Shaped,
                SlabType::Double => RenderKind::Opaque,
            },
        )),
        None => None,
    }
}

/// Whether `id` is in this module's range (registered or not) or is one
/// of the v0.12 families' states.
#[inline(always)]
pub const fn owns(id: u16) -> bool {
    (id >= STAIRS_ORIGIN && id <= RESERVED_LAST) || super::stone_blocks::owns_shape(id)
}

/// `Block::shaped` for this module's ids.
pub fn shaped(id: u16) -> Option<Shaped> {
    if let Some(s) = stairs(Block(id)) {
        return Some(Shaped::Stairs(s.facing));
    }
    (slab_type(Block(id)) == Some(SlabType::Top)).then_some(Shaped::Slab)
}

/// Java's `useShapeForLightOcclusion`: every stairs state and every slab
/// but the double one.
pub const fn uses_shape_for_light(id: u16) -> bool {
    if stairs_const(id).is_some() {
        return true;
    }
    match slab_const(id) {
        Some(s) => !matches!(s.kind, SlabType::Double),
        None => false,
    }
}

/// The block a new state's item is (the family's existing stairs or slab);
/// `None` for existing ids and other blocks.
pub fn item_block(id: u16) -> Option<Block> {
    if !(STAIRS_ORIGIN..END).contains(&id) && !super::stone_blocks::owns_shape(id) {
        return None;
    }
    if let Some(s) = stairs(Block(id)) {
        return Some(FAMILIES[s.family as usize].stairs);
    }
    slab(Block(id)).map(|s| FAMILIES[s.family as usize].slab)
}

/// The full block a stairs or slab family is cut from.
pub fn family_base(block: Block) -> Option<Block> {
    stairs(block).map(|s| s.family).or_else(|| slab(block).map(|s| s.family)).map(|f| FAMILIES[f as usize].base)
}

/// Java's `getCounterClockWise`.
const fn counter_clockwise(f: Facing) -> Facing {
    f.clockwise().opposite()
}

/// Java's `StairBlock.getStairsShape`. `at(f)` is the block beside the stairs
/// on side `f` (`None` when unloaded).
pub fn stairs_shape(s: Stairs, at: impl Fn(Facing) -> Option<Block>) -> StairShape {
    // Java's `facing` points at the tall half: our front's opposite.
    let facing = s.facing.opposite();
    let same_half = |b: Option<Block>| b.and_then(stairs).filter(|n| n.top == s.top);
    // `canTakeShape`: the side cell is not stairs facing the same way on the same half.
    let can_take = |side: Facing| same_half(at(side)).is_none_or(|n| n.facing != s.facing);
    if let Some(behind) = same_half(at(facing)) {
        let behind_facing = behind.facing.opposite();
        if behind_facing.along_x() != facing.along_x() && can_take(behind_facing.opposite()) {
            return if behind_facing == counter_clockwise(facing) {
                StairShape::OuterLeft
            } else {
                StairShape::OuterRight
            };
        }
    }
    if let Some(front) = same_half(at(facing.opposite())) {
        let front_facing = front.facing.opposite();
        if front_facing.along_x() != facing.along_x() && can_take(front_facing) {
            return if front_facing == counter_clockwise(facing) {
                StairShape::InnerLeft
            } else {
                StairShape::InnerRight
            };
        }
    }
    StairShape::Straight
}

/// `block` (stairs) with the shape its neighbours give it, as Java's
/// `updateShape` and `updateFromNeighbourShapes` leave it; other blocks
/// unchanged.
pub fn reshaped(block: Block, at: impl Fn(Facing) -> Option<Block>) -> Block {
    match stairs(block) {
        Some(s) => stairs_id(Stairs { shape: stairs_shape(s, at), ..s }),
        None => block,
    }
}

/// Java's `Half` from a placement click: bottom unless the clicked face is
/// the underside or a side clicked above the cell's middle
/// (`getStateForPlacement` of `SlabBlock` and `StairBlock`).
pub fn placed_on_top(face: glam::IVec3, click_y: f64) -> bool {
    face != glam::IVec3::Y && (face == glam::IVec3::NEG_Y || click_y > 0.5)
}

/// `SlabBlock.canBeReplaced` when the click is on the slab itself
/// (`replacingClickedOnBlock`): a single slab of the held kind takes the
/// second half from the face on its open side.
pub fn slab_takes_click(slab_block: Block, held: Block, face: glam::IVec3, click_y: f64) -> bool {
    let (Some(s), Some(h)) = (slab(slab_block), slab(held)) else { return false };
    if s.family != h.family {
        return false;
    }
    let above = click_y > 0.5;
    let horizontal = face.y == 0;
    match s.kind {
        SlabType::Double => false,
        SlabType::Bottom => face == glam::IVec3::Y || (above && horizontal),
        SlabType::Top => face == glam::IVec3::NEG_Y || (!above && horizontal),
    }
}

/// `SlabBlock.canBeReplaced` for a slab cell that wasn't clicked: any single
/// slab of the held kind.
pub fn slab_takes_placement(slab_block: Block, held: Block) -> bool {
    matches!((slab(slab_block), slab(held)), (Some(s), Some(h)) if s.family == h.family && s.kind != SlabType::Double)
}

/// The state Java's `getStateForPlacement` gives a held slab or stairs block
/// (dry; waterlogging from the replaced fluid is applied generically).
/// `replaced` is the block now in the target cell and `at(f)` the block
/// beside it on side `f`. Stairs arrive already facing the player.
pub fn placed(
    block: Block,
    replaced: Block,
    face: glam::IVec3,
    click_y: f64,
    at: impl Fn(Facing) -> Option<Block>,
) -> Block {
    if let Some(s) = slab(block) {
        if slab_takes_placement(replaced, block) {
            return slab_id(Slab { kind: SlabType::Double, wet: false, ..s });
        }
        let kind = if placed_on_top(face, click_y) { SlabType::Top } else { SlabType::Bottom };
        return slab_id(Slab { kind, wet: false, ..s });
    }
    if let Some(s) = stairs(block) {
        let s = Stairs { top: placed_on_top(face, click_y), shape: StairShape::Straight, wet: false, ..s };
        return stairs_id(Stairs { shape: stairs_shape(s, at), ..s });
    }
    block
}

/// `block` turned to face `facing`, keeping its half, shape and water.
pub fn with_facing(block: Block, facing: Facing) -> Option<Block> {
    stairs(block).map(|s| stairs_id(Stairs { facing, ..s }))
}

/// Whether the block has a full, sturdy top face because of its half: top
/// and double slabs, and upside-down stairs of any shape.
pub fn sturdy_top(block: Block) -> bool {
    stairs(block).is_some_and(|s| s.top) || matches!(slab_type(block), Some(SlabType::Top | SlabType::Double))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fits_the_reservation_and_round_trips() {
        assert_eq!(END, 7216);
        let mut seen = std::collections::HashSet::new();
        for (f, fam) in FAMILIES.iter().enumerate() {
            let has_stairs = fam.stairs != Block::AIR;
            assert!((!has_stairs || seen.insert(fam.stairs)) && seen.insert(fam.slab), "{fam:?} listed twice");
            if has_stairs {
                assert_eq!(fam.stairs.stairs_base(), Some(fam.base), "{f}");
            } else {
                assert!(f >= LEGACY_COUNT, "only v0.12 families may be slab-only");
            }
            assert_eq!(fam.slab.slab_base(), Some(fam.base), "{f}");
            for facing in Facing::ALL.into_iter().filter(|_| has_stairs) {
                for top in [false, true] {
                    for shape in StairShape::ALL {
                        for wet in [false, true] {
                            let s = Stairs { family: f as u8, facing, top, shape, wet };
                            let id = stairs_id(s);
                            assert_eq!(stairs(id), Some(s));
                            assert_eq!(stairs_const(id.0), Some(s));
                            let legacy = !top && shape == StairShape::Straight && !wet;
                            if f < LEGACY_COUNT {
                                assert_eq!(legacy, id.0 < STAIRS_ORIGIN, "{s:?}");
                            }
                            if legacy {
                                assert_eq!(id, fam.stairs.with_facing(facing));
                            }
                        }
                    }
                }
            }
            for kind in [SlabType::Bottom, SlabType::Top, SlabType::Double] {
                for wet in [false, true] {
                    let s = Slab { family: f as u8, kind, wet: wet && kind != SlabType::Double };
                    assert_eq!(slab(slab_id(s)), Some(s));
                }
            }
            assert_eq!(slab_id(Slab { family: f as u8, kind: SlabType::Bottom, wet: false }), fam.slab);
        }
        // Every id in the range decodes to exactly one state.
        for id in STAIRS_ORIGIN..END {
            let b = Block(id);
            assert!(stairs(b).is_some() != slab(b).is_some(), "{id}");
            assert!(b.name() != "unknown", "{id}");
        }
        for id in END..=RESERVED_LAST {
            assert_eq!(Block(id).name(), "unknown");
        }
        // Every stairs and slab in the registry belongs to a family.
        for id in 0..STAIRS_ORIGIN {
            let b = Block(id);
            if b.kind() == RenderKind::Invisible {
                continue;
            }
            let name = b.name();
            assert_eq!(name.ends_with(" stairs"), stairs(b).is_some(), "{id} {name}");
            assert_eq!(name.ends_with(" slab"), slab(b).is_some(), "{id} {name}");
        }
    }

    #[test]
    fn existing_ids_keep_their_meaning() {
        assert_eq!(
            stairs(Block::STONE_STAIRS),
            Some(Stairs { family: 0, facing: Facing::South, top: false, shape: StairShape::Straight, wet: false })
        );
        assert_eq!(Block::STONE_STAIRS, Block(112));
        assert_eq!(Block::STONE_BRICK_STAIRS.with_facing(Facing::West), Block(192));
        assert_eq!(Block::slab_of(Block::STONE), Some(Block(106)));
        assert_eq!(slab_type(Block(106)), Some(SlabType::Bottom));
        assert_eq!(slab_type(Block::STONE_BRICK_SLAB), Some(SlabType::Bottom));
        assert_eq!(Block(106).top_drop(), 8);
        assert_eq!(Block(112).shaped(), Some(Shaped::Stairs(Facing::South)));
    }

    /// Java facings (the tall side) as the codebase's fronts.
    fn java(f: Facing) -> Facing {
        f.opposite()
    }

    fn stair(family: u8, java_facing: Facing, top: bool) -> Block {
        stairs_id(Stairs { family, facing: java(java_facing), top, shape: StairShape::Straight, wet: false })
    }

    /// `getStairsShape` of `centre` in a 3x3 layout given as (dx, dz, block).
    fn shape_in(centre: Block, around: &[(i32, i32, Block)]) -> StairShape {
        let at = |f: Facing| {
            let o = f.offset();
            around.iter().find(|&&(x, z, _)| x == o.x && z == o.z).map(|&(_, _, b)| b).or(Some(Block::AIR))
        };
        stairs_shape(stairs(centre).unwrap(), at)
    }

    #[test]
    fn corner_shapes_follow_get_stairs_shape_for_every_layout() {
        use Facing::*;
        use StairShape::*;
        // Every Java facing of the centre, with the neighbour behind (on the
        // tall side) or in front turned either way, on both halves, for
        // two families (shape ignores the material).
        for top in [false, true] {
            for facing in Facing::ALL {
                let ccw = facing.clockwise().opposite();
                let cw = facing.clockwise();
                let centre = stair(0, facing, top);
                let behind = facing.offset();
                let front = facing.opposite().offset();
                let other = |f: Facing| stair(30, f, top);
                assert_eq!(shape_in(centre, &[]), Straight);
                assert_eq!(shape_in(centre, &[(behind.x, behind.z, other(ccw))]), OuterLeft, "{facing:?} {top}");
                assert_eq!(shape_in(centre, &[(behind.x, behind.z, other(cw))]), OuterRight, "{facing:?} {top}");
                assert_eq!(shape_in(centre, &[(front.x, front.z, other(ccw))]), InnerLeft, "{facing:?} {top}");
                assert_eq!(shape_in(centre, &[(front.x, front.z, other(cw))]), InnerRight, "{facing:?} {top}");
                // Parallel neighbours and the other half leave it straight.
                for f in [facing, facing.opposite()] {
                    assert_eq!(shape_in(centre, &[(behind.x, behind.z, other(f))]), Straight);
                    assert_eq!(shape_in(centre, &[(front.x, front.z, other(f))]), Straight);
                }
                let flipped = stair(1, cw, !top);
                assert_eq!(shape_in(centre, &[(behind.x, behind.z, flipped)]), Straight);
                assert_eq!(shape_in(centre, &[(front.x, front.z, flipped)]), Straight);
                // Slabs and full blocks never turn it.
                assert_eq!(
                    shape_in(centre, &[(behind.x, behind.z, Block(106)), (front.x, front.z, Block::STONE)]),
                    Straight
                );
                // `canTakeShape`: a matching stairs where the corner would
                // open (the side the neighbour's facing points away from)
                // keeps a straight run.
                let open = cw.opposite().offset();
                assert_eq!(
                    shape_in(centre, &[(behind.x, behind.z, other(cw)), (open.x, open.z, other(facing))]),
                    Straight
                );
                let open = cw.offset();
                assert_eq!(
                    shape_in(centre, &[(front.x, front.z, other(cw)), (open.x, open.z, other(facing))]),
                    Straight
                );
                // ...but stairs on that side facing elsewhere or on the other half don't.
                assert_eq!(
                    shape_in(centre, &[(front.x, front.z, other(cw)), (open.x, open.z, stair(2, facing, !top))]),
                    InnerRight
                );
                // Outer wins when both behind and front qualify.
                assert_eq!(
                    shape_in(centre, &[(behind.x, behind.z, other(ccw)), (front.x, front.z, other(cw))]),
                    OuterLeft
                );
            }
        }
        // A reference layout: a north-facing (Java) stair with an east-facing
        // one north of it is OUTER_RIGHT; with one south of it, INNER_RIGHT.
        let centre = stair(0, North, false);
        assert_eq!(shape_in(centre, &[(0, -1, stair(0, East, false))]), OuterRight);
        assert_eq!(shape_in(centre, &[(0, 1, stair(0, East, false))]), InnerRight);
        assert_eq!(shape_in(centre, &[(0, 1, stair(0, West, false))]), InnerLeft);
    }

    #[test]
    fn placement_halves_and_slab_merging_follow_java_clicks() {
        use glam::IVec3;
        let slab = FAMILIES[0].slab;
        let other_slab = FAMILIES[1].slab;
        let kind = |b: Block| slab_type(b).unwrap();
        let none = |_| None;
        // Top face: bottom. Underside: top. Sides by click height.
        assert_eq!(kind(placed(slab, Block::AIR, IVec3::Y, 0.9, none)), SlabType::Bottom);
        assert_eq!(kind(placed(slab, Block::AIR, IVec3::NEG_Y, 0.1, none)), SlabType::Top);
        assert_eq!(kind(placed(slab, Block::AIR, IVec3::X, 0.75, none)), SlabType::Top);
        assert_eq!(kind(placed(slab, Block::AIR, IVec3::NEG_Z, 0.25, none)), SlabType::Bottom);
        assert_eq!(kind(placed(slab, Block::AIR, IVec3::Z, 0.5, none)), SlabType::Bottom, "exactly half is bottom");
        // Into a single slab of the same kind (either half, wet or dry): double, dry.
        for replaced in [slab, slab_id(Slab { family: 0, kind: SlabType::Top, wet: true })] {
            let double = placed(slab, replaced, IVec3::Y, 0.5, none);
            assert_eq!(super::slab(double), Some(Slab { family: 0, kind: SlabType::Double, wet: false }));
            assert!(double.is_opaque());
        }
        assert!(!slab_takes_placement(slab_id(Slab { family: 0, kind: SlabType::Double, wet: false }), slab));
        assert!(!slab_takes_placement(other_slab, slab));
        // `canBeReplaced` on the clicked slab.
        let top = slab_id(Slab { family: 0, kind: SlabType::Top, wet: false });
        assert!(slab_takes_click(slab, slab, IVec3::Y, 0.5));
        assert!(slab_takes_click(slab, slab, IVec3::X, 0.6), "a side above the middle");
        assert!(!slab_takes_click(slab, slab, IVec3::X, 0.4));
        assert!(!slab_takes_click(slab, slab, IVec3::NEG_Y, 0.0));
        assert!(slab_takes_click(top, slab, IVec3::NEG_Y, 0.5));
        assert!(slab_takes_click(top, slab, IVec3::NEG_X, 0.4));
        assert!(!slab_takes_click(top, slab, IVec3::NEG_X, 0.6));
        assert!(!slab_takes_click(top, slab, IVec3::Y, 1.0));
        assert!(!slab_takes_click(slab, other_slab, IVec3::Y, 0.5), "another kind");
        assert!(!slab_takes_click(slab, FAMILIES[0].stairs, IVec3::Y, 0.5), "not a slab item");
        // Stairs keep the facing they arrive with and take their half.
        let east = FAMILIES[5].stairs.with_facing(Facing::East);
        let s = stairs(placed(east, Block::AIR, IVec3::X, 0.8, none)).unwrap();
        assert_eq!((s.facing, s.top, s.shape, s.wet), (Facing::East, true, StairShape::Straight, false));
        let s = stairs(placed(east, Block::AIR, IVec3::Y, 0.8, none)).unwrap();
        assert!(!s.top);
        let s = stairs(placed(east, Block::AIR, IVec3::NEG_Y, 0.2, none)).unwrap();
        assert!(s.top);
    }

    #[test]
    fn every_state_shares_its_familys_properties() {
        use crate::item::Item;
        for (f, fam) in FAMILIES.iter().enumerate() {
            let ids = (STAIRS_ORIGIN..END).map(Block).filter(|&b| {
                stairs(b).map(|s| s.family as usize) == Some(f) || slab(b).map(|s| s.family as usize) == Some(f)
            });
            for b in ids {
                let item = if stairs(b).is_some() { fam.stairs } else { fam.slab };
                assert_eq!(b.name(), item.name(), "{b:?}");
                assert_eq!(b.base(), item, "{b:?}");
                assert_eq!(b.drop(), Some(Item::from_block(item)), "{b:?}");
                assert_eq!(b.hardness(), item.hardness(), "{b:?}");
                assert_eq!(b.best_tool(), item.best_tool(), "{b:?}");
                assert_eq!(b.harvest_level(), item.harvest_level(), "{b:?}");
                // `FireBlock.getBurnOdds`/`getIgniteOdds`: waterlogged states never burn.
                let odds = if b.is_waterlogged() { (0, 0) } else { item.fire_odds() };
                assert_eq!(b.fire_odds(), odds, "{b:?}");
                assert_eq!(b.info().tex, item.info().tex, "{b:?}");
                assert!(b.is_solid() && b.is_targetable(), "{b:?}");
                assert_eq!(b.emission(), 0);
                assert_eq!(b.stairs_base(), item.stairs_base());
                assert_eq!(b.slab_base(), item.slab_base());
                assert_eq!(crate::mining::silk_drop(b), crate::mining::silk_drop(item));
            }
        }
    }

    #[test]
    fn water_pairs_cover_every_state_but_double_slabs() {
        for id in 0..super::super::block::STATE_CAPACITY as u16 {
            let b = Block(id);
            let expected = stairs(b).is_some() || matches!(slab_type(b), Some(SlabType::Top | SlabType::Bottom));
            assert_eq!(water_pair(id).is_some(), expected, "{id}");
            if let Some((other, wet)) = water_pair(id) {
                let (a, o) = (Block(id), Block(other));
                assert_eq!(
                    stairs(a).map(|s| Stairs { wet: false, ..s }),
                    stairs(o).map(|s| Stairs { wet: false, ..s })
                );
                assert_eq!(slab(a).map(|s| Slab { wet: false, ..s }), slab(o).map(|s| Slab { wet: false, ..s }));
                assert_eq!(stairs(a).map_or_else(|| slab(a).unwrap().wet, |s| s.wet), wet);
            }
        }
    }
}
