//! Window, input and the per-frame game loop.

mod actions;
mod agents;
mod anvil;
mod bed;
mod bow;
mod bucket;
mod console;
mod containers;
mod credits;
mod dimension;
mod doors;
mod enchanting;
mod farming;
mod gamepad;
mod hand;
mod hud;
mod items;
mod menu;
mod mobs;
mod pad_menu;
mod particles;
mod recipe_book;
mod search;
mod settings;
mod smithing;
mod split;
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
use winit::window::{CursorGrabMode, Fullscreen, Icon, Window, WindowId};

use crate::Args;
use crate::inventory::{HOTBAR_SLOTS, Inventory, Stack};
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
    Brewing(IVec3),
    Enchanting(IVec3),
    Anvil(IVec3),
    Smithing(IVec3),
}

struct Game {
    agents: agents::Agents,
    split: split::Split,
    pads: gamepad::Pads,
    /// `--pad-player` screen, seated once the world loads.
    virtual_pad: Option<String>,
    /// A controller player's state is swapped into the host's fields (see
    /// `Game::puppet`), and the container or bed they used, if any.
    puppet: bool,
    /// Who host interaction code is acting for: the host, or a controller
    /// player while `puppet` is set (thrown pearls remember their owner).
    actor: crate::entity::PlayerId,
    puppet_used: Option<glam::IVec3>,
    puppet_popup: Option<String>,
    console: console::Console,
    search: search::Search,
    renderer: Renderer,
    world: World,
    player: Player,
    camera: voxelcraft::camera::CameraMode,
    storage: Storage,
    keys: FxHashSet<KeyCode>,
    /// Shift / Ctrl state (shift-click, Ctrl+Q, sneak-placing).
    modifiers: winit::keyboard::ModifiersState,
    mouse_grabbed: bool,
    left_held: bool,
    right_held: bool,
    /// Preserve taps that begin and end between fixed game ticks.
    jump_pressed: bool,
    mine_pressed: bool,
    action_cooldown: f64,
    mode: GameMode,
    /// Shared world difficulty (old saves default to Normal).
    difficulty: Difficulty,
    /// World-wide one-life flag; locks difficulty to Hard.
    hardcore: bool,
    gamerules: GameRules,
    inventory: Inventory,
    inventory_open: bool,
    /// Crafting grid of the open screen: 2x2 in the inventory, 3x3 at a
    /// crafting table. Emptied back into the inventory when it closes.
    craft: crate::crafting::Grid,
    /// Inputs for the open enchanting table, anvil or smithing table;
    /// emptied back into the inventory when the screen closes.
    work: enchanting::WorkSlots,
    container: Container,
    recipe_book: recipe_book::RecipeBook,
    /// First visible row of the creative palette.
    creative_scroll: usize,
    /// Mouse position in physical pixels (for the inventory screen).
    cursor_px: (f32, f32),
    actions: actions::Actions,
    show_hud: bool,
    hand: hand::HandAnim,
    last_space: Instant,
    last_frame: Instant,
    clock: crate::simulation::FixedClock,
    previous_eye: DVec3,
    rendered_eye: DVec3,
    /// Fraction of the day: 0 sunrise, 0.25 noon, 0.5 sunset, 0.75 midnight.
    day_time: f64,
    /// Completed daylight cycles for `/time query day`.
    day_count: i64,
    /// Seconds spent asleep so far (the screen fades out), if in bed.
    sleeping: Option<f32>,
    /// Foot of the bed the player respawns at.
    spawn_bed: Option<glam::IVec3>,
    /// Whether this player has already seen the end credits.
    credits_seen: bool,
    /// Seconds into the credits while they play.
    credits: Option<f32>,
    /// Exact Overworld point set by `/spawnpoint`, replacing a bed spawn.
    spawn_point: Option<glam::IVec3>,
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
    /// Health, air, hunger and fall tracking (survival).
    vitals: Vitals,
    screenshot: Option<String>,
    screenshot_state: u32,
    place: Vec<(glam::IVec3, Block)>,
    /// `--open-block`: a container to open once placements are done.
    open_block: Option<IVec3>,
    /// `--drop`: thrown once the world has loaded.
    drop: Vec<(Item, u8)>,
    /// `--orbs` awards, spawned with the `--drop` items.
    orbs: Vec<u32>,
    placed: bool,
    /// `--bench-render`: per-frame wall times (CPU + GPU, serialised).
    bench_render: Option<Vec<f64>>,
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
        if let Some(enhanced) = self.args.enhanced_graphics {
            settings.enhanced_graphics = enhanced;
        }

