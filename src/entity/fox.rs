//! v1.0 gap Track 2e: Java 26.3's fox hunting AI and states (`Fox`).
//!
//! The fox's flags (`DATA_FLAGS_ID`: sitting, crouching, interested,
//! pouncing, faceplanted; sleeping stays [`super::animals::State::sleeping`]),
//! the client animation values `Fox.tick` eases (`crouchAmount`,
//! `interestedAngle`), its target goals (`NearestAttackableTargetGoal` for
//! chickens and rabbits, baby turtles on land and schooling fish, in the
//! variant's priorities) and what its goals sense from the mobs around it
//! (`FoxBehaviorGoal.alertable`, the items `FoxSearchForItemsGoal` walks to).
//! The goals themselves are in `mob::fox`.
use super::{Entities, Mob, MobKind, MobWorld};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

/// `Fox.MAX_CROUCH_AMOUNT`.
pub const MAX_CROUCH_AMOUNT: f32 = 5.0;
/// `NearestAttackableTargetGoal`'s follow range (`FOLLOW_RANGE` 32).
pub const FOLLOW_RANGE: f64 = 32.0;
/// `FoxBehaviorGoal.alertableTargeting`: 12 blocks, the box inflated 12, 6, 12.
const ALERT_RANGE: f64 = 12.0;

/// v1.0 gap Track 2f: the fox's own sounds (`SoundEvents.FOX_*` beyond
/// its ambient, hurt and death calls), in the established procedural style.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cue {
    /// `DefendTrustedTargetGoal.start`, and one tick in 20 while defending.
    Aggro,
    /// `FoxMeleeAttackGoal.checkAndPerformAttack`.
    Bite,
    /// `playEatingSound`: chewing what it holds before it eats it.
    Eat,
    /// The night call (`getAmbientSound`: one in ten at night with no
    /// player within 16 blocks), at volume 2.
    Screech,
    /// The ambient sound of a sleeping fox.
    Sleep,
    /// `FoxEatBerriesGoal.tick`, on the way to the berries.
    Sniff,
    /// `spitOutItem`, dropping what it held for a better item.
    Spit,
}

impl Cue {
    pub const ALL: [Cue; 7] = [Cue::Aggro, Cue::Bite, Cue::Eat, Cue::Screech, Cue::Sleep, Cue::Sniff, Cue::Spit];

    /// The Java sound event.
    pub fn key(self) -> &'static str {
        match self {
            Cue::Aggro => "entity.fox.aggro",
            Cue::Bite => "entity.fox.bite",
            Cue::Eat => "entity.fox.eat",
            Cue::Screech => "entity.fox.screech",
            Cue::Sleep => "entity.fox.sleep",
            Cue::Sniff => "entity.fox.sniff",
            Cue::Spit => "entity.fox.spit",
        }
    }
}

/// What a target goal hunts (`Fox.setTargetGoals`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hunt {
    /// v1.0 gap Track 2f: `DefendTrustedTargetGoal` (priority 3): the mob
    /// that last hurt the fox's first trusted player.
    Defend,
    /// `landTargetGoal`: chickens and rabbits.
    Land,
    /// `turtleEggTargetGoal`: baby turtles on land (`BABY_ON_LAND_SELECTOR`).
    Turtle,
    /// `fishTargetGoal`: `AbstractSchoolingFish` (cod, salmon, tropical fish).
    Fish,
}

impl Hunt {
    fn matches(self, m: &Mob) -> bool {
        match self {
            Hunt::Land => matches!(m.kind, MobKind::Chicken | MobKind::Rabbit),
            Hunt::Turtle => m.kind == MobKind::Turtle && m.is_baby() && !m.in_water,
            Hunt::Fish => matches!(m.kind, MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish),
            Hunt::Defend => false,
        }
    }
}

/// The target goals in the variant's order (red: land and turtle at 4,
/// fish at 6; snow: fish at 4, land and turtle at 6).
pub(crate) fn hunts(variant: u8) -> [(u8, Hunt); 3] {
    if variant == 0 {
        [(4, Hunt::Land), (4, Hunt::Turtle), (6, Hunt::Fish)]
    } else {
        [(4, Hunt::Fish), (6, Hunt::Land), (6, Hunt::Turtle)]
    }
}

