//! Clicking a recipe in the book moves the player's items into the menu
//! (Java 26.3's `ServerPlaceRecipe` and `PlaceRecipeHelper`). Items only
//! move between the inventory and the menu's slots; nothing is created,
//! and the menu's own crafting (`Grid::result`) or cooking makes the result.
//!
//! * The menu's inputs must fit back into the inventory first, unless the
//!   player is in Creative (who may drop what doesn't fit).
//! * With the items for one craft, the grid is emptied into the inventory
//!   and refilled: one more set than the grid holds when it already shows
//!   this recipe, every set the items allow with shift (`useMaxItems`),
//!   and never more than any used item stacks to.
//! * Without them the grid is emptied and the book shows the ghost recipe.

use super::stacked::{Stacked, usable_for_crafting};
use super::{BookRecipe, Kind};
use crate::crafting::Grid;
use crate::inventory::{Inventory, SLOTS, Stack};
use crate::item::Item;
use crate::world::furnace::Furnace;

/// What a click did (`RecipeBookMenu.PostPlaceAction`, plus whether
/// anything moved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The ingredients are in place.
    Filled,
    /// Nothing moved: the inputs can't go back to the inventory, or the
    /// grid already holds as many sets as the items or stacks allow.
    Unchanged,
    /// The items are missing: the grid was emptied and the book shows the
    /// recipe as a ghost (`PLACE_GHOST_RECIPE`).
    Ghost,
}

/// The slots of one menu the book reads and fills.
struct Menu<'a> {
    /// Every menu slot whose items count as available (the grid; the
    /// furnace's input, fuel and output).
    slots: &'a mut [Option<Stack>],
    width: usize,
    height: usize,
    /// The input grid's slots, row-major.
    inputs: &'a [usize],
    /// Slots emptied before placing (`slotsToClear`).
    clear: &'a [usize],
    /// Furnace contents count whatever their wear or name (`accountStack`).
    furnace: bool,
}

/// Fills a crafting grid with `recipe` from `inventory`. `selected` is the
/// hotbar slot in hand, where returned items go first.
pub fn place_in_grid(
    recipe: &BookRecipe,
    grid: &mut Grid,
    inventory: &mut Inventory,
    selected: usize,
    max: bool,
    creative: bool,
) -> Outcome {
    if !recipe.kind.is_crafting() || !recipe.fits(grid.size) {
        return Outcome::Unchanged;
    }
    let matches = recipe.matches_grid(grid);
    let n = grid.size;
    let all: Vec<usize> = (0..n * n).collect();
    let mut menu = Menu { slots: &mut grid.cells[..n * n], width: n, height: n, inputs: &all, clear: &all, furnace: false };
    place(recipe, &mut menu, inventory, selected, max, creative, matches)
}

/// Fills a furnace's input with `recipe`'s ingredient
/// (`AbstractFurnaceMenu.handlePlacement`): the input and output are
/// emptied into the inventory first, the fuel is left alone.
pub fn place_in_furnace(
    recipe: &BookRecipe,
    furnace: &mut Furnace,
    inventory: &mut Inventory,
    selected: usize,
    max: bool,
    creative: bool,
) -> Outcome {
    let kind = match furnace.kind {
        crate::world::furnace::FurnaceKind::Normal => Kind::Smelting,
        crate::world::furnace::FurnaceKind::Blast => Kind::Blasting,
        crate::world::furnace::FurnaceKind::Smoker => Kind::Smoking,
    };
    if recipe.kind != kind {
        return Outcome::Unchanged;
    }
    let matches = recipe.matches_input(furnace.input);
    let mut slots = [furnace.input, furnace.fuel, furnace.output];
    let mut menu = Menu { slots: &mut slots, width: 1, height: 1, inputs: &[0], clear: &[0, 2], furnace: true };
    let outcome = place(recipe, &mut menu, inventory, selected, max, creative, matches);
    if outcome != Outcome::Unchanged {
        // The input was emptied and refilled: `setItem` restarts cooking.
        furnace.set_input(None);
        furnace.set_input(slots[0]);
        furnace.output = slots[2];
    }
    outcome
}

