//! Ids for the stone and wood shapes that follow the deepslate ores.
//!
//! Stone shapes are 253..=332. Thirteen materials take six ids each from
//! 253..=330: four stair facings, a slab and a wall. Cobblestone and stone
//! brick walls, whose stairs and slabs already exist, are 331 and 332.
//!
//! Dark oak, mangrove and cherry logs and planks are 333..=338. Wood shapes
//! are 339..=499, seven woods by 23 ids: four stairs, a slab, a fence, eight
//! fence-gate states (facing × open), eight lower door states and one upper
//! door. The upper door does not store facing or open; its shape reads the
//! lower half. Spruce, birch, jungle and acacia already have logs and planks,
//! so only their shapes are new.
//!
//! Crimson and warped (woods 7 and 8) use the same 23-id layout from
//! `nether_biome_blocks::WOOD_ORIGIN` (1688..=1733).

use super::block::tex;
use super::block::{Block, Facing, Shaped};

pub const STONE_ORIGIN: u16 = 253;
pub const STONE_STRIDE: u16 = 6;
const STONE_MATERIALS: u16 = 13;
pub const COBBLE_WALL: u16 = 331;
pub const BRICK_WALL: u16 = 332;
pub const LOG_ORIGIN: u16 = 333;
pub const WOOD_ORIGIN: u16 = 339;
pub const WOOD_STRIDE: u16 = 23;
/// Woods laid out from [`WOOD_ORIGIN`]; the rest start at `NETHER_WOOD_ORIGIN`.
const OVERWORLD_WOODS: u16 = 7;
const WOODS: u16 = 12;
const EXTRA_WOOD_ORIGIN: u16 = 2200;
const EXTRA_STONE_ORIGIN: u16 = 2360;
const NETHER_WOOD_ORIGIN: u16 = super::nether_biome_blocks::WOOD_ORIGIN;

const STONE_BASES: [Block; 19] = [
    Block::MOSSY_COBBLESTONE,
    Block::GRANITE,
    Block::POLISHED_GRANITE,
    Block::DIORITE,
    Block::POLISHED_DIORITE,
    Block::ANDESITE,
    Block::POLISHED_ANDESITE,
    Block::TUFF,
    Block::CALCITE,
    Block::SMOOTH_STONE,
    Block::DEEPSLATE,
    Block::COBBLED_DEEPSLATE,
    Block::POLISHED_DEEPSLATE,
    Block::COBBLESTONE,
    Block::STONE_BRICKS,
    super::overworld_blocks::MUD_BRICKS,
    super::overworld_blocks::PRISMARINE,
    super::overworld_blocks::PRISMARINE_BRICKS,
    super::overworld_blocks::DARK_PRISMARINE,
];

const STAIR_NAMES: [&str; 19] = [
    "mossy cobblestone stairs",
    "granite stairs",
    "polished granite stairs",
    "diorite stairs",
    "polished diorite stairs",
    "andesite stairs",
    "polished andesite stairs",
    "tuff stairs",
    "calcite stairs",
    "smooth stone stairs",
    "deepslate stairs",
    "cobbled deepslate stairs",
    "polished deepslate stairs",
    "cobblestone stairs",
    "stone brick stairs",
    "mud brick stairs",
    "prismarine stairs",
    "prismarine brick stairs",
    "dark prismarine stairs",
];

const SLAB_NAMES: [&str; 19] = [
    "mossy cobblestone slab",
    "granite slab",
    "polished granite slab",
    "diorite slab",
    "polished diorite slab",
    "andesite slab",
    "polished andesite slab",
    "tuff slab",
    "calcite slab",
    "smooth stone slab",
    "deepslate slab",
    "cobbled deepslate slab",
    "polished deepslate slab",
    "cobblestone slab",
    "stone brick slab",
    "mud brick slab",
    "prismarine slab",
    "prismarine brick slab",
    "dark prismarine slab",
];

const WALL_NAMES: [&str; 19] = [
    "mossy cobblestone wall",
    "granite wall",
    "polished granite wall",
    "diorite wall",
    "polished diorite wall",
    "andesite wall",
    "polished andesite wall",
    "tuff wall",
    "calcite wall",
    "smooth stone wall",
    "deepslate wall",
    "cobbled deepslate wall",
    "polished deepslate wall",
    "cobblestone wall",
    "stone brick wall",
    "mud brick wall",
    "prismarine wall",
    "",
    "",
];

