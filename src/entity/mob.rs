//! A single mob: AI, movement physics and combat state.

use std::f32::consts::{PI, TAU};

use glam::{DVec3, IVec3};

use crate::item::Item;
use crate::physics::{self, BlockSource, Shape};
use crate::world::block::Block;
use crate::world::terrain::Dimension;

use super::{Ctx, EntityEvent, MobSound, MobWorld, Rng, path};

pub(crate) mod goals;
mod navigate;
mod nether_ai;
#[cfg(test)]
pub(crate) use nether_ai::STABLE_LAVA_HEIGHT;

const GRAVITY: f64 = 28.0;
/// Clears a 1-block ledge with a little margin (peak ~1.26 blocks).
const JUMP_VELOCITY: f64 = 8.4;
const MAX_STEP: f64 = 1.0 / 60.0;
/// Seconds of red flash (and knockback immunity) after taking damage.
pub const HURT_TIME: f32 = 0.5;
/// Length of the death animation before the mob is removed.
pub const DEATH_TIME: f32 = 0.9;
/// Hostile mobs notice players within this many blocks.
const CHASE_RANGE: f64 = 24.0;
const ATTACK_RANGE: f64 = 1.2;
const ATTACK_COOLDOWN: f32 = 1.0;
/// Zombies and skeletons burn in sunlight above this daylight level.
const BURN_DAYLIGHT: f32 = 0.45;
/// Spiders only hunt when it's darker than this (or after being hit).
const SPIDER_CALM_DAYLIGHT: f32 = 0.45;
/// Seconds a hit spider stays hostile in daylight.
const PROVOKED_TIME: f32 = 12.0;
/// Seconds zombified piglins stay angry after one of them is hit.
pub const PIGLIN_ANGER_TIME: f32 = 30.0;
/// Skeletons shoot from up to this far, and keep between these distances.
const SHOOT_RANGE: f64 = 16.0;
/// Witches throw from up to this far (Java's attack radius).
const WITCH_RANGE: f64 = 10.0;
const WITCH_SPEED: f64 = crate::entity::potion::WITCH_SPEED;
const SKELETON_NEAR: f64 = 5.0;
const SKELETON_FAR: f64 = 10.0;
/// Creepers light their fuse this close and keep it lit within `FUSE_KEEP`.
const FUSE_START: f64 = 3.0;
const FUSE_KEEP: f64 = 7.0;
/// Seconds from lighting the fuse to the explosion.
pub const FUSE_TIME: f32 = 1.5;
pub const CREEPER_POWER: f32 = 3.0;
/// Seconds an enderman stays angry after a stare or a hit (refreshed
/// while its target stays near).
const ENDERMAN_ANGER_TIME: f32 = 30.0;
/// Seconds of eye contact before an enderman turns on you (Java's 5 ticks).
const STARE_TIME: f32 = 0.25;
/// Endermen notice stares from this far, freeze while watched within
/// `FREEZE_DIST`, flee a stare closer than `FLEE_DIST`, and teleport after
/// targets farther than `FREEZE_DIST`.
const STARE_RANGE: f64 = 64.0;
const FREEZE_DIST: f64 = 16.0;
const FLEE_DIST: f64 = 4.0;
/// Blazes notice players this far away (Java's follow range) and melee
/// within `BLAZE_MELEE`.
const BLAZE_RANGE: f64 = 48.0;
const BLAZE_MELEE: f64 = 2.0;
/// Seconds a blaze charges (glowing) before a burst, between the burst's
/// three fireballs, and resting after it (Java's 60, 6 and 100 ticks).
const BLAZE_CHARGE: f32 = 3.0;
const BLAZE_VOLLEY: f32 = 0.3;
const BLAZE_REST: f32 = 5.0;
/// Ghasts notice players this far (Java's follow range), charge for a
/// second, then rest two seconds. Flying speed is 0.7 blocks a tick.
const GHAST_RANGE: f64 = 100.0;
const GHAST_CHARGE: f32 = 1.0;
const GHAST_REST: f32 = 2.0;
const GHAST_SPEED: f64 = 14.0;
/// Enderman eye height (Java's 2.55).
const ENDERMAN_EYE: f64 = 2.55;
/// Seconds `getLastDamageSource` remembers a hit (40 ticks), during which
/// `PanicGoal` keeps choosing new spots.
const PANIC_MEMORY: f32 = 2.0;
/// Falls deeper than this are avoided (blocks).
const MAX_SAFE_DROP: i32 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MobKind {
    Pig,
    Cow,
    Sheep,
    Chicken,
    Zombie,
    Skeleton,
    Creeper,
    Spider,
    /// Neutral Nether mob: leaves you alone until you hit one of them.
    ZombifiedPiglin,
    /// Neutral until looked in the eye or hit; teleports.
    Enderman,
    /// Fortress spawner mob: hovers and shoots bursts of fireballs.
    Blaze,
    /// Stronghold spawner mob: small, fast, nibbles for 1.
    Silverfish,
    CaveSpider,
    Slime,
    MagmaCube,
    /// Nether flyer: charges, then shoots a fireball a player can punch back.
    Ghast,
    /// Fortress skeleton: tall, melee, withers what it hits.
    WitherSkeleton,
    /// Desert zombie that doesn't burn and inflicts Hunger.
    Husk,
    /// River and ocean zombie that swims.
    Drowned,
    /// Throws splash potions, and drinks its own.
    Witch,
    Villager,
    /// A villager a zombie killed. Keeps the profession and can be cured.
    ZombieVillager,
    /// Village defender: 100 health, built or summoned.
    IronGolem,
    /// Two snow blocks and a pumpkin. Melts in water and deserts.
    SnowGolem,
    WanderingTrader,
    TraderLlama,
    /// Nether trader: barters for gold, attacks players without gold armor.
    Piglin,
    /// Bastion guard with a golden axe: always hostile, never bribed.
    PiglinBrute,
    /// Crimson-forest beast: charges players and tosses them in the air.
    Hoglin,
    /// A hoglin zombified outside the Nether: attacks everything.
    Zoglin,
    /// Walks on lava, and shivers out of it.
    Strider,
    Cod,
    Salmon,
    TropicalFish,
    Pufferfish,
    Squid,
    GlowSquid,
    Dolphin,
    Axolotl,
    Guardian,
    ElderGuardian,
    Cat,
    Pillager,
    /// Beach turtle: swims, nests on sand; babies grow up and shed a scute.
    Turtle,
    Wolf,
    Fox,
    Parrot,
    Rabbit,
    Goat,
    Horse,
    Donkey,
    Mule,
    /// Mountain and savanna pack animal: carries chests and carpets, spits, can't be steered.
    Llama,
    /// Desert-village mount: two seats, dashes, sits down now and then.
    Camel,
}

impl MobKind {
    pub fn is_aquatic(self) -> bool {
        matches!(
            self,
            Self::Cod
                | Self::Salmon
                | Self::TropicalFish
                | Self::Pufferfish
                | Self::Squid
                | Self::GlowSquid
                | Self::Dolphin
                | Self::Axolotl
                | Self::Guardian
                | Self::ElderGuardian
        )
    }

    /// Zombie, husk, drowned or zombie villager: shares the walk, armor and baby rolls.
    pub fn is_zombie(self) -> bool {
        matches!(self, Self::Zombie | Self::Husk | Self::Drowned | Self::ZombieVillager)
    }
    pub fn is_cube(self) -> bool {
        matches!(self, Self::Slime | Self::MagmaCube)
    }
    pub const ALL: [MobKind; 54] = [
        MobKind::Pig,
        MobKind::Cow,
        MobKind::Sheep,
        MobKind::Chicken,
        MobKind::Zombie,
        MobKind::Skeleton,
        MobKind::Creeper,
        MobKind::Spider,
        MobKind::ZombifiedPiglin,
        MobKind::Enderman,
        MobKind::Blaze,
        MobKind::Silverfish,
        MobKind::CaveSpider,
        MobKind::Slime,
        MobKind::MagmaCube,
        MobKind::Ghast,
        MobKind::WitherSkeleton,
        MobKind::Husk,
        MobKind::Drowned,
        MobKind::Witch,
        MobKind::Villager,
        MobKind::ZombieVillager,
        MobKind::IronGolem,
        MobKind::SnowGolem,
        MobKind::WanderingTrader,
        MobKind::TraderLlama,
        MobKind::Piglin,
        MobKind::PiglinBrute,
        MobKind::Hoglin,
        MobKind::Zoglin,
        MobKind::Strider,
        MobKind::Cod,
        MobKind::Salmon,
        MobKind::TropicalFish,
        MobKind::Pufferfish,
        MobKind::Squid,
        MobKind::GlowSquid,
        MobKind::Dolphin,
        MobKind::Axolotl,
        MobKind::Guardian,
        MobKind::ElderGuardian,
        MobKind::Cat,
        MobKind::Pillager,
        MobKind::Turtle,
        MobKind::Wolf,
        MobKind::Fox,
        MobKind::Parrot,
        MobKind::Rabbit,
        MobKind::Goat,
        MobKind::Horse,
        MobKind::Donkey,
        MobKind::Mule,
        MobKind::Llama,
        MobKind::Camel,
    ];

