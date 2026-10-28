//! v0.12 copper (BLD-01c): the unwaxed oxidation stages of Java 26.3's
//! copper families, ids 22528–24575 (docs/architecture.md, "v0.12 track
//! reservations"). Java classes: `WeatheringCopper`, `ChangeOverTimeBlock`,
//! the `WeatheringCopper*Block`s, `CopperBulbBlock`, `LightningRodBlock`,
//! `ChainBlock`, `IronBarsBlock`, `LanternBlock`, `TorchBlock` and
//! `WallTorchBlock`; see docs/verification/2026-10-25-v0.12-copper.md.
//!
//! Each family takes `stride` ids per stage (unaffected, exposed, weathered,
//! oxidized) from its origin, in [`Family::ALL`] order, so a stage change
//! keeps the local state: `id = origin + stage * stride + local`.
//!
//! | Family | Ids per stage | Local state |
//! | --- | --- | --- |
//! | block, cut, chiseled | 1 | — (the unaffected block of copper stays id 231) |
//! | grate | 2 | waterlogged |
//! | bulb | 4 | `lit + 2 * powered` |
//! | stairs | 80 | `slab_stair_states` local: `((top * 5 + shape) * 2 + wet) * 4 + facing` |
//! | slab | 5 | bottom dry, bottom wet, top dry, top wet, double |
//! | door | 24 | lower left `open * 4 + facing`, lower right (+8), upper (+16, no hinge) |
//! | trapdoor | 32 | `wet * 16 + top * 8 + open * 4 + facing` (the oak trapdoor's local) |
//! | bars | 2 | waterlogged |
//! | chain | 6 | `wet * 3 + axis` (Y, X, Z) |
//! | lantern | 4 | `hanging * 2 + wet` |
//! | chest | 24 | `type * 8 + wet * 4 + facing` (type single, left, right; `double_chest`) |
//! | lightning rod | 24 | `powered * 12 + wet * 6 + direction` (up, down, then [`Facing::ALL`]); only unpowered is registered |
//! | torch | 5, one stage | standing, then wall states facing [`Facing::ALL`] |
//!
//! Doors store their hinge on the lower half and powered in the saved
//! redstone edge set, like every other door (`mount_states`). Trapdoors act
//! as oak trapdoors for redstone and use (`redstone_blocks::component`), and
//! [`trapdoor_reskin`] puts a computed oak state back on the copper stage.
//! Stairs and slabs are `slab_stair_states` families 53..=56.

use glam::{DVec3, IVec3};

use super::World;
use super::block::{Block, Facing, RenderKind, Shaped};
use super::shape::{Box16, Boxes};
use super::{mount_states as ms, slab_stair_states as ss};
use crate::item::{Item, ToolKind};
use crate::session::placement::PlaceContext;

/// First id of the copper reservation.
pub const FIRST: u16 = 22528;
/// Last id of the reservation (docs/architecture.md).
pub const RESERVED_LAST: u16 = 24575;
/// The unaffected block of copper, registered long before this module.
const COPPER_BLOCK: u16 = 231;

/// Java `WeatheringCopper.WeatherState`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Stage {
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}

impl Stage {
    pub const ALL: [Stage; 4] = [Stage::Unaffected, Stage::Exposed, Stage::Weathered, Stage::Oxidized];

    /// The next stage (`WeatherState.next`), if it ages further.
    pub const fn next(self) -> Option<Stage> {
        match self {
            Stage::Unaffected => Some(Stage::Exposed),
            Stage::Exposed => Some(Stage::Weathered),
            Stage::Weathered => Some(Stage::Oxidized),
            Stage::Oxidized => None,
        }
    }

    /// The stage an axe scrapes back to.
    pub const fn previous(self) -> Option<Stage> {
        match self {
            Stage::Unaffected => None,
            Stage::Exposed => Some(Stage::Unaffected),
            Stage::Weathered => Some(Stage::Exposed),
            Stage::Oxidized => Some(Stage::Weathered),
        }
    }
}

/// A copper family: one Java block per stage (the torch has one stage).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Family {
    Block,
    Cut,
    Chiseled,
    Grate,
    Bulb,
    Stairs,
    Slab,
    Door,
    Trapdoor,
    Bars,
    Chain,
    Lantern,
    Chest,
    Rod,
    Torch,
}

impl Family {
    pub const ALL: [Family; 15] = [
        Family::Block,
        Family::Cut,
        Family::Chiseled,
        Family::Grate,
        Family::Bulb,
        Family::Stairs,
        Family::Slab,
        Family::Door,
        Family::Trapdoor,
        Family::Bars,
        Family::Chain,
        Family::Lantern,
        Family::Chest,
        Family::Rod,
        Family::Torch,
    ];

    /// Ids per stage, registered or reserved.
    pub const fn stride(self) -> u16 {
        match self {
            Family::Block | Family::Cut | Family::Chiseled => 1,
            Family::Grate | Family::Bars => 2,
            Family::Bulb | Family::Lantern => 4,
            Family::Stairs => 80,
            Family::Slab | Family::Torch => 5,
            Family::Door | Family::Chest | Family::Rod => 24,
            Family::Trapdoor => 32,
            Family::Chain => 6,
        }
    }

