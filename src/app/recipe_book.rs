//! v0.18 Track 5: Java 26.3's recipe book beside the inventory, the
//! crafting table and the three furnaces (`RecipeBookComponent`,
//! `RecipeBookPage`, `RecipeButton`, `RecipeBookTabButton`, `GhostSlots`
//! and `RecipeToast`), with original drawing.
//!
//! The player's known recipes, highlights and the open and filter setting
//! of each book live in their session (`crafting::book::RecipeBook`) and
//! are saved; this view keeps the selected tab, page, search text, ghost
//! recipe and toasts. Clicking a recipe runs the shared placement on the
//! host (`screen::Screen::place_recipe`); a LAN guest asks the host.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use voxelcraft::crafting::book::{self as rb, BookType, RecipeId, Shown, place::Outcome, stacked::Stacked};

use super::hud::SlotRef;
use super::{Container, Game, GameMode};
use crate::inventory::Stack;
use crate::item::Item;
use crate::render::ui::{Color, Ui, WHITE};

const SLOT: f32 = 18.0;
const PANEL_W: f32 = 9.0 * SLOT + 14.0;
const TAB_W: f32 = 35.0;
const TAB_H: f32 = 27.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// The screen a book is open on: its book type and crafting grid size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BookMenu {
    pub book: BookType,
    /// The crafting grid's size; `None` on a furnace.
    pub grid: Option<usize>,
}

/// What the book's page was computed from.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    menu: BookMenu,
    revision: u32,
    tab: usize,
    query: String,
    filtering: bool,
    contents: u64,
}

/// One toast entry: the workstation and the unlocked result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ToastEntry {
    pub station: Item,
    pub result: Stack,
}

/// The view state of the book (not saved).
#[derive(Default)]
pub(super) struct View {
    pub tab: usize,
    pub page: usize,
    pub query: String,
    pub search_focused: bool,
    /// The recipe shown as a ghost in the grid (`GhostSlots`), until a
    /// crafting slot is clicked.
    pub ghost: Option<RecipeId>,
    /// `lastRecipe`: Enter or Space places it again.
    pub last: Option<RecipeId>,
    /// `lastPlacedRecipe`: an uncraftable recipe isn't placed twice running.
    pub last_placed: Option<RecipeId>,
    /// The alternatives of a grouped button, opened with a right click
    /// (`OverlayRecipeComponent`): the collection's index on the page and
    /// where its first entry is drawn.
    pub overlay: Option<(usize, f32, f32)>,
    /// The page, computed when the book or the items change.
    pub shown: Vec<Shown>,
    /// Which of the book's tabs have anything that fits, computed with the page.
    tabs: Vec<bool>,
    key: Option<Key>,
    /// When a button last pulsed for newly shown recipes (`animationTime`).
    pulses: Vec<(RecipeId, Instant)>,
    /// `RecipeToast` entries and when the toast last changed.
    pub toasts: VecDeque<ToastEntry>,
    pub toast_changed: Option<Instant>,
    /// A LAN guest has had its first book from the host (whose recipes
    /// arrive without toasts, as Java's initial recipe book does).
    pub synced: bool,
}

impl View {
    /// `SlotSelectTime`: one step every 30 ticks while the screen shows.
    fn cycle(now: Instant, started: Instant) -> usize {
        ((now - started).as_secs_f32() * 20.0 / rb::TICKS_TO_SWAP as f32) as usize
    }
}

/// Positions of everything on the book (`RecipeBookComponent.initVisuals`).
pub(super) struct Layout {
    pub origin: (f32, f32),
    pub search: Rect,
    pub magnifier: Rect,
    pub filter: Rect,
    /// Visible tabs: their index in the book's tab list and rectangle.
    pub tabs: Vec<(usize, Rect)>,
    pub buttons: [Rect; rb::PER_PAGE],
    pub forward: Rect,
    pub back: Rect,
}

/// Something on the book under the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Control {
    Toggle,
    Search,
    Filter,
    Tab(usize),
    Button(usize),
    Forward,
    Back,
    Overlay(usize),
    /// The book's background: takes the click and does nothing.
    Book,
}

/// The fuels the furnace ghost cycles through (`SlotDisplay.AnyFuel`).
fn fuels() -> &'static [Item] {
    static FUELS: std::sync::OnceLock<Vec<Item>> = std::sync::OnceLock::new();
    FUELS.get_or_init(|| Item::creative_palette().filter(|&i| crate::world::furnace::burn_time(i).is_some()).collect())
}

