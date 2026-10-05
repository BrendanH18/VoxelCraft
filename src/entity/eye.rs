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
    target: DVec3,
    life: u32,
    /// Whether it drops back as an item rather than shattering.
    pub survives: bool,
    /// Fraction of a tick carried over between updates.
    pending: f64,
}

impl EnderEye {
    /// Released at `pos`, heading for `stronghold`: if that is more than 12
    /// blocks away it aims 12 blocks that way and 8 up.
    pub fn signalled(pos: DVec3, stronghold: DVec3, survives: bool) -> Self {
        let delta = stronghold - pos;
        let flat = delta.with_y(0.0).length();
        let target =
            if flat > LEAD { pos + DVec3::new(delta.x / flat * LEAD, 8.0, delta.z / flat * LEAD) } else { stronghold };
        Self { pos, previous_pos: pos, vel: DVec3::ZERO, target, life: 0, survives, pending: 0.0 }
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
        let speed = self.vel.with_y(0.0).length();
        let to = (self.target - self.pos).with_y(0.0);
        let dist = to.length();
        let angle = to.z.atan2(to.x);
        let mut h = speed + (dist - speed) * 0.0025;
        let mut y = self.vel.y;
        if dist < 1.0 {
            h *= 0.8;
            y *= 0.8;
        }
        let up = if self.pos.y < self.target.y { 1.0 } else { -1.0 };
        self.vel = DVec3::new(angle.cos() * h, y + (up - y) * 0.015, angle.sin() * h);
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
}