const WOOD_STAIRS: [&str; 12] = [
    "dark oak stairs",
    "spruce stairs",
    "birch stairs",
    "jungle stairs",
    "acacia stairs",
    "mangrove stairs",
    "cherry stairs",
    "crimson stairs",
    "warped stairs",
    "pale oak stairs",
    "bamboo stairs",
    "bamboo mosaic stairs",
];
const WOOD_SLABS: [&str; 12] = [
    "dark oak slab",
    "spruce slab",
    "birch slab",
    "jungle slab",
    "acacia slab",
    "mangrove slab",
    "cherry slab",
    "crimson slab",
    "warped slab",
    "pale oak slab",
    "bamboo slab",
    "bamboo mosaic slab",
];
const WOOD_FENCES: [&str; 12] = [
    "dark oak fence",
    "spruce fence",
    "birch fence",
    "jungle fence",
    "acacia fence",
    "mangrove fence",
    "cherry fence",
    "crimson fence",
    "warped fence",
    "pale oak fence",
    "bamboo fence",
    "bamboo mosaic fence",
];
const WOOD_GATES: [&str; 12] = [
    "dark oak fence gate",
    "spruce fence gate",
    "birch fence gate",
    "jungle fence gate",
    "acacia fence gate",
    "mangrove fence gate",
    "cherry fence gate",
    "crimson fence gate",
    "warped fence gate",
    "pale oak fence gate",
    "bamboo fence gate",
    "bamboo mosaic fence gate",
];
const WOOD_DOORS: [&str; 12] = [
    "dark oak door",
    "spruce door",
    "birch door",
    "jungle door",
    "acacia door",
    "mangrove door",
    "cherry door",
    "crimson door",
    "warped door",
    "pale oak door",
    "bamboo door",
    "bamboo mosaic door",
];

const DOOR_TEX: [(u16, u16); 12] = [
    (tex::DARK_OAK_DOOR_BOTTOM, tex::DARK_OAK_DOOR_TOP),
    (tex::SPRUCE_DOOR_BOTTOM, tex::SPRUCE_DOOR_TOP),
    (tex::BIRCH_DOOR_BOTTOM, tex::BIRCH_DOOR_TOP),
    (tex::JUNGLE_DOOR_BOTTOM, tex::JUNGLE_DOOR_TOP),
    (tex::ACACIA_DOOR_BOTTOM, tex::ACACIA_DOOR_TOP),
    (tex::MANGROVE_DOOR_BOTTOM, tex::MANGROVE_DOOR_TOP),
    (tex::CHERRY_DOOR_BOTTOM, tex::CHERRY_DOOR_TOP),
    (tex::NETHER_DOORS, tex::NETHER_DOORS + 1),
    (tex::NETHER_DOORS + 2, tex::NETHER_DOORS + 3),
    (tex::BUILDING_DOORS, tex::BUILDING_DOORS + 1),
    (tex::BUILDING_DOORS + 2, tex::BUILDING_DOORS + 3),
    (super::overworld_blocks::t::BAMBOO_MOSAIC, super::overworld_blocks::t::BAMBOO_MOSAIC),
];

#[derive(Clone, Copy)]
pub enum StoneForm {
    Stairs {
        index: u16,
        facing: Facing,
    },
    Slab {
        index: u16,
    },
    /// `index` 13 is cobblestone and 14 is stone bricks.
    Wall {
        index: u16,
    },
}

#[derive(Clone, Copy)]
pub enum WoodForm {
    Stairs { index: u16, facing: Facing },
    Slab { index: u16 },
    Fence { index: u16 },
    Gate { index: u16, facing: Facing, open: bool },
    Door { index: u16, facing: Facing, open: bool, upper: bool },
}

pub const fn stone_id(index: u16, local: u16) -> Block {
    if index < 15 {
        Block(STONE_ORIGIN + index * STONE_STRIDE + local)
    } else {
        Block(EXTRA_STONE_ORIGIN + (index - 15) * STONE_STRIDE + local)
    }
}

