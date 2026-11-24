//! v0.18 Track 5: the recipe book against Java 26.3
//! (`fixtures/java/26.3/recipe_book.json`, extracted by
//! `tools/extract_java_recipe_book_fixtures.py`): unlock criteria,
//! categories, tabs and labels, the search and craftable filter, grid and
//! furnace placement (`ServerPlaceRecipe`), `limited_crafting`, saves and
//! the migration of players saved before the book, and the agent adapter
//! that controller and LAN players also run on.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    agent::{Agent, Command},
    crafting::{
        Grid,
        book::{
            self, BookType, Category, RecipeBook, RecipeId,
            place::{self, Outcome},
            stacked::Stacked,
        },
    },
    entity::Entities,
    inventory::{Inventory, Stack},
    item::Item,
    rules::{GameMode, GameRules},
    simulation::difficulty::Difficulty,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        furnace::{Furnace, FurnaceKind},
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/recipe_book.json")).unwrap()
}

fn id(name: &str) -> RecipeId {
    RecipeId::from_name(name).unwrap_or_else(|| panic!("no recipe {name}"))
}

fn item(name: &str) -> Item {
    Item::from_name(name).unwrap_or_else(|| panic!("no item {name}"))
}

fn knowing(names: &[&str]) -> RecipeBook {
    let mut book = RecipeBook::default();
    for name in names {
        book.unlock(id(name));
    }
    book.take_toasts();
    book
}

/// A book after one tick with `inventory`.
fn ticked(inventory: &Inventory, in_water: bool) -> BTreeSet<&'static str> {
    let mut book = RecipeBook::default();
    book.tick(inventory, in_water);
    book.known().map(RecipeId::name).collect()
}

// ------------------------------------------------------------ Java's facts

#[test]
fn tabs_labels_settings_and_constants_match_java() {
    let f = fixture();
    let categories: Vec<&str> = f["categories"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert_eq!(Category::ALL.map(Category::id).to_vec(), categories);
    let labels = &f["labels"];
    let names = [
        (BookType::Crafting, "crafting"),
        (BookType::Furnace, "furnace"),
        (BookType::BlastFurnace, "blast_furnace"),
        (BookType::Smoker, "smoker"),
    ];
    for (book, name) in names {
        let search: Vec<&str> = book.categories().iter().map(|c| c.id()).collect();
        let want: Vec<&str> = f["search_tabs"][name].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
        assert_eq!(search, want, "{name} search tab");
        let tabs: Vec<(Option<&str>, Vec<&str>)> =
            book.tabs().iter().map(|t| (t.category.map(Category::id), t.icons.to_vec())).collect();
        let want: Vec<(Option<&str>, Vec<&str>)> = f["tabs"][name]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (t["category"].as_str(), t["icons"].as_array().unwrap().iter().map(|i| i.as_str().unwrap()).collect())
            })
            .collect();
        assert_eq!(tabs, want, "{name} tabs");
        for tab in book.tabs() {
            for icon in tab.icons {
                assert!(book::java_item(icon).is_some(), "{name} tab icon {icon} is registered");
            }
        }
        let key = f["filter_labels"][name].as_str().unwrap();
        assert_eq!(book.filter_label(), labels[key].as_str().unwrap(), "{name} filter label");
        let keys = book.settings_keys();
        assert_eq!(keys.0, f["settings_keys"][name]["open"].as_str().unwrap());
        assert_eq!(keys.1, f["settings_keys"][name]["filtering"].as_str().unwrap());
    }
    for (ours, key) in [
        (book::SHOWING_ALL, "gui.recipebook.toggleRecipes.all"),
        (book::SEARCH_HINT, "gui.recipebook.search_hint"),
        (book::MORE_RECIPES, "gui.recipebook.moreRecipes"),
        (book::NEXT_PAGE, "gui.recipebook.next_page"),
        (book::PREVIOUS_PAGE, "gui.recipebook.previous_page"),
        (book::TOAST_TITLE, "recipe.toast.title"),
        (book::TOAST_DESCRIPTION, "recipe.toast.description"),
    ] {
        assert_eq!(ours, labels[key].as_str().unwrap(), "{key}");
    }
    let c = &f["constants"];
    assert_eq!(book::IMAGE_WIDTH, c["RecipeBookComponent"]["IMAGE_WIDTH"].as_f64().unwrap() as f32);
    assert_eq!(book::IMAGE_HEIGHT, c["RecipeBookComponent"]["IMAGE_HEIGHT"].as_f64().unwrap() as f32);
    assert_eq!(book::OFFSET_X, c["RecipeBookComponent"]["OFFSET_X_POSITION"].as_f64().unwrap() as f32);
    assert_eq!(book::TICKS_TO_SWAP as u64, c["RecipeBookComponent"]["TICKS_TO_SWAP_SLOT"].as_u64().unwrap());
    assert_eq!(book::PER_PAGE as u64, c["RecipeBookPage"]["ITEMS_PER_PAGE"].as_u64().unwrap());
    assert_eq!(book::BUTTON_SIZE, c["RecipeButton"]["BACKGROUND_SIZE"].as_f64().unwrap() as f32);
    assert_eq!(book::HIGHLIGHT_TICKS, c["RecipeButton"]["ANIMATION_TIME"].as_f64().unwrap() as f32);
    assert_eq!(book::TOAST_MILLIS, c["RecipeToast"]["DISPLAY_TIME"].as_u64().unwrap());
}

