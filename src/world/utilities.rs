//! v0.12 utilities and special blocks (BLD-05b, BLD-02b, ENV-04): cauldrons,
//! flower pots and potted plants, lodestones, respawn anchors, campfires,
//! coral wall fans, ender and trapped chests and decorated pots.
//!
//! Every Java 26.3 state has its own id in the track's reservation
//! (28672–29695, `docs/architecture.md`), waterlogged states included; the
//! layouts are below. Behaviour lives in the submodules; this module is the
//! registry the shared tables in `block`, `shape` and `waterlog` consult.
//! Sources and differences: `docs/verification/2026-10-25-v0.12-utilities.md`.

pub mod anchor;
pub mod campfire;
pub mod cauldron;
pub mod chests;
pub mod dripleaf;
pub mod dripstone;
pub mod interact;
mod ticks;

#[cfg(test)]
mod tests;

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::block::{Block, Facing, RenderKind, tex as btex};
use super::shape::{Box16, Boxes};
use crate::inventory::Stack;
use crate::item::{Item, ToolKind};

/// Block-entity state of this module's blocks in one dimension, kept by
/// position like furnaces and chests, and saved under the level's
/// `utilities` property.
#[derive(Default)]
pub struct State {
    /// Campfire cooking slots.
    campfires: FxHashMap<IVec3, campfire::Slots>,
    /// The stack a decorated pot holds.
    decorated: FxHashMap<IVec3, Stack>,
    /// Cauldrons a dripstone drip will reach, with ticks left
    /// (`PointedDripstoneBlock.maybeTransferFluid`'s scheduled tick).
    drips: Vec<(IVec3, u16)>,
    /// Players looking into each trapped chest.
    viewers: FxHashMap<IVec3, u8>,
    /// Tilting big dripleaves, with ticks to their next tilt.
    tilts: Vec<(IVec3, u16)>,
    /// Cells where a falling stalactite landed this tick, with its damage.
    stalactite_hits: Vec<(IVec3, f32)>,
    /// Respawn anchors used outside the Nether this tick, for the host to
    /// blow up whichever player used them.
    anchor_blasts: Vec<IVec3>,
    /// Every campfire in loaded chunks, lit or not, for smoke.
    fires: rustc_hash::FxHashSet<IVec3>,
    /// Where a player's ender inventory is lent into the chest table for an
    /// open ender chest screen (whatever block is there now).
    lent: Option<IVec3>,
}

impl crate::world::World {
    /// Respawn anchors that went off since the last call
    /// (`RespawnAnchorBlock.explode`, power 5, fiery).
    pub fn take_anchor_blasts(&mut self) -> Vec<IVec3> {
        std::mem::take(&mut self.utilities.anchor_blasts)
    }
}

/// First state of the reservation.
pub const FIRST: u16 = 28672;
/// One past the last registered state; 28952..=29695 stay free.
pub const END: u16 = 28952;
const _: () = assert!(END <= 29696);

/// Empty cauldron; water cauldron levels 1–3 follow, then lava, then powder
/// snow levels 1–3 (`LayeredCauldronBlock.LEVEL`).
pub const CAULDRON: Block = Block(28672);
pub const LAVA_CAULDRON: Block = Block(28676);
pub const FLOWER_POT: Block = Block(28680);
/// The potted plants, in [`POTTED`] order.
const POTTED_FIRST: u16 = 28681;
/// 28714..=28720 stay free for later potted plants.
const POTTED_END: u16 = POTTED_FIRST + POTTED.len() as u16;
pub const LODESTONE: Block = Block(28721);
/// Respawn anchor with no charges; charges 1–4 follow.
const ANCHOR_FIRST: u16 = 28722;
/// Campfires: `28728 + soul * 32 + wet * 16 + signal * 8 + unlit * 4 + facing`
/// ([`FACINGS`] order), so each kind's default (lit, north, dry) comes first.
const CAMPFIRE_FIRST: u16 = 28728;
pub const CAMPFIRE: Block = Block(CAMPFIRE_FIRST);
pub const SOUL_CAMPFIRE: Block = Block(CAMPFIRE_FIRST + 32);
/// Coral wall fans: `28792 + dead * 40 + colour * 8 + dry * 4 + facing`;
/// Java registers them waterlogged by default.
const WALL_FAN_FIRST: u16 = 28792;
/// Ender chests: `28872 + wet * 4 + facing`.
const ENDER_FIRST: u16 = 28872;
pub const ENDER_CHEST: Block = Block(ENDER_FIRST);
/// Trapped chests: `28880 + wet * 12 + type * 4 + facing` (type single, left,
/// right). Chests here are single, as the plain chest is, so the left and
/// right ids stay unregistered until double chests exist.
const TRAPPED_FIRST: u16 = 28880;
pub const TRAPPED_CHEST: Block = Block(TRAPPED_FIRST);
/// Decorated pots: `28904 + wet * 8 + cracked * 4 + facing`.
const DECORATED_FIRST: u16 = 28904;
pub const DECORATED_POT: Block = Block(DECORATED_FIRST);
/// Big dripleaf `facing` and `tilt`: `28920 + wet * 16 + tilt * 4 + facing`.
/// North with no tilt is the existing big dripleaf (1950) and its wet
/// partner, so those two ids here stay unregistered.
const DRIPLEAF_FIRST: u16 = 28920;