/// The fox's target (`Mob.getTarget`), refreshed by the entity pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Prey {
    pub uid: u32,
    pub kind: MobKind,
    pub pos: DVec3,
    pub vel: DVec3,
    pub eye: f64,
    pub half_width: f64,
    pub height: f64,
    /// The target goal that picked it and its priority.
    pub hunt: Hunt,
    pub priority: u8,
    /// v1.0 gap Track 2g: a player the fox defends a trusted player from
    /// (`uid` is then unused).
    pub player: Option<super::PlayerId>,
}

/// v1.0 gap Track 2g: who last hurt a player (`LivingEntity.lastHurtByMob`):
/// a mob, by melee or its projectile, or another player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attacker {
    Mob(u32),
    Player(super::PlayerId),
}

/// The running goal outside the shared table, with its state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Running {
    /// `FaceplantGoal` (1): `countdown` full updates.
    Faceplant { countdown: i32 },
    /// `StalkPreyGoal` (5).
    Stalk,
    /// `FoxPounceGoal` (6, not interruptable).
    Pounce,
    /// `SeekShelterGoal` (6), walking to its hiding place.
    Shelter,
    /// `FoxMeleeAttackGoal` (7): `ticksUntilNextPathRecalculation` and
    /// `ticksUntilNextAttack`.
    Melee { repath: i32, attack: i32 },
    /// `SleepGoal` (7).
    Sleep,
    /// `FoxEatBerriesGoal` (10), a `MoveToBlockGoal(1.2, 12, 1)`: the bush
    /// or vine, `tryTicks`, `maxStayTicks`, `ticksWaited`.
    Berries { block: glam::IVec3, tries: i32, stay: i32, waited: i32 },
    /// `LeapAtTargetGoal` (10), until it lands.
    Leap,
    /// `FoxSearchForItemsGoal` (11).
    Items,
    /// `PerchAndSearchGoal` (13): `relX`, `relZ`, `lookTime`, `looksRemaining`.
    Perch { rel: (f64, f64), look: i32, looks: i32 },
    /// v1.0 gap Track 2f: `FoxStrollThroughVillageGoal` (9), `wantedPos`.
    VillageStroll { wanted: glam::IVec3 },
}

impl Running {
    pub(crate) fn priority(self) -> u8 {
        match self {
            Running::Faceplant { .. } => 1,
            Running::Stalk => 5,
            Running::Pounce | Running::Shelter => 6,
            Running::Melee { .. } | Running::Sleep => 7,
            Running::VillageStroll { .. } => 9,
            Running::Leap | Running::Berries { .. } => 10,
            Running::Items => 11,
            Running::Perch { .. } => 13,
        }
    }

    /// `Goal.isInterruptable` (only `FoxPounceGoal` refuses).
    pub(crate) fn interruptable(self) -> bool {
        !matches!(self, Running::Pounce)
    }
}

