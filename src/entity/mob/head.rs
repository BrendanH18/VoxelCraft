//! v1.0 gap Track 2c: Java 26.3's head and body for every mob.
//!
//! - **`LookControl`** (and `SmoothSwimmingLookControl`, the fox's, frog's,
//!   bee's, axolotl's, camel's and creaking's): every tick, after the move
//!   control, the pitch restarts level (`resetXRotOnTick`) and turns toward
//!   the point a behaviour asked for by at most its pitch limit, and the
//!   head (`yHeadRot`) by at most its yaw speed, for two ticks after each
//!   request (`lookAtCooldown`); without one the head turns back toward the
//!   body by 10° a tick. While the mob walks a path its head stays within
//!   `getMaxHeadYRot` of the body. Each kind's `getHeadRotSpeed`,
//!   `getMaxHeadXRot` and `getMaxHeadYRot` are Java's.
//! - **`BodyRotationControl`**: after the mob moved, a moving body faces its
//!   travel (`yBodyRot = yRot`) with the head within `getMaxHeadYRot`; a
//!   still one turns to keep within `getMaxHeadYRot` of a head that moved
//!   more than 15° since it settled, and once the head has been still for
//!   10 ticks the body turns to face it over the next 10. A mob carrying a
//!   mob passenger doesn't turn while still.
//! - **Idle look goals** (`LookAtPlayerGoal`, `InteractGoal`,
//!   `LookAtTradingPlayerGoal`, `RandomLookAroundGoal`) in each kind's
//!   `registerGoals` priorities and flags, started on the selector's full
//!   update (every other tick) and blocked while a better goal holds `LOOK`
//!   (or `MOVE` for the random look); a mob steered by a mob passenger loses
//!   both flags (`Mob.updateControlFlags`).
//! - **Brain looks**: `LookAtTargetSink(45, 90)` turns the head toward
//!   `LOOK_TARGET` while it stays visible, for 45 to 90 ticks, then erases
//!   it; each kind's idle activity sets it with `SetEntityLookTarget` (in a
//!   `RunOne` with `DoNothing`, or the riding piglin's
//!   `TriggerGate.triggerOneShuffled`), `SetEntityLookTargetSometimes`
//!   (its `Ticker`) or `RandomLookAround`.
//!
//! VoxelCraft keeps one yaw for Java's `yRot` and `yBodyRot` (the body the
//! models draw); `Mob::yaw` is that body, and the look's `head` and `pitch`
//! are `yHeadRot` and `xRot` in degrees. The drawn `head_yaw` and
//! `head_pitch` are derived from them after every update.
use glam::DVec3;

use super::{Ai, Mob, MobKind, MobWorld, Rng, line_of_sight};
use crate::entity::category::MobCategory;
use crate::entity::{Ctx, PlayerId, Target};

/// `Mob.getHeadRotSpeed`, `getMaxHeadXRot` and `getMaxHeadYRot`.
pub const HEAD_ROT_SPEED: f32 = 10.0;
pub const MAX_HEAD_X_ROT: f32 = 40.0;
pub const MAX_HEAD_Y_ROT: f32 = 75.0;
/// `LookControl.setLookAt`'s `lookAtCooldown`.
const LOOK_COOLDOWN: u8 = 2;
/// `MeleeAttackGoal`, `RangedBowAttackGoal`, `RangedCrossbowAttackGoal`
/// and `RangedAttackGoal`: `setLookAt(target, 30, 30)`.
pub const ATTACK_LOOK: f32 = 30.0;
/// The brain's `NearestLivingEntitySensor` radius and its scan rate.
const SENSE_RANGE: f64 = 16.0;
const SENSE_RATE: u32 = 20;
/// Mobs kept per sensor scan (nearest first; the selectors read the
/// closest few).
const SENSED: usize = 8;

/// `Mth.wrapDegrees`.
pub(crate) fn wrap_degrees(a: f32) -> f32 {
    super::swim::wrap_degrees(a)
}

/// `Control.rotateTowards` (`Mth.degreesDifference`, clamped).
fn rotate_towards(from: f32, to: f32, max: f32) -> f32 {
    from + wrap_degrees(to - from).clamp(-max, max)
}

/// `Mth.rotateIfNecessary`: `base` within `max` of `target`.
pub(crate) fn rotate_if_necessary(base: f32, target: f32, max: f32) -> f32 {
    target - wrap_degrees(target - base).clamp(-max, max)
}

/// `LookControl.getYRotD`: Java's yaw toward `d`, or `None` straight up or down.
pub(crate) fn yaw_toward(d: DVec3) -> Option<f32> {
    (d.x.abs() > 1e-5 || d.z.abs() > 1e-5).then(|| (d.z.atan2(d.x).to_degrees() as f32) - 90.0)
}

/// `LookControl.getXRotD`.
fn pitch_toward(d: DVec3) -> Option<f32> {
    let flat = d.x.hypot(d.z);
    (d.y.abs() > 1e-5 || flat > 1e-5).then(|| -(d.y.atan2(flat).to_degrees() as f32))
}

/// A player's eye height for its pose (standing, crouching, swimming).
pub(crate) fn player_eye(t: &Target) -> f64 {
    match t.shape.height {
        h if h < 1.0 => 0.4,
        h if h < 1.6 => 1.27,
        _ => crate::player::EYE_HEIGHT,
    }
}

/// Java's `LookControl` state and the `yHeadRot`/`xRot` it drives (none
/// saved, as in Java).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct JavaLook {
    /// `yHeadRot` and `xRot` in degrees (0 faces +Z; positive pitch looks down).
    pub head: f32,
    pub pitch: f32,
    cooldown: u8,
    wanted: DVec3,
    y_max: f32,
    x_max: f32,
    /// Set from the body the first time the mob is ticked.
    ready: bool,
}

impl JavaLook {
    /// Whether the mob has been ticked since it was made or loaded (until
    /// then its body is its heading).
    pub(crate) fn ready(&self) -> bool {
        self.ready
    }

    /// `LookControl.isLookingAtTarget`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn looking(&self) -> bool {
        self.cooldown > 0
    }
}

/// Who a `LookAtPlayerGoal` looks at (its `lookAtType`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Who {
    Player,
    /// `Mob.class`: any mob (not players).
    Mob,
    Kind(MobKind),
    /// `Guardian.class`, elder guardians included.
    Guardian,
}

/// One idle look goal of a `registerGoals` table.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Idle {
    /// `LookAtPlayerGoal`; `interact` is `InteractGoal` (flags LOOK and MOVE).
    LookAt { who: Who, range: f32, chance: f32, horizontal: bool, interact: bool },
    /// `RandomLookAroundGoal` (MOVE and LOOK; ticked every tick).
    Random,
    /// `LookAtTradingPlayerGoal`.
    Trading,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    priority: u8,
    goal: Idle,
}

