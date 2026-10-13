//! Game-side farming: trampling farmland and pick-block items. Tilling and
//! bone meal are shared session uses; growth happens in `world::growth`.

use glam::DVec3;

use crate::world::block::Block;

use super::Game;

impl Game {
    /// Landing on farmland from more than half a block may trample it back
    /// to dirt (the chance grows with the fall), popping off its crop.
    pub(super) fn trample(&mut self, fallen: f64) {
        if fallen <= 0.5 {
            return;
        }
        let below = (self.session.player.pos - DVec3::Y * 0.1).floor().as_ivec3();
        if !self.world.get_block(below).is_some_and(Block::is_farmland) {
            return;
        }
        let nanos = self.started.elapsed().subsec_nanos() as u64;
        if (crate::world::noise::hash_f(below.x, below.y, below.z, nanos) as f64) < fallen - 0.5 {
            self.world.set_block(below, Block::DIRT);
        }
    }
}
