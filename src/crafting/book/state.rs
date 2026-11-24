//! One player's recipe book (Java's `ServerRecipeBook` and
//! `RecipeBookSettings`): the recipes they know, the ones newly unlocked
//! and not yet seen in the book (highlighted), and the book's open and
//! filter settings for each book type, saved by stable Java recipe ID.
//!
//! Recipes unlock through their advancement's criteria, checked directly
//! (advancements themselves are DEF-01): an item entering the inventory,
//! ten occupied inventory slots (the chest), any tick (the crafting table)
//! or standing in water (boats), and crafting or smelting the recipe. A
//! new player knows nothing until those fire. As Java's slot listener
//! reports every filled slot when a player joins, a loaded player's saved
//! inventory is checked again on their first tick; a player saved before
//! the book existed starts from Java's initial state that way, and gains
//! only what their inventory proves.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::{Map, Value, json};

use super::{BookType, Grid, RecipeId, data, grid_recipe, recipes};
use crate::inventory::{Inventory, SLOTS, Stack};
use crate::item::Item;

/// `RecipeBookSettings.TypeSettings`: both default to false.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeSettings {
    pub open: bool,
    pub filtering: bool,
}

/// The inventory slots Java's `Inventory` container reports: 36 main, the
/// four armor pieces and the off-hand (the body and saddle slots a player
/// never fills count as empty).
const WATCHED: usize = SLOTS + 4 + 1;
const CONTAINER_SIZE: usize = SLOTS + 4 + 1 + 2;

#[derive(Clone, Debug)]
pub struct RecipeBook {
    known: Vec<u64>,
    highlight: Vec<u64>,
    /// Indexed by [`BookType::index`].
    pub settings: [TypeSettings; 4],
    /// Recipes unlocked with a notification since the adapter last took
    /// them (the toast). Not saved.
    toasts: Vec<RecipeId>,
    /// The inventory as of the last check; `None` until the first, so a
    /// loaded inventory counts as just filled. Not saved.
    seen: Option<Box<[Option<Stack>; WATCHED]>>,
    /// Changes whenever the known or highlighted recipes change, so views
    /// rebuild their pages. Not saved.
    pub revision: u32,
}

impl Default for RecipeBook {
    fn default() -> Self {
        let words = RecipeId::count().div_ceil(64);
        Self {
            known: vec![0; words],
            highlight: vec![0; words],
            settings: Default::default(),
            toasts: Vec::new(),
            seen: None,
            revision: 0,
        }
    }
}

fn has(bits: &[u64], id: RecipeId) -> bool {
    bits[id.0 as usize / 64] & (1 << (id.0 % 64)) != 0
}

fn set(bits: &mut [u64], id: RecipeId, on: bool) {
    let word = &mut bits[id.0 as usize / 64];
    if on {
        *word |= 1 << (id.0 % 64);
    } else {
        *word &= !(1 << (id.0 % 64));
    }
}

/// The criteria, indexed by what fires them.
struct Criteria {
    by_item: HashMap<Item, Vec<RecipeId>>,
    tick: Vec<RecipeId>,
    water: Vec<RecipeId>,
    slots: Vec<(RecipeId, usize)>,
}

fn criteria() -> &'static Criteria {
    static CRITERIA: OnceLock<Criteria> = OnceLock::new();
    CRITERIA.get_or_init(|| {
        let mut c = Criteria { by_item: HashMap::new(), tick: Vec::new(), water: Vec::new(), slots: Vec::new() };
        for id in RecipeId::all() {
            let unlock = &id.raw().unlock;
            for &name in unlock.items {
                if let Some(item) = super::name_item(name) {
                    let list = c.by_item.entry(item).or_default();
                    if !list.contains(&id) {
                        list.push(id);
                    }
                }
            }
            if unlock.tick {
                c.tick.push(id);
            }
            if unlock.enter_water {
                c.water.push(id);
            }
            if unlock.occupied_min > 0 {
                c.slots.push((id, usize::from(unlock.occupied_min)));
            }
        }
        c
    })
}

impl RecipeBook {
    pub fn knows(&self, id: RecipeId) -> bool {
        has(&self.known, id)
    }

    /// Newly unlocked and not yet shown in the book.
    pub fn highlighted(&self, id: RecipeId) -> bool {
        has(&self.highlight, id)
    }

