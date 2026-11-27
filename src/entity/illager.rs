//! v0.15 Track A: the illagers (vindicators, evokers, illusioners and
//! pillagers), the evoker's vexes and fangs, and ravagers, after Java 26.3
//! `AbstractIllager`, `SpellcasterIllager`, `Vindicator`, `Evoker`,
//! `Illusioner`, `Pillager`, `Vex`, `Ravager`, `EvokerFangs` and `Raider`.
//!
//! Each mob's goals run in `mob::illager`; this module holds the numbers,
//! the saved state, raid membership (the [`Raider`] hook raids attach to)
//! and the entity-level passes: targets other than players, a vex's life,
//! the evoker's summons, fangs and `wololo`, the ravager's roar and
//! griefing, and loot. Every number is checked against
//! `tests/fixtures/java/26.3/illagers.json`.
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::nether::Foe;
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng};
use crate::inventory::Stack;
use crate::item::{Item, Tier, ToolKind};
use crate::physics::Shape;
use crate::simulation::damage::{Cause, DamageType};
use crate::world::block::Block;

/// Seconds per game tick.
pub const TICK: f64 = 0.05;

/// Evoker fangs hurt with indirect magic.
pub const FANGS_CAUSE: Cause = Cause::new(DamageType::IndirectMagic, "was killed by an evoker using magic");
pub const ILLUSIONER_ARROW: Cause = Cause::new(DamageType::Arrow, "was shot by an illusioner");
pub const RAVAGER_ROAR: Cause = Cause::new(DamageType::MobAttack, "was slain by a ravager");

/// The FOLLOW_RANGE each kind's `createAttributes` sets.
pub fn follow_range(kind: MobKind) -> f64 {
    kind.follow_range_base()
}

/// `#illager` (and `#illager_friends`, which is only `#illager`).
pub fn is_illager(kind: MobKind) -> bool {
    matches!(kind, MobKind::Evoker | MobKind::Illusioner | MobKind::Pillager | MobKind::Vindicator)
}

/// `VillagerHostilesSensor.ACCEPTABLE_DISTANCE_FROM_HOSTILES`: how close
/// each hostile comes before villagers flee (the zombies, the illager
/// family and, since v0.15 Track H, zoglins).
pub fn villager_fear(kind: MobKind) -> Option<f64> {
    match kind {
        k if k.is_zombie() => Some(8.0),
        MobKind::Vex => Some(8.0),
        MobKind::Vindicator | MobKind::Zoglin => Some(10.0),
        MobKind::Evoker | MobKind::Illusioner | MobKind::Ravager => Some(12.0),
        MobKind::Pillager => Some(15.0),
        _ => None,
    }
}

/// `#raiders`: what a raid can hold, and what `HurtByTargetGoal(Raider)`
/// ignores hits from.
pub fn is_raider(kind: MobKind) -> bool {
    is_illager(kind) || matches!(kind, MobKind::Ravager | MobKind::Witch)
}

/// The illagers' sounds beyond their ambient, hurt and death calls
/// (`SoundEvents`: spell preparing and casting, fangs, the vex's charge,
/// the ravager's attack, stun and roar, the pillager's crossbow and the
/// raiders' celebration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// `evoker.prepare_summon`, `.prepare_attack`, `.prepare_wololo`,
    /// `illusioner.prepare_mirror` and `.prepare_blindness`.
    Prepare(Spell),
    /// `evoker.cast_spell` and `illusioner.cast_spell`.
    Cast,
    /// `evoker_fangs.attack`.
    Fangs,
    /// `vex.charge`.
    VexCharge,
    /// `ravager.attack`, `.stunned` and `.roar`.
    RavagerAttack,
    Stunned,
    Roar,
    /// `crossbow.loading_end` and `crossbow.shoot`.
    CrossbowLoaded,
    CrossbowShoot,
    /// `*.celebrate`.
    Celebrate(MobKind),
    /// v1.0 gap Track 2: `zombie.attack_wooden_door` (level event 1019) and
    /// `zombie.break_wooden_door` (1021), played by `BreakDoorGoal` for
    /// zombies and vindicators alike.
    DoorAttack,
    DoorBreak,
}

impl Cue {
    /// Every cue, for the sound bank.
    pub const ALL: [Cue; 18] = [
        Cue::Prepare(Spell::SummonVex),
        Cue::Prepare(Spell::Fangs),
        Cue::Prepare(Spell::Wololo),
        Cue::Prepare(Spell::Disappear),
        Cue::Prepare(Spell::Blindness),
        Cue::Cast,
        Cue::Fangs,
        Cue::VexCharge,
        Cue::RavagerAttack,
        Cue::Stunned,
        Cue::Roar,
        Cue::CrossbowLoaded,
        Cue::CrossbowShoot,
        Cue::Celebrate(MobKind::Vindicator),
        Cue::Celebrate(MobKind::Evoker),
        Cue::Celebrate(MobKind::Pillager),
        Cue::DoorAttack,
        Cue::DoorBreak,
    ];

    /// A stable key for sound banks and LAN.
    pub fn key(self) -> &'static str {
        match self {
            Cue::Prepare(Spell::SummonVex) => "evoker.prepare_summon",
            Cue::Prepare(Spell::Fangs) => "evoker.prepare_attack",
            Cue::Prepare(Spell::Wololo) => "evoker.prepare_wololo",
            Cue::Prepare(Spell::Disappear) => "illusioner.prepare_mirror",
            Cue::Prepare(_) => "illusioner.prepare_blindness",
            Cue::Cast => "evoker.cast_spell",
            Cue::Fangs => "evoker_fangs.attack",
            Cue::VexCharge => "vex.charge",
            Cue::RavagerAttack => "ravager.attack",
            Cue::Stunned => "ravager.stunned",
            Cue::Roar => "ravager.roar",
            Cue::CrossbowLoaded => "crossbow.loading_end",
            Cue::CrossbowShoot => "crossbow.shoot",
            Cue::Celebrate(MobKind::Evoker) => "evoker.celebrate",
            Cue::Celebrate(MobKind::Pillager) => "pillager.celebrate",
            Cue::Celebrate(_) => "vindicator.celebrate",
            Cue::DoorAttack => "zombie.attack_wooden_door",
            Cue::DoorBreak => "zombie.break_wooden_door",
        }
    }

    pub fn from_key(key: &str) -> Option<Cue> {
        Cue::ALL.into_iter().find(|c| c.key() == key)
    }
}

// ---------------------------------------------------------------- spells

/// `SpellcasterIllager.IllagerSpell`, in id order (the synced byte).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spell {
    #[default]
    None,
    SummonVex,
    Fangs,
    Wololo,
    Disappear,
    Blindness,
}

impl Spell {
    pub const ALL: [Spell; 6] =
        [Spell::None, Spell::SummonVex, Spell::Fangs, Spell::Wololo, Spell::Disappear, Spell::Blindness];

    /// `IllagerSpell.byId` with `OutOfBoundsStrategy.ZERO`.
    pub fn from_id(id: u64) -> Spell {
        Spell::ALL.get(id as usize).copied().unwrap_or(Spell::None)
    }

    /// `spellColor`: the particles at the caster's hands.
    pub fn colour(self) -> [f32; 3] {
        match self {
            Spell::None => [0.0, 0.0, 0.0],
            Spell::SummonVex => [0.7, 0.7, 0.8],
            Spell::Fangs => [0.4, 0.3, 0.35],
            Spell::Wololo => [0.7, 0.5, 0.2],
            Spell::Disappear => [0.3, 0.3, 0.8],
            Spell::Blindness => [0.1, 0.1, 0.2],
        }
    }
}

/// A `SpellcasterUseSpellGoal`: `getCastWarmupTime`, `getCastingTime` and
/// `getCastingInterval` in ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellGoal {
    pub spell: Spell,
    pub warmup: u32,
    pub casting: i32,
    pub interval: u32,
}

