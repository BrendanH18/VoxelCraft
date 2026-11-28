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
        for a in &mut self.arrows {
            body(&mut a.pos, &mut a.vel, 20.0, super::projectile::ARROW_SHAPE, None);
        }
        for t in &mut self.tridents {
            let noclip = t.no_physics;
            let shape = super::projectile::ARROW_SHAPE;
            push_body(
                world,
                PushBody { pos: &mut t.pos, vel: &mut t.vel, vel_per_tick: 1.0, shape, on_ground: None, noclip },
            );
        }
        // `EndCrystal.tick` never moves it, so a slime block's launch does
        // nothing; the push itself does.
        for c in &mut self.crystals.list {
            let mut vel = DVec3::ZERO;
            let shape = crate::physics::Shape::new(super::crystal::WIDTH / 2.0, super::crystal::HEIGHT);
            body(&mut c.pos, &mut vel, 20.0, shape, None);
        }
        self.push_projectiles(world);
        let mut broken = Vec::new();
        for (i, d) in self.decor.iter_mut().enumerate() {
            if d.is_hanging() {
                if crate::world::piston::pushes_box(world, d.aabb()) {
                    broken.push(i);
                }
                continue;
            }
            let mut vel = DVec3::Y * d.vel_y;
            body(&mut d.pos, &mut vel, 1.0, super::decor::STAND_SHAPE, None);
            d.vel_y = vel.y;
        }
        for i in broken.into_iter().rev() {
            self.piston_break_decor(i);
        }
    }

    /// v1.0 final F9: the other projectiles and the evoker's fangs. None of
    /// them overrides `getPistonPushReaction` or `move` in Java 26.3, so
    /// `moveEntityByPiston` moves each through `Entity.move(PISTON)` with
    /// its `EntityType` box (positions are Java's, the box's bottom centre)
    /// and a slime block sets its `deltaMovement` on the push axis. Shulker
    /// bullets are `noPhysics` and move through blocks.
    fn push_projectiles(&mut self, world: &World) {
        use crate::physics::Shape;
        let push = |pos: &mut DVec3, vel: &mut DVec3, per_tick: f64, size: (f64, f64), noclip: bool| {
            let shape = Shape::new(size.0 / 2.0, size.1);
            push_body(world, PushBody { pos, vel, vel_per_tick: per_tick, shape, on_ground: None, noclip });
        };
        // `EntityType` sizes: snowball, eggs, pearls, potions, bottles, eyes,
        // rockets and bobbers 0.25; wind charges, small fireballs, wither
        // skulls and shulker bullets 0.3125; large and dragon fireballs 1.
        const SMALL: (f64, f64) = (0.25, 0.25);
        const CHARGE: (f64, f64) = (0.3125, 0.3125);
        const LARGE: (f64, f64) = (1.0, 1.0);
        for p in &mut self.pearls {
            push(&mut p.pos, &mut p.vel, 20.0, SMALL, false);
        }
        for p in &mut self.potions {
            push(&mut p.pos, &mut p.vel, 20.0, SMALL, false);
        }
        for t in &mut self.thrown {
            let size = t.kind.size();
            push(&mut t.pos, &mut t.vel, 20.0, (size, size), false);
        }
        for b in &mut self.bobbers {
            push(&mut b.pos, &mut b.vel, 20.0, SMALL, false);
        }
        for e in &mut self.eyes {
            push(&mut e.pos, &mut e.vel, 1.0, SMALL, false);
        }
        for f in &mut self.fireballs {
            let size = f.size();
            push(&mut f.pos, &mut f.vel, 20.0, (size, size), false);
        }
        for r in &mut self.fireworks {
            push(&mut r.pos, &mut r.vel, 1.0, SMALL, false);
        }
        for s in &mut self.wither_skulls {
            push(&mut s.pos, &mut s.motion, 1.0, CHARGE, false);
        }
        for b in &mut self.shulker_bullets {
            push(&mut b.pos, &mut b.vel, 1.0, CHARGE, true);
        }
        let dragon_fireballs = self.fight.iter_mut().flat_map(|f| f.fireballs.iter_mut());
        for f in dragon_fireballs.chain(self.wild_dragons.fireballs.iter_mut()) {
            push(&mut f.pos, &mut f.vel, 1.0, LARGE, false);
        }
        // `EvokerFangs` never moves itself, so a launch changes nothing.
        for f in &mut self.fangs {
            let mut still = DVec3::ZERO;
            push(&mut f.pos, &mut still, 20.0, (0.5, 0.8), false);
        }
    }
}

#[cfg(test)]
mod tests;
