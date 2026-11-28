//! v1.0 final track F1: Java's other container click types on every
//! screen, for whichever player acts (the host, a controller seat or a LAN
//! guest): dragging the held stack across slots (`QUICK_CRAFT`),
//! double-clicking to collect matching items (`PICKUP_ALL`), shift
//! double-click to move every matching stack of a section, the number keys
//! and the swap-hands key over a slot (`SWAP`, buttons 0..8 and 40) and
//! the drop key over a slot (`THROW`).
//!
//! The rules (`crate::inventory::clicks`) see each screen's slots in its
//! Java menu order through [`MenuCells`], which keeps each slot's own
//! `mayPlace`, `mayPickup` and stack limit.

use crate::inventory::Stack;
use crate::inventory::clicks::{self, Cells, Drag};
use crate::item::Item;
use voxelcraft::session::storage as st;

use super::Container;
use super::hud::SlotRef;
use super::screen::{Screen, storage_slot};
use super::workstations as ws;

/// `Slot.container` sections for the shift double-click.
const PLAYER: u8 = 1;
const CRAFT: u8 = 2;
const RESULT: u8 = 3;

impl Screen<'_> {
    /// This screen's slots in Java's menu order (`AbstractContainerMenu.slots`):
    /// the container's slots, then the main inventory and the hotbar.
    pub(super) fn menu_slots(&mut self) -> Vec<SlotRef> {
        let craft = |n: usize| std::iter::once(SlotRef::CraftResult).chain((0..n).map(SlotRef::Craft));
        let mut out: Vec<SlotRef> = match *self.container {
            // `InventoryMenu`: result, 2x2 grid, armour, then the player's
            // slots and the off-hand last.
            Container::Inventory => craft(4)
                .chain(crate::item::ArmorPiece::ALL.into_iter().map(SlotRef::Armor))
                .collect(),
            Container::CraftingTable => craft(9).collect(),
            Container::Furnace(_) => vec![SlotRef::FurnaceInput, SlotRef::FurnaceFuel, SlotRef::FurnaceOutput],
            Container::Brewing(_) => vec![
                SlotRef::BrewBottle(0),
                SlotRef::BrewBottle(1),
                SlotRef::BrewBottle(2),
                SlotRef::BrewIngredient,
                SlotRef::BrewFuel,
            ],
            Container::Chest(_) | Container::EnderChest(_) | Container::Minecart(_) => {
                let n = self.storage().map_or(0, |s| s.container_len());
                (0..n).map(SlotRef::Chest).collect()
            }
            Container::Enchanting(_) => vec![SlotRef::EnchantItem, SlotRef::EnchantLapis],
            Container::Anvil(_) | Container::Grindstone(_) => {
                vec![SlotRef::AnvilLeft, SlotRef::AnvilRight, SlotRef::AnvilResult]
            }
            Container::Smithing(_) => {
                vec![SlotRef::SmithTemplate, SlotRef::SmithBase, SlotRef::SmithAddition, SlotRef::SmithResult]
            }
            Container::Stonecutter(_) => vec![SlotRef::CutInput, SlotRef::CutResult],
            Container::Loom(_) => {
                vec![SlotRef::LoomBanner, SlotRef::LoomDye, SlotRef::LoomPattern, SlotRef::LoomResult]
            }
            Container::Cartography(_) => vec![SlotRef::MapInput, SlotRef::MapExtra, SlotRef::MapResult],
            Container::Beacon(_) => vec![SlotRef::BeaconPayment],
            // `MerchantMenu`'s payment slots aren't kept: a trade is bought
            // from its offer at once.
            Container::Trading(_) => Vec::new(),
        };
        out.extend((crate::inventory::HOTBAR_SLOTS..crate::inventory::SLOTS).map(SlotRef::Inventory));
        out.extend((0..crate::inventory::HOTBAR_SLOTS).map(SlotRef::Inventory));
        if *self.container == Container::Inventory {
            out.push(SlotRef::Offhand);
        }
        out
    }

    /// Runs `f` over this screen's slots in menu order.
    fn with_cells<R>(&mut self, f: impl FnOnce(&mut MenuCells<'_, '_>) -> R) -> R {
        let slots = self.menu_slots();
        let mut cells = MenuCells { screen: self, slots };
        let out = f(&mut cells);
        ws::sync_work_menu(*cells.screen.container, cells.screen.session);
        out
    }

    /// Whether `slot` takes part in a drag of the held stack that already
    /// covers `taken` slots (`mouseDragged`'s check).
    pub(super) fn drag_accepts(&mut self, slot: SlotRef, kind: Drag, taken: usize) -> bool {
        let Some(held) = self.session.inventory.cursor else { return false };
        if !kind.allowed(self.session.mode.is_creative()) {
            return false;
        }
        self.with_cells(|c| c.index(slot).is_some_and(|i| clicks::drag_accepts(c, i, &held, kind, taken)))
    }

    /// What a drag over `slots` would leave in each slot and the cursor.
    pub(super) fn drag_preview(&mut self, slots: &[SlotRef], kind: Drag) -> Option<clicks::Preview> {
        let held = self.session.inventory.cursor?;
        Some(self.with_cells(|c| {
            let idx: Vec<usize> = slots.iter().filter_map(|s| c.index(*s)).collect();
            let mut p = clicks::drag_preview(c, &idx, kind, held);
            for (i, _) in &mut p.slots {
                // Report back as indices into `slots`.
                *i = slots.iter().position(|s| c.index(*s) == Some(*i)).unwrap_or(usize::MAX);
            }
            p
        }))
    }

    /// The end of a drag (`QUICK_CRAFT`): one slot is an ordinary click
    /// with the drag's button; more share the held stack out.
    pub(super) fn drag(&mut self, slots: &[SlotRef], kind: Drag) {
        if self.session.inventory.cursor.is_none() || !kind.allowed(self.session.mode.is_creative()) {
            return;
        }
        let mut unique: Vec<SlotRef> = Vec::new();
        for s in slots {
            if !unique.contains(s) {
                unique.push(*s);
            }
        }
        if let [only] = unique[..] {
            self.click_slot(only, kind != Drag::Even);
            return;
        }
        let mut cursor = self.session.inventory.cursor;
        self.with_cells(|c| {
            let idx: Vec<usize> = unique.iter().filter_map(|s| c.index(*s)).collect();
            clicks::finish_drag(c, &idx, kind, &mut cursor);
        });
        self.session.inventory.cursor = cursor;
    }

    /// A double click on `slot` with a stack held (`PICKUP_ALL`).
    pub(super) fn pickup_all(&mut self, slot: SlotRef, right: bool) {
        let mut cursor = self.session.inventory.cursor;
        self.with_cells(|c| {
            if let Some(i) = c.index(slot)
                && !c.is_result(i)
            {
                clicks::pickup_all(c, i, right, &mut cursor);
            }
        });
        self.session.inventory.cursor = cursor;
    }

    /// The shift double-click: every stack in `slot`'s section matching
    /// `last`, the stack the first click moved, is quick-moved in turn.
    pub(super) fn move_all(&mut self, slot: SlotRef, last: Stack) {
        let targets: Vec<SlotRef> = self.with_cells(|c| {
            let Some(i) = c.index(slot) else { return Vec::new() };
            if c.is_result(i) {
                return Vec::new();
            }
            let sections: Vec<u8> = c.slots.iter().map(|s| section(*s)).collect();
            clicks::move_all_targets(c, |j| sections[j], i, &last).into_iter().map(|j| c.slots[j]).collect()
        });
        for t in targets {
            self.quick_move(t);
        }
    }

    /// A number key (`hotbar` 0..8) or the swap-hands key (`None`, button
    /// 40) over `slot` with nothing held (`SWAP`).
    pub(super) fn swap_slot(&mut self, slot: SlotRef, hotbar: Option<usize>) {
        if self.session.inventory.cursor.is_some() {
            return;
        }
        let source = hotbar.map_or(SlotRef::Offhand, SlotRef::Inventory);
        if matches!(slot, SlotRef::Palette(_) | SlotRef::PaletteEmpty | SlotRef::Destroy) {
            // v0.18 Track 4: a creative cell fills the key's slot with a full stack.
            if let SlotRef::Palette(s) = slot {
                self.set_player_slot(source, Some(Stack { count: s.max(), ..s }));
            }
            return;
        }
        if is_result(slot) {
            // A result slot only empties into an empty key slot, running
            // its `onTake` as a click would.
            if self.player_slot(source).is_none() {
                self.click_slot(slot, false);
                let taken = self.session.inventory.cursor.take();
                self.set_player_slot(source, taken);
            }
            return;
        }
        self.with_cells(|c| {
            let (Some(i), Some(src)) = (c.index(slot), c.index(source).or_else(|| c.extra(source))) else {
                return;
            };
            swap(c, i, src);
        });
    }

    /// The drop key over `slot` with nothing held (`THROW`): one item, or
    /// the whole stack with the modifier; a result slot is crafted and
    /// thrown, again and again with the modifier while it makes the same item.
    pub(super) fn throw_slot(&mut self, slot: SlotRef, whole: bool) {
        if self.session.inventory.cursor.is_some() {
            return;
        }
        if let SlotRef::Palette(s) = slot {
            let stack = Stack { count: if whole { s.max() } else { 1 }, ..s };
            st::toss(self.session, self.entities, stack);
            return;
        }
        if is_result(slot) {
            let mut first: Option<Item> = None;
            for _ in 0..64 {
                self.click_slot(slot, false);
                let Some(taken) = self.session.inventory.cursor.take() else { break };
                st::toss(self.session, self.entities, taken);
                if !whole || first.is_some_and(|f| f != taken.item) {
                    break;
                }
                first = Some(taken.item);
                if self.slot_now(slot).is_none_or(|s| s.item != taken.item) {
                    break;
                }
            }
            return;
        }
        let thrown = self.with_cells(|c| {
            let i = c.index(slot)?;
            let s = c.get(i)?;
            if !c.may_pickup(i) {
                return None;
            }
            c.take(i, if whole { s.count } else { 1 })
        });
        if let Some(t) = thrown {
            st::toss(self.session, self.entities, t);
        }
    }

    /// The stack in `slot` (the stack a quick move is about to move).
    pub(super) fn stack_at(&mut self, slot: SlotRef) -> Option<Stack> {
        self.slot_now(slot)
    }

    fn slot_now(&mut self, slot: SlotRef) -> Option<Stack> {
        match slot {
            SlotRef::CraftResult => self.craft_result(),
            _ => self.with_cells(|c| c.index(slot).and_then(|i| c.get(i))),
        }
    }

    fn player_slot(&self, slot: SlotRef) -> Option<Stack> {
        match slot {
            SlotRef::Inventory(i) => self.session.inventory.slots[i],
            SlotRef::Offhand => self.session.inventory.offhand,
            _ => None,
        }
    }

    fn set_player_slot(&mut self, slot: SlotRef, stack: Option<Stack>) {
        match slot {
            SlotRef::Inventory(i) => self.session.inventory.slots[i] = stack,
            SlotRef::Offhand => self.session.inventory.offhand = stack,
            _ => {}
        }
    }
}

