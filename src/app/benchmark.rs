//! Repeatable live-game performance scenes. Runs the ordinary frame and 20 Hz simulation loop.
//!
//! A run ignores keyboard, pointer, controller and focus input, never saves,
//! and reports whether its samples are valid: unpaused 20 Hz simulation,
//! presented frames and (for the entity scene) a stable population.
use super::Game;
use crate::{
    Args,
    entity::MobKind,
    inventory::Stack,
    item::Item,
    world::{
        block::{Block, RenderKind},
        forms,
        terrain::SEA_LEVEL,
    },
};
use glam::{DVec3, IVec3};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use voxelcraft::performance::{self as perf, ENTITY_SCENE_MOBS, MENUS_SCREENS, RunEvidence};

/// Camera height above the generator's surface along flight routes.
const ROUTE_CLEARANCE: f64 = 24.0;
/// Grid spacing for sampled generator surface heights.
const HEIGHT_GRID: i32 = 8;
/// Entity scene time of day: dawn, below the undead sunburn threshold, held fixed.
const ENTITY_SCENE_TIME: f64 = 0.0;
/// Block states the materials scene draws from: the v0.12 track reservations
/// (docs/architecture.md), below the unassigned reserve.
const MATERIALS_STATES: (u16, u16) = (16384, 29696);

pub(super) struct Run {
    scene: String,
    output: PathBuf,
    warmup: f64,
    duration: f64,
    created: Instant,
    configured: bool,
    ready: Option<Instant>,
    sample_start: Option<Instant>,
    previous: Option<Instant>,
    origin: DVec3,
    frames: Vec<f64>,
    /// Each sampled frame's time by phase, beside `frames`.
    phases: Vec<perf::PhaseTimes>,
    /// The frame in progress: its phases so far and when the last one ended.
    phase: (perf::PhaseTimes, Instant),
    /// Each sample's frame serial, for its GPU time (0: no frame was drawn).
    serials: Vec<u64>,
    /// Each sample's wait for a drawable in ms (0: no frame was drawn).
    acquire: Vec<f32>,
    last_serial: u64,
    offscreen: bool,
    window_mode: &'static str,
    ticks_before: u64,
    discarded_before: Duration,
    paused_before: Duration,
    skipped_before: u64,
    entities_start: usize,
    /// Population the scene must hold, for scenes that fix one.
    expected_population: Option<usize>,
    /// The entity scene's own mobs, by uid; the world's generation herds are
    /// cleared once loading finishes so the scene holds exactly these.
    fixture_mobs: Vec<u32>,
    /// Generator surface heights on the `HEIGHT_GRID` lattice.
    heights: HashMap<(i32, i32), i32>,
    /// The menus scene's open screen.
    screen: Option<&'static str>,
    cold_load_ms: f64,
    prepared: bool,
    finished: bool,
}
/// Ends phase `index` of [`perf::FRAME_PHASES`] in the frame being drawn:
/// the time since the previous mark counts toward it.
pub(super) fn mark(run: &mut Option<Run>, index: usize) {
    if let Some(run) = run {
        let now = Instant::now();
        run.phase.0[index] += now.duration_since(run.phase.1).as_secs_f32() * 1000.;
        run.phase.1 = now;
    }
}

impl Run {
    pub fn from_args(args: &Args) -> Option<Self> {
        Some(Self {
            scene: args.benchmark_scene.clone()?,
            output: args.benchmark_output.clone()?,
            warmup: args.benchmark_warmup,
            duration: args.benchmark_duration,
            created: Instant::now(),
            configured: false,
            ready: None,
            sample_start: None,
            previous: None,
            origin: DVec3::ZERO,
            frames: Vec::new(),
            phases: Vec::new(),
            phase: (Default::default(), Instant::now()),
            serials: Vec::new(),
            acquire: Vec::new(),
            last_serial: 0,
            offscreen: args.benchmark_offscreen,
            window_mode: args.benchmark_window_mode,
            ticks_before: 0,
            discarded_before: Duration::ZERO,
            paused_before: Duration::ZERO,
            skipped_before: 0,
            entities_start: 0,
            expected_population: None,
            fixture_mobs: Vec::new(),
            heights: HashMap::new(),
            screen: None,
            cold_load_ms: 0.,
            prepared: false,
            finished: false,
        })
    }
}

