//! The Java 26.3 warden (MOB-04, v0.16 track A): `Warden`, `WardenAi`,
//! `AngerManagement`, `AngerLevel` and the brain behaviours `Emerging`,
//! `Digging`, `Roar`, `SetRoarTarget`, `TryToSniff`, `Sniffing`,
//! `SonicBoom`, `SetWardenLookTarget` and the shared `MeleeAttack`.
//!
//! The brain runs once per game tick in [`Entities::tick_wardens`], inside
//! the authoritative entity/world transaction; [`Mob::warden_steer`] turns
//! its walk target into movement through the shared ground navigation. Its
//! memories are tick counters with Java's expiries. The warden hears
//! vibrations at its eyes (radius 16) through the world's posted game
//! events and summons come from sculk shriekers
//! ([`crate::world::sculk::ShriekerResponse`]).
//!
//! Sources and differences: `docs/verification/2026-11-06-v0.16-sculk-warden.md`.

use glam::{DVec3, IVec3, Vec3};
use serde_json::{Value, json};

use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, PlayerId, Rng};
use crate::simulation::effects::Effect;
use crate::world::World;
use crate::world::vibration::{
    EntityRef, EventSource, GameEvent, Posted, VibrationData, VibrationInfo, valid_vibration,
};

pub const CAUSE_MELEE: crate::simulation::damage::Cause = crate::simulation::damage::Cause::WARDEN;
/// `death.attack.sonic_boom`. Armor and enchantments don't reduce it
/// (`#bypasses_armor`, `#bypasses_enchantments`).
pub const CAUSE_SONIC_BOOM: crate::simulation::damage::Cause = crate::simulation::damage::Cause::SONIC_BOOM;

/// Poses (Java `Pose`), saved by number.
pub const STANDING: u8 = 0;
pub const EMERGING: u8 = 1;
pub const DIGGING: u8 = 2;
pub const ROARING: u8 = 3;
pub const SNIFFING: u8 = 4;

/// `WardenAi.EMERGE_DURATION`, `ROAR_DURATION`, `SNIFFING_DURATION` and
/// `DIGGING_DURATION` (`Mth.ceil` of the animation lengths).
pub const EMERGE_TICKS: u16 = 134;
pub const ROAR_TICKS: u16 = 84;
pub const SNIFF_TICKS: u16 = 84;
pub const DIG_TICKS: u16 = 100;
/// `SonicBoom.DURATION` and `TICKS_BEFORE_PLAYING_SOUND`.
pub const SONIC_TICKS: u16 = 60;
pub const SONIC_SOUND_AT: u16 = 34;
/// `WardenAi.DIGGING_COOLDOWN`: a minute without disturbance and it digs down.
pub const DIG_COOLDOWN: u32 = 1200;
/// `AngerLevel` thresholds and `AngerManagement.MAX_ANGER`.
pub const AGITATED: i32 = 40;
pub const ANGRY: i32 = 80;
pub const MAX_ANGER: i32 = 150;
/// `Warden.DEFAULT_ANGER`, `PROJECTILE_ANGER`, `ON_HURT_ANGER_BOOST`.
const DEFAULT_ANGER: i32 = 35;
const PROJECTILE_ANGER: i32 = 10;
const ON_HURT_ANGER_BOOST: i32 = 20;
/// `Warden.ATTACK_DAMAGE`, `ATTACK_KNOCKBACK` and the sonic boom's damage.
pub const MELEE_DAMAGE: f32 = 30.0;
const ATTACK_KNOCKBACK: f32 = 1.5;
pub const SONIC_DAMAGE: f32 = 10.0;
/// `Warden.DARKNESS_RADIUS`, `DARKNESS_DURATION`, `DARKNESS_DISPLAY_LIMIT`
/// and the 120-tick interval.
pub const DARKNESS_RADIUS: f64 = 20.0;
pub const DARKNESS_TICKS: u32 = 260;
const DARKNESS_DISPLAY_LIMIT: u32 = 200;
/// `Warden.createAttributes`: movement 0.3, follow range 24.
pub const MOVEMENT_SPEED: f64 = 0.3_f32 as f64;
const FOLLOW_RANGE: f64 = 24.0;
/// `EntityType` eye height: 0.85 of the 2.9-block box, of the 1-block box
/// while digging or emerging.
const EYE: f64 = 2.9 * 0.85;

/// Warden sound events (`entity.warden.*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Ambient,
    Agitated,
    Angry,
    Listening,
    ListeningAngry,
    Heartbeat,
    TendrilClicks,
    Roar,
    Sniff,
    Emerge,
    Dig,
    AttackImpact,
    SonicCharge,
    SonicBoom,
    Hurt,
    Death,
    Step,
    NearbyClose,
    NearbyCloser,
    NearbyClosest,
}

impl Cue {
    pub const ALL: [Cue; 20] = [
        Cue::Ambient,
        Cue::Agitated,
        Cue::Angry,
        Cue::Listening,
        Cue::ListeningAngry,
        Cue::Heartbeat,
        Cue::TendrilClicks,
        Cue::Roar,
        Cue::Sniff,
        Cue::Emerge,
        Cue::Dig,
        Cue::AttackImpact,
        Cue::SonicCharge,
        Cue::SonicBoom,
        Cue::Hurt,
        Cue::Death,
        Cue::Step,
        Cue::NearbyClose,
        Cue::NearbyCloser,
        Cue::NearbyClosest,
    ];

    /// `SculkShriekerBlockEntity.SOUND_BY_LEVEL`.
    pub fn reply(level: u8) -> Option<Cue> {
        match level {
            1 => Some(Cue::NearbyClose),
            2 => Some(Cue::NearbyCloser),
            3 => Some(Cue::NearbyClosest),
            4 => Some(Cue::ListeningAngry),
            _ => None,
        }
    }
}

/// `AngerLevel.byAnger`.
pub fn anger_level(anger: i32) -> u8 {
    if anger >= ANGRY {
        2
    } else if anger >= AGITATED {
        1
    } else {
        0
    }
}

/// `AngerManagement`: anger per suspect, kept sorted (angry first, players
/// first, then most anger), and anger at entities that aren't present
/// (Java's `angerByUuid`), which decays and returns when they do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anger {
    pub suspects: Vec<(EntityRef, i32)>,
    pub absent: Vec<(EntityRef, i32)>,
    conversion: i32,
    highest: i32,
}

