//! Pause menu and options screen. Escape opens the pause menu (the game
//! stops simulating while it's up); options are saved to `saves/options.txt`
//! when leaving the options screen and on exit.
//!
//! Widgets are laid out by one function that both drawing and mouse
//! hit-testing use, like the inventory screen.

use crate::render::ui::{Color, Ui, WHITE};
use crate::simulation::difficulty::Difficulty;

use super::Game;
use super::settings::{FOV, RENDER_DISTANCE, SENSITIVITY, Settings};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Screen {
    Pause,
    Options,
    Lan,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Widget {
    Resume,
    Options,
    SaveAndQuit,
    OpenLan,
    LanMode,
    LanCheats,
    LanPvp,
    StartLan,
    RenderDistance,
    SimulationDistance,
    Fov,
    Sensitivity,
    Volume,
    MusicVolume,
    Vsync,
    Graphics,
    Fps,
    ViewBobbing,
    Fullscreen,
    Difficulty,
    Particles,
    CheckUpdates,
    Done,
}

/// What a click asks the app to do beyond the game itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MenuAction {
    Quit,
}

const BUTTON_W: f32 = 200.0;
const BUTTON_H: f32 = 20.0;
const GAP: f32 = 4.0;
/// Width of a slider's handle.
const HANDLE_W: f32 = 8.0;

impl Widget {
    fn is_slider(self) -> bool {
        matches!(
            self,
            Widget::RenderDistance
                | Widget::SimulationDistance
                | Widget::Fov
                | Widget::Sensitivity
                | Widget::Volume
                | Widget::MusicVolume
        )
    }

    fn label(self, s: &Settings, difficulty: Difficulty, hardcore: bool) -> String {
        match self {
            Widget::Resume => "Back to Game".into(),
            Widget::Options => "Options...".into(),
            Widget::SaveAndQuit => "Save and Quit to Title".into(),
            Widget::OpenLan => "Open to LAN".into(),
            Widget::LanMode => "Game Mode".into(),
            Widget::LanCheats => "Allow Cheats".into(),
            Widget::LanPvp => "PvP".into(),
            Widget::StartLan => "Start LAN World".into(),
            Widget::RenderDistance => format!("Render Distance: {} chunks", s.render_distance),
            Widget::SimulationDistance => format!("Simulation: {} Java chunks", s.simulation_distance),
            Widget::Fov => format!("FOV: {}", s.fov as i32),
            Widget::Sensitivity => format!("Sensitivity: {}%", (s.sensitivity * 100.0).round() as i32),
            Widget::Volume => match (s.volume * 100.0).round() as i32 {
                0 => "Volume: Off".into(),
                v => format!("Volume: {v}%"),
            },
            Widget::MusicVolume => match (s.music_volume * 100.0).round() as i32 {
                0 => "Music: Off".into(),
                v => format!("Music: {v}%"),
            },
            Widget::Vsync => format!("VSync: {}", if s.vsync { "On" } else { "Off" }),
            Widget::Graphics => format!("Graphics: {}", if s.enhanced_graphics { "Enhanced" } else { "Classic" }),
            Widget::Fps => format!("FPS Counter: {}", if s.show_fps { "On" } else { "Off" }),
            Widget::ViewBobbing => format!("View Bobbing: {}", if s.view_bobbing { "On" } else { "Off" }),
            Widget::Difficulty => format!("Difficulty: {difficulty}{}", if hardcore { " Locked" } else { "" }),
            Widget::Particles => format!("Particles: {}", s.particles.name()),
            Widget::Fullscreen => format!("Fullscreen: {}", s.fullscreen.name()),
            Widget::CheckUpdates => format!("Check for Updates: {}", if s.check_updates { "On" } else { "Off" }),
            Widget::Done => "Done".into(),
        }
    }

    /// Slider position 0..1 for the current setting.
    fn value(self, s: &Settings) -> f32 {
        let frac = |v: f32, (lo, hi): (f32, f32)| (v - lo) / (hi - lo);
        match self {
            Widget::RenderDistance => {
                frac(s.render_distance as f32, (RENDER_DISTANCE.0 as f32, RENDER_DISTANCE.1 as f32))
            }
            Widget::SimulationDistance => {
                let (lo, hi) = crate::world::activity::SIMULATION_DISTANCE_RANGE;
                frac(s.simulation_distance as f32, (lo as f32, hi as f32))
            }
            Widget::Fov => frac(s.fov, FOV),
            Widget::Sensitivity => frac(s.sensitivity, SENSITIVITY),
            Widget::Volume => s.volume,
            Widget::MusicVolume => s.music_volume,
            _ => 0.0,
        }
    }

