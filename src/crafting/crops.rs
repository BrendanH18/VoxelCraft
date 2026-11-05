//! Pinned Java 26.3 melon seed and beetroot soup recipes.
use super::{Recipe, shapeless};
use crate::item::Item;
pub(super) fn add_recipes(r: &mut Vec<Recipe>) {
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
