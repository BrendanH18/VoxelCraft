//! Arrows: ballistic projectiles that stick in blocks and hurt the player.

use glam::{DVec3, Vec3};

use super::{Ctx, EntityEvent, Rng};
use crate::physics::BlockSource;
use crate::player::{HALF_WIDTH, HEIGHT};

const SPEED: f64 = 22.0;
const GRAVITY: f64 = 20.0;
/// Seconds a stuck arrow stays before vanishing, and a flying arrow lives.
const STUCK_TIME: f32 = 8.0;
const MAX_FLIGHT: f32 = 10.0;
/// Longest move per collision check (blocks), so arrows can't skip a block.
const STEP: f64 = 0.2;

pub struct Arrow {
    pub pos: DVec3,
    vel: DVec3,
    /// Direction it points (kept once stuck).
    pub dir: Vec3,
    age: f32,
    stuck: bool,
}

impl Arrow {
    /// An arrow from `from` aimed to arc onto `target`, with a little spread.
    pub fn aimed(from: DVec3, target: DVec3, rng: &mut Rng) -> Self {
        let d = target - from;
        // Lead the drop: over flight time t the arrow sinks g t² / 2.
        let t = d.length() / SPEED;
        let aim = d + DVec3::Y * (0.5 * GRAVITY * t * t);
        let spread = DVec3::new(rng.range(-1.0, 1.0) as f64, rng.range(-1.0, 1.0) as f64, rng.range(-1.0, 1.0) as f64);
        let vel = (aim.normalize_or(DVec3::X) + spread * 0.03) * SPEED;
        Self { pos: from, vel, dir: vel.normalize().as_vec3(), age: 0.0, stuck: false }
    }

    /// Moves the arrow; returns `false` once it should be removed.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        if self.stuck {
            return self.age < STUCK_TIME;
        }
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            self.pos += delta / steps as f64;
            if world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.stuck = true;
                self.age = 0.0;
                return true;
            }
            let p = self.pos - ctx.player_pos;
            let in_player = p.x.abs() < HALF_WIDTH && p.z.abs() < HALF_WIDTH && (0.0..HEIGHT).contains(&p.y);
            if ctx.player_targetable && in_player {
                let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 3.0 + DVec3::Y * 3.0;
                let damage = (self.vel.length() / SPEED * 3.0).ceil() as f32;
                events.push(EntityEvent::PlayerHit {
                    damage,
                    knockback: push.as_vec3(),
                    cause: "was shot by a skeleton",
                });
                return false;
            }
        }
        self.dir = self.vel.normalize_or(DVec3::X).as_vec3();
        self.age < MAX_FLIGHT && self.pos.y > -64.0
    }

    pub fn is_stuck(&self) -> bool {
        self.stuck
    }
}
