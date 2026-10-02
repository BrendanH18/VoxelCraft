//! Arrows: ballistic projectiles that stick in blocks. Skeleton arrows hurt
//! the player; the player's own arrows hurt mobs and can be picked back up.

use glam::{DVec3, Vec3};

use super::{Ctx, EntityEvent, Mob, Rng};
use crate::physics::BlockSource;
use crate::player::{HALF_WIDTH, HEIGHT};

const SPEED: f64 = 22.0;
/// Speed of an arrow from a fully drawn bow (Minecraft's 3 blocks a tick).
pub const BOW_SPEED: f64 = 55.0;
/// Damage of a fully drawn bow's arrow before the critical bonus.
const BOW_DAMAGE: f64 = 6.0;
const GRAVITY: f64 = 20.0;
/// Seconds a stuck arrow stays before vanishing, and a flying arrow lives.
const STUCK_TIME: f32 = 8.0;
const PLAYER_STUCK_TIME: f32 = 60.0;
const MAX_FLIGHT: f32 = 10.0;
/// Longest move per collision check (blocks), so arrows can't skip a block.
const STEP: f64 = 0.2;

pub struct Arrow {
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    vel: DVec3,
    /// Direction it points (kept once stuck).
    pub dir: Vec3,
    age: f32,
    stuck: bool,
    /// Shot by the player: hits mobs instead of the player.
    pub from_player: bool,
    /// The player can collect it once stuck (not arrows shot in creative).
    pub pickup: bool,
    /// A fully drawn shot: deals a random bonus on hit.
    pub critical: bool,
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
        Self {
            pos: from,
            previous_pos: from,
            vel,
            dir: vel.normalize().as_vec3(),
            age: 0.0,
            stuck: false,
            from_player: false,
            pickup: false,
            critical: false,
        }
    }

    /// An arrow loosed by the player from `eye` along `dir` with bow
    /// `power` 0..1.
    pub fn shot(eye: DVec3, dir: DVec3, power: f32, pickup: bool) -> Self {
        let dir = dir.normalize_or(DVec3::X);
        let vel = dir * BOW_SPEED * power as f64;
        Self {
            pos: eye + dir * 0.3,
            previous_pos: eye + dir * 0.3,
            vel,
            dir: dir.as_vec3(),
            age: 0.0,
            stuck: false,
            from_player: true,
            pickup,
            critical: power >= 1.0,
        }
    }

    /// Moves the arrow; returns `false` once it should be removed.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        mobs: &mut [Mob],
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        if self.stuck {
            // Arrows fall out when the block they're in is broken.
            if !world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.stuck = false;
                self.vel = DVec3::ZERO;
                return true;
            }
            return self.age < if self.from_player { PLAYER_STUCK_TIME } else { STUCK_TIME };
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
            if self.from_player {
                if let Some(mob) = mobs.iter_mut().find(|m| {
                    let (min, max) = m.aabb();
                    m.alive() && self.pos.cmpge(min).all() && self.pos.cmple(max).all()
                }) {
                    let speed = self.vel.length();
                    let mut damage = (speed / BOW_SPEED * BOW_DAMAGE).ceil() as f32;
                    if self.critical {
                        damage += (rng.next_f32() * (damage / 2.0 + 1.0)).floor();
                    }
                    let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 4.0 + DVec3::Y * 4.0;
                    let killed = mob.damage(damage, Some(push), rng);
                    events.push(EntityEvent::MobShot { kind: mob.kind, pos: mob.pos, killed });
                    return false;
                }
                continue;
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
        if self.vel.length_squared() > 1e-6 {
            self.dir = self.vel.normalize().as_vec3();
        }
        self.age < MAX_FLIGHT && self.pos.y > -64.0
    }

    pub fn is_stuck(&self) -> bool {
        self.stuck
    }
}
