//! The colour blocks' neighbour rules and the loaded-banner index.

use glam::IVec3;

use super::super::World;
use super::super::block::{Block, Facing};
use super::super::chunk::{CHUNK_SIZE, ChunkData};
use super::{BANNER_ORIGIN, WALL_BANNER_ORIGIN, banner, candle, straw_bed};

/// Whether `b` is a standing or wall banner.
fn is_banner(b: Block) -> bool {
    (BANNER_ORIGIN..WALL_BANNER_ORIGIN + 64).contains(&b.0)
}

/// Whether a chunk can hold a banner, without visiting every cell when
/// its storage says it can't (byte storage holds only ids below 256).
fn may_hold_banner(data: &ChunkData) -> bool {
    match data {
        ChunkData::Uniform(b) => is_banner(*b),
        ChunkData::Bytes(_) => false,
        ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|&b| is_banner(b)),
        ChunkData::Dense(_) => true,
    }
}

/// `CandleBlock.placeLiquid`: a candle that takes on water goes out, and a
/// waterlogged candle can't burn.
pub(in super::super) fn settled_state(b: Block) -> Block {
    match candle::decode(b.0) {
        Some(c) if c.lit && c.wet => candle::encode(candle::Candle { lit: false, ..c }),
        _ => b,
    }
}

impl World {
    /// Straw beds someone lies in: the head and where they lie.
    pub fn straw_sleepers_mut(&mut self) -> &mut Vec<(IVec3, glam::DVec3)> {
        &mut self.straw_sleepers
    }

    /// Banners in loaded chunks, for the renderer: their cell and state.
    pub fn banners(&self) -> impl Iterator<Item = (IVec3, Block)> + '_ {
        self.banners.iter().map(|(&p, &b)| (p, b))
    }

    /// Indexes the banners of a chunk being loaded.
    pub(in super::super) fn register_banners(&mut self, cpos: IVec3, data: &ChunkData) {
        if !may_hold_banner(data) {
            return;
        }
        let base = cpos * CHUNK_SIZE as i32;
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let b = data.get(x, y, z);
                    if is_banner(b) {
                        self.banners.insert(base + IVec3::new(x as i32, y as i32, z as i32), b);
                    }
                }
            }
        }
    }

    /// Keeps the banner index in step with a block change at `p`.
    pub(in super::super) fn track_banner(&mut self, p: IVec3, old: Block, new: Block) {
        if is_banner(new) {
            self.banners.insert(p, new);
        } else if is_banner(old) {
            self.banners.remove(&p);
        }
    }

    /// Java's `updateShape` for the colour blocks beside `p`: a wall banner
    /// whose wall is gone and a straw bed half whose other half is gone
    /// break (dropping what they drop). Blocks resting on `p` are handled
    /// by `can_stay_on` in [`World::settle`].
    pub(in super::super) fn drop_unhung_colour_blocks(&mut self, p: IVec3) {
        let here = self.get_block(p);
        for f in Facing::ALL {
            let at = p + f.offset();
            let Some(b) = self.get_block(at) else { continue };
            let breaks = if let Some(bn) = banner::decode(b.0) {
                banner::support(bn, at) == p && !here.is_some_and(banner::holds)
            } else if let Some((facing, head)) = straw_bed::decode(b.0) {
                at + straw_bed::partner_offset(facing, head) == p && here != Some(straw_bed::half(facing, !head))
            } else {
                false
            };
            if breaks {
                self.edit(at, Block::AIR, true);
                self.spill_block(at, b);
            }
        }
    }
}
