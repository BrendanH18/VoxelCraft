//! First-person hand animation: swings, lowering and raising on item
//! switches, and the bob of walking.

use crate::item::Item;
use crate::render::hand::Hand;

/// A swing takes six ticks, like Minecraft's.
const SWING_TIME: f32 = 0.3;
/// Lowering (or raising) the hand on an item switch takes this long.
const EQUIP_TIME: f32 = 0.15;
/// Distance walked per bob stride (left to right).
const STRIDE: f32 = 1.7;

#[derive(Default)]
pub(super) struct HandAnim {
    /// Seconds into the current swing.
    swing: Option<f32>,
    /// 0 raised .. 1 lowered.
    equip: f32,
    /// The item drawn in the hand: it only changes while lowered.
    shown: Option<Item>,
    bob_phase: f32,
    bob: f32,
}

impl HandAnim {
    /// Swings the arm (breaking, hitting, placing). A swing already past
    /// halfway starts over, so holding the button keeps it going.
    pub(super) fn swing(&mut self) {
        if self.swing.is_none_or(|t| t >= SWING_TIME * 0.5) {
            self.swing = Some(0.0);
        }
    }

    /// Advances by `dt` with `held` in hand, having walked `walked` blocks
    /// on the ground.
    pub(super) fn update(&mut self, dt: f32, held: Option<Item>, walked: f32, on_ground: bool) {
        if let Some(t) = &mut self.swing {
            *t += dt;
            if *t >= SWING_TIME {
                self.swing = None;
            }
        }
        if held != self.shown {
            self.equip += dt / EQUIP_TIME;
            if self.equip >= 1.0 {
                self.equip = 1.0;
                self.shown = held;
            }
        } else {
            self.equip = (self.equip - dt / EQUIP_TIME).max(0.0);
        }
        self.bob_phase = (self.bob_phase + walked / STRIDE) % 2.0;
        let target = if on_ground { (walked / dt.max(1e-4) / 4.3).min(1.0) } else { 0.0 };
        self.bob += (target - self.bob) * (dt * 10.0).min(1.0);
    }

    /// The hand to draw; `eating` is chewing progress 0..1.
    pub(super) fn view(&self, eating: f32, sky_light: f32, block_light: f32) -> Hand {
        Hand {
            item: self.shown,
            swing: self.swing.map_or(0.0, |t| t / SWING_TIME),
            equip: self.equip,
            bob_phase: self.bob_phase,
            bob: self.bob,
            eating,
            sky_light,
            block_light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_items_lowers_then_raises_the_hand() {
        let mut h = HandAnim::default();
        let stick = Some(Item::STICK);
        h.update(0.1, stick, 0.0, true);
        assert_eq!(h.view(0.0, 1.0, 0.0).item, None, "still lowering the empty hand");
        h.update(0.1, stick, 0.0, true);
        assert_eq!((h.shown, h.equip), (stick, 1.0));
        for _ in 0..10 {
            h.update(0.05, stick, 0.0, true);
        }
        assert_eq!(h.equip, 0.0);
        h.swing();
        assert!(h.view(0.0, 1.0, 0.0).swing == 0.0);
        h.update(0.15, stick, 0.0, true);
        assert!((h.view(0.0, 1.0, 0.0).swing - 0.5).abs() < 1e-5);
        h.update(0.2, stick, 0.0, true);
        assert_eq!(h.view(0.0, 1.0, 0.0).swing, 0.0, "done");
    }
}
