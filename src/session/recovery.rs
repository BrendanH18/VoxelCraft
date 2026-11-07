//! The recovery compass's target: Java 26.3 `ServerPlayer.die` stores
//! `lastDeathLocation` (the dimension and `blockPosition()` where the
//! player died); it is saved with the player and survives respawns.

use super::PlayerSession;
use crate::world::World;

impl PlayerSession {
    /// Records the death location the first tick the player is dead, on
    /// every input path (desktop, controller, CLI agents and LAN players
    /// all tick their lodestone compasses through here).
    pub fn record_death(&mut self, world: &World) {
        if !self.vitals.is_dead() {
            self.death_recorded = false;
            return;
        }
        if !self.death_recorded {
            self.death_recorded = true;
            self.last_death = Some((self.player.pos.floor().as_ivec3(), world.generator.dimension));
        }
    }

    /// `x,y,z,dimension` for the level file.
    pub fn last_death_to_string(&self) -> Option<String> {
        self.last_death.map(|(p, d)| format!("{},{},{},{}", p.x, p.y, p.z, d.name()))
    }

    /// Restores [`PlayerSession::last_death_to_string`]; a dead player
    /// reloaded keeps the death already recorded.
    pub fn load_last_death(&mut self, text: &str) {
        let parts: Vec<&str> = text.split(',').collect();
        if let [x, y, z, d] = parts[..]
            && let (Ok(x), Ok(y), Ok(z), Some(d)) =
                (x.parse(), y.parse(), z.parse(), crate::world::terrain::Dimension::from_name(d))
        {
            self.last_death = Some((glam::IVec3::new(x, y, z), d));
            self.death_recorded = self.vitals.is_dead();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::PlayerId;
    use crate::world::terrain::{Dimension, Generator};
    use glam::{DVec3, IVec3};
    use std::sync::Arc;

    #[test]
    fn death_is_recorded_once_and_reloads() {
        let world = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 1);
        let mut s = PlayerSession::new(PlayerId(4), DVec3::new(10.7, 64.2, -3.4));
        s.record_death(&world);
        assert_eq!(s.last_death, None);
        s.vitals.death = Some("fell".into());
        s.record_death(&world);
        assert_eq!(s.last_death, Some((IVec3::new(10, 64, -4), Dimension::Overworld)));
        // Moving while dead doesn't move the mark.
        s.player.pos = DVec3::new(50.0, 70.0, 50.0);
        s.record_death(&world);
        assert_eq!(s.last_death.unwrap().0, IVec3::new(10, 64, -4));
        let text = s.last_death_to_string().unwrap();
        assert_eq!(text, "10,64,-4,overworld");
        let mut t = PlayerSession::new(PlayerId(5), DVec3::ZERO);
        t.load_last_death(&text);
        assert_eq!(t.last_death, s.last_death);
        // After respawning, the next death replaces it.
        s.vitals.death = None;
        s.record_death(&world);
        s.vitals.death = Some("again".into());
        s.record_death(&world);
        assert_eq!(s.last_death.unwrap().0, IVec3::new(50, 70, 50));
        t.load_last_death("garbage");
        assert_eq!(t.last_death.unwrap().0, IVec3::new(10, 64, -4));
    }
}
