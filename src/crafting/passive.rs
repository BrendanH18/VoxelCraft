//! Java 26.3 suspicious-stew recipes and their per-flower result components.
use super::*;
use crate::world::overworld_blocks as flowers;
const FLOWERS: [Ingredient; 13] = [
    &[b(crate::world::sniffer_plants::TORCHFLOWER)],
    &[b(Block::DANDELION)],
    &[b(Block::POPPY)],
    &[b(Block::BLUE_ORCHID)],
    &[b(flowers::ALLIUM)],
    &[b(flowers::AZURE_BLUET)],
    &[b(flowers::RED_TULIP)],
    &[b(flowers::ORANGE_TULIP)],
    &[b(flowers::WHITE_TULIP)],
    &[b(flowers::PINK_TULIP)],
    &[b(flowers::OXEYE_DAISY)],
    &[b(flowers::CORNFLOWER)],
    &[b(flowers::LILY_OF_THE_VALLEY)],
];
pub(super) fn add(recipes: &mut Vec<Recipe>) {
    recipes.push(shaped(
        &["f", "c", "s"],
        &[('f', &[Item::FEATHER]), ('c', &[Item::COPPER_INGOT]), ('s', &[Item::STICK])],
        Item::BRUSH,
        1,
    ));
    for flower in FLOWERS {
        // The 26.3 recipes specify #mushrooms twice, permitting the same colour.
        let mut recipe = shapeless(&[BOWL, MUSHROOMS, MUSHROOMS, flower], Item::SUSPICIOUS_STEW, 1);
        recipe.result.entity_data = crate::entity::mooshroom::flower_effect(flower[0]).expect("stew flower");
        recipes.push(recipe);
    }
}