    /// How many of a stage's locals are registered: lightning rods have no
    /// powered state yet (no lightning).
    pub const fn registered(self) -> u16 {
        match self {
            Family::Rod => 12,
            f => f.stride(),
        }
    }

    pub const fn stages(self) -> u16 {
        if matches!(self, Family::Torch) { 1 } else { 4 }
    }

    /// Java `WeatheringCopper`: every family but the torch ages.
    pub const fn weathers(self) -> bool {
        !matches!(self, Family::Torch)
    }

    /// First id of the family.
    pub const fn origin(self) -> u16 {
        let mut id = FIRST;
        let mut i = 0;
        while i < Family::ALL.len() {
            let f = Family::ALL[i];
            if f as u8 == self as u8 {
                return id;
            }
            id += f.stride() * f.stages();
            i += 1;
        }
        id
    }
}

/// One past the last id this module lays out.
pub const END: u16 = Family::Torch.origin() + Family::Torch.stride();
const _: () = assert!(END - 1 <= RESERVED_LAST, "copper overflows 22528–24575");
const COUNT: usize = (END - FIRST) as usize;

/// Stairs and slabs are `slab_stair_states` families from this index, one per stage.
pub const STAIR_FAMILY: usize = ss::LEGACY_COUNT + super::stone_blocks::SHAPES.len();
pub const STAIR_FAMILIES: usize = 4;

/// A copper state: family, stage and the family's local state.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Copper {
    pub family: Family,
    pub stage: Stage,
    pub local: u16,
}

/// Decodes a registered copper state (the block of copper, 231, included).
pub const fn decode(id: u16) -> Option<Copper> {
    if id == COPPER_BLOCK {
        return Some(Copper { family: Family::Block, stage: Stage::Unaffected, local: 0 });
    }
    if id < FIRST || id >= END {
        return None;
    }
    let mut i = Family::ALL.len();
    while i > 0 {
        i -= 1;
        let family = Family::ALL[i];
        let origin = family.origin();
        if id >= origin {
            let rel = id - origin;
            let (stage, local) = (rel / family.stride(), rel % family.stride());
            let stage = Stage::ALL[stage as usize];
            // The unaffected block of copper keeps its legacy id.
            if local >= family.registered() || (matches!(family, Family::Block) && rel == 0) {
                return None;
            }
            return Some(Copper { family, stage, local });
        }
    }
    None
}

/// The id of a copper state.
pub const fn encode(c: Copper) -> Block {
    if matches!(c.family, Family::Block) && matches!(c.stage, Stage::Unaffected) {
        return Block(COPPER_BLOCK);
    }
    Block(c.family.origin() + c.stage as u16 * c.family.stride() + c.local)
}

/// The block of `family` at `stage` with `local` state.
pub const fn block(family: Family, stage: Stage, local: u16) -> Block {
    encode(Copper { family, stage, local })
}

/// Per-id decode for runtime lookups: bit 15 set for registered states,
/// then family (bits 9..13), stage (7..8) and local (0..6).
static PACKED: [u16; COUNT] = {
    let mut out = [0u16; COUNT];
    let mut i = 0;
    while i < COUNT {
        if let Some(c) = decode(FIRST + i as u16) {
            out[i] = 1 << 15 | (c.family as u16) << 9 | (c.stage as u16) << 7 | c.local;
        }
        i += 1;
    }
    out
};

/// Whether `id` is in this module's laid-out range (registered or not).
#[inline(always)]
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id < END
}

/// The copper state of `b`, if it is copper (231 included).
#[inline]
pub fn state(b: Block) -> Option<Copper> {
    if b.0 == COPPER_BLOCK {
        return decode(COPPER_BLOCK);
    }
    let p = *PACKED.get(b.0.checked_sub(FIRST)? as usize)?;
    (p & 1 << 15 != 0).then(|| Copper {
        family: Family::ALL[(p >> 9 & 15) as usize],
        stage: Stage::ALL[(p >> 7 & 3) as usize],
        local: p & 127,
    })
}

fn of(b: Block, family: Family) -> Option<Copper> {
    state(b).filter(|c| c.family == family)
}

// ---- Textures -----------------------------------------------------------

/// Texture layers: per stage [`t::PER_STAGE`] layers from [`t::BASE`],
/// then the torch. Layers are never saved, so the integrator may move `BASE`.
pub mod t {
    pub const BASE: u16 = 2128;
    pub const PER_STAGE: u16 = 18;
    pub const BLOCK: u16 = 0;
    pub const CUT: u16 = 1;
    pub const CHISELED: u16 = 2;
    pub const GRATE: u16 = 3;
    /// Off, lit, powered, lit and powered.
    pub const BULB: u16 = 4;
    pub const DOOR_TOP: u16 = 8;
    pub const DOOR_BOTTOM: u16 = 9;
    pub const TRAPDOOR: u16 = 10;
    pub const BARS: u16 = 11;
    pub const CHAIN: u16 = 12;
    pub const LANTERN: u16 = 13;
    pub const CHEST_TOP: u16 = 14;
    pub const CHEST_SIDE: u16 = 15;
    pub const CHEST_FRONT: u16 = 16;
    pub const ROD: u16 = 17;
    pub const TORCH: u16 = BASE + 4 * PER_STAGE;
    /// One past the last layer.
    pub const END: u16 = TORCH + 1;
    const _: () = assert!(END <= 2384, "copper layers overflow 2128–2383");

