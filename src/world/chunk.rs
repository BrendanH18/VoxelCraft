//! Chunk block storage.
//!
//! Chunks are 32³ cubes. A chunk made of a single block type (open sky, solid
//! rock) is stored as one value instead of 64 KiB, which covers the majority
//! of chunks in a typical world.

use glam::IVec3;

use super::block::Block;

pub const CHUNK_BITS: i32 = 5;
pub const CHUNK_SIZE: usize = 1 << CHUNK_BITS;
pub const CHUNK_SIZE_I: i32 = CHUNK_SIZE as i32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;
/// Lowest block y of any dimension (the Overworld's, as in Java 1.18+).
/// Each dimension has its own bounds: see `terrain::Dimension::min_y`.
pub const WORLD_MIN_Y: i32 = -64;
/// One past the highest block y of any dimension (the Overworld's 319).
pub const WORLD_MAX_Y: i32 = 320;

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
    /// Direct byte IDs let existing terrain expand into meshing rows with SIMD.
    Bytes(Box<[u8; CHUNK_VOLUME]>),
    /// Direct IDs for chunks with more than 256 distinct states.
    Dense(Box<[Block; CHUNK_VOLUME]>),
    /// Byte indices keep ordinary chunks near their original 32 KiB footprint.
    Paletted {
        indices: Box<[u8; CHUNK_VOLUME]>,
        palette: Box<[Block; 256]>,
        len: u16,
    },
}

impl ChunkData {
    pub fn new_dense(fill: Block) -> Box<[Block; CHUNK_VOLUME]> {
        vec![fill; CHUNK_VOLUME].into_boxed_slice().try_into().unwrap()
    }

    /// Chooses uniform, byte-ID, paletted or direct storage without changing state IDs.
    pub fn from_dense(blocks: Box<[Block; CHUNK_VOLUME]>) -> Self {
        let first = blocks[0];
        if blocks.iter().all(|&b| b == first) {
            return ChunkData::Uniform(first);
        }
        // OR reduction vectorizes, unlike an early-exit test of every ID.
        if blocks.iter().fold(0, |ids, b| ids | b.0) <= u8::MAX as u16 {
            let bytes = blocks.iter().map(|b| b.0 as u8).collect::<Vec<_>>().into_boxed_slice().try_into().unwrap();
            return ChunkData::Bytes(bytes);
        }
        let mut palette = Box::new([Block::AIR; 256]);
        let mut indices: Box<[u8; CHUNK_VOLUME]> = vec![0; CHUNK_VOLUME].into_boxed_slice().try_into().unwrap();
        let mut lookup = [u16::MAX; super::block::STATE_CAPACITY];
        let mut len = 0;
        for (&block, idx) in blocks.iter().zip(indices.iter_mut()) {
            let entry = &mut lookup[block.0 as usize];
            if *entry == u16::MAX {
                if len == 256 {
                    return ChunkData::Dense(blocks);
                }
                *entry = len;
                palette[len as usize] = block;
                len += 1;
            }
            *idx = *entry as u8;
        }
        ChunkData::Paletted { indices, palette, len }
    }

