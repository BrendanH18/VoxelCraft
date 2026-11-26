//! v1.0 Track 3: the headless long-session soak.
//!
//! A creative-mode [`Agent`] (the CLI-agent player, with its real session
//! tick) wanders the three dimensions while the library's steps of
//! `Game::tick` run around it, as in the headless tick benchmark
//! ([`crate::tick_bench`]): world rules, automation, beacons, conduits and
//! geysers, entity storage and the mob update with natural spawning. A base
//! in the Overworld holds a wheat farm, a self-harvesting sugar cane farm,
//! two redstone clocks, a hopper chain, furnaces and a zombie spawner.
//!
//! The session repeats an itinerary of legs, each `leg_minutes` of
//! simulated time: idling at the base, walking and then flying outward on a
//! new heading each cycle in the Overworld, walking and flying in the
//! Nether, and flying out across the End. It autosaves every
//! `autosave_seconds` (Java's 6000 ticks), quits and reopens the world at
//! the start of each Overworld walk, and saves and reopens on every change
//! of dimension, as the desktop game does.
//!
//! Every `sample_seconds` it records resident memory, threads, tick and
//! streaming time, the save's size and the length of every queue, cache
//! and retained set the world and the entities expose
//! ([`World::diagnostics`], [`Entities::diagnostics`]), and the size of each
//! saved property. Time is simulated: ticks run as fast as the machine
//! allows, and the walker only waits (without ticking) where the chunk it
//! is about to enter hasn't loaded yet.
//!
//! What this leaves to the desktop: the renderer (meshes are built by the
//! workers and dropped), audio and particles (their requests are drained),
//! the player's survival and inventory screens, controllers and LAN.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};

use crate::agent::Agent;
use crate::entity::{Ctx, Entities, MobKind, PlayerId, Target};
use crate::inventory::Stack;
use crate::item::Item;
use crate::player::MoveInput;
use crate::rules::{GameMode, GameRules};
use crate::simulation::difficulty::Difficulty;
use crate::simulation::{TICK_SECONDS, lighting::SpawnLight};
use crate::world::block::{Block, Facing};
use crate::world::redstone_blocks as r;
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::{Dimension, Generator};
use crate::world::{World, WorldRules};

/// The desktop game's per-frame streaming light budget (`app::STREAM_LIGHT_BUDGET`).
const STREAM_LIGHT_BUDGET: Duration = Duration::from_millis(2);
/// Java's walking sprint speed and creative flight, in blocks per second.
const SPRINT: f64 = 5.612;
const CREATIVE_FLY: f64 = 10.92;
const CREATIVE_SPRINT_FLY: f64 = 21.6;
/// Longest the walker waits for the chunk ahead before giving up the leg.
const LOAD_TIMEOUT: Duration = Duration::from_secs(120);
/// Chunks (32 blocks) around the walker that must be loaded before it
/// moves: past Java's 128-block spawning and despawning distance.
const READY_RADIUS: i32 = 4;

#[derive(Clone, Debug)]
pub struct Options {
    pub seed: u64,
    /// Render distance in 32-block chunks.
    pub render_distance: i32,
    /// Simulated length of the whole session, in hours.
    pub hours: f64,
    /// Simulated length of each leg of the itinerary, in minutes.
    pub leg_minutes: f64,
    /// Simulated seconds between samples.
    pub sample_seconds: f64,
    /// Simulated seconds between autosaves (Java: 6000 ticks).
    pub autosave_seconds: f64,
    /// Where the world is saved. Emptied first.
    pub data_dir: PathBuf,
    /// Where the samples go as CSV (none: only the callback sees them).
    pub csv: Option<PathBuf>,
    /// Random tick speed (Java's default 3).
    pub random_tick_speed: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            seed: 12345,
            render_distance: 6,
            hours: 2.0,
            leg_minutes: 10.0,
            sample_seconds: 60.0,
            autosave_seconds: 300.0,
            data_dir: PathBuf::from("target/soak/world"),
            csv: None,
            random_tick_speed: 3,
        }
    }
}

/// How the walker moves on a leg.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Motion {
    /// Stands at the base.
    Hold,
    /// On the ground at this speed.
    Walk(f64),
    /// This many blocks above the ground at this speed.
    Fly(f64, f64),
}

/// The itinerary: name, dimension and motion of each leg.
pub const LEGS: [&str; 6] = ["base", "walk", "fly", "nether_walk", "nether_fly", "end_fly"];

