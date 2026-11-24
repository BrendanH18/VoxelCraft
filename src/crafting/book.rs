//! v0.18 Track 5: Java 26.3's recipe book.
//!
//! The data is Java's: every learnable recipe (`Recipe.isSpecial` false)
//! with its stable ID, recipe book category, group and notification flag,
//! and the criteria of the advancement under
//! `data/minecraft/advancement/recipes/` that unlocks it
//! (`tests/fixtures/java/26.3/recipe_book.json`, with the shapes from
//! `recipes.json`), embedded by `build.rs`. Java item names resolve to
//! VoxelCraft items once; a recipe the game can't make yet (an unregistered
//! result or ingredient) is still learnable and saved, but not shown.
//!
//! * [`state`]: what a player knows and has not yet seen, the book's open
//!   and filter settings per book type, unlock criteria and saves
//!   (`ServerRecipeBook`, `RecipeBookSettings`).
//! * [`stacked`]: what the player could craft (`StackedItemContents`).
//! * [`place`]: clicking a recipe fills the grid or furnace input from the
//!   inventory (`ServerPlaceRecipe`, `PlaceRecipeHelper`).
//!
//! Crafting itself stays in [`super::Grid`]; the book only moves items.

pub mod place;
pub mod stacked;
pub mod state;

use std::sync::OnceLock;

use super::Grid;
use crate::inventory::Stack;
use crate::item::Item;

pub use state::{RecipeBook, TypeSettings};

/// Java's recipe book categories (`RecipeBookCategories`), in registry order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    CraftingBuildingBlocks,
    CraftingRedstone,
    CraftingEquipment,
    CraftingMisc,
    FurnaceFood,
    FurnaceBlocks,
    FurnaceMisc,
    BlastFurnaceBlocks,
    BlastFurnaceMisc,
    SmokerFood,
    Stonecutter,
    Smithing,
    Campfire,
}

impl Category {
    pub const ALL: [Self; 13] = [
        Self::CraftingBuildingBlocks,
        Self::CraftingRedstone,
        Self::CraftingEquipment,
        Self::CraftingMisc,
        Self::FurnaceFood,
        Self::FurnaceBlocks,
        Self::FurnaceMisc,
        Self::BlastFurnaceBlocks,
        Self::BlastFurnaceMisc,
        Self::SmokerFood,
        Self::Stonecutter,
        Self::Smithing,
        Self::Campfire,
    ];

    /// The registry ID.
    pub const fn id(self) -> &'static str {
        match self {
            Self::CraftingBuildingBlocks => "crafting_building_blocks",
            Self::CraftingRedstone => "crafting_redstone",
            Self::CraftingEquipment => "crafting_equipment",
            Self::CraftingMisc => "crafting_misc",
            Self::FurnaceFood => "furnace_food",
            Self::FurnaceBlocks => "furnace_blocks",
            Self::FurnaceMisc => "furnace_misc",
            Self::BlastFurnaceBlocks => "blast_furnace_blocks",
            Self::BlastFurnaceMisc => "blast_furnace_misc",
            Self::SmokerFood => "smoker_food",
            Self::Stonecutter => "stonecutter",
            Self::Smithing => "smithing",
            Self::Campfire => "campfire",
        }
    }
}

/// The recipe's JSON type, as the book needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `crafting_shaped`, and `crafting_imbue` as its 3x3 shape.
    Shaped,
    Shapeless,
    /// `crafting_transmute`: one input and one material (map cloning takes
    /// one to eight maps).
    Transmute,
    /// `crafting_dye`: one target and one or more dyes.
    Dye,
    Smelting,
    Blasting,
    Smoking,
    Campfire,
    Stonecutting,
    Smithing,
}

impl Kind {
    pub const fn is_crafting(self) -> bool {
        matches!(self, Self::Shaped | Self::Shapeless | Self::Transmute | Self::Dye)
    }

    /// The workstation the toast shows (`RecipeDisplay.craftingStation`).
    pub fn station(self) -> Item {
        use crate::world::block::Block;
        match self {
            Self::Shaped | Self::Shapeless | Self::Transmute | Self::Dye => Item::from_block(Block::CRAFTING_TABLE),
            Self::Smelting => Item::from_block(Block::FURNACE),
            Self::Blasting => Item::from_block(Block::BLAST_FURNACE),
            Self::Smoking => Item::from_block(Block::SMOKER),
            Self::Campfire => Item::from_block(crate::world::utilities::CAMPFIRE),
            Self::Stonecutting => Item::from_block(Block::STONECUTTER),
            Self::Smithing => Item::from_block(Block::SMITHING_TABLE),
        }
    }
}

