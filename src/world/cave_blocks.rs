//! v0.13 underground blocks (GEN-02c): glow lichen and the sculk family
//! (sculk, sculk vein, sculk catalyst, sculk sensor and sculk shrieker).
//!
//! Every Java 26.3 state has its own id in track 3's reservation
//! (`docs/architecture.md`, v0.13 track reservations):
//!
//! | Ids | States |
//! |---|---|
//! | `9728 + 64 * wet + faces` | glow lichen, `faces` 1..=63 ([`DIRS`] bits) |
//! | `9856 + 64 * wet + faces` | sculk vein, likewise |
//! | 11264 | sculk |
//! | 11265, 11266 | sculk catalyst, `bloom` false then true |
//! | `11268 + 4 * wet + phase` | sculk sensor, [`Phase`] inactive, active, cooldown |
//! | `11276 + 4 * wet + 2 * shrieking + can_summon` | sculk shrieker |
//!
//! Face bits follow Java's `Direction` order (down, up, north, south, west,
//! east): bit `i` set means the block lies on the face of its neighbour in
//! direction [`DIRS`]`[i]`. A multiface state with no face doesn't exist, so
//! ids `9728`, `9792`, `9856` and `9920` stay unregistered.
//!
//! The catalyst, sensor and shrieker are inert world blocks until v0.16
//! (deep dark and the warden): no vibrations, shrieks, warden summoning or
//! catalyst blooms on mob deaths. Their states exist so later behaviour and
//! saved worlds need no new ids. Sources and differences:
//! `docs/verification/2026-10-30-v0.13-underground.md`.

#[cfg(test)]
mod tests;

use glam::{IVec3, Vec3};

use super::block::{Block, RenderKind};
use super::shape::{Box16, Boxes};
use crate::inventory::Stack;
use crate::item::{Item, ToolKind};

/// First glow lichen id (faces 0, unregistered).
pub const LICHEN_FIRST: u16 = 9728;
/// First sculk vein id (faces 0, unregistered).
pub const VEIN_FIRST: u16 = 9856;
/// One past the multiface states; the first reserved range ends here.
const MULTIFACE_END: u16 = 9984;
/// The second reserved range (11264..=11519) starts with sculk.
const SECOND_FIRST: u16 = 11264;
pub const SCULK: Block = Block(11264);
pub const SCULK_CATALYST: Block = Block(11265);
const SENSOR_FIRST: u16 = 11268;
const SHRIEKER_FIRST: u16 = 11276;
/// One past the last registered state; 11284..=11519 stay free.
const SECOND_END: u16 = 11284;
const _: () = assert!(SECOND_END <= 11520);

/// The neighbour each face bit lies on, in Java's `Direction` order.
pub const DIRS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];
pub const DOWN: u8 = 1;
pub const UP: u8 = 2;
pub const ALL_FACES: u8 = 63;

/// The two `MultifaceBlock` kinds VoxelCraft registers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Multiface {
    GlowLichen,
    SculkVein,
}

impl Multiface {
    const fn first(self) -> u16 {
        match self {
            Multiface::GlowLichen => LICHEN_FIRST,
            Multiface::SculkVein => VEIN_FIRST,
        }
    }
}

/// A glow lichen or sculk vein on the faces `faces` (non-zero; clamped to
/// the six bits).
pub const fn multiface(kind: Multiface, faces: u8, wet: bool) -> Block {
    let faces = if faces & ALL_FACES == 0 { DOWN } else { faces & ALL_FACES };
    Block(kind.first() + wet as u16 * 64 + faces as u16)
}

/// The kind, faces and `waterlogged` of a multiface state.
pub const fn multiface_of(b: Block) -> Option<(Multiface, u8, bool)> {
    let (kind, i) = if b.0 >= LICHEN_FIRST && b.0 < VEIN_FIRST {
        (Multiface::GlowLichen, b.0 - LICHEN_FIRST)
    } else if b.0 >= VEIN_FIRST && b.0 < MULTIFACE_END {
        (Multiface::SculkVein, b.0 - VEIN_FIRST)
    } else {
        return None;
    };
    let faces = (i % 64) as u8;
    if faces == 0 {
        return None;
    }
    Some((kind, faces, i >= 64))
}

