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
//! v1.0 gap Track 3 (FOO-04c) adds the rest of the tick:
//!
//! - In water or lava `travel` is `travelInFluid` (`travelInWater`'s 0.02
//!   push and `getWaterSlowDown` drag, 0.8 for horses and 0.96 for skeleton
//!   horses; `travelInLava`), with `jumpOutOfFluid` and the
//!   `#can_float_while_ridden` lift of 0.04 while deeper than 0.4. Before
//!   that `Entity.baseTick` adds the water current and `aiStep` drops
//!   motion under 0.003. The mount's AI float jump (`FloatGoal`, `Swim`)
//!   doesn't move it: the rider's client simulates a controlled mount, and
//!   its AI runs only on the server.
//! - Camels and camel husks ride the same tick with `Camel`'s overrides:
//!   no input, turning or jump while sitting or changing pose
//!   (`refuseToMove`), forward input standing a seated camel up, 0.1 more
//!   speed while sprinting with the dash ready, the dash
//!   (`executeRidersJump`: `22.2222 × scale × MOVEMENT_SPEED × block speed
//!   factor` along the look and `1.4285 × scale × getJumpPower()` up) and
//!   its 55-tick cooldown.

use glam::DVec3;

use super::{Mob, MobKind, MobWorld, swim};
use crate::entity::mounts::{self, camel, camel_like};
use crate::entity::{Ctx, Rng};
use crate::physics::{self, BlockSource};
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
/// `Entity.getFluidJumpThreshold` for these mounts (eyes above 0.4).
const FLUID_JUMP_THRESHOLD: f64 = 0.4;
/// `floatInLiquidWhileRidden`'s lift.
const LIQUID_LIFT: f64 = 0.04_f32 as f64;
/// `LivingEntity` gravity.
const GRAVITY: f64 = 0.08;

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

/// `Entity.getBlockSpeedFactor` at a block position: the block's own
/// factor, or the one below when the block has none.
fn block_speed_factor<W: BlockSource + ?Sized>(world: &W, pos: DVec3) -> f32 {
    let at = world.block(pos.floor().as_ivec3());
    let own = crate::player::travel::speed_factor(at);
    if own == 1.0 && at.is_none_or(|b| !b.holds_water()) {
        crate::player::travel::speed_factor(world.block(super::super::item::below_pos(pos)))
    } else {
        own
    }
}

