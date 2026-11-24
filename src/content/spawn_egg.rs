//! v0.18 Track 4: Java 26.3's spawn eggs (`SpawnEggItem`), one item per egg
//! whose mob VoxelCraft has: item ids 1850..=1936 in the order of
//! `tests/fixtures/java/26.3/spawn_eggs.json` (the Spawn Eggs tab's, then
//! the Wither's egg, which no tab lists). The Ender Dragon's egg (Java's
//! last) is not registered: the dragon only exists as the End fight's boss,
//! so 1937 stays free for it.
//!
//! Each egg stacks to 64 and is creative-only, as in Java: no recipe, loot
//! or trade makes one. Its icon is VoxelCraft's own two-colour egg; the
//! two colours are numbers from the 26.3 jar (the means of the two colour
//! clusters of Java's texture), not art. Using one (`SpawnEggItem.useOn`
//! and `use`), dispensing one (`SpawnEggItemBehavior`) and using one on a
//! matching mob (`spawnOffspringFromSpawnEgg`) live in
//! `entity::spawn_egg`; the use chain calls them from
//! `session::interact`. Checked in `tests/spawn_eggs.rs`.

use crate::entity::MobKind;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

/// The chicken's egg, the first of the Spawn Eggs tab.
pub const ITEM_FIRST: u16 = 1850;
/// One past the Wither's egg.
pub const ITEM_END: u16 = ITEM_FIRST + EGGS.len() as u16;
const _: () = assert!(ITEM_END == 1937);

/// `Item.Properties` default: eggs stack to 64.
pub const MAX_STACK: u8 = 64;

/// What `SpawnEggItem.spawnOffspringFromSpawnEgg` does with a mob of the
/// egg's own type (the fixture's `baby`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Baby {
    /// An `AgeableMob` whose `getBreedOffspring` makes a mob: a baby at
    /// `AgeableMob`'s starting age.
    Breed,
    /// A mob outside `AgeableMob` that overrides `setBaby` (the zombie
    /// family, piglins and zoglins): a baby for good.
    SetBaby,
    /// No offspring; the use passes on.
    None,
}

/// One spawn egg: its lower-case English name, its mob, its two colours,
/// whether its type may spawn in Peaceful (`EntityType.isAllowedInPeaceful`)
/// and its baby rule.
#[derive(Clone, Copy, Debug)]
pub struct Egg {
    pub name: &'static str,
    pub kind: MobKind,
    pub base: [u8; 3],
    pub spots: [u8; 3],
    pub peaceful: bool,
    pub baby: Baby,
}

const fn entry(name: &'static str, kind: MobKind, base: [u8; 3], spots: [u8; 3], peaceful: bool, baby: Baby) -> Egg {
    Egg { name, kind, base, spots, peaceful, baby }
}

