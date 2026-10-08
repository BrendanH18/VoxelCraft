//! A single mob: AI, movement physics and combat state.

use std::f32::consts::{PI, TAU};

use glam::{DVec3, IVec3};

use crate::item::Item;
use crate::physics::{self, BlockSource, Shape};
use crate::world::block::Block;
use crate::world::terrain::Dimension;

use super::{Ctx, EntityEvent, MobSound, MobWorld, Rng};

mod nether_ai;

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
}

impl MobKind {
    /// Zombie, husk, drowned or zombie villager: shares the walk, armor and baby rolls.
    pub fn is_zombie(self) -> bool {
        matches!(self, Self::Zombie | Self::Husk | Self::Drowned | Self::ZombieVillager)
    }
    pub fn is_cube(self) -> bool {
        matches!(self, Self::Slime | Self::MagmaCube)
    }
    pub const ALL: [MobKind; 31] = [
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
        }
    }

    /// Looks a mob up by name (spaces or underscores).
    pub fn from_name(name: &str) -> Option<MobKind> {
        let name = name.replace('_', " ");
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
        )
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
            MobKind::PiglinBrute | MobKind::Zoglin => false,
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
            (MobKind::Husk, Biome::Desert) => 1.0,
            (MobKind::Husk, _) => 0.0,
            (MobKind::Zombie, Biome::Desert) => 0.2,
            (MobKind::Zombie, Biome::River | Biome::Ocean) => 0.2,
            (MobKind::Drowned, Biome::River | Biome::Ocean) => 1.0,
            (MobKind::Drowned, _) => 0.0,
            // Java's weight 5 against 100 for most monsters; swamp huts keep
            // swamps full of them.
            (MobKind::Witch, Biome::Swamp) => 0.25,
            (MobKind::Witch, _) => 0.05,
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
        const POPPY: Item = Item::from_block(Block::POPPY);
        match self {
            MobKind::Pig => &[(Item::RAW_PORKCHOP, 1, 3)],
            MobKind::Cow => &[(Item::RAW_BEEF, 1, 3), (Item::LEATHER, 0, 2)],
            MobKind::Sheep => &[(WOOL, 1, 1)],
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
            | MobKind::PiglinBrute => &[],
            MobKind::TraderLlama => &[(Item::LEATHER, 0, 2)],
            MobKind::IronGolem => &[(Item::IRON_INGOT, 3, 5), (POPPY, 0, 2)],
            MobKind::SnowGolem => &[(Item::SNOWBALL, 0, 15)],
            MobKind::Slime => &[(Item::SLIME_BALL, 0, 2)],
            MobKind::MagmaCube => &[(Item::MAGMA_CREAM, -2, 1)],
            MobKind::Ghast => &[(Item::GUNPOWDER, 0, 2), (Item::GHAST_TEAR, 0, 1)],
            // Babies drop nothing (`nether::babies_drop_nothing`).
            MobKind::Hoglin => &[(Item::RAW_PORKCHOP, 2, 4), (Item::LEATHER, 0, 1)],
            MobKind::Zoglin => &[(Item::ROTTEN_FLESH, 1, 3)],
            MobKind::Strider => &[(Item::STRING, 2, 5)],
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
                let extra = if looting > 0 && item != Item::from(Block::WOOL) {
                    (looting as f32 * rng.next_f32()).round() as i32
                } else {
                    0
                };
                let base = lo as i32 + (rng.next_f32() * span as f32) as i32;
                (item, (base.max(0) + extra).clamp(0, u8::MAX as i32) as u8)
            })
            .filter(|&(_, n)| n > 0)
            .collect();
        if self == MobKind::Zombie {
            // Java: 2.5% plus 1% per looting level, rolled separately.
            let chance = 0.025 + 0.01 * looting as f32;
            if rng.next_f32() < chance {
                out.push((Item::CARROT, 1));
            }
            if rng.next_f32() < chance {
                out.push((Item::POTATO, 1));
            }
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
            MobKind::Blaze => 10,
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
    pub kind: MobKind,
    pub villager: Option<Box<super::villager::Villager>>,
    /// Piglin gear, gold, anger and zombification (see `entity::nether`).
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
    pub wool_color: crate::color::DyeColor,
    pub sheared: bool,
    /// Witch: seconds left drinking, whether it's swiftness, swiftness left,
    /// and seconds before it may throw slowness or poison again.
    drink_left: f32,
    drinking_swift: bool,
    swift_left: f32,
    slow_cd: f32,
    poison_cd: f32,
    /// Negative while a baby. Java's chicks start at -24000 and grow one tick at a time.
    pub age: i32,
    /// Seconds until a grown chicken lays an egg (Java's 6000–12000 ticks).
    pub(crate) egg_timer: f32,
    hop_left: f32,
    hop_delay: f32,
    pub(super) difficulty: crate::simulation::difficulty::Difficulty,
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
    attack_cooldown: f32,
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
    pub trader: Option<Box<super::wandering_trader::Trader>>,
    pub(super) trader_night: bool,
    pub(super) convert_by: Option<super::PlayerId>,
    pub(super) angry_player: Option<super::PlayerId>,
}

