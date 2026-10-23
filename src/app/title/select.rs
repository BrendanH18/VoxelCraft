//! Java's Select World screen (`SelectWorldScreen` and `WorldSelectionList`):
//! a search field, the scrolling list of worlds and the footer buttons.
//!
//! As in Java, the focused row is the selected one, so the arrow keys, Tab
//! and the controller's D-pad move the selection; a click selects, and a
//! double click, Enter or the controller's A plays.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use super::worlds::{self, Summary};
use super::{Action, TitleHost, create, edit, prompts};
use crate::app::gui::{self, BUTTON_H, Confirm, Look, Notice, Rect, Screen, Size, Step, TextField, Widget, fit};
use crate::render::ui::{Color, Ui, WHITE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Search,
    /// A row, by index into the worlds.
    Row(usize),
    Play,
    Create,
    Edit,
    Delete,
    Recreate,
    Back,
}

/// Java's `HeaderAndFooterLayout(this, 8 + 9 + 8 + 20 + 4, 60)`.
const HEADER: f32 = 49.0;
const FOOTER: f32 = 60.0;
/// Java's row width and height for this list.
const ROW_W: f32 = 270.0;
const ROW_H: f32 = 36.0;
const SEARCH_MAX: usize = 32;
/// Java's double-click time.
const DOUBLE_CLICK_MS: u128 = 250;
/// Java's grey for the second and third lines (0xFF808080).
const LINE_GREY: Color = [0.5, 0.5, 0.5, 1.0];
/// `ChatFormatting.RED`, and Java's hardcore red (0xFFFF0000).
const RED: Color = [1.0, 0.33, 0.33, 1.0];
const HARDCORE_RED: Color = [1.0, 0.0, 0.0, 1.0];

pub(in crate::app) struct SelectWorld {
    search: TextField,
    worlds: Vec<Summary>,
    /// The selected world's folder.
    selected: Option<String>,
    last_click: Option<(usize, Instant)>,
    /// Set by Edit World and deletion when the list on disk changed.
    stale: Rc<Cell<bool>>,
}

impl SelectWorld {
    pub fn new<H: TitleHost>(host: &H) -> Self {
        Self::with_search(host, "")
    }

    fn with_search<H: TitleHost>(host: &H, search: &str) -> Self {
        Self {
            search: TextField::with_text(search, SEARCH_MAX).hint("Search..."),
            worlds: worlds::list(host.saves_dir()),
            selected: None,
            last_click: None,
            stale: Rc::default(),
        }
    }

    /// Worlds the search lets through, as indices.
    fn shown(&self) -> Vec<usize> {
        (0..self.worlds.len()).filter(|&i| self.worlds[i].matches(self.search.text())).collect()
    }

    /// The selected world, if the search shows it.
    fn current(&self) -> Option<&Summary> {
        let i = self.worlds.iter().position(|w| Some(&w.dir) == self.selected.as_ref())?;
        self.worlds[i].matches(self.search.text()).then(|| &self.worlds[i])
    }

    fn reload<H: TitleHost>(&mut self, host: &H) {
        self.worlds = worlds::list(host.saves_dir());
        self.last_click = None;
    }

    fn list_rect((sw, sh): Size) -> Rect {
        // The scroll bar goes 8 pixels right of the rows, as in Java.
        [((sw - ROW_W) / 2.0).floor(), HEADER, ROW_W + 14.0, (sh - HEADER - FOOTER).max(ROW_H)]
    }

    /// Java's delete question; Delete removes the folder and refreshes the list.
    fn confirm_delete<H: TitleHost>(world: &Summary, stale: Rc<Cell<bool>>) -> Confirm<H> {
        let dir = world.dir.clone();
        Confirm::new(
            "Are you sure you want to delete this world?",
            vec![format!("'{}' will be lost forever! (A long time!)", world.name)],
            "Delete",
            move |host: &mut H| {
                stale.set(true);
                match delete(host, &dir) {
                    Ok(()) => Step::Stay,
                    Err(e) => Step::push(Notice::new("Failed to delete world", vec![e])),
                }
            },
        )
    }
}

/// Deletes a world's folder, refusing names that aren't plain world folders.
pub(super) fn delete<H: TitleHost>(host: &H, dir: &str) -> Result<(), String> {
    crate::data::validate_world_name(dir).map_err(|e| format!("\"{dir}\" isn't a world folder: {e}"))?;
    let path = host.saves_dir().join(dir);
    if crate::world::storage::Storage::new(&path).locked_elsewhere() {
        return Err(format!("\"{dir}\" is open in another VoxelCraft window. Close it, then try again."));
    }
    std::fs::remove_dir_all(&path).map_err(|e| {
        format!(
            "Could not delete \"{dir}\": {e}. Some of it may be left; delete the folder by hand from the saves folder."
        )
    })?;
    log::info!("deleted world {}", path.display());
    Ok(())
}

