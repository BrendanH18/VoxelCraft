//! Frog lifecycle and cross-entity tongue attacks, from the pinned Java 26.3
//! Frog, Tadpole, ShootTongue and the frog variant/food/loot data.
use super::{Ctx, Entities, Mob, MobKind, MobWorld};
use crate::{
    inventory::Stack,
    item::Item,
    world::{
        World,
        block::Block,
        frog_blocks,
        terrain::{Biome, Dimension},
    },
};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

pub const GROW_TICKS: u32 = 24000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrogSound {
    Tongue,
    Eat,
    Jump,
    Land,
    Lay,
    Hatch,
    Grow,
}
impl FrogSound {
    pub const ALL: [Self; 7] = [Self::Tongue, Self::Eat, Self::Jump, Self::Land, Self::Lay, Self::Hatch, Self::Grow];
}
#[derive(Clone, Debug)]
pub struct State {
    /// 0 temperate, 1 cold, 2 warm. Saved independently of enum ordering.
    pub variant: u8,
    pub age: u32,
    pub age_locked: bool,
    pub jump_cooldown: u16,
    pub pose: u8,
    pub tongue: Option<(u32, DVec3)>,
    pub(super) tongue_ticks: u16,
    pub(super) sense_ticks: u16,
    pub(super) hunt_ticks: u16,
    pub(super) fall_distance: f64,
    pub(super) sounds: u8,
    pub(super) initialized: bool,
    pub(super) carry: f64,
    pub(super) shore: Option<IVec3>,
    pub(super) shore_check: u16,
    pub(super) croak: u16,
    pub(super) jump_prepare: u16,
    pub(super) jump_velocity: Option<DVec3>,
    pub(super) laid: Option<IVec3>,
    pub(super) unreachable: [(u32, u16); 5],
}
impl Default for State {
    fn default() -> Self {
        Self {
            variant: 0,
            age: 0,
            age_locked: false,
            jump_cooldown: 100,
            pose: 0,
            tongue: None,
            tongue_ticks: 0,
            sense_ticks: 0,
            hunt_ticks: 0,
            fall_distance: 0.0,
            sounds: 0,
            initialized: false,
            carry: 0.0,
            shore: None,
            shore_check: 0,
            croak: 0,
            jump_prepare: 0,
            jump_velocity: None,
            laid: None,
            unreachable: [(0, 0); 5],
        }
    }
}
impl State {
    pub fn save(&self) -> Value {
        json!({"variant":self.variant,"age":self.age,"age_locked":self.age_locked,"jump_cooldown":self.jump_cooldown})
    }
    pub fn load(&mut self, v: &Value) {
        self.variant = v["variant"].as_u64().unwrap_or(0).min(2) as u8;
        self.age = v["age"].as_u64().unwrap_or(0).min(GROW_TICKS as u64) as u32;
        self.age_locked = v["age_locked"] == true;
        self.jump_cooldown = v["jump_cooldown"].as_u64().unwrap_or(100).min(140) as u16;
        self.initialized = true;
    }
    /// Tadpole bucket bits: present, 15 age bits, 10 health bits, age lock.
    /// This item's payload is independent of the older fish bucket encoding.
    pub fn bucket(&self, health: f32) -> u32 {
        1 | (self.age.min(GROW_TICKS) << 1)
            | (((health.clamp(0.01, 6.0) * 100.0).round() as u32) << 16)
            | (u32::from(self.age_locked) << 26)
    }
    pub fn from_bucket(&mut self, data: u32) -> Option<f32> {
        if data & 1 == 0 {
            return None;
        }
        self.age = ((data >> 1) & 32767).min(GROW_TICKS);
        self.age_locked = data & (1 << 26) != 0;
        Some((((data >> 16) & 1023) as f32 / 100.0).clamp(0.01, 6.0))
    }
}
/// 26.3 uses biome tag selectors, rather than a numeric temperature cutoff.
pub fn variant(biome: Biome, dimension: Dimension) -> u8 {
    use Biome::*;
    if dimension == Dimension::End {
        return 1;
    }
    if dimension == Dimension::Nether {
        return 2;
    }
    match biome {
        SnowyPlains | IceSpikes | FrozenPeaks | JaggedPeaks | SnowySlopes | FrozenOcean | DeepFrozenOcean | Grove
        | DeepDark | FrozenRiver | SnowyTaiga | SnowyBeach => 1,
        Desert | WarmOcean | Jungle | SparseJungle | BambooJungle | Savanna | SavannaPlateau | WindsweptSavanna
        | Badlands | WoodedBadlands | ErodedBadlands | MangroveSwamp => 2,
        _ => 0,
    }
}
fn at_variant<W: MobWorld + ?Sized>(world: &W, pos: DVec3, dimension: Dimension) -> u8 {
    variant(world.cave_biome(pos.floor().as_ivec3()), dimension)
}
pub fn froglight(variant: u8) -> Block {
    match variant {
        1 => frog_blocks::VERDANT,
        2 => frog_blocks::PEARLESCENT,
        _ => frog_blocks::OCHRE,
    }
}
pub fn prey(m: &Mob) -> bool {
    m.alive() && m.size == 1 && m.kind.is_cube()
}

