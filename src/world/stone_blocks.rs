//! v0.12 stone and minerals (BLD-01b): ids 20480..=22527, Java 26.3's
//! tuff, deepslate, quartz, sandstone, nether brick, end stone brick and
//! stone brick building families, blocks of coal and diamond, tinted glass
//! and the slime block. See docs/verification/2026-10-25-v0.12-stone.md.
//!
//! | Ids | States |
//! |---|---|
//! | 20480..=20508 | full blocks in [`FULL_NAMES`] order |
//! | 20509..=20511 | quartz pillar, axis Y, X, Z |
//! | 20512..=21471 | stairs, 80 per family in [`SHAPES`] order |
//! | 21472..=21541 | slabs, 5 per family: bottom, bottom wet, top, top wet, double |
//! | 21552..=21595 | walls, 4 per wall in [`WALLS`] order: UP, UP wet, no UP, no UP wet |
//!
//! Stairs locals follow `slab_stair_states`, `local = ((top * 5 + shape) *
//! 2 + wet) * 4 + facing`, so local 0 (south, bottom, straight, dry) is the
//! item. Stairs and slabs are decoded by `slab_stair_states` as families
//! [`super::slab_stair_states::LEGACY_COUNT`] and up, walls by
//! `wall_states`. The rest of 20480..=22527 stays unregistered for this
//! track.

use super::block::{Block, RenderKind, Shaped};
use super::slab_stair_states::Family;
use crate::item::ToolKind;

pub const ORIGIN: u16 = 20480;
/// Last id of the track's reservation (docs/architecture.md).
pub const RESERVED_LAST: u16 = 22527;

pub const POLISHED_TUFF: Block = Block(ORIGIN);
pub const TUFF_BRICKS: Block = Block(ORIGIN + 1);
pub const CHISELED_TUFF: Block = Block(ORIGIN + 2);
pub const CHISELED_TUFF_BRICKS: Block = Block(ORIGIN + 3);
pub const DEEPSLATE_BRICKS: Block = Block(ORIGIN + 4);
pub const CRACKED_DEEPSLATE_BRICKS: Block = Block(ORIGIN + 5);
pub const DEEPSLATE_TILES: Block = Block(ORIGIN + 6);
pub const CRACKED_DEEPSLATE_TILES: Block = Block(ORIGIN + 7);
pub const CHISELED_DEEPSLATE: Block = Block(ORIGIN + 8);
pub const QUARTZ_BLOCK: Block = Block(ORIGIN + 9);
pub const QUARTZ_BRICKS: Block = Block(ORIGIN + 10);
pub const CHISELED_QUARTZ_BLOCK: Block = Block(ORIGIN + 11);
pub const SMOOTH_QUARTZ: Block = Block(ORIGIN + 12);
pub const RED_SANDSTONE: Block = Block(ORIGIN + 13);
pub const CUT_RED_SANDSTONE: Block = Block(ORIGIN + 14);
pub const CHISELED_RED_SANDSTONE: Block = Block(ORIGIN + 15);
pub const SMOOTH_RED_SANDSTONE: Block = Block(ORIGIN + 16);
pub const CUT_SANDSTONE: Block = Block(ORIGIN + 17);
pub const CHISELED_SANDSTONE: Block = Block(ORIGIN + 18);
pub const SMOOTH_SANDSTONE: Block = Block(ORIGIN + 19);
pub const RED_NETHER_BRICKS: Block = Block(ORIGIN + 20);
pub const CHISELED_NETHER_BRICKS: Block = Block(ORIGIN + 21);
pub const CRACKED_NETHER_BRICKS: Block = Block(ORIGIN + 22);
pub const END_STONE_BRICKS: Block = Block(ORIGIN + 23);
pub const CHISELED_STONE_BRICKS: Block = Block(ORIGIN + 24);
pub const COAL_BLOCK: Block = Block(ORIGIN + 25);
pub const DIAMOND_BLOCK: Block = Block(ORIGIN + 26);
pub const TINTED_GLASS: Block = Block(ORIGIN + 27);
pub const SLIME_BLOCK: Block = Block(ORIGIN + 28);
/// Axis Y; X and Z follow (`RotatedPillarBlock`).
pub const QUARTZ_PILLAR: Block = Block(ORIGIN + 29);

