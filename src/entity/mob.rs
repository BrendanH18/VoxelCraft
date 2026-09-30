//! A single mob: AI, movement physics and combat state.

use std::f32::consts::{PI, TAU};

use glam::{DVec3, IVec3};

use crate::physics::{self, BlockSource, Shape};

use super::{Ctx, EntityEvent, MobWorld, Rng};

const GRAVITY: f64 = 28.0;
/// Clears a 1-block ledge with a little margin (peak ~1.26 blocks).
const JUMP_VELOCITY: f64 = 8.4;
const MAX_STEP: f64 = 1.0 / 60.0;
/// Seconds of red flash (and knockback immunity) after taking damage.
pub const HURT_TIME: f32 = 0.5;
/// Length of the death animation before the mob is removed.
pub const DEATH_TIME: f32 = 0.9;
/// Zombies notice players within this many blocks.
const CHASE_RANGE: f64 = 24.0;
const ATTACK_RANGE: f64 = 1.2;
const ATTACK_COOLDOWN: f32 = 1.0;
const ZOMBIE_DAMAGE: f32 = 3.0;
/// Zombies burn in sunlight above this daylight level.
const BURN_DAYLIGHT: f32 = 0.45;
/// Falls deeper than this are avoided (blocks).
const MAX_SAFE_DROP: i32 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MobKind {
    Pig,
    Zombie,
}

impl MobKind {
    pub const ALL: [MobKind; 2] = [MobKind::Pig, MobKind::Zombie];

