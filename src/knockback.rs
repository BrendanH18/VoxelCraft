//! Java 26.3 knockback and sweep-attack rules, in Java's units.
//!
//! Motion is in blocks per tick, as in `LivingEntity.knockback`; convert with
//! [`TICK_RATE`] where VoxelCraft keeps blocks per second. The constants and
//! formulas were read from `LivingEntity`, `Player`, `Mob`, `AbstractArrow`,
//! `Projectile`, `Mth` and the knockback/punch/sweeping-edge enchantment data,
//! and are pinned by `tests/java_knockback.rs` against `combat.json`.
use glam::DVec3;

/// Fixed ticks per second, to convert Java's blocks per tick to blocks per second.
pub const TICK_RATE: f64 = 20.0;
/// `LivingEntity.dealDefaultKnockback`'s `0.4f`, widened to a double.
pub const DEFAULT_STRENGTH: f64 = 0.4000000059604645;
/// `knockback` halves the target's existing horizontal motion (and the vertical on the ground).
pub const MOTION_DIVISOR: f64 = 2.0;
/// The most upward motion a grounded target gains.
pub const VERTICAL_CAP: f64 = 0.4;
/// Below this squared length the push direction is replaced by a random jitter (`1.0E-5f`).
pub const ZERO_DIRECTION: f64 = 9.999999747378752e-6;
/// Scale of the random jitter used for a zero-length push direction.
pub const JITTER_SCALE: f64 = 0.01;
/// Extra attack knockback a hit while sprinting adds.
pub const SPRINT_BONUS: f32 = 0.5;
/// `causeExtraKnockback` slows the attacker's horizontal motion to this share.
pub const ATTACKER_SLOWDOWN: f64 = 0.6;
/// Radians per degree as a float, as `Player.causeExtraKnockback` multiplies it.
pub const DEGREES_TO_RADIANS: f32 = 0.017453292;
/// Per-level Punch push of an arrow (`AbstractArrow.doKnockback`).
pub const ARROW_PUSH_PER_LEVEL: f64 = 0.6;
/// The upward push that accompanies a Punch shove.
pub const ARROW_PUSH_LIFT: f64 = 0.1;
/// A sweeping hit lands on the entities near the target: the target's box grown by this, and within `SWEEP_RANGE_SQR` of the attacker.
pub const SWEEP_INFLATE: DVec3 = DVec3::new(1.0, 0.25, 1.0);
pub const SWEEP_RANGE_SQR: f64 = 9.0;
/// The attacker's horizontal movement per tick must stay below this many times its movement speed.
pub const SWEEP_SPEED_FACTOR: f64 = 2.5;
/// `Player.createAttributes`' base movement speed (`0.1f`).
pub const PLAYER_SPEED: f64 = 0.10000000149011612;
/// `Speed` and `Slowness` multiply the movement speed by `1 + amount * level` each (`ADD_MULTIPLIED_TOTAL`).
pub const SPEED_PER_LEVEL: f64 = 0.20000000298023224;
pub const SLOWNESS_PER_LEVEL: f64 = -0.15000000596046448;
/// Strength of the strike a sweep adds to each swept victim.
pub const SWEEP_STRENGTH: f64 = DEFAULT_STRENGTH;

/// A call to `LivingEntity.knockback(strength, x, z)`: `(x, z)` points from the
/// victim toward the source, so the victim moves the opposite way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Push {
    pub strength: f64,
    pub x: f64,
    pub z: f64,
}

impl Push {
    /// The 0.4 knockback `hurtServer` deals, away from a source at `source` (feet position) for a victim at `victim`.
    pub fn hurt(source: DVec3, victim: DVec3) -> Self {
        Self { strength: DEFAULT_STRENGTH, x: source.x - victim.x, z: source.z - victim.z }
    }

    /// The knockback along the attacker's facing (`causeExtraKnockback`, sweeps): `(sin(yaw), -cos(yaw))`.
    pub fn facing(strength: f64, yaw_degrees: f32) -> Self {
        let (x, z) = facing_vector(yaw_degrees);
        Self { strength, x, z }
    }