/// Route position (relative to the scene origin) and horizontal direction of travel at `t` seconds.
fn route(scene: &str, t: f64) -> Option<(DVec3, DVec3)> {
    match scene {
        // 12 blocks/s crosses a Java chunk every 1.33 s; changes streaming centres normally.
        "streaming" => Some((
            DVec3::new(t * 12., 0., (t * 0.08).sin() * 32.),
            DVec3::new(12., 0., (t * 0.08).cos() * 0.08 * 32.).normalize(),
        )),
        "survival" => Some((
            DVec3::new((t * 0.1).sin() * 12., 0., (t * 0.1).cos() * 12.),
            DVec3::new((t * 0.1).cos(), 0., -(t * 0.1).sin()),
        )),
        _ => None,
    }
}

impl Game {
    /// Generator surface height, bilinearly interpolated between lattice samples so routes stay smooth.
    fn route_ground(&self, heights: &mut HashMap<(i32, i32), i32>, x: f64, z: f64) -> f64 {
        let grid = f64::from(HEIGHT_GRID);
        let (gx, gz) = ((x / grid).floor(), (z / grid).floor());
        let (fx, fz) = (x / grid - gx, z / grid - gz);
        let mut at = |dx: i32, dz: i32| {
            let key = (gx as i32 + dx, gz as i32 + dz);
            f64::from(
                *heights
                    .entry(key)
                    .or_insert_with(|| self.world.generator.column(key.0 * HEIGHT_GRID, key.1 * HEIGHT_GRID).height),
            )
        };
        let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
        (a * (1. - fx) + b * fx) * (1. - fz) + (c * (1. - fx) + d * fx) * fz
    }

    /// Camera height that clears the generated surface around and ahead of `pos`.
    fn route_height(&self, heights: &mut HashMap<(i32, i32), i32>, pos: DVec3, dir: DVec3) -> f64 {
        let ground = (-2..=4)
            .map(|k| {
                let p = pos + dir * f64::from(k * HEIGHT_GRID);
                self.route_ground(heights, p.x, p.z)
            })
            .fold(f64::from(SEA_LEVEL), f64::max);
        ground + ROUTE_CLEARANCE
    }

