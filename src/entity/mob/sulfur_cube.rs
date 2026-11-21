//! The sulfur cube's 26.3 goals and ball physics (see `entity::sulfur_cube`).
//!
//! Without a block: `CubeMobFloatGoal` (1), the tempt goal (2), the item
//! search (3), `CubeMobRandomDirectionGoal` (4) and
//! `CubeMobKeepOnJumpingGoal` (5) steer the shared cube hop in
//! `physics_step`; a tempting player or an item makes the hops aggressive
//! (a third of the delay). With a block every goal is gone and the cube
//! moves one Java tick at a time with `LivingEntity.travelInAir` or
//! `travelInFluid` under its archetype's friction, air drag and
//! bounciness (`Entity.restituteMovementAfterCollisions`), never stepping
//! up (`maxUpStep` 0).
use super::*;
use crate::entity::sulfur_cube::{
    self as cube, GRAVITY, HORIZONTAL_HIT_ANGLE_SCALE, TICK, VERTICAL_HIT_ANGLE_SCALE, VERTICAL_POSITION_ANGLE_SCALE,
    modified_friction,
};

/// `Vec2.rotate`.
fn rotate(v: (f32, f32), angle: f32) -> (f32, f32) {
    let (s, c) = angle.sin_cos();
    (v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

/// `Mth.clampedMap`.
fn clamped_map(v: f64, from_lo: f64, from_hi: f64, to_lo: f64, to_hi: f64) -> f64 {
    let t = if from_hi == from_lo { 0.0 } else { ((v - from_lo) / (from_hi - from_lo)).clamp(0.0, 1.0) };
    to_lo + t * (to_hi - to_lo)
}

impl Mob {
    /// A sulfur cube's steering: `None` while it holds a block (no goals),
    /// else a hop heading toward its lure or its random direction.
    pub(super) fn sulfur_cube_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        if self.sulfur_has_block() {
            self.ai = Ai::Idle;
            return Some((None, 0.0));
        }
        let ticks = self.goal_ticks(dt) as i32;
        let liquid = self.touching_water(world) || self.touching_lava(world);
        let s = self.sulfur_cube.as_mut()?;
        let heading = if let Some(lure) = s.lure {
            // The tempt goal and the item search look at their target and
            // hop toward it aggressively.
            self.ai = Ai::Chase;
            let to = lure - self.pos;
            if to.x.abs() + to.z.abs() < 1e-6 { s.heading } else { (to.z as f32).atan2(to.x as f32) }
        } else {
            // `CubeMobRandomDirectionGoal`: a new heading every 40-99 ticks
            // while on the ground, in liquid or levitating.
            self.ai = Ai::Wander;
            s.heading_wait -= ticks;
            if s.heading_wait <= 0 && (self.on_ground || liquid) {
                s.heading_wait = 40 + rng.next_int(60) as i32;
                s.heading = (rng.next_int(360) as f32).to_radians();
            }
            s.heading
        };
        self.move_yaw = heading;
        // `CubeMobFloatGoal`: jump on 80% of ticks in liquid.
        if liquid && rng.next_f32() < 0.8 {
            self.jump = true;
        }
        let dir = DVec3::new(f64::from(heading.cos()), 0.0, f64::from(heading.sin()));
        Some((Some(dir), 1.0))
    }

    /// The goals and size after the body slot changed
    /// (`SulfurCube.collectEquipmentChanges`): a block removes every goal
    /// and stops it; an empty body brings the goals back.
    pub(in crate::entity) fn sulfur_body_changed(&mut self) {
        self.ai = Ai::Idle;
        self.stop_navigation();
        if let Some(s) = &mut self.sulfur_cube {
            s.lure = None;
            s.heading_wait = 0;
        }
    }

    /// `SulfurCube.ageBoundaryReached`: grown up, it is an adult of size 2
    /// at full health.
    pub(in crate::entity) fn grow_up_sulfur_cube(&mut self) {
        self.baby = false;
        self.age = 0;
        self.set_sulfur_size(cube::ADULT_SIZE);
    }

    /// Ball physics for a cube holding a block, in whole Java ticks.
    pub(in crate::entity) fn sulfur_travel<W: BlockSource + ?Sized>(&mut self, dt: f64, world: &W) {
        let Some(p) = self.sulfur_physics() else { return };
        let mut carry = self.sulfur_cube.as_ref().map_or(0.0, |s| s.carry) + dt;
        while carry >= TICK - 1e-9 {
            carry -= TICK;
            self.sulfur_tick(world, &p);
        }
        if let Some(s) = &mut self.sulfur_cube {
            s.carry = carry;
        }
    }

    fn sulfur_tick<W: BlockSource + ?Sized>(&mut self, world: &W, p: &cube::Physics) {
        let mut dm = self.vel / crate::knockback::TICK_RATE;
        let ticks = self.sulfur_cube.as_mut().map_or(0, |s| {
            s.ticks = s.ticks.wrapping_add(1);
            s.ticks
        });
        let water = self.touching_water(world);
        let lava = !water && self.touching_lava(world);
        let air_drag = modified_friction(0.91, p.air_drag);
        if water || lava {
            // `travelInWater`/`travelInLava` with no input.
            let falling = dm.y <= 0.0;
            self.sulfur_move(world, &mut dm, p, air_drag);
            if water {
                dm *= 0.8;
                // `getFluidFallingAdjustedMovement`.
                let g = GRAVITY / 16.0;
                dm.y = if falling && (dm.y - 0.005).abs() >= 0.003 && (dm.y - g).abs() < 0.003 {
                    -0.003
                } else {
                    dm.y - g
                };
            } else {
                dm *= 0.5;
                dm.y -= GRAVITY / 4.0;
            }
            // `SulfurCube.travelInFluid`: a buoyant block bobs up.
            if p.buoyant {
                let wobble = 0.2 * (ticks as f32 * 0.4).sin();
                let height = self.shape().height;
                let immersion = self.fluid_depth(world, water) - height * 0.2 + f64::from(wobble);
                if immersion > 0.0 {
                    dm.y += immersion.min(1.0) * f64::from(0.04_f32);
                }
            }
        } else {
            let below = world.block(super::super::item::below_pos(self.pos));
            let friction =
                if self.on_ground { modified_friction(crate::player::friction(below), p.friction) } else { 1.0 };
            self.sulfur_move(world, &mut dm, p, air_drag);
            if let Some(level) = self.effects.get(crate::simulation::effects::Effect::Levitation).map(|a| a.amplifier) {
                dm.y += (0.05 * f64::from(level + 1) - dm.y) * 0.2;
            } else {
                dm.y -= GRAVITY;
            }
            let horizontal = f64::from(friction * air_drag);
            // `omnidirectionalAirMover`: vertical drag is the air drag too.
            dm = DVec3::new(dm.x * horizontal, dm.y * f64::from(air_drag), dm.z * horizontal);
        }
        self.vel = dm * crate::knockback::TICK_RATE;
    }

    /// How deep the box is in its fluid (`getFluidHeight`), at most its height.
    fn fluid_depth<W: BlockSource + ?Sized>(&self, world: &W, water: bool) -> f64 {
        let test: fn(Block) -> bool = if water { Block::holds_water } else { Block::is_lava };
        let base = self.pos.y.floor() as i32;
        let cell = |y: i32| IVec3::new(self.pos.x.floor() as i32, y, self.pos.z.floor() as i32);
        let mut top = None;
        for y in base..=base + self.shape().height.ceil() as i32 + 1 {
            if world.block(cell(y)).is_some_and(test) {
                top = Some(f64::from(y) + 1.0);
            } else if top.is_some() {
                break;
            }
        }
        top.map_or(0.0, |t| (t - self.pos.y).clamp(0.0, self.shape().height))
    }

    /// `Entity.move` without stepping, then
    /// `restituteMovementAfterCollisions` with the archetype's bounciness.
    fn sulfur_move<W: BlockSource + ?Sized>(&mut self, world: &W, dm: &mut DVec3, p: &cube::Physics, air_drag: f32) {
        let before = *dm;
        let start = self.pos;
        let mut v = before;
        let shape = self.shape();
        let hit = physics::move_box(world, &mut self.pos, &mut v, before, shape);
        let moved = self.pos - start;
        let x_col = (moved.x - before.x).abs() > 1e-7;
        let z_col = (moved.z - before.z).abs() > 1e-7;
        let y_col = (moved.y - before.y).abs() > 1e-7;
        let below_hit = y_col && before.y < 0.0;
        self.on_ground = below_hit || (self.on_ground && before.y == 0.0 && hit.on_ground);
        self.blocked = hit.horizontal;
        let mut restitution = p.bounciness;
        let mut out = before;
        if x_col {
            out.x = -before.x * restitution;
        }
        if z_col {
            out.z = -before.z * restitution;
        }
        if y_col {
            if below_hit {
                let block = world.block(super::super::item::below_pos(self.pos));
                restitution =
                    if -before.y <= GRAVITY { 0.0 } else { restitution.max(crate::player::bounciness(block)) };
            }
            let (compensation, drag) = if restitution > 0.0 {
                let portion = moved.y / before.y;
                (portion * GRAVITY, 1.0 + portion * (f64::from(air_drag) - 1.0))
            } else {
                (0.0, 1.0)
            };
            out.y = (compensation - before.y) * drag * restitution;
        }
        *dm = out;
    }

    /// `SulfurCube.knockback` for a hit from an entity: the hit angle turns
    /// and tips the archetype's knockback powers, scaled by the square root
    /// of the damage and by the knockback resistance. `eye` and `look` are
    /// the attacker's, `feet` its position, and `(xd, zd)` points from the
    /// cube to the damage source (`dealDefaultKnockback`).
    pub fn sulfur_knockback(&mut self, eye: DVec3, look: DVec3, feet: DVec3, damage: f32, xd: f64, zd: f64) {
        let Some(p) = self.sulfur_physics() else { return };
        let (h0, v0) = p.knockback;
        let height = self.shape().height;
        let center = self.pos + DVec3::Y * (height * 0.5);
        let look = look.normalize_or_zero();
        // `applyHorizontalHitAngleScale`.
        let to = (center - eye).normalize_or_zero();
        let diff = (look.x * to.z - look.z * to.x).atan2(look.x * to.x + look.z * to.z) as f32;
        let angle = rotate((xd as f32, zd as f32), diff * HORIZONTAL_HIT_ANGLE_SCALE);
        // `applyVerticalHitAnglePowerTransfer`.
        let half = 0.5 * height as f32;
        let top = (center + DVec3::Y * f64::from(half) - eye).normalize_or_zero();
        let bottom = (center - DVec3::Y * f64::from(half) - eye).normalize_or_zero();
        let factor = clamped_map(look.y, top.y, bottom.y, -1.0, 1.0) as f32;
        let mut ratio = (factor * VERTICAL_HIT_ANGLE_SCALE).abs();
        if factor < 0.0 {
            ratio = -ratio;
        }
        let (h1, v1) = (h0 * (1.0 - ratio), v0 * (1.0 + ratio));
        // `applyVerticalPositionAnglePowerRotation`.
        let d = self.pos - feet;
        let position_angle = (-d.y).atan2(DVec3::new(d.x, 0.0, d.z).length()) as f32;
        let mut power = rotate((h1, v1), -position_angle * VERTICAL_POSITION_ANGLE_SCALE);
        let hr = if h0 > 0.0 { power.0.abs() / h0 } else { 0.0 };
        let vr = if v0 > 0.0 { power.1.abs() / v0 } else { 0.0 };
        let most = hr.max(vr);
        if most > 1.0 {
            power = (power.0 / most, power.1 / most);
        }
        let multiplier = damage.sqrt();
        let resist = (1.0 - p.knockback_resistance) as f32;
        let horizontal = (power.0 * multiplier * resist * 0.4).clamp(-128.0, 128.0);
        let vertical = (power.1 * multiplier * resist).clamp(-128.0, 128.0);
        let push = DVec3::new(f64::from(angle.0), 0.0, f64::from(angle.1)).normalize_or_zero() * f64::from(horizontal);
        let dm = self.vel / crate::knockback::TICK_RATE;
        let dm = DVec3::new(dm.x - push.x, dm.y + f64::from(vertical) * 1.2, dm.z - push.z);
        self.vel = dm * crate::knockback::TICK_RATE;
    }
}