#[test]
fn every_learnable_recipe_has_javas_id_category_group_and_notification() {
    let f = fixture();
    let learnable = f["learnable"].as_object().unwrap();
    assert_eq!(learnable.len(), RecipeId::count());
    for (name, fact) in learnable {
        let raw = id(name).raw();
        assert_eq!(raw.category.id(), fact["category"].as_str().unwrap(), "{name}");
        assert_eq!(raw.group, fact["group"].as_str(), "{name}");
        assert_eq!(raw.notify, fact["notify"].as_bool().unwrap_or(true), "{name}");
    }
    for name in f["special"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        assert!(book::is_special(name) && RecipeId::from_name(name).is_none(), "{name}");
    }
    // The book shows only what the game makes, and every crafting and
    // furnace recipe the game makes is in it.
    let shown: BTreeSet<&str> = book::recipes().iter().map(|r| r.id.name()).collect();
    for r in voxelcraft::crafting::recipes() {
        assert!(
            book::recipes().iter().any(|b| b.result.item == r.result.item),
            "{} has no book recipe",
            r.result.item.name()
        );
    }
    assert!(shown.len() > 1000, "{} recipes shown", shown.len());
}

/// Every item criterion against the fixture: an item entering the
/// inventory unlocks exactly the recipes whose advancement names it (or a
/// tag holding it), plus the `tick` ones.
#[test]
fn item_criteria_unlock_exactly_the_recipes_that_name_the_item() {
    let f = fixture();
    let tags = &f["item_tags"];
    let mut by_item: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut ticking = BTreeSet::new();
    for (name, unlock) in f["unlocks"].as_object().unwrap() {
        for entry in unlock["items"].as_array().into_iter().flatten() {
            let entry = entry.as_str().unwrap();
            let items: Vec<String> = match entry.strip_prefix('#') {
                Some(tag) => tags[tag].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
                None => vec![entry.to_string()],
            };
            for i in items {
                by_item.entry(i).or_default().insert(name.as_str());
            }
        }
        if unlock["tick"] == true {
            ticking.insert(name.as_str());
        }
    }
    assert_eq!(ticked(&Inventory::default(), false), ticking, "the first tick");
    assert_eq!(ticking, BTreeSet::from(["crafting_table"]));
    let mut checked = 0;
    // Several Java names can resolve to one VoxelCraft item; group them.
    let mut by_resolved: BTreeMap<u16, BTreeSet<&str>> = BTreeMap::new();
    for (java, recipes) in &by_item {
        if let Some(i) = book::java_item(java) {
            by_resolved.entry(i.0).or_default().extend(recipes.iter().copied());
        }
    }
    for (raw, recipes) in by_resolved {
        let mut inventory = Inventory::default();
        inventory.slots[5] = Some(Stack::new(Item(raw), 1));
        let mut want = recipes.clone();
        want.extend(ticking.iter().copied());
        assert_eq!(ticked(&inventory, false), want, "holding {}", Item(raw).name());
        checked += 1;
    }
    assert!(checked > 400, "only {checked} items checked");
}

