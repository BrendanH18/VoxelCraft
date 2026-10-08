//! Append-only village blocks, 900..=958. Oriented workstations use four states.
use super::block::{Block, Facing, RenderKind, Shaped, tex};
use crate::item::ToolKind;

pub const BASES: [Block; 15] = [
    Block::DIRT_PATH,
    Block::HAY_BALE,
    Block::COMPOSTER,
    Block::BARREL,
    Block::SMOKER,
    Block::LIT_SMOKER,
    Block::BLAST_FURNACE,
    Block::LIT_BLAST_FURNACE,
    Block::CARTOGRAPHY_TABLE,
    Block::FLETCHING_TABLE,
    Block::GRINDSTONE,
    Block::LECTERN,
    Block::LOOM,
    Block::STONECUTTER,
    Block::BELL,
];
const NAMES: [&str; 15] = [
    "dirt path",
    "hay bale",
    "composter",
    "barrel",
    "smoker",
    "smoker",
    "blast furnace",
    "blast furnace",
    "cartography table",
    "fletching table",
    "grindstone",
    "lectern",
    "loom",
    "stonecutter",
    "bell",
];

pub const fn index(id: u16) -> Option<usize> {
    match id {
        900 => Some(0),
        901..=903 => Some(1),
        904 => Some(2),
        905..=952 => Some(3 + ((id - 905) / 4) as usize),
        _ => None,
    }
}
pub fn oriented(id: u16) -> Option<(Block, Facing)> {
    (905..=952).contains(&id).then(|| (Block(905 + (id - 905) / 4 * 4), Facing::ALL[((id - 905) % 4) as usize]))
}
pub fn base(id: u16) -> Option<Block> {
    if (901..=903).contains(&id) { Some(Block::HAY_BALE) } else { oriented(id).map(|(b, _)| b) }
}
pub fn placed(b: Block, normal: glam::IVec3) -> Block {
    if b.base() == Block::HAY_BALE {
        Block(
            901 + if normal.x != 0 {
                1
            } else if normal.z != 0 {
                2
            } else {
                0
            },
        )
    } else {
        b
    }
}
pub fn shaped(id: u16) -> Option<Shaped> {
    let i = index(id)?;
    matches!(i, 2 | 10 | 11 | 13 | 14)
        .then(|| Shaped::Village { kind: i as u8, facing: oriented(id).map_or(Facing::South, |(_, f)| f) })
}
pub fn palette_ids() -> impl Iterator<Item = u16> {
    BASES.into_iter().filter(|b| !matches!(*b, Block::LIT_SMOKER | Block::LIT_BLAST_FURNACE)).map(|b| b.0)
}
pub fn hardness(id: u16) -> Option<f32> {
    Some(match index(id)? {
        0 => 0.65,
        1 => 0.5,
        2 => 0.6,
        4..=7 => 3.5,
        10 => 2.0,
        13 => 3.5,
        14 => 5.0,
        _ => 2.5,
    })
}
pub fn tool(id: u16) -> Option<ToolKind> {
    Some(match index(id)? {
        0 => ToolKind::Shovel,
        1 => ToolKind::Hoe,
        4..=7 | 10 | 13 | 14 => ToolKind::Pickaxe,
        _ => ToolKind::Axe,
    })
}
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some(i) = index(id) else { return None };
    let layer = tex::VILLAGE + i as u16 * 3;
    let mut t = [layer, layer, layer + 1, layer + 2, layer, layer];
    if id >= 905 {
        t[Facing::ALL[((id - 905) % 4) as usize].face()] = layer + 2;
    }
    if i == 1 {
        let axis = id - 901;
        t = [layer; 6];
        let a = if axis == 1 {
            0
        } else if axis == 2 {
            4
        } else {
            2
        };
        t[a] = layer + 1;
        t[a + 1] = layer + 1;
    }
    Some((
        NAMES[i],
        if matches!(i, 2 | 10 | 11 | 13 | 14) {
            RenderKind::Shaped
        } else if i == 0 {
            RenderKind::Cutout
        } else {
            RenderKind::Opaque
        },
        t,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    #[test]
    fn all_workstations_register_turn_and_drop_base_states() {
        for b in BASES {
            assert_eq!(
                Block::from_name(b.name()),
                Some(match b {
                    Block::LIT_SMOKER => Block::SMOKER,
                    Block::LIT_BLAST_FURNACE => Block::BLAST_FURNACE,
                    _ => b,
                })
            );
            for f in Facing::ALL {
                let turned = b.with_facing(f);
                assert!(turned.kind() != RenderKind::Invisible);
                assert!(turned.info().tex.iter().all(|&t| u32::from(t) < tex::COUNT));
                if b != Block::DIRT_PATH && b != Block::HAY_BALE && b != Block::COMPOSTER {
                    assert_eq!(turned.oriented(), Some((b, f)));
                }
                assert_eq!(
                    turned.drop(),
                    Some(Item::from_block(match b {
                        Block::DIRT_PATH => Block::DIRT,
                        Block::LIT_SMOKER => Block::SMOKER,
                        Block::LIT_BLAST_FURNACE => Block::BLAST_FURNACE,
                        _ => b,
                    }))
                );
            }
        }
        assert_eq!(placed(Block::HAY_BALE, glam::IVec3::X), Block(902));
        assert_eq!(Block::DIRT_PATH.top_drop(), 1);
    }
}