fn leg(i: usize) -> (&'static str, Dimension, Motion) {
    match i % LEGS.len() {
        0 => (LEGS[0], Dimension::Overworld, Motion::Hold),
        1 => (LEGS[1], Dimension::Overworld, Motion::Walk(SPRINT)),
        2 => (LEGS[2], Dimension::Overworld, Motion::Fly(CREATIVE_SPRINT_FLY, 24.0)),
        3 => (LEGS[3], Dimension::Nether, Motion::Walk(SPRINT)),
        4 => (LEGS[4], Dimension::Nether, Motion::Fly(CREATIVE_FLY, 10.0)),
        _ => (LEGS[5], Dimension::End, Motion::Fly(CREATIVE_SPRINT_FLY, 20.0)),
    }
}

/// One sample: where the session is and every measured value, by name.
#[derive(Clone, Debug)]
pub struct Sample {
    pub leg: &'static str,
    pub dimension: &'static str,
    pub values: Vec<(String, f64)>,
}

impl Sample {
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values.iter().find(|(n, _)| n == name).map(|&(_, v)| v)
    }
}

/// What the whole run did.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub ticks: u64,
    pub samples: usize,
    pub saves: usize,
    pub reopens: usize,
    pub dimension_changes: usize,
    /// Legs cut short because the next chunk didn't load in time.
    pub load_timeouts: usize,
    pub wheat_harvested: usize,
}

/// The process's resident memory, its peak and its thread count (Linux;
/// `None` elsewhere).
pub fn process_memory() -> Option<(u64, u64, u64)> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        status.lines().find_map(|l| l.strip_prefix(name)).and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
    };
    Some((field("VmRSS:")?, field("VmHWM:")?, field("Threads:")?))
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map_or(0, |m| m.len()),
            Err(_) => 0,
        })
        .sum()
}

fn storage_for(root: &Path, dim: Dimension) -> Storage {
    match dim {
        Dimension::Overworld => Storage::new(root),
        Dimension::Nether => Storage::new(root.join("nether")),
        Dimension::End => Storage::new(root.join("end")),
    }
}

/// The saved properties of one dimension (`Game::dimension_props`).
fn dimension_props(world: &World, entities: &Entities) -> BTreeMap<String, String> {
    let mut props = BTreeMap::new();
    props.insert("redstone".to_string(), world.redstone_to_string());
    props.insert("automation".to_string(), world.automation_to_string());
    props.insert("falling".to_string(), world.falling_to_string());
    props.insert("furnaces".to_string(), world.furnaces_to_string());
    props.insert("brewing".to_string(), world.brewing_stands_to_string());
    props.insert("chests".to_string(), world.chests_to_string());
    props.insert("wood".to_string(), world.wood_to_string());
    props.insert("beehives".into(), world.beehives_to_string());
    props.insert("archaeology".into(), world.archaeology_to_string());
    props.insert("jukeboxes".into(), world.jukeboxes_to_string());
    props.insert("creaking_hearts".into(), world.creaking_hearts_to_string());
    props.insert("sculk".into(), world.sculk_to_string());
    props.insert("copper_golem_statues".into(), world.copper_golem_statues_to_string());
    props.insert("trial".into(), world.trial_to_string());
    props.insert("spawners".to_string(), world.spawners_to_string());
    props.insert("utilities".to_string(), world.utilities_to_string());
    props.insert("workstations".to_string(), world.workstations_to_string());
    props.insert("beacons".to_string(), world.beacons_to_string());
    props.insert("conduits".to_string(), world.conduits_to_string());
    props.insert("potent_sulfur".to_string(), world.potent_sulfur_to_string());
    props.insert("crafters".to_string(), world.crafters_to_string());
    entities.save_props(&mut props);
    if let Some(fight) = &entities.fight {
        props.insert("dragon".to_string(), fight.serialize(&entities.crystals));
    }
    props
}

/// Restores a freshly loaded dimension (`Game::restore_dimension`).
fn restore_dimension(world: &mut World, entities: &mut Entities, props: &BTreeMap<String, String>) {
    let get = |k: &str| props.get(k).map(String::as_str);
    if let Some(v) = get("redstone") {
        world.load_redstone(v);
    }
    if let Some(v) = get("automation") {
        world.load_automation(v);
    }
    if let Some(v) = get("falling") {
        world.load_falling(v);
    }
    if let Some(v) = get("brewing") {
        world.load_brewing_stands(v);
    }
    if let Some(v) = get("furnaces") {
        world.load_furnaces(v);
    }
    if let Some(v) = get("chests") {
        world.load_chests(v);
    }
    if let Some(v) = get("archaeology") {
        world.load_archaeology(v);
    }
    if let Some(v) = get("jukeboxes") {
        world.load_jukeboxes(v);
    }
    if let Some(v) = get("beehives") {
        world.load_beehives(v);
    }
    if let Some(v) = get("creaking_hearts") {
        world.load_creaking_hearts(v);
    }
    if let Some(v) = get("sculk") {
        world.load_sculk(v);
    }
    if let Some(v) = get("copper_golem_statues") {
        world.load_copper_golem_statues(v);
    }
    if let Some(v) = get("trial") {
        world.load_trial(v);
    }
    if let Some(v) = get("wood") {
        world.load_wood(v);
    }
    if let Some(v) = get("spawners") {
        world.load_spawners(v);
    }
    if let Some(v) = get("utilities") {
        world.load_utilities(v);
    }
    if let Some(v) = get("workstations") {
        world.load_workstations(v);
    }
    if let Some(v) = get("beacons") {
        world.load_beacons(v);
    }
    if let Some(v) = get("conduits") {
        world.load_conduits(v);
    }
    if let Some(v) = get("crafters") {
        world.load_crafters(v);
    }
    if let Some(v) = get("potent_sulfur") {
        world.load_potent_sulfur(v);
    }
    entities.load_props(props);
    entities.fight = world.generator.end().map(|end| crate::entity::dragon::Fight::load(end, get("dragon")));
    if let Some(fight) = &mut entities.fight {
        fight.restore_crystals(&mut entities.crystals, props.contains_key("end_crystals"));
    }
}

