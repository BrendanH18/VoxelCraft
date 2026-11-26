//! Window, input and the per-frame game loop.

mod actions;
mod agents;
mod anvil;
mod banners;
mod beacon;
mod bed;
mod benchmark;
mod books;
mod bow;
mod bucket;
mod conduit;
mod console;
mod containers;
mod crafter;
mod creative;
mod credits;
mod death;
mod dimension;
mod distortion;
mod doors;
mod enchanting;
mod farming;
mod flight;
mod frame_limit;
mod fullscreen;
mod gamepad;
mod gui;
mod hand;
mod hands;
mod hud;
mod items;
mod keys;
mod lan;
mod menu;
mod mobs;
mod options;
mod pad_menu;
mod particles;
mod patterns;
mod player_actions;
mod preview;
mod raid;
mod recipe_book;
mod save;
mod screen;
mod search;
mod settings;
mod sign_edit;
mod smithing;
mod split;
mod startup;
mod tick;
mod trading;
mod trial;
mod utilities;
mod workstations;
pub use crate::simulation::survival;
pub use voxelcraft::rules::GameMode;
mod title;
mod weather;
mod wither;
mod world_icon;

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
use crate::world::terrain::{Dimension, Generation, Generator};
use voxelcraft::rules::GameRules;

use survival::Vitals;

/// Placement/use repeat; mining timing is owned by the shared session.
const ACTION_REPEAT: f64 = 0.22;
const AUTOSAVE_EVERY: Duration = Duration::from_secs(120);
/// Light [`World::update_players`] may resolve per frame (v0.18 Track 10).
const STREAM_LIGHT_BUDGET: Duration = Duration::from_millis(2);
/// How long `--screenshot` waits, once streaming settles, for fluids to
/// come to rest. Placed water settles well inside it; generation 2's
/// springs can pour for minutes and would otherwise hold a capture open.
const SCREENSHOT_FLUID_WAIT: Duration = Duration::from_secs(5);
/// Seconds of holding right-click to eat (Minecraft's 32 ticks).
const EAT_TIME: f64 = 1.6;
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// Horizon colour at noon (also the fog colour).
const SKY: [f32; 3] = [0.42, 0.62, 0.98];
const ZENITH: [f32; 3] = [0.10, 0.27, 0.80];
const NIGHT_SKY: [f32; 3] = [0.008, 0.012, 0.035];
const NIGHT_ZENITH: [f32; 3] = [0.002, 0.003, 0.012];
const SUNSET: [f32; 3] = [0.95, 0.42, 0.18];

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
    /// The player's own ender inventory, opened at the ender chest in this
    /// cell (controller screens; the keyboard player's is lent to `Chest`).
    EnderChest(IVec3),
    Minecart(u32),
    Brewing(IVec3),
    Enchanting(IVec3),
    Anvil(IVec3),
    Grindstone(IVec3),
    Trading(u64),
    Smithing(IVec3),
    /// v0.15 Track B workstations.
    Stonecutter(IVec3),
    Loom(IVec3),
    Cartography(IVec3),
    /// v0.16 Track C: a beacon's power screen.
    Beacon(IVec3),
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
    /// Keys and mouse buttons held down during play (the binding table
    /// looks actions up in it).
    keys: FxHashSet<keys::Input>,
    /// Sneak and Sprint while their Controls option is Toggle.
    sneak_toggled: bool,
    sprint_toggled: bool,
    /// An immediate respawn waits for the step that killed the player
    /// (a blast, a save's settling) to finish; the next frame runs it, as
    /// Java's client sends its respawn request after the death.
    respawn_due: bool,
    /// Java's `ScrollWheelHandler`: wheel movement short of a whole notch.
    scroll: f64,
    frame_limit: frame_limit::FrameLimit,
    /// Nausea and portal warp (Distortion Effects).
    distortion: distortion::Distortion,
    /// Shift / Ctrl state (shift-click, Ctrl+Q, sneak-placing).
    modifiers: winit::keyboard::ModifiersState,
    mouse_grabbed: bool,
    /// Preserve taps that begin and end between fixed game ticks.
    jump_pressed: bool,
    /// Shared world difficulty (old saves default to Normal).
    difficulty: Difficulty,
    /// World-wide one-life flag; locks difficulty to Hard.
    hardcore: bool,
    /// Java's `DifficultyLocked`: the Options screen's lock was used.
    difficulty_locked: bool,
    gamerules: GameRules,
    inventory_open: bool,
    /// Crafting grid of the open screen: 2x2 in the inventory, 3x3 at a
    /// crafting table. Emptied back into the inventory when it closes.
    craft: crate::crafting::Grid,
    /// v0.18 Track 5: the recipe book's view (tab, page, search, ghost, toasts).
    recipe_book: recipe_book::View,
    /// v0.18 Track 4: the creative screen's tab, scroll and scrollbar drag.
    creative: creative::State,
    /// v0.18 Track 4: saved hotbars (`HotbarManager`), per install.
    hotbars: voxelcraft::creative::Hotbars,
    /// `saves/hotbars.txt`; `None` in scripted runs, which save nothing.
    hotbars_path: Option<std::path::PathBuf>,
    /// Rows scrolled in the stonecutter's recipe list or the loom's
    /// pattern list (v0.15 Track B).
    work_scroll: usize,
    /// Mouse position in physical pixels (for the inventory screen).
    cursor_px: (f32, f32),
    /// The host player's hands and open screen.
    body: gamepad::Body,
    show_hud: bool,
    /// v0.18 Track 9: this frame's captions (Closed Captions).
    captions: Vec<crate::audio::subtitles::Shown>,
    hand: hand::HandAnim,
    last_space: Instant,
    last_frame: Instant,
    clock: crate::simulation::FixedClock,
    previous_eye: DVec3,
    rendered_eye: DVec3,
    /// Each split-screen view's atmosphere state (v0.18 Track 8): the
    /// biome blend Java's fog reads, its rain fog and water vision.
    atmosphere_views: std::cell::RefCell<Vec<split::ViewAtmosphere>>,
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
    /// `--animation-time`: the animation clock stays here instead of running.
    animation_time: Option<f32>,
    /// Compass and clock needles wobbling outside the Overworld.
    dial_wobble: std::cell::Cell<(crate::item::Wobbler, crate::item::Wobbler)>,
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
    /// Why the last save failed, until it succeeds again.
    save_error: Option<String>,
    screenshot: Option<String>,
    /// v0.18 Track 7: the world icon's capture (`None` for LAN clients and
    /// scripted runs without `--world-icon`).
    world_icon: Option<world_icon::WorldIcon>,
    /// v0.18 Track 7: `--panorama DIR` and the face being rendered.
    panorama_out: Option<std::path::PathBuf>,
    panorama_face: Option<usize>,
    screenshot_state: u32,
    /// When `--screenshot` first found streaming settled (see [`SCREENSHOT_FLUID_WAIT`]).
    screenshot_settled: Option<Instant>,
    open_trading: bool,
    place: Vec<(glam::IVec3, Block)>,
    /// `--sign` front texts, written after `--place`.
    signs: Vec<(glam::IVec3, [String; 4])>,
    carts: Vec<(crate::entity::minecart::CartKind, glam::IVec3, bool)>,
    /// v0.17 Track D1: ticks counted for the minecart sound stretches.
    cart_sound_tick: u32,
    decor_scene: Vec<(IVec3, IVec3, Item, Option<Item>)>,
    /// `--brush`: strokes on suspicious blocks, after `--place`.
    brush_scene: Vec<(IVec3, IVec3, u8)>,
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
    /// Open menu screens (the game is paused while one is up).
    menu: gui::Stack<Game>,
    /// What a controller asked the menus for during the frame, for the app.
    menu_action: Option<menu::MenuAction>,
    /// v0.18 Track 2: the death screen and its quit confirmation, apart
    /// from `menu` because they don't pause the game.
    death: gui::Stack<Game>,
    /// When a LAN guest last asked the host to respawn it (the death
    /// screen stays closed meanwhile).
    death_respawn_sent: Option<Instant>,
    /// v0.18 Track 2: the anvil screen's name box.
    anvil_name: anvil::NameBox,
    /// The window has had keyboard focus at some point (losing focus only
    /// pauses after that, not when the game starts in the background).
    had_focus: bool,
    settings: settings::Settings,
    /// Where options are saved; `None` for scripted runs (screenshots,
    /// benchmarks), which neither read nor write them.
    settings_path: Option<std::path::PathBuf>,
    /// Carried through a world for the title screen.
    updates: crate::update::Updater,
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
            title.show_select();
            let what = if e.kind() == std::io::ErrorKind::WouldBlock {
                format!("\"{world_dir}\" is open in another VoxelCraft window. Close it there, then try again.")
            } else {
                format!("Can't open \"{world_dir}\": {e}. Check that the saves folder can be written, then try again.")
            };
            title.world_error("Couldn't open the world", what);
            self.title = Some(title);
            return;
        }
        let replacing = new.is_some() || self.args.new_world || self.args.join.is_some();
        if let Some(e) = unreadable_level(&Storage::new(self.saves_dir.join(world_dir)), replacing) {
            log::error!("cannot open world '{world_dir}': {e}");
            if scripted {
                // Screenshots and benchmarks have no one to read the title screen.
                eprintln!("cannot open world '{world_dir}': {e}");
                std::process::exit(1);
            }
            let mut title = title::Title::new(shell, &self.saves_dir);
            title.show_select();
            title.world_error(
                "Couldn't open the world",
                format!(
                    "Can't open \"{world_dir}\": {e}. Nothing was changed. Fix its level.txt, or restore the world \
                     from the backups folder."
                ),
            );
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
        let options_error = game.save_settings().err();
        game.close_lan("Server closed");
        let save_error = game.save_error.take();
        let shell = game.into_shell();
        let mut title = title::Title::new(shell, &self.saves_dir);
        if let Some(reason) = options_error {
            title.options_error(reason);
        }
        if let Some(e) = save_error {
            title.world_error(
                "Couldn't save the world",
                format!(
                    "Could not save the world: {e}. Recent changes may be lost. Check for free disk space and that \
                     the saves folder can be written, then open the world again."
                ),
            );
        }
        self.title = Some(title);
    }
}

