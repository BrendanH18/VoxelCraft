//! v0.16 Track D weapon effects on mobs: spear stabs (`Player.stabAttack`
//! from the jab and the charged attack), the mace smash's area knockback
//! (`MaceItem.knockback`) and Wind Burst's push on nearby mobs.

use glam::DVec3;

use super::{Entities, MobKind, PlayerId};
use crate::inventory::Stack;
use crate::knockback::{Push, TICK_RATE};

/// One `Player.stabAttack` on a mob.
#[derive(Clone, Copy, Debug)]
pub struct Stab {
    pub held: Stack,
    /// Damage before the enchantment bonus, when the stab deals damage.
    pub damage: Option<f32>,
    pub knockback: bool,
    pub dismount: bool,
    /// The attacker's feet and Java yaw in degrees.
    pub origin: DVec3,
    pub yaw: f32,
    /// Scales the enchantment bonus: the attack charge for a jab, 1 while
    /// charging (the item is in use in that hand).
    pub strength: f32,
    pub owner: PlayerId,
}

/// What a stab did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stabbed {
    /// Something happened (`stabAttack` returned true).
    pub affected: bool,
    pub hurt: bool,
    pub killed: Option<MobKind>,
    /// The extra knockback ran, so the attacker slows and stops sprinting.
    pub knocked: bool,
}

