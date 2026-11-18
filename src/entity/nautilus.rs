//! Java 26.3 nautiluses and zombie nautiluses (v0.15 Track C):
//! `AbstractNautilus`, `Nautilus`, `ZombieNautilus`, `NautilusAi`,
//! `ZombieNautilusAi` and `ChargeAttack`. Sources and differences are in
//! `docs/verification/2026-11-18-v0.15-new-mobs.md`.
//!
//! Both swim with `SmoothSwimmingMoveControl` and are tamed with a
//! pufferfish (one in three). A tame adult takes a saddle (mount slot 0)
//! and nautilus armor (slot 1); its rider steers it with Java's ridden
//! input, dashes with the jump key and has Breath of the Nautilus. A wild
//! adult charges whoever hurt it, and now and then a pufferfish. Only
//! nautiluses breed and dry out on land; zombie nautiluses burn in daylight
//! unless armored and carry the drowned that spawn on them.
use super::mob::swim::{TICK, move_relative, wrap_degrees};
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::inventory::Stack;
use crate::item::Item;
use crate::simulation::effects::Effect;
use glam::DVec3;
use serde_json::{Value, json};

/// `AbstractNautilus.createAttributes` and `ZombieNautilus.createAttributes`.
pub const MAX_HEALTH: f32 = 15.0;
pub const MOVEMENT_SPEED: f64 = 1.0;
pub const ZOMBIE_MOVEMENT_SPEED: f64 = 1.1;
pub const ATTACK_DAMAGE: f32 = 3.0;
pub const KNOCKBACK_RESISTANCE: f32 = 0.3;
/// `SmoothSwimmingMoveControl(this, 85, 10, 0.011F, 0.0F, true)`.
pub const IN_WATER_SPEED_MODIFIER: f64 = 0.011;
pub const MAX_TURN_PITCH: f32 = 85.0;
pub const MAX_TURN_YAW: f32 = 10.0;
/// `NAUTILUS_WATER_RESISTANCE`.
pub const WATER_RESISTANCE: f64 = 0.9;
/// `getRiddenSpeed`: these times the movement speed.
pub const RIDDEN_SPEED_IN_WATER: f64 = 0.0325;
pub const RIDDEN_SPEED_ON_LAND: f64 = 0.02;
/// `tickRidden`'s turn speed.
pub const RIDDEN_TURN: f32 = 0.5;
/// `DASH_COOLDOWN_TICKS`, `DASH_MOMENTUM_IN_WATER` and `DASH_MOMENTUM_ON_LAND`.
pub const DASH_COOLDOWN: i32 = 40;
pub const DASH_MOMENTUM_IN_WATER: f64 = 1.2;
pub const DASH_MOMENTUM_ON_LAND: f64 = 0.5;
/// `EFFECT_DURATION` and `EFFECT_REFRESH_RATE` of Breath of the Nautilus.
pub const EFFECT_DURATION: u32 = 60;
pub const EFFECT_REFRESH_RATE: u32 = 40;
/// `NAUTILUS_TOTAL_AIR_SUPPLY` and the dry-out hit.
pub const TOTAL_AIR: i32 = 300;
pub const DRY_OUT_DAMAGE: f32 = 2.0;
pub const SMALL_RESTRICTION_RADIUS: i32 = 16;
pub const LARGE_RESTRICTION_RADIUS: i32 = 32;
pub const RESTRICTION_RADIUS_BUFFER: i32 = 8;
/// `NautilusAi`: anger lasts 400 ticks; charges 80 ticks apart, up to 12
/// blocks, at targets within 11, at 0.6 (zombie 0.5) with knockback 2.
pub const ANGER_DURATION: i32 = 400;
pub const TIME_BETWEEN_ATTACKS: i32 = 80;
pub const MAX_CHARGE_DISTANCE: f64 = 12.0;
pub const MAX_TARGET_DETECTION_DISTANCE: f64 = 11.0;
pub const CHARGE_SPEED: f64 = 0.6;
pub const ZOMBIE_CHARGE_SPEED: f64 = 0.5;
pub const CHARGE_KNOCKBACK: f32 = 2.0;
/// `TIME_BETWEEN_NON_PLAYER_ATTACKS`: `UniformInt.of(2400, 3600)`, then a
/// coin flip before hunting the nearest pufferfish.
pub const NON_PLAYER_ATTACKS: (u32, u32) = (2400, 3600);
/// `FollowTemptation`: 1.3 (zombie 0.9), stopping 3.5 blocks away (2.5 for babies).
pub const TEMPT_SPEED: f64 = 1.3;
pub const ZOMBIE_TEMPT_SPEED: f64 = 0.9;
pub const TEMPT_RANGE: f64 = 10.0;
/// `AnimalPanic(1.6F)` and `AnimalMakeLove(…, 0.4F, 2)`.
pub const PANIC_SPEED: f64 = 1.6;
pub const LOVE_SPEED: f64 = 0.4;
/// `tryToTame`: `random.nextInt(3) == 0`.
pub const TAME_ONE_IN: u32 = 3;
/// `checkNautilusSpawnRules`: between 25 and 5 blocks below sea level.
pub const SPAWN_DEPTH: (i32, i32) = (25, 5);
/// `EntityTypes.NAUTILUS.passengerAttachments(1.1375F)`; babies at 0.5.
pub const PASSENGER_HEIGHT: f64 = 1.1375;

