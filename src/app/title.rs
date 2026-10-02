//! Title screen: the list of saved worlds, creating a new one (name, seed
//! and game mode) and deleting old ones. A world is loaded by handing the
//! [`Shell`] to `Game::start`; quitting to the title hands it back.
//!
//! Like the pause menu, one layout function feeds both drawing and mouse
//! hit-testing.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use glam::{DVec3, Vec3};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::CursorGrabMode;

use crate::render::FrameParams;
use crate::render::ui::{Ui, WHITE};
use crate::world::storage::Storage;

use super::menu::{BUTTON, HOT, bevel, inside};
use super::{GameMode, Shell};

/// A world made on the create screen.
pub(crate) struct NewWorld {
    pub name: String,
    pub seed: Option<u64>,
    pub mode: GameMode,
}

/// What the title screen asks the app to do.
pub(super) enum Action {
    /// Load the save folder, creating it as described if `Some`.
    Play(String, Option<NewWorld>),
    Quit,
}

/// A saved world in the list.
#[derive(Debug)]
struct Entry {
    /// Save folder name.
    dir: String,
    name: String,
    mode: String,
    /// Where the player was when the world was last saved.
    nether: bool,
    played: Option<SystemTime>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    List,
    Create,
    ConfirmDelete,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Widget {
    /// A row of the world list (index into the worlds).
    World(usize),
    Play,
    Create,
    Delete,
    Quit,
    NameField,
    SeedField,
    Mode,
    CreateWorld,
    Cancel,
    ConfirmDelete,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Name,
    Seed,
}

const BUTTON_W: f32 = 200.0;
const HALF_W: f32 = 133.0;
const BUTTON_H: f32 = 20.0;
const ROW_H: f32 = 28.0;
const LIST_W: f32 = 270.0;
const NAME_MAX: usize = 32;
const SEED_MAX: usize = 24;
/// Two clicks on a world row this close together play it.
const DOUBLE_CLICK: f32 = 0.4;

pub(super) struct Title {
    shell: Shell,
    saves_dir: PathBuf,
    worlds: Vec<Entry>,
    selected: Option<usize>,
    /// First visible row of the world list.
    scroll: usize,
    screen: Screen,
    name: String,
    seed: String,
    focus: Field,
    mode: GameMode,
    cursor_px: (f32, f32),
    last_click: Option<(usize, Instant)>,
    started: Instant,
    /// `--open-menu title --screenshot`: capture after a few frames, then quit.
    pub screenshot: Option<String>,
    frames: u32,
}

impl Title {
    pub fn new(shell: Shell, saves_dir: &Path) -> Self {
        let window = &shell.renderer.window;
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        let worlds = list_worlds(saves_dir);
        Self {
            shell,
            saves_dir: saves_dir.to_path_buf(),
            selected: (!worlds.is_empty()).then_some(0),
            worlds,
            scroll: 0,
            screen: Screen::List,
            name: String::new(),
            seed: String::new(),
            focus: Field::Name,
            mode: GameMode::Survival,
            cursor_px: (0.0, 0.0),
            last_click: None,
            started: Instant::now(),
            screenshot: None,
            frames: 0,
        }
    }

    pub fn into_shell(self) -> Shell {
        self.shell
    }

    pub fn request_redraw(&self) {
        self.shell.renderer.window.request_redraw();
    }

    pub fn save_settings(&self) {
        if let Some(path) = &self.shell.settings_path
            && let Err(e) = self.shell.settings.save(path)
        {
            log::error!("failed to save options to {}: {e}", path.display());
        }
    }