impl Entities {
    pub(crate) fn grow_tadpole<W: MobWorld + ?Sized>(&mut self, index: usize, world: &W, dimension: Dimension) {
        let old = &self.mobs[index];
        let mut frog = Mob::new(MobKind::Frog, old.pos, old.yaw);
        frog.uid = old.uid;
        frog.vel = old.vel;
        frog.name = old.name;
        frog.persistent = true;
        frog.leash = old.leash;
        frog.effects = old.effects.clone();
        // ConversionType/fudgePositionAfterSizeChange: the larger body may
        // need a small displacement to fit under a slab or low ceiling.
        if crate::physics::overlaps_solid(world, frog.pos, frog.shape()) {
            let mut best = None;
            for y in -4..=4 {
                for z in -2..=2 {
                    for x in -2..=2 {
                        let offset = DVec3::new(f64::from(x) * 0.05, f64::from(y) * 0.05, f64::from(z) * 0.05);
                        let pos = old.pos + offset;
                        if !crate::physics::overlaps_solid(world, pos, frog.shape())
                            && best.is_none_or(|(_, d)| offset.length_squared() < d)
                        {
                            best = Some((pos, offset.length_squared()));
                        }
                    }
                }
            }
            if let Some((pos, _)) = best {
                frog.pos = pos;
                frog.previous_pos = pos;
            }
        }

        let state = frog.frog.as_mut().unwrap();
        state.variant = at_variant(world, old.pos, dimension);
        state.initialized = true;
        state.jump_cooldown = 100 + self.rng.next_int(41) as u16;
        state.sounds |= 1 << FrogSound::Grow as u8;
        self.mobs[index] = frog;
    }
    pub fn feed_tadpole(&mut self, index: usize, item: Item, world: &World) -> bool {
        let Some(m) = self.mobs.get_mut(index).filter(|m| m.alive() && m.kind == MobKind::Tadpole) else {
            return false;
        };
        if item != Item::SLIME_BALL || m.frog.as_ref().unwrap().age_locked {
            return false;
        }
        let s = m.frog.as_mut().unwrap();
        if !s.age_locked {
            s.age += (GROW_TICKS.saturating_sub(s.age) / 20 / 10) * 20;
        }
        if s.age >= GROW_TICKS {
            self.grow_tadpole(index, world, world.generator.dimension);
        }
        true
    }
    pub(super) fn frog_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) {
        if !self.mobs.iter().any(|m| m.frog.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        let mut eats = Vec::new();
        for i in 0..self.mobs.len() {
            if !self.mobs[i].alive() || !world.entity_ticking(self.mobs[i].pos.floor().as_ivec3()) {
                continue;
            }
            let m = &mut self.mobs[i];
            let Some(s) = &mut m.frog else {
                continue;
            };
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / 0.05).floor() as u16;
            s.carry -= f64::from(ticks) * 0.05;
            s.jump_cooldown = s.jump_cooldown.saturating_sub(ticks);
            s.shore_check = s.shore_check.saturating_sub(ticks);
            s.croak = s.croak.saturating_sub(ticks);
            for (_, left) in &mut s.unreachable {
                *left = left.saturating_sub(ticks);
            }
            if m.kind == MobKind::Tadpole {
                if !s.age_locked {
                    s.age = s.age.saturating_add(u32::from(ticks));
                }
                if s.age >= GROW_TICKS {
                    self.grow_tadpole(i, world, ctx.dimension);
                }
                continue;
            }
            if !s.initialized {
                s.variant = at_variant(world, m.pos, ctx.dimension);
                s.jump_cooldown = 100 + self.rng.next_int(41) as u16;
                s.initialized = true;
            }
            if s.pose == 0 && s.tongue.is_some() {
                s.hunt_ticks = s.hunt_ticks.saturating_add(ticks);
                if s.hunt_ticks >= 100 {
                    if let Some((uid, _)) = s.tongue.take() {
                        s.unreachable.rotate_left(1);
                        s.unreachable[4] = (uid, 100);
                    }
                    s.sense_ticks = 0;
                    s.hunt_ticks = 0;
                }
            }
            if s.pose == 1 {
                s.tongue_ticks = s.tongue_ticks.saturating_add(ticks);
                if s.tongue_ticks >= 7 {
                    if let Some((uid, _)) = s.tongue {
                        eats.push((i, uid, s.variant));
                    }
                    s.pose = 2;
                }
            } else if s.pose == 2 {
                s.tongue_ticks = s.tongue_ticks.saturating_add(ticks);
                if s.tongue_ticks >= 11 {
                    s.pose = 0;
                    s.tongue = None;
                    s.sense_ticks = 0;
                }
            }
            s.sense_ticks = s.sense_ticks.saturating_sub(ticks);
        }
        let mut eaten = Vec::new();
        for (i, uid, color) in eats {
            let Some(j) = self.mobs.iter().position(|m| m.uid == uid && prey(m)) else {
                continue;
            };
            let pos = self.mobs[j].pos;
            self.mobs[i].frog.as_mut().unwrap().sounds |= 1 << FrogSound::Eat as u8;
            let attacker = self.mobs[i].pos;
            self.mobs[j].damage(10.0, Some(attacker), &mut self.rng);
            if self.mobs[j].alive() {
                continue;
            }
            if self.mob_loot {
                let stack = if self.mobs[j].kind == MobKind::MagmaCube {
                    Stack::new(froglight(color), 1)
                } else {
                    Stack::new(Item::SLIME_BALL, 1)
                };
                if stack.count > 0 {
                    self.scatter(stack, pos);
                }
                if self.mobs[j].player_hit_left > 0.0 {
                    self.spawn_xp(pos, 1);
                }
            }
            // Java removes a killed tongue target immediately, without a corpse.
            eaten.push(uid);
        }
        if !eaten.is_empty() {
            self.mobs.retain(|m| !eaten.contains(&m.uid));
            self.mob_index.rebuild(&self.mobs);
        }
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if m.kind != MobKind::Frog || !m.alive() || !m.simulation_active || !m.frog_can_hunt() {
                continue;
            }
            let s = m.frog.as_ref().unwrap();
            if s.pose != 0 && s.pose != 1 {
                continue;
            }
            let chosen = if let Some((uid, _)) = s.tongue {
                self.mobs.iter().position(|m| m.uid == uid && prey(m))
            } else if s.sense_ticks == 0 {
                self.mob_index.nearest(&self.mobs, m.pos, 10.0, |p| {
                    prey(p)
                        && !s.unreachable.iter().any(|&(id, left)| id == p.uid && left > 0)
                        && super::mob::line_of_sight(
                            world,
                            m.pos + DVec3::Y * 0.375,
                            p.pos + DVec3::Y * p.shape().height * 0.5,
                        )
                })
            } else {
                continue;
            };
            let target = chosen.map(|j| (self.mobs[j].uid, self.mobs[j].pos));
            let pos = m.pos;
            let s = self.mobs[i].frog.as_mut().unwrap();
            s.sense_ticks = 20;
            if s.tongue.map(|(id, _)| id) != target.map(|(id, _)| id) {
                s.hunt_ticks = 0;
            }
            s.tongue = target;
            if s.pose == 0
                && let Some(j) = chosen
                && pos.distance(self.mobs[j].pos) < 1.75
            {
                let to = pos - self.mobs[j].pos;
                self.mobs[j].vel = to.normalize_or_zero() * 15.0;
                let s = self.mobs[i].frog.as_mut().unwrap();
                s.pose = 1;
                s.tongue_ticks = 0;
                s.sounds |= 1 << FrogSound::Tongue as u8;
            }
        }
    }
}
pub fn finish_hatches(world: &mut World, entities: &mut Entities) {
    for (cell, count) in std::mem::take(&mut world.hatched_tadpoles) {
        entities.pending_sounds.push(super::EntityEvent::Sound {
            sound: super::MobSound::FrogAction(FrogSound::Hatch),
            pos: cell.as_dvec3() + DVec3::splat(0.5),
        });
        for _ in 0..count {
            let mut offset = || f64::from(entities.rng.next_f32()).clamp(0.20000000298023224, 0.7999999970197678);
            let pos = cell.as_dvec3() + DVec3::new(offset(), -0.5, offset());
            entities.spawn(MobKind::Tadpole, pos);
            entities.mobs.last_mut().unwrap().persistent = true;
        }
    }
}

#[cfg(test)]
mod tests;
