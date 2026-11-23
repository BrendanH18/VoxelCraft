//! v0.17 Track C1: the crafter's sounds and its screen (Java's
//! `CrafterScreen` over `CrafterMenu`). The nine slots are the chest
//! screen's `SlotRef::Chest` slots, so clicks, quick moves, the controller
//! menu and LAN go through the shared screen transactions
//! (`screen::Screen`); this module adds the slot toggles' overlay, the
//! powered indicator and the result preview.

use glam::{DVec3, IVec3};

use super::Game;
use crate::audio::Sound;
use crate::render::ui::Ui;
use crate::world::World;
use crate::world::crafter;

const SLOT: f32 = 18.0;

/// `CrafterMenu`'s grid, relative to the panel's slot origin (Java's 26, 17).
pub(super) fn grid_slot(i: usize) -> (f32, f32) {
    (25.0 + (i % 3) as f32 * SLOT, 18.0 + (i / 3) as f32 * SLOT)
}

/// The `NonInteractiveResultSlot` (Java's 134, 35).
pub(super) const RESULT: (f32, f32) = (133.0, 36.0);

/// Whether the open chest screen at `p` is a crafter's.
pub(super) fn is_crafter(world: &World, p: IVec3) -> bool {
    world.get_block(p).is_some_and(|b| crafter::owns(b.0))
}

/// A disabled slot's cover (`CrafterScreen`'s `disabled_slot` sprite,
/// drawn as original hatching).
pub(super) fn draw_disabled(ui: &mut Ui, x: f32, y: f32, size: f32) {
    ui.rect(x + 1.0, y + 1.0, size - 2.0, size - 2.0, [0.33, 0.31, 0.31, 1.0]);
    let n = (size - 2.0) as i32;
    for k in (0..n).step_by(3) {
        let k = k as f32;
        ui.rect(x + 1.0 + k, y + 1.0 + k, 2.0, 2.0, [0.55, 0.2, 0.2, 1.0]);
        ui.rect(x + size - 3.0 - k, y + 1.0 + k, 2.0, 2.0, [0.55, 0.2, 0.2, 1.0]);
    }
}

/// The arrow toward the result and the redstone indicator beneath it,
/// lit while the crafter is triggered (`CrafterMenu.isPowered`).
pub(super) fn draw_indicator(ui: &mut Ui, x: f32, y: f32, powered: bool) {
    let arrow = [0.45, 0.45, 0.45, 1.0];
    for i in 0..12 {
        ui.rect(x + i as f32, y + 3.0, 1.0, 2.0, arrow);
    }
    for i in 0..5 {
        ui.rect(x + 12.0 + i as f32, y + i as f32 - 1.0, 1.0, 10.0 - 2.0 * i as f32, arrow);
    }
    let dust = if powered { [0.95, 0.12, 0.08, 1.0] } else { [0.35, 0.08, 0.06, 1.0] };
    ui.rect(x + 5.0, y + 14.0, 6.0, 4.0, dust);
    ui.rect(x + 7.0, y + 12.0, 2.0, 8.0, dust);
}

impl Game {
    /// Plays what crafters did this tick (`CRAFTER_CRAFT` and `CRAFTER_FAIL`
    /// at volume 1, pitch 1); the craft's smoke is a world particle request.
    pub(super) fn play_crafter_events(&mut self) {
        for e in self.world.take_crafter_events() {
            let at = e.pos.as_dvec3() + DVec3::splat(0.5);
            self.audio.play(Sound::Crafter(e.cue), Some(at), 1.0, (1.0, 1.0));
        }
    }

    /// The crafter screen's own parts, over the shared chest slots: disabled
    /// slot covers, the indicator and the result preview. `px`, `py` are the
    /// panel's slot origin.
    pub(super) fn crafter_ui(&self, ui: &mut Ui, p: IVec3, px: f32, py: f32) {
        for i in 0..crafter::SLOTS {
            if self.world.crafter_slot_disabled(p, i) {
                let (x, y) = grid_slot(i);
                draw_disabled(ui, px + x, py + y, SLOT);
            }
        }
        let powered = self.world.get_block(p).and_then(crafter::decode).is_some_and(|s| s.triggered);
        draw_indicator(ui, px + 97.0, py + 41.0, powered);
        let (rx, ry) = (px + RESULT.0, py + RESULT.1);
        ui.rect(rx - 4.0, ry - 4.0, SLOT + 8.0, SLOT + 8.0, [0.55, 0.55, 0.55, 1.0]);
        ui.rect(rx, ry, SLOT, SLOT, [0.45, 0.45, 0.45, 1.0]);
        if let Some(stack) = self.world.crafter_result(p) {
            self.stack_ui(ui, rx, ry, stack);
        }
    }
}
