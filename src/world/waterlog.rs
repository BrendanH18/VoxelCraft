//! Java 26.3 `SimpleWaterloggedBlock`: states with a `waterlogged` property
//! come in dry/wet pairs, each its own state id. Blocks that always hold
//! water (kelp and seagrass, Java `LiquidBlockContainer`s without the
//! property) are not pairs; see [`super::overworld_blocks::waterlogged`].
//!
//! Each v0.9 family module reports its own pairs; this module owns the
//! remaining waterloggable families (BLD-04f, ids 8192–8703), the
//! precomputed lookups and the generic rules every pair follows:
//! [`carried`] (what a cell's water does when its block changes) and
//! [`flow_passes`] (Java `FlowingFluid.canPassThroughWall`).

use glam::IVec3;

use super::block::{Block, Facing, STATE_CAPACITY};

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet one.
pub const fn pair(id: u16) -> Option<(u16, bool)> {
    if let Some(p) = super::slab_stair_states::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = super::mount_states::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = super::wall_states::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = super::scaffolding::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = super::copper::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = super::wood::water_pair(id) {
        return Some(p);
    }
    family_pair(id)
}

/// First id of this module's partner states.
pub const FIRST: u16 = 8192;
/// End of the BLD-04f reservation (exclusive).
const RESERVED_END: u16 = 8704;

/// Pre-existing states that gained a partner, as inclusive id runs, and
/// whether the new partner is the wet state. Partners take consecutive ids
/// from [`FIRST`] in this order; append runs only at the end.
///
/// Sea pickles and live coral were registered holding water, so their new
/// partners are the dry states; every other run was registered dry.
const RUNS: [(u16, u16, bool); 61] = [
    // Fences (`FenceBlock`): oak, nether brick, dark oak, spruce, birch,
    // jungle, acacia, mangrove, cherry, crimson, warped, pale oak, bamboo.
    (136, 136, true),
    (182, 182, true),
    (344, 344, true),
    (367, 367, true),
    (390, 390, true),
    (413, 413, true),
    (436, 436, true),
    (459, 459, true),
    (482, 482, true),
    (1693, 1693, true),
    (1716, 1716, true),
    (2205, 2205, true),
    (2228, 2228, true),
    // Iron bars and glass panes (`IronBarsBlock`, `StainedGlassPaneBlock`).
    (197, 197, true),
    (640, 656, true),
    // Walls (`WallBlock`).
    (258, 258, true),
    (264, 264, true),
    (270, 270, true),
    (276, 276, true),
    (282, 282, true),
    (288, 288, true),
    (294, 294, true),
    (300, 300, true),
    (306, 306, true),
    (312, 312, true),
    (318, 318, true),
    (324, 324, true),
    (330, 332, true),
    (822, 822, true),
    (828, 828, true),
    (834, 834, true),
    (2365, 2365, true),
    (2371, 2371, true),
    // Chains (`ChainBlock`), each axis.
    (814, 816, true),
    // Trapdoors (`TrapDoorBlock`): iron, oak, crimson, warped, pale oak, bamboo.
    (1265, 1280, true),
    (1437, 1452, true),
    (1734, 1749, true),
    (1764, 1779, true),
    (2269, 2284, true),
    (2299, 2314, true),
    // Rails (`BaseRailBlock`): rail; powered, detector and activator rails.
    (500, 509, true),
    (1471, 1506, true),
    // Ladders (`LadderBlock`) and chests (`ChestBlock`).
    (137, 140, true),
    (53, 56, true),
    // Leaves (`LeavesBlock`): oak, spruce, birch..acacia, azalea and
    // flowering azalea, dark oak..pale oak.
    (7, 7, true),
    (23, 23, true),
    (73, 75, true),
    (1942, 1943, true),
    (2010, 2013, true),
    // Amethyst buds and clusters (`AmethystClusterBlock`), up/down and wall.
    (1926, 1933, true),
    (2400, 2415, true),
    // Pointed dripstone (`PointedDripstoneBlock`).
    (1914, 1923, true),
    // Hanging roots, big dripleaf, its stem, small dripleaf halves.
    (1949, 1953, true),
    // Mangrove propagule (`MangrovePropaguleBlock`), standing and hanging.
    (2015, 2015, true),
    (2160, 2164, true),
    // Mangrove roots (`MangroveRootsBlock`).
    (2020, 2020, true),
    // Dead coral plants and fans (`BaseCoralPlantTypeBlock`).
    (1995, 1999, true),
    (2005, 2009, true),
    // Sea pickles (`SeaPickleBlock`) and live coral plants and fans: dry partners.
    (1965, 1968, false),
    (1990, 1994, false),
    (2000, 2004, false),
];

