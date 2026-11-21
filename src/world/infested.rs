//! v0.16 Track E infested blocks (Java 26.3 `InfestedBlock` and
//! `InfestedRotatedPillarBlock`): stone, cobblestone, the four stone bricks
//! and deepslate with a silverfish inside. Each looks like its host, takes
//! half the host's mining time (`destroyTime(host / 2)`), resists
//! explosions at 0.75, drops nothing unless mined with Silk Touch (then the
//! host block, `blocks/infested_*`), and releases a silverfish when broken
//! without Silk Touch or blown up (`spawnAfterBreak`, under `blockDrops`).
//! Silverfish turn hosts into these blocks (`SilverfishMergeWithStoneGoal`,
//! see `entity::silverfish`).
//!
//! | Id | State |
//! | --- | --- |
//! | 30720 | infested stone |
//! | 30721 | infested cobblestone |
//! | 30722 | infested stone bricks |
//! | 30723 | infested mossy stone bricks |
//! | 30724 | infested cracked stone bricks |
//! | 30725 | infested chiseled stone bricks |
//! | 30726..=30728 | infested deepslate, `axis` y, x, z (y the default) |
//!
//! States 30729..=30847 of Track E's reservation stay unregistered.

use super::block::{Block, RenderKind};
use crate::item::{Item, ToolKind};

pub const FIRST: u16 = 30720;
pub const LAST: u16 = 30728;

pub const INFESTED_STONE: Block = Block(30720);
pub const INFESTED_COBBLESTONE: Block = Block(30721);
pub const INFESTED_STONE_BRICKS: Block = Block(30722);
pub const INFESTED_MOSSY_STONE_BRICKS: Block = Block(30723);
pub const INFESTED_CRACKED_STONE_BRICKS: Block = Block(30724);
pub const INFESTED_CHISELED_STONE_BRICKS: Block = Block(30725);
/// Infested deepslate standing up (`axis=y`); x and z follow.
pub const INFESTED_DEEPSLATE: Block = Block(30726);

/// Java's `InfestedBlock` hosts, in this module's id order (deepslate last).
pub const HOSTS: [Block; 7] = [
    Block::STONE,
    Block::COBBLESTONE,
    Block::STONE_BRICKS,
    Block::MOSSY_STONE_BRICKS,
    Block::CRACKED_STONE_BRICKS,
    super::stone_blocks::CHISELED_STONE_BRICKS,
    Block::DEEPSLATE,
];

const NAMES: [&str; 7] = [
    "infested stone",
    "infested cobblestone",
    "infested stone bricks",
    "infested mossy stone bricks",
    "infested cracked stone bricks",
    "infested chiseled stone bricks",
    "infested deepslate",
];

/// `InfestedBlock`'s `explosionResistance(0.75F)`.
pub const RESISTANCE: f32 = 0.75;

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// Index into [`HOSTS`] of an infested state.
const fn kind(id: u16) -> Option<usize> {
    if !owns(id) {
        return None;
    }
    let i = (id - FIRST) as usize;
    Some(if i > 6 { 6 } else { i })
}

/// The host block an infested state hides in (`getHostBlock`), `None` for
/// other blocks.
pub const fn host(b: Block) -> Option<Block> {
    match kind(b.0) {
        Some(i) => Some(HOSTS[i]),
        None => None,
    }
}

/// Infested deepslate's axis: 0 y, 1 x, 2 z.
pub const fn deepslate_axis(b: Block) -> Option<u16> {
    if b.0 >= INFESTED_DEEPSLATE.0 && b.0 <= LAST { Some(b.0 - INFESTED_DEEPSLATE.0) } else { None }
}

/// `InfestedBlock.isCompatibleHostBlock` with `infestedStateByHost`: the
/// infested state a silverfish turns `b` into. VoxelCraft's deepslate has
/// one state, so it becomes the upright infested deepslate.
pub fn infested(b: Block) -> Option<Block> {
    HOSTS.iter().position(|&h| h == b).map(|i| Block(FIRST + i as u16))
}

