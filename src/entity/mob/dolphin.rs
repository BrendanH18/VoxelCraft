//! Java 26.3 dolphin goals (`Dolphin.registerGoals`) on the flagged goal
//! selector ([`super::selector`]), in registration order:
//!
//! | Priority | Goal | Flags |
//! |---|---|---|
//! | 0 | `BreathAirGoal` (uninterruptible) | move, look |
//! | 1 | `DolphinSwimToTreasureGoal` (uninterruptible) | move, look |
//! | 2 | `DolphinSwimWithPlayerGoal` (4.0) | move, look |
//! | 4 | `RandomSwimmingGoal` (1.0, every 10) | move |
//! | 4 | `RandomLookAroundGoal` | move, look |
//! | 5 | `LookAtPlayerGoal` (6 blocks) | look |
//! | 5 | `DolphinJumpGoal` (every 10, uninterruptible) | move, jump |
//! | 6 | `MeleeAttackGoal` (1.2, follows unseen targets) | move, look |
//! | 7 | `Dolphin.MoveToItemGoal` | move |
//! | 8 | `Dolphin.PlayWithItemsGoal` | none |
//! | 8 | `FollowPlayerRiddenEntityGoal` (boats) | none |
//! | 9 | `AvoidEntityGoal` (guardians, 8 blocks, 1.0) | move |
//!
//! The look goals only turn the head (a dolphin's head turns one degree),
//! so only their hold on the flags is modelled: while one runs, the goals
//! it outranks or ties can't start. `TryFindLiquidGoal` (0, no flags) sets
//! the move control's wanted position, which `SmoothSwimmingMoveControl`
//! ignores once navigation is done and navigation overwrites while a path
//! runs, so it has no effect and is left out. `FollowPlayerRiddenEntityGoal`
//! for nautiluses is left out: VoxelCraft has none.
//!
//! The target selector's `HurtByTargetGoal` (alerting other dolphins) is
//! [`crate::entity::Entities::dolphin_hurt_by`]; `TargetGoal` keeps the
//! target while it is within the 16-block follow range and seen within 150
//! selector updates.
use glam::{DVec3, IVec3};

use super::selector::{Host, JUMP, LOOK, MOVE, Selector, Spec, reduced, spec};
use super::{JavaGoals, Mob, MobKind, line_of_sight};
use crate::entity::{Ctx, Entities, EntityEvent, MobWorld, PlayerId, Rng, Target};
use crate::inventory::Stack;
use crate::item::Item;
use crate::simulation::effects::Effect;
use crate::world::block::Block;

const BREATHE: usize = 0;
const TREASURE: usize = 1;
const SWIM_WITH: usize = 2;
const SWIM: usize = 3;
const LOOK_AROUND: usize = 4;
const LOOK_AT: usize = 5;
const LEAP: usize = 6;
const MELEE: usize = 7;
const FETCH: usize = 8;
const PLAY: usize = 9;
const BOAT: usize = 10;
const AVOID: usize = 11;

const fn locked(priority: u8, flags: u8) -> Spec {
    Spec { priority, flags, interruptable: false, every_tick: false }
}

const SPECS: [Spec; 12] = [
    locked(0, MOVE | LOOK),
    locked(1, MOVE | LOOK),
    spec(2, MOVE | LOOK),
    spec(4, MOVE),
    Spec { priority: 4, flags: MOVE | LOOK, interruptable: true, every_tick: true },
    spec(5, LOOK),
    locked(5, MOVE | JUMP),
    Spec { priority: 6, flags: MOVE | LOOK, interruptable: true, every_tick: true },
    spec(7, MOVE),
    spec(8, 0),
    spec(8, 0),
    spec(9, MOVE),
];

/// Air (seconds) below which `BreathAirGoal` runs: 140 ticks.
const BREATH_AIR: f32 = 7.0;
/// `DolphinSwimToTreasureGoal` needs 100 ticks of air.
const TREASURE_AIR: f32 = 5.0;
/// The `FOLLOW_RANGE` attribute (16, the default).
const FOLLOW_RANGE: f64 = 16.0;
/// `Mob.DEFAULT_ATTACK_REACH`: `sqrt(2.04) - 0.6`.
const ATTACK_REACH: f64 = 0.828_285_686_588_256_1;

/// A dolphin's goals. `got_fish` and `hand` are saved (`GotFish` and the
/// main hand); the rest is transient, as in Java.
#[derive(Clone, Debug, Default)]
pub(crate) struct Goals {
    /// `GOT_FISH`: fed a fish, it leads the way to treasure.
    pub(crate) got_fish: bool,
    /// The main hand (`canPickUpLoot`): an item it plays with.
    pub(crate) hand: Option<Stack>,
    sel: Selector,
    treasure: Option<IVec3>,
    treasure_stuck: bool,
    swim_with: Option<PlayerId>,
    random_look: i32,
    look_at: Option<PlayerId>,
    look_time: i32,
    breached: bool,
    /// `Mob.getTarget` and `TargetGoal.unseenTicks`.
    pub(crate) target: Option<PlayerId>,
    unseen: i32,
    melee_checked: Option<u32>,
    recalc: i32,
    attack_in: i32,
    pathed: Option<DVec3>,
    fetch_cooldown: u32,
    boat: Option<PlayerId>,
    boat_recalc: i32,
    boat_ahead: bool,
    /// The path a random swim or an avoid run found in `canUse`.
    planned: Option<crate::entity::path::Path>,
    /// What the entity pass saw on this tick: the first loose item in
    /// water within 8 blocks, the first boat within 5 blocks with a player
    /// steering it (and whether that player is moving), whether any such
    /// boat's player is moving, the followed player's riding and moving,
    /// and the nearest visible guardian within 8 blocks.
    pub(crate) seen_item: Option<DVec3>,
    pub(crate) seen_boat: Option<PlayerId>,
    pub(crate) boat_moving: bool,
    pub(crate) followed: (bool, bool),
    pub(crate) seen_guardian: Option<DVec3>,
    /// Requests for the entity pass: drop the held item, show the
    /// treasure particles.
    pub(crate) drop_hand: bool,
    pub(crate) happy: bool,
}

