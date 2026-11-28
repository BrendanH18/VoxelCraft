//! v1.0 gap Track 7b: the client-side animation state Java 26.3 keeps on
//! its entities for the animations that aren't driven by the walk, ticked
//! once a game tick from what the simulation did ([`super::Mob::tick_count`]
//! and the tick's movement) and read with the partial tick by the models.
//! Checked against the 26.3 client: `Chicken.aiStep` and
//! `Parrot.calculateFlapping` (`flap`, `flapSpeed`), `Squid.aiStep`
//! (`tentacleAngle`, `xBodyRot`, `zBodyRot`), `Guardian.aiStep`
//! (`clientSideTailAnimation`, `clientSideSpikesAnimation`),
//! `Bee.updateRollAmount`, `AbstractHorse.aiStep`'s `moveTail` and
//! `tick`'s `tailCounter` (rearing, `standAnim`, is Track 3b's
//! `mounts::State`), `LivingEntity.updateSwimAmount`, the
//! `AnimationState`s of `Rabbit`, `Bat` and `Breeze`, and
//! `Rabbit.jumpTicks`. Not saved, as in Java. The record is
//! `docs/verification/2026-11-27-v1.0-gap-track-07b-mob-animations.md`.

use glam::DVec3;

use super::{Mob, MobKind};

/// The state, every kind's fields in one small block (Java keeps each on
/// its own entity class).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    /// `Chicken` and `Parrot`: `flap` and `flapSpeed`, with the values of
    /// the tick before (`oFlap`, `oFlapSpeed`), and `flapping`.
    pub flap: f32,
    pub o_flap: f32,
    pub flap_speed: f32,
    pub o_flap_speed: f32,
    flapping: f32,
    /// `Squid`: `tentacleAngle`, `xBodyRot` and `zBodyRot` (degrees), each
    /// with its last tick's value, and `rotateSpeed`.
    pub tentacle_angle: f32,
    pub o_tentacle_angle: f32,
    pub x_body_rot: f32,
    pub o_x_body_rot: f32,
    pub z_body_rot: f32,
    pub o_z_body_rot: f32,
    rotate_speed: f32,
    /// `Guardian`: `clientSideTailAnimation`, its speed, and
    /// `clientSideSpikesAnimation`, with last tick's values.
    pub tail: f32,
    pub o_tail: f32,
    tail_speed: f32,
    pub spikes: f32,
    pub o_spikes: f32,
    /// `Bee.rollAmount` and its last tick's value.
    pub roll: f32,
    pub o_roll: f32,
    /// `AbstractHorse.tailCounter` (0: still; 1..8 the tail swishes).
    pub tail_counter: u8,
    /// `LivingEntity.swimAmount` and its last tick's value.
    pub swim: f32,
    pub o_swim: f32,
    /// `Rabbit.jumpTicks` and `jumpDuration`; the hop's and the idle head
    /// tilt's `AnimationState` start ticks and `idleAnimationTimeout`.
    jump_ticks: u8,
    jump_duration: u8,
    pub hop_start: Option<u32>,
    pub tilt_start: Option<u32>,
    idle_timeout: i32,
    /// `Bat`: when its flying or resting loop started.
    pub bat_start: Option<u32>,
    bat_resting: bool,
    /// `Allay`: `holdingItemAnimationTicks` (0..5), `dancingAnimationTicks`
    /// and `spinningAnimationTicks` (0..15), with last tick's values.
    pub hold: f32,
    pub o_hold: f32,
    dance_ticks: f32,
    pub spin: f32,
    pub o_spin: f32,
    /// `Breeze.idle`: when the idle loop started.
    pub idle_start: Option<u32>,
    was_on_ground: bool,
    /// v1.0 gap Track 7c: the axolotl's one running animation
    /// (`Axolotl.setupAnimationStates`' `soloAnimation`) and its start
    /// tick.
    pub axolotl: AxolotlAnim,
    pub axolotl_start: Option<u32>,
}