/// `SpellcasterUseSpellGoal.getCastWarmupTime`'s default.
pub const DEFAULT_WARMUP: u32 = 20;
pub const SUMMON: SpellGoal =
    SpellGoal { spell: Spell::SummonVex, warmup: DEFAULT_WARMUP, casting: 100, interval: 340 };
pub const FANGS: SpellGoal = SpellGoal { spell: Spell::Fangs, warmup: DEFAULT_WARMUP, casting: 40, interval: 100 };
pub const WOLOLO: SpellGoal = SpellGoal { spell: Spell::Wololo, warmup: 40, casting: 60, interval: 140 };
pub const MIRROR: SpellGoal = SpellGoal { spell: Spell::Disappear, warmup: DEFAULT_WARMUP, casting: 20, interval: 340 };
pub const BLINDNESS: SpellGoal =
    SpellGoal { spell: Spell::Blindness, warmup: DEFAULT_WARMUP, casting: 20, interval: 180 };

/// The spell goals in each caster's registration order, which is the order
/// the selector offers them a start.
pub fn spell_goals(kind: MobKind) -> &'static [SpellGoal] {
    match kind {
        MobKind::Evoker => &[SUMMON, FANGS, WOLOLO],
        MobKind::Illusioner => &[MIRROR, BLINDNESS],
        _ => &[],
    }
}

/// `EvokerSummonSpellGoal`: three vexes, cast only while
/// `nextInt(8) + 1` beats the vexes within 16 blocks.
pub const SUMMON_VEXES: u32 = 3;
pub const SUMMON_ROLL: u32 = 8;
pub const VEX_COUNT_RANGE: f64 = 16.0;
/// A summoned vex lives `20 * (30 + nextInt(90))` ticks.
pub const VEX_LIFE_MIN_SECONDS: i32 = 30;
pub const VEX_LIFE_SPREAD_SECONDS: u32 = 90;
/// `EvokerWololoSpellGoal`: a blue sheep within 16 blocks, in the box
/// grown 16 x 4 x 16, turns red; it needs `mobGriefing`.
pub const WOLOLO_RANGE: f64 = 16.0;
pub const WOLOLO_BOX_VERTICAL: f64 = 4.0;
/// `IllusionerMirrorSpellGoal` and `IllusionerBlindnessSpellGoal` effects.
pub const INVISIBILITY_TICKS: u32 = 1200;
pub const BLINDNESS_TICKS: u32 = 400;

// ---------------------------------------------------------------- fangs

/// `EvokerAttackSpellGoal.performSpellCasting`: a target closer than 3
/// blocks (`distanceToSqr < 9`) gets two rings, five fangs at 1.5 blocks
/// then eight at 2.5 blocks three ticks later; a farther one a line of 16
/// fangs 1.25 blocks apart, each a tick after the last.
pub const FANG_CLOSE_SQR: f64 = 9.0;
pub const FANG_INNER: (u32, f64) = (5, 1.5);
pub const FANG_OUTER: (u32, f64, i32) = (8, 2.5, 3);
pub const FANG_LINE: (u32, f64) = (16, 1.25);
/// `EvokerFangs`: 22 life ticks after the warmup, a bite 8 ticks into it
/// on whatever living entity's box touches its own grown 0.2 sideways, for
/// 6 damage; the owner and its allies are spared.
pub const FANG_LIFE: i32 = 22;
pub const FANG_BITE_AT: i32 = -8;
pub const FANG_DAMAGE: f32 = 6.0;
pub const FANG_INFLATE: f64 = 0.2;
pub const FANG_SHAPE: Shape = Shape::new(0.25, 0.8);

/// Where a cast puts its fangs, before each finds the ground: `(x, z,
/// yaw in radians, warmup ticks)`, relative to the evoker at the origin,
/// for a target `angle` radians away (`atan2(dz, dx)`, as Java's) at
/// squared distance `dist_sqr`. Java's float angles are kept.
pub fn fang_layout(angle: f32, dist_sqr: f64) -> Vec<(f64, f64, f32, i32)> {
    let mut out = Vec::new();
    if dist_sqr < FANG_CLOSE_SQR {
        for i in 0..FANG_INNER.0 {
            let a = angle + i as f32 * std::f32::consts::PI * 0.4;
            out.push((f64::from(a.cos()) * FANG_INNER.1, f64::from(a.sin()) * FANG_INNER.1, a, 0));
        }
        for i in 0..FANG_OUTER.0 {
            let a = angle + i as f32 * std::f32::consts::PI * 2.0 / 8.0 + 1.256_637_1;
            out.push((f64::from(a.cos()) * FANG_OUTER.1, f64::from(a.sin()) * FANG_OUTER.1, a, FANG_OUTER.2));
        }
    } else {
        for i in 0..FANG_LINE.0 {
            let reach = FANG_LINE.1 * f64::from(i + 1);
            out.push((f64::from(angle.cos()) * reach, f64::from(angle.sin()) * reach, angle, i as i32));
        }
    }
    out
}

/// `createSpellEntity`'s ground search: from `max_y` down to `floor(min_y) -
/// 1`, the first cell standing on a block with a sturdy top, the fang
/// resting on top of whatever shape that cell has. `None` finds no ground.
pub fn fang_ground<W: MobWorld + ?Sized>(world: &W, x: f64, z: f64, min_y: f64, max_y: f64) -> Option<f64> {
    let (cx, cz) = (x.floor() as i32, z.floor() as i32);
    let mut y = max_y.floor() as i32;
    let floor = min_y.floor() as i32 - 1;
    loop {
        let below = world.block(IVec3::new(cx, y - 1, cz)).unwrap_or(Block::AIR);
        if crate::world::mount_states::face_sturdy(below, IVec3::Y) {
            let top = crate::physics::collision_top(world, IVec3::new(cx, y, cz)).unwrap_or(0.0);
            return Some(f64::from(y) + top);
        }
        y -= 1;
        if y < floor {
            return None;
        }
    }
}

/// One evoker fang (`EvokerFangs`). Not saved: it lives at most 38 ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct Fang {
    pub pos: DVec3,
    /// Yaw in radians (`atan2(dz, dx)`).
    pub yaw: f32,
    /// `warmupDelayTicks`, counted down past zero.
    pub warmup: i32,
    /// `lifeTicks`, counted once the warmup is over.
    pub life: i32,
    /// The evoker's uid (0 for none).
    pub owner: u32,
}

impl Fang {
    pub fn new(pos: DVec3, yaw: f32, warmup: i32, owner: u32) -> Self {
        Fang { pos, yaw, warmup, life: FANG_LIFE, owner }
    }

    /// How far the jaws have risen, 0..1 (`getAnimationProgress`).
    pub fn progress(&self) -> f32 {
        if self.warmup >= 0 {
            return 0.0;
        }
        let left = self.life - 2;
        if left <= 0 { 1.0 } else { 1.0 - left as f32 / 20.0 }
    }

    /// The box a bite reaches: the fang's grown by [`FANG_INFLATE`] sideways.
    pub fn bite_box(&self) -> (DVec3, DVec3) {
        let (min, max) = FANG_SHAPE.aabb(self.pos);
        let grow = DVec3::new(FANG_INFLATE, 0.0, FANG_INFLATE);
        (min - grow, max + grow)
    }
}

// ---------------------------------------------------------------- vex

/// `Vex.tick`: a vex with a limited life takes 1 starvation damage when
/// its life runs out, and again every 20 ticks after.
pub const VEX_STARVE_INTERVAL: i32 = 20;
pub const VEX_STARVE_DAMAGE: f32 = 1.0;
/// `VexChargeAttackGoal`: a 1 in `reducedTickDelay(7)` start roll on a
/// target more than 2 blocks away; it aims again inside 3 blocks.
pub const VEX_CHARGE_ROLL: u32 = 7;
pub const VEX_CHARGE_MIN_SQR: f64 = 4.0;
pub const VEX_REAIM_SQR: f64 = 9.0;
/// `VexRandomMoveGoal`: up to three tries at a free cell within
/// 7 x 5 x 7 of the bound origin, flown to at speed 0.25.
pub const VEX_WANDER_SPEED: f64 = 0.25;
/// `VexMoveControl`: `speed * 0.05` of the remaining distance a tick, and
/// half the motion on arrival within the box's mean size.
pub const VEX_ACCELERATION: f64 = 0.05;
pub const VEX_ARRIVE_DAMPING: f64 = 0.5;
/// `EntityTypes.VEX.eyeHeight`.
pub const VEX_EYE: f64 = 0.51875;
/// `AABB.getSize` of the vex's 0.4 x 0.8 x 0.4 box.
pub const VEX_BOX_SIZE: f64 = (0.4 + 0.8 + 0.4) / 3.0;

