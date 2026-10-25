//! The title screen and everything reached from it before a world opens:
//! Java's title menu, Select World with Edit World, Re-Create and the
//! newer-version backup prompt, Create New World, Multiplayer and the
//! update screen. Every screen runs on the shared menu toolkit, so each
//! works by mouse, keyboard and controller. A world is loaded by handing the
//! [`Shell`] to `Game::start`; quitting to the title hands it back.
//!
//! The world screens are generic over [`TitleHost`], so tests drive them
//! without a window; Multiplayer and the update screen need the real
//! [`Title`] (its LAN discovery and updater).

use std::path::{Path, PathBuf};
use std::time::Instant;

use glam::{DVec3, Vec3};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::ModifiersState;
use winit::window::CursorGrabMode;

use crate::render::FrameParams;
use crate::render::ui::Ui;
use crate::simulation::difficulty::Difficulty;
use crate::update::State;

use super::gui::{self, Clipboard, Key, Nav, Notice, Step, SystemClipboard};
use super::options::OptionsHost;
use super::settings::Settings;
use super::{GameMode, Shell};

mod create;
mod edit;
mod home;
mod multiplayer;
mod prompts;
mod select;
mod update;
mod worlds;

/// `--open-menu` names that start on the title screens.
pub(super) const TITLE_MENUS: [&str; 8] =
    ["title", "select", "create", "edit", "recreate", "backup", "multiplayer", "direct"];

/// Whether the world in `dir` was last saved by a newer VoxelCraft.
pub(super) fn saved_by_newer(saves_dir: &Path, dir: &str) -> bool {
    worlds::summary(saves_dir, dir).is_some_and(|w| w.newer().is_some())
}

/// A world made on the create screen.
pub(crate) struct NewWorld {
    pub name: String,
    pub seed: Option<u64>,
    pub mode: GameMode,
    pub difficulty: Difficulty,
    pub hardcore: bool,
}

/// What the title screen asks the app to do.
pub(super) enum Action {
    /// Load the save folder, creating it as described if `Some`.
    Play(String, Option<NewWorld>),
    Join(voxelcraft::lan::Connection, String),
    Quit,
}

/// What the title's world screens need from the app.
pub(in crate::app) trait TitleHost: OptionsHost<Out = Action> + Sized {
    fn saves_dir(&self) -> &Path;
    /// Where Make Backup and the updater put backups (Java's `backups`
    /// folder beside `saves`).
    fn backups_dir(&self) -> PathBuf {
        let saves = self.saves_dir();
        saves.parent().unwrap_or(saves).join("backups")
    }
    /// Shows a folder in the system's file browser.
    fn open_path(&mut self, path: &Path);
    fn copy_text(&mut self, text: &str);
    /// "Update to 0.12" while a newer release is on offer.
    fn update_offer(&self) -> Option<String>;
    /// The updater's one-line status for the title screen.
    fn update_notice(&self) -> Option<String>;
    fn multiplayer(&mut self) -> Step<Self>;
    fn updates(&mut self) -> Step<Self>;
}

pub(super) struct Title {
    shell: Shell,
    saves_dir: PathBuf,
    cursor_px: (f32, f32),
    started: Instant,
    /// `--open-menu title --screenshot`: capture after a few frames, then quit.
    pub screenshot: Option<String>,
    /// `--benchmark-scene title`: cycle the screens and sample frame times.
    pub benchmark: Option<super::benchmark::TitleRun>,
    frames: u32,
    /// The last address typed on Direct Connection.
    address: String,
    /// The LAN profile name other players see.
    profile: String,
    /// Every title screen, the title menu at the bottom.
    menus: gui::Stack<Title>,
    modifiers: ModifiersState,
    clipboard: SystemClipboard,
}

impl gui::Host for Title {
    type Out = Action;

    fn click(&mut self) {
        self.shell.audio.ui_click();
    }

    fn in_world(&self) -> bool {
        false
    }
}

impl OptionsHost for Title {
    fn settings(&self) -> &Settings {
        &self.shell.settings
    }

    fn settings_mut(&mut self) -> &mut Settings {
        &mut self.shell.settings
    }

    fn apply_settings(&mut self) {
        self.shell.apply_settings();
    }

    fn save_settings(&self) -> Result<(), String> {
        Title::save_settings(self)
    }

    fn window_size(&self) -> Option<(u32, u32)> {
        Some(self.shell.renderer.size())
    }

    fn apply_fullscreen(&mut self) {
        self.shell.settings.fullscreen.apply(&self.shell.renderer.window);
    }
}

impl TitleHost for Title {
    fn saves_dir(&self) -> &Path {
        &self.saves_dir
    }

    fn open_path(&mut self, path: &Path) {
        crate::update::install::open_url(&path.to_string_lossy());
    }

    fn copy_text(&mut self, text: &str) {
        self.clipboard.set(text);
    }

    fn update_offer(&self) -> Option<String> {
        self.shell.updates.offer().map(|r| format!("Update to {}", r.version.short()))
    }

    fn update_notice(&self) -> Option<String> {
        self.shell.updates.notice.clone()
    }

