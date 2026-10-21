//! Geometry of shaped blocks (stairs, fences, gates, ladders, doors): a few
//! axis-aligned boxes in 1/16-block units, used alike by the mesher, physics,
//! the crosshair and inventory icons.
//!
//! Fences and gates look at their four horizontal neighbours, so their
//! shape is computed from a neighbour lookup rather than stored. Walls also
//! read the block above ([`Above`]) for their post and tall sides.
//!
//! A waterlogged state (BLD-04f partner) has its dry original's shape: each
//! entry point looks at [`Block::canonical`] states.

use super::block::{Block, Facing, Shaped};

/// A box in 1/16 block, relative to the cell's minimum corner. Collision
/// boxes of fences and gates reach up to 24 (1.5 blocks).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Box16 {
    pub min: [u8; 3],
    pub max: [u8; 3],
}

const fn b(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}

impl Box16 {
    pub const FULL: Box16 = b([0; 3], [16; 3]);

    /// The box turned a quarter `turns` times clockwise (seen from above)
    /// about the cell's vertical centre line. Boxes below are drawn for a
    /// block facing south (+Z); this turns them to face any way.
    fn turned(self, facing: Facing) -> Box16 {
        let turns = match facing {
            Facing::South => 0,
            Facing::West => 1,
            Facing::North => 2,
            Facing::East => 3,
        };
        let mut out = self;
        for _ in 0..turns {
            // (x, z) -> (16 - z, x): south (+Z) becomes west (-X).
            let (min, max) = (out.min, out.max);
            out.min = [16 - max[2], min[1], min[0]];
            out.max = [16 - min[2], max[1], max[0]];
        }
        out
    }

    /// Whether this box covers the rectangle `[u0, u1] x [v0, v1]` on the
    /// plane `axis = depth`, touching it from the side `toward` points.
    fn covers(&self, axis: usize, depth: u8, positive_side: bool, (u, v): (usize, usize), r: [u8; 4]) -> bool {
        let touches = if positive_side { self.min[axis] == depth } else { self.max[axis] == depth };
        touches && self.min[u] <= r[0] && self.max[u] >= r[1] && self.min[v] <= r[2] && self.max[v] >= r[3]
    }
}

/// Up to [`Boxes::CAPACITY`] boxes, stored inline.
#[derive(Clone, Copy, Debug)]
pub struct Boxes {
    boxes: [Box16; Boxes::CAPACITY],
    len: u8,
}

impl Boxes {
    pub const CAPACITY: usize = 10;

    const fn new() -> Self {
        Boxes { boxes: [Box16::FULL; Self::CAPACITY], len: 0 }
    }

    pub fn from_box(b: Box16) -> Self {
        let mut out = Self::new();
        out.push(b);
        out
    }

    fn push(&mut self, b: Box16) {
        self.boxes[self.len as usize] = b;
        self.len += 1;
    }

    fn push_turned(&mut self, boxes: &[Box16], facing: Facing) {
        for &b in boxes {
            self.push(b.turned(facing));
        }
    }