const ALL_RUNS: usize = RUNS.len();
const LAST_ORIGINAL: u16 = 2415;
const _: () = {
    let mut i = 0;
    while i < ALL_RUNS {
        assert!(RUNS[i].1 <= LAST_ORIGINAL);
        i += 1;
    }
};

/// Offset of each run's first partner from [`FIRST`].
const STARTS: [u16; ALL_RUNS] = {
    let mut out = [0; ALL_RUNS];
    let mut i = 1;
    while i < ALL_RUNS {
        let (a, b, _) = RUNS[i - 1];
        out[i] = out[i - 1] + (b - a + 1);
        i += 1;
    }
    out
};

/// How many partner states this module registers.
pub const COUNT: u16 = {
    let (a, b, _) = RUNS[ALL_RUNS - 1];
    STARTS[ALL_RUNS - 1] + (b - a + 1)
};
const _: () = assert!(FIRST + COUNT <= RESERVED_END, "BLD-04f partners overflow 8192–8703");

/// Pairs of the waterloggable families this module owns (fences, panes,
/// bars, walls, chains, trapdoors, rails, ladders, chests, leaves, amethyst,
/// dripstone, dripleaves, roots, propagules, sea pickles and coral).
pub const fn family_pair(id: u16) -> Option<(u16, bool)> {
    // Every original is below 2416; skip the scan for the other ids.
    if id > LAST_ORIGINAL && !(id >= FIRST && id < FIRST + COUNT) {
        return None;
    }
    let mut i = 0;
    while i < ALL_RUNS {
        let (a, b, new_wet) = RUNS[i];
        let start = FIRST + STARTS[i];
        let len = b - a + 1;
        if id >= a && id <= b {
            return Some((start + (id - a), !new_wet));
        }
        if id >= start && id < start + len {
            return Some((a + (id - start), new_wet));
        }
        i += 1;
    }
    None
}

/// The registered state whose properties a partner from this module shares
/// (`id` itself for every other state). Sea pickle and live coral partners
/// map to the wet originals, every other partner to its dry original.
#[inline]
pub const fn canonical(id: u16) -> u16 {
    if id >= super::wall_states::FIRST && id < super::wall_states::END {
        return id - (id - super::wall_states::FIRST) % 2;
    }
    if let Some(dry) = super::stone_blocks::wall_canonical(id) {
        return dry;
    }
    if super::wood::owns(id) {
        return super::wood::canonical(id);
    }
    if id >= FIRST && id < FIRST + COUNT { CANONICAL[(id - FIRST) as usize] } else { id }
}

static CANONICAL: [u16; COUNT as usize] = {
    let mut out = [0; COUNT as usize];
    let mut i = 0;
    while i < COUNT as usize {
        out[i] = match family_pair(FIRST + i as u16) {
            Some((other, _)) => other,
            None => 0,
        };
        i += 1;
    }
    out
};

/// The other state of each pair (0 where there is none) with the wet flag
/// in the top bit: one load for [`partner`] in edits and fluid ticks.
static PAIRS: [u16; STATE_CAPACITY] = {
    let mut arr = [0; STATE_CAPACITY];
    let mut i = 0;
    while i < STATE_CAPACITY {
        if let Some((other, is_wet)) = pair(i as u16) {
            arr[i] = other | (is_wet as u16) << 15;
        }
        i += 1;
    }
    arr
};

/// [`pair`] from the precomputed table.
#[inline(always)]
pub fn partner(id: u16) -> Option<(u16, bool)> {
    let v = PAIRS.get(id as usize).copied().unwrap_or(0);
    (v != 0).then_some((v & 0x7FFF, v & 0x8000 != 0))
}

