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

pub mod activity;
mod automation;
pub mod bastion;
pub mod bell;
pub mod biome;
pub mod biome_zoom;
pub mod block;
pub mod brewing;
pub mod caves;
pub mod chest;
pub mod chunk;
pub mod climate;
pub mod colors;
pub mod colour_blocks;
pub mod composter;
pub mod copper;
pub mod dismount;
pub mod dungeon;
pub mod end;
pub mod end_portal;
pub mod falling;
mod fire;
mod fluid;
mod foraging;
pub mod forms;
pub mod fortress;
pub mod furnace;
pub mod gadgets;
pub mod geode;
mod growth;
mod honey;
mod light_shape;
pub(crate) mod lighting;
pub mod mineshaft;
pub mod mount_states;
mod navigation;
pub mod nether;
pub mod nether_biome;
pub mod nether_biome_blocks;
pub mod nether_blocks;
pub mod nether_complexes;
mod nether_decoration;
pub mod nether_features;
mod nether_flora;
pub mod noise;
pub mod ore;
pub mod overworld_blocks;
mod overworld_flora;
mod portal;
pub mod pumpkin_blocks;
pub mod rails;
pub mod redstone;
pub mod redstone_blocks;
mod redstone_contacts;
#[cfg(test)]
mod rule_tests;
pub mod scaffolding;
pub mod shape;
pub mod slab_stair_states;
pub mod spawn_finder;
mod spawner;
mod sponge;
pub mod stone_blocks;
pub mod storage;
pub mod stronghold;
pub mod structure;
pub mod temples;
pub mod terrain;
pub mod trees;
pub mod utilities;
pub mod village;
pub mod village_blocks;
mod village_life;
pub mod wall_states;
pub mod waterlog;
pub mod wood;

use std::sync::Arc;

use glam::{DVec3, IVec2, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::mesh::{self, D, MARGIN, MeshData, MeshInput, NO_HEIGHT, Neighborhood, REGION_VOLUME, Region};
use crate::workers::{Job, JobResult, Workers};
use block::Block;
use chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, chunk_of, local_of};
use terrain::Generator;

pub struct ChunkSlot {
    pub data: Arc<ChunkData>,
    pub modified: bool,
    version: u32,
    meshed_version: Option<u32>,
    mesh_in_flight: bool,
    /// Authoritative block light; dark chunks allocate no light array.
    block_light: Option<lighting::BlockLight>,
    /// Gameplay array changes invalidate meshes once per light pass.
    light_mesh_dirty: bool,
    /// Authoritative skylight below the column heightmap; cells above it are
    /// implied sources. Absent until the column is lit, and for dark chunks.
    sky_light: Option<lighting::BlockLight>,
}

/// A generated chunk with its generator-only follow-up work done: surface
/// decoration, structure block entities and geode fluid ticks. Workers build
/// it, because structure layouts can need terrain and cave sampling that would
/// stall the main thread when a column's first chunk arrives.
pub struct GeneratedChunk {
    data: ChunkData,
    features: Vec<(IVec3, fortress::Feature)>,
    fluid_ticks: Vec<IVec3>,
}

impl GeneratedChunk {
    pub fn new(generator: &Generator, pos: IVec3, mut data: ChunkData) -> Self {
        foraging::decorate(generator, pos, &mut data);
        let features = generator.structure_features(pos);
        let fluid_ticks = if generator.dimension == terrain::Dimension::Overworld {
            generator.geodes.fluid_ticks(generator, pos * CHUNK_SIZE_I)
        } else {
            Vec::new()
        };
        Self { data, features, fluid_ticks }
    }
}

/// Per chunk-column state: skylight heightmap and how many of its chunks
/// are loaded.
struct Column {
    heights: Box<[i16; CHUNK_SIZE * CHUNK_SIZE]>,
    loaded: i32,
    /// Biome colours, once a worker has worked them out.
    foliage: Option<Box<[u8; CHUNK_SIZE * CHUNK_SIZE]>>,
    /// Snow lines (see `Generator::snow_lines`), worked out with the colours.
    snow: Option<Box<[i16; CHUNK_SIZE * CHUNK_SIZE]>>,
    /// Gameplay skylight has been seeded from the complete heightmap.
    sky_lit: bool,
}

/// A raycast's block hit: the cell, the face normal it entered through and
/// the world point where it met the block's outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHit {
    pub cell: IVec3,
    pub normal: IVec3,
    pub point: DVec3,
}

/// The game rules which change how blocks tick, as Java 26.3 defaults them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldRules {
    /// `tnt_explodes`: TNT blocks can be primed and primed TNT detonates.
    pub tnt_explodes: bool,
    /// `water_source_conversion`: flowing water between two sources becomes one.
    pub water_source_conversion: bool,
    /// `lava_source_conversion`: the same for lava.
    pub lava_source_conversion: bool,
    /// `fire_spread_radius_around_player`: fire ticks within this many blocks
    /// of a player; 0 stops fire and -1 removes the limit.
    pub fire_spread_radius: i32,
    /// The world difficulty's id (Java's `Difficulty.getId`), which fire
    /// spreading reads.
    pub difficulty: u8,
}

