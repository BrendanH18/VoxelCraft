//! v1.0 gap Track 2b: mob spears as Java 26.3 runs them.
//!
//! - **Look** (`LookControl`, `Mob.lookAt`): a mob aiming a spear keeps
//!   Java's `yHeadRot` and `xRot`. `SpearUseGoal` turns the body by
//!   `Mob.lookAt(target, 30, 30)` on its updates and asks the look control
//!   for 30° a tick; the piglin's `LookAtTargetSink` asks for the default
//!   10° a tick and 40° of pitch. Each tick the pitch restarts from level
//!   (`resetXRotOnTick`) and turns toward the wanted point by at most the
//!   pitch limit; the head turns by the yaw limit while the request lasts
//!   (two ticks), then back toward the body by 10°, and stays within 75° of
//!   the body while the mob walks a path. The charge's ray follows the head
//!   (`getHeadLookAngle`); its speeds are taken along the body's look
//!   (`getLookAngle`).
//! - **Stabs** (`KineticWeapon.damageEntities`, `LivingEntity.stabAttack`):
//!   every tick a spear is lowered, the ray from the eye stabs every player
//!   and mob whose box (grown by the 0.125 margin) it crosses between 1 and
//!   2.25 blocks plus the mob's own forward motion, short of the first block,
//!   skipping the mob's own vehicle and its passengers. Each target is
//!   remembered for the 10-tick contact cooldown, checked before the
//!   conditions. A contact that only knocks back pushes 0.4 along the mob's
//!   facing (and the attack knockback), without damage; one that dismounts
//!   takes the target off its vehicle. Mob stabs don't wear the spear:
//!   `LivingEntity.stabAttack` calls only `ItemStack.hurtEnemy`, and the
//!   wear is in `postHurtEnemy`, which only `Player.attack` and
//!   `Player.stabAttack` call.
//! - **Piglins** fight with a golden spear through `SpearApproach(1, 10)`,
//!   `SpearAttack(1, 1, 2)` and `SpearRetreat(1)` in their fight activity,
//!   ticked every tick as brain behaviours (see [`PiglinSpear`]).
use glam::DVec3;

use super::{Mob, MobKind, MobWorld, Rng};
use crate::entity::weapon_hits::MobStab;
use crate::entity::{Ctx, Entities, EntityEvent, PlayerId};
use crate::knockback::{Push, TICK_RATE};
use crate::weapons::spear;

pub use super::head::{HEAD_ROT_SPEED, MAX_HEAD_X_ROT};
/// `SpearUseGoal`: `mob.lookAt(target, 30, 30)` and
/// `getLookControl().setLookAt(target, 30, 30)`.
pub const SPEAR_GOAL_TURN: f32 = 30.0;
/// `SpearRetreat`'s `MAX_FLEEING_TIME` and its behaviour timeout.
pub const PIGLIN_MAX_FLEEING: i32 = 100;
/// `PiglinAi.initFightActivity`: `SpearApproach(1.0, 10.0F)`,
/// `SpearAttack(1.0, 1.0, 2.0F)`, `SpearRetreat(1.0)`.
pub const PIGLIN_APPROACH: f64 = 10.0;
pub const PIGLIN_IN_RANGE: f64 = 2.0;

use super::head::{wrap_degrees, yaw_toward};

/// `Entity.calculateViewVector(xRot, yRot)`, degrees.
pub fn view_vector(x_rot: f32, y_rot: f32) -> DVec3 {
    let (x, y) = (x_rot.to_radians(), -y_rot.to_radians());
    let (y_cos, y_sin, x_cos, x_sin) = (y.cos(), y.sin(), x.cos(), x.sin());
    DVec3::new(f64::from(y_sin * x_cos), f64::from(-x_sin), f64::from(y_cos * x_cos))
}

/// What a stab remembered (`recentKineticEnemies`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stabbed {
    Player(PlayerId),
    Mob(u32),
    /// v1.0 gap Track 2c: a minecart or boat, a decoration (by its
    /// position to a sixteenth; decorations have no ids) and an End crystal.
    Cart(u32),
    Decor([i32; 3]),
    Crystal(u32),
}