const fn look(priority: u8, who: Who, range: f32, chance: f32) -> Entry {
    Entry { priority, goal: Idle::LookAt { who, range, chance, horizontal: false, interact: false } }
}
const fn player(priority: u8, range: f32) -> Entry {
    look(priority, Who::Player, range, 0.02)
}
const fn random(priority: u8) -> Entry {
    Entry { priority, goal: Idle::Random }
}

const ZOMBIE: &[Entry] = &[player(8, 8.0), random(8)];
const SIX_SIX: &[Entry] = &[player(6, 8.0), random(6)];
const EIGHT_EIGHT: &[Entry] = &[player(8, 8.0), random(8)];
const WITCH: &[Entry] = &[player(3, 8.0), random(3)];
const ENDERMITE: &[Entry] = &[player(7, 8.0), random(8)];
const STRIDER: &[Entry] = &[player(8, 8.0), random(8), look(9, Who::Kind(MobKind::Strider), 8.0, 0.02)];
const RAVAGER: &[Entry] = &[player(6, 6.0), look(10, Who::Mob, 8.0, 0.02)];
const ILLAGER: &[Entry] = &[look(9, Who::Player, 3.0, 1.0), look(10, Who::Mob, 8.0, 0.02)];
const PILLAGER: &[Entry] = &[look(9, Who::Player, 15.0, 1.0), look(10, Who::Mob, 15.0, 0.02)];
const GUARDIAN: &[Entry] = &[player(8, 8.0), look(8, Who::Guardian, 12.0, 0.01), random(9)];
const WANDERING_TRADER: &[Entry] = &[
    Entry { priority: 1, goal: Idle::Trading },
    Entry {
        priority: 9,
        goal: Idle::LookAt { who: Who::Player, range: 3.0, chance: 1.0, horizontal: false, interact: true },
    },
    look(10, Who::Mob, 8.0, 0.02),
];
const FOX: &[Entry] = &[player(12, 24.0)];
const LLAMA: &[Entry] = &[player(8, 6.0), random(9)];
const SIX_SEVEN: &[Entry] = &[player(6, 6.0), random(7)];
const SEVEN_EIGHT: &[Entry] = &[player(7, 6.0), random(8)];
const SNOW_GOLEM: &[Entry] = &[player(3, 6.0), random(4)];
const TURTLE: &[Entry] = &[player(8, 8.0)];
const RABBIT: &[Entry] = &[player(11, 10.0)];
const PANDA: &[Entry] = &[player(9, 6.0), random(10)];
const PARROT: &[Entry] = &[player(1, 8.0)];
const WITHER: &[Entry] = &[player(6, 8.0), random(7)];
const CAT: &[Entry] = &[player(12, 10.0)];
const OCELOT: &[Entry] = &[player(11, 10.0)];
const WOLF: &[Entry] = &[player(10, 8.0), random(10)];

/// The idle look goals `kind` registers (`registerGoals`).
fn goal_table(kind: MobKind) -> &'static [Entry] {
    use MobKind as K;
    match kind {
        K::Zombie | K::Husk | K::Drowned | K::ZombieVillager | K::ZombifiedPiglin => ZOMBIE,
        K::Skeleton
        | K::Stray
        | K::Bogged
        | K::Parched
        | K::WitherSkeleton
        | K::Creeper
        | K::Spider
        | K::CaveSpider => SIX_SIX,
        K::Enderman | K::Blaze => EIGHT_EIGHT,
        K::Witch => WITCH,
        K::Endermite => ENDERMITE,
        K::Strider => STRIDER,
        K::Ravager => RAVAGER,
        K::Vex | K::Evoker | K::Vindicator | K::Illusioner => ILLAGER,
        K::Pillager => PILLAGER,
        K::Guardian | K::ElderGuardian => GUARDIAN,
        K::WanderingTrader => WANDERING_TRADER,
        K::Fox => FOX,
        K::Llama | K::TraderLlama => LLAMA,
        K::PolarBear | K::Chicken | K::Cow | K::Mooshroom => SIX_SEVEN,
        K::IronGolem | K::Pig | K::Sheep | K::Horse | K::Donkey | K::Mule | K::SkeletonHorse | K::ZombieHorse => {
            SEVEN_EIGHT
        }
        K::SnowGolem => SNOW_GOLEM,
        K::Turtle => TURTLE,
        K::Rabbit => RABBIT,
        K::Panda => PANDA,
        K::Parrot => PARROT,
        K::Wither => WITHER,
        K::Cat => CAT,
        K::Ocelot => OCELOT,
        K::Wolf => WOLF,
        _ => &[],
    }
}

/// Which entities a brain look behaviour picks from
/// `NEAREST_VISIBLE_LIVING_ENTITIES`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Sel {
    Player,
    Kind(MobKind),
    Category(MobCategory),
    /// Any living entity.
    Any,
    /// `PiglinAi.isPlayerHoldingLovedItem`.
    LovedItemPlayer,
}