/// One learnable recipe as `build.rs` embeds it: Java item names are
/// indices into `NAMES`, ingredients indices into `ALTS`.
pub struct Raw {
    pub id: &'static str,
    pub kind: Kind,
    pub category: Category,
    pub group: Option<&'static str>,
    /// `show_notification`: whether unlocking shows the toast.
    pub notify: bool,
    /// Shaped pattern size (0 for other kinds).
    pub width: u8,
    pub height: u8,
    /// Pattern cells (shaped, `u16::MAX` empty) or ingredients, as `ALTS` indices.
    pub slots: &'static [u16],
    pub result: u16,
    pub count: u8,
    pub unlock: RawUnlock,
}

/// The unlocking advancement's criteria; any one unlocks the recipe
/// (every recipe advancement has one requirement group).
pub struct RawUnlock {
    /// `inventory_changed` with one item predicate: an item or tag entering
    /// the inventory. Tags are expanded.
    pub items: &'static [u16],
    /// `tick`: the first tick (the crafting table).
    pub tick: bool,
    /// `inventory_changed` with `slots.occupied.min` (the chest).
    pub occupied_min: u8,
    /// `enter_block` water (boats and rafts).
    pub enter_water: bool,
}

mod data {
    use super::{Category, Kind, Raw, RawUnlock};
    include!(concat!(env!("OUT_DIR"), "/recipe_book.rs"));
}

/// The ID of a learnable recipe: its index in Java's sorted ID list. Saves
/// use [`RecipeId::name`], never the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecipeId(pub u16);

impl RecipeId {
    /// Every learnable recipe, in ID order.
    pub fn all() -> impl Iterator<Item = RecipeId> {
        (0..data::LEARNABLE.len() as u16).map(RecipeId)
    }

    pub fn count() -> usize {
        data::LEARNABLE.len()
    }

    pub fn raw(self) -> &'static Raw {
        &data::LEARNABLE[self.0 as usize]
    }

    /// Java's recipe ID without the namespace (`oak_planks`).
    pub fn name(self) -> &'static str {
        self.raw().id
    }

    /// A learnable recipe by ID, with or without `minecraft:`.
    pub fn from_name(name: &str) -> Option<RecipeId> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        data::LEARNABLE.binary_search_by(|r| r.id.cmp(name)).ok().map(|i| RecipeId(i as u16))
    }

    /// The recipe as the book shows it, if VoxelCraft can make it.
    pub fn recipe(self) -> Option<&'static BookRecipe> {
        let table = table();
        table.by_id[self.0 as usize].map(|i| &table.recipes[i as usize])
    }
}

/// Whether `name` is one of Java's special recipes, which are never
/// learnable and always craft under `limited_crafting`.
pub fn is_special(name: &str) -> bool {
    data::SPECIAL.contains(&name.strip_prefix("minecraft:").unwrap_or(name))
}

/// One ingredient's accepted items.
pub type Ingredient = Vec<Item>;

/// A recipe the book shows: resolved items, in Java's placement order.
pub struct BookRecipe {
    pub id: RecipeId,
    pub kind: Kind,
    pub category: Category,
    /// Index of the Java group in this table, for grouping into one button.
    pub group: Option<u16>,
    /// Pattern size for shaped recipes; 0 otherwise.
    pub width: usize,
    pub height: usize,
    /// `PlacementInfo.ingredients`: the filled pattern cells in order, or
    /// the shapeless ingredients.
    pub ingredients: Vec<Ingredient>,
    /// `PlacementInfo.slotsToIngredientIndex`: per pattern cell (shaped)
    /// or per ingredient, the ingredient it takes.
    pub slots: Vec<Option<usize>>,
    pub result: Stack,
}

impl BookRecipe {
    /// Whether it needs no more than a `size` x `size` grid
    /// (`CraftingRecipeBookComponent.canDisplay`).
    pub fn fits(&self, size: usize) -> bool {
        match self.kind {
            Kind::Shaped => self.width <= size && self.height <= size,
            Kind::Shapeless | Kind::Transmute | Kind::Dye => self.ingredients.len() <= size * size,
            _ => false,
        }
    }

    /// The book type that shows this recipe, if any.
    pub fn book(&self) -> Option<BookType> {
        BookType::ALL.into_iter().find(|b| b.categories().contains(&self.category))
    }

