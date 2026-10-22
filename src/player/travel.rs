//! One Java 26.3 player tick: `Entity.baseTick`'s fluid update,
//! `LocalPlayer.aiStep` (input, sprinting, crouching), `LivingEntity.aiStep`
//! (jumping), `Player.travel` / `LivingEntity.travel` and `Entity.move` with
//! Java's collision, step-up, edge back-off and bounce.
//!
//! Java mixes floats and doubles; constants written as `0.91_f32` are Java
//! float literals and are widened exactly where Java widens them.

use std::f32::consts::FRAC_PI_2;

use glam::{DVec3, IVec3};

use super::fluid::{self, FluidContact};
use super::{MoveInput, Player};
use crate::entity::model::{PlayerPose, resolve_pose};
use crate::physics::{self, BlockSource, Collider, JAVA_EPSILON};
use crate::world::block::{Block, Fluid, Shaped};

/// Java's ticks per second: velocity is stored per second, Java's per tick.
const TICK_RATE: f64 = 20.0;
/// `Attributes.GRAVITY` for players.
const GRAVITY: f64 = 0.08;
/// `Attributes.JUMP_STRENGTH`, read as a float by `getJumpPower`.
const JUMP_STRENGTH: f32 = 0.42;
/// `Attributes.SNEAKING_SPEED`.
const SNEAKING_SPEED: f32 = 0.3;
/// `UseEffects.DEFAULT.speedMultiplier`.
const ITEM_USE_SPEED: f32 = 0.2;
/// `Abilities.flyingSpeed`.
const FLYING_SPEED: f32 = 0.05;
/// The default `sprintWindow` option: ticks to double-tap forward.
const SPRINT_WINDOW: u8 = 7;
/// `Attributes.STEP_HEIGHT` for players, as `maxUpStep` returns it.
const MAX_UP_STEP: f32 = 0.6;

/// Java movement state that lives between ticks.
#[derive(Clone, Debug, Default)]
pub(super) struct Motion {
    pub horizontal_collision: bool,
    minor_horizontal_collision: bool,
    pub(super) no_jump_delay: u8,
    pub(super) sprint_trigger_time: u8,
    /// Last tick's keys: Java reads them before `input.tick()`.
    pub(super) was_shift: bool,
    pub(super) had_forward: bool,
    /// The movement scale a cobweb or berry bush set for the next move.
    pub(super) stuck: DVec3,
    pub(super) fluid: FluidContact,
    /// `mainSupportingBlockPos`.
    supporting: Option<IVec3>,
    /// Java's `fallDistance`, for the sneaking edge back-off.
    pub(super) fall_distance: f64,
    /// The input `applyInput` passed to `travel` (left, forward).
    xxa: f32,
    zza: f32,
    colliders: Vec<Collider>,
}

/// `Block.getFriction`.
pub fn friction(b: Option<Block>) -> f32 {
    match b {
        Some(Block::ICE) | Some(crate::world::gadgets::PACKED_ICE) => 0.98,
        Some(crate::world::overworld_blocks::BLUE_ICE) => 0.989,
        _ => 0.6,
    }
}

/// `Block.getSpeedFactor`.
pub fn speed_factor(b: Option<Block>) -> f32 {
    match b {
        Some(Block::SOUL_SAND | Block::HONEY_BLOCK) => 0.4,
        _ => 1.0,
    }
}

/// `Block.getJumpFactor`.
pub fn jump_factor(b: Option<Block>) -> f32 {
    if b == Some(Block::HONEY_BLOCK) { 0.5 } else { 1.0 }
}

/// `Block.getBounceRestitution` for living entities.
fn bounciness(b: Option<Block>) -> f64 {
    match b {
        Some(b) if b.is_bed() => 0.75_f32 as f64,
        _ => 0.0,
    }
}

/// Java's `Math.signum`: zero stays zero.
fn signum(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v.signum() }
}

fn expand_towards(min: DVec3, max: DVec3, v: DVec3) -> (DVec3, DVec3) {
    (min + v.min(DVec3::ZERO), max + v.max(DVec3::ZERO))
}

/// Java's `LocalPlayer.modifyInput`: the 0.98 key scale, item use and
/// sneaking slow-downs, then `modifyInputSpeedForSquareMovement`.
fn modify_input(x: f32, y: f32, using_item: bool, slow: bool) -> (f32, f32) {
    if x * x + y * y == 0.0 {
        return (x, y);
    }
    let (mut x, mut y) = (x * 0.98, y * 0.98);
    if using_item {
        (x, y) = (x * ITEM_USE_SPEED, y * ITEM_USE_SPEED);
    }
    if slow {
        (x, y) = (x * SNEAKING_SPEED, y * SNEAKING_SPEED);
    }
    let length = (x * x + y * y).sqrt();
    if length <= 0.0 {
        return (x, y);
    }
    let inverse = 1.0 / length;
    let (dx, dy) = (x * inverse, y * inverse);
    let (ax, ay) = (dx.abs(), dy.abs());
    let tan = if ay > ax { ax / ay } else { ay / ax };
    let to_square = (1.0 + tan * tan).sqrt();
    let modified = (length * to_square).min(1.0);
    (dx * modified, dy * modified)
}

