//! Java 26.3 spears: `Item$Properties.spear`, the `KineticWeapon` charge
//! (held use) and the `PiercingWeapon` jab (the attack key), with their
//! `AttackRange`. Speeds are in blocks per second, as Java's
//! `getKnownSpeed().scale(20)`.

use crate::item::Tier;

/// The arguments each `Items.*_SPEAR` registration passes to `spear(...)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    /// Seconds per jab (`ATTACK_ANIMATION` ticks and the attack speed).
    pub attack_duration: f32,
    pub damage_multiplier: f32,
    /// Seconds of charging before the kinetic attack starts.
    pub delay: f32,
    pub dismount_time: f32,
    pub dismount_threshold: f32,
    pub knockback_time: f32,
    pub knockback_threshold: f32,
    pub damage_time: f32,
    pub damage_threshold: f32,
}

#[allow(clippy::too_many_arguments)]
const fn s(
    attack_duration: f32,
    damage_multiplier: f32,
    delay: f32,
    dismount_time: f32,
    dismount_threshold: f32,
    knockback_time: f32,
    knockback_threshold: f32,
    damage_time: f32,
    damage_threshold: f32,
) -> Stats {
    Stats {
        attack_duration,
        damage_multiplier,
        delay,
        dismount_time,
        dismount_threshold,
        knockback_time,
        knockback_threshold,
        damage_time,
        damage_threshold,
    }
}

/// `Items.WOODEN_SPEAR` … `NETHERITE_SPEAR`.
pub const fn stats(tier: Tier) -> Stats {
    match tier {
        Tier::Wood => s(0.65, 0.7, 0.75, 5.0, 14.0, 10.0, 5.1, 15.0, 4.6),
        Tier::Stone => s(0.75, 0.82, 0.7, 4.5, 13.0, 9.0, 5.1, 13.75, 4.6),
        Tier::Copper => s(0.85, 0.82, 0.65, 4.0, 12.0, 8.25, 5.1, 12.5, 4.6),
        Tier::Iron => s(0.95, 0.95, 0.6, 2.5, 11.0, 6.75, 5.1, 11.25, 4.6),
        Tier::Gold => s(0.95, 0.7, 0.7, 3.5, 13.0, 8.5, 5.1, 13.75, 4.6),
        Tier::Diamond => s(1.05, 1.075, 0.5, 3.0, 10.0, 6.5, 5.1, 10.0, 4.6),
        Tier::Netherite => s(1.15, 1.2, 0.4, 2.5, 9.0, 5.5, 5.1, 8.75, 4.6),
    }
}

/// `KineticWeapon.Condition`: active for `max_ticks` ticks of use after the
/// delay, at no less than these speeds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Condition {
    pub max_ticks: u32,
    pub min_speed: f32,
    pub min_relative_speed: f32,
}

impl Condition {
    /// `Condition.test`: mobs use the 0.2 action factor, players 1.
    pub fn test(self, ticks_used: u32, attacker_speed: f64, relative_speed: f64, factor: f64) -> bool {
        ticks_used <= self.max_ticks
            && attacker_speed >= f64::from(self.min_speed) * factor
            && relative_speed >= f64::from(self.min_relative_speed) * factor
    }
}

/// `KineticWeapon` as `Item$Properties.spear` builds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kinetic {
    /// `contact_cooldown_ticks`: one target is stabbed at most this often.
    pub contact_cooldown: u32,
    pub delay_ticks: u32,
    pub dismount: Condition,
    pub knockback: Condition,
    pub damage: Condition,
    pub damage_multiplier: f32,
}

/// Java's `(int)(seconds * 20.0F)`.
fn ticks(seconds: f32) -> u32 {
    (seconds * 20.0) as u32
}