/// Java's horizontal facings in this module's id order (default north first).
pub const FACINGS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

const fn fi(f: Facing) -> u16 {
    match f {
        Facing::North => 0,
        Facing::East => 1,
        Facing::South => 2,
        Facing::West => 3,
    }
}

/// Texture layers, offsets from one base so the integrator can repack them.
pub const TEX: u16 = 2576;
pub mod t {
    use super::TEX;
    pub const CAULDRON_SIDE: u16 = TEX;
    /// The rim around an empty cauldron's floor.
    pub const CAULDRON_TOP: u16 = TEX + 1;
    pub const CAULDRON_BOTTOM: u16 = TEX + 2;
    /// The rim around water, lava and powder snow surfaces.
    pub const CAULDRON_WATER: u16 = TEX + 3;
    pub const CAULDRON_LAVA: u16 = TEX + 4;
    pub const CAULDRON_SNOW: u16 = TEX + 5;
    pub const POT_SIDE: u16 = TEX + 6;
    /// A pot's rim around its soil, and around an empty pot's hollow.
    pub const POT_SOIL: u16 = TEX + 7;
    pub const POT_EMPTY: u16 = TEX + 8;
    pub const POT_BOTTOM: u16 = TEX + 9;
    /// One side layer per potted plant: the pot below, the plant above.
    pub const POTTED: u16 = TEX + 10;
    pub const POTTED_CACTUS_TOP: u16 = TEX + 43;
    pub const LODESTONE_SIDE: u16 = TEX + 44;
    pub const LODESTONE_TOP: u16 = TEX + 45;
    /// Five sides, one per charge count.
    pub const ANCHOR_SIDE: u16 = TEX + 46;
    pub const ANCHOR_TOP_OFF: u16 = TEX + 51;
    pub const ANCHOR_TOP_ON: u16 = TEX + 52;
    pub const ANCHOR_BOTTOM: u16 = TEX + 53;
    /// Campfire faces: log ends, log lengths and top, lit then unlit, then
    /// the lit soul campfire's three (unlit, both kinds look alike).
    pub const CAMPFIRE: u16 = TEX + 54;
    /// Coral wall fans: `(dead * 5 + colour) * 4 + facing`.
    pub const WALL_FAN: u16 = TEX + 63;
    pub const ENDER_FRONT: u16 = TEX + 103;
    pub const ENDER_SIDE: u16 = TEX + 104;
    pub const ENDER_TOP: u16 = TEX + 105;
    pub const TRAPPED_FRONT: u16 = TEX + 106;
    pub const DECORATED_SIDE: u16 = TEX + 107;
    pub const DECORATED_TOP: u16 = TEX + 108;
    pub const DECORATED_CRACKED: u16 = TEX + 109;
    pub const COUNT: u16 = 110;
}
/// One past this module's last layer; the track owns 2576..=2767.
pub const TEX_END: u16 = TEX + t::COUNT;
const _: () = assert!(TEX_END <= 2768);

/// The plants a flower pot holds (Java's `FlowerPotBlock` table, limited
/// to plants VoxelCraft has), with each potted block's name.
pub const POTTED: [(Block, &str); 33] = {
    use super::nether_biome_blocks as n;
    use super::overworld_blocks as o;
    [
        (Block::OAK_SAPLING, "potted oak sapling"),
        (Block::SPRUCE_SAPLING, "potted spruce sapling"),
        (Block::BIRCH_SAPLING, "potted birch sapling"),
        (Block::JUNGLE_SAPLING, "potted jungle sapling"),
        (Block::ACACIA_SAPLING, "potted acacia sapling"),
        (o::CHERRY_SAPLING, "potted cherry sapling"),
        (o::DARK_OAK_SAPLING, "potted dark oak sapling"),
        (o::PALE_OAK_SAPLING, "potted pale oak sapling"),
        (o::MANGROVE_PROPAGULE, "potted mangrove propagule"),
        (Block::FERN, "potted fern"),
        (Block::DANDELION, "potted dandelion"),
        (Block::POPPY, "potted poppy"),
        (Block::BLUE_ORCHID, "potted blue orchid"),
        (o::ALLIUM, "potted allium"),
        (o::AZURE_BLUET, "potted azure bluet"),
        (o::RED_TULIP, "potted red tulip"),
        (o::ORANGE_TULIP, "potted orange tulip"),
        (o::WHITE_TULIP, "potted white tulip"),
        (o::PINK_TULIP, "potted pink tulip"),
        (o::OXEYE_DAISY, "potted oxeye daisy"),
        (o::CORNFLOWER, "potted cornflower"),
        (o::LILY_OF_THE_VALLEY, "potted lily of the valley"),
        (Block::RED_MUSHROOM, "potted red mushroom"),
        (Block::BROWN_MUSHROOM, "potted brown mushroom"),
        (Block::DEAD_BUSH, "potted dead bush"),
        (Block::CACTUS, "potted cactus"),
        (o::BAMBOO, "potted bamboo"),
        (o::AZALEA, "potted azalea"),
        (o::FLOWERING_AZALEA, "potted flowering azalea"),
        (n::CRIMSON_FUNGUS, "potted crimson fungus"),
        (n::WARPED_FUNGUS, "potted warped fungus"),
        (n::CRIMSON_ROOTS, "potted crimson roots"),
        (n::WARPED_ROOTS, "potted warped roots"),
    ]
};
/// Index of the cactus and the bamboo in [`POTTED`] (drawn as posts).
pub const POTTED_CACTUS: usize = 25;
pub const POTTED_BAMBOO: usize = 26;

