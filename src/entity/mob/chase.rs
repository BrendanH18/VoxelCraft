//! v1.0 gap Track 2: Java 26.3's goal scheduling for the shared hostile
//! chase, and the zombies' goal table with `SpearUseGoal`.
//!
//! - **Targets** (`NearestAttackableTargetGoal` with `mustSee`, over
//!   `TargetGoal`): a mob without a target looks for one only on its goal
//!   selector's full update (`(tickCount + id) % 2 == 0`), and then only one
//!   time in `reducedTickDelay(10)` = 5; it takes the nearest player it can
//!   see within its follow range. It keeps that target, rather than
//!   switching to a nearer player, while the target lives, stays targetable
//!   and within the follow range and was seen within
//!   `reducedTickDelay(60)` = 30 full updates. A target set from outside the
//!   goal (reinforcements, `HurtByTargetGoal`) is taken at once.
//! - **`MeleeAttackGoal`**: `canUse` runs at most every 20 ticks and needs a
//!   path to the target (or the target in reach); the goal stops when the
//!   path runs out (unless it follows unseen targets, as spiders do), and
//!   restarts on a later check. It ticks every tick (`requiresUpdateEveryTick`):
//!   it hits when the attack box (the mob's box widened by
//!   `sqrt(2.04) − 0.6`) meets the target's and it sees the target, then
//!   waits 20 ticks; a new start may hit at once.
//! - **Zombies' goals** (`Zombie.addBehaviourGoals`, `ZombifiedPiglin`'s):
//!   the door goal (flagless, see [`super::door_break`]), `SpearUseGoal`
//!   (priority 2, or 1 for zombified piglins; `MOVE` and `LOOK`) and
//!   `ZombieAttackGoal` (3, or 2) through [`super::selector`], so a held
//!   spear (a `KineticWeapon`) takes over from the melee goal.
//! - **`SpearUseGoal`** (ticked on full updates only): walk at the target
//!   until within 10 blocks, lower the spear (`startUsingItem`) for
//!   `reducedTickDelay(delay + damage window)` updates, charge at the
//!   target and, within 2 blocks or at the path's end, run on to a spot
//!   6–7 blocks away from it (`LandRandomPos.getPosAway`), then charge
//!   again; when the time is up, raise the spear and retreat 9–11 blocks,
//!   and finish there or after `reducedTickDelay(100)` updates. Riders add
//!   2 to those distances and steer their mount, at its
//!   `chargeSpeedModifier` (1.4 for zombie horses, 4 for camel husks).
//!   While the spear is lowered, `KineticWeapon.damageEntities` stabs
//!   players and mobs along the mob's look ([`super::spear`]): the base
//!   attack damage plus `floor(relative speed × multiplier)`, with the 0.2
//!   mob factor on every speed threshold.
use glam::{DVec3, IVec3};

use super::selector::{self, Host, LOOK, MOVE, Selector, Spec};
use super::{Ai, Mob, MobWorld, Rng, line_of_sight};
use crate::entity::{Ctx, EntityEvent, MobKind, Target};
use crate::weapons::spear;

/// `reducedTickDelay(10)`: `NearestAttackableTargetGoal`'s default
/// `randomInterval`.
pub const TARGET_INTERVAL: u32 = 5;
/// `reducedTickDelay(60)`: `TargetGoal.unseenMemoryTicks` in full updates.
pub const UNSEEN_UPDATES: i32 = 30;
/// `MeleeAttackGoal.COOLDOWN_BETWEEN_CAN_USE_CHECKS`.
pub const CAN_USE_COOLDOWN: u32 = 20;
/// `MeleeAttackGoal.getAttackInterval`: `adjustedTickDelay(20)`, and the
/// goal ticks every tick.
pub const ATTACK_INTERVAL: i32 = 20;
/// `SpearUseGoal`'s constructor arguments for zombies and zombified
/// piglins: both speed modifiers 1, approach 10, in range 2.
pub const SPEAR_APPROACH: f64 = 10.0;
pub const SPEAR_IN_RANGE: f64 = 2.0;
/// `SpearUseGoal.MAX_FLEEING_TIME`: `reducedTickDelay(100)`.
pub const SPEAR_MAX_FLEEING: i32 = 50;