pub fn is_nautilus(kind: MobKind) -> bool {
    matches!(kind, MobKind::Nautilus | MobKind::ZombieNautilus)
}

/// The `MOVEMENT_SPEED` attribute.
pub fn movement_speed(kind: MobKind) -> f64 {
    if kind == MobKind::ZombieNautilus { ZOMBIE_MOVEMENT_SPEED } else { MOVEMENT_SPEED }
}

/// Nautilus armor (`#can_wear_nautilus_armor` names both kinds).
pub fn wears(item: Item) -> bool {
    item.body_armor().is_some_and(|(fits, _)| fits == crate::equipment::BodyArmor::Nautilus)
}

/// `#nautilus_taming_items`.
pub fn taming_item(item: Item) -> bool {
    matches!(item, Item::PUFFERFISH | Item::PUFFERFISH_BUCKET)
}

/// `#nautilus_bucket_food`: eaten from the bucket, which becomes a water bucket.
pub fn bucket_food(item: Item) -> bool {
    matches!(item, Item::PUFFERFISH_BUCKET | Item::COD_BUCKET | Item::SALMON_BUCKET | Item::TROPICAL_FISH_BUCKET)
}

/// `#nautilus_food`: `#fishes` (raw and cooked cod and salmon, tropical
/// fish, pufferfish) and the fish buckets.
pub fn food(item: Item) -> bool {
    matches!(item, Item::COD | Item::SALMON | Item::TROPICAL_FISH | Item::PUFFERFISH)
        || item == Item::COOKED_COD
        || item == Item::COOKED_SALMON
        || bucket_food(item)
}

/// Where its one rider sits.
pub fn seat(m: &Mob) -> DVec3 {
    m.pos + DVec3::Y * if m.baby { 0.5 } else { PASSENGER_HEIGHT }
}

/// A charge in progress (`ChargeAttack.start`'s velocity and start).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    pub velocity: DVec3,
    pub start: DVec3,
    pub target: ChargeTarget,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChargeTarget {
    Player(PlayerId),
    /// A pufferfish's uid.
    Mob(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// `airSupply` in ticks (nautiluses dry out on land).
    pub air: i32,
    /// `dashCooldown`, and the `DASH` flag.
    pub dash_cooldown: i32,
    pub dashing: bool,
    /// `ANGRY_AT` and its ticks left.
    pub anger: Option<(PlayerId, i32)>,
    /// `CHARGE_COOLDOWN_TICKS` and `ATTACK_TARGET_COOLDOWN`.
    pub charge_cooldown: i32,
    pub hunt_cooldown: i32,
    pub charge: Option<Charge>,
    /// Ticks lived, for the 40-tick effect refresh.
    pub ticks: u32,
    remainder: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            air: TOTAL_AIR,
            dash_cooldown: 0,
            dashing: false,
            anger: None,
            charge_cooldown: 0,
            hunt_cooldown: 0,
            charge: None,
            ticks: 0,
            remainder: 0.0,
        }
    }
}