/// Java's `SWAP` between menu slot `i` and the key's inventory slot `src`.
fn swap(c: &mut MenuCells<'_, '_>, i: usize, src: usize) {
    if i == src {
        return;
    }
    match (c.get(src), c.get(i)) {
        (None, None) => {}
        (None, Some(t)) => {
            if c.may_pickup(i) {
                let taken = c.take(i, t.count);
                c.set(src, taken);
            }
        }
        (Some(s), None) => {
            if c.may_place(i, &s) {
                let max = c.max(i, &s);
                if s.count > max {
                    c.set(i, s.with_count_(max));
                    c.set(src, s.with_count_(s.count - max));
                } else {
                    c.set(src, None);
                    c.set(i, Some(s));
                }
            }
        }
        (Some(s), Some(t)) => {
            if c.may_pickup(i) && c.may_place(i, &s) {
                let max = c.max(i, &s);
                if s.count > max {
                    c.set(i, s.with_count_(max));
                    c.set(src, s.with_count_(s.count - max));
                    // `Inventory.add`, or dropped.
                    let left = c.screen.session.inventory.add_stack(t);
                    if left > 0 {
                        st::toss(c.screen.session, c.screen.entities, Stack { count: left, ..t });
                    }
                } else {
                    c.set(src, Some(t));
                    c.set(i, Some(s));
                }
            }
        }
    }
}