impl Default for WorldRules {
    fn default() -> Self {
        Self {
            tnt_explodes: true,
            water_source_conversion: true,
            lava_source_conversion: false,
            fire_spread_radius: 128,
            difficulty: 2,
        }
    }
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
    /// Geometry edits whose synchronous meshes await settled gameplay light.
    sync_meshes: FxHashSet<IVec3>,
    load_list: Vec<IVec3>,
    load_cursor: usize,
    center: Option<IVec3>,
    agent_centers: Vec<IVec3>,
    /// Remote replicas evict nothing until the host has placed the player: a
    /// placeholder centre would drop chunks the host already counts as delivered.
    hold_unloads: bool,
    /// Chunk radius kept loaded around agent and LAN players (4 unless a LAN host widens it).
    agent_radius: i32,
    /// Chunks of other players shown in split-screen: loaded and meshed at
    /// the full render distance, like the host's.
    view_centers: Vec<IVec3>,
    render_distance: i32,
    activity: activity::Activity,
    simulation_others: Vec<DVec3>,
    simulation_host: Option<IVec2>,
    /// The host's exact position, for distance rules such as fire spread.
    simulation_host_pos: Option<DVec3>,
    /// Whether each registered player (host first, then the others in
    /// order) is a Spectator, whom fire spread ignores.
    simulation_spectators: Vec<bool>,
    region: Option<Box<Region>>,
    meshes_enabled: bool,
    remote: bool,
    light_updates: lighting::LightUpdates,
    sky_updates: lighting::LightUpdates,
    fluids: fluid::FluidState,
    fire: fire::FireState,
    redstone: redstone::RedstoneState,
    automation: automation::AutomationState,
    falling: Vec<falling::FallingBlock>,
    /// Falling blocks of unloaded chunks, held until their chunk loads again
    /// (Java stores falling block entities with the chunk's other entities).
    stored_falling: FxHashMap<IVec3, Vec<falling::FallingBlock>>,
    /// Chunks installed since the entity simulation last asked: their stored
    /// entities come back. Unloaded chunks leave the set, so it stays within
    /// the loaded chunk count even when nothing drains it.
    entity_loads: FxHashSet<IVec3>,
    /// Furnace contents by position (see [`furnace`]).
    furnaces: FxHashMap<IVec3, furnace::Furnace>,
    /// Chest contents by position (see [`chest`]).
    chests: FxHashMap<IVec3, chest::Chest>,
    /// Brewing stand contents by position (see [`brewing`]).
    brewing_stands: FxHashMap<IVec3, brewing::BrewingStand>,
    /// Sign text, shelf items and chiseled bookshelf books (see [`wood::entities`]).
    wood: wood::entities::WoodEntities,
    /// Campfire, decorated pot and cauldron-drip state (see [`utilities`]).
    utilities: utilities::State,
    /// Spawner cages and the mob each makes (see `spawner`).
    spawners: FxHashMap<IVec3, crate::entity::MobKind>,
    village_pois: FxHashMap<IVec3, Block>,
    navigation_updates: navigation::Updates,
    village_homes: FxHashMap<IVec3, IVec3>,
    /// Banners in loaded chunks (see `colour_blocks`), which draw as models.
    banners: FxHashMap<IVec3, Block>,
    /// Straw beds in use (head, where the sleeper lies), which break when left.
    straw_sleepers: Vec<(IVec3, DVec3)>,
    /// Leaves waiting to decay (seconds left) after a log near them went.
    leaf_decay: FxHashMap<IVec3, f32>,
    /// Fractional random block ticks carried over between frames.
    random_ticks: f64,
    /// Random state for growth and chance drops.
    rng: u64,
    /// Java `doTileDrops`; block and block-entity removal still occurs when false.
    tile_drops: bool,
    /// The world-shaping gamerules, refreshed by the game each tick.
    pub rules: WorldRules,
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
    /// Note blocks that played this tick: position, pitch 0..=24, instrument.
    pub notes: Vec<(IVec3, u8, u8)>,
    /// Tripwire hook clicks and attach sounds since the last frame.
    pub trip_sounds: Vec<(IVec3, gadgets::TripSound)>,
    /// Freshly placed carved pumpkins and jack o'lanterns, checked for golem patterns by the game.
    pub golem_heads: Vec<IVec3>,
    /// Turtle eggs that hatched: cell and how many babies; the game spawns them.
    pub hatched_turtles: Vec<(IVec3, u8)>,
    compost_sequence: u64,
    bell_rings: Vec<IVec3>,
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

    /// Rendering replica: terrain only arrives from the LAN authority.
    pub fn new_remote(generator: Arc<Generator>, distance: i32) -> Self {
        let mut world = Self::new(generator, FxHashMap::default(), distance);
        world.remote = true;
        world.hold_unloads = true;
        world
    }

    /// Immutable subscription snapshots; pointer identity detects even nonvisual edits.
    pub fn network_chunks(&self, center: IVec3, radius: i32) -> Vec<(IVec3, Arc<ChunkData>)> {
        let mut chunks: Vec<_> = self
            .chunks
            .iter()
            .filter(|(p, _)| {
                let d = (**p - center).with_y(0);
                d.length_squared() <= radius * radius
            })
            .map(|(&p, s)| (p, s.data.clone()))
            .collect();
        chunks.sort_unstable_by_key(|(p, _)| {
            let d = *p - center;
            d.with_y(0).length_squared() * 2 + d.y * d.y
        });
        chunks
    }

    /// Largest host edit batch a replica applies cell by cell.
    const REPLICA_INCREMENTAL_EDITS: usize = 64;

    /// Applies a host's cell edits to a chunk this replica already holds.
    /// Light, heightmaps and meshes update incrementally, as for a local edit,
    /// without running simulation side effects.
    pub fn receive_edits(&mut self, pos: IVec3, cells: &[(u16, Block)]) {
        if !self.remote || cells.is_empty() || !self.chunks.contains_key(&pos) {
            return;
        }
        // Measured crossover (`lighting::tests::skylight_cost`): past roughly
        // a hundred cells, relighting the column from a snapshot is cheaper.
        if cells.len() > Self::REPLICA_INCREMENTAL_EDITS {
            let Some(data) = self.chunks.get(&pos).map(|s| s.data.with_edits(cells)) else { return };
            self.receive_chunk(pos, Arc::new(data));
        } else {
            self.apply_replica_edits(pos, cells);
        }
    }

    /// Set replica cells one at a time through the shared light, heightmap and
    /// remesh bookkeeping.
    pub(crate) fn apply_replica_edits(&mut self, pos: IVec3, cells: &[(u16, Block)]) {
        let base = pos * CHUNK_SIZE_I;
        for &(i, block) in cells {
            let i = i as i32;
            let l = IVec3::new(
                i & (CHUNK_SIZE_I - 1),
                i >> (2 * chunk::CHUNK_BITS),
                (i >> chunk::CHUNK_BITS) & (CHUNK_SIZE_I - 1),
            );
            let p = base + l;
            let Some(slot) = self.chunks.get_mut(&pos) else { return };
            let (x, y, z) = (l.x as usize, l.y as usize, l.z as usize);
            let old = slot.data.get(x, y, z);
            if old == block {
                continue;
            }
            let old_light = slot.block_light.as_ref().map_or(0, |light| light.get(x, y, z));
            Arc::make_mut(&mut slot.data).set(x, y, z, block);
            if old != block && (old.is_solid() || block.is_solid()) {
                self.navigation_updates.record(p);
            }
            self.light_block_changed(p, old, block, old_light);
            self.reheight_and_remesh(p, old, block, false);
            self.track_banner(p, old, block);
        }
        self.update_block_light();
    }

