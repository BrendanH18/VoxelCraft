//! Wolf armor's shaped craft and component-preserving dye recipe.
use super::*;
use crate::{color::DyeColor, entity::wolf_armor};
pub(super) fn add(recipes: &mut Vec<Recipe>) {
    recipes.push(shaped(&["X  ", "XXX", "X X"], &[('X', &[Item::ARMADILLO_SCUTE])], Item::WOLF_ARMOR, 1));
    const DYES: Ingredient = &[
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
    let mut dyed = shapeless(&[&[Item::WOLF_ARMOR], DYES], Item::WOLF_ARMOR, 1);
    dyed.result = wolf_armor::dye(dyed.result, &[DyeColor::White]).expect("wolf armor dye preview");
    recipes.push(dyed);
}
pub(super) fn dynamic(stacks: &[Stack]) -> Option<Stack> {
    if let [a, b] = stacks
        && a.item == Item::WOLF_ARMOR
        && b.item == Item::WOLF_ARMOR
        && a.count == 1
        && b.count == 1
    {
        let mut result = Stack::new(Item::WOLF_ARMOR, 1);
        let restored = (64 - a.damage.min(64)) + (64 - b.damage.min(64)) + 64 * 5 / 100;
        result.damage = 64_u16.saturating_sub(restored);
        for curse in [crate::enchant::Enchantment::BindingCurse, crate::enchant::Enchantment::VanishingCurse] {
            result.enchants.set(curse, a.enchants.level(curse).max(b.enchants.level(curse)));
        }
        return Some(result);
    }
    let mut armor = None;
    let mut dyes = Vec::<DyeColor>::new();
    for &stack in stacks {
        if stack.item == Item::WOLF_ARMOR {
            if armor.replace(stack).is_some() {
                return None;
            }
        } else {
            dyes.push(stack.item.dye_color()?);
        }
    }
    wolf_armor::dye(armor?, &dyes)
}