/// `MeleeAttackGoal`'s state (never saved).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Melee {
    pub running: bool,
    /// `lastCanUseCheck`, on the mob's tick clock.
    pub last_check: Option<u32>,
    /// `ticksUntilNextAttack`.
    pub wait: i32,
    /// `ZombieAttackGoal.raiseArmTicks`.
    pub raise_arm: u32,
}

/// `SpearUseGoal.SpearUseState` (never saved).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpearUse {
    pub engage: i32,
    pub fleeing: i32,
    pub away: Option<DVec3>,
    pub done: bool,
    /// Where the goal's last `moveTo` asked to go, and at what speed
    /// modifier (a mount steers there).
    pub dest: Option<DVec3>,
    pub speed: f64,
}

impl Default for SpearUse {
    fn default() -> Self {
        Self::new()
    }
}

impl SpearUse {
    fn new() -> Self {
        Self { engage: -1, fleeing: -1, away: None, done: false, dest: None, speed: 1.0 }
    }

    /// `tickAndCheckEngagement`.
    fn engagement_over(&mut self) -> bool {
        if self.engage > 0 {
            self.engage -= 1;
            return self.engage == 0;
        }
        false
    }

    /// `tickAndCheckFleeing`.
    fn fleeing_over(&mut self) -> bool {
        if self.fleeing > 0 {
            self.fleeing += 1;
            if self.fleeing > SPEAR_MAX_FLEEING {
                self.done = true;
                return true;
            }
        }
        false
    }
}

/// A lowered spear (`startUsingItem` with a `KineticWeapon`): ticks used
/// and `recentKineticEnemies`, each with the use tick it was stabbed on
/// (the memory starts afresh with each use, so the use clock serves as
/// Java's game time; never saved).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SpearCharge {
    pub used: u32,
    pub stabbed: Vec<(super::spear::Stabbed, u32)>,
}

/// The zombie goals the selector runs (the door goal runs on its own).
const SPEAR: usize = 0;
const ATTACK: usize = 1;

/// `addBehaviourGoals`: zombified piglins rank both goals one higher.
fn zombie_specs(kind: MobKind) -> [Spec; 2] {
    let first = if kind == MobKind::ZombifiedPiglin { 1 } else { 2 };
    [
        selector::spec(first, MOVE | LOOK),
        Spec { priority: first + 1, flags: MOVE | LOOK, interruptable: true, every_tick: true },
    ]
}

/// Kinds whose chase runs the zombie goal table: `Zombie` and the
/// subclasses that keep its `addBehaviourGoals` (husks, zombie villagers)
/// or replace it with the same two goals (zombified piglins). Drowned
/// register their own trident and attack goals.
pub(crate) fn zombie_table(kind: MobKind) -> bool {
    matches!(kind, MobKind::Zombie | MobKind::Husk | MobKind::ZombieVillager | MobKind::ZombifiedPiglin)
}

/// Kinds whose shared chase uses Java's `MeleeAttackGoal`: everything that
/// bites or hits in the shared chase except cubes (`SlimeAttackGoal`).
pub(crate) fn java_melee(kind: MobKind) -> bool {
    !kind.is_cube()
        && !matches!(
            kind,
            MobKind::Skeleton
                | MobKind::Stray
                | MobKind::Bogged
                | MobKind::Parched
                | MobKind::Witch
                | MobKind::Creeper
                | MobKind::Enderman
        )
}

/// `MeleeAttackGoal(…, followingTargetEvenIfNotSeen)`: spiders follow
/// targets they can't see; zombies and silverfish don't.
fn follows_unseen(kind: MobKind) -> bool {
    matches!(kind, MobKind::Spider | MobKind::CaveSpider)
}