    pub const fn layer(stage: super::Stage, kind: u16) -> u16 {
        BASE + stage as u16 * PER_STAGE + kind
    }

    /// The stage and kind a layer was made for (the torch is `None`).
    pub fn of(layer: u16) -> Option<(super::Stage, u16)> {
        let i = layer.checked_sub(BASE).filter(|&i| i < 4 * PER_STAGE)?;
        Some((super::Stage::ALL[(i / PER_STAGE) as usize], i % PER_STAGE))
    }
}

// ---- Registry -----------------------------------------------------------

const NAMES: [[&str; 4]; 14] = [
    ["block of copper", "exposed copper", "weathered copper", "oxidized copper"],
    ["cut copper", "exposed cut copper", "weathered cut copper", "oxidized cut copper"],
    ["chiseled copper", "exposed chiseled copper", "weathered chiseled copper", "oxidized chiseled copper"],
    ["copper grate", "exposed copper grate", "weathered copper grate", "oxidized copper grate"],
    ["copper bulb", "exposed copper bulb", "weathered copper bulb", "oxidized copper bulb"],
    ["cut copper stairs", "exposed cut copper stairs", "weathered cut copper stairs", "oxidized cut copper stairs"],
    ["cut copper slab", "exposed cut copper slab", "weathered cut copper slab", "oxidized cut copper slab"],
    ["copper door", "exposed copper door", "weathered copper door", "oxidized copper door"],
    ["copper trapdoor", "exposed copper trapdoor", "weathered copper trapdoor", "oxidized copper trapdoor"],
    ["copper bars", "exposed copper bars", "weathered copper bars", "oxidized copper bars"],
    ["copper chain", "exposed copper chain", "weathered copper chain", "oxidized copper chain"],
    ["copper lantern", "exposed copper lantern", "weathered copper lantern", "oxidized copper lantern"],
    ["copper chest", "exposed copper chest", "weathered copper chest", "oxidized copper chest"],
    ["lightning rod", "exposed lightning rod", "weathered lightning rod", "oxidized lightning rod"],
];

/// Java's name of the copper state. The wall torch has Java's block id
/// name; its item and pick block are the torch.
pub const fn name(c: Copper) -> &'static str {
    match c.family {
        Family::Torch if c.local > 0 => "copper wall torch",
        Family::Torch => "copper torch",
        f => NAMES[f as usize][c.stage as usize],
    }
}

/// Name, render kind and textures of this module's registered ids.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    let Some(c) = decode(id) else { return None };
    let s = c.stage;
    let (kind, tex) = match c.family {
        Family::Block => (Opaque, [t::layer(s, t::BLOCK); 6]),
        Family::Cut => (Opaque, [t::layer(s, t::CUT); 6]),
        Family::Chiseled => (Opaque, [t::layer(s, t::CHISELED); 6]),
        Family::Grate => (Cutout, [t::layer(s, t::GRATE); 6]),
        Family::Bulb => (Opaque, [t::layer(s, t::BULB + c.local); 6]),
        Family::Stairs => (Shaped, [t::layer(s, t::CUT); 6]),
        Family::Slab => (
            match c.local {
                0 | 1 => Cutout,
                2 | 3 => Shaped,
                _ => Opaque,
            },
            [t::layer(s, t::CUT); 6],
        ),
        Family::Door => (Shaped, [t::layer(s, if c.local >= 16 { t::DOOR_TOP } else { t::DOOR_BOTTOM }); 6]),
        Family::Trapdoor => (Shaped, [t::layer(s, t::TRAPDOOR); 6]),
        Family::Bars => (Shaped, [t::layer(s, t::BARS); 6]),
        Family::Chain => (Shaped, [t::layer(s, t::CHAIN); 6]),
        Family::Lantern => (Shaped, [t::layer(s, t::LANTERN); 6]),
        Family::Chest => match super::double_chest::state(Block(id)) {
            Some(chest) => (Opaque, super::double_chest::faces(chest)),
            None => return None,
        },
        Family::Rod => (Shaped, [t::layer(s, t::ROD); 6]),
        Family::Torch => (if c.local == 0 { Cross } else { Shaped }, [t::TORCH; 6]),
    };
    Some((name(c), kind, tex))
}

/// Light emitted (`lightLevel`): lit bulbs by stage (Java's 15, 12, 8, 4),
/// lanterns 15 and torches 14.
pub const fn emission(id: u16) -> u8 {
    match decode(id) {
        Some(Copper { family: Family::Bulb, stage, local }) if local & 1 == 1 => [15, 12, 8, 4][stage as usize],
        Some(Copper { family: Family::Lantern, .. }) => 15,
        Some(Copper { family: Family::Torch, .. }) => 14,
        _ => 0,
    }
}

