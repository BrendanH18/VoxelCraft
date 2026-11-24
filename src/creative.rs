//! v0.18 Track 4: Java 26.3's creative inventory, the parts that don't
//! draw: the tabs and their item order, the grid's scrolling, the creative
//! slot click rules and the saved hotbars. The screen itself is
//! `app::creative`.
//!
//! The tabs come straight from `tests/fixtures/java/26.3/creative_tabs.json`
//! (`tools/extract_java_creative_fixtures.py` builds every tab with Java's
//! own `CreativeModeTabs` code): each entry is resolved to a VoxelCraft
//! stack once, on first use. Items the game doesn't register are skipped;
//! stack variants it can't express (painting variants and goat horn
//! instruments) leave their plain item once; `tests/creative_inventory.rs`
//! lists every skip against `registry_gaps.json`.
//!
//! Saved hotbars (`HotbarManager`): nine rows of nine stacks per install,
//! kept by the desktop game in `saves/hotbars.txt` (Java's `hotbar.nbt`
//! sits beside `options.txt` too), one line per row of
//! [`crate::inventory::stack_to_string`] fields.

use std::sync::OnceLock;

use serde_json::Value;

use crate::inventory::Stack;
use crate::item::Item;

/// `CreativeModeTab.Row`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Top,
    Bottom,
}

/// `CreativeModeTab.Type`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Category,
    Hotbar,
    Search,
    Inventory,
}

/// One creative tab, as a player without operator permissions sees it.
#[derive(Debug)]
pub struct Tab {
    /// The registry id (`building_blocks`).
    pub id: String,
    pub title: String,
    pub row: Row,
    pub column: u8,
    pub kind: Kind,
    pub aligned_right: bool,
    pub show_title: bool,
    pub can_scroll: bool,
    pub icon: Item,
    /// What the tab shows, in Java's order.
    pub items: Vec<Stack>,
    /// What the tab adds to the search tab (Java's `TabVisibility`).
    pub search: Vec<Stack>,
}

impl Tab {
    /// `CreativeModeTab.shouldDisplay`: a category tab needs items.
    pub fn shown(&self) -> bool {
        self.kind != Kind::Category || !self.items.is_empty()
    }
}

/// Every tab, in registry order, with the search tab's own list.
#[derive(Debug)]
pub struct Catalogue {
    pub tabs: Vec<Tab>,
    /// The search tab's list: every other tab's search stacks in tab order,
    /// each stack once (`ItemStackLinkedSet.createTypeAndComponentsSet`).
    pub search: Vec<Stack>,
    /// Each search stack's lower-case name and enchantment lines, which
    /// [`Catalogue::search_hits`] matches against.
    pub search_text: Vec<String>,
    /// `(tab, java item id)` of every entry left out, each once per tab.
    pub skipped: Vec<(String, String)>,
}

impl Catalogue {
    /// The index of the tab with registry id `id`.
    pub fn index(&self, id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// The tab Java opens first (`CreativeModeTabs.getDefaultTab`).
    pub fn default_tab(&self) -> usize {
        self.index("building_blocks").unwrap_or(0)
    }

    /// The tabs a player sees (`CreativeModeTabs.tabs`), as indices.
    pub fn shown(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.tabs.len()).filter(|&i| self.tabs[i].shown())
    }

    /// Indices into [`Catalogue::search`] of the stacks matching `query`
    /// (every word in the name or enchantment lines).
    pub fn search_hits(&self, query: &str) -> Vec<u16> {
        let query = query.to_lowercase().replace('_', " ");
        let words: Vec<&str> = query.split_whitespace().collect();
        (0..self.search.len() as u16)
            .filter(|&i| words.iter().all(|w| self.search_text[i as usize].contains(w)))
            .collect()
    }

    /// What tab `i` lists: its items, the search results, or nothing.
    pub fn items(&self, i: usize) -> &[Stack] {
        match self.tabs.get(i) {
            Some(t) if t.kind == Kind::Search => &self.search,
            Some(t) => &t.items,
            None => &[],
        }
    }
}