/// Java's coral colours for wall fan names, as in `overworld_blocks`.
const WALL_FAN_NAMES: [&str; 10] = [
    "tube coral wall fan",
    "brain coral wall fan",
    "bubble coral wall fan",
    "fire coral wall fan",
    "horn coral wall fan",
    "dead tube coral wall fan",
    "dead brain coral wall fan",
    "dead bubble coral wall fan",
    "dead fire coral wall fan",
    "dead horn coral wall fan",
];

/// A water cauldron at `level` 1..=3.
pub const fn water_cauldron(level: u8) -> Block {
    Block(28672 + clamp_level(level))
}

/// A powder snow cauldron at `level` 1..=3.
pub const fn powder_snow_cauldron(level: u8) -> Block {
    Block(28676 + clamp_level(level))
}

const fn clamp_level(level: u8) -> u16 {
    if level < 1 {
        1
    } else if level > 3 {
        3
    } else {
        level as u16
    }
}

/// What a cauldron holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Contents {
    Empty,
    Water(u8),
    Lava,
    PowderSnow(u8),
}

/// The contents of a cauldron state.
pub const fn cauldron_of(b: Block) -> Option<Contents> {
    Some(match b.0 {
        28672 => Contents::Empty,
        28673..=28675 => Contents::Water((b.0 - 28672) as u8),
        28676 => Contents::Lava,
        28677..=28679 => Contents::PowderSnow((b.0 - 28676) as u8),
        _ => return None,
    })
}

/// The cauldron state holding `contents`.
pub const fn cauldron(contents: Contents) -> Block {
    match contents {
        Contents::Empty => CAULDRON,
        Contents::Water(0) | Contents::PowderSnow(0) => CAULDRON,
        Contents::Water(level) => water_cauldron(level),
        Contents::Lava => LAVA_CAULDRON,
        Contents::PowderSnow(level) => powder_snow_cauldron(level),
    }
}

/// The potted form of `plant`, if a pot can hold it.
pub fn potted(plant: Block) -> Option<Block> {
    POTTED.iter().position(|(p, _)| *p == plant).map(|i| Block(POTTED_FIRST + i as u16))
}

/// The plant in a potted block.
pub const fn potted_plant(b: Block) -> Option<Block> {
    if b.0 >= POTTED_FIRST && b.0 < POTTED_END { Some(POTTED[(b.0 - POTTED_FIRST) as usize].0) } else { None }
}

/// A flower pot, empty or potted.
pub const fn is_pot(b: Block) -> bool {
    b.0 >= FLOWER_POT.0 && b.0 < POTTED_END
}

/// A respawn anchor with `charges` 0..=4.
pub const fn respawn_anchor(charges: u8) -> Block {
    Block(ANCHOR_FIRST + if charges > 4 { 4 } else { charges as u16 })
}

pub const RESPAWN_ANCHOR: Block = Block(ANCHOR_FIRST);

/// The charges of a respawn anchor state.
pub const fn anchor_charges(b: Block) -> Option<u8> {
    if b.0 >= ANCHOR_FIRST && b.0 <= ANCHOR_FIRST + 4 { Some((b.0 - ANCHOR_FIRST) as u8) } else { None }
}

/// Every property of a campfire state (`CampfireBlock`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Campfire {
    pub soul: bool,
    pub lit: bool,
    pub signal: bool,
    pub wet: bool,
    pub facing: Facing,
}

pub const fn campfire(c: Campfire) -> Block {
    Block(
        CAMPFIRE_FIRST
            + c.soul as u16 * 32
            + c.wet as u16 * 16
            + c.signal as u16 * 8
            + (!c.lit) as u16 * 4
            + fi(c.facing),
    )
}

pub const fn campfire_of(b: Block) -> Option<Campfire> {
    if b.0 < CAMPFIRE_FIRST || b.0 >= CAMPFIRE_FIRST + 64 {
        return None;
    }
    let i = b.0 - CAMPFIRE_FIRST;
    Some(Campfire {
        soul: i >= 32,
        wet: i % 32 >= 16,
        signal: i % 16 >= 8,
        lit: i % 8 < 4,
        facing: FACINGS[(i % 4) as usize],
    })
}

/// A coral wall fan: `colour` in `overworld_blocks::CORAL_NAMES` order,
/// `facing` away from its wall.
pub const fn wall_fan(colour: u16, dead: bool, facing: Facing, wet: bool) -> Block {
    Block(WALL_FAN_FIRST + dead as u16 * 40 + (colour % 5) * 8 + (!wet) as u16 * 4 + fi(facing))
}

/// `(colour, dead, facing, wet)` of a coral wall fan.
pub const fn wall_fan_of(b: Block) -> Option<(u16, bool, Facing, bool)> {
    if b.0 < WALL_FAN_FIRST || b.0 >= WALL_FAN_FIRST + 80 {
        return None;
    }
    let i = b.0 - WALL_FAN_FIRST;
    Some(((i % 40) / 8, i >= 40, FACINGS[(i % 4) as usize], i % 8 < 4))
}

pub const fn ender_chest(facing: Facing, wet: bool) -> Block {
    Block(ENDER_FIRST + wet as u16 * 4 + fi(facing))
}

