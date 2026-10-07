//! Pieces of generated structures (Nether fortresses, strongholds): boxes
//! oriented like Java's `StructurePiece`s, a frame that maps a piece's local
//! coordinates to the world, painting clipped to one chunk, and chest loot.

use glam::IVec3;

use super::block::{Block, Facing};
use super::chest::{Chest, SLOTS};
use super::noise::splitmix64;
use crate::inventory::Stack;
use crate::item::Item;

/// An inclusive box of blocks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bounds {
    pub min: IVec3,
    pub max: IVec3,
}

impl Bounds {
    pub fn intersects(&self, o: &Bounds) -> bool {
        self.min.cmple(o.max).all() && o.min.cmple(self.max).all()
    }

    pub fn contains(&self, p: IVec3) -> bool {
        self.min.cmple(p).all() && p.cmple(self.max).all()
    }

    pub fn union(&self, o: &Bounds) -> Bounds {
        Bounds { min: self.min.min(o.min), max: self.max.max(o.max) }
    }

    pub fn shifted(&self, d: IVec3) -> Bounds {
        Bounds { min: self.min + d, max: self.max + d }
    }

    /// Java's `BoundingBox.orientBox`: a box of `size` grown from the
    /// doorway at `p` in direction `facing`.
    pub fn oriented(p: IVec3, off: IVec3, size: IVec3, facing: Facing) -> Bounds {
        let (w, h, d) = (size.x, size.y, size.z);
        let y = (p.y + off.y, p.y + off.y + h - 1);
        let (min, max) = match facing {
            Facing::North => ((p.x + off.x, p.z - d + 1 + off.z), (p.x + w - 1 + off.x, p.z + off.z)),
            Facing::South => ((p.x + off.x, p.z + off.z), (p.x + w - 1 + off.x, p.z + d - 1 + off.z)),
            Facing::West => ((p.x - d + 1 + off.z, p.z + off.x), (p.x + off.z, p.z + w - 1 + off.x)),
            Facing::East => ((p.x + off.z, p.z + off.x), (p.x + d - 1 + off.z, p.z + w - 1 + off.x)),
        };
        Bounds { min: IVec3::new(min.0, y.0, min.1), max: IVec3::new(max.0, y.1, max.1) }
    }
}

/// A small seeded generator for layouts and loot.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        splitmix64(&mut self.0)
    }

    /// Uniform in `0..n`.
    pub fn below(&mut self, n: u32) -> u32 {
        (((self.next_u64() >> 32) * n as u64) >> 32) as u32
    }

    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.below(hi - lo + 1)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A piece with a facing and a box: Java's local frame, where x runs
/// across, y up and z forward from the doorway it was entered by.
pub trait Oriented {
    fn facing(&self) -> Facing;
    fn bounds(&self) -> &Bounds;

    /// World position of local `(x, y, z)`: x across, y up, z forward.
    fn world(&self, x: i32, y: i32, z: i32) -> IVec3 {
        let b = self.bounds();
        let y = b.min.y + y;
        match self.facing() {
            Facing::South => IVec3::new(b.min.x + x, y, b.min.z + z),
            Facing::North => IVec3::new(b.min.x + x, y, b.max.z - z),
            Facing::West => IVec3::new(b.max.x - z, y, b.min.z + x),
            Facing::East => IVec3::new(b.min.x + z, y, b.min.z + x),
        }
    }

    /// The world direction of a local one (local south is forward, +z;
    /// local east is +x).
    fn turn(&self, local: Facing) -> Facing {
        let forward = self.facing();
        let across = if matches!(forward, Facing::South | Facing::North) { Facing::East } else { Facing::South };
        match local {
            Facing::South => forward,
            Facing::North => forward.opposite(),
            Facing::East => across,
            Facing::West => across.opposite(),
        }
    }

    /// How long the piece is along its facing.
    fn length(&self) -> i32 {
        let s = self.bounds().max - self.bounds().min;
        1 + if matches!(self.facing(), Facing::South | Facing::North) { s.z } else { s.x }
    }

    /// Doorway of a child grown straight ahead, `off_x` across and `off_y` up.
    fn ahead(&self, off_x: i32, off_y: i32) -> (IVec3, Facing) {
        let (b, y) = (self.bounds(), self.bounds().min.y + off_y);
        let p = match self.facing() {
            Facing::South => IVec3::new(b.min.x + off_x, y, b.max.z + 1),
            Facing::North => IVec3::new(b.min.x + off_x, y, b.min.z - 1),
            Facing::West => IVec3::new(b.min.x - 1, y, b.min.z + off_x),
            Facing::East => IVec3::new(b.max.x + 1, y, b.min.z + off_x),
        };
        (p, self.facing())
    }