/// `stack` with `n` items, or nothing at zero.
trait WithCount {
    fn with_count_(self, n: u8) -> Option<Stack>;
}

impl WithCount for Stack {
    fn with_count_(self, n: u8) -> Option<Stack> {
        (n > 0).then_some(Stack { count: n, ..self })
    }
}

fn is_result(slot: SlotRef) -> bool {
    matches!(
        slot,
        SlotRef::CraftResult
            | SlotRef::AnvilResult
            | SlotRef::SmithResult
            | SlotRef::CutResult
            | SlotRef::LoomResult
            | SlotRef::MapResult
    )
}

/// The slot's `Slot.container`, as far as the shift double-click tells.
fn section(slot: SlotRef) -> u8 {
    match slot {
        SlotRef::Inventory(_) | SlotRef::Armor(_) | SlotRef::Offhand => PLAYER,
        SlotRef::Craft(_) => CRAFT,
        s if is_result(s) => RESULT,
        _ => 0,
    }
}

/// One screen's slots in menu order, with each slot's rules.
pub(super) struct MenuCells<'s, 'a> {
    screen: &'s mut Screen<'a>,
    slots: Vec<SlotRef>,
}

impl MenuCells<'_, '_> {
    fn index(&self, slot: SlotRef) -> Option<usize> {
        self.slots.iter().position(|s| *s == slot)
    }