/// A dry grate passes light and skylight like glass (`TransparentBlock`).
pub const fn clear(id: u16) -> bool {
    matches!(decode(id), Some(Copper { family: Family::Grate, local: 0, .. }))
}

/// Grates hide the faces between two grates of the same state, like glass.
pub const fn self_culls(id: u16) -> bool {
    matches!(decode(id), Some(Copper { family: Family::Grate, .. }))
}

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet
/// one (stairs and slabs pair through `slab_stair_states`).
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    let Some(c) = decode(id) else { return None };
    let wet_bit = match c.family {
        Family::Grate | Family::Bars => 1,
        Family::Trapdoor => 16,
        Family::Chain => {
            let other = if c.local >= 3 { c.local - 3 } else { c.local + 3 };
            return Some((encode(Copper { local: other, ..c }).0, c.local >= 3));
        }
        Family::Lantern => 1,
        Family::Chest => 4,
        Family::Rod => {
            let other = if c.local % 12 >= 6 { c.local - 6 } else { c.local + 6 };
            return Some((encode(Copper { local: other, ..c }).0, c.local % 12 >= 6));
        }
        _ => return None,
    };
    Some((encode(Copper { local: c.local ^ wet_bit, ..c }).0, c.local & wet_bit != 0))
}

/// Mining properties: `(hardness, best tool, pickaxe level)` from Java's
/// block properties, `mineable/pickaxe` and `needs_stone_tool`.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    if !owns(b.0) {
        return None;
    }
    let c = state(b)?;
    Some(match c.family {
        Family::Door => (3.0, Some(ToolKind::Pickaxe), None),
        Family::Bars | Family::Chain => (5.0, Some(ToolKind::Pickaxe), Some(0)),
        Family::Lantern => (3.5, Some(ToolKind::Pickaxe), None),
        Family::Torch => (0.0, None, None),
        _ => (3.0, Some(ToolKind::Pickaxe), Some(1)),
    })
}

/// `Block::shaped` for this module's ids.
pub fn shaped(b: Block) -> Option<Shaped> {
    let c = state(b)?;
    Some(match c.family {
        Family::Stairs => Shaped::Stairs(ss::stairs(b)?.facing),
        Family::Slab if matches!(c.local, 2 | 3) => Shaped::Slab,
        Family::Door => {
            let local = c.local % 8;
            Shaped::Door { facing: Facing::ALL[(local % 4) as usize], open: local >= 4, upper: c.local >= 16 }
        }
        Family::Trapdoor => Shaped::Redstone,
        Family::Bars => Shaped::Pane,
        Family::Chain => Shaped::Chain((c.local % 3) as u8),
        Family::Lantern | Family::Rod => Shaped::Custom,
        Family::Torch if c.local > 0 => Shaped::Custom,
        _ => return None,
    })
}

/// The block items, recipes and rules use for a copper state: the dry,
/// default-facing state of its family and stage.
pub fn base(b: Block) -> Option<Block> {
    if !owns(b.0) {
        return None;
    }
    // Local 0 is every family's default: dry, closed, unlit, standing,
    // facing south (stairs and doors) or up (lightning rods), hinged left.
    state(b).map(|c| encode(Copper { local: 0, ..c }))
}

/// `Block::oriented`: stairs, doors and chests with the way they face.
pub fn oriented(b: Block) -> Option<(Block, Facing)> {
    let c = state(b)?;
    match c.family {
        Family::Stairs => ss::stairs(b).map(|s| (ss::FAMILIES[s.family as usize].stairs, s.facing)),
        Family::Door => Some((encode(Copper { local: 0, ..c }), Facing::ALL[(c.local % 4) as usize])),
        Family::Chest => Some((encode(Copper { local: 0, ..c }), Facing::ALL[(c.local % 4) as usize])),
        _ => None,
    }
}

/// `Block::with_facing`: stairs and chests turn, keeping their other properties.
pub fn with_facing(b: Block, facing: Facing) -> Option<Block> {
    let c = state(b).filter(|_| owns(b.0))?;
    match c.family {
        Family::Stairs => ss::with_facing(b, facing),
        Family::Chest => Some(encode(Copper { local: c.local / 4 * 4 + facing as u16, ..c })),
        _ => Some(b),
    }
}

/// What breaking the state drops: its item, except a door's upper half
/// (Java's loot tables; a double slab's second slab is added by `spill_mined`).
pub fn drop(b: Block) -> Option<Option<Item>> {
    if !owns(b.0) {
        return None;
    }
    let c = state(b)?;
    if c.family == Family::Door && c.local >= 16 {
        return Some(None);
    }
    Some(base(b).map(Item::from_block))
}