    pub fn as_slice(&self) -> &[Box16] {
        &self.boxes[..self.len as usize]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The smallest box around all of them (the selection outline).
    pub fn bounds(&self) -> Option<Box16> {
        let mut it = self.as_slice().iter();
        let first = *it.next()?;
        Some(it.fold(first, |acc, b| Box16 {
            min: std::array::from_fn(|i| acc.min[i].min(b.min[i])),
            max: std::array::from_fn(|i| acc.max[i].max(b.max[i])),
        }))
    }

    /// Whether the face of box `i` on `axis` (its max side if `positive`)
    /// over `[u0, u1] x [v0, v1]` is hidden by another box of the shape.
    pub fn face_hidden(&self, i: usize, axis: usize, positive: bool, r: [u8; 4]) -> bool {
        let b = self.boxes[i];
        let depth = if positive { b.max[axis] } else { b.min[axis] };
        let uv = ((axis + 1) % 3, (axis + 2) % 3);
        self.as_slice().iter().enumerate().any(|(j, o)| j != i && o.covers(axis, depth, positive, uv, r))
    }
}

/// A top slab: the upper half of the cell.
const TOP_SLAB: Box16 = b([0, 8, 0], [16, 16, 16]);
/// A ladder facing south hangs on the wall to its north.
const LADDER: [Box16; 1] = [b([0, 0, 0], [16, 16, 1])];
const LADDER_COLLISION: [Box16; 1] = [b([0, 0, 0], [16, 16, 3])];
/// A closed door facing south lies along the south side.
const DOOR: [Box16; 1] = [b([0, 0, 13], [16, 16, 16])];
/// A closed gate facing south runs along X across the middle.
const GATE_CLOSED: [Box16; 5] = [
    b([0, 5, 7], [2, 16, 9]),
    b([14, 5, 7], [16, 16, 9]),
    b([2, 6, 7], [14, 9, 9]),
    b([2, 12, 7], [14, 15, 9]),
    b([6, 9, 7], [10, 12, 9]),
];
/// Opened, its two halves fold back (north) against the posts.
const GATE_OPEN: [Box16; 8] = [
    b([0, 5, 7], [2, 16, 9]),
    b([14, 5, 7], [16, 16, 9]),
    b([0, 6, 1], [2, 9, 7]),
    b([0, 12, 1], [2, 15, 7]),
    b([0, 9, 1], [2, 12, 3]),
    b([14, 6, 1], [16, 9, 7]),
    b([14, 12, 1], [16, 15, 7]),
    b([14, 9, 1], [16, 12, 3]),
];
const GATE_COLLISION: [Box16; 1] = [b([0, 0, 6], [16, 24, 10])];
/// Java's brewing stand model: three stone plates around a rod.
const BREWING_STAND: [Box16; 4] =
    [b([8, 0, 1], [14, 2, 7]), b([2, 0, 5], [8, 2, 11]), b([8, 0, 9], [14, 2, 15]), b([7, 0, 7], [9, 14, 9])];
/// It collides as a 2-pixel slab with the rod on top.
const BREWING_STAND_COLLISION: [Box16; 2] = [b([0, 0, 0], [16, 2, 16]), b([7, 0, 7], [9, 14, 9])];
/// Iron bars: a post, with an arm to the south when they join that way.
const PANE_POST: Box16 = b([7, 0, 7], [9, 16, 9]);
const PANE_ARM: Box16 = b([7, 0, 9], [9, 16, 16]);
/// End portal frames are 13/16 tall; an eye sits on top in the middle.
const FRAME: Box16 = b([0, 0, 0], [16, 13, 16]);
const FRAME_EYE: Box16 = b([4, 13, 4], [12, 16, 12]);
/// The portal's surface: drawn 12/16 up, nothing to collide with.
const END_PORTAL: Box16 = b([0, 11, 0], [16, 12, 16]);
/// Java's dragon egg model, bottom to top.
const DRAGON_EGG: [Box16; 8] = [
    b([3, 0, 3], [13, 1, 13]),
    b([2, 1, 2], [14, 3, 14]),
    b([1, 3, 1], [15, 8, 15]),
    b([2, 8, 2], [14, 11, 14]),
    b([3, 11, 3], [13, 13, 13]),
    b([4, 13, 4], [12, 14, 12]),
    b([5, 14, 5], [11, 15, 11]),
    b([6, 15, 6], [10, 16, 10]),
];
const DRAGON_EGG_COLLISION: Box16 = b([1, 0, 1], [15, 16, 15]);
const ENCHANTING_TABLE: Box16 = b([0, 0, 0], [16, 12, 16]);
/// Java's anvil with its top along z: base, step, neck and top.
const ANVIL: [Box16; 4] =
    [b([2, 0, 2], [14, 4, 14]), b([4, 4, 3], [12, 5, 13]), b([6, 5, 4], [10, 10, 12]), b([3, 10, 0], [13, 16, 16])];
const RAIL: Box16 = b([0, 0, 0], [16, 2, 16]);
const FENCE_POST: Box16 = b([6, 0, 6], [10, 16, 10]);
/// The two rails reaching out to a neighbour on the south side.
const FENCE_RAILS: [Box16; 2] = [b([7, 6, 10], [9, 9, 16]), b([7, 12, 10], [9, 15, 16])];
const FENCE_POST_COLLISION: Box16 = b([6, 0, 6], [10, 24, 10]);
const FENCE_ARM_COLLISION: Box16 = b([6, 0, 10], [10, 24, 16]);
/// Java `WallBlock.makeShapes(16, 14)` for the outline and `(24, 24)` for
/// collision: an 8-wide post and 6-wide sides (south side shown), low sides
/// 14 high and tall ones 16.
const WALL_POST: Box16 = b([4, 0, 4], [12, 16, 12]);
const WALL_ARM: Box16 = b([5, 0, 5], [11, 14, 16]);
const WALL_ARM_TALL: Box16 = b([5, 0, 5], [11, 16, 16]);
const WALL_POST_COLLISION: Box16 = b([4, 0, 4], [12, 24, 12]);
const WALL_ARM_COLLISION: Box16 = b([5, 0, 5], [11, 24, 16]);
/// `WallBlock.TEST_SHAPE_POST` and `TEST_SHAPES_WALL` (north side shown):
/// what the block above must cover for a raised post or a tall side.
const WALL_TEST_POST: Box16 = b([7, 0, 7], [9, 16, 9]);
const WALL_TEST_SIDE: Box16 = b([7, 0, 0], [9, 16, 9]);

/// Java `Block.isExceptionForConnection`: blocks with sturdy faces that
/// fences, panes and walls still don't join (leaves, pumpkins, melons).
fn connection_exception(n: Block) -> bool {
    n.is_leaves() || matches!(n.base(), Block::PUMPKIN | Block::CARVED_PUMPKIN | Block::JACK_O_LANTERN | Block::MELON)
}

/// Java `BlockState.isFaceSturdy(FULL)` for the face of `n` that points
/// `toward` ([`super::mount_states::face_sturdy`]).
pub fn face_sturdy(n: Block, toward: Facing) -> bool {
    super::mount_states::face_sturdy(n, toward.offset())
}

/// Whether fence `fence` joins up with `n` on its `side` (Java
/// `FenceBlock.connectsTo`): a fence of the same kind (wooden and nether
/// brick fences don't join), a gate across the run, or a sturdy face.
pub fn fence_connects(fence: Block, n: Block, side: Facing) -> bool {
    let (fence, n) = (fence.canonical(), n.canonical());
    match n.shaped() {
        Some(Shaped::Fence) => (n == Block::NETHER_BRICK_FENCE) == (fence == Block::NETHER_BRICK_FENCE),
        // `FenceGateBlock.connectsToDirection`: a gate joins at either end of its run.
        Some(Shaped::Gate { facing, .. }) => facing.along_x() != side.along_x(),
        _ => !connection_exception(n) && face_sturdy(n, side.opposite()),
    }
}

/// Java `WallBlock.connectsTo`: other walls, bars and panes, a gate across
/// the run, or a sturdy face (fences don't count).
fn wall_connects(n: Block, side: Facing) -> bool {
    let n = n.canonical();
    match n.shaped() {
        Some(Shaped::Wall | Shaped::Pane) => true,
        Some(Shaped::Gate { facing, .. }) => facing.along_x() != side.along_x(),
        _ => !connection_exception(n) && face_sturdy(n, side.opposite()),
    }
}

/// Whether iron bars or a pane join `n` on `side` (Java
/// `IronBarsBlock.attachsTo`): other bars and panes, walls, or a sturdy face.
pub fn pane_connects_on(n: Block, side: Facing) -> bool {
    let n = n.canonical();
    matches!(n.shaped(), Some(Shaped::Pane | Shaped::Wall))
        || (!connection_exception(n) && face_sturdy(n, side.opposite()))
}

/// [`pane_connects_on`] for a neighbour whose side doesn't matter (full blocks).
pub fn pane_connects(n: Block) -> bool {
    pane_connects_on(n, Facing::South)
}

/// The block above a cell and that block's four horizontal neighbours
/// (in [`Facing::ALL`] order): what a wall reads for its post and tall
/// sides. Other shapes ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Above {
    pub block: Block,
    pub sides: [Block; 4],
}

