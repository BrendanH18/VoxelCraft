//! Chunk streaming, block access and edits.
//!
//! The world owns all loaded chunk data. Each frame it:
//! 1. unloads chunks that fell out of range (when the player changes chunk),
//! 2. collects finished jobs from the worker pool,
//! 3. submits generation jobs for the nearest missing chunks, and
//! 4. submits light+mesh jobs for chunks whose surrounding 3x3 columns are
//!    fully loaded.
//!
//! Finished meshes are queued in `mesh_uploads` for the renderer. Every
//! chunk carries a version number so meshes built from stale data are
//! discarded. Each chunk column also keeps a heightmap of its highest
//! light-blocking block, which seeds skylight in mesh jobs.

pub mod block;
pub mod chunk;
pub mod noise;
pub mod storage;
pub mod terrain;

use std::sync::Arc;

use glam::{DVec3, IVec2, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::mesh::{self, MeshData, MeshInput, Neighborhood, Region, D, MARGIN, NO_HEIGHT};
use crate::workers::{Job, JobResult, Workers};
use block::Block;
use chunk::{chunk_of, local_of, ChunkData, CHUNK_SIZE, CHUNK_SIZE_I, WORLD_HEIGHT, WORLD_HEIGHT_CHUNKS};
use terrain::Generator;

pub struct ChunkSlot {
    pub data: Arc<ChunkData>,
    pub modified: bool,
    version: u32,
    meshed_version: Option<u32>,
    mesh_in_flight: bool,
}

/// Per chunk-column state: skylight heightmap and how many of its chunks
/// are loaded.
struct Column {
    heights: Box<[i16; CHUNK_SIZE * CHUNK_SIZE]>,
    loaded: i32,
}

pub struct World {
    pub generator: Arc<Generator>,
    workers: Workers,
    chunks: FxHashMap<IVec3, ChunkSlot>,
    columns: FxHashMap<IVec2, Column>,
    /// Player-modified chunks that are currently unloaded.
    saved: FxHashMap<IVec3, Arc<ChunkData>>,
    gen_in_flight: FxHashSet<IVec3>,
    mesh_in_flight: usize,
    /// Chunks in mesh range that need (re)meshing.
    dirty: FxHashSet<IVec3>,
    load_list: Vec<IVec3>,
    load_cursor: usize,
    center: Option<IVec3>,
    render_distance: i32,
    region: Box<Region>,
    pub mesh_uploads: Vec<(IVec3, MeshData)>,
    pub mesh_removals: Vec<IVec3>,
}

fn column_of(chunk: IVec3) -> IVec2 {
    IVec2::new(chunk.x, chunk.z)
}

impl World {
    pub fn new(generator: Arc<Generator>, saved: FxHashMap<IVec3, Arc<ChunkData>>, render_distance: i32) -> Self {
        Self {
            workers: Workers::new(generator.clone()),
            generator,
            chunks: FxHashMap::default(),
            columns: FxHashMap::default(),
            saved,
            gen_in_flight: FxHashSet::default(),
            mesh_in_flight: 0,
            dirty: FxHashSet::default(),
            load_list: Vec::new(),
            load_cursor: 0,
            center: None,
            render_distance,
            region: Box::default(),
            mesh_uploads: Vec::new(),
            mesh_removals: Vec::new(),
        }
    }

    pub fn render_distance(&self) -> i32 {
        self.render_distance
    }

    pub fn set_render_distance(&mut self, rd: i32) {
        self.render_distance = rd.clamp(2, 32);
        // Force the load/unload sets to be recomputed.
        if let Some(c) = self.center.take() {
            self.recenter(c);
        }
    }

    pub fn loaded_chunks(&self) -> usize {
        self.chunks.len()
    }

    pub fn pending_jobs(&self) -> usize {
        self.gen_in_flight.len() + self.mesh_in_flight
    }

    pub fn worker_threads(&self) -> usize {
        self.workers.threads
    }

    fn horizontal_dist2(&self, pos: IVec3) -> i32 {
        let c = self.center.unwrap_or(IVec3::ZERO);
        let (dx, dz) = (pos.x - c.x, pos.z - c.z);
        dx * dx + dz * dz
    }

    fn priority(&self, pos: IVec3) -> i32 {
        let c = self.center.unwrap_or(IVec3::ZERO);
        let dy = (pos.y - c.y) * 3 / 2;
        self.horizontal_dist2(pos) * 2 + dy * dy
    }

    /// Chunks within this radius are meshed and drawn.
    fn in_mesh_range(&self, pos: IVec3) -> bool {
        self.horizontal_dist2(pos) <= self.render_distance * self.render_distance
    }

    /// Loaded radius is one ring larger than the mesh radius so every meshed
    /// chunk has all of its neighbours.
    fn in_load_range(&self, pos: IVec3) -> bool {
        let r = self.render_distance as f32 + 1.5;
        (self.horizontal_dist2(pos) as f32) <= r * r
    }

    /// Extra hysteresis before unloading so walking back and forth across a
    /// chunk border doesn't thrash.
    fn in_keep_range(&self, pos: IVec3) -> bool {
        let r = self.render_distance + 3;
        self.horizontal_dist2(pos) <= r * r
    }

    pub fn is_loaded(&self, block: IVec3) -> bool {
        block.y < 0 || block.y >= WORLD_HEIGHT || self.chunks.contains_key(&chunk_of(block))
    }

    /// Block at a world position; `None` if the chunk isn't loaded.
    pub fn get_block(&self, p: IVec3) -> Option<Block> {
        if p.y < 0 {
            return Some(Block::BEDROCK);
        }
        if p.y >= WORLD_HEIGHT {
            return Some(Block::AIR);
        }
        let l = local_of(p);
        self.chunks
            .get(&chunk_of(p))
            .map(|s| s.data.get(l.x as usize, l.y as usize, l.z as usize))
    }

    /// Changes a block. Chunks whose geometry changes are remeshed
    /// synchronously so the edit shows up this frame; chunks whose lighting
    /// changes are remeshed on the workers.
    pub fn set_block(&mut self, p: IVec3, block: Block) -> bool {
        if p.y < 0 || p.y >= WORLD_HEIGHT {
            return false;
        }
        let cpos = chunk_of(p);
        let l = local_of(p);
        let Some(slot) = self.chunks.get_mut(&cpos) else { return false };
        Arc::make_mut(&mut slot.data).set(l.x as usize, l.y as usize, l.z as usize, block);
        slot.modified = true;

        // Keep the column heightmap current.
        let (old_h, new_h) = self.update_height(p, block);

        // Light reaches 15 blocks, plus 1 for the face-adjacent sample cell;
        // a heightmap change also re-exposes everything between old and new.
        let reach = MARGIN as i32 + 1;
        let lo_y = p.y.min(old_h).min(new_h).max(0) - reach;
        let hi_y = p.y.max(old_h).max(new_h) + reach;
        let lo = chunk_of(IVec3::new(p.x - reach, lo_y, p.z - reach)).max(IVec3::new(i32::MIN, 0, i32::MIN));
        let hi = chunk_of(IVec3::new(p.x + reach, hi_y, p.z + reach)).min(IVec3::new(i32::MAX, WORLD_HEIGHT_CHUNKS - 1, i32::MAX));
        let geometry = |c: IVec3| {
            let d = (c - cpos).abs();
            let near = |axis: usize| {
                let v = l[axis];
                match (c - cpos)[axis] {
                    0 => true,
                    -1 => v == 0,
                    _ => v == CHUNK_SIZE_I - 1,
                }
            };
            d.max_element() <= 1 && near(0) && near(1) && near(2)
        };
        for cy in lo.y..=hi.y {
            for cz in lo.z..=hi.z {
                for cx in lo.x..=hi.x {
                    let c = IVec3::new(cx, cy, cz);
                    let Some(slot) = self.chunks.get_mut(&c) else { continue };
                    slot.version = slot.version.wrapping_add(1);
                    if geometry(c) {
                        self.remesh_now(c);
                    } else if self.in_mesh_range(c) {
                        self.dirty.insert(c);
                    }
                }
            }
        }
        true
    }

    /// Updates the heightmap for an edited block; returns (old, new) heights.
    fn update_height(&mut self, p: IVec3, block: Block) -> (i32, i32) {
        let key = column_of(chunk_of(p));
        let l = local_of(p);
        let i = (l.x + l.z * CHUNK_SIZE_I) as usize;
        let Some(col) = self.columns.get(&key) else { return (p.y, p.y) };
        let old = col.heights[i] as i32;
        let new = if block.light_opacity() > 0 {
            old.max(p.y)
        } else if p.y == old {
            (0..p.y)
                .rev()
                .find(|&y| self.get_block(IVec3::new(p.x, y, p.z)).is_some_and(|b| b.light_opacity() > 0))
                .unwrap_or(NO_HEIGHT as i32)
        } else {
            old
        };
        self.columns.get_mut(&key).unwrap().heights[i] = new as i16;
        (old.max(0), new.max(0))
    }

    /// A chunk can be meshed once the 3x3 columns around it are fully
    /// loaded (skylight needs complete heightmaps).
    fn ready_to_mesh(&self, pos: IVec3) -> bool {
        (-1..=1).all(|dz| {
            (-1..=1).all(|dx| {
                self.columns
                    .get(&IVec2::new(pos.x + dx, pos.z + dz))
                    .is_some_and(|c| c.loaded == WORLD_HEIGHT_CHUNKS)
            })
        })
    }

    fn gather(&self, pos: IVec3) -> Box<MeshInput> {
        let mut neighbors: Neighborhood = Default::default();
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let i = (dx + 1) + (dz + 1) * 3 + (dy + 1) * 9;
                    neighbors[i as usize] =
                        self.chunks.get(&(pos + IVec3::new(dx, dy, dz))).map(|s| s.data.clone());
                }
            }
        }
        let cols: [Option<&Column>; 9] =
            std::array::from_fn(|i| self.columns.get(&IVec2::new(pos.x + i as i32 % 3 - 1, pos.z + i as i32 / 3 - 1)));
        let mut heights = Box::new([NO_HEIGHT; D * D]);
        for rz in 0..D {
            let wz = rz as i32 - MARGIN as i32 + CHUNK_SIZE_I; // relative to the -1 column
            let (cz, lz) = ((wz / CHUNK_SIZE_I) as usize, (wz % CHUNK_SIZE_I) as usize);
            for rx in 0..D {
                let wx = rx as i32 - MARGIN as i32 + CHUNK_SIZE_I;
                let (cx, lx) = ((wx / CHUNK_SIZE_I) as usize, (wx % CHUNK_SIZE_I) as usize);
                if let Some(col) = cols[cx + cz * 3] {
                    heights[rx + rz * D] = col.heights[lx + lz * CHUNK_SIZE];
                }
            }
        }
        Box::new(MeshInput { neighbors, heights, base_y: pos.y * CHUNK_SIZE_I })
    }

    fn remesh_now(&mut self, pos: IVec3) {
        if !self.in_mesh_range(pos) || !self.ready_to_mesh(pos) {
            self.dirty.insert(pos);
            return;
        }
        let input = self.gather(pos);
        let mesh = mesh::build(&input, &mut self.region);
        let slot = self.chunks.get_mut(&pos).unwrap();
        slot.meshed_version = Some(slot.version);
        self.dirty.remove(&pos);
        self.mesh_uploads.push((pos, mesh));
    }

    /// Returns `true` when a chunk trivially produces no geometry: all air,
    /// or solid opaque and fully enclosed by solid opaque neighbours.
    fn trivially_empty(&self, pos: IVec3) -> bool {
        let data = &self.chunks[&pos].data;
        match data.uniform() {
            Some(Block::AIR) => true,
            Some(b) if b.is_opaque() => [
                IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z,
            ]
            .iter()
            .all(|&o| {
                let n = pos + o;
                n.y < 0
                    || self
                        .chunks
                        .get(&n)
                        .and_then(|s| s.data.uniform())
                        .is_some_and(|b| b.is_opaque())
            }),
            _ => false,
        }
    }

    fn recenter(&mut self, center: IVec3) {
        self.center = Some(center);

        // Unload far chunks; keep player edits around in memory.
        let far: Vec<IVec3> = self.chunks.keys().copied().filter(|&p| !self.in_keep_range(p)).collect();
        for pos in far {
            self.remove_chunk(pos);
        }
        // Chunks that just came into mesh range, or left it.
        let mut newly_dirty = Vec::new();
        for (&pos, slot) in &self.chunks {
            let needs = slot.meshed_version != Some(slot.version) && !slot.mesh_in_flight;
            if needs && self.in_mesh_range(pos) {
                newly_dirty.push(pos);
            }
        }
        self.dirty.extend(newly_dirty);
        let out_of_range: Vec<IVec3> = self.dirty.iter().copied().filter(|&p| !self.in_mesh_range(p)).collect();
        for p in out_of_range {
            self.dirty.remove(&p);
        }

        let r = self.render_distance + 2;
        self.load_list.clear();
        for dz in -r..=r {
            for dx in -r..=r {
                for y in 0..WORLD_HEIGHT_CHUNKS {
                    let p = IVec3::new(center.x + dx, y, center.z + dz);
                    if self.in_load_range(p) {
                        self.load_list.push(p);
                    }
                }
            }
        }
        let mut keyed: Vec<(i32, IVec3)> = self.load_list.iter().map(|&p| (self.priority(p), p)).collect();
        keyed.sort_unstable_by_key(|&(k, _)| k);
        self.load_list = keyed.into_iter().map(|(_, p)| p).collect();
        self.load_cursor = 0;
    }

    fn insert_chunk(&mut self, pos: IVec3, data: Arc<ChunkData>, modified: bool) {
        let heights = mesh::chunk_heights(&data, pos.y * CHUNK_SIZE_I);
        let col = self.columns.entry(column_of(pos)).or_insert_with(|| Column {
            heights: Box::new([NO_HEIGHT; CHUNK_SIZE * CHUNK_SIZE]),
            loaded: 0,
        });
        col.loaded += 1;
        for (h, new) in col.heights.iter_mut().zip(heights) {
            *h = (*h).max(new);
        }
        self.chunks.insert(
            pos,
            ChunkSlot { data, modified, version: 0, meshed_version: None, mesh_in_flight: false },
        );
        if self.in_mesh_range(pos) {
            self.dirty.insert(pos);
        }
    }

    fn remove_chunk(&mut self, pos: IVec3) {
        let slot = self.chunks.remove(&pos).unwrap();
        if slot.modified {
            self.saved.insert(pos, slot.data);
        }
        if let Some(col) = self.columns.get_mut(&column_of(pos)) {
            col.loaded -= 1;
            if col.loaded <= 0 {
                self.columns.remove(&column_of(pos));
            }
        }
        self.dirty.remove(&pos);
        self.mesh_removals.push(pos);
    }

    pub fn update(&mut self, player: DVec3) {
        let center = chunk_of(player.floor().as_ivec3());
        if self.center != Some(center) {
            self.recenter(center);
        }

        let cap = self.workers.threads * 3;

        // Collect finished work.
        while let Some(result) = self.workers.try_recv() {
            match result {
                JobResult::Generated(pos, data) => {
                    self.gen_in_flight.remove(&pos);
                    if self.in_keep_range(pos) && !self.chunks.contains_key(&pos) {
                        self.insert_chunk(pos, Arc::new(data), false);
                    }
                }
                JobResult::Meshed { pos, version, mesh } => {
                    self.mesh_in_flight -= 1;
                    let in_range = self.in_mesh_range(pos);
                    let Some(slot) = self.chunks.get_mut(&pos) else { continue };
                    slot.mesh_in_flight = false;
                    if slot.version == version {
                        slot.meshed_version = Some(version);
                        self.mesh_uploads.push((pos, mesh));
                    } else if slot.meshed_version != Some(slot.version) && in_range {
                        self.dirty.insert(pos);
                    }
                }
            }
        }

        // Generate the nearest missing chunks.
        while self.gen_in_flight.len() < cap && self.load_cursor < self.load_list.len() {
            let pos = self.load_list[self.load_cursor];
            self.load_cursor += 1;
            if self.chunks.contains_key(&pos) || self.gen_in_flight.contains(&pos) {
                continue;
            }
            if let Some(data) = self.saved.remove(&pos) {
                self.insert_chunk(pos, data, true);
            } else {
                self.gen_in_flight.insert(pos);
                self.workers.submit(Job::Generate(pos));
            }
        }

        // Mesh the nearest ready chunks.
        if !self.dirty.is_empty() && self.mesh_in_flight < cap {
            let mut candidates: Vec<(i32, IVec3)> =
                self.dirty.iter().map(|&p| (self.priority(p), p)).collect();
            candidates.sort_unstable_by_key(|&(k, _)| k);
            for (_, pos) in candidates {
                if self.mesh_in_flight >= cap {
                    break;
                }
                let Some(slot) = self.chunks.get(&pos) else {
                    self.dirty.remove(&pos);
                    continue;
                };
                if slot.mesh_in_flight || !self.ready_to_mesh(pos) {
                    continue;
                }
                self.dirty.remove(&pos);
                if self.trivially_empty(pos) {
                    let slot = self.chunks.get_mut(&pos).unwrap();
                    slot.meshed_version = Some(slot.version);
                    self.mesh_uploads.push((pos, MeshData::default()));
                    continue;
                }
                let input = self.gather(pos);
                let slot = self.chunks.get_mut(&pos).unwrap();
                slot.mesh_in_flight = true;
                self.mesh_in_flight += 1;
                self.workers.submit(Job::Mesh { pos, version: slot.version, input });
            }
        }
    }

    /// All player-modified chunks, loaded or not, for saving.
    pub fn modified_chunks(&self) -> Vec<(IVec3, Arc<ChunkData>)> {
        self.chunks
            .iter()
            .filter(|(_, s)| s.modified)
            .map(|(&p, s)| (p, s.data.clone()))
            .chain(self.saved.iter().map(|(&p, d)| (p, d.clone())))
            .collect()
    }

    /// DDA voxel traversal. Returns the first solid block hit and the face
    /// normal it was entered through.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(IVec3, IVec3)> {
        let mut cell = origin.floor().as_ivec3();
        let step = IVec3::new(dir.x.signum() as i32, dir.y.signum() as i32, dir.z.signum() as i32);
        let inv = DVec3::new(
            if dir.x != 0.0 { 1.0 / dir.x.abs() } else { f64::INFINITY },
            if dir.y != 0.0 { 1.0 / dir.y.abs() } else { f64::INFINITY },
            if dir.z != 0.0 { 1.0 / dir.z.abs() } else { f64::INFINITY },
        );
        let frac = origin - origin.floor();
        let first = |f: f64, s: i32| if s > 0 { 1.0 - f } else { f };
        let mut t_max = DVec3::new(
            first(frac.x, step.x) * inv.x,
            first(frac.y, step.y) * inv.y,
            first(frac.z, step.z) * inv.z,
        );
        let mut normal = IVec3::ZERO;
        let mut t = 0.0;
        while t <= max_dist {
            if let Some(b) = self.get_block(cell) {
                if b.is_solid() && cell.y >= 0 {
                    return Some((cell, normal));
                }
            }
            if t_max.x < t_max.y && t_max.x < t_max.z {
                cell.x += step.x;
                t = t_max.x;
                t_max.x += inv.x;
                normal = IVec3::new(-step.x, 0, 0);
            } else if t_max.y < t_max.z {
                cell.y += step.y;
                t = t_max.y;
                t_max.y += inv.y;
                normal = IVec3::new(0, -step.y, 0);
            } else {
                cell.z += step.z;
                t = t_max.z;
                t_max.z += inv.z;
                normal = IVec3::new(0, 0, -step.z);
            }
        }
        None
    }
}
