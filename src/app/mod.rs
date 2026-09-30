//! Window, input and the per-frame game loop.

mod hud;
pub mod survival;

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};
use rustc_hash::FxHashSet;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::inventory::{Inventory, Stack, HOTBAR_SLOTS};
use crate::player::{MoveInput, Player};
use crate::render::{FrameParams, Renderer};
use crate::world::block::Block;
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::Generator;
use crate::world::World;
use crate::Args;

use survival::Vitals;

const REACH: f64 = 6.0;
const ACTION_REPEAT: f64 = 0.22;
/// Pause between breaking one block and starting the next in survival.
const BREAK_DELAY: f64 = 0.15;
const AUTOSAVE_EVERY: Duration = Duration::from_secs(120);
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// Horizon colour at noon (also the fog colour).
const SKY: [f32; 3] = [0.42, 0.62, 0.98];
const ZENITH: [f32; 3] = [0.10, 0.27, 0.80];
const NIGHT_SKY: [f32; 3] = [0.008, 0.012, 0.035];
const NIGHT_ZENITH: [f32; 3] = [0.002, 0.003, 0.012];
const SUNSET: [f32; 3] = [0.95, 0.42, 0.18];
const WATER_FOG: [f32; 3] = [0.05, 0.14, 0.35];
/// Real seconds per in-game day.
const DAY_LENGTH: f64 = 600.0;

