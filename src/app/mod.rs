//! Window, input and the per-frame game loop.

mod actions;
mod bed;
mod bow;
mod bucket;
mod containers;
mod dimension;
mod farming;
mod hud;
mod items;
mod menu;
mod mobs;
mod recipe_book;
mod settings;
pub mod survival;
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
use winit::window::{CursorGrabMode, Fullscreen, Icon, Window, WindowId};

use crate::Args;
use crate::inventory::{HOTBAR_SLOTS, Inventory, Stack};
use crate::item::Item;
use crate::player::{MoveInput, Player};
use crate::render::{FrameParams, Renderer};
use crate::world::World;
use crate::world::block::Block;
use crate::world::storage::{LevelInfo, Storage};
use crate::world::terrain::{Dimension, Generator};

use survival::Vitals;

const REACH: f64 = 6.0;
const ACTION_REPEAT: f64 = 0.22;
/// Pause between breaking one block and starting the next in survival.
const BREAK_DELAY: f64 = 0.15;
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
const DAY_LENGTH: f64 = 600.0;

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
}

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
    /// Shift / Ctrl state (shift-click, Ctrl+Q, sneak-placing).
    modifiers: winit::keyboard::ModifiersState,
    mouse_grabbed: bool,
    left_held: bool,
    right_held: bool,
    action_cooldown: f64,
    mode: GameMode,
    inventory: Inventory,
    inventory_open: bool,
    /// Crafting grid of the open screen: 2x2 in the inventory, 3x3 at a
    /// crafting table. Emptied back into the inventory when it closes.
    craft: crate::crafting::Grid,
    container: Container,
    recipe_book: recipe_book::RecipeBook,
    /// First visible row of the creative palette.
    creative_scroll: usize,
    /// Mouse position in physical pixels (for the inventory screen).
    cursor_px: (f32, f32),
    actions: actions::Actions,
    show_hud: bool,
    last_space: Instant,
    last_frame: Instant,
    /// Fraction of the day: 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
    day_time: f64,
    /// Seconds spent asleep so far (the screen fades out), if in bed.
    sleeping: Option<f32>,
    /// Foot of the bed the player respawns at.
    spawn_bed: Option<glam::IVec3>,
    weather: weather::Weather,
    weather_verts: Vec<crate::render::weather::WeatherVertex>,
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
    /// Health, air, hunger and fall tracking (survival).
    vitals: Vitals,
    screenshot: Option<String>,
    screenshot_state: u32,
    place: Vec<(glam::IVec3, Block)>,
    /// `--drop`: thrown once the world has loaded.
    drop: Vec<(Item, u8)>,
    placed: bool,
    /// `--bench-render`: per-frame wall times (CPU + GPU, serialised).
    bench_render: Option<Vec<f64>>,
    frame_started: Option<Instant>,
    audio: crate::audio::Audio,
    mobs: mobs::Mobs,
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
        let save_on_exit = args.screenshot.is_none() && !args.bench_render;
        Self { args, saves_dir, save_on_exit, game: None, title: None }
    }

    /// Loads (or creates) a world and leaves the title screen. Debug
    /// options like `--give` only apply to the first world of a session.
    fn play(&mut self, shell: Shell, world_dir: &str, new: Option<title::NewWorld>) {
        let mut game = Game::start(shell, &self.args, &self.saves_dir, world_dir, new);
        game.apply_settings();
        self.args.clear_one_shot();
        self.title = None;
        self.game = Some(game);
    }

    /// Saves the world and goes back to the title screen.
    fn quit_to_title(&mut self) {
        let Some(mut game) = self.game.take() else { return };
        game.save();
        game.save_settings();
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
            .with_inner_size(PhysicalSize::new(1600, 900));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        // Saved options, with command-line overrides for this session.
        let scripted = self.args.screenshot.is_some() || self.args.bench_render;
        let settings_path = (!scripted).then(|| self.saves_dir.join("options.txt"));
        let mut settings = settings_path.as_deref().map(settings::Settings::load).unwrap_or_default();
        if let Some(rd) = self.args.render_distance {
            settings.render_distance = rd;
        }
        if let Some(v) = self.args.volume {
            settings.volume = v;
        }
        settings.vsync &= !self.args.no_vsync;

        let renderer = pollster::block_on(Renderer::new(window, settings.vsync));
        let audio = crate::audio::Audio::new(self.args.mute, settings.volume);
        let shell = Shell { renderer, audio, settings, settings_path };
        // A named world, a fresh one or a scripted run skips the title screen.
        let to_title = self.args.open_menu.as_deref() == Some("title");
        match self.args.world.clone() {
            Some(world) if !to_title => self.play(shell, &world, None),
            None if !to_title && (self.args.new_world || scripted) => self.play(shell, "world", None),
            _ => {
                let mut title = title::Title::new(shell, &self.saves_dir);
                title.screenshot = self.args.screenshot.take();
                self.title = Some(title);
            }
        }
    }

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
                Some(title::Action::Quit) => event_loop.exit(),
                None => {}
            }
            return;
        }
        let Some(game) = self.game.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => {
                self.save_on_exit = true;
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
                if game.had_focus && game.settings_path.is_some() && game.menu.is_none() && !game.vitals.is_dead() {
                    game.open_menu();
                } else {
                    game.set_grab(false);
                    game.keys.clear();
                    game.left_held = false;
                    game.right_held = false;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                match event.state {
                    ElementState::Pressed => {
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
                if game.menu.is_some() {
                    if button == MouseButton::Left && game.menu_click(pressed) == Some(menu::MenuAction::Quit) {
                        self.quit_to_title();
                    }
                    return;
                }
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
                            game.actions.breaking = None;
                            game.release_attack();
                        } else if game.attack() {
                            game.actions.breaking = None;
                        } else if game.mode == GameMode::Creative {
                            game.break_block();
                            game.action_cooldown = ACTION_REPEAT;
                        }
                    }
                    MouseButton::Right => {
                        game.right_held = pressed;
                        if pressed && !game.equip_held() && !game.start_draw() {
                            game.place_block();
                            game.action_cooldown = ACTION_REPEAT;
                        } else if !pressed {
                            game.actions.eat_timer = 0.0;
                            game.release_bow();
                        }
                    }
                    MouseButton::Middle if pressed => game.pick_block(),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { .. } if game.menu.is_some() => {}
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
                    game.select((game.actions.selected + step) % 9);
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
            let k = MOUSE_SENSITIVITY * game.settings.sensitivity;
            game.player.look(delta.0 as f32 * k, delta.1 as f32 * k);
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
    /// Loads the world saved in `saves_dir/world_dir`, or creates it (also
    /// when `new` describes a world made on the title screen).
    fn start(
        shell: Shell,
        args: &Args,
        saves_dir: &std::path::Path,
        world_dir: &str,
        new: Option<title::NewWorld>,
    ) -> Game {
        let Shell { renderer, audio, settings, settings_path } = shell;
        let storage = Storage::new(saves_dir.join(world_dir));
        let existing = if args.new_world || new.is_some() || !storage.exists() {
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
        let seed =
            existing.as_ref().map(|l| l.seed).or(new.as_ref().and_then(|n| n.seed)).or(args.seed).unwrap_or_else(
                || {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(1)
                },
            );
        let root_props = existing.as_ref().map(|l| l.props.clone()).unwrap_or_default();
        let saved_dimension = root_props.get("dimension").and_then(|d| Dimension::from_name(d)).unwrap_or_default();
        let dimension = args.dimension.unwrap_or(saved_dimension);
        let overworld_props = dimension::dimension_props(&root_props);
        let (saved, dimension_props) = if existing.is_some() {
            dimension::load_dimension(&storage, dimension, &overworld_props)
        } else {
            Default::default()
        };
        log::info!("world '{world_dir}' seed {seed}, {} modified chunks in the {}", saved.len(), dimension.name());

        let mode =
            match (new.as_ref().map(|n| n.mode).or(args.mode), existing.as_ref().and_then(|l| l.props.get("mode"))) {
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

        let mut inventory = inventory;
        for &(item, count) in &args.give {
            inventory.add(item, count);
        }
        for &item in &args.wear {
            if let Some((piece, _)) = item.as_armor() {
                inventory.armor[piece as usize] = Some(Stack::new(item, 1));
            }
        }

        let prop = |k: &str| existing.as_ref().and_then(|l| l.props.get(k));
        let mut vitals = Vitals::restore(
            prop("health").and_then(|s| s.parse().ok()).unwrap_or(survival::MAX_HEALTH),
            prop("air").and_then(|s| s.parse().ok()).unwrap_or(survival::MAX_AIR),
            prop("death").cloned(),
        );
        if let Some(h) = args.health {
            vitals = Vitals::restore(h, vitals.air, None);
        }
        if let Some(a) = args.air {
            vitals.air = a.clamp(0.0, survival::MAX_AIR);
        }
        if let Some(text) = prop("hunger") {
            let n: Vec<f32> = text.split(',').filter_map(|v| v.parse().ok()).collect();
            if let [food, saturation, exhaustion] = n[..] {
                vitals.hunger = survival::Hunger::restore(food, saturation, exhaustion);
            }
        }
        if let Some(f) = args.food {
            vitals.hunger = survival::Hunger::restore(f, 0.0, 0.0);
        }

        let generator = Arc::new(Generator::for_dimension(seed, dimension));
        let mut player = Player::new(Generator::new(seed).find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        if let Some((pos, yaw, pitch)) = existing.as_ref().and_then(|l| l.player) {
            player.pos = pos;
            player.yaw = yaw;
            player.pitch = pitch;
        }
        // `--dimension` into the other world: arrive by portal, as if the
        // player had walked through one where they stood.
        let arrival = (dimension != saved_dimension && args.pose.is_none()).then(|| {
            let scale = if dimension == Dimension::Nether { 1.0 / 8.0 } else { 8.0 };
            let p = (player.pos * DVec3::new(scale, 0.0, scale)).floor().as_ivec3();
            dimension::Arrival::Portal(p.with_y(if dimension == Dimension::Nether { 64 } else { 100 }))
        });
        if let Some([x, y, z, yaw, pitch]) = args.pose {
            player.pos = DVec3::new(x, y, z);
            player.yaw = (yaw as f32).to_radians();
            player.pitch = (pitch as f32).to_radians();
            player.flying = true;
        }
        player.can_fly = mode == GameMode::Creative;
        let world = World::new(generator, saved, settings.render_distance);
        log::info!("{} worker threads", world.worker_threads());

        let now = Instant::now();
        let mut game = Game {
            renderer,
            world,
            player,
            storage,
            keys: FxHashSet::default(),
            modifiers: Default::default(),
            mouse_grabbed: false,
            left_held: false,
            right_held: false,
            action_cooldown: 0.0,
            mode,
            inventory,
            inventory_open: args.open_inventory,
            craft: crate::crafting::Grid::new(2),
            container: Container::Inventory,
            recipe_book: recipe_book::RecipeBook::default(),
            creative_scroll: 0,
            cursor_px: (0.0, 0.0),
            actions: actions::Actions::default(),
            show_hud: true,
            last_space: now - Duration::from_secs(1),
            last_frame: now,
            day_time: args
                .time
                .or_else(|| existing.as_ref().and_then(|l| l.props.get("time")).and_then(|t| t.parse().ok()))
                .unwrap_or(0.08),
            sleeping: None,
            spawn_bed: existing.as_ref().and_then(|l| l.props.get("bed")).and_then(|t| {
                let v: Vec<i32> = t.split(',').filter_map(|s| s.parse().ok()).collect();
                (v.len() == 3).then(|| glam::IVec3::new(v[0], v[1], v[2]))
            }),
            weather: {
                let mut w = weather::Weather::new(seed);
                if let Some(text) = existing.as_ref().and_then(|l| l.props.get("weather")) {
                    w.deserialize(text);
                }
                if let Some(rain) = args.weather {
                    w.set(rain, true);
                }
                w
            },
            weather_verts: Vec::new(),
            started: now,
            last_save: now,
            stats_since: now,
            frames: 0,
            frame_time_sum: 0.0,
            fps: 0.0,
            cpu_ms: 0.0,
            show_debug: args.debug_overlay,
            popup: (String::new(), now - Duration::from_secs(10)),
            vitals,
            screenshot: args.screenshot.clone(),
            screenshot_state: 0,
            place: args.place.clone(),
            drop: args.drop.clone(),
            placed: false,
            bench_render: args.bench_render.then(Vec::new),
            frame_started: None,
            audio,
            mobs: mobs::Mobs::new(seed, args.spawn.clone(), args.wait),
            menu: match args.open_menu.as_deref() {
                Some("pause") => Some(menu::Screen::Pause),
                Some("options") => Some(menu::Screen::Options),
                _ => None,
            },
            menu_drag: None,
            had_focus: false,
            settings,
            settings_path,
            dimension,
            overworld_props: if dimension == Dimension::Nether { overworld_props } else { Default::default() },
            arrival,
            portal_time: 0.0,
            portal_locked: false,
            world_name: new
                .map(|n| n.name)
                .or_else(|| existing.as_ref().and_then(|l| l.props.get("name")).cloned())
                .unwrap_or_else(|| world_dir.to_string()),
        };
        game.renderer.force_offscreen = game.bench_render.is_some();
        game.restore_dimension(&dimension_props);
        if game.vitals.is_dead() {
            game.on_death();
        } else if game.screenshot.is_none() && game.bench_render.is_none() {
            game.set_grab(true);
        }
        game
    }

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

    fn on_key(&mut self, code: KeyCode) {
        if self.menu.is_some() {
            if code == KeyCode::Escape {
                self.menu_back();
            }
            return;
        }
        let blocked_when_dead =
            [KeyCode::KeyE, KeyCode::KeyG, KeyCode::KeyF, KeyCode::KeyQ, KeyCode::Space, KeyCode::Escape];
        if self.vitals.is_dead() && blocked_when_dead.contains(&code) {
            return;
        }
        match code {
            KeyCode::Escape if self.inventory_open => self.toggle_inventory(),
            KeyCode::Escape => self.open_menu(),
            KeyCode::KeyE => self.toggle_inventory(),
            KeyCode::KeyQ => self.drop_selected(self.modifiers.control_key()),
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
            KeyCode::KeyT => self.day_time = (self.day_time + 1.0 / 12.0).fract(),
            KeyCode::F11 => {
                let w = &self.renderer.window;
                w.set_fullscreen(match w.fullscreen() {
                    Some(_) => None,
                    None => Some(Fullscreen::Borderless(None)),
                });
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
        if self.actions.select(slot) {
            self.show_selected_name();
        }
    }

    fn show_selected_name(&mut self) {
        if let Some(s) = self.inventory.get(self.actions.selected) {
            self.show_popup(s.item.name());
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
        if taken > 0.0 {
            self.sleeping = None;
            self.audio.play(crate::audio::sounds::Sound::Hurt, None, 0.9, (0.92, 1.05));
        }
        if taken > 0.0 && self.vitals.is_dead() {
            self.on_death();
        }
        taken
    }

    /// Hurts the player through their armor (mobs, arrows, blasts, lava),
    /// wearing it down when the hit lands.
    pub(crate) fn damage_player_armored(&mut self, amount: f32, cause: &str) -> f32 {
        let reduced = survival::armor_reduce(amount, self.inventory.armor_points());
        let taken = self.damage_player(reduced, cause);
        if taken > 0.0 && self.mode == GameMode::Survival {
            for item in self.inventory.wear_armor(amount) {
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
        log::info!("player {}", self.vitals.death.as_deref().unwrap_or("died"));
        if self.inventory_open {
            self.toggle_inventory();
        }
        if self.mode == GameMode::Survival {
            self.drop_everything();
        }
        self.set_grab(false);
        self.keys.clear();
        self.left_held = false;
        self.right_held = false;
        self.actions.reset();
    }

    /// Back to the world spawn with full health.
    fn respawn(&mut self) {
        if self.dimension != Dimension::Overworld {
            self.vitals.respawn();
            self.switch_dimension(Dimension::Overworld, dimension::Arrival::Respawn);
            self.set_grab(true);
            return;
        }
        self.player.pos = self.respawn_point();
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
        self.actions.reset();
        self.show_popup(&format!("{} mode", capitalize(mode.name())));
    }

    fn toggle_inventory(&mut self) {
        self.inventory_open = !self.inventory_open;
        if self.inventory_open {
            self.set_grab(false);
            self.keys.clear();
            self.left_held = false;
            self.right_held = false;
            self.actions.reset();
        } else {
            if let Container::Chest(pos) = self.container {
                self.chest_sound(pos, 0.8);
            }
            self.inventory.return_stacks(self.craft.take_all());
            self.craft = crate::crafting::Grid::new(2);
            self.container = Container::Inventory;
            self.set_grab(true);
        }
    }

    /// Right-click on a crafting table: its 3x3 grid with the inventory.
    fn open_crafting_table(&mut self) {
        if self.inventory_open {
            return;
        }
        self.craft = crate::crafting::Grid::new(3);
        self.container = Container::CraftingTable;
        self.toggle_inventory();
    }

    /// Right-click on a furnace: its input, fuel and output with the inventory.
    fn open_furnace(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.furnace(pos).is_none() {
            return;
        }
        self.container = Container::Furnace(pos);
        self.toggle_inventory();
    }

    /// A click on a furnace slot. Fuel only takes things that burn; the
    /// output can only be taken from.
    fn furnace_click(&mut self, pos: IVec3, slot: hud::SlotRef, right: bool) {
        let cursor = &mut self.inventory.cursor;
        let Some(f) = self.world.furnace_mut(pos) else { return };
        match slot {
            hud::SlotRef::FurnaceInput => f.click_input(cursor, right),
            hud::SlotRef::FurnaceFuel => {
                if cursor.is_none_or(|c| crate::world::furnace::burn_time(c.item).is_some()) {
                    crate::inventory::click_slot(&mut f.fuel, cursor, right)
                }
            }
            hud::SlotRef::FurnaceOutput => match (cursor.as_mut(), f.output) {
                (None, out) => {
                    *cursor = out;
                    f.output = None;
                }
                (Some(c), Some(out)) if c.stacks_with(&out) && c.count as u16 + out.count as u16 <= c.max() as u16 => {
                    c.count += out.count;
                    f.output = None;
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// Clicking the crafting result: takes one craft onto the cursor (or
    /// onto a matching held stack, if it fits) and uses up the ingredients.
    fn take_craft_result(&mut self) {
        let Some(result) = self.craft.result() else { return };
        match &mut self.inventory.cursor {
            None => self.inventory.cursor = Some(result),
            Some(c) if c.stacks_with(&result) && c.count as u16 + result.count as u16 <= c.max() as u16 => {
                c.count += result.count
            }
            Some(_) => return,
        }
        self.craft.consume();
    }

    fn inventory_click(&mut self, right: bool) {
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
        if self.modifiers.shift_key() && self.inventory.cursor.is_none() {
            if let Some(slot) = slot {
                self.quick_move(slot);
            }
            return;
        }
        match slot {
            Some(hud::SlotRef::Inventory(i)) => self.inventory.click(i, right),
            Some(hud::SlotRef::Craft(i)) => {
                crate::inventory::click_slot(&mut self.craft.cells[i], &mut self.inventory.cursor, right)
            }
            Some(hud::SlotRef::CraftResult) => self.take_craft_result(),
            Some(hud::SlotRef::Armor(piece)) => self.inventory.click_armor(piece, right),
            Some(hud::SlotRef::Chest(i)) => {
                if let Container::Chest(pos) = self.container
                    && let Some(chest) = self.world.chest_mut(pos)
                {
                    crate::inventory::click_slot(&mut chest.slots[i], &mut self.inventory.cursor, right);
                }
            }
            Some(s @ (hud::SlotRef::FurnaceInput | hud::SlotRef::FurnaceFuel | hud::SlotRef::FurnaceOutput)) => {
                if let Container::Furnace(pos) = self.container {
                    self.furnace_click(pos, s, right);
                }
            }
            Some(hud::SlotRef::Palette(item)) => {
                // Creative palette: take a full stack, or trash the held one.
                self.inventory.cursor = match self.inventory.cursor {
                    Some(_) => None,
                    None => Some(Stack::new(item, item.max_stack())),
                };
            }
            None => {}
        }
    }

    /// Scrolls the creative palette by whole rows.
    fn scroll_palette(&mut self, rows: i32) {
        if self.mode == GameMode::Creative {
            let max = hud::palette_rows().saturating_sub(hud::PALETTE_ROWS);
            self.creative_scroll = self.creative_scroll.saturating_add_signed(rows as isize).min(max);
        }
    }

    fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.world.raycast(self.player.eye(), self.player.forward().as_dvec3(), REACH)
    }

    /// Instant break (creative).
    fn break_block(&mut self) {
        if self.attacking() {
            return;
        }
        if let Some((pos, _)) = self.target()
            && let Some(block) = self.world.get_block(pos)
            && block != Block::BEDROCK
        {
            self.world.set_block(pos, Block::AIR);
            self.audio.block_break(block, pos);
            if block.is_bed() {
                self.break_bed_partner(pos, block);
            }
        }
    }

    /// Timed break with drops (survival). Called every frame while the
    /// button is held.
    fn continue_breaking(&mut self, dt: f64) {
        if self.attacking() {
            self.actions.breaking = None;
            return;
        }
        let Some((pos, _)) = self.target() else {
            self.actions.breaking = None;
            return;
        };
        let Some(block) = self.world.get_block(pos) else { return };
        let held = self.held_item();
        let progress = self.actions.mine(pos, block, held, dt);
        if progress < 1.0 {
            self.audio.block_hit(block, pos, dt);
            return;
        }
        self.actions.breaking = None;
        self.action_cooldown = BREAK_DELAY;
        // Broken ice melts into water, unless it was floating over nothing.
        let melts = block == Block::ICE
            && self.dimension.has_sky()
            && self.world.get_block(pos - glam::IVec3::Y).is_some_and(|b| b != Block::AIR);
        self.world.set_block(pos, if melts { Block::WATER } else { Block::AIR });
        self.audio.block_break(block, pos);
        // Stone, ores and the like only drop with a good enough pickaxe.
        if crate::mining::can_harvest(block, held) {
            self.world.spill_block(pos, block);
        }
        if block.is_bed() {
            self.break_bed_partner(pos, block);
        }
        self.vitals.hunger.exhaust(survival::EXHAUST_MINE);
        if block.hardness() > 0.0 {
            self.wear_held(false);
        }
    }

    /// Eating: holding right-click with food in survival, when not full,
    /// finishes a bite after [`EAT_TIME`] seconds.
    fn eat(&mut self, acting: bool, dt: f64) {
        let food = self.held_item().and_then(|i| i.food());
        let eating = acting && self.right_held && self.mode == GameMode::Survival && self.vitals.hunger.can_eat();
        let Some((hunger, saturation)) = food.filter(|_| eating) else {
            self.actions.eat_timer = 0.0;
            return;
        };
        let before = self.actions.eat_timer;
        let finished = self.actions.eat(dt);
        // Chewing sounds four times a second.
        if (before / 0.25).floor() != ((before + dt) / 0.25).floor() {
            let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
            self.audio.play(sound, None, 0.7, (1.4, 1.7));
        }
        if finished {
            self.inventory.take_one(self.actions.selected);
            self.vitals.hunger.eat(hunger, saturation);
        }
    }

    /// Right-click with armor in hand puts it on (swapping out the worn
    /// piece), unless aimed at a container. Returns whether it did.
    fn equip_held(&mut self) -> bool {
        if self.mode != GameMode::Survival || self.aiming_at_usable() || !self.inventory.equip(self.actions.selected) {
            return false;
        }
        let sound = crate::audio::sounds::Sound::Place(crate::audio::sounds::Material::Wood);
        self.audio.play(sound, None, 0.6, (1.4, 1.6));
        true
    }

    /// The item in the selected hotbar slot.
    pub(super) fn held_item(&self) -> Option<Item> {
        self.inventory.get(self.actions.selected).map(|s| s.item)
    }

    /// Wears the held tool for a block broken or a mob hit (survival).
    pub(super) fn wear_held(&mut self, hitting_mob: bool) {
        let Some(held) = self.held_item() else { return };
        if self.mode == GameMode::Survival
            && self.inventory.wear(self.actions.selected, crate::mining::wear(held, hitting_mob))
        {
            self.show_popup(&format!("{} broke", capitalize(held.name())));
            self.audio.play(
                crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
                None,
                0.8,
                (1.3, 1.5),
            );
        }
    }

    fn place_block(&mut self) {
        if !self.aiming_at_usable() && self.use_bucket() {
            return;
        }
        let Some((pos, normal)) = self.target() else { return };
        // Containers open on right-click; holding Shift builds against them.
        match self.world.get_block(pos) {
            _ if self.modifiers.shift_key() => {}
            Some(Block::CRAFTING_TABLE) => return self.open_crafting_table(),
            Some(b) if crate::world::furnace::is_furnace(b) => return self.open_furnace(pos),
            Some(b) if crate::world::chest::is_chest(b) => return self.open_chest(pos),
            Some(b) if b.is_bed() => return self.use_bed(pos),
            _ => {}
        }
        if self.strike_flint(pos, normal) || self.use_item_on(pos, normal) {
            return;
        }
        // Clicking tall grass replaces it instead of building against it.
        let at = if self.world.get_block(pos).is_some_and(|b| b.is_replaceable()) { pos } else { pos + normal };
        if self.held_item() == Some(Item::BED) {
            if self.place_bed(at) && self.mode == GameMode::Survival {
                self.inventory.take_one(self.actions.selected);
            }
            return;
        }
        let Some(block) = self.inventory.get(self.actions.selected).and_then(|s| s.item.places()) else { return };
        // Furnaces and chests face whoever places them.
        let block = block.with_facing(crate::world::block::Facing::toward(self.player.forward()));
        if block.is_water() && !self.dimension.has_sky() {
            // Water boils away in the Nether.
            self.audio.play(crate::audio::sounds::Sound::Fuse, Some(at.as_dvec3()), 0.6, (1.6, 1.8));
            return;
        }
        let free = self.world.get_block(at).is_some_and(|b| b.is_replaceable());
        let below = self.world.get_block(at - glam::IVec3::Y);
        let supported = below.is_some_and(|below| block.can_stay_on(below))
            && (block != Block::SUGAR_CANE || below == Some(Block::SUGAR_CANE) || self.world.cane_has_water(at));
        if free
            && supported
            && !(block.is_solid() && self.player.intersects_block(at))
            && self.world.set_block(at, block)
        {
            self.audio.block_place(block, at);
            if self.mode == GameMode::Survival {
                self.inventory.take_one(self.actions.selected);
            }
        }
    }

    fn pick_block(&mut self) {
        let Some(b) = self.target().and_then(|(pos, _)| self.world.get_block(pos)) else { return };
        let item = farming::picked_item(b);
        match self.inventory.find(item) {
            Some(i) if i < HOTBAR_SLOTS => self.select(i),
            Some(i) => {
                self.actions.reset();
                self.inventory.slots.swap(i, self.actions.selected);
                self.show_selected_name();
            }
            None if self.mode == GameMode::Creative => {
                self.actions.reset();
                self.inventory.slots[self.actions.selected] = Some(Stack::new(item, item.max_stack()));
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
        // Fanned out so each one can be seen.
        let drops = std::mem::take(&mut self.drop);
        for (i, &(item, count)) in drops.iter().enumerate() {
            let turn = (i as f64 - (drops.len() as f64 - 1.0) / 2.0) * 0.22;
            let dir = glam::DQuat::from_rotation_y(turn) * self.player.forward().as_dvec3();
            self.mobs.entities.throw(Stack::new(item, count), self.player.eye(), dir);
        }
        self.placed = true;
        self.spawn_pending_mobs();
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
            0 if settled && self.world.is_idle() && self.mobs.waited() => {
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
        self.player.yaw += std::f32::consts::TAU / FRAMES as f32;
        self.frame_started = Some(Instant::now());
        false
    }

    fn save(&mut self) {
        let mut props = std::collections::BTreeMap::new();
        props.insert("mode".to_string(), self.mode.name().to_string());
        props.insert("name".to_string(), self.world_name.clone());
        // Save what's held or on the crafting grid as if the screen closed.
        let mut inventory = self.inventory.clone();
        inventory.return_stacks(self.craft.cells.iter().flatten().copied());
        props.insert("inventory".to_string(), inventory.serialize());
        props.insert("health".to_string(), self.vitals.health.to_string());
        props.insert("air".to_string(), format!("{:.2}", self.vitals.air));
        let h = self.vitals.hunger;
        props.insert("hunger".to_string(), format!("{:.2},{:.2},{:.3}", h.food, h.saturation, h.exhaustion));
        if let Some(cause) = &self.vitals.death {
            props.insert("death".to_string(), cause.clone());
        }
        props.insert("dimension".to_string(), self.dimension.name().to_string());
        props.insert("time".to_string(), format!("{:.5}", self.day_time));
        props.insert("weather".to_string(), self.weather.serialize());
        if let Some(b) = self.spawn_bed {
            props.insert("bed".to_string(), format!("{},{},{}", b.x, b.y, b.z));
        }
        let seed = self.world.generator.seed;
        let chunks = self.world.modified_chunks();
        // The overworld saves with the player; the Nether in its own folder.
        let result = match self.dimension {
            Dimension::Overworld => {
                props.extend(self.dimension_props());
                let player = Some((self.player.pos, self.player.yaw, self.player.pitch));
                self.storage.save(&LevelInfo { seed, player, props }, &chunks)
            }
            Dimension::Nether => {
                let nether = LevelInfo { seed, player: None, props: self.dimension_props() };
                dimension::storage_for(&self.storage, Dimension::Nether).save(&nether, &chunks).and_then(|()| {
                    props.extend(self.overworld_props.clone());
                    let player = Some((self.player.pos, self.player.yaw, self.player.pitch));
                    self.storage.save_level(&LevelInfo { seed, player, props })
                })
            }
        };
        match result {
            Ok(()) => log::info!(
                "saved {} modified {} chunks to {}",
                chunks.len(),
                self.dimension.name(),
                self.storage.dir().display()
            ),
            Err(e) => log::error!("save failed: {e}"),
        }
        self.last_save = Instant::now();
    }

    fn frame(&mut self) {
        let now = Instant::now();
        // The world stands still while a menu is open.
        let dt = if self.menu.is_some() { 0.0 } else { (now - self.last_frame).as_secs_f64().min(0.1) };
        self.last_frame = now;

        // --- Simulation ---------------------------------------------------
        // Travelling between dimensions: frozen until the far side loads.
        let arriving = self.update_arrival();
        let held = |k: KeyCode| self.keys.contains(&k);
        let axis = |pos: KeyCode, neg: KeyCode| held(pos) as i32 as f64 - held(neg) as i32 as f64;
        let input = if self.mouse_grabbed
            && !self.inventory_open
            && !self.vitals.is_dead()
            && self.sleeping.is_none()
            && !arriving
        {
            MoveInput {
                forward: axis(KeyCode::KeyW, KeyCode::KeyS),
                right: axis(KeyCode::KeyD, KeyCode::KeyA),
                jump: held(KeyCode::Space),
                descend: held(KeyCode::ShiftLeft),
                // Too hungry to sprint at 6 food or less (survival).
                sprint: (held(KeyCode::ControlLeft) || held(KeyCode::KeyR))
                    && (self.mode == GameMode::Creative || self.vitals.hunger.can_sprint()),
            }
        } else {
            MoveInput::default()
        };
        let before = self.player.pos;
        if !arriving {
            self.player.update(dt, input, &self.world);
            self.update_portal(dt);
        }
        let moved = (self.player.pos - before).with_y(0.0).length();
        self.weather.update(dt);
        self.update_sleep(dt);
        self.world.raining = self.weather.raining && self.dimension.has_sky();
        let rain_here = weather::rain_at(&self.world, &self.weather, self.player.pos);
        self.audio.update(&self.player, &self.world, rain_here, dt);
        let env = survival::Env {
            y: self.player.pos.y,
            on_ground: self.player.on_ground,
            flying: self.player.flying,
            in_water: self.player.in_water,
            head_in_water: self.player.head_in_water(&self.world),
            in_lava: self.player.in_lava(&self.world),
            moved: if self.player.flying { 0.0 } else { moved },
            sprinting: input.sprint && moved > 0.0,
            jumped: self.player.jumped,
        };
        let hurts = if arriving || self.arrival.is_some() {
            Default::default()
        } else {
            self.vitals.tick(dt as f32, &env, self.mode == GameMode::Creative)
        };
        self.trample(hurts.landed);
        if hurts.fall > 0.0 {
            self.damage_player(hurts.fall, survival::CAUSE_FALL);
        }
        if hurts.drown > 0.0 {
            self.damage_player(hurts.drown, survival::CAUSE_DROWN);
        }
        if hurts.lava > 0.0 {
            self.damage_player_armored(hurts.lava, survival::CAUSE_LAVA);
        }
        if hurts.starve > 0.0 {
            self.damage_player(hurts.starve, survival::CAUSE_STARVE);
        }

        self.action_cooldown -= dt;
        let acting = self.mouse_grabbed && !self.inventory_open;
        if acting && self.left_held && self.mode == GameMode::Survival {
            if self.action_cooldown <= 0.0 {
                self.continue_breaking(dt);
            }
        } else if acting
            && self.action_cooldown <= 0.0
            && (self.left_held || self.right_held)
            && self.actions.bow_draw.is_none()
            // Buckets act once per click.
            && !(self.right_held && !self.left_held && self.holding_bucket())
        {
            if self.left_held {
                self.break_block();
            } else {
                self.place_block();
            }
            self.action_cooldown = ACTION_REPEAT;
        }
        self.eat(acting, dt);
        self.update_bow(acting, dt);

        // --- World streaming ------------------------------------------------
        if !self.placed && self.screenshot.is_none() && self.world.pending_jobs() == 0 && self.world.loaded_chunks() > 0
        {
            self.apply_placements();
        }
        self.world.tick_fluids(dt);
        self.world.tick_falling(dt);
        self.world.tick_furnaces(dt);
        self.world.tick_random(dt, self.player.pos);
        self.world.tick_leaf_decay(dt);
        self.world.update(self.player.pos);
        for (pos, mesh) in self.world.mesh_uploads.drain(..) {
            self.renderer.upload_mesh(pos, mesh);
        }
        for pos in self.world.mesh_removals.drain(..) {
            self.renderer.remove_mesh(pos);
        }
        self.update_mobs(dt);
        self.update_items();

        if now - self.last_save > AUTOSAVE_EVERY {
            self.save();
        }

        // --- Render ---------------------------------------------------------
        self.day_time = (self.day_time + dt / DAY_LENGTH).fract();
        let mut sky = sky_state(self.day_time);
        sky.daylight = self.weather.dim(sky.daylight);
        sky.horizon = self.weather.overcast(sky.horizon);
        sky.zenith = self.weather.overcast(sky.zenith);
        let nether = !self.dimension.has_sky();
        if nether {
            // No sun, no weather: a steady dim glow in a red haze.
            sky.daylight = dimension::NETHER_LIGHT;
            sky.horizon = dimension::NETHER_FOG;
            sky.zenith = dimension::NETHER_FOG;
        }
        let rain = if nether { 0.0 } else { self.weather.strength };
        let daylight = sky.daylight;
        let in_lava = self.player.head_in_lava(&self.world);
        let underwater = env.head_in_water || in_lava;
        let view_dist = (self.world.render_distance() * 32) as f32;
        let (fog_color, fog_start, fog_end) = if in_lava {
            (LAVA_FOG, 0.0, 2.0)
        } else if underwater {
            (WATER_FOG.map(|c| c * daylight), 0.0, 28.0)
        } else if nether {
            (sky.horizon, 8.0, view_dist.min(160.0) * 0.8)
        } else {
            (sky.horizon, view_dist * 0.55, view_dist * 0.95)
        };
        let verts = self.mobs.entities.mesh(
            self.player.eye(),
            self.player.forward(),
            fog_end,
            (now - self.started).as_secs_f32(),
        );
        self.renderer.set_entities(verts);
        weather::sheets(&self.world, self.player.eye(), rain, &mut self.weather_verts);
        self.renderer.set_weather(&self.weather_verts);
        let params = FrameParams {
            camera: self.player.eye(),
            forward: self.player.forward(),
            fov_y: self.settings.fov.to_radians()
                * if input.sprint && input.forward > 0.0 { 1.08 } else { 1.0 }
                * (1.0 - 0.15 * self.bow_power().unwrap_or(0.0)),
            sky_color: fog_color.map(|c| c as f64),
            fog_color,
            fog_start,
            fog_end,
            daylight,
            zenith_color: if underwater { fog_color } else { sky.zenith },
            sun_dir: sky.sun_dir,
            time: (now - self.started).as_secs_f32(),
            highlight: self
                .target()
                .filter(|_| self.mob_target().is_none())
                .map(|(p, _)| (p, self.world.get_block(p).map_or(1.0, |b| b.height() as f32))),
            crack: self
                .actions
                .breaking
                .map(|(p, progress)| (p, crate::world::block::tex::CRACK_0 + (progress * 10.0).min(9.0) as u8)),
            block_models: self
                .world
                .falling_blocks()
                .iter()
                .map(|f| crate::render::BlockModel {
                    min: f.pos,
                    size: 1.0,
                    block: f.block,
                    sky_light: crate::entity::sky_light(&self.world, f.pos + glam::DVec3::splat(0.5)),
                    yaw: 0.0,
                    icon: None,
                })
                .chain(self.mobs.entities.tnt.iter().map(|t| {
                    let s = t.size();
                    crate::render::BlockModel {
                        min: t.pos - glam::DVec3::new(s as f64 / 2.0, (s as f64 - 1.0) / 2.0, s as f64 / 2.0),
                        size: s,
                        // White flashes count down to the blast.
                        block: if t.flash() { Block::WOOL } else { Block::TNT },
                        sky_light: crate::entity::sky_light(&self.world, t.pos + glam::DVec3::Y * 0.5),
                        yaw: 0.0,
                        icon: None,
                    }
                }))
                .chain(self.item_models())
                .collect(),
            rain,
            ui: if self.show_hud || self.vitals.is_dead() || self.menu.is_some() {
                self.build_ui(now)
            } else {
                Vec::new()
            },
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