    fn multiplayer(&mut self) -> Step<Self> {
        Step::push(multiplayer::Multiplayer::new(self))
    }

    fn updates(&mut self) -> Step<Self> {
        Step::push(update::UpdateScreen)
    }
}

impl Title {
    pub fn new(shell: Shell, saves_dir: &Path) -> Self {
        let window = &shell.renderer.window;
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        let mut menus = gui::Stack::default();
        menus.open(home::Home);
        Self {
            shell,
            saves_dir: saves_dir.to_path_buf(),
            cursor_px: (0.0, 0.0),
            started: Instant::now(),
            screenshot: None,
            benchmark: None,
            frames: 0,
            address: "127.0.0.1:25565".into(),
            profile: super::lan::local_profile(saves_dir),
            menus,
            modifiers: ModifiersState::empty(),
            clipboard: SystemClipboard,
        }
    }

    /// Back to the title menu with nothing over it.
    fn reset(&mut self) {
        self.menus.close();
        self.menus.open(home::Home);
    }

    /// Select World over the title menu, as after leaving a world in Java.
    pub fn show_select(&mut self) {
        self.reset();
        self.menus.open(select::SelectWorld::new(self));
    }

    /// Shows what went wrong with a world on a screen of its own, over
    /// whatever is open.
    pub fn world_error(&mut self, title: &str, reason: String) {
        self.menus.open(Notice::new(title, vec![reason]));
    }

    /// Shows that `options.txt` couldn't be written, over whatever is open.
    pub fn options_error(&mut self, reason: String) {
        self.menus.open(super::options::save_failed(reason));
    }

    /// Opens a world from outside the list (`--world`) the way the list
    /// does: a world saved by a newer version asks first.
    pub fn confirm_open(&mut self, dir: &str) -> Option<Action> {
        self.show_select();
        let world = worlds::summary(&self.saves_dir, dir)?;
        let step = select::open(self, &world);
        self.menus.run(step)
    }

    /// `--open-menu` names that start on the title screens; `world` picks
    /// the world for Edit World, Re-Create and the backup prompt (else the
    /// most recently played).
    pub fn open_menu(&mut self, name: &str, world: Option<&str>) {
        let pick = || {
            let all = worlds::list(&self.saves_dir);
            all.iter().find(|w| Some(w.dir.as_str()) == world).or(all.first()).cloned()
        };
        match name {
            "select" => self.show_select(),
            "create" => {
                self.show_select();
                self.menus.open(create::CreateWorld::new());
            }
            "multiplayer" => self.show_multiplayer(),
            "direct" => {
                self.show_multiplayer();
                self.menus.open(multiplayer::DirectConnect::new(&self.address));
            }
            "edit" | "recreate" | "backup" => {
                let picked = pick();
                self.show_select();
                let Some(world) = picked else { return };
                match name {
                    "edit" => self.menus.open(edit::EditWorld::new(&world, Default::default())),
                    "recreate" => self.menus.open(create::CreateWorld::recreate(&world)),
                    // The newer-version prompt as it looks, whatever the world's version.
                    _ => self.menus.open(prompts::BackupConfirm::downgrade(world, crate::update::Version(9, 9, 9))),
                }
            }
            _ => {}
        }
    }

    /// Mouse position and screen size in UI pixels.
    fn ui_cursor(&self) -> ((f32, f32), gui::Size) {
        let (w, h) = self.shell.renderer.size();
        let scale = self.ui_scale();
        ((self.cursor_px.0 / scale, self.cursor_px.1 / scale), (w as f32 / scale, h as f32 / scale))
    }

    /// Runs `f` on the screens with the title as their host.
    fn with_menus<T>(&mut self, f: impl FnOnce(&mut gui::Stack<Title>, &mut Title, gui::Size) -> T) -> T {
        let mut stack = std::mem::take(&mut self.menus);
        let (_, size) = self.ui_cursor();
        let out = f(&mut stack, self, size);
        self.menus = stack;
        // The title menu stays at the bottom; nothing closes it.
        if !self.menus.is_open() {
            self.menus.open(home::Home);
        }
        out
    }

    /// Controller buttons: menu navigation.
    fn poll_pads(&mut self) -> Option<Action> {
        let mut navs = Vec::new();
        if let Some(gilrs) = self.shell.gilrs.as_mut() {
            while let Some(event) = gilrs.next_event() {
                if let gilrs::EventType::ButtonPressed(button, _) = event.event {
                    navs.extend(Nav::from_pad(button));
                }
            }
        }
        let mut out = None;
        for nav in navs {
            out = out.or(self.with_menus(|stack, title, size| stack.nav(title, nav, size)));
        }
        out
    }

    /// The LAN connection was lost: say so over the Multiplayer screen.
    pub fn connection_error(&mut self, reason: String) {
        self.show_multiplayer();
        self.menus.open(Notice::new("Connection Lost", vec![reason]));
    }

    pub fn show_multiplayer(&mut self) {
        self.reset();
        let screen = multiplayer::Multiplayer::new(self);
        self.menus.open(screen);
    }

