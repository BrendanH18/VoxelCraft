//! Crafting: a 2x2 (inventory) or 3x3 (crafting table) grid of stacks and
//! the recipes that turn one of each ingredient into a result.
//!
//! Shaped recipes match wherever their pattern sits in the grid (the grid
//! is trimmed to the bounding box of its filled cells), and also mirrored
//! left to right, like Minecraft. Shapeless recipes match any arrangement
//! of exactly their ingredients.

use crate::inventory::Stack;
use crate::item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind};
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
                let rest = s.item.remainder();
                s.count -= 1;
                if s.count == 0 {
                    *cell = rest.map(|item| Stack::new(item, 1));
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
    b(Block::DARK_OAK_PLANKS),
    b(Block::MANGROVE_PLANKS),
    b(Block::CHERRY_PLANKS),
];
const HAY: Ingredient = &[b(Block::HAY_BALE)];
const WOOD_SLAB: Ingredient = &[
    b(Block(108)),
    b(crate::world::forms::wood_id(0, 4)),
    b(crate::world::forms::wood_id(1, 4)),
    b(crate::world::forms::wood_id(2, 4)),
    b(crate::world::forms::wood_id(3, 4)),
    b(crate::world::forms::wood_id(4, 4)),
    b(crate::world::forms::wood_id(5, 4)),
    b(crate::world::forms::wood_id(6, 4)),
];
const FURNACE: Ingredient = &[b(Block::FURNACE)];
const STONE_SLAB: Ingredient = &[b(Block::STONE_SLAB)];
const BOOKSHELF: Ingredient = &[b(Block::BOOKSHELF)];
const ANY_LOG: Ingredient = &[
    b(Block::LOG),
    b(Block::SPRUCE_LOG),
    b(Block::BIRCH_LOG),
    b(Block::JUNGLE_LOG),
    b(Block::ACACIA_LOG),
    b(Block::DARK_OAK_LOG),
    b(Block::MANGROVE_LOG),
    b(Block::CHERRY_LOG),
];
const OAK_PLANKS: Ingredient = &[b(Block::PLANKS)];
const MELON: Ingredient = &[b(Block::MELON)];
const SAND: Ingredient = &[b(Block::SAND)];
const SAND_STONE: Ingredient = &[b(Block::SANDSTONE)];
const GLASS: Ingredient = &[b(Block::GLASS)];
const STONE: Ingredient = &[b(Block::STONE)];
const SUGAR_CANE: Ingredient = &[b(Block::SUGAR_CANE)];
const COBBLESTONE: Ingredient = &[b(Block::COBBLESTONE)];
const STONE_CRAFTING: Ingredient = &[b(Block::COBBLESTONE), b(Block::BLACKSTONE), b(Block::COBBLED_DEEPSLATE)];
const GRANITE: Ingredient = &[b(Block::GRANITE)];
const DIORITE: Ingredient = &[b(Block::DIORITE)];
const ANDESITE: Ingredient = &[b(Block::ANDESITE)];
const COBBLED_DEEPSLATE: Ingredient = &[b(Block::COBBLED_DEEPSLATE)];
const MOSSY_COBBLE: Ingredient = &[b(Block::MOSSY_COBBLESTONE)];
const POLISHED_GRANITE: Ingredient = &[b(Block::POLISHED_GRANITE)];
const POLISHED_DIORITE: Ingredient = &[b(Block::POLISHED_DIORITE)];
const POLISHED_ANDESITE: Ingredient = &[b(Block::POLISHED_ANDESITE)];
const TUFF: Ingredient = &[b(Block::TUFF)];
const CALCITE: Ingredient = &[b(Block::CALCITE)];
const SMOOTH_STONE: Ingredient = &[b(Block::SMOOTH_STONE)];
const DEEPSLATE: Ingredient = &[b(Block::DEEPSLATE)];
const POLISHED_DEEPSLATE: Ingredient = &[b(Block::POLISHED_DEEPSLATE)];
const STONE_BRICKS: Ingredient = &[b(Block::STONE_BRICKS)];
const SPRUCE_PLANKS: Ingredient = &[b(Block::SPRUCE_PLANKS)];
const BIRCH_PLANKS: Ingredient = &[b(Block::BIRCH_PLANKS)];
const JUNGLE_PLANKS: Ingredient = &[b(Block::JUNGLE_PLANKS)];
const ACACIA_PLANKS: Ingredient = &[b(Block::ACACIA_PLANKS)];
const DARK_OAK_PLANKS: Ingredient = &[b(Block::DARK_OAK_PLANKS)];
const MANGROVE_PLANKS: Ingredient = &[b(Block::MANGROVE_PLANKS)];
const CHERRY_PLANKS: Ingredient = &[b(Block::CHERRY_PLANKS)];
const DARK_OAK_LOG: Ingredient = &[b(Block::DARK_OAK_LOG)];
const MANGROVE_LOG: Ingredient = &[b(Block::MANGROVE_LOG)];
const CHERRY_LOG: Ingredient = &[b(Block::CHERRY_LOG)];
const QUARTZ: Ingredient = &[Item::NETHER_QUARTZ];
const NETHER_BRICKS: Ingredient = &[b(Block::NETHER_BRICKS)];
const STICK: Ingredient = &[Item::STICK];
const LAPIS_BLOCK: Ingredient = &[b(Block::LAPIS_BLOCK)];
const OBSIDIAN: Ingredient = &[b(Block::OBSIDIAN)];
const IRON_BLOCK: Ingredient = &[b(Block::IRON_BLOCK)];
const RAW_IRON_BLOCK: Ingredient = &[b(Block::RAW_IRON_BLOCK)];
const RAW_GOLD_BLOCK: Ingredient = &[b(Block::RAW_GOLD_BLOCK)];
const RAW_COPPER_BLOCK: Ingredient = &[b(Block::RAW_COPPER_BLOCK)];
const COPPER_BLOCK: Ingredient = &[b(Block::COPPER_BLOCK)];
const EMERALD_BLOCK: Ingredient = &[b(Block::EMERALD_BLOCK)];
const IRON: Ingredient = &[Item::IRON_INGOT];
const GOLD: Ingredient = &[Item::GOLD_INGOT];
const NETHERRACK: Ingredient = &[b(Block::NETHERRACK)];
const NETHERITE_SCRAP: Ingredient = &[Item::NETHERITE_SCRAP];
const NETHERITE_INGOT: Ingredient = &[Item::NETHERITE_INGOT];
const NETHERITE_BLOCK: Ingredient = &[b(Block::NETHERITE_BLOCK)];
const FUEL_LUMP: Ingredient = &[Item::COAL, Item::CHARCOAL];
const BROWN_MUSHROOM: Ingredient = &[b(Block::BROWN_MUSHROOM)];
const RED_MUSHROOM: Ingredient = &[b(Block::RED_MUSHROOM)];
const BOWL: Ingredient = &[Item::BOWL];
const SNOWBALL: Ingredient = &[Item::SNOWBALL];
const PUMPKIN: Ingredient = &[b(Block::PUMPKIN)];
const MILK: Ingredient = &[Item::MILK_BUCKET];
const SUGAR: Ingredient = &[Item::SUGAR];
const EGG: Ingredient = &[Item::EGG];
const WHEAT: Ingredient = &[Item::WHEAT];
const STRING: Ingredient = &[Item::STRING];

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
            shapeless(&[SUGAR_CANE], Item::SUGAR, 1),
            shaped(&["###"], &[('#', SUGAR_CANE)], Item::PAPER, 3),
            shapeless(&[&[Item::PAPER], &[Item::PAPER], &[Item::PAPER], &[Item::LEATHER]], Item::BOOK, 1),
            shaped(&["###", "bbb", "###"], &[('#', PLANKS), ('b', &[Item::BOOK])], b(Block::BOOKSHELF), 1),
            shapeless(&[&[Item::ENDER_PEARL], &[Item::BLAZE_POWDER]], Item::EYE_OF_ENDER, 1),
            shaped(&["###", "###"], &[('#', &[Item::IRON_INGOT])], b(Block::IRON_BARS), 16),
            shaped(&["X X", "X#X", "X X"], &[('X', IRON), ('#', STICK)], b(Block::RAIL), 16),
            shaped(&["###", "###", "###"], &[('#', &[Item::LAPIS_LAZULI])], b(Block::LAPIS_BLOCK), 1),
            shapeless(&[LAPIS_BLOCK], Item::LAPIS_LAZULI, 9),
            shaped(&["###", "###", "###"], &[('#', IRON)], b(Block::IRON_BLOCK), 1),
            shapeless(&[IRON_BLOCK], Item::IRON_INGOT, 9),
            shaped(&["###", "###", "###"], &[('#', &[Item::RAW_IRON])], b(Block::RAW_IRON_BLOCK), 1),
            shapeless(&[RAW_IRON_BLOCK], Item::RAW_IRON, 9),
            shaped(&["###", "###", "###"], &[('#', &[Item::RAW_GOLD])], b(Block::RAW_GOLD_BLOCK), 1),
            shapeless(&[RAW_GOLD_BLOCK], Item::RAW_GOLD, 9),
            shaped(&["###", "###", "###"], &[('#', &[Item::RAW_COPPER])], b(Block::RAW_COPPER_BLOCK), 1),
            shapeless(&[RAW_COPPER_BLOCK], Item::RAW_COPPER, 9),
            shaped(&["###", "###", "###"], &[('#', &[Item::COPPER_INGOT])], b(Block::COPPER_BLOCK), 1),
            shapeless(&[COPPER_BLOCK], Item::COPPER_INGOT, 9),
            shaped(&["###", "###", "###"], &[('#', &[Item::EMERALD])], b(Block::EMERALD_BLOCK), 1),
            shapeless(&[EMERALD_BLOCK], Item::EMERALD, 9),
            shaped(&["##", "##"], &[('#', GRANITE)], b(Block::POLISHED_GRANITE), 4),
            shaped(&["##", "##"], &[('#', DIORITE)], b(Block::POLISHED_DIORITE), 4),
            shaped(&["##", "##"], &[('#', ANDESITE)], b(Block::POLISHED_ANDESITE), 4),
            shaped(&["##", "##"], &[('#', COBBLED_DEEPSLATE)], b(Block::POLISHED_DEEPSLATE), 4),
            // Vanilla: diorite and quartz, no stonecutter.
            shaped(&["CQ", "QC"], &[('C', COBBLESTONE), ('Q', QUARTZ)], b(Block::DIORITE), 2),
            shaped(&["DQ", "QD"], &[('D', DIORITE), ('Q', QUARTZ)], b(Block::GRANITE), 1),
            shaped(&["CD", "DC"], &[('C', COBBLESTONE), ('D', DIORITE)], b(Block::ANDESITE), 2),
            shapeless(
                &[NETHERITE_SCRAP, GOLD, NETHERITE_SCRAP, GOLD, NETHERITE_SCRAP, GOLD, NETHERITE_SCRAP, GOLD],
                Item::NETHERITE_INGOT,
                1,
            ),
            shaped(&["###", "###", "###"], &[('#', NETHERITE_INGOT)], b(Block::NETHERITE_BLOCK), 1),
            shapeless(&[NETHERITE_BLOCK], Item::NETHERITE_INGOT, 9),
            // Java copies an upgrade template with seven diamonds around
            // it and a netherrack below it.
            shaped(
                &["#S#", "#C#", "###"],
                &[('#', &[Item::DIAMOND]), ('S', &[Item::NETHERITE_UPGRADE]), ('C', NETHERRACK)],
                Item::NETHERITE_UPGRADE,
                2,
            ),
            shaped(&["III", " i ", "iii"], &[('I', IRON_BLOCK), ('i', IRON)], b(Block::ANVIL), 1),
            shaped(
                &[" b ", "d#d", "###"],
                &[('b', &[Item::BOOK]), ('d', &[Item::DIAMOND]), ('#', OBSIDIAN)],
                b(Block::ENCHANTING_TABLE),
                1,
            ),
            shaped(&["##", "##"], &[('#', STONE)], b(Block::STONE_BRICKS), 4),
            shaped(
                &["###", "#m#", "###"],
                &[('#', &[Item::GOLD_NUGGET]), ('m', &[Item::MELON_SLICE])],
                Item::GLISTERING_MELON_SLICE,
                1,
            ),
            shaped(&["#", "#"], &[('#', PLANKS)], Item::STICK, 4),
            shaped(&["##", "##"], &[('#', PLANKS)], b(Block::CRAFTING_TABLE), 1),
            shaped(&["@@", "##", "##"], &[('@', IRON), ('#', PLANKS)], b(Block::SMITHING_TABLE), 1),
            shaped(&["###", "# #", "###"], &[('#', STONE_CRAFTING)], b(Block::FURNACE), 1),
            shaped(&[" r ", "###"], &[('r', &[Item::BLAZE_ROD]), ('#', COBBLESTONE)], b(Block::BREWING_STAND), 1),
            shaped(&["###", "# #", "###"], &[('#', PLANKS)], b(Block::CHEST), 1),
            shaped(&["###"], &[('#', &[Item::WHEAT])], Item::BREAD, 1),
            shaped(&["###", "###", "###"], &[('#', WHEAT)], b(Block::HAY_BALE), 1),
            shaped(&["GGG", "GAG", "GGG"], &[('G', &[Item::GOLD_INGOT]), ('A', &[Item::APPLE])], Item::GOLDEN_APPLE, 1),
            shapeless(&[const { &[b(Block::PUMPKIN)] }], Item::PUMPKIN_SEEDS, 4),
            shapeless(
                &[const { &[b(Block::CARVED_PUMPKIN)] }, const { &[b(Block::TORCH)] }],
                b(Block::JACK_O_LANTERN),
                1,
            ),
            shapeless(&[HAY], Item::WHEAT, 9),
            shaped(&["# #", "# #", "###"], &[('#', WOOD_SLAB)], b(Block::COMPOSTER), 1),
            shaped(&["#s#", "# #", "#s#"], &[('#', PLANKS), ('s', WOOD_SLAB)], b(Block::BARREL), 1),
            shaped(&[" l ", "lFl", " l "], &[('l', ANY_LOG), ('F', FURNACE)], b(Block::SMOKER), 1),
            shaped(
                &["III", "IFI", "SSS"],
                &[('I', IRON), ('F', FURNACE), ('S', SMOOTH_STONE)],
                b(Block::BLAST_FURNACE),
                1,
            ),
            shaped(&["pp", "##", "##"], &[('p', &[Item::PAPER]), ('#', PLANKS)], b(Block::CARTOGRAPHY_TABLE), 1),
            shaped(&["ff", "##", "##"], &[('f', &[Item::FLINT]), ('#', PLANKS)], b(Block::FLETCHING_TABLE), 1),
            shaped(&["sSs", "# #"], &[('s', STICK), ('S', STONE_SLAB), ('#', PLANKS)], b(Block::GRINDSTONE), 1),
            shaped(&["sss", " B ", " s "], &[('s', WOOD_SLAB), ('B', BOOKSHELF)], b(Block::LECTERN), 1),
            shaped(&["ss", "##"], &[('s', STRING), ('#', PLANKS)], b(Block::LOOM), 1),
            shaped(&[" I ", "SSS"], &[('I', IRON), ('S', STONE)], b(Block::STONECUTTER), 1),
            shapeless(&[&[Item::BONE]], Item::BONE_MEAL, 3),
            shaped(&["c", "#"], &[('c', FUEL_LUMP), ('#', STICK)], b(Block::TORCH), 4),
            shaped(&["##", "##"], &[('#', SAND)], b(Block::SANDSTONE), 1),
            shaped(&["##", "##"], &[('#', &[Item::STRING])], b(Block::WOOL), 1),
            shapeless(&[&[Item::IRON_INGOT], &[Item::FLINT]], Item::FLINT_AND_STEEL, 1),
            shaped(&["# #", " # "], &[('#', &[Item::IRON_INGOT])], Item::BUCKET, 1),
            shaped(&["# #", " # "], &[('#', PLANKS)], Item::BOWL, 4),
            shapeless(&[BROWN_MUSHROOM, RED_MUSHROOM, BOWL], Item::MUSHROOM_STEW, 1),
            shaped(&["##", "##"], &[('#', SNOWBALL)], b(Block::SNOW), 1),
            shaped(&[" # ", "#X#", " # "], &[('#', IRON), ('X', &[Item::REDSTONE])], Item::COMPASS, 1),
            shaped(&[" # ", "#X#", " # "], &[('#', GOLD), ('X', &[Item::REDSTONE])], Item::CLOCK, 1),
            shaped(&["AAA", "BEB", "CCC"], &[('A', MILK), ('B', SUGAR), ('E', EGG), ('C', WHEAT)], Item::CAKE, 1),
            shapeless(&[PUMPKIN, SUGAR, EGG], Item::PUMPKIN_PIE, 1),
            shaped(&["  #", " #X", "# X"], &[('#', STICK), ('X', STRING)], Item::FISHING_ROD, 1),
            shaped(&[" #", "# "], &[('#', &[Item::IRON_INGOT])], Item::SHEARS, 1),
            shaped(&["# #", " # "], &[('#', GLASS)], Item::GLASS_BOTTLE, 3),
            shaped(&["X#X", "#X#", "X#X"], &[('X', &[Item::GUNPOWDER]), ('#', SAND)], b(Block::TNT), 1),
            shaped(&["##", "##"], &[('#', &[Item::NETHER_BRICK])], b(Block::NETHER_BRICKS), 1),
            shaped(&["##", "##"], &[('#', &[Item::GLOWSTONE_DUST])], b(Block::GLOWSTONE), 1),
            shaped(&["###", "###", "###"], &[('#', &[Item::GOLD_NUGGET])], Item::GOLD_INGOT, 1),
            shapeless(&[&[Item::GOLD_INGOT]], Item::GOLD_NUGGET, 9),
            shapeless(&[&[Item::BLAZE_ROD]], Item::BLAZE_POWDER, 2),
            shaped(
                &["#I#", "#I#"],
                &[('#', NETHER_BRICKS), ('I', &[Item::NETHER_BRICK])],
                b(Block::NETHER_BRICK_FENCE),
                6,
            ),
            shaped(&[" #s", "# s", " #s"], &[('#', STICK), ('s', &[Item::STRING])], Item::BOW, 1),
            shaped(&["f", "#", "e"], &[('f', &[Item::FLINT]), ('#', STICK), ('e', &[Item::FEATHER])], Item::ARROW, 4),
        ];
        const LOGS: [(Ingredient, Block); 5] = [
            (&[b(Block::LOG)], Block::PLANKS),
            (&[b(Block::SPRUCE_LOG)], Block::SPRUCE_PLANKS),
            (&[b(Block::BIRCH_LOG)], Block::BIRCH_PLANKS),
            (&[b(Block::JUNGLE_LOG)], Block::JUNGLE_PLANKS),
            (&[b(Block::ACACIA_LOG)], Block::ACACIA_PLANKS),
        ];
        const BLACK: Ingredient = &[b(Block::BLACKSTONE)];
        const POLISHED: Ingredient = &[b(Block::POLISHED_BLACKSTONE)];
        const BLACK_BRICKS: Ingredient = &[b(Block::POLISHED_BLACKSTONE_BRICKS)];
        const POLISHED_SLAB: Ingredient = &[b(crate::world::nether_blocks::shape_id(1, 4))];
        const GOLD_BLOCK: Ingredient = &[b(Block::GOLD_BLOCK)];
        const BASALT: Ingredient = &[b(Block::BASALT)];
        r.push(shaped(&["##", "##"], &[('#', BLACK)], b(Block::POLISHED_BLACKSTONE), 4));
        r.push(shaped(&["##", "##"], &[('#', POLISHED)], b(Block::POLISHED_BLACKSTONE_BRICKS), 4));
        r.push(shaped(&["#", "#"], &[('#', POLISHED_SLAB)], b(Block::CHISELED_POLISHED_BLACKSTONE), 1));
        r.push(shaped(&["##", "##"], &[('#', BASALT)], b(Block::POLISHED_BASALT), 4));
        r.push(shaped(&["##", "##"], &[('#', &[Item::MAGMA_CREAM])], b(Block::MAGMA), 1));
        r.push(shapeless(&[&[Item::SLIME_BALL], &[Item::BLAZE_POWDER]], Item::MAGMA_CREAM, 1));
        r.push(shaped(&["N", "I", "N"], &[('N', &[Item::IRON_NUGGET]), ('I', IRON)], b(Block::CHAIN), 1));
        r.push(shaped(&["###", "###", "###"], &[('#', GOLD)], b(Block::GOLD_BLOCK), 1));
        r.push(shapeless(&[GOLD_BLOCK], Item::GOLD_INGOT, 9));
        r.push(shaped(&["###", "###", "###"], &[('#', &[Item::IRON_NUGGET])], Item::IRON_INGOT, 1));
        r.push(shapeless(&[IRON], Item::IRON_NUGGET, 9));
        for (i, mat) in [BLACK, POLISHED, BLACK_BRICKS].into_iter().enumerate() {
            r.push(shaped(&["#  ", "## ", "###"], &[('#', mat)], b(crate::world::nether_blocks::shape_id(i, 0)), 4));
            r.push(shaped(&["###"], &[('#', mat)], b(crate::world::nether_blocks::shape_id(i, 4)), 6));
            r.push(shaped(&["###", "###"], &[('#', mat)], b(crate::world::nether_blocks::shape_id(i, 5)), 6));
        }
        // Planks first: the recipe guide lists them before everything else.
        for (i, (log, planks)) in LOGS.into_iter().enumerate() {
            r.insert(i, shapeless(&[log], b(planks), 4));
        }
        const MATERIALS: [(Tier, Ingredient); 5] = [
            (Tier::Wood, PLANKS),
            (Tier::Stone, STONE_CRAFTING),
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
        const SLABS: [Ingredient; 6] =
            [&[b(Block::STONE)], COBBLESTONE, OAK_PLANKS, SAND_STONE, &[b(Block::BRICKS)], &[b(Block::NETHER_BRICKS)]];
        for (i, base) in SLABS.into_iter().enumerate() {
            r.push(shaped(&["###"], &[('#', base)], b(Block(Block::STONE_SLAB.0 + i as u16)), 6));
            r.push(shaped(&["#  ", "## ", "###"], &[('#', base)], b(Block(Block::STONE_STAIRS.0 + i as u16 * 4)), 4));
        }
        r.push(shaped(&["#s#", "#s#"], &[('#', OAK_PLANKS), ('s', STICK)], b(Block::OAK_FENCE), 3));
        r.push(shaped(&["s#s", "s#s"], &[('#', OAK_PLANKS), ('s', STICK)], b(Block::FENCE_GATE), 1));
        r.push(shaped(&["s s", "sss", "s s"], &[('s', STICK)], b(Block::LADDER), 3));
        r.push(shaped(&["##", "##", "##"], &[('#', OAK_PLANKS)], Item::OAK_DOOR, 3));
        const STONE_SHAPES: [Ingredient; 13] = [
            MOSSY_COBBLE,
            GRANITE,
            POLISHED_GRANITE,
            DIORITE,
            POLISHED_DIORITE,
            ANDESITE,
            POLISHED_ANDESITE,
            TUFF,
            CALCITE,
            SMOOTH_STONE,
            DEEPSLATE,
            COBBLED_DEEPSLATE,
            POLISHED_DEEPSLATE,
        ];
        for (i, mat) in STONE_SHAPES.into_iter().enumerate() {
            let i = i as u16;
            r.push(shaped(&["#  ", "## ", "###"], &[('#', mat)], b(crate::world::forms::stone_id(i, 0)), 4));
            r.push(shaped(&["###"], &[('#', mat)], b(crate::world::forms::stone_id(i, 4)), 6));
            r.push(shaped(&["###", "###"], &[('#', mat)], b(crate::world::forms::stone_id(i, 5)), 6));
        }
        r.push(shaped(&["###", "###"], &[('#', COBBLESTONE)], b(Block::COBBLESTONE_WALL), 6));
        r.push(shaped(&["###", "###"], &[('#', STONE_BRICKS)], b(Block::STONE_BRICK_WALL), 6));
        const WOOD_SHAPES: [Ingredient; 7] = [
            DARK_OAK_PLANKS,
            SPRUCE_PLANKS,
            BIRCH_PLANKS,
            JUNGLE_PLANKS,
            ACACIA_PLANKS,
            MANGROVE_PLANKS,
            CHERRY_PLANKS,
        ];
        const EXTRA_LOGS: [(Ingredient, Block); 3] = [
            (DARK_OAK_LOG, Block::DARK_OAK_PLANKS),
            (MANGROVE_LOG, Block::MANGROVE_PLANKS),
            (CHERRY_LOG, Block::CHERRY_PLANKS),
        ];
        for (log, planks) in EXTRA_LOGS {
            r.push(shapeless(&[log], b(planks), 4));
        }
        for (i, mat) in WOOD_SHAPES.into_iter().enumerate() {
            let i = i as u16;
            r.push(shaped(&["#  ", "## ", "###"], &[('#', mat)], b(crate::world::forms::wood_id(i, 0)), 4));
            r.push(shaped(&["###"], &[('#', mat)], b(crate::world::forms::wood_id(i, 4)), 6));
            r.push(shaped(&["#s#", "#s#"], &[('#', mat), ('s', STICK)], b(crate::world::forms::wood_id(i, 5)), 3));
            r.push(shaped(&["s#s", "s#s"], &[('#', mat), ('s', STICK)], b(crate::world::forms::wood_id(i, 6)), 1));
            r.push(shaped(&["##", "##", "##"], &[('#', mat)], b(crate::world::forms::wood_id(i, 14)), 3));
        }
        const ARMOR: [(ArmorMaterial, Ingredient); 4] = [
            (ArmorMaterial::Leather, &[Item::LEATHER]),
            (ArmorMaterial::Iron, &[Item::IRON_INGOT]),
            (ArmorMaterial::Gold, &[Item::GOLD_INGOT]),
            (ArmorMaterial::Diamond, &[Item::DIAMOND]),
        ];
        for (material, m) in ARMOR {
            let armor = |piece, rows| shaped(rows, &[('X', m)], Item::armor(piece, material), 1);
            r.push(armor(ArmorPiece::Helmet, &["XXX", "X X"]));
            r.push(armor(ArmorPiece::Chestplate, &["X X", "XXX", "XXX"]));
            r.push(armor(ArmorPiece::Leggings, &["XXX", "X X", "X X"]));
            r.push(armor(ArmorPiece::Boots, &["X X", "X X"]));
        }
        add_redstone_recipes(&mut r);
        add_dye_recipes(&mut r);
        add_wool_recipes(&mut r);
        add_glass_terracotta_recipes(&mut r);
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netherite_requires_four_scraps_and_four_gold_and_packs_losslessly() {
        let mut g = Grid::new(3);
        for (i, cell) in g.cells[..8].iter_mut().enumerate() {
            *cell = Some(Stack::new(if i % 2 == 0 { Item::GOLD_INGOT } else { Item::NETHERITE_SCRAP }, 2));
        }
        assert_eq!(g.result(), Some(Stack::new(Item::NETHERITE_INGOT, 1)));
        g.consume();
        assert!(g.cells[..8].iter().all(|c| c.is_some_and(|s| s.count == 1)));
        g.cells[7] = Some(Stack::new(Item::GOLD_INGOT, 1));
        assert_eq!(g.result(), None, "five gold and three scraps cannot substitute");
        g.cells = [Some(Stack::new(Item::NETHERITE_INGOT, 1)); 9];
        assert_eq!(g.result(), Some(Stack::new(Block::NETHERITE_BLOCK, 1)));
        g.consume();
        assert_eq!(g.cells, [None; 9]);
        g.cells[4] = Some(Stack::new(Block::NETHERITE_BLOCK, 1));
        assert_eq!(g.result(), Some(Stack::new(Item::NETHERITE_INGOT, 9)));
    }

    #[test]
    fn upgrade_templates_copy_with_diamonds_and_netherrack() {
        let (d, t, n) = (Item::DIAMOND, Item::NETHERITE_UPGRADE, b(Block::NETHERRACK));
        let cells = [(0, 0, d), (1, 0, t), (2, 0, d), (0, 1, d), (1, 1, n), (2, 1, d), (0, 2, d), (1, 2, d), (2, 2, d)];
        let mut g = grid(3, &cells);
        assert_eq!(g.result(), Some(Stack::new(Item::NETHERITE_UPGRADE, 2)));
        g.consume();
        assert!(g.cells.iter().all(|c| c.is_some_and(|s| s.count == 1)), "one of each input is used");
        g.cells[4] = Some(Stack::new(Block::COBBLESTONE, 1));
        assert_eq!(g.result(), None, "only netherrack");
        assert_eq!(grid(3, &cells[1..]).result(), None, "all seven diamonds");
    }

    #[test]
    fn rails_use_six_iron_and_a_stick() {
        let cells = [
            (0, 0, Item::IRON_INGOT),
            (2, 0, Item::IRON_INGOT),
            (0, 1, Item::IRON_INGOT),
            (1, 1, Item::STICK),
            (2, 1, Item::IRON_INGOT),
            (0, 2, Item::IRON_INGOT),
            (2, 2, Item::IRON_INGOT),
        ];
        assert_eq!(grid(3, &cells).result(), Some(Stack::new(Block::RAIL, 16)));
        assert_eq!(grid(3, &cells[1..]).result(), None);
    }

    #[test]
    fn smithing_table_uses_two_iron_over_four_planks() {
        let cells = [
            (0, 0, Item::IRON_INGOT),
            (1, 0, Item::IRON_INGOT),
            (0, 1, b(Block::PLANKS)),
            (1, 1, b(Block::SPRUCE_PLANKS)),
            (0, 2, b(Block::BIRCH_PLANKS)),
            (1, 2, b(Block::JUNGLE_PLANKS)),
        ];
        assert_eq!(grid(3, &cells).result(), Some(Stack::new(Block::SMITHING_TABLE, 1)));
        assert_eq!(grid(3, &cells[1..]).result(), None);
    }

    #[test]
    fn armor_recipes() {
        let iron = Some(Stack::new(Item::IRON_INGOT, 1));
        let mut g = Grid::new(3);
        for i in [0, 2, 3, 4, 5, 6, 7, 8] {
            g.cells[i] = iron;
        }
        assert_eq!(g.result().unwrap().item, Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Iron));
        let mut g = Grid::new(2);
        let leather = Some(Stack::new(Item::LEATHER, 1));
        g.cells = [leather, None, leather, None, None, None, None, None, None];
        assert_eq!(g.result(), None);
        let mut g = Grid::new(3);
        for i in [3, 5, 6, 8] {
            g.cells[i] = leather;
        }
        assert_eq!(g.result().unwrap().item, Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather));
    }

    #[test]
    fn a_fishing_rod_is_three_sticks_and_two_string() {
        let cells =
            [(2, 0, Item::STICK), (1, 1, Item::STICK), (2, 1, Item::STRING), (0, 2, Item::STICK), (2, 2, Item::STRING)];
        assert_eq!(grid(3, &cells).result(), Some(Stack::new(Item::FISHING_ROD, 1)));
    }

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
    fn cake_returns_the_milk_buckets() {
        let cells = [
            (0, 0, Item::MILK_BUCKET),
            (1, 0, Item::MILK_BUCKET),
            (2, 0, Item::MILK_BUCKET),
            (0, 1, Item::SUGAR),
            (1, 1, Item::EGG),
            (2, 1, Item::SUGAR),
            (0, 2, Item::WHEAT),
            (1, 2, Item::WHEAT),
            (2, 2, Item::WHEAT),
        ];
        let mut g = Grid::new(3);
        for (x, y, item) in cells {
            g.cells[y * 3 + x] = Some(Stack::new(item, 1));
        }
        assert_eq!(g.result(), Some(Stack::new(Item::CAKE, 1)));
        g.consume();
        let buckets = g.cells.iter().flatten().filter(|s| s.item == Item::BUCKET).count();
        assert_eq!(buckets, 3);
        let mut pie = Grid::new(3);
        pie.cells[0] = Some(Stack::new(Block::PUMPKIN, 1));
        pie.cells[1] = Some(Stack::new(Item::SUGAR, 1));
        pie.cells[2] = Some(Stack::new(Item::EGG, 1));
        assert_eq!(pie.result(), Some(Stack::new(Item::PUMPKIN_PIE, 1)));
    }

    #[test]
    fn compass_and_clock_use_redstone() {
        let compass = grid(
            3,
            &[
                (1, 0, Item::IRON_INGOT),
                (0, 1, Item::IRON_INGOT),
                (1, 1, Item::REDSTONE),
                (2, 1, Item::IRON_INGOT),
                (1, 2, Item::IRON_INGOT),
            ],
        );
        assert_eq!(compass.result(), Some(Stack::new(Item::COMPASS, 1)));
        let clock = grid(
            3,
            &[
                (1, 0, Item::GOLD_INGOT),
                (0, 1, Item::GOLD_INGOT),
                (1, 1, Item::REDSTONE),
                (2, 1, Item::GOLD_INGOT),
                (1, 2, Item::GOLD_INGOT),
            ],
        );
        assert_eq!(clock.result(), Some(Stack::new(Item::CLOCK, 1)));
    }

    #[test]
    fn four_snowballs_craft_a_snow_block() {
        let g =
            grid(2, &[(0, 0, Item::SNOWBALL), (1, 0, Item::SNOWBALL), (0, 1, Item::SNOWBALL), (1, 1, Item::SNOWBALL)]);
        assert_eq!(g.result(), Some(Stack::new(Block::SNOW, 1)));
    }

    #[test]
    fn shaped_recipes_match_anywhere_and_mirrored() {
        // Sticks: two planks stacked, in any column.
        for x in 0..3 {
            let g = grid(3, &[(x, 1, P), (x, 2, P)]);
            assert_eq!(g.result(), Some(Stack::new(Item::STICK, 4)));
        }
        // Side by side makes a pressure plate, not sticks.
        assert_eq!(
            grid(3, &[(0, 0, P), (1, 0, P)]).result(),
            Some(Stack::new(crate::world::redstone_blocks::WOOD_PLATE, 1))
        );
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
        assert_eq!(g.result(), Some(Stack::new(crate::world::redstone_blocks::WOOD_BUTTON, 1)));
        assert_eq!(g.take_all(), vec![Stack::new(P, 1)]);
        assert!(g.cells.iter().all(Option::is_none));
    }

    #[test]
    fn every_tool_is_craftable() {
        let craftable: Vec<Item> = recipes().iter().map(|r| r.result.item).collect();
        for tier in Tier::ALL {
            for kind in [ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Axe, ToolKind::Hoe, ToolKind::Sword] {
                // Netherite gear only comes from smithing diamond gear.
                let netherite = tier == Tier::Netherite;
                assert_eq!(craftable.contains(&Item::tool(kind, tier)), !netherite, "{kind:?} {tier:?}");
            }
        }
        for piece in crate::item::ArmorPiece::ALL {
            assert!(!craftable.contains(&Item::armor(piece, crate::item::ArmorMaterial::Netherite)));
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

    #[test]
    fn raw_metal_and_gems_pack_into_blocks() {
        let pack = |item: Item, block: Block| {
            let mut g = Grid::new(3);
            g.cells = [Some(Stack::new(item, 1)); 9];
            assert_eq!(g.result(), Some(Stack::new(block, 1)));
            let mut back = Grid::new(2);
            back.cells[0] = Some(Stack::new(block, 1));
            assert_eq!(back.result(), Some(Stack::new(item, 9)));
        };
        pack(Item::RAW_IRON, Block::RAW_IRON_BLOCK);
        pack(Item::RAW_GOLD, Block::RAW_GOLD_BLOCK);
        pack(Item::RAW_COPPER, Block::RAW_COPPER_BLOCK);
        pack(Item::COPPER_INGOT, Block::COPPER_BLOCK);
        pack(Item::EMERALD, Block::EMERALD_BLOCK);
        let mut polished = Grid::new(2);
        for cell in &mut polished.cells[..4] {
            *cell = Some(Stack::new(Block::GRANITE, 1));
        }
        assert_eq!(polished.result(), Some(Stack::new(Block::POLISHED_GRANITE, 4)));
        let mut andesite = Grid::new(2);
        andesite.cells[0] = Some(Stack::new(Block::COBBLESTONE, 1));
        andesite.cells[1] = Some(Stack::new(Block::DIORITE, 1));
        andesite.cells[2] = Some(Stack::new(Block::DIORITE, 1));
        andesite.cells[3] = Some(Stack::new(Block::COBBLESTONE, 1));
        assert_eq!(andesite.result(), Some(Stack::new(Block::ANDESITE, 2)));
        assert_eq!(
            crate::world::furnace::smelt(Item::from_block(Block::STONE)),
            Some(Item::from_block(Block::SMOOTH_STONE))
        );
        assert_eq!(
            crate::world::furnace::smelt(Item::from_block(Block::COBBLED_DEEPSLATE)),
            Some(Item::from_block(Block::DEEPSLATE))
        );
        assert_eq!(crate::world::furnace::smelt(Item::from_block(Block::DEEPSLATE_IRON_ORE)), Some(Item::IRON_INGOT));
    }

    #[test]
    fn bowls_and_mushroom_stew_follow_the_vanilla_recipes() {
        let mut bowl = Grid::new(3);
        bowl.cells[0] = Some(Stack::new(Block::PLANKS, 1));
        bowl.cells[2] = Some(Stack::new(Block::PLANKS, 1));
        bowl.cells[4] = Some(Stack::new(Block::PLANKS, 1));
        assert_eq!(bowl.result(), Some(Stack::new(Item::BOWL, 4)));
        let mut stew = Grid::new(2);
        stew.cells[0] = Some(Stack::new(Block::BROWN_MUSHROOM, 1));
        stew.cells[1] = Some(Stack::new(Block::RED_MUSHROOM, 1));
        stew.cells[2] = Some(Stack::new(Item::BOWL, 1));
        assert_eq!(stew.result(), Some(Stack::new(Item::MUSHROOM_STEW, 1)));
        assert_eq!(Item::from_name("milk_bucket"), Some(Item::MILK_BUCKET));
        assert_eq!(Item::MUSHROOM_STEW.food(), Some((6, 7.2)));
        assert_eq!(Item::MUSHROOM_STEW.remainder(), Some(Item::BOWL));
        assert!(Item::creative_palette().any(|item| item == Item::BOWL));
        assert!(Block::creative_palette().any(|block| block == Block::RED_MUSHROOM));
    }
}

/// Vanilla 1.21 recipe JSON: source conversions and every dye mixing recipe.
fn add_dye_recipes(r: &mut Vec<Recipe>) {
    r.push(shapeless(&[&[Item::LAPIS_LAZULI]], Item(587), 1));
    r.push(shapeless(&[&[Item(587)], &[Item(589)]], Item(585), 2));
    r.push(shapeless(&[&[Item(591)], &[Item(576)]], Item(583), 2));
    r.push(shapeless(&[const { &[b(Block::BLUE_ORCHID)] }], Item(579), 1));
    r.push(shapeless(&[&[Item(587)], &[Item(576)]], Item(579), 2));
    r.push(shapeless(&[&[Item(591)], &[Item(576)], &[Item(576)]], Item(584), 3));
    r.push(shapeless(&[&[Item(583)], &[Item(576)]], Item(584), 2));
    r.push(shapeless(&[&[Item(589)], &[Item(576)]], Item(581), 2));
    r.push(shapeless(&[&[Item(587)], &[Item(590)], &[Item(582)]], Item(578), 3));
    r.push(shapeless(&[&[Item(587)], &[Item(590)], &[Item(590)], &[Item(576)]], Item(578), 4));
    r.push(shapeless(&[&[Item(586)], &[Item(582)]], Item(578), 2));
    r.push(shapeless(&[&[Item(590)], &[Item(580)]], Item(577), 2));
    r.push(shapeless(&[&[Item(590)], &[Item(576)]], Item(582), 2));
    r.push(shapeless(&[&[Item(587)], &[Item(590)]], Item(586), 2));
    r.push(shapeless(&[const { &[b(Block::POPPY)] }], Item(590), 1));
    r.push(shapeless(&[&[Item::BONE_MEAL]], Item(576), 1));
    r.push(shapeless(&[const { &[b(Block::DANDELION)] }], Item(580), 1));
}

#[cfg(test)]
mod dye_tests {
    use super::*;
    #[test]
    fn vanilla_dye_mixes_use_exact_ingredients_and_counts() {
        for recipe in recipes().iter().filter(|r| r.result.item.dye_color().is_some()) {
            assert_eq!(recipe.preview().result(), Some(recipe.result));
        }
        let mut g = Grid::new(2);
        for (i, id) in [587, 590, 590, 576].into_iter().enumerate() {
            g.cells[i] = Some(Stack::new(Item(id), 1));
        }
        assert_eq!(g.result(), Some(Stack::new(Item(578), 4)));
        g.cells[3] = None;
        assert_eq!(g.result(), None);
        g.cells = [None; 9];
        g.cells[0] = Some(Stack::new(Item::BONE_MEAL, 1));
        assert_eq!(g.result(), Some(Stack::new(Item(576), 1)));
    }
}

fn add_wool_recipes(r: &mut Vec<Recipe>) {
    use crate::color::DyeColor;
    static DYES: [[Item; 1]; 16] = color_ingredients(0);
    static WOOLS: [[Item; 1]; 16] = color_ingredients(1);
    static OTHER_WOOL: [[Item; 15]; 16] = other_colors(1);
    static OTHER_CARPET: [[Item; 15]; 16] = other_colors(2);
    static OTHER_BED: [[Item; 15]; 16] = other_colors(3);
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        r.push(shapeless(&[&OTHER_WOOL[i], &DYES[i]], b(Block::wool(c)), 1));
        r.push(shaped(&["###"], &[('#', &WOOLS[i])], b(Block::carpet(c)), 3));
        r.push(shapeless(&[&OTHER_CARPET[i], &DYES[i]], b(Block::carpet(c)), 1));
        r.push(shaped(&["WWW", "###"], &[('W', &WOOLS[i]), ('#', PLANKS)], c.bed(), 1));
        r.push(shapeless(&[&OTHER_BED[i], &DYES[i]], c.bed(), 1));
    }
    r.push(shaped(&[" #", "# "], &[('#', IRON)], Item::SHEARS, 1));
}

