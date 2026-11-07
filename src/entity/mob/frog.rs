//! Bounded frog/tadpole activities on the shared navigation and move controls.
use super::{Ai, Mob, MobKind};
use crate::{
    entity::{Ctx, MobWorld, Rng},
    physics,
    world::block::{Block, Facing},
};
use glam::{DVec3, IVec3};

impl Mob {
    pub(crate) fn frog_can_hunt(&self) -> bool {
        self.ai != Ai::Panic && !self.breeding()
    }
    pub(super) fn frog_priority(&self) -> bool {
        self.kind == MobKind::Frog
            && self.frog_can_hunt()
            && self
                .frog
                .as_ref()
                .is_some_and(|s| s.tongue.is_some() || matches!(s.pose, 1 | 2 | 4) || s.jump_prepare > 0)
            || self.kind == MobKind::Frog && self.ai != Ai::Panic && self.animal.as_ref().is_some_and(|a| a.pregnant)
    }
    pub(super) fn frog_steer<W: MobWorld + ?Sized>(
        &mut self,
        _dt: f32,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        if self.ai == Ai::Panic {
            if self.ai_timer <= 0.0 && self.nav.is_done() {
                self.ai = Ai::Idle;
                return (None, 0.0);
            }
            let speed = self.goal_speed(2.0);
            self.panic_target(w, ctx, rng, speed);
            return (self.follow_path(w, speed, false), speed);
        }
        let pose = self.frog.as_ref().unwrap().pose;
        if matches!(pose, 1 | 2 | 4) {
            self.stop_navigation();
            return (None, 0.0);
        }
        if let Some((uid, pos)) = self.frog.as_ref().unwrap().tongue {
            let speed = self.goal_speed(2.0);
            let wish = self.pursue(w, pos, speed, true, true, rng);
            if wish.is_none() && self.on_ground && self.pos.distance(pos) >= 1.75 && self.repath_in >= 15 {
                let s = self.frog.as_mut().unwrap();
                s.unreachable.rotate_left(1);
                s.unreachable[4] = (uid, 100);
                s.tongue = None;
            }
            return (wish, speed);
        }
        if self.kind == MobKind::Frog && self.animal.as_ref().is_some_and(|a| a.pregnant) {
            if self.on_ground && !self.in_water {
                let below = self.pos.floor().as_ivec3() - IVec3::Y;
                for facing in Facing::ALL {
                    let water = below + facing.offset();
                    let above = water + IVec3::Y;
                    if w.block(water).is_some_and(|b| b.holds_water() && !b.is_solid())
                        && w.block(above) == Some(Block::AIR)
                    {
                        // Pregnancy clears only once the entity pass accepts this water cell.
                        self.frog.as_mut().unwrap().laid = Some(above);
                        self.stop_navigation();
                        return (None, 0.0);
                    }
                }
            }
            if self.frog.as_ref().unwrap().shore_check == 0 {
                let p = self.pos.floor().as_ivec3();
                let mut best = None;
                for dy in -2..=2 {
                    for dz in -8..=8 {
                        for dx in -8..=8 {
                            let at = p + IVec3::new(dx, dy, dz);
                            if w.block(at) != Some(Block::AIR)
                                || !w.block(at - IVec3::Y).is_some_and(|b| b.is_solid() && !b.holds_water())
                            {
                                continue;
                            }
                            if !Facing::ALL.into_iter().any(|f| {
                                let q = at - IVec3::Y + f.offset();
                                w.block(q).is_some_and(|b| b.holds_water() && !b.is_solid())
                                    && w.block(q + IVec3::Y) == Some(Block::AIR)
                            }) {
                                continue;
                            }
                            let d = at.as_dvec3().distance_squared(self.pos);
                            if best.is_none_or(|(_, old)| d < old) {
                                best = Some((at, d));
                            }
                        }
                    }
                }
                let s = self.frog.as_mut().unwrap();
                s.shore = best.map(|(p, _)| p);
                s.shore_check = 40;
            }
            if let Some(goal) = self.frog.as_ref().unwrap().shore {
                let speed = self.goal_speed(1.0);
                return (self.pursue(w, goal.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), speed, true, false, rng), speed);
            }
            return (None, 0.0);
        }
        if self.kind == MobKind::Frog && !self.in_water {
            if self.frog.as_ref().unwrap().jump_prepare > 0 {
                let s = self.frog.as_mut().unwrap();
                s.jump_prepare -= 1;
                if s.jump_prepare == 0
                    && let Some(velocity) = s.jump_velocity.take()
                {
                    let boost = self
                        .effects
                        .get(crate::simulation::effects::Effect::JumpBoost)
                        .map_or(0.0, |e| 0.1 * (f64::from(e.amplifier) + 1.0));
                    self.vel = velocity.normalize_or_zero() * (velocity.length() + boost) * 20.0;
                    self.on_ground = false;
                    s.pose = 4;
                    s.fall_distance = 0.0;
                    s.sounds |= 1 << crate::entity::frog::FrogSound::Jump as u8;
                }
                self.stop_navigation();
                return (None, 0.0);
            }
            if self.frog.as_ref().unwrap().croak > 0 {
                return (None, 0.0);
            }
            self.frog.as_mut().unwrap().pose = 0;
            if self.on_ground && self.nav.is_done() && self.frog.as_ref().unwrap().jump_cooldown == 0 {
                if let Some(velocity) = long_jump(self, w, rng) {
                    let s = self.frog.as_mut().unwrap();
                    s.jump_velocity = Some(velocity);
                    s.jump_prepare = 40;
                    self.stop_navigation();
                    return (None, 0.0);
                }
                self.frog.as_mut().unwrap().jump_cooldown = (100 + rng.next_int(41)) as u16 / 2;
            }
        }
        let modifier = if self.kind == MobKind::Tadpole {
            0.5
        } else if self.in_water {
            0.75
        } else {
            1.0
        };
        let speed = self.goal_speed(modifier);
        if !self.nav.is_done() {
            return (self.follow_path(w, speed, false), speed);
        }
        if self.ai_timer > 0.0 {
            return (None, 0.0);
        }
        self.ai_timer = rng.range(1.5, 3.0);
        let roll = rng.next_int(if self.kind == MobKind::Frog && !self.in_water { 7 } else { 10 });
        if self.kind == MobKind::Frog && !self.in_water && roll >= 4 {
            let s = self.frog.as_mut().unwrap();
            s.pose = 3;
            s.croak = 60;
            return (None, 0.0);
        }
        if roll < 2 || (self.in_water && roll < 5) {
            if self.in_water {
                if let Some(goal) = self.swim_stroll_pos(w, ctx, rng) {
                    self.walk_to(w, goal.floor().as_ivec3(), 0, speed, false);
                }
            } else {
                self.try_stroll(w, ctx, rng);
            }
        }
        (self.follow_path(w, speed, false), speed)
    }
    pub(super) fn frog_fall<W: MobWorld + ?Sized>(&mut self, from_y: f64, w: &W, rng: &mut Rng) {
        if self.kind != MobKind::Frog || !self.alive() {
            return;
        }
        let s = self.frog.as_mut().unwrap();
        if self.in_water {
            s.fall_distance = 0.0;
            return;
        }
        s.fall_distance += (from_y - self.pos.y).max(0.0);
        if !self.on_ground {
            return;
        }
        let below = w.block((self.pos - DVec3::Y * 0.05).floor().as_ivec3());
        let distance = std::mem::take(&mut s.fall_distance);
        if below == Some(crate::world::stone_blocks::SLIME_BLOCK) {
            return;
        }
        let boost = self
            .effects
            .get(crate::simulation::effects::Effect::JumpBoost)
            .map_or(0.0, |e| f64::from(e.amplifier) + 1.0);
        let damage = ((distance - 3.0 - boost).ceil() - 5.0).max(0.0) as f32;
        if damage > 0.0 {
            self.damage(damage, None, rng);
        }
    }

    /// Mid-jump keeps horizontal motion (`setDiscardFriction(true)`).
    pub(super) fn frog_long_jump_tick<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng) -> bool {
        if self.kind != MobKind::Frog || self.frog.as_ref().is_none_or(|s| s.pose != 4) {
            return false;
        }
        let mut dm = self.vel * 0.05;
        let delta = dm;
        let hit = physics::move_box(w, &mut self.pos, &mut dm, delta, self.kind.shape());
        self.on_ground = hit.on_ground;
        self.blocked = hit.horizontal;
        dm.y -= 0.08;
        if hit.on_ground || self.in_water {
            dm.x *= 0.10000000149011612;
            dm.z *= 0.10000000149011612;
            let s = self.frog.as_mut().unwrap();
            s.pose = 0;
            s.sounds |= 1 << crate::entity::frog::FrogSound::Land as u8;
            s.jump_cooldown = 100 + rng.next_int(41) as u16;
        }
        self.vel = dm * 20.0;
        true
    }
}

