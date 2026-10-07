//! Experience orbs, like Java Edition's: they pop out of mined ores, killed
//! mobs, furnaces and dying players, fall and drift, home in on the nearest
//! living player within eight blocks, merge with same-sized orbs nearby,
//! and are absorbed one every two ticks. They burn in lava and fire and
//! vanish after five minutes in loaded chunks.

use glam::DVec3;

use super::{Ctx, MobWorld, Rng};
use crate::physics::{self, Shape};
use crate::simulation::experience::{self, Experience};

/// Seconds an orb lasts (Java's 6000 ticks).
pub const LIFETIME: f32 = 300.0;
/// Orbs fly to players closer than this.
const FOLLOW_DIST: f64 = 8.0;
/// Blocks per second squared (Java's 0.03 per tick²).
const GRAVITY: f64 = 12.0;
/// Pull toward a player, scaled by (1 - distance / 8)², in blocks per
/// second squared (Java's 0.1 per tick²).
const PULL: f64 = 40.0;
/// Java's 0.5 x 0.5 box.
const SHAPE: Shape = Shape::new(0.25, 0.5);
/// Same-sized orbs whose boxes come within this of each other merge.
const MERGE_DIST: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XpOrb {
    /// Points per orb.
    pub value: u32,
    /// Merged orbs of the same value; each pickup takes one.
    pub count: u32,
    /// Bottom centre.
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    /// Seconds since spawned (only counts in loaded chunks).
    pub age: f32,
    /// Random phase for the colour shimmer.
    pub phase: f32,
}

impl XpOrb {
    /// A fresh orb popping out in a random direction, like Java's.
    pub fn new(value: u32, pos: DVec3, rng: &mut Rng) -> Self {
        let vel = DVec3::new(rng.range(-4.0, 4.0) as f64, rng.range(0.0, 8.0) as f64, rng.range(-4.0, 4.0) as f64);
        Self { value, count: 1, pos, previous_pos: pos, vel, age: 0.0, phase: rng.range(0.0, std::f32::consts::TAU) }
    }

    /// Steps physics; returns `false` when the orb is gone (burnt or
    /// expired).
    pub fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) -> bool {
        self.age += dt as f32;
        if self.age >= LIFETIME || physics::touches_block(world, self.pos, SHAPE, |b| b.is_lava() || b.is_fire()) {
            return false;
        }
        let ticks = dt * 20.0;
        if physics::is_fluid_at(world, self.pos + DVec3::Y * 0.25) {
            // Java's underwater drift: rise slowly, slow down sideways.
            self.vel.y = (self.vel.y + 0.2 * dt).min(1.2);
            let drag = 0.99f64.powf(ticks);
            self.vel.x *= drag;
            self.vel.z *= drag;
        } else {
            self.vel.y -= GRAVITY * dt;
        }
        if physics::overlaps_solid(world, self.pos, SHAPE) {
            self.pos.y = self.pos.y.floor() + 1.0;
            self.vel = DVec3::ZERO;
            return true;
        }
        let centre = self.pos + DVec3::Y * 0.25;
        let nearest = ctx
            .players
            .iter()
            .filter(|t| t.alive)
            // Aim at the middle of the player, half their eye height up.
            .map(|t| t.pos + DVec3::Y * (crate::player::EYE_HEIGHT * 0.5) - centre)
            .min_by(|a, b| a.length_squared().total_cmp(&b.length_squared()));
        if let Some(to) = nearest.filter(|to| to.length_squared() < FOLLOW_DIST * FOLLOW_DIST) {
            let f = 1.0 - to.length() / FOLLOW_DIST;
            self.vel += to.normalize_or_zero() * f * f * PULL * dt;
        }
        let delta = self.vel * dt;
        let hit = physics::move_box(world, &mut self.pos, &mut self.vel, delta, SHAPE);
        // Java's drag: 0.98 per tick, and ground friction sideways.
        let side = if hit.on_ground { 0.6f64 * 0.98 } else { 0.98 }.powf(ticks);
        self.vel *= DVec3::new(side, 0.98f64.powf(ticks), side);
        true
    }

    /// Whether the player's box (feet at `player`) grown by Java's pickup
    /// margin touches this orb.
    pub fn touches_player(&self, player: DVec3) -> bool {
        let (min, max) = SHAPE.aabb(self.pos);
        let (pmin, pmax) =
            Shape::new(crate::player::HALF_WIDTH + 1.0, crate::player::HEIGHT + 1.0).aabb(player - DVec3::Y * 0.5);
        min.cmplt(pmax).all() && max.cmpgt(pmin).all()
    }

    /// `x,y,z,age,value,count` for the level file.
    pub fn serialize(&self) -> String {
        let p = self.pos;
        format!("{:.3},{:.3},{:.3},{:.1},{},{}", p.x, p.y, p.z, self.age, self.value, self.count)
    }

    /// Restores a stationary orb, rejecting nonfinite positions and empty values or counts.
    pub fn deserialize(text: &str, rng: &mut Rng) -> Option<Self> {
        let f: Vec<&str> = text.split(',').collect();
        let [x, y, z, age, value, count] = f[..] else { return None };
        let num = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
        let mut orb = Self::new(value.parse().ok().filter(|&v| v > 0)?, DVec3::new(num(x)?, num(y)?, num(z)?), rng);
        orb.vel = DVec3::ZERO;
        orb.age = num(age)?.clamp(0.0, LIFETIME as f64) as f32;
        orb.count = count.parse().ok().filter(|&c| c > 0)?;
        Some(orb)
    }
}