    /// Lowercase mob name used by commands and saved spawner entries.
    pub fn name(self) -> &'static str {
        match self {
            MobKind::Pig => "pig",
            MobKind::Cow => "cow",
            MobKind::Sheep => "sheep",
            MobKind::Chicken => "chicken",
            MobKind::Zombie => "zombie",
            MobKind::Skeleton => "skeleton",
            MobKind::Creeper => "creeper",
            MobKind::Spider => "spider",
            MobKind::ZombifiedPiglin => "zombified piglin",
            MobKind::Enderman => "enderman",
            MobKind::Blaze => "blaze",
            MobKind::Silverfish => "silverfish",
            MobKind::CaveSpider => "cave spider",
            MobKind::Slime => "slime",
            MobKind::MagmaCube => "magma cube",
            MobKind::Ghast => "ghast",
            MobKind::WitherSkeleton => "wither skeleton",
            MobKind::Husk => "husk",
            MobKind::Drowned => "drowned",
            MobKind::Witch => "witch",
            MobKind::Villager => "villager",
            MobKind::ZombieVillager => "zombie villager",
            MobKind::IronGolem => "iron golem",
            MobKind::SnowGolem => "snow golem",
            MobKind::WanderingTrader => "wandering trader",
            MobKind::TraderLlama => "trader llama",
            MobKind::Piglin => "piglin",
            MobKind::PiglinBrute => "piglin brute",
            MobKind::Hoglin => "hoglin",
            MobKind::Zoglin => "zoglin",
            MobKind::Strider => "strider",
            MobKind::Cod => "cod",
            MobKind::Salmon => "salmon",
            MobKind::TropicalFish => "tropical fish",
            MobKind::Pufferfish => "pufferfish",
            MobKind::Squid => "squid",
            MobKind::GlowSquid => "glow squid",
            MobKind::Dolphin => "dolphin",
            MobKind::Axolotl => "axolotl",
            MobKind::Guardian => "guardian",
            MobKind::ElderGuardian => "elder guardian",
            MobKind::Cat => "cat",
            MobKind::Pillager => "pillager",
            MobKind::Turtle => "turtle",
            MobKind::Wolf => "wolf",
            MobKind::Fox => "fox",
            MobKind::Parrot => "parrot",
            MobKind::Rabbit => "rabbit",
            MobKind::Goat => "goat",
            MobKind::Horse => "horse",
            MobKind::Donkey => "donkey",
            MobKind::Mule => "mule",
            MobKind::Llama => "llama",
            MobKind::Camel => "camel",
        }
    }

    /// Looks a mob up by name (spaces or underscores).
    pub fn from_name(name: &str) -> Option<MobKind> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name).replace('_', " ");
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Collision box dimensions for movement and spawn-space checks.
    pub fn shape(self) -> Shape {
        match self {
            MobKind::Pig => Shape::new(0.45, 0.9),
            MobKind::Cow => Shape::new(0.45, 1.4),
            MobKind::Sheep => Shape::new(0.45, 1.3),
            MobKind::Chicken => Shape::new(0.2, 0.7),
            MobKind::Zombie
            | MobKind::Husk
            | MobKind::Drowned
            | MobKind::ZombifiedPiglin
            | MobKind::Witch
            | MobKind::Villager
            | MobKind::WanderingTrader
            | MobKind::ZombieVillager
            | MobKind::Piglin
            | MobKind::PiglinBrute => Shape::new(0.3, 1.95),
            MobKind::IronGolem => Shape::new(0.7, 2.7),
            MobKind::SnowGolem => Shape::new(0.35, 1.9),
            MobKind::TraderLlama => Shape::new(0.45, 1.87),
            MobKind::Skeleton => Shape::new(0.3, 1.99),
            MobKind::Creeper => Shape::new(0.3, 1.7),
            MobKind::Spider => Shape::new(0.7, 0.9),
            MobKind::CaveSpider => Shape::new(0.35, 0.5),
            MobKind::Slime | MobKind::MagmaCube => Shape::new(0.255, 0.51),
            MobKind::Enderman => Shape::new(0.3, 2.9),
            MobKind::Blaze => Shape::new(0.3, 1.8),
            MobKind::Ghast => Shape::new(2.0, 4.0),
            MobKind::WitherSkeleton => Shape::new(0.35, 2.4),
            MobKind::Silverfish => Shape::new(0.2, 0.3),
            // Java's 1.3964844 x 1.4.
            MobKind::Hoglin | MobKind::Zoglin => Shape::new(0.698, 1.4),
            MobKind::Strider => Shape::new(0.45, 1.7),
            MobKind::Cod | MobKind::TropicalFish | MobKind::Pufferfish => Shape::new(0.25, 0.4),
            MobKind::Salmon => Shape::new(0.35, 0.4),
            MobKind::Squid | MobKind::GlowSquid => Shape::new(0.4, 0.8),
            MobKind::Dolphin => Shape::new(0.45, 0.6),
            MobKind::Axolotl => Shape::new(0.375, 0.42),
            MobKind::Guardian => Shape::new(0.425, 0.85),
            MobKind::ElderGuardian => Shape::new(0.99875, 1.9975),
            MobKind::Cat => Shape::new(0.3, 0.7),
            MobKind::Pillager => Shape::new(0.3, 1.95),
            MobKind::Turtle => Shape::new(0.6, 0.4),
            MobKind::Wolf => Shape::new(0.3, 0.85),
            MobKind::Fox => Shape::new(0.3, 0.7),
            MobKind::Parrot => Shape::new(0.25, 0.9),
            MobKind::Rabbit => Shape::new(0.2, 0.5),
            MobKind::Goat => Shape::new(0.45, 1.3),
            MobKind::Horse => Shape::new(0.698, 1.6),
            MobKind::Donkey => Shape::new(0.698, 1.5),
            MobKind::Mule => Shape::new(0.698, 1.6),
            MobKind::Llama => Shape::new(0.45, 1.87),
            MobKind::Camel => Shape::new(0.85, 2.375),
        }
    }

    /// Starting health in damage points, with two points per heart.
    pub fn max_health(self) -> f32 {
        match self {
            MobKind::Pig | MobKind::Cow => 10.0,
            MobKind::Sheep | MobKind::Silverfish => 8.0,
            MobKind::Chicken => 4.0,
            MobKind::Zombie
            | MobKind::Husk
            | MobKind::Drowned
            | MobKind::Skeleton
            | MobKind::WitherSkeleton
            | MobKind::Creeper
            | MobKind::ZombifiedPiglin
            | MobKind::Blaze
            | MobKind::Villager
            | MobKind::WanderingTrader
            | MobKind::ZombieVillager
            | MobKind::Strider => 20.0,
            MobKind::IronGolem => 100.0,
            MobKind::SnowGolem => 4.0,
            MobKind::TraderLlama => 30.0,
            MobKind::Spider => 16.0,
            MobKind::CaveSpider => 12.0,
            MobKind::Witch => 26.0,
            MobKind::Slime | MobKind::MagmaCube => 1.0,
            MobKind::Ghast => 10.0,
            MobKind::Enderman => 40.0,
            MobKind::Piglin => 16.0,
            MobKind::PiglinBrute => 50.0,
            MobKind::Hoglin | MobKind::Zoglin => 40.0,
            MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish | MobKind::Pufferfish => 3.0,
            MobKind::Squid | MobKind::GlowSquid | MobKind::Dolphin | MobKind::Cat => 10.0,
            MobKind::Axolotl => 14.0,
            MobKind::Guardian => 30.0,
            MobKind::ElderGuardian => 80.0,
            MobKind::Pillager => 24.0,
            MobKind::Turtle => 30.0,
            MobKind::Wolf => 8.0,
            MobKind::Fox | MobKind::Goat => 10.0,
            MobKind::Parrot => 6.0,
            MobKind::Rabbit => 3.0,
            MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama => 30.0,
            MobKind::Camel => 32.0,
        }
    }

    /// Whether this kind belongs to the hostile spawning group, including neutral monsters.
    pub fn is_hostile(self) -> bool {
        matches!(
            self,
            MobKind::Zombie
                | MobKind::Husk
                | MobKind::Drowned
                | MobKind::ZombieVillager
                | MobKind::Skeleton
                | MobKind::Creeper
                | MobKind::Spider
                | MobKind::ZombifiedPiglin
                | MobKind::Enderman
                | MobKind::Blaze
                | MobKind::Silverfish
                | MobKind::CaveSpider
                | MobKind::Slime
                | MobKind::MagmaCube
                | MobKind::Ghast
                | MobKind::WitherSkeleton
                | MobKind::Witch
                | MobKind::Piglin
                | MobKind::PiglinBrute
                | MobKind::Hoglin
                | MobKind::Zoglin
                | MobKind::Guardian
                | MobKind::ElderGuardian
                | MobKind::Pillager
        )
    }

    /// Java 26.3 `EntityType.isAllowedInPeaceful` is false for `Mob.checkDespawn`
    /// to discard these on Peaceful. Hostile here, except the piglin: its type
    /// is not registered with `notInPeaceful()` (piglin brutes, hoglins and
    /// zoglins are).
    pub fn despawns_in_peaceful(self) -> bool {
        self.is_hostile() && self != MobKind::Piglin
    }

    /// Unharmed by fire and lava.
    pub fn fire_immune(self) -> bool {
        matches!(
            self,
            MobKind::ZombifiedPiglin
                | MobKind::Blaze
                | MobKind::MagmaCube
                | MobKind::Ghast
                | MobKind::WitherSkeleton
                | MobKind::Zoglin
                | MobKind::Strider
        )
    }

    /// Hurt by water and rain, like Java's endermen and blazes.
    pub fn hurt_by_water(self) -> bool {
        matches!(self, MobKind::Enderman | MobKind::Blaze | MobKind::Strider)
    }

    /// Whether this kind spawns naturally in `dimension`.
    pub fn spawns_in(self, dimension: Dimension) -> bool {
        match self {
            MobKind::Enderman => true,
            MobKind::Mule => false,
            // Only from spawners and inside fortresses (`fortress_spawn`).
            MobKind::Blaze
            | MobKind::WitherSkeleton
            | MobKind::Villager
            | MobKind::ZombieVillager
            | MobKind::IronGolem
            | MobKind::SnowGolem
            | MobKind::WanderingTrader
            | MobKind::TraderLlama => false,
            // Only from stronghold spawners (and infested blocks, later).
            MobKind::Silverfish | MobKind::CaveSpider => false,
            // Only generated with bastions (`nether::populate_bastions`).
            MobKind::PiglinBrute | MobKind::Zoglin | MobKind::ElderGuardian | MobKind::Cat | MobKind::Pillager => false,
            // Picked by the Nether biome spawn lists (`world::nether_biome`).
            MobKind::Piglin
            | MobKind::Hoglin
            | MobKind::Strider
            | MobKind::MagmaCube
            | MobKind::ZombifiedPiglin
            | MobKind::Ghast => dimension == Dimension::Nether,
            _ => dimension == Dimension::Overworld,
        }
    }

    /// Chance that a spawn attempt for this kind goes ahead in `dimension`:
    /// Java's spawn weights make endermen a rare sight except in the End.
    pub fn spawn_chance(self, dimension: Dimension) -> f32 {
        match (self, dimension) {
            (MobKind::Enderman, Dimension::Overworld) => 0.1,
            (MobKind::Enderman, Dimension::Nether) => 0.02,
            // Nether wastes weight 2 against zombified piglins at 100.
            // Basalt deltas (weight 100) are not a biome here; fortresses
            // use the separate weighted list.
            (MobKind::MagmaCube, Dimension::Nether) => 0.02,
            // Nether wastes weight 50 against zombified piglins at 100.
            (MobKind::Ghast, Dimension::Nether) => 0.5,
            _ => 1.0,
        }
    }

    /// Chance a surface spawn attempt for this kind goes ahead in `biome`
    /// (Java's biome spawn lists: deserts spawn husks in place of most zombies,
    /// drowned only in rivers and oceans, and witches mostly in swamps).
    pub fn biome_chance(self, biome: crate::world::terrain::Biome) -> f32 {
        use crate::world::terrain::Biome;
        match (self, biome) {
            (MobKind::Horse, Biome::Plains | Biome::Savanna | Biome::SavannaPlateau) => 0.5,
            (MobKind::Donkey, Biome::Plains | Biome::Savanna | Biome::Meadow) => 0.25,
            // Java: llamas in windswept hills and savanna plateaus; camels only come with desert villages.
            (MobKind::Llama, Biome::WindsweptHills | Biome::WindsweptGravellyHills | Biome::WindsweptForest) => 0.5,
            (MobKind::Llama, Biome::SavannaPlateau | Biome::WindsweptSavanna) => 0.5,
            (MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama | MobKind::Camel, _) => 0.,
            (MobKind::Husk, Biome::Desert) => 1.0,
            (MobKind::Husk, _) => 0.0,
            (MobKind::Zombie, Biome::Desert) => 0.2,
            (MobKind::Zombie, b) if b.is_watery() => 0.2,
            (MobKind::Drowned, b) if b.is_watery() => 1.0,
            (MobKind::Drowned, _) => 0.0,
            // Mushroom fields spawn no monsters at all.
            (_, Biome::MushroomFields) if self.is_hostile() => 0.0,
            // Java's weight 5 against 100 for most monsters; swamp huts keep
            // swamps full of them.
            (MobKind::Witch, b) if b.is_swamp() => 0.25,
            (MobKind::Witch, _) => 0.05,
            // Java: turtles spawn on beaches only (weight 5).
            (
                MobKind::Wolf,
                Biome::Forest
                | Biome::OldGrowthBirchForest
                | Biome::Taiga
                | Biome::SnowyTaiga
                | Biome::OldGrowthPineTaiga
                | Biome::OldGrowthSpruceTaiga
                | Biome::Grove
                | Biome::SavannaPlateau
                | Biome::WindsweptForest
                | Biome::SparseJungle
                | Biome::WoodedBadlands,
            ) => 0.5,
            (
                MobKind::Fox,
                Biome::Taiga
                | Biome::SnowyTaiga
                | Biome::OldGrowthPineTaiga
                | Biome::OldGrowthSpruceTaiga
                | Biome::Grove,
            ) => 0.5,
            (MobKind::Parrot, Biome::Jungle | Biome::BambooJungle | Biome::SparseJungle) => 0.4,
            (
                MobKind::Rabbit,
                Biome::Desert
                | Biome::SnowyPlains
                | Biome::SnowyTaiga
                | Biome::Grove
                | Biome::Meadow
                | Biome::FlowerForest
                | Biome::Taiga
                | Biome::CherryGrove,
            ) => 0.4,
            (MobKind::Goat, Biome::FrozenPeaks | Biome::JaggedPeaks | Biome::SnowySlopes) => 0.5,
            (MobKind::Wolf | MobKind::Fox | MobKind::Parrot | MobKind::Rabbit | MobKind::Goat, _) => 0.0,
            (MobKind::Turtle, Biome::Beach) => 0.4,
            (MobKind::Turtle, _) => 0.0,
            (MobKind::GlowSquid | MobKind::Axolotl | MobKind::Guardian, _) => 1.0,
            (
                MobKind::TropicalFish,
                Biome::LushCaves
                | Biome::MangroveSwamp
                | Biome::WarmOcean
                | Biome::LukewarmOcean
                | Biome::DeepLukewarmOcean,
            ) => 1.0,
            (MobKind::Pufferfish, Biome::WarmOcean | Biome::LukewarmOcean | Biome::DeepLukewarmOcean) => 1.0,
            (MobKind::Cod, b) if b.is_ocean() && b != Biome::WarmOcean => 1.0,
            (MobKind::Salmon, b)
                if b.is_river()
                    || matches!(
                        b,
                        Biome::ColdOcean | Biome::DeepColdOcean | Biome::FrozenOcean | Biome::DeepFrozenOcean
                    ) =>
            {
                1.0
            }
            (MobKind::Squid, b)
                if (b.is_ocean() || b.is_river())
                    && !matches!(b, Biome::FrozenOcean | Biome::DeepFrozenOcean | Biome::FrozenRiver) =>
            {
                1.0
            }
            (MobKind::Dolphin, b) if b.is_ocean() && !matches!(b, Biome::FrozenOcean | Biome::DeepFrozenOcean) => 0.2,
            (k, _) if k.is_aquatic() => 0.0,
            _ => 1.0,
        }
    }

    /// Most mobs of this kind that spawn naturally around the player.
    pub fn spawn_cap(self, dimension: Dimension) -> usize {
        match self {
            MobKind::Zombie => 4,
            MobKind::Witch => 1,
            MobKind::Ghast => 4,
            MobKind::ZombifiedPiglin => 8,
            MobKind::Piglin | MobKind::Hoglin => 4,
            MobKind::Enderman if dimension == Dimension::End => 12,
            MobKind::Enderman => 1,
            k if k.is_hostile() => 3,
            _ => 4,
        }
    }

    /// What smite and bane of arthropods count it as.
    pub fn creature(self) -> crate::enchant::Creature {
        use crate::enchant::Creature;
        match self {
            MobKind::Zombie
            | MobKind::Husk
            | MobKind::Drowned
            | MobKind::ZombieVillager
            | MobKind::Skeleton
            | MobKind::WitherSkeleton
            | MobKind::ZombifiedPiglin
            | MobKind::Zoglin => Creature::Undead,
            MobKind::Spider | MobKind::CaveSpider | MobKind::Silverfish => Creature::Arthropod,
            _ => Creature::Other,
        }
    }

    pub(super) fn burns_in_sun(self) -> bool {
        matches!(self, MobKind::Zombie | MobKind::Drowned | MobKind::ZombieVillager | MobKind::Skeleton)
    }

    fn wander_speed(self) -> f64 {
        match self {
            MobKind::Pig => 1.3,
            MobKind::Cow
            | MobKind::Witch
            | MobKind::Villager
            | MobKind::WanderingTrader
            | MobKind::TraderLlama
            | MobKind::IronGolem
            | MobKind::SnowGolem
            | MobKind::Zombie
            | MobKind::Husk
            | MobKind::Drowned
            | MobKind::ZombieVillager
            | MobKind::Creeper
            | MobKind::ZombifiedPiglin => 1.1,
            // Java's 0.35 speed at its 0.6 idle multiplier.
            MobKind::Piglin | MobKind::PiglinBrute => 1.0,
            // Java's 0.3 speed at its 0.4 idle multiplier.
            MobKind::Hoglin | MobKind::Zoglin => 0.6,
            // Java's 0.175 speed.
            MobKind::Strider => 0.84,
            MobKind::Sheep | MobKind::Skeleton | MobKind::WitherSkeleton | MobKind::Blaze => 1.2,
            MobKind::Chicken | MobKind::Slime | MobKind::MagmaCube => 1.0,
            MobKind::Ghast => 4.0,
            MobKind::Spider | MobKind::CaveSpider | MobKind::Enderman | MobKind::Silverfish => 1.4,
            k if k.is_aquatic() => 1.2,
            MobKind::Cat | MobKind::Pillager => 1.2,
            _ => 1.2,
        }
    }

    fn chase_speed(self) -> f64 {
        match self {
            MobKind::Enderman => 4.5,
            // Java's 0.35 movement speed against the zombie's 0.23.
            MobKind::Piglin | MobKind::PiglinBrute => 3.6,
            MobKind::Hoglin | MobKind::Zoglin => 3.1,
            MobKind::Strider => 0.84,
            MobKind::Spider | MobKind::CaveSpider => 3.0,
            MobKind::ZombifiedPiglin | MobKind::Silverfish => 2.8,
            MobKind::WitherSkeleton => 2.4,
            MobKind::Skeleton => 2.2,
            MobKind::Creeper => 2.0,
            _ => 2.4,
        }
    }

    /// Melee damage and the death message it gives.
    pub(super) fn melee(self) -> (f32, &'static str) {
        match self {
            MobKind::CaveSpider => (2.0, "was slain by a cave spider"),
            MobKind::Spider => (2.0, "was slain by a spider"),
            MobKind::ZombifiedPiglin => (5.0, "was slain by a zombified piglin"),
            MobKind::Enderman => (7.0, "was slain by an enderman"),
            MobKind::Blaze => (6.0, "was slain by a blaze"),
            MobKind::WitherSkeleton => (8.0, "was slain by a wither skeleton"),
            MobKind::Husk => (3.0, "was slain by a husk"),
            MobKind::Drowned => (3.0, "was slain by a drowned"),
            MobKind::Silverfish => (1.0, "was slain by a silverfish"),
            MobKind::IronGolem => (15.0, "was slain by an iron golem"),
            MobKind::SnowGolem => (0.0, "was slain by a snow golem"),
            MobKind::ZombieVillager => (3.0, "was slain by a zombie villager"),
            MobKind::Piglin => (5.0, "was slain by a piglin"),
            MobKind::PiglinBrute => (7.0, "was slain by a piglin brute"),
            MobKind::Hoglin => (6.0, "was slain by a hoglin"),
            MobKind::Zoglin => (6.0, "was slain by a zoglin"),
            _ => (3.0, "was slain by a zombie"),
        }
    }

    /// Loot for a player kill: (item, min, max) rolls. A negative minimum
    /// makes the drop a chance (Java's spider eye: -1..1 is one in three).
    pub(super) fn loot(self) -> &'static [(Item, i8, u8)] {
        const WOOL: Item = Item::from_block(Block::WOOL);
        const SPONGE: Item = Item::from_block(crate::world::overworld_blocks::WET_SPONGE);
        const SEAGRASS: Item = Item::from_block(crate::world::overworld_blocks::SEAGRASS);
        const POPPY: Item = Item::from_block(Block::POPPY);
        match self {
            MobKind::Pig => &[(Item::RAW_PORKCHOP, 1, 3)],
            MobKind::Cow => &[(Item::RAW_BEEF, 1, 3), (Item::LEATHER, 0, 2)],
            MobKind::Sheep => &[(WOOL, 1, 1), (Item::RAW_MUTTON, 1, 2)],
            MobKind::Chicken => &[(Item::RAW_CHICKEN, 1, 1), (Item::FEATHER, 0, 2)],
            MobKind::Zombie | MobKind::Husk | MobKind::Drowned | MobKind::ZombieVillager => {
                &[(Item::ROTTEN_FLESH, 0, 2)]
            }
            MobKind::Skeleton => &[(Item::BONE, 0, 2), (Item::ARROW, 0, 2)],
            MobKind::Creeper => &[(Item::GUNPOWDER, 0, 2)],
            MobKind::Spider | MobKind::CaveSpider => &[(Item::STRING, 0, 2), (Item::SPIDER_EYE, -1, 1)],
            MobKind::ZombifiedPiglin => &[(Item::ROTTEN_FLESH, 0, 1), (Item::GOLD_NUGGET, 0, 1)],
            MobKind::Enderman => &[(Item::ENDER_PEARL, 0, 1)],
            MobKind::Blaze => &[(Item::BLAZE_ROD, 0, 1)],
            // Piglins drop only what they carry (`nether::equipment_drops`).
            MobKind::Silverfish
            | MobKind::Villager
            | MobKind::WanderingTrader
            | MobKind::Piglin
            | MobKind::PiglinBrute
            | MobKind::Camel => &[],
            MobKind::TraderLlama | MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama => {
                &[(Item::LEATHER, 0, 2)]
            }
            MobKind::IronGolem => &[(Item::IRON_INGOT, 3, 5), (POPPY, 0, 2)],
            MobKind::SnowGolem => &[(Item::SNOWBALL, 0, 15)],
            MobKind::Slime => &[(Item::SLIME_BALL, 0, 2)],
            MobKind::MagmaCube => &[(Item::MAGMA_CREAM, -2, 1)],
            MobKind::Ghast => &[(Item::GUNPOWDER, 0, 2), (Item::GHAST_TEAR, 0, 1)],
            // Babies drop nothing (`nether::babies_drop_nothing`).
            MobKind::Hoglin => &[(Item::RAW_PORKCHOP, 2, 4), (Item::LEATHER, 0, 1)],
            MobKind::Zoglin => &[(Item::ROTTEN_FLESH, 1, 3)],
            MobKind::Strider => &[(Item::STRING, 2, 5)],
            MobKind::Cod | MobKind::Dolphin => &[(Item::COD, 1, 1)],
            MobKind::Salmon => &[(Item::SALMON, 1, 1)],
            MobKind::TropicalFish => &[(Item::TROPICAL_FISH, 1, 1)],
            MobKind::Pufferfish => &[(Item::PUFFERFISH, 1, 1)],
            MobKind::Squid => &[(Item::INK_SAC, 1, 3)],
            MobKind::GlowSquid => &[(Item::GLOW_INK_SAC, 1, 3)],
            MobKind::Guardian => &[(Item::PRISMARINE_SHARD, 0, 2)],
            MobKind::ElderGuardian => &[(Item::PRISMARINE_SHARD, 0, 2), (SPONGE, 1, 1)],
            MobKind::Axolotl | MobKind::Cat | MobKind::Pillager => &[],
            MobKind::Turtle => &[(SEAGRASS, 0, 2)],
            MobKind::Wolf | MobKind::Fox | MobKind::Goat => &[],
            MobKind::Parrot => &[(Item::FEATHER, 1, 2)],
            MobKind::Rabbit => &[(Item::RAW_RABBIT, 0, 1), (Item::RABBIT_HIDE, 0, 1)],
            // The skull is rolled in `drops`.
            MobKind::WitherSkeleton => &[(Item::COAL, 0, 1), (Item::BONE, 0, 2)],
            // Java rolls a few of these; each is rolled on its own here.
            MobKind::Witch => &[
                (Item::GLASS_BOTTLE, 0, 2),
                (Item::GLOWSTONE_DUST, 0, 2),
                (Item::GUNPOWDER, 0, 2),
                (Item::REDSTONE, 0, 2),
                (Item::SPIDER_EYE, 0, 2),
                (Item::SUGAR, 0, 2),
                (Item::STICK, 0, 2),
            ],
        }
    }

    /// Rolls the drops for killing one of these; `looting` adds
    /// `round(looting * uniform(0, 1))` to each (Java's
    /// `enchanted_count_increase`).
    pub fn drops(self, rng: &mut Rng, looting: u8) -> Vec<(Item, u8)> {
        let mut out: Vec<(Item, u8)> = self
            .loot()
            .iter()
            .map(|&(item, lo, hi)| {
                let span = hi as i32 - lo as i32 + 1;
                // Java adds this even to a zero roll (looting raises the
                // maximum, so a 0-1 drop becomes 0-2 with Looting I).
                // Sheep's wool pool has no enchanted_count_increase.
                let extra = if looting > 0
                    && item != Item::from(Block::WOOL)
                    && item != Item::from(crate::world::overworld_blocks::WET_SPONGE)
                {
                    (looting as f32 * rng.next_f32()).round() as i32
                } else {
                    0
                };
                let base = lo as i32 + (rng.next_f32() * span as f32) as i32;
                (item, (base.max(0) + extra).clamp(0, u8::MAX as i32) as u8)
            })
            .filter(|&(_, n)| n > 0)
            .collect();
        if matches!(self, MobKind::Guardian | MobKind::ElderGuardian) {
            let cod_weight = if self == MobKind::ElderGuardian { 3 } else { 2 };
            let roll = rng.next_int(cod_weight + 3);
            if roll < cod_weight + 2 {
                out.push((
                    if roll < cod_weight { Item::COD } else { Item::PRISMARINE_CRYSTALS },
                    1 + (looting as f32 * rng.next_f32()).round() as u8,
                ));
            }
        }
        if matches!(self, MobKind::Zombie | MobKind::Husk | MobKind::ZombieVillager)
            && rng.chance(0.025 + 0.01 * looting as f32)
        {
            // One equally weighted entry from a single rare pool, not
            // independent vegetable rolls. Player-kill gating is at the drop sink.
            out.push(([Item::IRON_INGOT, Item::CARROT, Item::POTATO][rng.next_int(3) as usize], 1));
        }
        if self == MobKind::Drowned && rng.chance(0.11 + 0.02 * looting as f32) {
            out.push((Item::COPPER_INGOT, 1));
        }
        if self == MobKind::WitherSkeleton && rng.chance(0.025 + 0.01 * looting as f32) {
            out.push((Item::WITHER_SKULL, 1));
        }
        out
    }

    /// Experience for killing one (Java's: 5 for monsters, 1-3 for animals).
    pub fn xp(self, rng: &mut Rng) -> u32 {
        match self {
            MobKind::Villager | MobKind::IronGolem | MobKind::SnowGolem | MobKind::WanderingTrader => 0,
            MobKind::Blaze | MobKind::Guardian | MobKind::ElderGuardian => 10,
            k if k.is_hostile() => 5,
            _ => 1 + (rng.next_f32() * 3.0) as u32,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Ai {
    /// Standing still, glancing around.
    Idle,
    /// Walking in `move_yaw`.
    Wander,
    /// Running from an attacker (passive mobs), changing direction every so often.
    Panic,
    /// Going after the player (hostile mobs), each in its own way.
    Chase,
}

pub struct Mob {
    /// Transient activity snapshot, refreshed before every entity tick. Never saved.
    pub(crate) simulation_active: bool,
    pub kind: MobKind,
    pub name: crate::inventory::StackName,
    pub leash: Option<super::leash::Leash>,
    pub leash_pos: Option<DVec3>,
    pub animal: Option<Box<super::animals::State>>,
    pub villager: Option<Box<super::villager::Villager>>,
    /// Piglin gear, gold, anger and zombification (see `entity::nether`).
    pub mount: Option<Box<super::mounts::State>>,
    pub nether: Option<Box<super::nether::NetherMob>>,
    /// Identity other mobs aim at, assigned by `Entities` (0 until then).
    pub uid: u32,
    /// Saved with the level and never despawned (bastion residents, piglins
    /// that picked something up), like Java's persistence flag.
    pub persistent: bool,
    /// Java slime size (1, 2 or 4).
    pub size: u8,
    /// A baby zombie: half size and 50% faster.
    pub baby: bool,
    /// Seconds a baby turtle has grown (it grows up after Java's 20 minutes).
    pub grow: f32,
    pub wool_color: crate::color::DyeColor,
    pub sheared: bool,
    /// Witch: seconds left drinking, and the potion it is drinking.
    drink_left: f32,
    drinking: &'static str,
    /// Negative while a baby. Java's chicks start at -24000 and grow one tick at a time.
    pub age: i32,
    /// Seconds until a grown chicken lays an egg (Java's 6000–12000 ticks).
    pub(crate) egg_timer: f32,
    hop_left: f32,
    hop_delay: f32,
    pub(super) difficulty: crate::simulation::difficulty::Difficulty,
    /// Java's `DifficultyInstance.getEffectiveDifficulty` where the mob stands
    /// (chunk inhabited time is not tracked), refreshed every update.
    pub(super) local_difficulty: f32,
    /// Cart this mob is sitting in.
    pub riding: Option<u32>,
    /// Feet position (bottom centre of the box).
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    /// Body facing, same convention as the player: forward = (cos, 0, sin).
    pub yaw: f32,
    /// Head yaw relative to the body, and pitch (positive looks up).
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub health: f32,
    pub effects: crate::simulation::effects::Effects,
    /// Seconds of hurt flash left.
    pub hurt: f32,
    /// Seconds since death, while the death animation plays.
    pub dying: Option<f32>,
    pub on_ground: bool,
    pub in_water: bool,
    pub burning: bool,
    /// Walk cycle phase (radians) and amplitude (0..1).
    pub limb_phase: f32,
    pub limb_amp: f32,
    /// Seconds left of the arm swing after an attack.
    pub attack_anim: f32,
    /// Sky light estimate at the mob, 0..1 (refreshed a few times a second).
    pub sky_light: f32,
    /// Torch light where it stands, 0..1.
    pub block_light: f32,
    /// Creeper fuse: seconds lit, 0 when not fusing.
    pub fuse: f32,
    /// Seconds a hit spider stays angry in daylight.
    provoked: f32,
    pub(super) ai: Ai,
    pub(super) ai_timer: f32,
    pub(super) move_yaw: f32,
    head_target: (f32, f32),
    head_timer: f32,
    pub(super) attack_cooldown: f32,
    burn_timer: f32,
    fire_left: f32,
    /// Java credits environmental deaths for five seconds after a player hit.
    pub(super) player_hit_left: f32,
    light_timer: f32,
    /// Blocked horizontally on the last physics step.
    blocked: bool,
    /// Seconds left sidestepping around an obstacle while chasing, and
    /// which side (+1 / -1).
    detour: f32,
    detour_side: f64,
    /// Seconds until the next idle call.
    ambient_timer: f32,
    /// Helmet, chest, leggings, boots. Zombies and skeletons roll these.
    pub armor: [Option<super::armor::ArmorKind>; 4],
    /// Bit per armor slot: enchantment glint.
    pub armor_glint: u8,
    /// A hurt or death cry waiting to be reported by the next update.
    cry: Option<MobSound>,
    /// Endermen: teleport away at the next update (an arrow, water, fire).
    pub teleport_pending: bool,
    /// Endermen: seconds a player has been looking it in the eye.
    stare: f32,
    /// Endermen: seconds spent far from the target, and since the last
    /// time it turned on someone.
    teleport_timer: f32,
    since_anger: f32,
    /// Blazes: glowing while charging a burst; which shot of the burst is
    /// next; rising to stay `height_offset` above the target's eyes.
    pub charged: bool,
    attack_step: u8,
    lift: bool,
    height_offset: f32,
    offset_timer: f32,
    /// Feet of whatever this mob is walking toward (a villager, monster, or player).
    pub(super) hunt: Option<DVec3>,
    /// `think` wants a melee swing resolved against `hunt` this tick.
    pub(super) strike: bool,
    /// Player-built golems stay loaded instead of despawning.
    pub built: bool,
    /// Seconds of splash Weakness left. A golden apple starts a cure while this is up.
    pub(super) weakness_left: f32,
    /// Seconds until a weakened zombie villager becomes a villager again. Zero means not curing.
    pub(super) convert_left: f32,
    pub(super) convert_tick: f32,
    pub aquatic: Option<Box<super::aquatic::State>>,
    pub trader: Option<Box<super::wandering_trader::Trader>>,
    pub(super) trader_night: bool,
    pub(super) convert_by: Option<super::PlayerId>,
    pub(super) angry_player: Option<super::PlayerId>,
    /// A zombified piglin's main hand: `None` is the spawn default (a golden
    /// sword); a converted piglin keeps its crossbow, axe or empty hand.
    pub held: Option<super::nether::Weapon>,
    /// Java ground navigation: the path being walked (never saved).
    pub(super) nav: super::path::Navigation,
    /// Ticks until a pursuit may plan again, and where its goal was then.
    pub(super) repath_in: i32,
    pub(super) pathed: Option<DVec3>,
    /// `MoveControl` asked for a jump this tick.
    pub(super) jump: bool,
    /// This tick's movement follows a path, which already judged its drops.
    pub(super) pathing: bool,
    /// The current stroll or panic walks a planned path (Java's
    /// `RandomStrollGoal`/`PanicGoal`) rather than a heading.
    pub(super) path_move: bool,
    /// Seconds since it was hurt or a player came within 32 blocks (Java's
    /// `noActionTime`): idle mobs stop strolling and may despawn.
    pub(super) idle: f32,
    /// Java's goal selector for the shared movement goals (never saved).
    pub(crate) goals: goals::Goals,
}

impl Mob {
    /// Create a healthy, idle mob at `pos` with `yaw` in radians; snap its initial render position.
    pub fn new(kind: MobKind, pos: DVec3, yaw: f32) -> Self {
        let mount = super::mounts::rideable(kind).then(|| {
            Box::new(super::mounts::State::new(
                kind,
                &mut Rng::new(pos.x.to_bits() ^ pos.z.to_bits().rotate_left(17) ^ yaw.to_bits() as u64),
            ))
        });
        let health = mount.as_ref().map_or(kind.max_health(), |s| s.max_health);
        Self {
            simulation_active: true,
            mount,
            kind,
            name: Default::default(),
            leash: None,
            leash_pos: None,
            animal: kind.has_animal_state().then(|| Box::new(super::animals::State::new(pos))),
            villager: matches!(kind, MobKind::Villager | MobKind::ZombieVillager | MobKind::WanderingTrader)
                .then(|| Box::new(super::villager::Villager::new(0, 0))),
            nether: super::nether::NetherMob::for_kind(kind),
            aquatic: kind.is_aquatic().then(|| Box::new(super::aquatic::State::new())),
            uid: 0,
            persistent: super::mounts::equine(kind),
            size: 1,
            baby: false,
            grow: 0.0,
            wool_color: crate::color::DyeColor::White,
            sheared: false,
            drink_left: 0.0,
            drinking: "",
            age: 0,
            egg_timer: if kind == MobKind::Chicken { 300.0 + yaw.rem_euclid(TAU) / TAU * 300.0 } else { f32::MAX },
            hop_left: 0.0,
            hop_delay: 1.0,
            difficulty: Default::default(),
            local_difficulty: 0.0,
            riding: None,
            pos,
            previous_pos: pos,
            vel: DVec3::ZERO,
            yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            health,
            effects: Default::default(),
            hurt: 0.0,
            dying: None,
            on_ground: false,
            in_water: false,
            burning: false,
            limb_phase: 0.0,
            limb_amp: 0.0,
            attack_anim: 0.0,
            sky_light: 1.0,
            block_light: 0.0,
            ai: Ai::Idle,
            ai_timer: 1.0,
            move_yaw: yaw,
            head_target: (0.0, 0.0),
            head_timer: 0.0,
            attack_cooldown: 0.0,
            burn_timer: 0.0,
            fire_left: 0.0,
            player_hit_left: 0.0,
            light_timer: 0.0,
            fuse: 0.0,
            provoked: 0.0,
            blocked: false,
            detour: 0.0,
            detour_side: 1.0,
            ambient_timer: 2.0 + (yaw * 1000.0).rem_euclid(10.0),
            armor: [None; 4],
            armor_glint: 0,
            cry: None,
            teleport_pending: false,
            stare: 0.0,
            teleport_timer: 0.0,
            since_anger: 1e3,
            charged: false,
            attack_step: 0,
            lift: false,
            height_offset: 0.5,
            offset_timer: 0.0,
            hunt: None,
            strike: false,
            built: false,
            weakness_left: 0.0,
            convert_left: 0.0,
            convert_tick: 0.0,
            trader_night: false,
            convert_by: None,
            angry_player: None,
            held: None,
            trader: matches!(kind, MobKind::WanderingTrader | MobKind::TraderLlama)
                .then(|| Box::new(super::wandering_trader::Trader::new(pos))),
            nav: Default::default(),
            repath_in: 0,
            pathed: None,
            jump: false,
            pathing: false,
            path_move: false,
            idle: 0.0,
            goals: Default::default(),
        }
    }

    pub(super) fn melee_attack(&self) -> (f32, &'static str) {
        let (base, cause) = self.kind.melee();
        let strength = if self.convert_left > 0.0 { 3.0 } else { 0.0 };
        let weakness = if self.weakness_left > 0.0 { 4.0 } else { 0.0 };
        // Java adds the main-hand item's attack modifier to the zombified
        // piglin's base 5.
        let held = if self.kind == MobKind::ZombifiedPiglin {
            self.held.unwrap_or(super::nether::Weapon::GoldenSword).attack_bonus()
        } else {
            0.0
        };
        ((base + strength + held - weakness).max(0.0), cause)
    }

    /// Retaliation from the armor worn by a player struck in melee.
    pub(super) fn thorns_response(
        &mut self,
        target: &super::Target,
        knockback: DVec3,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        // Thorns: each piece has a 15% chance per level
        // to hit back for a uniform 1.0-5.0 damage.
        for level in target.thorns.into_iter().filter(|&l| l > 0) {
            if rng.chance(0.15 * level as f32) {
                let back = rng.range(1.0, 5.0);
                self.player_hit();
                if self.damage(back, Some(-knockback * 0.5 + DVec3::Y * 3.0), rng) {
                    events.push(EntityEvent::MobKilled {
                        kind: self.kind,
                        pos: self.pos,
                        burning: self.burning || target.held_enchants.has(crate::enchant::Enchantment::FireAspect),
                        player_kill: true,
                        looting: target.held_enchants.level(crate::enchant::Enchantment::Looting),
                    });
                }
            }
        }
    }

    /// A player hit this iron golem recently, so it may hit back.
    pub(super) fn angry_at_player(&self) -> bool {
        self.kind == MobKind::IronGolem && !self.built && self.player_hit_left > 0.0
    }

    /// The cure finished: a villager again, with Java's major_positive discount.
    pub(super) fn finish_cure(&mut self) {
        self.kind = MobKind::Villager;
        self.health = MobKind::Villager.max_health();
        self.baby = self.age < 0;
        self.weakness_left = 0.0;
        self.convert_left = 0.0;
        self.convert_tick = 0.0;
        // Naturally generated equipment is discarded by Java's conversion.
        // Picked-up equipment and binding curses are not represented on mobs.
        self.armor = [None; 4];
        self.armor_glint = 0;
        if let Some(v) = &mut self.villager {
            v.reputation = 0;
            if let Some(owner) = self.convert_by
                && v.gossip.entries.get(&owner).is_none_or(|g| g[2] < 20)
            {
                v.gossip.add(owner, 2, 20);
                v.gossip.add(owner, 3, 25);
            }
            self.convert_by = None;
            v.sleeping = false;
            v.trading = false;
            v.fleeing = false;
            v.goal = None;
        }
    }

    pub fn shape(&self) -> Shape {
        if self.kind.is_cube() {
            Shape::new(0.255 * self.size as f64, 0.51 * self.size as f64)
        } else if self.baby || self.age < 0 {
            let s = self.kind.shape();
            Shape::new(s.half_width * 0.5, s.height * 0.5)
        } else {
            self.kind.shape()
        }
    }

    pub fn set_size(&mut self, size: u8) {
        self.size = if size >= 4 {
            4
        } else if size >= 2 {
            2
        } else {
            1
        };
        self.health = (self.size as f32).powi(2);
    }

    pub fn aabb(&self) -> (DVec3, DVec3) {
        self.shape().aabb(self.pos)
    }

    pub fn alive(&self) -> bool {
        self.dying.is_none()
    }

    /// The player an angry iron golem is set against.
    pub fn angry_at(&self) -> Option<super::PlayerId> {
        self.angry_player
    }

    /// Whether a neutral mob is currently hostile (hit, or angered by a neighbour's hit).
    pub fn is_provoked(&self) -> bool {
        self.provoked > 0.0
    }

    /// Turns a neutral mob hostile for `secs`.
    pub fn anger(&mut self, secs: f32) {
        self.provoked = self.provoked.max(secs);
    }

    /// Takes a hit. `knockback` replaces the horizontal velocity (its y is
    /// the upward pop). Returns `true` if this killed the mob.
    pub fn damage(&mut self, amount: f32, knockback: Option<DVec3>, rng: &mut Rng) -> bool {
        if !self.alive() {
            return false;
        }
        // Full resistance (iron golems) cancels the push and leaves the motion alone, as `knockback` returns early.
        let taken = 1.0 - self.knockback_resistance();
        if let Some(kb) = knockback.filter(|_| taken > 0.0).map(|kb| kb * taken) {
            self.vel.x = kb.x;
            self.vel.z = kb.z;
            self.vel.y = self.vel.y.max(kb.y);
        }
        self.take_damage(amount, knockback.is_some(), rng)
    }

    /// Takes a hit followed by Java's knockbacks in order (`hurtServer`'s
    /// default push, then the attacker's extra one), each halving the motion
    /// left by the previous. Returns `true` if this killed the mob.
    pub fn damage_pushed(&mut self, amount: f32, pushes: &[crate::knockback::Push], rng: &mut Rng) -> bool {
        if !self.alive() {
            return false;
        }
        for &push in pushes {
            self.knock_back(push, rng);
        }
        self.take_damage(amount, true, rng)
    }

    /// Punch's shove of an arrow with `motion` (blocks per tick), added to the mob's velocity.
    pub fn shove_punch(&mut self, level: u32, motion: DVec3) {
        if let Some(shove) = crate::knockback::arrow_push(level, self.knockback_resistance(), motion) {
            self.vel += shove * crate::knockback::TICK_RATE;
        }
    }

    /// Java's `LivingEntity.knockback`, applied to this mob's velocity in blocks per second.
    pub fn knock_back(&mut self, push: crate::knockback::Push, rng: &mut Rng) {
        use crate::knockback::{TICK_RATE, apply};
        let mut jitter = || f64::from(rng.next_f32());
        if let Some(motion) =
            apply(self.vel / TICK_RATE, push, self.knockback_resistance(), self.on_ground, &mut jitter)
        {
            self.vel = motion * TICK_RATE;
        }
    }

    fn take_damage(&mut self, amount: f32, attacked: bool, rng: &mut Rng) -> bool {
        let amount = if self.kind == MobKind::MagmaCube {
            crate::simulation::survival::armor_reduce(amount, self.size as u32 * 3, 0.0)
        } else {
            amount
        };
        if let Some(a) = &mut self.animal {
            a.love = 0.0;
            a.courtship = 0.0;
        }
        let armor = self.mount.as_ref().and_then(|s| s.slots[1]).map_or(0, |s| super::mounts::armor_points(s.item));
        let amount = if armor > 0 { crate::simulation::survival::armor_reduce(amount, armor, 0.) } else { amount };
        let amount = amount * self.effects.resistance_factor();
        self.health -= self.effects.absorb(amount);
        if self.kind == MobKind::Axolotl && self.in_water && self.health > 0.0 && self.health < 7.0 && rng.chance(0.33)
        {
            self.aquatic.as_mut().unwrap().play_dead = 10.0;
        }
        self.hurt = HURT_TIME;
        self.idle = 0.0;
        // Java's panic goals react to any `PANIC_CAUSES` damage, except wolves
        // (`PANIC_ENVIRONMENTAL_CAUSES` only: an attack doesn't scare them)
        // and golems, which have none.
        let scared = match self.kind {
            MobKind::IronGolem | MobKind::SnowGolem => false,
            MobKind::Wolf => !attacked,
            k => !k.is_hostile(),
        };
        if scared {
            // `PanicGoal.shouldPanic` reads `getLastDamageSource`, which
            // forgets the damage after 40 ticks; the run then ends once its
            // path is done.
            self.ai = Ai::Panic;
            self.ai_timer = PANIC_MEMORY;
            self.move_yaw = rng.range(0.0, TAU);
            self.stop_navigation();
            self.path_move = true;
        }
        if self.kind == MobKind::IronGolem && !self.built && self.player_hit_left > 0.0 {
            self.player_hit_left = self.player_hit_left.max(15.0);
        }
        self.provoked =
            self.provoked.max(if self.kind == MobKind::Enderman { ENDERMAN_ANGER_TIME } else { PROVOKED_TIME });
        if self.health <= 0.0 {
            self.dying = Some(0.0);
            self.cry = Some(MobSound::Death(self.kind));
            return true;
        }
        self.cry = Some(MobSound::Hurt(self.kind));
        false
    }

    /// Sets it alight for at least `secs` (fire aspect, flame arrows).
    pub fn ignite(&mut self, secs: f32) {
        if !self.kind.fire_immune() && !self.in_water {
            self.fire_left = self.fire_left.max(secs);
            self.burning = self.alive() && self.fire_left > 0.0;
        }
    }

    pub(super) fn player_hit(&mut self) {
        self.player_hit_left = 5.0;
    }

    /// Advances the mob by `dt` seconds.
    pub fn update<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let event_start = events.len();
        let dtf = dt as f32;
        self.idle += dtf;
        if self.alive() {
            let outcome = self.effects.tick(dtf);
            if outcome.heal > 0.0 {
                self.health = (self.health + outcome.heal).min(self.max_health());
            }
            let damage = outcome.damage + outcome.wither + outcome.poison.min((self.health - 1.0).max(0.0));
            if damage > 0.0 {
                self.damage(damage, None, rng);
            }
        }
        self.hop_left -= dtf;
        if self.kind.is_cube() && self.on_ground && self.hop_left <= 0.0 {
            self.hop_delay = rng.range(0.5, 1.5);
        }
        if let Some(sound) = self.cry.take() {
            events.push(EntityEvent::Sound { sound, pos: self.pos + DVec3::Y * (self.shape().height * 0.8) });
        }
        self.ambient_timer -= dtf;
        if self.ambient_timer <= 0.0 {
            self.ambient_timer = rng.range(7.0, 18.0);
            if self.alive() && self.kind != MobKind::Creeper {
                let sound = MobSound::Ambient(self.kind);
                events.push(EntityEvent::Sound { sound, pos: self.pos + DVec3::Y * (self.shape().height * 0.8) });
            }
        }
        if self.alive()
            && let Some(vanished) = self.update_trader(dtf, self.trader_night)
        {
            events.push(EntityEvent::Sound { sound: MobSound::TraderDrink(vanished), pos: self.pos + DVec3::Y });
        }
        self.hurt = (self.hurt - dtf).max(0.0);
        self.provoked = (self.provoked - dtf).max(0.0);
        if matches!(self.kind, MobKind::Wolf | MobKind::Llama) && self.provoked == 0.0 {
            self.angry_player = None;
        }
        self.attack_cooldown -= dtf;
        self.player_hit_left = (self.player_hit_left - dtf).max(0.0);
        self.weakness_left = (self.weakness_left - dtf).max(0.0);
        self.advance_cure(dtf, world, rng);
        if let Some(v) = &mut self.villager {
            v.bell_hide = (v.bell_hide - dtf).max(0.0);
        }
        self.attack_anim = (self.attack_anim - dtf).max(0.0);

        self.light_timer -= dtf;
        if self.light_timer <= 0.0 {
            self.light_timer = 0.25;
            let at = self.pos + DVec3::new(0.0, self.shape().height * 0.6, 0.0);
            self.sky_light = sky_light(world, at);
            self.block_light = world.block_light(at.floor().as_ivec3()) as f32 / 15.0;
        }

        if self.teleport_pending && self.alive() {
            self.teleport_pending = false;
            self.teleport(world, rng, None, events);
        }

        super::aquatic::environment(self, dtf, world);
        // Bred horses and donkeys age with the shared animal clock; mules only here.
        if super::mounts::equine(self.kind) && self.animal.is_none() && self.baby && self.age < 0 {
            self.grow += dtf * 20.;
            let ticks = self.grow.floor() as i32;
            self.grow -= ticks as f32;
            self.age = (self.age + ticks).min(0);
            self.baby = self.age < 0;
        }
        self.pathing = false;
        let (wish, speed) = if let Some(t) = &mut self.dying {
            *t += dtf;
            (None, 0.0)
        } else {
            super::mounts::control(self, dt, rng).unwrap_or_else(|| self.think(dtf, world, ctx, rng, events))
        };

        // Don't walk off tall drops; wanderers pick another direction.
        let controlled = self.mount.as_ref().is_some_and(|s| s.controlled());
        let pathing = self.pathing;
        let wish = wish.filter(|&dir| {
            if controlled || pathing {
                return true;
            }
            let safe = match self.lava_step(world, dir) {
                Some(safe) => safe,
                None => !self.on_ground || self.in_water || !is_cliff(world, self.pos, dir, self.shape()),
            };
            if !safe && matches!(self.ai, Ai::Wander | Ai::Panic) {
                self.ai_timer = self.ai_timer.min(0.3);
                self.move_yaw = rng.range(0.0, TAU);
            }
            safe
        });

        // Turn the body toward the direction of travel.
        if let Some(dir) = wish.filter(|_| !controlled) {
            let target = (dir.z as f32).atan2(dir.x as f32);
            self.yaw = turn_toward(self.yaw, target, 8.0 * dtf);
        }

        if std::mem::take(&mut self.jump) && self.on_ground && wish.is_some() && !controlled && self.alive() {
            self.vel.y = JUMP_VELOCITY;
        }
        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        if self.riding.is_some() || self.animal.as_ref().is_some_and(|a| a.perched.is_some()) {
            self.vel = DVec3::ZERO;
            self.on_ground = true;
        } else {
            for _ in 0..steps {
                let controlled = self.mount.as_ref().is_some_and(|s| s.controlled());
                self.physics_step(h, world, wish, speed * if controlled { 1. } else { self.speed_factor() });
            }
        }
        if let Some(s) = &mut self.mount {
            let damage = std::mem::take(&mut s.rider_fall);
            if damage > 0.
                && let Some(player) = s.rider
            {
                events.push(EntityEvent::PlayerFall { player, damage });
            }
        }
        if self.health <= 0.0 && self.dying.is_none() {
            self.dying = Some(0.0);
            self.cry = Some(MobSound::Death(self.kind));
            events.push(EntityEvent::MobKilled {
                kind: self.kind,
                pos: self.pos,
                burning: false,
                player_kill: self.player_hit_left > 0.0,
                looting: 0,
            });
        }

        // Walk cycle follows ground speed.
        let hspeed = self.vel.x.hypot(self.vel.z) as f32;
        self.limb_phase = (self.limb_phase + hspeed * dtf * 3.2) % (TAU * 64.0);
        let amp = (hspeed / 2.0).min(1.0);
        self.limb_amp += (amp - self.limb_amp) * (dtf * 10.0).min(1.0);

        // Head eases toward its target.
        let k = (dtf * 6.0).min(1.0);
        self.head_yaw += (self.head_target.0 - self.head_yaw) * k;
        self.head_pitch += (self.head_target.1 - self.head_pitch) * k;

        let (health, provoked) = (self.health, self.provoked);
        let alive = self.alive();
        self.burn(dtf, world, ctx, rng);
        if alive && !self.alive() {
            let player_kill = self.player_hit_left > 0.0;
            events.push(EntityEvent::MobKilled {
                kind: self.kind,
                pos: self.pos,
                burning: self.fire_left > 0.0,
                player_kill,
                looting: 0, // Environmental damage has no attacking entity.
            });
        }
        // Endermen hurt by anything but a mob or player usually teleport,
        // and only get angry at attackers.
        if self.kind == MobKind::Enderman {
            self.provoked = provoked;
            if self.health < health && rng.chance(0.9) {
                self.teleport_pending = true;
            }
        }
        if self.alive() && self.kind == MobKind::Villager && self.age != 0 {
            let v = self.villager.as_mut().unwrap();
            v.growth += dt as f32 * 20.0;
            let ticks = v.growth.floor() as i32;
            v.growth -= ticks as f32;
            self.age = if self.age < 0 { (self.age + ticks).min(0) } else { (self.age - ticks).max(0) };
            self.baby = self.age < 0;
        }
        self.advance_animal_age(dtf);
        if self.alive() && self.kind == MobKind::Chicken && self.age >= 0 {
            self.egg_timer -= dtf;
            if self.egg_timer <= 0.0 {
                self.egg_timer = rng.range(300.0, 600.0);
                events.push(EntityEvent::LaidEgg { pos: self.pos });
            }
        }
        // A lethal Thorns response is emitted during thinking, before the
        // knockback physics step. Loot resolves after this update and must
        // identify the corpse at its final position, rather than the old one.
        for event in &mut events[event_start..] {
            if let EntityEvent::MobKilled { pos, .. } = event {
                *pos = self.pos;
            }
        }
    }

    /// Picks a movement direction and speed for this tick.
    fn think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        if let Some(wish) = self.leash_think(dt, world, rng) {
            return wish;
        }
        if let Some(wish) = self.companion_think(dt, world, ctx, rng, events) {
            return wish;
        }
        if self.kind == MobKind::Strider {
            self.update_strider_cold(world);
        }
        if self.kind.is_aquatic() {
            // Aquatic steering has no panic run of its own; the hit is still
            // forgotten after `PANIC_MEMORY`, so the goals resume.
            if self.ai == Ai::Panic {
                self.ai_timer -= dt;
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = 0.0;
                }
            }
            // An axolotl playing dead (`PLAY_DEAD`, its top activity) does
            // nothing else until the timer runs out.
            if self.aquatic.as_ref().is_some_and(|a| a.play_dead > 0.0) {
                self.stop_goal(world);
                return super::aquatic::think(self, dt, world, ctx, rng, events);
            }
        }
        if let Some(wish) = self.run_goals(world, ctx, rng) {
            return wish;
        }
        if self.kind.is_aquatic() {
            return super::aquatic::think(self, dt, world, ctx, rng, events);
        }
        if self.kind == MobKind::Pillager {
            return super::aquatic::pillager(self, dt, world, ctx, events);
        }
        if matches!(self.kind, MobKind::Villager | MobKind::WanderingTrader)
            && let Some(v) = &self.villager
        {
            if v.sleeping || v.trading && !v.fleeing {
                return (None, 0.0);
            }
            if let Some(goal) = v.goal {
                let speed = if v.fleeing { 4.0 } else { 2.0 };
                let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
                if flat.length_squared() < 1.0 {
                    self.stop_navigation();
                    return (None, 0.0);
                }
                // `MoveToTargetSink`: plan when the walk target changes or the
                // last path is used up, and walk it. A path that falls short
                // (`CANT_REACH_WALK_TARGET_SINCE`) or none at all waits a
                // second before planning again, so an unreachable bed or job
                // site isn't searched every tick.
                let cell = goal.floor().as_ivec3();
                if self.nav.is_done() || self.nav.target() != Some(path::surface_target(world, cell)) {
                    self.repath_in -= 1;
                    if self.repath_in <= 0 {
                        let reaches = self.walk_to(world, cell, 0, speed, false)
                            && self.nav.path.as_ref().is_some_and(|p| p.reached);
                        self.repath_in = if reaches { 0 } else { 20 };
                    }
                }
                // `InteractWithDoor`: open the closed wooden doors on the
                // previous and next nodes of the path.
                if let Some(p) = &self.nav.path {
                    for node in [p.previous_node(), p.next_node()].into_iter().flatten() {
                        if node.ty == path::PathType::WalkableDoor || node.ty == path::PathType::DoorWoodClosed {
                            for dy in 0..2 {
                                let cell = node.pos + IVec3::Y * dy;
                                if world.block(cell).is_some_and(|b| {
                                    matches!(
                                        b.shaped(),
                                        Some(crate::world::block::Shaped::Door { open: false, upper: false, .. })
                                    )
                                }) {
                                    events.push(EntityEvent::VillagerDoor { cell });
                                }
                            }
                        }
                    }
                }
                return (self.follow_path(world, speed, false), speed);
            }
        }
        if self.trader.as_ref().is_some_and(|t| t.drink > 0.0) {
            return (None, 0.0);
        }
        if let Some(goal) = self.hunt {
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            let dist = flat.length();
            if dist > 1e-4 {
                let dir = flat / dist;
                let reach = if self.kind == MobKind::IronGolem { 2.4 } else { 1.5 };
                self.head_target = (wrap((dir.z as f32).atan2(dir.x as f32) - self.yaw).clamp(-1.2, 1.2), 0.0);
                let golem = self.kind == MobKind::IronGolem;
                if dist <= reach && self.attack_cooldown <= 0.0 && (golem || self.kind.is_zombie()) {
                    self.attack_cooldown = if golem { 1.2 } else { ATTACK_COOLDOWN };
                    self.attack_anim = 0.4;
                    self.strike = true;
                }
                if dist <= 0.9 {
                    return (None, 0.0);
                }
                let speed = if self.kind == MobKind::IronGolem { 1.5 } else { self.kind.chase_speed() };
                let sees = line_of_sight(world, self.pos + DVec3::Y * (self.shape().height * 0.85), goal + DVec3::Y);
                return (self.pursue(world, goal, speed, sees, true, rng), speed);
            }
        }
        if self.kind == MobKind::Witch && self.witch_upkeep(dt, world, rng) {
            return (None, 0.0);
        }
        if self.nether.is_some()
            && let Some(wish) = self.nether_think(dt, world, ctx, rng, events)
        {
            return wish;
        }
        self.ai_timer -= dt;
        let target = ctx.nearest_target(self.pos);
        let to_player = target.map_or(DVec3::ZERO, |t| t.pos - self.pos);
        let flat = DVec3::new(to_player.x, 0.0, to_player.z);
        let hdist = flat.length();

        if self.kind.is_hostile() && self.nether.is_none() {
            let aggressive = match self.kind {
                MobKind::Spider | MobKind::CaveSpider => ctx.daylight < SPIDER_CALM_DAYLIGHT || self.provoked > 0.0,
                MobKind::ZombifiedPiglin => self.provoked > 0.0,
                MobKind::Enderman => self.enderman_anger(dt, world, ctx, rng, events),
                _ => true,
            };
            let (range, height) = match self.kind {
                MobKind::Blaze => (BLAZE_RANGE, 24.0),
                MobKind::Ghast => (GHAST_RANGE, GHAST_RANGE),
                MobKind::CaveSpider | MobKind::Slime | MobKind::MagmaCube => (16.0, 4.0),
                _ => (CHASE_RANGE, 12.0),
            };
            let chasing = aggressive && target.is_some() && hdist < range && to_player.y.abs() < height;
            if chasing {
                self.ai = Ai::Chase;
            } else if self.ai == Ai::Chase {
                self.ai = Ai::Idle;
                self.ai_timer = 1.0;
                self.stop_navigation();
            }
        }
        if self.ai != Ai::Chase {
            self.fuse = (self.fuse - dt).max(0.0);
            self.lift = false;
            self.charged = false;
            self.attack_step = 0;
        }

        match self.ai {
            Ai::Chase => {
                let Some(&target) = target else { return (None, 0.0) };
                // Look at the player's face.
                let eye = to_player.y + 1.62 - self.shape().height * 0.9;
                let face_yaw = (to_player.z as f32).atan2(to_player.x as f32);
                self.head_target = (wrap(face_yaw - self.yaw).clamp(-1.2, 1.2), (eye as f32).atan2(hdist as f32));
                let dir = if hdist > 1e-6 { flat / hdist } else { DVec3::X };
                if self.kind == MobKind::Enderman && self.enderman_tactics(dt, world, &target, rng, events) {
                    return (None, 0.0);
                }
                match self.kind {
                    MobKind::Skeleton => {
                        return self.skeleton_tactics(dt, world, target.pos, dir, hdist, rng, events);
                    }
                    MobKind::Blaze => {
                        if let Some(stop) = self.blaze_tactics(dt, world, &target, rng, events) {
                            return stop;
                        }
                    }
                    MobKind::Witch => return self.witch_tactics(world, &target, dir, hdist, rng, events),
                    MobKind::Ghast => {
                        return self.ghast_tactics(world, target.pos, dir, hdist, events);
                    }
                    MobKind::Creeper => {
                        if let Some(stop) = self.creeper_fuse(dt, hdist, events) {
                            return stop;
                        }
                    }
                    _ => {
                        let reach = if self.kind.is_cube() { 0.6 * self.size as f64 } else { ATTACK_RANGE };
                        let can_hurt = self.kind != MobKind::Slime || self.size > 1;
                        if can_hurt && hdist <= reach && to_player.y.abs() < 1.6 && self.attack_cooldown <= 0.0 {
                            self.attack_cooldown = ATTACK_COOLDOWN;
                            self.attack_anim = 0.35;
                            let knockback = dir * 6.0 + DVec3::Y * 5.0;
                            let (damage, cause) = if self.kind.is_cube() {
                                (
                                    self.size as f32 + if self.kind == MobKind::MagmaCube { 2.0 } else { 0.0 },
                                    if self.kind == MobKind::MagmaCube {
                                        "was slain by a magma cube"
                                    } else {
                                        "was slain by a slime"
                                    },
                                )
                            } else {
                                self.melee_attack()
                            };
                            // `Mob.doHurtTarget`: `hurtServer`'s push away from the mob, then
                            // `getKnockback` (the attack_knockback attribute over two) along its facing.
                            let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
                            let yaw = (-dir.x).atan2(dir.z).to_degrees() as f32;
                            events.push(EntityEvent::PlayerHit {
                                java: Some(crate::knockback::Hit::melee(self.pos, target.pos, yaw, extra)),
                                player: target.id,
                                damage,
                                knockback: knockback.as_vec3(),
                                cause,
                            });
                            events.push(EntityEvent::OwnerHurt { player: target.id, attacker: self.uid });
                            if self.kind == MobKind::Husk {
                                // `Husk.doHurtTarget`: 140 ticks times the whole part of the
                                // local effective difficulty.
                                let scale = self.local_difficulty as u32;
                                if scale > 0 {
                                    events.push(EntityEvent::PlayerEffect {
                                        player: target.id,
                                        effect: crate::simulation::effects::Effect::Hunger,
                                        amplifier: 0,
                                        ticks: 140 * scale,
                                    });
                                }
                            }
                            if self.kind == MobKind::WitherSkeleton {
                                events.push(EntityEvent::PlayerEffect {
                                    player: target.id,
                                    effect: crate::simulation::effects::Effect::Wither,
                                    amplifier: 0,
                                    ticks: 200,
                                });
                            }
                            if self.kind == MobKind::CaveSpider {
                                let ticks = match self.difficulty {
                                    crate::simulation::difficulty::Difficulty::Normal => 140,
                                    crate::simulation::difficulty::Difficulty::Hard => 300,
                                    _ => 0,
                                };
                                if ticks > 0 {
                                    events.push(EntityEvent::PlayerEffect {
                                        player: target.id,
                                        effect: crate::simulation::effects::Effect::Poison,
                                        amplifier: 0,
                                        ticks,
                                    });
                                }
                            }
                            self.thorns_response(&target, knockback, rng, events);
                        }
                    }
                }
                // Stop just short so we don't stand inside the player.
                if hdist <= 0.8 {
                    return (None, 0.0);
                }
                // Java's `MeleeAttackGoal`: walk a path to the target, planned
                // again as it moves. Only spiders follow unseen targets.
                let speed = self.kind.chase_speed() * if self.baby { 1.5 } else { 1.0 };
                let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
                let sees = matches!(self.kind, MobKind::Spider | MobKind::CaveSpider)
                    || line_of_sight(world, eye, target.pos + DVec3::Y * 1.62);
                (self.pursue(world, target.pos, speed, sees, true, rng), speed)
            }
            Ai::Panic => {
                let pathing = self.path_move && self.kind.walks_paths();
                if self.ai_timer <= 0.0 && (!pathing || self.nav.is_done()) {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(1.0, 3.0);
                    self.path_move = false;
                    self.stop_navigation();
                    return (None, 0.0);
                }
                let speed = self.panic_speed();
                if pathing {
                    if self.ai_timer > 0.0 {
                        self.panic_target(world, ctx, rng, speed);
                    }
                    if let Some(dir) = self.follow_path(world, speed, false) {
                        self.head_target = (0.0, 0.0);
                        return (Some(dir), speed);
                    }
                }
                if rng.chance(dt * 1.2) || self.blocked && rng.chance(dt * 4.0) {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), speed)
            }
            Ai::Wander if self.path_move => {
                self.head_target = (0.0, 0.0);
                let speed = self.stroll_speed();
                match self.follow_path(world, speed, false) {
                    Some(dir) if self.ai_timer > 0.0 => (Some(dir), speed),
                    _ => {
                        self.ai = Ai::Idle;
                        self.ai_timer = rng.range(2.0, 6.0);
                        self.path_move = false;
                        self.stop_navigation();
                        (None, 0.0)
                    }
                }
            }
            Ai::Wander => {
                // Turn away from walls we can't hop over.
                if self.blocked && self.on_ground && !self.in_water && !self.can_step_up(world, yaw_dir(self.move_yaw))
                {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(2.0, 6.0);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), self.kind.wander_speed())
            }
            Ai::Idle => {
                // Occasionally look around.
                self.head_timer -= dt;
                if self.head_timer <= 0.0 {
                    self.head_timer = rng.range(1.0, 3.5);
                    self.head_target =
                        if rng.chance(0.6) { (rng.range(-1.1, 1.1), rng.range(-0.35, 0.3)) } else { (0.0, 0.0) };
                }
                if self.kind.walks_paths() {
                    // `RandomStrollGoal`: a 1-in-`interval` chance a tick (1 in
                    // half the interval per goal update, every other tick).
                    if rng.chance(dt * 20.0 / self.kind.stroll_interval()) && self.try_stroll(world, ctx, rng) {
                        self.path_move = true;
                    }
                } else if self.ai_timer <= 0.0 {
                    if rng.chance(0.7) {
                        self.ai = Ai::Wander;
                        self.ai_timer = rng.range(2.0, 5.0);
                        self.move_yaw = rng.range(0.0, TAU);
                    } else {
                        self.ai_timer = rng.range(2.0, 5.0);
                    }
                }
                (None, 0.0)
            }
        }
    }

    /// Java's blaze attack: rise to hover a little above the target, melee
    /// when within 2 blocks, otherwise (when it can see the target) charge
    /// for 3 s and fire three fireballs 0.3 s apart, then rest 5 s. Returns
    /// the movement to use instead of closing in, if any.
    fn blaze_tactics<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        target: &super::Target,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        self.offset_timer -= dt;
        if self.offset_timer <= 0.0 {
            self.offset_timer = 5.0;
            self.height_offset = 0.5 + rng.range(-1.0, 1.0) * 3.0;
        }
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let target_eye = target.pos + DVec3::Y * crate::player::EYE_HEIGHT;
        self.lift = target_eye.y > eye.y + self.height_offset as f64;
        let to = target_eye - eye;
        let dist = to.length();
        if dist < BLAZE_MELEE {
            // Close enough to hit: the usual melee code handles it.
            return None;
        }
        if !line_of_sight(world, eye, target_eye) {
            return None;
        }
        if self.attack_cooldown <= 0.0 {
            self.attack_step += 1;
            match self.attack_step {
                1 => {
                    self.attack_cooldown = BLAZE_CHARGE;
                    self.charged = true;
                }
                2..=4 => {
                    self.attack_cooldown = BLAZE_VOLLEY;
                    // Java's spread grows with the square root of distance.
                    let spread = dist.sqrt() * 0.5 * 0.1;
                    let mut r = || rng.range(-1.0, 1.0) as f64 * spread;
                    let dir = (to / dist + DVec3::new(r(), 0.0, r())).normalize();
                    events.push(EntityEvent::Fireball { from: eye + dir * 0.5, dir, large: false });
                    events.push(EntityEvent::Sound { sound: MobSound::Fireball, pos: eye });
                }
                _ => {
                    self.attack_cooldown = BLAZE_REST;
                    self.attack_step = 0;
                    self.charged = false;
                }
            }
        }
        Some((None, 0.0))
    }

    /// Java's ghast: hover out of reach, charge for a second while it can
    /// see the target, then shoot one explosive fireball.
    fn ghast_tactics<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        player: DVec3,
        dir: DVec3,
        hdist: f64,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.5);
        let target_eye = player + DVec3::Y * crate::player::EYE_HEIGHT;
        let to = target_eye - eye;
        let dist = to.length().max(1e-6);
        let see = dist < GHAST_RANGE && line_of_sight(world, eye, target_eye);
        let flat = if hdist > 28.0 {
            dir
        } else if hdist < 14.0 {
            -dir
        } else {
            DVec3::new(-dir.z, 0.0, dir.x)
        };
        let dy = (target_eye.y + 3.0 - eye.y).clamp(-8.0, 8.0);
        let wish = (flat * 8.0 + DVec3::Y * dy).normalize_or(DVec3::Y);
        if see {
            if self.attack_cooldown <= 0.0 {
                if !self.charged {
                    self.charged = true;
                    self.attack_cooldown = GHAST_CHARGE;
                    events.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::Ghast), pos: eye });
                } else {
                    self.charged = false;
                    self.attack_cooldown = GHAST_REST;
                    let shot = to / dist;
                    let from = eye + shot * (self.shape().half_width.max(self.shape().height * 0.5) * 2.2);
                    events.push(EntityEvent::Fireball { from, dir: shot, large: true });
                    events.push(EntityEvent::Sound { sound: MobSound::Fireball, pos: eye });
                }
            }
        } else {
            self.charged = false;
        }
        (Some(wish), GHAST_SPEED)
    }

    /// Where an enderman's eyes are.
    fn eye(&self) -> DVec3 {
        self.pos + DVec3::Y * ENDERMAN_EYE
    }

    /// Java's `isLookingAtMe`: `target` looks within a narrowing cone of
    /// this enderman's eyes, with nothing in between.
    fn watched_by<W: MobWorld + ?Sized>(&self, world: &W, target: &super::Target) -> bool {
        let from = target.pos + DVec3::Y * crate::player::EYE_HEIGHT;
        let to = self.eye() - from;
        let d = to.length();
        target.look != DVec3::ZERO
            && d > 1e-3
            && d < STARE_RANGE
            && target.look.dot(to / d) > 1.0 - 0.025 / d
            && line_of_sight(world, from, self.eye())
    }

    /// Updates an enderman's anger: a long enough stare from a targetable
    /// player sets it off with a scream; a near target keeps it going; and
    /// in daylight under the open sky a calm one wanders off by teleporting.
    /// Returns whether it is angry.
    fn enderman_anger<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.since_anger += dt;
        let staring = ctx.players.iter().any(|t| t.targetable && self.watched_by(world, t));
        self.stare = if staring { self.stare + dt } else { 0.0 };
        if self.stare >= STARE_TIME && self.provoked <= 0.0 {
            self.provoked = ENDERMAN_ANGER_TIME;
            self.since_anger = 0.0;
            events.push(EntityEvent::Sound { sound: MobSound::Scream, pos: self.eye() });
        }
        if self.provoked > 0.0 && ctx.nearest_target(self.pos).is_some_and(|t| t.pos.distance(self.pos) < 32.0) {
            self.provoked = self.provoked.max(5.0);
        }
        let head = self.eye().floor().as_ivec3();
        if ctx.daylight > 0.5 && self.since_anger > 30.0 && world.exposed(head) {
            // Java: random * 30 < (brightness - 0.4) * 2, each tick.
            if rng.chance(dt * 20.0 * (ctx.daylight - 0.4) * 2.0 / 30.0) {
                self.provoked = 0.0;
                self.teleport_pending = true;
            }
        }
        self.provoked > 0.0
    }

    /// An angry enderman freezes while its target watches it (and flees a
    /// stare up close), and teleports toward a target that keeps its
    /// distance. Returns whether it should stand still this tick.
    fn enderman_tactics<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        target: &super::Target,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        let dist = target.pos.distance(self.pos);
        if self.watched_by(world, target) {
            self.teleport_timer = 0.0;
            if dist < FLEE_DIST {
                self.teleport(world, rng, None, events);
            }
            return dist < FREEZE_DIST;
        }
        if dist > FREEZE_DIST {
            self.teleport_timer += dt;
            if self.teleport_timer >= 1.5 {
                self.teleport_timer = 0.0;
                self.teleport(world, rng, Some(target.pos), events);
            }
        }
        false
    }

    /// Java's enderman teleport: a random spot up to 32 blocks away (or,
    /// `toward` a target, near the side of it facing this enderman), dropped
    /// onto the ground below, dry and with room to stand. Tries a few spots.
    pub fn teleport<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        rng: &mut Rng,
        toward: Option<DVec3>,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        let shape = self.shape();
        for _ in 0..16 {
            let mut r = || rng.range(-0.5, 0.5) as f64;
            let goal = match toward {
                Some(t) => {
                    let back = (self.pos - t).normalize_or(DVec3::X);
                    self.pos + DVec3::new(r() * 8.0, r() * 16.0, r() * 8.0) - back * 16.0
                }
                None => self.pos + DVec3::new(r() * 64.0, r() * 64.0, r() * 64.0),
            };
            let (x, z) = (goal.x.floor() as i32, goal.z.floor() as i32);
            let Some(ground) = (goal.y.floor() as i32 - 32..=goal.y.floor() as i32).rev().find(|&y| {
                world.block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid())
                    && !world.block(IVec3::new(x, y + 1, z)).is_some_and(|b| b.is_solid())
            }) else {
                continue;
            };
            let to = DVec3::new(x as f64 + 0.5, ground as f64 + 1.0, z as f64 + 0.5);
            let loaded = world.loaded(to.floor().as_ivec3());
            let dry = !physics::touches_block(world, to, shape, |b| b.holds_water() || b.is_lava());
            if loaded && dry && !physics::overlaps_solid(world, to, shape) {
                events.push(EntityEvent::Sound { sound: MobSound::Teleport, pos: self.pos + DVec3::Y });
                events.push(EntityEvent::Sound { sound: MobSound::Teleport, pos: to + DVec3::Y });
                self.pos = to;
                self.previous_pos = to;
                self.vel = DVec3::ZERO;
                return true;
            }
        }
        false
    }

    /// Skeletons keep their distance and shoot when they can see the
    /// player whose feet are at `player`.
    #[allow(clippy::too_many_arguments)]
    fn skeleton_tactics<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        player: DVec3,
        dir: DVec3,
        hdist: f64,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.9);
        let target = player + DVec3::Y * 1.2;
        if self.attack_cooldown <= 0.0 && hdist < SHOOT_RANGE && line_of_sight(world, eye, target) {
            self.attack_cooldown = rng.range(1.6, 2.4);
            self.attack_anim = 0.35;
            events.push(EntityEvent::Shoot {
                from: eye + dir * 0.5,
                target,
                cause: "was shot by a skeleton",
                shooter: self.uid,
            });
            events.push(EntityEvent::Sound { sound: MobSound::Bow, pos: eye });
        }
        if rng.chance(dt * 0.4) {
            self.detour_side = -self.detour_side;
        }
        let side = DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side;
        if hdist > SKELETON_FAR {
            let speed = self.kind.chase_speed();
            let sees = line_of_sight(world, eye, target);
            (self.pursue(world, player, speed, sees, true, rng), speed)
        } else if hdist < SKELETON_NEAR {
            (Some((-dir + side * 0.3).normalize()), 2.0)
        } else {
            (Some(side), 1.0)
        }
    }

    /// Java's witch `aiStep`: each tick it may start drinking Water Breathing
    /// (15%, eye in water), Fire Resistance (15%, burning) or Healing (5%,
    /// hurt), and its drink (1.6 s, standing still) applies the potion's
    /// effects to itself. Returns whether it is mid-drink.
    fn witch_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, rng: &mut Rng) -> bool {
        use crate::simulation::effects::Effect;
        if self.drink_left > 0.0 {
            self.drink_left -= dt;
            if self.drink_left <= 0.0
                && let Some(potion) = crate::potion::Potion::from_id(self.drinking)
            {
                for (effect, amplifier, ticks) in potion.effects() {
                    let outcome = self.effects.add(effect, amplifier, ticks);
                    self.health = (self.health + outcome.heal).min(self.kind.max_health());
                }
            }
            return self.drink_left > 0.0;
        }
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let eye_in_water = world.block(eye.floor().as_ivec3()).is_some_and(Block::holds_water);
        let tick_chance = |rng: &mut Rng, p: f32| rng.chance(1.0 - (1.0 - p).powf(dt * 20.0));
        let drink = if eye_in_water && !self.effects.has(Effect::WaterBreathing) && tick_chance(rng, 0.15) {
            "water_breathing"
        } else if self.burning && !self.effects.has(Effect::FireResistance) && tick_chance(rng, 0.15) {
            "fire_resistance"
        } else if self.health < self.kind.max_health() && tick_chance(rng, 0.05) {
            "healing"
        } else {
            return false;
        };
        self.drink_left = 1.6;
        self.drinking = drink;
        true
    }

    /// Java's witch `performRangedAttack`: a splash potion every three
    /// seconds from up to ten blocks away, chosen from the target's state
    /// (Slowness from 8+ blocks, then Poison at 8+ health, Weakness 25% of
    /// the time within 3 blocks, otherwise Harming). It drinks Swiftness to
    /// close a gap of more than 11 blocks, and stands still while it has a
    /// clear throw.
    fn witch_tactics<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        target: &super::Target,
        dir: DVec3,
        hdist: f64,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        use crate::simulation::effects::Effect;
        let player = target.pos;
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.9);
        let aim_at = player + DVec3::Y * 1.2;
        let sees = line_of_sight(world, eye, aim_at);
        if !self.effects.has(Effect::Speed) && (player - self.pos).length_squared() > 121.0 && rng.chance(0.5) {
            self.drink_left = 1.6;
            self.drinking = "swiftness";
            return (None, 0.0);
        }
        if hdist <= WITCH_RANGE && sees {
            if self.attack_cooldown <= 0.0 {
                let id = if hdist >= 8.0 && !target.has_effect(Effect::Slowness) {
                    "slowness"
                } else if target.health >= 8.0 && !target.has_effect(Effect::Poison) {
                    "poison"
                } else if hdist <= 3.0 && !target.has_effect(Effect::Weakness) && rng.chance(0.25) {
                    "weakness"
                } else {
                    "harming"
                };
                let flat = DVec3::new(aim_at.x - eye.x, 0.0, aim_at.z - eye.z);
                let aim = DVec3::new(flat.x, aim_at.y - 1.1 - eye.y + flat.length() * 0.2, flat.z).normalize_or(dir);
                if let Some(potion) = crate::potion::Potion::from_id(id) {
                    events.push(EntityEvent::ThrowPotion { from: eye + dir * 0.4, vel: aim * WITCH_SPEED, potion });
                }
                self.attack_cooldown = 3.0;
                self.attack_anim = 0.35;
            }
            return (None, 0.0);
        }
        (Some(dir), self.kind.chase_speed())
    }

    /// Creepers stop and hiss when close; returns the movement to use
    /// while fusing (or `None` to keep approaching).
    fn creeper_fuse(&mut self, dt: f32, hdist: f64, events: &mut Vec<EntityEvent>) -> Option<(Option<DVec3>, f64)> {
        let fusing = if self.fuse > 0.0 { hdist < FUSE_KEEP } else { hdist < FUSE_START };
        if !fusing {
            self.fuse = (self.fuse - dt).max(0.0);
            return None;
        }
        if self.fuse == 0.0 {
            events.push(EntityEvent::Sound { sound: MobSound::Fuse, pos: self.pos });
        }
        self.fuse += dt;
        if self.fuse >= FUSE_TIME {
            let center = self.pos + DVec3::Y * (self.shape().height * 0.5);
            events.push(EntityEvent::Explosion {
                center,
                power: CREEPER_POWER,
                cause: "was blown up by a creeper",
                credit_player: false,
            });
            // Gone in the blast: no death animation, no loot.
            self.health = 0.0;
            self.dying = Some(DEATH_TIME);
        }
        Some((None, 0.0))
    }

    pub(super) fn physics_step<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        wish: Option<DVec3>,
        speed: f64,
    ) {
        let effect_speed = self.effects.speed_factor();
        let speed = speed * effect_speed;
        if self.kind == MobKind::Strider && self.walk_on_lava(dt, world, wish, speed) {
            return;
        }
        let shape = self.shape();
        self.in_water = if self.kind.is_aquatic() {
            world.block((self.pos + DVec3::Y * 0.2).floor().as_ivec3()).is_some_and(Block::holds_water)
        } else {
            physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.3, 0.0))
        };
        if self.kind == MobKind::SnowGolem && self.alive() && self.in_water {
            // Java: one damage each tick while in water. Death loot is emitted
            // by the caller once health is gone.
            self.health -= self.effects.absorb(20.0 * dt as f32 * self.effects.resistance_factor());
        }
        let hopping = self.kind.is_cube();
        let speed = if hopping { (0.2 + 0.1 * self.size as f64) * 10.0 * effect_speed } else { speed };
        let wish = if hopping && self.on_ground && self.hop_left > 0.0 { None } else { wish };
        if hopping && self.on_ground && self.alive() && self.hop_left <= 0.0 {
            self.vel.y = 8.4 + if self.kind == MobKind::MagmaCube { 2.0 * self.size as f64 } else { 0.0 };
            self.hop_left = self.hop_delay;
            if self.kind == MobKind::MagmaCube {
                self.hop_left *= 4.0;
            }
            if self.ai == Ai::Chase {
                self.hop_left /= 3.0;
            }
        }
        if self.kind == MobKind::Rabbit && self.on_ground && wish.is_some() && self.hop_left <= 0.0 {
            self.vel.y = if self.blocked { 8.4 } else { 4.6 };
            self.hop_left = 0.5;
        }
        let target = wish.map_or(DVec3::ZERO, |d| d * speed);
        // Java's `Drowned.wantsToSwim` (a target) and `Turtle.travel` (steering):
        // otherwise they sink or float like other mobs in water below.
        let amphibious = matches!(self.kind, MobKind::Drowned | MobKind::Turtle);
        // Corpses take the ordinary water sink.
        let swims = match self.kind {
            MobKind::Drowned => self.alive() && (self.ai == Ai::Chase || self.hunt.is_some()),
            MobKind::Turtle => self.alive() && wish.is_some(),
            _ => self.kind.is_aquatic(),
        };
        if swims && self.in_water {
            self.vel += (target - self.vel) * (dt * 3.0).min(1.0);
            let delta = self.vel * dt;
            let hit = physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape);
            self.on_ground = hit.on_ground;
            self.blocked = hit.horizontal;
            // `travelInWater`: lift onto a bank only when the way ahead is free
            // 0.6 higher, so underwater walls don't launch them.
            if amphibious
                && self.blocked
                && let Some(w) = wish
            {
                let ahead = self.pos + DVec3::new(w.x, 0.0, w.z).normalize_or_zero() * 0.3 + DVec3::Y * 0.6;
                if !physics::overlaps_solid(world, ahead, shape) {
                    self.vel.y = self.vel.y.max(4.5);
                }
            }
            return;
        }
        if self.kind.is_aquatic() && self.kind != MobKind::Axolotl && self.on_ground && self.alive() {
            self.vel.y = 4.0;
            self.vel.x = self.yaw.cos() as f64 * 1.4;
            self.vel.z = self.yaw.sin() as f64 * 1.4;
        }

        if self.alive() && (self.kind == MobKind::Ghast || self.kind == MobKind::Parrot && wish.is_some()) {
            // Java ghasts fly: no gravity, and they steer in three dimensions.
            let k = (dt * 1.6).min(1.0);
            self.vel += (target - self.vel) * k;
        } else if self.in_water {
            let k = (dt * 4.0).min(1.0);
            self.vel.x += (target.x * 0.6 - self.vel.x) * k;
            self.vel.z += (target.z * 0.6 - self.vel.z) * k;
            if self.kind.floats() {
                // Buoyant below ~0.6 blocks of depth, so swimmers bob at the
                // surface (Java's `FloatGoal` and brain `Swim` jumps).
                if physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.6, 0.0)) {
                    self.vel.y = (self.vel.y + 22.0 * dt).min(2.0);
                } else {
                    self.vel.y -= GRAVITY * 0.25 * dt;
                }
                self.vel.y = self.vel.y.max(-4.0);
            } else {
                // Java `travelInWater` without a float goal: gravity / 16 a
                // tick against 0.8 drag sinks at most 0.02 blocks a tick, so
                // zombies, skeletons, piglins and golems walk the bottom.
                self.vel.y += (-0.4 - self.vel.y) * (dt * 4.0).min(1.0);
            }
            // Climb out onto the shore.
            if self.blocked && wish.is_some() {
                self.vel.y = self.vel.y.max(4.5);
            }
        } else {
            // Knockback isn't cancelled instantly: little control in the air.
            let k = (dt * if self.on_ground { 10.0 } else { 1.5 }).min(1.0);
            if self.on_ground || wish.is_some() {
                self.vel.x += (target.x - self.vel.x) * k;
                self.vel.z += (target.z - self.vel.z) * k;
            }
            self.vel.y = if super::mounts::equine(self.kind) {
                // Java: gravity 0.08 blocks/tick and vertical drag 0.98.
                ((self.vel.y - 32.0 * dt) * 0.98f64.powf(dt * 20.)).max(-78.)
            } else {
                (self.vel.y - GRAVITY * dt).max(-78.)
            };
            if matches!(self.kind, MobKind::Chicken | MobKind::Parrot) {
                self.vel.y = self.vel.y.max(-2.5); // flaps its way down
            }
            if self.kind == MobKind::Blaze && self.alive() {
                // Java blazes sink slowly and rise toward a target above.
                self.vel.y = self.vel.y.max(-2.3);
                if self.lift {
                    self.vel.y += (6.0 - self.vel.y) * (1.0 - 0.7f64.powf(dt * 20.0));
                }
            }
            if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider)
                && self.blocked
                && wish.is_some()
                && self.alive()
            {
                self.vel.y = self.vel.y.max(3.0); // climbs walls
            } else if self.alive()
                && !matches!(self.kind, MobKind::Ghast | MobKind::Blaze | MobKind::Parrot)
                && world.block(self.pos.floor().as_ivec3()).is_some_and(Block::is_climbable)
            {
                // Java `LivingEntity.handleOnClimbable`: a mob inside a ladder or vine slides at
                // most 0.15 blocks per tick and climbs at 0.2 while pushing against a wall.
                self.vel.y = self.vel.y.max(-3.0);
                if self.blocked && wish.is_some() {
                    self.vel.y = 4.0;
                }
            } else if let Some(dir) = wish
                && self.on_ground
                && self.blocked
                && !super::mounts::equine(self.kind)
                && !self.mount.as_ref().is_some_and(|s| s.controlled())
                && self.can_step_up(world, dir)
            {
                self.vel.y = JUMP_VELOCITY;
            }
        }
        if self.dying.is_some() && self.on_ground {
            // Corpses slide to a stop.
            self.vel.x *= 1.0 - (dt * 8.0).min(1.0);
            self.vel.z *= 1.0 - (dt * 8.0).min(1.0);
        }

        let delta = self.vel * dt;
        let from_y = self.pos.y;
        let step_height = if self.kind == MobKind::Camel {
            1.5
        } else if super::mounts::equine(self.kind) || self.mount.as_ref().is_some_and(|s| s.controlled()) {
            1.
        } else {
            crate::player::STEP_HEIGHT
        };
        let hit = if self.on_ground {
            physics::move_box_stepping(world, &mut self.pos, &mut self.vel, delta, shape, step_height)
        } else {
            physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape)
        };
        if let Some(s) = &mut self.mount {
            if self.in_water {
                s.fall_distance = 0.;
            } else {
                s.fall_distance += (from_y - self.pos.y).max(0.);
            }
            if hit.on_ground {
                let damage = if super::mounts::equine(self.kind) {
                    ((s.fall_distance - 6.) * 0.5).ceil().max(0.) as f32
                } else if self.kind == MobKind::Strider {
                    0.
                } else {
                    (s.fall_distance - 3.).ceil().max(0.) as f32
                };
                self.health -= self.effects.absorb(damage * self.effects.resistance_factor());
                if super::mounts::equine(self.kind) && s.rider.is_some() {
                    s.rider_fall += damage;
                }
                s.fall_distance = 0.;
            }
        }
        self.on_ground = hit.on_ground;
        self.blocked = hit.horizontal;
    }

    /// Whether the obstacle ahead is a single-block ledge we can jump onto.
    fn can_step_up<W: BlockSource + ?Sized>(&self, world: &W, dir: DVec3) -> bool {
        let raised = self.pos + DVec3::new(0.0, 1.02, 0.0);
        !physics::overlaps_solid(world, raised, self.shape())
            && !physics::overlaps_solid(world, raised + dir * 0.4, self.shape())
    }

    /// Fire keeps burning after contact; water/rain extinguishes it. Nether
    /// piglins resist fire and lava. Undead also ignite in direct sunlight.
    fn burn<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx, rng: &mut Rng) {
        let head = (self.pos + DVec3::new(0.0, self.shape().height - 0.1, 0.0)).floor().as_ivec3();
        let in_lava = physics::touches_block(world, self.pos, self.shape(), Block::is_lava);
        let in_fire = physics::touches_block(world, self.pos, self.shape(), Block::is_fire);
        let sunburn = self.kind.burns_in_sun()
            && ctx.daylight > BURN_DAYLIGHT
            && !ctx.raining
            && !self.in_water
            && world.exposed(head);
        if self.kind.fire_immune() {
            self.fire_left = 0.0;
        } else if in_lava {
            self.fire_left = 15.0;
        } else if self.in_water || world.rains_on(head) {
            self.fire_left = 0.0;
        } else if sunburn || in_fire {
            self.fire_left = 8.0;
            if in_fire && !self.burning {
                self.damage(1.0, None, rng);
            }
        } else {
            self.fire_left = (self.fire_left - dt).max(0.0);
        }
        let wet = physics::touches_block(world, self.pos, self.shape(), Block::holds_water) || world.rains_on(head);
        if self.kind.hurt_by_water() && wet && self.hurt <= 0.0 {
            // Water hurts endermen and blazes (Java's 1 damage, spaced by hurt time).
            self.damage(1.0, None, rng);
        }
        self.burning = self.alive() && self.fire_left > 0.0;
        if self.kind == MobKind::Blaze {
            // A charging blaze is wreathed in flames.
            self.burning = self.alive() && self.charged;
            return;
        }
        if !self.burning {
            self.burn_timer = 0.0;
            return;
        }
        let (interval, amount) = if in_lava {
            (0.5, 4.0)
        } else if in_fire {
            (0.5, 1.0)
        } else {
            (1.0, if sunburn { 2.0 } else { 1.0 })
        };
        self.burn_timer += dt;
        if self.burn_timer >= interval {
            self.burn_timer -= interval;
            self.damage(amount, None, rng);
        }
    }

    /// Visual rotation of the whole body for the death animation (0..1).
    pub fn death_progress(&self) -> f32 {
        self.dying.map_or(0.0, |t| (t / (DEATH_TIME * 0.6)).min(1.0))
    }
}