/// Why an existing world can't be opened, if its level file is unreadable.
/// Opening it anyway would start a fresh world over the old one.
fn unreadable_level(storage: &Storage, replacing: bool) -> Option<String> {
    if replacing || !storage.exists() {
        return None;
    }
    storage.load_level().err().map(|e| format!("its level file is unreadable ({e})"))
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
    /// The in-game updater; its check runs once per launch.
    updates: crate::update::Updater,
    /// Controller input: menu navigation on the title screen, then
    /// split-screen players and menus in a world.
    gilrs: Option<gilrs::Gilrs>,
}

impl Shell {
    /// Pushes the options the title screen can change to the renderer and audio.
    fn apply_settings(&mut self) {
        let s = self.settings;
        if self.renderer.vsync() != s.vsync {
            self.renderer.set_vsync(s.vsync);
        }
        self.renderer.set_brightness(s.brightness);
        self.renderer.set_mipmap_levels(s.mipmap_levels);
        self.audio.set_volumes(|source| s.sound_volume(source));
        self.audio.set_subtitles(s.show_subtitles);
    }
}

/// The command-line options that override saved options (or, in scripted
/// runs, the defaults) for this session only: they are never saved.
fn apply_session_overrides(settings: &mut settings::Settings, args: &Args) {
    if let Some(rd) = args.render_distance {
        settings.render_distance = rd;
    }
    if let Some(v) = args.volume {
        settings.volume = v;
    }
    settings.vsync &= !args.no_vsync && args.benchmark_scene.is_none();
    if let Some(enhanced) = args.enhanced_graphics {
        settings.enhanced_graphics = enhanced;
    }
    // `--gui-scale`: scripted captures read no options file, so this is how
    // they pick a scale; `Ui::gui_scale` still caps it to the window.
    if let Some(scale) = args.gui_scale {
        settings.gui_scale = scale;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() || self.title.is_some() {
            return;
        }
        let benchmark = self.args.benchmark_scene.is_some();
        let size = self.args.physical_size.unwrap_or(if benchmark { (1920, 1080) } else { (1600, 900) });
        let attrs = Window::default_attributes()
            .with_title("VoxelCraft")
            .with_window_icon(Some(window_icon()))
            .with_inner_size(PhysicalSize::new(size.0, size.1));
        let window_mode = fullscreen::Mode::from_benchmark(self.args.benchmark_window_mode);
        // A benchmark run is invalid once its window is covered, and one
        // started from a terminal or agent opens behind the active app. A
        // fullscreen run has the screen to itself.
        let attrs = if benchmark && window_mode == fullscreen::Mode::Off {
            attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop).with_active(true)
        } else {
            attrs
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        if self.args.benchmark_scene.is_some() {
            window.focus_window();
        }

        // Saved options, with command-line overrides for this session.
        let scripted = self.args.screenshot.is_some() || self.args.bench_render || self.args.benchmark_scene.is_some();
        let settings_path = (!scripted).then(|| self.saves_dir.join("options.txt"));
        let mut settings = settings_path.as_deref().map(settings::Settings::load).unwrap_or_default();
        apply_session_overrides(&mut settings, &self.args);

        if !scripted {
            settings.fullscreen.apply(&window);
        } else if benchmark {
            // Scripted runs save no options, so this lasts for the run, and
            // menus that reapply the option keep it.
            settings.fullscreen = window_mode;
            window_mode.apply_preferring(&window, Some(size));
        }
        let options = if benchmark {
            crate::render::Options {
                frame_latency: self.args.benchmark_frame_latency,
                gpu_timestamps: self.args.benchmark_gpu_timestamps,
            }
        } else {
            Default::default()
        };
        let mut renderer = pollster::block_on(Renderer::new(window, settings.vsync, options));
        renderer.force_offscreen = self.args.benchmark_scene.is_some() && self.args.benchmark_offscreen;
        let mut audio = crate::audio::Audio::new(self.args.mute, settings.volume);
        audio.set_volumes(|source| settings.sound_volume(source));
        audio.set_subtitles(settings.show_subtitles);
        let root = self.saves_dir.parent().unwrap_or(&self.saves_dir);
        let source = match &self.args.update_source {
            Some(dir) => crate::update::Source::Folder(dir.clone()),
            None => crate::update::Source::GitHub,
        };
        // Isolated data folders (screenshots, tests) check only a fixture.
        let check = !scripted
            && !self.args.no_update_check
            && settings.check_updates
            && (self.args.data_dir.is_none() || self.args.update_source.is_some());
        let mut updates = crate::update::Updater::new(root, source, check);
        updates.auto = self.args.update_now;
        // A restart after updating must not replay one-shot options such as
        // `--new`, which would replace a world.
        if self.args.data_dir.is_some() {
            updates.restart_args.extend(["--data-dir".into(), root.as_os_str().to_owned()]);
        }
        if let Some(dir) = &self.args.update_source {
            updates.restart_args.extend(["--update-source".into(), dir.as_os_str().to_owned()]);
        }
        if !scripted {
            // The window and renderer work: tell a pending update it started.
            updates.after_start();
        }
        let gilrs = if self.args.screenshot.is_none() { gamepad::Pads::open() } else { None };
        let shell = Shell { renderer, audio, settings, settings_path, updates, gilrs };
        // A named world, a fresh one or a scripted run skips the title screen.
        let to_title = self.args.open_menu.as_deref().is_some_and(|m| title::TITLE_MENUS.contains(&m))
            || self.args.benchmark_scene.as_deref() == Some("title");
        if let Some(address) = self.args.join.clone() {
            let mut title = title::Title::new(shell, &self.saves_dir);
            title.connect_address(address, self.args.profile.clone());
            self.title = Some(title);
            return;
        }
        match self.args.world.clone() {
            // A world saved by a newer version asks first, as from the list.
            Some(world)
                if !to_title && !scripted && !self.args.new_world && title::saved_by_newer(&self.saves_dir, &world) =>
            {
                let mut title = title::Title::new(shell, &self.saves_dir);
                match title.confirm_open(&world) {
                    Some(title::Action::Play(dir, new)) => self.play(title.into_shell(), &dir, new),
                    _ => self.title = Some(title),
                }
            }
            Some(world) if !to_title => self.play(shell, &world, None),
            None if !to_title && (self.args.new_world || scripted) => self.play(shell, "world", None),
            _ => {
                let mut title = title::Title::new(shell, &self.saves_dir);
                if let Some(menu) = self.args.open_menu.as_deref() {
                    title.open_menu(menu, self.args.world.as_deref());
                }
                title.screenshot = self.args.screenshot.take();
                title.benchmark = benchmark::TitleRun::from_args(&self.args);
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
                if game.death.is_open() {
                    game.death_cursor_moved();
                }
                game.creative_drag();
            }
            WindowEvent::Focused(true) => game.had_focus = true,
            WindowEvent::ModifiersChanged(m) => game.modifiers = m.state(),
            WindowEvent::Focused(false) => {
                // Like Minecraft: switching away pauses (not in scripted runs).
                if game.had_focus
                    && game.settings_path.is_some()
                    && !game.menu.is_open()
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
                        if game.console_key(&event)
                            || (!game.menu.is_open() && game.anvil_key(&event))
                            || game.search_key(&event)
                            || (game.inventory_open && !game.menu.is_open() && game.recipe_book_key(&event))
                        {
                            return;
                        }
                        if game.menu.is_open() {
                            if game.menu_key(&event) == Some(menu::MenuAction::Quit) {
                                self.quit_to_title();
                            }
                            return;
                        }
                        // v0.18 Track 2: the death screen takes the keys
                        // (Tab, arrows, Enter; Escape does nothing).
                        if game.death.is_open() {
                            if game.death_key(&event) == Some(menu::MenuAction::Quit) {
                                self.quit_to_title();
                            }
                            return;
                        }
                        if !event.repeat {
                            game.on_key(code);
                        }
                        if !game.menu.is_open() && !game.console.open {
                            game.keys.insert(keys::Input::Key(code));
                        }
                    }
                    ElementState::Released => {
                        game.keys.remove(&keys::Input::Key(code));
                        game.on_release(keys::Input::Key(code));
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                let input = keys::Input::Mouse(button);
                if !pressed {
                    game.keys.remove(&input);
                }
                if game.console.open {
                    return;
                }
                if game.credits.is_some() {
                    if pressed {
                        game.skip_credits();
                    }
                    return;
                }
                if game.menu.is_open() {
                    if game.menu_click(button, pressed) == Some(menu::MenuAction::Quit) {
                        self.quit_to_title();
                    }
                    return;
                }
                if game.session.vitals.is_dead() {
                    // v0.18 Track 2: Java's death screen buttons.
                    if game.death.is_open() && game.death_click(button, pressed) == Some(menu::MenuAction::Quit) {
                        self.quit_to_title();
                    }
                    return;
                }
                if game.inventory_open {
                    // Java's container screens close on the inventory key,
                    // even when it's a mouse button.
                    if pressed && game.settings.keys.is(keys::Action::Inventory, input) {
                        game.toggle_inventory();
                    } else if pressed && button == MouseButton::Middle && game.shows_palette() {
                        game.creative_clone();
                    } else if pressed {
                        game.inventory_click(button == MouseButton::Right);
                    } else if button == MouseButton::Left {
                        game.creative.dragging = false;
                    }
                    return;
                }
                if pressed && !game.mouse_grabbed {
                    game.set_grab(true);
                    return;
                }
                if pressed {
                    game.keys.insert(input);
                    game.on_press(input);
                } else {
                    game.on_release(input);
                }
            }
            WindowEvent::MouseWheel { .. } if game.console.open => {}
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 30.0,
                };
                game.wheel(lines);
            }
            WindowEvent::RedrawRequested => {
                game.frame();
                if game.menu_action.take() == Some(menu::MenuAction::Quit) {
                    self.quit_to_title();
                    return;
                }
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
            let sign = |invert: bool| if invert { -1.0 } else { 1.0 };
            game.session.player.look(
                delta.0 as f32 * k * sign(game.settings.invert_mouse_x),
                delta.1 as f32 * k * sign(game.settings.invert_mouse_y),
            );
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // macOS Command-Q emits LoopExiting without a CloseRequested event.
        // Scripted captures/benchmarks only save if the user explicitly quits.
        if self.save_on_exit
            && let Some(game) = &mut self.game
        {
            game.save();
            // Already logged; the game is closing, so there's no screen to show it on.
            let _ = game.save_settings();
            game.close_lan("Server closed");
        }
        if let Some(title) = &self.title {
            let _ = title.save_settings();
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
        let Game { mut renderer, mut audio, settings, settings_path, updates, pads, .. } = self;
        renderer.clear_world();
        audio.leave_world();
        Shell { renderer, audio, settings, settings_path, updates, gilrs: pads.into_gilrs() }
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

    /// A key press during play (not a repeat): Escape, which stays fixed as
    /// in Java, then every action bound to the key.
    fn on_key(&mut self, code: KeyCode) {
        if self.credits.is_some() {
            if matches!(code, KeyCode::Escape | KeyCode::Space | KeyCode::Enter) {
                self.skip_credits();
            }
            return;
        }
        if self.menu.is_open() {
            return;
        }
        if code == KeyCode::Escape {
            // The death screen ignores Escape.
            if self.session.vitals.is_dead() {
                return;
            }
            if self.inventory_open {
                self.toggle_inventory();
            } else {
                self.open_menu();
            }
            return;
        }
        self.on_press(keys::Input::Key(code));
    }

    /// A key or mouse button pressed during play: runs every action bound
    /// to it, as Java's `KeyMapping.click` does.
    fn on_press(&mut self, input: keys::Input) {
        let bindings = self.settings.keys;
        for action in bindings.actions(input) {
            if self.menu.is_open() {
                break;
            }
            self.press(action);
        }
    }

    /// A key or mouse button let go during play.
    fn on_release(&mut self, input: keys::Input) {
        if self.menu.is_open() || self.console.open || self.inventory_open || !self.mouse_grabbed {
            return;
        }
        let bindings = self.settings.keys;
        for action in bindings.actions(input) {
            match action {
                keys::Action::Attack => self.attack_button(false),
                keys::Action::Use => self.use_button(false),
                _ => {}
            }
        }
    }

    /// One action's press, with the death screen's guards.
    fn press(&mut self, action: keys::Action) {
        use keys::Action as A;
        let dead = self.session.vitals.is_dead();
        if dead && matches!(action, A::Inventory | A::GameMode | A::Fly | A::Drop | A::Jump) {
            return;
        }
        // The hands only act in play, with the mouse captured.
        let playing = !dead && !self.inventory_open && !self.console.open && self.mouse_grabbed;
        if !playing && matches!(action, A::Attack | A::Use | A::PickBlock) {
            return;
        }
        if let Some(slot) = action.hotbar() {
            // v0.17 Track D2: over a slot of an open screen, number keys
            // swap that slot with the hotbar slot instead.
            if self.inventory_open && self.lan.client.is_none() && !dead && self.swap_under_cursor(slot) {
                return;
            }
            // v0.18 Track 4: `Minecraft.handleKeybinds` for a player with
            // infinite materials: an activator held with a hotbar key loads
            // (load first) or saves that saved hotbar instead.
            if playing && self.session.mode.is_creative() && self.lan.client.is_none() {
                let (save, load) = (self.held(A::SaveHotbar), self.held(A::LoadHotbar));
                if save || load {
                    self.hotbar_load_or_save(slot, load);
                    return;
                }
            }
            self.select(slot);
            return;
        }
        match action {
            A::Attack => self.attack_button(true),
            A::Use => self.use_button(true),
            A::PickBlock => self.pick_block(),
            A::Inventory => self.toggle_inventory(),
            A::Drop => self.drop_selected(self.modifiers.control_key()),
            A::SwapHands if playing => {
                if self.lan.client.is_some() {
                    self.lan_command("swap");
                } else {
                    self.swap_hands();
                }
            }
            A::Chat if self.inventory_open && self.creative_chat_key() => {}
            A::Chat | A::Command if !self.inventory_open && !self.console.open => self.open_console(action == A::Chat),
            // Hardcore deaths end in spectator for good; only `/gamemode` cheats out.
            A::GameMode if self.lan.client.is_some() => self.lan_command(if self.session.mode.is_creative() {
                "/gamemode survival"
            } else {
                "/gamemode creative"
            }),
            A::GameMode if !self.hardcore => {
                let mode = match self.session.mode {
                    GameMode::Survival => GameMode::Creative,
                    GameMode::Creative | GameMode::Adventure | GameMode::Spectator => GameMode::Survival,
                };
                self.set_mode(mode);
            }
            A::Fly if self.session.player.can_fly => {
                self.session.player.flying = !self.session.player.flying;
                self.session.player.vel = DVec3::ZERO;
            }
            // Waiting in bed for other players: jumping gets up.
            A::Jump if self.session.sleeping.is_some() => self.session.sleeping = None,
            A::Jump => {
                self.jump_pressed = true;
                // Double-tap jump toggles flight, like Minecraft creative.
                let now = Instant::now();
                if self.session.player.can_fly && now - self.last_space < Duration::from_millis(280) {
                    self.session.player.flying = !self.session.player.flying;
                    self.session.player.vel.y = 0.0;
                }
                self.last_space = now;
            }
            // Java's `ToggleKeyMapping`: a press flips the held state.
            A::Sneak if self.settings.toggle_sneak => self.sneak_toggled = !self.sneak_toggled,
            A::Sprint if self.settings.toggle_sprint => self.sprint_toggled = !self.sprint_toggled,
            A::Vsync => {
                self.settings.vsync = !self.settings.vsync;
                self.apply_settings();
                self.show_popup(if self.settings.vsync { "VSync on" } else { "VSync off" });
            }
            A::Mute => {
                let muted = self.audio.toggle_mute();
                self.show_popup(if muted { "Sound off" } else { "Sound on" });
            }
            A::ToggleGui => self.show_hud = !self.show_hud,
            A::DebugOverlay => self.show_debug = !self.show_debug,
            A::TogglePerspective => self.camera.cycle(),
            A::Fullscreen => {
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
                // Logged; leaving Options or the world shows a failure on screen.
                let _ = self.save_settings();
            }
            A::RenderDistanceDown | A::RenderDistanceUp => {
                let step = if action == A::RenderDistanceDown { -1 } else { 1 };
                self.settings.render_distance = (self.settings.render_distance + step)
                    .clamp(settings::RENDER_DISTANCE.0, settings::RENDER_DISTANCE.1);
                self.apply_settings();
            }
            _ => {}
        }
    }

    /// Whether an action's key is down: held, or toggled on for Sneak and
    /// Sprint in Toggle mode. A table lookup per call.
    fn held(&self, action: keys::Action) -> bool {
        match action {
            keys::Action::Sneak if self.settings.toggle_sneak => self.sneak_toggled,
            keys::Action::Sprint if self.settings.toggle_sprint => self.sprint_toggled,
            _ => self.keys.contains(&self.settings.keys.get(action)),
        }
    }

    /// Mouse wheel, `lines` up positive: Scroll Sensitivity and Discrete
    /// Scrolling (`MouseHandler.onScroll`), then the menu, the inventory's
    /// lists or the hotbar.
    fn wheel(&mut self, lines: f64) {
        let s = &self.settings;
        let lines = if s.discrete_scroll && lines != 0.0 { lines.signum() } else { lines } * s.scroll_sensitivity;
        if self.menu.is_open() {
            self.menu_wheel(lines as f32);
        } else if self.inventory_open && self.shows_palette() {
            // v0.18 Track 4: `CreativeModeInventoryScreen.mouseScrolled`
            // takes the scaled wheel as it comes.
            self.creative_wheel(lines);
        } else if self.inventory_open {
            // Small steps add up, as on the hotbar, so low sensitivities still scroll.
            if self.scroll != 0.0 && lines.signum() != self.scroll.signum() {
                self.scroll = 0.0;
            }
            self.scroll += lines;
            let notches = self.scroll.trunc();
            if notches != 0.0 {
                self.scroll -= notches;
                let step = if notches > 0.0 { -1 } else { 1 };
                for _ in 0..(notches.abs() as usize).min(64) {
                    self.scroll_palette(step);
                }
            }
        } else {
            // `ScrollWheelHandler`: whole notches, one slot each, keeping
            // the rest unless the direction changes.
            if self.scroll != 0.0 && lines.signum() != self.scroll.signum() {
                self.scroll = 0.0;
            }
            self.scroll += lines;
            let notches = self.scroll.trunc();
            if notches != 0.0 {
                self.scroll -= notches;
                let step = if notches > 0.0 { 8 } else { 1 };
                self.select((self.body.actions.selected + step) % 9);
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
        self.held(keys::Action::Sneak)
    }

    /// The single entry point for hurting the player (falls, drowning,
    /// mobs, ...), through protection enchantments. `amount` is in half
    /// hearts; `cause` completes the death
    /// message "Player <cause>", e.g. "drowned" or "was slain by a zombie".
    /// Returns the damage actually taken: zero in creative, while dead, or
    /// when absorbed by the 0.5 s hurt immunity that follows each hit (a
    /// stronger hit within it only deals the difference).
    pub(crate) fn damage_player(
        &mut self,
        amount: f32,
        cause: impl Into<voxelcraft::simulation::damage::Cause>,
    ) -> f32 {
        let cause = cause.into();
        // Java's `Player.isInvulnerableTo` for the fall, fire and drowning rules.
        if self.gamerules.damage_immunity().blocks(cause) {
            return 0.0;
        }
        let amount = crate::enchant::protect(amount, &self.session.inventory.armor, cause);
        let taken = self.session.vitals.damage(amount, cause, self.session.mode.invulnerable());
        if taken > 0.0 {
            // A stronger hit inside the immunity window keeps the hurt direction, as Java skips it.
            if self.session.vitals.last_hit_fresh() {
                self.session.player.animation.hurt_direction = 0.0;
            }
            self.session.sleeping = None;
            self.audio.play(crate::audio::sounds::Sound::Hurt, None, 0.9, (0.92, 1.05));
        }
        if taken > 0.0 && self.session.vitals.is_dead() {
            if self.session.totem_saves(self.body.actions.selected) {
                self.show_popup("Totem of Undying");
                self.audio.play(crate::audio::sounds::Sound::Hurt, None, 0.6, (1.4, 1.6));
                return taken;
            }
            self.on_death();
        }
        taken
    }

    /// [`Game::damage_player_armored`] for an attack that comes from a place,
    /// `toward` being the horizontal direction from the player to its source:
    /// a raised shield facing it stops the hit (and its knockback).
    pub(crate) fn damage_player_from(
        &mut self,
        amount: f32,
        cause: impl Into<voxelcraft::simulation::damage::Cause>,
        toward: Option<glam::DVec3>,
    ) -> f32 {
        if let Some(toward) = toward {
            let slot = self.body.actions.selected;
            let hit = self.session.shield_intercept(slot, amount, toward);
            if hit.blocked {
                self.audio.play(crate::audio::sounds::Sound::Hit, None, 0.7, (0.5, 0.7));
                if hit.broke {
                    self.show_popup("Shield broke");
                    let sound = crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood);
                    self.audio.play_in(crate::audio::SoundSource::Players, sound, None, 0.8, (1.3, 1.5));
                }
                return 0.0;
            }
        }
        self.damage_player_armored(amount, cause)
    }

    /// Hurts the player through their armor (mobs, arrows, blasts, lava),
    /// wearing it down when the hit lands; `#bypasses_armor` damage keeps its
    /// full amount and wears nothing (`getDamageAfterArmorAbsorb`).
    pub(crate) fn damage_player_armored(
        &mut self,
        amount: f32,
        cause: impl Into<voxelcraft::simulation::damage::Cause>,
    ) -> f32 {
        let cause = cause.into();
        let (reduced, worn) = voxelcraft::simulation::damage::after_armor(
            amount,
            cause,
            self.session.inventory.armor_points(),
            self.session.inventory.armor_toughness(),
        );
        let taken = self.damage_player(reduced, cause);
        if taken > 0.0 && worn && self.session.mode.is_survival() {
            for item in self.session.inventory.wear_armor(amount) {
                self.show_popup(&format!("{} broke", capitalize(item.name())));
                let sound = crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood);
                self.audio.play_in(crate::audio::SoundSource::Players, sound, None, 0.8, (1.3, 1.5));
            }
        }
        taken
    }

    /// Releases the mouse and stops all actions for the death screen, and
    /// takes the shared death step (drops unless spectating or
    /// `keepInventory`, as every adapter does).
    fn on_death(&mut self) {
        if self.gamerules.bool("show_death_messages") {
            log::info!("player {}", self.session.vitals.death.as_deref().unwrap_or("died"));
        }
        if self.inventory_open {
            self.toggle_inventory();
        }
        if self.session.player.vehicle.take().is_some() {
            self.mobs.entities.dismount(&self.world, crate::entity::PlayerId::HOST);
        }
        // `ServerPlayer.die` stores `lastDeathLocation` before an immediate
        // or hardcore respawn can clear the dead state.
        self.session.record_death(&self.world);
        let grid = self.craft.take_all();
        self.session.die(&mut self.mobs.entities, self.gamerules.bool("keep_inventory"), grid);
        // `ThrownEnderpearl.tick` sees the dead owner before any respawn.
        if self.gamerules.bool("ender_pearls_vanish_on_death") {
            self.mobs.entities.discard_pearls_of_dead(|owner| owner == crate::entity::PlayerId::HOST);
        }
        // v0.18 Track 2: Java's client respawns at once when the game rule
        // hides the death screen (`shouldShowDeathScreen`), in Hardcore
        // too, where the respawn then makes the player a spectator;
        // otherwise the death screen offers Respawn or Spectate World.
        let immediate = self.gamerules.bool("immediate_respawn");
        // An immediate respawn keeps the mouse; the death screen frees it.
        if !immediate {
            self.set_grab(false);
        }
        self.keys.clear();
        // Java's `resetToggleKeys`.
        self.sneak_toggled = false;
        self.sprint_toggled = false;
        self.distortion.reset();
        self.body.left_held = false;
        self.body.right_held = false;
        self.jump_pressed = false;
        self.body.mine_pressed = false;
        self.body.actions.reset();
        if immediate {
            self.respawn_due = true;
        }
    }

    /// Runs the immediate respawn a death asked for, once the step that
    /// killed the player (a tick, a save's settling) is over.
    fn respawn_if_due(&mut self) {
        if std::mem::take(&mut self.respawn_due) && self.session.vitals.is_dead() {
            self.respawn_from_death();
            // A teleport: the camera snaps instead of gliding from the death spot.
            self.previous_eye = self.session.player.eye();
        }
    }

    /// Takes the mouse back after a respawn, unless a menu, the console or
    /// a scripted run has it.
    fn grab_after_respawn(&mut self) {
        if !self.menu.is_open()
            && !self.console.open
            && self.credits.is_none()
            && self.screenshot.is_none()
            && self.benchmark.is_none()
            && self.bench_render.is_none()
        {
            self.set_grab(true);
        }
    }

    /// Back to the bed or world spawn through the shared respawn step.
    fn respawn(&mut self) {
        if self.lan.client.is_some() {
            self.lan_command("respawn");
            return;
        }
        let keep_inventory = self.gamerules.bool("keep_inventory");
        // A Nether respawn anchor: wait for its chunk, then stand beside it.
        if let Some(anchor) = self.session.spawn_anchor {
            let here = self.session.player.pos;
            self.session.respawn(here, voxelcraft::session::DEFAULT_SPAWN_YAW, keep_inventory);
            if self.dimension == Dimension::Nether {
                self.arrival = Some(dimension::Arrival::Anchor(anchor));
            } else {
                self.switch_dimension(Dimension::Nether, dimension::Arrival::Anchor(anchor));
            }
            self.grab_after_respawn();
            return;
        }
        if self.dimension != Dimension::Overworld {
            let here = self.session.player.pos;
            self.session.respawn(here, voxelcraft::session::DEFAULT_SPAWN_YAW, keep_inventory);
            self.switch_dimension(Dimension::Overworld, dimension::Arrival::Respawn);
            self.grab_after_respawn();
            return;
        }
        // Still arriving (a death while travelling): the world isn't in, so
        // the bed can't be checked yet. Respawn here and let the arrival
        // wait for the bed's chunk, replacing any portal arrival.
        if self.arrival.is_some() {
            let here = self.session.player.pos;
            self.session.respawn(here, voxelcraft::session::DEFAULT_SPAWN_YAW, keep_inventory);
            self.arrival = Some(dimension::Arrival::Respawn);
            self.grab_after_respawn();
            return;
        }
        let (at, yaw) = self.respawn_point();
        self.session.respawn(at, yaw, keep_inventory);
        self.grab_after_respawn();
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
            // v0.18 Track 4: Java's search box has focus only on the search
            // tab, and reopening the screen clears it (`init`'s `selectTab`).
            let creative = self.session.mode == GameMode::Creative && self.body.container == Container::Inventory;
            self.search.focused = creative && self.creative.kind() == voxelcraft::creative::Kind::Search;
            if creative {
                self.search.query.clear();
                self.creative.dragging = false;
                self.creative.scroll = 0.0;
            }
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
                self.close_ender_chest(pos);
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
            // The creative tabs and scrollbar are a local view too.
            if self.shows_palette() && self.creative_press(right) {
                return;
            }
            if self.recipe_book_click(right) {
                return;
            }
            if self.session.inventory.cursor.is_some() && self.cursor_off_panel() {
                self.lan_command(if right { "toss_one" } else { "toss" });
                return;
            }
            if let Some(slot) = self.slot_under_cursor() {
                self.recipe_slot_clicked(slot);
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
        if self.shows_palette() && self.creative_press(right) {
            return;
        }
        if self.throw_cursor(right) {
            return;
        }
        if self.recipe_book_click(right) {
            return;
        }
        let slot = self.slot_under_cursor();
        if let Some(slot) = slot {
            self.recipe_slot_clicked(slot);
            self.audio.ui_click();
        }
        // v0.18 Track 4: in an item tab, shift-clicking the hotbar deletes
        // the stack (`ItemPickerMenu.quickMoveStack`).
        if self.shows_palette()
            && self.modifiers.shift_key()
            && self.creative.kind() != voxelcraft::creative::Kind::Inventory
            && let Some(hud::SlotRef::Inventory(i)) = slot
            && i < 9
        {
            self.session.inventory.slots[i] = None;
            return;
        }
        // Creative cells take shift with a stack held too (filling it).
        let creative_cell = matches!(slot, Some(hud::SlotRef::Palette(_) | hud::SlotRef::Destroy));
        if self.modifiers.shift_key() && (self.session.inventory.cursor.is_none() || creative_cell) {
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
            limited_crafting: self.gamerules.bool("limited_crafting"),
        };
        let result = f(&mut s);
        let mut close = false;
        for request in std::mem::take(&mut s.requests) {
            match request {
                screen::Request::Popup(text) => self.show_popup(&text),
                screen::Request::Close => close = true,
            }
        }
        if close && self.inventory_open {
            self.toggle_inventory();
        }
        result
    }

    fn click_slot(&mut self, slot: hud::SlotRef, right: bool) {
        self.with_screen(|s| s.click_slot(slot, right));
    }

    fn quick_move(&mut self, slot: hud::SlotRef) {
        self.with_screen(|s| s.quick_move(slot));
    }

    /// Scrolls a workstation's list by whole rows (the creative screen
    /// scrolls in `creative_wheel`).
    fn scroll_palette(&mut self, rows: i32) {
        self.scroll_work_list(rows);
    }

    /// v0.18 Track 4: a middle click on the creative screen
    /// (`ContainerInput.CLONE`): a full stack of what's under the pointer
    /// onto an empty cursor.
    fn creative_clone(&mut self) {
        if self.lan.client.is_some() {
            return;
        }
        let stack = match self.slot_under_cursor() {
            Some(hud::SlotRef::Palette(s)) => Some(s),
            Some(hud::SlotRef::Inventory(i)) => self.session.inventory.get(i),
            Some(hud::SlotRef::Armor(p)) => self.session.inventory.armor[p as usize],
            Some(hud::SlotRef::Offhand) => self.session.inventory.offhand,
            _ => None,
        };
        voxelcraft::creative::clone(&mut self.session.inventory.cursor, stack);
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
            // `AbstractMinecart.push`: the player gets a quarter of its push back.
            let back = self.mobs.entities.push_carts(self.session.player.pos, self.session.player.collision_shape());
            self.session.player.vel += back * 20.0;
        }
        for bot in self.agents.players.values_mut().filter(|b| b.active && b.agent.session.player.vehicle.is_none()) {
            let player = &mut bot.agent.session.player;
            let back = self.mobs.entities.push_carts(player.pos, player.collision_shape());
            player.vel += back * 20.0;
        }
        let rider =
            |player: &crate::player::Player, input: crate::player::MoveInput| crate::entity::minecart::RiderInput {
                push: player.ride_push(input),
                keys: crate::entity::minecart::boat::Keys::from_move(input.forward, input.right),
            };
        if self.session.player.vehicle.is_some() {
            wish.push((crate::entity::PlayerId::HOST, rider(&self.session.player, input)));
        }
        for bot in self.agents.players.values().filter(|b| b.active && b.agent.session.player.vehicle.is_some()) {
            wish.push((bot.agent.session.id, rider(&bot.agent.session.player, bot.agent.movement_input())));
        }
        self.mobs.entities.tick_minecarts(&mut self.world, &wish);
        // v1.0 Track 7: a boat's turn carries its riders' heading with it.
        self.mobs.entities.turn_boat_rider(&mut self.session.player, crate::entity::PlayerId::HOST);
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            let id = bot.agent.session.id;
            self.mobs.entities.turn_boat_rider(&mut bot.agent.session.player, id);
        }
        self.world.refresh_frame_signals(self.mobs.entities.frame_signals());
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
        self.play_shulker_sounds();
        self.play_lid_sounds();
        let notes = std::mem::take(&mut self.world.notes);
        for (pos, pitch, instrument) in notes {
            // v0.16 Track C: a head above imitates its mob at pitch 1.
            if let Some(sound) = mobs::head_note(crate::world::gadgets::Instrument::from_u8(instrument)) {
                self.audio.play(sound, Some(pos.as_dvec3() + glam::DVec3::splat(0.5)), 1.0, (1.0, 1.0));
                continue;
            }
            let sound = crate::audio::sounds::Sound::Note(crate::world::gadgets::Instrument::from_u8(instrument));
            let hz = crate::world::gadgets::pitch_hz(pitch);
            self.audio.play(sound, Some(pos.as_dvec3() + glam::DVec3::splat(0.5)), 1.0, (hz, hz));
        }
        for (pos, sound) in self.world.take_sculk_sounds() {
            use crate::audio::sounds::Sound;
            use crate::world::sculk::{SculkCue as C, SculkSound as S};
            let (sound, gain, pitch) = match sound {
                S::Clicking => (Sound::Sculk(C::Clicking), 1.0, (0.8, 1.0)),
                S::ClickingStop => (Sound::Sculk(C::ClickingStop), 1.0, (0.8, 1.0)),
                S::Resonate(p) => (Sound::Sculk(C::Resonate), 1.0, (p, p)),
                S::Bloom => (Sound::Sculk(C::Bloom), 1.0, (0.6, 1.0)),
                S::Spread => (Sound::Sculk(C::Spread), 1.0, (1.0, 1.0)),
                S::Grow => (Sound::Sculk(C::Grow), 1.0, (1.0, 1.0)),
                // Level event 3007: volume 2, pitch 0.6–1.0.
                S::Shriek => (Sound::Sculk(C::Shriek), 1.0, (0.6, 1.0)),
                S::WardenReply(level) => match voxelcraft::entity::warden::Cue::reply(level) {
                    Some(cue) => (Sound::Warden(cue), 1.0, (1.0, 1.0)),
                    None => continue,
                },
            };
            self.audio.play(sound, Some(pos), gain, pitch);
        }
        // Playing records: original tunes through the note voices
        // (`SoundSource::Records`), heard like any positioned sound.
        for note in std::mem::take(&mut self.world.records) {
            let sound = crate::audio::sounds::Sound::Note(crate::world::gadgets::Instrument::from_u8(note.instrument));
            let hz = crate::world::gadgets::pitch_hz(note.pitch);
            self.audio.play(sound, Some(note.pos.as_dvec3() + glam::DVec3::splat(0.5)), note.gain, (hz, hz));
        }
        // Level event 1010: "Now Playing" for players within 64 blocks.
        for (pos, song) in std::mem::take(&mut self.world.records_started) {
            if self.session.player.pos.distance_squared(pos.as_dvec3()) <= 64.0 * 64.0 {
                self.show_popup(&format!("Now Playing: {}", crate::world::jukebox::song_title(song)));
            }
        }
        // Level event 1043, `BOOK_PAGE_TURN`: a lectern's page turned.
        for voxelcraft::world::workstations::PageTurn(pos) in std::mem::take(&mut self.world.page_turns) {
            let at = pos.as_dvec3() + glam::DVec3::splat(0.5);
            self.audio.play_in(
                crate::audio::SoundSource::Blocks,
                crate::audio::sounds::Sound::Click,
                Some(at),
                0.5,
                (1.8, 2.0),
            );
        }
        // v0.16 Track A: level events 1033 and 1034 (`CHORUS_FLOWER_GROW`
        // and `CHORUS_FLOWER_DEATH`, `BLOCKS`), with the nearest shared sounds.
        for event in std::mem::take(&mut self.world.chorus_events) {
            use crate::world::end_blocks::ChorusEvent;
            let (cell, sound) = match event {
                ChorusEvent::Grew(cell) => {
                    (cell, crate::audio::sounds::Sound::Place(crate::audio::sounds::Material::Wood))
                }
                ChorusEvent::Died(cell) => {
                    (cell, crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood))
                }
            };
            self.audio.play_in(
                crate::audio::SoundSource::Blocks,
                sound,
                Some(cell.as_dvec3() + glam::DVec3::splat(0.5)),
                1.0,
                (0.8, 1.0),
            );
        }
        for (pos, sound) in std::mem::take(&mut self.world.trip_sounds) {
            // The existing click, at Java's volume 0.4 and the hook's pitch.
            self.audio.play_in(
                crate::audio::SoundSource::Blocks,
                crate::audio::sounds::Sound::Click,
                Some(pos.as_dvec3() + glam::DVec3::splat(0.5)),
                0.4,
                sound.pitch(),
            );
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
            Used::EditSign { cell, front } => {
                self.open_sign_editor(cell, front);
            }
            Used::Lectern { cell } => self.open_lectern(cell),
            Used::OpenBook { slot } => {
                self.open_held_book(slot);
            }
            Used::LecternBook { cell } => {
                let at = cell.as_dvec3() + DVec3::splat(0.5);
                self.audio.play(
                    crate::audio::sounds::Sound::Place(crate::audio::sounds::Material::Wood),
                    Some(at),
                    1.0,
                    (1.4, 1.6),
                );
            }
            Used::Utility { cell, effect } => self.present_utility(cell, effect),
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
            b if b.base() == Block::STONECUTTER => self.open_workstation(Container::Stonecutter(cell)),
            b if b.base() == Block::LOOM => self.open_workstation(Container::Loom(cell)),
            b if b.base() == Block::CARTOGRAPHY_TABLE => self.open_workstation(Container::Cartography(cell)),
            crate::world::beacon::BEACON => self.open_beacon(cell),
            _ => {}
        }
    }

    fn pick_block(&mut self) {
        if self.lan.client.is_some() {
            // The client's selected slot is authoritative in every input update,
            // so apply a hotbar pick here; the host still performs swaps and
            // Creative stacks for "pick".
            if let Some(i) = self.session.pick_hotbar_slot(&self.world) {
                self.select(i);
            }
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
            // A double plant's lower half brings its upper half, as item placement does.
            self.world.before_overworld_place(pos, block);
            if !self.world.set_block(pos, block) {
                failed_placements += 1;
                log::warn!("--place {pos} {}: chunk not loaded", block.name());
            }
        }
        self.world.tick_redstone();
        for (pos, lines) in std::mem::take(&mut self.signs) {
            if let Err(e) = self.world.set_sign_text(pos, true, lines) {
                log::warn!("--sign {pos}: {e}");
            }
        }
        for (kind, pos, lit) in std::mem::take(&mut self.carts) {
            // A cushion sits on the floor of its cell.
            if let Some(colour) = kind.cushion() {
                let at = pos.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                if !crate::world::colour_blocks::cushion::can_be_placed_at(&self.world, at)
                    || self.mobs.entities.place_cushion(colour, at, 0.0).is_none()
                {
                    log::warn!("--cart {pos}: nothing holds a cushion here");
                }
                continue;
            }
            match self.mobs.entities.place_cart(&self.world, kind, pos) {
                // One coal's fuel, with no push, so the lit cart stays put.
                Some(id) if lit => {
                    if let Some(cart) = self.mobs.entities.cart_mut(id) {
                        cart.fuel = crate::entity::minecart::FUEL_PER_ITEM;
                    }
                }
                Some(_) => {}
                None => log::warn!("--cart {pos}: no rail"),
            }
        }
        for (pos, face, item, held) in std::mem::take(&mut self.decor_scene) {
            self.place_scripted_decor(pos, face, item, held);
        }
        for (pos, face, strokes) in std::mem::take(&mut self.brush_scene) {
            if !self.world.brush_scripted(pos, face, strokes) {
                log::warn!("--brush {pos}: not a suspicious block");
            }
        }
        // v0.18 Track 6: pinned captures draw their throws and spawn rolls
        // from a fixed point of the entity random sequence.
        if self.mobs.still_mobs() {
            self.mobs.entities.reseed_for_capture(self.world.generator.seed);
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
                Some(Block::CRAFTING_TABLE) => self.open_crafting_table(),
                Some(b) if crate::world::furnace::is_furnace(b) => self.open_furnace(p),
                Some(b) if crate::world::chest::is_chest(b) => self.open_chest(p),
                Some(Block::BREWING_STAND) => self.open_brewing(p),
                Some(Block::ENCHANTING_TABLE) => self.open_enchanting(p),
                Some(b) if b.is_anvil() => self.open_anvil(p),
                Some(b) if b.base() == Block::GRINDSTONE => self.open_grindstone(p),
                Some(Block::SMITHING_TABLE) => self.open_smithing(p),
                Some(b) if b.base() == Block::STONECUTTER => self.open_workstation(Container::Stonecutter(p)),
                Some(b) if b.base() == Block::LOOM => self.open_workstation(Container::Loom(p)),
                Some(b) if b.base() == Block::CARTOGRAPHY_TABLE => self.open_workstation(Container::Cartography(p)),
                Some(crate::world::beacon::BEACON) => self.open_beacon(p),
                Some(b) if voxelcraft::world::workstations::lectern(b.0).is_some_and(|(_, book, _)| book) => {
                    self.open_lectern(p);
                }
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
                && (self.menu.is_open()
                    || self.console.open
                    || self.world.is_idle()
                    || self.screenshot_settled.get_or_insert_with(Instant::now).elapsed() >= SCREENSHOT_FLUID_WAIT)
                && self.mobs.waited() =>
            {
                self.screenshot_state = 1;
                false
            }
            // v0.18 Track 7: `--panorama`: the six faces first, one per frame.
            // Each face is drawn and captured on the frame after it is set.
            1 if let Some(dir) = self.panorama_out.clone() => {
                if self.renderer.capture_pending() {
                    return false;
                }
                let face = self.panorama_face.map_or(0, |f| f + 1);
                if face < 6 {
                    if let Err(e) = std::fs::create_dir_all(&dir) {
                        log::error!("--panorama: can't create {}: {e}", dir.display());
                    }
                    self.panorama_face = Some(face);
                    self.renderer.request_capture(dir.join(format!("panorama_{face}.png")));
                } else {
                    finish_panorama(&dir);
                    self.panorama_face = None;
                    self.panorama_out = None;
                }
                false
            }
            // `--world-icon` waits for the icon before the screenshot.
            1 if self.world_icon.as_ref().is_some_and(|i| !i.done()) && self.screenshot.is_some() => false,
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
        use keys::Action as A;
        let held = |a: A| self.held(a);
        let axis = |pos: A, neg: A| held(pos) as i32 as f64 - held(neg) as i32 as f64;
        if !self.menu.is_open()
            && !self.console.open
            && self.mouse_grabbed
            && !self.inventory_open
            && !self.session.vitals.is_dead()
            && self.session.sleeping.is_none()
            && !arriving
        {
            MoveInput {
                forward: axis(A::Forward, A::Back),
                right: axis(A::Right, A::Left),
                jump: held(A::Jump) || self.jump_pressed,
                descend: held(A::Sneak),
                sprint: held(A::Sprint),
                using_item: self.body.actions.eat_timer > 0.0 || self.session.using_item(),
                // Too hungry to sprint at 6 food or less (survival), or blind.
                sprint_blocked: !self.session.vitals.may_sprint(self.session.mode.invulnerable()),
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

    /// The swap-hands key: the selected stack and the off-hand trade places.
    fn swap_hands(&mut self) {
        self.with_hands(|hands| hands.swap_hands());
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
        if let Some(shot) = self.session.tick_weapons(&mut self.mobs.entities) {
            items::shot_sound(&mut self.audio, shot.ranged);
        }
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
                // The host's sprint follows the held key, which Java's client also re-engages.
                hands::Request::StopSprint => {}
                hands::Request::Popup(text) => self.show_popup(&text),
                hands::Request::Open(used) => self.present_use(used),
                hands::Request::MagicDamage(amount) => {
                    self.damage_player(amount, voxelcraft::simulation::damage::Cause::MAGIC);
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
        let paused = pauses_simulation(
            self.menu.is_open() || self.console.open || self.credits.is_some(),
            self.agents.host.is_some() || self.lan.host.is_some() || self.lan.client.is_some(),
        );
        self.respawn_if_due();
        self.sync_death_screen();
        let elapsed = now - self.last_frame;
        self.last_frame = now;
        let ticks = self.clock.advance(elapsed, paused);
        let dt = if paused { 0.0 } else { elapsed.as_secs_f64().min(0.25) };
        self.update_credits(elapsed.as_secs_f32().min(0.25));
        self.menu_tick();
        if let Some(action) = self.death_tick() {
            self.menu_action = Some(action);
        }
        self.recipe_book_frame();
        self.anvil_frame();
        // Benchmark scenes take no controller input (a stray Start would pause sampling).
        if self.lan.client.is_none() && self.benchmark.is_none() {
            self.poll_pads(dt as f32, paused);
        }

        // Streaming and GPU uploads continue during offline pause.
        if self.lan.client.is_none() {
            let eye = self.session.player.eye();
            self.update_arrival();
            // Moved by the arrival (a portal, a respawn, or towards a target
            // still loading): the camera snaps, as for any teleport.
            if self.session.player.eye() != eye {
                self.previous_eye = self.session.player.eye();
            }
        }
        let viewers: Vec<DVec3> = self.followed().map(|(_, b)| b.agent.session.player.pos).collect();
        self.world.set_viewers(&viewers);
        // Streaming spends at most this much of a frame on light; the rest
        // is resolved before the frame's first tick, below.
        self.world.set_stream_light_budget(Some(STREAM_LIGHT_BUDGET));
        self.world.update_players(self.session.player.pos, &self.agents.positions());
        if self.lan.client.is_none()
            && !self.placed
            && self.screenshot.is_none()
            && self.world.pending_jobs() == 0
            && self.world.loaded_chunks() > 0
        {
            self.apply_placements();
        }
        benchmark::mark(&mut self.benchmark, 0);
        if ticks > 0 {
            // Ticks read light exactly as when streaming resolved all of it
            // each frame.
            self.world.update_block_light();
        }
        for _ in 0..ticks {
            if self.lan.client.is_some() {
                self.tick_lan_client();
            } else {
                let started = Instant::now();
                self.tick();
                self.respawn_if_due();
                let took = started.elapsed().as_secs_f64();
                self.lan.stats_tick_secs += took;
                self.lan.stats_tick_max = self.lan.stats_tick_max.max(took);
                self.send_lan_state();
            }
        }
        if ticks > 0 {
            benchmark::mark(&mut self.benchmark, 1);
        }
        if paused {
            self.previous_eye = self.session.player.eye();
            self.mobs.entities.snapshot_positions();
            self.world.snapshot_falling_positions();
        }
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
        self.hand.update(
            dt as f32,
            self.held_item(),
            self.session.item_swap_scale(self.body.actions.selected, 1.0),
            self.session.draw_ticks(),
        );
        self.animate_hands(dt as f32, alpha, paused);
        for (pos, mesh) in self.world.mesh_uploads.drain(..) {
            self.renderer.upload_mesh(pos, mesh);
        }
        for pos in self.world.mesh_removals.drain(..) {
            self.renderer.remove_mesh(pos);
        }
        // Scripted runs (screenshots, benchmarks) have no options path and never autosave.
        if self.settings_path.is_some() && now - self.last_save > AUTOSAVE_EVERY {
            // A death the save's settling caused respawns at the next
            // frame's start, before its camera is sampled.
            self.save();
        }

        // --- Render ---------------------------------------------------------
        // v0.18 Track 8: the sky, fog and clouds are Java 26.3's environment
        // attributes (`voxelcraft::atmosphere`); `sky_state` keeps the sun's
        // direction. Both other dimensions set `visual/sky_light_factor` to 0,
        // so their terrain is lit by block light and the dimension's ambient
        // light term only (see `Dimension::ambient_light`).
        let mut sky = sky_state(self.day_time);
        let nether = !self.dimension.has_sky();
        if nether {
            sky.daylight = self.dimension.visual_sky_light_factor();
        }
        let scene = split::Scene {
            sky,
            rain: if nether { 0.0 } else { self.weather.strength },
            time: self.animation_time.unwrap_or_else(|| (now - self.started).as_secs_f32()),
            alpha,
            now,
            ticks: (dt * 20.0) as f32,
        };
        let atmosphere =
            self.atmosphere(&scene, 0, camera, forward, &self.session.vitals, self.session.mode == GameMode::Spectator);
        let fog_end = atmosphere.end();
        let dial = self.dial_of(&self.session.player);
        let others: Vec<(&Player, DVec3, crate::entity::model::PlayerAppearance, &str)> = self
            .agents
            .players
            .iter()
            .filter(|(_, b)| b.active && b.agent.session.vitals.visible())
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
                            b.agent.session.inventory.get(b.agent.selected),
                        ),
                    ),
                    name.as_str(),
                )
            })
            .collect();
        benchmark::mark(&mut self.benchmark, 2);
        self.mobs.entities.mesh(camera, forward, fog_end, scene.time, alpha);
        // v0.18 Track 6: ground shadows, for the players drawn in this view too.
        let mut casters = shadow_casters(&self.world, others.iter().map(|o| o.1), alpha);
        if !self.camera.first_person() && casts_shadow(&self.session.vitals, self.session.mode) {
            casters
                .push(voxelcraft::entity::shadow::player(eye - (self.session.player.eye() - self.session.player.pos)));
        }
        let verts = self.mobs.entities.shadows(&self.world, camera, alpha, &casters, self.settings.entity_shadows);
        push_avatars(&self.world, &others, camera, fog_end, scene.time, verts);
        crate::render::signs::text(
            &mut self.mobs.sign_text,
            crate::render::signs::FONT_VERSION,
            &self.world,
            camera,
            verts,
        );
        crate::entity::model::build_beacon_beams(&self.world, camera, scene.time, fog_end, verts);
        crate::entity::model::build_conduits(&self.world, camera, fog_end, verts);
        if !self.camera.first_person() {
            let glow = verts.glow_start();
            let look = Self::stamp_look(
                dial,
                self.hand.appearance(
                    &self.session.vitals,
                    (self.body.actions.eat_timer / EAT_TIME) as f32,
                    alpha,
                    self.session.inventory.armor,
                    self.session.inventory.get(self.body.actions.selected),
                ),
            );
            crate::entity::model::build_player(
                &self.session.player,
                eye - (self.session.player.eye() - self.session.player.pos),
                camera,
                (
                    crate::entity::sky_light(&self.world, eye),
                    self.world.block_light(eye.floor().as_ivec3()) as f32 / 15.0,
                ),
                scene.time,
                look,
                verts,
            );
            verts.glow_end(glow, look.glowing);
        }
        self.renderer.set_entities(verts);
        benchmark::mark(&mut self.benchmark, 3);
        self.renderer.set_particles(&self.particles.pool, &self.world, camera, self.session.player.forward(), alpha);
        weather::sheets(&self.world, camera, scene.rain, &mut self.weather_verts);
        self.renderer.set_weather(&self.weather_verts);
        let viewports = self.viewports();
        // v0.18 Track 9: the captions as heard from the camera this frame.
        {
            let forward = self.session.player.forward().as_dvec3();
            let right = forward.cross(glam::DVec3::Y).normalize_or_zero();
            self.captions = self.audio.subtitles(self.session.player.eye(), forward, right);
        }
        let (ui, blur_from) = if self.show_hud
            || self.session.vitals.is_dead()
            || self.menu.is_open()
            || self.console.open
            || self.credits.is_some()
        {
            self.build_ui(now)
        } else {
            (Vec::new(), None)
        };
        let mut params = FrameParams {
            camera,
            forward,
            view_effect: voxelcraft::camera::view_effect(
                &self.session.player,
                &self.session.vitals,
                alpha as f32,
                self.settings.view_bobbing,
                self.settings.damage_tilt,
            ) * self.distortion.warp(alpha as f32, self.settings.distortion),
            fov_y: self.settings.fov.to_radians()
                * if self.camera.first_person() && self.session.scoping() {
                    // `getFieldOfViewModifier` returns the spyglass's 0.1
                    // outright, ignoring FOV Effects.
                    voxelcraft::content::spyglass::ZOOM_FOV_MODIFIER
                } else {
                    fov_effects(
                        self.session.player.fov_modifier(alpha as f32) * (1.0 - 0.15 * self.bow_power().unwrap_or(0.0)),
                        self.settings.fov_effects,
                    )
                },
            sky_color: [0.0; 3],
            fog_color: [0.0; 3],
            fog_start: 0.0,
            fog_end: 0.0,
            fog_render: [0.0; 2],
            sky_end: 0.0,
            cloud_end: 0.0,
            sunrise: [0.0; 4],
            sky_light_color: [1.0; 3],
            clouds: self.cloud_params(&scene, atmosphere.cloud_color),
            cloud_color: [0.0; 4],
            daylight: if self.dimension.has_sky() { atmosphere.sky_light.0 } else { scene.sky.daylight },
            zenith_color: [0.0; 3],
            sun_dir: scene.sky.sun_dir,
            dimension: self.dimension,
            enhanced_graphics: self.settings.enhanced_graphics,
            time: scene.time,
            highlight: self.target().filter(|_| self.mob_target().is_none()).map(|(p, _)| {
                let (min, max) = self.world.outline(p);
                (p, min, max)
            }),
            cracks: self.cracks_seen_from(eye),
            block_models: {
                let mut models = self.block_models(alpha, camera);
                crate::render::signs::shelf_items(&self.world, camera, &mut models);
                crate::render::brushable::emerging_items(&self.world, camera, crate::render::signs::RANGE, &mut models);
                models
            },
            signs: crate::render::signs::models(&self.world, camera),
            // `ItemInHandRenderer` hides the hands while scoping.
            hand: (self.camera.first_person()
                && self.show_hud
                && !self.session.vitals.is_dead()
                && self.session.sleeping.is_none()
                && !self.session.scoping())
            .then(|| {
                let eye = self.session.player.eye();
                let eating = (self.body.actions.eat_timer / EAT_TIME) as f32;
                let held = self.session.inventory.get(self.body.actions.selected);
                self.map_hand(
                    &self.session,
                    self.body.actions.selected,
                    Self::stamp_hand(
                        held.map_or(dial, |s| dial.for_stack(&s)),
                        self.hand.view(eating, crate::entity::sky_light(&self.world, eye), self.torch_light(eye)),
                    ),
                )
            }),
            rain: scene.rain,
            night_vision: self
                .session
                .vitals
                .effects
                .vision(scene.time, self.session.player.head_in_water(&self.world)),
            ui,
            // v0.18 Track 2: a menu over the world blurs what is under it.
            blur: blur_from.map_or(0, |_| self.settings.menu_blur),
            ui_under_blur: blur_from.unwrap_or(0),
        };
        atmosphere.apply(&mut params);
        // v0.18 Track 7: `--panorama` renders each face as a square 90-degree view.
        if let Some(face) = self.panorama_face {
            let (forward, view_effect) = panorama_face_view(face);
            params.forward = forward;
            params.view_effect = view_effect;
            params.fov_y = std::f32::consts::FRAC_PI_2;
            params.hand = None;
            params.highlight = None;
            params.ui = Vec::new();
            params.blur = 0;
            params.ui_under_blur = 0;
        }
        if let Some(icon) = self.world_icon.as_mut() {
            let drawn = self.renderer.stats.visible;
            let settled = self.world.loaded_chunks() > 0 && self.world.pending_jobs() == 0;
            icon.before_frame(&mut self.renderer, drawn, settled);
        }
        benchmark::mark(&mut self.benchmark, 4);
        let Some(mut frame) = self.renderer.begin_frame() else {
            return; // hidden window: don't count this frame in the stats
        };
        benchmark::mark(&mut self.benchmark, 5);
        self.renderer.draw_view(&mut frame, &params, viewports[0]);
        self.draw_followers(&mut frame, &scene, &viewports);
        benchmark::mark(&mut self.benchmark, 6);
        self.renderer.end_frame(frame);
        benchmark::mark(&mut self.benchmark, 7);

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
        // Max Framerate. Scripted runs (screenshots, benchmarks) are never held back.
        if self.settings_path.is_some() && self.benchmark.is_none() {
            self.frame_limit.wait(self.settings.frame_cap());
        }
    }
}