impl Above {
    /// Open sky: no post override and no tall sides.
    pub const NONE: Above = Above { block: Block::AIR, sides: [Block::AIR; 4] };

    /// What `block` needs from above, read through `at` (offsets from its
    /// cell); [`Above::NONE`] without any lookups unless it is a wall.
    pub fn read(block: Block, at: impl Fn(glam::IVec3) -> Block) -> Above {
        if !matches!(block.shaped(), Some(Shaped::Wall)) {
            return Above::NONE;
        }
        Above { block: at(glam::IVec3::Y), sides: Facing::ALL.map(|f| at(glam::IVec3::Y + f.offset())) }
    }
}

/// Java `WallBlock.updateShape`: which sides join (and whether each is
/// tall) and whether the post is raised. A side is tall where the block
/// above covers it; a straight run drops its post unless something above
/// (a raised wall post, a torch, a covering block over the middle) holds
/// it up, and never with tall sides on both ends.
/// `neighbour` is `dyn` so the recursion through `collision_with` (for the
/// block above) instantiates once.
fn wall_state(neighbour: &dyn Fn(Facing) -> Block, above: Above) -> ([Option<bool>; 4], bool) {
    let above_block = above.block.canonical();
    // `getCollisionShape(...).getFaceShape(DOWN)` of the block above.
    let above_face = if above_block == Block::AIR {
        None
    } else {
        let sides = above.sides;
        let at = |f: Facing| sides[f as usize];
        Some(collision_or_cube(above_block, at, Block::AIR, Above::NONE))
    };
    let covered = |test: Box16| {
        above_face.is_some_and(|boxes| {
            let mut rows = [0u16; 16];
            for bx in boxes.as_slice().iter().filter(|bx| bx.min[1] == 0) {
                let bits = (((1u32 << bx.max[0].min(16)) - 1) & !((1u32 << bx.min[0]) - 1)) as u16;
                for row in &mut rows[bx.min[2] as usize..bx.max[2].min(16) as usize] {
                    *row |= bits;
                }
            }
            let need = (((1u32 << test.max[0]) - 1) & !((1u32 << test.min[0]) - 1)) as u16;
            rows[test.min[2] as usize..test.max[2] as usize].iter().all(|&r| r & need == need)
        })
    };
    let mut sides = [None; 4];
    for f in Facing::ALL {
        if wall_connects(neighbour(f), f) {
            // TEST_SHAPES_WALL is drawn for the north side; `turned` takes the
            // south side to `f`, so the north side goes to `f.opposite()`.
            sides[f as usize] = Some(covered(WALL_TEST_SIDE.turned(f.opposite())));
        }
    }
    let side = |f: Facing| sides[f as usize];
    let raised_above = matches!(above_block.shaped(), Some(Shaped::Wall)) && {
        let sides = above.sides;
        let (_, post) = wall_state(&|f: Facing| sides[f as usize], Above::NONE);
        post
    };
    let none = |f: Facing| side(f).is_none();
    let corner = Facing::ALL.into_iter().all(none)
        || none(Facing::North) != none(Facing::South)
        || none(Facing::East) != none(Facing::West);
    let post = if raised_above || corner {
        true
    } else if (side(Facing::North) == Some(true) && side(Facing::South) == Some(true))
        || (side(Facing::East) == Some(true) && side(Facing::West) == Some(true))
    {
        false
    } else {
        wall_post_override(above_block) || covered(WALL_TEST_POST)
    };
    (sides, post)
}

/// Java's `wall_post_override` block tag (torches, redstone torches,
/// tripwire, signs, banners and pressure plates; VoxelCraft has no signs or
/// banners yet). Wall torches aren't in it.
fn wall_post_override(b: Block) -> bool {
    use super::redstone_blocks::{self as r, Component};
    b == Block::TORCH
        || b == super::nether_biome_blocks::SOUL_TORCH
        || super::gadgets::is_tripwire(b)
        || matches!(r::component(b), Some(Component::Torch { mount: 0, .. } | Component::Plate { .. }))
}

/// `bx` cut down to the half of the cell on side `f`.
fn half_toward(mut bx: Box16, f: Facing) -> Box16 {
    let axis = if f.along_x() { 0 } else { 2 };
    if matches!(f, Facing::East | Facing::South) {
        bx.min[axis] = 8;
    } else {
        bx.max[axis] = 8;
    }
    bx
}