/// Whether stepping in `dir` would drop more than [`MAX_SAFE_DROP`] blocks.
pub fn is_cliff<W: BlockSource + ?Sized>(world: &W, pos: DVec3, dir: DVec3, shape: Shape) -> bool {
    let probe = pos + dir * (shape.half_width + 0.35);
    let (x, z) = (probe.x.floor() as i32, probe.z.floor() as i32);
    let y = (pos.y + 0.01).floor() as i32;
    // A block at foot level ahead is a step up, not a drop.
    for dy in 0..=MAX_SAFE_DROP + 1 {
        match world.block(IVec3::new(x, y - dy, z)) {
            None => return true,
            Some(b) if b.is_solid() || b.holds_water() => return false,
            Some(_) => {}
        }
    }
    true
}

/// Sky light (0..1) at a point for lighting entities, items and particles:
/// the authoritative propagated skylight of its cell (so caves, tunnels and
/// shafts light as the light engine says). Only where that light is not exact
/// yet (a partly loaded border, or a world without a light engine) does it
/// fall back to [`estimate_sky_light`].
pub fn sky_light<W: MobWorld + ?Sized>(world: &W, p: DVec3) -> f32 {
    let cell = p.floor().as_ivec3();
    match world.sky_light_level(cell) {
        Some(level) => f32::from(level) / 15.0,
        None => estimate_sky_light(world, p),
    }
}