impl Player {
    /// One 20 Hz Java tick. Does nothing until the feet's chunk is loaded.
    pub fn tick<W: BlockSource + ?Sized>(&mut self, input: MoveInput, world: &W) {
        self.honey_sliding = false;
        self.honey_landed = false;
        if world.block(self.pos.floor().as_ivec3()).is_none() {
            return;
        }
        let before = self.pos;
        self.jumped = false;
        let mut dm = self.vel / TICK_RATE;
        // A player standing still carries gravity's resting fall each tick,
        // and Java restores it with the saved `Motion`. One placed on the
        // ground without it (new, loaded, teleported or dismounted) gets it
        // here, so its first move still finds the ground.
        if self.on_ground && dm.y == 0.0 && !self.flying {
            dm.y = -GRAVITY * 0.98_f32 as f64;
        }
        self.base_tick(world, &mut dm);
        self.ai_step(input, world, &mut dm);
        self.vel = dm * TICK_RATE;
        self.climbing = self.on_climbable(world);
        // `Player.tick` ends with `updatePlayerPose`.
        self.pose = resolve_pose(self, world, self.sneaking);
        self.animation.update(
            (self.pos - before).with_y(0.0).length() as f32,
            (1.0 / TICK_RATE) as f32,
            self.yaw,
            self.flying,
            self.on_ground,
        );
        self.tick_fov();
        // `LivingEntity.updateSwimAmount`.
        self.swim_amount = if self.pose.visually_swimming() {
            (self.swim_amount + 0.09).min(1.0)
        } else {
            (self.swim_amount - 0.09).max(0.0)
        };
    }