    /// The knockback from a projectile moving with `motion`: pushed along its horizontal motion.
    pub fn projectile(motion: DVec3) -> Self {
        Self { strength: DEFAULT_STRENGTH, x: -motion.x, z: -motion.z }
    }
}

/// `Vec3.normalize`: zero below a length of `1.0E-5f` rather than only for an exactly zero vector.
pub fn java_normalize(v: DVec3) -> DVec3 {
    let length = v.length();
    if length < ZERO_DIRECTION { DVec3::ZERO } else { v / length }
}

/// `Mth.sin(double)`: a 65536-entry table of `(float) Math.sin(i / 10430.378350470453)`.
pub fn mth_sin(x: f64) -> f32 {
    const SCALE: f64 = 10430.378350470453;
    (((x * SCALE) as i64 & 65535) as f64 / SCALE).sin() as f32
}

/// `Mth.cos(double)`: the same table a quarter turn on.
pub fn mth_cos(x: f64) -> f32 {
    const SCALE: f64 = 10430.378350470453;
    (((x * SCALE + 16384.0) as i64 & 65535) as f64 / SCALE).sin() as f32
}

/// `(sin(yaw), -cos(yaw))` for a Java yaw in degrees, as `Player.causeExtraKnockback` computes it.
pub fn facing_vector(yaw_degrees: f32) -> (f64, f64) {
    let radians = f64::from(yaw_degrees * DEGREES_TO_RADIANS);
    (f64::from(mth_sin(radians)), -f64::from(mth_cos(radians)))
}

/// A VoxelCraft yaw (radians, 0 along +X) as Java's yaw in degrees (0 faces +Z).
pub fn java_yaw_degrees(yaw: f32) -> f32 {
    yaw.to_degrees() - 90.0
}

/// `LivingEntity.knockback`: the new motion (blocks per tick) of a target with
/// `old` motion, or `None` when its resistance cancels the push. `jitter` supplies
/// `nextDouble()` for a push whose direction has no length.
pub fn apply(
    old: DVec3,
    push: Push,
    resistance: f64,
    on_ground: bool,
    jitter: &mut dyn FnMut() -> f64,
) -> Option<DVec3> {
    let strength = push.strength * (1.0 - resistance);
    if strength <= 0.0 {
        return None;
    }
    let (mut x, mut z) = (push.x, push.z);
    while x * x + z * z < ZERO_DIRECTION {
        x = (jitter() - jitter()) * JITTER_SCALE;
        z = (jitter() - jitter()) * JITTER_SCALE;
    }
    let away = java_normalize(DVec3::new(x, 0.0, z)) * strength;
    let y = if on_ground { VERTICAL_CAP.min(old.y / MOTION_DIVISOR + strength) } else { old.y };
    Some(DVec3::new(old.x / MOTION_DIVISOR - away.x, y, old.z / MOTION_DIVISOR - away.z))
}

/// `Player.getKnockback` for a player or mob: `(attack_knockback + enchantment level) / 2`,
/// plus the sprint bonus for a sprinting, strong player attack. The enchantment's
/// per-level value is 1 (`linear base 1, +1 per level`).
pub fn attack_strength(attack_knockback: f64, enchantment_level: u32, sprint_knockback: bool) -> f32 {
    let base = (attack_knockback as f32 + enchantment_level as f32) / 2.0;
    base + if sprint_knockback { SPRINT_BONUS } else { 0.0 }
}

/// What `causeExtraKnockback` does to the attacker: horizontal motion times 0.6.
pub fn slow_attacker(motion: DVec3) -> DVec3 {
    DVec3::new(motion.x * ATTACKER_SLOWDOWN, motion.y, motion.z * ATTACKER_SLOWDOWN)
}

