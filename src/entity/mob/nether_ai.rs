//! Per-tick behaviour of the newer Nether mobs: admiring gold, fighting with
//! a golden sword or a crossbow, running away and fetching items. Goals come
//! from the periodic look around in `entity::nether`.

use glam::{DVec3, IVec3};

use super::{ATTACK_RANGE, Ai, Ctx, EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng, line_of_sight, wrap};
use crate::entity::nether::{Foe, Goal, LavaGoal, Weapon, ZOMBIFY_TIME};
use crate::world::block::Block;
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
/// Height of `LiquidBlock.STABLE_SHAPE` (8/16), which a lava source offers
/// mobs that can stand on lava.
pub(crate) const STABLE_LAVA_HEIGHT: f64 = 0.5;
/// `Mob.maxUpStep` for a strider.
const STEP_HEIGHT: f64 = crate::player::STEP_HEIGHT;

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
            return self.go_to_lava(world, rng);
        }
        let n = self.nether.as_mut()?;
        n.admire_disabled = (n.admire_disabled - dt).max(0.0);
        n.hunt_cooldown = (n.hunt_cooldown - dt).max(0.0);
        n.ate = (n.ate - dt).max(0.0);
        n.celebrate_left = (n.celebrate_left - dt).max(0.0);
        n.dancing &= n.celebrate_left > 0.0;
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
                Some((self.pursue(world, at, speed, true, false, rng), speed))
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
                Some(self.nether_attack(foe, at, speed, dt, world, ctx, rng, events))
            }
        }
    }

    /// Java's `StriderGoToLavaGoal`, a `MoveToBlockGoal` with an 8-block
    /// search range and 2 up or down: a strider out of lava finds the
    /// nearest lava under open space in Java's search order and walks a path
    /// there, replanning every 20 ticks. Unlike its parent goal it has no
    /// give-up or stay timer: it runs until the strider is in lava or the
    /// lava is gone. It searches again 200–400 ticks after the last search.
    /// `None` leaves it to the lower-priority goals (following its parent,
    /// strolling); panicking takes precedence.
    fn go_to_lava<W: MobWorld + ?Sized>(&mut self, world: &W, rng: &mut Rng) -> Option<(Option<DVec3>, f64)> {
        const RANGE: i32 = 8;
        const VERTICAL: i32 = 2;
        let valid = |p: IVec3| {
            world.block(p).is_some_and(Block::is_lava)
                && world
                    .block(p + IVec3::Y)
                    .is_some_and(|b| !b.is_lava() && crate::entity::path::pathfindable_on_land(b))
        };
        let in_lava = self.nether.as_ref().is_some_and(|n| !n.cold);
        let panicking = self.ai == Ai::Panic;
        let speed = self.kind.wander_speed();
        let n = self.nether.as_mut()?;
        if let Some(goal) = n.lava_goal {
            // Strider's `canContinueToUse` override, which skips `MoveToBlockGoal`'s timers.
            if in_lava || panicking || !valid(goal.target) {
                n.lava_goal = None;
                self.stop_navigation();
                if self.ai == Ai::Wander {
                    self.ai = Ai::Idle;
                    self.ai_timer = 0.0;
                }
                return None;
            }
        } else {
            // `canUse`, evaluated every other tick by the goal selector.
            if in_lava || panicking {
                return None;
            }
            if n.lava_next_start > 0 {
                n.lava_next_start -= 1;
                return None;
            }
            // `reducedTickDelay(200 + nextInt(200))` goal updates, two ticks each.
            n.lava_next_start = (200 + rng.next_int(200) as i32 + 1) / 2 * 2;
            let target = nearest_block(self.pos.floor().as_ivec3(), RANGE, VERTICAL, valid)?;
            n.lava_goal = Some(LavaGoal { target, try_ticks: 0 });
            // `moveMobToBlock`: the cell above the lava.
            self.walk_to(world, target + IVec3::Y, 1, speed, false);
        }
        let n = self.nether.as_mut()?;
        let goal = n.lava_goal.as_mut()?;
        let target = goal.target;
        // `tick`: within a block of the lava cell's centre counts as there.
        if (target.as_dvec3() + DVec3::splat(0.5)).distance_squared(self.pos) < 1.0 {
            goal.try_ticks -= 1;
        } else {
            goal.try_ticks += 1;
            if goal.try_ticks % 20 == 0 {
                self.walk_to(world, target, 1, speed, false);
            }
        }
        self.ai = Ai::Wander;
        self.look_at(target.as_dvec3() + DVec3::splat(0.5), 0.5);
        let dir = self.follow_path(world, speed, false);
        Some((dir, speed))
    }

    /// Fights `foe`, whose feet are at `at`: crossbow piglins keep between 5
    /// and 8 blocks and shoot living targets they can see; other weapons close in
    /// and strikes once a second.
    #[allow(clippy::too_many_arguments)]
    fn nether_attack<W: MobWorld + ?Sized>(
        &mut self,
        foe: Foe,
        at: DVec3,
        speed: f64,
        dt: f32,
        world: &W,
        ctx: &Ctx,
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
        if weapon == Weapon::Crossbow {
            let sees = hdist <= CROSSBOW_RANGE && line_of_sight(world, eye, aim);
            let n = self.nether.as_mut().unwrap();
            if sees {
                if n.charge == 0.0 {
                    n.aim_at = CROSSBOW_CHARGE + rng.range(CROSSBOW_AIM.0, CROSSBOW_AIM.1);
                }
                n.charge += dt;
                if n.charge >= n.aim_at {
                    n.charge = 0.0;
                    self.attack_anim = 0.35;
                    events.push(EntityEvent::Shoot {
                        from: eye + dir * 0.5,
                        target: aim,
                        cause: "was shot by a piglin",
                        shooter: self.uid,
                    });
                    events.push(EntityEvent::Sound { sound: MobSound::Bow, pos: eye });
                }
            } else {
                n.charge = n.charge.min(CROSSBOW_CHARGE);
            }
            return if !sees {
                (self.pursue(world, at, speed, false, true, rng), speed)
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
            if let Foe::Player(id) = foe
                && let Some(target) = ctx.players.iter().find(|t| t.id == id)
            {
                self.thorns_response(target, knockback, rng, events);
            }
        }
        if hdist <= 0.8 + if tusks { self.shape().half_width } else { 0.0 } {
            return (None, 0.0);
        }
        let sees = line_of_sight(world, eye, aim);
        (self.pursue(world, at, speed, sees, true, rng), speed)
    }

    /// Turns the head toward a point, up to `max` radians off the body.
    pub(crate) fn look_at(&mut self, at: DVec3, max: f32) {
        let to = at - self.pos;
        let hdist = to.x.hypot(to.z);
        let eye = to.y + 1.62 - self.shape().height * 0.9;
        let face_yaw = (to.z as f32).atan2(to.x as f32);
        self.head_target = (wrap(face_yaw - self.yaw).clamp(-max, max), (eye as f32).atan2(hdist as f32));
    }

    /// Heads along `dir`, side-stepping for a moment when blocked by a wall
    /// it can't jump (the same detour the other chasers use).
    pub(crate) fn steer<W: MobWorld + ?Sized>(&mut self, dir: DVec3, dt: f32, world: &W, rng: &mut Rng) -> DVec3 {
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

    /// Java's `floatStrider` and `LiquidBlock.STABLE_SHAPE`: a strider stands
    /// on a lava source with no lava above it as on a bottom slab, half a
    /// block up with its legs in the lava; sunk deeper it rises. Returns
    /// whether it handled this physics step.
    pub(super) fn walk_on_lava<W: crate::physics::BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        wish: Option<DVec3>,
        speed: f64,
    ) -> bool {
        let cell = |p: DVec3| p.floor().as_ivec3();
        let support = cell(self.pos - DVec3::Y * 0.01);
        let surface = (world.block(support) == Some(Block::LAVA)
            && !world.block(support + IVec3::Y).is_some_and(Block::is_lava))
        .then_some(support.y as f64 + STABLE_LAVA_HEIGHT);
        // Anywhere in the lava cell above the stable shape it falls onto it,
        // as onto a bottom slab; below it, it floats up.
        let standing = surface.is_some_and(|s| self.pos.y >= s - 0.05);
        let sunk = !standing && world.block(cell(self.pos + DVec3::Y * 0.05)).is_some_and(Block::is_lava);
        if !self.alive() || !sunk && !standing || self.vel.y > 0.5 && !sunk {
            return false;
        }
        self.in_water = false;
        let target = wish.map_or(DVec3::ZERO, |d| d * speed);
        let k = (dt * 10.0).min(1.0);
        self.vel.x += (target.x - self.vel.x) * k;
        self.vel.z += (target.z - self.vel.z) * k;
        let shape = self.shape();
        let (hit, landed) = if let Some(s) = surface.filter(|_| standing) {
            // Fall onto the stable shape through the ordinary collision, so a
            // shore block under part of the box holds it up first.
            self.vel.y = self.vel.y.min(0.0) - super::GRAVITY * dt;
            let mut delta = self.vel * dt;
            delta.y = delta.y.max(s - self.pos.y);
            // On the stable shape it steps like ground: a shore whose top is
            // level with the lava cell's top is half a block up.
            let hit = if self.on_ground {
                crate::physics::move_box_stepping(world, &mut self.pos, &mut self.vel, delta, shape, STEP_HEIGHT)
            } else {
                crate::physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape)
            };
            let landed = self.pos.y <= s + 1e-9;
            if landed || hit.on_ground {
                self.vel.y = 0.0;
            }
            (hit, landed)
        } else {
            self.vel.y = 2.0;
            let delta = self.vel * dt;
            (crate::physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape), false)
        };
        self.on_ground = landed || hit.on_ground;
        self.blocked = hit.horizontal;
        true
    }
}

