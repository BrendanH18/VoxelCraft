//! Java 26.3 happy ghasts (v0.15 Track C): `HappyGhast`, `HappyGhastAi`,
//! `Ghast.GhastMoveControl` (careful) and `Ghast.RandomFloatAroundGoal`, and
//! the `FlyingMoveControl` the ghastling uses. Sources and differences are in
//! `docs/verification/2026-11-18-v0.15-new-mobs.md`.
//!
//! A ghastling hatches from a dried ghast (`world::dried_ghast`) as a baby,
//! grows up in 20 minutes (snowballs speed it up by a tenth of what is
//! left), follows players holding snowballs and stays within 32 blocks of
//! its home. An adult wears a harness in its body slot (mount slot 1) and
//! carries up to four players; the first one steers it with Java's ridden
//! input and travel. A player standing on top puts it on a still timeout.
use super::mob::swim::TICK;
use super::{Ctx, Entities, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::inventory::Stack;
use crate::item::Item;
use crate::physics::{self, BlockSource};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

/// `HappyGhast.createAttributes`.
pub const MAX_HEALTH: f32 = 20.0;
pub const FLYING_SPEED: f64 = 0.05;
pub const MOVEMENT_SPEED: f64 = 0.05;
pub const TEMPT_RANGE: f64 = 16.0;
pub const FOLLOW_RANGE: f64 = 16.0;
/// `HappyGhast.BABY_SCALE`: the ghastling's size and model scale.
pub const BABY_SCALE: f64 = 0.2375;
/// `MAX_PASSANGERS`.
pub const MAX_PASSENGERS: usize = 4;
pub const SMALL_RESTRICTION_RADIUS: i32 = 32;
pub const LARGE_RESTRICTION_RADIUS: i32 = 64;
pub const RESTRICTION_RADIUS_BUFFER: i32 = 16;
pub const FAST_HEALING_TICKS: u32 = 20;
pub const SLOW_HEALING_TICKS: u32 = 600;
/// `STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD` and `MAX_STILL_TIMEOUT`.
pub const STILL_TIMEOUT_GRACE: u32 = 60;
pub const MAX_STILL_TIMEOUT: i32 = 10;
/// `getRiddenInput`'s `3.9F * FLYING_SPEED` and `tickRidden`'s turn speed.
pub const RIDDEN_INPUT_SCALE: f64 = 3.9;
pub const RIDDEN_TURN: f32 = 0.08;
/// `TemptGoal.ForNonPathfinders(1.0, …, 7.0)` for adults.
pub const ADULT_TEMPT_STOP: f64 = 7.0;
/// `HappyGhastAi`: tempted 1.25 to within 3, following adults (and
/// players) at 1.1 between 3 and 16 blocks, panicking at 2.0.
pub const BABY_TEMPT_SPEED: f64 = 1.25;
pub const BABY_CLOSE_ENOUGH: f64 = 3.0;
pub const BABY_FOLLOW_SPEED: f64 = 1.1;
pub const BABY_PANIC_SPEED: f64 = 2.0;
pub const ADULT_FOLLOW_MIN: f64 = 3.0;
pub const ADULT_FOLLOW_MAX: f64 = 16.0;
/// `RandomFloatAroundGoal(this, 16)`.
pub const FLOAT_DISTANCE_TO_BLOCKS: i32 = 16;
/// `getAmbientSoundInterval` is six times longer while carrying someone.
pub const RIDDEN_AMBIENT_FACTOR: f32 = 6.0;
/// Clouds (`isInClouds`): the overworld's cloud layer, 192.33 to 196.33.
const CLOUD_BOTTOM: f64 = 192.33;
const CLOUD_TOP: f64 = CLOUD_BOTTOM + 4.0;

/// Happy ghast state beyond the shared mount state (which holds riders and
/// the harness).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// `serverStillTimeout`, saved as `still_timeout`.
    pub still_timeout: i32,
    /// `tickCount` since it was created or loaded (the still-timeout grace).
    pub ticks: u32,
    /// Simulated seconds not yet spent on whole ticks.
    remainder: f64,
    /// A ghastling's nearest adult happy ghast within 16 blocks (`NEAREST_VISIBLE_ADULT`).
    pub(super) adult: Option<DVec3>,
    /// Someone is aboard: the harness goggles are down (`HARNESS_GOGGLES_DOWN`).
    pub goggles_down: bool,
}