#[test]
fn chest_needs_ten_occupied_slots_and_boats_need_water() {
    let mut inventory = Inventory::default();
    for slot in inventory.slots.iter_mut().take(9) {
        *slot = Some(Stack::new(Item::from_block(Block::DIRT), 1));
    }
    assert!(!ticked(&inventory, false).contains("chest"));
    // Armor and the off-hand count as inventory slots, as in Java.
    inventory.offhand = Some(Stack::new(Item::from_block(Block::DIRT), 1));
    assert!(ticked(&inventory, false).contains("chest"));
    let wet = ticked(&Inventory::default(), true);
    for boat in ["oak_boat", "spruce_boat", "bamboo_raft"] {
        assert!(wet.contains(boat), "{boat}");
    }
    assert!(!ticked(&Inventory::default(), false).contains("oak_boat"));
}

#[test]
fn slots_trigger_on_change_and_crafting_or_smelting_teaches_the_recipe() {
    let mut book = RecipeBook::default();
    let mut inventory = Inventory::default();
    book.tick(&inventory, false);
    assert!(!book.knows(id("oak_planks")));
    // Picking up a log later unlocks the planks, with a toast.
    book.take_toasts();
    inventory.slots[3] = Some(Stack::new(item("oak log"), 2));
    book.tick(&inventory, false);
    assert!(book.knows(id("oak_planks")) && book.highlighted(id("oak_planks")));
    assert_eq!(book.take_toasts(), vec![id("oak_planks")]);
    // The crafting table's recipe doesn't notify.
    let mut fresh = RecipeBook::default();
    fresh.tick(&Inventory::default(), false);
    assert!(fresh.knows(id("crafting_table")) && fresh.take_toasts().is_empty());
    // Crafting a recipe teaches it, special recipes teach nothing.
    let mut grid = Grid::new(2);
    grid.cells[0] = Some(Stack::new(Item::PAPER, 1));
    grid.cells[1] = Some(Stack::new(Item::GUNPOWDER, 1));
    book.crafted(&grid);
    assert!(book.knows(id("firework_rocket_simple")));
    let before = book.known_count();
    grid.cells[2] = Some(Stack::new(Item::GUNPOWDER, 1));
    book.crafted(&grid);
    assert_eq!(book.known_count(), before);
    // Taking from a furnace teaches the recipes its tally holds.
    book.cooked(FurnaceKind::Normal, &[item("raw iron")]);
    assert!(book.knows(id("iron_ingot_from_smelting_raw_iron")));
    assert!(!book.knows(id("iron_ingot_from_blasting_raw_iron")));
    book.cooked(FurnaceKind::Blast, &[item("raw iron")]);
    assert!(book.knows(id("iron_ingot_from_blasting_raw_iron")));
}

// ---------------------------------------------------------- pages and filters