/// Java 26.3 `StairBlock.getShape`: a slab across the cell (the bottom half,
/// or the top half when upside down) and the step in the other half. The
/// step is the half on Java's `facing` side (our front's opposite) for
/// straight stairs, one quarter of it for outer corners and that half plus
/// a quarter for inner corners; left corners turn toward Java's
/// counter-clockwise side, right corners toward its clockwise side.
fn push_stairs(out: &mut Boxes, block: Block, front: Facing) {
    use super::slab_stair_states::{self as ss, StairShape};
    let (top, shape) = ss::stairs(block).map_or((false, StairShape::Straight), |s| (s.top, s.shape));
    let (slab, step) = if top { ((8, 16), (0, 8)) } else { ((0, 8), (8, 16)) };
    out.push(b([0, slab.0, 0], [16, slab.1, 16]));
    let layer = b([0, step.0, 0], [16, step.1, 16]);
    let facing = front.opposite();
    let (cw, ccw) = (facing.clockwise(), facing.clockwise().opposite());
    match shape {
        StairShape::Straight => out.push(half_toward(layer, facing)),
        StairShape::OuterLeft => out.push(half_toward(half_toward(layer, facing), ccw)),
        StairShape::OuterRight => out.push(half_toward(half_toward(layer, facing), cw)),
        StairShape::InnerLeft | StairShape::InnerRight => {
            let side = if shape == StairShape::InnerLeft { ccw } else { cw };
            out.push(half_toward(layer, facing));
            out.push(half_toward(half_toward(layer, front), side));
        }
    }
}

/// Where an open door's panel lies: it swings to the left of someone
/// walking in through the side it faces.
/// Java `DoorBlock.getShape`: an open door swings to the clockwise side of
/// the face it closes on when hinged left, to the counter-clockwise side when
/// hinged right. (Our facing is Java's opposite, which names the same sides.)
fn open_door_side(facing: Facing, hinge_right: bool) -> Facing {
    if hinge_right { facing.clockwise().clockwise().clockwise() } else { facing.clockwise() }
}

/// The visible boxes of `block`; `neighbour(f)` is the block on side `f`
/// (fences, gates and walls look at it). `below` is the block under this
/// one: a wood door's upper half stores no facing, so it copies the lower
/// half. Empty for blocks that aren't shaped.
pub fn shape(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block) -> Boxes {
    shape_with(block, neighbour, below, Above::NONE)
}

/// [`shape`] with what is above the cell (walls read it).
pub fn shape_with(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block, above: Above) -> Boxes {
    let block = block.canonical();
    let neighbour = |f: Facing| neighbour(f).canonical();
    let below = below.canonical();
    let mut out = Boxes::new();
    match block.shaped() {
        Some(Shaped::Village { kind, facing }) => match kind {
            2 => {
                let level = super::composter::level(block).unwrap_or(0).min(7);
                out.push(b([0, 0, 0], [16, (1 + level * 2).max(2), 16]));
                out.push(b([0, 2, 0], [2, 16, 16]));
                out.push(b([14, 2, 0], [16, 16, 16]));
                out.push(b([2, 2, 0], [14, 16, 2]));
                out.push(b([2, 2, 14], [14, 16, 16]));
            }
            10 => grindstone_boxes(block, &mut out),
            11 => out.push_turned(
                &[b([0, 0, 0], [16, 2, 16]), b([4, 2, 4], [12, 13, 12]), b([0, 12, 0], [16, 16, 16])],
                facing,
            ),
            13 => out.push_turned(&[b([0, 0, 0], [16, 9, 16]), b([7, 9, 1], [9, 16, 15])], facing),
            14 => out.push_turned(
                &[
                    b([2, 0, 6], [4, 16, 10]),
                    b([12, 0, 6], [14, 16, 10]),
                    b([4, 13, 7], [12, 16, 9]),
                    b([5, 3, 5], [11, 12, 11]),
                    b([4, 2, 4], [12, 4, 12]),
                ],
                facing,
            ),
            _ => {}
        },
        Some(Shaped::Redstone) => {
            return redstone_shape(
                block,
                |d| if let Some(f) = Facing::from_offset(d) { neighbour(f) } else { Block::AIR },
            );
        }
        Some(Shaped::Stairs(f)) => push_stairs(&mut out, block, f),
        Some(Shaped::Slab) => out.push(TOP_SLAB),
        Some(Shaped::Ladder(f)) => out.push_turned(&LADDER, f),
        Some(Shaped::BrewingStand) => out.push_turned(&BREWING_STAND, Facing::South),
        Some(Shaped::Pane) => {
            out.push(PANE_POST);
            for f in Facing::ALL {
                if pane_connects_on(neighbour(f), f) {
                    out.push(PANE_ARM.turned(f));
                }
            }
        }
        Some(Shaped::Frame { eye, .. }) => {
            out.push(FRAME);
            if eye {
                out.push(FRAME_EYE);
            }
        }
        Some(Shaped::EndPortal) => out.push(END_PORTAL),
        Some(Shaped::DragonEgg) => DRAGON_EGG.iter().for_each(|&e| out.push(e)),
        Some(Shaped::EnchantingTable) => out.push(ENCHANTING_TABLE),
        Some(Shaped::Anvil { along_x }) => out.push_turned(&ANVIL, if along_x { Facing::East } else { Facing::South }),
        Some(Shaped::Door { facing, open, upper }) => {
            let ((facing, open), hinge_right) = if upper {
                let lower = match below.shaped() {
                    Some(Shaped::Door { facing, open, upper: false }) => (facing, open),
                    _ => (facing, open),
                };
                (lower, super::mount_states::is_right_hinge(below))
            } else {
                ((facing, open), super::mount_states::is_right_hinge(block))
            };
            out.push_turned(&DOOR, if open { open_door_side(facing, hinge_right) } else { facing })
        }
        Some(Shaped::Wall) => {
            let (sides, post) = wall_state(&neighbour, above);
            if post {
                out.push(WALL_POST);
            }
            for f in Facing::ALL {
                if let Some(tall) = sides[f as usize] {
                    out.push((if tall { WALL_ARM_TALL } else { WALL_ARM }).turned(f));
                }
            }
        }
        Some(Shaped::Gate { facing, open }) => {
            out.push_turned(if open { &GATE_OPEN[..] } else { &GATE_CLOSED[..] }, facing)
        }
        Some(Shaped::Chain(axis)) => {
            let (min, max) = match axis {
                1 => ([0, 6, 6], [16, 9, 9]),
                2 => ([6, 6, 0], [9, 9, 16]),
                _ => ([6, 0, 6], [9, 16, 9]),
            };
            out.push(b(min, max));
        }
        Some(Shaped::Rail) => out.push(RAIL),
        Some(Shaped::Custom) if super::mount_states::is_lantern(block.0) => lantern_boxes(block, &mut out),
        Some(Shaped::Custom) => super::overworld_blocks::boxes(block).unwrap_or(&[]).iter().for_each(|&b| out.push(b)),
        Some(Shaped::Hook { facing }) => out.push_turned(
            &[
                b([5, 2, 0], [11, 9, 2]),
                b([7, 3, 2], [9, 5, 7]),
                b([5, 2, 6], [11, 3, 7]),
                b([5, 2, 4], [6, 3, 7]),
                b([10, 2, 4], [11, 3, 7]),
            ],
            facing,
        ),
        Some(Shaped::Tripwire) => {
            let connects = |f: Facing| {
                let n = neighbour(f);
                super::gadgets::is_tripwire(n)
                    || super::gadgets::hook_state(n).is_some_and(|(facing, _, _)| facing == f.opposite())
            };
            let ns = connects(Facing::North) || connects(Facing::South);
            let ew = connects(Facing::East) || connects(Facing::West);
            if ns || !ew {
                out.push(b([8, 1, 0], [9, 2, 16]));
            }
            if ew || !ns {
                out.push(b([0, 1, 8], [16, 2, 9]));
            }
        }
        Some(Shaped::Cake { bites }) => out.push(b([1 + 2 * bites, 0, 1], [15, 8, 15])),
        Some(Shaped::Fence) => {
            out.push(FENCE_POST);
            for f in Facing::ALL {
                if fence_connects(block, neighbour(f), f) {
                    out.push_turned(&FENCE_RAILS, f);
                }
            }
        }
        None => {}
    }
    out
}

