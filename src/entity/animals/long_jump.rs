//! v1.0 gap Track 2g: the goat's long jumps, Java 26.3's
//! `GoatAi.initLongJumpActivity`: `LongJumpToRandomPos(TIME_BETWEEN_LONG_JUMPS,
//! 5, 5, 3.5714288)` picks a landing spot within 5 blocks each way, aims a
//! ballistic arc at it (`LongJumpUtil.calculateJumpVectorForAngle`) and
//! launches after 40 ticks; `LongJumpMidJump` flies it to the ground.
use glam::{DVec3, IVec3};

use crate::entity::path::{self, Start};
use crate::entity::{Mob, MobWorld, Rng};
use crate::physics::{self, Shape};
use crate::world::block::Block;

/// `GoatAi.TIME_BETWEEN_LONG_JUMPS`: 600 to 1200 ticks.
pub(crate) fn cooldown_seconds(rng: &mut Rng) -> f32 {
    (600 + rng.next_int(601)) as f32 / 20.0
}

/// `MAX_LONG_JUMP_WIDTH` and `MAX_LONG_JUMP_HEIGHT`.
const WIDTH: i32 = 5;
const HEIGHT: i32 = 5;
/// `JUMP_STRENGTH` (0.42) times the goat's 3.5714288: 1.5 blocks a tick.
const MAX_VELOCITY: f64 = (0.42f32 * 3.5714288f32) as f64;
/// `Entity.getGravity` for a mob: 0.08 blocks a tick squared.
const GRAVITY: f64 = 0.08;
/// `FIND_JUMP_TRIES`, `PREPARE_JUMP_DURATION`, the behaviour's 200-tick
/// timeout and `LongJumpMidJump`'s 100.
const TRIES: i32 = 20;
const PREPARE: i32 = 40;
const TIMEOUT: i32 = 200;
pub(crate) const MID_JUMP_TIMEOUT: i32 = 100;
/// `LongJumpToRandomPos.ALLOWED_ANGLES`.
const ANGLES: [i32; 4] = [65, 70, 75, 80];

/// A running `LongJumpToRandomPos` (never saved: Java's brain doesn't save
/// a running behaviour).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LongJump {
    /// `jumpCandidates`: cells and their weights (`ceil(distSqr)`).
    candidates: Vec<(IVec3, u32)>,
    /// `initialPosition`.
    initial: DVec3,
    /// `findJumpTries`.
    tries: i32,
    /// `chosenJump` (blocks a tick) and its landing cell.
    chosen: Option<(DVec3, IVec3)>,
    /// Ticks since the jump was chosen (`prepareJumpStart`).
    prepared: i32,
    /// Ticks before the 200-tick timeout.
    left: i32,
}

/// The goat's long-jump sounds (`NEUTRAL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    /// `GOAT_LONG_JUMP`: the launch.
    LongJump,
    /// `GOAT_SCREAMING_LONG_JUMP`: a screaming goat's launch.
    ScreamingLongJump,
    /// `GOAT_STEP` at volume 2: `LongJumpMidJump.stop` on the ground.
    Step,
}

impl Cue {
    pub const ALL: [Cue; 3] = [Cue::LongJump, Cue::ScreamingLongJump, Cue::Step];

    /// The Java sound event.
    pub fn key(self) -> &'static str {
        match self {
            Cue::LongJump => "entity.goat.long_jump",
            Cue::ScreamingLongJump => "entity.goat.screaming.long_jump",
            Cue::Step => "entity.goat.step",
        }
    }
}

/// What a tick of the long jump did, for the caller's sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Jumped {
    /// Nothing to report.
    None,
    /// `GOAT_LONG_JUMP` (or the screaming goat's): it launched.
    Launched,
    /// `LongJumpMidJump.stop` on the ground: `GOAT_STEP` at volume 2.
    Landed,
}

/// The `LONG_JUMPING` box a goat of `standing` size flies with.
pub(crate) fn jump_shape(standing: Shape) -> Shape {
    Shape::new(standing.half_width * 0.7, standing.height * 0.7)
}

