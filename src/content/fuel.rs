//! Java 26.3 cooking-fuel metadata for implemented content. No runtime data parsing.
use crate::{
    item::{Item, Tier},
    world::{block::Block, forms, gadgets, overworld_blocks as ob, redstone_blocks as rb},
};

/// Typed provider classes; normal/fast furnace adapters share these exact ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuelClass {
    Bamboo,
    /// `COOKING_TIME_WOOL_SLABS`: wool slabs (and cushions).
    WoolSlab,
    Carpet,
    /// Hanging signs (Java 26.3: 800 ticks).
    HangingSign,
    Small,
    Slab,
    Large,
    Wood,
    Boat,
    Coal,
    BlazeRod,
    DriedKelp,
    Lava,
}
impl FuelClass {
    pub const fn ticks(self) -> u32 {
        match self {
            Self::Bamboo | Self::WoolSlab => 50,
            Self::Carpet => 67,
            Self::HangingSign => 800,
            Self::Small => 100,
            Self::Slab => 150,
            Self::Large => 200,
            Self::Wood => 300,
            Self::Boat => 1200,
            Self::Coal => 1600,
            Self::BlazeRod => 2400,
            Self::DriedKelp => 4001,
            Self::Lava => 20000,
        }
    }
}

pub fn class(item: Item) -> Option<FuelClass> {
    use FuelClass::*;
    match item {
        Item::SCAFFOLDING => return Some(Bamboo),
        Item::LAVA_BUCKET => return Some(Lava),
        Item::COAL | Item::CHARCOAL => return Some(Coal),
        Item::BLAZE_ROD => return Some(BlazeRod),
        Item::STICK | Item::BOWL => return Some(Small),
        // `Items.CUSHION`: `COOKING_TIME_WOOL_SLABS`.
        i if i.cushion_color().is_some() => return Some(WoolSlab),
        Item::BOW | Item::FISHING_ROD | Item::CROSSBOW | Item::CHARGED_CROSSBOW => return Some(Wood),
        Item::OAK_DOOR => return Some(Large),
        i if i.as_tool().is_some_and(|(_, tier)| tier == Tier::Wood) => return Some(Large),
        // Append-only v0.7 boat/raft and chest-boat item range. Minecarts do not burn.
        Item(960..=979) => return Some(Boat),
        _ => {}
    }
    let block = item.block()?.base();
    if let Some(class) = crate::world::wood::fuel(block) {
        return Some(class);
    }
    if super::burnable_planks(Item::from_block(block)) || block.is_log() {
        return Some(Wood);
    }
    if block.is_sapling() || matches!(block, ob::AZALEA | ob::FLOWERING_AZALEA | Block::DEAD_BUSH) {
        return Some(Small);
    }
    if block.wool_color().is_some() {
        return Some(Small);
    }
    if block.carpet_color().is_some() {
        return Some(Carpet);
    }
    // v0.12 colour blocks: wool stairs burn as wool, wool slabs half as
    // long, and banners as wood (`Items.WOOL_STAIRS`, `WOOL_SLAB`, `BANNER`).
    if crate::world::colour_blocks::banner::decode(block.0).is_some() {
        return Some(Wood);
    }
    if block.stairs_base().is_some_and(|b| b.wool_color().is_some()) {
        return Some(Small);
    }
    if block.slab_base().is_some_and(|b| b.wool_color().is_some()) {
        return Some(WoolSlab);
    }
    if let Some((oak, wood)) = crate::world::nether_biome_blocks::switch_oak(block.0) {
        // Crimson/warped and polished blackstone switches are explicitly
        // non-fuels; the other woods burn.
        return (2..=10)
            .contains(&wood)
            .then_some(if matches!(rb::component(oak), Some(rb::Component::Button { .. })) { Small } else { Wood });
    }
    if let Some(form) = forms::wood_form(block.0) {
        let (index, class) = match form {
            forms::WoodForm::Slab { index } => (index, Slab),
            forms::WoodForm::Door { index, .. } => (index, Large),
            forms::WoodForm::Stairs { index, .. }
            | forms::WoodForm::Fence { index }
            | forms::WoodForm::Gate { index, .. } => (index, Wood),
        };
        return (index == 11 || super::burnable_planks(Item::from_block(forms::wood_planks(index)))).then_some(class);
    }
    match block {
        ob::BAMBOO => Some(Bamboo),
        ob::BAMBOO_BLOCK | ob::BAMBOO_MOSAIC | ob::MANGROVE_ROOTS => Some(Wood),
        ob::DRIED_KELP_BLOCK => Some(DriedKelp),
        Block::OAK_DOOR => Some(Large),
        rb::WOOD_BUTTON => Some(Small),
        Block::CRAFTING_TABLE
        | Block::CHEST
        | Block::BOOKSHELF
        | Block::SMITHING_TABLE
        | Block::BARREL
        | Block::COMPOSTER
        | Block::CARTOGRAPHY_TABLE
        | Block::FLETCHING_TABLE
        | Block::LOOM
        | Block::LECTERN
        | Block::LADDER
        | Block::OAK_FENCE
        | Block::FENCE_GATE
        | rb::WOOD_TRAPDOOR
        | rb::WOOD_PLATE
        | rb::DAYLIGHT
        | gadgets::NOTE => Some(Wood),
        b if b.slab_base() == Some(Block::PLANKS) => Some(Slab),
        b if b.stairs_base() == Some(Block::PLANKS) => Some(Wood),
        _ => None,
    }
}

/// Normal-furnace ticks, converted to seconds by the furnace adapter.
pub fn ticks(item: Item) -> Option<u32> {
    class(item).map(FuelClass::ticks)
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
