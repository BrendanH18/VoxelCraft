//! Fixed-tick, actor-owned food/drink validation and consumption transactions.
use super::PlayerSession;
use crate::{
    entity::{Entities, PlayerId},
    inventory::{HOTBAR_SLOTS, Stack},
    item::Item,
};

#[derive(Default)]
pub(super) struct State {
    key: Option<(usize, Item)>,
    ticks: u32,
    duration: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Consumed {
    pub player: PlayerId,
    pub item: Item,
    /// Deliver through the caller's existing damage/death route after inventory exchange.
    pub magic_damage: f32,
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub progress: f32,
    pub completed: Option<Consumed>,
    /// v1.0 gap Track 2h: this tick's `ItemStack.onUseTick` emits the use
    /// effects (`Consumable.shouldEmitParticlesAndSounds`): the eating or
    /// drinking sound, and five item particles for food.
    pub effects: bool,
}

/// v1.0 gap Track 2h: `Consumable.shouldEmitParticlesAndSounds` on the
/// tick whose use had `remaining` ticks left before it of `duration`: past
/// the first 21.875% of the use (`CONSUME_EFFECTS_START_FRACTION`), every
/// fourth tick (`CONSUME_EFFECTS_INTERVAL`).
pub fn use_effects(duration: u32, remaining: u32) -> bool {
    let used = duration.saturating_sub(remaining);
    let wait = (duration as f32 * 0.21875) as u32;
    used > wait && remaining.is_multiple_of(4)
}

impl PlayerSession {
    /// v0.16 Track A: after eating chorus fruit (`Consumed::item`), the
    /// consume effect `TeleportRandomlyConsumeEffect` moves the player to
    /// a random spot within eight blocks with ground under it, resetting
    /// the fall. Returns where they left from, or `None` when no try fitted.
    pub fn chorus_teleport(&mut self, world: &crate::world::World, entities: &mut Entities) -> Option<glam::DVec3> {
        // `TeleportRandomlyConsumeEffect`: `stopRiding` first, or the seat
        // would pull the player back.
        if self.player.vehicle.take().is_some() {
            entities.dismount(world, self.id);
        }
        let mut next = || f64::from(entities.roll());
        let to = crate::content::chorus::teleport_target(world, self.player.pos, crate::player::SHAPE, &mut next)?;
        let from = self.player.pos;
        self.player.pos = to;
        self.player.vel = glam::DVec3::ZERO;
        self.vitals.reset_fall();
        // `TeleportRandomlyConsumeEffect.apply`: `teleport` where they left.
        let source = crate::world::vibration::EventSource::player(self.id, self.player.sneaking);
        entities.sculk.post(from, crate::world::vibration::GameEvent::Teleport, source, None);
        Some(from)
    }

    pub fn cancel_consumption(&mut self) {
        self.consumption = State::default();
        self.hands.eating_off = false;
    }
    /// Eating or drinking is under way (Java's `isUsingItem` for food).
    pub fn consuming(&self) -> bool {
        self.consumption.key.is_some()
    }
    /// Java's `isUsingItem` for the movement slow-down: eating, drinking,
    /// drawing a bow or crossbow, brushing, emptying a bundle, or holding up
    /// a blown goat horn.
    pub fn using_item(&self) -> bool {
        // A spear's charge keeps full speed and the sprint (`UseEffects(true, false, 1.0)`).
        self.consuming()
            || (self.drawing() && !self.charging_spear())
            || self.shield_raised()
            || self.brushing()
            || self.bundle_in_use()
            || self.tooting()
    }
    pub fn consumption_progress(&self) -> f32 {
        if self.consumption.duration == 0 {
            0.
        } else {
            self.consumption.ticks as f32 / self.consumption.duration as f32
        }
    }
    pub fn consumption_elapsed(&self) -> f64 {
        f64::from(self.consumption.ticks) * crate::simulation::TICK_SECONDS
    }

    /// Validate starting or continuing use without changing player or world state.
    pub fn consumption_duration(&self, slot: usize) -> Result<u32, &'static str> {
        if self.vitals.is_dead() || self.sleeping.is_some() || !self.mode.can_interact() {
            return Err("player cannot consume now");
        }
        if slot >= HOTBAR_SLOTS {
            return Err("selected slot out of range");
        }
        let held = self.inventory.get(slot).ok_or("selected slot empty")?;
        if held.count == 0 || held.count > held.item.max_stack() {
            return Err("invalid held stack");
        }
        let duration = crate::content::consumption::ticks(held.item).ok_or("selected item is not food or a drink")?;
        // `UseCooldown`: chorus fruit can't be eaten again for a second.
        if held.item == crate::content::chorus::CHORUS_FRUIT && self.vitals.chorus_cooldown > 0.0 {
            return Err("chorus fruit is cooling down");
        }
        if !held.item.is_drink() && !self.can_eat(held.item) {
            return Err("not hungry");
        }
        Ok(duration)
    }

    /// Java's `Player.canEat`: hungry, always-edible food, or Creative
    /// (the invulnerable ability) may eat `food`.
    pub fn can_eat(&self, food: Item) -> bool {
        self.mode.is_creative() || food.always_edible() || self.vitals.hunger.can_eat()
    }