/// Timings gathered between two samples.
#[derive(Default)]
struct Window {
    ticks: u32,
    tick_ms: f64,
    tick_max: f64,
    over_50: u32,
    stream_ms: f64,
    stall: f64,
}

struct Soak {
    options: Options,
    generation: crate::world::terrain::Generation,
    dimension: Dimension,
    world: World,
    entities: Entities,
    agent: Agent,
    rules: GameRules,
    day_time: f64,
    day_count: i64,
    tick: u64,
    /// The base's floor cell (its north-west corner), once built.
    base: Option<IVec3>,
    /// Where the walker is heading this leg, and where it started.
    heading: DVec3,
    window: Window,
    last_save_ms: f64,
    last_load_ms: f64,
    summary: Summary,
    started: Instant,
}

impl Soak {
    fn new(options: Options) -> Result<Self, String> {
        let _ = std::fs::remove_dir_all(&options.data_dir);
        std::fs::create_dir_all(&options.data_dir).map_err(|e| format!("{}: {e}", options.data_dir.display()))?;
        let generator = Arc::new(Generator::new(options.seed));
        let generation = generator.generation;
        let ground = generator.column(0, 0).height.max(crate::world::terrain::SEA_LEVEL);
        let spawn = DVec3::new(0.5, f64::from(ground) + 1.0, 0.5);
        let mut world = World::new(generator, Default::default(), options.render_distance);
        world.set_stream_light_budget(Some(STREAM_LIGHT_BUDGET));
        let mut agent = Agent::new(spawn);
        agent.set_mode(GameMode::Creative);
        agent.session.player.flying = true;
        let entities = Entities::new(options.seed);
        world.update_players(spawn, &[]);
        Ok(Self {
            options,
            generation,
            dimension: Dimension::Overworld,
            world,
            entities,
            agent,
            rules: GameRules::default(),
            day_time: 0.25,
            day_count: 0,
            tick: 0,
            base: None,
            heading: DVec3::X,
            window: Window::default(),
            last_save_ms: 0.0,
            last_load_ms: 0.0,
            summary: Summary::default(),
            started: Instant::now(),
        })
    }

    fn pos(&self) -> DVec3 {
        self.agent.session.player.pos
    }

    /// One frame's streaming: install finished chunks, submit jobs and drop
    /// the finished meshes as the renderer would upload them.
    fn stream(&mut self) {
        let a = Instant::now();
        self.world.update_players(self.pos(), &[]);
        self.world.mesh_uploads.clear();
        self.world.mesh_removals.clear();
        self.window.stream_ms += a.elapsed().as_secs_f64() * 1000.0;
    }

    /// Streams (without ticking) until the chunks around `p` that the
    /// simulation reads have loaded: every column within [`READY_RADIUS`]
    /// chunks, from the row below to the row above. A player on the
    /// reference machine sees the terrain this close keep up with them;
    /// unpaced ticks would otherwise outrun the workers and simulate an
    /// empty world. Returns false on timeout.
    fn wait_for(&mut self, p: DVec3) -> bool {
        let cell = p.floor().as_ivec3();
        // Within the load range of a small render distance too.
        let radius = READY_RADIUS.min(self.options.render_distance + 1);
        let loaded = |w: &World| {
            let rows = (cell.y - 32).max(w.min_y())..=(cell.y + 32).min(w.max_y() - 1);
            (-radius..=radius).all(|dx| {
                (-radius..=radius).all(|dz| {
                    dx * dx + dz * dz > radius * radius
                        || rows
                            .clone()
                            .step_by(32)
                            .all(|y| w.is_loaded(IVec3::new(cell.x + dx * 32, y, cell.z + dz * 32)))
                })
            })
        };
        if loaded(&self.world) {
            return true;
        }
        let start = Instant::now();
        while !loaded(&self.world) {
            if start.elapsed() > LOAD_TIMEOUT {
                return false;
            }
            self.stream();
            std::thread::sleep(Duration::from_millis(1));
        }
        self.window.stall += start.elapsed().as_secs_f64();
        true
    }