/// `Direction.fromYRot`: the cardinal step a Java yaw faces.
fn cardinal(yrot: f32) -> IVec3 {
    match ((yrot / 90.0 + 0.5).floor() as i32).rem_euclid(4) {
        0 => IVec3::Z,
        1 => IVec3::NEG_X,
        2 => IVec3::NEG_Z,
        _ => IVec3::X,
    }
}

/// The cardinal step a player faces, from their look vector.
fn facing(t: &Target) -> IVec3 {
    let yrot = (t.look.z.atan2(t.look.x).to_degrees() as f32) - 90.0;
    cardinal(yrot)
}

struct Dolphin<'a, W: ?Sized> {
    m: &'a mut Mob,
    w: &'a W,
    ctx: &'a Ctx,
    rng: &'a mut Rng,
    events: &'a mut Vec<EntityEvent>,
    g: Goals,
}

impl<W: MobWorld + ?Sized> Dolphin<'_, W> {
    fn player(&self, id: Option<PlayerId>) -> Option<&Target> {
        id.and_then(|id| self.ctx.players.iter().find(|t| t.id == id && t.alive && !t.spectator))
    }

    fn air(&self) -> f32 {
        self.m.aquatic.as_ref().map_or(240.0, |a| a.air)
    }

    /// `PathNavigation.moveTo(x, y, z, speed)`.
    fn move_to(&mut self, cell: IVec3, modifier: f64) -> bool {
        let s = self.m.goal_speed(modifier);
        self.m.walk_to(self.w, cell, 1, s, false)
    }

    /// `BreathAirGoal.findAirPosition`, then `moveTo` the cell above it.
    fn find_air(&mut self) {
        let goal = super::super::aquatic::air_above(self.w, self.m.pos);
        self.move_to(goal + IVec3::Y, 1.0);
    }

    /// `Dolphin.closeToNextPos`: navigation's target within 12 blocks.
    fn close_to_next(&self) -> bool {
        self.m.nav.target().is_some_and(|t| (t.as_dvec3() + DVec3::splat(0.5)).distance_squared(self.m.pos) < 144.0)
    }

    /// The treasure's column at the dolphin's height is within 4 blocks.
    fn at_treasure(&self) -> bool {
        self.g.treasure.is_some_and(|t| {
            let cell = IVec3::new(t.x, self.m.pos.y.floor() as i32, t.z);
            (cell.as_dvec3() + DVec3::splat(0.5)).distance_squared(self.m.pos) < 16.0
        })
    }

    /// `getNearestPlayer(forNonCombat().range(r))`.
    fn nearest_player(&self, r: f64, from: DVec3) -> Option<PlayerId> {
        self.ctx
            .players
            .iter()
            .filter(|t| t.alive && !t.spectator && t.pos.distance_squared(from) <= r * r)
            .min_by(|a, b| a.pos.distance_squared(from).total_cmp(&b.pos.distance_squared(from)))
            .map(|t| t.id)
    }

    /// `Mob.isWithinMeleeAttackRange`: the box grown by the default reach
    /// across meets the target's.
    fn in_reach(&self, t: &Target) -> bool {
        let shape = self.m.shape();
        let d = t.pos - self.m.pos;
        let across = shape.half_width + ATTACK_REACH + t.shape.half_width;
        d.x.abs() < across && d.z.abs() < across && d.y < shape.height && d.y + t.shape.height > 0.0
    }

    fn sees(&self, at: DVec3) -> bool {
        let eye = self.m.pos + DVec3::Y * (self.m.shape().height * 0.85);
        line_of_sight(self.w, eye, at)
    }

    /// `ItemGoal.dropItem`.
    fn drop_item(&mut self) -> bool {
        if self.g.hand.is_some() {
            self.g.drop_hand = true;
            true
        } else {
            false
        }
    }
}