    pub fn event(&mut self, event: WindowEvent) -> Option<Action> {
        match event {
            WindowEvent::Resized(size) => self.shell.renderer.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => self.cursor_px = (position.x as f32, position.y as f32),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                return self.click();
            }
            WindowEvent::MouseWheel { delta, .. } if self.screen == Screen::List => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 30.0) as f32,
                };
                if dy.abs() >= 0.5 {
                    let rows = self.visible_rows();
                    let max = self.worlds.len().saturating_sub(rows);
                    self.scroll = self.scroll.saturating_add_signed(if dy > 0.0 { -1 } else { 1 }).min(max);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let PhysicalKey::Code(code) = event.physical_key else { return None };
                if let Some(action) = self.key(code) {
                    return Some(action);
                }
                if self.screen == Screen::Create
                    && let Some(text) = &event.text
                {
                    self.type_text(text);
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                return self.screenshot_done().then_some(Action::Quit);
            }
            _ => {}
        }
        None
    }

    fn key(&mut self, code: KeyCode) -> Option<Action> {
        match (self.screen, code) {
            (Screen::List, KeyCode::Enter) => return self.play_selected(),
            (Screen::List, KeyCode::ArrowUp | KeyCode::ArrowDown) if !self.worlds.is_empty() => {
                let i = self.selected.unwrap_or(0) as isize + if code == KeyCode::ArrowUp { -1 } else { 1 };
                self.select(i.clamp(0, self.worlds.len() as isize - 1) as usize);
            }
            (Screen::List, KeyCode::Delete) if self.selected.is_some() => self.screen = Screen::ConfirmDelete,
            (Screen::Create, KeyCode::Enter) => return Some(self.create()),
            (Screen::Create, KeyCode::Tab) => {
                self.focus = if self.focus == Field::Name { Field::Seed } else { Field::Name };
            }
            (Screen::Create, KeyCode::Backspace) => {
                self.field().pop();
            }
            (Screen::Create | Screen::ConfirmDelete, KeyCode::Escape) => self.screen = Screen::List,
            _ => {}
        }
        None
    }

    fn field(&mut self) -> &mut String {
        match self.focus {
            Field::Name => &mut self.name,
            Field::Seed => &mut self.seed,
        }
    }

    fn type_text(&mut self, text: &str) {
        let max = if self.focus == Field::Name { NAME_MAX } else { SEED_MAX };
        let field = self.field();
        for c in text.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
            if field.len() < max {
                field.push(c);
            }
        }
    }

    fn select(&mut self, i: usize) {
        self.selected = Some(i);
        let rows = self.visible_rows();
        if i < self.scroll {
            self.scroll = i;
        } else if i >= self.scroll + rows {
            self.scroll = i + 1 - rows;
        }
    }

    fn play_selected(&self) -> Option<Action> {
        let entry = self.worlds.get(self.selected?)?;
        Some(Action::Play(entry.dir.clone(), None))
    }

    fn create(&self) -> Action {
        let name = if self.name.trim().is_empty() { "New World".to_string() } else { self.name.trim().to_string() };
        let dir = folder_for(&name, |d| self.saves_dir.join(d).exists());
        let new = NewWorld { name, seed: parse_seed(&self.seed), mode: self.mode };
        Action::Play(dir, Some(new))
    }

    fn click(&mut self) -> Option<Action> {
        let (widget, _) = self.widget_under_cursor()?;
        self.shell.audio.ui_click();
        match widget {
            Widget::World(i) => {
                let double = self.last_click.is_some_and(|(j, t)| j == i && t.elapsed().as_secs_f32() < DOUBLE_CLICK);
                self.select(i);
                self.last_click = Some((i, Instant::now()));
                if double {
                    return self.play_selected();
                }
            }
            Widget::Play => return self.play_selected(),
            Widget::Create => {
                self.screen = Screen::Create;
                self.name = "New World".into();
                self.seed.clear();
                self.focus = Field::Name;
                self.mode = GameMode::Survival;
            }
            Widget::Delete if self.selected.is_some() => self.screen = Screen::ConfirmDelete,
            Widget::Quit => return Some(Action::Quit),
            Widget::NameField => self.focus = Field::Name,
            Widget::SeedField => self.focus = Field::Seed,
            Widget::Mode => {
                self.mode = match self.mode {
                    GameMode::Survival => GameMode::Creative,
                    GameMode::Creative => GameMode::Survival,
                }
            }
            Widget::CreateWorld => return Some(self.create()),
            Widget::Cancel => self.screen = Screen::List,
            Widget::ConfirmDelete => {
                self.delete_selected();
                self.screen = Screen::List;
            }
            Widget::Delete => {}
        }
        None
    }

    fn delete_selected(&mut self) {
        let Some(entry) = self.selected.and_then(|i| self.worlds.get(i)) else { return };
        // Only ever plain folder names straight inside the saves folder.
        if crate::data::validate_world_name(&entry.dir).is_err() {
            return;
        }
        let path = self.saves_dir.join(&entry.dir);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => log::info!("deleted world {}", path.display()),
            Err(e) => log::error!("failed to delete {}: {e}", path.display()),
        }
        self.worlds = list_worlds(&self.saves_dir);
        self.selected = (!self.worlds.is_empty()).then_some(0);
        self.scroll = 0;
    }

    /// Mouse position and screen size in UI pixels.
    fn ui_cursor(&self) -> ((f32, f32), (f32, f32)) {
        let scale = Ui::scale_for(self.shell.renderer.scale_factor());
        let (w, h) = self.shell.renderer.size();
        ((self.cursor_px.0 / scale, self.cursor_px.1 / scale), (w as f32 / scale, h as f32 / scale))
    }

    fn widget_under_cursor(&self) -> Option<(Widget, [f32; 4])> {
        let (mouse, size) = self.ui_cursor();
        self.layout(size).into_iter().find(|&(_, r)| inside(r, mouse))
    }

    /// The world list's box (x, y, w, h).
    fn list_rect((sw, sh): (f32, f32)) -> [f32; 4] {
        let top = 64.0;
        let bottom = sh - 2.0 * (BUTTON_H + 4.0) - 16.0;
        [((sw - LIST_W) / 2.0).floor(), top, LIST_W, (bottom - top).max(ROW_H)]
    }

    fn visible_rows(&self) -> usize {
        let (_, size) = self.ui_cursor();
        ((Self::list_rect(size)[3] - 4.0) / ROW_H).floor().max(1.0) as usize
    }

    fn layout(&self, (sw, sh): (f32, f32)) -> Vec<(Widget, [f32; 4])> {
        let cx = (sw / 2.0).floor();
        let mut out = Vec::new();
        match self.screen {
            Screen::List => {
                let [x, y, w, h] = Self::list_rect((sw, sh));
                let rows = ((h - 4.0) / ROW_H).floor().max(1.0) as usize;
                for (row, i) in (self.scroll..self.worlds.len()).take(rows).enumerate() {
                    out.push((Widget::World(i), [x + 2.0, y + 2.0 + row as f32 * ROW_H, w - 4.0, ROW_H - 2.0]));
                }
                let by = y + h + 8.0;
                let left = cx - HALF_W - 2.0;
                out.push((Widget::Play, [left, by, HALF_W, BUTTON_H]));
                out.push((Widget::Create, [cx + 2.0, by, HALF_W, BUTTON_H]));
                out.push((Widget::Delete, [left, by + BUTTON_H + 4.0, HALF_W, BUTTON_H]));
                out.push((Widget::Quit, [cx + 2.0, by + BUTTON_H + 4.0, HALF_W, BUTTON_H]));
            }
            Screen::Create => {
                let x = cx - BUTTON_W / 2.0;
                let y = (sh / 2.0 - 80.0).floor();
                out.push((Widget::NameField, [x, y + 12.0, BUTTON_W, BUTTON_H]));
                out.push((Widget::SeedField, [x, y + 52.0, BUTTON_W, BUTTON_H]));
                out.push((Widget::Mode, [x, y + 82.0, BUTTON_W, BUTTON_H]));
                out.push((Widget::CreateWorld, [cx - HALF_W - 2.0, y + 130.0, HALF_W, BUTTON_H]));
                out.push((Widget::Cancel, [cx + 2.0, y + 130.0, HALF_W, BUTTON_H]));
            }
            Screen::ConfirmDelete => {
                let y = (sh / 2.0 + 10.0).floor();
                out.push((Widget::ConfirmDelete, [cx - HALF_W - 2.0, y, HALF_W, BUTTON_H]));
                out.push((Widget::Cancel, [cx + 2.0, y, HALF_W, BUTTON_H]));
            }
        }
        out
    }

    fn label(&self, widget: Widget) -> String {
        match widget {
            Widget::Play => "Play World".into(),
            Widget::Create => "Create New World".into(),
            Widget::Delete => "Delete".into(),
            Widget::Quit => "Quit Game".into(),
            Widget::Mode => format!("Game Mode: {}", super::capitalize(self.mode.name())),
            Widget::CreateWorld => "Create World".into(),
            Widget::Cancel => "Cancel".into(),
            Widget::ConfirmDelete => "Delete".into(),
            Widget::World(_) | Widget::NameField | Widget::SeedField => String::new(),
        }
    }

    fn build_ui(&self) -> Vec<crate::render::ui::UiVertex> {
        let (w, h) = self.shell.renderer.size();
        let mut ui = Ui::new(w as f32, h as f32, self.shell.renderer.scale_factor());
        let (sw, sh) = ui.size();
        let centred = |ui: &mut Ui, y: f32, s: &str, color| {
            ui.text(((sw - Ui::text_width(s)) / 2.0).floor(), y, s, color);
        };
        let hovered = self.widget_under_cursor().map(|(w, _)| w);
        let grey = [0.63, 0.63, 0.63, 1.0];
        let blink = ((self.started.elapsed().as_secs_f32() * 2.0) as u32).is_multiple_of(2);

        match self.screen {
            Screen::List => {
                ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.25]);
                let title = "VoxelCraft";
                let k = 4.0;
                ui.text_scaled(((sw - Ui::text_width(title) * k) / 2.0).floor(), 16.0, title, WHITE, k);
                let [x, y, lw, lh] = Self::list_rect((sw, sh));
                ui.rect(x, y, lw, lh, [0.0, 0.0, 0.0, 0.8]);
                if self.worlds.is_empty() {
                    centred(&mut ui, y + lh / 2.0 - 4.0, "No worlds yet: create one!", grey);
                }
            }
            Screen::Create => {
                ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.6]);
                let y = (sh / 2.0 - 80.0).floor();
                centred(&mut ui, y - 24.0, "Create New World", WHITE);
                let x = (sw / 2.0 - BUTTON_W / 2.0).floor();
                ui.text(x, y, "World Name", grey);
                ui.text(x, y + 40.0, "Seed (blank for random)", grey);
                let hint = if self.mode == GameMode::Creative {
                    "Unlimited blocks, flying, no damage"
                } else {
                    "Gather, craft and stay alive"
                };
                centred(&mut ui, y + 108.0, hint, grey);
            }
            Screen::ConfirmDelete => {
                ui.rect(0.0, 0.0, sw, sh, [0.25, 0.0, 0.0, 0.75]);
                let name = self.selected.and_then(|i| self.worlds.get(i)).map_or("", |e| e.name.as_str());
                centred(&mut ui, sh / 2.0 - 30.0, &format!("Delete '{name}'?"), WHITE);
                centred(&mut ui, sh / 2.0 - 16.0, "It will be lost forever! (A long time!)", grey);
            }
        }

        for (widget, r @ [x, y, w, h]) in self.layout((sw, sh)) {
            let hot = hovered == Some(widget);
            match widget {
                Widget::World(i) => {
                    let e = &self.worlds[i];
                    if self.selected == Some(i) {
                        ui.rect(x, y, w, h, [0.75, 0.75, 0.75, 1.0]);
                        ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [0.05, 0.05, 0.05, 1.0]);
                    } else if hot {
                        ui.rect(x, y, w, h, [1.0, 1.0, 1.0, 0.08]);
                    }
                    ui.text(x + 6.0, y + 4.0, &e.name, WHITE);
                    let place = if e.nether { ", in the Nether" } else { "" };
                    let mut detail = format!("{}{place} - {}", super::capitalize(&e.mode), played_ago(e.played));
                    if e.dir != e.name {
                        detail = format!("{detail} ({})", e.dir);
                    }
                    ui.text(x + 6.0, y + 15.0, &fit(&detail, w - 12.0), grey);
                }
                Widget::NameField | Widget::SeedField => {
                    let focused = (widget == Widget::NameField) == (self.focus == Field::Name);
                    ui.rect(x, y, w, h, if focused { WHITE } else { [0.63, 0.63, 0.63, 1.0] });
                    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [0.0, 0.0, 0.0, 1.0]);
                    let text = if widget == Widget::NameField { &self.name } else { &self.seed };
                    let end = ui.text(x + 4.0, y + 6.0, text, [0.88, 0.88, 0.88, 1.0]);
                    if focused && blink {
                        ui.text(x + 5.0 + end, y + 6.0, "_", [0.88, 0.88, 0.88, 1.0]);
                    }
                }
                _ => {
                    let enabled = !matches!(widget, Widget::Play | Widget::Delete) || self.selected.is_some();
                    let fill = if !enabled {
                        [0.2, 0.2, 0.2, 1.0]
                    } else if hot {
                        HOT
                    } else {
                        BUTTON
                    };
                    bevel(&mut ui, r, fill, enabled);
                    let label = self.label(widget);
                    let color = match (enabled, hot) {
                        (false, _) => grey,
                        (true, true) => [1.0, 1.0, 0.63, 1.0],
                        (true, false) => WHITE,
                    };
                    ui.text((x + (w - Ui::text_width(&label)) / 2.0).floor(), y + 6.0, &label, color);
                }
            }
        }
        let version = format!("VoxelCraft {}", env!("CARGO_PKG_VERSION"));
        ui.text(2.0, sh - 10.0, &version, WHITE);
        ui.verts
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
            fov_y: 70f32.to_radians(),
            sky_color: sky.horizon.map(|c| c as f64),
            fog_color: sky.horizon,
            fog_start: 200.0,
            fog_end: 400.0,
            daylight: sky.daylight,
            zenith_color: sky.zenith,
            sun_dir: sky.sun_dir,
            time: t * 8.0,
            highlight: None,
            crack: None,
            block_models: Vec::new(),
            ui: self.build_ui(),
            rain: 0.0,
        };
        self.shell.renderer.render(&params);
    }
}

