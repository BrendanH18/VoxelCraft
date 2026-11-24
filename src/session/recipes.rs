//! v0.18 Track 5: the recipe book's per-tick unlock criteria for one player.

use glam::IVec3;

use super::PlayerSession;
use crate::rules::GameMode;
use crate::world::World;

impl PlayerSession {
    /// Checks this tick's recipe unlock criteria (`crafting::book::state`):
    /// the inventory slots that changed, and whether the player's box is
    /// inside water (`enter_block`, from `Entity.checkInsideBlocks`, which
    /// spectators skip).
    pub fn tick_recipe_book(&mut self, world: &World) {
        let in_water = self.mode != GameMode::Spectator && self.inside_water(world);
        self.recipe_book.tick(&self.inventory, in_water);
    }

    /// Whether a water cell (not a bubble column or waterlogged block, which
    /// are other block states in Java) overlaps the player's box, shrunk by
    /// Java's 1.0E-5 margin.
    fn inside_water(&self, world: &World) -> bool {
        let (min, max) = self.player.collision_shape().aabb(self.player.pos);
        let lo = (min + 1.0e-5).floor().as_ivec3();
        let hi = (max - 1.0e-5).floor().as_ivec3();
        (lo.x..=hi.x).any(|x| {
            (lo.y..=hi.y).any(|y| {
                (lo.z..=hi.z).any(|z| {
                    world
                        .get_block(IVec3::new(x, y, z))
                        .is_some_and(|b| b.is_water() && !crate::world::bubble::is_column(b))
                })
            })
        })
    }
}
