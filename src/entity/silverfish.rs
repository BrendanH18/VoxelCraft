//! v0.16 Track E silverfish goals (MOB-01c), from Java 26.3 `Silverfish`:
//!
//! - `SilverfishMergeWithStoneGoal` (priority 5): with no target and its
//!   path finished, on a goal-selector update, under `mobGriefing`, one
//!   time in `reducedTickDelay(10)` = 5 a silverfish picks one of the six
//!   directions (`Direction.getRandom`); when the block that way from
//!   `BlockPos.containing(x, y + 0.5, z)` is a host
//!   (`InfestedBlock.isCompatibleHostBlock`) it becomes the infested block
//!   and the silverfish vanishes (`spawnAnim`, `discard`).
//! - `SilverfishWakeUpFriendsGoal` (priority 3): a hit from an entity
//!   (`hurtServer`) arms a countdown of `adjustedTickDelay(20)` = 10 goal
//!   ticks (20 game ticks, the goal ticking on the selector's full
//!   updates). When it runs out the silverfish scans y 0, 1, -1 … 5, -5
//!   and x and z 0, 1, -1 … 10, -10 around its block: each infested block
//!   found is broken with drops (`destroyBlock`, releasing its silverfish)
//!   under `mobGriefing` or turned back into its host without it, and the
//!   scan stops after each one with probability one half.
//!
//! Infested blocks release their silverfish here too
//! ([`Entities::release_silverfish`]), from mining, explosions and the
//! wake-up goal: `InfestedBlock.spawnInfestation` puts it at the block's
//! bottom centre with `spawnAnim`.
use super::{Entities, EntityEvent, MobKind, MobWorld};
use crate::world::block::Block;
use crate::world::infested;
use glam::{DVec3, IVec3};

/// `reducedTickDelay(10)`: the merge goal's odds per selector update.
pub const MERGE_ODDS: u32 = 5;
/// `adjustedTickDelay(20)`: goal ticks before the friends are woken.
pub const WAKE_DELAY: u32 = 10;
/// The wake-up scan's reach: 10 blocks across, 5 up and down.
pub const WAKE_RADIUS: i32 = 10;
pub const WAKE_HEIGHT: i32 = 5;
/// Seconds per game tick.
const TICK: f64 = 0.05;
/// `Direction.values()` order: down, up, north, south, west, east.
const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

/// A silverfish's goal state. Never saved: Java keeps neither field.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// `SilverfishWakeUpFriendsGoal.lookForFriends`.
    pub(super) look_for_friends: u32,
    /// Game ticks counted, for the goal selector's every-other-tick updates.
    ticks: u32,
    /// Seconds not yet counted as a tick.
    carry: f64,
}

impl State {
    /// `SilverfishWakeUpFriendsGoal.notifyHurt`.
    pub fn notify_hurt(&mut self) {
        if self.look_for_friends == 0 {
            self.look_for_friends = WAKE_DELAY;
        }
    }
}

/// `y`, `x` and `z` offsets in the order the wake-up scan visits them:
/// 0, 1, -1, 2, -2, … (Java's `(off <= 0 ? 1 : 0) - off`).
fn scan_order(reach: i32) -> impl Iterator<Item = i32> {
    std::iter::successors(Some(0), move |&o| {
        let next = i32::from(o <= 0) - o;
        (next.abs() <= reach).then_some(next)
    })
}

/// Cells of the wake-up scan around `base`, in Java's order.
pub fn wake_scan(base: IVec3) -> impl Iterator<Item = IVec3> {
    scan_order(WAKE_HEIGHT).flat_map(move |y| {
        scan_order(WAKE_RADIUS).flat_map(move |x| scan_order(WAKE_RADIUS).map(move |z| base + IVec3::new(x, y, z)))
    })
}

impl Entities {
    /// Spawns the silverfish of infested blocks broken at `cells`
    /// (`InfestedBlock.spawnInfestation`), emptying the list.
    pub fn release_silverfish(&mut self, cells: &mut Vec<IVec3>) {
        for cell in cells.drain(..) {
            let at = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            self.spawn(MobKind::Silverfish, at);
            if let Some(m) = self.mobs.last_mut() {
                m.persistent = false;
            }
            self.spawn_anim(at, MobKind::Silverfish);
        }
    }

