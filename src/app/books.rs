//! v0.15 Track B book screens on the menu toolkit: the book and quill's
//! editor and sign screen (`BookEditScreen`, `BookSignScreen`), and the
//! reader for a written book in hand or a book on a lectern
//! (`BookViewScreen`, `LecternScreen`). Like every menu here they pause an
//! unshared world. Saving and the lectern's buttons go through the shared
//! session rules (`session::books`); a LAN client sends them to the host.

use glam::IVec3;

use super::Game;
use super::gui::{BUTTON_H, Key, Screen, Size, Step, TextField, Widget};
use crate::render::ui::Ui;
use voxelcraft::content::books;
use voxelcraft::inventory::components::{WRITABLE_MAX_PAGES, WRITABLE_PAGE_LENGTH};
use winit::keyboard::KeyCode;

/// `BookViewScreen.IMAGE_WIDTH`/`IMAGE_HEIGHT`, the page text's offsets.
const IMAGE: f32 = 192.0;
const TEXT_X: f32 = 36.0;
const TEXT_Y: f32 = 30.0;
const LINE: f32 = 9.0;
const PAPER: [f32; 4] = [0.93, 0.89, 0.78, 1.0];
const INK: [f32; 4] = [0.05, 0.05, 0.05, 1.0];

fn book_left(sw: f32) -> f32 {
    ((sw - IMAGE) / 2.0).floor()
}

/// The book's page and its indicator, as `BookViewScreen.render` lays them out.
fn draw_page(ui: &mut Ui, sw: f32, page: &str, index: usize, count: usize, caret: bool) {
    let left = book_left(sw);
    ui.rect(left + 20.0, 2.0, IMAGE - 40.0, IMAGE - 10.0, [0.45, 0.3, 0.18, 1.0]);
    ui.rect(left + 24.0, 6.0, IMAGE - 48.0, IMAGE - 18.0, PAPER);
    let indicator = format!("Page {} of {}", index + 1, count.max(1));
    ui.text_flat(left + IMAGE - 44.0 - Ui::text_width(&indicator), 18.0, &indicator, INK);
    let lines = books::wrap(page, books::PAGE_WIDTH, Ui::text_width);
    for (i, line) in lines.iter().take(books::PAGE_LINES + 2).enumerate() {
        ui.text_flat(left + TEXT_X, TEXT_Y + i as f32 * LINE, line, INK);
    }
    if caret {
        let last = lines.last().map_or("", String::as_str);
        let row = lines.len().saturating_sub(1) as f32;
        ui.text_flat(left + TEXT_X + Ui::text_width(last), TEXT_Y + row * LINE, "_", INK);
    }
}

/// The page buttons (`PageButton` at x 43 and 116, y 157).
fn page_buttons<Id>(sw: f32, back: Id, forward: Id, show_back: bool, show_forward: bool) -> Vec<Widget<Id>> {
    let left = book_left(sw);
    let mut out = Vec::new();
    if show_back {
        out.push(Widget::button(back, [left + 43.0, 157.0, 23.0, 13.0], "<"));
    }
    if show_forward {
        out.push(Widget::button(forward, [left + 116.0, 157.0, 23.0, 13.0], ">"));
    }
    out
}

// ---- Book and quill ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum EditId {
    Back,
    Forward,
    Sign,
    Done,
    Title,
    Finalize,
    Cancel,
}

pub(super) struct BookEdit {
    /// Hotbar slot or `session::books::OFFHAND_SLOT`.
    slot: usize,
    pages: Vec<String>,
    page: usize,
    /// The sign screen's title box, while signing.
    signing: Option<TextField>,
}

impl BookEdit {
    fn current(&mut self) -> &mut String {
        &mut self.pages[self.page]
    }

    /// `MultiLineEditBox`'s limits: 1024 characters and 14 lines a page.
    fn fits(page: &str) -> bool {
        page.encode_utf16().count() <= WRITABLE_PAGE_LENGTH
            && books::wrap(page, books::PAGE_WIDTH, Ui::text_width).len() <= books::PAGE_LINES
    }

