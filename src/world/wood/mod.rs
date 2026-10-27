//! v0.12 wood track (BLD-01a, BLD-04): axis states for the overworld logs
//! and the bamboo block, `*_wood`, stripped logs and wood, the missing
//! trapdoors, buttons and pressure plates, signs, shelves and the chiseled
//! bookshelf. Ids 16384..=20479 (the track's reservation in
//! `docs/architecture.md`); one id per Java state, waterlogged states
//! included.
//!
//! | Ids | Use |
//! | --- | --- |
//! | 16384..=16403 | X then Z axis of the ten [`LOGS`] (their existing ids mean Y) |
//! | 16416..=16496 | wood, stripped log and stripped wood of each [`Wood`], Y/X/Z: `16416 + 9 * wood + 3 * kind + axis` |
//! | 16497..=16499 | block of stripped bamboo, Y/X/Z |
//! | 16512..=17023 | trapdoors, buttons and plates: 64 per [`switches`] family |
//! | 17024..=18559 | signs: 128 per [`Kind`] (see [`signs`]) |
//! | 18560..=19327 | shelves: 64 per [`Kind`] (see [`shelves`]) |
//! | 19328..=19583 | chiseled bookshelf: `19328 + 64 * facing + slots` |
//!
//! Every other id in the range stays unregistered.

use glam::IVec3;

use super::block::{Block, Facing, RenderKind, Wood};
use crate::item::{Item, ToolKind};

pub mod bookshelf;
pub mod entities;
#[cfg(test)]
mod entity_tests;
pub mod shelves;
pub mod signs;
pub mod switches;
#[cfg(test)]
mod tests;
mod world;

pub use world::ShelfUse;

pub const FIRST: u16 = 16384;
pub const LAST: u16 = 20479;

/// Texture layers of this track, as offsets from one base the integrator
/// may move (texture layers are never saved).
pub mod tex {
    pub const BASE: u16 = 1776;
    /// Stripped log sides: the nine [`super::Wood`]s, then bamboo.
    pub const STRIPPED_SIDE: u16 = BASE;
    /// Stripped log ends, in the same order.
    pub const STRIPPED_TOP: u16 = BASE + 10;
    /// Log sides turned a quarter, for X and Z axis faces: the ten [`super::LOGS`].
    pub const LOG_SIDE_TURNED: u16 = BASE + 20;
    /// Stripped sides turned a quarter, in [`STRIPPED_SIDE`] order.
    pub const STRIPPED_SIDE_TURNED: u16 = BASE + 30;
    /// Trapdoors of the seven wood [`super::switches`] families.
    pub const TRAPDOORS: u16 = BASE + 40;
    /// Flat item sprites of the signs, by [`super::Kind`].
    pub const SIGN_ICONS: u16 = BASE + 47;
    /// Flat item sprites of the hanging signs, by [`super::Kind`].
    pub const HANGING_SIGN_ICONS: u16 = BASE + 59;
    /// Shelf body (sides, top, boards) by [`super::Kind`].
    pub const SHELF: u16 = BASE + 71;
    /// Shelf fronts, five per kind: unpowered, then powered unconnected,
    /// left, centre and right (as seen from the front).
    pub const SHELF_FRONTS: u16 = BASE + 83;
    pub const CHISELED_TOP: u16 = BASE + 143;
    pub const CHISELED_SIDE: u16 = BASE + 144;
    /// Chiseled bookshelf fronts, one per set of filled slots (bit `i` is
    /// slot `i`, as seen from the front).
    pub const CHISELED_FRONTS: u16 = BASE + 145;
    pub const END: u16 = BASE + 209;
    const _: () = assert!(END <= 2000, "the wood track owns layers 1776..=1999");
}

// ---- Woods -------------------------------------------------------------

