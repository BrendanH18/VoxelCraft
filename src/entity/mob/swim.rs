//! Java 26.3 movement for swimmers and flyers: each kind's `MoveControl` and
//! its `travel` override, one step per 50 ms tick with `deltaMovement` in
//! blocks per tick (`Mob::vel` holds it times 20).
//!
//! - **Fish:** `AbstractFish.FishMoveControl` (speed eased by 0.125 a tick,
//!   climbing a tenth of it toward the wanted height) and `travelInWater`
//!   (0.01 acceleration, 0.9 drag, sinking 0.005 a tick); a fish on land flops
//!   0.4 up.
//! - **Dolphins and axolotls:** `SmoothSwimmingMoveControl` (10 degrees of
//!   yaw and 5 of pitch a tick, pitch limited to 85; dolphins 0.02 of their
//!   speed in water and 0.1 out of it with a 0.005 lift, axolotls 0.1 and
//!   0.5) and their `travelInWater` at `getSpeed`.
//! - **Guardians:** `GuardianMoveControl`, its tail sway, and `travelInWater`
//!   at 0.1 that sinks only while still.
//! - **Turtles in water:** `TurtleMoveControl.updateSpeed` (slower away from
//!   home and as babies) and `travelInWater` at 0.1.
//! - **Squid:** `Squid.aiStep`: a stroke along the goal's movement vector late
//!   in each tentacle cycle, 0.9 drag between strokes, and a straight fall
//!   out of water.
//! - **Ghasts:** `GhastMoveControl` (a 0.1 push toward the wanted position
//!   every 2–6 ticks while the way is clear) and `travelFlying` (no gravity,
//!   0.91 drag in air).
//! - **Airborne blazes:** `Blaze.aiStep`'s hover (falling slowed to 0.6 a
//!   tick), `customServerAiStep`'s lift toward a target above, and
//!   `travelInAir` at the flying speed.
//!
//! Out of water, fish, dolphins, guardians and squid use `travelInAir`;
//! turtles and axolotls walk with the ground movement in `physics_step`.
use glam::DVec3;

use super::{Mob, MobKind};
use crate::entity::{MobWorld, Rng, path};
use crate::physics::{self, BlockSource};
use crate::world::block::Block;

/// Seconds in one Java tick.
pub(crate) const TICK: f64 = 0.05;
/// `Attributes.GRAVITY`.
const GRAVITY: f64 = 0.08;
/// `MoveControl.MIN_SPEED_SQR`.
const MIN_SPEED_SQR: f64 = 2.500_000_3e-7;

/// A mob's `MoveControl` and `LivingEntity` movement inputs (never saved:
/// Java re-plans after a reload).
#[derive(Clone, Debug, Default)]
pub(crate) struct MoveCtl {
    /// `operation == MOVE_TO`: the wanted position.
    pub wanted: Option<DVec3>,
    /// `speedModifier * MOVEMENT_SPEED` for the wanted position.
    pub speed: f64,
    /// `LivingEntity.speed` (`getSpeed()`).
    pub cur: f64,
    /// `zza` and `yya`, the forward and vertical inputs to `travel`.
    zza: f64,
    yya: f64,
    /// `xRot`, the swimming pitch in degrees.
    pub pitch: f32,
    /// `tickCount`, for the guardian's sway.
    ticks: u32,
    /// `GhastMoveControl.floatDuration`.
    float_duration: i32,
    /// `Guardian.isMoving`.
    pub moving: bool,
    /// `BreathAirGoal` is running: an extra stroke each tick.
    pub breathing: bool,
    /// Simulated seconds not yet spent on a whole tick.
    pub time: f64,
    /// The target a ghast faces instead of its motion (`GhastLookGoal`).
    pub look: Option<DVec3>,
    /// The ghast's and blaze's player target (`Mob.getTarget`) and the
    /// selector updates since `TargetGoal` last saw it.
    pub target: Option<crate::entity::PlayerId>,
    pub unseen: i32,
    /// `BlazeAttackGoal.lastSeen` and `GhastShootFireballGoal.chargeTime`,
    /// in ticks.
    pub last_seen: i32,
    pub charge: i32,
}