#[test]
fn pages_show_known_recipes_that_fit_by_tab_search_and_filter() {
    let mut book = knowing(&["oak_planks", "stick", "crafting_table", "wooden_pickaxe", "chest", "torch"]);
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item("oak planks"), 8));
    let available = Stacked::of_inventory(&inventory);
    let names = |page: &[book::Shown]| -> Vec<&'static str> {
        page.iter().flat_map(|s| s.collection.recipes.iter().map(|r| r.name())).collect()
    };
    // The 2x2 grid hides 3x3 recipes entirely.
    let small = book::page(&book, BookType::Crafting, None, Some(2), "", &available);
    let small_names = names(&small);
    assert!(small_names.contains(&"stick") && small_names.contains(&"crafting_table"));
    assert!(!small_names.contains(&"wooden_pickaxe") && !small_names.contains(&"chest"));
    let big = names(&book::page(&book, BookType::Crafting, None, Some(3), "", &available));
    assert!(big.contains(&"wooden_pickaxe") && big.contains(&"chest"));
    // Tabs hold their own category; the search tab lists equipment first.
    let equipment =
        names(&book::page(&book, BookType::Crafting, Some(Category::CraftingEquipment), Some(3), "", &available));
    assert_eq!(equipment, vec!["wooden_pickaxe"]);
    assert_eq!(big.first(), Some(&"wooden_pickaxe"));
    // Search matches result names.
    let found = names(&book::page(&book, BookType::Crafting, None, Some(3), "pick", &available));
    assert_eq!(found, vec!["wooden_pickaxe"]);
    // Craftable marks and the filter: eight planks make sticks, a table and a chest.
    let all = book::page(&book, BookType::Crafting, None, Some(3), "", &available);
    let craftable: BTreeSet<&str> = all.iter().flat_map(|s| s.craftable.iter().map(|r| r.name())).collect();
    assert_eq!(craftable, BTreeSet::from(["stick", "crafting_table", "chest"]));
    book.set_filtering(BookType::Crafting, true);
    let filtered: BTreeSet<&str> =
        names(&book::page(&book, BookType::Crafting, None, Some(3), "", &available)).into_iter().collect();
    assert_eq!(filtered, craftable);
    // Furnace books show their own cooking type only.
    let smelt = knowing(&["iron_ingot_from_smelting_raw_iron", "iron_ingot_from_blasting_raw_iron"]);
    let furnace = names(&book::page(&smelt, BookType::Furnace, None, None, "", &available));
    let blast = names(&book::page(&smelt, BookType::BlastFurnace, None, None, "", &available));
    assert_eq!(
        (furnace, blast),
        (vec!["iron_ingot_from_smelting_raw_iron"], vec!["iron_ingot_from_blasting_raw_iron"])
    );
}

#[test]
fn grouped_recipes_share_one_button() {
    let book = knowing(&["oak_planks", "birch_planks", "stick"]);
    let page = book::page(&book, BookType::Crafting, None, Some(2), "", &Stacked::default());
    let planks = page.iter().find(|s| s.collection.recipes.contains(&id("oak_planks"))).unwrap();
    assert_eq!(planks.collection.recipes, vec![id("birch_planks"), id("oak_planks")]);
    assert!(page.iter().any(|s| s.collection.recipes == vec![id("stick")]));
}

// ------------------------------------------------------------ placement

fn recipe(name: &str) -> &'static book::BookRecipe {
    id(name).recipe().unwrap()
}

#[test]
fn a_click_moves_one_set_and_again_adds_one_more() {
    let mut inventory = Inventory::default();
    inventory.slots[4] = Some(Stack::new(item("oak log"), 5));
    let mut grid = Grid::new(2);
    let planks = recipe("oak_planks");
    assert_eq!(place::place_in_grid(planks, &mut grid, &mut inventory, 0, false, false), Outcome::Filled);
    assert_eq!(grid.cells.iter().flatten().map(|s| s.count).sum::<u8>(), 1);
    assert_eq!(inventory.slots[4].unwrap().count, 4);
    // The shared grid crafts it.
    assert_eq!(grid.result(), Some(Stack::new(item("oak planks"), 4)));
    // Clicking the shown recipe again adds one more set.
    assert_eq!(place::place_in_grid(planks, &mut grid, &mut inventory, 0, false, false), Outcome::Filled);
    assert_eq!(grid.cells.iter().flatten().map(|s| s.count).sum::<u8>(), 2);
    assert_eq!(inventory.slots[4].unwrap().count, 3);
    // Shift moves every set the items allow.
    assert_eq!(place::place_in_grid(planks, &mut grid, &mut inventory, 0, true, false), Outcome::Filled);
    assert_eq!(grid.cells.iter().flatten().map(|s| s.count).sum::<u8>(), 5);
    assert!(inventory.slots[4].is_none());
    // Nothing left to add: the grid stays as it is.
    assert_eq!(place::place_in_grid(planks, &mut grid, &mut inventory, 0, false, false), Outcome::Unchanged);
}