/// `EnderChestBlock.useWithoutItem`: a conductor on top keeps it shut.
pub fn ender_chest_opens(world: &super::World, pos: IVec3) -> bool {
    !world.get_block(pos + IVec3::Y).is_some_and(super::redstone_blocks::conductor)
}

/// `(facing, wet)` of an ender chest.
pub const fn ender_chest_of(b: Block) -> Option<(Facing, bool)> {
    if b.0 < ENDER_FIRST || b.0 >= ENDER_FIRST + 8 {
        return None;
    }
    let i = b.0 - ENDER_FIRST;
    Some((FACINGS[(i % 4) as usize], i >= 4))
}

/// A single trapped chest.
pub const fn trapped_chest(facing: Facing, wet: bool) -> Block {
    Block(TRAPPED_FIRST + wet as u16 * 12 + fi(facing))
}

/// `(facing, wet)` of a (single) trapped chest.
pub const fn trapped_chest_of(b: Block) -> Option<(Facing, bool)> {
    if b.0 < TRAPPED_FIRST || b.0 >= TRAPPED_FIRST + 24 {
        return None;
    }
    let i = b.0 - TRAPPED_FIRST;
    if i % 12 >= 4 {
        return None;
    }
    Some((FACINGS[(i % 4) as usize], i >= 12))
}

pub const fn decorated_pot(facing: Facing, cracked: bool, wet: bool) -> Block {
    Block(DECORATED_FIRST + wet as u16 * 8 + cracked as u16 * 4 + fi(facing))
}

/// `(facing, cracked, wet)` of a decorated pot.
pub const fn decorated_pot_of(b: Block) -> Option<(Facing, bool, bool)> {
    if b.0 < DECORATED_FIRST || b.0 >= DECORATED_FIRST + 16 {
        return None;
    }
    let i = b.0 - DECORATED_FIRST;
    Some((FACINGS[(i % 4) as usize], i % 8 >= 4, i >= 8))
}

/// Java's `Tilt` of a big dripleaf.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tilt {
    None,
    Unstable,
    Partial,
    Full,
}

impl Tilt {
    pub const ALL: [Tilt; 4] = [Tilt::None, Tilt::Unstable, Tilt::Partial, Tilt::Full];
}

/// The existing big dripleaf's wet partner.
const DRIPLEAF_WET: u16 = match super::waterlog::family_pair(1950) {
    Some((wet, false)) => wet,
    _ => panic!("big dripleaf has a wet partner"),
};

/// A big dripleaf head facing `facing` (toward its placer) with `tilt`.
pub const fn big_dripleaf(facing: Facing, tilt: Tilt, wet: bool) -> Block {
    if fi(facing) == 0 && matches!(tilt, Tilt::None) {
        return Block(if wet { DRIPLEAF_WET } else { 1950 });
    }
    Block(DRIPLEAF_FIRST + wet as u16 * 16 + tilt as u16 * 4 + fi(facing))
}

/// `(facing, tilt, wet)` of any big dripleaf head state.
pub const fn dripleaf_of(b: Block) -> Option<(Facing, Tilt, bool)> {
    if b.0 == 1950 {
        return Some((Facing::North, Tilt::None, false));
    }
    if b.0 == DRIPLEAF_WET {
        return Some((Facing::North, Tilt::None, true));
    }
    if b.0 < DRIPLEAF_FIRST || b.0 >= DRIPLEAF_FIRST + 32 {
        return None;
    }
    let i = b.0 - DRIPLEAF_FIRST;
    if i.is_multiple_of(16) {
        return None;
    }
    Some((FACINGS[(i % 4) as usize], Tilt::ALL[((i % 16) / 4) as usize], i >= 16))
}

/// Whether `id` is one of this module's states (registered or not).
#[inline]
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id < END
}

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

const fn column(side: u16, top: u16, bottom: u16) -> [u16; 6] {
    [side, side, top, bottom, side, side]
}

/// Side textures with `front` on the face `facing` points out of.
const fn fronted(front: u16, side: u16, top: u16, facing: Facing) -> [u16; 6] {
    let mut t = [side, side, top, top, side, side];
    t[facing.face()] = front;
    t
}

