//! Java 26.3 armadillo threat memory, shell states, scutes and brushing.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng, nether::Foe};
use crate::{
    inventory::Stack,
    item::Item,
    physics::{self, Shape},
};
use glam::DVec3;
use serde_json::{Value, json};
pub const IDLE: u8 = 0;
pub const ROLLING: u8 = 1;
pub const SCARED: u8 = 2;
pub const UNROLLING: u8 = 3;
#[derive(Clone, Debug)]
pub struct State {
    pub pose: u8,
    pub pose_ticks: u16,
    pub peeking: u8,
    pub scute_time: i32,
    danger: u16,
    sense: u8,
    peek_in: u16,
    danger_was_around: bool,
    attacker: Option<(Foe, u16)>,
    carry: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            pose: IDLE,
            pose_ticks: 0,
            peeking: 0,
            scute_time: 6000,
            danger: 0,
            sense: 0,
            peek_in: 0,
            danger_was_around: false,
            attacker: None,
            carry: 0.0,
        }
    }
}
impl State {
    pub fn scared(&self) -> bool {
        self.pose != IDLE
    }
    pub fn hide(&self) -> bool {
        self.peeking == 0
            && match self.pose {
                ROLLING => self.pose_ticks > 5,
                SCARED => true,
                UNROLLING => self.pose_ticks < 26,
                _ => false,
            }
    }
    fn switch(&mut self, pose: u8) {
        self.pose = pose;
        self.pose_ticks = 0;
    }
    pub fn save(&self) -> Value {
        json!({"state":(["idle","rolling","scared","unrolling"][self.pose.min(3) as usize]),"scute_time":self.scute_time,"danger":self.danger})
    }
    pub fn load(&mut self, v: &Value) {
        self.pose = match v["state"].as_str() {
            Some("rolling") => ROLLING,
            Some("scared") => SCARED,
            Some("unrolling") => UNROLLING,
            _ => IDLE,
        };
        self.pose_ticks = 0;
        self.scute_time =
            v["scute_time"].as_i64().unwrap_or(i64::from(self.scute_time)).clamp(i32::MIN as i64, i32::MAX as i64)
                as i32;
        self.danger = v["danger"].as_u64().unwrap_or(0).min(80) as u16;
    }
    pub(super) fn look(&self) -> Value {
        json!({"pose":self.pose,"ticks":self.pose_ticks,"peek":self.peeking})
    }
    pub(super) fn apply_look(&mut self, v: &Value) {
        self.pose = v["pose"].as_u64().unwrap_or(0).min(3) as u8;
        self.pose_ticks = v["ticks"].as_u64().unwrap_or(0).min(u16::MAX as u64) as u16;
        self.peeking = v["peek"].as_u64().unwrap_or(0).min(50) as u8;
    }
}
pub(super) fn fresh(m: &mut Mob, rng: &mut Rng) {
    if let Some(s) = &mut m.armadillo {
        s.scute_time = 6000 + rng.next_int(6000) as i32;
        s.sense = rng.next_int(5) as u8;
    }
}
fn intersects(pos: DVec3, s: Shape, p: DVec3, o: Shape) -> bool {
    (pos.x - p.x).abs() < s.half_width + o.half_width + 7.0
        && (pos.z - p.z).abs() < s.half_width + o.half_width + 7.0
        && pos.y - 2.0 < p.y + o.height
        && pos.y + s.height + 2.0 > p.y
}
impl Mob {
    fn can_stay_rolled<W: MobWorld + ?Sized>(&self, w: &W) -> bool {
        self.ai != super::mob::Ai::Panic
            && !self.in_water
            && !physics::touches_block(w, self.pos, self.shape(), |b| b.is_water() || b.is_lava() || b.is_waterlogged())
            && self.leash.is_none()
            && self.riding.is_none()
    }
    fn roll_up(&mut self) {
        if self.armadillo.as_deref().is_none_or(State::scared) {
            return;
        }
        self.armadillo.as_mut().unwrap().switch(ROLLING);
        self.vel = DVec3::ZERO;
        self.stop_navigation();
        if let Some(a) = &mut self.animal {
            a.love = 0.0;
            a.courtship = 0.0;
            a.partner = None;
            a.breed_now = None;
        }
    }
}
impl Entities {
    pub(super) fn armadillo_hurt(&mut self, index: usize, foe: Foe) {
        let Some(m) = self.mobs.get_mut(index).filter(|m| m.kind == MobKind::Armadillo && m.alive()) else { return };
        let s = m.armadillo.as_mut().unwrap();
        s.danger = 80;
        s.attacker = Some((foe, 100));
        if m.ai != super::mob::Ai::Panic && !m.in_water && m.leash.is_none() && m.riding.is_none() {
            m.roll_up();
        }
    }
    /// Adult brushing has no cooldown, does not reset shedding and is not gated by mob drops.
    pub fn brush_armadillo(&mut self, index: usize) -> bool {
        let Some(m) =
            self.mobs.get(index).filter(|m| m.kind == MobKind::Armadillo && m.alive() && !m.baby && m.age >= 0)
        else {
            return false;
        };
        let pos = m.pos;
        self.scatter(Stack::new(Item::ARMADILLO_SCUTE, 1), pos + DVec3::Y * 0.2);
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::Armadillo), pos });
        true
    }
    pub(super) fn armadillo_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        w: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.armadillo.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        for i in 0..self.mobs.len() {
            if self.mobs[i].armadillo.is_none() || !self.mobs[i].alive() || !self.mobs[i].simulation_active {
                continue;
            }
            let stay = self.mobs[i].can_stay_rolled(w);
            let mut s = self.mobs[i].armadillo.take().unwrap();
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / 0.05).floor() as u16;
            s.carry -= f64::from(ticks) * 0.05;
            for _ in 0..ticks.min(100) {
                s.pose_ticks = s.pose_ticks.saturating_add(1);
                s.danger = s.danger.saturating_sub(1);
                s.peek_in = s.peek_in.saturating_sub(1);
                s.peeking = s.peeking.saturating_sub(1);
                s.attacker = s.attacker.and_then(|(a, t)| (t > 1).then_some((a, t - 1)));
                s.scute_time = s.scute_time.saturating_sub(1);
                if s.scute_time <= 0 && !self.mobs[i].baby && self.mob_loot {
                    let pos = self.mobs[i].pos;
                    self.scatter(Stack::new(Item::ARMADILLO_SCUTE, 1), pos + DVec3::Y * 0.2);
                    s.scute_time = 6000 + self.rng.next_int(6000) as i32;
                    events.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::Armadillo), pos });
                }
                if !stay {
                    s.danger = 0;
                    if s.scared() {
                        s.switch(IDLE);
                        events.push(EntityEvent::Sound {
                            sound: MobSound::Ambient(MobKind::Armadillo),
                            pos: self.mobs[i].pos,
                        });
                    }
                    continue;
                }
                s.sense = s.sense.saturating_sub(1);
                if s.sense == 0 {
                    s.sense = 5;
                    let m = &self.mobs[i];
                    let (pos, shape) = (m.pos, m.shape());
                    let mut threat = false;
                    self.mob_index.visit(pos, 9.0, |j| {
                        let other = &self.mobs[j];
                        if j != i
                            && (other.kind.creature() == crate::enchant::Creature::Undead
                                || s.attacker.is_some_and(|(a, _)| a == Foe::Mob(other.uid)))
                            && intersects(pos, shape, other.pos, other.shape())
                        {
                            threat = true;
                        }
                    });
                    threat |= ctx.players.iter().any(|p| {
                        p.alive
                            && !p.spectator
                            && (p.sprinting || p.passenger || s.attacker.is_some_and(|(a, _)| a == Foe::Player(p.id)))
                            && intersects(pos, shape, p.pos, p.shape)
                    });
                    if threat {
                        s.danger = 80;
                    }
                }
                if s.pose == IDLE && s.danger > 0 && self.mobs[i].on_ground {
                    s.switch(ROLLING);
                    let m = &mut self.mobs[i];
                    m.vel = DVec3::ZERO;
                    m.stop_navigation();
                    if let Some(a) = &mut m.animal {
                        a.love = 0.0;
                        a.courtship = 0.0;
                        a.partner = None;
                        a.breed_now = None;
                    }
                    events.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::Armadillo), pos: m.pos });
                }
                if s.danger == 0 {
                    if s.scared() {
                        s.switch(IDLE);
                        events.push(EntityEvent::Sound {
                            sound: MobSound::Ambient(MobKind::Armadillo),
                            pos: self.mobs[i].pos,
                        });
                    }
                    continue;
                }
                if s.pose == ROLLING && s.pose_ticks > 10 {
                    s.switch(SCARED);
                    continue;
                }
                let around = s.danger > 75;
                if around != s.danger_was_around {
                    s.peek_in = 150 + self.rng.next_int(301) as u16;
                }
                s.danger_was_around = around;
                if s.pose == SCARED {
                    if s.peek_in == 0 && self.mobs[i].on_ground && around {
                        s.peeking = 50;
                        s.peek_in = 150 + self.rng.next_int(301) as u16;
                        events.push(EntityEvent::Sound {
                            sound: MobSound::Ambient(MobKind::Armadillo),
                            pos: self.mobs[i].pos,
                        });
                    }
                    if s.danger < 30 {
                        s.switch(UNROLLING);
                    }
                } else if s.pose == UNROLLING && s.danger > 30 {
                    s.switch(SCARED);
                }
            }
            self.mobs[i].armadillo = Some(s);
        }
    }
}
#[cfg(test)]
mod tests;