/// A brain behaviour that writes `LOOK_TARGET`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Setter {
    /// `SetEntityLookTarget` alone.
    Direct { sel: Sel, range: f32 },
    /// `SetEntityLookTargetSometimes` with its `Ticker` interval.
    Sometimes { sel: Sel, range: f32, min: i32, max: i32 },
    /// `RunOne` of `SetEntityLookTarget`s (selector, range, weight) and a
    /// `DoNothing(min, max)` of the given weight.
    RunOne { options: &'static [(Sel, f32, u8)], nothing: (u8, i32, i32) },
    /// `TriggerGate.triggerOneShuffled` of `SetEntityLookTarget`s and an
    /// always-true trigger of the given weight (the riding piglin's).
    Shuffled { options: &'static [(Sel, f32, u8)], nothing: u8 },
    /// `RandomLookAround(interval, maxYaw, 0, 0)`.
    Around { min: i32, max: i32, max_yaw: f32 },
}

const PIGLIN_LOOKS: &[(Sel, f32, u8)] =
    &[(Sel::Player, 8.0, 1), (Sel::Kind(MobKind::Piglin), 8.0, 1), (Sel::Any, 8.0, 1)];
const BRUTE_LOOKS: &[(Sel, f32, u8)] = &[
    (Sel::Player, 8.0, 1),
    (Sel::Kind(MobKind::Piglin), 8.0, 1),
    (Sel::Kind(MobKind::PiglinBrute), 8.0, 1),
    (Sel::Any, 8.0, 1),
];
const CELEBRATE_LOOKS: &[(Sel, f32, u8)] = &[(Sel::Kind(MobKind::Piglin), 8.0, 1)];
const VILLAGER_FULL: &[(Sel, f32, u8)] = &[
    (Sel::Kind(MobKind::Cat), 8.0, 8),
    (Sel::Kind(MobKind::Villager), 8.0, 2),
    (Sel::Player, 8.0, 2),
    (Sel::Category(MobCategory::Creature), 8.0, 1),
    (Sel::Category(MobCategory::WaterCreature), 8.0, 1),
    (Sel::Category(MobCategory::Axolotls), 8.0, 1),
    (Sel::Category(MobCategory::UndergroundWaterCreature), 8.0, 1),
    (Sel::Category(MobCategory::WaterAmbient), 8.0, 1),
    (Sel::Category(MobCategory::Monster), 8.0, 1),
];
const VILLAGER_MINIMAL: &[(Sel, f32, u8)] = &[(Sel::Kind(MobKind::Villager), 8.0, 2), (Sel::Player, 8.0, 2)];
const SNIFFER_LOOKS: &[(Sel, f32, u8)] = &[(Sel::Player, 6.0, 1)];

const PLAYER_SOMETIMES: Setter = Setter::Sometimes { sel: Sel::Player, range: 6.0, min: 30, max: 60 };
const ANY_SOMETIMES: Setter = Setter::Sometimes { sel: Sel::Any, range: 8.0, min: 30, max: 60 };
const AROUND: Setter = Setter::Around { min: 150, max: 250, max_yaw: 30.0 };

/// Whether `kind` runs a brain (its looks come from `LOOK_TARGET`).
pub(crate) fn brain(kind: MobKind) -> bool {
    use MobKind as K;
    matches!(
        kind,
        K::Villager
            | K::Piglin
            | K::PiglinBrute
            | K::Hoglin
            | K::Zoglin
            | K::Creaking
            | K::Allay
            | K::CopperGolem
            | K::Armadillo
            | K::Goat
            | K::Frog
            | K::Axolotl
            | K::Tadpole
            | K::Sniffer
            | K::Breeze
            | K::Nautilus
            | K::ZombieNautilus
    )
}

/// Kinds whose head the game draws from its own code, not a look control:
/// the shulker (`ShulkerLookControl`), bat, phantom (no look control),
/// ghast, happy ghast, warden and camels (their own look memories, see
/// `equine` and `warden`), and the Wither (`wither_tick`).
fn own_head(kind: MobKind) -> bool {
    use MobKind as K;
    matches!(kind, K::Shulker | K::Bat | K::Phantom | K::Ghast | K::HappyGhast | K::Warden)
}

/// Which look control a kind has.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Control {
    Plain,
    /// `SmoothSwimmingLookControl(maxYRotFromCenter)`.
    SmoothSwim(f32),
}

fn control(kind: MobKind) -> Control {
    match kind {
        MobKind::Dolphin | MobKind::Tadpole | MobKind::Nautilus | MobKind::ZombieNautilus => Control::SmoothSwim(10.0),
        MobKind::Axolotl => Control::SmoothSwim(20.0),
        _ => Control::Plain,
    }
}

/// Whether a kind's selectors need the mobs around it sensed.
pub(crate) fn senses_mobs(kind: MobKind) -> bool {
    use MobKind as K;
    matches!(
        kind,
        K::Strider
            | K::Ravager
            | K::Vex
            | K::Evoker
            | K::Vindicator
            | K::Illusioner
            | K::Pillager
            | K::Guardian
            | K::ElderGuardian
            | K::WanderingTrader
            | K::Villager
            | K::Piglin
            | K::PiglinBrute
            | K::Hoglin
            | K::Zoglin
            | K::Creaking
            | K::Allay
    )
}

/// A sensed living mob (`NEAREST_VISIBLE_LIVING_ENTITIES`), nearest first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Seen {
    pub uid: u32,
    pub kind: MobKind,
    /// Eye position, refreshed every tick while it is looked at.
    pub eye: DVec3,
    /// Feet position at the scan.
    pub pos: DVec3,
}

/// What a look goal or `LOOK_TARGET` follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tracked {
    Player(PlayerId),
    /// A mob uid and its eye position, refreshed by the sense pass.
    Mob(u32, DVec3),
    /// `BlockPosTracker`.
    Point(DVec3),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Running {
    /// `LookAtPlayerGoal.lookAt` and `lookTime` (in full updates).
    LookAt { at: Tracked, left: i32 },
    /// `RandomLookAroundGoal.relX`, `relZ` and `lookTime` (in ticks).
    Random { rel: (f64, f64), left: i32 },
}

/// The look goals, brain look behaviours and body control (never saved,
/// as Java doesn't save them).
#[derive(Clone, Debug, Default)]
pub(crate) struct Head {
    /// `tickCount` for the selector's full updates.
    ticks: u32,
    /// The running idle goal: its index in the table and state.
    running: Option<(u8, Running)>,
    /// `LOOK_TARGET`, and the `LookAtTargetSink` ticks left (0: not running).
    pub(crate) target: Option<Tracked>,
    sink: Option<i32>,
    /// `SetEntityLookTargetSometimes.Ticker.ticksUntilNextStart`.
    ticker: i32,
    /// A running `DoNothing` of a `RunOne`.
    nothing_left: i32,
    /// `GAZE_COOLDOWN_TICKS` (`RandomLookAround`).
    gaze_cooldown: i32,
    /// `BodyRotationControl.headStableTime` and `lastStableYHeadRot`.
    stable: i32,
    last_stable: f32,
    /// A behaviour other than the idle look goals asked the look control
    /// this tick (it holds `LOOK`).
    pub(crate) requested: bool,
    /// Sensed by the entity pass: a living mob rides this one (its first
    /// passenger steers: `Mob.getControllingPassenger`).
    pub(crate) carrying_mob: bool,
    /// `NEAREST_VISIBLE_LIVING_ENTITIES` (mobs only; players are read live).
    pub(crate) seen: Vec<Seen>,
}

impl Mob {
    /// `getHeadRotSpeed`.
    pub(crate) fn head_rot_speed(&self) -> f32 {
        match self.kind {
            MobKind::Breeze => 25.0,
            MobKind::Frog => 35.0,
            _ => HEAD_ROT_SPEED,
        }
    }

    /// `getMaxHeadXRot`.
    pub(crate) fn max_head_x_rot(&self) -> f32 {
        match self.kind {
            MobKind::Shulker | MobKind::Guardian | MobKind::ElderGuardian => 180.0,
            MobKind::Slime | MobKind::MagmaCube | MobKind::SulfurCube => 0.0,
            MobKind::Wolf if self.animal.as_ref().is_some_and(|a| a.sit_pose) => 20.0,
            MobKind::Axolotl | MobKind::Dolphin => 1.0,
            _ => MAX_HEAD_X_ROT,
        }
    }