impl Anger {
    fn sort(&mut self) {
        self.suspects.sort_by(|a, b| {
            let (aa, ba) = (a.1 >= ANGRY, b.1 >= ANGRY);
            let (ap, bp) = (matches!(a.0, EntityRef::Player(_)), matches!(b.0, EntityRef::Player(_)));
            ba.cmp(&aa).then(bp.cmp(&ap)).then(b.1.cmp(&a.1))
        });
        self.highest = self.suspects.iter().map(|s| s.1).max().unwrap_or(0);
    }

    pub fn of(&self, e: EntityRef) -> i32 {
        self.suspects.iter().find(|s| s.0 == e).map_or(0, |s| s.1)
    }

    /// `increaseAnger`: capped at 150, plus any absent anger the first time.
    pub fn increase(&mut self, e: EntityRef, amount: i32) -> i32 {
        let value = if let Some(s) = self.suspects.iter_mut().find(|s| s.0 == e) {
            s.1 = (s.1 + amount).min(MAX_ANGER);
            s.1
        } else {
            let mut v = amount.min(MAX_ANGER);
            if let Some(i) = self.absent.iter().position(|s| s.0 == e) {
                v += self.absent.remove(i).1;
            }
            self.suspects.push((e, v));
            v
        };
        self.sort();
        value
    }

    pub fn clear(&mut self, e: EntityRef) {
        self.suspects.retain(|s| s.0 != e);
        self.sort();
    }

    /// `getActiveAnger`: anger at the target, else the highest.
    pub fn active(&self, target: Option<EntityRef>) -> i32 {
        match target {
            Some(t) => self.of(t),
            None => self.highest,
        }
    }

    /// `getActiveEntity`: the first suspect that passes `valid`.
    pub fn active_entity(&self, valid: impl Fn(EntityRef) -> bool) -> Option<EntityRef> {
        self.suspects.iter().map(|s| s.0).find(|&e| valid(e))
    }

    /// `AngerManagement.tick`, every 20 game ticks: absent anger comes back
    /// for present entities every second call, everything decays by one,
    /// and suspects that are gone or no longer valid drop out (to the absent
    /// list when they only left).
    fn tick(&mut self, present: impl Fn(EntityRef) -> Presence) {
        self.conversion -= 1;
        if self.conversion <= 0 {
            let back: Vec<_> = self.absent.iter().copied().filter(|s| present(s.0) != Presence::Gone).collect();
            self.absent.retain(|s| present(s.0) == Presence::Gone);
            self.suspects.extend(back);
            self.conversion = 2;
        }
        self.absent.retain_mut(|s| {
            s.1 -= 1;
            s.1 > 0
        });
        let mut absent = Vec::new();
        self.suspects.retain_mut(|s| {
            let p = present(s.0);
            if s.1 > 1 && p == Presence::Valid {
                s.1 -= 1;
                return true;
            }
            if s.1 > 1 && p == Presence::Gone {
                absent.push((s.0, s.1 - 1));
            }
            false
        });
        self.absent.extend(absent);
        self.sort();
    }

    fn save(&self) -> Value {
        json!(
            self.suspects
                .iter()
                .chain(&self.absent)
                .map(|(e, a)| json!({"entity":entity_save(*e),"anger":a}))
                .collect::<Vec<_>>()
        )
    }

    /// Java loads every suspect into `angerByUuid`; they come back as their
    /// entities are found.
    fn load(v: &Value) -> Self {
        let absent = v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some((entity_load(&s["entity"])?, s["anger"].as_i64()?.clamp(0, i64::from(i32::MAX)) as i32))
            })
            .collect();
        Self { suspects: Vec::new(), absent, conversion: 0, highest: 0 }
    }
}

fn entity_save(e: EntityRef) -> Value {
    match e {
        EntityRef::Player(p) => json!(["player", p.0]),
        EntityRef::Mob(uid) => json!(["mob", uid]),
    }
}

fn entity_load(v: &Value) -> Option<EntityRef> {
    let id = u32::try_from(v[1].as_u64()?).ok()?;
    match v[0].as_str()? {
        "player" => Some(EntityRef::Player(PlayerId(id))),
        "mob" => Some(EntityRef::Mob(id)),
        _ => None,
    }
}

/// Where a suspect is, for anger bookkeeping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Presence {
    /// Present and a valid target.
    Valid,
    /// Present but not a valid target (dead, creative, another warden).
    Invalid,
    /// Not in this world (unloaded, elsewhere): anger waits.
    Gone,
}

/// A running timed behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Emerge,
    Dig,
    Roar,
    Sniff,
}

impl Action {
    fn id(self) -> u8 {
        self as u8
    }

    fn from_id(id: u64) -> Option<Self> {
        [Action::Emerge, Action::Dig, Action::Roar, Action::Sniff].get(id as usize).copied()
    }
}

/// `WardenAi.updateActivity`'s priority list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Activity {
    Emerge,
    Dig,
    Roar,
    Fight,
    Investigate,
    Sniff,
    #[default]
    Idle,
}

/// The warden's brain and body state.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub pose: u8,
    pub activity: Activity,
    pub action: Option<(Action, u16)>,
    /// A running sonic boom and its elapsed ticks.
    pub sonic: Option<u16>,
    pub anger: Anger,
    pub vibration: VibrationData,
    // Memories with expiry, in ticks left (0: absent).
    pub emerging: u32,
    pub dig_cooldown: u32,
    pub vibration_cooldown: u32,
    pub recent_projectile: u32,
    pub touch_cooldown: u32,
    pub sniff_cooldown: u32,
    pub sonic_cooldown: u32,
    pub attack_cooling: u32,
    pub sniffing: bool,
    pub disturbance: Option<(IVec3, u32)>,
    pub roar_target: Option<EntityRef>,
    pub attack_target: Option<EntityRef>,
    /// `WALK_TARGET`: where to go and the speed modifier.
    pub walk: Option<(DVec3, f64)>,
    pub look: Option<DVec3>,
    idle_wait: u16,
    /// Ticks spent on the current walk target (not saved).
    walk_ticks: u16,
    pub ticks: u64,
    ambient: i32,
    /// Client animation timers (`tendrilAnimation`, `heartAnimation`).
    pub tendril: u8,
    pub heart: u8,
    /// `CLIENT_ANGER_LEVEL`: the active anger, synced every 20 ticks.
    pub client_anger: i32,
    carry: f64,
    /// Hits taken since the last brain update: attacker and whether direct.
    hurt_by: Vec<(EntityRef, bool)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            pose: STANDING,
            activity: Activity::Idle,
            action: None,
            sonic: None,
            anger: Anger::default(),
            vibration: VibrationData::default(),
            emerging: 0,
            // `finalizeSpawn`: every warden starts with the dig cooldown.
            dig_cooldown: DIG_COOLDOWN,
            vibration_cooldown: 0,
            recent_projectile: 0,
            touch_cooldown: 0,
            sniff_cooldown: 0,
            sonic_cooldown: 0,
            attack_cooling: 0,
            sniffing: false,
            disturbance: None,
            roar_target: None,
            attack_target: None,
            walk: None,
            look: None,
            idle_wait: 0,
            walk_ticks: 0,
            ticks: 0,
            ambient: -80,
            tendril: 0,
            heart: 0,
            client_anger: 0,
            carry: 0.0,
            hurt_by: Vec::new(),
        }
    }
}

