//! The player's bow: hold right-click to draw, release to shoot an arrow.

use crate::audio::sounds::Sound;
use crate::item::Item;
use crate::world::block::Block;

use super::{Game, GameMode};

/// Seconds to draw the bow fully (Minecraft's 20 ticks).
pub(super) const FULL_DRAW: f64 = 1.0;
/// Weaker draws than this don't shoot.
const MIN_POWER: f32 = 0.1;

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
                || crate::world::furnace::is_furnace(b)
                || crate::world::chest::is_chest(b)
        })
    }

    /// Right-click with a bow: starts drawing it if there's an arrow to
    /// shoot (creative needs none). Returns whether it did.
    pub(super) fn start_draw(&mut self) -> bool {
        let has_arrow = self.mode == GameMode::Creative || self.inventory.find(Item::ARROW).is_some();
        if self.held_item() != Some(Item::BOW) || !has_arrow || self.aiming_at_usable() {
            return false;
        }
        self.actions.bow_draw = Some(0.0);
        true
    }

    /// Keeps drawing while the button is held; switching away from the bow
    /// lets the string go without shooting.
    pub(super) fn update_bow(&mut self, acting: bool, dt: f64) {
        let Some(t) = self.actions.bow_draw else { return };
        if !acting || self.held_item() != Some(Item::BOW) {
            self.actions.bow_draw = None;
        } else {
            self.actions.bow_draw = Some(t + dt);
        }
    }

    /// Releasing right-click looses an arrow if the bow was drawn enough.
    pub(super) fn release_bow(&mut self) {
        let Some(t) = self.actions.bow_draw.take() else { return };
        let power = power(t);
        if power < MIN_POWER || self.held_item() != Some(Item::BOW) {
            return;
        }
        let survival = self.mode == GameMode::Survival;
        if survival {
            let Some(slot) = self.inventory.find(Item::ARROW) else { return };
            self.inventory.take_one(slot);
            if self.inventory.wear(self.actions.selected, 1) {
                self.show_popup("Bow broke");
            }
        }
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        self.mobs.entities.shoot_arrow(eye, dir, power, survival);
        self.audio.play(Sound::Bow, None, 0.8, (1.0 + 0.2 * (1.0 - power), 1.1 + 0.2 * (1.0 - power)));
        if survival {
            self.vitals.hunger.exhaust(super::survival::EXHAUST_ATTACK);
        }
    }

    /// Current draw strength for the HUD and field of view, if drawing.
    pub(super) fn bow_power(&self) -> Option<f32> {
        self.actions.bow_draw.map(power)
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
}