impl Mob {
    /// The selector clock for the shared chase: whether this tick is the
    /// full update. Kinds with an animal goal table already counted the
    /// tick in `run_goals`.
    pub(super) fn chase_full(&mut self) -> bool {
        if super::goals::table(self.kind).is_empty() {
            self.selector_full()
        } else {
            let t = self.goals.ticks.wrapping_sub(1);
            t.wrapping_add(self.uid).is_multiple_of(2) || t <= 1
        }
    }

    fn eye_pos(&self) -> DVec3 {
        self.look_eye()
    }

    fn sees(&self, world: &(impl MobWorld + ?Sized), t: &Target) -> bool {
        line_of_sight(world, self.eye_pos(), t.eye())
    }

    /// `NearestAttackableTargetGoal` and `TargetGoal` for the shared chase.
    /// `forced` is a target set from outside the goal (reinforcements, a
    /// hurt-by retaliation), taken at once and kept while it is in range;
    /// `aggressive` gates a new search (spiders by day, witches cooling
    /// down). `range` is the follow range and `height` the vertical limit
    /// of a new search. Sets `ai` to `Chase` or back to `Idle`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn chase_schedule<'a, W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &'a Ctx,
        forced: Option<&'a Target>,
        aggressive: bool,
        range: f64,
        height: f64,
        full: bool,
        rng: &mut Rng,
    ) -> Option<&'a Target> {
        let pos = self.pos;
        let in_range = |t: &Target| t.targetable && t.alive && t.pos.distance_squared(pos) <= range * range;
        if let Some(t) = forced.filter(|t| in_range(t)) {
            if self.chase_target != Some(t.id) {
                self.chase_unseen = 0;
            }
            self.chase_target = Some(t.id);
            self.ai = Ai::Chase;
            return Some(t);
        }
        // `TargetGoal.canContinueToUse`, on full updates.
        if let Some(id) = self.chase_target {
            let current = ctx.players.iter().find(|t| t.id == id).filter(|t| in_range(t));
            let mut keep =
                current.is_some() && (aggressive || matches!(self.kind, MobKind::Spider | MobKind::CaveSpider));
            if keep && full {
                // `SpiderAttackGoal.canContinueToUse`: in daylight, one
                // update in 100 lets go.
                if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider) && !aggressive && rng.next_int(100) == 0 {
                    keep = false;
                } else if current.is_some_and(|t| self.sees(world, t)) {
                    self.chase_unseen = 0;
                } else {
                    self.chase_unseen += 1;
                    keep = self.chase_unseen <= UNSEEN_UPDATES;
                }
            }
            if keep {
                self.ai = Ai::Chase;
                return current;
            }
            self.drop_chase_target();
        }
        // `NearestAttackableTargetGoal.canUse`.
        if !full || !aggressive || rng.next_int(TARGET_INTERVAL) != 0 {
            return None;
        }
        let found = ctx
            .players
            .iter()
            .filter(|t| {
                t.targetable
                    && t.alive
                    && (t.pos.y - self.pos.y).abs() < height
                    && t.pos.distance_squared(self.pos) <= t.detection_range_for(range, self.kind).powi(2)
            })
            .filter(|t| self.sees(world, t))
            .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))?;
        self.chase_target = Some(found.id);
        self.chase_unseen = 0;
        self.ai = Ai::Chase;
        Some(found)
    }

    /// `TargetGoal.stop` (`setTarget(null)`) and the chase goals' stops.
    pub(super) fn drop_chase_target(&mut self) {
        self.chase_target = None;
        self.chase_unseen = 0;
        if self.ai == Ai::Chase {
            self.ai = Ai::Idle;
            self.ai_timer = 1.0;
        }
        self.stop_chase_goals();
    }

    /// Stops the running chase goals (`MeleeAttackGoal.stop`,
    /// `SpearUseGoal.stop`).
    pub(super) fn stop_chase_goals(&mut self) {
        if self.melee.running || self.spear.is_some() {
            self.stop_navigation();
        }
        self.melee.running = false;
        self.spear = None;
        self.spear_use = None;
        self.chase_goals = Selector::default();
    }

    /// `isWithinMeleeAttackRange`: the attack box (with a vehicle's box
    /// when riding) meets the target's.
    pub(super) fn in_melee_reach(&self, t: &Target) -> bool {
        let shape = self.shape();
        let reach = crate::entity::endermite::ATTACK_REACH;
        (self.pos.x - t.pos.x).abs() < shape.half_width + t.shape.half_width + reach
            && (self.pos.z - t.pos.z).abs() < shape.half_width + t.shape.half_width + reach
            && self.pos.y < t.pos.y + t.shape.height
            && self.pos.y + shape.height > t.pos.y
    }

    /// `MeleeAttackGoal.canUse`.
    fn melee_can_use<W: MobWorld + ?Sized>(&mut self, world: &W, t: &Target, speed: f64) -> bool {
        let now = self.goals.ticks;
        if self.melee.last_check.is_some_and(|at| now.wrapping_sub(at) < CAN_USE_COOLDOWN) {
            return false;
        }
        self.melee.last_check = Some(now);
        if !t.alive {
            return false;
        }
        if self.riding.is_some() {
            return true;
        }
        // `createPath(target, 0)`; `start` then walks it.
        self.stop_navigation();
        self.walk_to(world, t.pos.floor().as_ivec3(), 0, speed, true) || self.in_melee_reach(t)
    }

    /// `MeleeAttackGoal.canContinueToUse`. A rider's navigation is its
    /// mount's, which steers on its own here, so a rider keeps the goal
    /// while the target lives.
    fn melee_can_continue(&self, t: &Target) -> bool {
        t.alive && if follows_unseen(self.kind) || self.riding.is_some() { t.targetable } else { !self.nav.is_done() }
    }

    /// `MeleeAttackGoal.start` (and `ZombieAttackGoal`'s).
    fn melee_start(&mut self) {
        self.melee.running = true;
        self.melee.wait = 0;
        self.melee.raise_arm = 0;
        self.repath_in = 0;
    }

    /// `MeleeAttackGoal.tick`: chase (re-planning as Java does in
    /// [`Mob::pursue`]), then hit when in reach and in sight.
    fn melee_tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        t: &Target,
        speed: f64,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let to = t.pos - self.pos;
        let flat = to.with_y(0.0);
        let hdist = flat.length();
        let dir = if hdist > 1e-6 { flat / hdist } else { DVec3::X };
        let sees = self.sees(world, t);
        let wish = if hdist <= 0.8 {
            None
        } else {
            self.pursue(world, t.pos, speed, sees || follows_unseen(self.kind), true, rng)
        };
        self.melee.wait = (self.melee.wait - 1).max(0);
        if self.in_melee_reach(t) && self.melee.wait <= 0 && sees {
            self.melee.wait = ATTACK_INTERVAL;
            self.attack_cooldown = ATTACK_INTERVAL as f32 / 20.0;
            self.hostile_melee_hit(t, dir, rng, events);
        }
        // `ZombieAttackGoal`: arms up from the fifth tick while the next
        // hit is under half the interval away.
        self.melee.raise_arm += 1;
        (wish, speed)
    }

    /// The shared chase's melee for kinds outside the zombie table: one
    /// `MeleeAttackGoal`, started and stopped on full updates.
    pub(super) fn chase_melee<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        t: &Target,
        speed: f64,
        full: bool,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        if full {
            if self.melee.running && !self.melee_can_continue(t) {
                self.melee.running = false;
                self.stop_navigation();
            }
            if !self.melee.running && self.melee_can_use(world, t, speed) {
                self.melee_start();
            }
        }
        if self.melee.running { self.melee_tick(world, t, speed, rng, events) } else { (None, 0.0) }
    }

    /// The zombie goal table for one tick of the chase: the door goal, then
    /// `SpearUseGoal` and `ZombieAttackGoal` through the selector.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn zombie_chase<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        t: &Target,
        speed: f64,
        full: bool,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let specs = zombie_specs(self.kind);
        let mut selector = std::mem::take(&mut self.chase_goals);
        let mut host = ZombieHost { m: self, world, ctx, t, speed, rng, events, wish: None };
        selector.tick(&specs, full, &mut host);
        let wish = host.wish;
        self.chase_goals = selector;
        if let Some(wish) = wish {
            return wish;
        }
        // Between its full updates `SpearUseGoal` doesn't tick, but the
        // navigation it set keeps walking.
        if self.spear.is_some() && self.riding.is_none() {
            let speed = self.spear.as_ref().map_or(1.0, |s| s.speed);
            let speed = self.kind.chase_speed() * speed * if self.baby { 1.5 } else { 1.0 };
            return (self.follow_path(world, speed, true), speed);
        }
        (None, 0.0)
    }

    /// `SpearUseGoal.tick` (on full updates).
    fn spear_tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        t: &Target,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        let Some(tier) = self.holds_spear() else { return (None, 0.0) };
        let mut s = self.spear.unwrap_or_default();
        let dist2 = self.pos.distance_squared(t.pos);
        // A mount applies its own `chargeSpeedModifier` when it steers to
        // `dest` (`Entities::steer_spear_mounts`); on foot it is 1.
        let charge = 1.0;
        let mount_distance = if self.riding.is_some() { 2.0 } else { 0.0 };
        self.face_target(t);
        let to_target = |s: &mut SpearUse, speed: f64| {
            s.dest = Some(t.pos);
            s.speed = speed;
        };
        if s.engage < 0 {
            if dist2 > SPEAR_APPROACH * SPEAR_APPROACH {
                to_target(&mut s, charge);
                self.spear = Some(s);
                return self.spear_move(world, t.pos, charge);
            }
            let kinetic = spear::kinetic(tier);
            let duration = (kinetic.delay_ticks + kinetic.damage.max_ticks) as i32;
            s.engage = selector::reduced(duration);
            // `startUsingItem(MAIN_HAND)`.
            self.lower_spear();
        }
        if s.engagement_over() {
            self.spear_use = None;
            let d = dist2.sqrt();
            s.away = self.land_pos_away(
                world,
                ctx,
                rng,
                (9.0 + mount_distance - d).max(0.0),
                (11.0 + mount_distance - d).max(1.0),
                7,
                t.pos,
            );
            s.fleeing = 1;
        }
        let mut wish = (None, 0.0);
        if !s.fleeing_over() {
            if let Some(away) = s.away {
                s.dest = Some(away);
                s.speed = charge;
                wish = self.spear_move(world, away, charge);
                if self.nav.is_done() {
                    if s.fleeing > 0 {
                        s.done = true;
                        self.spear = Some(s);
                        return wish;
                    }
                    s.away = None;
                }
            } else {
                to_target(&mut s, charge);
                wish = self.spear_move(world, t.pos, charge);
                if dist2 < SPEAR_IN_RANGE * SPEAR_IN_RANGE || self.nav.is_done() {
                    let d = dist2.sqrt();
                    s.away = self.land_pos_away(
                        world,
                        ctx,
                        rng,
                        6.0 + mount_distance - d,
                        7.0 + mount_distance - d,
                        7,
                        t.pos,
                    );
                }
            }
        }
        self.spear = Some(s);
        wish
    }

    /// `navigation.moveTo(…)` for the spear goal; a rider leaves the walking
    /// to its mount (which steers to the goal's `dest`).
    pub(super) fn spear_move<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        to: DVec3,
        modifier: f64,
    ) -> (Option<DVec3>, f64) {
        if self.riding.is_some() {
            return (None, 0.0);
        }
        let speed = self.kind.chase_speed() * modifier * if self.baby { 1.5 } else { 1.0 };
        // `moveTo(Entity)` and `moveTo(x, y, z)` both ask `createPath` with
        // reach 1 (v1.0 gap Track 2b: an entity target used reach 0), which
        // keeps an unfinished path that already ends at the same block.
        self.walk_to(world, to.floor().as_ivec3(), 1, speed, true);
        (self.follow_path(world, speed, true), speed)
    }

    /// `mob.lookAt(target, 30, 30)` turns the body by at most 30°, and
    /// `getLookControl().setLookAt(target, 30, 30)` the head and pitch
    /// (see [`super::spear`]).
    fn face_target(&mut self, t: &Target) {
        use super::spear::SPEAR_GOAL_TURN;
        self.turn_body_to(t.pos, SPEAR_GOAL_TURN);
        self.set_look_at(t.eye(), SPEAR_GOAL_TURN, SPEAR_GOAL_TURN);
    }

    /// `LandRandomPos.getPosAway(mob, min, max, vertical, avoid)`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn land_pos_away<W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        min: f64,
        max: f64,
        v: i32,
        avoid: DVec3,
    ) -> Option<DVec3> {
        let mut away = self.pos - avoid;
        if away.length() == 0.0 {
            away = DVec3::new(next_double(rng) - 0.5, 0.0, next_double(rng) - 0.5);
        }
        let caps = self.caps(false);
        let mut best: Option<(f32, IVec3)> = None;
        for _ in 0..10 {
            // `generateRandomDirectionWithinRadians(…, PI / 2)`.
            let center = away.z.atan2(away.x) - std::f32::consts::FRAC_PI_2 as f64;
            let angle = center + f64::from(2.0 * rng.next_f32() - 1.0) * std::f64::consts::FRAC_PI_2;
            let r = (min + (max - min) * next_double(rng).sqrt()) * std::f64::consts::SQRT_2;
            let (x, z) = (-r * angle.sin(), r * angle.cos());
            if x.abs() > max || z.abs() > max {
                continue;
            }
            let y = rng.next_int((2 * v + 1) as u32) as i32 - v;
            let mut p = (self.pos + DVec3::new(x, f64::from(y), z)).floor().as_ivec3();
            if !(crate::world::chunk::WORLD_MIN_Y..=319).contains(&p.y) || !self.stable_destination(world, p) {
                continue;
            }
            // `movePosUpOutOfSolid`, then no water and no malus.
            while p.y <= 319 && world.block(p).is_none_or(|b| b.is_solid()) {
                p.y += 1;
            }
            if world.block(p).is_some_and(|b| b.holds_water() || b.is_water())
                || caps.malus.get(crate::entity::path::static_type(world, p)) != 0.0
            {
                continue;
            }
            let value = self.walk_value(world, ctx, p);
            if value > best.map_or(f32::NEG_INFINITY, |(b, _)| b) {
                best = Some((value, p));
            }
        }
        best.map(|(_, p)| p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5))
    }

    /// `Mob.doHurtTarget` on a player from the shared melee: the attack
    /// damage with the held weapon, the push and attack knockback, Fire
    /// Aspect, and the husk's Hunger, the wither skeleton's Wither and the
    /// cave spider's Poison.
    pub(super) fn hostile_melee_hit(
        &mut self,
        target: &Target,
        dir: DVec3,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        use crate::simulation::effects::Effect;
        self.attack_anim = 0.35;
        let knockback = dir * 6.0 + DVec3::Y * 5.0;
        let (damage, cause) = self.melee_attack();
        let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
        let yaw = (-dir.x).atan2(dir.z).to_degrees() as f32;
        events.push(EntityEvent::PlayerHit {
            java: Some(crate::knockback::Hit::melee(self.pos, target.pos, yaw, extra)),
            player: target.id,
            damage,
            knockback: knockback.as_vec3(),
            cause,
        });
        events.push(EntityEvent::OwnerHurt { player: target.id, attacker: self.uid });
        let fire = self.weapon_fire_seconds();
        if fire > 0.0 {
            events.push(EntityEvent::Ignite { player: target.id, secs: fire });
        }
        let effect = |effect, ticks| EntityEvent::PlayerEffect { player: target.id, effect, amplifier: 0, ticks };
        match self.kind {
            // `Husk.doHurtTarget`: 140 ticks times the whole part of the
            // local effective difficulty.
            MobKind::Husk if self.local_difficulty as u32 > 0 => {
                events.push(effect(Effect::Hunger, 140 * self.local_difficulty as u32));
            }
            MobKind::WitherSkeleton => events.push(effect(Effect::Wither, 200)),
            MobKind::CaveSpider => {
                let ticks = match self.difficulty {
                    crate::simulation::difficulty::Difficulty::Normal => 140,
                    crate::simulation::difficulty::Difficulty::Hard => 300,
                    _ => 0,
                };
                if ticks > 0 {
                    events.push(effect(Effect::Poison, ticks));
                }
            }
            _ => {}
        }
        self.thorns_response(target, knockback, rng, events);
    }
}