/// `VexMoveControl.tick` and `LivingEntity.travel` without physics, a tick
/// at a time: steer toward the wanted position, fly through blocks, then
/// air drag (0.91 sideways, 0.98 up and down; vexes have no gravity).
pub fn vex_travel(m: &mut Mob, dt: f64) {
    let Some(s) = m.illager.as_mut() else { return };
    s.move_carry += dt.max(0.0);
    while s.move_carry >= TICK - 1e-9 {
        s.move_carry -= TICK;
        let mut v = m.vel * TICK;
        if let Some((wanted, speed)) = s.wanted {
            let d = wanted - m.pos;
            let len = d.length();
            if len < VEX_BOX_SIZE {
                s.wanted = None;
                v *= VEX_ARRIVE_DAMPING;
            } else {
                v += d * (speed * VEX_ACCELERATION / len);
            }
        }
        m.pos += v;
        m.vel = DVec3::new(v.x * 0.91, v.y * 0.98, v.z * 0.91) / TICK;
    }
    m.on_ground = false;
}

// ---------------------------------------------------------------- ravager

/// `Ravager` timers: a hit holds it still 10 ticks, a shield stuns it 40,
/// and the roar that ends a stun comes 10 ticks into its 20.
pub const RAVAGER_ATTACK_TICKS: i32 = 10;
pub const RAVAGER_STUN_TICKS: i32 = 40;
pub const RAVAGER_ROAR_TICKS: i32 = 20;
pub const RAVAGER_ROAR_AT: i32 = 10;
/// `roar`: 6 damage to every living entity but ravagers and illagers
/// within 4 blocks of its box, and `strongKnockback` to all but players.
pub const RAVAGER_ROAR_DAMAGE: f32 = 6.0;
pub const RAVAGER_ROAR_RADIUS: f64 = 4.0;
/// `strongKnockback`: `push(dx / d² * 4, 0.2, dz / d² * 4)` blocks a tick
/// with `d²` at least 0.001. VoxelCraft caps the sideways push at 4 blocks
/// a tick (the push at one block) so a body inside the ravager isn't flung
/// through walls.
pub const RAVAGER_PUSH: f64 = 4.0;
pub const RAVAGER_PUSH_UP: f64 = 0.2;
pub const RAVAGER_PUSH_MIN_SQR: f64 = 0.001;
pub const RAVAGER_PUSH_CAP: f64 = 4.0;
/// `blockedByItem`: half the time a shield stuns it.
pub const RAVAGER_STUN_CHANCE: f64 = 0.5;
/// `aiStep`: its speed eases a tenth of the way a tick toward 0.35 with a
/// target and 0.3 without; it is 0 while immobile.
pub const RAVAGER_BASE_SPEED: f64 = 0.3;
pub const RAVAGER_ATTACK_SPEED: f64 = 0.35;
pub const RAVAGER_SPEED_LERP: f64 = 0.1;
/// Leaves broken while pushing into them: its box grown this much.
pub const RAVAGER_GRIEF_INFLATE: f64 = 0.2;
/// `getAttackBoundingBox` shrinks 0.05 sideways.
pub const RAVAGER_ATTACK_DEFLATE: f64 = 0.05;
pub const RAVAGER_KNOCKBACK_RESISTANCE: f64 = 0.75;
pub const RAVAGER_ATTACK_KNOCKBACK: f64 = 1.5;
/// Where a rider sits: the passenger attachment (0, 2.2625, -0.0625) less
/// an illager's riding offset of -0.6.
pub const RAVAGER_SEAT: (f64, f64) = (2.2625 - 0.6, -0.0625);

/// `strongKnockback` from a ravager at `from` on something at `to`, in
/// blocks a tick (capped as [`RAVAGER_PUSH_CAP`] says).
pub fn ravager_push(from: DVec3, to: DVec3) -> DVec3 {
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let d2 = (dx * dx + dz * dz).max(RAVAGER_PUSH_MIN_SQR);
    let side = DVec3::new(dx / d2 * RAVAGER_PUSH, 0.0, dz / d2 * RAVAGER_PUSH);
    let len = side.length();
    let side = if len > RAVAGER_PUSH_CAP { side * (RAVAGER_PUSH_CAP / len) } else { side };
    side + DVec3::Y * RAVAGER_PUSH_UP
}

// ---------------------------------------------------------------- crossbow

/// `RangedCrossbowAttackGoal(pillager, 1.0, 8.0)`.
pub const CROSSBOW_RADIUS: f64 = 8.0;
pub const CROSSBOW_SEE_TIME: i32 = 5;
pub const CROSSBOW_DELAY_BASE: u32 = 20;
pub const CROSSBOW_DELAY_SPREAD: u32 = 20;
/// `PATHFINDING_DELAY_RANGE`: one to two seconds.
pub const CROSSBOW_PATH_DELAY: (u32, u32) = (20, 40);
/// `CrossbowItem.MOB_ARROW_POWER`, blocks a tick (a Java `float`).
pub const MOB_ARROW_POWER: f64 = 1.6_f32 as f64;
/// Multishot's `projectile_spread` (degrees).
pub const MULTISHOT_SPREAD: f32 = 10.0;

/// `CrossbowItem.getChargeDuration`: `floor((1.25 - 0.25 * quickCharge) * 20)` ticks.
pub fn charge_ticks(quick_charge: u8) -> u32 {
    ((1.25_f32 - 0.25 * f32::from(quick_charge)) * 20.0).floor().max(0.0) as u32
}

/// `ProjectileWeaponItem.shoot`'s angles (degrees) for `count` projectiles:
/// the first straight, then alternating sides `2 * spread / (count - 1)`
/// apart.
pub fn multishot_angles(count: u32, spread: f32) -> Vec<f32> {
    let step = if count <= 1 { 0.0 } else { 2.0 * spread / (count - 1) as f32 };
    let offset = ((count.max(1) - 1) % 2) as f32 * step / 2.0;
    let mut direction = 1.0;
    (0..count)
        .map(|i| {
            let angle = offset + direction * i.div_ceil(2) as f32 * step;
            direction = -direction;
            angle
        })
        .collect()
}

/// `Monster.rangedAttackUncertainty`: `14 - 4 * difficulty id`.
pub fn uncertainty(difficulty: crate::simulation::difficulty::Difficulty) -> f64 {
    14.0 - 4.0 * f64::from(difficulty as u8)
}

/// `RangedCrossbowAttackGoal.CrossbowState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Crossbow {
    #[default]
    Uncharged,
    Charging,
    Charged,
    ReadyToAttack,
}

// ---------------------------------------------------------------- vindicator

/// `Vindicator.setCustomName`: this name makes it attack everything.
pub const JOHNNY: &str = "Johnny";
/// `BreakDoorGoal.DEFAULT_DOOR_BREAK_TIME` (the goal itself is shared with
/// zombies in `mob::door_break`).
pub const DOOR_BREAK_TIME: u32 = 240;

/// `VindicatorBreakDoorGoal`'s predicate: Normal or Hard, and only in an
/// active raid.
pub fn breaks_doors(difficulty: crate::simulation::difficulty::Difficulty, raid_active: bool) -> bool {
    use crate::simulation::difficulty::Difficulty as D;
    raid_active && matches!(difficulty, D::Normal | D::Hard)
}

