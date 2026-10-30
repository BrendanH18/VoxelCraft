//! Container screen transactions for whichever player clicks: the host,
//! a controller seat or a LAN peer. A [`Screen`] borrows that player's
//! session, open container and crafting grid with the shared world, so no
//! player's state is swapped into another's.

use glam::{DVec3, IVec3};

use crate::audio::{
    Audio, SoundSource,
    sounds::{Material, Sound},
};
use crate::crafting::Grid;
use crate::enchant::{self, AnvilResult, Offer};
use crate::entity::Entities;
use crate::inventory::{HOTBAR_SLOTS, SLOTS, Stack, move_into};
use crate::item::Item;
use crate::world::World;
use crate::world::{brewing, furnace};
use voxelcraft::session::PlayerSession;

use super::Container;
use super::hud::SlotRef;

/// Something the clicking player's adapter must do.
#[derive(Debug, PartialEq)]
pub(super) enum Request {
    Popup(String),
}

pub(super) struct Screen<'a> {
    pub session: &'a mut PlayerSession,
    pub container: &'a mut Container,
    pub craft: &'a mut Grid,
    pub world: &'a mut World,
    pub entities: &'a mut Entities,
    pub audio: &'a mut Audio,
    pub requests: Vec<Request>,
}

/// Where stacks leaving a container land in the inventory: the hotbar from
/// the right, then the main grid from the bottom right, like Minecraft.
fn to_player_order() -> Vec<usize> {
    (0..HOTBAR_SLOTS).rev().chain((HOTBAR_SLOTS..SLOTS).rev()).collect()
}

/// What the anvil or grindstone inputs make, ignoring the survival cost
/// limit, and whether it's "Too Expensive!" for this player.
pub(super) fn anvil_preview(container: Container, session: &PlayerSession) -> Option<(AnvilResult, bool)> {
    if matches!(container, Container::Grindstone(_)) {
        return voxelcraft::grindstone::result(session.work[0], session.work[1]).map(|r| (r, false));
    }
    let creative = session.mode.is_creative();
    let result = enchant::anvil_any_cost(session.work[0]?, session.work[1], creative)?;
    Some((result, !creative && result.cost >= enchant::TOO_EXPENSIVE))
}

/// The anvil result a click can take now: affordable and not too expensive.
pub(super) fn anvil_result(container: Container, session: &PlayerSession) -> Option<AnvilResult> {
    let (result, too_expensive) = anvil_preview(container, session)?;
    let affordable = session.mode.is_creative() || session.vitals.xp.level >= result.cost;
    (!too_expensive && affordable).then_some(result)
}

pub(super) fn smithing_result(session: &PlayerSession) -> Option<Stack> {
    crate::smithing::upgrade(session.work[0]?, session.work[1]?, session.work[2]?)
}

/// The three enchanting offers for what's in the item slot.
pub(super) fn enchant_offers(container: Container, session: &PlayerSession, world: &World) -> [Offer; 3] {
    let Container::Enchanting(pos) = container else { return Default::default() };
    match session.work[0] {
        Some(stack) if enchant::table_accepts(stack) => {
            enchant::offers(session.vitals.xp.seed, stack.item, enchant::bookshelves(world, pos))
        }
        _ => Default::default(),
    }
}

/// Whether offer `i` can be taken now: enough levels for its cost and lapis
/// for its number (Creative needs neither).
pub(super) fn can_take_offer(session: &PlayerSession, i: usize, offer: Offer) -> bool {
    let creative = session.mode.is_creative();
    let lapis = session.work[1].map_or(0, |s| s.count) as usize;
    let level = session.vitals.xp.level;
    offer.cost > 0 && (creative || (lapis > i && level >= offer.cost && level > i as u32))
}

