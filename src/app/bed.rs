//! Beds: two-block placement, breaking both halves together, and sleeping
//! through the night (and the rain), which also sets the respawn point.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::world::World;
use crate::world::block::Block;

use super::{Game, GameMode};

/// Seconds the screen takes to fade out before the night is skipped.
pub(super) const SLEEP_TIME: f32 = 2.5;
/// Hostile mobs this close keep the player awake.
const MONSTER_RANGE: f64 = 8.0;
/// Time of day to wake up at (just after sunrise).
const WAKE_TIME: f64 = 0.02;

const SIDES: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

/// The other half of the bed at `pos`, if it's still there.
fn partner(world: &World, pos: IVec3, half: Block) -> Option<IVec3> {
    let other = if half == Block::BED_FOOT { Block::BED_HEAD } else { Block::BED_FOOT };
    SIDES.iter().map(|&s| pos + s).find(|&p| world.get_block(p) == Some(other))
}

/// Whether the night (or the rain) can be slept through at `day_time`.
pub(super) fn can_sleep(day_time: f64, raining: bool) -> bool {
    raining || super::sky_state(day_time).daylight <= 0.35
}

impl Game {
    /// Places a bed with its foot at `at` and its head one block further
    /// along the way the player faces. Returns whether it went down.
    pub(super) fn place_bed(&mut self, at: IVec3) -> bool {
        let f = self.player.forward();
        let dir = if f.x.abs() > f.z.abs() {
            IVec3::new(f.x.signum() as i32, 0, 0)
        } else {
            IVec3::new(0, 0, f.z.signum() as i32)
        };
        let head = at + dir;
        let fits = |p: IVec3| {
            self.world.get_block(p).is_some_and(|b| b.is_replaceable())
                && self.world.get_block(p - IVec3::Y).is_some_and(|b| b.is_opaque())
                && !self.player.intersects_block(p)
        };
        if !fits(at) || !fits(head) {
            return false;
        }
        self.world.set_block(at, Block::BED_FOOT);
        self.world.set_block(head, Block::BED_HEAD);
        self.audio.block_place(Block::WOOL, at);
        true
    }

    /// After one half of a bed at `pos` broke, removes the other half. Only
    /// the foot drops the bed, so breaking the head spills the foot's drop.
    pub(super) fn break_bed_partner(&mut self, pos: IVec3, half: Block) {
        let Some(other) = partner(&self.world, pos, half) else { return };
        self.world.set_block(other, Block::AIR);
        if half == Block::BED_HEAD && self.mode == GameMode::Survival {
            self.world.spill_block(other, Block::BED_FOOT);
        }
    }

    /// Right-click on a bed: sets the respawn point and, at night or in the
    /// rain with no monsters about, goes to sleep.
    pub(super) fn use_bed(&mut self, pos: IVec3) {
        let Some(at) = self.bed_rest(pos) else { return };
        self.player.pos = at;
        self.player.vel = DVec3::ZERO;
        self.player.flying = false;
        self.keys.clear();
        self.left_held = false;
        self.right_held = false;
        self.actions.reset();
        self.sleeping = Some(0.0);
        self.audio.play(Sound::Step(Material::Snow), None, 0.6, (0.8, 0.9));
    }

    /// The checks every sleeper goes through: beds explode outside the
    /// Overworld; using one sets the respawn point; sleep needs night or rain
    /// and no monsters nearby. Returns where to lie down.
    pub(super) fn bed_rest(&mut self, pos: IVec3) -> Option<DVec3> {
        if self.bed_explodes(pos) {
            return None;
        }
        let half = self.world.get_block(pos)?;
        let foot = if half == Block::BED_FOOT { Some(pos) } else { partner(&self.world, pos, half) }?;
        if self.spawn_bed != Some(foot) {
            self.spawn_bed = Some(foot);
            self.show_popup("Respawn point set");
        }
        if !can_sleep(self.day_time, self.weather.raining) {
            self.show_popup("You can only sleep at night");
            return None;
        }
        let centre = foot.as_dvec3() + DVec3::splat(0.5);
        let monsters =
            self.mobs.entities.mobs.iter().any(|m| m.kind.is_hostile() && m.pos.distance(centre) < MONSTER_RANGE);
        if monsters {
            self.show_popup("You may not rest now; there are monsters nearby");
            return None;
        }
        Some(DVec3::new(centre.x, foot.y as f64 + Block::BED_FOOT.height(), centre.z))
    }

    /// Advances sleep. Once every player (the host and each living
    /// controller player) has faded out in bed, it's morning and the rain
    /// has stopped, as in Java with everyone needing to sleep.
    pub(super) fn update_sleep(&mut self, dt: f64) {
        if let Some(t) = &mut self.sleeping {
            *t = (*t + dt as f32).min(SLEEP_TIME);
        }
        let (asleep, players) = self.advance_pad_sleep(dt as f32);
        let host = self.sleeping.is_some_and(|t| t >= SLEEP_TIME);
        let anyone = self.sleeping.is_some() || asleep > 0;
        let host_counts = !self.vitals.is_dead();
        if !anyone || asleep < players || (host_counts && !host) {
            return;
        }
        self.sleeping = None;
        self.wake_pads();
        self.day_time = WAKE_TIME;
        if self.weather.raining {
            self.weather.set(false, true);
        }
        self.show_popup("Good morning");
    }

    /// Players asleep (faded out) and players who must sleep, the host
    /// included, for the "waiting for others" message.
    pub(super) fn sleep_count(&self) -> (usize, usize) {
        let (asleep, players) = self.pad_sleep_count();
        let host = !self.vitals.is_dead();
        let host_asleep = self.sleeping.is_some_and(|t| t >= SLEEP_TIME);
        (asleep + host_asleep as usize, players + host as usize)
    }

    /// Where to respawn: on the bed last slept in if it's still there,
    /// otherwise the world spawn.
    pub(super) fn respawn_point(&mut self) -> DVec3 {
        if let Some(foot) = self.spawn_bed {
            if self.world.get_block(foot) == Some(Block::BED_FOOT) {
                let top = foot.y as f64 + Block::BED_FOOT.height();
                return DVec3::new(foot.x as f64 + 0.5, top, foot.z as f64 + 0.5);
            }
            self.spawn_bed = None;
            self.show_popup("Your home bed was missing");
        }
        self.world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleeping_only_at_night_or_in_rain() {
        assert!(!can_sleep(0.25, false), "noon");
        assert!(can_sleep(0.75, false), "midnight");
        assert!(can_sleep(0.25, true), "rain");
        assert!(!can_sleep(WAKE_TIME, false), "waking up doesn't allow another sleep");
    }
}
