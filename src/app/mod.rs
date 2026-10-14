//! Window, input and the per-frame game loop.

mod actions;
mod agents;
mod anvil;
mod bed;
mod benchmark;
mod bow;
mod bucket;
mod console;
mod containers;
mod credits;
mod dimension;
mod doors;
mod enchanting;
mod farming;
mod fullscreen;
mod gamepad;
mod hand;
mod hands;
mod hud;
mod items;
mod lan;
mod menu;
mod mobs;
mod pad_menu;
mod particles;
mod player_actions;
mod recipe_book;
mod save;
mod screen;
mod search;
mod settings;
mod smithing;
mod split;
mod startup;
mod tick;
mod trading;
pub use crate::simulation::survival;
pub use voxelcraft::rules::GameMode;
mod title;
mod weather;

use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::{DVec3, IVec3};
use rustc_hash::FxHashSet;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Icon, Window, WindowId};

use crate::Args;
use crate::inventory::{Inventory, Stack};
use crate::item::Item;
use crate::player::{MoveInput, Player};
use crate::render::{FrameParams, Renderer};
use crate::simulation::difficulty::Difficulty;
use crate::world::World;
use crate::world::block::Block;
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::{Dimension, Generator};
use voxelcraft::rules::GameRules;

use survival::Vitals;

/// Placement/use repeat; mining timing is owned by the shared session.
const ACTION_REPEAT: f64 = 0.22;
const AUTOSAVE_EVERY: Duration = Duration::from_secs(120);
/// Seconds of holding right-click to eat (Minecraft's 32 ticks).
const EAT_TIME: f64 = 1.6;
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// Horizon colour at noon (also the fog colour).
const SKY: [f32; 3] = [0.42, 0.62, 0.98];
const ZENITH: [f32; 3] = [0.10, 0.27, 0.80];
const NIGHT_SKY: [f32; 3] = [0.008, 0.012, 0.035];
const NIGHT_ZENITH: [f32; 3] = [0.002, 0.003, 0.012];
const SUNSET: [f32; 3] = [0.95, 0.42, 0.18];
const WATER_FOG: [f32; 3] = [0.05, 0.14, 0.35];
const LAVA_FOG: [f32; 3] = [0.75, 0.25, 0.03];
/// Real seconds per in-game day.
const DAY_LENGTH: f64 = crate::simulation::DAY_LENGTH;

fn window_icon() -> Icon {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../../packaging/icons/VoxelCraft.png")));
    let mut reader = decoder.read_info().expect("embedded icon header");
    let mut rgba = vec![0; reader.output_buffer_size().expect("embedded icon size")];
    let frame = reader.next_frame(&mut rgba).expect("embedded icon pixels");
    rgba.truncate(frame.buffer_size());
    Icon::from_rgba(rgba, frame.width, frame.height).expect("embedded RGBA icon")
}

/// Creative mode's starting hotbar.
const CREATIVE_HOTBAR: [Item; 9] = [
    Item::from_block(Block::DIRT),
    Item::from_block(Block::STONE),
    Item::from_block(Block::COBBLESTONE),
    Item::from_block(Block::PLANKS),
    Item::from_block(Block::LOG),
    Item::from_block(Block::BRICKS),
    Item::from_block(Block::GLASS),
    Item::from_block(Block::GLOWSTONE),
    Item::from_block(Block::WATER),
];

/// What the inventory screen is open on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Container {
    /// The player's own inventory (2x2 crafting in survival).
    Inventory,
    CraftingTable,
    Furnace(IVec3),
    Chest(IVec3),
    Minecart(u32),
    Brewing(IVec3),
    Enchanting(IVec3),
    Anvil(IVec3),
    Grindstone(IVec3),
    Trading(u64),
    Smithing(IVec3),
}

struct Game {
    session: voxelcraft::session::PlayerSession,
    /// Exclusive claim on the open save (`session.lock`), released when the game closes.
    session_lock: Option<std::fs::File>,
    agents: agents::Agents,
    lan: lan::Lan,
    split: split::Split,
    pads: gamepad::Pads,
    /// `--pad-player` screen, seated once the world loads.
    virtual_pad: Option<String>,
    console: console::Console,
    search: search::Search,
    renderer: Renderer,
    world: World,
    camera: voxelcraft::camera::CameraMode,
    scripted_ride: Option<String>,
    storage: Storage,
    keys: FxHashSet<KeyCode>,
    /// Shift / Ctrl state (shift-click, Ctrl+Q, sneak-placing).
    modifiers: winit::keyboard::ModifiersState,
    mouse_grabbed: bool,
    /// Preserve taps that begin and end between fixed game ticks.
    jump_pressed: bool,
    /// Shared world difficulty (old saves default to Normal).
    difficulty: Difficulty,
    /// World-wide one-life flag; locks difficulty to Hard.
    hardcore: bool,
    gamerules: GameRules,
    inventory_open: bool,
    /// Crafting grid of the open screen: 2x2 in the inventory, 3x3 at a
    /// crafting table. Emptied back into the inventory when it closes.
    craft: crate::crafting::Grid,
    recipe_book: recipe_book::RecipeBook,
    /// First visible row of the creative palette.
    creative_scroll: usize,
    /// Mouse position in physical pixels (for the inventory screen).
    cursor_px: (f32, f32),
    /// The host player's hands and open screen.
    body: gamepad::Body,
    show_hud: bool,
    hand: hand::HandAnim,
    last_space: Instant,
    last_frame: Instant,
    clock: crate::simulation::FixedClock,
    previous_eye: DVec3,
    rendered_eye: DVec3,
    /// Biome-blended Nether fog colour (Java's cubic Gaussian sampler).
    nether_fog: crate::world::nether_biome::FogSampler,
    /// Fraction of the day: 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
    day_time: f64,
    /// Completed daylight cycles for `/time query day`.
    day_count: i64,
    /// Whether this player has already seen the end credits.
    credits_seen: bool,
    /// Seconds into the credits while they play.
    credits: Option<f32>,
    /// Shared Overworld spawn changed by `/setworldspawn`.
    world_spawn: glam::IVec3,
    weather: weather::Weather,
    weather_verts: Vec<crate::render::weather::WeatherVertex>,
    started: Instant,
    last_save: Instant,
    // HUD/F3 stats, refreshed twice a second using presented frames.
    stats_since: Instant,
    frames: u32,
    frame_time_sum: f64,
    fps: f64,
    cpu_ms: f64,
    show_debug: bool,
    /// Short message above the hotbar (item names, mode changes).
    popup: (String, Instant),
    screenshot: Option<String>,
    screenshot_state: u32,
    open_trading: bool,
    place: Vec<(glam::IVec3, Block)>,
    carts: Vec<(crate::entity::minecart::CartKind, glam::IVec3)>,
    /// `--open-block`: a container to open once placements are done.
    open_block: Option<IVec3>,
    /// `--drop`: thrown once the world has loaded.
    drop: Vec<(Item, u8)>,
    /// `--orbs` awards, spawned with the `--drop` items.
    orbs: Vec<u32>,
    placed: bool,
    /// `--bench-render`: per-frame wall times (CPU + GPU, serialised).
    bench_render: Option<Vec<f64>>,
    benchmark: Option<benchmark::Run>,
    frame_started: Option<Instant>,
    audio: crate::audio::Audio,
    mobs: mobs::Mobs,
    particles: crate::particles::System,
    /// Open menu screen, if any (the game is paused while one is up).
    menu: Option<menu::Screen>,
    /// Slider being dragged.
    menu_drag: Option<menu::Widget>,
    /// The window has had keyboard focus at some point (losing focus only
    /// pauses after that, not when the game starts in the background).
    had_focus: bool,
    settings: settings::Settings,
    /// Where options are saved; `None` for scripted runs (screenshots,
    /// benchmarks), which neither read nor write them.
    settings_path: Option<std::path::PathBuf>,
    /// Name shown in the world list (the save folder's name may differ).
    world_name: String,
    /// Terrain generator the world was made with: 1 before v0.6's
    /// 1.18-style Overworld, 2 since. Kept so old worlds stay marked.
    terrain_version: u8,
    dimension: Dimension,
    /// The overworld's furnaces, chests and items while the player is in
    /// the Nether (saved in the root level file).
    overworld_props: std::collections::BTreeMap<String, String>,
    /// Travelling: where the player goes once the ground there has loaded.
    arrival: Option<dimension::Arrival>,
    /// Seconds spent standing in a portal.
    portal_time: f32,
    /// Just came out of a portal: stepping out of it rearms it.
    portal_locked: bool,
}

