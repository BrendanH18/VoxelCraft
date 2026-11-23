//! Storage screen transactions (v0.17 Track D2): clicks, shift-click quick
//! moves, number-key swaps and throws on the open storage screen of
//! whichever player acts. Storage is every chest-backed block (chests,
//! both halves of a double chest, trapped and copper chests, barrels,
//! shulker boxes, hoppers, dispensers and droppers), furnaces, smokers and
//! blast furnaces, brewing stands, chest and hopper carts (and the other
//! vehicles with storage) and the player's ender inventory.
//!
//! The desktop host, controller seats and LAN peers reach these through
//! the app's `screen::Screen`, which keeps the workstation screens;
//! headless callers and `tests/storage_transactions.rs` use them
//! directly. Java 26.3 references: `AbstractContainerMenu.doClick`
//! (`PICKUP`, `QUICK_MOVE`, `SWAP` and `THROW`), `moveItemStackTo`, and
//! `quickMoveStack` in `ChestMenu`, `ShulkerBoxMenu`, `HopperMenu`,
//! `DispenserMenu`, `AbstractFurnaceMenu` and `BrewingStandMenu`.

use glam::IVec3;

use super::PlayerSession;
use crate::entity::Entities;
use crate::inventory::{HOTBAR_SLOTS, Inventory, SLOTS, Stack, click_slot, move_into};
use crate::item::Item;
use crate::world::{World, brewing, furnace};

/// An open storage screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// A chest-backed block, read as one container (54 slots for a double
    /// chest, 5 for a hopper, 9 for a dispenser or dropper).
    Block(IVec3),
    /// A furnace, smoker or blast furnace.
    Furnace(IVec3),
    Brewing(IVec3),
    /// A chest or hopper cart, chest boat or pack animal
    /// ([`Entities::vehicle_slots`]).
    Vehicle(u32),
    /// The acting player's own ender inventory.
    Ender,
}

/// A slot of a storage screen: the player's 36 inventory slots (hotbar
/// first) or one of the storage's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageSlot {
    Inventory(usize),
    /// Slot `i` of a [`Storage::Block`], [`Storage::Vehicle`] or [`Storage::Ender`].
    Container(usize),
    FurnaceInput,
    FurnaceFuel,
    FurnaceOutput,
    /// Brewing stand bottles (left, middle, right), ingredient and fuel.
    BrewBottle(usize),
    BrewIngredient,
    BrewFuel,
}

/// Java's backwards `moveItemStackTo` over a menu's player slots: the
/// hotbar from the right, then the main grid from the bottom right.
pub const TO_PLAYER: [usize; SLOTS] = {
    let mut order = [0; SLOTS];
    let mut i = 0;
    while i < SLOTS {
        order[i] = if i < HOTBAR_SLOTS { HOTBAR_SLOTS - 1 - i } else { SLOTS - 1 - (i - HOTBAR_SLOTS) };
        i += 1;
    }
    order
};

/// The same slots forwards: the main grid from the top left, then the
/// hotbar from the left (`AbstractFurnaceMenu` for its input and fuel).
pub const TO_PLAYER_FORWARDS: [usize; SLOTS] = {
    let mut order = [0; SLOTS];
    let mut i = 0;
    while i < SLOTS {
        order[i] = if i < SLOTS - HOTBAR_SLOTS { HOTBAR_SLOTS + i } else { i - (SLOTS - HOTBAR_SLOTS) };
        i += 1;
    }
    order
};

/// A shift-clicked inventory stack no storage takes moves between the
/// hotbar and the main grid. Returns what didn't move.
pub fn between_sections(inventory: &mut Inventory, from: usize, stack: Stack) -> Option<Stack> {
    let order: Vec<usize> =
        if from < HOTBAR_SLOTS { (HOTBAR_SLOTS..SLOTS).collect() } else { (0..HOTBAR_SLOTS).collect() };
    move_into(stack, &mut inventory.slots, &order)
}

/// A number key over inventory slot `slot` on a screen without storage:
/// the two inventory stacks trade places.
pub fn swap_in_inventory(inventory: &mut Inventory, slot: usize, hotbar: usize) {
    if slot < SLOTS && hotbar < HOTBAR_SLOTS {
        inventory.slots.swap(slot, hotbar);
    }
}

