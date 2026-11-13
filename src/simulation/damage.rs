//! Java 26.3's damage types and damage type tags
//! (`data/minecraft/damage_type/*.json`, `data/minecraft/tags/damage_type/*.json`,
//! `net.minecraft.world.damagesource.DamageType`/`DamageTypes`/`DamageSources`).
//!
//! Every hit on a player carries a [`Cause`]: its Java [`DamageType`] and the
//! death message VoxelCraft shows ("Player <text>"). The damage rules read the
//! type's tags (armor, Resistance, Fire Resistance, the protection
//! enchantments, the totem and the player damage game rules), never the text.
//! `tests/java_damage_types.rs` checks the table against the extracted fixture.

/// `DamageScaling`: whether difficulty scales the damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageScaling {
    Never,
    WhenCausedByLivingNonPlayer,
    Always,
}

/// `DamageEffects`: the hurt sound family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DamageEffects {
    Hurt,
    Thorns,
    Drowning,
    Burning,
    Poking,
    Freezing,
}

/// `DeathMessageType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeathMessageType {
    Default,
    FallVariants,
    IntentionalGameDesign,
}

macro_rules! ids {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident = $id:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        $vis enum $name { $($variant,)* }

        impl $name {
            /// Every value, in Java registry id order.
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            /// The Java registry id, without the `minecraft:` namespace.
            pub const fn id(self) -> &'static str {
                match self { $($name::$variant => $id,)* }
            }

            /// The value with this Java id (with or without `minecraft:`).
            pub fn from_id(id: &str) -> Option<Self> {
                let id = id.strip_prefix("minecraft:").unwrap_or(id);
                match id { $($id => Some($name::$variant),)* _ => None }
            }
        }
    };
}

ids! {
    /// A Java damage type (`Registries.DAMAGE_TYPE`). Not saved: a dead
    /// player's save keeps the death text, and the type is derived when hit.
    /// New Java types go at the end.
    pub enum DamageType {
        Arrow = "arrow",
        BadRespawnPoint = "bad_respawn_point",
        Cactus = "cactus",
        Campfire = "campfire",
        Cramming = "cramming",
        DragonBreath = "dragon_breath",
        Drown = "drown",
        DryOut = "dry_out",
        EnderPearl = "ender_pearl",
        Explosion = "explosion",
        Fall = "fall",
        FallingAnvil = "falling_anvil",
        FallingBlock = "falling_block",
        FallingStalactite = "falling_stalactite",
        Fireball = "fireball",
        Fireworks = "fireworks",
        FlyIntoWall = "fly_into_wall",
        Freeze = "freeze",
        Generic = "generic",
        GenericKill = "generic_kill",
        HotFloor = "hot_floor",
        InFire = "in_fire",
        InWall = "in_wall",
        IndirectMagic = "indirect_magic",
        Lava = "lava",
        LightningBolt = "lightning_bolt",
        MaceSmash = "mace_smash",
        Magic = "magic",
        MobAttack = "mob_attack",
        MobAttackNoAggro = "mob_attack_no_aggro",
        MobProjectile = "mob_projectile",
        OnFire = "on_fire",
        OutOfWorld = "out_of_world",
        OutsideBorder = "outside_border",
        PlayerAttack = "player_attack",
        PlayerExplosion = "player_explosion",
        SonicBoom = "sonic_boom",
        Spear = "spear",
        Spit = "spit",
        Stalagmite = "stalagmite",
        Starve = "starve",
        Sting = "sting",
        SulfurCubeHot = "sulfur_cube_hot",
        SweetBerryBush = "sweet_berry_bush",
        Thorns = "thorns",
        Thrown = "thrown",
        Trident = "trident",
        UnattributedFireball = "unattributed_fireball",
        WindCharge = "wind_charge",
        Wither = "wither",
        WitherSkull = "wither_skull",
    }
}

