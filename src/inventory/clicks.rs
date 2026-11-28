//! v1.0 final track F1: Java's container click types over any screen's
//! slots, in the menu's slot order: dragging a held stack across slots
//! (`QUICK_CRAFT`) and double-clicking to collect matching items
//! (`PICKUP_ALL`).
//!
//! Java 26.3 references: `AbstractContainerMenu.doClick` (its
//! `QUICK_CRAFT` and `PICKUP_ALL` branches), `canItemQuickReplace`,
//! `getQuickCraftPlaceCount`, `isValidQuickcraftType`, `canDragTo` and
//! `canTakeItemForPickAll`, with `AbstractContainerScreen.mouseDragged`
//! and `mouseReleased` on the client. Each screen lists its slots through
//! [`Cells`]; the screens own their slot rules.

use super::Stack;

/// The three drags (`quickcraftType`): left shares the stack out evenly,
/// right places one in each slot, middle (players with infinite materials
/// only) fills each slot to a full stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    Even,
    One,
    Clone,
}

impl Drag {
    /// `isValidQuickcraftType`: the middle drag needs infinite materials.
    pub fn allowed(self, infinite_materials: bool) -> bool {
        self != Drag::Clone || infinite_materials
    }
}

/// One screen's slots, indexed in its menu order.
pub trait Cells {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn get(&mut self, i: usize) -> Option<Stack>;
    /// `Slot.setByPlayer`.
    fn set(&mut self, i: usize, stack: Option<Stack>);
    /// `Slot.mayPlace`.
    fn may_place(&mut self, i: usize, stack: &Stack) -> bool;
    /// `Slot.mayPickup`.
    fn may_pickup(&mut self, _i: usize) -> bool {
        true
    }
    /// `Slot.getMaxStackSize(stack)`.
    fn max(&mut self, _i: usize, stack: &Stack) -> u8 {
        stack.max()
    }
    /// `canDragTo` and `canTakeItemForPickAll`: false for result slots.
    fn is_result(&self, _i: usize) -> bool {
        false
    }
    /// `Slot.safeTake(count, count, player)`: takes up to `n` items,
    /// running the slot's `onTake` (a furnace output's experience, a
    /// result's crafting). The default takes from [`Cells::get`].
    fn take(&mut self, i: usize, n: u8) -> Option<Stack> {
        let stack = self.get(i)?;
        let n = n.min(stack.count);
        self.set(i, stack.with_count(stack.count - n));
        stack.with_count(n)
    }
}

/// `canItemQuickReplace(slot, stack, stackSizeMatters)`: an empty slot,
/// or the same item with room for it.
pub fn quick_replace(slot: Option<Stack>, stack: &Stack, size_matters: bool) -> bool {
    match slot {
        None => true,
        Some(s) if s.stacks_with(stack) => {
            s.count as u16 + if size_matters { 0 } else { stack.count as u16 } <= stack.max() as u16
        }
        Some(_) => false,
    }
}

/// Whether slot `i` joins a drag of `held` that has `taken` slots already
/// (`mouseDragged`, and `doClick`'s `quickcraftStatus == 1` check).
pub fn drag_accepts(cells: &mut impl Cells, i: usize, held: &Stack, kind: Drag, taken: usize) -> bool {
    i < cells.len()
        && quick_replace(cells.get(i), held, true)
        && cells.may_place(i, held)
        && (kind == Drag::Clone || held.count as usize > taken)
        && !cells.is_result(i)
}

/// `getQuickCraftPlaceCount`: how many of `held` each of `slots` slots gets.
pub fn place_count(slots: usize, kind: Drag, held: &Stack) -> u8 {
    match kind {
        Drag::Even => (held.count as usize / slots.max(1)) as u8,
        Drag::One => 1,
        Drag::Clone => held.max(),
    }
}

/// The end of a drag over two or more `slots` (`quickcraftStatus == 2`):
/// each slot is topped up by [`place_count`] to its limit and the cursor
/// keeps the rest. A one-slot drag is an ordinary click, which the caller
/// makes. Returns the slots written.
pub fn finish_drag(cells: &mut impl Cells, slots: &[usize], kind: Drag, cursor: &mut Option<Stack>) -> Vec<usize> {
    let Some(held) = *cursor else { return Vec::new() };
    let mut left = held.count as i32;
    let mut written = Vec::new();
    for &i in slots {
        // Java checks the carried stack, which stays whole until the end.
        if i >= cells.len()
            || !quick_replace(cells.get(i), &held, true)
            || !cells.may_place(i, &held)
            || !(kind == Drag::Clone || held.count as usize >= slots.len())
            || cells.is_result(i)
        {
            continue;
        }
        let had = cells.get(i).map_or(0, |s| s.count);
        let limit = held.max().min(cells.max(i, &held));
        let now = (place_count(slots.len(), kind, &held) as u16 + had as u16).min(limit as u16) as u8;
        left -= now as i32 - had as i32;
        cells.set(i, held.with_count(now));
        written.push(i);
    }
    // Creative's middle drag never uses up the held stack below zero.
    *cursor = held.with_count(left.clamp(0, u8::MAX as i32) as u8);
    written
}