/// The fox's state beside [`super::animals::State`].
#[derive(Clone, Debug, Default)]
pub struct State {
    /// `isSitting` (saved as `Sitting`): `PerchAndSearchGoal`.
    pub sitting: bool,
    /// `isCrouching` (saved as `Crouching`): stalking, then ready to pounce.
    pub crouching: bool,
    /// `isInterested`: the head tilt while it stalks.
    pub interested: bool,
    /// `isPouncing`.
    pub pouncing: bool,
    /// `isFaceplanted`: a pounce into snow.
    pub faceplanted: bool,
    /// v1.0 gap Track 2f: `isDefending` (`FLAG_DEFENDING`): it targets what
    /// hurt a player it trusts, and doesn't flee or panic meanwhile.
    pub defending: bool,
    /// `crouchAmount` and its last tick's value (0..=5, eased 0.2 a tick).
    pub crouch_amount: f32,
    pub crouch_amount_o: f32,
    /// `interestedAngle` and its last tick's value (eased 0.4 of the way).
    pub interested_angle: f32,
    pub interested_angle_o: f32,
    pub(crate) target: Option<Prey>,
    pub(crate) running: Option<Running>,
    /// `SleepGoal.countdown` (full updates; rolled on the first tick).
    pub(crate) sleep_countdown: Option<i32>,
    /// `SeekShelterGoal.interval` (full updates).
    pub(crate) shelter_interval: Option<i32>,
    /// `MoveToBlockGoal.nextStartTick` of `FoxEatBerriesGoal` (full updates).
    pub(crate) berries_next: i32,
    /// The hiding place `SeekShelterGoal` walks to.
    pub(crate) shelter_at: Option<DVec3>,
    /// `MeleeAttackGoal.lastCanUseCheck` (game ticks of the fox).
    pub(crate) melee_check: Option<u32>,
    /// Sensed by the entity pass: `FoxBehaviorGoal.alertable`.
    pub(crate) alertable: bool,
    /// Sensed by the entity pass: the first pickable item within 8 blocks.
    pub(crate) item_near: Option<DVec3>,
    /// The village test of `SeekShelterGoal` at the fox (`isVillage`).
    pub(crate) in_village: bool,
    /// The move input of the last tick was zero (`xxa`, `yya`, `zza`).
    pub(crate) still: bool,
    /// The fox's own tick count (for the melee goal's 20-tick check).
    pub(crate) ticks: u32,
    /// v1.0 gap Track 2f: `level.isThundering()`.
    pub(crate) thundering: bool,
    /// v1.0 gap Track 2f: `DefendTrustedTargetGoal.timestamp` (the trusted
    /// player's hurt it last answered).
    pub(crate) defend_stamp: Option<u32>,
    /// v1.0 gap Track 2f: `FoxStrollThroughVillageGoal.canUse`'s roll and
    /// spot, found by the entity pass: `wantedPos` near a village at night.
    pub(crate) village_stroll: Option<glam::IVec3>,
}

impl State {
    /// `isFullyCrouched`.
    pub fn fully_crouched(&self) -> bool {
        self.crouch_amount == MAX_CROUCH_AMOUNT
    }

    /// `Fox.getCrouchAmount(partialTick)` for the model.
    pub fn crouch(&self, alpha: f32) -> f32 {
        self.crouch_amount_o + (self.crouch_amount - self.crouch_amount_o) * alpha
    }

    /// `Fox.getHeadRollAngle(partialTick)` for the model (radians).
    pub fn head_roll(&self, alpha: f32) -> f32 {
        let a = self.interested_angle_o + (self.interested_angle - self.interested_angle_o) * alpha;
        a * 0.11 * std::f32::consts::PI
    }

    /// `Fox.clearStates` (sleeping is cleared by the caller).
    pub(crate) fn clear(&mut self) {
        self.interested = false;
        self.crouching = false;
        self.sitting = false;
        self.defending = false;
        self.faceplanted = false;
    }

    /// `Fox.tick`'s client values, both sides.
    pub(crate) fn animate(&mut self) {
        self.interested_angle_o = self.interested_angle;
        let to = if self.interested { 1.0 } else { 0.0 };
        self.interested_angle += (to - self.interested_angle) * 0.4;
        self.crouch_amount_o = self.crouch_amount;
        if self.crouching {
            self.crouch_amount = (self.crouch_amount + 0.2).min(MAX_CROUCH_AMOUNT);
        } else {
            self.crouch_amount = 0.0;
        }
    }

    /// `Fox.canMove`: not sleeping (passed in), sitting or faceplanted.
    pub(crate) fn can_move(&self, sleeping: bool) -> bool {
        !sleeping && !self.sitting && !self.faceplanted
    }

    /// The saved flags Java writes (`Sleeping`, `Sitting`, `Crouching`),
    /// only when set.
    pub(crate) fn save_into(&self, v: &mut Value, sleeping: bool) {
        if sleeping {
            v["fox_sleeping"] = json!(true);
        }
        if self.sitting {
            v["fox_sitting"] = json!(true);
        }
        if self.crouching {
            v["fox_crouching"] = json!(true);
        }
    }