fn pos(v: &Value) -> Option<IVec3> {
    let a: [i32; 3] = serde_json::from_value(v.clone()).ok()?;
    Some(IVec3::from_array(a))
}

impl State {
    /// A warden a shrieker summoned (`EntitySpawnReason.TRIGGERED`): it
    /// emerges from the ground.
    pub fn emerging() -> Self {
        Self { pose: EMERGING, emerging: u32::from(EMERGE_TICKS), activity: Activity::Emerge, ..Self::default() }
    }

    pub fn digging_or_emerging(&self) -> bool {
        matches!(self.pose, DIGGING | EMERGING)
    }

    pub fn save(&self) -> Value {
        json!({
            "pose": self.pose,
            "action": self.action.map(|(a, t)| json!([a.id(), t])),
            "sonic": self.sonic,
            "anger": self.anger.save(),
            "listener": self.vibration.save(),
            "is_emerging": self.emerging,
            "dig_cooldown": self.dig_cooldown,
            "vibration_cooldown": self.vibration_cooldown,
            "recent_projectile": self.recent_projectile,
            "touch_cooldown": self.touch_cooldown,
            "sniff_cooldown": self.sniff_cooldown,
            "sonic_boom_cooldown": self.sonic_cooldown,
            "attack_cooling_down": self.attack_cooling,
            "is_sniffing": self.sniffing,
            "disturbance": self.disturbance.map(|(p, t)| json!([p.to_array(), t])),
            "roar_target": self.roar_target.map(entity_save),
            "attack_target": self.attack_target.map(entity_save),
            "ticks": self.ticks,
        })
    }

    pub fn load(v: &Value) -> Self {
        let n = |k: &str, max: u64| v[k].as_u64().unwrap_or(0).min(max) as u32;
        let mut s = Self {
            pose: v["pose"].as_u64().unwrap_or(0).min(4) as u8,
            action: v["action"][0]
                .as_u64()
                .and_then(Action::from_id)
                .zip(v["action"][1].as_u64().map(|t| t.min(200) as u16)),
            sonic: v["sonic"].as_u64().map(|t| t.min(u64::from(SONIC_TICKS)) as u16),
            anger: Anger::load(&v["anger"]),
            vibration: VibrationData::load(&v["listener"]),
            emerging: n("is_emerging", u64::from(EMERGE_TICKS)),
            dig_cooldown: v["dig_cooldown"].as_u64().map_or(DIG_COOLDOWN, |t| t.min(u64::from(DIG_COOLDOWN)) as u32),
            vibration_cooldown: n("vibration_cooldown", 40),
            recent_projectile: n("recent_projectile", 100),
            touch_cooldown: n("touch_cooldown", 20),
            sniff_cooldown: n("sniff_cooldown", 200),
            sonic_cooldown: n("sonic_boom_cooldown", 200),
            attack_cooling: n("attack_cooling_down", 60),
            sniffing: v["is_sniffing"] == true,
            disturbance: pos(&v["disturbance"][0]).zip(v["disturbance"][1].as_u64().map(|t| t.min(100) as u32)),
            roar_target: entity_load(&v["roar_target"]),
            attack_target: entity_load(&v["attack_target"]),
            ticks: v["ticks"].as_u64().unwrap_or(0),
            ..Self::default()
        };
        s.activity = s.next_activity();
        s
    }

    /// The fields LAN clients draw from.
    pub(super) fn look_state(&self) -> Value {
        json!({"pose":self.pose,"action":self.action.map(|(a,t)|json!([a.id(),t])),"sonic":self.sonic,
            "tendril":self.tendril,"heart":self.heart,"anger":self.client_anger})
    }

    pub(super) fn apply_look(&mut self, v: &Value) {
        self.pose = v["pose"].as_u64().unwrap_or(0).min(4) as u8;
        self.action =
            v["action"][0].as_u64().and_then(Action::from_id).zip(v["action"][1].as_u64().map(|t| t.min(200) as u16));
        self.sonic = v["sonic"].as_u64().map(|t| t.min(u64::from(SONIC_TICKS)) as u16);
        self.tendril = v["tendril"].as_u64().unwrap_or(0).min(10) as u8;
        self.heart = v["heart"].as_u64().unwrap_or(0).min(10) as u8;
        self.client_anger = v["anger"].as_i64().unwrap_or(0).clamp(0, i64::from(MAX_ANGER)) as i32;
    }

    /// `WardenAi.updateActivity`.
    fn next_activity(&self) -> Activity {
        if self.emerging > 0 {
            Activity::Emerge
        } else if self.roar_target.is_none() && self.dig_cooldown == 0 {
            Activity::Dig
        } else if self.roar_target.is_some() {
            Activity::Roar
        } else if self.attack_target.is_some() {
            Activity::Fight
        } else if self.disturbance.is_some() {
            Activity::Investigate
        } else if self.sniffing {
            Activity::Sniff
        } else {
            Activity::Idle
        }
    }

    /// `WardenAi.setDigCooldown`: refreshed only while present.
    fn refresh_dig_cooldown(&mut self) {
        if self.dig_cooldown > 0 {
            self.dig_cooldown = DIG_COOLDOWN;
        }
    }

    /// `Warden.getHeartBeatDelay`.
    pub fn heartbeat_delay(&self) -> u64 {
        let anger = (self.client_anger as f32 / ANGRY as f32).clamp(0.0, 1.0);
        (40 - (anger * 30.0).floor() as i32) as u64
    }
}

/// A suspect as the brain sees it.
#[derive(Clone, Copy, Debug)]
struct Foe {
    pos: DVec3,
    half_width: f64,
    height: f64,
    valid: bool,
}