impl Mob {
    /// Create a healthy, idle mob at `pos` with `yaw` in radians; snap its initial render position.
    pub fn new(kind: MobKind, pos: DVec3, yaw: f32) -> Self {
        Self {
            kind,
            villager: matches!(kind, MobKind::Villager | MobKind::ZombieVillager | MobKind::WanderingTrader)
                .then(|| Box::new(super::villager::Villager::new(0, 0))),
            nether: super::nether::NetherMob::for_kind(kind),
            uid: 0,
            persistent: false,
            size: 1,
            baby: false,
            wool_color: crate::color::DyeColor::White,
            sheared: false,
            drink_left: 0.0,
            drinking_swift: false,
            swift_left: 0.0,
            slow_cd: 0.0,
            poison_cd: 0.0,
            age: 0,
            egg_timer: if kind == MobKind::Chicken { 300.0 + yaw.rem_euclid(TAU) / TAU * 300.0 } else { f32::MAX },
            hop_left: 0.0,
            hop_delay: 1.0,
            difficulty: Default::default(),
            riding: None,
            pos,
            previous_pos: pos,
            vel: DVec3::ZERO,
            yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            health: kind.max_health(),
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
            trader: matches!(kind, MobKind::WanderingTrader | MobKind::TraderLlama)
                .then(|| Box::new(super::wandering_trader::Trader::new(pos))),
        }
    }

