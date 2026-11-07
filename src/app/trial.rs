//! v0.16 Track C glue: trial spawners and vaults tick with every player in
//! the world, and their Trial Omen conversions land on those players.
use voxelcraft::entity::trial::{TrialOutcome, TrialPlayer, TrialRules};
use voxelcraft::rules::GameMode;
use voxelcraft::session::PlayerSession;
use voxelcraft::simulation::effects::Effect;

use super::Game;

fn trial_player(s: &PlayerSession) -> TrialPlayer {
    TrialPlayer {
        id: s.id,
        pos: s.player.pos,
        eye: s.player.eye(),
        counts: !s.vitals.is_dead() && !matches!(s.mode, GameMode::Creative | GameMode::Spectator),
        bad_omen: s.vitals.effects.get(Effect::BadOmen).map(|e| e.amplifier),
        trial_omen: s.vitals.effects.has(Effect::TrialOmen),
    }
}

/// `transformBadOmenIntoTrialOmen` on one player.
fn take_trial_omen(s: &mut PlayerSession, ticks: u32) {
    s.vitals.effects.remove(Effect::BadOmen);
    s.vitals.apply_effect(Effect::TrialOmen, 0, ticks);
}

impl Game {
    pub(super) fn tick_trial(&mut self) {
        let mut players = vec![trial_player(&self.session)];
        players.extend(self.agents.players.values().filter(|b| b.active).map(|b| trial_player(&b.agent.session)));
        let rules = TrialRules {
            spawner_blocks: self.gamerules.bool("spawner_blocks_work"),
            spawn_mobs: self.gamerules.bool("spawn_monsters"),
            peaceful: self.difficulty == voxelcraft::simulation::difficulty::Difficulty::Peaceful,
        };
        for outcome in self.mobs.entities.tick_trial(&mut self.world, &players, rules) {
            match outcome {
                TrialOutcome::TrialOmen { player, ticks } => {
                    if player == self.session.id {
                        take_trial_omen(&mut self.session, ticks);
                    } else if let Some(bot) = self.agents.by_id_mut(player) {
                        take_trial_omen(&mut bot.agent.session, ticks);
                    }
                }
            }
        }
    }
}