impl<W: MobWorld + ?Sized> Host for Dolphin<'_, W> {
    fn can_use(&mut self, goal: usize) -> bool {
        match goal {
            BREATHE => self.air() < BREATH_AIR,
            TREASURE => self.g.got_fish && self.air() >= TREASURE_AIR,
            SWIM_WITH => {
                self.g.swim_with = self.nearest_player(10.0, self.m.pos);
                let target = self.g.target;
                self.player(self.g.swim_with).is_some_and(|t| t.swimming && Some(t.id) != target)
            }
            SWIM => {
                if self.m.idle >= 5.0 || self.rng.next_int(reduced(10) as u32) != 0 {
                    return false;
                }
                match self.m.swimmable_pos(self.w, self.ctx, self.rng, 10, 7) {
                    Some(p) => {
                        // `start` plans `moveTo(x, y, z)` within a block.
                        let s = self.m.goal_speed(1.0);
                        let (caps, mover) = (self.m.caps(false), self.m.mover(s));
                        self.g.planned = self.m.nav.create_path(self.w, &caps, &mover, p, 1);
                        true
                    }
                    None => false,
                }
            }
            LOOK_AROUND => self.rng.next_f32() < 0.02,
            LOOK_AT => {
                if self.rng.next_f32() >= 0.02 {
                    return false;
                }
                let eye = self.m.pos + DVec3::Y * (self.m.shape().height * 0.85);
                self.g.look_at = self.nearest_player(6.0, eye);
                self.g.look_at.is_some()
            }
            LEAP => self.leap_can_use(),
            MELEE => {
                let now = self.m.goals.ticks;
                if self.g.melee_checked.is_some_and(|t| now.wrapping_sub(t) < 20) {
                    return false;
                }
                self.g.melee_checked = Some(now);
                let Some(t) = self.player(self.g.target).copied() else { return false };
                let s = self.m.goal_speed(1.2);
                let caps = self.m.caps(true);
                let mover = self.m.mover(s);
                self.m.nav.create_path(self.w, &caps, &mover, t.pos.floor().as_ivec3(), 0).is_some()
                    || self.in_reach(&t)
            }
            FETCH => self.g.fetch_cooldown <= self.m.goals.ticks && self.g.seen_item.is_some(),
            PLAY => self.g.hand.is_some(),
            BOAT => {
                let (_, moved) = self.g.followed;
                self.g.boat.is_some() && moved || self.g.boat_moving
            }
            AVOID => {
                let Some(from) = self.g.seen_guardian else { return false };
                let Some(away) = self.m.pos_away(self.w, self.ctx, self.rng, 16, 7, from) else { return false };
                let target = away.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                if from.distance_squared(target) < from.distance_squared(self.m.pos) {
                    return false;
                }
                let s = self.m.goal_speed(1.0);
                let caps = self.m.caps(false);
                let mover = self.m.mover(s);
                self.g.planned = self.m.nav.create_path(self.w, &caps, &mover, away, 0);
                self.g.planned.is_some()
            }
            _ => false,
        }
    }

    fn can_continue(&mut self, goal: usize) -> bool {
        match goal {
            BREATHE => self.air() < BREATH_AIR,
            TREASURE => {
                self.g.treasure.is_some() && !self.at_treasure() && !self.g.treasure_stuck && self.air() >= TREASURE_AIR
            }
            SWIM_WITH => {
                let pos = self.m.pos;
                self.player(self.g.swim_with).is_some_and(|t| t.swimming && t.pos.distance_squared(pos) < 256.0)
            }
            SWIM | AVOID => !self.m.nav.is_done(),
            LOOK_AROUND => self.g.random_look >= 0,
            LOOK_AT => {
                let pos = self.m.pos;
                self.g.look_time > 0 && self.player(self.g.look_at).is_some_and(|t| t.pos.distance_squared(pos) <= 36.0)
            }
            LEAP => {
                let yd = self.m.vel.y * super::swim::TICK;
                let pitch = self.m.move_ctl.pitch;
                (yd * yd >= 0.03 || pitch == 0.0 || pitch.abs() >= 10.0 || !self.m.in_water) && !self.m.on_ground
            }
            // `followingTargetEvenIfNotSeen`: any living target in survival.
            MELEE => {
                self.player(self.g.target).is_some_and(|t| t.targetable && self.m.within_home(t.pos.floor().as_ivec3()))
            }
            FETCH => self.can_use(FETCH),
            PLAY => self.g.hand.is_some(),
            BOAT => {
                let (riding, moved) = self.g.followed;
                self.g.boat.is_some() && riding && moved
            }
            _ => false,
        }
    }

    fn start(&mut self, goal: usize) {
        match goal {
            BREATHE => {
                self.m.stop_navigation();
                self.find_air();
            }
            TREASURE => {
                self.g.treasure_stuck = false;
                self.m.stop_navigation();
                match self.w.dolphin_treasure(self.m.pos.floor().as_ivec3()) {
                    Some(p) => {
                        self.g.treasure = Some(p);
                        self.g.happy = true;
                    }
                    None => self.g.treasure_stuck = true,
                }
            }
            SWIM_WITH => {
                if let Some(id) = self.g.swim_with {
                    self.events.push(EntityEvent::PlayerEffect {
                        player: id,
                        effect: Effect::DolphinsGrace,
                        amplifier: 0,
                        ticks: 100,
                    });
                }
            }
            SWIM | AVOID => {
                let mover = self.m.mover(self.m.goal_speed(1.0));
                let path = self.g.planned.take();
                self.m.nav.move_to(self.w, path, &mover);
            }
            LOOK_AROUND => {
                self.rng.next_f32();
                self.g.random_look = 20 + self.rng.next_int(20) as i32;
            }
            LOOK_AT => self.g.look_time = reduced(40 + self.rng.next_int(40) as i32),
            LEAP => {
                let d = cardinal(self.m.yrot()).as_dvec3();
                self.m.vel += DVec3::new(d.x * 0.6, 0.7, d.z * 0.6) / super::swim::TICK;
                self.m.stop_navigation();
                self.g.breached = false;
            }
            MELEE => {
                if let Some(t) = self.player(self.g.target).copied() {
                    self.m.walk_to(self.w, t.pos.floor().as_ivec3(), 0, self.m.goal_speed(1.2), true);
                }
                self.g.recalc = 0;
                self.g.attack_in = 0;
                self.g.pathed = None;
            }
            FETCH => {
                if let Some(at) = self.g.seen_item {
                    self.move_to(at.floor().as_ivec3(), 1.2);
                }
                self.g.fetch_cooldown = 0;
            }
            BOAT => {
                self.g.boat = self.g.seen_boat.or(self.g.boat);
                self.g.boat_recalc = 0;
                self.g.boat_ahead = false;
            }
            _ => {}
        }
    }

    fn stop(&mut self, goal: usize) {
        match goal {
            TREASURE => {
                if self.g.treasure.is_none() || self.at_treasure() || self.g.treasure_stuck {
                    self.g.got_fish = false;
                }
            }
            SWIM_WITH => {
                self.g.swim_with = None;
                self.m.stop_navigation();
            }
            SWIM => self.m.stop_navigation(),
            LEAP => self.m.move_ctl.pitch = 0.0,
            MELEE => {
                if !self.player(self.g.target).is_some_and(|t| t.targetable) {
                    self.g.target = None;
                }
                self.m.stop_navigation();
            }
            FETCH => {
                if self.drop_item() {
                    self.g.fetch_cooldown = self.m.goals.ticks + self.rng.next_int(100);
                }
            }
            PLAY => {
                self.drop_item();
            }
            BOAT => self.g.boat = None,
            _ => {}
        }
    }

    fn tick(&mut self, goal: usize) {
        match goal {
            BREATHE => {
                self.find_air();
                self.m.move_ctl.strokes.push(0.02);
            }
            TREASURE => self.treasure_tick(),
            SWIM_WITH => {
                let Some(t) = self.player(self.g.swim_with).copied() else { return };
                if t.pos.distance_squared(self.m.pos) < 6.25 {
                    self.m.stop_navigation();
                } else {
                    self.move_to(t.pos.floor().as_ivec3(), 4.0);
                }
                if t.swimming && self.rng.next_int(6) == 0 {
                    self.events.push(EntityEvent::PlayerEffect {
                        player: t.id,
                        effect: Effect::DolphinsGrace,
                        amplifier: 0,
                        ticks: 100,
                    });
                }
            }
            LOOK_AROUND => self.g.random_look -= 1,
            LOOK_AT => self.g.look_time -= 1,
            LEAP => self.leap_tick(),
            MELEE => self.melee_tick(),
            FETCH => {
                if let Some(at) = self.g.seen_item {
                    self.move_to(at.floor().as_ivec3(), 1.2);
                }
            }
            PLAY => {
                self.drop_item();
            }
            BOAT => self.boat_tick(),
            _ => {}
        }
    }
}