pub fn kinetic(tier: Tier) -> Kinetic {
    let s = stats(tier);
    let attacker =
        |time: f32, speed: f32| Condition { max_ticks: ticks(time), min_speed: speed, min_relative_speed: 0.0 };
    Kinetic {
        contact_cooldown: 10,
        delay_ticks: ticks(s.delay),
        dismount: attacker(s.dismount_time, s.dismount_threshold),
        knockback: attacker(s.knockback_time, s.knockback_threshold),
        damage: Condition { max_ticks: ticks(s.damage_time), min_speed: 0.0, min_relative_speed: s.damage_threshold },
        damage_multiplier: s.damage_multiplier,
    }
}

/// What one contact of a charged spear does (`KineticWeapon.damageEntities`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub damage: bool,
    pub knockback: bool,
    pub dismount: bool,
    /// `baseMobDamage + floor(relativeSpeed * damageMultiplier)`.
    pub amount: f32,
}

impl Kinetic {
    /// The charge's effect on a target after `used` ticks of use, with the
    /// attacker's speed along its look and the target's. `base` is the
    /// attacker's base attack damage (1 for a player). `None` before the
    /// delay ends or when no condition holds.
    pub fn contact(
        self,
        used: u32,
        attacker_along: f64,
        target_along: f64,
        base: f32,
        player: bool,
    ) -> Option<Contact> {
        let used = used.checked_sub(self.delay_ticks)?;
        let factor = if player { 1.0 } else { 0.2 };
        let relative = (attacker_along - target_along).max(0.0);
        let dismount = self.dismount.test(used, attacker_along, relative, factor);
        let knockback = self.knockback.test(used, attacker_along, relative, factor);
        let damage = self.damage.test(used, attacker_along, relative, factor);
        (dismount || knockback || damage).then(|| Contact {
            damage,
            knockback,
            dismount,
            amount: base + (relative * f64::from(self.damage_multiplier)).floor() as f32,
        })
    }

    /// `computeDamageUseDuration`: ticks of use the charge can still hurt.
    pub fn damage_use_duration(self) -> u32 {
        self.delay_ticks + self.damage.max_ticks
    }
}

/// `AttackRange(2.0, 4.5, 2.0, 6.5, 0.125, 0.5)` for every spear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub margin: f64,
}

pub const MIN_REACH: f32 = 2.0;
pub const MAX_REACH: f32 = 4.5;
pub const MIN_CREATIVE_REACH: f32 = 2.0;
pub const MAX_CREATIVE_REACH: f32 = 6.5;
pub const HITBOX_MARGIN: f32 = 0.125;
pub const MOB_FACTOR: f32 = 0.5;

/// `AttackRange.effectiveMinRange`/`effectiveMaxRange` for a player.
pub fn player_range(creative: bool) -> Range {
    let (min, max) = if creative { (MIN_CREATIVE_REACH, MAX_CREATIVE_REACH) } else { (MIN_REACH, MAX_REACH) };
    Range { min: f64::from(min), max: f64::from(max), margin: f64::from(HITBOX_MARGIN) }
}

/// The same range for a mob (scaled by `mob_factor`).
pub fn mob_range() -> Range {
    Range {
        min: f64::from(MIN_REACH * MOB_FACTOR),
        max: f64::from(MAX_REACH * MOB_FACTOR),
        margin: f64::from(HITBOX_MARGIN),
    }
}

/// Lunge's `post_piercing_attack` impulse: the look direction with its
/// vertical part dropped (`coordinate_scale [1, 0, 1]`), times 0.458 per
/// level, in blocks per tick. Not normalized: looking down lunges less.
pub fn lunge_impulse(level: u8, look: glam::DVec3) -> glam::DVec3 {
    let magnitude = 0.458f32 * f32::from(level);
    glam::DVec3::new(look.x, 0.0, look.z) * f64::from(magnitude)
}

/// Lunge's requirements: not riding, not gliding, not in water, and a
/// player has at least 7 food unless in Creative.
pub fn lunge_allowed(riding: bool, gliding: bool, in_water: bool, food: u32, creative: bool) -> bool {
    !riding && !gliding && !in_water && (creative || food >= 7)
}

/// Lunge's `apply_exhaustion`: 4 per level.
pub fn lunge_exhaustion(level: u8) -> f32 {
    4.0 * f32::from(level)
}
