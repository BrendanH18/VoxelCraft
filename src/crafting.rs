//! Crafting: a 2x2 (inventory) or 3x3 (crafting table) grid of stacks and
//! the recipes that turn one of each ingredient into a result.
//!
//! Shaped recipes match wherever their pattern sits in the grid (the grid
//! is trimmed to the bounding box of its filled cells), and also mirrored
//! left to right, like Minecraft. Shapeless recipes match any arrangement
//! of exactly their ingredients.

use crate::inventory::Stack;
use crate::item::{Item, Tier, ToolKind};
use crate::world::block::Block;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grid {
    /// Edge length: 2 or 3.
    pub size: usize,
    /// Row-major; only the first `size * size` cells are used.
    pub cells: [Option<Stack>; 9],
}

impl Grid {
    pub fn new(size: usize) -> Self {
        assert!(size == 2 || size == 3);
        Self { size, cells: [None; 9] }
    }

    pub fn cell(&self, x: usize, y: usize) -> Option<Stack> {
        self.cells[y * self.size + x]
    }

    /// The items in the grid, trimmed to the filled bounding box: (width,
    /// height, row-major items).
    fn trimmed(&self) -> Option<(usize, usize, Vec<Option<Item>>)> {
        let n = self.size;
        let filled = |x: usize, y: usize| self.cell(x, y).is_some();
        let cols: Vec<usize> = (0..n).filter(|&x| (0..n).any(|y| filled(x, y))).collect();
        let rows: Vec<usize> = (0..n).filter(|&y| (0..n).any(|x| filled(x, y))).collect();
        let (x0, x1, y0, y1) = (*cols.first()?, *cols.last()?, *rows.first()?, *rows.last()?);
        let items =
            (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).map(|(x, y)| self.cell(x, y).map(|s| s.item));
        Some((x1 - x0 + 1, y1 - y0 + 1, items.collect()))
    }

    /// What the grid crafts right now.
    pub fn result(&self) -> Option<Stack> {
        let (w, h, items) = self.trimmed()?;
        recipes().iter().find(|r| r.matches(w, h, &items)).map(|r| r.result)
    }

    /// Uses up one of each ingredient (after taking the result).
    pub fn consume(&mut self) {
        for cell in self.cells.iter_mut().take(self.size * self.size) {
            if let Some(s) = cell {
                s.count -= 1;
                if s.count == 0 {
                    *cell = None;
                }
            }
        }
    }

    /// Empties the grid, returning its stacks.
    pub fn take_all(&mut self) -> Vec<Stack> {
        self.cells.iter_mut().filter_map(Option::take).collect()
    }
}

/// An ingredient slot: one of these items.
type Ingredient = &'static [Item];

enum Shape {
    /// Rows of the pattern; each char is a key into the ingredient list,
    /// space for empty.
    Shaped(&'static [&'static str], Vec<(char, Ingredient)>),
    Shapeless(Vec<Ingredient>),
}

pub struct Recipe {
    shape: Shape,
    pub result: Stack,
}

impl Recipe {
    /// A valid ingredient layout for the guide, using the first alternative
    /// for each slot. These previews are never placed in the player's grid.
    pub fn preview(&self) -> Grid {
        match &self.shape {
            Shape::Shaped(rows, key) => {
                let mut grid = Grid::new(if rows.len() > 2 || rows[0].len() > 2 { 3 } else { 2 });
                for (y, row) in rows.iter().enumerate() {
                    for (x, c) in row.chars().enumerate() {
                        grid.cells[y * grid.size + x] =
                            key.iter().find(|(k, _)| *k == c).map(|(_, options)| Stack::new(options[0], 1));
                    }
                }
                grid
            }
            Shape::Shapeless(ingredients) => {
                let mut grid = Grid::new(if ingredients.len() > 4 { 3 } else { 2 });
                for (cell, options) in grid.cells.iter_mut().zip(ingredients) {
                    *cell = Some(Stack::new(options[0], 1));
                }
                grid
            }
        }
    }

    /// Alternative ingredients in the currently displayed preview slot.
    pub fn alternatives(&self, slot: usize) -> Option<Ingredient> {
        match &self.shape {
            Shape::Shaped(rows, key) => {
                let size = if rows.len() > 2 || rows[0].len() > 2 { 3 } else { 2 };
                let c = rows.get(slot / size)?.chars().nth(slot % size)?;
                key.iter().find(|(k, _)| *k == c).map(|(_, options)| *options)
            }
            Shape::Shapeless(ingredients) => ingredients.get(slot).copied(),
        }
    }

