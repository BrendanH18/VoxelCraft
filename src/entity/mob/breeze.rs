//! The breeze's fight activity (Java 26.3 `BreezeAi.initFightActivity`):
//! `Shoot` (priority 1), `LongJump` (2), `ShootWhenStuck` (3) and `Slide`
//! (4), with `BreezeAttackEntitySensor` choosing the nearest attackable
//! player in its follow range. Idle breezes stroll at 0.6 speed.
use super::super::breeze::{self as b, Pose};
use super::*;

/// `RandomSource.nextGaussian` from this RNG (Box–Muller).
fn gaussian(rng: &mut Rng) -> f64 {
    let u1 = f64::from(rng.next_f32()).max(1e-7);
    let u2 = f64::from(rng.next_f32());
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// `BreezeUtil.randomPointBehindTarget`: 4–8 blocks out, around the
/// target's back by a gaussian of 45 degrees.
fn point_behind(target: &super::super::Target, rng: &mut Rng) -> DVec3 {
    let look = target.look.with_y(0.0).normalize_or(DVec3::Z);
    let back = (-look.x).atan2(look.z).to_degrees() as f32 + 180.0;
    let yaw = back + gaussian(rng) as f32 * 90.0 / 2.0;
    let distance = 4.0 + rng.next_f32() * 4.0;
    // `Vec3.directionFromRotation(0, yaw)`.
    let r = yaw.to_radians();
    target.pos + DVec3::new(-f64::from(r.sin()), 0.0, f64::from(r.cos())) * f64::from(distance)
}

/// `BreezeUtil.hasLineOfSight`: a collider clip from the breeze's feet.
fn clear<W: MobWorld + ?Sized>(w: &W, from: DVec3, to: DVec3) -> bool {
    from.distance(to) <= 50.0f64.max(b::FOLLOW_RANGE) && line_of_sight(w, from, to)
}

/// `LongJump.snapToSurface`: the first collider 10 blocks down (or up),
/// one above it.
fn snap_to_surface<W: MobWorld + ?Sized>(w: &W, p: DVec3) -> Option<IVec3> {
    let start = p.floor().as_ivec3();
    for dy in 0..=10 {
        let at = start - IVec3::Y * dy;
        if w.block(at).is_some_and(Block::is_solid) {
            return Some(at + IVec3::Y);
        }
    }
    for dy in 1..=10 {
        let at = start + IVec3::Y * dy;
        if w.block(at).is_some_and(Block::is_solid) {
            return Some(at + IVec3::Y);
        }
    }
    None
}

impl Mob {
    pub(super) fn breeze_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let full = self.selector_full();
        let target = self.java_target(world, ctx, rng, b::FOLLOW_RANGE, full, |_| true).copied();
        let s = self.breeze.as_mut().unwrap();
        s.clock += dt;
        let mut ticks = 0;
        while s.clock >= 0.05 - 1e-6 {
            s.clock -= 0.05;
            ticks += 1;
        }
        let mut wish = (None, 0.0);
        for _ in 0..ticks {
            wish = self.breeze_tick(world, ctx, target.as_ref(), rng, events);
        }
        if ticks == 0
            && let Some(t) = self.breeze.as_ref().unwrap().walk_target
        {
            wish = (Some((t - self.pos).with_y(0.0).normalize_or_zero()), self.slide_speed());
        }
        wish
    }

    fn slide_speed(&self) -> f64 {
        crate::entity::path::ground_speed(f64::from(b::MOVEMENT_SPEED * b::SLIDE_SPEED))
    }

    /// One brain tick: count memories down, then run the fight behaviours.
    fn breeze_tick<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        _ctx: &Ctx,
        target: Option<&super::super::Target>,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let on_ground = self.on_ground;
        let in_water = self.in_water;
        let difficulty = ctx_difficulty(self);
        let pos = self.pos;
        let uid = self.uid;
        let eye = pos + DVec3::Y * 1.3452;
        let s = self.breeze.as_mut().unwrap();
        let dec = |n: &mut u32| *n = n.saturating_sub(1);
        dec(&mut s.shoot);
        dec(&mut s.shoot_cooldown);
        dec(&mut s.jump_cooldown);
        s.since_hurt = s.since_hurt.saturating_add(1);
        // `playWhirlSound` every 1-80 ticks while moving.
        if s.whirl == 0 {
            s.whirl = 1 + rng.next_int(80);
            if self.vel.with_y(0.0).length_squared() > 0.01 {
                events.push(EntityEvent::Sound { sound: MobSound::Breeze(b::Cue::Whirl), pos });
            }
        }
        let s = self.breeze.as_mut().unwrap();
        s.whirl -= 1;
        let Some(t) = target else {
            // No fight: `StopAttackingIfTargetInvalid`, then idle strolling.
            if s.pose != Pose::LongJumping {
                s.pose = Pose::Standing;
            }
            s.inhaling = 0;
            s.shoot_charging = 0;
            s.shoot_recovering = 0;
            s.jump_target = None;
            s.walk_target = None;
            return (None, 0.0);
        };
        let to_target = t.pos - pos;
        self.head_target = (wrap((to_target.z as f32).atan2(to_target.x as f32) - self.yaw).clamp(-1.2, 1.2), 0.0);
        let s = self.breeze.as_mut().unwrap();

        // Shoot, already running: charging, then the shot, then recovering.
        if s.shoot_charging > 0 || s.shoot_recovering > 0 {
            if s.shoot_charging > 0 {
                s.shoot_charging -= 1;
                if s.shoot_charging == 0 {
                    // `Shoot.tick`: aim at the target's 0.3 height from the
                    // firing height; speed 0.7, inaccuracy 5 - 4 * difficulty.
                    let from = DVec3::new(pos.x, pos.y + b::HEIGHT / 2.0 + 0.3, pos.z);
                    let aim = DVec3::new(t.pos.x, t.pos.y + t.shape.height * 0.3, t.pos.z) - from;
                    let inaccuracy = (5 - 4 * difficulty) as f64;
                    events.push(EntityEvent::ShootWindCharge { from, dir: aim, inaccuracy, shooter: uid });
                    events.push(EntityEvent::Sound { sound: MobSound::Breeze(b::Cue::Shoot), pos });
                    s.shoot_recovering = b::SHOOT_RECOVER;
                }
            } else {
                s.shoot_recovering -= 1;
                if s.shoot_recovering == 0 {
                    // `Shoot.stop`.
                    s.pose = Pose::Standing;
                    s.shoot_cooldown = b::SHOOT_COOLDOWN;
                    s.shoot = 0;
                }
            }
            self.stop_navigation();
            return (None, 0.0);
        }

        // LongJump, already running: inhale, launch, land.
        if s.pose == Pose::Inhaling || s.pose == Pose::LongJumping {
            if in_water && !s.leaving_water && s.pose == Pose::LongJumping {
                // `isFinishedJumping` in water without the leaving flag.
            }
            if s.pose == Pose::Inhaling {
                s.inhaling = s.inhaling.saturating_sub(1);
                if s.inhaling == 0 {
                    let target = s.jump_target;
                    let jump = target.and_then(|at| {
                        let mut angles = b::JUMP_ANGLES;
                        for n in (1..angles.len()).rev() {
                            angles.swap(n, rng.next_int(n as u32 + 1) as usize);
                        }
                        let at = DVec3::new(at.x, at.y, at.z);
                        angles
                            .into_iter()
                            .find_map(|a| super::frog::jump_vector_for(self, w, at, a, b::MAX_JUMP_SPEED, false))
                    });
                    let s = self.breeze.as_mut().unwrap();
                    match jump {
                        Some(v) => {
                            s.leaving_water = in_water;
                            s.pose = Pose::LongJumping;
                            s.airborne = false;
                            events.push(EntityEvent::Sound { sound: MobSound::Breeze(b::Cue::Jump), pos });
                            self.vel = v * 20.0;
                            self.on_ground = false;
                        }
                        None => {
                            s.pose = Pose::Standing;
                            s.jump_target = None;
                        }
                    }
                }
                return (None, 0.0);
            }
            // Long jumping: wait for the ground (or water once out of it).
            if !on_ground {
                s.airborne = true;
            }
            let finished = s.airborne && (on_ground || (in_water && !s.leaving_water));
            if !in_water {
                s.leaving_water = false;
            }
            if finished {
                s.pose = Pose::Standing;
                s.airborne = false;
                s.jump_target = None;
                s.jump_cooldown = if s.since_hurt < 100 { b::JUMP_COOLDOWN_HURT } else { b::JUMP_COOLDOWN };
                s.shoot = b::SHOOT_AFTER_JUMP;
                events.push(EntityEvent::Sound { sound: MobSound::Breeze(b::Cue::Land), pos });
            }
            return (None, 0.0);
        }

        // A slide walking to its target (`SlideToTargetSink`).
        if let Some(goal) = s.walk_target {
            s.walk_left = s.walk_left.saturating_sub(1);
            let arrived = (goal - pos).with_y(0.0).length_squared() <= 1.0;
            if arrived || s.walk_left == 0 || !on_ground {
                s.walk_target = None;
                s.shoot = b::SHOOT_AFTER_SLIDE;
            } else {
                let speed = self.slide_speed();
                return (self.pursue(w, goal, speed, true, true, rng), speed);
            }
        }
        let s = self.breeze.as_mut().unwrap();

        // 1. Shoot.
        if s.pose == Pose::Standing
            && s.shoot > 0
            && s.shoot_cooldown == 0
            && s.jump_target.is_none()
            && pos.distance_squared(t.pos) < b::SHOOT_RANGE_SQR
        {
            s.pose = Pose::Shooting;
            s.shoot_charging = b::SHOOT_CHARGE;
            events.push(EntityEvent::Sound { sound: MobSound::Breeze(b::Cue::Inhale), pos });
            self.stop_navigation();
            return (None, 0.0);
        }
        if s.pose == Pose::Standing && s.shoot > 0 && pos.distance_squared(t.pos) >= b::SHOOT_RANGE_SQR {
            // `checkExtraStartConditions` erases the memory out of range.
            s.shoot = 0;
        }

        // 2. LongJump (`canRun`).
        if (on_ground || in_water) && s.pose == Pose::Standing && s.jump_cooldown == 0 {
            let distance = pos.distance(t.pos);
            let can_jump = distance > b::INNER_RADIUS
                && w.block(pos.floor().as_ivec3()) != Some(Block::HONEY_BLOCK)
                && (1..=4).all(|dy| {
                    w.block(pos.floor().as_ivec3() + IVec3::Y * dy)
                        .is_none_or(|bl| bl == Block::AIR || bl.holds_water())
                });
            if can_jump {
                let behind = point_behind(t, rng);
                if let Some(cell) = snap_to_surface(w, behind) {
                    let below = w.block(cell - IVec3::Y);
                    let dangerous = below.is_some_and(|bl| bl.is_lava() || bl.is_fire() || bl == Block::MAGMA);
                    let centre = cell.as_dvec3() + DVec3::splat(0.5);
                    if !dangerous && (clear(w, pos, centre) || clear(w, pos, centre + DVec3::Y * 4.0)) {
                        let s = self.breeze.as_mut().unwrap();
                        s.jump_target = Some(DVec3::new(centre.x, cell.y as f64, centre.z));
                        s.inhaling = b::INHALE;
                        s.pose = Pose::Inhaling;
                        events.push(EntityEvent::Sound { sound: MobSound::Breeze(b::Cue::Charge), pos });
                        let to = centre - pos;
                        self.yaw = (to.z as f32).atan2(to.x as f32);
                        self.stop_navigation();
                        return (None, 0.0);
                    }
                }
            }
        }
        let s = self.breeze.as_mut().unwrap();

        // 3. ShootWhenStuck.
        if in_water && s.shoot == 0 {
            s.shoot = b::SHOOT_AFTER_SLIDE;
            return (None, 0.0);
        }

        // 4. Slide.
        if on_ground && !in_water && s.pose == Pose::Standing && s.jump_cooldown == 0 && s.shoot == 0 {
            let away = if pos.distance_squared(t.pos) < b::INNER_RADIUS * b::INNER_RADIUS {
                // `DefaultRandomPos.getPosAway(5, 5)` from the target, with sight.
                (0..10).find_map(|_| {
                    let d = (pos - t.pos).with_y(0.0).normalize_or(DVec3::X);
                    let spin = rng.range(-1.2, 1.2) as f64;
                    let dir = DVec3::new(d.x * spin.cos() - d.z * spin.sin(), 0.0, d.x * spin.sin() + d.z * spin.cos());
                    let p = pos + dir * f64::from(1.0 + rng.next_f32() * 4.0);
                    (clear(w, pos, p) && p.distance_squared(t.pos) > pos.distance_squared(t.pos)).then_some(p)
                })
            } else {
                None
            };
            let goal = away.unwrap_or_else(|| {
                if rng.chance(0.5) {
                    point_behind(t, rng)
                } else {
                    // `randomPointInMiddleCircle`.
                    let d = t.pos - pos;
                    let keep = d.length() - (8.0 + f64::from(rng.next_f32()) * (4.0 - 8.0));
                    pos + d.normalize_or_zero() * DVec3::new(keep, 0.0, keep)
                }
            });
            let s = self.breeze.as_mut().unwrap();
            s.walk_target = Some(goal);
            s.walk_left = b::SLIDE_MIN + rng.next_int(b::SLIDE_MAX - b::SLIDE_MIN + 1);
            let _ = eye;
        }
        (None, 0.0)
    }
}

/// `Difficulty.getId` of the mob's world.
fn ctx_difficulty(m: &Mob) -> i32 {
    use crate::simulation::difficulty::Difficulty;
    match m.difficulty {
        Difficulty::Peaceful => 0,
        Difficulty::Easy => 1,
        Difficulty::Normal => 2,
        Difficulty::Hard => 3,
    }
}