/// v1.0 gap Track 2c: a non-living entity a mob's stab hurt.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Other {
    Cart(u32),
    Decor(usize),
    Crystal(u32),
}

/// `SpearAttack.SpearStatus`, the piglin's `SPEAR_STATUS` memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpearStatus {
    Approach,
    Charging,
    Retreat,
}

/// A piglin's spear behaviours and their memories (never saved, as Java
/// doesn't save `SPEAR_*`). The status outlives a fight, as Java's does:
/// only `ATTACK_TARGET` is erased when the fight activity ends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PiglinSpear {
    pub status: Option<SpearStatus>,
    /// Which of `SpearApproach`, `SpearAttack` and `SpearRetreat` run.
    pub running: [bool; 3],
    /// `SPEAR_ENGAGE_TIME`.
    pub engage: i32,
    /// `SPEAR_CHARGE_POSITION`.
    pub charge_pos: Option<DVec3>,
    /// `SPEAR_FLEEING_TIME` and `SPEAR_FLEEING_POSITION`.
    pub flee_time: i32,
    pub flee_pos: Option<DVec3>,
}

const APPROACH: usize = 0;
const ATTACK: usize = 1;
const RETREAT: usize = 2;

impl Mob {
    /// Whether the main hand holds a `KineticWeapon` (a spear): an equipped
    /// spear, or the golden spear of a piglin's or zombified piglin's
    /// [`crate::entity::nether::Weapon`].
    pub(crate) fn holds_spear(&self) -> Option<crate::item::Tier> {
        use crate::entity::nether::Weapon;
        if let Some(stack) = self.equipment.main_hand() {
            return crate::weapons::spear_tier(stack.item);
        }
        let weapon = match self.kind {
            MobKind::ZombifiedPiglin => self.held,
            _ => self.nether.as_ref().map(|n| n.weapon),
        };
        (weapon == Some(Weapon::GoldenSpear)).then_some(crate::item::Tier::Gold)
    }

    /// The eye (Java's eye height is close to 0.85 of the box for humanoids).
    pub(crate) fn spear_eye(&self) -> DVec3 {
        self.pos + DVec3::Y * (self.shape().height * 0.85)
    }

    /// `Mob.lookAt(entity, yMax, xMax)` on the body's `yRot` (its `xRot`
    /// is overwritten by the look control the same tick).
    pub(crate) fn turn_body_to(&mut self, at: DVec3, max: f32) {
        if let Some(yaw) = yaw_toward(at - self.pos) {
            let r = self.yrot();
            self.set_yrot(r + wrap_degrees(yaw - r).clamp(-max, max));
        }
    }

    /// `getHeadLookAngle` and `getLookAngle` (v1.0 gap Track 2c: every
    /// mob keeps Java's `yHeadRot` and `xRot`).
    fn spear_looks(&self) -> (DVec3, DVec3) {
        let (pitch, head) = (self.look.pitch, self.look.head);
        (view_vector(pitch, head), view_vector(pitch, self.yrot()))
    }

    /// `startUsingItem(MAIN_HAND)` with a spear: a fresh contact memory.
    pub(crate) fn lower_spear(&mut self) {
        self.spear_use = Some(super::chase::SpearCharge::default());
    }
}

/// A living mob as a spear sees it during one stab pass.
#[derive(Clone, Copy, Debug)]
struct StabView {
    uid: u32,
    /// The mob at the bottom of its vehicle stack (its own uid when on foot).
    root: u32,
    /// `getMotion`: the root vehicle's motion, blocks a second.
    motion: DVec3,
    riding: bool,
}

impl Entities {
    /// The index of the mob at the bottom of mob `i`'s vehicle stack (its
    /// own when on foot; carts count as no mob vehicle).
    fn stab_root(&self, mut i: usize, uids: &rustc_hash::FxHashMap<u32, usize>) -> usize {
        for _ in 0..8 {
            let Some(v) = self.mobs[i].riding.filter(|v| v & crate::entity::mounts::MOB_VEHICLE != 0) else { break };
            let Some(&j) = uids.get(&(v & !crate::entity::mounts::MOB_VEHICLE)) else { break };
            i = j;
        }
        i
    }

