//! v0.16 Track D generation 5 Overworld blocks (BLD-01b, GEN-03, GEN-04a):
//! Java 26.3's cinnabar and sulfur building families, potent sulfur and
//! sulfur spikes, the red shrub, golden dandelion, shelf mushroom and nether
//! gold ore. Ids 16000..=16383 (Track D's reservation in
//! `docs/architecture.md`); the six stone stairs families sit in the wood
//! range's Track D part (`wood::poplar::STONE_STAIRS`, 80 ids each).
//!
//! | Ids | States |
//! | --- | --- |
//! | 16000..=16007 | cinnabar, polished cinnabar, cinnabar bricks, chiseled cinnabar, then the same four of sulfur |
//! | 16008..=16012 | potent sulfur: dry, wet, dormant, erupting, continuous (Java's `PotentSulfurState` order) |
//! | 16016..=16035 | sulfur spike: `16016 + (down * 5 + thickness) * 2 + wet` |
//! | 16040..=16069 | slabs, 5 per family in [`SHAPES`] order from 1 (bottom, bottom wet, top, top wet, double) |
//! | 16070..=16093 | walls, 4 per family (UP, UP wet, no UP, no UP wet) |
//! | 16096 | nether gold ore |
//! | 16097 | red shrub |
//! | 16098 | golden dandelion |
//! | 16099 | potted golden dandelion |
//! | 16100..=16107 | shelf mushroom: `16100 + age * 4 + facing` (facing in Java's north, south, west, east order) |
//!
//! Every other id in the range stays unregistered.

use super::block::{Block, Facing, RenderKind};
use super::slab_stair_states::Family;
use super::wood::poplar;
use crate::item::{Item, ToolKind};

pub const FIRST: u16 = 16000;
pub const LAST: u16 = 16383;

pub const CINNABAR: Block = Block(16000);
pub const POLISHED_CINNABAR: Block = Block(16001);
pub const CINNABAR_BRICKS: Block = Block(16002);
pub const CHISELED_CINNABAR: Block = Block(16003);
pub const SULFUR: Block = Block(16004);
pub const POLISHED_SULFUR: Block = Block(16005);
pub const SULFUR_BRICKS: Block = Block(16006);
pub const CHISELED_SULFUR: Block = Block(16007);
const FULL_NAMES: [&str; 8] = [
    "cinnabar",
    "polished cinnabar",
    "cinnabar bricks",
    "chiseled cinnabar",
    "sulfur",
    "polished sulfur",
    "sulfur bricks",
    "chiseled sulfur",
];

/// Java's `PotentSulfurState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Potent {
    Dry,
    Wet,
    Dormant,
    Erupting,
    Continuous,
}

impl Potent {
    pub const ALL: [Potent; 5] = [Potent::Dry, Potent::Wet, Potent::Dormant, Potent::Erupting, Potent::Continuous];
}

pub const POTENT_SULFUR: Block = Block(16008);
pub const fn potent(state: Potent) -> Block {
    Block(POTENT_SULFUR.0 + state as u16)
}
pub const fn potent_state(b: Block) -> Option<Potent> {
    if b.0 >= POTENT_SULFUR.0 && b.0 < POTENT_SULFUR.0 + 5 {
        Some(Potent::ALL[(b.0 - POTENT_SULFUR.0) as usize])
    } else {
        None
    }
}

/// Sulfur spikes: Java's `SpeleothemBlock` (`thickness`,
/// `vertical_direction`, `waterlogged`), thickness in
/// `overworld_blocks::Thickness` order.
pub const SPIKE_FIRST: u16 = 16016;
pub const fn spike(down: bool, thickness: super::overworld_blocks::Thickness, wet: bool) -> Block {
    Block(SPIKE_FIRST + (down as u16 * 5 + thickness as u16) * 2 + wet as u16)
}
pub const SULFUR_SPIKE: Block = spike(false, super::overworld_blocks::Thickness::Tip, false);
/// Whether a spike points down, its thickness and whether it holds water.
pub const fn spike_of(b: Block) -> Option<(bool, super::overworld_blocks::Thickness, bool)> {
    use super::overworld_blocks::Thickness;
    if b.0 < SPIKE_FIRST || b.0 >= SPIKE_FIRST + 20 {
        return None;
    }
    let i = b.0 - SPIKE_FIRST;
    let t = match i / 2 % 5 {
        0 => Thickness::Tip,
        1 => Thickness::TipMerge,
        2 => Thickness::Frustum,
        3 => Thickness::Middle,
        _ => Thickness::Base,
    };
    Some((i >= 10, t, i % 2 == 1))
}

