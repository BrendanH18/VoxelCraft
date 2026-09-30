//! Window, input and the per-frame game loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::DVec3;
use rustc_hash::FxHashSet;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::player::{MoveInput, Player};
use crate::render::{FrameParams, Renderer};
use crate::world::block::Block;
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::Generator;
use crate::world::World;
use crate::Args;

const REACH: f64 = 6.0;
const ACTION_REPEAT: f64 = 0.22;
const AUTOSAVE_EVERY: Duration = Duration::from_secs(120);
const MOUSE_SENSITIVITY: f32 = 0.0022;
const SKY: [f32; 3] = [0.30, 0.55, 0.95];
const NIGHT_SKY: [f32; 3] = [0.008, 0.012, 0.035];
const SUNSET: [f32; 3] = [0.95, 0.42, 0.18];
const WATER_FOG: [f32; 3] = [0.05, 0.14, 0.35];
/// Real seconds per in-game day.
const DAY_LENGTH: f64 = 600.0;

const HOTBAR: [Block; 9] = [
    Block::GRASS,
    Block::DIRT,
    Block::STONE,
    Block::COBBLESTONE,
    Block::PLANKS,
    Block::LOG,
    Block::GLASS,
    Block::BRICKS,
    Block::GLOWSTONE,
];

struct Game {
    renderer: Renderer,
    world: World,
    player: Player,
    storage: Storage,
    keys: FxHashSet<KeyCode>,
    mouse_grabbed: bool,
    left_held: bool,
    right_held: bool,
    action_cooldown: f64,
    hotbar: [Block; 9],
    selected: usize,
    show_hud: bool,
    last_space: Instant,
    last_frame: Instant,
    /// Fraction of the day: 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
    day_time: f64,
    last_save: Instant,
    // Title-bar stats, refreshed twice a second.
    stats_since: Instant,
    frames: u32,
    frame_time_sum: f64,
    screenshot: Option<String>,
    screenshot_state: u32,
}

pub struct App {
    args: Args,
    game: Option<Game>,
}

