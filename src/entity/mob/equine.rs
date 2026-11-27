//! v1.0 Track 7 (FOO-04c): a ridden horse, donkey, mule, skeleton horse or
//! zombie horse moves a whole Java tick at a time, as Java 26.3's
//! `LivingEntity.travelRidden` with `AbstractHorse`'s overrides does:
//!
//! - `getRiddenInput`: half the rider's strafe (Java's `xxa`, positive to
//!   the left) and the rider's forward input, a quarter of it backwards.
//! - `tickRidden`: the horse takes the rider's yaw, and on the ground a
//!   released jump (`executeRidersJump`) sets the vertical speed to
//!   `JUMP_STRENGTH × scale × block jump factor + Jump Boost` and, moving
//!   forward, adds `0.4 × scale` along the heading. The scale is 1 for a
//!   charge of 90% or more, else `0.4 + 0.4 × charge / 90` (the charge in
//!   whole percent, `Mth.floor(jumpRidingScale * 100)`).
//! - `travel` with the `MOVEMENT_SPEED` attribute: `travelInAir`'s
//!   friction-influenced speed, `STEP_HEIGHT` 1, gravity 0.08 and drag 0.98.
//! - `causeFallDamage`: `floor((fall + 1.0E-6 - SAFE_FALL_DISTANCE 6) ×
//!   FALL_DAMAGE_MULTIPLIER 0.5)` to the horse, and only when that hurts,
//!   each rider's own fall damage for the same distance
//!   (`propagateFallToPassengers`): a player's `floor(fall + 1.0E-6 - 3)`.
//!
//! In water or lava the horse keeps the earlier continuous ride.

use glam::DVec3;

use super::{Mob, MobKind, swim};
use crate::physics::BlockSource;
use crate::world::block::Block;

/// `AbstractHorse.createBaseHorseAttributes`: `STEP_HEIGHT`,
/// `SAFE_FALL_DISTANCE` and `FALL_DAMAGE_MULTIPLIER`.
pub const STEP_HEIGHT: f64 = 1.0;
pub const SAFE_FALL: f64 = 6.0;
pub const FALL_MULTIPLIER: f64 = 0.5;
/// `executeRidersJump`'s forward push per unit of jump scale.
pub const JUMP_FORWARD: f32 = 0.4;
/// A player's `SAFE_FALL_DISTANCE`.
pub const PLAYER_SAFE_FALL: f64 = 3.0;

/// `AbstractHorse.getPlayerJumpPendingScale` for the rider's jump charge
/// (0..=1 as the jump bar shows it).
pub fn jump_scale(charge: f32) -> f32 {
    let amount = (charge * 100.0).floor() as i32;
    if amount >= 90 { 1.0 } else { 0.4 + 0.4 * amount.max(0) as f32 / 90.0 }
}

/// `LivingEntity.calculateFallDamage` with a safe distance and multiplier.
pub fn fall_damage(fall: f64, safe: f64, multiplier: f64) -> f32 {
    ((fall + 1.0E-6 - safe) * multiplier).floor().max(0.0) as f32
}