/// The twelve sign and shelf woods: the nine overworld [`Wood`]s, bamboo,
/// crimson and warped.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Oak,
    Spruce,
    Birch,
    Jungle,
    Acacia,
    DarkOak,
    Mangrove,
    Cherry,
    PaleOak,
    Bamboo,
    Crimson,
    Warped,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Oak,
        Kind::Spruce,
        Kind::Birch,
        Kind::Jungle,
        Kind::Acacia,
        Kind::DarkOak,
        Kind::Mangrove,
        Kind::Cherry,
        Kind::PaleOak,
        Kind::Bamboo,
        Kind::Crimson,
        Kind::Warped,
    ];

    pub const fn index(self) -> u16 {
        self as u16
    }

    pub const fn name(self) -> &'static str {
        [
            "oak", "spruce", "birch", "jungle", "acacia", "dark oak", "mangrove", "cherry", "pale oak", "bamboo",
            "crimson", "warped",
        ][self as usize]
    }

    pub const fn planks(self) -> Block {
        match self {
            Kind::Bamboo => super::overworld_blocks::BAMBOO_PLANKS,
            Kind::Crimson => super::nether_biome_blocks::CRIMSON_PLANKS,
            Kind::Warped => super::nether_biome_blocks::WARPED_PLANKS,
            _ => Wood::ALL[self as usize].planks(),
        }
    }

    /// The stripped log the kind's hanging signs and shelves are made of.
    pub const fn stripped_log(self) -> Block {
        match self {
            Kind::Bamboo => Block(STRIPPED_BAMBOO),
            Kind::Crimson => super::nether_biome_blocks::STRIPPED_CRIMSON_STEM,
            Kind::Warped => super::nether_biome_blocks::STRIPPED_WARPED_STEM,
            _ => pillar(self as u16, Pillar::StrippedLog, 0),
        }
    }

    /// Crimson and warped never burn (Java's `ignitedByLava` and fuel tags leave them out).
    pub const fn burns(self) -> bool {
        !matches!(self, Kind::Crimson | Kind::Warped)
    }

    pub const fn of(index: u16) -> Kind {
        Kind::ALL[index as usize]
    }
}

// ---- Logs, wood and stripped blocks -------------------------------------

/// The overworld logs and the bamboo block, whose existing ids are their
/// Y-axis states.
pub const LOGS: [Block; 10] = [
    Block::LOG,
    Block::SPRUCE_LOG,
    Block::BIRCH_LOG,
    Block::JUNGLE_LOG,
    Block::ACACIA_LOG,
    Block::DARK_OAK_LOG,
    Block::MANGROVE_LOG,
    Block::CHERRY_LOG,
    super::overworld_blocks::PALE_OAK_LOG,
    super::overworld_blocks::BAMBOO_BLOCK,
];
const AXIS_FIRST: u16 = 16384;
const PILLAR_FIRST: u16 = 16416;
/// Block of stripped bamboo, Y axis (X and Z follow).
pub const STRIPPED_BAMBOO: u16 = 16497;
const PILLAR_END: u16 = 16500;

/// The three new pillar blocks of each overworld wood.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pillar {
    Wood,
    StrippedLog,
    StrippedWood,
}

/// The `kind` block of `Wood::ALL[wood]` on `axis` (0 Y, 1 X, 2 Z).
pub const fn pillar(wood: u16, kind: Pillar, axis: u16) -> Block {
    Block(PILLAR_FIRST + wood * 9 + kind as u16 * 3 + axis)
}

/// Index into [`LOGS`] of a log's Y-axis id.
const fn log_index(id: u16) -> Option<u16> {
    let mut i = 0;
    while i < LOGS.len() {
        if LOGS[i].0 == id {
            return Some(i as u16);
        }
        i += 1;
    }
    None
}

/// What a pillar id is: (Y-axis id, axis, wood or log index, stripped).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PillarState {
    y: u16,
    axis: u16,
    /// Index into [`LOGS`] (bamboo is 9).
    index: u16,
    /// 0 log, 1 wood, 2 stripped log, 3 stripped wood.
    form: u16,
}

