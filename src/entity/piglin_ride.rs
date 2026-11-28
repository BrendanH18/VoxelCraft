//! v1.0 gap Track 2: baby piglins riding baby hoglins, as Java 26.3's
//! `PiglinAi` does it.
//!
//! - `PiglinSpecificSensor` (every 20 ticks) remembers the nearest visible
//!   baby hoglin within 16 blocks (`NEAREST_VISIBLE_BABY_HOGLIN`).
//! - In the idle activity, `babySometimesRideBabyHoglin` copies it into
//!   `RIDE_TARGET` for 200–600 ticks (`RIDE_DURATION`), once its
//!   `SetEntityLookTargetSometimes.Ticker` (200–800 ticks, `RIDE_START_INTERVAL`)
//!   runs out; the ticker counts only ticks on which that behaviour can run.
//! - With a ride target the piglin's activity is `RIDE` (below admiring,
//!   fighting, avoiding and celebrating): `Mount.create(0.8)` walks to the
//!   target at speed modifier 0.8 and mounts within 1 block, and
//!   `DismountOrSkipMounting.create(8, wantsToStopRiding)` gets off (and
//!   forgets the target) when the vehicle is dead, more than 8 blocks away,
//!   grown up, either was hurt recently (`HURT_BY`: the last 40 ticks), or
//!   is a piglin that rides nothing.
//! - `Piglin.startRiding`: a baby mounting a hoglin climbs its first
//!   passengers up to the third (`getTopPassenger(hoglin, 3)`), so piglins
//!   stack at most three high.
//! - `updateActivity`: a baby riding a baby piglin or hoglin with no ride
//!   target gets off, so a reloaded world's riders dismount at once.
//!
//! Seats: a baby hoglin carries its rider at 0.875 (an adult at 1.49375), a
//! baby piglin at its height 0.98, and a baby piglin rides 0.1875 above
//! its feet (its `VEHICLE` attachment).
use super::mounts::MOB_VEHICLE;
use super::{Ctx, Entities, MobKind, MobWorld};

/// `PiglinAi.RIDE_START_INTERVAL`: `TimeUtil.rangeOfSeconds(10, 40)`.
pub const RIDE_START_INTERVAL: (u32, u32) = (200, 800);
/// `PiglinAi.RIDE_DURATION`: `TimeUtil.rangeOfSeconds(10, 30)`.
pub const RIDE_DURATION: (u32, u32) = (200, 600);
/// `DismountOrSkipMounting.create(8, …)`.
pub const MAX_RIDE_DISTANCE: f64 = 8.0;
/// `Mount.create(0.8F)` and its 1-block reach.
pub const MOUNT_SPEED: f64 = 0.8;
pub const MOUNT_REACH: f64 = 1.0;
/// `getTopPassenger(vehicle, 3)`.
pub const STACK_LIMIT: u32 = 3;
/// `LivingEntity.getLastDamageSource` forgets a hit after 40 ticks.
pub const HURT_MEMORY: u32 = 40;
/// The piglin's sensor scan rate and range (follow range 16).
pub const SENSE_TICKS: u32 = 20;
pub const SENSE_RANGE: f64 = 16.0;
/// Passenger attachment heights: baby hoglin, adult hoglin, baby piglin.
pub const BABY_HOGLIN_SEAT: f64 = 0.875;
pub const HOGLIN_SEAT: f64 = 1.49375;
pub const BABY_PIGLIN_SEAT: f64 = 0.98;
/// A baby piglin's `VEHICLE` attachment.
pub const BABY_PIGLIN_VEHICLE_ATTACHMENT: f64 = 0.1875;

/// A baby piglin's riding memories (never saved, as `RIDE_TARGET` and the
/// sensor's memory aren't).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ride {
    /// `NEAREST_VISIBLE_BABY_HOGLIN` (uid) and ticks to the next scan.
    pub seen: Option<u32>,
    pub scan_in: u32,
    /// `RIDE_TARGET` (uid) and the ticks it has left.
    pub target: Option<(u32, u32)>,
    /// The ticker's `ticksUntilNextStart`.
    pub ticker: u32,
}

/// `UniformInt.sample`.
fn sample(rng: &mut super::Rng, (lo, hi): (u32, u32)) -> u32 {
    lo + rng.next_int(hi - lo + 1)
}

/// Kinds that carry baby piglins.
pub fn carries_piglins(kind: MobKind) -> bool {
    matches!(kind, MobKind::Hoglin | MobKind::Piglin)
}