    /// One tick of every lowered mob spear (`LivingEntity.updatingUsingItem`
    /// → `KineticWeapon.damageEntities`), before the mobs move, as Java's
    /// `LivingEntity.tick` runs it before `aiStep`.
    pub(crate) fn mob_spear_stabs<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.spear_use.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        let uids: rustc_hash::FxHashMap<u32, usize> = self.mobs.iter().enumerate().map(|(i, m)| (m.uid, i)).collect();
        let views: Vec<StabView> = (0..self.mobs.len())
            .map(|i| {
                let root = self.stab_root(i, &uids);
                let m = &self.mobs[i];
                StabView {
                    uid: m.uid,
                    root: self.mobs[root].uid,
                    motion: self.mobs[root].vel,
                    riding: m.riding.is_some(),
                }
            })
            .collect();
        for i in 0..self.mobs.len() {
            if self.mobs[i].spear_use.is_none() || !self.mobs[i].simulation_active || !self.mobs[i].alive() {
                continue;
            }
            let Some(tier) = self.mobs[i].holds_spear() else {
                self.mobs[i].spear_use = None;
                continue;
            };
            let Some(mut charge) = self.mobs[i].spear_use.take() else { continue };
            let used = charge.used;
            charge.used += 1;
            let kinetic = spear::kinetic(tier);
            // Nothing is touched, or remembered, before the delay.
            if used < kinetic.delay_ticks {
                self.mobs[i].spear_use = Some(charge);
                continue;
            }
            let me = views[i];
            let m = &self.mobs[i];
            let (head, body) = m.spear_looks();
            let eye = m.spear_eye();
            let along = body.dot(me.motion);
            // `getKnownMovement` is the mob's own motion (none while riding).
            let forward = head.dot(m.vel) / TICK_RATE;
            let range = spear::mob_range();
            let block = spear::block_distance(world, eye, head, range.max + forward.max(0.0));
            let reach = range.max + forward.max(0.0);
            let end = block.map_or(reach, |b| b.min(reach));
            let base = m.kind.melee().0;
            let enchants = m.main_hand().map(|s| s.active_enchants()).unwrap_or_default();
            let sharpness = crate::enchant::damage_bonus(enchants, crate::enchant::Creature::Other);
            let extra = crate::knockback::attack_strength(m.attack_knockback(), 0, false);
            let fire = m.weapon_fire_seconds();
            let (origin, yaw, uid, cause) = (m.pos, m.yrot(), m.uid, m.melee_attack().1);
            let mut affected = false;
            let mut knocked = 0;
            let mut others: Vec<(Other, f32)> = Vec::new();
            let fresh = |charge: &mut super::chase::SpearCharge, key: Stabbed| {
                // `wasRecentlyStabbed`, then `rememberStabbedEntity`.
                if charge.stabbed.iter().any(|&(k, at)| k == key && used - at < kinetic.contact_cooldown) {
                    return false;
                }
                charge.stabbed.retain(|&(k, _)| k != key);
                charge.stabbed.push((key, used));
                true
            };
            if end >= range.min {
                let from = eye + head * range.min;
                let span = head * (end - range.min);
                // v1.0 gap Track 2c: the margin's block check.
                let crosses = |min: DVec3, max: DVec3| spear::stab_hits(world, from, span, min, max, range.margin);
                // Players (`canHitEntity`: alive and not spectating).
                for t in ctx.players.iter().filter(|t| t.alive && !t.spectator) {
                    let half = DVec3::new(t.shape.half_width, 0.0, t.shape.half_width);
                    if !crosses(t.pos - half, t.pos + half + DVec3::Y * t.shape.height)
                        || !fresh(&mut charge, Stabbed::Player(t.id))
                    {
                        continue;
                    }
                    let Some(contact) = kinetic.contact(used, along, body.dot(t.vel), base, false) else { continue };
                    affected = true;
                    if contact.dismount && t.passenger {
                        events.push(EntityEvent::PlayerDismount { player: t.id });
                    }
                    if contact.damage {
                        events.push(EntityEvent::PlayerHit {
                            java: Some(crate::knockback::Hit {
                                pushes: [Some(Push::hurt(origin, t.pos)), None],
                                punch: None,
                                throw: None,
                            }),
                            player: t.id,
                            damage: contact.amount + sharpness,
                            knockback: ((t.pos - origin).with_y(0.0).normalize_or_zero() * 6.0 + DVec3::Y * 5.0)
                                .as_vec3(),
                            cause,
                        });
                        events.push(EntityEvent::OwnerHurt { player: t.id, attacker: uid });
                        if fire > 0.0 {
                            events.push(EntityEvent::Ignite { player: t.id, secs: fire });
                        }
                    }
                    if contact.knockback {
                        knocked += 1 + usize::from(extra > 0.0);
                        events.push(EntityEvent::PlayerShove {
                            player: t.id,
                            pushes: [
                                Some(Push::facing(crate::knockback::DEFAULT_STRENGTH, yaw)),
                                (extra > 0.0).then(|| Push::facing(f64::from(extra), yaw)),
                            ],
                        });
                    }
                }
                // Mobs, nearest first, but never the spear's own mount or
                // its fellow passengers (`isPassengerOfSameVehicle`).
                let targets = self.spear_targets(world, eye, head, range, forward, block, Some(uid));
                for j in targets {
                    let v = views[j];
                    if v.root == me.root || !fresh(&mut charge, Stabbed::Mob(v.uid)) {
                        continue;
                    }
                    let Some(contact) = kinetic.contact(used, along, body.dot(v.motion), base, false) else {
                        continue;
                    };
                    affected = true;
                    if contact.knockback {
                        knocked += 1 + usize::from(extra > 0.0);
                    }
                    events.push(EntityEvent::MobStab(MobStab {
                        target: v.uid,
                        attacker: uid,
                        damage: contact.damage.then_some(contact.amount + sharpness),
                        origin,
                        yaw,
                        knockback: contact.knockback,
                        extra,
                        dismount: contact.dismount && v.riding,
                    }));
                }
                // v1.0 gap Track 2c: the entities that aren't living but are
                // pickable (`PiercingWeapon.canHitEntity`): vehicles,
                // decorations and End crystals. `stabAttack` only hurts them
                // (knockback and `hurtEnemy` are for living targets); the
                // mob's own cart is its vehicle (`isPassengerOfSameVehicle`).
                let own_cart = self.mobs[i].riding.filter(|v| v & crate::entity::mounts::MOB_VEHICLE == 0);
                for c in &self.minecarts {
                    let (min, max) = c.aabb();
                    if Some(c.id) == own_cart || !crosses(min, max) || !fresh(&mut charge, Stabbed::Cart(c.id)) {
                        continue;
                    }
                    // Carts keep blocks a tick; mob speeds here are a second's.
                    let motion = c.vel * TICK_RATE;
                    let Some(contact) = kinetic.contact(used, along, body.dot(motion), base, false) else {
                        continue;
                    };
                    affected |= contact.damage || contact.knockback;
                    if contact.damage {
                        others.push((Other::Cart(c.id), contact.amount + sharpness));
                    }
                }
                for (k, d) in self.decor.iter().enumerate() {
                    let (min, max) = d.aabb();
                    let key = Stabbed::Decor((d.pos * 16.0).floor().as_ivec3().to_array());
                    if !crosses(min, max) || !fresh(&mut charge, key) {
                        continue;
                    }
                    let Some(contact) = kinetic.contact(used, along, 0.0, base, false) else { continue };
                    affected |= contact.damage || contact.knockback;
                    if contact.damage {
                        others.push((Other::Decor(k), contact.amount + sharpness));
                    }
                }
                for c in &self.crystals.list {
                    let (min, max) = c.aabb();
                    if !crosses(min, max) || !fresh(&mut charge, Stabbed::Crystal(c.id)) {
                        continue;
                    }
                    let Some(contact) = kinetic.contact(used, along, 0.0, base, false) else { continue };
                    affected |= contact.damage || contact.knockback;
                    if contact.damage {
                        others.push((Other::Crystal(c.id), contact.amount + sharpness));
                    }
                }
            }
            self.stab_others(uid, std::mem::take(&mut others));
            let m = &mut self.mobs[i];
            // `causeExtraKnockback` slows the attacker for each push it makes.
            for _ in 0..knocked {
                m.vel = crate::knockback::slow_attacker(m.vel);
            }
            if affected {
                m.attack_anim = 0.35;
            }
            m.spear_use = Some(charge);
        }
    }

    /// `LivingEntity.stabAttack` on a mob, after `resolve_mob_hits` has dealt
    /// any damage: the 0.4 push along the attacker's facing and the attack
    /// knockback, then the dismount.
    pub(crate) fn finish_mob_stab(&mut self, stab: MobStab) {
        let Some(i) = self.mobs.iter().position(|m| m.uid == stab.target && m.alive()) else { return };
        if stab.knockback {
            let pushes = [
                Some(Push::facing(crate::knockback::DEFAULT_STRENGTH, stab.yaw)),
                (stab.extra > 0.0).then(|| Push::facing(f64::from(stab.extra), stab.yaw)),
            ];
            for push in pushes.into_iter().flatten() {
                self.mobs[i].knock_back(push, &mut self.rng);
            }
        }
        if stab.dismount {
            self.mobs[i].riding = None;
        }
    }

    /// v1.0 gap Track 2c: `hurtServer` on the non-living entities mob
    /// `attacker`'s stab damaged: a vehicle (`VehicleEntity.hurtServer`, ten
    /// times the damage, breaking past 40), a decoration
    /// (`Entities::mob_hurt_decor`) and an End crystal (any blow but the
    /// dragon's sets it off).
    fn stab_others(&mut self, attacker: u32, hits: Vec<(Other, f32)>) {
        let source = crate::world::vibration::EventSource::mob(attacker);
        let mut decor: Vec<usize> = Vec::new();
        for (hit, damage) in hits {
            match hit {
                Other::Cart(id) => {
                    if let Some(at) = self.cart(id).map(|c| c.pos) {
                        self.sculk.post(at, crate::world::vibration::GameEvent::EntityDamage, source, None);
                    }
                    self.shoot_cart(id, damage, false, false, false, DVec3::ZERO);
                }
                Other::Decor(k) => decor.push(k),
                Other::Crystal(id) => {
                    self.crystals.hit(id, crate::entity::crystal::Blow::Other);
                }
            }
        }
        // Highest index first, so a removal never moves one still to come.
        decor.sort_unstable_by(|a, b| b.cmp(a));
        decor.dedup();
        for k in decor {
            self.mob_hurt_decor(k, source);
        }
    }
}

