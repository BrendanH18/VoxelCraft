//! Off-hand, shield and totem rules (PLY-05b), actor-owned so the desktop,
//! controller, LAN and CLI paths all share them.
//!
//! The off-hand is [`Inventory::offhand`](crate::inventory::Inventory). Java
//! tries the main hand first and the off-hand when the main hand's use passes;
//! [`PlayerSession::use_held`] and [`PlayerSession::tick_consumption_either`]
//! do that by trading the two stacks for the length of one call, so every
//! existing item rule works unchanged from the off-hand.
//!
//! A raised shield blocks after [`SHIELD_DELAY`] ticks. It stops attacks
//! that come from the front (within 90 degrees of the view, horizontally):
//! melee, arrows and explosions. Hits of 3 or more half-hearts wear it by
//! `1 + floor(damage)` (Unbreaking can save points), and an axe hit disables it
//! for [`DISABLE_TICKS`]. Fall, drowning, fire, lava, starvation and magic
//! have no source position and bypass it.
use glam::DVec3;

use super::PlayerSession;
use crate::{
    enchant,
    inventory::{HOTBAR_SLOTS, Stack},
    item::Item,
    simulation::effects::Effect,
};

/// Ticks a shield must be raised before it blocks (`BlocksAttacks.blockDelaySeconds` 0.25).
pub const SHIELD_DELAY: u32 = 5;
/// Ticks an axe hit disables a shield (`disable_cooldown_scale` 1.0, five seconds).
pub const DISABLE_TICKS: u32 = 100;
/// Damage below this does not wear the shield.
pub const WEAR_THRESHOLD: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Main,
    Off,
}

#[derive(Default)]
pub(super) struct State {
    raised: Option<Hand>,
    ticks: u32,
    cooldown: u32,
    /// Eating from the off-hand (see [`PlayerSession::tick_consumption_either`]).
    pub(super) eating_off: bool,
    /// The attack being resolved comes from an axe, which disables a shield it hits.
    axe: bool,
    /// Shared device hands ticked the shield this step (see
    /// [`PlayerSession::tick_blocking_unless_ticked`]).
    ticked: bool,
}

/// What a shield did to one attack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShieldHit {
    /// The attack was stopped: no damage, no knockback.
    pub blocked: bool,
    /// The hit wore the shield out and it broke.
    pub broke: bool,
}

/// Items whose right-click starts a held use, which the off-hand never overrides.
fn claims_use(item: Item, can_eat: bool) -> bool {
    item == Item::SHIELD
        || item.is_drink()
        || (item.food().is_some() && can_eat)
        || matches!(item, Item::BOW | Item::CROSSBOW | Item::CHARGED_CROSSBOW)
}

impl PlayerSession {
    /// The hand whose shield a right-click raises: a shield in the main hand, else one in the off-hand
    /// when the main hand's item does not claim the use.
    pub fn shield_hand(&self, slot: usize) -> Option<Hand> {
        let main = self.inventory.get(slot).map(|s| s.item);
        if main == Some(Item::SHIELD) {
            return Some(Hand::Main);
        }
        let off = self.inventory.offhand.map(|s| s.item);
        let claimed = main.is_some_and(|i| claims_use(i, self.can_eat(i)));
        (off == Some(Item::SHIELD) && !claimed).then_some(Hand::Off)
    }

    /// Starts raising a shield after a right-click nothing else answered.
    /// Refused while the shield is disabled or no shield is in reach.
    pub fn start_blocking(&mut self, slot: usize) -> bool {
        if self.hands.cooldown > 0
            || self.hands.raised.is_some()
            || self.vitals.is_dead()
            || self.sleeping.is_some()
            || !self.mode.can_interact()
        {
            return false;
        }
        let Some(hand) = self.shield_hand(slot) else { return false };
        self.hands.raised = Some(hand);
        self.hands.ticks = 0;
        true
    }

    /// One fixed tick of shield state: the raise delay counts up and the
    /// disable cooldown down. Releasing the button, losing the shield or
    /// switching hotbar slot lowers it.
    pub fn tick_blocking(&mut self, slot: usize, held: bool) {
        self.hands.ticked = true;
        self.hands.cooldown = self.hands.cooldown.saturating_sub(1);
        let Some(hand) = self.hands.raised else { return };
        let there = match hand {
            Hand::Main => self.inventory.get(slot).is_some_and(|s| s.item == Item::SHIELD),
            Hand::Off => self.inventory.offhand.is_some_and(|s| s.item == Item::SHIELD),
        };
        if !held || !there || self.hands.cooldown > 0 || self.vitals.is_dead() {
            self.stop_blocking();
        } else {
            self.hands.ticks = self.hands.ticks.saturating_add(1);
        }
    }

