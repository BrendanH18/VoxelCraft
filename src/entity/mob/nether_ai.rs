//! Per-tick behaviour of the newer Nether mobs: admiring gold, fighting with
//! a golden sword or a crossbow, running away and fetching items. Goals come
//! from the periodic look around in `entity::nether`.

use glam::DVec3;

use super::{ATTACK_RANGE, Ai, Ctx, EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng, line_of_sight, wrap};
use crate::entity::nether::{Foe, Goal, Weapon, ZOMBIFY_TIME};
use crate::world::terrain::Dimension;

/// Crossbow piglins shoot from Java's 8 blocks and back away inside 5.
const CROSSBOW_RANGE: f64 = 8.0;
const CROSSBOW_NEAR: f64 = 5.0;
/// Seconds to load (Java's 25 ticks), then a pause before the shot (Java's
/// 20-40 ticks after charging).
const CROSSBOW_CHARGE: f32 = 1.25;
const CROSSBOW_AIM: (f32, f32) = (1.0, 2.0);
/// Java's 20-tick melee cooldown.
const MELEE_COOLDOWN: f32 = 1.0;
/// Attack bonus of a golden sword (on top of a piglin's 5) and a golden axe
/// (on top of a brute's 7).
const GOLDEN_SWORD_BONUS: f32 = 3.0;
const GOLDEN_AXE_BONUS: f32 = 6.0;
/// Hoglin and zoglin attack cooldowns (Java's 40 ticks, 15 for babies).
const TUSK_COOLDOWN: f32 = 2.0;
const BABY_TUSK_COOLDOWN: f32 = 0.75;
/// Baby piglins are 20% faster (Java's speed modifier).
const BABY_SPEED: f64 = 1.2;