    fn matches(&self, w: usize, h: usize, items: &[Option<Item>]) -> bool {
        match &self.shape {
            Shape::Shaped(rows, key) => {
                if rows.len() != h || rows[0].len() != w {
                    return false;
                }
                let fits = |mirror: bool| {
                    rows.iter().enumerate().all(|(y, row)| {
                        row.chars().enumerate().all(|(x, c)| {
                            let x = if mirror { w - 1 - x } else { x };
                            let want = key.iter().find(|(k, _)| *k == c).map(|(_, i)| *i);
                            match (want, items[y * w + x]) {
                                (None, None) => true,
                                (Some(options), Some(item)) => options.contains(&item),
                                _ => false,
                            }
                        })
                    })
                };
                fits(false) || fits(true)
            }
            Shape::Shapeless(needed) => {
                let mut have: Vec<Item> = items.iter().flatten().copied().collect();
                if have.len() != needed.len() {
                    return false;
                }
                needed.iter().all(|options| match have.iter().position(|i| options.contains(i)) {
                    Some(p) => {
                        have.swap_remove(p);
                        true
                    }
                    None => false,
                })
            }
        }
    }
}

const fn b(block: Block) -> Item {
    Item::from_block(block)
}

/// Any kind of planks (oak first, so guides show oak).
const PLANKS: Ingredient = &[
    b(Block::PLANKS),
    b(Block::SPRUCE_PLANKS),
    b(Block::BIRCH_PLANKS),
    b(Block::JUNGLE_PLANKS),
    b(Block::ACACIA_PLANKS),
];
const MELON: Ingredient = &[b(Block::MELON)];
const SAND: Ingredient = &[b(Block::SAND)];
const COBBLESTONE: Ingredient = &[b(Block::COBBLESTONE)];
const STICK: Ingredient = &[Item::STICK];
const FUEL_LUMP: Ingredient = &[Item::COAL, Item::CHARCOAL];

fn shaped(rows: &'static [&'static str], key: &[(char, Ingredient)], result: Item, count: u8) -> Recipe {
    Recipe { shape: Shape::Shaped(rows, key.to_vec()), result: Stack::new(result, count) }
}

fn shapeless(ingredients: &[Ingredient], result: Item, count: u8) -> Recipe {
    Recipe { shape: Shape::Shapeless(ingredients.to_vec()), result: Stack::new(result, count) }
}

