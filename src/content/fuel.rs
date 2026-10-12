//! Java 26.3 furnace fuel ticks for implemented content. No runtime data parsing.
use crate::{
    item::{Item, Tier},
    world::{block::Block, forms, overworld_blocks as ob},
};

/// Normal-furnace ticks, converted to seconds by the furnace adapter.
/// Java's current dried-kelp provider is 4001, not the older rounded 4000.
pub fn ticks(item: Item) -> Option<u32> {
    match item {
        Item::LAVA_BUCKET => return Some(20000),
        Item::COAL | Item::CHARCOAL => return Some(1600),
        Item::BLAZE_ROD => return Some(2400),
        Item::STICK => return Some(100),
        Item::OAK_DOOR => return Some(200),
        i if i.as_tool().is_some_and(|(_, tier)| tier == Tier::Wood) => return Some(200),
        _ => {}
    }
    let block = item.block()?.base();
    if super::burnable_planks(Item::from_block(block)) || block.is_log() {
        return Some(300);
    }
    if let Some(form) = forms::wood_form(block.0) {
        let (index, ticks) = match form {
            forms::WoodForm::Slab { index } => (index, 150),
            forms::WoodForm::Door { index, .. } => (index, 200),
            forms::WoodForm::Stairs { index, .. }
            | forms::WoodForm::Fence { index }
            | forms::WoodForm::Gate { index, .. } => (index, 300),
        };
        return super::burnable_planks(Item::from_block(forms::wood_planks(index))).then_some(ticks);
    }
    match block {
        ob::BAMBOO => Some(50),
        ob::BAMBOO_BLOCK | ob::BAMBOO_MOSAIC => Some(300),
        ob::DRIED_KELP_BLOCK => Some(4001),
        Block::OAK_DOOR => Some(200),
        Block::CRAFTING_TABLE
        | Block::CHEST
        | Block::SMITHING_TABLE
        | Block::BARREL
        | Block::COMPOSTER
        | Block::CARTOGRAPHY_TABLE
        | Block::FLETCHING_TABLE
        | Block::LOOM
        | Block::LECTERN
        | Block::LADDER
        | Block::OAK_FENCE
        | Block::FENCE_GATE => Some(300),
        b if b.slab_base() == Some(Block::PLANKS) => Some(150),
        b if b.stairs_base() == Some(Block::PLANKS) => Some(300),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaped_wood_keeps_its_family_fuel_rules() {
        for wood in 0..9 {
            for (local, expected) in [(0, 300), (4, 150), (5, 300), (6, 300), (14, 200)] {
                assert_eq!(
                    ticks(Item::from_block(forms::wood_id(wood, local))),
                    (wood < 7).then_some(expected),
                    "wood {wood}, form {local}"
                );
            }
        }
    }
}
