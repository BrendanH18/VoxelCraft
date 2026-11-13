//! Java 26.3 `crafting_dye` recipes (`DyeRecipe`): one dyeable item and any
//! number of dyes mix into the item's `dyed_color`
//! (`DyedItemColor.applyDyes`), keeping every other component. The five
//! leather recipes are here; wolf armor's sits with its other recipes.
use super::*;
use crate::{
    color::DyeColor,
    inventory::components::{apply_dyes, dyeable},
};

/// The `#dyes` tag, in `DyeColor` order.
pub(super) const DYES: Ingredient = &[
    Item(576),
    Item(577),
    Item(578),
    Item(579),
    Item(580),
    Item(581),
    Item(582),
    Item(583),
    Item(584),
    Item(585),
    Item(586),
    Item(587),
    Item(588),
    Item(589),
    Item(590),
    Item(591),
];

/// The targets of `leather_{helmet,chestplate,leggings,boots,horse_armor}_dyed`,
/// one ingredient each.
const LEATHER: [Ingredient; 5] = [
    &[Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather)],
    &[Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather)],
    &[Item::armor(ArmorPiece::Leggings, ArmorMaterial::Leather)],
    &[Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather)],
    &[Item::LEATHER_HORSE_ARMOR],
];

/// The recipe-book entries: the target and one dye, showing the white
/// demo `SlotDisplay.DyedSlotDemo` starts with. The grid's result comes
/// from [`dynamic`].
pub(super) fn add(recipes: &mut Vec<Recipe>) {
    for ingredient in LEATHER {
        let mut dyed = shapeless(&[ingredient, DYES], ingredient[0], 1);
        dyed.result = apply_dyes(dyed.result, &[DyeColor::White]).expect("dyeable recipe target");
        recipes.push(dyed);
    }
}

/// `DyeRecipe.matches` and `assemble` for every dyeable target: exactly one
/// dyeable item, at least one dye, and nothing else.
pub(super) fn dynamic(stacks: &[Stack]) -> Option<Stack> {
    let mut target = None;
    let mut dyes = Vec::<DyeColor>::new();
    for &stack in stacks {
        if dyeable(stack.item) {
            if target.replace(stack).is_some() {
                return None;
            }
        } else {
            dyes.push(stack.item.dye_color()?);
        }
    }
    apply_dyes(target?, &dyes)
}