impl State {
    pub fn save(&self) -> Value {
        json!({"still_timeout": self.still_timeout})
    }
    pub fn load(v: &Value) -> Self {
        Self {
            still_timeout: v["still_timeout"].as_i64().unwrap_or(0).clamp(0, i64::from(MAX_STILL_TIMEOUT)) as i32,
            ..Self::default()
        }
    }
    /// `isOnStillTimeout`.
    pub fn still(&self) -> bool {
        self.still_timeout > 0
    }
}

/// The items `TemptGoal` follows: harnesses and snowballs (`happy_ghast_tempt_items`)
/// for an adult with no harness, snowballs (`happy_ghast_food`) otherwise.
pub fn tempted_by(m: &Mob, item: Item) -> bool {
    let harnessed = m.mount.as_ref().is_some_and(|s| s.slots[1].is_some());
    is_food(item) || !harnessed && !m.baby && crate::content::harness::colour(item).is_some()
}

/// `#happy_ghast_food`.
pub fn is_food(item: Item) -> bool {
    item == Item::SNOWBALL
}

/// `getHappyGhastRestrictionRadius`.
pub fn restriction_radius(m: &Mob) -> i32 {
    if !m.baby && m.mount.as_ref().is_none_or(|s| s.slots[1].is_none()) {
        LARGE_RESTRICTION_RADIUS
    } else {
        SMALL_RESTRICTION_RADIUS
    }
}

/// Every player aboard, first (controlling) seat first.
pub fn passengers(s: &super::mounts::State) -> impl Iterator<Item = PlayerId> + '_ {
    [s.rider, s.passenger, s.seats[0], s.seats[1]].into_iter().flatten()
}

/// `EntityTypes.HAPPY_GHAST.passengerAttachments`: four seats at the edges
/// of the top, 1.7 blocks out along the body's front, left, back and
/// right, in boarding order.
pub fn seat(m: &Mob, s: &super::mounts::State, player: PlayerId) -> Option<DVec3> {
    let index = [s.rider, s.passenger, s.seats[0], s.seats[1]].iter().position(|p| *p == Some(player))?;
    let (sin, cos) = (m.yaw.sin() as f64, m.yaw.cos() as f64);
    let forward = DVec3::new(cos, 0.0, sin);
    let left = DVec3::new(sin, 0.0, -cos);
    let offset = [forward, left, -forward, -left][index] * 1.7;
    Some(m.pos + offset + DVec3::Y * m.shape().height)
}

/// `getDismountLocationForPassenger`: on top of the ghast, and the still
/// timeout `removePassenger` starts.
pub(super) fn dismount(m: &mut Mob, player: PlayerId) -> DVec3 {
    let top = m.pos + DVec3::Y * m.shape().height;
    let Some(s) = m.mount.as_mut() else { return top };
    for seat in {
        let [third, fourth] = &mut s.seats;
        [&mut s.rider, &mut s.passenger, third, fourth]
    } {
        if *seat == Some(player) {
            *seat = None;
        }
    }
    // The remaining passengers move up a seat, as Java's passenger list does.
    let left: Vec<PlayerId> = passengers(s).collect();
    s.rider = left.first().copied();
    s.passenger = left.get(1).copied();
    s.seats = [left.get(2).copied(), left.get(3).copied()];
    if let Some(h) = m.happy_ghast.as_mut() {
        h.still_timeout = MAX_STILL_TIMEOUT;
    }
    if s.rider.is_none() {
        // `removePassenger`: no one left aboard clears its home.
        m.restrict = None;
    }
    top
}

/// `HappyGhast.getRiddenInput`: strafe (Java's `xxa`, left positive), the
/// look's climb and forward parts (halved and reversed backwards), half a
/// unit up while jumping, all scaled by 3.9 × the flying speed.
pub fn ridden_input(forward: f32, strafe_right: f32, pitch: f32, jumping: bool) -> DVec3 {
    let (mut up, mut ahead) = (0.0f32, 0.0f32);
    if forward != 0.0 {
        // Java's `xRot` is the negated pitch: `-sin(xRot)` climbs when looking up.
        let x_rot = -pitch;
        let (mut f, mut u) = (x_rot.cos(), -x_rot.sin());
        if forward < 0.0 {
            f *= -0.5;
            u *= -0.5;
        }
        up = u;
        ahead = f;
    }
    if jumping {
        up += 0.5;
    }
    DVec3::new(f64::from(-strafe_right), f64::from(up), f64::from(ahead)) * (RIDDEN_INPUT_SCALE * FLYING_SPEED)
}

