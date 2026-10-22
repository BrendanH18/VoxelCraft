//! Client visual tick: consume shared gameplay requests and sample animation
//! ticks around the cameras. One pool is shared by every split-screen view.

use super::Game;
use crate::particles::Request;

impl Game {
    pub(super) fn tick_particles(&mut self) {
        let setting = self.settings.particles;
        self.particles.tick(&self.world, setting);
        let mut centers = [self.session.player.pos; super::split::MAX_VIEWS];
        let mut count = 1;
        for (_, bot) in self.followed() {
            centers[count] = bot.agent.session.player.pos;
            count += 1;
        }
        let centers = &centers[..count];
        let visible = |request: &Request| {
            let (pos, radius) = match request {
                Request::Particle(p) => (p.pos, 128.0),
                Request::Break { cell, .. } | Request::Hit { cell, .. } => (cell.as_dvec3(), 128.0),
                Request::Burst(b) | Request::Tracking(b) => (b.pos, if b.forced { 128.0 } else { 32.0 }),
                Request::Explosion { pos, .. } => (*pos, 128.0),
                Request::EyeBreak { pos } | Request::Honey { pos, .. } => (*pos, 48.0),
            };
            centers.iter().any(|center| pos.distance_squared(*center) <= radius * radius)
        };
        let lan = self.lan.host.is_some() && self.lan.has_peers();
        while let Some(request) = self.world.particles.pop() {
            if lan && let Some(j) = request.to_json() {
                self.lan.effects.push((request.pos(), j));
            }
            if visible(&request) {
                if let Request::Honey { pos, sound: true, .. } = request {
                    self.audio.play(crate::audio::Sound::HoneySlide, Some(pos), 1.0, (1.0, 1.0));
                }
                self.particles.request(request, setting, &self.world);
            }
        }
        while let Some(request) = self.mobs.entities.particles.pop() {
            if lan && let Some(j) = request.to_json() {
                self.lan.effects.push((request.pos(), j));
            }
            if visible(&request) {
                if let Request::Honey { pos, sound: true, .. } = request {
                    self.audio.play(crate::audio::Sound::HoneySlide, Some(pos), 1.0, (1.0, 1.0));
                }
                self.particles.request(request, setting, &self.world);
            }
        }
        for (i, &center) in centers.iter().enumerate() {
            // Nearby views share animation sampling; separated views each
            // get a local sample without allocating a second particle pool.
            if !centers[..i].iter().any(|p| p.distance_squared(center) < 16.0 * 16.0) {
                self.particles.animate_blocks(&self.world, center, setting);
                self.particles.rain(
                    &self.world,
                    center,
                    self.weather.strength,
                    setting,
                    self.settings.enhanced_graphics,
                );
            }
        }
        self.particles.entities(&self.world, &self.mobs.entities, centers, setting);
        self.particles.effects(self.session.player.pos, &self.session.vitals.effects, setting);
        for bot in self.agents.players.values().filter(|b| b.active && !b.agent.session.vitals.is_dead()) {
            if centers.iter().any(|p| p.distance_squared(bot.agent.session.player.pos) <= 32.0 * 32.0) {
                self.particles.effects(bot.agent.session.player.pos, &bot.agent.session.vitals.effects, setting);
            }
        }
    }
}