impl Screen<'_> {
    /// Shift-click: moves the whole stack in `slot` to the other side (the
    /// open container and the inventory, or the hotbar and the main grid);
    /// on a crafting result, crafts as many as fit.
    pub(super) fn quick_move(&mut self, slot: SlotRef) {
        match slot {
            SlotRef::Inventory(i) => {
                let Some(stack) = self.session.inventory.slots[i].take() else { return };
                let left = self.move_from_inventory(i, stack);
                self.session.inventory.slots[i] = left;
            }
            SlotRef::Chest(i) => {
                if let Container::Minecart(id) = *self.container {
                    let stack =
                        self.entities.vehicle_slots_mut(id).and_then(|slots| slots.get_mut(i)).and_then(Option::take);
                    if let Some(stack) = stack {
                        let left = self.move_to_player(stack);
                        if let Some(slot) = self.entities.vehicle_slots_mut(id).and_then(|slots| slots.get_mut(i)) {
                            *slot = left;
                        }
                    }
                    return;
                }
                if let Container::EnderChest(_) = *self.container {
                    let Some(stack) = self.session.ender.slots.get_mut(i).and_then(Option::take) else { return };
                    self.session.ender.slots[i] = self.move_to_player(stack);
                    return;
                }
                let Container::Chest(p) = *self.container else { return };
                if i >= self.world.container_slots(p) {
                    return;
                }
                let Some(stack) = self.world.chest_slot(p, i) else { return };
                self.world.set_chest_slot(p, i, None);
                let left = self.move_to_player(stack);
                self.world.set_chest_slot(p, i, left);
            }
            SlotRef::Craft(i) => {
                let Some(stack) = self.craft.cells[i].take() else { return };
                self.craft.cells[i] = self.move_to_player(stack);
            }
            SlotRef::CraftResult => {
                // Craft until the ingredients run out or the result won't fit.
                let order = to_player_order();
                while let Some(result) = self.craft.result() {
                    let mut slots = self.session.inventory.slots;
                    if move_into(result, &mut slots, &order).is_some() {
                        break;
                    }
                    self.session.inventory.slots = slots;
                    self.consume_craft();
                }
            }
            SlotRef::FurnaceInput | SlotRef::FurnaceFuel | SlotRef::FurnaceOutput => {
                let Container::Furnace(p) = *self.container else { return };
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
                let Container::Brewing(p) = *self.container else { return };
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
                let Some(stack) = self.session.work[i].take() else { return };
                self.session.work[i] = self.move_to_player(stack);
            }
            SlotRef::EnchantOffer(i) => self.take_offer(i),
            SlotRef::AnvilLeft | SlotRef::AnvilRight => {
                let i = (slot == SlotRef::AnvilRight) as usize;
                let Some(stack) = self.session.work[i].take() else { return };
                self.session.work[i] = self.move_to_player(stack);
            }
            SlotRef::AnvilResult => self.quick_take_anvil(),
            SlotRef::SmithTemplate | SlotRef::SmithBase | SlotRef::SmithAddition => {
                let i = match slot {
                    SlotRef::SmithTemplate => 0,
                    SlotRef::SmithBase => 1,
                    _ => 2,
                };
                let Some(stack) = self.session.work[i].take() else { return };
                self.session.work[i] = self.move_to_player(stack);
            }
            SlotRef::SmithResult => self.quick_take_smithing(),
            SlotRef::Offhand => {
                let Some(stack) = self.session.inventory.offhand.take() else { return };
                self.session.inventory.offhand = self.move_to_player(stack);
            }
            SlotRef::Armor(piece) => {
                if !self.session.inventory.can_unequip(piece, self.session.mode.is_creative()) {
                    return;
                }
                let Some(stack) = self.session.inventory.armor[piece as usize].take() else { return };
                self.session.inventory.armor[piece as usize] = self.move_to_player(stack);
            }
            SlotRef::Trade(i) => self.buy_trade(i),
            SlotRef::Palette(item) => {
                if self.session.mode.is_creative() {
                    self.session.inventory.add(item, item.max_stack());
                }
            }
        }
    }

    pub(super) fn move_to_player(&mut self, stack: Stack) -> Option<Stack> {
        move_into(stack, &mut self.session.inventory.slots, &to_player_order())
    }

    /// Where a shift-clicked inventory stack goes: into the open chest, a
    /// furnace's input (smeltables) or fuel slot, or else between the hotbar
    /// and the main grid. Returns what didn't move.
    pub(super) fn move_from_inventory(&mut self, from: usize, stack: Stack) -> Option<Stack> {
        match *self.container {
            Container::Minecart(id) => {
                if self.entities.vehicle_slots(id).is_none() {
                    return Some(stack);
                }
                return self.entities.insert_vehicle(id, stack);
            }
            Container::EnderChest(_) => {
                let order: Vec<usize> = (0..crate::world::chest::SLOTS).collect();
                return move_into(stack, &mut self.session.ender.slots, &order);
            }
            Container::Chest(p) => {
                let order: Vec<usize> = (0..self.world.container_slots(p)).collect();
                return self.world.with_chest_slots(p, |slots| move_into(stack, slots, &order)).unwrap_or(Some(stack));
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
            Container::Inventory if self.session.mode.is_survival() => {
                // Armor goes on, if that slot is free.
                if let Some((piece, _)) = stack.item.as_armor()
                    && self.session.inventory.armor[piece as usize].is_none()
                {
                    self.session.inventory.armor[piece as usize] = Some(stack);
                    return None;
                }
            }
            Container::Inventory | Container::CraftingTable | Container::Trading(_) => {}
        }
        let order: Vec<usize> =
            if from < HOTBAR_SLOTS { (HOTBAR_SLOTS..SLOTS).collect() } else { (0..HOTBAR_SLOTS).collect() };
        move_into(stack, &mut self.session.inventory.slots, &order)
    }

    /// A click on a brewing stand slot: bottles take one potion or glass
    /// bottle each, the ingredient slot only brewing ingredients and the
    /// fuel slot only blaze powder (Java's slot rules).
    pub(super) fn brewing_click(&mut self, pos: IVec3, slot: SlotRef, right: bool) {
        use crate::world::brewing;
        let cursor = &mut self.session.inventory.cursor;
        let Some(b) = self.world.brewing_stand_mut(pos) else { return };
        match slot {
            SlotRef::BrewBottle(i) => {
                let cell = &mut b.bottles[i];
                match cursor {
                    None => *cursor = cell.take(),
                    Some(c) if !brewing::fits_bottle_slot(c.item) => {}
                    Some(c) if cell.is_none() => {
                        *cell = Some(Stack::new(c.item, 1));
                        c.count -= 1;
                        if c.count == 0 {
                            *cursor = None;
                        }
                    }
                    Some(c) if c.count == 1 => std::mem::swap(cell, cursor),
                    Some(_) => {}
                }
            }
            SlotRef::BrewIngredient if cursor.is_none_or(|c| brewing::is_ingredient(c.item)) => {
                crate::inventory::click_slot(&mut b.ingredient, cursor, right)
            }
            SlotRef::BrewFuel if cursor.is_none_or(|c| c.item == Item::BLAZE_POWDER) => {
                crate::inventory::click_slot(&mut b.fuel, cursor, right)
            }
            _ => {}
        }
    }

    /// A click on a furnace slot. Fuel only takes things that burn; the
    /// output can only be taken from.
    pub(super) fn furnace_click(&mut self, pos: IVec3, slot: SlotRef, right: bool) {
        let cursor = &mut self.session.inventory.cursor;
        let Some(f) = self.world.furnace_mut(pos) else { return };
        let before = f.output.map_or(0, |s| s.count);
        match slot {
            SlotRef::FurnaceInput => f.click_input(cursor, right),
            SlotRef::FurnaceFuel => {
                if cursor.is_none_or(|c| crate::world::furnace::burn_time(c.item).is_some()) {
                    crate::inventory::click_slot(&mut f.fuel, cursor, right)
                }
            }
            SlotRef::FurnaceOutput => match (cursor.as_mut(), f.output) {
                (None, out) => {
                    *cursor = out;
                    f.output = None;
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
    pub(super) fn award_furnace_xp(&mut self, pos: IVec3, before: u8) {
        let roll = self.entities.roll();
        let Some(xp) = self.world.furnace_mut(pos).and_then(|f| f.take_output_xp(before, roll)) else { return };
        self.entities.spawn_xp(self.session.player.pos, xp);
    }

    /// Shared desktop/controller/LAN remainder delivery, including stacked honey.
    fn consume_craft(&mut self) {
        for mut stack in self.craft.consume_with_remainders() {
            stack.count = self.session.inventory.add_stack(stack);
            if stack.count != 0 {
                self.entities.throw(stack, self.session.player.eye(), self.session.player.forward().as_dvec3());
            }
        }
    }

    /// Clicking the crafting result: takes one craft onto the cursor (or
    /// onto a matching held stack, if it fits) and uses up the ingredients.
    pub(super) fn take_craft_result(&mut self) {
        let Some(result) = self.craft.result() else { return };
        match &mut self.session.inventory.cursor {
            None => self.session.inventory.cursor = Some(result),
            Some(c) if c.stacks_with(&result) && c.count as u16 + result.count as u16 <= c.max() as u16 => {
                c.count += result.count
            }
            Some(_) => return,
        }
        self.consume_craft();
    }

    /// A left (or right) click on a slot of the open container screen.
    pub(super) fn click_slot(&mut self, slot: SlotRef, right: bool) {
        match Some(slot) {
            Some(SlotRef::Inventory(i)) => self.session.inventory.click(i, right),
            Some(SlotRef::Craft(i)) => {
                crate::inventory::click_slot(&mut self.craft.cells[i], &mut self.session.inventory.cursor, right)
            }
            Some(SlotRef::CraftResult) => self.take_craft_result(),
            Some(SlotRef::Trade(i)) => self.buy_trade(i),
            Some(SlotRef::Offhand) => self.session.inventory.click_offhand(right),
            Some(SlotRef::Armor(piece)) => {
                self.session.inventory.click_armor(piece, right, self.session.mode.is_creative())
            }
            Some(SlotRef::Chest(i)) => {
                if let Container::Minecart(id) = *self.container {
                    if self.session.inventory.cursor.is_none_or(|s| self.entities.vehicle_accepts(id, i, s.item))
                        && let Some(slot) = self.entities.vehicle_slots_mut(id).and_then(|slots| slots.get_mut(i))
                    {
                        crate::inventory::click_slot(slot, &mut self.session.inventory.cursor, right);
                    }
                    return;
                }
                if let Container::EnderChest(_) = *self.container {
                    if let Some(slot) = self.session.ender.slots.get_mut(i) {
                        crate::inventory::click_slot(slot, &mut self.session.inventory.cursor, right);
                    }
                    return;
                }
                if let Container::Chest(pos) = *self.container
                    && i < self.world.container_slots(pos)
                {
                    let cursor = &mut self.session.inventory.cursor;
                    self.world
                        .with_chest_slots(pos, |slots| crate::inventory::click_slot(&mut slots[i], cursor, right));
                }
            }
            Some(s @ (SlotRef::FurnaceInput | SlotRef::FurnaceFuel | SlotRef::FurnaceOutput)) => {
                if let Container::Furnace(pos) = *self.container {
                    self.furnace_click(pos, s, right);
                }
            }
            Some(s @ (SlotRef::BrewBottle(_) | SlotRef::BrewIngredient | SlotRef::BrewFuel)) => {
                if let Container::Brewing(pos) = *self.container {
                    self.brewing_click(pos, s, right);
                }
            }
            Some(s @ (SlotRef::EnchantItem | SlotRef::EnchantLapis | SlotRef::EnchantOffer(_))) => {
                self.table_click(s, right)
            }
            Some(s @ (SlotRef::AnvilLeft | SlotRef::AnvilRight | SlotRef::AnvilResult)) => self.anvil_click(s, right),
            Some(s @ (SlotRef::SmithTemplate | SlotRef::SmithBase | SlotRef::SmithAddition | SlotRef::SmithResult)) => {
                self.smithing_click(s, right)
            }
            Some(SlotRef::Palette(item)) => {
                // Creative palette: take a full stack, or trash the held one.
                self.session.inventory.cursor = match self.session.inventory.cursor {
                    Some(_) => None,
                    None => Some(Stack::new(item, item.max_stack())),
                };
            }
            None => {}
        }
    }

    /// A click on the table's slots or offers. The item slot holds one
    /// item, the lapis slot only lapis (Java's slot rules).
    pub(super) fn table_click(&mut self, slot: SlotRef, right: bool) {
        let cursor = &mut self.session.inventory.cursor;
        match slot {
            SlotRef::EnchantItem => match (*cursor, self.session.work[0]) {
                (Some(c), None) => {
                    self.session.work[0] = Some(Stack { count: 1, ..c });
                    *cursor = (c.count > 1).then_some(Stack { count: c.count - 1, ..c });
                }
                (Some(c), Some(held)) if c.count == 1 => {
                    self.session.work[0] = Some(c);
                    *cursor = Some(held);
                }
                (None, held) => {
                    *cursor = held;
                    self.session.work[0] = None;
                }
                _ => {}
            },
            SlotRef::EnchantLapis => {
                if cursor.is_none_or(|c| c.item == Item::LAPIS_LAZULI) {
                    crate::inventory::click_slot(&mut self.session.work[1], cursor, right);
                }
            }
            SlotRef::EnchantOffer(i) => self.take_offer(i),
            _ => {}
        }
    }

    /// Java's `EnchantmentMenu.clickMenuButton`: enchants the item with
    /// offer `i`'s roll, costing `i + 1` levels and lapis, and draws a new
    /// enchantment seed.
    pub(super) fn take_offer(&mut self, i: usize) {
        let Container::Enchanting(pos) = *self.container else { return };
        let offer = enchant_offers(*self.container, self.session, self.world)[i];
        let Some(mut stack) = self.session.work[0] else { return };
        if !can_take_offer(self.session, i, offer) {
            return;
        }
        let list = enchant::offer_enchants(self.session.vitals.xp.seed, stack.item, i, offer.cost);
        if list.is_empty() {
            return;
        }
        if stack.item == Item::BOOK {
            stack.item = Item::ENCHANTED_BOOK;
        }
        for (e, level) in list {
            stack.enchants.set(e, level);
        }
        self.session.work[0] = Some(stack);
        if self.session.mode.is_survival() {
            self.session.vitals.xp.add_levels(-(i as i64 + 1));
            if let Some(lapis) = &mut self.session.work[1] {
                lapis.count -= i as u8 + 1;
                if lapis.count == 0 {
                    self.session.work[1] = None;
                }
            }
        }
        self.session.vitals.xp.seed = (enchant::roll() * u32::MAX as f32) as u32 as i32;
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        // `ENCHANTMENT_TABLE_USE` is `BLOCKS`.
        self.audio.play_in(SoundSource::Blocks, Sound::LevelUp, Some(at), 0.6, (1.5, 1.7));
    }

    /// Shift-click from the inventory onto the table: lapis to its slot,
    /// anything else into the empty item slot. Returns what didn't move.
    pub(super) fn move_to_table(&mut self, stack: Stack) -> Option<Stack> {
        if stack.item == Item::LAPIS_LAZULI {
            return move_into(stack, std::slice::from_mut(&mut self.session.work[1]), &[0]);
        }
        if self.session.work[0].is_some() {
            return Some(stack);
        }
        self.session.work[0] = Some(Stack { count: 1, ..stack });
        (stack.count > 1).then_some(Stack { count: stack.count - 1, ..stack })
    }

    /// A click on an anvil slot: the inputs take anything; the result goes
    /// to an empty cursor.
    pub(super) fn anvil_click(&mut self, slot: SlotRef, right: bool) {
        match slot {
            SlotRef::AnvilLeft | SlotRef::AnvilRight => {
                let i = (slot == SlotRef::AnvilRight) as usize;
                if !matches!(*self.container, Container::Grindstone(_))
                    || self
                        .session
                        .inventory
                        .cursor
                        .is_none_or(|s| voxelcraft::grindstone::accepts(s.item) || !s.enchants.is_empty())
                {
                    crate::inventory::click_slot(&mut self.session.work[i], &mut self.session.inventory.cursor, right);
                }
            }
            SlotRef::AnvilResult if self.session.inventory.cursor.is_none() => {
                if let Some(out) = self.take_anvil_result() {
                    self.session.inventory.cursor = Some(out);
                }
            }
            _ => {}
        }
    }

    /// Java's `AnvilMenu.onTake`: pays the levels, uses up the inputs and
    /// maybe chips the anvil. Returns the result.
    pub(super) fn take_anvil_result(&mut self) -> Option<Stack> {
        if let Container::Grindstone(pos) = *self.container {
            let result = anvil_result(*self.container, self.session)?;
            let xp = voxelcraft::grindstone::xp(self.session.work[0], self.session.work[1], enchant::roll());
            self.session.work[0] = None;
            self.session.work[1] = None;
            if xp > 0 {
                self.world.xp_drops.push((pos, xp));
            }
            return Some(result.output);
        }
        let Container::Anvil(pos) = *self.container else { return None };
        let result = anvil_result(*self.container, self.session)?;
        let survival = self.session.mode.is_survival();
        if survival {
            self.session.vitals.xp.add_levels(-(result.cost as i64));
        }
        self.session.work[0] = None;
        self.session.work[1] = match (result.uses, self.session.work[1]) {
            (Some(n), Some(s)) if s.count > n => Some(Stack { count: s.count - n, ..s }),
            _ => None,
        };
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        let broke = if survival { enchant::wear_anvil(self.world, pos) } else { None };
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
        let Some(result) = anvil_result(*self.container, self.session) else { return };
        let mut slots = self.session.inventory.slots;
        if move_into(result.output, &mut slots, &(0..crate::inventory::SLOTS).collect::<Vec<_>>()).is_none()
            && let Some(out) = self.take_anvil_result()
        {
            self.session.inventory.add_stack(out);
        }
    }

    /// Shift-click from the inventory onto the anvil: one item into the left
    /// input if it's empty, else the right. Returns what didn't move.
    pub(super) fn move_to_anvil(&mut self, stack: Stack) -> Option<Stack> {
        if matches!(*self.container, Container::Grindstone(_))
            && !voxelcraft::grindstone::accepts(stack.item)
            && stack.enchants.is_empty()
        {
            return Some(stack);
        }
        if self.session.work[0].is_none() {
            self.session.work[0] = Some(Stack { count: 1, ..stack });
            return (stack.count > 1).then_some(Stack { count: stack.count - 1, ..stack });
        }
        move_into(stack, std::slice::from_mut(&mut self.session.work[1]), &[0])
    }

    pub(super) fn smithing_click(&mut self, slot: SlotRef, right: bool) {
        let input = match slot {
            SlotRef::SmithTemplate => Some((0, crate::smithing::is_template as fn(_) -> _)),
            SlotRef::SmithBase => Some((1, crate::smithing::is_base as fn(_) -> _)),
            SlotRef::SmithAddition => Some((2, crate::smithing::is_addition as fn(_) -> _)),
            _ => None,
        };
        if let Some((i, accepts)) = input {
            if self.session.inventory.cursor.is_none_or(|s| accepts(s.item)) {
                crate::inventory::click_slot(&mut self.session.work[i], &mut self.session.inventory.cursor, right);
            }
        } else if slot == SlotRef::SmithResult
            && self.session.inventory.cursor.is_none()
            && let Some(out) = self.take_smithing_result()
        {
            self.session.inventory.cursor = Some(out);
        }
    }

    /// Consumes one template, base and addition, returning the copied base
    /// metadata on the Netherite result.
    pub(super) fn take_smithing_result(&mut self) -> Option<Stack> {
        let Container::Smithing(pos) = *self.container else { return None };
        let result = smithing_result(self.session)?;
        for input in &mut self.session.work {
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
        let Some(result) = smithing_result(self.session) else { return };
        let mut slots = self.session.inventory.slots;
        if move_into(result, &mut slots, &(0..crate::inventory::SLOTS).collect::<Vec<_>>()).is_none()
            && let Some(out) = self.take_smithing_result()
        {
            self.session.inventory.add_stack(out);
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
        .find_map(|(i, accepts)| (accepts && self.session.work[i].is_none()).then_some(i));
        let Some(i) = slot else { return Some(stack) };
        move_into(stack, std::slice::from_mut(&mut self.session.work[i]), &[0])
    }

    pub(super) fn buy_trade(&mut self, index: usize) {
        let Container::Trading(id) = *self.container else { return };
        if !self.entities.merchant_in_reach(id, self.session.player.eye(), self.session.entity_screen_range()) {
            return;
        }
        match self.entities.trade_for(id, index, &mut self.session.inventory, self.session.id) {
            Ok(xp) => {
                self.entities.spawn_xp(self.session.player.pos, xp);
                self.audio.ui_click();
            }
            Err(message) => self.requests.push(Request::Popup(message.to_string())),
        }
    }
}