/// Target selection (`getShape`) differs from visible model boxes and
/// movement collision. Fence arms fill the spaces between their rails;
/// small plants occupy only their reference outline within the cell.
pub fn outline(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block) -> Boxes {
    outline_with(block, neighbour, below, Above::NONE)
}

/// [`outline`] with what is above the cell (walls read it).
pub fn outline_with(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block, above: Above) -> Boxes {
    let block = block.canonical();
    let neighbour = |f: Facing| neighbour(f).canonical();
    let below = below.canonical();
    if matches!(block.shaped(), Some(Shaped::Fence)) {
        let mut out = Boxes::from_box(FENCE_POST);
        for f in Facing::ALL {
            if fence_connects(block, neighbour(f), f) {
                out.push(b([6, 0, 8], [10, 16, 16]).turned(f));
            }
        }
        return out;
    }
    if let Some(Shaped::Gate { facing, .. }) = block.shaped() {
        let in_wall = Facing::ALL
            .into_iter()
            .any(|f| f.along_x() != facing.along_x() && matches!(neighbour(f).shaped(), Some(Shaped::Wall)));
        return Boxes::from_box(b([0, 0, 6], [16, if in_wall { 13 } else { 16 }, 10]).turned(facing));
    }
    if let Some(Shaped::Ladder(facing)) = block.shaped() {
        return Boxes::from_box(LADDER_COLLISION[0].turned(facing));
    }
    if block.shaped().is_some() {
        return shape_with(block, neighbour, below, above);
    }
    let bx = match block {
        Block::TALL_GRASS | Block::FERN => b([2, 0, 2], [14, 13, 14]),
        Block::DANDELION | Block::POPPY | Block::BLUE_ORCHID => b([5, 0, 5], [11, 10, 11]),
        Block::TORCH => b([6, 0, 6], [10, 10, 10]),
        b0 if b0.is_sapling() => b([2, 0, 2], [14, 12, 14]),
        b0 if b0.is_mushroom() => b([5, 0, 5], [11, 6, 11]),
        _ => b([0; 3], [16, 16 - block.top_drop(), 16]),
    };
    Boxes::from_box(bx)
}

