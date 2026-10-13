//! Presentation for the shared bow and crossbow (`session::ranged`); drawing
//! and shooting run through the acting player's hands (`hands::Actor`).

use super::Game;

impl Game {
    /// Current draw strength for the HUD and field of view, if drawing.
    pub(super) fn bow_power(&self) -> Option<f32> {
        self.session.draw_power(self.body.actions.selected)
    }
}
