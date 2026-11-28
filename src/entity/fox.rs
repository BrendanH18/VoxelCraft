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
use super::{Entities, Mob, MobKind};
use glam::DVec3;
use serde_json::{Value, json};

/// `Fox.MAX_CROUCH_AMOUNT`.
pub const MAX_CROUCH_AMOUNT: f32 = 5.0;
/// `NearestAttackableTargetGoal`'s follow range (`FOLLOW_RANGE` 32).
pub const FOLLOW_RANGE: f64 = 32.0;
/// `FoxBehaviorGoal.alertableTargeting`: 12 blocks, the box inflated 12, 6, 12.
const ALERT_RANGE: f64 = 12.0;

/// What a target goal hunts (`Fox.setTargetGoals`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hunt {
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
}

impl Running {
    pub(crate) fn priority(self) -> u8 {
        match self {
            Running::Faceplant { .. } => 1,
            Running::Stalk => 5,
            Running::Pounce | Running::Shelter => 6,
            Running::Melee { .. } | Running::Sleep => 7,
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
    /// The foxes' targets and senses before the mobs think: each target goal
    /// keeps a living target within its follow range and, without one, looks
    /// on the fox's full updates one time in `reducedTickDelay(10)` for the
    /// nearest match to its eyes (no sight needed); `alertable` and the items
    /// within 8 blocks.
    pub(super) fn fox_upkeep(&mut self, ctx: &super::Ctx) {
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
                }
            };
            // `TargetGoal.canContinueToUse`: alive and within the follow range.
            let kept = m.fox.as_ref().unwrap().target.and_then(|p| {
                let &j = by_uid.get(&p.uid)?;
                let t = &self.mobs[j];
                (t.alive() && t.pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE)
                    .then(|| prey(j, p.hunt, p.priority))
            });
            let mut target = kept;
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
            let s = self.mobs[i].fox.as_mut().unwrap();
            s.target = target;
            s.alertable = alertable;
            s.item_near = item_near;
            s.in_village = in_village;
        }
    }
}

#[cfg(test)]
mod tests;
