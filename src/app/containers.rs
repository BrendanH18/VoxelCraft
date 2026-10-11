//! Chest screens and shift-click quick moves between the open container
//! and the inventory.

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};
use crate::inventory::{HOTBAR_SLOTS, SLOTS, Stack, move_into};
use crate::item::Item;
use crate::world::{brewing, furnace};

use super::hud::SlotRef;
use super::{Container, Game};

/// Where stacks leaving a container land in the inventory: the hotbar from
/// the right, then the main grid from the bottom right, like Minecraft.
fn to_player_order() -> Vec<usize> {
    (0..HOTBAR_SLOTS).rev().chain((HOTBAR_SLOTS..SLOTS).rev()).collect()
}

impl Game {
    pub(super) fn open_container_count(&self) -> usize {
        match self.container {
            Container::Chest(p) => self.world.container_slots(p),
            Container::Minecart(id) => self.mobs.entities.vehicle_slots(id).map_or(0, |s| s.len()),
            _ => 0,
        }
    }

    /// Right-click on a chest: its 27 slots above the inventory.
    pub(super) fn open_chest(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.chest(pos).is_none() {
            return;
        }
        self.container = Container::Chest(pos);
        self.toggle_inventory();
        self.chest_sound(pos, 0.65);
        if self.world.get_block(pos).is_some_and(crate::entity::nether::guarded_by_piglins) {
            self.piglins_notice(true);
        }
    }