/// Saved worlds, most recently played first.
fn list_worlds(saves_dir: &Path) -> Vec<Entry> {
    let Ok(dir) = std::fs::read_dir(saves_dir) else { return Vec::new() };
    let mut worlds: Vec<Entry> = dir
        .flatten()
        .filter_map(|e| {
            let dir = e.file_name().into_string().ok()?;
            crate::data::validate_world_name(&dir).ok()?;
            let storage = Storage::new(e.path());
            if !storage.exists() {
                return None;
            }
            let level = storage.load_level().ok();
            let prop = |k: &str| level.as_ref().and_then(|l| l.props.get(k)).cloned();
            let played = std::fs::metadata(e.path().join("level.txt")).and_then(|m| m.modified()).ok();
            Some(Entry {
                name: prop("name").unwrap_or_else(|| dir.clone()),
                mode: prop("mode").unwrap_or_else(|| "survival".into()),
                nether: prop("dimension").as_deref() == Some("nether"),
                dir,
                played,
            })
        })
        .collect();
    worlds.sort_by(|a, b| b.played.cmp(&a.played).then_with(|| a.dir.cmp(&b.dir)));
    worlds
}

/// `text`, cut short with "..." if it's wider than `width` UI pixels.
fn fit(text: &str, width: f32) -> String {
    if Ui::text_width(text) <= width {
        return text.to_string();
    }
    let mut cut = text.to_string();
    while !cut.is_empty() && Ui::text_width(&format!("{cut}...")) > width {
        cut.pop();
    }
    format!("{}...", cut.trim_end())
}