ids! {
    /// A Java damage type tag (`DamageTypeTags`).
    pub enum DamageTag {
        AlwaysHurtsEnderDragons = "always_hurts_ender_dragons",
        AlwaysKillsArmorStands = "always_kills_armor_stands",
        AlwaysMostSignificantFall = "always_most_significant_fall",
        AlwaysTriggersSilverfish = "always_triggers_silverfish",
        AvoidsGuardianThorns = "avoids_guardian_thorns",
        BurnFromStepping = "burn_from_stepping",
        BurnsArmorStands = "burns_armor_stands",
        BypassesArmor = "bypasses_armor",
        BypassesCooldown = "bypasses_cooldown",
        BypassesEffects = "bypasses_effects",
        BypassesEnchantments = "bypasses_enchantments",
        BypassesInvulnerability = "bypasses_invulnerability",
        BypassesResistance = "bypasses_resistance",
        BypassesShield = "bypasses_shield",
        BypassesWolfArmor = "bypasses_wolf_armor",
        CanBreakArmorStand = "can_break_armor_stand",
        DamagesHelmet = "damages_helmet",
        IgnitesArmorStands = "ignites_armor_stands",
        IsDrowning = "is_drowning",
        IsExplosion = "is_explosion",
        IsFall = "is_fall",
        IsFire = "is_fire",
        IsFreezing = "is_freezing",
        IsLightning = "is_lightning",
        IsPlayerAttack = "is_player_attack",
        IsProjectile = "is_projectile",
        MaceSmash = "mace_smash",
        NoAnger = "no_anger",
        NoImpact = "no_impact",
        NoKnockback = "no_knockback",
        NoWolfRetaliation = "no_wolf_retaliation",
        PanicCauses = "panic_causes",
        PanicEnvironmentalCauses = "panic_environmental_causes",
        SulfurCubeWithBlockImmuneTo = "sulfur_cube_with_block_immune_to",
        WitchResistantTo = "witch_resistant_to",
        WitherImmuneTo = "wither_immune_to",
    }
}

