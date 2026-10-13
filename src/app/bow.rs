//! Presentation for the shared bow and crossbow (`session::ranged`).

use crate::audio::sounds::Sound;
use voxelcraft::session::ranged::{Ranged, Shot};

use super::Game;

impl Game {
    /// Right-click with a bow or crossbow: start drawing, or fire a charged
    /// crossbow. Returns whether it did.
    pub(super) fn start_draw(&mut self) -> bool {
        let shot = self.session.start_draw(self.actions.selected, &self.world, &mut self.mobs.entities);
        shot.inspect(|s| self.present_shot(*s)).is_some()
    }

    /// One tick of holding use: keep drawing, or load a crossbow.
    pub(super) fn update_bow(&mut self, acting: bool) {
        if let Some(shot) = self.session.tick_draw(self.actions.selected, acting) {
            self.present_shot(shot);
        }
    }

    /// Releasing right-click looses an arrow if the bow was drawn enough.
    pub(super) fn release_bow(&mut self) {
        if let Some(shot) = self.session.release_draw(self.actions.selected, &mut self.mobs.entities) {
            self.present_shot(shot);
        }
    }

    /// Current draw strength for the HUD and field of view, if drawing.
    pub(super) fn bow_power(&self) -> Option<f32> {
        self.session.draw_power(self.actions.selected)
    }

    fn present_shot(&mut self, shot: Shot) {
        match shot.ranged {
            Ranged::Drawing => {}
            Ranged::CrossbowLoaded { from } => self.audio.play(Sound::Bow, Some(from), 0.5, (0.55, 0.65)),
            Ranged::CrossbowFired { from } => self.audio.play(Sound::Bow, Some(from), 0.9, (1.25, 1.35)),
            Ranged::ArrowShot { from, power } => {
                self.audio.play(Sound::Bow, Some(from), 0.8, (1.0 + 0.2 * (1.0 - power), 1.1 + 0.2 * (1.0 - power)));
            }
        }
        if let Some(item) = shot.tool_broken {
            self.show_popup(&format!("{} broke", super::capitalize(item.name())));
        }
    }
}
