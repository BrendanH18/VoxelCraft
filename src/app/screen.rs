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
use crate::inventory::{Stack, move_into};
use crate::item::Item;
use crate::world::World;
use voxelcraft::session::PlayerSession;
use voxelcraft::session::storage::{Storage, StorageScreen, StorageSlot};

use super::Container;
use super::hud::SlotRef;

/// Something the clicking player's adapter must do.
#[derive(Debug, PartialEq)]
pub(super) enum Request {
    Popup(String),
    /// v0.16 Track C: the screen asked to close (the beacon's Done and
    /// Cancel buttons).
    Close,
}

pub(super) struct Screen<'a> {
    pub session: &'a mut PlayerSession,
    pub container: &'a mut Container,
    pub craft: &'a mut Grid,
    pub world: &'a mut World,
    pub entities: &'a mut Entities,
    pub audio: &'a mut Audio,
    pub requests: Vec<Request>,
    /// v0.18 Track 5: the `limited_crafting` gamerule.
    pub limited_crafting: bool,
}

/// Where stacks leaving a container land in the inventory: the hotbar from
/// the right, then the main grid from the bottom right, like Minecraft.
fn to_player_order() -> Vec<usize> {
    voxelcraft::session::storage::TO_PLAYER.to_vec()
}

/// The shared storage screen (`session::storage`) behind `container`, if
/// it is one: every chest-backed block, furnace, brewing stand, vehicle
/// and ender inventory.
pub(super) fn storage_of(container: Container) -> Option<Storage> {
    match container {
        Container::Chest(p) => Some(Storage::Block(p)),
        Container::EnderChest(_) => Some(Storage::Ender),
        Container::Minecart(id) => Some(Storage::Vehicle(id)),
        Container::Furnace(p) => Some(Storage::Furnace(p)),
        Container::Brewing(p) => Some(Storage::Brewing(p)),
        _ => None,
    }
}

/// The crafting result `session` may take from `craft` (see
/// [`Screen::craft_result`]); the HUD shows the same.
pub(super) fn crafting_result(
    craft: &Grid,
    world: &World,
    session: &PlayerSession,
    limited_crafting: bool,
) -> Option<Stack> {
    if !session.recipe_book.allows(craft, limited_crafting) {
        return None;
    }
    craft.result_in(&world.maps)
}

/// The storage slot a screen slot names, if it is one.
pub(super) fn storage_slot(slot: SlotRef) -> Option<StorageSlot> {
    Some(match slot {
        SlotRef::Inventory(i) => StorageSlot::Inventory(i),
        SlotRef::Chest(i) => StorageSlot::Container(i),
        SlotRef::FurnaceInput => StorageSlot::FurnaceInput,
        SlotRef::FurnaceFuel => StorageSlot::FurnaceFuel,
        SlotRef::FurnaceOutput => StorageSlot::FurnaceOutput,
        SlotRef::BrewBottle(i) => StorageSlot::BrewBottle(i),
        SlotRef::BrewIngredient => StorageSlot::BrewIngredient,
        SlotRef::BrewFuel => StorageSlot::BrewFuel,
        _ => return None,
    })
}