impl Mob {
    /// `ableToAttack`: an attack target and a kinetic weapon.
    fn piglin_spear_able(&self) -> bool {
        self.holds_spear().is_some()
    }

    /// Stops every running spear behaviour (the fight ended: `ableToAttack`
    /// fails without an attack target), with each behaviour's `stop`.
    pub(crate) fn stop_piglin_spear(&mut self) {
        let Some(mut s) = self.nether.as_ref().map(|n| n.spear) else { return };
        if !s.running.iter().any(|&r| r) {
            return;
        }
        self.piglin_spear_stop(&mut s);
        if let Some(n) = self.nether.as_mut() {
            n.spear = s;
        }
    }

    fn piglin_spear_stop(&mut self, s: &mut PiglinSpear) {
        if s.running[APPROACH] {
            s.running[APPROACH] = false;
            self.stop_navigation();
            s.status = Some(SpearStatus::Charging);
        }
        if s.running[ATTACK] {
            s.running[ATTACK] = false;
            self.stop_navigation();
            self.spear_use = None;
            s.charge_pos = None;
            s.engage = 0;
            s.status = Some(SpearStatus::Retreat);
        }
        if s.running[RETREAT] {
            s.running[RETREAT] = false;
            self.stop_navigation();
            self.spear_use = None;
            s.flee_time = 0;
            s.flee_pos = None;
            s.status = None;
        }
    }