impl State {
    pub fn save(&self) -> Value {
        json!({
            "air": self.air,
            "dash_cooldown": self.dash_cooldown,
            "angry_at": self.anger.map(|(p, t)| json!([p.0, t])),
            "charge_cooldown": self.charge_cooldown,
            "attack_target_cooldown": self.hunt_cooldown,
        })
    }
    pub fn load(v: &Value) -> Self {
        let int = |k: &str, lo: i64, hi: i64, d: i32| v[k].as_i64().map_or(d, |n| n.clamp(lo, hi) as i32);
        Self {
            air: int("air", -20, i64::from(TOTAL_AIR), TOTAL_AIR),
            dash_cooldown: int("dash_cooldown", 0, i64::from(DASH_COOLDOWN), 0),
            anger: v["angry_at"].as_array().and_then(|a| {
                let p = u32::try_from(a.first()?.as_u64()?).ok()?;
                let t = a.get(1)?.as_i64()?.clamp(0, i64::from(ANGER_DURATION)) as i32;
                Some((PlayerId(p), t))
            }),
            charge_cooldown: int("charge_cooldown", 0, i64::from(TIME_BETWEEN_ATTACKS), 0),
            hunt_cooldown: int("attack_target_cooldown", 0, i64::from(NON_PLAYER_ATTACKS.1), 0),
            ..Self::default()
        }
    }
}

impl Mob {
    fn nautilus_tame(&self) -> bool {
        self.mount.as_ref().is_some_and(|s| s.tame)
    }

    /// A saddled tame nautilus with a player aboard (`getControllingPassenger`).
    pub(super) fn nautilus_controlled(&self) -> bool {
        self.mount.as_ref().is_some_and(|s| s.rider.is_some() && s.tame && s.saddled())
    }