impl Mob {
    /// Whether a player at feet `p` (on no happy ghast) stands on top of
    /// this ghast (`scanPlayerAboveGhast`).
    pub(super) fn happy_ghast_box_has(&self, p: DVec3) -> bool {
        let shape = self.shape();
        let (min, max) = shape.aabb(self.pos);
        let lo = DVec3::new(min.x - 1.0, max.y - 1.0e-5, min.z - 1.0);
        let hi = DVec3::new(max.x + 1.0, max.y + shape.height / 2.0, max.z + 1.0);
        // `AABB.contains` is half-open.
        p.x >= lo.x && p.x < hi.x && p.y >= lo.y && p.y < hi.y && p.z >= lo.z && p.z < hi.z
    }

    /// Whether a rider steers this ghast now (`getControllingPassenger`).
    pub(super) fn happy_ghast_controlled(&self) -> bool {
        !self.baby
            && self.happy_ghast.as_ref().is_some_and(|h| !h.still())
            && self.mount.as_ref().is_some_and(|s| s.rider.is_some() && s.slots[1].is_some())
    }

    /// The happy ghast's AI decisions (`registerGoals` for adults,
    /// `HappyGhastAi` for ghastlings); the move controls run in
    /// [`Mob::happy_ghast_tick`].
    pub(super) fn happy_ghast_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        // `PanicGoal` ends once the hurt is forgotten (`mob::PANIC_MEMORY`).
        if self.ai == super::mob::Ai::Panic {
            self.ai_timer -= dt;
            if self.ai_timer <= 0.0 {
                self.ai = super::mob::Ai::Idle;
            }
        }
        let ridden = self.mount.as_ref().is_some_and(|s| s.rider.is_some());
        if ridden || self.happy_ghast.as_ref().is_some_and(|h| h.still()) {
            self.move_ctl.wanted = None;
            return (None, 0.0);
        }
        let tempter = ctx
            .players
            .iter()
            .filter(|p| p.alive && !p.spectator && p.held_item.is_some_and(|i| tempted_by(self, i)))
            .filter(|p| p.pos.distance_squared(self.pos) < TEMPT_RANGE * TEMPT_RANGE)
            .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)));
        if !self.baby {
            // Priority 4: `TemptGoal.ForNonPathfinders`, then 5: `RandomFloatAroundGoal`.
            if let Some(p) = tempter {
                let eye = p.pos + DVec3::Y * 1.62;
                if eye.distance_squared(self.pos) < ADULT_TEMPT_STOP * ADULT_TEMPT_STOP {
                    self.move_ctl.wanted = None;
                } else {
                    self.move_ctl.wanted = Some(self.pos + (eye - self.pos) * f64::from(rng.next_f32()));
                    self.move_ctl.speed = 1.0;
                }
                self.move_ctl.look = Some(eye);
                return (None, 0.0);
            }
            self.move_ctl.look = None;
            self.happy_ghast_float(world, rng);
            return (None, 0.0);
        }
        // Ghastling: panic, tempt, follow a player, follow an adult, stroll.
        let (target, speed, close) = if self.ai == super::mob::Ai::Panic && self.ai_timer > 0.0 {
            let away = self.pos
                + DVec3::new(rng.range(-10.0, 10.0) as f64, rng.range(-4.0, 4.0) as f64, rng.range(-10.0, 10.0) as f64);
            (Some(away), BABY_PANIC_SPEED, 0.0)
        } else if let Some(p) = tempter {
            (Some(p.pos + DVec3::Y * 1.62), BABY_TEMPT_SPEED, BABY_CLOSE_ENOUGH)
        } else if let Some(p) = ctx
            .players
            .iter()
            .filter(|p| p.alive && !p.spectator)
            .map(|p| (p, p.pos.distance(self.pos)))
            .filter(|&(_, d)| d <= ADULT_FOLLOW_MAX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            (Some(p.0.pos + DVec3::Y), BABY_FOLLOW_SPEED, ADULT_FOLLOW_MIN)
        } else if let Some(a) = self.happy_ghast.as_ref().and_then(|h| h.adult) {
            (Some(a + DVec3::Y * 2.0), BABY_FOLLOW_SPEED, ADULT_FOLLOW_MIN)
        } else {
            match self.move_ctl.wanted {
                Some(w) if w.distance_squared(self.pos) > 1.0 => (Some(w), 1.0, 0.0),
                _ if rng.next_int(120) == 0 => {
                    let r = |rng: &mut Rng, h: f32| f64::from(rng.range(-h, h));
                    let goal = self.pos + DVec3::new(r(rng, 10.0), r(rng, 7.0), r(rng, 10.0));
                    (Some(self.happy_ghast_home(goal)), 1.0, 0.0)
                }
                _ => (None, 1.0, 0.0),
            }
        };
        match target {
            Some(t) if t.distance(self.pos) > close => {
                self.move_ctl.wanted = Some(t);
                self.move_ctl.speed = speed;
            }
            _ => self.move_ctl.wanted = None,
        }
        (None, 0.0)
    }

    /// Pulls a random goal back inside the home radius.
    fn happy_ghast_home(&self, goal: DVec3) -> DVec3 {
        match self.restrict {
            Some((home, r)) if goal.distance(home.as_dvec3()) > f64::from(r) => home.as_dvec3() + DVec3::splat(0.5),
            _ => goal,
        }
    }

    /// `RandomFloatAroundGoal(16)`: once there is no wanted position, or it
    /// is within a block or past 60, up to 64 random spots within 16 blocks
    /// on each axis (inside the home), the first one in air with a block
    /// within 15 in some direction; above the heightmap it is reflected down.
    fn happy_ghast_float<W: BlockSource + ?Sized>(&mut self, world: &W, rng: &mut Rng) {
        if let Some(w) = self.move_ctl.wanted {
            let dd = w.distance_squared(self.pos);
            if (1.0..=3600.0).contains(&dd) {
                return;
            }
        }
        let mut result = None;
        for _ in 0..64 {
            let mut r = || f64::from(rng.next_f32() * 2.0 - 1.0) * 16.0;
            let target = self.pos + DVec3::new(r(), r(), r());
            let inside = self
                .restrict
                .is_none_or(|(home, radius)| target.floor().as_ivec3().distance_squared(home) < radius * radius);
            if !inside {
                continue;
            }
            result = Some(target);
            if good_float_target(world, target) {
                break;
            }
        }
        let mut target = result.unwrap_or_else(|| {
            let mut r = || f64::from(rng.next_f32() * 2.0 - 1.0) * 16.0;
            self.pos + DVec3::new(r(), r(), r())
        });
        let cell = target.floor().as_ivec3();
        if let Some(height) = surface_height(world, cell.x, cell.z, cell.y)
            && height < cell.y
        {
            target.y = self.pos.y - (self.pos.y - target.y).abs();
        }
        self.move_ctl.wanted = Some(target);
        self.move_ctl.speed = 1.0;
    }

    /// One Java tick of happy ghast movement: the ridden travel, the still
    /// timeout's `stopInPlace`, `GhastMoveControl` (adults) or
    /// `FlyingMoveControl` (ghastlings), then `travelFlying` at the flying
    /// speed × 5/3.
    pub(super) fn happy_ghast_tick<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, rng: &mut Rng) {
        let alive = self.alive();
        let still = self.happy_ghast.as_ref().is_some_and(|h| h.still());
        let mut input = DVec3::ZERO;
        let speed = FLYING_SPEED * 5.0 / 3.0;
        if alive && self.happy_ghast_controlled() {
            let s = self.mount.as_ref().unwrap();
            input = ridden_input(s.forward, s.strafe, s.look_pitch, s.ascend);
            // `tickRidden`: turn 8% of the way to the rider's yaw.
            let rider_yrot = s.yaw.to_degrees() - 90.0;
            let yrot = self.yrot();
            self.set_yrot(yrot + super::mob::swim::wrap_degrees(rider_yrot - yrot) * RIDDEN_TURN);
            self.move_ctl.wanted = None;
        } else if alive && still {
            // `GhastMoveControl`: WAIT and `stopInPlace`; the look control
            // snaps the body to the nearest quarter turn.
            *dm = DVec3::ZERO;
            self.move_ctl.wanted = None;
            let yrot = self.yrot();
            let snapped = yrot - wrap_degrees_90(yrot);
            self.set_yrot(snapped);
        } else if alive && !self.baby {
            if let Some(wanted) = self.move_ctl.wanted {
                self.move_ctl.float_duration -= 1;
                if self.move_ctl.float_duration < 0 {
                    self.move_ctl.float_duration += rng.next_int(5) as i32 + 2;
                    let travel = wanted - self.pos;
                    if happy_ghast_can_reach(world, self.pos, self.shape(), travel) {
                        *dm += travel.normalize_or_zero() * speed;
                    } else {
                        self.move_ctl.wanted = None;
                    }
                }
            }
            match self.move_ctl.look {
                Some(look) => {
                    let d = look - self.pos;
                    self.set_yrot(-(d.x.atan2(d.z).to_degrees() as f32));
                }
                None if dm.x != 0.0 || dm.z != 0.0 => self.set_yrot(-(dm.x.atan2(dm.z).to_degrees() as f32)),
                None => {}
            }
        } else if alive && let Some(wanted) = self.move_ctl.wanted {
            // `FlyingMoveControl(180, true)`: face the target at up to 90°
            // a tick and fly at the speed modifier × the flying speed.
            let d = wanted - self.pos;
            if d.length_squared() >= 2.500_000_3e-7 {
                let target = (d.z.atan2(d.x).to_degrees() as f32) - 90.0;
                let yrot = self.yrot();
                self.set_yrot(yrot + super::mob::swim::wrap_degrees(target - yrot).clamp(-90.0, 90.0));
                let fly = self.move_ctl.speed * FLYING_SPEED * self.effects.speed_factor();
                let horizontal = d.x.hypot(d.z);
                let up = if d.y.abs() > 1e-5 || horizontal > 1e-5 { if d.y > 0.0 { fly } else { -fly } } else { 0.0 };
                input = DVec3::new(0.0, up, fly);
            }
        }
        // `travelFlying(input, speed, speed, speed)`: the same speed in
        // water and lava, with 0.8 and 0.5 drag there and 0.91 in air.
        let drag = if self.touching_water(world) {
            0.8
        } else if self.touching_lava(world) {
            0.5
        } else {
            0.91
        };
        super::mob::swim::move_relative(dm, speed, input, self.yrot());
        self.java_move(world, dm, 0.0);
        *dm *= drag;
    }
}