/// `id` is the wet state of a waterlogged pair.
#[inline(always)]
pub fn wet(id: u16) -> bool {
    PAIRS.get(id as usize).is_some_and(|&v| v & 0x8000 != 0)
}

/// Holds a still water source: water, a wet pair state, kelp or seagrass
/// (Java `FluidState.is(Fluids.WATER)`, which excludes flowing water).
#[inline]
pub fn holds_source(b: Block) -> bool {
    b == Block::WATER || b.is_waterlogged()
}

/// Java `LiquidBlockContainer`: a block a water bucket fills in place
/// (`BucketItem.use` targets the clicked cell). Kelp and seagrass accept
/// nothing (`canPlaceLiquid` is false) but still take the bucket's water.
pub fn liquid_container(b: Block) -> bool {
    partner(b.0).is_some() || super::overworld_blocks::waterlogged(b.0)
}

/// What a cell holds after its block changes from `old` to `new`, following
/// Java's fluid states:
/// - a waterloggable state written over a water source is waterlogged, as
///   every `getStateForPlacement`/`updateShape` that reads the fluid at its
///   position makes it (`setValue` keeps the property on state changes);
/// - removing a block that holds water leaves the source
///   (`Level.removeBlock`/`destroyBlock` set `fluidState.createLegacyBlock()`).
///
/// Callers that must drop the water (`pickupBlock`, sponges, pistons) use
/// `World::edit_exact` instead.
#[inline]
pub fn carried(old: Block, new: Block) -> Block {
    if new == Block::AIR {
        return if old.is_waterlogged() { Block::WATER } else { new };
    }
    match partner(new.0) {
        Some((wet, false)) if holds_source(old) => Block(wet),
        _ => new,
    }
}

/// Java `FlowingFluid.canPassThroughWall`: water moves between the cells
/// `from` and `from + dir` unless either collision shape is a full block or
/// the two shapes together cover the face between them
/// (`Shapes.mergedFaceOccludes`). Only waterloggable states change the
/// answer: fluid sources and flowing fluid have empty shapes, and other
/// blocks either take the fluid (plants) or stop it (everything else).
pub fn flow_passes(get: impl Fn(IVec3) -> Option<Block>, from: IVec3, dir: IVec3) -> bool {
    let (Some(a), Some(b)) = (get(from), get(from + dir)) else { return true };
    if partner(a.0).is_none() && partner(b.0).is_none() {
        return true;
    }
    let Some(face) = Facing6::of(dir) else { return true };
    let cover = |cell: IVec3, block: Block, face: Facing6| -> [u16; 16] {
        let neighbour = |f: Facing| get(cell + f.offset()).unwrap_or(Block::AIR);
        let below = get(cell - IVec3::Y).unwrap_or(Block::AIR);
        let boxes = super::shape::collision_or_cube(block, neighbour, below, super::shape::Above::NONE);
        face_cover(&boxes, face)
    };
    let (ca, cb) = (cover(from, a, face), cover(from + dir, b, face.opposite()));
    if ca == [u16::MAX; 16] || cb == [u16::MAX; 16] {
        return false;
    }
    !ca.iter().zip(cb).all(|(x, y)| x | y == u16::MAX)
}

impl super::World {
    /// `SimpleWaterloggedBlock.placeLiquid` with a water source (a water
    /// bucket or dispenser): a dry waterloggable block at `p` becomes wet and
    /// its water starts to flow. Returns whether it changed; a block that
    /// already holds water, kelp and seagrass stay as they are.
    pub fn fill_waterlogged(&mut self, p: IVec3) -> bool {
        match self.get_block(p) {
            Some(b) if b.waterloggable() && !b.is_waterlogged() => self.set_block(p, b.with_water(true)),
            _ => false,
        }
    }

    /// `SimpleWaterloggedBlock.pickupBlock` (a bucket, dispenser or sponge):
    /// a waterlogged block at `p` loses its water, then breaks with its
    /// drops if it can't stay without it (`canSurvive`). Returns whether
    /// there was water to take.
    pub fn drain_waterlogged(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p).filter(|b| wet(b.0)) else { return false };
        let dry = b.with_water(false);
        self.edit_exact(p, dry, true);
        let below = self.get_block(p - IVec3::Y).unwrap_or(Block::AIR);
        if !dry.can_stay_on(below) {
            self.spill_block(p, dry);
            self.edit_exact(p, Block::AIR, true);
        }
        self.wake_fluids(p);
        self.update_block_light();
        true
    }
}

