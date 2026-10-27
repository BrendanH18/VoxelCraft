//! Beds: two-block placement, breaking both halves together, and sleeping
//! through the night (and the rain), which also sets the respawn point.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::world::block::{Block, Facing};
use crate::world::colour_blocks::straw_bed;

use super::Game;

/// Seconds the screen takes to fade out before the night is skipped.
pub(super) const SLEEP_TIME: f32 = 2.5;
/// Hostile mobs this close keep the player awake.
const MONSTER_RANGE: f64 = 8.0;
/// Time of day to wake up at (just after sunrise).
const WAKE_TIME: f64 = 0.02;

/// A bed's head block and the direction from its foot to it.
type BedHead = (IVec3, Facing);
/// The foot of a bed, its head if the bed is whole, and where to lie down.
type BedRules = (IVec3, Option<BedHead>, Result<DVec3, &'static str>);

/// Whether the night (or the rain) can be slept through at `day_time`.
pub(super) fn can_sleep(day_time: f64, raining: bool) -> bool {
    raining || super::sky_state(day_time).daylight <= 0.35
}

impl Game {
    /// Bed explosions use the same two-cell removal helper as mining.
    pub(super) fn break_bed_partner(&mut self, pos: IVec3, half: Block) {
        self.world.break_bed_partner(pos, half, self.session.mode.is_survival() && self.gamerules.bool("block_drops"));
    }

    /// Right-click on a bed: sets the respawn point and, at night or in the
    /// rain with no monsters about, goes to sleep.
    pub(super) fn use_bed(&mut self, pos: IVec3) {
        let Some((at, head)) = self.bed_rest(pos) else { return };
        self.session.begin_sleep(head, at);
        self.occupy_straw_bed(head, at);
        self.keys.clear();
        self.body.left_held = false;
        self.body.right_held = false;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
        self.audio.play(Sound::Step(Material::Snow), None, 0.6, (0.8, 0.9));
    }

    /// The checks every sleeper goes through: beds explode outside the
    /// Overworld; using one sets the respawn point; sleep needs night or rain,
    /// no monsters nearby and a free bed. Returns where to lie down and the
    /// bed's head block.
    pub(super) fn bed_rest(&mut self, pos: IVec3) -> Option<(DVec3, Option<BedHead>)> {
        if self.bed_explodes(pos) {
            return None;
        }
        let (foot, head, rest) = self.bed_rules(pos)?;
        // `straw_bed_rule`: `can_set_spawn` is never.
        if !is_straw(self.world.get_block(foot)) && self.session.spawn_bed != Some(foot) {
            self.session.spawn_bed = Some(foot);
            self.session.spawn_point = None;
            self.session.spawn_anchor = None;
            self.show_popup("Respawn point set");
        }
        rest.map_err(|e| self.show_popup(e)).ok().map(|at| (at, head))
    }

    /// The foot of the bed at `pos`, its head and foot-to-head direction (none
    /// for a bed that lost its other half), and where to lie down in it or
    /// why not.
    fn bed_rules(&self, pos: IVec3) -> Option<BedRules> {
        let half = self.world.get_block(pos)?;
        let (foot, head, height) = if let Some((foot, head, facing)) = self.straw_bed_halves(pos) {
            // `StrawBedBlock.getSleepHeight`: the top of the foot's shape.
            (foot, Some((head, facing)), 4.0 / 16.0)
        } else {
            let partner = self.world.bed_partner(pos, half);
            let foot = if !half.is_bed_head() { Some(pos) } else { partner }?;
            let head = if half.is_bed_head() { Some(pos) } else { partner };
            let head = head.and_then(|head| crate::world::dismount::bed_orientation(foot, head));
            (foot, head, Block::BED_FOOT.height())
        };
        let centre = foot.as_dvec3() + DVec3::splat(0.5);
        let at = DVec3::new(centre.x, foot.y as f64 + height, centre.z);
        let monsters =
            || self.mobs.entities.mobs.iter().any(|m| m.kind.is_hostile() && m.pos.distance(centre) < MONSTER_RANGE);
        let host = self.session.sleeping.is_some().then_some(self.session.player.pos);
        let agents = self
            .agents
            .players
            .values()
            .filter(|b| b.agent.session.sleeping.is_some())
            .map(|b| b.agent.session.player.pos);
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
        Some((foot, head, rest))
    }

    /// An agent's `sleep`: makes the bed it's looking at its respawn point
    /// and lies down in it, by the same rules as the host.
    pub(super) fn agent_sleep(&mut self, name: &str) -> Result<(), String> {
        let agent = &self.agents.players[name].agent;
        let pos = (agent.target(&self.world).map(|(p, _)| p))
            .filter(|&p| self.world.get_block(p).is_some_and(|b| b.is_bed() || is_straw(Some(b))))
            .ok_or("not looking at a bed")?;
        if self.bed_explodes(pos) {
            return Err("beds explode outside the Overworld".into());
        }
        self.agent_sleep_at(name, pos).1
    }

    /// Agent `name` uses the bed at `pos` (which has not exploded): it becomes
    /// their respawn point, and they lie down if the bed rules allow. Returns
    /// whether the respawn point changed, and the sleep result.
    pub(super) fn agent_sleep_at(&mut self, name: &str, pos: IVec3) -> (bool, Result<(), String>) {
        let Some((foot, head, rest)) = self.bed_rules(pos) else { return (false, Err("not looking at a bed".into())) };
        let Some(bot) = self.agents.players.get_mut(name) else { return (false, Err("no such player".into())) };
        let agent = &mut bot.agent;
        let straw = is_straw(self.world.get_block(foot));
        let changed = !straw && agent.session.spawn_bed != Some(foot);
        if !straw {
            agent.session.spawn_bed = Some(foot);
            agent.session.spawn_point = None;
            agent.session.spawn_anchor = None;
        }
        let at = match rest {
            Ok(at) => at,
            Err(e) => return (changed, Err(e.into())),
        };
        agent.session.begin_sleep(head, at);
        agent.previous_pos = at;
        agent.remaining = 0;
        self.occupy_straw_bed(head, at);
        self.audio.play(Sound::Step(Material::Snow), Some(at), 0.6, (0.8, 0.9));
        (changed, Ok(()))
    }

    /// Foot, head and foot-to-head facing of the whole straw bed at `pos`;
    /// `None` for anything else, a straw bed missing its other half
    /// included (`useWithoutItem` does nothing then).
    pub(super) fn straw_bed_halves(&self, pos: IVec3) -> Option<(IVec3, IVec3, Facing)> {
        let (facing, head) = straw_bed::decode(self.world.get_block(pos)?.0)?;
        let foot = if head { pos - facing.offset() } else { pos };
        let whole = self.world.get_block(foot) == Some(straw_bed::half(facing, false))
            && self.world.get_block(foot + facing.offset()) == Some(straw_bed::half(facing, true));
        whole.then_some((foot, foot + facing.offset(), facing))
    }

    /// `StrawBedBlock.destroyBed`: the head goes (its foot follows without
    /// a drop) with the straw bed's leaving sound.
    pub(super) fn destroy_straw_bed(&mut self, head: IVec3) {
        if self.world.get_block(head).and_then(|b| straw_bed::decode(b.0)).is_some_and(|(_, h)| h) {
            self.world.set_block(head, Block::AIR);
            self.audio.play(Sound::Break(Material::Grass), Some(head.as_dvec3() + DVec3::splat(0.5)), 1.0, (0.8, 1.0));
        }
    }

    /// Remembers a straw bed someone lies in, to break it once they leave.
    fn occupy_straw_bed(&mut self, head: Option<BedHead>, at: DVec3) {
        if let Some((head, _)) = head.filter(|&(head, _)| is_straw(self.world.get_block(head))) {
            self.world.straw_sleepers_mut().push((head, at));
        }
    }

    /// `destroy_on_leave`: a straw bed nobody lies in any more falls apart.
    fn release_straw_beds(&mut self) {
        let sleepers: Vec<DVec3> = std::iter::once(&self.session)
            .chain(self.agents.players.values().map(|b| &b.agent.session))
            .filter(|s| s.sleeping.is_some())
            .map(|s| s.player.pos)
            .collect();
        let world = &mut self.world;
        let mut left = Vec::new();
        world.straw_sleepers_mut().retain(|&(head, at)| {
            let occupied = sleepers.iter().any(|p| p.distance(at) < 0.5);
            if !occupied {
                left.push(head);
            }
            occupied
        });
        for head in left {
            self.destroy_straw_bed(head);
        }
    }

    /// Advances sleep. Once every player (the host and each active, living
    /// agent or controller player) has faded out in bed, it's morning and
    /// the rain has stopped, as in Java with everyone needing to sleep.
    pub(super) fn update_sleep(&mut self, dt: f64) {
        // Anyone who woke since the last tick (a CLI player's command wakes
        // it before the gameplay tick) stands up beside their bed first, as
        // `LivingEntity.stopSleeping` does, before an empty straw bed breaks.
        self.session.settle_wake(&self.world);
        for b in self.agents.players.values_mut() {
            b.agent.session.settle_wake(&self.world);
        }
        self.release_straw_beds();
        if let Some(t) = &mut self.session.sleeping {
            *t = (*t + dt as f32).min(SLEEP_TIME);
        }
        for bot in self.agents.players.values_mut() {
            if let Some(t) = &mut bot.agent.session.sleeping {
                *t = (*t + dt as f32).min(SLEEP_TIME);
            }
        }
        let anyone =
            self.session.sleeping.is_some() || self.agents.players.values().any(|b| b.agent.session.sleeping.is_some());
        let (asleep, players) = self.sleep_count();
        let needed = required_sleepers(players, self.gamerules.int("players_sleeping_percentage"));
        if !anyone || asleep < needed {
            return;
        }
        if (0.5..0.96).contains(&self.day_time) {
            if self.session.sleeping.is_some() {
                self.mobs.entities.cat_morning_gifts(self.session.id, self.session.player.pos);
            }
            for bot in self.agents.players.values().filter(|b| b.agent.session.sleeping.is_some()) {
                self.mobs.entities.cat_morning_gifts(bot.id, bot.agent.session.player.pos);
            }
        }
        self.session.wake(&self.world);
        for bot in self.agents.players.values_mut() {
            bot.agent.session.wake(&self.world);
        }
        // `ServerLevel.tick` skips the night only with `advance_time` and
        // clears rain only with `advance_weather`.
        if self.gamerules.bool("advance_time") {
            self.day_time = WAKE_TIME;
        }
        if self.gamerules.bool("advance_weather") && self.weather.raining {
            self.weather.set(false, true);
        }
        self.show_popup("Good morning");
    }

    /// Players asleep (faded out) and players who must sleep, the host
    /// included, for the "waiting for others" message.
    pub(super) fn sleep_count(&self) -> (usize, usize) {
        let faded = |t: Option<f32>| t.is_some_and(|t| t >= SLEEP_TIME);
        let mut counts = (faded(self.session.sleeping) as usize, !self.session.vitals.is_dead() as usize);
        for (name, bot) in &self.agents.players {
            if bot.active && !bot.agent.session.vitals.is_dead() && !self.unplugged(name) {
                counts.0 += faded(bot.agent.session.sleeping) as usize;
                counts.1 += 1;
            }
        }
        counts
    }

    /// Where to respawn: on the bed last slept in if it's still there,
    /// otherwise the world spawn.
    pub(super) fn respawn_point(&mut self) -> (DVec3, f32) {
        let (at, yaw, missing_bed) = self.respawn_point_of(&super::lan::PlayerRef::Host);
        if missing_bed {
            self.show_popup("Your home bed was missing");
        }
        (at, yaw)
    }

    /// Where `who` respawns, the yaw they face there, and whether their home
    /// bed was missing.
    pub(super) fn respawn_point_of(&mut self, who: &super::lan::PlayerRef) -> (DVec3, f32, bool) {
        let session = match who {
            super::lan::PlayerRef::Host => Some(&mut self.session),
            super::lan::PlayerRef::Agent(name) => self.agents.players.get_mut(name).map(|b| &mut b.agent.session),
        };
        let personal = session.map(|s| s.personal_spawn(&self.world));
        let missing = personal.as_ref().is_some_and(|p| p.missing_bed);
        if let Some(p) = personal
            && let Some(position) = p.position
        {
            return (position, p.yaw, missing);
        }
        (self.world_spawn_point(), voxelcraft::session::DEFAULT_SPAWN_YAW, missing)
    }

    /// The world spawn, found as Java's `PlayerSpawnFinder` does within the
    /// `spawnRadius` rule (the world border and a world default of Adventure
    /// are not modelled).
    fn world_spawn_point(&self) -> DVec3 {
        let radius = self.gamerules.int("respawn_radius");
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let generator = &self.world.generator;
        crate::world::spawn_finder::find_spawn(
            &self.world,
            self.world_spawn,
            radius,
            (nanos >> 4) as u32,
            false,
            |x, z| self.world.surface_height(x, z).or_else(|| Some(generator.column(x, z).height)),
            (self.world.min_y(), self.world.max_y()),
        )
    }
}

pub(super) fn is_straw(b: Option<Block>) -> bool {
    b.is_some_and(|b| straw_bed::decode(b.0).is_some())
}

fn required_sleepers(players: usize, percentage: i32) -> usize {
    if players == 0 {
        return 0;
    }
    let percent = percentage.max(0) as usize;
    (players.saturating_mul(percent).div_ceil(100)).max(1)
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
}