/// Name, render kind and face textures of every registered state.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    if !owns(id) {
        return None;
    }
    const fn cauldron(top: u16) -> [u16; 6] {
        column(t::CAULDRON_SIDE, top, t::CAULDRON_BOTTOM)
    }
    Some(match id {
        28672 => ("cauldron", Shaped, cauldron(t::CAULDRON_TOP)),
        28673..=28675 => ("water cauldron", Shaped, cauldron(t::CAULDRON_WATER)),
        28676 => ("lava cauldron", Shaped, cauldron(t::CAULDRON_LAVA)),
        28677..=28679 => ("powder snow cauldron", Shaped, cauldron(t::CAULDRON_SNOW)),
        28680 => ("flower pot", Shaped, column(t::POT_SIDE, t::POT_EMPTY, t::POT_BOTTOM)),
        POTTED_FIRST..POTTED_END => {
            let i = (id - POTTED_FIRST) as usize;
            let top = if i == POTTED_CACTUS { t::POTTED_CACTUS_TOP } else { t::POT_SOIL };
            (POTTED[i].1, Shaped, column(t::POTTED + i as u16, top, t::POT_BOTTOM))
        }
        28721 => ("lodestone", Opaque, column(t::LODESTONE_SIDE, t::LODESTONE_TOP, t::LODESTONE_TOP)),
        ANCHOR_FIRST..=28726 => {
            let charges = id - ANCHOR_FIRST;
            let top = if charges > 0 { t::ANCHOR_TOP_ON } else { t::ANCHOR_TOP_OFF };
            ("respawn anchor", Opaque, column(t::ANCHOR_SIDE + charges, top, t::ANCHOR_BOTTOM))
        }
        CAMPFIRE_FIRST..WALL_FAN_FIRST => {
            let Some(c) = campfire_of(Block(id)) else { return None };
            let set = if !c.lit {
                3
            } else if c.soul {
                6
            } else {
                0
            };
            let (end, long, top) = (t::CAMPFIRE + set, t::CAMPFIRE + set + 1, t::CAMPFIRE + set + 2);
            // The bottom logs run north–south for a campfire facing north or
            // south: their ends show on the north and south faces.
            let tex =
                if c.facing.along_x() { [end, end, top, top, long, long] } else { [long, long, top, top, end, end] };
            (if c.soul { "soul campfire" } else { "campfire" }, Shaped, tex)
        }
        WALL_FAN_FIRST..ENDER_FIRST => {
            let Some((colour, dead, facing, _)) = wall_fan_of(Block(id)) else { return None };
            let i = dead as u16 * 5 + colour;
            (WALL_FAN_NAMES[i as usize], Shaped, all(t::WALL_FAN + i * 4 + fi(facing)))
        }
        ENDER_FIRST..TRAPPED_FIRST => {
            let Some((facing, _)) = ender_chest_of(Block(id)) else { return None };
            ("ender chest", Opaque, fronted(t::ENDER_FRONT, t::ENDER_SIDE, t::ENDER_TOP, facing))
        }
        TRAPPED_FIRST..DECORATED_FIRST => {
            let Some((facing, _)) = trapped_chest_of(Block(id)) else { return None };
            ("trapped chest", Opaque, fronted(t::TRAPPED_FRONT, btex::CHEST_SIDE, btex::CHEST_TOP, facing))
        }
        DECORATED_FIRST..DRIPLEAF_FIRST => {
            let Some((_, cracked, _)) = decorated_pot_of(Block(id)) else { return None };
            let side = if cracked { t::DECORATED_CRACKED } else { t::DECORATED_SIDE };
            ("decorated pot", Shaped, column(side, t::DECORATED_TOP, t::DECORATED_TOP))
        }
        DRIPLEAF_FIRST..END => {
            if dripleaf_of(Block(id)).is_none() {
                return None;
            }
            match super::overworld_blocks::registry(1950) {
                Some(info) => info,
                None => return None,
            }
        }
        _ => return None,
    })
}

/// The other state of a waterlogged pair, and whether `id` is the wet one.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if !owns(id) || registry(id).is_none() {
        return None;
    }
    let (first, bit, wet_when_set) = match id {
        CAMPFIRE_FIRST..WALL_FAN_FIRST => (CAMPFIRE_FIRST, 16, true),
        WALL_FAN_FIRST..ENDER_FIRST => (WALL_FAN_FIRST, 4, false),
        ENDER_FIRST..TRAPPED_FIRST => (ENDER_FIRST, 4, true),
        TRAPPED_FIRST..DECORATED_FIRST => (TRAPPED_FIRST, 12, true),
        DECORATED_FIRST..DRIPLEAF_FIRST => (DECORATED_FIRST, 8, true),
        DRIPLEAF_FIRST..END => (DRIPLEAF_FIRST, 16, true),
        _ => return None,
    };
    let i = id - first;
    let set = (i / bit) % 2 == 1;
    let other = if set { id - bit } else { id + bit };
    Some((other, set == wet_when_set))
}

/// Light emission (Java: lit campfire 15, soul campfire 10, lava cauldron
/// 15, ender chest 7, respawn anchor 0/3/7/11/15 by charges).
pub const fn emission(id: u16) -> u8 {
    if let Some(c) = campfire_of(Block(id)) {
        return if !c.lit {
            0
        } else if c.soul {
            10
        } else {
            15
        };
    }
    if let Some(charges) = anchor_charges(Block(id)) {
        return [0, 3, 7, 11, 15][charges as usize];
    }
    if ender_chest_of(Block(id)).is_some() {
        return 7;
    }
    if id == LAVA_CAULDRON.0 { 15 } else { 0 }
}

/// Shaped states drawn from [`boxes`] (`Shaped::Custom`).
#[inline]
pub fn is_custom(b: Block) -> bool {
    owns(b.0) && b.kind() == RenderKind::Shaped
}

/// Shaped states with see-through texels (plants, flames, fans): drawn in
/// the alpha-tested pass.
pub fn see_through(b: Block) -> bool {
    is_pot(b) || campfire_of(b).is_some() || wall_fan_of(b).is_some()
}

