//! Authoritative block light and skylight, independent of render meshes.
//! Increasing light floods outward; removal clears dependent paths before
//! relighting from surviving sources. Only lit chunks allocate packed nibble
//! arrays, one per layer.
//!
//! Skylight follows Java's sky light engine: every cell above a column's
//! highest light-dampening block is a source at level 15, and light spreads
//! from there losing `max(1, dampening)` per step, like block light. Sources
//! are implied by the column heightmap rather than stored. A column is lit
//! only once all of its chunks are loaded, so its heightmap is complete, and
//! [`World::sky_light`] reports unknown light until the surrounding 3x3
//! columns are lit: values beside unloaded terrain are never guessed.

use glam::{IVec2, IVec3};

use super::block::Block;
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, chunk_of, index, local_of};
use super::{ChunkSlot, World, column_of};
use crate::mesh::NO_HEIGHT;

const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
const HORIZONTAL: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];
const MAX_LIGHT: u8 = 15;

/// Packed light levels of one chunk, for either layer.
#[derive(Clone, Debug)]
pub(crate) struct BlockLight {
    cells: Box<[u8; CHUNK_VOLUME / 2]>,
    lit: usize,
}

impl BlockLight {
    /// Allocate a dark packed array; the owning chunk drops it again when no cells remain lit.
    fn new() -> Self {
        Self { cells: Box::new([0; CHUNK_VOLUME / 2]), lit: 0 }
    }

    /// Read a light level in 0..=15 at chunk-local coordinates in 0..32.
    pub(crate) fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        self.at(index(x, y, z))
    }

    /// Read a light level in 0..=15 at a chunk-linear index.
    fn at(&self, i: usize) -> u8 {
        self.cells[i / 2] >> (i % 2 * 4) & 15
    }

    /// Write a light level in 0..=15 at a chunk-linear index, maintaining the count of lit cells.
    fn set(&mut self, i: usize, level: u8) {
        let shift = i % 2 * 4;
        let old = self.cells[i / 2] >> shift & 15;
        self.lit = self.lit + usize::from(level != 0) - usize::from(old != 0);
        self.cells[i / 2] = (self.cells[i / 2] & !(15 << shift)) | level << shift;
    }

    /// Construct packed mesh-reference light for tests, counting its nonzero cells.
    #[cfg(test)]
    pub(crate) fn from_nibbles(cells: Box<[u8; CHUNK_VOLUME / 2]>) -> Self {
        let lit = cells.iter().map(|v| usize::from(v & 15 != 0) + usize::from(v >> 4 != 0)).sum();
        Self { cells, lit }
    }

    /// Initialize light from the chunk's own emitters, allocating nothing for source-free chunks.
    fn emitters(data: &ChunkData) -> Option<Self> {
        if let Some(b) = data.uniform() {
            let e = b.emission();
            return (e > 0).then(|| Self { cells: Box::new([e | e << 4; CHUNK_VOLUME / 2]), lit: CHUNK_VOLUME });
        }
        let mut light = None;
        let mut i = 0usize;
        data.for_each_block(|b| {
            let e = b.emission();
            if e > 0 {
                light.get_or_insert_with(Self::new).set(i, e);
            }
            i += 1;
        });
        light
    }
}

/// Which light a queue or query works on: Java's `LightLayer.BLOCK` or `SKY`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Layer {
    Block,
    Sky,
}

#[derive(Default)]
pub(super) struct LightUpdates {
    remove: Vec<(IVec3, u8)>,
    increase: Vec<IVec3>,
    /// Most entries both queues held at once since the last [`World::take_light_peak`].
    peak: usize,
}

impl LightUpdates {
    /// Whether all pending light removals and increases have been resolved.
    pub fn is_empty(&self) -> bool {
        self.remove.is_empty() && self.increase.is_empty()
    }

    fn note_peak(&mut self) {
        self.peak = self.peak.max(self.remove.len() + self.increase.len());
    }
}

impl World {
    fn updates(&mut self, layer: Layer) -> &mut LightUpdates {
        match layer {
            Layer::Block => &mut self.light_updates,
            Layer::Sky => &mut self.sky_updates,
        }
    }

    /// The largest number of queued block and sky light entries since the
    /// previous call: evidence that edits and streaming stay bounded.
    pub fn take_light_peak(&mut self) -> usize {
        let peak = self.light_updates.peak.max(self.sky_updates.peak);
        self.light_updates.peak = 0;
        self.sky_updates.peak = 0;
        peak
    }

    /// Raw heightmap entry (highest light-dampening block, or `NO_HEIGHT`)
    /// of a column whose skylight has been seeded; `None` otherwise.
    fn lit_height(&self, p: IVec3) -> Option<i32> {
        let col = self.columns.get(&IVec2::new(p.x >> 5, p.z >> 5)).filter(|c| c.sky_lit)?;
        let l = local_of(p);
        Some(col.heights[(l.x + l.z * CHUNK_SIZE_I) as usize] as i32)
    }

    /// A cell of a lit column above its heightmap: an implied level-15 skylight source.
    fn sky_source(&self, p: IVec3) -> bool {
        self.lit_height(p).is_some_and(|h| p.y > h)
    }

    fn layer_light(slot: &ChunkSlot, layer: Layer) -> Option<&BlockLight> {
        match layer {
            Layer::Block => slot.block_light.as_ref(),
            Layer::Sky => slot.sky_light.as_ref(),
        }
    }

    /// Read stored light without borrowing from adjacent cells; unloaded or unlit cells return zero.
    fn stored(&self, layer: Layer, p: IVec3) -> u8 {
        let l = local_of(p);
        self.chunks
            .get(&chunk_of(p))
            .and_then(|s| Self::layer_light(s, layer))
            .map_or(0, |light| light.get(l.x as usize, l.y as usize, l.z as usize))
    }

    fn raw_block_light(&self, p: IVec3) -> u8 {
        self.stored(Layer::Block, p)
    }