/// What the anvil or grindstone inputs make, ignoring the survival cost
/// limit, and whether it's "Too Expensive!" for this player.
pub(super) fn anvil_preview(container: Container, session: &PlayerSession) -> Option<(AnvilResult, bool)> {
    if matches!(container, Container::Grindstone(_)) {
        return voxelcraft::grindstone::result(session.work[0], session.work[1]).map(|r| (r, false));
    }
    let creative = session.mode.is_creative();
    // v0.18 Track 2: with the name box's text, as `AnvilMenu.createResult`.
    let result = enchant::anvil_menu(session.work[0]?, session.work[1], session.anvil_name(), creative)?;
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
        if self.workstation_quick_move(slot) {
            return;
        }
        match slot {
            SlotRef::Inventory(i) => {
                let Some(stack) = self.session.inventory.slots[i].take() else { return };
                let left = self.move_from_inventory(i, stack);
                self.session.inventory.slots[i] = left;
            }
            SlotRef::Craft(i) => {
                let Some(stack) = self.craft.cells[i].take() else { return };
                self.craft.cells[i] = self.move_to_player(stack);
            }
            SlotRef::CraftResult => {
                // Craft until the ingredients run out or the result won't fit.
                let order = to_player_order();
                while let Some(mut result) = self.craft_result() {
                    let mut slots = self.session.inventory.slots;
                    if move_into(result, &mut slots, &order).is_some() {
                        break;
                    }
                    self.world.maps.post_process(&mut result);
                    move_into(result, &mut self.session.inventory.slots, &order);
                    self.consume_craft();
                }
            }
            SlotRef::Chest(_)
            | SlotRef::FurnaceInput
            | SlotRef::FurnaceFuel
            | SlotRef::FurnaceOutput
            | SlotRef::BrewBottle(_)
            | SlotRef::BrewIngredient
            | SlotRef::BrewFuel => {
                if let (Some(mut screen), Some(slot)) = (self.storage(), storage_slot(slot)) {
                    screen.quick_move(slot);
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
            SlotRef::CutInput
            | SlotRef::CutResult
            | SlotRef::CutRecipe(_)
            | SlotRef::LoomBanner
            | SlotRef::LoomDye
            | SlotRef::LoomPattern
            | SlotRef::LoomResult
            | SlotRef::LoomChoice(_)
            | SlotRef::MapInput
            | SlotRef::MapExtra
            | SlotRef::MapResult
            | SlotRef::BeaconPayment
            | SlotRef::BeaconPower(_)
            | SlotRef::BeaconDone
            | SlotRef::BeaconCancel => {}
            // v0.18 Track 4: shift on a creative cell takes a full stack,
            // or fills the held one (`slotClicked` with `QUICK_MOVE`).
            SlotRef::Palette(stack) if self.session.mode.is_creative() => {
                voxelcraft::creative::click(&mut self.session.inventory.cursor, Some(stack), false, true)
            }
            // Shift on Destroy Item empties the whole inventory.
            SlotRef::Destroy if self.session.mode.is_creative() => {
                let inv = &mut self.session.inventory;
                inv.slots.fill(None);
                inv.armor.fill(None);
                inv.offhand = None;
                let _ = self.craft.take_all();
            }
            SlotRef::Palette(_) | SlotRef::PaletteEmpty | SlotRef::Destroy => {}
        }
    }

    pub(super) fn move_to_player(&mut self, stack: Stack) -> Option<Stack> {
        move_into(stack, &mut self.session.inventory.slots, &to_player_order())
    }

    /// Where a shift-clicked inventory stack goes: into the open chest, a
    /// furnace's input (smeltables) or fuel slot, or else between the hotbar
    /// and the main grid. Returns what didn't move.
    pub(super) fn move_from_inventory(&mut self, from: usize, mut stack: Stack) -> Option<Stack> {
        if let Some(mut screen) = self.storage()
            && let Some(left) = screen.quick_move_in(stack)
        {
            return left;
        }
        match *self.container {
            Container::Chest(_)
            | Container::EnderChest(_)
            | Container::Minecart(_)
            | Container::Furnace(_)
            | Container::Brewing(_) => {}
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
            Container::Stonecutter(_) | Container::Loom(_) | Container::Cartography(_) | Container::Beacon(_) => {
                if let Some(left) = self.move_to_workstation(stack)
                    && left != Some(stack)
                {
                    return left;
                }
            }
            Container::Inventory if self.session.mode.is_survival() => {
                // Armor (and a mob head) goes on, if that slot is free.
                if let Some(piece) = stack.item.wear_slot()
                    && self.session.inventory.armor[piece as usize].is_none()
                {
                    // One item fits (`ArmorSlot.getMaxStackSize`); the rest
                    // of a stack of heads moves on below.
                    self.session.inventory.armor[piece as usize] = Some(Stack { count: 1, ..stack });
                    if stack.count <= 1 {
                        return None;
                    }
                    stack.count -= 1;
                }
            }
            Container::Inventory | Container::CraftingTable | Container::Trading(_) => {}
        }
        voxelcraft::session::storage::between_sections(&mut self.session.inventory, from, stack)
    }

    /// The shared storage screen for this player's open container.
    pub(super) fn storage(&mut self) -> Option<StorageScreen<'_>> {
        let storage = storage_of(*self.container)?;
        Some(StorageScreen {
            session: &mut *self.session,
            world: &mut *self.world,
            entities: &mut *self.entities,
            storage,
        })
    }

    /// What this player may take from the crafting result: the grid's
    /// result, unless `limited_crafting` hides a recipe they don't know
    /// (`RecipeCraftingHolder.setRecipeUsed`).
    pub(super) fn craft_result(&self) -> Option<Stack> {
        crafting_result(self.craft, self.world, self.session, self.limited_crafting)
    }

    /// Shared desktop/controller/LAN remainder delivery, including stacked
    /// honey. Taking the result teaches its recipe
    /// (`RecipeCraftingHolder.awardUsedRecipes`).
    fn consume_craft(&mut self) {
        self.session.recipe_book.crafted(self.craft);
        for mut stack in self.craft.consume_with_remainders() {
            stack.count = self.session.inventory.add_stack(stack);
            if stack.count != 0 {
                self.entities.throw(stack, self.session.player.eye(), self.session.player.forward().as_dvec3());
            }
        }
    }

    /// v0.18 Track 5: the recipe book's click on recipe `id`
    /// (`ServerGamePacketListenerImpl.handlePlaceRecipe`): only a known
    /// recipe, only on a crafting grid or furnace, never by a spectator. The
    /// items move by Java's placement rules (`crafting::book::place`), with
    /// `selected` the hotbar slot returned items go to first.
    pub(super) fn place_recipe(
        &mut self,
        id: voxelcraft::crafting::book::RecipeId,
        max: bool,
        selected: usize,
    ) -> Option<voxelcraft::crafting::book::place::Outcome> {
        use voxelcraft::crafting::book::place;
        if self.session.mode == voxelcraft::rules::GameMode::Spectator || !self.session.recipe_book.knows(id) {
            return None;
        }
        let recipe = id.recipe()?;
        let creative = self.session.mode.is_creative();
        let inventory = &mut self.session.inventory;
        Some(match *self.container {
            Container::Furnace(p) => {
                place::place_in_furnace(recipe, self.world.furnace_mut(p)?, inventory, selected, max, creative)
            }
            Container::Inventory | Container::CraftingTable => {
                place::place_in_grid(recipe, self.craft, inventory, selected, max, creative)
            }
            _ => return None,
        })
    }

    /// Clicking the crafting result: takes one craft onto the cursor (or
    /// onto a matching held stack, if it fits) and uses up the ingredients.
    pub(super) fn take_craft_result(&mut self) {
        let Some(mut result) = self.craft_result() else { return };
        match &mut self.session.inventory.cursor {
            None => {
                // `MapItem.onCraftedBy`: a zoomed-out map gets its new map id.
                self.world.maps.post_process(&mut result);
                self.session.inventory.cursor = Some(result);
            }
            Some(c)
                if !(result.item.is_map() && result.entity_data != 0)
                    && c.stacks_with(&result)
                    && c.count as u16 + result.count as u16 <= c.max() as u16 =>
            {
                c.count += result.count
            }
            Some(_) => return,
        }
        self.consume_craft();
    }

    /// v0.17 Track C1: `CrafterScreen.slotClicked` toggles an empty slot: a
    /// disabled one turns on (and then takes the click), an enabled one
    /// turns off under an empty cursor. Returns whether the click is used up.
    fn crafter_toggle(&mut self, pos: IVec3, i: usize) -> bool {
        if self.world.crafter(pos).is_none()
            || self.world.chest_slot(pos, i).is_some()
            || self.session.mode == voxelcraft::rules::GameMode::Spectator
        {
            return false;
        }
        if self.world.crafter_slot_disabled(pos, i) {
            self.world.set_crafter_slot_enabled(pos, i, true);
            self.audio.play(Sound::Click, None, 0.4, (1.0, 1.0));
            false
        } else if self.session.inventory.cursor.is_none() {
            self.world.set_crafter_slot_enabled(pos, i, false);
            self.audio.play(Sound::Click, None, 0.4, (0.75, 0.75));
            true
        } else {
            false
        }
    }

    /// A left (or right) click on a slot of the open container screen.
    pub(super) fn click_slot(&mut self, slot: SlotRef, right: bool) {
        if self.workstation_click(slot, right) {
            return;
        }
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
            Some(
                s @ (SlotRef::Chest(_)
                | SlotRef::FurnaceInput
                | SlotRef::FurnaceFuel
                | SlotRef::FurnaceOutput
                | SlotRef::BrewBottle(_)
                | SlotRef::BrewIngredient
                | SlotRef::BrewFuel),
            ) => {
                if let (SlotRef::Chest(i), Container::Chest(pos)) = (s, *self.container)
                    && self.crafter_toggle(pos, i)
                {
                    return;
                }
                if let (Some(mut screen), Some(slot)) = (self.storage(), storage_slot(s)) {
                    screen.click(slot, right);
                }
            }
            Some(s @ (SlotRef::EnchantItem | SlotRef::EnchantLapis | SlotRef::EnchantOffer(_))) => {
                self.table_click(s, right)
            }
            Some(s @ (SlotRef::AnvilLeft | SlotRef::AnvilRight | SlotRef::AnvilResult)) => self.anvil_click(s, right),
            Some(s @ (SlotRef::SmithTemplate | SlotRef::SmithBase | SlotRef::SmithAddition | SlotRef::SmithResult)) => {
                self.smithing_click(s, right)
            }
            Some(
                SlotRef::CutInput
                | SlotRef::CutResult
                | SlotRef::CutRecipe(_)
                | SlotRef::LoomBanner
                | SlotRef::LoomDye
                | SlotRef::LoomPattern
                | SlotRef::LoomResult
                | SlotRef::LoomChoice(_)
                | SlotRef::MapInput
                | SlotRef::MapExtra
                | SlotRef::MapResult
                | SlotRef::BeaconPayment
                | SlotRef::BeaconPower(_)
                | SlotRef::BeaconDone
                | SlotRef::BeaconCancel,
            ) => {}
            // v0.18 Track 4: Java's creative cell rules (`creative::click`).
            Some(SlotRef::Palette(stack)) if self.session.mode.is_creative() => {
                voxelcraft::creative::click(&mut self.session.inventory.cursor, Some(stack), right, false)
            }
            Some(SlotRef::PaletteEmpty) if self.session.mode.is_creative() => {
                voxelcraft::creative::click(&mut self.session.inventory.cursor, None, right, false)
            }
            // Destroy Item deletes the held stack.
            Some(SlotRef::Destroy) if self.session.mode.is_creative() => self.session.inventory.cursor = None,
            Some(SlotRef::Palette(_) | SlotRef::PaletteEmpty | SlotRef::Destroy) => {}
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
        // `onEnchantmentPerformed` takes the levels in every game mode;
        // `consume` spares a creative player's lapis.
        self.session.vitals.xp.add_levels(-(i as i64 + 1));
        if !self.session.mode.is_creative()
            && let Some(lapis) = &mut self.session.work[1]
        {
            lapis.count -= i as u8 + 1;
            if lapis.count == 0 {
                self.session.work[1] = None;
            }
        }
        self.session.vitals.xp.seed = enchant::new_seed();
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
        // A level-up while the screen is open can unlock an explorer map.
        self.entities.prepare_merchant_maps(self.world, id);
        match self.entities.trade_for(id, index, &mut self.session.inventory, self.session.id) {
            Ok(xp) => {
                self.entities.spawn_xp(self.session.player.pos, xp);
                self.audio.ui_click();
            }
            Err(message) => self.requests.push(Request::Popup(message.to_string())),
        }
    }
}
