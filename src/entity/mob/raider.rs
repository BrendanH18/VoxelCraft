//! v0.15 Track E: the raiders' movement goals when they have nothing to
//! attack (Java 26.3 `Raider` and `PatrollingMonster`), in goal priority
//! order: `ObtainRaidLeaderBannerGoal` (1), `PathfindToRaidGoal` (3),
//! `RaiderMoveThroughVillageGoal` and `LongDistancePatrolGoal` (4). The
//! raid tick (`entity::raid`) fills in each raider's raid centre, village
//! flag, home to walk to, banner to fetch and patrol companions;
//! `HoldGroundAttackGoal` runs before the attack goals.
use super::*;
use crate::entity::raid;

impl Mob {
    /// The raid and patrol movement goals for a raider without a target.
    /// `None` leaves it to its other goals.
    pub(super) fn raider_goals<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        if !crate::entity::illager::is_raider(self.kind) || self.riding.is_some() {
            return None;
        }
        let s = self.illager.as_ref()?;
        let r = &s.raider;
        // `ObtainRaidLeaderBannerGoal`, at 1.15.
        if let Some(banner) = r.banner_target {
            let speed = self.goal_speed(raid::BANNER_WALK_SPEED);
            return Some((self.pursue(world, banner, speed, true, false, rng), speed));
        }
        // `PathfindToRaidGoal`: outside the village, a random spot up to 15
        // blocks toward the centre, at 1.0.
        if let Some(center) = r.raid_center {
            if !r.in_village {
                let goal = self.raid_step_toward(world, center, rng);
                let speed = self.goal_speed(1.0);
                return Some((self.pursue(world, goal, speed, true, false, rng), speed));
            }
            // `RaiderMoveThroughVillageGoal(1.05, 1)`: walk to a home, then
            // remember it among the last three.
            if let Some(home) = r.village_target {
                let at = home.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
                let reach = self.shape().half_width * 2.0 + 1.0;
                if (at - self.pos).with_y(0.0).length() < reach {
                    let s = self.illager.as_mut().unwrap();
                    if s.raider.visited.len() > 2 {
                        s.raider.visited.remove(0);
                    }
                    s.raider.visited.push(home);
                    s.raider.village_target = None;
                    self.stop_navigation();
                    return None;
                }
                // `start`: `setNoActionTime(0)`.
                self.idle = 0.0;
                let speed = self.goal_speed(raid::VILLAGE_WALK_SPEED);
                return Some((self.pursue(world, at, speed, true, false, rng), speed));
            }
            return None;
        }
        self.patrol_goal(world, rng)
    }

    /// `DefaultRandomPos.getPosTowards(mob, 15, 4, centre, PI / 2)`: a spot
    /// up to 15 blocks away within a quarter turn either side of the
    /// centre, kept until reached.
    fn raid_step_toward<W: MobWorld + ?Sized>(&mut self, world: &W, center: IVec3, rng: &mut Rng) -> DVec3 {
        let s = self.illager.as_mut().unwrap();
        if let Some(goal) = s.raider_walk.filter(|g| (*g - self.pos).with_y(0.0).length() > 1.5) {
            return goal;
        }
        let to = (center.as_dvec3() - self.pos).with_y(0.0);
        let dist = to.length();
        let base = (to.z).atan2(to.x);
        let angle = base + f64::from(rng.range(-std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4));
        let reach = dist.min(15.0);
        let x = self.pos.x + angle.cos() * reach;
        let z = self.pos.z + angle.sin() * reach;
        let y = world.surface(x.floor() as i32, z.floor() as i32).map_or(self.pos.y, |t| f64::from(t + 1));
        let goal = DVec3::new(x, y, z);
        self.illager.as_mut().unwrap().raider_walk = Some(goal);
        goal
    }

    /// `PatrollingMonster.LongDistancePatrolGoal`: patrol members walk
    /// toward the patrol target, curving around it; the leader picks a new
    /// target within 10 blocks of the old and hands each step to its
    /// companions; a member with no companions stops patrolling.
    fn patrol_goal<W: MobWorld + ?Sized>(&mut self, world: &W, rng: &mut Rng) -> Option<(Option<DVec3>, f64)> {
        let s = self.illager.as_mut()?;
        let r = &mut s.raider;
        if r.patrol_cooldown > 0 {
            r.patrol_cooldown -= 1;
        }
        if !r.patrolling || r.patrol_target.is_none() || r.patrol_cooldown > 0 || r.raid_active {
            return None;
        }
        let leader = r.patrol_leader;
        let speed_mod = if leader { raid::PATROL_LEADER_SPEED } else { raid::PATROL_SPEED };
        let walking = s.raider_walk.filter(|g| (*g - self.pos).with_y(0.0).length() > 1.5);
        if walking.is_none() {
            if !r.companions {
                r.patrolling = false;
                return None;
            }
            let target = r.patrol_target.unwrap();
            let target_v = target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            if leader && target_v.distance(self.pos) < raid::PATROL_ARRIVE {
                // `findPatrolTarget`.
                r.patrol_target = Some(
                    self.pos.floor().as_ivec3()
                        + IVec3::new(
                            raid::PATROL_TARGET_OFFSET + rng.next_int(raid::PATROL_TARGET_SPREAD) as i32,
                            0,
                            raid::PATROL_TARGET_OFFSET + rng.next_int(raid::PATROL_TARGET_SPREAD) as i32,
                        ),
                );
                return None;
            }
            // `distance.yRot(90.0F)`: Java passes 90 as radians.
            let d = self.pos - target_v;
            let (sin, cos) = (raid::PATROL_TURN_DEGREES.sin() as f64, raid::PATROL_TURN_DEGREES.cos() as f64);
            let turned = DVec3::new(d.x * cos + d.z * sin, d.y, d.z * cos - d.x * sin);
            let long = turned * raid::PATROL_TURN_SCALE + target_v;
            let step = (long - self.pos).normalize_or_zero() * raid::PATROL_STEP + self.pos;
            let cell = step.floor().as_ivec3();
            let y = world.surface(cell.x, cell.z).map_or(cell.y, |t| t + 1);
            let path_target = IVec3::new(cell.x, y, cell.z);
            if leader {
                r.patrol_step = Some(path_target);
            }
            s.raider_walk = Some(path_target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        }
        let goal = self.illager.as_ref().unwrap().raider_walk.unwrap();
        let speed = self.goal_speed(speed_mod);
        let dir = self.pursue(world, goal, speed, true, false, rng);
        if dir.is_none() && self.nav.is_done() {
            // `moveRandomly` and the 200-tick cooldown after a failed path.
            let s = self.illager.as_mut().unwrap();
            s.raider.patrol_cooldown = raid::PATROL_FAILED_COOLDOWN;
            let wander = self.pos.floor().as_ivec3()
                + IVec3::new(
                    raid::PATROL_WANDER_OFFSET + rng.next_int(raid::PATROL_WANDER_SPREAD) as i32,
                    0,
                    raid::PATROL_WANDER_OFFSET + rng.next_int(raid::PATROL_WANDER_SPREAD) as i32,
                );
            let y = world.surface(wander.x, wander.z).map_or(wander.y, |t| t + 1);
            s.raider_walk = Some(DVec3::new(f64::from(wander.x) + 0.5, f64::from(y), f64::from(wander.z) + 0.5));
        }
        Some((dir, speed))
    }

    /// `Raider.HoldGroundAttackGoal(10)` for pillagers and vindicators on
    /// patrol outside a raid: with a target not yet within 10 blocks and
    /// not hit by a player, stand still and watch it; within 10 blocks it
    /// turns aggressive and the attack goal takes over.
    pub(super) fn hold_ground(&mut self, target: Option<DVec3>) -> bool {
        if !matches!(self.kind, MobKind::Pillager | MobKind::Vindicator) {
            return false;
        }
        let hit_by_player = self.player_hit_left > 0.0;
        let Some(s) = self.illager.as_mut() else { return false };
        let Some(at) = target else { return false };
        if s.raider.raid.is_some() || !s.raider.patrolling || s.aggressive || hit_by_player {
            return false;
        }
        if (at - self.pos).length_squared() > raid::HOLD_GROUND_RADIUS * raid::HOLD_GROUND_RADIUS {
            self.stop_navigation();
            return true;
        }
        false
    }
}