/// `Mth.wrapDegrees90`.
fn wrap_degrees_90(a: f32) -> f32 {
    let a = a.rem_euclid(180.0);
    if a >= 90.0 { a - 180.0 } else { a }
}

/// `isGoodTarget(level, target, 16)`: air, with a non-air block within 15
/// steps in one of the six directions.
fn good_float_target<W: BlockSource + ?Sized>(world: &W, target: DVec3) -> bool {
    let cell = target.floor().as_ivec3();
    if world.block(cell).is_some_and(|b| b != crate::world::block::Block::AIR) {
        return false;
    }
    let dirs = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    dirs.iter().any(|&d| {
        (1..FLOAT_DISTANCE_TO_BLOCKS)
            .any(|i| world.block(cell + d * i).is_some_and(|b| b != crate::world::block::Block::AIR))
    })
}

/// The `MOTION_BLOCKING` height at a column: the cell above its highest
/// solid or fluid block at or below `from + 64`.
fn surface_height<W: BlockSource + ?Sized>(world: &W, x: i32, z: i32, from: i32) -> Option<i32> {
    let mut y = from + 64;
    while y > from - 128 {
        let b = world.block(IVec3::new(x, y, z))?;
        if b.is_solid() || b.is_fluid() {
            return Some(y + 1);
        }
        y -= 1;
    }
    None
}

