//! Saddleable animals and Java horse attributes, temper, equipment and rider controls.
use super::{Entities, Mob, MobKind, PlayerId, Rng};
use crate::inventory::{Inventory, Stack, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::physics::{self, BlockSource};
use crate::player::{MoveInput, Player};
use glam::DVec3;
use serde_json::{Value, json};

/// Mob vehicle ids occupy the upper half; carts keep their existing ids.
pub const MOB_VEHICLE: u32 = 1 << 31;

/// Java's `AbstractHorse` family: temper taming, mount health and inventory, horse fall damage.
pub fn equine(kind: MobKind) -> bool {
    matches!(
        kind,
        MobKind::SkeletonHorse
            | MobKind::ZombieHorse
            | MobKind::Horse
            | MobKind::Donkey
            | MobKind::Mule
            | MobKind::Llama
            | MobKind::Camel
            | MobKind::CamelHusk
    )
}
pub fn rideable(kind: MobKind) -> bool {
    equine(kind)
        || matches!(kind, MobKind::Pig | MobKind::Strider | MobKind::HappyGhast)
        || super::nautilus::is_nautilus(kind)
}
/// The camel and the camel husk (`CamelHusk extends Camel`): two seats,
/// sitting, the dash and no taming.
pub fn camel_like(kind: MobKind) -> bool {
    matches!(kind, MobKind::Camel | MobKind::CamelHusk)
}
/// Mounts with a charged jump (a camel's charge is its dash).
pub fn jumps(kind: MobKind) -> bool {
    matches!(
        kind,
        MobKind::SkeletonHorse
            | MobKind::ZombieHorse
            | MobKind::Horse
            | MobKind::Donkey
            | MobKind::Mule
            | MobKind::Camel
            | MobKind::CamelHusk
    )
}
/// Whether `m` has Java's mount inventory screen (`HasCustomInventoryScreen`
/// opening `HorseInventoryMenu` or, since v1.0 gap Track 3,
/// `NautilusInventoryMenu`): a tame adult horse, donkey, mule, llama, camel
/// or undead horse, or a tame adult nautilus or zombie nautilus.
pub fn has_mount_screen(m: &Mob) -> bool {
    (equine(m.kind) || super::nautilus::is_nautilus(m.kind)) && !m.baby && m.mount.as_ref().is_some_and(|s| s.tame)
}
/// Donkeys, mules and llamas can carry a chest.
pub fn chested(kind: MobKind) -> bool {
    matches!(kind, MobKind::Donkey | MobKind::Mule | MobKind::Llama)
}
/// Java's `getMaxTemper`: llamas tame within 30 temper, other horses within 100.
pub fn max_temper(kind: MobKind) -> u8 {
    if kind == MobKind::Llama { 30 } else { 100 }
}
/// Carpets are llama decor.
pub fn is_carpet(item: Item) -> bool {
    item.block().is_some_and(|b| b.carpet_color().is_some())
}
/// v1.0 gap Track 3b: what Java's mount inventory screen shows
/// (`AbstractMountInventoryScreen` with `HorseInventoryMenu` or
/// `NautilusInventoryMenu`): the saddle slot at (8, 18), the body slot at
/// (8, 36) with its sprite, and `columns` columns of three chest slots from
/// (80, 18). The vehicle's slots keep their order: 0 the saddle, 1 the body,
/// then the chest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MountScreen {
    pub saddle: bool,
    pub armor: Option<MountArmor>,
    pub columns: usize,
}

/// The body slot's empty sprite: `container/slot/horse_armor`,
/// `llama_armor` or `nautilus_armor_inventory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountArmor {
    Horse,
    Llama,
    Nautilus,
}

impl MountScreen {
    /// Java's slot corners (screen pixels from the panel's top left) for
    /// each of the vehicle's slots the screen shows, by slot index.
    pub fn slots(&self) -> Vec<(usize, f32, f32)> {
        let mut out = Vec::with_capacity(2 + self.columns * 3);
        if self.saddle {
            out.push((0, 8.0, 18.0));
        }
        if self.armor.is_some() {
            out.push((1, 8.0, 36.0));
        }
        for i in 0..self.columns * 3 {
            let (x, y) = (i % self.columns, i / self.columns);
            out.push((2 + i, 80.0 + 18.0 * x as f32, 18.0 + 18.0 * y as f32));
        }
        out
    }
}

/// v1.0 gap Track 3b: Java's `AbstractHorse.canPerformRearing`: every horse,
/// donkey, mule and undead horse rears; llamas, trader llamas and camels
/// (and camel husks) don't.
pub fn rears(kind: MobKind) -> bool {
    equine(kind) && !matches!(kind, MobKind::Llama | MobKind::TraderLlama) && !camel_like(kind)
}

/// v1.0 gap Track 3b: a mount sound Java plays from `Camel` (`CAMEL_SIT`,
/// `CAMEL_STAND`, `CAMEL_DASH`, `CAMEL_DASH_READY`, and `CamelHusk`'s own)
/// or `AbstractHorse.makeMad` (`getAngrySound`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    CamelSit,
    CamelStand,
    CamelDash,
    CamelDashReady,
    CamelHuskSit,
    CamelHuskStand,
    CamelHuskDash,
    CamelHuskDashReady,
    HorseAngry,
    HorseAngryBaby,
    DonkeyAngry,
    MuleAngry,
    ZombieHorseAngry,
    LlamaAngry,
    /// v1.0 gap Track 2c: `AbstractHorse.playJumpSound` and its overrides.
    HorseJump,
    DonkeyJump,
    MuleJump,
    SkeletonHorseJumpWater,
}

impl Cue {
    pub const ALL: [Cue; 18] = [
        Cue::CamelSit,
        Cue::CamelStand,
        Cue::CamelDash,
        Cue::CamelDashReady,
        Cue::CamelHuskSit,
        Cue::CamelHuskStand,
        Cue::CamelHuskDash,
        Cue::CamelHuskDashReady,
        Cue::HorseAngry,
        Cue::HorseAngryBaby,
        Cue::DonkeyAngry,
        Cue::MuleAngry,
        Cue::ZombieHorseAngry,
        Cue::LlamaAngry,
        Cue::HorseJump,
        Cue::DonkeyJump,
        Cue::MuleJump,
        Cue::SkeletonHorseJumpWater,
    ];

    /// The Java sound event.
    pub fn key(self) -> &'static str {
        match self {
            Cue::CamelSit => "entity.camel.sit",
            Cue::CamelStand => "entity.camel.stand",
            Cue::CamelDash => "entity.camel.dash",
            Cue::CamelDashReady => "entity.camel.dash_ready",
            Cue::CamelHuskSit => "entity.camel_husk.sit",
            Cue::CamelHuskStand => "entity.camel_husk.stand",
            Cue::CamelHuskDash => "entity.camel_husk.dash",
            Cue::CamelHuskDashReady => "entity.camel_husk.dash_ready",
            Cue::HorseAngry => "entity.horse.angry",
            Cue::HorseAngryBaby => "entity.baby_horse.angry",
            Cue::DonkeyAngry => "entity.donkey.angry",
            Cue::MuleAngry => "entity.mule.angry",
            Cue::ZombieHorseAngry => "entity.zombie_horse.angry",
            Cue::LlamaAngry => "entity.llama.angry",
            Cue::HorseJump => "entity.horse.jump",
            Cue::DonkeyJump => "entity.donkey.jump",
            Cue::MuleJump => "entity.mule.jump",
            Cue::SkeletonHorseJumpWater => "entity.skeleton_horse.jump_water",
        }
    }

    /// The cue `kind` (a `baby` or not) plays for `event`, if it has one:
    /// a skeleton horse has no angry sound, and only camels sit and dash.
    pub fn for_event(kind: MobKind, baby: bool, event: Event) -> Option<Cue> {
        let husk = kind == MobKind::CamelHusk;
        Some(match (event, kind) {
            (Event::Sit, MobKind::Camel | MobKind::CamelHusk) => {
                if husk {
                    Cue::CamelHuskSit
                } else {
                    Cue::CamelSit
                }
            }
            (Event::Stand, MobKind::Camel | MobKind::CamelHusk) => {
                if husk {
                    Cue::CamelHuskStand
                } else {
                    Cue::CamelStand
                }
            }
            (Event::Dash, MobKind::Camel | MobKind::CamelHusk) => {
                if husk {
                    Cue::CamelHuskDash
                } else {
                    Cue::CamelDash
                }
            }
            (Event::DashReady, MobKind::Camel | MobKind::CamelHusk) => {
                if husk {
                    Cue::CamelHuskDashReady
                } else {
                    Cue::CamelDashReady
                }
            }
            (Event::Angry, MobKind::Horse) => {
                if baby {
                    Cue::HorseAngryBaby
                } else {
                    Cue::HorseAngry
                }
            }
            (Event::Angry, MobKind::Donkey) => Cue::DonkeyAngry,
            (Event::Angry, MobKind::Mule) => Cue::MuleAngry,
            (Event::Angry, MobKind::ZombieHorse) => Cue::ZombieHorseAngry,
            (Event::Angry, MobKind::Llama | MobKind::TraderLlama) => Cue::LlamaAngry,
            // `playJumpSound`: the donkey's and mule's own; a skeleton horse
            // in water its `jump_water`; every other horse `HORSE_JUMP`.
            (Event::Jump { .. }, MobKind::Donkey) => Cue::DonkeyJump,
            (Event::Jump { .. }, MobKind::Mule) => Cue::MuleJump,
            (Event::Jump { water: true }, MobKind::SkeletonHorse) => Cue::SkeletonHorseJumpWater,
            (Event::Jump { .. }, MobKind::Horse | MobKind::SkeletonHorse | MobKind::ZombieHorse) => Cue::HorseJump,
            _ => return None,
        })
    }
}

/// v1.0 gap Track 3b: what a mount did that makes a sound, before the
/// mount's kind picks the cue ([`Cue::for_event`]). `Ambient` is
/// `RandomStandGoal`'s `getAmbientStandSound` (the ordinary ambient call).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Sit,
    Stand,
    Dash,
    DashReady,
    Angry,
    Ambient,
    /// v1.0 gap Track 2c: `handleStartJump`'s `playJumpSound` (0.4 volume),
    /// in water or not.
    Jump {
        water: bool,
    },
}

/// v1.0 gap Track 3b: `AbstractHorse`'s rearing (`setStanding(20)`).
pub const STAND_TICKS: u8 = 20;
/// `Mob.getAmbientSoundInterval`, `RandomStandGoal`'s reset (`getAmbientStandInterval`).
pub const AMBIENT_STAND_INTERVAL: i32 = 80;

/// `Avatar.DEFAULT_VEHICLE_ATTACHMENT`: a riding player's feet are this far
/// below the seat a vehicle offers.
pub const PLAYER_VEHICLE_ATTACHMENT: f64 = 0.6;
/// Ticks between camel dashes (Java's 55).
pub const DASH_COOLDOWN: u16 = 55;

/// v1.0 gap Track 3: Java 26.3's `Camel` (and `CamelHusk`, which extends
/// it) on its own: the pose clock, sitting dimensions and the two seats.
pub mod camel {
    use super::{Mob, State};
    use glam::DVec3;

    /// `SITDOWN_DURATION_TICKS` and `STANDUP_DURATION_TICKS`.
    pub const SITDOWN_TICKS: u32 = 40;
    pub const STANDUP_TICKS: u32 = 52;
    /// `resetLastPoseChangeTickToFullStand`: `gameTime - 52 - 1`.
    pub const FULL_STAND: u32 = STANDUP_TICKS + 1;
    /// `CamelAi.RandomSitting(20)`: 20 seconds in a pose before changing it.
    pub const RANDOM_SIT_TICKS: u32 = 400;
    /// Pose clocks stop counting here; nothing reads past 400.
    pub const POSE_TIME_CAP: u32 = 1_000_000;
    /// `Attributes.JUMP_STRENGTH` (0.42F) and `MOVEMENT_SPEED` (0.09F).
    pub const JUMP_STRENGTH: f64 = 0.42_f32 as f64;
    pub const MOVEMENT_SPEED: f32 = 0.09;
    /// `RUNNING_SPEED_BONUS`, `DASH_HORIZONTAL_MOMENTUM` and `DASH_VERTICAL_MOMENTUM`.
    pub const RUNNING_SPEED_BONUS: f32 = 0.1;
    pub const DASH_HORIZONTAL: f32 = 22.2222;
    pub const DASH_VERTICAL: f32 = 1.4285;
    /// `MAX_HEAD_Y_ROT`, degrees.
    pub const MAX_HEAD_Y_ROT: f32 = 30.0;
    /// `SITTING_HEIGHT_DIFFERENCE` and `BABY_SCALE`.
    pub const SITTING_HEIGHT_DIFFERENCE: f32 = 1.43;
    pub const BABY_SCALE: f32 = 0.6;

