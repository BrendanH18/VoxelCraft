//! Java's `RecoverWorldDataScreen`, which `WorldOpenFlows` opens when a
//! world's level file can't be read (v1.0 Track 1). It lists the level file
//! and the copy the last save kept (`level.txt_old`, Java's
//! `level.dat_old`) with when each was written and what is wrong with it.
//! **Attempt to Restore** is offered when the level file is unreadable and
//! its kept copy is readable: it backs the world up first, as Java's
//! `EditWorldScreen.makeBackupAndShowToast` does, then
//! [`Storage::restore_level_from_old`] moves the unreadable file to
//! `level.txt_corrupted_<date>` and the kept copy into its place.
//!
//! Differences: Java's **Report a Bug** button is left out (there is no
//! public bug tracker to send players to), and each state line is drawn in
//! one colour rather than Java's grey date inside it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::SystemTime;

use super::{Action, TitleHost, prompts, worlds};
use crate::app::gui::{BUTTON_H, Confirm, Notice, Screen, Size, Step, Widget, wrap};
use crate::render::ui::{Color, Ui};
use crate::world::storage::Storage;

const LINE: f32 = 9.0;
const SPACING: f32 = 8.0;
const BUTTON: f32 = 120.0;
/// Java's `ChatFormatting.RED` and `GREEN`.
const RED: Color = [1.0, 0x55 as f32 / 255.0, 0x55 as f32 / 255.0, 1.0];
const GREEN: Color = [0x55 as f32 / 255.0, 1.0, 0x55 as f32 / 255.0, 1.0];
const WHITE: Color = [1.0, 1.0, 1.0, 1.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Restore,
    Back,
}

/// "Failed to load world": what is wrong with the level file and its kept
/// copy, and the restore when there is something to restore from.
pub(in crate::app) struct RecoverWorld {
    dir: String,
    message: String,
    /// Each state line with whether it reports a problem.
    states: Vec<(String, bool)>,
    can_restore: bool,
    /// Set once the world on disk changed, so Select World reloads it.
    stale: Rc<Cell<bool>>,
}

impl RecoverWorld {
    pub fn new<H: TitleHost>(host: &H, dir: &str, stale: Rc<Cell<bool>>) -> Self {
        let storage = Storage::new(host.saves_dir().join(dir));
        let current = storage.load_level().err();
        let old = storage.load_level_old().err();
        let date = |old: bool| storage.level_modified(old).map_or_else(|| "unknown".to_string(), worlds::short_date);
        let issue = |e: &Option<std::io::Error>| match e {
            None => ("No issues".to_string(), false),
            Some(e) if e.kind() == std::io::ErrorKind::NotFound => ("Missing file".to_string(), true),
            Some(e) => (e.to_string(), true),
        };
        let mut states = Vec::new();
        let (text, bad) = issue(&current);
        states.push((format!("State from {}: {text}", date(false)), bad));
        // Java's `buildInfo` leaves out a fallback that was never written.
        if !old.as_ref().is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            let (text, bad) = issue(&old);
            states.push((format!("State from {}: {text}", date(true)), bad));
        }
        Self {
            dir: dir.to_string(),
            message: format!(
                "The following issues occurred while trying to read world folder \"{dir}\". It might be possible \
                 to restore the world from an older state."
            ),
            states,
            can_restore: current.is_some() && old.is_none(),
            stale,
        }
    }

    /// The text's lines, the state lines with their colour, and the top of
    /// the block, which Java centres on the screen.
    fn layout(&self, (sw, sh): Size) -> (Vec<String>, Vec<(String, bool)>, f32) {
        let width = sw - 50.0;
        let message = wrap(&self.message, width);
        let states: Vec<(String, bool)> =
            self.states.iter().flat_map(|(line, bad)| wrap(line, width).into_iter().map(|l| (l, *bad))).collect();
        let height = LINE
            + SPACING
            + message.len() as f32 * LINE
            + SPACING
            + states.len() as f32 * LINE
            + SPACING
            + BUTTON_H
            + SPACING
            + BUTTON_H;
        (message, states, ((sh - height) / 2.0).floor().max(4.0))
    }

    fn buttons_y(&self, size: Size) -> f32 {
        let (message, states, top) = self.layout(size);
        top + LINE + SPACING + message.len() as f32 * LINE + SPACING + states.len() as f32 * LINE + SPACING
    }

    /// Restores the level file once the backup has run, as Java's
    /// `attemptRestore` does whether or not the backup worked.
    fn restore<H: TitleHost>(host: &mut H, dir: String, backup: prompts::Made, stale: &Cell<bool>) -> Step<H> {
        let storage = Storage::new(host.saves_dir().join(&dir));
        let mut lines = Vec::new();
        if let Err(e) = &backup {
            lines.push(format!("The backup failed: {e}"));
        }
        let restored = if storage.locked_elsewhere() {
            Err(std::io::Error::other("the world is open in another VoxelCraft window"))
        } else {
            storage.restore_level_from_old(&corrupted_stamp(SystemTime::now()))
        };
        stale.set(true);
        match restored {
            Ok(kept) => {
                log::info!(
                    "restored {} from its level.txt_old (the unreadable file is {kept:?})",
                    storage.dir().display()
                );
                lines.insert(0, "Recovery was successful!".to_string());
                Step::replace(
                    Confirm::new("Recovery done", lines, "Continue", move |_| Step::Out(Action::Play(dir, None)))
                        .no("Back"),
                )
            }
            Err(e) => {
                log::error!("failed to restore {} from its level.txt_old: {e}", storage.dir().display());
                lines.insert(0, "Failed to recover from previous state.".to_string());
                Step::replace(Notice::new("Recovery done", lines))
            }
        }
    }
}