/// The catalogue, built on first use.
pub fn catalogue() -> &'static Catalogue {
    static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
    CATALOGUE.get_or_init(|| build(include_str!("../tests/fixtures/java/26.3/creative_tabs.json")))
}

/// The stack a fixture entry stands for, or `None` when VoxelCraft has no
/// such item. Components it can't express leave the plain item.
pub fn resolve(entry: &Value) -> Option<Stack> {
    let id = entry["item"].as_str()?;
    let item = java_item(id)?;
    let c = &entry["components"];
    if let Some(id_of_potion) = c["potion_contents"]["potion"].as_str() {
        // A potion the game lacks is a registry gap, not a water bottle.
        let p = crate::potion::Potion::from_id(id_of_potion)?;
        return match id {
            "potion" => Some(Stack::new(Item::potion(p), 1)),
            "splash_potion" => Some(Stack::new(Item::splash_potion(p), 1)),
            "lingering_potion" => Some(Stack::lingering_potion(p)),
            "tipped_arrow" => Some(Stack::tipped_arrow(p, 1)),
            _ => Some(Stack::new(item, 1)),
        };
    }
    if let Some(books) = c["stored_enchantments"].as_object() {
        let mut stack = Stack::new(item, 1);
        for (name, level) in books {
            let e = crate::enchant::Enchantment::from_name(name)?;
            stack.enchants.set(e, level.as_u64()? as u8);
        }
        return Some(stack);
    }
    if let Some(flight) = c["fireworks"]["flight_duration"].as_u64() {
        let fireworks = crate::content::fireworks::Fireworks { flight: flight as u8, explosions: Vec::new() };
        return Some(crate::content::fireworks::rocket(fireworks, 1));
    }
    if let Some(amplifier) = c["ominous_bottle_amplifier"].as_u64() {
        return Some(Stack::new(crate::world::trial::ominous_bottle(amplifier as u8), 1));
    }
    if let Some(effects) = c["suspicious_stew_effects"].as_array() {
        let [effect] = effects.as_slice() else { return Some(Stack::new(item, 1)) };
        let code = crate::entity::mooshroom::stew_code(effect["id"].as_str()?, effect["duration"].as_u64()? as u32)?;
        return Some(Stack { entity_data: code, ..Stack::new(Item::SUSPICIOUS_STEW, 1) });
    }
    // `Raid.getOminousBannerInstance`: Java's raid banner.
    if c.get("banner_patterns").is_some() {
        return (c["item_name"]["translate"] == "block.minecraft.ominous_banner")
            .then(crate::content::banner_patterns::ominous_banner);
    }
    Some(Stack::new(item, 1))
}

/// A Java item id as the game's item, by id or (as `tests/java_registry.rs`
/// resolves them) by its English name from `registry_names.json`.
fn java_item(id: &str) -> Option<Item> {
    static NAMES: OnceLock<Value> = OnceLock::new();
    let names = NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../tests/fixtures/java/26.3/registry_names.json")).unwrap_or_default()
    });
    let english = names["items"][id].as_str().or_else(|| names["blocks"][id].as_str()).unwrap_or_default();
    crate::java_registry::item(id, english)
}

/// The player's contents of a fixture tab (`items`, else `player`).
fn contents(tab: &Value) -> &Value {
    if tab.get("items").is_some() { &tab["items"] } else { &tab["player"] }
}