    /// An agent's own shield tick, skipped when shared device hands (a
    /// controller seat or LAN peer) already ticked it this step, so the
    /// shield advances once per tick whoever drives it.
    pub fn tick_blocking_unless_ticked(&mut self, slot: usize, held: bool) {
        if !std::mem::take(&mut self.hands.ticked) {
            self.tick_blocking(slot, held);
            self.hands.ticked = false;
        }
    }

    pub fn stop_blocking(&mut self) {
        self.hands.raised = None;
        self.hands.ticks = 0;
    }

    /// A shield is held up (Java's `isUsingItem` for a shield), blocking or not yet.
    pub fn shield_raised(&self) -> bool {
        self.hands.raised.is_some()
    }

    /// Java's `isBlocking`: raised for at least [`SHIELD_DELAY`] ticks.
    pub fn blocking(&self) -> bool {
        self.hands.raised.is_some() && self.hands.ticks >= SHIELD_DELAY
    }

    /// Ticks left before a disabled shield can be raised again.
    pub fn shield_cooldown(&self) -> u32 {
        self.hands.cooldown
    }

    /// Share of the disable cooldown left, for the cooldown overlay.
    pub fn shield_cooldown_fraction(&self) -> f32 {
        self.hands.cooldown as f32 / DISABLE_TICKS as f32
    }

    /// An axe hit (`Player.disableShield`): a raised shield is lowered and
    /// cannot be raised for [`DISABLE_TICKS`]. Returns whether it was blocking.
    pub fn disable_shield(&mut self) -> bool {
        let was = self.blocking();
        if was {
            self.stop_blocking();
            self.hands.cooldown = DISABLE_TICKS;
        }
        was
    }

    /// Lets a raised shield take an attack of `amount` half hearts from the
    /// horizontal direction `toward` (from this player to the attacker).
    /// Attacks from behind (more than 90 degrees off the view) and ones
    /// without a source are not blocked. Wear and breakage are handled here.
    pub fn shield_intercept(&mut self, slot: usize, amount: f32, toward: DVec3) -> ShieldHit {
        if !self.blocking() || amount <= 0.0 {
            return ShieldHit::default();
        }
        let flat = DVec3::new(toward.x, 0.0, toward.z);
        let view = self.player.forward().as_dvec3();
        let facing = DVec3::new(view.x, 0.0, view.z);
        // A hit straight from above or below has no horizontal source: not blocked.
        if flat.length_squared() <= 1e-9 || flat.normalize().dot(facing) <= 0.0 {
            return ShieldHit::default();
        }
        let mut hit = ShieldHit { blocked: true, broke: false };
        let axe = std::mem::take(&mut self.hands.axe);
        if amount >= WEAR_THRESHOLD && self.mode.is_survival() {
            let points = 1 + amount.floor() as u16;
            let hand = self.hands.raised.expect("blocking implies raised");
            let cell = match hand {
                Hand::Main => &mut self.inventory.slots[slot],
                Hand::Off => &mut self.inventory.offhand,
            };
            if let Some(s) = cell {
                let worn = (0..points).filter(|_| enchant::wears(s.enchants, false, enchant::roll())).count() as u16;
                s.damage = s.damage.saturating_add(worn);
                if s.damage >= Item::SHIELD.durability().unwrap_or(u16::MAX) {
                    *cell = None;
                    hit.broke = true;
                }
            }
            if hit.broke {
                self.stop_blocking();
            }
        }
        if axe {
            self.disable_shield();
        }
        hit
    }

    /// Marks the next attack on this player as an axe's (`Player.canDisableShield`),
    /// or clears the mark once the attack has been resolved.
    pub fn axe_incoming(&mut self, axe: bool) {
        self.hands.axe = axe;
    }

    /// Swap-item-with-offhand: also ends whatever the hands were doing.
    pub fn swap_hands(&mut self, slot: usize) -> bool {
        if !self.mode.can_interact() || slot >= HOTBAR_SLOTS || self.vitals.is_dead() || self.sleeping.is_some() {
            return false;
        }
        self.cancel_consumption();
        self.cancel_draw();
        self.stop_blocking();
        self.inventory.swap_offhand(slot);
        true
    }