/// Sets the Johnny flag when a name tag names a vindicator "Johnny" (it
/// stays set if renamed).
pub fn named(m: &mut Mob) {
    if m.kind == MobKind::Vindicator
        && m.name.as_str() == Some(JOHNNY)
        && let Some(s) = &mut m.illager
    {
        s.johnny = true;
    }
}

// ---------------------------------------------------------------- state

/// What a raid needs from a raider (`Raider` and `PatrollingMonster`): the
/// hook v0.15 Track E attaches raids and patrols to (`entity::raid`).
/// `raid_active` gates the vindicator's door breaking and `celebrating`
/// the celebration pose; the raid tick refreshes both and the goal inputs
/// below every tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Raider {
    /// `Raider.wave` (saved as Java's `Wave`).
    pub wave: i32,
    /// `canJoinRaid`: set when it spawns (`Raider.finalizeSpawn`).
    pub can_join_raid: bool,
    /// The raid it belongs to (`RaidId`), by Track E's raid id.
    pub raid: Option<u32>,
    /// `hasActiveRaid()`; Track E refreshes it every update. Not saved.
    pub raid_active: bool,
    /// `IS_CELEBRATING` (`RaiderCelebration`). Not saved.
    pub celebrating: bool,
    /// `PatrollingMonster`'s `PatrolLeader` and `Patrolling`.
    pub patrol_leader: bool,
    pub patrolling: bool,
    /// `PatrollingMonster.patrolTarget` (saved as `patrol_target`).
    pub patrol_target: Option<IVec3>,
    /// `Raider.ticksOutsideRaid` (not saved, as in Java).
    pub ticks_outside: i32,
    /// The last player to hit it, credited as a hero if it dies in a raid.
    pub last_player: Option<super::PlayerId>,
    /// Its death has been handled by the raid tick.
    pub dead_seen: bool,
    /// Goal inputs from the raid tick: the active raid's centre while it is
    /// undecided, whether it stands in a village, a home to walk through,
    /// the homes it walked to lately (`RaiderMoveThroughVillageGoal`) and an
    /// ominous banner to fetch (`ObtainRaidLeaderBannerGoal`).
    pub raid_center: Option<IVec3>,
    pub in_village: bool,
    pub village_target: Option<IVec3>,
    pub visited: Vec<IVec3>,
    pub banner_target: Option<glam::DVec3>,
    /// `LongDistancePatrolGoal`: other patrollers within 16, the step a
    /// leader just chose for its companions, and the cooldown after a
    /// failed path.
    pub companions: bool,
    pub patrol_step: Option<IVec3>,
    pub patrol_cooldown: u32,
    /// v1.0 Track 6: a witch's `NearestHealableRaiderTargetGoal.cooldown`
    /// in ticks, and the raider it means to heal (`setTarget`) with that
    /// raider's position. Neither is saved, as in Java.
    pub heal_cooldown: i32,
    pub heal_target: Option<(u32, glam::DVec3)>,
}

impl Raider {
    fn save(&self) -> Value {
        let mut v = json!({"wave": self.wave, "can_join_raid": self.can_join_raid, "raid_id": self.raid,
            "patrol_leader": self.patrol_leader, "patrolling": self.patrolling});
        if let Some(t) = self.patrol_target {
            v["patrol_target"] = json!(t.to_array());
        }
        v
    }

    fn load(v: &Value) -> Raider {
        Raider {
            wave: v["wave"].as_i64().map_or(0, |w| w.clamp(0, 1000) as i32),
            can_join_raid: v["can_join_raid"] == true,
            raid: v["raid_id"].as_u64().and_then(|r| u32::try_from(r).ok()),
            patrol_leader: v["patrol_leader"] == true,
            patrolling: v["patrolling"] == true,
            patrol_target: v["patrol_target"].as_array().and_then(|a| {
                let c = |i: usize| a.get(i).and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok());
                Some(IVec3::new(c(0)?, c(1)?, c(2)?))
            }),
            ..Raider::default()
        }
    }
}

/// `AbstractIllager.IllagerArmPose`, which the model draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArmPose {
    #[default]
    Crossed,
    Attacking,
    Spellcasting,
    BowAndArrow,
    CrossbowHold,
    CrossbowCharge,
    Celebrating,
    Neutral,
}

impl ArmPose {
    pub const ALL: [ArmPose; 8] = [
        ArmPose::Crossed,
        ArmPose::Attacking,
        ArmPose::Spellcasting,
        ArmPose::BowAndArrow,
        ArmPose::CrossbowHold,
        ArmPose::CrossbowCharge,
        ArmPose::Celebrating,
        ArmPose::Neutral,
    ];
}

/// Work an illager's goals hand to the entity pass after the mob update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// `EvokerSummonSpellGoal.performSpellCasting`.
    SummonVexes,
    /// A fang where `createSpellEntity` found the ground.
    Fang { pos: DVec3, yaw: f32, warmup: i32 },
    /// `EvokerWololoSpellGoal.performSpellCasting` on this sheep.
    Wololo(u32),
    /// The ravager's `roar`.
    Roar,
    /// The ravager's `blockedByItem` on this player's shield.
    Blocked(super::PlayerId),
    /// Fire Aspect on a mob a weapon hit, for this many seconds.
    Ignite(u32, f32),
    /// `IllusionerBlindnessSpellGoal` on a mob target.
    Blind(u32),
}

/// Illager, vex and ravager state: raid membership, spells, the vex's life
/// and owner, the ravager's timers and the pillager's crossbow.
#[derive(Clone, Debug)]
pub struct State {
    pub raider: Raider,
    /// `tickCount` since it loaded, which spell intervals count.
    pub ticks: u32,
    carry: f64,
    move_carry: f64,
    /// `HurtByTargetGoal`'s mob attacker (players use the shared player
    /// target), and the mob its other target goals picked.
    pub(crate) hurt_by: Option<u32>,
    pub(crate) hunted: Option<u32>,
    /// The mob target and where it stands this tick (`illager_upkeep`).
    pub(crate) foe_at: Option<(u32, DVec3, Shape)>,
    /// `spellCastingTickCount` (saved as `spell_ticks`).
    pub spell_ticks: i32,
    /// The spell being cast (`DATA_SPELL_CASTING_ID`).
    pub spell: Spell,
    /// The running spell goal and its `attackWarmupDelay`.
    pub(crate) casting: Option<(SpellGoal, u32)>,
    /// `nextAttackTickCount` of each of [`spell_goals`].
    pub(crate) next_cast: [u32; 3],
    /// The vexes within 16 blocks and this update's `nextInt(8) + 1`, for
    /// `EvokerSummonSpellGoal.canUse`.
    pub(crate) vexes_seen: u32,
    pub(crate) summon_roll: u32,
    /// `IllusionerBlindnessSpellGoal.lastTargetId`.
    pub(crate) last_blind: Option<Foe>,
    /// The sheep a `wololo` aims at.
    pub(crate) wololo: Option<(u32, DVec3)>,
    /// `Vindicator.isJohnny` (`johnny`).
    pub johnny: bool,
    /// The vex's owner (`owner`), bound origin (`bound_pos`) and limited
    /// life (`life_ticks`).
    pub owner: Option<u32>,
    pub bound: Option<IVec3>,
    pub life: Option<i32>,
    /// `Vex.isCharging`.
    pub charging: bool,
    /// The vex's `MoveControl` wanted position and speed.
    pub(crate) wanted: Option<(DVec3, f64)>,
    /// The ravager's `attackTick`, `stunnedTick` and `roarTick`.
    pub attack_tick: i32,
    pub stun_tick: i32,
    pub roar_tick: i32,
    /// The ravager's movement speed attribute, eased each tick.
    pub ravager_speed: f64,
    /// `RangedCrossbowAttackGoal` and the pillager's `IS_CHARGING_CROSSBOW`.
    pub crossbow: Crossbow,
    pub charging_crossbow: bool,
    pub(crate) see_time: i32,
    pub(crate) attack_delay: u32,
    pub(crate) path_delay: i32,
    pub(crate) use_ticks: u32,
    /// `MeleeAttackGoal.ticksUntilNextAttack` (and the bow goal's timer).
    pub(crate) attack_wait: u32,
    /// `isAggressive`: drawn as the attacking pose.
    pub aggressive: bool,
    pub(crate) actions: Vec<Action>,
    /// v0.15 Track E: where a raid or patrol goal is walking.
    pub(crate) raider_walk: Option<DVec3>,
}