/// Rough sky light (0..1) at a point from the heightmap: 1 in the open,
/// dimmed under water or leaves, and falling off with distance from the
/// nearest open column under overhangs; 0 deep underground.
fn estimate_sky_light<W: MobWorld + ?Sized>(world: &W, p: DVec3) -> f32 {
    let cell = p.floor().as_ivec3();
    if world.exposed(cell) {
        return 1.0;
    }
    // Straight up through translucent blocks (water, leaves)?
    let mut level = 15i32;
    let mut y = cell.y + 1;
    let top = world.surface(cell.x, cell.z).unwrap_or(cell.y);
    while y <= top && level > 0 {
        match world.block(IVec3::new(cell.x, y, cell.z)) {
            Some(b) if b.is_opaque() => {
                level = 0;
                break;
            }
            Some(b) if b.light_opacity() > 0 => level -= 2,
            _ => {}
        }
        y += 1;
    }
    // Otherwise light spilling in from the side.
    for d in 1..=4i32 {
        let side = 15 - 2 * d;
        if side <= level {
            break;
        }
        let ring = (-d..=d).flat_map(|a| [(a, -d), (a, d), (-d, a), (d, a)]);
        if ring.into_iter().any(|(dx, dz)| world.exposed(cell + IVec3::new(dx, 0, dz))) {
            level = side;
            break;
        }
    }
    level.max(0) as f32 / 15.0
}

