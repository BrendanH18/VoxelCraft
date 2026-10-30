//! Java 26.3 biome spawn data for existing non-monster categories.
//! See tests/fixtures/java/26.3/spawn_categories.json for the extracted reference.
use super::MobKind;
use super::category::MobCategory;
use super::monster_spawns::{JavaMonster as J, MonsterSpawn, e};
use crate::world::terrain::Biome;

const LIST_0: [MonsterSpawn; 1] = [e(J::Missing("polar_bear"), 1, 1, 2)];
const LIST_1: [MonsterSpawn; 1] = [e(J::Missing("bat"), 10, 8, 8)];
const LIST_2: [MonsterSpawn; 1] = [e(J::Known(MobKind::GlowSquid), 10, 4, 6)];
const LIST_3: [MonsterSpawn; 2] = [e(J::Known(MobKind::Squid), 1, 1, 4), e(J::Missing("nautilus"), 2, 1, 1)];
const LIST_4: [MonsterSpawn; 1] = [e(J::Known(MobKind::Salmon), 15, 1, 5)];
const LIST_5: [MonsterSpawn; 2] = [e(J::Known(MobKind::Squid), 3, 1, 4), e(J::Missing("nautilus"), 2, 1, 1)];
const LIST_6: [MonsterSpawn; 2] = [e(J::Known(MobKind::Cod), 15, 3, 6), e(J::Known(MobKind::Salmon), 15, 1, 5)];
const LIST_7: [MonsterSpawn; 3] =
    [e(J::Known(MobKind::Squid), 1, 1, 4), e(J::Known(MobKind::Dolphin), 1, 1, 2), e(J::Missing("nautilus"), 5, 1, 1)];
const LIST_8: [MonsterSpawn; 1] = [e(J::Known(MobKind::Cod), 10, 3, 6)];
const LIST_9: [MonsterSpawn; 3] =
    [e(J::Known(MobKind::Squid), 8, 1, 4), e(J::Known(MobKind::Dolphin), 2, 1, 2), e(J::Missing("nautilus"), 5, 1, 1)];
const LIST_10: [MonsterSpawn; 3] = [
    e(J::Known(MobKind::Cod), 8, 3, 6),
    e(J::Known(MobKind::Pufferfish), 5, 1, 3),
    e(J::Known(MobKind::TropicalFish), 25, 8, 8),
];
const LIST_11: [MonsterSpawn; 3] =
    [e(J::Known(MobKind::Squid), 10, 1, 2), e(J::Known(MobKind::Dolphin), 2, 1, 2), e(J::Missing("nautilus"), 5, 1, 1)];
const LIST_12: [MonsterSpawn; 3] = [
    e(J::Known(MobKind::Cod), 15, 3, 6),
    e(J::Known(MobKind::Pufferfish), 5, 1, 3),
    e(J::Known(MobKind::TropicalFish), 25, 8, 8),
];
const LIST_13: [MonsterSpawn; 3] =
    [e(J::Missing("nautilus"), 5, 1, 1), e(J::Known(MobKind::Squid), 10, 4, 4), e(J::Known(MobKind::Dolphin), 2, 1, 2)];
const LIST_14: [MonsterSpawn; 2] =
    [e(J::Known(MobKind::Pufferfish), 15, 1, 3), e(J::Known(MobKind::TropicalFish), 25, 8, 8)];
