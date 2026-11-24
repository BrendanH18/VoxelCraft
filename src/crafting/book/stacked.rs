//! What a player could craft from their items (Java's
//! `StackedItemContents`): counts per item type, and whether each of a
//! recipe's ingredients can take `amount` items of one type at once.
//!
//! Java assigns one item type to each ingredient for the whole craft, and
//! a type with `n` items can serve `n / amount` ingredients; that is a
//! bipartite matching with capacities, solved here with augmenting paths.
//! Java's `RecipePicker` tries item types in hash order; this tries them in
//! the order they were counted (hotbar first), so when several types fit
//! an ingredient the choice can differ, never whether the recipe fits.

use crate::inventory::{Inventory, SLOTS, Stack};
use crate::item::Item;

/// Item counts in the order first seen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stacked {
    amounts: Vec<(Item, u32)>,
}

/// `Inventory.isUsableForCrafting`: undamaged, unenchanted and unnamed.
pub fn usable_for_crafting(stack: &Stack) -> bool {
    stack.damage == 0 && stack.enchants.is_empty() && stack.name.as_str().is_none()
}

impl Stacked {
    /// The player's 36 inventory slots (`Inventory.fillStackedContents`).
    pub fn of_inventory(inventory: &Inventory) -> Self {
        let mut out = Self::default();
        for stack in inventory.slots[..SLOTS].iter().flatten() {
            out.account_simple(stack);
        }
        out
    }

    /// `accountSimpleStack`: only stacks usable for crafting.
    pub fn account_simple(&mut self, stack: &Stack) {
        if usable_for_crafting(stack) {
            self.account(stack);
        }
    }

    /// `accountStack`: any stack (a furnace's contents).
    pub fn account(&mut self, stack: &Stack) {
        let count = u32::from(stack.count.min(stack.max().max(1)));
        if count == 0 {
            return;
        }
        match self.amounts.iter_mut().find(|(i, _)| *i == stack.item) {
            Some((_, n)) => *n += count,
            None => self.amounts.push((stack.item, count)),
        }
    }

    pub fn count(&self, item: Item) -> u32 {
        self.amounts.iter().find(|(i, _)| *i == item).map_or(0, |&(_, n)| n)
    }

    /// The item type each ingredient takes when crafting `amount` at once
    /// (`canCraft` with its output), or `None` if the items don't suffice.
    pub fn can_craft(&self, ingredients: &[Vec<Item>], amount: u32) -> Option<Vec<Item>> {
        if ingredients.is_empty() {
            return None;
        }
        if amount == 0 {
            return Some(Vec::new());
        }
        // Item types any ingredient accepts, with how many ingredients each can serve.
        let types: Vec<(Item, u32)> = self
            .amounts
            .iter()
            .filter(|(item, n)| *n >= amount && ingredients.iter().any(|options| options.contains(item)))
            .map(|&(item, n)| (item, n / amount))
            .collect();
        let mut assigned: Vec<Option<usize>> = vec![None; ingredients.len()];
        let mut used: Vec<u32> = vec![0; types.len()];
        for ingredient in 0..ingredients.len() {
            let mut visited = vec![false; types.len()];
            if !augment(ingredient, ingredients, &types, &mut assigned, &mut used, &mut visited) {
                return None;
            }
        }
        Some(assigned.into_iter().map(|t| types[t.unwrap()].0).collect())
    }

    /// The most crafts the items allow at once, up to `max`
    /// (`getBiggestCraftableStack`, a binary search like Java's).
    pub fn biggest(&self, ingredients: &[Vec<Item>], max: u32) -> u32 {
        // `getResultUpperBound`: no ingredient can take more than its best type holds.
        let upper = ingredients
            .iter()
            .map(|options| self.amounts.iter().filter(|(i, _)| options.contains(i)).map(|&(_, n)| n).max().unwrap_or(0))
            .min()
            .unwrap_or(0)
            .min(max);
        let (mut low, mut high) = (0, upper + 1);
        while high - low > 1 {
            let mid = (low + high) / 2;
            if self.can_craft(ingredients, mid).is_some() {
                low = mid;
            } else {
                high = mid;
            }
        }
        low
    }
}

/// Kuhn's augmenting path for one ingredient over item types with capacity.
fn augment(
    ingredient: usize,
    ingredients: &[Vec<Item>],
    types: &[(Item, u32)],
    assigned: &mut [Option<usize>],
    used: &mut [u32],
    visited: &mut [bool],
) -> bool {
    for (t, &(item, capacity)) in types.iter().enumerate() {
        if visited[t] || !ingredients[ingredient].contains(&item) {
            continue;
        }
        visited[t] = true;
        if used[t] < capacity {
            used[t] += 1;
            assigned[ingredient] = Some(t);
            return true;
        }
        // Move one ingredient this type serves to another type.
        for other in 0..assigned.len() {
            if assigned[other] == Some(t) && augment(other, ingredients, types, assigned, used, visited) {
                // `other` moved off `t` (its new type counted it), and
                // this ingredient takes its place: `used[t]` is unchanged.
                assigned[ingredient] = Some(t);
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stacked(items: &[(Item, u8)]) -> Stacked {
        let mut s = Stacked::default();
        for &(item, n) in items {
            s.account_simple(&Stack::new(item, n));
        }
        s
    }

    #[test]
    fn each_ingredient_takes_one_item_type() {
        let (a, b) = (Item::STICK, Item::COAL);
        let s = stacked(&[(a, 3), (b, 2)]);
        // Two ingredients of sticks need 2 * amount sticks.
        assert_eq!(s.can_craft(&[vec![a], vec![a]], 1), Some(vec![a, a]));
        assert_eq!(s.can_craft(&[vec![a], vec![a]], 2), None);
        assert_eq!(s.biggest(&[vec![a], vec![a]], 64), 1);
        assert_eq!(s.biggest(&[vec![a], vec![b]], 64), 2);
        // An ingredient that accepts either moves to the free type.
        assert_eq!(s.can_craft(&[vec![a, b], vec![a]], 2), Some(vec![b, a]));
        assert_eq!(s.can_craft(&[vec![a]], 4), None);
    }

    #[test]
    fn damaged_enchanted_and_named_stacks_are_not_counted() {
        let mut damaged = Stack::new(Item::STICK, 1);
        damaged.damage = 3;
        let named = Stack::new(Item::STICK, 1).with_name("Wand").unwrap();
        let mut s = Stacked::default();
        s.account_simple(&damaged);
        s.account_simple(&named);
        assert_eq!(s.count(Item::STICK), 0);
        s.account(&damaged);
        assert_eq!(s.count(Item::STICK), 1);
    }
}