/// Java 26.3 LivingEntity.hasLineOfSight clips collision shapes with
/// Fluid.NONE, capped at 128 blocks. DDA visits each crossed cell once;
/// the old quarter-block samples missed thin/corner hits and filled slab gaps.
pub(super) fn line_of_sight<W: BlockSource + ?Sized>(world: &W, from: DVec3, to: DVec3) -> bool {
    if !from.is_finite() || !to.is_finite() || from.distance_squared(to) > 128.0 * 128.0 {
        return false;
    }
    let d = to - from;
    if d == DVec3::ZERO {
        return true;
    }
    let mut cell = from.floor().as_ivec3();
    let step = IVec3::from_array(d.to_array().map(|v| {
        if v > 0. {
            1
        } else if v < 0. {
            -1
        } else {
            0
        }
    }));
    let inv = DVec3::from_array(d.to_array().map(|v| if v == 0. { f64::INFINITY } else { 1. / v.abs() }));
    let frac = from - from.floor();
    let mut next = DVec3::from_array(std::array::from_fn(|axis| {
        if step[axis] == 0 {
            f64::INFINITY
        } else {
            (if step[axis] > 0 { 1. - frac[axis] } else { frac[axis] }) * inv[axis]
        }
    }));
    // <= 225 cell crossings for a 128-block segment, plus tied-plane cells.
    let bound = d.abs().ceil().element_sum() as usize + 4;
    for _ in 0..bound {
        let blocks = |p| world.block(p).is_some() && physics::ray_collision(world, p, from, d).is_some_and(|t| t < 1.0);
        if blocks(cell) {
            return false;
        }
        // Fence, wall and closed-gate collision extends into the row above.
        let below = cell - IVec3::Y;
        if world.block(below).is_some_and(|b| {
            matches!(
                b.shaped(),
                Some(
                    crate::world::block::Shaped::Fence
                        | crate::world::block::Shaped::Wall
                        | crate::world::block::Shaped::Gate { .. }
                )
            )
        }) && blocks(below)
        {
            return false;
        }
        let axis = if next.x < next.y && next.x < next.z {
            0
        } else if next.y < next.z {
            1
        } else {
            2
        };
        if next[axis] > 1.0 {
            return true;
        }
        cell[axis] += step[axis];
        next[axis] += inv[axis];
    }
    false
}

