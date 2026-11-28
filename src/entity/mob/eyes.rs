//! v1.0 gap Track 2d: each kind's eye height (Java 26.3's
//! `Entity.getEyeHeight`, from `EntityDimensions.eyeHeight`).
//!
//! `EntityTypes` gives each type's dimensions and, for most, an
//! `eyeHeight`; without one it is `0.85` of the height
//! (`EntityDimensions.scalable`). `LivingEntity.getDimensions` gives
//! `SLEEPING_DIMENSIONS` (eye 0.2) to a sleeping mob and otherwise the
//! kind's `getDefaultDimensions`: the type's dimensions scaled by
//! `getAgeScale` (0.5 for babies unless the kind says otherwise), each
//! kind's own `BABY_DIMENSIONS` with its `withEyeHeight`, and the pose and
//! size overrides (camels sitting, a sniffer digging, a warden digging or
//! emerging, phantoms and cubes by size, a shulker's lid on the floor).
//! `EntityDimensions.scale` scales the eye with the height.
use super::{Mob, MobKind};

/// The adult eye height of a kind (`EntityTypes`' `eyeHeight`, or 0.85 of
/// its height).
fn adult(kind: MobKind) -> f32 {
    use MobKind as K;
    match kind {
        K::Allay => 0.36,
        K::Armadillo => 0.26,
        K::Axolotl => 0.2751,
        K::Bat => 0.45,
        K::Bee => 0.3,
        K::Blaze => 1.8 * 0.85,
        K::Skeleton | K::Stray | K::Bogged | K::Parched => 1.74,
        K::Breeze => 1.3452,
        K::Camel | K::CamelHusk => 2.275,
        K::Cat => 0.35,
        K::CaveSpider => 0.45,
        K::Chicken => 0.644,
        K::Cod => 0.195,
        K::Cow | K::Mooshroom => 1.3,
        K::CopperGolem => 0.8125,
        K::Creaking => 2.3,
        K::Creeper => 1.7 * 0.85,
        K::Dolphin => 0.3,
        K::Donkey => 1.425,
        K::Zombie | K::Husk | K::Drowned | K::ZombieVillager => 1.74,
        K::ElderGuardian => 0.99875,
        K::Enderman => 2.55,
        K::Endermite | K::Silverfish => 0.13,
        K::Evoker | K::Illusioner | K::Pillager | K::Vindicator => 1.95 * 0.85,
        K::Fox => 0.4,
        K::Frog => 0.5 * 0.85,
        K::Ghast | K::HappyGhast => 2.6,
        K::GlowSquid | K::Squid => 0.4,
        K::Goat => 1.3 * 0.85,
        K::Guardian => 0.425,
        K::Hoglin | K::Zoglin => 1.4 * 0.85,
        K::Horse | K::Mule | K::SkeletonHorse | K::ZombieHorse => 1.52,
        K::IronGolem => 2.7 * 0.85,
        K::Llama | K::TraderLlama => 1.7765,
        K::MagmaCube | K::Slime => 0.325,
        K::Nautilus | K::ZombieNautilus => 0.2751,
        K::Ocelot => 0.7 * 0.85,
        K::Panda => 1.25 * 0.85,
        K::Parrot => 0.54,
        K::Phantom => 0.175,
        K::Pig => 0.9 * 0.85,
        K::Piglin | K::PiglinBrute | K::ZombifiedPiglin => 1.79,
        K::PolarBear => 1.4 * 0.85,
        K::Pufferfish => 0.455,
        K::Rabbit => 0.59,
        K::Ravager => 2.2 * 0.85,
        K::Salmon | K::TropicalFish => 0.26,
        K::Sheep => 1.235,
        K::Shulker => 0.5,
        K::Sniffer => 1.05,
        K::SnowGolem => 1.7,
        K::Spider => 0.65,
        K::Strider => 1.7 * 0.85,
        K::SulfurCube => 0.175,
        K::Tadpole => 0.195_000_01,
        K::Turtle => 0.4 * 0.85,
        K::Vex => 0.518_75,
        K::Villager | K::WanderingTrader | K::Witch => 1.62,
        K::Warden => 2.9 * 0.85,
        K::Wither => 3.5 * 0.85,
        K::WitherSkeleton => 2.1,
        K::Wolf => 0.68,
    }
}