/// Creative mode's starting hotbar.
const CREATIVE_HOTBAR: [Block; 9] = [
    Block::DIRT,
    Block::STONE,
    Block::COBBLESTONE,
    Block::PLANKS,
    Block::LOG,
    Block::BRICKS,
    Block::GLASS,
    Block::GLOWSTONE,
    Block::WATER,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameMode {
    Survival,
    Creative,
}

impl GameMode {
    fn name(self) -> &'static str {
        match self {
            GameMode::Survival => "survival",
            GameMode::Creative => "creative",
        }
    }
}

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
    mode: GameMode,
    inventory: Inventory,
    inventory_open: bool,
    /// Mouse position in physical pixels (for the inventory screen).
    cursor_px: (f32, f32),
    /// Block being broken in survival and progress 0..1.
    breaking: Option<(IVec3, f32)>,
    selected: usize,
    show_hud: bool,
    last_space: Instant,
    last_frame: Instant,
    /// Fraction of the day: 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
    day_time: f64,
    started: Instant,
    last_save: Instant,
    // Title-bar stats, refreshed twice a second.
    stats_since: Instant,
    frames: u32,
    frame_time_sum: f64,
    fps: f64,
    cpu_ms: f64,
    show_debug: bool,
    /// Short message above the hotbar (item names, mode changes).
    popup: (String, Instant),
    /// Health, air and fall tracking (survival).
    vitals: Vitals,
    screenshot: Option<String>,
    screenshot_state: u32,
    place: Vec<(glam::IVec3, Block)>,
    placed: bool,
    /// `--bench-render`: per-frame wall times (CPU + GPU, serialised).
    bench_render: Option<Vec<f64>>,
    frame_started: Option<Instant>,
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

        let mode = match (self.args.mode, existing.as_ref().and_then(|l| l.props.get("mode"))) {
            (Some(m), _) => m,
            (None, Some(m)) if m == "creative" => GameMode::Creative,
            _ => GameMode::Survival,
        };
        let inventory = existing
            .as_ref()
            .and_then(|l| l.props.get("inventory"))
            .and_then(|s| Inventory::deserialize(s))
            .unwrap_or_else(|| match mode {
                GameMode::Creative => Inventory::with_hotbar(&CREATIVE_HOTBAR),
                GameMode::Survival => Inventory::default(),
            });

        let prop = |k: &str| existing.as_ref().and_then(|l| l.props.get(k));
        let mut vitals = Vitals::restore(
            prop("health").and_then(|s| s.parse().ok()).unwrap_or(survival::MAX_HEALTH),
            prop("air").and_then(|s| s.parse().ok()).unwrap_or(survival::MAX_AIR),
            prop("death").cloned(),
        );
        if let Some(h) = self.args.health {
            vitals = Vitals::restore(h, vitals.air, None);
        }
        if let Some(a) = self.args.air {
            vitals.air = a.clamp(0.0, survival::MAX_AIR);
        }

        let generator = Arc::new(Generator::new(seed));
        let mut player = Player::new(generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        if let Some((pos, yaw, pitch)) = existing.as_ref().and_then(|l| l.player) {
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
        player.can_fly = mode == GameMode::Creative;
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
            mode,
            inventory,
            inventory_open: self.args.open_inventory,
            cursor_px: (0.0, 0.0),
            breaking: None,
            selected: 0,
            show_hud: true,
            last_space: now - Duration::from_secs(1),
            last_frame: now,
            day_time: self.args.time.unwrap_or(0.08),
            started: now,
            last_save: now,
            stats_since: now,
            frames: 0,
            frame_time_sum: 0.0,
            fps: 0.0,
            cpu_ms: 0.0,
            show_debug: self.args.debug_overlay,
            popup: (String::new(), now - Duration::from_secs(10)),
            vitals,
            screenshot: self.args.screenshot.clone(),
            screenshot_state: 0,
            place: self.args.place.clone(),
            placed: false,
            bench_render: self.args.bench_render.then(Vec::new),
            frame_started: None,
        };
        game.renderer.force_offscreen = game.bench_render.is_some();
        if game.vitals.is_dead() {
            game.on_death();
        } else if game.screenshot.is_none() && game.bench_render.is_none() {
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
            WindowEvent::CursorMoved { position, .. } => {
                game.cursor_px = (position.x as f32, position.y as f32);
            }
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
                            if code == KeyCode::Escape && !game.mouse_grabbed && !game.inventory_open {
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
                if game.vitals.is_dead() {
                    if pressed && button == MouseButton::Left {
                        game.respawn();
                    }
                    return;
                }
                if game.inventory_open {
                    if pressed {
                        game.inventory_click(button == MouseButton::Right);
                    }
                    return;
                }
                if pressed && !game.mouse_grabbed {
                    game.set_grab(true);
                    return;
                }
                match button {
                    MouseButton::Left => {
                        game.left_held = pressed;
                        if !pressed {
                            game.breaking = None;
                        } else if game.mode == GameMode::Creative {
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
                    game.select((game.selected + step) % 9);
                }
            }
            WindowEvent::RedrawRequested => {
                game.frame();
                if game.screenshot_done() || game.bench_render_done() {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let (Some(game), DeviceEvent::MouseMotion { delta }) = (self.game.as_mut(), event)
            && game.mouse_grabbed
            && !game.inventory_open
        {
                game.player.look(delta.0 as f32 * MOUSE_SENSITIVITY, delta.1 as f32 * MOUSE_SENSITIVITY);
            }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(game) = &self.game {
            game.renderer.window.request_redraw();
        }
    }
}

struct SkyState {
    daylight: f32,
    horizon: [f32; 3],
    zenith: [f32; 3],
    sun_dir: glam::Vec3,
}

/// Lighting and sky colours for a time of day.
fn sky_state(t: f64) -> SkyState {
    let angle = (t * std::f64::consts::TAU) as f32;
    let s = angle.sin();
    let daylight = (s * 1.8 + 0.35).clamp(0.12, 1.0);
    let k = (daylight - 0.12) / 0.88;
    let glow = (-(s * 5.0).powi(2)).exp() * 0.55;
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
    SkyState {
        daylight,
        horizon: lerp(lerp(NIGHT_SKY, SKY, k), SUNSET, glow * k.max(0.3)),
        zenith: lerp(NIGHT_ZENITH, ZENITH, k),
        // Rises in the east (+X), sets in the west, tilted slightly south.
        sun_dir: glam::Vec3::new(angle.cos(), s, 0.25).normalize(),
    }
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
        if self.vitals.is_dead() && matches!(code, KeyCode::KeyE | KeyCode::KeyG | KeyCode::KeyF | KeyCode::Space) {
            return;
        }
        match code {
            KeyCode::Escape if self.inventory_open => self.toggle_inventory(),
            KeyCode::Escape => self.set_grab(false),
            KeyCode::KeyE => self.toggle_inventory(),
            KeyCode::KeyG => {
                let mode = match self.mode {
                    GameMode::Survival => GameMode::Creative,
                    GameMode::Creative => GameMode::Survival,
                };
                self.set_mode(mode);
            }
            KeyCode::KeyF if self.player.can_fly => {
                self.player.flying = !self.player.flying;
                self.player.vel = DVec3::ZERO;
            }
            KeyCode::Space => {
                // Double-tap space toggles flight, like Minecraft creative.
                let now = Instant::now();
                if self.player.can_fly && now - self.last_space < Duration::from_millis(280) {
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
            KeyCode::F3 => self.show_debug = !self.show_debug,
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
                    self.select(i);
                }
            }
        }
    }

    fn select(&mut self, slot: usize) {
        if slot != self.selected {
            self.selected = slot;
            self.show_selected_name();
        }
    }

    fn show_selected_name(&mut self) {
        if let Some(s) = self.inventory.get(self.selected) {
            self.show_popup(s.block.name());
        }
    }

    fn show_popup(&mut self, text: &str) {
        self.popup = (text.to_string(), Instant::now());
    }

    /// The single entry point for hurting the player (falls, drowning,
    /// mobs, ...). `amount` is in half hearts; `cause` completes the death
    /// message "Player <cause>", e.g. "drowned" or "was slain by a zombie".
    /// Returns the damage actually taken: zero in creative, while dead, or
    /// when absorbed by the 0.5 s hurt immunity that follows each hit (a
    /// stronger hit within it only deals the difference).
    pub(crate) fn damage_player(&mut self, amount: f32, cause: &str) -> f32 {
        let taken = self.vitals.damage(amount, cause, self.mode == GameMode::Creative);
        if taken > 0.0 && self.vitals.is_dead() {
            self.on_death();
        }
        taken
    }

    /// Releases the mouse and stops all actions for the death screen.
    fn on_death(&mut self) {
        log::info!("player {}", self.vitals.death.as_deref().unwrap_or("died"));
        if self.inventory_open {
            self.inventory_open = false;
            self.inventory.return_cursor();
        }
        self.set_grab(false);
        self.keys.clear();
        self.left_held = false;
        self.right_held = false;
        self.breaking = None;
    }

    /// Back to the world spawn with full health; the inventory is kept.
    fn respawn(&mut self) {
        self.player.pos = self.world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        self.player.vel = DVec3::ZERO;
        self.player.flying = false;
        self.vitals.respawn();
        self.set_grab(true);
    }

    fn set_mode(&mut self, mode: GameMode) {
        self.mode = mode;
        self.player.can_fly = mode == GameMode::Creative;
        if !self.player.can_fly {
            self.player.flying = false;
        }
        self.breaking = None;
        self.show_popup(&format!("{} mode", capitalize(mode.name())));
    }

    fn toggle_inventory(&mut self) {
        self.inventory_open = !self.inventory_open;
        if self.inventory_open {
            self.set_grab(false);
            self.keys.clear();
            self.left_held = false;
            self.right_held = false;
            self.breaking = None;
        } else {
            self.inventory.return_cursor();
            self.set_grab(true);
        }
    }

    fn inventory_click(&mut self, right: bool) {
        match self.slot_under_cursor() {
            Some(hud::SlotRef::Inventory(i)) => self.inventory.click(i, right),
            Some(hud::SlotRef::Palette(block)) => {
                // Creative palette: take a full stack, or trash the held one.
                self.inventory.cursor = match self.inventory.cursor {
                    Some(_) => None,
                    None => Some(Stack::new(block, crate::inventory::MAX_STACK)),
                };
            }
            None => {}
        }
    }

    fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.world.raycast(self.player.eye(), self.player.forward().as_dvec3(), REACH)
    }

    /// Instant break (creative).
    fn break_block(&mut self) {
        if let Some((pos, _)) = self.target()
            && self.world.get_block(pos) != Some(Block::BEDROCK)
        {
            self.world.set_block(pos, Block::AIR);
        }
    }

    /// Timed break with drops (survival). Called every frame while the
    /// button is held.
    fn continue_breaking(&mut self, dt: f64) {
        let Some((pos, _)) = self.target() else {
            self.breaking = None;
            return;
        };
        let Some(block) = self.world.get_block(pos) else { return };
        let progress = match self.breaking {
            Some((p, progress)) if p == pos => progress,
            _ => 0.0,
        };
        let progress = progress + (dt / block.break_time() as f64) as f32;
        if progress < 1.0 {
            self.breaking = Some((pos, progress));
            return;
        }
        self.breaking = None;
        self.action_cooldown = BREAK_DELAY;
        self.world.set_block(pos, Block::AIR);
        if let Some(drop) = block.drop() {
            self.inventory.add(drop, 1);
        }
    }

    fn place_block(&mut self) {
        let Some((pos, normal)) = self.target() else { return };
        let at = pos + normal;
        let Some(stack) = self.inventory.get(self.selected) else { return };
        let block = stack.block;
        let free = self.world.get_block(at).is_some_and(|b| b.is_replaceable());
        if free && !(block.is_solid() && self.player.intersects_block(at)) && self.world.set_block(at, block)
            && self.mode == GameMode::Survival {
                self.inventory.take_one(self.selected);
            }
    }

    fn pick_block(&mut self) {
        let Some(b) = self.target().and_then(|(pos, _)| self.world.get_block(pos)) else { return };
        match self.inventory.find(b) {
            Some(i) if i < HOTBAR_SLOTS => self.select(i),
            Some(i) => {
                self.inventory.slots.swap(i, self.selected);
                self.show_selected_name();
            }
            None if self.mode == GameMode::Creative => {
                self.inventory.slots[self.selected] = Some(Stack::new(b, crate::inventory::MAX_STACK));
                self.show_selected_name();
            }
            None => {}
        }
    }

    /// Applies `--place` edits (once).
    fn apply_placements(&mut self) {
        for &(mut pos, block) in &self.place {
            if pos.y == i32::MIN {
                pos.y = self.world.generator.column(pos.x, pos.z).height + 1;
            }
            if !self.world.set_block(pos, block) {
                log::warn!("--place {pos} {}: chunk not loaded", block.name());
            }
        }
        self.placed = true;
    }

    /// Drives `--screenshot`: once streaming settles, capture a frame and
    /// report completion on the frame after.
    fn screenshot_done(&mut self) -> bool {
        let Some(path) = self.screenshot.clone() else { return false };
        let settled = self.world.loaded_chunks() > 0 && self.world.pending_jobs() == 0;
        match self.screenshot_state {
            0 if settled && !self.placed => {
                self.apply_placements();
                false
            }
            0 if settled && self.world.is_idle() => {
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

    /// Drives `--bench-render`: after streaming settles, spins the camera a
    /// full turn, waiting for the GPU every frame, then prints timings.
    fn bench_render_done(&mut self) -> bool {
        const FRAMES: usize = 360;
        let settled = self.world.loaded_chunks() > 0 && self.world.pending_jobs() == 0;
        let Some(times) = self.bench_render.as_mut() else { return false };
        if times.is_empty() && !settled {
            self.frame_started = None;
            return false;
        }
        self.renderer.wait_idle();
        if let Some(start) = self.frame_started.take() {
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        if times.len() >= FRAMES {
            let mut sorted = std::mem::take(times);
            self.bench_render = None;
            sorted.sort_by(f64::total_cmp);
            let avg = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let s = self.renderer.stats;
            println!(
                "render {}x{} rd={}: avg {:.2} ms ({:.0} fps), p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
                self.renderer.size().0,
                self.renderer.size().1,
                self.world.render_distance(),
                avg,
                1000.0 / avg,
                sorted[sorted.len() / 2],
                sorted[sorted.len() * 99 / 100],
                sorted[sorted.len() - 1],
            );
            println!(
                "last frame: {} meshes, {} visible, {} draw calls, {:.2}M quads, {:.0} MB vertex data",
                s.meshes,
                s.visible,
                s.draw_calls,
                s.quads as f64 / 1e6,
                s.gpu_bytes as f64 / 1e6
            );
            return true;
        }
        self.player.yaw += std::f32::consts::TAU / FRAMES as f32;
        self.frame_started = Some(Instant::now());
        false
    }

    fn save(&mut self) {
        let mut props = std::collections::BTreeMap::new();
        props.insert("mode".to_string(), self.mode.name().to_string());
        let mut inventory = self.inventory.clone();
        inventory.return_cursor();
        props.insert("inventory".to_string(), inventory.serialize());
        props.insert("health".to_string(), self.vitals.health.to_string());
        props.insert("air".to_string(), format!("{:.2}", self.vitals.air));
        if let Some(cause) = &self.vitals.death {
            props.insert("death".to_string(), cause.clone());
        }
        let level = LevelInfo {
            seed: self.world.generator.seed,
            player: Some((self.player.pos, self.player.yaw, self.player.pitch)),
            props,
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
        let input = if self.mouse_grabbed && !self.inventory_open && !self.vitals.is_dead() {
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
        let env = survival::Env {
            y: self.player.pos.y,
            on_ground: self.player.on_ground,
            flying: self.player.flying,
            in_water: self.player.in_water,
            head_in_water: self.player.head_in_water(&self.world),
        };
        let hurts = self.vitals.tick(dt as f32, &env, self.mode == GameMode::Creative);
        if hurts.fall > 0.0 {
            self.damage_player(hurts.fall, survival::CAUSE_FALL);
        }
        if hurts.drown > 0.0 {
            self.damage_player(hurts.drown, survival::CAUSE_DROWN);
        }

        self.action_cooldown -= dt;
        let acting = self.mouse_grabbed && !self.inventory_open;
        if acting && self.left_held && self.mode == GameMode::Survival {
            if self.action_cooldown <= 0.0 {
                self.continue_breaking(dt);
            }
        } else if acting && self.action_cooldown <= 0.0 && (self.left_held || self.right_held) {
            if self.left_held {
                self.break_block();
            } else {
                self.place_block();
            }
            self.action_cooldown = ACTION_REPEAT;
        }

        // --- World streaming ------------------------------------------------
        if !self.placed && self.screenshot.is_none() && self.world.pending_jobs() == 0 && self.world.loaded_chunks() > 0 {
            self.apply_placements();
        }
        self.world.tick_fluids(dt);
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
        let sky = sky_state(self.day_time);
        let daylight = sky.daylight;
        let underwater = env.head_in_water;
        let view_dist = (self.world.render_distance() * 32) as f32;
        let (fog_color, fog_start, fog_end) = if underwater {
            (WATER_FOG.map(|c| c * daylight), 0.0, 28.0)
        } else {
            (sky.horizon, view_dist * 0.55, view_dist * 0.95)
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
            zenith_color: if underwater { fog_color } else { sky.zenith },
            sun_dir: sky.sun_dir,
            time: (now - self.started).as_secs_f32(),
            highlight: self.target().map(|(p, _)| p),
            crack: self.breaking.map(|(p, progress)| {
                (p, crate::world::block::tex::CRACK_0 + (progress * 10.0).min(9.0) as u8)
            }),
            ui: if self.show_hud || self.vitals.is_dead() { self.build_ui(now) } else { Vec::new() },
        };
        if !self.renderer.render(&params) {
            return; // hidden window: don't count this frame in the stats
        }

        // --- Stats ----------------------------------------------------------
        self.frames += 1;
        self.frame_time_sum += (Instant::now() - now).as_secs_f64() - self.renderer.stats.acquire_ms / 1000.0;
        let elapsed = (now - self.stats_since).as_secs_f64();
        if elapsed >= 0.5 {
            self.fps = self.frames as f64 / elapsed;
            self.cpu_ms = self.frame_time_sum / self.frames as f64 * 1000.0;
            self.frames = 0;
            self.frame_time_sum = 0.0;
            self.stats_since = now;
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