pub fn yaw_dir(yaw: f32) -> DVec3 {
    let (s, c) = yaw.sin_cos();
    DVec3::new(c as f64, 0.0, s as f64)
}

/// Angle wrapped to [-PI, PI).
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

fn turn_toward(from: f32, to: f32, max: f32) -> f32 {
    let d = wrap(to - from);
    wrap(from + d.clamp(-max, max))
}

#[cfg(test)]
mod skylight_tests {
    use super::*;
    use crate::world::block::Block;

    #[test]
    fn entities_and_particles_light_from_propagated_skylight() {
        // A wide roof: the cells beneath it keep only the light that spread
        // in from the open edge, which the old heightmap probe (a fixed
        // four-block search for an open column) could not see.
        let mut world = crate::world::lighting::flat_test_world(
            (-1..=1).flat_map(|x| (-1..=1).map(move |z| glam::IVec2::new(x, z))),
            63,
        );
        for x in 4..=28 {
            for z in 4..=28 {
                world.set_block(IVec3::new(x, 67, z), Block::STONE);
            }
        }
        world.update_block_light();
        let at = |x: f64, y: f64| DVec3::new(x, y, 16.5);
        assert_eq!(sky_light(&world, at(40.5, 66.5)), 1.0, "open sky");
        // Two cells in from the edge: 15 spreads in horizontally, losing one a step.
        let lip = world.sky_light(at(5.5, 66.5).floor().as_ivec3()).unwrap();
        assert!(lip < 15 && lip > 9, "{lip}");
        assert_eq!(sky_light(&world, at(5.5, 66.5)), f32::from(lip) / 15.0);
        // Thirteen steps from open air, light 2 has spread in at every height
        // under the roof; the old probe, which searched four columns, saw none.
        assert_eq!(world.sky_light(at(16.5, 64.5).floor().as_ivec3()), Some(2));
        assert_eq!(sky_light(&world, at(16.5, 64.5)), 2.0 / 15.0);
        assert_eq!(estimate_sky_light(&world, at(16.5, 64.5)), 0.0);
    }