/// Redstone dust includes diagonal/vertical neighbours while ordinary shapes
/// need only the four horizontal blocks. Meshing supplies the full lookup.
pub fn redstone_shape(block: Block, neighbour: impl Fn(glam::IVec3) -> Block) -> Boxes {
    use super::redstone_blocks::{self as r, Component};
    let block = block.canonical();
    let neighbour = |d: glam::IVec3| neighbour(d).canonical();
    let mut out = Boxes::new();
    match r::component(block) {
        Some(Component::Piston { facing, extended: true, .. }) => {
            out.push(directional_box(b([0, 0, 0], [16, 16, 12]), facing))
        }
        Some(Component::PistonHead { facing, .. }) => {
            out.push(directional_box(b([0, 0, 12], [16, 16, 16]), facing));
            out.push(directional_box(b([6, 6, 0], [10, 10, 12]), facing));
        }
        Some(Component::Moving) => out.push(b([0, 0, 0], [16, 16, 16])),
        Some(Component::Hopper { facing, .. }) => {
            for bx in [
                b([0, 10, 0], [16, 16, 2]),
                b([0, 10, 14], [16, 16, 16]),
                b([0, 10, 2], [2, 16, 14]),
                b([14, 10, 2], [16, 16, 14]),
                b([4, 4, 4], [12, 10, 12]),
            ] {
                out.push(bx);
            }
            out.push(if facing == 4 {
                b([6, 0, 6], [10, 4, 10])
            } else {
                b([6, 4, 8], [10, 8, 16]).turned(Facing::ALL[facing as usize])
            });
        }
        Some(Component::Wire(_)) => {
            let connections = r::connections(neighbour);
            out.push(b([6, 0, 6], [10, 1, 10]));
            for f in Facing::ALL {
                if connections[f as usize] != 0 {
                    out.push(b([6, 0, 8], [10, 1, 16]).turned(f));
                }
                if connections[f as usize] == 2 {
                    out.push(b([6, 0, 15], [10, 16, 16]).turned(f));
                }
            }
        }
        Some(Component::Lever { mount, on }) => {
            let handle = if on { b([6, 3, 8], [10, 10, 12]) } else { b([6, 3, 4], [10, 10, 8]) };
            for bx in [b([4, 0, 5], [12, 3, 11]), handle] {
                out.push(mounted(bx, mount));
            }
        }
        Some(Component::Button { mount, on, .. }) => {
            out.push(mounted(b([5, 0, 6], [11, if on { 1 } else { 2 }, 10]), mount))
        }
        Some(Component::Torch { mount, .. }) => {
            if mount == 0 {
                out.push(b([7, 0, 7], [9, 8, 9]));
                out.push(b([6, 7, 6], [10, 10, 10]));
            } else {
                let facing = Facing::ALL[(mount - 1) as usize];
                out.push(b([7, 3, 1], [9, 11, 3]).turned(facing));
                out.push(b([6, 10, 0], [10, 13, 4]).turned(facing));
            }
        }
        Some(Component::Repeater { facing, delay, .. }) => {
            out.push(b([0, 0, 0], [16, 2, 16]));
            out.push(b([7, 2, 11], [9, 8, 13]).turned(facing));
            out.push(b([7, 2, 1 + delay * 2], [9, 8, 3 + delay * 2]).turned(facing));
        }
        Some(Component::Comparator { facing, .. }) => {
            out.push(b([0, 0, 0], [16, 2, 16]));
            for bx in [b([3, 2, 3], [5, 8, 5]), b([11, 2, 3], [13, 8, 5]), b([7, 2, 11], [9, 8, 13])] {
                out.push(bx.turned(facing));
            }
        }
        Some(Component::Plate { power, .. }) => out.push(b([1, 0, 1], [15, if power > 0 { 1 } else { 2 }, 15])),
        Some(Component::Daylight { .. }) => out.push(b([0, 0, 0], [16, 6, 16])),
        Some(Component::Trapdoor { facing, open, top, .. }) => {
            out.push(if open {
                b([0, 0, 0], [16, 16, 3]).turned(facing)
            } else if top {
                b([0, 13, 0], [16, 16, 16])
            } else {
                b([0, 0, 0], [16, 3, 16])
            });
        }
        _ => {}
    }
    out
}

/// Java `GrindstoneBlock.makeShapes`: the wheel and its two pairs of legs,
/// drawn for a wall grindstone facing south (attached at z = 0), then laid
/// on the floor or ceiling by swapping depth and height.
const GRINDSTONE_WALL: [Box16; 5] = [
    b([4, 2, 4], [12, 14, 16]),
    b([2, 6, 0], [4, 10, 9]),
    b([2, 5, 7], [4, 11, 13]),
    b([12, 6, 0], [14, 10, 9]),
    b([12, 5, 7], [14, 11, 13]),
];

fn grindstone_boxes(block: Block, out: &mut Boxes) {
    use super::mount_states::AttachFace;
    let Some((face, facing)) = super::mount_states::grindstone_state(block.0) else { return };
    for bx in GRINDSTONE_WALL {
        let bx = match face {
            AttachFace::Wall => bx,
            AttachFace::Floor => b([bx.min[0], bx.min[2], bx.min[1]], [bx.max[0], bx.max[2], bx.max[1]]),
            AttachFace::Ceiling => b([bx.min[0], 16 - bx.max[2], bx.min[1]], [bx.max[0], 16 - bx.min[2], bx.max[1]]),
        };
        // Our wall facing is Java's, drawn from the south-facing base.
        out.push(bx.turned(facing));
    }
}

/// Java `LanternBlock`: a 6×7×6 body under a 4×2×4 cap, one pixel higher
/// when hanging.
fn lantern_boxes(block: Block, out: &mut Boxes) {
    let up = super::mount_states::lantern_of(block).is_some_and(|l| l.hanging) as u8;
    out.push(b([5, up, 5], [11, 7 + up, 11]));
    out.push(b([6, 7 + up, 6], [10, 9 + up, 10]));
}

fn directional_box(bx: Box16, facing: u8) -> Box16 {
    match facing {
        4 => b([bx.min[0], bx.min[2], bx.min[1]], [bx.max[0], bx.max[2], bx.max[1]]),
        5 => b([bx.min[0], 16 - bx.max[2], bx.min[1]], [bx.max[0], 16 - bx.min[2], bx.max[1]]),
        _ => bx.turned(Facing::ALL[facing as usize]),
    }
}

/// A box drawn for a floor mount facing south, moved to `mount` (see
/// `mount_states::mount_attach`); floor and ceiling mounts turn with their
/// facing.
fn mounted(bx: Box16, mount: u8) -> Box16 {
    let flipped = b([bx.min[0], 16 - bx.max[1], bx.min[2]], [bx.max[0], 16 - bx.min[1], bx.max[2]]);
    match mount {
        0 => bx,
        5 => flipped,
        6..=8 => bx.turned(Facing::ALL[(mount - 5) as usize]),
        9..=11 => flipped.turned(Facing::ALL[(mount - 8) as usize]),
        _ => b([bx.min[0], bx.min[2], bx.min[1]], [bx.max[0], bx.max[2], bx.max[1]])
            .turned(Facing::ALL[(mount - 1) as usize]),
    }
}

/// What `block` collides with: like [`shape`], but ladders are thicker, open
/// gates let you through, and fences and closed gates are 1.5 blocks tall
/// so nothing can jump them.
pub fn collision(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block) -> Boxes {
    collision_with(block, neighbour, below, Above::NONE)
}