impl Mob {
    /// One `travelRidden` tick for a steered horse. `self.vel` is in blocks
    /// a second, as elsewhere.
    pub(super) fn equine_ridden_tick<W: BlockSource + ?Sized>(&mut self, world: &W) {
        let Some(s) = self.mount.as_mut() else { return };
        // `getRiddenInput`.
        let sideways = -f64::from(s.strafe) * 0.5;
        let mut forward = f64::from(s.forward);
        if forward <= 0.0 {
            forward *= 0.25;
        }
        let input = DVec3::new(sideways, 0.0, forward);
        // `tickRidden`: the rider's heading, then a released jump.
        self.yaw = s.yaw;
        let mut dm = self.vel / 20.0;
        if self.on_ground
            && let Some(charge) = s.take_pending_jump()
        {
            let scale = jump_scale(charge);
            let below = world.block(super::super::item::below_pos(self.pos));
            let boost = 0.1 * self.effects.jump_boost() as f32;
            let power = s.jump as f32 * scale * crate::player::travel::jump_factor(below) + boost;
            dm.y = power as f64;
            if input.z > 0.0 {
                // `getYRot()`: the rider's, just taken.
                let r = (s.yaw.to_degrees() - 90.0) * (std::f32::consts::PI / 180.0);
                let (sin, cos) = (crate::knockback::mth_sin(r as f64), crate::knockback::mth_cos(r as f64));
                dm.x += f64::from(-JUMP_FORWARD * sin * scale);
                dm.z += f64::from(JUMP_FORWARD * cos * scale);
            }
        }
        // `setSpeed(getRiddenSpeed)`, then `travel(input)`.
        let speed = s.speed;
        self.move_ctl.cur = speed;
        let from_y = self.pos.y;
        self.travel_in_air(world, &mut dm, input);
        self.vel = dm * 20.0;
        self.equine_fall(world, from_y);
    }

    /// Fall distance and `causeFallDamage` after a ridden tick.
    fn equine_fall<W: BlockSource + ?Sized>(&mut self, world: &W, from_y: f64) {
        let below = world.block(super::super::item::below_pos(self.pos));
        let Some(s) = self.mount.as_mut() else { return };
        s.fall_distance += (from_y - self.pos.y).max(0.0);
        if !self.on_ground {
            return;
        }
        let fall = std::mem::take(&mut s.fall_distance);
        // `Block.fallOn`'s damage modifier: none on slime, a fifth on hay.
        let modifier = match below {
            Some(crate::world::stone_blocks::SLIME_BLOCK) => 0.0,
            Some(Block::HAY_BALE) => 0.2f32 as f64,
            _ => 1.0,
        };
        let damage = fall_damage(fall, SAFE_FALL, FALL_MULTIPLIER * modifier);
        if damage <= 0.0 {
            return;
        }
        if s.rider.is_some() {
            s.rider_fall += fall_damage(fall, PLAYER_SAFE_FALL, modifier);
        }
        self.health -= self.effects.absorb(damage * self.effects.resistance_factor());
    }
}

/// Whether a steered `kind` rides with [`Mob::equine_ridden_tick`].
pub fn java_ridden(kind: MobKind) -> bool {
    matches!(kind, MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::SkeletonHorse | MobKind::ZombieHorse)
}

