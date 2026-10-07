//! Beds: two-block placement, breaking both halves together, and sleeping
//! through the night (and the rain), which also sets the respawn point.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::world::World;
use crate::world::block::Block;

use super::Game;

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
        if half == Block::BED_HEAD && self.mode.is_survival() && self.gamerules.bool("doTileDrops") {
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
    /// Overworld; using one sets the respawn point; sleep needs night or rain,
    /// no monsters nearby and a free bed. Returns where to lie down.
    pub(super) fn bed_rest(&mut self, pos: IVec3) -> Option<DVec3> {
        if self.bed_explodes(pos) {
            return None;
        }
        let (foot, rest) = self.bed_rules(pos)?;
        if self.spawn_bed != Some(foot) {
            self.spawn_bed = Some(foot);
            self.spawn_point = None;
            self.show_popup("Respawn point set");
        }
        rest.map_err(|e| self.show_popup(e)).ok()
    }

    /// The foot of the bed at `pos`, and where to lie down in it or why not.
    fn bed_rules(&self, pos: IVec3) -> Option<(IVec3, Result<DVec3, &'static str>)> {
        let half = self.world.get_block(pos)?;
        let foot = if half == Block::BED_FOOT { Some(pos) } else { partner(&self.world, pos, half) }?;
        let centre = foot.as_dvec3() + DVec3::splat(0.5);
        let at = DVec3::new(centre.x, foot.y as f64 + Block::BED_FOOT.height(), centre.z);
        let monsters =
            || self.mobs.entities.mobs.iter().any(|m| m.kind.is_hostile() && m.pos.distance(centre) < MONSTER_RANGE);
        let host = self.sleeping.is_some().then_some(self.player.pos);
        let agents = self.agents.players.values().filter(|b| b.agent.sleeping.is_some()).map(|b| b.agent.player.pos);
        let occupied = || host.into_iter().chain(agents).any(|p| p.distance(at) < 0.5);
        let rest = if !can_sleep(self.day_time, self.weather.raining) {
            Err("You can only sleep at night")
        } else if monsters() {
            Err("You may not rest now; there are monsters nearby")
        } else if occupied() {
            Err("This bed is occupied")
        } else {
            Ok(at)
        };
        Some((foot, rest))
    }

    /// An agent's `sleep`: makes the bed it's looking at its respawn point
    /// and lies down in it, by the same rules as the host.
    pub(super) fn agent_sleep(&mut self, name: &str) -> Result<(), String> {
        let agent = &self.agents.players[name].agent;
        let pos = (agent.target(&self.world).map(|(p, _)| p))
            .filter(|&p| self.world.get_block(p).is_some_and(|b| b.is_bed()))
            .ok_or("not looking at a bed")?;
        if self.bed_explodes(pos) {
            return Err("beds explode outside the Overworld".into());
        }
        let (foot, rest) = self.bed_rules(pos).ok_or("not looking at a bed")?;
        let agent = &mut self.agents.players.get_mut(name).unwrap().agent;
        agent.spawn_bed = Some(foot);
        agent.spawn_point = None;
        let at = rest?;
        agent.player.pos = at;
        agent.player.vel = DVec3::ZERO;
        agent.player.flying = false;
        agent.previous_pos = at;
        agent.remaining = 0;
        agent.sleeping = Some(0.0);
        self.audio.play(Sound::Step(Material::Snow), Some(at), 0.6, (0.8, 0.9));
        Ok(())
    }

    /// Advances sleep. Once every player (the host and each active, living
    /// agent or controller player) has faded out in bed, it's morning and
    /// the rain has stopped, as in Java with everyone needing to sleep.
    pub(super) fn update_sleep(&mut self, dt: f64) {
        if let Some(t) = &mut self.sleeping {
            *t = (*t + dt as f32).min(SLEEP_TIME);
        }
        for bot in self.agents.players.values_mut() {
            if let Some(t) = &mut bot.agent.sleeping {
                *t = (*t + dt as f32).min(SLEEP_TIME);
            }
        }
        let anyone = self.sleeping.is_some() || self.agents.players.values().any(|b| b.agent.sleeping.is_some());
        let (asleep, players) = self.sleep_count();
        let needed = required_sleepers(players, self.gamerules.int("playersSleepingPercentage"));
        if !anyone || asleep < needed {
            return;
        }
        self.sleeping = None;
        for bot in self.agents.players.values_mut() {
            bot.agent.sleeping = None;
        }
        self.day_time = WAKE_TIME;
        if self.weather.raining {
            self.weather.set(false, true);
        }
        self.show_popup("Good morning");
    }

    /// Players asleep (faded out) and players who must sleep, the host
    /// included, for the "waiting for others" message.
    pub(super) fn sleep_count(&self) -> (usize, usize) {
        let faded = |t: Option<f32>| t.is_some_and(|t| t >= SLEEP_TIME);
        let mut counts = (faded(self.sleeping) as usize, !self.vitals.is_dead() as usize);
        for (name, bot) in &self.agents.players {
            if bot.active && !bot.agent.vitals.is_dead() && !self.unplugged(name) {
                counts.0 += faded(bot.agent.sleeping) as usize;
                counts.1 += 1;
            }
        }
        counts
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
        if let Some(point) = self.spawn_point {
            return point.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        }
        let radius = self.gamerules.int("spawnRadius");
        let offset = spawn_offset(self.world.generator.seed, radius);
        let x = self.world_spawn.x.saturating_add(offset.x);
        let z = self.world_spawn.z.saturating_add(offset.z);
        let y = if offset == IVec3::ZERO { self.world_spawn.y } else { self.world.generator.column(x, z).height + 1 };
        DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5)
    }
}

fn required_sleepers(players: usize, percentage: i32) -> usize {
    if players == 0 {
        return 0;
    }
    let percent = percentage.max(0) as usize;
    (players.saturating_mul(percent).div_ceil(100)).max(1)
}

fn spawn_offset(seed: u64, radius: i32) -> IVec3 {
    let radius = radius.max(0) as u64;
    if radius == 0 {
        return IVec3::ZERO;
    }
    let mix = |mut n: u64| {
        n ^= n >> 30;
        n = n.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        n ^= n >> 27;
        n = n.wrapping_mul(0x94d0_49bb_1331_11eb);
        n ^ (n >> 31)
    };
    let width = radius.saturating_mul(2).saturating_add(1);
    let coordinate = |n| (mix(n) % width) as i64 - radius as i64;
    IVec3::new(coordinate(seed) as i32, 0, coordinate(seed ^ 0x9e37_79b9_7f4a_7c15) as i32)
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

    #[test]
    fn sleeping_percentage_rounds_up_and_always_needs_one() {
        assert_eq!(required_sleepers(3, 100), 3);
        assert_eq!(required_sleepers(3, 50), 2);
        assert_eq!(required_sleepers(10, 1), 1);
        assert_eq!(required_sleepers(10, 0), 1);
        assert_eq!(required_sleepers(3, 101), 4);
        assert_eq!(required_sleepers(0, 100), 0);
    }

    #[test]
    fn spawn_radius_zero_is_exact_and_offsets_stay_in_range() {
        assert_eq!(spawn_offset(7, 0), IVec3::ZERO);
        for seed in 0..100 {
            let p = spawn_offset(seed, 10);
            assert!(p.x.abs() <= 10 && p.z.abs() <= 10);
        }
    }
}