impl MobKind {
    /// Kinds with their own `travelInWater` and swimming `MoveControl`.
    pub(crate) fn java_swimmer(self) -> bool {
        matches!(
            self,
            MobKind::Cod
                | MobKind::Salmon
                | MobKind::TropicalFish
                | MobKind::Pufferfish
                | MobKind::Squid
                | MobKind::GlowSquid
                | MobKind::Dolphin
                | MobKind::Axolotl
                | MobKind::Guardian
                | MobKind::ElderGuardian
                | MobKind::Turtle
        )
    }

    /// Swimmers that never walk: Java's travel moves them on land as well.
    fn water_bound(self) -> bool {
        self.java_swimmer() && !matches!(self, MobKind::Axolotl | MobKind::Turtle)
    }

    pub(crate) fn is_fish(self) -> bool {
        matches!(self, MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish | MobKind::Pufferfish)
    }
}

/// `Entity.getInputVector` added to `dm`: `input` (normalized past length 1)
/// times `speed`, turned to face `yrot` degrees.
fn move_relative(dm: &mut DVec3, speed: f64, input: DVec3, yrot: f32) {
    let len = input.length_squared();
    if len < 1e-7 {
        return;
    }
    let v = if len > 1.0 { input.normalize() } else { input } * speed;
    let (s, c) = yrot.to_radians().sin_cos();
    let (s, c) = (s as f64, c as f64);
    *dm += DVec3::new(v.x * c - v.z * s, v.y, v.z * c + v.x * s);
}

/// `Mth.wrapDegrees`.
fn wrap_degrees(a: f32) -> f32 {
    let a = a.rem_euclid(360.0);
    if a >= 180.0 { a - 360.0 } else { a }
}

/// `MoveControl.rotlerp`: turn `a` toward `b` by at most `max` degrees.
fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    let r = a + wrap_degrees(b - a).clamp(-max, max);
    if r < 0.0 {
        r + 360.0
    } else if r > 360.0 {
        r - 360.0
    } else {
        r
    }
}

/// `Mth.rotateTowards`.
fn rotate_towards(from: f32, to: f32, max: f32) -> f32 {
    from + wrap_degrees(to - from).clamp(-max, max)
}

/// The yaw (degrees, Java's `yRot`) facing along `d`.
fn yaw_to(d: DVec3) -> f32 {
    (d.z.atan2(d.x).to_degrees() as f32) - 90.0
}

/// `Block.getFriction` of what a mob stands on.
fn friction(b: Option<Block>) -> f64 {
    match b {
        Some(Block::ICE | crate::world::gadgets::PACKED_ICE) => 0.98,
        Some(crate::world::overworld_blocks::BLUE_ICE) => 0.989,
        _ => 0.6,
    }
}

impl Mob {
    /// Java's `yRot` in degrees (VoxelCraft's yaw faces +X at zero).
    fn yrot(&self) -> f32 {
        self.yaw.to_degrees() - 90.0
    }

    fn set_yrot(&mut self, r: f32) {
        self.yaw = super::wrap((r + 90.0).to_radians());
    }