    /// Reads [`State::save_into`]; returns `Sleeping`.
    pub(crate) fn load(&mut self, v: &Value) -> bool {
        self.sitting = v["fox_sitting"] == true;
        self.crouching = v["fox_crouching"] == true;
        v["fox_sleeping"] == true
    }

    /// What LAN clients draw (Track 7d's model reads these).
    pub(crate) fn look(&self) -> Value {
        json!({
            "sitting": self.sitting,
            "crouching": self.crouching,
            "interested": self.interested,
            "pouncing": self.pouncing,
            "faceplanted": self.faceplanted,
            "defending": self.defending,
            "crouch": self.crouch_amount,
            "interest": self.interested_angle,
        })
    }

    pub(crate) fn apply_look(&mut self, v: &Value) {
        self.sitting = v["sitting"] == true;
        self.crouching = v["crouching"] == true;
        self.interested = v["interested"] == true;
        self.pouncing = v["pouncing"] == true;
        self.faceplanted = v["faceplanted"] == true;
        self.defending = v["defending"] == true;
        let num = |k: &str, max: f32| {
            v[k].as_f64().filter(|n| n.is_finite()).unwrap_or(0.0).clamp(0.0, f64::from(max)) as f32
        };
        (self.crouch_amount_o, self.crouch_amount) = (self.crouch_amount, num("crouch", MAX_CROUCH_AMOUNT));
        (self.interested_angle_o, self.interested_angle) = (self.interested_angle, num("interest", 1.0));
    }
}

/// `FoxAlertableEntitiesSelector` for a mob: what keeps a fox awake and
/// from perching (players are tested apart).
fn alerts(m: &Mob) -> bool {
    match m.kind {
        MobKind::Fox => false,
        MobKind::Chicken | MobKind::Rabbit => true,
        _ if m.kind.is_hostile() => true,
        MobKind::Wolf | MobKind::Cat | MobKind::Parrot => m.animal.as_ref().is_none_or(|a| a.owner.is_none()),
        // `!target.isSleeping() && !target.isDiscrete()`.
        _ => !(m.villager.as_ref().is_some_and(|v| v.sleeping) || m.animal.as_ref().is_some_and(|a| a.sleeping)),
    }
}

impl Entities {
    /// v1.0 gap Track 2g: player `attacker` hurt player `victim` (a PvP hit
    /// that landed): `victim`'s `lastHurtByMob`, which foxes that trust it
    /// defend against.
    pub fn note_player_hurt_by_player(&mut self, victim: super::PlayerId, attacker: super::PlayerId) {
        if victim == attacker {
            return;
        }
        self.player_hurts = self.player_hurts.wrapping_add(1);
        self.player_hurt_by.insert(victim, (Attacker::Player(attacker), self.player_hurts, 0));
    }