const fn pillar_state(id: u16) -> Option<PillarState> {
    if id >= AXIS_FIRST && id < AXIS_FIRST + 2 * LOGS.len() as u16 {
        let index = (id - AXIS_FIRST) / 2;
        return Some(PillarState { y: LOGS[index as usize].0, axis: 1 + (id - AXIS_FIRST) % 2, index, form: 0 });
    }
    if id >= PILLAR_FIRST && id < STRIPPED_BAMBOO {
        let i = id - PILLAR_FIRST;
        let axis = i % 3;
        return Some(PillarState { y: id - axis, axis, index: i / 9, form: 1 + i % 9 / 3 });
    }
    if id >= STRIPPED_BAMBOO && id < PILLAR_END {
        return Some(PillarState { y: STRIPPED_BAMBOO, axis: id - STRIPPED_BAMBOO, index: 9, form: 2 });
    }
    match log_index(id) {
        Some(index) => Some(PillarState { y: id, axis: 0, index, form: 0 }),
        None => None,
    }
}

/// `y` (a Y-axis pillar id) on `axis`.
const fn pillar_on(y: u16, axis: u16) -> u16 {
    if axis == 0 {
        return y;
    }
    match log_index(y) {
        Some(index) => AXIS_FIRST + index * 2 + axis - 1,
        None => y + axis,
    }
}

/// Textures of a pillar state: `end` on the two faces its axis points out
/// of; sides on the other faces, turned a quarter where Java's model turns
/// the grain to run along the axis.
const fn pillar_tex(axis: u16, side: u16, turned: u16, end: u16) -> [u16; 6] {
    match axis {
        1 => [end, end, turned, turned, turned, turned],
        2 => [turned, turned, side, side, end, end],
        _ => [side, side, end, end, side, side],
    }
}

/// Name, kind and textures of the logs' X/Z states, the wood and stripped blocks.
const fn pillar_registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some(s) = pillar_state(id) else { return None };
    if s.form == 0 {
        // A log on its side: the Y-axis log's textures moved round.
        let y = super::block::make(s.y);
        let t = pillar_tex(s.axis, y.tex[0], tex::LOG_SIDE_TURNED + s.index, y.tex[2]);
        return Some((y.name, RenderKind::Opaque, t));
    }
    const WOODS: [&str; 9] = [
        "oak wood",
        "spruce wood",
        "birch wood",
        "jungle wood",
        "acacia wood",
        "dark oak wood",
        "mangrove wood",
        "cherry wood",
        "pale oak wood",
    ];
    const STRIPPED_LOGS: [&str; 10] = [
        "stripped oak log",
        "stripped spruce log",
        "stripped birch log",
        "stripped jungle log",
        "stripped acacia log",
        "stripped dark oak log",
        "stripped mangrove log",
        "stripped cherry log",
        "stripped pale oak log",
        "block of stripped bamboo",
    ];
    const STRIPPED_WOODS: [&str; 9] = [
        "stripped oak wood",
        "stripped spruce wood",
        "stripped birch wood",
        "stripped jungle wood",
        "stripped acacia wood",
        "stripped dark oak wood",
        "stripped mangrove wood",
        "stripped cherry wood",
        "stripped pale oak wood",
    ];
    let i = s.index as usize;
    let bark = super::block::make(LOGS[i].0).tex[0];
    let stripped = tex::STRIPPED_SIDE + s.index;
    let stripped_turned = tex::STRIPPED_SIDE_TURNED + s.index;
    Some(match s.form {
        // Java's wood models put the bark on every face.
        1 => (WOODS[i], RenderKind::Opaque, pillar_tex(s.axis, bark, tex::LOG_SIDE_TURNED + s.index, bark)),
        2 => (
            STRIPPED_LOGS[i],
            RenderKind::Opaque,
            pillar_tex(s.axis, stripped, stripped_turned, tex::STRIPPED_TOP + s.index),
        ),
        _ => (STRIPPED_WOODS[i], RenderKind::Opaque, pillar_tex(s.axis, stripped, stripped_turned, stripped)),
    })
}