const FULL_COUNT: u16 = 29;
const FULL_NAMES: [&str; FULL_COUNT as usize] = [
    "polished tuff",
    "tuff bricks",
    "chiseled tuff",
    "chiseled tuff bricks",
    "deepslate bricks",
    "cracked deepslate bricks",
    "deepslate tiles",
    "cracked deepslate tiles",
    "chiseled deepslate",
    "block of quartz",
    "quartz bricks",
    "chiseled quartz block",
    "smooth quartz block",
    "red sandstone",
    "cut red sandstone",
    "chiseled red sandstone",
    "smooth red sandstone",
    "cut sandstone",
    "chiseled sandstone",
    "smooth sandstone",
    "red nether bricks",
    "chiseled nether bricks",
    "cracked nether bricks",
    "end stone bricks",
    "chiseled stone bricks",
    "block of coal",
    "block of diamond",
    "tinted glass",
    "slime block",
];

/// Texture layers, offsets from one base so the integrator can repack them.
pub mod t {
    pub const TEX: u16 = 2000;
    pub const POLISHED_TUFF: u16 = TEX;
    pub const TUFF_BRICKS: u16 = TEX + 1;
    pub const CHISELED_TUFF: u16 = TEX + 2;
    pub const CHISELED_TUFF_TOP: u16 = TEX + 3;
    pub const CHISELED_TUFF_BRICKS: u16 = TEX + 4;
    pub const CHISELED_TUFF_BRICKS_TOP: u16 = TEX + 5;
    pub const DEEPSLATE_BRICKS: u16 = TEX + 6;
    pub const CRACKED_DEEPSLATE_BRICKS: u16 = TEX + 7;
    pub const DEEPSLATE_TILES: u16 = TEX + 8;
    pub const CRACKED_DEEPSLATE_TILES: u16 = TEX + 9;
    pub const CHISELED_DEEPSLATE: u16 = TEX + 10;
    pub const QUARTZ_SIDE: u16 = TEX + 11;
    pub const QUARTZ_TOP: u16 = TEX + 12;
    /// Java's `quartz_block_bottom`, also every face of smooth quartz.
    pub const QUARTZ_BOTTOM: u16 = TEX + 13;
    pub const QUARTZ_BRICKS: u16 = TEX + 14;
    pub const CHISELED_QUARTZ: u16 = TEX + 15;
    pub const CHISELED_QUARTZ_TOP: u16 = TEX + 16;
    pub const QUARTZ_PILLAR_SIDE: u16 = TEX + 17;
    pub const QUARTZ_PILLAR_TOP: u16 = TEX + 18;
    pub const RED_SANDSTONE: u16 = TEX + 19;
    /// Also every face of smooth red sandstone.
    pub const RED_SANDSTONE_TOP: u16 = TEX + 20;
    pub const RED_SANDSTONE_BOTTOM: u16 = TEX + 21;
    pub const CUT_RED_SANDSTONE: u16 = TEX + 22;
    pub const CHISELED_RED_SANDSTONE: u16 = TEX + 23;
    pub const CUT_SANDSTONE: u16 = TEX + 24;
    pub const CHISELED_SANDSTONE: u16 = TEX + 25;
    pub const RED_NETHER_BRICKS: u16 = TEX + 26;
    pub const CHISELED_NETHER_BRICKS: u16 = TEX + 27;
    pub const CRACKED_NETHER_BRICKS: u16 = TEX + 28;
    pub const END_STONE_BRICKS: u16 = TEX + 29;
    pub const CHISELED_STONE_BRICKS: u16 = TEX + 30;
    pub const COAL_BLOCK: u16 = TEX + 31;
    pub const DIAMOND_BLOCK: u16 = TEX + 32;
    pub const TINTED_GLASS: u16 = TEX + 33;
    pub const SLIME_BLOCK: u16 = TEX + 34;
    /// The pillar side turned a quarter, for faces whose grain runs across.
    pub const QUARTZ_PILLAR_SIDE_TURNED: u16 = TEX + 35;
    /// One past the last layer; the reservation ends at 2127.
    pub const END: u16 = TEX + 36;
    const _: () = assert!(END <= 2128);
}

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

const fn column(side: u16, top: u16, bottom: u16) -> [u16; 6] {
    [side, side, top, bottom, side, side]
}