fn build(json: &str) -> Catalogue {
    let v: Value = serde_json::from_str(json).expect("creative_tabs.json is valid JSON");
    let mut tabs = Vec::new();
    let mut skipped = Vec::new();
    for t in v["tabs"].as_array().into_iter().flatten() {
        let id = t["id"].as_str().unwrap_or_default().to_string();
        let kind = match t["type"].as_str() {
            Some("hotbar") => Kind::Hotbar,
            Some("search") => Kind::Search,
            Some("inventory") => Kind::Inventory,
            _ => Kind::Category,
        };
        let mut list = |key: &str| -> Vec<Stack> {
            let entries = &contents(t)[key];
            let mut out: Vec<Stack> = Vec::new();
            for e in entries.as_array().into_iter().flatten() {
                match resolve(e) {
                    Some(s) if !out.contains(&s) => out.push(s),
                    Some(_) => {}
                    None => {
                        let item = e["item"].as_str().unwrap_or_default().to_string();
                        if !skipped.contains(&(id.clone(), item.clone())) {
                            skipped.push((id.clone(), item));
                        }
                    }
                }
            }
            out
        };
        let (items, search) = if kind == Kind::Category {
            let items = list("display");
            let search = if contents(t).get("search").is_some() { list("search") } else { items.clone() };
            (items, search)
        } else {
            (Vec::new(), Vec::new())
        };
        // A tab whose icon the game lacks (the operator tab's command block) shows a stick.
        let icon = t["icon"].as_str().and_then(java_item).unwrap_or(Item::STICK);
        tabs.push(Tab {
            title: t["title"].as_str().unwrap_or_default().to_string(),
            row: if t["row"] == "top" { Row::Top } else { Row::Bottom },
            column: t["column"].as_u64().unwrap_or(0) as u8,
            kind,
            aligned_right: t["aligned_right"] == true,
            show_title: t["show_title"] != false,
            can_scroll: t["can_scroll"] != false,
            icon,
            items,
            search,
            id,
        });
    }
    let mut search: Vec<Stack> = Vec::new();
    for t in &tabs {
        for s in &t.search {
            if !search.contains(s) {
                search.push(*s);
            }
        }
    }
    let search_text = search.iter().map(haystack).collect();
    Catalogue { tabs, search, search_text, skipped }
}

/// The search tab's filter: every word of `query` appears in the stack's
/// name or its tooltip's enchantment lines (Java's name search tree reads
/// the whole tooltip, and its id search the registry id).
pub fn matches(stack: &Stack, query: &str) -> bool {
    let text = haystack(stack);
    let query = query.to_lowercase().replace('_', " ");
    query.split_whitespace().all(|word| text.contains(word))
}

/// What the search tab matches a stack by.
fn haystack(stack: &Stack) -> String {
    let mut text = stack.display_name().to_lowercase();
    for (line, _) in stack.enchants.lines() {
        text.push(' ');
        text.push_str(&line.to_lowercase());
    }
    text
}

/// `ItemPickerMenu` scrolling over a list of `len` stacks: Java keeps a
/// fraction 0..1 and shows rows from `getRowIndexForScroll`.
pub mod scroll {
    /// Rows shown at once (`NUM_ROWS`) and stacks per row (`NUM_COLS`).
    pub const ROWS: usize = 5;
    pub const COLS: usize = 9;

    /// `calculateRowCount`: rows past the five shown (can be negative).
    pub fn hidden_rows(len: usize) -> i32 {
        len.div_ceil(COLS) as i32 - ROWS as i32
    }

    /// `canScroll`: more than 45 stacks.
    pub fn can_scroll(len: usize) -> bool {
        len > ROWS * COLS
    }

    /// `getRowIndexForScroll`: the first row shown.
    pub fn first_row(len: usize, offs: f32) -> usize {
        ((offs * hidden_rows(len) as f32 + 0.5) as i32).max(0) as usize
    }

    /// `subtractInputFromScroll`: a wheel turn of `lines` (up positive).
    pub fn wheel(len: usize, offs: f32, lines: f64) -> f32 {
        let rows = hidden_rows(len);
        if rows <= 0 {
            return offs;
        }
        (offs - (lines / f64::from(rows)) as f32).clamp(0.0, 1.0)
    }