/// Every registered egg, in item id order.
pub const EGGS: [Egg; 87] = [
    entry("chicken spawn egg", MobKind::Chicken, [207, 205, 205], [181, 103, 53], true, Baby::Breed),
    entry("cow spawn egg", MobKind::Cow, [64, 54, 43], [166, 166, 166], true, Baby::Breed),
    entry("pig spawn egg", MobKind::Pig, [237, 138, 134], [183, 87, 83], true, Baby::Breed),
    entry("sheep spawn egg", MobKind::Sheep, [224, 220, 219], [171, 135, 113], true, Baby::Breed),
    entry("camel spawn egg", MobKind::Camel, [232, 176, 87], [160, 121, 60], true, Baby::Breed),
    entry("donkey spawn egg", MobKind::Donkey, [111, 96, 82], [193, 188, 179], true, Baby::Breed),
    entry("horse spawn egg", MobKind::Horse, [99, 63, 24], [231, 213, 198], true, Baby::Breed),
    entry("mule spawn egg", MobKind::Mule, [69, 40, 26], [135, 114, 93], true, Baby::Breed),
    entry("cat spawn egg", MobKind::Cat, [90, 89, 89], [199, 192, 195], true, Baby::Breed),
    entry("parrot spawn egg", MobKind::Parrot, [193, 70, 23], [5, 93, 240], true, Baby::None),
    entry("wolf spawn egg", MobKind::Wolf, [182, 174, 170], [74, 70, 65], true, Baby::Breed),
    entry("armadillo spawn egg", MobKind::Armadillo, [128, 83, 85], [187, 117, 109], true, Baby::Breed),
    entry("bat spawn egg", MobKind::Bat, [37, 31, 27], [115, 83, 112], true, Baby::None),
    entry("bee spawn egg", MobKind::Bee, [229, 179, 70], [104, 58, 34], true, Baby::Breed),
    entry("fox spawn egg", MobKind::Fox, [213, 145, 94], [6, 4, 14], true, Baby::Breed),
    entry("goat spawn egg", MobKind::Goat, [216, 207, 191], [84, 77, 73], true, Baby::Breed),
    entry("llama spawn egg", MobKind::Llama, [231, 209, 165], [158, 127, 95], true, Baby::Breed),
    entry("ocelot spawn egg", MobKind::Ocelot, [251, 222, 150], [202, 149, 83], true, Baby::Breed),
    entry("panda spawn egg", MobKind::Panda, [198, 198, 198], [27, 27, 33], true, Baby::Breed),
    entry("polar bear spawn egg", MobKind::PolarBear, [227, 225, 220], [67, 66, 71], true, Baby::Breed),
    entry("rabbit spawn egg", MobKind::Rabbit, [133, 92, 60], [190, 152, 124], true, Baby::Breed),
    entry("axolotl spawn egg", MobKind::Axolotl, [242, 172, 213], [200, 85, 141], true, Baby::Breed),
    entry("cod spawn egg", MobKind::Cod, [151, 124, 97], [68, 129, 177], true, Baby::None),
    entry("dolphin spawn egg", MobKind::Dolphin, [169, 191, 213], [97, 115, 146], true, Baby::Breed),
    entry("frog spawn egg", MobKind::Frog, [200, 124, 78], [126, 69, 40], true, Baby::Breed),
    entry("glow squid spawn egg", MobKind::GlowSquid, [35, 122, 127], [98, 210, 181], true, Baby::Breed),
    entry("nautilus spawn egg", MobKind::Nautilus, [166, 77, 56], [162, 171, 172], true, Baby::Breed),
    entry("pufferfish spawn egg", MobKind::Pufferfish, [212, 155, 52], [115, 156, 172], true, Baby::None),
    entry("salmon spawn egg", MobKind::Salmon, [124, 79, 64], [149, 166, 175], true, Baby::None),
    entry("squid spawn egg", MobKind::Squid, [76, 106, 129], [128, 17, 17], true, Baby::Breed),
    entry("tadpole spawn egg", MobKind::Tadpole, [72, 49, 31], [71, 138, 185], true, Baby::None),
    entry("tropical fish spawn egg", MobKind::TropicalFish, [141, 80, 47], [189, 190, 195], true, Baby::None),
    entry("turtle spawn egg", MobKind::Turtle, [66, 142, 65], [178, 170, 122], true, Baby::Breed),
    entry("allay spawn egg", MobKind::Allay, [59, 190, 227], [170, 253, 255], true, Baby::None),
    entry("mooshroom spawn egg", MobKind::Mooshroom, [131, 27, 30], [167, 167, 167], true, Baby::Breed),
    entry("sniffer spawn egg", MobKind::Sniffer, [66, 62, 38], [232, 172, 46], true, Baby::Breed),
    entry("sulfur cube spawn egg", MobKind::SulfurCube, [233, 238, 173], [201, 166, 131], true, Baby::Breed),
    entry("copper golem spawn egg", MobKind::CopperGolem, [152, 78, 53], [215, 124, 98], true, Baby::None),
    entry("iron golem spawn egg", MobKind::IronGolem, [193, 174, 153], [95, 93, 49], true, Baby::None),
    entry("snow golem spawn egg", MobKind::SnowGolem, [172, 106, 25], [193, 205, 204], true, Baby::None),
    entry("trader llama spawn egg", MobKind::TraderLlama, [220, 192, 139], [86, 70, 81], true, Baby::Breed),
    entry("villager spawn egg", MobKind::Villager, [101, 64, 54], [175, 122, 94], true, Baby::Breed),
    entry("wandering trader spawn egg", MobKind::WanderingTrader, [101, 57, 45], [211, 154, 68], true, Baby::None),
    entry("bogged spawn egg", MobKind::Bogged, [157, 149, 125], [77, 81, 30], false, Baby::None),
    entry("camel husk spawn egg", MobKind::CamelHusk, [56, 48, 38], [91, 79, 61], true, Baby::None),
    entry("drowned spawn egg", MobKind::Drowned, [78, 111, 72], [102, 210, 177], false, Baby::SetBaby),
    entry("husk spawn egg", MobKind::Husk, [173, 151, 105], [95, 82, 62], false, Baby::SetBaby),
    entry("parched spawn egg", MobKind::Parched, [104, 99, 87], [187, 166, 121], false, Baby::None),
    entry("skeleton spawn egg", MobKind::Skeleton, [191, 191, 191], [135, 134, 134], false, Baby::None),
    entry("skeleton horse spawn egg", MobKind::SkeletonHorse, [155, 154, 154], [215, 215, 215], true, Baby::Breed),
    entry("stray spawn egg", MobKind::Stray, [86, 106, 107], [180, 197, 200], false, Baby::None),
    entry("zombie spawn egg", MobKind::Zombie, [91, 116, 85], [4, 141, 147], false, Baby::SetBaby),
    entry("zombie horse spawn egg", MobKind::ZombieHorse, [48, 64, 36], [138, 148, 87], true, Baby::Breed),
    entry("zombie nautilus spawn egg", MobKind::ZombieNautilus, [76, 94, 57], [182, 197, 181], true, Baby::None),
    entry("zombie villager spawn egg", MobKind::ZombieVillager, [101, 125, 65], [61, 68, 39], false, Baby::SetBaby),
    entry("cave spider spawn egg", MobKind::CaveSpider, [20, 45, 41], [132, 122, 111], false, Baby::None),
    entry("spider spawn egg", MobKind::Spider, [59, 51, 44], [132, 122, 111], false, Baby::None),
    entry("breeze spawn egg", MobKind::Breeze, [190, 180, 217], [88, 108, 157], false, Baby::None),
    entry("creaking spawn egg", MobKind::Creaking, [105, 98, 95], [238, 143, 41], false, Baby::None),
    entry("creeper spawn egg", MobKind::Creeper, [48, 140, 49], [157, 198, 152], false, Baby::None),
    entry("elder guardian spawn egg", MobKind::ElderGuardian, [148, 149, 138], [67, 76, 103], false, Baby::None),
    entry("guardian spawn egg", MobKind::Guardian, [77, 118, 113], [204, 79, 33], false, Baby::None),
    entry("phantom spawn egg", MobKind::Phantom, [85, 92, 117], [209, 201, 177], false, Baby::None),
    entry("silverfish spawn egg", MobKind::Silverfish, [118, 121, 122], [42, 42, 43], false, Baby::None),
    entry("slime spawn egg", MobKind::Slime, [123, 190, 104], [76, 140, 59], false, Baby::None),
    entry("warden spawn egg", MobKind::Warden, [14, 44, 53], [102, 168, 159], false, Baby::None),
    entry("witch spawn egg", MobKind::Witch, [140, 118, 97], [47, 33, 45], false, Baby::None),
    entry("evoker spawn egg", MobKind::Evoker, [146, 147, 132], [59, 62, 61], false, Baby::None),
    entry("pillager spawn egg", MobKind::Pillager, [65, 62, 63], [140, 149, 149], false, Baby::None),
    entry("ravager spawn egg", MobKind::Ravager, [68, 68, 66], [128, 150, 146], false, Baby::None),
    entry("vex spawn egg", MobKind::Vex, [111, 134, 156], [204, 220, 235], false, Baby::None),
    entry("vindicator spawn egg", MobKind::Vindicator, [58, 66, 66], [139, 145, 144], false, Baby::None),
    entry("blaze spawn egg", MobKind::Blaze, [249, 199, 52], [127, 39, 3], false, Baby::None),
    entry("ghast spawn egg", MobKind::Ghast, [232, 227, 227], [179, 170, 170], false, Baby::None),
    entry("happy ghast spawn egg", MobKind::HappyGhast, [231, 226, 226], [176, 168, 168], true, Baby::Breed),
    entry("hoglin spawn egg", MobKind::Hoglin, [165, 105, 81], [226, 175, 131], false, Baby::Breed),
    entry("magma cube spawn egg", MobKind::MagmaCube, [67, 11, 6], [222, 126, 42], false, Baby::None),
    entry("piglin spawn egg", MobKind::Piglin, [212, 151, 111], [128, 69, 45], true, Baby::SetBaby),
    entry("piglin brute spawn egg", MobKind::PiglinBrute, [212, 152, 109], [123, 67, 47], false, Baby::None),
    entry("strider spawn egg", MobKind::Strider, [115, 54, 56], [221, 190, 157], true, Baby::Breed),
    entry("wither skeleton spawn egg", MobKind::WitherSkeleton, [53, 55, 55], [27, 27, 27], false, Baby::None),
    entry("zoglin spawn egg", MobKind::Zoglin, [215, 154, 150], [113, 98, 75], false, Baby::SetBaby),
    entry("zombified piglin spawn egg", MobKind::ZombifiedPiglin, [134, 97, 80], [224, 187, 185], false, Baby::SetBaby),
    entry("enderman spawn egg", MobKind::Enderman, [40, 34, 41], [200, 52, 239], false, Baby::None),
    entry("endermite spawn egg", MobKind::Endermite, [58, 39, 73], [182, 28, 223], false, Baby::None),
    entry("shulker spawn egg", MobKind::Shulker, [135, 95, 135], [204, 206, 143], true, Baby::None),
    entry("wither spawn egg", MobKind::Wither, [51, 52, 52], [146, 146, 146], false, Baby::None),
];