    /// `cargo test --release --lib skylight_probe_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "timing measurement"]
    fn skylight_probe_cost() {
        let world = crate::world::lighting::flat_test_world(
            (-1..=1).flat_map(|x| (-1..=1).map(move |z| glam::IVec2::new(x, z))),
            63,
        );
        let points: Vec<DVec3> = (0..4096)
            .map(|i| DVec3::new(8.0 + (i % 64) as f64 * 0.25, 60.0 + (i / 64 % 16) as f64, 8.0 + (i / 1024) as f64))
            .collect();
        let time = |f: &dyn Fn(&crate::world::World, DVec3) -> f32| {
            let mut runs: Vec<f64> = (0..15)
                .map(|_| {
                    let start = std::time::Instant::now();
                    let mut sum = 0.0;
                    for _ in 0..25 {
                        for &p in &points {
                            sum += f(&world, p);
                        }
                    }
                    std::hint::black_box(sum);
                    start.elapsed().as_secs_f64() * 1e9 / (25.0 * points.len() as f64)
                })
                .collect();
            runs.sort_by(f64::total_cmp);
            runs[runs.len() / 2]
        };
        println!("heightmap estimate: {:.0} ns/query", time(&|w, p| estimate_sky_light(w, p)));
        println!("propagated skylight: {:.0} ns/query", time(&|w, p| sky_light(w, p)));
    }