    /// A cell's current level in a layer, counting implied sky sources.
    fn level(&self, layer: Layer, p: IVec3) -> u8 {
        match layer {
            Layer::Block => self.stored(Layer::Block, p),
            Layer::Sky => match self.lit_height(p) {
                Some(h) if p.y > h => MAX_LIGHT,
                Some(_) => self.stored(Layer::Sky, p),
                None => 0,
            },
        }
    }

    /// A lightable cell's block and current level in one lookup per map.
    // Physics treats space above the world as air. Lighting needs actual
    // storage, so it must never enqueue that virtual air or unloaded cells;
    // skylight also stays out of columns that are not lit yet.
    fn light_cell(&self, layer: Layer, p: IVec3) -> Option<(Block, u8)> {
        let height = match layer {
            Layer::Block => None,
            Layer::Sky => Some(self.lit_height(p)?),
        };
        let slot = self.chunks.get(&chunk_of(p))?;
        let l = local_of(p);
        let i = index(l.x as usize, l.y as usize, l.z as usize);
        let b = slot.data.get(l.x as usize, l.y as usize, l.z as usize);
        let level = match height {
            Some(h) if p.y > h => MAX_LIGHT,
            _ => Self::layer_light(slot, layer).map_or(0, |light| light.at(i)),
        };
        Some((b, level))
    }

    /// Neighbours of `p` that `level` would brighten, with their new levels.
    /// Neighbours in the same chunk reuse its storage and column heightmap
    /// instead of looking them up again.
    fn flood_from(&self, layer: Layer, p: IVec3, level: u8, out: &mut [(IVec3, u8); 6]) -> usize {
        let home = chunk_of(p);
        let (Some(slot), col) = (self.chunks.get(&home), self.columns.get(&IVec2::new(home.x, home.z))) else {
            return 0;
        };
        let mut found = 0;
        for d in SIDES {
            let n = p + d;
            let (b, old) = if chunk_of(n) == home {
                let l = local_of(n);
                let b = slot.data.get(l.x as usize, l.y as usize, l.z as usize);
                let source = layer == Layer::Sky
                    && col.is_some_and(|c| n.y > c.heights[(l.x + l.z * CHUNK_SIZE_I) as usize] as i32);
                let stored = || {
                    Self::layer_light(slot, layer)
                        .map_or(0, |light| light.at(index(l.x as usize, l.y as usize, l.z as usize)))
                };
                (b, if source { MAX_LIGHT } else { stored() })
            } else {
                let Some(cell) = self.light_cell(layer, n) else { continue };
                cell
            };
            if b.light_opacity() >= 15 {
                continue;
            }
            let next = level.saturating_sub(b.light_opacity().max(1));
            if next > old {
                out[found] = (n, next);
                found += 1;
            }
        }
        found
    }

    /// The level a cell holds on its own: block emission or an implied sky source.
    fn source_level(&self, layer: Layer, p: IVec3) -> u8 {
        match layer {
            Layer::Block => self.get_block(p).map_or(0, Block::emission),
            Layer::Sky if self.sky_source(p) => MAX_LIGHT,
            Layer::Sky => 0,
        }
    }

    /// Update a loaded cell, allocating on first light and releasing storage
    /// when its chunk becomes dark. Implied sky sources are never stored.
    fn store(&mut self, layer: Layer, p: IVec3, level: u8) {
        if layer == Layer::Sky && level != 0 && self.sky_source(p) {
            return;
        }
        self.store_raw(layer, p, level);
    }

    /// [`World::store`] for a cell known not to be an implied sky source.
    fn store_raw(&mut self, layer: Layer, p: IVec3, level: u8) {
        let Some(slot) = self.chunks.get_mut(&chunk_of(p)) else { return };
        let light = match layer {
            Layer::Block => &mut slot.block_light,
            Layer::Sky => &mut slot.sky_light,
        };
        if level == 0 && light.is_none() {
            return;
        }
        let l = local_of(p);
        let packed = light.get_or_insert_with(BlockLight::new);
        packed.set(index(l.x as usize, l.y as usize, l.z as usize), level);
        if packed.lit == 0 {
            *light = None;
        }
    }