/// `Block::can_stay_on` for standing lanterns and torches; `None` for the
/// rest (doors keep the generic rule).
pub fn can_stay_on(b: Block, below: Block) -> Option<bool> {
    let c = state(b).filter(|_| owns(b.0))?;
    let centre = || ms::supports_center(below, IVec3::Y);
    match c.family {
        // `LanternBlock.canSurvive`; a hanging lantern checks the block above.
        Family::Lantern => Some(c.local >= 2 || centre()),
        // `TorchBlock.canSurvive`; a wall torch checks its wall (`WallTorchBlock`).
        Family::Torch => Some(c.local > 0 || centre()),
        _ => None,
    }
}

/// A copper wall torch: no collision, and water washes it away.
pub fn is_wall_torch(b: Block) -> bool {
    of(b, Family::Torch).is_some_and(|c| c.local > 0)
}

/// The standing copper torch, in Java's `wall_post_override` tag.
pub fn is_standing_torch(b: Block) -> bool {
    of(b, Family::Torch).is_some_and(|c| c.local == 0)
}

/// `b`'s family with its stage and properties cleared, so redstone
/// bookkeeping treats oxidising or scraping as the same block (Java copies
/// every property across `changeOverTime`, POWERED included).
pub fn unstaged(b: Block) -> Option<Block> {
    state(b).map(|c| encode(Copper { local: 0, stage: Stage::Unaffected, ..c }))
}

/// `PushReaction.POPPED`: copper lanterns and torches break when pushed.
pub fn pops_when_pushed(b: Block) -> bool {
    state(b).is_some_and(|c| matches!(c.family, Family::Lantern | Family::Torch))
}

/// A copper chest of any stage: a 27-slot chest.
pub fn is_chest(b: Block) -> bool {
    owns(b.0) && of(b, Family::Chest).is_some()
}

/// Whether a block change from `old` to `new` keeps a chest's contents:
/// a copper chest aging or scraped stays the same container.
pub fn same_chest(old: Block, new: Block) -> bool {
    is_chest(old) && is_chest(new)
}

/// Shaped boxes the mesher draws with the alpha test.
pub fn see_through(b: Block) -> bool {
    matches!(state(b).map(|c| c.family), Some(Family::Bars | Family::Chain | Family::Lantern | Family::Torch))
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    let families = [
        Family::Block,
        Family::Chiseled,
        Family::Grate,
        Family::Cut,
        Family::Stairs,
        Family::Slab,
        Family::Door,
        Family::Trapdoor,
        Family::Bars,
        Family::Chain,
        Family::Bulb,
        Family::Lantern,
        Family::Chest,
        Family::Rod,
    ];
    families
        .into_iter()
        .flat_map(|f| Stage::ALL.map(|s| block(f, s, 0).0))
        // The block of copper is already in the palette.
        .filter(|&id| id != COPPER_BLOCK)
        .chain([block(Family::Torch, Stage::Unaffected, 0).0])
}

// ---- Families -----------------------------------------------------------

/// The `slab_stair_states` family of a stage: cut copper, its stairs and slab.
pub const fn stair_family(stage: usize) -> ss::Family {
    let s = Stage::ALL[stage];
    ss::Family { base: block(Family::Cut, s, 0), stairs: block(Family::Stairs, s, 0), slab: block(Family::Slab, s, 0) }
}

/// Stage, `lit` and `powered` of a copper bulb.
pub fn bulb(b: Block) -> Option<(Stage, bool, bool)> {
    of(b, Family::Bulb).map(|c| (c.stage, c.local & 1 == 1, c.local & 2 == 2))
}

pub fn bulb_block(stage: Stage, lit: bool, powered: bool) -> Block {
    block(Family::Bulb, stage, lit as u16 + 2 * powered as u16)
}

/// `CopperBulbBlock.getAnalogOutputSignal`: 15 when lit.
pub fn bulb_signal(b: Block) -> Option<u8> {
    bulb(b).map(|(_, lit, _)| if lit { 15 } else { 0 })
}

/// The oak trapdoor state a copper trapdoor acts as (dry; the copper state
/// carries the water).
pub const fn trapdoor_twin(id: u16) -> Option<Block> {
    match decode(id) {
        Some(Copper { family: Family::Trapdoor, local, .. }) => Some(Block(1437 + local % 16)),
        _ => None,
    }
}

/// `to` (an oak trapdoor state computed by redstone rules) on the copper of
/// `from`; `None` when `from` isn't a copper trapdoor.
pub const fn trapdoor_reskin(from: Block, to: Block) -> Option<Block> {
    match decode(from.0) {
        Some(c @ Copper { family: Family::Trapdoor, .. }) if to.0 >= 1437 && to.0 <= 1452 => {
            Some(encode(Copper { local: to.0 - 1437, ..c }))
        }
        Some(Copper { family: Family::Trapdoor, .. }) => Some(to),
        _ => None,
    }
}

/// A copper door half: facing, open, right hinge (lower halves), upper.
fn door(c: Copper) -> (Facing, bool, bool, bool) {
    let local = c.local % 8;
    (Facing::ALL[(local % 4) as usize], local >= 4, (8..16).contains(&c.local), c.local >= 16)
}

fn door_block(stage: Stage, facing: Facing, open: bool, right: bool, upper: bool) -> Block {
    let local = if upper { 16 } else { 8 * right as u16 } + open as u16 * 4 + facing as u16;
    block(Family::Door, stage, local)
}

