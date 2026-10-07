//! The anvil screen, as Java has it (without renaming): two inputs and a
//! result that repairs gear with its material, merges two of the same item
//! or adds an enchanted book's enchantments, for levels (see
//! `enchant::anvil`). Each use may chip the anvil.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::enchant::{self, AnvilResult};
use crate::inventory::{Stack, move_into};

use super::hud::SlotRef;
use super::{Container, Game};

impl Game {
    /// Right-click on an anvil.
    pub(super) fn open_anvil(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.container = Container::Anvil(pos);
        self.toggle_inventory();
    }

    /// What the inputs make, ignoring the survival cost limit, and whether
    /// it's "Too Expensive!" for this player.
    pub(super) fn anvil_preview(&self) -> Option<(AnvilResult, bool)> {
        let creative = self.mode.is_creative();
        let result = enchant::anvil_any_cost(self.work[0]?, self.work[1], creative)?;
        Some((result, !creative && result.cost >= enchant::TOO_EXPENSIVE))
    }

    /// The result a click can take now: affordable and not too expensive.
    pub(super) fn anvil_result(&self) -> Option<AnvilResult> {
        let (result, too_expensive) = self.anvil_preview()?;
        let affordable = self.mode.is_creative() || self.vitals.xp.level >= result.cost;
        (!too_expensive && affordable).then_some(result)
    }

    /// A click on an anvil slot: the inputs take anything; the result goes
    /// to an empty cursor.
    pub(super) fn anvil_click(&mut self, slot: SlotRef, right: bool) {
        match slot {
            SlotRef::AnvilLeft | SlotRef::AnvilRight => {
                let i = (slot == SlotRef::AnvilRight) as usize;
                crate::inventory::click_slot(&mut self.work[i], &mut self.inventory.cursor, right);
            }
            SlotRef::AnvilResult if self.inventory.cursor.is_none() => {
                if let Some(out) = self.take_anvil_result() {
                    self.inventory.cursor = Some(out);
                }
            }
            _ => {}
        }
    }

    /// Java's `AnvilMenu.onTake`: pays the levels, uses up the inputs and
    /// maybe chips the anvil. Returns the result.
    pub(super) fn take_anvil_result(&mut self) -> Option<Stack> {
        let Container::Anvil(pos) = self.container else { return None };
        let result = self.anvil_result()?;
        let survival = self.mode.is_survival();
        if survival {
            self.vitals.xp.add_levels(-(result.cost as i64));
        }
        self.work[0] = None;
        self.work[1] = match (result.uses, self.work[1]) {
            (Some(n), Some(s)) if s.count > n => Some(Stack { count: s.count - n, ..s }),
            _ => None,
        };
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        let broke = if survival { enchant::wear_anvil(&mut self.world, pos) } else { None };
        // A broken anvil's screen closes on the next update (see
        // `Game::update_items`), returning what's held.
        if broke == Some(true) {
            self.audio.play(Sound::Break(Material::Stone), Some(at), 1.0, (0.5, 0.6));
        } else {
            self.audio.play(Sound::Place(Material::Stone), Some(at), 0.9, (1.6, 1.8));
        }
        Some(result.output)
    }

    /// Shift-click on the result: into the inventory, if it fits.
    pub(super) fn quick_take_anvil(&mut self) {
        let Some(result) = self.anvil_result() else { return };
        let mut slots = self.inventory.slots;
        if move_into(result.output, &mut slots, &(0..crate::inventory::SLOTS).collect::<Vec<_>>()).is_none()
            && let Some(out) = self.take_anvil_result()
        {
            self.inventory.add_stack(out);
        }
    }

    /// Shift-click from the inventory onto the anvil: one item into the left
    /// input if it's empty, else the right. Returns what didn't move.
    pub(super) fn move_to_anvil(&mut self, stack: Stack) -> Option<Stack> {
        if self.work[0].is_none() {
            self.work[0] = Some(Stack { count: 1, ..stack });
            return (stack.count > 1).then_some(Stack { count: stack.count - 1, ..stack });
        }
        move_into(stack, std::slice::from_mut(&mut self.work[1]), &[0])
    }
}
