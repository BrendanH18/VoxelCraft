//! Bubble columns (ENV-01a): the water above soul sand rises, the water above
//! magma blocks drags down.
//!
//! Two block states hold a column cell: [`UP`] (the `drag` property false)
//! and [`DOWN`] (true), ids 10112 and 10113 of the v0.13 track-5 range. Both
//! are a still water source in every other respect: `Block::is_water`,
//! `fluid` and `fluid_level` see a source, the flow simulation reads them as
//! `Block::WATER` (`World::cell`), buckets and sponges take them as water,
//! and removing them over a block that holds water leaves a source.
//!
//! A column starts in a water source directly above soul sand or a magma
//! block and runs up through every water source (or column cell) above it.
//! Any change to a cell queues it and the cell above; the queue is worked
//! once per fluid tick (0.25 s, Java's five-tick delay), re-deriving the whole
//! chain from its base so a broken base turns the column back into water.
//!
//! Entities inside a column are pushed along `y` by [`push`]; the caps are the
//! familiar ones (up 0.7 inside and 1.8 at the surface, down 0.3 inside and
//! 0.9 at the surface, in blocks per tick).

use glam::{DVec3, IVec3};

use super::World;
use super::block::{Block, RenderKind, tex};

/// First id of the track's reserved block states (10112–10239).
pub const FIRST: u16 = 10112;
/// A column that lifts entities (soul sand below).
pub const UP: Block = Block(FIRST);
/// A column that pulls entities down (magma block below).
pub const DOWN: Block = Block(FIRST + 1);
pub const LAST: u16 = FIRST + 1;

#[inline]
pub const fn is_column(b: Block) -> bool {
    b.0 == FIRST || b.0 == FIRST + 1
}

/// `drag`: the column pulls down.
#[inline]
pub const fn drags(b: Block) -> bool {
    b.0 == FIRST + 1
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    match id {
        FIRST => Some(("bubble column", RenderKind::Translucent, [tex::BUBBLE_UP; 6])),
        id if id == FIRST + 1 => Some(("bubble column", RenderKind::Translucent, [tex::BUBBLE_DOWN; 6])),
        _ => None,
    }
}

/// The column a block below starts, if any (`BubbleColumnBlock.getColumnState`).
pub fn column_above(below: Block) -> Option<Block> {
    match below {
        Block::SOUL_SAND => Some(UP),
        Block::MAGMA => Some(DOWN),
        b if is_column(b) => Some(b),
        _ => None,
    }
}

/// `canExistIn`: a still water source or a column cell.
#[inline]
pub fn can_exist_in(b: Block) -> bool {
    b == Block::WATER || is_column(b)
}

/// The vertical speed (blocks per tick) after a column cell acts on an
/// entity moving at `dy`. `surface` means the cell above is air.
pub fn push(dy: f64, down: bool, surface: bool) -> f64 {
    match (down, surface) {
        (true, true) => (dy - 0.03).max(-0.9),
        (false, true) => (dy + 0.1).min(1.8),
        (true, false) => (dy - 0.03).max(-0.3),
        (false, false) => (dy + 0.06).min(0.7),
    }
}

/// Applies every column cell the box `min..max` overlaps to the vertical
/// speed `dy` (blocks per tick). Returns the new speed and whether the box
/// touched a column; entities that touch one also reset their fall distance.
pub fn apply(get: impl Fn(IVec3) -> Option<Block>, min: DVec3, max: DVec3, mut dy: f64) -> Option<f64> {
    let eps = DVec3::splat(1.0e-5);
    let lo = (min + eps).floor().as_ivec3();
    let hi = (max - eps).floor().as_ivec3();
    let mut touched = false;
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let p = IVec3::new(x, y, z);
                let Some(b) = get(p).filter(|&b| is_column(b)) else { continue };
                touched = true;
                let surface = get(p + IVec3::Y) == Some(Block::AIR);
                dy = push(dy, drags(b), surface);
            }
        }
    }
    touched.then_some(dy)
}

/// The cells of a generated chunk (its low corner `origin`) that hold water
/// directly above soul sand or a magma block; they get columns on the first
/// fluid tick. The scan skips chunks that hold neither base.
pub fn seeds(data: &super::chunk::ChunkData, origin: IVec3) -> Vec<IVec3> {
    use super::chunk::{CHUNK_SIZE, ChunkData};
    let base = |b: Block| b == Block::SOUL_SAND || b == Block::MAGMA;
    let may_hold = match data {
        ChunkData::Uniform(_) => false,
        ChunkData::Bytes(bytes) => bytes.contains(&(Block::SOUL_SAND.0 as u8)),
        ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|&b| base(b)),
        ChunkData::Dense(_) => true,
    };
    let mut out = Vec::new();
    if !may_hold {
        return out;
    }
    for y in 0..CHUNK_SIZE - 1 {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if base(data.get(x, y, z)) && data.get(x, y + 1, z) == Block::WATER {
                    out.push(origin + IVec3::new(x as i32, y as i32 + 1, z as i32));
                }
            }
        }
    }
    out
}

impl World {
    /// A cell changed from `old` to `new`: queue the column work it implies.
    pub(super) fn bubble_changed(&mut self, p: IVec3, old: Block, new: Block) {
        let relevant = |b: Block| b == Block::SOUL_SAND || b == Block::MAGMA || can_exist_in(b);
        if relevant(old) || relevant(new) {
            self.fluids.bubbles.insert(p);
            self.fluids.bubbles.insert(p + IVec3::Y);
        }
    }

    /// Works the queued cells (see the module docs).
    pub(super) fn tick_bubbles(&mut self) {
        if self.fluids.bubbles.is_empty() {
            return;
        }
        let activity = &self.activity;
        let chunks = &self.chunks;
        let batch: Vec<IVec3> = self
            .fluids
            .bubbles
            .iter()
            .filter(|&&p| activity.contains(p, true) && chunks.contains_key(&super::chunk::chunk_of(p)))
            .copied()
            .collect();
        for p in &batch {
            self.fluids.bubbles.remove(p);
        }
        for p in batch {
            self.update_column(p);
        }
    }

    /// Re-derives the chain of water sources and column cells that starts at
    /// `start` from the block below it.
    fn update_column(&mut self, start: IVec3) {
        if !self.get_block(start).is_some_and(can_exist_in) {
            return;
        }
        let Some(below) = self.get_block(start - IVec3::Y) else { return };
        let want = column_above(below).unwrap_or(Block::WATER);
        let mut p = start;
        while let Some(b) = self.get_block(p).filter(|&b| can_exist_in(b)) {
            if b != want {
                self.edit_exact(p, want, false);
            }
            p += IVec3::Y;
        }
    }
}

#[cfg(test)]
mod tests;
