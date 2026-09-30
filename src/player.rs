//! First-person player: movement, collision and camera orientation.

use glam::{DVec3, IVec3, Vec3};

use crate::world::World;

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
}

impl Player {
    pub fn new(pos: DVec3) -> Self {
        Self { pos, vel: DVec3::ZERO, yaw: 0.0, pitch: 0.0, on_ground: false, flying: false, can_fly: false, in_water: false }
    }

    pub fn eye(&self) -> DVec3 {
        self.pos + DVec3::new(0.0, EYE_HEIGHT, 0.0)
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
            let drop = if above.is_some_and(|a| a.is_water()) { 0.0 } else { b.water_drop() as f64 / 16.0 };
            b.is_water() && eye.y - eye.y.floor() < 1.0 - drop
        })
    }

    /// Whether the player's box overlaps a block cell.
    pub fn intersects_block(&self, b: IVec3) -> bool {
        let (min, max) = self.aabb(self.pos);
        let bmin = b.as_dvec3();
        let bmax = bmin + DVec3::ONE;
        min.cmplt(bmax).all() && max.cmpgt(bmin).all()
    }

    fn aabb(&self, pos: DVec3) -> (DVec3, DVec3) {
        (
            pos - DVec3::new(HALF_WIDTH, 0.0, HALF_WIDTH),
            pos + DVec3::new(HALF_WIDTH, HEIGHT, HALF_WIDTH),
        )
    }

    pub fn update(&mut self, dt: f64, input: MoveInput, world: &World) {
        // Don't simulate until the ground under us has streamed in.
        if !world.is_loaded(self.pos.floor().as_ivec3()) {
            return;
        }
        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        for _ in 0..steps {
            self.step(h, input, world);
        }
    }

    fn step(&mut self, dt: f64, input: MoveInput, world: &World) {
        let feet = self.pos + DVec3::new(0.0, 0.3, 0.0);
        self.in_water = world.get_block(feet.floor().as_ivec3()).is_some_and(|b| b.is_water());

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
            let target = wish * SWIM_SPEED;
            let k = (dt * 6.0).min(1.0);
            self.vel.x += (target.x - self.vel.x) * k;
            self.vel.z += (target.z - self.vel.z) * k;
            self.vel.y -= GRAVITY * 0.25 * dt;
            if input.jump {
                self.vel.y = (self.vel.y + 30.0 * dt).min(3.8);
            }
            self.vel.y = self.vel.y.max(-4.0);
        } else {
            let speed = if input.sprint { SPRINT_SPEED } else { WALK_SPEED };
            let target = wish * speed;
            // Snappy on the ground, limited air control.
            let k = (dt * if self.on_ground { 18.0 } else { 3.5 }).min(1.0);
            self.vel.x += (target.x - self.vel.x) * k;
            self.vel.z += (target.z - self.vel.z) * k;
            self.vel.y = (self.vel.y - GRAVITY * dt).max(-78.0);
            if input.jump && self.on_ground {
                self.vel.y = JUMP_VELOCITY;
            }
        }

        let delta = self.vel * dt;
        self.on_ground = false;
        // Resolve one axis at a time; Y first so landing takes priority.
        for axis in [1usize, 0, 2] {
            if delta[axis] == 0.0 {
                continue;
            }
            let mut next = self.pos;
            next[axis] += delta[axis];
            if let Some(resolved) = self.collide(world, next, axis, delta[axis]) {
                if axis == 1 && delta[axis] < 0.0 {
                    self.on_ground = true;
                }
                self.pos = resolved;
                self.vel[axis] = 0.0;
            } else {
                self.pos = next;
            }
        }
        if self.flying && self.on_ground {
            self.flying = false;
        }
    }

    /// If the box at `pos` overlaps solid blocks, returns `pos` pushed back
    /// against them along `axis`.
    fn collide(&self, world: &World, pos: DVec3, axis: usize, dir: f64) -> Option<DVec3> {
        const EPS: f64 = 1e-5;
        let (min, max) = self.aabb(pos);
        let lo = (min + DVec3::splat(EPS)).floor().as_ivec3();
        let hi = (max - DVec3::splat(EPS)).floor().as_ivec3();
        let mut hit: Option<f64> = None;
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let b = IVec3::new(x, y, z);
                    // Unloaded chunks act solid so we never fall out of the world.
                    let solid = world.get_block(b).is_none_or(|b| b.is_solid());
                    if !solid {
                        continue;
                    }
                    let edge = if dir > 0.0 { b[axis] as f64 } else { b[axis] as f64 + 1.0 };
                    hit = Some(match hit {
                        Some(h) if dir > 0.0 => h.min(edge),
                        Some(h) => h.max(edge),
                        None => edge,
                    });
                }
            }
        }
        let edge = hit?;
        let mut out = pos;
        out[axis] = match axis {
            1 if dir > 0.0 => edge - HEIGHT - EPS,
            1 => edge + EPS,
            _ if dir > 0.0 => edge - HALF_WIDTH - EPS,
            _ => edge + HALF_WIDTH + EPS,
        };
        Some(out)
    }
}