    /// `Camel.getDefaultDimensions`: (width, height) for the pose.
    pub fn dimensions(baby: bool, sitting: bool) -> (f64, f64) {
        match (baby, sitting) {
            (true, true) => (0.95_f32 as f64, 0.425_f32 as f64),
            (true, false) => (0.95_f32 as f64, 1.4_f32 as f64),
            (false, true) => (1.7_f32 as f64, (2.375_f32 - SITTING_HEIGHT_DIFFERENCE) as f64),
            (false, false) => (1.7_f32 as f64, 2.375_f32 as f64),
        }
    }

    /// `Camel.getBodyAnchorAnimationYOffset` at a whole tick: how high a
    /// rider's attachment sits for the front (`front`) or rear seat.
    pub fn body_anchor(s: &State, baby: bool, front: bool) -> f64 {
        let scale = if baby { BABY_SCALE } else { 1.0 };
        let height = dimensions(baby, s.camel_sitting).1;
        let mut offset = height - if baby { 0.09375 } else { 0.375 };
        let difference = scale * SITTING_HEIGHT_DIFFERENCE;
        let drop = difference - scale * 0.2;
        let bottom = difference - drop;
        if s.in_pose_transition() {
            let (duration, half, flex) = if s.camel_sitting {
                (SITDOWN_TICKS, 28, if front { 0.5 } else { 0.1 })
            } else {
                (STANDUP_TICKS, if front { 24 } else { 32 }, if front { 0.6 } else { 0.35 })
            };
            let t = (s.pose_time as f32).clamp(0.0, duration as f32);
            let first = t < half as f32;
            let part = if first { t / half as f32 } else { (t - half as f32) / (duration - half) as f32 };
            let flex_point = difference - flex * drop;
            let lerp = |a: f32, b: f32| a + part * (b - a);
            offset += f64::from(if s.camel_sitting {
                if first { lerp(difference, flex_point) } else { lerp(flex_point, bottom) }
            } else if first {
                lerp(bottom - difference, bottom - flex_point)
            } else {
                lerp(bottom - flex_point, 0.0)
            });
        } else if s.camel_sitting {
            offset += f64::from(bottom);
        }
        offset
    }

    /// `Camel.getPassengerAttachmentPoint` for passenger `index` of
    /// `count`: half a block forward alone or in front, 0.7 back behind
    /// (0.2 further forward for an animal), all times the age scale.
    pub fn attachment(m: &Mob, s: &State, index: usize, count: usize, animal: bool) -> DVec3 {
        let scale = if m.baby { f64::from(BABY_SCALE) } else { 1.0 };
        let front = index == 0;
        let mut offset = 0.5;
        if count > 1 {
            if !front {
                offset = -0.7;
            }
            if animal {
                offset += 0.2;
            }
        }
        let forward = DVec3::new(f64::from(m.yaw.cos()), 0.0, f64::from(m.yaw.sin()));
        m.pos + DVec3::Y * body_anchor(s, m.baby, front) + forward * (offset * scale)
    }

    /// How far a camel's body has sunk toward sitting, 0 standing to 1
    /// sitting, for the model.
    pub fn sit_fraction(s: &State, baby: bool) -> f32 {
        let age = if baby { 0.09375 } else { 0.375 };
        let scale = if baby { BABY_SCALE } else { 1.0 };
        let stand = dimensions(baby, false).1 - age;
        let sit = dimensions(baby, true).1 - age + f64::from(scale * 0.2);
        (((stand - body_anchor(s, baby, true)) / (stand - sit)) as f32).clamp(0.0, 1.0)
    }
}

/// Java's `AbstractHorse.createOffspringAttribute`: the parents' mean, spread by a
/// triangular random draw, reflected back inside the attribute's natural range.
pub fn offspring_attribute(a: f64, b: f64, min: f64, max: f64, rng: &mut Rng) -> f64 {
    let (a, b) = (a.clamp(min, max), b.clamp(min, max));
    let spread = (a - b).abs() + 0.15 * (max - min) * 2.;
    let draw = (rng.next_f32() as f64 + rng.next_f32() as f64 + rng.next_f32() as f64) / 3. - 0.5;
    let v = (a + b) / 2. + spread * draw;
    if v > max {
        max - (v - max)
    } else if v < min {
        min + (min - v)
    } else {
        v
    }
}

/// Foal stats and coat from its parents (no-op for animals without mount state).
pub(super) fn inherit(child: &mut Mob, a: &Mob, b: &Mob, rng: &mut Rng) {
    let (Some(c), Some(x), Some(y)) = (child.mount.as_mut(), a.mount.as_deref(), b.mount.as_deref()) else { return };
    if !equine(child.kind) {
        return;
    }
    if child.kind == MobKind::Camel {
        return;
    }
    c.max_health = offspring_attribute(x.max_health as f64, y.max_health as f64, 15., 30., rng) as f32;
    if child.kind == MobKind::Llama {
        // Java: a random strength up to the stronger parent's, rarely one more; a parent's coat.
        let top = x.strength.max(y.strength) as u32 + u32::from(rng.chance(0.03));
        c.strength = (1 + rng.next_int(top.max(1))).min(5) as u8;
        c.variant = if rng.chance(0.5) { x.variant } else { y.variant };
        child.health = c.max_health;
        return;
    }
    c.speed = offspring_attribute(x.speed, y.speed, 0.1125, 0.3375, rng);
    c.jump = offspring_attribute(x.jump, y.jump, 0.4, 1., rng);
    if child.kind == MobKind::Horse {
        // Java: each parent's coat 4/9 of the time, otherwise a random one; markings likewise.
        let mut pick = |p: u8, q: u8, n: u32| match rng.next_int(9) {
            0..=3 => p,
            4..=7 => q,
            _ => rng.next_int(n) as u8,
        };
        c.variant = pick(x.variant, y.variant, 7);
        c.marking = pick(x.marking, y.marking, 5);
    }
    child.health = c.max_health;
}

pub struct State {
    pub tame: bool,
    pub temper: u8,
    pub owner: Option<PlayerId>,
    pub rider: Option<PlayerId>,
    pub max_health: f32,
    /// Java movement_speed attribute, converted to blocks/sec by control().
    pub speed: f64,
    pub jump: f64,
    pub variant: u8,
    pub marking: u8,
    pub chest: bool,
    /// Llama strength 1..=5: three chest slots per point.
    pub strength: u8,
    /// A camel's second (rear) seat.
    pub passenger: Option<PlayerId>,
    /// A llama's caravan: the uid of the llama it follows, and where that llama is.
    pub caravan: Option<u32>,
    pub caravan_pos: Option<DVec3>,
    /// Ticks until a camel can dash again (`Camel.dashCooldown`), and
    /// whether it is mid-dash (`Camel.DASH`, never saved).
    pub dash_cooldown: u16,
    pub dashing: bool,
    /// v1.0 gap Track 3: a camel's pose (`Camel.LAST_POSE_CHANGE_TICK`):
    /// whether it sits, and the ticks since it last sat down or stood up
    /// (`getPoseTime`), which time the 40-tick sit and 52-tick stand.
    pub camel_sitting: bool,
    pub pose_time: u32,
    /// The camel brain's idle `RunOne` (`DoNothing`'s ticks left), and
    /// whether a mob rider steers this camel husk (`isMobControlled`).
    pub(super) idle_wait: u16,
    pub(super) mob_controlled: bool,
    /// v1.0 gap Track 3b: the camel brain's `LOOK_TARGET` memory (a player
    /// within 6 blocks, set by `SetEntityLookTargetSometimes` and erased by
    /// `LookAtTargetSink` after 45 to 89 ticks), its ticker, and the sink's
    /// ticks left.
    pub(super) look_target: Option<(PlayerId, DVec3)>,
    pub(super) look_ticker: i32,
    pub(super) look_left: u16,
    /// v1.0 gap Track 2d: a llama's `RangedAttackGoal.attackTime` in ticks
    /// (-1 with no target; not saved, as in Java).
    pub(super) spit_time: i32,
    /// v1.0 gap Track 3b: `BodyRotationControl`'s `headStableTime` and
    /// `lastStableYHeadRot` (radians, VoxelCraft's yaw).
    pub(super) head_stable: u8,
    pub(super) last_stable_head: f32,
    /// v1.0 gap Track 3b: ticks since `Camel.DASH` turned on, for the dash
    /// animation (`dashAnimationState`), and a released jump the server's
    /// `handleStartJump` still has to see (`floor(charge × 100)`).
    pub dash_ticks: u16,
    started_jump: Option<i32>,
    /// v1.0 gap Track 3b: `AbstractHorse`'s rearing: the `FLAG_STANDING`
    /// countdown (`standCounter`), `allowStandSliding`, `RandomStandGoal`'s
    /// `nextStand`, and the client's `standAnim` (now and a tick ago),
    /// which lifts and shifts the rider and drives the rearing animation.
    pub stand_counter: u8,
    allow_stand_sliding: bool,
    next_stand: i32,
    pub stand_anim: f32,
    pub stand_anim_o: f32,
    /// v1.0 gap Track 2c: `AbstractHorse`'s grazing: `FLAG_EATING`
    /// (`EatingHaystack`, saved only while set), `eatingCounter`, the
    /// client's `eatAnim` (now and a tick ago) for the head-down animation,
    /// and the mouth (`FLAG_OPEN_MOUTH`'s `mouthCounter` and `mouthAnim`).
    /// `on_grass` is whether a grass block is under its feet this tick.
    pub eating: bool,
    eating_counter: u8,
    pub eat_anim: f32,
    pub eat_anim_o: f32,
    mouth_counter: u8,
    pub mouth_anim: f32,
    pub mouth_anim_o: f32,
    pub(crate) on_grass: bool,
    /// v1.0 gap Track 3b: sounds this mount made since the last update.
    pub(super) events: Vec<Event>,
    /// A nautilus's dash cooldown as the rider's jump charge sees it
    /// (`PlayerRideableJumping.getJumpCooldown`), and whether the mount
    /// stood on the ground when the rider let go of jump.
    pub(super) jump_cooldown: i32,
    pub(super) vehicle_on_ground: bool,
    /// Saddle (llamas: unused), horse armor or llama carpet, then fifteen storage slots.
    /// Happy ghasts wear their harness and nautiluses their armor in slot 1.
    pub slots: [Option<Stack>; 17],
    /// A happy ghast's third and fourth seats (`MAX_PASSANGERS` is 4).
    pub seats: [Option<PlayerId>; 2],
    /// The controlling rider's look pitch (radians, positive up) and raw
    /// forward, strafe and jump keys, for flying and swimming mounts that
    /// steer with Java's `getRiddenInput`.
    pub look_pitch: f32,
    pub forward: f32,
    pub strafe: f32,
    pub ascend: bool,
    pub jump_charge: f32,
    /// `LocalPlayer.jumpRidingTicks` (negative for ten ticks after a jump).
    jump_ticks: i32,
    jump_held: bool,
    pending_jump: Option<f32>,
    pub boost_ticks: u16,
    pub boost_total: u16,
    wish: DVec3,
    /// The controlling rider's yaw (radians).
    pub(crate) yaw: f32,
    steering: bool,
    sprinting: bool,
    pub fall_distance: f64,
    pub(super) rider_fall: f32,
    regen_ticks: f32,
}

