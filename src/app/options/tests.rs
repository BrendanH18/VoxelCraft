//! Every Options screen at every window size, driven through the toolkit.

use std::cell::Cell;

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

use super::*;
use crate::app::gui::{Key, Kind, Nav, Stack, inside};
use crate::app::keys::{Action, Input};
use crate::audio::SoundSource;
use crate::render::ui::Ui;

/// An options host without a window, renderer or world.
#[derive(Default)]
pub(in crate::app) struct Host {
    pub settings: Settings,
    pub applied: u32,
    pub saves: Cell<u32>,
    pub fullscreen: u32,
    pub world: Option<WorldDifficulty>,
    pub window: Option<(u32, u32)>,
}

impl super::super::gui::Host for Host {
    type Out = ();
    fn click(&mut self) {}
    fn in_world(&self) -> bool {
        self.world.is_some()
    }
}

impl OptionsHost for Host {
    fn settings(&self) -> &Settings {
        &self.settings
    }
    fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }
    fn apply_settings(&mut self) {
        self.applied += 1;
    }
    fn save_settings(&self) {
        self.saves.set(self.saves.get() + 1);
    }
    fn apply_fullscreen(&mut self) {
        self.fullscreen += 1;
    }
    fn window_size(&self) -> Option<(u32, u32)> {
        self.window
    }
    fn difficulty(&self) -> Option<WorldDifficulty> {
        self.world
    }
    fn cycle_difficulty(&mut self) {
        if let Some(d) = &mut self.world
            && !d.hardcore
            && !d.locked
        {
            d.level = d.level.next();
        }
    }
    fn lock_difficulty(&mut self) {
        if let Some(d) = &mut self.world {
            d.locked = true;
        }
    }
}

fn world(level: Difficulty, hardcore: bool, locked: bool) -> Option<WorldDifficulty> {
    Some(WorldDifficulty { level, hardcore, locked })
}

/// From the smallest UI a split-screen view allows to a large window.
const SIZES: [Size; 6] =
    [(320.0, 180.0), (320.0, 240.0), (480.0, 270.0), (533.0, 300.0), (640.0, 360.0), (960.0, 540.0)];

/// Settings with the longest labels.
fn longest() -> Settings {
    let mut keys = crate::app::keys::Bindings::default();
    // A conflict, so the bracketed label and its stripe are laid out too.
    keys.set(Action::Drop, Input::Key(KeyCode::KeyW));
    keys.set(Action::Inventory, Input::Mouse(MouseButton::Forward));
    Settings {
        render_distance: 32,
        simulation_distance: 32,
        fov: 109.0,
        sensitivity: 2.95,
        volume: 0.99,
        music_volume: 0.99,
        record_volume: 0.99,
        weather_volume: 0.99,
        block_volume: 0.99,
        hostile_volume: 0.99,
        neutral_volume: 0.99,
        player_volume: 0.99,
        ambient_volume: 0.99,
        ui_volume: 0.99,
        enhanced_graphics: true,
        particles: crate::particles::Setting::Decreased,
        fullscreen: crate::app::fullscreen::Mode::Borderless,
        max_fps: 250,
        brightness: 0.99,
        gui_scale: 3,
        fov_effects: 0.99,
        distortion: 0.99,
        damage_tilt: 0.99,
        toggle_sneak: true,
        toggle_sprint: true,
        scroll_sensitivity: 0.0123,
        keys,
        ..Settings::default()
    }
}

