//! Wind bursts (Java 26.3 `AbstractWindCharge.explode`, `WindCharge`,
//! `BreezeWindCharge` and `ServerExplosion` with `TRIGGER_BLOCK`): a blast
//! that hurts nothing, knocks entities away and works doors, trapdoors,
//! fence gates, buttons, levers, bells and lit candles in its rays.
//!
//! [`wind_burst`] is the shared entry point: wind charges call it, and
//! the mace's Wind Burst enchantment (Track D) can call it with its own
//! radius and knockback through [`WindBurst::new`].
use glam::{DVec3, IVec3};

use super::{Entities, PlayerId};
use crate::physics::BlockSource;
use crate::world::World;

/// `WindCharge.RADIUS` and its calculator's knockback multiplier.
pub const CHARGE_RADIUS: f32 = 1.2;
pub const CHARGE_KNOCKBACK: f32 = 1.22;
/// `BreezeWindCharge.RADIUS`; the default knockback multiplier.
pub const BREEZE_RADIUS: f32 = 3.0;
pub const BREEZE_KNOCKBACK: f32 = 1.0;
/// `WindChargeItem`: shoot power 1.5 blocks a tick, inaccuracy 1, and a
/// 10-tick item cooldown.
pub const THROW_SPEED: f64 = 1.5 * 20.0;
pub const THROW_INACCURACY: f64 = 1.0;
pub const COOLDOWN_TICKS: u32 = 10;
/// `Shoot.PROJECTILE_MOVEMENT_SCALE`: a breeze's charge flies 0.7 a tick.
pub const BREEZE_SPEED: f64 = 0.7 * 20.0;
/// `AbstractWindCharge.onHitEntity`: one point of wind charge damage.
pub const HIT_DAMAGE: f32 = 1.0;
/// `LivingEntity.setIgnoreFallDamageFromCurrentImpulse`: grace ticks.
pub const IMPULSE_GRACE: u32 = 40;

/// One wind burst.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindBurst {
    pub center: DVec3,
    pub radius: f32,
    /// The calculator's knockback multiplier.
    pub knockback: f32,
    /// A player's `WIND_CHARGE` (`ServerPlayer.onExplosionHit`): players
    /// it hits measure their next fall from where they were hit.
    pub resets_fall: bool,
    /// The player who threw it, for the mobs it angers.
    pub owner: Option<PlayerId>,
}

impl WindBurst {
    pub const fn new(center: DVec3, radius: f32, knockback: f32, resets_fall: bool, owner: Option<PlayerId>) -> Self {
        Self { center, radius, knockback, resets_fall, owner }
    }

    /// A player's wind charge bursting at `center`.
    pub const fn charge(center: DVec3, owner: Option<PlayerId>) -> Self {
        Self::new(center, CHARGE_RADIUS, CHARGE_KNOCKBACK, true, owner)
    }

    /// A breeze's wind charge.
    pub const fn breeze(center: DVec3) -> Self {
        Self::new(center, BREEZE_RADIUS, BREEZE_KNOCKBACK, false, None)
    }

    /// `ServerExplosion.hurtEntities`' push on an entity whose feet are at
    /// `pos`, eye at `eye_y` and box `min..max`: blocks per second, or
    /// `None` out of reach. `resistance` is the explosion knockback
    /// resistance of living entities.
    pub fn push<W: BlockSource + ?Sized>(
        &self,
        world: &W,
        pos: DVec3,
        eye_y: f64,
        (min, max): (DVec3, DVec3),
        resistance: f64,
    ) -> Option<DVec3> {
        let diameter = f64::from(self.radius) * 2.0;
        let d = pos.distance(self.center) / diameter;
        if d > 1.0 {
            return None;
        }
        let dir = DVec3::new(pos.x - self.center.x, eye_y - self.center.y, pos.z - self.center.z);
        if dir.length_squared() == 0.0 {
            return None;
        }
        let exposure = f64::from(crate::world::explosion::seen_percent(world, self.center, min, max));
        let knockback = (1.0 - d) * exposure * f64::from(self.knockback) * (1.0 - resistance.clamp(0.0, 1.0));
        Some(dir.normalize() * knockback * crate::knockback::TICK_RATE)
    }
}