    fn insert(&mut self, text: &str) {
        for c in text.chars().filter(|&c| c == '\n' || (c.is_ascii() && !c.is_ascii_control())) {
            let mut next = self.current().clone();
            next.push(c);
            if Self::fits(&next) {
                *self.current() = next;
            }
        }
    }

    /// `pageForward`: past the last page a new one is added (up to 100).
    fn forward(&mut self) {
        if self.page + 1 >= self.pages.len() && self.pages.len() < WRITABLE_MAX_PAGES {
            self.pages.push(String::new());
        }
        self.page = (self.page + 1).min(self.pages.len() - 1);
    }

    /// `eraseEmptyTrailingPages`.
    fn trimmed(&self) -> Vec<String> {
        let mut pages = self.pages.clone();
        while pages.last().is_some_and(String::is_empty) {
            pages.pop();
        }
        pages
    }
}

impl Screen<Game> for BookEdit {
    type Id = EditId;

    fn title(&self, _: &Game) -> String {
        if self.signing.is_some() { String::new() } else { "Edit Book".into() }
    }

    fn widgets(&self, _: &Game, (sw, _): Size) -> Vec<Widget<EditId>> {
        let cx = (sw / 2.0).floor();
        let top = IMAGE + 4.0;
        if let Some(title) = &self.signing {
            let blank = title.text().trim().is_empty();
            return vec![
                Widget::field(EditId::Title, [cx - 60.0, 60.0, 120.0, 14.0]),
                Widget::button(EditId::Finalize, [cx - 100.0, top, 98.0, BUTTON_H], "Sign and Close").enabled(!blank),
                Widget::button(EditId::Cancel, [cx + 2.0, top, 98.0, BUTTON_H], "Cancel"),
            ];
        }
        let mut out = page_buttons(sw, EditId::Back, EditId::Forward, self.page > 0, true);
        out.push(Widget::button(EditId::Sign, [cx - 100.0, top, 98.0, BUTTON_H], "Sign"));
        out.push(Widget::button(EditId::Done, [cx + 2.0, top, 98.0, BUTTON_H], "Done"));
        out
    }

    fn activate(&mut self, game: &mut Game, id: EditId) -> Step<Game> {
        match id {
            EditId::Back => self.page = self.page.saturating_sub(1),
            EditId::Forward => self.forward(),
            EditId::Sign => self.signing = Some(TextField::new(books::SIGN_TITLE_LENGTH)),
            EditId::Cancel => self.signing = None,
            EditId::Done => {
                game.finish_book_edit(self.slot, self.trimmed(), None);
                return Step::Close;
            }
            EditId::Title | EditId::Finalize => {
                let title = self.signing.as_ref().map(|t| t.text().trim().to_string()).unwrap_or_default();
                if !title.is_empty() {
                    game.finish_book_edit(self.slot, self.pages.clone(), Some(title));
                    return Step::Close;
                }
            }
        }
        Step::Stay
    }

    fn back(&mut self, _: &mut Game) -> Step<Game> {
        // Java's book screens close on Escape without sending the pages.
        if self.signing.take().is_some() { Step::Stay } else { Step::Close }
    }

    fn field(&mut self, id: EditId) -> Option<&mut TextField> {
        (id == EditId::Title).then_some(self.signing.as_mut()?)
    }

    fn field_ref(&self, id: EditId) -> Option<&TextField> {
        (id == EditId::Title).then_some(self.signing.as_ref()?)
    }

    fn initial_focus(&self) -> Option<EditId> {
        None
    }

