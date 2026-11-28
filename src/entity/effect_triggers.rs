//! v0.15 Track D: the status effects that act on the world when their
//! bearer is hurt or killed (Java 26.3 `WindChargedMobEffect`,
//! `WeavingMobEffect`, `OozingMobEffect` and `InfestedMobEffect`).
//!
//! A death hands the body's effects to an [`EffectDeath`] (Java's
//! `LivingEntity.triggerOnDeathMobEffects` runs `onMobRemoved` for each
//! effect and then clears them), and the authoritative world step
//! ([`Entities::update_world`]) settles it: a wind burst, cobwebs around the
//! body and size-2 slimes. Infested hits roll their silverfish where the hit
//! lands and spawn them in the same step. Every input path shares this:
//! mobs die inside the entity update and players through
//! [`crate::session::PlayerSession::die`].
use super::*;
use crate::simulation::effects::{self, DeathTriggers, Effects};

/// A body killed with death-trigger effects, waiting for the world step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EffectDeath {
    pub triggers: DeathTriggers,
    /// The body's feet, box half-width and height.
    pub pos: DVec3,
    pub half_width: f64,
    pub height: f64,
    /// Players weave cobwebs whatever `mobGriefing` says.
    pub player: bool,
    /// The killed mob, which Oozing's count leaves out (`slime != mob`).
    pub uid: Option<u32>,
}

impl Entities {
    /// `triggerOnDeathMobEffects` for a player killed at `pos` (feet).
    pub fn player_death_effects(&mut self, effects: &Effects, pos: DVec3) {
        let triggers = effects.death_triggers();
        if triggers.any() {
            self.effect_deaths.push(EffectDeath {
                triggers,
                pos,
                half_width: 0.3,
                height: 1.8,
                player: true,
                uid: None,
            });
        }
    }

    /// Mobs that started dying hand over their effects, which Java clears
    /// on death. A body removed outright (an exploding creeper, a trap horse
    /// that expires) is discarded, not killed, and keeps them.
    fn collect_mob_effect_deaths(&mut self) {
        for m in &mut self.mobs {
            if m.dying.is_none_or(|t| t >= mob::DEATH_TIME) || m.effects.is_empty() {
                continue;
            }
            let triggers = m.effects.death_triggers();
            m.effects.clear();
            if triggers.any() {
                let shape = m.shape();
                self.effect_deaths.push(EffectDeath {
                    triggers,
                    pos: m.pos,
                    half_width: shape.half_width,
                    height: shape.height,
                    player: false,
                    uid: Some(m.uid),
                });
            }
        }
    }

    /// The world half of the effect hooks: deaths with Wind Charged,
    /// Weaving or Oozing, and silverfish rolled by Infested hits.
    pub(crate) fn settle_effect_hooks(&mut self, world: &mut crate::world::World, events: &mut Vec<EntityEvent>) {
        self.collect_mob_effect_deaths();
        for d in std::mem::take(&mut self.effect_deaths) {
            if d.triggers.wind_burst {
                events.push(EntityEvent::WindBurst(self.wind_charged_burst(&d)));
            }
            if d.triggers.cobwebs && (d.player || self.villager_griefing) {
                weave_cobwebs(world, d.pos.floor().as_ivec3(), &mut self.rng);
            }
            if d.triggers.slimes {
                self.ooze(&d);
            }
        }
        for i in 0..self.mobs.len() {
            let count = std::mem::take(&mut self.mobs[i].infested);
            if count > 0 {
                let m = &self.mobs[i];
                let look = look_vector(m.pose_values()[0] + m.head_yaw, m.head_pitch);
                let at = m.pos + DVec3::Y * (m.shape().height / 2.0);
                self.spawn_infested_silverfish(at, look, count);
            }
        }
    }

    /// `WindChargedMobEffect.onMobRemoved`: `level.explode` at half height
    /// with strength `3 + nextFloat() * 2` and the wind charge calculator
    /// (`AbstractWindCharge.EXPLOSION_DAMAGE_CALCULATOR`, knockback 1), which
    /// triggers blocks and hurts nothing.
    fn wind_charged_burst(&mut self, d: &EffectDeath) -> wind::WindBurst {
        let strength = effects::WIND_CHARGED_MIN_STRENGTH + self.rng.next_f32() * effects::WIND_CHARGED_STRENGTH_SPREAD;
        let center = d.pos + DVec3::Y * f64::from(d.height as f32 / 2.0);
        wind::WindBurst::new(center, strength, wind::BREEZE_KNOCKBACK, false, None)
    }