/// The block items, picks and rules use for a state: campfires their lit
/// north-facing default, wall fans the standing fan that places them,
/// chests and pots their default, tilted dripleaves the plain one.
pub fn base(b: Block) -> Option<Block> {
    if !owns(b.0) {
        return None;
    }
    if let Some(c) = campfire_of(b) {
        return Some(if c.soul { SOUL_CAMPFIRE } else { CAMPFIRE });
    }
    if let Some((colour, dead, _, _)) = wall_fan_of(b) {
        return Some(super::overworld_blocks::coral(2, colour, dead));
    }
    if ender_chest_of(b).is_some() {
        return Some(ENDER_CHEST);
    }
    if trapped_chest_of(b).is_some() {
        return Some(TRAPPED_CHEST);
    }
    if decorated_pot_of(b).is_some() {
        return Some(DECORATED_POT);
    }
    if owns(b.0) && dripleaf_of(b).is_some() {
        return Some(super::overworld_blocks::BIG_DRIPLEAF);
    }
    None
}

/// What pick-block gives: a filled cauldron the cauldron, a potted plant
/// its plant (`FlowerPotBlock.getCloneItemStack`), a charged anchor the anchor.
pub fn picked(b: Block) -> Option<Item> {
    if cauldron_of(b).is_some() {
        return Some(CAULDRON.into());
    }
    if let Some(plant) = potted_plant(b) {
        return Some(plant.into());
    }
    if anchor_charges(b).is_some() {
        return Some(RESPAWN_ANCHOR.into());
    }
    None
}

/// `(hardness, best tool, pickaxe harvest level)` per Java's block
/// properties and `mineable/*` and `needs_*_tool` tags.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    use ToolKind::*;
    if !owns(b.0) || registry(b.0).is_none() || dripleaf_of(b).is_some() {
        return None;
    }
    Some(if cauldron_of(b).is_some() {
        (2.0, Some(Pickaxe), Some(0))
    } else if let Some((_, true, ..)) = wall_fan_of(b) {
        // Dead coral needs a pickaxe to drop anything, even with silk touch.
        (0.0, Some(Pickaxe), Some(0))
    } else if is_pot(b) || wall_fan_of(b).is_some() || decorated_pot_of(b).is_some() {
        (0.0, None, None)
    } else if b == LODESTONE {
        (3.5, Some(Pickaxe), Some(0))
    } else if anchor_charges(b).is_some() {
        (50.0, Some(Pickaxe), Some(3))
    } else if campfire_of(b).is_some() {
        (2.0, Some(Axe), None)
    } else if ender_chest_of(b).is_some() {
        // 26.3 dropped `requiresCorrectToolForDrops`: any tool yields obsidian.
        (22.5, Some(Pickaxe), None)
    } else {
        // Trapped chests.
        (2.5, Some(Axe), None)
    })
}

/// What breaking the block drops (`Some(None)`: nothing here; extra and
/// counted drops come from [`extra_drops`]); `None` for other blocks.
pub fn drop(b: Block) -> Option<Option<Item>> {
    if !owns(b.0) || dripleaf_of(b).is_some() {
        return None;
    }
    Some(if cauldron_of(b).is_some() {
        Some(CAULDRON.into())
    } else if is_pot(b) {
        Some(FLOWER_POT.into())
    } else if anchor_charges(b).is_some() {
        Some(RESPAWN_ANCHOR.into())
    } else if let Some(c) = campfire_of(b) {
        Some(if c.soul { super::nether_biome_blocks::SOUL_SOIL.into() } else { Item::CHARCOAL })
    } else if wall_fan_of(b).is_some() {
        // Like standing fans: only silk touch keeps them.
        None
    } else if ender_chest_of(b).is_some() {
        Some(Block::OBSIDIAN.into())
    } else if let Some((_, cracked, _)) = decorated_pot_of(b) {
        // A cracked pot breaks into its four sides (plain pots: bricks).
        (!cracked).then_some(DECORATED_POT.into())
    } else {
        Some(b.base().into())
    })
}

/// Extra loot beyond [`drop`]'s single item (Java's loot tables): the
/// second charcoal of a campfire, an ender chest's other seven obsidian,
/// a potted plant's plant and a cracked pot's four bricks.
pub fn extra_drops(b: Block) -> Option<crate::inventory::Stack> {
    use crate::inventory::Stack;
    if let Some(c) = campfire_of(b) {
        return (!c.soul).then(|| Stack::new(Item::CHARCOAL, 1));
    }
    if ender_chest_of(b).is_some() {
        return Some(Stack::new(Block::OBSIDIAN, 7));
    }
    if let Some(plant) = potted_plant(b) {
        return Some(Stack::new(plant, 1));
    }
    if let Some((_, true, _)) = decorated_pot_of(b) {
        return Some(Stack::new(Item::BRICK, 4));
    }
    None
}

/// `DecoratedPotBlock.playerWillDestroy`: a pot broken with a sword, axe,
/// pickaxe, shovel or hoe (`#breaks_decorated_pots`) without silk touch
/// cracks and breaks into its sides.
pub fn shattered(b: Block, held: Option<Stack>) -> Block {
    match (decorated_pot_of(b), held) {
        (Some((facing, false, wet)), Some(s))
            if s.item.as_tool().is_some() && !s.active_enchants().has(crate::enchant::Enchantment::SilkTouch) =>
        {
            decorated_pot(facing, true, wet)
        }
        _ => b,
    }
}

/// What silk touch keeps (`tool/can_silk_touch` loot alternatives).
pub fn silk_drop(b: Block) -> Option<Item> {
    if campfire_of(b).is_some() || ender_chest_of(b).is_some() {
        return Some(b.base().into());
    }
    wall_fan_of(b).map(|_| b.base().into())
}