/// The item block (`Block::base`): the upright deepslate for its axes.
pub const fn base(id: u16) -> Option<Block> {
    if deepslate_axis(Block(id)).is_some() { Some(INFESTED_DEEPSLATE) } else { None }
}

/// Name, render kind and textures: the host's looks.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    match kind(id) {
        Some(i) => Some((NAMES[i], RenderKind::Opaque, super::block::make(HOSTS[i].0).tex)),
        None => None,
    }
}

/// Hardness (half the host's), the pickaxe of `#mineable/pickaxe`, and no
/// required tier (`requiresCorrectToolForDrops` is unset).
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    host(b).map(|h| (h.hardness() / 2.0, Some(ToolKind::Pickaxe), None))
}

/// Survival drops without Silk Touch: none.
pub fn drop(b: Block) -> Option<Option<Item>> {
    owns(b.0).then_some(None)
}

/// `blocks/infested_*`: Silk Touch drops the host block.
pub fn silk_drop(b: Block) -> Option<Item> {
    host(b).map(Item::from_block)
}

/// Default states for the creative palette.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    FIRST..=INFESTED_DEEPSLATE.0
}

/// `InfestedRotatedPillarBlock.getStateForPlacement`: infested deepslate
/// along the clicked face's axis (`None` for other items).
pub fn placed(raw: Block, face: glam::IVec3) -> Option<Block> {
    if raw != INFESTED_DEEPSLATE {
        return None;
    }
    let axis = if face.x != 0 {
        1
    } else if face.z != 0 {
        2
    } else {
        0
    };
    Some(Block(INFESTED_DEEPSLATE.0 + axis))
}

/// Whether breaking `b` with `tool` lets its silverfish out
/// (`InfestedBlock.spawnAfterBreak`: not with an enchantment in
/// `#prevents_infested_spawns`, which is Silk Touch).
pub fn releases(b: Block, tool: crate::enchant::Enchants) -> bool {
    owns(b.0) && !tool.has(crate::enchant::Enchantment::SilkTouch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infested_blocks_mirror_their_hosts() {
        for (i, &h) in HOSTS.iter().enumerate() {
            let b = Block(FIRST + i as u16);
            assert_eq!(host(b), Some(h));
            assert_eq!(infested(h), Some(b));
            assert_eq!(b.info().tex, h.info().tex, "{}", b.name());
            assert!((b.hardness() - h.hardness() / 2.0).abs() < 1e-6, "{}", b.name());
            assert_eq!(silk_drop(b), Some(Item::from_block(h)));
        }
        assert_eq!(INFESTED_STONE.hardness(), 0.75);
        assert_eq!(INFESTED_COBBLESTONE.hardness(), 1.0);
        assert_eq!(INFESTED_DEEPSLATE.hardness(), 1.5);
        for axis in 0..3 {
            let b = Block(INFESTED_DEEPSLATE.0 + axis);
            assert_eq!(deepslate_axis(b), Some(axis));
            assert_eq!(host(b), Some(Block::DEEPSLATE));
            assert_eq!(b.base(), INFESTED_DEEPSLATE);
        }
        assert_eq!(placed(INFESTED_DEEPSLATE, glam::IVec3::X), Some(Block(INFESTED_DEEPSLATE.0 + 1)));
        assert_eq!(placed(INFESTED_DEEPSLATE, glam::IVec3::NEG_Z), Some(Block(INFESTED_DEEPSLATE.0 + 2)));
        assert_eq!(placed(INFESTED_DEEPSLATE, glam::IVec3::Y), Some(INFESTED_DEEPSLATE));
        assert!(infested(Block::DIRT).is_none());
        let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
        assert!(releases(INFESTED_STONE, crate::enchant::Enchants::NONE));
        assert!(!releases(INFESTED_STONE, silk));
        assert!(!releases(Block::STONE, crate::enchant::Enchants::NONE));
    }
}