/// Java's `FileNameDateFormatter` (`yyyy-MM-dd_HH.mm.ss`), in UTC like the
/// game's backup folders.
pub(in crate::app) fn corrupted_stamp(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (year, month, day) = crate::update::backup::civil(days as i64);
    format!("{year:04}-{month:02}-{day:02}_{:02}.{:02}.{:02}", rem / 3600, rem / 60 % 60, rem % 60)
}

impl<H: TitleHost> Screen<H> for RecoverWorld {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Failed to load world".into()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<Id>> {
        let y = self.buttons_y(size);
        let x = ((size.0 - BUTTON) / 2.0).floor();
        let mut restore =
            Widget::button(Id::Restore, [x, y, BUTTON, BUTTON_H], "Attempt to Restore").enabled(self.can_restore);
        if !self.can_restore {
            restore = restore.tooltip("No state to recover from available");
        }
        vec![restore, Widget::button(Id::Back, [x, y + BUTTON_H + SPACING, BUTTON, BUTTON_H], "Back")]
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Restore if self.can_restore => {
                let (dir, stale) = (self.dir.clone(), self.stale.clone());
                Step::push(prompts::backup(host, &self.dir, move |host, made| Self::restore(host, dir, made, &stale)))
            }
            Id::Restore => Step::Stay,
            Id::Back => Step::Pop,
        }
    }

    fn title_y(&self, size: Size, _: f32) -> f32 {
        self.layout(size).2
    }

    fn draw(&self, _: &H, ui: &mut Ui, size: Size, _: f32) {
        let (message, states, top) = self.layout(size);
        let mut y = top + LINE + SPACING;
        for line in &message {
            ui.text(((size.0 - Ui::text_width(line)) / 2.0).floor(), y, line, WHITE);
            y += LINE;
        }
        y += SPACING;
        let left = ((size.0 - states.iter().map(|(l, _)| Ui::text_width(l)).fold(0.0, f32::max)) / 2.0).floor();
        for (line, bad) in &states {
            ui.text(left, y, line, if *bad { RED } else { GREEN });
            y += LINE;
        }
    }
}
