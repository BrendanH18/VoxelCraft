//! Double chests (Java 26.3 `ChestBlock`, `ChestType`,
//! `DoubleBlockCombiner`, `CopperChestBlock` and
//! `WeatheringCopperChestBlock`). Chests, trapped chests and copper chests
//! have Java's `type` property: `single`, or the `left` or `right` half of
//! a pair. Two halves facing the same way, side by side, share one 54-slot
//! inventory; each half keeps its own 27 slots in the chest table, the
//! right half's first (`CompoundContainer(first, second)`).
//!
//! State ids (docs/architecture.md):
//!
//! | Chest | Ids |
//! | --- | --- |
//! | chest | single 53–56 ([`Facing::ALL`] order) and their wet partners; left and right from [`PLAIN_DOUBLE`]: `28952 + wet * 8 + (type - 1) * 4 + facing` (north, east, south, west) |
//! | trapped chest | `28880 + wet * 12 + type * 4 + facing` (north, east, south, west; `utilities`) |
//! | copper chests | `type * 8 + wet * 4 + facing` per stage ([`Facing::ALL`] order; `copper`) |
//!
//! Pairing follows Java: placement joins a single neighbour facing the
//! same way (or, sneaking, the chest whose side was clicked), and
//! `updateShape` keeps both halves in step as neighbours change. Copper
//! chests pair across stages; the pair takes the least oxidised stage and
//! ages together, only the half that isn't `right` taking random ticks.

use glam::IVec3;

use super::World;
use super::block::{Block, Facing};
use super::copper::{self, Stage};
use super::shape::Box16;
use crate::inventory::Stack;
use crate::session::placement::PlaceContext;

/// Java `ChestType`, in its declaration order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ChestType {
    Single,
    Left,
    Right,
}

impl ChestType {
    pub const ALL: [ChestType; 3] = [ChestType::Single, ChestType::Left, ChestType::Right];

    /// `ChestType.getOpposite`: the partner half's type.
    pub const fn opposite(self) -> ChestType {
        match self {
            ChestType::Single => ChestType::Single,
            ChestType::Left => ChestType::Right,
            ChestType::Right => ChestType::Left,
        }
    }
}

/// Which Java block a chest state belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Plain,
    Trapped,
    /// A copper chest at its oxidation stage (waxed chests come with bees).
    Copper(Stage),
}

impl Kind {
    /// `chestCanConnectTo`: a chest joins its own block, a copper chest
    /// any copper chest (`BlockTags.COPPER_CHESTS`).
    pub const fn connects(self, other: Kind) -> bool {
        matches!(
            (self, other),
            (Kind::Plain, Kind::Plain) | (Kind::Trapped, Kind::Trapped) | (Kind::Copper(_), Kind::Copper(_))
        )
    }
}

/// A chest state's properties.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChestState {
    pub kind: Kind,
    pub facing: Facing,
    pub ty: ChestType,
    pub wet: bool,
}

/// First plain-chest double-half id; 16 ids follow.
pub const PLAIN_DOUBLE: u16 = 28952;
/// One past the plain double halves.
pub const PLAIN_DOUBLE_END: u16 = PLAIN_DOUBLE + 16;
const TRAPPED_FIRST: u16 = 28880;

/// The single chests' wet partners (BLD-04f), in [`Facing::ALL`] order.
const PLAIN_WET: [u16; 4] = {
    let mut out = [0; 4];
    let mut i = 0;
    while i < 4 {
        out[i] = match super::waterlog::family_pair(53 + i as u16) {
            Some((wet, false)) => wet,
            _ => panic!("the chest has a wet partner"),
        };
        i += 1;
    }
    out
};

/// Facings in the utilities modules' id order.
const NESW: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

const fn nesw(f: Facing) -> u16 {
    match f {
        Facing::North => 0,
        Facing::East => 1,
        Facing::South => 2,
        Facing::West => 3,
    }
}

const fn type_index(t: ChestType) -> u16 {
    t as u16
}