impl Default for State {
    fn default() -> Self {
        State {
            raider: Raider::default(),
            ticks: 0,
            carry: 0.0,
            move_carry: 0.0,
            hurt_by: None,
            hunted: None,
            foe_at: None,
            spell_ticks: 0,
            spell: Spell::None,
            casting: None,
            next_cast: [0; 3],
            vexes_seen: 0,
            summon_roll: 0,
            last_blind: None,
            wololo: None,
            johnny: false,
            owner: None,
            bound: None,
            life: None,
            charging: false,
            wanted: None,
            attack_tick: 0,
            stun_tick: 0,
            roar_tick: 0,
            ravager_speed: RAVAGER_BASE_SPEED,
            crossbow: Crossbow::Uncharged,
            charging_crossbow: false,
            see_time: 0,
            attack_delay: 0,
            path_delay: 0,
            use_ticks: 0,
            attack_wait: 0,
            aggressive: false,
            actions: Vec::new(),
            raider_walk: None,
        }
    }
}

impl State {
    /// State for the kinds this module drives.
    pub fn for_kind(kind: MobKind) -> Option<Box<State>> {
        (is_illager(kind) || matches!(kind, MobKind::Vex | MobKind::Ravager)).then(Box::default)
    }

    /// Whole ticks elapsed by `dt` seconds.
    pub(crate) fn take_ticks(&mut self, dt: f64) -> u32 {
        self.carry += dt.max(0.0);
        let n = ((self.carry + 1e-9) / TICK).floor().max(0.0);
        self.carry -= n * TICK;
        n as u32
    }

    /// `isCastingSpell`.
    pub fn casting_spell(&self) -> bool {
        self.spell_ticks > 0
    }

    /// The ravager's `isImmobile` timers.
    pub fn ravager_busy(&self) -> bool {
        self.attack_tick > 0 || self.stun_tick > 0 || self.roar_tick > 0
    }

    /// The arm pose each kind's `getArmPose` draws.
    pub fn arm_pose(&self, kind: MobKind, main_hand: Option<Item>) -> ArmPose {
        match kind {
            MobKind::Vindicator if self.aggressive => ArmPose::Attacking,
            MobKind::Evoker | MobKind::Illusioner if self.casting_spell() => ArmPose::Spellcasting,
            MobKind::Illusioner if self.aggressive => ArmPose::BowAndArrow,
            MobKind::Pillager if self.charging_crossbow => ArmPose::CrossbowCharge,
            MobKind::Pillager if main_hand.is_some_and(|i| i == Item::CROSSBOW || i == Item::CHARGED_CROSSBOW) => {
                ArmPose::CrossbowHold
            }
            MobKind::Pillager if self.aggressive => ArmPose::Attacking,
            MobKind::Pillager => ArmPose::Neutral,
            _ if self.raider.celebrating => ArmPose::Celebrating,
            _ => ArmPose::Crossed,
        }
    }

    /// The saved fields (Java's names in snake case).
    pub fn save(&self) -> Value {
        let mut v = json!({"raider": self.raider.save(), "spell_ticks": self.spell_ticks});
        if self.johnny {
            v["johnny"] = true.into();
        }
        if let Some(b) = self.bound {
            v["bound_pos"] = json!(b.to_array());
        }
        if let Some(life) = self.life {
            v["life_ticks"] = life.into();
        }
        if let Some(owner) = self.owner {
            v["owner"] = owner.into();
        }
        v["attack_tick"] = self.attack_tick.into();
        v["stun_tick"] = self.stun_tick.into();
        v["roar_tick"] = self.roar_tick.into();
        v
    }

    /// Reads [`State::save`] (defaults for a record without it).
    pub fn load(&mut self, v: &Value) {
        let int = |key: &str, max: i64| v[key].as_i64().map_or(0, |n| n.clamp(0, max) as i32);
        self.raider = Raider::load(&v["raider"]);
        self.spell_ticks = int("spell_ticks", 1000);
        self.johnny = v["johnny"] == true;
        self.bound = v["bound_pos"].as_array().and_then(|a| {
            let c = |i: usize| a.get(i).and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok());
            Some(IVec3::new(c(0)?, c(1)?, c(2)?))
        });
        self.life = v["life_ticks"].as_i64().map(|n| n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
        self.owner = v["owner"].as_u64().and_then(|n| u32::try_from(n).ok());
        self.attack_tick = int("attack_tick", 100);
        self.stun_tick = int("stun_tick", 100);
        self.roar_tick = int("roar_tick", 100);
    }

    /// What LAN clients draw.
    pub fn look(&self) -> Value {
        json!([
            self.spell as u8,
            self.charging_crossbow,
            self.aggressive,
            self.raider.celebrating,
            self.charging,
            self.attack_tick,
            self.stun_tick,
            self.roar_tick,
            self.spell_ticks
        ])
    }

    pub fn apply_look(&mut self, v: &Value) {
        let int = |i: usize| v[i].as_i64().map_or(0, |n| n.clamp(0, 1000) as i32);
        self.spell = Spell::from_id(v[0].as_u64().unwrap_or(0));
        self.charging_crossbow = v[1] == true;
        self.aggressive = v[2] == true;
        self.raider.celebrating = v[3] == true;
        self.charging = v[4] == true;
        self.attack_tick = int(5);
        self.stun_tick = int(6);
        self.roar_tick = int(7);
        self.spell_ticks = int(8);
    }
}

/// `finalizeSpawn` for these kinds: the vindicator's iron axe (outside a
/// raid), the pillager's crossbow (a 1 in 300 Piercing I from
/// `pillager_spawn_crossbow`), the vex's iron sword that never drops and
/// the illusioner's bow; then `populateDefaultEquipmentEnchantments` for all
/// but the illusioner, and `canJoinRaid` for the raiders.
pub fn finalize_spawn(m: &mut Mob, special: f32, rng: &mut Rng) {
    use super::equipment::Slot;
    let hand = match m.kind {
        MobKind::Vindicator => Some(Item::tool(ToolKind::Axe, Tier::Iron)),
        MobKind::Pillager => Some(Item::CROSSBOW),
        MobKind::Vex => Some(Item::tool(ToolKind::Sword, Tier::Iron)),
        MobKind::Illusioner => Some(Item::BOW),
        _ => None,
    };
    let Some(item) = hand else {
        if let Some(s) = &mut m.illager {
            s.raider.can_join_raid = is_raider(m.kind);
        }
        return;
    };
    m.equipment.set(Slot::MainHand, Some(Stack::new(item, 1)));
    if m.kind == MobKind::Vex {
        m.equipment.set_drop_chance(Slot::MainHand, 0.0);
    }
    if m.kind != MobKind::Illusioner {
        m.equipment.enchant_spawned(rng, special);
    }
    if m.kind == MobKind::Pillager
        && rng.next_int(300) == 0
        && let Some(s) = m.equipment.get_mut(Slot::MainHand)
    {
        s.enchants.set(crate::enchant::Enchantment::Piercing, 1);
    }
    if let Some(s) = &mut m.illager {
        s.raider.can_join_raid = is_raider(m.kind);
    }
}

/// The illagers' loot tables: `entities/vindicator` and the evoker's
/// emerald (killed by a player, 0-1 plus Looting's `enchanted_count_increase`),
/// the evoker's totem of undying and the ravager's saddle (always). The vex
/// and illusioner tables have no pools; the pillager's captain bottle
/// waits for raid captains (Track E).
pub fn loot(kind: MobKind, player_kill: bool, looting: u8, rng: &mut Rng, out: &mut Vec<(Item, u8)>) {
    if kind == MobKind::Evoker {
        out.push((Item::TOTEM_OF_UNDYING, 1));
    }
    if kind == MobKind::Ravager {
        out.push((Item::SADDLE, 1));
    }
    if matches!(kind, MobKind::Vindicator | MobKind::Evoker) && player_kill {
        let mut n = rng.next_int(2);
        if looting > 0 {
            n += (f32::from(looting) * rng.next_f32()).round() as u32;
        }
        if n > 0 {
            out.push((Item::EMERALD, n as u8));
        }
    }
}