impl DamageType {
    /// `message_id`: the `death.attack.<id>` translation key suffix.
    pub const fn message_id(self) -> &'static str {
        use DamageType as T;
        match self {
            T::Arrow => "arrow",
            T::BadRespawnPoint => "badRespawnPoint",
            T::Cactus => "cactus",
            T::Campfire | T::InFire => "inFire",
            T::Cramming => "cramming",
            T::DragonBreath => "dragonBreath",
            T::Drown => "drown",
            T::DryOut => "dryout",
            T::EnderPearl | T::Fall => "fall",
            T::Explosion => "explosion",
            T::FallingAnvil => "anvil",
            T::FallingBlock => "fallingBlock",
            T::FallingStalactite => "fallingStalactite",
            T::Fireball => "fireball",
            T::Fireworks => "fireworks",
            T::FlyIntoWall => "flyIntoWall",
            T::Freeze => "freeze",
            T::Generic => "generic",
            T::GenericKill => "genericKill",
            T::HotFloor => "hotFloor",
            T::InWall => "inWall",
            T::IndirectMagic => "indirectMagic",
            T::Lava => "lava",
            T::LightningBolt => "lightningBolt",
            T::MaceSmash => "mace_smash",
            T::Magic => "magic",
            T::MobAttack | T::MobAttackNoAggro | T::MobProjectile | T::Spit | T::WindCharge => "mob",
            T::OnFire | T::UnattributedFireball => "onFire",
            T::OutOfWorld => "outOfWorld",
            T::OutsideBorder => "outsideBorder",
            T::PlayerAttack => "player",
            T::PlayerExplosion => "explosion.player",
            T::SonicBoom => "sonic_boom",
            T::Spear => "spear",
            T::Stalagmite => "stalagmite",
            T::Starve => "starve",
            T::Sting => "sting",
            T::SulfurCubeHot => "sulfurCubeHot",
            T::SweetBerryBush => "sweetBerryBush",
            T::Thorns => "thorns",
            T::Thrown => "thrown",
            T::Trident => "trident",
            T::Wither => "wither",
            T::WitherSkull => "witherSkull",
        }
    }

    /// Food exhaustion a hit costs when it takes health (`Player.actuallyHurt`).
    pub const fn exhaustion(self) -> f32 {
        use DamageType as T;
        match self {
            T::OnFire
            | T::InWall
            | T::Cramming
            | T::Drown
            | T::Starve
            | T::Fall
            | T::EnderPearl
            | T::FlyIntoWall
            | T::OutOfWorld
            | T::Generic
            | T::Magic
            | T::Wither
            | T::DragonBreath
            | T::Freeze
            | T::Stalagmite
            | T::IndirectMagic
            | T::SonicBoom
            | T::OutsideBorder
            | T::GenericKill => 0.0,
            _ => 0.1,
        }
    }

    pub const fn scaling(self) -> DamageScaling {
        use DamageType as T;
        match self {
            T::Explosion | T::PlayerExplosion | T::SonicBoom | T::BadRespawnPoint => DamageScaling::Always,
            _ => DamageScaling::WhenCausedByLivingNonPlayer,
        }
    }

    pub const fn effects(self) -> DamageEffects {
        use DamageType as T;
        match self {
            T::InFire
            | T::Campfire
            | T::OnFire
            | T::Lava
            | T::HotFloor
            | T::SulfurCubeHot
            | T::UnattributedFireball
            | T::Fireball => DamageEffects::Burning,
            T::Drown => DamageEffects::Drowning,
            T::SweetBerryBush => DamageEffects::Poking,
            T::Freeze => DamageEffects::Freezing,
            T::Thorns => DamageEffects::Thorns,
            _ => DamageEffects::Hurt,
        }
    }

    pub const fn death_message_type(self) -> DeathMessageType {
        match self {
            DamageType::Fall | DamageType::EnderPearl => DeathMessageType::FallVariants,
            DamageType::BadRespawnPoint => DeathMessageType::IntentionalGameDesign,
            _ => DeathMessageType::Default,
        }
    }

    /// Whether this type is in `tag` (`DamageSource.is(TagKey)`), with the
    /// tags' `#tag` references expanded.
    pub fn is(self, tag: DamageTag) -> bool {
        use DamageTag as G;
        use DamageType as T;
        match tag {
            G::AlwaysHurtsEnderDragons => self.is(G::IsExplosion),
            G::AlwaysKillsArmorStands => {
                matches!(self, T::Arrow | T::Trident | T::Fireball | T::WitherSkull | T::WindCharge)
            }
            G::AlwaysMostSignificantFall => self == T::OutOfWorld,
            G::AlwaysTriggersSilverfish => self == T::Magic,
            G::AvoidsGuardianThorns => matches!(self, T::Magic | T::Thorns) || self.is(G::IsExplosion),
            G::BurnFromStepping => matches!(self, T::Campfire | T::HotFloor | T::SulfurCubeHot),
            G::BurnsArmorStands => self == T::OnFire,
            G::BypassesArmor => matches!(
                self,
                T::OnFire
                    | T::InWall
                    | T::Cramming
                    | T::Drown
                    | T::FlyIntoWall
                    | T::Generic
                    | T::Wither
                    | T::DragonBreath
                    | T::Starve
                    | T::Fall
                    | T::EnderPearl
                    | T::Freeze
                    | T::Stalagmite
                    | T::Magic
                    | T::IndirectMagic
                    | T::OutOfWorld
                    | T::GenericKill
                    | T::SonicBoom
                    | T::OutsideBorder
            ),
            G::BypassesCooldown => false,
            G::BypassesEffects => self == T::Starve,
            G::BypassesEnchantments => self == T::SonicBoom,
            G::BypassesInvulnerability | G::BypassesResistance => matches!(self, T::OutOfWorld | T::GenericKill),
            G::BypassesShield => {
                self.is(G::BypassesArmor)
                    || matches!(
                        self,
                        T::Cactus
                            | T::Campfire
                            | T::DryOut
                            | T::FallingAnvil
                            | T::FallingStalactite
                            | T::HotFloor
                            | T::SulfurCubeHot
                            | T::InFire
                            | T::Lava
                            | T::LightningBolt
                            | T::SweetBerryBush
                    )
            }
            G::BypassesWolfArmor => {
                self.is(G::BypassesInvulnerability)
                    || matches!(
                        self,
                        T::Cramming
                            | T::Drown
                            | T::DryOut
                            | T::Freeze
                            | T::InWall
                            | T::IndirectMagic
                            | T::Magic
                            | T::OutsideBorder
                            | T::Starve
                            | T::Thorns
                            | T::Wither
                    )
            }
            G::CanBreakArmorStand => self == T::PlayerExplosion || self.is(G::IsPlayerAttack),
            G::DamagesHelmet => matches!(self, T::FallingAnvil | T::FallingBlock | T::FallingStalactite),
            G::IgnitesArmorStands => matches!(self, T::InFire | T::Campfire),
            G::IsDrowning => self == T::Drown,
            G::IsExplosion => matches!(self, T::Fireworks | T::Explosion | T::PlayerExplosion | T::BadRespawnPoint),
            G::IsFall => matches!(self, T::Fall | T::EnderPearl | T::Stalagmite),
            G::IsFire => matches!(
                self,
                T::InFire
                    | T::Campfire
                    | T::OnFire
                    | T::Lava
                    | T::HotFloor
                    | T::SulfurCubeHot
                    | T::UnattributedFireball
                    | T::Fireball
            ),
            G::IsFreezing => self == T::Freeze,
            G::IsLightning => self == T::LightningBolt,
            G::IsPlayerAttack => matches!(self, T::PlayerAttack | T::Spear | T::MaceSmash),
            G::IsProjectile => matches!(
                self,
                T::Arrow
                    | T::Trident
                    | T::MobProjectile
                    | T::UnattributedFireball
                    | T::Fireball
                    | T::WitherSkull
                    | T::Thrown
                    | T::WindCharge
            ),
            G::MaceSmash => self == T::MaceSmash,
            G::NoAnger => self == T::MobAttackNoAggro,
            G::NoImpact => self == T::Drown,
            G::NoKnockback => matches!(
                self,
                T::Explosion
                    | T::PlayerExplosion
                    | T::BadRespawnPoint
                    | T::InFire
                    | T::LightningBolt
                    | T::OnFire
                    | T::Lava
                    | T::HotFloor
                    | T::SulfurCubeHot
                    | T::InWall
                    | T::Cramming
                    | T::Drown
                    | T::Starve
                    | T::Cactus
                    | T::Fall
                    | T::EnderPearl
                    | T::FlyIntoWall
                    | T::OutOfWorld
                    | T::Generic
                    | T::Magic
                    | T::Wither
                    | T::DragonBreath
                    | T::DryOut
                    | T::SweetBerryBush
                    | T::Freeze
                    | T::Stalagmite
                    | T::OutsideBorder
                    | T::GenericKill
                    | T::Campfire
                    | T::Spear
            ),
            G::NoWolfRetaliation => self == T::SulfurCubeHot,
            G::PanicCauses => {
                self.is(G::PanicEnvironmentalCauses)
                    || self.is(G::IsPlayerAttack)
                    || matches!(
                        self,
                        T::Arrow
                            | T::DragonBreath
                            | T::Explosion
                            | T::Fireball
                            | T::Fireworks
                            | T::IndirectMagic
                            | T::Magic
                            | T::MobAttack
                            | T::MobProjectile
                            | T::PlayerExplosion
                            | T::SonicBoom
                            | T::Sting
                            | T::Thrown
                            | T::Trident
                            | T::UnattributedFireball
                            | T::WindCharge
                            | T::Wither
                            | T::WitherSkull
                    )
            }
            G::PanicEnvironmentalCauses => matches!(
                self,
                T::Cactus
                    | T::Freeze
                    | T::HotFloor
                    | T::SulfurCubeHot
                    | T::InFire
                    | T::Lava
                    | T::LightningBolt
                    | T::OnFire
            ),
            G::SulfurCubeWithBlockImmuneTo => {
                self.is(G::IsExplosion)
                    || matches!(
                        self,
                        T::Arrow
                            | T::Cactus
                            | T::DryOut
                            | T::Fall
                            | T::FallingAnvil
                            | T::FallingBlock
                            | T::FallingStalactite
                            | T::Freeze
                            | T::MaceSmash
                            | T::HotFloor
                            | T::MobAttack
                            | T::MobAttackNoAggro
                            | T::MobProjectile
                            | T::PlayerAttack
                            | T::Spear
                            | T::Spit
                            | T::Stalagmite
                            | T::Sting
                            | T::SulfurCubeHot
                            | T::SweetBerryBush
                            | T::Thrown
                            | T::Trident
                            | T::WindCharge
                    )
            }
            G::WitchResistantTo => matches!(self, T::Magic | T::IndirectMagic | T::SonicBoom | T::Thorns),
            G::WitherImmuneTo => self == T::Drown,
        }
    }
}