/// v0.18 Track 7: rewrites the six captured faces as RGB PNGs (the captures
/// are RGBA with an opaque alpha), the form `assets/panorama/` keeps.
fn finish_panorama(dir: &std::path::Path) {
    for face in 0..6 {
        let path = dir.join(format!("panorama_{face}.png"));
        let result = (|| -> Result<(), String> {
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut reader = png::Decoder::new(std::io::BufReader::new(file)).read_info().map_err(|e| e.to_string())?;
            let mut buf = vec![0; reader.output_buffer_size().ok_or("too large")?];
            let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
            if info.width != info.height {
                return Err(format!("{} x {} isn't square: give --size a square", info.width, info.height));
            }
            let rgb: Vec<u8> =
                buf[..info.buffer_size()].as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
            let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), info.width, info.height);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::High);
            enc.write_header().and_then(|mut w| w.write_image_data(&rgb)).map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => log::info!("panorama face {}", path.display()),
            Err(e) => log::error!("--panorama: {}: {e}", path.display()),
        }
    }
}

/// v0.18 Track 7: the camera for panorama face `face`: its forward
/// direction, and for the up and down faces (which the camera's Y-up view
/// can't look along) a turn of view space from a northward view, so each
/// face comes out with `voxelcraft::title::panorama::FACES`' up.
fn panorama_face_view(face: usize) -> (glam::Vec3, glam::Mat4) {
    use voxelcraft::title::panorama::FACES;
    match face {
        4 => (glam::Vec3::NEG_Z, glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        5 => (glam::Vec3::NEG_Z, glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        _ => (FACES[face].forward, glam::Mat4::IDENTITY),
    }
}

/// The FOV Effects option (`fovEffectScale`, 0..1) applied to a gameplay
/// FOV modifier, as `AbstractClientPlayer.getFieldOfViewModifier` does.
pub(super) fn fov_effects(modifier: f32, scale: f32) -> f32 {
    1.0 + (modifier - 1.0) * scale
}

impl Game {
    /// Falling blocks, primed TNT, dropped items and banners near `camera`,
    /// at interpolated positions.
    fn block_models(&self, alpha: f64, camera: glam::DVec3) -> Vec<crate::render::BlockModel> {
        self.world
            .falling_blocks()
            .iter()
            .map(|f| crate::render::BlockModel {
                patterns: None,
                lid: None,
                min: f.previous_pos.lerp(f.pos, alpha),
                size: 1.0,
                block: f.block,
                sky_light: crate::entity::sky_light(&self.world, f.pos + glam::DVec3::splat(0.5)),
                block_light: self.torch_light(f.pos + glam::DVec3::splat(0.5)),
                yaw: 0.0,
                icon: None,
                orient: None,
                tint: [255; 4],
            })
            .chain(self.mobs.entities.tnt.iter().map(|t| {
                let s = t.size();
                crate::render::BlockModel {
                    patterns: None,
                    lid: None,
                    min: t.previous_pos.lerp(t.pos, alpha)
                        - glam::DVec3::new(s as f64 / 2.0, (s as f64 - 1.0) / 2.0, s as f64 / 2.0),
                    size: s,
                    // White flashes count down to the blast.
                    block: if t.flash() { Block::WOOL } else { Block::TNT },
                    sky_light: crate::entity::sky_light(&self.world, t.pos + glam::DVec3::Y * 0.5),
                    block_light: self.torch_light(t.pos + glam::DVec3::Y * 0.5),
                    yaw: 0.0,
                    icon: None,
                    orient: None,
                    tint: [255; 4],
                }
            }))
            .chain(self.world.moving_piston_blocks(alpha).chain(self.world.moving_piston_heads(alpha)).map(
                |(block, min)| crate::render::BlockModel {
                    patterns: None,
                    lid: None,
                    min,
                    size: 1.0,
                    block,
                    sky_light: crate::entity::sky_light(&self.world, min + glam::DVec3::splat(0.5)),
                    block_light: self.torch_light(min + glam::DVec3::splat(0.5)),
                    yaw: 0.0,
                    icon: None,
                    orient: None,
                    tint: [255; 4],
                },
            ))
            .chain(self.item_models(alpha))
            .chain(self.banner_models(camera))
            .chain(self.shulker_models(camera, alpha))
            .chain(self.lid_models(alpha))
            .chain(self.decor_models())
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
    verts: &mut crate::entity::model::EntityMesh,
) {
    for &(player, feet, appearance, name) in players {
        // A body around the camera (players can share a spot) would fill the view.
        let inside = (camera - feet).with_y(0.0).length() < 0.4 && (feet.y..feet.y + 1.9).contains(&camera.y);
        if !inside && feet.distance_squared(camera) < (fog_end as f64 + 2.0).powi(2) {
            let glow = verts.glow_start();
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
            verts.glow_end(glow, appearance.glowing);
            if appearance.death.is_none() && !appearance.invisible {
                crate::entity::model::build_player_name(player, feet, name, camera, verts);
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Single-player pause, as Java's integrated server pauses only while it is
/// not shared: an open game menu, the console or the credits stop gameplay
/// ticks, unless other players are in this world (the CLI agent server, a
/// LAN host or a LAN client). Controller players' own menus never pause the
/// world; the inventory and container screens don't either.
fn pauses_simulation(screen_open: bool, shared: bool) -> bool {
    screen_open && !shared
}

/// v0.18 Track 6: whether a player's body casts a ground shadow: drawn
/// (`Vitals::visible`), not Invisible and not a spectator (Java's
/// `isInvisible` hides both).
pub(crate) fn casts_shadow(
    vitals: &voxelcraft::simulation::survival::Vitals,
    mode: voxelcraft::rules::GameMode,
) -> bool {
    vitals.visible()
        && !vitals.effects.has(voxelcraft::simulation::effects::Effect::Invisibility)
        && mode != voxelcraft::rules::GameMode::Spectator
}

/// v0.18 Track 6: the shadow casters the entity list doesn't hold: the
/// players drawn in this view (at `players`' feet) and the falling blocks.
pub(crate) fn shadow_casters(
    world: &voxelcraft::world::World,
    players: impl Iterator<Item = DVec3>,
    alpha: f64,
) -> Vec<voxelcraft::entity::shadow::Caster> {
    use voxelcraft::entity::shadow;
    let falling = world.falling_blocks().iter().map(|f| shadow::falling_block(f.previous_pos.lerp(f.pos, alpha)));
    players.map(shadow::player).chain(falling).collect()
}

#[cfg(test)]
mod pause_tests {
    use super::*;
    use crate::simulation::FixedClock;
    use std::time::Duration;

    #[test]
    fn menus_pause_only_an_unshared_world_and_resume_without_catching_up() {
        assert!(pauses_simulation(true, false));
        assert!(!pauses_simulation(true, true), "agents or LAN players keep it running");
        assert!(!pauses_simulation(false, false));
        let mut clock = FixedClock::default();
        assert_eq!(clock.advance(Duration::from_millis(500), pauses_simulation(true, false)), 0);
        assert_eq!(clock.advance(Duration::from_millis(200), pauses_simulation(true, true)), 4);
        // Time spent paused is not charged on resume.
        assert_eq!(clock.advance(Duration::from_millis(50), false), 1);
        assert_eq!(clock.paused(), Duration::from_millis(500));
    }

    #[test]
    fn unreadable_levels_are_not_opened() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-unreadable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let storage = Storage::new(dir.clone());
        assert_eq!(unreadable_level(&storage, false), None, "no world yet: create one");
        std::fs::write(dir.join("level.txt"), "seed=12\nname=ok\n").unwrap();
        assert_eq!(unreadable_level(&storage, false), None);
        std::fs::write(dir.join("level.txt"), "seed=not-a-number\n").unwrap();
        assert!(unreadable_level(&storage, false).unwrap().contains("bad seed"));
        assert_eq!(unreadable_level(&storage, true), None, "--new and new worlds replace it on purpose");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