/// The egg data of a spawn egg item.
pub fn egg_of(item: Item) -> Option<&'static Egg> {
    item.0.checked_sub(ITEM_FIRST).and_then(|i| EGGS.get(i as usize))
}

/// The mob a spawn egg item makes.
pub fn mob(item: Item) -> Option<MobKind> {
    egg_of(item).map(|e| e.kind)
}

/// Whether `item` is a spawn egg.
pub fn is_spawn_egg(item: Item) -> bool {
    egg_of(item).is_some()
}

/// The spawn egg for `kind`, if Java has one (the illusioner has none).
pub fn egg(kind: MobKind) -> Option<Item> {
    EGGS.iter().position(|e| e.kind == kind).map(|i| Item(ITEM_FIRST + i as u16))
}

/// Every spawn egg, in item id order.
pub fn all() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    let e = egg_of(item)?;
    Some(ItemInfo {
        name: e.name,
        kind: ItemKind::Material,
        max_stack: MAX_STACK,
        sprite: Sprite::SpawnEgg(e.base, e.spots),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eggs_are_unique_and_round_trip() {
        for (i, e) in EGGS.iter().enumerate() {
            let item = Item(ITEM_FIRST + i as u16);
            assert_eq!(mob(item), Some(e.kind));
            assert_eq!(egg(e.kind), Some(item), "{}", e.name);
            assert_eq!(item.name(), e.name);
            assert_eq!(item.max_stack(), 64);
            assert_eq!(Item::from_name(e.name), Some(item));
        }
        assert_eq!(egg(MobKind::Illusioner), None);
        assert_eq!(mob(Item(ITEM_END)), None);
        assert_eq!(EGGS.len() + 1, MobKind::ALL.len(), "every mob but the illusioner has an egg");
    }
}