impl App {
    pub fn new(args: Args) -> Self {
        Self { args, game: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("VoxelCraft")
            .with_inner_size(PhysicalSize::new(1600, 900));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let renderer = pollster::block_on(Renderer::new(window, !self.args.no_vsync));

        let storage = Storage::new(format!("saves/{}", self.args.world));
        let existing = if self.args.new_world || !storage.exists() {
            None
        } else {
            match storage.load_level() {
                Ok(level) => Some(level),
                Err(e) => {
                    log::error!("failed to load level, starting fresh: {e}");
                    None
                }
            }
        };
        let seed = existing.as_ref().map(|l| l.seed).or(self.args.seed).unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1)
        });
        let saved = if existing.is_some() {
            storage.load_chunks().unwrap_or_else(|e| {
                log::error!("failed to load chunks: {e}");
                Default::default()
            })
        } else {
            Default::default()
        };
        log::info!("world '{}' seed {seed}, {} modified chunks", self.args.world, saved.len());

        let generator = Arc::new(Generator::new(seed));
        let mut player = Player::new(generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        if let Some((pos, yaw, pitch)) = existing.and_then(|l| l.player) {
            player.pos = pos;
            player.yaw = yaw;
            player.pitch = pitch;
        }
        if let Some([x, y, z, yaw, pitch]) = self.args.pose {
            player.pos = DVec3::new(x, y, z);
            player.yaw = (yaw as f32).to_radians();
            player.pitch = (pitch as f32).to_radians();
            player.flying = true;
        }
        let world = World::new(generator, saved, self.args.render_distance);
        log::info!("{} worker threads", world.worker_threads());

        let now = Instant::now();
        let mut game = Game {
            renderer,
            world,
            player,
            storage,
            keys: FxHashSet::default(),
            mouse_grabbed: false,
            left_held: false,
            right_held: false,
            action_cooldown: 0.0,
            hotbar: HOTBAR,
            selected: 0,
            show_hud: true,
            last_space: now - Duration::from_secs(1),
            last_frame: now,
            day_time: 0.08,
            last_save: now,
            stats_since: now,
            frames: 0,
            frame_time_sum: 0.0,
            screenshot: self.args.screenshot.clone(),
            screenshot_state: 0,
        };
        if game.screenshot.is_none() {
            game.set_grab(true);
        }
        self.game = Some(game);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(game) = self.game.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => {
                game.save();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => game.renderer.resize(size.width, size.height),
            WindowEvent::Focused(false) => {
                game.set_grab(false);
                game.keys.clear();
                game.left_held = false;
                game.right_held = false;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                match event.state {
                    ElementState::Pressed => {
                        if !event.repeat {
                            if code == KeyCode::Escape && !game.mouse_grabbed {
                                game.save();
                                event_loop.exit();
                                return;
                            }
                            game.on_key(code);
                        }
                        game.keys.insert(code);
                    }
                    ElementState::Released => {
                        game.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                if pressed && !game.mouse_grabbed {
                    game.set_grab(true);
                    return;
                }
                match button {
                    MouseButton::Left => {
                        game.left_held = pressed;
                        if pressed {
                            game.break_block();
                            game.action_cooldown = ACTION_REPEAT;
                        }
                    }
                    MouseButton::Right => {
                        game.right_held = pressed;
                        if pressed {
                            game.place_block();
                            game.action_cooldown = ACTION_REPEAT;
                        }
                    }
                    MouseButton::Middle if pressed => game.pick_block(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 30.0) as f32,
                };
                if dy.abs() >= 0.5 {
                    let step = if dy > 0.0 { 8 } else { 1 };
                    game.selected = (game.selected + step) % 9;
                }
            }
            WindowEvent::RedrawRequested => {
                game.frame();
                if game.screenshot_done() {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let (Some(game), DeviceEvent::MouseMotion { delta }) = (self.game.as_mut(), event) {
            if game.mouse_grabbed {
                game.player.look(delta.0 as f32 * MOUSE_SENSITIVITY, delta.1 as f32 * MOUSE_SENSITIVITY);
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(game) = &self.game {
            game.renderer.window.request_redraw();
        }
    }
}

/// Skylight multiplier and sky colour for a time of day.
fn sky_state(t: f64) -> (f32, [f32; 3]) {
    let s = (t * std::f64::consts::TAU).sin() as f32;
    let daylight = (s * 1.8 + 0.35).clamp(0.12, 1.0);
    let k = (daylight - 0.12) / 0.88;
    let glow = (-(s * 5.0).powi(2)).exp() * 0.55;
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
    let base = lerp(NIGHT_SKY, SKY, k);
    (daylight, lerp(base, SUNSET, glow * k.max(0.3)))
}

impl Game {
    fn set_grab(&mut self, grab: bool) {
        let window = &self.renderer.window;
        if grab {
            let ok = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            window.set_cursor_visible(!ok);
            self.mouse_grabbed = ok;
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.mouse_grabbed = false;
        }
    }

    fn on_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Escape => self.set_grab(false),
            KeyCode::KeyF => {
                self.player.flying = !self.player.flying;
                self.player.vel = DVec3::ZERO;
            }
            KeyCode::Space => {
                // Double-tap space toggles flight, like Minecraft creative.
                let now = Instant::now();
                if now - self.last_space < Duration::from_millis(280) {
                    self.player.flying = !self.player.flying;
                    self.player.vel.y = 0.0;
                }
                self.last_space = now;
            }
            KeyCode::KeyV => {
                let v = !self.renderer.vsync();
                self.renderer.set_vsync(v);
            }
            KeyCode::F1 => self.show_hud = !self.show_hud,
            KeyCode::KeyT => self.day_time = (self.day_time + 1.0 / 12.0).fract(),
            KeyCode::F11 => {
                let w = &self.renderer.window;
                w.set_fullscreen(match w.fullscreen() {
                    Some(_) => None,
                    None => Some(Fullscreen::Borderless(None)),
                });
            }
            KeyCode::BracketLeft | KeyCode::Minus => {
                let rd = self.world.render_distance() - 1;
                self.world.set_render_distance(rd);
            }
            KeyCode::BracketRight | KeyCode::Equal => {
                let rd = self.world.render_distance() + 1;
                self.world.set_render_distance(rd);
            }
            _ => {
                let digits = [
                    KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5,
                    KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
                ];
                if let Some(i) = digits.iter().position(|&d| d == code) {
                    self.selected = i;
                }
            }
        }
    }

    fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.world.raycast(self.player.eye(), self.player.forward().as_dvec3(), REACH)
    }

    fn break_block(&mut self) {
        if let Some((pos, _)) = self.target() {
            if self.world.get_block(pos) != Some(Block::BEDROCK) {
                self.world.set_block(pos, Block::AIR);
            }
        }
    }

    fn place_block(&mut self) {
        let Some((pos, normal)) = self.target() else { return };
        let at = pos + normal;
        let block = self.hotbar[self.selected];
        let free = matches!(self.world.get_block(at), Some(Block::AIR | Block::WATER));
        if free && !(block.is_solid() && self.player.intersects_block(at)) {
            self.world.set_block(at, block);
        }
    }

    fn pick_block(&mut self) {
        if let Some(b) = self.target().and_then(|(pos, _)| self.world.get_block(pos)) {
            if let Some(i) = self.hotbar.iter().position(|&h| h == b) {
                self.selected = i;
            } else {
                self.hotbar[self.selected] = b;
            }
        }
    }

    /// Drives `--screenshot`: once streaming settles, capture a frame and
    /// report completion on the frame after.
    fn screenshot_done(&mut self) -> bool {
        let Some(path) = self.screenshot.clone() else { return false };
        match self.screenshot_state {
            0 if self.world.loaded_chunks() > 0 && self.world.pending_jobs() == 0 => {
                self.screenshot_state = 1;
                false
            }
            // Settled for one frame (uploads applied); capture on the next.
            1 => {
                self.renderer.request_capture(path);
                self.screenshot_state = 2;
                false
            }
            2 => !self.renderer.capture_pending(),
            _ => false,
        }
    }

    fn save(&mut self) {
        let level = LevelInfo {
            seed: self.world.generator.seed,
            player: Some((self.player.pos, self.player.yaw, self.player.pitch)),
        };
        let chunks = self.world.modified_chunks();
        match self.storage.save(&level, &chunks) {
            Ok(()) => log::info!("saved {} modified chunks to {}", chunks.len(), self.storage.dir().display()),
            Err(e) => log::error!("save failed: {e}"),
        }
        self.last_save = Instant::now();
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f64().min(0.1);
        self.last_frame = now;

        // --- Simulation ---------------------------------------------------
        let held = |k: KeyCode| self.keys.contains(&k);
        let axis = |pos: KeyCode, neg: KeyCode| held(pos) as i32 as f64 - held(neg) as i32 as f64;
        let input = if self.mouse_grabbed {
            MoveInput {
                forward: axis(KeyCode::KeyW, KeyCode::KeyS),
                right: axis(KeyCode::KeyD, KeyCode::KeyA),
                jump: held(KeyCode::Space),
                descend: held(KeyCode::ShiftLeft),
                sprint: held(KeyCode::ControlLeft) || held(KeyCode::KeyR),
            }
        } else {
            MoveInput::default()
        };
        self.player.update(dt, input, &self.world);

        self.action_cooldown -= dt;
        if self.action_cooldown <= 0.0 && (self.left_held || self.right_held) && self.mouse_grabbed {
            if self.left_held {
                self.break_block();
            } else {
                self.place_block();
            }
            self.action_cooldown = ACTION_REPEAT;
        }

        // --- World streaming ------------------------------------------------
        self.world.update(self.player.pos);
        for (pos, mesh) in self.world.mesh_uploads.drain(..) {
            self.renderer.upload_mesh(pos, mesh);
        }
        for pos in self.world.mesh_removals.drain(..) {
            self.renderer.remove_mesh(pos);
        }

        if now - self.last_save > AUTOSAVE_EVERY {
            self.save();
        }

        // --- Render ---------------------------------------------------------
        self.day_time = (self.day_time + dt / DAY_LENGTH).fract();
        let (daylight, sky) = sky_state(self.day_time);
        let underwater = self.player.head_in_water(&self.world);
        let view_dist = (self.world.render_distance() * 32) as f32;
        let (fog_color, fog_start, fog_end) = if underwater {
            (WATER_FOG.map(|c| c * daylight), 0.0, 28.0)
        } else {
            (sky, view_dist * 0.55, view_dist * 0.95)
        };
        let params = FrameParams {
            camera: self.player.eye(),
            forward: self.player.forward(),
            fov_y: 70f32.to_radians() * if input.sprint && input.forward > 0.0 { 1.08 } else { 1.0 },
            sky_color: fog_color.map(|c| c as f64),
            fog_color,
            fog_start,
            fog_end,
            daylight,
            highlight: self.target().map(|(p, _)| p),
            hotbar: self.hotbar.map(|b| b.info().tex[0]),
            selected_slot: self.selected,
            show_hud: self.show_hud,
        };
        self.renderer.render(&params);

        // --- Stats ----------------------------------------------------------
        self.frames += 1;
        self.frame_time_sum += (Instant::now() - now).as_secs_f64();
        let elapsed = (now - self.stats_since).as_secs_f64();
        if elapsed >= 0.5 {
            let s = self.renderer.stats;
            let p = self.player.pos;
            self.renderer.window.set_title(&format!(
                "VoxelCraft | {:.0} fps ({:.2} ms cpu) | xyz {:.1} {:.1} {:.1} | rd {} | chunks {} loaded, {} meshed, {} visible | {} draws, {:.2}M quads, {:.0} MB | {}{}{}",
                self.frames as f64 / elapsed,
                self.frame_time_sum / self.frames as f64 * 1000.0,
                p.x, p.y, p.z,
                self.world.render_distance(),
                self.world.loaded_chunks(),
                s.meshes,
                s.visible,
                s.draw_calls,
                s.quads as f64 / 1e6,
                s.gpu_bytes as f64 / 1e6,
                self.hotbar[self.selected].name(),
                if self.player.flying { " | flying" } else { "" },
                if self.renderer.vsync() { "" } else { " | no vsync" },
            ));
            self.frames = 0;
            self.frame_time_sum = 0.0;
            self.stats_since = now;
        }
    }
}