#[test]
fn shaped_recipes_take_from_several_stacks_and_respect_stack_limits() {
    // Sticks: two planks stacked vertically, centred in a 3x3 grid's middle column.
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item("oak planks"), 3));
    inventory.slots[9] = Some(Stack::new(item("oak planks"), 5));
    let mut grid = Grid::new(3);
    assert_eq!(place::place_in_grid(recipe("stick"), &mut grid, &mut inventory, 0, true, false), Outcome::Filled);
    assert_eq!(grid.cells[1].map(|s| s.count), Some(4));
    assert_eq!(grid.cells[4].map(|s| s.count), Some(4));
    assert!(inventory.slots[0].is_none() && inventory.slots[9].is_none());
    assert_eq!(grid.result().map(|s| s.item), Some(Item::STICK));
    // A cake's milk buckets stack to one, so shift places one set.
    let mut inventory = Inventory::default();
    for (slot, (name, count)) in [
        ("milk bucket", 1),
        ("milk bucket", 1),
        ("milk bucket", 1),
        ("milk bucket", 1),
        ("milk bucket", 1),
        ("milk bucket", 1),
        ("sugar", 10),
        ("egg", 5),
        ("wheat", 20),
    ]
    .into_iter()
    .enumerate()
    {
        inventory.slots[slot] = Some(Stack::new(item(name), count));
    }
    let mut grid = Grid::new(3);
    assert_eq!(place::place_in_grid(recipe("cake"), &mut grid, &mut inventory, 0, true, false), Outcome::Filled);
    assert!(grid.cells.iter().flatten().all(|s| s.count == 1), "{:?}", grid.cells);
    assert_eq!(grid.result().map(|s| s.item), Some(item("cake")));
}

#[test]
fn missing_items_empty_the_grid_and_show_the_ghost() {
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item("oak planks"), 1));
    let mut grid = Grid::new(2);
    grid.cells[3] = Some(Stack::new(Item::from_block(Block::DIRT), 2));
    assert_eq!(place::place_in_grid(recipe("stick"), &mut grid, &mut inventory, 0, false, false), Outcome::Ghost);
    assert!(grid.cells.iter().all(Option::is_none));
    assert_eq!(inventory.find(Item::from_block(Block::DIRT)).map(|i| inventory.slots[i].unwrap().count), Some(2));
    // Damaged, enchanted or named items are never used.
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item("oak log"), 1).with_name("Kindling").unwrap());
    let mut grid = Grid::new(2);
    assert_eq!(place::place_in_grid(recipe("oak_planks"), &mut grid, &mut inventory, 0, false, false), Outcome::Ghost);
    assert!(inventory.slots[0].is_some());
}

#[test]
fn a_grid_that_cannot_go_back_stays_unless_creative() {
    let mut inventory = Inventory::default();
    for slot in &mut inventory.slots {
        *slot = Some(Stack::new(Item::from_block(Block::STONE), 64));
    }
    inventory.slots[35] = Some(Stack::new(item("oak log"), 3));
    let mut grid = Grid::new(2);
    grid.cells[0] = Some(Stack::new(Item::from_block(Block::DIRT), 1));
    let planks = recipe("oak_planks");
    assert_eq!(place::place_in_grid(planks, &mut grid, &mut inventory, 0, false, false), Outcome::Unchanged);
    assert_eq!(grid.cells[0].map(|s| s.item), Some(Item::from_block(Block::DIRT)));
    // Creative may drop what doesn't fit (it spills for the adapter to drop).
    assert_eq!(place::place_in_grid(planks, &mut grid, &mut inventory, 0, false, true), Outcome::Filled);
    assert_eq!(inventory.take_spill().len(), 1);
}

