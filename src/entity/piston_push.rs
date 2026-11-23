//! v0.17 Track B: piston pushes for the simulated entity collections
//! (`world::piston`). Every entity is `PushReaction.PUSH_PULL` in Java 26.3
//! except markers, displays, interactions, area effect clouds, ominous item
//! spawners and marker armor stands; of those VoxelCraft simulates the
//! lingering clouds and the item spawners, which are skipped here.
use glam::DVec3;

use crate::world::{
    World,
    piston::{PushBody, push_body},
};

impl super::Entities {
    /// Moves and launches mobs, items, orbs, carts and primed TNT the way
    /// the last tick's moving blocks require.
    pub fn apply_piston_pushes(&mut self, world: &World) {
        if world.piston_pushes().is_empty() {
            return;
        }
        let body = |pos: &mut DVec3, vel: &mut DVec3, per_tick: f64, shape, on_ground: Option<bool>| {
            push_body(world, PushBody { pos, vel, vel_per_tick: per_tick, shape, on_ground, noclip: false });
        };
        for m in &mut self.mobs {
            let shape = m.shape();
            body(&mut m.pos, &mut m.vel, 20.0, shape, Some(m.on_ground));
        }
        for i in &mut self.items {
            body(&mut i.pos, &mut i.vel, 20.0, super::item::SHAPE, None);
        }
        for o in &mut self.orbs {
            body(&mut o.pos, &mut o.vel, 20.0, super::orb::SHAPE, None);
        }
        for c in &mut self.minecarts {
            let shape = c.shape();
            body(&mut c.pos, &mut c.vel, 1.0, shape, None);
        }
        for t in &mut self.tnt {
            body(&mut t.pos, &mut t.vel, 20.0, super::tnt::SHAPE, None);
        }
    }
}