fn played_ago(t: Option<SystemTime>) -> String {
    let Some(secs) = t.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()) else { return "never played".into() };
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match secs {
        0..60 => "just now".into(),
        60..3600 => plural(secs / 60, "minute"),
        3600..86400 => plural(secs / 3600, "hour"),
        _ => plural(secs / 86400, "day"),
    }
}

/// A save folder name for a world called `name`: letters and digits kept,
/// spaces turned into underscores, made unique with a number if `taken`.
fn folder_for(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut base: String = name
        .chars()
        .filter_map(|c| match c {
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => Some(c),
            ' ' => Some('_'),
            _ => None,
        })
        .take(NAME_MAX)
        .collect();
    if base.is_empty() || crate::data::validate_world_name(&base).is_err() {
        base = format!("world{base}");
    }
    let mut dir = base.clone();
    let mut n = 2;
    while taken(&dir) {
        dir = format!("{base}-{n}");
        n += 1;
    }
    dir
}

/// A seed from the create screen: a number as typed, any other text hashed
/// (like Minecraft), or `None` for a random one.
fn parse_seed(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(n) = text.parse::<i64>() {
        return Some(n as u64);
    }
    // FNV-1a.
    Some(text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_safe_and_unique() {
        assert_eq!(folder_for("My World", |_| false), "My_World");
        assert_eq!(folder_for("Ünïcode!?", |_| false), "ncode");
        assert_eq!(folder_for("???", |_| false), "world");
        assert_eq!(folder_for("con", |_| false), "worldcon");
        let taken = ["New_World", "New_World-2"];
        assert_eq!(folder_for("New World", |d| taken.contains(&d)), "New_World-3");
        for name in ["a b c", "x".repeat(80).as_str(), "nul"] {
            assert!(crate::data::validate_world_name(&folder_for(name, |_| false)).is_ok(), "{name}");
        }
    }

    #[test]
    fn seeds_parse_numbers_and_hash_text() {
        assert_eq!(parse_seed(""), None);
        assert_eq!(parse_seed(" 42 "), Some(42));
        assert_eq!(parse_seed("-1"), Some(u64::MAX));
        assert_eq!(parse_seed("glacier"), parse_seed("glacier"));
        assert_ne!(parse_seed("glacier"), parse_seed("Glacier"));
    }

    #[test]
    fn worlds_list_newest_first_with_names() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-title-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, props) in [("old", "name=Old Times\nmode=creative\n"), ("new", "")] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            std::fs::write(dir.join(folder).join("level.txt"), format!("seed=1\n{props}")).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::fs::create_dir_all(dir.join("not-a-world")).unwrap();
        let worlds = list_worlds(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(worlds.len(), 2, "{worlds:?}");
        assert_eq!(
            (worlds[0].dir.as_str(), worlds[0].name.as_str(), worlds[0].mode.as_str()),
            ("new", "new", "survival")
        );
        assert_eq!((worlds[1].name.as_str(), worlds[1].mode.as_str()), ("Old Times", "creative"));
        assert_eq!(played_ago(Some(SystemTime::now())), "just now");
        assert_eq!(fit("short", 100.0), "short");
        let long = fit("Survival, in the Nether - 12 minutes ago (My_Long_World_Name)", 200.0);
        assert!(long.ends_with("...") && Ui::text_width(&long) <= 200.0, "{long}");
    }
}
