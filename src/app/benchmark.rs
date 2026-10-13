//! Repeatable live-game performance scenes. Runs the ordinary frame and 20 Hz simulation loop.
use super::Game;
use crate::{
    Args,
    entity::MobKind,
    inventory::Stack,
    item::Item,
    world::{block::Block, forms},
};
use glam::{DVec3, IVec3};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) struct Run {
    scene: String,
    output: PathBuf,
    warmup: f64,
    duration: f64,
    created: Instant,
    ready: Option<Instant>,
    sample_start: Option<Instant>,
    previous: Option<Instant>,
    origin: DVec3,
    frames: Vec<f64>,
    ticks_before: u64,
    discarded_before: Duration,
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
            ready: None,
            sample_start: None,
            previous: None,
            origin: DVec3::ZERO,
            frames: Vec::new(),
            ticks_before: 0,
            discarded_before: Duration::ZERO,
            cold_load_ms: 0.,
            prepared: false,
            finished: false,
        })
    }
}
impl Game {
    pub(super) fn benchmark_frame(&mut self) {
        let Some(mut run) = self.benchmark.take() else {
            return;
        };
        let now = Instant::now();
        if run.finished {
            self.benchmark = Some(run);
            return;
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
                        for i in 0..160 {
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
        if run.created.elapsed() > Duration::from_secs(180) && run.ready.is_none() {
            panic!("benchmark {}: scene did not finish loading within 180 seconds", run.scene);
        }
        if let Some(ready) = run.ready {
            if run.sample_start.is_none() && ready.elapsed().as_secs_f64() >= run.warmup {
                run.sample_start = Some(now);
                run.previous = Some(now);
                run.ticks_before = self.clock.ticks();
                run.discarded_before = self.clock.discarded();
            }
            let route = ready.elapsed().as_secs_f64();
            self.session.player.yaw = (route * 0.12) as f32;
            if run.scene == "streaming" {
                // 12 blocks/s crosses a Java chunk every 1.33 s; changes streaming centres normally.
                self.session.player.pos = run.origin + DVec3::new(route * 12., 0., (route * 0.08).sin() * 32.);
            } else if run.scene == "survival" {
                self.session.player.pos =
                    run.origin + DVec3::new((route * 0.1).sin() * 12., 0., (route * 0.1).cos() * 12.);
            }
            if let Some(start) = run.sample_start {
                if let Some(previous) = run.previous.replace(now)
                    && now > previous
                {
                    run.frames.push(now.duration_since(previous).as_secs_f64() * 1000.);
                }
                if start.elapsed().as_secs_f64() >= run.duration {
                    self.renderer.wait_idle();
                    let stats = voxelcraft::performance::FrameStats::from_milliseconds(&run.frames)
                        .expect("benchmark produced no finite frame samples");
                    let ticks = self.clock.ticks() - run.ticks_before;
                    let discarded = (self.clock.discarded() - run.discarded_before).as_secs_f64() * 1000.;
                    let (width, height) = self.renderer.size();
                    let report = serde_json::json!({"schema":1,"scene":run.scene,"seed":self.world.generator.seed,"resolution_physical":[width,height],
                        "view_distance_blocks":self.world.render_distance()*32,"simulation_distance_blocks":128,
                        "warmup_seconds":run.warmup,"sample_seconds":start.elapsed().as_secs_f64(),"frame_stats":stats.json(),"frame_times_ms":run.frames,
                        "cold_load_ms":run.cold_load_ms,"completed_simulation_ticks":ticks,"discarded_simulation_ms":discarded,
                        "loaded_chunks":self.world.loaded_chunks(),"entities_end":self.mobs.entities.mobs.len(),"gpu_memory_used_bytes":self.renderer.stats.gpu_used_bytes,
                        "gpu_memory_reserved_bytes":self.renderer.stats.gpu_bytes,"presentation":"uncapped window surface; normal renderer",
                        "fixture_overrides":{"camera_flight":true,"initial_health":10000},"vsync":false,"graphics":if self.settings.enhanced_graphics {"enhanced"} else {"classic"},
                        "gpu_timestamp_ms":null,"source_revision":option_env!("VOXELCRAFT_BUILD_REVISION").unwrap_or("unknown"),
                        "frame_thresholds_passed":stats.low_one_percent_fps>=240. && stats.p99_ms<=1000./240. && discarded==0.,
                        "acceptance_eligible_configuration":width==1920 && height==1080 && self.world.render_distance()==8 && run.duration>=60. && run.warmup>=10.});
                    if let Some(parent) = run.output.parent() {
                        std::fs::create_dir_all(parent).expect("create benchmark output directory");
                    }
                    std::fs::write(&run.output, serde_json::to_string_pretty(&report).unwrap() + "\n")
                        .expect("write benchmark results");
                    println!(
                        "benchmark {}: {} frames, {:.1} FPS average, {:.1} FPS 1% low, {:.3} ms p99; {}",
                        run.scene,
                        stats.frames,
                        stats.average_fps,
                        stats.low_one_percent_fps,
                        stats.p99_ms,
                        run.output.display()
                    );
                    run.finished = true;
                }
            }
        }
        self.benchmark = Some(run);
    }
    pub(super) fn benchmark_done(&self) -> bool {
        self.benchmark.as_ref().is_some_and(|r| r.finished)
    }
}