    /// `getMaxHeadYRot`.
    pub(crate) fn max_head_y_rot(&self) -> f32 {
        match self.kind {
            MobKind::Shulker => 180.0,
            MobKind::Ravager => 45.0,
            MobKind::Breeze | MobKind::Camel | MobKind::CamelHusk => 30.0,
            MobKind::Goat => 15.0,
            MobKind::Frog => 5.0,
            MobKind::Axolotl | MobKind::Dolphin => 1.0,
            MobKind::Sniffer => 50.0,
            MobKind::Armadillo if self.armadillo.as_ref().is_some_and(|s| s.scared()) => 0.0,
            MobKind::Armadillo => 32.0,
            _ => MAX_HEAD_Y_ROT,
        }
    }

    /// The eye (Java's default eye height, 0.85 of the box).
    pub(crate) fn look_eye(&self) -> DVec3 {
        self.pos + DVec3::Y * (self.shape().height * 0.85)
    }

    fn look_ready(&mut self) {
        if !self.look.ready {
            self.look.ready = true;
            self.body_yaw = self.yaw;
            self.look.head = self.yrot() + self.head_yaw.to_degrees();
            self.look.pitch = -self.head_pitch.to_degrees();
        }
    }

    /// Java's `yBodyRot` in degrees.
    pub(crate) fn body_yrot(&self) -> f32 {
        self.body_yaw.to_degrees() - 90.0
    }

    fn set_body_yrot(&mut self, r: f32) {
        self.body_yaw = super::wrap((r + 90.0).to_radians());
    }

    fn request(&mut self, at: DVec3, y_max: f32, x_max: f32) {
        self.look_ready();
        let l = &mut self.look;
        l.wanted = at;
        l.y_max = y_max;
        l.x_max = x_max;
        l.cooldown = LOOK_COOLDOWN;
    }

    /// `LookControl.setLookAt(x, y, z, yMaxRotSpeed, xMaxRotAngle)` from a
    /// behaviour that holds the `LOOK` flag this tick.
    pub(crate) fn set_look_at(&mut self, at: DVec3, y_max: f32, x_max: f32) {
        self.request(at, y_max, x_max);
        self.head.requested = true;
    }

    /// A goal holds `LOOK` this tick without asking the look control (the
    /// Wither's `WitherDoNothingGoal`).
    pub(crate) fn hold_look(&mut self) {
        self.head.requested = true;
    }

    /// `LookControl.setLookAt(x, y, z)`: the mob's own speeds.
    pub(crate) fn look_toward(&mut self, at: DVec3) {
        let (y, x) = (self.head_rot_speed(), self.max_head_x_rot());
        self.set_look_at(at, y, x);
    }

    /// `BehaviorUtils.lookAtEntity` and the attack behaviours: `LOOK_TARGET`
    /// set to a brain mob's target, which `LookAtTargetSink` follows.
    pub(crate) fn set_look_target(&mut self, at: Tracked) {
        self.head.target = Some(at);
    }

    /// Whether a running goal or behaviour other than the idle looks holds
    /// `MOVE` (strolling, panic, chasing, a moving table goal).
    fn move_held(&self, wish: bool) -> bool {
        wish || matches!(self.ai, Ai::Chase | Ai::Panic)
            || (self.ai == Ai::Wander && self.path_move)
            || self.goals.running.is_some()
            || !self.nav.is_done()
    }

    /// Whether one holds `LOOK`: an attack goal, a table goal that looks
    /// (`BreedGoal`, `TemptGoal`, `FollowOwnerGoal`), or a behaviour that
    /// asked the look control this tick.
    fn look_held(&self) -> bool {
        use super::goals::Goal as G;
        self.head.requested
            || self.ai == Ai::Chase
            || self.goals.running.is_some_and(|i| {
                matches!(super::goals::table(self.kind)[i as usize].goal, G::Breed | G::Tempt | G::FollowOwner { .. })
            })
    }

    fn tracked_pos(&self, at: Tracked, ctx: &Ctx) -> Option<DVec3> {
        match at {
            Tracked::Player(id) => ctx
                .players
                .iter()
                .find(|t| t.id == id && t.alive && !t.spectator)
                .map(|t| t.pos + DVec3::Y * player_eye(t)),
            Tracked::Mob(_, eye) | Tracked::Point(eye) => Some(eye),
        }
    }

    /// A player rides this mob (`EntitySelector.notRiding`).
    fn ridden_by(&self, id: PlayerId) -> bool {
        self.mount.as_ref().is_some_and(|s| s.rider == Some(id))
    }