/// Faces of full block `i` (+X, -X, +Y, -Y, +Z, -Z).
const fn full_tex(i: u16) -> [u16; 6] {
    use super::block::tex;
    match i {
        0 => all(t::POLISHED_TUFF),
        1 => all(t::TUFF_BRICKS),
        2 => column(t::CHISELED_TUFF, t::CHISELED_TUFF_TOP, t::CHISELED_TUFF_TOP),
        3 => column(t::CHISELED_TUFF_BRICKS, t::CHISELED_TUFF_BRICKS_TOP, t::CHISELED_TUFF_BRICKS_TOP),
        4 => all(t::DEEPSLATE_BRICKS),
        5 => all(t::CRACKED_DEEPSLATE_BRICKS),
        6 => all(t::DEEPSLATE_TILES),
        7 => all(t::CRACKED_DEEPSLATE_TILES),
        8 => all(t::CHISELED_DEEPSLATE),
        9 => column(t::QUARTZ_SIDE, t::QUARTZ_TOP, t::QUARTZ_TOP),
        10 => all(t::QUARTZ_BRICKS),
        11 => column(t::CHISELED_QUARTZ, t::CHISELED_QUARTZ_TOP, t::CHISELED_QUARTZ_TOP),
        12 => all(t::QUARTZ_BOTTOM),
        13 => column(t::RED_SANDSTONE, t::RED_SANDSTONE_TOP, t::RED_SANDSTONE_BOTTOM),
        14 => column(t::CUT_RED_SANDSTONE, t::RED_SANDSTONE_TOP, t::RED_SANDSTONE_TOP),
        15 => column(t::CHISELED_RED_SANDSTONE, t::RED_SANDSTONE_TOP, t::RED_SANDSTONE_TOP),
        16 => all(t::RED_SANDSTONE_TOP),
        17 => column(t::CUT_SANDSTONE, tex::SANDSTONE_TOP, tex::SANDSTONE_TOP),
        18 => column(t::CHISELED_SANDSTONE, tex::SANDSTONE_TOP, tex::SANDSTONE_TOP),
        19 => all(tex::SANDSTONE_TOP),
        20 => all(t::RED_NETHER_BRICKS),
        21 => all(t::CHISELED_NETHER_BRICKS),
        22 => all(t::CRACKED_NETHER_BRICKS),
        23 => all(t::END_STONE_BRICKS),
        24 => all(t::CHISELED_STONE_BRICKS),
        25 => all(t::COAL_BLOCK),
        26 => all(t::DIAMOND_BLOCK),
        27 => all(t::TINTED_GLASS),
        _ => all(t::SLIME_BLOCK),
    }
}

/// Quartz pillar faces for axis 0 (Y), 1 (X) or 2 (Z). The grain runs
/// along the axis, so faces it crosses use the turned side.
const fn pillar_tex(axis: u16) -> [u16; 6] {
    let (side, turned, end) = (t::QUARTZ_PILLAR_SIDE, t::QUARTZ_PILLAR_SIDE_TURNED, t::QUARTZ_PILLAR_TOP);
    match axis {
        1 => [end, end, turned, turned, turned, turned],
        2 => [turned, turned, side, side, end, end],
        _ => [side, side, end, end, side, side],
    }
}

// Stairs and slab families.

pub const STAIRS_FIRST: u16 = ORIGIN + 32;
const STAIRS_STRIDE: u16 = 80;
/// Families with stairs; the slab-only families follow them.
pub const STAIR_FAMILIES: u16 = 12;
pub const SLABS_FIRST: u16 = STAIRS_FIRST + STAIR_FAMILIES * STAIRS_STRIDE;
const SLABS_STRIDE: u16 = 5;
pub const SLAB_FAMILIES: u16 = 14;
pub const SLABS_END: u16 = SLABS_FIRST + SLAB_FAMILIES * SLABS_STRIDE;

const fn shape(base: Block, i: u16) -> Family {
    Family {
        base,
        stairs: if i < STAIR_FAMILIES { Block(STAIRS_FIRST + i * STAIRS_STRIDE) } else { Block::AIR },
        slab: Block(SLABS_FIRST + i * SLABS_STRIDE),
    }
}

/// Stairs/slab families, appended to `slab_stair_states::FAMILIES`. The
/// last two (cut sandstone and cut red sandstone) have only a slab; their
/// `stairs` is air.
pub const SHAPES: [Family; SLAB_FAMILIES as usize] = [
    shape(POLISHED_TUFF, 0),
    shape(TUFF_BRICKS, 1),
    shape(DEEPSLATE_BRICKS, 2),
    shape(DEEPSLATE_TILES, 3),
    shape(QUARTZ_BLOCK, 4),
    shape(SMOOTH_QUARTZ, 5),
    shape(RED_SANDSTONE, 6),
    shape(SMOOTH_RED_SANDSTONE, 7),
    shape(SMOOTH_SANDSTONE, 8),
    shape(RED_NETHER_BRICKS, 9),
    shape(END_STONE_BRICKS, 10),
    shape(Block::MOSSY_STONE_BRICKS, 11),
    shape(CUT_SANDSTONE, 12),
    shape(CUT_RED_SANDSTONE, 13),
];