    #[inline(always)]
    pub fn get(&self, x: usize, y: usize, z: usize) -> Block {
        match self {
            ChunkData::Uniform(b) => *b,
            ChunkData::Dense(blocks) => blocks[index(x, y, z)],
            ChunkData::Bytes(blocks) => Block(blocks[index(x, y, z)] as u16),
            ChunkData::Paletted { indices, palette, .. } => palette[indices[index(x, y, z)] as usize],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Block) {
        let idx = index(x, y, z);
        if let ChunkData::Uniform(b) = *self {
            if b == block {
                return;
            }
            if b.0 <= u8::MAX as u16 && block.0 <= u8::MAX as u16 {
                let mut blocks: Box<[u8; CHUNK_VOLUME]> =
                    vec![b.0 as u8; CHUNK_VOLUME].into_boxed_slice().try_into().unwrap();
                blocks[idx] = block.0 as u8;
                *self = ChunkData::Bytes(blocks);
                return;
            }
            let mut palette = Box::new([Block::AIR; 256]);
            palette[0] = b;
            palette[1] = block;
            let mut indices: Box<[u8; CHUNK_VOLUME]> = vec![0; CHUNK_VOLUME].into_boxed_slice().try_into().unwrap();
            indices[idx] = 1;
            *self = ChunkData::Paletted { indices, palette, len: 2 };
            return;
        }
        match self {
            ChunkData::Dense(blocks) => blocks[idx] = block,
            ChunkData::Bytes(bytes) => {
                if block.0 <= u8::MAX as u16 {
                    bytes[idx] = block.0 as u8;
                } else {
                    let mut blocks = Self::new_dense(Block::AIR);
                    for (dst, &src) in blocks.iter_mut().zip(bytes.iter()) {
                        *dst = Block(src as u16);
                    }
                    blocks[idx] = block;
                    *self = Self::from_dense(blocks);
                }
            }
            ChunkData::Paletted { indices, palette, len } => {
                let entry = palette[..*len as usize].iter().position(|&b| b == block);
                if let Some(entry) = entry {
                    indices[idx] = entry as u8;
                } else if *len < 256 {
                    palette[*len as usize] = block;
                    indices[idx] = *len as u8;
                    *len += 1;
                } else {
                    let mut blocks = Self::new_dense(Block::AIR);
                    for (dst, &entry) in blocks.iter_mut().zip(indices.iter()) {
                        *dst = palette[entry as usize];
                    }
                    blocks[idx] = block;
                    // Removed states may have left unused entries: rebuild before
                    // falling back to direct storage for >256 live states.
                    *self = Self::from_dense(blocks);
                }
            }
            ChunkData::Uniform(_) => unreachable!(),
        }
    }

    /// Copies a contiguous row into meshing scratch storage without per-cell
    /// enum dispatch. Snapshots continue to share the original Arc unchanged.
    pub fn copy_row(&self, start: usize, dst: &mut [Block]) {
        match self {
            ChunkData::Uniform(block) => dst.fill(*block),
            ChunkData::Dense(blocks) => dst.copy_from_slice(&blocks[start..start + dst.len()]),
            ChunkData::Bytes(blocks) => {
                let end = start + dst.len();
                for (dst, &src) in dst.iter_mut().zip(&blocks[start..end]) {
                    *dst = Block(src as u16);
                }
            }
            ChunkData::Paletted { indices, palette, .. } => {
                let end = start + dst.len();
                for (dst, &entry) in dst.iter_mut().zip(&indices[start..end]) {
                    *dst = palette[entry as usize];
                }
            }
        }
    }

    /// Block storage on the heap, excluding Arc/header overhead.
    pub fn heap_bytes(&self) -> usize {
        match self {
            ChunkData::Uniform(_) => 0,
            ChunkData::Bytes(_) => CHUNK_VOLUME,
            ChunkData::Dense(_) => CHUNK_VOLUME * std::mem::size_of::<Block>(),
            ChunkData::Paletted { .. } => CHUNK_VOLUME + 256 * std::mem::size_of::<Block>(),
        }
    }

    pub fn uniform(&self) -> Option<Block> {
        match self {
            ChunkData::Uniform(b) => Some(*b),
            ChunkData::Bytes(_) | ChunkData::Dense(_) | ChunkData::Paletted { .. } => None,
        }
    }

    /// Iterates all blocks in storage order.
    pub fn for_each_block(&self, mut f: impl FnMut(Block)) {
        match self {
            ChunkData::Uniform(b) => (0..CHUNK_VOLUME).for_each(|_| f(*b)),
            ChunkData::Dense(blocks) => blocks.iter().for_each(|&b| f(b)),
            ChunkData::Bytes(blocks) => blocks.iter().for_each(|&b| f(Block(b as u16))),
            ChunkData::Paletted { indices, palette, .. } => indices.iter().for_each(|&i| f(palette[i as usize])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_chunks_promote_to_a_palette_and_keep_their_snapshot() {
        let mut data = ChunkData::Uniform(Block::STONE);
        data.set(1, 2, 3, Block::GLASS);
        assert!(matches!(data, ChunkData::Bytes(_)));
        assert_eq!(data.heap_bytes(), CHUNK_VOLUME);
        let original = std::sync::Arc::new(data);
        let mut edited = original.clone();
        std::sync::Arc::make_mut(&mut edited).set(4, 5, 6, Block(4095));
        assert!(matches!(&*edited, ChunkData::Paletted { .. }));
        assert_eq!(original.get(4, 5, 6), Block::STONE);
        assert_eq!(edited.get(4, 5, 6), Block(4095));
        assert_eq!(edited.get(1, 2, 3), Block::GLASS);
        let mut row = [Block::AIR; 32];
        original.copy_row(index(0, 2, 3), &mut row);
        assert_eq!(row[1], Block::GLASS);
        assert_eq!(row[31], Block::STONE);
    }

    #[test]
    fn unused_palette_entries_do_not_force_dense_storage() {
        let mut data = ChunkData::Uniform(Block::AIR);
        for id in 256..600 {
            data.set(0, 0, 0, Block(id));
        }
        assert!(matches!(data, ChunkData::Paletted { .. }));
        assert_eq!(data.get(0, 0, 0), Block(599));
        assert_eq!(data.get(1, 0, 0), Block::AIR);
    }

    #[test]
    fn high_states_keep_compact_storage_and_snapshot_isolation() {
        let mut data = ChunkData::Uniform(Block(4095));
        data.set(31, 31, 31, Block(1024));
        assert_eq!(data.heap_bytes(), CHUNK_VOLUME + 512);
        let original = std::sync::Arc::new(data);
        let mut edited = original.clone();
        std::sync::Arc::make_mut(&mut edited).set(31, 31, 31, Block(2048));
        assert_eq!(original.get(31, 31, 31), Block(1024));
        assert_eq!(edited.get(31, 31, 31), Block(2048));
        let mut row = [Block::AIR; CHUNK_SIZE];
        edited.copy_row(index(0, 31, 31), &mut row);
        assert_eq!(row[0], Block(4095));
        assert_eq!(row[31], Block(2048));
    }

    #[test]
    fn palette_overflow_promotes_without_losing_states() {
        let mut data = ChunkData::Uniform(Block::AIR);
        for i in 1..=256 {
            data.set(i % 32, i / 32, 0, Block(256 + i as u16));
        }
        assert!(matches!(data, ChunkData::Dense(_)));
        for i in 1..=256 {
            assert_eq!(data.get(i % 32, i / 32, 0), Block(256 + i as u16));
        }
        let mut blocks = ChunkData::new_dense(Block(4095));
        blocks[0] = Block(256);
        let compact = ChunkData::from_dense(blocks);
        assert!(matches!(compact, ChunkData::Paletted { .. }));
        let mut all = Vec::new();
        compact.for_each_block(|b| all.push(b));
        assert_eq!(all[0], Block(256));
        assert!(all[1..].iter().all(|&b| b == Block(4095)));
    }

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
