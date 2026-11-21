//! v0.16 Track D poplar wood family (BLD-01a): Java 26.3's `poplar_*`
//! blocks, laid out in the unused rest of the v0.12 wood track's
//! reservation (`docs/architecture.md`, v0.16 track reservations). Every
//! Java state has its own id; waterlogged states included.
//!
//! | Ids | States |
//! | --- | --- |
//! | 19584..=19586 | poplar log, axis Y, X, Z |
//! | 19587..=19595 | wood, stripped log, stripped wood: `19587 + 3 * form + axis` |
//! | 19596 | poplar planks |
//! | 19597..=19599 | orange, red and yellow poplar leaves; 19600..=19602 waterlogged |
//! | 19603, 19604 | poplar sapling, `stage` 0 then 1 |
//! | 19605 | potted poplar sapling |
//! | 19606..=19623 | fence, gate and door: `forms` wood index 12, `FORMS + local` for locals 5..=22 |
//! | 19624 | fence, waterlogged |
//! | 19625..=19632 | right-hinge lower doors, `open * 4 + facing` (see `mount_states`) |
//! | 19633..=19637 | slab: bottom, bottom wet, top, top wet, double |
//! | 19648..=19727 | stairs, in `slab_stair_states`' local order |
//! | 19728..=19791 | trapdoor, button and pressure plate: `wood::switches` family 8 |
//! | 19792..=19919 | signs: `wood::signs` layout for [`Kind::Poplar`] |
//! | 19920..=19983 | shelves: `wood::shelves` layout for [`Kind::Poplar`] |
//! | 20000..=20479 | cinnabar and sulfur stairs (`world::gen5_blocks`), 80 per family |
//!
//! Every other id in 19584..=20479 stays unregistered.

use glam::IVec3;

use super::super::block::{Block, RenderKind};
use super::Props;
use crate::item::{Item, ToolKind};

pub const FIRST: u16 = 19584;
pub const LAST: u16 = 20479;

pub const LOG: Block = Block(19584);
const PILLARS: u16 = 19587;
pub const WOOD: Block = Block(PILLARS);
pub const STRIPPED_LOG: Block = Block(PILLARS + 3);
pub const STRIPPED_WOOD: Block = Block(PILLARS + 6);
pub const PLANKS: Block = Block(19596);
/// Orange, red and yellow, Java's `orange_poplar_leaves`,
/// `red_poplar_leaves` and `yellow_poplar_leaves`.
pub const LEAVES: [Block; 3] = [Block(19597), Block(19598), Block(19599)];
pub const ORANGE_LEAVES: Block = LEAVES[0];
pub const RED_LEAVES: Block = LEAVES[1];
pub const YELLOW_LEAVES: Block = LEAVES[2];
const LEAVES_WET: u16 = 19600;
pub const SAPLING: Block = Block(19603);
/// The sapling at Java's `stage=1`.
pub const SAPLING_STAGE_ONE: Block = Block(19604);
pub const POTTED_SAPLING: Block = Block(19605);
/// `forms::wood_id(12, local)` is `FORMS + local`; locals 5..=22 are used.
pub const FORMS: u16 = 19601;
/// `forms` wood index of poplar.
pub const FORM_INDEX: u16 = 12;
pub const FENCE: Block = Block(FORMS + 5);
pub const GATE: Block = Block(FORMS + 6);
pub const DOOR: Block = Block(FORMS + 14);
const FENCE_WET: u16 = 19624;
/// Right-hinge lower doors, `open * 4 + facing`.
pub const RIGHT_DOORS: u16 = 19625;
pub const SLAB: Block = Block(19633);
pub const STAIRS: Block = Block(19648);
/// The `wood::switches` family of poplar's trapdoor, button and plate.
pub const SWITCHES: u16 = 19728;
pub const SIGNS: u16 = 19792;
pub const SHELVES: u16 = 19920;
/// Cinnabar and sulfur stairs (see `world::gen5_blocks`).
pub const STONE_STAIRS: u16 = 20000;
pub const STONE_STAIRS_END: u16 = 20480;

