//! Thrown eyes of ender, like Java's `EyeOfEnder`: they drift up and
//! toward the nearest stronghold for four seconds, then drop back as an item
//! (four times in five) or shatter.

use glam::DVec3;

/// Ticks an eye flies before it drops or shatters (Java's 80).
const LIFE: u32 = 80;
/// How far toward the stronghold one eye leads (Java's 12 blocks).
const LEAD: f64 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnderEye {
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    /// Blocks per tick.
    vel: DVec3,
    /// Where it steers (`target`); none for an eye loaded from a save, as
    /// Java doesn't save it (v1.0 gap Track 8f).
    target: Option<DVec3>,
    life: u32,
    /// Whether it drops back as an item rather than shattering.
    pub survives: bool,
    /// Fraction of a tick carried over between updates.
    pending: f64,
}

impl EnderEye {
    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(pos: DVec3, previous_pos: DVec3) -> Self {
        Self { pos, previous_pos, vel: DVec3::ZERO, target: Some(pos), life: 0, survives: true, pending: 0.0 }
    }

    /// v1.0 gap Track 8f: the level file's copy. Java saves an eye as an
    /// entity with its `Item` (`EyeOfEnder.addAdditionalSaveData`): its
    /// position and motion, but not its target, life or whether it
    /// survives, so a reloaded eye flies on in a straight line for another
    /// 80 ticks and shatters.
    pub(super) fn save(&self) -> serde_json::Value {
        use super::saved_projectiles::v3;
        serde_json::json!({"pos": v3(self.pos), "motion": v3(self.vel)})
    }

    pub(super) fn load(v: &serde_json::Value) -> Option<Self> {
        use super::saved_projectiles as s;
        let pos = s::read_v3(&v["pos"])?;
        let vel = s::read_v3(&v["motion"]).unwrap_or(DVec3::ZERO);
        Some(Self { pos, previous_pos: pos, vel, target: None, life: 0, survives: false, pending: 0.0 })
    }

    /// Released at `pos`, heading for `stronghold`: if that is more than 12
    /// blocks away it aims 12 blocks that way and 8 up.
    pub fn signalled(pos: DVec3, stronghold: DVec3, survives: bool) -> Self {
        let delta = stronghold - pos;
        let flat = delta.with_y(0.0).length();
        let target =
            if flat > LEAD { pos + DVec3::new(delta.x / flat * LEAD, 8.0, delta.z / flat * LEAD) } else { stronghold };
        Self { pos, previous_pos: pos, vel: DVec3::ZERO, target: Some(target), life: 0, survives, pending: 0.0 }
    }

    /// Moves the eye; returns `false` once its flight is over.
    pub(super) fn update(&mut self, dt: f64) -> bool {
        self.pending += dt * 20.0;
        while self.pending > 1.0 - 1e-6 {
            self.pending -= 1.0;
            if !self.tick() {
                return false;
            }
        }
        true
    }

    /// One Java tick: steer toward the target, slowly speeding up.
    fn tick(&mut self) -> bool {
        self.pos += self.vel;
        if let Some(target) = self.target {
            let speed = self.vel.with_y(0.0).length();
            let to = (target - self.pos).with_y(0.0);
            let dist = to.length();
            let angle = to.z.atan2(to.x);
            let mut h = speed + (dist - speed) * 0.0025;
            let mut y = self.vel.y;
            if dist < 1.0 {
                h *= 0.8;
                y *= 0.8;
            }
            let up = if self.pos.y < target.y { 1.0 } else { -1.0 };
            self.vel = DVec3::new(angle.cos() * h, y + (up - y) * 0.015, angle.sin() * h);
        }
        self.life += 1;
        self.life <= LIFE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fly(eye: &mut EnderEye) -> u32 {
        let mut ticks = 0;
        while eye.update(0.05) {
            ticks += 1;
        }
        ticks
    }

    #[test]
    fn eyes_lead_toward_far_strongholds_and_rise() {
        let start = DVec3::new(0.5, 70.0, 0.5);
        let mut eye = EnderEye::signalled(start, DVec3::new(-1500.0, 30.0, 0.0), true);
        let ticks = fly(&mut eye);
        assert_eq!(ticks, LIFE, "four seconds of flight");
        let moved = eye.pos - start;
        assert!(moved.x < -6.0 && moved.x > -14.0, "drifts most of 12 blocks west: {moved}");
        assert!(moved.z.abs() < 0.5, "straight toward it: {moved}");
        assert!(moved.y > 3.0 && moved.y < 9.0, "rises: {moved}");
    }

    #[test]
    fn eyes_sink_toward_a_stronghold_below() {
        let start = DVec3::new(0.5, 70.0, 0.5);
        let mut eye = EnderEye::signalled(start, DVec3::new(4.0, 30.0, 3.0), false);
        fly(&mut eye);
        assert!(eye.pos.y < start.y - 3.0, "heads down: {}", eye.pos);
        assert!(eye.pos.with_y(0.0).distance(DVec3::new(4.0, 0.0, 3.0)) < 4.0, "over it: {}", eye.pos);
    }

    /// v1.0 gap Track 8f: a saved eye reloads where it was with its motion,
    /// flies straight on (no target), and shatters after Java's 80 ticks.
    #[test]
    fn saved_eyes_fly_straight_on_and_shatter() {
        let mut eye = EnderEye::signalled(DVec3::new(0.5, 70.0, 0.5), DVec3::new(500.0, 30.0, 0.0), true);
        for _ in 0..10 {
            eye.tick();
        }
        let saved = eye.save();
        let mut loaded = EnderEye::load(&saved).unwrap();
        assert_eq!((loaded.pos, loaded.vel), (eye.pos, eye.vel));
        assert_eq!(loaded.save(), saved, "resaves the same");
        assert!(!loaded.survives);
        let (start, vel) = (loaded.pos, loaded.vel);
        assert_eq!(fly(&mut loaded), LIFE);
        assert!((loaded.pos - (start + vel * f64::from(LIFE + 1))).length() < 1e-9, "a straight line");
        assert!(EnderEye::load(&serde_json::json!({"motion": [0, 1, 0]})).is_none());
    }
}