/// Punch's extra push of an arrow (`AbstractArrow.doKnockback`), added to the target's motion.
pub fn arrow_push(punch_level: u32, resistance: f64, arrow_motion: DVec3) -> Option<DVec3> {
    let strength = f64::from(punch_level as f32);
    if strength <= 0.0 {
        return None;
    }
    let horizontal = java_normalize(DVec3::new(arrow_motion.x, 0.0, arrow_motion.z));
    let push = horizontal * (strength * ARROW_PUSH_PER_LEVEL * (1.0 - resistance).max(0.0));
    (push.length_squared() > 0.0).then_some(DVec3::new(push.x, ARROW_PUSH_LIFT, push.z))
}

/// The player's movement-speed attribute (`Player.getSpeed`) with `Speed` and
/// `Slowness` levels (0 when absent), and the sprinting modifier when `sprinting`.
pub fn movement_speed(speed_level: u32, slowness_level: u32, sprinting: bool) -> f32 {
    let mut value = PLAYER_SPEED;
    if sprinting {
        value *= 1.0 + 0.30000001192092896;
    }
    if speed_level > 0 {
        value *= 1.0 + SPEED_PER_LEVEL * f64::from(speed_level);
    }
    if slowness_level > 0 {
        value *= 1.0 + SLOWNESS_PER_LEVEL * f64::from(slowness_level);
    }
    value.clamp(0.0, 1024.0) as f32
}

/// `Player.isSweepAttack`, given Java's three flags (strong attack, critical, sprint knockback).
pub fn sweep_allowed(
    strong: bool,
    critical: bool,
    sprint_knockback: bool,
    on_ground: bool,
    horizontal_distance_sqr: f64,
    speed: f32,
    holding_sword: bool,
) -> bool {
    // `Mth.square(getSpeed() * 2.5)`, a plain product.
    let limit = f64::from(speed) * SWEEP_SPEED_FACTOR;
    strong && !critical && !sprint_knockback && on_ground && horizontal_distance_sqr < limit * limit && holding_sword
}

/// A sweeping-edge enchantment's `sweeping_damage_ratio`: `level / (level + 1)`.
pub fn sweeping_ratio(level: u32) -> f32 {
    if level == 0 { 0.0 } else { (level as f64 / (level as f64 + 1.0)) as f32 }
}

/// A swept victim's damage: `(1 + ratio * damage) + the victim's enchantment bonus`, times the attack strength.
pub fn sweep_damage(scaled_damage: f32, sweeping_level: u32, victim_bonus: f32, attack_strength: f32) -> f32 {
    (1.0 + sweeping_ratio(sweeping_level) * scaled_damage + victim_bonus) * attack_strength
}

/// Whether the box `victim` (min, max) overlaps `target` grown by [`SWEEP_INFLATE`], strictly as `AABB.intersects`.
pub fn in_sweep_box(target: (DVec3, DVec3), victim: (DVec3, DVec3)) -> bool {
    let (lo, hi) = (target.0 - SWEEP_INFLATE, target.1 + SWEEP_INFLATE);
    lo.x < victim.1.x
        && hi.x > victim.0.x
        && lo.y < victim.1.y
        && hi.y > victim.0.y
        && lo.z < victim.1.z
        && hi.z > victim.0.z
}

/// The sounds `Player.attack` plays, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackSound {
    Knockback,
    Crit,
    Sweep,
    Strong,
    Weak,
    NoDamage,
}

/// `Player.attack`'s sounds: the knockback sound as soon as a strong attack is
/// made while sprinting, then, if the target was hurt, the critical sound,
/// the sweep sound, and the strong or weak one only when neither applied.
pub fn attack_sounds(strong: bool, sprinting: bool, critical: bool, sweep: bool, hurt: bool) -> Vec<AttackSound> {
    let mut sounds = Vec::new();
    if sprinting && strong {
        sounds.push(AttackSound::Knockback);
    }
    if !hurt {
        sounds.push(AttackSound::NoDamage);
        return sounds;
    }
    if critical {
        sounds.push(AttackSound::Crit);
    }
    if sweep {
        sounds.push(AttackSound::Sweep);
    }
    if !critical && !sweep {
        sounds.push(if strong { AttackSound::Strong } else { AttackSound::Weak });
    }
    sounds
}

