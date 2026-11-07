//! v0.16 Track D recipes: Java 26.3 `recipe/{wooden,stone,copper,iron,golden,diamond}_spear.json`
//! (`"  X", " # ", "#  "` with the tier's `#*_tool_materials` and sticks) and
//! `recipe/mace.json` (a heavy core over a breeze rod). The netherite spear is
//! a smithing upgrade (`crate::smithing`).

use super::{Ingredient, PLANKS, Recipe, STICK, STONE_CRAFTING, b, shaped};
use crate::item::{Item, Tier};
use crate::weapons::spear;

pub(super) fn add(r: &mut Vec<Recipe>) {
    const MATERIALS: [(Tier, Ingredient); 6] = [
        (Tier::Wood, PLANKS),
        (Tier::Stone, STONE_CRAFTING),
        (Tier::Copper, &[Item::COPPER_INGOT]),
        (Tier::Iron, &[Item::IRON_INGOT]),
        (Tier::Gold, &[Item::GOLD_INGOT]),
        (Tier::Diamond, &[Item::DIAMOND]),
    ];
    for (tier, material) in MATERIALS {
        r.push(shaped(&["  X", " # ", "#  "], &[('X', material), ('#', STICK)], spear(tier), 1));
    }
    // The breeze rod is Track C's item: the recipe exists once it is registered.
    if let Some(rod) = crate::weapons::breeze_rod() {
        let rod: Ingredient = Box::leak(Box::new([rod]));
        const CORE: Ingredient = &[b(crate::world::heavy_core::HEAVY_CORE)];
        r.push(shaped(&["#", "I"], &[('#', CORE), ('I', rod)], crate::weapons::MACE, 1));
    }
}