    pub(super) fn benchmark_frame(&mut self) {
        let Some(mut run) = self.benchmark.take() else {
            return;
        };
        let mut now = Instant::now();
        if run.finished {
            self.benchmark = Some(run);
            return;
        }
        if !run.configured {
            run.configured = true;
            if run.scene == "entities" {
                // Undead burn in daylight and natural spawning varies the count; hold the
                // specified population with a fixed dawn and no natural spawns.
                for rule in ["advance_time", "spawn_mobs"] {
                    self.gamerules.set(rule, "false").expect("benchmark gamerule");
                }
                self.day_time = ENTITY_SCENE_TIME;
            }
        }
        if !run.prepared
            && self.world.pending_jobs() == 0
            && self.world.is_loaded(self.session.player.pos.floor().as_ivec3())
        {
            run.origin = self.session.player.pos;
            if !matches!(run.scene.as_str(), "survival" | "streaming" | "menus") {
                let at = run.origin.floor().as_ivec3();
                // A compact inspection platform, built on the actual seed's surface.
                for x in -12..=12 {
                    for z in -12..=12 {
                        self.world.set_block(at + IVec3::new(x, -1, z), Block::STONE);
                    }
                }
                match run.scene.as_str() {
                    "building" => {
                        for x in -10..=10 {
                            for z in -10..=10 {
                                if (x + z) % 3 == 0 {
                                    for y in 0..8 {
                                        let block = if y % 3 == 0 { forms::wood_id(9, 0) } else { Block::GLASS };
                                        self.world.set_block(at + IVec3::new(x, y, z), block);
                                    }
                                }
                            }
                        }
                    }
                    "entities" => {
                        let preexisting = self.mobs.entities.mobs.len();
                        for i in 0..ENTITY_SCENE_MOBS {
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
                            self.mobs.entities.spawn(
                                kind,
                                (at + IVec3::new(i as i32 % 21 - 10, 1, i as i32 / 21 - 4)).as_dvec3() + 0.5,
                            );
                        }
                        self.mobs.entities.ensure_mob_ids();
                        run.fixture_mobs = self.mobs.entities.mobs[preexisting..].iter().map(|m| m.uid).collect();
                        run.expected_population = Some(ENTITY_SCENE_MOBS);
                    }
                    "automation" => {
                        use crate::world::redstone_blocks as r;
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
                    "materials" => self.build_materials_scene(at),
                    _ => {}
                }
                self.session.player.pos = run.origin + DVec3::new(0., 10., -18.);
                self.session.player.pitch = -0.28;
            }
            self.session.player.can_fly = true;
            self.session.player.flying = true;
            self.session.vitals.health = 10000.;
            run.origin = self.session.player.pos;
            run.cold_load_ms = run.created.elapsed().as_secs_f64() * 1000.;
            run.prepared = true;
            // Building a scene can take seconds; time readiness, warm-up and
            // samples from after it, not from the start of this frame.
            now = Instant::now();
        }
        if run.prepared && run.ready.is_none() && self.world.pending_jobs() == 0 {
            run.ready = Some(now);
        }
        if run.ready.is_some() && run.sample_start.is_none() && !run.fixture_mobs.is_empty() {
            // Chunk generation adds creature herds as columns finish loading
            // and lighting settles (Java's `spawnMobsForChunkGeneration`, which
            // `doMobSpawning` doesn't gate). Through the warm-up, keep only the
            // specified population; a herd arriving while sampling counts as drift.
            self.mobs.entities.mobs.retain(|m| run.fixture_mobs.contains(&m.uid));
        }
        if run.ready.is_some() && run.sample_start.is_none() && run.scene == "materials" {
            // Blocks the support pass couldn't hold popped off as items; keep
            // them out of the scene.
            self.mobs.entities.items.clear();
        }
        if run.created.elapsed() > Duration::from_secs(180) && run.ready.is_none() {
            panic!("benchmark {}: scene did not finish loading within 180 seconds", run.scene);
        }
        if let Some(ready) = run.ready {
            if run.sample_start.is_none() && ready.elapsed().as_secs_f64() >= run.warmup {
                run.sample_start = Some(now);
                run.previous = Some(now);
                run.ticks_before = self.clock.ticks();
                run.discarded_before = self.clock.discarded();
                run.paused_before = self.clock.paused();
                run.skipped_before = self.renderer.skipped_frames;
                run.entities_start = self.mobs.entities.mobs.len();
                run.last_serial = self.renderer.frame_serial();
                // Warm-up frames' GPU times aren't sampled.
                self.renderer.take_gpu_times();
            }
            let t = ready.elapsed().as_secs_f64();
            if run.scene == "menus" {
                let screen = perf::menu_screen(&MENUS_SCREENS, t);
                if run.screen != Some(screen) {
                    run.screen = Some(screen);
                    self.show_benchmark_screen(screen);
                }
            }
            if let Some((offset, dir)) = route(&run.scene, t) {
                // Follow the generated surface at a fixed clearance, facing the direction of travel.
                let mut pos = run.origin + offset;
                pos.y = self.route_height(&mut run.heights, pos, dir);
                self.session.player.pos = pos;
                self.session.player.yaw = dir.z.atan2(dir.x) as f32;
                self.session.player.pitch = -0.2;
            } else {
                self.session.player.yaw = (t * 0.12) as f32;
            }
            if let Some(start) = run.sample_start {
                if let Some(previous) = run.previous.replace(now)
                    && now > previous
                {
                    run.frames.push(now.duration_since(previous).as_secs_f64() * 1000.);
                    run.phases.push(run.phase.0);
                    let serial = self.renderer.frame_serial();
                    let fresh = serial != run.last_serial;
                    run.serials.push(if fresh { serial } else { 0 });
                    run.acquire.push(if fresh { self.renderer.stats.acquire_ms as f32 } else { 0. });
                    run.last_serial = serial;
                }
                if start.elapsed().as_secs_f64() >= run.duration {
                    self.finish_benchmark(&mut run, start);
                }
            }
        }
        run.phase = (Default::default(), Instant::now());
        self.benchmark = Some(run);
    }

    /// The materials scene: one state of every v0.12 block, cycled over a
    /// 25 x 25 grid three blocks apart on a stone floor ahead of the camera,
    /// with text on every sign and item frames holding items on a wall.
    fn build_materials_scene(&mut self, at: IVec3) {
        for x in -37..=37 {
            for z in -8..=70 {
                self.world.set_block(at + IVec3::new(x, -1, z), Block::STONE);
                // Clear trees and hills over the grid so every block shows.
                for y in 0..32 {
                    let p = at + IVec3::new(x, y, z);
                    if self.world.get_block(p).is_some_and(|b| b != Block::AIR && !b.is_fluid()) {
                        self.world.set_block(p, Block::AIR);
                    }
                }
            }
        }
        let mut names = std::collections::HashSet::new();
        let blocks: Vec<Block> = (MATERIALS_STATES.0..MATERIALS_STATES.1)
            .map(Block)
            .filter(|b| b.kind() != RenderKind::Invisible && names.insert(b.name()))
            .collect();
        let lines = || ["VoxelCraft".to_string(), "v0.12".to_string(), "materials".to_string(), "scene".to_string()];
        for (i, block) in blocks.iter().cycle().take(25 * 25).enumerate() {
            let p = at + IVec3::new(i as i32 % 25 * 3 - 36, 0, i as i32 / 25 * 3 - 3);
            // Attached blocks pop off without support: add stone on one side
            // at a time until the state stays.
            self.world.set_block(p, *block);
            for side in [IVec3::Y, IVec3::Z, IVec3::NEG_Z, IVec3::X, IVec3::NEG_X] {
                if self.world.get_block(p) == Some(*block) {
                    break;
                }
                self.world.set_block(p + side, Block::STONE);
                self.world.set_block(p, *block);
            }
            for front in [true, false] {
                let _ = self.world.set_sign_text(p, front, lines());
            }
        }
        // A wall of item frames facing the camera, behind the grid's first row.
        let held = [Item::DIAMOND, Item::COPPER_INGOT, Item::GLOW_INK_SAC, Item::RAW_IRON, Item::COAL];
        for x in -12..=12 {
            for y in 0..4 {
                let wall = at + IVec3::new(x, y, -7);
                self.world.set_block(wall, Block::STONE);
                let item = if (x + y) % 2 == 0 { Item::ITEM_FRAME } else { Item::GLOW_ITEM_FRAME };
                let placed = self.mobs.entities.place_decor(&self.world, item, wall, IVec3::NEG_Z, 0., &[]);
                if let (Some(Some(_)), Some(frame)) = (placed, self.mobs.entities.decor.last_mut()) {
                    frame.item = Some(Stack::new(held[(x + y).rem_euclid(5) as usize], 1));
                }
            }
        }
    }

    fn finish_benchmark(&mut self, run: &mut Run, start: Instant) {
        self.renderer.wait_idle();
        let stats = voxelcraft::performance::FrameStats::from_milliseconds(&run.frames)
            .expect("benchmark produced no finite frame samples");
        let sample_seconds = start.elapsed().as_secs_f64();
        let ticks = self.clock.ticks() - run.ticks_before;
        let discarded = (self.clock.discarded() - run.discarded_before).as_secs_f64() * 1000.;
        let paused = (self.clock.paused() - run.paused_before).as_secs_f64() * 1000.;
        let unpresented = self.renderer.skipped_frames - run.skipped_before;
        let entities_end = self.mobs.entities.mobs.len();
        let (width, height) = self.renderer.size();
        let (requested_present, resolved_present) = self.renderer.present_modes();
        let uncapped = matches!(resolved_present, wgpu::PresentMode::Immediate | wgpu::PresentMode::Mailbox);
        let graphics = if self.settings.enhanced_graphics { "enhanced" } else { "classic" };
        let evidence = RunEvidence {
            world: true,
            simulation: perf::scene_measures_simulation(&run.scene),
            resolution: (width, height),
            render_distance_chunks: self.world.render_distance(),
            warmup_seconds: run.warmup,
            requested_seconds: run.duration,
            sample_seconds,
            enhanced_graphics: self.settings.enhanced_graphics,
            uncapped_presentation: uncapped,
            offscreen: run.offscreen,
            presentation_experiment: perf::presentation_experiment(
                self.renderer.options().frame_latency,
                run.window_mode,
            ),
            completed_ticks: ticks,
            paused_ms: paused,
            unpresented_frames: unpresented,
            population: run.expected_population.map(|expected| (expected, run.entities_start, entities_end)),
        };
        let configuration_problems = evidence.configuration_problems();
        let run_problems = evidence.run_problems();
        let memory = self.renderer.gpu_memory();
        let shown = presentation(&mut self.renderer, &run.frames, &run.acquire, &run.serials);
        let settings = &self.settings;
        let options = serde_json::json!({
            "render_distance_chunks": settings.render_distance,
            "fov_degrees": settings.fov,
            "view_bobbing": settings.view_bobbing,
            "particles": format!("{:?}", settings.particles),
            "enhanced_graphics": settings.enhanced_graphics,
            "vsync_setting": settings.vsync,
            "hud": self.show_hud,
            "debug_overlay": self.show_debug,
            "volume": settings.volume,
            "window_mode": run.window_mode,
            "frame_latency": self.renderer.options().frame_latency,
            "metal_maximum_drawable_count": metal_drawables(&self.renderer),
            "display": super::fullscreen::describe(&self.renderer.window, run.window_mode == "exclusive"),
            "day_time_start": if run.scene == "entities" { Some(ENTITY_SCENE_TIME) } else { None },
            "day_cycle": self.gamerules.bool("advance_time"),
            "natural_spawning": self.gamerules.bool("spawn_mobs"),
            "camera_route_clearance_blocks": route(&run.scene, 0.).map(|_| ROUTE_CLEARANCE),
        });
        let validity = serde_json::json!({
            "completed_simulation_ticks": ticks,
            "expected_simulation_ticks": sample_seconds * voxelcraft::performance::TICKS_PER_SECOND,
            "simulation_ticks_within_tolerance": voxelcraft::performance::ticks_within_tolerance(ticks, sample_seconds),
            "discarded_simulation_ms": discarded,
            "paused_simulation_ms": paused,
            "unpresented_frames": unpresented,
            "entities_start": run.entities_start,
            "entities_end": entities_end,
            "expected_population": run.expected_population,
            "frame_thresholds_passed": stats.passes_gate(run.offscreen) && discarded == 0.,
            "acceptance_eligible_configuration": configuration_problems.is_empty(),
            "run_valid": run_problems.is_empty(),
            "ineligible_reasons": configuration_problems.iter().chain(&run_problems).collect::<Vec<_>>(),
        });
        let mut report = serde_json::json!({
            "schema": 2,
            "scene": run.scene,
            "screens": if run.scene == "menus" { Some(MENUS_SCREENS.as_slice()) } else { None },
            "screen_seconds": if run.scene == "menus" { Some(perf::MENU_SCREEN_SECONDS) } else { None },
            "simulation_measured": perf::scene_measures_simulation(&run.scene),
            "seed": self.world.generator.seed,
            "resolution_physical": [width, height],
            "view_distance_blocks": self.world.render_distance() * 32,
            "simulation_distance_blocks": self.world.simulation_distance() * 16,
            "warmup_seconds": run.warmup,
            "requested_sample_seconds": run.duration,
            "sample_seconds": sample_seconds,
            "frame_stats": stats.json(),
            "frame_times_ms": run.frames,
            "frame_phases_ms": perf::phase_breakdown(&run.frames, &run.phases),
            "cold_load_ms": run.cold_load_ms,
            "loaded_chunks": self.world.loaded_chunks(),
            "gpu_memory_allocated_bytes": memory.device_allocated,
            "gpu_memory_reserved_bytes": memory.device_reserved,
            "chunk_arena_used_bytes": memory.chunk_arena.0,
            "chunk_arena_reserved_bytes": memory.chunk_arena.1,
            "presentation": if run.offscreen {
                "offscreen image, nothing presented; normal renderer"
            } else {
                "window surface; normal renderer"
            },
            "present_mode_requested": format!("{requested_present:?}"),
            "present_mode_resolved": format!("{resolved_present:?}"),
            "vsync": !uncapped,
            "graphics": graphics,
            "options": options,
            "fixture_overrides": {"camera_flight": true, "initial_health": 10000},
            "source_revision": option_env!("VOXELCRAFT_BUILD_REVISION").unwrap_or("unknown"),
        });
        if let (Some(report), serde_json::Value::Object(validity)) = (report.as_object_mut(), validity) {
            report.extend(validity);
            report.extend(shown);
        }
        if let Some(parent) = run.output.parent() {
            std::fs::create_dir_all(parent).expect("create benchmark output directory");
        }
        std::fs::write(&run.output, serde_json::to_string_pretty(&report).unwrap() + "\n")
            .expect("write benchmark results");
        println!(
            "benchmark {}: {} frames, {:.1} FPS average, {:.1} FPS 1% low, {:.3} ms p99; {} ticks, {:.0} ms paused, \
             {} unpresented; {}",
            run.scene,
            stats.frames,
            stats.average_fps,
            stats.low_one_percent_fps,
            stats.p99_ms,
            ticks,
            paused,
            unpresented,
            run.output.display()
        );
        run.finished = true;
    }
    /// Opens one of the menus scene's screens over the world, as the
    /// player would: the inventory, or the pause menu and the screens under it.
    fn show_benchmark_screen(&mut self, screen: &str) {
        if screen == "inventory" {
            self.menu.close();
            if !self.inventory_open {
                self.toggle_inventory();
            }
        } else {
            if self.inventory_open {
                self.toggle_inventory();
            }
            self.menu = super::menu::startup_menu(Some(screen));
        }
    }

    pub(super) fn benchmark_done(&self) -> bool {
        self.benchmark.as_ref().is_some_and(|r| r.finished)
    }
}

/// The title scene: frame intervals while the title screens cycle over the
/// turning sky, with no world open. It ignores input like the world scenes.
pub(super) struct TitleRun {
    output: PathBuf,
    warmup: f64,
    duration: f64,
    created: Instant,
    sample_start: Option<Instant>,
    previous: Option<Instant>,
    frames: Vec<f64>,
    /// Each sample's acquire wait and frame serial (0: no frame was drawn).
    acquire: Vec<f32>,
    serials: Vec<u64>,
    last_serial: u64,
    skipped_before: u64,
    screen: Option<&'static str>,
    offscreen: bool,
    window_mode: &'static str,
}

impl TitleRun {
    pub fn from_args(args: &Args) -> Option<Self> {
        (args.benchmark_scene.as_deref() == Some("title")).then(|| Self {
            output: args.benchmark_output.clone().expect("checked when parsing"),
            warmup: args.benchmark_warmup,
            duration: args.benchmark_duration,
            created: Instant::now(),
            sample_start: None,
            previous: None,
            frames: Vec::new(),
            acquire: Vec::new(),
            serials: Vec::new(),
            last_serial: 0,
            skipped_before: 0,
            screen: None,
            offscreen: args.benchmark_offscreen,
            window_mode: args.benchmark_window_mode,
        })
    }