#[test]
fn furnace_books_fill_the_input_and_leave_the_fuel() {
    let mut furnace = Furnace {
        fuel: Some(Stack::new(Item::COAL, 3)),
        output: Some(Stack::new(Item::IRON_INGOT, 2)),
        ..Furnace::default()
    };
    furnace.cook = 50;
    let mut inventory = Inventory::default();
    inventory.slots[2] = Some(Stack::new(item("raw iron"), 10));
    let smelt = recipe("iron_ingot_from_smelting_raw_iron");
    assert_eq!(place::place_in_furnace(smelt, &mut furnace, &mut inventory, 0, false, false), Outcome::Filled);
    assert_eq!(furnace.input, Some(Stack::new(item("raw iron"), 1)));
    assert_eq!(furnace.fuel, Some(Stack::new(Item::COAL, 3)));
    // The output went back to the inventory; cooking restarts.
    assert!(furnace.output.is_none() && furnace.cook == 0);
    assert_eq!(inventory.find(Item::IRON_INGOT).map(|i| inventory.slots[i].unwrap().count), Some(2));
    assert_eq!(place::place_in_furnace(smelt, &mut furnace, &mut inventory, 0, true, false), Outcome::Filled);
    assert_eq!(furnace.input.map(|s| s.count), Some(10));
    // A blasting recipe doesn't go in a plain furnace.
    let blast = recipe("iron_ingot_from_blasting_raw_iron");
    assert_eq!(place::place_in_furnace(blast, &mut furnace, &mut inventory, 0, false, false), Outcome::Unchanged);
}

// ------------------------------------------------------- limited_crafting

#[test]
fn limited_crafting_allows_only_known_recipes_and_specials() {
    let book = knowing(&["stick"]);
    let mut grid = Grid::new(2);
    grid.cells[0] = Some(Stack::new(item("oak planks"), 1));
    grid.cells[2] = Some(Stack::new(item("oak planks"), 1));
    assert!(book.allows(&grid, true) && book.allows(&grid, false));
    let mut planks = Grid::new(2);
    planks.cells[0] = Some(Stack::new(item("oak log"), 1));
    assert!(!book.allows(&planks, true) && book.allows(&planks, false));
    let mut rocket = Grid::new(2);
    rocket.cells[0] = Some(Stack::new(Item::PAPER, 1));
    rocket.cells[1] = Some(Stack::new(Item::GUNPOWDER, 1));
    rocket.cells[2] = Some(Stack::new(Item::GUNPOWDER, 1));
    assert!(book.allows(&rocket, true), "special recipes always craft");
}

/// A player saved before the book (no recipe data) under `limited_crafting`
/// can still start: logs unlock planks, planks unlock sticks, the first tick
/// the crafting table, and sticks the wooden pickaxe, all crafted through
/// the book's placement and the shared grid.
#[test]
fn a_migrated_player_can_start_from_logs_under_limited_crafting() {
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(item("oak log"), 4));
    let mut book = RecipeBook::deserialize("");
    assert_eq!(book.known_count(), 0);
    let craft = |book: &mut RecipeBook, inventory: &mut Inventory, name: &str, size: usize| {
        book.tick(inventory, false);
        assert!(book.knows(id(name)), "{name} unlocked");
        let mut grid = Grid::new(size);
        assert_eq!(place::place_in_grid(recipe(name), &mut grid, inventory, 0, true, false), Outcome::Filled, "{name}");
        while book.allows(&grid, true)
            && let Some(result) = grid.result()
        {
            assert_eq!(inventory.add_stack(result), 0);
            book.crafted(&grid);
            grid.consume();
        }
        for stack in grid.take_all() {
            inventory.add_stack(stack);
        }
    };
    craft(&mut book, &mut inventory, "oak_planks", 2);
    craft(&mut book, &mut inventory, "stick", 2);
    craft(&mut book, &mut inventory, "crafting_table", 2);
    craft(&mut book, &mut inventory, "wooden_pickaxe", 3);
    assert!(inventory.find(item("wooden pickaxe")).is_some());
    // No history was invented: nothing unrelated is known.
    assert!(!book.knows(id("iron_pickaxe")) && !book.knows(id("furnace")));
}

