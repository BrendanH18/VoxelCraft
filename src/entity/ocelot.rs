//! Java 26.3 ocelot trust, temptation fright, prey sensing and loaded lifetime.
use super::{
    Ctx, Entities, Mob, MobKind, PlayerId, Target,
    mob::goals::{self, Goal},
};
use crate::{inventory::Stack, particles};
use glam::DVec3;
#[derive(Clone, Debug, Default)]
pub struct State {
    pub trusting: bool,
    pub crouching: bool,
    pub sprinting: bool,
    pub ticks: u32,
    pub(super) prey: Option<(u32, DVec3)>,
    pub(super) tempt_anchor: Option<(PlayerId, DVec3, DVec3)>,
    pub(super) leaping: bool,
    sense: u8,
    carry: f64,
}
impl Mob {
    pub(crate) fn ocelot_tempt_allowed(&mut self, t: &Target) -> bool {
        let Some(s) = &mut self.ocelot else { return true };
        if let Some((_, pos, look)) = s.tempt_anchor.filter(|(id, _, _)| *id == t.id) {
            if !s.trusting && self.pos.distance_squared(t.pos) < 36.0 {
                let turn = |a: DVec3, b: DVec3| {
                    a.length_squared() > 0.5
                        && b.length_squared() > 0.5
                        && ((a.x.atan2(a.z) - b.x.atan2(b.z)).abs().to_degrees() > 5.0
                            || (a.y.clamp(-1.0, 1.0).asin() - b.y.clamp(-1.0, 1.0).asin()).abs().to_degrees() > 5.0)
                };
                if t.pos.distance_squared(pos) > 0.01 || turn(t.look, look) {
                    return false;
                }
                s.tempt_anchor = Some((t.id, pos, t.look));
            } else {
                s.tempt_anchor = Some((t.id, t.pos, t.look));
            }
        } else {
            s.tempt_anchor = Some((t.id, t.pos, t.look));
        }
        true
    }
}
impl Entities {
    /// Ocelots trust globally; this neither assigns an owner nor sits/tames them.
    pub fn try_trust_ocelot(&mut self, index: usize, held: Option<Stack>, at: DVec3) -> bool {
        let Some(m) = self.mobs.get_mut(index).filter(|m| m.kind == MobKind::Ocelot && m.alive()) else { return false };
        if m.ocelot.as_ref().unwrap().trusting
            || !held.is_some_and(|s| m.kind.breeding_food(s.item))
            || at.distance_squared(m.pos) >= 9.0
            || !m.goals.running.is_some_and(|i| matches!(goals::table(m.kind)[i as usize].goal, Goal::Tempt))
        {
            return false;
        }
        if self.rng.next_int(3) == 0 {
            m.ocelot.as_mut().unwrap().trusting = true;
            self.animal_hearts(index);
        } else {
            self.particles.push(particles::Request::Burst(particles::Burst::new(
                particles::Kind::Smoke,
                m.pos + DVec3::Y * 0.5,
                7,
            )));
        }
        true
    }
    pub(super) fn ocelot_upkeep(&mut self, dt: f64, _ctx: &Ctx) {
        if !self.mobs.iter().any(|m| m.ocelot.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        let by_uid: rustc_hash::FxHashMap<u32, usize> = self.mobs.iter().enumerate().map(|(i, m)| (m.uid, i)).collect();
        for i in 0..self.mobs.len() {
            if self.mobs[i].ocelot.is_none() || !self.mobs[i].alive() || !self.mobs[i].simulation_active {
                continue;
            }
            let mut s = self.mobs[i].ocelot.take().unwrap();
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / 0.05).floor().min(u32::MAX as f64) as u32;
            s.carry -= f64::from(ticks) * 0.05;
            s.ticks = s.ticks.saturating_add(ticks);
            s.crouching = false;
            s.sprinting = false;
            s.sense = s.sense.saturating_sub(ticks.min(255) as u8);
            let pos = self.mobs[i].pos;
            let prey = |m: &Mob| m.kind == MobKind::Chicken || (m.kind == MobKind::Turtle && m.baby && !m.in_water);
            s.prey = s.prey.and_then(|(uid, _)| {
                by_uid
                    .get(&uid)
                    .map(|&j| &self.mobs[j])
                    .filter(|m| m.alive() && prey(m) && m.pos.distance_squared(pos) <= 225.0)
                    .map(|m| (uid, m.pos))
            });
            if s.prey.is_none() && s.sense == 0 {
                s.sense = 10;
                let target = self
                    .mob_index
                    .nearest(&self.mobs, pos, 16.0, |m| m.kind == MobKind::Chicken && (m.pos.y - pos.y).abs() <= 4.0)
                    .or_else(|| {
                        self.mob_index.nearest(&self.mobs, pos, 16.0, |m| {
                            m.kind == MobKind::Turtle && m.baby && !m.in_water && (m.pos.y - pos.y).abs() <= 4.0
                        })
                    });
                s.prey = target.map(|j| (self.mobs[j].uid, self.mobs[j].pos));
            }
            self.mobs[i].ocelot = Some(s);
        }
    }
}
#[cfg(test)]
mod tests;