/// Logs, wood and stripped blocks take the axis of the clicked face
/// (Java's `RotatedPillarBlock.getStateForPlacement`).
pub fn axis_placed(block: Block, normal: IVec3) -> Option<Block> {
    let s = pillar_state(block.0)?;
    let axis = if normal.x != 0 {
        1
    } else if normal.z != 0 {
        2
    } else {
        0
    };
    Some(Block(pillar_on(s.y, axis)))
}

/// The axis of a pillar state (0 Y, 1 X, 2 Z).
pub fn axis(block: Block) -> Option<u16> {
    pillar_state(block.0).map(|s| s.axis)
}

/// What an axe strips a log, wood or bamboo block into, keeping its axis
/// (Java 26.3 `data/minecraft/block_transformer/axe.json`).
pub fn stripped(block: Block) -> Option<Block> {
    let s = pillar_state(block.0)?;
    let y = match (s.form, s.index) {
        (0, 9) => STRIPPED_BAMBOO,
        (0, i) => pillar(i, Pillar::StrippedLog, 0).0,
        (1, i) => pillar(i, Pillar::StrippedWood, 0).0,
        _ => return None,
    };
    Some(Block(pillar_on(y, s.axis)))
}

/// Java's `#logs` members of this module (wood and stripped forms; the
/// bamboo blocks are `#bamboo_blocks` instead).
pub fn is_log(block: Block) -> bool {
    pillar_state(block.0).is_some_and(|s| s.form > 0 && s.index < 9)
}

/// Java's `#jungle_logs`: jungle log, wood and their stripped forms.
pub fn is_jungle_log(block: Block) -> bool {
    pillar_state(block.0).is_some_and(|s| s.index == 3)
}

/// The log tag of each overworld wood (`#oak_logs` and the rest): log,
/// wood, stripped log and stripped wood, the log first so guides show it.
pub const LOG_TAGS: [[Item; 4]; 9] = {
    let mut out = [[Item(0); 4]; 9];
    let mut w = 0;
    while w < 9 {
        out[w] = [
            Item::from_block(LOGS[w]),
            Item::from_block(pillar(w as u16, Pillar::Wood, 0)),
            Item::from_block(pillar(w as u16, Pillar::StrippedLog, 0)),
            Item::from_block(pillar(w as u16, Pillar::StrippedWood, 0)),
        ];
        w += 1;
    }
    out
};

/// `#bamboo_blocks`.
pub const BAMBOO_BLOCKS: [Item; 2] =
    [Item::from_block(super::overworld_blocks::BAMBOO_BLOCK), Item::from_block(Block(STRIPPED_BAMBOO))];

/// Every overworld log, wood and stripped block (`#logs_that_burn`).
pub const ALL_LOGS: [Item; 36] = {
    let mut out = [Item(0); 36];
    let mut i = 0;
    while i < 36 {
        out[i] = LOG_TAGS[i % 9][i / 9];
        i += 1;
    }
    out
};

// ---- Shared registry hooks ----------------------------------------------

/// Whether `id` is in this track's reservation.
#[inline(always)]
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// Name, render kind and face textures of this module's states.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    if !owns(id) {
        return None;
    }
    if let Some(r) = pillar_registry(id) {
        return Some(r);
    }
    if let Some(r) = switches::registry(id) {
        return Some(r);
    }
    if let Some(r) = signs::registry(id) {
        return Some(r);
    }
    if let Some(r) = shelves::registry(id) {
        return Some(r);
    }
    bookshelf::registry(id)
}