/// [`collision_with`] for any block: an unshaped solid block (a cube, or a
/// bottom slab's half) gets a box as high as the block, and other unshaped
/// blocks (air, fluids, plants) none. Collision of shaped blocks is drawn
/// by [`collision_with`]; this is for rules that compare whole shapes.
pub fn collision_or_cube(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block, above: Above) -> Boxes {
    let block = block.canonical();
    if block.kind() == super::block::RenderKind::Shaped {
        return collision_with(block, neighbour, below, above);
    }
    let mut out = Boxes::new();
    match block.base() {
        // Drawn as cubes, but Java's shapes are inset: a single chest
        // (`Block.column(14, 0, 14)`) and cactus collision.
        Block::CHEST => out.push(b([1, 0, 1], [15, 14, 15])),
        Block::CACTUS => out.push(b([1, 0, 1], [15, 15, 15])),
        _ if block.is_solid() => out.push(b([0, 0, 0], [16, 16 - block.top_drop(), 16])),
        _ => {}
    }
    out
}

/// [`collision`] with what is above the cell (a wall's raised post).
pub fn collision_with(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block, above: Above) -> Boxes {
    let block = block.canonical();
    let neighbour = |f: Facing| neighbour(f).canonical();
    let below = below.canonical();
    let mut out = Boxes::new();
    match block.shaped() {
        Some(Shaped::Redstone)
            if matches!(
                super::redstone_blocks::component(block),
                Some(
                    super::redstone_blocks::Component::Wire(_)
                        | super::redstone_blocks::Component::Lever { .. }
                        | super::redstone_blocks::Component::Button { .. }
                        | super::redstone_blocks::Component::Torch { .. }
                        | super::redstone_blocks::Component::Plate { .. }
                )
            ) => {}
        Some(Shaped::Rail | Shaped::Hook { .. } | Shaped::Tripwire) => {}
        Some(Shaped::Ladder(f)) => out.push_turned(&LADDER_COLLISION, f),
        Some(Shaped::BrewingStand) => out.push_turned(&BREWING_STAND_COLLISION, Facing::South),
        Some(Shaped::EndPortal) => {}
        Some(Shaped::Custom) if super::mount_states::is_lantern(block.0) => lantern_boxes(block, &mut out),
        Some(Shaped::Custom) => {
            super::overworld_blocks::collision(block).unwrap_or(&[]).iter().for_each(|&b| out.push(b))
        }
        Some(Shaped::DragonEgg) => out.push(DRAGON_EGG_COLLISION),
        Some(Shaped::Gate { open: true, .. }) => {}
        Some(Shaped::Gate { facing, .. }) => out.push_turned(&GATE_COLLISION, facing),
        Some(Shaped::Fence) => {
            out.push(FENCE_POST_COLLISION);
            for f in Facing::ALL {
                if fence_connects(block, neighbour(f), f) {
                    out.push(FENCE_ARM_COLLISION.turned(f));
                }
            }
        }
        Some(Shaped::Wall) => {
            let (sides, post) = wall_state(&neighbour, above);
            if post {
                out.push(WALL_POST_COLLISION);
            }
            for f in Facing::ALL {
                if sides[f as usize].is_some() {
                    out.push(WALL_ARM_COLLISION.turned(f));
                }
            }
        }
        _ => return shape_with(block, neighbour, below, above),
    }
    out
}

