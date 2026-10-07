//! Smithing table screen: upgrade template + diamond base + Netherite
//! ingot -> matching Netherite gear.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::inventory::{Stack, move_into};

use super::hud::SlotRef;
use super::{Container, Game};

impl Game {
    pub(super) fn open_smithing(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.container = Container::Smithing(pos);
        self.toggle_inventory();
    }

    pub(super) fn smithing_result(&self) -> Option<Stack> {
        crate::smithing::upgrade(self.work[0]?, self.work[1]?, self.work[2]?)
    }

    pub(super) fn smithing_click(&mut self, slot: SlotRef, right: bool) {
        let input = match slot {
            SlotRef::SmithTemplate => Some((0, crate::smithing::is_template as fn(_) -> _)),
            SlotRef::SmithBase => Some((1, crate::smithing::is_base as fn(_) -> _)),
            SlotRef::SmithAddition => Some((2, crate::smithing::is_addition as fn(_) -> _)),
            _ => None,
        };
        if let Some((i, accepts)) = input {
            if self.inventory.cursor.is_none_or(|s| accepts(s.item)) {
                crate::inventory::click_slot(&mut self.work[i], &mut self.inventory.cursor, right);
            }
        } else if slot == SlotRef::SmithResult
            && self.inventory.cursor.is_none()
            && let Some(out) = self.take_smithing_result()
        {
            self.inventory.cursor = Some(out);
        }
    }

    /// Consumes one template, base and addition, returning the copied base
    /// metadata on the Netherite result.
    pub(super) fn take_smithing_result(&mut self) -> Option<Stack> {
        let Container::Smithing(pos) = self.container else { return None };
        let result = self.smithing_result()?;
        for input in &mut self.work {
            let stack = input.as_mut().expect("a smithing result has all inputs");
            stack.count -= 1;
            if stack.count == 0 {
                *input = None;
            }
        }
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        self.audio.play(Sound::Place(Material::Wood), Some(at), 0.8, (1.3, 1.5));
        Some(result)
    }

    pub(super) fn quick_take_smithing(&mut self) {
        let Some(result) = self.smithing_result() else { return };
        let mut slots = self.inventory.slots;
        if move_into(result, &mut slots, &(0..crate::inventory::SLOTS).collect::<Vec<_>>()).is_none()
            && let Some(out) = self.take_smithing_result()
        {
            self.inventory.add_stack(out);
        }
    }

    /// Java routes a shift-clicked ingredient to its first matching empty
    /// input slot.
    pub(super) fn move_to_smithing(&mut self, stack: Stack) -> Option<Stack> {
        let slot = [
            crate::smithing::is_template(stack.item),
            crate::smithing::is_base(stack.item),
            crate::smithing::is_addition(stack.item),
        ]
        .into_iter()
        .enumerate()
        .find_map(|(i, accepts)| (accepts && self.work[i].is_none()).then_some(i));
        let Some(i) = slot else { return Some(stack) };
        move_into(stack, std::slice::from_mut(&mut self.work[i]), &[0])
    }
}