impl<W: MobWorld + ?Sized> Dolphin<'_, W> {
    /// `DolphinJumpGoal.canUse`: one in five updates, clear water ahead at
    /// steps 0, 1 and 4 to 7 along the facing, with two cells of air above.
    fn leap_can_use(&mut self) -> bool {
        if self.rng.next_int(reduced(10) as u32) != 0 {
            return false;
        }
        let step = cardinal(self.m.yrot());
        let base = self.m.pos.floor().as_ivec3();
        [0, 1, 4, 5, 6, 7].iter().all(|&i| {
            let p = base + step * i;
            self.w.block(p).is_some_and(Block::holds_water)
                && self.w.block(p + IVec3::Y) == Some(Block::AIR)
                && self.w.block(p + IVec3::Y * 2) == Some(Block::AIR)
        })
    }

    /// `DolphinJumpGoal.tick`: note the breach, then pitch the body along
    /// the motion, easing it back to level near the top of the arc.
    fn leap_tick(&mut self) {
        if !self.g.breached {
            self.g.breached = self.w.block(self.m.pos.floor().as_ivec3()).is_some_and(Block::holds_water);
        }
        let dm = self.m.vel * super::swim::TICK;
        let pitch = &mut self.m.move_ctl.pitch;
        if dm.y * dm.y < 0.03 && *pitch != 0.0 {
            *pitch += super::swim::wrap_degrees(-*pitch) * 0.2;
        } else if dm.length() > 1e-5 {
            *pitch = ((-dm.y).atan2(dm.x.hypot(dm.z)).to_degrees()) as f32;
        }
    }

    /// `DolphinSwimToTreasureGoal.tick`: a leg toward the treasure whenever
    /// navigation is done or its target is within 12 blocks.
    fn treasure_tick(&mut self) {
        let Some(treasure) = self.g.treasure else { return };
        if !self.close_to_next() && !self.m.nav.is_done() {
            return;
        }
        let at = treasure.as_dvec3() + DVec3::splat(0.5);
        let (w, ctx) = (self.w, self.ctx);
        let mut next = self
            .m
            .pos_towards(w, ctx, self.rng, 16, 1, at, std::f64::consts::PI / 8.0)
            .or_else(|| self.m.pos_towards(w, ctx, self.rng, 8, 4, at, std::f64::consts::FRAC_PI_2));
        // Water that a swimmer can path through.
        if next.is_some_and(|p| !w.block(p).is_some_and(|b| b.holds_water() && (b.is_water() || !b.is_solid()))) {
            next = self.m.pos_towards(w, ctx, self.rng, 8, 5, at, std::f64::consts::FRAC_PI_2);
        }
        let Some(next) = next else {
            self.g.treasure_stuck = true;
            return;
        };
        self.move_to(next, 1.3);
        if self.rng.next_int(40) == 0 {
            self.g.happy = true;
        }
    }

    /// `MeleeAttackGoal.tick` (every tick).
    fn melee_tick(&mut self) {
        let Some(t) = self.player(self.g.target).copied() else { return };
        self.g.recalc = (self.g.recalc - 1).max(0);
        if self.g.recalc <= 0
            && self.g.pathed.is_none_or(|p| t.pos.distance_squared(p) >= 1.0 || self.rng.next_f32() < 0.05)
        {
            self.g.pathed = Some(t.pos);
            self.g.recalc = 4 + self.rng.next_int(7) as i32;
            let d2 = self.m.pos.distance_squared(t.pos);
            if d2 > 1024.0 {
                self.g.recalc += 10;
            } else if d2 > 256.0 {
                self.g.recalc += 5;
            }
            let s = self.m.goal_speed(1.2);
            if !self.m.walk_to(self.w, t.pos.floor().as_ivec3(), 0, s, true) {
                self.g.recalc += 15;
            }
        }
        self.g.attack_in = (self.g.attack_in - 1).max(0);
        if self.g.attack_in <= 0 && self.in_reach(&t) && self.sees(t.pos + DVec3::Y * t.shape.height * 0.85) {
            self.g.attack_in = 20;
            self.m.attack_anim = 0.3;
            let flat = (t.pos - self.m.pos).with_y(0.0);
            let yaw = (-flat.x).atan2(flat.z).to_degrees() as f32;
            let extra = crate::knockback::attack_strength(self.m.attack_knockback(), 0, false);
            self.events.push(EntityEvent::PlayerHit {
                java: Some(crate::knockback::Hit::melee(self.m.pos, t.pos, yaw, extra)),
                player: t.id,
                damage: 3.0,
                knockback: (flat.normalize_or(DVec3::X) * 4.0 + DVec3::Y * 3.0).as_vec3(),
                cause: "was slain by a dolphin",
            });
        }
    }

    /// `FollowPlayerRiddenEntityGoal.tick`: an extra stroke, then every 5
    /// updates a path to the cell behind the player, or once within 4
    /// blocks 10 blocks ahead of them, until it falls 12 behind.
    fn boat_tick(&mut self) {
        let ahead = self.g.boat_ahead;
        self.m.move_ctl.strokes.push(if ahead { 0.01 } else { 0.015 });
        let Some(t) = self.player(self.g.boat).copied() else { return };
        self.g.boat_recalc -= 1;
        if self.g.boat_recalc > 0 {
            return;
        }
        self.g.boat_recalc = reduced(10);
        let cell = t.pos.floor().as_ivec3();
        let distance = self.m.pos.distance(t.pos);
        if !ahead {
            self.move_to(cell - facing(&t) - IVec3::Y, 1.0);
            if distance < 4.0 {
                self.g.boat_recalc = 0;
                self.g.boat_ahead = true;
            }
        } else {
            self.move_to(cell + facing(&t) * 10 - IVec3::Y, 1.0);
            if distance > 12.0 {
                self.g.boat_recalc = 0;
                self.g.boat_ahead = false;
            }
        }
    }
}

impl Mob {
    pub(crate) fn dolphin(&self) -> Option<&Goals> {
        match self.java_goals.as_deref() {
            Some(JavaGoals::Dolphin(d)) => Some(d),
            _ => None,
        }
    }