    /// After a presented frame: records its interval once warm-up is over and
    /// returns the screen to show if it is time for the next one.
    pub fn frame(&mut self, renderer: &mut crate::render::Renderer) -> Option<&'static str> {
        let drawn = Drawn {
            skipped: renderer.skipped_frames,
            serial: renderer.frame_serial(),
            acquire_ms: renderer.stats.acquire_ms as f32,
        };
        let sampling = self.sample_start.is_some();
        let screen = self.frame_at(Instant::now(), drawn);
        if !sampling && self.sample_start.is_some() {
            // Warm-up frames' GPU times aren't sampled.
            renderer.take_gpu_times();
        }
        screen
    }

    fn frame_at(&mut self, now: Instant, drawn: Drawn) -> Option<&'static str> {
        let t = now.duration_since(self.created).as_secs_f64();
        if self.sample_start.is_none() {
            if t >= self.warmup {
                self.sample_start = Some(now);
                self.previous = Some(now);
                self.skipped_before = drawn.skipped;
                self.last_serial = drawn.serial;
            }
        } else if let Some(previous) = self.previous.replace(now)
            && now > previous
        {
            self.frames.push(now.duration_since(previous).as_secs_f64() * 1000.);
            let fresh = drawn.serial != self.last_serial;
            self.serials.push(if fresh { drawn.serial } else { 0 });
            self.acquire.push(if fresh { drawn.acquire_ms } else { 0. });
            self.last_serial = drawn.serial;
        }
        let screen = perf::menu_screen(&perf::TITLE_SCREENS, t);
        (self.screen.replace(screen) != Some(screen)).then_some(screen)
    }

    /// Whether the sample period is over.
    pub fn done(&self) -> bool {
        self.sample_start.is_some_and(|s| s.elapsed().as_secs_f64() >= self.duration)
    }

    /// Writes the report, as a world scene does, without the world's parts.
    pub fn finish(&self, renderer: &mut crate::render::Renderer, settings: &super::settings::Settings) {
        renderer.wait_idle();
        let shown = presentation(renderer, &self.frames, &self.acquire, &self.serials);
        let start = self.sample_start.expect("sampling started");
        let stats =
            perf::FrameStats::from_milliseconds(&self.frames).expect("benchmark produced no finite frame samples");
        let sample_seconds = start.elapsed().as_secs_f64();
        let unpresented = renderer.skipped_frames - self.skipped_before;
        let (width, height) = renderer.size();
        let (requested_present, resolved_present) = renderer.present_modes();
        let uncapped = matches!(resolved_present, wgpu::PresentMode::Immediate | wgpu::PresentMode::Mailbox);
        let evidence = RunEvidence {
            world: false,
            simulation: false,
            resolution: (width, height),
            render_distance_chunks: 0,
            warmup_seconds: self.warmup,
            requested_seconds: self.duration,
            sample_seconds,
            enhanced_graphics: settings.enhanced_graphics,
            uncapped_presentation: uncapped,
            offscreen: self.offscreen,
            presentation_experiment: perf::presentation_experiment(renderer.options().frame_latency, self.window_mode),
            completed_ticks: 0,
            paused_ms: 0.,
            unpresented_frames: unpresented,
            population: None,
        };
        let configuration_problems = evidence.configuration_problems();
        let run_problems = evidence.run_problems();
        let mut report = serde_json::json!({
            "schema": 2,
            "scene": "title",
            "screens": perf::TITLE_SCREENS,
            "screen_seconds": perf::MENU_SCREEN_SECONDS,
            "simulation_measured": false,
            "resolution_physical": [width, height],
            "view_distance_blocks": null,
            "simulation_distance_blocks": null,
            "warmup_seconds": self.warmup,
            "requested_sample_seconds": self.duration,
            "sample_seconds": sample_seconds,
            "frame_stats": stats.json(),
            "frame_times_ms": self.frames,
            "presentation": if self.offscreen {
                "offscreen image, nothing presented; title screen"
            } else {
                "window surface; title screen"
            },
            "present_mode_requested": format!("{requested_present:?}"),
            "present_mode_resolved": format!("{resolved_present:?}"),
            "vsync": !uncapped,
            "graphics": if settings.enhanced_graphics { "enhanced" } else { "classic" },
            "options": {
                "gui_scale": settings.gui_scale,
                "vsync_setting": settings.vsync,
                "window_mode": self.window_mode,
                "frame_latency": renderer.options().frame_latency,
                "metal_maximum_drawable_count": metal_drawables(renderer),
                "display": super::fullscreen::describe(&renderer.window, self.window_mode == "exclusive"),
            },
            "source_revision": option_env!("VOXELCRAFT_BUILD_REVISION").unwrap_or("unknown"),
            "completed_simulation_ticks": 0,
            "paused_simulation_ms": null,
            "unpresented_frames": unpresented,
            "frame_thresholds_passed": stats.passes_gate(self.offscreen),
            "acceptance_eligible_configuration": configuration_problems.is_empty(),
            "run_valid": run_problems.is_empty(),
            "ineligible_reasons": configuration_problems.iter().chain(&run_problems).collect::<Vec<_>>(),
        });
        if let Some(report) = report.as_object_mut() {
            report.extend(shown);
        }
        if let Some(parent) = self.output.parent() {
            std::fs::create_dir_all(parent).expect("create benchmark output directory");
        }
        std::fs::write(&self.output, serde_json::to_string_pretty(&report).unwrap() + "\n")
            .expect("write benchmark results");
        println!(
            "benchmark title: {} frames, {:.1} FPS average, {:.1} FPS 1% low, {:.3} ms p99; {} unpresented; {}",
            stats.frames,
            stats.average_fps,
            stats.low_one_percent_fps,
            stats.p99_ms,
            unpresented,
            self.output.display()
        );
    }
}