// ---- Stairs, slabs and walls ---------------------------------------------

const SLABS_FIRST: u16 = 16040;
const WALLS_FIRST: u16 = 16070;
/// The six stone families with stairs, slabs and walls.
pub const STONE_BASES: [Block; 6] =
    [CINNABAR, POLISHED_CINNABAR, CINNABAR_BRICKS, SULFUR, POLISHED_SULFUR, SULFUR_BRICKS];
const SHAPE_NAMES: [(&str, &str, &str); 6] = [
    ("cinnabar stairs", "cinnabar slab", "cinnabar wall"),
    ("polished cinnabar stairs", "polished cinnabar slab", "polished cinnabar wall"),
    ("cinnabar brick stairs", "cinnabar brick slab", "cinnabar brick wall"),
    ("sulfur stairs", "sulfur slab", "sulfur wall"),
    ("polished sulfur stairs", "polished sulfur slab", "polished sulfur wall"),
    ("sulfur brick stairs", "sulfur brick slab", "sulfur brick wall"),
];

const fn stone_shape(i: u16) -> Family {
    Family {
        base: STONE_BASES[i as usize],
        stairs: Block(poplar::STONE_STAIRS + i * 80),
        slab: Block(SLABS_FIRST + i * 5),
    }
}

/// Track D's `slab_stair_states` families, appended after resin: poplar,
/// then the six stones in [`STONE_BASES`] order.
pub const SHAPES: [Family; 7] =
    [poplar::SHAPES, stone_shape(0), stone_shape(1), stone_shape(2), stone_shape(3), stone_shape(4), stone_shape(5)];

pub const fn stairs(stone: u16) -> Block {
    stone_shape(stone).stairs
}
pub const fn slab(stone: u16) -> Block {
    stone_shape(stone).slab
}

/// The UP, dry state of stone wall `w`: its item and default state.
pub const fn wall(w: u16) -> Block {
    Block(WALLS_FIRST + w * 4)
}

/// Wall index and local state (UP dry, UP wet, no-UP dry, no-UP wet).
pub const fn wall_local(id: u16) -> Option<(u16, u16)> {
    if id >= WALLS_FIRST && id < WALLS_FIRST + 24 {
        Some(((id - WALLS_FIRST) / 4, (id - WALLS_FIRST) % 4))
    } else {
        None
    }
}

pub const fn wall_water_pair(id: u16) -> Option<(u16, bool)> {
    match wall_local(id) {
        Some((_, local)) => Some((id ^ 1, local % 2 == 1)),
        None => None,
    }
}

pub const fn wall_base(id: u16) -> Option<Block> {
    match wall_local(id) {
        Some((w, _)) => Some(STONE_BASES[w as usize]),
        None => None,
    }
}

const fn stone_slab_local(id: u16) -> Option<(u16, u16)> {
    if id >= SLABS_FIRST && id < SLABS_FIRST + 30 {
        Some(((id - SLABS_FIRST) / 5, (id - SLABS_FIRST) % 5))
    } else {
        None
    }
}

/// The stairs and slab states of Track D's families (poplar's included).
pub const fn owns_shape(id: u16) -> bool {
    (id >= poplar::STAIRS.0 && id < poplar::STAIRS.0 + 80)
        || (id >= poplar::SLAB.0 && id < poplar::SLAB.0 + 5)
        || poplar::is_stone_stairs(id)
        || stone_slab_local(id).is_some()
}

/// Name, render kind and textures of the stone stairs in the wood range.
pub const fn stairs_registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    if !poplar::is_stone_stairs(id) {
        return None;
    }
    let f = (id - poplar::STONE_STAIRS) / 80;
    Some((SHAPE_NAMES[f as usize].0, RenderKind::Shaped, full_tex(STONE_BASES[f as usize].0 - FIRST)))
}

