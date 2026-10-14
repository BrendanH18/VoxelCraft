//! First-person hand animation: swings, lowering and raising on item
//! switches, and the bob of walking.

use crate::item::Item;
use crate::render::hand::Hand;

/// A swing takes six ticks, like Minecraft's.
const SWING_TIME: f32 = 0.3;
/// The hand's height filter runs once per game tick, as Java's does.
const TICK: f32 = 0.05;

#[derive(Default)]
pub(super) struct HandAnim {
    /// Seconds into the current swing.
    swing: Option<f32>,
    /// Java's `mainHandHeight` (1 raised .. 0 lowered) now and a tick ago,
    /// followed by the seconds into the current tick.
    height: f32,
    previous_height: f32,
    tick_time: f32,
    /// The item drawn in the hand: it only changes while lowered.
    shown: Option<Item>,
}

impl HandAnim {
    /// Swings the arm (breaking, hitting, placing). A swing already past
    /// halfway starts over, so holding the button keeps it going.
    /// Seconds into the current swing, for LAN clients to draw others' arms.
    pub(super) fn swing_time(&self) -> Option<f32> {
        self.swing
    }
    /// Mirrors a remote player's swing.
    pub(super) fn set_swing_time(&mut self, t: Option<f32>) {
        self.swing = t.filter(|t| t.is_finite() && (0.0..SWING_TIME).contains(t));
    }

    pub(super) fn swing(&mut self) {
        if self.swing.is_none_or(|t| t >= SWING_TIME * 0.5) {
            self.swing = Some(0.0);
        }
    }

    /// Advances equip/swing animation. Walking bob is shared with the camera.
    /// `swap_scale` is the session's `getItemSwapScale`: the hand dips on an
    /// item switch and after every attack or miss, rising as the charge returns.
    pub(super) fn update(&mut self, dt: f32, held: Option<Item>, swap_scale: f32) {
        if let Some(t) = &mut self.swing {
            *t += dt;
            if *t >= SWING_TIME {
                self.swing = None;
            }
        }
        // Java `FirstPersonHandsAndItems.tick`: ease toward the cubed charge,
        // or all the way down while the item shown differs, at most 0.4 a tick.
        self.tick_time += dt;
        while self.tick_time >= TICK {
            self.tick_time -= TICK;
            self.previous_height = self.height;
            let target = if held == self.shown { swap_scale * swap_scale * swap_scale } else { 0.0 };
            self.height += (target - self.height).clamp(-0.4, 0.4);
            if self.height < 0.1 {
                self.shown = held;
            }
        }
    }

    /// 0 raised .. 1 lowered, interpolated within the tick.
    fn equip(&self) -> f32 {
        1.0 - (self.previous_height + (self.height - self.previous_height) * (self.tick_time / TICK).min(1.0))
    }

    pub(super) fn appearance(
        &self,
        vitals: &crate::simulation::survival::Vitals,
        eating: f32,
        alpha: f64,
        worn: [Option<crate::inventory::Stack>; 4],
    ) -> crate::entity::model::PlayerAppearance {
        crate::entity::model::PlayerAppearance {
            held: self.shown,
            swing: self.swing.map_or(0.0, |t| t / SWING_TIME),
            using: eating,
            hurt: vitals.since_damage() < 0.5,
            death: vitals.death_time(),
            alpha: alpha as f32,
            armor: crate::entity::armor::from_stacks(&worn),
            held_icon: None,
        }
    }

    /// The hand to draw; `eating` is chewing progress 0..1.
    pub(super) fn view(&self, eating: f32, sky_light: f32, block_light: f32) -> Hand {
        Hand {
            item: self.shown,
            swing: self.swing.map_or(0.0, |t| t / SWING_TIME),
            equip: self.equip(),
            eating,
            sky_light,
            block_light,
            icon: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(h: &mut HandAnim, ticks: u32, held: Option<Item>, scale: f32) {
        for _ in 0..ticks {
            h.update(TICK, held, scale);
        }
    }

    #[test]
    fn switching_items_lowers_then_raises_the_hand() {
        let mut h = HandAnim::default();
        let stick = Some(Item::STICK);
        run(&mut h, 6, None, 1.0);
        assert_eq!((h.shown, h.height), (None, 1.0), "raised, empty");
        // 0.4 a tick down; the item changes once the hand is below 0.1.
        run(&mut h, 2, stick, 0.0);
        assert_eq!(h.view(0.0, 1.0, 0.0).item, None, "still lowering the empty hand");
        run(&mut h, 1, stick, 0.0);
        assert_eq!(h.shown, stick);
        run(&mut h, 10, stick, 1.0);
        assert_eq!((h.height, h.equip()), (1.0, 0.0));
        h.swing();
        assert!(h.view(0.0, 1.0, 0.0).swing == 0.0);
        h.update(0.15, stick, 1.0);
        assert!((h.view(0.0, 1.0, 0.0).swing - 0.5).abs() < 1e-5);
        h.update(0.2, stick, 1.0);
        assert_eq!(h.view(0.0, 1.0, 0.0).swing, 0.0, "done");
    }

    #[test]
    fn the_hand_dips_with_the_cubed_attack_charge_and_rises_again() {
        let mut h = HandAnim::default();
        let sword = Some(Item::STICK);
        run(&mut h, 10, sword, 1.0);
        assert_eq!(h.equip(), 0.0);
        // An attack restarts the swap scale at 0: the target is zero, but the
        // item stays because it is the one held.
        run(&mut h, 1, sword, 0.0);
        assert!((h.height - 0.6).abs() < 1e-6);
        run(&mut h, 3, sword, 0.0);
        assert_eq!((h.height, h.shown), (0.0, sword));
        // Half the charge is an eighth of the height; the hand eases up by at most 0.4 a tick.
        run(&mut h, 1, sword, 0.5);
        assert!((h.height - 0.125).abs() < 1e-6);
        run(&mut h, 1, sword, 1.0);
        assert!((h.height - 0.525).abs() < 1e-6);
    }
}