    pub(crate) fn dolphin_mut(&mut self) -> Option<&mut Goals> {
        match self.java_goals.as_deref_mut() {
            Some(JavaGoals::Dolphin(d)) => Some(d),
            _ => None,
        }
    }

    /// One tick of the dolphin's goals; `full` is the selector's full
    /// update. Returns the movement along whatever path the goals planned.
    pub(crate) fn dolphin_steer<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
        full: bool,
    ) -> (Option<DVec3>, f64) {
        let Some(g) = self.dolphin_mut().map(std::mem::take) else { return (None, 0.0) };
        let mut sel = g.sel;
        let mut host = Dolphin { m: self, w, ctx, rng, events, g };
        if full {
            host.target_tick();
        }
        sel.tick(&SPECS, full, &mut host);
        let mut g = host.g;
        g.sel = sel;
        if let Some(d) = self.dolphin_mut() {
            *d = g;
        }
        let s = self.nav.speed;
        (self.follow_path(w, s, false), s)
    }

    /// Whether the dolphin has an attack target (`travelInWater` sinks only
    /// without one).
    pub(crate) fn dolphin_has_target(&self) -> bool {
        self.dolphin().is_some_and(|d| d.target.is_some())
    }
}

impl<W: MobWorld + ?Sized> Dolphin<'_, W> {
    /// `TargetGoal.canContinueToUse` for the hurt-by target: alive, in
    /// survival, within the follow range and home, and seen within
    /// `reducedTickDelay(300)` updates.
    fn target_tick(&mut self) {
        let Some(t) = self.player(self.g.target).copied() else {
            self.g.target = None;
            return;
        };
        let keep = t.targetable
            && t.pos.distance_squared(self.m.pos) <= FOLLOW_RANGE * FOLLOW_RANGE
            && self.m.within_home(t.pos.floor().as_ivec3());
        if keep && self.sees(t.pos + DVec3::Y * t.shape.height * 0.85) {
            self.g.unseen = 0;
        } else if keep {
            self.g.unseen += 1;
        }
        if !keep || self.g.unseen > reduced(300) {
            self.g.target = None;
        }
    }
}

/// `ItemTags.FISHES`, which dolphins take from players.
pub(crate) fn is_fish(item: Item) -> bool {
    matches!(
        item,
        Item::COD | Item::COOKED_COD | Item::SALMON | Item::COOKED_SALMON | Item::PUFFERFISH | Item::TROPICAL_FISH
    )
}

impl Entities {
    /// `Dolphin.mobInteract`: a fish makes the dolphin lead to treasure.
    /// `Some(true)` consumes the fish. (VoxelCraft has no dolphin eating
    /// sound yet.)
    pub(crate) fn feed_dolphin(&mut self, index: usize, held: Option<Stack>) -> Option<bool> {
        let m = self.mobs.get_mut(index)?;
        if m.kind != MobKind::Dolphin || !m.alive() || !held.is_some_and(|s| is_fish(s.item)) {
            return None;
        }
        m.dolphin_mut()?.got_fish = true;
        Some(true)
    }

    /// `HurtByTargetGoal(Guardian).setAlertOthers()`: a player who hurts an
    /// adult dolphin becomes its target and that of every dolphin without
    /// one in its follow range (10 up and down).
    pub(crate) fn dolphin_hurt_by(&mut self, index: usize, player: PlayerId) {
        let Some(m) = self.mobs.get(index) else { return };
        if m.kind != MobKind::Dolphin || m.baby || !m.alive() {
            return;
        }
        let pos = m.pos;
        for (j, other) in self.mobs.iter_mut().enumerate() {
            let near = || {
                let d = other.pos - pos;
                (-FOLLOW_RANGE..1.0 + FOLLOW_RANGE).contains(&d.x)
                    && (-FOLLOW_RANGE..1.0 + FOLLOW_RANGE).contains(&d.z)
                    && (-10.0..11.0).contains(&d.y)
            };
            if other.kind != MobKind::Dolphin || !other.alive() || j != index && !near() {
                continue;
            }
            if let Some(d) = other.dolphin_mut()
                && (j == index || d.target.is_none())
            {
                d.target = Some(player);
                d.unseen = 0;
            }
        }
    }

    /// The dolphins' entity searches before mobs think, on each one's full
    /// selector update: items to fetch, boats to follow, guardians to avoid.
    pub(crate) fn sense_dolphins<W: MobWorld + ?Sized>(&mut self, w: &W, moved: &[(PlayerId, bool)]) {
        if !self.mobs.iter().any(|m| m.kind == MobKind::Dolphin) {
            return;
        }
        let riding = |id: PlayerId| {
            self.minecarts.iter().any(|c| c.rider == Some(id) || c.second_rider == Some(id))
                || self.mobs.iter().any(|m| m.mount.as_ref().is_some_and(|s| s.rider == Some(id)))
        };
        let moving = |id: PlayerId| moved.iter().any(|&(p, m)| p == id && m);
        let mut seen = Vec::new();
        for (i, m) in self.mobs.iter().enumerate() {
            // Goals read these only on the selector's full update.
            if m.kind != MobKind::Dolphin || !m.alive() || !m.simulation_active || !m.goals.full(m.uid) {
                continue;
            }
            let shape = m.shape();
            let inside = |p: DVec3, r: f64| {
                let d = p - m.pos;
                d.x.abs() <= shape.half_width + r
                    && d.z.abs() <= shape.half_width + r
                    && d.y >= -r
                    && d.y <= shape.height + r
            };
            let item = self
                .items
                .iter()
                .find(|it| {
                    it.pickup_delay <= 0.0
                        && inside(it.pos, 8.0)
                        && w.block(it.pos.floor().as_ivec3()).is_some_and(Block::holds_water)
                })
                .map(|it| it.pos);
            let steered = |c: &&super::super::minecart::Minecart| c.kind.boat().is_some() && inside(c.pos, 5.0);
            let boats: Vec<PlayerId> = self.minecarts.iter().filter(steered).filter_map(|c| c.rider).collect();
            let followed = m.dolphin().and_then(|d| d.boat).map_or((false, false), |id| (riding(id), moving(id)));
            let eye = m.pos + DVec3::Y * (shape.height * 0.85);
            // `getNearestEntity` with `TargetingConditions`: the nearest of
            // the guardians it can see.
            let mut nearest: Option<(f64, usize)> = None;
            self.mob_index.visit(m.pos, 11.0, |j| {
                let g = &self.mobs[j];
                let d2 = g.pos.distance_squared(m.pos);
                if matches!(g.kind, MobKind::Guardian | MobKind::ElderGuardian)
                    && g.alive()
                    && inside(g.pos, 8.0)
                    && d2 <= 64.0
                    && nearest.is_none_or(|(bd, bj)| d2 < bd || d2 == bd && j < bj)
                    && line_of_sight(w, eye, g.pos + DVec3::Y * (g.shape().height * 0.85))
                {
                    nearest = Some((d2, j));
                }
            });
            let guardian = nearest.map(|(_, j)| self.mobs[j].pos);
            seen.push((i, item, boats.first().copied(), boats.iter().any(|&id| moving(id)), followed, guardian));
        }
        for (i, item, boat, boat_moving, followed, guardian) in seen {
            if let Some(d) = self.mobs[i].dolphin_mut() {
                d.seen_item = item;
                d.seen_boat = boat;
                d.boat_moving = boat_moving;
                d.followed = followed;
                d.seen_guardian = guardian;
            }
        }
    }