// ------------------------------------------------------------- saves

#[test]
fn the_book_saves_by_recipe_id_and_old_or_odd_data_loads() {
    let mut book = knowing(&["oak_planks", "stick", "iron_ingot_from_smelting_raw_iron"]);
    book.mark_seen(id("stick"));
    book.set_open(BookType::Crafting, true);
    book.set_filtering(BookType::Smoker, true);
    let text = book.serialize();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["isGuiOpen"], true);
    assert_eq!(v["isSmokerFilteringCraftable"], true);
    assert_eq!(v["isFurnaceGuiOpen"], false);
    assert!(v["recipes"].as_array().unwrap().contains(&Value::from("minecraft:oak_planks")));
    assert!(!v["toBeDisplayed"].as_array().unwrap().contains(&Value::from("minecraft:stick")));
    let back = RecipeBook::deserialize(&text);
    assert_eq!(back.serialize(), text);
    assert!(back.knows(id("stick")) && !back.highlighted(id("stick")) && back.highlighted(id("oak_planks")));
    // Unknown IDs drop, missing settings are Java's defaults, junk is a new book.
    let odd = RecipeBook::deserialize(r#"{"recipes":["minecraft:no_such_recipe","stick"],"isSmokerGuiOpen":true}"#);
    assert_eq!(odd.known().map(RecipeId::name).collect::<Vec<_>>(), vec!["stick"]);
    assert!(odd.settings(BookType::Smoker).open && !odd.settings(BookType::Crafting).open);
    assert_eq!(RecipeBook::deserialize("not json").known_count(), 0);
    // A legacy player's first tick checks the saved inventory.
    let mut inventory = Inventory::default();
    inventory.armor[0] = Some(Stack::new(item("iron boots"), 1));
    inventory.slots[20] = Some(Stack::new(Item::IRON_INGOT, 3));
    let mut legacy = RecipeBook::deserialize("");
    legacy.tick(&inventory, false);
    assert!(legacy.knows(id("iron_pickaxe")) && legacy.knows(id("crafting_table")));
    assert!(!legacy.knows(id("diamond_pickaxe")));
}

// ---------------------------------------------------- the agent adapter

fn world() -> World {
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
    for _ in 0..10_000 {
        world.update(DVec3::new(1.0, 150.0, 1.0));
        if world.is_loaded(IVec3::new(1, 150, 1)) {
            for x in -3..=5 {
                for z in -3..=5 {
                    world.set_block(IVec3::new(x, 149, z), Block::STONE);
                }
            }
            return world;
        }
        std::thread::yield_now();
    }
    panic!("world failed to load");
}

