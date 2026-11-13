//! Java 26.3 `TridentItem` and `ThrownTrident` numbers shared by the
//! player's throw, Riptide and the thrown entity (`entity::trident`).

use crate::enchant::{Enchantment, Enchants};

/// `TridentItem.THROW_THRESHOLD_TIME`: shorter holds neither throw nor launch.
pub const THROW_THRESHOLD: u32 = 10;
/// `TridentItem.BASE_DAMAGE` (thrown).
pub const BASE_DAMAGE: f32 = 8.0;
/// `PROJECTILE_SHOOT_POWER`, blocks per tick.
pub const SHOOT_POWER: f32 = 2.5;
/// `Projectile.getMovementToShoot`'s spread per unit of uncertainty.
pub const SPREAD: f64 = 0.0172275;
/// `AbstractArrow.getDefaultGravity`.
pub const GRAVITY: f64 = 0.05;
/// `AbstractArrow.getAirDrag`.
pub const AIR_DRAG: f64 = 0.99f32 as f64;
/// `ThrownTrident.getWaterInertia` (arrows use 0.6).
pub const WATER_INERTIA: f64 = 0.99f32 as f64;
/// `AbstractArrow.tickDespawn`: ticks in the ground before it's discarded.
pub const DESPAWN_TICKS: u32 = 1200;
/// `AbstractArrow.SHAKE_TIME`.
pub const SHAKE_TICKS: u8 = 7;
/// `startAutoSpinAttack(20, 8.0F, stack)`.
pub const SPIN_TICKS: u32 = 20;
pub const SPIN_DAMAGE: f32 = 8.0;
/// `TridentItem.releaseUsing` lifts a grounded Riptide user this far.
pub const RIPTIDE_LIFT: f64 = 1.1999999f32 as f64;
/// Drowned throw from `DrownedTridentAttackGoal(this, 1.0, 40, 10.0F)`.
pub const DROWNED_ATTACK_INTERVAL: u32 = 40;
pub const DROWNED_ATTACK_RADIUS: f64 = 10.0;
/// `Drowned.performRangedAttack`'s shoot power.
pub const DROWNED_SHOOT_POWER: f32 = 1.6;

/// Riptide's `trident_spin_attack_strength`: 1.5 plus 0.75 per level above
/// the first, in blocks per tick (0 without Riptide).
pub fn riptide_strength(enchants: Enchants) -> f32 {
    let level = enchants.level(Enchantment::Riptide);
    if level == 0 { 0.0 } else { 1.5 + 0.75 * f32::from(level - 1) }
}

/// Loyalty's `trident_return_acceleration`: 1 per level, clamped to 0..127.
pub fn loyalty(enchants: Enchants) -> u8 {
    enchants.level(Enchantment::Loyalty).min(127)
}

/// `Mob.rangedAttackUncertainty`: `14 - difficulty * 4` (Peaceful 0 … Hard 3).
pub fn mob_uncertainty(difficulty_id: u8) -> f32 {
    14.0 - 4.0 * f32::from(difficulty_id)
}

/// Java's `(yRot, xRot)` in degrees, the shooter's view: `-sin(y)cos(x),
/// -sin(x), cos(y)cos(x)` (`Projectile.shootFromRotation`).
pub fn rotation_vector(y_rot: f32, x_rot: f32) -> glam::DVec3 {
    let (y, x) = (y_rot.to_radians(), x_rot.to_radians());
    glam::DVec3::new(f64::from(-y.sin() * x.cos()), f64::from(-x.sin()), f64::from(y.cos() * x.cos()))
}
