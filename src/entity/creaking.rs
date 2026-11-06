//! Heart binding, lifecycle and resin production for Java 26.3 creakings.
use super::{Ctx, Entities, MobKind, Rng};
use crate::{
    simulation::difficulty::Difficulty,
    world::{
        World,
        block::Block,
        cave_blocks::{self as faces, Multiface},
        creaking as heart,
    },
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};
#[derive(Clone, Debug)]
pub struct State {
    pub home: Option<IVec3>,
    pub active: bool,
    pub can_move: bool,
    pub hit: bool,
    pub hit_cooldown: f32,
    pub teardown: f32,
}
impl Default for State {
    fn default() -> Self {
        Self { home: None, active: false, can_move: true, hit: false, hit_cooldown: 0., teardown: 0. }
    }
}
impl State {
    pub fn save(&self) -> Value {
        json!({"home":self.home.map(|p|p.to_array()),"active":self.active,"move":self.can_move,"teardown":self.teardown,"twitch":self.hit_cooldown})
    }
    pub fn load(&mut self, v: &Value) {
        self.home = v["home"].as_array().filter(|a| a.len() == 3).and_then(|a| {
            Some(IVec3::new(
                i32::try_from(a[0].as_i64()?).ok()?,
                i32::try_from(a[1].as_i64()?).ok()?,
                i32::try_from(a[2].as_i64()?).ok()?,
            ))
        });
        self.active = v["active"].as_bool().unwrap_or(false);
        self.hit_cooldown = v["twitch"].as_f64().unwrap_or(0.).clamp(0., 0.4) as f32;
        self.can_move = v["move"].as_bool().unwrap_or(true);
        self.teardown = v["teardown"].as_f64().unwrap_or(0.).clamp(0., 2.3) as f32;
    }
}
fn shuffled(rng: &mut Rng) -> [usize; 6] {
    let mut a = [0, 1, 2, 3, 4, 5];
    for i in (1..6).rev() {
        a.swap(i, rng.next_int(i as u32 + 1) as usize);
    }
    a
}
fn spread_resin(w: &mut World, p: IVec3, rng: &mut Rng) -> bool {
    let mut queue = std::collections::VecDeque::from([(p, 0)]);
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some((p, depth)) = queue.pop_front() {
        if !seen.insert(p) || seen.len() > 64 {
            continue;
        }
        if w.get_block(p).is_some_and(heart::pale_log) {
            for i in shuffled(rng) {
                let q = p + faces::DIRS[i];
                let Some(b) = w.get_block(q) else { continue };
                let (f, wet) = match faces::multiface_of(b) {
                    Some((Multiface::ResinClump, f, wet)) => (f, wet),
                    _ if b == Block::AIR || b == Block::WATER => (0, b == Block::WATER),
                    _ => continue,
                };
                let bit = 1 << (i ^ 1);
                if f & bit == 0 {
                    w.set_block(q, faces::multiface(Multiface::ResinClump, f | bit, wet));
                    return true;
                }
            }
        }
        if depth < 2 {
            for i in shuffled(rng) {
                let q = p + faces::DIRS[i];
                if w.get_block(q).is_some_and(heart::pale_log) {
                    queue.push_back((q, depth + 1));
                }
            }
        }
    }
    false
}
impl Entities {
    pub fn tick_creaking_hearts(&mut self, dt: f64, w: &mut World, ctx: &Ctx, difficulty: Difficulty) {
        if w.creaking_hearts.is_empty() && !self.mobs.iter().any(|m| m.creaking.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        let keys: Vec<_> =
            w.creaking_active.iter().copied().filter(|p| w.is_entity_ticking(IVec3::from_array(*p))).collect();
        let night = w.creaking_night.unwrap_or(ctx.daylight < 0.35);
        for key in keys {
            let p = IVec3::from_array(key);
            let Some(b) = w.get_block(p) else { continue };
            let Some((axis, _, natural)) = heart::heart(b) else { continue };
            let mut h = w.creaking_hearts[&key].clone();
            h.emitter = (h.emitter - dt).max(0.);
            h.grace = (h.grace - dt).max(0.);
            h.timer -= dt;
            let protector = h.protector.and_then(|id| {
                self.mobs
                    .iter()
                    .position(|m| m.uid == id && m.alive() && m.creaking.as_ref().is_some_and(|s| s.home == Some(p)))
            });
            // A protector in an unloaded chunk is waiting in storage, not gone.
            let stored = || h.protector.is_some_and(|id| self.storage.mobs().any(|m| m.uid == id));
            if h.protector.is_some() && protector.is_none() && h.grace <= 1e-9 && !stored() {
                h.protector = None;
            }
            let activity = if !heart::required_logs(b, p, |q| w.get_block(q)) && h.protector.is_none() {
                heart::Activity::Uprooted
            } else if night {
                heart::Activity::Awake
            } else {
                heart::Activity::Dormant
            };
            let next = heart::block(axis, activity, natural);
            if b != next {
                w.set_block(p, next);
            }
            if let Some(i) = protector {
                let m = &mut self.mobs[i];
                let s = m.creaking.as_mut().unwrap();
                if (!night && !m.persistent) || m.pos.distance(p.as_dvec3() + DVec3::new(0.5, 0., 0.5)) > 34. {
                    s.teardown = s.teardown.max(0.001);
                    s.can_move = false;
                }
                if std::mem::take(&mut s.hit) && h.emitter <= 1e-9 {
                    if activity == heart::Activity::Awake {
                        for _ in 0..2 + self.rng.next_int(2) {
                            spread_resin(w, p, &mut self.rng);
                        }
                    }
                    h.emitter = 5.;
                }
                if h.emitter > 2.5 {
                    let from = m.pos + DVec3::Y * 1.35;
                    let to = p.as_dvec3() + DVec3::splat(0.5);
                    for j in 0..3 {
                        let pos = from.lerp(to, (j as f64 + f64::from(self.rng.next_f32())) / 3.);
                        let mut particle =
                            crate::particles::Particle::new(pos, crate::particles::Texture::Block(heart::RESIN_BLOCK));
                        particle.size = 0.035;
                        particle.velocity = (to - pos) * 1.5;
                        particle.physics = false;
                        particle.lifetime = 10;
                        w.particles.push(crate::particles::Request::Particle(particle));
                    }
                }
                h.signal = 15
                    - (m.pos.distance(p.as_dvec3() + DVec3::new(0.5, 0., 0.5)).clamp(0., 32.) / 32. * 15.).floor()
                        as u8;
            } else {
                h.signal = 0;
                if h.timer <= 1e-9 {
                    h.timer = f64::from(20 + self.rng.next_int(5)) * 0.05;
                    if h.protector.is_none()
                        && activity == heart::Activity::Awake
                        && difficulty != Difficulty::Peaceful
                        && ctx.spawning
                        && ctx.nearest_player_dist2(p.as_dvec3()).is_some_and(|d| d < 1024.)
                    {
                        for _ in 0..5 {
                            let x = p.x + self.rng.next_int(33) as i32 - 16;
                            let z = p.z + self.rng.next_int(33) as i32 - 16;
                            let mut found = None;
                            for y in (p.y - 8..=p.y + 8).rev() {
                                let at = IVec3::new(x, y, z);
                                if !w.is_entity_ticking(at) {
                                    continue;
                                }
                                if w.get_block(at - IVec3::Y).is_some_and(|b| b.is_solid() && !b.is_leaves())
                                    && (0..3).all(|i| w.get_block(at + IVec3::Y * i) == Some(Block::AIR))
                                {
                                    found = Some(at);
                                    break;
                                }
                            }
                            let Some(at) = found else { continue };
                            self.spawn(MobKind::Creaking, at.as_dvec3() + DVec3::new(0.5, 0., 0.5));
                            self.ensure_mob_ids();
                            let m = self.mobs.last_mut().unwrap();
                            m.creaking.as_mut().unwrap().home = Some(p);
                            h.protector = Some(m.uid);
                            break;
                        }
                    }
                }
            }
            let old = w.creaking_hearts.get(&key).map_or(0, |h| h.signal);
            w.creaking_hearts.insert(key, h);
            if old != w.creaking_hearts[&key].signal {
                w.redstone_changed(p);
            }
        }
        for m in &mut self.mobs {
            let Some(s) = m.creaking.as_mut() else { continue };
            let Some(p) = s.home else { continue };
            if !m.simulation_active {
                continue;
            }
            if w.get_block(p).is_some()
                && (w.get_block(p).is_some_and(|b| heart::heart(b).is_none())
                    || w.creaking_hearts.get(&p.to_array()).is_none_or(|h| h.protector != Some(m.uid)))
            {
                s.teardown = s.teardown.max(0.001);
                s.can_move = false;
            }
            if s.teardown > 0. {
                s.teardown += dt as f32;
                if s.teardown > 2.25 {
                    m.health = 0.;
                    m.dying = Some(1.);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