// ---- Plants and ore ------------------------------------------------------

pub const NETHER_GOLD_ORE: Block = Block(16096);
pub const RED_SHRUB: Block = Block(16097);
pub const GOLDEN_DANDELION: Block = Block(16098);
pub const POTTED_GOLDEN_DANDELION: Block = Block(16099);
const SHELF_FIRST: u16 = 16100;
/// Java's shelf mushroom default state: facing north, age 0.
pub const SHELF_MUSHROOM: Block = Block(SHELF_FIRST);

/// A shelf mushroom facing `facing` (away from its support) at `age` 0 or 1.
pub const fn shelf_mushroom(facing: Facing, age: u16) -> Block {
    Block(SHELF_FIRST + age * 4 + super::wood::facing_index(facing))
}

/// Facing and age of a shelf mushroom.
pub const fn shelf_of(b: Block) -> Option<(Facing, u16)> {
    if b.0 >= SHELF_FIRST && b.0 < SHELF_FIRST + 8 {
        let i = b.0 - SHELF_FIRST;
        Some((super::wood::FACINGS[(i % 4) as usize], i / 4))
    } else {
        None
    }
}

/// The potted form of a Track D plant (poplar's sapling included).
pub const fn potted(plant: Block) -> Option<Block> {
    if plant.0 == GOLDEN_DANDELION.0 {
        Some(POTTED_GOLDEN_DANDELION)
    } else if plant.0 == poplar::SAPLING.0 {
        Some(poplar::POTTED_SAPLING)
    } else {
        None
    }
}

/// The plant in one of Track D's pots.
pub const fn potted_plant(b: Block) -> Option<Block> {
    if b.0 == POTTED_GOLDEN_DANDELION.0 {
        Some(GOLDEN_DANDELION)
    } else if b.0 == poplar::POTTED_SAPLING.0 {
        Some(poplar::SAPLING)
    } else {
        None
    }
}

// ---- Registry ---------------------------------------------------------------

/// Texture layers (Track D owns 749..=948; poplar takes the first).
pub mod t {
    pub const BASE: u16 = super::poplar::t::END;
    /// The eight full blocks, in id order; the chiseled ones have tops.
    pub const FULL: u16 = BASE;
    pub const CHISELED_CINNABAR_TOP: u16 = BASE + 8;
    pub const CHISELED_SULFUR_TOP: u16 = BASE + 9;
    /// Potent sulfur: dry, wet (also dormant), active.
    pub const POTENT: u16 = BASE + 10;
    /// Sulfur spike thickness, tip..base, pointing up (down is mirrored).
    pub const SPIKE: u16 = BASE + 13;
    pub const NETHER_GOLD_ORE: u16 = BASE + 18;
    pub const RED_SHRUB: u16 = BASE + 19;
    pub const GOLDEN_DANDELION: u16 = BASE + 20;
    pub const POTTED_GOLDEN_DANDELION: u16 = BASE + 21;
    pub const SHELF_MUSHROOM: u16 = BASE + 22;
    pub const SHELF_MUSHROOM_TOP: u16 = BASE + 23;
    pub const END: u16 = BASE + 24;
    const _: () = assert!(END <= 949, "Track D owns layers 749..=948");
}

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