/// Clicking off the screen throws the held stack (or one item of it).
/// Returns whether anything was thrown.
pub fn toss_cursor(session: &mut PlayerSession, entities: &mut Entities, one: bool) -> bool {
    let Some(stack) = session.inventory.cursor else { return false };
    let count = if one { 1 } else { stack.count };
    session.inventory.cursor = stack.with_count(stack.count - count);
    toss(session, entities, Stack { count, ..stack });
    true
}

/// Drops one item (or the whole stack) of inventory slot `slot`, as the
/// drop key does. Returns what was thrown.
pub fn drop_slot(session: &mut PlayerSession, entities: &mut Entities, slot: usize, whole: bool) -> Option<Stack> {
    let stack = session.inventory.slots[slot]?;
    let count = if whole { stack.count } else { 1 };
    session.inventory.slots[slot] = stack.with_count(stack.count - count);
    let thrown = Stack { count, ..stack };
    toss(session, entities, thrown);
    Some(thrown)
}

/// `Player.drop` from the eyes along the view, as an item entity.
pub fn toss(session: &PlayerSession, entities: &mut Entities, stack: Stack) {
    entities.throw(stack, session.player.eye(), session.player.forward().as_dvec3());
}

/// `Slot.getMaxStackSize(stack)` for a storage screen slot: one potion per
/// bottle slot and one bucket in a furnace's fuel slot
/// (`BrewingStandMenu$PotionSlot`, `FurnaceFuelSlot`), otherwise the item's
/// own limit.
pub fn slot_max(slot: StorageSlot, stack: &Stack) -> u8 {
    match slot {
        StorageSlot::BrewBottle(_) => 1,
        StorageSlot::FurnaceFuel if stack.item == Item::BUCKET => 1,
        _ => stack.max(),
    }
}

/// One player's open storage screen.
pub struct StorageScreen<'a> {
    pub session: &'a mut PlayerSession,
    pub world: &'a mut World,
    pub entities: &'a mut Entities,
    pub storage: Storage,
}