impl Mob {
    /// Movement for the warden's walk target ([`Entities::tick_wardens`]
    /// decides it): still while emerging, digging, roaring or sniffing.
    pub(super) fn warden_steer<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng) -> (Option<DVec3>, f64) {
        let s = self.warden.as_ref().unwrap();
        if s.digging_or_emerging() || s.action.is_some() {
            self.stop_navigation();
            return (None, 0.0);
        }
        if let Some(look) = s.look {
            let to = look - self.pos;
            if to.with_y(0.0).length_squared() > 1e-6 {
                let yaw = (to.z as f32).atan2(to.x as f32);
                self.yaw = super::mob::turn_toward(self.yaw, yaw, 0.6);
            }
        }
        let Some((goal, modifier)) = s.walk else {
            self.stop_navigation();
            return (None, 0.0);
        };
        let hunting = s.attack_target.is_some();
        let speed = super::path::ground_speed(MOVEMENT_SPEED) * modifier;
        (self.pursue(w, goal, speed, true, hunting, rng), speed)
    }
}

/// `Mob.isWithinMeleeAttackRange`: the attacker's box grown by
/// `sqrt(2.04) - 0.6` horizontally meets the target's.
fn in_melee(pos: DVec3, half_width: f64, height: f64, foe: &Foe) -> bool {
    let reach = f64::from(2.04_f32).sqrt() - f64::from(0.6_f32);
    let r = half_width + reach;
    (pos.x - foe.pos.x).abs() < r + foe.half_width
        && (pos.z - foe.pos.z).abs() < r + foe.half_width
        && pos.y < foe.pos.y + foe.height
        && foe.pos.y < pos.y + height
}

/// `Entity.closerThan(other, horizontal, vertical)`.
fn closer_than(a: DVec3, b: DVec3, h: f64, v: f64) -> bool {
    let d = b - a;
    d.x * d.x + d.z * d.z < h * h && d.y * d.y < v * v
}