pub const fn wood_id(index: u16, local: u16) -> Block {
    if index < OVERWORLD_WOODS {
        Block(WOOD_ORIGIN + index * WOOD_STRIDE + local)
    } else if index < 9 {
        Block(NETHER_WOOD_ORIGIN + (index - OVERWORLD_WOODS) * WOOD_STRIDE + local)
    } else {
        Block(EXTRA_WOOD_ORIGIN + (index - 9) * WOOD_STRIDE + local)
    }
}

pub const fn stone_base(index: u16) -> Block {
    STONE_BASES[index as usize]
}

pub const fn wall_material(index: u16) -> Block {
    match index {
        13 => Block::COBBLESTONE,
        14 => Block::STONE_BRICKS,
        _ => STONE_BASES[index as usize],
    }
}

pub const fn wood_planks(index: u16) -> Block {
    match index {
        0 => Block::DARK_OAK_PLANKS,
        1 => Block::SPRUCE_PLANKS,
        2 => Block::BIRCH_PLANKS,
        3 => Block::JUNGLE_PLANKS,
        4 => Block::ACACIA_PLANKS,
        5 => Block::MANGROVE_PLANKS,
        6 => Block::CHERRY_PLANKS,
        7 => super::nether_biome_blocks::CRIMSON_PLANKS,
        8 => super::nether_biome_blocks::WARPED_PLANKS,
        9 => super::overworld_blocks::PALE_OAK_PLANKS,
        10 => super::overworld_blocks::BAMBOO_PLANKS,
        _ => super::overworld_blocks::BAMBOO_MOSAIC,
    }
}

const fn facing_at(local: u16) -> Facing {
    Facing::ALL[(local % 4) as usize]
}

pub const fn stone_form(id: u16) -> Option<StoneForm> {
    if id >= STONE_ORIGIN && id < STONE_ORIGIN + STONE_MATERIALS * STONE_STRIDE {
        let i = id - STONE_ORIGIN;
        let index = i / STONE_STRIDE;
        let local = i % STONE_STRIDE;
        return Some(match local {
            0..=3 => StoneForm::Stairs { index, facing: facing_at(local) },
            4 => StoneForm::Slab { index },
            _ => StoneForm::Wall { index },
        });
    }
    if id >= EXTRA_STONE_ORIGIN && id < EXTRA_STONE_ORIGIN + 4 * STONE_STRIDE {
        let index = 15 + (id - EXTRA_STONE_ORIGIN) / STONE_STRIDE;
        let local = (id - EXTRA_STONE_ORIGIN) % STONE_STRIDE;
        return match local {
            0..=3 => Some(StoneForm::Stairs { index, facing: facing_at(local) }),
            4 => Some(StoneForm::Slab { index }),
            _ if index < 17 => Some(StoneForm::Wall { index }),
            _ => None,
        };
    }
    match id {
        COBBLE_WALL => Some(StoneForm::Wall { index: 13 }),
        BRICK_WALL => Some(StoneForm::Wall { index: 14 }),
        _ => None,
    }
}

pub const fn wood_form(id: u16) -> Option<WoodForm> {
    let (i, first) = if id >= WOOD_ORIGIN && id < WOOD_ORIGIN + OVERWORLD_WOODS * WOOD_STRIDE {
        (id - WOOD_ORIGIN, 0)
    } else if id >= NETHER_WOOD_ORIGIN && id < NETHER_WOOD_ORIGIN + 2 * WOOD_STRIDE {
        (id - NETHER_WOOD_ORIGIN, OVERWORLD_WOODS)
    } else if id >= EXTRA_WOOD_ORIGIN && id < EXTRA_WOOD_ORIGIN + (WOODS - 9) * WOOD_STRIDE {
        (id - EXTRA_WOOD_ORIGIN, 9)
    } else {
        return None;
    };
    let index = first + i / WOOD_STRIDE;
    let local = i % WOOD_STRIDE;
    if index == 11 && local > 4 {
        return None;
    }
    Some(match local {
        0..=3 => WoodForm::Stairs { index, facing: facing_at(local) },
        4 => WoodForm::Slab { index },
        5 => WoodForm::Fence { index },
        6..=13 => WoodForm::Gate { index, facing: facing_at(local - 6), open: local >= 10 },
        14..=21 => WoodForm::Door { index, facing: facing_at(local - 14), open: local >= 18, upper: false },
        _ => WoodForm::Door { index, facing: Facing::South, open: false, upper: true },
    })
}