/// The item and default state: lying on the floor.
pub const GLOW_LICHEN: Block = multiface(Multiface::GlowLichen, DOWN, false);
pub const SCULK_VEIN: Block = multiface(Multiface::SculkVein, DOWN, false);

/// Java's `SculkSensorPhase`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Inactive,
    Active,
    Cooldown,
}

impl Phase {
    pub const ALL: [Phase; 3] = [Phase::Inactive, Phase::Active, Phase::Cooldown];
}

pub const fn sculk_sensor(phase: Phase, wet: bool) -> Block {
    Block(SENSOR_FIRST + wet as u16 * 4 + phase as u16)
}

/// The phase and `waterlogged` of a sculk sensor.
pub const fn sensor_of(b: Block) -> Option<(Phase, bool)> {
    if b.0 < SENSOR_FIRST || b.0 >= SHRIEKER_FIRST {
        return None;
    }
    let i = b.0 - SENSOR_FIRST;
    let phase = match i % 4 {
        0 => Phase::Inactive,
        1 => Phase::Active,
        2 => Phase::Cooldown,
        _ => return None,
    };
    Some((phase, i >= 4))
}

pub const SCULK_SENSOR: Block = sculk_sensor(Phase::Inactive, false);

pub const fn sculk_shrieker(shrieking: bool, can_summon: bool, wet: bool) -> Block {
    Block(SHRIEKER_FIRST + wet as u16 * 4 + shrieking as u16 * 2 + can_summon as u16)
}

/// `(shrieking, can_summon, waterlogged)` of a sculk shrieker.
pub const fn shrieker_of(b: Block) -> Option<(bool, bool, bool)> {
    if b.0 < SHRIEKER_FIRST || b.0 >= SECOND_END {
        return None;
    }
    let i = b.0 - SHRIEKER_FIRST;
    Some((i & 2 != 0, i & 1 != 0, i >= 4))
}

pub const SCULK_SHRIEKER: Block = sculk_shrieker(false, false, false);

pub const fn sculk_catalyst(bloom: bool) -> Block {
    Block(SCULK_CATALYST.0 + bloom as u16)
}

/// The `bloom` of a sculk catalyst.
pub const fn catalyst_bloom(b: Block) -> Option<bool> {
    match b.0 {
        11265 => Some(false),
        11266 => Some(true),
        _ => None,
    }
}

/// Whether `id` is in this module's reservations (registered or not).
#[inline]
pub const fn owns(id: u16) -> bool {
    (id >= LICHEN_FIRST && id < MULTIFACE_END) || (id >= SECOND_FIRST && id < SECOND_END)
}

/// Texture layers, offsets from one base so the integrator can repack them
/// (track 3 owns 2768..=2831).
pub const TEX: u16 = 2768;
pub mod t {
    use super::TEX;
    pub const GLOW_LICHEN: u16 = TEX;
    pub const SCULK_VEIN: u16 = TEX + 1;
    pub const SCULK: u16 = TEX + 2;
    pub const CATALYST_SIDE: u16 = TEX + 3;
    pub const CATALYST_TOP: u16 = TEX + 4;
    pub const CATALYST_SIDE_BLOOM: u16 = TEX + 5;
    pub const CATALYST_TOP_BLOOM: u16 = TEX + 6;
    pub const CATALYST_BOTTOM: u16 = TEX + 7;
    /// The sensor's side: tendrils over the upper half, the body below.
    pub const SENSOR_SIDE: u16 = TEX + 8;
    pub const SENSOR_TOP: u16 = TEX + 9;
    pub const SENSOR_SIDE_ACTIVE: u16 = TEX + 10;
    pub const SENSOR_TOP_ACTIVE: u16 = TEX + 11;
    pub const SENSOR_BOTTOM: u16 = TEX + 12;
    /// The shrieker's side: its bony rim over the upper half, the body below.
    pub const SHRIEKER_SIDE: u16 = TEX + 13;
    pub const SHRIEKER_TOP: u16 = TEX + 14;
    pub const SHRIEKER_TOP_CAN_SUMMON: u16 = TEX + 15;
    pub const SHRIEKER_BOTTOM: u16 = TEX + 16;
    pub const COUNT: u16 = 17;
}
/// One past this module's last layer.
pub const TEX_END: u16 = TEX + t::COUNT;
const _: () = assert!(TEX_END <= 2832);

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