    /// The off-hand is a swap source on every screen, though only the
    /// inventory screen shows it: it gets an index past the menu's slots.
    fn extra(&mut self, slot: SlotRef) -> Option<usize> {
        (slot == SlotRef::Offhand).then(|| {
            self.slots.push(SlotRef::Offhand);
            self.slots.len() - 1
        })
    }

    fn creative(&self) -> bool {
        self.screen.session.mode.is_creative()
    }

    /// The slot's furnace, when taking from it pays out experience.
    fn furnace_output(&self, slot: SlotRef) -> Option<glam::IVec3> {
        match (slot, *self.screen.container) {
            (SlotRef::FurnaceOutput, Container::Furnace(p)) => Some(p),
            _ => None,
        }
    }
}

impl Cells for MenuCells<'_, '_> {
    fn len(&self) -> usize {
        self.slots.len()
    }

    fn get(&mut self, i: usize) -> Option<Stack> {
        let s = &*self.screen.session;
        match self.slots[i] {
            SlotRef::Inventory(j) => s.inventory.slots[j],
            SlotRef::Offhand => s.inventory.offhand,
            SlotRef::Armor(p) => s.inventory.armor[p as usize],
            SlotRef::Craft(j) => self.screen.craft.cells.get(j).copied().flatten(),
            SlotRef::EnchantItem | SlotRef::AnvilLeft | SlotRef::SmithTemplate => s.work[0],
            SlotRef::EnchantLapis | SlotRef::AnvilRight | SlotRef::SmithBase => s.work[1],
            SlotRef::SmithAddition => s.work[2],
            r if is_result(r) => None,
            w if ws::work_index(w).is_some() => s.work[ws::work_index(w).unwrap()],
            other => {
                let slot = storage_slot(other)?;
                self.screen.storage()?.get(slot)
            }
        }
    }