/// The two halves a copper door item places, closed and hinged left
/// (`content::placement::door_halves`).
pub fn door_halves(item_block: Block, facing: Facing) -> Option<[Block; 2]> {
    let c = of(item_block, Family::Door)?;
    Some([door_block(c.stage, facing, false, false, false), door_block(c.stage, facing, false, false, true)])
}

/// `DoorBlock.HINGE == RIGHT` for copper lower halves.
pub const fn is_right_hinge(id: u16) -> bool {
    matches!(decode(id), Some(Copper { family: Family::Door, local: 8..=15, .. }))
}

/// The right-hinge twin of a left-hinge lower copper door.
pub const fn right_of(left: Block) -> Option<Block> {
    match decode(left.0) {
        Some(c @ Copper { family: Family::Door, local: 0..=7, .. }) => Some(encode(Copper { local: c.local + 8, ..c })),
        _ => None,
    }
}

/// `Block::toggled` for copper doors: the other open state facing `facing`,
/// keeping hinge and half.
pub fn toggled(b: Block, facing: Facing) -> Option<Block> {
    let c = of(b, Family::Door).filter(|_| owns(b.0))?;
    let (_, open, right, upper) = door(c);
    Some(door_block(c.stage, facing, !open, right, upper))
}

/// A dry copper lantern, standing or hanging.
fn lantern(stage: Stage, hanging: bool) -> Block {
    block(Family::Lantern, stage, hanging as u16 * 2)
}

/// Lightning rod direction index: up, down, then [`Facing::ALL`].
fn rod_direction(local: u16) -> IVec3 {
    match local % 6 {
        0 => IVec3::Y,
        1 => IVec3::NEG_Y,
        i => Facing::ALL[(i - 2) as usize].offset(),
    }
}

fn rod_local(normal: IVec3) -> u16 {
    match (normal.x, normal.y, normal.z) {
        (_, 1, _) => 0,
        (_, -1, _) => 1,
        _ => Facing::from_offset(normal).map_or(0, |f| 2 + f as u16),
    }
}

/// The state the item `raw` places with `ctx` for the copper families with
/// their own placement rules (lanterns, torches, lightning rods, chains and
/// chests): `None` if `raw` is not one of them, `Some(None)` if nothing can
/// hold it. Water is added by the shared placement.
pub fn placed(raw: Block, ctx: &PlaceContext, get: impl Fn(IVec3) -> Option<Block>) -> Option<Option<Block>> {
    let c = state(raw).filter(|_| owns(raw.0))?;
    let at = ctx.at;
    let nearest = ms::context_looking(ctx);
    match c.family {
        // `LanternBlock.getStateForPlacement`: the nearest vertical look
        // direction whose block supports the lantern's centre.
        Family::Lantern => {
            Some(nearest.iter().filter(|d| d.y != 0).find_map(|&d| {
                get(at + d).is_some_and(|b| ms::supports_center(b, -d)).then(|| lantern(c.stage, d.y > 0))
            }))
        }
        // `StandingAndWallBlockItem.getPlacementState`: the first look
        // direction (never up) whose state survives; the wall state is the
        // first horizontal direction whose wall is sturdy.
        Family::Torch => {
            let wall = nearest.iter().filter(|d| d.y == 0).find_map(|&d| {
                let facing = Facing::from_offset(d)?.opposite();
                get(at + d).is_some_and(|b| ms::face_sturdy(b, -d)).then_some(facing)
            });
            let standing = get(at - IVec3::Y).is_some_and(|b| ms::supports_center(b, IVec3::Y));
            Some(nearest.iter().find_map(|&d| match d.y {
                1 => None,
                -1 => standing.then(|| block(Family::Torch, Stage::Unaffected, 0)),
                _ => wall.map(|f| block(Family::Torch, Stage::Unaffected, 1 + f as u16)),
            }))
        }
        // `LightningRodBlock.getStateForPlacement`: pointing out of the clicked face.
        Family::Rod => Some(Some(encode(Copper { local: rod_local(ctx.face), ..c }))),
        // `RotatedPillarBlock`: along the clicked face's axis.
        Family::Chain => {
            let axis = if ctx.face.x != 0 {
                1
            } else if ctx.face.z != 0 {
                2
            } else {
                0
            };
            Some(Some(encode(Copper { local: axis, ..c })))
        }
        // `ChestBlock.getStateForPlacement` (`double_chest::placed`).
        Family::Chest => Some(super::double_chest::placed(raw, ctx, get)),
        _ => None,
    }
}

// ---- Shapes -------------------------------------------------------------

const fn b16(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}