/// The `slab_stair_states` family of poplar's stairs and slab.
pub const SHAPES: super::super::slab_stair_states::Family =
    super::super::slab_stair_states::Family { base: PLANKS, stairs: STAIRS, slab: SLAB };

/// Texture layers (v0.16 Track D owns 749..=948).
pub mod t {
    pub const BASE: u16 = 749;
    pub const LOG_SIDE: u16 = BASE;
    pub const LOG_TOP: u16 = BASE + 1;
    pub const LOG_SIDE_TURNED: u16 = BASE + 2;
    pub const STRIPPED_SIDE: u16 = BASE + 3;
    pub const STRIPPED_TOP: u16 = BASE + 4;
    pub const STRIPPED_SIDE_TURNED: u16 = BASE + 5;
    pub const PLANKS: u16 = BASE + 6;
    /// Orange, red and yellow leaves.
    pub const LEAVES: u16 = BASE + 7;
    pub const SAPLING: u16 = BASE + 10;
    pub const DOOR_BOTTOM: u16 = BASE + 11;
    pub const DOOR_TOP: u16 = BASE + 12;
    pub const TRAPDOOR: u16 = BASE + 13;
    pub const SIGN_ICON: u16 = BASE + 14;
    pub const HANGING_SIGN_ICON: u16 = BASE + 15;
    pub const SHELF: u16 = BASE + 16;
    /// Unpowered, then powered unconnected, left, centre and right.
    pub const SHELF_FRONTS: u16 = BASE + 17;
    pub const POTTED_SAPLING: u16 = BASE + 22;
    pub const END: u16 = BASE + 23;
}

// ---- Logs, wood and stripped blocks --------------------------------------

/// Form (0 log, 1 wood, 2 stripped log, 3 stripped wood) and axis (0 Y, 1
/// X, 2 Z) of a pillar state.
pub const fn pillar_state(id: u16) -> Option<(u16, u16)> {
    if id >= LOG.0 && id < PLANKS.0 {
        let i = id - LOG.0;
        Some((i / 3, i % 3))
    } else {
        None
    }
}

pub const fn pillar(form: u16, axis: u16) -> Block {
    Block(LOG.0 + form * 3 + axis)
}

const fn pillar_tex(axis: u16, side: u16, turned: u16, end: u16) -> [u16; 6] {
    match axis {
        1 => [end, end, turned, turned, turned, turned],
        2 => [turned, turned, side, side, end, end],
        _ => [side, side, end, end, side, side],
    }
}

/// `RotatedPillarBlock.getStateForPlacement`: the clicked face's axis.
pub fn axis_placed(block: Block, normal: IVec3) -> Option<Block> {
    let (form, _) = pillar_state(block.0)?;
    let axis = if normal.x != 0 {
        1
    } else if normal.z != 0 {
        2
    } else {
        0
    };
    Some(pillar(form, axis))
}

pub fn axis(block: Block) -> Option<u16> {
    pillar_state(block.0).map(|(_, axis)| axis)
}

/// `block_transformer/axe.json`: log to stripped log, wood to stripped wood.
pub fn stripped(block: Block) -> Option<Block> {
    match pillar_state(block.0)? {
        (0, axis) => Some(pillar(2, axis)),
        (1, axis) => Some(pillar(3, axis)),
        _ => None,
    }
}

/// Java's `#poplar_logs` (and so `#logs` and `#logs_that_burn`).
pub const LOG_TAG: [Item; 4] =
    [Item::from_block(LOG), Item::from_block(WOOD), Item::from_block(STRIPPED_LOG), Item::from_block(STRIPPED_WOOD)];

