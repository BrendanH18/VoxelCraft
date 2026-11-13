//! The endermite (Java 26.3 `Endermite`): a small monster a landed ender
//! pearl sometimes leaves behind, gone after two minutes unless something
//! makes it persistent, and hunted by endermen. Its goals run in
//! `mob::endermite`; this module holds its numbers, its saved state and the
//! entity-level passes (lifetime, pearl spawns, mob targets, endermen).
//!
//! Every number is checked against `tests/fixtures/java/26.3/small_mobs.json`.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld};
use crate::physics::Shape;
use glam::DVec3;
use serde_json::Value;

/// `Endermite.createAttributes`: max health 8, movement speed 0.25 and
/// attack damage 2; the follow range is `Mob.createMobAttributes`' 16.
pub const MAX_HEALTH: f32 = 8.0;
pub const MOVEMENT_SPEED: f64 = 0.25;
pub const ATTACK_DAMAGE: f32 = 2.0;
pub const FOLLOW_RANGE: f64 = 16.0;
/// `EntityTypes.ENDERMITE.eyeHeight(0.13F)`.
pub const EYE_HEIGHT: f64 = 0.13;
/// `xpReward` set in the constructor.
pub const EXPERIENCE: u32 = 3;
/// `Endermite.MAX_LIFE`: discarded once `life` reaches it.
pub const MAX_LIFE: i32 = 2400;
/// `ThrownEnderpearl.onHit`: the chance a landing leaves an endermite.
pub const PEARL_CHANCE: f32 = 0.05;
/// `MeleeAttackGoal.attackInterval` (`adjustedTickDelay(20)`, updated every tick).
pub const ATTACK_INTERVAL: u32 = 20;
/// Portal particles `aiStep` adds each tick.
pub const PORTAL_PARTICLES: u16 = 2;
/// `Enderman.createAttributes` follow range and attack damage, which bound
/// and power its `NearestAttackableTargetGoal<Endermite>` (priority 3).
pub const ENDERMAN_FOLLOW_RANGE: f64 = 64.0;
pub const ENDERMAN_DAMAGE: f32 = 7.0;
/// Enderman eye height (`EntityTypes.ENDERMAN.eyeHeight(2.55F)`).
const ENDERMAN_EYE: f64 = 2.55;
/// `Mob.DEFAULT_ATTACK_REACH`: `sqrt(2.04) - 0.6`, the horizontal growth of
/// the attack box (`Mob.getAttackBoundingBox`).
pub const ATTACK_REACH: f64 = 0.828_427_124_746_190_1;
/// An enderman's horizontal reach to an endermite's centre: the two half
/// widths (0.3 and 0.2) and [`ATTACK_REACH`].
pub const ENDERMAN_REACH: f64 = 0.3 + 0.2 + ATTACK_REACH;
/// Seconds per game tick.
const TICK: f64 = 0.05;

/// An endermite's saved lifetime and its goal state.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// `Endermite.life`: ticks lived, saved as `lifetime` (Java's `Lifetime`).
    pub life: i32,
    /// The mob its `HurtByTargetGoal` turned on (players use the shared
    /// player target in `Mob::move_ctl`).
    pub(super) foe: Option<u32>,
    /// Where `foe` stands this tick, filled in by [`Entities::endermite_upkeep`].
    pub(super) foe_at: Option<(DVec3, Shape)>,
    /// `MeleeAttackGoal.ticksUntilNextAttack`.
    pub(super) attack_wait: u32,
    /// Seconds not yet counted as a tick of `life`.
    carry: f64,
}

impl State {
    /// The fields an endermite saves beside the shared mob record.
    pub fn save_into(&self, v: &mut Value) {
        v["lifetime"] = self.life.into();
    }

    /// Reads `lifetime` (0 when absent, as `getIntOr("Lifetime", 0)`).
    pub fn load(&mut self, v: &Value) {
        self.life = v["lifetime"].as_i64().map_or(0, |l| l.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
    }
}

/// `Mob.isWithinMeleeAttackRange`: the attacker's box grown by
/// [`ATTACK_REACH`] horizontally overlaps the target's box.
pub(super) fn within_melee(pos: DVec3, shape: Shape, other: DVec3, other_shape: Shape) -> bool {
    (pos.x - other.x).abs() < shape.half_width + other_shape.half_width + ATTACK_REACH
        && (pos.z - other.z).abs() < shape.half_width + other_shape.half_width + ATTACK_REACH
        && pos.y < other.y + other_shape.height
        && pos.y + shape.height > other.y
}

/// The body yaw (`(cos, 0, sin)` forward) of a view direction, or 0 for an
/// unknown one.
fn yaw_of(look: DVec3) -> f32 {
    if look.x == 0.0 && look.z == 0.0 { 0.0 } else { look.z.atan2(look.x) as f32 }
}

impl Entities {
    /// Each landed ender pearl of this update: `ThrownEnderpearl.onHit`
    /// rolls 5% for a living player thrower (before any other check, as Java
    /// does) and, when monsters may spawn (`spawn_mobs` and
    /// `spawn_monsters`) and the difficulty isn't Peaceful, puts an
    /// endermite where the thrower stood before teleporting, facing as they
    /// did.
    pub(super) fn pearl_endermites(&mut self, events: &[EntityEvent], ctx: &Ctx) {
        for e in events {
            let EntityEvent::PearlLanded { owner, .. } = *e else { continue };
            let Some(thrower) = ctx.players.iter().find(|t| t.id == owner && t.alive && !t.spectator) else {
                continue;
            };
            if self.rng.next_f32() < PEARL_CHANCE
                && ctx.spawning
                && self.monster_spawning
                && self.difficulty != crate::simulation::difficulty::Difficulty::Peaceful
            {
                self.mobs.push(Mob::new(MobKind::Endermite, thrower.pos, yaw_of(thrower.look)));
            }
        }
    }

