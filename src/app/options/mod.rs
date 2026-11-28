//! Java's Options screen and its sub-screens, opened from both the title
//! screen and the pause menu. They work on any [`OptionsHost`], so the same
//! screens change the options with or without a world open. Each screen
//! saves `options.txt` when it closes, as Java does.

mod accessibility;
mod chat;
mod controls;
mod credits;
mod keybinds;
mod sound;
mod video;
mod world;

pub(super) use accessibility::Accessibility;
pub(super) use controls::{Controls, MouseSettings};
pub(super) use keybinds::KeyBinds;
pub(super) use sound::MusicAndSounds;
pub(super) use video::VideoSettings;
pub(super) use world::WorldOptions;

use super::gui::{self, Host, Look, OptionList, Rect, Screen, Size, Stack, Step, Widget};
use super::settings::{FOV, Settings};
use crate::render::ui::Ui;
use crate::simulation::difficulty::Difficulty;

/// The open world's difficulty, as World Options shows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct WorldDifficulty {
    pub level: Difficulty,
    pub hardcore: bool,
    /// Java's `DifficultyLocked`, set with the lock button.
    pub locked: bool,
}

/// What the Options screens need from the app.
pub(super) trait OptionsHost: Host {
    fn settings(&self) -> &Settings;
    fn settings_mut(&mut self) -> &mut Settings;
    /// Pushes the settings to the renderer, audio and (in a world) the world.
    fn apply_settings(&mut self);
    /// Writes `options.txt`; the error says what failed, for a notice.
    fn save_settings(&self) -> Result<(), String>;
    /// Puts the window into the fullscreen mode the settings ask for.
    fn apply_fullscreen(&mut self);
    /// The window's size in physical pixels, which bounds GUI Scale.
    fn window_size(&self) -> Option<(u32, u32)> {
        None
    }
    /// The open world's difficulty; `None` on the title screen.
    fn difficulty(&self) -> Option<WorldDifficulty> {
        None
    }
    /// Sets the world's difficulty (World Options' Apply Changes).
    fn set_difficulty(&mut self, _: Difficulty) {}
    /// Locks the world's difficulty for good.
    fn lock_difficulty(&mut self) {}
}

/// Changes the settings, applying them if anything changed.
pub(super) fn change<H: OptionsHost>(host: &mut H, f: impl FnOnce(&mut Settings)) {
    let before = *host.settings();
    f(host.settings_mut());
    *host.settings_mut() = host.settings().clamped();
    if *host.settings() != before {
        host.apply_settings();
    }
}

/// `lo..hi` at `t` (0..1).
pub(super) fn lerp((lo, hi): (f32, f32), t: f32) -> f32 {
    lo + (hi - lo) * t.clamp(0.0, 1.0)
}

/// v0.18 Track 2: Menu Background Blur's label (`Options.genericValueOrOffLabel`).
pub(in crate::app) fn menu_blur_label(v: u32) -> String {
    if v == 0 { "Menu Background Blur: OFF".into() } else { format!("Menu Background Blur: {v}") }
}

/// The Menu Background Blur slider, shared by Video and Accessibility
/// Settings as in Java.
pub(in crate::app) fn menu_blur_slider<Id>(id: Id, blur: u32) -> gui::Widget<Id> {
    let (lo, hi) = crate::app::settings::MENU_BLUR;
    gui::Widget::slider(id, [0.0; 4], menu_blur_label(blur), (blur - lo) as f32 / (hi - lo) as f32, hi - lo)
        .tooltip("Changes the blurriness of menu backgrounds.")
}

/// Menu Background Blur at slider position `t`.
pub(in crate::app) fn menu_blur_at(t: f32) -> u32 {
    let (lo, hi) = crate::app::settings::MENU_BLUR;
    lo + (t.clamp(0.0, 1.0) * (hi - lo) as f32).round() as u32
}