/// `GhastMoveControl.canReach` with `careful`: nothing in the swept path,
/// and no `#happy_ghast_avoids` block within a block of the destination.
fn happy_ghast_can_reach<W: BlockSource + ?Sized>(world: &W, pos: DVec3, shape: physics::Shape, travel: DVec3) -> bool {
    let (min, max) = shape.aabb(pos + travel);
    let (lo, hi) = ((min - DVec3::ONE).floor().as_ivec3(), (max + DVec3::ONE).floor().as_ivec3());
    for x in lo.x..=hi.x {
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                if world.block(IVec3::new(x, y, z)).is_some_and(avoids) {
                    return false;
                }
            }
        }
    }
    physics::sweep_clear(world, pos, shape, travel)
}

/// `#happy_ghast_avoids`: sweet berry bushes, cactus, wither roses, magma,
/// fire and pointed dripstone (`#speleothems`).
pub fn avoids(b: crate::world::block::Block) -> bool {
    use crate::world::block::Block;
    b == Block::CACTUS
        || b == Block::MAGMA
        || b.is_fire()
        || matches!(b.name(), "sweet berry bush" | "wither rose" | "pointed dripstone" | "sulfur spike")
}

/// What a use on a happy ghast did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// A snowball was eaten (a ghastling grows).
    Fed,
    /// A harness went on (the held one is used up).
    Harnessed,
    /// Shears took the harness off.
    Sheared,
}

