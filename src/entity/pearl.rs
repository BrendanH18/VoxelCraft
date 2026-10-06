//! Thrown ender pearls, like Java's: they fly at 1.5 blocks a tick under
//! gravity and drag, and where they hit a block or a mob their thrower
//! teleports there, taking 5 damage.

use glam::DVec3;

use super::{EntityEvent, Mob, PlayerId, Rng};
use crate::physics::BlockSource;

/// Launch speed (Java's 1.5 blocks a tick).
pub const SPEED: f64 = 30.0;
/// Seconds before the same player can throw another (Java's 20 ticks).
pub const COOLDOWN: f32 = 1.0;
/// Damage the thrower takes on landing.
pub const DAMAGE: f32 = 5.0;
/// Blocks per second squared (Java's 0.03 per tick²).
const GRAVITY: f64 = 12.0;
/// Longest move per collision check (blocks).
const STEP: f64 = 0.2;
/// Seconds of flight before a pearl that never lands is dropped.
const MAX_FLIGHT: f32 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pearl {
    pub owner: PlayerId,
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    age: f32,
}

impl Pearl {
    /// Thrown from `eye` along `dir`, carrying the thrower's velocity
    /// `carry`, with Java's slight random spread.
    pub fn thrown(owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3, rng: &mut Rng) -> Self {
        let mut spread = || rng.range(-1.0, 1.0) as f64 * 0.0075 * 1.7;
        let aim = dir.normalize_or(DVec3::X) + DVec3::new(spread(), spread(), spread());
        let pos = eye - DVec3::Y * 0.1;
        Self { owner, pos, previous_pos: pos, vel: aim.normalize() * SPEED + carry, age: 0.0 }
    }

    /// Moves the pearl; returns `false` once it has landed (pushing a
    /// [`EntityEvent::PearlLanded`]) or is lost.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        mobs: &mut [Mob],
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        let ticks = dt * 20.0;
        let water = world.block(self.pos.floor().as_ivec3()).is_some_and(|b| b.is_water());
        self.vel *= if water { 0.8f64 } else { 0.99 }.powf(ticks);
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            let cell = next.floor().as_ivec3();
            // Gateways take the thrower through, like Java's.
            if world.block(cell) == Some(crate::world::block::Block::END_GATEWAY) {
                events.push(EntityEvent::PearlGateway { owner: self.owner, cell });
                return false;
            }
            if world.block(cell).is_some_and(|b| b.is_solid()) {
                // Land just outside the block it hit.
                events.push(EntityEvent::PearlLanded { owner: self.owner, pos: self.pos });
                return false;
            }
            self.pos = next;
            if let Some(mob) = mobs.iter_mut().find(|m| {
                let (min, max) = m.aabb();
                m.alive() && next.cmpge(min).all() && next.cmple(max).all()
            }) {
                // Java's pearl hit deals no damage but still knocks back.
                let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 2.0 + DVec3::Y * 2.0;
                mob.damage(0.0, Some(push), rng);
                events.push(EntityEvent::PearlLanded { owner: self.owner, pos: mob.pos });
                return false;
            }
        }
        self.age < MAX_FLIGHT && self.pos.y > -128.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MobKind;
    use crate::physics::test_util::Grid;
    use crate::world::block::Block;
    use glam::IVec3;

    fn fly(pearl: &mut Pearl, world: &Grid, mobs: &mut [Mob]) -> Option<(PlayerId, DVec3)> {
        let mut rng = Rng::new(1);
        let mut events = Vec::new();
        for _ in 0..400 {
            if !pearl.update(0.05, world, mobs, &mut rng, &mut events) {
                break;
            }
        }
        events.iter().find_map(|e| match *e {
            EntityEvent::PearlLanded { owner, pos } => Some((owner, pos)),
            _ => None,
        })
    }

    #[test]
    fn pearls_arc_onto_the_ground_like_java() {
        let world = Grid::flat(10);
        let mut rng = Rng::new(2);
        // Thrown 45° up from eye height over flat ground, Java's pearl
        // (1.5 blocks a tick, 0.99 drag, 0.03 gravity) lands about 50 away.
        let dir = DVec3::new(1.0, 1.0, 0.0).normalize();
        let mut pearl = Pearl::thrown(PlayerId(3), DVec3::new(0.5, 11.62, 0.5), dir, DVec3::ZERO, &mut rng);
        let (owner, pos) = fly(&mut pearl, &world, &mut []).expect("lands");
        assert_eq!(owner, PlayerId(3));
        assert!((45.0..56.0).contains(&pos.x), "landed at {pos}");
        assert!((pos.y - 10.0).abs() < 0.5, "on the ground: {pos}");
    }

    #[test]
    fn pearls_stop_at_walls_and_mobs() {
        let mut world = Grid::flat(10);
        for y in 10..20 {
            world.set(IVec3::new(6, y, 0), Block::STONE);
        }
        let mut rng = Rng::new(3);
        let mut pearl = Pearl::thrown(PlayerId::HOST, DVec3::new(0.5, 11.6, 0.5), DVec3::X, DVec3::ZERO, &mut rng);
        let (_, pos) = fly(&mut pearl, &world, &mut []).unwrap();
        assert!(pos.x < 6.0 && pos.x > 5.0, "just short of the wall: {pos}");

        let world = Grid::flat(10);
        let mut mobs = [Mob::new(MobKind::Zombie, DVec3::new(4.5, 10.0, 0.5), 0.0)];
        let mut pearl = Pearl::thrown(PlayerId::HOST, DVec3::new(0.5, 11.0, 0.5), DVec3::X, DVec3::ZERO, &mut rng);
        let (_, pos) = fly(&mut pearl, &world, &mut mobs).unwrap();
        assert!(pos.distance(mobs[0].pos) < 1e-6, "lands on the mob");
        assert_eq!(mobs[0].health, MobKind::Zombie.max_health(), "unhurt");
    }
}
