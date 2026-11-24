//! v0.18 Track 4: Java 26.3's creative inventory screen
//! (`CreativeModeInventoryScreen`), drawn in VoxelCraft's own panel style.
//! Positions, sizes and scrolling are Java's, from
//! `tests/fixtures/java/26.3/creative_layout.json`: a 195 x 136 panel
//! centred on the screen, the 9 x 5 grid at (9, 18) and the hotbar at
//! (9, 112), the scrollbar at (175, 18), seven 26 x 32 tabs above and seven
//! below (the last two of each row aligned right), the search box at
//! (82, 6) on the search tab, and the Survival Inventory tab's slots,
//! Destroy Item slot and player preview. The tabs and their stacks are
//! `voxelcraft::creative`'s.
//!
//! Differences from Java: tabs and slots are flat rectangles rather than
//! Java's sprites; a tab is chosen when the button goes down (Java waits
//! for the release); searching matches every word of a query against
//! stack names (Java's search trees also match registry ids and tags).

use voxelcraft::creative::{self as cv, Kind, Row, scroll};

use super::Game;
use super::hud::{SLOT, SlotRef};
use crate::inventory::Stack;
use crate::render::ui::{Color, Ui, WHITE};

/// The panel (`imageWidth` x `imageHeight`).
pub(super) const W: f32 = 195.0;
pub(super) const H: f32 = 136.0;
const TITLE: Color = [0.25, 0.25, 0.25, 1.0];

/// Where the creative screen stands: the tab Java remembers between
/// openings (`selectedTab` is static), the scroll fraction and whether the
/// scrollbar is held.
#[derive(Debug, Default)]
pub(super) struct State {
    tab: Option<usize>,
    pub scroll: f32,
    pub dragging: bool,
    /// The last search and its hits, so drawing and hit tests don't filter
    /// every stack again each frame.
    hits: std::cell::RefCell<Option<(String, std::rc::Rc<Vec<u16>>)>>,
}

/// What the grid scrolls through: a tab's stacks, the search hits, or the
/// saved hotbars' 81 cells.
pub(super) enum Cells {
    Items(&'static [Stack]),
    Hits(std::rc::Rc<Vec<u16>>),
    Hotbar(Vec<Option<Stack>>),
}

impl Cells {
    pub fn len(&self) -> usize {
        match self {
            Cells::Items(s) => s.len(),
            Cells::Hits(h) => h.len(),
            Cells::Hotbar(c) => c.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, i: usize) -> Option<Stack> {
        match self {
            Cells::Items(s) => s.get(i).copied(),
            Cells::Hits(h) => h.get(i).and_then(|&j| cv::catalogue().search.get(j as usize)).copied(),
            Cells::Hotbar(c) => c.get(i).copied().flatten(),
        }
    }
}

impl State {
    /// The selected tab (Building Blocks until one is picked).
    pub fn tab(&self) -> usize {
        self.tab.unwrap_or_else(|| cv::catalogue().default_tab())
    }

    pub fn kind(&self) -> Kind {
        cv::catalogue().tabs.get(self.tab()).map_or(Kind::Category, |t| t.kind)
    }
}

/// A tab's left edge in the panel (`getTabX`).
fn tab_x(t: &cv::Tab) -> f32 {
    let column = f32::from(t.column);
    if t.aligned_right { W - 27.0 * (7.0 - column) + 1.0 } else { 27.0 * column }
}

/// A tab's top for clicks (`getTabY`) and as drawn (`extractTabButton`).
fn tab_y(t: &cv::Tab, drawn: bool) -> f32 {
    match (t.row, drawn) {
        (Row::Top, false) => -32.0,
        (Row::Top, true) => -28.0,
        (Row::Bottom, false) => H,
        (Row::Bottom, true) => H - 4.0,
    }
}

/// The 45 cells of a list scrolled to `offs`: the stack in each, if any.
fn visible(list: &Cells, offs: f32) -> [Option<Stack>; 45] {
    std::array::from_fn(|cell| scroll::at(list.len(), offs, cell).and_then(|i| list.get(i)))
}

impl Game {
    /// The panel's top-left corner (`AbstractContainerScreen.init`).
    pub(super) fn creative_origin(&self, screen: (f32, f32)) -> (f32, f32) {
        (((screen.0 - W) / 2.0).floor(), ((screen.1 - H) / 2.0).floor())
    }

