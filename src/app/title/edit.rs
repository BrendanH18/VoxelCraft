//! Java's Edit World screen (`EditWorldScreen`): rename the world, reset its
//! icon, open its folder, make a backup and open the backups folder.
//! Optimize World and Export World Generation Settings are left out:
//! VoxelCraft has no world upgrader and no data-driven generation settings.
//! In Optimize World's place is the world's seed, which copies to the
//! clipboard (Java's `/seed` line, "Seed: [n]", copies the same way).
//!
//! v0.18 Track 7: Reset Icon, as in Java, deletes the world's `icon.png`
//! (quietly; a failure leaves the file) and greys itself out; the next
//! visit to the world takes a new icon. It is greyed out from the start
//! when the world has no icon file.

use std::cell::Cell;
use std::rc::Rc;

use super::worlds::{Summary, seed_text};
use super::{TitleHost, prompts};
use crate::app::gui::{BUTTON_H, BUTTON_W, GREY, Notice, Screen, Size, Step, TextField, Widget};
use crate::render::ui::Ui;
use crate::world::storage::Storage;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Name,
    ResetIcon,
    OpenFolder,
    Backup,
    BackupsFolder,
    Seed,
    Save,
    Cancel,
}

const NAME_MAX: usize = 32;
/// Java's half-width buttons (Save and Cancel).
const HALF_W: f32 = 98.0;
const LABEL_H: f32 = 9.0;
/// Rows of full-width buttons under the name field.
const BUTTONS: [Id; 5] = [Id::ResetIcon, Id::OpenFolder, Id::Backup, Id::BackupsFolder, Id::Seed];

pub(in crate::app) struct EditWorld {
    dir: String,
    seed: u64,
    name: TextField,
    /// Tells Select World to reload its list.
    stale: Rc<Cell<bool>>,
    /// Whether the world has an icon file (looked up once, then cleared by
    /// Reset Icon), so the button isn't checked against the disk each frame.
    has_icon: Cell<Option<bool>>,
}

impl EditWorld {
    pub fn new(world: &Summary, stale: Rc<Cell<bool>>) -> Self {
        Self {
            dir: world.dir.clone(),
            seed: world.seed,
            name: TextField::with_text(&world.name, NAME_MAX),
            stale,
            has_icon: Cell::new(None),
        }
    }

    fn icon_path<H: TitleHost>(&self, host: &H) -> std::path::PathBuf {
        host.saves_dir().join(&self.dir).join(voxelcraft::title::icon::FILE)
    }

    /// Java's `Files.isRegularFile(iconFile)`.
    fn has_icon<H: TitleHost>(&self, host: &H) -> bool {
        let known = self.has_icon.get().unwrap_or_else(|| self.icon_path(host).is_file());
        self.has_icon.set(Some(known));
        known
    }

    /// Java's column, 5 pixels apart with a 20-pixel spacer above and
    /// before Save, centred; spacers and gaps shrink in short windows.
    /// Returns the label's top, the gap, the spacer and the button rows.
    fn column((_, sh): Size) -> (f32, f32, f32, usize) {
        let fits = |rows: usize, gap: f32, spacer: f32| {
            LABEL_H + BUTTON_H * (rows as f32 + 2.0) + (rows as f32 + 4.0) * gap + 2.0 * spacer <= sh - 26.0
        };
        let rows = BUTTONS.len();
        // The smallest windows pair Reset Icon with Make Backup on one row.
        let rows = if fits(rows, 1.0, 2.0) { rows } else { rows - 1 };
        let (gap, spacer) =
            [(5.0, 20.0), (3.0, 8.0), (1.0, 2.0)].into_iter().find(|&(g, s)| fits(rows, g, s)).unwrap_or((1.0, 2.0));
        let total = LABEL_H + BUTTON_H * (rows as f32 + 2.0) + (rows as f32 + 4.0) * gap + 2.0 * spacer;
        let top = ((sh - total) / 2.0).floor();
        ((top + spacer + gap).max(26.0), gap, spacer, rows)
    }

    fn rename<H: TitleHost>(&mut self, host: &mut H) -> Step<H> {
        let name = self.name.text().to_string();
        if name.trim().is_empty() {
            return Step::Stay;
        }
        let storage = Storage::new(host.saves_dir().join(&self.dir));
        // Hold the world's session lock while level.txt changes, so a window
        // that opens the world meanwhile can't save over the rename.
        let renamed = storage.lock().and_then(|_lock| storage.set_props(&[("name", &name)]));
        match renamed {
            Ok(()) => {
                self.stale.set(true);
                Step::Pop
            }
            Err(e) => {
                log::error!("failed to rename world {}: {e}", self.dir);
                Step::replace(Notice::new(
                    "Failed to access world",
                    vec![format!(
                        "Could not rename \"{}\": {e}. Its old name is kept. Check that the saves folder can be \
                         written, then try again.",
                        self.dir
                    )],
                ))
            }
        }
    }
}