// ---- Other blocks -------------------------------------------------------

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// The orange, red or yellow leaves (0, 1, 2), dry or wet.
pub const fn leaves_colour(id: u16) -> Option<u16> {
    if id >= LEAVES[0].0 && id < LEAVES_WET + 3 { Some((id - LEAVES[0].0) % 3) } else { None }
}

pub fn is_leaves(b: Block) -> bool {
    leaves_colour(b.0).is_some()
}

pub fn is_log(b: Block) -> bool {
    pillar_state(b.0).is_some()
}

pub fn is_planks(b: Block) -> bool {
    b == PLANKS
}

pub fn is_sapling(b: Block) -> bool {
    b == SAPLING || b == SAPLING_STAGE_ONE
}

/// A right-hinge lower door's left-hinge twin.
pub const fn door_twin(id: u16) -> Option<Block> {
    if id >= RIGHT_DOORS && id < RIGHT_DOORS + 8 { Some(Block(FORMS + 14 + id - RIGHT_DOORS)) } else { None }
}

/// The right-hinge twin of a left-hinge lower door.
pub const fn right_of(left: Block) -> Option<Block> {
    if left.0 >= FORMS + 14 && left.0 < FORMS + 22 { Some(Block(RIGHT_DOORS + left.0 - FORMS - 14)) } else { None }
}

/// Fence, gate and door states (`forms` wood index 12): local 5..=22.
pub const fn form_local(id: u16) -> Option<u16> {
    if id >= FORMS + 5 && id <= FORMS + 22 { Some(id - FORMS) } else { None }
}

const fn stairs_local(id: u16) -> Option<u16> {
    if id >= STAIRS.0 && id < STAIRS.0 + 80 { Some(id - STAIRS.0) } else { None }
}

const fn slab_local(id: u16) -> Option<u16> {
    if id >= SLAB.0 && id < SLAB.0 + 5 { Some(id - SLAB.0) } else { None }
}

/// Whether `id` is one of the cinnabar and sulfur stairs kept here.
pub const fn is_stone_stairs(id: u16) -> bool {
    id >= STONE_STAIRS && id < STONE_STAIRS_END
}

/// States of this range that `Block`'s shared shape rules handle (fences,
/// gates, doors, stairs and slabs), not `wood::with_facing`.
pub const fn generic(id: u16) -> bool {
    form_local(id).is_some()
        || door_twin(id).is_some()
        || id == FENCE_WET
        || stairs_local(id).is_some()
        || slab_local(id).is_some()
        || is_stone_stairs(id)
}

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

/// Name, render kind and textures of poplar's own states (switches, signs
/// and shelves answer through their modules; the stone stairs through
/// `gen5_blocks`).
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    if let Some((form, axis)) = pillar_state(id) {
        return Some(match form {
            0 => ("poplar log", Opaque, pillar_tex(axis, t::LOG_SIDE, t::LOG_SIDE_TURNED, t::LOG_TOP)),
            1 => ("poplar wood", Opaque, pillar_tex(axis, t::LOG_SIDE, t::LOG_SIDE_TURNED, t::LOG_SIDE)),
            2 => (
                "stripped poplar log",
                Opaque,
                pillar_tex(axis, t::STRIPPED_SIDE, t::STRIPPED_SIDE_TURNED, t::STRIPPED_TOP),
            ),
            _ => (
                "stripped poplar wood",
                Opaque,
                pillar_tex(axis, t::STRIPPED_SIDE, t::STRIPPED_SIDE_TURNED, t::STRIPPED_SIDE),
            ),
        });
    }
    if let Some(c) = leaves_colour(id) {
        let name = match c {
            0 => "orange poplar leaves",
            1 => "red poplar leaves",
            _ => "yellow poplar leaves",
        };
        return Some((name, Cutout, all(t::LEAVES + c)));
    }
    if let Some(local) = form_local(id) {
        return Some(match local {
            5 => ("poplar fence", Shaped, all(t::PLANKS)),
            6..=13 => ("poplar fence gate", Shaped, all(t::PLANKS)),
            22 => ("poplar door", Shaped, all(t::DOOR_TOP)),
            _ => ("poplar door", Shaped, all(t::DOOR_BOTTOM)),
        });
    }
    if door_twin(id).is_some() {
        return Some(("poplar door", Shaped, all(t::DOOR_BOTTOM)));
    }
    if id == FENCE_WET {
        return Some(("poplar fence", Shaped, all(t::PLANKS)));
    }
    if stairs_local(id).is_some() {
        return Some(("poplar stairs", Shaped, all(t::PLANKS)));
    }
    if let Some(local) = slab_local(id) {
        let kind = match local {
            0 | 1 => Cutout,
            2 | 3 => Shaped,
            _ => Opaque,
        };
        return Some(("poplar slab", kind, all(t::PLANKS)));
    }
    if is_stone_stairs(id) {
        return super::super::gen5_blocks::stairs_registry(id);
    }
    Some(match id {
        19596 => ("poplar planks", Opaque, all(t::PLANKS)),
        19603 | 19604 => ("poplar sapling", Cross, all(t::SAPLING)),
        19605 => {
            use super::super::utilities::t as u;
            (
                "potted poplar sapling",
                Shaped,
                [
                    t::POTTED_SAPLING,
                    t::POTTED_SAPLING,
                    u::POT_SOIL,
                    u::POT_BOTTOM,
                    t::POTTED_SAPLING,
                    t::POTTED_SAPLING,
                ],
            )
        }
        _ => return None,
    })
}

