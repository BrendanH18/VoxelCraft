//! Pinned Java 26.3 melon seed and beetroot soup recipes.
use super::{Recipe, shapeless};
use crate::item::Item;
pub(super) fn add_recipes(r: &mut Vec<Recipe>) {
    use crate::{color::DyeColor, world::sniffer_plants as sp};
    const TORCH: &[Item] = &[Item::from_block(sp::TORCHFLOWER)];
    r.push(shapeless(&[TORCH], DyeColor::Orange.dye(), 1));
    const PITCHER: &[Item] = &[Item::from_block(sp::PITCHER_PLANT)];
    r.push(shapeless(&[PITCHER], DyeColor::Cyan.dye(), 2));
    r.push(shapeless(&[&[Item::MELON_SLICE]], Item::MELON_SEEDS, 1));
    r.push(shapeless(
        &[
            &[Item::BOWL],
            &[Item::BEETROOT],
            &[Item::BEETROOT],
            &[Item::BEETROOT],
            &[Item::BEETROOT],
            &[Item::BEETROOT],
            &[Item::BEETROOT],
        ],
        Item::BEETROOT_SOUP,
        1,
    ));
}