/// A box drawn for a block pointing up, turned to point along `dir`.
fn pointing(bx: Box16, dir: IVec3) -> Box16 {
    let (lo, hi) = (bx.min, bx.max);
    match (dir.x, dir.y, dir.z) {
        (_, 1, _) => bx,
        (_, -1, _) => b16([lo[0], 16 - hi[1], lo[2]], [hi[0], 16 - lo[1], hi[2]]),
        // Lay the vertical axis along z (south) or -z (north), x (east) or -x (west).
        (_, _, 1) => b16([lo[0], lo[2], lo[1]], [hi[0], hi[2], hi[1]]),
        (_, _, -1) => b16([lo[0], lo[2], 16 - hi[1]], [hi[0], hi[2], 16 - lo[1]]),
        (1, _, _) => b16([lo[1], lo[0], lo[2]], [hi[1], hi[0], hi[2]]),
        _ => b16([16 - hi[1], lo[0], lo[2]], [16 - lo[1], hi[0], hi[2]]),
    }
}

/// A box drawn for a wall torch facing south (on a wall to its north),
/// turned to `facing` (see `shape::Box16::turned`).
fn turned(bx: Box16, facing: Facing) -> Box16 {
    let turns = match facing {
        Facing::South => 0,
        Facing::West => 1,
        Facing::North => 2,
        Facing::East => 3,
    };
    let mut out = bx;
    for _ in 0..turns {
        let (min, max) = (out.min, out.max);
        out.min = [16 - max[2], min[1], min[0]];
        out.max = [16 - min[2], max[1], max[0]];
    }
    out
}

/// Visible boxes of the `Shaped::Custom` copper states.
pub fn boxes(b: Block, out: &mut Boxes) {
    let Some(c) = state(b) else { return };
    match c.family {
        // `LanternBlock`: a 6×7×6 body under a 4×2×4 cap, one pixel higher hanging.
        Family::Lantern => {
            let up = (c.local >= 2) as u8;
            out.push(b16([5, up, 5], [11, 7 + up, 11]));
            out.push(b16([6, 7 + up, 6], [10, 9 + up, 10]));
        }
        // `template_lightning_rod`: a 2×12×2 rod under a 4×4×4 tip.
        Family::Rod => {
            let dir = rod_direction(c.local);
            out.push(pointing(b16([7, 0, 7], [9, 12, 9]), dir));
            out.push(pointing(b16([6, 12, 6], [10, 16, 10]), dir));
        }
        // A stick against the wall and its head, like the redstone wall torch.
        Family::Torch if c.local > 0 => {
            let facing = Facing::ALL[(c.local - 1) as usize];
            out.push(turned(b16([7, 3, 1], [9, 11, 3]), facing));
            out.push(turned(b16([6, 10, 0], [10, 13, 4]), facing));
        }
        _ => {}
    }
}

/// Collision of the `Shaped::Custom` copper states: lanterns as drawn, a
/// lightning rod's `RodBlock` 4×4 column, and none for wall torches.
pub fn collision(b: Block, out: &mut Boxes) {
    let Some(c) = state(b) else { return };
    match c.family {
        Family::Lantern => boxes(b, out),
        Family::Rod => out.push(pointing(b16([6, 0, 6], [10, 16, 10]), rod_direction(c.local))),
        _ => {}
    }
}

/// Selection outlines that differ from the drawn boxes: the lightning rod's
/// column, `WallTorchBlock`'s 6×10×5 box against the wall (Java's is 5
/// wide; ours rounds to whole pixels) and the standing torch's 4×10×4 column.
pub fn outline(b: Block) -> Option<Boxes> {
    let c = state(b).filter(|_| owns(b.0))?;
    match c.family {
        Family::Rod => Some(Boxes::from_box(pointing(b16([6, 0, 6], [10, 16, 10]), rod_direction(c.local)))),
        Family::Torch if c.local == 0 => Some(Boxes::from_box(b16([6, 0, 6], [10, 10, 10]))),
        Family::Torch => {
            Some(Boxes::from_box(turned(b16([5, 3, 0], [11, 13, 5]), Facing::ALL[(c.local - 1) as usize])))
        }
        _ => None,
    }
}

// ---- World rules --------------------------------------------------------

/// Java `ChangeOverTimeBlock`'s chance each random tick to try aging
/// (`eachBlockOncePerDayChance`).
const AGE_CHANCE: f32 = 0.056_888_89;
/// `ChangeOverTimeBlock.SCAN_DISTANCE`.
const SCAN: i32 = 4;

/// Whether a random tick can age this state: aging families short of
/// oxidized; only a door's lower half (`WeatheringCopperDoorBlock`).
pub fn ages(b: Block) -> bool {
    state(b).is_some_and(|c| {
        c.family.weathers() && c.stage.next().is_some() && !(c.family == Family::Door && c.local >= 16)
    })
}

/// `WeatheringCopperChestBlock.randomTick`: a pair ages through its half
/// that isn't `right` (the other half follows by `updateShape`), and not
/// while anyone has it open.
fn chest_ticks(world: &World, p: IVec3, b: Block) -> bool {
    use super::double_chest as dc;
    match dc::state(b) {
        Some(s) => s.ty != dc::ChestType::Right && world.chest_viewers(p) == 0,
        None => true,
    }
}