const fn column(side: u16, top: u16, bottom: u16) -> [u16; 6] {
    [side, side, top, bottom, side, side]
}

/// Name, render kind and face textures of every registered state.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    if let Some((kind, _, _)) = multiface_of(Block(id)) {
        return Some(match kind {
            Multiface::GlowLichen => ("glow lichen", Shaped, all(t::GLOW_LICHEN)),
            Multiface::SculkVein => ("sculk vein", Shaped, all(t::SCULK_VEIN)),
        });
    }
    if id == SCULK.0 {
        return Some(("sculk", Opaque, all(t::SCULK)));
    }
    if let Some(bloom) = catalyst_bloom(Block(id)) {
        let tex = if bloom {
            column(t::CATALYST_SIDE_BLOOM, t::CATALYST_TOP_BLOOM, t::CATALYST_BOTTOM)
        } else {
            column(t::CATALYST_SIDE, t::CATALYST_TOP, t::CATALYST_BOTTOM)
        };
        return Some(("sculk catalyst", Opaque, tex));
    }
    if let Some((phase, _)) = sensor_of(Block(id)) {
        let tex = match phase {
            Phase::Active => column(t::SENSOR_SIDE_ACTIVE, t::SENSOR_TOP_ACTIVE, t::SENSOR_BOTTOM),
            _ => column(t::SENSOR_SIDE, t::SENSOR_TOP, t::SENSOR_BOTTOM),
        };
        return Some(("sculk sensor", Shaped, tex));
    }
    if let Some((_, can_summon, _)) = shrieker_of(Block(id)) {
        let top = if can_summon { t::SHRIEKER_TOP_CAN_SUMMON } else { t::SHRIEKER_TOP };
        return Some(("sculk shrieker", Shaped, column(t::SHRIEKER_SIDE, top, t::SHRIEKER_BOTTOM)));
    }
    None
}

/// Glow lichen and sculk veins have no collision (`noCollission`).
pub const fn passable(id: u16) -> bool {
    multiface_of(Block(id)).is_some()
}

/// The other state of a waterlogged pair, and whether `id` is the wet one.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if let Some((_, _, wet)) = multiface_of(Block(id)) {
        return Some(if wet { (id - 64, true) } else { (id + 64, false) });
    }
    if let Some((_, wet)) = sensor_of(Block(id)) {
        return Some(if wet { (id - 4, true) } else { (id + 4, false) });
    }
    if let Some((_, _, wet)) = shrieker_of(Block(id)) {
        return Some(if wet { (id - 4, true) } else { (id + 4, false) });
    }
    None
}

/// Light emission (Java: glow lichen 7 on any face, sculk catalyst 6,
/// sculk sensor 1).
pub const fn emission(id: u16) -> u8 {
    match multiface_of(Block(id)) {
        Some((Multiface::GlowLichen, ..)) => 7,
        Some(_) => 0,
        None if catalyst_bloom(Block(id)).is_some() => 6,
        None if sensor_of(Block(id)).is_some() => 1,
        None => 0,
    }
}

/// Shaped states drawn from [`boxes`] (`Shaped::Custom`).
#[inline]
pub fn is_custom(b: Block) -> bool {
    owns(b.0) && b.kind() == RenderKind::Shaped
}