    /// Doorway of a child grown out of the local west (`left`) or east
    /// side, `off_z` along the piece. Java gives these offsets in world
    /// terms; mirroring them for north and west facings keeps doorways
    /// where the piece's walls leave a gap whichever way it runs.
    fn side(&self, left: bool, off_y: i32, off_z: i32) -> (IVec3, Facing) {
        let (b, y) = (self.bounds(), self.bounds().min.y + off_y);
        let off = if matches!(self.facing(), Facing::North | Facing::West) { self.length() - 3 - off_z } else { off_z };
        let p = match (self.facing(), left) {
            (Facing::South | Facing::North, true) => IVec3::new(b.min.x - 1, y, b.min.z + off),
            (Facing::South | Facing::North, false) => IVec3::new(b.max.x + 1, y, b.min.z + off),
            (_, true) => IVec3::new(b.min.x + off, y, b.min.z - 1),
            (_, false) => IVec3::new(b.min.x + off, y, b.max.z + 1),
        };
        (p, self.turn(if left { Facing::West } else { Facing::East }))
    }
}

/// Writes one piece into the region from `base` to `top`, inclusive.
/// The buffer uses X-fastest, then Z, then Y order (like chunk storage).
pub struct Paint<'a, P> {
    pub blocks: &'a mut [Block],
    pub base: IVec3,
    pub top: IVec3,
    pub piece: &'a P,
    pub open: &'a dyn Fn(i32, i32, i32, i32) -> bool,
    /// What `column_down` builds pillars of.
    pub pillar: Block,
}

impl<P: Oriented> Paint<'_, P> {
    #[inline]
    fn index(&self, local: IVec3) -> usize {
        let size = self.top - self.base + IVec3::ONE;
        ((local.y * size.z + local.z) * size.x + local.x) as usize
    }

    /// Index of piece-local `(x, y, z)` if it lies in this clipped region.
    pub fn cell(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let p = self.piece.world(x, y, z);
        let inside = p.cmpge(self.base).all() && p.cmple(self.top).all();
        inside.then(|| {
            let l = p - self.base;
            self.index(l)
        })
    }

    /// Fills the local box from `(x0, y0, z0)` to `(x1, y1, z1)`, with the
    /// argument order of Java's `generateBox` so pieces read like it.
    #[allow(clippy::too_many_arguments)]
    pub fn fill(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, block: Block) {
        let (a, b) = (self.piece.world(x0, y0, z0), self.piece.world(x1, y1, z1));
        let lo = a.min(b).max(self.base);
        let hi = a.max(b).min(self.top);
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let l = IVec3::new(x, y, z) - self.base;
                    self.blocks[self.index(l)] = block;
                }
            }
        }
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, block: Block) {
        self.fill(x, y, z, x, y, z, block);
    }

    /// Java's `fillColumnDown`: [`Paint::pillar`] blocks from local
    /// `(x, y, z)` down through air and liquid until they meet the ground
    /// (stopping above y = 1).
    pub fn column_down(&mut self, x: i32, y: i32, z: i32) {
        let p = self.piece.world(x, y, z);
        if p.x < self.base.x || p.x > self.top.x || p.z < self.base.z || p.z > self.top.z || p.y < self.base.y {
            return;
        }
        let mut wy = p.y;
        if wy > self.top.y {
            if !(self.open)(p.x, p.z, wy, self.top.y + 1) {
                return;
            }
            wy = self.top.y;
        }
        let l = p - self.base;
        while wy > 1 && wy >= self.base.y {
            let i = self.index(IVec3::new(l.x, wy - self.base.y, l.z));
            let b = self.blocks[i];
            if b != Block::AIR && !b.is_fluid() {
                return;
            }
            self.blocks[i] = self.pillar;
            wy -= 1;
        }
    }

    pub fn columns_down(&mut self, x0: i32, z0: i32, x1: i32, z1: i32) {
        for x in x0..=x1 {
            for z in z0..=z1 {
                self.column_down(x, -1, z);
            }
        }
    }

    /// Stairs of `material` climbing toward local `up`.
    pub fn stairs(&self, material: Block, up: Facing) -> Block {
        Block::stairs_of(material).unwrap().with_facing(self.piece.turn(up).opposite())
    }

    /// A block turned to face local `facing` (furnaces, chests, frames).
    pub fn facing(&self, block: Block, facing: Facing) -> Block {
        block.with_facing(self.piece.turn(facing))
    }
}

/// A loot table entry: item, weight, and fewest and most of it.
pub type LootEntry = (Item, u32, u32, u32);

/// A chest filled by `rolls` (fewest, most) weighted picks from `table`,
/// each into a random empty slot like Java's, seeded by `seed`.
pub fn fill_chest(seed: u64, rolls: (u32, u32), table: &[LootEntry]) -> Chest {
    let total: u32 = table.iter().map(|e| e.1).sum();
    let mut rng = Rng(seed ^ 0x6C6F_6F74);
    let mut chest = Chest::default();
    for _ in 0..rng.range(rolls.0, rolls.1) {
        let mut r = rng.below(total);
        let &(item, _, lo, hi) = table
            .iter()
            .find(|e| {
                let hit = r < e.1;
                r = r.saturating_sub(e.1);
                hit
            })
            .unwrap();
        let count = rng.range(lo, hi) as u8;
        // Java scatters loot into random empty slots.
        let empty: Vec<usize> = (0..SLOTS).filter(|&i| chest.slots[i].is_none()).collect();
        if let Some(&slot) = empty.get(rng.below(empty.len() as u32) as usize) {
            chest.slots[slot] = Some(Stack::new(item, count));
        }
    }
    chest
}
