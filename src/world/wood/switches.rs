//! Trapdoors, buttons and pressure plates of spruce, birch, jungle, acacia,
//! dark oak, mangrove and cherry, and the polished blackstone button and
//! plate (Java 26.3 `TrapDoorBlock`, `ButtonBlock`, `PressurePlateBlock`).
//!
//! Like the crimson, warped, pale oak and bamboo switches
//! (`nether_biome_blocks::switch_oak`), each state mirrors an existing
//! state: the wood families mirror oak and polished blackstone mirrors
//! stone, so every redstone rule applies unchanged and
//! `nether_biome_blocks::keep_wood` puts a changed state back on the right
//! family. The switch wood index of family `f` is `f + 4`.
//!
//! Each family has 64 ids from `16512 + 64 * f`:
//!
//! | Local | State | Mirrors |
//! | --- | --- | --- |
//! | 0..=15 | trapdoor, dry | oak 1437..=1452 |
//! | 16..=27 | button mounts 0..=5, off then on | oak 1140..=1151 / stone 1128..=1139 |
//! | 28..=29 | pressure plate, off then on | oak 1215..=1216 / stone 1213..=1214 |
//! | 30..=41 | button mounts 6..=11 | oak 7708..=7719 / stone 7696..=7707 |
//! | 42..=57 | trapdoor, wet | the dry trapdoor at local - 42 |
//!
//! Polished blackstone has no trapdoor, so its locals 0..=15 and 42..=57
//! stay unregistered.

use super::super::block::{Block, RenderKind, tex as block_tex};

pub const FIRST: u16 = 16512;
pub const STRIDE: u16 = 64;
pub const FAMILIES: u16 = 8;
pub const END: u16 = FIRST + STRIDE * FAMILIES;
/// The switch wood index (`nether_biome_blocks::switch_oak`) of the first family.
pub const WOOD_OFFSET: u16 = 4;
/// The polished blackstone family's switch wood index.
pub const POLISHED_BLACKSTONE: u16 = WOOD_OFFSET + 7;

const NAMES: [&str; 8] =
    ["spruce", "birch", "jungle", "acacia", "dark oak", "mangrove", "cherry", "polished blackstone"];

const fn planks(family: u16) -> u16 {
    match family {
        0 => block_tex::SPRUCE_PLANKS,
        1 => block_tex::BIRCH_PLANKS,
        2 => block_tex::JUNGLE_PLANKS,
        3 => block_tex::ACACIA_PLANKS,
        4 => block_tex::DARK_OAK_PLANKS,
        5 => block_tex::MANGROVE_PLANKS,
        6 => block_tex::CHERRY_PLANKS,
        _ => block_tex::POLISHED_BLACKSTONE,
    }
}

/// The planks (or polished blackstone) a family is crafted from.
pub const fn material(family: u16) -> Block {
    match family {
        0 => Block::SPRUCE_PLANKS,
        1 => Block::BIRCH_PLANKS,
        2 => Block::JUNGLE_PLANKS,
        3 => Block::ACACIA_PLANKS,
        4 => Block::DARK_OAK_PLANKS,
        5 => Block::MANGROVE_PLANKS,
        6 => Block::CHERRY_PLANKS,
        _ => Block::POLISHED_BLACKSTONE,
    }
}

const fn split(id: u16) -> Option<(u16, u16)> {
    if id < FIRST || id >= END {
        return None;
    }
    let family = (id - FIRST) / STRIDE;
    let local = (id - FIRST) % STRIDE;
    let used = if family == 7 { local >= 16 && local < 42 } else { local < 58 };
    if used { Some((family, local)) } else { None }
}

/// The oak (or stone) state `id` mirrors and its switch wood index.
pub const fn twin(id: u16) -> Option<(Block, u16)> {
    let Some((family, local)) = split(id) else { return None };
    let wood = family + WOOD_OFFSET;
    let mirror = if family == 7 {
        match local {
            16..=27 => 1128 + local - 16,
            28..=29 => 1213 + local - 28,
            _ => 7696 + local - 30,
        }
    } else {
        match local {
            0..=15 => 1437 + local,
            16..=27 => 1140 + local - 16,
            28..=29 => 1215 + local - 28,
            30..=41 => 7708 + local - 30,
            _ => 1437 + local - 42,
        }
    };
    Some((Block(mirror), wood))
}

/// The `wood` (4..=11) twin of an oak or stone state; anything else is
/// returned unchanged.
pub const fn reskin(original: Block, wood: u16) -> Block {
    let family = wood - WOOD_OFFSET;
    let base = FIRST + family * STRIDE;
    let o = original.0;
    if family == 7 {
        return match o {
            1128..=1139 => Block(base + 16 + o - 1128),
            1213..=1214 => Block(base + 28 + o - 1213),
            7696..=7707 => Block(base + 30 + o - 7696),
            _ => original,
        };
    }
    match o {
        1437..=1452 => Block(base + o - 1437),
        1140..=1151 => Block(base + 16 + o - 1140),
        1215..=1216 => Block(base + 28 + o - 1215),
        7708..=7719 => Block(base + 30 + o - 7708),
        _ => original,
    }
}

/// Wet trapdoors pair with the dry ones 42 ids before them.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    match split(id) {
        Some((family, local)) if family < 7 && local < 16 => Some((id + 42, false)),
        Some((family, local)) if family < 7 && local >= 42 => Some((id - 42, true)),
        _ => None,
    }
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some((family, local)) = split(id) else { return None };
    const TRAPDOORS: [&str; 7] = [
        "spruce trapdoor",
        "birch trapdoor",
        "jungle trapdoor",
        "acacia trapdoor",
        "dark oak trapdoor",
        "mangrove trapdoor",
        "cherry trapdoor",
    ];
    const BUTTONS: [&str; 8] = [
        "spruce button",
        "birch button",
        "jungle button",
        "acacia button",
        "dark oak button",
        "mangrove button",
        "cherry button",
        "polished blackstone button",
    ];
    const PLATES: [&str; 8] = [
        "spruce pressure plate",
        "birch pressure plate",
        "jungle pressure plate",
        "acacia pressure plate",
        "dark oak pressure plate",
        "mangrove pressure plate",
        "cherry pressure plate",
        "polished blackstone pressure plate",
    ];
    let f = family as usize;
    let (name, layer) = match local {
        0..=15 | 42..=57 => (TRAPDOORS[f], super::tex::TRAPDOORS + family),
        28..=29 => (PLATES[f], planks(family)),
        _ => (BUTTONS[f], planks(family)),
    };
    Some((name, RenderKind::Shaped, [layer; 6]))
}

/// The trapdoor, button and plate of family `f` (default states).
pub const fn trapdoor(family: u16) -> Block {
    Block(FIRST + family * STRIDE)
}
pub const fn button(family: u16) -> Block {
    Block(FIRST + family * STRIDE + 16)
}
pub const fn plate(family: u16) -> Block {
    Block(FIRST + family * STRIDE + 28)
}

/// The family of a switch state.
pub const fn family(id: u16) -> Option<u16> {
    match split(id) {
        Some((family, _)) => Some(family),
        None => None,
    }
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    (0..FAMILIES).flat_map(|f| {
        let ids = [trapdoor(f).0, button(f).0, plate(f).0];
        ids.into_iter().filter(move |&id| f < 7 || id != trapdoor(f).0)
    })
}

/// Display name of a family, for tests and records.
pub const fn family_name(family: u16) -> &'static str {
    NAMES[family as usize]
}