    /// Per tick, for every loaded endermite: `aiStep` counts its life unless
    /// it is persistent and discards it at [`MAX_LIFE`]; its mob target is
    /// dropped when that mob died or left the follow range. Endermen that
    /// aren't after a player then pick the nearest endermite they can see.
    pub(super) fn endermite_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W) {
        if !self.mobs.iter().any(|m| m.kind == MobKind::Endermite) {
            return;
        }
        self.ensure_mob_ids();
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &mut self.mobs[i];
            let (active, persistent) = (m.simulation_active && m.alive(), m.persistent);
            let Some(s) = m.endermite.as_mut().filter(|_| active) else {
                i += 1;
                continue;
            };
            s.carry += dt.max(0.0);
            let ticks = ((s.carry + 1e-9) / TICK).floor();
            s.carry -= ticks * TICK;
            if !persistent {
                s.life = s.life.saturating_add(ticks as i32);
            }
            if s.life >= MAX_LIFE {
                // `discard`: no death, drops or experience.
                self.mobs.swap_remove(i);
                continue;
            }
            i += 1;
        }
        for i in 0..self.mobs.len() {
            let Some(foe) = self.mobs[i].endermite.as_ref().and_then(|s| s.foe) else { continue };
            let pos = self.mobs[i].pos;
            let at = self
                .mobs
                .iter()
                .find(|o| o.uid == foe && o.alive() && o.simulation_active)
                .filter(|o| o.pos.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE)
                .map(|o| (o.pos, o.shape()));
            let s = self.mobs[i].endermite.as_mut().unwrap();
            s.foe_at = at;
            if at.is_none() {
                s.foe = None;
            }
        }
        self.endermen_hunt(world);
    }

    /// `Enderman`'s `NearestAttackableTargetGoal<Endermite>(mustSee)`: an
    /// enderman with no player to be angry at walks at the nearest endermite
    /// within its 64-block follow range that it can see.
    fn endermen_hunt<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let mites: Vec<DVec3> = self
            .mobs
            .iter()
            .filter(|m| m.kind == MobKind::Endermite && m.alive() && m.simulation_active)
            .map(|m| m.pos)
            .collect();
        for m in &mut self.mobs {
            if m.kind != MobKind::Enderman
                || !m.alive()
                || !m.simulation_active
                || m.is_provoked()
                || m.teleport_pending
                || m.riding.is_some()
            {
                continue;
            }
            let eye = m.pos + DVec3::Y * ENDERMAN_EYE;
            m.hunt = mites
                .iter()
                .filter(|p| p.distance_squared(m.pos) <= ENDERMAN_FOLLOW_RANGE * ENDERMAN_FOLLOW_RANGE)
                .filter(|&&p| super::mob::line_of_sight(world, eye, p + DVec3::Y * EYE_HEIGHT))
                .min_by(|a, b| a.distance_squared(m.pos).total_cmp(&b.distance_squared(m.pos)))
                .copied();
        }
    }

    /// The swing an enderman's hunt asked for lands on an endermite within
    /// its attack box, for its 7 attack damage.
    pub(super) fn enderman_strike(&mut self, index: usize, events: &mut Vec<EntityEvent>) {
        let (pos, shape, attacker) = (self.mobs[index].pos, self.mobs[index].shape(), self.mobs[index].uid);
        let victim = self
            .mobs
            .iter()
            .filter(|m| m.kind == MobKind::Endermite && m.alive() && within_melee(pos, shape, m.pos, m.shape()))
            .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)));
        let Some(victim) = victim else { return };
        let dir = (victim.pos - pos).with_y(0.0).normalize_or_zero();
        events.push(EntityEvent::MobHit {
            target: victim.uid,
            attacker,
            damage: ENDERMAN_DAMAGE,
            knockback: dir * 4.0 + DVec3::Y * 2.0,
        });
    }
}

#[cfg(test)]
mod tests;