/// `HoglinBase.throwTarget`: a hoglin's tusk toss, added to the victim's motion
/// (not a `knockback` call), scaled by its `attack_knockback` less the victim's resistance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throw {
    /// The thrower's `attack_knockback` attribute.
    pub strength: f64,
    /// Victim position minus the thrower's, horizontally.
    pub dx: f64,
    pub dz: f64,
    /// `nextInt(21) - 10`, passed to `Vec3.yRot` as radians (Java does not convert it from degrees).
    pub turn: f32,
    /// `nextFloat()` scaling the horizontal toss from 0.2 to 0.7.
    pub reach: f32,
    /// `nextFloat()` scaling the lift from 0 to 0.5.
    pub lift: f32,
}

impl Throw {
    /// The motion added (blocks per tick), or `None` when resistance cancels the toss.
    pub fn push(&self, resistance: f64) -> Option<DVec3> {
        let strength = self.strength - resistance;
        if strength <= 0.0 {
            return None;
        }
        let scale = f64::from(self.reach * 0.5 + 0.2) * strength;
        let away = java_normalize(DVec3::new(self.dx, 0.0, self.dz)) * scale;
        let (sin, cos) = (f64::from(mth_sin(f64::from(self.turn))), f64::from(mth_cos(f64::from(self.turn))));
        let x = away.x * cos + away.z * sin;
        let z = away.z * cos - away.x * sin;
        Some(DVec3::new(x, strength * f64::from(self.lift) * 0.5, z))
    }
}

/// What one Java hit delivers to a victim's motion, in order: `LivingEntity.knockback`
/// calls (the default hurt push, then the attacker's extra one), then an arrow's
/// Punch shove added on top.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hit {
    pub pushes: [Option<Push>; 2],
    /// Punch level and the arrow's motion in blocks per tick (`AbstractArrow.doKnockback`).
    pub punch: Option<(u32, DVec3)>,
    /// A hoglin's tusk toss.
    pub throw: Option<Throw>,
}

impl Hit {
    /// A melee hit by an attacker at `source` (feet) facing Java `yaw` degrees on a victim at `victim`:
    /// `hurtServer`'s push, then `extra` (attack knockback) if positive.
    pub fn melee(source: DVec3, victim: DVec3, yaw: f32, extra: f32) -> Self {
        Self {
            pushes: [Some(Push::hurt(source, victim)), (extra > 0.0).then(|| Push::facing(f64::from(extra), yaw))],
            punch: None,
            throw: None,
        }
    }

    /// An arrow flying with `motion` (blocks per tick) shot from a weapon with Punch `level`.
    pub fn arrow(motion: DVec3, level: u32) -> Self {
        Self {
            pushes: [Some(Push::projectile(motion)), None],
            punch: (level > 0).then_some((level, motion)),
            throw: None,
        }
    }

    /// This hit without `hurtServer`'s default push, as a stronger hit inside
    /// the victim's immunity window lands: the attacker's extra knockback, Punch
    /// and a hoglin toss still apply.
    pub fn without_default(self) -> Self {
        Self { pushes: [None, self.pushes[1]], ..self }
    }

    /// The first push's direction of motion, for hurt-direction displays.
    pub fn away(&self) -> Option<DVec3> {
        self.pushes[0].map(|p| DVec3::new(-p.x, 0.0, -p.z))
    }

    /// The victim's new motion (blocks per tick) after the whole hit.
    pub fn apply(&self, mut motion: DVec3, resistance: f64, on_ground: bool, jitter: &mut dyn FnMut() -> f64) -> DVec3 {
        for push in self.pushes.into_iter().flatten() {
            if let Some(next) = apply(motion, push, resistance, on_ground, jitter) {
                motion = next;
            }
        }
        if let Some((level, arrow)) = self.punch
            && let Some(shove) = arrow_push(level, resistance, arrow)
        {
            motion += shove;
        }
        if let Some(shove) = self.throw.and_then(|throw| throw.push(resistance)) {
            motion += shove;
        }
        motion
    }
}
