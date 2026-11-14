//! Java 26.3 polar bear targeting, persistent anger and family sensing.
use super::{Ctx, Entities, MobKind, MobWorld, PlayerId, Rng, nether::Foe};
use crate::{physics::Shape, simulation::difficulty::Difficulty};
use glam::DVec3;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub(super) struct Aim {
    pub foe: Foe,
    pub pos: DVec3,
    pub shape: Shape,
}
#[derive(Clone, Debug)]
pub struct State {
    pub standing: bool,
    pub animation: f32,
    pub(super) angry_at: Option<Foe>,
    pub(super) anger_end: i64,
    pub(super) current: Option<Aim>,
    pub(super) pending: Option<Foe>,
    pub(super) unseen: u16,
    pub(super) warning: u16,
    carry: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            standing: false,
            animation: 0.0,
            angry_at: None,
            anger_end: -1,
            current: None,
            pending: None,
            unseen: 0,
            warning: 0,
            carry: 0.0,
        }
    }
}
impl State {
    pub fn save(&self) -> Value {
        json!({"anger_end":self.anger_end,"player":self.angry_at.and_then(|f|if let Foe::Player(p)=f{Some(p.0)}else{None}),"mob":self.angry_at.and_then(|f|if let Foe::Mob(p)=f{Some(p)}else{None})})
    }
    pub fn load(&mut self, v: &Value) {
        self.anger_end = v["anger_end"].as_i64().unwrap_or(-1);
        self.angry_at = v["player"]
            .as_u64()
            .and_then(|p| u32::try_from(p).ok())
            .map(|p| Foe::Player(PlayerId(p)))
            .or_else(|| v["mob"].as_u64().and_then(|p| u32::try_from(p).ok()).map(Foe::Mob));
    }
    pub(super) fn calm(&mut self) {
        self.angry_at = None;
        self.current = None;
        self.pending = None;
        self.anger_end = -1;
        self.standing = false;
    }
}
fn end(now: i64, rng: &mut Rng) -> i64 {
    now.saturating_add(400 + i64::from(rng.next_int(381)))
}
fn intersects(a: DVec3, ashape: Shape, b: DVec3, bshape: Shape, xz: f64, y: f64) -> bool {
    (a.x - b.x).abs() < ashape.half_width + bshape.half_width + xz
        && (a.z - b.z).abs() < ashape.half_width + bshape.half_width + xz
        && a.y - y < b.y + bshape.height
        && a.y + ashape.height + y > b.y
}
impl Entities {
    pub(super) fn polar_hurt(&mut self, index: usize, foe: Foe) {
        let Some(m) = self.mobs.get(index).filter(|m| m.kind == MobKind::PolarBear && m.alive()) else { return };
        let (pos, shape, baby) = (m.pos, m.shape(), m.baby);
        let now = self.spawn_ticks.min(i64::MAX as u64) as i64;
        let universal = self.neutral_universal_anger && matches!(foe, Foe::Player(_));
        for (i, m) in self.mobs.iter_mut().enumerate() {
            if m.kind != MobKind::PolarBear || !m.alive() || m.baby || (!baby && i != index) || foe == Foe::Mob(m.uid) {
                continue;
            }
            if baby && (!m.simulation_active || !intersects(pos, shape, m.pos, m.shape(), 20.0, 10.0)) {
                continue;
            }
            let s = m.polar_bear.as_mut().unwrap();
            s.angry_at = (!universal).then_some(foe);
            s.pending = (!universal).then_some(foe);
            s.current = None;
            s.anger_end = end(now, &mut self.rng);
            s.unseen = 0;
        }
    }
    fn polar_aim(&self, foe: Foe, ctx: &Ctx) -> Option<Aim> {
        match foe {
            Foe::Player(id) => ctx.players.iter().find(|p| p.id == id && p.targetable && p.alive).map(|p| Aim {
                foe,
                pos: p.pos,
                shape: p.shape,
            }),
            Foe::Mob(id) => self.mobs.iter().find(|m| m.uid == id && m.alive() && m.simulation_active).map(|m| Aim {
                foe,
                pos: m.pos,
                shape: m.shape(),
            }),
        }
    }
    pub(super) fn polar_sense<W: MobWorld + ?Sized>(&mut self, dt: f64, w: &W, ctx: &Ctx) {
        if !self.mobs.iter().any(|m| m.polar_bear.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        let now = self.spawn_ticks.min(i64::MAX as u64) as i64;
        for i in 0..self.mobs.len() {
            if self.mobs[i].kind != MobKind::PolarBear || !self.mobs[i].alive() || !self.mobs[i].simulation_active {
                continue;
            }
            let mut s = self.mobs[i].polar_bear.take().unwrap();
            if !self.mobs[i].alive() {
                s.calm();
                s.animation = (s.animation - (dt.max(0.0) * 20.0) as f32).max(0.0);
                self.mobs[i].polar_bear = Some(s);
                continue;
            }
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / 0.05).floor() as u16;
            s.carry -= f64::from(ticks) * 0.05;
            s.warning = s.warning.saturating_sub(ticks);
            s.animation = (s.animation + if s.standing { f32::from(ticks) } else { -f32::from(ticks) }).clamp(0.0, 6.0);
            let m = &self.mobs[i];
            let pos = m.pos;
            let shape = m.shape();
            if m.baby {
                s.calm();
                self.mobs[i].polar_bear = Some(s);
                continue;
            }
            // A dead player is forgiven by `Entities::player_died` alone.
            if let Some(Foe::Player(id)) = s.angry_at
                && (ctx.players.iter().find(|p| p.id == id).is_some_and(|p| p.spectator || p.alive && !p.targetable)
                    || self.difficulty == Difficulty::Peaceful)
            {
                s.calm();
            }
            if let Some(Foe::Mob(id)) = s.angry_at
                && self.mobs.iter().find(|m| m.uid == id).is_some_and(|m| !m.alive())
            {
                s.calm();
            }
            let pending = s.pending.take();
            let mut target = pending
                .and_then(|f| self.polar_aim(f, ctx))
                .or_else(|| s.current.and_then(|a| self.polar_aim(a.foe, ctx)));
            if let Some(t) = target {
                if pos.distance_squared(t.pos) > 400.0 {
                    target = None;
                } else if !super::mob::line_of_sight(
                    w,
                    pos + DVec3::Y * shape.height * 0.85,
                    t.pos + DVec3::Y * t.shape.height * 0.85,
                ) {
                    s.unseen = s.unseen.saturating_add(ticks);
                    if s.unseen > 60 {
                        target = None;
                    }
                } else {
                    s.unseen = 0;
                }
            }
            if target.is_none() && s.anger_end <= now {
                s.angry_at = None;
                s.anger_end = -1;
            }
            let full = (self.spawn_ticks + u64::from(self.mobs[i].uid)).is_multiple_of(2);
            let player_eligible =
                |p: &&super::Target| p.targetable && p.alive && self.difficulty != Difficulty::Peaceful;
            if target.is_none() && full {
                let mut cub = false;
                self.mob_index.visit(pos, 10.0, |j| {
                    let b = &self.mobs[j];
                    if b.kind == MobKind::PolarBear && b.baby && intersects(pos, shape, b.pos, b.shape(), 8.0, 4.0) {
                        cub = true;
                    }
                });
                if cub && self.rng.next_int(10) == 0 {
                    target = ctx
                        .players
                        .iter()
                        .filter(player_eligible)
                        .filter(|p| pos.distance_squared(p.pos) <= 100.0)
                        .filter(|p| {
                            self.mobs[i].polar_reachable(w, p.pos)
                                && super::mob::line_of_sight(
                                    w,
                                    pos + DVec3::Y * shape.height * 0.85,
                                    p.pos + DVec3::Y * 1.62,
                                )
                        })
                        .min_by(|a, b| pos.distance_squared(a.pos).total_cmp(&pos.distance_squared(b.pos)))
                        .map(|p| Aim { foe: Foe::Player(p.id), pos: p.pos, shape: p.shape });
                }
                if target.is_none() && s.anger_end > now && self.rng.next_int(5) == 0 {
                    let universal = self.neutral_universal_anger && s.angry_at.is_none();
                    target = ctx
                        .players
                        .iter()
                        .filter(player_eligible)
                        .filter(|p| universal || s.angry_at == Some(Foe::Player(p.id)))
                        .filter(|p| {
                            pos.distance_squared(p.pos) <= 400.0
                                && super::mob::line_of_sight(
                                    w,
                                    pos + DVec3::Y * shape.height * 0.85,
                                    p.pos + DVec3::Y * 1.62,
                                )
                        })
                        .min_by(|a, b| pos.distance_squared(a.pos).total_cmp(&pos.distance_squared(b.pos)))
                        .map(|p| Aim { foe: Foe::Player(p.id), pos: p.pos, shape: p.shape });
                }
                if target.is_none() && self.rng.next_int(5) == 0 {
                    target = self
                        .mob_index
                        .nearest(&self.mobs, pos, 20.0, |b| {
                            b.kind == MobKind::Fox
                                && super::mob::line_of_sight(
                                    w,
                                    pos + DVec3::Y * shape.height * 0.85,
                                    b.pos + DVec3::Y * b.shape().height * 0.85,
                                )
                                && self.mobs[i].polar_reachable(w, b.pos)
                        })
                        .map(|j| Aim {
                            foe: Foe::Mob(self.mobs[j].uid),
                            pos: self.mobs[j].pos,
                            shape: self.mobs[j].shape(),
                        });
                }
            }
            // NeutralMob.updatePersistentAnger(level,true) resamples while a target exists.
            if let Some(t) = target {
                s.angry_at = Some(t.foe);
                s.anger_end = end(now, &mut self.rng);
            }
            s.current = target;
            if target.is_none() {
                s.standing = false;
            }
            self.mobs[i].polar_bear = Some(s);
        }
    }
}
#[cfg(test)]
mod tests;
