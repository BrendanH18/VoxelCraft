//! Stored Java 26.3 `WallBlock.UP` (BLD-04d). Existing dry/wet wall ids
//! mean UP=true, Java's default. 8000–8039 add UP=false, dry then wet, in
//! FAMILIES order. Horizontal NONE/LOW/TALL sides still derive from neighbours.
//! v0.12's walls keep all four of their states (UP dry, UP wet, no UP dry,
//! no UP wet) together in their own module's range ([`super::stone_blocks`]).
use super::{
    World,
    block::{Block, Facing},
    shape,
};
use glam::IVec3;

pub const FIRST: u16 = 8000;
pub const FAMILIES: [Block; 20] = [
    Block(258),
    Block(264),
    Block(270),
    Block(276),
    Block(282),
    Block(288),
    Block(294),
    Block(300),
    Block(306),
    Block(312),
    Block(318),
    Block(324),
    Block(330),
    Block(331),
    Block(332),
    Block(822),
    Block(828),
    Block(834),
    Block(2365),
    Block(2371),
];
pub const END: u16 = FIRST + FAMILIES.len() as u16 * 2;

/// Material twin of a new state, for property, item and registry lookups.
pub const fn original(id: u16) -> Option<Block> {
    if id >= FIRST && id < END {
        return Some(FAMILIES[((id - FIRST) / 2) as usize]);
    }
    match super::stone_blocks::wall_local(id) {
        Some((w, 2 | 3)) => Some(super::stone_blocks::wall(w)),
        _ => None,
    }
}

pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if id >= FIRST && id < END {
        return Some((FIRST + ((id - FIRST) ^ 1), (id - FIRST) % 2 == 1));
    }
    super::stone_blocks::wall_water_pair(id)
}

// One byte lookup keeps non-wall edits and geometry queries cheap. Low
// bits are family+1; bit 7 means UP=false. Water remains in waterlog's table.
static STATES: [u8; super::block::STATE_CAPACITY] = {
    let mut states = [0; super::block::STATE_CAPACITY];
    let mut i = 0;
    while i < FAMILIES.len() {
        let old = FAMILIES[i].0;
        states[old as usize] = i as u8 + 1;
        if let Some((wet, _)) = super::waterlog::family_pair(old) {
            states[wet as usize] = i as u8 + 1;
        }
        states[FIRST as usize + i * 2] = (i as u8 + 1) | 128;
        states[FIRST as usize + i * 2 + 1] = (i as u8 + 1) | 128;
        i += 1;
    }
    let mut w = 0;
    while w < super::stone_blocks::WALL_COUNT {
        let family = (FAMILIES.len() as u16 + w + 1) as u8;
        let first = super::stone_blocks::wall(w).0 as usize;
        states[first] = family;
        states[first + 1] = family;
        states[first + 2] = family | 128;
        states[first + 3] = family | 128;
        w += 1;
    }
    states
};
const _: () = assert!(FAMILIES.len() + (super::stone_blocks::WALL_COUNT as usize) < 128);

/// Stored UP, including old waterlogged partners.
#[inline]
pub fn up(block: Block) -> Option<bool> {
    let state = STATES.get(block.0 as usize).copied().unwrap_or(0);
    (state != 0).then_some(state & 128 == 0)
}

pub fn with_up(block: Block, up: bool) -> Block {
    let state = STATES.get(block.0 as usize).copied().unwrap_or(0);
    if state == 0 {
        return block;
    }
    let i = (state & 127) as usize - 1;
    let wet = block.is_waterlogged();
    if i >= FAMILIES.len() {
        let first = super::stone_blocks::wall((i - FAMILIES.len()) as u16).0;
        return Block(first + (!up as u16) * 2 + wet as u16);
    }
    if up { FAMILIES[i].with_water(wet) } else { Block(FIRST + i as u16 * 2 + wet as u16) }
}

/// `getStateForPlacement` / `updateShape`: compute UP against the actual
/// upper wall's stored UP; no recursive scan of its inferred neighbours.
pub fn reshaped(block: Block, at: impl Fn(IVec3) -> Block) -> Block {
    if up(block).is_none() {
        return block;
    }
    let above = shape::Above::read(block, &at);
    let (_, post) = shape::wall_state(&|f: Facing| at(f.offset()), above);
    with_up(block, post)
}

impl World {
    /// A post change only travels down. Other edits also affect horizontal
    /// neighbours and the bottom face of their blocks above a wall. Bound
    /// the walk by loaded cells/world height, without recursive edit calls.
    pub(super) fn reshape_walls_beside(&mut self, p: IVec3, old: Block, block: Block, sync: bool) {
        if old == block {
            return;
        }
        let starts = [
            IVec3::NEG_Y,
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Z,
            IVec3::NEG_Z,
            IVec3::NEG_Y + IVec3::X,
            IVec3::NEG_Y - IVec3::X,
            IVec3::NEG_Y + IVec3::Z,
            IVec3::NEG_Y - IVec3::Z,
        ];
        for d in starts {
            let mut n = p + d;
            while self.contains_y(n.y) {
                let Some(wall) = self.get_block(n).filter(|&b| up(b).is_some()) else { break };
                let next = reshaped(wall, |d| self.get_block(n + d).unwrap_or(Block::AIR));
                if next == wall {
                    break;
                }
                self.edit_exact_inner(n, next, sync, false);
                n -= IVec3::Y;
            }
        }
    }
}

#[cfg(test)]
mod tests;