/// Shaped states with see-through texels: drawn in the alpha-tested pass.
pub fn see_through(b: Block) -> bool {
    is_custom(b)
}

/// The block items, picks and rules use for a state: multiface blocks on
/// the floor, and the default (dry, inactive) sensor, shrieker and catalyst.
pub fn base(b: Block) -> Option<Block> {
    if let Some((kind, ..)) = multiface_of(b) {
        return Some(multiface(kind, DOWN, false));
    }
    if sensor_of(b).is_some() {
        return Some(SCULK_SENSOR);
    }
    if shrieker_of(b).is_some() {
        return Some(SCULK_SHRIEKER);
    }
    catalyst_bloom(b).map(|_| SCULK_CATALYST)
}

/// The number of faces a multiface state covers.
pub fn faces(b: Block) -> Option<u32> {
    multiface_of(b).map(|(_, f, _)| f.count_ones())
}

/// Whether `id` is a registered state of this module.
fn registered(id: u16) -> bool {
    owns(id) && registry(id).is_some()
}

/// `(hardness, best tool, pickaxe harvest level)` per Java's block
/// properties and `mineable/*` tags: glow lichen 0.2 (axe); sculk and its
/// vein 0.2, the sensor 1.5, the catalyst and shrieker 3.0 (hoe). None
/// needs the correct tool.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    if !registered(b.0) {
        return None;
    }
    Some(match multiface_of(b) {
        Some((Multiface::GlowLichen, ..)) => (0.2, Some(ToolKind::Axe), None),
        Some((Multiface::SculkVein, ..)) => (0.2, Some(ToolKind::Hoe), None),
        None if b == SCULK => (0.2, Some(ToolKind::Hoe), None),
        None if sensor_of(b).is_some() => (1.5, Some(ToolKind::Hoe), None),
        None => (3.0, Some(ToolKind::Hoe), None),
    })
}

/// What breaking the block drops without silk touch or shears: nothing.
/// Glow lichen needs shears ([`sheared`]); the sculk blocks need silk touch
/// ([`silk_drop`]) and otherwise give experience ([`xp`]).
pub fn drop(b: Block) -> Option<Option<Item>> {
    registered(b.0).then_some(None)
}

/// What silk touch keeps: every sculk block (one vein per face, see
/// [`drop_count`]); glow lichen only drops for shears.
pub fn silk_drop(b: Block) -> Option<Item> {
    if !registered(b.0) || matches!(multiface_of(b), Some((Multiface::GlowLichen, ..))) {
        return None;
    }
    Some(base(b).unwrap_or(b).into())
}

/// How many of [`silk_drop`]'s item one state gives: a vein's face count.
pub fn drop_count(b: Block) -> u8 {
    faces(b).map_or(1, |n| n as u8)
}

/// Shears keep glow lichen, one per face (`MultifaceBlock` loot with
/// `tool/can_shear`).
pub fn sheared(b: Block) -> Option<Stack> {
    match multiface_of(b) {
        Some((Multiface::GlowLichen, faces, _)) => Some(Stack::new(GLOW_LICHEN, faces.count_ones() as u8)),
        _ => None,
    }
}

/// Experience from breaking a sculk block without silk touch (Java's
/// `dropExperience`: sculk 1; catalyst, sensor and shrieker 5).
pub fn xp(b: Block) -> Option<u32> {
    let b = b.canonical();
    if b == SCULK {
        Some(1)
    } else if catalyst_bloom(b).is_some() || sensor_of(b).is_some() || shrieker_of(b).is_some() {
        Some(5)
    } else {
        None
    }
}

/// Glow lichen and sculk veins are replaceable (`BlockBehaviour.Properties.replaceable`).
pub fn replaceable(b: Block) -> bool {
    multiface_of(b).is_some()
}

/// Java `FireBlock` odds `(encouragement, flammability)`: glow lichen burns
/// like vines' lesser kin (15, 100).
pub fn fire_odds(b: Block) -> Option<(u8, u8)> {
    matches!(multiface_of(b), Some((Multiface::GlowLichen, ..))).then_some((15, 100))
}

