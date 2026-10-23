//! The pause menu and Open to LAN screen, on the shared menu toolkit
//! ([`gui`](super::gui)), and the game's side of the Options screens.
//! Escape opens the pause menu (the game stops simulating while it's up).

use winit::event::{KeyEvent, MouseButton};

use super::gui::{self, Key, Nav, Screen, Size, Stack, Step, Widget};
use super::options::{self, OptionsHost};
use super::settings::Settings;
use super::{Game, GameMode};
use crate::render::ui::Ui;
use crate::simulation::difficulty::Difficulty;

/// What the menus ask the app to do beyond the game itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MenuAction {
    Quit,
}

impl gui::Host for Game {
    type Out = MenuAction;

    fn click(&mut self) {
        self.audio.ui_click();
    }

    fn in_world(&self) -> bool {
        true
    }
}

impl OptionsHost for Game {
    fn settings(&self) -> &Settings {
        &self.settings
    }

    fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    fn apply_settings(&mut self) {
        Game::apply_settings(self);
    }

    fn save_settings(&self) {
        Game::save_settings(self);
    }

    fn apply_fullscreen(&mut self) {
        self.settings.fullscreen.apply(&self.renderer.window);
    }

    fn window_size(&self) -> Option<(u32, u32)> {
        Some(self.renderer.size())
    }

    fn difficulty(&self) -> Option<options::WorldDifficulty> {
        Some(options::WorldDifficulty {
            level: self.difficulty,
            hardcore: self.hardcore,
            locked: self.difficulty_locked,
        })
    }

    fn lock_difficulty(&mut self) {
        self.difficulty_locked = true;
    }

