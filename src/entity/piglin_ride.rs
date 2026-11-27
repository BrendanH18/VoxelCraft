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
use glam::DVec3;

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
                let eye = m.pos + DVec3::Y * (m.shape().height * 0.85);
                ride.seen = self
                    .mobs
                    .iter()
                    .filter(|h| h.kind == MobKind::Hoglin && h.baby && h.alive())
                    .filter(|h| h.pos.distance_squared(m.pos) <= SENSE_RANGE * SENSE_RANGE)
                    .filter(|h| super::mob::line_of_sight(world, eye, h.pos + DVec3::Y * (h.shape().height * 0.85)))
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
            // `Piglin.startRiding`: climb to the top of the stack.
            let mount = mount.map(|uid| self.top_passenger(uid, STACK_LIMIT, &uid_index));
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