    /// The walkable floor under `(x, z)`: the top of the first non-air
    /// block below an air gap of two, scanning down from the dimension's
    /// top (so the Nether's roof never counts). `None` over the void or
    /// where nothing of the column has loaded.
    fn floor(&self, x: f64, z: f64) -> Option<f64> {
        let (x, z) = (x.floor() as i32, z.floor() as i32);
        let top = match self.dimension {
            Dimension::Nether => 122,
            _ => self.world.max_y() - 1,
        };
        let mut air = 0;
        for y in (self.world.min_y()..=top).rev() {
            // Streaming loads the rows near the walker first: an unloaded
            // chunk above it is unknown, not ground.
            let Some(b) = self.world.get_block(IVec3::new(x, y, z)) else {
                air = 0;
                continue;
            };
            if b == Block::AIR {
                air += 1;
            } else {
                if air >= 2 {
                    return Some(f64::from(y + 1));
                }
                air = 0;
            }
        }
        None
    }

    /// Where the walker goes next on this leg.
    fn step(&mut self, motion: Motion, t: f64) -> Option<DVec3> {
        let (speed, clearance) = match motion {
            Motion::Hold => return None,
            Motion::Walk(s) => (s, 0.0),
            Motion::Fly(s, c) => (s, c),
        };
        // Wander: the heading swings slowly either side of the leg's course.
        let swing = (t * 0.05).sin() * 0.6;
        let (s, c) = swing.sin_cos();
        let h = self.heading;
        let dir = DVec3::new(h.x * c - h.z * s, 0.0, h.x * s + h.z * c);
        let mut next = self.pos() + dir * speed * TICK_SECONDS;
        // The Overworld walker follows the generator's surface (above the sea),
        // as the benchmark routes do, so it never wanders into caves while the
        // rows above stream in; the Nether and End have no such surface.
        let floor = if self.dimension == Dimension::Overworld {
            let column = self.world.generator.column(next.x.floor() as i32, next.z.floor() as i32);
            Some(f64::from(column.height.max(crate::world::terrain::SEA_LEVEL) + 1))
        } else {
            self.floor(next.x, next.z)
        };
        let target = match (floor, self.dimension) {
            (Some(f), Dimension::Nether) => (f + clearance).min(120.0),
            (Some(f), _) => f + clearance,
            (None, _) => self.pos().y,
        };
        // Climb and drop at most a block and a half a tick.
        next.y = self.pos().y + (target - self.pos().y).clamp(-1.5, 1.5);
        Some(next)
    }

    /// Saves the current dimension (`Game::save`).
    fn save(&mut self) -> Result<(), String> {
        let a = Instant::now();
        let seed = self.world.generator.seed;
        let chunks = self.world.modified_chunks();
        let props = dimension_props(&self.world, &self.entities);
        let storage = storage_for(&self.options.data_dir, self.dimension);
        let p = &self.agent.session.player;
        let player = (self.dimension == Dimension::Overworld).then_some((p.pos, p.yaw, p.pitch));
        storage.save(&LevelInfo { seed, player, props }, &chunks).map_err(|e| format!("save: {e}"))?;
        self.last_save_ms = a.elapsed().as_secs_f64() * 1000.0;
        self.summary.saves += 1;
        Ok(())
    }

    /// Saves, drops the world and its entities, and opens `to` from disk
    /// with the walker at `at`, waiting for its surroundings to load.
    fn reopen(&mut self, to: Dimension, at: DVec3) -> Result<(), String> {
        self.save()?;
        let a = Instant::now();
        let storage = storage_for(&self.options.data_dir, to);
        let (chunks, props) = if storage.exists() {
            let chunks = storage.load_chunks().map_err(|e| format!("load {to:?}: {e}"))?;
            let props = storage.load_level().map(|l| l.props).unwrap_or_default();
            (chunks, props)
        } else {
            Default::default()
        };
        let seed = self.world.generator.seed;
        let maps = std::mem::take(&mut self.world.maps);
        let loot = std::mem::take(&mut self.world.loot_sequences);
        // Quit: the old world, its workers and entities are dropped as the
        // new ones (with nothing loaded yet) replace them.
        let generator = Arc::new(Generator::with_generation(seed, to, self.generation));
        let mut world = World::new(generator, chunks, self.options.render_distance);
        world.set_stream_light_budget(Some(STREAM_LIGHT_BUDGET));
        world.maps = maps;
        world.loot_sequences = loot;
        drop(std::mem::replace(&mut self.world, world));
        drop(std::mem::replace(&mut self.entities, Entities::new(seed)));
        restore_dimension(&mut self.world, &mut self.entities, &props);
        if to != self.dimension {
            self.summary.dimension_changes += 1;
        } else {
            self.summary.reopens += 1;
        }
        self.dimension = to;
        self.agent.session.player.pos = at;
        self.agent.session.player.vel = DVec3::ZERO;
        self.world.update_players(at, &[]);
        if !self.wait_for(at) {
            return Err(format!("{to:?} did not load around {at} within {LOAD_TIMEOUT:?}"));
        }
        // Settle the arrival point onto the ground.
        if let Some(f) = self.floor(at.x, at.z) {
            self.agent.session.player.pos.y = f;
        }
        self.window.stall = 0.0;
        self.last_load_ms = a.elapsed().as_secs_f64() * 1000.0;
        Ok(())
    }