/// A state whose properties are another's: a log on its side answers as its
/// Y-axis log, and a wood or stripped block as its Y-axis state.
#[inline]
pub const fn alias(id: u16) -> Option<u16> {
    if !(id >= AXIS_FIRST && id < PILLAR_END) {
        return None;
    }
    match pillar_state(id) {
        Some(s) if s.axis != 0 => Some(s.y),
        _ => None,
    }
}

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet one.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if !owns(id) {
        return None;
    }
    if let Some(p) = switches::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = signs::water_pair(id) {
        return Some(p);
    }
    shelves::water_pair(id)
}

/// The dry state of a wet state of this module (`id` itself otherwise).
#[inline]
pub const fn canonical(id: u16) -> u16 {
    match water_pair(id) {
        Some((dry, true)) => dry,
        _ => id,
    }
}

/// The block items, recipes and rules use: one state per block.
pub fn base(id: u16) -> Option<Block> {
    if !owns(id) {
        return None;
    }
    if let Some(s) = pillar_state(id) {
        return Some(Block(s.y));
    }
    signs::base(id).or_else(|| shelves::base(id)).or_else(|| bookshelf::base(id))
}

/// Mining, fire and drop properties of this module's own blocks (switch
/// twins answer as their oak or stone originals).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Props {
    pub hardness: f32,
    pub tool: Option<ToolKind>,
    /// Java `FireBlock` (encouragement, consumption).
    pub fire: (u8, u8),
    pub lava: bool,
    /// Survival drop; `None` drops nothing (silk touch aside).
    pub drop: Option<Item>,
}

pub fn props(block: Block) -> Option<Props> {
    let id = canonical(block.0);
    if !owns(id) {
        return None;
    }
    if let Some(s) = pillar_state(id) {
        // Java: logs, wood and stripped blocks are 2.0, axe, fire 5/5.
        return Some(Props {
            hardness: 2.0,
            tool: Some(ToolKind::Axe),
            fire: (5, 5),
            lava: true,
            drop: Some(Item::from_block(Block(s.y))),
        });
    }
    signs::props(Block(id)).or_else(|| shelves::props(Block(id))).or_else(|| bookshelf::props(Block(id)))
}

/// The same state turned to face `facing`, for the module's states that have
/// a horizontal facing; everything else is unchanged.
pub fn with_facing(block: Block, facing: Facing) -> Block {
    shelves::with_facing(block, facing).or_else(|| bookshelf::with_facing(block, facing)).unwrap_or(block)
}

/// Fuel class of this module's items (Java 26.3 fuel values).
pub fn fuel(block: Block) -> Option<crate::content::fuel::FuelClass> {
    use crate::content::fuel::FuelClass;
    let id = block.0;
    if id == STRIPPED_BAMBOO {
        return Some(FuelClass::Wood);
    }
    if let Some(s) = signs::sign(block) {
        return s.kind.burns().then_some(if s.hanging() { FuelClass::HangingSign } else { FuelClass::Large });
    }
    if let Some(s) = shelves::shelf(block) {
        return s.kind.burns().then_some(FuelClass::Wood);
    }
    if bookshelf::slots(block).is_some() {
        return Some(FuelClass::Wood);
    }
    None
}

/// Default states for the creative palette.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (0..9u16)
        .flat_map(|w| {
            [pillar(w, Pillar::Wood, 0).0, pillar(w, Pillar::StrippedLog, 0).0, pillar(w, Pillar::StrippedWood, 0).0]
        })
        .chain([STRIPPED_BAMBOO])
        .chain(switches::palette_ids())
        .chain(signs::palette_ids())
        .chain(shelves::palette_ids())
        .chain(bookshelf::palette_ids())
}

/// Whether a state of this module draws as `Shaped::Custom` boxes (signs and shelves).
pub const fn custom_shape(id: u16) -> bool {
    signs::sign(Block(id)).is_some() || shelves::shelf(Block(id)).is_some()
}

/// The chunk-mesh boxes of a custom-shaped state: a shelf's boards; signs
/// draw nothing here (the client draws them, `render::signs`).
pub fn mesh_boxes(block: Block) -> Option<super::shape::Boxes> {
    let block = Block(canonical(block.0));
    if signs::sign(block).is_some() {
        return Some(super::shape::Boxes::new());
    }
    shelves::boxes(block)
}