/// Faces of full block `i` (0..8).
const fn full_tex(i: u16) -> [u16; 6] {
    match i {
        3 => [t::FULL + 3, t::FULL + 3, t::CHISELED_CINNABAR_TOP, t::CHISELED_CINNABAR_TOP, t::FULL + 3, t::FULL + 3],
        7 => [t::FULL + 7, t::FULL + 7, t::CHISELED_SULFUR_TOP, t::CHISELED_SULFUR_TOP, t::FULL + 7, t::FULL + 7],
        _ => all(t::FULL + i),
    }
}

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// Name, render kind and textures of this module's states in 16000..=16383.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    if id >= FIRST && id < FIRST + 8 {
        return Some((FULL_NAMES[(id - FIRST) as usize], Opaque, full_tex(id - FIRST)));
    }
    if let Some(s) = potent_state(Block(id)) {
        let layer = match s {
            Potent::Dry => t::POTENT,
            Potent::Wet | Potent::Dormant => t::POTENT + 1,
            Potent::Erupting | Potent::Continuous => t::POTENT + 2,
        };
        return Some(("potent sulfur", Opaque, all(layer)));
    }
    if let Some((_, thickness, _)) = spike_of(Block(id)) {
        return Some(("sulfur spike", Cross, all(t::SPIKE + thickness as u16)));
    }
    if let Some((f, local)) = stone_slab_local(id) {
        let kind = match local {
            0 | 1 => Cutout,
            2 | 3 => Shaped,
            _ => Opaque,
        };
        return Some((SHAPE_NAMES[f as usize].1, kind, full_tex(STONE_BASES[f as usize].0 - FIRST)));
    }
    if let Some((w, _)) = wall_local(id) {
        return Some((SHAPE_NAMES[w as usize].2, Shaped, full_tex(STONE_BASES[w as usize].0 - FIRST)));
    }
    if let Some((_, _)) = shelf_of(Block(id)) {
        return Some((
            "shelf mushroom",
            Shaped,
            [
                t::SHELF_MUSHROOM,
                t::SHELF_MUSHROOM,
                t::SHELF_MUSHROOM_TOP,
                t::SHELF_MUSHROOM,
                t::SHELF_MUSHROOM,
                t::SHELF_MUSHROOM,
            ],
        ));
    }
    Some(match id {
        16096 => ("nether gold ore", Opaque, all(t::NETHER_GOLD_ORE)),
        16097 => ("red shrub", Cross, all(t::RED_SHRUB)),
        16098 => ("golden dandelion", Cross, all(t::GOLDEN_DANDELION)),
        16099 => {
            use super::utilities::t as u;
            (
                "potted golden dandelion",
                Shaped,
                [
                    t::POTTED_GOLDEN_DANDELION,
                    t::POTTED_GOLDEN_DANDELION,
                    u::POT_SOIL,
                    u::POT_BOTTOM,
                    t::POTTED_GOLDEN_DANDELION,
                    t::POTTED_GOLDEN_DANDELION,
                ],
            )
        }
        _ => return None,
    })
}

/// Waterlogged pairs: slabs, walls and spikes.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if !owns(id) {
        return None;
    }
    if stone_slab_local(id).is_some() {
        return super::slab_stair_states::water_pair(id);
    }
    if let Some(p) = wall_water_pair(id) {
        return Some(p);
    }
    match spike_of(Block(id)) {
        Some((_, _, wet)) => Some((id ^ 1, wet)),
        None => None,
    }
}

/// The block items and rules use: the dry tip spike, the young shelf
/// mushroom facing north, dry potent sulfur.
pub const fn base(id: u16) -> Option<Block> {
    if spike_of(Block(id)).is_some() {
        return Some(SULFUR_SPIKE);
    }
    if shelf_of(Block(id)).is_some() {
        return Some(SHELF_MUSHROOM);
    }
    if potent_state(Block(id)).is_some() {
        return Some(POTENT_SULFUR);
    }
    match wall_local(id) {
        Some((w, _)) => Some(wall(w)),
        None => None,
    }
}

/// Hardness, preferred tool and pickaxe tier of the full blocks and
/// plants (Java 26.3 `Blocks`); stairs, slabs and walls ask with their
/// `Block::material`.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    let pick = Some(ToolKind::Pickaxe);
    if (FIRST..FIRST + 8).contains(&b.0) || potent_state(b).is_some() {
        return Some((1.5, pick, Some(0)));
    }
    if spike_of(b).is_some() {
        return Some((1.5, pick, None));
    }
    if b == NETHER_GOLD_ORE {
        return Some((3.0, pick, Some(0)));
    }
    if shelf_of(b).is_some() || matches!(b, RED_SHRUB | GOLDEN_DANDELION | POTTED_GOLDEN_DANDELION) {
        return Some((0.0, None, None));
    }
    None
}