impl Entities {
    /// Pushes every living mob the burst reaches; nothing is hurt. Items,
    /// orbs and decorations ignore wind (`shouldAffectBlocklikeEntities` is
    /// false for wind charge sources).
    pub fn wind_blast<W: BlockSource + ?Sized>(&mut self, world: &W, burst: &WindBurst) {
        let reach = f64::from(burst.radius) * 2.0;
        for m in &mut self.mobs {
            if !m.alive() || m.pos.distance(burst.center) > reach {
                continue;
            }
            let eye = m.pos.y + m.shape().height * 0.85;
            if let Some(push) = burst.push(world, m.pos, eye, m.aabb(), 0.0) {
                m.vel += push;
                m.on_ground = false;
            }
        }
        // Java's gust emitters (small for a player's charge, large for a
        // breeze's): a ring of white puffs here.
        let mut gust = crate::particles::Burst::new(
            crate::particles::Kind::Poof,
            burst.center,
            if burst.radius >= 2.0 { 24 } else { 12 },
        );
        gust.spread = DVec3::splat(f64::from(burst.radius) * 0.5);
        gust.velocity_spread = DVec3::splat(0.15);
        self.particles.push(crate::particles::Request::Burst(gust));
    }
}

impl World {
    /// `TRIGGER_BLOCK` on the blast's cells: wind charges open and close
    /// wooden and copper doors, trapdoors and fence gates, press buttons,
    /// flip levers, ring bells and put out candles. Nothing breaks.
    pub fn wind_trigger(&mut self, burst: &WindBurst) -> usize {
        use crate::world::redstone_blocks::{self as r, Component};
        let cells = self.exploded_cells(burst.center, burst.radius);
        let mut triggered = 0;
        let mut doors_done: Vec<IVec3> = Vec::new();
        for p in cells {
            let Some(b) = self.get_block(p) else { continue };
            if b.base() == crate::world::block::Block::BELL {
                triggered += usize::from(self.ring_bell(p));
                continue;
            }
            if crate::world::colour_blocks::candle::is_lit(b) {
                triggered += usize::from(self.extinguish_candle(p));
                continue;
            }
            match r::component(b) {
                Some(Component::Trapdoor { iron: false, .. } | Component::Lever { .. })
                | Some(Component::Button { on: false, .. }) => {
                    triggered += usize::from(self.use_redstone(p));
                    continue;
                }
                Some(Component::IronDoor { .. } | Component::Trapdoor { iron: true, .. }) => continue,
                _ => {}
            }
            if b.is_door() || matches!(b.shaped(), Some(crate::world::block::Shaped::Gate { .. })) {
                // Both halves of a door share one toggle.
                let lower = if matches!(b.shaped(), Some(crate::world::block::Shaped::Door { upper: true, .. })) {
                    p - IVec3::Y
                } else {
                    p
                };
                if doors_done.contains(&lower) {
                    continue;
                }
                doors_done.push(lower);
                let forward = (p.as_dvec3() + DVec3::splat(0.5) - burst.center).as_vec3();
                if crate::session::interact::toggle_door(self, p, forward).is_some() {
                    triggered += 1;
                }
            }
        }
        triggered
    }
}

/// The whole burst: block triggers, mobs and the given players. Players
/// in Spectator, or flying in Creative, are not pushed.
pub fn wind_burst(
    world: &mut World,
    entities: &mut Entities,
    sessions: &mut [&mut crate::session::PlayerSession],
    burst: &WindBurst,
) {
    entities.wind_blast(world, burst);
    for s in sessions.iter_mut() {
        if let Some(push) = player_push(world, s, burst) {
            s.player.vel += push;
            s.player.on_ground = false;
            if burst.resets_fall {
                s.vitals.wind_impulse(s.player.pos.y, IMPULSE_GRACE);
            }
        }
    }
    world.wind_trigger(burst);
}