impl StorageScreen<'_> {
    /// How many [`StorageSlot::Container`] slots the storage has (furnaces
    /// and brewing stands use their named slots instead).
    pub fn container_len(&self) -> usize {
        match self.storage {
            Storage::Block(p) if self.world.chest(p).is_some() => self.world.container_slots(p),
            Storage::Vehicle(id) => self.entities.vehicle_slots(id).map_or(0, <[_]>::len),
            Storage::Ender => crate::world::chest::SLOTS,
            _ => 0,
        }
    }

    /// Every slot of this screen: the storage's, then the inventory's.
    pub fn slots(&self) -> Vec<StorageSlot> {
        let mut out: Vec<StorageSlot> = match self.storage {
            Storage::Furnace(_) => {
                vec![StorageSlot::FurnaceInput, StorageSlot::FurnaceFuel, StorageSlot::FurnaceOutput]
            }
            Storage::Brewing(_) => (0..3)
                .map(StorageSlot::BrewBottle)
                .chain([StorageSlot::BrewIngredient, StorageSlot::BrewFuel])
                .collect(),
            _ => (0..self.container_len()).map(StorageSlot::Container).collect(),
        };
        out.extend((0..SLOTS).map(StorageSlot::Inventory));
        out
    }

    /// The stack in `slot`, if the slot exists on this screen.
    pub fn get(&self, slot: StorageSlot) -> Option<Stack> {
        match (slot, self.storage) {
            (StorageSlot::Inventory(i), _) => self.session.inventory.slots.get(i).copied().flatten(),
            (StorageSlot::Container(i), Storage::Block(p)) if i < self.container_len() => self.world.chest_slot(p, i),
            (StorageSlot::Container(i), Storage::Vehicle(id)) => {
                self.entities.vehicle_slots(id).and_then(|s| s.get(i).copied().flatten())
            }
            (StorageSlot::Container(i), Storage::Ender) => self.session.ender.slots.get(i).copied().flatten(),
            (StorageSlot::FurnaceInput, Storage::Furnace(p)) => self.world.furnace(p)?.input,
            (StorageSlot::FurnaceFuel, Storage::Furnace(p)) => self.world.furnace(p)?.fuel,
            (StorageSlot::FurnaceOutput, Storage::Furnace(p)) => self.world.furnace(p)?.output,
            (StorageSlot::BrewBottle(i), Storage::Brewing(p)) => {
                self.world.brewing_stand(p)?.bottles.get(i).copied()?
            }
            (StorageSlot::BrewIngredient, Storage::Brewing(p)) => self.world.brewing_stand(p)?.ingredient,
            (StorageSlot::BrewFuel, Storage::Brewing(p)) => self.world.brewing_stand(p)?.fuel,
            _ => None,
        }
    }

    /// Writes `slot` (`Container.setItem`), waking comparators; a new
    /// furnace input restarts its cook as in `AbstractFurnaceBlockEntity.setItem`.
    fn set(&mut self, slot: StorageSlot, stack: Option<Stack>) {
        match (slot, self.storage) {
            (StorageSlot::Inventory(i), _) => self.session.inventory.slots[i] = stack,
            (StorageSlot::Container(i), Storage::Block(p)) => {
                self.world.set_chest_slot(p, i, stack);
            }
            (StorageSlot::Container(i), Storage::Vehicle(id)) => {
                if let Some(cell) = self.entities.vehicle_slots_mut(id).and_then(|s| s.get_mut(i)) {
                    *cell = stack;
                }
            }
            (StorageSlot::Container(i), Storage::Ender) => self.session.ender.slots[i] = stack,
            (StorageSlot::FurnaceInput, Storage::Furnace(p)) => {
                if let Some(f) = self.world.furnace_mut(p) {
                    f.set_input(stack);
                }
            }
            (StorageSlot::FurnaceFuel, Storage::Furnace(p)) => {
                if let Some(f) = self.world.furnace_mut(p) {
                    f.fuel = stack;
                }
            }
            (StorageSlot::FurnaceOutput, Storage::Furnace(p)) => {
                if let Some(f) = self.world.furnace_mut(p) {
                    f.output = stack;
                }
            }
            (StorageSlot::BrewBottle(i), Storage::Brewing(p)) => {
                if let Some(b) = self.world.brewing_stand_mut(p) {
                    b.bottles[i] = stack;
                }
            }
            (StorageSlot::BrewIngredient, Storage::Brewing(p)) => {
                if let Some(b) = self.world.brewing_stand_mut(p) {
                    b.ingredient = stack;
                }
            }
            (StorageSlot::BrewFuel, Storage::Brewing(p)) => {
                if let Some(b) = self.world.brewing_stand_mut(p) {
                    b.fuel = stack;
                }
            }
            _ => {}
        }
    }

    /// Whether `slot` exists on this screen.
    pub fn has(&self, slot: StorageSlot) -> bool {
        match (slot, self.storage) {
            (StorageSlot::Inventory(i), _) => i < SLOTS,
            (StorageSlot::Container(i), _) => i < self.container_len(),
            (
                StorageSlot::FurnaceInput | StorageSlot::FurnaceFuel | StorageSlot::FurnaceOutput,
                Storage::Furnace(p),
            ) => self.world.furnace(p).is_some(),
            (StorageSlot::BrewBottle(i), Storage::Brewing(p)) => i < 3 && self.world.brewing_stand(p).is_some(),
            (StorageSlot::BrewIngredient | StorageSlot::BrewFuel, Storage::Brewing(p)) => {
                self.world.brewing_stand(p).is_some()
            }
            _ => false,
        }
    }

    /// `Slot.mayPlace` for `stack` in `slot`.
    pub fn may_place(&self, slot: StorageSlot, stack: &Stack) -> bool {
        if !self.has(slot) {
            return false;
        }
        match (slot, self.storage) {
            (StorageSlot::Container(i), Storage::Vehicle(id)) => self.entities.vehicle_accepts(id, i, stack.item),
            // `ShulkerBoxSlot.mayPlace`; v0.17 Track C1: `CrafterSlot.mayPlace`.
            (StorageSlot::Container(i), Storage::Block(p)) => {
                self.world.container_accepts(p, stack) && !self.world.crafter_slot_disabled(p, i)
            }
            // `FurnaceFuelSlot.mayPlace`: fuel, or an empty bucket.
            (StorageSlot::FurnaceFuel, _) => furnace::burn_time(stack.item).is_some() || stack.item == Item::BUCKET,
            // `FurnaceResultSlot.mayPlace`.
            (StorageSlot::FurnaceOutput, _) => false,
            // `BrewingStandMenu`'s potion, ingredient and fuel slots.
            (StorageSlot::BrewBottle(_), _) => brewing::fits_bottle_slot(stack.item),
            (StorageSlot::BrewIngredient, _) => brewing::is_ingredient(stack.item),
            (StorageSlot::BrewFuel, _) => stack.item == Item::BLAZE_POWDER,
            _ => true,
        }
    }

    /// A left (or right) click on `slot` with the cursor stack (`PICKUP`).
    pub fn click(&mut self, slot: StorageSlot, right: bool) {
        match slot {
            StorageSlot::Inventory(i) if i < SLOTS => self.session.inventory.click(i, right),
            StorageSlot::Inventory(_) => {}
            StorageSlot::Container(i) => self.container_click(i, right),
            StorageSlot::FurnaceInput | StorageSlot::FurnaceFuel | StorageSlot::FurnaceOutput => {
                if let Storage::Furnace(p) = self.storage {
                    self.furnace_click(p, slot, right);
                }
            }
            StorageSlot::BrewBottle(_) | StorageSlot::BrewIngredient | StorageSlot::BrewFuel => {
                if let Storage::Brewing(p) = self.storage {
                    self.brewing_click(p, slot, right);
                }
            }
        }
    }

    fn container_click(&mut self, i: usize, right: bool) {
        let cursor = &mut self.session.inventory.cursor;
        match self.storage {
            Storage::Vehicle(id) => {
                if cursor.is_none_or(|s| self.entities.vehicle_accepts(id, i, s.item))
                    && let Some(slot) = self.entities.vehicle_slots_mut(id).and_then(|slots| slots.get_mut(i))
                {
                    click_slot(slot, cursor, right);
                }
            }
            Storage::Ender => {
                if let Some(slot) = self.session.ender.slots.get_mut(i) {
                    click_slot(slot, cursor, right);
                }
            }
            Storage::Block(pos)
                if i < self.world.container_slots(pos)
                    // v0.16 Track B: `ShulkerBoxSlot.mayPlace`.
                    && cursor.is_none_or(|c| self.world.container_accepts(pos, &c))
                    // v0.17 Track C1: `CrafterSlot.mayPlace`: nothing into a disabled slot.
                    && !self.world.crafter_slot_disabled(pos, i) =>
            {
                self.world.with_chest_slots(pos, |slots| click_slot(&mut slots[i], cursor, right));
            }
            _ => {}
        }
    }

    /// A click on a brewing stand slot: bottles take one potion or glass
    /// bottle each, the ingredient slot only brewing ingredients and the
    /// fuel slot only blaze powder (Java's slot rules).
    fn brewing_click(&mut self, pos: IVec3, slot: StorageSlot, right: bool) {
        let cursor = &mut self.session.inventory.cursor;
        let Some(b) = self.world.brewing_stand_mut(pos) else { return };
        match slot {
            StorageSlot::BrewBottle(i) if i < 3 => {
                let cell = &mut b.bottles[i];
                match cursor {
                    None => *cursor = cell.take(),
                    Some(c) if !brewing::fits_bottle_slot(c.item) => {}
                    Some(c) if cell.is_none() => {
                        // The potion keeps its components (name, contents).
                        *cell = Some(Stack { count: 1, ..*c });
                        c.count -= 1;
                        if c.count == 0 {
                            *cursor = None;
                        }
                    }
                    Some(c) if c.count == 1 => std::mem::swap(cell, cursor),
                    Some(_) => {}
                }
            }
            StorageSlot::BrewIngredient if cursor.is_none_or(|c| brewing::is_ingredient(c.item)) => {
                click_slot(&mut b.ingredient, cursor, right)
            }
            StorageSlot::BrewFuel if cursor.is_none_or(|c| c.item == Item::BLAZE_POWDER) => {
                click_slot(&mut b.fuel, cursor, right)
            }
            _ => {}
        }
    }

    /// A click on a furnace slot. Fuel only takes things that burn; the
    /// output can only be taken from (`FurnaceResultSlot`): all of it, or
    /// half with the right button, onto an empty cursor, or all of it onto
    /// a matching held stack with room for it.
    fn furnace_click(&mut self, pos: IVec3, slot: StorageSlot, right: bool) {
        let cursor = &mut self.session.inventory.cursor;
        let Some(f) = self.world.furnace_mut(pos) else { return };
        let before = f.output.map_or(0, |s| s.count);
        match slot {
            StorageSlot::FurnaceInput => f.click_input(cursor, right),
            StorageSlot::FurnaceFuel => {
                if cursor.is_none_or(|c| furnace::burn_time(c.item).is_some()) {
                    click_slot(&mut f.fuel, cursor, right)
                }
            }
            StorageSlot::FurnaceOutput => match (cursor.as_mut(), f.output) {
                (None, Some(out)) => {
                    let n = if right { out.count.div_ceil(2) } else { out.count };
                    *cursor = out.with_count(n);
                    f.output = out.with_count(out.count - n);
                }
                (Some(c), Some(out)) if c.stacks_with(&out) && c.count as u16 + out.count as u16 <= c.max() as u16 => {
                    c.count += out.count;
                    f.output = None;
                }
                _ => {}
            },
            _ => {}
        }
        self.award_furnace_xp(pos, before);
    }

    /// Taking any smelted items (the output held `before` items) releases
    /// all the experience the furnace stored, as orbs at the player, like
    /// Java's result slot.
    pub fn award_furnace_xp(&mut self, pos: IVec3, before: u8) {
        let entities = &mut *self.entities;
        let Some(xp) = self.world.furnace_mut(pos).and_then(|f| f.take_output_xp(before, || entities.roll())) else {
            return;
        };
        self.entities.spawn_xp(self.session.player.pos, xp);
    }

    /// Shift-click (`QUICK_MOVE`): the whole stack in `slot` moves to the
    /// other side, as each menu's `quickMoveStack` does.
    pub fn quick_move(&mut self, slot: StorageSlot) {
        if !self.has(slot) {
            return;
        }
        let Some(stack) = self.get(slot) else { return };
        if let StorageSlot::Inventory(i) = slot {
            self.session.inventory.slots[i] = None;
            let left = self.move_from_inventory(i, stack);
            self.session.inventory.slots[i] = left;
            return;
        }
        // `AbstractFurnaceMenu` moves its input and fuel forwards; every
        // other storage slot goes back to the player backwards.
        let order = match slot {
            StorageSlot::FurnaceInput | StorageSlot::FurnaceFuel => &TO_PLAYER_FORWARDS,
            _ => &TO_PLAYER,
        };
        let before = stack.count;
        let left = move_into(stack, &mut self.session.inventory.slots, order);
        self.set(slot, left);
        if let (StorageSlot::FurnaceOutput, Storage::Furnace(p)) = (slot, self.storage) {
            // The helper compares the restored output with its original
            // count and drains stored XP once, even for a partial move.
            self.award_furnace_xp(p, before);
        }
    }

    /// Where a shift-clicked inventory stack goes. Returns what didn't move.
    pub fn move_from_inventory(&mut self, from: usize, stack: Stack) -> Option<Stack> {
        match self.quick_move_in(stack) {
            Some(left) => left,
            None => between_sections(&mut self.session.inventory, from, stack),
        }
    }

    /// The storage's part of a shift-click from the inventory: `Some` with
    /// what is left when the storage handled it (even if nothing fitted),
    /// `None` when the stack moves between the hotbar and the main grid
    /// instead.
    pub fn quick_move_in(&mut self, stack: Stack) -> Option<Option<Stack>> {
        match self.storage {
            Storage::Vehicle(id) => {
                if self.entities.vehicle_slots(id).is_none() {
                    return Some(Some(stack));
                }
                Some(self.entities.insert_vehicle(id, stack))
            }
            Storage::Ender => {
                let order: Vec<usize> = (0..crate::world::chest::SLOTS).collect();
                Some(move_into(stack, &mut self.session.ender.slots, &order))
            }
            Storage::Block(p) => {
                // v0.16 Track B: a shulker box doesn't move into a shulker box.
                if !self.world.container_accepts(p, &stack) {
                    return Some(Some(stack));
                }
                // v0.17 Track C1: a crafter's disabled slots take nothing (`CrafterSlot.mayPlace`).
                let order: Vec<usize> =
                    (0..self.world.container_slots(p)).filter(|&i| !self.world.crafter_slot_disabled(p, i)).collect();
                Some(self.world.with_chest_slots(p, |slots| move_into(stack, slots, &order)).unwrap_or(Some(stack)))
            }
            Storage::Furnace(p) => {
                // `AbstractFurnaceMenu.quickMoveStack`: smeltables to the
                // input, else fuel to the fuel slot, else between the
                // hotbar and the main grid.
                let smeltable = self.world.furnace(p).is_some_and(|f| f.kind.smelt(stack.item).is_some());
                let fuel = furnace::burn_time(stack.item).is_some();
                let f = self.world.furnace_mut(p)?;
                if smeltable {
                    // Into an empty input slot Java calls `setItem` (a fresh
                    // cook); merging into the same item keeps progress.
                    let mut cell = f.input;
                    let left = move_into(stack, std::slice::from_mut(&mut cell), &[0]);
                    if f.input.is_none() {
                        f.set_input(cell);
                    } else {
                        f.input = cell;
                    }
                    return Some(left);
                }
                fuel.then(|| move_into(stack, std::slice::from_mut(&mut f.fuel), &[0]))
            }
            Storage::Brewing(p) => {
                // `BrewingStandMenu.quickMoveStack`: blaze powder fills the
                // fuel slot and stops there; only when the fuel slot takes
                // none does it try the ingredient slot. Other ingredients go
                // to theirs, potions and bottles one into each empty bottle
                // slot. An ingredient or bottle that doesn't fit stays put.
                let b = self.world.brewing_stand_mut(p)?;
                let mut stack = stack;
                if stack.item == Item::BLAZE_POWDER {
                    let left = move_into(stack, std::slice::from_mut(&mut b.fuel), &[0]);
                    if left != Some(stack) {
                        return Some(left);
                    }
                }
                if brewing::is_ingredient(stack.item) {
                    return Some(move_into(stack, std::slice::from_mut(&mut b.ingredient), &[0]));
                }
                if brewing::fits_bottle_slot(stack.item) {
                    for cell in b.bottles.iter_mut().filter(|c| c.is_none()) {
                        *cell = Some(Stack { count: 1, ..stack });
                        stack.count -= 1;
                        if stack.count == 0 {
                            return Some(None);
                        }
                    }
                    return Some(Some(stack));
                }
                None
            }
        }
    }

    /// The number key `hotbar` (0..9) over `slot` (`SWAP`): the hotbar
    /// stack and the slot's trade places, as far as the slot allows.
    pub fn swap(&mut self, slot: StorageSlot, hotbar: usize) {
        if hotbar >= HOTBAR_SLOTS || !self.has(slot) || slot == StorageSlot::Inventory(hotbar) {
            return;
        }
        let source = self.session.inventory.slots[hotbar];
        let target = self.get(slot);
        let output = match (slot, self.storage) {
            (StorageSlot::FurnaceOutput, Storage::Furnace(p)) => Some((p, target.map_or(0, |s| s.count))),
            _ => None,
        };
        match (source, target) {
            (None, None) => return,
            (None, Some(t)) => {
                self.session.inventory.slots[hotbar] = Some(t);
                self.set(slot, None);
            }
            (Some(s), None) => {
                if !self.may_place(slot, &s) {
                    return;
                }
                let max = slot_max(slot, &s);
                if s.count > max {
                    self.set(slot, s.with_count(max));
                    self.session.inventory.slots[hotbar] = s.with_count(s.count - max);
                } else {
                    self.session.inventory.slots[hotbar] = None;
                    self.set(slot, Some(s));
                }
            }
            (Some(s), Some(t)) => {
                if !self.may_place(slot, &s) {
                    return;
                }
                let max = slot_max(slot, &s);
                if s.count > max {
                    // The slot takes what it can; the stack it held goes
                    // back into the inventory, or is dropped.
                    self.set(slot, s.with_count(max));
                    self.session.inventory.slots[hotbar] = s.with_count(s.count - max);
                    let left = self.session.inventory.add_stack(t);
                    if left > 0 {
                        toss(self.session, self.entities, Stack { count: left, ..t });
                    }
                } else {
                    self.session.inventory.slots[hotbar] = Some(t);
                    self.set(slot, Some(s));
                }
            }
        }
        if let Some((p, before)) = output {
            self.award_furnace_xp(p, before);
        }
    }

    /// The drop key over `slot` (`THROW`), with nothing held: one item, or
    /// the whole stack. Returns what was thrown.
    pub fn throw(&mut self, slot: StorageSlot, whole: bool) -> Option<Stack> {
        if self.session.inventory.cursor.is_some() || !self.has(slot) {
            return None;
        }
        let stack = self.get(slot)?;
        let count = if whole { stack.count } else { 1 };
        self.set(slot, stack.with_count(stack.count - count));
        let thrown = Stack { count, ..stack };
        toss(self.session, self.entities, thrown);
        if let (StorageSlot::FurnaceOutput, Storage::Furnace(p)) = (slot, self.storage) {
            self.award_furnace_xp(p, stack.count);
        }
        Some(thrown)
    }
}