/// The ballistic candidate calculation used by LongJumpToRandomPos. Choose
/// distance-squared weighted positions; at most twenty candidates. Candidate
/// range and velocity limit bound the swept collision checks.
fn long_jump<W: MobWorld + ?Sized>(m: &Mob, w: &W, rng: &mut Rng) -> Option<DVec3> {
    let origin = m.pos.floor().as_ivec3();
    let mut targets = Vec::new();
    for dy in -2..=2 {
        for dz in -4..=4 {
            for dx in -4..=4 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let at = origin + IVec3::new(dx, dy, dz);
                let Some(below) = w.block(at - IVec3::Y) else {
                    continue;
                };
                let preferred = matches!(below.name(), "lily pad" | "big dripleaf");
                let path_type = crate::entity::path::static_type(w, at);
                let trapdoor = crate::entity::path::state_type(Some(below)) == crate::entity::path::PathType::Trapdoor;
                if (!preferred && !trapdoor && (!below.is_opaque() || m.caps(false).malus.get(path_type) != 0.0))
                    || below.holds_water()
                    || below.is_lava()
                    || w.block(at) != Some(Block::AIR)
                {
                    continue;
                }
                if w.block(at + IVec3::Y).is_some_and(|b| b.holds_water() || b.is_lava()) {
                    continue;
                }
                let pos = at.as_dvec3() + DVec3::splat(0.5);
                if physics::overlaps_solid(w, pos, m.shape()) {
                    continue;
                }
                targets.push((pos, (dx * dx + dy * dy + dz * dz) as u32, preferred));
            }
        }
    }
    if rng.chance(0.5) && targets.iter().any(|t| t.2) {
        targets.retain(|t| t.2);
    }
    for _ in 0..20 {
        let total = targets.iter().map(|t| t.1).sum::<u32>();
        if total == 0 {
            break;
        }
        let mut selected = rng.next_int(total);
        let index = targets
            .iter()
            .position(|t| {
                if selected < t.1 {
                    true
                } else {
                    selected -= t.1;
                    false
                }
            })
            .unwrap();
        let (target, _, _) = targets.swap_remove(index);
        let mut angles = [65, 70, 75, 80];
        for n in (1..angles.len()).rev() {
            angles.swap(n, rng.next_int(n as u32 + 1) as usize);
        }
        for angle in angles {
            if let Some(v) = jump_vector(m, w, target, angle) {
                return Some(v);
            }
        }
    }
    None
}