    /// `--join`: connect straight away, from the Multiplayer screen.
    pub fn connect_address(&mut self, address: String, profile: Option<String>) {
        self.show_multiplayer();
        if let Some(profile) = profile {
            self.profile = profile;
        }
        self.address = address.clone();
        let step = multiplayer::connect(self, address);
        self.menus.run(step);
    }

    pub fn into_shell(self) -> Shell {
        self.shell
    }

    pub fn request_redraw(&self) {
        self.shell.renderer.window.request_redraw();
    }

    /// Writes `options.txt`, logging and returning what went wrong.
    pub fn save_settings(&self) -> Result<(), String> {
        let Some(path) = &self.shell.settings_path else { return Ok(()) };
        super::settings::save_or_report(&self.shell.settings, path)
    }

    pub fn event(&mut self, event: WindowEvent) -> Option<Action> {
        // A benchmark ignores input, as in a world.
        if self.benchmark.is_some()
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::CursorMoved { .. }
                    | WindowEvent::ModifiersChanged(_)
            )
        {
            return None;
        }
        match event {
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Resized(size) => self.shell.renderer.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_px = (position.x as f32, position.y as f32);
                let (pos, _) = self.ui_cursor();
                self.with_menus(|stack, title, size| stack.mouse_moved(title, pos, size));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                return self.with_menus(|stack, title, size| stack.mouse_button(title, button, pressed, size));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 30.0) as f32,
                };
                self.with_menus(|stack, title, size| stack.wheel(title, lines, size));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = Key::from_event(&event, self.modifiers)?;
                return self.with_menus(|stack, title, size| stack.key(title, &key, size));
            }
            WindowEvent::RedrawRequested => {
                if self.benchmark.is_some() {
                    return self.benchmark_frame().then_some(Action::Quit);
                }
                if let Some(action) = self.poll_pads() {
                    return Some(action);
                }
                if let Some(action) = self.with_menus(|stack, title, size| stack.tick(title, size)) {
                    return Some(action);
                }
                self.shell.updates.poll(self.shell.settings.skip_update);
                if self.shell.updates.auto && matches!(self.shell.updates.state, State::Available(_)) {
                    self.shell.updates.start(&self.saves_dir);
                }
                if matches!(self.shell.updates.state, State::Ready(..)) && self.shell.updates.launch() {
                    return Some(Action::Quit);
                }
                self.frame();
                return self.screenshot_done().then_some(Action::Quit);
            }
            _ => {}
        }
        None
    }

    /// Physical pixels per UI pixel: the GUI Scale option, as in a world.
    fn ui_scale(&self) -> f32 {
        let (w, h) = self.shell.renderer.size();
        Ui::gui_scale(w as f32, h as f32, self.shell.settings.gui_scale)
    }

    fn build_ui(&self) -> Vec<crate::render::ui::UiVertex> {
        let (w, h) = self.shell.renderer.size();
        let mut ui = Ui::with_scale(w as f32, h as f32, self.ui_scale());
        self.menus.draw(self, &mut ui);
        ui.verts
    }

    /// A title benchmark frame; true once the report is written.
    fn benchmark_frame(&mut self) -> bool {
        let Some(mut run) = self.benchmark.take() else { return false };
        self.frame();
        if let Some(screen) = run.frame(self.shell.renderer.skipped_frames) {
            self.reset();
            match screen {
                "title" => {}
                "select" | "create" | "multiplayer" | "direct" => self.open_menu(screen, None),
                _ => super::options::open_screens(&mut self.menus, screen),
            }
        }
        let done = run.done();
        if done {
            run.finish(&self.shell.renderer, &self.shell.settings);
        }
        self.benchmark = Some(run);
        done
    }

    fn screenshot_done(&mut self) -> bool {
        let Some(path) = &self.screenshot else { return false };
        self.frames += 1;
        match self.frames {
            3 => self.shell.renderer.request_capture(path.clone()),
            4.. => return !self.shell.renderer.capture_pending(),
            _ => {}
        }
        false
    }

    /// Draws a slowly turning sky behind the menus.
    fn frame(&mut self) {
        let t = self.started.elapsed().as_secs_f32();
        let sky = super::sky_state(0.12);
        let yaw = t * 0.02;
        let forward = Vec3::new(yaw.cos(), 0.18, yaw.sin()).normalize();
        let params = FrameParams {
            camera: DVec3::new(0.0, 90.0, 0.0),
            forward,
            view_effect: glam::Mat4::IDENTITY,
            fov_y: 70f32.to_radians(),
            sky_color: sky.horizon.map(|c| c as f64),
            fog_color: sky.horizon,
            fog_start: 200.0,
            fog_end: 400.0,
            daylight: sky.daylight,
            zenith_color: sky.zenith,
            dimension: crate::world::terrain::Dimension::Overworld,
            enhanced_graphics: self.shell.settings.enhanced_graphics,
            sun_dir: sky.sun_dir,
            time: t * 8.0,
            highlight: None,
            hand: None,
            cracks: Vec::new(),
            block_models: Vec::new(),
            ui: self.build_ui(),
            rain: 0.0,
            night_vision: 0.0,
        };
        self.shell.renderer.render(&params);
    }
}

#[cfg(test)]
mod tests;