pub const fn stone_name(form: StoneForm) -> &'static str {
    match form {
        StoneForm::Stairs { index, .. } => STAIR_NAMES[index as usize],
        StoneForm::Slab { index } => SLAB_NAMES[index as usize],
        StoneForm::Wall { index } => WALL_NAMES[index as usize],
    }
}

pub const fn wood_name(form: WoodForm) -> &'static str {
    match form {
        WoodForm::Stairs { index, .. } => WOOD_STAIRS[index as usize],
        WoodForm::Slab { index } => WOOD_SLABS[index as usize],
        WoodForm::Fence { index } => WOOD_FENCES[index as usize],
        WoodForm::Gate { index, .. } => WOOD_GATES[index as usize],
        WoodForm::Door { index, .. } => WOOD_DOORS[index as usize],
    }
}

pub const fn as_shaped(id: u16) -> Option<Shaped> {
    match stone_form(id) {
        Some(StoneForm::Stairs { facing, .. }) => return Some(Shaped::Stairs(facing)),
        Some(StoneForm::Wall { .. }) => return Some(Shaped::Wall),
        Some(StoneForm::Slab { .. }) | None => {}
    }
    match wood_form(id) {
        Some(WoodForm::Stairs { facing, .. }) => Some(Shaped::Stairs(facing)),
        Some(WoodForm::Fence { .. }) => Some(Shaped::Fence),
        Some(WoodForm::Gate { facing, open, .. }) => Some(Shaped::Gate { facing, open }),
        Some(WoodForm::Door { facing, open, upper, .. }) => Some(Shaped::Door { facing, open, upper }),
        Some(WoodForm::Slab { .. }) | None => None,
    }
}

pub const fn door_tex(index: u16, upper: bool) -> u16 {
    let (bottom, top) = DOOR_TEX[index as usize];
    if upper { top } else { bottom }
}

/// Planks a new fence, gate or door is made of.
pub const fn planks_of(block: Block) -> Option<Block> {
    match wood_form(block.0) {
        Some(WoodForm::Fence { index } | WoodForm::Gate { index, .. } | WoodForm::Door { index, .. }) => {
            Some(wood_planks(index))
        }
        _ => None,
    }
}

/// South-facing closed gate, used when placing one.
pub const fn gate_index(block: Block) -> Option<u16> {
    match wood_form(block.0) {
        Some(WoodForm::Gate { index, .. }) => Some(index),
        _ => None,
    }
}

/// Creative palette ids, without the retired forms
/// ([`super::stone_blocks::RETIRED`]).
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (0..STONE_MATERIALS)
        .flat_map(|i| {
            let b = STONE_ORIGIN + i * STONE_STRIDE;
            [b, b + 4, b + 5]
        })
        .filter(|&id| !super::stone_blocks::RETIRED.contains(&Block(id)))
        .chain(
            (15..19).flat_map(|i| {
                [stone_id(i, 0).0, stone_id(i, 4).0].into_iter().chain((i < 17).then(|| stone_id(i, 5).0))
            }),
        )
        .chain((9..12).flat_map(|i| {
            [0, 4, 5, 6, 14]
                .into_iter()
                .filter(move |&local| i != 11 || local <= 4)
                .map(move |local| wood_id(i, local).0)
        }))
        .chain((2..4).flat_map(|wood| {
            [1437, 1140, 1215].map(|oak| super::nether_biome_blocks::switch_reskin(Block(oak), wood).0)
        }))
        .chain([COBBLE_WALL, BRICK_WALL])
        .chain(LOG_ORIGIN..WOOD_ORIGIN)
        .chain((0..OVERWORLD_WOODS).flat_map(|i| {
            let b = WOOD_ORIGIN + i * WOOD_STRIDE;
            // South stairs, slab, fence, closed south gate, closed south door.
            [b, b + 4, b + 5, b + 6, b + 14]
        }))
}