pub struct App {
    args: Args,
    saves_dir: std::path::PathBuf,
    save_on_exit: bool,
    game: Option<Game>,
    /// The title screen, while no world is loaded.
    title: Option<title::Title>,
}

impl App {
    pub fn new(args: Args, saves_dir: std::path::PathBuf) -> Self {
        let save_on_exit = args.screenshot.is_none() && !args.bench_render && args.benchmark_scene.is_none();
        Self { args, saves_dir, save_on_exit, game: None, title: None }
    }

    /// Loads (or creates) a world and leaves the title screen. Debug
    /// options like `--give` only apply to the first world of a session.
    fn play(&mut self, shell: Shell, world_dir: &str, new: Option<title::NewWorld>) {
        // One process per save; scripted runs (screenshots, benchmarks) never autosave and may share.
        // Benchmarks never write; a screenshot run saves only if its window is explicitly closed.
        let scripted = self.args.screenshot.is_some() || self.args.bench_render || self.args.benchmark_scene.is_some();
        let lock = Storage::new(self.saves_dir.join(world_dir)).lock();
        if let Err(e) = &lock
            && !scripted
        {
            log::warn!("cannot open world '{world_dir}': {e}");
            let mut title = title::Title::new(shell, &self.saves_dir);
            title.world_error(format!("Can't open \"{world_dir}\": {e}"));
            self.title = Some(title);
            return;
        }
        let mut game = Game::start(shell, &self.args, &self.saves_dir, world_dir, new);
        game.session_lock = lock.ok();
        game.apply_settings();
        self.args.clear_one_shot();
        self.title = None;
        self.game = Some(game);
    }

    fn join_lan(&mut self, connection: voxelcraft::lan::Connection, profile: String) {
        let shell = self.title.take().expect("title screen").into_shell();
        self.args.join = Some("connected".into());
        let mut game = Game::start(shell, &self.args, &self.saves_dir, "lan-client", None);
        game.lan.client = Some(connection);
        game.lan.profile = profile;
        game.arrival = None;
        game.placed = true;
        game.world =
            World::new_remote(Arc::new(Generator::new(0)), game.settings.render_distance.clamp(2, lan::LAN_VIEW_MAX));
        game.lan.offline_render_distance = Some(game.settings.render_distance);
        // Start at the distance we will ask the host for; its first state may lower it.
        game.settings.render_distance = game.settings.render_distance.clamp(2, lan::LAN_VIEW_MAX);
        game.apply_settings();
        self.args.clear_one_shot();
        self.game = Some(game);
    }

    /// Saves the world and goes back to the title screen.
    fn quit_to_title(&mut self) {
        let Some(mut game) = self.game.take() else { return };
        game.save();
        game.save_settings();
        game.close_lan("Server closed");
        let shell = game.into_shell();
        self.title = Some(title::Title::new(shell, &self.saves_dir));
    }
}

/// What outlives a world: the window's renderer, sound and options. A game
/// takes it on start and hands it back to the title screen on quit.
pub(crate) struct Shell {
    renderer: Renderer,
    audio: crate::audio::Audio,
    settings: settings::Settings,
    /// Where options are saved; `None` for scripted runs (screenshots,
    /// benchmarks), which neither read nor write them.
    settings_path: Option<std::path::PathBuf>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() || self.title.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("VoxelCraft")
            .with_window_icon(Some(window_icon()))
            .with_inner_size({
                let (w, h) = self.args.physical_size.unwrap_or(if self.args.benchmark_scene.is_some() {
                    (1920, 1080)
                } else {
                    (1600, 900)
                });
                PhysicalSize::new(w, h)
            });
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        // Saved options, with command-line overrides for this session.
        let scripted = self.args.screenshot.is_some() || self.args.bench_render || self.args.benchmark_scene.is_some();
        let settings_path = (!scripted).then(|| self.saves_dir.join("options.txt"));
        let mut settings = settings_path.as_deref().map(settings::Settings::load).unwrap_or_default();
        if let Some(rd) = self.args.render_distance {
            settings.render_distance = rd;
        }
        if let Some(v) = self.args.volume {
            settings.volume = v;
        }
        settings.vsync &= !self.args.no_vsync && self.args.benchmark_scene.is_none();
        if let Some(enhanced) = self.args.enhanced_graphics {
            settings.enhanced_graphics = enhanced;
        }