const SHAPE_NAMES: [(&str, &str); SLAB_FAMILIES as usize] = [
    ("polished tuff stairs", "polished tuff slab"),
    ("tuff brick stairs", "tuff brick slab"),
    ("deepslate brick stairs", "deepslate brick slab"),
    ("deepslate tile stairs", "deepslate tile slab"),
    ("quartz stairs", "quartz slab"),
    ("smooth quartz stairs", "smooth quartz slab"),
    ("red sandstone stairs", "red sandstone slab"),
    ("smooth red sandstone stairs", "smooth red sandstone slab"),
    ("smooth sandstone stairs", "smooth sandstone slab"),
    ("red nether brick stairs", "red nether brick slab"),
    ("end stone brick stairs", "end stone brick slab"),
    ("mossy stone brick stairs", "mossy stone brick slab"),
    ("", "cut sandstone slab"),
    ("", "cut red sandstone slab"),
];

/// Family (relative to [`SHAPES`]) and local index of a stairs id.
pub const fn stairs_local(id: u16) -> Option<(u16, u16)> {
    if id >= STAIRS_FIRST && id < SLABS_FIRST {
        let i = id - STAIRS_FIRST;
        Some((i / STAIRS_STRIDE, i % STAIRS_STRIDE))
    } else {
        None
    }
}

/// Family and local index (0 bottom, 1 bottom wet, 2 top, 3 top wet, 4
/// double) of a slab id.
pub const fn slab_local(id: u16) -> Option<(u16, u16)> {
    if id >= SLABS_FIRST && id < SLABS_END {
        let i = id - SLABS_FIRST;
        Some((i / SLABS_STRIDE, i % SLABS_STRIDE))
    } else {
        None
    }
}

pub const fn stairs_at(family: u16, local: u16) -> Block {
    Block(STAIRS_FIRST + family * STAIRS_STRIDE + local)
}

pub const fn slab_at(family: u16, local: u16) -> Block {
    Block(SLABS_FIRST + family * SLABS_STRIDE + local)
}

/// Whether `id` is one of this module's stairs or slab states.
#[inline(always)]
pub const fn owns_shape(id: u16) -> bool {
    id >= STAIRS_FIRST && id < SLABS_END
}

// Walls.

pub const WALLS_FIRST: u16 = ORIGIN + 1072;
pub const WALL_COUNT: u16 = 11;
pub const WALLS_END: u16 = WALLS_FIRST + WALL_COUNT * 4;
const _: () = assert!(SLABS_END <= WALLS_FIRST && WALLS_END - 1 <= RESERVED_LAST);

/// The blocks walls are built from, in id order.
pub const WALLS: [Block; WALL_COUNT as usize] = [
    POLISHED_TUFF,
    TUFF_BRICKS,
    DEEPSLATE_BRICKS,
    DEEPSLATE_TILES,
    RED_SANDSTONE,
    RED_NETHER_BRICKS,
    END_STONE_BRICKS,
    Block::MOSSY_STONE_BRICKS,
    Block::SANDSTONE,
    Block::NETHER_BRICKS,
    Block::BRICKS,
];

const WALL_NAMES: [&str; WALL_COUNT as usize] = [
    "polished tuff wall",
    "tuff brick wall",
    "deepslate brick wall",
    "deepslate tile wall",
    "red sandstone wall",
    "red nether brick wall",
    "end stone brick wall",
    "mossy stone brick wall",
    "sandstone wall",
    "nether brick wall",
    "brick wall",
];

/// The UP, dry state of wall `i`: its item and default state.
pub const fn wall(i: u16) -> Block {
    Block(WALLS_FIRST + i * 4)
}

/// Wall index and local state (UP dry, UP wet, no-UP dry, no-UP wet).
#[inline]
pub const fn wall_local(id: u16) -> Option<(u16, u16)> {
    if id >= WALLS_FIRST && id < WALLS_END { Some(((id - WALLS_FIRST) / 4, (id - WALLS_FIRST) % 4)) } else { None }
}

