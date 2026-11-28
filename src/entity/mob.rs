//! A single mob: AI, movement physics and combat state.

use std::f32::consts::{PI, TAU};

use glam::{DVec3, IVec3};

use crate::item::Item;
use crate::physics::{self, BlockSource, Shape};
use crate::simulation::damage::Cause;
use crate::world::block::Block;
use crate::world::terrain::Dimension;

use super::{Ctx, EntityEvent, MobSound, MobWorld, Rng, path};

pub(crate) mod brain_walk;
mod breeze;
pub(crate) mod chase;
mod creaking;
pub(crate) mod dolphin;
pub(crate) mod door_break;
mod drowned;
mod endermite;
pub(crate) mod equine;
mod eyes;
pub(crate) use eyes::baby_shape;
mod frog;
pub(crate) mod goals;
pub(super) mod hazards;
pub(crate) mod head;
mod illager;
mod navigate;
pub(crate) use navigate::is_sand;
mod fox;
mod nether_ai;
mod ocelot;
mod polar_bear;
mod raider;
pub(crate) mod selector;
pub(crate) mod spear;
mod sulfur_cube;
pub(crate) mod swim;
pub(crate) mod turtle;
#[cfg(test)]
pub(crate) use nether_ai::STABLE_LAVA_HEIGHT;

const GRAVITY: f64 = 28.0;
/// A walker's speed on a slime block against ordinary ground. Java moves a
/// walker `a / (1 - f)` a tick on ground of friction `f * 0.91` with
/// acceleration `a ∝ 0.216 / f³`; `SlimeBlock.stepOn` multiplies the
/// motion by 0.4 a tick as well, so with slime's 0.8 against 0.6 the ratio
/// is `(0.546 / 0.728)³ * (1 - 0.546) / (1 - 0.4 * 0.728)`.
const SLIME_WALK: f64 = 0.270;
/// What a bounce keeps of a mob's speed. Mobs fall here without Java's
/// vertical drag of 0.98 a tick, which makes each of Java's slime bounces
/// about 0.7 as high as the last (a 5-block drop: 3.6, 2.3, 1.8 blocks);
/// `√0.7` of the speed gives the same.
const BOUNCE_DRAG: f64 = 0.84;
/// Clears a 1-block ledge with a little margin (peak ~1.26 blocks).
const JUMP_VELOCITY: f64 = 8.4;
const MAX_STEP: f64 = 1.0 / 60.0;
/// Seconds of red flash (and knockback immunity) after taking damage.
pub const HURT_TIME: f32 = 0.5;
/// v1.0 gap Track 8d: `hurtServer`'s `damageCooldownTime = 20` on a full hit.
pub const COOLDOWN: u8 = 20;
/// The hurt cooldown's half-way mark: above it a hit lands only by its excess.
pub const COOLDOWN_HALF: u8 = 10;
/// How long a dead mob stays before it is removed. v1.0 gap Track 7:
/// Java's `tickDeath` removes it as its `deathTime` reaches 20 ticks after
/// death; this game removes it at the start of a tick, before that tick's
/// update, so the threshold sits between 18 and 19 ticks of
/// [`Mob::dying`] and the mob is last drawn at the same `deathTime` 19.
pub const DEATH_TIME: f32 = (super::motion::DEATH_TICKS - 1.5) * super::motion::TICK;
/// What [`Mob::pose_values`] holds (see there).
pub type Pose = [f32; 7];
/// Hostile mobs notice players within this many blocks.
const CHASE_RANGE: f64 = 24.0;
const ATTACK_RANGE: f64 = 1.2;
const ATTACK_COOLDOWN: f32 = 1.0;
/// Zombies and skeletons burn in sunlight above this daylight level.
const BURN_DAYLIGHT: f32 = 0.45;
/// Spiders only hunt when it's darker than this (or after being hit).
const SPIDER_CALM_DAYLIGHT: f32 = 0.45;
/// Seconds a hit spider stays hostile in daylight.
pub(super) const PROVOKED_TIME: f32 = 12.0;
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
pub(super) const ENDERMAN_ANGER_TIME: f32 = 30.0;
/// Seconds of eye contact before an enderman turns on you (Java's 5 ticks).
const STARE_TIME: f32 = 0.25;
/// Endermen notice stares from this far, freeze while watched within
/// `FREEZE_DIST`, flee a stare closer than `FLEE_DIST`, and teleport after
/// targets farther than `FREEZE_DIST`.
const STARE_RANGE: f64 = 64.0;
const FREEZE_DIST: f64 = 16.0;
const FLEE_DIST: f64 = 4.0;
/// Blazes notice players this far away (Java's follow range).
const BLAZE_RANGE: f64 = 48.0;
/// Seconds a blaze charges (glowing) before a burst, between the burst's
/// three fireballs, and resting after it (Java's 60, 6 and 100 ticks).
const BLAZE_CHARGE: f32 = 3.0;
const BLAZE_VOLLEY: f32 = 0.3;
const BLAZE_REST: f32 = 5.0;
/// Ghasts notice players this far (Java's follow range).
const GHAST_RANGE: f64 = 100.0;
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
    SkeletonHorse,
    ZombieHorse,
    Bee,
    Stray,
    Bogged,
    Phantom,
    Mooshroom,
    Bat,
    Frog,
    Tadpole,
    PolarBear,
    Panda,
    Armadillo,
    Ocelot,
    CopperGolem,
    Creaking,
    Allay,
    Sniffer,
    /// v0.16 (LAN index 72): the deep dark's blind guardian.
    Warden,
    /// v0.16 Track C. Saved as `breeze`; LAN index 73 (72 stays reserved
    /// for Track A's warden until integration).
    Breeze,
    /// v0.14 (MOBN-26): the pearl pest. Saved as `endermite`; its LAN index
    /// is its position in [`MobKind::ALL`].
    Endermite,
    /// v0.15 Track A: an axe-swinging illager. Saved as `vindicator`; its
    /// LAN index is its position in [`MobKind::ALL`] (see `entity::illager`).
    Vindicator,
    /// v0.15 Track A: the spellcaster that raises fangs and vexes.
    Evoker,
    /// v0.15 Track A: an evoker's summoned flier.
    Vex,
    /// v0.15 Track A: the illagers' beast, which an illager can ride.
    Ravager,
    /// v0.15 Track A: commands only, as in Java.
    Illusioner,
    /// v0.15 Track C: grown from a dried ghast; ridden in a harness by up
    /// to four players. Saved as `happy_ghast` (see `entity::happy_ghast`).
    HappyGhast,
    /// v0.15 Track C: tamed with pufferfish, saddled and ridden underwater.
    /// Saved as `nautilus` (see `entity::nautilus`).
    Nautilus,
    /// v0.15 Track C: the undead nautilus drowned ride. Saved as `zombie_nautilus`.
    ZombieNautilus,
    /// v0.15 Track C: the undead camel husks ride at night in deserts.
    /// Saved as `camel_husk` (see `entity::camel_husk`).
    CamelHusk,
    /// v0.15 Track C: the desert skeleton with Weakness arrows. Saved as `parched`.
    Parched,
    /// v0.16 Track C: the three-headed boss built from soul sand and
    /// wither skeleton skulls. Saved as `wither` (see `entity::wither`).
    Wither,
    /// v0.16 Track A: the End city's shell-dwelling turret. Saved as
    /// `shulker` (see `entity::shulker`).
    Shulker,
    /// v0.16 Track D: the sulfur caves' block-swallowing cube. Saved as
    /// `sulfur_cube` (see `entity::sulfur_cube`).
    SulfurCube,
}

impl MobKind {
    pub fn is_aquatic(self) -> bool {
        matches!(
            self,
            Self::Cod
                | Self::Tadpole
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
        matches!(self, Self::Slime | Self::MagmaCube | Self::SulfurCube)
    }
    /// Wire index: the position in [`MobKind::ALL`] (warden 72, breeze 73).
    pub fn lan_index(self) -> usize {
        Self::ALL.iter().position(|&k| k == self).unwrap()
    }
    pub fn from_lan_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
    pub const ALL: [MobKind; 88] = [
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
        MobKind::SkeletonHorse,
        MobKind::ZombieHorse,
        MobKind::Bee,
        MobKind::Stray,
        MobKind::Bogged,
        MobKind::Phantom,
        MobKind::Mooshroom,
        MobKind::Bat,
        MobKind::Frog,
        MobKind::Tadpole,
        MobKind::PolarBear,
        MobKind::Panda,
        MobKind::Armadillo,
        MobKind::Ocelot,
        MobKind::CopperGolem,
        MobKind::Creaking,
        MobKind::Allay,
        MobKind::Sniffer,
        MobKind::Warden,
        MobKind::Breeze,
        MobKind::Endermite,
        MobKind::Vindicator,
        MobKind::Evoker,
        MobKind::Vex,
        MobKind::Ravager,
        MobKind::Illusioner,
        MobKind::HappyGhast,
        MobKind::Nautilus,
        MobKind::ZombieNautilus,
        MobKind::CamelHusk,
        MobKind::Parched,
        MobKind::Wither,
        MobKind::Shulker,
        MobKind::SulfurCube,
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
            MobKind::SkeletonHorse => "skeleton_horse",
            MobKind::ZombieHorse => "zombie_horse",
            MobKind::Bee => "bee",
            MobKind::Stray => "stray",
            MobKind::Bogged => "bogged",
            MobKind::Phantom => "phantom",
            MobKind::Mooshroom => "mooshroom",
            MobKind::Bat => "bat",
            MobKind::Frog => "frog",
            MobKind::Tadpole => "tadpole",
            MobKind::PolarBear => "polar_bear",
            MobKind::Panda => "panda",
            MobKind::Armadillo => "armadillo",
            MobKind::Ocelot => "ocelot",
            MobKind::CopperGolem => "copper_golem",
            MobKind::Creaking => "creaking",
            MobKind::Allay => "allay",
            MobKind::Sniffer => "sniffer",
            MobKind::Warden => "warden",
            MobKind::Breeze => "breeze",
            MobKind::Endermite => "endermite",
            MobKind::Vindicator => "vindicator",
            MobKind::Evoker => "evoker",
            MobKind::Vex => "vex",
            MobKind::Ravager => "ravager",
            MobKind::Illusioner => "illusioner",
            MobKind::HappyGhast => "happy_ghast",
            MobKind::Nautilus => "nautilus",
            MobKind::ZombieNautilus => "zombie_nautilus",
            MobKind::CamelHusk => "camel_husk",
            MobKind::Parched => "parched",
            MobKind::Wither => "wither",
            MobKind::Shulker => "shulker",
            MobKind::SulfurCube => "sulfur_cube",
        }
    }

