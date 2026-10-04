//! First-person player: movement, collision and camera orientation.

use glam::{DVec3, IVec3, Vec3};

use crate::physics::{self, Shape};
use crate::world::World;
use crate::world::block::Block;

pub const EYE_HEIGHT: f64 = 1.62;
pub const HALF_WIDTH: f64 = 0.3;
pub const HEIGHT: f64 = 1.8;
const GRAVITY: f64 = 28.0;
const JUMP_VELOCITY: f64 = 8.6;
const WALK_SPEED: f64 = 4.4;
const SPRINT_SPEED: f64 = 6.0;
const FLY_SPEED: f64 = 11.0;
const FLY_SPRINT_SPEED: f64 = 30.0;
const SWIM_SPEED: f64 = 2.8;
const MAX_STEP: f64 = 1.0 / 120.0;
/// Ledges this tall are walked up without jumping (slabs, stairs).
pub const STEP_HEIGHT: f64 = 0.6;
/// Ladder speeds: climbing, and the fastest slide down.
const CLIMB_SPEED: f64 = 2.35;
/// Sneaking walks at 30% speed, and lowers the eyes this far.
const SNEAK_FACTOR: f64 = 0.3;
const SNEAK_EYE_DROP: f64 = 0.3;
const LADDER_SLIDE: f64 = 3.0;
pub const SHAPE: Shape = Shape::new(HALF_WIDTH, HEIGHT);

#[derive(Default, Clone, Copy)]
pub struct MoveInput {
    pub forward: f64,
    pub right: f64,
    pub jump: bool,
    pub descend: bool,
    pub sprint: bool,
}

pub struct Player {
    /// Feet position (bottom centre of the bounding box).
    pub pos: DVec3,
    pub vel: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub flying: bool,
    /// Creative mode: flight allowed.
    pub can_fly: bool,
    pub in_water: bool,
    /// Jumped off the ground during the last `update` (hunger).
    pub jumped: bool,
    /// Holding on to a ladder: falls are broken.
    pub climbing: bool,
    /// Walked into a wall on the last step (climbs ladders).
    pushing_wall: bool,
    /// Holding Shift on the ground: slow, quiet, and never walks off edges.
    pub sneaking: bool,
    /// How far the eyes have lowered for sneaking, 0..1 (eased).
    crouch: f64,
    /// Status effects on movement (see [`Player::apply_effects`]).
    modifiers: Modifiers,
}

/// What status effects do to movement.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Modifiers {
    speed: f64,
    jump_levels: u32,
    slow_falling: bool,
}

impl Default for Modifiers {
    fn default() -> Self {
        Self { speed: 1.0, jump_levels: 0, slow_falling: false }
    }
}

impl Player {
    pub fn new(pos: DVec3) -> Self {
        Self {
            pos,
            vel: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            flying: false,
            can_fly: false,
            in_water: false,
            jumped: false,
            climbing: false,
            pushing_wall: false,
            sneaking: false,
            crouch: 0.0,
            modifiers: Modifiers::default(),
        }
    }