    /// `Entity.baseTick`: fluid contact and currents, then `updateSwimming`.
    fn base_tick<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3) {
        if world.block(self.on_pos(world, 0.2)) == Some(Block::AIR) {
            self.frozen_fraction = 0.0;
        }
        self.was_in_powder_snow = self.in_powder_snow;
        self.in_powder_snow = false;
        self.eye_was_in_water = self.motion.fluid.eye_in_water;
        let (min, max) = self.collision_shape().aabb(self.pos);
        // `Player.isPushedByFluid`: not while flying.
        let pushed = !self.flying;
        let contact = fluid::contact(world, min, max, self.eye(), pushed);
        if contact.in_water() {
            self.motion.fall_distance = 0.0;
        }
        if pushed {
            *dm += contact.current_impulse(*dm, false);
        }
        self.motion.fluid = contact;
        self.in_water = contact.in_water();
        // `Player.updateSwimming` / `Entity.updateSwimming`.
        let riding = self.vehicle.is_some();
        self.swimming = if self.flying {
            false
        } else if self.swimming {
            self.sprinting && self.in_water && !riding
        } else {
            let feet = world.block(self.pos.floor().as_ivec3()).and_then(fluid::fluid_at);
            self.sprinting && self.under_water() && !riding && feet.is_some_and(|c| c.fluid == Fluid::Water)
        };
        if contact.in_lava() {
            self.motion.fall_distance *= 0.5;
        }
    }

    /// `Entity.isUnderWater`: the eyes were in water at the start of the tick.
    fn under_water(&self) -> bool {
        self.eye_was_in_water && self.in_water
    }

    /// Java's `canPlayerFitWithinBlocksAndEntitiesWhen`.
    fn fits<W: BlockSource + ?Sized>(&self, world: &W, pose: PlayerPose) -> bool {
        crate::entity::model::fits_player(world, self, pose)
    }

    /// `LocalPlayer.aiStep` and the `Player` / `LivingEntity.aiStep` it calls.
    fn ai_step<W: BlockSource + ?Sized>(&mut self, input: MoveInput, world: &W, dm: &mut DVec3) {
        self.motion.sprint_trigger_time = self.motion.sprint_trigger_time.saturating_sub(1);
        // Java reads last tick's keys before `input.tick()`.
        let (was_shift, had_forward) = (self.motion.was_shift, self.motion.had_forward);
        let riding = self.vehicle.is_some();
        self.crouching = !self.flying
            && !self.swimming
            && !riding
            && self.fits(world, PlayerPose::Crouching)
            && (was_shift || !self.fits(world, PlayerPose::Standing));

        // `KeyboardInput.tick`: left and forward, unit length for keys.
        let (mut left, mut forward) = (-input.right as f32, input.forward as f32);
        let length = (left * left + forward * forward).sqrt();
        if length < 1.0e-4 {
            (left, forward) = (0.0, 0.0);
        } else if length > 1.0 {
            (left, forward) = (left / length, forward / length);
        }
        let has_forward = forward > 1.0e-5;
        let shift = input.descend;
        self.sneaking = shift;

        if !self.noclip && !riding {
            let reach = super::HALF_WIDTH * 2.0 * 0.35;
            for (sx, sz) in [(-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0), (1.0, 1.0)] {
                self.move_towards_closest_space(world, self.pos.x + sx * reach, self.pos.z + sz * reach, dm);
            }
        }

        let slow_using = input.using_item && !riding;
        if was_shift || slow_using || input.forward < 0.0 {
            self.motion.sprint_trigger_time = 0;
        }
        let moving_slowly = self.crouching || self.pose.crawling(self.in_water);
        let under_water = self.under_water();
        let in_shallow_water = self.in_water && !under_water;
        let possible = |allowed_in_shallow: bool| !input.sprint_blocked && (allowed_in_shallow || !in_shallow_water);
        let can_start =
            !self.sprinting && has_forward && possible(self.flying) && !slow_using && (!moving_slowly || under_water);
        if can_start {
            if !had_forward {
                if self.motion.sprint_trigger_time > 0 {
                    self.sprinting = true;
                } else {
                    self.motion.sprint_trigger_time = SPRINT_WINDOW;
                }
            }
            if input.sprint {
                self.sprinting = true;
            }
        }
        if self.sprinting {
            let stop = if self.swimming {
                !possible(true) || !self.in_water || (!has_forward && !self.on_ground && !shift)
            } else {
                !possible(self.flying)
                    || !has_forward
                    || (self.motion.horizontal_collision && !self.motion.minor_horizontal_collision)
            };
            if stop {
                self.sprinting = false;
            }
        }

        // `goDownInWater`.
        if self.in_water && shift && !self.flying {
            dm.y -= 0.04_f32 as f64;
        }
        if self.flying {
            let ya = input.jump as i32 - shift as i32;
            if ya != 0 {
                dm.y += (ya as f32 * FLYING_SPEED * 3.0) as f64;
            }
        }

        // `Player.aiStep`.
        if self.flying && !riding {
            self.motion.fall_distance = 0.0;
        }
        // `LivingEntity.aiStep`.
        self.motion.no_jump_delay = self.motion.no_jump_delay.saturating_sub(1);
        if dm.x * dm.x + dm.z * dm.z < 9.0e-6 {
            dm.x = 0.0;
            dm.z = 0.0;
        }
        if dm.y.abs() < 0.003 {
            dm.y = 0.0;
        }
        let (xxa, zza) = modify_input(left, forward, slow_using, moving_slowly);
        (self.motion.xxa, self.motion.zza) = (xxa, zza);
        let jumping = input.jump;
        if jumping && !self.flying {
            self.jump_controls(world, dm);
        } else {
            self.motion.no_jump_delay = 0;
        }
        self.travel(world, DVec3::new(xxa as f64, 0.0, zza as f64), jumping, dm);
        self.apply_effects_from_blocks(world, dm);

        if self.on_ground && self.flying && !self.noclip {
            self.flying = false;
        }
        self.motion.was_shift = shift;
        self.motion.had_forward = has_forward;
    }

    /// The jumping branch of `LivingEntity.aiStep`.
    fn jump_controls<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3) {
        let fluid = self.motion.fluid;
        let in_lava = fluid.in_lava();
        let height = if in_lava { fluid.lava_height } else { fluid.water_height };
        let in_water_deep = self.in_water && height > 0.0;
        let threshold = self.fluid_jump_threshold();
        if !in_water_deep || (self.on_ground && height <= threshold) {
            if !in_lava || (self.on_ground && fluid.lava_height <= threshold) {
                if (self.on_ground || (in_water_deep && height <= threshold)) && self.motion.no_jump_delay == 0 {
                    self.jump_from_ground(world, dm);
                    self.motion.no_jump_delay = 10;
                }
            } else {
                dm.y += 0.04_f32 as f64;
            }
        } else {
            dm.y += 0.04_f32 as f64;
        }
    }

    /// `Entity.getFluidJumpThreshold`.
    fn fluid_jump_threshold(&self) -> f64 {
        if self.pose.eye_height() < 0.4 { 0.0 } else { 0.4 }
    }

    /// `LivingEntity.jumpFromGround`.
    fn jump_from_ground<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3) {
        let boost = if self.modifiers.jump_levels > 0 { 0.1 * self.modifiers.jump_levels as f32 } else { 0.0 };
        let power = JUMP_STRENGTH * self.block_jump_factor(world) + boost;
        if power <= 1.0e-5 {
            return;
        }
        dm.y = dm.y.max(power as f64);
        if self.sprinting {
            let angle = self.yaw - FRAC_PI_2;
            dm.x += -angle.sin() as f64 * 0.2;
            dm.z += angle.cos() as f64 * 0.2;
        }
        self.jumped = true;
    }

    /// `Player.travel`: sprint-swimming steers by pitch, and flight keeps
    /// 0.6 of its vertical speed.
    fn travel<W: BlockSource + ?Sized>(&mut self, world: &W, input: DVec3, jumping: bool, dm: &mut DVec3) {
        if self.swimming {
            let look_y = self.forward().y as f64;
            let multiplier = if look_y < -0.2 { 0.085 } else { 0.06 };
            let head = DVec3::new(self.pos.x, self.pos.y + 1.0 - 0.1, self.pos.z).floor().as_ivec3();
            if look_y <= 0.0 || jumping || world.block(head).and_then(fluid::fluid_at).is_some() {
                dm.y += (look_y - dm.y) * multiplier;
            }
        }
        let original_y = dm.y;
        let fluid = self.motion.fluid;
        if (fluid.in_water() || fluid.in_lava()) && !self.flying {
            self.travel_in_fluid(world, input, dm);
        } else {
            self.travel_in_air(world, input, jumping, dm);
        }
        if self.flying {
            dm.y = original_y * 0.6;
        }
    }

    /// `LivingEntity.getEffectiveGravity`: Slow Falling caps a fall's gravity at 0.01.
    fn effective_gravity(&self, dm: DVec3) -> f64 {
        if dm.y <= 0.0 && self.modifiers.slow_falling { GRAVITY.min(0.01) } else { GRAVITY }
    }

    /// `LivingEntity.travelInAir`.
    fn travel_in_air<W: BlockSource + ?Sized>(&mut self, world: &W, input: DVec3, jumping: bool, dm: &mut DVec3) {
        let below = self.block_below_affecting_movement(world);
        let block_friction = if self.on_ground { friction(world.block(below)) } else { 1.0 };
        let movement = self.relative_friction_movement(world, input, block_friction, jumping, dm);
        let y = movement.y - self.effective_gravity(*dm);
        let f = (block_friction * 0.91_f32) as f64;
        *dm = DVec3::new(movement.x * f, y * 0.98_f32 as f64, movement.z * f);
    }

    /// `LivingEntity.handleRelativeFrictionAndCalculateMovement`.
    fn relative_friction_movement<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        input: DVec3,
        block_friction: f32,
        jumping: bool,
        dm: &mut DVec3,
    ) -> DVec3 {
        let speed = self.friction_influenced_speed(block_friction);
        self.move_relative(speed, input, dm);
        *dm = self.handle_on_climbable(world, *dm);
        self.move_entity(world, *dm, dm);
        let mut movement = *dm;
        if (self.motion.horizontal_collision || jumping)
            && (self.on_climbable(world) || self.was_in_powder_snow && self.leather_boots)
        {
            movement.y = 0.2;
        }
        movement
    }

    /// `LivingEntity.getFrictionInfluencedSpeed`.
    fn friction_influenced_speed(&self, block_friction: f32) -> f32 {
        if self.on_ground {
            let speed = self.movement_speed();
            if block_friction as f64 > 0.6 {
                speed * (0.21600002_f32 / (block_friction * block_friction * block_friction))
            } else {
                speed
            }
        } else if self.flying {
            // `Player.getFlyingSpeed`.
            if self.sprinting { FLYING_SPEED * 2.0 } else { FLYING_SPEED }
        } else if self.sprinting {
            0.025999999
        } else {
            0.02
        }
    }

    /// `Entity.moveRelative` with `getInputVector`.
    fn move_relative(&self, speed: f32, input: DVec3, dm: &mut DVec3) {
        let length = input.length_squared();
        if length < 1.0e-7 {
            return;
        }
        let movement = if length > 1.0 { input.normalize() } else { input } * speed as f64;
        let angle = self.yaw - FRAC_PI_2;
        let (sin, cos) = (angle.sin() as f64, angle.cos() as f64);
        *dm += DVec3::new(movement.x * cos - movement.z * sin, movement.y, movement.z * cos + movement.x * sin);
    }

    /// `LivingEntity.onClimbable`: a climbable block at the feet, or an open
    /// trapdoor on top of a ladder facing the same way.
    pub fn on_climbable<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        if self.noclip {
            return false;
        }
        let at = self.pos.floor().as_ivec3();
        let Some(b) = world.block(at) else { return false };
        if b.is_climbable() {
            return true;
        }
        match crate::world::redstone_blocks::component(b) {
            Some(crate::world::redstone_blocks::Component::Trapdoor { facing, open: true, .. }) => {
                world.block(at - IVec3::Y).and_then(Block::shaped) == Some(Shaped::Ladder(facing))
            }
            _ => false,
        }
    }

    /// `LivingEntity.handleOnClimbable`: slow sideways movement and falls;
    /// sneaking holds on.
    fn handle_on_climbable<W: BlockSource + ?Sized>(&mut self, world: &W, mut delta: DVec3) -> DVec3 {
        if !self.on_climbable(world) {
            return delta;
        }
        self.motion.fall_distance = 0.0;
        let max = 0.15_f32 as f64;
        delta.x = delta.x.clamp(-max, max);
        delta.z = delta.z.clamp(-max, max);
        delta.y = delta.y.max(-max);
        let in_scaffolding =
            world.block(self.pos.floor().as_ivec3()).is_some_and(crate::world::scaffolding::is_scaffolding);
        if delta.y < 0.0 && !in_scaffolding && self.sneaking && !self.flying {
            delta.y = 0.0;
        }
        delta
    }

    /// `LivingEntity.travelInFluid`: water, or lava.
    fn travel_in_fluid<W: BlockSource + ?Sized>(&mut self, world: &W, input: DVec3, dm: &mut DVec3) {
        let falling = dm.y <= 0.0;
        let old_y = self.pos.y;
        let gravity = self.effective_gravity(*dm);
        if self.motion.fluid.in_water() {
            let mut slow_down: f32 = if self.sprinting { 0.9 } else { 0.8 };
            let mut speed: f32 = 0.02;
            // `WATER_MOVEMENT_EFFICIENCY` from Depth Strider: a third per level.
            let mut efficiency = (0.33333334_f32 * self.modifiers.depth_strider as f32).min(1.0);
            if !self.on_ground {
                efficiency *= 0.5;
            }
            if efficiency > 0.0 {
                slow_down += (0.54600006 - slow_down) * efficiency;
                speed += (self.movement_speed() - speed) * efficiency;
            }
            if self.modifiers.dolphins_grace {
                slow_down = 0.96;
            }
            self.move_relative(speed, input, dm);
            self.move_entity(world, *dm, dm);
            let mut movement = *dm;
            if self.motion.horizontal_collision && self.on_climbable(world) {
                movement.y = 0.2;
            }
            movement *= DVec3::new(slow_down as f64, 0.8_f32 as f64, slow_down as f64);
            *dm = self.fluid_falling_adjusted(gravity, falling, movement);
        } else {
            self.move_relative(0.02, input, dm);
            self.move_entity(world, *dm, dm);
            if self.motion.fluid.lava_height <= self.fluid_jump_threshold() {
                *dm *= DVec3::new(0.5, 0.8_f32 as f64, 0.5);
                *dm = self.fluid_falling_adjusted(gravity, falling, *dm);
            } else {
                *dm *= 0.5;
            }
            if gravity != 0.0 {
                dm.y -= gravity / 4.0;
            }
        }
        // `jumpOutOfFluid`.
        if self.motion.horizontal_collision
            && self.is_free(world, DVec3::new(dm.x, dm.y + 0.6_f32 as f64 - self.pos.y + old_y, dm.z))
        {
            dm.y = 0.3_f32 as f64;
        }
    }

    /// `LivingEntity.getFluidFallingAdjustedMovement`.
    fn fluid_falling_adjusted(&self, gravity: f64, falling: bool, movement: DVec3) -> DVec3 {
        if gravity == 0.0 || self.sprinting {
            return movement;
        }
        let y = if falling && (movement.y - 0.005).abs() >= 0.003 && (movement.y - gravity / 16.0).abs() < 0.003 {
            -0.003
        } else {
            movement.y - gravity / 16.0
        };
        movement.with_y(y)
    }

    /// `Entity.isFree`: the box moved by `offset` hits no block and no liquid.
    fn is_free<W: BlockSource + ?Sized>(&self, world: &W, offset: DVec3) -> bool {
        let (min, max) = self.collision_shape().aabb(self.pos + offset);
        if !physics::no_collision_context(world, min, max, Some(self.collision_context())) {
            return false;
        }
        let (lo, hi) = (min.floor().as_ivec3(), max.ceil().as_ivec3() - IVec3::ONE);
        !(lo.y..=hi.y).any(|y| {
            (lo.z..=hi.z)
                .any(|z| (lo.x..=hi.x).any(|x| world.block(IVec3::new(x, y, z)).and_then(fluid::fluid_at).is_some()))
        })
    }

    /// `LocalPlayer.moveTowardsClosestSpace`: pushed out of a suffocating block
    /// towards its nearest open side.
    fn move_towards_closest_space<W: BlockSource + ?Sized>(&self, world: &W, x: f64, z: f64, dm: &mut DVec3) {
        let cell = IVec3::new(x.floor() as i32, self.pos.y.floor() as i32, z.floor() as i32);
        if !self.suffocates_at(world, cell) {
            return;
        }
        let (xd, zd) = (x - cell.x as f64, z - cell.z as f64);
        let mut best: Option<IVec3> = None;
        let mut closest = f64::MAX;
        for dir in [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z] {
            let along = if dir.x != 0 { xd } else { zd };
            let to_edge = if dir.x + dir.z > 0 { 1.0 - along } else { along };
            if to_edge < closest && !self.suffocates_at(world, cell + dir) {
                closest = to_edge;
                best = Some(dir);
            }
        }
        match best {
            Some(dir) if dir.x != 0 => dm.x = 0.1 * dir.x as f64,
            Some(dir) => dm.z = 0.1 * dir.z as f64,
            None => {}
        }
    }

    /// `LocalPlayer.suffocatesAt`: a suffocating block's collision in the
    /// column `cell` over the body's height.
    fn suffocates_at<W: BlockSource + ?Sized>(&self, world: &W, cell: IVec3) -> bool {
        let (bmin, bmax) = self.collision_shape().aabb(self.pos);
        let min = DVec3::new(cell.x as f64, bmin.y, cell.z as f64) + DVec3::splat(JAVA_EPSILON);
        let max = DVec3::new(cell.x as f64 + 1.0, bmax.y, cell.z as f64 + 1.0) - DVec3::splat(JAVA_EPSILON);
        let mut boxes = Vec::new();
        physics::colliders_context(world, min, max, Some(self.collision_context()), &mut boxes);
        boxes.iter().any(|c| {
            world.block(c.cell).is_none_or(|b| b.is_opaque() && b.is_solid())
                && c.min.cmplt(max).all()
                && c.max.cmpgt(min).all()
        })
    }

    /// `Entity.move` for `MoverType.SELF`: collide, step up, back off edges
    /// while sneaking, then stop or bounce on collided axes and apply the
    /// block's speed factor to `dm`.
    fn move_entity<W: BlockSource + ?Sized>(&mut self, world: &W, mut delta: DVec3, dm: &mut DVec3) {
        if self.noclip {
            self.pos += delta;
            self.motion.horizontal_collision = false;
            self.motion.minor_horizontal_collision = false;
            self.on_ground = false;
            return;
        }
        if self.motion.stuck.length_squared() > 1.0e-7 {
            delta *= self.motion.stuck;
            self.motion.stuck = DVec3::ZERO;
            *dm = DVec3::ZERO;
        }
        delta = self.back_off_from_edge(world, delta);
        let movement = self.collide(world, delta);
        let moved = movement.length_squared();
        if moved > 1.0e-7 || delta.length_squared() - moved < 1.0e-7 {
            self.pos += movement;
        }
        let x_collision = (delta.x - movement.x).abs() >= 1.0e-5_f32 as f64;
        let z_collision = (delta.z - movement.z).abs() >= 1.0e-5_f32 as f64;
        self.motion.horizontal_collision = x_collision || z_collision;
        let vertical = delta.y != movement.y;
        let below = vertical && delta.y < 0.0;
        self.on_ground = below;
        self.check_supporting_block(world, below, movement);
        self.honey_landed = below && self.motion.fall_distance > 0.0 && self.on_honey(world);
        self.motion.minor_horizontal_collision =
            self.motion.horizontal_collision && self.horizontal_collision_minor(movement);
        // `checkFallDamage`'s distance bookkeeping.
        if self.on_ground {
            self.motion.fall_distance = 0.0;
        } else if movement.y < 0.0 {
            self.motion.fall_distance -= movement.y;
        }
        if (delta.y.abs() > 0.0 && vertical) || self.motion.horizontal_collision {
            let effect = world.block(self.on_pos(world, 0.2));
            self.restitute(effect, x_collision, z_collision, vertical, below, movement, dm);
        }
        let factor = if self.flying { 1.0 } else { self.block_speed_factor(world) } as f64;
        dm.x *= factor;
        dm.z *= factor;
    }

    /// `Entity.restituteMovementAfterCollisions`: collided axes stop, and a
    /// fast enough landing on a bed bounces unless sneaking.
    #[allow(clippy::too_many_arguments)]
    fn restitute(
        &self,
        effect: Option<Block>,
        x_collision: bool,
        z_collision: bool,
        vertical: bool,
        below: bool,
        movement: DVec3,
        dm: &mut DVec3,
    ) {
        let suppress = self.sneaking;
        let mut restitution = 0.0;
        let current = *dm;
        let mut after = current;
        if x_collision {
            after.x = -current.x * restitution;
        }
        if z_collision {
            after.z = -current.z * restitution;
        }
        if vertical {
            let gravity = self.effective_gravity(current);
            if below {
                restitution = if -current.y > gravity && !suppress && effect != Some(Block::HONEY_BLOCK) {
                    f64::max(restitution, bounciness(effect))
                } else {
                    0.0
                };
            }
            let (compensation, drag) = if restitution > 0.0 {
                let portion = movement.y / current.y;
                (portion * gravity, 1.0 + (0.98_f32 as f64 - 1.0) * portion)
            } else {
                (0.0, 1.0)
            };
            after.y = (compensation - current.y) * drag * restitution;
        }
        *dm = after;
    }

    /// `LocalPlayer.isHorizontalCollisionMinor`: the wall turned the move by
    /// less than 8 degrees, which doesn't stop a sprint.
    fn horizontal_collision_minor(&self, movement: DVec3) -> bool {
        let angle = self.yaw - FRAC_PI_2;
        let (sin, cos) = (angle.sin() as f64, angle.cos() as f64);
        let (xxa, zza) = (self.motion.xxa as f64, self.motion.zza as f64);
        let gx = xxa * cos - zza * sin;
        let gz = zza * cos + xxa * sin;
        let wanted = gx * gx + gz * gz;
        let moved = movement.x * movement.x + movement.z * movement.z;
        if wanted < 1.0e-5_f32 as f64 || moved < 1.0e-5_f32 as f64 {
            return false;
        }
        let dot = gx * movement.x + gz * movement.z;
        (dot / (wanted * moved).sqrt()).acos() < 0.13962634_f32 as f64
    }

    /// `Entity.collide`: block collisions, then the lowest step up to 0.6
    /// blocks that carries the move further.
    fn collide<W: BlockSource + ?Sized>(&mut self, world: &W, delta: DVec3) -> DVec3 {
        let (min, max) = self.collision_shape().aabb(self.pos);
        let mut colliders = std::mem::take(&mut self.motion.colliders);
        let step = if delta.length_squared() == 0.0 {
            delta
        } else {
            let (qmin, qmax) = expand_towards(min, max, delta);
            physics::colliders_context(world, qmin, qmax, Some(self.collision_context()), &mut colliders);
            physics::collide_with(&colliders, min, max, delta)
        };
        let mut result = step;
        let (xc, yc, zc) = (delta.x != step.x, delta.y != step.y, delta.z != step.z);
        let landed = yc && delta.y < 0.0;
        if (landed || self.on_ground) && (xc || zc) {
            let lift = if landed { DVec3::new(0.0, step.y, 0.0) } else { DVec3::ZERO };
            let (gmin, gmax) = (min + lift, max + lift);
            let (mut smin, smax) = expand_towards(gmin, gmax, DVec3::new(delta.x, MAX_UP_STEP as f64, delta.z));
            if !landed {
                smin.y += -1.0e-5_f32 as f64;
            }
            physics::colliders_context(world, smin, smax, Some(self.collision_context()), &mut colliders);
            let skip = step.y as f32;
            let mut candidates: Vec<f32> = Vec::new();
            for group in colliders.chunk_by(|a, b| a.cell == b.cell) {
                let mut coords: Vec<f64> = group.iter().flat_map(|c| [c.min.y, c.max.y]).collect();
                coords.sort_by(f64::total_cmp);
                coords.dedup();
                for coord in coords {
                    let relative = (coord - gmin.y) as f32;
                    if relative < 0.0 || relative == skip {
                        continue;
                    }
                    if relative > MAX_UP_STEP {
                        break;
                    }
                    if !candidates.contains(&relative) {
                        candidates.push(relative);
                    }
                }
            }
            candidates.sort_by(f32::total_cmp);
            let gained = |v: DVec3| v.x * v.x + v.z * v.z;
            for height in candidates {
                let up = physics::collide_with(&colliders, gmin, gmax, DVec3::new(delta.x, height as f64, delta.z));
                if gained(up) > gained(step) {
                    result = up - DVec3::new(0.0, min.y - gmin.y, 0.0);
                    break;
                }
            }
        }
        self.motion.colliders = colliders;
        result
    }

    /// `Player.maybeBackOffFromEdge`: sneaking on the ground never walks
    /// off a drop deeper than a step.
    fn back_off_from_edge<W: BlockSource + ?Sized>(&self, world: &W, delta: DVec3) -> DVec3 {
        let max_down = MAX_UP_STEP as f64;
        if self.flying || delta.y > 0.0 || !self.sneaking || !self.above_ground(world, max_down) {
            return delta;
        }
        let (mut dx, mut dz) = (delta.x, delta.z);
        let (sx, sz) = (signum(dx) * 0.05, signum(dz) * 0.05);
        while dx != 0.0 && self.can_fall_at_least(world, dx, 0.0, max_down) {
            if dx.abs() <= 0.05 {
                dx = 0.0;
                break;
            }
            dx -= sx;
        }
        while dz != 0.0 && self.can_fall_at_least(world, 0.0, dz, max_down) {
            if dz.abs() <= 0.05 {
                dz = 0.0;
                break;
            }
            dz -= sz;
        }
        while dx != 0.0 && dz != 0.0 && self.can_fall_at_least(world, dx, dz, max_down) {
            dx = if dx.abs() <= 0.05 { 0.0 } else { dx - sx };
            dz = if dz.abs() <= 0.05 { 0.0 } else { dz - sz };
        }
        DVec3::new(dx, delta.y, dz)
    }

    /// `Player.isAboveGround`.
    fn above_ground<W: BlockSource + ?Sized>(&self, world: &W, max_down: f64) -> bool {
        let fall = self.motion.fall_distance;
        self.on_ground || (fall < max_down && !self.can_fall_at_least(world, 0.0, 0.0, max_down - fall))
    }

    /// `Player.canFallAtLeast`: nothing under the body, moved by (dx, dz),
    /// within `height`.
    fn can_fall_at_least<W: BlockSource + ?Sized>(&self, world: &W, dx: f64, dz: f64, height: f64) -> bool {
        let (min, max) = self.collision_shape().aabb(self.pos);
        physics::no_collision_context(
            world,
            DVec3::new(min.x + JAVA_EPSILON + dx, min.y - height - JAVA_EPSILON, min.z + JAVA_EPSILON + dz),
            DVec3::new(max.x - JAVA_EPSILON + dx, min.y, max.z - JAVA_EPSILON + dz),
            Some(self.collision_context()),
        )
    }

    /// `Entity.checkSupportingBlock`: the block under the feet nearest the
    /// body's centre, found while on the ground.
    fn check_supporting_block<W: BlockSource + ?Sized>(&mut self, world: &W, on_ground: bool, movement: DVec3) {
        if !on_ground {
            self.motion.supporting = None;
            return;
        }
        let (min, max) = self.collision_shape().aabb(self.pos);
        let context = self.collision_context();
        let test = |offset: DVec3, colliders: &mut Vec<Collider>| {
            let tmin = DVec3::new(min.x, min.y - 1.0e-6, min.z) + offset;
            let tmax = DVec3::new(max.x, min.y, max.z) + offset;
            physics::colliders_context(world, tmin, tmax, Some(context), colliders);
            let mut best: Option<(IVec3, f64)> = None;
            for c in colliders.iter() {
                let d = (c.cell.as_dvec3() + DVec3::splat(0.5)).distance_squared(self.pos);
                let later = |a: IVec3, b: IVec3| (a.y, a.z, a.x) > (b.y, b.z, b.x);
                if best.is_none_or(|(p, bd)| d < bd || (d == bd && later(c.cell, p))) {
                    best = Some((c.cell, d));
                }
            }
            best.map(|(p, _)| p)
        };
        let mut colliders = std::mem::take(&mut self.motion.colliders);
        let mut found = test(DVec3::ZERO, &mut colliders);
        if found.is_none() {
            found = test(DVec3::new(-movement.x, 0.0, -movement.z), &mut colliders);
        }
        self.motion.colliders = colliders;
        self.motion.supporting = found;
    }

    /// `Entity.getOnPos(offset)`: the supporting block moved down to the
    /// offset's depth, except walls and gates (and fences for small offsets).
    fn on_pos<W: BlockSource + ?Sized>(&self, world: &W, offset: f32) -> IVec3 {
        let y = (self.pos.y - offset as f64).floor() as i32;
        match self.motion.supporting {
            Some(s) => {
                let b = world.block(s);
                let keep = b.and_then(Block::shaped).is_some_and(|shape| {
                    matches!(shape, Shaped::Wall | Shaped::Gate { .. }) || (offset <= 0.5 && shape == Shaped::Fence)
                });
                if keep { s } else { IVec3::new(s.x, y, s.z) }
            }
            None => IVec3::new(self.pos.x.floor() as i32, y, self.pos.z.floor() as i32),
        }
    }

    /// `getBlockPosBelowThatAffectsMyMovement`.
    fn block_below_affecting_movement<W: BlockSource + ?Sized>(&self, world: &W) -> IVec3 {
        self.on_pos(world, 0.500001)
    }

    /// `Entity.getBlockJumpFactor`.
    fn block_jump_factor<W: BlockSource + ?Sized>(&self, world: &W) -> f32 {
        let here = jump_factor(world.block(self.pos.floor().as_ivec3()));
        if here == 1.0 { jump_factor(world.block(self.block_below_affecting_movement(world))) } else { here }
    }

    /// `Entity.getBlockSpeedFactor`: the feet's block, or the one below
    /// when that is neutral (water never passes it down).
    fn block_speed_factor<W: BlockSource + ?Sized>(&self, world: &W) -> f32 {
        let at = world.block(self.pos.floor().as_ivec3());
        let here = speed_factor(at);
        if at.is_some_and(Block::is_water) || here != 1.0 {
            here
        } else {
            speed_factor(world.block(self.block_below_affecting_movement(world)))
        }
    }

    /// `Entity.applyEffectsFromBlocks` for the blocks that slow the player:
    /// cobwebs and sweet berry bushes (`makeStuckInBlock`).
    fn apply_effects_from_blocks<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3) {
        self.in_web = false;
        if self.noclip {
            return;
        }
        let (min, max) = self.collision_shape().aabb(self.pos);
        let (min, max) = (min + DVec3::splat(1.0e-5), max - DVec3::splat(1.0e-5));
        let (lo, hi) = (min.floor().as_ivec3(), max.floor().as_ivec3());
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let Some(b) = world.block(IVec3::new(x, y, z)) else { continue };
                    if b == Block::HONEY_BLOCK {
                        self.honey_slide(IVec3::new(x, y, z), dm);
                        continue;
                    }
                    if b == crate::world::scaffolding::POWDER_SNOW {
                        self.in_powder_snow = true;
                    }
                    let stuck = if b == crate::world::scaffolding::POWDER_SNOW {
                        // Living entities are slowed only when their feet cell is powder snow.
                        if world.block(self.pos.floor().as_ivec3()) != Some(b) {
                            continue;
                        }
                        DVec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32))
                    } else if b == Block::COBWEB {
                        DVec3::new(0.25, 0.05_f32 as f64, 0.25)
                    } else if crate::world::overworld_blocks::berry_age(b).is_some() {
                        DVec3::new(0.8_f32 as f64, 0.75, 0.8_f32 as f64)
                    } else {
                        continue;
                    };
                    self.motion.stuck = stuck;
                    self.motion.fall_distance = 0.0;
                    self.in_web = true;
                }
            }
        }
    }
}

mod honey;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod powder_snow_tests;