fn add_glass_terracotta_recipes(r: &mut Vec<Recipe>) {
    use crate::color::DyeColor;
    static DYES: [[Item; 1]; 16] = color_ingredients(0);
    static GLASSES: [[Item; 1]; 16] = color_ingredients(4);
    const TERRACOTTA: Ingredient = &[b(Block::TERRACOTTA)];
    const PANE: Ingredient = &[b(Block::GLASS_PANE)];
    const GRAVEL: Ingredient = &[b(Block::GRAVEL)];
    r.push(shaped(&["###", "###"], &[('#', GLASS)], b(Block::GLASS_PANE), 16));
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        r.push(shaped(&["###", "#D#", "###"], &[('#', GLASS), ('D', &DYES[i])], b(Block::stained_glass(c)), 8));
        r.push(shaped(&["###", "###"], &[('#', &GLASSES[i])], b(Block::stained_pane(c)), 16));
        r.push(shaped(&["###", "#D#", "###"], &[('#', PANE), ('D', &DYES[i])], b(Block::stained_pane(c)), 8));
        r.push(shaped(
            &["###", "#D#", "###"],
            &[('#', TERRACOTTA), ('D', &DYES[i])],
            b(Block::stained_terracotta(c)),
            8,
        ));
        r.push(shapeless(
            &[&DYES[i], SAND, SAND, SAND, SAND, GRAVEL, GRAVEL, GRAVEL, GRAVEL],
            b(Block::concrete_powder(c)),
            8,
        ));
    }
}