    /// Advance one authoritative gameplay tick, never a caller-supplied duration.
    /// Invalid requests validate before inventory, effects or entity RNG mutation.
    pub fn tick_consumption(
        &mut self,
        slot: usize,
        active: bool,
        entities: &mut Entities,
    ) -> Result<Step, &'static str> {
        use crate::world::vibration::GameEvent as Use;
        if !active {
            // v1.0 Track 9: `stopUsingItem` while eating posts `item_interact_finish`.
            if self.consuming() {
                self.use_vibration(entities, Use::ItemInteractFinish);
            }
            self.cancel_consumption();
            return Ok(Step::default());
        }
        let duration = self.consumption_duration(slot)?;
        let held = self.inventory.get(slot).expect("validated held stack");
        let key = Some((slot, held.item));
        if self.consumption.key != key {
            // v1.0 Track 9: `startUsingItem` posts `item_interact_start`.
            self.consumption = State { key, ticks: 0, duration };
            self.use_vibration(entities, Use::ItemInteractStart);
        }
        // `updateUsingItem`: `onUseTick` with the ticks left, then one fewer.
        let effects = use_effects(duration, duration.saturating_sub(self.consumption.ticks));
        self.consumption.ticks += 1;
        if self.consumption.ticks < duration {
            return Ok(Step { progress: self.consumption_progress(), completed: None, effects });
        }
        self.cancel_consumption();
        let mut magic_damage = 0.;
        if held.item == Item::MILK_BUCKET {
            self.vitals.effects.clear();
            crate::survival_items::exchange(
                &mut self.inventory,
                slot,
                Item::BUCKET,
                self.mode.is_creative(),
                entities,
                &self.player,
            );
        } else if let Some(amplifier) = crate::world::trial::ominous_amplifier(held.item) {
            // `OminousBottleAmplifier.onConsume`: Bad Omen for 120000 ticks;
            // the bottle is used up (Creative keeps it).
            if !self.mode.is_creative() {
                self.inventory.take_one(slot);
            }
            self.vitals.apply_effect(
                crate::simulation::effects::Effect::BadOmen,
                amplifier,
                crate::world::trial::BAD_OMEN_TICKS,
            );
        } else if let Some(potion) = held.item.as_potion() {
            if self.mode.is_survival() {
                self.inventory.slots[slot] = Some(Stack::new(Item::GLASS_BOTTLE, 1));
            }
            magic_damage = potion.drink(&mut self.vitals);
        } else if let Some((food, saturation)) = held.item.food() {
            let effects = held.item.food_effect_roll(entities.roll());
            // Creative (infinite materials) keeps the stack and its remainder untouched.
            if !self.mode.is_creative() {
                if let Some(remainder) = held.item.remainder() {
                    crate::survival_items::exchange(
                        &mut self.inventory,
                        slot,
                        remainder,
                        false,
                        entities,
                        &self.player,
                    );
                } else {
                    self.inventory.take_one(slot);
                }
            }
            self.vitals.hunger.eat(food, saturation);
            if held.item == Item::SUSPICIOUS_STEW {
                magic_damage += crate::entity::mooshroom::consume_stew(held, &mut self.vitals);
            }
            if held.item == Item::HONEY_BOTTLE {
                self.vitals.effects.remove(crate::simulation::effects::Effect::Poison);
            }
            if held.item == crate::content::chorus::CHORUS_FRUIT {
                self.vitals.chorus_cooldown = crate::content::chorus::COOLDOWN;
            }
            for &(effect, amplifier, ticks) in effects {
                magic_damage += self.vitals.apply_effect(effect, amplifier, ticks);
            }
        }
        // `Consumable.onConsume` and `LivingEntity.completeUsingItem`.
        let drink =
            held.item == Item::MILK_BUCKET || held.item.as_potion().is_some() || held.item == Item::HONEY_BOTTLE;
        let source = crate::world::vibration::EventSource::player(self.id, self.player.sneaking);
        use crate::world::vibration::GameEvent;
        let event = if drink { GameEvent::Drink } else { GameEvent::Eat };
        entities.sculk.post(self.player.pos, event, source, None);
        entities.sculk.post(self.player.pos, GameEvent::ItemInteractFinish, source, None);
        Ok(Step { progress: 0., completed: Some(Consumed { player: self.id, item: held.item, magic_damage }), effects })
    }
}

#[cfg(test)]
mod rhythm_tests {
    /// v1.0 gap Track 2h: `shouldEmitParticlesAndSounds` over a 32-tick
    /// meal: past its first 7 ticks, every fourth (with 24, 20, ... 4 left),
    /// and over a 16-tick one (dried kelp) with 12, 8 and 4 left.
    #[test]
    fn use_effects_follow_javas_rhythm() {
        let left = |duration: u32| -> Vec<u32> {
            (1..=duration).rev().filter(|&remaining| super::use_effects(duration, remaining)).collect()
        };
        assert_eq!(left(32), vec![24, 20, 16, 12, 8, 4]);
        assert_eq!(left(16), vec![12, 8, 4]);
        assert_eq!(left(40), vec![28, 24, 20, 16, 12, 8, 4]);
    }
}