/// One of the six directions, for face projections.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Facing6 {
    Up,
    Down,
    Side { axis: usize, positive: bool },
}

impl Facing6 {
    fn of(d: IVec3) -> Option<Self> {
        Some(match (d.x, d.y, d.z) {
            (0, 1, 0) => Facing6::Up,
            (0, -1, 0) => Facing6::Down,
            (x, 0, 0) if x.abs() == 1 => Facing6::Side { axis: 0, positive: x > 0 },
            (0, 0, z) if z.abs() == 1 => Facing6::Side { axis: 2, positive: z > 0 },
            _ => return None,
        })
    }

    fn opposite(self) -> Self {
        match self {
            Facing6::Up => Facing6::Down,
            Facing6::Down => Facing6::Up,
            Facing6::Side { axis, positive } => Facing6::Side { axis, positive: !positive },
        }
    }

    /// Axis index (0 x, 1 y, 2 z) and whether the face is at the high end.
    fn axis(self) -> (usize, bool) {
        match self {
            Facing6::Up => (1, true),
            Facing6::Down => (1, false),
            Facing6::Side { axis, positive } => (axis, positive),
        }
    }
}

/// The part of a cell's face covered by boxes touching it (Java
/// `VoxelShape.getFaceShape`), as 16 rows of 16 bits; boxes reaching past
/// the cell (fence collision is 24 high) are clipped to it.
fn face_cover(boxes: &super::shape::Boxes, face: Facing6) -> [u16; 16] {
    let (d, positive) = face.axis();
    let (u, v) = ((d + 1) % 3, (d + 2) % 3);
    let mut rows = [0u16; 16];
    for bx in boxes.as_slice() {
        let touches = if positive { bx.max[d] >= 16 } else { bx.min[d] == 0 };
        if !touches {
            continue;
        }
        let (u0, u1) = (bx.min[u].min(16), bx.max[u].min(16));
        let (v0, v1) = (bx.min[v].min(16), bx.max[v].min(16));
        if u0 >= u1 {
            continue;
        }
        let bits = (((1u32 << (u1 - u0)) - 1) << u0) as u16;
        for row in &mut rows[v0 as usize..v1 as usize] {
            *row |= bits;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    #[test]
    fn pairs_are_symmetric_registered_and_not_always_wet() {
        for id in 0..STATE_CAPACITY as u16 {
            let Some((other, is_wet)) = pair(id) else { continue };
            assert_eq!(pair(other), Some((id, !is_wet)), "{id} and {other} must pair both ways");
            assert_ne!(Block(id).name(), "unknown", "{id} is paired but unregistered");
            assert!(!crate::world::overworld_blocks::waterlogged(id), "{id} is always wet; it can't be a pair");
            assert_eq!(wet(id), is_wet);
            assert_eq!(partner(id), Some((other, is_wet)));
            assert_eq!(Block(id).with_water(is_wet), Block(id));
            assert_eq!(Block(id).with_water(!is_wet), Block(other));
        }
    }

    #[test]
    fn layout_fills_the_reservation_from_its_start_without_gaps() {
        assert_eq!(COUNT, 285);
        for id in FIRST..FIRST + COUNT {
            let (original, _) = family_pair(id).expect("every partner id is used");
            assert!(original < 2525, "{id} pairs with the pre-existing state {original}");
            assert_eq!(canonical(id), original);
        }
        for id in FIRST + COUNT..RESERVED_END {
            assert!(family_pair(id).is_none() && Block(id).name() == "unknown", "{id} stays free");
        }
        // Spot checks against the table in docs/architecture.md.
        assert_eq!(family_pair(136), Some((8192, false)), "oak fence");
        assert_eq!(family_pair(1965), Some((COUNT + FIRST - 14, true)), "sea pickles were registered wet");
        assert_eq!(family_pair(2004), Some((FIRST + COUNT - 1, true)), "the last live coral fan");
    }
}

#[cfg(test)]
mod world_tests;