/// `Entity.isAlliedTo` from `a` toward `b` (no scoreboard teams here):
/// illagers ally `#illager_friends`, and an evoker its own vexes and those
/// of any illager (`considersEntityAsAlly`).
pub fn allied(a: MobKind, b: &Mob, owner_kind: Option<MobKind>) -> bool {
    if is_illager(a) && is_illager(b.kind) {
        return true;
    }
    a == MobKind::Evoker && b.kind == MobKind::Vex && owner_kind.is_some_and(is_illager)
}

/// The mobs these kinds' `NearestAttackableTargetGoal`s pick when no player
/// is about: villagers and wandering traders (`AbstractVillager`, not
/// babies: `AbstractIllager.canAttack`) and iron golems. A Johnny
/// vindicator takes any living mob but its allies.
fn attackable(attacker: &Mob, johnny: bool, m: &Mob, owner_kind: Option<MobKind>) -> bool {
    if !m.alive() || m.uid == attacker.uid {
        return false;
    }
    let villager = matches!(m.kind, MobKind::Villager | MobKind::WanderingTrader) && !m.baby && m.age >= 0;
    match attacker.kind {
        MobKind::Vex => false,
        _ if johnny => !allied(attacker.kind, m, owner_kind) && m.kind != MobKind::Vex || villager,
        _ => villager || m.kind == MobKind::IronGolem,
    }
}