/// v1.0 gap Track 7c: which of the axolotl's `AnimationState`s runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxolotlAnim {
    PlayDead,
    Swim,
    Walk,
    WalkUnderWater,
    IdleUnderWater,
    IdleFloorUnderWater,
    #[default]
    IdleFloor,
}

impl Default for Anim {
    fn default() -> Self {
        Self {
            flap: 0.0,
            o_flap: 0.0,
            flap_speed: 0.0,
            o_flap_speed: 0.0,
            flapping: 1.0,
            tentacle_angle: 0.0,
            o_tentacle_angle: 0.0,
            x_body_rot: 0.0,
            o_x_body_rot: 0.0,
            z_body_rot: 0.0,
            o_z_body_rot: 0.0,
            rotate_speed: 0.0,
            tail: 0.0,
            o_tail: 0.0,
            tail_speed: 0.0,
            spikes: 0.0,
            o_spikes: 0.0,
            roll: 0.0,
            o_roll: 0.0,
            tail_counter: 0,
            swim: 0.0,
            o_swim: 0.0,
            jump_ticks: 0,
            jump_duration: 0,
            hop_start: None,
            tilt_start: None,
            // `Rabbit.idleAnimationTimeout` starts at 180 to 219.
            idle_timeout: 200,
            bat_start: None,
            bat_resting: false,
            hold: 0.0,
            o_hold: 0.0,
            dance_ticks: 0.0,
            spin: 0.0,
            o_spin: 0.0,
            idle_start: None,
            was_on_ground: true,
            axolotl: AxolotlAnim::IdleFloor,
            axolotl_start: None,
        }
    }
}

impl Anim {
    /// `Allay.isSpinning`: the first 15 of every 55 dancing ticks.
    pub fn spinning(&self) -> bool {
        self.dance_ticks % 55.0 < 15.0
    }
}

/// Java's `Mth.lerp(partial, old, new)`.
pub fn lerp(alpha: f32, old: f32, new: f32) -> f32 {
    old + (new - old) * alpha
}