impl Entities {
    /// Summons a warden at `pos` as a shrieker does: it emerges.
    pub fn spawn_emerging_warden(&mut self, pos: DVec3) -> u32 {
        self.spawn(MobKind::Warden, pos);
        self.ensure_mob_ids();
        let m = self.mobs.last_mut().unwrap();
        m.warden = Some(Box::new(State::emerging()));
        let uid = m.uid;
        let at = m.pos + DVec3::Y;
        // `finalizeSpawn` plays `WARDEN_AGITATED`; `trySpawnMob` its ambient sound.
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(Cue::Agitated), pos: at });
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(Cue::Ambient), pos: at });
        uid
    }

    fn foe(&self, ctx: &Ctx, e: EntityRef) -> Option<Foe> {
        match e {
            EntityRef::Player(id) => ctx.players.iter().find(|p| p.id == id).map(|p| Foe {
                pos: p.pos,
                half_width: p.shape.half_width,
                height: p.shape.height,
                valid: p.targetable && p.alive && !p.spectator,
            }),
            EntityRef::Mob(uid) => self.mobs.iter().find(|m| m.uid == uid).map(|m| {
                let shape = m.shape();
                Foe {
                    pos: m.pos,
                    half_width: shape.half_width,
                    height: shape.height,
                    valid: m.alive() && m.kind != MobKind::Warden,
                }
            }),
        }
    }

    /// `Warden.canTargetEntity`.
    fn warden_can_target(&self, ctx: &Ctx, e: EntityRef) -> bool {
        self.foe(ctx, e).is_some_and(|f| f.valid)
    }

    fn presence(&self, ctx: &Ctx, e: EntityRef) -> Presence {
        match self.foe(ctx, e) {
            None => Presence::Gone,
            Some(f) if f.valid => Presence::Valid,
            Some(_) => Presence::Invalid,
        }
    }

    /// `Warden.increaseAngerAt`.
    fn warden_anger(&mut self, i: usize, s: &mut State, ctx: &Ctx, e: EntityRef, amount: i32, sound: bool) {
        if !self.warden_can_target(ctx, e) {
            return;
        }
        s.refresh_dig_cooldown();
        let switch = !matches!(s.attack_target, Some(EntityRef::Player(_)));
        let anger = s.anger.increase(e, amount);
        if matches!(e, EntityRef::Player(_)) && switch && anger >= ANGRY {
            s.attack_target = None;
        }
        if sound && s.pose != ROARING {
            let cue =
                if anger_level(s.anger.active(s.attack_target)) == 0 { Cue::Listening } else { Cue::ListeningAngry };
            let at = self.mobs[i].pos + DVec3::Y * 2.0;
            self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(cue), pos: at });
        }
    }

    /// `Warden.getEntityAngryAt`: the top suspect, once angry.
    fn warden_angry_at(&self, s: &State, ctx: &Ctx) -> Option<EntityRef> {
        (anger_level(s.anger.active(s.attack_target)) == 2)
            .then(|| s.anger.active_entity(|e| self.warden_can_target(ctx, e)))
            .flatten()
    }

    /// `WardenAi.setDisturbanceLocation`.
    fn warden_disturbed(&self, s: &mut State, ctx: &Ctx, at: IVec3) {
        if self.warden_angry_at(s, ctx).is_some() || s.attack_target.is_some() {
            return;
        }
        s.refresh_dig_cooldown();
        s.sniff_cooldown = 100;
        s.look = Some(at.as_dvec3() + DVec3::splat(0.5));
        s.disturbance = Some((at, 100));
        s.walk = None;
    }

    /// `Warden.setAttackTarget`.
    fn warden_attack(s: &mut State, target: EntityRef) {
        s.roar_target = None;
        s.attack_target = Some(target);
        s.sonic_cooldown = 200;
    }

    /// `Warden.hurtServer`: anger at the attacker (by 100), who becomes the
    /// attack target when there is none and the hit was direct or within 5
    /// blocks. Settled at the warden's next brain update.
    pub(super) fn warden_hurt(&mut self, i: usize, attacker: EntityRef, direct: bool) {
        if let Some(s) = self.mobs.get_mut(i).and_then(|m| m.warden.as_mut())
            && !s.digging_or_emerging()
            && s.hurt_by.len() < 16
        {
            s.hurt_by.push((attacker, direct));
        }
    }

    fn warden_settle_hurts(&mut self, i: usize, s: &mut State, ctx: &Ctx) {
        for (attacker, direct) in std::mem::take(&mut s.hurt_by) {
            self.warden_anger(i, s, ctx, attacker, ANGRY + ON_HURT_ANGER_BOOST, false);
            if s.attack_target.is_none()
                && let Some(f) = self.foe(ctx, attacker)
                && (direct || f.pos.distance_squared(self.mobs[i].pos) < 25.0)
            {
                Self::warden_attack(s, attacker);
            }
        }
    }

    /// One update of every warden, and of shriekers answering: called from
    /// the authoritative `update_world` transaction.
    pub fn tick_wardens(&mut self, dt: f64, w: &mut World, ctx: &Ctx) {
        self.answer_shriekers(w, ctx);
        let any = self.mobs.iter().any(|m| m.warden.is_some());
        w.set_entity_listeners(any);
        if !any {
            return;
        }
        let posted = w.take_entity_game_events();
        let now = w.game_tick();
        for i in 0..self.mobs.len() {
            if i >= self.mobs.len() {
                break;
            }
            let m = &self.mobs[i];
            if m.warden.is_none() || !m.alive() || !w.is_entity_ticking(m.pos.floor().as_ivec3()) {
                continue;
            }
            let mut s = self.mobs[i].warden.take().unwrap();
            self.warden_settle_hurts(i, &mut s, ctx);
            self.warden_hear(i, &mut s, w, ctx, &posted);
            s.carry += dt.clamp(0.0, 0.25) * 20.0;
            let ticks = s.carry.floor() as u32;
            s.carry -= f64::from(ticks);
            let mut removed = false;
            for _ in 0..ticks {
                if self.warden_tick(i, &mut s, w, ctx, now) {
                    removed = true;
                    break;
                }
            }
            if removed {
                // `Digging.stop`: discarded, without loot or experience.
                self.mobs.swap_remove(i);
                continue;
            }
            self.mobs[i].warden = Some(s);
        }
    }

    /// `VibrationSystem.Listener.handleGameEvent` for posted events.
    fn warden_hear(&self, i: usize, s: &mut State, w: &World, ctx: &Ctx, posted: &[Posted]) {
        let m = &self.mobs[i];
        let eye = m.pos + DVec3::Y * if s.digging_or_emerging() { 0.85 } else { EYE };
        let eye_cell = eye.floor().as_ivec3();
        for e in posted {
            if s.vibration.current.is_some() {
                return;
            }
            if !valid_vibration(e.event, &e.source, e.affected, GameEvent::warden_listens) {
                continue;
            }
            if (e.pos.floor().as_ivec3() - eye_cell).length_squared() > 16 * 16 {
                continue;
            }
            // `VibrationUser.canReceiveVibration`.
            if s.vibration_cooldown > 0 || s.digging_or_emerging() || e.source.untargetable {
                continue;
            }
            if e.source.entity.is_some_and(|src| src == EntityRef::Mob(m.uid) || !self.warden_can_target(ctx, src)) {
                continue;
            }
            if w.vibration_occluded(e.pos, eye) {
                continue;
            }
            let info =
                VibrationInfo { event: e.event, distance: e.pos.distance(eye) as f32, pos: e.pos, source: e.source };
            s.vibration.add_candidate(info, e.tick);
        }
    }

    /// `Warden.VibrationUser.onReceiveVibration`.
    fn warden_receive(&mut self, i: usize, s: &mut State, ctx: &Ctx, info: VibrationInfo) {
        let at = self.mobs[i].pos;
        s.vibration_cooldown = 40;
        s.tendril = 10;
        self.pending_sounds
            .push(EntityEvent::Sound { sound: MobSound::Warden(Cue::TendrilClicks), pos: at + DVec3::Y * 2.5 });
        let mut suspicious = info.pos.floor().as_ivec3();
        if let Some(owner) = info.source.projectile_owner {
            if let Some(f) = self.foe(ctx, owner)
                && f.pos.distance_squared(at) < 900.0
            {
                if s.recent_projectile > 0 {
                    if f.valid {
                        suspicious = f.pos.floor().as_ivec3();
                    }
                    self.warden_anger(i, s, ctx, owner, DEFAULT_ANGER, true);
                } else {
                    self.warden_anger(i, s, ctx, owner, PROJECTILE_ANGER, true);
                }
            }
            s.recent_projectile = 100;
        } else if let Some(src) = info.source.entity {
            self.warden_anger(i, s, ctx, src, DEFAULT_ANGER, true);
        }
        if anger_level(s.anger.active(s.attack_target)) < 2 {
            let active = s.anger.active_entity(|e| self.warden_can_target(ctx, e));
            if info.source.projectile_owner.is_some() || active.is_none() || active == info.source.entity {
                self.warden_disturbed(s, ctx, suspicious);
            }
        }
    }

    /// The nearest attackable entity (`WardenEntitySensor`): the nearest
    /// valid player within the follow range, else the nearest valid mob.
    fn warden_nearest(&self, i: usize, ctx: &Ctx) -> Option<(EntityRef, DVec3)> {
        let at = self.mobs[i].pos;
        let near = |p: DVec3| {
            let d = p - at;
            d.x.abs() <= FOLLOW_RANGE && d.y.abs() <= FOLLOW_RANGE && d.z.abs() <= FOLLOW_RANGE
        };
        let player = ctx
            .players
            .iter()
            .filter(|p| p.targetable && p.alive && !p.spectator && near(p.pos))
            .min_by(|a, b| a.pos.distance_squared(at).total_cmp(&b.pos.distance_squared(at)))
            .map(|p| (EntityRef::Player(p.id), p.pos));
        player.or_else(|| {
            self.mobs
                .iter()
                .enumerate()
                .filter(|&(j, m)| j != i && near(m.pos) && self.warden_can_target(ctx, EntityRef::Mob(m.uid)))
                .min_by(|a, b| a.1.pos.distance_squared(at).total_cmp(&b.1.pos.distance_squared(at)))
                .map(|(_, m)| (EntityRef::Mob(m.uid), m.pos))
        })
    }

    /// `SetRoarTarget`.
    fn warden_set_roar_target(&self, s: &mut State, ctx: &Ctx) {
        if s.roar_target.is_none()
            && s.attack_target.is_none()
            && let Some(t) = self.warden_angry_at(s, ctx)
        {
            s.roar_target = Some(t);
        }
    }

    /// One game tick of the brain. Returns true when the warden dug away.
    fn warden_tick(&mut self, i: usize, s: &mut State, w: &mut World, ctx: &Ctx, now: u64) -> bool {
        s.ticks += 1;
        // `Brain.forgetOutdatedMemories`.
        for c in [
            &mut s.emerging,
            &mut s.dig_cooldown,
            &mut s.vibration_cooldown,
            &mut s.recent_projectile,
            &mut s.touch_cooldown,
            &mut s.sniff_cooldown,
            &mut s.sonic_cooldown,
            &mut s.attack_cooling,
        ] {
            *c = c.saturating_sub(1);
        }
        if let Some((_, t)) = &mut s.disturbance {
            *t = t.saturating_sub(1);
            if *t == 0 {
                s.disturbance = None;
            }
        }
        s.tendril = s.tendril.saturating_sub(1);
        s.heart = s.heart.saturating_sub(1);
        // `Warden.tick`: the vibration ticker, then persistence.
        if let Some(info) = s.vibration.tick(now, |d| d.floor().max(0.0) as u32, true) {
            self.warden_receive(i, s, ctx, info);
        }
        if self.mobs[i].persistent {
            s.refresh_dig_cooldown();
        }
        let at = self.mobs[i].pos;
        let sound = |cue: Cue| EntityEvent::Sound { sound: MobSound::Warden(cue), pos: at + DVec3::Y * 2.0 };
        // Touch: `Warden.doPush` on anything it bumps into.
        if s.touch_cooldown == 0 && !s.digging_or_emerging() {
            let me = self.mobs[i].shape();
            let touching = ctx
                .players
                .iter()
                .filter(|p| p.alive && !p.spectator)
                .map(|p| (EntityRef::Player(p.id), p.pos, p.shape.half_width, p.shape.height))
                .chain(self.mobs.iter().enumerate().filter(|&(j, m)| j != i && m.alive()).map(|(_, m)| {
                    let sh = m.shape();
                    (EntityRef::Mob(m.uid), m.pos, sh.half_width, sh.height)
                }))
                .find(|&(_, p, hw, h)| {
                    (p.x - at.x).abs() < hw + me.half_width
                        && (p.z - at.z).abs() < hw + me.half_width
                        && p.y < at.y + me.height
                        && at.y < p.y + h
                });
            if let Some((e, p, _, _)) = touching {
                s.touch_cooldown = 20;
                self.warden_anger(i, s, ctx, e, DEFAULT_ANGER, true);
                self.warden_disturbed(s, ctx, p.floor().as_ivec3());
            }
        }
        // `SetWardenLookTarget` (core).
        if s.attack_target.is_none() {
            let target = s
                .roar_target
                .and_then(|r| self.foe(ctx, r))
                .map(|f| f.pos.floor().as_ivec3())
                .or(s.disturbance.map(|d| d.0));
            if let Some(t) = target {
                s.look = Some(t.as_dvec3() + DVec3::splat(0.5));
            }
        }
        // Running timed behaviours.
        if let Some((action, elapsed)) = &mut s.action {
            *elapsed += 1;
            let (action, elapsed) = (*action, *elapsed);
            match action {
                Action::Emerge if elapsed >= EMERGE_TICKS => {
                    s.pose = STANDING;
                    s.action = None;
                }
                Action::Dig if elapsed >= DIG_TICKS => return true,
                Action::Roar => {
                    if elapsed == 25 {
                        self.pending_sounds.push(sound(Cue::Roar));
                    }
                    if elapsed >= ROAR_TICKS {
                        if s.pose == ROARING {
                            s.pose = STANDING;
                        }
                        if let Some(t) = s.roar_target.take() {
                            Self::warden_attack(s, t);
                        }
                        s.action = None;
                    }
                }
                Action::Sniff if elapsed >= SNIFF_TICKS => {
                    if s.pose == SNIFFING {
                        s.pose = STANDING;
                    }
                    s.sniffing = false;
                    s.action = None;
                    if let Some((e, p)) = self.warden_nearest(i, ctx) {
                        if closer_than(at, p, 6.0, 20.0) {
                            self.warden_anger(i, s, ctx, e, DEFAULT_ANGER, true);
                        }
                        if s.disturbance.is_none() {
                            self.warden_disturbed(s, ctx, p.floor().as_ivec3());
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(elapsed) = &mut s.sonic {
            *elapsed += 1;
            let elapsed = *elapsed;
            if elapsed == SONIC_SOUND_AT {
                self.warden_sonic_boom(i, s, w, ctx);
            }
            if elapsed >= SONIC_TICKS {
                s.sonic = None;
                s.sonic_cooldown = 40;
            }
        }
        // `MoveToTargetSink`: a walk target is done once reached, and given
        // up when it can't be reached in ten seconds.
        if let Some((goal, _)) = s.walk {
            s.walk_ticks = s.walk_ticks.saturating_add(1);
            if goal.with_y(0.0).distance_squared(at.with_y(0.0)) < 1.0 && (goal.y - at.y).abs() < 2.0
                || s.walk_ticks > 200
            {
                s.walk = None;
            }
        }
        if s.walk.is_none() {
            s.walk_ticks = 0;
        }
        // The current activity's behaviours.
        let (on_ground, in_water) = (self.mobs[i].on_ground, self.mobs[i].in_water);
        match s.activity {
            Activity::Emerge => {
                if s.action.is_none() && s.walk.is_none() && s.emerging > 0 {
                    s.pose = EMERGING;
                    s.action = Some((Action::Emerge, 0));
                    self.pending_sounds.push(sound(Cue::Emerge));
                }
            }
            Activity::Dig => {
                if s.action.is_none() && s.attack_target.is_none() && s.walk.is_none() {
                    if on_ground {
                        s.pose = DIGGING;
                        s.action = Some((Action::Dig, 0));
                        self.pending_sounds.push(sound(Cue::Dig));
                    } else if in_water {
                        self.pending_sounds.push(sound(Cue::Agitated));
                        return true;
                    }
                }
            }
            Activity::Roar => {
                if s.action.is_none()
                    && s.attack_target.is_none()
                    && let Some(t) = s.roar_target
                {
                    s.walk = None;
                    if let Some(f) = self.foe(ctx, t) {
                        s.look = Some(f.pos + DVec3::Y * f.height * 0.85);
                    }
                    s.pose = ROARING;
                    s.action = Some((Action::Roar, 0));
                    self.warden_anger(i, s, ctx, t, ON_HURT_ANGER_BOOST, false);
                }
            }
            Activity::Fight => self.warden_fight(i, s, &*w, ctx),
            Activity::Investigate => {
                self.warden_set_roar_target(s, ctx);
                if let Some((p, _)) = s.disturbance {
                    let goal = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                    // `GoToTargetLocation`: within two blocks counts as there.
                    if goal.with_y(0.0).distance(at.with_y(0.0)) <= 2.0 && (goal.y - at.y).abs() < 3.0 {
                        s.walk = None;
                    } else if s.walk.is_none() {
                        s.walk = Some((goal, 0.7));
                    }
                }
            }
            Activity::Sniff => {
                self.warden_set_roar_target(s, ctx);
                if s.action.is_none() && s.sniffing && s.attack_target.is_none() && s.walk.is_none() {
                    s.action = Some((Action::Sniff, 0));
                    self.pending_sounds.push(sound(Cue::Sniff));
                }
            }
            Activity::Idle => {
                self.warden_set_roar_target(s, ctx);
                // `TryToSniff`.
                if s.sniff_cooldown == 0 && s.disturbance.is_none() && self.warden_nearest(i, ctx).is_some() {
                    s.sniffing = true;
                    s.sniff_cooldown = 100 + self.rng.next_int(101);
                    s.walk = None;
                    s.pose = SNIFFING;
                } else if !s.sniffing && s.walk.is_none() {
                    if s.idle_wait > 0 {
                        s.idle_wait -= 1;
                    } else if self.rng.next_int(3) < 2 {
                        // `RandomStroll.stroll(0.5)`: a land position within 10 and 7.
                        let mut rng = Rng::new(self.rng.next_u64());
                        let m = &self.mobs[i];
                        if let Some(p) = m.random_pos(&*w, ctx, &mut rng, 10, 7, true) {
                            s.walk = Some((p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), 0.5));
                        }
                    } else {
                        // `DoNothing(30, 60)`.
                        s.idle_wait = 30 + self.rng.next_int(31) as u16;
                    }
                }
            }
        }
        // Darkness every 120 ticks around it.
        if (s.ticks + u64::from(self.mobs[i].uid)).is_multiple_of(120) {
            self.darkness_around(at, DARKNESS_RADIUS, ctx);
        }
        if s.ticks.is_multiple_of(20) {
            let mut anger = std::mem::take(&mut s.anger);
            anger.tick(|e| self.presence(ctx, e));
            s.anger = anger;
            s.client_anger = s.anger.active(s.attack_target);
        }
        // Heartbeat and ambient calls (`Warden.tick` client side, `Mob.baseTick`).
        if s.ticks.is_multiple_of(s.heartbeat_delay()) {
            s.heart = 10;
            self.pending_sounds.push(sound(Cue::Heartbeat));
        }
        s.ambient += 1;
        if s.pose != ROARING && !s.digging_or_emerging() && (self.rng.next_int(1000) as i32) < s.ambient {
            s.ambient = -80;
            let cue = match anger_level(s.anger.active(s.attack_target)) {
                0 => Cue::Ambient,
                1 => Cue::Agitated,
                _ => Cue::Angry,
            };
            self.pending_sounds.push(sound(cue));
        }
        s.activity = s.next_activity();
        false
    }

    /// The fight activity: give up an invalid target, chase, sonic boom and
    /// melee (`StopAttackingIfTargetInvalid`, `SetWalkTargetFromAttackTarget
    /// IfTargetOutOfReach(1.2)`, `SonicBoom`, `MeleeAttack(18)`).
    fn warden_fight(&mut self, i: usize, s: &mut State, w: &World, ctx: &Ctx) {
        s.refresh_dig_cooldown();
        let Some(target) = s.attack_target else { return };
        let foe = self.foe(ctx, target);
        let valid = foe.is_some_and(|f| f.valid);
        if !valid || anger_level(s.anger.active(Some(target))) < 2 {
            if !valid {
                s.anger.clear(target);
            }
            s.refresh_dig_cooldown();
            s.attack_target = None;
            s.walk = None;
            return;
        }
        let foe = foe.unwrap();
        let m = &self.mobs[i];
        let shape = m.shape();
        let eye = m.pos + DVec3::Y * EYE;
        let target_eye = foe.pos + DVec3::Y * foe.height * 0.85;
        s.look = Some(target_eye);
        let sees = super::mob::line_of_sight(w, eye, target_eye);
        let reach = in_melee(m.pos, shape.half_width, shape.height, &foe);
        s.walk = if reach && sees { None } else { Some((foe.pos, 1.2)) };
        if s.sonic.is_none() && s.sonic_cooldown == 0 && closer_than(m.pos, foe.pos, 15.0, 20.0) {
            s.sonic = Some(0);
            s.attack_cooling = u32::from(SONIC_TICKS);
            self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(Cue::SonicCharge), pos: eye });
        }
        if s.attack_cooling == 0 && reach && sees {
            s.attack_cooling = 18;
            // `Warden.doHurtTarget`.
            s.sonic_cooldown = 40;
            let m = &mut self.mobs[i];
            m.attack_anim = 0.75;
            let java_yaw = crate::knockback::java_yaw_degrees(m.yaw);
            let away = (foe.pos - m.pos).with_y(0.0).normalize_or_zero();
            let (pos, uid) = (m.pos, m.uid);
            // `Mob.doHurtTarget` slows the attacker after its knockback.
            m.vel.x *= 0.6;
            m.vel.z *= 0.6;
            self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(Cue::AttackImpact), pos: eye });
            match target {
                EntityRef::Player(player) => {
                    self.pending_sounds.push(EntityEvent::PlayerHit {
                        player,
                        damage: MELEE_DAMAGE,
                        knockback: (away * 8.0 + DVec3::Y * 4.0).as_vec3(),
                        java: Some(crate::knockback::Hit::melee(pos, foe.pos, java_yaw, ATTACK_KNOCKBACK * 0.5)),
                        cause: CAUSE_MELEE,
                    });
                    self.pending_sounds.push(EntityEvent::OwnerHurt { player, attacker: uid });
                }
                EntityRef::Mob(target) => self.pending_sounds.push(EntityEvent::MobHit {
                    target,
                    attacker: uid,
                    damage: MELEE_DAMAGE,
                    knockback: away * 8.0 + DVec3::Y * 4.0,
                }),
            }
        }
    }

    /// `SonicBoom.tick` at its sound: the beam, its sound, 10 damage that
    /// armor and enchantments don't reduce, and a 2.5 / 0.5 push.
    fn warden_sonic_boom(&mut self, i: usize, s: &mut State, w: &mut World, ctx: &Ctx) {
        let Some(target) = s.attack_target else { return };
        let Some(foe) = self.foe(ctx, target).filter(|f| f.valid) else { return };
        let m = &self.mobs[i];
        if !closer_than(m.pos, foe.pos, 15.0, 20.0) {
            return;
        }
        // `EntityAttachment.WARDEN_CHEST` at 1.6.
        let source = m.pos + DVec3::Y * 1.6;
        let target_eye = foe.pos + DVec3::Y * foe.height * 0.85;
        let delta = target_eye - source;
        let dir = delta.normalize_or_zero();
        let steps = delta.length().floor() as i32 + 7;
        for k in 1..steps {
            let mut b = crate::particles::Burst::new(crate::particles::Kind::Effect, source + dir * f64::from(k), 1);
            b.color = Some([0.55, 0.85, 0.95, 1.0]);
            b.forced = true;
            w.particles.push(crate::particles::Request::Burst(b));
        }
        let uid = m.uid;
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(Cue::SonicBoom), pos: source });
        match target {
            EntityRef::Player(player) => self.pending_sounds.push(EntityEvent::PlayerSonicBoom {
                player,
                damage: SONIC_DAMAGE,
                push: Vec3::new((dir.x * 2.5) as f32, (dir.y * 0.5) as f32, (dir.z * 2.5) as f32),
                from: m.pos,
            }),
            EntityRef::Mob(t) => {
                if let Some(j) = self.mobs.iter().position(|m| m.uid == t) {
                    let resist = 1.0 - self.mobs[j].knockback_resistance();
                    let push = DVec3::new(dir.x * 2.5, dir.y * 0.5, dir.z * 2.5) * resist * crate::knockback::TICK_RATE;
                    self.pending_sounds.push(EntityEvent::MobHit {
                        target: t,
                        attacker: uid,
                        damage: SONIC_DAMAGE,
                        knockback: push,
                    });
                }
            }
        }
    }

    /// `Warden.applyDarknessAround` / `MobEffectUtil.addEffectToPlayersAround`:
    /// Darkness (260 ticks) to survival players within `radius` whose
    /// Darkness is missing or has under 200 ticks left.
    pub(super) fn darkness_around(&mut self, at: DVec3, radius: f64, ctx: &Ctx) {
        for p in &ctx.players {
            if p.targetable
                && p.alive
                && !p.spectator
                && p.pos.distance_squared(at) < radius * radius
                && (!p.has_effect(Effect::Darkness) || p.darkness_ticks < DARKNESS_DISPLAY_LIMIT)
            {
                self.pending_sounds.push(EntityEvent::PlayerEffect {
                    player: p.id,
                    effect: Effect::Darkness,
                    amplifier: 0,
                    ticks: DARKNESS_TICKS,
                });
            }
        }
    }

    /// `SculkShriekerBlockEntity.tryRespond` for shriekers that finished:
    /// at warning level 4 a warden is summoned (`SpawnUtil.trySpawnMob`:
    /// 20 tries within 5 blocks across and 6 up or down, standing on a full
    /// top face with nothing solid or liquid in its 0.9 × 2.9 box); otherwise,
    /// or when that fails, the warden's distant reply sounds. Survival
    /// players within 40 blocks get Darkness either way.
    fn answer_shriekers(&mut self, w: &mut World, ctx: &Ctx) {
        for r in w.take_shrieker_responses() {
            let center = r.pos.as_dvec3() + DVec3::splat(0.5);
            let summoned = r.warning >= 4 && self.summon_warden(w, r.pos).is_some();
            if !summoned && let Some(cue) = Cue::reply(r.warning) {
                let offset = IVec3::new(
                    self.rng.next_int(21) as i32 - 10,
                    self.rng.next_int(21) as i32 - 10,
                    self.rng.next_int(21) as i32 - 10,
                );
                let pos = (r.pos + offset).as_dvec3();
                self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Warden(cue), pos });
            }
            self.darkness_around(center, 40.0, ctx);
        }
    }

    /// `SpawnUtil.trySpawnMob(WARDEN, TRIGGERED, level, pos, 20, 5, 6,
    /// ON_TOP_OF_COLLIDER, false)`.
    pub fn summon_warden(&mut self, w: &World, at: IVec3) -> Option<u32> {
        for _ in 0..20 {
            let dx = self.rng.next_int(11) as i32 - 5;
            let dz = self.rng.next_int(11) as i32 - 5;
            let mut p = at + IVec3::new(dx, 6, dz);
            let mut above = w.get_block(p)?;
            let mut found = false;
            for _ in (-6..=6).rev() {
                p.y -= 1;
                let Some(here) = w.get_block(p) else { break };
                if !above.is_solid() && crate::world::mount_states::face_sturdy(here, IVec3::Y) {
                    p.y += 1;
                    found = true;
                    break;
                }
                above = here;
            }
            if !found {
                continue;
            }
            let feet = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            if !warden_fits(w, feet) {
                continue;
            }
            return Some(self.spawn_emerging_warden(feet));
        }
        None
    }

    /// Wardens within the 48-block box around `at`
    /// (`WardenSpawnTracker.hasNearbyWarden`).
    pub fn warden_near(&self, at: DVec3) -> bool {
        self.mobs.iter().any(|m| {
            m.kind == MobKind::Warden && m.alive() && {
                let d = (m.pos - at).abs();
                d.x < 24.0 + 0.45 && d.z < 24.0 + 0.45 && m.pos.y < at.y + 24.0 && m.pos.y + 2.9 > at.y - 24.0
            }
        })
    }
}

/// `Warden.checkSpawnObstruction`: no liquid and no block collision in
/// the 0.9 × 2.9 box at `feet`.
fn warden_fits(w: &World, feet: DVec3) -> bool {
    let min = (feet - DVec3::new(0.45, 0.0, 0.45)).floor().as_ivec3();
    let max = (feet + DVec3::new(0.45, 2.9, 0.45) - DVec3::splat(1e-7)).floor().as_ivec3();
    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                match w.get_block(IVec3::new(x, y, z)) {
                    Some(b) if !b.is_solid() && !b.is_fluid() && !b.is_waterlogged() => {}
                    _ => return false,
                }
            }
        }
    }
    true
}

/// Source of a mob's own game events.
pub fn mob_source(m: &Mob) -> EventSource {
    EventSource { dampens: m.kind == MobKind::Warden, ..EventSource::mob(m.uid) }
}

#[cfg(test)]
mod tests;