/// `swim::TICK`, for the caller's accumulator.
pub(super) const TICK: f64 = swim::TICK;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::PlayerId;
    use crate::inventory::Stack;
    use crate::item::Item;
    use crate::physics::test_util::Grid;

    /// A tamed, saddled horse on stone at y = 64 with a rider steering.
    fn ridden(jump: f64, speed: f64) -> Mob {
        let mut m = Mob::new(MobKind::Horse, DVec3::new(0.5, 64.0, 0.5), 0.0);
        m.on_ground = true;
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.rider = Some(PlayerId::HOST);
        s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        s.jump = jump;
        s.speed = speed;
        m
    }

    #[test]
    fn jump_scale_follows_the_charge_in_whole_percent() {
        assert_eq!(jump_scale(1.0), 1.0);
        assert_eq!(jump_scale(0.9), 1.0);
        assert_eq!(jump_scale(0.45), 0.4 + 0.4 * 45.0 / 90.0);
        assert_eq!(jump_scale(0.0), 0.4);
    }

    #[test]
    fn a_released_jump_launches_at_the_jump_strength_and_leaps_forward() {
        let w = Grid::flat(64);
        let mut m = ridden(0.7, 0.225);
        // Java yaw 0: forward is +z.
        m.mount.as_mut().unwrap().yaw = std::f32::consts::FRAC_PI_2;
        m.mount.as_mut().unwrap().forward = 1.0;
        m.mount.as_mut().unwrap().pending_jump_for_test(1.0);
        m.equine_ridden_tick(&w);
        // The tick moves by the launch (0.7 up), then gravity and drag:
        // (0.7 - 0.08) * 0.98.
        // (`JUMP_STRENGTH` is read as a float: 0.7f.)
        assert!((m.pos.y - 64.7).abs() < 1e-6, "{}", m.pos.y);
        assert!((m.vel.y / 20.0 - (0.7 - 0.08) * 0.98).abs() < 1e-6);
        // `executeRidersJump` added 0.4 forward, and `travelInAir` (still
        // reading last tick's ground) the ground speed 0.225.
        assert!(m.vel.z > 0.0 && (m.pos.z - 0.5 - (0.4 + 0.225)).abs() < 1e-6, "{}", m.pos.z);
        // Successive ticks carry it to the peak Java's equations give: the
        // sum of the rising speeds v, (v - 0.08) * 0.98, ... from 0.7f.
        let (mut v, mut expected) = (0.7f32 as f64, 0.0);
        while v > 0.0 {
            expected += v;
            v = (v - 0.08) * 0.98;
        }
        let mut peak = m.pos.y;
        for _ in 0..40 {
            m.equine_ridden_tick(&w);
            peak = peak.max(m.pos.y);
        }
        assert!((peak - 64.0 - expected).abs() < 1e-6, "{} vs {expected}", peak - 64.0);
    }

    #[test]
    fn walking_reaches_javas_ground_speed_and_backing_is_a_quarter() {
        let w = Grid::flat(64);
        for (forward, scale) in [(1.0f32, 1.0), (-1.0, -0.25)] {
            let mut m = ridden(0.7, 0.3);
            m.mount.as_mut().unwrap().yaw = std::f32::consts::FRAC_PI_2;
            m.mount.as_mut().unwrap().forward = forward;
            for _ in 0..100 {
                m.equine_ridden_tick(&w);
            }
            // On stone (0.6): each tick adds the speed (0.216 / 0.216 of it)
            // then keeps 0.546, so after the friction v = speed * 0.546 / (1 - 0.546).
            let top = 0.3 * scale * 0.546 / (1.0 - 0.6 * 0.91);
            assert!((m.vel.z / 20.0 - top).abs() < 1e-6, "{} vs {top}", m.vel.z / 20.0);
        }
    }

    #[test]
    fn falls_floor_and_pass_on_only_when_they_hurt_the_horse() {
        assert_eq!(fall_damage(7.0, SAFE_FALL, FALL_MULTIPLIER), 0.0);
        assert_eq!(fall_damage(8.0, SAFE_FALL, FALL_MULTIPLIER), 1.0);
        assert_eq!(fall_damage(11.0, SAFE_FALL, FALL_MULTIPLIER), 2.0);
        let w = Grid::flat(64);
        let mut m = ridden(0.7, 0.225);
        m.pos.y = 74.5;
        m.on_ground = false;
        for _ in 0..60 {
            m.equine_ridden_tick(&w);
            if m.on_ground {
                break;
            }
        }
        let s = m.mount.as_ref().unwrap();
        // Ten and a half blocks: the horse takes floor(4.5 * 0.5) = 2, the
        // rider floor(10.5 - 3) = 7.
        assert_eq!(s.rider_fall, 7.0);
        assert!((m.mount.as_ref().unwrap().max_health - m.health - 2.0).abs() < 1e-6 || m.health <= 53.0);
    }

    #[test]
    fn horses_step_a_full_block() {
        let mut w = Grid::flat(64);
        for z in -2..3 {
            w.set(glam::IVec3::new(2, 64, z), Block::STONE);
        }
        let mut m = ridden(0.7, 0.3);
        m.mount.as_mut().unwrap().yaw = 0.0;
        m.mount.as_mut().unwrap().forward = 1.0;
        let mut top = m.pos.y;
        for _ in 0..40 {
            m.equine_ridden_tick(&w);
            top = top.max(m.pos.y);
        }
        // Up onto the one-block wall at x = 2 without a jump, and off it.
        assert!(m.pos.x > 3.0 && top >= 65.0, "{:?} peak {top}", m.pos);
    }
}
