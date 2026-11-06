//! Java 26.3 ancient-seed digging. The bounded activity scheduler uses shared
//! ground navigation and animal interaction adapters; see the verification record.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng};
use crate::{
    inventory::Stack,
    item::Item,
    world::{World, block::Block, terrain::Dimension},
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};
pub const IDLE: u8 = 0;
pub const HAPPY: u8 = 1;
pub const SCENTING: u8 = 2;
pub const SNIFFING: u8 = 3;
pub const SEARCHING: u8 = 4;
pub const DIGGING: u8 = 5;
pub const RISING: u8 = 6;
/// Sniffer and sniffer egg sound events (`entity.sniffer.*`, `block.sniffer_egg.*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Scenting,
    Sniffing,
    Searching,
    Digging,
    DiggingStop,
    DropSeed,
    Happy,
    EggCrack,
    EggHatch,
    EggPlop,
}
impl Cue {
    pub const ALL: [Cue; 10] = [
        Cue::Scenting,
        Cue::Sniffing,
        Cue::Searching,
        Cue::Digging,
        Cue::DiggingStop,
        Cue::DropSeed,
        Cue::Happy,
        Cue::EggCrack,
        Cue::EggHatch,
        Cue::EggPlop,
    ];
}
#[derive(Clone, Debug)]
pub struct State {
    pub pose: u8,
    pub elapsed: u16,
    pub duration: u16,
    pub cooldown: u16,
    pub target: Option<IVec3>,
    pub explored: Vec<(u8, IVec3)>,
    pub dropped: bool,
    carry: f64,
    /// The cue `Sniffer.transitionTo` plays for the state just entered.
    entered: Option<Cue>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            pose: IDLE,
            elapsed: 0,
            duration: 100,
            cooldown: 0,
            target: None,
            explored: Vec::new(),
            dropped: false,
            carry: 0.0,
            entered: None,
        }
    }
}
fn pos(v: &Value) -> Option<IVec3> {
    Some(IVec3::new(
        i32::try_from(v[0].as_i64()?).ok()?,
        i32::try_from(v[1].as_i64()?).ok()?,
        i32::try_from(v[2].as_i64()?).ok()?,
    ))
}
impl State {
    pub fn save(&self) -> Value {
        json!({"pose":self.pose,"elapsed":self.elapsed,"duration":self.duration,"cooldown":self.cooldown,
            "target":self.target.map(|p|p.to_array()),"explored":self.explored.iter().map(|(d,p)|json!([d,p.to_array()])).collect::<Vec<_>>(),"dropped":self.dropped})
    }
    pub fn load(&mut self, v: &Value) {
        self.pose = v["pose"].as_u64().unwrap_or(0).min(6) as u8;
        self.elapsed = v["elapsed"].as_u64().unwrap_or(0).min(600) as u16;
        self.duration = v["duration"].as_u64().unwrap_or(100).clamp(1, 600) as u16;
        self.cooldown = v["cooldown"].as_u64().unwrap_or(0).min(9600) as u16;
        self.target = pos(&v["target"]);
        self.dropped = v["dropped"] == true;
        self.explored = v["explored"]
            .as_array()
            .into_iter()
            .flatten()
            .take(21)
            .filter_map(|v| Some((v[0].as_u64()?.min(2) as u8, pos(&v[1])?)))
            .collect();
    }
    fn switch(&mut self, pose: u8, duration: u16) {
        self.entered = match pose {
            SCENTING => Some(Cue::Scenting),
            SNIFFING => Some(Cue::Sniffing),
            RISING => Some(Cue::DiggingStop),
            HAPPY => Some(Cue::Happy),
            _ => None,
        };
        self.pose = pose;
        self.elapsed = 0;
        self.duration = duration;
    }
    pub(super) fn look(&self) -> Value {
        json!({"pose":self.pose,"elapsed":self.elapsed})
    }
    pub(super) fn apply_look(&mut self, v: &Value) {
        self.pose = v["pose"].as_u64().unwrap_or(0).min(6) as u8;
        self.elapsed = v["elapsed"].as_u64().unwrap_or(0).min(600) as u16;
    }
}
/// Expanded `sniffer_diggable_block`: dirt, mud, moss_blocks, grass and podzol.
pub fn diggable(b: Block) -> bool {
    use crate::world::overworld_blocks as o;
    matches!(
        b,
        Block::DIRT
            | Block::GRASS
            | Block::SNOWY_GRASS
            | o::COARSE_DIRT
            | o::ROOTED_DIRT
            | o::PODZOL
            | o::MUD
            | o::MUDDY_MANGROVE_ROOTS
            | o::MOSS_BLOCK
            | o::PALE_MOSS_BLOCK
    )
}
fn dimension(d: Dimension) -> u8 {
    match d {
        Dimension::Overworld => 0,
        Dimension::Nether => 1,
        Dimension::End => 2,
    }
}
fn valid(w: &World, s: &State, p: IVec3, d: u8) -> bool {
    w.get_block(p).is_some_and(diggable)
        && w.get_block(p + IVec3::Y) == Some(Block::AIR)
        && !s.explored.contains(&(d, p))
}
/// Per-tick effects of the current state. `SnifferSoundInstance` loops
/// `SNIFFER_DIGGING` while searching or digging (an original one-second cue
/// every 20 ticks here); `playSearchingSound` adds `SNIFFER_SEARCHING` every
/// 20 ticks; `emitDiggingParticles` sprays the dug block between 1.7 s and
/// 6 s of the dig with its hit sound every 10 ticks. Java spawns 30 terrain
/// particles a tick; two crack particles a tick keep the request queue bounded.
fn effects(w: &mut World, s: &State, pos: DVec3, sounds: &mut Vec<EntityEvent>) {
    let mut cue = |sound| sounds.push(EntityEvent::Sound { sound, pos });
    if matches!(s.pose, SEARCHING | DIGGING) && s.elapsed % 20 == 1 {
        cue(MobSound::Sniffer(Cue::Digging));
    }
    if s.pose == SEARCHING && s.elapsed % 20 == 11 {
        cue(MobSound::Sniffer(Cue::Searching));
    }
    if s.pose != DIGGING || !(35..120).contains(&s.elapsed) {
        return;
    }
    let Some(cell) = s.target else { return };
    let Some(block) = w.get_block(cell).filter(|&b| b != Block::AIR) else { return };
    for _ in 0..2 {
        w.particles.push(crate::particles::Request::Hit { cell, block, face: IVec3::Y });
    }
    if s.elapsed % 10 == 0 {
        cue(MobSound::SnifferDigHit(block));
    }
}
impl Mob {
    pub(super) fn sniffer_steer<W: MobWorld + ?Sized>(
        &mut self,
        _dt: f32,
        w: &W,
        c: &Ctx,
        r: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        let interrupted = self.baby
            || self.in_water
            || self.hurt > 0.0
            || self.riding.is_some()
            || self.ai == super::mob::Ai::Panic
            || self.animal.as_ref().is_some_and(|a| a.love > 0.0)
            || c.players.iter().any(|p| {
                p.alive
                    && !p.spectator
                    && p.held_item == Some(Item::TORCHFLOWER_SEEDS)
                    && p.pos.distance_squared(self.pos) < 100.0
            });
        if interrupted {
            let s = self.sniffer.as_mut().unwrap();
            s.switch(IDLE, 100);
            s.target = None;
            return None;
        }
        let s = self.sniffer.as_ref().unwrap();
        if s.pose == SEARCHING {
            let target = s.target?;
            let speed = self.goal_speed(1.25);
            let wish = self.pursue(w, target.as_dvec3() + DVec3::new(0.5, 1., 0.5), speed, true, false, r);
            return Some((wish, speed));
        }
        if s.pose != IDLE {
            self.stop_navigation();
            return Some((None, 0.0));
        }
        None
    }
}
impl Entities {
    /// Called from the same authoritative update_world transaction as bees and golems.
    pub fn tick_sniffers(&mut self, dt: f64, w: &mut World, ctx: &Ctx) {
        for (p, hatched) in std::mem::take(&mut w.sniffer_egg_events) {
            let pos = p.as_dvec3() + DVec3::splat(0.5);
            let cue = if hatched { Cue::EggHatch } else { Cue::EggCrack };
            self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Sniffer(cue), pos });
            if hatched {
                self.spawn(MobKind::Sniffer, pos);
                let m = self.mobs.last_mut().unwrap();
                m.age = -48000;
                m.baby = true;
            }
        }
        let d = dimension(ctx.dimension);
        for i in 0..self.mobs.len() {
            if self.mobs[i].sniffer.is_none()
                || !self.mobs[i].alive()
                || !w.entity_ticking(self.mobs[i].pos.floor().as_ivec3())
            {
                continue;
            }
            let m = &mut self.mobs[i];
            let mut s = m.sniffer.take().unwrap();
            s.carry += dt.clamp(0.0, 0.25) * 20.;
            let ticks = s.carry.floor() as u16;
            s.carry -= ticks as f64;
            for _ in 0..ticks {
                if let Some(cue) = s.entered.take() {
                    self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Sniffer(cue), pos: m.pos });
                }
                s.cooldown = s.cooldown.saturating_sub(1);
                s.elapsed = s.elapsed.saturating_add(1);
                effects(w, &s, m.pos, &mut self.pending_sounds);
                if m.baby
                    || m.in_water
                    || m.hurt > 0.0
                    || m.riding.is_some()
                    || m.ai == super::mob::Ai::Panic
                    || m.animal.as_ref().is_some_and(|a| a.love > 0.0)
                    || ctx.players.iter().any(|p| {
                        p.alive
                            && !p.spectator
                            && p.held_item == Some(Item::TORCHFLOWER_SEEDS)
                            && p.pos.distance_squared(m.pos) < 100.0
                    })
                {
                    s.switch(IDLE, 100);
                    s.target = None;
                    continue;
                }
                if s.pose == DIGGING && !s.target.is_some_and(|p| valid(w, &s, p, d)) {
                    s.switch(IDLE, 100);
                    s.target = None;
                    continue;
                }
                if s.pose == DIGGING && s.elapsed >= 120 && !s.dropped {
                    s.dropped = true;
                    use crate::world::loot::Random;
                    let rng = w.loot_sequences.get(w.generator.seed, "minecraft:gameplay/sniffer_digging");
                    let item = if rng.bounded(2) == 0 { Item::TORCHFLOWER_SEEDS } else { Item::PITCHER_POD };
                    // Gift loot is independent of doMobLoot (LivingEntity.dropFromGiftLootTable).
                    self.items.push(super::ItemEntity::new(
                        Stack::new(item, 1),
                        s.target.unwrap().as_dvec3() + DVec3::new(0.5, 1.2, 0.5),
                        DVec3::ZERO,
                        0.5,
                        &mut self.rng,
                    ));
                    let sound = MobSound::Sniffer(Cue::DropSeed);
                    self.pending_sounds.push(EntityEvent::Sound { sound, pos: m.pos });
                }
                if s.pose == SEARCHING {
                    if let Some(p) = s.target.filter(|&p| valid(w, &s, p, d)) {
                        let at = p.as_dvec3() + DVec3::new(0.5, 1., 0.5);
                        if m.on_ground && m.pos.distance_squared(at) < 2.25 {
                            s.switch(DIGGING, 160 + self.rng.next_int(21) as u16);
                            s.dropped = false;
                        } else if s.elapsed >= 600 {
                            s.switch(IDLE, 100);
                            s.target = None;
                        }
                    } else {
                        s.switch(IDLE, 100);
                        s.target = None;
                    }
                    continue;
                }
                if s.elapsed < s.duration {
                    continue;
                }
                match s.pose {
                    IDLE if s.cooldown == 0 => {
                        if self.rng.next_int(2) == 0 {
                            s.switch(SCENTING, 40 + self.rng.next_int(41) as u16);
                        } else {
                            s.switch(SNIFFING, 40 + self.rng.next_int(41) as u16);
                        }
                    }
                    SCENTING => s.switch(IDLE, 100),
                    SNIFFING => {
                        // Sniffer.calculateDigPosition: five LandRandomPos attempts,
                        // horizontal radii 10,12,14,16,18 and vertical radius 3.
                        s.target = None;
                        for attempt in 0..5 {
                            let Some(feet) = m.random_pos(w, ctx, &mut self.rng, 10 + 2 * attempt, 3, true) else {
                                continue;
                            };
                            let p = feet - IVec3::Y;
                            if !valid(w, &s, p, d) {
                                continue;
                            }
                            let start = super::path::Start { pos: m.pos, on_ground: m.on_ground, in_water: m.in_water };
                            if super::path::find_path(w, &m.caps(false), start, &[p], 1)
                                .is_some_and(|path| path.reached)
                            {
                                s.target = Some(p);
                                break;
                            }
                        }
                        s.switch(if s.target.is_some() { SEARCHING } else { IDLE }, 600);
                    }
                    DIGGING => {
                        s.cooldown = 9600;
                        s.switch(RISING, 40);
                    }
                    RISING => {
                        if let Some(p) = s.target.take() {
                            s.explored.truncate(20);
                            s.explored.insert(0, (d, p));
                        }
                        s.switch(HAPPY, 40 + self.rng.next_int(61) as u16);
                    }
                    _ => s.switch(IDLE, 100),
                }
            }
            if let Some(cue) = s.entered.take() {
                self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Sniffer(cue), pos: m.pos });
            }
            m.sniffer = Some(s);
        }
    }
}
#[cfg(test)]
mod tests;