    pub fn eye(&self) -> DVec3 {
        self.pos + DVec3::new(0.0, EYE_HEIGHT - SNEAK_EYE_DROP * self.crouch, 0.0)
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cy * cp, sp, sy * cp)
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch - dy).clamp(-1.5533, 1.5533); // ±89°
    }

    pub fn head_in_water(&self, world: &World) -> bool {
        let eye = self.eye();
        world.get_block(eye.floor().as_ivec3()).is_some_and(|b| {
            let above = world.get_block(eye.floor().as_ivec3() + IVec3::Y);
            // Respect the lowered surface of the top water block.
            let drop = if above.is_some_and(|a| a.is_water()) { 0.0 } else { b.fluid_drop() as f64 / 16.0 };
            b.is_water() && eye.y - eye.y.floor() < 1.0 - drop
        })
    }

    /// Whether any part of the player's box is in lava.
    pub fn in_lava(&self, world: &World) -> bool {
        physics::touches_block(world, self.pos, SHAPE, Block::is_lava)
    }

    pub fn in_fire(&self, world: &World) -> bool {
        physics::touches_block(world, self.pos, SHAPE, Block::is_fire)
    }

    pub fn head_in_lava(&self, world: &World) -> bool {
        world.get_block(self.eye().floor().as_ivec3()).is_some_and(|b| b.is_lava())
    }

    /// Sneaking: cancels horizontal movement (per axis) that would leave
    /// nothing within a step's height under the player, like Minecraft.
    fn hold_edges(&mut self, world: &World, delta: &mut DVec3) {
        let below = |d: DVec3| physics::overlaps_solid(world, self.pos + d - DVec3::Y * STEP_HEIGHT, SHAPE);
        for axis in [0, 2] {
            let mut d = DVec3::ZERO;
            d[axis] = delta[axis];
            if delta[axis] != 0.0 && !below(d) {
                delta[axis] = 0.0;
                self.vel[axis] = 0.0;
            }
        }
        // Diagonally off a corner, with each axis fine on its own.
        if !below(DVec3::new(delta.x, 0.0, delta.z)) {
            delta.x = 0.0;
            delta.z = 0.0;
        }
    }

    /// Whether the player's box overlaps a block cell.
    pub fn intersects_block(&self, b: IVec3) -> bool {
        let (min, max) = SHAPE.aabb(self.pos);
        let bmin = b.as_dvec3();
        let bmax = bmin + DVec3::ONE;
        min.cmplt(bmax).all() && max.cmpgt(bmin).all()
    }

    /// Takes on the movement effects of Speed, Slowness, Jump Boost and
    /// Slow Falling; call before each update.
    pub fn apply_effects(&mut self, effects: &crate::simulation::effects::Effects) {
        use crate::simulation::effects::Effect;
        self.modifiers = Modifiers {
            speed: effects.speed_factor(),
            jump_levels: effects.jump_boost(),
            slow_falling: effects.has(Effect::SlowFalling),
        };
    }

    pub fn update(&mut self, dt: f64, input: MoveInput, world: &World) {
        // Don't simulate until the ground under us has streamed in.
        if !world.is_loaded(self.pos.floor().as_ivec3()) {
            return;
        }
        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        self.jumped = false;
        for _ in 0..steps {
            self.step(h, input, world);
        }
    }

    fn step(&mut self, dt: f64, input: MoveInput, world: &World) {
        let feet = self.pos + DVec3::new(0.0, 0.3, 0.0);
        // Lava swims like (slow) water.
        self.in_water = world.get_block(feet.floor().as_ivec3()).is_some_and(|b| b.is_fluid());
        self.sneaking = input.descend && !self.flying && !self.in_water;
        let target = if self.sneaking { 1.0 } else { 0.0 };
        self.crouch += (target - self.crouch) * (dt * 14.0).min(1.0);

        let yaw = self.yaw as f64;
        let fwd = DVec3::new(yaw.cos(), 0.0, yaw.sin());
        let right = DVec3::new(-yaw.sin(), 0.0, yaw.cos());
        let mut wish = fwd * input.forward + right * input.right;
        if wish.length_squared() > 1.0 {
            wish = wish.normalize();
        }

        if self.flying {
            let speed = if input.sprint { FLY_SPRINT_SPEED } else { FLY_SPEED };
            let vertical = (input.jump as i32 - input.descend as i32) as f64;
            let target = wish * speed + DVec3::Y * vertical * speed;
            self.vel = self.vel.lerp(target, (dt * 12.0).min(1.0));
        } else if self.in_water {
            let target = wish * SWIM_SPEED * self.modifiers.speed;
            let k = (dt * 6.0).min(1.0);
            self.vel.x += (target.x - self.vel.x) * k;
            self.vel.z += (target.z - self.vel.z) * k;
            self.vel.y -= GRAVITY * 0.25 * dt;
            if input.jump {
                self.vel.y = (self.vel.y + 30.0 * dt).min(3.8);
            }
            self.vel.y = self.vel.y.max(-4.0);
        } else {
            let speed = match (self.sneaking, input.sprint) {
                (true, _) => WALK_SPEED * SNEAK_FACTOR,
                (false, true) => SPRINT_SPEED,
                (false, false) => WALK_SPEED,
            };
            // Snappy on the ground, slippery on ice, limited air control;
            // soul sand drags at your feet.
            let below = (self.pos - DVec3::Y * 0.05).floor().as_ivec3();
            let ground = world.get_block(below).filter(|_| self.on_ground);
            let on_ice = ground == Some(Block::ICE);
            let target = wish * speed * self.modifiers.speed * if ground == Some(Block::SOUL_SAND) { 0.4 } else { 1.0 };
            let grip = match (self.on_ground, on_ice) {
                (true, true) => 1.6,
                (true, false) => 18.0,
                _ => 3.5,
            };
            let k = (dt * grip).min(1.0);
            self.vel.x += (target.x - self.vel.x) * k;
            self.vel.z += (target.z - self.vel.z) * k;
            // Java's slow falling: an eighth of the gravity while falling.
            let gravity = if self.modifiers.slow_falling && self.vel.y <= 0.0 { GRAVITY / 8.0 } else { GRAVITY };
            self.vel.y = (self.vel.y - gravity * dt).max(-78.0);
            if input.jump && self.on_ground {
                // Jump Boost: +0.1 blocks per tick of take-off speed per level.
                self.vel.y = JUMP_VELOCITY + 2.0 * self.modifiers.jump_levels as f64;
                self.jumped = true;
            }
        }

        // Ladders: walking into one (or jumping) climbs, sneaking holds on,
        // and a fall slows to a slide.
        self.climbing = !self.flying && world.get_block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_ladder());
        if self.climbing {
            self.vel.y = self.vel.y.max(-LADDER_SLIDE);
            if input.descend {
                self.vel.y = self.vel.y.max(0.0);
            }
            if input.jump || (self.pushing_wall && wish != DVec3::ZERO) {
                self.vel.y = CLIMB_SPEED;
            }
        }

        let mut delta = self.vel * dt;
        if self.sneaking && self.on_ground {
            self.hold_edges(world, &mut delta);
        }
        let hit = if self.on_ground && !self.flying {
            physics::move_box_stepping(world, &mut self.pos, &mut self.vel, delta, SHAPE, STEP_HEIGHT)
        } else {
            physics::move_box(world, &mut self.pos, &mut self.vel, delta, SHAPE)
        };
        self.on_ground = hit.on_ground;
        self.pushing_wall = hit.horizontal;
        if self.flying && self.on_ground {
            self.flying = false;
        }
    }
}