const LIST_15: [MonsterSpawn; 1] = [e(J::Missing("mooshroom"), 8, 4, 8)];
const LIST_16: [MonsterSpawn; 1] = [e(J::Known(MobKind::Squid), 2, 1, 4)];
const LIST_17: [MonsterSpawn; 1] = [e(J::Known(MobKind::Salmon), 5, 1, 5)];
const LIST_18: [MonsterSpawn; 1] = [e(J::Known(MobKind::Turtle), 5, 2, 5)];
const LIST_19: [MonsterSpawn; 6] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Horse), 5, 2, 6),
    e(J::Known(MobKind::Donkey), 1, 1, 3),
];
const LIST_20: [MonsterSpawn; 2] = [e(J::Known(MobKind::Rabbit), 10, 2, 3), e(J::Missing("polar_bear"), 1, 1, 2)];
const LIST_21: [MonsterSpawn; 5] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Wolf), 5, 4, 4),
];
const LIST_22: [MonsterSpawn; 5] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Rabbit), 4, 2, 3),
];
const LIST_23: [MonsterSpawn; 4] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
];
const LIST_24: [MonsterSpawn; 7] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Wolf), 8, 4, 4),
    e(J::Known(MobKind::Rabbit), 4, 2, 3),
    e(J::Known(MobKind::Fox), 8, 2, 4),
];
const LIST_25: [MonsterSpawn; 5] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Missing("frog"), 10, 2, 5),
];
const LIST_26: [MonsterSpawn; 1] = [e(J::Missing("frog"), 10, 2, 5)];
const LIST_27: [MonsterSpawn; 1] = [e(J::Known(MobKind::TropicalFish), 25, 8, 8)];
const LIST_28: [MonsterSpawn; 2] = [e(J::Known(MobKind::Rabbit), 12, 2, 3), e(J::Known(MobKind::Camel), 1, 1, 1)];
const LIST_29: [MonsterSpawn; 7] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Horse), 1, 2, 6),
    e(J::Known(MobKind::Donkey), 1, 1, 1),
    e(J::Missing("armadillo"), 10, 2, 3),
];
const LIST_30: [MonsterSpawn; 9] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Horse), 1, 2, 6),
    e(J::Known(MobKind::Donkey), 1, 1, 1),
    e(J::Missing("armadillo"), 10, 2, 3),
    e(J::Known(MobKind::Llama), 8, 4, 4),
    e(J::Known(MobKind::Wolf), 8, 4, 8),
];
const LIST_31: [MonsterSpawn; 7] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Parrot), 40, 1, 2),
    e(J::Missing("panda"), 1, 1, 2),
];
const LIST_32: [MonsterSpawn; 6] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Wolf), 8, 2, 4),
];
const LIST_33: [MonsterSpawn; 7] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Parrot), 40, 1, 2),
    e(J::Missing("panda"), 80, 1, 2),
];
const LIST_34: [MonsterSpawn; 5] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Missing("armadillo"), 6, 1, 2),
];
const LIST_35: [MonsterSpawn; 6] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Missing("armadillo"), 6, 1, 2),
    e(J::Known(MobKind::Wolf), 2, 4, 8),
];
const LIST_36: [MonsterSpawn; 3] = [
    e(J::Known(MobKind::Donkey), 1, 1, 2),
    e(J::Known(MobKind::Rabbit), 2, 2, 6),
    e(J::Known(MobKind::Sheep), 2, 2, 4),
];
const LIST_37: [MonsterSpawn; 3] =
    [e(J::Known(MobKind::Pig), 1, 1, 2), e(J::Known(MobKind::Rabbit), 2, 2, 6), e(J::Known(MobKind::Sheep), 2, 2, 4)];
const LIST_38: [MonsterSpawn; 3] =
    [e(J::Known(MobKind::Wolf), 1, 1, 1), e(J::Known(MobKind::Rabbit), 8, 2, 3), e(J::Known(MobKind::Fox), 4, 2, 4)];