/// Java's `RandomSource.nextDouble`: 53 random bits.
fn next_double(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// The zombie goals as the selector's host.
struct ZombieHost<'a, W: MobWorld + ?Sized> {
    m: &'a mut Mob,
    world: &'a W,
    ctx: &'a Ctx,
    t: &'a Target,
    speed: f64,
    rng: &'a mut Rng,
    events: &'a mut Vec<EntityEvent>,
    wish: Option<(Option<DVec3>, f64)>,
}

impl<W: MobWorld + ?Sized> Host for ZombieHost<'_, W> {
    fn can_use(&mut self, goal: usize) -> bool {
        match goal {
            // `SpearUseGoal.canUse`: a target, a kinetic weapon, not using it.
            SPEAR => self.m.holds_spear().is_some() && self.m.spear_use.is_none(),
            ATTACK => self.m.melee_can_use(self.world, self.t, self.speed),
            _ => false,
        }
    }
    fn can_continue(&mut self, goal: usize) -> bool {
        match goal {
            SPEAR => self.m.holds_spear().is_some() && self.m.spear.is_some_and(|s| !s.done),
            ATTACK => self.m.melee_can_continue(self.t),
            _ => false,
        }
    }
    fn start(&mut self, goal: usize) {
        match goal {
            SPEAR => self.m.spear = Some(SpearUse::new()),
            _ => self.m.melee_start(),
        }
    }
    fn stop(&mut self, goal: usize) {
        match goal {
            SPEAR => {
                self.m.spear = None;
                self.m.spear_use = None;
            }
            _ => self.m.melee.running = false,
        }
        self.m.stop_navigation();
    }
    fn tick(&mut self, goal: usize) {
        self.wish = Some(match goal {
            SPEAR => self.m.spear_tick(self.world, self.ctx, self.t, self.rng),
            _ => self.m.melee_tick(self.world, self.t, self.speed, self.rng, self.events),
        });
    }
}