/// Creative palette entries (the default state of each item).
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [GLOW_LICHEN.0, SCULK.0, SCULK_VEIN.0, SCULK_CATALYST.0, SCULK_SENSOR.0, SCULK_SHRIEKER.0].into_iter()
}

/// Whether the block in direction `DIRS[face]` of `at` holds a multiface
/// face (Java `MultifaceBlock.canAttachTo`: a full face toward it).
pub fn supports(neighbour: Block, face: usize) -> bool {
    super::mount_states::face_sturdy(neighbour, -DIRS[face])
}

/// The state a glow lichen or sculk vein item places at `at` (Java
/// `MultifaceBlock.getStateForPlacement`): the existing block of the same
/// kind there gains the first face, in the order the player looks, that is
/// free and has support. `None` if `raw` isn't a multiface item;
/// `Some(None)` if no face can take it. Water is added by the caller.
pub fn placed(raw: Block, at: IVec3, look: Vec3, get: impl Fn(IVec3) -> Option<Block>) -> Option<Option<Block>> {
    let (kind, ..) = multiface_of(raw.canonical())?;
    let existing = get(at).and_then(multiface_of).filter(|&(k, ..)| k == kind).map_or(0, |(_, faces, _)| faces);
    for d in super::mount_states::nearest_looking(look) {
        let Some(i) = DIRS.iter().position(|&x| x == d) else { continue };
        let bit = 1u8 << i;
        if existing & bit == 0 && get(at + DIRS[i]).is_some_and(|n| supports(n, i)) {
            return Some(Some(multiface(kind, existing | bit, false)));
        }
    }
    Some(None)
}

/// `state` with the faces whose support is gone removed, or `None` when no
/// face is left. `get` reads the neighbours of the state's cell.
pub fn supported_faces(state: Block, neighbour: impl Fn(usize) -> Option<Block>) -> Option<Option<Block>> {
    let (kind, faces, wet) = multiface_of(state)?;
    let mut kept = faces;
    for i in 0..6 {
        // An unloaded neighbour keeps its face.
        if faces & (1 << i) != 0 && neighbour(i).is_some_and(|n| !supports(n, i)) {
            kept &= !(1 << i);
        }
    }
    Some((kept != 0).then(|| multiface(kind, kept, wet)))
}

const fn bx(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}

/// One sixteenth thick plates against each face, in [`DIRS`] order.
const PLATES: [Box16; 6] = [
    bx([0, 0, 0], [16, 1, 16]),
    bx([0, 15, 0], [16, 16, 16]),
    bx([0, 0, 0], [16, 16, 1]),
    bx([0, 0, 15], [16, 16, 16]),
    bx([0, 0, 0], [1, 16, 16]),
    bx([15, 0, 0], [16, 16, 16]),
];

/// Java's sculk sensor and shrieker shape, `Block.column(16, 0, 8)`.
const SLAB: Box16 = bx([0, 0, 0], [16, 8, 16]);

/// Visible model boxes: multiface plates; the sensor's body with two
/// crossed tendril planes; the shrieker's body with its bony rim.
pub fn boxes(b: Block) -> Boxes {
    let mut out = Boxes::new();
    if let Some((_, faces, _)) = multiface_of(b) {
        for (i, &plate) in PLATES.iter().enumerate() {
            if faces & (1 << i) != 0 {
                out.push(plate);
            }
        }
    } else if sensor_of(b).is_some() {
        out.push(SLAB);
        out.push(bx([1, 8, 8], [15, 16, 8]));
        out.push(bx([8, 8, 1], [8, 16, 15]));
    } else if shrieker_of(b).is_some() {
        out.push(SLAB);
        out.push(bx([1, 8, 1], [15, 15, 15]));
    }
    out
}

/// Selection outlines (`getShape`): the plates, or the half-block body.
pub fn outline(b: Block) -> Boxes {
    if multiface_of(b).is_some() {
        return boxes(b);
    }
    Boxes::from_box(SLAB)
}