/// The shape shown for `block` as an item (in inventories and when
/// dropped): stairs face east, so their profile shows on the left of an
/// icon, and fences reach out east and west.
pub fn item_shape(block: Block) -> Boxes {
    let block = block.canonical();
    if block == Block::STONE {
        static MODEL: std::sync::LazyLock<Vec<Box16>> = std::sync::LazyLock::new(|| {
            crate::assets::model_boxes("voxelcraft:stone_model").expect("invalid original stone model")
        });
        let mut out = Boxes::new();
        for &b in MODEL.iter() {
            out.push(b);
        }
        return out;
    }
    let block = if block.stairs_base().is_some() { block.with_facing(Facing::East) } else { block };
    shape(block, |f| if f.along_x() { block } else { Block::AIR }, Block::AIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: Facing) -> Block {
        Block::AIR
    }

    #[test]
    fn turning_maps_south_to_each_facing() {
        // The door panel (south side) moves to the side each facing names.
        let door = DOOR[0];
        assert_eq!(door.turned(Facing::South), door);
        assert_eq!(door.turned(Facing::West), b([0, 0, 0], [3, 16, 16]));
        assert_eq!(door.turned(Facing::North), b([0, 0, 0], [16, 16, 3]));
        assert_eq!(door.turned(Facing::East), b([13, 0, 0], [16, 16, 16]));
    }

    #[test]
    fn stairs_rise_away_from_their_front() {
        let s = shape(Block::STONE_STAIRS.with_facing(Facing::East), none, Block::AIR);
        assert_eq!(s.as_slice(), &[b([0, 0, 0], [16, 8, 16]), b([0, 8, 0], [8, 16, 16])]);
        // The tall half's underside sits on the slab, so it's hidden.
        assert!(s.face_hidden(1, 1, false, [0, 16, 0, 8]));
        // Only part of the slab's top is under the tall half.
        assert!(!s.face_hidden(0, 1, true, [0, 16, 0, 16]));
    }

    #[test]
    fn fences_join_fences_gates_and_walls() {
        let lone = shape(Block::OAK_FENCE, none, Block::AIR);
        assert_eq!(lone.as_slice(), &[FENCE_POST]);
        let joined = shape(
            Block::OAK_FENCE,
            |f| match f {
                Facing::East => Block::OAK_FENCE,
                Facing::West => Block::STONE,
                // A gate running along X doesn't join on its side.
                Facing::South => Block::gate(Facing::South, false),
                Facing::North => Block::gate(Facing::East, false),
            },
            Block::AIR,
        );
        assert_eq!(joined.as_slice().len(), 1 + 3 * 2);
        let tall = collision(Block::OAK_FENCE, none, Block::AIR).bounds().unwrap();
        let mixed = |f: Facing| if f == Facing::East { Block::NETHER_BRICK_FENCE } else { Block::AIR };
        assert_eq!(shape(Block::OAK_FENCE, mixed, Block::AIR).as_slice().len(), 1, "wood and nether brick don't join");
        let oak = |f: Facing| if f == Facing::East { Block::OAK_FENCE } else { Block::AIR };
        assert_eq!(shape(Block::NETHER_BRICK_FENCE, oak, Block::AIR).as_slice().len(), 1, "either way round");
        let nether = |f: Facing| if f == Facing::East { Block::NETHER_BRICK_FENCE } else { Block::AIR };
        assert_eq!(shape(Block::NETHER_BRICK_FENCE, nether, Block::AIR).as_slice().len(), 3);
        assert_eq!(tall.max[1], 24);
    }

    #[test]
    fn doors_swing_and_gates_open_up() {
        let closed = shape(Block::door(Facing::South, false, false), none, Block::AIR);
        let open = shape(Block::door(Facing::South, true, false), none, Block::AIR);
        assert_eq!(closed.as_slice(), &DOOR);
        assert_ne!(open.as_slice(), closed.as_slice());
        // A compact upper door copies the lower half, which is the only
        // place its facing and open bit are stored.
        let lower = crate::world::forms::wood_id(0, 18);
        let upper = crate::world::forms::wood_id(0, 22);
        let copied = shape(upper, none, lower);
        assert_eq!(copied.as_slice(), open.as_slice());
        assert!(collision(Block::gate(Facing::North, true), none, Block::AIR).is_empty());
        assert_eq!(collision(Block::gate(Facing::East, false), none, Block::AIR).bounds().unwrap().max[1], 24);
        assert!(shape(Block::STONE, none, Block::AIR).is_empty());
    }

    /// Whether the 1/16 voxel at `(x, y, z)` is inside one of `boxes`.
    fn filled(boxes: &Boxes, x: u8, y: u8, z: u8) -> bool {
        boxes.as_slice().iter().any(|b| {
            (b.min[0]..b.max[0]).contains(&x) && (b.min[1]..b.max[1]).contains(&y) && (b.min[2]..b.max[2]).contains(&z)
        })
    }

    #[test]
    fn every_stairs_state_matches_javas_quadrant_shapes() {
        use crate::world::slab_stair_states::{self as ss, StairShape, Stairs};
        for front in Facing::ALL {
            for top in [false, true] {
                for shape in StairShape::ALL {
                    for wet in [false, true] {
                        let block = ss::stairs_id(Stairs { family: 3, facing: front, top, shape, wet });
                        let boxes = super::shape(block, none, Block::AIR);
                        // Java's `facing` (tall side) and the quadrants the step
                        // fills: outer = SHAPE_OUTER, inner = SHAPE_INNER,
                        // left turning counter-clockwise.
                        let facing = front.opposite();
                        let (cw, ccw) = (facing.clockwise(), facing.clockwise().opposite());
                        let toward = |q: (i32, i32), f: Facing| q.0 * f.offset().x + q.1 * f.offset().z > 0;
                        let step = |q: (i32, i32)| match shape {
                            StairShape::Straight => toward(q, facing),
                            StairShape::OuterLeft => toward(q, facing) && toward(q, ccw),
                            StairShape::OuterRight => toward(q, facing) && toward(q, cw),
                            StairShape::InnerLeft => toward(q, facing) || toward(q, ccw),
                            StairShape::InnerRight => toward(q, facing) || toward(q, cw),
                        };
                        for x in 0..16u8 {
                            for y in 0..16u8 {
                                for z in 0..16u8 {
                                    let q = (if x < 8 { -1 } else { 1 }, if z < 8 { -1 } else { 1 });
                                    let slab_half = (y < 8) != top;
                                    assert_eq!(filled(&boxes, x, y, z), slab_half || step(q), "{block:?} {x} {y} {z}");
                                }
                            }
                        }
                        // Collision and the crosshair outline are the same shape.
                        assert_eq!(collision(block, none, Block::AIR).as_slice(), boxes.as_slice());
                        assert_eq!(outline(block, none, Block::AIR).as_slice(), boxes.as_slice());
                    }
                }
            }
        }
    }

    #[test]
    fn slab_types_have_their_half_or_whole_cell() {
        use crate::world::slab_stair_states::{self as ss, Slab, SlabType};
        for family in 0..ss::FAMILY_COUNT as u8 {
            let id = |kind, wet| ss::slab_id(Slab { family, kind, wet });
            for wet in [false, true] {
                let bottom = id(SlabType::Bottom, wet);
                assert!(bottom.shaped().is_none());
                assert_eq!(bottom.height(), 0.5);
                assert_eq!(outline(bottom, none, Block::AIR).as_slice(), &[b([0, 0, 0], [16, 8, 16])]);
                let top = id(SlabType::Top, wet);
                assert_eq!(top.kind(), crate::world::block::RenderKind::Shaped);
                assert_eq!(top.top_drop(), 0);
                for boxes in
                    [shape(top, none, Block::AIR), collision(top, none, Block::AIR), outline(top, none, Block::AIR)]
                {
                    assert_eq!(boxes.as_slice(), &[b([0, 8, 0], [16, 16, 16])]);
                }
            }
            let double = id(SlabType::Double, false);
            assert!(double.is_opaque() && double.shaped().is_none() && double.height() == 1.0);
            assert_eq!(outline(double, none, Block::AIR).as_slice(), &[Box16::FULL]);
        }
    }
}