    /// After mobs move: dolphins throw what they play with (`ItemGoal.drop`,
    /// 40 ticks before anyone may pick it up), pick up loose items with an
    /// empty hand under mobGriefing (`Mob.aiStep`), and show the treasure
    /// goal's happy particles.
    pub(crate) fn dolphin_items(&mut self) {
        for i in 0..self.mobs.len() {
            let m = &mut self.mobs[i];
            if m.kind != MobKind::Dolphin || !m.simulation_active {
                continue;
            }
            let (pos, yrot, pitch, shape, alive) = (m.pos, m.yrot(), m.move_ctl.pitch, m.shape(), m.alive());
            let Some(d) = m.dolphin_mut() else { continue };
            if std::mem::take(&mut d.happy) {
                let mut b = crate::particles::Burst::new(
                    crate::particles::Kind::Effect,
                    pos + DVec3::Y * (shape.height * 0.5 + 0.2),
                    7,
                );
                b.color = Some([0.35, 0.85, 0.3, 1.0]);
                b.spread = DVec3::new(shape.half_width * 2.0, shape.height * 0.5, shape.half_width * 2.0);
                self.particles.push(crate::particles::Request::Burst(b));
            }
            if std::mem::take(&mut d.drop_hand)
                && let Some(stack) = d.hand.take()
            {
                let (ys, yc) = yrot.to_radians().sin_cos();
                let (ps, pc) = pitch.to_radians().sin_cos();
                let dir = self.rng.next_f32() * std::f32::consts::TAU;
                let pow = 0.02 * self.rng.next_f32();
                let vel = DVec3::new(
                    (0.3 * -ys * pc + dir.cos() * pow) as f64,
                    (0.3 * ps * 1.5) as f64,
                    (0.3 * yc * pc + dir.sin() * pow) as f64,
                ) / super::swim::TICK;
                let at = pos + DVec3::Y * (shape.height * 0.85 - 0.3);
                self.items.push(crate::entity::ItemEntity::new(stack, at, vel, 2.0, &mut self.rng));
            }
            let d = self.mobs[i].dolphin().unwrap();
            if !alive || d.hand.is_some() || !self.villager_griefing {
                continue;
            }
            let inside = |p: DVec3| {
                let dd = p - pos;
                dd.x.abs() <= shape.half_width + 1.0
                    && dd.z.abs() <= shape.half_width + 1.0
                    && dd.y > -0.25
                    && dd.y < shape.height
            };
            if let Some(j) = self.items.iter().position(|it| it.pickup_delay <= 0.0 && inside(it.pos)) {
                let it = self.items.swap_remove(j);
                self.mobs[i].dolphin_mut().unwrap().hand = Some(it.stack);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::minecart::{CartKind, Minecart};
    use crate::physics::{BlockSource, test_util::Grid};
    use crate::world::terrain::Dimension;

    /// A walled sea on a stone floor: water from `y = 8 - depth + 1` to 8
    /// over x -40..=90 and z -20..=20, air above, and a treasure.
    struct Sea {
        grid: Grid,
        treasure: Option<IVec3>,
    }
    impl BlockSource for Sea {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.grid.block(p)
        }
    }
    impl MobWorld for Sea {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(8)
        }
        fn exposed(&self, p: IVec3) -> bool {
            p.y > 8
        }
        fn dolphin_treasure(&self, _: IVec3) -> Option<IVec3> {
            self.treasure
        }
    }
    fn sea(depth: i32) -> Sea {
        let floor = 8 - depth;
        let mut grid = Grid::flat(floor + 1);
        for x in -41..=91 {
            for z in -21..=21 {
                for y in floor..=12 {
                    let wall = x == -41 || x == 91 || z == -21 || z == 21 || y == floor;
                    let b = if wall {
                        Block::STONE
                    } else if y <= 8 {
                        Block::WATER
                    } else {
                        Block::AIR
                    };
                    grid.set(IVec3::new(x, y, z), b);
                }
            }
        }
        Sea { grid, treasure: None }
    }
    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players,
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    fn swimmer(at: DVec3, swimming: bool) -> Target {
        let mut t = Target::new(PlayerId(1), at, true);
        t.swimming = swimming;
        t.in_water = true;
        t.shape = crate::physics::Shape::new(0.3, 0.6);
        t
    }
    fn dolphins(at: &[DVec3]) -> Entities {
        let mut e = Entities::new(11);
        for &p in at {
            e.spawn(MobKind::Dolphin, p);
        }
        e.ensure_mob_ids();
        e
    }
    fn running(e: &Entities, i: usize, goal: usize) -> bool {
        e.mobs[i].dolphin().unwrap().sel.running(goal)
    }

    /// `DolphinSwimWithPlayerGoal`: the nearest player within 10 blocks,
    /// if sprint-swimming, gets Dolphin's Grace when the goal starts and on
    /// one update in six, and the dolphin swims to within 2.5 blocks of
    /// them. A player who isn't swimming gets nothing.
    #[test]
    fn dolphins_swim_with_swimming_players_and_grant_their_grace() {
        let w = sea(10);
        let graces = |swimming: bool| {
            let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
            let c = ctx(vec![swimmer(DVec3::new(8.5, 3.0, 0.5), swimming)]);
            let mut count = 0;
            let mut closest = f64::MAX;
            for _ in 0..200 {
                let events = e.update(0.05, &w, &c);
                count += events
                    .iter()
                    .filter(|ev| {
                        matches!(ev, EntityEvent::PlayerEffect { effect: Effect::DolphinsGrace, ticks: 100, .. })
                    })
                    .count();
                closest = closest.min(e.mobs[0].pos.distance(c.players[0].pos));
            }
            (count, closest)
        };
        let (count, closest) = graces(true);
        // One on start, then about one in six of 100 updates.
        assert!((6..=40).contains(&count), "{count} graces");
        assert!(closest < 3.0, "came to {closest}");
        assert_eq!(graces(false).0, 0);
    }

    /// `Dolphin.mobInteract` and `DolphinSwimToTreasureGoal`: a fed dolphin
    /// swims to the nearest ocean ruin or shipwreck and forgets the fish
    /// within 4 blocks of its column; with none to find it forgets at once.
    #[test]
    fn fed_dolphins_lead_to_treasure() {
        let mut w = sea(10);
        w.treasure = Some(IVec3::new(70, 0, 0));
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
        assert_eq!(e.feed_dolphin(0, Some(Stack::new(Item::STRING, 1))), None);
        assert_eq!(e.feed_dolphin(0, Some(Stack::new(Item::COOKED_COD, 1))), Some(true));
        let c = ctx(vec![]);
        let mut ticks = 0;
        while e.mobs[0].dolphin().unwrap().got_fish && ticks < 2000 {
            e.update(0.05, &w, &c);
            ticks += 1;
        }
        let at = e.mobs[0].pos;
        assert!(!e.mobs[0].dolphin().unwrap().got_fish, "still searching at {at} after {ticks} ticks");
        assert!(DVec3::new(70.5, at.y, 0.5).distance(at) < 4.5, "stopped at {at}");

        w.treasure = None;
        e.feed_dolphin(0, Some(Stack::new(Item::SALMON, 1)));
        for _ in 0..4 {
            e.update(0.05, &w, &c);
        }
        assert!(!e.mobs[0].dolphin().unwrap().got_fish, "no treasure: the goal is stuck and ends");
    }

    /// `HurtByTargetGoal(...).setAlertOthers()` and `MeleeAttackGoal(1.2)`:
    /// a hit turns the dolphin and its neighbours without a target on a
    /// survival player; they bite for 3. A player in creative is dropped.
    #[test]
    fn hurt_dolphins_alert_their_pod_and_bite() {
        let w = sea(10);
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5), DVec3::new(6.5, 3.0, 4.5), DVec3::new(30.5, 3.0, 0.5)]);
        let c = ctx(vec![swimmer(DVec3::new(3.5, 3.0, 0.5), false)]);
        e.hurt_by_player_target(0, PlayerId(1));
        e.resolve_hurt_by(&c);
        let targets: Vec<_> = e.mobs.iter().map(|m| m.dolphin().unwrap().target).collect();
        assert_eq!(targets, vec![Some(PlayerId(1)), Some(PlayerId(1)), None], "only within the 16-block follow range");
        let (mut bites, mut attacked) = (0, [false; 2]);
        // Random swims (4) outrank the attack (6) and take its move flag, so
        // an angry pod bites in short bouts between swims.
        for _ in 0..400 {
            bites += e
                .update(0.05, &w, &c)
                .iter()
                .filter(|ev| matches!(ev, EntityEvent::PlayerHit { damage: 3.0, .. }))
                .count();
            for (i, a) in attacked.iter_mut().enumerate() {
                *a |= running(&e, i, MELEE);
            }
        }
        assert!(bites >= 1 && attacked == [true; 2], "{bites} bites, {attacked:?}");
        let mut creative = swimmer(DVec3::new(3.5, 3.0, 0.5), false);
        creative.targetable = false;
        let c = ctx(vec![creative]);
        for _ in 0..3 {
            e.update(0.05, &w, &c);
        }
        assert!(e.mobs.iter().all(|m| m.dolphin().unwrap().target.is_none()));
    }

    /// `HurtByTargetGoal.canUse` refuses a creative attacker, judged when
    /// the goal next runs: a hit from a player who has since switched to
    /// creative keeps the dolphin's target and alerts nobody, and one from a
    /// player who has switched to survival counts.
    #[test]
    fn creative_hits_leave_a_dolphins_target_alone() {
        let w = sea(10);
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5), DVec3::new(4.5, 3.0, 0.5)]);
        let b = |survival: bool| {
            let mut t = Target::new(PlayerId(2), DVec3::new(0.5, 3.0, 8.5), survival);
            t.in_water = true;
            t
        };
        let c = ctx(vec![swimmer(DVec3::new(3.5, 3.0, 0.5), false), b(true)]);
        e.hurt_by_player_target(0, PlayerId(1));
        e.update(0.05, &w, &c);
        e.mobs[1].dolphin_mut().unwrap().target = None;
        // B hits while in survival, then switches to creative before the
        // dolphins' next update.
        e.hurt_by_player_target(0, PlayerId(2));
        e.update(0.05, &w, &ctx(vec![swimmer(DVec3::new(3.5, 3.0, 0.5), false), b(false)]));
        assert_eq!(e.mobs[0].dolphin().unwrap().target, Some(PlayerId(1)));
        assert_eq!(e.mobs[1].dolphin().unwrap().target, None);
        // The other way round, the hit counts.
        e.hurt_by_player_target(0, PlayerId(2));
        e.update(0.05, &w, &c);
        assert_eq!(e.mobs[0].dolphin().unwrap().target, Some(PlayerId(2)));
    }

    /// `AvoidEntityGoal` takes the nearest guardian it can see: one behind
    /// a wall doesn't hide a farther one in plain view.
    #[test]
    fn dolphins_avoid_the_nearest_visible_guardian() {
        let mut w = sea(10);
        for y in 0..=8 {
            for z in -3..=3 {
                w.grid.set(IVec3::new(2, y, z), Block::STONE);
            }
        }
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
        e.spawn(MobKind::Guardian, DVec3::new(3.5, 3.0, 0.5));
        e.spawn(MobKind::Guardian, DVec3::new(-5.5, 3.0, 0.5));
        e.ensure_mob_ids();
        e.mob_index.rebuild(&e.mobs);
        e.sense_dolphins(&w, &[]);
        assert_eq!(e.mobs[0].dolphin().unwrap().seen_guardian, Some(DVec3::new(-5.5, 3.0, 0.5)));
    }

    /// `MoveToItemGoal`, `Mob.aiStep`'s pickup and `PlayWithItemsGoal`: a
    /// dolphin heads for a loose item in the water, picks up what it
    /// touches, and throws it on with a 40-tick pickup delay.
    #[test]
    fn dolphins_fetch_and_toss_items() {
        let w = sea(10);
        let item = |at: DVec3| {
            crate::entity::ItemEntity::new(Stack::new(Item::STRING, 1), at, DVec3::ZERO, 0.0, &mut Rng::new(1))
        };
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
        e.items.push(item(DVec3::new(5.5, 3.0, 0.5)));
        e.update(0.05, &w, &ctx(vec![]));
        assert!(running(&e, 0, FETCH));
        assert!(e.mobs[0].nav.target().is_some_and(|t| t.as_dvec3().distance(DVec3::new(5.0, 3.0, 0.0)) < 1.5));
        // One it touches it takes, then throws on the next full update.
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
        e.items.push(item(DVec3::new(0.5, 3.2, 0.5)));
        e.update(0.05, &w, &ctx(vec![]));
        assert_eq!(e.mobs[0].dolphin().unwrap().hand, Some(Stack::new(Item::STRING, 1)));
        assert!(e.items.is_empty());
        let mut tossed = None;
        for _ in 0..3 {
            e.update(0.05, &w, &ctx(vec![]));
            tossed = tossed.or(e.items.first().map(|it| (it.pickup_delay, it.vel)));
        }
        let (delay, vel) = tossed.expect("thrown");
        assert!(delay > 1.8 && vel.length() > 3.0, "{delay} {vel}");
        assert!(e.mobs[0].dolphin().unwrap().hand.is_none());
    }

    /// `DolphinJumpGoal`: at the surface, facing open water with air above,
    /// a dolphin leaps out (0.7 up and 0.6 along its facing) and pitches
    /// with the arc.
    #[test]
    fn dolphins_leap_from_the_surface() {
        let w = sea(1);
        let mut e = dolphins(&[DVec3::new(0.5, 8.0, 0.5)]);
        let c = ctx(vec![]);
        let mut top: f64 = 0.0;
        let mut leaps = 0;
        for _ in 0..1200 {
            e.update(0.05, &w, &c);
            top = top.max(e.mobs[0].pos.y);
            leaps += usize::from(running(&e, 0, LEAP));
        }
        assert!(leaps > 0 && top > 9.0, "top {top}");
    }

    /// `AvoidEntityGoal(Guardian, 8, 1.0, 1.0)`: a dolphin that sees a
    /// guardian within 8 blocks swims away from it.
    #[test]
    fn dolphins_swim_away_from_guardians() {
        let w = sea(10);
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
        e.spawn(MobKind::Guardian, DVec3::new(4.5, 3.0, 0.5));
        e.ensure_mob_ids();
        let c = ctx(vec![]);
        let mut avoided = false;
        for _ in 0..200 {
            e.update(0.05, &w, &c);
            e.mobs[1].pos = DVec3::new(4.5, 3.0, 0.5);
            avoided |= running(&e, 0, AVOID);
        }
        assert!(avoided);
    }

    /// `FollowPlayerRiddenEntityGoal`: a dolphin within 5 blocks of a boat
    /// a moving player steers follows it.
    #[test]
    fn dolphins_follow_a_moving_boat() {
        let w = sea(10);
        let mut e = dolphins(&[DVec3::new(0.5, 6.0, 2.5)]);
        let mut boat = Minecart::new(1, CartKind::Boat { wood: 0, chest: false }, DVec3::new(0.5, 8.6, 0.5));
        boat.rider = Some(PlayerId(1));
        e.minecarts.push(boat);
        let mut followed = false;
        let mut player = swimmer(DVec3::new(0.5, 8.6, 0.5), false);
        player.look = DVec3::X;
        for t in 0..300 {
            player.pos.x = 0.5 + t as f64 * 0.15;
            e.minecarts[0].pos = player.pos;
            e.minecarts[0].previous_pos = player.pos;
            e.update(0.05, &w, &ctx(vec![player]));
            followed |= running(&e, 0, BOAT);
        }
        let gap = e.mobs[0].pos.with_y(0.0).distance(player.pos.with_y(0.0));
        assert!(followed && gap < 14.0, "gap {gap}");
    }

    /// `GotFish`, the main hand and `homePosition` save with the mob.
    #[test]
    fn dolphin_and_home_state_round_trips_through_saves() {
        let mut e = dolphins(&[DVec3::new(0.5, 3.0, 0.5)]);
        e.mobs[0].persistent = true;
        e.mobs[0].restrict = Some((IVec3::new(4, 5, 6), 16));
        let d = e.mobs[0].dolphin_mut().unwrap();
        d.got_fish = true;
        d.hand = Some(Stack::new(Item::STRING, 3));
        let text = e.nether_mobs_to_string();
        let mut loaded = Entities::new(2);
        loaded.load_nether_mobs(&text);
        let m = &loaded.mobs[0];
        let d = m.dolphin().unwrap();
        assert!(d.got_fish);
        assert_eq!(d.hand, Some(Stack::new(Item::STRING, 3)));
        assert_eq!(m.restrict, Some((IVec3::new(4, 5, 6), 16)));
    }

    #[test]
    fn cardinal_matches_javas_direction_from_yaw() {
        assert_eq!(cardinal(0.0), IVec3::Z);
        assert_eq!(cardinal(90.0), IVec3::NEG_X);
        assert_eq!(cardinal(-90.0), IVec3::X);
        assert_eq!(cardinal(180.0), IVec3::NEG_Z);
        assert_eq!(cardinal(44.0), IVec3::Z);
    }
}