    /// Every cell of the selected tab's list, for the grid: category tabs
    /// list their stacks, the search tab its matches, Saved Hotbars its
    /// nine rows (an empty row shows nothing here; its placeholder is drawn).
    pub(super) fn creative_list(&self) -> Cells {
        let c = cv::catalogue();
        match self.creative.kind() {
            Kind::Category => Cells::Items(c.items(self.creative.tab())),
            Kind::Search => {
                let query = self.search.query.trim();
                if query.is_empty() {
                    return Cells::Items(&c.search);
                }
                let mut cache = self.creative.hits.borrow_mut();
                if let Some((q, hits)) = cache.as_ref()
                    && q == query
                {
                    return Cells::Hits(hits.clone());
                }
                let hits = std::rc::Rc::new(c.search_hits(query));
                *cache = Some((query.to_string(), hits.clone()));
                Cells::Hits(hits)
            }
            Kind::Hotbar => Cells::Hotbar(self.hotbars.cells()),
            Kind::Inventory => Cells::Items(&[]),
        }
    }

    /// Slot positions (frame corners) on the creative screen.
    pub(super) fn creative_slots(&self, screen: (f32, f32)) -> Vec<(SlotRef, f32, f32)> {
        let (x0, y0) = self.creative_origin(screen);
        // Java gives each slot's item corner; frames are a pixel up and left.
        let at = |x: f32, y: f32| (x0 + x - 1.0, y0 + y - 1.0);
        let mut out = Vec::with_capacity(54);
        if self.creative.kind() == Kind::Inventory {
            // `selectTab` with `Type.INVENTORY`: armor in two columns, the
            // off-hand, the main grid and hotbar, and Destroy Item.
            for (i, piece) in crate::item::ArmorPiece::ALL.into_iter().enumerate() {
                let (x, y) = at(54.0 + (i / 2) as f32 * 54.0, 6.0 + (i % 2) as f32 * 27.0);
                out.push((SlotRef::Armor(piece), x, y));
            }
            let (x, y) = at(35.0, 20.0);
            out.push((SlotRef::Offhand, x, y));
            for i in 0..27 {
                let (x, y) = at(9.0 + (i % 9) as f32 * SLOT, 54.0 + (i / 9) as f32 * SLOT);
                out.push((SlotRef::Inventory(9 + i), x, y));
            }
            let (x, y) = at(173.0, 112.0);
            out.push((SlotRef::Destroy, x, y));
        } else {
            let list = self.creative_list();
            let hotbar_tab = self.creative.kind() == Kind::Hotbar;
            for (cell, stack) in visible(&list, self.creative.scroll).into_iter().enumerate() {
                let (x, y) = at(9.0 + (cell % 9) as f32 * SLOT, 18.0 + (cell / 9) as f32 * SLOT);
                let slot = match stack {
                    Some(s) => SlotRef::Palette(s),
                    // An empty saved hotbar's placeholder is locked; other
                    // cells of an empty row stay clickable.
                    None if hotbar_tab && self.hotbar_placeholder(cell).is_some() => continue,
                    None => SlotRef::PaletteEmpty,
                };
                out.push((slot, x, y));
            }
        }
        for i in 0..9 {
            let (x, y) = at(9.0 + i as f32 * SLOT, 112.0);
            out.push((SlotRef::Inventory(i), x, y));
        }
        out
    }

    /// On the Saved Hotbars tab, the hotbar whose empty-row placeholder is
    /// in visible cell `cell` (`selectTab` puts a paper in row n's column n).
    fn hotbar_placeholder(&self, cell: usize) -> Option<usize> {
        let index = scroll::at(81, self.creative.scroll, cell)?;
        let (row, col) = (index / 9, index % 9);
        (row == col && self.hotbars.is_empty(row)).then_some(row)
    }

    /// The slot under the pointer (`mouse` in GUI pixels).
    pub(super) fn creative_slot_at(&self, screen: (f32, f32), mouse: (f32, f32)) -> Option<SlotRef> {
        self.creative_slots(screen)
            .into_iter()
            .find(|&(_, x, y)| mouse.0 >= x && mouse.0 < x + SLOT && mouse.1 >= y && mouse.1 < y + SLOT)
            .map(|(r, _, _)| r)
    }