#[test]
fn agents_use_the_book_and_honour_limited_crafting() {
    let mut world = world();
    let mut entities = Entities::new(5);
    let mut rules = GameRules::default();
    rules.set("limited_crafting", "true").unwrap();
    let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    agent.set_mode(GameMode::Survival);
    agent.inventory.slots[0] = Some(Stack::new(item("oak log"), 3));
    agent.tick_rules(&mut world, &mut entities, Difficulty::Normal, &rules);
    assert!(agent.recipe_book.knows(id("oak_planks")) && agent.recipe_book.knows(id("crafting_table")));
    // `craft` refuses a recipe the player doesn't know under the rule.
    agent.inventory.slots[1] = Some(Stack::new(item("oak planks"), 2));
    let err = agent.execute(Command::Craft(Item::STICK), &mut world, &mut entities, &[]);
    assert!(err.is_err(), "sticks aren't known until planks enter the inventory slot");
    agent.tick_rules(&mut world, &mut entities, Difficulty::Normal, &rules);
    agent.execute(Command::Craft(Item::STICK), &mut world, &mut entities, &[]).unwrap();
    assert_eq!(agent.inventory.find(Item::STICK).map(|i| agent.inventory.slots[i].unwrap().count), Some(4));
    // `recipe <id> max` places every set through the book and takes them all.
    let cmd = Command::parse("recipe minecraft:oak_planks max").unwrap();
    agent.execute(cmd, &mut world, &mut entities, &[]).unwrap();
    assert!(agent.inventory.find(item("oak log")).is_none());
    assert_eq!(agent.inventory.find(item("oak planks")).map(|i| agent.inventory.slots[i].unwrap().count), Some(12));
    assert!(agent.execute(Command::parse("recipe iron_pickaxe").unwrap(), &mut world, &mut entities, &[]).is_err());
    // The listing marks craftable and new recipes; observe reports the book.
    let list = agent.recipe_list(&world, None, "");
    let entries = list["recipes"].as_array().unwrap();
    let planks = entries.iter().find(|e| e["id"] == "minecraft:oak_planks").unwrap();
    assert_eq!(planks["craftable"], false);
    let table = entries.iter().find(|e| e["id"] == "minecraft:crafting_table").unwrap();
    assert_eq!(table["craftable"], true);
    assert!(entries.iter().all(|e| e["id"] != "minecraft:wooden_pickaxe"), "3x3 recipes need a table");
    let only = agent.recipe_list(&world, Some(true), "");
    assert!(only["recipes"].as_array().unwrap().iter().all(|e| e["craftable"] == true));
    let observed = agent.observe(&world, 0);
    assert_eq!(observed["recipe_book"]["known"].as_u64().unwrap() as usize, agent.recipe_book.known_count());
    assert_eq!(observed["recipe_book"]["settings"]["isGuiOpen"], false);
}

#[test]
fn recipe_commands_parse() {
    assert!(matches!(Command::parse("recipe oak_planks"), Ok(Command::Recipe(r, false)) if r == id("oak_planks")));
    assert!(matches!(Command::parse("recipe minecraft:stick max"), Ok(Command::Recipe(r, true)) if r == id("stick")));
    assert!(Command::parse("recipe no_such_recipe").is_err());
    assert!(matches!(Command::parse("recipes craftable iron"), Ok(Command::Recipes(Some(true), q)) if q == "iron"));
    assert!(matches!(Command::parse("recipes"), Ok(Command::Recipes(None, q)) if q.is_empty()));
}

/// Headless costs, printed for the record (`--nocapture`): the per-tick
/// unlock check with an inventory that changes every tick, and building a
/// page with every recipe known. The bounds are loose; they only catch a
/// cost that grows with something it shouldn't.
#[test]
fn unlock_ticks_and_pages_stay_cheap() {
    use std::time::Instant;
    let mut inventory = Inventory::default();
    for (i, slot) in inventory.slots.iter_mut().enumerate() {
        *slot = Some(Stack::new(item("oak log"), 1 + i as u8));
    }
    let mut book = RecipeBook::default();
    book.tick(&inventory, false);
    let ticks = 20_000;
    let start = Instant::now();
    for t in 0..ticks {
        // One slot changes each tick, as picking things up does.
        inventory.slots[t % 36] = Some(Stack::new(item("oak log"), 1 + (t % 60) as u8));
        book.tick(&inventory, t % 2 == 0);
    }
    let per_tick = start.elapsed().as_secs_f64() * 1e9 / ticks as f64;
    let mut all = RecipeBook::default();
    for id in RecipeId::all() {
        all.unlock(id);
    }
    let available = Stacked::of_inventory(&inventory);
    let start = Instant::now();
    let rounds = 20;
    let mut shown = 0;
    for _ in 0..rounds {
        shown = book::page(&all, BookType::Crafting, None, Some(3), "", &available).len();
    }
    let per_page = start.elapsed().as_secs_f64() * 1e3 / rounds as f64;
    eprintln!("recipe book: unlock tick {per_tick:.0} ns; full crafting page ({shown} buttons) {per_page:.2} ms");
    assert!(per_tick < 100_000.0, "{per_tick} ns per tick");
    assert!(per_page < 250.0, "{per_page} ms per page");
}