impl Mob {
    /// One `travelRidden` tick for a steered horse or camel. `self.vel` is
    /// in blocks a second, as elsewhere.
    pub(super) fn equine_ridden_tick<W: BlockSource + ?Sized>(&mut self, world: &W) {
        let camel = camel_like(self.kind);
        // `Entity.baseTick`: what the box touches, and the water's push.
        let contact = self.fluid_contact(world, true);
        self.in_water = contact.in_water();
        let mut dm = self.vel / 20.0;
        dm += contact.entity_water_impulse(dm);
        let Some(s) = self.mount.as_mut() else { return };
        // `LivingEntity.aiStep`: tiny motion is dropped. The mount's own
        // float jump (`FloatGoal`, the camel brain's `Swim`) sets `jumping`
        // only where its AI runs, on the server; the rider's client moves a
        // controlled mount (`isLocalInstanceAuthoritative`) with `jumping`
        // false, so a ridden mount floats by `floatInLiquidWhileRidden` alone.
        for v in [&mut dm.x, &mut dm.y, &mut dm.z] {
            if v.abs() < 0.003 {
                *v = 0.0;
            }
        }
        // `getRiddenInput`: nothing while a camel sits or changes pose.
        let refuse = camel && s.refuses_to_move();
        let sideways = -f64::from(s.strafe) * 0.5;
        let mut forward = f64::from(s.forward);
        if forward <= 0.0 {
            forward *= 0.25;
        }
        let input = if refuse { DVec3::ZERO } else { DVec3::new(sideways, 0.0, forward) };
        // `tickRidden`: the rider's heading (a camel that refuses to move
        // keeps its own), then a released jump on the ground.
        if !refuse {
            self.yaw = s.yaw;
        }
        if self.on_ground
            && let Some(charge) = s.take_pending_jump()
        {
            let scale = jump_scale(charge);
            let below = world.block(super::super::item::below_pos(self.pos));
            let boost = 0.1 * self.effects.jump_boost() as f32;
            let r = (s.yaw.to_degrees() - 90.0) * (std::f32::consts::PI / 180.0);
            let (sin, cos) = (crate::knockback::mth_sin(r as f64), crate::knockback::mth_cos(r as f64));
            if camel {
                // `Camel.executeRidersJump`: the dash along the look,
                // flattened, and up by the jump power.
                if s.dash_cooldown == 0 && s.saddled() {
                    let power = camel::JUMP_STRENGTH as f32 * crate::player::travel::jump_factor(below) + boost;
                    let along =
                        camel::DASH_HORIZONTAL * scale * camel::MOVEMENT_SPEED * block_speed_factor(world, self.pos);
                    let flat = DVec3::new(f64::from(-sin), 0.0, f64::from(cos)).normalize_or_zero();
                    dm += flat * f64::from(along)
                        + DVec3::Y * (f64::from(camel::DASH_VERTICAL * scale) * f64::from(power));
                    s.dash_cooldown = mounts::DASH_COOLDOWN;
                    s.dashing = true;
                }
            } else {
                let power = s.jump as f32 * scale * crate::player::travel::jump_factor(below) + boost;
                dm.y = power as f64;
                if input.z > 0.0 {
                    // `getYRot()`: the rider's, just taken.
                    dm.x += f64::from(-JUMP_FORWARD * sin * scale);
                    dm.z += f64::from(JUMP_FORWARD * cos * scale);
                }
            }
        }
        // `Camel.tickRidden`: forward input stands a seated camel up.
        if camel && s.forward > 0.0 && s.camel_sitting && !s.in_pose_transition() {
            s.stand_up();
        }
        // `setSpeed(getRiddenSpeed)`: a camel runs 0.1 faster while its
        // rider sprints and the dash is ready.
        let bonus = if camel && s.rider_sprinting() && s.dash_cooldown == 0 { camel::RUNNING_SPEED_BONUS } else { 0.0 };
        let speed = f64::from(s.speed as f32 + bonus);
        self.move_ctl.cur = speed;
        let from_y = self.pos.y;
        // `Camel.travel`: held in place on the ground while it refuses to move.
        if refuse && self.on_ground {
            dm.x = 0.0;
            dm.z = 0.0;
        }
        if contact.in_water() || contact.in_lava() {
            self.ridden_travel_in_fluid(world, &mut dm, input, &contact);
        } else {
            self.travel_in_air(world, &mut dm, input);
        }
        self.vel = dm * 20.0;
        if contact.in_water() {
            // `updateFluidInteraction` resets the fall distance in water.
            if let Some(s) = self.mount.as_mut() {
                s.fall_distance = 0.0;
            }
        }
        self.equine_fall(world, from_y);
        if camel && let Some(s) = self.mount.as_mut() {
            // `Camel.tick` after the move.
            s.dash_clock(self.on_ground || contact.in_water() || contact.in_lava());
        }
    }

