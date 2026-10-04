//! Chest screens and shift-click quick moves between the open container
//! and the inventory.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::inventory::{HOTBAR_SLOTS, SLOTS, Stack, move_into};
use crate::world::{chest, furnace};

use super::hud::SlotRef;
use super::{Container, Game, GameMode};

/// Where stacks leaving a container land in the inventory: the hotbar from
/// the right, then the main grid from the bottom right, like Minecraft.
fn to_player_order() -> Vec<usize> {
    (0..HOTBAR_SLOTS).rev().chain((HOTBAR_SLOTS..SLOTS).rev()).collect()
}

impl Game {
    /// Right-click on a chest: its 27 slots above the inventory.
    pub(super) fn open_chest(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.chest(pos).is_none() {
            return;
        }
        self.container = Container::Chest(pos);
        self.toggle_inventory();
        self.chest_sound(pos, 0.65);
    }

    pub(super) fn chest_sound(&mut self, pos: IVec3, pitch: f32) {
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        self.audio.play(Sound::Place(Material::Wood), Some(at), 0.7, (pitch, pitch + 0.08));
    }

    /// Shift-click: moves the whole stack in `slot` to the other side (the
    /// open container and the inventory, or the hotbar and the main grid);
    /// on a crafting result, crafts as many as fit.
    pub(super) fn quick_move(&mut self, slot: SlotRef) {
        match slot {
            SlotRef::Inventory(i) => {
                let Some(stack) = self.inventory.slots[i].take() else { return };
                let left = self.move_from_inventory(i, stack);
                self.inventory.slots[i] = left;
            }
            SlotRef::Chest(i) => {
                let Container::Chest(p) = self.container else { return };
                let Some(stack) = self.world.chest_mut(p).and_then(|c| c.slots[i].take()) else { return };
                let left = self.move_to_player(stack);
                if let Some(c) = self.world.chest_mut(p) {
                    c.slots[i] = left;
                }
            }
            SlotRef::Craft(i) => {
                let Some(stack) = self.craft.cells[i].take() else { return };
                self.craft.cells[i] = self.move_to_player(stack);
            }
            SlotRef::CraftResult => {
                // Craft until the ingredients run out or the result won't fit.
                let order = to_player_order();
                while let Some(result) = self.craft.result() {
                    let mut slots = self.inventory.slots;
                    if move_into(result, &mut slots, &order).is_some() {
                        break;
                    }
                    self.inventory.slots = slots;
                    self.craft.consume();
                }
            }
            SlotRef::FurnaceInput | SlotRef::FurnaceFuel | SlotRef::FurnaceOutput => {
                let Container::Furnace(p) = self.container else { return };
                let Some(f) = self.world.furnace_mut(p) else { return };
                let cell = match slot {
                    SlotRef::FurnaceInput => &mut f.input,
                    SlotRef::FurnaceFuel => &mut f.fuel,
                    _ => &mut f.output,
                };
                let Some(stack) = cell.take() else { return };
                let left = self.move_to_player(stack);
                if let Some(f) = self.world.furnace_mut(p) {
                    match slot {
                        SlotRef::FurnaceInput => f.input = left,
                        SlotRef::FurnaceFuel => f.fuel = left,
                        _ => f.output = left,
                    }
                }
                if slot == SlotRef::FurnaceOutput {
                    // The helper compares the restored output with its original
                    // count and drains stored XP once, even for a partial move.
                    self.award_furnace_xp(p, stack.count);
                }
            }
            SlotRef::Armor(piece) => {
                let Some(stack) = self.inventory.armor[piece as usize].take() else { return };
                self.inventory.armor[piece as usize] = self.move_to_player(stack);
            }
            SlotRef::Palette(item) => {
                if self.mode == GameMode::Creative {
                    self.inventory.add(item, item.max_stack());
                }
            }
        }
    }

    fn move_to_player(&mut self, stack: Stack) -> Option<Stack> {
        move_into(stack, &mut self.inventory.slots, &to_player_order())
    }

    /// Where a shift-clicked inventory stack goes: into the open chest, a
    /// furnace's input (smeltables) or fuel slot, or else between the hotbar
    /// and the main grid. Returns what didn't move.
    fn move_from_inventory(&mut self, from: usize, stack: Stack) -> Option<Stack> {
        match self.container {
            Container::Chest(p) => {
                let order: Vec<usize> = (0..chest::SLOTS).collect();
                return match self.world.chest_mut(p) {
                    Some(c) => move_into(stack, &mut c.slots, &order),
                    None => Some(stack),
                };
            }
            Container::Furnace(p) => {
                let smeltable = furnace::smelt(stack.item).is_some();
                let fuel = furnace::burn_time(stack.item).is_some();
                if let Some(f) = self.world.furnace_mut(p)
                    && (smeltable || fuel)
                {
                    let cell = if smeltable { &mut f.input } else { &mut f.fuel };
                    return move_into(stack, std::slice::from_mut(cell), &[0]);
                }
            }
            Container::Inventory if self.mode == GameMode::Survival => {
                // Armor goes on, if that slot is free.
                if let Some((piece, _)) = stack.item.as_armor()
                    && self.inventory.armor[piece as usize].is_none()
                {
                    self.inventory.armor[piece as usize] = Some(stack);
                    return None;
                }
            }
            Container::Inventory | Container::CraftingTable => {}
        }
        let order: Vec<usize> =
            if from < HOTBAR_SLOTS { (HOTBAR_SLOTS..SLOTS).collect() } else { (0..HOTBAR_SLOTS).collect() };
        move_into(stack, &mut self.inventory.slots, &order)
    }
}