/// Opens a world the way Java's `WorldOpenFlows` does: a world last saved by
/// a newer version asks for a backup first. The summary is read again first,
/// so a save made by another window since the list was built still counts.
pub(super) fn open<H: TitleHost>(host: &H, world: &Summary) -> Step<H> {
    let Some(world) = worlds::summary(host.saves_dir(), &world.dir) else {
        return Step::push(Notice::new("Failed to access world", vec![format!("\"{}\" is gone.", world.dir)]));
    };
    if world.locked {
        return Step::push(Notice::new(
            "Failed to access world",
            vec![format!("\"{}\" is open in another VoxelCraft window. Close it there, then try again.", world.name)],
        ));
    }
    if let Some(e) = &world.unreadable {
        return Step::push(Notice::new(
            "Failed to access world",
            vec![format!(
                "\"{}\" can't be opened: its level.txt is unreadable ({e}). Restore it from the backups folder.",
                world.name
            )],
        ));
    }
    match world.newer() {
        Some(version) => Step::push(prompts::BackupConfirm::downgrade(world.clone(), version)),
        None => Step::Out(Action::Play(world.dir.clone(), None)),
    }
}

impl<H: TitleHost> Screen<H> for SelectWorld {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Select World".into()
    }

    fn widgets(&self, _: &H, size: Size) -> Vec<Widget<Id>> {
        let (sw, sh) = size;
        let cx = (sw / 2.0).floor();
        let mut out = vec![Widget::field(Id::Search, [cx - 100.0, 21.0, 200.0, BUTTON_H])];
        let [x, top, ..] = Self::list_rect(size);
        for (k, i) in self.shown().into_iter().enumerate() {
            let mut row = Widget::custom(Id::Row(i), [x, top + 4.0 + k as f32 * ROW_H, ROW_W, ROW_H]).scrolling();
            let world = &self.worlds[i];
            if world.locked {
                row = row.tooltip("Locked by another running instance of VoxelCraft.");
            } else if let Some(e) = &world.unreadable {
                row = row.tooltip(format!("Its level.txt can't be read: {e}"));
            } else if world.newer().is_some() {
                row = row.tooltip("World was saved in a newer version,\nand loading it could cause problems!");
            }
            out.push(row);
        }
        let current = self.current();
        // Java's `isDisabled`/`requiresFileFixing`: locked or unreadable worlds
        // can be deleted but not played or edited.
        let usable = current.is_some_and(|w| !w.locked && w.unreadable.is_none());
        // Java's footer: a 4-column grid, 71-pixel columns 8 apart, rows 4 apart.
        let left = cx - 154.0;
        let (row1, row2) = ((sh - FOOTER + 8.0).floor(), (sh - FOOTER + 32.0).floor());
        let col = |c: f32, y: f32, w: f32| [left + c * 79.0, y, w, BUTTON_H];
        out.extend([
            Widget::button(Id::Play, col(0.0, row1, 150.0), "Play Selected World").enabled(usable),
            Widget::button(Id::Create, col(2.0, row1, 150.0), "Create New World"),
            Widget::button(Id::Edit, col(0.0, row2, 71.0), "Edit").enabled(usable),
            Widget::button(Id::Delete, col(1.0, row2, 71.0), "Delete").enabled(current.is_some_and(|w| !w.locked)),
            Widget::button(Id::Recreate, col(2.0, row2, 71.0), "Re-Create")
                // Java's `canRecreate`: not locked by another window, and readable.
                .enabled(current.is_some_and(|w| !w.locked && w.unreadable.is_none())),
            Widget::button(Id::Back, col(3.0, row2, 71.0), "Back"),
        ]);
        out
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        if let Id::Row(i) = id {
            self.selected = self.worlds.get(i).map(|w| w.dir.clone());
            return match self.current() {
                Some(world) if !world.locked => open(host, world),
                _ => Step::Stay,
            };
        }
        let current = self.current().cloned();
        match (id, current) {
            (Id::Play, Some(world)) => open(host, &world),
            (Id::Create, _) => Step::push(create::CreateWorld::new()),
            (Id::Edit, Some(world)) => Step::push(edit::EditWorld::new(&world, self.stale.clone())),
            (Id::Delete, Some(world)) => Step::push(Self::confirm_delete(&world, self.stale.clone())),
            (Id::Recreate, Some(world)) => Step::push(create::CreateWorld::recreate(&world)),
            (Id::Back, _) => Step::Pop,
            _ => Step::Stay,
        }
    }

    fn click(&mut self, host: &mut H, id: Id) -> Step<H> {
        let Id::Row(i) = id else { return self.activate(host, id) };
        let double = self.last_click.is_some_and(|(j, t)| j == i && t.elapsed().as_millis() < DOUBLE_CLICK_MS);
        self.selected = self.worlds.get(i).map(|w| w.dir.clone());
        self.last_click = Some((i, Instant::now()));
        if double { self.activate(host, id) } else { Step::Stay }
    }

    fn focus(&mut self, _: &mut H, id: Id) {
        if let Id::Row(i) = id {
            self.selected = self.worlds.get(i).map(|w| w.dir.clone());
        }
    }

    fn field(&mut self, id: Id) -> Option<&mut TextField> {
        (id == Id::Search).then_some(&mut self.search)
    }

    fn field_ref(&self, id: Id) -> Option<&TextField> {
        (id == Id::Search).then_some(&self.search)
    }

    fn tick(&mut self, host: &mut H) -> Step<H> {
        if self.stale.take() {
            self.reload(host);
            // Java opens Create New World once the last world is gone.
            if self.worlds.is_empty() {
                return Step::replace(create::CreateWorld::new());
            }
        }
        Step::Stay
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(Self::list_rect(size))
    }

    fn scroll_step(&self) -> f32 {
        ROW_H / 2.0
    }

    fn initial_focus(&self) -> Option<Id> {
        Some(Id::Search)
    }

    fn title_y(&self, _: Size, _: f32) -> f32 {
        8.0
    }

    fn draw_custom(&self, _: &H, ui: &mut Ui, id: Id, rect: Rect, look: Look) {
        let Id::Row(i) = id else { return };
        let Some(world) = self.worlds.get(i) else { return };
        let [x, y, w, h] = rect;
        if self.selected.as_ref() == Some(&world.dir) {
            // Java's selection: a white outline while the list has focus, grey otherwise.
            let outline = if look.focused { [1.0, 1.0, 1.0, 1.0] } else { LINE_GREY };
            ui.rect(x, y, w, h, outline);
            ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [0.0, 0.0, 0.0, 1.0]);
        }
        let (cx, cy) = (x + 2.0, y + 2.0);
        draw_icon(ui, [cx, cy, 32.0, 32.0], look.hovered);
        let text_x = cx + 35.0;
        let room = x + w - 2.0 - text_x;
        ui.text(text_x, cy + 1.0, &fit(&world.name, room), WHITE);
        ui.text(text_x, cy + 12.0, &fit(&world.folder_and_date(), room), LINE_GREY);
        draw_segments(ui, text_x, cy + 21.0, room, &info(world));
    }
}

