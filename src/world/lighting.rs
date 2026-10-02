//! Authoritative block light, independent of render meshes. Increasing
//! light floods outward; removal clears dependent paths before relighting
//! from surviving sources. Only lit chunks allocate packed nibble arrays.

use glam::IVec3;

use super::World;
use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, ChunkData, chunk_of, index, local_of};

const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

#[derive(Clone, Debug)]
pub(crate) struct BlockLight {
    cells: Box<[u8; CHUNK_VOLUME / 2]>,
    lit: usize,
}

impl BlockLight {
    fn new() -> Self {
        Self { cells: Box::new([0; CHUNK_VOLUME / 2]), lit: 0 }
    }

    pub(crate) fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        let i = index(x, y, z);
        self.cells[i / 2] >> (i % 2 * 4) & 15
    }

    fn set(&mut self, i: usize, level: u8) {
        let shift = i % 2 * 4;
        let old = self.cells[i / 2] >> shift & 15;
        self.lit = self.lit + usize::from(level != 0) - usize::from(old != 0);
        self.cells[i / 2] = (self.cells[i / 2] & !(15 << shift)) | level << shift;
    }

    #[cfg(test)]
    pub(crate) fn from_nibbles(cells: Box<[u8; CHUNK_VOLUME / 2]>) -> Self {
        let lit = cells.iter().map(|v| usize::from(v & 15 != 0) + usize::from(v >> 4 != 0)).sum();
        Self { cells, lit }
    }

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

#[derive(Default)]
pub(super) struct LightUpdates {
    remove: Vec<(IVec3, u8)>,
    increase: Vec<IVec3>,
}

impl LightUpdates {
    pub fn is_empty(&self) -> bool {
        self.remove.is_empty() && self.increase.is_empty()
    }
}

impl World {
    // Physics treats space above the world as air. Lighting needs actual
    // storage, so it must never enqueue that virtual air or unloaded cells.
    fn lighting_block(&self, p: IVec3) -> Option<Block> {
        let l = local_of(p);
        self.chunks.get(&chunk_of(p)).map(|s| s.data.get(l.x as usize, l.y as usize, l.z as usize))
    }

    fn raw_block_light(&self, p: IVec3) -> u8 {
        let l = local_of(p);
        self.chunks
            .get(&chunk_of(p))
            .and_then(|s| s.block_light.as_ref())
            .map_or(0, |light| light.get(l.x as usize, l.y as usize, l.z as usize))
    }

    fn set_block_light(&mut self, p: IVec3, level: u8) {
        let Some(slot) = self.chunks.get_mut(&chunk_of(p)) else { return };
        if level == 0 && slot.block_light.is_none() {
            return;
        }
        let l = local_of(p);
        let light = slot.block_light.get_or_insert_with(BlockLight::new);
        light.set(index(l.x as usize, l.y as usize, l.z as usize), level);
        if light.lit == 0 {
            slot.block_light = None;
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

    pub(super) fn light_block_changed(&mut self, p: IVec3, old: Block, new: Block, old_level: u8) {
        if old.emission() == new.emission() && old.light_opacity() == new.light_opacity() {
            return;
        }
        let emission = new.emission();
        self.set_block_light(p, emission);
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
    }

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
    }

    /// Resolve batched changes before gameplay queries. Called after edits,
    /// streaming, and world simulation steps; no workers or meshes required.
    pub fn update_block_light(&mut self) {
        while let Some((p, level)) = self.light_updates.remove.pop() {
            for d in SIDES {
                let n = p + d;
                let old = self.raw_block_light(n);
                if old == 0 {
                    continue;
                }
                if old < level {
                    let emission = self.get_block(n).map_or(0, Block::emission);
                    if old > emission {
                        self.set_block_light(n, emission);
                        self.light_updates.remove.push((n, old));
                    }
                    if emission > 0 {
                        self.light_updates.increase.push(n);
                    }
                } else {
                    self.light_updates.increase.push(n);
                }
            }
        }
        let mut head = 0;
        while head < self.light_updates.increase.len() {
            let p = self.light_updates.increase[head];
            head += 1;
            let Some(b) = self.lighting_block(p) else { continue };
            let mut level = self.raw_block_light(p).max(b.emission());
            if b.light_opacity() < 15 {
                for d in SIDES {
                    level = level.max(self.raw_block_light(p + d).saturating_sub(b.light_opacity().max(1)));
                }
            }
            self.set_block_light(p, level);
            if level <= 1 {
                continue;
            }
            for d in SIDES {
                let n = p + d;
                let Some(b) = self.lighting_block(n) else { continue };
                if b.light_opacity() >= 15 {
                    continue;
                }
                let next = level.saturating_sub(b.light_opacity().max(1));
                if next > self.raw_block_light(n) {
                    self.set_block_light(n, next);
                    self.light_updates.increase.push(n);
                }
            }
        }
        self.light_updates.increase.clear();
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
}