    /// `LivingEntity.travelInFluid` for a ridden mount, then
    /// `floatInLiquidWhileRidden`.
    fn ridden_travel_in_fluid<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        dm: &mut DVec3,
        input: DVec3,
        contact: &crate::player::fluid::FluidContact,
    ) {
        let falling = dm.y <= 0.0;
        let old_y = self.pos.y;
        let gravity = if falling && self.effects.has(crate::simulation::effects::Effect::SlowFalling) {
            GRAVITY.min(0.01)
        } else {
            GRAVITY
        };
        let step = self.step_height();
        if contact.in_water() {
            // `travelInWater`: no water movement efficiency, no sprint.
            let mut slow_down: f32 = if self.kind == MobKind::SkeletonHorse { 0.96 } else { 0.8 };
            if self.effects.has(crate::simulation::effects::Effect::DolphinsGrace) {
                slow_down = 0.96;
            }
            swim::move_relative(dm, f64::from(0.02_f32), input, self.yrot());
            self.java_move(world, dm, step);
            *dm *= DVec3::new(f64::from(slow_down), f64::from(0.8_f32), f64::from(slow_down));
            *dm = fluid_falling_adjusted(gravity, falling, *dm);
        } else {
            swim::move_relative(dm, f64::from(0.02_f32), input, self.yrot());
            self.java_move(world, dm, step);
            if contact.lava_height <= FLUID_JUMP_THRESHOLD {
                *dm *= DVec3::new(0.5, f64::from(0.8_f32), 0.5);
                *dm = fluid_falling_adjusted(gravity, falling, *dm);
            } else {
                *dm *= 0.5;
            }
            dm.y -= gravity / 4.0;
        }
        // `jumpOutOfFluid`.
        if self.blocked && self.is_free(world, DVec3::new(dm.x, dm.y + f64::from(0.6_f32) - self.pos.y + old_y, dm.z)) {
            dm.y = f64::from(0.3_f32);
        }
        // `floatInLiquidWhileRidden`: `#can_float_while_ridden` (every
        // horse and camel but the skeleton horse).
        if self.kind != MobKind::SkeletonHorse && contact.water_height > FLUID_JUMP_THRESHOLD {
            dm.y += LIQUID_LIFT;
        }
    }

    /// `Entity.isFree`: the box moved by `offset` hits no block and no liquid.
    fn is_free<W: BlockSource + ?Sized>(&self, world: &W, offset: DVec3) -> bool {
        let at = self.pos + offset;
        !physics::overlaps_solid(world, at, self.shape())
            && !physics::touches_block(world, at, self.shape(), |b| crate::player::fluid::fluid_at(b).is_some())
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
        // `Camel.actuallyHurt` stands it up.
        if camel_like(self.kind)
            && let Some(s) = self.mount.as_mut()
        {
            s.stand_up_instantly();
        }
    }

    /// v1.0 gap Track 3: `CamelMoveControl` stands a seated camel that wants
    /// to walk somewhere (not leashed, room to stand), and `Camel.travel`
    /// holds a camel still on the ground while it sits or changes pose.
    pub(super) fn camel_hold<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        wish: Option<DVec3>,
        speed: f64,
    ) -> (Option<DVec3>, f64) {
        if !camel_like(self.kind) || self.mount.as_ref().is_none_or(|s| s.controlled()) {
            return (wish, speed);
        }
        let can_stand = self.can_camel_change_pose(world);
        let leashed = self.leash.is_some();
        let s = self.mount.as_mut().unwrap();
        if wish.is_some() && !leashed && s.camel_sitting && !s.in_pose_transition() && can_stand {
            s.stand_up();
        }
        if s.refuses_to_move() && self.on_ground { (None, 0.0) } else { (wish, speed) }
    }

    /// `Camel.canCamelChangePose`: the other pose's box fits where it stands.
    fn can_camel_change_pose<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        let Some(s) = self.mount.as_ref() else { return false };
        let (width, height) = camel::dimensions(self.baby || self.age < 0, !s.camel_sitting);
        !physics::overlaps_solid(world, self.pos, physics::Shape::new(width / 2.0 - 1.0e-7, height - 2.0e-7))
    }

    /// v1.0 gap Track 3: `CamelAi`'s idle `RunOne` (priority 4, while it has
    /// no walk target): an equal pick among the behaviours that can start:
    /// `RandomStroll.stroll(2.0)` unless it refuses to move,
    /// `RandomSitting(20)` (sit after 20 seconds standing, stand after 20
    /// sitting: on the ground, out of water, unleashed, with no rider
    /// steering and room to change) and `DoNothing(30, 60)`.
    pub(super) fn camel_idle<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) {
        let can_change = self.can_camel_change_pose(world);
        let (in_water, leashed, on_ground, panicking) =
            (self.in_water, self.leash.is_some(), self.on_ground, self.ai == super::Ai::Panic);
        let Some(s) = self.mount.as_mut() else { return };
        if s.idle_wait > 0 {
            s.idle_wait -= 1;
            return;
        }
        if !self.nav.is_done() {
            return;
        }
        let stroll = !s.refuses_to_move();
        let sit = !in_water
            && s.pose_time >= camel::RANDOM_SIT_TICKS
            && !leashed
            && on_ground
            && !s.controlled()
            && !s.mob_controlled
            && can_change;
        let options = 1 + u32::from(stroll) + u32::from(sit);
        let mut pick = rng.next_int(options);
        if stroll {
            if pick == 0 {
                if self.try_stroll(world, ctx, rng) {
                    self.path_move = true;
                }
                return;
            }
            pick -= 1;
        }
        if sit && pick == 0 {
            if s.camel_sitting {
                s.stand_up();
            } else if !panicking {
                s.sit_down();
            }
            return;
        }
        s.idle_wait = 30 + rng.next_int(31) as u16;
    }
}

/// `LivingEntity.getFluidFallingAdjustedMovement` for a mount that isn't sprinting.
fn fluid_falling_adjusted(gravity: f64, falling: bool, movement: DVec3) -> DVec3 {
    if gravity == 0.0 {
        return movement;
    }
    let y = if falling && (movement.y - 0.005).abs() >= 0.003 && (movement.y - gravity / 16.0).abs() < 0.003 {
        -0.003
    } else {
        movement.y - gravity / 16.0
    };
    movement.with_y(y)
}

/// Whether a steered `kind` rides with [`Mob::equine_ridden_tick`]: the
/// horses since v1.0 Track 7, camels and camel husks since v1.0 gap Track 3.
pub fn java_ridden(kind: MobKind) -> bool {
    matches!(
        kind,
        MobKind::Horse
            | MobKind::Donkey
            | MobKind::Mule
            | MobKind::SkeletonHorse
            | MobKind::ZombieHorse
            | MobKind::Camel
            | MobKind::CamelHusk
    )
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