    /// Current gameplay block light (0..=15). Unloaded cells are dark.
    /// Shaped opaque cells borrow adjacent light for entity/hand rendering,
    /// but those borrowed values never propagate through the solid block.
    pub fn block_light(&self, p: IVec3) -> u8 {
        if self.get_block(p).is_some_and(Block::borrows_light) {
            [IVec3::Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                .into_iter()
                .map(|d| self.raw_block_light(p + d))
                .max()
                .unwrap_or(0)
        } else {
            self.raw_block_light(p)
        }
    }

    /// Gameplay skylight (0..=15), Java's `LightLayer.SKY` level before the
    /// time of day darkens it. The Nether has none. `None` where it cannot be
    /// exact: cells whose column or a
    /// horizontal neighbour column is not fully loaded and lit.
    /// Shaped opaque cells borrow adjacent light like [`World::block_light`].
    pub fn sky_light(&self, p: IVec3) -> Option<u8> {
        use super::terrain::Dimension;
        match self.generator.dimension {
            Dimension::Nether => return Some(0),
            Dimension::Overworld | Dimension::End => {}
        }
        let key = IVec2::new(p.x >> 5, p.z >> 5);
        let lit = (-1..=1)
            .all(|dz| (-1..=1).all(|dx| self.columns.get(&(key + IVec2::new(dx, dz))).is_some_and(|c| c.sky_lit)));
        if !lit {
            return None;
        }
        if p.y >= self.max_y() {
            return Some(MAX_LIGHT);
        }
        if self.get_block(p).is_some_and(Block::borrows_light) {
            return [IVec3::Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                .into_iter()
                .map(|d| self.level(Layer::Sky, p + d))
                .max();
        }
        Some(self.level(Layer::Sky, p))
    }

    /// Java's `getMaxLocalRawBrightness(pos, darken)`: the brighter of the
    /// darkened skylight and block light, `None` where skylight is unknown.
    pub fn raw_brightness(&self, p: IVec3, sky_darken: u8) -> Option<u8> {
        Some(self.sky_light(p)?.saturating_sub(sky_darken).max(self.block_light(p)))
    }

    /// Queue removal and relighting after an edit changes emission or opacity.
    /// `old_level` is the cell's stored light before the edit.
    pub(super) fn light_block_changed(&mut self, p: IVec3, old: Block, new: Block, old_level: u8) {
        if old.emission() == new.emission() && old.light_opacity() == new.light_opacity() {
            return;
        }
        let emission = new.emission();
        self.store(Layer::Block, p, emission);
        if old_level > emission {
            self.light_updates.remove.push((p, old_level));
        }
        self.light_updates.increase.push(p);
        for d in SIDES {
            if self.raw_block_light(p + d) > 1 {
                self.light_updates.increase.push(p + d);
            }
        }
    }

    /// Queue skylight changes after an edit in a lit column. `old_height` and
    /// `new_height` are the column's raw heightmap entries around the edit:
    /// cells that stop being sources lose their implied level, cells that
    /// become sources start spreading it, and an opacity change beneath the
    /// heightmap relights like block light.
    pub(super) fn sky_block_changed(&mut self, p: IVec3, old: Block, new: Block, old_height: i32, new_height: i32) {
        if self.lit_height(p).is_none() {
            return;
        }
        let min_y = self.min_y();
        if new_height > old_height {
            for y in (old_height + 1).max(min_y)..=new_height {
                let q = p.with_y(y);
                self.sky_updates.remove.push((q, MAX_LIGHT));
                self.sky_updates.increase.push(q);
            }
        } else if new_height < old_height {
            for y in (new_height + 1).max(min_y)..=old_height {
                let q = p.with_y(y);
                self.clear_sky(q);
                self.sky_updates.increase.push(q);
            }
        } else if p.y <= new_height && old.light_opacity() != new.light_opacity() {
            let old_level = self.stored(Layer::Sky, p);
            self.clear_sky(p);
            if old_level > 0 {
                self.sky_updates.remove.push((p, old_level));
            }
            self.sky_updates.increase.push(p);
            for d in SIDES {
                if self.level(Layer::Sky, p + d) > 1 {
                    self.sky_updates.increase.push(p + d);
                }
            }
        }
        self.sky_updates.note_peak();
    }

    /// Drop a stored sky level, including one left in a cell that just became a source.
    fn clear_sky(&mut self, p: IVec3) {
        let Some(slot) = self.chunks.get_mut(&chunk_of(p)) else { return };
        let Some(light) = slot.sky_light.as_mut() else { return };
        let l = local_of(p);
        light.set(index(l.x as usize, l.y as usize, l.z as usize), 0);
        if light.lit == 0 {
            slot.sky_light = None;
        }
    }

    /// Seed a newly installed chunk's sources and queue incoming light from loaded neighbours.
    pub(super) fn load_block_light(&mut self, pos: IVec3) {
        let data = &self.chunks[&pos].data;
        let base = pos * CHUNK_SIZE_I;
        let light = BlockLight::emitters(data);
        // Interior cells of a lava sea need no queue entries: their
        // neighbours already emit the same light. Seed only its surface.
        if light.is_some() {
            let mut i = 0usize;
            data.for_each_block(|b| {
                let e = b.emission();
                let l = IVec3::new((i & 31) as i32, (i >> 10) as i32, (i >> 5 & 31) as i32);
                if e > 1
                    && SIDES.into_iter().any(|d| {
                        let n = l + d;
                        if n.min_element() < 0 || n.max_element() >= CHUNK_SIZE_I {
                            return true;
                        }
                        let b = data.get(n.x as usize, n.y as usize, n.z as usize);
                        b.light_opacity() < 15 && b.emission() < e.saturating_sub(b.light_opacity().max(1))
                    })
                {
                    self.light_updates.increase.push(base + l);
                }
                i += 1;
            });
        }
        self.chunks.get_mut(&pos).unwrap().block_light = light;
        // Surviving neighbour light must also enter a newly loaded chunk.
        for d in SIDES {
            let neighbor = pos + d;
            if self.chunks.get(&neighbor).is_none_or(|s| s.block_light.is_none()) {
                continue;
            }
            let axis = if d.x != 0 {
                0
            } else if d.y != 0 {
                1
            } else {
                2
            };
            for u in 0..CHUNK_SIZE_I {
                for v in 0..CHUNK_SIZE_I {
                    let mut l = IVec3::ZERO;
                    l[axis] = if d[axis] > 0 { CHUNK_SIZE_I } else { -1 };
                    l[(axis + 1) % 3] = u;
                    l[(axis + 2) % 3] = v;
                    let p = base + l;
                    if self.raw_block_light(p) > 1 {
                        self.light_updates.increase.push(p);
                    }
                }
            }
        }
        self.light_updates.note_peak();
    }

    /// Queue removal at an unloaded chunk's lit borders so surviving chunks can relight.
    pub(super) fn unload_block_light(&mut self, pos: IVec3, light: Option<&BlockLight>) {
        let Some(light) = light else { return };
        let base = pos * CHUNK_SIZE_I;
        for axis in 0..3 {
            for side in [0, CHUNK_SIZE_I - 1] {
                for u in 0..CHUNK_SIZE_I {
                    for v in 0..CHUNK_SIZE_I {
                        let mut l = IVec3::ZERO;
                        l[axis] = side;
                        l[(axis + 1) % 3] = u;
                        l[(axis + 2) % 3] = v;
                        let level = light.get(l.x as usize, l.y as usize, l.z as usize);
                        if level > 1 {
                            self.light_updates.remove.push((base + l, level));
                        }
                    }
                }
            }
        }
        self.light_updates.note_peak();
    }

    /// Whether a cell exists and lets light through (dampening below 15).
    fn transmits(&self, p: IVec3) -> bool {
        self.get_block(p).is_some_and(|b| b.light_opacity() < MAX_LIGHT)
    }

    /// Seed the skylight of a fully loaded column once its heightmap is
    /// complete: queue its implied sources wherever they border a
    /// transparent non-source cell, in this column or a lit neighbour, plus
    /// light already stored along a lit neighbour's border.
    pub(super) fn light_sky_column(&mut self, key: IVec2) {
        if !self.generator.dimension.has_skylight() {
            return;
        }
        let full = self.column_chunks();
        let Some(col) = self.columns.get_mut(&key) else { return };
        if col.sky_lit || col.loaded != full {
            return;
        }
        col.sky_lit = true;
        let (min_y, max_y) = (self.min_y(), self.max_y());
        let rows = self.generator.dimension.chunk_rows();
        let base = IVec3::new(key.x * CHUNK_SIZE_I, 0, key.y * CHUNK_SIZE_I);
        let mut seeds = std::mem::take(&mut self.sky_updates.increase);
        for lz in 0..CHUNK_SIZE_I {
            for lx in 0..CHUNK_SIZE_I {
                let p0 = base + IVec3::new(lx, 0, lz);
                let h = self.lit_height(p0).unwrap_or(NO_HEIGHT as i32);
                // Leaves and water at the top take light from the source above.
                if h >= min_y && h + 1 < max_y && self.transmits(p0.with_y(h)) {
                    seeds.push(p0.with_y(h + 1));
                }
                for d in HORIZONTAL {
                    let n0 = p0 + d;
                    let Some(nh) = self.lit_height(n0) else { continue };
                    for y in (h + 1).max(min_y)..=nh.min(max_y - 1) {
                        if self.transmits(n0.with_y(y)) {
                            seeds.push(p0.with_y(y));
                        }
                    }
                    if column_of(chunk_of(n0)) == key {
                        continue;
                    }
                    // A lit neighbour's sources beside our transparent cells.
                    for y in (nh + 1).max(min_y)..=h.min(max_y - 1) {
                        if self.transmits(p0.with_y(y)) {
                            seeds.push(n0.with_y(y));
                        }
                    }
                    // Light the neighbour already holds below both heightmaps.
                    let top = h.min(nh);
                    for cy in rows.clone() {
                        let lo = cy * CHUNK_SIZE_I;
                        if lo > top {
                            break;
                        }
                        let Some(light) = self.chunks.get(&chunk_of(n0.with_y(lo))).and_then(|s| s.sky_light.as_ref())
                        else {
                            continue;
                        };
                        let nl = local_of(n0);
                        for ly in 0..CHUNK_SIZE.min((top - lo + 1) as usize) {
                            if light.get(nl.x as usize, ly, nl.z as usize) > 1 {
                                seeds.push(n0.with_y(lo + ly as i32));
                            }
                        }
                    }
                }
            }
        }
        self.sky_updates.increase = seeds;
        self.sky_updates.note_peak();
    }

    /// Before a lit column loses a chunk (or its data is replaced), queue
    /// removal of the light it passed into lit neighbours, then forget its
    /// skylight. Neighbours relight from their own surviving sources.
    pub(super) fn unlight_sky_column(&mut self, key: IVec2) {
        if !self.columns.get(&key).is_some_and(|c| c.sky_lit) {
            return;
        }
        let (min_y, max_y) = (self.min_y(), self.max_y());
        let base = IVec3::new(key.x * CHUNK_SIZE_I, 0, key.y * CHUNK_SIZE_I);
        let mut removals = std::mem::take(&mut self.sky_updates.remove);
        for i in 0..CHUNK_SIZE_I {
            let edges = [
                (IVec3::new(CHUNK_SIZE_I - 1, 0, i), IVec3::X),
                (IVec3::new(0, 0, i), IVec3::NEG_X),
                (IVec3::new(i, 0, CHUNK_SIZE_I - 1), IVec3::Z),
                (IVec3::new(i, 0, 0), IVec3::NEG_Z),
            ];
            for (l, d) in edges {
                let p0 = base + l;
                let n0 = p0 + d;
                let (Some(h), Some(nh)) = (self.lit_height(p0), self.lit_height(n0)) else { continue };
                // Above both heightmaps every pair is two sources.
                for y in min_y..=h.max(nh).min(max_y - 1) {
                    let (p, n) = (p0.with_y(y), n0.with_y(y));
                    let lp = self.level(Layer::Sky, p);
                    if lp > 1 && y <= nh && self.transmits(n) && self.level(Layer::Sky, n) < lp {
                        removals.push((p, lp));
                    }
                }
            }
        }
        self.sky_updates.remove = removals;
        self.sky_updates.note_peak();
        self.columns.get_mut(&key).unwrap().sky_lit = false;
        for cy in self.generator.dimension.chunk_rows() {
            if let Some(slot) = self.chunks.get_mut(&IVec3::new(key.x, cy, key.y)) {
                slot.sky_light = None;
            }
        }
    }

    /// Resolve batched block and sky light changes before gameplay queries.
    /// Called after edits, streaming, and world simulation steps; no workers
    /// or meshes required.
    pub fn update_block_light(&mut self) {
        self.run_light(Layer::Block);
        self.run_light(Layer::Sky);
    }

    fn run_light(&mut self, layer: Layer) {
        let mut queue = std::mem::take(self.updates(layer));
        while let Some((p, level)) = queue.remove.pop() {
            for d in SIDES {
                let n = p + d;
                let old = self.level(layer, n);
                if old == 0 {
                    continue;
                }
                if old < level {
                    let own = self.source_level(layer, n);
                    if old > own {
                        self.store(layer, n, own);
                        queue.remove.push((n, old));
                    }
                    if own > 0 {
                        queue.increase.push(n);
                    }
                } else {
                    queue.increase.push(n);
                }
            }
            queue.note_peak();
        }
        // Queued seeds recompute their level from every neighbour; cells
        // queued during the flood already hold the level that reached them.
        let seeds = queue.increase.len();
        let mut head = 0;
        while head < queue.increase.len() {
            let p = queue.increase[head];
            head += 1;
            let Some((b, mut level)) = self.light_cell(layer, p) else { continue };
            if head <= seeds {
                if layer == Layer::Block {
                    level = level.max(b.emission());
                }
                if b.light_opacity() < 15 {
                    for d in SIDES {
                        level = level.max(self.level(layer, p + d).saturating_sub(b.light_opacity().max(1)));
                    }
                }
                self.store(layer, p, level);
            }
            if level <= 1 {
                continue;
            }
            let mut brighter = [(IVec3::ZERO, 0); 6];
            let found = self.flood_from(layer, p, level, &mut brighter);
            for &(n, next) in &brighter[..found] {
                // Below 15, so never an implied sky source.
                self.store_raw(layer, n, next);
                queue.increase.push(n);
            }
            queue.peak = queue.peak.max(queue.increase.len() - head + queue.remove.len());
        }
        queue.increase.clear();
        let peak = queue.peak;
        let slot = self.updates(layer);
        // Edits made while resolving (none today) would have queued here.
        debug_assert!(slot.is_empty());
        slot.peak = slot.peak.max(peak);
        slot.remove = queue.remove;
        slot.increase = queue.increase;
    }
}

/// Fully loaded flat test columns, stone up to `ground` and air above,
/// streamed in as player-modified chunks.
#[cfg(test)]
pub(crate) fn flat_test_world(columns: impl IntoIterator<Item = IVec2>, ground: i32) -> World {
    let mut world = World::new_headless(std::sync::Arc::new(super::terrain::Generator::new(7)), Default::default(), 2);
    for c in columns {
        world.load_test_column(c, ground);
    }
    world.update_block_light();
    world
}

#[cfg(test)]
impl World {
    /// One chunk of a flat test column: stone up to `ground`, air above.
    pub(crate) fn flat_test_chunk(cy: i32, ground: i32) -> std::sync::Arc<ChunkData> {
        let lo = cy * CHUNK_SIZE_I;
        std::sync::Arc::new(if lo + CHUNK_SIZE_I - 1 <= ground {
            ChunkData::Uniform(Block::STONE)
        } else if lo > ground {
            ChunkData::Uniform(Block::AIR)
        } else {
            let mut blocks = ChunkData::new_dense(Block::AIR);
            for (i, b) in blocks.iter_mut().enumerate() {
                if lo + (i >> 10) as i32 <= ground {
                    *b = Block::STONE;
                }
            }
            ChunkData::from_dense(blocks)
        })
    }

    /// Stream a column back in, preferring its saved edits over flat terrain.
    pub(crate) fn load_test_column(&mut self, c: IVec2, ground: i32) {
        for cy in self.generator.dimension.chunk_rows() {
            let pos = IVec3::new(c.x, cy, c.y);
            let data = self.saved.remove(&pos).unwrap_or_else(|| Self::flat_test_chunk(cy, ground));
            self.insert_chunk(pos, data, true);
        }
    }

    /// Unload every chunk of a column, keeping its edits like streaming does.
    pub(crate) fn unload_test_column(&mut self, c: IVec2) {
        for cy in self.generator.dimension.chunk_rows() {
            self.remove_chunk(IVec3::new(c.x, cy, c.y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{self, Region};
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    fn world(chunks: &[IVec3]) -> World {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        for &p in chunks {
            world.insert_chunk(p, Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        world.update_block_light();
        world
    }

    fn torch(world: &mut World, p: IVec3) {
        world.set_block(p - IVec3::Y, Block::STONE);
        world.set_block(p, Block::TORCH);
    }

    #[test]
    fn torch_falloff_removal_and_dark_chunk_storage() {
        let c = IVec3::new(0, 4, 0);
        let mut world = world(&[c]);
        let p = IVec3::new(16, 144, 16);
        assert!(world.chunks[&c].block_light.is_none());
        torch(&mut world, p);
        for d in [IVec3::X, IVec3::Y, IVec3::Z, IVec3::NEG_X, IVec3::NEG_Z] {
            for distance in 0..=14 {
                assert_eq!(world.block_light(p + d * distance), 14u8.saturating_sub(distance as u8));
            }
        }
        assert_eq!(world.block_light(p - IVec3::Y), 0);
        world.set_block(p, Block::AIR);
        assert!(world.chunks[&c].block_light.is_none(), "removing the last source releases packed storage");
        assert!(world.light_updates.is_empty());
        assert!(world.mesh_uploads.is_empty());
    }

    #[test]
    fn blockers_and_overlapping_sources_relight_correctly() {
        let mut world = world(&[IVec3::new(0, 4, 0)]);
        for y in 128..160 {
            for z in 0..32 {
                world.edit(IVec3::new(16, y, z), Block::STONE, false);
            }
        }
        let p = IVec3::new(14, 150, 16);
        let hole = IVec3::new(16, 150, 16);
        let other_side = hole + IVec3::X;
        torch(&mut world, p);
        assert_eq!(world.block_light(other_side), 0);
        for (block, level) in [(Block::AIR, 11), (Block::STONE, 0), (Block::GLASS, 11), (Block::LEAVES, 11)] {
            world.set_block(hole, block);
            assert_eq!(world.block_light(other_side), level, "{block:?}");
        }
        let second = IVec3::new(20, 150, 16);
        torch(&mut world, second);
        world.set_block(hole, Block::STONE);
        assert_eq!(world.block_light(other_side), 11);
        world.set_block(p, Block::AIR);
        assert_eq!(world.block_light(other_side), 11, "the other source survives removal");
        world.set_block(second, Block::AIR);
        assert_eq!(world.block_light(other_side), 0);
    }

    #[test]
    fn light_crosses_loaded_borders_and_recovers_on_reload() {
        let left = IVec3::new(-1, 4, 0);
        let right = IVec3::new(0, 4, 0);
        let mut world = world(&[left]);
        let source = IVec3::new(-1, 150, 16);
        let target = source + IVec3::X;
        torch(&mut world, source);
        assert_eq!(world.block_light(target), 0);
        world.insert_chunk(right, Arc::new(ChunkData::Uniform(Block::AIR)), false);
        world.update_block_light();
        assert_eq!(world.block_light(target), 13);
        world.remove_chunk(right);
        world.update_block_light();
        assert_eq!(world.block_light(target), 0);
        world.insert_chunk(right, Arc::new(ChunkData::Uniform(Block::AIR)), false);
        world.update_block_light();
        assert_eq!(world.block_light(target), 13);
        world.remove_chunk(left);
        world.update_block_light();
        assert_eq!(world.block_light(target), 0);
        assert!(world.chunks[&right].block_light.is_none());
        let data = world.saved.remove(&left).unwrap();
        world.insert_chunk(left, data, true);
        world.update_block_light();
        assert_eq!(world.block_light(target), 13, "saved emitters regenerate their light");
        assert!(world.mesh_uploads.is_empty() && world.mesh_removals.is_empty());
    }

    #[test]
    fn world_height_bounds_and_uniform_emitters_are_finite() {
        let c = IVec3::new(0, 7, 0);
        let mut world = world(&[c]);
        let source = IVec3::new(16, 255, 16);
        world.set_block(source, Block::GLOWSTONE);
        assert_eq!(world.block_light(source), 15);
        assert_eq!(world.block_light(source - IVec3::Y), 14);
        assert_eq!(world.block_light(source + IVec3::Y), 0);
        world.remove_chunk(c);
        world.insert_chunk(c, Arc::new(ChunkData::Uniform(Block::LAVA)), false);
        world.update_block_light();
        assert_eq!(world.block_light(source), 15);
        assert_eq!(world.block_light(IVec3::new(16, 224, 16)), 15);
        assert_eq!(world.block_light(source + IVec3::Y), 0);
        world.remove_chunk(c);
        world.update_block_light();
        assert!(world.light_updates.is_empty());
    }

    #[test]
    fn gameplay_light_agrees_with_fresh_mesh_light_after_edits() {
        let chunks: Vec<_> =
            (-1..=1).flat_map(|x| (-1..=1).flat_map(move |z| (3..=5).map(move |y| IVec3::new(x, y, z)))).collect();
        let mut world = world(&chunks);
        let source = IVec3::new(8, 150, 8);
        torch(&mut world, source);
        world.set_block(IVec3::new(20, 150, 20), Block::GLOWSTONE);
        world.set_block(IVec3::new(9, 150, 8), Block::LEAVES);
        world.set_block(IVec3::new(8, 151, 8), Block::GLASS);
        world.edit(IVec3::new(8, 150, 9), Block::WATER, false);
        world.set_block(IVec3::new(19, 150, 20), Block::slab_of(Block::STONE).unwrap());
        for remove in [false, true] {
            if remove {
                world.set_block(source, Block::AIR);
                world.set_block(IVec3::new(20, 151, 20), Block::STONE);
            }
            world.update_block_light();
            let mesh = mesh::build(&world.gather(IVec3::new(0, 4, 0)), &mut Region::default());
            let reference = mesh.block_light.unwrap();
            for y in 0..32 {
                for z in 0..32 {
                    for x in 0..32 {
                        let p = IVec3::new(x as i32, y as i32 + 128, z as i32);
                        assert_eq!(world.block_light(p), reference.get(x, y, z), "{p}, remove={remove}");
                    }
                }
            }
        }
    }

    fn layered(cy: i32, ground: i32) -> Arc<ChunkData> {
        World::flat_test_chunk(cy, ground)
    }

    /// Fully loaded flat columns with the ground surface at y = 63.
    fn flat(cols: impl IntoIterator<Item = IVec2>) -> World {
        super::flat_test_world(cols, 63)
    }

    fn square(r: i32) -> Vec<IVec2> {
        (-r..=r).flat_map(|z| (-r..=r).map(move |x| IVec2::new(x, z))).collect()
    }

    /// The same chunks streamed into a new world in another order.
    fn rebuilt(world: &World) -> World {
        let mut fresh = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        let mut chunks: Vec<_> = world.chunks.iter().map(|(&p, s)| (p, s.data.clone())).collect();
        chunks.sort_unstable_by_key(|(p, _)| (p.y * 7 + p.x * 3 + p.z).rem_euclid(11));
        for (p, data) in chunks {
            fresh.insert_chunk(p, data, true);
        }
        fresh.update_block_light();
        fresh
    }

    /// A LAN replica applies host edits incrementally; its light must match a
    /// replica that installs the same final chunk as a fresh snapshot.
    #[test]
    fn replica_edits_relight_incrementally_like_a_fresh_snapshot() {
        let replica = |w: &mut World| {
            for c in square(1) {
                for cy in w.generator.dimension.chunk_rows() {
                    w.receive_chunk(IVec3::new(c.x, cy, c.y), World::flat_test_chunk(cy, 63));
                }
            }
            w.update_block_light();
        };
        let mut edited = World::new_remote(Arc::new(Generator::new(7)), 2);
        replica(&mut edited);
        let pos = IVec3::new(0, 2, 0);
        let mut cells = Vec::new();
        let mut cell = |l: IVec3, b: Block| cells.push((index(l.x as usize, l.y as usize, l.z as usize) as u16, b));
        // Chunk row 2 starts at y 64: a stone roof at y 70 with one glass
        // cell, and a torch on the ground beneath it.
        for x in 2..12 {
            for z in 2..12 {
                cell(IVec3::new(x, 6, z), if (x, z) == (6, 6) { Block::GLASS } else { Block::STONE });
            }
        }
        cell(IVec3::new(4, 0, 4), Block::TORCH);
        // 100 cells: past the batch size `receive_edits` applies incrementally.
        edited.apply_replica_edits(pos, &cells);
        // Then open the roof again in one place.
        edited.receive_edits(pos, &[(index(9, 6, 9) as u16, Block::AIR)]);

        let final_data = edited.chunks[&pos].data.clone();
        let mut fresh = World::new_remote(Arc::new(Generator::new(7)), 2);
        replica(&mut fresh);
        fresh.receive_chunk(pos, final_data);
        fresh.update_block_light();
        for x in -4..20 {
            for z in -4..20 {
                for y in 56..80 {
                    let p = IVec3::new(x, y, z);
                    assert_eq!(edited.sky_light(p), fresh.sky_light(p), "sky {p}");
                    assert_eq!(edited.block_light(p), fresh.block_light(p), "block {p}");
                }
            }
        }
        assert!(edited.block_light(IVec3::new(4, 64, 4)) >= 14, "the torch lights its cell");
        assert!(
            edited.sky_light(IVec3::new(6, 69, 6)).unwrap() > edited.sky_light(IVec3::new(4, 69, 4)).unwrap(),
            "glass lets sky through"
        );
    }

    /// Release-mode cost of skylight work on a replica. Run with
    /// `cargo test --release --lib skylight_cost -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing measurement"]
    fn skylight_cost() {
        use std::time::Instant;
        let replica = || {
            let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
            for c in square(2) {
                for cy in w.generator.dimension.chunk_rows() {
                    w.receive_chunk(IVec3::new(c.x, cy, c.y), World::flat_test_chunk(cy, 63));
                }
            }
            w.update_block_light();
            w
        };
        let median = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v[v.len() / 2]
        };
        let pos = IVec3::new(0, 2, 0);
        let roof: Vec<_> =
            (0..CHUNK_SIZE).flat_map(|x| (0..CHUNK_SIZE).map(move |z| (index(x, 6, z) as u16, Block::STONE))).collect();
        let mut column = vec![];
        for _ in 0..15 {
            let mut w = replica();
            let data = w.chunks[&pos].data.clone();
            let t = Instant::now();
            w.receive_chunk(pos, data);
            w.update_block_light();
            column.push(t.elapsed().as_secs_f64() * 1e3);
        }
        println!("whole-column relight: median {:.3} ms", median(column));
        for n in [1, 4, 16, 64, 256, 1024] {
            let (mut snapshot, mut incremental) = (vec![], vec![]);
            for _ in 0..15 {
                let mut w = replica();
                let data = Arc::new(w.chunks[&pos].data.with_edits(&roof[..n]));
                let t = Instant::now();
                w.receive_chunk(pos, data);
                w.update_block_light();
                snapshot.push(t.elapsed().as_secs_f64() * 1e3);
                let mut w = replica();
                let t = Instant::now();
                w.apply_replica_edits(pos, &roof[..n]);
                incremental.push(t.elapsed().as_secs_f64() * 1e3);
            }
            println!(
                "{n:>4} roof cells: snapshot {:.3} ms, incremental {:.3} ms",
                median(snapshot),
                median(incremental)
            );
        }
    }

    #[test]
    fn skylight_fills_shafts_spreads_into_tunnels_and_follows_covers() {
        let mut world = flat(square(2));
        let top = IVec3::new(16, 63, 16);
        assert_eq!(world.sky_light(top + IVec3::Y), Some(15));
        assert_eq!(world.sky_light(top), Some(0), "opaque ground");
        assert_eq!(world.sky_light(IVec3::new(16, 400, 16)), Some(15), "above the build limit");
        for y in 40..=63 {
            world.set_block(top.with_y(y), Block::AIR);
        }
        // Tunnel east at the bottom, across the column border at x = 32.
        for x in 17..=40 {
            world.set_block(IVec3::new(x, 40, 16), Block::AIR);
        }
        let tunnel = |w: &World, x: i32| w.sky_light(IVec3::new(x, 40, 16)).unwrap();
        assert_eq!(world.sky_light(top.with_y(40)), Some(15), "an open shaft is all sources");
        for x in 17..=40 {
            assert_eq!(tunnel(&world, x), 15u8.saturating_sub((x - 16) as u8), "x = {x}");
        }
        // Covering the shaft leaves the sealed cave dark; glass lets the sky back.
        world.set_block(top, Block::STONE);
        assert!((40..63).all(|y| world.sky_light(top.with_y(y)) == Some(0)));
        assert_eq!(tunnel(&world, 20), 0);
        world.set_block(top, Block::GLASS);
        assert_eq!(world.sky_light(top.with_y(40)), Some(15));
        assert_eq!(tunnel(&world, 20), 11);
        // Leaves dampen by one and stop the source column: light then falls
        // by one per block down the shaft.
        world.set_block(top, Block::LEAVES);
        assert_eq!(world.sky_light(top), Some(14));
        for y in 49..63 {
            assert_eq!(world.sky_light(top.with_y(y)), Some((y - 49) as u8), "y = {y}");
        }
        assert_eq!(tunnel(&world, 17), 0);
        world.set_block(top, Block::AIR);
        assert_eq!(tunnel(&world, 30), 1);
        assert!(world.light_updates.is_empty() && world.sky_updates.is_empty());
    }

    #[test]
    fn overhangs_light_from_the_side_and_match_mesh_and_fresh_light() {
        let mut world = flat(square(1));
        // A 5x5 roof at y = 70 shades the ground beneath it from the sides.
        for z in 14..=18 {
            for x in 14..=18 {
                world.set_block(IVec3::new(x, 70, z), Block::STONE);
            }
        }
        assert_eq!(world.sky_light(IVec3::new(16, 69, 16)), Some(12));
        assert_eq!(world.sky_light(IVec3::new(16, 64, 16)), Some(12));
        assert_eq!(world.sky_light(IVec3::new(14, 64, 16)), Some(14));
        // Deterministic mixed edits around the centre column and its borders.
        let mut seed = 0x5eed_u32;
        let mut next = |n: u32| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) % n
        };
        let palette =
            [Block::AIR, Block::AIR, Block::STONE, Block::LEAVES, Block::GLASS, Block::slab_of(Block::STONE).unwrap()];
        for _ in 0..600 {
            let p = IVec3::new(next(48) as i32 - 8, 44 + next(40) as i32, next(48) as i32 - 8);
            world.set_block(p, palette[next(palette.len() as u32) as usize]);
        }
        let fresh = rebuilt(&world);
        for cy in [1, 2] {
            let pos = IVec3::new(0, cy, 0);
            let mesh = mesh::build(&world.gather(pos), &mut Region::default());
            let reference = mesh.sky_light.unwrap();
            for y in 0..32 {
                for z in 0..32 {
                    for x in 0..32 {
                        let p = IVec3::new(x as i32, y as i32 + cy * 32, z as i32);
                        let sky = world.sky_light(p);
                        assert_eq!(sky, Some(reference.get(x, y, z)), "mesh {p}");
                        assert_eq!(sky, fresh.sky_light(p), "fresh {p}");
                    }
                }
            }
        }
    }

    #[test]
    fn skylight_recovers_across_unloaded_reloaded_and_partly_loaded_columns() {
        let mut world = flat((-1..=2).flat_map(|x| (-1..=1).map(move |z| IVec2::new(x, z))));
        // A shaft in column (1, 0) lights a tunnel back into column (0, 0).
        let shaft = IVec3::new(36, 0, 16);
        for y in 50..=63 {
            world.set_block(shaft.with_y(y), Block::AIR);
        }
        for x in 24..36 {
            world.set_block(IVec3::new(x, 50, 16), Block::AIR);
        }
        let before: Vec<_> = (24..36).map(|x| world.sky_light(IVec3::new(x, 50, 16))).collect();
        assert_eq!(before[7], Some(10), "x = 31, five steps from the shaft");
        let rows = world.generator.dimension.chunk_rows();
        for cy in rows.clone() {
            world.remove_chunk(IVec3::new(1, cy, 0));
        }
        world.update_block_light();
        let peak = world.take_light_peak();
        assert!(peak > 0 && peak < 50_000, "bounded unload queue: {peak}");
        assert_eq!(world.sky_light(IVec3::new(31, 50, 16)), None, "beside unloaded terrain");
        assert_eq!(world.level(Layer::Sky, IVec3::new(31, 50, 16)), 0, "light from the unloaded column is gone");
        assert_eq!(world.level(Layer::Sky, IVec3::new(-16, 64, 16)), 15, "other columns keep their light");
        // Half the column streams back: still unknown, and edits there wait.
        for cy in rows.clone().step_by(2) {
            let data = world.saved.remove(&IVec3::new(1, cy, 0)).unwrap_or_else(|| layered(cy, 63));
            world.insert_chunk(IVec3::new(1, cy, 0), data, true);
        }
        world.update_block_light();
        assert_eq!(world.sky_light(IVec3::new(31, 50, 16)), None);
        world.set_block(IVec3::new(40, 70, 16), Block::STONE);
        for cy in rows.skip(1).step_by(2) {
            let data = world.saved.remove(&IVec3::new(1, cy, 0)).unwrap_or_else(|| layered(cy, 63));
            world.insert_chunk(IVec3::new(1, cy, 0), data, true);
        }
        world.update_block_light();
        let after: Vec<_> = (24..36).map(|x| world.sky_light(IVec3::new(x, 50, 16))).collect();
        assert_eq!(after, before);
        let fresh = rebuilt(&world);
        for y in 40..80 {
            for x in -8..72 {
                let p = IVec3::new(x, y, 16);
                assert_eq!(world.sky_light(p), fresh.sky_light(p), "{p}");
            }
        }
    }

    #[test]
    fn skylight_flags_are_separate_from_visual_sky_and_weather() {
        let nether = World::new_headless(
            Arc::new(Generator::for_dimension(1, crate::world::terrain::Dimension::Nether)),
            Default::default(),
            2,
        );
        assert_eq!(nether.sky_light(IVec3::new(0, 64, 0)), Some(0));
        let mut end = World::new_headless(
            Arc::new(Generator::for_dimension(1, crate::world::terrain::Dimension::End)),
            Default::default(),
            2,
        );
        assert_eq!(end.sky_light(IVec3::new(0, 64, 0)), None, "unloaded End terrain is unknown");
        for c in square(1) {
            end.load_test_column(c, 63);
        }
        end.update_block_light();
        assert_eq!(end.sky_light(IVec3::new(0, 64, 0)), Some(15));
        assert_eq!(end.sky_light(IVec3::new(0, 62, 0)), Some(0));
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/lighting_spawning.json")).unwrap();
        for (dimension, key) in [
            (crate::world::terrain::Dimension::Overworld, "overworld"),
            (crate::world::terrain::Dimension::Nether, "the_nether"),
            (crate::world::terrain::Dimension::End, "the_end"),
        ] {
            assert_eq!(Some(dimension.has_skylight()), fixture["dimensions"][key]["has_skylight"].as_bool());
            // Rendered sky contribution follows the visual factor; propagation follows has_skylight.
            assert_eq!(
                Some(dimension.visual_sky_light_factor() as f64),
                fixture["dimensions"][key]["visual_sky_light_factor"].as_f64(),
                "{key}"
            );
        }
        let world = flat([IVec2::ZERO]);
        assert_eq!(world.sky_light(IVec3::new(16, 64, 16)), None, "neighbour columns are not loaded");
    }

    #[test]
    fn raw_brightness_combines_darkened_sky_and_torches() {
        let mut world = flat(square(1));
        let open = IVec3::new(8, 64, 8);
        assert_eq!(world.raw_brightness(open, 0), Some(15));
        assert_eq!(world.raw_brightness(open, 11), Some(4), "a clear night");
        world.set_block(IVec3::new(10, 64, 8), Block::TORCH);
        assert_eq!(world.raw_brightness(open, 11), Some(12));
        assert_eq!(world.raw_brightness(open + IVec3::new(0, -1, 0), 0), Some(0), "inside stone");
    }
}