    /// Looks a mob up by name (spaces or underscores).
    pub fn from_name(name: &str) -> Option<MobKind> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name).replace('_', " ");
        Self::ALL.into_iter().find(|k| k.name().replace('_', " ") == name)
    }

    /// Collision box dimensions for movement and spawn-space checks.
    pub fn shape(self) -> Shape {
        match self {
            MobKind::Sniffer => Shape::new(0.95, 1.75),
            MobKind::Warden => Shape::new(0.45, 2.9),
            // `EntityType.BREEZE`: 0.6 x 1.77.
            MobKind::Breeze => Shape::new(0.3, 1.77),
            // `EntityTypes.ENDERMITE`: 0.4 x 0.3.
            MobKind::Endermite => Shape::new(0.2, 0.3),
            // `EntityTypes`: illagers 0.6 x 1.95, the vex 0.4 x 0.8 and the
            // ravager 1.95 x 2.2 (`tests/fixtures/java/26.3/illagers.json`).
            MobKind::Vindicator | MobKind::Evoker | MobKind::Illusioner => Shape::new(0.3, 1.95),
            MobKind::Vex => Shape::new(0.2, 0.8),
            MobKind::Ravager => Shape::new(0.975, 2.2),
            // `EntityTypes.HAPPY_GHAST`: 4 x 4.
            MobKind::HappyGhast => Shape::new(2.0, 4.0),
            // `EntityTypes.NAUTILUS` and `ZOMBIE_NAUTILUS`: 0.875 x 0.95.
            MobKind::Nautilus | MobKind::ZombieNautilus => Shape::new(0.4375, 0.95),
            // `EntityTypes.CAMEL_HUSK`: the camel's 1.7 x 2.375.
            MobKind::CamelHusk => Shape::new(0.85, 2.375),
            // `EntityTypes.PARCHED`: 0.6 x 1.99.
            MobKind::Parched => Shape::new(0.3, 1.99),
            // `EntityTypes.WITHER`: 0.9 x 3.5.
            MobKind::Wither => Shape::new(0.45, 3.5),
            // `EntityTypes.SHULKER`: 1 x 1 (shut; the lid is not modelled in the box).
            MobKind::Shulker => Shape::new(0.5, 1.0),
            MobKind::Bat => Shape::new(0.25, 0.9),
            MobKind::Frog => Shape::new(0.25, 0.5),
            MobKind::Tadpole => Shape::new(0.2, 0.3),
            MobKind::PolarBear => Shape::new(0.7, 1.4),
            MobKind::Panda => Shape::new(0.65, 1.25),
            MobKind::Armadillo => Shape::new(0.35, 0.65),
            MobKind::Pig => Shape::new(0.45, 0.9),
            MobKind::Cow | MobKind::Mooshroom => Shape::new(0.45, 1.4),
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
            MobKind::Allay => Shape::new(0.175, 0.6),
            MobKind::CopperGolem => Shape::new(0.245, 0.98),
            MobKind::Creaking => Shape::new(0.45, 2.7),
            MobKind::IronGolem => Shape::new(0.7, 2.7),
            MobKind::SnowGolem => Shape::new(0.35, 1.9),
            MobKind::TraderLlama => Shape::new(0.45, 1.87),
            MobKind::Skeleton | MobKind::Stray | MobKind::Bogged => Shape::new(0.3, 1.99),
            MobKind::Creeper => Shape::new(0.3, 1.7),
            MobKind::Spider => Shape::new(0.7, 0.9),
            MobKind::CaveSpider => Shape::new(0.35, 0.5),
            MobKind::Slime | MobKind::MagmaCube => Shape::new(0.255, 0.51),
            // `EntityTypes.SULFUR_CUBE`: 0.49 x 0.49, scaled by the size.
            MobKind::SulfurCube => Shape::new(super::sulfur_cube::BASE_SIZE * 0.5, super::sulfur_cube::BASE_SIZE),
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
            MobKind::Cat | MobKind::Ocelot => Shape::new(0.3, 0.7),
            MobKind::Pillager => Shape::new(0.3, 1.95),
            MobKind::Turtle => Shape::new(0.6, 0.4),
            MobKind::Wolf => Shape::new(0.3, 0.85),
            MobKind::Fox => Shape::new(0.3, 0.7),
            MobKind::Parrot => Shape::new(0.25, 0.9),
            MobKind::Rabbit => Shape::new(0.2, 0.5),
            MobKind::Goat => Shape::new(0.45, 1.3),
            MobKind::SkeletonHorse | MobKind::ZombieHorse | MobKind::Horse => Shape::new(0.698, 1.6),
            MobKind::Donkey => Shape::new(0.698, 1.5),
            MobKind::Mule => Shape::new(0.698, 1.6),
            MobKind::Llama => Shape::new(0.45, 1.87),
            MobKind::Phantom => Shape::new(0.45, 0.5),
            MobKind::Camel => Shape::new(0.85, 2.375),
            MobKind::Bee => Shape::new(0.35, 0.6),
        }
    }

    /// Starting health in damage points, with two points per heart.
    pub fn max_health(self) -> f32 {
        match self {
            MobKind::Pig | MobKind::Cow | MobKind::Mooshroom | MobKind::Frog => 10.0,
            MobKind::Sheep | MobKind::Silverfish | MobKind::Endermite => 8.0,
            MobKind::Chicken => 4.0,
            MobKind::Zombie
            | MobKind::Husk
            | MobKind::Drowned
            | MobKind::Skeleton
            | MobKind::Stray
            | MobKind::WitherSkeleton
            | MobKind::Creeper
            | MobKind::ZombifiedPiglin
            | MobKind::Blaze
            | MobKind::Villager
            | MobKind::WanderingTrader
            | MobKind::ZombieVillager
            | MobKind::Strider
            | MobKind::Panda => 20.0,
            MobKind::Sniffer => 14.0,
            MobKind::Warden => 500.0,
            MobKind::Breeze => 30.0,
            MobKind::Allay => 20.0,
            MobKind::CopperGolem => 12.0,
            MobKind::Creaking => 1.0,
            MobKind::IronGolem => 100.0,
            MobKind::SnowGolem => 4.0,
            MobKind::TraderLlama => 30.0,
            MobKind::Spider | MobKind::Bogged => 16.0,
            MobKind::CaveSpider => 12.0,
            MobKind::Witch => 26.0,
            MobKind::Slime | MobKind::MagmaCube => 1.0,
            MobKind::Ghast => 10.0,
            MobKind::Enderman => 40.0,
            MobKind::Piglin => 16.0,
            MobKind::PiglinBrute => 50.0,
            MobKind::Hoglin | MobKind::Zoglin => 40.0,
            MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish | MobKind::Pufferfish => 3.0,
            MobKind::Squid | MobKind::GlowSquid | MobKind::Dolphin | MobKind::Cat | MobKind::Ocelot => 10.0,
            MobKind::Axolotl => 14.0,
            MobKind::Guardian => 30.0,
            MobKind::ElderGuardian => 80.0,
            MobKind::Pillager => 24.0,
            MobKind::Turtle | MobKind::PolarBear => 30.0,
            MobKind::Armadillo => 12.0,
            MobKind::Wolf => 8.0,
            MobKind::Fox | MobKind::Goat => 10.0,
            MobKind::Parrot | MobKind::Bat | MobKind::Tadpole => 6.0,
            MobKind::Rabbit => 3.0,
            MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama => 30.0,
            MobKind::Camel => 32.0,
            MobKind::SkeletonHorse => 15.,
            MobKind::ZombieHorse => 25.,
            MobKind::Bee => 10.0,
            MobKind::Phantom => 20.0,
            MobKind::Vindicator | MobKind::Evoker => 24.0,
            MobKind::Vex => 14.0,
            MobKind::Ravager => 100.0,
            MobKind::Illusioner => 32.0,
            // `HappyGhast.createAttributes`, `AbstractNautilus.createAttributes`,
            // `Camel.createAttributes` (camel husks share it) and `Parched.createAttributes`.
            MobKind::HappyGhast => super::happy_ghast::MAX_HEALTH,
            MobKind::Nautilus | MobKind::ZombieNautilus => super::nautilus::MAX_HEALTH,
            MobKind::CamelHusk => 32.0,
            MobKind::Parched => 16.0,
            MobKind::Wither => super::wither::MAX_HEALTH,
            MobKind::Shulker => super::shulker::MAX_HEALTH,
            // `SulfurCube.setCubeMobHealth`: 4 x size (an adult's 2).
            MobKind::SulfurCube => 8.0,
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
                | MobKind::Stray
                | MobKind::Bogged
                | MobKind::Phantom
                | MobKind::Creaking
                | MobKind::Warden
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
                | MobKind::Breeze
                | MobKind::Endermite
                | MobKind::Vindicator
                | MobKind::Evoker
                | MobKind::Vex
                | MobKind::Ravager
                | MobKind::Illusioner
                | MobKind::Parched
                | MobKind::Wither
                | MobKind::Shulker
                | MobKind::SulfurCube
        )
    }

    /// Java 26.3 `EntityType.isAllowedInPeaceful` is false for `Mob.checkDespawn`
    /// to discard these on Peaceful. Hostile here, except the piglin: its type
    /// is not registered with `notInPeaceful()` (piglin brutes, hoglins and
    /// zoglins are).
    pub fn despawns_in_peaceful(self) -> bool {
        // The shulker's and the sulfur cube's types aren't `notInPeaceful`
        // either (checked in the 26.3 jar's `EntityTypes`), so End city
        // shulkers stay in Peaceful.
        self.is_hostile() && !matches!(self, MobKind::Piglin | MobKind::Shulker | MobKind::SulfurCube)
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
                | MobKind::Warden
                // `EntityTypes.VEX.fireImmune()`.
                | MobKind::Vex
                // `EntityTypes.WITHER.fireImmune()`.
                | MobKind::Wither
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
            MobKind::Sniffer
            | MobKind::Warden
            | MobKind::Mule
            | MobKind::Bee
            | MobKind::Phantom
            | MobKind::CopperGolem
            | MobKind::Creaking
            | MobKind::Allay
            // Only from trial spawners.
            | MobKind::Breeze => false,
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
            // No biome lists it; only landed ender pearls make them.
            MobKind::Endermite => false,
            // Only End city and ship markers and duplication make them.
            MobKind::Shulker => false,
            // No biome lists illagers: raids, patrols and mansions bring them
            // (later v0.15 tracks), vexes come only from evokers and
            // illusioners only from commands.
            MobKind::Vindicator | MobKind::Evoker | MobKind::Vex | MobKind::Ravager | MobKind::Illusioner => false,
            // Dried ghasts hatch happy ghasts; camel husks come with husks
            // and zombie nautiluses with drowned (`Husk`/`Drowned.finalizeSpawn`).
            MobKind::HappyGhast | MobKind::CamelHusk | MobKind::ZombieNautilus => false,
            // Only built from soul sand and wither skeleton skulls (v0.16 Track C).
            MobKind::Wither => false,
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
            // Java lists parched only in deserts (`monster_spawns::DESERT`).
            (MobKind::Parched, Biome::Desert) => 1.0,
            (MobKind::Parched, _) => 0.0,
            // v0.16 Track D: Java lists the sulfur cube only in sulfur caves
            // (`monster_spawns::SULFUR_CAVES`).
            (MobKind::SulfurCube, Biome::SulfurCaves) => 1.0,
            (MobKind::SulfurCube, _) => 0.0,
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
            | MobKind::Zoglin
            | MobKind::SkeletonHorse
            | MobKind::ZombieHorse
            | MobKind::Stray
            | MobKind::Bogged
            | MobKind::Parched
            | MobKind::CamelHusk
            | MobKind::ZombieNautilus
            | MobKind::Phantom
            | MobKind::Wither => Creature::Undead,
            // `#sensitive_to_bane_of_arthropods` (`#arthropod`).
            MobKind::Spider | MobKind::CaveSpider | MobKind::Silverfish | MobKind::Endermite | MobKind::Bee => {
                Creature::Arthropod
            }
            _ => Creature::Other,
        }
    }

    pub(super) fn burns_in_sun(self) -> bool {
        matches!(
            self,
            MobKind::Zombie
                | MobKind::ZombieHorse
                | MobKind::Drowned
                | MobKind::ZombieVillager
                | MobKind::Skeleton
                | MobKind::Stray
                | MobKind::Bogged
                | MobKind::Phantom
                // `#burn_in_daylight`; its body armor shades it (`sunProtectionSlot`).
                | MobKind::ZombieNautilus
        )
    }

    fn wander_speed(self) -> f64 {
        match self {
            MobKind::Pig => 1.3,
            MobKind::Cow
            | MobKind::Mooshroom
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
            MobKind::Sheep
            | MobKind::Skeleton
            | MobKind::Stray
            | MobKind::Bogged
            | MobKind::Parched
            | MobKind::WitherSkeleton
            | MobKind::Blaze => 1.2,
            MobKind::Chicken | MobKind::Slime | MobKind::MagmaCube => 1.0,
            MobKind::Ghast => 4.0,
            MobKind::Spider | MobKind::CaveSpider | MobKind::Enderman | MobKind::Silverfish => 1.4,
            // `WaterAvoidingRandomStrollGoal(1.0)` at movement speed 0.25.
            MobKind::Endermite => path::ground_speed(super::endermite::MOVEMENT_SPEED),
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
            MobKind::Skeleton | MobKind::Stray | MobKind::Bogged | MobKind::Parched => 2.2,
            MobKind::Creeper => 2.0,
            _ => 2.4,
        }
    }

    /// Melee damage and the death message it gives.
    pub(super) fn melee(self) -> (f32, Cause) {
        match self {
            MobKind::CaveSpider => (2.0, Cause::mob("was slain by a cave spider")),
            MobKind::Spider => (2.0, Cause::mob("was slain by a spider")),
            MobKind::ZombifiedPiglin => (5.0, Cause::mob("was slain by a zombified piglin")),
            MobKind::Enderman => (7.0, Cause::mob("was slain by an enderman")),
            MobKind::Blaze => (6.0, Cause::mob("was slain by a blaze")),
            MobKind::WitherSkeleton => (8.0, Cause::mob("was slain by a wither skeleton")),
            MobKind::Husk => (3.0, Cause::mob("was slain by a husk")),
            MobKind::Drowned => (3.0, Cause::mob("was slain by a drowned")),
            MobKind::Silverfish => (1.0, Cause::mob("was slain by a silverfish")),
            MobKind::Endermite => (super::endermite::ATTACK_DAMAGE, Cause::mob("was slain by an endermite")),
            MobKind::PolarBear => (6.0, Cause::mob("was slain by a polar bear")),
            MobKind::Panda => (6.0, Cause::mob("was slain by a panda")),
            MobKind::Ocelot => (3.0, Cause::mob("was slain by an ocelot")),
            MobKind::IronGolem => (15.0, Cause::mob("was slain by an iron golem")),
            MobKind::SnowGolem => (0.0, Cause::mob("was slain by a snow golem")),
            MobKind::ZombieVillager => (3.0, Cause::mob("was slain by a zombie villager")),
            MobKind::Piglin => (5.0, Cause::mob("was slain by a piglin")),
            MobKind::PiglinBrute => (7.0, Cause::mob("was slain by a piglin brute")),
            MobKind::Hoglin => (6.0, Cause::mob("was slain by a hoglin")),
            MobKind::Zoglin => (6.0, Cause::mob("was slain by a zoglin")),
            MobKind::Warden => (super::warden::MELEE_DAMAGE, super::warden::CAUSE_MELEE),
            // `createAttributes` attack damage, before the held weapon.
            MobKind::Vindicator => (5.0, Cause::mob("was slain by a vindicator")),
            MobKind::Pillager => (5.0, Cause::mob("was slain by a pillager")),
            MobKind::Vex => (4.0, Cause::mob("was slain by a vex")),
            MobKind::Ravager => (12.0, Cause::mob("was slain by a ravager")),
            // `Monster.createMonsterAttributes`' registry default 2.
            MobKind::Evoker => (2.0, Cause::mob("was slain by an evoker")),
            MobKind::Illusioner => (2.0, Cause::mob("was slain by an illusioner")),
            MobKind::Nautilus => (super::nautilus::ATTACK_DAMAGE, Cause::mob("was slain by a nautilus")),
            MobKind::ZombieNautilus => (super::nautilus::ATTACK_DAMAGE, Cause::mob("was slain by a zombie nautilus")),
            _ => (3.0, Cause::mob("was slain by a zombie")),
        }
    }

    /// Loot for a player kill: (item, min, max) rolls. A negative minimum
    /// makes the drop a chance (Java's spider eye: -1..1 is one in three).
    pub(super) fn loot(self) -> &'static [(Item, i8, u8)] {
        const WOOL: Item = Item::from_block(Block::WOOL);
        const SPONGE: Item = Item::from_block(crate::world::overworld_blocks::WET_SPONGE);
        const SEAGRASS: Item = Item::from_block(crate::world::overworld_blocks::SEAGRASS);
        const POPPY: Item = Item::from_block(Block::POPPY);
        const BAMBOO: Item = Item::from_block(crate::world::overworld_blocks::BAMBOO);
        const CATALYST: Item = Item::from_block(crate::world::cave_blocks::SCULK_CATALYST);
        match self {
            MobKind::Pig => &[(Item::RAW_PORKCHOP, 1, 3)],
            MobKind::Bat | MobKind::Frog | MobKind::Tadpole | MobKind::PolarBear | MobKind::Armadillo => &[],
            MobKind::Panda => &[(BAMBOO, 1, 1)],
            MobKind::Cow => &[(Item::RAW_BEEF, 1, 3), (Item::LEATHER, 0, 2)],
            MobKind::Mooshroom => &[(Item::LEATHER, 0, 2), (Item::RAW_BEEF, 1, 3)],
            MobKind::Sheep => &[(WOOL, 1, 1), (Item::RAW_MUTTON, 1, 2)],
            MobKind::Chicken => &[(Item::RAW_CHICKEN, 1, 1), (Item::FEATHER, 0, 2)],
            MobKind::Zombie | MobKind::Husk | MobKind::Drowned | MobKind::ZombieVillager => {
                &[(Item::ROTTEN_FLESH, 0, 2)]
            }
            MobKind::SkeletonHorse => &[(Item::BONE, 0, 2)],
            MobKind::ZombieHorse => &[(Item::ROTTEN_FLESH, 2, 3)],
            // `entities/parched` adds a player-kill Weakness arrow (rolled with the strays').
            MobKind::Skeleton | MobKind::Stray | MobKind::Bogged | MobKind::Parched => {
                &[(Item::BONE, 0, 2), (Item::ARROW, 0, 2)]
            }
            // `entities/camel_husk`: 2-3 rotten flesh. `entities/zombie_nautilus`'s
            // 0-3 rotten flesh needs a player kill (gated where loot drops), and
            // `entities/happy_ghast` has no pools. A nautilus's shell is rolled in `drops`.
            MobKind::CamelHusk => &[(Item::ROTTEN_FLESH, 2, 3)],
            MobKind::ZombieNautilus => &[(Item::ROTTEN_FLESH, 0, 3)],
            MobKind::HappyGhast | MobKind::Nautilus => &[],
            MobKind::Creeper => &[(Item::GUNPOWDER, 0, 2)],
            MobKind::Spider | MobKind::CaveSpider => &[(Item::STRING, 0, 2), (Item::SPIDER_EYE, -1, 1)],
            MobKind::ZombifiedPiglin => &[(Item::ROTTEN_FLESH, 0, 1), (Item::GOLD_NUGGET, 0, 1)],
            MobKind::Enderman => &[(Item::ENDER_PEARL, 0, 1)],
            MobKind::Blaze => &[(Item::BLAZE_ROD, 0, 1)],
            // `entities/breeze`: 1-2 rods, killed by a player (rolled in `drops`).
            MobKind::Breeze => &[],
            // Piglins drop only what they carry (`nether::equipment_drops`).
            // `entities/endermite` and `entities/sulfur_cube` have no pools
            // (a sulfur cube drops only the block it holds); `entities/shulker`'s
            // shell is rolled in `drops`.
            MobKind::Shulker
            | MobKind::Endermite
            | MobKind::SulfurCube
            | MobKind::Silverfish
            | MobKind::Villager
            | MobKind::WanderingTrader
            | MobKind::Piglin
            | MobKind::PiglinBrute
            | MobKind::Camel
            | MobKind::Bee
            | MobKind::Allay => &[],
            MobKind::TraderLlama | MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama => {
                &[(Item::LEATHER, 0, 2)]
            }
            MobKind::CopperGolem => &[(Item::COPPER_INGOT, 1, 3)],
            MobKind::Creaking => &[],
            // `entities/warden`: one sculk catalyst, unconditionally.
            MobKind::Warden => &[(CATALYST, 1, 1)],
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
            MobKind::Sniffer | MobKind::Axolotl | MobKind::Cat | MobKind::Ocelot | MobKind::Pillager => &[],
            // `entities/vindicator` and the evoker's emerald pool (killed by
            // a player, 0-1 plus Looting) and the evoker's totem, the
            // ravager's saddle: rolled in `illager::drops`. `entities/vex`
            // and `entities/illusioner` have no pools.
            MobKind::Vindicator | MobKind::Evoker | MobKind::Vex | MobKind::Ravager | MobKind::Illusioner => &[],
            MobKind::Turtle => &[(SEAGRASS, 0, 2)],
            MobKind::Wolf | MobKind::Fox | MobKind::Goat => &[],
            MobKind::Parrot => &[(Item::FEATHER, 1, 2)],
            // 26.3 `entities/rabbit`: always one raw rabbit (v1.0 Track 6).
            MobKind::Rabbit => &[(Item::RAW_RABBIT, 1, 1), (Item::RABBIT_HIDE, 0, 1)],
            // The skull is rolled in `drops`.
            MobKind::WitherSkeleton => &[(Item::COAL, 0, 1), (Item::BONE, 0, 2)],
            // `entities/wither` has no pools; the Nether star is
            // `dropCustomDeathLoot` (`Entities::wither_star`).
            MobKind::Wither => &[],
            // Java rolls a few of these; each is rolled on its own here.
            MobKind::Phantom => &[(Item::PHANTOM_MEMBRANE, 0, 1)],
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
        if self == MobKind::Panda {
            return vec![(Item::from_block(crate::world::overworld_blocks::BAMBOO), 1)];
        }
        if self == MobKind::PolarBear {
            let item = if rng.next_int(4) < 3 { Item::COD } else { Item::SALMON };
            let count = rng.next_int(3) + (f32::from(looting) * rng.next_f32()).round() as u32;
            return if count == 0 { Vec::new() } else { vec![(item, count.min(u8::MAX as u32) as u8)] };
        }
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
            MobKind::Allay
            | MobKind::CopperGolem
            | MobKind::Creaking
            | MobKind::Bat
            | MobKind::Villager
            | MobKind::IronGolem
            | MobKind::SnowGolem
            | MobKind::WanderingTrader => 0,
            MobKind::Blaze | MobKind::Guardian | MobKind::ElderGuardian | MobKind::Breeze => 10,
            MobKind::Endermite => super::endermite::EXPERIENCE,
            MobKind::Shulker => super::shulker::EXPERIENCE,
            MobKind::Evoker => 10,
            MobKind::Vex => 3,
            MobKind::Ravager => 20,
            MobKind::Wither => super::wither::EXPERIENCE,
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
    /// Ticks its lead's holder has been missing (`restoreLeashFromSave`'s
    /// grace); never saved.
    pub(crate) leash_wait: u16,
    /// `LeashData.angularMomentum`, degrees a tick; never saved.
    pub(crate) leash_spin: f64,
    /// The lead is within its elastic reach this tick
    /// (`closeRangeLeashBehaviour`); never saved.
    pub(crate) leash_close: bool,
    pub animal: Option<Box<super::animals::State>>,
    pub bee: Option<super::bee::State>,
    pub villager: Option<Box<super::villager::Villager>>,
    /// Piglin gear, gold, anger and zombification (see `entity::nether`).
    pub mount: Option<Box<super::mounts::State>>,
    pub nether: Option<Box<super::nether::NetherMob>>,
    /// Identity other mobs aim at, assigned by `Entities` (0 until then).
    pub uid: u32,
    /// Saved with the level and never despawned (bastion residents, piglins
    /// that picked something up), like Java's persistence flag.
    pub persistent: bool,
    /// Java 26.3 `AgeableMob.AGE_LOCKED`: a golden dandelion stopped this
    /// baby growing up (v0.16 Track D). Saved.
    pub age_locked: bool,
    /// v1.0 gap Track 3b: `AgeableMob.forcedAge`, the growth a baby was
    /// forced (fed) through, given back as its age when it comes of age by
    /// feeding (saved when set), and `forcedAgeTimer`, the 40 ticks of
    /// happy-villager particles after a forced growth (not saved).
    pub forced_age: i32,
    pub(crate) forced_age_timer: u8,
    /// Forced-growth particles due since the last particle pass.
    pub(crate) forced_sparkles: u8,
    /// Java slime size (1, 2 or 4).
    pub size: u8,
    /// A baby zombie: half size and 50% faster.
    pub baby: bool,
    /// Seconds a baby turtle has grown (it grows up after Java's 20 minutes).
    pub grow: f32,
    pub wool_color: crate::color::DyeColor,
    pub sheared: bool,
    /// Java's `EntityEquipment` and `DropChances`: a stack and a drop chance
    /// per slot (a skeleton's bow, a drowned's trident, worn armor).
    pub equipment: super::equipment::Equipment,
    /// Breach's armor-effectiveness change for the hit being applied (0
    /// outside a player's mace hit); never saved.
    pub(super) armor_effectiveness: f32,
    /// The damage type of the hit being applied, when a caller knows it (an
    /// arrow or a blast); consumed by `take_damage`. Never saved.
    pub(crate) hit_cause: Option<Cause>,
    /// v1.0 Track 5: blocks fallen since the last landing (`fallDistance`,
    /// saved as `fall_distance` since v1.0 gap Track 1) and the fall damage
    /// the last moves queued, with its type (`fall` or `stalagmite`).
    pub(super) fall_distance: f64,
    pub(super) pending_fall: f32,
    pub(super) pending_fall_cause: Cause,
    pub phantom: Option<Box<super::phantom::State>>,
    pub freezing: Option<super::hostiles::Freezing>,
    pub mooshroom: Option<Box<super::mooshroom::State>>,
    pub bat: Option<Box<super::bat::State>>,
    pub frog: Option<Box<super::frog::State>>,
    pub polar_bear: Option<Box<super::polar_bear::State>>,
    pub panda: Option<Box<super::panda::State>>,
    pub armadillo: Option<Box<super::armadillo::State>>,
    pub ocelot: Option<Box<super::ocelot::State>>,
    /// v1.0 gap Track 2e: the fox's hunting states and goals.
    pub fox: Option<Box<super::fox::State>>,
    pub copper_golem: Option<Box<super::copper_golem::State>>,
    pub creaking: Option<Box<super::creaking::State>>,
    pub sniffer: Option<Box<super::sniffer::State>>,
    pub breeze: Option<Box<super::breeze::State>>,
    pub allay: Option<Box<super::allay::State>>,
    pub warden: Option<Box<super::warden::State>>,
    /// Endermites: lifetime and target (see `entity::endermite`).
    pub endermite: Option<Box<super::endermite::State>>,
    /// v0.16 Track A: a shulker's face, lid, colour and goals (see `entity::shulker`).
    pub shulker: Option<Box<super::shulker::State>>,
    /// v0.16 Track D: sulfur cube timers, fuse and goals (see `entity::sulfur_cube`).
    pub sulfur_cube: Option<Box<super::sulfur_cube::State>>,
    /// v0.16 Track E: a silverfish's merge and wake-up goals (see `entity::silverfish`).
    pub silverfish: Option<Box<super::silverfish::State>>,
    /// v0.15 Track A: illager, vex and ravager state, raid membership
    /// included (see `entity::illager`).
    pub illager: Option<Box<super::illager::State>>,
    /// `Mob.canPickUpLoot` (saved as `can_pick_up_loot`): items within reach
    /// are equipped under `mobGriefing` (see `entity::mob_combat`).
    pub can_pick_up_loot: bool,
    /// v0.15 Track C: happy ghast still timeout and upkeep (see `entity::happy_ghast`).
    pub happy_ghast: Option<Box<super::happy_ghast::State>>,
    /// v0.15 Track C: nautilus air, dash, anger and charge (see `entity::nautilus`).
    pub nautilus: Option<Box<super::nautilus::State>>,
    pub wolf_armor: Option<Box<super::wolf_armor::State>>,
    /// v0.16 Track C: the Wither's charge, heads and targets (see `entity::wither`).
    pub wither: Option<Box<super::wither::State>>,
    /// v0.16 Track C: seconds left in which a death counts as the Wither's
    /// kill (its skull or blast hurt this mob), growing a wither rose. Never saved.
    pub(crate) wither_credit: f32,
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
    /// v0.18 Track 6: [`Mob::pose_values`] at the start of the last
    /// simulation step, so the model turns and walks smoothly between ticks
    /// as Java's renderers do with the partial tick. Not saved.
    pub previous_pose: Pose,
    pub vel: DVec3,
    /// Body facing, same convention as the player: forward = (cos, 0, sin).
    /// v1.0 gap Track 2c: this is Java's `yRot`, the heading the move
    /// control turns and movement follows; the drawn body is [`Self::body_yaw`].
    pub yaw: f32,
    /// v1.0 gap Track 2c: Java's `yBodyRot`, which `BodyRotationControl`
    /// turns (the travel heading while moving, after the head while still);
    /// [`Self::pose_values`] hands it to the models as their body yaw. Not
    /// saved: it starts at [`Self::yaw`].
    pub body_yaw: f32,
    /// Head yaw relative to the body, and pitch (positive looks up).
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub health: f32,
    pub effects: crate::simulation::effects::Effects,
    /// v0.15 Track D: silverfish Infested rolled on hits this tick, spawned
    /// by `Entities::update_world` (`InfestedMobEffect.onMobHurt`). Not saved.
    pub(crate) infested: u8,
    /// Seconds of hurt flash left.
    pub hurt: f32,
    /// Seconds since death, while the death animation plays.
    pub dying: Option<f32>,
    pub on_ground: bool,
    pub in_water: bool,
    /// v1.0 gap Track 3b: the dimension's `FAST_LAVA` (the Nether), for
    /// lava currents, set from the update's context.
    pub(crate) fast_lava: bool,
    /// v1.0 gap Track 3b: the part of a land mob's motion (blocks a tick)
    /// that water and lava currents built up, kept apart from its walking
    /// in the continuous fluid model and dragged as Java's `travelInWater`
    /// and `travelInLava` drag the whole motion.
    pub(crate) current: DVec3,
    pub burning: bool,
    /// v1.0 gap Track 7: Java's `WalkAnimationState` position and speed
    /// ([`super::motion`]). While a model is built they hold what Java's
    /// renderer hands its model instead: `walkAnimationPos` (scaled for
    /// babies, interpolated) and `walkAnimationSpeed` (at most 1).
    pub limb_phase: f32,
    pub limb_amp: f32,
    /// Seconds left of the arm swing after an attack.
    pub attack_anim: f32,
    /// v1.0 gap Track 7: Java's `SwingState`, started when an attack sets
    /// [`Self::attack_anim`]. Not saved.
    pub swing: super::motion::Swing,
    /// [`Self::attack_anim`] as the last tick left it, to see a new attack.
    swing_seen: f32,
    /// v1.0 gap Track 7b: Java's `tickCount`, the ticks this mob has lived
    /// since it was spawned or loaded, which its model's idle animations
    /// read as `ageInTicks` ([`Self::age_in_ticks`]). Not saved, as in Java.
    pub tick_count: u32,
    /// v1.0 gap Track 7b: Java's client-side animation state
    /// ([`super::mob_anim`]). Not saved.
    pub anim: super::mob_anim::Anim,
    /// Sky light estimate at the mob, 0..1 (refreshed a few times a second).
    pub sky_light: f32,
    /// Torch light where it stands, 0..1.
    pub block_light: f32,
    /// Creeper fuse: seconds lit (Java's `swell`), 0 when not fusing. A
    /// fall adds to it (`Creeper.causeFallDamage`) without lighting it.
    pub fuse: f32,
    /// A creeper's `swellDir > 0`: its `SwellGoal` is running, so the fuse
    /// burns up rather than down. Not saved, as in Java.
    pub swelling: bool,
    /// Seconds a hit spider stays angry in daylight.
    pub(super) provoked: f32,
    pub(super) ai: Ai,
    pub(super) ai_timer: f32,
    pub(super) move_yaw: f32,
    pub(super) attack_cooldown: f32,
    burn_timer: f32,
    fire_left: f32,
    /// Java credits environmental deaths for five seconds after a player hit.
    pub(super) player_hit_left: f32,
    light_timer: f32,
    /// Blocked horizontally on the last physics step.
    pub(super) blocked: bool,
    /// Seconds left sidestepping around an obstacle while chasing, and
    /// which side (+1 / -1).
    detour: f32,
    detour_side: f64,
    /// Seconds until the next idle call.
    ambient_timer: f32,
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
    pub lightning_charged: bool,
    pub undead_mount: super::undead_mounts::State,
    /// Java ConversionTracker water exposure and conversion countdown, in ticks.
    pub water_conversion: super::lightning::WaterConversion,
    attack_step: u8,
    lift: bool,
    height_offset: f32,
    offset_timer: f32,
    /// Feet of whatever this mob is walking toward (a villager, monster, or player).
    pub(super) hunt: Option<DVec3>,
    /// The eye height of what `hunt` follows (Track 2d; 1 for a place).
    pub(super) hunt_eye: f64,
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
    /// A neutral mob angry with no persistent target after a player's hit
    /// under `universal_anger` (Java's `isAngryAtAllPlayers`; see `neutral`).
    pub(super) neutral_universal: bool,
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
    /// Java's `MoveControl` for swimmers, ghasts and airborne blazes.
    pub(crate) move_ctl: swim::MoveCtl,
    /// `Mob.homePosition` and `homeRadius` (`restrictTo`): random targets
    /// and attack targets stay within the radius. Elder guardians set one
    /// around where they first act; saved as Java saves it.
    pub(crate) restrict: Option<(IVec3, i32)>,
    /// The flagged goal selector and goal state of dolphins and turtles.
    pub(crate) java_goals: Option<Box<JavaGoals>>,
    /// Spawn-rolled attribute modifiers and reinforcement charges (see `entity::zombie`).
    pub attrs: super::zombie::Attrs,
    /// A hit since the last update asks for reinforcements: `Some(attacker)`
    /// when a player's melee is known. Never saved.
    pub(super) reinforce_call: Option<Option<super::PlayerId>>,
    /// The player a reinforcement was sent after (`setTarget`). Never saved.
    pub(super) reinforce_target: Option<super::PlayerId>,
    /// v1.0 gap Track 2: a running `BreakDoorGoal` (not saved).
    pub(crate) door_break: Option<door_break::DoorBreak>,
    /// The `mobGriefing` rule, refreshed before each update.
    pub(crate) griefing: bool,
    /// v1.0 gap Track 2: the shared chase's `TargetGoal` target and its
    /// unseen full updates, `MeleeAttackGoal`, the zombie goal selector,
    /// `SpearUseGoal` and a lowered spear (none saved, as in Java).
    pub(crate) chase_target: Option<super::PlayerId>,
    pub(crate) chase_unseen: i32,
    pub(crate) melee: chase::Melee,
    pub(crate) chase_goals: selector::Selector,
    pub(crate) spear: Option<chase::SpearUse>,
    pub(crate) spear_use: Option<chase::SpearCharge>,
    /// v1.0 gap Track 2c: Java's `yHeadRot` and `xRot` and the
    /// `LookControl` that turns them, for every mob (never saved).
    pub(crate) look: head::JavaLook,
    /// v1.0 gap Track 2c: the idle look goals, brain look behaviours and
    /// `BodyRotationControl` (never saved).
    pub(crate) head: head::Head,
    /// v1.0 gap Track 2e: a brain's `WALK_TARGET` and idle movement.
    pub(crate) walking: brain_walk::Walking,
    /// A mount carrying a rider's spear charge: `hunt` at its
    /// `chargeSpeedModifier` (1.4 for zombie horses, 4 for camel husks).
    pub(crate) hunt_charge: bool,
    /// A strider a zombified piglin rider steers (never saved; refreshed
    /// every update).
    pub(crate) mob_steered: bool,
    /// Ticks since it was last hurt (`HURT_BY` lasts 40), and where a baby
    /// piglin walks to mount (`Mount`); never saved.
    pub(crate) hurt_age: u32,
    /// v1.0 gap Track 8d: `LivingEntity.damageCooldownTime` (Java's old
    /// `invulnerableTime`), in ticks: 20 after a hit that lands in full,
    /// counted down once a tick; above 10, a hit lands only by what it adds
    /// over `last_hurt`. Never saved (Java saves neither).
    pub(crate) damage_cooldown: u8,
    /// `LivingEntity.lastHurt`: the amount of the last hit that landed,
    /// before armor and the rest of `actuallyHurt`.
    pub(crate) last_hurt: f32,
    pub(crate) ride_walk: Option<DVec3>,
    /// v1.0 Track 9: how many times a slime block or bed has bounced it
    /// (`restituteMovementAfterCollisions`), for the `bounce` game event.
    /// Never saved.
    pub(crate) bounces: u32,
}