    /// `NautilusAi`/`ZombieNautilusAi` idle decisions: a mob rider's
    /// target, love, temptation and a swim now and then. Charges and the
    /// pufferfish hunt run in [`Entities::nautilus_upkeep`].
    pub(super) fn nautilus_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        _events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        if self.ai == super::mob::Ai::Panic {
            self.ai_timer -= dt;
            if self.ai_timer <= 0.0 {
                self.ai = super::mob::Ai::Idle;
            }
        }
        let ridden = self.mount.as_ref().is_some_and(|s| s.rider.is_some());
        if ridden || self.nautilus.as_ref().is_some_and(|s| s.charge.is_some()) {
            self.move_ctl.wanted = None;
            return (None, 0.0);
        }
        // A drowned aboard steers toward its target (`isMobControlled`).
        if let Some(goal) = self.hunt {
            self.move_ctl.wanted = Some(goal);
            self.move_ctl.speed = 1.0;
            return (None, 0.0);
        }
        let zombie = self.kind == MobKind::ZombieNautilus;
        // Priority 0 (nautilus core): `AnimalPanic(1.6)`.
        if !zombie && self.ai == super::mob::Ai::Panic && self.ai_timer > 0.0 {
            if self.move_ctl.wanted.is_none_or(|w| w.distance_squared(self.pos) < 1.0) {
                self.move_ctl.wanted = water_goal(world, self.pos, rng, 5, 4);
            }
            self.move_ctl.speed = PANIC_SPEED;
            return (None, 0.0);
        }
        // 1: `AnimalMakeLove` (nautiluses only).
        if let Some((uid, partner)) = self.animal.as_ref().and_then(|a| a.partner) {
            self.move_ctl.wanted = Some(partner);
            self.move_ctl.speed = LOVE_SPEED;
            if partner.distance_squared(self.pos) < 9.0 {
                let a = self.animal.as_mut().unwrap();
                a.courtship += dt;
                if a.courtship >= super::animals::COURTSHIP_SECONDS {
                    a.courtship = 0.0;
                    a.breed_now = Some(uid);
                }
            }
            return (None, 0.0);
        }
        // 2: `FollowTemptation`.
        let close = if self.baby { 2.5 } else { 3.5 };
        if let Some(p) = ctx
            .players
            .iter()
            .filter(|p| p.alive && !p.spectator && p.held_item.is_some_and(food))
            .filter(|p| p.pos.distance_squared(self.pos) < TEMPT_RANGE * TEMPT_RANGE)
            .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))
        {
            let eye = p.pos + DVec3::Y;
            self.move_ctl.wanted = (eye.distance(self.pos) > close).then_some(eye);
            self.move_ctl.speed = if zombie { ZOMBIE_TEMPT_SPEED } else { TEMPT_SPEED };
            return (None, 0.0);
        }
        // 4: `RandomStroll.swim(1.0)` behind the walk-target gate.
        let reached = self.move_ctl.wanted.is_none_or(|w| w.distance_squared(self.pos) < 1.0);
        if reached && rng.chance(dt / 6.0) {
            let goal = water_goal(world, self.pos, rng, 10, 7);
            self.move_ctl.wanted = goal.map(|g| match self.restrict {
                Some((home, r)) if g.distance(home.as_dvec3()) > f64::from(r) => home.as_dvec3() + DVec3::splat(0.5),
                _ => g,
            });
            self.move_ctl.speed = 1.0;
        } else if reached {
            self.move_ctl.wanted = None;
        }
        (None, 0.0)
    }

    /// One Java tick of nautilus movement: the ridden travel and dash, a
    /// charge, or `SmoothSwimmingMoveControl` toward the wanted position,
    /// then `travelInWater` (0.9 drag, no sinking) or `travelInAir`.
    pub(super) fn nautilus_tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        dm: &mut DVec3,
        in_water: bool,
        alive: bool,
    ) {
        let speed_attr = movement_speed(self.kind) * self.effects.speed_factor();
        if alive && self.nautilus_controlled() {
            let s = self.mount.as_mut().unwrap();
            // `getRiddenInput`: unscaled strafe, climb and forward.
            let (mut up, mut ahead) = (0.0f32, 0.0f32);
            if s.forward != 0.0 {
                let x_rot = -s.look_pitch;
                let (mut f, mut u) = (x_rot.cos(), -x_rot.sin());
                if s.forward < 0.0 {
                    f *= -0.5;
                    u *= -0.5;
                }
                up = u;
                ahead = f;
            }
            let input = DVec3::new(f64::from(-s.strafe), f64::from(up), f64::from(ahead));
            let rider_yrot = s.yaw.to_degrees() - 90.0;
            let (sy, cy) = s.yaw.sin_cos();
            let (sp, cp) = s.look_pitch.sin_cos();
            let look = DVec3::new(f64::from(cy * cp), f64::from(sp), f64::from(sy * cp));
            let jump = s.take_pending_jump();
            let yrot = self.yrot();
            self.set_yrot(yrot + wrap_degrees(rider_yrot - yrot) * RIDDEN_TURN);
            self.move_ctl.pitch = -s_pitch_degrees(self.mount.as_ref().unwrap().look_pitch) * 0.5;
            // `executeRidersJump`: the dash along the rider's look.
            if let (Some(charge), Some(n)) = (jump, self.nautilus.as_mut())
                && charge > 0.0
                && n.dash_cooldown <= 0
            {
                let amount = if charge >= 0.9 { 1.0 } else { 0.4 + 0.4 * f64::from(charge) / 0.9 };
                let momentum = if in_water { DASH_MOMENTUM_IN_WATER } else { DASH_MOMENTUM_ON_LAND };
                *dm += look * (momentum * amount * speed_attr);
                n.dash_cooldown = DASH_COOLDOWN;
                n.dashing = true;
            }
            let ridden =
                if in_water { RIDDEN_SPEED_IN_WATER } else { RIDDEN_SPEED_ON_LAND } * movement_speed(self.kind);
            self.move_ctl.cur = ridden;
            self.nautilus_travel(world, dm, input, in_water);
            return;
        }
        if alive && let Some(c) = self.nautilus.as_ref().and_then(|s| s.charge) {
            // `ChargeAttack.tick`: the start velocity, every tick.
            *dm = c.velocity;
            if c.velocity.x != 0.0 || c.velocity.z != 0.0 {
                self.set_yrot(-(c.velocity.x.atan2(c.velocity.z).to_degrees() as f32));
            }
            self.java_move(world, dm, 0.6);
            *dm *= WATER_RESISTANCE;
            return;
        }
        let mut input = DVec3::ZERO;
        self.move_ctl.cur = 0.0;
        if alive && let Some(wanted) = self.move_ctl.wanted {
            let d = wanted - self.pos;
            if d.length_squared() >= 2.500_000_3e-7 {
                let target = (d.z.atan2(d.x).to_degrees() as f32) - 90.0;
                let yrot = self.yrot();
                self.set_yrot(yrot + wrap_degrees(target - yrot).clamp(-MAX_TURN_YAW, MAX_TURN_YAW));
                let speed = self.move_ctl.speed * speed_attr;
                if in_water {
                    self.move_ctl.cur = speed * IN_WATER_SPEED_MODIFIER;
                    let h = d.x.hypot(d.z);
                    if d.y.abs() > 1e-5 || h > 1e-5 {
                        let x_rot =
                            wrap_degrees(-(d.y.atan2(h).to_degrees() as f32)).clamp(-MAX_TURN_PITCH, MAX_TURN_PITCH);
                        let p = self.move_ctl.pitch;
                        self.move_ctl.pitch = p + wrap_degrees(x_rot - p).clamp(-5.0, 5.0);
                    }
                    let (s, c) = self.move_ctl.pitch.to_radians().sin_cos();
                    input = DVec3::new(0.0, -f64::from(s) * speed, f64::from(c) * speed);
                }
                // Out of water the outside speed modifier is 0: no stroke.
            }
        }
        self.nautilus_travel(world, dm, input, in_water);
    }

    /// `AbstractNautilus.travelInWater` and `LivingEntity.travelInAir`.
    fn nautilus_travel<W: MobWorld + ?Sized>(&mut self, world: &W, dm: &mut DVec3, input: DVec3, in_water: bool) {
        if in_water {
            move_relative(dm, self.move_ctl.cur, input, self.yrot());
            self.java_move(world, dm, 0.6);
            *dm *= WATER_RESISTANCE;
        } else {
            self.travel_in_air(world, dm, input);
        }
    }
}

