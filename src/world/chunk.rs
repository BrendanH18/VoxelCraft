//! Chunk block storage.
//!
//! Chunks are 32³ cubes. A chunk made of a single block type (open sky, solid
//! rock) is stored as one value instead of 32 KiB, which covers the majority
//! of chunks in a typical world.

use glam::IVec3;

use super::block::Block;

pub const CHUNK_BITS: i32 = 5;
pub const CHUNK_SIZE: usize = 1 << CHUNK_BITS;
pub const CHUNK_SIZE_I: i32 = CHUNK_SIZE as i32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
/// World height in chunks (256 blocks).
pub const WORLD_HEIGHT_CHUNKS: i32 = 8;
pub const WORLD_HEIGHT: i32 = WORLD_HEIGHT_CHUNKS * CHUNK_SIZE_I;

/// Linear index of a local block coordinate. X is fastest, then Z, then Y.
#[inline(always)]
pub fn index(x: usize, y: usize, z: usize) -> usize {
    x | (z << CHUNK_BITS) | (y << (2 * CHUNK_BITS))
}

#[inline(always)]
pub fn chunk_of(block: IVec3) -> IVec3 {
    block >> CHUNK_BITS
}

#[inline(always)]
pub fn local_of(block: IVec3) -> IVec3 {
    block & (CHUNK_SIZE_I - 1)
}

#[derive(Clone)]
pub enum ChunkData {
    Uniform(Block),
    Dense(Box<[Block; CHUNK_VOLUME]>),
}

impl ChunkData {
    pub fn new_dense(fill: Block) -> Box<[Block; CHUNK_VOLUME]> {
        vec![fill; CHUNK_VOLUME].into_boxed_slice().try_into().unwrap()
    }

    /// Wraps dense storage, collapsing it to `Uniform` when every block matches.
    pub fn from_dense(blocks: Box<[Block; CHUNK_VOLUME]>) -> Self {
        let first = blocks[0];
        if blocks.iter().all(|&b| b == first) {
            ChunkData::Uniform(first)
        } else {
            ChunkData::Dense(blocks)
        }
    }

    #[inline(always)]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        match self {
            ChunkData::Uniform(b) => *b,
            ChunkData::Dense(blocks) => blocks[index(x, y, z)],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        if let ChunkData::Uniform(b) = *self {
            if b == block {
                return;
            }
            *self = ChunkData::Dense(Self::new_dense(b));
        }
        if let ChunkData::Dense(blocks) = self {
            blocks[index(x, y, z)] = block;
        }
    }

    pub fn uniform(&self) -> Option<Block> {
        match self {
            ChunkData::Uniform(b) => Some(*b),
            ChunkData::Dense(_) => None,
        }
    }

    /// Iterates all blocks in storage order.
    pub fn for_each_block(&self, mut f: impl FnMut(Block)) {
        match self {
            ChunkData::Uniform(b) => (0..CHUNK_VOLUME).for_each(|_| f(*b)),
            ChunkData::Dense(blocks) => blocks.iter().for_each(|&b| f(b)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_promotes_to_dense_on_write() {
        let mut c = ChunkData::Uniform(Block::AIR);
        c.set(1, 2, 3, Block::AIR);
        assert!(c.uniform().is_some());
        c.set(1, 2, 3, Block::STONE);
        assert!(c.uniform().is_none());
        assert_eq!(c.get(1, 2, 3), Block::STONE);
        assert_eq!(c.get(0, 0, 0), Block::AIR);
    }

    #[test]
    fn negative_coordinates_map_to_chunks() {
        assert_eq!(chunk_of(IVec3::new(-1, 0, 32)), IVec3::new(-1, 0, 1));
        assert_eq!(local_of(IVec3::new(-1, 0, 32)), IVec3::new(31, 0, 0));
    }
}