    /// `mouseDragged` on the scrollbar: `y` below the grid's top edge
    /// (`topPos + 18`) puts the 15-pixel thumb's centre under the pointer
    /// over the bar's 112 - 15 travel.
    pub fn drag(y: f32) -> f32 {
        ((y - 7.5) / (112.0 - 15.0)).clamp(0.0, 1.0)
    }

    /// The thumb's top, below the bar's top (`(112 - 17) * scrollOffs`).
    pub fn thumb(offs: f32) -> f32 {
        ((112.0 - 17.0) * offs).trunc()
    }

    /// The stack shown in grid cell `cell` (0..45).
    pub fn at(len: usize, offs: f32, cell: usize) -> Option<usize> {
        let i = (first_row(len, offs) + cell / COLS) * COLS + cell % COLS;
        (i < len).then_some(i)
    }
}

/// `CreativeModeInventoryScreen.slotClicked` on a creative slot holding
/// `clicked` (`None` for an empty cell), with `cursor` the carried stack:
/// a left click (or a right click on an empty cursor) picks one up (a full
/// stack with shift); on the same stack, left adds one (fills it with
/// shift) and right takes one off; on another, left clears the cursor and
/// right takes one off.
pub fn click(cursor: &mut Option<Stack>, clicked: Option<Stack>, right: bool, shift: bool) {
    match (cursor.as_mut(), clicked) {
        (Some(c), Some(s)) if c.stacks_with(&s) => {
            if !right {
                if shift {
                    c.count = c.max();
                } else if c.count < c.max() {
                    c.count += 1;
                }
            } else {
                c.count -= 1;
            }
        }
        (None, Some(s)) => {
            *cursor = Some(Stack { count: if shift { s.max() } else { s.count.max(1) }, ..s });
        }
        (Some(_), _) if !right => *cursor = None,
        (Some(c), _) => c.count -= 1,
        (None, None) => {}
    }
    if cursor.is_some_and(|c| c.count == 0) {
        *cursor = None;
    }
}

/// `ContainerInput.CLONE` (middle click) on a creative slot: a full stack
/// onto an empty cursor.
pub fn clone(cursor: &mut Option<Stack>, clicked: Option<Stack>) {
    if cursor.is_none()
        && let Some(s) = clicked
    {
        *cursor = Some(Stack { count: s.max(), ..s });
    }
}

/// Saved hotbars: `HotbarManager.NUM_HOTBAR_GROUPS` rows of nine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hotbars {
    pub rows: [[Option<Stack>; 9]; 9],
}