    fn cycle_difficulty(&mut self) {
        if self.hardcore || self.difficulty_locked {
            return;
        }
        self.difficulty = self.difficulty.next();
        if self.difficulty == Difficulty::Peaceful {
            self.mobs.entities.despawn_hostiles();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PauseId {
    Resume,
    Options,
    Lan,
    Quit,
}

/// Java's Game Menu, without the Advancements, Statistics and feedback
/// buttons VoxelCraft has no screens for.
pub(super) struct Pause;

impl Screen<Game> for Pause {
    type Id = PauseId;

    fn title(&self, _: &Game) -> String {
        "Game Menu".into()
    }

    fn widgets(&self, game: &Game, size: Size) -> Vec<Widget<PauseId>> {
        let z = [0.0; 4];
        let lan = match (&game.lan.host, &game.lan.client) {
            (Some(host), _) => format!("LAN Port: {}", host.address.port()),
            (_, Some(_)) => "Connected to LAN".into(),
            _ => "Open to LAN".into(),
        };
        gui::centred_rows(
            vec![
                vec![Widget::button(PauseId::Resume, z, "Back to Game")],
                vec![Widget::button(PauseId::Options, z, "Options..."), Widget::button(PauseId::Lan, z, lan)],
                vec![Widget::button(PauseId::Quit, z, "Save and Quit to Title")],
            ],
            204.0,
            size,
        )
    }

    fn activate(&mut self, game: &mut Game, id: PauseId) -> Step<Game> {
        match id {
            PauseId::Resume => Step::Close,
            PauseId::Options => Step::push(options::Options),
            PauseId::Lan if game.lan.host.is_none() && game.lan.client.is_none() => Step::push(OpenLan),
            PauseId::Lan => {
                game.show_popup("World is already connected to LAN");
                Step::Stay
            }
            PauseId::Quit => Step::Out(MenuAction::Quit),
        }
    }

    fn title_y(&self, _: Size, first: f32) -> f32 {
        first - 20.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LanId {
    Mode,
    Cheats,
    Pvp,
    Start,
    Cancel,
}

pub(super) struct OpenLan;

impl Screen<Game> for OpenLan {
    type Id = LanId;

    fn title(&self, _: &Game) -> String {
        "Open to LAN".into()
    }

    fn widgets(&self, game: &Game, size: Size) -> Vec<Widget<LanId>> {
        let z = [0.0; 4];
        let lan = &game.lan;
        gui::centred_rows(
            vec![
                vec![
                    Widget::button(LanId::Mode, z, format!("Game Mode: {}", super::capitalize(lan.mode.name()))),
                    Widget::button(LanId::Cheats, z, format!("Allow Cheats: {}", gui::on_off(lan.cheats))),
                ],
                vec![Widget::button(LanId::Pvp, z, format!("PvP: {}", gui::on_off(lan.pvp)))],
                vec![Widget::button(LanId::Start, z, "Start LAN World"), Widget::button(LanId::Cancel, z, "Cancel")],
            ],
            310.0,
            size,
        )
    }

    fn activate(&mut self, game: &mut Game, id: LanId) -> Step<Game> {
        match id {
            LanId::Mode => {
                game.lan.mode = match game.lan.mode {
                    GameMode::Survival => GameMode::Creative,
                    _ => GameMode::Survival,
                }
            }
            LanId::Cheats => game.lan.cheats = !game.lan.cheats,
            LanId::Pvp => game.lan.pvp = !game.lan.pvp,
            LanId::Start => match game.open_lan("0.0.0.0:0".parse().unwrap()) {
                Ok(()) => return Step::Close,
                Err(e) => game.show_popup(&e),
            },
            LanId::Cancel => return Step::Pop,
        }
        Step::Stay
    }

    fn title_y(&self, _: Size, first: f32) -> f32 {
        first - 20.0
    }
}

/// The in-world menus `--open-menu` can start with.
pub(super) const STARTUP_MENUS: [&str; 9] =
    ["pause", "lan", "options", "video", "sound", "controls", "mouse", "keybinds", "accessibility"];

/// The stack `--open-menu` starts a world with: the pause menu, then
/// Options and the screens above it, as a player reaches them.
pub(super) fn startup_menu(name: Option<&str>) -> Stack<Game> {
    let mut stack = Stack::default();
    if name.is_none_or(|m| !STARTUP_MENUS.contains(&m)) {
        return stack;
    }
    stack.open(Pause);
    if name != Some("pause") && name != Some("lan") {
        stack.open(options::Options);
    }
    match name {
        Some("lan") => stack.open(OpenLan),
        Some("video") => stack.open(options::VideoSettings),
        Some("sound") => stack.open(options::MusicAndSounds),
        Some("controls") => stack.open(options::Controls),
        Some("mouse") => {
            stack.open(options::Controls);
            stack.open(options::MouseSettings);
        }
        Some("keybinds") => {
            stack.open(options::Controls);
            stack.open(options::KeyBinds::default());
        }
        Some("accessibility") => stack.open(options::Accessibility),
        _ => {}
    }
    stack
}

impl Game {
    /// Mouse position in UI pixels, and the screen size in UI pixels.
    fn menu_view(&self) -> ((f32, f32), Size) {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        ((self.cursor_px.0 / scale, self.cursor_px.1 / scale), (w as f32 / scale, h as f32 / scale))
    }

    /// Runs `f` on the menu stack with the game as its host; resumes play
    /// when it closes the last screen.
    fn with_menu<T>(&mut self, f: impl FnOnce(&mut Stack<Game>, &mut Game, Size) -> T) -> T {
        let mut stack = std::mem::take(&mut self.menu);
        let (_, size) = self.menu_view();
        let was_open = stack.is_open();
        let out = f(&mut stack, self, size);
        self.menu = stack;
        if was_open && !self.menu.is_open() {
            self.resume_from_menu();
        }
        out
    }

    /// Opens the pause menu: frees the mouse and stops every action.
    pub(super) fn open_menu(&mut self) {
        if self.inventory_open {
            self.toggle_inventory();
        }
        self.menu.close();
        self.menu.open(Pause);
        self.clock.advance(std::time::Duration::ZERO, true);
        self.previous_eye = self.session.player.eye();
        self.mobs.entities.snapshot_positions();
        self.world.snapshot_falling_positions();
        self.set_grab(false);
        self.keys.clear();
        self.body.left_held = false;
        self.body.right_held = false;
        self.jump_pressed = false;
        self.body.mine_pressed = false;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
        self.release_attack();
    }

    fn resume_from_menu(&mut self) {
        // A hidden/minimized window may not have redrawn while paused. Do
        // not charge that elapsed wall time on resume.
        self.last_frame = std::time::Instant::now();
        self.set_grab(true);
    }

    /// A key press while a menu is open.
    pub(super) fn menu_key(&mut self, event: &KeyEvent) -> Option<MenuAction> {
        let key = Key::from_event(event, self.modifiers)?;
        self.with_menu(|stack, game, size| stack.key(game, &key, size))
    }

    /// A mouse button press or release while a menu is open.
    pub(super) fn menu_click(&mut self, button: MouseButton, pressed: bool) -> Option<MenuAction> {
        self.with_menu(|stack, game, size| stack.mouse_button(game, button, pressed, size))
    }

    /// Mouse moved: hover, and a held slider follows it.
    pub(super) fn menu_cursor_moved(&mut self) {
        if !self.menu.is_open() {
            return;
        }
        let (pos, _) = self.menu_view();
        self.with_menu(|stack, game, size| stack.mouse_moved(game, pos, size));
    }

    pub(super) fn menu_wheel(&mut self, lines: f32) {
        self.with_menu(|stack, game, size| stack.wheel(game, lines, size));
    }

    /// Once a frame while a menu is open: screens that run on their own
    /// (the Credits roll) advance and can close themselves.
    pub(super) fn menu_tick(&mut self) {
        if !self.menu.is_open() {
            return;
        }
        if let Some(action) = self.with_menu(|stack, game, size| stack.tick(game, size)) {
            self.menu_action = Some(action);
        }
    }

    /// Controller navigation from a controller that hasn't joined as a player.
    pub(super) fn menu_nav(&mut self, nav: Nav) -> Option<MenuAction> {
        self.with_menu(|stack, game, size| stack.nav(game, nav, size))
    }

    /// Pushes the settings to the world, renderer and audio.
    pub(super) fn apply_settings(&mut self) {
        if self.lan.client.is_some() {
            // Keep the negotiated LAN distance within the range the host serves;
            // the player's own setting is restored on leaving (`offline_render_distance`).
            self.settings.render_distance = self.settings.render_distance.clamp(2, super::lan::LAN_VIEW_MAX);
        }
        let s = self.settings;
        if self.world.render_distance() != s.render_distance {
            self.world.set_render_distance(s.render_distance);
        }
        self.world.set_simulation_distance(s.simulation_distance);
        if self.renderer.vsync() != s.vsync {
            self.renderer.set_vsync(s.vsync);
        }
        self.renderer.set_brightness(s.brightness);
        self.audio.set_volumes(|source| s.sound_volume(source));
    }

    pub(super) fn save_settings(&self) {
        if let Some(path) = &self.settings_path
            && let Err(e) = {
                let mut settings = self.settings;
                if let Some(distance) = self.lan.offline_render_distance {
                    settings.render_distance = distance;
                }
                settings.save(path)
            }
        {
            log::error!("failed to save options to {}: {e}", path.display());
        }
    }

    pub(super) fn menu_ui(&self, ui: &mut Ui) {
        self.menu.draw(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_and_lan_rows_are_centred_and_hit_testable() {
        for size in [(640.0, 360.0), (533.0, 300.0), (320.0, 180.0)] {
            let pause = gui::centred_rows(
                vec![
                    vec![Widget::button(0, [0.0; 4], "Back to Game")],
                    vec![Widget::button(1, [0.0; 4], "Options..."), Widget::button(2, [0.0; 4], "Open to LAN")],
                    vec![Widget::button(3, [0.0; 4], "Save and Quit to Title")],
                ],
                204.0,
                size,
            );
            let centre = |r: [f32; 4]| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            assert_eq!(centre(pause[0].rect).0, (size.0 / 2.0).floor());
            assert!(pause[0].rect[1] >= 20.0, "room for the title at {size:?}");
            for (i, w) in pause.iter().enumerate() {
                assert!(w.rect[1] + w.rect[3] <= size.1, "{i} on screen at {size:?}");
                assert_eq!(pause.iter().position(|o| gui::inside(o.rect, centre(w.rect))), Some(i), "{i} overlaps");
                if let gui::Kind::Button(label) = &w.kind {
                    assert!(Ui::text_width(label) <= w.rect[2] - 8.0, "{label} fits");
                }
            }
        }
    }
}