    /// Builds the base's fixtures around `b` (the floor's north-west corner).
    fn build_base(&mut self, b: IVec3) {
        let w = &mut self.world;
        // A clear stone pad 24 x 24 with four blocks of air over it.
        for x in -2..22 {
            for z in -2..22 {
                w.set_block(b + IVec3::new(x, -1, z), Block::STONE);
                for y in 0..5 {
                    w.set_block(b + IVec3::new(x, y, z), Block::AIR);
                }
            }
        }
        // Wheat: 7 x 7 farmland around a water source.
        for x in 0..7 {
            for z in 0..7 {
                let p = b + IVec3::new(x, -1, z);
                if (x, z) == (3, 3) {
                    w.set_block(p, Block::WATER);
                } else {
                    w.set_block(p, Block::FARMLAND);
                    w.set_block(p + IVec3::Y, Block::wheat(0));
                }
            }
        }
        // The self-harvesting sugar cane farm of the reference contraptions.
        let c = b + IVec3::new(10, -1, 1);
        for (d, block) in [
            (IVec3::new(1, 0, 1), Block::STONE),
            (IVec3::new(2, 0, 1), Block::STONE),
            (IVec3::new(0, 1, 1), Block::STONE),
            (IVec3::new(1, 1, 0), Block::STONE),
            (IVec3::new(1, 1, 2), Block::STONE),
            (IVec3::new(1, 1, 1), Block::WATER),
            (IVec3::new(2, 1, 1), Block::SAND),
            (IVec3::new(2, 2, 1), Block::SUGAR_CANE),
            (IVec3::new(4, 3, 1), Block::STONE),
            (IVec3::new(3, 3, 1), r::piston(3, false, false)),
            (IVec3::new(4, 4, 1), r::wire(0)),
            (IVec3::new(3, 4, 1), r::observer(3, false)),
        ] {
            w.set_block(c + d, block);
        }
        // An observer clock driving dust and a lamp.
        let o = b + IVec3::new(10, 0, 8);
        w.set_block(o, r::observer(2, false));
        w.set_block(o + IVec3::X, r::observer(3, false));
        w.set_block(o + IVec3::X * 2, r::wire(0));
        w.set_block(o + IVec3::new(3, 0, 0), r::LAMP);
        // A repeater loop, started by a button press.
        let l = b + IVec3::new(10, 0, 12);
        let rows: [&[u8]; 3] = [b"bw>w", b".^.v", b".w<w"];
        for (z, row) in rows.iter().enumerate() {
            for (x, ch) in row.iter().enumerate() {
                let block = match ch {
                    b'b' => r::button(0, false, false),
                    b'w' => r::wire(0),
                    b'>' => r::repeater(Facing::East, 4, false),
                    b'v' => r::repeater(Facing::South, 4, false),
                    b'<' => r::repeater(Facing::West, 4, false),
                    b'^' => r::repeater(Facing::North, 4, false),
                    _ => continue,
                };
                w.set_block(l + IVec3::new(x as i32, 0, z as i32), block);
            }
        }
        w.use_redstone(l);
        // A hopper chain between two chests, refilled at every base visit.
        let h = b + IVec3::new(16, 0, 2);
        w.set_block(h, Block::CHEST);
        w.set_block(h + IVec3::Y, r::hopper(4, false));
        w.set_block(h + IVec3::Y * 2, r::hopper(4, false));
        w.set_block(h + IVec3::Y * 3, Block::CHEST);
        // Furnaces, refilled the same way.
        for i in 0..4 {
            w.set_block(b + IVec3::new(16 + i, 0, 8), Block::FURNACE);
        }
        // A zombie spawner in the open (they burn by day).
        w.set_block(b + IVec3::new(4, 0, 16), Block::SPAWNER);
        w.set_spawner(b + IVec3::new(4, 0, 16), MobKind::Zombie);
        self.base = Some(b);
        self.refill_base();
    }