/// What [`drag_preview`] shows during a drag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// Each slot's stack once the drag ends.
    pub slots: Vec<(usize, Stack)>,
    /// The cursor's count once the drag ends (`quickCraftingRemaining`).
    pub remaining: u8,
}

/// `recalculateQuickCraftRemaining`: what the slots and cursor would hold
/// if the drag ended now.
pub fn drag_preview(cells: &mut impl Cells, slots: &[usize], kind: Drag, held: Stack) -> Preview {
    let mut remaining = held.count as i32;
    let mut out = Vec::new();
    if slots.len() > 1 {
        for &i in slots {
            if i >= cells.len() || !quick_replace(cells.get(i), &held, true) || !cells.may_place(i, &held) {
                continue;
            }
            let had = cells.get(i).map_or(0, |s| s.count);
            let limit = held.max().min(cells.max(i, &held));
            let now = (place_count(slots.len(), kind, &held) as u16 + had as u16).min(limit as u16) as u8;
            remaining -= now as i32 - had as i32;
            if let Some(s) = held.with_count(now) {
                out.push((i, s));
            }
        }
    }
    Preview { slots: out, remaining: remaining.clamp(0, u8::MAX as i32) as u8 }
}

/// `PICKUP_ALL` after a double click on `clicked` with `cursor` held:
/// collects matching stacks into the cursor, part stacks first, from the
/// first slot (left button) or the last (right). Java does nothing when
/// the clicked slot still holds something it could pick up.
pub fn pickup_all(cells: &mut impl Cells, clicked: usize, right: bool, cursor: &mut Option<Stack>) {
    let Some(mut held) = *cursor else { return };
    if clicked >= cells.len() || (cells.get(clicked).is_some() && cells.may_pickup(clicked)) {
        return;
    }
    let len = cells.len();
    for pass in 0..2 {
        for step in 0..len {
            if held.count >= held.max() {
                break;
            }
            let i = if right { len - 1 - step } else { step };
            let Some(stack) = cells.get(i) else { continue };
            if !quick_replace(Some(stack), &held, true) || !cells.may_pickup(i) || cells.is_result(i) {
                continue;
            }
            if pass == 0 && stack.count == stack.max() {
                continue;
            }
            if let Some(took) = cells.take(i, held.max() - held.count) {
                held.count += took.count;
            }
        }
    }
    *cursor = Some(held);
}