impl Entities {
    /// Mobs a spear reaches (`ProjectileUtil.getHitEntitiesAlong`): every live
    /// mob whose box, grown by `margin`, crosses the segment from `eye +
    /// look * min` to `eye + look * (max + movement along the look)`, and
    /// nearer than `block` (the first block the ray from the eye hits), by
    /// distance from the eye.
    pub fn spear_targets(
        &self,
        eye: DVec3,
        look: DVec3,
        range: crate::weapons::spear::Range,
        along: f64,
        block: Option<f64>,
        except: Option<u32>,
    ) -> Vec<usize> {
        let look = look.normalize_or_zero();
        let reach = range.max + along.max(0.0);
        let end = block.map_or(reach, |b| b.min(reach));
        if end < range.min {
            return Vec::new();
        }
        let from = eye + look * range.min;
        let span = look * (end - range.min);
        let mut hits: Vec<(usize, f64)> = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive() && Some(m.uid) != except)
            .filter_map(|(i, m)| {
                let (min, max) = m.aabb();
                let grow = DVec3::splat(range.margin);
                let inside = from.cmpge(min - grow).all() && from.cmple(max + grow).all();
                let t = if inside { Some(0.0) } else { crate::physics::ray_aabb(from, span, min - grow, max + grow) };
                t.filter(|t| *t <= 1.0).map(|_| (i, m.pos.distance_squared(eye)))
            })
            .collect();
        hits.sort_by(|a, b| a.1.total_cmp(&b.1));
        hits.into_iter().map(|(i, _)| i).collect()
    }

    /// `Player.stabAttack` on mob `index`: the damage (with the weapon's
    /// enchantment bonus scaled by `strength`) and `hurtServer`'s push, then
    /// the 0.4 extra knockback and the Knockback enchantment's, then a
    /// dismount; fire aspect, loot with Looting and the usual reactions.
    pub fn stab_mob(&mut self, index: usize, stab: Stab) -> Stabbed {
        use crate::enchant::Enchantment;
        let Some(target) = self.mobs.get(index).filter(|m| m.alive()) else { return Stabbed::default() };
        if target.allay.as_ref().is_some_and(|s| s.player == Some(stab.owner)) {
            return Stabbed::default();
        }
        let (kind, pos) = (target.kind, target.pos);
        let enchants = stab.held.active_enchants();
        let magic = (crate::enchant::damage_bonus(enchants, kind.creature())
            + crate::weapons::impaling_bonus(enchants, kind))
            * stab.strength;
        let (knockback_level, fire) = crate::mining::weapon_extras(Some(stab.held));
        let mut out = Stabbed::default();
        let old_health = self.mobs[index].health;
        if let Some(damage) = stab.damage {
            self.animals_owner_attack(index, stab.owner);
            let pushes = [
                Some(Push::hurt(stab.origin, pos)),
                stab.knockback.then(|| Push::facing(crate::knockback::DEFAULT_STRENGTH, stab.yaw)),
            ];
            out.killed = self.hurt_by_player(index, damage + magic, &pushes, stab.owner);
            out.hurt = self.mobs[index].health < old_health || out.killed.is_some();
            if out.hurt && fire > 0.0 {
                self.mobs[index].ignite(fire);
            }
        } else if stab.knockback {
            let push = Push::facing(crate::knockback::DEFAULT_STRENGTH, stab.yaw);
            self.mobs[index].knock_back(push, &mut self.rng);
        }
        if stab.knockback {
            out.knocked = true;
            let extra = crate::knockback::attack_strength(0.0, u32::from(knockback_level), false);
            if extra > 0.0 && self.mobs[index].alive() {
                self.mobs[index].knock_back(Push::facing(f64::from(extra), stab.yaw), &mut self.rng);
            }
        }
        let mut dismounted = false;
        if stab.dismount && self.mobs[index].riding.is_some() {
            self.mobs[index].riding = None;
            dismounted = true;
        }
        out.affected = out.hurt || stab.knockback || dismounted;
        if out.hurt {
            self.player_hurt_mob(index, stab.owner, out.killed.is_some());
            if magic > 0.0 {
                let shape = self.mobs[index].shape();
                let mut burst = crate::particles::Burst::new(
                    crate::particles::Kind::MagicCrit,
                    pos + DVec3::Y * shape.height * 0.5,
                    32,
                );
                burst.spread = DVec3::new(shape.half_width * 0.5, shape.height * 0.25, shape.half_width * 0.5);
                self.particles.push(crate::particles::Request::Tracking(burst));
            }
        }
        if let Some(kind) = out.killed {
            let burning = self.mobs[index].burning || fire > 0.0;
            self.drop_loot_with_fire(kind, pos, enchants.level(Enchantment::Looting), burning, true);
        }
        out
    }

    /// `MaceItem.knockback` around mob `target`: every other living mob
    /// within 3.5 blocks of it (not the attacker's own tamed animals) is
    /// pushed away and up. `heavy` doubles it (a fall of more than 5 blocks).
    /// Level event 2013's dust is requested at the target's feet.
    pub fn smash_knockback(&mut self, target: usize, attacker: PlayerId, heavy: bool) {
        let Some(centre) = self.mobs.get(target).map(|m| m.pos) else { return };
        for (i, m) in self.mobs.iter_mut().enumerate() {
            if i == target || !m.alive() {
                continue;
            }
            let owned = m.animal.as_ref().is_some_and(|a| a.owner == Some(attacker));
            if owned || m.allay.as_ref().is_some_and(|s| s.player == Some(attacker)) {
                continue;
            }
            if let Some(push) = crate::weapons::mace::smash_push(centre, m.pos, heavy, m.knockback_resistance()) {
                m.vel += push * TICK_RATE;
            }
        }
        let mut burst = crate::particles::Burst::new(crate::particles::Kind::Poof, centre, 12);
        burst.spread = DVec3::new(1.2, 0.1, 1.2);
        self.particles.push(crate::particles::Request::Burst(burst));
    }

    /// Wind Burst's blast at `center` on the mobs around it (the attacker is
    /// pushed by its session): `ServerExplosion.hurtEntities` knockback with
    /// no damage. `exposure` gives each mob's share of the blast in view.
    pub fn wind_burst_mobs(&mut self, center: DVec3, multiplier: f32, exposure: impl Fn(DVec3, DVec3) -> f32) {
        for m in self.mobs.iter_mut().filter(|m| m.alive()) {
            let (min, max) = m.aabb();
            let eye = m.pos + DVec3::Y * (m.shape().height * 0.85);
            let seen = exposure(min, max);
            if let Some(push) =
                crate::weapons::mace::wind_burst_push(center, m.pos, eye, seen, multiplier, m.knockback_resistance())
            {
                m.vel += push * TICK_RATE;
            }
        }
        let mut burst = crate::particles::Burst::new(crate::particles::Kind::Poof, center, 20);
        burst.spread = DVec3::splat(1.0);
        self.particles.push(crate::particles::Request::Burst(burst));
    }
}