    pub fn name(self) -> &'static str {
        match self {
            MobKind::Pig => "pig",
            MobKind::Zombie => "zombie",
        }
    }

    pub fn from_name(name: &str) -> Option<MobKind> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub fn shape(self) -> Shape {
        match self {
            MobKind::Pig => Shape::new(0.45, 0.9),
            MobKind::Zombie => Shape::new(0.3, 1.95),
        }
    }

    pub fn max_health(self) -> f32 {
        match self {
            MobKind::Pig => 10.0,
            MobKind::Zombie => 20.0,
        }
    }

    fn wander_speed(self) -> f64 {
        match self {
            MobKind::Pig => 1.3,
            MobKind::Zombie => 1.1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Ai {
    /// Standing still, glancing around.
    Idle,
    /// Walking in `move_yaw`.
    Wander,
    /// Running from an attacker (pigs), changing direction every so often.
    Panic,
    /// Walking straight at the player (zombies).
    Chase,
}

pub struct Mob {
    pub kind: MobKind,
    /// Feet position (bottom centre of the box).
    pub pos: DVec3,
    pub vel: DVec3,
    /// Body facing, same convention as the player: forward = (cos, 0, sin).
    pub yaw: f32,
    /// Head yaw relative to the body, and pitch (positive looks up).
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub health: f32,
    /// Seconds of hurt flash left.
    pub hurt: f32,
    /// Seconds since death, while the death animation plays.
    pub dying: Option<f32>,
    pub on_ground: bool,
    pub in_water: bool,
    pub burning: bool,
    /// Walk cycle phase (radians) and amplitude (0..1).
    pub limb_phase: f32,
    pub limb_amp: f32,
    /// Seconds left of the arm swing after an attack.
    pub attack_anim: f32,
    /// Sky light estimate at the mob, 0..1 (refreshed a few times a second).
    pub sky_light: f32,
    pub(super) ai: Ai,
    pub(super) ai_timer: f32,
    pub(super) move_yaw: f32,
    head_target: (f32, f32),
    head_timer: f32,
    attack_cooldown: f32,
    burn_timer: f32,
    light_timer: f32,
    /// Blocked horizontally on the last physics step.
    blocked: bool,
    /// Seconds left sidestepping around an obstacle while chasing, and
    /// which side (+1 / -1).
    detour: f32,
    detour_side: f64,
}

impl Mob {
    pub fn new(kind: MobKind, pos: DVec3, yaw: f32) -> Self {
        Self {
            kind,
            pos,
            vel: DVec3::ZERO,
            yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            health: kind.max_health(),
            hurt: 0.0,
            dying: None,
            on_ground: false,
            in_water: false,
            burning: false,
            limb_phase: 0.0,
            limb_amp: 0.0,
            attack_anim: 0.0,
            sky_light: 1.0,
            ai: Ai::Idle,
            ai_timer: 1.0,
            move_yaw: yaw,
            head_target: (0.0, 0.0),
            head_timer: 0.0,
            attack_cooldown: 0.0,
            burn_timer: 0.0,
            light_timer: 0.0,
            blocked: false,
            detour: 0.0,
            detour_side: 1.0,
        }
    }

    pub fn shape(&self) -> Shape {
        self.kind.shape()
    }

    pub fn aabb(&self) -> (DVec3, DVec3) {
        self.shape().aabb(self.pos)
    }

    pub fn alive(&self) -> bool {
        self.dying.is_none()
    }

    /// Takes a hit. `knockback` replaces the horizontal velocity (its y is
    /// the upward pop). Returns `true` if this killed the mob.
    pub fn damage(&mut self, amount: f32, knockback: Option<DVec3>, rng: &mut Rng) -> bool {
        if !self.alive() {
            return false;
        }
        self.health -= amount;
        self.hurt = HURT_TIME;
        if let Some(kb) = knockback {
            self.vel.x = kb.x;
            self.vel.z = kb.z;
            self.vel.y = self.vel.y.max(kb.y);
        }
        if self.kind == MobKind::Pig {
            self.ai = Ai::Panic;
            self.ai_timer = rng.range(3.0, 5.0);
            self.move_yaw = rng.range(0.0, TAU);
        }
        if self.health <= 0.0 {
            self.dying = Some(0.0);
            return true;
        }
        false
    }

    /// Advances the mob by `dt` seconds.
    pub fn update<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let dtf = dt as f32;
        self.hurt = (self.hurt - dtf).max(0.0);
        self.attack_cooldown -= dtf;
        self.attack_anim = (self.attack_anim - dtf).max(0.0);

        self.light_timer -= dtf;
        if self.light_timer <= 0.0 {
            self.light_timer = 0.25;
            self.sky_light = sky_light(world, self.pos + DVec3::new(0.0, self.shape().height * 0.6, 0.0));
        }

        let (wish, speed) = if let Some(t) = &mut self.dying {
            *t += dtf;
            (None, 0.0)
        } else {
            self.think(dtf, world, ctx, rng, events)
        };

        // Don't walk off tall drops; wanderers pick another direction.
        let wish = wish.filter(|&dir| {
            let safe = !self.on_ground || self.in_water || !is_cliff(world, self.pos, dir, self.shape());
            if !safe && matches!(self.ai, Ai::Wander | Ai::Panic) {
                self.ai_timer = self.ai_timer.min(0.3);
                self.move_yaw = rng.range(0.0, TAU);
            }
            safe
        });

        // Turn the body toward the direction of travel.
        if let Some(dir) = wish {
            let target = (dir.z as f32).atan2(dir.x as f32);
            self.yaw = turn_toward(self.yaw, target, 8.0 * dtf);
        }

        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        for _ in 0..steps {
            self.physics_step(h, world, wish, speed);
        }

        // Walk cycle follows ground speed.
        let hspeed = self.vel.x.hypot(self.vel.z) as f32;
        self.limb_phase = (self.limb_phase + hspeed * dtf * 3.2) % (TAU * 64.0);
        let amp = (hspeed / 2.0).min(1.0);
        self.limb_amp += (amp - self.limb_amp) * (dtf * 10.0).min(1.0);

        // Head eases toward its target.
        let k = (dtf * 6.0).min(1.0);
        self.head_yaw += (self.head_target.0 - self.head_yaw) * k;
        self.head_pitch += (self.head_target.1 - self.head_pitch) * k;

        self.burn(dtf, world, ctx, rng);
    }

    /// Picks a movement direction and speed for this tick.
    fn think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        self.ai_timer -= dt;
        let to_player = ctx.player_pos - self.pos;
        let flat = DVec3::new(to_player.x, 0.0, to_player.z);
        let hdist = flat.length();

        if self.kind == MobKind::Zombie {
            let chasing = ctx.player_targetable && hdist < CHASE_RANGE && to_player.y.abs() < 12.0;
            if chasing {
                self.ai = Ai::Chase;
            } else if self.ai == Ai::Chase {
                self.ai = Ai::Idle;
                self.ai_timer = 1.0;
            }
        }

        match self.ai {
            Ai::Chase => {
                // Look at the player's face.
                let eye = to_player.y + 1.62 - self.shape().height * 0.9;
                let face_yaw = (to_player.z as f32).atan2(to_player.x as f32);
                self.head_target = (wrap(face_yaw - self.yaw).clamp(-1.2, 1.2), (eye as f32).atan2(hdist as f32));
                if hdist <= ATTACK_RANGE && to_player.y.abs() < 1.6 && self.attack_cooldown <= 0.0 {
                    self.attack_cooldown = ATTACK_COOLDOWN;
                    self.attack_anim = 0.35;
                    let dir = if hdist > 1e-6 { flat / hdist } else { DVec3::X };
                    let knockback = dir * 6.0 + DVec3::Y * 5.0;
                    events.push(EntityEvent::PlayerHit { damage: ZOMBIE_DAMAGE, knockback: knockback.as_vec3() });
                }
                // Stop just short so we don't stand inside the player.
                if hdist <= 0.8 {
                    return (None, 0.0);
                }
                // No pathfinding: head straight for the player, and if a
                // wall we can't jump is in the way, sidestep for a moment.
                let dir = flat / hdist;
                self.detour -= dt;
                if self.detour <= 0.0 && self.blocked && self.on_ground && !self.can_step_up(world, dir) {
                    self.detour = rng.range(0.6, 1.2);
                    self.detour_side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                }
                let dir = if self.detour > 0.0 {
                    let side = DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side;
                    (side + dir * 0.35).normalize()
                } else {
                    dir
                };
                (Some(dir), 2.4)
            }
            Ai::Panic => {
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(1.0, 3.0);
                    return (None, 0.0);
                }
                if rng.chance(dt * 1.2) || self.blocked && rng.chance(dt * 4.0) {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), 3.4)
            }
            Ai::Wander => {
                // Turn away from walls we can't hop over.
                if self.blocked && self.on_ground && !self.in_water && !self.can_step_up(world, yaw_dir(self.move_yaw))
                {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(2.0, 6.0);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), self.kind.wander_speed())
            }
            Ai::Idle => {
                // Occasionally look around.
                self.head_timer -= dt;
                if self.head_timer <= 0.0 {
                    self.head_timer = rng.range(1.0, 3.5);
                    self.head_target =
                        if rng.chance(0.6) { (rng.range(-1.1, 1.1), rng.range(-0.35, 0.3)) } else { (0.0, 0.0) };
                }
                if self.ai_timer <= 0.0 {
                    if rng.chance(0.7) {
                        self.ai = Ai::Wander;
                        self.ai_timer = rng.range(2.0, 5.0);
                        self.move_yaw = rng.range(0.0, TAU);
                    } else {
                        self.ai_timer = rng.range(2.0, 5.0);
                    }
                }
                (None, 0.0)
            }
        }
    }

    fn physics_step<W: BlockSource + ?Sized>(&mut self, dt: f64, world: &W, wish: Option<DVec3>, speed: f64) {
        let shape = self.shape();
        self.in_water = physics::is_water_at(world, self.pos + DVec3::new(0.0, 0.3, 0.0));
        let target = wish.map_or(DVec3::ZERO, |d| d * speed);

        if self.in_water {
            let k = (dt * 4.0).min(1.0);
            self.vel.x += (target.x * 0.6 - self.vel.x) * k;
            self.vel.z += (target.z * 0.6 - self.vel.z) * k;
            // Buoyant below ~0.6 blocks of depth, so mobs bob at the surface.
            if physics::is_water_at(world, self.pos + DVec3::new(0.0, 0.6, 0.0)) {
                self.vel.y = (self.vel.y + 22.0 * dt).min(2.0);
            } else {
                self.vel.y -= GRAVITY * 0.25 * dt;
            }
            self.vel.y = self.vel.y.max(-4.0);
            // Climb out onto the shore.
            if self.blocked && wish.is_some() {
                self.vel.y = self.vel.y.max(4.5);
            }
        } else {
            // Knockback isn't cancelled instantly: little control in the air.
            let k = (dt * if self.on_ground { 10.0 } else { 1.5 }).min(1.0);
            if self.on_ground || wish.is_some() {
                self.vel.x += (target.x - self.vel.x) * k;
                self.vel.z += (target.z - self.vel.z) * k;
            }
            self.vel.y = (self.vel.y - GRAVITY * dt).max(-78.0);
            if let Some(dir) = wish
                && self.on_ground
                && self.blocked
                && self.can_step_up(world, dir)
            {
                self.vel.y = JUMP_VELOCITY;
            }
        }
        if self.dying.is_some() && self.on_ground {
            // Corpses slide to a stop.
            self.vel.x *= 1.0 - (dt * 8.0).min(1.0);
            self.vel.z *= 1.0 - (dt * 8.0).min(1.0);
        }

        let delta = self.vel * dt;
        let hit = physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape);
        self.on_ground = hit.on_ground;
        self.blocked = hit.horizontal;
    }

    /// Whether the obstacle ahead is a single-block ledge we can jump onto.
    fn can_step_up<W: BlockSource + ?Sized>(&self, world: &W, dir: DVec3) -> bool {
        let raised = self.pos + DVec3::new(0.0, 1.02, 0.0);
        !physics::overlaps_solid(world, raised, self.shape())
            && !physics::overlaps_solid(world, raised + dir * 0.4, self.shape())
    }

    /// Zombies burn in direct sunlight.
    fn burn<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx, rng: &mut Rng) {
        let head = (self.pos + DVec3::new(0.0, self.shape().height - 0.1, 0.0)).floor().as_ivec3();
        self.burning = self.kind == MobKind::Zombie
            && self.alive()
            && ctx.daylight > BURN_DAYLIGHT
            && !self.in_water
            && world.exposed(head);
        if !self.burning {
            self.burn_timer = 0.0;
            return;
        }
        self.burn_timer += dt;
        if self.burn_timer >= 1.0 {
            self.burn_timer -= 1.0;
            self.damage(2.0, None, rng);
        }
    }

    /// Visual rotation of the whole body for the death animation (0..1).
    pub fn death_progress(&self) -> f32 {
        self.dying.map_or(0.0, |t| (t / (DEATH_TIME * 0.6)).min(1.0))
    }
}