/// What hurt a player: the Java damage type and the death message
/// VoxelCraft shows for it ("Player <text>"). The text is what a dead
/// player's save keeps; the rules only read `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cause {
    pub kind: DamageType,
    pub text: &'static str,
}

impl Cause {
    pub const fn new(kind: DamageType, text: &'static str) -> Self {
        Self { kind, text }
    }

    /// A mob's melee hit (`DamageSources.mobAttack`).
    pub const fn mob(text: &'static str) -> Self {
        Self::new(DamageType::MobAttack, text)
    }

    /// Whether the damage type is in `tag`.
    pub fn is(self, tag: DamageTag) -> bool {
        self.kind.is(tag)
    }

    // Environment (`DamageSources`' fixed sources).
    pub const FALL: Cause = Cause::new(DamageType::Fall, super::survival::CAUSE_FALL);
    /// The agent's landing message (Java's `death.attack.fall`).
    pub const FALL_HARD: Cause = Cause::new(DamageType::Fall, "hit the ground too hard");
    /// `ThrownEnderpearl.onHit`'s 5 damage (`enderPearl()`).
    pub const ENDER_PEARL: Cause = Cause::new(DamageType::EnderPearl, super::survival::CAUSE_FALL);
    pub const DROWN: Cause = Cause::new(DamageType::Drown, super::survival::CAUSE_DROWN);
    pub const LAVA: Cause = Cause::new(DamageType::Lava, super::survival::CAUSE_LAVA);
    /// Burning after leaving the fire (`onFire()`).
    pub const ON_FIRE: Cause = Cause::new(DamageType::OnFire, super::survival::CAUSE_FIRE);
    /// Standing in a fire block (`BaseFireBlock.entityInside`, `inFire()`).
    pub const IN_FIRE: Cause = Cause::new(DamageType::InFire, "went up in flames");
    /// Standing on magma (`MagmaBlock.stepOn`, `hotFloor()`).
    pub const HOT_FLOOR: Cause = Cause::new(DamageType::HotFloor, "discovered the floor was lava");
    /// A lit campfire (`CampfireBlock.entityInside`, `campfire()`).
    pub const CAMPFIRE: Cause = Cause::new(DamageType::Campfire, "went up in flames");
    pub const STARVE: Cause = Cause::new(DamageType::Starve, super::survival::CAUSE_STARVE);
    pub const FALLING_STALACTITE: Cause = Cause::new(DamageType::FallingStalactite, super::survival::CAUSE_STALACTITE);
    pub const FREEZE: Cause = Cause::new(DamageType::Freeze, super::survival::CAUSE_FREEZE);
    /// Drunk or commanded Instant Damage (`magic()`). VoxelCraft also uses it
    /// for splash potions, which Java types `indirect_magic`.
    pub const MAGIC: Cause = Cause::new(DamageType::Magic, super::survival::CAUSE_MAGIC);
    pub const WITHER: Cause = Cause::new(DamageType::Wither, "withered away");
    /// `/kill` (`genericKill()`).
    pub const GENERIC_KILL: Cause = Cause::new(DamageType::GenericKill, "was killed");
    pub const OUT_OF_WORLD: Cause = Cause::new(DamageType::OutOfWorld, "fell out of the world");
    pub const LIGHTNING: Cause = Cause::new(DamageType::LightningBolt, "was struck by lightning");

