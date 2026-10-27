//! Actor-owned melee following Java 26.3 `Player.attack`; adapters present outcomes.
//!
//! Java has no hard attack cooldown: every attack lands, scaled by the
//! attack-strength charge. The charge counts fixed ticks since the last
//! attack, miss or main-hand item change, over `20 / attack_speed` ticks.
use super::PlayerSession;
use crate::{
    enchant::{self, Creature},
    entity::{Entities, MobKind, PlayerId, PlayerMelee},
    inventory::HOTBAR_SLOTS,
    item::{Item, ToolKind},
    knockback, mining, physics,
    simulation::survival,
    world::World,
};
use glam::{DVec3, IVec3};

#[derive(Default)]
pub(super) struct State {
    ticker: u32,
    /// Java's `itemSwapTicker`: the client-side count the first-person hand
    /// lowers by; it restarts with the charge (attack, miss, item change).
    swap_ticker: u32,
    last_held: Option<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    FenceKnot,
    Cart,
    Fireball,
    /// An End crystal or dragon part.
    Fight,
    Mob(usize),
}

/// What the crosshair attack indicator shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttackIndicator {
    /// The charge, 0..1, as a filling bar.
    Charging(f32),
    /// Aimed at a living target with a full charge.
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Hit {
        player: PlayerId,
        target: Target,
        /// Where the hit landed, for sounds and particles.
        at: DVec3,
        strength: f32,
        critical: bool,
        /// Java's `strong` attack (charge above 0.9).
        strong: bool,
        /// A sprinting, strong attack: Java's extra half level of knockback and its sound.
        sprint_knockback: bool,
        /// A sword sweep struck (and `doSweepAttack` ran): its sound and particle.
        sweep: bool,
        /// The hit's extra knockback stopped the attacker's sprint and slowed its motion
        /// (`causeExtraKnockback`); adapters holding a sprint flag clear it.
        sprint_cancelled: bool,
        killed: Option<MobKind>,
        tool_broken: Option<Item>,
    },
    /// Nothing attackable within reach; the charge restarts as in Java.
    Missed { player: PlayerId },
}

impl Outcome {
    /// The sounds Java's `Player.attack` plays for this hit, in order.
    pub fn sounds(&self) -> Vec<crate::knockback::AttackSound> {
        match *self {
            Outcome::Hit { critical, strong, sprint_knockback, sweep, .. } => {
                crate::knockback::attack_sounds(strong, sprint_knockback, critical, sweep, true)
            }
            Outcome::Missed { .. } => Vec::new(),
        }
    }
}

impl PlayerSession {
    /// Java 26.3 entity interaction range, including Creative's +2 modifier.
    pub fn entity_reach(&self) -> f64 {
        if self.mode.is_creative() { 5.0 } else { 3.0 }
    }

    /// Advance the attack-strength charge by one fixed tick. Switching to a
    /// different main-hand item restarts it (Java `Player.tick`).
    pub fn tick_attack_strength(&mut self, slot: usize) {
        let held = self.inventory.get(slot).map(|s| s.item);
        self.combat.ticker = self.combat.ticker.saturating_add(1);
        self.combat.swap_ticker = self.combat.swap_ticker.saturating_add(1);
        if held != self.combat.last_held {
            self.combat.ticker = 0;
            self.combat.swap_ticker = 0;
            self.combat.last_held = held;
        }
    }

    /// Java's `getCurrentItemAttackStrengthDelay` for the item in `slot`,
    /// including Haste and Mining Fatigue.
    pub fn attack_strength_delay(&self, slot: usize) -> f32 {
        mining::attack_strength_delay_with(self.inventory.get(slot).map(|s| s.item), &self.vitals.effects)
    }

    /// Java's `getAttackStrengthScale(partial)` for the item in `slot`.
    pub fn attack_strength(&self, slot: usize, partial: f32) -> f32 {
        ((self.combat.ticker as f32 + partial) / self.attack_strength_delay(slot)).clamp(0.0, 1.0)
    }

    /// Java's `getItemSwapScale(partial)`: how far the first-person hand has
    /// risen again since the last attack, miss or item change.
    pub fn item_swap_scale(&self, slot: usize, partial: f32) -> f32 {
        ((self.combat.swap_ticker as f32 + partial) / self.attack_strength_delay(slot)).clamp(0.0, 1.0)
    }

    /// Java's `cannotAttackWithItem(stack, lead)`: an item with a minimum
    /// attack charge refuses attacks until the charge, `lead` ticks ahead, reaches it.
    pub fn cannot_attack_with(&self, slot: usize, lead: u32) -> bool {
        let minimum = mining::minimum_attack_charge(self.inventory.get(slot).map(|s| s.item));
        mining::charge_refuses_attack(minimum, self.combat.ticker, lead, self.attack_strength_delay(slot))
    }