/// Whether stepping in `dir` would drop more than [`MAX_SAFE_DROP`] blocks.
pub fn is_cliff<W: BlockSource + ?Sized>(world: &W, pos: DVec3, dir: DVec3, shape: Shape) -> bool {
    let probe = pos + dir * (shape.half_width + 0.35);
    let (x, z) = (probe.x.floor() as i32, probe.z.floor() as i32);
    let y = (pos.y + 0.01).floor() as i32;
    // A block at foot level ahead is a step up, not a drop.
    for dy in 0..=MAX_SAFE_DROP + 1 {
        match world.block(IVec3::new(x, y - dy, z)) {
            None => return true,
            Some(b) if b.is_solid() || b.is_water() => return false,
            Some(_) => {}
        }
    }
    true
}

/// Rough sky light (0..1) at a point: 1 in the open, dimmed under water or
/// leaves, and falling off with distance from the nearest open column
/// under overhangs; 0 deep underground.
pub fn sky_light<W: MobWorld + ?Sized>(world: &W, p: DVec3) -> f32 {
    let cell = p.floor().as_ivec3();
    if world.exposed(cell) {
        return 1.0;
    }
    // Straight up through translucent blocks (water, leaves)?
    let mut level = 15i32;
    let mut y = cell.y + 1;
    let top = world.surface(cell.x, cell.z).unwrap_or(cell.y);
    while y <= top && level > 0 {
        match world.block(IVec3::new(cell.x, y, cell.z)) {
            Some(b) if b.is_opaque() => {
                level = 0;
                break;
            }
            Some(b) if b.light_opacity() > 0 => level -= 2,
            _ => {}
        }
        y += 1;
    }
    // Otherwise light spilling in from the side.
    for d in 1..=4i32 {
        let side = 15 - 2 * d;
        if side <= level {
            break;
        }
        let ring = (-d..=d).flat_map(|a| [(a, -d), (a, d), (-d, a), (d, a)]);
        if ring.into_iter().any(|(dx, dz)| world.exposed(cell + IVec3::new(dx, 0, dz))) {
            level = side;
            break;
        }
    }
    level.max(0) as f32 / 15.0
}

pub fn yaw_dir(yaw: f32) -> DVec3 {
    let (s, c) = yaw.sin_cos();
    DVec3::new(c as f64, 0.0, s as f64)
}

/// Angle wrapped to [-PI, PI).
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

fn turn_toward(from: f32, to: f32, max: f32) -> f32 {
    let d = wrap(to - from);
    wrap(from + d.clamp(-max, max))
}