    /// The tab under the pointer (`checkTabClicked`), as a catalogue index.
    pub(super) fn creative_tab_at(&self, screen: (f32, f32), mouse: (f32, f32)) -> Option<usize> {
        let (x0, y0) = self.creative_origin(screen);
        let (mx, my) = (mouse.0 - x0, mouse.1 - y0);
        let c = cv::catalogue();
        c.shown().find(|&i| {
            let t = &c.tabs[i];
            let (x, y) = (tab_x(t), tab_y(t, false));
            mx >= x && mx <= x + 26.0 && my >= y && my <= y + 32.0
        })
    }

    fn creative_mouse(&self) -> ((f32, f32), (f32, f32)) {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        ((w as f32 / scale, h as f32 / scale), (self.cursor_px.0 / scale, self.cursor_px.1 / scale))
    }

    /// `hasClickedOutside`: off the panel and off the selected tab.
    pub(super) fn creative_off_panel(&self) -> bool {
        let (screen, (mx, my)) = self.creative_mouse();
        let (x0, y0) = self.creative_origin(screen);
        let outside = mx < x0 || my < y0 || mx >= x0 + W || my >= y0 + H;
        outside && self.creative_tab_at(screen, (mx, my)) != Some(self.creative.tab())
    }

    /// `insideScrollbar`: the bar, on a tab that scrolls.
    fn in_scrollbar(&self) -> bool {
        let (screen, (mx, my)) = self.creative_mouse();
        let (x0, y0) = self.creative_origin(screen);
        let scrolls = cv::catalogue().tabs.get(self.creative.tab()).is_some_and(|t| t.can_scroll);
        scrolls && mx >= x0 + 175.0 && my >= y0 + 18.0 && mx < x0 + 189.0 && my < y0 + 130.0
    }

    fn creative_can_scroll(&self) -> bool {
        let scrolls = cv::catalogue().tabs.get(self.creative.tab()).is_some_and(|t| t.can_scroll);
        scrolls && scroll::can_scroll(self.creative_list().len())
    }

    /// `selectTab`: a new tab clears the search unless it is the search
    /// tab again, and scrolls to the top.
    pub(super) fn creative_select(&mut self, tab: usize) {
        let was = self.creative.tab();
        self.creative.tab = Some(tab);
        if self.creative.kind() == Kind::Search {
            self.search.focused = true;
            if was != tab {
                self.search.query.clear();
            }
        } else {
            self.search.focused = false;
            self.search.query.clear();
        }
        self.search.selected = false;
        self.creative.scroll = 0.0;
        self.creative.dragging = false;
    }

    /// A press on the screen's own controls (tabs and the scrollbar).
    /// Returns whether it took the click.
    pub(super) fn creative_press(&mut self, right: bool) -> bool {
        if right {
            return false;
        }
        let (screen, mouse) = self.creative_mouse();
        if let Some(tab) = self.creative_tab_at(screen, mouse) {
            if tab != self.creative.tab() {
                self.creative_select(tab);
                self.audio.ui_click();
            }
            return true;
        }
        if self.creative.kind() != Kind::Inventory && self.in_scrollbar() {
            self.creative.dragging = self.creative_can_scroll();
            self.creative_drag();
            return true;
        }
        false
    }

    /// `mouseDragged` while the scrollbar is held.
    pub(super) fn creative_drag(&mut self) {
        if !self.creative.dragging || !self.inventory_open || !self.shows_palette() {
            return;
        }
        let (screen, (_, my)) = self.creative_mouse();
        let (_, y0) = self.creative_origin(screen);
        self.creative.scroll = scroll::drag(my - y0 - 18.0);
    }

    /// `mouseScrolled`: `lines` of the wheel, up positive.
    pub(super) fn creative_wheel(&mut self, lines: f64) {
        if self.creative_can_scroll() {
            self.creative.scroll = scroll::wheel(self.creative_list().len(), self.creative.scroll, lines);
        }
    }