/// Merges orbs of the same value lying close together (the older one
/// absorbs the younger's count).
pub fn merge(orbs: &mut Vec<XpOrb>) {
    let reach = SHAPE.half_width * 2.0 + MERGE_DIST;
    let mut i = 0;
    while i < orbs.len() {
        let mut j = i + 1;
        while j < orbs.len() {
            let (a, b) = (&orbs[i], &orbs[j]);
            if a.value == b.value && (a.pos - b.pos).abs().cmplt(DVec3::splat(reach)).all() {
                let b = orbs.swap_remove(j);
                let a = &mut orbs[i];
                a.count = a.count.saturating_add(b.count);
                a.age = a.age.min(b.age);
                continue;
            }
            j += 1;
        }
        i += 1;
    }
}

/// A player at `feet` absorbs one touching orb if their pickup cooldown
/// allows; mending gear in `inventory` (held in `selected` or worn) takes
/// its share first. Returns `Some(chime)` when one was absorbed, where
/// `chime` is the level-up sound volume if it reached a multiple of five
/// levels.
pub fn absorb(
    orbs: &mut Vec<XpOrb>,
    feet: DVec3,
    xp: &mut Experience,
    inventory: &mut crate::inventory::Inventory,
    selected: usize,
) -> Option<Option<f32>> {
    if xp.pickup_cooldown > 0.0 {
        return None;
    }
    let i = orbs.iter().position(|o| o.touches_player(feet))?;
    let value = orbs[i].value;
    orbs[i].count -= 1;
    if orbs[i].count == 0 {
        orbs.swap_remove(i);
    }
    xp.pickup_cooldown = experience::PICKUP_INTERVAL;
    let value = inventory.mend(value, selected);
    Some(xp.add_points(value as i64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PlayerId, Target};
    use crate::physics::test_util::Grid;
    use crate::world::block::Block;
    use glam::IVec3;

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            players,
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        }
    }

    fn run(orb: &mut XpOrb, world: &Grid, ctx: &Ctx, secs: f64) -> bool {
        (0..(secs * 20.0) as usize).all(|_| orb.update(0.05, world, ctx))
    }

    #[test]
    fn orbs_fall_settle_and_expire() {
        let world = Grid::flat(10);
        let mut rng = Rng::new(1);
        let mut orb = XpOrb::new(3, DVec3::new(0.5, 11.0, 0.5), &mut rng);
        assert!(run(&mut orb, &world, &ctx(vec![]), 4.0));
        assert!((orb.pos.y - 10.0).abs() < 1e-3, "rests on the floor: {:?}", orb.pos);
        assert!(orb.vel.length() < 0.05, "stopped: {:?}", orb.vel);
        orb.age = LIFETIME - 0.5;
        assert!(!run(&mut orb, &world, &ctx(vec![]), 1.0), "despawns after five minutes");
    }

    #[test]
    fn orbs_home_in_on_the_nearest_living_player_within_eight_blocks() {
        let world = Grid::flat(10);
        let mut rng = Rng::new(2);
        let start = DVec3::new(0.5, 10.0, 0.5);
        let near = Target::new(PlayerId::HOST, DVec3::new(3.5, 10.0, 0.5), false);
        let mut orb = XpOrb::new(1, start, &mut rng);
        orb.vel = DVec3::ZERO;
        let c = ctx(vec![near]);
        assert!((0..60).any(|_| {
            orb.update(0.05, &world, &c);
            orb.touches_player(near.pos)
        }));

        let far = Target::new(PlayerId::HOST, DVec3::new(9.5, 10.0, 0.5), false);
        let dead = Target { alive: false, ..near };
        for players in [vec![far], vec![dead]] {
            let mut orb = XpOrb::new(1, start, &mut rng);
            orb.vel = DVec3::ZERO;
            run(&mut orb, &world, &ctx(players), 3.0);
            assert!((orb.pos.x - 0.5).abs() < 0.01, "stays put: {:?}", orb.pos);
        }
    }

    #[test]
    fn orbs_float_and_burn() {
        let mut world = Grid::flat(0);
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 0..5 {
                    world.set(IVec3::new(x, y, z), Block::WATER);
                }
                world.set(IVec3::new(x, 0, z + 10), Block::LAVA);
            }
        }
        let mut rng = Rng::new(3);
        let mut orb = XpOrb::new(1, DVec3::new(0.5, 1.0, 0.5), &mut rng);
        orb.vel = DVec3::ZERO;
        run(&mut orb, &world, &ctx(vec![]), 10.0);
        assert!(orb.pos.y > 4.0, "rises in water: {:?}", orb.pos);
        let mut orb = XpOrb::new(1, DVec3::new(0.5, 3.0, 10.5), &mut rng);
        orb.vel = DVec3::ZERO;
        assert!(!run(&mut orb, &world, &ctx(vec![]), 2.0), "burns in lava");
    }

    #[test]
    fn same_values_merge_and_pickups_take_one_every_two_ticks() {
        let mut rng = Rng::new(4);
        let at = |value, x: f64, rng: &mut Rng| XpOrb::new(value, DVec3::new(x, 10.0, 0.0), rng);
        let mut orbs = vec![at(3, 0.0, &mut rng), at(3, 0.4, &mut rng), at(7, 0.2, &mut rng), at(3, 5.0, &mut rng)];
        merge(&mut orbs);
        let mut counts: Vec<(u32, u32)> = orbs.iter().map(|o| (o.value, o.count)).collect();
        counts.sort();
        assert_eq!(counts, [(3, 1), (3, 2), (7, 1)]);

        orbs.retain(|o| o.pos.x < 1.0);
        let mut xp = Experience::default();
        let feet = DVec3::new(0.0, 10.0, 0.0);
        assert!(absorb(&mut orbs, feet, &mut xp, &mut Default::default(), 0).is_some());
        assert!(absorb(&mut orbs, feet, &mut xp, &mut Default::default(), 0).is_none(), "cooling down");
        let mut ticks = 1;
        while !orbs.is_empty() {
            xp.tick(0.05);
            absorb(&mut orbs, feet, &mut xp, &mut Default::default(), 0);
            ticks += 1;
        }
        assert_eq!(xp.total, 13);
        assert_eq!(ticks, 5, "three orbs, one every two ticks");
        assert!(absorb(&mut orbs, feet, &mut xp, &mut Default::default(), 0).is_none());
    }

    #[test]
    fn pickup_range_and_saving() {
        let mut rng = Rng::new(5);
        let mut orb = XpOrb::new(17, DVec3::new(1.2, 10.0, 0.0), &mut rng);
        orb.count = 4;
        assert!(orb.touches_player(DVec3::new(0.0, 10.0, 0.0)));
        assert!(!orb.touches_player(DVec3::new(-1.5, 10.0, 0.0)));
        assert!(!orb.touches_player(DVec3::new(0.0, 13.0, 0.0)));
        let back = XpOrb::deserialize(&orb.serialize(), &mut rng).unwrap();
        assert_eq!((back.value, back.count, back.age), (17, 4, orb.age));
        assert!(back.pos.distance(orb.pos) < 1e-3);
        for bad in ["1,2,3", "1,2,3,0,0,1", "1,2,3,0,5,0", "nan,2,3,0,5,1"] {
            assert!(XpOrb::deserialize(bad, &mut rng).is_none(), "{bad}");
        }
    }
}