    /// `LivingEntity.checkTotemDeathProtection`: a lethal hit that left the
    /// player dead is undone by a totem of undying in either hand (main
    /// hand first). The totem is used up; health is 1 half heart with
    /// Regeneration II 45 s, Absorption II 5 s and Fire Resistance 40 s.
    /// Void and `/kill` damage bypass it. Returns whether it saved the player.
    pub fn totem_saves(&mut self, slot: usize) -> bool {
        let cause = self.vitals.death.as_deref().unwrap_or_default();
        if !self.vitals.is_dead() || matches!(cause, "fell out of the world" | "was killed") {
            return false;
        }
        let taken: Option<Stack> = if self.inventory.get(slot).is_some_and(|s| s.item == Item::TOTEM_OF_UNDYING) {
            self.inventory.slots[slot].take()
        } else if self.inventory.offhand.is_some_and(|s| s.item == Item::TOTEM_OF_UNDYING) {
            self.inventory.offhand.take()
        } else {
            None
        };
        let Some(totem) = taken else { return false };
        // Creative keeps nothing to lose; only a one-use stack is consumed here.
        if totem.count > 1 {
            let rest = Stack { count: totem.count - 1, ..totem };
            if self.inventory.get(slot).is_none() {
                self.inventory.slots[slot] = Some(rest);
            } else {
                self.inventory.offhand = Some(rest);
            }
        }
        self.vitals.death = None;
        self.vitals.health = 1.0;
        self.vitals.effects.clear();
        for (effect, amplifier, ticks) in
            [(Effect::Regeneration, 1, 900), (Effect::Absorption, 1, 100), (Effect::FireResistance, 0, 800)]
        {
            self.vitals.apply_effect(effect, amplifier, ticks);
        }
        self.vitals.extinguish();
        true
    }

    /// Whether the off-hand holds an item that could be eaten or drunk now,
    /// and the main hand's item (a shield, bow or food) doesn't claim the use.
    fn offhand_consumable(&mut self, slot: usize) -> bool {
        if self.inventory.get(slot).is_some_and(|s| claims_use(s.item, self.can_eat(s.item))) {
            return false;
        }
        self.inventory.swap_offhand(slot);
        let ok = self.consumption_duration(slot).is_ok();
        self.inventory.swap_offhand(slot);
        ok
    }

    /// [`PlayerSession::tick_consumption`] that falls back to the off-hand
    /// when the main hand holds nothing to eat or drink, as Java's use does.
    /// The hand chosen at the start stays chosen until the use ends.
    pub fn tick_consumption_either(
        &mut self,
        slot: usize,
        active: bool,
        entities: &mut crate::entity::Entities,
    ) -> Result<super::consumption::Step, &'static str> {
        let off = active
            && slot < HOTBAR_SLOTS
            && (self.hands.eating_off || (self.consumption_duration(slot).is_err() && self.offhand_consumable(slot)));
        if !off {
            self.hands.eating_off = false;
            return self.tick_consumption(slot, active, entities);
        }
        self.inventory.swap_offhand(slot);
        let result = self.tick_consumption(slot, active, entities);
        self.inventory.swap_offhand(slot);
        self.hands.eating_off = result.is_ok() && self.consuming();
        result
    }

    /// [`PlayerSession::consumption_duration`] for whichever hand would be used.
    pub fn consumption_duration_either(&mut self, slot: usize) -> Result<u32, &'static str> {
        let main = self.consumption_duration(slot);
        if main.is_ok() || slot >= HOTBAR_SLOTS || !self.offhand_consumable(slot) {
            return main;
        }
        self.inventory.swap_offhand(slot);
        let off = self.consumption_duration(slot);
        self.inventory.swap_offhand(slot);
        off
    }

    /// The item an eat or drink would use right now: the main hand's if it
    /// can be consumed, else the off-hand's.
    pub fn consumable_in_hands(&mut self, slot: usize) -> Option<Item> {
        if self.consumption_duration(slot).is_ok() {
            return self.inventory.get(slot).map(|s| s.item);
        }
        if self.hands.eating_off || self.offhand_consumable(slot) {
            self.inventory.offhand.map(|s| s.item)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