const LIST_39: [MonsterSpawn; 2] = [e(J::Known(MobKind::Rabbit), 4, 2, 3), e(J::Known(MobKind::Goat), 5, 1, 3)];
const LIST_40: [MonsterSpawn; 1] = [e(J::Known(MobKind::Goat), 5, 1, 3)];
const LIST_41: [MonsterSpawn; 5] = [
    e(J::Known(MobKind::Sheep), 12, 4, 4),
    e(J::Known(MobKind::Pig), 10, 4, 4),
    e(J::Known(MobKind::Chicken), 10, 4, 4),
    e(J::Known(MobKind::Cow), 8, 4, 4),
    e(J::Known(MobKind::Llama), 5, 4, 6),
];
const LIST_42: [MonsterSpawn; 1] = [e(J::Known(MobKind::Axolotl), 10, 4, 6)];
pub(super) fn biome_list(biome: Biome, category: MobCategory) -> &'static [MonsterSpawn] {
    match (biome, category) {
        (Biome::DeepFrozenOcean, MobCategory::Creature) => &LIST_0,
        (Biome::DeepFrozenOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::DeepFrozenOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::DeepFrozenOcean, MobCategory::WaterCreature) => &LIST_3,
        (Biome::DeepFrozenOcean, MobCategory::WaterAmbient) => &LIST_4,
        (Biome::DeepColdOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::DeepColdOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::DeepColdOcean, MobCategory::WaterCreature) => &LIST_5,
        (Biome::DeepColdOcean, MobCategory::WaterAmbient) => &LIST_6,
        (Biome::DeepOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::DeepOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::DeepOcean, MobCategory::WaterCreature) => &LIST_7,
        (Biome::DeepOcean, MobCategory::WaterAmbient) => &LIST_8,
        (Biome::DeepLukewarmOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::DeepLukewarmOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::DeepLukewarmOcean, MobCategory::WaterCreature) => &LIST_9,
        (Biome::DeepLukewarmOcean, MobCategory::WaterAmbient) => &LIST_10,
        (Biome::FrozenOcean, MobCategory::Creature) => &LIST_0,
        (Biome::FrozenOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::FrozenOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::FrozenOcean, MobCategory::WaterCreature) => &LIST_3,
        (Biome::FrozenOcean, MobCategory::WaterAmbient) => &LIST_4,
        (Biome::ColdOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::ColdOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::ColdOcean, MobCategory::WaterCreature) => &LIST_5,
        (Biome::ColdOcean, MobCategory::WaterAmbient) => &LIST_6,
        (Biome::Ocean, MobCategory::Ambient) => &LIST_1,
        (Biome::Ocean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Ocean, MobCategory::WaterCreature) => &LIST_7,
        (Biome::Ocean, MobCategory::WaterAmbient) => &LIST_8,
        (Biome::LukewarmOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::LukewarmOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::LukewarmOcean, MobCategory::WaterCreature) => &LIST_11,
        (Biome::LukewarmOcean, MobCategory::WaterAmbient) => &LIST_12,
        (Biome::WarmOcean, MobCategory::Ambient) => &LIST_1,
        (Biome::WarmOcean, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::WarmOcean, MobCategory::WaterCreature) => &LIST_13,
        (Biome::WarmOcean, MobCategory::WaterAmbient) => &LIST_14,
        (Biome::MushroomFields, MobCategory::Creature) => &LIST_15,
        (Biome::MushroomFields, MobCategory::Ambient) => &LIST_1,
        (Biome::MushroomFields, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::River, MobCategory::Ambient) => &LIST_1,
        (Biome::River, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::River, MobCategory::WaterCreature) => &LIST_16,
        (Biome::River, MobCategory::WaterAmbient) => &LIST_17,
        (Biome::FrozenRiver, MobCategory::Ambient) => &LIST_1,
        (Biome::FrozenRiver, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::FrozenRiver, MobCategory::WaterCreature) => &LIST_16,
        (Biome::FrozenRiver, MobCategory::WaterAmbient) => &LIST_17,
        (Biome::Beach, MobCategory::Creature) => &LIST_18,
        (Biome::Beach, MobCategory::Ambient) => &LIST_1,
        (Biome::Beach, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SnowyBeach, MobCategory::Ambient) => &LIST_1,
        (Biome::SnowyBeach, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::StonyShore, MobCategory::Ambient) => &LIST_1,
        (Biome::StonyShore, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Plains, MobCategory::Creature) => &LIST_19,
        (Biome::Plains, MobCategory::Ambient) => &LIST_1,
        (Biome::Plains, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SunflowerPlains, MobCategory::Creature) => &LIST_19,
        (Biome::SunflowerPlains, MobCategory::Ambient) => &LIST_1,
        (Biome::SunflowerPlains, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SnowyPlains, MobCategory::Creature) => &LIST_20,
        (Biome::SnowyPlains, MobCategory::Ambient) => &LIST_1,
        (Biome::SnowyPlains, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::IceSpikes, MobCategory::Creature) => &LIST_20,
        (Biome::IceSpikes, MobCategory::Ambient) => &LIST_1,
        (Biome::IceSpikes, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Forest, MobCategory::Creature) => &LIST_21,
        (Biome::Forest, MobCategory::Ambient) => &LIST_1,
        (Biome::Forest, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::FlowerForest, MobCategory::Creature) => &LIST_22,
        (Biome::FlowerForest, MobCategory::Ambient) => &LIST_1,
        (Biome::FlowerForest, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::BirchForest, MobCategory::Creature) => &LIST_23,
        (Biome::BirchForest, MobCategory::Ambient) => &LIST_1,
        (Biome::BirchForest, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::OldGrowthBirchForest, MobCategory::Creature) => &LIST_23,
        (Biome::OldGrowthBirchForest, MobCategory::Ambient) => &LIST_1,
        (Biome::OldGrowthBirchForest, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::DarkForest, MobCategory::Creature) => &LIST_23,
        (Biome::DarkForest, MobCategory::Ambient) => &LIST_1,
        (Biome::DarkForest, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::PaleGarden, MobCategory::Ambient) => &LIST_1,
        (Biome::PaleGarden, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Taiga, MobCategory::Creature) => &LIST_24,
        (Biome::Taiga, MobCategory::Ambient) => &LIST_1,
        (Biome::Taiga, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SnowyTaiga, MobCategory::Creature) => &LIST_24,
        (Biome::SnowyTaiga, MobCategory::Ambient) => &LIST_1,
        (Biome::SnowyTaiga, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::OldGrowthPineTaiga, MobCategory::Creature) => &LIST_24,
        (Biome::OldGrowthPineTaiga, MobCategory::Ambient) => &LIST_1,
        (Biome::OldGrowthPineTaiga, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::OldGrowthSpruceTaiga, MobCategory::Creature) => &LIST_24,
        (Biome::OldGrowthSpruceTaiga, MobCategory::Ambient) => &LIST_1,
        (Biome::OldGrowthSpruceTaiga, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Swamp, MobCategory::Creature) => &LIST_25,
        (Biome::Swamp, MobCategory::Ambient) => &LIST_1,
        (Biome::Swamp, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::MangroveSwamp, MobCategory::Creature) => &LIST_26,
        (Biome::MangroveSwamp, MobCategory::Ambient) => &LIST_1,
        (Biome::MangroveSwamp, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::MangroveSwamp, MobCategory::WaterAmbient) => &LIST_27,
        (Biome::Desert, MobCategory::Creature) => &LIST_28,
        (Biome::Desert, MobCategory::Ambient) => &LIST_1,
        (Biome::Desert, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Savanna, MobCategory::Creature) => &LIST_29,
        (Biome::Savanna, MobCategory::Ambient) => &LIST_1,
        (Biome::Savanna, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SavannaPlateau, MobCategory::Creature) => &LIST_30,
        (Biome::SavannaPlateau, MobCategory::Ambient) => &LIST_1,
        (Biome::SavannaPlateau, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::WindsweptSavanna, MobCategory::Creature) => &LIST_29,
        (Biome::WindsweptSavanna, MobCategory::Ambient) => &LIST_1,
        (Biome::WindsweptSavanna, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Jungle, MobCategory::Creature) => &LIST_31,
        (Biome::Jungle, MobCategory::Ambient) => &LIST_1,
        (Biome::Jungle, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SparseJungle, MobCategory::Creature) => &LIST_32,
        (Biome::SparseJungle, MobCategory::Ambient) => &LIST_1,
        (Biome::SparseJungle, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::BambooJungle, MobCategory::Creature) => &LIST_33,
        (Biome::BambooJungle, MobCategory::Ambient) => &LIST_1,
        (Biome::BambooJungle, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Badlands, MobCategory::Creature) => &LIST_34,
        (Biome::Badlands, MobCategory::Ambient) => &LIST_1,
        (Biome::Badlands, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::WoodedBadlands, MobCategory::Creature) => &LIST_35,
        (Biome::WoodedBadlands, MobCategory::Ambient) => &LIST_1,
        (Biome::WoodedBadlands, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::ErodedBadlands, MobCategory::Creature) => &LIST_34,
        (Biome::ErodedBadlands, MobCategory::Ambient) => &LIST_1,
        (Biome::ErodedBadlands, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Meadow, MobCategory::Creature) => &LIST_36,
        (Biome::Meadow, MobCategory::Ambient) => &LIST_1,
        (Biome::Meadow, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::CherryGrove, MobCategory::Creature) => &LIST_37,
        (Biome::CherryGrove, MobCategory::Ambient) => &LIST_1,
        (Biome::CherryGrove, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::Grove, MobCategory::Creature) => &LIST_38,
        (Biome::Grove, MobCategory::Ambient) => &LIST_1,
        (Biome::Grove, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::SnowySlopes, MobCategory::Creature) => &LIST_39,
        (Biome::SnowySlopes, MobCategory::Ambient) => &LIST_1,
        (Biome::SnowySlopes, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::JaggedPeaks, MobCategory::Creature) => &LIST_40,
        (Biome::JaggedPeaks, MobCategory::Ambient) => &LIST_1,
        (Biome::JaggedPeaks, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::FrozenPeaks, MobCategory::Creature) => &LIST_40,
        (Biome::FrozenPeaks, MobCategory::Ambient) => &LIST_1,
        (Biome::FrozenPeaks, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::StonyPeaks, MobCategory::Ambient) => &LIST_1,
        (Biome::StonyPeaks, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::WindsweptHills, MobCategory::Creature) => &LIST_41,
        (Biome::WindsweptHills, MobCategory::Ambient) => &LIST_1,
        (Biome::WindsweptHills, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::WindsweptGravellyHills, MobCategory::Creature) => &LIST_41,
        (Biome::WindsweptGravellyHills, MobCategory::Ambient) => &LIST_1,
        (Biome::WindsweptGravellyHills, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::WindsweptForest, MobCategory::Creature) => &LIST_41,
        (Biome::WindsweptForest, MobCategory::Ambient) => &LIST_1,
        (Biome::WindsweptForest, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::LushCaves, MobCategory::Ambient) => &LIST_1,
        (Biome::LushCaves, MobCategory::Axolotls) => &LIST_42,
        (Biome::LushCaves, MobCategory::UndergroundWaterCreature) => &LIST_2,
        (Biome::LushCaves, MobCategory::WaterAmbient) => &LIST_27,
        (Biome::DripstoneCaves, MobCategory::Ambient) => &LIST_1,
        (Biome::DripstoneCaves, MobCategory::UndergroundWaterCreature) => &LIST_2,
        _ => &[],
    }
}
pub(super) fn generation_probability(biome: Biome) -> f32 {
    match biome {
        Biome::SnowyPlains => 0.07,
        Biome::IceSpikes => 0.07,
        Biome::Badlands => 0.03,
        Biome::WoodedBadlands => 0.04,
        Biome::ErodedBadlands => 0.03,
        _ => 0.1,
    }
}