/// The chest state of `b`, if it is a chest, trapped chest or copper chest
/// (ender chests and barrels never pair).
pub const fn state(b: Block) -> Option<ChestState> {
    let id = b.0;
    if id >= 53 && id <= 56 {
        let facing = Facing::ALL[(id - 53) as usize];
        return Some(ChestState { kind: Kind::Plain, facing, ty: ChestType::Single, wet: false });
    }
    if id >= PLAIN_DOUBLE && id < PLAIN_DOUBLE_END {
        let i = id - PLAIN_DOUBLE;
        let ty = if (i % 8) / 4 == 0 { ChestType::Left } else { ChestType::Right };
        return Some(ChestState { kind: Kind::Plain, facing: NESW[(i % 4) as usize], ty, wet: i >= 8 });
    }
    if id >= TRAPPED_FIRST && id < TRAPPED_FIRST + 24 {
        let i = id - TRAPPED_FIRST;
        let ty = ChestType::ALL[((i % 12) / 4) as usize];
        return Some(ChestState { kind: Kind::Trapped, facing: NESW[(i % 4) as usize], ty, wet: i >= 12 });
    }
    if copper::owns(id) {
        return match copper::decode(id) {
            Some(c) if matches!(c.family, copper::Family::Chest) => Some(ChestState {
                kind: Kind::Copper(c.stage),
                facing: Facing::ALL[(c.local % 4) as usize],
                ty: ChestType::ALL[(c.local / 8) as usize],
                wet: (c.local / 4) % 2 == 1,
            }),
            _ => None,
        };
    }
    let mut i = 0;
    while i < 4 {
        if id == PLAIN_WET[i] {
            return Some(ChestState { kind: Kind::Plain, facing: Facing::ALL[i], ty: ChestType::Single, wet: true });
        }
        i += 1;
    }
    None
}

/// The dry single chest a plain chest's double half answers for in
/// property lookups (`Block::alias`).
#[inline]
pub fn plain_single(id: u16) -> Option<Block> {
    (PLAIN_DOUBLE..PLAIN_DOUBLE_END).contains(&id).then(|| Block(53 + NESW[((id - PLAIN_DOUBLE) % 4) as usize] as u16))
}

/// The id of a chest state.
pub const fn block(s: ChestState) -> Block {
    let f = s.facing;
    match s.kind {
        Kind::Plain => match s.ty {
            ChestType::Single if s.wet => Block(PLAIN_WET[f as usize]),
            ChestType::Single => Block(53 + f as u16),
            ty => Block(PLAIN_DOUBLE + s.wet as u16 * 8 + (type_index(ty) - 1) * 4 + nesw(f)),
        },
        Kind::Trapped => Block(TRAPPED_FIRST + s.wet as u16 * 12 + type_index(s.ty) * 4 + nesw(f)),
        Kind::Copper(stage) => {
            copper::block(copper::Family::Chest, stage, type_index(s.ty) * 8 + s.wet as u16 * 4 + f as u16)
        }
    }
}

/// `ChestBlock.getConnectedDirection`: where a half's partner is (a
/// single chest's answer is never used).
pub const fn connected(facing: Facing, ty: ChestType) -> Facing {
    match ty {
        ChestType::Left => facing.clockwise(),
        _ => facing.counter_clockwise(),
    }
}

/// `ChestBlock.candidatePartnerFacing`: the facing of a single chest at
/// `p` that a chest of `kind` could join.
fn candidate_facing(kind: Kind, p: IVec3, get: &impl Fn(IVec3) -> Option<Block>) -> Option<Facing> {
    let s = get(p).and_then(state)?;
    (kind.connects(s.kind) && s.ty == ChestType::Single).then_some(s.facing)
}