impl State {
    pub fn new(kind: MobKind, rng: &mut Rng) -> Self {
        let horse = kind == MobKind::Horse;
        let camel = camel_like(kind);
        let max_health = if equine(kind) && !camel && !super::undead_mounts::undead(kind) {
            (15 + rng.next_int(8) + rng.next_int(9)) as f32
        } else {
            kind.max_health()
        };
        let speed = if kind == MobKind::SkeletonHorse {
            f64::from(0.2_f32)
        } else if kind == MobKind::ZombieHorse {
            (9. + rng.next_f32() as f64 + rng.next_f32() as f64 + rng.next_f32() as f64) / f64::from(42.16_f32)
        } else if horse {
            (0.45 + rng.next_f32() as f64 * 0.3 + rng.next_f32() as f64 * 0.3 + rng.next_f32() as f64 * 0.3) * 0.25
        } else if camel {
            0.09
        } else if kind == MobKind::HappyGhast {
            super::happy_ghast::FLYING_SPEED
        } else if super::nautilus::is_nautilus(kind) {
            super::nautilus::movement_speed(kind)
        } else {
            0.175
        };
        // Java: strength 1..=3, or 1..=5 one time in 25.
        let strength = if kind == MobKind::Llama {
            let top = if rng.chance(0.04) { 5 } else { 3 };
            1 + rng.next_int(top) as u8
        } else {
            3
        };
        let jump = if camel {
            camel::JUMP_STRENGTH
        } else if kind == MobKind::ZombieHorse {
            0.5 + (rng.next_f32() as f64 + rng.next_f32() as f64 + rng.next_f32() as f64) / 15.
        } else if horse || kind == MobKind::SkeletonHorse {
            0.4 + rng.next_f32() as f64 * 0.2 + rng.next_f32() as f64 * 0.2 + rng.next_f32() as f64 * 0.2
        } else {
            0.5
        };
        Self {
            // Camels need no taming, only a saddle; nautiluses are tamed with pufferfish.
            tame: (!equine(kind) || camel) && !super::nautilus::is_nautilus(kind),
            temper: 0,
            owner: None,
            rider: None,
            max_health,
            speed,
            jump,
            variant: match kind {
                MobKind::Horse => rng.next_int(7) as u8,
                MobKind::Llama => rng.next_int(4) as u8,
                _ => 0,
            },
            marking: if horse { rng.next_int(5) as u8 } else { 0 },
            chest: false,
            strength,
            passenger: None,
            caravan: None,
            caravan_pos: None,
            dash_cooldown: 0,
            dashing: false,
            camel_sitting: false,
            pose_time: camel::FULL_STAND,
            idle_wait: 0,
            mob_controlled: false,
            look_target: None,
            look_ticker: 0,
            look_left: 0,
            spit_time: -1,
            head_stable: 0,
            last_stable_head: 0.0,
            dash_ticks: 0,
            started_jump: None,
            stand_counter: 0,
            allow_stand_sliding: false,
            next_stand: -AMBIENT_STAND_INTERVAL,
            stand_anim: 0.0,
            stand_anim_o: 0.0,
            eating: false,
            eating_counter: 0,
            eat_anim: 0.0,
            eat_anim_o: 0.0,
            mouth_counter: 0,
            mouth_anim: 0.0,
            mouth_anim_o: 0.0,
            on_grass: false,
            events: Vec::new(),
            jump_cooldown: 0,
            vehicle_on_ground: true,
            slots: [None; 17],
            seats: [None; 2],
            look_pitch: 0.,
            forward: 0.,
            strafe: 0.,
            ascend: false,
            jump_charge: 0.,
            jump_ticks: 0,
            jump_held: false,
            pending_jump: None,
            boost_ticks: 0,
            boost_total: 0,
            wish: DVec3::ZERO,
            yaw: 0.,
            steering: false,
            sprinting: false,
            fall_distance: 0.,
            rider_fall: 0.,
            regen_ticks: 0.,
        }
    }
    pub fn controlled(&self) -> bool {
        self.rider.is_some() && self.steering
    }
    #[cfg(test)]
    pub fn pending_jump_for_test(&mut self, charge: f32) {
        self.pending_jump = Some(charge);
    }
    /// The charged jump the rider released, once (a nautilus's dash).
    pub fn take_pending_jump(&mut self) -> Option<f32> {
        self.pending_jump.take()
    }
    pub fn saddled(&self) -> bool {
        self.slots[0].is_some_and(|s| s.item == Item::SADDLE)
    }
    pub fn slot_count(&self, kind: MobKind) -> usize {
        match kind {
            MobKind::HappyGhast => 2,
            k if super::nautilus::is_nautilus(k) => 2,
            MobKind::Llama if self.chest => 2 + 3 * self.strength as usize,
            _ if self.chest => 17,
            MobKind::SkeletonHorse | MobKind::ZombieHorse | MobKind::Horse | MobKind::Llama => 2,
            _ => 1,
        }
    }
    pub fn accepts(&self, kind: MobKind, slot: usize, item: Item) -> bool {
        if kind == MobKind::HappyGhast {
            return slot == 1 && crate::content::harness::colour(item).is_some();
        }
        if !self.tame {
            return false;
        }
        if super::nautilus::is_nautilus(kind) {
            return match slot {
                0 => item == Item::SADDLE,
                1 => super::nautilus::wears(item),
                _ => false,
            };
        }
        match slot {
            0 => item == Item::SADDLE && kind != MobKind::Llama,
            1 => wears_horse_armor(kind) && horse_material(item).is_some() || kind == MobKind::Llama && is_carpet(item),
            2..=16 => self.chest && slot < self.slot_count(kind),
            _ => false,
        }
    }
    pub fn input(
        &mut self,
        kind: MobKind,
        player: &Player,
        input: MoveInput,
        held: Option<Item>,
        offhand: Option<Item>,
    ) {
        self.wish = player.ride_push(input) / 0.22;
        self.yaw = player.yaw;
        self.look_pitch = player.pitch;
        self.forward = input.forward as f32;
        self.strafe = input.right as f32;
        self.ascend = input.jump;
        self.sprinting = input.sprint;
        let steering_item = match kind {
            MobKind::Pig => Some(Item::CARROT_ON_A_STICK),
            MobKind::Strider => Some(Item::WARPED_FUNGUS_ON_A_STICK),
            _ => None,
        };
        self.steering =
            self.saddled() && self.tame && steering_item.is_none_or(|i| held == Some(i) || offhand == Some(i));
        if kind == MobKind::HappyGhast {
            // `HappyGhast.getControllingPassenger`: a harness; the still
            // timeout is checked where it flies.
            self.steering = self.slots[1].is_some();
        }
        // `LocalPlayer.aiStep`'s riding jump (v1.0 gap Track 3): only for a
        // vehicle the rider controls that `canJump` (saddled; a camel not
        // sitting or changing pose) while its `getJumpCooldown` is 0.
        let camel = camel_like(kind);
        let jumpable =
            (equine(kind) || super::nautilus::is_nautilus(kind)) && self.steering && !(camel && self.refuses_to_move());
        let cooldown = if camel { i32::from(self.dash_cooldown) } else { self.jump_cooldown };
        if jumpable && cooldown == 0 {
            if self.jump_ticks < 0 {
                self.jump_ticks += 1;
                if self.jump_ticks == 0 {
                    self.jump_charge = 0.;
                }
            }
            if self.jump_held && !input.jump {
                self.jump_ticks = -10;
                let amount = (self.jump_charge * 100.0).floor() as i32;
                // `onPlayerJump`: a camel dashes only from the ground; a
                // saddled horse rears (`standIfPossible`, sliding allowed).
                if !camel || self.vehicle_on_ground {
                    self.pending_jump = Some(self.jump_charge);
                }
                if !camel && self.saddled() && rears(kind) {
                    self.allow_stand_sliding = true;
                    self.stand_if_possible(kind);
                }
                // `sendRidingJump`: the server's `handleStartJump` runs
                // when the vehicle `canJump` and the amount is above 0.
                if amount > 0 {
                    self.started_jump = Some(amount);
                }
            } else if !self.jump_held && input.jump {
                self.jump_ticks = 0;
                self.jump_charge = 0.;
            } else if self.jump_held {
                self.jump_ticks += 1;
                self.jump_charge = jump_charge(self.jump_ticks);
            }
        } else {
            self.jump_charge = 0.;
        }
        self.jump_held = input.jump;
    }
    /// `Camel.tick`'s dash bookkeeping, once a tick: the dash ends once
    /// the cooldown is under 50 and the camel is down (on the ground, in a
    /// liquid or riding), and the cooldown counts down.
    pub(super) fn dash_clock(&mut self, settled: bool) {
        if self.dashing && self.dash_cooldown < 50 && settled {
            self.dashing = false;
        }
        // `Camel.tick`: the dash-ready sound as the cooldown runs out.
        if self.dash_cooldown > 0 {
            self.dash_cooldown -= 1;
            if self.dash_cooldown == 0 {
                self.events.push(Event::DashReady);
            }
        }
        self.dash_ticks = if self.dashing { self.dash_ticks.saturating_add(1) } else { 0 };
    }
    /// v1.0 gap Track 3b: `Camel.handleStartJump` for a released jump the
    /// camel `canJump` from (saddled, not sitting or changing pose): the
    /// dash sound and `DASH`, whose change sets a 55-tick cooldown when
    /// none is running (`onSyncedDataUpdated`), even for a release in the
    /// air that didn't dash. `AbstractHorse.handleStartJump` rears.
    pub(super) fn start_jump(&mut self, kind: MobKind, in_water: bool) {
        let Some(_amount) = self.started_jump.take() else { return };
        if !self.saddled() {
            return;
        }
        if camel_like(kind) {
            if self.refuses_to_move() {
                return;
            }
            self.events.push(Event::Dash);
            if !self.dashing {
                self.dashing = true;
                if self.dash_cooldown == 0 {
                    self.dash_cooldown = DASH_COOLDOWN;
                }
            }
        } else if rears(kind) {
            self.allow_stand_sliding = true;
            self.stand_if_possible(kind);
            // v1.0 gap Track 2c: `playJumpSound`.
            self.events.push(Event::Jump { water: in_water });
        }
    }
    /// `AbstractHorse.standIfPossible`: rear for 20 ticks (`setStanding`).
    pub fn stand_if_possible(&mut self, kind: MobKind) {
        if rears(kind) {
            // `setStanding`: rearing ends a meal.
            self.eating = false;
            self.stand_counter = STAND_TICKS;
        }
    }
    /// v1.0 gap Track 2c: `AbstractHorse.canEatGrass` (llamas and camels
    /// don't graze).
    pub fn grazes(kind: MobKind) -> bool {
        rears(kind)
    }
    #[cfg(test)]
    pub(crate) fn started_jump_for_test(&mut self, amount: i32) {
        self.started_jump = Some(amount);
    }
    /// v1.0 gap Track 2c: `AbstractHorse.isImmobile`: grazing or rearing.
    pub fn immobile(&self) -> bool {
        self.eating || self.standing()
    }
    /// v1.0 gap Track 2c: one tick of `AbstractHorse.aiStep`'s grazing (a
    /// 1-in-300 start while nobody rides it and a grass block is under it,
    /// ending after 50 ticks) for a living mount, then `tick`'s
    /// `eatAnim` and `mouthAnim` (`standAnim` zeroes the eating one).
    pub(super) fn eat_tick(&mut self, kind: MobKind, alive: bool, rng: &mut Rng) {
        if !Self::grazes(kind) {
            return;
        }
        if alive {
            let vehicle = self.rider.is_some() || self.passenger.is_some();
            if !self.eating && !vehicle && rng.next_int(300) == 0 && self.on_grass {
                self.eating = true;
            }
            if self.eating {
                self.eating_counter += 1;
                if self.eating_counter > 50 {
                    self.eating_counter = 0;
                    self.eating = false;
                }
            }
        }
        if self.mouth_counter > 0 {
            self.mouth_counter += 1;
            if self.mouth_counter > 30 {
                self.mouth_counter = 0;
            }
        }
        self.eat_anim_o = self.eat_anim;
        self.eat_anim = if self.eating {
            (self.eat_anim + ((1.0 - self.eat_anim) * 0.4 + 0.05)).min(1.0)
        } else {
            (self.eat_anim + ((0.0 - self.eat_anim) * 0.4 - 0.05)).max(0.0)
        };
        if self.standing() {
            self.eat_anim = 0.0;
            self.eat_anim_o = 0.0;
        }
        self.mouth_anim_o = self.mouth_anim;
        self.mouth_anim = if self.mouth_counter > 0 {
            (self.mouth_anim + ((1.0 - self.mouth_anim) * 0.7 + 0.05)).min(1.0)
        } else {
            (self.mouth_anim + ((0.0 - self.mouth_anim) * 0.7 - 0.05)).max(0.0)
        };
    }
    /// v1.0 gap Track 2c: `AbstractHorse.eating`'s `openMouth` when fed.
    pub fn open_mouth(&mut self) {
        self.mouth_counter = 1;
    }
    /// `AbstractHorse.getEatAnim` at `partial` of the way through the tick.
    pub fn eat_anim(&self, partial: f32) -> f32 {
        self.eat_anim_o + (self.eat_anim - self.eat_anim_o) * partial
    }
    /// `AbstractHorse.makeMad`: rear (if it can) and play the angry sound,
    /// unless it already rears.
    pub fn make_mad(&mut self, kind: MobKind) {
        if !self.standing() {
            self.stand_if_possible(kind);
            self.events.push(Event::Angry);
        }
    }
    /// `AbstractHorse.isStanding` (`FLAG_STANDING`): rearing. The rearing
    /// animation reads [`State::stand_anim`].
    pub fn standing(&self) -> bool {
        self.stand_counter > 0
    }
    /// `AbstractHorse.getStandAnim`: the rearing animation's progress at
    /// `partial` of the way through the tick, 0 to 1.
    pub fn stand_anim(&self, partial: f32) -> f32 {
        self.stand_anim_o + (self.stand_anim - self.stand_anim_o) * partial
    }
    /// `AbstractHorse.getRiddenInput` holds a rearing horse on the ground
    /// unless a jump is pending or `allowStandSliding`.
    pub(super) fn rearing_holds(&self, on_ground: bool) -> bool {
        on_ground && self.pending_jump.is_none() && self.standing() && !self.allow_stand_sliding
    }
    /// One tick of `AbstractHorse.tick`'s standing countdown and
    /// `standAnim`, and `RandomStandGoal` (every tick, `requiresUpdateEveryTick`)
    /// for an AI that runs: `nextStand` counts up from -80, and once past 0
    /// a `nextStand`-in-1000 roll resets it and, one time in ten, rears
    /// with the ambient call. `immobile` is `isImmobile` (eating or rearing).
    pub(super) fn rear_tick(&mut self, kind: MobKind, ai: bool, rng: &mut Rng) {
        if !rears(kind) {
            return;
        }
        if ai {
            self.next_stand += 1;
            if self.next_stand > 0 && (rng.next_int(1000) as i32) < self.next_stand {
                self.next_stand = -AMBIENT_STAND_INTERVAL;
                if !self.standing() && rng.next_int(10) == 0 {
                    self.stand_if_possible(kind);
                    self.events.push(Event::Ambient);
                }
            }
        }
        if self.stand_counter > 0 {
            self.stand_counter -= 1;
        }
        self.stand_anim_o = self.stand_anim;
        if self.standing() {
            self.stand_anim = (self.stand_anim + ((1.0 - self.stand_anim) * 0.4 + 0.05)).min(1.0);
        } else {
            self.allow_stand_sliding = false;
            let a = self.stand_anim;
            self.stand_anim = (a + ((0.8 * a * a * a - a) * 0.6 - 0.05)).max(0.0);
        }
    }
    /// The sounds since the last call, oldest first.
    pub(super) fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    /// The controlling rider's forward key (`zza`) and sprinting.
    pub fn rider_forward(&self) -> f32 {
        self.forward
    }
    pub fn rider_sprinting(&self) -> bool {
        self.sprinting
    }
    /// `Camel.isInPoseTransition`: sitting down takes 40 ticks, standing up 52.
    pub fn in_pose_transition(&self) -> bool {
        self.pose_time < if self.camel_sitting { camel::SITDOWN_TICKS } else { camel::STANDUP_TICKS }
    }
    /// `Camel.refuseToMove`: sitting, or between poses.
    pub fn refuses_to_move(&self) -> bool {
        self.camel_sitting || self.in_pose_transition()
    }
    /// `Camel.sitDown` and `standUp`: the pose changes and its clock restarts.
    /// v1.0 gap Track 3b: with Java's `CAMEL_SIT` and `CAMEL_STAND` sounds.
    pub fn sit_down(&mut self) {
        if !self.camel_sitting {
            self.camel_sitting = true;
            self.pose_time = 0;
            self.events.push(Event::Sit);
        }
    }
    pub fn stand_up(&mut self) {
        if self.camel_sitting {
            self.camel_sitting = false;
            self.pose_time = 0;
            self.events.push(Event::Stand);
        }
    }
    /// `Camel.standUpInstantly`: standing, with no transition left.
    pub fn stand_up_instantly(&mut self) {
        self.camel_sitting = false;
        self.pose_time = camel::FULL_STAND;
    }
    pub fn save(&self) -> Value {
        let mut v = json!({"tame":self.tame,"temper":self.temper,"owner":self.owner.map(|p|p.0),"rider":self.rider.map(|p|p.0),
            "health":self.max_health,"speed":self.speed,"jump":self.jump,"variant":self.variant,"marking":self.marking,
            "chest":self.chest,"strength":self.strength,"passenger":self.passenger.map(|p|p.0),
            "slots":self.slots.iter().map(|s|stack_to_string(*s)).collect::<Vec<_>>()});
        // v1.0 gap Track 3: a camel's pose and clock. A mount whose pose
        // never changed (every other mount, a fresh camel) writes the old
        // standing keys, so its record saves byte for byte as before.
        if self.camel_sitting || self.pose_time != camel::FULL_STAND {
            v["camel_sitting"] = json!(self.camel_sitting);
            v["pose_time"] = json!(self.pose_time);
        } else {
            v["sit_ticks"] = json!(0);
            v["stand_ticks"] = json!(0);
        }
        // A happy ghast's third and fourth seats, written only when taken so
        // older mounts save byte for byte as before.
        if self.seats.iter().any(Option::is_some) {
            v["seats"] = json!(self.seats.map(|p| p.map(|p| p.0)));
        }
        // v1.0 gap Track 2c: Java's `EatingHaystack`, written only while it
        // grazes so other mounts save byte for byte as before.
        if self.eating {
            v["eating"] = json!(true);
        }
        v
    }
    pub fn load(kind: MobKind, v: &Value) -> Option<Self> {
        let mut s = Self::new(kind, &mut Rng::new(0));
        s.tame = v["tame"].as_bool()?;
        s.temper = u8::try_from(v["temper"].as_u64()?).ok()?.min(100);
        s.owner = v["owner"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
        s.rider = v["rider"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
        s.max_health = v["health"].as_f64()? as f32;
        s.speed = v["speed"].as_f64()?;
        s.jump = v["jump"].as_f64()?;
        // Happy ghasts and nautiluses keep their fixed attribute speeds.
        let speeds =
            if kind == MobKind::HappyGhast || super::nautilus::is_nautilus(kind) { 0.01..=1.2 } else { 0.05..=0.4 };
        if !s.max_health.is_finite()
            || !(1. ..=40.).contains(&s.max_health)
            || !speeds.contains(&s.speed)
            || !(0.4..=1.).contains(&s.jump)
        {
            return None;
        }
        s.variant = v["variant"].as_u64()?.min(6) as u8;
        s.marking = v["marking"].as_u64()?.min(4) as u8;
        s.chest = v["chest"].as_bool().unwrap_or(false) && chested(kind);
        s.strength = v["strength"].as_u64().map_or(s.strength, |n| n.clamp(1, 5) as u8);
        s.passenger = v["passenger"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
        s.eating = v["eating"].as_bool().unwrap_or(false) && Self::grazes(kind);
        if kind == MobKind::HappyGhast
            && let Some(seats) = v["seats"].as_array()
        {
            for (seat, p) in s.seats.iter_mut().zip(seats) {
                *seat = p.as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId);
            }
        }
        if camel_like(kind) {
            s.jump = camel::JUMP_STRENGTH;
            if let Some(sitting) = v["camel_sitting"].as_bool() {
                s.camel_sitting = sitting;
                s.pose_time =
                    v["pose_time"].as_u64().map_or(camel::FULL_STAND, |t| t.min(camel::POSE_TIME_CAP as u64) as u32);
            } else if v["sit_ticks"].as_u64().unwrap_or(0) > 0 {
                // Saves before v1.0 gap Track 3 counted down a sit: it is
                // seated, free to stand when its brain chooses.
                s.camel_sitting = true;
                s.pose_time = camel::RANDOM_SIT_TICKS;
            } else if let Some(left) = v["stand_ticks"].as_u64().filter(|&t| t > 0) {
                s.pose_time = camel::STANDUP_TICKS - left.min(u64::from(camel::STANDUP_TICKS)) as u32;
            }
        }
        if kind == MobKind::Llama {
            s.variant = s.variant.min(3);
        }
        for (i, piece) in v["slots"].as_array()?.iter().take(17).enumerate() {
            let stack = stack_from_str(piece.as_str()?)?;
            // Older builds let skeleton horses wear horse armor; it loads so
            // that `Entities::load_nether_mobs` can drop it beside the horse.
            let legacy = i == 1 && kind == MobKind::SkeletonHorse && stack.is_some_and(|s| armor_points(s.item) > 0);
            if let Some(stack) = stack
                && !s.accepts(kind, i, stack.item)
                && !legacy
            {
                return None;
            }
            s.slots[i] = stack;
        }
        Some(s)
    }
}

/// Java's `#can_wear_horse_armor` (horse and zombie horse): the animals horse
/// armor's `Equippable` lets on. Skeleton horses have the body slot but no tag.
pub fn wears_horse_armor(kind: MobKind) -> bool {
    matches!(kind, MobKind::Horse | MobKind::ZombieHorse)
}

/// Horse armor's material, for the horse armor items only.
fn horse_material(item: Item) -> Option<crate::item::ArmorMaterial> {
    item.body_armor().filter(|(fits, _)| *fits == crate::equipment::BodyArmor::Horse).map(|(_, m)| m)
}

/// Any body armor's material: horse armor, or nautilus armor (v0.15).
fn body_material(item: Item) -> Option<crate::item::ArmorMaterial> {
    item.body_armor().map(|(_, m)| m)
}

/// The armor a horse or nautilus gains from wearing `item` (`ArmorType.BODY`
/// defense); 0 for anything that is not body armor.
pub fn armor_points(item: Item) -> u32 {
    body_material(item).map_or(0, |m| u32::from(m.body_defense()))
}

/// The armor toughness body armor adds (`ArmorMaterial.toughness`).
pub fn armor_toughness(item: Item) -> f32 {
    body_material(item).map_or(0.0, |m| m.toughness())
}

/// The knockback resistance body armor adds (netherite's 0.1).
pub fn armor_knockback_resistance(item: Item) -> f32 {
    body_material(item).map_or(0.0, |m| m.knockback_resistance())
}
/// Java charge peaks after ten ticks then drops toward 0.8 for a long hold.
pub fn jump_charge(ticks: i32) -> f32 {
    if ticks < 10 { ticks as f32 * 0.1 } else { 0.8 + 2. / (ticks - 9) as f32 * 0.1 }
}

/// Java's horse jump height in blocks for a jump strength `j` (the Minecraft Wiki's fit of
/// vanilla's per-tick motion: 1.1 blocks at 0.4 up to 5.3 at 1.0).
fn java_peak(j: f64) -> f64 {
    ((-0.181_758_495_2 * j + 3.689_713_992) * j + 2.128_599_134) * j - 0.343_930_367
}

/// Launch speed (blocks/s) that reaches Java's peak for jump strength `v` under
/// this crate's continuous equine physics: dv/dt = -g - c·v, with g = 32 and c = -20·ln 0.98.
pub fn launch_speed(v: f64) -> f64 {
    const G: f64 = 32.;
    let c = -20. * 0.98f64.ln();
    let peak = |v0: f64| v0 / c - G / (c * c) * (1. + c * v0 / G).ln();
    let target = java_peak(v);
    let (mut lo, mut hi) = (0., 80.);
    for _ in 0..50 {
        let mid = (lo + hi) / 2.;
        if peak(mid) < target { lo = mid } else { hi = mid }
    }
    lo
}

/// Java `handleEating`: health healed, temper gained and baby growth in ticks.
pub fn food(kind: MobKind, item: Item) -> Option<(f32, u8, i32)> {
    if kind == MobKind::SkeletonHorse {
        return None;
    }
    if kind == MobKind::ZombieHorse {
        return (item == Item::from_block(crate::world::block::Block::RED_MUSHROOM)).then_some((3., 3, 0));
    }
    let hay = Item::from_block(crate::world::block::Block::HAY_BALE);
    if kind == MobKind::Llama {
        return match item {
            Item::WHEAT => Some((2., 3, 200)),
            i if i == hay => Some((10., 6, 1800)),
            _ => None,
        };
    }
    if kind == MobKind::Camel {
        return (item == Item::from_block(crate::world::block::Block::CACTUS)).then_some((2., 0, 200));
    }
    // `#camel_husk_food`: a rabbit's foot heals 2; husks never love or grow.
    if kind == MobKind::CamelHusk {
        return (item == Item::RABBIT_FOOT).then_some((2., 0, 0));
    }
    Some(match item {
        Item::SUGAR => (1., 3, 600),
        Item::WHEAT => (2., 3, 400),
        Item::APPLE => (3., 3, 1200),
        Item::GOLDEN_CARROT => (4., 5, 1200),
        Item::GOLDEN_APPLE | Item::ENCHANTED_GOLDEN_APPLE => (10., 10, 4800),
        i if i == Item::from_block(crate::world::block::Block::HAY_BALE) => (20., 0, 3600),
        _ => return None,
    })
}

/// Run before ordinary AI movement. Untamed mounts run and occasionally buck/tame.
/// Tamed unsaddled animals and pigs without a steering item keep their normal AI.
pub(super) fn control(mob: &mut Mob, dt: f64, rng: &mut Rng) -> Option<(Option<DVec3>, f64)> {
    if mob.kind == MobKind::HappyGhast || super::nautilus::is_nautilus(mob.kind) {
        // Flying and swimming mounts travel in their own Java ticks
        // (`happy_ghast`, `nautilus`); a steering rider replaces their AI.
        mob.mount.as_ref()?;
        let steered =
            if mob.kind == MobKind::HappyGhast { mob.happy_ghast_controlled() } else { mob.nautilus_controlled() };
        return (steered && mob.alive()).then_some((None, 0.));
    }
    let s = mob.mount.as_mut()?;
    if mob.health <= 0.0 {
        s.rider = None;
        s.passenger = None;
        return None;
    }
    let ticks = (dt * 20.).round() as u32;
    s.regen_ticks += dt as f32 * 20.;
    if equine(mob.kind) && s.regen_ticks >= 900. {
        mob.health = (mob.health + 1.).min(s.max_health);
        s.regen_ticks = 0.;
    }
    // v1.0 gap Track 3b: `AbstractHorse`'s rearing a tick at a time;
    // `RandomStandGoal` runs while the AI does (not while it rears).
    for _ in 0..ticks {
        let ai = !s.immobile();
        s.rear_tick(mob.kind, ai, rng);
        s.eat_tick(mob.kind, mob.health > 0.0, rng);
    }
    if camel_like(mob.kind) {
        // v1.0 gap Track 3: the pose clock, and `Camel.tick` standing a
        // sitting camel up in water. Sitting and standing otherwise come
        // from its brain (`Mob::camel_idle`) and rider; a steered camel's
        // dash clock runs in its ridden tick.
        s.pose_time = s.pose_time.saturating_add(ticks).min(camel::POSE_TIME_CAP);
        if s.camel_sitting && mob.in_water {
            s.stand_up_instantly();
        }
        if !s.controlled() {
            for _ in 0..ticks {
                s.dash_clock(mob.on_ground || mob.in_water || mob.riding.is_some());
            }
        }
    } else {
        s.dash_cooldown = s.dash_cooldown.saturating_sub(ticks as u16);
    }
    // `LivingEntity.aiStep`: a rearing or grazing horse is immobile
    // (`isImmobile`): its AI doesn't run and it walks nowhere.
    if s.immobile() && !s.controlled() {
        return Some((None, 0.));
    }
    // A caravan llama walks after the one ahead, keeping about two blocks back.
    if s.rider.is_none()
        && let Some(ahead) = s.caravan_pos
    {
        let flat = (ahead - mob.pos) * DVec3::new(1., 0., 1.);
        if flat.length_squared() > 2.5 * 2.5 {
            // `LlamaFollowCaravanGoal` doesn't look (v1.0 gap Track 2c).
            return Some((Some(flat.normalize()), 2.6));
        }
    }
    let rider = s.rider?;
    if !s.tame {
        if rng.chance((dt as f32 * 20. / 50.).min(1.)) {
            if rng.next_int(max_temper(mob.kind) as u32) < s.temper as u32 {
                s.tame = true;
                s.owner = Some(rider);
            } else {
                // `RunAroundLikeCrazyGoal.tick`: more temper, the rider
                // thrown off, then `makeMad` (v1.0 gap Track 3b).
                s.temper = (s.temper + 5).min(max_temper(mob.kind));
                s.rider = None;
                s.make_mad(mob.kind);
            }
        }
        return Some((Some(DVec3::new(mob.yaw.cos() as f64, 0., mob.yaw.sin() as f64)), 2.4));
    }
    if !s.steering {
        return None;
    }
    // v1.0 Track 7: steered horses take their whole Java tick (and the
    // released jump) in `Mob::equine_ridden_tick`; camels too since v1.0
    // gap Track 3.
    if super::mob::equine::java_ridden(mob.kind) {
        return Some((None, 0.));
    }
    mob.yaw = s.yaw;
    let forward = DVec3::new(mob.yaw.cos() as f64, 0., mob.yaw.sin() as f64);
    if mob.on_ground
        && let Some(charge) = s.pending_jump.take()
    {
        mob.vel.y = launch_speed(s.jump * if charge >= 0.9 { 1. } else { 0.4 + 0.4 * charge as f64 / 0.9 });
    }
    let mut speed = if equine(mob.kind) {
        s.speed * 43.17
    } else if mob.kind == MobKind::Pig {
        2.42
    } else if mob.nether.as_ref().is_some_and(|n| n.cold) {
        1.74
    } else {
        4.14
    };
    if s.boost_total > 0 {
        s.boost_ticks = s.boost_ticks.saturating_add(1);
        if s.boost_ticks > s.boost_total {
            s.boost_total = 0;
            s.boost_ticks = 0;
        } else {
            speed *= 1. + 1.15 * (std::f64::consts::PI * s.boost_ticks as f64 / s.boost_total as f64).sin();
        }
    }
    Some((Some(if equine(mob.kind) { s.wish } else { forward }), speed))
}

impl Entities {
    /// Java llama behaviour beyond riding: spit at whoever hurt them (once per hit) and at
    /// wild wolves, scare wild wolves off, and follow a led llama in a caravan.
    pub(super) fn tick_llamas(&mut self, ctx: &super::Ctx) {
        if !self.mobs.iter().any(|m| m.kind == MobKind::Llama) {
            return;
        }
        self.ensure_mob_ids();
        // Spitting: Java's LlamaSpit, 1.5 blocks a tick with a little spread.
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if !m.simulation_active || m.kind != MobKind::Llama || !m.alive() || m.baby {
                continue;
            }
            let player = m.angry_player.and_then(|id| ctx.players.iter().find(|t| t.id == id && t.targetable));
            // v1.0 gap Track 2d: `RangedAttackGoal.tick` looks at the target's
            // eyes every tick it runs (`setLookAt(target, 30, 30)`).
            let mut eye = player.map(|t| t.eye());
            let target = if let Some(t) = player {
                // Java aims at a third of the target's height.
                Some(t.pos + DVec3::Y * 0.6)
            } else {
                // Java's LlamaAttackWolfGoal: wild wolves in range get spat at.
                self.mobs
                    .iter()
                    .filter(|w| {
                        w.kind == MobKind::Wolf
                            && w.alive()
                            && w.animal.as_ref().is_some_and(|a| a.owner.is_none())
                            && w.pos.distance_squared(m.pos) < 16.0 * 16.0
                    })
                    .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                    .map(|w| {
                        eye = Some(w.look_eye());
                        w.pos + DVec3::Y * (w.shape().height / 3.0)
                    })
            };
            let Some((target, eye)) = target.zip(eye) else {
                // `RangedAttackGoal.stop`: `attackTime = -1`.
                if let Some(s) = self.mobs[i].mount.as_mut() {
                    s.spit_time = -1;
                }
                continue;
            };
            let head = m.look_eye();
            let mouth = head + (target - head).with_y(0.0).normalize_or_zero() * 0.5;
            let m = &mut self.mobs[i];
            m.set_look_at(eye, super::mob::head::ATTACK_LOOK, super::mob::head::ATTACK_LOOK);
            // `attackTime`: 40 ticks from the first tick (`lerp(d / 20, 40,
            // 40)`), then the spit when it counts down to 0.
            let Some(s) = m.mount.as_mut() else { continue };
            s.spit_time -= 1;
            if s.spit_time < 0 {
                s.spit_time = 40;
            }
            if s.spit_time != 0 || m.attack_cooldown > 0.0 || (target - mouth).length_squared() > 16.0 * 16.0 {
                continue;
            }
            s.spit_time = 40;
            let spit = super::thrown::Thrown::spit(mouth, target, m.uid, &mut self.rng);
            self.sculk.shot(mouth, crate::world::vibration::EventSource::mob(m.uid));
            let m = &mut self.mobs[i];
            m.attack_cooldown = 2.0;
            m.attack_anim = 0.3;
            // Java: one spit per provocation, then the llama calms down.
            m.angry_player = None;
            self.thrown.push(spit);
            let mut b = crate::particles::Burst::new(crate::particles::Kind::Poof, mouth, 4);
            b.velocity_spread = DVec3::splat(0.01);
            b.color = Some([0.95, 0.95, 0.9, 1.0]);
            self.particles.push(crate::particles::Request::Burst(b));
        }
        // Caravans: an unled adult llama joins the tail of a led llama (or its caravan)
        // within 9 blocks, up to ten llamas long, and follows the one ahead.
        let ids: Vec<(u32, DVec3, bool, Option<u32>)> = self
            .mobs
            .iter()
            .filter(|m| m.simulation_active && m.kind == MobKind::Llama && m.alive())
            .map(|m| (m.uid, m.pos, m.leash.is_some(), m.mount.as_ref().and_then(|s| s.caravan)))
            .collect();
        let pos_of = |uid: u32| ids.iter().find(|l| l.0 == uid).map(|l| l.1);
        let depth = |mut uid: u32| {
            let mut n = 1;
            while let Some(next) = ids.iter().find(|l| l.0 == uid).and_then(|l| l.3) {
                n += 1;
                uid = next;
                if n > 10 {
                    break;
                }
            }
            n
        };
        let mut joins = Vec::new();
        for m in self.mobs.iter().filter(|m| m.simulation_active && m.kind == MobKind::Llama && m.alive()) {
            let Some(s) = m.mount.as_ref() else { continue };
            if m.leash.is_some() || m.baby || s.rider.is_some() || s.caravan.is_some() {
                continue;
            }
            let head = ids
                .iter()
                .filter(|l| {
                    l.0 != m.uid
                        && (l.2 || l.3.is_some())
                        && !ids.iter().any(|f| f.3 == Some(l.0))
                        && l.1.distance_squared(m.pos) < 81.0
                        && depth(l.0) < 10
                })
                .min_by(|a, b| a.1.distance_squared(m.pos).total_cmp(&b.1.distance_squared(m.pos)));
            if let Some(h) = head {
                joins.push((m.uid, h.0));
            }
        }
        for m in self.mobs.iter_mut().filter(|m| m.simulation_active && m.kind == MobKind::Llama) {
            let Some(s) = m.mount.as_mut() else { continue };
            if let Some((_, head)) = joins.iter().find(|j| j.0 == m.uid) {
                s.caravan = Some(*head);
            }
            // Leave when led, ridden, or the llama ahead is gone or more than 16 blocks away.
            let ahead = s.caravan.and_then(pos_of);
            if m.leash.is_some() || s.rider.is_some() || ahead.is_none_or(|p| p.distance_squared(m.pos) > 256.0) {
                s.caravan = None;
            }
            s.caravan_pos = s.caravan.and_then(pos_of);
        }
    }
}

impl Entities {
    /// Assign identities before scripted or interactive riding, including newly spawned mobs.
    pub fn ensure_mob_ids(&mut self) {
        for m in &mut self.mobs {
            if m.uid == 0 {
                self.next_uid = self.next_uid.max(1);
                m.uid = self.next_uid;
                self.next_uid = self.next_uid.wrapping_add(1).max(1);
            }
        }
    }
    /// Keep the physical player and camera on their seat, or clear a lost vehicle.
    pub fn seat_player(&self, player: &mut Player, id: PlayerId) {
        let Some(vehicle) = player.vehicle else { return };
        let seat = self.mount_seat(vehicle, id).or_else(|| self.cart(vehicle)?.seat_for(id));
        if let Some(pos) = seat {
            player.pos = pos;
            player.vel = DVec3::ZERO;
            player.on_ground = true;
        } else {
            if let Some(cart) = self.cart(vehicle) {
                player.pos = cart.pos + DVec3::X;
            }
            player.vehicle = None;
            player.vel = DVec3::ZERO;
        }
    }
    /// v1.0 Track 7: `AbstractBoat.positionRider` for a player riding a
    /// boat: the boat's turn this tick carries the rider's yaw, kept within
    /// 105° of the boat's heading.
    pub fn turn_boat_rider(&self, player: &mut Player, id: PlayerId) {
        let Some(cart) = player.vehicle.and_then(|v| self.cart(v)) else { return };
        if cart.kind.boat().is_some() && (cart.rider == Some(id) || cart.second_rider == Some(id)) {
            player.yaw = cart.turn_boat_rider(player.yaw);
        }
    }