/// LongJumpUtil uses an unscaled arc for swept clearance, then scales the
/// selected launch vector by the Java float 0.95.
fn jump_vector<W: MobWorld + ?Sized>(m: &Mob, w: &W, target: DVec3, degrees: u32) -> Option<DVec3> {
    jump_vector_for(m, w, target, degrees, (0.42 * f64::from(3.571_428_8_f32)) as f32, true)
}

/// `LongJumpUtil.calculateJumpVectorForAngle` with its speed limit and
/// clearance flag (breezes jump without the swept check).
pub(super) fn jump_vector_for<W: MobWorld + ?Sized>(
    m: &Mob,
    w: &W,
    target: DVec3,
    degrees: u32,
    max_speed: f32,
    clearance: bool,
) -> Option<DVec3> {
    let horizontal = DVec3::new(target.x - m.pos.x, 0.0, target.z - m.pos.z);
    let d = target - horizontal.normalize_or_zero() * 0.5 - m.pos;
    let distance = d.x.hypot(d.z);
    let angle_f = degrees as f32 * std::f32::consts::PI / 180.0;
    let angle = f64::from(angle_f);
    let cos = angle.cos();
    let v2 = distance * distance * 0.08 / (distance * f64::from(2.0 * angle_f).sin() - 2.0 * d.y * cos * cos);
    if !v2.is_finite() || v2 < 0.0 || v2.sqrt() > f64::from(max_speed) || distance < 1e-9 {
        return None;
    }
    let speed = v2.sqrt();
    let direction = DVec3::new(d.x, 0.0, d.z).normalize();
    let vx = speed * cos;
    let vy = speed * angle.sin();
    let samples = if clearance { (distance / vx).ceil() as u32 * 2 } else { 0 };
    let mut previous = None;
    for step in 1..samples {
        let x = distance * f64::from(step) / f64::from(samples);
        let y = angle.tan() * x - x * x * 0.08 / (2.0 * v2 * cos * cos);
        let point = m.pos + direction * x + DVec3::Y * y;
        if let Some(from) = previous {
            let delta: DVec3 = point - from;
            let count = (delta.length() / 0.5).ceil() as u32;
            let mut p = from;
            for n in 0..count {
                p = if n + 1 == count { point } else { p + delta.normalize_or_zero() * (0.5 * 0.8999999761581421) };
                if physics::overlaps_solid(w, p, m.kind.shape()) {
                    return None;
                }
            }
        }
        previous = Some(point);
    }
    Some((direction * vx + DVec3::Y * vy) * 0.949999988079071)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{BlockSource, test_util::Grid};
    struct Floor(Grid);
    impl BlockSource for Floor {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Floor {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn biome(&self, _: i32, _: i32) -> crate::world::terrain::Biome {
            crate::world::terrain::Biome::Swamp
        }
    }
    #[test]
    fn frog_ballistic_arc_uses_java_target_offset_and_rejects_a_ceiling() {
        let mut w = Floor(Grid::flat(1));
        let m = Mob::new(MobKind::Frog, DVec3::new(0.5, 1.0, 0.5), 0.0);
        let v = jump_vector(&m, &w, DVec3::new(4.5, 1.5, 0.5), 70).unwrap();
        // Independently evaluated 26.3 LongJumpUtil formula, target shortened by .5.
        assert!((v.x - 0.220_249_7).abs() < 0.0001, "{v:?}");
        assert!((v.y - 0.605_131_3).abs() < 0.0001, "{v:?}");
        for x in -1..=6 {
            for z in -1..=1 {
                w.0.set(IVec3::new(x, 2, z), Block::STONE);
            }
        }
        assert!(jump_vector(&m, &w, DVec3::new(4.5, 1.5, 0.5), 70).is_none());
    }
    #[test]
    fn frog_fall_reduces_damage_by_five_and_mid_jump_has_no_drag() {
        let w = Floor(Grid::flat(1));
        let mut rng = Rng::new(1);
        let mut m = Mob::new(MobKind::Frog, DVec3::new(0.5, 1.0, 0.5), 0.0);
        m.on_ground = true;
        m.frog_fall(9.0, &w, &mut rng);
        assert_eq!(m.health, 10.0);
        m.frog_fall(10.0, &w, &mut rng);
        assert_eq!(m.health, 9.0);
        m.pos.y = 10.0;
        m.frog.as_mut().unwrap().pose = 4;
        m.vel = DVec3::new(10.0, 8.0, 0.0);
        assert!(m.frog_long_jump_tick(&w, &mut rng));
        assert_eq!(m.vel, DVec3::new(10.0, 6.4, 0.0));
    }
}
