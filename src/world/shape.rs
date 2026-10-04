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
const FENCE_POST: Box16 = b([6, 0, 6], [10, 16, 10]);
/// The two rails reaching out to a neighbour on the south side.
const FENCE_RAILS: [Box16; 2] = [b([7, 6, 10], [9, 9, 16]), b([7, 12, 10], [9, 15, 16])];
const FENCE_POST_COLLISION: Box16 = b([6, 0, 6], [10, 24, 10]);
const FENCE_ARM_COLLISION: Box16 = b([6, 0, 10], [10, 24, 16]);

/// Whether fence `fence` joins up with `n` on its `side`. Wooden and nether
/// brick fences don't join each other, like Java's.
pub fn fence_connects(fence: Block, n: Block, side: Facing) -> bool {
    match n.shaped() {
        Some(Shaped::Fence) => (n == Block::NETHER_BRICK_FENCE) == (fence == Block::NETHER_BRICK_FENCE),
        // A gate joins fences at either end of its run.
        Some(Shaped::Gate { facing, .. }) => facing.along_x() != side.along_x(),
        _ => n.is_opaque(),
    }
}

/// Where an open door's panel lies: it swings to the left of someone
/// walking in through the side it faces.
fn open_door_side(facing: Facing) -> Facing {
    facing.clockwise()
}

/// The visible boxes of `block`; `neighbour(f)` is the block on side `f`
/// (fences and gates only look at it). Empty for blocks that aren't shaped.
pub fn shape(block: Block, neighbour: impl Fn(Facing) -> Block) -> Boxes {
    let mut out = Boxes::new();
    match block.shaped() {
        Some(Shaped::Stairs(f)) => out.push_turned(&STAIRS, f),
        Some(Shaped::Ladder(f)) => out.push_turned(&LADDER, f),
        Some(Shaped::Door { facing, open, .. }) => {
            out.push_turned(&DOOR, if open { open_door_side(facing) } else { facing })
        }
        Some(Shaped::Gate { facing, open }) => {
            out.push_turned(if open { &GATE_OPEN[..] } else { &GATE_CLOSED[..] }, facing)
        }
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

/// What `block` collides with: like [`shape`], but ladders are thicker, open
/// gates let you through, and fences and closed gates are 1.5 blocks tall
/// so nothing can jump them.
pub fn collision(block: Block, neighbour: impl Fn(Facing) -> Block) -> Boxes {
    let mut out = Boxes::new();
    match block.shaped() {
        Some(Shaped::Ladder(f)) => out.push_turned(&LADDER_COLLISION, f),
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
        _ => return shape(block, neighbour),
    }
    out
}

/// The shape shown for `block` as an item (in inventories and when
/// dropped): stairs face east, so their profile shows on the left of an
/// icon, and fences reach out east and west.
pub fn item_shape(block: Block) -> Boxes {
    let block = if block.stairs_base().is_some() { block.with_facing(Facing::East) } else { block };
    shape(block, |f| if f.along_x() { block } else { Block::AIR })
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
        let s = shape(Block::STONE_STAIRS.with_facing(Facing::East), none);
        assert_eq!(s.as_slice(), &[b([0, 0, 0], [16, 8, 16]), b([0, 8, 0], [8, 16, 16])]);
        // The tall half's underside sits on the slab, so it's hidden.
        assert!(s.face_hidden(1, 1, false, [0, 16, 0, 8]));
        // Only part of the slab's top is under the tall half.
        assert!(!s.face_hidden(0, 1, true, [0, 16, 0, 16]));
    }

    #[test]
    fn fences_join_fences_gates_and_walls() {
        let lone = shape(Block::OAK_FENCE, none);
        assert_eq!(lone.as_slice(), &[FENCE_POST]);
        let joined = shape(Block::OAK_FENCE, |f| match f {
            Facing::East => Block::OAK_FENCE,
            Facing::West => Block::STONE,
            // A gate running along X doesn't join on its side.
            Facing::South => Block::gate(Facing::South, false),
            Facing::North => Block::gate(Facing::East, false),
        });
        assert_eq!(joined.as_slice().len(), 1 + 3 * 2);
        let tall = collision(Block::OAK_FENCE, none).bounds().unwrap();
        let mixed = |f: Facing| if f == Facing::East { Block::NETHER_BRICK_FENCE } else { Block::AIR };
        assert_eq!(shape(Block::OAK_FENCE, mixed).as_slice().len(), 1, "wood and nether brick don't join");
        let oak = |f: Facing| if f == Facing::East { Block::OAK_FENCE } else { Block::AIR };
        assert_eq!(shape(Block::NETHER_BRICK_FENCE, oak).as_slice().len(), 1, "either way round");
        let nether = |f: Facing| if f == Facing::East { Block::NETHER_BRICK_FENCE } else { Block::AIR };
        assert_eq!(shape(Block::NETHER_BRICK_FENCE, nether).as_slice().len(), 3);
        assert_eq!(tall.max[1], 24);
    }

    #[test]
    fn doors_swing_and_gates_open_up() {
        let closed = shape(Block::door(Facing::South, false, false), none);
        let open = shape(Block::door(Facing::South, true, false), none);
        assert_eq!(closed.as_slice(), &DOOR);
        assert_ne!(open.as_slice(), closed.as_slice());
        assert!(collision(Block::gate(Facing::North, true), none).is_empty());
        assert_eq!(collision(Block::gate(Facing::East, false), none).bounds().unwrap().max[1], 24);
        assert!(shape(Block::STONE, none).is_empty());
    }
}
