//! v1.0 gap Track 7: the client-side motion Java 26.3 draws every mob
//! with, in ticks as Java counts them. Checked against the 26.3 client:
//! `WalkAnimationState`, `LivingEntity.calculateEntityAnimation` and
//! `updateWalkAnimation` (and the camel's, frog's and creaking's
//! overrides), `LivingEntity.SwingState`, `SwingAnimation.DEFAULT`,
//! `LivingEntityRenderer.setupRotations` and `getFlipDegrees` (and the
//! spider's, silverfish's and endermite's overrides),
//! `LivingEntityRenderer.extractRenderState`, `OverlayTexture` and
//! `Mth.triangleWave`. The record is
//! `docs/verification/2026-11-27-v1.0-gap-track-07-art-motion.md`.
//!
//! A mob keeps Java's walk state in [`super::Mob::limb_phase`] (Java's
//! `position`, before the baby scale) and [`super::Mob::limb_amp`]
//! (`speed`); [`super::Mob::render_pose`] turns them into the values a
//! model reads, `walkAnimationPos` and `walkAnimationSpeed`.

use super::MobKind;

/// Seconds in one game tick.
pub const TICK: f32 = 0.05;
/// `updateWalkAnimation`: how far the walk speed moves toward its target
/// each tick (the camel's is 0.2).
pub const WALK_LERP: f32 = 0.4;
/// `WalkAnimationState.update`'s position scale for babies: their legs
/// cycle three times as fast.
pub const BABY_WALK_SCALE: f32 = 3.0;
/// The walk cycle's frequency every limb model uses: `cos(pos * 0.6662)`.
pub const WALK_FREQ: f32 = 0.6662;
/// The usual leg swing: `cos(pos * 0.6662) * 1.4 * speed`.
pub const LEG_SWING: f32 = 1.4;
/// `LivingEntity.handleDamageEvent`: `walkAnimation.setSpeed(1.5F)`, the
/// kick a hit gives the legs.
pub const HURT_KICK: f32 = 1.5;
/// `SwingAnimation.DEFAULT`: an arm swing lasts 6 ticks.
pub const SWING_TICKS: u8 = 6;
/// `LivingEntity.tickDeath` removes a mob when `deathTime` reaches 20.
pub const DEATH_TICKS: f32 = 20.0;

/// How the walk speed's target follows the distance moved in a tick.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WalkRule {
    /// `distance * gain`, capped at `cap`.
    pub gain: f32,
    pub cap: f32,
    pub lerp: f32,
    /// `omnidirectionalAirMover`: count the vertical distance too.
    pub with_y: bool,
}

/// `LivingEntity.updateWalkAnimation` and its overrides.
pub fn walk_rule(kind: MobKind) -> WalkRule {
    let plain = WalkRule { gain: 4.0, cap: 1.0, lerp: WALK_LERP, with_y: false };
    match kind {
        MobKind::Camel | MobKind::CamelHusk => WalkRule { gain: 6.0, lerp: 0.2, ..plain },
        MobKind::Frog => WalkRule { gain: 25.0, ..plain },
        MobKind::Creaking => WalkRule { gain: 25.0, cap: 3.0, ..plain },
        // `Bee` and `Parrot` return true from `omnidirectionalAirMover`
        // (the sulfur cube only while carrying a block).
        MobKind::Bee | MobKind::Parrot => WalkRule { with_y: true, ..plain },
        _ => plain,
    }
}

/// One tick of `WalkAnimationState.update`, after moving `distance`
/// blocks: the speed eases toward its target and the position advances by
/// the new speed.
pub fn walk_step(position: &mut f32, speed: &mut f32, distance: f32, rule: WalkRule) {
    let target = (distance * rule.gain).min(rule.cap);
    *speed += (target - *speed) * rule.lerp;
    *position += *speed;
}