/// Waterlogged pairs: leaves, the fence, stairs and slabs.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if let Some(c) = leaves_colour(id) {
        return if id >= LEAVES_WET { Some((LEAVES[c as usize].0, true)) } else { Some((LEAVES_WET + c, false)) };
    }
    if id == FENCE.0 {
        return Some((FENCE_WET, false));
    }
    if id == FENCE_WET {
        return Some((FENCE.0, true));
    }
    if stairs_local(id).is_some() || slab_local(id).is_some() || is_stone_stairs(id) {
        return super::super::slab_stair_states::water_pair(id);
    }
    None
}

/// The block items and rules use: logs on their side answer as Y, the
/// stage-1 sapling as the sapling.
pub fn base(id: u16) -> Option<Block> {
    if let Some((form, _)) = pillar_state(id) {
        return Some(pillar(form, 0));
    }
    match id {
        19596..=19599 | 19603 | 19605 => Some(Block(id)),
        19604 => Some(SAPLING),
        _ => None,
    }
}

/// A state whose properties are another's: pillars on their side.
pub const fn alias(id: u16) -> Option<u16> {
    match pillar_state(id) {
        Some((form, axis)) if axis != 0 => Some(pillar(form, 0).0),
        _ => None,
    }
}

/// Java 26.3 `Blocks` and `FireBlock.bootStrap` for poplar's own states:
/// logs and wood 2.0 (fire 5/5), planks, stairs, slabs, fences and gates
/// 2.0 (5/20), doors 3.0 (lava only), leaves 0.2 with a hoe (30/60),
/// saplings and the pot instant.
pub fn props(block: Block) -> Option<Props> {
    let id = super::canonical(block.0);
    let axe = Some(ToolKind::Axe);
    let p = |hardness, tool, fire, lava, drop: Option<Block>| Props {
        hardness,
        tool,
        fire,
        lava,
        drop: drop.map(Item::from_block),
    };
    if let Some((form, _)) = pillar_state(id) {
        return Some(p(2.0, axe, (5, 5), true, Some(pillar(form, 0))));
    }
    if leaves_colour(id).is_some() {
        // Leaves roll their own drops (`World::spill_mined`).
        return Some(p(0.2, Some(ToolKind::Hoe), (30, 60), true, None));
    }
    if let Some(local) = form_local(id) {
        return Some(match local {
            5 => p(2.0, axe, (5, 20), true, Some(FENCE)),
            6..=13 => p(2.0, axe, (5, 20), true, Some(GATE)),
            22 => p(3.0, axe, (0, 0), true, None),
            _ => p(3.0, axe, (0, 0), true, Some(DOOR)),
        });
    }
    if door_twin(id).is_some() {
        return Some(p(3.0, axe, (0, 0), true, Some(DOOR)));
    }
    if stairs_local(id).is_some() {
        return Some(p(2.0, axe, (5, 20), true, Some(STAIRS)));
    }
    if slab_local(id).is_some() {
        return Some(p(2.0, axe, (5, 20), true, Some(SLAB)));
    }
    match id {
        19596 => Some(p(2.0, axe, (5, 20), true, Some(PLANKS))),
        19603 | 19604 => Some(p(0.0, None, (0, 0), false, Some(SAPLING))),
        _ => None,
    }
}