/// `ChestBlock.getStateForPlacement` (and `CopperChestBlock`'s): the
/// chest `raw` faces the placer; sneaking against a chest's side joins
/// that chest, taking its facing, and otherwise a single neighbour facing
/// the same way on either side becomes the partner (clockwise first). A
/// copper chest that pairs takes the less oxidised of the two stages.
/// `None` if `raw` is no chest. Water is added by the shared placement.
pub fn placed(raw: Block, ctx: &PlaceContext, get: impl Fn(IVec3) -> Option<Block>) -> Option<Block> {
    let s = state(raw)?;
    let mut facing = ctx.facing.opposite();
    let mut ty = ChestType::Single;
    let face = Facing::from_offset(ctx.face);
    if let Some(face) = face
        && ctx.sneaking
        && let Some(partner) = candidate_facing(s.kind, ctx.at - face.offset(), &get)
        && partner.along_x() != face.along_x()
    {
        facing = partner;
        ty = if facing.counter_clockwise() == face.opposite() { ChestType::Right } else { ChestType::Left };
    }
    if ty == ChestType::Single && !ctx.sneaking {
        ty = if candidate_facing(s.kind, ctx.at + facing.clockwise().offset(), &get) == Some(facing) {
            ChestType::Left
        } else if candidate_facing(s.kind, ctx.at + facing.counter_clockwise().offset(), &get) == Some(facing) {
            ChestType::Right
        } else {
            ChestType::Single
        };
    }
    let mut kind = s.kind;
    // `getLeastOxidizedChestOfConnectedBlocks`.
    if ty != ChestType::Single
        && let Kind::Copper(mine) = kind
        && let Some(Kind::Copper(theirs)) = get(ctx.at + connected(facing, ty).offset()).and_then(state).map(|p| p.kind)
        && theirs < mine
    {
        kind = Kind::Copper(theirs);
    }
    Some(block(ChestState { kind, facing, ty, wet: s.wet }))
}

/// `ChestBlock.updateShape` (with `CopperChestBlock`'s) for the chest
/// `here` when its horizontal neighbour toward `dir` is now `neighbour`:
/// a single chest joins a half that points at it; a half whose partner
/// is no longer a chest becomes single; a copper half takes its
/// partner's stage. `None` when nothing changes.
pub fn reshaped(here: Block, dir: Facing, neighbour: Block) -> Option<Block> {
    let mut s = state(here)?;
    let n = state(neighbour).filter(|n| s.kind.connects(n.kind));
    match n {
        Some(n)
            if n.ty != ChestType::Single
                && s.ty == ChestType::Single
                && s.facing == n.facing
                && connected(n.facing, n.ty) == dir.opposite() =>
        {
            s.ty = n.ty.opposite();
        }
        Some(_) => {}
        None if s.ty != ChestType::Single && connected(s.facing, s.ty) == dir => s.ty = ChestType::Single,
        None => {}
    }
    // `CopperChestBlock.updateShape`: a half facing a copper chest takes its block.
    if let (Some(n), Kind::Copper(_)) = (n, s.kind)
        && s.ty != ChestType::Single
        && connected(s.facing, s.ty) == dir
    {
        s.kind = n.kind;
    }
    let b = block(s);
    (b != here).then_some(b)
}

/// What an edit changes for its chest neighbours: kind, facing and type.
pub fn shape_key(b: Block) -> Option<(Kind, Facing, ChestType)> {
    state(b).map(|s| (s.kind, s.facing, s.ty))
}

/// `ChestBlock.getShape`: a single chest is inset a pixel on every side
/// and two short of the top (`Block.column(14, 0, 14)`); a half reaches
/// the side it shares with its partner.
pub fn shape(b: Block) -> Option<Box16> {
    let s = state(b)?;
    let (mut min, mut max) = ([1, 0, 1], [15, 14, 15]);
    if s.ty != ChestType::Single {
        match connected(s.facing, s.ty) {
            Facing::North => min[2] = 0,
            Facing::South => max[2] = 16,
            Facing::West => min[0] = 0,
            Facing::East => max[0] = 16,
        }
    }
    Some(Box16 { min, max })
}

// ---- Textures -----------------------------------------------------------

/// Texture layers of the double halves, from `utilities::t::DOUBLE`: per
/// material (wood, then the four copper stages) the back with its joint at
/// the low or high end of the face, then the top with its joint at low or
/// high x and low or high z; then each front (chest, trapped chest, the
/// four copper stages) with its joint low or high. Drawn from the single
/// textures by [`texture`].
pub mod tex {
    pub const BASE: u16 = crate::world::utilities::t::DOUBLE;
    pub const PER_MATERIAL: u16 = 6;
    pub const SIDE: u16 = 0;
    pub const TOP_X: u16 = 2;
    pub const TOP_Z: u16 = 4;
    pub const FRONTS: u16 = BASE + 5 * PER_MATERIAL;
    pub const COUNT: u16 = 5 * PER_MATERIAL + 6 * 2;
}

const fn material(kind: Kind) -> u16 {
    match kind {
        Kind::Plain | Kind::Trapped => 0,
        Kind::Copper(s) => 1 + s as u16,
    }
}