/// The push a player takes, if any.
pub fn player_push<W: BlockSource + ?Sized>(
    world: &W,
    s: &crate::session::PlayerSession,
    burst: &WindBurst,
) -> Option<DVec3> {
    use crate::rules::GameMode;
    if s.mode == GameMode::Spectator || (s.mode.is_creative() && s.player.flying) || s.vitals.is_dead() {
        return None;
    }
    let p = &s.player;
    let resistance = s.inventory.explosion_knockback_resistance();
    burst.push(world, p.pos, p.eye().y, p.collision_shape().aabb(p.pos), resistance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Mob, MobKind};
    use crate::physics::test_util::Grid;
    use crate::world::redstone_blocks as r;

    #[test]
    fn a_burst_pushes_away_without_hurting() {
        let world = Grid::flat(0);
        let mut e = Entities::new(1);
        e.mobs.push(Mob::new(MobKind::Zombie, DVec3::new(1.5, 0.0, 0.5), 0.0));
        let burst = WindBurst::charge(DVec3::new(0.5, 0.5, 0.5), None);
        e.wind_blast(&world, &burst);
        let m = &e.mobs[0];
        assert_eq!(m.health, MobKind::Zombie.max_health());
        assert!(m.vel.x > 0.0 && m.vel.y > 0.0, "{}", m.vel);
        // Out of reach (2 x 1.2 blocks) nothing moves.
        let far = WindBurst::charge(DVec3::new(10.5, 0.5, 0.5), None);
        let before = e.mobs[0].vel;
        e.wind_blast(&world, &far);
        assert_eq!(e.mobs[0].vel, before);
        // A breeze's burst is wider (radius 3) at multiplier 1.
        let b = WindBurst::breeze(DVec3::ZERO);
        assert_eq!((b.radius, b.knockback, b.resets_fall), (3.0, 1.0, false));
        assert_eq!(WindBurst::charge(DVec3::ZERO, None).knockback, 1.22);
    }

    #[test]
    fn bursts_flip_levers_and_open_wooden_trapdoors() {
        let g = std::sync::Arc::new(crate::world::terrain::Generator::new(4));
        let mut w = World::new_headless(g, Default::default(), 2);
        for x in -1..=1 {
            for z in -1..=1 {
                w.load_test_column(glam::IVec2::new(x, z), 63);
            }
        }
        let lever = IVec3::new(4, 64, 4);
        let trapdoor = IVec3::new(5, 64, 4);
        w.set_block(lever, r::lever(0, false));
        let closed = r::trapdoor(crate::world::block::Facing::North, false, false, false);
        w.set_block(trapdoor, closed);
        let floor = w.get_block(IVec3::new(4, 63, 4));
        let n = w.wind_trigger(&WindBurst::charge(DVec3::new(4.9, 64.5, 5.2), None));
        assert!(n >= 2, "{n}");
        assert_eq!(r::component(w.get_block(lever).unwrap()), Some(r::Component::Lever { mount: 0, on: true }));
        assert!(matches!(
            r::component(w.get_block(trapdoor).unwrap()),
            Some(r::Component::Trapdoor { open: true, .. })
        ));
        // Nothing breaks.
        assert_eq!(w.get_block(IVec3::new(4, 63, 4)), floor);
    }

    #[test]
    fn wind_impulse_spares_falls_that_end_above_the_hit() {
        use crate::simulation::survival::Vitals;
        let mut v = Vitals::default();
        v.wind_impulse(64.0, IMPULSE_GRACE);
        assert_eq!(v.impulse, Some(64.0));
        assert_eq!(v.impulse_grace, IMPULSE_GRACE);
    }
}