    /// Whether the grid's items make this recipe (`Recipe.matches`).
    pub fn matches_grid(&self, grid: &Grid) -> bool {
        let n = grid.size;
        let cells = &grid.cells[..n * n];
        match self.kind {
            Kind::Shaped => {
                let Some((x0, y0, w, h)) = bounds(cells, n) else { return false };
                if w != self.width || h != self.height {
                    return false;
                }
                let fits = |mirror: bool| {
                    (0..h).all(|y| {
                        (0..w).all(|x| {
                            let px = if mirror { w - 1 - x } else { x };
                            let want = self.slots[y * w + px].map(|i| &self.ingredients[i]);
                            match (want, cells[(y0 + y) * n + x0 + x]) {
                                (None, None) => true,
                                (Some(options), Some(s)) => options.contains(&s.item),
                                _ => false,
                            }
                        })
                    })
                };
                fits(false) || fits(true)
            }
            Kind::Shapeless => {
                let mut have: Vec<Item> = cells.iter().flatten().map(|s| s.item).collect();
                have.len() == self.ingredients.len()
                    && self.ingredients.iter().all(|options| match have.iter().position(|i| options.contains(i)) {
                        Some(p) => {
                            have.swap_remove(p);
                            true
                        }
                        None => false,
                    })
            }
            Kind::Transmute | Kind::Dye => {
                // One input (target) and the material (dyes): one for a
                // transmute (map cloning takes up to eight), any for a dye.
                let (input, material) = (&self.ingredients[0], &self.ingredients[1]);
                let (mut inputs, mut materials) = (0, 0);
                for s in cells.iter().flatten() {
                    if input.contains(&s.item) && inputs == 0 {
                        inputs += 1;
                    } else if material.contains(&s.item) {
                        materials += 1;
                    } else {
                        return false;
                    }
                }
                let most = match self.kind {
                    Kind::Dye => 8,
                    _ if self.id.name() == "map_cloning" => 8,
                    _ => 1,
                };
                inputs == 1 && (1..=most).contains(&materials)
            }
            _ => false,
        }
    }

    /// Whether a furnace input slot holds this recipe's ingredient.
    pub fn matches_input(&self, input: Option<Stack>) -> bool {
        matches!(self.kind, Kind::Smelting | Kind::Blasting | Kind::Smoking)
            && input.is_some_and(|s| self.ingredients[0].contains(&s.item))
    }
}

/// The filled bounding box of a grid: x, y, width, height.
fn bounds(cells: &[Option<Stack>], n: usize) -> Option<(usize, usize, usize, usize)> {
    let filled = |x: usize, y: usize| cells[y * n + x].is_some();
    let cols: Vec<usize> = (0..n).filter(|&x| (0..n).any(|y| filled(x, y))).collect();
    let rows: Vec<usize> = (0..n).filter(|&y| (0..n).any(|x| filled(x, y))).collect();
    let (x0, x1, y0, y1) = (*cols.first()?, *cols.last()?, *rows.first()?, *rows.last()?);
    Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

struct Table {
    /// Shown recipes in ID order.
    recipes: Vec<BookRecipe>,
    /// Per learnable recipe, its index in `recipes`.
    by_id: Vec<Option<u16>>,
    /// Resolved `NAMES`.
    items: Vec<Option<Item>>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let items: Vec<Option<Item>> =
            data::NAMES.iter().map(|&(id, english)| crate::java_registry::item(id, english)).collect();
        let alternatives = |alt: u16| -> Ingredient {
            let mut out: Ingredient = Vec::new();
            for &n in data::ALTS[alt as usize] {
                if let Some(item) = items[n as usize]
                    && !out.contains(&item)
                {
                    out.push(item);
                }
            }
            out
        };
        let mut groups: Vec<&'static str> = Vec::new();
        let mut recipes = Vec::new();
        let mut by_id = vec![None; data::LEARNABLE.len()];
        for (i, raw) in data::LEARNABLE.iter().enumerate() {
            if !matches!(
                raw.kind,
                Kind::Shaped
                    | Kind::Shapeless
                    | Kind::Transmute
                    | Kind::Dye
                    | Kind::Smelting
                    | Kind::Blasting
                    | Kind::Smoking
            ) {
                continue;
            }
            let Some(result) = items[raw.result as usize] else { continue };
            let mut ingredients = Vec::new();
            let mut slots = Vec::new();
            let mut complete = true;
            for &slot in raw.slots {
                if slot == u16::MAX {
                    slots.push(None);
                    continue;
                }
                let options = alternatives(slot);
                complete &= !options.is_empty();
                slots.push(Some(ingredients.len()));
                ingredients.push(options);
            }
            if !complete {
                continue;
            }
            let (mut width, mut height) = (raw.width as usize, raw.height as usize);
            if raw.kind == Kind::Shaped {
                // `ShapedRecipePattern.shrink`: empty outer rows and columns go.
                let cell = |x: usize, y: usize| slots[y * width + x].is_some();
                let cols: Vec<usize> = (0..width).filter(|&x| (0..height).any(|y| cell(x, y))).collect();
                let rows: Vec<usize> = (0..height).filter(|&y| (0..width).any(|x| cell(x, y))).collect();
                let (x0, x1, y0, y1) = (cols[0], cols[cols.len() - 1], rows[0], rows[rows.len() - 1]);
                slots =
                    (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).map(|(x, y)| slots[y * width + x]).collect();
                (width, height) = (x1 - x0 + 1, y1 - y0 + 1);
            }
            let group = raw.group.map(|g| match groups.iter().position(|&h| h == g) {
                Some(p) => p as u16,
                None => {
                    groups.push(g);
                    (groups.len() - 1) as u16
                }
            });
            by_id[i] = Some(recipes.len() as u16);
            recipes.push(BookRecipe {
                id: RecipeId(i as u16),
                kind: raw.kind,
                category: raw.category,
                group,
                width,
                height,
                ingredients,
                slots,
                result: Stack::new(result, raw.count),
            });
        }
        Table { recipes, by_id, items }
    })
}