/// `LongJumpToRandomPos.defaultAcceptableLandingSpot`, after
/// `isAcceptableLandingPosition`'s other column: a cell over a
/// solid-rendering block that costs the goat nothing to stand in.
fn acceptable<W: MobWorld + ?Sized>(w: &W, m: &Mob, at: IVec3, target: IVec3) -> bool {
    if at.x == target.x && at.z == target.z {
        return false;
    }
    let solid_below = w.block(target - IVec3::Y).is_some_and(|b| b.is_opaque() && b.is_solid());
    solid_below && m.caps(false).malus.get(path::static_type(w, target)) == 0.0
}

/// `LongJumpUtil.calculateJumpVectorForAngle` with its collision sweep, in
/// blocks a tick.
fn jump_vector<W: MobWorld + ?Sized>(w: &W, from: DVec3, target: DVec3, angle: i32, shape: Shape) -> Option<DVec3> {
    let plane = DVec3::new(target.x - from.x, 0.0, target.z - from.z).normalize_or_zero() * 0.5;
    let aim = target - plane - from;
    let rad = f64::from(angle as f32 * std::f32::consts::PI / 180.0);
    let xz = aim.z.atan2(aim.x);
    let r2 = aim.x * aim.x + aim.z * aim.z;
    let r = r2.sqrt();
    let y = aim.y;
    let v0sqr = r2 * GRAVITY / (r * (2.0 * rad).sin() - 2.0 * y * rad.cos().powi(2));
    if v0sqr < 0.0 || !v0sqr.is_finite() {
        return None;
    }
    let v0 = v0sqr.sqrt();
    if v0 > MAX_VELOCITY {
        return None;
    }
    let (v0r, v0y) = (v0 * rad.cos(), v0 * rad.sin());
    // Sampled along the arc, each step swept in box-sized moves.
    let samples = (r / v0r).ceil() as i32 * 2;
    let mut ri = 0.0;
    let mut previous: Option<DVec3> = None;
    for _ in 0..samples - 1 {
        ri += r / f64::from(samples);
        let yi = rad.sin() / rad.cos() * ri - ri * ri * GRAVITY / (2.0 * v0sqr * rad.cos().powi(2));
        let at = from + DVec3::new(ri * xz.cos(), yi, ri * xz.sin());
        if let Some(p) = previous
            && !clear(w, shape, p, at)
        {
            return None;
        }
        previous = Some(at);
    }
    Some(DVec3::new(v0r * xz.cos(), v0y, v0r * xz.sin()) * f64::from(0.95f32))
}

/// `LongJumpUtil.isClearTransition`.
fn clear<W: MobWorld + ?Sized>(w: &W, shape: Shape, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let step = (shape.half_width * 2.0).min(shape.height);
    let checks = (d.length() / step).ceil() as i32;
    let dir = d.normalize_or_zero();
    let mut at = a;
    for i in 0..checks {
        at = if i == checks - 1 { b } else { at + dir * (step * f64::from(0.9f32)) };
        if physics::overlaps_solid(w, at, shape) {
            return false;
        }
    }
    true
}