    /// Sets the setting from a slider position 0..1.
    fn set_value(self, t: f32, s: &mut Settings) {
        let t = t.clamp(0.0, 1.0);
        let lerp = |(lo, hi): (f32, f32)| lo + (hi - lo) * t;
        match self {
            Widget::RenderDistance => {
                s.render_distance = lerp((RENDER_DISTANCE.0 as f32, RENDER_DISTANCE.1 as f32)).round() as i32
            }
            Widget::SimulationDistance => {
                let (lo, hi) = crate::world::activity::SIMULATION_DISTANCE_RANGE;
                s.simulation_distance = lerp((lo as f32, hi as f32)).round() as i32;
            }
            Widget::Fov => s.fov = lerp(FOV).round(),
            // Steps of 5%.
            Widget::Sensitivity => s.sensitivity = (lerp(SENSITIVITY) * 20.0).round() / 20.0,
            Widget::MusicVolume => s.music_volume = (t * 100.0).round() / 100.0,
            Widget::Volume => s.volume = (t * 100.0).round() / 100.0,
            _ => {}
        }
    }
}

/// Widgets of a screen and their rectangles (x, y, w, h) in UI pixels.
fn layout(screen: Screen, (sw, sh): (f32, f32)) -> Vec<(Widget, [f32; 4])> {
    let widgets: &[Widget] = match screen {
        Screen::Pause => &[Widget::Resume, Widget::Options, Widget::OpenLan, Widget::SaveAndQuit],
        Screen::Lan => &[Widget::LanMode, Widget::LanCheats, Widget::LanPvp, Widget::StartLan, Widget::Done],
        Screen::Options => &[
            Widget::RenderDistance,
            Widget::SimulationDistance,
            Widget::Fov,
            Widget::Sensitivity,
            Widget::Volume,
            Widget::MusicVolume,
            Widget::Vsync,
            Widget::Fullscreen,
            Widget::Graphics,
            Widget::Fps,
            Widget::ViewBobbing,
            Widget::Difficulty,
            Widget::Particles,
            Widget::CheckUpdates,
            Widget::Done,
        ],
    };
    // The added camera option also fits the shorter split-screen HUD.
    let gap = GAP.min(((sh - widgets.len() as f32 * BUTTON_H - 24.0) / (widgets.len() as f32 + 1.0)).max(1.0));
    let total = widgets.len() as f32 * BUTTON_H + (widgets.len() as f32 + 1.0) * gap + 20.0;
    let x = ((sw - BUTTON_W) / 2.0).floor();
    let mut y = ((sh - total) / 2.0).floor() + 20.0;
    let column = widgets
        .iter()
        .map(|&w| {
            // "Done" and "Save and Quit" sit a little apart from the rest.
            if matches!(w, Widget::Done | Widget::SaveAndQuit) {
                y += gap * 2.0;
            }
            let r = [x, y, BUTTON_W, BUTTON_H];
            y += BUTTON_H + gap;
            (w, r)
        })
        .collect::<Vec<_>>();
    match column.last() {
        Some((_, r)) if r[1] + r[3] > sh && widgets.len() > 5 => two_columns(widgets, (sw, sh)),
        _ => column,
    }
}

/// A long screen in a short window (a 1600 x 900 window on a 2x display is
/// 533 x 300 UI pixels): two columns, as Java lays out its Options, with
/// "Done" centred below.
fn two_columns(widgets: &[Widget], (sw, sh): (f32, f32)) -> Vec<(Widget, [f32; 4])> {
    let (items, done) = match widgets.split_last() {
        Some((&Widget::Done, items)) => (items, true),
        _ => (widgets, false),
    };
    let rows = items.len().div_ceil(2) + usize::from(done);
    let w = BUTTON_W.min(((sw - 3.0 * GAP) / 2.0).floor());
    let gap = GAP.min(((sh - rows as f32 * BUTTON_H - 24.0) / (rows as f32 + 2.0)).max(1.0));
    let total = rows as f32 * BUTTON_H + (rows as f32 + 2.0) * gap + 20.0;
    let top = ((sh - total) / 2.0).floor() + 20.0;
    let (left, right) = ((sw / 2.0 - w - GAP / 2.0).floor(), (sw / 2.0 + GAP / 2.0).floor());
    let mut out: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, &widget)| {
            let x = if i % 2 == 0 { left } else { right };
            (widget, [x, top + (i / 2) as f32 * (BUTTON_H + gap), w, BUTTON_H])
        })
        .collect();
    if done {
        let y = top + items.len().div_ceil(2) as f32 * (BUTTON_H + gap) + gap;
        out.push((Widget::Done, [((sw - BUTTON_W) / 2.0).floor(), y, BUTTON_W, BUTTON_H]));
    }
    out
}