    // Projectiles.
    pub const ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by an arrow");
    pub const PLAYER_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by a player");
    pub const SKELETON_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by a skeleton");
    pub const STRAY_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by a stray");
    pub const BOGGED_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by a bogged");
    pub const PIGLIN_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by a piglin");
    pub const PILLAGER_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by a pillager");
    pub const DROWNED_TRIDENT: Cause = Cause::new(DamageType::Trident, "was impaled by a drowned");
    pub const PLAYER_TRIDENT: Cause = Cause::new(DamageType::Trident, "was impaled by a player");
    pub const BREEZE_WIND_CHARGE: Cause = Cause::new(DamageType::WindCharge, "was slain by a breeze");
    pub const WIND_CHARGE: Cause = Cause::new(DamageType::WindCharge, "was blasted by a wind charge");
    pub const LLAMA_SPIT: Cause = Cause::new(DamageType::Spit, "was spat on by a llama");
    /// A small fireball's direct hit (`fireball(fireball, blaze)`).
    pub const BLAZE_FIREBALL: Cause = Cause::new(DamageType::Fireball, "was fireballed by a blaze");

    // Explosions (`DamageSources.explosion`: `player_explosion` when the
    // blast has both a direct and an indirect living source).
    pub const CREEPER: Cause = Cause::new(DamageType::PlayerExplosion, "was blown up by a creeper");
    /// A ghast fireball's blast (`LargeFireball.onHit`'s explosion, owned by the ghast).
    pub const GHAST_FIREBALL: Cause = Cause::new(DamageType::PlayerExplosion, "was fireballed by a ghast");
    /// Unowned primed TNT.
    pub const TNT: Cause = Cause::new(DamageType::Explosion, "was blown up by TNT");
    pub const END_CRYSTAL: Cause = Cause::new(DamageType::Explosion, "was blown up by an End crystal");
    /// Beds outside the Overworld and respawn anchors outside the Nether.
    pub const BAD_RESPAWN_POINT: Cause =
        Cause::new(DamageType::BadRespawnPoint, "was killed by [Intentional Game Design]");