/// Collision: none for multiface blocks, the half block otherwise.
pub fn collision(b: Block) -> Boxes {
    if multiface_of(b).is_some() { Boxes::new() } else { Boxes::from_box(SLAB) }
}

/// A random face to spread glow lichen to with bone meal (Java
/// `MultifaceSpreader`'s same-position, same-plane and wrap-around spreads):
/// the cell and the state to put there, trying faces and directions in the
/// order `roll` shuffles them.
pub fn spread(
    at: IVec3,
    state: Block,
    mut roll: impl FnMut() -> u64,
    get: impl Fn(IVec3) -> Option<Block>,
) -> Option<(IVec3, Block)> {
    let (kind, faces, _) = multiface_of(state)?;
    let mut from: Vec<usize> = (0..6).filter(|i| faces & (1 << i) != 0).collect();
    shuffle(&mut from, &mut roll);
    let mut toward = [0usize, 1, 2, 3, 4, 5];
    shuffle(&mut toward, &mut roll);
    // A cell the lichen can take a face in: air, a water source, or the
    // same block without that face.
    let takes = |cell: IVec3, face: usize| -> Option<Block> {
        let here = get(cell)?;
        let have = match multiface_of(here) {
            Some((k, f, _)) if k == kind => f,
            _ if here == Block::AIR || here == Block::WATER => 0,
            _ => return None,
        };
        if have & (1 << face) != 0 || !get(cell + DIRS[face]).is_some_and(|n| supports(n, face)) {
            return None;
        }
        Some(multiface(kind, have | 1 << face, here == Block::WATER || here.is_waterlogged()))
    };
    for &f in &from {
        for &d in &toward {
            if d / 2 == f / 2 {
                continue; // parallel to the face
            }
            let candidates = [(at, d), (at + DIRS[d], f), (at + DIRS[d] + DIRS[f], opposite(d))];
            for (cell, face) in candidates {
                if let Some(b) = takes(cell, face) {
                    return Some((cell, b));
                }
            }
        }
    }
    None
}

/// The face index opposite `i` in [`DIRS`].
pub const fn opposite(i: usize) -> usize {
    i ^ 1
}

fn shuffle<T>(items: &mut [T], roll: &mut impl FnMut() -> u64) {
    for i in (1..items.len()).rev() {
        let j = (roll() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

impl super::World {
    /// Neighbour update: multiface blocks around `p` lose the faces that
    /// lay on it once it no longer gives support; one with no face left
    /// goes (leaving its water). They drop nothing (Java `updateShape`).
    pub(super) fn update_multiface(&mut self, p: IVec3) {
        for (i, d) in DIRS.iter().enumerate() {
            let q = p - *d;
            let Some(state) = self.get_block(q) else { continue };
            let Some((_, faces, _)) = multiface_of(state) else { continue };
            if faces & (1 << i) == 0 {
                continue;
            }
            if let Some(next) = supported_faces(state, |j| self.get_block(q + DIRS[j])) {
                let next = next.unwrap_or(Block::AIR);
                if next != state {
                    self.edit(q, next, true);
                }
            }
        }
    }

    /// Bone meal on glow lichen spreads it one face (`GlowLichenBlock`);
    /// `None` for other blocks.
    pub(super) fn cave_bone_meal(&mut self, p: IVec3, b: Block) -> Option<bool> {
        if !matches!(multiface_of(b), Some((Multiface::GlowLichen, ..))) {
            return None;
        }
        let mut rolls = Vec::with_capacity(12);
        for _ in 0..12 {
            rolls.push(self.roll());
        }
        let mut next = rolls.into_iter().cycle();
        let found = spread(p, b, || next.next().unwrap_or(0), |q| self.get_block(q));
        Some(match found {
            Some((cell, state)) => {
                self.edit(cell, state, true);
                true
            }
            None => false,
        })
    }
}
