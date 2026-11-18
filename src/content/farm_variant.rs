//! v0.15 Track D: the climate variants of chickens, cows and pigs (Java
//! 26.3 `TemperatureVariants`, `ChickenVariants`, `CowVariants`,
//! `PigVariants` and `data/minecraft/{chicken,cow,pig}_variant/*.json`),
//! and the blue and brown eggs.
//!
//! A farm animal picks its variant when it spawns
//! (`VariantUtils.selectVariantToSpawn`): warm in
//! `#spawns_warm_variant_farm_animals` biomes, cold in
//! `#spawns_cold_variant_farm_animals`, temperate otherwise (priority 0).
//! Babies bred from two parents take either parent's variant at random. Warm
//! chickens lay brown eggs and cold ones blue (`gameplay/chicken_lay`), and a
//! thrown egg's chick hatches with the egg's variant (`ThrownEgg.onHit`).
//! Saves keep the variant's registry name.

use crate::item::{Item, ItemInfo, ItemKind, Sprite};

/// `TemperatureVariants`: temperate (the default), warm and cold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FarmVariant {
    #[default]
    Temperate = 0,
    Warm = 1,
    Cold = 2,
}

impl FarmVariant {
    pub const ALL: [FarmVariant; 3] = [FarmVariant::Temperate, FarmVariant::Warm, FarmVariant::Cold];

    /// The registry name, without `minecraft:`.
    pub fn name(self) -> &'static str {
        match self {
            FarmVariant::Temperate => "temperate",
            FarmVariant::Warm => "warm",
            FarmVariant::Cold => "cold",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Self::ALL.into_iter().find(|v| v.name() == name)
    }

    /// The variant stored in `animals::State::variant` (0 for older saves).
    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(usize::from(i)).copied().unwrap_or_default()
    }

    /// The egg a chicken of this variant lays and that hatches one.
    pub fn egg(self) -> Item {
        match self {
            FarmVariant::Temperate => Item::EGG,
            FarmVariant::Warm => BROWN_EGG,
            FarmVariant::Cold => BLUE_EGG,
        }
    }
}

/// The biomes of `#spawns_warm_variant_farm_animals`, nested tags expanded
/// (`is_jungle`, `is_savanna`, `is_nether`, `is_badlands`).
pub const WARM_BIOMES: [&str; 19] = [
    "desert",
    "warm_ocean",
    "bamboo_jungle",
    "jungle",
    "sparse_jungle",
    "savanna",
    "savanna_plateau",
    "windswept_savanna",
    "nether_wastes",
    "soul_sand_valley",
    "crimson_forest",
    "warped_forest",
    "basalt_deltas",
    "badlands",
    "eroded_badlands",
    "wooded_badlands",
    "mangrove_swamp",
    "deep_lukewarm_ocean",
    "lukewarm_ocean",
];

/// The biomes of `#spawns_cold_variant_farm_animals`, `is_end` expanded.
pub const COLD_BIOMES: [&str; 27] = [
    "snowy_plains",
    "ice_spikes",
    "frozen_peaks",
    "jagged_peaks",
    "snowy_slopes",
    "frozen_ocean",
    "deep_frozen_ocean",
    "grove",
    "deep_dark",
    "frozen_river",
    "snowy_taiga",
    "snowy_beach",
    "the_end",
    "end_highlands",
    "end_midlands",
    "small_end_islands",
    "end_barrens",
    "cold_ocean",
    "deep_cold_ocean",
    "old_growth_pine_taiga",
    "old_growth_spruce_taiga",
    "taiga",
    "windswept_forest",
    "windswept_gravelly_hills",
    "windswept_hills",
    "stony_peaks",
    "dappled_forest",
];

/// `VariantUtils.selectVariantToSpawn` for a biome's Java id: the highest
/// priority variant whose biome check passes; temperate is the fallback.
pub fn for_biome(biome: &str) -> FarmVariant {
    let biome = biome.strip_prefix("minecraft:").unwrap_or(biome);
    if WARM_BIOMES.contains(&biome) {
        FarmVariant::Warm
    } else if COLD_BIOMES.contains(&biome) {
        FarmVariant::Cold
    } else {
        FarmVariant::Temperate
    }
}

/// The variant of an animal spawning in `dimension` at `biome`: the Nether's
/// biomes are all warm and the End's all cold.
pub fn for_spawn(dimension: crate::world::terrain::Dimension, biome: crate::world::terrain::Biome) -> FarmVariant {
    use crate::world::terrain::Dimension;
    match dimension {
        Dimension::Nether => FarmVariant::Warm,
        Dimension::End => FarmVariant::Cold,
        Dimension::Overworld => for_biome(biome.name()),
    }
}

/// `Items.BLUE_EGG` and `Items.BROWN_EGG`.
pub const BLUE_EGG: Item = Item(1559);
pub const BROWN_EGG: Item = Item(1560);

/// The chicken variant an egg item hatches (`DataComponents.CHICKEN_VARIANT`).
pub fn egg_variant(item: Item) -> Option<FarmVariant> {
    match item {
        Item::EGG => Some(FarmVariant::Temperate),
        BROWN_EGG => Some(FarmVariant::Warm),
        BLUE_EGG => Some(FarmVariant::Cold),
        _ => None,
    }
}

/// Java's `#eggs`.
pub fn is_egg(item: Item) -> bool {
    egg_variant(item).is_some()
}

/// The shell colour of each egg's icon.
pub const BLUE_SHELL: [u8; 3] = [150, 196, 220];
pub const BROWN_SHELL: [u8; 3] = [186, 134, 88];

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    let (name, shell) = match item {
        BLUE_EGG => ("blue egg", BLUE_SHELL),
        BROWN_EGG => ("brown egg", BROWN_SHELL),
        _ => return None,
    };
    Some(ItemInfo { name, kind: ItemKind::Material, max_stack: 16, sprite: Sprite::TintedEgg(shell) })
}

/// The body and leg tints each variant's original look uses (temperate
/// keeps the classic colours): warm animals are ruddier, cold ones paler
/// with a bluish cast.
pub fn tint(variant: FarmVariant, rgb: [u8; 3]) -> [u8; 3] {
    let scale =
        |c: [u8; 3], k: [f32; 3]| -> [u8; 3] { std::array::from_fn(|i| (f32::from(c[i]) * k[i]).min(255.0) as u8) };
    match variant {
        FarmVariant::Temperate => rgb,
        FarmVariant::Warm => scale(rgb, [1.05, 0.82, 0.64]),
        FarmVariant::Cold => scale(rgb, [0.88, 0.95, 1.08]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eggs_and_variants_round_trip() {
        for v in FarmVariant::ALL {
            assert_eq!(FarmVariant::from_name(v.name()), Some(v));
            assert_eq!(FarmVariant::from_index(v as u8), v);
            assert_eq!(egg_variant(v.egg()), Some(v));
        }
        assert_eq!(BLUE_EGG.name(), "blue egg");
        assert_eq!(Item::from_name("brown_egg"), Some(BROWN_EGG));
        assert_eq!((BLUE_EGG.max_stack(), BROWN_EGG.max_stack()), (16, 16));
        assert_eq!(for_biome("minecraft:savanna"), FarmVariant::Warm);
        assert_eq!(for_biome("taiga"), FarmVariant::Cold);
        assert_eq!(for_biome("plains"), FarmVariant::Temperate);
    }
}