/// `WalkAnimationState.position(partial)` and `speed(partial)`, as
/// `LivingEntityRenderer.extractRenderState` reads them: the position
/// runs back by the unfinished part of the tick's speed and is scaled
/// (three times for babies); the speed is lerped and capped at 1.
pub fn walk_render(position: f32, old_speed: f32, speed: f32, alpha: f32, scale: f32) -> (f32, f32) {
    ((position - speed * (1.0 - alpha)) * scale, (old_speed + (speed - old_speed) * alpha).min(1.0))
}

/// A limb's swing: `cos(pos * 0.6662 [+ pi]) * amplitude * speed`.
pub fn limb(pos: f32, speed: f32, amplitude: f32, opposite: bool) -> f32 {
    let phase = pos * WALK_FREQ + if opposite { std::f32::consts::PI } else { 0.0 };
    phase.cos() * amplitude * speed
}

/// A keyframe walk: the looping animation's length in seconds and the
/// factors its model passes to `KeyframeAnimation.applyWalk`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct KeyframeWalk {
    pub length: f32,
    pub speed_factor: f32,
    pub scale_factor: f32,
}

/// `ArmadilloModel`: `ARMADILLO_WALK` (1.4583 s), `applyWalk(.., 16.5, 2.5)`.
pub const ARMADILLO_WALK: KeyframeWalk = KeyframeWalk { length: 1.4583, speed_factor: 16.5, scale_factor: 2.5 };
/// `SnifferModel`: `SNIFFER_WALK` (2 s), `applyWalk(.., 9, 100)`.
pub const SNIFFER_WALK: KeyframeWalk = KeyframeWalk { length: 2.0, speed_factor: 9.0, scale_factor: 100.0 };
/// `CopperGolemModel`: `COPPER_GOLEM_WALK` (0.8333 s), `applyWalk(.., 2, 2.5)`.
pub const COPPER_GOLEM_WALK: KeyframeWalk = KeyframeWalk { length: 0.8333, speed_factor: 2.0, scale_factor: 2.5 };
/// `CamelModel`: `CAMEL_WALK` (1.5 s), `applyWalk(.., 2, 2.5)`.
pub const CAMEL_WALK: KeyframeWalk = KeyframeWalk { length: 1.5, speed_factor: 2.0, scale_factor: 2.5 };
/// `FrogModel`: `FROG_WALK` (1.25 s), `applyWalk(.., 1.5, 2.5)`.
pub const FROG_WALK: KeyframeWalk = KeyframeWalk { length: 1.25, speed_factor: 1.5, scale_factor: 2.5 };
/// `FrogModel` in water: `FROG_SWIM` (1.04167 s), `applyWalk(.., 1, 2.5)`.
pub const FROG_SWIM: KeyframeWalk = KeyframeWalk { length: 1.04167, speed_factor: 1.0, scale_factor: 2.5 };
/// `CreakingModel`: `CREAKING_WALK` (1.125 s), `applyWalk(.., 1, 1)`.
pub const CREAKING_WALK: KeyframeWalk = KeyframeWalk { length: 1.125, speed_factor: 1.0, scale_factor: 1.0 };

/// `KeyframeAnimation.applyWalk`: how far through its loop (0..1) a
/// keyframe walk is at walk position `pos` (`pos * 50 * speedFactor`
/// whole milliseconds), and how strongly it plays
/// (`min(speed * scaleFactor, 1)`). This game's models draw their own
/// limbs on that cycle; Java's keyframes aren't copied.
pub fn keyframe_walk(pos: f32, speed: f32, k: KeyframeWalk) -> (f32, f32) {
    let ms = (pos * 50.0 * k.speed_factor).trunc();
    ((ms / 1000.0 / k.length).rem_euclid(1.0), (speed * k.scale_factor).min(1.0))
}

/// [`keyframe_walk`] as an angle round the loop, and the strength.
pub fn keyframe_phase(pos: f32, speed: f32, k: KeyframeWalk) -> (f32, f32) {
    let (t, scale) = keyframe_walk(pos, speed, k);
    (t * std::f32::consts::TAU, scale)
}