/// Where `v` lies in `lo..hi`, 0..1.
pub(super) fn frac(v: f32, (lo, hi): (f32, f32)) -> f32 {
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Java's FOV label: "Normal" at 70 and "Quake Pro" at 110.
pub(super) fn fov_label(fov: f32) -> String {
    match fov.round() as i32 {
        70 => "FOV: Normal".into(),
        110 => "FOV: Quake Pro".into(),
        v => format!("FOV: {v}"),
    }
}

/// Java's percentage sliders: "Off" at zero.
pub(super) fn percent_label(name: &str, v: f32) -> String {
    match (v * 100.0).round() as i32 {
        0 => format!("{name}: OFF"),
        v => format!("{name}: {v}%"),
    }
}

/// Opens Options and, for a sub-screen's `--open-menu` name, the screens a
/// player passes through to reach it.
pub(super) fn open_screens<H: OptionsHost>(stack: &mut Stack<H>, name: &str) {
    stack.open(Options);
    match name {
        "video" => stack.open(VideoSettings),
        "sound" => stack.open(MusicAndSounds),
        "controls" => stack.open(Controls),
        "mouse" => {
            stack.open(Controls);
            stack.open(MouseSettings);
        }
        "keybinds" => {
            stack.open(Controls);
            stack.open(KeyBinds::default());
        }
        "accessibility" => stack.open(Accessibility),
        "chat" => stack.open(chat::ChatSettings),
        _ => {}
    }
}

/// Back from an options screen: save, then return to the screen below,
/// through a notice if `options.txt` couldn't be written.
pub(super) fn close<H: OptionsHost>(host: &mut H) -> Step<H> {
    match host.save_settings() {
        Ok(()) => Step::Pop,
        Err(reason) => Step::replace(save_failed(reason)),
    }
}

/// The notice for an `options.txt` that couldn't be written.
pub(in crate::app) fn save_failed(reason: String) -> gui::Notice {
    gui::Notice::new(
        "Couldn't save options",
        vec![
            "The changes work for now, but are lost when the game closes.".into(),
            reason,
            "Check that the folder exists, isn't read-only and has free space.".into(),
        ],
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Id {
    Fov,
    Sound,
    Video,
    Controls,
    Chat,
    Accessibility,
    Credits,
    CheckUpdates,
    Done,
}

/// The Options screen: Java 26.3's, without the screens VoxelCraft has no
/// use for (Online, Skin Customization, Language, Resource Packs,
/// Telemetry). Difficulty and its lock moved to the pause menu's World
/// Options in Java 26.3, and in VoxelCraft with v1.0 final F6.
pub(super) struct Options;

impl<H: OptionsHost> Screen<H> for Options {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Options".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let s = host.settings();
        let z = [0.0; 4];
        let mut list = OptionList::new(size);
        let fov = Widget::slider(Id::Fov, z, fov_label(s.fov), frac(s.fov, FOV), (FOV.1 - FOV.0) as u32);
        list.pair(fov, None);
        // Java's grid order, without the screens VoxelCraft leaves out.
        list.pairs(vec![
            Widget::button(Id::Sound, z, "Music & Sounds..."),
            Widget::button(Id::Video, z, "Video Settings..."),
            Widget::button(Id::Controls, z, "Controls..."),
            Widget::button(Id::Chat, z, "Chat Settings..."),
            Widget::button(Id::Accessibility, z, "Accessibility Settings..."),
            Widget::button(Id::Credits, z, "Credits & Attribution..."),
            Widget::button(Id::CheckUpdates, z, format!("Check for Updates: {}", gui::on_off(s.check_updates)))
                .tooltip("Look for a newer VoxelCraft release when the game starts."),
        ]);
        list.done(Widget::button(Id::Done, z, "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Fov => {}
            Id::Sound => return Step::push(MusicAndSounds),
            Id::Video => return Step::push(VideoSettings),
            Id::Controls => return Step::push(Controls),
            Id::Chat => return Step::push(chat::ChatSettings),
            Id::Accessibility => return Step::push(Accessibility),
            Id::Credits => return Step::push(credits::CreditsAndAttribution),
            // Takes effect at the next launch's title screen.
            Id::CheckUpdates => change(host, |s| s.check_updates = !s.check_updates),
            Id::Done => return close(host),
        }
        Step::Stay
    }

    fn slide(&mut self, host: &mut H, id: Id, t: f32) {
        if id == Id::Fov {
            change(host, |s| s.fov = lerp(FOV, t).round());
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}

/// Java's lock icon on a button: a padlock, its shackle open until locked.
pub(super) fn draw_lock(ui: &mut Ui, rect: Rect, locked: bool, look: Look) {
    gui::draw_button(ui, rect, "", look);
    let [x, y, _, _] = rect;
    let colour = if look.enabled { [0.86, 0.86, 0.86, 1.0] } else { [0.55, 0.55, 0.55, 1.0] };
    let dark = [0.25, 0.25, 0.25, 1.0];
    // Body.
    ui.rect(x + 6.0, y + 9.0, 8.0, 6.0, colour);
    ui.rect(x + 9.0, y + 11.0, 2.0, 2.0, dark);
    // Shackle: closed onto the body when locked, lifted and open when not.
    let top = if locked { y + 4.0 } else { y + 2.0 };
    let post = y + 9.0 - (top + 1.0);
    ui.rect(x + 7.0, top, 6.0, 1.0, colour);
    ui.rect(x + 7.0, top + 1.0, 1.0, post, colour);
    ui.rect(x + 12.0, top + 1.0, 1.0, if locked { post } else { 2.0 }, colour);
}

#[cfg(test)]
pub(super) mod tests;