    fn set(&mut self, i: usize, stack: Option<Stack>) {
        let slot = self.slots[i];
        let s = &mut *self.screen.session;
        match slot {
            SlotRef::Inventory(j) => s.inventory.slots[j] = stack,
            SlotRef::Offhand => s.inventory.offhand = stack,
            SlotRef::Armor(p) => s.inventory.armor[p as usize] = stack,
            SlotRef::Craft(j) => {
                if let Some(c) = self.screen.craft.cells.get_mut(j) {
                    *c = stack;
                }
            }
            SlotRef::EnchantItem | SlotRef::AnvilLeft | SlotRef::SmithTemplate => s.work[0] = stack,
            SlotRef::EnchantLapis | SlotRef::AnvilRight | SlotRef::SmithBase => s.work[1] = stack,
            SlotRef::SmithAddition => s.work[2] = stack,
            r if is_result(r) => {}
            w if ws::work_index(w).is_some() => s.work[ws::work_index(w).unwrap()] = stack,
            other => {
                if let (Some(slot), Some(mut v)) = (storage_slot(other), self.screen.storage()) {
                    v.set(slot, stack);
                }
            }
        }
    }

    fn may_place(&mut self, i: usize, stack: &Stack) -> bool {
        match self.slots[i] {
            SlotRef::Inventory(_) | SlotRef::Offhand | SlotRef::Craft(_) => true,
            SlotRef::Armor(p) => stack.item.wear_slot() == Some(p),
            SlotRef::EnchantItem => true,
            SlotRef::EnchantLapis => stack.item == Item::LAPIS_LAZULI,
            SlotRef::AnvilLeft | SlotRef::AnvilRight => {
                !matches!(*self.screen.container, Container::Grindstone(_))
                    || voxelcraft::grindstone::accepts(stack.item)
                    || !stack.enchants.is_empty()
            }
            SlotRef::SmithTemplate => crate::smithing::is_template(stack.item),
            SlotRef::SmithBase => crate::smithing::is_base(stack.item),
            SlotRef::SmithAddition => crate::smithing::is_addition(stack.item),
            r if is_result(r) => false,
            w if ws::work_index(w).is_some() => ws::accepts(w, stack),
            other => match storage_slot(other) {
                Some(slot) => self.screen.storage().is_some_and(|v| v.may_place(slot, stack)),
                None => false,
            },
        }
    }

    fn may_pickup(&mut self, i: usize) -> bool {
        match self.slots[i] {
            SlotRef::Armor(p) => self.screen.session.inventory.can_unequip(p, self.creative()),
            _ => true,
        }
    }

    fn max(&mut self, i: usize, stack: &Stack) -> u8 {
        match self.slots[i] {
            SlotRef::Armor(_) | SlotRef::EnchantItem | SlotRef::BeaconPayment => 1,
            other => storage_slot(other).map_or(stack.max(), |slot| st::slot_max(slot, stack)),
        }
    }

    fn is_result(&self, i: usize) -> bool {
        is_result(self.slots[i])
    }

    fn take(&mut self, i: usize, n: u8) -> Option<Stack> {
        let stack = self.get(i)?;
        let n = n.min(stack.count);
        self.set(i, stack.with_count_(stack.count - n));
        if let Some(p) = self.furnace_output(self.slots[i])
            && let Some(mut v) = self.screen.storage()
        {
            v.award_furnace_xp(p, stack.count);
        }
        stack.with_count_(n)
    }
}

/// A drag in progress (`isQuickCrafting`, `quickCraftingSlots`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ScreenDrag {
    pub button: winit::event::MouseButton,
    pub kind: Drag,
    pub slots: Vec<SlotRef>,
}

/// The host's container-screen mouse state (`AbstractContainerScreen`).
#[derive(Default)]
pub(super) struct ScreenMouse {
    pub drag: Option<ScreenDrag>,
    /// `lastClickSlot`, `lastClickTime` and `lastClickButton`.
    last_click: Option<(Option<SlotRef>, std::time::Instant, winit::event::MouseButton)>,
    /// `doubleclick`.
    double: bool,
    /// `lastQuickMoved`.
    pub last_quick_moved: Option<Stack>,
    /// This frame's drag preview: the dragged slots' stacks and the
    /// cursor's remaining count.
    pub preview: Option<(Vec<(SlotRef, Stack)>, u8)>,
}