fn place(
    recipe: &BookRecipe,
    menu: &mut Menu,
    inventory: &mut Inventory,
    selected: usize,
    max: bool,
    creative: bool,
    matches: bool,
) -> Outcome {
    if !creative && !test_clear(menu, inventory, selected) {
        return Outcome::Unchanged;
    }
    let mut available = Stacked::of_inventory(inventory);
    for stack in menu.slots.iter().flatten() {
        if menu.furnace {
            available.account(stack);
        } else {
            available.account_simple(stack);
        }
    }
    if available.can_craft(&recipe.ingredients, 1).is_none() {
        clear(menu, inventory, selected);
        return Outcome::Ghost;
    }
    let biggest = available.biggest(&recipe.ingredients, u32::MAX);
    let inputs: Vec<Stack> = menu.inputs.iter().filter_map(|&i| menu.slots[i]).collect();
    if matches && inputs.iter().any(|s| biggest.min(u32::from(s.max())) < u32::from(s.count) + 1) {
        return Outcome::Unchanged;
    }
    let amount = if max {
        biggest
    } else if matches {
        inputs.iter().map(|s| u32::from(s.count)).min().map_or(u32::MAX, |n| n + 1)
    } else {
        1
    };
    let Some(mut used) = available.can_craft(&recipe.ingredients, amount) else { return Outcome::Unchanged };
    let adjusted = used.iter().fold(amount, |a, item| a.min(u32::from(item.max_stack())));
    if adjusted != amount {
        match available.can_craft(&recipe.ingredients, adjusted) {
            Some(again) => used = again,
            None => return Outcome::Unchanged,
        }
    }
    clear(menu, inventory, selected);
    let (w, h) = if recipe.kind == Kind::Shaped { (recipe.width, recipe.height) } else { (menu.width, menu.height) };
    let entries: Vec<Option<usize>> = if menu.furnace { vec![Some(0)] } else { recipe.slots.clone() };
    let mut placements = Vec::new();
    arrange(menu.width, menu.height, w, h, &entries, |ingredient, cell| {
        if let Some(i) = ingredient {
            placements.push((i, cell));
        }
    });
    for (ingredient, cell) in placements {
        let target = &mut menu.slots[menu.inputs[cell]];
        let mut remaining = adjusted;
        while remaining > 0 {
            match move_to_grid(inventory, target, used[ingredient], remaining) {
                Some(left) => remaining = left,
                None => break,
            }
        }
    }
    Outcome::Filled
}

/// `PlaceRecipeHelper.placeRecipe`: visits the grid cells the recipe's
/// entries go to, centring patterns less than half the grid's size.
pub fn arrange(
    grid_w: usize,
    grid_h: usize,
    recipe_w: usize,
    recipe_h: usize,
    entries: &[Option<usize>],
    mut out: impl FnMut(Option<usize>, usize),
) {
    let mut entries = entries.iter().copied().peekable();
    let mut index = 0;
    let mut y = 0;
    while y < grid_h {
        let start_y = (grid_h as f32 / 2.0 - recipe_h as f32 / 2.0).floor() as i64;
        if (recipe_h as f32) < grid_h as f32 / 2.0 && start_y > y as i64 {
            index += grid_w;
            y += 1;
        }
        for x in 0..grid_w {
            if entries.peek().is_none() {
                return;
            }
            let start_x = (grid_w as f32 / 2.0 - recipe_w as f32 / 2.0).floor() as i64;
            let (mut total, mut add) = (recipe_w as i64, x < recipe_w);
            if (recipe_w as f32) < grid_w as f32 / 2.0 {
                total = start_x + recipe_w as i64;
                add = start_x <= x as i64 && (x as i64) < start_x + recipe_w as i64;
            }
            if add {
                out(entries.next().unwrap(), index);
            } else if total == x as i64 {
                index += grid_w - x;
                break;
            }
            index += 1;
        }
        y += 1;
    }
}