    /// The page takes typing, Backspace, Enter (a new line) and Page Up /
    /// Page Down (`BookEditScreen.keyPressed`).
    fn typed(&mut self, _: &mut Game, key: &Key) -> bool {
        if self.signing.is_some() {
            return false;
        }
        match key.code {
            KeyCode::Backspace => {
                self.current().pop();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => self.insert("\n"),
            KeyCode::PageUp => self.page = self.page.saturating_sub(1),
            KeyCode::PageDown => self.forward(),
            KeyCode::Escape | KeyCode::Tab => return false,
            _ => match &key.text {
                Some(text) if !key.command => self.insert(text),
                _ => return false,
            },
        }
        true
    }

    /// Closes if the book left the hand.
    fn tick(&mut self, game: &mut Game) -> Step<Game> {
        if game.book_in_slot(self.slot).is_none_or(|s| s.item != books::WRITABLE_BOOK) {
            return Step::Close;
        }
        Step::Stay
    }

    fn background(&self, _: &Game, ui: &mut Ui, (sw, sh): Size) {
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]);
        if self.signing.is_some() {
            let left = book_left(sw);
            ui.rect(left + 20.0, 2.0, IMAGE - 40.0, IMAGE - 10.0, [0.45, 0.3, 0.18, 1.0]);
            ui.rect(left + 24.0, 6.0, IMAGE - 48.0, IMAGE - 18.0, PAPER);
            let cx = (sw / 2.0).floor();
            let label = "Enter Book Title:";
            ui.text_flat(cx - Ui::text_width(label) / 2.0, 46.0, label, INK);
            let by = "by Player";
            ui.text_flat(cx - Ui::text_width(by) / 2.0, 80.0, by, [0.3, 0.3, 0.3, 1.0]);
            let note = "Note! When you sign the book, it";
            ui.text_flat(cx - Ui::text_width(note) / 2.0, 108.0, note, INK);
            let note = "will no longer be editable.";
            ui.text_flat(cx - Ui::text_width(note) / 2.0, 117.0, note, INK);
        } else {
            draw_page(ui, sw, &self.pages[self.page], self.page, self.pages.len(), true);
        }
    }
}

// ---- Reading: a written book in hand, or a lectern --------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ViewId {
    Back,
    Forward,
    Take,
    Done,
}

pub(super) struct BookView {
    /// The lectern being read, or `None` for a book in hand.
    lectern: Option<IVec3>,
    pages: Vec<String>,
    page: usize,
}

impl Screen<Game> for BookView {
    type Id = ViewId;

    fn title(&self, _: &Game) -> String {
        String::new()
    }

    fn widgets(&self, game: &Game, (sw, _): Size) -> Vec<Widget<ViewId>> {
        let cx = (sw / 2.0).floor();
        let count = self.pages.len().max(1);
        let mut out = page_buttons(sw, ViewId::Back, ViewId::Forward, self.page > 0, self.page + 1 < count);
        let top = IMAGE + 4.0;
        if self.lectern.is_some() {
            // `LecternScreen.createMenuControls`: Done and Take Book.
            out.push(Widget::button(ViewId::Done, [cx - 100.0, top, 98.0, BUTTON_H], "Done"));
            let may_build = game.session.mode.can_build();
            out.push(Widget::button(ViewId::Take, [cx + 2.0, top, 98.0, BUTTON_H], "Take Book").enabled(may_build));
        } else {
            out.push(Widget::button(ViewId::Done, [cx - 100.0, top, 200.0, BUTTON_H], "Done"));
        }
        out
    }

    fn activate(&mut self, game: &mut Game, id: ViewId) -> Step<Game> {
        let count = self.pages.len().max(1);
        let page = match id {
            ViewId::Back => self.page.saturating_sub(1),
            ViewId::Forward => (self.page + 1).min(count - 1),
            ViewId::Take => {
                if let Some(pos) = self.lectern {
                    game.lectern_take(pos);
                }
                return Step::Close;
            }
            ViewId::Done => return Step::Close,
        };
        if page != self.page {
            self.page = page;
            if let Some(pos) = self.lectern {
                game.lectern_page(pos, page);
            }
        }
        Step::Stay
    }

    fn back(&mut self, _: &mut Game) -> Step<Game> {
        Step::Close
    }

    fn typed(&mut self, game: &mut Game, key: &Key) -> bool {
        let id = match key.code {
            KeyCode::PageUp => ViewId::Back,
            KeyCode::PageDown => ViewId::Forward,
            _ => return false,
        };
        let _ = self.activate(game, id);
        true
    }

