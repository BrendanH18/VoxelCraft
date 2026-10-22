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
}

impl PlayerSession {
    pub fn cancel_consumption(&mut self) {
        self.consumption = State::default();
    }
    /// Eating or drinking is under way (Java's `isUsingItem` for food).
    pub fn consuming(&self) -> bool {
        self.consumption.key.is_some()
    }
    /// Java's `isUsingItem` for the movement slow-down: eating, drinking, or
    /// drawing a bow or crossbow.
    pub fn using_item(&self) -> bool {
        self.consuming() || self.drawing()
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
        if !active {
            self.cancel_consumption();
            return Ok(Step::default());
        }
        let duration = self.consumption_duration(slot)?;
        let held = self.inventory.get(slot).expect("validated held stack");
        let key = Some((slot, held.item));
        if self.consumption.key != key {
            self.consumption = State { key, ticks: 0, duration };
        }
        self.consumption.ticks += 1;
        if self.consumption.ticks < duration {
            return Ok(Step { progress: self.consumption_progress(), completed: None });
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
            if held.item == Item::HONEY_BOTTLE {
                self.vitals.effects.remove(crate::simulation::effects::Effect::Poison);
            }
            for &(effect, amplifier, ticks) in effects {
                magic_damage += self.vitals.apply_effect(effect, amplifier, ticks);
            }
        }
        Ok(Step { progress: 0., completed: Some(Consumed { player: self.id, item: held.item, magic_damage }) })
    }
}