impl Mob {
    /// A strider its zombified piglin steers (v1.0 gap Track 2): its own
    /// `MOVE` goals are off, and the rider's goals walk its navigation with
    /// the strider's malus (`shouldPassengersInheritMalus`), all at speed
    /// modifier 1: `ZombieAttackGoal` toward the rider's target (`hunt`), or
    /// `WaterAvoidingRandomStrollGoal`, one full update in
    /// `reducedTickDelay(120)` to a `LandRandomPos.getPos(10, 7)` spot.
    pub(super) fn steered_strider<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        if self.goals.running.is_some() {
            self.stop_goal(world);
        }
        let full = self.selector_full();
        let speed = self.stroll_speed();
        if let Some(goal) = self.hunt {
            let sees = line_of_sight(world, self.eye_pos(), goal + DVec3::Y);
            if (goal - self.pos).with_y(0.0).length() <= 0.9 {
                return (None, 0.0);
            }
            return (self.pursue(world, goal, speed, sees, true, rng), speed);
        }
        if self.nav.is_done()
            && full
            && rng.next_int(selector::reduced(120) as u32) == 0
            && let Some(p) = self.land_random_pos(world, ctx, rng)
        {
            self.walk_to(world, p, 0, speed, false);
        }
        (self.follow_path(world, speed, false), speed)
    }
}

#[cfg(test)]
mod tests;