    /// The foxes' targets and senses before the mobs think: each target goal
    /// keeps a living target within its follow range and, without one, looks
    /// on the fox's full updates one time in `reducedTickDelay(10)` for the
    /// nearest match to its eyes (no sight needed); `alertable` and the items
    /// within 8 blocks.
    pub(super) fn fox_upkeep(&mut self, ctx: &super::Ctx) {
        // v1.0 gap Track 2f: `LivingEntity.baseTick` forgets a player's
        // `lastHurtByMob` 100 ticks after the hurt.
        self.player_hurt_by.retain(|_, (_, _, age)| {
            *age += 1;
            *age <= 100
        });
        if !self.mobs.iter().any(|m| m.fox.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        let by_uid: rustc_hash::FxHashMap<u32, usize> = self.mobs.iter().enumerate().map(|(i, m)| (m.uid, i)).collect();
        for i in 0..self.mobs.len() {
            if self.mobs[i].fox.is_none() || !self.mobs[i].alive() || !self.mobs[i].simulation_active {
                continue;
            }
            let m = &self.mobs[i];
            let (pos, uid) = (m.pos, m.uid);
            let eye = m.look_eye();
            let full = m.goals.full(uid);
            let variant = m.animal.as_ref().map_or(0, |a| a.variant);
            let prey = |j: usize, hunt: Hunt, priority: u8| {
                let t = &self.mobs[j];
                let s = t.shape();
                Prey {
                    uid: t.uid,
                    kind: t.kind,
                    pos: t.pos,
                    vel: t.vel,
                    eye: t.eye_height(),
                    half_width: s.half_width,
                    height: s.height,
                    hunt,
                    priority,
                    player: None,
                }
            };
            // v1.0 gap Track 2g: a player target (a trusted player's attacker).
            let player_prey = |t: &super::Target, hunt: Hunt, priority: u8| Prey {
                uid: u32::MAX,
                kind: MobKind::Fox,
                pos: t.pos,
                vel: t.vel,
                eye: t.eye_height(),
                half_width: t.shape.half_width,
                height: t.shape.height,
                hunt,
                priority,
                player: Some(t.id),
            };
            // `TargetGoal.canContinueToUse`: alive and within the follow range.
            let kept = m.fox.as_ref().unwrap().target.and_then(|p| {
                if let Some(id) = p.player {
                    // `canAttack`: a living player that isn't creative or
                    // spectating.
                    let t = ctx.players.iter().find(|t| t.id == id)?;
                    return (t.alive && t.targetable && t.pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE)
                        .then(|| player_prey(t, p.hunt, p.priority));
                }
                let &j = by_uid.get(&p.uid)?;
                let t = &self.mobs[j];
                (t.alive() && t.pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE)
                    .then(|| prey(j, p.hunt, p.priority))
            });
            let mut target = kept;
            // v1.0 gap Track 2f: `DefendTrustedTargetGoal` (priority 3, one
            // time in `reducedTickDelay(10)`): the first trusted player in
            // the world, hurt by a mob since the hurt it last answered, and
            // that mob within the follow range.
            let mut defended = None;
            if full && target.is_none_or(|t| t.priority > 3) && self.rng.next_int(5) == 0 {
                let trusted = m.animal.as_ref().map(|a| a.trusted).unwrap_or_default();
                let stamp = m.fox.as_ref().unwrap().defend_stamp;
                if let Some(p) = trusted.iter().flatten().find(|id| ctx.players.iter().any(|t| t.id == **id))
                    && let Some(&(attacker, seq, _)) = self.player_hurt_by.get(p)
                    && Some(seq) != stamp
                {
                    match attacker {
                        Attacker::Mob(a) => {
                            if let Some(&j) = by_uid.get(&a)
                                && a != uid
                                && self.mobs[j].alive()
                                && self.mobs[j].pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE
                            {
                                target = Some(prey(j, Hunt::Defend, 3));
                                defended = Some(seq);
                            }
                        }
                        // Track 2g: another player, unless the fox trusts it
                        // (the goal's selector) or can't attack it.
                        Attacker::Player(a) => {
                            if let Some(t) = ctx.players.iter().find(|t| t.id == a)
                                && !trusted.contains(&Some(a))
                                && t.alive
                                && t.targetable
                                && t.pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE
                            {
                                target = Some(player_prey(t, Hunt::Defend, 3));
                                defended = Some(seq);
                            }
                        }
                    }
                }
            }
            if full {
                for (priority, hunt) in hunts(variant) {
                    // A target goal starts only over a worse running one.
                    if target.is_some_and(|t| t.priority <= priority) {
                        continue;
                    }
                    if self.rng.next_int(5) != 0 {
                        continue;
                    }
                    let found = self
                        .mob_index
                        .nearest(&self.mobs, eye, FOLLOW_RANGE + 2.0, |t| {
                            t.alive()
                                && t.uid != uid
                                && hunt.matches(t)
                                && (t.pos - pos).abs().max_element() <= FOLLOW_RANGE + 0.5
                                && t.pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE
                        })
                        .map(|j| prey(j, hunt, priority));
                    if found.is_some() {
                        target = found;
                        break;
                    }
                }
            }
            let fox = &self.mobs[i];
            let alertable = self
                .mob_index
                .nearest(&self.mobs, pos, ALERT_RANGE + 6.0, |t| {
                    let d = t.pos - pos;
                    t.alive()
                        && t.uid != uid
                        && d.x.abs() <= ALERT_RANGE + 0.5
                        && d.z.abs() <= ALERT_RANGE + 0.5
                        && d.y.abs() <= 6.0 + 1.0
                        && t.pos.distance_squared(pos) <= ALERT_RANGE * ALERT_RANGE
                        && alerts(t)
                })
                .is_some()
                || ctx.players.iter().any(|t| {
                    let trusted = fox.animal.as_ref().is_some_and(|a| a.trusted.contains(&Some(t.id)));
                    // `forCombat` skips creative and spectating players.
                    t.alive
                        && t.targetable
                        && !trusted
                        && !t.sneaking
                        && t.pos.distance_squared(pos) <= ALERT_RANGE * ALERT_RANGE
                        && (t.pos.y - pos.y).abs() <= 6.0 + 1.0
                });
            let item_near = self
                .items
                .iter()
                .find(|it| it.pickup_delay <= 0.0 && (it.pos - pos).abs().max_element() <= 8.0 + 0.5)
                .map(|it| it.pos);
            let in_village = self.raids.villages.is_village(pos.floor().as_ivec3());
            let thundering = self.thundering;
            let m = &mut self.mobs[i];
            let s = m.fox.as_mut().unwrap();
            s.target = target;
            s.alertable = alertable;
            s.item_near = item_near;
            s.in_village = in_village;
            s.thundering = thundering;
            if target.is_none() {
                // `Fox.setTarget(null)` ends the defence.
                s.defending = false;
            }
            if let Some(seq) = defended {
                // `start`: the aggro call, defending, awake.
                s.defend_stamp = Some(seq);
                s.defending = true;
                if let Some(a) = m.animal.as_mut() {
                    a.sleeping = false;
                }
                let at = m.pos + DVec3::Y * (m.shape().height * 0.8);
                self.pending_sounds
                    .push(super::EntityEvent::Sound { sound: super::MobSound::Fox(Cue::Aggro), pos: at });
            }
        }
    }

    /// v1.0 gap Track 2f: `FoxStrollThroughVillageGoal.canUse`'s roll for
    /// each fox on its full updates: at night, one time in
    /// `reducedTickDelay(200)`, within six sections of a village, the best
    /// of `LandRandomPos.getPos(15, 7)`'s spots by `-sectionsToVillage`
    /// becomes its `wantedPos` (its goal at priority 9 takes it).
    pub(super) fn fox_village_stroll<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &super::Ctx) {
        if ctx.daylight > 0.5 || !self.mobs.iter().any(|m| m.fox.is_some()) {
            for m in &mut self.mobs {
                if let Some(s) = m.fox.as_mut() {
                    s.village_stroll = None;
                }
            }
            return;
        }
        self.refresh_villages(world, false);
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if m.fox.is_none() || !m.alive() || !m.simulation_active || !m.goals.full(m.uid) {
                continue;
            }
            let here = m.pos.floor().as_ivec3();
            let mut wanted = None;
            if self.rng.next_int(100) == 0 && self.raids.villages.close_to_village(here, 6) {
                // `RandomPos.generateRandomPos` with the scorer: the best of
                // the spots by section distance to a village.
                let mut best: Option<(i32, IVec3)> = None;
                for _ in 0..10 {
                    if let Some(p) = m.random_pos(world, ctx, &mut self.rng, 15, 7, true) {
                        let d = self.raids.villages.sections_to_village(p);
                        if best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, p));
                        }
                    }
                }
                wanted = best.map(|(_, p)| p);
            }
            self.mobs[i].fox.as_mut().unwrap().village_stroll = wanted;
        }
    }
}

#[cfg(test)]
mod tests;