    /// Install an exact authoritative snapshot, invalidate stale meshes and heightmaps.
    /// This does not settle blocks, generate decoration or create simulation drops.
    pub fn receive_chunk(&mut self, pos: IVec3, data: Arc<ChunkData>) {
        if !self.remote || !self.generator.dimension.chunk_rows().contains(&pos.y) {
            return;
        }
        // Replaced data invalidates the column's heightmap and skylight.
        self.unlight_sky_column(column_of(pos));
        // Light the old data spread into neighbours must be removed as on unload,
        // or it floods back into the replacement from their border cells.
        let old_light = self.chunks.get_mut(&pos).and_then(|s| s.block_light.take());
        self.unload_block_light(pos, old_light.as_ref());
        let version = self.chunks.get(&pos).map_or(0, |s| s.version.wrapping_add(1));
        let in_flight = self.chunks.get(&pos).is_some_and(|s| s.mesh_in_flight);
        let existed = self.chunks.contains_key(&pos);
        // The snapshot replaces the chunk's banners for the renderer.
        self.banners.retain(|p, _| chunk_of(*p) != pos);
        self.register_banners(pos, &data);
        self.chunks.insert(
            pos,
            ChunkSlot {
                data,
                modified: false,
                version,
                meshed_version: None,
                mesh_in_flight: in_flight,
                block_light: None,
                light_mesh_dirty: false,
                sky_light: None,
            },
        );
        let key = column_of(pos);
        let col = self.columns.entry(key).or_insert_with(|| {
            self.workers.submit(Job::Foliage(key));
            Column {
                heights: Box::new([NO_HEIGHT; CHUNK_SIZE * CHUNK_SIZE]),
                loaded: 0,
                foliage: None,
                snow: None,
                sky_lit: false,
            }
        });
        if !existed {
            col.loaded += 1;
        }
        col.heights.fill(NO_HEIGHT);
        if self.generator.dimension.has_skylight() {
            for (p, s) in &self.chunks {
                if column_of(*p) == key {
                    let heights = mesh::chunk_heights(&s.data, p.y * CHUNK_SIZE_I);
                    for (h, new) in col.heights.iter_mut().zip(heights) {
                        *h = (*h).max(new);
                    }
                }
            }
        }
        self.load_block_light(pos);
        self.light_sky_column(key);
        // A heightmap change affects every vertical chunk of the neighboring
        // columns; a data change also affects their block-light margins.
        let affected: Vec<_> = self
            .chunks
            .keys()
            .copied()
            .filter(|p| (p.x - pos.x).abs() <= 1 && (p.z - pos.z).abs() <= 1 && self.in_mesh_range(*p))
            .collect();
        for p in affected {
            let slot = self.chunks.get_mut(&p).unwrap();
            slot.version = slot.version.wrapping_add(1);
            self.dirty.insert(p);
        }
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
            sync_meshes: FxHashSet::default(),
            load_list: Vec::new(),
            load_cursor: 0,
            center: None,
            agent_centers: Vec::new(),
            hold_unloads: false,
            agent_radius: 4,
            view_centers: Vec::new(),
            render_distance,
            activity: Default::default(),
            simulation_others: Vec::new(),
            simulation_host: None,
            simulation_host_pos: None,
            simulation_spectators: Vec::new(),
            region: None,
            meshes_enabled,
            remote: false,
            light_updates: Default::default(),
            sky_updates: Default::default(),
            fluids: Default::default(),
            fire: Default::default(),
            redstone: Default::default(),
            automation: Default::default(),
            falling: Vec::new(),
            stored_falling: FxHashMap::default(),
            entity_loads: FxHashSet::default(),
            furnaces: FxHashMap::default(),
            chests: FxHashMap::default(),
            brewing_stands: FxHashMap::default(),
            wood: Default::default(),
            utilities: Default::default(),
            spawners: FxHashMap::default(),
            village_pois: FxHashMap::default(),
            navigation_updates: navigation::Updates::default(),
            village_homes: FxHashMap::default(),
            banners: FxHashMap::default(),
            straw_sleepers: Vec::new(),
            leaf_decay: FxHashMap::default(),
            random_ticks: 0.0,
            rng,
            tile_drops: true,
            rules: WorldRules::default(),
            drops: Vec::new(),
            particles: Default::default(),
            xp_drops: Vec::new(),
            brews_done: Vec::new(),
            primed_tnt: Vec::new(),
            notes: Vec::new(),
            trip_sounds: Vec::new(),
            golem_heads: Vec::new(),
            hatched_turtles: Vec::new(),
            compost_sequence: 0,
            bell_rings: Vec::new(),
            raining: false,
            mesh_uploads: Vec::new(),
            mesh_removals: Vec::new(),
        }
    }

    pub fn render_distance(&self) -> i32 {
        self.render_distance
    }

    /// Lowest block y of this world's dimension.
    #[inline]
    pub fn min_y(&self) -> i32 {
        self.generator.dimension.min_y()
    }

    /// One past the highest block y of this world's dimension.
    #[inline]
    pub fn max_y(&self) -> i32 {
        self.generator.dimension.max_y()
    }

    /// Whether `y` lies inside this world's build height.
    #[inline]
    pub fn contains_y(&self, y: i32) -> bool {
        self.generator.dimension.contains_y(y)
    }

    /// Chunks per column in this dimension.
    fn column_chunks(&self) -> i32 {
        self.generator.dimension.column_chunks()
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

    /// A remote replica's player now has its authoritative position: allow evictions.
    pub fn release_unloads(&mut self) {
        if self.hold_unloads {
            self.hold_unloads = false;
            self.center = None;
        }
    }

    /// Widens (or restores) the radius loaded around agent and LAN players.
    pub fn set_agent_radius(&mut self, radius: i32) {
        let radius = radius.clamp(4, 32);
        if radius != self.agent_radius {
            self.agent_radius = radius;
            self.center = None;
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
            && self.sky_updates.is_empty()
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
                d.length_squared() <= self.agent_radius * self.agent_radius
            })
    }

    /// Extra hysteresis before unloading so walking back and forth across a
    /// chunk border doesn't thrash.
    fn in_keep_range(&self, pos: IVec3) -> bool {
        let r = self.render_distance + 3;
        self.horizontal_dist2(pos) <= r * r
            || self.agent_centers.iter().any(|c| {
                let d = (pos - *c).with_y(0);
                d.length_squared() <= (self.agent_radius + 1) * (self.agent_radius + 1)
            })
    }

    /// 16-block columns of every fully loaded storage column, for generation herds.
    /// Herd placement reads skylight, which is only exact once the column and
    /// its eight neighbours are lit, so frontier columns wait.
    pub(crate) fn visit_spawn_columns(&self, visit: &mut dyn FnMut(IVec2)) {
        let lit = |c: IVec2| self.columns.get(&c).is_some_and(|c| c.sky_lit);
        for (&c, column) in &self.columns {
            if column.loaded == self.column_chunks()
                && (-1..=1).all(|dz| (-1..=1).all(|dx| lit(c + IVec2::new(dx, dz))))
            {
                for dz in 0..2 {
                    for dx in 0..2 {
                        visit(c * 2 + IVec2::new(dx, dz));
                    }
                }
            }
        }
    }

    /// Whether every chunk of the column holding `(x, z)` is loaded.
    pub fn column_loaded(&self, x: i32, z: i32) -> bool {
        let key = column_of(chunk_of(IVec3::new(x, 0, z)));
        self.columns.get(&key).is_some_and(|c| c.loaded == self.column_chunks())
    }

    pub fn is_loaded(&self, block: IVec3) -> bool {
        !self.contains_y(block.y) || self.chunks.contains_key(&chunk_of(block))
    }

    /// The storage chunk an entity at `p` belongs to. Heights beyond the
    /// world clamp into the top or bottom chunk, so entities above or below
    /// the build limit still unload with their column (Java keys entity
    /// sections by column status, not by height).
    pub fn entity_chunk(&self, p: DVec3) -> IVec3 {
        let cell = p.floor().as_ivec3();
        chunk_of(cell.with_y(cell.y.clamp(self.min_y(), self.max_y() - 1)))
    }

    /// Whether storage chunk `chunk` is loaded (Java's `Visibility` is at
    /// least `TRACKED`): entities there stay in the level.
    pub fn chunk_loaded(&self, chunk: IVec3) -> bool {
        self.chunks.contains_key(&chunk)
    }

    /// Chunks installed since the previous call, whose stored entities can
    /// return to the level.
    pub fn take_entity_loads(&mut self) -> Vec<IVec3> {
        let mut loads: Vec<IVec3> = self.entity_loads.drain().collect();
        loads.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        loads
    }

    /// Highest light-blocking block (including leaves and water) in a fully
    /// loaded column; `None` if the column isn't loaded or is empty.
    pub fn surface_height(&self, x: i32, z: i32) -> Option<i32> {
        let key = column_of(chunk_of(IVec3::new(x, 0, z)));
        let col = self.columns.get(&key).filter(|c| c.loaded == self.column_chunks())?;
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

    /// The height from which snow falls instead of rain in column `(x, z)`
    /// (`i16::MIN`: always snow; `i16::MAX`: a dry biome with neither),
    /// once known.
    pub fn snow_line_at(&self, x: i32, z: i32) -> Option<i16> {
        let col = self.columns.get(&column_of(chunk_of(IVec3::new(x, 0, z))))?;
        let l = local_of(IVec3::new(x, 0, z));
        col.snow.as_ref().map(|f| f[(l.x + l.z * CHUNK_SIZE_I) as usize])
    }

    /// Whether rain (not snow) is falling on cell `p` right now.
    pub fn rains_on(&self, p: IVec3) -> bool {
        // Match the weather renderer: dry biomes stay clear, and cold
        // biomes and high ground get snow.
        self.raining
            && self.generator.dimension.has_sky()
            && self.sky_exposed(p)
            && self.snow_line_at(p.x, p.z).is_some_and(|line| {
                // Like the weather sheets: snow or rain by the column's ground.
                // Unknown ground (a partly loaded column): judge by the cell.
                let ground = self.surface_height(p.x, p.z).unwrap_or(p.y);
                line != i16::MAX && ground < line as i32
            })
    }

    /// Whether a cell sees the sky straight up (nothing light-blocking
    /// above it). Unloaded columns count as exposed.
    pub fn sky_exposed(&self, p: IVec3) -> bool {
        self.surface_height(p.x, p.z).is_none_or(|h| p.y > h)
    }

    /// Block at a world position; `None` if the chunk isn't loaded.
    pub fn get_block(&self, p: IVec3) -> Option<Block> {
        if p.y < self.min_y() {
            return Some(Block::BEDROCK);
        }
        if p.y >= self.max_y() {
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
        // Removing a block that holds water leaves its source (see `edit`).
        let broken = block == Block::AIR;
        let changed = self.edit(p, block, true);
        if changed
            && broken
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
    /// The cell's water follows Java's fluid states ([`waterlog::carried`]):
    /// a waterloggable state written over a water source is waterlogged,
    /// and removing a block that holds water leaves the source.
    pub(crate) fn edit(&mut self, p: IVec3, block: Block, sync: bool) -> bool {
        let block = match self.get_block(p) {
            Some(old) => waterlog::carried(old, block),
            None => block,
        };
        self.edit_exact(p, block, sync)
    }

    /// [`World::edit`] that writes `block` as given, for the changes that
    /// drop a cell's water: bucket pickup and sponges drying a waterlogged
    /// block, sponges clearing kelp and seagrass, pistons moving blocks.
    pub(crate) fn edit_exact(&mut self, p: IVec3, block: Block, sync: bool) -> bool {
        self.edit_exact_inner(p, block, sync, true)
    }

    /// The wall-post walk owns its downward notifications; other edits
    /// notify surrounding walls after recording the normal edit effects.
    fn edit_exact_inner(&mut self, p: IVec3, block: Block, sync: bool, update_walls: bool) -> bool {
        if !self.contains_y(p.y) {
            return false;
        }
        let block = wall_states::reshaped(block, |d| self.get_block(p + d).unwrap_or(Block::AIR));
        let block = colour_blocks::settled_state(block);
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
        if old.fire_age().is_some() && block.fire_age().is_some() {
            return true;
        }
        // Java notifies navigators only when the collision shape changes.
        if old != block && (old.is_solid() || block.is_solid()) {
            self.navigation_updates.record(p);
        }
        self.light_block_changed(p, old, block, old_light);
        self.track_fire(p, old, block);
        self.track_redstone(p, old, block);
        if old != block {
            self.scaffolding_changed(p);
        }
        self.track_automation(p, old, block);
        self.track_furnace(p, old, block);
        self.track_chest(p, old, block);
        self.track_banner(p, old, block);
        self.track_brewing_stand(p, old, block);
        self.track_wood(p, old, block);
        self.track_utilities(p, old, block);
        self.track_spawner(p, old, block);
        self.track_village_poi(p, block);
        if composter::level(block) == Some(7) {
            self.schedule_redstone(p, 20, 0);
        }
        if pumpkin_blocks::is_head(block) {
            self.golem_heads.push(p);
        }
        if old.is_log() && !block.is_log() {
            self.log_removed(p);
        }
        self.extinguish_unsupported_fire(p);
        self.reheight_and_remesh(p, old, block, sync);
        self.reshape_stairs_beside(p, old, block, sync);
        if update_walls {
            self.reshape_walls_beside(p, old, block, sync);
        }
        // `updateShape` of a waterlogged state schedules a water tick.
        if block != old && waterlog::wet(block.0) {
            self.wake_fluids(p);
        }
        // Live coral plants without water die on a scheduled tick.
        if overworld_flora::live_coral_plant(block) {
            self.schedule_coral_death(p);
        }
        if old.holds_water() && !block.holds_water() {
            for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                if self.get_block(p + d).is_some_and(overworld_flora::live_coral_plant) {
                    self.schedule_coral_death(p + d);
                }
            }
        }
        true
    }

    /// Java's `StairBlock.updateShape`: stairs beside a changed cell take the
    /// corner shape their horizontal neighbours give them. A stairs shape
    /// depends only on which neighbours are stairs and their facing and
    /// half, so other changes (fluids, waterlogging, a shape change) skip
    /// the lookups.
    fn reshape_stairs_beside(&mut self, p: IVec3, old: Block, block: Block, sync: bool) {
        let key = |b: Block| slab_stair_states::stairs(b).map(|s| (s.facing, s.top));
        if key(old) == key(block) {
            return;
        }
        for f in block::Facing::ALL {
            let n = p + f.offset();
            let Some(stairs) = self.get_block(n).filter(|&b| key(b).is_some()) else { continue };
            let shaped = slab_stair_states::reshaped(stairs, |d| self.get_block(n + d.offset()));
            if shaped != stairs {
                self.edit(n, shaped, sync);
            }
        }
    }

    /// Java's `Block.updateFromNeighbourShapes` for a block set without its
    /// own placement rules (a piston's moved block): stairs and walls take their shape.
    pub(crate) fn shaped_by_neighbours(&self, p: IVec3, block: Block) -> Block {
        let block = slab_stair_states::reshaped(block, |d| self.get_block(p + d.offset()));
        wall_states::reshaped(block, |d| self.get_block(p + d).unwrap_or(Block::AIR))
    }

    /// Keep the column heightmap and skylight current after a cell changed
    /// from `old` to `block`, and schedule remeshing of every chunk whose
    /// geometry or lighting depends on it.
    fn reheight_and_remesh(&mut self, p: IVec3, old: Block, block: Block, sync: bool) {
        let cpos = chunk_of(p);
        let l = local_of(p);
        let old_height = self.raw_height(p);
        let (old_h, new_h) = self.update_height(p, block);
        if let (Some(before), Some(after)) = (old_height, self.raw_height(p)) {
            self.sky_block_changed(p, old, block, before, after);
        }
        if !self.meshes_enabled {
            return;
        }

        // Light reaches 15 blocks, plus 1 for the face-adjacent sample cell;
        // a heightmap change also re-exposes everything between old and new.
        let reach = 16;
        let lo_y = p.y.min(old_h).min(new_h).max(self.min_y()) - reach;
        let hi_y = p.y.max(old_h).max(new_h) + reach;
        let rows = self.generator.dimension.chunk_rows();
        let lo = chunk_of(IVec3::new(p.x - reach, lo_y, p.z - reach)).max(IVec3::new(i32::MIN, rows.start, i32::MIN));
        let hi = chunk_of(IVec3::new(p.x + reach, hi_y, p.z + reach)).min(IVec3::new(i32::MAX, rows.end - 1, i32::MAX));
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
                        self.sync_meshes.insert(c);
                    } else if self.in_mesh_range(c) {
                        self.dirty.insert(c);
                    }
                }
            }
        }
    }

    /// A column's raw heightmap entry (`NO_HEIGHT` when clear), if the column exists.
    fn raw_height(&self, p: IVec3) -> Option<i32> {
        let col = self.columns.get(&column_of(chunk_of(p)))?;
        let l = local_of(p);
        Some(col.heights[(l.x + l.z * CHUNK_SIZE_I) as usize] as i32)
    }

    /// Java's `WORLD_SURFACE` heightmap: the highest non-air block of a fully
    /// loaded column, `None` when the column is not loaded or holds only air.
    pub fn world_surface_height(&self, x: i32, z: i32) -> Option<i32> {
        if !self.column_loaded(x, z) {
            return None;
        }
        let l = local_of(IVec3::new(x, 0, z));
        for cy in self.generator.dimension.chunk_rows().rev() {
            let slot = self.chunks.get(&IVec3::new(x >> chunk::CHUNK_BITS, cy, z >> chunk::CHUNK_BITS))?;
            if slot.data.uniform() == Some(Block::AIR) {
                continue;
            }
            for ly in (0..CHUNK_SIZE).rev() {
                if slot.data.get(l.x as usize, ly, l.z as usize) != Block::AIR {
                    return Some(cy * CHUNK_SIZE_I + ly as i32);
                }
            }
        }
        None
    }

    /// Updates the heightmap for an edited block; returns (old, new) heights.
    fn update_height(&mut self, p: IVec3, block: Block) -> (i32, i32) {
        if !self.generator.dimension.has_skylight() {
            return (p.y, p.y);
        }
        let key = column_of(chunk_of(p));
        let l = local_of(p);
        let i = (l.x + l.z * CHUNK_SIZE_I) as usize;
        let Some(col) = self.columns.get(&key) else { return (p.y, p.y) };
        let old = col.heights[i] as i32;
        let stop = block.sky_stop_height(p.y).unwrap_or(NO_HEIGHT as i32);
        let new = if stop > old {
            stop
        } else if p.y >= old {
            (self.min_y()..=p.y)
                .rev()
                .filter_map(|y| self.get_block(p.with_y(y)).and_then(|b| b.sky_stop_height(y)))
                .next()
                .unwrap_or(NO_HEIGHT as i32)
        } else {
            old
        };
        self.columns.get_mut(&key).unwrap().heights[i] = new as i16;
        (old.max(self.min_y()), new.max(self.min_y()))
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
                        .is_some_and(|c| c.loaded == self.column_chunks())
                })
            })
    }

    /// Copy geometry and authoritative light for a CPU mesh job. Resolve pending
    /// light first; complete surrounding columns are required for exact skylight.
    pub fn mesh_input(&self, pos: IVec3) -> Box<MeshInput> {
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
        let slots: [Option<&ChunkSlot>; 27] = std::array::from_fn(|i| {
            let i = i as i32;
            self.chunks.get(&(pos + IVec3::new(i % 3 - 1, i / 9 - 1, i / 3 % 3 - 1)))
        });
        let mut light = Box::new([0; REGION_VOLUME]);
        for ry in 0..D {
            let wy = ry + CHUNK_SIZE - MARGIN;
            let (cy, ly) = (wy / CHUNK_SIZE, wy % CHUNK_SIZE);
            let world_y = pos.y * CHUNK_SIZE_I + ry as i32 - MARGIN as i32;
            for rz in 0..D {
                let wz = rz + CHUNK_SIZE - MARGIN;
                let (cz, lz) = (wz / CHUNK_SIZE, wz % CHUNK_SIZE);
                for rx in 0..D {
                    let wx = rx + CHUNK_SIZE - MARGIN;
                    let (cx, lx) = (wx / CHUNK_SIZE, wx % CHUNK_SIZE);
                    let sky = if self.generator.dimension.has_skylight() {
                        cols[cx + cz * 3].filter(|c| c.sky_lit).map_or(0, |c| {
                            if world_y > i32::from(c.heights[lx + lz * CHUNK_SIZE]) {
                                15
                            } else {
                                slots[cx + cz * 3 + cy * 9]
                                    .and_then(|s| s.sky_light.as_ref())
                                    .map_or(0, |l| l.get(lx, ly, lz))
                            }
                        })
                    } else {
                        0
                    };
                    let block = slots[cx + cz * 3 + cy * 9]
                        .and_then(|s| s.block_light.as_ref())
                        .map_or(0, |l| l.get(lx, ly, lz));
                    light[rx + rz * D + ry * D * D] = sky | block << 4;
                }
            }
        }
        let foliage = cols[4].and_then(|c| c.foliage.clone()).unwrap_or_else(|| Box::new([0; CHUNK_SIZE * CHUNK_SIZE]));
        Box::new(MeshInput { neighbors, light, base_y: pos.y * CHUNK_SIZE_I, min_y: self.min_y(), foliage })
    }

    /// A light snapshot can affect corner samples in adjacent chunks. Bump
    /// versions after each light pass so in-flight older snapshots are discarded.
    fn invalidate_light_meshes(&mut self) {
        let changed: Vec<_> =
            self.chunks.iter_mut().filter_map(|(&p, s)| std::mem::take(&mut s.light_mesh_dirty).then_some(p)).collect();
        if !self.meshes_enabled || changed.is_empty() {
            return;
        }
        let mut affected = FxHashSet::default();
        for p in changed {
            for y in -1..=1 {
                for z in -1..=1 {
                    for x in -1..=1 {
                        affected.insert(p + IVec3::new(x, y, z));
                    }
                }
            }
        }
        for p in affected {
            if let Some(slot) = self.chunks.get_mut(&p) {
                slot.version = slot.version.wrapping_add(1);
                if self.in_mesh_range(p) {
                    self.dirty.insert(p);
                }
            }
        }
    }

    fn flush_sync_meshes(&mut self) {
        if !self.light_settled() || self.sync_meshes.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.sync_meshes);
        for &p in &pending {
            self.remesh_now(p);
        }
        self.sync_meshes = pending;
        self.sync_meshes.clear();
    }

    /// Build a ready chunk's mesh immediately and queue it for upload; defer unready chunks.
    /// Headless worlds skip both paths.
    fn remesh_now(&mut self, pos: IVec3) {
        if !self.meshes_enabled {
            return;
        }
        if !self.in_mesh_range(pos) {
            // Recentering re-queues it if it comes into range later.
            return;
        }
        if !self.ready_to_mesh(pos) {
            self.dirty.insert(pos);
            return;
        }
        let input = self.mesh_input(pos);
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
                    n.y < self.generator.dimension.chunk_rows().start
                        || self.chunks.get(&n).and_then(|s| s.data.uniform()).is_some_and(|b| b.is_opaque())
                })
            }
            _ => false,
        }
    }

    fn recenter(&mut self, center: IVec3) {
        self.center = Some(center);

        // Unload far chunks; keep player edits around in memory.
        let far: Vec<IVec3> = if self.hold_unloads {
            Vec::new()
        } else {
            self.chunks.keys().copied().filter(|&p| !self.in_keep_range(p)).collect()
        };
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
                    for y in self.generator.dimension.chunk_rows() {
                        let p = IVec3::new(c.x + dx, y, c.z + dz);
                        if self.in_load_range(p) {
                            self.load_list.push(p);
                        }
                    }
                }
            }
        }
        let r = self.agent_radius;
        for c in &self.agent_centers {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dz * dz > r * r {
                        continue;
                    }
                    for y in self.generator.dimension.chunk_rows() {
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
    /// Unmodified data is treated as freshly generated (see [`GeneratedChunk`]).
    pub(crate) fn insert_chunk(&mut self, pos: IVec3, data: Arc<ChunkData>, modified: bool) {
        if modified {
            self.load_fires(pos, &data);
            self.install_chunk(pos, data, true);
        } else {
            let data = Arc::unwrap_or_clone(data);
            self.insert_generated(pos, GeneratedChunk::new(&self.generator, pos, data));
        }
    }

    /// Install a generated chunk whose generator-derived work is already done.
    fn insert_generated(&mut self, pos: IVec3, chunk: GeneratedChunk) {
        let GeneratedChunk { data, features, fluid_ticks } = chunk;
        self.register_features(&data, features);
        for p in fluid_ticks {
            self.wake_fluids(p);
        }
        self.install_chunk(pos, Arc::new(data), false);
    }

    fn install_chunk(&mut self, pos: IVec3, data: Arc<ChunkData>, modified: bool) {
        self.register_village_life(pos, &data);
        self.register_banners(pos, &data);
        let heights = mesh::chunk_heights(&data, pos.y * CHUNK_SIZE_I);
        let workers = &self.workers;
        let col = self.columns.entry(column_of(pos)).or_insert_with(|| {
            workers.submit(Job::Foliage(column_of(pos)));
            Column {
                heights: Box::new([NO_HEIGHT; CHUNK_SIZE * CHUNK_SIZE]),
                loaded: 0,
                foliage: None,
                snow: None,
                sky_lit: false,
            }
        });
        col.loaded += 1;
        // Without a sky (the Nether) every cell counts as open: the
        // dimension's dim, even light is daylight at a fixed low level.
        if self.generator.dimension.has_skylight() {
            for (h, new) in col.heights.iter_mut().zip(heights) {
                *h = (*h).max(new);
            }
        }
        self.chunks.insert(
            pos,
            ChunkSlot {
                data,
                modified,
                version: 0,
                meshed_version: None,
                mesh_in_flight: false,
                block_light: None,
                light_mesh_dirty: false,
                sky_light: None,
            },
        );
        self.load_block_light(pos);
        self.light_sky_column(column_of(pos));
        self.load_redstone_chunk(pos);
        self.entity_loads.insert(pos);
        if let Some(blocks) = self.stored_falling.remove(&pos) {
            self.falling.extend(blocks);
        }
        if self.in_mesh_range(pos) {
            self.dirty.insert(pos);
        }
    }

    /// Unload a present chunk, retaining edited data and queuing removal of its outgoing light.
    /// Only graphical worlds emit a renderer removal message.
    fn remove_chunk(&mut self, pos: IVec3) {
        self.unlight_sky_column(column_of(pos));
        self.unload_redstone_chunk(pos);
        let slot = self.chunks.remove(&pos).unwrap();
        self.entity_loads.remove(&pos);
        // A LAN replica's falling blocks come from the host's snapshots.
        if !self.remote {
            self.store_falling(pos);
        }
        self.village_pois.retain(|p, _| chunk_of(*p) != pos);
        self.village_homes.retain(|p, _| chunk_of(*p) != pos);
        self.banners.retain(|p, _| chunk_of(*p) != pos);
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
        self.set_simulation_players(std::iter::once(player).chain(agents.iter().copied()));
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
                JobResult::Generated(pos, chunk) => {
                    self.gen_in_flight.remove(&pos);
                    if self.in_keep_range(pos) && !self.chunks.contains_key(&pos) {
                        self.insert_generated(pos, *chunk);
                    }
                }
                JobResult::Foliage(key, foliage, snow) => {
                    if let Some(col) = self.columns.get_mut(&key) {
                        col.foliage = Some(foliage);
                        col.snow = Some(snow);
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
        while !self.remote && self.gen_in_flight.len() < cap && self.load_cursor < self.load_list.len() {
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
                let input = self.mesh_input(pos);
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
        self.raycast_hit(origin, dir, max_dist).map(|h| (h.cell, h.normal))
    }

    /// Like [`World::raycast`], with the exact point where the ray meets the
    /// block's outline (Java's `BlockHitResult.getLocation`).
    pub fn raycast_hit(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<BlockHit> {
        self.raycast_by(origin, dir, max_dist, Block::is_targetable, false, false)
    }

    /// `ScaffoldingBlock.getShape` is a full cube while holding scaffolding.
    pub fn raycast_scaffolding(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<BlockHit> {
        self.raycast_by(origin, dir, max_dist, Block::is_targetable, false, true)
    }

    /// The selection outline of the block at `p`: min and max corners
    /// relative to the cell (low blocks and shaped blocks are smaller).
    pub fn outline(&self, p: IVec3) -> ([f32; 3], [f32; 3]) {
        let Some(b) = self.get_block(p) else { return ([0.0; 3], [1.0; 3]) };
        let neighbour = |f: block::Facing| self.get_block(p + f.offset()).unwrap_or(Block::AIR);
        let below = self.get_block(p - IVec3::Y).unwrap_or(Block::AIR);
        let above = shape::Above::read(b, |d| self.get_block(p + d).unwrap_or(Block::AIR));
        shape::outline_with(b, neighbour, below, above).bounds().map_or(([0.0; 3], [1.0; 3]), |bx| {
            (bx.min.map(|c| f32::from(c) / 16.0), bx.max.map(|c| f32::from(c) / 16.0))
        })
    }

    /// Like [`World::raycast`], but also stops at water and lava sources,
    /// including the water a waterlogged block holds (Java's
    /// `ClipContext.Fluid.SOURCE_ONLY`, for buckets and bottles).
    pub fn raycast_sources(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(IVec3, IVec3)> {
        self.raycast_by(
            origin,
            dir,
            max_dist,
            |b| b.is_targetable() || b == Block::WATER || b == Block::LAVA || b.is_waterlogged(),
            true,
            false,
        )
        .map(|h| (h.cell, h.normal))
    }

    /// DDA traversal stopping at the first cell `hits` accepts whose shape
    /// the ray meets. With `sources`, a waterlogged cell is also met by its
    /// water's surface (`FlowingFluid.getShape`: a full cell under more
    /// water, else as high as a source).
    fn raycast_by(
        &self,
        origin: DVec3,
        dir: DVec3,
        max_dist: f64,
        hits: impl Fn(Block) -> bool,
        sources: bool,
        holding_scaffolding: bool,
    ) -> Option<BlockHit> {
        if !origin.is_finite() || !dir.is_finite() || !max_dist.is_finite() || max_dist < 0.0 || dir == DVec3::ZERO {
            return None;
        }
        let mut cell = origin.floor().as_ivec3();
        let axis_step = |v: f64| {
            if v > 0. {
                1
            } else if v < 0. {
                -1
            } else {
                0
            }
        };
        let step = IVec3::new(axis_step(dir.x), axis_step(dir.y), axis_step(dir.z));
        let inv = DVec3::new(
            if dir.x != 0.0 { 1.0 / dir.x.abs() } else { f64::INFINITY },
            if dir.y != 0.0 { 1.0 / dir.y.abs() } else { f64::INFINITY },
            if dir.z != 0.0 { 1.0 / dir.z.abs() } else { f64::INFINITY },
        );
        let frac = origin - origin.floor();
        let first = |f: f64, s: i32, inverse: f64| {
            // An axis-parallel ray on an integer plane must not produce 0*inf.
            if s == 0 { f64::INFINITY } else { (if s > 0 { 1.0 - f } else { f }) * inverse }
        };
        let mut t_max =
            DVec3::new(first(frac.x, step.x, inv.x), first(frac.y, step.y, inv.y), first(frac.z, step.z, inv.z));
        let mut normal = IVec3::ZERO;
        let mut t = 0.0;
        while t <= max_dist {
            if let Some(b) = self.get_block(cell)
                && hits(b)
                && cell.y >= self.min_y()
            {
                let mut intersection = if holding_scaffolding && scaffolding::is_scaffolding(b) {
                    crate::physics::ray_aabb_face(origin, dir, cell.as_dvec3(), cell.as_dvec3() + DVec3::ONE)
                } else {
                    crate::physics::ray_shape(self, cell, origin, dir)
                };
                if sources && b.is_waterlogged() {
                    let full = self.get_block(cell + IVec3::Y).is_some_and(Block::holds_water);
                    let top = if full { 1.0 } else { 1.0 - Block::WATER.fluid_drop() as f64 / 16.0 };
                    let lo = cell.as_dvec3();
                    let water = crate::physics::ray_aabb_face(origin, dir, lo, lo + DVec3::new(1.0, top, 1.0));
                    intersection = match (intersection, water) {
                        (Some(a), Some(w)) => Some(if w.0 < a.0 { w } else { a }),
                        (a, w) => a.or(w),
                    };
                }
                if let Some((hit, face)) = intersection
                    && hit <= max_dist
                {
                    let normal = if face == IVec3::ZERO { normal } else { face };
                    return Some(BlockHit { cell, normal, point: origin + dir * hit });
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
    fn installing_a_chunk_wakes_fluid_beside_geode_cracks() {
        let generator = Arc::new(Generator::new(5));
        let mut wet = None;
        'find: for cz in -12..12 {
            for cx in -12..12 {
                for g in generator.geodes.source(&generator, IVec2::new(cx, cz)).iter() {
                    if let Some(&p) = g.fluid_ticks.first() {
                        wet = Some(p);
                        break 'find;
                    }
                }
            }
        }
        let p = wet.expect("a geode cracked open beside fluid");
        let cpos = chunk_of(p);
        let mut world = World::new_headless(generator.clone(), Default::default(), 4);
        assert_eq!(world.active_fluids(), 0);
        world.insert_chunk(cpos, Arc::new(generator.generate(cpos)), false);
        assert!(world.active_fluids() > 0, "the cracked geode's fluid is scheduled to tick");
    }

    #[test]
    fn generated_jungle_fluids_settle_after_neighbor_updates() {
        let pos = DVec3::new(1882.0, 99.0, 682.0);
        let mut world = World::new_headless(Arc::new(Generator::new(1)), Default::default(), 4);
        let end = Instant::now() + Duration::from_secs(30);
        while world.loaded_chunks() == 0 || world.pending_jobs() > 0 {
            world.update(pos);
            world.mesh_uploads.clear();
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..1200 {
            crate::simulation::tick_world_rules(&mut world, pos, false, 0);
        }
        assert_eq!(world.active_fluids(), 0, "jungle fluids stay active: {}", world.active_fluids());
        assert!(world.falling.is_empty());
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
        assert!((to.y - egg.y).abs() <= 7);
        assert!(
            world.get_block(to - IVec3::Y).is_some_and(|b| b != Block::AIR),
            "Java 26.3 rejects unsupported destinations"
        );
        assert!(world.contains_y(to.y));
    }

    fn surface_y(world: &World, x: i32, z: i32) -> i32 {
        (world.min_y()..world.max_y())
            .rev()
            .find(|&y| world.get_block(IVec3::new(x, y, z)).unwrap().is_solid())
            .unwrap()
    }

    #[test]
    fn fortress_chunks_register_loot_chests_and_blaze_spawners() {
        use super::fortress::{Fortresses, Kind};
        let layouts = Fortresses::new(1);
        let fortress = (-2..=2).find_map(|x| layouts.get(IVec2::new(x, 0))).unwrap();
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
            player.tick(MoveInput::default(), &world);
        }
        let top = surface_y(&world, x, z) as f64 + 1.0;
        assert!(player.on_ground, "player should land");
        assert!((player.pos.y - top).abs() < 0.01, "feet at {} vs ground {top}", player.pos.y);

        // Walking into a wall stops at the wall, on a floor built for it.
        for dx in -4..=4 {
            for dz in -2..=2 {
                world.set_block(IVec3::new(x + dx, top as i32 - 1, z + dz), Block::STONE);
                for dy in 0..5 {
                    world.set_block(IVec3::new(x + dx, top as i32 + dy, z + dz), Block::AIR);
                }
            }
        }
        let wall_x = x + 2;
        for y in 0..3 {
            world.set_block(IVec3::new(wall_x, top as i32 + y, z), Block::STONE);
        }
        let walk = MoveInput { forward: 1.0, ..Default::default() };
        player.yaw = 0.0; // facing +X
        for _ in 0..120 {
            player.tick(walk, &world);
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
                player.tick(input, &world);
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
    fn concrete_powder_solidifies_in_and_beside_water() {
        use crate::color::DyeColor;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -2..=2 {
            for z in -2..=2 {
                world.edit(IVec3::new(x, y, z), Block::STONE, false);
            }
        }
        world.set_block(IVec3::new(0, y + 1, 0), Block::WATER);
        world.set_block(IVec3::new(1, y + 1, 0), Block::concrete_powder(DyeColor::Red));
        assert_eq!(world.get_block(IVec3::new(1, y + 1, 0)), Some(Block::concrete(DyeColor::Red)));
        world.set_block(IVec3::new(2, y + 1, 0), Block::WATER);
        world.set_block(IVec3::new(2, y + 4, 0), Block::concrete_powder(DyeColor::Blue));
        for _ in 0..180 {
            world.tick_falling(1.0 / 60.0);
        }
        assert_eq!(world.get_block(IVec3::new(2, y + 1, 0)), Some(Block::concrete(DyeColor::Blue)));
        assert!(world.falling_blocks().is_empty());
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
    fn snow_layers_melt_only_in_bright_block_light() {
        use super::overworld_blocks as ob;
        let mut world = settled_world(DVec3::new(0.0, 200.0, 0.0));
        let y = 200;
        for x in -8..=8 {
            for z in -8..=8 {
                world.edit(IVec3::new(x, y, z), Block::DIRT, false);
            }
        }
        // Far from any torch the layers stay put.
        let far = IVec3::new(-6, y + 1, -6);
        world.set_block(far, ob::snow_layer(3));
        for _ in 0..50 {
            world.random_tick(far);
        }
        assert_eq!(world.get_block(far), Some(ob::snow_layer(3)));
        // Beside a torch (block light 13 > 11) they melt and drop their snowballs.
        let near = IVec3::new(2, y + 1, 2);
        world.set_block(near, ob::snow_layer(3));
        world.set_block(near + IVec3::X, Block::TORCH);
        world.drops.clear();
        world.random_tick(near);
        assert_eq!(world.get_block(near), Some(Block::AIR));
        assert_eq!(
            world.drops.iter().map(|(_, s)| (s.item, s.count)).collect::<Vec<_>>(),
            [(crate::item::Item::SNOWBALL, 3)]
        );
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

#[cfg(test)]
mod remesh_tests {
    use super::*;

    /// A synchronous edit in the loaded-but-undrawn ring must not leave a
    /// chunk queued forever (screenshots and `is_idle` wait on the queue).
    #[test]
    fn edits_outside_mesh_range_do_not_queue_remeshes() {
        let mut w = World::new(Arc::new(Generator::new(1)), Default::default(), 2);
        w.center = Some(IVec3::ZERO);
        let far = IVec3::new(4, 2, 0);
        assert!(!w.in_mesh_range(far));
        w.insert_chunk(far, Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w.dirty.clear();
        w.set_block(far * CHUNK_SIZE_I + IVec3::splat(5), Block::STONE);
        assert!(!w.dirty.contains(&far));
    }
}