pub(super) fn inside([x, y, w, h]: [f32; 4], (mx, my): (f32, f32)) -> bool {
    mx >= x && mx < x + w && my >= y && my < y + h
}

impl Game {
    /// Mouse position in UI pixels, and the screen size in UI pixels.
    fn menu_cursor(&self) -> ((f32, f32), (f32, f32)) {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        ((self.cursor_px.0 / scale, self.cursor_px.1 / scale), (w as f32 / scale, h as f32 / scale))
    }

    fn widget_under_cursor(&self) -> Option<(Widget, [f32; 4])> {
        let screen = self.menu?;
        let (mouse, size) = self.menu_cursor();
        layout(screen, size).into_iter().find(|&(_, r)| inside(r, mouse))
    }

    /// Opens the pause menu: frees the mouse and stops every action.
    pub(super) fn open_menu(&mut self) {
        if self.inventory_open {
            self.toggle_inventory();
        }
        self.menu = Some(Screen::Pause);
        self.clock.advance(std::time::Duration::ZERO, true);
        self.previous_eye = self.session.player.eye();
        self.mobs.entities.snapshot_positions();
        self.world.snapshot_falling_positions();
        self.menu_drag = None;
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

    /// Escape inside the menus: options back to pause, pause back to the game.
    pub(super) fn menu_back(&mut self) {
        match self.menu {
            Some(Screen::Lan) => self.menu = Some(Screen::Pause),
            Some(Screen::Options) => {
                self.save_settings();
                self.menu = Some(Screen::Pause);
            }
            Some(Screen::Pause) => {
                self.menu = None;
                // A hidden/minimized window may not have redrawn while
                // paused. Do not charge that elapsed wall time on resume.
                self.last_frame = std::time::Instant::now();
                self.set_grab(true);
            }
            None => {}
        }
        self.menu_drag = None;
    }

    /// A mouse button press or release while a menu is open.
    pub(super) fn menu_click(&mut self, pressed: bool) -> Option<MenuAction> {
        if !pressed {
            self.menu_drag = None;
            return None;
        }
        let (widget, rect) = self.widget_under_cursor()?;
        self.audio.ui_click();
        if widget.is_slider() {
            self.menu_drag = Some(widget);
            self.drag_slider(widget, rect);
            return None;
        }
        match widget {
            Widget::Resume => self.menu_back(),
            Widget::Options => self.menu = Some(Screen::Options),
            Widget::OpenLan => {
                if self.lan.host.is_none() && self.lan.client.is_none() {
                    self.menu = Some(Screen::Lan);
                } else {
                    self.show_popup("World is already connected to LAN");
                }
            }
            Widget::LanMode => {
                self.lan.mode = match self.lan.mode {
                    super::GameMode::Survival => super::GameMode::Creative,
                    _ => super::GameMode::Survival,
                }
            }
            Widget::LanCheats => self.lan.cheats = !self.lan.cheats,
            Widget::LanPvp => self.lan.pvp = !self.lan.pvp,
            Widget::StartLan => {
                if let Err(e) = self.open_lan("0.0.0.0:0".parse().unwrap()) {
                    self.show_popup(&e);
                } else {
                    self.menu = None;
                    self.set_grab(true);
                }
            }
            Widget::Done => self.menu_back(),
            Widget::Vsync => {
                self.settings.vsync = !self.settings.vsync;
                self.apply_settings();
            }
            Widget::Graphics => {
                self.settings.enhanced_graphics = !self.settings.enhanced_graphics;
                self.apply_settings();
            }
            Widget::Fps => {
                self.settings.show_fps = !self.settings.show_fps;
                self.apply_settings();
            }
            Widget::ViewBobbing => {
                self.settings.view_bobbing = !self.settings.view_bobbing;
                self.apply_settings();
            }
            // Takes effect at the next launch's title screen.
            Widget::CheckUpdates => self.settings.check_updates = !self.settings.check_updates,
            Widget::Difficulty => {
                if !self.hardcore {
                    self.difficulty = self.difficulty.next();
                    if self.difficulty == Difficulty::Peaceful {
                        self.mobs.entities.despawn_hostiles();
                    }
                    self.show_popup(&format!("Difficulty: {}", self.difficulty));
                }
            }
            Widget::Particles => self.settings.particles = self.settings.particles.next(),
            Widget::Fullscreen => {
                self.settings.fullscreen = self.settings.fullscreen.next();
                self.settings.fullscreen.apply(&self.renderer.window);
            }
            Widget::SaveAndQuit => return Some(MenuAction::Quit),
            _ => {}
        }
        None
    }

    /// Mouse moved: a held slider follows it.
    pub(super) fn menu_cursor_moved(&mut self) {
        let (Some(widget), Some(screen)) = (self.menu_drag, self.menu) else { return };
        let (_, size) = self.menu_cursor();
        if let Some((_, rect)) = layout(screen, size).into_iter().find(|&(w, _)| w == widget) {
            self.drag_slider(widget, rect);
        }
    }

    fn drag_slider(&mut self, widget: Widget, [x, _, w, _]: [f32; 4]) {
        let ((mx, _), _) = self.menu_cursor();
        let t = (mx - x - HANDLE_W / 2.0) / (w - HANDLE_W);
        let before = self.settings;
        widget.set_value(t, &mut self.settings);
        if self.settings != before {
            self.apply_settings();
        }
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
        self.audio.set_volume(s.volume);
        self.audio.set_music_volume(s.music_volume);
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
        let Some(screen) = self.menu else { return };
        let (sw, sh) = ui.size();
        // Blending is in linear space: it takes a high alpha to look dark.
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.78]);
        let widgets = layout(screen, (sw, sh));
        let title = match screen {
            Screen::Pause => "Game Menu",
            Screen::Options => "Options",
            Screen::Lan => "Open to LAN",
        };
        let top = widgets.first().map_or(sh / 2.0, |(_, r)| r[1]);
        ui.text(((sw - Ui::text_width(title)) / 2.0).floor(), top - 20.0, title, WHITE);