/// Default states for the creative palette, in Java's creative order.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [
        LOG.0,
        WOOD.0,
        STRIPPED_LOG.0,
        STRIPPED_WOOD.0,
        PLANKS.0,
        STAIRS.0,
        SLAB.0,
        FENCE.0,
        GATE.0,
        DOOR.0,
        ORANGE_LEAVES.0,
        RED_LEAVES.0,
        YELLOW_LEAVES.0,
        SAPLING.0,
    ]
    .into_iter()
}

/// Java 26.3 fuel providers of poplar's own items.
pub fn fuel(block: Block) -> Option<crate::content::fuel::FuelClass> {
    use crate::content::fuel::FuelClass::*;
    let id = block.0;
    if pillar_state(id).is_some() || id == PLANKS.0 || id == STAIRS.0 || id == FENCE.0 || id == GATE.0 {
        return Some(Wood);
    }
    match block {
        SLAB => Some(Slab),
        DOOR => Some(Large),
        SAPLING => Some(Small),
        _ => None,
    }
}

/// Wood-sounding states of this range.
pub fn wooden(id: u16) -> bool {
    pillar_state(id).is_some()
        || id == PLANKS.0
        || form_local(id).is_some()
        || door_twin(id).is_some()
        || id == FENCE_WET
        || stairs_local(id).is_some()
        || slab_local(id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_do_not_overlap() {
        const { assert!(FENCE_WET > FORMS + 22 && RIGHT_DOORS > FENCE_WET && SLAB.0 >= RIGHT_DOORS + 8) };
        const { assert!(STAIRS.0 >= SLAB.0 + 5 && SWITCHES >= STAIRS.0 + 80 && SIGNS >= SWITCHES + 64) };
        const { assert!(SHELVES >= SIGNS + 128 && STONE_STAIRS >= SHELVES + 64 && STONE_STAIRS_END - 1 == LAST) };
        assert_eq!(LEAVES_WET, LEAVES[2].0 + 1);
        assert_eq!(SAPLING.0, LEAVES_WET + 3);
        assert_eq!(FORMS + 5, POTTED_SAPLING.0 + 1);
    }

    #[test]
    fn pillars_strip_and_keep_their_axis() {
        for axis in 0..3 {
            assert_eq!(stripped(pillar(0, axis)), Some(pillar(2, axis)));
            assert_eq!(stripped(pillar(1, axis)), Some(pillar(3, axis)));
            assert_eq!(stripped(pillar(2, axis)), None);
            assert_eq!(base(pillar(1, axis).0), Some(WOOD));
        }
        assert_eq!(axis_placed(LOG, IVec3::X), Some(pillar(0, 1)));
        assert_eq!(axis_placed(STRIPPED_WOOD, IVec3::NEG_Z), Some(pillar(3, 2)));
    }

    #[test]
    fn waterlogged_pairs_are_symmetric() {
        for id in FIRST..=SHELVES {
            if let Some((other, wet)) = water_pair(id) {
                assert_eq!(water_pair(other), Some((id, !wet)), "{id}");
            }
        }
    }
}
