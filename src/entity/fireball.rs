//! Blaze fireballs, like Java's small fireballs: they speed up along their
//! heading toward 1.9 blocks a tick, set the player they hit alight for
//! 5 s after 5 damage, and light a fire where they hit a block.

use glam::{DVec3, Vec3};

use super::{Ctx, EntityEvent};
use crate::physics::BlockSource;

/// Damage to the player hit by a blaze fireball, and seconds they burn.
pub const DAMAGE: f32 = 5.0;
pub const BURN: f32 = 5.0;
/// Ghast fireballs explode with this power (Java's large fireball).
pub const GHAST_POWER: f32 = 1.0;
/// Top speed in blocks per second (Java's 0.1 per tick acceleration with
/// 0.95 inertia), and the speed at launch. Ghast fireballs stay slower
/// so a player can punch them back.
const TOP_SPEED: f64 = 38.0;
const LARGE_TOP_SPEED: f64 = 16.0;
const LAUNCH_SPEED: f64 = 2.0;
/// Longest move per collision check (blocks).
const STEP: f64 = 0.2;
const LIFETIME: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fireball {
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    vel: DVec3,
    /// Unit heading it accelerates along.
    dir: DVec3,
    age: f32,
    /// Ghast fireball: explodes and can be deflected.
    large: bool,
    /// A player punched this fireball, so the blast counts as their kill.
    deflected: bool,
}

impl Fireball {
    /// Launches from `from` with a normalized heading, falling back to +X for zero.
    pub fn new(from: DVec3, dir: DVec3) -> Self {
        let dir = dir.normalize_or(DVec3::X);
        Self { pos: from, previous_pos: from, vel: dir * LAUNCH_SPEED, dir, age: 0.0, large: false, deflected: false }
    }

    /// A ghast's explosive fireball.
    pub fn large(from: DVec3, dir: DVec3) -> Self {
        Self { large: true, ..Self::new(from, dir) }
    }

    pub fn is_large(&self) -> bool {
        self.large
    }

    /// Sends a ghast fireball back along the puncher's look direction.
    pub fn deflect(&mut self, look: DVec3) {
        let dir = look.normalize_or(DVec3::X);
        self.dir = dir;
        self.vel = dir * 10.0;
        self.deflected = true;
        self.age = 0.0;
    }

    /// Moves the fireball; returns `false` once it has hit something or
    /// burnt out.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        mobs: &[super::Mob],
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.age += dt as f32;
        let keep = 0.95f64.powf(dt * 20.0);
        let top = if self.large { LARGE_TOP_SPEED } else { TOP_SPEED };
        self.vel = self.vel * keep + self.dir * top * (1.0 - keep);
        let delta = self.vel * dt;
        let steps = (delta.length() / STEP).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                self.impact(None, events);
                return false;
            }
            self.pos = next;
            if let Some(hit) = ctx.players.iter().find(|t| t.targetable && t.contains(next)) {
                self.impact(Some(hit), events);
                return false;
            }
            if self.large
                && mobs.iter().any(|m| {
                    if !m.alive() {
                        return false;
                    }
                    let (min, max) = m.aabb();
                    next.cmpge(min).all() && next.cmple(max).all()
                })
            {
                self.impact(None, events);
                return false;
            }
        }
        self.age < LIFETIME && self.pos.y > -64.0
    }

    /// Blaze fireballs burn whoever they touch. Ghast fireballs explode.
    fn impact(&self, player: Option<&super::Target>, events: &mut Vec<EntityEvent>) {
        if self.large {
            events.push(EntityEvent::Explosion {
                center: self.pos,
                power: GHAST_POWER,
                cause: "was fireballed by a ghast",
                credit_player: self.deflected,
            });
            return;
        }
        if let Some(hit) = player {
            let push = DVec3::new(self.vel.x, 0.0, self.vel.z).normalize_or_zero() * 2.0 + DVec3::Y * 2.0;
            events.push(EntityEvent::PlayerHit {
                player: hit.id,
                damage: DAMAGE,
                knockback: push.as_vec3(),
                cause: "was fireballed by a blaze",
            });
            events.push(EntityEvent::Ignite { player: hit.id, secs: BURN });
        }
        events.push(EntityEvent::IgniteBlock { cell: self.pos.floor().as_ivec3() });
    }

    /// Direction of travel, for drawing.
    pub fn heading(&self) -> Vec3 {
        self.dir.as_vec3()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PlayerId, Target};
    use crate::physics::test_util::Grid;
    use crate::world::terrain::Dimension;

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx { players, daylight: 0.0, spawning: false, raining: false, dimension: Dimension::Nether }
    }

    #[test]
    fn fireballs_speed_up_hit_and_ignite_players() {
        let world = Grid::flat(0);
        let player = Target::new(PlayerId(2), DVec3::new(20.5, 10.0, 0.5), true);
        let c = ctx(vec![player]);
        let mut f = Fireball::new(DVec3::new(0.5, 11.0, 0.5), DVec3::X);
        let mut events = Vec::new();
        let mut ticks = 0;
        while f.update(0.05, &world, &c, &[], &mut events) {
            ticks += 1;
            assert!(ticks < 100);
        }
        // ~20 blocks: about 1.2 s from a standing start.
        assert!((15..35).contains(&ticks), "{ticks} ticks");
        assert!(events.contains(&EntityEvent::Ignite { player: PlayerId(2), secs: BURN }));
        assert!(events.iter().any(|e| matches!(e, EntityEvent::PlayerHit { damage: DAMAGE, .. })));
        // Creative (untargetable) players are flown past.
        let c = ctx(vec![Target::new(PlayerId(2), DVec3::new(20.5, 10.0, 0.5), false)]);
        let mut f = Fireball::new(DVec3::new(0.5, 11.0, 0.5), DVec3::X);
        let mut events = Vec::new();
        while f.update(0.05, &world, &c, &[], &mut events) {}
        assert!(events.is_empty());
    }

    #[test]
    fn fireballs_light_fires_where_they_land() {
        let world = Grid::flat(10);
        let mut f = Fireball::new(DVec3::new(0.5, 12.5, 0.5), DVec3::new(1.0, -1.0, 0.0));
        let mut events = Vec::new();
        while f.update(0.05, &world, &ctx(vec![]), &[], &mut events) {}
        let Some(EntityEvent::IgniteBlock { cell }) = events.first().copied() else { panic!("{events:?}") };
        assert_eq!(cell.y, 10, "the air cell above the floor");
        assert!(cell.x >= 2 && cell.x <= 3, "{cell}");
    }

    #[test]
    fn ghast_fireballs_explode_and_remember_a_deflection() {
        let world = Grid::flat(10);
        let mut f = Fireball::large(DVec3::new(0.5, 16.0, 0.5), DVec3::X);
        f.deflect(DVec3::NEG_Y);
        assert!(f.heading().y < 0.0);
        let mut events = Vec::new();
        while f.update(0.05, &world, &ctx(vec![]), &[], &mut events) {}
        assert!(events.iter().any(|e| matches!(
            e,
            EntityEvent::Explosion {
                credit_player: true,
                power: GHAST_POWER,
                cause: "was fireballed by a ghast",
                ..
            }
        )));
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::IgniteBlock { .. } | EntityEvent::PlayerHit { .. })));
    }
}
