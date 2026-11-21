//! v0.16 Track F glue: conduits tick with the world clock; their Conduit
//! Power lands on every player in range in water or rain, a hunting conduit
//! strikes its target, and their sounds play.
use glam::DVec3;

use crate::audio::sounds::Sound;
use voxelcraft::rules::GameMode;
use voxelcraft::session::PlayerSession;
use voxelcraft::simulation::effects::{Active, Effect};
use voxelcraft::world::conduit::{self, Cue, Event};

use super::Game;

/// `applyEffects` on one player: Conduit Power 260 ticks, ambient and
/// visible, when the player's block is closer than `range` to the conduit
/// and the player is in water or rain.
fn give_power(s: &mut PlayerSession, world: &crate::world::World, pos: glam::IVec3, range: i32) {
    if s.vitals.is_dead() || s.mode == GameMode::Spectator {
        return;
    }
    if !conduit::closer_than(pos, s.player.pos.floor().as_ivec3(), f64::from(range)) || !s.in_water_or_rain(world) {
        return;
    }
    s.vitals.give_effect(Active::new(Effect::ConduitPower, 0, conduit::EFFECT_TICKS).with_flags(true, true, true));
}

impl Game {
    /// Every conduit in a loaded chunk, on the world's game time, then what
    /// they did.
    pub(super) fn tick_conduits(&mut self) {
        let time = self.world.maps.game_ticks;
        let mut seed = time ^ 0xC0_D017;
        let roll = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % 40
        };
        self.world.tick_conduits(time, roll);
        for event in self.world.take_conduit_events() {
            match event {
                Event::Power { pos, range } => {
                    give_power(&mut self.session, &self.world, pos, range);
                    for bot in self.agents.players.values_mut().filter(|b| b.active) {
                        give_power(&mut bot.agent.session, &self.world, pos, range);
                    }
                }
                Event::Hunt { pos, hunting } => {
                    let current = self.world.conduit(pos).and_then(|c| c.target);
                    let (target, struck) = self.mobs.entities.conduit_hunt(&self.world, pos, hunting, current);
                    self.world.set_conduit_target(pos, target);
                    if let Some(at) = struck {
                        self.audio.play(Sound::Conduit(Cue::AttackTarget), Some(at), 1.0, (1.0, 1.0));
                    }
                }
                Event::Sound(cue, p) => {
                    self.audio.play(Sound::Conduit(cue), Some(p.as_dvec3() + DVec3::splat(0.5)), 1.0, (1.0, 1.0));
                }
            }
        }
    }
}
