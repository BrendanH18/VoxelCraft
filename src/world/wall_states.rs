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
    if let Some(local) = super::creaking::wall_local(id) {
        return if local >= 2 { Some(super::creaking::WALL) } else { None };
    }
    if let Some((w, local)) = super::gen5_blocks::wall_local(id) {
        return if local >= 2 { Some(super::gen5_blocks::wall(w)) } else { None };
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
    if let Some(local) = super::creaking::wall_local(id) {
        return Some((super::creaking::WALL.0 + (local ^ 1), local % 2 == 1));
    }
    if let Some(p) = super::gen5_blocks::wall_water_pair(id) {
        return Some(p);
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
    let family = (FAMILIES.len() + super::stone_blocks::WALL_COUNT as usize + 1) as u8;
    let first = super::creaking::WALL.0 as usize;
    states[first] = family;
    states[first + 1] = family;
    states[first + 2] = family | 128;
    states[first + 3] = family | 128;
    // v0.16 Track D's cinnabar and sulfur walls, after resin's.
    let mut w = 0;
    while w < 6 {
        let family = (FAMILIES.len() + super::stone_blocks::WALL_COUNT as usize + 2 + w) as u8;
        let first = super::gen5_blocks::wall(w as u16).0 as usize;
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
    if super::creaking::wall_local(block.0).is_some() {
        return Block(super::creaking::WALL.0 + (!up as u16) * 2 + wet as u16);
    }
    if let Some((w, _)) = super::gen5_blocks::wall_local(block.0) {
        return Block(super::gen5_blocks::wall(w).0 + (!up as u16) * 2 + wet as u16);
    }
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
    /// v1.0 final F7: walls saved before VoxelCraft stored `UP` (v0.8 and
    /// older, whose ids all load as UP=true, and any chunk an older build
    /// kept in `chunks.bin`) get the post Java's `WallBlock.updateShape`
    /// gives them when their chunk is first read from `chunks.bin`, instead
    /// of waiting for a neighbour to change. The post of a wall under
    /// another wall follows the upper one's, so each is computed from the
    /// top of its stack down (across chunks: an upper chunk's walls are
    /// recomputed the same way, not trusted). A wall a neighbour of which
    /// isn't available (a never-edited chunk not loaded yet) keeps its
    /// stored post and is listed in `pending`, for
    /// [`World::retry_legacy_wall_posts`] once that chunk loads. Consistent
    /// walls compute what they already store, so
    /// walls saved by v0.9 to v0.18 don't change. No id or format changes:
    /// the chunk is written to its region record at the next save.
    pub(super) fn migrate_legacy_wall_posts(
        &self,
        pos: IVec3,
        data: std::sync::Arc<super::chunk::ChunkData>,
        pending: &mut Vec<IVec3>,
    ) -> std::sync::Arc<super::chunk::ChunkData> {
        use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, chunk_of};
        use std::cell::{Cell, RefCell};
        if data.uniform().is_some() {
            return data;
        }
        let cell = |i: usize| (i % CHUNK_SIZE, (i / CHUNK_SIZE) % CHUNK_SIZE, i / (CHUNK_SIZE * CHUNK_SIZE));
        if !(0..CHUNK_VOLUME).any(|i| {
            let (x, y, z) = cell(i);
            up(data.get(x, y, z)).is_some()
        }) {
            return data;
        }
        let base = pos * CHUNK_SIZE_I;
        let others = RefCell::new(rustc_hash::FxHashMap::default());
        let missing = Cell::new(false);
        // The block as saved, or `None` where no chunk is available.
        let raw = |q: IVec3| -> Option<Block> {
            if !self.contains_y(q.y) {
                return Some(Block::AIR);
            }
            let c = chunk_of(q);
            let l = q - c * CHUNK_SIZE_I;
            let at = |d: &super::chunk::ChunkData| d.get(l.x as usize, l.y as usize, l.z as usize);
            if c == pos {
                return Some(at(&data));
            }
            let mut others = others.borrow_mut();
            others.entry(c).or_insert_with(|| self.section_data(c)).as_deref().map(at)
        };
        let known = |q: IVec3| {
            raw(q).unwrap_or_else(|| {
                missing.set(true);
                Block::AIR
            })
        };
        // The recomputed wall at `p`: its stack's posts from the top down.
        let migrated = |p: IVec3, wall: Block| -> (Block, bool) {
            let mut stack = vec![(p, wall)];
            let mut q = p + IVec3::Y;
            while let Some(b) = raw(q).filter(|&b| up(b).is_some()) {
                stack.push((q, b));
                q += IVec3::Y;
            }
            let mut above: Option<(IVec3, Block)> = None;
            let mut out = (wall, false);
            for &(q, b) in stack.iter().rev() {
                missing.set(false);
                let at = |d: IVec3| match above {
                    Some((a, ab)) if a == q + d => ab,
                    _ => known(q + d),
                };
                let next = reshaped(b, at);
                out = if missing.get() { (b, true) } else { (next, false) };
                above = Some((q, out.0));
            }
            out
        };
        let mut out: Option<super::chunk::ChunkData> = None;
        for i in 0..CHUNK_VOLUME {
            let (x, y, z) = cell(i);
            let wall = data.get(x, y, z);
            if up(wall).is_none() {
                continue;
            }
            let p = base + IVec3::new(x as i32, y as i32, z as i32);
            let (next, unknown) = migrated(p, wall);
            if unknown {
                pending.push(p);
            }
            if next != wall {
                out.get_or_insert_with(|| (*data).clone()).set(x, y, z, next);
            }
        }
        out.map_or(data, std::sync::Arc::new)
    }

    /// v1.0 final F9: the `legacy_walls` level file key (and chunk record
    /// entry): the walls still waiting for a neighbouring chunk, as
    /// `x,y,z` joined by `;`, sorted. Written only when some wait, so a
    /// world without them saves as before.
    pub fn legacy_walls_to_string(&self) -> String {
        let mut walls: Vec<IVec3> = self.legacy_walls.iter().copied().collect();
        walls.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        walls.iter().map(|p| format!("{},{},{}", p.x, p.y, p.z)).collect::<Vec<_>>().join(";")
    }

    /// Reads [`World::legacy_walls_to_string`] into the wait list; damaged
    /// entries are skipped.
    pub fn load_legacy_walls(&mut self, text: &str) {
        let one = |e: &str| {
            let mut it = e.split(',').map(|v| v.trim().parse::<i32>());
            let p = IVec3::new(it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
            it.next().is_none().then_some(p)
        };
        self.legacy_walls.extend(text.split(';').filter_map(one));
    }

    /// Whether any legacy wall waits for a neighbouring chunk.
    pub fn has_legacy_walls(&self) -> bool {
        !self.legacy_walls.is_empty()
    }

    /// v1.0 final F7: the legacy walls [`World::migrate_legacy_wall_posts`]
    /// couldn't settle, next to the chunk at `pos` that just loaded, take
    /// their post now, as an ordinary shape update (which walks down a
    /// stack). v1.0 final F9: the list is saved (the `legacy_walls` key,
    /// and the chunk record of a wall's chunk when that unloads), so a
    /// wall still waiting when the game saves or its chunk unloads is
    /// settled when its neighbours load in a later session.
    pub(super) fn retry_legacy_wall_posts(&mut self, pos: IVec3) {
        use super::chunk::chunk_of;
        if self.legacy_walls.is_empty() {
            return;
        }
        let near = |p: IVec3| {
            let d = (chunk_of(p) - pos).abs();
            d.x <= 1 && d.z <= 1 && (0..=1).contains(&(pos.y - chunk_of(p).y))
        };
        let ready: Vec<IVec3> = self.legacy_walls.iter().copied().filter(|&p| near(p)).collect();
        for p in ready {
            let around =
                [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y].into_iter().flat_map(|d| [d, d + IVec3::Y]);
            let unloaded =
                around.chain([IVec3::Y * 2]).any(|d| self.contains_y(p.y + d.y) && self.get_block(p + d).is_none());
            if unloaded {
                continue;
            }
            self.legacy_walls.remove(&p);
            let Some(wall) = self.get_block(p).filter(|&b| up(b).is_some()) else { continue };
            let next = reshaped(wall, |d| self.get_block(p + d).unwrap_or(Block::AIR));
            if next != wall {
                let flags = super::redstone_order::flags::UPDATE_ALL | super::redstone_order::flags::COMPAT;
                self.edit_exact_inner(p, next, false, true, flags);
            }
        }
    }

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
                self.edit_exact_inner(
                    n,
                    next,
                    sync,
                    false,
                    super::redstone_order::flags::UPDATE_ALL | super::redstone_order::flags::COMPAT,
                );
                n -= IVec3::Y;
            }
        }
    }
}

#[cfg(test)]
mod tests;