/// Every recipe, built once.
pub fn recipes() -> &'static [Recipe] {
    static RECIPES: std::sync::OnceLock<Vec<Recipe>> = std::sync::OnceLock::new();
    RECIPES.get_or_init(|| {
        let mut r = vec![
            shaped(&["##", "##"], &[('#', &[Item::CLAY_BALL])], b(Block::CLAY), 1),
            shaped(&["##", "##"], &[('#', &[Item::BRICK])], b(Block::BRICKS), 1),
            shapeless(&[MELON], Item::MELON_SLICE, 9),
            shaped(&["#", "#"], &[('#', PLANKS)], Item::STICK, 4),
            shaped(&["##", "##"], &[('#', PLANKS)], b(Block::CRAFTING_TABLE), 1),
            shaped(&["###", "# #", "###"], &[('#', COBBLESTONE)], b(Block::FURNACE), 1),
            shaped(&["###", "# #", "###"], &[('#', PLANKS)], b(Block::CHEST), 1),
            shaped(&["###"], &[('#', &[Item::WHEAT])], Item::BREAD, 1),
            shapeless(&[&[Item::BONE]], Item::BONE_MEAL, 3),
            shaped(&["c", "#"], &[('c', FUEL_LUMP), ('#', STICK)], b(Block::TORCH), 4),
            shaped(&["##", "##"], &[('#', SAND)], b(Block::SANDSTONE), 1),
            shaped(&["##", "##"], &[('#', &[Item::STRING])], b(Block::WOOL), 1),
            shaped(&["f", "#", "e"], &[('f', &[Item::FLINT]), ('#', STICK), ('e', &[Item::FEATHER])], Item::ARROW, 4),
        ];
        const LOGS: [(Ingredient, Block); 5] = [
            (&[b(Block::LOG)], Block::PLANKS),
            (&[b(Block::SPRUCE_LOG)], Block::SPRUCE_PLANKS),
            (&[b(Block::BIRCH_LOG)], Block::BIRCH_PLANKS),
            (&[b(Block::JUNGLE_LOG)], Block::JUNGLE_PLANKS),
            (&[b(Block::ACACIA_LOG)], Block::ACACIA_PLANKS),
        ];
        // Planks first: the recipe guide lists them before everything else.
        for (i, (log, planks)) in LOGS.into_iter().enumerate() {
            r.insert(i, shapeless(&[log], b(planks), 4));
        }
        const MATERIALS: [(Tier, Ingredient); 5] = [
            (Tier::Wood, PLANKS),
            (Tier::Stone, COBBLESTONE),
            (Tier::Iron, &[Item::IRON_INGOT]),
            (Tier::Gold, &[Item::GOLD_INGOT]),
            (Tier::Diamond, &[Item::DIAMOND]),
        ];
        for (tier, m) in MATERIALS {
            let tool = |kind, rows| shaped(rows, &[('X', m), ('#', STICK)], Item::tool(kind, tier), 1);
            r.push(tool(ToolKind::Pickaxe, &["XXX", " # ", " # "]));
            r.push(tool(ToolKind::Shovel, &["X", "#", "#"]));
            r.push(tool(ToolKind::Axe, &["XX", "X#", " #"]));
            r.push(tool(ToolKind::Hoe, &["XX", " #", " #"]));
            r.push(tool(ToolKind::Sword, &["X", "X", "#"]));
        }
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(size: usize, cells: &[(usize, usize, Item)]) -> Grid {
        let mut g = Grid::new(size);
        for &(x, y, item) in cells {
            g.cells[y * size + x] = Some(Stack::new(item, 2));
        }
        g
    }

    const P: Item = b(Block::PLANKS);
    const C: Item = b(Block::COBBLESTONE);

    #[test]
    fn shaped_recipes_match_anywhere_and_mirrored() {
        // Sticks: two planks stacked, in any column.
        for x in 0..3 {
            let g = grid(3, &[(x, 1, P), (x, 2, P)]);
            assert_eq!(g.result(), Some(Stack::new(Item::STICK, 4)));
        }
        // Side by side is not sticks.
        assert_eq!(grid(3, &[(0, 0, P), (1, 0, P)]).result(), None);
        // An axe and its mirror image.
        let axe = Item::tool(ToolKind::Axe, Tier::Stone);
        let left = grid(3, &[(0, 0, C), (1, 0, C), (0, 1, C), (1, 1, Item::STICK), (1, 2, Item::STICK)]);
        let right = grid(3, &[(1, 0, C), (2, 0, C), (2, 1, C), (1, 1, Item::STICK), (1, 2, Item::STICK)]);
        assert_eq!(left.result().map(|s| s.item), Some(axe));
        assert_eq!(right.result().map(|s| s.item), Some(axe));
        // A 3-wide pickaxe doesn't fit a 2x2 grid, but a table fits in one.
        assert_eq!(
            grid(2, &[(0, 0, P), (1, 0, P), (0, 1, P), (1, 1, P)]).result().unwrap().item,
            b(Block::CRAFTING_TABLE)
        );
        let pick = [(0, 0, C), (1, 0, C), (2, 0, C), (1, 1, Item::STICK), (1, 2, Item::STICK)];
        assert_eq!(grid(3, &pick).result().unwrap().item, Item::tool(ToolKind::Pickaxe, Tier::Stone));
        // An extra stray item spoils it.
        let mut spoiled = pick.to_vec();
        spoiled.push((0, 2, P));
        assert_eq!(grid(3, &spoiled).result(), None);
    }

    #[test]
    fn every_wood_makes_its_own_planks_and_shares_recipes() {
        use crate::world::block::Wood;
        for wood in Wood::ALL {
            let planks = Item::from(wood.planks());
            assert_eq!(grid(2, &[(0, 0, wood.log().into())]).result(), Some(Stack::new(wood.planks(), 4)));
            assert_eq!(grid(2, &[(0, 0, planks), (0, 1, planks)]).result(), Some(Stack::new(Item::STICK, 4)));
        }
        // Mixed planks still make a crafting table.
        let (s, j) = (b(Block::SPRUCE_PLANKS), b(Block::JUNGLE_PLANKS));
        assert_eq!(
            grid(2, &[(0, 0, P), (1, 0, s), (0, 1, j), (1, 1, P)]).result().unwrap().item,
            b(Block::CRAFTING_TABLE)
        );
        let clay = [(0, 0, Item::CLAY_BALL), (1, 0, Item::CLAY_BALL), (0, 1, Item::CLAY_BALL), (1, 1, Item::CLAY_BALL)];
        assert_eq!(grid(2, &clay).result(), Some(Stack::new(Block::CLAY, 1)));
    }

    #[test]
    fn alternatives_and_shapeless() {
        for lump in [Item::COAL, Item::CHARCOAL] {
            assert_eq!(grid(2, &[(1, 0, lump), (1, 1, Item::STICK)]).result(), Some(Stack::new(Block::TORCH, 4)));
        }
        assert_eq!(grid(2, &[(1, 1, b(Block::LOG))]).result(), Some(Stack::new(Block::PLANKS, 4)));
        assert_eq!(grid(2, &[(0, 0, b(Block::LOG)), (1, 1, b(Block::LOG))]).result(), None);
    }

    #[test]
    fn consuming_takes_one_of_each() {
        let mut g = grid(2, &[(0, 0, P), (0, 1, P)]);
        g.cells[0] = Some(Stack::new(P, 1));
        g.consume();
        assert_eq!(g.cells[0], None);
        assert_eq!(g.cells[2], Some(Stack::new(P, 1)));
        assert_eq!(g.result(), None);
        assert_eq!(g.take_all(), vec![Stack::new(P, 1)]);
        assert!(g.cells.iter().all(Option::is_none));
    }

    #[test]
    fn every_tool_is_craftable() {
        let craftable: Vec<Item> = recipes().iter().map(|r| r.result.item).collect();
        for tier in Tier::ALL {
            for kind in [ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Axe, ToolKind::Hoe, ToolKind::Sword] {
                assert!(craftable.contains(&Item::tool(kind, tier)), "{kind:?} {tier:?}");
            }
        }
    }

    #[test]
    fn guide_previews_match_their_recipes_and_display_alternatives() {
        for recipe in recipes() {
            let preview = recipe.preview();
            assert_eq!(preview.result(), Some(recipe.result), "{} preview", recipe.result.item.name());
            for (i, stack) in preview.cells.iter().enumerate() {
                if let Some(stack) = stack {
                    assert!(recipe.alternatives(i).unwrap().contains(&stack.item));
                } else {
                    assert!(recipe.alternatives(i).is_none());
                }
            }
        }
        let torches = recipes().iter().find(|r| r.result.item == Item::from_block(Block::TORCH)).unwrap();
        assert_eq!(torches.alternatives(0), Some(&[Item::COAL, Item::CHARCOAL][..]));
    }

    #[test]
    fn crafting_leftovers_survive_close_and_save_with_a_full_inventory() {
        use crate::inventory::{Inventory, click_slot};

        let mut inv = Inventory::default();
        inv.slots.fill(Some(Stack::new(Block::STONE, 64)));
        inv.slots[0] = Some(Stack::new(Block::LOG, 64));
        let mut g = Grid::new(2);
        inv.click(0, false);
        click_slot(&mut g.cells[0], &mut inv.cursor, false);
        inv.cursor = g.result();
        g.consume();
        inv.click(0, false);
        assert!(inv.slots.iter().all(Option::is_some));

        // Autosave uses a copy, so the open crafting grid remains usable.
        let mut snapshot = inv.clone();
        snapshot.return_stacks(g.cells.iter().flatten().copied());
        let saved = snapshot.serialize();
        assert_eq!(g.cells[0], Some(Stack::new(Block::LOG, 63)));

        // Closing uses the same return path; what doesn't fit spills.
        inv.return_stacks(g.take_all());
        assert!(g.cells.iter().all(Option::is_none));
        assert_eq!(inv.serialize(), saved);
        let mut restored = Inventory::deserialize(&saved).unwrap();
        assert_eq!(restored.get(0), Some(Stack::new(Block::PLANKS, 4)));
        assert_eq!(restored.take_spill(), vec![Stack::new(Block::LOG, 63)]);
    }
}