    /// A lectern's page follows the block entity (another reader may turn
    /// it); the screen closes when the book goes.
    fn tick(&mut self, game: &mut Game) -> Step<Game> {
        if let Some(pos) = self.lectern {
            let Some(l) = game.world.lectern_at(pos) else { return Step::Close };
            self.pages = books::pages(&l.book);
            self.page = l.page.min(self.pages.len().saturating_sub(1));
        }
        Step::Stay
    }

    fn background(&self, _: &Game, ui: &mut Ui, (sw, sh): Size) {
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]);
        let page = self.pages.get(self.page).map_or("", String::as_str);
        draw_page(ui, sw, page, self.page, self.pages.len(), false);
    }
}

impl Game {
    /// The stack in a book slot (hotbar index or the off-hand).
    pub(super) fn book_in_slot(&self, slot: usize) -> Option<crate::inventory::Stack> {
        if slot == voxelcraft::session::books::OFFHAND_SLOT {
            self.session.inventory.offhand
        } else {
            self.session.inventory.slots.get(slot).copied().flatten()
        }
    }

    fn open_book_screen(&mut self, screen: impl Screen<Game>) {
        if self.inventory_open {
            self.toggle_inventory();
        }
        self.menu.close();
        self.menu.open(screen);
        self.set_grab(false);
        self.keys.clear();
        self.body.left_held = false;
        self.body.right_held = false;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
    }

    /// `Player.openItemGui`: the editor for a book and quill, the reader
    /// for a written book. `slot` is a hotbar index or the off-hand.
    pub(super) fn open_held_book(&mut self, slot: usize) -> bool {
        let Some(stack) = self.book_in_slot(slot) else { return false };
        if stack.item == books::WRITABLE_BOOK {
            let mut pages = books::writable_pages(&stack).to_vec();
            if pages.is_empty() {
                pages.push(String::new());
            }
            self.open_book_screen(BookEdit { slot, pages, page: 0, signing: None });
            true
        } else if stack.item == books::WRITTEN_BOOK {
            self.open_book_screen(BookView { lectern: None, pages: books::pages(&stack), page: 0 });
            true
        } else {
            false
        }
    }

    /// `LecternBlock.openScreen`.
    pub(super) fn open_lectern(&mut self, pos: IVec3) {
        let Some(l) = self.world.lectern_at(pos) else { return };
        let (pages, page) = (books::pages(&l.book), l.page);
        self.open_book_screen(BookView { lectern: Some(pos), pages, page });
    }

    /// The editor closed with Done (`title` `None`) or Sign and Close.
    pub(super) fn finish_book_edit(&mut self, slot: usize, pages: Vec<String>, title: Option<String>) {
        if self.lan.client.is_some() {
            let pages = serde_json::to_string(&pages).unwrap_or_default();
            // The host edits the peer's selected slot (`book` commands).
            let _ = slot;
            match title {
                Some(t) => self.lan_command(&format!("book sign {} {pages}", serde_json::json!(t))),
                None => self.lan_command(&format!("book write {pages}")),
            }
            return;
        }
        let result = match title {
            Some(t) => self.session.sign_book(slot, &pages, &t, "Player"),
            None => self.session.edit_book(slot, &pages),
        };
        if let Err(e) = result {
            log::info!("book edit not applied: {e}");
        }
    }

    pub(super) fn lectern_page(&mut self, pos: IVec3, page: usize) {
        if self.lan.client.is_some() {
            self.lan_command(&format!("lectern {} {} {} page {page}", pos.x, pos.y, pos.z));
            return;
        }
        if let Err(e) = self.session.turn_lectern_page(&mut self.world, pos, page as i64) {
            log::info!("lectern page not turned: {e}");
        }
    }

    pub(super) fn lectern_take(&mut self, pos: IVec3) {
        if self.lan.client.is_some() {
            self.lan_command(&format!("lectern {} {} {} take", pos.x, pos.y, pos.z));
            return;
        }
        match self.session.take_lectern_book(&mut self.world, pos) {
            Ok(Some(rest)) => {
                self.mobs.entities.throw(rest, self.session.player.eye(), self.session.player.forward().as_dvec3())
            }
            Ok(None) => {}
            Err(e) => log::info!("lectern book not taken: {e}"),
        }
    }
}