/// What a title frame left for its sample: the renderer's skipped-frame
/// count, the serial of the frame last begun and its acquire wait.
#[derive(Clone, Copy, Default)]
struct Drawn {
    skipped: u64,
    serial: u64,
    acquire_ms: f32,
}

/// The layer's drawable count on Metal for the run's frame latency.
fn metal_drawables(renderer: &crate::render::Renderer) -> Option<u32> {
    (renderer.backend() == wgpu::Backend::Metal).then(|| perf::metal_drawable_count(renderer.options().frame_latency))
}

/// Report fields on presentation and GPU time: the window mode the window is
/// in (the options give the one asked for), whether frames were timed, each
/// sample's acquire wait and GPU time beside `frame_times_ms`, and the
/// [`perf::gpu_breakdown`].
/// `serials` gives each sample's frame (0: none). Call after
/// [`crate::render::Renderer::wait_idle`] so frames in flight are read back.
fn presentation(
    renderer: &mut crate::render::Renderer,
    intervals: &[f64],
    acquire: &[f32],
    serials: &[u64],
) -> serde_json::Map<String, serde_json::Value> {
    let times: HashMap<u64, f32> = renderer.take_gpu_times().into_iter().collect();
    let losses = renderer.gpu_timer_losses();
    let gpu: Option<Vec<Option<f32>>> = losses.map(|_| serials.iter().map(|s| times.get(s).copied()).collect());
    let mut out = serde_json::Map::new();
    out.insert("window_mode_applied".into(), super::fullscreen::Mode::current(&renderer.window).name().into());
    out.insert("gpu_timestamps".into(), renderer.gpu_timestamps().into());
    out.insert(
        "gpu_timestamp_ms".into(),
        gpu.as_deref().and_then(|g| perf::gpu_breakdown(intervals, acquire, g)).into(),
    );
    out.insert("gpu_timer_ring_full_frames".into(), losses.map(|l| l.0).into());
    out.insert("gpu_timer_invalid_frames".into(), losses.map(|l| l.1).into());
    out.insert("frame_acquire_ms".into(), acquire.into());
    out.insert("frame_gpu_ms".into(), gpu.into());
    out
}