impl Mob {
    /// `LongJumpToRandomPos` and `LongJumpMidJump` for one tick. `allowed`:
    /// the `LONG_JUMP` activity's conditions (no temptation, breeding or
    /// walk target, and not ramming); the caller counts the cooldown down.
    pub(crate) fn goat_long_jump<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng, allowed: bool) -> Jumped {
        let Some(a) = self.animal.as_ref() else { return Jumped::None };
        // `LongJumpMidJump` (priority 0): until it is on the ground, or 100
        // ticks; landing keeps a tenth of its speed across.
        if a.long_jumping {
            let left = a.mid_jump_left - 1;
            let on_ground = self.on_ground;
            let a = self.animal.as_mut().unwrap();
            a.mid_jump_left = left;
            if on_ground || left <= 0 {
                a.long_jumping = false;
                a.jump_cooldown = cooldown_seconds(rng);
                if on_ground {
                    self.vel.x *= f64::from(0.1f32);
                    self.vel.z *= f64::from(0.1f32);
                    return Jumped::Landed;
                }
            }
            return Jumped::None;
        }
        if !allowed || a.jump_cooldown > 0.0 {
            self.animal.as_mut().unwrap().long_jump = None;
            return Jumped::None;
        }
        if a.long_jump.is_none() {
            // `checkExtraStartConditions`: on the ground, out of water and
            // lava, not on honey; otherwise half a cooldown.
            let cell = self.pos.floor().as_ivec3();
            let on_honey = w.block(cell) == Some(Block::HONEY_BLOCK);
            let in_lava = w.block(cell).is_some_and(Block::is_lava);
            if !self.on_ground || self.in_water || in_lava || on_honey {
                self.animal.as_mut().unwrap().jump_cooldown = cooldown_seconds(rng) / 2.0;
                return Jumped::None;
            }
            // `start`: every cell within 5 each way but its own.
            let mut candidates = Vec::with_capacity(1330);
            for x in -WIDTH..=WIDTH {
                for y in -HEIGHT..=HEIGHT {
                    for z in -WIDTH..=WIDTH {
                        if x != 0 || y != 0 || z != 0 {
                            candidates.push((cell + IVec3::new(x, y, z), (x * x + y * y + z * z) as u32));
                        }
                    }
                }
            }
            let jump =
                LongJump { candidates, initial: self.pos, tries: TRIES, chosen: None, prepared: 0, left: TIMEOUT };
            self.animal.as_mut().unwrap().long_jump = Some(Box::new(jump));
        }
        let mut jump = self.animal.as_mut().unwrap().long_jump.take().unwrap();
        // `tickOrStop`: the timeout, then `canStillUse`: still where it
        // started, tries left, out of water and something to jump to.
        jump.left -= 1;
        if jump.left < 0 {
            return Jumped::None;
        }
        let valid = jump.initial == self.pos
            && jump.tries > 0
            && !self.in_water
            && (jump.chosen.is_some() || !jump.candidates.is_empty());
        if !valid {
            self.animal.as_mut().unwrap().jump_cooldown = cooldown_seconds(rng) / 2.0;
            self.head.target = None;
            return Jumped::None;
        }
        let result = if let Some((vector, _)) = jump.chosen {
            jump.prepared += 1;
            if jump.prepared >= PREPARE {
                // The launch, in blocks a second; `LongJumpMidJump` takes
                // over (`LONG_JUMPING`).
                self.vel = vector * 20.0;
                let a = self.animal.as_mut().unwrap();
                a.long_jumping = true;
                a.mid_jump_left = MID_JUMP_TIMEOUT;
                Jumped::Launched
            } else {
                Jumped::None
            }
        } else {
            jump.tries -= 1;
            self.pick_long_jump(w, rng, &mut jump);
            Jumped::None
        };
        if !self.animal.as_ref().unwrap().long_jumping {
            self.animal.as_mut().unwrap().long_jump = Some(jump);
        }
        result
    }

    /// `pickCandidate`: weighted picks (`WeightedRandom.getRandomItem`), each
    /// removed, until one is acceptable, has an arc at one of the shuffled
    /// angles and can't be walked to within 8 blocks of path.
    fn pick_long_jump<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng, jump: &mut LongJump) {
        let at = self.pos.floor().as_ivec3();
        let shape = jump_shape(self.shape());
        while !jump.candidates.is_empty() {
            let total: u32 = jump.candidates.iter().map(|c| c.1).sum();
            let mut roll = rng.next_int(total.max(1)) as i64;
            let mut index = 0;
            for (i, c) in jump.candidates.iter().enumerate() {
                roll -= i64::from(c.1);
                if roll < 0 {
                    index = i;
                    break;
                }
            }
            let (target, _) = jump.candidates.remove(index);
            if !acceptable(w, self, at, target) {
                continue;
            }
            let centre = target.as_dvec3() + DVec3::splat(0.5);
            let mut angles = ANGLES;
            for i in (1..angles.len()).rev() {
                angles.swap(i, rng.next_int(i as u32 + 1) as usize);
            }
            let Some(vector) = angles.iter().find_map(|&a| jump_vector(w, self.pos, centre, a, shape)) else {
                continue;
            };
            self.head.target = Some(crate::entity::mob::head::Tracked::Point(centre));
            let mut caps = self.caps(false);
            caps.max_path_length = 8.0;
            let start = Start { pos: self.pos, on_ground: self.on_ground, in_water: self.in_water };
            let walkable = path::find_path(w, &caps, start, &[target], 0).is_some_and(|p| p.reached);
            if !walkable {
                jump.chosen = Some((vector, target));
                jump.prepared = 0;
                return;
            }
        }
    }

    /// Track 2g: whether the goat is in its `LONG_JUMP` activity, looking for,
    /// readying or flying a long jump (it doesn't walk meanwhile).
    pub(crate) fn goat_jump_busy(&self) -> bool {
        self.animal.as_ref().is_some_and(|a| a.long_jumping || a.long_jump.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MobKind;
    use crate::physics::test_util::Grid;

    /// The arc: Java's vector for a goat four blocks from a spot two up
    /// clears open air, lands near the spot under 0.08 gravity, and stays
    /// within 1.5 blocks a tick; a wall in the way refuses it.
    #[test]
    fn long_jump_arcs_land_on_their_spot() {
        let mut w = Grid::flat(64);
        let from = DVec3::new(0.5, 64.0, 0.5);
        let target = DVec3::new(4.5, 66.5, 0.5);
        let shape = jump_shape(Shape::new(0.45, 1.3));
        let v = (65..=80).step_by(5).find_map(|a| jump_vector(&w, from, target, a, shape)).expect("an arc");
        assert!(v.length() <= MAX_VELOCITY, "{v}");
        // Fly it: no drag (`setDiscardFriction`), 0.08 gravity.
        let (mut p, mut vel) = (from, v);
        let mut best = f64::MAX;
        for _ in 0..60 {
            p += vel;
            vel.y -= GRAVITY;
            if vel.y < 0.0 {
                best = best.min((p.x - 4.0).abs() + (p.y - 66.0).abs());
            }
        }
        assert!(best < 0.6, "{best}");
        for y in 64..70 {
            w.set(IVec3::new(2, y, 0), Block::STONE);
        }
        assert!((65..=80).step_by(5).all(|a| jump_vector(&w, from, target, a, shape).is_none()));
    }

    /// A goat off cooldown picks a spot, stands 40 ticks and launches,
    /// long jumping until it lands, then waits 600 to 1200 ticks.
    #[test]
    fn goats_prepare_launch_and_land() {
        let mut w = Grid::flat(64);
        // A ledge three up it can't walk to.
        for x in 3..6 {
            for z in -1..2 {
                for y in 64..67 {
                    w.set(IVec3::new(x, y, z), Block::STONE);
                }
            }
        }
        let mut launched = 0;
        for seed in 0..10 {
            let mut rng = Rng::new(seed);
            let mut m = Mob::new(MobKind::Goat, DVec3::new(0.5, 64.0, 0.5), 0.0);
            m.on_ground = true;
            m.animal.as_mut().unwrap().jump_cooldown = 0.0;
            let mut ticks = 0;
            while ticks < 200 {
                ticks += 1;
                if m.goat_long_jump(&w, &mut rng, true) == Jumped::Launched {
                    break;
                }
            }
            if !m.goat_long_jumping() {
                continue;
            }
            launched += 1;
            assert!(ticks > PREPARE, "{ticks}");
            assert!(m.vel.y > 0.0 && m.vel.length() <= MAX_VELOCITY * 20.0 + 1e-9);
            // In flight it keeps jumping; on the ground it lands.
            m.on_ground = false;
            assert_eq!(m.goat_long_jump(&w, &mut rng, true), Jumped::None);
            assert!(m.goat_long_jumping());
            m.on_ground = true;
            assert_eq!(m.goat_long_jump(&w, &mut rng, true), Jumped::Landed);
            let a = m.animal.as_ref().unwrap();
            assert!(!a.long_jumping && (30.0..=60.0).contains(&a.jump_cooldown), "{}", a.jump_cooldown);
        }
        assert!(launched > 0);
        // Not on the ground: half a cooldown, no jump.
        let mut rng = Rng::new(1);
        let mut m = Mob::new(MobKind::Goat, DVec3::new(0.5, 64.0, 0.5), 0.0);
        m.on_ground = false;
        m.animal.as_mut().unwrap().jump_cooldown = 0.0;
        assert_eq!(m.goat_long_jump(&w, &mut rng, true), Jumped::None);
        assert!((15.0..=30.0).contains(&m.animal.as_ref().unwrap().jump_cooldown));
    }
}
