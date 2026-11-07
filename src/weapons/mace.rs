//! Java 26.3 `MaceItem`: the smash attack and what Density, Breach and Wind
//! Burst add to it. Velocities here are in blocks per tick, as in Java;
//! callers scale by [`crate::knockback::TICK_RATE`].

use glam::DVec3;

use crate::enchant::{Enchantment, Enchants};

/// `MaceItem.SMASH_ATTACK_FALL_THRESHOLD`: falls longer than this smash.
pub const SMASH_FALL_THRESHOLD: f64 = 1.5;
/// `SMASH_ATTACK_HEAVY_THRESHOLD`: falls longer than this smash heavily
/// (doubled knockback, the heavy ground sound).
pub const HEAVY_THRESHOLD: f64 = 5.0;
/// `SMASH_ATTACK_KNOCKBACK_RADIUS`.
pub const KNOCKBACK_RADIUS: f64 = 3.5;
/// `SMASH_ATTACK_KNOCKBACK_POWER` (a Java `float`).
pub const KNOCKBACK_POWER: f64 = 0.7f32 as f64;
/// `hurtEnemy` leaves the attacker rising this slowly (blocks per tick).
pub const BOUNCE: f64 = 0.01f32 as f64;
/// `LivingEntity.applyPostImpulseGraceTime(40)` from `setIgnoreFallDamageFromCurrentImpulse`.
pub const IMPULSE_GRACE_TICKS: u32 = 40;
/// Wind Burst's `minecraft:explode` radius.
pub const WIND_BURST_RADIUS: f32 = 3.5;

/// `MaceItem.canSmashAttack`: fallen more than 1.5 blocks and not gliding.
pub fn can_smash(fall_distance: f64, fall_flying: bool) -> bool {
    fall_distance > SMASH_FALL_THRESHOLD && !fall_flying
}

/// `MaceItem.getAttackDamageBonus`: 4 per block for the first 3, 2 per block
/// to 8, then 1 per block, plus Density's `smash_damage_per_fallen_block`
/// (0.5 per level) times the whole fall. Zero when the fall doesn't smash.
pub fn smash_bonus(fall_distance: f64, fall_flying: bool, enchants: Enchants) -> f32 {
    if !can_smash(fall_distance, fall_flying) {
        return 0.0;
    }
    let f = fall_distance;
    let damage = if f <= 3.0 {
        4.0 * f
    } else if f <= 8.0 {
        12.0 + 2.0 * (f - 3.0)
    } else {
        22.0 + f - 8.0
    };
    let density = 0.5 * f64::from(enchants.level(Enchantment::Density)) as f32;
    (damage + f64::from(density) * f) as f32
}

/// Breach's `armor_effectiveness` change: -0.15 per level.
pub fn breach_effectiveness(enchants: Enchants) -> f32 {
    -0.15 * f32::from(enchants.level(Enchantment::Breach))
}

/// `MaceItem.getKnockbackPower`: how hard the smash pushes a living thing
/// `distance` blocks from the target (zero or less pushes nothing).
pub fn knockback_power(distance: f64, heavy: bool, knockback_resistance: f64) -> f64 {
    (KNOCKBACK_RADIUS - distance) * KNOCKBACK_POWER * if heavy { 2.0 } else { 1.0 } * (1.0 - knockback_resistance)
}

/// The push `MaceItem.knockback` gives something at `nearby` around a target
/// at `target` (`nearby.push(dir * power, 0.7, …)`), in blocks per tick, or
/// `None` out of range or with no power left.
pub fn smash_push(target: DVec3, nearby: DVec3, heavy: bool, knockback_resistance: f64) -> Option<DVec3> {
    let direction = nearby - target;
    if direction.length_squared() > KNOCKBACK_RADIUS * KNOCKBACK_RADIUS {
        return None;
    }
    let power = knockback_power(direction.length(), heavy, knockback_resistance);
    if power <= 0.0 {
        return None;
    }
    let horizontal = direction.normalize_or_zero() * power;
    Some(DVec3::new(horizontal.x, KNOCKBACK_POWER, horizontal.z))
}

/// Wind Burst's `knockback_multiplier`: `lookup [1.2, 1.75, 2.2]`, then
/// `linear 1.5 + 0.35 per level above the first`.
pub fn wind_burst_multiplier(level: u8) -> f32 {
    match level {
        0 => 0.0,
        1 => 1.2,
        2 => 1.75,
        3 => 2.2,
        l => 1.5 + 0.35 * f32::from(l - 1),
    }
}

/// Wind Burst's `post_attack` requirement: the attacker isn't flying and
/// has fallen at least 1.5 blocks (`fall_distance {min: 1.5}`, inclusive).
pub fn wind_burst_applies(enchants: Enchants, fall_distance: f64, flying: bool) -> bool {
    enchants.has(Enchantment::WindBurst) && !flying && fall_distance >= SMASH_FALL_THRESHOLD
}

/// `ServerExplosion.hurtEntities` knockback of Wind Burst's blast at
/// `center` on something whose position is `pos` and eye `eye`, `exposure`
/// in view: `(1 - dist / 2r) * exposure * multiplier * (1 - resistance)`
/// along `eye - center`, in blocks per tick. `None` beyond `2r`.
pub fn wind_burst_push(
    center: DVec3,
    pos: DVec3,
    eye: DVec3,
    exposure: f32,
    multiplier: f32,
    resistance: f64,
) -> Option<DVec3> {
    let double_radius = f64::from(WIND_BURST_RADIUS * 2.0);
    let dist = pos.distance(center) / double_radius;
    if dist > 1.0 {
        return None;
    }
    let power = (1.0 - dist) * f64::from(exposure) * f64::from(multiplier) * (1.0 - resistance);
    Some((eye - center).normalize_or_zero() * power)
}

/// The sound `hurtEnemy` plays: an air smash, or a ground smash that is
/// heavy after a fall of more than five blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmashSound {
    Air,
    Ground,
    Heavy,
}

impl SmashSound {
    pub fn of(target_on_ground: bool, fall_distance: f64) -> Self {
        if !target_on_ground {
            Self::Air
        } else if fall_distance > HEAVY_THRESHOLD {
            Self::Heavy
        } else {
            Self::Ground
        }
    }
}

/// Wind Burst's `block_interaction: trigger` around `center`, through the
/// wind charge's block pass (doors, trapdoors, gates, buttons, levers,
/// bells and candles; nothing breaks). The push on players and mobs is
/// [`wind_burst_push`]'s; the gust particles are not drawn yet.
pub fn wind_burst_blocks(world: &mut crate::world::World, center: DVec3) {
    let burst = crate::entity::wind::WindBurst::new(center, WIND_BURST_RADIUS, 0.0, false, None);
    world.wind_trigger(&burst);
}