    // Mobs.
    /// `Guardian.hurtServer`'s spikes (`thorns(guardian)`).
    pub const GUARDIAN_SPIKES: Cause = Cause::new(DamageType::Thorns, "was pricked by a guardian");
    /// The laser's attack (`doHurtTarget`); VoxelCraft folds its
    /// `indirect_magic` part into the same hit.
    pub const GUARDIAN_BEAM: Cause = Cause::mob("was slain by a guardian");
    pub const BEE_STING: Cause = Cause::new(DamageType::Sting, "was stung by a bee");
    pub const PUFFERFISH: Cause = Cause::mob("was stung by a pufferfish");
    pub const GOAT_RAM: Cause = Cause::mob("was rammed by a goat");
    pub const ENDER_DRAGON: Cause = Cause::mob("was slain by the Ender Dragon");
    /// The dragon's breath cloud: Instant Damage from an owned area effect
    /// cloud, which `HealOrHarmMobEffect` types `indirect_magic`.
    pub const DRAGON_BREATH: Cause = Cause::new(DamageType::IndirectMagic, "was killed by dragon's breath");
    pub const WARDEN: Cause = Cause::mob("was slain by a warden");
    pub const SONIC_BOOM: Cause = Cause::new(DamageType::SonicBoom, "was obliterated by a sonically-charged shriek");