        let hovered = self.widget_under_cursor().map(|(w, _)| w);
        for (widget, [x, y, w, h]) in widgets {
            let hot = hovered == Some(widget) || self.menu_drag == Some(widget);
            let label = match widget {
                Widget::LanMode => format!("Game Mode: {}", super::capitalize(self.lan.mode.name())),
                Widget::LanCheats => format!("Allow Cheats: {}", if self.lan.cheats { "On" } else { "Off" }),
                Widget::LanPvp => format!("PvP: {}", if self.lan.pvp { "On" } else { "Off" }),
                Widget::OpenLan if self.lan.host.is_some() => {
                    format!("LAN Port: {}", self.lan.host.as_ref().unwrap().address.port())
                }
                Widget::OpenLan if self.lan.client.is_some() => "Connected to LAN".into(),
                _ => widget.label(&self.settings, self.difficulty, self.hardcore),
            };
            if widget.is_slider() {
                // Minecraft-style: a dark track with a button-like handle.
                bevel(ui, [x, y, w, h], [0.16, 0.16, 0.16, 1.0], false);
                let hx = x + (w - HANDLE_W) * widget.value(&self.settings).clamp(0.0, 1.0);
                bevel(ui, [hx, y, HANDLE_W, h], if hot { HOT } else { BUTTON }, true);
            } else {
                bevel(ui, [x, y, w, h], if hot { HOT } else { BUTTON }, true);
            }
            let color = if hot { [1.0, 1.0, 0.63, 1.0] } else { WHITE };
            ui.text((x + (w - Ui::text_width(&label)) / 2.0).floor(), y + 6.0, &label, color);
        }
    }
}

pub(super) const BUTTON: Color = [0.42, 0.42, 0.42, 1.0];
pub(super) const HOT: Color = [0.48, 0.52, 0.72, 1.0];