    /// Whether this tick moves with Java's per-tick travel instead of the
    /// ground and air approximation in `physics_step`.
    pub(super) fn java_moves<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        match self.kind {
            k if k.water_bound() => true,
            MobKind::Axolotl | MobKind::Turtle => self.touching_water(world),
            MobKind::Ghast => true,
            MobKind::Blaze => self.alive() && (!self.on_ground || self.lift),
            _ => false,
        }
    }

    /// `isInWater`: the box touches water.
    fn touching_water<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        physics::touches_block(world, self.pos, self.shape(), Block::holds_water)
    }

    /// `isInLava`.
    fn touching_lava<W: BlockSource + ?Sized>(&self, world: &W) -> bool {
        physics::touches_block(world, self.pos, self.shape(), Block::is_lava)
    }

    /// The wanted position while navigation still has a path to follow.
    fn move_target(&self) -> Option<DVec3> {
        self.move_ctl.wanted.filter(|_| !self.nav.is_done())
    }

    /// `Entity.move`: collide `dm` against blocks, stepping up `maxUpStep`
    /// from the ground. Collisions zero the blocked components of `dm`.
    fn java_move<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3, step: f64) {
        let shape = self.shape();
        let delta = *dm;
        let hit = if self.on_ground && step > 0.0 {
            physics::move_box_stepping(world, &mut self.pos, dm, delta, shape, step)
        } else {
            physics::move_box(world, &mut self.pos, dm, delta, shape)
        };
        self.on_ground = hit.on_ground;
        self.blocked = hit.horizontal;
    }

    /// `maxUpStep` (the `STEP_HEIGHT` attribute).
    fn step_height(&self) -> f64 {
        if matches!(self.kind, MobKind::Turtle | MobKind::Axolotl) { 1.0 } else { 0.6 }
    }

    /// `LivingEntity.travelInAir` with `input` and the mob's `getSpeed()`.
    fn travel_in_air<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3, input: DVec3) {
        let below = (self.pos - DVec3::Y * 0.5).floor().as_ivec3();
        let f = if self.on_ground { friction(world.block(below)) } else { 1.0 };
        // `getFrictionInfluencedSpeed`.
        let speed = if !self.on_ground {
            0.02
        } else if f > 0.6 {
            self.move_ctl.cur * (0.216 / (f * f * f))
        } else {
            self.move_ctl.cur
        };
        move_relative(dm, speed, input, self.yrot());
        self.java_move(world, dm, self.step_height());
        let y = dm.y - GRAVITY;
        *dm = DVec3::new(dm.x * f * 0.91, y * 0.98, dm.z * f * 0.91);
    }

    /// One Java tick of movement; `wish`/`speed` are the ground steering for
    /// the airborne blaze. `self.vel` is read and written in blocks a second.
    pub(super) fn java_tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        wish: Option<DVec3>,
        speed: f64,
        rng: &mut Rng,
    ) {
        let mut dm = self.vel * TICK;
        let in_water = self.touching_water(world);
        self.in_water = in_water;
        self.move_ctl.ticks = self.move_ctl.ticks.wrapping_add(1);
        let alive = self.alive();
        // Land flops come before `LivingEntity.aiStep`.
        if alive && !in_water && self.on_ground {
            let (h, up) = match self.kind {
                k if k.is_fish() => (0.05, 0.4),
                MobKind::Guardian | MobKind::ElderGuardian => (0.4, 0.5),
                _ => (0.0, 0.0),
            };
            if up > 0.0 {
                dm += DVec3::new((rng.next_f32() as f64 * 2.0 - 1.0) * h, up, (rng.next_f32() as f64 * 2.0 - 1.0) * h);
                if !self.kind.is_fish() {
                    self.set_yrot(rng.next_f32() * 360.0);
                }
                self.on_ground = false;
            }
        }
        if self.kind == MobKind::Blaze && !self.on_ground && dm.y < 0.0 {
            dm.y *= 0.6;
        }
        // `LivingEntity.aiStep` drops tiny motion.
        for v in [&mut dm.x, &mut dm.y, &mut dm.z] {
            if v.abs() < 0.003 {
                *v = 0.0;
            }
        }
        match self.kind {
            k if k.is_fish() => self.fish_tick(world, &mut dm, in_water, alive),
            MobKind::Dolphin | MobKind::Axolotl => self.smooth_swim_tick(world, &mut dm, in_water, alive),
            MobKind::Guardian | MobKind::ElderGuardian => self.guardian_tick(world, &mut dm, in_water, alive),
            MobKind::Turtle => self.turtle_tick(world, &mut dm, alive),
            MobKind::Squid | MobKind::GlowSquid => self.squid_tick(world, &mut dm, in_water, alive, rng),
            MobKind::Ghast => self.ghast_tick(world, &mut dm, alive, rng),
            MobKind::Blaze => self.blaze_tick(world, &mut dm, wish, speed),
            _ => {}
        }
        // `Dolphin.tick` flops after the entity tick, when dry.
        if self.kind == MobKind::Dolphin && alive && !in_water && self.on_ground && !world.rains_on(self.block_pos()) {
            dm += DVec3::new((rng.next_f32() as f64 * 2.0 - 1.0) * 0.2, 0.5, (rng.next_f32() as f64 * 2.0 - 1.0) * 0.2);
            self.set_yrot(rng.next_f32() * 360.0);
            self.on_ground = false;
        }
        self.vel = dm / TICK;
    }

    fn block_pos(&self) -> glam::IVec3 {
        self.pos.floor().as_ivec3()
    }

    /// `FishMoveControl.tick` and `AbstractFish.travelInWater`.
    fn fish_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, in_water: bool, alive: bool) {
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        if world.block(eye.floor().as_ivec3()).is_some_and(Block::holds_water) {
            dm.y += 0.005;
        }
        match self.move_target().filter(|_| alive) {
            Some(wanted) => {
                let target = self.target_speed();
                let c = &mut self.move_ctl;
                c.cur += 0.125 * (target - c.cur);
                let d = wanted - self.pos;
                if d.y != 0.0 {
                    dm.y += c.cur * (d.y / d.length()) * 0.1;
                }
                if d.x != 0.0 || d.z != 0.0 {
                    let r = rotlerp(self.yrot(), yaw_to(d), 90.0);
                    self.set_yrot(r);
                }
            }
            None => self.move_ctl.cur = 0.0,
        }
        self.move_ctl.zza = self.move_ctl.cur;
        let input = DVec3::new(0.0, 0.0, self.move_ctl.zza);
        if in_water {
            move_relative(dm, 0.01, input, self.yrot());
            self.java_move(world, dm, self.step_height());
            *dm *= 0.9;
            // Fish never have an attack target.
            dm.y -= 0.005;
        } else {
            self.travel_in_air(world, dm, input);
        }
    }

    /// `SmoothSwimmingMoveControl.tick` and the dolphin's and axolotl's
    /// `travelInWater`.
    fn smooth_swim_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, in_water: bool, alive: bool) {
        let dolphin = self.kind == MobKind::Dolphin;
        let (in_factor, out_factor) = if dolphin { (0.02, 0.1) } else { (0.1, 0.5) };
        if dolphin && in_water {
            dm.y += 0.005;
        }
        if dolphin && self.move_ctl.breathing {
            // `BreathAirGoal.tick`: one more stroke on last tick's inputs.
            let input = DVec3::new(0.0, self.move_ctl.yya, self.move_ctl.zza);
            move_relative(dm, 0.02, input, self.yrot());
            self.java_move(world, dm, self.step_height());
        }
        let playing_dead = self.aquatic.as_ref().is_some_and(|a| a.play_dead > 0.0);
        match self.move_target().filter(|_| alive && !playing_dead) {
            Some(wanted) => {
                let d = wanted - self.pos;
                if d.length_squared() < MIN_SPEED_SQR {
                    self.move_ctl.zza = 0.0;
                } else {
                    let target = yaw_to(d);
                    let r = rotlerp(self.yrot(), target, 10.0);
                    self.set_yrot(r);
                    let speed = self.target_speed();
                    if in_water {
                        self.move_ctl.cur = speed * in_factor;
                        let h = d.x.hypot(d.z);
                        if d.y.abs() > 1e-5 || h > 1e-5 {
                            let x_rot = wrap_degrees(-(d.y.atan2(h).to_degrees() as f32)).clamp(-85.0, 85.0);
                            self.move_ctl.pitch = rotate_towards(self.move_ctl.pitch, x_rot, 5.0);
                        }
                        let (s, c) = self.move_ctl.pitch.to_radians().sin_cos();
                        self.move_ctl.zza = c as f64 * speed;
                        self.move_ctl.yya = -s as f64 * speed;
                    } else {
                        let left = wrap_degrees(self.yrot() - target).abs();
                        let turning = 1.0 - ((left - 10.0) / 50.0).clamp(0.0, 1.0) as f64;
                        self.move_ctl.cur = speed * out_factor * turning;
                        self.move_ctl.zza = self.move_ctl.cur;
                    }
                }
            }
            None => {
                self.move_ctl.cur = 0.0;
                self.move_ctl.zza = 0.0;
                self.move_ctl.yya = 0.0;
            }
        }
        let input = DVec3::new(0.0, self.move_ctl.yya, self.move_ctl.zza);
        if in_water {
            move_relative(dm, self.move_ctl.cur, input, self.yrot());
            self.java_move(world, dm, self.step_height());
            *dm *= 0.9;
            // Dolphins sink without an attack target; axolotls drift.
            if dolphin {
                dm.y -= 0.005;
            }
        } else {
            self.travel_in_air(world, dm, input);
        }
    }

    /// `GuardianMoveControl.tick` and `Guardian.travelInWater`.
    fn guardian_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, in_water: bool, alive: bool) {
        let aiming = self.aquatic.as_ref().is_some_and(|a| a.beam.is_some());
        match self.move_target().filter(|_| alive && !aiming) {
            Some(wanted) => {
                let d = wanted - self.pos;
                let len = d.length();
                let r = rotlerp(self.yrot(), yaw_to(d), 90.0);
                self.set_yrot(r);
                let target = self.target_speed();
                let c = &mut self.move_ctl;
                c.cur += 0.125 * (target - c.cur);
                let t = c.ticks.wrapping_add(self.uid) as f64;
                let push = (t * 0.5).sin() * 0.05;
                let (s, co) = (r as f64).to_radians().sin_cos();
                let y_push = (t * 0.75).sin() * 0.05;
                *dm += DVec3::new(push * co, y_push * (s + co) * 0.25 + c.cur * (d.y / len) * 0.1, push * s);
                c.moving = true;
            }
            None => {
                self.move_ctl.cur = 0.0;
                self.move_ctl.moving = false;
            }
        }
        self.move_ctl.zza = self.move_ctl.cur;
        let input = DVec3::new(0.0, 0.0, self.move_ctl.zza);
        if in_water {
            move_relative(dm, 0.1, input, self.yrot());
            self.java_move(world, dm, self.step_height());
            *dm *= 0.9;
            if !self.move_ctl.moving && !aiming {
                dm.y -= 0.005;
            }
        } else {
            self.travel_in_air(world, dm, input);
        }
    }

    /// `TurtleMoveControl.tick` and `Turtle.travelInWater`.
    fn turtle_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, alive: bool) {
        let home = self.animal.as_ref().map(|a| a.home.as_dvec3() + DVec3::splat(0.5));
        let near_home = |pos: DVec3, r: f64| home.is_some_and(|h| h.distance_squared(pos) < r * r);
        dm.y += 0.005;
        let c = &mut self.move_ctl;
        if !near_home(self.pos, 16.0) {
            c.cur = (c.cur / 2.0).max(0.08);
        }
        if self.baby {
            c.cur = (c.cur / 3.0).max(0.06);
        }
        match self.move_target().filter(|_| alive) {
            Some(wanted) => {
                let d = wanted - self.pos;
                let dd = d.length();
                if dd < 1e-5 {
                    self.move_ctl.cur = 0.0;
                } else {
                    let r = rotlerp(self.yrot(), yaw_to(d), 90.0);
                    self.set_yrot(r);
                    let target = self.target_speed();
                    let c = &mut self.move_ctl;
                    c.cur += 0.125 * (target - c.cur);
                    dm.y += c.cur * (d.y / dd) * 0.1;
                }
            }
            None => self.move_ctl.cur = 0.0,
        }
        self.move_ctl.zza = self.move_ctl.cur;
        move_relative(dm, 0.1, DVec3::new(0.0, 0.0, self.move_ctl.zza), self.yrot());
        self.java_move(world, dm, self.step_height());
        *dm *= 0.9;
        let going_home = self.animal.as_ref().is_some_and(|a| a.goal.is_some());
        if !(going_home && near_home(self.pos, 20.0)) {
            dm.y -= 0.005;
        }
    }

    /// `Squid.aiStep` and `Squid.travel` (a plain `move`).
    fn squid_tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        dm: &mut DVec3,
        in_water: bool,
        alive: bool,
        rng: &mut Rng,
    ) {
        let a = self.aquatic.as_mut().unwrap();
        a.tentacle += a.tentacle_speed;
        if a.tentacle > std::f32::consts::TAU {
            a.tentacle -= std::f32::consts::TAU;
            if rng.next_int(10) == 0 {
                a.tentacle_speed = 1.0 / (rng.next_f32() + 1.0) * 0.2;
            }
        }
        if in_water {
            if a.tentacle < std::f32::consts::PI {
                if a.tentacle / std::f32::consts::PI > 0.75 && alive {
                    *dm = a.vector;
                }
            } else {
                *dm *= 0.9;
            }
            if dm.x != 0.0 || dm.z != 0.0 {
                // `yBodyRot` eases a tenth of the way toward the motion.
                let body = self.yrot();
                let target = -(dm.x.atan2(dm.z).to_degrees() as f32);
                self.set_yrot(body + wrap_degrees(target - body) * 0.1);
            }
        } else {
            *dm = DVec3::new(0.0, (dm.y - GRAVITY) * 0.98, 0.0);
        }
        self.java_move(world, dm, 0.0);
    }

    /// `GhastMoveControl.tick`, `Ghast.faceMovementDirection` and
    /// `travelFlying`. The look goal faces the target, set by `think`.
    fn ghast_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, alive: bool, rng: &mut Rng) {
        if alive && let Some(wanted) = self.move_ctl.wanted {
            self.move_ctl.float_duration -= 1;
            if self.move_ctl.float_duration < 0 {
                self.move_ctl.float_duration += rng.next_int(5) as i32 + 2;
                let travel = wanted - self.pos;
                if self.ghast_can_reach(world, travel) {
                    // `FLYING_SPEED` 0.06 times 5/3.
                    *dm += travel.normalize_or_zero() * (0.06 * 5.0 / 3.0);
                } else {
                    self.move_ctl.wanted = None;
                }
            }
        }
        if alive && self.move_ctl.look.is_none() && (dm.x != 0.0 || dm.z != 0.0) {
            self.set_yrot(-(dm.x.atan2(dm.z).to_degrees() as f32));
        }
        // No input: `zza` stays zero, so `moveRelative` adds nothing.
        let drag = if self.touching_water(world) {
            0.8
        } else if self.touching_lava(world) {
            0.5
        } else {
            0.91
        };
        self.java_move(world, dm, 0.0);
        *dm *= drag;
    }

    /// `GhastMoveControl.canReach`: the box swept from here by `travel`
    /// meets no block it doesn't already touch.
    fn ghast_can_reach<W: BlockSource + ?Sized>(&self, world: &W, travel: DVec3) -> bool {
        physics::sweep_clear(world, self.pos, self.shape(), travel)
    }

    /// `Blaze.customServerAiStep`'s lift, the `MoveControl` toward `wish`
    /// and `travelInAir`, for a blaze off the ground or rising to a target.
    fn blaze_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, wish: Option<DVec3>, speed: f64) {
        if self.lift {
            dm.y += (0.3 - dm.y) * 0.3;
        }
        match wish {
            Some(dir) => {
                let r = rotlerp(self.yrot(), yaw_to(dir), 90.0);
                self.set_yrot(r);
                self.move_ctl.cur = path::java_speed(speed) * self.effects.speed_factor();
            }
            None => self.move_ctl.cur = 0.0,
        }
        self.move_ctl.zza = self.move_ctl.cur;
        self.travel_in_air(world, dm, DVec3::new(0.0, 0.0, self.move_ctl.zza));
    }

    /// `speedModifier * getAttributeValue(MOVEMENT_SPEED)`: the wanted speed
    /// with Speed and Slowness applied to the attribute.
    fn target_speed(&self) -> f64 {
        self.move_ctl.speed * self.effects.speed_factor()
    }

    /// Point the move control at `wanted` at the ground speed `speed`
    /// (blocks a second), as navigation's `setWantedPosition`.
    pub(super) fn set_wanted(&mut self, wanted: Option<DVec3>, speed: f64) {
        self.move_ctl.wanted = wanted;
        self.move_ctl.speed = path::java_speed(speed);
    }

    /// `Ghast.RandomFloatAroundGoal` on the selector's full update: when
    /// there is no wanted position, or it is within a block or past 60, pick
    /// one up to 16 blocks away on each axis.
    pub(super) fn ghast_float(&mut self, rng: &mut Rng) {
        if let Some(w) = self.move_ctl.wanted {
            let dd = w.distance_squared(self.pos);
            if (1.0..=3600.0).contains(&dd) {
                return;
            }
        }
        // `distanceToBlocks` is 0, so `isGoodTarget` accepts the first spot
        // and the heightmap fallback never runs.
        let mut r = || (rng.next_f32() as f64 * 2.0 - 1.0) * 16.0;
        let target = self.pos + DVec3::new(r(), r(), r());
        self.move_ctl.wanted = Some(target);
        self.move_ctl.speed = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;
    use glam::IVec3;

    fn ocean() -> Grid {
        let mut w = Grid::flat(-64);
        for x in -20..=120 {
            for y in -20..=20 {
                for z in -20..=20 {
                    w.set(IVec3::new(x, y, z), Block::WATER);
                }
            }
        }
        w
    }

    /// Steady speeds of Java's water travel: an acceleration `a` a tick
    /// against 0.9 drag moves 10a blocks a tick (the kept motion plus the
    /// tick's own push).
    fn steady(kind: MobKind, speed_mod: f64, ticks: usize) -> f64 {
        let w = ocean();
        let mut m = Mob::new(kind, DVec3::new(0.5, 0.0, 0.5), 0.0);
        m.uid = 2;
        let mut rng = Rng::new(1);
        m.move_ctl.speed = speed_mod;
        let mut last = m.pos;
        let mut v = 0.0;
        for _ in 0..ticks {
            // Keep a fixed wanted position far ahead, with a live path.
            m.move_ctl.wanted = Some(m.pos + DVec3::new(30.0, 0.0, 0.0));
            m.nav.force_not_done();
            m.java_tick(&w, None, 0.0, &mut rng);
            v = (m.pos - last).length();
            last = m.pos;
        }
        v
    }

    #[test]
    fn swimmers_settle_at_javas_water_speeds() {
        // Cod: speed eases to 0.7 (the default movement speed); 0.01 * 0.7
        // a tick against 0.9 drag is 0.063 blocks a tick.
        // Cod: speed eases to 0.7 (the default movement speed); 0.01 * 0.7
        // a tick is 0.07 blocks a tick, with the idle 0.005 rise on top.
        let cod = steady(MobKind::Cod, 0.7, 100);
        assert!((cod - 0.0702).abs() < 1e-3, "{cod}");
        // Dolphins: 1.2 * 0.02 on a normalized input, 0.24 a tick.
        let dolphin = steady(MobKind::Dolphin, 1.2, 100);
        assert!((dolphin - 0.24).abs() < 2e-3, "{dolphin}");
        // Axolotls idling in water: modifier 0.5, 0.05 on a 0.5 input.
        let axolotl = steady(MobKind::Axolotl, 0.5, 100);
        assert!((axolotl - 0.25).abs() < 2e-3, "{axolotl}");
        // Guardians: 0.1 * 0.5 a tick, plus the tail's sway.
        let g = steady(MobKind::Guardian, 0.5, 100);
        assert!((0.45..0.55).contains(&g), "{g}");
    }

    /// Slowness lowers the movement speed attribute the control reads:
    /// Slowness II takes 30% off a cod's swimming speed.
    #[test]
    fn slowness_slows_swimmers() {
        let w = ocean();
        let run = |slow: bool| {
            let mut m = Mob::new(MobKind::Cod, DVec3::new(0.5, 0.0, 0.5), 0.0);
            if slow {
                m.effects.add(crate::simulation::effects::Effect::Slowness, 1, 10_000);
            }
            m.move_ctl.speed = 0.7;
            let mut rng = Rng::new(1);
            for _ in 0..100 {
                m.move_ctl.wanted = Some(m.pos + DVec3::new(30.0, 0.0, 0.0));
                m.nav.force_not_done();
                m.java_tick(&w, None, 0.0, &mut rng);
            }
            m.vel.x / 20.0
        };
        let (fast, slow) = (run(false), run(true));
        assert!((slow / fast - 0.7).abs() < 0.01, "{fast} {slow}");
    }

    #[test]
    fn idle_fish_rise_slowly_and_drag_stops_them() {
        let w = ocean();
        let mut m = Mob::new(MobKind::Cod, DVec3::new(0.5, 0.0, 0.5), 0.0);
        m.vel = DVec3::new(2.0, 0.0, 0.0);
        let mut rng = Rng::new(1);
        for _ in 0..100 {
            m.java_tick(&w, None, 0.0, &mut rng);
        }
        // Java drops motion under 0.003 before the eye's 0.005 lift, so an
        // idle fish rises 0.005 a tick until its eye leaves the water.
        assert!((m.pos.y - 0.5).abs() < 1e-6, "{}", m.pos.y);
        assert_eq!(m.vel.x, 0.0);
        // 2 blocks a second is 0.1 a tick; 0.9 drag sums to under a block.
        assert!(m.pos.x < 1.5);
    }

    #[test]
    fn fish_flop_on_land_and_squid_fall_straight() {
        let w = Grid::flat(-1);
        let mut f = Mob::new(MobKind::Cod, DVec3::new(0.5, 0.0, 0.5), 0.0);
        f.on_ground = true;
        let mut rng = Rng::new(3);
        f.java_tick(&w, None, 0.0, &mut rng);
        // 0.4 up, then gravity and drag: (0.4 - 0.08) * 0.98.
        assert!((f.vel.y / 20.0 - 0.3136).abs() < 1e-9);
        let mut s = Mob::new(MobKind::Squid, DVec3::new(0.5, 5.0, 0.5), 0.0);
        s.vel = DVec3::new(3.0, 0.0, 0.0);
        s.java_tick(&w, None, 0.0, &mut rng);
        assert_eq!(s.vel.x, 0.0);
        assert!((s.vel.y / 20.0 + 0.0784).abs() < 1e-9);
    }

    #[test]
    fn dolphins_turn_ten_degrees_a_tick() {
        let w = ocean();
        let mut m = Mob::new(MobKind::Dolphin, DVec3::new(0.5, 0.0, 0.5), 0.0);
        m.move_ctl.speed = 1.2;
        let start = m.yrot();
        m.move_ctl.wanted = Some(m.pos + DVec3::new(0.0, 0.0, -20.0));
        m.nav.force_not_done();
        m.java_tick(&w, None, 0.0, &mut Rng::new(1));
        assert!((wrap_degrees(m.yrot() - start).abs() - 10.0).abs() < 1e-3);
    }

    #[test]
    fn ghasts_drift_toward_their_float_target_without_gravity() {
        let w = Grid::flat(-64);
        let mut m = Mob::new(MobKind::Ghast, DVec3::new(0.5, 80.0, 0.5), 0.0);
        let mut rng = Rng::new(9);
        m.move_ctl.wanted = Some(m.pos + DVec3::new(20.0, 0.0, 0.0));
        let start = m.pos;
        for _ in 0..60 {
            m.java_tick(&w, None, 0.0, &mut rng);
        }
        assert!(m.pos.x > start.x + 3.0);
        assert!((m.pos.y - start.y).abs() < 1e-9);
        // About one 0.1 push every four ticks against 0.91 drag.
        assert!(m.vel.length() / 20.0 < 0.35);
    }

    #[test]
    fn falling_blazes_hover_and_lift_toward_a_target_above() {
        let w = Grid::flat(-64);
        let mut m = Mob::new(MobKind::Blaze, DVec3::new(0.5, 80.0, 0.5), 0.0);
        let mut rng = Rng::new(9);
        for _ in 0..80 {
            m.java_tick(&w, None, 0.0, &mut rng);
        }
        // (0.6 v - 0.08) * 0.98 settles at -0.0784 / 0.412 blocks a tick.
        assert!((m.vel.y / 20.0 + 0.0784 / 0.412).abs() < 1e-3, "{}", m.vel.y / 20.0);
        m.lift = true;
        for _ in 0..80 {
            m.java_tick(&w, None, 0.0, &mut rng);
        }
        // (0.7 v + 0.01) * 0.98 settles at 0.0098 / 0.314.
        assert!((m.vel.y / 20.0 - 0.0098 / 0.314).abs() < 1e-3, "{}", m.vel.y / 20.0);
    }
}
