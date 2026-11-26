//! Game-side farming: trampling farmland and pick-block items. Tilling and
//! bone meal are shared session uses; growth happens in `world::growth`.

use super::Game;

impl Game {
    /// Landing on farmland from more than half a block may trample it back
    /// to dirt (`FarmlandBlock.fallOn`, see [`crate::world::World::player_landed`]).
    pub(super) fn trample(&mut self, fallen: f64) {
        let shape = self.session.player.collision_shape();
        let source = self.session.event_source();
        self.world.player_landed(self.session.player.pos, shape, fallen, source);
    }
}
