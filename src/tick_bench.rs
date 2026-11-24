//! Headless tick and streaming benchmark (v0.18 Track 10).
//!
//! The live benchmark scenes (`app::benchmark`, `tools/benchmark_scenes.py`)
//! need a window and a GPU, so they run only on the reference Mac. This
//! harness runs the main thread's simulation and streaming work without
//! either, so it can be measured and profiled on a GPU-less Linux machine:
//! each frame it streams the world around the camera (`World::update_players`,
//! which installs finished chunks and submits generation and mesh jobs to the
//! real workers, then drops the finished meshes as the renderer would upload
//! them), and every 50 ms of wall-clock time it runs one game tick with the
//! library's own steps of `Game::tick` in their order: world rules (block,
//! fluid, random and block-entity ticks, light), the automation entity pass
//! (hoppers, carts, dispensers), piston pushes and the mob update with
//! natural spawning.
//!
//! What the desktop tick adds and this leaves out: the player's own movement
//! and vitals (the camera flies a fixed route, as the live scenes' does), the
//! audio and particles fed by entity events, and agents, LAN and controllers.
//! The numbers measure CPU time on the machine that runs it, not the
//! reference Mac's frame time, and are only comparable with runs on the same
//! machine.

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use crate::entity::{Ctx, Entities, MobKind, PlayerId, Target};
use crate::inventory::Stack;
use crate::item::Item;
use crate::rules::GameRules;
use crate::simulation::difficulty::Difficulty;
use crate::simulation::{TICK_SECONDS, lighting::SpawnLight};
use crate::world::block::Block;
use crate::world::redstone_blocks as r;
use crate::world::terrain::{Dimension, Generator, SEA_LEVEL};
use crate::world::{World, WorldRules};

/// The scenes this harness can run.
pub const SCENES: [&str; 6] = ["streaming", "survival", "entities", "automation", "hoppers", "village"];

/// Each tick's time by phase, in milliseconds.
pub const TICK_PHASES: [&str; 5] = ["setup", "world_rules", "automation", "mobs", "drops"];
type TickTimes = [f32; TICK_PHASES.len()];

/// Camera height above the generator's surface along the routes, as the live scenes'.
const ROUTE_CLEARANCE: f64 = 24.0;
/// Mobs in the entities scene, as the live scene's.
const ENTITY_MOBS: usize = 160;

#[derive(Clone, Debug)]
pub struct Options {
    pub scene: String,
    pub seed: u64,
    /// Render distance in 32-block chunks.
    pub render_distance: i32,
    pub warmup: f64,
    pub seconds: f64,
    /// Frames per second the harness paces itself to (its frame work is
    /// measured, not the pacing).
    pub frame_rate: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            scene: "streaming".into(),
            seed: 12345,
            render_distance: 8,
            warmup: 10.0,
            seconds: 60.0,
            frame_rate: 240.0,
        }
    }
}

/// The world, its entities and the camera the harness flies.
pub struct Harness {
    pub world: World,
    pub entities: Entities,
    pub camera: DVec3,
    origin: DVec3,
    scene: String,
    day_time: f64,
    day_count: i64,
    ticks: u64,
    spawning: bool,
}

impl Harness {
    /// A fresh generated world of `options.seed` with the camera at spawn.
    pub fn new(options: &Options) -> Self {
        let generator = Arc::new(Generator::new(options.seed));
        let ground = generator.column(0, 0).height;
        let camera = DVec3::new(0.5, f64::from(ground.max(SEA_LEVEL)) + 2.0, 0.5);
        let world = World::new(generator, Default::default(), options.render_distance);
        let entities = Entities::new(options.seed);
        let mut h = Self {
            world,
            entities,
            camera,
            origin: camera,
            scene: options.scene.clone(),
            day_time: 0.25,
            day_count: 0,
            ticks: 0,
            spawning: true,
        };
        if h.scene == "village" {
            let g = Arc::clone(&h.world.generator);
            if let Some(at) = g.villages.locate_style(&g, IVec3::ZERO, crate::world::village::Style::Plains, 16) {
                let ground = g.column(at.x, at.z).height;
                h.camera = DVec3::new(f64::from(at.x) + 0.5, f64::from(ground) + 2.0, f64::from(at.z) + 0.5);
                h.origin = h.camera;
            }
        }
        h
    }