    /// What a player tending the base does on each visit: refill the hopper
    /// chain and furnaces, and replant the ripe wheat.
    fn refill_base(&mut self) {
        let Some(b) = self.base else { return };
        let w = &mut self.world;
        if let Some(top) = w.chest_mut(b + IVec3::new(16, 3, 2)) {
            top.slots.iter_mut().take(9).for_each(|s| *s = Some(Stack::new(Item::RAW_IRON, 64)));
        }
        if let Some(bottom) = w.chest_mut(b + IVec3::new(16, 0, 2)) {
            bottom.slots.fill(None);
        }
        for i in 0..4 {
            if let Some(f) = w.furnace_mut(b + IVec3::new(16 + i, 0, 8)) {
                f.input = Some(Stack::new(Item::RAW_IRON, 64));
                f.fuel = Some(Stack::new(Item::COAL, 64));
                f.output = None;
            }
        }
        self.harvest();
    }

    fn harvest(&mut self) {
        let Some(b) = self.base else { return };
        for x in 0..7 {
            for z in 0..7 {
                let p = b + IVec3::new(x, 0, z);
                if self.world.get_block(p).is_some_and(|c| c == Block::wheat(7)) {
                    self.world.set_block(p, Block::wheat(0));
                    self.summary.wheat_harvested += 1;
                }
            }
        }
    }

    /// One game tick: the library's steps of `Game::tick`, in its order.
    fn game_tick(&mut self) {
        let a = Instant::now();
        let dt = TICK_SECONDS;
        let pos = self.pos();
        let dim = self.dimension;
        // Rain one quarter of the time, thunder within it now and then.
        let minute = self.tick / 1200;
        let raining = dim == Dimension::Overworld && minute % 20 >= 15;
        let thundering = raining && minute % 40 >= 35;
        let w = &mut self.world;
        w.set_simulation_players(std::iter::once(pos));
        w.set_simulation_spectators(std::iter::once(false));
        w.set_tile_drops(true);
        w.rules = WorldRules { difficulty: Difficulty::Normal.id(), ..self.rules.world_rules() };
        w.raining = raining;
        self.agent.hold(MoveInput::default(), false, false);
        self.agent.tick_rules(w, &mut self.entities, Difficulty::Normal, &self.rules);
        self.agent.events.clear();
        let ticks = (self.day_time.rem_euclid(1.) * 24000.).floor() as u32;
        let (rain, thunder) = (f32::from(u8::from(raining)), f32::from(u8::from(thundering)));
        let light = SpawnLight::for_dimension(dim, ticks, rain, thunder);
        let sun = if dim == Dimension::Overworld { crate::simulation::lighting::sun_angle(ticks) } else { 0.0 };
        w.set_redstone_daylight(self.day_time, light.sky_darken, sun);
        let shape = self.agent.session.player.collision_shape();
        let source = crate::world::vibration::EventSource::player(PlayerId::HOST, false);
        w.redstone_contacts_from(std::iter::once((pos, shape, source)), &self.entities);
        w.set_shulker_viewers(&[]);
        w.set_container_viewers(&[]);
        w.set_chest_viewers(Vec::new());
        crate::simulation::tick_world_rules(w, pos, true, self.options.random_tick_speed);
        let _ = w.take_block_pushes();
        if !w.piston_pushes().is_empty() {
            self.entities.apply_piston_pushes(w);
        }
        self.entities.stalactite_hits(w.stalactite_hits());
        w.tick_automation_entities(&mut self.entities);
        let time = w.maps.game_ticks;
        w.tick_beacons(time);
        drop(w.take_beacon_events());
        drop(w.take_crafter_events());
        let mut seed = time ^ 0xC0_D017;
        w.tick_conduits(time, move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % 40
        });
        drop(w.take_conduit_events());
        w.tick_potent_sulfur(time);
        drop(w.take_potent_sulfur_events());
        // The mob update (`Game::update_mobs`).
        let e = &mut self.entities;
        e.sync_chunk_entities(w);
        e.clock_time = self.day_count * 24_000 + (self.day_time * 24_000.0).round() as i64;
        e.village_time = self.day_time;
        e.village_day = self.day_count;
        e.thundering = thundering;
        for (cell, short_fuse) in std::mem::take(&mut w.primed_tnt) {
            e.prime_tnt(cell, short_fuse);
        }
        e.release_silverfish(&mut w.released_silverfish);
        crate::entity::golem::finish_golems(w, e);
        crate::entity::frog::finish_hatches(w, e);
        for (cell, count) in std::mem::take(&mut w.hatched_turtles) {
            e.hatch_turtles(cell, count);
        }
        let daylight = if dim == Dimension::Overworld {
            let angle = (self.day_time - 0.25) * std::f64::consts::TAU;
            (angle.cos() * 0.5 + 0.5).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let ctx = Ctx {
            spawn_light: light,
            players: vec![Target::new(PlayerId::HOST, pos, false)],
            daylight,
            raining,
            spawning: true,
            dimension: dim,
        };
        w.set_bee_clock(dim, ticks);
        e.tick_lightning(dt, w, &ctx, thundering);
        drop(e.update_world(dt, w, &ctx, Difficulty::Normal));
        for (cell, stack) in std::mem::take(&mut w.drops) {
            e.drop_from_block(stack, cell);
        }
        for (cell, xp) in std::mem::take(&mut w.xp_drops) {
            e.spawn_xp(cell.as_dvec3() + DVec3::splat(0.5), xp);
        }
        e.clear_pending_sounds();
        e.particles.drain().for_each(drop);
        // What the desktop's audio, particles and HUD take each frame.
        w.particles.drain().for_each(drop);
        w.notes.clear();
        w.trip_sounds.clear();
        w.wither_skulls.clear();
        w.brews_done.clear();
        w.chorus_events.clear();
        w.page_turns.clear();
        w.jukebox_events.clear();
        w.records.clear();
        w.records_started.clear();
        (self.day_time, self.day_count) =
            crate::simulation::advance_day(self.day_time, self.day_count, dt, &self.rules);
        self.tick += 1;
        let ms = a.elapsed().as_secs_f64() * 1000.0;
        self.window.ticks += 1;
        self.window.tick_ms += ms;
        self.window.tick_max = self.window.tick_max.max(ms);
        self.window.over_50 += u32::from(ms > 50.0);
    }