/// Java's explosion resistance where it differs from the hardness.
pub fn resistance(b: Block) -> Option<f32> {
    let base = b.canonical();
    if (FIRST..FIRST + 8).contains(&base.0)
        || potent_state(base).is_some()
        || stone_slab_local(base.0).is_some()
        || wall_local(base.0).is_some()
        || poplar::is_stone_stairs(base.0)
    {
        return Some(6.0);
    }
    if spike_of(base).is_some() {
        return Some(3.0);
    }
    None
}

/// Whether an explosion decays the block's drops (`explosion_decay`).
pub fn explosion_decays(b: Block) -> bool {
    stone_slab_local(b.canonical().0).is_some() || b == NETHER_GOLD_ORE || shelf_of(b).is_some()
}

/// Survival drops: `Some(None)` drops nothing here (rolled elsewhere).
pub fn drop(b: Block) -> Option<Option<Item>> {
    if !owns(b.0) || stone_slab_local(b.0).is_some() {
        return None;
    }
    if b == NETHER_GOLD_ORE {
        // Two to six gold nuggets with Fortune (`World::spill_mined`).
        return Some(None);
    }
    if b == POTTED_GOLDEN_DANDELION {
        return Some(Some(super::utilities::FLOWER_POT.into()));
    }
    Some(Some(Item::from_block(match base(b.0) {
        Some(base) => base,
        None => b,
    })))
}

/// The dry state of a wet spike or wall (`id` itself otherwise).
pub const fn canonical(id: u16) -> u16 {
    if wall_local(id).is_none() && spike_of(Block(id)).is_none() {
        return id;
    }
    match water_pair(id) {
        Some((dry, true)) => dry,
        _ => id,
    }
}

/// `ShelfMushroomBlock.SHAPES`: a cap and a stalk against the support,
/// larger when grown (`age` 1); outline and collision alike.
pub fn shelf_boxes(b: Block) -> Option<super::shape::Boxes> {
    use super::shape::{Box16, Boxes};
    let (facing, age) = shelf_of(b)?;
    // Drawn facing south, the support to the north (z 0).
    let parts: [Box16; 2] = if age == 0 {
        [Box16 { min: [3, 9, 0], max: [13, 11, 7] }, Box16 { min: [5, 8, 0], max: [11, 9, 4] }]
    } else {
        [Box16 { min: [1, 8, 0], max: [15, 11, 10] }, Box16 { min: [4, 6, 0], max: [12, 8, 6] }]
    };
    let mut out = Boxes::new();
    for p in parts {
        out.push(p.turned(facing));
    }
    Some(out)
}

/// How many of its item a block drops: two from a grown shelf mushroom
/// (`blocks/shelf_mushroom`, `age=1`).
pub fn drop_count(b: Block) -> u8 {
    match shelf_of(b) {
        Some((_, 1)) => 2,
        _ => 1,
    }
}

/// Fire odds (`FireBlock.bootStrap`) and lava ignition of the plants.
pub fn fire_odds(b: Block) -> Option<(u8, u8)> {
    match b {
        RED_SHRUB => Some((60, 100)),
        GOLDEN_DANDELION => Some((60, 100)),
        _ => None,
    }
}

/// Default states for the creative palette.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (0..8u16).map(|i| FIRST + i).chain((0..6u16).flat_map(|f| [stairs(f).0, slab(f).0, wall(f).0])).chain([
        POTENT_SULFUR.0,
        SULFUR_SPIKE.0,
        NETHER_GOLD_ORE.0,
        RED_SHRUB.0,
        GOLDEN_DANDELION.0,
        SHELF_MUSHROOM.0,
    ])
}

/// Survival rules of the plants: the shrub and dandelion need Java's
/// `#supports_vegetation` soil; spikes and shelf mushrooms are checked by
/// their own support rules.
pub fn can_stay_on(b: Block, below: Block) -> Option<bool> {
    match b {
        RED_SHRUB | GOLDEN_DANDELION | poplar::SAPLING | poplar::SAPLING_STAGE_ONE => {
            Some(super::overworld_blocks::dirt_like(below))
        }
        _ => None,
    }
}

