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

pub mod bastion;
pub mod block;
pub mod brewing;
pub mod chest;
pub mod chunk;
pub mod dungeon;
pub mod end;
pub mod end_portal;
pub mod falling;
mod fire;
mod fluid;
pub mod forms;
pub mod fortress;
pub mod furnace;
mod growth;
mod height;
pub(crate) mod lighting;
pub mod mineshaft;
pub mod nether;
pub mod nether_blocks;
pub mod noise;
pub mod ore;
mod portal;
pub mod shape;
mod spawner;
pub mod storage;
pub mod stronghold;
pub mod structure;
pub mod terrain;

use std::sync::Arc;

use glam::{DVec3, IVec2, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::mesh::{self, D, MARGIN, MeshData, MeshInput, NO_HEIGHT, Neighborhood, Region};
use crate::workers::{Job, JobResult, Workers};
use block::{Block, RenderKind};
use chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, WORLD_HEIGHT, WORLD_HEIGHT_CHUNKS, chunk_of, local_of};
use terrain::Generator;

pub struct ChunkSlot {
    pub data: Arc<ChunkData>,
    pub modified: bool,
    version: u32,
    meshed_version: Option<u32>,
    mesh_in_flight: bool,
    /// Authoritative block light; dark chunks allocate no light array.
    block_light: Option<lighting::BlockLight>,
}

/// Per chunk-column state: skylight heightmap and how many of its chunks
/// are loaded.
struct Column {
    heights: Box<[i16; CHUNK_SIZE * CHUNK_SIZE]>,
    loaded: i32,
    /// Biome colours, once a worker has worked them out.
    foliage: Option<Box<[u8; CHUNK_SIZE * CHUNK_SIZE]>>,
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
    agent_centers: Vec<IVec3>,
    /// Chunks of other players shown in split-screen: loaded and meshed at
    /// the full render distance, like the host's.
    view_centers: Vec<IVec3>,
    render_distance: i32,
    region: Option<Box<Region>>,
    meshes_enabled: bool,
    light_updates: lighting::LightUpdates,
    fluids: fluid::FluidState,
    fire: fire::FireState,
    falling: Vec<falling::FallingBlock>,
    /// Furnace contents by position (see [`furnace`]).
    furnaces: FxHashMap<IVec3, furnace::Furnace>,
    /// Chest contents by position (see [`chest`]).
    chests: FxHashMap<IVec3, chest::Chest>,
    /// Brewing stand contents by position (see [`brewing`]).
    brewing_stands: FxHashMap<IVec3, brewing::BrewingStand>,
    /// Spawner cages and the mob each makes (see `spawner`).
    spawners: FxHashMap<IVec3, crate::entity::MobKind>,
    /// Leaves waiting to decay (seconds left) after a log near them went.
    leaf_decay: FxHashMap<IVec3, f32>,
    /// Fractional random block ticks carried over between frames.
    random_ticks: f64,
    /// Random state for growth and chance drops.
    rng: u64,
    /// Java `doTileDrops`; block and block-entity removal still occurs when false.
    tile_drops: bool,
    /// Items the world let go of (mined blocks, container contents, plants
    /// that popped off or washed away, explosion debris) and the cell they
    /// came from; the game turns them into dropped items.
    pub drops: Vec<(IVec3, crate::inventory::Stack)>,
    /// Bounded client visual requests, also emitted by headless player actions.
    pub particles: crate::particles::Requests,
    /// Experience released at a block (a broken furnace's store); the game
    /// turns it into orbs.
    pub xp_drops: Vec<(IVec3, u32)>,
    /// Brewing stands that finished a brew; the game plays its sound.
    pub brews_done: Vec<IVec3>,
    /// TNT blocks a blast or fire took out, with whether to shorten the
    /// fuse (blasts only); the game turns them into entities.
    pub primed_tnt: Vec<(IVec3, bool)>,
    /// Whether it's raining (set by the game each frame).
    pub raining: bool,
    pub mesh_uploads: Vec<(IVec3, MeshData)>,
    pub mesh_removals: Vec<IVec3>,
}

fn column_of(chunk: IVec3) -> IVec2 {
    IVec2::new(chunk.x, chunk.z)
}

impl World {
    /// Create a world that generates terrain and submits render meshes as `update` polls streaming.
    /// `saved` contains edited chunks; `render_distance` is measured in 32-block chunks.
    pub fn new(generator: Arc<Generator>, saved: FxHashMap<IVec3, Arc<ChunkData>>, render_distance: i32) -> Self {
        Self::with_meshing(generator, saved, render_distance, true)
    }

    /// Stream and simulate terrain without submitting render-mesh jobs or
    /// allocating meshing scratch space. Gameplay light is maintained by
    /// the same authority as a graphical world.
    pub fn new_headless(generator: Arc<Generator>, saved: FxHashMap<IVec3, Arc<ChunkData>>, distance: i32) -> Self {
        Self::with_meshing(generator, saved, distance, false)
    }