/// Build static ingredient tables once; vanilla recolouring excludes the result colour.
const fn color_ingredients(family: u8) -> [[Item; 1]; 16] {
    let mut out = [[Item(0); 1]; 16];
    let mut i = 0;
    while i < 16 {
        let c = crate::color::DyeColor::ALL[i];
        out[i][0] = match family {
            0 => c.dye(),
            1 => Item::from_block(Block::wool(c)),
            2 => Item::from_block(Block::carpet(c)),
            4 => Item::from_block(Block::stained_glass(c)),
            5 => Item::from_block(Block::stained_pane(c)),
            _ => c.bed(),
        };
        i += 1;
    }
    out
}
const fn other_colors(family: u8) -> [[Item; 15]; 16] {
    let source = color_ingredients(family);
    let mut out = [[Item(0); 15]; 16];
    let mut i = 0;
    while i < 16 {
        let mut j = 0;
        while j < 15 {
            out[i][j] = source[j + (j >= i) as usize][0];
            j += 1;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod wool_recipe_tests {
    use super::*;
    use crate::color::DyeColor;
    #[test]
    fn wool_beds_use_one_color_and_recoloring_excludes_the_same_color() {
        for c in DyeColor::ALL {
            let mut g = Grid::new(3);
            for x in 0..3 {
                g.cells[x] = Some(Stack::new(Block::wool(c), 1));
                g.cells[x + 3] = Some(Stack::new(Block::PLANKS, 1));
            }
            assert_eq!(g.result(), Some(Stack::new(c.bed(), 1)));
            g.cells[1] =
                Some(Stack::new(Block::wool(if c == DyeColor::White { DyeColor::Black } else { DyeColor::White }), 1));
            assert_eq!(g.result(), None);
            let mut g = Grid::new(2);
            g.cells[0] = Some(Stack::new(Block::wool(c), 1));
            g.cells[1] = Some(Stack::new(c.dye(), 1));
            assert_eq!(g.result(), None);
            let other = if c == DyeColor::White { DyeColor::Black } else { DyeColor::White };
            g.cells[0] = Some(Stack::new(Block::wool(other), 1));
            assert_eq!(g.result(), Some(Stack::new(Block::wool(c), 1)));
            let mut g = Grid::new(3);
            for x in 0..3 {
                g.cells[x] = Some(Stack::new(Block::wool(c), 1));
            }
            assert_eq!(g.result(), Some(Stack::new(Block::carpet(c), 3)));
        }
        let mut g = Grid::new(2);
        g.cells[1] = Some(Stack::new(Item::IRON_INGOT, 1));
        g.cells[2] = Some(Stack::new(Item::IRON_INGOT, 1));
        assert_eq!(g.result(), Some(Stack::new(Item::SHEARS, 1)));
    }
}

#[cfg(test)]
mod glass_terracotta_recipe_tests {
    use super::*;
    use crate::color::DyeColor;
    #[test]
    fn dyeing_eight_around_a_dye_and_panes_from_six_glass() {
        let mut g = Grid::new(3);
        for i in 0..9 {
            g.cells[i] = Some(Stack::new(Block::GLASS, 1));
        }
        assert_eq!(g.result(), None);
        g.cells[4] = Some(Stack::new(DyeColor::Red.dye(), 1));
        assert_eq!(g.result(), Some(Stack::new(Block::stained_glass(DyeColor::Red), 8)));
        g.cells = [None; 9];
        for i in 0..6 {
            g.cells[i] = Some(Stack::new(Block::stained_glass(DyeColor::Blue), 1));
        }
        assert_eq!(g.result(), Some(Stack::new(Block::stained_pane(DyeColor::Blue), 16)));
        g.cells = [None; 9];
        for i in 0..6 {
            g.cells[i] = Some(Stack::new(Block::GLASS, 1));
        }
        assert_eq!(g.result(), Some(Stack::new(Block::GLASS_PANE, 16)));
        g.cells = [None; 9];
        for i in 0..9 {
            g.cells[i] = Some(Stack::new(Block::TERRACOTTA, 1));
        }
        g.cells[4] = Some(Stack::new(DyeColor::Black.dye(), 1));
        assert_eq!(g.result(), Some(Stack::new(Block::stained_terracotta(DyeColor::Black), 8)));
        g.cells = [None; 9];
        g.cells[0] = Some(Stack::new(DyeColor::White.dye(), 1));
        for i in 1..5 {
            g.cells[i] = Some(Stack::new(Block::SAND, 1));
        }
        for i in 5..9 {
            g.cells[i] = Some(Stack::new(Block::GRAVEL, 1));
        }
        assert_eq!(g.result(), Some(Stack::new(Block::concrete_powder(DyeColor::White), 8)));
    }
}

fn add_redstone_recipes(recipes: &mut Vec<Recipe>) {
    use crate::world::redstone_blocks as r;
    const DUST: Ingredient = &[Item::REDSTONE];
    const TORCH: Ingredient = &[b(r::TORCH)];
    const QUARTZ: Ingredient = &[Item::NETHER_QUARTZ];
    const OAK: Ingredient = &[b(Block::PLANKS)];
    const GLOW: Ingredient = &[b(Block::GLOWSTONE)];
    const REDSTONE_BLOCK: Ingredient = &[b(r::REDSTONE_BLOCK)];
    const SLAB: Ingredient = &[b(Block(108))];
    const HAY: Ingredient = &[b(Block::HAY_BALE)];
    const PISTON: Ingredient = &[b(r::PISTON)];
    const SLIME: Ingredient = &[Item::SLIME_BALL];
    const BOW: Ingredient = &[Item::BOW];
    const CHEST: Ingredient = &[b(Block::CHEST)];
    const CHARCOAL: Ingredient = &[Item::COAL, Item::CHARCOAL];
    const GUNPOWDER: Ingredient = &[Item::GUNPOWDER];
    const BLAZE: Ingredient = &[Item::BLAZE_POWDER];
    recipes.extend([
        shaped(&["s", "c"], &[('s', STICK), ('c', COBBLESTONE)], b(r::LEVER), 1),
        shapeless(&[STONE], b(r::STONE_BUTTON), 1),
        shapeless(&[OAK], b(r::WOOD_BUTTON), 1),
        shaped(&["r", "s"], &[('r', DUST), ('s', STICK)], b(r::TORCH), 1),
        shaped(&["trt", "sss"], &[('t', TORCH), ('r', DUST), ('s', STONE)], b(r::REPEATER), 1),
        shaped(&[" t ", "tqt", "sss"], &[('t', TORCH), ('q', QUARTZ), ('s', STONE)], b(r::COMPARATOR), 1),
        shaped(&[" r ", "rgr", " r "], &[('r', DUST), ('g', GLOW)], b(r::LAMP), 1),
        shaped(&["rrr", "rrr", "rrr"], &[('r', DUST)], b(r::REDSTONE_BLOCK), 1),
        shapeless(&[REDSTONE_BLOCK], Item::REDSTONE, 9),
        shaped(&["ss"], &[('s', STONE)], b(r::STONE_PLATE), 1),
        shaped(&["pp"], &[('p', OAK)], b(r::WOOD_PLATE), 1),
        shaped(&["gg"], &[('g', GOLD)], b(r::LIGHT_PLATE), 1),
        shaped(&["ii"], &[('i', IRON)], b(r::HEAVY_PLATE), 1),
        shaped(&["ii", "ii", "ii"], &[('i', IRON)], b(r::IRON_DOOR), 3),
        shaped(&["ii", "ii"], &[('i', IRON)], b(r::IRON_TRAPDOOR), 1),
        shaped(&["ppp", "ppp"], &[('p', OAK)], b(r::WOOD_TRAPDOOR), 2),
        shaped(&["ggg", "qqq", "sss"], &[('g', GLASS), ('q', QUARTZ), ('s', SLAB)], b(r::DAYLIGHT), 1),
        shaped(&[" r ", "rhr", " r "], &[('r', DUST), ('h', HAY)], b(r::TARGET), 1),
        shaped(&["ccc", "cbc", "crc"], &[('c', COBBLESTONE), ('b', BOW), ('r', DUST)], b(r::DISPENSER), 1),
        shaped(&["ccc", "c c", "crc"], &[('c', COBBLESTONE), ('r', DUST)], b(r::DROPPER), 1),
        shaped(&["i i", "ici", " i "], &[('i', IRON), ('c', CHEST)], b(r::HOPPER), 1),
        shapeless(&[CHARCOAL, GUNPOWDER, BLAZE], Item::FIRE_CHARGE, 3),
        shaped(&["ppp", "cic", "crc"], &[('p', PLANKS), ('c', COBBLESTONE), ('i', IRON), ('r', DUST)], b(r::PISTON), 1),
        shaped(&["s", "p"], &[('s', SLIME), ('p', PISTON)], b(r::STICKY_PISTON), 1),
        shaped(&["ccc", "rrq", "ccc"], &[('c', COBBLESTONE), ('r', DUST), ('q', QUARTZ)], b(r::OBSERVER), 1),
    ]);
}

#[cfg(test)]
mod redstone_recipe_tests {
    use super::*;
    #[test]
    fn every_craftable_redstone_component_has_a_matching_recipe() {
        use crate::world::redstone_blocks as r;
        for id in r::palette_ids().filter(|&id| id != r::WIRE.0) {
            let item = b(Block(id));
            let recipe = recipes().iter().find(|recipe| recipe.result.item == item).expect("redstone recipe");
            let grid = recipe.preview();
            assert_eq!(grid.result().map(|s| s.item), Some(item));
        }
    }
}
