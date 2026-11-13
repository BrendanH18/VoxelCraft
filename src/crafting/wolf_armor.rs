//! Wolf armor's shaped craft, paired repair and dye recipe entry (the dye
//! grid itself is the shared [`super::dyed::dynamic`]).
use super::*;
use crate::{color::DyeColor, entity::wolf_armor};
pub(super) fn add(recipes: &mut Vec<Recipe>) {
    recipes.push(shaped(&["X  ", "XXX", "X X"], &[('X', &[Item::ARMADILLO_SCUTE])], Item::WOLF_ARMOR, 1));
    let mut dyed = shapeless(&[&[Item::WOLF_ARMOR], super::dyed::DYES], Item::WOLF_ARMOR, 1);
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
    None
}
