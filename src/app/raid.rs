//! v0.15 Track E glue: every player's raid omens tick before the entities,
//! and the raid outcomes (Hero of the Village, the horn) land on them after.
use voxelcraft::entity::raid::{Cue, Outcome};
use voxelcraft::session::raid::Stats;
use voxelcraft::simulation::difficulty::Difficulty;
use voxelcraft::simulation::effects::Effect;

use super::Game;

impl Game {
    /// Bad Omen to Raid Omen to a raid, for the host and each active agent,
    /// and the heroes the villagers see.
    pub(super) fn tick_raid_omens(&mut self) {
        let peaceful = self.difficulty == Difficulty::Peaceful;
        // `can_start_raid`: false in the Nether's dimension type.
        let can_start = self.dimension != crate::world::terrain::Dimension::Nether;
        let entities = &mut self.mobs.entities;
        self.session.tick_raid_omens(entities, &self.world, peaceful, can_start);
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            bot.agent.session.tick_raid_omens(entities, &self.world, peaceful, can_start);
        }
        let hero = |s: &voxelcraft::session::PlayerSession| {
            s.vitals.effects.get(Effect::HeroOfTheVillage).map(|e| (s.id, e.amplifier))
        };
        let mut heroes: Vec<_> = hero(&self.session).into_iter().collect();
        heroes.extend(self.agents.players.values().filter(|b| b.active).filter_map(|b| hero(&b.agent.session)));
        self.mobs.entities.raids.heroes = heroes;
    }

    /// Victory rewards and horn calls from this tick's raids.
    pub(super) fn apply_raid_outcomes(&mut self) {
        for outcome in std::mem::take(&mut self.mobs.entities.raids.outcomes) {
            match outcome {
                // `Raid.tick` on victory: Hero of the Village, then
                // `awardStat(Stats.RAID_WIN)`.
                Outcome::Hero { player, amplifier } => {
                    if player == self.session.id {
                        self.session.crown_hero(amplifier);
                        self.session.stats.award(Stats::RAID_WIN);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.session.crown_hero(amplifier);
                        bot.agent.session.stats.award(Stats::RAID_WIN);
                    }
                }
                Outcome::Horn { player, at } if player == self.session.id => {
                    let sound = crate::audio::sounds::Sound::Raid(Cue::Horn);
                    self.audio.play_in(crate::audio::SoundSource::Neutral, sound, Some(at), 1.0, (1.0, 1.0));
                }
                // Any other player's horn goes to that LAN guest with its
                // next state (issue #98).
                Outcome::Horn { player, at } => self.lan.horns.push((player, at)),
                // `Raid.absorbRaidOmen`: `awardStat(Stats.RAID_TRIGGER)`.
                // (Advancements are not modelled.)
                Outcome::Triggered { player } => {
                    if player == self.session.id {
                        self.session.stats.award(Stats::RAID_TRIGGER);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        bot.agent.session.stats.award(Stats::RAID_TRIGGER);
                    }
                }
            }
        }
    }
}