/// Degrees of a pitch in radians.
fn s_pitch_degrees(p: f32) -> f32 {
    p.to_degrees()
}

/// A random water cell within `h` blocks across and `v` up or down
/// (`RandomStroll.swim`'s `BehaviorUtils.getRandomSwimmablePos`).
fn water_goal<W: MobWorld + ?Sized>(world: &W, pos: DVec3, rng: &mut Rng, h: i32, v: i32) -> Option<DVec3> {
    for _ in 0..10 {
        let off = glam::IVec3::new(
            rng.next_int(2 * h as u32 + 1) as i32 - h,
            rng.next_int(2 * v as u32 + 1) as i32 - v,
            rng.next_int(2 * h as u32 + 1) as i32 - h,
        );
        let cell = pos.floor().as_ivec3() + off;
        if world.block(cell).is_some_and(|b| b.is_water()) {
            return Some(cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        }
    }
    None
}

/// What a use on a nautilus did, and what the held stack becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// The food was eaten; `bucket` leaves a water bucket.
    Ate { bucket: bool },
    /// A taming try that worked (or not) ate the food.
    Tamed { tamed: bool, bucket: bool },
    /// Shears took the saddle off.
    Sheared,
}

impl Entities {
    /// `AbstractNautilus.mobInteract` for food: feeding babies, taming
    /// with a pufferfish (one in three), healing and love for tame adults,
    /// and shears taking the saddle off. Saddles, armor and riding go
    /// through [`Entities::use_mount`].
    pub fn use_nautilus(&mut self, index: usize, held: Option<Stack>, player: PlayerId) -> Option<Use> {
        let m = self.mobs.get_mut(index).filter(|m| is_nautilus(m.kind) && m.alive())?;
        let item = held?.item;
        let bucket = bucket_food(item);
        m.persistent = true;
        if m.baby {
            if !food(item) {
                return None;
            }
            m.feed_growth();
            return Some(Use::Ate { bucket });
        }
        let tame = m.nautilus_tame();
        if !tame {
            if !taming_item(item) {
                return None;
            }
            let tamed = self.rng.next_int(TAME_ONE_IN) == 0;
            let m = &mut self.mobs[index];
            if tamed {
                let s = m.mount.as_mut()?;
                s.tame = true;
                s.owner = Some(player);
                m.move_ctl.wanted = None;
                if let Some(n) = m.nautilus.as_mut() {
                    n.anger = None;
                    n.charge = None;
                }
            } else {
                let p = m.pos + DVec3::Y * 0.7;
                self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
                    crate::particles::Kind::Smoke,
                    p,
                    7,
                )));
            }
            return Some(Use::Tamed { tamed, bucket });
        }
        if item == Item::SHEARS {
            let s = m.mount.as_mut()?;
            if s.rider.is_some() {
                return None;
            }
            let saddle = s.slots[0].take()?;
            let pos = m.pos + DVec3::Y * 0.5;
            self.scatter(saddle, pos);
            return Some(Use::Sheared);
        }
        if !food(item) {
            return None;
        }
        if m.health < m.max_health() {
            // `feed(player, hand, stack, 2.0F, 1.0F)`.
            m.health = (m.health + 2.0).min(m.max_health());
            return Some(Use::Ate { bucket });
        }
        if m.kind == MobKind::Nautilus
            && m.age == 0
            && let Some(a) = m.animal.as_mut().filter(|a| a.love <= 0.0)
        {
            a.love = super::animals::LOVE_SECONDS;
            a.love_by = Some(player);
            self.animal_hearts(index);
            return Some(Use::Ate { bucket });
        }
        None
    }

    /// Per-tick nautilus upkeep: air on land, the dash cooldown, Breath of
    /// the Nautilus for the rider, anger, charges at players and
    /// pufferfish (`ChargeAttack`), and a tame one's home.
    pub(super) fn nautilus_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| is_nautilus(m.kind)) {
            return;
        }
        self.ensure_mob_ids();
        for i in 0..self.mobs.len() {
            let m = &mut self.mobs[i];
            if !is_nautilus(m.kind) || !m.alive() || !m.simulation_active {
                continue;
            }
            let Some(n) = m.nautilus.as_mut() else { continue };
            n.remainder += dt.max(0.0) / TICK;
            let ticks = (n.remainder + 1e-9).floor() as u32;
            n.remainder -= f64::from(ticks);
            for _ in 0..ticks {
                self.nautilus_tick_once(i, world, ctx, events);
            }
        }
    }

    fn nautilus_tick_once<W: MobWorld + ?Sized>(
        &mut self,
        i: usize,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        let in_water = {
            let m = &self.mobs[i];
            m.touching_water(world)
        };
        let m = &mut self.mobs[i];
        let kind = m.kind;
        let n = m.nautilus.as_mut().unwrap();
        n.ticks = n.ticks.wrapping_add(1);
        // `Nautilus.handleAirSupply`: a nautilus out of water loses a tick
        // of air each tick and takes 2 when it reaches -20.
        let mut dry = false;
        if kind == MobKind::Nautilus {
            if in_water {
                n.air = TOTAL_AIR;
            } else {
                n.air -= 1;
                if n.air <= -20 {
                    n.air = 0;
                    dry = true;
                }
            }
        }
        if n.dashing && n.dash_cooldown < 35 {
            n.dashing = false;
        }
        n.dash_cooldown = (n.dash_cooldown - 1).max(0);
        n.charge_cooldown = (n.charge_cooldown - 1).max(0);
        n.hunt_cooldown = n.hunt_cooldown.saturating_sub(1).max(0);
        if let Some((p, t)) = n.anger {
            n.anger = (t > 1).then_some((p, t - 1));
        }
        let effect_refresh = n.ticks.is_multiple_of(EFFECT_REFRESH_RATE);
        if dry {
            m.damage(DRY_OUT_DAMAGE, None, &mut self.rng);
        }
        // `applyEffects`: Breath of the Nautilus for a player first passenger.
        if let Some(rider) = m.mount.as_ref().and_then(|s| s.rider)
            && let Some(t) = ctx.players.iter().find(|t| t.id == rider)
            && (!t.has_effect(Effect::BreathOfTheNautilus) || effect_refresh)
        {
            events.push(EntityEvent::PlayerEffect {
                player: rider,
                effect: Effect::BreathOfTheNautilus,
                amplifier: 0,
                ticks: EFFECT_DURATION,
            });
        }
        // `checkRestriction`, tame ones only.
        let tame = m.mount.as_ref().is_some_and(|s| s.tame);
        let ridden = m.mount.as_ref().is_some_and(|s| s.rider.is_some());
        if tame && !ridden && m.leash.is_none() {
            let radius = if !m.baby && m.mount.as_ref().is_some_and(|s| !s.saddled()) {
                LARGE_RESTRICTION_RADIUS
            } else {
                SMALL_RESTRICTION_RADIUS
            };
            let cell = m.pos.floor().as_ivec3();
            let stale = m.restrict.is_none_or(|(home, r)| {
                r != radius
                    || home.as_dvec3().distance_squared(cell.as_dvec3())
                        >= f64::from((radius + RESTRICTION_RADIUS_BUFFER).pow(2))
            });
            if stale {
                m.restrict = Some((cell, radius));
            }
        }
        self.nautilus_fight(i, world, ctx, in_water, events);
    }

    /// `NautilusAi.findNearestValidAttackTarget` and `ChargeAttack`.
    fn nautilus_fight<W: MobWorld + ?Sized>(
        &mut self,
        i: usize,
        _world: &W,
        ctx: &Ctx,
        in_water: bool,
        events: &mut Vec<EntityEvent>,
    ) {
        let m = &self.mobs[i];
        let tame = m.mount.as_ref().is_some_and(|s| s.tame);
        let breeding = m.animal.as_ref().is_some_and(|a| a.love > 0.0);
        let ridden = m.mount.as_ref().is_some_and(|s| s.rider.is_some());
        let tempted = ctx
            .players
            .iter()
            .any(|p| p.held_item.is_some_and(food) && p.pos.distance_squared(m.pos) < TEMPT_RANGE * TEMPT_RANGE);
        let pos = m.pos;
        let shape = m.shape();
        let speed = if m.kind == MobKind::ZombieNautilus { ZOMBIE_CHARGE_SPEED } else { CHARGE_SPEED };
        let n = m.nautilus.as_ref().unwrap();
        // A charge in progress: `canStillUse`, then contact (`tick`).
        if let Some(c) = n.charge {
            let target = match c.target {
                ChargeTarget::Player(p) => ctx.players.iter().find(|t| t.id == p && t.alive).map(|t| (t.pos, t.shape)),
                ChargeTarget::Mob(uid) => {
                    self.mobs.iter().find(|o| o.uid == uid && o.alive()).map(|o| (o.pos, o.shape()))
                }
            };
            let keep = !tame
                && pos.distance_squared(c.start) < MAX_CHARGE_DISTANCE * MAX_CHARGE_DISTANCE
                && target.is_some_and(|(t, _)| {
                    t.distance_squared(pos) < MAX_TARGET_DETECTION_DISTANCE * MAX_TARGET_DETECTION_DISTANCE
                });
            let hit = target.is_some_and(|(t, s)| boxes_touch(pos, shape, t, s));
            if hit {
                self.nautilus_charge_hit(i, c, events);
            }
            if hit || !keep {
                let n = self.mobs[i].nautilus.as_mut().unwrap();
                n.charge = None;
                n.charge_cooldown = TIME_BETWEEN_ATTACKS;
            }
            return;
        }
        if tame || breeding || ridden || tempted || !in_water || m.baby || n.charge_cooldown > 0 {
            return;
        }
        // The angry-at player, if in water and within reach.
        let mut target = n.anger.and_then(|(p, _)| {
            ctx.players
                .iter()
                .find(|t| t.id == p && t.alive && t.targetable && t.in_water)
                .filter(|t| t.pos.distance_squared(pos) < MAX_TARGET_DETECTION_DISTANCE.powi(2))
                .map(|t| (ChargeTarget::Player(p), t.pos))
        });
        if target.is_none() && n.hunt_cooldown <= 0 {
            let (lo, hi) = NON_PLAYER_ATTACKS;
            let cooldown = (lo + self.rng.next_int(hi - lo + 1)) as i32;
            self.mobs[i].nautilus.as_mut().unwrap().hunt_cooldown = cooldown;
            if self.rng.next_f32() >= 0.5 {
                // `#nautilus_hostiles`: the nearest visible pufferfish in water.
                target = self
                    .mob_index
                    .nearest(&self.mobs, pos, 16.0, |o| o.kind == MobKind::Pufferfish && o.alive() && o.in_water)
                    .map(|j| (ChargeTarget::Mob(self.mobs[j].uid), self.mobs[j].pos));
            }
        }
        if let Some((target, at)) = target {
            let velocity = (at - pos).normalize_or_zero() * speed;
            let m = &mut self.mobs[i];
            m.nautilus.as_mut().unwrap().charge = Some(Charge { velocity, start: pos, target });
            m.vel = velocity / TICK;
        }
    }

    /// `dealDamageToTarget` and `dealKnockBack`.
    fn nautilus_charge_hit(&mut self, i: usize, c: Charge, events: &mut Vec<EntityEvent>) {
        let m = &self.mobs[i];
        let (damage, cause) = m.kind.melee();
        let speed = if m.kind == MobKind::ZombieNautilus { ZOMBIE_CHARGE_SPEED } else { CHARGE_SPEED };
        let factor = (speed as f32 * movement_speed(m.kind) as f32).clamp(0.2, 2.0) * CHARGE_KNOCKBACK;
        let push = c.velocity.normalize_or_zero() * f64::from(factor) * 8.0 + DVec3::Y * 4.0;
        match c.target {
            ChargeTarget::Player(player) => {
                events.push(EntityEvent::PlayerHit { player, damage, knockback: push.as_vec3(), java: None, cause })
            }
            ChargeTarget::Mob(uid) => {
                if let Some(j) = self.mobs.iter().position(|o| o.uid == uid) {
                    let mut rng = Rng::new(u64::from(uid) ^ 0x6e61_7574);
                    self.mobs[j].damage(damage, Some(push), &mut rng);
                }
            }
        }
    }

    /// `AbstractNautilus.finalizeSpawn`'s first `ATTACK_TARGET_COOLDOWN`,
    /// and the zombie nautilus's warm-ocean coral variant.
    pub(super) fn nautilus_on_spawn(&mut self, warm: bool) {
        let (lo, hi) = NON_PLAYER_ATTACKS;
        let cooldown = (lo + self.rng.next_int(hi - lo + 1)) as i32;
        if let Some(m) = self.mobs.last_mut()
            && let Some(n) = m.nautilus.as_mut()
        {
            n.hunt_cooldown = cooldown;
            if m.kind == MobKind::ZombieNautilus
                && let Some(s) = m.mount.as_mut()
            {
                s.variant = u8::from(warm);
            }
        }
    }
}

/// Whether two mob boxes overlap.
fn boxes_touch(a: DVec3, sa: crate::physics::Shape, b: DVec3, sb: crate::physics::Shape) -> bool {
    let (amin, amax) = sa.aabb(a);
    let (bmin, bmax) = sb.aabb(b);
    amin.x < bmax.x && amax.x > bmin.x && amin.y < bmax.y && amax.y > bmin.y && amin.z < bmax.z && amax.z > bmin.z
}

/// `checkNautilusSpawnRules`: 5 to 25 blocks below sea level, water below
/// and above.
pub fn spawn_ok(
    at: crate::world::block::Block,
    below: crate::world::block::Block,
    above: crate::world::block::Block,
    y: i32,
) -> bool {
    use crate::world::terrain::SEA_LEVEL;
    let _ = at;
    y >= SEA_LEVEL - SPAWN_DEPTH.0 && y <= SEA_LEVEL - SPAWN_DEPTH.1 && below.holds_water() && above.is_water()
}

#[cfg(test)]
mod tests;