impl Entities {
    /// Per update, before the mobs move: count their ticks, starve vexes
    /// whose life ran out, choose mob targets (villagers and golems; a vex
    /// copies its owner's), and find where each mob target stands.
    pub(super) fn illager_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.illager.is_some()) {
            return;
        }
        let _ = ctx;
        self.ensure_mob_ids();
        for i in 0..self.mobs.len() {
            let m = &mut self.mobs[i];
            if !m.simulation_active || !m.alive() {
                continue;
            }
            let Some(s) = m.illager.as_mut() else { continue };
            let ticks = s.take_ticks(dt);
            // Raid witches only count their ticks; their goals are the witch's.
            if m.kind == MobKind::Witch {
                s.ticks = s.ticks.wrapping_add(ticks);
                continue;
            }
            s.ticks = s.ticks.wrapping_add(ticks);
            // `Vex.tick`: starve once the life runs out, then every second.
            let mut starve = 0;
            if let Some(life) = &mut s.life {
                for _ in 0..ticks {
                    *life -= 1;
                    if *life <= 0 {
                        *life = VEX_STARVE_INTERVAL;
                        starve += 1;
                    }
                }
            }
            for _ in 0..starve {
                if m.damage_bypassing_wolf_armor(VEX_STARVE_DAMAGE, None, &mut self.rng) {
                    events.push(EntityEvent::MobKilled {
                        kind: m.kind,
                        pos: m.pos,
                        burning: false,
                        player_kill: false,
                        looting: 0,
                    });
                    break;
                }
            }
        }
        // Targets among mobs: kinds, positions and owners in one pass.
        let view: Vec<(u32, MobKind, DVec3, Shape, bool)> =
            self.mobs.iter().map(|m| (m.uid, m.kind, m.pos, m.shape(), m.alive() && m.simulation_active)).collect();
        let owner_kind = |uid: Option<u32>, view: &[(u32, MobKind, DVec3, Shape, bool)]| {
            uid.and_then(|u| view.iter().find(|v| v.0 == u && v.4)).map(|v| v.1)
        };
        for i in 0..self.mobs.len() {
            let Some(s) = self.mobs[i].illager.as_ref() else { continue };
            if !self.mobs[i].simulation_active || !self.mobs[i].alive() || self.mobs[i].kind == MobKind::Witch {
                continue;
            }
            let (pos, kind, uid, johnny, owner) =
                (self.mobs[i].pos, self.mobs[i].kind, self.mobs[i].uid, s.johnny, s.owner);
            let range = follow_range(kind);
            // A vex takes its owner's mob target (`VexCopyOwnerTargetGoal`)
            // and player target.
            let mut copied = None;
            let mut owner_player = None;
            if kind == MobKind::Vex
                && let Some(o) = owner.and_then(|o| self.mobs.iter().find(|m| m.uid == o && m.alive()))
            {
                copied = o.illager.as_ref().and_then(|t| t.foe_at.map(|f| f.0));
                owner_player = o.move_ctl.target;
            }
            let full = self.mobs[i].goals.full(uid);
            let s = self.mobs[i].illager.as_ref().unwrap();
            let (hurt_by, mut hunted) = (s.hurt_by, s.hunted);
            if kind == MobKind::Vex {
                hunted = copied;
                if let Some(p) = owner_player
                    && self.mobs[i].move_ctl.target.is_none()
                {
                    self.mobs[i].move_ctl.target = Some(p);
                }
            } else if hunted.is_none() && full && self.rng.next_int(5) == 0 {
                // `NearestAttackableTargetGoal` (`randomInterval` 10): the
                // nearest it can see within its follow range.
                let eye = pos + DVec3::Y * (self.mobs[i].shape().height * 0.85);
                let me = &self.mobs[i];
                hunted = self
                    .mobs
                    .iter()
                    .filter(|m| m.simulation_active && m.pos.distance_squared(pos) <= range * range)
                    .filter(|m| attackable(me, johnny, m, owner_kind(m.illager.as_ref().and_then(|t| t.owner), &view)))
                    .filter(|m| super::mob::line_of_sight(world, eye, m.pos + DVec3::Y * (m.shape().height * 0.85)))
                    .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
                    .map(|m| m.uid);
            }
            let find = |u: Option<u32>| {
                u.and_then(|u| view.iter().find(|v| v.0 == u && v.4))
                    .filter(|v| v.2.distance_squared(pos) <= range * range)
                    .map(|v| (v.0, v.2, v.3))
            };
            let at_hurt = find(hurt_by);
            let at_hunt = find(hunted);
            let s = self.mobs[i].illager.as_mut().unwrap();
            if at_hurt.is_none() {
                s.hurt_by = None;
            }
            s.hunted = at_hunt.map(|f| f.0);
            s.foe_at = at_hurt.or(at_hunt);
            if kind == MobKind::Evoker && full {
                self.evoker_senses(i);
            }
        }
    }

    /// What the evoker's spell goals ask of the world on its selector
    /// update: the vexes near it and this update's summon roll, and, with
    /// nothing to attack, a random blue sheep for `wololo`.
    fn evoker_senses(&mut self, i: usize) {
        let vexes = self.vexes_near(i);
        let roll = self.rng.next_int(SUMMON_ROLL) + 1;
        let m = &self.mobs[i];
        let s = m.illager.as_ref().unwrap();
        let free = s.foe_at.is_none() && m.move_ctl.target.is_none() && !s.casting_spell();
        let keep = s.casting.is_some_and(|(g, _)| g == WOLOLO);
        let pos = m.pos;
        let sheep: Vec<(u32, DVec3)> = if free && !keep && self.villager_griefing {
            let (min, max) = m.aabb();
            let grow = DVec3::new(WOLOLO_RANGE, WOLOLO_BOX_VERTICAL, WOLOLO_RANGE);
            self.mobs
                .iter()
                .filter(|o| o.kind == MobKind::Sheep && o.alive() && o.wool_color == crate::color::DyeColor::Blue)
                .filter(|o| {
                    let (a, b) = o.aabb();
                    a.cmplt(max + grow).all()
                        && b.cmpgt(min - grow).all()
                        && o.pos.distance_squared(pos) <= WOLOLO_RANGE * WOLOLO_RANGE
                })
                .map(|o| (o.uid, o.pos))
                .collect()
        } else {
            Vec::new()
        };
        let pick = (!sheep.is_empty()).then(|| sheep[self.rng.next_int(sheep.len() as u32) as usize]);
        let s = self.mobs[i].illager.as_mut().unwrap();
        s.vexes_seen = vexes;
        s.summon_roll = roll;
        if !keep {
            s.wololo = pick;
        }
    }

    /// `HurtByTargetGoal(Raider).setAlertOthers()` for the mob at `index`
    /// hurt by `foe`: raiders' hits are ignored; otherwise it turns on the
    /// attacker, and so do the mobs of its own kind without a target whose
    /// boxes touch `unitCubeFromLowerCorner(position).inflate(range, 10,
    /// range)`.
    pub(super) fn illager_hurt(&mut self, index: usize, foe: Foe) {
        let Some(m) = self.mobs.get(index).filter(|m| m.illager.is_some() && m.alive()) else { return };
        if let Foe::Mob(uid) = foe
            && self.mobs.iter().any(|a| a.uid == uid && is_raider(a.kind))
        {
            return;
        }
        let (kind, pos) = (m.kind, m.pos);
        let range = follow_range(kind);
        for (j, m) in self.mobs.iter_mut().enumerate() {
            if !m.alive() || m.kind != kind || foe == Foe::Mob(m.uid) {
                continue;
            }
            let free = m.illager.as_ref().is_some_and(|s| s.foe_at.is_none()) && m.move_ctl.target.is_none();
            let d = m.pos - pos;
            let hw = m.shape().half_width;
            let near = d.x + hw > -range
                && d.x - hw < range + 1.0
                && d.z + hw > -range
                && d.z - hw < range + 1.0
                && d.y + m.shape().height > -10.0
                && d.y < 11.0;
            if j != index && !(near && free) {
                continue;
            }
            match foe {
                Foe::Player(player) => {
                    m.hurt_by_target_player(player);
                    m.illager.as_mut().unwrap().hurt_by = None;
                }
                Foe::Mob(uid) => {
                    m.move_ctl.target = None;
                    m.illager.as_mut().unwrap().hurt_by = Some(uid);
                }
            }
        }
    }

    /// After the mobs moved: the actions their goals asked for (summons,
    /// fangs, `wololo`, roars, shield stuns), the fangs' ticks and bites,
    /// spell and stun particles and riders on ravagers.
    pub(super) fn illager_after<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        let _ = world;
        let mut work = Vec::new();
        for (i, m) in self.mobs.iter_mut().enumerate() {
            let (alive, pos, yaw, height) = (m.alive(), m.pos, m.yaw, m.shape().height);
            let Some(s) = &mut m.illager else { continue };
            work.extend(s.actions.drain(..).map(|a| (i, a)));
            if s.casting_spell() && alive {
                let [red, green, blue] = s.spell.colour();
                let side = yaw + std::f32::consts::FRAC_PI_2;
                let side_dir = DVec3::new(f64::from(side.cos()), 0.0, f64::from(side.sin()));
                for side in [-1.0, 1.0] {
                    let at = pos + side_dir * 0.6 * side + DVec3::Y * 1.8;
                    let mut burst = crate::particles::Burst::new(crate::particles::Kind::Effect, at, 1);
                    burst.color = Some([red, green, blue, 1.0]);
                    self.particles.push(crate::particles::Request::Burst(burst));
                }
            }
            if s.stun_tick > 0 && alive && self.rng.next_int(6) == 0 {
                let mut burst =
                    crate::particles::Burst::new(crate::particles::Kind::Effect, pos + DVec3::Y * (height - 0.3), 1);
                burst.spread = DVec3::splat(0.3);
                burst.color = Some([0.498, 0.514, 0.573, 1.0]);
                self.particles.push(crate::particles::Request::Burst(burst));
            }
        }
        for (i, action) in work {
            match action {
                Action::SummonVexes => self.summon_vexes(i),
                Action::Fang { pos, yaw, warmup } => {
                    let owner = self.mobs[i].uid;
                    self.fangs.push(Fang::new(pos, yaw, warmup, owner));
                    // v1.0 Track 9: `createSpellEntity` posts `entity_place`
                    // at the fang, from the evoker.
                    let source = super::warden::mob_source(&self.mobs[i]);
                    self.sculk.post(pos, crate::world::vibration::GameEvent::EntityPlace, source, None);
                }
                Action::Wololo(uid) => {
                    if self.villager_griefing
                        && let Some(sheep) = self.mobs.iter_mut().find(|m| m.uid == uid && m.alive())
                    {
                        sheep.wool_color = crate::color::DyeColor::Red;
                    }
                }
                Action::Roar => self.ravager_roar(i, ctx, events),
                Action::Blocked(player) => self.ravager_blocked(i, player, ctx, events),
                Action::Ignite(uid, secs) => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.uid == uid && m.alive()) {
                        m.ignite(secs);
                    }
                }
                Action::Blind(uid) => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.uid == uid && m.alive()) {
                        m.effects.add(crate::simulation::effects::Effect::Blindness, 0, BLINDNESS_TICKS);
                    }
                }
            }
        }
        self.tick_fangs(dt, ctx, events);
        self.seat_ravager_riders();
    }

    /// `EvokerSummonSpellGoal.performSpellCasting`: three vexes at the
    /// evoker's cell offset up to 2 sideways and 1 up, each bound to its
    /// cell, owned by the evoker and given `20 * (30 + nextInt(90))` ticks.
    fn summon_vexes(&mut self, i: usize) {
        let (cell, owner) = (self.mobs[i].pos.floor().as_ivec3(), self.mobs[i].uid);
        for _ in 0..SUMMON_VEXES {
            let dx = self.rng.next_int(5) as i32 - 2;
            let dz = self.rng.next_int(5) as i32 - 2;
            let at = cell + IVec3::new(dx, 1, dz);
            self.spawn(MobKind::Vex, at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
            // v1.0 Track 9: `EvokerSummonSpellGoal.performSpellCasting`
            // posts `entity_place` at the cell's centre, from the evoker.
            let source = super::warden::mob_source(&self.mobs[i]);
            self.sculk.post(
                at.as_dvec3() + DVec3::splat(0.5),
                crate::world::vibration::GameEvent::EntityPlace,
                source,
                None,
            );
            let life = 20 * (VEX_LIFE_MIN_SECONDS + self.rng.next_int(VEX_LIFE_SPREAD_SECONDS) as i32);
            let vex = self.mobs.last_mut().unwrap();
            vex.persistent = false;
            let s = vex.illager.as_mut().unwrap();
            s.owner = Some(owner);
            s.bound = Some(at);
            s.life = Some(life);
        }
        self.ensure_mob_ids();
    }

    /// The vexes within [`VEX_COUNT_RANGE`] of the mob at `i`, for the
    /// summon roll.
    pub(crate) fn vexes_near(&self, i: usize) -> u32 {
        let (min, max) = self.mobs[i].aabb();
        let grow = DVec3::splat(VEX_COUNT_RANGE);
        let pos = self.mobs[i].pos;
        self.mobs
            .iter()
            .filter(|m| m.kind == MobKind::Vex && m.alive())
            .filter(|m| {
                let (a, b) = m.aabb();
                a.cmplt(max + grow).all()
                    && b.cmpgt(min - grow).all()
                    && m.pos.distance_squared(pos) <= VEX_COUNT_RANGE * VEX_COUNT_RANGE
            })
            .count() as u32
    }

    /// The ravager's `roar`: 6 damage to every living thing but ravagers and
    /// illagers within 4 blocks of its box (players included), and the
    /// strong push to all but ravagers. Armor stands don't count here.
    fn ravager_roar(&mut self, i: usize, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        if !self.mobs[i].alive() {
            return;
        }
        let (min, max) = self.mobs[i].aabb();
        let (from, uid) = (self.mobs[i].pos, self.mobs[i].uid);
        let grow = DVec3::splat(RAVAGER_ROAR_RADIUS);
        let (min, max) = (min - grow, max + grow);
        let touches = |pos: DVec3, shape: Shape| {
            let (a, b) = shape.aabb(pos);
            a.cmplt(max).all() && b.cmpgt(min).all()
        };
        for j in 0..self.mobs.len() {
            let m = &mut self.mobs[j];
            if j == i || !m.alive() || m.kind == MobKind::Ravager || !touches(m.pos, m.shape()) {
                continue;
            }
            if !is_illager(m.kind) {
                let (kind, pos, burning) = (m.kind, m.pos, m.burning);
                if m.damage(RAVAGER_ROAR_DAMAGE, None, &mut self.rng) {
                    let player_kill = m.player_hit_left > 0.0;
                    events.push(EntityEvent::MobKilled { kind, pos, burning, player_kill, looting: 0 });
                }
            }
            m.vel += ravager_push(from, m.pos) * 20.0;
        }
        for t in &ctx.players {
            if t.targetable && touches(t.pos, t.shape) {
                let yaw = (-(t.pos.x - from.x)).atan2(t.pos.z - from.z).to_degrees() as f32;
                events.push(EntityEvent::PlayerHit {
                    player: t.id,
                    damage: RAVAGER_ROAR_DAMAGE,
                    knockback: glam::Vec3::ZERO,
                    java: Some(crate::knockback::Hit::melee(from, t.pos, yaw, 0.0)),
                    cause: RAVAGER_ROAR,
                });
                events.push(EntityEvent::OwnerHurt { player: t.id, attacker: uid });
                events
                    .push(EntityEvent::Shove { player: t.id, velocity: (ravager_push(from, t.pos) * 20.0).as_vec3() });
            }
        }
        let mut b = crate::particles::Burst::new(crate::particles::Kind::Poof, from + DVec3::Y * 1.1, 40);
        b.velocity_spread = DVec3::splat(0.2);
        self.particles.push(crate::particles::Request::Burst(b));
        // v1.0 Track 9: `Ravager.roar` posts `entity_action`.
        self.post_mob_event(i, crate::world::vibration::GameEvent::EntityAction);
    }

    /// `blockedByItem`: unless roaring, half the time the ravager is stunned
    /// for 40 ticks (and the defender pushed), otherwise the defender gets
    /// the strong push.
    fn ravager_blocked(&mut self, i: usize, player: super::PlayerId, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let Some(t) = ctx.players.iter().find(|t| t.id == player) else { return };
        let from = self.mobs[i].pos;
        let Some(s) = self.mobs[i].illager.as_mut() else { return };
        if s.roar_tick != 0 {
            return;
        }
        if f64::from(self.rng.next_f32()) < RAVAGER_STUN_CHANCE {
            s.stun_tick = RAVAGER_STUN_TICKS;
            events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::Stunned), pos: from });
            let away = (t.pos - from).with_y(0.0).normalize_or_zero();
            events.push(EntityEvent::Shove { player, velocity: (away * 1.0).as_vec3() });
        } else {
            events.push(EntityEvent::Shove { player, velocity: (ravager_push(from, t.pos) * 20.0).as_vec3() });
        }
    }

    /// `EvokerFangs.tick` for every fang: after its warmup, a bite 8 ticks
    /// in on every living thing its grown box touches but its owner and the
    /// owner's allies, gone once its 22 ticks are over.
    fn tick_fangs(&mut self, dt: f64, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        if self.fangs.is_empty() {
            return;
        }
        self.fang_carry += dt.max(0.0);
        let ticks = ((self.fang_carry + 1e-9) / TICK).floor();
        self.fang_carry -= ticks * TICK;
        for _ in 0..ticks as u32 {
            let mut k = 0;
            while k < self.fangs.len() {
                let f = &mut self.fangs[k];
                f.warmup -= 1;
                if f.warmup < 0 {
                    if f.warmup == FANG_BITE_AT {
                        let fang = f.clone();
                        self.fang_bite(&fang, ctx, events);
                    }
                    let f = &mut self.fangs[k];
                    if f.warmup == -1 {
                        events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::Fangs), pos: f.pos });
                    }
                    f.life -= 1;
                    if f.life < 0 {
                        self.fangs.swap_remove(k);
                        continue;
                    }
                }
                k += 1;
            }
        }
    }

    fn fang_bite(&mut self, fang: &Fang, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let (min, max) = fang.bite_box();
        let touches = |pos: DVec3, shape: Shape| {
            let (a, b) = shape.aabb(pos);
            a.cmplt(max).all() && b.cmpgt(min).all()
        };
        let owner = self.mobs.iter().find(|m| m.uid == fang.owner && m.alive()).map(|m| (m.uid, m.kind));
        let owner_kinds: Vec<(u32, MobKind)> = self.mobs.iter().map(|m| (m.uid, m.kind)).collect();
        for m in &mut self.mobs {
            if !m.alive() || !touches(m.pos, m.shape()) || owner.is_some_and(|(u, _)| u == m.uid) {
                continue;
            }
            if let Some((_, kind)) = owner {
                let vex_owner = m
                    .illager
                    .as_ref()
                    .and_then(|s| s.owner)
                    .and_then(|o| owner_kinds.iter().find(|(u, _)| *u == o).map(|(_, k)| *k));
                if allied(kind, m, vex_owner) {
                    continue;
                }
            }
            let (kind, pos, burning) = (m.kind, m.pos, m.burning);
            if m.damage_bypassing_wolf_armor(FANG_DAMAGE, None, &mut self.rng) {
                let player_kill = m.player_hit_left > 0.0;
                events.push(EntityEvent::MobKilled { kind, pos, burning, player_kill, looting: 0 });
            }
        }
        for t in &ctx.players {
            if t.targetable && touches(t.pos, t.shape) {
                events.push(EntityEvent::PlayerHit {
                    player: t.id,
                    damage: FANG_DAMAGE,
                    knockback: glam::Vec3::ZERO,
                    java: None,
                    cause: FANGS_CAUSE,
                });
            }
        }
    }

    /// Seats an illager on the ravager `vehicle` (both by uid): the
    /// ravager's passenger seat, which raids (Track E) and commands use.
    /// Returns whether it sat down.
    pub fn mount_ravager(&mut self, rider: u32, vehicle: u32) -> bool {
        let ok_vehicle = self.mobs.iter().any(|m| m.uid == vehicle && m.kind == MobKind::Ravager && m.alive());
        let taken = self.mobs.iter().any(|m| m.riding == Some(super::mounts::MOB_VEHICLE | vehicle) && m.alive());
        let Some(r) = self.mobs.iter_mut().find(|m| m.uid == rider && m.alive()) else { return false };
        if !ok_vehicle || taken || !is_raider(r.kind) || r.riding.is_some() {
            return false;
        }
        r.riding = Some(super::mounts::MOB_VEHICLE | vehicle);
        true
    }

    /// Keeps ravager riders on their seats.
    fn seat_ravager_riders(&mut self) {
        let seats: Vec<(u32, DVec3, DVec3, f32)> = self
            .mobs
            .iter()
            .filter(|m| m.kind == MobKind::Ravager && m.alive())
            .map(|m| (m.uid, m.pos, m.vel, m.yaw))
            .collect();
        if seats.is_empty() {
            return;
        }
        for m in &mut self.mobs {
            let Some(vehicle) = m.riding.filter(|v| v & super::mounts::MOB_VEHICLE != 0) else { continue };
            let Some(&(_, pos, vel, yaw)) = seats.iter().find(|s| s.0 == vehicle & !super::mounts::MOB_VEHICLE) else {
                continue;
            };
            m.previous_pos = m.pos;
            let back = DVec3::new(f64::from(yaw.cos()), 0.0, f64::from(yaw.sin())) * RAVAGER_SEAT.1;
            m.pos = pos + DVec3::Y * RAVAGER_SEAT.0 + back;
            m.vel = vel;
        }
    }
}

#[cfg(test)]
mod tests;