const NETHER_WASTES_MONSTER: [MonsterSpawn; 5] = [
    e(J::Known(MobKind::Ghast), 50, 4, 4),
    e(J::Known(MobKind::ZombifiedPiglin), 100, 4, 4),
    e(J::Known(MobKind::MagmaCube), 2, 4, 4),
    e(J::Known(MobKind::Enderman), 1, 4, 4),
    e(J::Known(MobKind::Piglin), 15, 4, 4),
];

const NETHER_WASTES_CREATURE: [MonsterSpawn; 1] = [e(J::Known(MobKind::Strider), 60, 1, 2)];

const SOUL_SAND_VALLEY_MONSTER: [MonsterSpawn; 3] = [
    e(J::Known(MobKind::Skeleton), 20, 5, 5),
    e(J::Known(MobKind::Ghast), 50, 4, 4),
    e(J::Known(MobKind::Enderman), 1, 4, 4),
];

const SOUL_SAND_VALLEY_CREATURE: [MonsterSpawn; 1] = [e(J::Known(MobKind::Strider), 60, 1, 2)];

const CRIMSON_FOREST_MONSTER: [MonsterSpawn; 3] = [
    e(J::Known(MobKind::ZombifiedPiglin), 1, 2, 4),
    e(J::Known(MobKind::Hoglin), 9, 3, 4),
    e(J::Known(MobKind::Piglin), 5, 3, 4),
];