/// Creative palette entries (the default state of each item).
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [
        CAULDRON.0,
        FLOWER_POT.0,
        LODESTONE.0,
        RESPAWN_ANCHOR.0,
        CAMPFIRE.0,
        SOUL_CAMPFIRE.0,
        ENDER_CHEST.0,
        TRAPPED_CHEST.0,
        DECORATED_POT.0,
    ]
    .into_iter()
}

/// Whether this module's `block` can rest on `below` (`None`: no rule here).
/// Wall fans check their wall instead ([`wall_of`]).
pub fn can_stay_on(block: Block, below: Block) -> Option<bool> {
    if !owns(block.0) {
        return None;
    }
    if dripleaf_of(block).is_some() {
        return super::overworld_blocks::can_stay_on(super::overworld_blocks::BIG_DRIPLEAF, below);
    }
    Some(true)
}

/// The wall a coral wall fan hangs on (the cell behind it).
pub fn wall_of(block: Block) -> Option<Facing> {
    wall_fan_of(block).map(|(_, _, facing, _)| facing.opposite())
}

/// `BaseCoralWallFanBlock.canSurvive`: the wall's face toward the fan is sturdy.
pub fn wall_ok(block: Block, wall: Block) -> bool {
    match wall_of(block) {
        Some(w) => super::mount_states::face_sturdy(wall, w.opposite().offset()),
        None => true,
    }
}

/// The state an item of this module places at `at` (Java's
/// `getStateForPlacement`): campfires and decorated pots face where the
/// player looks, chests and big dripleaves back at them; a campfire over a
/// hay bale signals and one placed in water is out. A coral fan
/// (`StandingAndWallBlockItem`) takes the first of the player's nearest
/// looking directions it can survive in: down for the floor, sideways for a
/// wall, never up. `look` is the player's horizontal facing and `eye` their
/// look vector.
pub fn placed(
    block: Block,
    at: IVec3,
    look: Facing,
    eye: glam::Vec3,
    wet: bool,
    get: impl Fn(IVec3) -> Option<Block>,
) -> Option<Block> {
    if let Some((2, colour, dead)) = super::overworld_blocks::coral_of(block.canonical()) {
        let mut dirs = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];
        dirs.sort_by(|a, b| eye.dot(b.as_vec3()).total_cmp(&eye.dot(a.as_vec3())));
        for d in dirs {
            if d == IVec3::NEG_Y {
                let floor = block.canonical().with_water(wet);
                if get(at - IVec3::Y).is_some_and(|below| floor.can_stay_on(below)) {
                    return Some(floor);
                }
            } else if d.y == 0 {
                let fan = wall_fan(colour, dead, Facing::from_offset(-d)?, wet);
                if get(at + d).is_some_and(|wall| wall_ok(fan, wall)) {
                    return Some(fan);
                }
            }
        }
        return None;
    }
    let toward = look.opposite();
    if let Some(c) = campfire_of(block) {
        let signal = get(at - IVec3::Y).is_some_and(campfire::smoke_source);
        return Some(campfire(Campfire { facing: look, wet, lit: !wet, signal, ..c }));
    }
    if ender_chest_of(block).is_some() {
        return Some(ender_chest(toward, wet));
    }
    if trapped_chest_of(block).is_some() {
        return Some(trapped_chest(toward, wet));
    }
    if decorated_pot_of(block).is_some() {
        return Some(decorated_pot(look, false, wet));
    }
    if block.canonical() == super::overworld_blocks::BIG_DRIPLEAF {
        return Some(big_dripleaf(toward, Tilt::None, wet));
    }
    None
}

const fn bx(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}

/// The cauldron's walls, floor and four legs (Java's `cauldron` model; each
/// L-shaped leg is drawn as its 4x4 square, which differs only under the floor).
const CAULDRON_BOXES: [Box16; 9] = [
    bx([0, 3, 0], [2, 16, 16]),
    bx([14, 3, 0], [16, 16, 16]),
    bx([2, 3, 0], [14, 16, 2]),
    bx([2, 3, 14], [14, 16, 16]),
    bx([2, 3, 2], [14, 4, 14]),
    bx([0, 0, 0], [4, 3, 4]),
    bx([12, 0, 0], [16, 3, 4]),
    bx([0, 0, 12], [4, 3, 16]),
    bx([12, 0, 12], [16, 3, 16]),
];

/// The flower pot's four sides and soil (Java's `flower_pot` model).
const POT_BOXES: [Box16; 5] = [
    bx([5, 0, 5], [6, 6, 11]),
    bx([10, 0, 5], [11, 6, 11]),
    bx([6, 0, 5], [10, 6, 6]),
    bx([6, 0, 10], [10, 6, 11]),
    bx([6, 0, 6], [10, 4, 10]),
];

/// The campfire's four logs and its ember bed, logs running north–south
/// below (Java's `template_campfire`), then its two flame planes.
const CAMPFIRE_BOXES: [Box16; 7] = [
    bx([1, 0, 0], [5, 4, 16]),
    bx([0, 3, 11], [16, 7, 15]),
    bx([11, 0, 0], [15, 4, 16]),
    bx([0, 3, 1], [16, 7, 5]),
    bx([5, 0, 0], [11, 1, 16]),
    bx([1, 1, 8], [15, 16, 8]),
    bx([8, 1, 1], [8, 16, 15]),
];