    /// Initialize shared world state and workers, selecting whether streaming also builds render meshes.
    fn with_meshing(
        generator: Arc<Generator>,
        saved: FxHashMap<IVec3, Arc<ChunkData>>,
        render_distance: i32,
        meshes_enabled: bool,
    ) -> Self {
        let rng = generator.seed ^ 0x6772_6f77;
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
            agent_centers: Vec::new(),
            view_centers: Vec::new(),
            render_distance,
            region: None,
            meshes_enabled,
            light_updates: Default::default(),
            fluids: Default::default(),
            fire: Default::default(),
            falling: Vec::new(),
            furnaces: FxHashMap::default(),
            chests: FxHashMap::default(),
            brewing_stands: FxHashMap::default(),
            spawners: FxHashMap::default(),
            leaf_decay: FxHashMap::default(),
            random_ticks: 0.0,
            rng,
            tile_drops: true,
            drops: Vec::new(),
            particles: Default::default(),
            xp_drops: Vec::new(),
            brews_done: Vec::new(),
            primed_tnt: Vec::new(),
            raining: false,
            mesh_uploads: Vec::new(),
            mesh_removals: Vec::new(),
        }
    }

    pub fn render_distance(&self) -> i32 {
        self.render_distance
    }

    pub fn set_tile_drops(&mut self, enabled: bool) {
        self.tile_drops = enabled;
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

    /// No generation, meshing or water flow left to do.
    pub fn is_idle(&self) -> bool {
        self.pending_jobs() == 0
            && self.dirty.is_empty()
            && self.active_fluids() == 0
            && self.falling.is_empty()
            && self.light_updates.is_empty()
    }

    pub fn pending_jobs(&self) -> usize {
        self.gen_in_flight.len() + self.mesh_in_flight
    }

    pub fn worker_threads(&self) -> usize {
        self.workers.threads
    }

    /// Horizontal chunk distance² to the nearest view (the host's or a
    /// split-screen player's).
    fn horizontal_dist2(&self, pos: IVec3) -> i32 {
        let c = self.center.unwrap_or(IVec3::ZERO);
        std::iter::once(c)
            .chain(self.view_centers.iter().copied())
            .map(|c| {
                let (dx, dz) = (pos.x - c.x, pos.z - c.z);
                dx * dx + dz * dz
            })
            .min()
            .unwrap_or(0)
    }

    fn priority(&self, pos: IVec3) -> i32 {
        let c = self.center.unwrap_or(IVec3::ZERO);
        let dy = (pos.y - c.y) * 3 / 2;
        self.horizontal_dist2(pos) * 2 + dy * dy
    }

    /// Chunks within this radius are meshed and drawn.
    fn in_mesh_range(&self, pos: IVec3) -> bool {
        self.meshes_enabled && self.horizontal_dist2(pos) <= self.render_distance * self.render_distance
    }

    /// Loaded radius is one ring larger than the mesh radius so every meshed
    /// chunk has all of its neighbours.
    fn in_load_range(&self, pos: IVec3) -> bool {
        let r = self.render_distance as f32 + 1.5;
        (self.horizontal_dist2(pos) as f32) <= r * r
            || self.agent_centers.iter().any(|c| {
                let d = (pos - *c).with_y(0);
                d.length_squared() <= 16
            })
    }

    /// Extra hysteresis before unloading so walking back and forth across a
    /// chunk border doesn't thrash.
    fn in_keep_range(&self, pos: IVec3) -> bool {
        let r = self.render_distance + 3;
        self.horizontal_dist2(pos) <= r * r
            || self.agent_centers.iter().any(|c| {
                let d = (pos - *c).with_y(0);
                d.length_squared() <= 25
            })
    }

    /// Whether every chunk of the column holding `(x, z)` is loaded.
    pub fn column_loaded(&self, x: i32, z: i32) -> bool {
        let key = column_of(chunk_of(IVec3::new(x, 0, z)));
        self.columns.get(&key).is_some_and(|c| c.loaded == WORLD_HEIGHT_CHUNKS)
    }

    pub fn is_loaded(&self, block: IVec3) -> bool {
        block.y < 0 || block.y >= WORLD_HEIGHT || self.chunks.contains_key(&chunk_of(block))
    }

    /// Highest light-blocking block (including leaves and water) in a fully
    /// loaded column; `None` if the column isn't loaded or is empty.
    pub fn surface_height(&self, x: i32, z: i32) -> Option<i32> {
        let key = column_of(chunk_of(IVec3::new(x, 0, z)));
        let col = self.columns.get(&key).filter(|c| c.loaded == WORLD_HEIGHT_CHUNKS)?;
        let l = local_of(IVec3::new(x, 0, z));
        let h = col.heights[(l.x + l.z * CHUNK_SIZE_I) as usize];
        (h != NO_HEIGHT).then_some(h as i32)
    }

    /// Foliage colour group of a column (see `terrain::Biome::foliage`),
    /// once known.
    pub fn foliage_at(&self, x: i32, z: i32) -> Option<u8> {
        let col = self.columns.get(&column_of(chunk_of(IVec3::new(x, 0, z))))?;
        let l = local_of(IVec3::new(x, 0, z));
        col.foliage.as_ref().map(|f| f[(l.x + l.z * CHUNK_SIZE_I) as usize])
    }

    /// Whether rain (not snow) is falling on cell `p` right now.
    pub fn rains_on(&self, p: IVec3) -> bool {
        // Match the weather renderer: dry biomes stay clear; cold biomes
        // and columns whose surface is above the snow line get snow.
        self.raining
            && self.generator.dimension.has_sky()
            && self.surface_height(p.x, p.z).is_none_or(|h| h <= 150)
            && self.sky_exposed(p)
            && matches!(self.foliage_at(p.x, p.z), Some(0 | 1 | 3))
    }

    /// Whether a cell sees the sky straight up (nothing light-blocking
    /// above it). Unloaded columns count as exposed.
    pub fn sky_exposed(&self, p: IVec3) -> bool {
        self.surface_height(p.x, p.z).is_none_or(|h| p.y > h)
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
        self.chunks.get(&chunk_of(p)).map(|s| s.data.get(l.x as usize, l.y as usize, l.z as usize))
    }

    /// Player edit. Chunks whose geometry changes are remeshed synchronously
    /// so the edit shows up this frame; chunks whose lighting changes are
    /// remeshed on the workers. Nearby fluid is woken up to flow, and
    /// unsupported sand, gravel and plants fall or pop off.
    pub fn set_block(&mut self, p: IVec3, block: Block) -> bool {
        let old = self.get_block(p);
        let changed = self.edit(p, block, true);
        if changed
            && block == Block::AIR
            && let Some(old) = old.filter(|b| *b != Block::AIR && !b.is_fluid())
        {
            self.particles.push(crate::particles::Request::Break { cell: p, block: old });
        }
        if changed {
            self.settle(p);
            self.break_unsupported_portals(p);
        }
        self.update_block_light();
        changed
    }

    /// Changes a block and schedules remeshing of every chunk whose geometry
    /// or lighting depends on it (synchronously for geometry if `sync`).
    fn edit(&mut self, p: IVec3, block: Block, sync: bool) -> bool {
        if p.y < 0 || p.y >= WORLD_HEIGHT {
            return false;
        }
        let cpos = chunk_of(p);
        let l = local_of(p);
        let Some(slot) = self.chunks.get_mut(&cpos) else { return false };
        let old = slot.data.get(l.x as usize, l.y as usize, l.z as usize);
        let old_light =
            slot.block_light.as_ref().map_or(0, |light| light.get(l.x as usize, l.y as usize, l.z as usize));
        Arc::make_mut(&mut slot.data).set(l.x as usize, l.y as usize, l.z as usize, block);
        slot.modified = true;
        // Fire ages are saved state, but all ages have identical geometry
        // and lighting: don't invalidate meshes or copy light snapshots.
        if old.is_fire() && block.is_fire() {
            return true;
        }
        self.light_block_changed(p, old, block, old_light);
        self.track_fire(p, old, block);
        self.track_furnace(p, old, block);
        self.track_chest(p, old, block);
        self.track_brewing_stand(p, old, block);
        self.track_spawner(p, old, block);
        if old.is_log() && !block.is_log() {
            self.log_removed(p);
        }
        self.extinguish_unsupported_fire(p);

        // Keep the column heightmap current.
        let (old_h, new_h) = self.update_height(p, block);
        if !self.meshes_enabled {
            return true;
        }

        // Light reaches 15 blocks, plus 1 for the face-adjacent sample cell;
        // a heightmap change also re-exposes everything between old and new.
        let reach = MARGIN as i32 + 1;
        let lo_y = p.y.min(old_h).min(new_h).max(0) - reach;
        let hi_y = p.y.max(old_h).max(new_h) + reach;
        let lo = chunk_of(IVec3::new(p.x - reach, lo_y, p.z - reach)).max(IVec3::new(i32::MIN, 0, i32::MIN));
        let hi = chunk_of(IVec3::new(p.x + reach, hi_y, p.z + reach)).min(IVec3::new(
            i32::MAX,
            WORLD_HEIGHT_CHUNKS - 1,
            i32::MAX,
        ));
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
                    if sync && geometry(c) {
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
        if !self.generator.dimension.has_sky() {
            return (p.y, p.y);
        }
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
    /// loaded (skylight needs complete heightmaps) and its own column's
    /// biome colours are known.
    fn ready_to_mesh(&self, pos: IVec3) -> bool {
        self.columns.get(&column_of(pos)).is_some_and(|c| c.foliage.is_some())
            && (-1..=1).all(|dz| {
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
                    neighbors[i as usize] = self.chunks.get(&(pos + IVec3::new(dx, dy, dz))).map(|s| s.data.clone());
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
        let foliage = cols[4].and_then(|c| c.foliage.clone()).unwrap_or_else(|| Box::new([0; CHUNK_SIZE * CHUNK_SIZE]));
        Box::new(MeshInput { neighbors, heights, base_y: pos.y * CHUNK_SIZE_I, foliage })
    }

    /// Build a ready chunk's mesh immediately and queue it for upload; defer unready chunks.
    /// Headless worlds skip both paths.
    fn remesh_now(&mut self, pos: IVec3) {
        if !self.meshes_enabled {
            return;
        }
        if !self.in_mesh_range(pos) || !self.ready_to_mesh(pos) {
            self.dirty.insert(pos);
            return;
        }
        let input = self.gather(pos);
        let mesh = mesh::build(&input, self.region.get_or_insert_with(Box::default));
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
            Some(b) if b.is_opaque() => {
                [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z].iter().all(|&o| {
                    let n = pos + o;
                    n.y < 0 || self.chunks.get(&n).and_then(|s| s.data.uniform()).is_some_and(|b| b.is_opaque())
                })
            }
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
        for c in std::iter::once(center).chain(self.view_centers.clone()) {
            for dz in -r..=r {
                for dx in -r..=r {
                    for y in 0..WORLD_HEIGHT_CHUNKS {
                        let p = IVec3::new(c.x + dx, y, c.z + dz);
                        if self.in_load_range(p) {
                            self.load_list.push(p);
                        }
                    }
                }
            }
        }
        for c in &self.agent_centers {
            for dz in -4..=4 {
                for dx in -4..=4 {
                    if dx * dx + dz * dz > 16 {
                        continue;
                    }
                    for y in 0..WORLD_HEIGHT_CHUNKS {
                        self.load_list.push(IVec3::new(c.x + dx, y, c.z + dz));
                    }
                }
            }
        }
        self.load_list.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        self.load_list.dedup();
        let mut keyed: Vec<(i32, IVec3)> = self.load_list.iter().map(|&p| (self.priority(p), p)).collect();
        keyed.sort_unstable_by_key(|&(k, _)| k);
        self.load_list = keyed.into_iter().map(|(_, p)| p).collect();
        self.load_cursor = 0;
    }

    /// Install chunk data, seed gameplay light and update column state.
    /// Queue render work when in mesh range; saved chunks also restore scheduled fire.
    fn insert_chunk(&mut self, pos: IVec3, data: Arc<ChunkData>, modified: bool) {
        if modified {
            self.load_fires(pos, &data);
        } else {
            self.register_structure_features(pos, &data);
        }
        let heights = mesh::chunk_heights(&data, pos.y * CHUNK_SIZE_I);
        let workers = &self.workers;
        let col = self.columns.entry(column_of(pos)).or_insert_with(|| {
            workers.submit(Job::Foliage(column_of(pos)));
            Column { heights: Box::new([NO_HEIGHT; CHUNK_SIZE * CHUNK_SIZE]), loaded: 0, foliage: None }
        });
        col.loaded += 1;
        // Without a sky (the Nether) every cell counts as open: the
        // dimension's dim, even light is daylight at a fixed low level.
        if self.generator.dimension.has_sky() {
            for (h, new) in col.heights.iter_mut().zip(heights) {
                *h = (*h).max(new);
            }
        }
        self.chunks.insert(
            pos,
            ChunkSlot { data, modified, version: 0, meshed_version: None, mesh_in_flight: false, block_light: None },
        );
        self.load_block_light(pos);
        if self.in_mesh_range(pos) {
            self.dirty.insert(pos);
        }
    }

    /// Unload a present chunk, retaining edited data and queuing removal of its outgoing light.
    /// Only graphical worlds emit a renderer removal message.
    fn remove_chunk(&mut self, pos: IVec3) {
        let slot = self.chunks.remove(&pos).unwrap();
        self.unload_block_light(pos, slot.block_light.as_ref());
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
        if self.meshes_enabled {
            self.mesh_removals.push(pos);
        }
    }

    /// Poll terrain/mesh workers and stream chunks around the player without advancing game time.
    /// Resolve gameplay light before scheduling meshes; drain render messages separately.
    pub fn update(&mut self, player: DVec3) {
        self.update_players(player, &[]);
    }

    /// Stream the host view and the union of agents' smaller simulation ranges.
    /// Agent-only chunks remain unmeshed outside the host's view distance.
    /// Other players seen in split-screen views: their surroundings load and
    /// mesh at the render distance too (applied on the next update).
    pub fn set_viewers(&mut self, viewers: &[DVec3]) {
        let mut centers: Vec<_> = viewers.iter().map(|p| chunk_of(p.floor().as_ivec3())).collect();
        centers.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        centers.dedup();
        if centers != self.view_centers {
            self.view_centers = centers;
            self.center = None;
        }
    }

    pub fn update_players(&mut self, player: DVec3, agents: &[DVec3]) {
        let mut centers: Vec<_> = agents.iter().map(|p| chunk_of(p.floor().as_ivec3())).collect();
        centers.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        centers.dedup();
        if centers != self.agent_centers {
            self.agent_centers = centers;
            self.center = None;
        }
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
                JobResult::Foliage(key, foliage) => {
                    if let Some(col) = self.columns.get_mut(&key) {
                        col.foliage = Some(foliage);
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

        self.update_block_light();

        // Mesh the nearest ready chunks.
        if !self.dirty.is_empty() && self.mesh_in_flight < cap {
            let mut candidates: Vec<(i32, IVec3)> = self.dirty.iter().map(|&p| (self.priority(p), p)).collect();
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

    /// DDA voxel traversal. Returns the first targetable block hit and the
    /// face normal it was entered through.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(IVec3, IVec3)> {
        self.raycast_by(origin, dir, max_dist, Block::is_targetable)
    }

    /// The selection outline of the block at `p`: min and max corners
    /// relative to the cell (low blocks and shaped blocks are smaller).
    pub fn outline(&self, p: IVec3) -> ([f32; 3], [f32; 3]) {
        let Some(b) = self.get_block(p) else { return ([0.0; 3], [1.0; 3]) };
        if b.kind() == RenderKind::Shaped {
            let neighbour = |f: block::Facing| self.get_block(p + f.offset()).unwrap_or(Block::AIR);
            let below = self.get_block(p - IVec3::Y).unwrap_or(block::Block::AIR);
            if let Some(bx) = shape::shape(b, neighbour, below).bounds() {
                return (bx.min.map(|c| c as f32 / 16.0), bx.max.map(|c| c as f32 / 16.0));
            }
        }
        ([0.0; 3], [1.0, b.height() as f32, 1.0])
    }

    /// Like [`World::raycast`], but also stops at water and lava sources
    /// (for buckets).
    pub fn raycast_sources(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(IVec3, IVec3)> {
        self.raycast_by(origin, dir, max_dist, |b| b.is_targetable() || b == Block::WATER || b == Block::LAVA)
    }

    fn raycast_by(
        &self,
        origin: DVec3,
        dir: DVec3,
        max_dist: f64,
        hits: impl Fn(Block) -> bool,
    ) -> Option<(IVec3, IVec3)> {
        let mut cell = origin.floor().as_ivec3();
        let step = IVec3::new(dir.x.signum() as i32, dir.y.signum() as i32, dir.z.signum() as i32);
        let inv = DVec3::new(
            if dir.x != 0.0 { 1.0 / dir.x.abs() } else { f64::INFINITY },
            if dir.y != 0.0 { 1.0 / dir.y.abs() } else { f64::INFINITY },
            if dir.z != 0.0 { 1.0 / dir.z.abs() } else { f64::INFINITY },
        );
        let frac = origin - origin.floor();
        let first = |f: f64, s: i32| if s > 0 { 1.0 - f } else { f };
        let mut t_max =
            DVec3::new(first(frac.x, step.x) * inv.x, first(frac.y, step.y) * inv.y, first(frac.z, step.z) * inv.z);
        let mut normal = IVec3::ZERO;
        let mut t = 0.0;
        while t <= max_dist {
            if let Some(b) = self.get_block(cell)
                && hits(b)
                && cell.y >= 0
            {
                if b.kind() != RenderKind::Shaped {
                    return Some((cell, normal));
                }
                // Shaped blocks only count where the ray meets their boxes.
                if let Some((hit, face)) = crate::physics::ray_shape(self, cell, origin, dir)
                    && hit <= max_dist
                {
                    return Some((cell, if face == IVec3::ZERO { normal } else { face }));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{MoveInput, Player};
    use std::time::{Duration, Instant};

    /// Streams the world around `at` until all jobs are done.
    fn settled_world(at: DVec3) -> World {
        let generator = Arc::new(Generator::new(7));
        let mut world = World::new(generator, Default::default(), 3);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            world.update(at);
            world.mesh_uploads.clear();
            if world.loaded_chunks() > 0 && world.pending_jobs() == 0 && world.dirty.is_empty() {
                return world;
            }
            assert!(Instant::now() < deadline, "world never settled");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn the_dead_dragon_opens_the_exit_portal_and_leaves_an_egg_that_jumps() {
        let generator = Arc::new(Generator::for_dimension(3, super::terrain::Dimension::End));
        let origin = generator.end().unwrap().podium();
        let mut world = World::new_headless(generator, Default::default(), 2);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !(-1..=1).all(|c| (-1..=1).all(|d| world.column_loaded(c * 16, d * 16))) {
            world.update(origin.as_dvec3());
            assert!(Instant::now() < deadline, "End never loaded");
            std::thread::sleep(Duration::from_millis(1));
        }
        // Generated shut: bedrock rim and pillar, nothing in the bowl.
        assert_eq!(world.get_block(origin + IVec3::new(3, 0, 0)), Some(Block::BEDROCK));
        assert_eq!(world.get_block(origin + IVec3::new(1, 0, 1)), Some(Block::AIR));
        assert_eq!(world.get_block(origin + IVec3::Y * 3), Some(Block::BEDROCK));
        world.open_exit_portal(true);
        let portal = (-2..=2)
            .flat_map(|z| (-2..=2).map(move |x| IVec3::new(x, 0, z)))
            .filter(|&d| world.get_block(origin + d) == Some(Block::END_PORTAL))
            .count();
        // Java's 5x5 disc less its corners and the pillar.
        assert_eq!(portal, 20);
        let egg = origin + IVec3::Y * 4;
        assert_eq!(world.get_block(egg), Some(Block::DRAGON_EGG));
        let to = world.teleport_egg(egg).expect("the egg should find room");
        assert_eq!(world.get_block(egg), Some(Block::AIR));
        assert!((to - egg).abs().max_element() <= 15);
    }

    fn surface_y(world: &World, x: i32, z: i32) -> i32 {
        (0..WORLD_HEIGHT).rev().find(|&y| world.get_block(IVec3::new(x, y, z)).unwrap().is_solid()).unwrap()
    }

    #[test]
    fn fortress_chunks_register_loot_chests_and_blaze_spawners() {
        use super::fortress::{Fortresses, Kind};
        let fortress = Fortresses::new(1).get(IVec2::ZERO).unwrap();
        let generator = Arc::new(Generator::for_dimension(1, super::terrain::Dimension::Nether));
        let mut found = (None, None);
        for piece in &fortress.pieces {
            let chunk = chunk_of(piece.bounds.min);
            for (p, f) in generator.structure_features(chunk) {
                match f {
                    super::fortress::Feature::Chest(_) if found.0.is_none() => found.0 = Some(p),
                    super::fortress::Feature::Spawner(_) if piece.kind == Kind::Throne => found.1 = Some(p),
                    _ => {}
                }
            }
        }
        let (chest, cage) = (found.0.expect("a loot chest"), found.1.expect("a throne"));
        for (at, check) in [(chest, 0), (cage, 1)] {
            let mut world = World::new_headless(generator.clone(), Default::default(), 1);
            let deadline = Instant::now() + Duration::from_secs(20);
            while world.get_block(at).is_none() || world.pending_jobs() > 0 {
                world.update(at.as_dvec3());
                assert!(Instant::now() < deadline, "fortress never loaded");
                std::thread::sleep(Duration::from_millis(1));
            }
            if check == 0 {
                assert!(super::chest::is_chest(world.get_block(at).unwrap()));
                let filled = world.chest(at).unwrap().slots.iter().flatten().count();
                assert!((2..=4).contains(&filled), "{filled} stacks");
                // A looted chest stays empty when its chunk generates again.
                world.chest_mut(at).unwrap().slots = [None; super::chest::SLOTS];
                let data = world.chunks[&chunk_of(at)].data.clone();
                world.register_structure_features(chunk_of(at), &data);
                assert!(world.chest(at).unwrap().slots.iter().all(Option::is_none));
            } else {
                assert_eq!(world.get_block(at), Some(Block::SPAWNER));
                assert_eq!(world.spawner(at), Some(crate::entity::MobKind::Blaze));
            }
        }
    }

    #[test]
    fn spawners_require_loaded_cage_blocks() {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        let cage = IVec3::new(1, 160, 1);
        let stale = IVec3::new(2, 160, 1);
        world.load_spawners("1,160,1=blaze|2,160,1=zombie|40,160,1=blaze");
        assert!(world.spawners().is_empty(), "saved cages in unloaded chunks stay inactive");
        world.insert_chunk(chunk_of(cage), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        world.set_block(cage, Block::SPAWNER);
        assert_eq!(world.spawner(cage), Some(crate::entity::MobKind::Blaze), "saved kind survives loading");
        assert_eq!(world.spawners(), vec![(cage, crate::entity::MobKind::Blaze)]);
        world.set_block(stale, Block::STONE);
        assert_eq!(world.spawners(), vec![(cage, crate::entity::MobKind::Blaze)], "stale entries stay inactive");
        world.set_block(cage, Block::AIR);
        assert!(world.spawners().is_empty(), "breaking a cage stops spawning");
    }

    #[test]
    fn unloaded_agents_preserve_survival_inventory_and_timed_commands() {
        use crate::agent::{Agent, Command};
        use crate::entity::Entities;
        use crate::item::Item;
        use crate::simulation::survival::{Env, Hunger};

        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        let pos = DVec3::new(1.5, 160.0, 1.5);
        let mut agent = Agent::new(pos);
        let mut entities = Entities::new(7);
        agent.inventory.add(Item::DIAMOND, 3);
        agent.vitals.tick(0.5, &Env { in_fire: true, ..Default::default() }, false);
        agent.vitals.health = 1.0;
        agent.vitals.hunger = Hunger::restore(0.0, 0.0, 0.0);
        agent.execute(Command::parse("wait 20").unwrap(), &mut world, &mut entities, &[]).unwrap();
        for feet_loaded in [false, true] {
            if feet_loaded {
                world.insert_chunk(IVec3::new(0, 5, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
            }
            assert_eq!(world.is_loaded(pos.floor().as_ivec3()), feet_loaded);
            assert!(!world.is_loaded(pos.floor().as_ivec3() - IVec3::Y));
            agent.previous_pos = pos - DVec3::X;
            for _ in 0..25 {
                agent.tick(&mut world, &mut entities);
            }
            assert_eq!(agent.previous_pos, pos);
            assert_eq!(agent.player.pos, pos);
            assert_eq!(agent.remaining, 20);
            assert_eq!(agent.vitals.health, 1.0);
            assert_eq!(agent.inventory.get(0).unwrap().count, 3);
            assert!(agent.vitals.burning());
            assert!(entities.items.is_empty());
        }
        world.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::STONE)), false);
        agent.tick(&mut world, &mut entities);
        assert_eq!(agent.remaining, 19, "loaded agents resume timed input");
        for _ in 0..25 {
            agent.tick(&mut world, &mut entities);
        }
        assert!(agent.vitals.is_dead(), "loaded agents resume survival damage");
        assert!(agent.inventory.get(0).is_none());
        assert_eq!(entities.items.iter().map(|i| i.stack.count as u32).sum::<u32>(), 3);
    }

    #[test]
    fn edits_raycast_and_physics() {
        let spawn = Generator::new(7).find_spawn();
        let world_pos = spawn.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let mut world = settled_world(world_pos);
        let (x, z) = (spawn.x, spawn.z);
        let ground = surface_y(&world, x, z);
        world.set_block(IVec3::new(x, ground + 1, z), Block::AIR); // any tall grass

        // Looking straight down from above hits the surface block.
        let eye = DVec3::new(x as f64 + 0.5, ground as f64 + 4.5, z as f64 + 0.5);
        let (hit, normal) = world.raycast(eye, DVec3::NEG_Y, 10.0).expect("ray should hit ground");
        assert_eq!(hit, IVec3::new(x, ground, z));
        assert_eq!(normal, IVec3::Y);

        // Placing a block on top is visible to both reads and the raycast.
        assert!(world.set_block(hit + normal, Block::GLOWSTONE));
        assert_eq!(world.get_block(hit + normal), Some(Block::GLOWSTONE));
        assert_eq!(world.raycast(eye, DVec3::NEG_Y, 10.0).unwrap().0, hit + normal);
        assert!(!world.mesh_uploads.is_empty(), "edit should remesh synchronously");
        assert!(world.modified_chunks().iter().any(|(p, _)| *p == chunk_of(hit)));

        // The heightmap follows the edit up and back down.
        let col = column_of(chunk_of(hit));
        let i = ((x & 31) + (z & 31) * 32) as usize;
        assert_eq!(world.columns[&col].heights[i] as i32, ground + 1);
        world.set_block(hit + normal, Block::AIR);
        assert_eq!(world.columns[&col].heights[i] as i32, surface_y(&world, x, z));

        // A player dropped from above lands on the ground and stays there.
        let mut player = Player::new(DVec3::new(x as f64 + 0.5, ground as f64 + 6.0, z as f64 + 0.5));
        for _ in 0..240 {
            player.update(1.0 / 60.0, MoveInput::default(), &world);
        }
        let top = surface_y(&world, x, z) as f64 + 1.0;
        assert!(player.on_ground, "player should land");
        assert!((player.pos.y - top).abs() < 0.01, "feet at {} vs ground {top}", player.pos.y);

        // Walking into a wall stops at the wall.
        let wall_x = x + 2;
        for y in 0..3 {
            world.set_block(IVec3::new(wall_x, top as i32 + y, z), Block::STONE);
        }
        let walk = MoveInput { forward: 1.0, ..Default::default() };
        player.yaw = 0.0; // facing +X
        for _ in 0..120 {
            player.update(1.0 / 60.0, walk, &world);
        }
        assert!(player.pos.x <= wall_x as f64 - 0.3 + 1e-3, "walked through wall: x={}", player.pos.x);
        assert!(player.pos.x > wall_x as f64 - 0.4);

        // On a pillar, sneaking stops at the edge; walking falls off.
        let pillar = IVec3::new(x, top as i32 + 3, z);
        world.set_block(pillar, Block::STONE);
        for (sneak, stays) in [(true, true), (false, false)] {
            let mut player = Player::new(pillar.as_dvec3() + DVec3::new(0.5, 1.0, 0.5));
            player.yaw = 0.7; // diagonal, toward +X +Z
            let input = MoveInput { forward: 1.0, descend: sneak, ..Default::default() };
            for _ in 0..120 {
                player.update(1.0 / 60.0, input, &world);
            }
            let on_pillar = (player.pos.y - (pillar.y as f64 + 1.0)).abs() < 0.01;
            assert_eq!(on_pillar, stays, "sneak {sneak}: {}", player.pos);
            if sneak {
                // Leaning out over the edge, but no further than the box allows.
                assert!(player.pos.x > pillar.x as f64 + 1.0 && player.pos.x < pillar.x as f64 + 1.3 + 1e-6);
                assert!(player.eye().y < player.pos.y + crate::entity::model::PlayerPose::STANDING_EYE - 0.2);
            }
        }
    }

    #[test]
    fn plants_are_targeted_pop_off_and_wash_away() {
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -4..=4 {
            for z in -4..=4 {
                world.edit(IVec3::new(x, y, z), Block::DIRT, false);
            }
        }
        let flower = IVec3::new(0, y + 1, 0);
        assert!(world.set_block(flower, Block::POPPY));
        // The crosshair selects the flower, not the dirt under it.
        let eye = DVec3::new(0.5, y as f64 + 4.5, 0.5);
        assert_eq!(world.raycast(eye, DVec3::NEG_Y, 10.0).unwrap().0, flower);

        // Removing the dirt pops the flower off as an item.
        world.set_block(flower - IVec3::Y, Block::AIR);
        assert_eq!(world.get_block(flower), Some(Block::AIR));
        let dropped = |world: &mut World| std::mem::take(&mut world.drops).into_iter().map(|(_, s)| s.item).collect();
        let poppy: Vec<crate::item::Item> = dropped(&mut world);
        assert_eq!(poppy, [Block::POPPY.into()]);

        // Water flowing past a torch washes it away (refill the hole first,
        // or the water would head for that drop instead).
        world.set_block(flower - IVec3::Y, Block::DIRT);
        let torch = IVec3::new(3, y + 1, 0);
        world.set_block(torch, Block::TORCH);
        world.set_block(IVec3::new(2, y + 1, 0), Block::WATER);
        for _ in 0..20 {
            world.tick_fluids(0.25);
        }
        assert!(world.get_block(torch).unwrap().is_water());
        let torch: Vec<crate::item::Item> = dropped(&mut world);
        assert_eq!(torch, [Block::TORCH.into()], "washed away as an item");
    }

    #[test]
    fn water_spreads_seven_blocks_and_dries_up() {
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        // A 21x21 stone platform high in the sky.
        let y = 200;
        for x in -10..=10 {
            for z in -10..=10 {
                world.edit(IVec3::new(x, y, z), Block::STONE, false);
            }
        }
        let tick = |world: &mut World, n: usize| {
            for _ in 0..n {
                world.tick_fluids(0.25);
            }
        };
        let at = |world: &World, x: i32, z: i32| world.get_block(IVec3::new(x, y + 1, z)).unwrap();

        world.set_block(IVec3::new(0, y + 1, 0), Block::WATER);
        tick(&mut world, 30);
        assert_eq!(at(&world, 0, 0), Block::WATER);
        assert_eq!(at(&world, 1, 0).water_level(), Some(1));
        assert_eq!(at(&world, 7, 0).water_level(), Some(7));
        assert_eq!(at(&world, 3, 4).water_level(), Some(7), "spreads in a diamond");
        assert_eq!(at(&world, 8, 0), Block::AIR);
        assert_eq!(world.active_fluids(), 0, "flow should settle");

        // Water pours off the platform edge once it reaches it.
        world.set_block(IVec3::new(9, y + 1, 0), Block::WATER);
        tick(&mut world, 12);
        assert!(world.get_block(IVec3::new(11, y, 0)).unwrap().is_water(), "should pour over the edge");
        assert!(world.get_block(IVec3::new(11, y - 5, 0)).unwrap().is_water(), "and fall");

        // Removing both sources dries everything up.
        world.set_block(IVec3::new(0, y + 1, 0), Block::AIR);
        world.set_block(IVec3::new(9, y + 1, 0), Block::AIR);
        tick(&mut world, 60);
        for x in -10..=10 {
            for z in -10..=10 {
                assert_eq!(at(&world, x, z), Block::AIR, "water left at {x},{z}");
            }
        }
        assert!(!world.get_block(IVec3::new(11, y - 5, 0)).unwrap().is_water());

        // Two adjacent sources fill a 1x3 trench's middle into a new source.
        for x in 0..3 {
            world.edit(IVec3::new(x, y + 1, 5), Block::AIR, false);
            world.edit(IVec3::new(x, y + 1, 4), Block::STONE, false);
            world.edit(IVec3::new(x, y + 1, 6), Block::STONE, false);
        }
        world.set_block(IVec3::new(0, y + 1, 5), Block::WATER);
        world.set_block(IVec3::new(2, y + 1, 5), Block::WATER);
        tick(&mut world, 10);
        assert_eq!(at(&world, 1, 5), Block::WATER, "infinite source");
    }

    #[test]
    fn lava_spreads_slowly_and_hardens_against_water() {
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -10..=10 {
            for z in -10..=10 {
                world.edit(IVec3::new(x, y, z), Block::STONE, false);
            }
        }
        let tick = |world: &mut World, n: usize| {
            for _ in 0..n {
                world.tick_fluids(0.25);
            }
        };
        let at = |world: &World, x: i32, z: i32| world.get_block(IVec3::new(x, y + 1, z)).unwrap();

        // Lava moves once per six water ticks and stops after three blocks.
        world.set_block(IVec3::new(0, y + 1, 0), Block::LAVA);
        tick(&mut world, 5);
        assert_eq!(at(&world, 1, 0), Block::AIR, "lava is slow");
        tick(&mut world, 60);
        assert_eq!(at(&world, 1, 0), Block::flowing_lava(1));
        assert_eq!(at(&world, 3, 0), Block::flowing_lava(3));
        assert_eq!(at(&world, 4, 0), Block::AIR);

        // Water reaching flowing lava turns it to cobblestone, and the
        // source it touches to obsidian.
        world.set_block(IVec3::new(-1, y + 1, 0), Block::WATER);
        world.set_block(IVec3::new(4, y + 1, 0), Block::WATER);
        tick(&mut world, 12);
        assert_eq!(at(&world, 0, 0), Block::OBSIDIAN);
        assert_eq!(at(&world, 3, 0), Block::COBBLESTONE);

        // Lava pouring onto water turns the water to stone.
        for x in 5..=7 {
            world.edit(IVec3::new(x, y + 1, 8), Block::STONE, false);
        }
        world.set_block(IVec3::new(6, y + 2, 8), Block::WATER);
        world.set_block(IVec3::new(6, y + 3, 8), Block::LAVA);
        tick(&mut world, 12);
        assert_eq!(world.get_block(IVec3::new(6, y + 2, 8)), Some(Block::STONE));
    }

    #[test]
    fn sand_and_gravel_fall_and_stack() {
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -2..=2 {
            for z in -2..=2 {
                world.edit(IVec3::new(x, y, z), Block::STONE, false);
            }
        }
        let fall = |world: &mut World| {
            for _ in 0..180 {
                world.tick_falling(1.0 / 60.0);
            }
        };
        let at = |world: &World, x: i32, y: i32| world.get_block(IVec3::new(x, y, 0)).unwrap();

        // Sand placed in mid-air leaves the grid and lands on the platform.
        world.set_block(IVec3::new(0, y + 6, 0), Block::SAND);
        assert_eq!(at(&world, 0, y + 6), Block::AIR);
        assert_eq!(world.falling_blocks().len(), 1);
        fall(&mut world);
        assert_eq!(at(&world, 0, y + 1), Block::SAND);
        assert!(world.falling_blocks().is_empty());

        // Knocking out a pedestal drops the whole column in order; the dead
        // bush on top pops off.
        let column = [Block::DIRT, Block::GRAVEL, Block::GRAVEL, Block::SAND, Block::DEAD_BUSH];
        for (i, &b) in column.iter().enumerate() {
            world.edit(IVec3::new(1, y + 1 + i as i32, 0), b, false);
        }
        world.set_block(IVec3::new(1, y + 1, 0), Block::AIR);
        assert_eq!(world.falling_blocks().len(), 3);
        assert_eq!(at(&world, 1, y + 5), Block::AIR, "bush pops off");
        fall(&mut world);
        assert_eq!([at(&world, 1, y + 1), at(&world, 1, y + 2), at(&world, 1, y + 3)], column[1..4]);
        assert_eq!(at(&world, 1, y + 4), Block::AIR);
    }

    #[test]
    fn ancient_debris_drops_one_block_with_fortune_or_silk_touch() {
        use crate::enchant::{Enchantment, Enchants};
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let p = IVec3::new(0, 200, 0);
        for enchantment in [None, Some((Enchantment::Fortune, 3)), Some((Enchantment::SilkTouch, 1))] {
            let mut tool = Enchants::NONE;
            if let Some((enchantment, level)) = enchantment {
                tool.set(enchantment, level);
            }
            world.drops.clear();
            world.spill_mined(p, Block::ANCIENT_DEBRIS, tool);
            assert_eq!(world.drops, vec![(p, crate::inventory::Stack::new(Block::ANCIENT_DEBRIS, 1))]);
        }
    }

    #[test]
    fn explosions_carve_a_crater_but_spare_obsidian() {
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -6..=6 {
            for z in -6..=6 {
                for dy in -4..=0 {
                    world.edit(IVec3::new(x, y + dy, z), Block::STONE, false);
                }
            }
        }
        world.edit(IVec3::new(1, y, 0), Block::OBSIDIAN, false);
        world.edit(IVec3::new(-1, y, 0), Block::ANCIENT_DEBRIS, false);
        world.edit(IVec3::new(0, y, 1), Block::NETHERITE_BLOCK, false);
        // A sand column through the blast: the bottom is blown away (well
        // inside the ragged edge), the top survives (beyond it) and falls.
        for dy in 1..=6 {
            world.edit(IVec3::new(0, y + dy, 0), Block::SAND, false);
        }

        let removed = world.explode(DVec3::new(0.5, y as f64 + 1.0, 0.5), 3.0);
        assert!(removed > 30, "only {removed} blocks");
        assert_eq!(world.get_block(IVec3::new(0, y, 0)), Some(Block::AIR));
        assert_eq!(world.get_block(IVec3::new(1, y, 0)), Some(Block::OBSIDIAN));
        assert_eq!(world.get_block(IVec3::new(-1, y, 0)), Some(Block::ANCIENT_DEBRIS));
        assert_eq!(world.get_block(IVec3::new(0, y, 1)), Some(Block::NETHERITE_BLOCK));
        assert_eq!(world.get_block(IVec3::new(6, y - 4, 6)), Some(Block::STONE), "outside the blast");
        assert_eq!(world.get_block(IVec3::new(0, y + 1, 0)), Some(Block::AIR));
        let falling = world.falling_blocks().len();
        assert!((3..=4).contains(&falling), "{falling} sand blocks falling");
    }

    #[test]
    fn crops_ripen_saplings_grow_and_leaves_decay() {
        use crate::item::Item;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -8..=8 {
            for z in -8..=8 {
                world.edit(IVec3::new(x, y, z), Block::DIRT, false);
            }
        }
        // Farmland by water gets wet, and wheat on it ripens.
        let soil = IVec3::new(0, y, 0);
        world.set_block(soil, Block::FARMLAND);
        world.edit(IVec3::new(2, y, 0), Block::WATER, false);
        assert!(world.set_block(soil + IVec3::Y, Block::wheat(0)));
        world.random_tick(soil);
        assert_eq!(world.get_block(soil), Some(Block::WET_FARMLAND));
        for _ in 0..400 {
            world.random_tick(soil + IVec3::Y);
        }
        assert_eq!(world.get_block(soil + IVec3::Y), Some(Block::wheat(7)));

        // Ripe wheat drops wheat and one to four seeds.
        world.drops.clear();
        world.spill_block(soil + IVec3::Y, Block::wheat(7));
        let got: Vec<_> = world.drops.iter().map(|&(_, s)| (s.item, s.count)).collect();
        assert_eq!(got[0], (Item::WHEAT, 1));
        assert!(got[1].0 == Item::WHEAT_SEEDS && (1..=4).contains(&got[1].1), "{got:?}");

        // Dry farmland with nothing growing turns back to dirt; trampling it
        // pops the crop off.
        let dry = IVec3::new(-7, y, -7);
        world.set_block(dry, Block::FARMLAND);
        world.random_tick(dry);
        assert_eq!(world.get_block(dry), Some(Block::DIRT));
        world.set_block(soil, Block::DIRT);
        assert_eq!(world.get_block(soil + IVec3::Y), Some(Block::AIR), "wheat needs farmland");

        // A sapling grows into a tree; felling it makes its leaves decay.
        let sapling = IVec3::new(-4, y + 1, 4);
        world.set_block(sapling, Block::OAK_SAPLING);
        assert!(world.grow_tree(sapling));
        assert_eq!(world.get_block(sapling), Some(Block::LOG));
        let leaves = |w: &World| {
            (-3..=3)
                .flat_map(|dx| (0..=8).flat_map(move |dy| (-3..=3).map(move |dz| IVec3::new(dx, dy, dz))))
                .filter(|&d| w.get_block(sapling + d) == Some(Block::LEAVES))
                .count()
        };
        assert!(leaves(&world) > 10);
        let mut trunk = sapling;
        while world.get_block(trunk) == Some(Block::LOG) {
            world.set_block(trunk, Block::AIR);
            trunk += IVec3::Y;
        }
        for _ in 0..30 {
            world.tick_leaf_decay(0.5);
        }
        assert_eq!(leaves(&world), 0, "leaves decay without their logs");
    }

    #[test]
    fn nether_wart_grows_in_the_dark_on_soul_sand() {
        use crate::item::Item;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let sand = IVec3::new(0, 200, 0);
        world.set_block(sand, Block::SOUL_SAND);
        // Roofed over: wart ignores light, unlike wheat.
        world.set_block(sand + IVec3::Y * 3, Block::STONE);
        assert!(world.set_block(sand + IVec3::Y, Block::nether_wart(0)));
        assert!(Block::nether_wart(0).can_stay_on(Block::SOUL_SAND));
        assert!(!Block::nether_wart(0).can_stay_on(Block::FARMLAND));
        assert!(!world.apply_bone_meal(sand + IVec3::Y), "bone meal does nothing to wart");
        for _ in 0..300 {
            world.random_tick(sand + IVec3::Y);
        }
        assert_eq!(world.get_block(sand + IVec3::Y), Some(Block::nether_wart(3)));

        // Ripe wart drops 2-4, unripe wart one.
        world.drops.clear();
        world.spill_block(sand + IVec3::Y, Block::nether_wart(3));
        let ripe: u8 = world.drops.iter().map(|&(_, s)| s.count).sum();
        assert!(world.drops.iter().all(|(_, s)| s.item == Item::NETHER_WART) && (2..=4).contains(&ripe));
        world.drops.clear();
        world.spill_block(sand + IVec3::Y, Block::nether_wart(1));
        assert_eq!(world.drops.iter().map(|&(_, s)| (s.item, s.count)).collect::<Vec<_>>(), [(Item::NETHER_WART, 1)]);

        // It pops off when the soul sand goes.
        world.set_block(sand, Block::NETHERRACK);
        assert_eq!(world.get_block(sand + IVec3::Y), Some(Block::AIR));
        assert_eq!(Item::from_name("nether_wart"), Some(Item::NETHER_WART));
        assert_eq!(Item::NETHER_WART.places(), Some(Block::nether_wart(0)));
    }

    #[test]
    fn chests_keep_their_contents_save_and_spill() {
        use crate::inventory::Stack;
        use crate::item::Item;
        use block::Facing;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let p = IVec3::new(0, 200, 0);
        world.set_block(p, Block::CHEST.with_facing(Facing::East));
        world.chest_mut(p).expect("placing a chest creates its contents").slots[4] = Some(Stack::new(Item::COAL, 7));
        world.chest_mut(p).unwrap().slots[26] = Some(Stack::new(Block::DIRT, 64));

        let mut other = settled_world(DVec3::new(0.0, 200.0, 0.0));
        other.load_chests(&world.chests_to_string());
        assert_eq!(other.chest(p), world.chest(p));

        world.set_block(p, Block::AIR);
        assert!(world.chest(p).is_none());
        let spilled: Vec<_> = world.drops.iter().map(|&(_, s)| s).collect();
        assert_eq!(spilled, [Stack::new(Item::COAL, 7), Stack::new(Block::DIRT, 64)]);
    }

    #[test]
    fn brewing_stands_brew_save_and_spill() {
        use crate::inventory::Stack;
        use crate::item::Item;
        use crate::potion::Potion;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let p = IVec3::new(0, 200, 0);
        world.set_block(p, Block::BREWING_STAND);
        let water = Some(Stack::new(Item::potion(Potion::WATER), 1));
        let b = world.brewing_stand_mut(p).expect("placing a stand creates its contents");
        b.bottles = [water; 3];
        b.ingredient = Some(Stack::new(Item::NETHER_WART, 1));
        b.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
        for _ in 0..401 {
            world.tick_brewing(0.05);
        }
        assert_eq!(world.brews_done, [p]);
        let awkward = Some(Stack::new(Item::potion(Potion::AWKWARD), 1));
        assert_eq!(world.brewing_stand(p).unwrap().bottles, [awkward; 3]);
        let mut other = settled_world(DVec3::new(0.0, 200.0, 0.0));
        other.load_brewing_stands(&world.brewing_stands_to_string());
        assert_eq!(other.brewing_stand(p), world.brewing_stand(p));
        world.drops.clear();
        world.set_block(p, Block::AIR);
        assert!(world.brewing_stand(p).is_none());
        assert_eq!(world.drops.len(), 3, "three awkward potions spill (wart and powder used up)");
    }

    #[test]
    fn furnaces_light_up_smelt_save_and_spill() {
        use crate::inventory::Stack;
        use crate::item::Item;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let p = IVec3::new(0, 200, 0);
        world.set_block(p, Block::FURNACE.with_facing(block::Facing::West));
        let f = world.furnace_mut(p).expect("placing a furnace creates its contents");
        f.input = Some(Stack::new(Block::SAND, 4));
        f.fuel = Some(Stack::new(Item::COAL, 1));
        world.tick_furnaces(0.1);
        let lit = Block::LIT_FURNACE.with_facing(block::Facing::West);
        assert_eq!(world.get_block(p), Some(lit), "burning furnaces glow, facing the same way");
        for _ in 0..21 {
            world.tick_furnaces(1.0);
        }
        assert_eq!(world.furnace(p).unwrap().output, Some(Stack::new(Block::GLASS, 2)));

        // Saved and restored furnaces carry on where they were.
        let saved = world.furnaces_to_string();
        let mut other = settled_world(DVec3::new(0.0, 200.0, 0.0));
        other.load_furnaces(&saved);
        assert_eq!(other.furnace(p).unwrap().output, world.furnace(p).unwrap().output);

        assert!((world.furnace(p).unwrap().xp - 0.2).abs() < 1e-4, "glass stores 0.1 each");

        // Breaking it spills everything and its experience, and the state is gone.
        world.furnace_mut(p).unwrap().xp = 3.0;
        world.set_block(p, Block::AIR);
        assert_eq!(world.xp_drops, [(p, 3)]);
        assert!(world.furnace(p).is_none());
        let spilled: Vec<_> = world.drops.iter().map(|&(_, s)| s.item).collect();
        assert_eq!(spilled, [Item::from_block(Block::SAND), Item::from_block(Block::GLASS)]);
    }
}