    /// The chat key on a tab other than search opens the search tab
    /// (`keyPressed`), swallowing that key's text.
    pub(super) fn creative_chat_key(&mut self) -> bool {
        if !self.inventory_open || !self.shows_palette() || self.creative.kind() == Kind::Search {
            return false;
        }
        // winit delivers the key's text with the press, which `search_key`
        // saw before the tab changed, so nothing needs ignoring.
        if let Some(tab) = cv::catalogue().index("search") {
            self.creative_select(tab);
        }
        true
    }

    /// `handleHotbarLoadOrSave`: the save activator with a hotbar key
    /// stores the hotbar in row `row`; the load activator copies it back.
    pub(super) fn hotbar_load_or_save(&mut self, row: usize, load: bool) {
        if load {
            self.hotbars.load(row, &mut self.session.inventory);
            return;
        }
        self.hotbars.store(row, &self.session.inventory);
        let activator = self.settings.keys.get(super::keys::Action::LoadHotbar).display();
        let key = self.settings.keys.get(super::keys::Action::hotbar_key(row)).display();
        self.show_popup(&format!("Item hotbar saved (restore with {activator} + {key})"));
        self.save_hotbars();
    }

    /// `HotbarManager.save`: a failure is logged and play goes on.
    pub(super) fn save_hotbars(&self) {
        if let Some(path) = &self.hotbars_path
            && let Err(e) = std::fs::write(path, self.hotbars.serialize())
        {
            log::warn!("couldn't save hotbars to {}: {e}", path.display());
        }
    }