/// The item a `NAMES` entry resolves to.
fn name_item(name: u16) -> Option<Item> {
    table().items[name as usize]
}

/// Every recipe the book can show, in ID order.
pub fn recipes() -> &'static [BookRecipe] {
    &table().recipes
}

/// The item a Java item name resolves to (for tab icons), if registered.
pub fn java_item(id: &str) -> Option<Item> {
    let table = table();
    data::NAMES
        .iter()
        .position(|&(n, _)| n == id)
        .and_then(|i| table.items[i])
        .or_else(|| crate::java_registry::item(id, ""))
}

/// The learnable recipe a crafting grid makes, if any: the first in ID
/// order whose shape matches (Java's `RecipeManager.getRecipeFor`). `None`
/// for an empty grid, a special recipe or no recipe.
pub fn grid_recipe(grid: &Grid) -> Option<RecipeId> {
    let n = grid.size;
    let filled = grid.cells[..n * n].iter().flatten().count();
    if filled == 0 {
        return None;
    }
    recipes()
        .iter()
        .filter(|r| r.kind.is_crafting())
        .filter(|r| match r.kind {
            Kind::Shaped => r.slots.iter().flatten().count() == filled,
            Kind::Shapeless => r.ingredients.len() == filled,
            _ => true,
        })
        .find(|r| r.matches_grid(grid))
        .map(|r| r.id)
}

/// `RecipeBookType`: the four menus with a recipe book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BookType {
    Crafting,
    Furnace,
    BlastFurnace,
    Smoker,
}

/// One tab of a book: the search tab (every category of the book) or one
/// category, with Java's icon item IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tab {
    pub category: Option<Category>,
    pub icons: &'static [&'static str],
}

impl BookType {
    pub const ALL: [Self; 4] = [Self::Crafting, Self::Furnace, Self::BlastFurnace, Self::Smoker];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// The book of a furnace kind.
    pub fn of_furnace(kind: crate::world::furnace::FurnaceKind) -> Self {
        use crate::world::furnace::FurnaceKind;
        match kind {
            FurnaceKind::Normal => Self::Furnace,
            FurnaceKind::Blast => Self::BlastFurnace,
            FurnaceKind::Smoker => Self::Smoker,
        }
    }