    fn sample(&mut self, leg: &'static str) -> Sample {
        let p = self.pos();
        let n = f64::from(self.window.ticks.max(1));
        let (rss, hwm, threads) = process_memory().unwrap_or((0, 0, 0));
        let mut values: Vec<(String, f64)> = vec![
            ("tick".into(), self.tick as f64),
            ("sim_minutes".into(), self.tick as f64 * TICK_SECONDS / 60.0),
            ("wall_seconds".into(), self.started.elapsed().as_secs_f64()),
            ("x".into(), p.x.round()),
            ("y".into(), p.y.round()),
            ("z".into(), p.z.round()),
            ("rss_kb".into(), rss as f64),
            ("hwm_kb".into(), hwm as f64),
            ("threads".into(), threads as f64),
            ("tick_ms_mean".into(), self.window.tick_ms / n),
            ("tick_ms_max".into(), self.window.tick_max),
            ("ticks_over_50ms".into(), f64::from(self.window.over_50)),
            ("stream_ms_per_tick".into(), self.window.stream_ms / n),
            ("stall_seconds".into(), self.window.stall),
            ("save_ms".into(), self.last_save_ms),
            ("load_ms".into(), self.last_load_ms),
            ("save_bytes".into(), dir_size(&self.options.data_dir) as f64),
            ("saves".into(), self.summary.saves as f64),
            ("wheat_harvested".into(), self.summary.wheat_harvested as f64),
        ];
        values.extend(self.world.diagnostics().into_iter().map(|(k, v)| (format!("w.{k}"), v as f64)));
        values.extend(self.entities.diagnostics().into_iter().map(|(k, v)| (format!("e.{k}"), v as f64)));
        let props = dimension_props(&self.world, &self.entities);
        for &key in PROP_KEYS {
            values.push((format!("p.{key}_bytes"), props.get(key).map_or(0, String::len) as f64));
        }
        self.window = Window::default();
        self.summary.samples += 1;
        Sample { leg, dimension: self.dimension.name(), values }
    }
}

/// The saved properties sampled, in a fixed order (the CSV's columns).
const PROP_KEYS: &[&str] = &[
    "redstone",
    "automation",
    "falling",
    "furnaces",
    "brewing",
    "chests",
    "wood",
    "beehives",
    "archaeology",
    "jukeboxes",
    "creaking_hearts",
    "sculk",
    "copper_golem_statues",
    "trial",
    "spawners",
    "utilities",
    "workstations",
    "beacons",
    "conduits",
    "potent_sulfur",
    "crafters",
    "nether_mobs",
    "villagers",
    "items",
    "orbs",
    "minecarts",
    "decor",
    "potion_clouds",
    "tridents",
    "raids",
    "end_crystals",
    "dragon",
];

/// Writes samples as CSV, the header from the first.
struct Csv(std::io::BufWriter<std::fs::File>);