#[cfg(test)]
mod tests {
    use super::{Drawn, TitleRun, route};
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    #[test]
    fn the_title_scene_samples_only_after_its_warm_up() {
        let created = Instant::now();
        let mut run = TitleRun {
            output: PathBuf::new(),
            warmup: 1.0,
            duration: 60.0,
            created,
            sample_start: None,
            previous: None,
            frames: Vec::new(),
            acquire: Vec::new(),
            serials: Vec::new(),
            last_serial: 0,
            skipped_before: 0,
            screen: None,
            offscreen: false,
            window_mode: "windowed",
        };
        let at = |ms: u64| created + Duration::from_millis(ms);
        let drawn = |serial, acquire_ms| Drawn { skipped: 3, serial, acquire_ms };
        assert_eq!(run.frame_at(at(0), Drawn::default()), Some("title"));
        for ms in [100, 500, 900] {
            run.frame_at(at(ms), Drawn { serial: ms, ..Drawn::default() });
        }
        assert!(run.frames.is_empty() && run.sample_start.is_none(), "warm-up frames don't count");
        run.frame_at(at(1000), drawn(1000, 0.5));
        run.frame_at(at(1004), drawn(1001, 0.75));
        assert_eq!(run.screen, Some("title"));
        // A hidden window drew nothing: no new serial, so no GPU time or acquire.
        assert_eq!(run.frame_at(at(2500), drawn(1001, 0.75)), Some("select"));
        assert_eq!(run.frames, [4.0, 1496.0]);
        assert_eq!(run.serials, [1001, 0]);
        assert_eq!(run.acquire, [0.75, 0.0]);
        assert_eq!(run.skipped_before, 3);
    }

    #[test]
    fn routes_face_their_direction_of_travel() {
        for scene in ["streaming", "survival"] {
            for t in [0.0, 7.5, 40.0, 123.0] {
                let (a, dir) = route(scene, t).unwrap();
                let (b, _) = route(scene, t + 1e-4).unwrap();
                let travel = (b - a).normalize();
                assert!(travel.dot(dir) > 0.999, "{scene} at {t}: {travel} vs {dir}");
                assert!((dir.length() - 1.0).abs() < 1e-9 && dir.y == 0.0);
            }
        }
        assert!(route("entities", 1.0).is_none());
    }
}