    pub fn known(&self) -> impl Iterator<Item = RecipeId> + '_ {
        RecipeId::all().filter(|&id| self.knows(id))
    }

    pub fn known_count(&self) -> usize {
        self.known.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn highlighted_count(&self) -> usize {
        self.highlight.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// `ServerRecipeBook.addRecipes` for one recipe: known and highlighted,
    /// with a toast if the recipe notifies. False if already known.
    pub fn unlock(&mut self, id: RecipeId) -> bool {
        if self.knows(id) {
            return false;
        }
        set(&mut self.known, id, true);
        set(&mut self.highlight, id, true);
        if id.raw().notify && id.toast().is_some() {
            self.toasts.push(id);
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// `removeHighlight`: the book showed the recipe.
    pub fn mark_seen(&mut self, id: RecipeId) {
        if self.highlighted(id) {
            set(&mut self.highlight, id, false);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Recipes unlocked with a notification since the last call, for the toast.
    pub fn take_toasts(&mut self) -> Vec<RecipeId> {
        std::mem::take(&mut self.toasts)
    }

    pub fn settings(&self, book: BookType) -> TypeSettings {
        self.settings[book.index()]
    }

    pub fn set_open(&mut self, book: BookType, open: bool) {
        self.settings[book.index()].open = open;
    }

    pub fn set_filtering(&mut self, book: BookType, filtering: bool) {
        self.settings[book.index()].filtering = filtering;
    }

    /// One tick's unlock criteria: `tick`, `enter_block` water when
    /// `in_water`, and `inventory_changed` for each inventory slot that
    /// changed since the last tick (every filled slot on the first).
    pub fn tick(&mut self, inventory: &Inventory, in_water: bool) {
        let c = criteria();
        for &id in &c.tick {
            self.unlock(id);
        }
        if in_water {
            for &id in &c.water {
                self.unlock(id);
            }
        }
        let mut now: Box<[Option<Stack>; WATCHED]> = Box::new([None; WATCHED]);
        now[..SLOTS].copy_from_slice(&inventory.slots);
        now[SLOTS..SLOTS + 4].copy_from_slice(&inventory.armor);
        now[SLOTS + 4] = inventory.offhand;
        let changed: Vec<usize> = match &self.seen {
            None => (0..WATCHED).filter(|&i| now[i].is_some()).collect(),
            Some(old) => (0..WATCHED).filter(|&i| old[i] != now[i]).collect(),
        };
        if !changed.is_empty() {
            let occupied = now.iter().flatten().count();
            debug_assert!(occupied <= CONTAINER_SIZE);
            for i in changed {
                for &(id, min) in &c.slots {
                    if occupied >= min {
                        self.unlock(id);
                    }
                }
                if let Some(ids) = now[i].and_then(|s| c.by_item.get(&s.item)) {
                    for &id in ids {
                        self.unlock(id);
                    }
                }
            }
        }
        self.seen = Some(now);
    }

    /// `RecipeCraftingHolder.awardUsedRecipes`: taking a crafting result
    /// teaches its recipe.
    pub fn crafted(&mut self, grid: &Grid) {
        if let Some(id) = grid_recipe(grid) {
            self.unlock(id);
        }
    }

    /// `AbstractFurnaceBlockEntity.awardUsedRecipes`: taking from a
    /// furnace's output teaches every recipe its tally holds
    /// (`furnace::recipe_key` names them).
    pub fn cooked(&mut self, kind: crate::world::furnace::FurnaceKind, keys: &[Item]) {
        let cooking = BookType::of_furnace(kind).cooking();
        for recipe in recipes().iter().filter(|r| Some(r.kind) == cooking) {
            let result = recipe.result.item;
            if recipe.ingredients[0]
                .iter()
                .any(|&input| keys.contains(&crate::world::furnace::recipe_key(input, result)))
            {
                self.unlock(recipe.id);
            }
        }
    }

    /// Whether `limited_crafting` lets this player take what `grid` makes:
    /// special recipes always craft, learnable ones only once known
    /// (`RecipeCraftingHolder.setRecipeUsed`).
    pub fn allows(&self, grid: &Grid, limited: bool) -> bool {
        !limited || grid_recipe(grid).is_none_or(|id| self.knows(id))
    }

    /// A LAN guest's copy from the host's [`RecipeBook::serialize`]: the
    /// known and highlighted recipes are replaced, and newly known ones
    /// queue their toasts. The settings are taken only with `settings`,
    /// since the guest changes its own and tells the host.
    pub fn sync_from(&mut self, text: &str, settings: bool) {
        let other = Self::deserialize(text);
        for id in other.known() {
            if !self.knows(id) && id.raw().notify && id.toast().is_some() {
                self.toasts.push(id);
            }
        }
        self.known = other.known;
        self.highlight = other.highlight;
        if settings {
            self.settings = other.settings;
        }
        self.revision = self.revision.wrapping_add(1);
    }

    /// Java's `recipeBook` player data as JSON: the settings keys, then
    /// `recipes` and `toBeDisplayed` as namespaced recipe IDs.
    pub fn serialize(&self) -> String {
        let mut map = Map::new();
        for book in BookType::ALL {
            let (open, filtering) = book.settings_keys();
            let s = self.settings(book);
            map.insert(open.into(), json!(s.open));
            map.insert(filtering.into(), json!(s.filtering));
        }
        let ids = |bits: &[u64]| -> Vec<Value> {
            RecipeId::all().filter(|&id| has(bits, id)).map(|id| json!(format!("minecraft:{}", id.name()))).collect()
        };
        map.insert("recipes".into(), Value::Array(ids(&self.known)));
        map.insert("toBeDisplayed".into(), Value::Array(ids(&self.highlight)));
        Value::Object(map).to_string()
    }

    /// Reads [`RecipeBook::serialize`]'s JSON (`loadUntrusted`): missing
    /// settings are false, and IDs that name no learnable recipe are
    /// dropped. Unreadable text gives the initial book.
    pub fn deserialize(text: &str) -> Self {
        let mut book = Self::default();
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else { return book };
        for b in BookType::ALL {
            let (open, filtering) = b.settings_keys();
            book.settings[b.index()] = TypeSettings {
                open: map.get(open).and_then(Value::as_bool).unwrap_or(false),
                filtering: map.get(filtering).and_then(Value::as_bool).unwrap_or(false),
            };
        }
        for (key, bits) in [("recipes", &mut book.known), ("toBeDisplayed", &mut book.highlight)] {
            for id in map.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                if let Some(id) = RecipeId::from_name(id) {
                    set(bits, id, true);
                }
            }
        }
        book
    }
}

impl RecipeId {
    /// What the toast shows (`RecipeToast.addOrUpdate`): the workstation
    /// and the result, if VoxelCraft registers every item of the recipe.
    pub fn toast(self) -> Option<(Item, Stack)> {
        let raw = self.raw();
        let result = super::name_item(raw.result)?;
        let complete = raw
            .slots
            .iter()
            .filter(|&&s| s != u16::MAX)
            .all(|&s| data::ALTS[s as usize].iter().any(|&n| super::name_item(n).is_some()));
        complete.then(|| (raw.kind.station(), Stack::new(result, raw.count)))
    }
}