/// Java's double-click window (`mouseClicked`, 250 ms).
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether a slot can be dragged over or collected from at all (a menu
/// slot, not a creative cell, button or result).
pub(super) fn real_slot(slot: SlotRef) -> bool {
    !is_result(slot)
        && !matches!(
            slot,
            SlotRef::Palette(_)
                | SlotRef::PaletteEmpty
                | SlotRef::Destroy
                | SlotRef::Trade(_)
                | SlotRef::CutRecipe(_)
                | SlotRef::LoomChoice(_)
                | SlotRef::EnchantOffer(_)
                | SlotRef::BeaconPower(_)
                | SlotRef::BeaconDone
                | SlotRef::BeaconCancel
        )
}

/// The drag a mouse button makes.
pub(super) fn drag_kind(button: winit::event::MouseButton) -> Option<Drag> {
    use winit::event::MouseButton as B;
    match button {
        B::Left => Some(Drag::Even),
        B::Right => Some(Drag::One),
        B::Middle => Some(Drag::Clone),
        _ => None,
    }
}

/// The LAN command for a drag's kind.
pub(super) fn drag_word(kind: Drag) -> &'static str {
    match kind {
        Drag::Even => "even",
        Drag::One => "one",
        Drag::Clone => "clone",
    }
}

pub(super) fn parse_drag_word(word: &str) -> Option<Drag> {
    match word {
        "even" => Some(Drag::Even),
        "one" => Some(Drag::One),
        "clone" => Some(Drag::Clone),
        _ => None,
    }
}

impl super::Game {
    /// `mouseClicked`'s double-click test and bookkeeping.
    pub(super) fn note_screen_click(&mut self, button: winit::event::MouseButton) {
        let slot = self.slot_under_cursor();
        let now = std::time::Instant::now();
        let m = &mut self.screen_mouse;
        m.double = m.last_click.is_some_and(|(s, t, b)| s == slot && now - t < DOUBLE_CLICK && b == button);
        m.last_click = Some((slot, now, button));
    }

    /// A press with a stack held over a menu slot starts a drag; the click
    /// itself waits for the release (`mouseClicked` with a carried stack).
    /// Returns whether the press is used.
    pub(super) fn begin_screen_drag(&mut self, button: winit::event::MouseButton) -> bool {
        let Some(kind) = drag_kind(button) else { return false };
        let creative = self.session.mode.is_creative();
        if self.session.inventory.cursor.is_none() {
            // Survival's middle button clones nothing (`CLONE` needs
            // infinite materials).
            return kind == Drag::Clone && !creative && !self.shows_palette();
        }
        let Some(slot) = self.slot_under_cursor().filter(|s| real_slot(*s)) else {
            return kind == Drag::Clone && !creative;
        };
        let _ = slot;
        self.note_screen_click(button);
        self.screen_mouse.drag = Some(ScreenDrag { button, kind, slots: Vec::new() });
        true
    }

    /// `mouseDragged`: the slot under the pointer joins the drag if it can.
    pub(super) fn screen_drag_moved(&mut self) {
        let Some(drag) = self.screen_mouse.drag.clone() else { return };
        let Some(slot) = self.slot_under_cursor().filter(|s| real_slot(*s)) else { return };
        if drag.slots.contains(&slot) {
            return;
        }
        let taken = drag.slots.len();
        let ok = if self.lan.client.is_some() {
            // The host checks the slot's rules; a guest checks what it sees.
            self.session.inventory.cursor.is_some_and(|held| {
                (drag.kind == Drag::Clone || held.count as usize > taken)
                    && clicks::quick_replace(self.slot_stack(slot), &held, true)
            })
        } else {
            self.with_screen(|s| s.drag_accepts(slot, drag.kind, taken))
        };
        if ok && let Some(d) = &mut self.screen_mouse.drag {
            d.slots.push(slot);
        }
    }