    /// Java's crosshair attack indicator: a filling bar while the charge is
    /// below full, or the "full" mark when aimed at a living target with a
    /// fully charged weapon slower than 5 ticks. `None` when neither shows.
    pub fn attack_indicator(&self, slot: usize, aiming_at_living: bool) -> Option<AttackIndicator> {
        let scale = self.attack_strength(slot, 0.0);
        if aiming_at_living && scale >= 1.0 && self.attack_strength_delay(slot) > 5.0 {
            Some(AttackIndicator::Full)
        } else if scale < 1.0 {
            Some(AttackIndicator::Charging(scale))
        } else {
            None
        }
    }

    /// Java's `Player.canCriticalAttack`: the player has fallen some distance
    /// since the ground (`fallDistance > 0`, which Water, ladders, flight and
    /// Slow Falling clear), is airborne, not riding, not sprinting
    /// and hits a living target. The charge check is the caller's.
    pub fn can_critical_attack(&self, living: bool, sprinting: bool) -> bool {
        let p = &self.player;
        self.vitals.fall_distance(p.pos.y) > 0.0
            && !p.on_ground
            && !p.climbing
            && !p.in_water
            && !self.vitals.effects.mobility_restricted()
            && p.vehicle.is_none()
            && living
            && !sprinting
    }

    /// Java's knockback on this player: each push halves the motion and adds the
    /// push, a grounded player rises up to 0.4, and armor's knockback resistance
    /// scales it. `jitter` supplies `nextDouble()` for a zero-length direction.
    /// Call it after the hit's damage lands: a stronger hit inside an immunity
    /// window skips the default push and the hurt direction, as `hurtServer` does.
    pub fn knocked_back(&mut self, hit: &knockback::Hit, jitter: &mut dyn FnMut() -> f64) {
        let fresh = self.vitals.last_hit_fresh();
        let hit = if fresh { *hit } else { hit.without_default() };
        if fresh && let Some(away) = hit.away() {
            self.player.hurt_from(away);
        }
        let resistance = f64::from(self.inventory.knockback_resistance()).clamp(-2.0, 1.0);
        let motion = hit.apply(self.player.vel / knockback::TICK_RATE, resistance, self.player.on_ground, jitter);
        self.player.vel = motion * knockback::TICK_RATE;
    }

    /// Restart the charge, as an attack or a swing at nothing does.
    pub fn reset_attack_strength(&mut self) {
        self.combat.ticker = 0;
        self.combat.swap_ticker = 0;
    }