/// A baby's eye height: the kind's `BABY_DIMENSIONS.withEyeHeight`, or the
/// adult's scaled by its `getAgeScale`.
fn baby_eye(kind: MobKind) -> f32 {
    use MobKind as K;
    match kind {
        K::Sheep => 0.656_25,
        K::Dolphin => 0.093_75,
        K::Armadillo => 0.218_75,
        K::Cat | K::Ocelot => 0.343_75,
        K::Squid | K::GlowSquid => 0.37,
        K::Pig => 0.406_25,
        K::Chicken => 0.281_25,
        K::Panda => 0.281_25,
        K::Rabbit => 0.39,
        K::Cow | K::Mooshroom => 0.69,
        K::Camel | K::CamelHusk => 1.38,
        K::Turtle => adult(kind) * 0.3,
        K::PolarBear => 0.343_75,
        K::HappyGhast => 0.468_75,
        K::Horse | K::SkeletonHorse | K::ZombieHorse => adult(kind) * 0.7,
        K::Axolotl => 0.093_75,
        K::Goat => 0.593_75,
        K::Fox | K::Wolf => 0.343_75,
        K::Strider => 0.4375,
        K::Hoglin | K::Zoglin => 0.625,
        K::Piglin | K::ZombifiedPiglin => 0.78,
        K::Zombie | K::Drowned => 0.775,
        K::Husk => 0.825,
        K::ZombieVillager => 0.67,
        K::Villager => 0.63,
        // `AbstractChestedHorse.babyDimensions`, `Llama.BABY_DIMENSIONS`,
        // `Nautilus.BABY_DIMENSIONS` and `LivingEntity.getAgeScale`.
        _ => adult(kind) * 0.5,
    }
}

impl Mob {
    /// Java's `getEyeHeight()` for this mob's age, pose and size.
    pub(crate) fn eye_height(&self) -> f64 {
        f64::from(self.eye_height_f32())
    }

    fn eye_height_f32(&self) -> f32 {
        let baby = self.baby || self.age < 0;
        // `Pose.SLEEPING`: `SLEEPING_DIMENSIONS`.
        if self.villager.as_ref().is_some_and(|v| v.sleeping) {
            return 0.2;
        }
        match self.kind {
            MobKind::Warden if self.warden.as_ref().is_some_and(|s| s.digging_or_emerging()) => {
                // `EntityDimensions.fixed(width, 1)`.
                return 0.85;
            }
            MobKind::Sniffer if self.sniffer.as_ref().is_some_and(|s| s.pose == super::super::sniffer::DIGGING) => {
                return 0.81 * if baby { 0.5 } else { 1.0 };
            }
            MobKind::Camel | MobKind::CamelHusk if self.mount.as_ref().is_some_and(|s| s.camel_sitting) => {
                return if baby { 0.41 } else { 0.845 };
            }
            MobKind::Phantom => {
                let size = self.phantom.as_ref().map_or(0, |p| p.size);
                return adult(self.kind) * (1.0 + 0.15 * f32::from(size));
            }
            MobKind::Slime | MobKind::MagmaCube | MobKind::SulfurCube => {
                return adult(self.kind) * f32::from(self.size);
            }
            MobKind::Shulker => {
                // `Shulker.getDefaultDimensions`: on the floor the lid lifts
                // the box by `currentPeekAmount`.
                let s = self.shulker.as_ref();
                let lift = s
                    .filter(|s| s.attach == crate::world::end_blocks::Dir::Down && s.amount > 0.0)
                    .map_or(0.0, |s| s.amount);
                return adult(self.kind) * (1.0 + lift);
            }
            _ => {}
        }
        if baby { baby_eye(self.kind) } else { adult(self.kind) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_heights_follow_javas_entity_types() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(adult(MobKind::Zombie), 1.74));
        assert!(close(adult(MobKind::Creeper), 1.445));
        assert!(close(adult(MobKind::IronGolem), 2.295));
        assert!(close(adult(MobKind::Villager), 1.62));
        assert!(close(baby_eye(MobKind::Villager), 0.63));
        assert!(close(baby_eye(MobKind::Bee), 0.15));
        assert!(close(baby_eye(MobKind::Horse), 1.064));
        assert!(close(baby_eye(MobKind::Turtle), 0.102));
        assert!(close(adult(MobKind::Spider), 0.65));
    }
}