impl<H: TitleHost> Screen<H> for EditWorld {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Edit World".into()
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let x = (size.0 / 2.0 - BUTTON_W / 2.0).floor();
        let (label, gap, spacer, rows) = Self::column(size);
        let paired = rows < BUTTONS.len();
        let mut y = label + LABEL_H + gap;
        let mut out = vec![Widget::field(Id::Name, [x, y, BUTTON_W, BUTTON_H])];
        for id in BUTTONS {
            let rect = match id {
                Id::ResetIcon if paired => {
                    y += BUTTON_H + gap;
                    [x, y, HALF_W, BUTTON_H]
                }
                Id::Backup if paired => [x + HALF_W + 4.0, label + LABEL_H + gap + BUTTON_H + gap, HALF_W, BUTTON_H],
                _ => {
                    y += BUTTON_H + gap;
                    [x, y, BUTTON_W, BUTTON_H]
                }
            };
            out.push(match id {
                Id::ResetIcon => Widget::button(id, rect, "Reset Icon").enabled(self.has_icon(host)),
                Id::OpenFolder => Widget::button(id, rect, "Open World Folder"),
                Id::Backup => Widget::button(id, rect, "Make Backup"),
                Id::BackupsFolder => Widget::button(id, rect, "Open Backups Folder"),
                _ => Widget::button(id, rect, format!("Seed: {}", seed_text(self.seed))).tooltip("Copy to Clipboard"),
            });
        }
        y += BUTTON_H + gap + spacer + gap;
        let named = !self.name.text().trim().is_empty();
        out.push(Widget::button(Id::Save, [x, y, HALF_W, BUTTON_H], "Save").enabled(named));
        out.push(Widget::button(Id::Cancel, [x + HALF_W + 4.0, y, HALF_W, BUTTON_H], "Cancel"));
        out
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            // Enter in the name field saves and closes, as in Java.
            Id::Name | Id::Save => self.rename(host),
            Id::ResetIcon => {
                // `FileUtils.deleteQuietly`, then the button greys out.
                if let Err(e) = std::fs::remove_file(self.icon_path(host)) {
                    log::warn!("couldn't delete the icon of {}: {e}", self.dir);
                }
                self.has_icon.set(Some(false));
                host.forget_icons();
                Step::Stay
            }
            Id::OpenFolder => {
                let path = host.saves_dir().join(&self.dir);
                host.open_path(&path);
                Step::Stay
            }
            Id::Backup => {
                let dir = self.dir.clone();
                Step::push(prompts::backup(host, &self.dir, move |_, made| match made {
                    // Java goes back to the world list and says where it went.
                    Ok((_, size)) => Step::Pop.then(Step::push(Notice::new(
                        format!("Backed up: {dir}"),
                        vec![format!("Size: {} MB", size.div_ceil(1_048_576))],
                    ))),
                    Err(e) => Step::push(Notice::new(
                        "Backup failed",
                        vec![e, "The world wasn't changed. Check for free disk space, then try again.".into()],
                    )),
                }))
            }
            Id::BackupsFolder => {
                let path = host.backups_dir();
                match std::fs::create_dir_all(&path) {
                    Ok(()) => {
                        host.open_path(&path);
                        Step::Stay
                    }
                    Err(e) => Step::push(Notice::new(
                        "Couldn't open the backups folder",
                        vec![format!("Could not create {}: {e}", path.display())],
                    )),
                }
            }
            Id::Seed => {
                host.copy_text(&seed_text(self.seed));
                Step::Stay
            }
            Id::Cancel => Step::Pop,
        }
    }

    fn field(&mut self, id: Id) -> Option<&mut TextField> {
        (id == Id::Name).then_some(&mut self.name)
    }

    fn field_ref(&self, id: Id) -> Option<&TextField> {
        (id == Id::Name).then_some(&self.name)
    }

    fn initial_focus(&self) -> Option<Id> {
        Some(Id::Name)
    }

    fn title_y(&self, size: Size, _: f32) -> f32 {
        15.0f32.min(Self::column(size).0 - 11.0)
    }

    fn draw(&self, _: &H, ui: &mut Ui, size: Size, _: f32) {
        let x = (size.0 / 2.0 - BUTTON_W / 2.0).floor();
        ui.text(x, Self::column(size).0, "World Name", GREY);
    }
}
