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
    world::{block::Block, forms, terrain::SEA_LEVEL},
};
use glam::{DVec3, IVec3};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use voxelcraft::performance::{ENTITY_SCENE_MOBS, RunEvidence};

/// Camera height above the generator's surface along flight routes.
const ROUTE_CLEARANCE: f64 = 24.0;
/// Grid spacing for sampled generator surface heights.
const HEIGHT_GRID: i32 = 8;
/// Entity scene time of day: dawn, below the undead sunburn threshold, held fixed.
const ENTITY_SCENE_TIME: f64 = 0.0;

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
    cold_load_ms: f64,
    prepared: bool,
    finished: bool,
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
            ticks_before: 0,
            discarded_before: Duration::ZERO,
            paused_before: Duration::ZERO,
            skipped_before: 0,
            entities_start: 0,
            expected_population: None,
            fixture_mobs: Vec::new(),
            heights: HashMap::new(),
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
        let now = Instant::now();
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
            if run.scene != "survival" && run.scene != "streaming" {
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
            }
            let t = ready.elapsed().as_secs_f64();
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
                }
                if start.elapsed().as_secs_f64() >= run.duration {
                    self.finish_benchmark(&mut run, start);
                }
            }
        }
        self.benchmark = Some(run);
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
            resolution: (width, height),
            render_distance_chunks: self.world.render_distance(),
            warmup_seconds: run.warmup,
            requested_seconds: run.duration,
            sample_seconds,
            enhanced_graphics: self.settings.enhanced_graphics,
            uncapped_presentation: uncapped,
            completed_ticks: ticks,
            paused_ms: paused,
            unpresented_frames: unpresented,
            population: run.expected_population.map(|expected| (expected, run.entities_start, entities_end)),
        };
        let configuration_problems = evidence.configuration_problems();
        let run_problems = evidence.run_problems();
        let memory = self.renderer.gpu_memory();
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
            "fullscreen": "not applied to scripted runs",
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
            "frame_thresholds_passed": stats.low_one_percent_fps >= 240. && stats.p99_ms <= 1000. / 240. && discarded == 0.,
            "acceptance_eligible_configuration": configuration_problems.is_empty(),
            "run_valid": run_problems.is_empty(),
            "ineligible_reasons": configuration_problems.iter().chain(&run_problems).collect::<Vec<_>>(),
        });
        let mut report = serde_json::json!({
            "schema": 2,
            "scene": run.scene,
            "seed": self.world.generator.seed,
            "resolution_physical": [width, height],
            "view_distance_blocks": self.world.render_distance() * 32,
            "simulation_distance_blocks": self.world.simulation_distance() * 16,
            "warmup_seconds": run.warmup,
            "requested_sample_seconds": run.duration,
            "sample_seconds": sample_seconds,
            "frame_stats": stats.json(),
            "frame_times_ms": run.frames,
            "cold_load_ms": run.cold_load_ms,
            "loaded_chunks": self.world.loaded_chunks(),
            "gpu_memory_allocated_bytes": memory.device_allocated,
            "gpu_memory_reserved_bytes": memory.device_reserved,
            "chunk_arena_used_bytes": memory.chunk_arena.0,
            "chunk_arena_reserved_bytes": memory.chunk_arena.1,
            "gpu_timestamp_ms": null,
            "presentation": "window surface; normal renderer",
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
    pub(super) fn benchmark_done(&self) -> bool {
        self.benchmark.as_ref().is_some_and(|r| r.finished)
    }
}

#[cfg(test)]
mod tests {
    use super::route;

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