impl Entities {
    /// Feeding a ghastling, harnessing an adult and shearing a harness off
    /// (`HappyGhast.mobInteract` and `Equippable.harness`). Riding is in
    /// [`Entities::use_mount`].
    pub fn use_happy_ghast(&mut self, index: usize, held: Option<Stack>) -> Option<Use> {
        let m = self.mobs.get_mut(index).filter(|m| m.kind == MobKind::HappyGhast && m.alive())?;
        let item = held?.item;
        if m.baby && is_food(item) {
            // `Animal.mobInteract`: `canFallInLove` is false; a baby ages up a tenth.
            m.feed_growth();
            m.persistent = true;
            return Some(Use::Fed);
        }
        let s = m.mount.as_mut()?;
        if !m.baby && crate::content::harness::colour(item).is_some() && s.slots[1].is_none() {
            s.slots[1] = Some(Stack { count: 1, ..held? });
            m.persistent = true;
            return Some(Use::Harnessed);
        }
        // `canShearEquipment`: not while anyone rides.
        if item == Item::SHEARS && passengers(s).next().is_none() {
            let stack = s.slots[1].take()?;
            let pos = m.pos + DVec3::Y * m.shape().height;
            self.scatter(stack, pos);
            return Some(Use::Sheared);
        }
        None
    }

    /// Boards an adult happy ghast with a harness (`doPlayerRide`), up to
    /// four players. `addPassenger` starts a ten-tick still timeout only
    /// while someone stands on top.
    pub(super) fn board_happy_ghast(&mut self, index: usize, player: PlayerId) -> bool {
        let m = &mut self.mobs[index];
        if m.baby || !m.alive() {
            return false;
        }
        let Some(s) = m.mount.as_mut() else { return false };
        if s.slots[1].is_none() || passengers(s).any(|p| p == player) {
            return false;
        }
        let seat = {
            let [third, fourth] = &mut s.seats;
            [&mut s.rider, &mut s.passenger, third, fourth]
        }
        .into_iter()
        .find(|p| p.is_none());
        let Some(seat) = seat else { return false };
        *seat = Some(player);
        // Someone standing on top puts it back on the timeout next tick.
        if let Some(h) = m.happy_ghast.as_mut() {
            h.still_timeout = 0;
        }
        m.persistent = true;
        true
    }