/// The slots a shift double-click moves (`mouseReleased`'s shift branch):
/// every other filled slot in the same section (`slot.container`) holding
/// the item last shift-clicked, which the caller then quick-moves.
pub fn move_all_targets(
    cells: &mut impl Cells,
    section: impl Fn(usize) -> u8,
    clicked: usize,
    last: &Stack,
) -> Vec<usize> {
    let mut out = Vec::new();
    for i in 0..cells.len() {
        if section(i) == section(clicked)
            && cells.may_pickup(i)
            && cells.get(i).is_some_and(|s| quick_replace(Some(s), last, true))
        {
            out.push(i);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    struct Row {
        slots: Vec<Option<Stack>>,
        result: Option<usize>,
        locked: Option<usize>,
    }

    impl Cells for Row {
        fn len(&self) -> usize {
            self.slots.len()
        }
        fn get(&mut self, i: usize) -> Option<Stack> {
            self.slots[i]
        }
        fn set(&mut self, i: usize, stack: Option<Stack>) {
            self.slots[i] = stack;
        }
        fn may_place(&mut self, i: usize, _: &Stack) -> bool {
            Some(i) != self.result
        }
        fn may_pickup(&mut self, i: usize) -> bool {
            Some(i) != self.locked
        }
        fn is_result(&self, i: usize) -> bool {
            Some(i) == self.result
        }
    }

    fn stone() -> Item {
        Item::from_block(crate::world::block::Block::STONE)
    }

    fn dirt() -> Item {
        Item::from_block(crate::world::block::Block::DIRT)
    }

    fn st(item: Item, count: u8) -> Stack {
        Stack::new(item, count)
    }

    fn row(slots: Vec<Option<Stack>>) -> Row {
        Row { slots, result: None, locked: None }
    }

    #[test]
    fn left_drag_shares_evenly_and_keeps_the_rest() {
        let mut r = row(vec![None, None, None, Some(st(dirt(), 3))]);
        let mut cursor = Some(st(stone(), 64));
        let preview = drag_preview(&mut r, &[0, 1, 2], Drag::Even, cursor.unwrap());
        assert_eq!(preview.remaining, 1);
        finish_drag(&mut r, &[0, 1, 2], Drag::Even, &mut cursor);
        assert_eq!(r.slots[0..3], [Some(st(stone(), 21)); 3]);
        assert_eq!(cursor, Some(st(stone(), 1)));
    }

    #[test]
    fn left_drag_tops_up_to_the_stack_limit() {
        let mut r = row(vec![Some(st(stone(), 60)), None]);
        let mut cursor = Some(st(stone(), 20));
        finish_drag(&mut r, &[0, 1], Drag::Even, &mut cursor);
        assert_eq!(r.slots, vec![Some(st(stone(), 64)), Some(st(stone(), 10))]);
        assert_eq!(cursor, Some(st(stone(), 6)));
    }

    #[test]
    fn right_drag_places_one_each_and_middle_fills_without_using_any() {
        let mut r = row(vec![None, None, None]);
        let mut cursor = Some(st(stone(), 5));
        finish_drag(&mut r, &[0, 2], Drag::One, &mut cursor);
        assert_eq!(r.slots, vec![Some(st(stone(), 1)), None, Some(st(stone(), 1))]);
        assert_eq!(cursor, Some(st(stone(), 3)));
        let mut r = row(vec![None, None]);
        let mut cursor = Some(st(stone(), 1));
        finish_drag(&mut r, &[0, 1], Drag::Clone, &mut cursor);
        assert_eq!(r.slots, vec![Some(st(stone(), 64)); 2]);
        assert!(!Drag::Clone.allowed(false) && Drag::Clone.allowed(true));
    }

    #[test]
    fn drags_skip_other_items_results_and_slots_past_the_count() {
        let mut r = Row { slots: vec![Some(st(dirt(), 1)), None, None], result: Some(2), locked: None };
        let held = st(stone(), 2);
        assert!(!drag_accepts(&mut r, 0, &held, Drag::Even, 0));
        assert!(drag_accepts(&mut r, 1, &held, Drag::Even, 0));
        assert!(!drag_accepts(&mut r, 2, &held, Drag::Even, 0));
        // Two items reach two slots at most.
        assert!(!drag_accepts(&mut r, 1, &held, Drag::Even, 2));
        assert!(drag_accepts(&mut r, 1, &held, Drag::Clone, 2));
    }

    #[test]
    fn pickup_all_takes_part_stacks_first_then_full_ones() {
        let mut r =
            row(vec![Some(st(stone(), 64)), None, Some(st(stone(), 10)), Some(st(dirt(), 5)), Some(st(stone(), 30))]);
        let mut cursor = Some(st(stone(), 4));
        pickup_all(&mut r, 1, false, &mut cursor);
        assert_eq!(cursor, Some(st(stone(), 64)));
        assert_eq!(r.slots[2], None);
        assert_eq!(r.slots[4], None);
        // Only the second pass touches the full stack.
        assert_eq!(r.slots[0], Some(st(stone(), 44)));
        assert_eq!(r.slots[3], Some(st(dirt(), 5)));
    }

    #[test]
    fn pickup_all_runs_backwards_on_the_right_button_and_skips_results() {
        let mut r = Row {
            slots: vec![Some(st(stone(), 50)), None, Some(st(stone(), 50)), Some(st(stone(), 3))],
            result: Some(3),
            locked: None,
        };
        let mut cursor = Some(st(stone(), 10));
        pickup_all(&mut r, 1, true, &mut cursor);
        assert_eq!(r.slots[2], None);
        assert_eq!(r.slots[0], Some(st(stone(), 46)));
        assert_eq!(r.slots[3], Some(st(stone(), 3)));
        assert_eq!(cursor, Some(st(stone(), 64)));
    }

    #[test]
    fn pickup_all_needs_the_clicked_slot_empty() {
        let mut r = row(vec![Some(st(dirt(), 1)), Some(st(stone(), 3))]);
        let mut cursor = Some(st(stone(), 1));
        pickup_all(&mut r, 0, false, &mut cursor);
        assert_eq!(cursor, Some(st(stone(), 1)));
        // A slot that can't be picked up from counts as empty.
        r.locked = Some(0);
        pickup_all(&mut r, 0, false, &mut cursor);
        assert_eq!(cursor, Some(st(stone(), 4)));
    }

    #[test]
    fn move_all_targets_stay_in_the_clicked_section() {
        let mut r = row(vec![Some(st(stone(), 3)), Some(st(stone(), 64)), Some(st(stone(), 1)), None]);
        let targets = move_all_targets(&mut r, |i| (i >= 2) as u8, 0, &st(stone(), 1));
        // A full stack still counts (`stackSizeMatters` adds nothing).
        assert_eq!(targets, vec![0, 1]);
    }
}