    /// Java's `angerNearbyPiglins`: piglins around the player turn on them
    /// for opening a chest (only the ones that see it) or breaking gold.
    pub(super) fn piglins_notice(&mut self, only_if_seen: bool) {
        let (actor, at) = (self.actor, self.player.pos);
        self.mobs.entities.piglins_notice(actor, at, only_if_seen, &self.world);
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
                if let Container::Minecart(id) = self.container {
                    let stack = self
                        .mobs
                        .entities
                        .vehicle_slots_mut(id)
                        .and_then(|slots| slots.get_mut(i))
                        .and_then(Option::take);
                    if let Some(stack) = stack {
                        let left = self.move_to_player(stack);
                        if let Some(slot) = self.mobs.entities.vehicle_slots_mut(id).and_then(|slots| slots.get_mut(i))
                        {
                            *slot = left;
                        }
                    }
                    return;
                }
                let Container::Chest(p) = self.container else { return };
                if i >= self.world.container_slots(p) {
                    return;
                }
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
            SlotRef::BrewBottle(_) | SlotRef::BrewIngredient | SlotRef::BrewFuel => {
                let Container::Brewing(p) = self.container else { return };
                let Some(b) = self.world.brewing_stand_mut(p) else { return };
                let cell = match slot {
                    SlotRef::BrewBottle(i) => &mut b.bottles[i],
                    SlotRef::BrewIngredient => &mut b.ingredient,
                    _ => &mut b.fuel,
                };
                let Some(stack) = cell.take() else { return };
                let left = self.move_to_player(stack);
                if let Some(b) = self.world.brewing_stand_mut(p) {
                    match slot {
                        SlotRef::BrewBottle(i) => b.bottles[i] = left,
                        SlotRef::BrewIngredient => b.ingredient = left,
                        _ => b.fuel = left,
                    }
                }
            }
            SlotRef::EnchantItem | SlotRef::EnchantLapis => {
                let i = (slot == SlotRef::EnchantLapis) as usize;
                let Some(stack) = self.work[i].take() else { return };
                self.work[i] = self.move_to_player(stack);
            }
            SlotRef::EnchantOffer(i) => self.take_offer(i),
            SlotRef::AnvilLeft | SlotRef::AnvilRight => {
                let i = (slot == SlotRef::AnvilRight) as usize;
                let Some(stack) = self.work[i].take() else { return };
                self.work[i] = self.move_to_player(stack);
            }
            SlotRef::AnvilResult => self.quick_take_anvil(),
            SlotRef::SmithTemplate | SlotRef::SmithBase | SlotRef::SmithAddition => {
                let i = match slot {
                    SlotRef::SmithTemplate => 0,
                    SlotRef::SmithBase => 1,
                    _ => 2,
                };
                let Some(stack) = self.work[i].take() else { return };
                self.work[i] = self.move_to_player(stack);
            }
            SlotRef::SmithResult => self.quick_take_smithing(),
            SlotRef::Armor(piece) => {
                if !self.inventory.can_unequip(piece, self.mode.is_creative()) {
                    return;
                }
                let Some(stack) = self.inventory.armor[piece as usize].take() else { return };
                self.inventory.armor[piece as usize] = self.move_to_player(stack);
            }
            SlotRef::Trade(i) => self.buy_trade(i),
            SlotRef::Palette(item) => {
                if self.mode.is_creative() {
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
            Container::Minecart(id) => {
                if self.mobs.entities.vehicle_slots(id).is_none() {
                    return Some(stack);
                }
                return self.mobs.entities.insert_vehicle(id, stack);
            }
            Container::Chest(p) => {
                let order: Vec<usize> = (0..self.world.container_slots(p)).collect();
                return match self.world.chest_mut(p) {
                    Some(c) => move_into(stack, &mut c.slots, &order),
                    None => Some(stack),
                };
            }
            Container::Furnace(p) => {
                let smeltable = self.world.furnace(p).is_some_and(|f| f.kind.smelt(stack.item).is_some());
                let fuel = furnace::burn_time(stack.item).is_some();
                if let Some(f) = self.world.furnace_mut(p)
                    && (smeltable || fuel)
                {
                    let cell = if smeltable { &mut f.input } else { &mut f.fuel };
                    return move_into(stack, std::slice::from_mut(cell), &[0]);
                }
            }
            Container::Brewing(p) => {
                // Java: bottles into empty bottle slots one each, blaze
                // powder into the fuel then the ingredient slot, other
                // ingredients into theirs.
                if let Some(b) = self.world.brewing_stand_mut(p) {
                    let mut stack = stack;
                    if brewing::fits_bottle_slot(stack.item) {
                        let before = stack.count;
                        for cell in b.bottles.iter_mut().filter(|c| c.is_none()) {
                            *cell = Some(Stack::new(stack.item, 1));
                            stack.count -= 1;
                            if stack.count == 0 {
                                return None;
                            }
                        }
                        if stack.count != before {
                            return Some(stack);
                        }
                    }
                    let mut left = Some(stack);
                    if stack.item == Item::BLAZE_POWDER {
                        left = move_into(stack, std::slice::from_mut(&mut b.fuel), &[0]);
                    }
                    if let Some(rest) = left.filter(|s| brewing::is_ingredient(s.item)) {
                        left = move_into(rest, std::slice::from_mut(&mut b.ingredient), &[0]);
                    }
                    if left != Some(stack) {
                        return left;
                    }
                }
            }
            Container::Enchanting(_) => {
                let left = self.move_to_table(stack);
                if left != Some(stack) {
                    return left;
                }
            }
            Container::Anvil(_) | Container::Grindstone(_) => {
                let left = self.move_to_anvil(stack);
                if left != Some(stack) {
                    return left;
                }
            }
            Container::Smithing(_) => {
                let left = self.move_to_smithing(stack);
                if left != Some(stack) {
                    return left;
                }
            }
            Container::Inventory if self.mode.is_survival() => {
                // Armor goes on, if that slot is free.
                if let Some((piece, _)) = stack.item.as_armor()
                    && self.inventory.armor[piece as usize].is_none()
                {
                    self.inventory.armor[piece as usize] = Some(stack);
                    return None;
                }
            }
            Container::Inventory | Container::CraftingTable | Container::Trading(_) => {}
        }
        let order: Vec<usize> =
            if from < HOTBAR_SLOTS { (HOTBAR_SLOTS..SLOTS).collect() } else { (0..HOTBAR_SLOTS).collect() };
        move_into(stack, &mut self.inventory.slots, &order)
    }
}