    /// Streams until the camera's surroundings are generated and the
    /// workers are idle, then builds the scene's fixture. Returns false if
    /// loading took longer than `limit`.
    pub fn load(&mut self, limit: Duration) -> bool {
        let start = Instant::now();
        loop {
            self.stream();
            if self.world.pending_jobs() == 0 && self.world.is_loaded(self.camera.floor().as_ivec3()) {
                break;
            }
            if start.elapsed() > limit {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.build_fixture();
        true
    }

    fn build_fixture(&mut self) {
        let at = self.camera.floor().as_ivec3();
        let floor = matches!(self.scene.as_str(), "entities" | "automation" | "hoppers");
        if floor {
            for x in -12..=12 {
                for z in -12..=12 {
                    self.world.set_block(at + IVec3::new(x, -1, z), Block::STONE);
                }
            }
        }
        match self.scene.as_str() {
            "entities" => {
                // The live scene's population, at dawn with no natural spawning.
                self.spawning = false;
                self.day_time = 0.0;
                for i in 0..ENTITY_MOBS {
                    let kind = [
                        MobKind::Cow,
                        MobKind::Sheep,
                        MobKind::Pig,
                        MobKind::Chicken,
                        MobKind::Zombie,
                        MobKind::Skeleton,
                        MobKind::Wolf,
                        MobKind::Horse,
                    ][i % 8];
                    let p = at + IVec3::new(i as i32 % 21 - 10, 1, i as i32 / 21 - 4);
                    self.entities.spawn(kind, p.as_dvec3() + 0.5);
                }
                self.entities.ensure_mob_ids();
            }
            "automation" => {
                // The live scene's 85 lit furnaces beside powered dust.
                for x in -8..=8 {
                    for z in [-8, -4, 0, 4, 8] {
                        let pos = at + IVec3::new(x, 0, z);
                        self.world.set_block(pos, Block::FURNACE);
                        let furnace = self.world.furnace_mut(pos).unwrap();
                        furnace.input = Some(Stack::new(Item::RAW_IRON, 64));
                        furnace.fuel = Some(Stack::new(Item::COAL, 64));
                        self.world.set_block(pos + IVec3::Z, r::REDSTONE_BLOCK);
                        self.world.set_block(pos + IVec3::Z * 2, r::WIRE);
                    }
                }
            }
            "hoppers" => {
                // 100 chains of chest, two hoppers and chest, with a
                // comparator reading each top chest: hopper block entity
                // ticks, transfers, comparator updates and hopper carts.
                for x in -10..10 {
                    for z in [-10, -6, -2, 2, 6] {
                        let base = at + IVec3::new(x, 0, z);
                        self.world.set_block(base, Block::CHEST);
                        self.world.set_block(base + IVec3::Y, r::hopper(4, false));
                        self.world.set_block(base + IVec3::Y * 2, r::hopper(4, false));
                        self.world.set_block(base + IVec3::Y * 3, Block::CHEST);
                        let top = self.world.chest_mut(base + IVec3::Y * 3).unwrap();
                        top.slots.fill(Some(Stack::new(Item::RAW_IRON, 64)));
                    }
                }
                // Hopper minecarts on a rail line beside them.
                for x in -10..10 {
                    self.world.set_block(at + IVec3::new(x, 0, 10), Block::RAIL);
                }
                for x in (-10..10).step_by(2) {
                    let pos = at.as_dvec3() + DVec3::new(f64::from(x) + 0.5, 0.0, 10.5);
                    self.entities.spawn_cart(crate::entity::minecart::CartKind::Hopper, pos);
                }
            }
            _ => {}
        }
        if floor {
            self.camera = at.as_dvec3() + DVec3::new(0.5, 10.0, -17.5);
        }
        self.origin = self.camera;
    }

    /// Where the camera is `t` seconds into the scene: the live scenes'
    /// routes (12 blocks/s with lateral turns for streaming, a 12-block
    /// orbit for survival) at the generator's surface plus clearance.
    fn fly(&mut self, t: f64) {
        let (offset, dir) = match self.scene.as_str() {
            "streaming" => (
                DVec3::new(t * 12., 0., (t * 0.08).sin() * 32.),
                DVec3::new(12., 0., (t * 0.08).cos() * 0.08 * 32.).normalize(),
            ),
            "survival" => (
                DVec3::new((t * 0.1).sin() * 12., 0., (t * 0.1).cos() * 12.),
                DVec3::new((t * 0.1).cos(), 0., -(t * 0.1).sin()),
            ),
            _ => return,
        };
        let mut pos = self.origin + offset;
        let g = &self.world.generator;
        let ground = (-2..=4)
            .map(|k| {
                let p = pos + dir * f64::from(k * 8);
                f64::from(g.column(p.x.floor() as i32, p.z.floor() as i32).height)
            })
            .fold(f64::from(SEA_LEVEL), f64::max);
        pos.y = ground + ROUTE_CLEARANCE;
        self.camera = pos;
    }

    /// One frame's streaming: install finished chunks, submit jobs and drop
    /// the finished meshes as the renderer takes them.
    pub fn stream(&mut self) {
        self.world.update_players(self.camera, &[]);
        self.world.mesh_uploads.clear();
        self.world.mesh_removals.clear();
    }

    /// One game tick, timed by phase.
    pub fn tick(&mut self) -> TickTimes {
        let mut times = TickTimes::default();
        let mut last = Instant::now();
        let mut mark = |i: usize, times: &mut TickTimes| {
            let now = Instant::now();
            times[i] += now.duration_since(last).as_secs_f32() * 1000.0;
            last = now;
        };
        let rules = GameRules::default();
        let w = &mut self.world;
        w.set_simulation_players(std::iter::once(self.camera));
        w.set_simulation_spectators(std::iter::once(false));
        w.set_tile_drops(true);
        w.rules = WorldRules { difficulty: Difficulty::Normal.id(), ..rules.world_rules() };
        let day_ticks = (self.day_time.rem_euclid(1.) * 24000.).floor() as u32;
        let light = SpawnLight::for_dimension(Dimension::Overworld, day_ticks, 0.0, 0.0);
        w.set_redstone_daylight(self.day_time, light.sky_darken, crate::simulation::lighting::sun_angle(day_ticks));
        let shape = crate::player::Player::new(self.camera).collision_shape();
        let source = crate::world::vibration::EventSource::player(PlayerId::HOST, false);
        w.redstone_contacts_from(std::iter::once((self.camera, shape, source)), &self.entities);
        w.set_shulker_viewers(&[]);
        w.set_chest_viewers(Vec::new());
        mark(0, &mut times);
        crate::simulation::tick_world_rules(w, self.camera, true, 3);
        mark(1, &mut times);
        if !w.piston_pushes().is_empty() {
            self.entities.apply_piston_pushes(w);
        }
        w.tick_automation_entities(&mut self.entities);
        mark(2, &mut times);
        let e = &mut self.entities;
        e.sync_chunk_entities(w);
        e.clock_time = self.day_count * 24_000 + (self.day_time * 24_000.0).round() as i64;
        e.village_time = self.day_time;
        e.village_day = self.day_count;
        e.release_silverfish(&mut w.released_silverfish);
        crate::entity::golem::finish_golems(w, e);
        crate::entity::frog::finish_hatches(w, e);
        for (cell, count) in std::mem::take(&mut w.hatched_turtles) {
            e.hatch_turtles(cell, count);
        }
        let ctx = Ctx {
            spawn_light: light,
            players: vec![Target::new(PlayerId::HOST, self.camera, false)],
            daylight: daylight(self.day_time),
            raining: false,
            spawning: self.spawning,
            dimension: Dimension::Overworld,
        };
        w.set_bee_clock(Dimension::Overworld, day_ticks);
        e.tick_lightning(TICK_SECONDS, w, &ctx, false);
        let events = e.update_world(TICK_SECONDS, w, &ctx, Difficulty::Normal);
        std::hint::black_box(events);
        mark(3, &mut times);
        for (cell, stack) in std::mem::take(&mut w.drops) {
            e.drop_from_block(stack, cell);
        }
        for (cell, xp) in std::mem::take(&mut w.xp_drops) {
            e.spawn_xp(cell.as_dvec3() + DVec3::splat(0.5), xp);
        }
        w.primed_tnt.clear();
        e.clear_pending_sounds();
        w.particles.drain().for_each(drop);
        if self.scene != "entities" {
            (self.day_time, self.day_count) =
                crate::simulation::advance_day(self.day_time, self.day_count, TICK_SECONDS, &rules);
        }
        self.ticks += 1;
        mark(4, &mut times);
        times
    }
}

/// The sky's daylight factor, as the desktop's `sky_state` (a cosine of the
/// day clock, 1 at noon).
fn daylight(day_time: f64) -> f32 {
    let angle = (day_time - 0.25) * std::f64::consts::TAU;
    (angle.cos() * 0.5 + 0.5).clamp(0.0, 1.0) as f32
}

/// One sampled frame.
#[derive(Clone, Copy, Default)]
struct Frame {
    stream: f32,
    ticks: TickTimes,
    ticked: u8,
}

impl Frame {
    fn total(&self) -> f32 {
        self.stream + self.ticks.iter().sum::<f32>()
    }
}

fn percentile(sorted: &[f32], p: f64) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

fn summary(mut values: Vec<f32>) -> Value {
    values.sort_by(f32::total_cmp);
    let n = values.len().max(1) as f64;
    let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
    let r = |v: f64| (v * 1000.0).round() / 1000.0;
    json!({
        "mean": r(mean),
        "p50": r(f64::from(percentile(&values, 0.5))),
        "p99": r(f64::from(percentile(&values, 0.99))),
        "max": r(f64::from(values.last().copied().unwrap_or(0.0))),
    })
}

/// Runs `options.scene`, returning its report: the main thread's work per
/// frame (streaming plus any ticks it ran), per streaming call and per tick
/// phase, and where the slowest 1% of frames spent their time.
pub fn run(options: &Options) -> Result<Value, String> {
    if !SCENES.contains(&options.scene.as_str()) {
        return Err(format!("unknown tick benchmark scene {} (expected one of {})", options.scene, SCENES.join(", ")));
    }
    let created = Instant::now();
    let mut h = Harness::new(options);
    if !h.load(Duration::from_secs(300)) {
        return Err(format!("{}: the scene did not load within 300 seconds", options.scene));
    }
    let load_ms = created.elapsed().as_secs_f64() * 1000.0;
    let frame = Duration::from_secs_f64(1.0 / options.frame_rate);
    let tick = Duration::from_secs_f64(TICK_SECONDS);
    let start = Instant::now();
    let mut next_tick = start;
    let mut frames = Vec::new();
    let mut ticks = Vec::new();
    let mut late_ticks = 0u32;
    let (mut mobs, mut chunks) = (0usize, 0usize);
    loop {
        let begun = Instant::now();
        let t = begun.duration_since(start).as_secs_f64();
        if t >= options.warmup + options.seconds {
            break;
        }
        h.fly(t);
        let mut f = Frame::default();
        let a = Instant::now();
        h.stream();
        f.stream = a.elapsed().as_secs_f32() * 1000.0;
        // A fixed 20 Hz clock that never runs more than 10 ticks a frame.
        let mut due = 0;
        while next_tick <= Instant::now() && due < 10 {
            let times = h.tick();
            for (sum, v) in f.ticks.iter_mut().zip(times) {
                *sum += v;
            }
            if t >= options.warmup {
                ticks.push(times);
            }
            next_tick += tick;
            due += 1;
        }
        if due == 10 {
            next_tick = Instant::now() + tick;
            late_ticks += 1;
        }
        f.ticked = due as u8;
        if t >= options.warmup {
            frames.push(f);
            mobs = mobs.max(h.entities.mobs.len());
            chunks = chunks.max(h.world.loaded_chunks());
        }
        let spent = begun.elapsed();
        if spent < frame {
            std::thread::sleep(frame - spent);
        }
    }
    let totals: Vec<f32> = frames.iter().map(Frame::total).collect();
    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by(|&a, &b| totals[b].total_cmp(&totals[a]));
    let slow: Vec<&Frame> = order.iter().take((frames.len() / 100).max(1)).map(|&i| &frames[i]).collect();
    let slow_mean = |pick: &dyn Fn(&Frame) -> f32| {
        let n = slow.len().max(1) as f64;
        ((slow.iter().map(|f| f64::from(pick(f))).sum::<f64>() / n) * 1000.0).round() / 1000.0
    };
    let mut phases = serde_json::Map::new();
    phases.insert("stream".into(), summary(frames.iter().map(|f| f.stream).collect()));
    for (i, name) in TICK_PHASES.iter().enumerate() {
        phases.insert((*name).into(), summary(ticks.iter().map(|t: &TickTimes| t[i]).collect()));
    }
    let mut slowest = serde_json::Map::new();
    slowest.insert("total".into(), json!(slow_mean(&|f| f.total())));
    slowest.insert("stream".into(), json!(slow_mean(&|f| f.stream)));
    for (i, name) in TICK_PHASES.iter().enumerate() {
        slowest.insert((*name).into(), json!(slow_mean(&|f| f.ticks[i])));
    }
    Ok(json!({
        "scene": options.scene,
        "seed": options.seed,
        "render_distance": options.render_distance,
        "simulation_distance": h.world.simulation_distance(),
        "worker_threads": h.world.worker_threads(),
        "load_ms": load_ms.round(),
        "sample_seconds": options.seconds,
        "frames": frames.len(),
        "ticks": ticks.len(),
        "late_tick_frames": late_ticks,
        "max_mobs": mobs,
        "max_loaded_chunks": chunks,
        "frame_ms": summary(totals.clone()),
        "frames_over_8_33_ms": totals.iter().filter(|&&v| v > 8.33).count(),
        "frames_over_50_ms": totals.iter().filter(|&&v| v > 50.0).count(),
        "tick_ms": summary(ticks.iter().map(|t| t.iter().sum()).collect()),
        "phases_ms": phases,
        "slowest_1pct_mean_ms": slowest,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_name_is_accepted_and_others_are_not() {
        let bad = Options { scene: "nowhere".into(), ..Default::default() };
        assert!(run(&bad).is_err());
        assert_eq!(SCENES.len(), 6);
    }

    #[test]
    fn a_short_hopper_run_reports_its_phases() {
        let options =
            Options { scene: "hoppers".into(), render_distance: 2, warmup: 0.0, seconds: 0.3, ..Default::default() };
        let report = run(&options).unwrap();
        assert!(report["ticks"].as_u64().unwrap() >= 1, "{report}");
        for name in TICK_PHASES {
            assert!(report["phases_ms"][name]["mean"].is_number(), "{report}");
        }
    }
}
