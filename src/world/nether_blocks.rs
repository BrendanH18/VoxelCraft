//! Append-only Nether materials (800..=834), separate from overworld forms.
use super::block::{Block, Facing, RenderKind, Shaped, tex};

pub const SHAPE_BASES: [Block; 3] = [Block::BLACKSTONE, Block::POLISHED_BLACKSTONE, Block::POLISHED_BLACKSTONE_BRICKS];
const NAMES: [[&str; 3]; 3] = [
    ["blackstone stairs", "blackstone slab", "blackstone wall"],
    ["polished blackstone stairs", "polished blackstone slab", "polished blackstone wall"],
    ["polished blackstone brick stairs", "polished blackstone brick slab", "polished blackstone brick wall"],
];
pub const fn shape_id(index: usize, local: u16) -> Block {
    Block(817 + index as u16 * 6 + local)
}
pub const fn form(id: u16) -> Option<(usize, u16)> {
    if id >= 817 && id <= 834 { Some(((id - 817) as usize / 6, (id - 817) % 6)) } else { None }
}
pub fn shape_of(base: Block, local: u16) -> Option<Block> {
    SHAPE_BASES.iter().position(|&b| b == base).map(|i| shape_id(i, local))
}
pub const fn shaped(id: u16) -> Option<Shaped> {
    if id >= 814 && id <= 816 {
        return Some(Shaped::Chain((id - 814) as u8));
    }
    match form(id) {
        Some((_, f @ 0..=3)) => Some(Shaped::Stairs(Facing::ALL[f as usize])),
        Some((_, 5)) => Some(Shaped::Wall),
        _ => None,
    }
}
pub const fn base(id: u16) -> Option<Block> {
    match id {
        806..=808 => Some(Block::BASALT),
        809..=811 => Some(Block::POLISHED_BASALT),
        814..=816 => Some(Block::CHAIN),
        _ => match form(id) {
            Some((i, 0..=3)) => Some(shape_id(i, 0)),
            _ => None,
        },
    }
}
/// Axis states are Y, X, Z. Placement follows the clicked face.
pub fn placed(block: Block, normal: glam::IVec3) -> Block {
    if let Some(b) = super::nether_biome_blocks::axis_placed(block, normal) {
        return b;
    }
    let base = block.base();
    if matches!(base, Block::BASALT | Block::POLISHED_BASALT | Block::CHAIN) {
        Block(
            base.0
                + if normal.x != 0 {
                    1
                } else if normal.z != 0 {
                    2
                } else {
                    0
                },
        )
    } else {
        block
    }
}
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (800..=806)
        .chain([809, 812, 813, 814])
        .chain((0..3).flat_map(|i| [shape_id(i, 0).0, shape_id(i, 4).0, shape_id(i, 5).0]))
}
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    let plain = match id {
        800 => ("blackstone", tex::BLACKSTONE),
        801 => ("polished blackstone", tex::POLISHED_BLACKSTONE),
        802 => ("polished blackstone bricks", tex::POLISHED_BLACKSTONE_BRICKS),
        803 => ("cracked polished blackstone bricks", tex::CRACKED_POLISHED_BLACKSTONE_BRICKS),
        804 => ("chiseled polished blackstone", tex::CHISELED_POLISHED_BLACKSTONE),
        805 => ("gilded blackstone", tex::GILDED_BLACKSTONE),
        812 => ("magma block", tex::MAGMA),
        813 => ("block of gold", tex::GOLD_BLOCK),
        814..=816 => return Some(("chain", Shaped, [tex::CHAIN; 6])),
        806..=811 => {
            let polished = id >= 809;
            let side = if polished { tex::POLISHED_BASALT_SIDE } else { tex::BASALT_SIDE };
            let end = side + 1;
            let axis = (id - 806) % 3;
            let t = match axis {
                1 => [end, end, side, side, side, side],
                2 => [side, side, side, side, end, end],
                _ => [side, side, end, end, side, side],
            };
            return Some((if polished { "polished basalt" } else { "basalt" }, Opaque, t));
        }
        _ => {
            if let Some((i, local)) = form(id) {
                let t = tex::BLACKSTONE + i as u16;
                return Some((
                    NAMES[i][if local < 4 { 0 } else { (local - 3) as usize }],
                    if local == 4 { Cutout } else { Shaped },
                    [t; 6],
                ));
            }
            return None;
        }
    };
    Some((plain.0, Opaque, [plain.1; 6]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        item::{Item, Tier, ToolKind},
        mining,
    };
    #[test]
    fn nether_materials_keep_axes_shapes_and_pickaxe_rules() {
        for base in [Block::BASALT, Block::POLISHED_BASALT, Block::CHAIN] {
            for (n, offset) in [(glam::IVec3::Y, 0), (glam::IVec3::X, 1), (glam::IVec3::Z, 2)] {
                let b = placed(base, n);
                assert_eq!(b.0, base.0 + offset);
                assert_eq!(b.drop(), Some(Item::from_block(base)));
            }
        }
        for b in SHAPE_BASES {
            for local in [0, 4, 5] {
                let shaped = shape_of(b, local).unwrap();
                assert_eq!(shaped.hardness(), b.hardness());
                assert!(!mining::can_harvest(shaped, None));
                assert!(mining::can_harvest(shaped, Some(Item::tool(ToolKind::Pickaxe, Tier::Wood))));
            }
        }
        assert!(!mining::can_harvest(Block::GOLD_BLOCK, Some(Item::tool(ToolKind::Pickaxe, Tier::Stone))));
        assert!(mining::can_harvest(Block::GOLD_BLOCK, Some(Item::tool(ToolKind::Pickaxe, Tier::Iron))));
    }
}