    pub fn mount(&self, vehicle: u32) -> Option<&Mob> {
        (vehicle & MOB_VEHICLE != 0)
            .then(|| self.mobs.iter().find(|m| m.uid == vehicle & !MOB_VEHICLE && m.alive()))
            .flatten()
    }
    pub fn mount_mut(&mut self, vehicle: u32) -> Option<&mut Mob> {
        (vehicle & MOB_VEHICLE != 0)
            .then(|| self.mobs.iter_mut().find(|m| m.uid == vehicle & !MOB_VEHICLE && m.alive()))
            .flatten()
    }
    pub fn mount_seat(&self, vehicle: u32, player: PlayerId) -> Option<DVec3> {
        let m = self.mount(vehicle)?;
        let s = m.mount.as_ref()?;
        let seat = m.pos + DVec3::Y * (m.shape().height * 0.75 - 0.35);
        if m.kind == MobKind::HappyGhast {
            return super::happy_ghast::seat(m, s, player);
        }
        if super::nautilus::is_nautilus(m.kind) {
            return (s.rider == Some(player)).then(|| super::nautilus::seat(m));
        }
        if !camel_like(m.kind) {
            // v1.0 gap Track 3b: `AbstractHorse.getPassengerAttachmentPoint`
            // lifts the rider 0.15 and moves them 0.7 back as it rears
            // (`standAnimO`).
            let rear = f64::from(s.stand_anim_o) * if m.baby { 0.5 } else { 1.0 };
            let forward = DVec3::new(f64::from(m.yaw.cos()), 0.0, f64::from(m.yaw.sin()));
            let seat = seat + DVec3::Y * (0.15 * rear) - forward * (0.7 * rear);
            return (s.rider == Some(player)).then_some(seat);
        }
        // v1.0 gap Track 3: `Camel.getPassengerAttachmentPoint`, less the
        // player's 0.6 vehicle attachment (`Avatar.DEFAULT_VEHICLE_ATTACHMENT`).
        let count = usize::from(s.rider.is_some()) + usize::from(s.passenger.is_some());
        let index = if s.rider == Some(player) {
            0
        } else if s.passenger == Some(player) {
            usize::from(s.rider.is_some())
        } else {
            return None;
        };
        Some(camel::attachment(m, s, index, count, false) - DVec3::Y * PLAYER_VEHICLE_ATTACHMENT)
    }
    /// v1.0 Track 9: `Camera.alignWithEntity`'s third-person distance for a
    /// rider: the larger of the player's `camera_distance` (4) and a living
    /// mount's (the happy ghast's 8); carts, boats and cushions aren't living.
    pub fn camera_distance(&self, vehicle: Option<u32>) -> f64 {
        let mount = vehicle.and_then(|v| self.mount(v)).map_or(crate::camera::CAMERA_DISTANCE, |m| {
            if m.kind == MobKind::HappyGhast {
                crate::camera::HAPPY_GHAST_CAMERA_DISTANCE
            } else {
                crate::camera::CAMERA_DISTANCE
            }
        });
        mount.max(crate::camera::CAMERA_DISTANCE)
    }
    /// Where a vehicle is: a mount's or a cart, boat or cushion's position.
    pub fn vehicle_pos(&self, id: u32) -> Option<DVec3> {
        if id & MOB_VEHICLE != 0 { self.mount(id).map(|m| m.pos) } else { self.cart(id).map(|c| c.pos) }
    }
    pub fn mount_for_player(&self, player: PlayerId) -> Option<u32> {
        self.mobs
            .iter()
            .find(|m| {
                m.alive()
                    && m.mount.as_ref().is_some_and(|s| {
                        s.rider == Some(player) || s.passenger == Some(player) || s.seats.contains(&Some(player))
                    })
            })
            .map(|m| MOB_VEHICLE | m.uid)
    }
    pub fn mount_input(
        &mut self,
        id: PlayerId,
        player: &Player,
        input: MoveInput,
        inventory: &Inventory,
        selected: usize,
    ) {
        if let Some(vehicle) = player.vehicle
            && let Some(m) = self.mount_mut(vehicle)
            && let Some(s) = m.mount.as_mut()
            && s.rider == Some(id)
        {
            s.jump_cooldown = m.nautilus.as_ref().map_or(0, |n| n.dash_cooldown);
            s.vehicle_on_ground = m.on_ground;
            s.input(m.kind, player, input, inventory.get(selected).map(|s| s.item), None);
        }
    }
    /// Use saddle/armor/chest/food or mount an adult under the crosshair.
    /// Returns true when the interaction was handled; callers attach via mount_for_player.
    #[allow(clippy::too_many_arguments)]
    pub fn use_mount(
        &mut self,
        eye: DVec3,
        dir: DVec3,
        reach: f64,
        player: PlayerId,
        inventory: &mut Inventory,
        selected: usize,
        creative: bool,
    ) -> bool {
        let Some((i, _)) = self.raycast(eye, dir, reach) else { return false };
        self.ensure_mob_ids();
        let occupied = self.mobs.iter().any(|r| r.riding == Some(MOB_VEHICLE | self.mobs[i].uid) && r.alive());
        if self.mobs[i].kind == MobKind::HappyGhast {
            // `HappyGhast.mobInteract`: a harnessed adult takes up to four players.
            return !occupied && self.board_happy_ghast(i, player);
        }
        let m = &mut self.mobs[i];
        if occupied
            || m.kind == MobKind::SkeletonHorse && m.mount.as_ref().is_some_and(|s| !s.tame)
            || !rideable(m.kind)
            || m.riding.is_some()
        {
            return false;
        }
        let s = m.mount.as_mut().unwrap();
        let kind = m.kind;
        let held = inventory.get(selected).map(|s| s.item);
        let mut consume = false;
        if held == Some(Item::SADDLE) && s.tame && !s.saddled() && !m.baby && kind != MobKind::Llama {
            s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
            consume = true;
        } else if let Some(item) = held
            && (matches!(kind, MobKind::SkeletonHorse | MobKind::ZombieHorse | MobKind::Horse | MobKind::Llama)
                || super::nautilus::is_nautilus(kind))
            && !m.baby
            && s.accepts(kind, 1, item)
        {
            // Horse armor or llama carpet: equip, returning what was worn.
            // The worn stack keeps its components (a dyed colour, enchantments).
            let worn = inventory.get(selected).map_or_else(|| Stack::new(item, 1), |s| Stack { count: 1, ..s });
            let previous = s.slots[1].replace(worn);
            if !creative {
                inventory.take_one(selected);
            }
            m.persistent = true;
            let pos = m.pos;
            if let Some(stack) = previous {
                let count = inventory.add_stack(stack);
                if count > 0 {
                    self.scatter(Stack { count, ..stack }, pos);
                }
            }
            return true;
        } else if held == Some(Item::from_block(crate::world::block::Block::CHEST))
            && s.tame
            && !m.baby
            && !s.chest
            && chested(kind)
        {
            s.chest = true;
            consume = true;
        } else if let Some((heal, temper, growth)) = held.and_then(|i| food(kind, i))
            && equine(kind)
        {
            if m.baby && !super::undead_mounts::undead(kind) {
                m.age = (m.age + growth).min(0);
                m.baby = m.age < 0;
                consume = true;
            }
            let max = max_temper(kind);
            if m.health < s.max_health || !s.tame && s.temper < max {
                m.health = (m.health + heal).min(s.max_health);
                s.temper = s.temper.saturating_add(temper).min(max);
                consume = true;
            } else if !consume {
                return false;
            }
            // v1.0 gap Track 2c: `AbstractHorse.eating` opens the mouth.
            if rears(kind) {
                s.open_mouth();
            }
            // v1.0 Track 9: `AbstractHorse.handleEating` posts `eat` once the
            // food did something.
            let source = super::warden::mob_source(m);
            self.sculk.post(m.pos, crate::world::vibration::GameEvent::Eat, source, None);
        } else if !m.baby
            && s.rider.is_none()
            && (equine(kind) && (s.tame || held.is_none())
                || super::nautilus::is_nautilus(kind) && s.tame && !held.is_some_and(super::nautilus::food)
                || !equine(kind) && !super::nautilus::is_nautilus(kind) && s.saddled())
        {
            s.rider = Some(player);
        } else if !m.baby
            && camel_like(kind)
            && s.rider.is_some_and(|r| r != player)
            && s.passenger.is_none()
            && held.is_none_or(|i| food(kind, i).is_none())
        {
            // A camel's rear seat.
            s.passenger = Some(player);
        } else if !m.baby
            && !s.tame
            && held.is_some()
            && matches!(kind, MobKind::Horse | MobKind::Donkey | MobKind::Mule | MobKind::Llama | MobKind::ZombieHorse)
        {
            // v1.0 gap Track 3b: `Horse`, `ZombieHorse` and
            // `AbstractChestedHorse.mobInteract`: any item that isn't food
            // makes an untamed animal mad (it rears, if it can, and plays
            // its angry sound) instead of letting the player on.
            s.make_mad(kind);
            return true;
        } else {
            return false;
        }
        if consume && !creative {
            inventory.take_one(selected);
        }
        m.persistent = true;
        true
    }
    pub fn boost_mount(
        &mut self,
        player: PlayerId,
        inventory: &mut Inventory,
        selected: usize,
        creative: bool,
    ) -> bool {
        let Some(vehicle) = self.mount_for_player(player) else { return false };
        let m = self.mount_mut(vehicle).unwrap();
        let (item, wear) = match m.kind {
            MobKind::Pig => (Item::CARROT_ON_A_STICK, 7),
            MobKind::Strider => (Item::WARPED_FUNGUS_ON_A_STICK, 1),
            _ => return false,
        };
        let Some(stack) = inventory.get(selected) else { return false };
        if stack.item != item || stack.damage >= item.durability().unwrap() {
            return false;
        }
        let s = m.mount.as_mut().unwrap();
        if !s.saddled() || s.boost_total > 0 {
            return false;
        }
        // Java nextInt(841) + 140: 7–49 seconds, smooth sine speed boost.
        let total = self.rng.next_int(841) as u16 + 140;
        let s = self.mount_mut(vehicle).unwrap().mount.as_mut().unwrap();
        s.boost_total = total;
        s.boost_ticks = 0;
        if !creative && inventory.wear(selected, wear) {
            inventory.slots[selected] = Some(Stack { enchants: stack.enchants, ..Stack::new(Item::FISHING_ROD, 1) });
        }
        true
    }
    pub fn dismount_animal<W: BlockSource + ?Sized>(&mut self, world: &W, player: PlayerId) -> Option<DVec3> {
        let id = self.mount_for_player(player)?;
        let m = self.mount_mut(id)?;
        let s = m.mount.as_mut()?;
        if m.kind == MobKind::HappyGhast {
            return Some(super::happy_ghast::dismount(m, player));
        }
        if s.rider == Some(player) {
            // The rear passenger slides forward, as in Java.
            s.rider = s.passenger.take();
        } else {
            s.passenger = None;
        }
        let side = DVec3::new(-(m.yaw.sin() as f64), 0., m.yaw.cos() as f64) * (m.shape().half_width + 0.4);
        for offset in [side, -side, DVec3::X * 1.5, DVec3::NEG_X * 1.5, DVec3::Z * 1.5, DVec3::NEG_Z * 1.5] {
            for dy in [0., 1., -1.] {
                let mut pos = m.pos + offset + DVec3::Y * dy;
                if !physics::overlaps_solid(world, pos, crate::player::SHAPE)
                    && !physics::touches_block(world, pos, crate::player::SHAPE, |b| b.is_lava())
                {
                    let mut velocity = DVec3::NEG_Y;
                    let floor =
                        physics::move_box(world, &mut pos, &mut velocity, DVec3::NEG_Y * 2., crate::player::SHAPE);
                    if floor.on_ground && !physics::touches_block(world, pos, crate::player::SHAPE, |b| b.is_lava()) {
                        return Some(pos);
                    }
                }
            }
        }
        Some(m.pos + DVec3::Y * m.shape().height)
    }
}

impl Entities {
    /// Shared container access: cart ids stay unchanged, equines use tagged mob ids.
    pub fn vehicle_slots(&self, id: u32) -> Option<&[Option<Stack>]> {
        if id & MOB_VEHICLE == 0 {
            let c = self.cart(id)?;
            return Some(&c.slots[..c.slot_count()]);
        }
        let m = self.mount(id)?;
        let s = m.mount.as_ref()?;
        has_mount_screen(m).then(|| &s.slots[..s.slot_count(m.kind)])
    }
    pub fn vehicle_slots_mut(&mut self, id: u32) -> Option<&mut [Option<Stack>]> {
        if id & MOB_VEHICLE == 0 {
            let c = self.cart_mut(id)?;
            let n = c.slot_count();
            return Some(&mut c.slots[..n]);
        }
        let m = self.mount_mut(id)?;
        let open = has_mount_screen(m);
        let s = m.mount.as_mut()?;
        let n = s.slot_count(m.kind);
        open.then(|| &mut s.slots[..n])
    }
    pub fn vehicle_accepts(&self, id: u32, slot: usize, item: Item) -> bool {
        if id & MOB_VEHICLE == 0 {
            return self.vehicle_slots(id).is_some_and(|s| slot < s.len());
        }
        self.mount(id).is_some_and(|m| {
            m.mount.as_ref().is_some_and(|s| slot < s.slot_count(m.kind) && s.accepts(m.kind, slot, item))
        })
    }
    pub fn insert_vehicle(&mut self, id: u32, mut stack: Stack) -> Option<Stack> {
        let n = self.vehicle_slots(id)?.len();
        for i in 0..n {
            if !self.vehicle_accepts(id, i, stack.item) {
                continue;
            }
            let slots = self.vehicle_slots_mut(id)?;
            let cell = &mut slots[i];
            let room = match cell {
                None => stack.max(),
                Some(s) if s.stacks_with(&stack) => s.max() - s.count,
                _ => 0,
            };
            let count = room.min(stack.count);
            if count > 0 {
                *cell = Some(Stack { count: cell.map_or(0, |s| s.count) + count, ..stack });
                stack.count -= count;
            }
            if stack.count == 0 {
                return None;
            }
        }
        Some(stack)
    }
    /// v1.0 gap Track 3b: the layout of Java's mount inventory screen for
    /// mob vehicle `id` (`HorseInventoryMenu`, `NautilusInventoryMenu`), or
    /// `None` for carts, boats and mobs without one.
    pub fn mount_screen(&self, id: u32) -> Option<MountScreen> {
        let m = self.mount(id)?;
        let s = m.mount.as_ref()?;
        if !equine(m.kind) && !super::nautilus::is_nautilus(m.kind) {
            return None;
        }
        // `inventoryColumns`: a donkey's or mule's chest is 5 wide, a
        // llama's 3 slots a strength point.
        let columns = match m.kind {
            MobKind::Llama if s.chest => usize::from(s.strength),
            _ if s.chest => 5,
            _ => 0,
        };
        Some(MountScreen {
            // `#can_equip_saddle` (not llamas) and `canUseSlot(SADDLE)`.
            saddle: m.kind != MobKind::Llama,
            // `#can_wear_horse_armor` or a llama, or a nautilus's body slot.
            armor: match m.kind {
                MobKind::Horse | MobKind::ZombieHorse => Some(MountArmor::Horse),
                MobKind::Llama => Some(MountArmor::Llama),
                k if super::nautilus::is_nautilus(k) => Some(MountArmor::Nautilus),
                _ => None,
            },
            columns,
        })
    }
    /// v1.0 gap Track 3b: the mount screen's title, Java's
    /// `getDisplayName`: the custom name, else the kind's.
    pub fn mount_name(&self, id: u32) -> Option<String> {
        let m = self.mount(id)?;
        Some(m.name.as_str().map_or_else(
            || {
                let name = m.kind.name();
                let mut out = String::with_capacity(name.len());
                let mut up = true;
                for ch in name.chars().map(|c| if c == '_' { ' ' } else { c }) {
                    out.extend(if up { ch.to_uppercase().collect::<Vec<_>>() } else { vec![ch] });
                    up = ch == ' ';
                }
                out
            },
            str::to_string,
        ))
    }
    pub fn vehicle_title(&self, id: u32) -> &'static str {
        if let Some(m) = self.mount(id) {
            return match m.kind {
                MobKind::Horse => "Horse (saddle, armor)",
                MobKind::Donkey => "Donkey (saddle, storage)",
                MobKind::Mule => "Mule (saddle, storage)",
                // v1.0 gap Track 3: `NautilusInventoryMenu`'s saddle and
                // body armor slots.
                MobKind::Nautilus => "Nautilus (saddle, armor)",
                MobKind::ZombieNautilus => "Zombie Nautilus (saddle, armor)",
                _ => "Mount",
            };
        }
        if let Some(c) = self.cart(id) {
            return if c.kind.boat().is_some() {
                "Boat with Chest"
            } else if c.slot_count() == 5 {
                "Minecart with Hopper"
            } else {
                "Minecart with Chest"
            };
        }
        "Storage"
    }
    pub fn vehicle_container(&self, eye: DVec3, dir: DVec3, reach: f64) -> Option<u32> {
        let cart = self.cart_container(eye, dir, reach).map(|(id, _)| id);
        let animal = self.raycast(eye, dir, reach).and_then(|(i, _)| {
            let m = &self.mobs[i];
            // `AbstractHorse.mobInteract` and `AbstractNautilus.openCustomInventoryScreen`:
            // not while someone rides it. `Camel.mobInteract` opens it anyway.
            let ridden = m.mount.as_ref().is_some_and(|s| s.rider.is_some() || s.passenger.is_some());
            (has_mount_screen(m) && (!ridden || camel_like(m.kind))).then_some(MOB_VEHICLE | m.uid)
        });
        match (cart, animal) {
            (Some(c), Some(m)) => {
                if self.cart(c)?.pos.distance_squared(eye) < self.mount(m)?.pos.distance_squared(eye) {
                    Some(c)
                } else {
                    Some(m)
                }
            }
            (c, m) => c.or(m),
        }
    }
    /// Java `ContainerEntity.isChestVehicleStillValid`/horse inventories: the
    /// vehicle's box is within `range` of the eye.
    pub fn vehicle_in_reach(&self, id: u32, eye: DVec3, range: f64) -> bool {
        let aabb = self.cart(id).map(|c| c.aabb()).or_else(|| self.mount(id).map(|m| m.aabb()));
        self.vehicle_slots(id).is_some()
            && aabb.is_some_and(|(min, max)| crate::physics::box_distance_squared(eye, min, max) < range * range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;
    use crate::world::{World, chunk::ChunkData, terrain::Generator};
    use glam::IVec3;
    use std::sync::Arc;

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(1)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        for x in 0..16 {
            for z in 0..16 {
                w.set_block(IVec3::new(x, 139, z), Block::STONE);
            }
        }
        w
    }
    /// v1.0 gap Track 2d: `RangedAttackGoal(1.25, 40, 20)`: an angry llama
    /// looks at the player for its 40-tick `attackTime` and spits then, not
    /// at once.
    #[test]
    fn an_angry_llama_aims_for_forty_ticks_before_it_spits() {
        let mut e = Entities::new(5);
        e.mobs.push(Mob::new(MobKind::Llama, DVec3::new(0.5, 64.0, 0.5), 0.0));
        e.ensure_mob_ids();
        e.mobs[0].angry_player = Some(PlayerId::HOST);
        let ctx = crate::entity::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![crate::entity::Target::new(PlayerId::HOST, DVec3::new(6.5, 64.0, 0.5), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        let mut ticks = 0;
        while e.thrown.is_empty() {
            e.tick_llamas(&ctx);
            assert!(e.mobs[0].look.looking(), "it looks at the player every tick");
            ticks += 1;
            assert!(ticks < 100);
        }
        assert_eq!(ticks, 41, "attackTime: 40 after the first tick");
    }

    #[test]
    fn natural_packs_share_variants_and_follow_java_pack_baby_rules() {
        use crate::entity::monster_spawns::PackGroup;
        use crate::world::terrain::Biome;
        let mut e = Entities::new(94);
        let mut pack = PackGroup::default();
        for _ in 0..25 {
            e.spawn_natural(MobKind::Horse, DVec3::ZERO, Biome::Plains, &mut pack);
        }
        let coat = pack.variant.unwrap();
        let first = e.mobs[0].mount.as_ref().unwrap().marking;
        assert!(e.mobs.iter().any(|m| m.mount.as_ref().unwrap().marking != first));
        assert!(!e.mobs[0].baby, "a pack's first member is an adult");
        for m in &e.mobs {
            let state = m.mount.as_ref().unwrap();
            assert_eq!(state.variant, coat);
            let reloaded = State::load(MobKind::Horse, &state.save()).unwrap();
            assert_eq!((reloaded.variant, reloaded.marking), (coat, state.marking));
        }
        let mut donkeys = PackGroup::default();
        e.spawn_natural(MobKind::Donkey, DVec3::ZERO, Biome::Plains, &mut donkeys);
        assert_eq!(donkeys.variant, None);
        assert_eq!(e.mobs.last().unwrap().mount.as_ref().unwrap().variant, 0);
        // RabbitGroupData: an adult, then babies. FoxGroupData: two adults, then babies.
        for (kind, babies) in [(MobKind::Llama, None), (MobKind::Rabbit, Some(1)), (MobKind::Fox, Some(2))] {
            e.mobs.clear();
            let mut pack = PackGroup::default();
            for _ in 0..12 {
                e.spawn_natural(kind, DVec3::ZERO, Biome::Plains, &mut pack);
            }
            assert!(pack.variant.is_some(), "{kind:?}");
            assert!(e.mobs.iter().all(|m| {
                let v = m.mount.as_ref().map(|s| s.variant).or(m.animal.as_ref().map(|a| a.variant));
                v == pack.variant
            }));
            if let Some(adults) = babies {
                let pattern: Vec<_> = e.mobs.iter().map(|m| m.baby).collect();
                assert!(pattern.iter().enumerate().all(|(i, &b)| b == (i >= adults)), "{kind:?}: {pattern:?}");
            }
        }
    }

    #[test]
    fn java_horse_distributions_and_fixed_pack_animal_attributes() {
        let mut rng = Rng::new(17);
        let mut totals = [0.; 3];
        for _ in 0..10000 {
            let s = State::new(MobKind::Horse, &mut rng);
            assert!((15. ..=30.).contains(&s.max_health));
            assert!((0.1125..0.3375).contains(&s.speed));
            assert!((0.4..1.).contains(&s.jump));
            assert!(s.variant < 7 && s.marking < 5);
            totals[0] += s.max_health as f64;
            totals[1] += s.speed;
            totals[2] += s.jump;
        }
        assert!((totals[0] / 10000. - 22.5).abs() < 0.15);
        assert!((totals[1] / 10000. - 0.225).abs() < 0.002);
        assert!((totals[2] / 10000. - 0.7).abs() < 0.005);
        for kind in [MobKind::Donkey, MobKind::Mule] {
            let s = State::new(kind, &mut rng);
            assert_eq!((s.speed, s.jump), (0.175, 0.5));
        }
        assert!(!MobKind::Mule.is_breedable());
    }
    #[test]
    fn bucking_raises_temper_and_a_full_temper_mount_tames_to_its_rider() {
        let mut m = Mob::new(MobKind::Horse, DVec3::ZERO, 0.);
        let mut rng = Rng::new(5);
        m.mount.as_mut().unwrap().rider = Some(PlayerId(7));
        control(&mut m, 2.5, &mut rng);
        let s = m.mount.as_ref().unwrap();
        assert_eq!((s.temper, s.rider, s.tame), (5, None, false));
        let s = m.mount.as_mut().unwrap();
        s.temper = 100;
        s.rider = Some(PlayerId(7));
        control(&mut m, 2.5, &mut rng);
        let s = m.mount.as_ref().unwrap();
        assert!(s.tame);
        assert_eq!(s.owner, Some(PlayerId(7)));
    }
    #[test]
    fn pig_needs_a_saddle_and_matching_stick_and_boost_uses_durability_once() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Pig, DVec3::new(5., 140., 5.));
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        let eye = DVec3::new(5., 140.5, 2.);
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert!(inv.slots[0].is_none());
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        let id = e.mount_for_player(PlayerId::HOST).unwrap();
        let mut p = Player::new(eye);
        p.vehicle = Some(id);
        e.mount_input(PlayerId::HOST, &p, MoveInput::default(), &inv, 0);
        assert!(!e.mount(id).unwrap().mount.as_ref().unwrap().controlled());
        inv.slots[0] = Some(Stack::new(Item::CARROT_ON_A_STICK, 1));
        e.mount_input(PlayerId::HOST, &p, MoveInput::default(), &inv, 0);
        assert!(e.mount(id).unwrap().mount.as_ref().unwrap().controlled());
        assert!(e.boost_mount(PlayerId::HOST, &mut inv, 0, false));
        assert_eq!(inv.get(0).unwrap().damage, 7);
        assert!(!e.boost_mount(PlayerId::HOST, &mut inv, 0, false));
        assert_eq!(inv.get(0).unwrap().damage, 7);
        assert!((140..=980).contains(&e.mount(id).unwrap().mount.as_ref().unwrap().boost_total));
    }
    #[test]
    fn equine_storage_validates_gear_and_saves_owner_stats_chest_and_rider() {
        let mut e = Entities::new(3);
        e.spawn(MobKind::Donkey, DVec3::new(5., 140., 5.));
        let m = &mut e.mobs[0];
        m.uid = 7;
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.owner = Some(PlayerId(3));
        s.rider = Some(PlayerId(4));
        s.chest = true;
        let id = MOB_VEHICLE | 7;
        assert_eq!(e.vehicle_slots(id).unwrap().len(), 17);
        assert!(!e.vehicle_accepts(id, 1, Item::DIAMOND_HORSE_ARMOR));
        assert!(e.insert_vehicle(id, Stack::new(Item::SADDLE, 1)).is_none());
        assert!(e.insert_vehicle(id, Stack::new(Item::DIAMOND, 64)).is_none());
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(3);
        restored.load_nether_mobs(&saved);
        assert_eq!(restored.nether_mobs_to_string(), saved);
        assert_eq!(restored.mount_for_player(PlayerId(4)), Some(id));
        let pos = restored.dismount_animal(&world(), PlayerId(4)).unwrap();
        assert!(!physics::overlaps_solid(&world(), pos, crate::player::SHAPE));
    }
    #[test]
    fn charged_jump_uses_mount_attribute_and_armor_reduces_damage_without_wear() {
        assert_eq!(jump_charge(10), 1.);
        assert_eq!(jump_charge(0), 0.);
        assert!(jump_charge(100) > 0.8 && jump_charge(100) < 0.81);
        let mut m = Mob::new(MobKind::Horse, DVec3::ZERO, 0.);
        m.on_ground = true;
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.rider = Some(PlayerId::HOST);
        s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        s.slots[1] = Some(Stack::new(Item::DIAMOND_HORSE_ARMOR, 1));
        s.jump = 0.7;
        let p = Player::new(DVec3::ZERO);
        for _ in 0..10 {
            m.mount.as_mut().unwrap().input(
                MobKind::Horse,
                &p,
                MoveInput { jump: true, ..Default::default() },
                None,
                None,
            );
        }
        m.mount.as_mut().unwrap().input(MobKind::Horse, &p, MoveInput::default(), None, None);
        // v1.0 Track 7: the released jump waits for the Java ridden tick.
        // `LocalPlayer`: the press tick starts at 0, nine held ticks reach
        // 0.9, a full jump.
        control(&mut m, 0.05, &mut Rng::new(1));
        let charge = m.mount.as_mut().unwrap().take_pending_jump().unwrap();
        assert_eq!(charge, 9.0 * 0.1f32);
        assert_eq!(crate::entity::mob::equine::jump_scale(charge), 1.0);
        let before = m.health;
        m.damage(5., None, &mut Rng::new(1));
        assert!(m.health > before - 5.);
        assert_eq!(m.mount.as_ref().unwrap().slots[1].unwrap().damage, 0);
    }
    #[test]
    fn feeding_heals_raises_temper_and_grows_foals_without_mounting_them() {
        let mut e = Entities::new(5);
        e.spawn(MobKind::Horse, DVec3::new(5., 140., 5.));
        e.mobs[0].baby = true;
        e.mobs[0].age = -24000;
        e.mobs[0].health = 5.;
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::GOLDEN_CARROT, 2));
        assert!(e.use_mount(DVec3::new(5., 140.5, 3.), DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert_eq!(e.mobs[0].health, 9.);
        assert_eq!(e.mobs[0].age, -22800);
        assert_eq!(e.mobs[0].mount.as_ref().unwrap().temper, 5);
        assert_eq!(e.mount_for_player(PlayerId::HOST), None);
        assert_eq!(inv.get(0).unwrap().count, 1);
    }
    #[test]
    fn old_persistent_pig_save_loads_and_boat_passenger_identity_survives() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Pig, DVec3::new(5., 140., 5.));
        e.mobs[0].persistent = true;
        e.mobs[0].riding = Some(13);
        let text = e.nether_mobs_to_string();
        let mut old: Value = serde_json::from_str(&text).unwrap();
        old["mobs"][0].as_object_mut().unwrap().remove("mount");
        old["mobs"][0].as_object_mut().unwrap().remove("uid");
        let mut loaded = Entities::new(1);
        loaded.load_nether_mobs(&old.to_string());
        assert_eq!(loaded.mobs.len(), 1);
        assert_eq!(loaded.mobs[0].riding, Some(13));
        assert!(loaded.mobs[0].mount.is_some());
        assert!(!loaded.mobs[0].mount.as_ref().unwrap().saddled());
    }
    #[test]
    fn strider_shivering_changes_riding_speed_and_pig_uses_java_speed() {
        for (kind, cold, speed) in
            [(MobKind::Pig, false, 2.42), (MobKind::Strider, false, 4.14), (MobKind::Strider, true, 1.74)]
        {
            let mut m = Mob::new(kind, DVec3::ZERO, 0.);
            if let Some(n) = &mut m.nether {
                n.cold = cold;
            }
            let s = m.mount.as_mut().unwrap();
            s.rider = Some(PlayerId::HOST);
            s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
            let item = if kind == MobKind::Pig { Item::CARROT_ON_A_STICK } else { Item::WARPED_FUNGUS_ON_A_STICK };
            s.input(kind, &Player::new(DVec3::ZERO), MoveInput::default(), Some(item), None);
            assert_eq!(control(&mut m, 0.05, &mut Rng::new(1)).unwrap().1, speed);
        }
    }
    #[test]
    fn equine_steps_a_full_block_and_rounds_half_fall_damage() {
        let mut w = world();
        for z in 0..16 {
            w.set_block(IVec3::new(7, 140, z), Block::STONE);
        }
        let mut m = Mob::new(MobKind::Horse, DVec3::new(5., 140., 5.), 0.);
        m.on_ground = true;
        let mut top = m.pos.y;
        for _ in 0..30 {
            m.physics_step(0.05, &w, Some(DVec3::X), 5.);
            top = top.max(m.pos.y);
        }
        // Climbs onto the one-block wall at x = 7 without jumping, then walks off it.
        assert!(m.pos.x > 8. && top >= 141., "{:?} peak {top}", m.pos);
        // `calculateFallDamage`: floor((fall - 6) * 0.5), so seven blocks
        // don't hurt (Java 26.3 floors; this used to round up) and ten do 2.
        for (from, hurt) in [(147., 0.), (151., 2.)] {
            m.pos = DVec3::new(4., from, 5.);
            m.vel = DVec3::ZERO;
            m.on_ground = false;
            let before = m.health;
            for _ in 0..240 {
                m.physics_step(1. / 120., &w, None, 0.);
                if m.on_ground {
                    break;
                }
            }
            assert!(m.on_ground);
            // v1.0 gap Track 8d: the fall hurts through `hurtServer`, which
            // the tick's `take_fall` applies after the moves.
            m.damage_cooldown = 0;
            m.take_fall(&mut crate::entity::Rng::new(1));
            assert_eq!(before - m.health, hurt, "from {from}");
        }
    }
    #[test]
    fn horse_jump_height_matches_java_attribute_range() {
        let w = world();
        for (jump, expected) in [(0.4, 1.1093), (0.7, 2.8933), (1., 5.29997)] {
            let mut m = Mob::new(MobKind::Horse, DVec3::new(5., 140., 5.), 0.);
            m.vel.y = launch_speed(jump);
            let mut peak = m.pos.y;
            for _ in 0..240 {
                m.physics_step(1. / 120., &w, None, 0.);
                peak = peak.max(m.pos.y);
                if m.on_ground {
                    break;
                }
            }
            assert!((peak - 140. - expected).abs() < 0.2, "jump {jump}: {} vs {expected}", peak - 140.);
        }
    }
    #[test]
    fn offspring_attributes_stay_in_range_and_centre_on_the_parents() {
        let mut rng = Rng::new(3);
        let mut sum = 0.;
        for _ in 0..20000 {
            let v = offspring_attribute(0.2, 0.3, 0.1125, 0.3375, &mut rng);
            assert!((0.1125..=0.3375).contains(&v));
            sum += v;
        }
        assert!((sum / 20000. - 0.25).abs() < 0.005);
        // Reflection keeps a draw past the top inside the range.
        for _ in 0..1000 {
            assert!(offspring_attribute(1., 1., 0.4, 1., &mut rng) <= 1.);
        }
    }
    #[test]
    fn tamed_horse_and_donkey_breed_a_mule_but_untamed_horses_do_not_love() {
        let mut e = Entities::new(4);
        // Inside the loaded chunk: the breeding goal runs through full updates.
        e.spawn(MobKind::Horse, DVec3::new(6., 140., 6.));
        e.spawn(MobKind::Donkey, DVec3::new(8., 140., 6.));
        assert_eq!(e.use_animal(0, Some(Stack::new(Item::GOLDEN_CARROT, 1)), PlayerId::HOST), None);
        for i in 0..2 {
            e.mobs[i].uid = i as u32 + 1;
            e.mobs[i].mount.as_mut().unwrap().tame = true;
            assert_eq!(e.use_animal(i, Some(Stack::new(Item::GOLDEN_CARROT, 1)), PlayerId::HOST), Some(true));
        }
        let w = world();
        let ctx = super::super::Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![super::super::Target::new(PlayerId::HOST, DVec3::new(0., 140., 0.), false)],
            daylight: 1.,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        };
        for _ in 0..80 {
            e.update(0.05, &w, &ctx);
        }
        assert_eq!(e.mobs.len(), 3);
        let foal = &e.mobs[2];
        assert_eq!(foal.kind, MobKind::Mule);
        assert!(foal.baby);
        let s = foal.mount.as_ref().unwrap();
        assert!((15. ..=30.).contains(&s.max_health) && (0.4..=1.).contains(&s.jump));
        assert_eq!(foal.health, s.max_health);
    }
    #[test]
    fn llamas_have_java_strength_coats_and_chest_slots() {
        let mut rng = Rng::new(11);
        let mut seen = [0u32; 6];
        for _ in 0..4000 {
            let s = State::new(MobKind::Llama, &mut rng);
            seen[s.strength as usize] += 1;
            assert!(s.variant < 4 && !s.tame);
        }
        assert_eq!(seen[0], 0);
        // Strength 4 and 5 only come from the 1-in-25 roll.
        assert!(seen[4] + seen[5] > 0 && seen[4] + seen[5] < 200);
        let mut s = State::new(MobKind::Llama, &mut rng);
        s.tame = true;
        s.chest = true;
        s.strength = 2;
        assert_eq!(s.slot_count(MobKind::Llama), 8);
        assert!(!s.accepts(MobKind::Llama, 0, Item::SADDLE));
        let carpet = Item::from_block(crate::world::block::Block::carpet(crate::color::DyeColor::Blue));
        assert!(s.accepts(MobKind::Llama, 1, carpet));
        assert!(s.accepts(MobKind::Llama, 7, Item::WHEAT) && !s.accepts(MobKind::Llama, 8, Item::WHEAT));
        let saved = State::load(MobKind::Llama, &s.save()).unwrap();
        assert_eq!((saved.strength, saved.chest), (2, true));
    }
    #[test]
    fn camel_poses_save_and_old_saves_load_seated_or_mid_stand() {
        let mut e = Entities::new(43);
        e.spawn(MobKind::Camel, DVec3::new(5., 140., 5.));
        let s = e.mobs[0].mount.as_mut().unwrap();
        // `finalizeSpawn`: standing, fully (`resetLastPoseChangeTickToFullStand`).
        assert!(!s.camel_sitting && s.pose_time == camel::FULL_STAND && !s.refuses_to_move());
        assert_eq!(s.jump, camel::JUMP_STRENGTH);
        s.sit_down();
        assert!(s.camel_sitting && s.in_pose_transition() && s.refuses_to_move());
        s.pose_time = 17;
        let loaded = State::load(MobKind::Camel, &s.save()).unwrap();
        assert_eq!((loaded.camel_sitting, loaded.pose_time), (true, 17));
        // Before v1.0 gap Track 3: a countdown sitting, or the stand-up left.
        let mut old = s.save();
        let o = old.as_object_mut().unwrap();
        o.remove("camel_sitting");
        o.remove("pose_time");
        o.insert("sit_ticks".into(), json!(300));
        o.insert("jump".into(), json!(0.5));
        let seated = State::load(MobKind::Camel, &old).unwrap();
        assert!(seated.camel_sitting && seated.pose_time == camel::RANDOM_SIT_TICKS && !seated.in_pose_transition());
        assert_eq!(seated.jump, camel::JUMP_STRENGTH, "the fixed attribute, not the old 0.5");
        let o = old.as_object_mut().unwrap();
        o.insert("sit_ticks".into(), json!(0));
        o.insert("stand_ticks".into(), json!(12));
        let rising = State::load(MobKind::Camel, &old).unwrap();
        assert!(!rising.camel_sitting && rising.pose_time == 40 && rising.in_pose_transition());
        old.as_object_mut().unwrap().remove("stand_ticks");
        let standing = State::load(MobKind::Camel, &old).unwrap();
        assert!(!standing.camel_sitting && !standing.refuses_to_move());
        // `standUpInstantly` (a hit, water) skips the transition and resets
        // the clock to a full stand.
        let s = e.mobs[0].mount.as_mut().unwrap();
        s.pose_time = 5000;
        s.stand_up_instantly();
        assert!(!s.refuses_to_move() && s.pose_time == camel::FULL_STAND);
        s.sit_down();
        e.mobs[0].damage(1.0, None, &mut Rng::new(1));
        assert!(!e.mobs[0].mount.as_ref().unwrap().camel_sitting, "`actuallyHurt` stands it up");
    }

    #[test]
    fn camel_dimensions_and_seats_follow_the_pose() {
        let mut e = Entities::new(3);
        e.spawn(MobKind::Camel, DVec3::new(5., 140., 5.));
        e.mobs[0].on_ground = true;
        e.mobs[0].yaw = 0.0;
        assert_eq!(e.mobs[0].shape().height, 2.375_f32 as f64);
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::SADDLE, 1));
        let eye = DVec3::new(5., 141.5, 2.);
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
        assert!(e.mobs[0].mount.as_ref().unwrap().saddled());
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId::HOST, &mut inv, 1, false));
        let id = e.mount_for_player(PlayerId::HOST).unwrap();
        // One rider sits half a block forward at the standing anchor 2.0
        // (2.375 - 0.375), feet 0.6 below it.
        let lone = e.mount_seat(id, PlayerId::HOST).unwrap();
        assert!((lone - DVec3::new(5.5, 140. + 2.0 - 0.6, 5.)).length() < 1e-6, "{lone}");
        assert!(e.use_mount(eye, DVec3::Z, 5., PlayerId(7), &mut inv, 1, false));
        let (front, back) = (e.mount_seat(id, PlayerId::HOST).unwrap(), e.mount_seat(id, PlayerId(7)).unwrap());
        assert!((front.x - 5.5).abs() < 1e-6 && (back.x - 4.3).abs() < 1e-6, "{front} {back}");
        // Seated: the 0.945-tall box, both riders at 0.945 - 0.375 + 0.2.
        let s = e.mobs[0].mount.as_mut().unwrap();
        s.sit_down();
        s.pose_time = camel::SITDOWN_TICKS;
        assert!((e.mobs[0].shape().height - (2.375_f32 - 1.43_f32) as f64).abs() < 1e-6);
        let front = e.mount_seat(id, PlayerId::HOST).unwrap();
        let anchor = (2.375_f32 - 1.43_f32) as f64 - 0.375 + 0.2_f32 as f64;
        assert!((front.y - (140. + anchor - 0.6)).abs() < 1e-5, "{}", front.y - 140.);
        // Halfway down, the front seat is between the two.
        e.mobs[0].mount.as_mut().unwrap().pose_time = 20;
        let mid = e.mount_seat(id, PlayerId::HOST).unwrap().y - 140. + 0.6;
        assert!(mid > anchor && mid < 2.0, "{mid}");
        // The rear rider moves to the front when the driver gets off.
        assert!(e.dismount_animal(&world(), PlayerId::HOST).is_some());
        assert_eq!(e.mobs[0].mount.as_ref().unwrap().rider, Some(PlayerId(7)));
        // Babies: 0.95 × 1.4 standing, 0.95 × 0.425 seated.
        let mut baby = Mob::new(MobKind::Camel, DVec3::ZERO, 0.0);
        baby.baby = true;
        assert_eq!((baby.shape().half_width * 2.0, baby.shape().height), (0.95_f32 as f64, 1.4_f32 as f64));
        baby.mount.as_mut().unwrap().sit_down();
        assert_eq!(baby.shape().height, 0.425_f32 as f64);
    }
}