/// Per-kind state of the mobs on the flagged goal selector. Mobs hold it
/// boxed, so the variants' sizes don't matter.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum JavaGoals {
    Dolphin(dolphin::Goals),
    Turtle(turtle::Goals),
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
            infested: 0,
            mount,
            kind,
            name: Default::default(),
            leash: None,
            leash_pos: None,
            leash_wait: 0,
            leash_spin: 0.0,
            leash_close: false,
            animal: kind.has_animal_state().then(|| Box::new(super::animals::State::new(pos))),
            villager: matches!(kind, MobKind::Villager | MobKind::ZombieVillager | MobKind::WanderingTrader)
                .then(|| Box::new(super::villager::Villager::new(0, 0))),
            nether: super::nether::NetherMob::for_kind(kind),
            aquatic: kind.is_aquatic().then(|| Box::new(super::aquatic::State::new())),
            uid: 0,
            persistent: super::mounts::equine(kind) && kind != MobKind::CamelHusk
                || matches!(kind, MobKind::Tadpole | MobKind::CopperGolem | MobKind::Allay),
            age_locked: false,
            forced_age: 0,
            forced_age_timer: 0,
            forced_sparkles: 0,
            size: 1,
            baby: false,
            grow: 0.0,
            wool_color: crate::color::DyeColor::White,
            sheared: false,
            freezing: None,
            equipment: {
                // `AbstractSkeleton.populateDefaultEquipmentSlots` always gives a bow.
                let mut e = super::equipment::Equipment::default();
                if kind.is_bow_skeleton() {
                    e.set(super::equipment::Slot::MainHand, Some(crate::inventory::Stack::new(Item::BOW, 1)));
                }
                e
            },
            armor_effectiveness: 0.0,
            hit_cause: None,
            fall_distance: 0.0,
            pending_fall: 0.0,
            pending_fall_cause: Cause::FALL,
            phantom: (kind == MobKind::Phantom).then(|| Box::new(super::phantom::State::new(pos))),
            mooshroom: (kind == MobKind::Mooshroom).then(Box::default),
            bat: (kind == MobKind::Bat).then(Box::default),
            frog: matches!(kind, MobKind::Frog | MobKind::Tadpole).then(Box::default),
            polar_bear: (kind == MobKind::PolarBear).then(Box::default),
            panda: (kind == MobKind::Panda).then(Box::default),
            armadillo: (kind == MobKind::Armadillo).then(Box::default),
            ocelot: (kind == MobKind::Ocelot).then(Box::default),
            fox: (kind == MobKind::Fox).then(Box::default),
            copper_golem: (kind == MobKind::CopperGolem).then(Box::default),
            creaking: (kind == MobKind::Creaking).then(Box::default),
            sniffer: (kind == MobKind::Sniffer).then(Box::default),
            breeze: (kind == MobKind::Breeze)
                .then(|| Box::new(super::breeze::State { since_hurt: 1000, ..Default::default() })),
            allay: (kind == MobKind::Allay).then(Box::default),
            warden: (kind == MobKind::Warden).then(Box::default),
            endermite: (kind == MobKind::Endermite).then(Box::default),
            shulker: (kind == MobKind::Shulker).then(Box::default),
            sulfur_cube: (kind == MobKind::SulfurCube).then(Box::default),
            silverfish: (kind == MobKind::Silverfish).then(Box::default),
            illager: super::illager::State::for_kind(kind),
            can_pick_up_loot: false,
            happy_ghast: (kind == MobKind::HappyGhast).then(Box::default),
            nautilus: super::nautilus::is_nautilus(kind).then(Box::default),
            wolf_armor: (kind == MobKind::Wolf).then(Box::default),
            wither: (kind == MobKind::Wither).then(Box::default),
            wither_credit: 0.0,
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
            previous_pose: [yaw, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            vel: DVec3::ZERO,
            yaw,
            body_yaw: yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            health,
            effects: Default::default(),
            hurt: 0.0,
            dying: None,
            on_ground: false,
            in_water: false,
            fast_lava: false,
            current: DVec3::ZERO,
            burning: false,
            limb_phase: 0.0,
            limb_amp: 0.0,
            attack_anim: 0.0,
            swing: Default::default(),
            swing_seen: 0.0,
            tick_count: 0,
            anim: Default::default(),
            sky_light: 1.0,
            block_light: 0.0,
            ai: Ai::Idle,
            ai_timer: 1.0,
            move_yaw: yaw,
            attack_cooldown: 0.0,
            burn_timer: 0.0,
            fire_left: 0.0,
            player_hit_left: 0.0,
            light_timer: 0.0,
            fuse: 0.0,
            swelling: false,
            bee: (kind == MobKind::Bee).then(super::bee::State::default),
            provoked: 0.0,
            blocked: false,
            detour: 0.0,
            detour_side: 1.0,
            ambient_timer: 2.0 + (yaw * 1000.0).rem_euclid(10.0),
            cry: None,
            teleport_pending: false,
            stare: 0.0,
            teleport_timer: 0.0,
            since_anger: 1e3,
            charged: false,
            lightning_charged: false,
            undead_mount: Default::default(),
            water_conversion: Default::default(),
            attack_step: 0,
            lift: false,
            height_offset: 0.5,
            offset_timer: 0.0,
            hunt: None,
            hunt_eye: 1.0,
            strike: false,
            built: false,
            weakness_left: 0.0,
            convert_left: 0.0,
            convert_tick: 0.0,
            trader_night: false,
            convert_by: None,
            angry_player: None,
            neutral_universal: false,
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
            move_ctl: Default::default(),
            restrict: None,
            java_goals: match kind {
                MobKind::Dolphin => Some(Box::new(JavaGoals::Dolphin(Default::default()))),
                MobKind::Turtle => Some(Box::new(JavaGoals::Turtle(Default::default()))),
                _ => None,
            },
            attrs: Default::default(),
            reinforce_call: None,
            reinforce_target: None,
            door_break: None,
            griefing: true,
            chase_target: None,
            chase_unseen: 0,
            melee: Default::default(),
            chase_goals: Default::default(),
            spear: None,
            spear_use: None,
            look: Default::default(),
            head: Default::default(),
            walking: Default::default(),
            hunt_charge: false,
            mob_steered: false,
            hurt_age: u32::MAX,
            damage_cooldown: 0,
            last_hurt: 0.0,
            ride_walk: None,
            bounces: 0,
        }
    }

    pub(super) fn melee_attack(&self) -> (f32, Cause) {
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
        // `Mob.doHurtTarget` reads the attack damage attribute, which the
        // main-hand item's modifiers raise (`entity::mob_combat`).
        let weapon = if self.kind == MobKind::ZombifiedPiglin { 0.0 } else { self.weapon_attack_bonus() };
        ((base + strength + held + weapon - weakness).max(0.0), cause)
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
                if self.damage_bypassing_wolf_armor(back, Some(-knockback * 0.5 + DVec3::Y * 3.0), rng) {
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
    /// Returns the preserved equipment that drops (`ZombieVillager.finishConversion`).
    pub(super) fn finish_cure(&mut self) -> Vec<crate::inventory::Stack> {
        self.kind = MobKind::Villager;
        self.health = MobKind::Villager.max_health();
        self.baby = self.age < 0;
        self.weakness_left = 0.0;
        self.convert_left = 0.0;
        self.convert_tick = 0.0;
        // `ConversionParams.single(this, false, false)`: equipment is not
        // kept. `dropPreservedEquipment` drops preserved slots, except items
        // with Curse of Binding (`prevent_armor_change`), which go to the
        // villager's inventory at the slot's index.
        let mut old = std::mem::take(&mut self.equipment);
        let mut kept = Vec::new();
        for slot in super::equipment::Slot::ALL {
            if old.get(slot).is_some_and(|s| s.enchants.has(crate::enchant::Enchantment::BindingCurse)) {
                kept.extend(old.take(slot).map(|s| (slot.index(), s)));
            }
        }
        let dropped = old.take_preserved();
        if let Some(v) = &mut self.villager {
            for (index, stack) in kept {
                v.food[index] = Some(stack);
            }
        }
        if let Some(v) = &mut self.villager {
            v.reputation = 0;
            // `ZOMBIE_VILLAGER_CURED`: major positive 20 and minor positive
            // 25; their caps keep repeat cures from stacking.
            if let Some(owner) = self.convert_by {
                v.gossip.add(owner, super::villager_gossip::MAJOR_POSITIVE, 20);
                v.gossip.add(owner, super::villager_gossip::MINOR_POSITIVE, 25);
            }
            self.convert_by = None;
            v.sleeping = false;
            v.trading = false;
            v.customer = None;
            v.fleeing = false;
            v.goal = None;
        }
        // `finishConversion`: the villager wakes with 200 ticks of Nausea.
        self.effects.add(crate::simulation::effects::Effect::Nausea, 0, 200);
        dropped
    }

    pub fn shape(&self) -> Shape {
        // `Warden.getDefaultDimensions`: one block tall while digging or emerging.
        if self.warden.as_ref().is_some_and(|s| s.digging_or_emerging()) {
            return Shape::new(0.45, 1.0);
        }
        if self.sniffer.as_ref().is_some_and(|s| s.pose == super::sniffer::DIGGING) {
            let scale = if self.baby || self.age < 0 { 0.5 } else { 1.0 };
            return Shape::new(0.95 * scale, f64::from(1.75_f32 - 0.4_f32) * scale);
        }
        if let Some(p) = &self.phantom {
            let s = 1. + f64::from(p.size) * 0.15;
            Shape::new(0.45 * s, 0.5 * s)
        } else if self.kind == MobKind::SulfurCube {
            let base = super::sulfur_cube::BASE_SIZE * f64::from(self.size);
            Shape::new(base * 0.5, base)
        } else if self.kind.is_cube() {
            Shape::new(0.255 * self.size as f64, 0.51 * self.size as f64)
        } else if let Some(s) = self.mount.as_ref().filter(|_| super::mounts::camel_like(self.kind)) {
            // v1.0 gap Track 3: `Camel.getDefaultDimensions` by pose and age.
            let (width, height) = super::mounts::camel::dimensions(self.baby || self.age < 0, s.camel_sitting);
            Shape::new(width / 2.0, height)
        } else {
            let shape = if self.baby || self.age < 0 { baby_shape(self.kind) } else { self.kind.shape() };
            // v1.0 gap Track 2f: `Goat.getDefaultDimensions` scales its box by
            // `LONG_JUMPING_DIMENSION_SCALE_FACTOR` (0.7) in the
            // `LONG_JUMPING` pose.
            if self.goat_long_jumping() { Shape::new(shape.half_width * 0.7, shape.height * 0.7) } else { shape }
        }
    }

    /// v1.0 gap Track 2f: a goat in Java's `Pose.LONG_JUMPING`
    /// (`LongJumpMidJump`: from its launch until it lands), for the box
    /// and the model.
    pub fn goat_long_jumping(&self) -> bool {
        self.kind == MobKind::Goat && self.animal.as_ref().is_some_and(|a| a.long_jumping)
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
        self.provoked > 0.0 || self.bee.is_some_and(|s| s.anger > 0)
    }

    /// Turns a neutral mob hostile for `secs`.
    pub fn anger(&mut self, secs: f32) {
        self.provoked = self.provoked.max(secs);
    }

    /// Takes a hit. `knockback` replaces the horizontal velocity (its y is
    /// the upward pop). Returns `true` if this killed the mob.
    pub fn damage(&mut self, amount: f32, knockback: Option<DVec3>, rng: &mut Rng) -> bool {
        self.damage_typed(amount, knockback, false, rng)
    }
    /// Damage tagged #bypasses_wolf_armor (magic, wither, drowning, freezing,
    /// suffocation, cramming, starvation, world border and invulnerability bypass).
    pub fn damage_bypassing_wolf_armor(&mut self, amount: f32, knockback: Option<DVec3>, rng: &mut Rng) -> bool {
        self.damage_typed(amount, knockback, true, rng)
    }
    /// Whether an ordinary hit of `amount` is refused outright: the mob is
    /// dead, a skeleton trap's `invulnerableTime` is running, or a rolled-up
    /// armadillo shrugs off a hit of 1 or less. Arrows apply their potion
    /// and Flame only to hits `hurtOrSimulate` accepts.
    pub fn refuses_hit(&self, amount: f32) -> bool {
        // v0.16 Track C: `WitherBoss.hurtServer` refuses everything while it
        // charges, and arrows (`AbstractArrow`, tridents too) once its armor
        // is up at half health.
        if self.wither.as_ref().is_some_and(|s| s.invul > 0)
            || super::wither::powered(self)
                && self.hit_cause.is_some_and(|c| {
                    matches!(
                        c.kind,
                        crate::simulation::damage::DamageType::Arrow | crate::simulation::damage::DamageType::Trident
                    )
                })
        {
            return true;
        }
        !self.alive()
            || self.warden.as_ref().is_some_and(|s| s.digging_or_emerging())
            || self.undead_mount.protection > 0.
            || self.armadillo.as_ref().is_some_and(|s| s.scared() && amount <= 1.0)
            // `primeTime` makes a lit sulfur cube permanently invulnerable.
            || self.sulfur_cube.as_ref().is_some_and(|s| s.primed())
            || self.cooldown_refuses(amount)
    }

    /// v1.0 gap Track 8d: `LivingEntity.hurtServer`'s hurt cooldown refusing
    /// a hit: within the first half of `damageCooldownTime` (above 10 of
    /// its 20 ticks), a hit no harder than `lastHurt` does nothing at all
    /// (`#bypasses_cooldown` is empty in 26.3).
    pub fn cooldown_refuses(&self, amount: f32) -> bool {
        // `Armadillo.hurtServer` halves a scared one's hit before the check.
        let amount =
            if self.armadillo.as_ref().is_some_and(|s| s.scared()) { ((amount - 1.0) / 2.0).max(0.0) } else { amount };
        self.damage_cooldown > COOLDOWN_HALF && amount <= self.last_hurt
    }

    /// Whether a hit now would land in full (`tookFullDamage`): outside the
    /// first half of the hurt cooldown. Only a full hit flashes, plays the
    /// hurt or death sound, kicks the legs and takes `hurtServer`'s own
    /// knockback (`dealDefaultKnockback`).
    pub fn full_hit(&self) -> bool {
        self.damage_cooldown <= COOLDOWN_HALF
    }
    fn damage_typed(&mut self, amount: f32, knockback: Option<DVec3>, bypass_wolf_armor: bool, rng: &mut Rng) -> bool {
        // `isInvulnerableTo` refuses the hit before any knockback.
        if self.refuses_hit(amount) {
            return false;
        }
        // Full resistance (iron golems) cancels the push and leaves the motion alone, as `knockback` returns early.
        let taken = if self.creaking.as_ref().is_some_and(|s| s.home.is_some() || !s.can_move) {
            0.0
        } else {
            1.0 - self.knockback_resistance()
        };
        // v1.0 gap Track 8d: a hit inside the hurt cooldown takes no knockback.
        if let Some(kb) = knockback.filter(|_| taken > 0.0 && self.full_hit()).map(|kb| kb * taken) {
            self.vel.x = kb.x;
            self.vel.z = kb.z;
            self.vel.y = self.vel.y.max(kb.y);
        }
        // A sulfur cube with a block takes a mob's hit only as knockback.
        if knockback.is_some() && self.sulfur_shrugs_off(crate::simulation::damage::DamageType::MobAttack) {
            return false;
        }
        self.take_damage(amount, knockback.is_some(), bypass_wolf_armor, rng)
    }

    /// Explosion damage, which carries no hit knockback (`#no_knockback`),
    /// then `Entity.pushFromExplosion` adds `push` (blocks per tick) to the
    /// motion. Returns `true` if this killed the mob.
    pub fn blast(&mut self, amount: f32, push: DVec3, rng: &mut Rng) -> bool {
        // `Warden.ignoreExplosion` while digging or emerging, and
        // `WitherBoss.hurtServer` refusing everything while it charges.
        if !self.alive()
            || self.warden.as_ref().is_some_and(|s| s.digging_or_emerging())
            || self.wither.as_ref().is_some_and(|s| s.invul > 0)
        {
            return false;
        }
        if self.sulfur_shrugs_off(crate::simulation::damage::DamageType::Explosion) {
            self.vel += push * crate::knockback::TICK_RATE;
            return false;
        }
        self.hit_cause = Some(Cause::TNT);
        let killed = self.take_damage(amount, true, false, rng);
        self.vel += push * crate::knockback::TICK_RATE;
        killed
    }

    /// Takes a hit followed by Java's knockbacks in order (`hurtServer`'s
    /// default push, then the attacker's extra one), each halving the motion
    /// left by the previous. Returns `true` if this killed the mob.
    ///
    /// The first push is `hurtServer`'s own (`dealDefaultKnockback`), which a
    /// hit inside the hurt cooldown doesn't take (v1.0 gap Track 8d).
    pub fn damage_pushed(&mut self, amount: f32, pushes: &[crate::knockback::Push], rng: &mut Rng) -> bool {
        let (default, extra) = match pushes.split_first() {
            Some((first, rest)) => (Some(*first), rest),
            None => (None, pushes),
        };
        self.damage_knocked(amount, default, extra, rng)
    }

    /// [`Mob::damage_pushed`] with `hurtServer`'s own push (`default`, taken
    /// only by a full hit) apart from the attacker's (`extra`: the attack's
    /// knockback and sweep, which any hit that lands takes).
    pub fn damage_knocked(
        &mut self,
        amount: f32,
        default: Option<crate::knockback::Push>,
        extra: &[crate::knockback::Push],
        rng: &mut Rng,
    ) -> bool {
        // `isInvulnerableTo` refuses the hit before any knockback.
        if self.refuses_hit(amount) {
            return false;
        }
        let default = default.filter(|_| self.full_hit());
        for push in default.into_iter().chain(extra.iter().copied()) {
            self.knock_back(push, rng);
        }
        // A sulfur cube with a block takes melee and projectile hits only
        // as knockback (`#sulfur_cube_with_block_immune_to`).
        let kind = self.hit_cause.map_or(crate::simulation::damage::DamageType::PlayerAttack, |c| c.kind);
        if self.sulfur_shrugs_off(kind) {
            self.hit_cause = None;
            return false;
        }
        self.take_damage(amount, true, false, rng)
    }

    /// [`Mob::damage_pushed`] for a projectile's hit: armor and Projectile
    /// Protection see an arrow-type hit.
    pub fn damage_projectile(&mut self, amount: f32, pushes: &[crate::knockback::Push], rng: &mut Rng) -> bool {
        // `Shulker.hurtServer`: a shut shulker shrugs off arrows.
        if self.shulker.as_ref().is_some_and(|s| s.closed()) {
            return false;
        }
        self.hit_cause = Some(Cause::ARROW);
        let killed = self.damage_pushed(amount, pushes, rng);
        self.hit_cause = None;
        killed
    }

    /// Punch's shove of an arrow with `motion` (blocks per tick), added to the mob's velocity.
    pub fn shove_punch(&mut self, level: u32, motion: DVec3) {
        if let Some(shove) = crate::knockback::arrow_push(level, self.knockback_resistance(), motion) {
            self.vel += shove * crate::knockback::TICK_RATE;
        }
    }

    /// Java's `LivingEntity.knockback`, applied to this mob's velocity in blocks per second.
    pub fn knock_back(&mut self, push: crate::knockback::Push, rng: &mut Rng) {
        if self.creaking.as_ref().is_some_and(|s| s.home.is_some() || !s.can_move) {
            return;
        }
        use crate::knockback::{TICK_RATE, apply};
        let mut jitter = || f64::from(rng.next_f32());
        if let Some(motion) =
            apply(self.vel / TICK_RATE, push, self.knockback_resistance(), self.on_ground, &mut jitter)
        {
            self.vel = motion * TICK_RATE;
        }
    }

    fn take_damage(&mut self, amount: f32, attacked: bool, bypass_wolf_armor: bool, rng: &mut Rng) -> bool {
        // A trap's `setInvulnerableTime(60)` sets `Entity.invulnerableTime`
        // (not the hurt cooldown): `isInvulnerableToBase` refuses ordinary
        // damage until it runs out.
        if let Some(s) = self.creaking.as_mut().filter(|s| s.home.is_some()) {
            if attacked && s.hit_cooldown <= 0.0 {
                s.hit = self.player_hit_left > 0.0;
                s.hit_cooldown = 0.4;
                self.hurt = 0.4;
                self.cry = Some(MobSound::Hurt(self.kind));
                return false;
            }
            return false;
        }
        if self.undead_mount.protection > 0. {
            return false;
        }
        let amount =
            if self.armadillo.as_ref().is_some_and(|s| s.scared()) { ((amount - 1.0) / 2.0).max(0.0) } else { amount };
        if self.armadillo.is_some() && amount <= 0.0 {
            return false;
        }
        // v1.0 gap Track 8d: `LivingEntity.hurtServer`'s hurt cooldown, on
        // the amount `Armadillo.hurtServer` passes on, before armor
        // (`actuallyHurt` reduces only what lands).
        let full = self.full_hit();
        let amount = if full {
            self.last_hurt = amount;
            self.damage_cooldown = COOLDOWN;
            amount
        } else {
            if amount <= self.last_hurt {
                self.hit_cause = None;
                return false;
            }
            let excess = amount - self.last_hurt;
            self.last_hurt = amount;
            excess
        };
        // v0.16 Track A: a shut shulker's shell is 20 armor; the hit
        // decides on its teleport and duplication next tick.
        let amount = match &mut self.shulker {
            Some(s) => {
                s.hurt = true;
                crate::simulation::survival::armor_reduce_with(amount, s.armor(), 0.0, self.armor_effectiveness)
            }
            None => amount,
        };
        let amount = if self.kind == MobKind::MagmaCube {
            crate::simulation::survival::armor_reduce_with(amount, self.size as u32 * 3, 0.0, self.armor_effectiveness)
        } else if self.kind == MobKind::Wither && !bypass_wolf_armor {
            // `WitherBoss.createAttributes`: 4 armor points.
            crate::simulation::survival::armor_reduce_with(amount, super::wither::ARMOR, 0.0, self.armor_effectiveness)
        } else {
            amount
        };
        if let Some(s) = &mut self.wither {
            s.hurt();
        }
        // v0.16 Track E: `Silverfish.hurtServer` wakes its friends when an
        // entity's hit lands.
        if attacked && let Some(s) = &mut self.silverfish {
            s.notify_hurt();
        }
        // `Camel.actuallyHurt` stands it up at once (v1.0 gap Track 3).
        if super::mounts::camel_like(self.kind)
            && let Some(s) = self.mount.as_mut()
        {
            s.stand_up_instantly();
        }
        if let Some(a) = &mut self.animal {
            a.love = 0.0;
            a.courtship = 0.0;
            // `Wolf` and `Parrot.hurtServer` cancel the order to sit; a cat's
            // doesn't.
            if matches!(self.kind, MobKind::Wolf | MobKind::Parrot) {
                a.sitting = false;
            }
        }
        let amount = if bypass_wolf_armor { amount } else { self.absorb_wolf_armor(amount, rng) };
        // Horse armor's BODY attributes: armor points and toughness.
        let body = self.mount.as_ref().and_then(|s| s.slots[1]).map(|s| s.item);
        let armor = body.map_or(0, super::mounts::armor_points);
        let bypasses_armor = self.hit_cause.is_some_and(|c| c.is(crate::simulation::damage::DamageTag::BypassesArmor));
        let amount = if armor > 0 && !bypasses_armor {
            let toughness = body.map_or(0.0, super::mounts::armor_toughness);
            crate::simulation::survival::armor_reduce_with(amount, armor, toughness, self.armor_effectiveness)
        } else {
            amount
        };
        // Worn humanoid armor (`getDamageAfterArmorAbsorb`), then
        // Resistance, then the armor's protection enchantments
        // (`getDamageAfterMagicAbsorb`); only hits from an attack or a blast
        // carry a damage type here (`entity::mob_combat`).
        let cause = self.hit_cause.take().or(attacked.then_some(super::mob_combat::ATTACK_CAUSE));
        let amount = match cause {
            Some(cause) if !bypass_wolf_armor => super::mob_combat::after_worn_armor(self, amount, cause),
            _ => amount,
        };
        let amount = amount * self.effects.resistance_factor();
        let amount = match cause {
            Some(cause) if !bypass_wolf_armor => crate::enchant::protect(amount, &self.worn_stacks(), cause),
            _ => amount,
        };
        self.health -= self.effects.absorb(amount);
        // v1.0 gap Track 3b: `AbstractHorse.hurtServer` rears one time in
        // three after a hit that hurt (`standIfPossible`).
        if super::mounts::rears(self.kind)
            && let Some(s) = self.mount.as_mut()
            && rng.next_int(3) == 0
        {
            s.stand_if_possible(self.kind);
        }
        // `InfestedMobEffect.onMobHurt`, after a hit that didn't kill (a
        // death clears the effects first).
        if self.health > 0.0
            && self.effects.has(crate::simulation::effects::Effect::Infested)
            && rng.next_f32() <= crate::simulation::effects::INFESTED_CHANCE
        {
            let (lo, hi) = crate::simulation::effects::INFESTED_SILVERFISH;
            self.infested = self.infested.saturating_add((lo + rng.next_int(hi - lo + 1)) as u8);
        }
        // `Zombie.hurtServer` calls reinforcements after any accepted hit.
        if super::zombie::calls_reinforcements(self.kind) && self.reinforce_call.is_none() {
            self.reinforce_call = Some(None);
        }
        if self.kind == MobKind::Axolotl && self.in_water && self.health > 0.0 && self.health < 7.0 && rng.chance(0.33)
        {
            self.aquatic.as_mut().unwrap().play_dead = 10.0;
        }
        if let Some(s) = &mut self.copper_golem {
            s.interrupt();
        }
        // Only a full hit sets `hurtTime` and sends the damage event
        // (`handleDamageEvent` kicks the legs); any hit that lands is
        // remembered (`setLastHurtByMob`).
        if full {
            self.hurt = HURT_TIME;
            self.limb_amp = super::motion::HURT_KICK;
        }
        self.hurt_age = 0;
        if let Some(s) = &mut self.bat {
            s.resting = false;
        }
        self.idle = 0.0;
        // Java's panic goals react to any `PANIC_CAUSES` damage, except wolves
        // (`PANIC_ENVIRONMENTAL_CAUSES` only: an attack doesn't scare them)
        // and golems, which have none.
        if let Some(s) = &mut self.panda {
            s.sitting = false;
        }
        let scared = match self.kind {
            // Squid flee their attacker instead; dolphins and axolotls
            // have no panic goal.
            MobKind::IronGolem
            | MobKind::SnowGolem
            | MobKind::Squid
            | MobKind::GlowSquid
            | MobKind::Dolphin
            | MobKind::Axolotl => false,
            // `ZombieNautilusAi` has no `AnimalPanic`; happy ghast adults have
            // no panic goal (`HappyGhastAi`'s is the ghastling's).
            MobKind::Ocelot | MobKind::ZombieNautilus => false,
            MobKind::HappyGhast => self.baby,
            MobKind::Wolf => !attacked,
            MobKind::PolarBear => self.baby || !attacked,
            MobKind::Panda | MobKind::Armadillo => !attacked,
            // v1.0 gap Track 2f: `FoxPanicGoal.shouldPanic`: not while defending.
            MobKind::Fox => !self.fox.as_ref().is_some_and(|s| s.defending),
            k => !k.is_hostile(),
        };
        if scared {
            if let Some(s) = &mut self.frog {
                s.tongue = None;
                s.pose = 0;
                s.jump_prepare = 0;
                s.jump_velocity = None;
            }
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
            // A killing hit inside the hurt cooldown dies silently.
            if full {
                self.cry = Some(if let Some(s) = &self.copper_golem {
                    MobSound::CopperGolem(super::copper_golem::Sound::Death, s.stage as u8)
                } else {
                    MobSound::Death(self.kind)
                });
            }
            return true;
        }
        if !full {
            return false;
        }
        self.cry = Some(if let Some(s) = &self.copper_golem {
            MobSound::CopperGolem(super::copper_golem::Sound::Hurt, s.stage as u8)
        } else {
            MobSound::Hurt(self.kind)
        });
        false
    }

    /// Sets it alight for at least `secs` (fire aspect, flame arrows).
    pub fn ignite(&mut self, secs: f32) {
        if self.creaking.as_ref().is_some_and(|s| s.home.is_some()) {
            return;
        }
        if !self.kind.fire_immune() && !self.in_water {
            self.fire_left = self.fire_left.max(secs);
            self.burning = self.alive() && self.fire_left > 0.0;
        }
    }

    pub(super) fn player_hit(&mut self) {
        self.player_hit_left = 5.0;
    }

    /// [`Self::player_hit`] by `owner`, melee or projectile: a raider
    /// remembers its player attacker so the raid can credit them as a hero.
    pub(super) fn player_hit_by(&mut self, owner: super::PlayerId) {
        self.player_hit();
        if let Some(s) = &mut self.illager {
            s.raider.last_player = Some(owner);
        }
    }

    /// v0.18 Track 6: what the model's pose is drawn from: body yaw, head
    /// yaw and pitch, walk position and speed, and (v1.0 gap Track 7) the
    /// arm swing and the attack countdown.
    pub fn pose_values(&self) -> Pose {
        let body = if self.look.ready() { self.body_yaw } else { self.yaw };
        [body, self.head_yaw, self.head_pitch, self.limb_phase, self.limb_amp, self.swing.anim, self.attack_anim]
    }

    pub fn set_pose_values(&mut self, [yaw, head_yaw, head_pitch, limb_phase, limb_amp, swing, attack]: Pose) {
        (self.yaw, self.body_yaw, self.head_yaw, self.head_pitch, self.limb_phase, self.limb_amp) =
            (yaw, yaw, head_yaw, head_pitch, limb_phase, limb_amp);
        (self.swing.anim, self.attack_anim) = (swing, attack);
    }

    /// Whether Java draws this mob as a baby (`isBaby`).
    pub fn is_baby(&self) -> bool {
        self.baby || self.age < 0
    }

    /// The pose `alpha` of the way from the last step's start to now, as
    /// Java's renderers interpolate it: angles the short way round
    /// (`Mth.rotLerp`); the walk as `WalkAnimationState.position(partial)`
    /// and `speed(partial)` (scaled three times for babies, capped at 1,
    /// and still while dead or riding); the swing as
    /// `SwingState.getAnimation`; and the attack countdown less the
    /// partial tick, as the iron golem's and ravager's renderers do.
    pub fn render_pose(&self, alpha: f32) -> Pose {
        use super::motion;
        let (from, to) = (self.previous_pose, self.pose_values());
        let angle = |a: f32, b: f32| a + ((b - a + PI).rem_euclid(TAU) - PI) * alpha;
        let scale = if self.is_baby() { motion::BABY_WALK_SCALE } else { 1.0 };
        let (walk_pos, walk_speed) = if self.alive() && self.riding.is_none() {
            motion::walk_render(to[3], from[4], to[4], alpha, scale)
        } else {
            (0.0, 0.0)
        };
        [
            angle(from[0], to[0]),
            angle(from[1], to[1]),
            angle(from[2], to[2]),
            walk_pos,
            walk_speed,
            motion::swing_render(from[5], to[5], alpha),
            (to[6] - alpha * motion::TICK).max(0.0),
        ]
    }

    /// v1.0 gap Track 7b: Java's `ageInTicks`, `tickCount` plus the
    /// partial tick, the clock this mob's idle animations run on (each mob
    /// on its own, not in step with the others).
    pub fn age_in_ticks(&self, alpha: f32) -> f32 {
        // A float, as Java's is.
        self.tick_count as f32 + alpha
    }

    /// v1.0 gap Track 7: `deathTime` with the partial tick, as
    /// `LivingEntityRenderer.extractRenderState` reads it (0 alive, and in
    /// the tick the mob died).
    pub fn death_ticks(&self, alpha: f32) -> f32 {
        match self.dying {
            Some(t) if t > 0.0 => t / super::motion::TICK + alpha,
            _ => 0.0,
        }
    }

    /// One tick of the walk cycle after the mob moved by `moved`:
    /// `LivingEntity.calculateEntityAnimation`, which stops it while dead
    /// or riding.
    pub(crate) fn advance_walk(&mut self, moved: DVec3) {
        use super::motion;
        // v1.0 gap Track 7b: `tickCount` counts every tick, alive or not,
        // and the client animations follow the tick.
        self.tick_count = self.tick_count.wrapping_add(1);
        self.tick_anim(moved);
        if !self.alive() || self.riding.is_some() {
            (self.limb_phase, self.limb_amp) = (0.0, 0.0);
            return;
        }
        let rule = motion::walk_rule(self.kind);
        let carrying = self.sulfur_body().is_some();
        let y = if rule.with_y || carrying { moved.y } else { 0.0 };
        let distance = DVec3::new(moved.x, y, moved.z).length() as f32;
        motion::walk_step(&mut self.limb_phase, &mut self.limb_amp, distance, rule);
    }

    /// Starts Java's arm swing when an attack set [`Self::attack_anim`]
    /// since the last tick, then advances it a tick
    /// (`LivingEntity.swing` and `SwingState.tick`).
    pub(crate) fn advance_swing(&mut self) {
        if self.attack_anim > self.swing_seen {
            self.swing.start_if_able(super::motion::SWING_TICKS);
        }
        self.swing.tick();
        self.swing_seen = self.attack_anim;
    }

    /// v0.18 Track 6: `--still-mobs`. Holds a landed mob in place facing
    /// `face`, its limbs and head at rest and its lighting current.
    pub fn hold_still<W: MobWorld + ?Sized>(&mut self, world: &W, face: DVec3) {
        let to = face - self.pos;
        self.yaw = to.z.atan2(to.x) as f32;
        (self.head_yaw, self.head_pitch, self.limb_amp, self.vel) = (0.0, 0.0, 0.0, DVec3::ZERO);
        (self.burning, self.hurt) = (false, 0.0);
        let at = self.pos + DVec3::new(0.0, self.shape().height * 0.6, 0.0);
        self.sky_light = sky_light(world, at);
        self.block_light = world.block_light(at.floor().as_ivec3()) as f32 / 15.0;
    }

    pub fn update<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let event_start = events.len();
        let tick_start = self.pos;
        self.fast_lava = ctx.dimension == crate::world::terrain::Dimension::Nether;
        self.advance_swing();
        let before = self.kind;
        self.tick_water_conversion(dt, world);
        if self.kind != before {
            self.drowning_converted(before, rng);
        }
        let dtf = dt as f32;
        self.idle += dtf;
        if let Some(s) = &mut self.creaking {
            s.hit_cooldown = (s.hit_cooldown - dtf).max(0.0);
        }
        if self.alive() {
            let boosted = self.effects.max_health_bonus();
            let outcome = self.effects.tick(dtf);
            if outcome.heal > 0.0 {
                self.health = (self.health + outcome.heal).min(self.max_health());
            }
            // `onAttributeUpdated(MAX_HEALTH)` when Health Boost runs out.
            if self.effects.max_health_bonus() < boosted {
                let base = self.mount.as_ref().map_or(self.max_health(), |s| s.max_health);
                self.health = self.health.min(base + self.effects.max_health_bonus());
            }
            // v1.0 gap Track 8d: each effect hurts on its own
            // (`hurtServer(magic, 1)`, `hurtServer(wither, 1)`), so the hurt
            // cooldown refuses a second 1 in the same tick.
            let poison = outcome.poison.min((self.health - 1.0).max(0.0));
            for damage in [poison, outcome.wither, outcome.damage] {
                if damage > 0.0 && self.alive() && self.damage_bypassing_wolf_armor(damage, None, rng) {
                    events.push(EntityEvent::MobKilled {
                        kind: self.kind,
                        pos: self.pos,
                        burning: self.burning,
                        player_kill: self.player_hit_left > 0.0,
                        looting: 0,
                    });
                }
            }
        }
        self.hop_left -= dtf;
        if self.kind.is_cube() && self.on_ground && self.hop_left <= 0.0 {
            self.hop_delay = rng.range(0.5, 1.5);
        }
        if let Some(sound) = self.cry.take() {
            // v1.0 gap Track 2c: `Mob.playHurtSound` resets the ambient
            // sound time to `-getAmbientSoundInterval()` (80 ticks), so no
            // idle call follows a hurt cry within four seconds.
            if matches!(sound, MobSound::Hurt(_)) {
                self.ambient_timer = self.ambient_timer.max(4.0);
            }
            events.push(EntityEvent::Sound { sound, pos: self.pos + DVec3::Y * (self.shape().height * 0.8) });
        }
        self.ambient_timer -= dtf;
        if self.ambient_timer <= 0.0 {
            self.ambient_timer =
                if self.kind == MobKind::Ocelot { rng.range(45.0, 56.0) } else { rng.range(7.0, 18.0) };
            if self.alive()
                && self.armadillo.as_ref().is_none_or(|s| !s.scared())
                && self.kind != MobKind::CopperGolem
                && self
                    .sniffer
                    .as_ref()
                    .is_none_or(|s| !matches!(s.pose, super::sniffer::SEARCHING | super::sniffer::DIGGING))
                && self.kind != MobKind::Tadpole
                && self.kind != MobKind::Creeper
                && self.kind != MobKind::Warden
                && (self.bat.as_ref().is_none_or(|s| !s.resting) || rng.next_int(4) == 0)
            {
                let sound = self.fox_ambient(ctx, rng).unwrap_or(MobSound::Ambient(self.kind));
                events.push(EntityEvent::Sound { sound, pos: self.pos + DVec3::Y * (self.shape().height * 0.8) });
            }
        }
        if self.alive()
            && let Some(vanished) = self.update_trader(dtf, self.trader_night)
        {
            events.push(EntityEvent::Sound { sound: MobSound::TraderDrink(vanished), pos: self.pos + DVec3::Y });
        }
        self.hurt = (self.hurt - dtf).max(0.0);
        self.hurt_age = self.hurt_age.saturating_add(1);
        // `LivingEntity.baseTick`: the hurt cooldown counts down a tick.
        self.damage_cooldown = self.damage_cooldown.saturating_sub(1);
        self.provoked = (self.provoked - dtf).max(0.0);
        if matches!(self.kind, MobKind::Wolf | MobKind::Llama | MobKind::Enderman | MobKind::ZombifiedPiglin)
            && self.provoked == 0.0
        {
            self.angry_player = None;
            self.neutral_universal = false;
        }
        self.attack_cooldown -= dtf;
        self.player_hit_left = (self.player_hit_left - dtf).max(0.0);
        self.wither_credit = (self.wither_credit - dtf).max(0.0);
        // v0.16 Track C: `WitherRoseBlock.entityInside` (not in Peaceful;
        // wither skeletons and the Wither take no Wither).
        if self.alive()
            && self.difficulty != crate::simulation::difficulty::Difficulty::Peaceful
            && !matches!(self.kind, MobKind::WitherSkeleton | MobKind::Wither)
            && crate::physics::touches_block(world, self.pos, self.shape(), |b| {
                b == crate::world::wither_rose::WITHER_ROSE
            })
        {
            let ticks = crate::world::wither_rose::CONTACT_TICKS;
            self.effects.add(crate::simulation::effects::Effect::Wither, 0, ticks);
        }
        self.weakness_left = (self.weakness_left - dtf).max(0.0);
        self.advance_cure(dtf, world, rng, events);
        self.freeze_in_powder_snow(dtf, world);
        if let Some(v) = &mut self.villager {
            v.bell_hide = (v.bell_hide - dtf).max(0.0);
        }
        self.attack_anim = (self.attack_anim - dtf).max(0.0);
        self.swing_seen = self.attack_anim;

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

        if self.alive() {
            super::aquatic::environment(self, dtf, world, rng);
            if !self.alive() {
                events.push(EntityEvent::MobKilled {
                    kind: self.kind,
                    pos: self.pos,
                    burning: self.burning,
                    player_kill: self.player_hit_left > 0.0,
                    looting: 0,
                });
            }
        }
        // Bred horses and donkeys age with the shared animal clock; mules only here.
        if super::mounts::equine(self.kind)
            && !super::undead_mounts::undead(self.kind)
            && self.animal.is_none()
            && self.baby
            && self.age < 0
        {
            self.grow += dtf * 20.;
            let ticks = self.grow.floor() as i32;
            self.grow -= ticks as f32;
            self.age = (self.age + ticks).min(0);
            self.baby = self.age < 0;
        }
        super::bee::upkeep(self, dtf, rng, events);
        self.pathing = false;
        // v1.0 gap Track 3b: the camel brain's look memory, before its
        // idle behaviours pick (`SetWalkTargetFromLookTarget` reads it).
        let body_yaw = self.yaw;
        if self.dying.is_none() {
            for _ in 0..((dtf / super::motion::TICK).round() as u32).max(1) {
                self.camel_look(world, ctx, rng);
            }
        }
        let (wish, speed) = if let Some(t) = &mut self.dying {
            *t += dtf;
            (None, 0.0)
        } else if self.kind == MobKind::Phantom {
            if self.riding.is_none() {
                self.phantom_flight(dt, world, ctx, rng, events);
            }
            (None, 0.)
        } else if self.kind == MobKind::Wither {
            // Its goals run in `Entities::wither_tick`.
            (None, 0.)
        } else {
            // v1.0 gap Track 2c: `AbstractHorse.aiStep` grazes on the grass
            // block under its feet (`blockPosition().below()`).
            if let Some(s) = self.mount.as_mut() {
                let below = self.pos.floor().as_ivec3() - IVec3::Y;
                s.on_grass = world.block(below) == Some(crate::world::block::Block::GRASS);
            }
            super::mounts::control(self, dt, rng).unwrap_or_else(|| self.think(dtf, world, ctx, rng, events))
        };
        let (wish, speed) = self.camel_hold(world, wish, speed);
        // v1.0 gap Track 2c: the idle look goals and brain look behaviours,
        // after the mob's other goals took their flags.
        if self.dying.is_none() {
            self.head_goals(world, ctx, rng, wish.is_some());
            // v1.0 gap Track 2f: `HarvestFarmland` breaking a ripe crop.
            if let Some(cell) = self.walking.harvest_now.take() {
                events.push(EntityEvent::VillagerHarvest { cell });
            }
        }

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

        // Swimmers, ghasts and airborne blazes move a whole Java tick at a
        // time with their own move controls (`swim`).
        let java = (!controlled || self.kind == MobKind::HappyGhast || super::nautilus::is_nautilus(self.kind))
            && self.riding.is_none()
            && self.java_moves(world);
        // Turn toward the direction of travel: v1.0 gap Track 2c,
        // `MoveControl.tick`'s `rotlerp(yRot, wanted, 90)` a tick.
        if let Some(dir) = wish.filter(|_| !controlled && !java && !matches!(self.kind, MobKind::Bee | MobKind::Allay))
        {
            let target = (dir.z as f32).atan2(dir.x as f32);
            self.yaw = turn_toward(self.yaw, target, std::f32::consts::FRAC_PI_2 * dtf / super::motion::TICK);
        }
        // `LookControl.tick` after the move control (v1.0 gap Track 2b for
        // spears, Track 2c for every mob).
        for _ in 0..((dtf / super::motion::TICK).round() as u32).max(1) {
            self.look_tick();
        }

        if std::mem::take(&mut self.jump) && self.on_ground && wish.is_some() && !controlled && self.alive() {
            self.vel.y = JUMP_VELOCITY;
        }
        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        let alive_before_move = self.alive();
        if self.creaking.as_ref().is_some_and(|s| !s.can_move || s.teardown > 0.0) || self.shulker.is_some() {
            self.vel = DVec3::ZERO;
        } else if self.riding.is_some() || self.animal.as_ref().is_some_and(|a| a.perched.is_some()) {
            self.vel = DVec3::ZERO;
            self.on_ground = true;
        } else if self.kind == MobKind::Allay {
            super::allay::travel(self, dt, world, wish, speed);
        } else if self.kind == MobKind::Bee {
            super::bee::travel(self, dt, world, wish, speed);
        } else if self.kind == MobKind::Phantom {
            // PhantomMoveControl already moved this flying body.
        } else if self.kind == MobKind::Wither {
            self.wither_motion(dt, world);
        } else if self.kind == MobKind::Vex {
            // `Vex.tick` sets `noPhysics`: it flies through blocks.
            super::illager::vex_travel(self, dt);
        } else if self.sulfur_has_block() {
            self.sulfur_travel(dt, world);
        } else if controlled && equine::java_ridden(self.kind) {
            // v1.0 Track 7: a steered horse rides Java's `travelRidden`, in
            // water and lava as well since v1.0 gap Track 3, as do camels.
            self.move_ctl.time += dt;
            while self.move_ctl.time >= equine::TICK - 1e-9 {
                self.move_ctl.time -= equine::TICK;
                self.equine_ridden_tick(world);
            }
        } else if java {
            // Whole Java ticks of simulated time; the fixed 20 Hz step runs one.
            // The move controls apply Speed and Slowness to the raw speed.
            self.move_ctl.time += dt;
            while self.move_ctl.time >= swim::TICK - 1e-9 {
                self.move_ctl.time -= swim::TICK;
                self.java_tick(world, wish, speed, rng);
            }
        } else {
            for _ in 0..steps {
                let controlled = self.mount.as_ref().is_some_and(|s| s.controlled());
                self.physics_step(h, world, wish, speed * if controlled { 1. } else { self.speed_factor() });
            }
        }
        let wolf_fall = self.wolf_armor.as_mut().map_or(0.0, |s| std::mem::take(&mut s.pending_fall));
        if wolf_fall > 0.0 {
            let cause = std::mem::replace(&mut self.pending_fall_cause, Cause::FALL);
            self.damage_from(wolf_fall, cause, rng);
        }
        self.take_fall(rng);
        if let Some(s) = &mut self.mount {
            let damage = std::mem::take(&mut s.rider_fall);
            if damage > 0.
                && let Some(player) = s.rider
            {
                events.push(EntityEvent::PlayerFall { player, damage });
            }
        }
        // v1.0 gap Track 3b: the camel's sit, stand, dash and dash-ready
        // sounds, and a horse's angry and rearing calls.
        let mount_events = self.mount.as_mut().map(|s| s.take_events()).unwrap_or_default();
        if !mount_events.is_empty() {
            let at = self.pos + DVec3::Y * (self.shape().height * 0.8);
            for event in mount_events {
                let sound = match event {
                    super::mounts::Event::Ambient => Some(MobSound::Ambient(self.kind)),
                    e => super::mounts::Cue::for_event(self.kind, self.baby, e).map(MobSound::Mount),
                };
                if let Some(sound) = sound {
                    events.push(EntityEvent::Sound { sound, pos: at });
                }
            }
        }
        if self.health <= 0.0 && (self.dying.is_none() || alive_before_move) {
            // v0.16 Track C: `LivingEntity.createWitherRose` for a death
            // the Wither is credited with.
            if self.wither_credit > 0.0 && self.kind != MobKind::Wither {
                events.push(EntityEvent::WitherRose { pos: self.pos });
            }
            self.dying = Some(0.0);
            self.cry = Some(if let Some(s) = &self.copper_golem {
                MobSound::CopperGolem(super::copper_golem::Sound::Death, s.stage as u8)
            } else {
                MobSound::Death(self.kind)
            });
            events.push(EntityEvent::MobKilled {
                kind: self.kind,
                pos: self.pos,
                burning: false,
                player_kill: self.player_hit_left > 0.0,
                looting: 0,
            });
        }

        // v1.0 gap Track 7: the walk cycle follows the distance moved this
        // tick, as Java's `calculateEntityAnimation` does.
        for _ in 0..((dtf / super::motion::TICK).round() as u32).max(1) - 1 {
            self.advance_walk(DVec3::ZERO);
        }
        self.advance_walk(self.pos - tick_start);

        // v1.0 gap Track 3b: a camel's body follows its head within 30
        // degrees, and holds still while it sits or changes pose.
        self.camel_turn(body_yaw, self.pos - tick_start);
        // v1.0 gap Track 2c: `BodyRotationControl` after the move, and the
        // drawn head from Java's `yHeadRot` and `xRot`.
        self.body_tick(self.pos - tick_start);

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
                events.push(EntityEvent::LaidEgg { pos: self.pos, variant: self.farm_variant() });
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

    /// `InteractWithDoor`: open the closed wooden doors on the previous and
    /// next nodes of the path.
    fn open_path_doors<W: MobWorld + ?Sized>(&self, world: &W, events: &mut Vec<EntityEvent>) {
        let Some(p) = &self.nav.path else { return };
        for node in [p.previous_node(), p.next_node()].into_iter().flatten() {
            if node.ty == path::PathType::WalkableDoor || node.ty == path::PathType::DoorWoodClosed {
                for dy in 0..2 {
                    let cell = node.pos + IVec3::Y * dy;
                    if world.block(cell).is_some_and(|b| {
                        matches!(b.shaped(), Some(crate::world::block::Shaped::Door { open: false, upper: false, .. }))
                    }) {
                        events.push(EntityEvent::VillagerDoor { cell });
                    }
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
        if self.creaking.is_some() {
            return self.creaking_think(world, ctx, rng, events);
        }
        if self.warden.is_some() {
            return self.warden_steer(world, rng);
        }
        if self.breeze.is_some() {
            return self.breeze_think(dt, world, ctx, rng, events);
        }
        // v0.16 Track A: shulkers act in `Entities::shulker_upkeep`.
        if self.shulker.is_some() {
            return (None, 0.0);
        }
        if self.endermite.is_some()
            && let Some(wish) = self.endermite_think(dt, world, ctx, rng, events)
        {
            return wish;
        }
        // v1.0 Track 6: a raid witch with a raider to heal walks until it
        // sees it within 10 blocks (`RangedAttackGoal`); the raid tick throws.
        if self.kind == MobKind::Witch
            && let Some((_, at, at_eye)) = self.illager.as_ref().and_then(|s| s.raider.heal_target)
        {
            let eye = self.look_eye();
            let sees = line_of_sight(world, eye, at + DVec3::Y * at_eye);
            // `RangedAttackGoal.tick`: `setLookAt(target, 30, 30)`.
            self.set_look_at(at + DVec3::Y * at_eye, head::ATTACK_LOOK, head::ATTACK_LOOK);
            if (at - self.pos).with_y(0.0).length() <= WITCH_RANGE && sees {
                self.stop_navigation();
                return (None, 0.0);
            }
            let speed = self.kind.chase_speed();
            return (self.pursue(world, at, speed, sees, true, rng), speed);
        }
        // Raid witches carry the raider state but keep the witch's own goals.
        if self.illager.is_some()
            && self.kind != MobKind::Witch
            && let Some(wish) = self.illager_think(dt, world, ctx, rng, events)
        {
            return wish;
        }
        if self.kind == MobKind::Bat {
            self.bat_sense(world, ctx);
            return (None, 0.0);
        }
        if self.kind == MobKind::HappyGhast {
            return self.happy_ghast_think(dt, world, ctx, rng);
        }
        if self.nautilus.is_some() {
            return self.nautilus_think(dt, world, ctx, rng, events);
        }
        // A camel husk with a husk aboard goes where the husk hunts.
        if self.kind == MobKind::CamelHusk
            && let Some(goal) = self.hunt
        {
            let charge = if self.hunt_charge { 4.0 } else { 1.0 };
            let speed = self.mount.as_ref().map_or(2.2, |s| s.speed * 43.17) * charge;
            if ((goal - self.pos) * DVec3::new(1.0, 0.0, 1.0)).length() <= 0.9 {
                return (None, 0.0);
            }
            let sees = line_of_sight(world, self.look_eye(), goal + DVec3::Y);
            return (self.pursue(world, goal, speed, sees, true, rng), speed);
        }
        if let Some(wish) = self.leash_think(dt, world, rng) {
            return wish;
        }
        if self.sulfur_cube.is_some()
            && let Some(wish) = self.sulfur_cube_think(dt, world, rng)
        {
            return wish;
        }
        if self.sniffer.is_some()
            && let Some(wish) = self.sniffer_steer(dt, world, ctx, rng)
        {
            return wish;
        }
        if self.allay.is_some() {
            return self.allay_steer(dt, world, ctx, rng);
        }
        if self.copper_golem.is_some() {
            return self.copper_steer(dt, world, ctx, rng);
        }
        if self.armadillo.as_ref().is_some_and(|s| s.scared()) {
            self.stop_goal(world);
            return (None, 0.0);
        }
        if self.ocelot.is_some() {
            let base = self.run_goals(world, ctx, rng);
            if let Some(wish) = self.ocelot_think(world, rng, events) {
                return wish;
            }
            if let Some(wish) = base {
                let s = self.ocelot.as_mut().unwrap();
                s.crouching = wish.0.is_some()
                    && self
                        .goals
                        .running
                        .is_some_and(|i| matches!(goals::table(self.kind)[i as usize].goal, goals::Goal::Tempt));
                s.sprinting =
                    wish.0.is_some() && self.goals.avoiding.is_some_and(|(_, at)| at.distance_squared(self.pos) < 49.0);
                return wish;
            }
        }
        if self.panda.is_some() {
            let base = self.run_goals(world, ctx, rng);
            if let Some(wish) = self.panda_think(world, rng, events) {
                return wish;
            }
            if let Some(wish) = base {
                return wish;
            }
        }
        if let Some(wish) = self.polar_think(world, rng, events) {
            return wish;
        }
        // v1.0 gap Track 2e: the fox's goals around the shared table
        // (`FoxMoveControl` holds it still while it can't move).
        if self.fox.is_some() {
            let full = self.goals.full(self.uid);
            let wish = match self.fox_pre_think(world, rng, full, events) {
                Some(wish) => {
                    self.goals.ticks = self.goals.ticks.wrapping_add(1);
                    Some(wish)
                }
                None => {
                    let base = self.run_goals(world, ctx, rng);
                    self.fox_think(world, ctx, rng, full, events).or(base)
                }
            };
            let s = self.fox.as_ref().unwrap();
            let can_move = s.can_move(self.animal.as_ref().is_some_and(|a| a.sleeping));
            let wish = wish.map(|w| if can_move { w } else { (None, 0.0) });
            if let Some(wish) = wish {
                self.fox.as_mut().unwrap().still = wish.0.is_none();
                return wish;
            }
            if !can_move {
                self.fox.as_mut().unwrap().still = true;
                return (None, 0.0);
            }
        }
        if let Some(wish) = self.companion_think(dt, world, ctx, rng, events) {
            return wish;
        }
        if self.kind == MobKind::Strider {
            self.update_strider_cold(world);
            if self.mob_steered {
                return self.steered_strider(world, ctx, rng);
            }
        }
        // A guardian aiming its beam stands still, and an axolotl playing
        // dead (`PLAY_DEAD`, its top activity) does nothing else.
        if self.kind.is_aquatic() && super::aquatic::upkeep(self, dt, world, ctx, rng, events) {
            self.stop_goal(world);
            return (None, 0.0);
        }
        if self.kind == MobKind::Bee && self.bee.is_some_and(|s| s.anger > 0) {
            return super::bee::steer(self, dt, world, ctx, rng, events);
        }
        if self.frog_priority() {
            return self.frog_steer(dt, world, ctx, rng);
        }
        if self.panda.is_none()
            && self.ocelot.is_none()
            && self.fox.is_none()
            && let Some(wish) = self.run_goals(world, ctx, rng)
        {
            if self.sniffer.is_some() {
                self.walking.walk = None;
            }
            return wish;
        }
        // v1.0 gap Track 2e: an idle sniffer walks its `RunOne`'s walk
        // targets (`MoveToTargetSink(500, 700)`) instead of wandering.
        if self.sniffer.is_some() && self.ai != Ai::Panic {
            return self.walk_sink(world, ctx, rng, (500, 700)).unwrap_or((None, 0.0));
        }
        if self.kind == MobKind::Bee {
            return super::bee::steer(self, dt, world, ctx, rng, events);
        }
        if self.kind == MobKind::Turtle {
            return self.turtle_steer(dt, world, ctx, rng);
        }
        if self.frog.is_some() {
            return self.frog_steer(dt, world, ctx, rng);
        }
        if self.kind.is_aquatic() {
            return super::aquatic::steer(self, dt, world, ctx, rng, events);
        }
        if matches!(self.kind, MobKind::Villager | MobKind::WanderingTrader)
            && let Some(v) = &self.villager
        {
            if v.sleeping || v.trading && !v.fleeing {
                self.walking.walk = None;
                return (None, 0.0);
            }
            if v.goal.is_some() {
                // The game's own walks (home, job site, bell, crops, a
                // partner, fleeing) take `WALK_TARGET`.
                self.walking.walk = None;
            } else if self.kind == MobKind::Villager {
                // v1.0 gap Track 2e: the brain's idle movement replaces the
                // wandering: `MoveToTargetSink` walks what the activity's
                // `RunOne` set (the play package's own sink times out after
                // 80 to 120 ticks).
                let timeout = if self.age < 0 { (80, 120) } else { (150, 250) };
                let Some(wish) = self.walk_sink(world, ctx, rng, timeout) else { return (None, 0.0) };
                self.open_path_doors(world, events);
                return wish;
            }
            let v = self.villager.as_ref().unwrap();
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
                let wish = (self.follow_path(world, speed, false), speed);
                self.open_path_doors(world, events);
                return wish;
            }
        }
        if self.trader.as_ref().is_some_and(|t| t.drink > 0.0) {
            return (None, 0.0);
        }
        // An enderman hunting an endermite still notices stares: its
        // `EndermanLookForPlayerGoal` (target priority 1) outranks the
        // endermite goal (3).
        if self.kind == MobKind::Enderman && self.hunt.is_some() && self.enderman_anger(dt, world, ctx, rng, events) {
            self.hunt = None;
        }
        if let Some(goal) = self.hunt {
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            let dist = flat.length();
            if dist > 1e-4 {
                let reach = match self.kind {
                    MobKind::IronGolem => 2.4,
                    MobKind::Enderman => super::endermite::ENDERMAN_REACH,
                    _ => 1.5,
                };
                // `MeleeAttackGoal.tick`: `setLookAt(target, 30, 30)`.
                self.set_look_at(goal + DVec3::Y * self.hunt_eye, head::ATTACK_LOOK, head::ATTACK_LOOK);
                let golem = self.kind == MobKind::IronGolem;
                if dist <= reach
                    && self.attack_cooldown <= 0.0
                    && (golem || self.kind.is_zombie() || self.kind == MobKind::Enderman)
                {
                    self.attack_cooldown = if golem { 1.2 } else { ATTACK_COOLDOWN };
                    // `IronGolem.doHurtTarget`: `attackAnimationTick = 10`.
                    self.attack_anim = if golem { 10.0 * super::motion::TICK } else { 0.4 };
                    self.strike = true;
                }
                if dist <= 0.9 {
                    return (None, 0.0);
                }
                let speed = if super::undead_mounts::undead(self.kind) {
                    let charge = if self.hunt_charge && self.kind == MobKind::ZombieHorse { 1.4 } else { 1.0 };
                    self.mount.as_ref().map_or(2.2, |s| s.speed * 43.17) * charge
                } else if self.kind == MobKind::IronGolem {
                    1.5
                } else {
                    self.kind.chase_speed()
                };
                let sees = line_of_sight(world, self.look_eye(), goal + DVec3::Y * self.hunt_eye);
                return (self.pursue(world, goal, speed, sees, true, rng), speed);
            }
        }
        if self.kind == MobKind::Witch && self.witch_upkeep(dt, world, rng) {
            return (None, 0.0);
        }
        // v0.15 Track E: a raid witch not chasing anyone walks the raid.
        if self.kind == MobKind::Witch
            && self.ai != Ai::Chase
            && self.illager.is_some()
            && let Some(wish) = self.raider_goals(world, rng)
        {
            return wish;
        }
        // v1.0 gap Track 2: `Mount.create(0.8)` walks a baby piglin to its
        // ride target.
        if self.kind == MobKind::Piglin
            && self.riding.is_none()
            && let Some(at) = self.ride_walk
        {
            let speed = self.kind.chase_speed() * 0.8 * if self.baby { nether_ai::BABY_SPEED } else { 1.0 };
            let sees = line_of_sight(world, self.look_eye(), at + DVec3::Y * 0.5);
            return (self.pursue(world, at, speed, sees, false, rng), speed);
        }
        if self.nether.is_some()
            && let Some(wish) = self.nether_think(dt, world, ctx, rng, events)
        {
            return wish;
        }
        if self.kind == MobKind::Ghast {
            return self.ghast_think(dt, world, ctx, rng, events);
        }
        if self.kind == MobKind::Blaze
            && let Some(wish) = self.blaze_think(dt, world, ctx, rng, events)
        {
            return wish;
        }
        self.ai_timer -= dt;
        let target = if self.kind.is_zombie() {
            self.zombie_target(ctx)
        } else if matches!(self.kind, MobKind::Enderman | MobKind::ZombifiedPiglin) && self.provoked > 0.0 {
            // `NeutralMob.isAngryAt`: an angry one goes only for its persistent
            // target (`neutral` picks one under `universal_anger`).
            self.angry_player.and_then(|id| {
                ctx.players.iter().find(|t| {
                    t.id == id
                        && t.targetable
                        && t.alive
                        && t.pos.distance_squared(self.pos) <= t.detection_range_for(CHASE_RANGE, self.kind).powi(2)
                })
            })
        } else if self.kind.is_hostile() && self.nether.is_none() {
            let range = match self.kind {
                MobKind::CaveSpider | MobKind::Slime | MobKind::MagmaCube => 16.,
                _ => CHASE_RANGE,
            };
            ctx.nearest_detectable_target(self.pos, range)
        } else {
            ctx.nearest_target(self.pos)
        };
        // v1.0 gap Track 2: the shared hostile chase follows Java's target
        // goal scheduling (`chase::chase_schedule`).
        let mut full = false;
        let target = if self.kind.is_hostile()
            && self.nether.is_none()
            && self.kind != MobKind::Blaze
            && self.kind != MobKind::Endermite
            && (self.illager.is_none() || self.kind == MobKind::Witch)
        {
            full = self.chase_full();
            let aggressive = match self.kind {
                MobKind::Spider | MobKind::CaveSpider => ctx.daylight < SPIDER_CALM_DAYLIGHT || self.provoked > 0.0,
                MobKind::ZombifiedPiglin => self.provoked > 0.0,
                // `NearestAttackableWitchTargetGoal.canAttack` is off while
                // the heal goal cools down (v1.0 Track 6).
                MobKind::Witch => self.illager.as_ref().is_none_or(|s| s.raider.heal_cooldown <= 0),
                MobKind::Enderman => self.enderman_anger(dt, world, ctx, rng, events),
                _ => true,
            };
            let (range, height) = match self.kind {
                // Slimes' and magma cubes' target predicate: 4 blocks up or down.
                MobKind::CaveSpider | MobKind::Slime | MobKind::MagmaCube => (16.0, 4.0),
                // `NearestAttackableTargetGoal` searches the follow range.
                k if k.is_zombie() => (self.follow_range(), f64::INFINITY),
                _ => (CHASE_RANGE, f64::INFINITY),
            };
            // A reinforcement's `setTarget` and a hit (`HurtByTargetGoal`)
            // set the target outside the goal, at once.
            let sent = self.reinforce_target;
            let forced = target.filter(|t| sent == Some(t.id) || self.provoked > 0.0);
            self.chase_schedule(world, ctx, forced, aggressive, range, height, full, rng)
        } else {
            target
        };
        // A lowered spear stabs every tick (`updatingUsingItem`) in
        // `Entities::mob_spear_stabs`, before the mobs move; the flagless
        // door goal runs beside everything else.
        if self.kind != MobKind::Vindicator && self.has_door_goal() {
            self.door_goal(world, full, rng, events);
        } else {
            self.door_break = None;
        }
        let to_player = target.map_or(DVec3::ZERO, |t| t.pos - self.pos);
        let flat = DVec3::new(to_player.x, 0.0, to_player.z);
        let hdist = flat.length();

        if self.ai != Ai::Chase {
            self.fuse = (self.fuse - dt).max(0.0);
            self.swelling = false;
            self.lift = false;
            self.charged = false;
            self.attack_step = 0;
        }

        match self.ai {
            Ai::Chase => {
                let Some(&target) = target else { return (None, 0.0) };
                // The attack goals look at the target's eyes:
                // `setLookAt(target, 30, 30)`.
                self.set_look_at(
                    target.pos + DVec3::Y * head::player_eye(&target),
                    head::ATTACK_LOOK,
                    head::ATTACK_LOOK,
                );
                let dir = if hdist > 1e-6 { flat / hdist } else { DVec3::X };
                if self.kind == MobKind::Enderman && self.enderman_tactics(dt, world, &target, rng, events) {
                    return (None, 0.0);
                }
                match self.kind {
                    MobKind::Skeleton | MobKind::Stray | MobKind::Bogged | MobKind::Parched => {
                        return self.skeleton_tactics(dt, world, target.pos, dir, hdist, rng, events);
                    }
                    MobKind::Drowned if self.holds_trident() => {
                        return self.drowned_trident_tactics(dt, world, &target, rng, events);
                    }
                    MobKind::Witch => return self.witch_tactics(world, &target, dir, hdist, rng, events),
                    MobKind::Creeper => {
                        if let Some(stop) = self.creeper_fuse(dt, hdist, events) {
                            return stop;
                        }
                    }
                    // v1.0 gap Track 2: Java's `MeleeAttackGoal`, and the
                    // zombies' goal table with `SpearUseGoal`.
                    k if chase::java_melee(k) => {
                        let speed = self.kind.chase_speed() * if self.baby { 1.5 } else { 1.0 };
                        return if chase::zombie_table(k) {
                            self.zombie_chase(world, ctx, &target, speed, full, rng, events)
                        } else {
                            self.chase_melee(world, &target, speed, full, rng, events)
                        };
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
                                    Cause::mob(if self.kind == MobKind::MagmaCube {
                                        "was slain by a magma cube"
                                    } else {
                                        "was slain by a slime"
                                    }),
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
                            // MOB-02d: the held weapon's Fire Aspect.
                            let fire = self.weapon_fire_seconds();
                            if fire > 0.0 {
                                events.push(EntityEvent::Ignite { player: target.id, secs: fire });
                            }
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
                let eye = self.look_eye();
                let sees = matches!(self.kind, MobKind::Spider | MobKind::CaveSpider)
                    || line_of_sight(world, eye, target.eye());
                (self.pursue(world, target.pos, speed, sees, true, rng), speed)
            }
            Ai::Panic => {
                self.panic_teleport_to_owner(world, ctx, rng);
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
                        return (Some(dir), speed);
                    }
                }
                if rng.chance(dt * 1.2) || self.blocked && rng.chance(dt * 4.0) {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                (Some(yaw_dir(self.move_yaw)), speed)
            }
            Ai::Wander if self.path_move => {
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
                (Some(yaw_dir(self.move_yaw)), self.kind.wander_speed())
            }
            Ai::Idle => {
                if super::mounts::camel_like(self.kind) {
                    self.camel_idle(world, ctx, rng);
                } else if self.kind.walks_paths() {
                    // `RandomStrollGoal`: a 1-in-`interval` chance a tick (1 in
                    // half the interval per goal update, every other tick).
                    // Track 2d: a running idle look goal holding `MOVE` at a
                    // better priority keeps the stroll from starting.
                    let stroll = goals::table(self.kind).iter().find(|e| matches!(e.goal, goals::Goal::Stroll));
                    let allowed = stroll.is_none_or(|e| self.idle_look_allows(e.priority, false));
                    if allowed
                        && rng.chance(dt * 20.0 / self.kind.stroll_interval())
                        && self.try_stroll(world, ctx, rng)
                    {
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

    /// Java's `TargetGoal` for a player: `NearestAttackableTargetGoal`
    /// (`randomInterval` 10, `mustSee`) picks the nearest visible survival
    /// player within the follow range that `accept` allows, and the target
    /// is kept while it stays within range and was seen in the last
    /// `reducedTickDelay(60)` selector updates. Runs on the selector's full
    /// update; returns the target.
    fn java_target<'a, W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &'a Ctx,
        rng: &mut Rng,
        range: f64,
        full: bool,
        accept: impl Fn(&super::Target) -> bool,
    ) -> Option<&'a super::Target> {
        let eye = self.look_eye();
        let sees = |t: &super::Target| line_of_sight(world, eye, t.eye());
        let c = &mut self.move_ctl;
        let current = c.target.and_then(|id| ctx.players.iter().find(|t| t.id == id));
        if let Some(t) = current {
            let keep = t.targetable && t.alive && t.pos.distance_squared(self.pos) <= range * range;
            if keep && full {
                c.unseen = if sees(t) { 0 } else { c.unseen + 1 };
            }
            if keep && c.unseen <= c.unseen_limit.max(30) {
                return Some(t);
            }
            c.target = None;
        }
        if !full || rng.next_int(5) != 0 {
            return None;
        }
        let found = ctx
            .players
            .iter()
            .filter(|t| {
                t.targetable
                    && t.alive
                    && t.pos.distance_squared(self.pos) <= t.detection_range_for(range, self.kind).powi(2)
                    && accept(t)
            })
            .filter(|t| sees(t))
            .min_by(|l, r| l.pos.distance_squared(self.pos).total_cmp(&r.pos.distance_squared(self.pos)));
        let c = &mut self.move_ctl;
        c.target = found.map(|t| t.id);
        c.unseen = 0;
        c.unseen_limit = 30;
        found
    }

    /// `HurtByTargetGoal.start`: target the attacker, remembered unseen for
    /// `reducedTickDelay(300)` updates.
    fn hurt_by_target(&mut self, player: super::PlayerId) {
        let c = &mut self.move_ctl;
        c.target = Some(player);
        c.unseen = 0;
        c.unseen_limit = selector::reduced(300);
    }

    /// Selector bookkeeping for kinds without a goal table: counts
    /// `tickCount` and says whether this tick is the full update.
    fn selector_full(&mut self) -> bool {
        let full = self.goals.full(self.uid);
        self.goals.ticks = self.goals.ticks.wrapping_add(1);
        full
    }

    /// The blaze's 26.3 goals: `NearestAttackableTargetGoal` (follow range
    /// 48), `BlazeAttackGoal` and the `customServerAiStep` height offset.
    /// Within 2 blocks and in sight it hits every 20 ticks and walks at the
    /// target; farther, while in sight, it holds still and charges for 60
    /// ticks, fires three small fireballs 6 ticks apart with Java's
    /// spread, and rests 100 ticks; out of sight it heads for the target for
    /// 5 ticks. It rises while the target's eyes are more than
    /// `allowedHeightOffset` above its own. `None` without a target leaves it
    /// to `WaterAvoidingRandomStrollGoal`.
    fn blaze_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let full = self.selector_full();
        let ticks = self.goal_ticks(dt);
        // `nextHeightOffsetChangeTick`: a new offset every 100 ticks.
        self.offset_timer -= dt;
        if self.offset_timer <= 0.0 {
            self.offset_timer = 5.0;
            self.height_offset = rng.triangle(0.5, 6.891) as f32;
        }
        let Some(&target) = self.java_target(world, ctx, rng, BLAZE_RANGE, full, |_| true) else {
            if self.ai == Ai::Chase {
                // `BlazeAttackGoal.stop`.
                self.ai = Ai::Idle;
                self.ai_timer = 1.0;
                self.charged = false;
                self.move_ctl.last_seen = 0;
                self.stop_navigation();
            }
            self.lift = false;
            return None;
        };
        if self.ai != Ai::Chase {
            // `BlazeAttackGoal.start`.
            self.ai = Ai::Chase;
            self.attack_step = 0;
            self.path_move = false;
            self.stop_navigation();
        }
        let eye = self.look_eye();
        let target_eye = target.eye();
        self.lift = target_eye.y > eye.y + self.height_offset as f64;
        let to = target.pos - self.pos;
        // `BlazeAttackGoal.tick`: `setLookAt(target, 10, 10)`.
        self.set_look_at(target_eye, 10.0, 10.0);
        let sees = line_of_sight(world, eye, target_eye);
        self.move_ctl.last_seen = if sees { 0 } else { self.move_ctl.last_seen + ticks as i32 };
        let toward = (Some((to * DVec3::new(1.0, 0.0, 1.0)).normalize_or(DVec3::X)), path::ground_speed(0.23));
        let d2 = to.length_squared();
        if d2 < 4.0 {
            if !sees {
                return Some((None, 0.0));
            }
            if self.attack_cooldown <= 0.0 {
                self.attack_cooldown = 1.0;
                self.attack_anim = 0.35;
                let (damage, cause) = self.melee_attack();
                let dir = (to * DVec3::new(1.0, 0.0, 1.0)).normalize_or(DVec3::X);
                let knockback = dir * 6.0 + DVec3::Y * 5.0;
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
                self.thorns_response(&target, knockback, rng, events);
            }
            return Some(toward);
        }
        if d2 < BLAZE_RANGE * BLAZE_RANGE && sees {
            if self.attack_cooldown <= 0.0 {
                self.attack_step += 1;
                match self.attack_step {
                    1 => {
                        self.attack_cooldown = BLAZE_CHARGE;
                        self.charged = true;
                    }
                    2..=4 => self.attack_cooldown = BLAZE_VOLLEY,
                    _ => {
                        self.attack_cooldown = BLAZE_REST;
                        self.attack_step = 0;
                        self.charged = false;
                    }
                }
                if self.attack_step > 1 {
                    // Aimed from the middle of the blaze at the middle of the target.
                    let mid = self.pos.y + self.shape().height * 0.5;
                    let (xd, yd, zd) = (to.x, target.pos.y + target.shape.height * 0.5 - mid, to.z);
                    let spread = d2.sqrt().sqrt() * 0.5 * 2.297;
                    let dir = DVec3::new(rng.triangle(xd, spread), yd, rng.triangle(zd, spread)).normalize_or(DVec3::X);
                    let from = DVec3::new(self.pos.x, mid + 0.5, self.pos.z);
                    events.push(EntityEvent::Fireball { from, dir, large: false });
                    events.push(EntityEvent::Sound { sound: MobSound::Fireball, pos: eye });
                }
            }
            return Some((None, 0.0));
        }
        if self.move_ctl.last_seen < 5 {
            return Some(toward);
        }
        Some((None, 0.0))
    }

    /// The ghast's 26.3 goals: `RandomFloatAroundGoal` on each full update
    /// whatever else it does, `NearestAttackableTargetGoal` (follow range
    /// 100, within 4 blocks vertically), `GhastLookGoal` facing a target
    /// within 64 blocks, and `GhastShootFireballGoal`: charging while it sees
    /// a target within 64 blocks, warning at 10 ticks and firing at 20 from 4
    /// blocks in front, then waiting 40 ticks; out of sight the charge drains.
    fn ghast_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let full = self.selector_full();
        let ticks = self.goal_ticks(dt);
        if full {
            self.ghast_float(rng);
        }
        let y = self.pos.y;
        let target = self.java_target(world, ctx, rng, GHAST_RANGE, full, |t| (t.pos.y - y).abs() <= 4.0).copied();
        let Some(target) = target else {
            if self.ai == Ai::Chase {
                self.ai = Ai::Idle;
                self.charged = false;
            }
            self.move_ctl.look = None;
            self.move_ctl.charge = 0;
            return (None, 0.0);
        };
        self.ai = Ai::Chase;
        let mid = self.pos + DVec3::Y * (self.shape().height * 0.5);
        let target_mid = target.pos + DVec3::Y * (target.shape.height * 0.5);
        let close = target.pos.distance_squared(self.pos) < 4096.0;
        self.move_ctl.look = Some(target.pos);
        if close {
            let to = target.pos - self.pos;
            self.yaw = (to.z as f32).atan2(to.x as f32);
        }
        let sees = close && ticks > 0 && line_of_sight(world, mid, target_mid);
        let c = &mut self.move_ctl;
        // `GhastShootFireballGoal.tick` once per simulated tick.
        for _ in 0..ticks {
            if sees {
                c.charge += 1;
                if c.charge == 10 {
                    events.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::Ghast), pos: mid });
                }
                if c.charge == 20 {
                    let view = yaw_dir(self.yaw);
                    let from = DVec3::new(self.pos.x + view.x * 4.0, mid.y + 0.5, self.pos.z + view.z * 4.0);
                    let aim = DVec3::new(target.pos.x - from.x, target_mid.y - (0.5 + mid.y), target.pos.z - from.z);
                    events.push(EntityEvent::Fireball { from, dir: aim.normalize_or(view), large: true });
                    events.push(EntityEvent::Sound { sound: MobSound::Fireball, pos: mid });
                    c.charge = -40;
                }
            } else if c.charge > 0 {
                c.charge -= 1;
            }
        }
        self.charged = self.move_ctl.charge > 10;
        (None, 0.0)
    }

    /// Where an enderman's eyes are.
    fn eye(&self) -> DVec3 {
        self.pos + DVec3::Y * ENDERMAN_EYE
    }

    /// Java's `isLookingAtMe`: `target` looks within a narrowing cone of
    /// this enderman's eyes, with nothing in between.
    fn watched_by<W: MobWorld + ?Sized>(&self, world: &W, target: &super::Target) -> bool {
        let from = target.eye();
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
        let starer = ctx.players.iter().find(|t| t.targetable && self.watched_by(world, t)).map(|t| t.id);
        self.stare = if starer.is_some() { self.stare + dt } else { 0.0 };
        if self.stare >= STARE_TIME && self.provoked <= 0.0 {
            self.provoked = ENDERMAN_ANGER_TIME;
            // `EndermanLookForPlayerGoal` targets the starer, and
            // `updatePersistentAnger` makes it the persistent target.
            self.angry_player = starer;
            self.neutral_universal = false;
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
                // `Enderman.teleport`: `teleport` where it left.
                events.push(EntityEvent::Vibration {
                    pos: self.pos,
                    event: crate::world::vibration::GameEvent::Teleport,
                    source: crate::world::vibration::EventSource::mob(self.uid),
                });
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
        let eye = self.look_eye();
        let target = player + DVec3::Y * 1.2;
        if self.attack_cooldown <= 0.0 && hdist < SHOOT_RANGE && line_of_sight(world, eye, target) {
            self.attack_cooldown = if self.kind == MobKind::Skeleton {
                rng.range(1.6, 2.4)
            } else {
                self.kind.bow_interval(self.difficulty == crate::simulation::difficulty::Difficulty::Hard)
            };
            self.attack_anim = 0.35;
            events.push(EntityEvent::Shoot {
                from: eye + dir * 0.5,
                target,
                cause: match self.kind {
                    MobKind::Stray => Cause::STRAY_ARROW,
                    MobKind::Bogged => Cause::BOGGED_ARROW,
                    MobKind::Parched => Cause::PARCHED_ARROW,
                    _ => Cause::SKELETON_ARROW,
                },
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
    /// A witch is drinking a potion (`isDrinkingPotion`).
    pub fn drinking_potion(&self) -> bool {
        self.drink_left > 0.0
    }

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
        let eye = self.look_eye();
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
        let eye = self.look_eye();
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
        // `SwellGoal.canUse`: already swelling, or the target within 3 blocks;
        // it stops past 7. A swell left by a fall burns down meanwhile.
        let fusing = if self.swelling { hdist < FUSE_KEEP } else { hdist < FUSE_START };
        self.swelling = fusing;
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
            if self.lightning_charged {
                events.push(EntityEvent::ChargedBlast { center });
            }
            events.push(EntityEvent::Explosion {
                center,
                power: CREEPER_POWER * if self.lightning_charged { 2. } else { 1. },
                cause: Cause::CREEPER,
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
        self.in_water = if self.kind.is_aquatic() || self.kind == MobKind::Frog {
            world.block((self.pos + DVec3::Y * 0.2).floor().as_ivec3()).is_some_and(Block::holds_water)
        } else {
            physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.3, 0.0))
        };
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
        let below = world.block(super::item::below_pos(self.pos));
        let target = if self.on_ground && below == Some(crate::world::stone_blocks::SLIME_BLOCK) {
            target * SLIME_WALK
        } else {
            target
        };
        // Java's `Drowned.wantsToSwim` (a target): otherwise they sink like
        // other mobs in water below. Swimmers with their own `travelInWater`
        // move in `swim`.
        let swims = self.kind == MobKind::Drowned && self.alive() && (self.ai == Ai::Chase || self.hunt.is_some());
        if swims && self.in_water {
            self.vel += (target - self.vel) * (dt * 3.0).min(1.0);
            let delta = self.vel * dt;
            let hit = physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape);
            self.on_ground = hit.on_ground;
            self.blocked = hit.horizontal;
            // `travelInWater`: lift onto a bank only when the way ahead is free
            // 0.6 higher, so underwater walls don't launch them.
            if self.blocked
                && let Some(w) = wish
            {
                let ahead = self.pos + DVec3::new(w.x, 0.0, w.z).normalize_or_zero() * 0.3 + DVec3::Y * 0.6;
                if !physics::overlaps_solid(world, ahead, shape) {
                    self.vel.y = self.vel.y.max(4.5);
                }
            }
            return;
        }
        if self.alive() && self.kind == MobKind::Parrot && wish.is_some() {
            let k = (dt * 1.6).min(1.0);
            self.vel += (target - self.vel) * k;
        } else if self.in_water {
            let k = (dt * 4.0).min(1.0);
            // Flowing water and lava carry the mob in `Mob::current` (v1.0
            // gap Track 3b; see `fluid_current_flow`).
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
            self.vel.y = if let Some(amplifier) = self.effects.levitation() {
                // v0.15 Track D: `LivingEntity.travelInAir` eases the motion
                // toward 0.05 blocks a tick per level (0.2 of the gap a tick,
                // then 0.98 drag) instead of falling.
                let ticks = dt * 20.0;
                let target = 0.05 * (f64::from(amplifier) + 1.0) * 20.0;
                (target + (self.vel.y - target) * 0.8f64.powf(ticks)) * 0.98f64.powf(ticks)
            } else if super::mounts::equine(self.kind) {
                // Java: gravity 0.08 blocks/tick and vertical drag 0.98.
                ((self.vel.y - 32.0 * dt) * 0.98f64.powf(dt * 20.)).max(-78.)
            } else {
                (self.vel.y - GRAVITY * dt).max(-78.)
            };
            if matches!(self.kind, MobKind::Chicken | MobKind::Parrot) {
                self.vel.y = self.vel.y.max(-2.5); // flaps its way down
            }
            if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider)
                && self.blocked
                && wish.is_some()
                && self.alive()
            {
                self.vel.y = self.vel.y.max(3.0); // climbs walls
            } else if self.alive()
                && !matches!(self.kind, MobKind::Ghast | MobKind::Blaze | MobKind::Parrot)
                // v1.0 gap Track 3b: `AbstractHorse.onClimbable` is false:
                // horses, llamas and camels never climb.
                && !super::mounts::equine(self.kind)
                && self.kind != MobKind::TraderLlama
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

        if self.in_water {
            // `BubbleColumnBlock.entityInside`, rescaled from ticks to `dt`.
            let (min, max) = shape.aabb(self.pos);
            if let Some(dy) = crate::world::bubble::apply(|p| world.block(p), min, max, self.vel.y / 20.0) {
                self.vel.y += (dy * 20.0 - self.vel.y) * (dt * 20.0).min(1.0);
            }
        }
        // v1.0 Track 5: `makeStuckInBlock` in a cobweb (see `hazards`).
        // v1.0 gap Track 3b: the current's share of the motion, built up a
        // tick at a time (`fluid_current_flow`).
        let flow = self.fluid_current_flow(world, dt);
        let before = self.vel + flow;
        let mut moving = before;
        let delta = match self.web_hold(world) {
            Some(hold) => {
                let delta = moving * dt * hold;
                self.vel = DVec3::ZERO;
                self.current = DVec3::ZERO;
                moving = DVec3::ZERO;
                self.fall_distance = 0.0;
                delta
            }
            None => moving * dt,
        };
        let from_y = self.pos.y;
        let falling = self.vel.y;
        let step_height = if super::mounts::camel_like(self.kind) {
            1.5
        } else if self.kind == MobKind::CopperGolem
            || self.kind == MobKind::Ravager
            || super::mounts::equine(self.kind)
            || self.mount.as_ref().is_some_and(|s| s.controlled())
        {
            1.
        } else {
            crate::player::STEP_HEIGHT
        };
        let hit = if self.on_ground {
            physics::move_box_stepping(world, &mut self.pos, &mut moving, delta, shape, step_height)
        } else {
            physics::move_box(world, &mut self.pos, &mut moving, delta, shape)
        };
        // A blocked axis stops the walking and the current alike.
        for axis in 0..3 {
            if moving[axis] == 0.0 && before[axis] != 0.0 {
                self.vel[axis] = 0.0;
                self.current[axis] = 0.0;
            }
        }
        // `Entity.restituteMovementAfterCollisions`: slime blocks and beds
        // throw a falling mob back up, once it falls faster than one tick of
        // gravity (0.08 a tick).
        let below = world.block(super::item::below_pos(self.pos));
        self.wolf_fall(from_y, hit.on_ground, below);
        // v1.0 Track 5: every other mob's fall (mounts and wolves keep
        // their own).
        if self.mount.is_none() && self.wolf_armor.is_none() {
            self.track_fall(world, from_y, hit.on_ground);
        }
        let bounce = crate::player::bounciness(below);
        if hit.on_ground && bounce > 0.0 && -falling > 0.08 * 20.0 {
            self.vel.y = -falling * bounce * BOUNCE_DRAG;
            self.bounces = self.bounces.wrapping_add(1);
        }
        if let Some(s) = &mut self.mount {
            if self.in_water {
                s.fall_distance = 0.;
            } else {
                s.fall_distance += (from_y - self.pos.y).max(0.);
            }
            // `SlimeBlock.fallOn`: no fall damage on slime.
            if hit.on_ground && below == Some(crate::world::stone_blocks::SLIME_BLOCK) {
                s.fall_distance = 0.;
            }
            if hit.on_ground {
                // v1.0 Track 7: `LivingEntity.calculateFallDamage` floors;
                // a horse's safe fall is 6 and its multiplier 0.5.
                let damage = if super::mounts::equine(self.kind) {
                    equine::fall_damage(s.fall_distance, equine::SAFE_FALL, equine::FALL_MULTIPLIER)
                } else if self.kind == MobKind::Strider {
                    0.
                } else {
                    (s.fall_distance - 3.).ceil().max(0.) as f32
                };
                // v1.0 gap Track 8d: through `hurtServer`, queued for the
                // tick's `take_fall`.
                if damage > 0. {
                    self.pending_fall += damage;
                    self.pending_fall_cause = Cause::FALL;
                }
                // `AbstractHorse.causeFallDamage`: only a fall that hurts the
                // horse passes on, as each rider's own fall damage.
                if super::mounts::equine(self.kind) && s.rider.is_some() && damage > 0. {
                    s.rider_fall += equine::fall_damage(s.fall_distance, equine::PLAYER_SAFE_FALL, 1.0);
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
        let body_protected = matches!(self.kind, MobKind::ZombieHorse | MobKind::ZombieNautilus)
            && self.mount.as_ref().is_some_and(|s| s.slots[1].is_some());
        let sunburn = self.kind.burns_in_sun()
            && !body_protected
            && ctx.daylight > BURN_DAYLIGHT
            && !ctx.raining
            && !self.in_water
            && world.exposed(head);
        // v1.0 Track 5: `Mob.burnUndead`: anything on the head keeps the
        // sun off, and a helmet wears instead.
        let sunburn = sunburn && !(self.alive() && self.sun_blocked(ctx.daylight, rng));
        if self.kind.fire_immune() {
            self.fire_left = 0.0;
        } else if in_lava {
            self.fire_left = 15.0;
        } else if self.in_water || world.rains_on(head) {
            self.fire_left = 0.0;
        } else if sunburn || in_fire {
            self.fire_left = 8.0;
        } else {
            self.fire_left = (self.fire_left - dt).max(0.0);
        }
        let wet = physics::touches_block(world, self.pos, self.shape(), Block::holds_water) || world.rains_on(head);
        // `LivingEntity.aiStep`: `isSensitiveToWater` mobs (endermen, blazes,
        // striders, snow golems) take `hurtServer(drown, 1)` each tick in
        // water or rain, which the hurt cooldown spaces (v1.0 gap Track 8d).
        if (self.kind.hurt_by_water() || self.kind == MobKind::SnowGolem) && wet && self.alive() {
            self.damage_from(1.0, Cause::DROWN, rng);
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
        // v1.0 Track 5: `lava`, `in_fire` and `on_fire` damage, so armor
        // and Fire Protection reduce it as Java's do. v1.0 gap Track 8d:
        // lava (`lavaHurt`, 4) and fire (`BaseFireBlock.entityInside`, 1)
        // hurt every tick they touch, which the hurt cooldown spaces to one
        // in ten ticks; burning hurts once a second outside lava
        // (`remainingFireTicks % 20`), refused too while a hit as hard is
        // cooling down.
        if in_lava {
            self.burn_timer = 0.0;
            self.damage_from(4.0, Cause::LAVA, rng);
            return;
        }
        if in_fire {
            self.damage_from(1.0, Cause::IN_FIRE, rng);
        }
        self.burn_timer += dt;
        if self.burn_timer >= 1.0 && self.alive() {
            self.burn_timer -= 1.0;
            self.damage_from(if sunburn { 2.0 } else { 1.0 }, Cause::ON_FIRE, rng);
        }
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
pub(super) fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

pub(super) fn turn_toward(from: f32, to: f32, max: f32) -> f32 {
    let d = wrap(to - from);
    wrap(from + d.clamp(-max, max))
}

/// v1.0 Track 3: the long-session soak (`crate::soak`) sums path lengths.
impl Mob {
    pub(super) fn soak_path_len(&self) -> usize {
        self.nav.path.as_ref().map_or(0, |p| p.nodes.len())
    }
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
        // v1.0 gap Track 8d: the water's `hurtServer(drown, 1)` comes from
        // `aiStep` in a whole update now, not the physics step.
        world.set(IVec3::new(0, 12, 0), Block::WATER);
        let mut m = Mob::new(MobKind::SnowGolem, DVec3::new(0.5, 11., 0.5), 0.);
        m.effects.add(Effect::Resistance, 2, 400);
        let before = m.health;
        let c = crate::entity::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![crate::entity::Target::new(
                crate::entity::PlayerId::HOST,
                DVec3::new(60.5, 10.0, 0.5),
                false,
            )],
            daylight: 0.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        m.update(0.05, &world, &c, &mut Rng::new(1), &mut Vec::new());
        assert!((before - m.health - 0.4).abs() < 1e-6, "{}", before - m.health);
    }
}

impl super::Entities {
    /// The target selector's `HurtByTargetGoal` after a player hurt mob
    /// `index`: dolphins and blazes turn on them and alert their kind.
    /// A player hurt mob `index`. Its `HurtByTargetGoal` decides on the next
    /// update, as Java's does on the next selector update, about the latest
    /// attacker only (`getLastHurtByMob`).
    pub(super) fn hurt_by_player_target(&mut self, index: usize, player: super::PlayerId) {
        if self.mobs.get(index).is_some_and(|m| {
            matches!(
                m.kind,
                MobKind::Wither
                    | MobKind::Dolphin
                    | MobKind::Blaze
                    | MobKind::Bee
                    | MobKind::PolarBear
                    | MobKind::Panda
                    | MobKind::Armadillo
                    | MobKind::Endermite
                    | MobKind::Shulker
            ) || m.illager.is_some()
                || super::neutral::is_neutral(m.kind)
        }) {
            self.ensure_mob_ids();
            let uid = self.mobs[index].uid;
            match self.hurt_by.iter_mut().find(|(u, _)| *u == uid) {
                Some(hit) => hit.1 = player,
                None => self.hurt_by.push((uid, player)),
            }
        }
    }

    /// `HurtByTargetGoal.canUse` for the waiting hits: `canAttack(attacker,
    /// HURT_BY_TARGETING)` refuses creative and spectating players, so their
    /// hits change no target and alert nobody.
    pub(super) fn resolve_hurt_by(&mut self, ctx: &super::Ctx) {
        for (uid, player) in std::mem::take(&mut self.hurt_by) {
            let Some(index) = self.mobs.iter().position(|m| m.uid == uid) else { continue };
            let kind = self.mobs[index].kind;
            // `ResetUniversalAngerTargetGoal` takes a player's hit instead of
            // `HurtByTargetGoal` under `universal_anger`, with no attack check.
            if self.neutral_universal_anger && kind != MobKind::PolarBear && super::neutral::is_neutral(kind) {
                self.universal_anger_hit(index);
                continue;
            }
            let defensive = self.mobs[index].kind == MobKind::Armadillo;
            if !ctx
                .players
                .iter()
                .any(|t| t.id == player && if defensive { t.alive && !t.spectator } else { t.targetable })
            {
                continue;
            }
            match self.mobs[index].kind {
                MobKind::Bee => self.anger_bees(self.mobs[index].pos.floor().as_ivec3(), player),
                MobKind::Dolphin => self.dolphin_hurt_by(index, player),
                MobKind::Blaze => self.blaze_hurt_by(index, player),
                MobKind::Wither => self.wither_hurt_by(index, player),
                MobKind::PolarBear => self.polar_hurt(index, super::nether::Foe::Player(player)),
                MobKind::Panda => self.panda_hurt(index, super::nether::Foe::Player(player)),
                MobKind::Armadillo => self.armadillo_hurt(index, super::nether::Foe::Player(player)),
                MobKind::Endermite => self.endermite_hurt(index, super::nether::Foe::Player(player)),
                MobKind::Shulker => self.shulker_hurt_by(index, player),
                _ if self.mobs[index].illager.is_some() => self.illager_hurt(index, super::nether::Foe::Player(player)),
                MobKind::Wolf | MobKind::IronGolem | MobKind::Enderman | MobKind::ZombifiedPiglin => {
                    self.neutral_hurt_by(index, player)
                }
                _ => {}
            }
        }
    }

    /// `Blaze`'s `HurtByTargetGoal(this).setAlertOthers()`: the attacker
    /// becomes the target of the blaze and of every blaze without one within
    /// its 48-block follow range (10 up and down).
    fn blaze_hurt_by(&mut self, index: usize, player: super::PlayerId) {
        let Some(pos) = self.mobs.get(index).filter(|m| m.alive()).map(|m| m.pos) else { return };
        for (j, m) in self.mobs.iter_mut().enumerate() {
            let d = m.pos - pos;
            let near = (-48.0..49.0).contains(&d.x) && (-48.0..49.0).contains(&d.z) && (-10.0..11.0).contains(&d.y);
            if m.kind == MobKind::Blaze && m.alive() && (j == index || near && m.move_ctl.target.is_none()) {
                m.hurt_by_target(player);
            }
        }
    }
}

/// v1.0 gap Track 7: the motion Java's client draws mobs with, through the
/// mob's own tick ([`super::motion`] tests the formulas themselves).
#[cfg(test)]
mod motion_tests {
    use super::*;
    use crate::entity::{PlayerId, Target, motion};
    use crate::physics::test_util::Grid;

    fn ctx(player: DVec3) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId::HOST, player, false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    /// `calculateEntityAnimation`: the walk speed eases toward four times
    /// the distance moved in the tick (vertical distance only for bees and
    /// parrots); babies' legs cycle three times as fast; dead or riding
    /// mobs stop.
    #[test]
    fn the_walk_follows_the_distance_moved_each_tick() {
        let mut pig = Mob::new(MobKind::Pig, DVec3::ZERO, 0.0);
        pig.advance_walk(DVec3::new(0.1, 0.5, 0.0));
        assert!(close(pig.limb_amp, 0.16) && close(pig.limb_phase, 0.16), "{} {}", pig.limb_amp, pig.limb_phase);
        pig.previous_pose = pig.pose_values();
        pig.advance_walk(DVec3::new(0.0, 0.0, 0.1));
        assert!(close(pig.limb_amp, 0.256) && close(pig.limb_phase, 0.416));
        // Drawn half way through the tick, a step behind by half its speed.
        let half = pig.render_pose(0.5);
        assert!(close(half[3], 0.416 - 0.128) && close(half[4], 0.208), "{half:?}");
        pig.baby = true;
        assert!(close(pig.render_pose(1.0)[3], 0.416 * motion::BABY_WALK_SCALE));

        let mut bee = Mob::new(MobKind::Bee, DVec3::ZERO, 0.0);
        bee.advance_walk(DVec3::new(0.0, 0.1, 0.0));
        assert!(close(bee.limb_amp, 0.16), "a bee counts its climb");

        pig.dying = Some(0.0);
        pig.health = 0.0;
        pig.advance_walk(DVec3::new(0.1, 0.0, 0.0));
        assert_eq!((pig.limb_phase, pig.limb_amp), (0.0, 0.0), "`walkAnimation.stop()`");
        let mut rider = Mob::new(MobKind::Skeleton, DVec3::ZERO, 0.0);
        rider.limb_amp = 0.5;
        rider.riding = Some(7);
        rider.advance_walk(DVec3::new(0.1, 0.0, 0.0));
        assert_eq!(rider.limb_amp, 0.0);
    }

    /// `handleDamageEvent`: a hit sets the walk speed to 1.5, which the
    /// legs show capped at 1 and which eases back over the next ticks.
    #[test]
    fn a_hit_kicks_the_legs() {
        let mut rng = Rng::new(3);
        let mut m = Mob::new(MobKind::Cow, DVec3::ZERO, 0.0);
        assert!(!m.damage(1.0, None, &mut rng), "not killed");
        assert_eq!(m.limb_amp, motion::HURT_KICK);
        m.previous_pose = m.pose_values();
        assert_eq!(m.render_pose(0.5)[4], 1.0);
        m.advance_walk(DVec3::ZERO);
        assert!(close(m.limb_amp, 0.9) && close(m.limb_phase, 0.9));
    }

    /// An attack (here set between ticks, as a raid's or a mount's code
    /// does) starts Java's 6-tick swing on the next tick: 0, 1/6, ... 1.
    #[test]
    fn an_attack_swings_the_arm_over_six_ticks() {
        let world = Grid::flat(10);
        let c = ctx(DVec3::new(60.5, 10.0, 0.5));
        let (mut rng, mut events) = (Rng::new(5), Vec::new());
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5), 0.0);
        m.update(0.05, &world, &c, &mut rng, &mut events);
        assert!(!m.swing.swinging());
        m.attack_anim = 0.35;
        let mut seen = Vec::new();
        for _ in 0..9 {
            m.update(0.05, &world, &c, &mut rng, &mut events);
            seen.push(m.swing.anim);
        }
        let want = [0.0, 1.0 / 6.0, 2.0 / 6.0, 0.5, 4.0 / 6.0, 5.0 / 6.0, 1.0, 0.0, 0.0];
        assert!(seen.iter().zip(want).all(|(a, b)| close(*a, b)), "{seen:?}");
    }

    /// `deathTime` counts the ticks since death; the overlay is red from
    /// the hit through the death, and off ten ticks after a hit.
    #[test]
    fn death_time_counts_ticks_with_the_partial_tick() {
        let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
        assert_eq!(m.death_ticks(0.7), 0.0);
        m.dying = Some(0.0);
        assert_eq!(m.death_ticks(0.7), 0.0, "drawn upright in the tick it died");
        m.dying = Some(3.0 * motion::TICK);
        assert!(close(m.death_ticks(0.25), 3.25));
        // Removed between its 19th and 20th tick of `dying`.
        let ticks = std::hint::black_box(DEATH_TIME) / motion::TICK;
        assert!(ticks > 18.0 && ticks < 19.0, "{ticks}");
    }
}

/// v1.0 gap Track 8d: `LivingEntity.hurtServer`'s hurt cooldown
/// (`damageCooldownTime`, `lastHurt`) on mobs.
#[cfg(test)]
mod hurt_cooldown_tests {
    use super::*;
    use crate::entity::{PlayerId, Target};
    use crate::physics::test_util::Grid;

    fn ctx() -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId::HOST, DVec3::new(60.5, 10.0, 0.5), false)],
            daylight: 0.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    /// A hit sets the cooldown to 20; until it has run down to 10 a hit no
    /// harder than the last is refused outright, and a harder one deals
    /// only its excess, with no flash, sound, leg kick or knockback; the
    /// tenth tick after a full hit takes a full hit again.
    #[test]
    fn hits_inside_the_cooldown_land_only_by_their_excess() {
        let world = Grid::flat(10);
        let c = ctx();
        let (mut rng, mut events) = (Rng::new(5), Vec::new());
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5), 0.0);
        m.update(0.05, &world, &c, &mut rng, &mut events);
        assert!(!m.damage(4.0, Some(DVec3::new(4.0, 2.0, 0.0)), &mut rng));
        assert_eq!((m.health, m.damage_cooldown, m.last_hurt), (16.0, COOLDOWN, 4.0));
        assert_eq!(m.vel.x, 4.0, "a full hit takes its knockback");
        assert!(matches!(m.cry.take(), Some(MobSound::Hurt(MobKind::Zombie))));
        // As hard, or softer: refused (and an arrow would bounce).
        assert!(m.refuses_hit(4.0) && m.refuses_hit(1.0) && !m.refuses_hit(5.0));
        m.vel = DVec3::ZERO;
        m.hurt = 0.0;
        m.damage(3.0, Some(DVec3::new(4.0, 2.0, 0.0)), &mut rng);
        assert_eq!(m.health, 16.0);
        // Harder: the excess, quietly, with no push.
        m.damage(6.0, Some(DVec3::new(4.0, 2.0, 0.0)), &mut rng);
        assert_eq!((m.health, m.last_hurt), (14.0, 6.0));
        assert_eq!((m.vel.x, m.hurt), (0.0, 0.0), "no knockback, no red flash");
        assert!(m.cry.is_none(), "no hurt sound");
        // Nine ticks on, still cooling (11 left); the tenth makes it 10.
        for _ in 0..9 {
            m.update(0.05, &world, &c, &mut rng, &mut events);
        }
        assert!(m.refuses_hit(1.0));
        m.update(0.05, &world, &c, &mut rng, &mut events);
        assert!(m.full_hit() && !m.refuses_hit(1.0));
        m.damage(1.0, None, &mut rng);
        assert_eq!((m.health, m.damage_cooldown), (13.0, COOLDOWN));
    }

    /// Lava hurts every tick it is touched (`lavaHurt`), so the cooldown
    /// is what spaces it: 4 a half second, not every tick.
    #[test]
    fn lava_hurts_once_per_half_cooldown() {
        let mut world = Grid::flat(10);
        world.set(glam::IVec3::new(0, 10, 0), crate::world::block::Block::LAVA);
        let c = ctx();
        let (mut rng, mut events) = (Rng::new(5), Vec::new());
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5), 0.0);
        for _ in 0..20 {
            m.update(0.05, &world, &c, &mut rng, &mut events);
            m.pos = DVec3::new(0.5, 10.0, 0.5);
        }
        assert_eq!(m.health, 20.0 - 8.0, "two lava hits in twenty ticks");
    }

    /// The guardian's beam on a mob: `indirectMagic` then the attack in
    /// the same tick, the second landing only by what it adds.
    #[test]
    fn a_second_hit_in_the_same_tick_adds_only_its_excess() {
        let mut rng = Rng::new(1);
        let mut m = Mob::new(MobKind::Squid, DVec3::ZERO, 0.0);
        m.damage(1.0, Some(DVec3::ZERO), &mut rng);
        m.damage(6.0, Some(DVec3::ZERO), &mut rng);
        assert_eq!(m.health, 10.0 - 6.0);
    }

    /// Snow golems in water take `hurtServer(drown, 1)` a tick, which the
    /// cooldown spaces to 1 a half second (it was 20 a second).
    #[test]
    fn a_snow_golem_in_water_takes_one_a_half_second() {
        let mut world = Grid::flat(10);
        world.set(glam::IVec3::new(0, 10, 0), crate::world::block::Block::WATER);
        world.set(glam::IVec3::new(0, 11, 0), crate::world::block::Block::WATER);
        let c = ctx();
        let (mut rng, mut events) = (Rng::new(5), Vec::new());
        let mut m = Mob::new(MobKind::SnowGolem, DVec3::new(0.5, 10.0, 0.5), 0.0);
        for _ in 0..20 {
            m.update(0.05, &world, &c, &mut rng, &mut events);
            m.pos = DVec3::new(0.5, 10.0, 0.5);
        }
        assert_eq!(m.health, 4.0 - 2.0);
    }
}