fn check<S: Screen<Host>>(make: impl Fn() -> S, name: &str) {
    let worlds = [None, world(Difficulty::Peaceful, false, false), world(Difficulty::Hard, true, true)];
    for world in worlds {
        for size in SIZES {
            let host = Host { settings: longest(), world, window: Some((1920, 1080)), ..Host::default() };
            let screen = make();
            let widgets = screen.widgets(&host, size);
            let list = screen.list(&host, size);
            for w in &widgets {
                let [x, y, rw, rh] = w.rect;
                assert!(x >= 0.0 && x + rw <= size.0, "{name} {:?} on screen at {size:?}", w.id);
                // Labels wider than their widget scroll, as in Java.
                if let Kind::Button(l) | Kind::Slider { label: l, .. } = &w.kind {
                    assert!(Ui::text_width(l) <= rw * 1.6, "{name} {l:?} is readable while it scrolls");
                }
                if !w.scrolls {
                    assert!(y >= 20.0 && y + rh <= size.1, "{name} {:?} on screen at {size:?}", w.id);
                }
            }
            // The list stays clear of the title and the footer's Done.
            if let Some([_, top, _, h]) = list {
                assert!(top >= 20.0 && top + h <= size.1 - 20.0, "{name} list at {size:?}");
            }
            // Tab reaches every widget, each on screen and clickable when focused.
            let mut stack: Stack<Host> = Stack::default();
            stack.open(make());
            let mut host = host;
            let focusable = widgets.iter().filter(|w| w.enabled && !matches!(w.kind, Kind::Label(_))).count();
            let mut seen = Vec::new();
            for _ in 0..focusable {
                stack.key(&mut host, &Key::new(KeyCode::Tab), size);
                let (shape, focus) = stack.peek(&host, size);
                let i = focus.expect("focused");
                assert!(shape.slots[i].shown, "{name} widget {i} scrolled into view at {size:?}");
                let [x, y, w, h] = shape.slots[i].rect;
                let hit = shape.slots.iter().position(|s| s.shown && inside(s.rect, (x + w / 2.0, y + h / 2.0)));
                assert_eq!(hit, Some(i), "{name} widget {i} overlaps another at {size:?}");
                seen.push(i);
            }
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), focusable, "{name}: Tab reaches every widget at {size:?}");
        }
    }
}

#[test]
fn every_options_screen_fits_and_every_widget_is_reachable() {
    check(|| Options, "Options");
    check(|| VideoSettings, "Video Settings");
    check(|| MusicAndSounds, "Music & Sounds");
    check(|| Controls, "Controls");
    check(|| MouseSettings, "Mouse Settings");
    check(|| Accessibility, "Accessibility Settings");
    check(KeyBinds::default, "Key Binds");
    check(|| credits::CreditsAndAttribution, "Credits and Attribution");
}

fn slider_value<S: Screen<Host>>(screen: &S, host: &Host, id: S::Id) -> Option<f32> {
    screen.widgets(host, (640.0, 360.0)).into_iter().find_map(|w| match w.kind {
        Kind::Slider { value, .. } if w.id == id => Some(value),
        _ => None,
    })
}