    /// Melee whatever is under the crosshair within entity reach and in front
    /// of the targeted block. `sprinting` is the adapter's current sprint state.
    pub fn attack(
        &mut self,
        slot: usize,
        world: &World,
        entities: &mut Entities,
        sprinting: bool,
    ) -> Result<Outcome, &'static str> {
        if !self.mode.can_interact() || self.vitals.is_dead() || self.sleeping.is_some() {
            return Err("player cannot attack now");
        }
        if slot >= HOTBAR_SLOTS {
            return Err("selected slot out of range");
        }
        if !self.player.pos.is_finite() || !self.player.yaw.is_finite() || !self.player.pitch.is_finite() {
            return Err("player view is unavailable");
        }
        let stack = self.inventory.get(slot);
        if stack.is_some_and(|s| s.count == 0 || s.count > s.item.max_stack()) {
            return Err("invalid held stack");
        }
        // The server refuses attacks with an item that needs a minimum charge.
        if self.cannot_attack_with(slot, 5) {
            return Err("the held item is not charged enough to attack");
        }
        let held = stack.map(|s| s.item);
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        // Entities behind the targeted block are out of reach.
        let block = self
            .block_target(world)
            .and_then(|(b, _)| physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + DVec3::ONE));
        let reach = block.map_or(self.entity_reach(), |d| d.min(self.entity_reach()));
        let target = if crate::survival_items::knot_target(&self.player, world, entities, reach).is_some() {
            Target::FenceKnot
        } else if entities
            .cart_ray(eye, dir, crate::entity::minecart::interaction_reach(world, eye, dir, reach))
            .is_some()
        {
            Target::Cart
        } else if entities.large_fireball(eye, dir, reach).is_some() {
            Target::Fireball
        } else if entities.fight_raycast(eye, dir, reach).is_some() {
            Target::Fight
        } else if let Some((i, _)) = entities.raycast(eye, dir, reach) {
            Target::Mob(i)
        } else {
            self.reset_attack_strength();
            return Ok(Outcome::Missed { player: self.id });
        };

        // Java `Player.attack`, evaluated before the charge restarts.
        let strength = self.attack_strength(slot, 0.5);
        let strong = strength > 0.9;
        let sprint_knockback = sprinting && strong;
        let p = &self.player;
        let living = matches!(target, Target::Mob(_));
        let critical = strong && self.can_critical_attack(living, sprinting);
        // `isSweepAttack`: last tick's horizontal movement against 2.5x the movement speed.
        let (speed_level, slowness_level) = self.vitals.effects.movement_speed_levels();
        let speed = knockback::movement_speed(speed_level, slowness_level, sprinting);
        let moved_sqr = (p.vel.with_y(0.0) / knockback::TICK_RATE).length_squared();
        let sword = held.and_then(Item::as_tool).is_some_and(|(k, _)| k == ToolKind::Sword);
        let sweep = knockback::sweep_allowed(strong, critical, sprint_knockback, p.on_ground, moved_sqr, speed, sword);
        let bonus = self.vitals.effects.attack_bonus();
        let base = (mining::attack_damage(held) + bonus).max(0.0)
            * (0.2 + strength * strength * 0.8)
            * if critical { 1.5 } else { 1.0 };
        let enchants = stack.map_or(Default::default(), |s| s.active_enchants());
        let other = enchant::damage_bonus(enchants, Creature::Other) * strength;
        // `getKnockback`: Knockback level over two, plus half a level for a sprint hit.
        let extra = knockback::attack_strength(0.0, u32::from(mining::weapon_extras(stack).0), sprint_knockback);
        let (origin, yaw) = (p.pos, knockback::java_yaw_degrees(p.yaw));
        self.reset_attack_strength();

        let mut killed = None;
        let landed;
        let mut swept = false;
        // Java `itemAttackInteraction` wears the weapon only against a
        // LivingEntity (a dragon part counts as its dragon), never a crystal or cart.
        let mut living_hit = false;
        let at = match target {
            Target::FenceKnot => {
                let knot = crate::survival_items::knot_target(&self.player, world, entities, reach);
                landed = knot.is_some_and(|cell| entities.break_fence_knot(cell));
                knot.map_or(eye + dir, |c: IVec3| c.as_dvec3() + DVec3::splat(0.5))
            }
            Target::Cart => {
                let cart_reach = crate::entity::minecart::interaction_reach(world, eye, dir, reach);
                // `Cushion.isBreakingDeniedFor`: only players who may build break cushions.
                let denied = !self.mode.can_build()
                    && entities
                        .cart_ray(eye, dir, cart_reach)
                        .is_some_and(|i| entities.minecarts[i].kind.cushion().is_some());
                landed = !denied && entities.hurt_cart(eye, dir, cart_reach, self.mode.is_creative());
                eye + dir * 2.0
            }
            Target::Fireball => {
                landed = entities.punch_fireball(eye, dir, reach);
                eye + dir * 2.0
            }
            Target::Fight => {
                let (hit, t) = entities.fight_raycast(eye, dir, reach).ok_or("target vanished")?;
                living_hit = matches!(hit, crate::entity::dragon::Hit::Dragon(_));
                landed = entities.strike(hit, base + other, self.id);
                if landed && other > 0.0 {
                    let mut burst = crate::particles::Burst::new(crate::particles::Kind::MagicCrit, eye + dir * t, 16);
                    burst.spread = DVec3::splat(0.4);
                    entities.particles.push(crate::particles::Request::Tracking(burst));
                }
                eye + dir * t
            }
            Target::Mob(i) => {
                let at = entities.mobs[i].pos + DVec3::Y * 0.5;
                landed = entities.mobs[i].alive();
                let hit = PlayerMelee {
                    held: stack,
                    bonus,
                    strength,
                    critical,
                    sprint_knockback,
                    origin,
                    yaw,
                    sweep,
                    owner: self.id,
                };
                killed = entities.player_melee(i, hit);
                living_hit = true;
                swept = landed && sweep;
                at
            }
        };
        if swept {
            // `doSweepAttack`'s particle, one block ahead of the attacker: (x - sin, z + cos).
            let (sin, cos) = knockback::facing_vector(yaw);
            let at = DVec3::new(origin.x - sin, origin.y + 0.5 * self.player.collision_shape().height, origin.z - cos);
            entities.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
                crate::particles::Kind::Poof,
                at,
                1,
            )));
        }
        // `causeExtraKnockback`: any hit that pushes stops the sprint and slows the attacker.
        // A deflected fireball and a leash knot return before any hurt.
        let sprint_cancelled = landed && extra > 0.0 && matches!(target, Target::Mob(_) | Target::Fight | Target::Cart);
        if sprint_cancelled {
            self.player.vel = knockback::slow_attacker(self.player.vel);
            self.player.sprinting = false;
        }
        let mut tool_broken = None;
        if landed && self.mode.is_survival() {
            // A fireball is only deflected (Java returns before exhaustion).
            if matches!(target, Target::Mob(_) | Target::Fight | Target::Cart) {
                self.vitals.hunger.exhaust(survival::EXHAUST_ATTACK);
            }
            if let Some(item) = held
                && living_hit
                && self.inventory.wear(slot, mining::wear(item, true))
            {
                tool_broken = Some(item);
            }
        }
        Ok(Outcome::Hit {
            player: self.id,
            target,
            at,
            strength,
            critical,
            strong,
            sprint_knockback,
            sweep: swept,
            sprint_cancelled,
            killed,
            tool_broken,
        })
    }
}