    /// The search tab's categories (`SearchRecipeBookCategory`).
    pub const fn categories(self) -> &'static [Category] {
        use Category::*;
        match self {
            Self::Crafting => &[CraftingEquipment, CraftingBuildingBlocks, CraftingMisc, CraftingRedstone],
            Self::Furnace => &[FurnaceFood, FurnaceBlocks, FurnaceMisc],
            Self::BlastFurnace => &[BlastFurnaceBlocks, BlastFurnaceMisc],
            Self::Smoker => &[SmokerFood],
        }
    }

    /// The tabs, top to bottom (`CraftingRecipeBookComponent.TABS`,
    /// `FurnaceScreen`, `BlastFurnaceScreen`, `SmokerScreen`).
    pub const fn tabs(self) -> &'static [Tab] {
        use Category::*;
        const SEARCH: Tab = Tab { category: None, icons: &["compass"] };
        match self {
            Self::Crafting => &[
                SEARCH,
                Tab { category: Some(CraftingEquipment), icons: &["iron_axe", "golden_sword"] },
                Tab { category: Some(CraftingBuildingBlocks), icons: &["bricks"] },
                Tab { category: Some(CraftingMisc), icons: &["lava_bucket", "apple"] },
                Tab { category: Some(CraftingRedstone), icons: &["redstone"] },
            ],
            Self::Furnace => &[
                SEARCH,
                Tab { category: Some(FurnaceFood), icons: &["porkchop"] },
                Tab { category: Some(FurnaceBlocks), icons: &["stone"] },
                Tab { category: Some(FurnaceMisc), icons: &["lava_bucket", "emerald"] },
            ],
            Self::BlastFurnace => &[
                SEARCH,
                Tab { category: Some(BlastFurnaceBlocks), icons: &["redstone_ore"] },
                Tab { category: Some(BlastFurnaceMisc), icons: &["iron_shovel", "golden_leggings"] },
            ],
            Self::Smoker => &[SEARCH, Tab { category: Some(SmokerFood), icons: &["porkchop"] }],
        }
    }

    /// The filter button's tooltip while filtering
    /// (`gui.recipebook.toggleRecipes.*`).
    pub const fn filter_label(self) -> &'static str {
        match self {
            Self::Crafting => "Showing Craftable",
            Self::Furnace => "Showing Smeltable",
            Self::BlastFurnace => "Showing Blastable",
            Self::Smoker => "Showing Smokable",
        }
    }

    /// `RecipeBookSettings.TypeSettings` keys: open, filtering.
    pub const fn settings_keys(self) -> (&'static str, &'static str) {
        match self {
            Self::Crafting => ("isGuiOpen", "isFilteringCraftable"),
            Self::Furnace => ("isFurnaceGuiOpen", "isFurnaceFilteringCraftable"),
            Self::BlastFurnace => ("isBlastingFurnaceGuiOpen", "isBlastingFurnaceFilteringCraftable"),
            Self::Smoker => ("isSmokerGuiOpen", "isSmokerFilteringCraftable"),
        }
    }

    /// The recipe kind a furnace-type book places.
    pub const fn cooking(self) -> Option<Kind> {
        match self {
            Self::Crafting => None,
            Self::Furnace => Some(Kind::Smelting),
            Self::BlastFurnace => Some(Kind::Blasting),
            Self::Smoker => Some(Kind::Smoking),
        }
    }
}

/// `gui.recipebook.toggleRecipes.all`.
pub const SHOWING_ALL: &str = "Showing All";
/// `gui.recipebook.search_hint`.
pub const SEARCH_HINT: &str = "Search...";
/// `gui.recipebook.moreRecipes`.
pub const MORE_RECIPES: &str = "Right Click for More";
/// `gui.recipebook.next_page` and `previous_page`.
pub const NEXT_PAGE: &str = "Next Page";
pub const PREVIOUS_PAGE: &str = "Previous Page";
/// `recipe.toast.title` and `recipe.toast.description`.
pub const TOAST_TITLE: &str = "New Recipe(s) Unlocked!";
pub const TOAST_DESCRIPTION: &str = "Check your recipe book";
/// `RecipeToast.DISPLAY_TIME`, milliseconds.
pub const TOAST_MILLIS: u64 = 5000;
/// `RecipeBookComponent.IMAGE_WIDTH` and `IMAGE_HEIGHT`.
pub const IMAGE_WIDTH: f32 = 147.0;
pub const IMAGE_HEIGHT: f32 = 166.0;
/// `RecipeBookComponent.OFFSET_X_POSITION`: how far left of centre the
/// book sits beside the menu.
pub const OFFSET_X: f32 = 86.0;
/// `RecipeBookComponent.TICKS_TO_SWAP_SLOT`: ghost and grouped icons cycle.
pub const TICKS_TO_SWAP: u32 = 30;
/// `RecipeBookPage.ITEMS_PER_PAGE` (five by four buttons).
pub const PER_PAGE: usize = 20;
/// `RecipeButton.BACKGROUND_SIZE` and `ANIMATION_TIME` (ticks).
pub const BUTTON_SIZE: f32 = 25.0;
pub const HIGHLIGHT_TICKS: f32 = 15.0;
/// `AbstractRecipeBookScreen.widthTooNarrow`: narrower screens lay the
/// book over the menu.
pub const NARROW_BELOW: f32 = 379.0;