/// Swaps x and z: the campfire model turned a quarter (it is symmetric
/// under a half turn).
const fn swap_xz(b: Box16) -> Box16 {
    bx([b.min[2], b.min[1], b.min[0]], [b.max[2], b.max[1], b.max[0]])
}

/// Visible model boxes of this module's custom-shaped states.
pub fn boxes(b: Block) -> Boxes {
    let mut out = Boxes::new();
    if let Some(contents) = cauldron_of(b) {
        CAULDRON_BOXES.iter().for_each(|&x| out.push(x));
        // The surface: Java's `LayeredCauldronBlock.getContentHeight`
        // (6 + 3 * level) / 16, and 15/16 for lava.
        let top = match contents {
            Contents::Empty => None,
            Contents::Water(level) | Contents::PowderSnow(level) => Some(6 + 3 * level),
            Contents::Lava => Some(15),
        };
        if let Some(top) = top {
            out.push(bx([2, 4, 2], [14, top, 14]));
        }
    } else if is_pot(b) {
        POT_BOXES.iter().for_each(|&x| out.push(x));
        match potted_plant(b).map(|_| (b.0 - POTTED_FIRST) as usize) {
            Some(POTTED_CACTUS) => out.push(bx([6, 4, 6], [10, 16, 10])),
            Some(POTTED_BAMBOO) => out.push(bx([7, 4, 7], [9, 16, 9])),
            Some(_) => {
                // The cross model, as two planes along the axes.
                out.push(bx([3, 4, 8], [13, 16, 8]));
                out.push(bx([8, 4, 3], [8, 16, 13]));
            }
            None => {}
        }
    } else if let Some(c) = campfire_of(b) {
        let n = if c.lit { 7 } else { 5 };
        for &x in &CAMPFIRE_BOXES[..n] {
            out.push(if c.facing.along_x() { swap_xz(x) } else { x });
        }
    } else if let Some((_, _, facing, _)) = wall_fan_of(b) {
        // The fan as a flat leaf out from its wall, at the middle of Java's
        // `BaseCoralWallFanBlock` outline (11 deep, y 4..12).
        out.push(match facing {
            Facing::North => bx([0, 8, 5], [16, 8, 16]),
            Facing::South => bx([0, 8, 0], [16, 8, 11]),
            Facing::East => bx([0, 8, 0], [11, 8, 16]),
            Facing::West => bx([5, 8, 0], [16, 8, 16]),
        });
    } else if let Some((_, tilt, _)) = dripleaf_of(b) {
        // Java tilts the leaf down 22.5 and 45 degrees; drawn here as the
        // leaf lowered by its tilt.
        out.push(match tilt {
            Tilt::None | Tilt::Unstable => bx([0, 11, 0], [16, 15, 16]),
            Tilt::Partial => bx([0, 10, 0], [16, 13, 16]),
            Tilt::Full => bx([0, 8, 0], [16, 11, 16]),
        });
    } else if decorated_pot_of(b).is_some() {
        // Body, neck and lip of Java's decorated pot.
        out.push(bx([1, 0, 1], [15, 13, 15]));
        out.push(bx([5, 13, 5], [11, 14, 11]));
        out.push(bx([4, 14, 4], [12, 16, 12]));
    }
    out
}

/// Selection outlines (`getShape`) of the custom-shaped states.
pub fn outline(b: Block) -> Boxes {
    if is_pot(b) {
        // `FlowerPotBlock.SHAPE`: `Block.column(6, 0, 6)`.
        return Boxes::from_box(bx([5, 0, 5], [11, 6, 11]));
    }
    if let Some((_, _, facing, _)) = wall_fan_of(b) {
        return Boxes::from_box(match facing {
            Facing::North => bx([0, 4, 5], [16, 12, 16]),
            Facing::South => bx([0, 4, 0], [16, 12, 11]),
            Facing::East => bx([0, 4, 0], [11, 12, 16]),
            Facing::West => bx([5, 4, 0], [16, 12, 16]),
        });
    }
    if campfire_of(b).is_some() {
        // `CampfireBlock.SHAPE`: `Block.column(16, 0, 7)`.
        return Boxes::from_box(bx([0, 0, 0], [16, 7, 16]));
    }
    if decorated_pot_of(b).is_some() {
        return Boxes::from_box(bx([1, 0, 1], [15, 16, 15]));
    }
    if cauldron_of(b).is_some() {
        let mut out = Boxes::new();
        CAULDRON_BOXES.iter().for_each(|&x| out.push(x));
        return out;
    }
    boxes(b)
}

/// Collision boxes: cauldrons are hollow, fans have none, the rest match
/// their outline.
pub fn collision(b: Block) -> Boxes {
    if wall_fan_of(b).is_some() {
        return Boxes::new();
    }
    // `BigDripleafBlock.LEAF_SHAPES`: a partial tilt lowers the leaf's top
    // to 13; a full tilt lets everything through.
    if let Some((_, tilt, _)) = dripleaf_of(b) {
        return match tilt {
            Tilt::None | Tilt::Unstable => Boxes::from_box(bx([0, 11, 0], [16, 15, 16])),
            Tilt::Partial => Boxes::from_box(bx([0, 11, 0], [16, 13, 16])),
            Tilt::Full => Boxes::new(),
        };
    }
    outline(b)
}