    /// `Mob.spawnAnim`: twenty poof particles over the body.
    fn spawn_anim(&mut self, at: DVec3, kind: MobKind) {
        let shape = kind.shape();
        let mut burst =
            crate::particles::Burst::new(crate::particles::Kind::Poof, at + DVec3::Y * shape.height * 0.5, 20);
        burst.spread = DVec3::new(shape.half_width, shape.height * 0.5, shape.half_width);
        burst.forced = true;
        burst.velocity_spread = DVec3::splat(0.02);
        self.particles.push(crate::particles::Request::Burst(burst));
    }

    /// Per update, for every active silverfish: its merge and wake-up
    /// goals. Block changes go out as [`EntityEvent::AnimalBlock`]s (the
    /// game applies them when the cell still holds what was seen); a
    /// broken infested block's silverfish is spawned here.
    pub(super) fn silverfish_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.silverfish.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        let griefing = self.villager_griefing;
        let mut released = Vec::new();
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &mut self.mobs[i];
            if !(m.simulation_active && m.alive()) {
                i += 1;
                continue;
            }
            let (uid, pos, idle) = (m.uid, m.pos, m.ai == super::mob::Ai::Idle);
            let targeted = m.move_ctl.target.is_some() || m.ai == super::mob::Ai::Chase;
            let Some(s) = m.silverfish.as_mut() else {
                i += 1;
                continue;
            };
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / TICK).floor();
            s.carry -= ticks * TICK;
            let mut merged = false;
            for _ in 0..ticks as u32 {
                s.ticks = s.ticks.wrapping_add(1);
                // `Mob.serverAiStep`: goals start and tick on every other tick.
                if !(s.ticks.wrapping_add(uid)).is_multiple_of(2) {
                    continue;
                }
                if s.look_for_friends > 0 {
                    s.look_for_friends -= 1;
                    if s.look_for_friends == 0 {
                        for cell in wake_scan(pos.floor().as_ivec3()) {
                            let Some(b) = world.block(cell) else { continue };
                            let Some(host) = infested::host(b) else { continue };
                            if griefing {
                                events.push(EntityEvent::AnimalBlock { cell, from: b, to: Block::AIR });
                                self.particles.push(crate::particles::Request::Break { cell, block: b });
                                released.push(cell);
                            } else {
                                events.push(EntityEvent::AnimalBlock { cell, from: b, to: host });
                            }
                            if self.rng.next_int(2) == 0 {
                                break;
                            }
                        }
                    }
                }
                if !targeted && idle && griefing && self.rng.next_int(MERGE_ODDS) == 0 {
                    let dir = DIRECTIONS[self.rng.next_int(6) as usize];
                    let cell = DVec3::new(pos.x, pos.y + 0.5, pos.z).floor().as_ivec3() + dir;
                    if let Some(b) = world.block(cell)
                        && let Some(to) = infested::infested(b)
                    {
                        events.push(EntityEvent::AnimalBlock { cell, from: b, to });
                        merged = true;
                        break;
                    }
                }
            }
            if merged {
                // `discard`: no death, drops or experience.
                self.mobs.swap_remove(i);
                self.spawn_anim(pos, MobKind::Silverfish);
                continue;
            }
            i += 1;
        }
        self.release_silverfish(&mut released);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_scan_follows_javas_order_and_reach() {
        let order: Vec<i32> = scan_order(5).collect();
        assert_eq!(order, [0, 1, -1, 2, -2, 3, -3, 4, -4, 5, -5]);
        let cells: Vec<IVec3> = wake_scan(IVec3::ZERO).collect();
        assert_eq!(cells.len(), 11 * 21 * 21);
        assert_eq!(cells[0], IVec3::ZERO);
        assert_eq!(cells[1], IVec3::new(0, 0, 1));
        assert_eq!(cells[2], IVec3::new(0, 0, -1));
        assert_eq!(cells[21], IVec3::new(1, 0, 0));
        assert_eq!(*cells.last().unwrap(), IVec3::new(-10, -5, -10));
    }

    #[test]
    fn hurt_arms_the_wake_up_once() {
        let mut s = State::default();
        s.notify_hurt();
        assert_eq!(s.look_for_friends, WAKE_DELAY);
        s.look_for_friends = 3;
        s.notify_hurt();
        assert_eq!(s.look_for_friends, 3);
    }
}