/// One button of the book: a recipe group (or a single recipe) of one
/// category, in the order Java's `ClientRecipeBook.rebuildCollections`
/// builds them from the known recipes.
#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    pub category: Category,
    pub recipes: Vec<RecipeId>,
}

/// The known recipes the book shows, grouped into buttons per category.
pub fn collections(book: &RecipeBook) -> Vec<Collection> {
    let mut out: Vec<Collection> = Vec::new();
    let mut groups: Vec<(Category, u16, usize)> = Vec::new();
    for recipe in recipes() {
        if !book.knows(recipe.id) {
            continue;
        }
        match recipe.group {
            None => out.push(Collection { category: recipe.category, recipes: vec![recipe.id] }),
            Some(g) => match groups.iter().find(|&&(c, h, _)| c == recipe.category && h == g) {
                Some(&(_, _, i)) => out[i].recipes.push(recipe.id),
                None => {
                    groups.push((recipe.category, g, out.len()));
                    out.push(Collection { category: recipe.category, recipes: vec![recipe.id] });
                }
            },
        }
    }
    out
}

/// What the book shows of one collection on one menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub collection: Collection,
    /// Recipes that fit the menu (`RecipeCollection.selected`).
    pub fitting: Vec<RecipeId>,
    /// Those the player has the items for (`craftable`).
    pub craftable: Vec<RecipeId>,
}

impl Shown {
    /// The recipes its button cycles through: the craftable ones while
    /// filtering, else every fitting one (`RecipeButton.init`).
    pub fn entries(&self, filtering: bool) -> &[RecipeId] {
        if filtering { &self.craftable } else { &self.fitting }
    }
}

/// The book's page for one tab: the known collections of the tab's
/// categories with something fitting the menu, matching `query`
/// (`RecipeBookComponent.updateCollections`), and craftable only while
/// filtering. `grid` is the crafting grid's size, `None` for furnaces.
pub fn page(
    book: &RecipeBook,
    book_type: BookType,
    tab: Option<Category>,
    grid: Option<usize>,
    query: &str,
    available: &stacked::Stacked,
) -> Vec<Shown> {
    let categories: &[Category] = match tab {
        Some(ref c) => std::slice::from_ref(c),
        None => book_type.categories(),
    };
    let query = query.trim().to_lowercase();
    let mut out = Vec::new();
    // The search tab lists its categories in `SearchRecipeBookCategory` order.
    let all = collections(book);
    for &category in categories {
        for c in all.iter().filter(|c| c.category == category) {
            let fitting: Vec<RecipeId> = c
                .recipes
                .iter()
                .copied()
                .filter(|id| {
                    let r = id.recipe().expect("collections hold shown recipes");
                    match (grid, book_type.cooking()) {
                        (Some(size), None) => r.fits(size),
                        (None, Some(kind)) => r.kind == kind,
                        _ => false,
                    }
                })
                .collect();
            if fitting.is_empty() {
                continue;
            }
            if !query.is_empty() && !c.recipes.iter().any(|id| matches_query(*id, &query)) {
                continue;
            }
            let craftable: Vec<RecipeId> = fitting
                .iter()
                .copied()
                .filter(|id| available.can_craft(&id.recipe().unwrap().ingredients, 1).is_some())
                .collect();
            out.push(Shown { collection: c.clone(), fitting, craftable });
        }
    }
    if book.settings[book_type.index()].filtering {
        out.retain(|s| !s.craftable.is_empty());
    }
    out
}

/// The recipe search: the result's name or Java ID contains every word
/// (Java's recipe search tree indexes result names and item IDs).
pub fn matches_query(id: RecipeId, query: &str) -> bool {
    let Some(recipe) = id.recipe() else { return false };
    let item = recipe.result.item;
    let name = recipe.result.display_name().to_lowercase();
    let java = table().items.iter().position(|&i| i == Some(item)).map(|i| data::NAMES[i].0).unwrap_or("");
    query.split_whitespace().all(|word| name.contains(word) || java.contains(word) || item.matches_query(word))
}

#[cfg(test)]
mod tests;