    #[test]
    fn unknown_skylight_falls_back_to_the_heightmap_estimate() {
        // One lone column: its neighbours are unloaded, so skylight is unknown.
        let world = crate::world::lighting::flat_test_world([glam::IVec2::ZERO], 63);
        assert_eq!(world.sky_light(IVec3::new(16, 70, 16)), None);
        assert_eq!(sky_light(&world, DVec3::new(16.5, 70.5, 16.5)), 1.0);
    }
}

#[cfg(test)]
mod equipment_effect_tests {
    use super::*;
    use crate::{physics::test_util::Grid, simulation::effects::Effect};

    #[test]
    fn java_sight_clips_collision_shapes_and_ignores_fluids() {
        let mut world = Grid::flat(0);
        let p = IVec3::new(0, 2, 0);
        let ray = |y| (DVec3::new(-1., y, 0.5), DVec3::new(2., y, 0.5));
        world.set(p, Block::STONE_SLAB);
        let (from, to) = ray(2.75);
        assert!(line_of_sight(&world, from, to), "above the slab");
        let (from, to) = ray(2.25);
        assert!(!line_of_sight(&world, from, to), "through the slab");
        world.set(p, Block::gate(crate::world::block::Facing::South, true));
        assert!(line_of_sight(&world, from, to), "open gate has no collision");
        world.set(p, Block::GLASS);
        assert!(!line_of_sight(&world, from, to), "Java collider sight is blocked by glass");
        world.set(p, Block::WATER);
        assert!(line_of_sight(&world, from, to), "Fluid.NONE");
        world.set(p, Block::STONE);
        assert!(
            !line_of_sight(&world, DVec3::new(-0.04, 2.5, 0.92), DVec3::new(0.06, 2.5, 1.02)),
            "corner crossing missed by point sampling"
        );
        world.set(p, Block::OAK_FENCE);
        let (from, to) = ray(3.25);
        assert!(!line_of_sight(&world, from, to), "fence extends into the next cell");
        assert!(!line_of_sight(&world, DVec3::new(0., 4., 0.), DVec3::new(129., 4., 0.)));
        assert!(line_of_sight(&world, DVec3::new(0., 4., 0.), DVec3::new(128., 4., 0.)));
    }

    #[test]
    fn turtle_master_slowness_reaches_walking_and_hopping_physics() {
        let world = Grid::flat(10);
        for kind in [MobKind::Horse, MobKind::Zombie, MobKind::Slime] {
            let mut normal = Mob::new(kind, DVec3::new(0.5, 11., 0.5), 0.);
            let mut slowed = Mob::new(kind, normal.pos, 0.);
            slowed.effects.add(Effect::Slowness, 3, 400);
            normal.physics_step(0.05, &world, Some(DVec3::X), 3.);
            slowed.physics_step(0.05, &world, Some(DVec3::X), 3.);
            assert!(normal.vel.x > 0.);
            assert!((slowed.vel.x / normal.vel.x - 0.4).abs() < 1e-6, "{kind:?}");
        }
    }

    #[test]
    fn swimming_drowned_climb_low_banks_but_not_tall_underwater_walls() {
        // Water from y = 11 to 20 over a stone floor, a stone wall at x = 1.
        let mut world = Grid::flat(10);
        for y in 11..=20 {
            for z in -2..=2 {
                world.set(IVec3::new(0, y, z), Block::WATER);
                world.set(IVec3::new(1, y, z), Block::STONE);
            }
        }
        let mut m = Mob::new(MobKind::Drowned, DVec3::new(0.5, 13., 0.5), 0.);
        m.ai = Ai::Chase;
        for _ in 0..20 {
            m.physics_step(0.05, &world, Some(DVec3::X), 3.);
        }
        assert!(m.pos.y < 13.5, "a tall wall launched the drowned to {:?}", m.pos);
        // Lower the wall to a bank whose top is within 0.6 of its feet: now it climbs.
        for y in 13..=20 {
            for z in -2..=2 {
                world.set(IVec3::new(1, y, z), Block::WATER);
            }
        }
        m.pos.y = 12.6;
        for _ in 0..20 {
            m.physics_step(0.05, &world, Some(DVec3::X), 3.);
        }
        assert!(m.pos.x > 1.0, "the drowned never climbed the bank: {:?}", m.pos);
    }

    #[test]
    fn resistance_also_reduces_snow_golem_water_damage() {
        let mut world = Grid::flat(10);
        world.set(IVec3::new(0, 11, 0), Block::WATER);
        let mut m = Mob::new(MobKind::SnowGolem, DVec3::new(0.5, 11., 0.5), 0.);
        m.effects.add(Effect::Resistance, 2, 400);
        let before = m.health;
        m.physics_step(0.05, &world, None, 0.);
        assert!((before - m.health - 0.4).abs() < 1e-6);
    }
}