/// Plants that placing a block overwrites (`replaceable()`): the red shrub.
pub fn replaceable(b: Block) -> bool {
    b == RED_SHRUB
}

/// Whether `b` has a full sturdy face toward `dir` (what spikes stand on and
/// shelf mushrooms grow from).
fn sturdy(b: Block, dir: glam::IVec3) -> bool {
    super::mount_states::face_sturdy(b, dir)
}

/// The state a held spike or shelf mushroom places (`Some(None)`: nowhere
/// fits); `None` for other items.
///
/// `SpeleothemBlock.getStateForPlacement`: the spike points away from the
/// nearest vertical looking direction when it can stand there, otherwise
/// the other way. `ShelfMushroomBlock.getStateForPlacement`: the first
/// horizontal looking direction whose opposite side holds a sturdy face.
pub fn placed(
    raw: Block,
    ctx: &crate::session::placement::PlaceContext,
    get: impl Fn(glam::IVec3) -> Option<Block>,
) -> Option<Option<Block>> {
    use super::overworld_blocks::Thickness;
    use glam::IVec3;
    let at = ctx.at;
    let block = |p: IVec3| get(p).unwrap_or(Block::AIR);
    if raw == SULFUR_SPIKE {
        let first_down = ctx.look.y > 0.0;
        for down in [first_down, !first_down] {
            let support = if down { at + IVec3::Y } else { at - IVec3::Y };
            let s = block(support);
            let face = if down { IVec3::NEG_Y } else { IVec3::Y };
            if sturdy(s, face) || matches!(spike_of(s.canonical()), Some((d, _, _)) if d == down) {
                return Some(Some(spike(down, Thickness::Tip, false)));
            }
        }
        return Some(None);
    }
    if raw == SHELF_MUSHROOM {
        for d in super::mount_states::nearest_looking(ctx.look) {
            let Some(f) = Facing::from_offset(d) else { continue };
            // Facing away from the support, which lies along the look.
            let facing = f.opposite();
            if sturdy(block(at + d), -d) {
                return Some(Some(shelf_mushroom(facing, 0)));
            }
        }
        return Some(None);
    }
    None
}

/// A spike's support rule (`SpeleothemBlock.canSurvive`): an up spike needs
/// a sturdy face or an up spike under it.
pub fn spike_stands(b: Block, below: Block) -> Option<bool> {
    let (down, _, _) = spike_of(b.canonical())?;
    Some(down || sturdy(below, glam::IVec3::Y) || matches!(spike_of(below.canonical()), Some((false, _, _))))
}

/// A down spike's rule: a sturdy face or a down spike above it.
pub fn spike_hangs(b: Block, above: Block) -> Option<bool> {
    let (down, _, _) = spike_of(b.canonical())?;
    down.then(|| sturdy(above, glam::IVec3::NEG_Y) || matches!(spike_of(above.canonical()), Some((true, _, _))))
}

/// A shelf mushroom's rule: its support behind it keeps a sturdy face.
pub fn shelf_supported(b: Block, behind: Block) -> Option<bool> {
    let (facing, _) = shelf_of(b)?;
    Some(sturdy(behind, facing.offset()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fits_the_reservation() {
        const { assert!(SPIKE_FIRST >= POTENT_SULFUR.0 + 5) };
        const { assert!(SLABS_FIRST >= SPIKE_FIRST + 20) };
        assert_eq!(WALLS_FIRST, SLABS_FIRST + 30);
        const { assert!(NETHER_GOLD_ORE.0 >= WALLS_FIRST + 24) };
        const { assert!(SHELF_FIRST + 8 <= LAST) };
        for down in [false, true] {
            for t in [super::super::overworld_blocks::Thickness::Tip, super::super::overworld_blocks::Thickness::Base] {
                for wet in [false, true] {
                    assert_eq!(spike_of(spike(down, t, wet)), Some((down, t, wet)));
                }
            }
        }
        for f in Facing::ALL {
            for age in 0..2 {
                assert_eq!(shelf_of(shelf_mushroom(f, age)), Some((f, age)));
            }
        }
    }
}