/// A stable value in 0..1 for the client's random rolls (Java's client
/// `RandomSource`, which isn't reproducible anyway): this mob's own, a new
/// one each tick and `salt`.
fn roll(uid: u32, tick: u32, salt: u32) -> f32 {
    let mut x = (u64::from(uid) << 32 | u64::from(tick)) ^ (u64::from(salt) * 0x9e37_79b9_7f4a_7c15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// `Rabbit.jumpDuration` for every hop.
pub const RABBIT_JUMP: u8 = 15;

impl Mob {
    /// v1.0 gap Track 7b: one tick of the client animations after the mob
    /// moved by `moved`.
    pub(crate) fn tick_anim(&mut self, moved: DVec3) {
        let tick = self.tick_count;
        let (uid, on_ground, in_water, alive) = (self.uid, self.on_ground, self.in_water, self.alive());
        let hunts = self.drowned_swims();
        let a = &mut self.anim;
        match self.kind {
            MobKind::Chicken | MobKind::Parrot => {
                // `Chicken.aiStep` and `Parrot.calculateFlapping` (a parrot
                // on a shoulder or riding folds its wings).
                let flying = !on_ground && self.riding.is_none();
                (a.o_flap, a.o_flap_speed) = (a.flap, a.flap_speed);
                a.flap_speed = (a.flap_speed + if flying { 4.0 } else { -1.0 } * 0.3).clamp(0.0, 1.0);
                if !on_ground && a.flapping < 1.0 {
                    a.flapping = 1.0;
                }
                a.flapping *= 0.9;
                a.flap += a.flapping * 2.0;
            }
            MobKind::Squid | MobKind::GlowSquid => {
                // `Squid.aiStep` from the simulation's tentacle cycle.
                let movement = self.aquatic.as_ref().map_or(0.0, |s| s.tentacle);
                (a.o_x_body_rot, a.o_z_body_rot, a.o_tentacle_angle) = (a.x_body_rot, a.z_body_rot, a.tentacle_angle);
                use std::f32::consts::PI;
                if in_water {
                    if movement < PI {
                        let scale = movement / PI;
                        a.tentacle_angle = (scale * scale * PI).sin() * PI * 0.25;
                        a.rotate_speed = if scale > 0.75 { 1.0 } else { a.rotate_speed * 0.8 };
                    } else {
                        a.tentacle_angle = 0.0;
                        a.rotate_speed *= 0.99;
                    }
                    let horizontal = moved.x.hypot(moved.z) as f32;
                    a.z_body_rot += PI * a.rotate_speed * 1.5;
                    a.x_body_rot += (-(horizontal.atan2(moved.y as f32)).to_degrees() - a.x_body_rot) * 0.1;
                } else {
                    a.tentacle_angle = movement.sin().abs() * PI * 0.25;
                    a.x_body_rot += (-90.0 - a.x_body_rot) * 0.02;
                }
            }
            MobKind::Guardian | MobKind::ElderGuardian => {
                // `Guardian.aiStep`, client side, while alive.
                if alive {
                    let moving = self.move_ctl.moving;
                    a.o_tail = a.tail;
                    a.tail_speed = if !in_water {
                        2.0
                    } else if moving {
                        if a.tail_speed < 0.5 { 4.0 } else { a.tail_speed + (0.5 - a.tail_speed) * 0.1 }
                    } else {
                        a.tail_speed + (0.125 - a.tail_speed) * 0.2
                    };
                    a.tail += a.tail_speed;
                    a.o_spikes = a.spikes;
                    a.spikes = if !in_water {
                        roll(uid, tick, 1)
                    } else if moving {
                        a.spikes + (0.0 - a.spikes) * 0.25
                    } else {
                        a.spikes + (1.0 - a.spikes) * 0.06
                    };
                }
            }
            MobKind::Bee => {
                // `Bee.updateRollAmount`.
                a.o_roll = a.roll;
                a.roll = if self.bee.is_some_and(|b| b.rolling) {
                    (a.roll + 0.2).min(1.0)
                } else {
                    (a.roll - 0.24).max(0.0)
                };
            }
            k if super::mounts::equine(k) => {
                // `AbstractHorse.aiStep`'s `moveTail` (one tick in 200)
                // and `tick`'s count.
                if a.tail_counter == 0 && roll(uid, tick, 2) < 1.0 / 200.0 {
                    a.tail_counter = 1;
                }
                if a.tail_counter > 0 {
                    a.tail_counter += 1;
                    if a.tail_counter > 8 {
                        a.tail_counter = 0;
                    }
                }
            }
            MobKind::Drowned => {
                // `updateSwimAmount`: `Drowned.updateSwimming` swims (is
                // visually swimming) under water while it wants to swim,
                // here while it hunts in the water.
                let swimming = alive && in_water && hunts;
                (a.o_swim, a.swim) =
                    (a.swim, if swimming { (a.swim + 0.09).min(1.0) } else { (a.swim - 0.09).max(0.0) });
            }
            MobKind::Rabbit => {
                // A hop starts as the rabbit leaves the ground
                // (`Rabbit.startJumping`: `jumpDuration` 15), and
                // `jumpTicks` counts it out.
                if a.was_on_ground && !on_ground && moved.y > 0.0 {
                    (a.jump_duration, a.jump_ticks) = (RABBIT_JUMP, 0);
                }
                if a.jump_ticks != a.jump_duration {
                    a.jump_ticks += 1;
                } else if a.jump_duration != 0 {
                    (a.jump_ticks, a.jump_duration) = (0, 0);
                }
                // `Rabbit.setupAnimationStates`.
                if a.idle_timeout <= 0 && self.leash.is_none() {
                    a.idle_timeout = 180 + (roll(uid, tick, 3) * 40.0) as i32;
                    a.tilt_start = Some(tick);
                } else if a.jump_ticks > 0 {
                    a.hop_start.get_or_insert(tick);
                    a.tilt_start = None;
                } else {
                    a.idle_timeout -= 1;
                    a.hop_start = None;
                }
            }
            MobKind::Bat => {
                // `Bat.setupAnimationStates`: each loop starts when the bat
                // takes off or hangs up.
                let resting = self.bat.as_ref().is_some_and(|s| s.resting);
                if a.bat_start.is_none() || resting != a.bat_resting {
                    (a.bat_start, a.bat_resting) = (Some(tick), resting);
                }
            }
            MobKind::Allay => {
                // `Allay.tick`, client side.
                let (holding, dancing) = self.allay.as_ref().map_or((false, false), |s| (s.liked.is_some(), s.dancing));
                a.o_hold = a.hold;
                a.hold = (a.hold + if holding { 1.0 } else { -1.0 }).clamp(0.0, 5.0);
                if dancing {
                    a.dance_ticks += 1.0;
                    a.o_spin = a.spin;
                    a.spin = (a.spin + if a.spinning() { 1.0 } else { -1.0 }).clamp(0.0, 15.0);
                } else {
                    (a.dance_ticks, a.spin, a.o_spin) = (0.0, 0.0, 0.0);
                }
            }
            MobKind::Breeze => {
                a.idle_start.get_or_insert(tick);
            }
            MobKind::Axolotl => {
                // v1.0 gap Track 7c: `Axolotl.setupAnimationStates`; a new
                // state starts its animation, the others stop.
                use AxolotlAnim::*;
                let moving = self.limb_amp > 1e-5;
                let now = if self.aquatic.as_ref().is_some_and(|s| s.play_dead > 0.0) {
                    PlayDead
                } else if moving {
                    match (in_water, on_ground) {
                        (true, false) => Swim,
                        (false, true) => Walk,
                        _ => WalkUnderWater,
                    }
                } else {
                    match (in_water, on_ground) {
                        (true, false) => IdleUnderWater,
                        (true, true) => IdleFloorUnderWater,
                        _ => IdleFloor,
                    }
                };
                if a.axolotl_start.is_none() || now != a.axolotl {
                    (a.axolotl, a.axolotl_start) = (now, Some(tick));
                }
            }
            _ => {}
        }
        a.was_on_ground = on_ground;
    }

    /// Whether a drowned swims (`Drowned.wantsToSwim`): it hunts.
    fn drowned_swims(&self) -> bool {
        self.ai == super::mob::Ai::Chase || self.hunt.is_some()
    }

    /// v1.0 gap Track 7b: `KeyframeAnimation.apply`'s place in a looping
    /// animation of `length` seconds started at tick `start`, at
    /// `ageInTicks` (whole milliseconds, as `AnimationState.getTimeInMillis`):
    /// 0..1 round the loop.
    pub fn loop_time(&self, start: Option<u32>, length: f32, alpha: f32) -> Option<f32> {
        let start = start?;
        let ms = ((self.age_in_ticks(alpha) - start as f32) * 50.0).trunc();
        Some((ms / 1000.0 / length).rem_euclid(1.0))
    }

    /// v1.0 gap Track 7b: `Rabbit.getJumpCompletion`, 0 when not hopping.
    pub fn jump_completion(&self, alpha: f32) -> f32 {
        let a = &self.anim;
        if a.jump_duration == 0 { 0.0 } else { (f32::from(a.jump_ticks) + alpha) / f32::from(a.jump_duration) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    /// `Chicken.aiStep`: airborne, `flapSpeed` climbs 1.2 a tick (to 1) and
    /// `flap` advances by `flapping * 2`, `flapping` held at 1 then times
    /// 0.9; on the ground `flapSpeed` falls 0.3 a tick.
    #[test]
    fn a_falling_chicken_flaps_as_java_s() {
        let mut m = Mob::new(MobKind::Chicken, DVec3::ZERO, 0.0);
        m.on_ground = false;
        m.tick_anim(DVec3::ZERO);
        assert!(close(m.anim.flap_speed, 1.0) && close(m.anim.flap, 1.8), "{:?}", m.anim);
        m.tick_anim(DVec3::ZERO);
        assert!(close(m.anim.flap, 3.6) && close(m.anim.o_flap, 1.8));
        m.on_ground = true;
        m.tick_anim(DVec3::ZERO);
        assert!(close(m.anim.flap_speed, 0.7));
        assert!(close(m.anim.flap, 3.6 + 1.62), "{}", m.anim.flap);
    }

    /// `Squid.aiStep`: in water the tentacles open over the first half of
    /// the cycle (`sin(s^2 * pi) * pi / 4`) and the body spins on
    /// `rotateSpeed`; out of water it tips toward -90 degrees.
    #[test]
    fn the_squid_s_tentacles_follow_its_cycle() {
        let mut m = Mob::new(MobKind::Squid, DVec3::ZERO, 0.0);
        m.in_water = true;
        m.aquatic.as_mut().unwrap().tentacle = std::f32::consts::FRAC_PI_2;
        m.tick_anim(DVec3::ZERO);
        let s: f32 = 0.5;
        assert!(close(m.anim.tentacle_angle, (s * s * std::f32::consts::PI).sin() * std::f32::consts::FRAC_PI_4));
        m.in_water = false;
        m.tick_anim(DVec3::ZERO);
        assert!(close(m.anim.x_body_rot, -1.8), "{}", m.anim.x_body_rot);
    }

    /// `AbstractHorse.tick`: a swish lasts until `tailCounter` passes 8.
    #[test]
    fn a_horse_s_tail_swishes_for_eight_ticks() {
        let mut m = Mob::new(MobKind::Horse, DVec3::ZERO, 0.0);
        m.anim.tail_counter = 1;
        for _ in 0..7 {
            m.tick_anim(DVec3::ZERO);
            assert!(m.anim.tail_counter > 0);
        }
        m.tick_anim(DVec3::ZERO);
        assert_eq!(m.anim.tail_counter, 0);
    }

    /// `KeyframeAnimation.apply`'s clock: whole milliseconds since the
    /// state started, round the loop.
    #[test]
    fn loops_run_from_their_start_tick() {
        let mut m = Mob::new(MobKind::Bat, DVec3::ZERO, 0.0);
        m.tick_count = 13;
        assert_eq!(m.loop_time(None, 0.5, 0.0), None);
        assert!(close(m.loop_time(Some(10), 0.5, 0.0).unwrap(), 0.3));
        assert!(close(m.loop_time(Some(3), 0.5, 0.5).unwrap(), 0.05));
    }

    /// v1.0 gap Track 7c: `Axolotl.setupAnimationStates` runs one
    /// animation at a time, restarting it when the state changes.
    #[test]
    fn an_axolotl_plays_one_animation_at_a_time() {
        let mut m = Mob::new(MobKind::Axolotl, DVec3::ZERO, 0.0);
        (m.in_water, m.on_ground, m.limb_amp) = (true, false, 0.3);
        m.tick_count = 5;
        m.tick_anim(DVec3::ZERO);
        assert_eq!((m.anim.axolotl, m.anim.axolotl_start), (AxolotlAnim::Swim, Some(5)));
        m.tick_count = 6;
        m.tick_anim(DVec3::ZERO);
        assert_eq!(m.anim.axolotl_start, Some(5), "the same state keeps its clock");
        (m.in_water, m.on_ground, m.limb_amp) = (false, true, 0.0);
        m.tick_count = 7;
        m.tick_anim(DVec3::ZERO);
        assert_eq!((m.anim.axolotl, m.anim.axolotl_start), (AxolotlAnim::IdleFloor, Some(7)));
    }

    /// `LivingEntity.updateSwimAmount`: 0.09 a tick toward swimming.
    #[test]
    fn a_hunting_drowned_eases_into_its_swim() {
        let mut m = Mob::new(MobKind::Drowned, DVec3::ZERO, 0.0);
        m.in_water = true;
        m.ai = crate::entity::mob::Ai::Chase;
        m.tick_anim(DVec3::ZERO);
        m.tick_anim(DVec3::ZERO);
        assert!(close(m.anim.swim, 0.18) && close(m.anim.o_swim, 0.09));
    }
}