    /// The nearest player for a look selector: within `range` (scaled by
    /// its visibility unless `brain`), alive, not spectating, in sight.
    fn nearest_player<'a, W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &'a Ctx,
        range: f64,
        brain: bool,
        accept: impl Fn(&Target) -> bool,
    ) -> Option<&'a Target> {
        let eye = self.look_eye();
        let mut best: Option<(f64, &Target)> = None;
        for t in &ctx.players {
            if !t.alive || t.spectator || self.ridden_by(t.id) || !accept(t) {
                continue;
            }
            let reach = if brain { range } else { t.detection_range_for(range, self.kind) };
            let d2 = t.pos.distance_squared(self.pos);
            if d2 > reach * reach || brain && d2 > SENSE_RANGE * SENSE_RANGE {
                continue;
            }
            if best.is_some_and(|(b, _)| b <= d2) {
                continue;
            }
            if line_of_sight(world, eye, t.pos + DVec3::Y * player_eye(t)) {
                best = Some((d2, t));
            }
        }
        best.map(|(_, t)| t)
    }

    /// `NearestVisibleLivingEntities.findClosest` for a brain selector.
    fn find_closest<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, sel: Sel, range: f32) -> Option<Tracked> {
        let range = f64::from(range);
        let player = match sel {
            Sel::Player | Sel::Any => self.nearest_player(world, ctx, range, true, |_| true),
            Sel::LovedItemPlayer => self
                .nearest_player(world, ctx, range, true, |t| t.held_item.is_some_and(crate::entity::nether::is_loved)),
            _ => None,
        };
        let mob = matches!(sel, Sel::Any | Sel::Kind(_) | Sel::Category(_));
        let found = mob
            .then(|| {
                self.head.seen.iter().find(|s| {
                    s.pos.distance_squared(self.pos) <= range * range
                        && match sel {
                            Sel::Kind(k) => s.kind == k,
                            Sel::Category(c) => s.kind.category() == c,
                            _ => true,
                        }
                })
            })
            .flatten();
        match (player, found) {
            (Some(p), Some(m)) if m.pos.distance_squared(self.pos) < p.pos.distance_squared(self.pos) => {
                Some(Tracked::Mob(m.uid, m.eye))
            }
            (Some(p), _) => Some(Tracked::Player(p.id)),
            (None, Some(m)) => Some(Tracked::Mob(m.uid, m.eye)),
            (None, None) => None,
        }
    }

    /// Whether a look target is still in `NEAREST_VISIBLE_LIVING_ENTITIES`
    /// (`EntityTracker.isVisibleBy`).
    fn still_visible<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, at: Tracked) -> bool {
        match at {
            Tracked::Player(id) => ctx.players.iter().any(|t| {
                t.id == id
                    && t.alive
                    && !t.spectator
                    && t.pos.distance_squared(self.pos) <= SENSE_RANGE * SENSE_RANGE
                    && line_of_sight(world, self.look_eye(), t.pos + DVec3::Y * player_eye(t))
            }),
            Tracked::Mob(uid, _) => self.head.seen.iter().any(|s| s.uid == uid),
            Tracked::Point(_) => true,
        }
    }

    /// This tick's idle look goals or brain look behaviours, after the
    /// mob's other behaviours ran (`wish`: one of them moves it).
    pub(super) fn head_goals<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, wish: bool) {
        if own_head(self.kind) || !self.alive() {
            self.head.requested = false;
            return;
        }
        self.look_ready();
        if brain(self.kind) {
            self.brain_looks(world, ctx, rng);
        } else if crate::entity::mounts::camel_like(self.kind) {
            self.camel_random_look(rng);
        } else {
            self.look_goals(world, ctx, rng, wish);
        }
        self.head.requested = false;
        self.head.ticks = self.head.ticks.wrapping_add(1);
    }

    /// The goal selector's part for the idle look goals.
    fn look_goals<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, wish: bool) {
        let table = goal_table(self.kind);
        if table.is_empty() {
            return;
        }
        let full = self.head.ticks.wrapping_add(self.uid).is_multiple_of(2) || self.head.ticks <= 1;
        // `updateControlFlags`: a mob passenger steering this mob disables
        // MOVE and LOOK.
        let disabled = self.head.carrying_mob;
        let move_held = self.move_held(wish);
        let look_held = self.look_held();
        // A better goal taking a flag stops the running one at once.
        if let Some((i, r)) = self.head.running {
            let flags_move = matches!(table[i as usize].goal, Idle::Random | Idle::LookAt { interact: true, .. });
            if look_held || flags_move && move_held || disabled {
                self.head.running = None;
            } else if full {
                // `canContinueToUse`.
                let keep = match r {
                    Running::LookAt { at, left } => {
                        let range = match table[i as usize].goal {
                            Idle::LookAt { range, .. } => f64::from(range),
                            _ => 8.0,
                        };
                        left > 0
                            && self.tracked_pos(at, ctx).is_some_and(|p| {
                                let feet = match at {
                                    Tracked::Player(id) => ctx.players.iter().find(|t| t.id == id).map_or(p, |t| t.pos),
                                    _ => p,
                                };
                                feet.distance_squared(self.pos) <= range * range
                            })
                            && (!matches!(at, Tracked::Mob(..))
                                || self.head.seen.iter().any(|s| matches!(at, Tracked::Mob(u, _) if u == s.uid)))
                            && !(table[i as usize].goal == Idle::Trading
                                && !self.villager.as_ref().is_some_and(|v| v.trading))
                    }
                    Running::Random { left, .. } => left >= 0,
                };
                if !keep {
                    self.head.running = None;
                }
            }
        }
        if full && !disabled && !look_held {
            for (i, entry) in table.iter().enumerate() {
                if self.head.running.is_some_and(|(r, _)| table[r as usize].priority <= entry.priority) {
                    continue;
                }
                let needs_move = matches!(entry.goal, Idle::Random | Idle::LookAt { interact: true, .. });
                if needs_move && move_held {
                    continue;
                }
                if let Some(r) = self.can_use_look(world, ctx, rng, entry.goal) {
                    self.head.running = Some((i as u8, r));
                }
            }
        }
        // Tick: `LookAtPlayerGoal` on full updates, `RandomLookAroundGoal`
        // every tick.
        let Some((i, r)) = self.head.running else { return };
        match r {
            Running::LookAt { at, left } if full => {
                let horizontal = matches!(table[i as usize].goal, Idle::LookAt { horizontal: true, .. });
                if let Some(p) = self.tracked_pos(at, ctx) {
                    let y = if horizontal { self.look_eye().y } else { p.y };
                    let (ys, xs) = (self.head_rot_speed(), self.max_head_x_rot());
                    self.request(DVec3::new(p.x, y, p.z), ys, xs);
                    self.head.running = Some((i, Running::LookAt { at, left: left - 1 }));
                }
            }
            Running::Random { rel, left } => {
                let eye = self.look_eye();
                let (ys, xs) = (self.head_rot_speed(), self.max_head_x_rot());
                self.request(DVec3::new(self.pos.x + rel.0, eye.y, self.pos.z + rel.1), ys, xs);
                self.head.running = Some((i, Running::Random { rel, left: left - 1 }));
            }
            _ => {}
        }
    }

    /// `canUse` and `start` of an idle look goal.
    fn can_use_look<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, rng: &mut Rng, goal: Idle) -> Option<Running> {
        match goal {
            Idle::Random => {
                if rng.next_f32() >= 0.02 {
                    return None;
                }
                let a = std::f64::consts::TAU * f64::from(rng.next_f32());
                Some(Running::Random { rel: (a.cos(), a.sin()), left: 20 + rng.next_int(20) as i32 })
            }
            Idle::Trading => {
                // `isTrading`: the trading player (the nearest one, as the
                // game keeps a trading flag rather than its customer).
                if !self.villager.as_ref().is_some_and(|v| v.trading) {
                    return None;
                }
                let t = ctx
                    .players
                    .iter()
                    .filter(|t| t.alive)
                    .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))?;
                Some(Running::LookAt { at: Tracked::Player(t.id), left: (40 + rng.next_int(40) as i32 + 1) / 2 })
            }
            Idle::LookAt { who, range, chance, .. } => {
                if rng.next_f32() >= chance {
                    return None;
                }
                if self.kind == MobKind::Fox && self.fox_busy() {
                    return None;
                }
                let at = match who {
                    Who::Player => {
                        Tracked::Player(self.nearest_player(world, ctx, f64::from(range), false, |_| true)?.id)
                    }
                    _ => {
                        let r = f64::from(range);
                        let s = self.head.seen.iter().find(|s| {
                            s.pos.distance_squared(self.pos) <= r * r
                                && (s.pos.y - self.pos.y).abs() <= 3.0 + self.shape().height
                                && match who {
                                    Who::Kind(k) => s.kind == k,
                                    Who::Guardian => matches!(s.kind, MobKind::Guardian | MobKind::ElderGuardian),
                                    _ => true,
                                }
                        })?;
                        Tracked::Mob(s.uid, s.eye)
                    }
                };
                // `start`: `adjustedTickDelay(40 + nextInt(40))` full updates.
                Some(Running::LookAt { at, left: (40 + rng.next_int(40) as i32 + 1) / 2 })
            }
        }
    }

    /// `FoxLookAtPlayerGoal`: not while faceplanted or interested.
    fn fox_busy(&self) -> bool {
        self.animal.as_ref().is_some_and(|a| a.sleeping)
    }

    /// The brain's look behaviours this tick: the activity's `LOOK_TARGET`
    /// writers, then the core `LookAtTargetSink(45, 90)`.
    fn brain_looks<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) {
        // Start phase, in priority order: the core sink starts while the
        // memory is present (`Behavior.tryStart`: `minDuration +
        // nextInt(max + 1 - min)` ticks, timed out once past them), then the
        // villager's trading sink, then the activity's writers (each needs
        // the memory absent).
        if self.head.sink.is_none() && self.head.target.is_some() {
            self.head.sink = Some(45 + rng.next_int(46) as i32 + 1);
        }
        self.head.gaze_cooldown = (self.head.gaze_cooldown - 1).max(0);
        if self.kind == MobKind::Villager {
            self.villager_trading_look(ctx);
        }
        if self.head.nothing_left > 0 {
            self.head.nothing_left -= 1;
        }
        for setter in self.brain_setters() {
            if self.head.target.is_some() {
                break;
            }
            self.run_setter(world, ctx, rng, *setter);
        }
        // Tick phase: `LookAtTargetSink.tickOrStop`.
        let Some(left) = self.head.sink else { return };
        let visible = self.head.target.is_some_and(|at| self.still_visible(world, ctx, at));
        if left <= 0 || !visible {
            self.head.target = None;
            self.head.sink = None;
            return;
        }
        self.head.sink = Some(left - 1);
        let still = self.kind == MobKind::Creaking && self.creaking.as_ref().is_some_and(|s| !s.can_move);
        if let Some(p) = self.head.target.and_then(|at| self.tracked_pos(at, ctx))
            && !still
        {
            let (ys, xs) = (self.head_rot_speed(), self.max_head_x_rot());
            self.request(p, ys, xs);
        }
    }

    /// `LookAndFollowTradingPlayerSink`: a trading villager within 4
    /// blocks of its customer keeps it as `LOOK_TARGET` (not in water or
    /// hurt recently).
    fn villager_trading_look(&mut self, ctx: &Ctx) {
        if !self.villager.as_ref().is_some_and(|v| v.trading) || self.in_water || self.hurt_age < 10 {
            return;
        }
        let near = ctx
            .players
            .iter()
            .filter(|t| t.alive && t.pos.distance_squared(self.pos) <= 16.0)
            .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)));
        if let Some(t) = near {
            self.set_look_target(Tracked::Player(t.id));
        }
    }

    /// The `LOOK_TARGET` writers of the brain's current activity.
    fn brain_setters(&self) -> &'static [Setter] {
        use MobKind as K;
        match self.kind {
            K::Villager => {
                let Some(v) = &self.villager else { return &[] };
                const FULL: &[Setter] = &[Setter::RunOne { options: VILLAGER_FULL, nothing: (2, 30, 60) }];
                const MINIMAL: &[Setter] = &[Setter::RunOne { options: VILLAGER_MINIMAL, nothing: (8, 30, 60) }];
                use crate::entity::villager::Activity;
                let raiding = v.raid_activity != crate::entity::villager::RaidActivity::default();
                if v.fleeing || v.bell_hide > 0.0 || raiding {
                    return MINIMAL;
                }
                match v.activity {
                    Activity::Work | Activity::Rest => MINIMAL,
                    Activity::Idle | Activity::Meet | Activity::Play => FULL,
                }
            }
            K::Piglin => self.piglin_setters(),
            K::PiglinBrute => {
                const IDLE: &[Setter] = &[Setter::RunOne { options: BRUTE_LOOKS, nothing: (1, 30, 60) }];
                if self.ai == Ai::Chase { &[] } else { IDLE }
            }
            K::Hoglin | K::Zoglin => {
                const IDLE: &[Setter] = &[ANY_SOMETIMES];
                if self.ai == Ai::Chase
                    || self
                        .nether
                        .as_ref()
                        .is_some_and(|n| matches!(n.goal, crate::entity::nether::Goal::Attack { .. }))
                {
                    &[]
                } else {
                    IDLE
                }
            }
            K::Creaking => &[ANY_SOMETIMES],
            K::Allay => &[Setter::Sometimes { sel: Sel::Any, range: 6.0, min: 30, max: 60 }],
            K::CopperGolem => &[Setter::Sometimes { sel: Sel::Player, range: 6.0, min: 40, max: 80 }],
            K::Armadillo => &[PLAYER_SOMETIMES, AROUND],
            K::Goat | K::Frog | K::Axolotl | K::Tadpole => &[PLAYER_SOMETIMES],
            K::Sniffer => &[Setter::RunOne { options: SNIFFER_LOOKS, nothing: (7, 5, 20) }],
            _ => &[],
        }
    }

    /// `PiglinAi`'s look writers in the piglin's activity: idle and avoid
    /// (`createIdleLookBehaviors`, with the loved-item player in idle),
    /// celebrating, and riding (`initRideHoglinActivity`).
    fn piglin_setters(&self) -> &'static [Setter] {
        use crate::entity::nether::Goal;
        const IDLE: &[Setter] = &[
            Setter::Direct { sel: Sel::LovedItemPlayer, range: 14.0 },
            Setter::RunOne { options: PIGLIN_LOOKS, nothing: (1, 30, 60) },
        ];
        const AVOID: &[Setter] = &[Setter::RunOne { options: PIGLIN_LOOKS, nothing: (1, 30, 60) }];
        const CELEBRATE: &[Setter] = &[
            Setter::Direct { sel: Sel::LovedItemPlayer, range: 14.0 },
            Setter::RunOne { options: CELEBRATE_LOOKS, nothing: (2, 10, 20) },
        ];
        const RIDE: &[Setter] = &[
            Setter::Direct { sel: Sel::LovedItemPlayer, range: 8.0 },
            Setter::Shuffled { options: PIGLIN_LOOKS, nothing: 1 },
        ];
        let Some(n) = &self.nether else { return &[] };
        if n.offhand.is_some() {
            return &[];
        }
        match n.goal {
            Goal::Attack { .. } => &[],
            Goal::Flee(_) => AVOID,
            _ if self.riding.is_some() => RIDE,
            _ if n.celebrate_left > 0.0 => CELEBRATE,
            _ => IDLE,
        }
    }

    fn run_setter<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, setter: Setter) {
        match setter {
            Setter::Direct { sel, range } => {
                if let Some(at) = self.find_closest(world, ctx, sel, range) {
                    self.head.target = Some(at);
                }
            }
            Setter::Sometimes { sel, range, min, max } => {
                if self.find_closest(world, ctx, sel, range).is_none() {
                    return;
                }
                // `Ticker.tickDownAndCheck`.
                let fire = if self.head.ticker == 0 {
                    self.head.ticker = min + rng.next_int((max - min + 1) as u32) as i32 - 1;
                    false
                } else {
                    self.head.ticker -= 1;
                    self.head.ticker == 0
                };
                if fire {
                    self.head.target = self.find_closest(world, ctx, sel, range);
                }
            }
            Setter::RunOne { options, nothing } => {
                if self.head.nothing_left > 0 {
                    return;
                }
                // `ShufflingList.shuffle`: each entry keyed by
                // `-nextFloat() ^ (1 / weight)`, tried in ascending order.
                let mut order: Vec<(f32, usize)> = (0..=options.len())
                    .map(|i| {
                        let w = if i < options.len() { options[i].2 } else { nothing.0 };
                        (-rng.next_f32().powf(1.0 / f32::from(w.max(1))), i)
                    })
                    .collect();
                order.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (_, i) in order {
                    if i == options.len() {
                        self.head.nothing_left = nothing.1 + rng.next_int((nothing.2 - nothing.1 + 1) as u32) as i32;
                        return;
                    }
                    let (sel, range, _) = options[i];
                    if let Some(at) = self.find_closest(world, ctx, sel, range) {
                        self.head.target = Some(at);
                        return;
                    }
                }
            }
            Setter::Shuffled { options, nothing } => {
                if self.riding.is_none() {
                    return;
                }
                let mut order: Vec<(f32, usize)> = (0..=options.len())
                    .map(|i| {
                        let w = if i < options.len() { options[i].2 } else { nothing };
                        (-rng.next_f32().powf(1.0 / f32::from(w.max(1))), i)
                    })
                    .collect();
                order.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (_, i) in order {
                    if i == options.len() {
                        return;
                    }
                    let (sel, range, _) = options[i];
                    if let Some(at) = self.find_closest(world, ctx, sel, range) {
                        self.head.target = Some(at);
                        return;
                    }
                }
            }
            Setter::Around { min, max, max_yaw } => {
                if self.head.gaze_cooldown > 0 {
                    return;
                }
                self.random_look_around(rng, min, max, max_yaw);
            }
        }
    }

    /// `RandomLookAround.start` with no pitch range.
    fn random_look_around(&mut self, rng: &mut Rng, min: i32, max: i32, max_yaw: f32) {
        let pitch = rng.next_f32() * 0.0;
        let rotation = wrap_degrees(self.yrot() + 2.0 * rng.next_f32() * max_yaw - max_yaw);
        let dir = super::spear::view_vector(pitch, rotation);
        self.head.target = Some(Tracked::Point(self.look_eye() + dir));
        self.head.gaze_cooldown = min + rng.next_int((max - min + 1) as u32) as i32;
    }

    /// The camel brain's `RandomLookAround(150..250, 30, 0, 0)`, beside the
    /// look memory `equine::camel_look` keeps (it erases its own target).
    fn camel_random_look(&mut self, rng: &mut Rng) {
        self.head.gaze_cooldown = (self.head.gaze_cooldown - 1).max(0);
        let has_target = self.mount.as_ref().is_some_and(|s| s.look_target.is_some());
        if !has_target && self.head.target.is_none() && self.head.gaze_cooldown == 0 {
            self.random_look_around(rng, 150, 250, 30.0);
            self.head.sink = Some(45 + rng.next_int(46) as i32 + 1);
        }
        if let Some(Tracked::Point(p)) = self.head.target {
            let left = self.head.sink.unwrap_or(0);
            if has_target || left <= 0 {
                self.head.target = None;
                self.head.sink = None;
            } else {
                self.head.sink = Some(left - 1);
                let (ys, xs) = (self.head_rot_speed(), self.max_head_x_rot());
                self.request(p, ys, xs);
            }
        }
    }

    /// `LookControl.tick` (or the kind's own), after the move control.
    pub(crate) fn look_tick(&mut self) {
        if own_head(self.kind) || self.dying.is_some() {
            return;
        }
        self.look_ready();
        // A rider steering by hand sets the head to the body
        // (`tickRidden`: `yHeadRot = yBodyRot = yRot`).
        if self.mount.as_ref().is_some_and(|s| s.controlled()) {
            self.body_yaw = self.yaw;
            self.look.head = self.yrot();
            self.look.pitch = 0.0;
            self.look.cooldown = 0;
            return;
        }
        let skip = match self.kind {
            MobKind::Fox => self.animal.as_ref().is_some_and(|a| a.sleeping),
            MobKind::Axolotl => self.aquatic.as_ref().is_some_and(|a| a.play_dead > 0.0),
            MobKind::Creaking => self.creaking.as_ref().is_some_and(|s| !s.can_move),
            MobKind::Bee => self.bee.is_some_and(|s| s.anger > 0),
            _ => false,
        };
        if skip {
            return;
        }
        let body = self.body_yrot();
        let eye = self.look_eye();
        let navigating = !self.nav.is_done();
        let mut l = self.look;
        match control(self.kind) {
            Control::Plain => {
                // `resetXRotOnTick`: not for a frog with a tongue target.
                let keep_pitch = self.kind == MobKind::Frog && self.frog.as_ref().is_some_and(|f| f.tongue.is_some());
                if !keep_pitch {
                    l.pitch = 0.0;
                }
                if l.cooldown > 0 {
                    l.cooldown -= 1;
                    let d = l.wanted - eye;
                    if let Some(yaw) = yaw_toward(d) {
                        l.head = rotate_towards(l.head, yaw, l.y_max);
                    }
                    if let Some(pitch) = pitch_toward(d) {
                        l.pitch = rotate_towards(l.pitch, pitch, l.x_max);
                    }
                } else {
                    l.head = rotate_towards(l.head, body, HEAD_ROT_SPEED);
                }
                if navigating {
                    l.head = rotate_if_necessary(l.head, body, self.max_head_y_rot());
                }
            }
            Control::SmoothSwim(max_from_center) => {
                if l.cooldown > 0 {
                    l.cooldown -= 1;
                    let d = l.wanted - eye;
                    if let Some(yaw) = yaw_toward(d) {
                        l.head = rotate_towards(l.head, yaw + 20.0, l.y_max);
                    }
                    if let Some(pitch) = pitch_toward(d) {
                        l.pitch = rotate_towards(l.pitch, pitch + 10.0, l.x_max);
                    }
                } else {
                    if !navigating {
                        l.pitch = rotate_towards(l.pitch, 0.0, 5.0);
                    }
                    // `yMaxRotSpeed` of the last request (0 before any).
                    l.head = rotate_towards(l.head, body, l.y_max);
                }
                let off = wrap_degrees(l.head - body);
                if off < -max_from_center {
                    self.set_body_yrot(body - 4.0);
                } else if off > max_from_center {
                    self.set_body_yrot(body + 4.0);
                }
            }
        }
        l.head = wrap_degrees(l.head);
        self.look = l;
    }

    /// `BodyRotationControl.clientTick` (or the kind's own), after the mob
    /// moved, then the drawn head. `moved` is this tick's horizontal move.
    pub(super) fn body_tick(&mut self, moved: DVec3) {
        if own_head(self.kind) {
            // Their own code turns the one yaw they draw.
            self.body_yaw = self.yaw;
            return;
        }
        self.look_ready();
        if self.dying.is_none() {
            let frozen = match self.kind {
                MobKind::Armadillo => self.armadillo.as_ref().is_some_and(|s| s.scared()),
                MobKind::Creaking => self.creaking.as_ref().is_some_and(|s| !s.can_move),
                _ => false,
            };
            if crate::entity::mounts::camel_like(self.kind) || self.mount.as_ref().is_some_and(|s| s.controlled()) {
                // `CamelBodyRotationControl` (Track 3b) turns the camel's
                // heading; a steered mount's body is its rider's heading.
                self.body_yaw = self.yaw;
            } else if !frozen {
                self.body_rotation(moved);
            }
        }
        let body = self.body_yrot();
        self.head_yaw = super::wrap(wrap_degrees(self.look.head - body).to_radians());
        self.head_pitch = -self.look.pitch.to_radians();
    }

    fn body_rotation(&mut self, moved: DVec3) {
        let max = self.max_head_y_rot();
        let head = self.look.head;
        if moved.x * moved.x + moved.z * moved.z > f64::from(2.500_000_3e-7_f32) {
            // `yBodyRot = yRot`, the head within its limit of it.
            self.body_yaw = self.yaw;
            self.look.head = rotate_if_necessary(head, self.yrot(), max);
            self.head.last_stable = self.look.head;
            self.head.stable = 0;
        } else if !self.head.carrying_mob {
            if wrap_degrees(head - self.head.last_stable).abs() > 15.0 {
                self.head.stable = 0;
                self.head.last_stable = head;
                let body = rotate_if_necessary(self.body_yrot(), head, max);
                self.set_body_yrot(body);
            } else {
                self.head.stable += 1;
                if self.head.stable > 10 {
                    let fraction = ((self.head.stable - 10) as f32 / 10.0).clamp(0.0, 1.0);
                    let body = rotate_if_necessary(self.body_yrot(), head, max * (1.0 - fraction));
                    self.set_body_yrot(body);
                }
            }
        }
    }
}

