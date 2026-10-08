//! The player's bow: hold right-click to draw, release to shoot an arrow.
//! A crossbow shares the draw timer: hold right-click until it loads, then
//! right-click again to shoot.

use crate::audio::sounds::Sound;
use crate::item::Item;
use crate::world::block::Block;

use super::Game;

/// Seconds to draw the bow fully (Minecraft's 20 ticks).
pub(super) const FULL_DRAW: f64 = 1.0;
/// Weaker draws than this don't shoot.
const MIN_POWER: f32 = 0.1;
/// Seconds to load a crossbow (Java's 25 ticks without Quick Charge).
pub(super) const CROSSBOW_CHARGE: f64 = 1.25;
/// A crossbow bolt leaves at 3.15 blocks a tick against a full bow's 3.0.
const CROSSBOW_POWER: f32 = 3.15 / 3.0;

/// Bow power 0..1 after drawing for `secs`, rising quickly then easing in,
/// as in Minecraft.
pub(super) fn power(secs: f64) -> f32 {
    let f = (secs / FULL_DRAW) as f32;
    ((f * f + 2.0 * f) / 3.0).min(1.0)
}

impl Game {
    /// Whether the crosshair is on a block that right-click uses
    /// (containers and beds) rather than building against it.
    pub(super) fn aiming_at_usable(&self) -> bool {
        self.target().and_then(|(pos, _)| self.world.get_block(pos)).is_some_and(|b| {
            b == Block::CRAFTING_TABLE
                || b.is_bed()
                || b.is_door()
                || b.is_gate()
                || crate::world::furnace::is_furnace(b)
                || crate::world::chest::is_chest(b)
                || b == Block::BREWING_STAND
                || b == Block::ENCHANTING_TABLE
                || b.is_anvil()
                || b == Block::SMITHING_TABLE
        })
    }

    /// Right-click with a bow: starts drawing it if there's an arrow to
    /// shoot (creative needs none). Returns whether it did.
    pub(super) fn start_draw(&mut self) -> bool {
        let has_arrow = self.mode.is_creative() || self.inventory.find(Item::ARROW).is_some();
        if self.aiming_at_usable() {
            return false;
        }
        if self.held_item() == Some(Item::CHARGED_CROSSBOW) {
            self.fire_crossbow();
            return true;
        }
        if !matches!(self.held_item(), Some(Item::BOW | Item::CROSSBOW)) || !has_arrow {
            return false;
        }
        self.actions.bow_draw = Some(0.0);
        true
    }

    /// A drawn crossbow holds its bolt: the held item becomes the charged
    /// crossbow (keeping wear and enchantments) and an arrow is used up.
    fn load_crossbow(&mut self) {
        let slot = self.actions.selected;
        if self.mode.is_survival() {
            let Some(arrow) = self.inventory.find(Item::ARROW) else { return };
            self.inventory.take_one(arrow);
        }
        if let Some(stack) = &mut self.inventory.slots[slot] {
            stack.item = Item::CHARGED_CROSSBOW;
        }
        let eye = self.player.eye();
        self.audio.play(Sound::Bow, Some(eye), 0.5, (0.55, 0.65));
    }

    /// Shoots the bolt in a charged crossbow, which then needs loading again.
    fn fire_crossbow(&mut self) {
        let slot = self.actions.selected;
        let survival = self.mode.is_survival();
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        self.mobs.entities.shoot_enchanted(eye, dir, CROSSBOW_POWER, survival, Default::default());
        self.audio.play(Sound::Bow, Some(eye), 0.9, (1.25, 1.35));
        if let Some(stack) = &mut self.inventory.slots[slot] {
            stack.item = Item::CROSSBOW;
        }
        if survival && self.inventory.wear(slot, 1) {
            self.show_popup("Crossbow broke");
        }
    }

    /// Keeps drawing while the button is held; switching away from the bow
    /// lets the string go without shooting.
    pub(super) fn update_bow(&mut self, acting: bool, dt: f64) {
        let Some(t) = self.actions.bow_draw else { return };
        let held = self.held_item();
        if !acting || !matches!(held, Some(Item::BOW | Item::CROSSBOW)) {
            self.actions.bow_draw = None;
        } else if held == Some(Item::CROSSBOW) && t + dt >= CROSSBOW_CHARGE {
            self.actions.bow_draw = None;
            self.load_crossbow();
        } else {
            self.actions.bow_draw = Some(t + dt);
        }
    }

    /// Releasing right-click looses an arrow if the bow was drawn enough.
    pub(super) fn release_bow(&mut self) {
        let Some(t) = self.actions.bow_draw.take() else { return };
        // Letting go of a crossbow before it loads just lowers it.
        let power = power(t);
        if power < MIN_POWER || self.held_item() != Some(Item::BOW) {
            return;
        }
        let survival = self.mode.is_survival();
        let enchants = self.inventory.get(self.actions.selected).map_or(Default::default(), |s| s.enchants);
        // Infinity needs one arrow but never uses it up (the shot can't be
        // picked up, like creative's).
        let infinite = enchants.has(crate::enchant::Enchantment::Infinity);
        if survival {
            let Some(slot) = self.inventory.find(Item::ARROW) else { return };
            if !infinite {
                self.inventory.take_one(slot);
            }
            if self.inventory.wear(self.actions.selected, 1) {
                self.show_popup("Bow broke");
            }
        }
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        self.mobs.entities.shoot_enchanted(eye, dir, power, survival && !infinite, enchants);
        self.audio.play(Sound::Bow, Some(eye), 0.8, (1.0 + 0.2 * (1.0 - power), 1.1 + 0.2 * (1.0 - power)));
        if survival {
            self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
        }
    }

    /// Current draw strength for the HUD and field of view, if drawing.
    pub(super) fn bow_power(&self) -> Option<f32> {
        // Loading a crossbow doesn't zoom the view like a drawn bow.
        self.actions.bow_draw.filter(|_| self.held_item() == Some(Item::BOW)).map(power)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_rises_to_full_after_a_second() {
        assert_eq!(power(0.0), 0.0);
        assert!(power(0.05) < MIN_POWER, "a tap doesn't shoot");
        assert!((power(0.5) - 0.4167).abs() < 1e-3);
        assert_eq!(power(FULL_DRAW), 1.0);
        assert_eq!(power(5.0), 1.0);
    }

    #[test]
    fn crossbows_keep_their_wear_when_loaded_and_take_java_durability() {
        assert_eq!(Item::CROSSBOW.durability(), Some(465));
        assert_eq!(Item::CHARGED_CROSSBOW.durability(), Some(465));
        assert_eq!(Item::CROSSBOW.max_stack(), 1);
        assert_eq!(Item::from_name("crossbow"), Some(Item::CROSSBOW));
        assert_ne!(Item::CROSSBOW.icon_layer(), Item::CHARGED_CROSSBOW.icon_layer());
        const { assert!(CROSSBOW_POWER > 1.0 && CROSSBOW_CHARGE == 1.25) };
    }
}