    pub(super) fn melee_attack(&self) -> (f32, &'static str) {
        let (base, cause) = self.kind.melee();
        let strength = if self.convert_left > 0.0 { 3.0 } else { 0.0 };
        let weakness = if self.weakness_left > 0.0 { 4.0 } else { 0.0 };
        ((base + strength - weakness).max(0.0), cause)
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
        let amount = if self.kind == MobKind::MagmaCube {
            crate::simulation::survival::armor_reduce(amount, self.size as u32 * 3, 0.0)
        } else {
            amount
        };
        self.health -= amount;
        self.hurt = HURT_TIME;
        if let Some(kb) = knockback.map(|kb| kb * self.knockback_taken())
            && self.kind != MobKind::IronGolem
        {
            self.vel.x = kb.x;
            self.vel.z = kb.z;
            self.vel.y = self.vel.y.max(kb.y);
        }
        if !self.kind.is_hostile() && self.kind != MobKind::IronGolem {
            self.ai = Ai::Panic;
            self.ai_timer = rng.range(3.0, 5.0);
            self.move_yaw = rng.range(0.0, TAU);
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
        let dtf = dt as f32;
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
        if self.alive() {
            self.update_trader(dtf, self.trader_night);
        }
        self.hurt = (self.hurt - dtf).max(0.0);
        self.provoked = (self.provoked - dtf).max(0.0);
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

        let (wish, speed) = if let Some(t) = &mut self.dying {
            *t += dtf;
            (None, 0.0)
        } else {
            self.think(dtf, world, ctx, rng, events)
        };

        // Don't walk off tall drops; wanderers pick another direction.
        let wish = wish.filter(|&dir| {
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
        if let Some(dir) = wish {
            let target = (dir.z as f32).atan2(dir.x as f32);
            self.yaw = turn_toward(self.yaw, target, 8.0 * dtf);
        }

        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        if self.riding.is_some() {
            self.vel = DVec3::ZERO;
            self.on_ground = true;
        } else {
            for _ in 0..steps {
                self.physics_step(h, world, wish, speed * self.speed_factor());
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
        if self.alive() && self.kind == MobKind::Chicken {
            let ticks = ((dt * 20.0).round() as i32).max(1);
            if self.age < 0 {
                self.age = (self.age + ticks).min(0);
            } else {
                self.egg_timer -= dtf;
                if self.egg_timer <= 0.0 {
                    self.egg_timer = rng.range(300.0, 600.0);
                    events.push(EntityEvent::LaidEgg { pos: self.pos });
                }
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
        if matches!(self.kind, MobKind::Villager | MobKind::WanderingTrader)
            && let Some(v) = &self.villager
        {
            if v.sleeping || v.trading && !v.fleeing {
                return (None, 0.0);
            }
            if let Some(goal) = v.goal {
                let dir = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
                if dir.length_squared() < 1.0 {
                    return (None, 0.0);
                }
                let mut dir = dir.normalize();
                for dy in 0..2 {
                    let cell = (self.pos + dir * 1.1).floor().as_ivec3() + IVec3::Y * dy;
                    if world.block(cell).is_some_and(|b| {
                        matches!(b.shaped(), Some(crate::world::block::Shaped::Door { open: false, upper: false, .. }))
                    }) {
                        events.push(EntityEvent::VillagerDoor { cell });
                    }
                }
                self.detour -= dt;
                if self.blocked && self.on_ground && self.detour <= 0.0 && !self.can_step_up(world, dir) {
                    self.detour = 0.8;
                    self.detour_side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                }
                if self.detour > 0.0 {
                    dir = (DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side + dir * 0.35).normalize();
                }
                return (Some(dir), if v.fleeing { 4.0 } else { 2.0 });
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
                return (Some(dir), speed);
            }
        }
        if self.kind == MobKind::Witch && self.witch_upkeep(dt, rng) {
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
                    MobKind::Witch => return self.witch_tactics(world, target.pos, dir, hdist, rng, events),
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
                            events.push(EntityEvent::PlayerHit {
                                player: target.id,
                                damage,
                                knockback: knockback.as_vec3(),
                                cause,
                            });
                            if self.kind == MobKind::Husk {
                                // Java: 140 ticks times the local difficulty (about 1, 2, 3).
                                let scale = match self.difficulty {
                                    crate::simulation::difficulty::Difficulty::Peaceful => 0,
                                    crate::simulation::difficulty::Difficulty::Easy => 1,
                                    crate::simulation::difficulty::Difficulty::Normal => 2,
                                    crate::simulation::difficulty::Difficulty::Hard => 3,
                                };
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
                                            burning: self.burning
                                                || target.held_enchants.has(crate::enchant::Enchantment::FireAspect),
                                            player_kill: true,
                                            looting: target.held_enchants.level(crate::enchant::Enchantment::Looting),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                // Stop just short so we don't stand inside the player.
                if hdist <= 0.8 {
                    return (None, 0.0);
                }
                // No pathfinding: head straight for the player, and if a
                // wall we can't jump is in the way, sidestep for a moment.
                self.detour -= dt;
                if self.detour <= 0.0 && self.blocked && self.on_ground && !self.can_step_up(world, dir) {
                    self.detour = rng.range(0.6, 1.2);
                    self.detour_side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                }
                let dir = if self.detour > 0.0 {
                    let side = DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side;
                    (side + dir * 0.35).normalize()
                } else {
                    dir
                };
                (Some(dir), self.kind.chase_speed() * if self.baby { 1.5 } else { 1.0 })
            }
            Ai::Panic => {
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(1.0, 3.0);
                    return (None, 0.0);
                }
                if rng.chance(dt * 1.2) || self.blocked && rng.chance(dt * 4.0) {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), 3.4)
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
                if self.ai_timer <= 0.0 {
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
            let dry = !physics::touches_block(world, to, shape, |b| b.is_water() || b.is_lava());
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
            events.push(EntityEvent::Shoot { from: eye + dir * 0.5, target });
            events.push(EntityEvent::Sound { sound: MobSound::Bow, pos: eye });
        }
        if rng.chance(dt * 0.4) {
            self.detour_side = -self.detour_side;
        }
        let side = DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side;
        if hdist > SKELETON_FAR {
            (Some(dir), self.kind.chase_speed())
        } else if hdist < SKELETON_NEAR {
            (Some((-dir + side * 0.3).normalize()), 2.0)
        } else {
            (Some(side), 1.0)
        }
    }

    /// Java's witch: drinks healing when hurt (1.6 s, standing still), and
    /// returns whether it is mid-drink. Cooldowns tick here too.
    fn witch_upkeep(&mut self, dt: f32, rng: &mut Rng) -> bool {
        self.slow_cd = (self.slow_cd - dt).max(0.0);
        self.poison_cd = (self.poison_cd - dt).max(0.0);
        self.swift_left = (self.swift_left - dt).max(0.0);
        if self.drink_left > 0.0 {
            self.drink_left -= dt;
            if self.drink_left <= 0.0 {
                if self.drinking_swift {
                    // Swiftness: 3 minutes of +20% speed.
                    self.swift_left = 180.0;
                } else {
                    self.health = (self.health + 4.0).min(self.kind.max_health());
                }
            }
            return self.drink_left > 0.0;
        }
        if self.health < self.kind.max_health() && rng.chance(dt) {
            self.drink_left = 1.6;
            self.drinking_swift = false;
            return true;
        }
        false
    }

    /// Java's witch: lobs a splash potion every three seconds from up to ten
    /// blocks away (slowness from afar, poison, otherwise harming), standing
    /// still while it has a clear throw, and drinks swiftness to close a
    /// long gap. The potion isn't chosen from the target's health or effects.
    fn witch_tactics<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        player: DVec3,
        dir: DVec3,
        hdist: f64,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.9);
        let target = player + DVec3::Y * 1.2;
        let boost = if self.swift_left > 0.0 { 1.2 } else { 1.0 };
        let sees = line_of_sight(world, eye, target);
        if self.swift_left <= 0.0 && hdist > 11.0 && rng.chance(0.01) {
            self.drink_left = 1.6;
            self.drinking_swift = true;
            return (None, 0.0);
        }
        if hdist <= WITCH_RANGE && sees {
            if self.attack_cooldown <= 0.0 {
                let id = if hdist >= 8.0 && self.slow_cd <= 0.0 {
                    self.slow_cd = 30.0;
                    "slowness"
                } else if self.poison_cd <= 0.0 {
                    self.poison_cd = 45.0;
                    "poison"
                } else {
                    "harming"
                };
                let flat = DVec3::new(target.x - eye.x, 0.0, target.z - eye.z);
                let aim = DVec3::new(flat.x, target.y - 1.1 - eye.y + flat.length() * 0.2, flat.z).normalize_or(dir);
                if let Some(potion) = crate::potion::Potion::from_id(id) {
                    events.push(EntityEvent::ThrowPotion { from: eye + dir * 0.4, vel: aim * WITCH_SPEED, potion });
                }
                self.attack_cooldown = 3.0;
                self.attack_anim = 0.35;
            }
            return (None, 0.0);
        }
        (Some(dir), self.kind.chase_speed() * boost)
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

    fn physics_step<W: BlockSource + ?Sized>(&mut self, dt: f64, world: &W, wish: Option<DVec3>, speed: f64) {
        if self.kind == MobKind::Strider && self.walk_on_lava(dt, world, wish, speed) {
            return;
        }
        let shape = self.shape();
        self.in_water = physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.3, 0.0));
        if self.kind == MobKind::SnowGolem && self.alive() && self.in_water {
            // Java: one damage each tick while in water. Death loot is emitted
            // by the caller once health is gone.
            self.health -= 20.0 * dt as f32;
        }
        let hopping = self.kind.is_cube();
        let speed = if hopping { (0.2 + 0.1 * self.size as f64) * 10.0 } else { speed };
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
        let target = wish.map_or(DVec3::ZERO, |d| d * speed);

        if self.kind == MobKind::Ghast && self.alive() {
            // Java ghasts fly: no gravity, and they steer in three dimensions.
            let k = (dt * 1.6).min(1.0);
            self.vel += (target - self.vel) * k;
        } else if self.in_water {
            let k = (dt * 4.0).min(1.0);
            self.vel.x += (target.x * 0.6 - self.vel.x) * k;
            self.vel.z += (target.z * 0.6 - self.vel.z) * k;
            // Buoyant below ~0.6 blocks of depth, so mobs bob at the surface.
            if physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.6, 0.0)) {
                self.vel.y = (self.vel.y + 22.0 * dt).min(2.0);
            } else {
                self.vel.y -= GRAVITY * 0.25 * dt;
            }
            self.vel.y = self.vel.y.max(-4.0);
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
            self.vel.y = (self.vel.y - GRAVITY * dt).max(-78.0);
            if self.kind == MobKind::Chicken {
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
            } else if let Some(dir) = wish
                && self.on_ground
                && self.blocked
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
        let hit = if self.on_ground {
            physics::move_box_stepping(world, &mut self.pos, &mut self.vel, delta, shape, crate::player::STEP_HEIGHT)
        } else {
            physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape)
        };
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
        let wet = physics::touches_block(world, self.pos, self.shape(), Block::is_water) || world.rains_on(head);
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
            Some(b) if b.is_solid() || b.is_water() => return false,
            Some(_) => {}
        }
    }
    true
}

/// Rough sky light (0..1) at a point: 1 in the open, dimmed under water or
/// leaves, and falling off with distance from the nearest open column
/// under overhangs; 0 deep underground.
pub fn sky_light<W: MobWorld + ?Sized>(world: &W, p: DVec3) -> f32 {
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

/// Whether nothing solid lies on the straight line between two points.
pub(super) fn line_of_sight<W: BlockSource + ?Sized>(world: &W, from: DVec3, to: DVec3) -> bool {
    let d = to - from;
    let steps = (d.length() * 4.0).ceil().max(1.0) as i32;
    (1..steps).all(|i| {
        let p = from + d * (i as f64 / steps as f64);
        !world.block(p.floor().as_ivec3()).is_some_and(|b| b.is_solid())
    })
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