    /// `mouseReleased`: a double click collects (or, with shift, moves
    /// every matching stack); a drag ends; a held stack's click lands.
    pub(super) fn screen_release(&mut self, button: winit::event::MouseButton) {
        use winit::event::MouseButton as B;
        let slot = self.slot_under_cursor();
        if self.screen_mouse.double
            && button == B::Left
            && let Some(slot) = slot.filter(|s| real_slot(*s))
        {
            self.screen_mouse.double = false;
            self.screen_mouse.last_click = None;
            self.screen_mouse.drag = None;
            if self.modifiers.shift_key() {
                if let Some(last) = self.screen_mouse.last_quick_moved {
                    self.move_all_from(slot, last);
                }
            } else if self.lan.client.is_some() {
                if let Some(code) = super::lan::encode_slot(slot) {
                    self.lan_command(&format!("collect {code} 0"));
                }
            } else {
                self.with_screen(|s| s.pickup_all(slot, false));
            }
            return;
        }
        let Some(drag) = self.screen_mouse.drag.take() else { return };
        if drag.button != button {
            // `mouseReleased`: another button's release cancels the drag.
            return;
        }
        if !drag.slots.is_empty() {
            if self.lan.client.is_some() {
                let codes: Vec<String> =
                    drag.slots.iter().filter_map(|s| super::lan::encode_slot(*s)).map(|c| c.to_string()).collect();
                self.lan_command(&format!("drag {} {}", drag_word(drag.kind), codes.join(" ")));
            } else {
                self.with_screen(|s| s.drag(&drag.slots, drag.kind));
                self.audio.ui_click();
            }
        } else if drag.kind != Drag::Clone {
            self.inventory_click(button == B::Right);
        }
    }

    /// The shift double-click's quick moves, slot by slot as Java's client
    /// sends them.
    fn move_all_from(&mut self, slot: SlotRef, last: Stack) {
        if self.lan.client.is_none() {
            self.with_screen(|s| s.move_all(slot, last));
            return;
        }
        let same = section(slot);
        let slots: Vec<SlotRef> = self.inventory_slots_now();
        for s in slots {
            if section(s) == same
                && real_slot(s)
                && self.slot_stack(s).is_some_and(|st| clicks::quick_replace(Some(st), &last, true))
                && let Some(code) = super::lan::encode_slot(s)
            {
                self.lan_click(code, false, true, None);
            }
        }
    }

    /// Every slot the open screen shows.
    fn inventory_slots_now(&self) -> Vec<SlotRef> {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        self.inventory_slots((w as f32 / scale, h as f32 / scale)).into_iter().map(|(s, _, _)| s).collect()
    }

    /// The number keys (`Some(hotbar)`) or the swap-hands key (`None`) over
    /// the hovered slot, with nothing held (`checkHotbarKeyPressed`).
    pub(super) fn swap_slot_under_cursor(&mut self, hotbar: Option<usize>) {
        if self.session.inventory.cursor.is_some() {
            return;
        }
        let Some(slot) = self.slot_under_cursor() else { return };
        if self.lan.client.is_some() {
            if let Some(code) = super::lan::encode_slot(slot) {
                self.lan_command(&format!("slot_swap {code} {}", hotbar.unwrap_or(40)));
            }
            return;
        }
        self.with_screen(|s| s.swap_slot(slot, hotbar));
    }

    /// The drag preview: each dragged slot's stack once the drag ends and
    /// the cursor's remaining count (`recalculateQuickCraftRemaining`).
    pub(super) fn drag_preview(&mut self) -> Option<(Vec<(SlotRef, Stack)>, u8)> {
        let drag = self.screen_mouse.drag.clone()?;
        if drag.slots.len() < 2 || self.lan.client.is_some() {
            return None;
        }
        let p = self.with_screen(|s| s.drag_preview(&drag.slots, drag.kind))?;
        Some((p.slots.into_iter().filter_map(|(i, st)| drag.slots.get(i).map(|s| (*s, st))).collect(), p.remaining))
    }
}