/// `moveItemToGrid`: takes up to `count` of `item` from the first usable
/// main inventory stack that can join `target`. The count still to move,
/// or `None` when no stack is left.
fn move_to_grid(inventory: &mut Inventory, target: &mut Option<Stack>, item: Item, count: u32) -> Option<u32> {
    let slot = (0..SLOTS).find(|&i| {
        inventory.slots[i].is_some_and(|s| {
            s.item == item && usable_for_crafting(&s) && target.is_none_or(|t| t.stacks_with(&s))
        })
    })?;
    let stack = inventory.slots[slot]?;
    let taken = count.min(u32::from(stack.count)) as u8;
    inventory.slots[slot] = stack.with_count(stack.count - taken);
    match target {
        Some(t) => t.count += taken,
        None => *target = Some(Stack { count: taken, ..stack }),
    }
    Some(count - u32::from(taken))
}

/// `clearGrid`: every slot to clear goes back to the inventory; what
/// doesn't fit spills (the adapter drops it).
fn clear(menu: &mut Menu, inventory: &mut Inventory, selected: usize) {
    for &i in menu.clear {
        if let Some(stack) = menu.slots[i].take() {
            place_back(inventory, stack, selected);
        }
    }
}

/// Where a stack can join an existing one (`getSlotWithRemainingSpace`):
/// the selected slot, the off-hand (`None`), then the main slots in order.
fn slot_with_space(inventory: &Inventory, stack: &Stack, selected: usize) -> Option<Option<usize>> {
    let room = |dest: &Option<Stack>| dest.is_some_and(|d| d.stacks_with(stack) && d.max() > 1 && d.count < d.max());
    if room(&inventory.slots[selected]) {
        Some(Some(selected))
    } else if room(&inventory.offhand) {
        Some(None)
    } else {
        (0..SLOTS).find(|&i| room(&inventory.slots[i])).map(Some)
    }
}

/// `placeItemBackInInventory`: onto matching stacks (the hand first), then
/// into empty main slots, spilling the rest.
pub fn place_back(inventory: &mut Inventory, mut stack: Stack, selected: usize) {
    while stack.count > 0 {
        let dest = match slot_with_space(inventory, &stack, selected) {
            Some(Some(i)) => &mut inventory.slots[i],
            Some(None) => &mut inventory.offhand,
            None => match inventory.slots.iter().position(Option::is_none) {
                Some(i) => &mut inventory.slots[i],
                None => {
                    inventory.spill_stack(stack);
                    return;
                }
            },
        };
        let held = dest.map_or(0, |d| d.count);
        let n = stack.count.min(stack.max().saturating_sub(held));
        *dest = Some(Stack { count: held + n, ..stack });
        stack.count -= n;
    }
}

/// `testClearGrid`: whether every input stack has somewhere to go.
fn test_clear(menu: &Menu, inventory: &Inventory, selected: usize) -> bool {
    let mut pending: Vec<Stack> = Vec::new();
    let free = inventory.slots.iter().filter(|s| s.is_none()).count();
    for &i in menu.inputs {
        let Some(mut stack) = menu.slots[i] else { continue };
        let joins = slot_with_space(inventory, &stack, selected).is_some();
        if !joins && pending.len() <= free {
            for other in pending.iter_mut() {
                if other.item == stack.item && other.count != other.max() && other.count + stack.count <= other.max() {
                    other.count += stack.count;
                    stack.count = 0;
                    break;
                }
            }
            if stack.count > 0 {
                if pending.len() >= free {
                    return false;
                }
                pending.push(stack);
            }
        } else if !joins {
            return false;
        }
    }
    true
}
