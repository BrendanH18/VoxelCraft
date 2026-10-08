//! Geometry of shaped blocks (stairs, fences, gates, ladders, doors): a few
//! axis-aligned boxes in 1/16-block units, used alike by the mesher, physics,
//! the crosshair and inventory icons.
//!
//! Fences and gates look at their four horizontal neighbours, so their
//! shape is computed from a neighbour lookup rather than stored.

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

/// Stairs facing south: a bottom slab with the tall half at the back (north).
const STAIRS: [Box16; 2] = [b([0, 0, 0], [16, 8, 16]), b([0, 8, 0], [16, 16, 8])];
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
/// A wall post. The low arm (south) stops at 14; a tall arm meets fences.
const WALL_POST: Box16 = b([4, 0, 4], [12, 16, 12]);
const WALL_ARM: Box16 = b([5, 0, 8], [11, 14, 16]);
const WALL_ARM_TALL: Box16 = b([5, 0, 8], [11, 16, 16]);
const WALL_POST_COLLISION: Box16 = b([4, 0, 4], [12, 24, 12]);
const WALL_ARM_COLLISION: Box16 = b([4, 0, 8], [12, 24, 16]);

/// Whether fence `fence` joins up with `n` on its `side`. Wooden and nether
/// brick fences don't join each other, like Java's.
pub fn fence_connects(fence: Block, n: Block, side: Facing) -> bool {
    match n.shaped() {
        Some(Shaped::Fence) => (n == Block::NETHER_BRICK_FENCE) == (fence == Block::NETHER_BRICK_FENCE),
        // A gate joins fences at either end of its run.
        Some(Shaped::Gate { facing, .. }) => facing.along_x() != side.along_x(),
        Some(Shaped::Wall) => true,
        _ => n.is_opaque(),
    }
}

/// Walls join full blocks, fences, gates and other walls.
fn wall_connects(n: Block) -> bool {
    matches!(n.shaped(), Some(Shaped::Wall | Shaped::Fence | Shaped::Gate { .. })) || n.is_opaque()
}

/// Whether iron bars join `n` on a side: other bars and full blocks.
pub fn pane_connects(n: Block) -> bool {
    n == Block::IRON_BARS
        || n == Block::GLASS
        || n.is_glass_pane()
        || n.stained_glass_color().is_some()
        || n.is_opaque()
}

/// Where an open door's panel lies: it swings to the left of someone
/// walking in through the side it faces.
fn open_door_side(facing: Facing) -> Facing {
    facing.clockwise()
}

/// The visible boxes of `block`; `neighbour(f)` is the block on side `f`
/// (fences, gates and walls look at it). `below` is the block under this
/// one: a wood door's upper half stores no facing, so it copies the lower
/// half. Empty for blocks that aren't shaped.
pub fn shape(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block) -> Boxes {
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
            10 => out.push_turned(
                &[b([2, 0, 6], [4, 7, 10]), b([12, 0, 6], [14, 7, 10]), b([4, 2, 2], [12, 16, 14])],
                facing,
            ),
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
        Some(Shaped::Stairs(f)) => out.push_turned(&STAIRS, f),
        Some(Shaped::Ladder(f)) => out.push_turned(&LADDER, f),
        Some(Shaped::BrewingStand) => out.push_turned(&BREWING_STAND, Facing::South),
        Some(Shaped::Pane) => {
            out.push(PANE_POST);
            for f in Facing::ALL {
                if pane_connects(neighbour(f)) {
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
            let (facing, open) = if upper {
                match below.shaped() {
                    Some(Shaped::Door { facing, open, upper: false }) => (facing, open),
                    _ => (facing, open),
                }
            } else {
                (facing, open)
            };
            out.push_turned(&DOOR, if open { open_door_side(facing) } else { facing })
        }
        Some(Shaped::Wall) => {
            let linked = |f: Facing| wall_connects(neighbour(f));
            let (north, south, east, west) =
                (linked(Facing::North), linked(Facing::South), linked(Facing::East), linked(Facing::West));
            let straight = (north && south && !east && !west) || (east && west && !north && !south);
            if !straight {
                out.push(WALL_POST);
            }
            for f in Facing::ALL {
                if linked(f) {
                    let tall =
                        matches!(neighbour(f).shaped(), Some(Shaped::Wall | Shaped::Fence | Shaped::Gate { .. }));
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

/// Redstone dust includes diagonal/vertical neighbours while ordinary shapes
/// need only the four horizontal blocks. Meshing supplies the full lookup.
pub fn redstone_shape(block: Block, neighbour: impl Fn(glam::IVec3) -> Block) -> Boxes {
    use super::redstone_blocks::{self as r, Component};
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
            let connections = r::connections(&neighbour);
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

fn directional_box(bx: Box16, facing: u8) -> Box16 {
    match facing {
        4 => b([bx.min[0], bx.min[2], bx.min[1]], [bx.max[0], bx.max[2], bx.max[1]]),
        5 => b([bx.min[0], 16 - bx.max[2], bx.min[1]], [bx.max[0], 16 - bx.min[2], bx.max[1]]),
        _ => bx.turned(Facing::ALL[facing as usize]),
    }
}

fn mounted(bx: Box16, mount: u8) -> Box16 {
    match mount {
        0 => bx,
        5 => b([bx.min[0], 16 - bx.max[1], bx.min[2]], [bx.max[0], 16 - bx.min[1], bx.max[2]]),
        _ => b([bx.min[0], bx.min[2], bx.min[1]], [bx.max[0], bx.max[2], bx.max[1]])
            .turned(Facing::ALL[(mount - 1) as usize]),
    }
}

/// What `block` collides with: like [`shape`], but ladders are thicker, open
/// gates let you through, and fences and closed gates are 1.5 blocks tall
/// so nothing can jump them.
pub fn collision(block: Block, neighbour: impl Fn(Facing) -> Block, below: Block) -> Boxes {
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
            out.push(WALL_POST_COLLISION);
            for f in Facing::ALL {
                if wall_connects(neighbour(f)) {
                    out.push(WALL_ARM_COLLISION.turned(f));
                }
            }
        }
        _ => return shape(block, neighbour, below),
    }
    out
}

/// The shape shown for `block` as an item (in inventories and when
/// dropped): stairs face east, so their profile shows on the left of an
/// icon, and fences reach out east and west.
pub fn item_shape(block: Block) -> Boxes {
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
}