    /// Draws the creative screen.
    pub(super) fn creative_ui(&self, ui: &mut Ui) {
        let (sw, sh) = ui.size();
        let (x0, y0) = self.creative_origin((sw, sh));
        let mouse = (self.cursor_px.0 / ui.scale, self.cursor_px.1 / ui.scale);
        let c = cv::catalogue();
        let selected = self.creative.tab();
        // `EffectsInInventory`: active effects to the panel's left.
        super::hud::effect_list_ui(ui, &self.session.vitals.effects, x0, y0);
        // Unselected tabs behind the panel, then the panel, then the selected tab.
        for i in c.shown().filter(|&i| i != selected) {
            self.creative_tab_ui(ui, (x0, y0), i, false);
        }
        ui.rect(x0, y0, W, H, [0.78, 0.78, 0.78, 1.0]);
        ui.rect(x0, y0, W, 1.0, WHITE);
        ui.rect(x0, y0, 1.0, H, WHITE);
        ui.rect(x0, y0 + H - 1.0, W, 1.0, [0.33, 0.33, 0.33, 1.0]);
        ui.rect(x0 + W - 1.0, y0, 1.0, H, [0.33, 0.33, 0.33, 1.0]);
        self.creative_tab_ui(ui, (x0, y0), selected, true);
        let tab = &c.tabs[selected];
        // `extractLabels`: the title at (8, 6) unless the tab hides it.
        if tab.show_title {
            ui.text_flat(x0 + 8.0, y0 + 6.0, &tab.title, TITLE);
        }
        match tab.kind {
            Kind::Search => {
                // The search box: 80 x 9 at (82, 6), no border, white text.
                let (bx, by) = (x0 + 82.0, y0 + 6.0);
                ui.rect(bx - 1.0, by - 1.0, 89.0, 11.0, [0.12, 0.12, 0.12, 1.0]);
                let text = format!("{}{}", self.search.query, if self.search.focused { "_" } else { "" });
                ui.text_clipped(bx, by, &text, WHITE, (bx, bx + 87.0));
                if self.creative_list().is_empty() {
                    ui.text_flat(x0 + 9.0, y0 + 22.0, "No matching items", [0.3, 0.3, 0.3, 1.0]);
                }
            }
            Kind::Inventory => {
                // `InventoryScreen.extractEntityInInventoryFollowsMouse`.
                ui.rect(x0 + 72.0, y0 + 5.0, 34.0, 45.0, [0.0, 0.0, 0.0, 1.0]);
                super::preview::player(ui, (x0 + 73.0, y0 + 6.0, x0 + 105.0, y0 + 49.0), 20.0, 0.0625, mouse);
            }
            _ => {}
        }
        if tab.can_scroll && tab.kind != Kind::Inventory {
            // The scrollbar's well and its 12 x 15 thumb (`(112 - 17) * scrollOffs`).
            ui.rect(x0 + 174.0, y0 + 17.0, 14.0, 112.0, [0.33, 0.33, 0.33, 1.0]);
            ui.rect(x0 + 175.0, y0 + 18.0, 13.0, 111.0, [0.55, 0.55, 0.55, 1.0]);
            let enabled = self.creative_can_scroll();
            let top = y0 + 18.0 + scroll::thumb(self.creative.scroll);
            ui.rect(x0 + 175.0, top, 12.0, 15.0, if enabled { WHITE } else { [0.68, 0.68, 0.68, 1.0] });
            ui.rect(
                x0 + 176.0,
                top + 1.0,
                10.0,
                13.0,
                if enabled { [0.78, 0.78, 0.78, 1.0] } else { [0.6, 0.6, 0.6, 1.0] },
            );
        }
        if tab.kind == Kind::Hotbar {
            // Empty rows' placeholders: a paper, labelled on hover.
            for cell in 0..45 {
                if let Some(row) = self.hotbar_placeholder(cell) {
                    let (x, y) = (x0 + 8.0 + (cell % 9) as f32 * SLOT, y0 + 17.0 + (cell / 9) as f32 * SLOT);
                    ui.rect(x, y, SLOT, SLOT, [0.55, 0.55, 0.55, 1.0]);
                    ui.rect(x + 1.0, y + 1.0, SLOT - 1.0, SLOT - 1.0, [0.45, 0.45, 0.45, 1.0]);
                    if let Some(layer) = crate::item::Item::PAPER.icon_layer() {
                        ui.icon(x + 1.0, y + 1.0, 16.0, layer, [1.0, 1.0, 1.0, 0.6]);
                    }
                    if mouse.0 >= x && mouse.0 < x + SLOT && mouse.1 >= y && mouse.1 < y + SLOT {
                        let save = self.settings.keys.get(super::keys::Action::SaveHotbar).display();
                        let key = self.settings.keys.get(super::keys::Action::hotbar_key(row)).display();
                        self.tooltip(ui, &format!("Save hotbar with {save} + {key}"));
                    }
                }
            }
        }
        self.slots_ui(ui, x0, y0);
        // `checkTabHovering`: a tab's title over its inner 21 x 27.
        if self.session.inventory.cursor.is_none()
            && let Some(i) = self.creative_tab_at((sw, sh), mouse)
        {
            let t = &c.tabs[i];
            let (x, y) = (x0 + tab_x(t), y0 + tab_y(t, false));
            if mouse.0 >= x + 3.0 && mouse.0 < x + 24.0 && mouse.1 >= y + 3.0 && mouse.1 < y + 30.0 {
                self.tooltip(ui, &t.title);
            }
        }
    }