const CRIMSON_FOREST_CREATURE: [MonsterSpawn; 1] = [e(J::Known(MobKind::Strider), 60, 1, 2)];

const WARPED_FOREST_MONSTER: [MonsterSpawn; 1] = [e(J::Known(MobKind::Enderman), 1, 4, 4)];

const WARPED_FOREST_CREATURE: [MonsterSpawn; 1] = [e(J::Known(MobKind::Strider), 60, 1, 2)];

const BASALT_DELTAS_MONSTER: [MonsterSpawn; 2] =
    [e(J::Known(MobKind::Ghast), 40, 1, 1), e(J::Known(MobKind::MagmaCube), 100, 2, 5)];

const BASALT_DELTAS_CREATURE: [MonsterSpawn; 1] = [e(J::Known(MobKind::Strider), 60, 1, 2)];

const THE_END_MONSTER: [MonsterSpawn; 1] = [e(J::Known(MobKind::Enderman), 10, 4, 4)];

pub(super) fn dimension_list(
    biome: crate::world::nether_biome::NetherBiome,
    category: MobCategory,
) -> &'static [MonsterSpawn] {
    use crate::world::nether_biome::NetherBiome as N;
    match (biome, category) {
        (N::NetherWastes, MobCategory::Monster) => &NETHER_WASTES_MONSTER,
        (N::NetherWastes, MobCategory::Creature) => &NETHER_WASTES_CREATURE,
        (N::SoulSandValley, MobCategory::Monster) => &SOUL_SAND_VALLEY_MONSTER,
        (N::SoulSandValley, MobCategory::Creature) => &SOUL_SAND_VALLEY_CREATURE,
        (N::CrimsonForest, MobCategory::Monster) => &CRIMSON_FOREST_MONSTER,
        (N::CrimsonForest, MobCategory::Creature) => &CRIMSON_FOREST_CREATURE,
        (N::WarpedForest, MobCategory::Monster) => &WARPED_FOREST_MONSTER,
        (N::WarpedForest, MobCategory::Creature) => &WARPED_FOREST_CREATURE,
        (N::BasaltDeltas, MobCategory::Monster) => &BASALT_DELTAS_MONSTER,
        (N::BasaltDeltas, MobCategory::Creature) => &BASALT_DELTAS_CREATURE,
        _ => &[],
    }
}
pub(super) fn end_list(category: MobCategory) -> &'static [MonsterSpawn] {
    if category == MobCategory::Monster { &THE_END_MONSTER } else { &[] }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambient_lists_hold_no_existing_mob_so_the_pass_can_be_skipped() {
        for b in Biome::ALL {
            assert!(biome_list(b, MobCategory::Ambient).iter().all(|s| s.mob.kind().is_none()), "{b:?}");
        }
    }
    #[test]
    fn friendly_biome_lists_match_the_official_26_3_attributes() {
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/spawn_categories.json")).unwrap();
        let categories = [
            (MobCategory::Creature, "creature"),
            (MobCategory::Ambient, "ambient"),
            (MobCategory::Axolotls, "axolotls"),
            (MobCategory::UndergroundWaterCreature, "underground_water_creature"),
            (MobCategory::WaterCreature, "water_creature"),
            (MobCategory::WaterAmbient, "water_ambient"),
        ];
        for biome in Biome::ALL {
            // `deep_dark` has no spawn settings at all (Java's
            // `OverworldBiomes.deepDark` registers none); the fixture
            // predates the biome here.
            if biome == Biome::DeepDark {
                assert!(categories.iter().all(|&(c, _)| biome_list(biome, c).is_empty()));
                continue;
            }
            for (category, name) in categories {
                let ours: Vec<_> = biome_list(biome, category)
                    .iter()
                    .map(|e| (e.mob.id().replace(' ', "_"), e.weight, e.min, e.max))
                    .collect();
                let theirs: Vec<_> = reference["biomes"][biome.name()][name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| {
                        (
                            e[0].as_str().unwrap().to_owned(),
                            e[1].as_u64().unwrap() as u32,
                            e[2].as_u64().unwrap() as u32,
                            e[3].as_u64().unwrap() as u32,
                        )
                    })
                    .collect();
                assert_eq!(ours, theirs, "{biome:?} {category:?}");
                assert!(
                    biome_list(biome, category).iter().filter_map(|e| e.mob.kind()).all(|k| k.category() == category)
                );
            }
        }
        for biome in crate::world::nether_biome::NetherBiome::ALL {
            for (category, name) in [(MobCategory::Monster, "monster"), (MobCategory::Creature, "creature")] {
                let ours: Vec<_> = dimension_list(biome, category)
                    .iter()
                    .map(|e| (e.mob.id().replace(' ', "_"), e.weight, e.min, e.max))
                    .collect();
                let theirs: Vec<_> = reference["biomes"][biome.name()][name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| {
                        (
                            e[0].as_str().unwrap().to_owned(),
                            e[1].as_u64().unwrap() as u32,
                            e[2].as_u64().unwrap() as u32,
                            e[3].as_u64().unwrap() as u32,
                        )
                    })
                    .collect();
                assert_eq!(ours, theirs, "{biome:?} {category:?}");
            }
        }
    }
}