        let renderer = pollster::block_on(Renderer::new(window, settings.vsync));
        let mut audio = crate::audio::Audio::new(self.args.mute, settings.volume);
        audio.set_music_volume(settings.music_volume);
        let shell = Shell { renderer, audio, settings, settings_path };
        // A named world, a fresh one or a scripted run skips the title screen.
        let to_title = matches!(self.args.open_menu.as_deref(), Some("title" | "create"));
        match self.args.world.clone() {
            Some(world) if !to_title => self.play(shell, &world, None),
            None if !to_title && (self.args.new_world || scripted) => self.play(shell, "world", None),
            _ => {
                let mut title = title::Title::new(shell, &self.saves_dir);
                if self.args.open_menu.as_deref() == Some("create") {
                    title.show_create();
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
                    game.jump_pressed = false;
                    game.mine_pressed = false;
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

pub(super) struct SkyState {
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
        // `--new` replaces the save, so the old world's Nether must not carry
        // over into the new one. Scripted runs never write saves.
        if args.new_world && settings_path.is_some() {
            for dimension in [Dimension::Nether, Dimension::End] {
                let nether = dimension::storage_for(&storage, dimension);
                if nether.exists()
                    && let Err(e) = std::fs::remove_dir_all(nether.dir())
                {
                    log::error!("failed to remove the old dimension save {}: {e}", nether.dir().display());
                }
            }
        }
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

        let mode = new
            .as_ref()
            .map(|n| n.mode)
            .or(args.mode)
            .or_else(|| existing.as_ref().and_then(|l| l.props.get("mode")).and_then(|m| GameMode::from_name(m)))
            .unwrap_or_default();
        let hardcore = new.as_ref().is_some_and(|n| n.hardcore)
            || root_props.get("hardcore").is_some_and(|v| v == "true" || v == "1");
        let difficulty = if hardcore {
            Difficulty::Hard
        } else {
            new.as_ref()
                .map(|n| n.difficulty)
                .or_else(|| root_props.get("difficulty").and_then(|d| Difficulty::from_name(d)))
                .unwrap_or_default()
        };
        let gamerules =
            root_props.get("gamerules").map_or_else(GameRules::default, |text| GameRules::deserialize(text));
        let inventory = existing
            .as_ref()
            .and_then(|l| l.props.get("inventory"))
            .and_then(|s| Inventory::deserialize(s))
            .unwrap_or_else(|| match mode {
                GameMode::Creative => Inventory::with_hotbar(&CREATIVE_HOTBAR),
                GameMode::Survival | GameMode::Adventure | GameMode::Spectator => Inventory::default(),
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
        for &(e, level) in &args.enchants {
            match crate::enchant::command(inventory.slots[0], e, level) {
                Ok(stack) => inventory.slots[0] = Some(stack),
                Err(err) => log::warn!("--enchant: {err}"),
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
        if let Some(xp) = prop("xp").and_then(|t| crate::simulation::experience::Experience::parse(t)) {
            vitals.xp = xp;
        }
        if let Some(text) = prop("effects") {
            vitals.effects = crate::simulation::effects::Effects::deserialize(text);
        }
        for &(effect, secs, amp) in &args.effects {
            let damage = vitals.apply_effect(effect, amp, secs.saturating_mul(20).max(1));
            vitals.damage(damage, survival::CAUSE_MAGIC, mode.invulnerable());
        }
        if let Some(level) = args.xp {
            let total = (0..level.min(1000)).map(crate::simulation::experience::points_to_next).sum();
            vitals.xp = crate::simulation::experience::Experience::restore(level, 0, total);
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
            if dimension == Dimension::End {
                return dimension::Arrival::EndSpawn;
            }
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
        player.can_fly = mode.can_fly();
        player.noclip = mode == GameMode::Spectator;
        if mode == GameMode::Spectator {
            player.flying = true;
        }
        let mut world = World::new(generator, saved, settings.render_distance);
        world.set_tile_drops(gamerules.bool("doTileDrops"));
        log::info!("{} worker threads", world.worker_threads());

        let now = Instant::now();
        let previous_eye = player.eye();
        let mut agents = agents::Agents { cheats: args.agent_cheats, ..Default::default() };
        if let Some(address) = args.agent_listen {
            agents.host =
                Some(voxelcraft::control::Host::bind(address, args.agent_token.clone()).unwrap_or_else(|e| {
                    eprintln!("agent host: {e}");
                    std::process::exit(1);
                }));
            eprintln!("Agent host listening on {}", agents.host.as_ref().unwrap().address);
        }
        if let Some(text) = root_props.get("agents") {
            agents.restore(text, dimension.name(), player.pos);
        }
        if let Some(text) = root_props.get("pad_beds") {
            agents.restore_pad_beds(text);
        }
        let mut game = Game {
            agents,
            split: split::Split { follow: args.split_screen.clone(), side_by_side: args.split_side },
            // Screenshot runs never read controllers.
            virtual_pad: args.pad_player.clone(),
            puppet: false,
            actor: crate::entity::PlayerId::HOST,
            puppet_used: None,
            puppet_popup: None,
            pads: {
                let mut pads = if args.screenshot.is_none() { gamepad::Pads::new() } else { Default::default() };
                let prop = |k: &str| root_props.get(k).map_or("", String::as_str);
                pads.restore(prop("pads"));
                pads
            },
            search: search::Search {
                query: args.inventory_search.clone().unwrap_or_default(),
                focused: args.open_inventory && args.screenshot.is_none(),
                ..Default::default()
            },
            console: console::Console { open: args.open_console, input: "/".into(), ..Default::default() },
            renderer,
            world,
            player,
            storage,
            keys: FxHashSet::default(),
            modifiers: Default::default(),
            mouse_grabbed: false,
            left_held: false,
            right_held: false,
            jump_pressed: false,
            mine_pressed: false,
            action_cooldown: 0.0,
            mode,
            difficulty,
            hardcore,
            gamerules,
            inventory,
            inventory_open: args.open_inventory,
            craft: crate::crafting::Grid::new(2),
            work: [None; 3],
            container: Container::Inventory,
            recipe_book: recipe_book::RecipeBook::default(),
            creative_scroll: 0,
            cursor_px: (0.0, 0.0),
            actions: actions::Actions::default(),
            show_hud: true,
            hand: Default::default(),
            camera: args.camera,
            last_space: now - Duration::from_secs(1),
            last_frame: now,
            clock: Default::default(),
            previous_eye,
            rendered_eye: previous_eye,
            day_time: args
                .time
                .or_else(|| existing.as_ref().and_then(|l| l.props.get("time")).and_then(|t| t.parse().ok()))
                .unwrap_or(0.08),
            day_count: root_props.get("day_count").and_then(|value| value.parse().ok()).unwrap_or(0),
            sleeping: None,
            credits_seen: root_props.get("credits_seen").is_some_and(|v| v == "1"),
            credits: None,
            spawn_bed: existing.as_ref().and_then(|l| l.props.get("bed")).and_then(|t| {
                let v: Vec<i32> = t.split(',').filter_map(|s| s.parse().ok()).collect();
                (v.len() == 3).then(|| glam::IVec3::new(v[0], v[1], v[2]))
            }),
            spawn_point: existing.as_ref().and_then(|l| l.props.get("spawn_point")).and_then(|text| {
                let n: Vec<i32> = text.split(',').filter_map(|value| value.parse().ok()).collect();
                (n.len() == 3).then(|| IVec3::new(n[0], n[1], n[2]))
            }),
            world_spawn: root_props
                .get("world_spawn")
                .and_then(|text| {
                    let n: Vec<i32> = text.split(',').filter_map(|value| value.parse().ok()).collect();
                    (n.len() == 3).then(|| IVec3::new(n[0], n[1], n[2]))
                })
                .unwrap_or_else(|| Generator::new(seed).find_spawn()),
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
            open_block: args.open_block,
            drop: args.drop.clone(),
            orbs: args.orbs.clone(),
            placed: false,
            bench_render: args.bench_render.then(Vec::new),
            frame_started: None,
            audio,
            mobs: mobs::Mobs::new(seed, args.spawn.clone(), args.wait),
            particles: crate::particles::System::new(seed),
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
            overworld_props: if dimension != Dimension::Overworld { overworld_props } else { Default::default() },
            arrival,
            portal_time: 0.0,
            portal_locked: false,
            world_name: new
                .map(|n| n.name)
                .or_else(|| existing.as_ref().and_then(|l| l.props.get("name")).cloned())
                .unwrap_or_else(|| world_dir.to_string()),
        };
        if game.console.open {
            game.set_grab(false);
        }
        game.renderer.force_offscreen = game.bench_render.is_some();
        game.restore_dimension(&dimension_props);
        if game.vitals.is_dead() {
            game.on_death();
        } else if game.screenshot.is_none() && game.bench_render.is_none() && !game.console.open {
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
                    GameMode::Creative | GameMode::Adventure | GameMode::Spectator => GameMode::Survival,
                };
                self.set_mode(mode);
            }
            KeyCode::KeyF if self.player.can_fly => {
                self.player.flying = !self.player.flying;
                self.player.vel = DVec3::ZERO;
            }
            // Waiting in bed for other players: jumping gets up.
            KeyCode::Space if self.sleeping.is_some() => self.sleeping = None,
            KeyCode::Space => {
                self.jump_pressed = true;
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
            KeyCode::F5 => self.camera.cycle(),
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
            self.show_popup(s.display_name());
        }
    }

    fn show_popup(&mut self, text: &str) {
        // Messages about a controller player's hands go to their own view.
        if self.puppet {
            self.puppet_popup = Some(text.to_string());
        } else {
            self.popup = (text.to_string(), Instant::now());
        }
    }

    /// Sneaking builds against containers and beds instead of using them.
    fn sneak_building(&self) -> bool {
        if self.puppet { self.player.sneaking } else { self.modifiers.shift_key() }
    }

    /// The single entry point for hurting the player (falls, drowning,
    /// mobs, ...), through protection enchantments. `amount` is in half
    /// hearts; `cause` completes the death
    /// message "Player <cause>", e.g. "drowned" or "was slain by a zombie".
    /// Returns the damage actually taken: zero in creative, while dead, or
    /// when absorbed by the 0.5 s hurt immunity that follows each hit (a
    /// stronger hit within it only deals the difference).
    pub(crate) fn damage_player(&mut self, amount: f32, cause: &str) -> f32 {
        let amount = crate::enchant::protect(amount, &self.inventory.armor, cause);
        let taken = self.vitals.damage(amount, cause, self.mode.invulnerable());
        if taken > 0.0 {
            self.player.animation.hurt_direction = 0.0;
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
        let reduced = survival::armor_reduce(amount, self.inventory.armor_points(), self.inventory.armor_toughness());
        let taken = self.damage_player(reduced, cause);
        if taken > 0.0 && self.mode.is_survival() {
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
        self.vitals.effects.clear();
        if self.gamerules.bool("showDeathMessages") {
            log::info!("player {}", self.vitals.death.as_deref().unwrap_or("died"));
        }
        if self.inventory_open {
            self.toggle_inventory();
        }
        if self.mode.is_survival() && !self.gamerules.bool("keepInventory") {
            self.drop_everything();
        }
        if self.hardcore {
            self.vitals.respawn();
            self.set_mode(GameMode::Spectator);
            self.show_popup("Game over - Spectator mode");
        }
        self.set_grab(false);
        self.keys.clear();
        self.left_held = false;
        self.right_held = false;
        self.jump_pressed = false;
        self.mine_pressed = false;
        self.actions.reset();
        if self.hardcore {
            self.set_grab(true);
        } else if self.gamerules.bool("doImmediateRespawn") {
            self.respawn();
        }
    }

    /// Back to the world spawn with full health.
    fn respawn(&mut self) {
        let kept_xp = self.gamerules.bool("keepInventory").then_some(self.vitals.xp);
        if self.dimension != Dimension::Overworld {
            self.vitals.respawn();
            if let Some(xp) = kept_xp {
                self.vitals.xp = xp;
            }
            self.switch_dimension(Dimension::Overworld, dimension::Arrival::Respawn);
            self.set_grab(true);
            return;
        }
        self.player.pos = self.respawn_point();
        self.player.vel = DVec3::ZERO;
        self.player.flying = false;
        self.vitals.respawn();
        if let Some(xp) = kept_xp {
            self.vitals.xp = xp;
        }
        self.set_grab(true);
    }

    fn set_mode(&mut self, mode: GameMode) {
        self.mode = mode;
        self.player.can_fly = mode.can_fly();
        self.player.noclip = mode == GameMode::Spectator;
        if mode == GameMode::Spectator {
            self.player.flying = true;
        }
        if !self.player.can_fly {
            self.player.flying = false;
        }
        self.actions.reset();
        self.show_popup(&format!("{} mode", capitalize(mode.name())));
    }

    /// Open or close the inventory, clearing queued input on entry and returning crafting stacks on exit.
    fn toggle_inventory(&mut self) {
        if !self.inventory_open && !self.mode.can_interact() {
            return;
        }
        self.inventory_open = !self.inventory_open;
        if self.inventory_open {
            self.search.focused = self.mode == GameMode::Creative && self.container == Container::Inventory;
            self.search.selected = false;
            self.set_grab(false);
            self.keys.clear();
            self.left_held = false;
            self.right_held = false;
            self.jump_pressed = false;
            self.mine_pressed = false;
            self.actions.reset();
        } else {
            if let Container::Chest(pos) = self.container {
                self.chest_sound(pos, 0.8);
            }
            let table = self.take_work();
            self.inventory.return_stacks(self.craft.take_all().into_iter().chain(table));
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

    /// Right-click on a brewing stand: its bottles, ingredient and fuel.
    fn open_brewing(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.brewing_stand(pos).is_none() {
            return;
        }
        self.container = Container::Brewing(pos);
        self.toggle_inventory();
    }

    /// A click on a brewing stand slot: bottles take one potion or glass
    /// bottle each, the ingredient slot only brewing ingredients and the
    /// fuel slot only blaze powder (Java's slot rules).
    fn brewing_click(&mut self, pos: IVec3, slot: hud::SlotRef, right: bool) {
        use crate::world::brewing;
        let cursor = &mut self.inventory.cursor;
        let Some(b) = self.world.brewing_stand_mut(pos) else { return };
        match slot {
            hud::SlotRef::BrewBottle(i) => {
                let cell = &mut b.bottles[i];
                match cursor {
                    None => *cursor = cell.take(),
                    Some(c) if !brewing::fits_bottle_slot(c.item) => {}
                    Some(c) if cell.is_none() => {
                        *cell = Some(Stack::new(c.item, 1));
                        c.count -= 1;
                        if c.count == 0 {
                            *cursor = None;
                        }
                    }
                    Some(c) if c.count == 1 => std::mem::swap(cell, cursor),
                    Some(_) => {}
                }
            }
            hud::SlotRef::BrewIngredient if cursor.is_none_or(|c| brewing::is_ingredient(c.item)) => {
                crate::inventory::click_slot(&mut b.ingredient, cursor, right)
            }
            hud::SlotRef::BrewFuel if cursor.is_none_or(|c| c.item == Item::BLAZE_POWDER) => {
                crate::inventory::click_slot(&mut b.fuel, cursor, right)
            }
            _ => {}
        }
    }

    /// A click on a furnace slot. Fuel only takes things that burn; the
    /// output can only be taken from.
    fn furnace_click(&mut self, pos: IVec3, slot: hud::SlotRef, right: bool) {
        let cursor = &mut self.inventory.cursor;
        let Some(f) = self.world.furnace_mut(pos) else { return };
        let before = f.output.map_or(0, |s| s.count);
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
        self.award_furnace_xp(pos, before);
    }

    /// Taking any smelted items (the output held `before` items) releases
    /// all the experience the furnace stored, as orbs at the player, like
    /// Java's result slot.
    pub(super) fn award_furnace_xp(&mut self, pos: IVec3, before: u8) {
        let roll = self.mobs.entities.roll();
        let Some(xp) = self.world.furnace_mut(pos).and_then(|f| f.take_output_xp(before, roll)) else { return };
        self.mobs.entities.spawn_xp(self.player.pos, xp);
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
        if self.modifiers.shift_key() && self.inventory.cursor.is_none() {
            if let Some(slot) = slot {
                self.quick_move(slot);
            }
            return;
        }
        if let Some(slot) = slot {
            self.click_slot(slot, right);
        }
    }

    /// A left (or right) click on a slot of the open container screen.
    fn click_slot(&mut self, slot: hud::SlotRef, right: bool) {
        match Some(slot) {
            Some(hud::SlotRef::Inventory(i)) => self.inventory.click(i, right),
            Some(hud::SlotRef::Craft(i)) => {
                crate::inventory::click_slot(&mut self.craft.cells[i], &mut self.inventory.cursor, right)
            }
            Some(hud::SlotRef::CraftResult) => self.take_craft_result(),
            Some(hud::SlotRef::Armor(piece)) => self.inventory.click_armor(piece, right, self.mode.is_creative()),
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
            Some(s @ (hud::SlotRef::BrewBottle(_) | hud::SlotRef::BrewIngredient | hud::SlotRef::BrewFuel)) => {
                if let Container::Brewing(pos) = self.container {
                    self.brewing_click(pos, s, right);
                }
            }
            Some(s @ (hud::SlotRef::EnchantItem | hud::SlotRef::EnchantLapis | hud::SlotRef::EnchantOffer(_))) => {
                self.table_click(s, right)
            }
            Some(s @ (hud::SlotRef::AnvilLeft | hud::SlotRef::AnvilRight | hud::SlotRef::AnvilResult)) => {
                self.anvil_click(s, right)
            }
            Some(
                s @ (hud::SlotRef::SmithTemplate
                | hud::SlotRef::SmithBase
                | hud::SlotRef::SmithAddition
                | hud::SlotRef::SmithResult),
            ) => self.smithing_click(s, right),
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
        if self.mode.is_creative() {
            let max = hud::palette_rows(&self.search.query).saturating_sub(hud::PALETTE_ROWS);
            self.creative_scroll = self.creative_scroll.saturating_add_signed(rows as isize).min(max);
        }
    }

    fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.world.raycast(self.player.eye(), self.player.forward().as_dvec3(), REACH)
    }

    /// Instant break (creative).
    fn break_block(&mut self) {
        if !self.mode.can_build() || self.attacking() {
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
            if block.is_door() {
                self.break_door_partner(pos, block);
            }
        }
    }

    /// Sends the dragon egg at `pos` somewhere nearby, with a teleport sound.
    fn teleport_egg(&mut self, pos: glam::IVec3) {
        if let Some(to) = self.world.teleport_egg(pos) {
            self.audio.play(crate::audio::sounds::Sound::Teleport, Some(to.as_dvec3()), 0.6, (0.9, 1.1));
        }
    }

    /// Timed break with drops (survival). Called every frame while the
    /// button is held.
    fn continue_breaking(&mut self, dt: f64) {
        if !self.mode.can_build() || self.attacking() {
            self.actions.breaking = None;
            return;
        }
        let Some((pos, face)) = self.target() else {
            self.actions.breaking = None;
            return;
        };
        let Some(block) = self.world.get_block(pos) else { return };
        // The dragon egg won't be mined: it jumps away from the hit.
        if block == Block::DRAGON_EGG {
            self.actions.breaking = None;
            self.action_cooldown = BREAK_DELAY;
            self.teleport_egg(pos);
            return;
        }
        let held = self.held_item();
        let digger = crate::mining::Digger {
            held: self.inventory.get(self.actions.selected),
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(&self.world),
            on_ground: self.player.on_ground || self.player.flying,
        };
        let progress = self.actions.mine(pos, block, crate::mining::dig_time(block, digger), dt);
        if progress < 1.0 {
            self.audio.block_hit(block, pos, dt);
            self.world.particles.push(crate::particles::Request::Hit { cell: pos, block, face });
            return;
        }
        self.actions.breaking = None;
        self.action_cooldown = BREAK_DELAY;
        // Broken ice melts into water, unless it was floating over nothing
        // or silk touch keeps it whole.
        let tool = digger.held.map_or(Default::default(), |s| s.active_enchants());
        let melts = block == Block::ICE
            && !tool.has(crate::enchant::Enchantment::SilkTouch)
            && self.dimension.has_sky()
            && self.world.get_block(pos - glam::IVec3::Y).is_some_and(|b| b != Block::AIR);
        self.world.set_block(pos, if melts { Block::WATER } else { Block::AIR });
        if melts {
            self.world.particles.push(crate::particles::Request::Break { cell: pos, block });
        }
        self.audio.block_break(block, pos);
        // Stone, ores and the like only drop with a good enough pickaxe.
        if self.gamerules.bool("doTileDrops") && crate::mining::can_harvest(block, held) {
            self.world.spill_mined(pos, block, tool);
            self.mobs.entities.drop_mined_xp(block, pos, tool);
        }
        if block.is_bed() {
            self.break_bed_partner(pos, block);
        }
        if block.is_door() {
            self.break_door_partner(pos, block);
        }
        self.vitals.hunger.exhaust(survival::EXHAUST_MINE);
        if block.hardness() > 0.0 {
            self.wear_held(false);
        }
    }

    /// Eating: holding right-click with food in survival, when not full,
    /// finishes a bite after [`EAT_TIME`] seconds. Potions drink the same
    /// way, in any mode.
    fn eat(&mut self, acting: bool, dt: f64) {
        let potion = self.held_item().and_then(|i| i.as_potion());
        let food = self.held_item().and_then(|i| i.food());
        let hungry = self.mode.is_survival() && self.vitals.hunger.can_eat();
        let using = acting && self.right_held && (potion.is_some() || (food.is_some() && hungry));
        if !using {
            self.actions.eat_timer = 0.0;
            return;
        }
        let before = self.actions.eat_timer;
        let finished = self.actions.eat(dt);
        // Chewing sounds four times a second.
        if (before / 0.25).floor() != ((before + dt) / 0.25).floor() {
            let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
            self.audio.play(sound, Some(self.player.eye()), 0.7, (1.4, 1.7));
        }
        if !finished {
            return;
        }
        if let Some(potion) = potion {
            // Creative keeps the potion; survival is left with the bottle.
            if self.mode.is_survival() {
                self.inventory.slots[self.actions.selected] = Some(crate::inventory::Stack::new(Item::GLASS_BOTTLE, 1));
            }
            let damage = potion.drink(&mut self.vitals);
            self.damage_player(damage, survival::CAUSE_MAGIC);
        } else if let Some((hunger, saturation)) = food {
            let effect = self.held_item().and_then(Item::food_effect);
            self.inventory.take_one(self.actions.selected);
            self.vitals.hunger.eat(hunger, saturation);
            if let Some((effect, amp, ticks)) = effect {
                self.vitals.apply_effect(effect, amp, ticks);
            }
        }
    }

    /// Right-click with armor in hand puts it on (swapping out the worn
    /// piece), unless aimed at a container. Returns whether it did.
    fn equip_held(&mut self) -> bool {
        if !self.mode.is_survival() || self.aiming_at_usable() || !self.inventory.equip(self.actions.selected) {
            return false;
        }
        let sound = crate::audio::sounds::Sound::Place(crate::audio::sounds::Material::Wood);
        self.audio.play(sound, Some(self.player.eye()), 0.6, (1.4, 1.6));
        true
    }

    /// Torch light at a point, 0..1, for lighting things drawn outside the
    /// chunk meshes.
    pub(super) fn torch_light(&self, p: glam::DVec3) -> f32 {
        self.world.block_light(p.floor().as_ivec3()) as f32 / 15.0
    }

    /// The item in the selected hotbar slot.
    pub(super) fn held_item(&self) -> Option<Item> {
        self.inventory.get(self.actions.selected).map(|s| s.item)
    }

    /// Wears the held tool for a block broken or a mob hit (survival).
    pub(super) fn wear_held(&mut self, hitting_mob: bool) {
        let Some(held) = self.held_item() else { return };
        if self.mode.is_survival() && self.inventory.wear(self.actions.selected, crate::mining::wear(held, hitting_mob))
        {
            self.show_popup(&format!("{} broke", capitalize(held.name())));
            self.audio.play(
                crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
                Some(self.player.eye()),
                0.8,
                (1.3, 1.5),
            );
        }
    }

    fn place_block(&mut self) {
        if !self.mode.can_interact() {
            return;
        }
        if !self.aiming_at_usable() && (self.use_bucket() || self.throw_pearl() || self.throw_eye()) {
            return;
        }
        let Some((pos, normal)) = self.target() else { return };
        if self.insert_eye(pos) {
            return;
        }
        // Containers open on right-click; holding Shift builds against them.
        match self.world.get_block(pos) {
            _ if self.sneak_building() => {}
            // A controller player's own screens and bed open instead.
            Some(b)
                if self.puppet
                    && (b == Block::CRAFTING_TABLE
                        || b == Block::BREWING_STAND
                        || b == Block::ENCHANTING_TABLE
                        || b.is_anvil()
                        || b == Block::SMITHING_TABLE
                        || b.is_bed()
                        || crate::world::furnace::is_furnace(b)
                        || crate::world::chest::is_chest(b)) =>
            {
                self.puppet_used = Some(pos);
                return;
            }
            Some(Block::CRAFTING_TABLE) => return self.open_crafting_table(),
            Some(b) if crate::world::furnace::is_furnace(b) => return self.open_furnace(pos),
            Some(b) if crate::world::chest::is_chest(b) => return self.open_chest(pos),
            Some(Block::BREWING_STAND) => return self.open_brewing(pos),
            Some(Block::ENCHANTING_TABLE) => return self.open_enchanting(pos),
            Some(b) if b.is_anvil() => return self.open_anvil(pos),
            Some(Block::SMITHING_TABLE) => return self.open_smithing(pos),
            Some(Block::DRAGON_EGG) => return self.teleport_egg(pos),
            Some(b) if b.is_bed() => return self.use_bed(pos),
            Some(b) if b.is_door() || b.is_gate() => {
                self.toggle_door(pos, self.player.forward());
                return;
            }
            _ => {}
        }
        if !self.mode.can_build() {
            return;
        }
        if self.strike_flint(pos, normal) || self.use_item_on(pos, normal) {
            return;
        }
        // Clicking tall grass replaces it instead of building against it.
        let at = if self.world.get_block(pos).is_some_and(|b| b.is_replaceable()) { pos } else { pos + normal };
        if self.agents.positions().iter().any(|&p| Player::new(p).intersects_block(at)) {
            return;
        }
        let placed = match self.held_item() {
            Some(Item::BED) => Some(self.place_bed(at)),
            Some(i)
                if i == Item::OAK_DOOR
                    || i.block().is_some_and(|b| {
                        matches!(crate::world::forms::wood_form(b.0), Some(crate::world::forms::WoodForm::Door { .. }))
                    }) =>
            {
                Some(self.place_door(at, i))
            }
            Some(i) if i.block().is_some_and(|b| b.is_ladder()) => Some(self.place_ladder(pos, normal)),
            _ => None,
        };
        if let Some(placed) = placed {
            if placed && self.mode.is_survival() {
                self.inventory.take_one(self.actions.selected);
            }
            return;
        }
        let Some(block) = self.inventory.get(self.actions.selected).and_then(|s| s.item.places()) else { return };
        // A slab on top of the same slab makes the full block.
        if block.is_slab()
            && normal == IVec3::Y
            && self.world.get_block(pos) == Some(block)
            && let Some(full) = block.slab_base()
        {
            if !self.player.intersects_block(pos) {
                self.world.set_block(pos, full);
                self.audio.block_place(full, pos);
                if self.mode.is_survival() {
                    self.inventory.take_one(self.actions.selected);
                }
            }
            return;
        }
        // Furnaces and chests face whoever places them.
        let block = crate::world::nether_blocks::placed(block, normal)
            .with_facing(crate::world::block::Facing::toward(self.player.forward()));
        if block.is_water() && self.dimension == Dimension::Nether {
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
            if self.mode.is_survival() {
                self.inventory.take_one(self.actions.selected);
            }
        }
    }

    fn pick_block(&mut self) {
        let Some(b) = self.target().and_then(|(pos, _)| self.world.get_block(pos)) else { return };
        if b.is_fire() {
            return;
        }
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
        let ahead = self.player.pos + self.player.forward().as_dvec3().with_y(0.0).normalize_or_zero() * 4.0;
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
                Some(Block::SMITHING_TABLE) => self.open_smithing(p),
                _ => log::warn!("--open-block {p}: no container there"),
            }
        }
        self.spawn_pending_mobs();
        // Seated once the ground it stands on is in place.
        if let Some(screen) = self.virtual_pad.take() {
            self.virtual_pad(&screen);
        }
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
        self.player.yaw += std::f32::consts::TAU / FRAMES as f32;
        self.frame_started = Some(Instant::now());
        false
    }

    fn save(&mut self) {
        let mut props = std::collections::BTreeMap::new();
        props.insert("agents".into(), self.agents.serialize(self.dimension.name()));
        props.insert("pads".into(), self.pads.serialize());
        props.insert("mode".to_string(), self.mode.name().to_string());
        props.insert("difficulty".to_string(), self.difficulty.name().to_string());
        props.insert("hardcore".to_string(), self.hardcore.to_string());
        props.insert("gamerules".to_string(), self.gamerules.serialize());
        props.insert(
            "world_spawn".to_string(),
            format!("{},{},{}", self.world_spawn.x, self.world_spawn.y, self.world_spawn.z),
        );
        props.insert("name".to_string(), self.world_name.clone());
        // Save what's held or on the crafting grid as if the screen closed.
        let mut inventory = self.inventory.clone();
        inventory.return_stacks(self.craft.cells.iter().chain(&self.work).flatten().copied());
        props.insert("inventory".to_string(), inventory.serialize());
        props.insert("health".to_string(), self.vitals.health.to_string());
        props.insert("air".to_string(), format!("{:.2}", self.vitals.air));
        props.insert("xp".to_string(), self.vitals.xp.serialize());
        props.insert("effects".to_string(), self.vitals.effects.serialize());
        let h = self.vitals.hunger;
        props.insert("hunger".to_string(), format!("{:.2},{:.2},{:.3}", h.food, h.saturation, h.exhaustion));
        if let Some(cause) = &self.vitals.death {
            props.insert("death".to_string(), cause.clone());
        }
        props.insert("dimension".to_string(), self.dimension.name().to_string());
        props.insert("time".to_string(), format!("{:.5}", self.day_time));
        props.insert("day_count".to_string(), self.day_count.to_string());
        props.insert("weather".to_string(), self.weather.serialize());
        if self.credits_seen {
            props.insert("credits_seen".to_string(), "1".to_string());
        }
        if let Some(b) = self.spawn_bed {
            props.insert("bed".to_string(), format!("{},{},{}", b.x, b.y, b.z));
        }
        if let Some(p) = self.spawn_point {
            props.insert("spawn_point".to_string(), format!("{},{},{}", p.x, p.y, p.z));
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
            Dimension::Nether | Dimension::End => {
                let nether = LevelInfo { seed, player: None, props: self.dimension_props() };
                dimension::storage_for(&self.storage, self.dimension).save(&nether, &chunks).and_then(|()| {
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

    /// Sample held keys and a queued jump tap for a gameplay tick; return neutral input while control is blocked.
    fn movement_input(&self, arriving: bool) -> MoveInput {
        let held = |k: KeyCode| self.keys.contains(&k);
        let axis = |pos: KeyCode, neg: KeyCode| held(pos) as i32 as f64 - held(neg) as i32 as f64;
        if self.menu.is_none()
            && !self.console.open
            && self.mouse_grabbed
            && !self.inventory_open
            && !self.vitals.is_dead()
            && self.sleeping.is_none()
            && !arriving
        {
            MoveInput {
                forward: axis(KeyCode::KeyW, KeyCode::KeyS),
                right: axis(KeyCode::KeyD, KeyCode::KeyA),
                jump: held(KeyCode::Space) || self.jump_pressed,
                descend: held(KeyCode::ShiftLeft),
                // Too hungry to sprint at 6 food or less (survival).
                sprint: (held(KeyCode::ControlLeft) || held(KeyCode::KeyR))
                    && (self.mode.invulnerable() || self.vitals.hunger.can_sprint()),
            }
        } else {
            MoveInput::default()
        }
    }

    /// Attack/mine button (left click, or a controller's RT) pressed or released.
    fn attack_button(&mut self, pressed: bool) {
        if !self.mode.can_interact() {
            return;
        }
        self.left_held = pressed;
        if pressed {
            self.hand.swing();
        }
        if !pressed {
            self.actions.breaking = None;
            self.release_attack();
        } else if self.attack() {
            self.actions.breaking = None;
        } else if self.mode == GameMode::Creative {
            self.break_block();
            self.action_cooldown = ACTION_REPEAT;
        } else {
            self.mine_pressed = true;
        }
    }

    /// Use button (right click, or a controller's LT) pressed or released.
    fn use_button(&mut self, pressed: bool) {
        if !self.mode.can_interact() {
            return;
        }
        self.right_held = pressed;
        if pressed && !self.equip_held() && !self.start_draw() {
            if self.held_item().is_none_or(|i| i.food().is_none()) {
                self.hand.swing();
            }
            self.place_block();
            self.action_cooldown = ACTION_REPEAT;
        } else if !pressed {
            self.actions.eat_timer = 0.0;
            self.release_bow();
        }
    }

    /// Held attack/use buttons for one tick: mining, repeated breaking or
    /// placing, eating and drawing a bow.
    fn act(&mut self, acting: bool, dt: f64) {
        let mine_pressed = std::mem::take(&mut self.mine_pressed);
        self.action_cooldown -= dt;
        if acting && (self.left_held || mine_pressed) && self.mode.is_survival() {
            if self.action_cooldown <= 0.0 {
                self.continue_breaking(dt);
            }
            if self.actions.breaking.is_some() {
                self.hand.swing();
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
            self.hand.swing();
            self.action_cooldown = ACTION_REPEAT;
        }
        if !self.left_held {
            self.actions.breaking = None;
        }
        self.eat(acting, dt);
        self.update_bow(acting, dt);
    }

    /// One fixed gameplay step. Rendering and worker polling never
    /// change the amount of simulation time advanced here.
    fn tick(&mut self) {
        self.world.set_tile_drops(self.gamerules.bool("doTileDrops"));
        let dt = crate::simulation::TICK_SECONDS;
        let arriving = self.update_arrival();
        let input = self.movement_input(arriving);
        self.jump_pressed = false;
        self.previous_eye = self.player.eye();
        let before = self.player.pos;
        if !arriving {
            self.player.apply_effects(&self.vitals.effects);
            let armor = &self.inventory.armor;
            self.player.wear_boots(crate::enchant::armor_level(armor, crate::enchant::Enchantment::DepthStrider));
            self.player.update(dt, input, &self.world);
            crate::particles::water_entry(&self.player, before, &mut self.world);
            self.update_portal(dt);
        }
        let moved = (self.player.pos - before).with_y(0.0).length();
        if self.gamerules.bool("doWeatherCycle") {
            self.weather.update(dt);
        }
        self.update_sleep(dt);
        self.world.raining = self.weather.raining && self.dimension.has_sky();
        let env = crate::simulation::survival::Env {
            respiration: crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::Respiration),
            frost_walker: crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::FrostWalker)
                > 0,
            ..crate::simulation::player_environment(&self.player, &self.world, input, moved)
        };
        let hurts = if arriving || self.arrival.is_some() {
            Default::default()
        } else {
            self.vitals.tick_rules(
                dt as f32,
                &env,
                self.mode.invulnerable(),
                self.difficulty,
                self.gamerules.bool("naturalRegeneration"),
            )
        };
        self.trample(hurts.landed);
        if hurts.fall > 0.0 && self.gamerules.bool("fallDamage") {
            self.damage_player(hurts.fall, survival::CAUSE_FALL);
        }
        if hurts.drown > 0.0 && self.gamerules.bool("drowningDamage") {
            self.damage_player(hurts.drown, survival::CAUSE_DROWN);
        }
        if hurts.lava > 0.0 && self.gamerules.bool("fireDamage") {
            self.damage_player_armored(hurts.lava, survival::CAUSE_LAVA);
        }
        if hurts.fire > 0.0 && self.gamerules.bool("fireDamage") {
            self.damage_player_armored(hurts.fire, survival::CAUSE_FIRE);
        }
        if hurts.burn > 0.0 && self.gamerules.bool("fireDamage") {
            self.damage_player(hurts.burn, survival::CAUSE_FIRE);
        }
        if hurts.starve > 0.0 {
            self.damage_player(hurts.starve, survival::CAUSE_STARVE);
        }

        let acting = self.menu.is_none() && !self.console.open && self.mouse_grabbed && !self.inventory_open;
        self.act(acting, dt);
        self.drive_pads(dt);
        self.tick_agents();
        crate::simulation::tick_world_rules(
            &mut self.world,
            self.player.pos,
            self.gamerules.bool("doFireTick"),
            self.gamerules.int("randomTickSpeed") as u32,
        );
        self.update_mobs(dt);
        self.update_items();
        self.tick_particles();
        if self.gamerules.bool("doDaylightCycle") {
            self.day_time += dt / DAY_LENGTH;
            if self.day_time >= 1.0 {
                self.day_time = self.day_time.fract();
                self.day_count = self.day_count.saturating_add(1);
            }
        }
    }

    /// Poll streaming, run due fixed gameplay ticks and render interpolated positions.
    /// Offline pause keeps streaming active and snaps interpolation to the current state.
    fn frame(&mut self) {
        let now = Instant::now();
        self.poll_agents();
        let paused = (self.menu.is_some() || self.console.open || self.credits.is_some()) && self.agents.host.is_none();
        let elapsed = now - self.last_frame;
        self.last_frame = now;
        let ticks = self.clock.advance(elapsed, paused);
        let dt = if paused { 0.0 } else { elapsed.as_secs_f64().min(0.25) };
        self.update_credits(elapsed.as_secs_f32().min(0.25));
        self.poll_pads(dt as f32, paused);

        // Streaming and GPU uploads continue during offline pause.
        self.update_arrival();
        let viewers: Vec<DVec3> = self.followed().map(|(_, b)| b.agent.player.pos).collect();
        self.world.set_viewers(&viewers);
        self.world.update_players(self.player.pos, &self.agents.positions());
        if !self.placed && self.screenshot.is_none() && self.world.pending_jobs() == 0 && self.world.loaded_chunks() > 0
        {
            self.apply_placements();
        }
        for _ in 0..ticks {
            self.tick();
        }
        if paused {
            self.previous_eye = self.player.eye();
            self.mobs.entities.snapshot_positions();
            self.world.snapshot_falling_positions();
        }
        let input = self.movement_input(self.arrival.is_some());
        let rain_here = weather::rain_at(&self.world, &self.weather, self.player.pos);
        // `followed()`, spelled out so `audio` can be borrowed mutably.
        let others: Vec<_> = (self.split.follow.iter())
            .filter_map(|name| self.agents.players.get(name))
            .filter(|b| b.active)
            .take(split::MAX_VIEWS - 1)
            .map(|b| (&b.agent.player, weather::rain_at(&self.world, &self.weather, b.agent.player.pos)))
            .collect();
        let dragon_music = self.mobs.entities.fight.as_ref().is_some_and(|f| f.boss_bar(self.player.pos).is_some());
        self.audio.update_music(
            &self.player,
            &self.world,
            self.mode == GameMode::Creative,
            dragon_music,
            self.credits.is_some(),
        );
        self.audio.update(&self.player, &self.world, rain_here, &others, dt);
        self.agent_sounds();
        let alpha = if paused { 1.0 } else { self.clock.alpha() };
        let eye = crate::simulation::interpolated_eye(self.previous_eye, self.player.eye(), alpha);
        self.rendered_eye = eye;
        let (camera, forward) = self.camera.view(&self.world, eye, self.player.forward());
        self.hand.update(dt as f32, self.held_item());
        self.animate_hands(dt as f32, alpha, paused);
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
        let mut sky = sky_state(self.day_time);
        sky.daylight = self.weather.dim(sky.daylight);
        sky.horizon = self.weather.overcast(sky.horizon);
        sky.zenith = self.weather.overcast(sky.zenith);
        let nether = !self.dimension.has_sky();
        if nether {
            // No sun, no weather: a steady dim glow in a red haze.
            sky.daylight = if self.dimension == Dimension::End { 0.65 } else { dimension::NETHER_LIGHT };
            sky.horizon = if self.dimension == Dimension::End { [0.045, 0.025, 0.065] } else { dimension::NETHER_FOG };
            sky.zenith = if self.dimension == Dimension::End { [0.018, 0.009, 0.03] } else { dimension::NETHER_FOG };
        }
        let scene = split::Scene {
            sky,
            rain: if nether { 0.0 } else { self.weather.strength },
            time: (now - self.started).as_secs_f32(),
            alpha,
            now,
        };
        let split::Fog { color: fog_color, start: fog_start, end: fog_end, underwater } = self.fog(&scene, camera);
        let others: Vec<(&Player, DVec3, crate::entity::model::PlayerAppearance)> = self
            .agents
            .players
            .values()
            .filter(|b| b.active && (!b.agent.vitals.is_dead() || b.agent.vitals.since_damage() < 1.0))
            .map(|b| {
                (
                    &b.agent.player,
                    b.agent.previous_pos.lerp(b.agent.player.pos, alpha),
                    b.hand.appearance(&b.agent.vitals, b.agent.eating(), alpha, b.agent.inventory.armor),
                )
            })
            .collect();
        let verts = self.mobs.entities.mesh(camera, forward, fog_end, scene.time, alpha);
        push_avatars(&self.world, &others, camera, fog_end, scene.time, verts);
        if !self.camera.first_person() {
            crate::entity::model::build_player(
                &self.player,
                eye - (self.player.eye() - self.player.pos),
                camera,
                (
                    crate::entity::sky_light(&self.world, eye),
                    self.world.block_light(eye.floor().as_ivec3()) as f32 / 15.0,
                ),
                scene.time,
                self.hand.appearance(
                    &self.vitals,
                    (self.actions.eat_timer / EAT_TIME) as f32,
                    alpha,
                    self.inventory.armor,
                ),
                verts,
            );
        }
        self.renderer.set_entities(verts);
        self.renderer.set_particles(&self.particles.pool, &self.world, camera, self.player.forward(), alpha);
        weather::sheets(&self.world, camera, scene.rain, &mut self.weather_verts);
        self.renderer.set_weather(&self.weather_verts);
        let viewports = self.viewports();
        let params = FrameParams {
            camera,
            forward,
            view_effect: voxelcraft::camera::view_effect(
                &self.player,
                &self.vitals,
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
            crack: self
                .actions
                .breaking
                .map(|(p, progress)| (p, crate::world::block::tex::CRACK_0 + (progress * 10.0).min(9.0) as u16)),
            block_models: self.block_models(alpha),
            hand: (self.camera.first_person() && self.show_hud && !self.vitals.is_dead() && self.sleeping.is_none())
                .then(|| {
                    let eye = self.player.eye();
                    let eating = (self.actions.eat_timer / EAT_TIME) as f32;
                    self.hand.view(eating, crate::entity::sky_light(&self.world, eye), self.torch_light(eye))
                }),
            rain: scene.rain,
            night_vision: self.vitals.effects.night_vision(scene.time),
            ui: if self.show_hud
                || self.vitals.is_dead()
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
            .chain(self.item_models(alpha))
            .collect()
    }
}

/// Adds player models (feet at the given interpolated positions) within
/// fog range of `camera`.
fn push_avatars(
    world: &World,
    players: &[(&Player, DVec3, crate::entity::model::PlayerAppearance)],
    camera: DVec3,
    fog_end: f32,
    time: f32,
    verts: &mut Vec<crate::entity::model::EntityVertex>,
) {
    for &(player, feet, appearance) in players {
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
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