impl Mob {
    /// Movement for a mob with a [`crate::entity::nether::NetherMob`], or
    /// `None` to idle and wander as usual.
    pub(super) fn nether_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let speed = self.kind.chase_speed() * if self.baby && self.kind == MobKind::Piglin { BABY_SPEED } else { 1.0 };
        if self.kind == MobKind::Strider {
            let feet = self.pos.floor().as_ivec3();
            let warm = |p| world.block(p).is_some_and(|b: crate::world::block::Block| b.is_lava());
            let cold = !warm(feet) && !warm(feet - glam::IVec3::Y);
            if let Some(n) = self.nether.as_mut() {
                n.cold = cold;
            }
        }
        let n = self.nether.as_mut()?;
        n.admire_disabled = (n.admire_disabled - dt).max(0.0);
        n.hunt_cooldown = (n.hunt_cooldown - dt).max(0.0);
        n.ate = (n.ate - dt).max(0.0);
        n.flee_left = (n.flee_left - dt).max(0.0);
        n.foe_left -= dt;
        if n.foe_left <= 0.0 {
            n.foe = None;
        }
        // Java's `isConverting`: outside a piglin-safe dimension.
        if ctx.dimension != Dimension::Nether && !n.immune {
            n.zombify += dt;
            if n.zombify >= ZOMBIFY_TIME {
                n.convert = true;
            }
        } else {
            n.zombify = 0.0;
        }
        if n.offhand.is_some() {
            // Standing still, gazing at the gold in its hand.
            n.admire_left -= dt;
            if n.admire_left <= 0.0 {
                n.barter = true;
            }
            self.ai = Ai::Idle;
            self.head_target = (0.0, -0.6);
            return Some((None, 0.0));
        }
        match n.goal {
            Goal::None => {
                if self.ai == Ai::Chase {
                    self.ai = Ai::Idle;
                    self.ai_timer = 1.0;
                }
                None
            }
            Goal::Fetch(at) | Goal::Walk(at) => {
                self.ai = Ai::Wander;
                let flat = (at - self.pos) * DVec3::new(1.0, 0.0, 1.0);
                if flat.length_squared() < 0.04 {
                    return Some((None, 0.0));
                }
                self.look_at(at, 0.5);
                let dir = self.steer(flat.normalize(), dt, world, rng);
                Some((Some(dir), speed))
            }
            Goal::Flee(from) => {
                self.ai = Ai::Panic;
                self.head_target = (0.0, 0.0);
                let away = ((self.pos - from) * DVec3::new(1.0, 0.0, 1.0)).normalize_or(DVec3::X);
                let dir = self.steer(away, dt, world, rng);
                Some((Some(dir), speed))
            }
            Goal::Attack { foe, pos } => {
                let at = match foe {
                    Foe::Player(id) => ctx.players.iter().find(|t| t.id == id).map_or(pos, |t| t.pos),
                    Foe::Mob(_) => pos,
                };
                Some(self.nether_attack(foe, at, speed, dt, world, rng, events))
            }
        }
    }

    /// Fights `foe`, whose feet are at `at`: crossbow piglins keep between 5
    /// and 8 blocks and shoot players they can see; everyone else closes in
    /// and strikes once a second.
    #[allow(clippy::too_many_arguments)]
    fn nether_attack<W: MobWorld + ?Sized>(
        &mut self,
        foe: Foe,
        at: DVec3,
        speed: f64,
        dt: f32,
        world: &W,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        self.ai = Ai::Chase;
        let to = at - self.pos;
        let flat = DVec3::new(to.x, 0.0, to.z);
        let hdist = flat.length();
        let dir = if hdist > 1e-6 { flat / hdist } else { DVec3::X };
        self.look_at(at, 1.4);
        let weapon = self.nether.as_ref().map_or(Weapon::None, |n| n.weapon);
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.9);
        let aim = at + DVec3::Y * 1.2;
        if weapon == Weapon::Crossbow && matches!(foe, Foe::Player(_)) {
            let sees = hdist <= CROSSBOW_RANGE && line_of_sight(world, eye, aim);
            let n = self.nether.as_mut().unwrap();
            if sees {
                n.charge += dt;
                if n.charge >= CROSSBOW_CHARGE + rng.range(CROSSBOW_AIM.0, CROSSBOW_AIM.1) {
                    n.charge = 0.0;
                    self.attack_anim = 0.35;
                    events.push(EntityEvent::Shoot { from: eye + dir * 0.5, target: aim });
                    events.push(EntityEvent::Sound { sound: MobSound::Bow, pos: eye });
                }
            } else {
                n.charge = n.charge.min(CROSSBOW_CHARGE);
            }
            return if !sees {
                (Some(self.steer(dir, dt, world, rng)), speed)
            } else if hdist < CROSSBOW_NEAR {
                (Some(-dir), speed * 0.75)
            } else {
                (None, 0.0)
            };
        }
        let tusks = matches!(self.kind, MobKind::Hoglin | MobKind::Zoglin);
        let reach = ATTACK_RANGE + if tusks { self.shape().half_width } else { 0.0 };
        if hdist <= reach && to.y.abs() < 1.6 && self.attack_cooldown <= 0.0 {
            self.attack_anim = 0.35;
            let (base, cause) = self.kind.melee();
            let mut knockback = dir * 6.0 + DVec3::Y * 5.0;
            let damage = if tusks {
                self.attack_cooldown = if self.baby { BABY_TUSK_COOLDOWN } else { TUSK_COOLDOWN };
                if self.baby {
                    0.5
                } else {
                    // Java's `HoglinBase.throwTarget`: a shove of 0.2-0.7
                    // blocks a tick within 10 degrees of straight away,
                    // and up to 0.5 blocks a tick upward.
                    let turn = (rng.next_int(21) as f32 - 10.0).to_radians() as f64;
                    let (s, c) = turn.sin_cos();
                    let away = DVec3::new(dir.x * c - dir.z * s, 0.0, dir.x * s + dir.z * c);
                    knockback += away * (rng.range(0.2, 0.7) as f64 * 20.0) + DVec3::Y * (rng.next_f32() as f64 * 10.0);
                    // Java: half the attack damage plus a random part of it.
                    base / 2.0 + rng.next_int(base as u32) as f32
                }
            } else {
                self.attack_cooldown = MELEE_COOLDOWN;
                base + match weapon {
                    Weapon::GoldenSword => GOLDEN_SWORD_BONUS,
                    Weapon::GoldenAxe => GOLDEN_AXE_BONUS,
                    _ => 0.0,
                }
            };
            events.push(match foe {
                Foe::Player(player) => EntityEvent::PlayerHit { player, damage, knockback: knockback.as_vec3(), cause },
                Foe::Mob(target) => EntityEvent::MobHit { target, attacker: self.uid, damage, knockback },
            });
        }
        if hdist <= 0.8 + if tusks { self.shape().half_width } else { 0.0 } {
            return (None, 0.0);
        }
        (Some(self.steer(dir, dt, world, rng)), speed)
    }

    /// Turns the head toward a point, up to `max` radians off the body.
    fn look_at(&mut self, at: DVec3, max: f32) {
        let to = at - self.pos;
        let hdist = to.x.hypot(to.z);
        let eye = to.y + 1.62 - self.shape().height * 0.9;
        let face_yaw = (to.z as f32).atan2(to.x as f32);
        self.head_target = (wrap(face_yaw - self.yaw).clamp(-max, max), (eye as f32).atan2(hdist as f32));
    }

    /// Heads along `dir`, side-stepping for a moment when blocked by a wall
    /// it can't jump (the same detour the other chasers use).
    fn steer<W: MobWorld + ?Sized>(&mut self, dir: DVec3, dt: f32, world: &W, rng: &mut Rng) -> DVec3 {
        self.detour -= dt;
        if self.detour <= 0.0 && self.blocked && self.on_ground && !self.can_step_up(world, dir) {
            self.detour = rng.range(0.6, 1.2);
            self.detour_side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        }
        if self.detour > 0.0 {
            (DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side + dir * 0.35).normalize()
        } else {
            dir
        }
    }

    /// Share of a hit's knockback that lands (Java's knockback resistance:
    /// 0.6 for hoglins and zoglins).
    pub(super) fn knockback_taken(&self) -> f64 {
        match self.kind {
            MobKind::Hoglin | MobKind::Zoglin => 0.4,
            _ => 1.0,
        }
    }

    /// Movement multiplier: a strider out of lava is 34% slower (Java's
    /// suffocating modifier).
    pub(super) fn speed_factor(&self) -> f64 {
        if self.nether.as_ref().is_some_and(|n| n.cold) { 0.66 } else { 1.0 }
    }

    /// Whether a step along `dir` is safe for a strider, which treats lava as
    /// floor and, once on lava, won't wander off it (Java scores lava 10
    /// and dry land unwalkable from lava). `None` for everyone else.
    pub(super) fn lava_step<W: MobWorld + ?Sized>(&self, world: &W, dir: DVec3) -> Option<bool> {
        if self.kind != MobKind::Strider {
            return None;
        }
        let ahead = self.pos + dir * (self.shape().half_width + 0.35);
        let (x, z) = (ahead.x.floor() as i32, ahead.z.floor() as i32);
        let y = (self.pos.y + 0.01).floor() as i32;
        let lava_ahead = (0..=4).any(|dy| world.block(glam::IVec3::new(x, y - dy, z)).is_some_and(|b| b.is_lava()));
        let on_lava = self.nether.as_ref().is_some_and(|n| !n.cold);
        Some(if on_lava { lava_ahead } else { lava_ahead || !super::is_cliff(world, self.pos, dir, self.shape()) })
    }

    /// Java's `floatStrider`: on a lava surface a strider stands as if on
    /// ground; sunk into lava it rises. Returns whether it handled this
    /// physics step.
    pub(super) fn walk_on_lava<W: crate::physics::BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        wish: Option<DVec3>,
        speed: f64,
    ) -> bool {
        let lava = |p: DVec3| world.block(p.floor().as_ivec3()).is_some_and(|b| b.is_lava());
        let under = (self.pos - DVec3::Y * 0.05).floor();
        let sunk = lava(self.pos + DVec3::Y * 0.05);
        let standing = !sunk && lava(self.pos - DVec3::Y * 0.05) && self.pos.y - (under.y + 1.0) < 0.1;
        if !self.alive() || !sunk && !standing || self.vel.y > 0.5 && !sunk {
            return false;
        }
        self.in_water = false;
        let target = wish.map_or(DVec3::ZERO, |d| d * speed);
        let k = (dt * 10.0).min(1.0);
        self.vel.x += (target.x - self.vel.x) * k;
        self.vel.z += (target.z - self.vel.z) * k;
        if sunk {
            self.vel.y = 2.0;
        } else {
            self.vel.y = 0.0;
            self.pos.y = under.y + 1.0;
        }
        let (delta, shape) = (self.vel * dt, self.shape());
        let hit = crate::physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape);
        self.on_ground = standing || hit.on_ground;
        self.blocked = hit.horizontal;
        // Climb out onto a shore one block up.
        if self.blocked && wish.is_some() && self.can_step_up(world, wish.unwrap_or(DVec3::X)) {
            self.vel.y = super::JUMP_VELOCITY;
        }
        true
    }
}