        if !scripted {
            settings.fullscreen.apply(&window);
        }
        let renderer = pollster::block_on(Renderer::new(window, settings.vsync));
        let mut audio = crate::audio::Audio::new(self.args.mute, settings.volume);
        audio.set_music_volume(settings.music_volume);
        let shell = Shell { renderer, audio, settings, settings_path };
        // A named world, a fresh one or a scripted run skips the title screen.
        let to_title = matches!(self.args.open_menu.as_deref(), Some("title" | "create" | "multiplayer"));
        if let Some(address) = self.args.join.clone() {
            let mut title = title::Title::new(shell, &self.saves_dir);
            title.connect_address(address, self.args.profile.clone());
            self.title = Some(title);
            return;
        }
        match self.args.world.clone() {
            Some(world) if !to_title => self.play(shell, &world, None),
            None if !to_title && (self.args.new_world || scripted) => self.play(shell, "world", None),
            _ => {
                let mut title = title::Title::new(shell, &self.saves_dir);
                if self.args.open_menu.as_deref() == Some("create") {
                    title.show_create();
                }
                if self.args.open_menu.as_deref() == Some("multiplayer") {
                    title.show_multiplayer();
                }
                title.screenshot = self.args.screenshot.take();
                self.title = Some(title);
            }
        }
    }

    /// Route window events to the active title/game screen and handle exit requests.
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(title) = self.title.as_mut() {
            if matches!(event, WindowEvent::CloseRequested) {
                event_loop.exit();
            }
            match title.event(event) {
                Some(title::Action::Play(dir, new)) => {
                    let shell = self.title.take().expect("title screen").into_shell();
                    self.play(shell, &dir, new);
                }
                Some(title::Action::Join(connection, profile)) => self.join_lan(connection, profile),
                Some(title::Action::Quit) => event_loop.exit(),
                None => {}
            }
            return;
        }
        let Some(game) = self.game.as_mut() else { return };
        // A live benchmark ignores keyboard, pointer and focus changes so stray input
        // cannot pause it or open menus; closing the window abandons it without saving.
        if game.benchmark.is_some()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::ModifiersChanged(_)
                    | WindowEvent::Focused(_)
            )
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_on_exit = game.benchmark.is_none();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => game.renderer.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => {
                game.cursor_px = (position.x as f32, position.y as f32);
                game.menu_cursor_moved();
            }
            WindowEvent::Focused(true) => game.had_focus = true,
            WindowEvent::ModifiersChanged(m) => game.modifiers = m.state(),
            WindowEvent::Focused(false) => {
                // Like Minecraft: switching away pauses (not in scripted runs).
                if game.had_focus
                    && game.settings_path.is_some()
                    && game.menu.is_none()
                    && !game.session.vitals.is_dead()
                {
                    game.open_menu();
                } else {
                    game.set_grab(false);
                    game.keys.clear();
                    game.body.left_held = false;
                    game.body.right_held = false;
                    game.jump_pressed = false;
                    game.body.mine_pressed = false;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                match event.state {
                    ElementState::Pressed => {
                        if game.console_key(&event) || game.search_key(&event) {
                            return;
                        }
                        if !event.repeat {
                            game.on_key(code);
                        }
                        if game.menu.is_none() {
                            game.keys.insert(code);
                        }
                    }
                    ElementState::Released => {
                        game.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                if game.console.open {
                    return;
                }
                if game.credits.is_some() {
                    if pressed {
                        game.skip_credits();
                    }
                    return;
                }
                if game.menu.is_some() {
                    if button == MouseButton::Left && game.menu_click(pressed) == Some(menu::MenuAction::Quit) {
                        self.quit_to_title();
                    }
                    return;
                }
                if game.session.vitals.is_dead() {
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
                    MouseButton::Left => game.attack_button(pressed),
                    MouseButton::Right => game.use_button(pressed),
                    MouseButton::Middle if pressed => game.pick_block(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { .. } if game.menu.is_some() || game.console.open => {}
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 30.0) as f32,
                };
                if dy.abs() >= 0.5 && game.inventory_open {
                    if game.recipe_book.open && game.shows_recipes() {
                        game.recipe_book.step(if dy > 0.0 { -1 } else { 1 });
                    } else {
                        game.scroll_palette(if dy > 0.0 { -1 } else { 1 });
                    }
                } else if dy.abs() >= 0.5 {
                    let step = if dy > 0.0 { 8 } else { 1 };
                    game.select((game.body.actions.selected + step) % 9);
                }
            }
            WindowEvent::RedrawRequested => {
                game.frame();
                if let Some(reason) = game.lan.lost.take() {
                    self.quit_to_title();
                    if let Some(title) = self.title.as_mut() {
                        title.connection_error(reason);
                    }
                    return;
                }
                if game.screenshot_done() || game.bench_render_done() || game.benchmark_done() {
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
            && game.benchmark.is_none()
        {
            let k = MOUSE_SENSITIVITY * game.settings.sensitivity;
            game.session.player.look(delta.0 as f32 * k, delta.1 as f32 * k);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // macOS Command-Q emits LoopExiting without a CloseRequested event.
        // Scripted captures/benchmarks only save if the user explicitly quits.
        if self.save_on_exit
            && let Some(game) = &mut self.game
        {
            game.save();
            game.save_settings();
            game.close_lan("Server closed");
        }
        if let Some(title) = &self.title {
            title.save_settings();
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(game) = &self.game {
            game.renderer.window.request_redraw();
        }
        if let Some(title) = &self.title {
            title.request_redraw();
        }
    }
}

pub(super) struct SkyState {
    daylight: f32,
    horizon: [f32; 3],
    zenith: [f32; 3],
    sun_dir: glam::Vec3,
}

/// Lighting and sky colours for a time of day.
fn seat_rider(entities: &crate::entity::Entities, player: &mut Player, id: crate::entity::PlayerId) {
    entities.seat_player(player, id);
}

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
    /// Ends the game, keeping what the title screen needs.
    fn into_shell(self) -> Shell {
        let Game { mut renderer, mut audio, settings, settings_path, .. } = self;
        renderer.clear_world();
        audio.leave_world();
        Shell { renderer, audio, settings, settings_path }
    }

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

    /// Dispatch a key press through menu/death guards and retain gameplay taps for the next tick.
    fn on_key(&mut self, code: KeyCode) {
        if self.credits.is_some() {
            if matches!(code, KeyCode::Escape | KeyCode::Space | KeyCode::Enter) {
                self.skip_credits();
            }
            return;
        }
        if self.menu.is_some() {
            if code == KeyCode::Escape {
                self.menu_back();
            }
            return;
        }
        let blocked_when_dead =
            [KeyCode::KeyE, KeyCode::KeyG, KeyCode::KeyF, KeyCode::KeyQ, KeyCode::Space, KeyCode::Escape];
        if self.session.vitals.is_dead() && blocked_when_dead.contains(&code) {
            return;
        }
        match code {
            KeyCode::Escape if self.inventory_open => self.toggle_inventory(),
            KeyCode::Escape => self.open_menu(),
            KeyCode::KeyE => self.toggle_inventory(),
            KeyCode::KeyQ => self.drop_selected(self.modifiers.control_key()),
            // Hardcore deaths end in spectator for good; only `/gamemode` cheats out.
            KeyCode::KeyG if self.lan.client.is_some() => self.lan_command(if self.session.mode.is_creative() {
                "/gamemode survival"
            } else {
                "/gamemode creative"
            }),
            KeyCode::KeyG if !self.hardcore => {
                let mode = match self.session.mode {
                    GameMode::Survival => GameMode::Creative,
                    GameMode::Creative | GameMode::Adventure | GameMode::Spectator => GameMode::Survival,
                };
                self.set_mode(mode);
            }
            KeyCode::KeyF if self.session.player.can_fly => {
                self.session.player.flying = !self.session.player.flying;
                self.session.player.vel = DVec3::ZERO;
            }
            // Waiting in bed for other players: jumping gets up.
            KeyCode::Space if self.session.sleeping.is_some() => self.session.sleeping = None,
            KeyCode::Space => {
                self.jump_pressed = true;
                // Double-tap space toggles flight, like Minecraft creative.
                let now = Instant::now();
                if self.session.player.can_fly && now - self.last_space < Duration::from_millis(280) {
                    self.session.player.flying = !self.session.player.flying;
                    self.session.player.vel.y = 0.0;
                }
                self.last_space = now;
            }
            KeyCode::KeyV => {
                self.settings.vsync = !self.settings.vsync;
                self.apply_settings();
                self.show_popup(if self.settings.vsync { "VSync on" } else { "VSync off" });
            }
            KeyCode::KeyM => {
                let muted = self.audio.toggle_mute();
                self.show_popup(if muted { "Sound off" } else { "Sound on" });
            }
            KeyCode::F1 => self.show_hud = !self.show_hud,
            KeyCode::F3 => self.show_debug = !self.show_debug,
            KeyCode::F5 => self.camera.cycle(),
            KeyCode::F11 => {
                // Leaves whatever fullscreen the window is in, or enters the
                // chosen mode (borderless if the option is Off).
                let w = &self.renderer.window;
                self.settings.fullscreen = match fullscreen::Mode::current(w) {
                    fullscreen::Mode::Off if self.settings.fullscreen == fullscreen::Mode::Off => {
                        fullscreen::Mode::Borderless
                    }
                    fullscreen::Mode::Off => self.settings.fullscreen,
                    _ => fullscreen::Mode::Off,
                };
                self.settings.fullscreen.apply(w);
                self.save_settings();
            }
            KeyCode::BracketLeft | KeyCode::Minus | KeyCode::BracketRight | KeyCode::Equal => {
                let step = if matches!(code, KeyCode::BracketLeft | KeyCode::Minus) { -1 } else { 1 };
                self.settings.render_distance = (self.settings.render_distance + step)
                    .clamp(settings::RENDER_DISTANCE.0, settings::RENDER_DISTANCE.1);
                self.apply_settings();
            }
            _ => {
                let digits = [
                    KeyCode::Digit1,
                    KeyCode::Digit2,
                    KeyCode::Digit3,
                    KeyCode::Digit4,
                    KeyCode::Digit5,
                    KeyCode::Digit6,
                    KeyCode::Digit7,
                    KeyCode::Digit8,
                    KeyCode::Digit9,
                ];
                if let Some(i) = digits.iter().position(|&d| d == code) {
                    self.select(i);
                }
            }
        }
    }

    fn select(&mut self, slot: usize) {
        // Mining progress follows the held item (Java `sameDestroyTarget`), not the slot.
        if self.body.actions.select(slot) {
            self.session.cancel_consumption();
            self.show_selected_name();
        }
    }

    fn show_selected_name(&mut self) {
        if let Some(s) = self.session.inventory.get(self.body.actions.selected) {
            self.show_popup(s.display_name());
        }
    }

    fn show_popup(&mut self, text: &str) {
        self.popup = (text.to_string(), Instant::now());
    }

    /// Sneaking builds against containers and beds instead of using them.
    fn sneak_building(&self) -> bool {
        self.modifiers.shift_key()
    }

    /// The single entry point for hurting the player (falls, drowning,
    /// mobs, ...), through protection enchantments. `amount` is in half
    /// hearts; `cause` completes the death
    /// message "Player <cause>", e.g. "drowned" or "was slain by a zombie".
    /// Returns the damage actually taken: zero in creative, while dead, or
    /// when absorbed by the 0.5 s hurt immunity that follows each hit (a
    /// stronger hit within it only deals the difference).
    pub(crate) fn damage_player(&mut self, amount: f32, cause: &str) -> f32 {
        let amount = crate::enchant::protect(amount, &self.session.inventory.armor, cause);
        let taken = self.session.vitals.damage(amount, cause, self.session.mode.invulnerable());
        if taken > 0.0 {
            self.session.player.animation.hurt_direction = 0.0;
            self.session.sleeping = None;
            self.audio.play(crate::audio::sounds::Sound::Hurt, None, 0.9, (0.92, 1.05));
        }
        if taken > 0.0 && self.session.vitals.is_dead() {
            self.on_death();
        }
        taken
    }

    /// Hurts the player through their armor (mobs, arrows, blasts, lava),
    /// wearing it down when the hit lands.
    pub(crate) fn damage_player_armored(&mut self, amount: f32, cause: &str) -> f32 {
        let reduced = survival::armor_reduce(
            amount,
            self.session.inventory.armor_points(),
            self.session.inventory.armor_toughness(),
        );
        let taken = self.damage_player(reduced, cause);
        if taken > 0.0 && self.session.mode.is_survival() {
            for item in self.session.inventory.wear_armor(amount) {
                self.show_popup(&format!("{} broke", capitalize(item.name())));
                let sound = crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood);
                self.audio.play(sound, None, 0.8, (1.3, 1.5));
            }
        }
        taken
    }

    /// Releases the mouse and stops all actions for the death screen. A
    /// survival player drops everything they carried.
    fn on_death(&mut self) {
        self.session.vitals.effects.clear();
        if self.gamerules.bool("showDeathMessages") {
            log::info!("player {}", self.session.vitals.death.as_deref().unwrap_or("died"));
        }
        if self.inventory_open {
            self.toggle_inventory();
        }
        if self.session.mode.is_survival() && !self.gamerules.bool("keepInventory") {
            self.drop_everything();
        }
        if self.hardcore {
            self.session.vitals.respawn();
            self.set_mode(GameMode::Spectator);
            self.show_popup("Game over - Spectator mode");
        }
        self.set_grab(false);
        self.keys.clear();
        self.body.left_held = false;
        self.body.right_held = false;
        self.jump_pressed = false;
        self.body.mine_pressed = false;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
        if self.hardcore {
            self.set_grab(true);
        } else if self.gamerules.bool("doImmediateRespawn") {
            self.respawn();
        }
    }

    /// Back to the world spawn with full health.
    fn respawn(&mut self) {
        if self.lan.client.is_some() {
            self.lan_command("respawn");
            return;
        }
        let kept_xp = self.gamerules.bool("keepInventory").then_some(self.session.vitals.xp);
        if self.dimension != Dimension::Overworld {
            self.session.vitals.respawn();
            if let Some(xp) = kept_xp {
                self.session.vitals.xp = xp;
            }
            self.switch_dimension(Dimension::Overworld, dimension::Arrival::Respawn);
            self.set_grab(true);
            return;
        }
        self.session.player.pos = self.respawn_point();
        self.session.player.vel = DVec3::ZERO;
        self.session.player.flying = false;
        self.session.vitals.respawn();
        if let Some(xp) = kept_xp {
            self.session.vitals.xp = xp;
        }
        self.set_grab(true);
    }

    fn set_mode(&mut self, mode: GameMode) {
        self.session.set_mode(mode);
        self.body.actions.reset();
        self.session.cancel_consumption();
        self.show_popup(&format!("{} mode", capitalize(mode.name())));
    }

    /// Open or close the inventory, clearing queued input on entry and returning crafting stacks on exit.
    fn toggle_inventory(&mut self) {
        if self.lan.client.is_some() {
            self.lan_command(if self.inventory_open { "close" } else { "inventory" });
            return;
        }
        if !self.inventory_open && !self.session.mode.can_interact() {
            return;
        }
        if !self.inventory_open
            && self.body.container == Container::Inventory
            && let Some(id) = self.session.player.vehicle
            && self.mobs.entities.vehicle_slots(id).is_some_and(|slots| !slots.is_empty())
        {
            self.body.container = Container::Minecart(id);
        }
        self.inventory_open = !self.inventory_open;
        if self.inventory_open {
            self.search.focused =
                self.session.mode == GameMode::Creative && self.body.container == Container::Inventory;
            self.search.selected = false;
            self.set_grab(false);
            self.keys.clear();
            self.body.left_held = false;
            self.body.right_held = false;
            self.jump_pressed = false;
            self.body.mine_pressed = false;
            self.body.actions.reset();
            self.session.cancel_mining();
            self.session.cancel_consumption();
        } else {
            if let Container::Chest(pos) = self.body.container {
                self.chest_sound(pos, 0.8);
            }
            let table = self.take_work();
            self.session.inventory.return_stacks(self.craft.take_all().into_iter().chain(table));
            self.craft = crate::crafting::Grid::new(2);
            self.body.container = Container::Inventory;
            self.set_grab(true);
        }
    }

    /// Right-click on a crafting table: its 3x3 grid with the inventory.
    fn open_crafting_table(&mut self) {
        if self.inventory_open {
            return;
        }
        self.craft = crate::crafting::Grid::new(3);
        self.body.container = Container::CraftingTable;
        self.toggle_inventory();
    }

    /// Right-click on a furnace: its input, fuel and output with the inventory.
    fn open_furnace(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.furnace(pos).is_none() {
            return;
        }
        self.body.container = Container::Furnace(pos);
        self.toggle_inventory();
    }

    /// Right-click on a brewing stand: its bottles, ingredient and fuel.
    fn open_brewing(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.brewing_stand(pos).is_none() {
            return;
        }
        self.body.container = Container::Brewing(pos);
        self.toggle_inventory();
    }

    fn inventory_click(&mut self, right: bool) {
        if self.lan.client.is_some() {
            // Search, the recipe book and its paging are local views; slots and tossing go to the host.
            if self.search_click() {
                return;
            }
            if !right && let Some(control) = self.recipe_control_under_cursor() {
                match control {
                    recipe_book::Control::Toggle => self.recipe_book.open = !self.recipe_book.open,
                    recipe_book::Control::Previous => self.recipe_book.step(-1),
                    recipe_book::Control::Next => self.recipe_book.step(1),
                }
                self.audio.ui_click();
                return;
            }
            if self.session.inventory.cursor.is_some() && self.cursor_off_panel() {
                self.lan_command(if right { "toss_one" } else { "toss" });
                return;
            }
            if let Some(slot) = self.slot_under_cursor().and_then(lan::encode_slot) {
                self.audio.ui_click();
                self.lan_send(voxelcraft::lan::Packet::Click { slot, right, shift: self.modifiers.shift_key() });
            }
            return;
        }
        if self.search_click() {
            return;
        }
        if self.throw_cursor(right) {
            return;
        }
        if !right && let Some(control) = self.recipe_control_under_cursor() {
            match control {
                recipe_book::Control::Toggle => self.recipe_book.open = !self.recipe_book.open,
                recipe_book::Control::Previous => self.recipe_book.step(-1),
                recipe_book::Control::Next => self.recipe_book.step(1),
            }
            self.audio.ui_click();
            return;
        }
        let slot = self.slot_under_cursor();
        if slot.is_some() {
            self.audio.ui_click();
        }
        if self.modifiers.shift_key() && self.session.inventory.cursor.is_none() {
            if let Some(slot) = slot {
                self.quick_move(slot);
            }
            return;
        }
        if let Some(slot) = slot {
            self.click_slot(slot, right);
        }
    }

    /// Runs `f` on the host's open screen and resolves what it requests.
    fn with_screen<R>(&mut self, f: impl FnOnce(&mut screen::Screen) -> R) -> R {
        let mut s = screen::Screen {
            session: &mut self.session,
            container: &mut self.body.container,
            craft: &mut self.craft,
            world: &mut self.world,
            entities: &mut self.mobs.entities,
            audio: &mut self.audio,
            requests: Vec::new(),
        };
        let result = f(&mut s);
        for request in std::mem::take(&mut s.requests) {
            match request {
                screen::Request::Popup(text) => self.show_popup(&text),
            }
        }
        result
    }

    fn click_slot(&mut self, slot: hud::SlotRef, right: bool) {
        self.with_screen(|s| s.click_slot(slot, right));
    }

    fn quick_move(&mut self, slot: hud::SlotRef) {
        self.with_screen(|s| s.quick_move(slot));
    }

    /// Scrolls the creative palette by whole rows.
    fn scroll_palette(&mut self, rows: i32) {
        if self.session.mode.is_creative() {
            let max = hud::palette_rows(&self.search.query).saturating_sub(hud::PALETTE_ROWS);
            self.creative_scroll = self.creative_scroll.saturating_add_signed(rows as isize).min(max);
        }
    }

    fn ride_minecarts(&mut self, input: crate::player::MoveInput) {
        for m in &mut self.mobs.entities.mobs {
            if let Some(s) = &mut m.mount
                && let Some(id) = s.rider
            {
                let alive = if id == crate::entity::PlayerId::HOST {
                    !self.session.vitals.is_dead()
                } else {
                    self.agents
                        .players
                        .values()
                        .any(|b| b.active && b.agent.session.id == id && !b.agent.session.vitals.is_dead())
                };
                if !alive {
                    s.rider = None;
                }
            }
        }
        self.mobs.entities.mount_input(
            crate::entity::PlayerId::HOST,
            &self.session.player,
            input,
            &self.session.inventory,
            self.body.actions.selected,
        );
        let mut wish = Vec::new();
        if self.session.player.vehicle.is_none() && self.session.mode != GameMode::Spectator {
            self.mobs.entities.push_carts(self.session.player.pos, self.session.player.collision_shape().half_width);
        }
        for bot in self.agents.players.values().filter(|b| b.active && b.agent.session.player.vehicle.is_none()) {
            self.mobs
                .entities
                .push_carts(bot.agent.session.player.pos, bot.agent.session.player.collision_shape().half_width);
        }
        if self.session.player.vehicle.is_some() {
            wish.push((crate::entity::PlayerId::HOST, self.session.player.ride_push(input)));
        }
        for bot in self.agents.players.values().filter(|b| b.active && b.agent.session.player.vehicle.is_some()) {
            wish.push((bot.agent.session.id, bot.agent.session.player.ride_push(bot.agent.movement_input())));
        }
        self.mobs.entities.tick_minecarts(&mut self.world, &wish);
        seat_rider(&self.mobs.entities, &mut self.session.player, crate::entity::PlayerId::HOST);
        let ids: Vec<_> = self
            .agents
            .players
            .values()
            .filter(|b| b.active)
            .map(|b| (b.agent.session.id, b.agent.session.player.vehicle))
            .collect();
        for (id, vehicle) in ids {
            if vehicle.is_none() {
                continue;
            }
            if let Some(bot) = self.agents.players.values_mut().find(|b| b.agent.session.id == id) {
                seat_rider(&self.mobs.entities, &mut bot.agent.session.player, id);
            }
        }
    }

    fn play_notes(&mut self) {
        let notes = std::mem::take(&mut self.world.notes);
        for (pos, pitch, instrument) in notes {
            let sound = crate::audio::sounds::Sound::Note(crate::world::gadgets::Instrument::from_u8(instrument));
            let hz = crate::world::gadgets::pitch_hz(pitch);
            self.audio.play(sound, Some(pos.as_dvec3() + glam::DVec3::splat(0.5)), 1.0, (hz, hz));
        }
    }

    fn open_vehicle(&mut self, id: u32) {
        if !self.inventory_open {
            self.body.container = Container::Minecart(id);
            self.toggle_inventory();
        }
    }

    /// Opens what a use requested: a screen, bed, vehicle store or merchant.
    fn present_use(&mut self, used: voxelcraft::session::interact::Used) {
        use voxelcraft::session::interact::Used;
        match used {
            Used::OpenVehicle(id) => self.open_vehicle(id),
            Used::OpenMerchant(id) => self.open_merchant(id),
            Used::Screen { cell, block } => self.open_block_screen(cell, block),
            Used::Bed { cell } => self.use_bed(cell),
            _ => {}
        }
    }

    /// Opens the desktop screen for a used workstation or container block.
    fn open_block_screen(&mut self, cell: glam::IVec3, block: Block) {
        match block {
            Block::CRAFTING_TABLE => self.open_crafting_table(),
            b if crate::world::furnace::is_furnace(b) => self.open_furnace(cell),
            b if crate::world::chest::is_chest(b) => self.open_chest(cell),
            Block::BREWING_STAND => self.open_brewing(cell),
            Block::ENCHANTING_TABLE => self.open_enchanting(cell),
            b if b.is_anvil() => self.open_anvil(cell),
            b if b.base() == Block::GRINDSTONE => self.open_grindstone(cell),
            Block::SMITHING_TABLE => self.open_smithing(cell),
            _ => {}
        }
    }

    fn pick_block(&mut self) {
        if self.lan.client.is_some() {
            self.lan_command("pick");
            return;
        }
        use voxelcraft::session::interact::Pick;
        match self.session.pick_block(self.body.actions.selected, &self.world) {
            Some(Pick::Select(i)) => self.select(i),
            Some(Pick::Replaced) => {
                self.body.actions.reset();
                self.show_selected_name();
            }
            None => {}
        }
    }

    /// Applies `--place` edits (once).
    fn apply_placements(&mut self) {
        let mut failed_placements = 0;
        for &(mut pos, block) in &self.place {
            if pos.y == i32::MIN {
                pos.y = self.world.generator.column(pos.x, pos.z).height + 1;
            }
            if !self.world.set_block(pos, block) {
                failed_placements += 1;
                log::warn!("--place {pos} {}: chunk not loaded", block.name());
            }
        }
        self.world.tick_redstone();
        for (kind, pos) in std::mem::take(&mut self.carts) {
            if self.mobs.entities.place_cart(&self.world, kind, pos).is_none() {
                log::warn!("--cart {pos}: no rail");
            }
        }
        // Fanned out so each one can be seen.
        let drops = std::mem::take(&mut self.drop);
        for (i, &(item, count)) in drops.iter().enumerate() {
            let turn = (i as f64 - (drops.len() as f64 - 1.0) / 2.0) * 0.22;
            let dir = glam::DQuat::from_rotation_y(turn) * self.session.player.forward().as_dvec3();
            self.mobs.entities.throw(Stack::new(item, count), self.session.player.eye(), dir);
        }
        let ahead =
            self.session.player.pos + self.session.player.forward().as_dvec3().with_y(0.0).normalize_or_zero() * 4.0;
        for points in std::mem::take(&mut self.orbs) {
            self.mobs.entities.spawn_xp(ahead + DVec3::Y, points);
        }
        self.placed = true;
        if let Some(p) = self.open_block.take() {
            match self.world.get_block(p) {
                Some(b) if crate::world::furnace::is_furnace(b) => self.open_furnace(p),
                Some(b) if crate::world::chest::is_chest(b) => self.open_chest(p),
                Some(Block::BREWING_STAND) => self.open_brewing(p),
                Some(Block::ENCHANTING_TABLE) => self.open_enchanting(p),
                Some(b) if b.is_anvil() => self.open_anvil(p),
                Some(b) if b.base() == Block::GRINDSTONE => self.open_grindstone(p),
                Some(Block::SMITHING_TABLE) => self.open_smithing(p),
                _ => log::warn!("--open-block {p}: no container there"),
            }
        }
        self.spawn_pending_mobs();
        if let Some(name) = self.scripted_ride.take() {
            self.mobs.entities.ensure_mob_ids();
            let mut vehicle = None;
            if let Some(mob) = self.mobs.entities.mobs.iter_mut().find(|m| m.kind.name() == name && m.mount.is_some()) {
                let s = mob.mount.as_mut().unwrap();
                s.tame = true;
                s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
                s.rider = Some(crate::entity::PlayerId::HOST);
                vehicle = Some(crate::entity::mounts::MOB_VEHICLE | mob.uid);
            } else if let Some(cart) = self.mobs.entities.minecarts.iter_mut().find(|c| c.kind.item().name() == name) {
                cart.rider = Some(crate::entity::PlayerId::HOST);
                vehicle = Some(cart.id);
            }
            if let Some(id) = vehicle {
                self.session.player.vehicle = Some(id);
                seat_rider(&self.mobs.entities, &mut self.session.player, crate::entity::PlayerId::HOST);
                if self.inventory_open {
                    self.body.container = Container::Minecart(id);
                }
            } else {
                log::warn!("--ride: no scripted vehicle named {name}");
            }
        }
        // Seated once the ground it stands on is in place.
        if let Some(screen) = self.virtual_pad.take() {
            self.virtual_pad(&screen);
        }
        log::info!("scripted scene ready: {} placements, {failed_placements} failures", self.place.len());
    }

    /// Drives `--screenshot`: once streaming settles, capture a frame and
    /// report completion on the frame after.
    fn screenshot_done(&mut self) -> bool {
        let Some(path) = self.screenshot.clone() else { return false };
        let settled = self.world.loaded_chunks() > 0 && self.world.pending_jobs() == 0;
        if self.open_trading
            && self.placed
            && self.mobs.waited()
            && let Some(id) = self.mobs.entities.target_merchant(
                &self.world,
                self.session.player.eye(),
                self.session.player.forward().as_dvec3(),
                self.session.entity_reach(),
            )
            && self
                .mobs
                .entities
                .merchant(id)
                .and_then(|m| m.villager.as_ref())
                .is_some_and(|v| v.offers.iter().any(Option::is_some))
        {
            if !self.open_pad_trading(id) {
                self.open_merchant(id);
            }
            self.open_trading = false;
        }
        match self.screenshot_state {
            0 if settled && !self.placed => {
                self.apply_placements();
                false
            }
            // Paused captures cannot drain simulation work queued by
            // --place (fluid wakeups, falling blocks). Wait only for jobs.
            0 if settled
                && (self.menu.is_some() || self.console.open || self.world.is_idle())
                && self.mobs.waited() =>
            {
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
                "last frame: {} meshes, {} visible, {} draw calls, {:.2}M quads, {:.0} MB quad data ({:.0} MB reserved)",
                s.meshes,
                s.visible,
                s.draw_calls,
                s.quads as f64 / 1e6,
                s.gpu_used_bytes as f64 / 1e6,
                s.gpu_bytes as f64 / 1e6
            );
            return true;
        }
        self.session.player.yaw += std::f32::consts::TAU / FRAMES as f32;
        self.frame_started = Some(Instant::now());
        false
    }

    /// Sample held keys and a queued jump tap for a gameplay tick; return neutral input while control is blocked.
    fn movement_input(&self, arriving: bool) -> MoveInput {
        let held = |k: KeyCode| self.keys.contains(&k);
        let axis = |pos: KeyCode, neg: KeyCode| held(pos) as i32 as f64 - held(neg) as i32 as f64;
        if self.menu.is_none()
            && !self.console.open
            && self.mouse_grabbed
            && !self.inventory_open
            && !self.session.vitals.is_dead()
            && self.session.sleeping.is_none()
            && !arriving
        {
            MoveInput {
                forward: axis(KeyCode::KeyW, KeyCode::KeyS),
                right: axis(KeyCode::KeyD, KeyCode::KeyA),
                jump: held(KeyCode::Space) || self.jump_pressed,
                descend: held(KeyCode::ShiftLeft),
                // Too hungry to sprint at 6 food or less (survival).
                sprint: (held(KeyCode::ControlLeft) || held(KeyCode::KeyR))
                    && (self.session.mode.invulnerable() || self.session.vitals.hunger.can_sprint()),
            }
        } else {
            MoveInput::default()
        }
    }

    /// Attack/mine button (left click, or a controller's RT) pressed or released.
    fn attack_button(&mut self, pressed: bool) {
        if self.lan.client.is_some() {
            self.body.left_held = pressed;
            self.lan_button_edge(true, pressed);
            if pressed {
                self.hand.swing();
            }
            return;
        }
        let sprint = self.sprinting_for_attack();
        let hit_player = pressed
            && self.session.mode.can_interact()
            && self.attack_lan_player(&lan::PlayerRef::Host, self.body.actions.selected, sprint);
        self.with_hands(|hands| hands.attack_button(pressed, hit_player));
    }

    /// Use button (right click, or a controller's LT) pressed or released.
    fn use_button(&mut self, pressed: bool) {
        if self.lan.client.is_some() {
            self.body.right_held = pressed;
            self.lan_button_edge(false, pressed);
            if pressed {
                self.hand.swing();
            }
            return;
        }
        self.with_hands(|hands| hands.use_button(pressed));
    }

    /// Held attack/use buttons for one tick: mining, repeated breaking or
    /// placing, eating and drawing a bow.
    fn act(&mut self, acting: bool, dt: f64) {
        // Agents (controller/LAN hands) charge in their own tick; this is the host's.
        self.session.tick_attack_strength(self.body.actions.selected);
        let aiming = self.attacking();
        self.with_hands(|hands| hands.act(acting, dt, aiming));
    }

    /// Runs `f` on the acting player's hands and resolves what it requests.
    fn with_hands<R>(&mut self, f: impl FnOnce(&mut hands::Actor) -> R) -> R {
        let sneaking = self.sneak_building();
        let sprinting = self.sprinting_for_attack();
        let others = self.agents.positions();
        let mut actor = hands::Actor {
            session: &mut self.session,
            body: &mut self.body,
            hand: &mut self.hand,
            world: &mut self.world,
            entities: &mut self.mobs.entities,
            audio: &mut self.audio,
            gamerules: &self.gamerules,
            sneaking,
            sprinting,
            others: &others,
            requests: Vec::new(),
        };
        let result = f(&mut actor);
        for request in std::mem::take(&mut actor.requests) {
            match request {
                hands::Request::Popup(text) => self.show_popup(&text),
                hands::Request::Open(used) => self.present_use(used),
                hands::Request::MagicDamage(amount) => {
                    self.damage_player(amount, survival::CAUSE_MAGIC);
                }
            }
        }
        result
    }

    /// Poll streaming, run due fixed gameplay ticks and render interpolated positions.
    /// Offline pause keeps streaming active and snaps interpolation to the current state.
    fn frame(&mut self) {
        self.benchmark_frame();
        let now = Instant::now();
        self.poll_lan();
        self.poll_agents();
        let paused = (self.menu.is_some() || self.console.open || self.credits.is_some())
            && self.agents.host.is_none()
            && self.lan.host.is_none()
            && self.lan.client.is_none();
        let elapsed = now - self.last_frame;
        self.last_frame = now;
        let ticks = self.clock.advance(elapsed, paused);
        let dt = if paused { 0.0 } else { elapsed.as_secs_f64().min(0.25) };
        self.update_credits(elapsed.as_secs_f32().min(0.25));
        // Benchmark scenes take no controller input (a stray Start would pause sampling).
        if self.lan.client.is_none() && self.benchmark.is_none() {
            self.poll_pads(dt as f32, paused);
        }

        // Streaming and GPU uploads continue during offline pause.
        if self.lan.client.is_none() {
            self.update_arrival();
        }
        let viewers: Vec<DVec3> = self.followed().map(|(_, b)| b.agent.session.player.pos).collect();
        self.world.set_viewers(&viewers);
        self.world.update_players(self.session.player.pos, &self.agents.positions());
        if self.lan.client.is_none()
            && !self.placed
            && self.screenshot.is_none()
            && self.world.pending_jobs() == 0
            && self.world.loaded_chunks() > 0
        {
            self.apply_placements();
        }
        for _ in 0..ticks {
            if self.lan.client.is_some() {
                self.tick_lan_client();
            } else {
                let started = Instant::now();
                self.tick();
                let took = started.elapsed().as_secs_f64();
                self.lan.stats_tick_secs += took;
                self.lan.stats_tick_max = self.lan.stats_tick_max.max(took);
                self.send_lan_state();
            }
        }
        if paused {
            self.previous_eye = self.session.player.eye();
            self.mobs.entities.snapshot_positions();
            self.world.snapshot_falling_positions();
        }
        let input = self.movement_input(self.arrival.is_some());
        let rain_here = weather::rain_at(&self.world, &self.weather, self.session.player.pos);
        // `followed()`, spelled out so `audio` can be borrowed mutably.
        let others: Vec<_> = (self.split.follow.iter())
            .filter_map(|name| self.agents.players.get(name))
            .filter(|b| b.active)
            .take(split::MAX_VIEWS - 1)
            .map(|b| {
                (&b.agent.session.player, weather::rain_at(&self.world, &self.weather, b.agent.session.player.pos))
            })
            .collect();
        let dragon_music =
            self.mobs.entities.fight.as_ref().is_some_and(|f| f.boss_bar(self.session.player.pos).is_some());
        self.audio.update_music(
            &self.session.player,
            &self.world,
            self.session.mode == GameMode::Creative,
            dragon_music,
            self.credits.is_some(),
        );
        self.audio.update(&self.session.player, &self.world, rain_here, &others, dt);
        self.agent_sounds();
        let alpha = if paused { 1.0 } else { self.clock.alpha() };
        let eye = crate::simulation::interpolated_eye(self.previous_eye, self.session.player.eye(), alpha);
        self.rendered_eye = eye;
        let (camera, forward) = self.camera.view(&self.world, eye, self.session.player.forward());
        self.hand.update(dt as f32, self.held_item());
        self.animate_hands(dt as f32, alpha, paused);
        for (pos, mesh) in self.world.mesh_uploads.drain(..) {
            self.renderer.upload_mesh(pos, mesh);
        }
        for pos in self.world.mesh_removals.drain(..) {
            self.renderer.remove_mesh(pos);
        }
        // Scripted runs (screenshots, benchmarks) have no options path and never autosave.
        if self.settings_path.is_some() && now - self.last_save > AUTOSAVE_EVERY {
            self.save();
        }

        // --- Render ---------------------------------------------------------
        let mut sky = sky_state(self.day_time);
        sky.daylight = self.weather.dim(sky.daylight);
        sky.horizon = self.weather.overcast(sky.horizon);
        sky.zenith = self.weather.overcast(sky.zenith);
        let nether = !self.dimension.has_sky();
        if nether {
            // No sun, no weather: a steady dim glow in a red haze.
            sky.daylight = if self.dimension == Dimension::End { 0.65 } else { dimension::NETHER_LIGHT };
            let fog = match self.world.generator.dimension {
                Dimension::Nether => {
                    let generator = &self.world.generator;
                    self.nether_fog.sample(camera, |qx, qz| {
                        generator
                            .nether_biome(qx * 4, qz * 4)
                            .unwrap_or(crate::world::nether_biome::NetherBiome::NetherWastes)
                    })
                }
                _ => dimension::NETHER_FOG,
            };
            sky.horizon = if self.dimension == Dimension::End { [0.045, 0.025, 0.065] } else { fog };
            sky.zenith = if self.dimension == Dimension::End { [0.018, 0.009, 0.03] } else { fog };
        }
        let scene = split::Scene {
            sky,
            rain: if nether { 0.0 } else { self.weather.strength },
            time: (now - self.started).as_secs_f32(),
            alpha,
            now,
        };
        let split::Fog { color: fog_color, start: fog_start, end: fog_end, underwater } = self.fog(&scene, camera);
        let dial = self.dial_of(&self.session.player);
        let others: Vec<(&Player, DVec3, crate::entity::model::PlayerAppearance, &str)> = self
            .agents
            .players
            .iter()
            .filter(|(_, b)| {
                b.active && (!b.agent.session.vitals.is_dead() || b.agent.session.vitals.since_damage() < 1.0)
            })
            .map(|(name, b)| {
                let pose =
                    dial.at(b.agent.session.player.yaw, b.agent.session.player.pos.x, b.agent.session.player.pos.z);
                (
                    &b.agent.session.player,
                    b.agent.previous_pos.lerp(b.agent.session.player.pos, alpha),
                    Self::stamp_look(
                        pose,
                        b.hand.appearance(
                            &b.agent.session.vitals,
                            b.agent.eating(),
                            alpha,
                            b.agent.session.inventory.armor,
                        ),
                    ),
                    name.as_str(),
                )
            })
            .collect();
        let verts = self.mobs.entities.mesh(camera, forward, fog_end, scene.time, alpha);
        push_avatars(&self.world, &others, camera, fog_end, scene.time, verts);
        if !self.camera.first_person() {
            crate::entity::model::build_player(
                &self.session.player,
                eye - (self.session.player.eye() - self.session.player.pos),
                camera,
                (
                    crate::entity::sky_light(&self.world, eye),
                    self.world.block_light(eye.floor().as_ivec3()) as f32 / 15.0,
                ),
                scene.time,
                Self::stamp_look(
                    dial,
                    self.hand.appearance(
                        &self.session.vitals,
                        (self.body.actions.eat_timer / EAT_TIME) as f32,
                        alpha,
                        self.session.inventory.armor,
                    ),
                ),
                verts,
            );
        }
        self.renderer.set_entities(verts);
        self.renderer.set_particles(&self.particles.pool, &self.world, camera, self.session.player.forward(), alpha);
        weather::sheets(&self.world, camera, scene.rain, &mut self.weather_verts);
        self.renderer.set_weather(&self.weather_verts);
        let viewports = self.viewports();
        let params = FrameParams {
            camera,
            forward,
            view_effect: voxelcraft::camera::view_effect(
                &self.session.player,
                &self.session.vitals,
                alpha as f32,
                self.settings.view_bobbing,
            ),
            fov_y: self.settings.fov.to_radians()
                * if input.sprint && input.forward > 0.0 { 1.08 } else { 1.0 }
                * (1.0 - 0.15 * self.bow_power().unwrap_or(0.0)),
            sky_color: fog_color.map(|c| c as f64),
            fog_color,
            fog_start,
            fog_end,
            daylight: scene.sky.daylight,
            zenith_color: if underwater { fog_color } else { scene.sky.zenith },
            sun_dir: scene.sky.sun_dir,
            dimension: self.dimension,
            enhanced_graphics: self.settings.enhanced_graphics,
            time: scene.time,
            highlight: self.target().filter(|_| self.mob_target().is_none()).map(|(p, _)| {
                let (min, max) = self.world.outline(p);
                (p, min, max)
            }),
            cracks: self.cracks_seen_from(eye),
            block_models: self.block_models(alpha),
            hand: (self.camera.first_person()
                && self.show_hud
                && !self.session.vitals.is_dead()
                && self.session.sleeping.is_none())
            .then(|| {
                let eye = self.session.player.eye();
                let eating = (self.body.actions.eat_timer / EAT_TIME) as f32;
                Self::stamp_hand(
                    dial,
                    self.hand.view(eating, crate::entity::sky_light(&self.world, eye), self.torch_light(eye)),
                )
            }),
            rain: scene.rain,
            night_vision: self.session.vitals.effects.night_vision(scene.time),
            ui: if self.show_hud
                || self.session.vitals.is_dead()
                || self.menu.is_some()
                || self.console.open
                || self.credits.is_some()
            {
                self.build_ui(now)
            } else {
                Vec::new()
            },
        };
        let Some(mut frame) = self.renderer.begin_frame() else {
            return; // hidden window: don't count this frame in the stats
        };
        self.renderer.draw_view(&mut frame, &params, viewports[0]);
        self.draw_followers(&mut frame, &scene, &viewports);
        self.renderer.end_frame(frame);

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

impl Game {
    /// Falling blocks, primed TNT and dropped items, at interpolated positions.
    fn block_models(&self, alpha: f64) -> Vec<crate::render::BlockModel> {
        self.world
            .falling_blocks()
            .iter()
            .map(|f| crate::render::BlockModel {
                min: f.previous_pos.lerp(f.pos, alpha),
                size: 1.0,
                block: f.block,
                sky_light: crate::entity::sky_light(&self.world, f.pos + glam::DVec3::splat(0.5)),
                block_light: self.torch_light(f.pos + glam::DVec3::splat(0.5)),
                yaw: 0.0,
                icon: None,
            })
            .chain(self.mobs.entities.tnt.iter().map(|t| {
                let s = t.size();
                crate::render::BlockModel {
                    min: t.previous_pos.lerp(t.pos, alpha)
                        - glam::DVec3::new(s as f64 / 2.0, (s as f64 - 1.0) / 2.0, s as f64 / 2.0),
                    size: s,
                    // White flashes count down to the blast.
                    block: if t.flash() { Block::WOOL } else { Block::TNT },
                    sky_light: crate::entity::sky_light(&self.world, t.pos + glam::DVec3::Y * 0.5),
                    block_light: self.torch_light(t.pos + glam::DVec3::Y * 0.5),
                    yaw: 0.0,
                    icon: None,
                }
            }))
            .chain(self.world.moving_piston_blocks(alpha).map(|(block, min)| crate::render::BlockModel {
                min,
                size: 1.0,
                block,
                sky_light: crate::entity::sky_light(&self.world, min + glam::DVec3::splat(0.5)),
                block_light: self.torch_light(min + glam::DVec3::splat(0.5)),
                yaw: 0.0,
                icon: None,
            }))
            .chain(self.item_models(alpha))
            .collect()
    }
}

/// Adds player models (feet at the given interpolated positions) within
/// fog range of `camera`.
fn push_avatars(
    world: &World,
    players: &[(&Player, DVec3, crate::entity::model::PlayerAppearance, &str)],
    camera: DVec3,
    fog_end: f32,
    time: f32,
    verts: &mut Vec<crate::entity::model::EntityVertex>,
) {
    for &(player, feet, appearance, name) in players {
        // A body around the camera (players can share a spot) would fill the view.
        let inside = (camera - feet).with_y(0.0).length() < 0.4 && (feet.y..feet.y + 1.9).contains(&camera.y);
        if !inside && feet.distance_squared(camera) < (fog_end as f64 + 2.0).powi(2) {
            crate::entity::model::build_player(
                player,
                feet,
                camera,
                (
                    crate::entity::sky_light(world, player.eye()),
                    world.block_light(player.eye().floor().as_ivec3()) as f32 / 15.0,
                ),
                time,
                appearance,
                verts,
            );
            if appearance.death.is_none() {
                crate::entity::model::build_player_name(player, feet, name, camera, verts);
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