/// A cheap fingerprint of what the player could craft from.
fn fingerprint(stacks: impl Iterator<Item = Option<Stack>>) -> u64 {
    stacks.fold(0xcbf2_9ce4_8422_2325u64, |h, s| {
        let v = s.map_or(0, |s| (u64::from(s.item.0) << 16) | (u64::from(s.count) << 1) | u64::from(s.damage == 0));
        (h ^ v).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Game {
    /// The book of the open screen, if it has one: the survival inventory's
    /// 2x2 grid, the crafting table and the furnaces. Creative's inventory
    /// is the palette, as in Java.
    pub(super) fn book_menu(&self) -> Option<BookMenu> {
        if !self.inventory_open || self.session.mode == GameMode::Spectator {
            return None;
        }
        match self.body.container {
            Container::Inventory if self.session.mode.is_survival() => {
                Some(BookMenu { book: BookType::Crafting, grid: Some(2) })
            }
            Container::CraftingTable => Some(BookMenu { book: BookType::Crafting, grid: Some(3) }),
            Container::Furnace(p) => {
                let kind = self.world.furnace(p).map(|f| f.kind).unwrap_or_default();
                Some(BookMenu { book: BookType::of_furnace(kind), grid: None })
            }
            _ => None,
        }
    }

    pub(super) fn book_open(&self) -> bool {
        self.book_menu().is_some_and(|m| self.session.recipe_book.settings(m.book).open)
    }

    /// `AbstractRecipeBookScreen.widthTooNarrow`.
    pub(super) fn book_narrow(screen_w: f32) -> bool {
        screen_w < rb::NARROW_BELOW
    }

    /// The inventory panel's left edge (`RecipeBookComponent.updateScreenPosition`):
    /// beside an open book on a wide screen, else centred.
    pub(super) fn panel_left(&self, screen_w: f32) -> f32 {
        if self.book_open() && !Self::book_narrow(screen_w) {
            (177.0 + ((screen_w - PANEL_W - 200.0) / 2.0).floor()).floor()
        } else {
            ((screen_w - PANEL_W) / 2.0).floor()
        }
    }

    pub(super) fn book_layout(&self, (sw, sh): (f32, f32)) -> Layout {
        let narrow = Self::book_narrow(sw);
        let xo = ((sw - rb::IMAGE_WIDTH) / 2.0).floor() - if narrow { 0.0 } else { rb::OFFSET_X };
        let yo = ((sh - rb::IMAGE_HEIGHT) / 2.0).floor();
        let search = Rect { x: xo + 25.0, y: yo + 13.0, w: 81.0, h: 14.0 };
        let mut tabs = Vec::new();
        if let Some(menu) = self.book_menu() {
            let mut row = 0.0;
            for (i, tab) in menu.book.tabs().iter().enumerate() {
                // `RecipeBookTabButton.updateVisibility`: a category tab shows
                // when it has anything that fits.
                let visible = tab.category.is_none() || self.recipe_book.tabs.get(i).copied().unwrap_or(false);
                if visible {
                    tabs.push((i, Rect { x: xo - 30.0, y: yo + 3.0 + TAB_H * row, w: TAB_W, h: TAB_H }));
                    row += 1.0;
                }
            }
        }
        let mut buttons = [Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; rb::PER_PAGE];
        for (i, b) in buttons.iter_mut().enumerate() {
            *b = Rect {
                x: xo + 11.0 + rb::BUTTON_SIZE * (i % 5) as f32,
                y: yo + 31.0 + rb::BUTTON_SIZE * (i / 5) as f32,
                w: rb::BUTTON_SIZE,
                h: rb::BUTTON_SIZE,
            };
        }
        Layout {
            origin: (xo, yo),
            magnifier: Rect { x: xo + 8.0, y: search.y, w: search.x - xo - 8.0, h: search.h },
            search,
            filter: Rect { x: xo + 110.0, y: yo + 12.0, w: 26.0, h: 16.0 },
            tabs,
            buttons,
            forward: Rect { x: xo + 93.0, y: yo + 137.0, w: 12.0, h: 17.0 },
            back: Rect { x: xo + 38.0, y: yo + 137.0, w: 12.0, h: 17.0 },
        }
    }

    fn tab_has_recipes(&self, menu: BookMenu, tab: usize) -> bool {
        let Some(category) = menu.book.tabs()[tab].category else { return true };
        rb::collections(&self.session.recipe_book).iter().filter(|c| c.category == category).any(|c| {
            c.recipes.iter().any(|id| {
                let r = id.recipe().unwrap();
                match (menu.grid, menu.book.cooking()) {
                    (Some(size), _) => r.fits(size),
                    (None, Some(kind)) => r.kind == kind,
                    _ => false,
                }
            })
        })
    }

    /// The recipe book button (`AbstractRecipeBookScreen.initButton`), placed
    /// against the grid as Java places it: under the 2x2 grid, left of the
    /// 3x3 grid, left of the furnace's input.
    pub(super) fn book_toggle_rect(&self, screen: (f32, f32)) -> Option<Rect> {
        let menu = self.book_menu()?;
        let slots = self.inventory_slots(screen);
        let at = |slot: SlotRef| slots.iter().find(|(r, _, _)| *r == slot).map(|&(_, x, y)| (x, y));
        let (x, y) = match (menu.grid, self.body.container) {
            // `InventoryScreen.initButton`: (leftPos + 104, height / 2 - 22),
            // which is (104, 61) in the 176 x 166 panel whose 2x2 grid's
            // first item sits at (98, 18). v0.18 Track 4 put the grid at
            // Java's slot positions (frames a pixel up and left of the
            // item corners), so the button keeps Java's offset from it.
            (Some(2), _) => inventory_toggle(at(SlotRef::Craft(0))?),
            (Some(_), _) => {
                let (gx, gy) = at(SlotRef::Craft(0))?;
                (gx - 25.0, gy + 17.0)
            }
            (None, _) => {
                let (ix, iy) = at(SlotRef::FurnaceInput)?;
                (ix - 36.0, iy + 17.0)
            }
        };
        Some(Rect { x, y, w: 20.0, h: 18.0 })
    }

    /// The control under the mouse, the toggle button included.
    pub(super) fn book_control(&self) -> Option<Control> {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        let screen = (w as f32 / scale, h as f32 / scale);
        let mouse = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        let menu = self.book_menu()?;
        if self.book_open() {
            let l = self.book_layout(screen);
            if let Some((index, x, y)) = self.recipe_book.overlay {
                let count = self.recipe_book.shown.get(index).map_or(0, |s| s.entries(self.filtering(menu)).len());
                for i in 0..count {
                    let r = overlay_rect(x, y, count, i);
                    if r.contains(mouse) {
                        return Some(Control::Overlay(i));
                    }
                }
            }
            if let Some(i) = l.buttons.iter().position(|r| r.contains(mouse))
                && self.page_entry(i).is_some()
            {
                return Some(Control::Button(i));
            }
            let pages = self.page_count();
            if pages > 1 && self.recipe_book.page + 1 < pages && l.forward.contains(mouse) {
                return Some(Control::Forward);
            }
            if pages > 1 && self.recipe_book.page > 0 && l.back.contains(mouse) {
                return Some(Control::Back);
            }
            if l.search.contains(mouse) || l.magnifier.contains(mouse) {
                return Some(Control::Search);
            }
            if l.filter.contains(mouse) {
                return Some(Control::Filter);
            }
            if let Some(&(i, _)) = l.tabs.iter().find(|(_, r)| r.contains(mouse)) {
                return Some(Control::Tab(i));
            }
            let (xo, yo) = l.origin;
            if (Rect { x: xo, y: yo, w: rb::IMAGE_WIDTH, h: rb::IMAGE_HEIGHT }).contains(mouse) {
                return Some(Control::Book);
            }
        }
        if !(self.book_open() && Self::book_narrow(screen.0))
            && self.book_toggle_rect(screen).is_some_and(|r| r.contains(mouse))
        {
            return Some(Control::Toggle);
        }
        let _ = menu;
        None
    }

    /// Whether the open book hides the menu (a narrow screen), so slots
    /// under it take no clicks.
    pub(super) fn book_covers_menu(&self) -> bool {
        let scale = self.ui_scale();
        self.book_open() && Self::book_narrow(self.ui_size().0 as f32 / scale)
    }

    fn filtering(&self, menu: BookMenu) -> bool {
        self.session.recipe_book.settings(menu.book).filtering
    }

    fn page_count(&self) -> usize {
        self.recipe_book.shown.len().div_ceil(rb::PER_PAGE)
    }

    fn page_entry(&self, button: usize) -> Option<&Shown> {
        self.recipe_book.shown.get(self.recipe_book.page * rb::PER_PAGE + button)
    }

    /// The player's items the book counts (`StackedItemContents`): the
    /// inventory plus the grid or furnace.
    fn stacked(&self, menu: BookMenu) -> Stacked {
        let mut available = Stacked::of_inventory(&self.session.inventory);
        match (menu.grid, self.body.container) {
            (Some(n), _) => {
                for s in self.craft.cells[..n * n].iter().flatten() {
                    available.account_simple(s);
                }
            }
            (None, Container::Furnace(p)) => {
                if let Some(f) = self.world.furnace(p) {
                    for s in [f.input, f.fuel, f.output].iter().flatten() {
                        available.account(s);
                    }
                }
            }
            _ => {}
        }
        available
    }

    /// Once a frame: rebuilds the page when the book or items change,
    /// clears the highlight of recipes it shows (telling a LAN host), and
    /// moves new unlocks to the toast.
    pub(super) fn recipe_book_frame(&mut self) {
        self.pad_recipe_books_seen();
        let now = Instant::now();
        let fresh = self.session.recipe_book.take_toasts();
        if !fresh.is_empty() {
            if self.lan.client.is_some() && !self.recipe_book.synced {
                self.recipe_book.synced = true;
            } else {
                for id in fresh {
                    if let Some((station, result)) = id.toast() {
                        self.recipe_book.toasts.push_back(ToastEntry { station, result });
                    }
                }
                self.recipe_book.toast_changed = Some(now);
            }
        }
        if self.lan.client.is_some() {
            self.recipe_book.synced |= self.session.recipe_book.known_count() > 0;
        }
        if let Some(changed) = self.recipe_book.toast_changed
            && now - changed > Duration::from_millis(rb::TOAST_MILLIS + 1000)
        {
            self.recipe_book.toasts.clear();
            self.recipe_book.toast_changed = None;
        }
        self.recipe_book.pulses.retain(|(_, at)| now - *at < Duration::from_millis(750));
        let Some(menu) = self.book_menu().filter(|m| self.session.recipe_book.settings(m.book).open) else {
            self.recipe_book.key = None;
            return;
        };
        let tabs = menu.book.tabs();
        if self.recipe_book.tab >= tabs.len() {
            self.recipe_book.tab = 0;
        }
        let contents = match (menu.grid, self.body.container) {
            (Some(n), _) => fingerprint(
                self.session.inventory.slots.iter().copied().chain(self.craft.cells[..n * n].iter().copied()),
            ),
            (None, Container::Furnace(p)) => {
                let f = self.world.furnace(p);
                fingerprint(self.session.inventory.slots.iter().copied().chain([
                    f.and_then(|f| f.input),
                    f.and_then(|f| f.fuel),
                    f.and_then(|f| f.output),
                ]))
            }
            _ => 0,
        };
        let key = Key {
            menu,
            revision: self.session.recipe_book.revision,
            tab: self.recipe_book.tab,
            query: self.recipe_book.query.clone(),
            filtering: self.filtering(menu),
            contents,
        };
        if self.recipe_book.key.as_ref() == Some(&key) {
            return;
        }
        let reset = self.recipe_book.key.as_ref().is_none_or(|k| k.tab != key.tab || k.menu != key.menu);
        self.recipe_book.tabs = (0..tabs.len()).map(|i| self.tab_has_recipes(menu, i)).collect();
        let available = self.stacked(menu);
        self.recipe_book.shown =
            rb::page(&self.session.recipe_book, menu.book, tabs[key.tab].category, menu.grid, &key.query, &available);
        let pages = self.page_count();
        if reset || self.recipe_book.page >= pages {
            self.recipe_book.page = 0;
        }
        self.recipe_book.key = Some(key);
        self.mark_page_seen(menu);
    }

    /// `RecipeButton.init`: recipes shown on the page lose their highlight,
    /// and their button pulses once.
    fn mark_page_seen(&mut self, menu: BookMenu) {
        let filtering = self.filtering(menu);
        let start = self.recipe_book.page * rb::PER_PAGE;
        let seen: Vec<RecipeId> = self
            .recipe_book
            .shown
            .iter()
            .skip(start)
            .take(rb::PER_PAGE)
            .flat_map(|s| s.entries(filtering).iter().copied())
            .filter(|&id| self.session.recipe_book.highlighted(id))
            .collect();
        let now = Instant::now();
        for id in seen {
            self.session.recipe_book.mark_seen(id);
            self.recipe_book.pulses.push((id, now));
            if self.lan.client.is_some() {
                self.lan_command(&format!("recipe_seen {}", id.name()));
            }
        }
        // The highlight change bumped the revision: keep the page as computed.
        if let Some(k) = &mut self.recipe_book.key {
            k.revision = self.session.recipe_book.revision;
        }
    }

    /// A left or right click on the book or its button. True when it took
    /// the click.
    pub(super) fn recipe_book_click(&mut self, right: bool) -> bool {
        let Some(control) = self.book_control() else { return false };
        let Some(menu) = self.book_menu() else { return false };
        let shift = self.modifiers.shift_key();
        if !matches!(control, Control::Search) {
            self.recipe_book.search_focused = false;
        }
        if !matches!(control, Control::Overlay(_) | Control::Button(_)) {
            self.recipe_book.overlay = None;
        }
        match control {
            Control::Toggle if !right => {
                let open = !self.session.recipe_book.settings(menu.book).open;
                self.set_book_settings(menu.book, Some(open), None);
                self.audio.ui_click();
            }
            Control::Toggle => return false,
            Control::Search => self.recipe_book.search_focused = true,
            Control::Filter if !right => {
                let filtering = !self.filtering(menu);
                self.set_book_settings(menu.book, None, Some(filtering));
                self.audio.ui_click();
            }
            Control::Tab(i) if !right => {
                if self.recipe_book.tab != i {
                    self.recipe_book.tab = i;
                    self.recipe_book.page = 0;
                    self.audio.ui_click();
                }
            }
            Control::Forward if !right => {
                self.recipe_book.page += 1;
                self.recipe_book.key = None;
                self.audio.ui_click();
            }
            Control::Back if !right => {
                self.recipe_book.page = self.recipe_book.page.saturating_sub(1);
                self.recipe_book.key = None;
                self.audio.ui_click();
            }
            Control::Button(i) => {
                let Some(shown) = self.page_entry(i).cloned() else { return true };
                let entries = shown.entries(self.filtering(menu)).to_vec();
                if right {
                    // `RecipeBookPage.mouseClicked`: right click opens the
                    // alternatives of a grouped button.
                    if entries.len() > 1 && self.recipe_book.overlay.is_none() {
                        let l = self.book_layout(self.screen_size());
                        let b = l.buttons[i];
                        self.recipe_book.overlay = Some((self.recipe_book.page * rb::PER_PAGE + i, b.x, b.y));
                    }
                } else if let Some(&id) = entries.get(self.cycle_index() % entries.len().max(1)) {
                    self.recipe_book.overlay = None;
                    self.click_recipe(menu, &shown, id, shift);
                }
            }
            Control::Overlay(i) => {
                if let Some((index, ..)) = self.recipe_book.overlay
                    && let Some(shown) = self.recipe_book.shown.get(index).cloned()
                    && let Some(&id) = shown.entries(self.filtering(menu)).get(i)
                {
                    self.recipe_book.overlay = None;
                    self.click_recipe(menu, &shown, id, shift);
                }
            }
            _ => {}
        }
        true
    }

    fn screen_size(&self) -> (f32, f32) {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        (w as f32 / scale, h as f32 / scale)
    }

    fn cycle_index(&self) -> usize {
        View::cycle(Instant::now(), self.started)
    }

    /// `RecipeBookComponent.tryPlaceRecipe` and the host's `handlePlaceRecipe`.
    fn click_recipe(&mut self, menu: BookMenu, shown: &Shown, id: RecipeId, max: bool) {
        let craftable = shown.craftable.contains(&id);
        if !craftable && self.recipe_book.last_placed == Some(id) {
            return;
        }
        self.recipe_book.last_placed = Some(id);
        self.recipe_book.last = Some(id);
        self.recipe_book.ghost = None;
        self.audio.ui_click();
        if self.lan.client.is_some() {
            // The host places; without the items it empties the grid, and
            // the guest shows the ghost as Java's client does on its reply.
            self.lan_command(&format!("recipe_place {} {}", id.name(), if max { "max" } else { "one" }));
            if !craftable {
                self.recipe_book.ghost = Some(id);
            }
        } else {
            let selected = self.body.actions.selected;
            let outcome = self.with_screen(|s| s.place_recipe(id, max, selected));
            if outcome == Some(Outcome::Ghost) {
                self.recipe_book.ghost = Some(id);
            }
        }
        // A narrow screen's book covers the menu, so it closes after a pick.
        if Self::book_narrow(self.screen_size().0) {
            self.set_book_settings(menu.book, Some(false), None);
        }
        self.recipe_book.key = None;
    }

    /// Changes this player's book settings (sent to a LAN host).
    fn set_book_settings(&mut self, book: BookType, open: Option<bool>, filtering: Option<bool>) {
        if let Some(open) = open {
            self.session.recipe_book.set_open(book, open);
            self.recipe_book.overlay = None;
        }
        if let Some(filtering) = filtering {
            self.session.recipe_book.set_filtering(book, filtering);
        }
        self.recipe_book.key = None;
        if self.lan.client.is_some() {
            let s = self.session.recipe_book.settings(book);
            self.lan_command(&format!("recipe_settings {} {} {}", book_name(book), s.open as u8, s.filtering as u8));
        }
    }

    /// A crafting slot was clicked: the ghost goes (`RecipeBookComponent.slotClicked`).
    pub(super) fn recipe_slot_clicked(&mut self, slot: SlotRef) {
        if matches!(
            slot,
            SlotRef::Craft(_)
                | SlotRef::CraftResult
                | SlotRef::FurnaceInput
                | SlotRef::FurnaceFuel
                | SlotRef::FurnaceOutput
        ) {
            self.recipe_book.ghost = None;
            self.recipe_book.last_placed = None;
        }
    }

    /// Keys for the book (`RecipeBookComponent.keyPressed` and `charTyped`):
    /// typing in the search box, the chat key to focus it, Escape to close a
    /// narrow book, Enter or Space to place the last recipe again.
    pub(super) fn recipe_book_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::keyboard::{KeyCode, PhysicalKey};
        let Some(menu) = self.book_menu() else { return false };
        if !self.book_open() || event.state != winit::event::ElementState::Pressed {
            return false;
        }
        let PhysicalKey::Code(code) = event.physical_key else { return self.recipe_book.search_focused };
        if code == KeyCode::Escape && Self::book_narrow(self.screen_size().0) {
            self.recipe_book.search_focused = false;
            self.set_book_settings(menu.book, Some(false), None);
            return true;
        }
        if self.recipe_book.search_focused {
            match code {
                KeyCode::Escape => self.recipe_book.search_focused = false,
                KeyCode::Enter | KeyCode::NumpadEnter => self.recipe_book.search_focused = false,
                KeyCode::Backspace | KeyCode::Delete => {
                    self.recipe_book.query.pop();
                }
                _ => {
                    if !self.modifiers.control_key()
                        && !self.modifiers.super_key()
                        && let Some(text) = &event.text
                    {
                        for c in text.chars().filter(|&c| voxelcraft::font::allowed(c)) {
                            // `EditBox.setMaxLength(50)`.
                            if self.recipe_book.query.chars().count() < 50 {
                                self.recipe_book.query.push(c);
                            }
                        }
                    }
                }
            }
            return code != KeyCode::Escape || self.recipe_book.search_focused;
        }
        if self.settings.keys.is(super::keys::Action::Chat, super::keys::Input::Key(code)) {
            self.recipe_book.search_focused = true;
            return true;
        }
        if matches!(code, KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space)
            && let Some(id) = self.recipe_book.last
        {
            let shown = self.recipe_book.shown.iter().find(|s| s.collection.recipes.contains(&id)).cloned();
            if let Some(shown) = shown {
                self.click_recipe(menu, &shown, id, self.modifiers.shift_key());
                return true;
            }
        }
        false
    }

    /// Draws the book, its button and the ghost recipe. Returns a tooltip.
    pub(super) fn recipe_book_ui(&self, ui: &mut Ui, now: Instant) -> Option<String> {
        let menu = self.book_menu()?;
        let screen = ui.size();
        let mouse = (self.cursor_px.0 / ui.scale, self.cursor_px.1 / ui.scale);
        let mut tip = None;
        let cycle = View::cycle(now, self.started);
        if let Some(r) = self.book_toggle_rect(screen)
            && !(self.book_open() && Self::book_narrow(screen.0))
        {
            book_button(ui, r, r.contains(mouse));
        }
        self.ghost_ui(ui, menu, cycle);
        if !self.book_open() {
            return None;
        }
        let l = self.book_layout(screen);
        let (xo, yo) = l.origin;
        panel(ui, Rect { x: xo, y: yo, w: rb::IMAGE_WIDTH, h: rb::IMAGE_HEIGHT });
        // Tabs: the selected one joins the book.
        for &(i, r) in &l.tabs {
            let selected = i == self.recipe_book.tab;
            let r = if selected { Rect { x: r.x - 2.0, w: r.w + 2.0, ..r } } else { r };
            ui.rect(r.x, r.y, r.w - 5.0, r.h, if selected { [0.78, 0.78, 0.78, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
            ui.rect(r.x, r.y, r.w - 5.0, 1.0, WHITE);
            ui.rect(r.x, r.y, 1.0, r.h, WHITE);
            ui.rect(r.x, r.y + r.h - 1.0, r.w - 5.0, 1.0, [0.33, 0.33, 0.33, 1.0]);
            let icons = menu.book.tabs()[i].icons;
            for (k, icon) in icons.iter().enumerate() {
                if let Some(item) = rb::java_item(icon) {
                    let dx = if icons.len() > 1 { if k == 0 { 3.0 } else { 14.0 } } else { 9.0 };
                    self.stack_ui(ui, r.x + dx - 1.0, r.y + 5.0, Stack::new(item, 1));
                }
            }
        }
        // Search box with its hint.
        let s = l.search;
        ui.rect(
            s.x - 1.0,
            s.y - 1.0,
            s.w + 2.0,
            s.h + 2.0,
            if self.recipe_book.search_focused { WHITE } else { [0.63, 0.63, 0.63, 1.0] },
        );
        ui.rect(s.x, s.y, s.w, s.h, [0.0, 0.0, 0.0, 1.0]);
        magnifier(ui, l.magnifier);
        if self.recipe_book.query.is_empty() && !self.recipe_book.search_focused {
            ui.text_clipped(s.x + 4.0, s.y + 3.0, rb::SEARCH_HINT, [0.5, 0.5, 0.5, 1.0], (s.x, s.x + s.w - 2.0));
        } else {
            let caret = if self.recipe_book.search_focused && ((now - self.started).as_millis() / 300).is_multiple_of(2)
            {
                "_"
            } else {
                ""
            };
            let text = format!("{}{caret}", self.recipe_book.query);
            ui.text_clipped(s.x + 4.0, s.y + 3.0, &text, WHITE, (s.x, s.x + s.w - 2.0));
        }
        // The craftable filter.
        let filtering = self.filtering(menu);
        let f = l.filter;
        let hover = f.contains(mouse);
        ui.rect(f.x, f.y, f.w, f.h, if hover { [0.85, 0.85, 0.85, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
        ui.rect(
            f.x + 1.0,
            f.y + 1.0,
            f.w - 2.0,
            f.h - 2.0,
            if filtering { [0.33, 0.6, 0.25, 1.0] } else { [0.35, 0.35, 0.35, 1.0] },
        );
        for k in 0..3 {
            let w = 14.0 - 4.0 * k as f32;
            ui.rect(f.x + (f.w - w) / 2.0, f.y + 4.0 + 3.0 * k as f32, w, 2.0, WHITE);
        }
        if hover {
            tip = Some(if filtering { menu.book.filter_label() } else { rb::SHOWING_ALL }.to_string());
        }
        // The recipe buttons.
        for (i, r) in l.buttons.iter().enumerate() {
            let Some(shown) = self.page_entry(i) else { continue };
            let entries = shown.entries(filtering);
            if entries.is_empty() {
                continue;
            }
            let craftable = !shown.craftable.is_empty();
            let many = entries.len() > 1;
            let pulse = self.recipe_book.pulses.iter().any(|(id, _)| entries.contains(id));
            let base: Color = if craftable { [0.55, 0.55, 0.55, 1.0] } else { [0.62, 0.27, 0.27, 1.0] };
            if many {
                ui.rect(r.x + 3.0, r.y + 3.0, r.w - 3.0, r.h - 3.0, [0.3, 0.3, 0.3, 1.0]);
            }
            let inset = if pulse { 0.0 } else { 1.0 };
            ui.rect(r.x + inset, r.y + inset, r.w - 3.0 - inset, r.h - 3.0 - inset, base);
            ui.rect(r.x + inset, r.y + inset, r.w - 3.0 - inset, 1.0, WHITE);
            let id = entries[cycle % entries.len()];
            let recipe = id.recipe().unwrap();
            if many && entries.iter().all(|e| e.recipe().unwrap().result.item == recipe.result.item) {
                self.stack_ui(ui, r.x + 4.0, r.y + 4.0, recipe.result);
            }
            self.stack_ui(ui, r.x + 3.0, r.y + 3.0, recipe.result);
            if r.contains(mouse) && self.recipe_book.overlay.is_none() {
                ui.rect(r.x + 1.0, r.y + 1.0, r.w - 4.0, r.h - 4.0, [1.0, 1.0, 1.0, 0.25]);
                let mut text = recipe.result.display_name().to_string();
                if many {
                    text = format!("{text}\n{}", rb::MORE_RECIPES);
                }
                tip = Some(text);
            }
        }
        // Paging.
        let pages = self.page_count();
        if pages > 1 {
            let text = format!("{}/{}", self.recipe_book.page + 1, pages);
            ui.text(xo - (Ui::text_width(&text) / 2.0).floor() + 73.0, yo + 141.0, &text, WHITE);
            if self.recipe_book.page + 1 < pages {
                arrow(ui, l.forward, true, l.forward.contains(mouse));
                if l.forward.contains(mouse) {
                    tip = Some(rb::NEXT_PAGE.to_string());
                }
            }
            if self.recipe_book.page > 0 {
                arrow(ui, l.back, false, l.back.contains(mouse));
                if l.back.contains(mouse) {
                    tip = Some(rb::PREVIOUS_PAGE.to_string());
                }
            }
        }
        // The alternatives of a grouped button.
        if let Some((index, x, y)) = self.recipe_book.overlay
            && let Some(shown) = self.recipe_book.shown.get(index)
        {
            let entries = shown.entries(filtering);
            let n = entries.len();
            let first = overlay_rect(x, y, n, 0);
            let cols = n.min(4) as f32;
            let rows = n.div_ceil(4) as f32;
            panel(
                ui,
                Rect {
                    x: first.x - 4.0,
                    y: first.y - 4.0,
                    w: cols * rb::BUTTON_SIZE + 8.0,
                    h: rows * rb::BUTTON_SIZE + 8.0,
                },
            );
            for (k, id) in entries.iter().enumerate() {
                let r = overlay_rect(x, y, n, k);
                let craftable = shown.craftable.contains(id);
                let base: Color = if craftable { [0.55, 0.55, 0.55, 1.0] } else { [0.62, 0.27, 0.27, 1.0] };
                ui.rect(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, base);
                let recipe = id.recipe().unwrap();
                self.stack_ui(ui, r.x + 4.0, r.y + 4.0, recipe.result);
                if r.contains(mouse) {
                    ui.rect(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, [1.0, 1.0, 1.0, 0.25]);
                    tip = Some(recipe.result.display_name().to_string());
                }
            }
        }
        tip
    }

    /// The ghost recipe in the grid or furnace (`GhostSlots`): red-backed,
    /// faded items cycling through each ingredient's alternatives.
    fn ghost_ui(&self, ui: &mut Ui, menu: BookMenu, cycle: usize) {
        let Some(recipe) = self.recipe_book.ghost.and_then(RecipeId::recipe) else { return };
        let slots = self.inventory_slots(ui.size());
        let at = |slot: SlotRef| slots.iter().find(|(r, _, _)| *r == slot).map(|&(_, x, y)| (x + 1.0, y + 1.0));
        let mut ghosts: Vec<((f32, f32), Stack, bool)> = Vec::new();
        match menu.grid {
            Some(n) => {
                let (w, h) = if recipe.kind == rb::Kind::Shaped { (recipe.width, recipe.height) } else { (n, n) };
                rb::place::arrange(n, n, w, h, &recipe.slots, |ingredient, cell| {
                    if let (Some(i), Some(p)) = (ingredient, at(SlotRef::Craft(cell))) {
                        let options = &recipe.ingredients[i];
                        ghosts.push((p, Stack::new(options[cycle % options.len()], 1), false));
                    }
                });
                if let Some(p) = at(SlotRef::CraftResult) {
                    ghosts.push((p, recipe.result, true));
                }
            }
            None => {
                let options = &recipe.ingredients[0];
                if let Some(p) = at(SlotRef::FurnaceInput) {
                    ghosts.push((p, Stack::new(options[cycle % options.len()], 1), false));
                }
                let fuel_empty = match self.body.container {
                    Container::Furnace(pos) => self.world.furnace(pos).is_none_or(|f| f.fuel.is_none()),
                    _ => false,
                };
                if fuel_empty
                    && let Some(p) = at(SlotRef::FurnaceFuel)
                    && !fuels().is_empty()
                {
                    ghosts.push((p, Stack::new(fuels()[cycle % fuels().len()], 1), false));
                }
                if let Some(p) = at(SlotRef::FurnaceOutput) {
                    ghosts.push((p, recipe.result, true));
                }
            }
        }
        for ((x, y), stack, result) in ghosts {
            ui.rect(x, y, 16.0, 16.0, [1.0, 0.0, 0.0, 0.19]);
            let shown = if result { stack } else { Stack { count: 1, ..stack } };
            self.stack_ui(ui, x - 1.0, y - 1.0, shown);
            ui.rect(x, y, 16.0, 16.0, [1.0, 1.0, 1.0, 0.19]);
        }
    }

    /// The ghost item under the mouse, for its tooltip.
    pub(super) fn ghost_tooltip(&self, slot: Option<SlotRef>) -> Option<String> {
        let recipe = self.recipe_book.ghost.and_then(RecipeId::recipe)?;
        let menu = self.book_menu()?;
        let cycle = View::cycle(Instant::now(), self.started);
        let slot = slot?;
        match (slot, menu.grid) {
            (SlotRef::CraftResult | SlotRef::FurnaceOutput, _) => Some(recipe.result.display_name().to_string()),
            (SlotRef::Craft(cell), Some(n)) if self.craft.cells[cell].is_none() => {
                let (w, h) = if recipe.kind == rb::Kind::Shaped { (recipe.width, recipe.height) } else { (n, n) };
                let mut found = None;
                rb::place::arrange(n, n, w, h, &recipe.slots, |ingredient, c| {
                    if c == cell
                        && let Some(i) = ingredient
                    {
                        let options = &recipe.ingredients[i];
                        found = Some(Stack::new(options[cycle % options.len()], 1).display_name().to_string());
                    }
                });
                found
            }
            (SlotRef::FurnaceInput, None) => {
                let options = &recipe.ingredients[0];
                Some(Stack::new(options[cycle % options.len()], 1).display_name().to_string())
            }
            _ => None,
        }
    }

    /// `RecipeToast`: top right, the newest unlocks cycling, for five
    /// seconds after the last change.
    pub(super) fn recipe_toast_ui(&self, ui: &mut Ui, now: Instant) {
        let Some(changed) = self.recipe_book.toast_changed else { return };
        let entries = &self.recipe_book.toasts;
        if entries.is_empty() {
            return;
        }
        let age = (now - changed).as_millis() as f32;
        let show = rb::TOAST_MILLIS as f32;
        // Slide in and out over 600 ms (`ToastManager`'s animation).
        let slide = (age / 600.0).min(1.0).min(((show + 600.0 - age) / 600.0).max(0.0));
        if slide <= 0.0 {
            return;
        }
        let (sw, _) = ui.size();
        let (w, h) = (160.0, 32.0);
        let x = sw - w * slide;
        panel(ui, Rect { x, y: 0.0, w, h });
        ui.text_flat(x + 30.0, 7.0, rb::TOAST_TITLE, [0.31, 0.0, 0.31, 1.0]);
        ui.text_flat(x + 30.0, 18.0, rb::TOAST_DESCRIPTION, [0.0, 0.0, 0.0, 1.0]);
        let per = (show / entries.len() as f32).max(1.0);
        let entry = entries[((age / per) as usize) % entries.len()];
        self.stack_ui(ui, x + 7.0, 7.0, Stack { count: 1, ..entry.result });
        // The workstation, small in the corner (`RecipeToast`'s 0.6 scale).
        match (entry.station.icon_layer(), entry.station.block()) {
            (Some(layer), _) => ui.icon(x + 3.0, 3.0, 9.6, layer, WHITE),
            (None, Some(block)) => ui.block_icon(x + 3.0, 3.0, 9.6, block),
            _ => {}
        }
    }
}

/// Where alternative `i` of `n` sits in the overlay over a button at (x, y)
/// (four to a row, centred over the button).
fn overlay_rect(x: f32, y: f32, n: usize, i: usize) -> Rect {
    let cols = n.min(4) as f32;
    let rows = n.div_ceil(4) as f32;
    let left = x + rb::BUTTON_SIZE / 2.0 - cols * rb::BUTTON_SIZE / 2.0;
    let top = y + rb::BUTTON_SIZE / 2.0 - rows * rb::BUTTON_SIZE / 2.0;
    Rect {
        x: left + (i % 4) as f32 * rb::BUTTON_SIZE,
        y: top + (i / 4) as f32 * rb::BUTTON_SIZE,
        w: rb::BUTTON_SIZE,
        h: rb::BUTTON_SIZE,
    }
}

/// A raised grey panel like the container screens'.
fn panel(ui: &mut Ui, r: Rect) {
    ui.rect(r.x, r.y, r.w, r.h, [0.78, 0.78, 0.78, 1.0]);
    ui.rect(r.x, r.y, r.w, 1.0, WHITE);
    ui.rect(r.x, r.y, 1.0, r.h, WHITE);
    ui.rect(r.x, r.y + r.h - 1.0, r.w, 1.0, [0.33, 0.33, 0.33, 1.0]);
    ui.rect(r.x + r.w - 1.0, r.y, 1.0, r.h, [0.33, 0.33, 0.33, 1.0]);
}

/// The survival inventory's book button, from its 2x2 grid's first slot
/// frame: Java's (104, 61) against the grid's first item at (98, 18), the
/// frame being a pixel up and left of the item (v0.18 Track 4's layout).
fn inventory_toggle((gx, gy): (f32, f32)) -> (f32, f32) {
    (gx + 1.0 + (104.0 - 98.0), gy + 1.0 + (61.0 - 18.0))
}

/// The recipe book button: a green book with pages.
fn book_button(ui: &mut Ui, r: Rect, hover: bool) {
    ui.rect(r.x, r.y, r.w, r.h, if hover { [0.95, 0.95, 0.95, 1.0] } else { [0.55, 0.55, 0.55, 1.0] });
    ui.rect(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, [0.42, 0.42, 0.42, 1.0]);
    let (bx, by) = (r.x + 5.0, r.y + 3.0);
    ui.rect(bx, by, 10.0, 12.0, [0.18, 0.45, 0.2, 1.0]);
    ui.rect(bx + 2.0, by + 1.0, 7.0, 10.0, [0.93, 0.9, 0.8, 1.0]);
    for k in 0..3 {
        ui.rect(bx + 3.0, by + 3.0 + 2.0 * k as f32, 5.0, 1.0, [0.45, 0.4, 0.35, 1.0]);
    }
}

/// The search box's magnifying glass.
fn magnifier(ui: &mut Ui, r: Rect) {
    let (x, y) = (r.x + (r.w - 9.0) / 2.0, r.y + 2.0);
    let c = [0.25, 0.25, 0.25, 1.0];
    ui.rect(x + 1.0, y, 4.0, 1.0, c);
    ui.rect(x + 1.0, y + 5.0, 4.0, 1.0, c);
    ui.rect(x, y + 1.0, 1.0, 4.0, c);
    ui.rect(x + 5.0, y + 1.0, 1.0, 4.0, c);
    for k in 0..3 {
        ui.rect(x + 5.0 + k as f32, y + 5.0 + k as f32, 2.0, 2.0, c);
    }
}

/// A page arrow pointing right (`forward`) or left.
fn arrow(ui: &mut Ui, r: Rect, forward: bool, hover: bool) {
    let c = if hover { WHITE } else { [0.82, 0.82, 0.82, 1.0] };
    for k in 0..6 {
        let h = 15.0 - 2.5 * k as f32;
        let x = if forward { r.x + 2.0 + k as f32 } else { r.x + r.w - 3.0 - k as f32 };
        ui.rect(x, r.y + (r.h - h) / 2.0, 1.0, h, c);
    }
}

/// The book names LAN commands use.
pub(super) fn book_name(book: BookType) -> &'static str {
    match book {
        BookType::Crafting => "crafting",
        BookType::Furnace => "furnace",
        BookType::BlastFurnace => "blast_furnace",
        BookType::Smoker => "smoker",
    }
}

pub(super) fn book_from_name(name: &str) -> Option<BookType> {
    BookType::ALL.into_iter().find(|&b| book_name(b) == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v0.18 Track 4 moved the survival inventory to Java's slots: the
    /// button lands at Java's (104, 61) and clears every survival slot frame
    /// (armour, off-hand, grid, result, main grid).
    #[test]
    fn the_inventory_book_button_sits_where_java_puts_it() {
        let (x, y) = inventory_toggle((97.0, 17.0));
        assert_eq!((x, y), (104.0, 61.0));
        let button = Rect { x, y, w: 20.0, h: 18.0 };
        let mut frames: Vec<(f32, f32)> = (0..4).map(|i| (7.0, 7.0 + 18.0 * i as f32)).collect();
        frames.extend([(76.0, 61.0), (97.0, 17.0), (115.0, 17.0), (97.0, 35.0), (115.0, 35.0), (153.0, 27.0)]);
        frames.extend((0..9).map(|c| (7.0 + 18.0 * c as f32, 83.0)));
        for (fx, fy) in frames {
            let overlaps =
                fx < button.x + button.w && fx + 18.0 > button.x && fy < button.y + button.h && fy + 18.0 > button.y;
            assert!(!overlaps, "the button covers the slot at ({fx}, {fy})");
        }
    }

    #[test]
    fn overlay_centres_alternatives_on_the_button() {
        let one = overlay_rect(100.0, 50.0, 1, 0);
        assert_eq!((one.x, one.y), (100.0, 50.0));
        let first = overlay_rect(100.0, 50.0, 6, 0);
        assert_eq!(first.x, 100.0 + 12.5 - 50.0);
        assert_eq!(overlay_rect(100.0, 50.0, 6, 5).y, first.y + rb::BUTTON_SIZE);
    }

    #[test]
    fn book_names_round_trip() {
        for b in BookType::ALL {
            assert_eq!(book_from_name(book_name(b)), Some(b));
        }
    }
}