/// `Mth.triangleWave`: a wave of `period` between -1 and 1.
pub fn triangle_wave(x: f32, period: f32) -> f32 {
    ((x % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

/// `LivingEntityRenderer.setupRotations`: how far over (0 upright, 1
/// fully) a dying mob is, `deathTime` ticks (with the partial tick) after
/// it died: `sqrt((deathTime - 1) / 20 * 1.6)`, capped at 1, so it lands
/// 13.5 ticks in.
pub fn death_fall(death_ticks: f32) -> f32 {
    if death_ticks <= 0.0 { 0.0 } else { ((death_ticks - 1.0) / 20.0 * 1.6).max(0.0).sqrt().min(1.0) }
}

/// `getFlipDegrees`, in radians: dying mobs fall onto their side, and
/// spiders, silverfish and endermites onto their backs.
pub fn flip(kind: MobKind) -> f32 {
    match kind {
        MobKind::Spider | MobKind::CaveSpider | MobKind::Silverfish | MobKind::Endermite => std::f32::consts::PI,
        _ => std::f32::consts::FRAC_PI_2,
    }
}

/// `LivingEntity.SwingState`: an arm swing's progress, 0 to 1 over its
/// duration. Not saved, as in Java.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Swing {
    /// The swing under way: its duration in ticks.
    current: Option<u8>,
    ticks: u8,
    /// `oldAnimation` and `animation`.
    pub old: f32,
    pub anim: f32,
}

impl Swing {
    /// `startIfAble`: a swing can't restart in its first half.
    pub fn start_if_able(&mut self, duration: u8) {
        if self.current.is_some_and(|d| self.ticks <= d / 2 && self.ticks > 0) {
            return;
        }
        if self.current != Some(duration) {
            (self.old, self.anim) = (0.0, 0.0);
        }
        (self.current, self.ticks) = (Some(duration), 0);
    }

    /// `tick`, once a game tick.
    pub fn tick(&mut self) {
        self.old = self.anim;
        match self.current {
            Some(d) => {
                if d > 0 {
                    self.anim = (f32::from(self.ticks) / f32::from(d)).min(1.0);
                }
                let done = self.ticks > d;
                self.ticks = self.ticks.saturating_add(1);
                if done {
                    self.current = None;
                    self.anim = 0.0;
                }
            }
            None => self.anim = 0.0,
        }
    }

    pub fn swinging(&self) -> bool {
        self.current.is_some()
    }
}

/// `SwingState.getAnimation(partial)`: lerped, and across a restart lerped
/// on toward 1 rather than back.
pub fn swing_render(old: f32, anim: f32, alpha: f32) -> f32 {
    let to = if anim >= old { anim } else { anim + 1.0 };
    old + (to - old) * alpha
}

/// `AnimationUtils.animateZombieArms`: both arms' (x, y) rotation in
/// Java's model space, for a zombie raising its arms (`raised`) and its
/// swing.
pub fn zombie_arms(aggressive: bool, raised: bool, swing: f32) -> (f32, f32) {
    use std::f32::consts::PI;
    let drop = if raised { -PI / if aggressive { 1.5 } else { 2.25 } } else { 0.0 };
    let y_mod = if raised { 1.0 } else { -1.0 } * (swing * PI).sin();
    let x_mod = ((1.0 - (1.0 - swing) * (1.0 - swing)) * PI).sin();
    (drop + y_mod * 1.2 - x_mod * 0.4, 0.1 - y_mod * 0.6)
}

/// `AnimationUtils.swingWeaponDown`: the weapon arm's and the other arm's
/// x rotation (before the idle bob) for an armed illager or piglin.
pub fn weapon_down(swing: f32, age_ticks: f32) -> (f32, f32) {
    use std::f32::consts::PI;
    let a2 = (swing * PI).sin();
    let a = ((1.0 - (1.0 - swing) * (1.0 - swing)) * PI).sin();
    (
        -1.884_955_8 + (age_ticks * 0.09).cos() * 0.15 + a2 * 2.2 - a * 0.4,
        (age_ticks * 0.19).cos() * 0.5 + a2 * 1.2 - a * 0.4,
    )
}

/// `AnimationUtils.bobModelPart`'s x rotation: the idle sway of a
/// humanoid's arms (`scale` 1 for the right arm, -1 the left).
pub fn arm_bob(age_ticks: f32, scale: f32) -> f32 {
    scale * (age_ticks * 0.067).sin() * 0.05
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    /// `WalkAnimationState.update(min(d * 4, 1), 0.4, scale)`, tick by tick:
    /// a mob walking 0.1 blocks a tick eases toward speed 0.4.
    #[test]
    fn walk_speed_eases_toward_four_times_the_distance() {
        let rule = walk_rule(MobKind::Zombie);
        let (mut pos, mut speed) = (0.0, 0.0);
        walk_step(&mut pos, &mut speed, 0.1, rule);
        assert!(close(speed, 0.16) && close(pos, 0.16), "{speed} {pos}");
        walk_step(&mut pos, &mut speed, 0.1, rule);
        assert!(close(speed, 0.256) && close(pos, 0.416), "{speed} {pos}");
        for _ in 0..60 {
            walk_step(&mut pos, &mut speed, 0.1, rule);
        }
        assert!(close(speed, 0.4), "{speed}");
        // Running 0.3 blocks a tick saturates at 1.
        for _ in 0..60 {
            walk_step(&mut pos, &mut speed, 0.3, rule);
        }
        assert!(close(speed, 1.0), "{speed}");
        // Standing still, the speed decays by 0.6 a tick.
        walk_step(&mut pos, &mut speed, 0.0, rule);
        assert!(close(speed, 0.6), "{speed}");
    }

    #[test]
    fn overrides_follow_their_mobs() {
        let camel = walk_rule(MobKind::Camel);
        assert_eq!((camel.gain, camel.cap, camel.lerp), (6.0, 1.0, 0.2));
        let creaking = walk_rule(MobKind::Creaking);
        assert_eq!((creaking.gain, creaking.cap), (25.0, 3.0));
        assert_eq!(walk_rule(MobKind::Frog).gain, 25.0);
        assert!(walk_rule(MobKind::Bee).with_y && walk_rule(MobKind::Parrot).with_y);
        assert!(!walk_rule(MobKind::Pig).with_y);
    }

    /// `position(partial)` runs back by the tick's speed; `speed(partial)`
    /// lerps and caps at 1 (a hit's 1.5 kick shows as 1).
    #[test]
    fn rendered_walk_interpolates_and_scales_babies() {
        assert_eq!(walk_render(10.0, 0.2, 0.4, 0.25, 1.0), (10.0 - 0.4 * 0.75, 0.25));
        assert_eq!(walk_render(10.0, 0.2, 0.4, 1.0, BABY_WALK_SCALE), (30.0, 0.4));
        assert_eq!(walk_render(5.0, 1.5, 1.5, 0.5, 1.0).1, 1.0);
    }

    /// Legs on opposite corners swing together: `cos(p * 0.6662) * 1.4 * s`.
    #[test]
    fn limbs_swing_as_java_s_cosine() {
        assert!(close(limb(0.0, 1.0, LEG_SWING, false), 1.4));
        assert!(close(limb(0.0, 1.0, LEG_SWING, true), -1.4));
        let half = std::f32::consts::PI / WALK_FREQ;
        assert!(close(limb(half, 0.5, LEG_SWING, false), -0.7));
    }

    /// `applyWalk`: a camel 7.5 walk units in is half way round its 1.5 s
    /// loop (7.5 * 50 * 2 = 750 ms); the strength ramps with the speed.
    #[test]
    fn keyframe_walks_loop_on_java_s_clock() {
        let (t, scale) = keyframe_walk(7.5, 0.2, CAMEL_WALK);
        assert!(close(t, 0.5) && close(scale, 0.5), "{t} {scale}");
        let (t, scale) = keyframe_walk(15.0, 0.9, CAMEL_WALK);
        assert!(close(t, 0.0) && scale == 1.0, "{t} {scale}");
        // A sniffer's full scale comes at 1/100 of the speed.
        assert_eq!(keyframe_walk(1.0, 0.01, SNIFFER_WALK).1, 1.0);
        // Whole milliseconds, as Java's `(long)` cast.
        assert_eq!(keyframe_walk(0.0101, 1.0, CREAKING_WALK).0, 0.0);
    }

    #[test]
    fn triangle_wave_matches_mth() {
        // (|x % p - p/2| - p/4) / (p/4)
        assert!(close(triangle_wave(0.0, 10.0), 1.0));
        assert!(close(triangle_wave(2.5, 10.0), 0.0));
        assert!(close(triangle_wave(5.0, 10.0), -1.0));
        assert!(close(triangle_wave(13.0, 13.0), 1.0));
        assert!(close(triangle_wave(9.75, 13.0), 0.0));
    }

    /// The topple starts one tick after `deathTime` starts counting, eases
    /// over as a square root and lands 13.5 ticks in.
    #[test]
    fn death_topple_is_java_s_square_root() {
        assert_eq!(death_fall(0.0), 0.0);
        assert_eq!(death_fall(1.0), 0.0);
        assert!(close(death_fall(6.0), (5.0f32 / 20.0 * 1.6).sqrt()));
        assert!(death_fall(13.4) < 1.0);
        assert_eq!(death_fall(13.5), 1.0);
        assert_eq!(death_fall(19.9), 1.0);
        assert_eq!(flip(MobKind::Zombie), std::f32::consts::FRAC_PI_2);
        assert_eq!(flip(MobKind::Spider), std::f32::consts::PI);
        assert_eq!(flip(MobKind::Endermite), std::f32::consts::PI);
    }

    /// A 6-tick swing reads 0, 1/6, ... 1 on successive ticks, then ends;
    /// it can't restart in its first half.
    #[test]
    fn swing_runs_six_ticks() {
        let mut s = Swing::default();
        s.start_if_able(SWING_TICKS);
        let mut seen = Vec::new();
        for _ in 0..9 {
            s.tick();
            seen.push(s.anim);
        }
        let want = [0.0, 1.0 / 6.0, 2.0 / 6.0, 0.5, 4.0 / 6.0, 5.0 / 6.0, 1.0, 0.0, 0.0];
        assert!(seen.iter().zip(want).all(|(a, b)| close(*a, b)), "{seen:?}");
        assert!(!s.swinging());

        let mut s = Swing::default();
        s.start_if_able(SWING_TICKS);
        s.tick();
        s.tick();
        s.tick();
        s.start_if_able(SWING_TICKS);
        s.tick();
        assert!(close(s.anim, 3.0 / 6.0), "ticks 1..=3 refuse a restart: {}", s.anim);
        s.tick();
        s.start_if_able(SWING_TICKS);
        s.tick();
        assert!(close(s.anim, 0.0), "past half way it restarts: {}", s.anim);
        // Drawn across a restart (`anim` below `old`), it carries on toward
        // `anim + 1` instead of snapping back.
        assert!(close(swing_render(0.9, 0.1, 0.5), 1.0));
        assert!(close(swing_render(0.2, 0.4, 0.5), 0.3));
    }

    /// `animateZombieArms` at rest: arms raised to -pi/2.25 (pi/1.5 when
    /// aggressive), and mid-swing raised and swept.
    #[test]
    fn zombie_arms_follow_animation_utils() {
        let (x, y) = zombie_arms(false, true, 0.0);
        assert!(close(x, -std::f32::consts::PI / 2.25) && close(y, 0.1));
        let (x, _) = zombie_arms(true, true, 0.0);
        assert!(close(x, -std::f32::consts::PI / 1.5));
        let (x, y) = zombie_arms(true, true, 0.5);
        let xm = (0.75 * std::f32::consts::PI).sin();
        assert!(close(x, -std::f32::consts::PI / 1.5 + 1.2 - xm * 0.4) && close(y, 0.1 - 0.6));
    }
}