impl Entities {
    /// One tick of every baby piglin's riding behaviours.
    pub(super) fn piglin_rides<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        let _ = ctx;
        let uid_index: rustc_hash::FxHashMap<u32, usize> =
            self.mobs.iter().enumerate().map(|(i, m)| (m.uid, i)).collect();
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if m.kind != MobKind::Piglin || !m.alive() || !m.simulation_active {
                continue;
            }
            let Some(n) = m.nether.as_ref() else { continue };
            let mut ride = n.ride;
            let baby = m.baby;
            let vehicle = m.riding.filter(|v| v & MOB_VEHICLE != 0).map(|v| v & !MOB_VEHICLE);
            // `RIDE_TARGET` expires.
            if let Some((uid, left)) = ride.target {
                ride.target = (left > 1).then_some((uid, left - 1));
            }
            // `PiglinSpecificSensor`.
            if ride.scan_in == 0 {
                ride.scan_in = SENSE_TICKS;
                let eye = m.look_eye();
                ride.seen = self
                    .mobs
                    .iter()
                    .filter(|h| h.kind == MobKind::Hoglin && h.baby && h.alive())
                    .filter(|h| h.pos.distance_squared(m.pos) <= SENSE_RANGE * SENSE_RANGE)
                    .filter(|h| super::mob::line_of_sight(world, eye, h.look_eye()))
                    .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                    .map(|h| h.uid);
            }
            ride.scan_in -= 1;
            // The activities above `RIDE` and `IDLE` (admiring, fighting,
            // avoiding, celebrating) hold off both.
            let busy = n.admire_left > 0.0
                || n.celebrate_left > 0.0
                || !matches!(n.goal, super::nether::Goal::None | super::nether::Goal::Walk(_));
            let mut mount = None;
            let mut walk = None;
            let mut dismount = false;
            if !busy && let Some((uid, _)) = ride.target {
                // `RIDE`: `Mount`, then `DismountOrSkipMounting`.
                let target = uid_index.get(&uid).map(|&j| &self.mobs[j]);
                if vehicle.is_none()
                    && let Some(t) = target.filter(|t| t.alive())
                {
                    if t.pos.distance(m.pos) < MOUNT_REACH {
                        mount = Some(uid);
                    } else {
                        walk = Some(t.pos);
                    }
                }
                let current = vehicle.or(mount).or(Some(uid)).and_then(|u| uid_index.get(&u).map(|&j| &self.mobs[j]));
                let valid = current.is_some_and(|v| v.alive() && v.pos.distance(m.pos) < MAX_RIDE_DISTANCE);
                let stop = current.is_some_and(|v| {
                    !v.baby
                        || !v.alive()
                        || m.hurt_age < HURT_MEMORY
                        || v.hurt_age < HURT_MEMORY
                        || v.kind == MobKind::Piglin && v.riding.is_none()
                });
                if !valid || stop {
                    dismount = true;
                    mount = None;
                    walk = None;
                }
            } else if !busy && baby && ride.target.is_none() && vehicle.is_none() {
                // `IDLE`: `babySometimesRideBabyHoglin`.
                if let Some(seen) = ride.seen {
                    let due = if ride.ticker == 0 {
                        ride.ticker = sample(&mut self.rng, RIDE_START_INTERVAL) - 1;
                        false
                    } else {
                        ride.ticker -= 1;
                        ride.ticker == 0
                    };
                    if due {
                        ride.target = Some((seen, sample(&mut self.rng, RIDE_DURATION)));
                    }
                }
            }
            // `updateActivity`: no ride target, riding a baby piglin or hoglin.
            let baby_vehicle = vehicle
                .and_then(|u| uid_index.get(&u))
                .is_some_and(|&j| carries_piglins(self.mobs[j].kind) && self.mobs[j].baby);
            if dismount || ride.target.is_none() && baby && baby_vehicle {
                dismount = true;
                ride.target = ride.target.filter(|_| !dismount);
            }
            // `Piglin.startRiding`: climb to the top of the stack; it fails
            // when that vehicle already carries someone (`canAddPassenger`).
            let mount = mount
                .map(|uid| self.top_passenger(uid, STACK_LIMIT, &uid_index))
                .filter(|&uid| !self.mobs.iter().any(|r| r.alive() && r.riding == Some(MOB_VEHICLE | uid)));
            let m = &mut self.mobs[i];
            m.nether.as_mut().unwrap().ride = ride;
            m.ride_walk = walk;
            if dismount && vehicle.is_some() {
                m.riding = None;
            } else if let Some(uid) = mount {
                m.riding = Some(MOB_VEHICLE | uid);
                m.stop_navigation();
            }
        }
    }

    /// `Piglin.getTopPassenger`: the first passenger of the first
    /// passenger…, at most `counter − 1` levels up.
    fn top_passenger(&self, vehicle: u32, counter: u32, uid_index: &rustc_hash::FxHashMap<u32, usize>) -> u32 {
        if counter == 1 {
            return vehicle;
        }
        let first = self.mobs.iter().find(|m| m.alive() && m.riding == Some(MOB_VEHICLE | vehicle)).map(|m| m.uid);
        match first {
            Some(p) if uid_index.contains_key(&p) => self.top_passenger(p, counter - 1, uid_index),
            _ => vehicle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PlayerId, Target};
    use crate::physics::test_util::Grid;
    use crate::simulation::lighting::SpawnLight;
    use crate::world::terrain::Dimension;
    use glam::DVec3;

    fn ctx() -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId(1), DVec3::new(40.5, 0.0, 0.5), false)],
            daylight: 0.0,
            spawn_light: SpawnLight::NIGHT,
            spawning: false,
            raining: false,
            dimension: Dimension::Nether,
        }
    }

    fn baby(e: &mut Entities, kind: MobKind, pos: DVec3) -> usize {
        e.spawn(kind, pos);
        let i = e.mobs.len() - 1;
        let m = &mut e.mobs[i];
        m.baby = true;
        m.age = -24000;
        m.persistent = true;
        if let Some(n) = m.nether.as_mut() {
            n.weapon = crate::entity::nether::Weapon::None;
            n.immune = true;
        }
        i
    }

    /// `babySometimesRideBabyHoglin`: a baby piglin that sees a baby hoglin
    /// picks it as its ride target after 200–800 ticks, walks over and
    /// rides it until the 200–600-tick memory runs out, then gets off.
    #[test]
    fn baby_piglins_ride_baby_hoglins_for_a_while() {
        let world = Grid::flat(0);
        let mut e = Entities::new(21);
        let hoglin = baby(&mut e, MobKind::Hoglin, DVec3::new(3.5, 0.0, 0.5));
        let piglin = baby(&mut e, MobKind::Piglin, DVec3::new(0.5, 0.0, 0.5));
        e.ensure_mob_ids();
        let c = ctx();
        let vehicle = MOB_VEHICLE | e.mobs[hoglin].uid;
        let mut mounted = None;
        let mut left = None;
        for tick in 0..2400 {
            e.update(0.05, &world, &c);
            let p = &e.mobs[piglin];
            if mounted.is_none() && p.riding == Some(vehicle) {
                mounted = Some(tick);
                let seat = e.mobs[hoglin].pos.y + BABY_HOGLIN_SEAT - BABY_PIGLIN_VEHICLE_ATTACHMENT;
                assert!((p.pos.y - seat).abs() < 1e-6, "seated at {} not {seat}", p.pos.y);
            }
            if mounted.is_some() && p.riding.is_none() {
                left = Some(tick);
                break;
            }
        }
        let on = mounted.expect("mounted");
        assert!((150..=900).contains(&on), "mounted at {on}");
        let off = left.expect("got off when the memory ran out");
        assert!((on + 150..=on + 620).contains(&off), "rode {on}..{off}");
    }

    /// `Piglin.startRiding`: babies climb to the third passenger at most,
    /// so a fourth can't get on; hurting the hoglin makes riders get off.
    #[test]
    fn piglins_stack_three_high_and_get_off_when_hurt() {
        let world = Grid::flat(0);
        let mut e = Entities::new(5);
        let hoglin = baby(&mut e, MobKind::Hoglin, DVec3::new(0.5, 0.0, 0.5));
        let riders: Vec<usize> = (0..4).map(|_| baby(&mut e, MobKind::Piglin, DVec3::new(0.9, 0.0, 0.5))).collect();
        e.ensure_mob_ids();
        let h = e.mobs[hoglin].uid;
        for &r in &riders {
            let n = e.mobs[r].nether.as_mut().unwrap();
            n.ride.target = Some((h, 400));
            n.ride.scan_in = 1000;
            // One at a time, so each finds the last one's seat taken.
            e.piglin_rides(&world, &ctx());
            e.position_mob_riders();
        }
        let uid = |i: usize| e.mobs[i].uid;
        assert_eq!(e.mobs[riders[0]].riding, Some(MOB_VEHICLE | h));
        assert_eq!(e.mobs[riders[1]].riding, Some(MOB_VEHICLE | uid(riders[0])));
        assert_eq!(e.mobs[riders[2]].riding, Some(MOB_VEHICLE | uid(riders[1])));
        assert_eq!(e.mobs[riders[3]].riding, None, "three high at most");
        let top = e.mobs[riders[2]].pos.y;
        let expect =
            e.mobs[hoglin].pos.y + BABY_HOGLIN_SEAT + 2.0 * BABY_PIGLIN_SEAT - 3.0 * BABY_PIGLIN_VEHICLE_ATTACHMENT;
        assert!((top - expect).abs() < 1e-6, "top seat {top} vs {expect}");
        // `wantsToStopRiding`: the hoglin was hurt recently.
        e.mobs[hoglin].hurt_age = 0;
        e.piglin_rides(&world, &ctx());
        assert_eq!(e.mobs[riders[0]].riding, None);
        // `isBabyRidingBaby` with no ride target: the stacked ones get off too.
        for &r in &riders[1..3] {
            e.mobs[r].nether.as_mut().unwrap().ride.target = None;
        }
        e.piglin_rides(&world, &ctx());
        assert!(riders.iter().all(|&r| e.mobs[r].riding.is_none()));
    }
}