const fn front_kind(kind: Kind) -> u16 {
    match kind {
        Kind::Plain => 0,
        Kind::Trapped => 1,
        Kind::Copper(s) => 2 + s as u16,
    }
}

/// Single front, side and top layers of a material or front kind.
const fn singles(kind: Kind) -> (u16, u16, u16) {
    use super::block::tex as b;
    match kind {
        Kind::Plain => (b::CHEST_FRONT, b::CHEST_SIDE, b::CHEST_TOP),
        Kind::Trapped => (super::utilities::t::TRAPPED_FRONT, b::CHEST_SIDE, b::CHEST_TOP),
        Kind::Copper(s) => (
            copper::t::layer(s, copper::t::CHEST_FRONT),
            copper::t::layer(s, copper::t::CHEST_SIDE),
            copper::t::layer(s, copper::t::CHEST_TOP),
        ),
    }
}

/// Face textures (+X, -X, +Y, -Y, +Z, -Z) of a chest state. Faces are
/// textured by world position (u runs along +X or +Z), so a half's
/// joint is at the high end of a face when its partner is east or south.
pub const fn faces(s: ChestState) -> [u16; 6] {
    let (front, side, top) = singles(s.kind);
    let mut t = [side, side, top, top, side, side];
    if matches!(s.ty, ChestType::Single) {
        t[s.facing.face()] = front;
        return t;
    }
    let d = connected(s.facing, s.ty);
    let hi = matches!(d, Facing::East | Facing::South) as u16;
    let m = tex::BASE + material(s.kind) * tex::PER_MATERIAL;
    let top = m + if d.along_x() { tex::TOP_X } else { tex::TOP_Z } + hi;
    t[2] = top;
    t[3] = top;
    t[s.facing.opposite().face()] = m + tex::SIDE + hi;
    t[s.facing.face()] = tex::FRONTS + front_kind(s.kind) * 2 + hi;
    t
}

/// How a double-half layer is drawn: the single layer it copies, whether
/// the joint runs along the texture's rows (a top joined north or south)
/// rather than its columns, and whether the joint is at the high end.
pub fn texture(layer: u16) -> Option<(u16, bool, bool)> {
    let i = layer.checked_sub(tex::BASE).filter(|&i| i < tex::COUNT)?;
    const KINDS: [Kind; 6] = [
        Kind::Plain,
        Kind::Trapped,
        Kind::Copper(Stage::Unaffected),
        Kind::Copper(Stage::Exposed),
        Kind::Copper(Stage::Weathered),
        Kind::Copper(Stage::Oxidized),
    ];
    let hi = i % 2 == 1;
    if i >= 5 * tex::PER_MATERIAL {
        let k = (i - 5 * tex::PER_MATERIAL) / 2;
        return Some((singles(KINDS[k as usize]).0, false, hi));
    }
    const MATERIALS: [Kind; 5] = [
        Kind::Plain,
        Kind::Copper(Stage::Unaffected),
        Kind::Copper(Stage::Exposed),
        Kind::Copper(Stage::Weathered),
        Kind::Copper(Stage::Oxidized),
    ];
    let (_, side, top) = singles(MATERIALS[(i / tex::PER_MATERIAL) as usize]);
    Some(match (i % tex::PER_MATERIAL) / 2 {
        0 => (side, false, hi),
        1 => (top, false, hi),
        _ => (top, true, hi),
    })
}

/// The single texture's column (or row) a double half's pixel `i` copies
/// along the joint axis: the outer frame stays, the frame at the joint is
/// replaced by plain interior, and the joint pixel shows its half of the
/// centre latch (columns 7 and 8 of the single front).
pub const fn joint_source(i: usize, hi: bool) -> usize {
    if hi {
        match i {
            0..=6 => i,
            15 => 7,
            _ => 9 + (i - 7) % 6,
        }
    } else {
        match i {
            9..=15 => i,
            0 => 8,
            _ => 1 + (i - 1) % 6,
        }
    }
}

// ---- World --------------------------------------------------------------

impl World {
    /// `DoubleBlockCombiner.combineWithNeigbour`: the other half of the
    /// chest at `p` when both halves agree (same block, same facing,
    /// opposite types).
    pub fn chest_partner(&self, p: IVec3) -> Option<IVec3> {
        let s = self.get_block(p).and_then(state).filter(|s| s.ty != ChestType::Single)?;
        let q = p + connected(s.facing, s.ty).offset();
        let n = self.get_block(q).and_then(state)?;
        (n.kind == s.kind && n.facing == s.facing && n.ty == s.ty.opposite()).then_some(q)
    }