    /// Per-tick happy ghast upkeep: the still timeout (`tick`), slow and
    /// fast healing (`continuousHeal`), the home (`checkRestriction`),
    /// ghastlings growing up and finding an adult to follow.
    pub(super) fn happy_ghast_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) {
        if !self.mobs.iter().any(|m| m.kind == MobKind::HappyGhast) {
            return;
        }
        let adults: Vec<DVec3> =
            self.mobs.iter().filter(|m| m.kind == MobKind::HappyGhast && m.alive() && !m.baby).map(|m| m.pos).collect();
        // Players on top of a ghast, by their root vehicle's position.
        let standing: Vec<DVec3> = ctx
            .players
            .iter()
            .filter(|p| !p.spectator)
            .filter(|p| {
                !self.mobs.iter().any(|m| {
                    m.kind == MobKind::HappyGhast && m.mount.as_ref().is_some_and(|s| passengers(s).any(|q| q == p.id))
                })
            })
            .map(|p| p.pos)
            .collect();
        for m in &mut self.mobs {
            if m.kind != MobKind::HappyGhast || !m.alive() || !m.simulation_active {
                continue;
            }
            let Some(h) = m.happy_ghast.as_mut() else { continue };
            h.remainder += dt.max(0.0) / TICK;
            let ticks = (h.remainder + 1e-9).floor() as u32;
            h.remainder -= f64::from(ticks);
            if ticks == 0 {
                continue;
            }
            let pos = m.pos;
            let height = m.shape().height;
            let aboard = m.mount.as_ref().is_some_and(|s| passengers(s).next().is_some());
            m.happy_ghast.as_mut().unwrap().goggles_down = aboard;
            for _ in 0..ticks {
                let h = m.happy_ghast.as_mut().unwrap();
                h.ticks = h.ticks.saturating_add(1);
                if h.still_timeout > 0 && h.ticks > STILL_TIMEOUT_GRACE {
                    h.still_timeout -= 1;
                }
                let tick = h.ticks;
                if standing.iter().any(|&p| m.happy_ghast_box_has(p)) {
                    m.happy_ghast.as_mut().unwrap().still_timeout = MAX_STILL_TIMEOUT;
                }
                // `continuousHeal`: every 600 ticks, or 20 in rain or clouds.
                if m.health < m.max_health() {
                    let cell = pos.floor().as_ivec3();
                    let clouds = pos.y + height > CLOUD_BOTTOM && pos.y < CLOUD_TOP;
                    let fast = clouds || ctx.raining && world.rains_on(cell);
                    if tick.is_multiple_of(if fast { FAST_HEALING_TICKS } else { SLOW_HEALING_TICKS }) {
                        m.health = (m.health + 1.0).min(m.max_health());
                    }
                }
                // `AgeableMob.aiStep`: a ghastling grows a tick at a time.
                if m.age < 0 {
                    m.age += 1;
                    m.baby = m.age < 0;
                }
            }
            // `checkRestriction`: home is where it is when it strays past
            // the radius plus 16 or the radius changes.
            let ridden = m.mount.as_ref().is_some_and(|s| s.rider.is_some());
            if m.leash.is_none() && !ridden {
                let radius = restriction_radius(m);
                let cell = m.pos.floor().as_ivec3();
                let stale = match m.restrict {
                    Some((home, r)) => {
                        r != radius
                            || home.as_dvec3().distance_squared(cell.as_dvec3())
                                >= f64::from((radius + RESTRICTION_RADIUS_BUFFER).pow(2))
                    }
                    None => true,
                };
                if stale {
                    m.restrict = Some((cell, radius));
                }
            }
            if m.baby {
                m.happy_ghast.as_mut().unwrap().adult = adults
                    .iter()
                    .copied()
                    .filter(|a| a.distance(m.pos) <= FOLLOW_RANGE)
                    .min_by(|a, b| a.distance_squared(m.pos).total_cmp(&b.distance_squared(m.pos)));
            }
        }
    }

    /// Plays and hatches what dried ghasts did this tick (`world::dried_ghast`).
    pub fn hatch_dried_ghasts(&mut self, w: &mut crate::world::World) {
        use crate::world::dried_ghast::Hydration;
        for e in std::mem::take(&mut w.dried_ghast_events) {
            if let Hydration::Hatched(p, yaw) = e {
                self.spawn_ghastling(p, yaw);
                let pos = p.as_dvec3() + DVec3::splat(0.5);
                let sound = super::MobSound::Ambient(MobKind::HappyGhast);
                self.pending_sounds.push(super::EntityEvent::Sound { sound, pos });
            }
        }
    }

    /// The dried ghast's `spawnGhastling`: a baby happy ghast at the block's
    /// bottom centre, facing the block's way (yaw in radians, ours).
    pub fn spawn_ghastling(&mut self, cell: IVec3, yaw: f32) {
        let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let mut m = Mob::new(MobKind::HappyGhast, pos, yaw);
        m.baby = true;
        m.age = super::animals::BABY_AGE;
        m.persistent = true;
        self.mobs.push(m);
        self.ensure_mob_ids();
    }
}

#[cfg(test)]
mod tests;
