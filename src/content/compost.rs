//! Java 26.3 composting metadata for the implemented item catalogue.
//! Static IDs and typed classes keep gameplay lookups independent of names and assets.
use crate::{
    item::Item,
    world::{biome_plants as bp, block::Block, nether_biome_blocks as nb, overworld_blocks as ob},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompostClass {
    Low,
    Half,
    Medium,
    High,
    Always,
}
impl CompostClass {
    pub const fn percent(self) -> u8 {
        match self {
            Self::Low => 30,
            Self::Half => 50,
            Self::Medium => 65,
            Self::High => 85,
            Self::Always => 100,
        }
    }
}
const CHANCE_30: &[Item] = &[
    Item::MELON_SEEDS,
    Item::BEETROOT_SEEDS,
    Item::WHEAT_SEEDS,
    Item::PUMPKIN_SEEDS,
    Item::GLOW_BERRIES,
    Item::SWEET_BERRIES,
    Item::DRIED_KELP,
    Item::from_block(Block::SPRUCE_LEAVES),
    Item::from_block(Block::TALL_GRASS),
    Item::from_block(Block::OAK_SAPLING),
    Item::from_block(Block::SPRUCE_SAPLING),
    Item::from_block(Block::BIRCH_LEAVES),
    Item::from_block(Block::JUNGLE_LEAVES),
    Item::from_block(Block::ACACIA_LEAVES),
    Item::from_block(Block::BIRCH_SAPLING),
    Item::from_block(Block::JUNGLE_SAPLING),
    Item::from_block(Block::ACACIA_SAPLING),
    Item::from_block(ob::MOSS_CARPET),
    Item::from_block(ob::PALE_MOSS_CARPET),
    Item::from_block(ob::AZALEA_LEAVES),
    Item::from_block(ob::HANGING_ROOTS),
    Item::from_block(ob::SMALL_DRIPLEAF),
    Item::from_block(ob::KELP),
    Item::from_block(ob::SEAGRASS),
    Item::from_block(ob::DARK_OAK_LEAVES),
    Item::from_block(ob::MANGROVE_LEAVES),
    Item::from_block(ob::CHERRY_LEAVES),
    Item::from_block(ob::PALE_OAK_LEAVES),
    Item::from_block(ob::DARK_OAK_SAPLING),
    Item::from_block(ob::MANGROVE_PROPAGULE),
    Item::from_block(ob::CHERRY_SAPLING),
    Item::from_block(ob::PALE_OAK_SAPLING),
    Item::from_block(ob::MANGROVE_ROOTS),
    Item::from_block(ob::PINK_PETALS),
    Item::from_block(ob::PALE_HANGING_MOSS_TIP),
    Item::from_block(ob::PALE_HANGING_MOSS),
    Item::from_block(bp::LEAF_LITTER),
    Item::from_block(bp::WILDFLOWERS),
    Item::from_block(bp::BUSH),
    Item::from_block(bp::FIREFLY_BUSH),
    Item::from_block(bp::SHORT_DRY_GRASS),
    Item::from_block(bp::TALL_DRY_GRASS),
    Item::from_block(bp::CACTUS_FLOWER),
];
const CHANCE_50: &[Item] = &[
    Item::MELON_SLICE,
    Item::from_block(Block::CACTUS),
    Item::from_block(Block::SUGAR_CANE),
    Item::from_block(ob::FLOWERING_AZALEA_LEAVES),
    Item::from_block(crate::world::cave_blocks::GLOW_LICHEN),
    Item::from_block(ob::DRIED_KELP_BLOCK),
    Item::from_block(ob::DOUBLE_TALL_GRASS),
    Item::from_block(ob::VINE),
    Item::from_block(nb::NETHER_SPROUTS),
    Item::from_block(nb::WEEPING_VINES),
    Item::from_block(nb::TWISTING_VINES),
];
const CHANCE_65: &[Item] = &[
    Item::CARROT,
    Item::POTATO,
    Item::APPLE,
    Item::WHEAT,
    Item::BEETROOT,
    Item::NETHER_WART,
    Item::COCOA_BEANS,
    Item::from_block(Block::BROWN_MUSHROOM),
    Item::from_block(Block::RED_MUSHROOM),
    Item::from_block(Block::DANDELION),
    Item::from_block(Block::POPPY),
    Item::from_block(Block::PUMPKIN),
    Item::from_block(Block::MELON),
    Item::from_block(Block::FERN),
    Item::from_block(Block::BLUE_ORCHID),
    Item::from_block(Block::NETHER_WART),
    Item::from_block(Block::CARVED_PUMPKIN),
    Item::from_block(ob::MOSS_BLOCK),
    Item::from_block(ob::PALE_MOSS_BLOCK),
    Item::from_block(ob::AZALEA),
    Item::from_block(ob::SPORE_BLOSSOM),
    Item::from_block(ob::BIG_DRIPLEAF),
    Item::from_block(ob::SEA_PICKLE),
    Item::from_block(ob::CORNFLOWER),
    Item::from_block(ob::LILY_OF_THE_VALLEY),
    Item::from_block(ob::OXEYE_DAISY),
    Item::from_block(ob::AZURE_BLUET),
    Item::from_block(ob::ALLIUM),
    Item::from_block(ob::RED_TULIP),
    Item::from_block(ob::ORANGE_TULIP),
    Item::from_block(ob::WHITE_TULIP),
    Item::from_block(ob::PINK_TULIP),
    Item::from_block(ob::SUNFLOWER),
    Item::from_block(ob::LILAC),
    Item::from_block(ob::ROSE_BUSH),
    Item::from_block(ob::PEONY),
    Item::from_block(ob::LARGE_FERN),
    Item::from_block(ob::LILY_PAD),
    Item::from_block(ob::MUSHROOM_STEM),
    Item::from_block(nb::SHROOMLIGHT),
    Item::from_block(nb::CRIMSON_FUNGUS),
    Item::from_block(nb::WARPED_FUNGUS),
    Item::from_block(nb::CRIMSON_ROOTS),
    Item::from_block(nb::WARPED_ROOTS),
];
const CHANCE_85: &[Item] = &[
    Item::COOKIE,
    Item::BAKED_POTATO,
    Item::BREAD,
    Item::from_block(Block::HAY_BALE),
    Item::from_block(ob::FLOWERING_AZALEA),
    Item::from_block(ob::RED_MUSHROOM_BLOCK),
    Item::from_block(ob::BROWN_MUSHROOM_BLOCK),
    Item::from_block(nb::NETHER_WART_BLOCK),
    Item::from_block(nb::WARPED_WART_BLOCK),
    // `Items.STRAW_BED`: `COMPOSTABLE_MEDIUM_HIGH`.
    Item::from_block(crate::world::colour_blocks::straw_bed::ITEM_BLOCK),
];
const CHANCE_100: &[Item] = &[Item::CAKE, Item::PUMPKIN_PIE];
pub fn class(item: Item) -> Option<CompostClass> {
    if CHANCE_100.contains(&item) {
        return Some(CompostClass::Always);
    }
    if CHANCE_85.contains(&item) {
        return Some(CompostClass::High);
    }
    if CHANCE_65.contains(&item) {
        return Some(CompostClass::Medium);
    }
    if CHANCE_50.contains(&item) {
        return Some(CompostClass::Half);
    }
    if CHANCE_30.contains(&item) {
        return Some(CompostClass::Low);
    }
    item.block().filter(|b| b.is_leaves() || b.is_sapling()).map(|_| CompostClass::Low)
}
pub fn chance(item: Item) -> f32 {
    class(item).map_or(0.0, |c| f32::from(c.percent()) / 100.0)
}