    /// The halves of the container at `p`, the first 27 slots' half first
    /// (Java's right half), or `p` alone.
    pub fn chest_halves(&self, p: IVec3) -> (IVec3, Option<IVec3>) {
        match self.chest_partner(p) {
            Some(q) if self.get_block(p).and_then(state).is_some_and(|s| s.ty == ChestType::Right) => (p, Some(q)),
            Some(q) => (q, Some(p)),
            None => (p, None),
        }
    }

    /// `ChestBlock.isChestBlockedAt` over both halves (`combine` with the
    /// blocked test): a redstone conductor on top of either half keeps the
    /// chest shut and its comparator output at 0. Sitting cats, the other
    /// test, don't sit on chests yet.
    pub fn chest_opens(&self, p: IVec3) -> bool {
        let blocked = |q: IVec3| self.get_block(q + IVec3::Y).is_some_and(super::redstone_blocks::conductor);
        let (first, second) = self.chest_halves(p);
        !blocked(first) && !second.is_some_and(blocked)
    }

    /// Where slot `i` of the container at `p` is kept: a half and its slot.
    fn chest_cell(&self, p: IVec3, i: usize) -> (IVec3, usize) {
        match self.chest_halves(p) {
            (_, Some(second)) if i >= super::chest::SLOTS => (second, i - super::chest::SLOTS),
            (first, _) => (first, i),
        }
    }

    /// Slot `i` of the container at `p` (both halves of a double chest).
    pub fn chest_slot(&self, p: IVec3, i: usize) -> Option<Stack> {
        let (cell, i) = self.chest_cell(p, i);
        self.chest(cell)?.slots.get(i).copied().flatten()
    }

    /// Writes slot `i` of the container at `p`; false without a chest.
    pub fn set_chest_slot(&mut self, p: IVec3, i: usize, stack: Option<Stack>) -> bool {
        let (cell, i) = self.chest_cell(p, i);
        let Some(slot) = self.chest_mut(cell).and_then(|c| c.slots.get_mut(i)) else { return false };
        *slot = stack;
        self.chest_contents_changed(p);
        true
    }

    /// Runs `f` on every slot of the container at `p` as one slice (54 for
    /// a double chest) and writes the slots back; `None` without a chest.
    pub fn with_chest_slots<R>(&mut self, p: IVec3, f: impl FnOnce(&mut [Option<Stack>]) -> R) -> Option<R> {
        const N: usize = super::chest::SLOTS;
        let count = self.container_slots(p);
        let (first, second) = self.chest_halves(p);
        let mut slots = [None; 2 * N];
        slots[..N].copy_from_slice(&self.chest(first)?.slots);
        if let Some(c) = second.and_then(|q| self.chest(q)) {
            slots[N..].copy_from_slice(&c.slots);
        }
        let out = f(&mut slots[..count]);
        if let Some(c) = self.chest_mut(first) {
            c.slots.copy_from_slice(&slots[..N]);
        }
        if let Some(c) = second.and_then(|q| self.chest_mut(q)) {
            c.slots.copy_from_slice(&slots[N..]);
        }
        self.chest_contents_changed(p);
        Some(out)
    }

    /// `CompoundContainer.setChanged`: comparators by either half see it.
    fn chest_contents_changed(&mut self, p: IVec3) {
        self.redstone_changed(p);
        if let Some(q) = self.chest_partner(p) {
            self.redstone_changed(q);
        }
    }

    /// `ChestBlock.updateShape` for the chests beside a changed cell.
    pub(super) fn reshape_chests_beside(&mut self, p: IVec3, old: Block, block: Block, sync: bool) {
        if shape_key(old) == shape_key(block) {
            return;
        }
        for f in Facing::ALL {
            let n = p + f.offset();
            let Some(chest) = self.get_block(n).filter(|&b| state(b).is_some()) else { continue };
            if let Some(shaped) = reshaped(chest, f.opposite(), block) {
                self.edit(n, shaped, sync);
            }
        }
    }
}

#[cfg(test)]
mod tests;
