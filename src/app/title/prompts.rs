//! Java's backup prompt before opening a world saved by a newer version
//! (`BackupConfirmScreen`, as `WorldOpenFlows` uses it for a downgrade), and
//! the "Creating Backup" screen shown while a backup runs. Backups use the
//! updater's backup code ([`crate::update::backup::backup_world`]).

use std::path::PathBuf;

use crossbeam_channel::Receiver;

use super::worlds::Summary;
use super::{Action, TitleHost};
use crate::app::gui::{BUTTON_H, GREY, Notice, Screen, Size, Step, Widget, wrap};
use crate::render::ui::{Color, Ui};
use crate::update::Version;

/// What a finished backup made: the folder and its size in bytes.
pub(super) type Made = Result<(PathBuf, u64), String>;

/// Java's severe-question colour (0xFFDF5050).
const SEVERE: Color = [0.875, 0.314, 0.314, 1.0];
const LINE: f32 = 9.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Backup,
    Skip,
    Cancel,
}

/// "Downgrading a world is not supported": back up and load, load anyway,
/// or cancel.
pub(in crate::app) struct BackupConfirm {
    world: Summary,
    warning: String,
}

impl BackupConfirm {
    pub fn downgrade(world: Summary, saved: Version) -> Self {
        let warning = format!(
            "This world was last played in version {}; you are on version {}. Downgrading a world could cause \
             corruption - we cannot guarantee that it will load or work. If you still want to continue, please \
             make a backup.",
            saved.short(),
            Version::current().short()
        );
        Self { world, warning }
    }

    /// The warning's lines and how far the screen moves up to fit a short
    /// window (Java places it from the top: title at 50, text at 70).
    fn layout(&self, (sw, sh): Size) -> (Vec<String>, f32) {
        let lines = wrap(&self.warning, sw - 50.0);
        let text = (lines.len() + 1) as f32 * LINE;
        let shift = (sh - (124.0 + text + BUTTON_H + 4.0)).min(0.0);
        (lines, shift.max(-40.0))
    }
}

impl<H: TitleHost> Screen<H> for BackupConfirm {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        // Drawn in red by `draw`.
        String::new()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<Id>> {
        let (lines, shift) = self.layout(size);
        let text = (lines.len() + 1) as f32 * LINE;
        let cx = (size.0 / 2.0).floor();
        let y = 100.0 + text + shift;
        vec![
            Widget::button(Id::Backup, [cx - 155.0, y, 150.0, BUTTON_H], "Create Backup and Load"),
            Widget::button(Id::Skip, [cx + 5.0, y, 150.0, BUTTON_H], "I Know What I'm Doing!"),
            Widget::button(Id::Cancel, [cx - 75.0, y + 24.0, 150.0, BUTTON_H], "Cancel"),
        ]
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        let dir = self.world.dir.clone();
        match id {
            Id::Backup => Step::push(backup(host, &self.world.dir, move |_, made: Made| match made {
                // Pop this prompt too, then load.
                Ok(_) => Step::Pop.then(Step::Out(Action::Play(dir, None))),
                Err(e) => Step::replace(Notice::new(
                    "Backup failed",
                    vec![e, "The world was not opened. Fix the problem above, then try again.".to_string()],
                )),
            })),
            Id::Skip => Step::Pop.then(Step::Out(Action::Play(dir, None))),
            Id::Cancel => Step::Pop,
        }
    }

    fn draw(&self, _: &H, ui: &mut Ui, size: Size, _: f32) {
        let (lines, shift) = self.layout(size);
        let centred = |ui: &mut Ui, y: f32, text: &str, color| {
            ui.text(((size.0 - Ui::text_width(text)) / 2.0).floor(), y, text, color);
        };
        centred(ui, 50.0 + shift, "Downgrading a world is not supported", SEVERE);
        for (i, line) in lines.iter().enumerate() {
            centred(ui, 70.0 + shift + i as f32 * LINE, line, [1.0, 1.0, 1.0, 1.0]);
        }
    }
}

type Done<H> = Box<dyn FnOnce(&mut H, Made) -> Step<H>>;

/// Java's "Creating Backup" screen: no buttons, gone when the backup ends.
pub(in crate::app) struct Waiting<H: TitleHost> {
    rx: Receiver<Made>,
    done: Option<Done<H>>,
}

/// Backs up the world in `dir` on another thread behind a waiting screen,
/// then pops it and runs `done` with the result.
pub(super) fn backup<H: TitleHost>(
    host: &H,
    dir: &str,
    done: impl FnOnce(&mut H, Made) -> Step<H> + 'static,
) -> Waiting<H> {
    let (world, backups) = (host.saves_dir().join(dir), host.backups_dir());
    let (tx, rx) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        let made = crate::update::backup::backup_world(&world, &backups);
        match &made {
            Ok((path, _)) => log::info!("backed up {} to {}", world.display(), path.display()),
            Err(e) => log::error!("backup of {} failed: {e}", world.display()),
        }
        let _ = tx.send(made);
    });
    Waiting { rx, done: Some(Box::new(done)) }
}

const WAIT_MESSAGE: &str =
    "Depending on the size of your world, this may take a while. Please do not close the game or shut off your device.";

impl<H: TitleHost> Screen<H> for Waiting<H> {
    type Id = ();

    fn title(&self, _: &H) -> String {
        "Creating Backup".into()
    }

    fn widgets(&self, _: &H, _: Size) -> Vec<Widget<()>> {
        Vec::new()
    }

    fn activate(&mut self, _: &mut H, _: ()) -> Step<H> {
        Step::Stay
    }

    /// The backup can't be stopped halfway.
    fn back(&mut self, _: &mut H) -> Step<H> {
        Step::Stay
    }

    fn tick(&mut self, host: &mut H) -> Step<H> {
        let made = match self.rx.try_recv() {
            Ok(made) => made,
            Err(crossbeam_channel::TryRecvError::Empty) => return Step::Stay,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Err("the backup stopped unexpectedly".into()),
        };
        match self.done.take() {
            Some(done) => Step::Pop.then(done(host, made)),
            None => Step::Pop,
        }
    }

    fn title_y(&self, (_, sh): Size, _: f32) -> f32 {
        (sh / 2.0 - 30.0).floor()
    }

    fn draw(&self, _: &H, ui: &mut Ui, (sw, sh): Size, _: f32) {
        for (i, line) in wrap(WAIT_MESSAGE, (sw - 50.0).min(300.0)).iter().enumerate() {
            let y = (sh / 2.0 - 10.0).floor() + i as f32 * 10.0;
            ui.text(((sw - Ui::text_width(line)) / 2.0).floor(), y, line, GREY);
        }
    }
}