/// The wet partner of a wall state and whether `id` is the wet one.
pub const fn wall_water_pair(id: u16) -> Option<(u16, bool)> {
    match wall_local(id) {
        Some((_, local)) => Some((id ^ 1, local % 2 == 1)),
        None => None,
    }
}

/// The dry state a wet wall state shares its properties with.
#[inline]
pub const fn wall_canonical(id: u16) -> Option<u16> {
    match wall_local(id) {
        Some((_, local)) if local % 2 == 1 => Some(id - 1),
        _ => None,
    }
}

// Registry.

/// Where a state's textures come from: its own faces or another block's.
#[derive(Clone, Copy)]
pub enum Tex {
    Faces([u16; 6]),
    Like(Block),
}

/// Name, render kind and textures of every registered state.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, Tex)> {
    use RenderKind::*;
    if id >= ORIGIN && id < ORIGIN + FULL_COUNT {
        let i = id - ORIGIN;
        let kind = if id == TINTED_GLASS.0 || id == SLIME_BLOCK.0 { Translucent } else { Opaque };
        return Some((FULL_NAMES[i as usize], kind, Tex::Faces(full_tex(i))));
    }
    if id >= QUARTZ_PILLAR.0 && id < QUARTZ_PILLAR.0 + 3 {
        return Some(("quartz pillar", Opaque, Tex::Faces(pillar_tex(id - QUARTZ_PILLAR.0))));
    }
    if let Some((f, _)) = stairs_local(id) {
        return Some((SHAPE_NAMES[f as usize].0, Shaped, Tex::Like(SHAPES[f as usize].base)));
    }
    if let Some((f, local)) = slab_local(id) {
        // As `slab_stair_states::registry`: a bottom slab is a low cutout
        // block, a top slab a shaped half and a double slab a full cube.
        let kind = match local {
            0 | 1 => Cutout,
            2 | 3 => Shaped,
            _ => Opaque,
        };
        return Some((SHAPE_NAMES[f as usize].1, kind, Tex::Like(SHAPES[f as usize].base)));
    }
    if let Some((w, _)) = wall_local(id) {
        return Some((WALL_NAMES[w as usize], Shaped, Tex::Like(WALLS[w as usize])));
    }
    None
}

/// Translucent full blocks that still collide (`make`'s `solid`).
pub const fn translucent_solid(id: u16) -> bool {
    id == TINTED_GLASS.0 || id == SLIME_BLOCK.0
}

/// `HalfTransparentBlock.skipRendering`: faces between two of the same block hide.
pub const fn self_cull(id: u16) -> bool {
    id == TINTED_GLASS.0 || id == SLIME_BLOCK.0
}

/// Light dampening overrides: `TintedGlassBlock.getLightDampening` is 15
/// and it never propagates skylight down.
pub const fn opacity(id: u16) -> Option<u8> {
    if id == TINTED_GLASS.0 { Some(15) } else { None }
}

pub const fn shaped(id: u16) -> Option<Shaped> {
    if wall_local(id).is_some() { Some(Shaped::Wall) } else { None }
}

pub const fn wall_base(id: u16) -> Option<Block> {
    match wall_local(id) {
        Some((w, _)) => Some(WALLS[w as usize]),
        None => None,
    }
}

/// The item form of a full block: the pillar's Y axis.
pub const fn base(id: u16) -> Option<Block> {
    if id > QUARTZ_PILLAR.0 && id < QUARTZ_PILLAR.0 + 3 { Some(QUARTZ_PILLAR) } else { None }
}

/// Whether the block has an axis that `Block::with_facing` must keep.
pub const fn is_pillar(block: Block) -> bool {
    block.0 >= QUARTZ_PILLAR.0 && block.0 < QUARTZ_PILLAR.0 + 3
}

/// `RotatedPillarBlock.getStateForPlacement`: the axis of the clicked face.
pub fn axis_placed(block: Block, normal: glam::IVec3) -> Option<Block> {
    is_pillar(block).then(|| {
        Block(
            QUARTZ_PILLAR.0
                + if normal.x != 0 {
                    1
                } else if normal.z != 0 {
                    2
                } else {
                    0
                },
        )
    })
}