impl Hotbars {
    /// One line per row: nine `stack_to_string` fields separated by spaces.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            let fields: Vec<String> = row.iter().map(|&s| crate::inventory::stack_to_string(s)).collect();
            out.push_str(&fields.join(" "));
            out.push('\n');
        }
        out
    }

    /// Reads [`Hotbars::serialize`]'s text; an unreadable slot or a missing
    /// row stays empty (`HotbarManager.load` logs and goes on).
    pub fn deserialize(text: &str) -> Self {
        let mut out = Self::default();
        for (row, line) in out.rows.iter_mut().zip(text.lines()) {
            for (slot, field) in row.iter_mut().zip(line.split(' ')) {
                *slot = crate::inventory::stack_from_str(field).flatten();
            }
        }
        out
    }

    /// `Hotbar.isEmpty`.
    pub fn is_empty(&self, row: usize) -> bool {
        self.rows.get(row).is_none_or(|r| r.iter().all(Option::is_none))
    }

    /// `Hotbar.storeFrom`: the inventory's hotbar, slots 0..9.
    pub fn store(&mut self, row: usize, inventory: &crate::inventory::Inventory) {
        if let Some(r) = self.rows.get_mut(row) {
            for (i, slot) in r.iter_mut().enumerate() {
                *slot = inventory.get(i);
            }
        }
    }

    /// `handleHotbarLoadOrSave` loading: the row replaces the hotbar.
    pub fn load(&self, row: usize, inventory: &mut crate::inventory::Inventory) {
        if let Some(r) = self.rows.get(row) {
            inventory.slots[..9].copy_from_slice(r);
        }
    }

    /// The Saved Hotbars tab's 81 cells (`selectTab` with `Type.HOTBAR`): a
    /// saved row's stacks, or for an empty row a locked placeholder in the
    /// row's own column (`None` here; the screen draws the hint).
    pub fn cells(&self) -> Vec<Option<Stack>> {
        self.rows.iter().flat_map(|r| r.iter().copied()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_follows_item_picker_menu() {
        // 100 stacks: 12 rows, 7 hidden.
        assert_eq!(scroll::hidden_rows(100), 7);
        assert!(scroll::can_scroll(46) && !scroll::can_scroll(45));
        assert_eq!(scroll::first_row(100, 0.0), 0);
        assert_eq!(scroll::first_row(100, 1.0), 7);
        // Rounds half up: 0.5 of 7 rows is 3.5 -> 4.
        assert_eq!(scroll::first_row(100, 0.5), 4);
        // One notch down is one row's share.
        let down = scroll::wheel(100, 0.0, -1.0);
        assert!((down - 1.0 / 7.0).abs() < 1e-6);
        assert_eq!(scroll::first_row(100, down), 1);
        assert_eq!(scroll::wheel(100, 0.0, 1.0), 0.0);
        assert_eq!(scroll::wheel(30, 0.3, -1.0), 0.3, "nothing hidden: no scroll");
        assert_eq!(scroll::drag(7.5), 0.0);
        assert_eq!(scroll::drag(104.5), 1.0);
        assert_eq!(scroll::thumb(1.0), 95.0);
        assert_eq!(scroll::at(100, 1.0, 0), Some(63));
        assert_eq!(scroll::at(100, 1.0, 44), None);
    }

    #[test]
    fn creative_clicks_follow_java() {
        let stone = Stack::new(Item::from_block(crate::world::block::Block::STONE), 1);
        let dirt = Stack::new(Item::from_block(crate::world::block::Block::DIRT), 1);
        let mut cursor = None;
        click(&mut cursor, Some(stone), false, false);
        assert_eq!(cursor.map(|c| c.count), Some(1), "a click takes one");
        click(&mut cursor, Some(stone), false, false);
        assert_eq!(cursor.map(|c| c.count), Some(2), "the same stack adds one");
        click(&mut cursor, Some(stone), false, true);
        assert_eq!(cursor.map(|c| c.count), Some(64), "shift fills it");
        click(&mut cursor, Some(stone), true, false);
        assert_eq!(cursor.map(|c| c.count), Some(63), "right takes one off");
        click(&mut cursor, Some(dirt), true, false);
        assert_eq!(cursor.map(|c| (c.item, c.count)), Some((stone.item, 62)));
        click(&mut cursor, Some(dirt), false, false);
        assert_eq!(cursor, None, "left on another stack deletes the held one");
        click(&mut cursor, Some(dirt), false, true);
        assert_eq!(cursor.map(|c| c.count), Some(64), "shift takes a full stack");
        let mut empty = None;
        clone(&mut empty, Some(dirt));
        assert_eq!(empty.map(|c| c.count), Some(64));
    }

    #[test]
    fn hotbars_round_trip_and_load() {
        let mut inv = crate::inventory::Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::DIAMOND, 5));
        inv.slots[8] = Some(Stack::tipped_arrow(crate::potion::Potion(3), 2));
        let mut h = Hotbars::default();
        assert!(h.is_empty(2));
        h.store(2, &inv);
        assert!(!h.is_empty(2));
        let back = Hotbars::deserialize(&h.serialize());
        assert_eq!(back, h);
        let mut fresh = crate::inventory::Inventory::default();
        fresh.slots[4] = Some(Stack::new(Item::STICK, 1));
        back.load(2, &mut fresh);
        assert_eq!(fresh.slots[..9], inv.slots[..9]);
        assert_eq!(Hotbars::deserialize("garbage\n"), Hotbars::default());
    }
}