fn label<S: Screen<Host>>(screen: &S, host: &Host, id: S::Id) -> String {
    screen
        .widgets(host, (640.0, 360.0))
        .into_iter()
        .find_map(|w| match w.kind {
            Kind::Button(l) | Kind::Slider { label: l, .. } | Kind::Label(l) if w.id == id => Some(l),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{id:?} has a label"))
}

fn sliders<S: Screen<Host>>(mut screen: S) {
    let mut host = Host::default();
    let ids: Vec<_> = screen
        .widgets(&host, (640.0, 360.0))
        .into_iter()
        .filter(|w| matches!(w.kind, Kind::Slider { .. }))
        .map(|w| w.id)
        .collect();
    assert!(!ids.is_empty());
    for id in ids {
        for t in [0.0, 1.0, 0.5] {
            screen.slide(&mut host, id, t);
            let v = slider_value(&screen, &host, id).unwrap();
            assert!((v - t).abs() < 0.03, "{id:?} at {t} reads {v}");
            assert_eq!(host.settings.clamped(), host.settings, "{id:?} gives valid settings");
        }
    }
    assert!(host.applied > 0, "slider changes are applied");
}

#[test]
fn sliders_cover_their_whole_range_both_ways() {
    sliders(Options);
    sliders(VideoSettings);
    sliders(MusicAndSounds);
    sliders(MouseSettings);
    sliders(Accessibility);
    let mut host = Host::default();
    VideoSettings.slide(&mut host, video::Id::RenderDistance, 0.2);
    assert_eq!(host.settings.render_distance, 8);
    assert_eq!(label(&VideoSettings, &host, video::Id::RenderDistance), "Render Distance: 16 Chunks");
}

#[test]
fn max_framerate_and_brightness_map_like_java() {
    let mut host = Host::default();
    let mut video = VideoSettings;
    assert_eq!(label(&video, &host, video::Id::MaxFps), "Max Framerate: 120 fps");
    video.slide(&mut host, video::Id::MaxFps, 1.0);
    assert_eq!((host.settings.max_fps, host.settings.frame_cap()), (260, None));
    assert_eq!(label(&video, &host, video::Id::MaxFps), "Max Framerate: Unlimited");
    video.slide(&mut host, video::Id::MaxFps, 0.0);
    assert_eq!(host.settings.frame_cap(), Some(10));
    // One arrow-key step is ten frames a second.
    let mut stack: Stack<Host> = Stack::default();
    stack.open(VideoSettings);
    stack.key(&mut host, &Key::new(KeyCode::Tab), (640.0, 360.0));
    stack.key(&mut host, &Key::new(KeyCode::ArrowRight), (640.0, 360.0));
    assert_eq!(host.settings.max_fps, 20);

    assert_eq!(label(&video, &host, video::Id::Brightness), "Brightness: Default");
    video.slide(&mut host, video::Id::Brightness, 0.0);
    assert_eq!(label(&video, &host, video::Id::Brightness), "Brightness: Moody");
    video.slide(&mut host, video::Id::Brightness, 1.0);
    assert_eq!(label(&video, &host, video::Id::Brightness), "Brightness: Bright");
    video.slide(&mut host, video::Id::Brightness, 0.37);
    assert_eq!(
        (host.settings.brightness, label(&video, &host, video::Id::Brightness)),
        (0.37, "Brightness: 37".into())
    );
}

#[test]
fn gui_scale_cycles_auto_then_each_scale_that_fits() {
    let mut host = Host { window: Some((1920, 1080)), ..Host::default() };
    let mut video = VideoSettings;
    let mut seen = vec![label(&video, &host, video::Id::GuiScale)];
    for _ in 0..5 {
        video.activate(&mut host, video::Id::GuiScale);
        seen.push(label(&video, &host, video::Id::GuiScale));
    }
    assert_eq!(
        seen,
        ["GUI Scale: Auto", "GUI Scale: 1", "GUI Scale: 2", "GUI Scale: 3", "GUI Scale: 4", "GUI Scale: Auto"]
    );
    // A saved scale too big for a smaller window shows and cycles as the largest that fits.
    host.settings.gui_scale = 4;
    host.window = Some((800, 600));
    assert_eq!(label(&video, &host, video::Id::GuiScale), "GUI Scale: 2");
    video.activate(&mut host, video::Id::GuiScale);
    assert_eq!(host.settings.gui_scale, 0);
    assert_eq!(video::next_gui_scale(0, 1), 1);
    assert_eq!(video::next_gui_scale(1, 1), 0);
}

#[test]
fn accessibility_sliders_use_javas_labels_and_curves() {
    let mut host = Host::default();
    let mut screen = Accessibility;
    assert_eq!(label(&screen, &host, accessibility::Id::Distortion), "Distortion Effects: 100%");
    screen.slide(&mut host, accessibility::Id::Distortion, 0.0);
    assert_eq!(label(&screen, &host, accessibility::Id::Distortion), "Distortion Effects: OFF");
    // FOV Effects' slider is the square root of its value.
    screen.slide(&mut host, accessibility::Id::FovEffects, 0.5);
    assert_eq!(host.settings.fov_effects, 0.25);
    assert_eq!(label(&screen, &host, accessibility::Id::FovEffects), "FOV Effects: 25%");
    screen.slide(&mut host, accessibility::Id::DamageTilt, 0.5);
    assert_eq!(label(&screen, &host, accessibility::Id::DamageTilt), "Damage Tilt: 50%");
    screen.activate(&mut host, accessibility::Id::ViewBobbing);
    assert!(!host.settings.view_bobbing);
    assert_eq!(crate::app::fov_effects(1.2, 0.0), 1.0, "FOV Effects OFF keeps the FOV");
    assert!((crate::app::fov_effects(1.2, 0.5) - 1.1).abs() < 1e-6);
}

/// Every sound category has a slider, laid out as Java's Music & Sound Options.
#[test]
fn music_and_sounds_follows_javas_layout() {
    let host = Host::default();
    let size = (640.0, 360.0);
    let widgets = MusicAndSounds.widgets(&host, size);
    let sliders: Vec<_> = widgets
        .iter()
        .filter_map(|w| match &w.kind {
            Kind::Slider { label, .. } => Some((w.id, label.clone(), w.rect)),
            _ => None,
        })
        .collect();
    let labels: Vec<_> = sliders.iter().map(|(_, l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Master Volume: 100%",
            "Music: 100%",
            "Jukebox/Note Blocks: 100%",
            "Weather: 100%",
            "Blocks: 100%",
            "Hostile Mobs: 100%",
            "Friendly Mobs: 100%",
            "Players: 100%",
            "Ambient/Environment: 100%",
            "UI: 100%",
        ]
    );
    // Master is full width; the rest are two to a row, left then right.
    let [_, _, master_w, _] = sliders[0].2;
    let [_, _, music_w, _] = sliders[1].2;
    assert!(master_w > music_w * 1.8, "master is wide: {master_w} vs {music_w}");
    for pair in sliders[1..].chunks(2) {
        if let [a, b] = pair {
            assert_eq!(a.2[1], b.2[1], "{} and {} share a row", a.1, b.1);
            assert!(a.2[0] < b.2[0]);
        }
    }
    assert!(sliders[1].2[1] > sliders[0].2[1] && sliders[3].2[1] > sliders[1].2[1], "rows go down");
    // Zero reads OFF.
    let mut quiet = Host::default();
    for source in SoundSource::ALL {
        quiet.settings.set_sound_volume(source, 0.0);
    }
    for w in MusicAndSounds.widgets(&quiet, size) {
        if let Kind::Slider { label, .. } = w.kind {
            assert!(label.ends_with(": OFF"), "{label}");
        }
    }
}

#[test]
fn every_sound_slider_sets_its_own_category_over_the_whole_range() {
    for source in SoundSource::ALL {
        for (t, want) in [(0.0, 0.0), (1.0, 1.0), (0.5, 0.5), (0.337, 0.34)] {
            let mut host = Host::default();
            // Start every category elsewhere so only the slid one can move to `want`.
            for other in SoundSource::ALL {
                host.settings.set_sound_volume(other, 0.7);
            }
            MusicAndSounds.slide(&mut host, sound::Id::Volume(source), t);
            for other in SoundSource::ALL {
                let expect = if other == source { want } else { 0.7 };
                assert!((host.settings.sound_volume(other) - expect).abs() < 1e-6, "{source:?} at {t}: {other:?}");
            }
        }
    }
}

/// Keyboard (Tab, arrows) and controller (D-pad) reach and move every slider.
#[test]
fn every_sound_slider_is_reachable_by_keyboard_and_controller() {
    for size in SIZES {
        let mut host = Host::default();
        let mut stack: Stack<Host> = Stack::default();
        stack.open(MusicAndSounds);
        let (shape, _) = stack.peek(&host, size);
        let sliders: Vec<usize> = (0..shape.slots.len()).filter(|&i| shape.slots[i].focusable).collect();
        // Ten sliders and Done.
        assert_eq!(sliders.len(), SoundSource::ALL.len() + 1, "{size:?}");
        for (n, source) in SoundSource::ALL.into_iter().enumerate() {
            // Tab to slider `n`, lower it with the arrow key, raise it with the D-pad.
            let mut tabs = Host::default();
            let mut keys: Stack<Host> = Stack::default();
            keys.open(MusicAndSounds);
            for _ in 0..=n {
                keys.key(&mut tabs, &Key::new(KeyCode::Tab), size);
            }
            keys.key(&mut tabs, &Key::new(KeyCode::ArrowLeft), size);
            let lowered = tabs.settings.sound_volume(source);
            assert!(lowered < 1.0, "{source:?} lowered by ArrowLeft at {size:?}");
            for other in SoundSource::ALL.into_iter().filter(|&o| o != source) {
                assert_eq!(tabs.settings.sound_volume(other), 1.0);
            }
            keys.nav(&mut tabs, Nav::Right, size);
            assert!(tabs.settings.sound_volume(source) > lowered, "{source:?} raised by the D-pad");
        }
        // The D-pad reaches every slider and Done: as in Java, a slider reached
        // this way isn't adjusted until A is pressed, so Down walks the left
        // column and Right steps across to the other one.
        let mut seen = std::collections::BTreeSet::new();
        for downs in 1..=sliders.len() {
            let mut walk: Stack<Host> = Stack::default();
            walk.open(MusicAndSounds);
            for _ in 0..downs {
                walk.nav(&mut host, Nav::Down, size);
            }
            seen.insert(walk.peek(&host, size).1.expect("focused"));
            walk.nav(&mut host, Nav::Right, size);
            seen.insert(walk.peek(&host, size).1.expect("focused"));
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), sliders, "the D-pad reaches every widget at {size:?}");
        assert!(host.settings == Settings::default(), "navigating changes nothing");
    }
}

#[test]
fn closing_any_options_screen_saves_and_goes_back() {
    let mut stack: Stack<Host> = Stack::default();
    let mut host = Host::default();
    stack.open(Options);
    stack.open(VideoSettings);
    stack.open(MusicAndSounds);
    stack.nav(&mut host, Nav::Back, (640.0, 360.0));
    stack.key(&mut host, &Key::new(KeyCode::Escape), (640.0, 360.0));
    assert_eq!(host.saves.get(), 2);
    stack.nav(&mut host, Nav::Back, (640.0, 360.0));
    assert_eq!(host.saves.get(), 3);
    assert!(!stack.is_open());
}

#[test]
fn toggles_apply_at_once_and_fullscreen_reaches_the_window() {
    let mut host = Host::default();
    let mut video = VideoSettings;
    video.activate(&mut host, video::Id::Vsync);
    assert!(!host.settings.vsync && host.applied == 1);
    video.activate(&mut host, video::Id::Fullscreen);
    assert_eq!((host.settings.fullscreen, host.fullscreen), (crate::app::fullscreen::Mode::Borderless, 1));
    Options.activate(&mut host, Id::CheckUpdates);
    assert!(!host.settings.check_updates);
    let mut controls = Controls;
    controls.activate(&mut host, controls::ControlsId::Sneak);
    controls.activate(&mut host, controls::ControlsId::Sprint);
    assert!(host.settings.toggle_sneak && host.settings.toggle_sprint);
    assert_eq!(label(&controls, &host, controls::ControlsId::Sneak), "Sneak: Toggle");
    let mut mouse = MouseSettings;
    for id in [controls::MouseId::Discrete, controls::MouseId::InvertX, controls::MouseId::InvertY] {
        mouse.activate(&mut host, id);
    }
    assert!(host.settings.discrete_scroll && host.settings.invert_mouse_x && host.settings.invert_mouse_y);
    assert_eq!(label(&mouse, &host, controls::MouseId::Scroll), "Scroll Sensitivity: 1.00");
    mouse.slide(&mut host, controls::MouseId::Scroll, 1.0);
    assert_eq!(host.settings.scroll_sensitivity, 10.0);
}

#[test]
fn difficulty_shows_only_in_a_world_and_locks_like_java() {
    let find = |host: &Host, id: Id| Options.widgets(host, (640.0, 360.0)).into_iter().find(|w| w.id == id);
    assert!(find(&Host::default(), Id::Difficulty).is_none(), "the title screen has no difficulty");
    assert!(find(&Host::default(), Id::Lock).is_none());
    let mut host = Host { world: world(Difficulty::Easy, false, false), ..Host::default() };
    let button = find(&host, Id::Difficulty).unwrap();
    let lock = find(&host, Id::Lock).unwrap();
    assert!(button.enabled && lock.enabled);
    assert_eq!(button.rect[0] + button.rect[2], lock.rect[0], "the lock sits at the button's end");
    assert_eq!(button.rect[2] + lock.rect[2], 150.0);
    Options.activate(&mut host, Id::Difficulty);
    assert_eq!(host.world.unwrap().level, Difficulty::Normal);

    // The lock asks first; No leaves it unlocked, Yes locks it for good.
    let size = (640.0, 360.0);
    let mut stack: Stack<Host> = Stack::default();
    stack.open(Options);
    let lock_at = |stack: &Stack<Host>, host: &Host| {
        let (shape, _) = stack.peek(host, size);
        let i = Options.widgets(host, size).iter().position(|w| w.id == Id::Lock).unwrap();
        let [x, y, w, h] = shape.slots[i].rect;
        (x + w / 2.0, y + h / 2.0)
    };
    let at = lock_at(&stack, &host);
    stack.mouse_moved(&mut host, at, size);
    stack.mouse_button(&mut host, MouseButton::Left, true, size);
    stack.key(&mut host, &Key::new(KeyCode::Escape), size);
    assert!(!host.world.unwrap().locked, "cancelled");
    stack.mouse_button(&mut host, MouseButton::Left, true, size);
    stack.nav(&mut host, Nav::Down, size);
    stack.nav(&mut host, Nav::Activate, size);
    assert!(host.world.unwrap().locked, "Yes locks the difficulty");
    let button = find(&host, Id::Difficulty).unwrap();
    assert!(!button.enabled && button.tooltip.as_deref() == Some("Difficulty is locked."));
    assert!(!find(&host, Id::Lock).unwrap().enabled);
    Options.activate(&mut host, Id::Difficulty);
    assert_eq!(host.world.unwrap().level, Difficulty::Normal, "locked difficulty can't change");

    let host = Host { world: world(Difficulty::Hard, true, false), ..Host::default() };
    let hardcore = find(&host, Id::Difficulty).unwrap();
    assert!(!hardcore.enabled);
    assert_eq!(hardcore.tooltip.as_deref(), Some("Cannot change the difficulty in a hardcore world."));
}

/// A Key Binds screen in a stack, and the rect centre of one of its widgets.
fn key_binds() -> (Stack<Host>, Host) {
    let mut stack: Stack<Host> = Stack::default();
    stack.open(Controls);
    stack.open(KeyBinds::default());
    (stack, Host::default())
}

const SIZE: Size = (640.0, 360.0);

fn centre_of(stack: &Stack<Host>, host: &Host, id: keybinds::Id) -> (f32, f32) {
    let (shape, _) = stack.peek(host, SIZE);
    let i = KeyBinds::default().widgets(host, SIZE).iter().position(|w| w.id == id).unwrap();
    let [x, y, w, h] = shape.slots[i].rect;
    assert!(shape.slots[i].shown, "{id:?} is on screen");
    (x + w / 2.0, y + h / 2.0)
}

fn click(stack: &mut Stack<Host>, host: &mut Host, at: (f32, f32), button: MouseButton) {
    stack.mouse_moved(host, at, SIZE);
    stack.mouse_button(host, button, true, SIZE);
    stack.mouse_button(host, button, false, SIZE);
}

#[test]
fn clicking_a_binding_then_a_key_or_button_rebinds_it() {
    let (mut stack, mut host) = key_binds();
    let jump = centre_of(&stack, &host, keybinds::Id::Change(Action::Jump));
    click(&mut stack, &mut host, jump, MouseButton::Left);
    assert_eq!(host.settings.keys.get(Action::Jump), Input::Key(KeyCode::Space), "waiting, not yet changed");
    // Tab is a key like any other while a binding waits.
    stack.key(&mut host, &Key::new(KeyCode::Tab), SIZE);
    assert_eq!(host.settings.keys.get(Action::Jump), Input::Key(KeyCode::Tab));
    // A mouse button, even the one that clicks.
    click(&mut stack, &mut host, jump, MouseButton::Left);
    click(&mut stack, &mut host, jump, MouseButton::Back);
    assert_eq!(host.settings.keys.get(Action::Jump), Input::Mouse(MouseButton::Back));
    // Escape unbinds, as in Java, and leaves the screen open.
    click(&mut stack, &mut host, jump, MouseButton::Left);
    stack.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
    assert_eq!(host.settings.keys.get(Action::Jump), Input::Unbound);
    assert!(stack.is_open());
    assert!(host.applied >= 3);
    // With nothing waiting, Escape saves and goes back.
    stack.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
    assert_eq!(host.saves.get(), 1);
}

#[test]
fn keyboard_and_controller_reach_and_rebind_every_binding() {
    let (mut stack, mut host) = key_binds();
    // Enter on a focused binding starts waiting; the next key binds it.
    stack.key(&mut host, &Key::new(KeyCode::Tab), SIZE);
    let (_, focus) = stack.peek(&host, SIZE);
    let widgets = KeyBinds::default().widgets(&host, SIZE);
    let keybinds::Id::Change(first) = widgets[focus.unwrap()].id else { panic!("first stop is a binding") };
    stack.key(&mut host, &Key::new(KeyCode::Enter), SIZE);
    stack.key(&mut host, &Key::new(KeyCode::KeyK), SIZE);
    assert_eq!(host.settings.keys.get(first), Input::Key(KeyCode::KeyK));
    // The controller's back button lets go of a waiting binding, then leaves.
    stack.nav(&mut host, Nav::Activate, SIZE);
    stack.nav(&mut host, Nav::Back, SIZE);
    assert_eq!(host.settings.keys.get(first), Input::Key(KeyCode::KeyK), "unchanged");
    assert!(stack.is_open());
    // Down reaches the last binding's row through the scrolling list.
    for _ in 0..200 {
        stack.nav(&mut host, Nav::Down, SIZE);
    }
    let (shape, focus) = stack.peek(&host, SIZE);
    assert!(shape.slots[focus.unwrap()].shown);
    stack.nav(&mut host, Nav::Back, SIZE);
    assert_eq!(host.saves.get(), 1);
}

#[test]
fn conflicts_show_in_brackets_and_resets_restore_defaults() {
    let (mut stack, mut host) = key_binds();
    let sneak = centre_of(&stack, &host, keybinds::Id::Change(Action::Sneak));
    click(&mut stack, &mut host, sneak, MouseButton::Left);
    stack.key(&mut host, &Key::new(KeyCode::KeyW), SIZE);
    let widgets = KeyBinds::default().widgets(&host, SIZE);
    let get = |id| widgets.iter().find(|w| w.id == id).unwrap();
    let tip = get(keybinds::Id::Change(Action::Sneak)).tooltip.clone();
    assert_eq!(tip.as_deref(), Some("This key is also used for:\nWalk Forward"));
    assert_eq!(
        get(keybinds::Id::Change(Action::Forward)).tooltip.as_deref(),
        Some("This key is also used for:\nSneak")
    );
    assert!(get(keybinds::Id::Reset(Action::Sneak)).enabled);
    assert!(!get(keybinds::Id::Reset(Action::Forward)).enabled, "Forward is still on its default");
    assert!(get(keybinds::Id::ResetAll).enabled);
    // Drawing the bracketed and waiting labels is safe.
    let mut ui = Ui::with_scale(1280.0, 720.0, 2.0);
    stack.draw(&host, &mut ui);
    click(&mut stack, &mut host, sneak, MouseButton::Left);
    stack.draw(&host, &mut ui);
    stack.nav(&mut host, Nav::Back, SIZE);

    // Reset puts one binding back; Reset Keys puts back every binding.
    let reset = centre_of(&stack, &host, keybinds::Id::Reset(Action::Sneak));
    click(&mut stack, &mut host, reset, MouseButton::Left);
    assert!(host.settings.keys.is_default(Action::Sneak));
    host.settings.keys.set(Action::Jump, Input::Unbound);
    host.settings.keys.set(Action::Attack, Input::Key(KeyCode::KeyJ));
    let all = centre_of(&stack, &host, keybinds::Id::ResetAll);
    click(&mut stack, &mut host, all, MouseButton::Left);
    assert!(host.settings.keys.all_default());
    let widgets = KeyBinds::default().widgets(&host, SIZE);
    assert!(!widgets.iter().find(|w| w.id == keybinds::Id::ResetAll).unwrap().enabled);
}

#[test]
fn key_binds_list_every_action_under_javas_categories() {
    let host = Host::default();
    let widgets = KeyBinds::default().widgets(&host, SIZE);
    let headings: Vec<String> = widgets
        .iter()
        .filter_map(|w| match &w.kind {
            Kind::Label(l) => Some(l.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(headings, ["Movement", "Miscellaneous", "Multiplayer", "Gameplay", "Inventory", "Debug", "VoxelCraft"]);
    for action in Action::ALL {
        assert!(widgets.iter().any(|w| w.id == keybinds::Id::Change(action)), "{action:?} is listed");
    }
}

#[test]
fn credits_roll_from_options_and_escape_leaves_it() {
    let mut stack: Stack<Host> = Stack::default();
    let mut host = Host::default();
    stack.open(Options);
    stack.open(credits::CreditsAndAttribution);
    stack.nav(&mut host, Nav::Down, SIZE);
    stack.nav(&mut host, Nav::Activate, SIZE);
    let mut ui = Ui::with_scale(1280.0, 720.0, 2.0);
    stack.draw(&host, &mut ui);
    assert!(!ui.verts.is_empty());
    stack.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
    stack.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
    stack.key(&mut host, &Key::new(KeyCode::Escape), SIZE);
    assert!(!stack.is_open());
}

#[test]
fn the_credits_roll_closes_itself_at_its_end() {
    let mut stack: Stack<Host> = Stack::default();
    let mut host = Host::default();
    stack.open(Options);
    stack.open(credits::Roll::default());
    stack.tick(&mut host, SIZE);
    assert_eq!(stack.depth(), 2, "still rolling");
    stack.open(credits::Roll::finished());
    stack.tick(&mut host, SIZE);
    assert_eq!(stack.depth(), 2, "the finished roll closed itself");
}

#[test]
fn labels_follow_java() {
    assert_eq!(fov_label(70.0), "FOV: Normal");
    assert_eq!(fov_label(110.0), "FOV: Quake Pro");
    assert_eq!(fov_label(90.0), "FOV: 90");
    assert_eq!(percent_label("Master Volume", 0.0), "Master Volume: OFF");
    assert_eq!(percent_label("Music", 0.456), "Music: 46%");
    assert_eq!(video::max_fps_label(60), "Max Framerate: 60 fps");
    // f32 values just under a whole percent still show that percent.
    assert_eq!(video::brightness_label(0.53), "Brightness: 53");
    assert_eq!(video::brightness_label(0.59), "Brightness: 59");
}