    /// `OozingMobEffect.onMobRemoved`: two size-2 slimes half a block above
    /// the feet, fewer when slimes within 2 blocks already reach the
    /// `max_entity_cramming` limit.
    fn ooze(&mut self, d: &EffectDeath) {
        let reach = DVec3::new(d.half_width, 0.0, d.half_width) + DVec3::splat(effects::OOZING_RADIUS);
        let (lo, hi) = (
            d.pos - DVec3::new(reach.x, effects::OOZING_RADIUS, reach.z),
            d.pos + DVec3::new(reach.x, d.height + effects::OOZING_RADIUS, reach.z),
        );
        let mobs = &self.mobs;
        let nearby = |limit: i32| {
            mobs.iter()
                .filter(|m| m.kind == MobKind::Slime && m.alive() && Some(m.uid) != d.uid)
                .filter(|m| {
                    let (a, b) = m.aabb();
                    a.cmplt(hi).all() && b.cmpgt(lo).all()
                })
                .take(limit.max(0) as usize)
                .count() as i32
        };
        let count = effects::oozing_slimes(self.max_entity_cramming, nearby, effects::OOZING_SLIMES as i32);
        for _ in 0..count.max(0) {
            self.spawn(MobKind::Slime, d.pos + DVec3::Y * 0.5);
            let slime = self.mobs.last_mut().unwrap();
            slime.set_size(effects::OOZING_SLIME_SIZE);
            slime.persistent = false;
        }
    }

    /// `InfestedMobEffect.spawnSilverfish`, `count` times: at `at`, pushed
    /// along a third of the look direction (half again upward) turned by a
    /// random angle within a quarter turn either way.
    pub fn spawn_infested_silverfish(&mut self, at: DVec3, look: DVec3, count: u8) {
        for _ in 0..count {
            let angle = self.rng.next_f32() * std::f32::consts::PI - std::f32::consts::FRAC_PI_2;
            let v = DVec3::new(look.x * 0.3, look.y * 0.3 * 1.5, look.z * 0.3);
            // JOML `rotateY`: x' = x cos + z sin, z' = -x sin + z cos.
            let (sin, cos) = (f64::from(angle.sin()), f64::from(angle.cos()));
            let v = DVec3::new(v.x * cos + v.z * sin, v.y, -v.x * sin + v.z * cos);
            self.spawn(MobKind::Silverfish, at);
            let fish = self.mobs.last_mut().unwrap();
            fish.vel = v * crate::knockback::TICK_RATE;
            fish.persistent = false;
            self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Hurt(MobKind::Silverfish), pos: at });
        }
    }

    /// Whether a living mob's box overlaps the block at `p`
    /// (`Level.isUnobstructed`, for Frost Walker's ice).
    pub fn cell_occupied(&self, p: IVec3) -> bool {
        let (lo, hi) = (p.as_dvec3(), p.as_dvec3() + DVec3::ONE);
        self.mobs.iter().any(|m| {
            let (a, b) = m.aabb();
            m.alive() && a.cmplt(hi).all() && b.cmpgt(lo).all()
        })
    }

    /// Rolls a player's Infested hits ([`crate::simulation::survival::Vitals::infested_hits`]).
    pub fn player_infested_hits(&mut self, hits: u8, at: DVec3, look: DVec3) {
        for _ in 0..hits {
            if self.rng.next_f32() <= effects::INFESTED_CHANCE {
                let (lo, hi) = effects::INFESTED_SILVERFISH;
                let count = (lo + self.rng.next_int(hi - lo + 1)) as u8;
                self.spawn_infested_silverfish(at, look, count);
            }
        }
    }
}

/// The look vector of a body facing `yaw` (this game's convention, forward
/// = (cos, 0, sin)) and `pitch` (positive looks up).
fn look_vector(yaw: f32, pitch: f32) -> DVec3 {
    let (yaw, pitch) = (f64::from(yaw), f64::from(pitch));
    DVec3::new(yaw.cos() * pitch.cos(), pitch.sin(), yaw.sin() * pitch.cos())
}

/// `WeavingMobEffect.spawnCobwebsRandomlyAround`: up to 2 or 3 cobwebs at
/// 15 random cells of the 3x3x3 cube around `center`, each replaceable and
/// above a block whose top is sturdy.
pub fn weave_cobwebs(world: &mut crate::world::World, center: IVec3, rng: &mut Rng) -> usize {
    let (lo, hi) = effects::WEAVING_COBWEBS;
    let wanted = (lo + rng.next_int(hi - lo + 1)) as usize;
    let r = effects::WEAVING_RADIUS;
    let side = (2 * r + 1) as u32;
    let mut chosen: Vec<IVec3> = Vec::with_capacity(wanted);
    for _ in 0..effects::WEAVING_SEARCHES {
        let p = center - IVec3::splat(r)
            + IVec3::new(rng.next_int(side) as i32, rng.next_int(side) as i32, rng.next_int(side) as i32);
        if chosen.contains(&p) {
            continue;
        }
        let here = world.get_block(p);
        let below = world.get_block(p - IVec3::Y);
        if here.is_some_and(|b| b.is_replaceable())
            && below.is_some_and(|b| crate::world::mount_states::face_sturdy(b, IVec3::Y))
        {
            chosen.push(p);
            if chosen.len() >= wanted {
                break;
            }
        }
    }
    for &p in &chosen {
        world.set_block(p, crate::world::block::Block::COBWEB);
        world.particles.push(crate::particles::Request::Hit {
            cell: p,
            block: crate::world::block::Block::COBWEB,
            face: IVec3::Y,
        });
    }
    chosen.len()
}

#[cfg(test)]
mod tests;