impl Csv {
    fn create(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self(std::io::BufWriter::new(file)))
    }

    fn write(&mut self, sample: &Sample, header: bool) -> std::io::Result<()> {
        use std::io::Write;
        if header {
            let names: Vec<&str> = sample.values.iter().map(|(n, _)| n.as_str()).collect();
            writeln!(self.0, "leg,dimension,{}", names.join(","))?;
        }
        let values: Vec<String> =
            sample.values.iter().map(|(_, v)| format!("{}", (v * 1000.0).round() / 1000.0)).collect();
        writeln!(self.0, "{},{},{}", sample.leg, sample.dimension, values.join(","))?;
        self.0.flush()
    }
}

/// Runs the soak, handing each sample to `on_sample` (and to the CSV).
pub fn run(options: &Options, mut on_sample: impl FnMut(&Sample)) -> Result<Summary, String> {
    let mut csv = options.csv.as_deref().map(Csv::create).transpose()?;
    let mut s = Soak::new(options.clone())?;
    let total_ticks = (options.hours * 3600.0 / TICK_SECONDS).round() as u64;
    let leg_ticks = ((options.leg_minutes * 60.0 / TICK_SECONDS).round() as u64).max(1);
    let sample_ticks = ((options.sample_seconds / TICK_SECONDS).round() as u64).max(1);
    let autosave_ticks = ((options.autosave_seconds / TICK_SECONDS).round() as u64).max(1);
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    let mut leg_index = 0usize;
    let mut since_save = 0u64;
    while s.tick < total_ticks {
        let (name, dim, motion) = leg(leg_index);
        let cycle = leg_index / LEGS.len();
        let angle = cycle as f64 * golden + (leg_index % LEGS.len()) as f64 * 0.7;
        s.heading = DVec3::new(angle.cos(), 0.0, angle.sin());
        // Arrive: the walk leg quits and reopens the world; the others
        // change dimension where needed (saving and reopening, as a portal does).
        match name {
            "base" => {
                let at = s.base.map_or(s.pos(), |b| b.as_dvec3() + DVec3::new(8.5, 0.0, -1.5));
                if s.dimension != Dimension::Overworld {
                    s.reopen(Dimension::Overworld, at)?;
                } else if !s.wait_for(at) {
                    return Err("the spawn did not load".into());
                }
                if s.base.is_none() {
                    let p = s.pos();
                    let floor = s.floor(p.x, p.z).unwrap_or(p.y);
                    let b = IVec3::new(p.x.floor() as i32 - 8, floor as i32, p.z.floor() as i32 + 2);
                    s.build_base(b);
                    s.agent.session.player.pos = b.as_dvec3() + DVec3::new(8.5, 0.0, -1.5);
                } else {
                    s.refill_base();
                }
            }
            _ if dim != s.dimension => {
                let at = match dim {
                    Dimension::Nether => DVec3::new(0.5, 70.0, 0.5),
                    Dimension::End => DVec3::new(100.5, 50.0, 0.5),
                    Dimension::Overworld => s.base.map_or(DVec3::ZERO, |b| b.as_dvec3()),
                };
                s.reopen(dim, at)?;
            }
            "walk" => {
                // Quit at the base and come back, then set out along this
                // cycle's heading.
                let at = s.base.map_or(s.pos(), |b| b.as_dvec3() + DVec3::new(8.5, 0.0, 8.5) + s.heading * 24.0);
                s.reopen(Dimension::Overworld, at)?;
            }
            _ => {}
        }
        let leg_start = s.tick;
        let mut t = 0.0;
        while s.tick - leg_start < leg_ticks && s.tick < total_ticks {
            if let Some(next) = s.step(motion, t) {
                // Move, then let the terrain around the new spot load (no
                // tick runs meanwhile), so streaming centres on it.
                let yaw = (next.z - s.pos().z).atan2(next.x - s.pos().x) as f32;
                let player = &mut s.agent.session.player;
                player.pos = next;
                player.vel = DVec3::ZERO;
                player.yaw = yaw;
                if !s.wait_for(next) {
                    s.summary.load_timeouts += 1;
                    break;
                }
            }
            s.stream();
            s.world.update_block_light();
            s.game_tick();
            t += TICK_SECONDS;
            since_save += 1;
            if since_save >= autosave_ticks {
                s.save()?;
                since_save = 0;
            }
            if s.tick % sample_ticks == 0 {
                if name == "base" {
                    s.harvest();
                }
                let sample = s.sample(name);
                if let Some(csv) = &mut csv {
                    csv.write(&sample, s.summary.samples == 1).map_err(|e| format!("csv: {e}"))?;
                }
                on_sample(&sample);
            }
        }
        leg_index += 1;
    }
    s.save()?;
    s.summary.ticks = s.tick;
    Ok(s.summary)
}