    /// One brain tick of the piglin's spear behaviours toward a target with
    /// feet at `at` and eyes at `eye`: each behaviour that isn't running
    /// tries to start, in the activity's order, then each running one stops
    /// (`canStillUse`, the retreat's 100-tick timeout) or ticks. Returns the
    /// movement when a behaviour moved the piglin this tick.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn piglin_spear<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        at: DVec3,
        eye: DVec3,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        let mut s = self.nether.as_ref()?.spear;
        let able = self.piglin_spear_able();
        let dist2 = self.pos.distance_squared(at);
        let mount = if self.riding.is_some() { 2.0 } else { 0.0 };
        // `startEachNonRunningBehavior`.
        if !s.running[APPROACH] && s.status.is_none() && able && self.spear_use.is_none() {
            s.status = Some(SpearStatus::Approach);
            s.running[APPROACH] = true;
        }
        if !s.running[ATTACK] && s.status == Some(SpearStatus::Charging) && able && self.spear_use.is_none() {
            let kinetic = spear::kinetic(self.holds_spear()?);
            s.engage = kinetic.damage_use_duration() as i32;
            s.charge_pos = None;
            self.lower_spear();
            s.running[ATTACK] = true;
        }
        if !s.running[RETREAT] && s.status == Some(SpearStatus::Retreat) && able && self.spear_use.is_none() {
            let d = dist2.sqrt();
            if let Some(away) =
                self.land_pos_away(world, ctx, rng, (9.0 + mount - d).max(0.0), (11.0 + mount - d).max(1.0), 7, at)
            {
                s.flee_pos = Some(away);
                s.flee_time = 0;
                s.running[RETREAT] = true;
            }
        }
        // `tickEachRunningBehavior`.
        let mut wish = None;
        if s.running[APPROACH] {
            if !(able && dist2 > PIGLIN_APPROACH * PIGLIN_APPROACH) {
                s.running[APPROACH] = false;
                self.stop_navigation();
                s.status = Some(SpearStatus::Charging);
            } else {
                self.set_look_at(eye, HEAD_ROT_SPEED, MAX_HEAD_X_ROT);
                wish = Some(self.spear_move(world, at, 1.0));
            }
        }
        if s.running[ATTACK] {
            if !(s.engage > 0 && able) {
                s.running[ATTACK] = false;
                self.stop_navigation();
                self.spear_use = None;
                s.charge_pos = None;
                s.engage = 0;
                s.status = Some(SpearStatus::Retreat);
            } else {
                self.set_look_at(eye, HEAD_ROT_SPEED, MAX_HEAD_X_ROT);
                s.engage -= 1;
                if let Some(away) = s.charge_pos {
                    wish = Some(self.spear_move(world, away, 1.0));
                    if self.nav.is_done() {
                        s.charge_pos = None;
                    }
                } else {
                    wish = Some(self.spear_move(world, at, 1.0));
                    if dist2 < PIGLIN_IN_RANGE * PIGLIN_IN_RANGE || self.nav.is_done() {
                        let d = dist2.sqrt();
                        s.charge_pos = self.land_pos_away(world, ctx, rng, 6.0 + mount - d, 7.0 + mount - d, 7, at);
                    }
                }
            }
        }
        if s.running[RETREAT] {
            // `canStillUse` (fleeing time under 100, which comes before the
            // behaviour's own 100-tick timeout): the navigation it checks on
            // its first tick is the walk-target sink's path to the target,
            // which [`Mob::nether_attack`] keeps when no behaviour moves.
            if !(s.flee_time < PIGLIN_MAX_FLEEING && s.flee_pos.is_some() && !self.nav.is_done() && able) {
                s.running[RETREAT] = false;
                self.stop_navigation();
                self.spear_use = None;
                s.flee_time = 0;
                s.flee_pos = None;
                s.status = None;
            } else {
                self.set_look_at(eye, HEAD_ROT_SPEED, MAX_HEAD_X_ROT);
                s.flee_time += 1;
                if let Some(flee) = s.flee_pos {
                    wish = Some(self.spear_move(world, flee, 1.0));
                }
            }
        }
        if let Some(n) = self.nether.as_mut() {
            n.spear = s;
        }
        wish
    }
}

#[cfg(test)]
mod tests;
