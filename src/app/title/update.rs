//! The update screen: a newer release's notes, then downloading, checking
//! and installing it (see `docs/updates.md`). Java has no equivalent; it
//! uses the toolkit's layout and input like every other title screen.

use super::Title;
use crate::app::gui::{BUTTON_H, BUTTON_W, GREY, Screen, Size, Step, Widget, fit, wrap};
use crate::render::ui::{Ui, WHITE};
use crate::update::{State, Step as Job};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    UpdateNow,
    ReleasePage,
    Later,
    SkipVersion,
    Cancel,
}

const HALF_W: f32 = 133.0;
const NOTES_W: f32 = 390.0;
const RED: [f32; 4] = [1.0, 0.5, 0.45, 1.0];

pub(in crate::app) struct UpdateScreen;

impl UpdateScreen {
    /// The notes box (x, y, w, h).
    fn notes_rect((sw, sh): Size) -> [f32; 4] {
        let w = NOTES_W.min(sw - 16.0);
        let top = 44.0;
        let bottom = sh - 2.0 * (BUTTON_H + 4.0) - 40.0;
        [((sw - w) / 2.0).floor(), top, w, (bottom - top).max(28.0)]
    }

    /// Mid-update: nothing to do but wait (or cancel a download).
    fn busy(title: &Title) -> bool {
        matches!(title.shell.updates.state, State::Working(_) | State::Ready(..) | State::Installing(_))
    }
}

impl Screen<Title> for UpdateScreen {
    type Id = Id;

    fn title(&self, title: &Title) -> String {
        let updates = &title.shell.updates;
        let Some(release) = updates.offer() else { return String::new() };
        if Self::busy(title) {
            format!("Updating to VoxelCraft {}", release.version.short())
        } else {
            format!("VoxelCraft {} is available", release.version.short())
        }
    }

    fn widgets(&self, title: &Title, (sw, sh): Size) -> Vec<Widget<Id>> {
        let cx = (sw / 2.0).floor();
        let (left, right) = (cx - HALF_W - 2.0, cx + 2.0);
        let (row1, row2) = (sh - 2.0 * (BUTTON_H + 4.0) - 10.0, sh - BUTTON_H - 10.0);
        let at = |x: f32, y: f32| [x, y, HALF_W, BUTTON_H];
        let wide = |y: f32| [cx - BUTTON_W / 2.0, y, BUTTON_W, BUTTON_H];
        let updates = &title.shell.updates;
        match updates.state {
            State::Working(_) => vec![Widget::button(Id::Cancel, wide(row2), "Cancel")],
            State::Ready(..) | State::Installing(_) => Vec::new(),
            _ if updates.install.is_ok() => {
                let now = if matches!(updates.state, State::Failed(..)) { "Try Again" } else { "Update and Restart" };
                vec![
                    Widget::button(Id::UpdateNow, at(left, row1), now),
                    Widget::button(Id::ReleasePage, at(right, row1), "Release Page"),
                    Widget::button(Id::Later, at(left, row2), "Later"),
                    Widget::button(Id::SkipVersion, at(right, row2), "Skip This Version"),
                ]
            }
            _ => vec![
                Widget::button(Id::ReleasePage, at(left, row1), "Release Page"),
                Widget::button(Id::Later, at(right, row1), "Later"),
                Widget::button(Id::SkipVersion, wide(row2), "Skip This Version"),
            ],
        }
    }

    fn activate(&mut self, title: &mut Title, id: Id) -> Step<Title> {
        let updates = &mut title.shell.updates;
        match id {
            Id::UpdateNow => updates.start(&title.saves_dir),
            Id::ReleasePage => {
                let page = updates.offer().map_or(crate::update::RELEASES_PAGE, |r| r.page.as_str());
                crate::update::install::open_url(page);
            }
            Id::Later => {
                updates.dismissed = true;
                return Step::Pop;
            }
            Id::SkipVersion => {
                title.shell.settings.skip_update = updates.offer().map(|r| r.version);
                updates.dismissed = true;
                title.save_settings();
                return Step::Pop;
            }
            Id::Cancel => {
                // `--update-now` would start the cancelled update again.
                updates.auto = false;
                updates.cancel();
            }
        }
        Step::Stay
    }

    /// Escape and B go back, except while an update is underway.
    fn back(&mut self, title: &mut Title) -> Step<Title> {
        if Self::busy(title) { Step::Stay } else { Step::Pop }
    }

    fn tick(&mut self, title: &mut Title) -> Step<Title> {
        if title.shell.updates.offer().is_none() { Step::Pop } else { Step::Stay }
    }

    fn title_y(&self, _: Size, _: f32) -> f32 {
        24.0
    }

    fn draw(&self, title: &Title, ui: &mut Ui, (sw, sh): Size, _: f32) {
        let centred = |ui: &mut Ui, y: f32, s: &str, color| {
            ui.text(((sw - Ui::text_width(s)) / 2.0).floor(), y, s, color);
        };
        let updates = &title.shell.updates;
        let Some(release) = updates.offer() else { return };
        let [x, y, w, h] = Self::notes_rect((sw, sh));
        ui.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.8]);
        let mut lines: Vec<String> = release.plain_notes().iter().flat_map(|l| wrap(l, w - 12.0)).collect();
        let rows = ((h - 8.0) / 11.0).floor().max(1.0) as usize;
        if lines.len() > rows {
            lines.truncate(rows.saturating_sub(1));
            lines.push("More on the release page...".into());
        }
        if lines.is_empty() {
            lines.push("See the release page for what's new.".into());
        }
        for (i, line) in lines.iter().enumerate() {
            ui.text(x + 6.0, y + 5.0 + i as f32 * 11.0, line, [0.88, 0.88, 0.88, 1.0]);
        }
        let status_y = y + h + 6.0;
        match &updates.state {
            State::Working(job) => {
                let text = match job.step {
                    Job::Downloading { done, total } if total > 0 => {
                        let mb = |b: u64| b as f64 / 1_000_000.0;
                        let bar = [x, status_y + 12.0, w, 6.0];
                        ui.rect(bar[0], bar[1], bar[2], bar[3], [0.2, 0.2, 0.2, 1.0]);
                        let frac = (done as f32 / total as f32).clamp(0.0, 1.0);
                        ui.rect(bar[0], bar[1], bar[2] * frac, bar[3], [0.5, 0.85, 0.45, 1.0]);
                        format!("Downloading... {:.1} of {:.1} MB", mb(done), mb(total))
                    }
                    Job::Downloading { .. } => "Downloading...".into(),
                    Job::Verifying => "Checking the download...".into(),
                    Job::BackingUp => "Backing up your worlds...".into(),
                    Job::Preparing => "Getting the update ready...".into(),
                };
                centred(ui, status_y, &text, WHITE);
            }
            State::Ready(..) | State::Installing(_) => {
                centred(ui, status_y, "Restarting to finish the update...", WHITE)
            }
            State::Failed(_, e) => centred(ui, status_y, &fit(&format!("Update failed: {e}"), sw - 16.0), RED),
            _ => match &updates.install {
                Ok(_) => centred(ui, status_y, "Your worlds are backed up before updating.", GREY),
                Err(why) => {
                    centred(ui, status_y, &fit(&format!("{why}."), sw - 16.0), GREY);
                    centred(ui, status_y + 11.0, "Download it from the release page instead.", GREY);
                }
            },
        }
    }
}