/// Java's third line: game mode, then notes, then the saving version.
fn info(world: &Summary) -> Vec<(String, Color)> {
    if world.locked {
        return vec![("Locked by another running instance of VoxelCraft.".into(), RED)];
    }
    if world.unreadable.is_some() {
        return vec![("Failed to access world".into(), RED)];
    }
    let mode = if world.hardcore { HARDCORE_RED } else { LINE_GREY };
    let mut out = vec![(world.mode_name().to_string(), mode)];
    if world.nether {
        out.push((", in the Nether".into(), LINE_GREY));
    }
    if world.legacy {
        out.push((", pre-0.6 terrain".into(), LINE_GREY));
    }
    out.push((", Version: ".into(), LINE_GREY));
    let version = if world.newer().is_some() { RED } else { LINE_GREY };
    out.push((world.version_name(), version));
    out
}

/// Coloured runs of text on one line, cut short with "..." to `room` as
/// Java's clamped text widgets are.
fn draw_segments(ui: &mut Ui, x: f32, y: f32, room: f32, parts: &[(String, Color)]) {
    let whole: String = parts.iter().map(|(t, _)| t.as_str()).collect();
    let fitted = fit(&whole, room);
    // `fit` keeps a prefix of the text, then adds "...".
    let cut = fitted != whole;
    let mut left = if cut { fitted.chars().count() - 3 } else { usize::MAX };
    let (mut at, mut color) = (x, LINE_GREY);
    for (text, c) in parts {
        if left == 0 {
            break;
        }
        let end = text.char_indices().nth(left).map_or(text.len(), |(i, _)| i);
        let shown = &text[..end];
        ui.text(at, y, shown, *c);
        at += Ui::text_width(shown);
        left = left.saturating_sub(shown.chars().count());
        color = *c;
    }
    if cut {
        ui.text(at, y, "...", color);
    }
}

/// VoxelCraft has no world icons: Java's blank icon, with its play overlay
/// while the row is hovered.
fn draw_icon(ui: &mut Ui, [x, y, w, h]: Rect, hovered: bool) {
    gui::bevel(ui, [x, y, w, h], [0.24, 0.24, 0.24, 1.0], false);
    if hovered {
        ui.rect(x, y, w, h, [0.565, 0.565, 0.565, 0.627]);
        ui.text_scaled(x + 10.0, y + 8.0, ">", WHITE, 2.0);
    }
}