/// A filled box with a black outline and (if `raised`) light top-left and
/// dark bottom-right edges.
pub(super) fn bevel(ui: &mut Ui, [x, y, w, h]: [f32; 4], fill: Color, raised: bool) {
    ui.rect(x, y, w, h, [0.0, 0.0, 0.0, 1.0]);
    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, fill);
    if raised {
        let light = [fill[0] + 0.25, fill[1] + 0.25, fill[2] + 0.25, 1.0];
        let dark = [fill[0] * 0.55, fill[1] * 0.55, fill[2] * 0.55, 1.0];
        ui.rect(x + 1.0, y + 1.0, w - 2.0, 1.0, light);
        ui.rect(x + 1.0, y + 1.0, 1.0, h - 2.0, light);
        ui.rect(x + 1.0, y + h - 3.0, w - 2.0, 2.0, dark);
        ui.rect(x + w - 2.0, y + 1.0, 1.0, h - 2.0, dark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliders_map_both_ways_across_their_range() {
        let mut s = Settings::default();
        for w in [Widget::RenderDistance, Widget::Fov, Widget::Sensitivity, Widget::Volume, Widget::MusicVolume] {
            w.set_value(0.0, &mut s);
            assert!(w.value(&s).abs() < 1e-4, "{w:?} at min");
            w.set_value(1.0, &mut s);
            assert!((w.value(&s) - 1.0).abs() < 1e-4, "{w:?} at max");
            w.set_value(5.0, &mut s);
            assert!((w.value(&s) - 1.0).abs() < 1e-4, "{w:?} clamps");
        }
        assert_eq!(s.clamped(), s, "slider values are always valid settings");
        Widget::RenderDistance.set_value(0.2, &mut s);
        assert_eq!(s.render_distance, 8);
        assert_eq!(Widget::RenderDistance.label(&s, Difficulty::Normal, false), "Render Distance: 8 chunks");
        // Every label fits inside its button.
        for w in [
            Widget::RenderDistance,
            Widget::SimulationDistance,
            Widget::Fov,
            Widget::Sensitivity,
            Widget::Volume,
            Widget::MusicVolume,
            Widget::Vsync,
            Widget::Graphics,
            Widget::Fps,
            Widget::ViewBobbing,
            Widget::Difficulty,
            Widget::Particles,
            Widget::Fullscreen,
            Widget::CheckUpdates,
        ] {
            let longest = Settings {
                render_distance: 32,
                simulation_distance: 32,
                fov: 110.0,
                sensitivity: 3.0,
                volume: 1.0,
                music_volume: 1.0,
                vsync: false,
                enhanced_graphics: true,
                show_fps: true,
                view_bobbing: true,
                particles: crate::particles::Setting::Decreased,
                fullscreen: super::super::fullscreen::Mode::Borderless,
                check_updates: true,
                skip_update: None,
            };
            assert!(
                Ui::text_width(&w.label(&longest, Difficulty::Hard, false)) < BUTTON_W - 8.0,
                "{w:?} label too wide"
            );
        }
    }

    /// A 1600 x 900 window on a 2x display gives 533 x 300 UI pixels: the
    /// Options screen switches to two columns and keeps every button on screen.
    #[test]
    fn short_windows_fit_every_option() {
        for size in [(533.0, 300.0), (480.0, 270.0), (640.0, 360.0)] {
            let widgets = layout(Screen::Options, size);
            assert_eq!(widgets.len(), 15);
            for (i, &(w, r)) in widgets.iter().enumerate() {
                assert!(r[0] >= 0.0 && r[0] + r[2] <= size.0, "{w:?} on screen at {size:?}");
                assert!(r[1] > 0.0 && r[1] + r[3] <= size.1, "{w:?} on screen at {size:?}");
                let centre = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
                assert_eq!(widgets.iter().position(|&(_, r)| inside(r, centre)), Some(i), "{w:?} overlaps");
            }
            let done = widgets.last().unwrap();
            assert_eq!(done.0, Widget::Done);
            assert!((done.1[0] + done.1[2] / 2.0 - size.0 / 2.0).abs() <= 0.5, "Done centred at {size:?}");
        }
        // Short screens keep their single column.
        assert!(layout(Screen::Pause, (533.0, 300.0)).iter().all(|(_, r)| r[0] + r[2] / 2.0 == 266.0));
        let two = |size| layout(Screen::Options, size).iter().any(|(_, r)| r[0] + r[2] / 2.0 != size.0 / 2.0);
        assert!(two((533.0, 300.0)) && !two((640.0, 360.0)), "two columns only when one doesn't fit");
    }

    #[test]
    fn layout_is_centred_and_hit_testable() {
        for screen in [Screen::Pause, Screen::Options, Screen::Lan] {
            let size = (640.0, 360.0);
            let widgets = layout(screen, size);
            for (i, &(w, r)) in widgets.iter().enumerate() {
                assert_eq!(r[0] + r[2] / 2.0, 320.0, "{w:?} centred");
                assert!(r[1] > 0.0 && r[1] + r[3] < size.1, "{w:?} on screen");
                let centre = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
                assert_eq!(widgets.iter().position(|&(_, r)| inside(r, centre)), Some(i), "{w:?} overlaps");
            }
        }
    }
}