    // Players.
    pub const PLAYER_ATTACK: Cause = Cause::new(DamageType::PlayerAttack, "was slain by a player");
    pub const THORNS: Cause = Cause::new(DamageType::Thorns, "was killed trying to hurt a player");

    /// Every named cause, for the reference test and for [`Cause::legacy`]
    /// (the first cause with a text wins).
    pub const KNOWN: &'static [Cause] = &[
        Cause::FALL,
        Cause::FALL_HARD,
        Cause::ENDER_PEARL,
        Cause::DROWN,
        Cause::LAVA,
        Cause::ON_FIRE,
        Cause::IN_FIRE,
        Cause::HOT_FLOOR,
        Cause::CAMPFIRE,
        Cause::STARVE,
        Cause::FALLING_STALACTITE,
        Cause::FREEZE,
        Cause::MAGIC,
        Cause::WITHER,
        Cause::GENERIC_KILL,
        Cause::OUT_OF_WORLD,
        Cause::LIGHTNING,
        Cause::ARROW,
        Cause::PLAYER_ARROW,
        Cause::SKELETON_ARROW,
        Cause::STRAY_ARROW,
        Cause::BOGGED_ARROW,
        Cause::PIGLIN_ARROW,
        Cause::PILLAGER_ARROW,
        Cause::DROWNED_TRIDENT,
        Cause::PLAYER_TRIDENT,
        Cause::BREEZE_WIND_CHARGE,
        Cause::WIND_CHARGE,
        Cause::LLAMA_SPIT,
        Cause::BLAZE_FIREBALL,
        Cause::CREEPER,
        Cause::GHAST_FIREBALL,
        Cause::TNT,
        Cause::END_CRYSTAL,
        Cause::BAD_RESPAWN_POINT,
        Cause::GUARDIAN_SPIKES,
        Cause::GUARDIAN_BEAM,
        Cause::BEE_STING,
        Cause::PUFFERFISH,
        Cause::GOAT_RAM,
        Cause::ENDER_DRAGON,
        Cause::DRAGON_BREATH,
        Cause::WARDEN,
        Cause::SONIC_BOOM,
        Cause::PLAYER_ATTACK,
        Cause::THORNS,
    ];

    /// The cause for a bare death message, for callers that still pass
    /// text: a named cause with exactly this text, otherwise a mob's melee
    /// hit. New damage should name its [`Cause`] instead.
    pub fn legacy(text: &'static str) -> Self {
        Self::KNOWN.iter().copied().find(|c| c.text == text).unwrap_or(Self::mob(text))
    }
}

impl From<&'static str> for Cause {
    fn from(text: &'static str) -> Self {
        Self::legacy(text)
    }
}

impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text)
    }
}

/// Armor's share of a hit (`LivingEntity.getDamageAfterArmorAbsorb`):
/// `bypasses_armor` damage keeps its full amount and wears nothing.
/// Returns the damage left and whether the armor takes wear.
pub fn after_armor(amount: f32, cause: Cause, points: u32, toughness: f32) -> (f32, bool) {
    if cause.is(DamageTag::BypassesArmor) {
        (amount, false)
    } else {
        (super::survival::armor_reduce(amount, points, toughness), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_known_texts_resolve() {
        for &t in DamageType::ALL {
            assert_eq!(DamageType::from_id(t.id()), Some(t));
            assert_eq!(DamageType::from_id(&format!("minecraft:{}", t.id())), Some(t));
        }
        for &g in DamageTag::ALL {
            assert_eq!(DamageTag::from_id(g.id()), Some(g));
        }
        assert_eq!(Cause::from("was killed"), Cause::GENERIC_KILL);
        assert_eq!(Cause::from("fell from a high place"), Cause::FALL);
        assert_eq!(Cause::from("was slain by a zombie"), Cause::mob("was slain by a zombie"));
    }
}