/// The selection outline of a custom-shaped state.
pub fn outline(block: Block) -> Option<super::shape::Boxes> {
    let block = Block(canonical(block.0));
    signs::outline(block).or_else(|| shelves::boxes(block))
}

/// The collision boxes of a custom-shaped state.
pub fn collision(block: Block) -> Option<super::shape::Boxes> {
    let block = Block(canonical(block.0));
    signs::collision(block).or_else(|| shelves::boxes(block))
}

/// Java's `useShapeForLightOcclusion`: shelves.
pub const fn shape_blocks_light(id: u16) -> bool {
    shelves::shelf(Block(id)).is_some()
}

/// Whether `block` can rest on `below`: standing signs need a solid block
/// (Java's `isSolid`); the other signs lean on their wall or ceiling
/// (see `World::drop_unsupported_signs`).
pub fn can_stay_on(block: Block, below: Block) -> Option<bool> {
    let s = signs::sign(block)?;
    Some(!matches!(s.mount, signs::Mount::Standing { .. }) || below.is_solid())
}

/// Silk touch drops (the chiseled bookshelf drops nothing otherwise).
pub fn silk_drop(block: Block) -> Option<Item> {
    bookshelf::silk_drop(Block(canonical(block.0)))
}

/// Blocks redstone updates reach: shelves follow their neighbours' power.
pub fn redstone_participant(block: Block) -> bool {
    shelves::shelf(block).is_some()
}

/// Drawn as a flat sprite in inventories and when dropped: signs.
pub fn flat_icon(block: Block) -> bool {
    signs::sign(block).is_some()
}

/// The state the item `raw` places for this module's directional blocks:
/// signs (standing, wall or hanging, by Java's look-order rules), shelves
/// and chiseled bookshelves facing the player. `None` if `raw` is none of
/// them; `Some(None)` if no direction can hold it.
pub fn placed(
    raw: Block,
    ctx: &crate::session::placement::PlaceContext,
    get: impl Fn(IVec3) -> Option<Block>,
) -> Option<Option<Block>> {
    if let Some(r) = signs::placed(raw, ctx, &get) {
        return Some(r);
    }
    let facing = ctx.facing.opposite();
    if let Some(s) = shelves::shelf(raw) {
        return Some(Some(
            shelves::Shelf { facing, powered: false, chain: shelves::Chain::Unconnected, wet: false, ..s }.block(),
        ));
    }
    if bookshelf::slots(raw).is_some() {
        return Some(Some(bookshelf::state(facing, 0)));
    }
    None
}

/// Wood-sounding blocks of this module and the wooden switch twins
/// (`nether_biome_blocks::switch_oak` woods 2..=10).
pub fn wooden(block: Block) -> bool {
    let id = canonical(block.0);
    pillar_state(id).is_some()
        || custom_shape(id)
        || bookshelf::slots(Block(id)).is_some()
        || super::nether_biome_blocks::switch_oak(id).is_some_and(|(_, wood)| (2..=10).contains(&wood))
}

/// Java's horizontal `FACING` value order (north, south, west, east), which
/// shelves and chiseled bookshelves use for their ids so that the lowest
/// id, the one names resolve to, is Java's default state (facing north).
pub const FACINGS: [Facing; 4] = [Facing::North, Facing::South, Facing::West, Facing::East];

pub const fn facing_index(f: Facing) -> u16 {
    match f {
        Facing::North => 0,
        Facing::South => 1,
        Facing::West => 2,
        Facing::East => 3,
    }
}

/// Whether `block` keeps a block entity (signs, shelves, chiseled
/// bookshelves), which pistons can't move.
pub fn has_entity(block: Block) -> bool {
    let b = Block(canonical(block.0));
    signs::sign(b).is_some() || shelves::shelf(b).is_some() || bookshelf::slots(b).is_some()
}