impl crate::entity::Entities {
    /// The look senses, every tick before the mobs move: which mobs carry
    /// a mob passenger, the brain's `NearestLivingEntitySensor` (every 20
    /// ticks, staggered by uid) for the kinds whose look selectors pick
    /// mobs, and the eye of every mob being looked at.
    pub(crate) fn sense_looks<W: MobWorld + ?Sized>(&mut self, world: &W) {
        use crate::entity::mounts::MOB_VEHICLE;
        let mut by_uid: rustc_hash::FxHashMap<u32, usize> = rustc_hash::FxHashMap::default();
        for (i, m) in self.mobs.iter().enumerate() {
            by_uid.insert(m.uid, i);
        }
        let mut carrying = vec![false; self.mobs.len()];
        for m in &self.mobs {
            if m.alive()
                && let Some(v) = m.riding.filter(|v| v & MOB_VEHICLE != 0)
                && let Some(&j) = by_uid.get(&(v & !MOB_VEHICLE))
            {
                carrying[j] = true;
            }
        }
        for i in 0..self.mobs.len() {
            self.mobs[i].head.carrying_mob = carrying[i];
            let m = &self.mobs[i];
            if !m.simulation_active || !m.alive() || !senses_mobs(m.kind) {
                continue;
            }
            let scan = m.head.ticks.wrapping_add(m.uid).is_multiple_of(SENSE_RATE) || m.head.ticks == 0;
            if scan {
                let eye = m.look_eye();
                let mut near: Vec<(f64, usize)> = Vec::new();
                self.mob_index.visit(m.pos, SENSE_RANGE, |j| {
                    let b = &self.mobs[j];
                    if j == i || !b.alive() {
                        return;
                    }
                    let d = b.pos - m.pos;
                    if d.x.abs() <= SENSE_RANGE && d.z.abs() <= SENSE_RANGE && d.y.abs() <= SENSE_RANGE {
                        near.push((d.length_squared(), j));
                    }
                });
                near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let mut seen = Vec::with_capacity(SENSED);
                for (_, j) in near {
                    if seen.len() >= SENSED {
                        break;
                    }
                    let b = &self.mobs[j];
                    let at = b.look_eye();
                    if line_of_sight(world, eye, at) {
                        seen.push(Seen { uid: b.uid, kind: b.kind, eye: at, pos: b.pos });
                    }
                }
                self.mobs[i].head.seen = seen;
            } else {
                // Positions move between scans; the list keeps its members.
                let mut seen = std::mem::take(&mut self.mobs[i].head.seen);
                seen.retain_mut(|s| match by_uid.get(&s.uid) {
                    Some(&j) if self.mobs[j].alive() => {
                        s.eye = self.mobs[j].look_eye();
                        s.pos = self.mobs[j].pos;
                        true
                    }
                    _ => false,
                });
                self.mobs[i].head.seen = seen;
            }
        }
        // Mobs looked at: refresh their eyes (gone ones end the look).
        for i in 0..self.mobs.len() {
            let refresh = |at: Tracked| match at {
                Tracked::Mob(uid, _) => match by_uid.get(&uid) {
                    Some(&j) if self.mobs[j].alive() => Some(Tracked::Mob(uid, self.mobs[j].look_eye())),
                    _ => None,
                },
                other => Some(other),
            };
            let h = &self.mobs[i].head;
            let target = h.target.and_then(refresh);
            let running = match h.running {
                Some((k, Running::LookAt { at, left })) => refresh(at).map(|at| (k, Running::LookAt { at, left })),
                other => other,
            };
            let h = &mut self.mobs[i].head;
            if h.target.is_some() && target.is_none() {
                h.sink = None;
            }
            h.target = target;
            h.running = running;
        }
    }
}

#[cfg(test)]
mod tests;