/// `MoveToBlockGoal.findNearestBlock`: rings out from the mob's block, one
/// below its feet first, then level, two below, one above and three below,
/// alternating sides in each ring as Java does.
fn nearest_block(origin: IVec3, range: i32, vertical: i32, valid: impl Fn(IVec3) -> bool) -> Option<IVec3> {
    let alternate = |v: i32| if v > 0 { -v } else { 1 - v };
    let mut y = 0;
    while y <= vertical {
        for r in 0..range {
            let mut x = 0;
            while x <= r {
                let mut z = if x < r && x > -r { r } else { 0 };
                while z <= r {
                    let p = origin + IVec3::new(x, y - 1, z);
                    if valid(p) {
                        return Some(p);
                    }
                    z = alternate(z);
                }
                x = alternate(x);
            }
        }
        y = alternate(y);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PlayerId, Target};
    use crate::physics::test_util::Grid;
    use glam::IVec3;

    #[test]
    fn crossbow_piglins_shoot_mob_targets_instead_of_switching_to_melee() {
        let world = Grid::flat(0);
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![],
            daylight: 1.,
            spawning: false,
            raining: false,
            dimension: Dimension::Nether,
        };
        let mut piglin = Mob::new(MobKind::Piglin, DVec3::new(0.5, 1., 0.5), 0.);
        piglin.uid = 1;
        piglin.nether.as_mut().unwrap().weapon = Weapon::Crossbow;
        let mut rng = Rng::new(7);
        let mut events = Vec::new();
        for _ in 0..80 {
            piglin.nether_attack(Foe::Mob(2), DVec3::new(6., 1., 0.5), 2., 0.05, &world, &ctx, &mut rng, &mut events);
        }
        assert!(events.iter().any(|e| matches!(e, EntityEvent::Shoot { shooter: 1, .. })));
        assert!(events.iter().all(|e| !matches!(e, EntityEvent::MobHit { .. })));
    }

    #[test]
    fn crossbow_samples_one_aim_delay_per_shot_and_keeps_it_when_sight_is_lost() {
        let mut world = Grid::flat(10);
        let at = DVec3::new(6.5, 10.0, 0.5);
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            players: vec![Target::new(PlayerId::HOST, at, true)],
            daylight: 0.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Nether,
        };
        let mut mob = Mob::new(MobKind::Piglin, DVec3::new(0.5, 10.0, 0.5), 0.0);
        mob.nether.as_mut().unwrap().weapon = Weapon::Crossbow;
        let mut rng = Rng::new(42);
        let mut expected = Rng::new(42);
        let mut events = Vec::new();
        let delay = CROSSBOW_CHARGE + expected.range(CROSSBOW_AIM.0, CROSSBOW_AIM.1);
        let dt = 0.05;
        for _ in 0..30 {
            mob.nether_attack(Foe::Player(PlayerId::HOST), at, 2.0, dt, &world, &ctx, &mut rng, &mut events);
        }
        assert!(events.is_empty(), "still aiming");
        assert_eq!(mob.nether.as_ref().unwrap().aim_at, delay);
        assert_eq!(rng.next_f32(), expected.next_f32(), "aiming only draws once");

        let wall = glam::IVec3::new(3, 11, 0);
        world.set(wall, crate::world::block::Block::STONE);
        mob.nether_attack(Foe::Player(PlayerId::HOST), at, 2.0, dt, &world, &ctx, &mut rng, &mut events);
        assert_eq!(mob.nether.as_ref().unwrap().charge, CROSSBOW_CHARGE);
        assert_eq!(mob.nether.as_ref().unwrap().aim_at, delay, "losing sight keeps this shot's delay");
        world.set(wall, crate::world::block::Block::AIR);
        let mut elapsed = CROSSBOW_CHARGE;
        while events.is_empty() {
            mob.nether_attack(Foe::Player(PlayerId::HOST), at, 2.0, dt, &world, &ctx, &mut rng, &mut events);
            elapsed += dt;
            assert!(elapsed < delay + dt * 2.0, "the shot fires at its sampled threshold");
        }
        assert!(elapsed >= delay && elapsed < delay + dt);
        assert!(matches!(events[0], EntityEvent::Shoot { cause: "was shot by a piglin", .. }));
        assert_eq!(mob.nether.as_ref().unwrap().charge, 0.0);

        // Movement while sight was lost may use RNG; compare from this point.
        let mut expected = Rng(rng.0);
        let next = CROSSBOW_CHARGE + expected.range(CROSSBOW_AIM.0, CROSSBOW_AIM.1);
        events.clear();
        mob.nether_attack(Foe::Player(PlayerId::HOST), at, 2.0, dt, &world, &ctx, &mut rng, &mut events);
        assert_eq!(mob.nether.as_ref().unwrap().aim_at, next, "the next load draws a new delay");
        assert_eq!(rng.next_f32(), expected.next_f32());
    }

    #[test]
    fn strider_lava_search_follows_javas_ring_order() {
        let within = |set: &'static [(i32, i32, i32)]| move |p: IVec3| set.contains(&(p.x, p.y, p.z));
        let o = IVec3::ZERO;
        // One below the feet first, then level, two below, one above, three below.
        assert_eq!(nearest_block(o, 8, 2, |_| true), Some(IVec3::new(0, -1, 0)));
        assert_eq!(nearest_block(o, 8, 2, within(&[(0, 0, 0), (0, -2, 0)])), Some(IVec3::ZERO));
        assert_eq!(nearest_block(o, 8, 2, within(&[(0, 1, 0), (0, -2, 0)])), Some(IVec3::new(0, -2, 0)));
        assert_eq!(nearest_block(o, 8, 2, within(&[(0, -3, 0), (0, 1, 0)])), Some(IVec3::new(0, 1, 0)));
        // A ring before its layer's next ring; +z before -x in ring one.
        assert_eq!(nearest_block(o, 8, 2, within(&[(1, -1, 0), (0, -1, 1), (-1, -1, 0)])), Some(IVec3::new(0, -1, 1)));
        assert_eq!(nearest_block(o, 8, 2, within(&[(2, -1, 0), (1, 0, 1)])), Some(IVec3::new(2, -1, 0)));
        // Java's range is exclusive: ring 7 is the last.
        assert_eq!(nearest_block(o, 8, 2, within(&[(8, -1, 0)])), None);
        assert_eq!(nearest_block(o, 8, 2, within(&[(7, -1, -7)])), Some(IVec3::new(7, -1, -7)));
    }
}