    /// One tab: a 26 x 32 tab joined to the panel when selected, with its
    /// icon centred and nudged a pixel toward the panel.
    fn creative_tab_ui(&self, ui: &mut Ui, (x0, y0): (f32, f32), i: usize, selected: bool) {
        let t = &cv::catalogue().tabs[i];
        let (x, y) = (x0 + tab_x(t), y0 + tab_y(t, true));
        let face = if selected { [0.78, 0.78, 0.78, 1.0] } else { [0.62, 0.62, 0.62, 1.0] };
        ui.rect(x, y, 26.0, 32.0, [0.2, 0.2, 0.2, 1.0]);
        ui.rect(x + 1.0, y + 1.0, 24.0, 30.0, face);
        ui.rect(x + 1.0, y + 1.0, 24.0, 1.0, WHITE);
        if selected {
            // Joined to the panel: the panel's edge under it is open.
            let join = if t.row == Row::Top { y0 } else { y0 + H - 1.0 };
            ui.rect(x + 1.0, join - 1.0, 24.0, 3.0, face);
        }
        let nudge = if t.row == Row::Top { 1.0 } else { -1.0 };
        self.stack_ui(ui, x + 13.0 - 8.0 - 1.0, y + 16.0 - 8.0 + nudge - 1.0, Stack::new(t.icon, 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> serde_json::Value {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/creative_layout.json")).unwrap()
    }

    #[test]
    fn the_panel_and_tabs_sit_where_java_puts_them() {
        let l = layout();
        let cr = &l["creative"];
        assert_eq!((cr["size"][0].as_f64().unwrap() as f32, cr["size"][1].as_f64().unwrap() as f32), (W, H));
        let tabs = &cr["tabs"];
        let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        let c = cv::catalogue();
        for t in c.tabs.iter() {
            let column = f32::from(t.column);
            // `getTabX`: 27 a column, or from the right edge.
            let x = if t.aligned_right {
                W - f(&tabs["spacing"]) * (f(&tabs["right_columns"]) - column) + f(&tabs["right_offset"])
            } else {
                f(&tabs["spacing"]) * column
            };
            assert_eq!(tab_x(t), x, "{}", t.id);
            let (hit, drawn) = match t.row {
                Row::Top => (f(&tabs["top_y"]), f(&tabs["drawn_top_y"])),
                Row::Bottom => (f(&tabs["bottom_y"]), f(&tabs["drawn_bottom_y"])),
            };
            assert_eq!((tab_y(t, false), tab_y(t, true)), (hit, drawn), "{}", t.id);
        }
        let at = |id: &str| tab_x(&c.tabs[c.index(id).unwrap()]);
        assert_eq!((at("building_blocks"), at("hotbar"), at("search"), at("inventory")), (0.0, 142.0, 169.0, 169.0));
        // Tabs don't overlap within a row.
        for row in [Row::Top, Row::Bottom] {
            let mut xs: Vec<f32> = c.shown().map(|i| &c.tabs[i]).filter(|t| t.row == row).map(tab_x).collect();
            xs.sort_by(f32::total_cmp);
            assert!(xs.windows(2).all(|w| w[1] - w[0] >= 26.0), "{xs:?}");
        }
    }

    #[test]
    fn the_inventory_tab_and_grid_use_javas_slot_positions() {
        let l = layout();
        let inv = &l["creative"]["inventory_tab"];
        let n = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
        // The values `creative_slots` draws with (Java's item corners).
        assert_eq!((n(&inv["armor"]["origin"][0]), n(&inv["armor"]["origin"][1])), (54.0, 6.0));
        assert_eq!((n(&inv["armor"]["column_pitch"]), n(&inv["armor"]["row_pitch"])), (54.0, 27.0));
        assert_eq!((n(&inv["offhand"][0]), n(&inv["offhand"][1])), (35.0, 20.0));
        assert_eq!((n(&inv["main"]["origin"][0]), n(&inv["main"]["origin"][1])), (9.0, 54.0));
        assert_eq!((n(&inv["destroy_slot"][0]), n(&inv["destroy_slot"][1])), (173.0, 112.0));
        assert_eq!(n(&inv["hotbar_y"]), 112.0);
        let p = &inv["preview"];
        assert_eq!(
            (n(&p["box"][0]), n(&p["box"][1]), n(&p["box"][2]), n(&p["box"][3]), n(&p["size"])),
            (73.0, 6.0, 105.0, 49.0, 20.0)
        );
        let grid = &l["creative"]["grid"];
        assert_eq!((n(&grid["origin"][0]), n(&grid["origin"][1]), n(&grid["pitch"])), (9.0, 18.0, SLOT));
        assert_eq!((n(&grid["hotbar"][0]), n(&grid["hotbar"][1])), (9.0, 112.0));
        let sb = &l["creative"]["scrollbar"];
        assert_eq!((n(&sb["at"][0]), n(&sb["at"][1]), n(&sb["size"][1])), (175.0, 18.0, 112.0));
        let s = &l["creative"]["search_box"];
        assert_eq!((n(&s["at"][0]), n(&s["at"][1]), n(&s["max_length"])), (82.0, 6.0, 50.0));
        // The survival inventory's preview and main grid.
        let sv = &l["survival"]["preview"];
        assert_eq!((n(&sv["box"][0]), n(&sv["box"][1]), n(&sv["box"][2]), n(&sv["box"][3])), (26.0, 8.0, 75.0, 78.0));
        assert_eq!(n(&sv["size"]), 30.0);
    }
}
