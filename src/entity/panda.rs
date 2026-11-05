//! Java 26.3 panda genes and the action/retaliation state on shared animal goals.
use super::{Ctx, Entities, Mob, MobKind, MobWorld, PlayerId, Rng, nether::Foe};
use crate::{
    inventory::Stack,
    item::Item,
    physics::Shape,
    world::{block::Block, overworld_blocks as o},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Gene {
    #[default]
    Normal,
    Lazy,
    Worried,
    Playful,
    Brown,
    Weak,
    Aggressive,
}
impl Gene {
    pub const ALL: [Self; 7] =
        [Self::Normal, Self::Lazy, Self::Worried, Self::Playful, Self::Brown, Self::Weak, Self::Aggressive];
    pub fn name(self) -> &'static str {
        ["normal", "lazy", "worried", "playful", "brown", "weak", "aggressive"][self as usize]
    }
    fn read(v: &Value) -> Self {
        Self::ALL.into_iter().find(|g| Some(g.name()) == v.as_str()).unwrap_or_default()
    }
    pub(super) fn random(rng: &mut Rng) -> Self {
        Self::from_roll(rng.next_int(16))
    }
    fn from_roll(n: u32) -> Self {
        match n {
            0 => Self::Lazy,
            1 => Self::Worried,
            2 => Self::Playful,
            4 => Self::Aggressive,
            3 | 5..=8 => Self::Weak,
            9..=10 => Self::Brown,
            _ => Self::Normal,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Aim {
    foe: Foe,
    pos: DVec3,
    shape: Shape,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    pub main: Gene,
    pub hidden: Gene,
    pub sitting: bool,
    pub on_back: bool,
    pub rolling: u8,
    pub sneezing: u8,
    pub eating: u16,
    pub unhappy: u8,
    pub scared: bool,
    pub(super) target: Option<Aim>,
    got_bamboo: bool,
    did_bite: bool,
    unseen: u16,
    ticks: u32,
    unhappy_cooldown: u32,
    sit_cooldown: u32,
    back_cooldown: u32,
    forage: Option<DVec3>,
    roll_delta: DVec3,
    fall_distance: f64,
    carry: f64,
    pub(super) spit: Option<Stack>,
}
impl State {
    pub fn variant(&self) -> Gene {
        if matches!(self.main, Gene::Brown | Gene::Weak) && self.main != self.hidden { Gene::Normal } else { self.main }
    }
    pub(super) fn can_act(&self) -> bool {
        !self.on_back && !self.scared && !self.sitting && self.eating == 0 && self.rolling == 0
    }
    pub fn save(&self) -> Value {
        json!({"main":self.main.name(),"hidden":self.hidden.name()})
    }
    pub fn load(&mut self, v: &Value) {
        self.main = Gene::read(&v["main"]);
        self.hidden = Gene::read(&v["hidden"]);
    }
    pub(super) fn look(&self) -> Value {
        let mut v = self.save();
        v["sitting"] = json!(self.sitting);
        v["scared"] = json!(self.scared);
        v["back"] = json!(self.on_back);
        v["roll"] = json!(self.rolling);
        v["sneeze"] = json!(self.sneezing);
        v["eat"] = json!(self.eating);
        v["unhappy"] = json!(self.unhappy);
        v
    }
    pub(super) fn apply_look(&mut self, v: &Value) {
        self.load(v);
        self.sitting = v["sitting"] == true;
        self.scared = v["scared"] == true;
        self.on_back = v["back"] == true;
        self.rolling = v["roll"].as_u64().unwrap_or(0).min(32) as u8;
        self.sneezing = v["sneeze"].as_u64().unwrap_or(0).min(21) as u8;
        self.eating = v["eat"].as_u64().unwrap_or(0).min(u16::MAX as u64) as u16;
        self.unhappy = v["unhappy"].as_u64().unwrap_or(0).min(32) as u8;
    }
    fn allele(&self, rng: &mut Rng) -> Gene {
        if rng.next_int(2) == 0 { self.main } else { self.hidden }
    }
}
pub(super) fn inherit(child: &mut Mob, a: &Mob, b: &Mob, rng: &mut Rng) {
    let (Some(c), Some(a), Some(b)) = (&mut child.panda, &a.panda, &b.panda) else { return };
    let (a, b) = if rng.next_int(2) == 0 { (a, b) } else { (b, a) };
    c.main = a.allele(rng);
    c.hidden = b.allele(rng);
    if rng.next_int(32) == 0 {
        c.main = Gene::random(rng);
    }
    if rng.next_int(32) == 0 {
        c.hidden = Gene::random(rng);
    }
    child.health = child.max_health();
}
fn edible(item: Item) -> bool {
    item == Item::from_block(o::BAMBOO) || item == Item::CAKE
}
fn in_box(pos: DVec3, shape: Shape, p: DVec3, other: Shape, xz: f64, y: f64) -> bool {
    (pos.x - p.x).abs() < shape.half_width + other.half_width + xz
        && (pos.z - p.z).abs() < shape.half_width + other.half_width + xz
        && pos.y - y < p.y + other.height
        && pos.y + shape.height + y > p.y
}
impl Mob {
    pub(super) fn panda_breed_allowed<W: MobWorld + ?Sized>(&mut self, w: &W) -> bool {
        let Some(s) = &mut self.panda else { return true };
        if s.unhappy > 0 {
            return false;
        }
        let p = self.pos.floor().as_ivec3();
        for y in 0..3 {
            for x in -7..=7 {
                for z in -7..=7 {
                    if w.block(p + IVec3::new(x, y, z))
                        .is_some_and(|b| matches!(b, o::BAMBOO | o::BAMBOO_SMALL_LEAVES | o::BAMBOO_LARGE_LEAVES))
                    {
                        return true;
                    }
                }
            }
        }
        if s.ticks >= s.unhappy_cooldown {
            s.unhappy = 32;
            s.unhappy_cooldown = s.ticks.saturating_add(600);
        }
        false
    }
    /// Action priority on the shared selector; custom sit is 7, attack is 3.
    fn panda_priority(&self) -> u8 {
        if let Some(i) = self.goals.running {
            super::mob::goals::table(self.kind)[i as usize].priority
        } else if self.ai == super::mob::Ai::Panic {
            2
        } else {
            u8::MAX
        }
    }
}
impl Entities {
    pub(super) fn use_panda(
        &mut self,
        index: usize,
        held: Option<Stack>,
        player: PlayerId,
        infinite: bool,
    ) -> Option<bool> {
        let m = self.mobs.get_mut(index)?;
        if !m.alive() {
            return None;
        }
        let s = m.panda.as_mut()?;
        if s.scared {
            return None;
        }
        if s.on_back {
            s.on_back = false;
            s.back_cooldown = s.ticks.saturating_add(200);
            return Some(false);
        }
        if !held.is_some_and(|h| m.kind.breeding_food(h.item)) {
            return None;
        }
        if s.target.is_some() {
            s.got_bamboo = true;
        }
        let start_love = !m.baby && m.age == 0 && m.animal.as_ref().unwrap().love <= 0.0;
        if m.baby || m.age < 0 {
            m.feed_growth();
        } else if m.age == 0 && m.animal.as_ref().unwrap().love <= 0.0 {
            let a = m.animal.as_mut().unwrap();
            a.love = super::animals::LOVE_SECONDS;
            a.love_by = Some(player);
        } else if !s.sitting && !m.in_water {
            s.sitting = true;
            s.eating = 1;
            let a = m.animal.as_mut().unwrap();
            if let Some(old) = a.mouth.take().filter(|_| !infinite) {
                self.items.push(super::item::ItemEntity::new(old, m.pos, DVec3::ZERO, 0.5, &mut self.rng));
            }
            a.mouth = Some(Stack::new(held.unwrap().item, 1));
        } else {
            return None;
        }
        m.persistent = true;
        if start_love {
            self.animal_hearts(index);
        }
        Some(true)
    }
    pub(super) fn panda_hurt(&mut self, index: usize, foe: Foe) {
        let Some(victim) = self.mobs.get(index).filter(|m| m.kind == MobKind::Panda && m.alive()) else { return };
        let (pos, shape) = (victim.pos, victim.shape());
        for (i, m) in self.mobs.iter_mut().enumerate() {
            if m.kind != MobKind::Panda || !m.alive() || foe == Foe::Mob(m.uid) {
                continue;
            }
            let eligible = i == index
                || m.simulation_active
                    && in_box(pos, shape, m.pos, m.shape(), 16.0, 10.0)
                    && m.panda.as_ref().is_some_and(|s| s.variant() == Gene::Aggressive && s.target.is_none());
            if eligible {
                let s = m.panda.as_mut().unwrap();
                s.target = Some(Aim { foe, pos, shape });
                s.got_bamboo = false;
                s.did_bite = false;
                s.unseen = 0;
            }
        }
    }
    pub(super) fn panda_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        w: &W,
        ctx: &Ctx,
        events: &mut Vec<super::EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.panda.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        let mut sneezes = Vec::new();
        for i in 0..self.mobs.len() {
            if !self.mobs[i].simulation_active || !self.mobs[i].alive() || self.mobs[i].panda.is_none() {
                continue;
            }
            let priority = self.mobs[i].panda_priority();
            let mut s = self.mobs[i].panda.take().unwrap();
            s.scared = s.variant() == Gene::Worried && self.thundering;
            if self.mobs[i].in_water {
                s.fall_distance = 0.0;
            }
            if s.variant() == Gene::Worried {
                if s.scared && !self.mobs[i].in_water {
                    s.sitting = true;
                    s.eating = 0;
                } else if s.eating == 0 {
                    s.sitting = false;
                }
            }
            let target = s
                .target
                .and_then(|a| match a.foe {
                    Foe::Player(id) => ctx
                        .players
                        .iter()
                        .find(|p| {
                            p.id == id
                                && p.alive
                                && p.targetable
                                && self.difficulty != crate::simulation::difficulty::Difficulty::Peaceful
                        })
                        .map(|p| Aim { foe: a.foe, pos: p.pos, shape: p.shape }),
                    Foe::Mob(id) => self
                        .mobs
                        .iter()
                        .find(|m| m.uid == id && m.alive() && m.simulation_active)
                        .map(|m| Aim { foe: a.foe, pos: m.pos, shape: m.shape() }),
                })
                .filter(|a| a.pos.distance_squared(self.mobs[i].pos) <= 256.0);
            s.target = if s.did_bite || s.got_bamboo { None } else { target };
            if s.target.is_none() {
                s.got_bamboo = false;
                s.did_bite = false;
                s.unseen = 0;
            }
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / 0.05).floor() as u16;
            s.carry -= f64::from(ticks) * 0.05;
            for _ in 0..ticks.min(100) {
                s.ticks = s.ticks.saturating_add(1);
                let m = &mut self.mobs[i];
                let full = (u64::from(s.ticks) + u64::from(m.uid)).is_multiple_of(2);
                if let Some(a) = s.target {
                    if super::mob::line_of_sight(
                        w,
                        m.pos + DVec3::Y * m.shape().height * 0.85,
                        a.pos + DVec3::Y * a.shape.height * 0.85,
                    ) {
                        s.unseen = 0;
                    } else {
                        s.unseen = s.unseen.saturating_add(1);
                        if s.unseen > 300 {
                            s.target = None;
                        }
                    }
                }
                if matches!(s.unhappy, 29 | 14) {
                    events.push(super::EntityEvent::Sound {
                        sound: super::MobSound::Ambient(MobKind::Panda),
                        pos: m.pos,
                    });
                }
                s.unhappy = s.unhappy.saturating_sub(1);
                if s.sneezing > 0 {
                    s.sneezing += 1;
                    if s.sneezing > 21 {
                        s.sneezing = 0;
                        sneezes.push((m.pos, m.shape()));
                        events.push(super::EntityEvent::Sound {
                            sound: super::MobSound::Ambient(MobKind::Panda),
                            pos: m.pos,
                        });
                    }
                }
                if s.rolling > 0 {
                    s.rolling += 1;
                    if s.rolling > 32 {
                        s.rolling = 0;
                    } else if matches!(s.rolling, 7 | 15 | 23) {
                        m.vel.x = 0.0;
                        m.vel.z = 0.0;
                        if m.on_ground {
                            m.vel.y = 5.4;
                        }
                    } else {
                        m.vel.x = s.roll_delta.x;
                        m.vel.z = s.roll_delta.z;
                    }
                }
                if !s.sitting || m.animal.as_ref().unwrap().mouth.is_none() {
                    s.eating = 0;
                } else if s.eating == 0 && !s.scared && self.rng.next_int(80) == 1 {
                    s.eating = 1;
                }
                if s.eating > 0 {
                    if s.eating > 80 && self.rng.next_int(20) == 1 {
                        if s.eating > 100 && m.animal.as_ref().unwrap().mouth.is_some_and(|h| edible(h.item)) {
                            m.animal.as_mut().unwrap().mouth = None;
                            s.sitting = false;
                        }
                        s.eating = 0;
                    } else {
                        s.eating = s.eating.saturating_add(1);
                    }
                }
                if full {
                    if s.on_back
                        && (m.in_water
                            || s.variant() != Gene::Lazy && self.rng.next_int(300) == 1
                            || self.rng.next_int(1000) == 1)
                    {
                        s.on_back = false;
                        s.back_cooldown = s.ticks.saturating_add(200);
                    }
                    if s.sitting
                        && !s.scared
                        && (m.in_water
                            || s.variant() != Gene::Lazy && self.rng.next_int(300) == 1
                            || self.rng.next_int(1000) == 1)
                    {
                        s.sitting = false;
                        if let Some(h) = m.animal.as_mut().unwrap().mouth.take() {
                            self.items.push(super::item::ItemEntity::new(h, m.pos, DVec3::ZERO, 0.5, &mut self.rng));
                            s.sit_cooldown = s.ticks.saturating_add(
                                (10 + self.rng.next_int(if s.variant() == Gene::Lazy { 50 } else { 150 })) * 20,
                            );
                        }
                    }
                    if m.baby
                        && s.sneezing == 0
                        && s.can_act()
                        && (s.variant() == Gene::Weak && self.rng.next_int(250) == 1 || self.rng.next_int(3000) == 1)
                    {
                        s.sneezing = 1;
                    }
                    if s.can_act()
                        && s.variant() == Gene::Lazy
                        && s.ticks > s.back_cooldown
                        && self.rng.next_int(200) == 1
                    {
                        s.on_back = true;
                    }
                    if priority > 12 && (m.baby || s.variant() == Gene::Playful) && m.on_ground && s.can_act() {
                        let d = DVec3::new(f64::from(m.yaw.cos()), 0.0, f64::from(m.yaw.sin()));
                        let edge = IVec3::new(
                            if d.x.abs() > 0.5 { d.x.signum() as i32 } else { 0 },
                            -1,
                            if d.z.abs() > 0.5 { d.z.signum() as i32 } else { 0 },
                        );
                        if w.block(m.pos.floor().as_ivec3() + edge) == Some(Block::AIR)
                            || s.variant() == Gene::Playful && self.rng.next_int(30) == 1
                            || self.rng.next_int(250) == 1
                        {
                            s.rolling = 1;
                            s.roll_delta = m.vel.with_y(0.0) + d * if m.baby { 2.0 } else { 4.0 };
                            m.vel = s.roll_delta + DVec3::Y * 5.4;
                        }
                    }
                    if priority > 7
                        && s.target.is_none()
                        && !m.baby
                        && !m.in_water
                        && s.can_act()
                        && s.unhappy == 0
                        && s.ticks >= s.sit_cooldown
                    {
                        let mouth = m.animal.as_ref().unwrap().mouth;
                        if mouth.is_some() {
                            s.sitting = true;
                            s.forage = None;
                        } else {
                            s.forage = self
                                .items
                                .iter()
                                .find(|h| {
                                    h.health > 0
                                        && h.pickup_delay <= 0.0
                                        && edible(h.stack.item)
                                        && in_box(m.pos, m.shape(), h.pos, Shape::new(0.125, 0.25), 6.0, 6.0)
                                })
                                .map(|h| h.pos);
                        }
                    }
                }
                if self.villager_griefing
                    && m.animal.as_ref().unwrap().mouth.is_none()
                    && let Some(j) = self.items.iter().position(|h| {
                        h.health > 0
                            && h.pickup_delay <= 0.0
                            && edible(h.stack.item)
                            && in_box(m.pos, m.shape(), h.pos, Shape::new(0.125, 0.25), 1.0, 0.0)
                    })
                {
                    // Java picks up the entire stack, and consumes the entire hand at the end.
                    m.animal.as_mut().unwrap().mouth = Some(self.items.swap_remove(j).stack);
                    if !m.baby && !m.in_water && priority > 7 {
                        s.sitting = true;
                        s.forage = None;
                    }
                }
            }
            self.mobs[i].panda = Some(s);
        }
        for (pos, shape) in sneezes {
            let mut burst =
                crate::particles::Burst::new(crate::particles::Kind::Effect, pos + DVec3::Y * shape.height * 0.85, 6);
            burst.color = Some([0.6, 0.8, 0.5, 1.0]);
            self.particles.push(crate::particles::Request::Burst(burst));
            for m in &mut self.mobs {
                if m.kind == MobKind::Panda
                    && !m.baby
                    && m.on_ground
                    && !m.in_water
                    && m.panda.as_ref().is_some_and(|s| s.can_act())
                    && in_box(pos, shape, m.pos, m.shape(), 10.0, 10.0)
                {
                    m.vel.y = 8.4 + f64::from(m.effects.jump_boost()) * 2.0;
                }
            }
            if self.mob_loot && self.rng.next_int(700) == 0 {
                self.items.push(super::item::ItemEntity::new(
                    Stack::new(Item::SLIME_BALL, 1),
                    pos,
                    DVec3::ZERO,
                    0.5,
                    &mut self.rng,
                ));
            }
        }
    }
}
impl Mob {
    pub(super) fn panda_land_tick<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        wish: Option<DVec3>,
        speed: f64,
        rng: &mut Rng,
    ) {
        let from = self.pos.y;
        let mut dm = self.vel * 0.05;
        for v in [&mut dm.x, &mut dm.y, &mut dm.z] {
            if v.abs() < 0.003 {
                *v = 0.0;
            }
        }
        let input =
            if let Some(dir) = wish.filter(|_| self.alive() && self.panda.as_deref().is_some_and(State::can_act)) {
                let target = dir.z.atan2(dir.x) as f32;
                let delta =
                    (target - self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                self.yaw += delta.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
                self.move_ctl.cur = crate::entity::path::java_speed(speed) * self.effects.speed_factor();
                DVec3::Z * (self.move_ctl.cur * 0.98)
            } else {
                self.move_ctl.cur = 0.0;
                DVec3::ZERO
            };
        self.travel_in_air(w, &mut dm, input);
        self.vel = dm * 20.0;
        let s = self.panda.as_mut().unwrap();
        s.fall_distance += (from - self.pos.y).max(0.0);
        if self.on_ground {
            let distance = std::mem::take(&mut s.fall_distance);
            let below = w.block((self.pos - DVec3::Y * 0.05).floor().as_ivec3());
            let damage = (distance - 3.0 - f64::from(self.effects.jump_boost())).ceil().max(0.0) as f32;
            if damage > 0.0 && below != Some(crate::world::stone_blocks::SLIME_BLOCK) {
                self.damage(damage, None, rng);
            }
        }
    }
    pub(super) fn panda_think<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        rng: &mut Rng,
        events: &mut Vec<super::EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let priority = self.panda_priority();
        let s = self.panda.as_mut()?;
        if s.sitting && !s.scared && priority < 7 {
            s.sitting = false;
            s.eating = 0;
            if let Some(h) = self.animal.as_mut().unwrap().mouth.take() {
                s.spit = Some(h);
                s.sit_cooldown =
                    s.ticks.saturating_add((10 + rng.next_int(if s.variant() == Gene::Lazy { 50 } else { 150 })) * 20);
            }
        }
        if !s.can_act() {
            self.nav.stop();
            return Some((None, 0.0));
        }
        if let Some(a) = s.target
            && priority > 2
        {
            self.stop_goal(w);
            let d = a.pos - self.pos;
            let dir = d.with_y(0.0).normalize_or(DVec3::X);
            let sees = super::mob::line_of_sight(
                w,
                self.pos + DVec3::Y * self.shape().height * 0.85,
                a.pos + DVec3::Y * a.shape.height * 0.85,
            );
            let reach = 2.0399999618530273_f64.sqrt() - 0.6000000238418579;
            if in_box(self.pos, self.shape(), a.pos, a.shape, reach, 0.0) && sees && self.attack_cooldown <= 0.0 {
                self.attack_cooldown = 1.0;
                self.attack_anim = 0.35;
                let s = self.panda.as_mut().unwrap();
                if s.variant() != Gene::Aggressive {
                    s.did_bite = true;
                }
                match a.foe {
                    Foe::Player(player) => {
                        events.push(super::EntityEvent::PlayerHit {
                            player,
                            damage: 6.0,
                            knockback: (dir * 6.0 + DVec3::Y * 5.0).as_vec3(),
                            java: Some(crate::knockback::Hit::melee(
                                self.pos,
                                a.pos,
                                (-dir.x).atan2(dir.z).to_degrees() as f32,
                                0.0,
                            )),
                            cause: "was slain by a panda",
                        });
                        events.push(super::EntityEvent::OwnerHurt { player, attacker: self.uid });
                    }
                    Foe::Mob(target) => events.push(super::EntityEvent::MobHit {
                        target,
                        attacker: self.uid,
                        damage: 6.0,
                        knockback: dir * 4.0 + DVec3::Y * 2.0,
                    }),
                }
            }
            let speed = self.goal_speed(f64::from(1.2_f32));
            return Some((self.pursue(w, a.pos, speed, sees, true, rng), speed));
        }
        if let Some(at) = s.forage
            && priority > 7
        {
            let speed = self.goal_speed(f64::from(1.2_f32));
            return Some((self.pursue(w, at, speed, true, false, rng), speed));
        }
        None
    }
}

#[cfg(test)]
mod tests;