impl World {
    /// Uniform in `[0, 1)` from the world's random stream (Java `nextFloat`).
    pub(super) fn next_float(&mut self) -> f32 {
        (self.roll() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// `ChangeOverTimeBlock.changeOverTime`: a 0.0569 chance to try, then
    /// abort if any aging copper within Manhattan distance 4 is younger;
    /// otherwise age with chance `((older + 1) / (older + same + 1))²`,
    /// times 0.75 while unaffected (`getChanceModifier`). All properties
    /// carry over; a door's upper half follows its lower half
    /// (`DoorBlock.updateShape`).
    pub(super) fn tick_copper(&mut self, p: IVec3, b: Block) {
        let Some(c) = state(b) else { return };
        if !ages(b) || !chest_ticks(self, p, b) || self.next_float() >= AGE_CHANCE {
            return;
        }
        let (mut same, mut older) = (0u32, 0u32);
        for dx in -SCAN..=SCAN {
            for dy in -SCAN..=SCAN {
                for dz in -SCAN..=SCAN {
                    let d = IVec3::new(dx, dy, dz);
                    if d == IVec3::ZERO || dx.abs() + dy.abs() + dz.abs() > SCAN {
                        continue;
                    }
                    let Some(n) = self.get_block(p + d).and_then(state).filter(|n| n.family.weathers()) else {
                        continue;
                    };
                    match n.stage.cmp(&c.stage) {
                        std::cmp::Ordering::Less => return,
                        std::cmp::Ordering::Greater => older += 1,
                        std::cmp::Ordering::Equal => same += 1,
                    }
                }
            }
        }
        let chance = (older + 1) as f32 / (older + same + 1) as f32;
        let modifier = if c.stage == Stage::Unaffected { 0.75 } else { 1.0 };
        if self.next_float() < chance * chance * modifier {
            let next = c.stage.next().unwrap();
            self.restage_copper(p, c, next, false);
        }
    }

    /// Moves the copper at `p` to `stage`, keeping its properties, with a
    /// door's other half. `player` edits (scraping) update light at once.
    fn restage_copper(&mut self, p: IVec3, c: Copper, stage: Stage, player: bool) {
        let mut cells = vec![(p, Copper { stage, ..c })];
        if c.family == Family::Door {
            let other = if c.local >= 16 { p - IVec3::Y } else { p + IVec3::Y };
            if let Some(o) = self.get_block(other).and_then(state).filter(|o| o.family == Family::Door) {
                cells.push((other, Copper { stage, ..o }));
            }
        }
        for (q, n) in cells {
            if player {
                self.set_block(q, encode(n));
            } else {
                self.edit(q, encode(n), false);
            }
        }
    }

    /// The axe's `scrape` block transformer (`block_transformer/axe.json`):
    /// copper one stage back, keeping its properties (both door halves).
    /// Returns whether it scraped.
    pub fn scrape_copper(&mut self, p: IVec3) -> bool {
        let Some(c) = self.get_block(p).and_then(state).filter(|c| c.family.weathers()) else { return false };
        let Some(previous) = c.stage.previous() else { return false };
        self.restage_copper(p, c, previous, true);
        // `ParticleUtils.spawnParticlesOnBlockFaces(.., SCRAPE, 3..5)`.
        let mut burst =
            crate::particles::Burst::new(crate::particles::Kind::Effect, p.as_dvec3() + DVec3::splat(0.5), 12);
        burst.spread = DVec3::splat(0.45);
        burst.color = Some([0.42, 0.82, 0.74, 1.0]);
        self.particles.push(crate::particles::Request::Burst(burst));
        true
    }

    /// `CopperBulbBlock.checkAndFlip`: on a change of neighbour signal the
    /// bulb stores it as POWERED, and a rising edge toggles LIT.
    pub(super) fn copper_bulb_update(&mut self, p: IVec3, b: Block) {
        let Some((stage, lit, powered)) = bulb(b) else { return };
        let signal = self.redstone_power(p) > 0;
        if signal != powered {
            let lit = if powered { lit } else { !lit };
            self.edit(p, bulb_block(stage, lit, signal), false);
        }
    }

    /// `LanternBlock.updateShape` and `WallTorchBlock.updateShape` for
    /// copper: a hanging lantern under `p` or a wall torch on one of its
    /// sides breaks once `p` no longer holds it (a lantern leaves its water).
    pub(super) fn drop_unsupported_copper(&mut self, p: IVec3) {
        let here = self.get_block(p).unwrap_or(Block::AIR);
        let below = p - IVec3::Y;
        if let Some(lantern) = self.get_block(below).filter(|&l| of(l, Family::Lantern).is_some_and(|c| c.local >= 2))
            && !ms::supports_center(here, IVec3::NEG_Y)
        {
            self.edit(below, if lantern.is_waterlogged() { Block::WATER } else { Block::AIR }, true);
            self.spill_block(below, lantern);
        }
        for f in Facing::ALL {
            let at = p + f.offset();
            if let Some(torch) = self.get_block(at).filter(|&t| is_wall_torch(t))
                && state(torch).is_some_and(|c| Facing::ALL[(c.local - 1) as usize] == f)
                && !ms::face_sturdy(here, f.offset())
            {
                self.edit(at, Block::AIR, true);
                self.spill_block(at, torch);
            }
        }
    }
}

#[cfg(test)]
mod tests;