/// Hardness, preferred tool and pickaxe tier for the full blocks (Java
/// 26.3 `Blocks`; stairs, slabs and walls copy their base through
/// `Block::material`). Every stone block requires a pickaxe of any tier.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    let id = b.0;
    if !(ORIGIN..ORIGIN + 32).contains(&id) {
        return None;
    }
    let pick = Some(ToolKind::Pickaxe);
    Some(match b {
        TINTED_GLASS => (0.3, None, None),
        SLIME_BLOCK => (0.0, None, None),
        COAL_BLOCK => (5.0, pick, Some(0)),
        DIAMOND_BLOCK => (5.0, pick, Some(2)),
        POLISHED_TUFF | TUFF_BRICKS | CHISELED_TUFF | CHISELED_TUFF_BRICKS | CHISELED_STONE_BRICKS => {
            (1.5, pick, Some(0))
        }
        // `ofLegacyCopy(COBBLED_DEEPSLATE)`.
        DEEPSLATE_BRICKS
        | CRACKED_DEEPSLATE_BRICKS
        | DEEPSLATE_TILES
        | CRACKED_DEEPSLATE_TILES
        | CHISELED_DEEPSLATE => (3.5, pick, Some(0)),
        SMOOTH_QUARTZ | SMOOTH_SANDSTONE | SMOOTH_RED_SANDSTONE => (2.0, pick, Some(0)),
        RED_NETHER_BRICKS | CHISELED_NETHER_BRICKS | CRACKED_NETHER_BRICKS => (2.0, pick, Some(0)),
        END_STONE_BRICKS => (3.0, pick, Some(0)),
        // Quartz blocks and pillar, quartz bricks, the sandstones.
        _ => (0.8, pick, Some(0)),
    })
}

/// Slabs whose destroy time Java sets apart from their base
/// (`registerSlab(id, base, 2.0F)`): sandstone, cut sandstone, quartz, red
/// sandstone and cut red sandstone slabs take 2.0 where the block takes 0.8.
pub fn slab_hardness(b: Block) -> Option<f32> {
    let base = super::slab_stair_states::slab(b).map(|s| super::slab_stair_states::FAMILIES[s.family as usize].base)?;
    matches!(base, Block::SANDSTONE | CUT_SANDSTONE | QUARTZ_BLOCK | RED_SANDSTONE | CUT_RED_SANDSTONE).then_some(2.0)
}

/// Java 26.3 smelting recipes producing this track's blocks (all 0.1
/// experience, 200 ticks), plus stone bricks to cracked stone bricks.
pub fn smelt(item: crate::item::Item) -> Option<crate::item::Item> {
    use crate::item::Item;
    let out = match item.block()? {
        DEEPSLATE_BRICKS => CRACKED_DEEPSLATE_BRICKS,
        DEEPSLATE_TILES => CRACKED_DEEPSLATE_TILES,
        Block::NETHER_BRICKS => CRACKED_NETHER_BRICKS,
        Block::SANDSTONE => SMOOTH_SANDSTONE,
        RED_SANDSTONE => SMOOTH_RED_SANDSTONE,
        QUARTZ_BLOCK => SMOOTH_QUARTZ,
        Block::STONE_BRICKS => Block::CRACKED_STONE_BRICKS,
        _ => return None,
    };
    Some(Item::from_block(out))
}

/// Retired v0.7 forms with no Java counterpart (owner, 2026-10-25): calcite
/// stairs, slab and wall; deepslate stairs, slab and wall; smooth stone
/// stairs and wall; polished granite, diorite and andesite walls. Placed
/// blocks keep rendering and dropping themselves; their ids are never
/// reused, but no recipe or creative tab offers them.
pub const RETIRED: [Block; 11] = {
    use super::forms::stone_id;
    [
        stone_id(8, 0),
        stone_id(8, 4),
        stone_id(8, 5),
        stone_id(10, 0),
        stone_id(10, 4),
        stone_id(10, 5),
        stone_id(9, 0),
        stone_id(9, 5),
        stone_id(2, 5),
        stone_id(4, 5),
        stone_id(6, 5),
    ]
};

pub fn is_retired(block: Block) -> bool {
    RETIRED.contains(&block.base())
}

/// Creative palette order: full blocks with their shapes, then walls.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (ORIGIN..ORIGIN + 30)
        .chain(
            (0..SLAB_FAMILIES)
                .flat_map(|f| (f < STAIR_FAMILIES).then(|| stairs_at(f, 0).0).into_iter().chain([slab_at(f, 0).0])),
        )
        .chain((0..WALL_COUNT).map(|w| wall(w).0))
}

#[cfg(test)]
mod tests;
