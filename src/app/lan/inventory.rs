//! Reliable inventory updates, paced independently of movement snapshots.

use std::collections::HashMap;
use std::sync::Arc;

use crate::crafting::Grid;
use crate::inventory::{Inventory, SLOTS, Stack, components, stack_from_str, stack_to_string};
use voxelcraft::lan::{INVENTORY_PART_BYTES, Packet};

const MAX_INVENTORY_BYTES: usize = 32 * 1024 * 1024;
const LARGE_STACK_BYTES: usize = 4096;
// Cursor, grid size, up to nine grid cells, inventory slots, armor and offhand.
const MAX_FIELDS: usize = 2 + 9 + SLOTS + 5;
// The existing inventory serialization separates stacks with these characters.
const SEPARATORS: [char; 4] = [',', '|', '#', '\n'];

fn fields(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split_inclusive(SEPARATORS).map(|part| {
        let len = part.trim_end_matches(SEPARATORS).len();
        // split_inclusive leaves exactly one separator even for an empty field.
        part.split_at(len)
    })
}

fn displayed_stack(slot: &mut Option<Stack>, payload_budget: &mut usize) {
    if let Some(stack) = slot
        && let Some(payload) = stack.payload
    {
        let bytes = payload.field().len();
        if bytes > components::MAX_FIELD || bytes > *payload_budget {
            stack.payload = None;
        } else {
            *payload_budget -= bytes;
        }
    }
}

#[derive(Default)]
pub(super) struct Sender {
    captured: Option<Inventory>,
    grid: Option<Grid>,
    remaining: std::vec::IntoIter<u8>,
    large: Vec<Arc<str>>,
    pub(super) revision: u32,
    multipart_revision: Option<u32>,
}

impl Sender {
    pub(super) fn current(&self, inventory: &Inventory) -> bool {
        self.remaining.len() == 0 && self.captured.as_ref() == Some(inventory)
    }

    pub(super) fn cursor_matches(&self, revision: u32, cursor: Option<Stack>) -> bool {
        self.remaining.len() == 0
            && revision == self.revision
            && self.captured.as_ref().is_some_and(|sent| sent.cursor == cursor)
    }

    pub(super) fn held_matches(&self, slot: usize, inventory: &Inventory) -> bool {
        let held = |inv: &Inventory| {
            if slot == voxelcraft::session::books::OFFHAND_SLOT { inv.offhand } else { inv.get(slot) }
        };
        self.remaining.len() == 0
            && self.captured.as_ref().is_some_and(|sent| held(sent).is_some() && held(sent) == held(inventory))
    }

    pub(super) fn awaiting_large_update(&self, displayed_revision: u32) -> bool {
        self.multipart_revision
            .is_some_and(|revision| (1..=i32::MAX as u32).contains(&revision.wrapping_sub(displayed_revision)))
    }

    /// Finish the previous update before starting another so the receiver
    /// can replace its inventory atomically, even if slots change mid-send.
    pub(super) fn next(&mut self, inventory: &Inventory, grid: Grid) -> Option<Packet> {
        if self.remaining.len() == 0 {
            // Spill exists only on the host. It is never shown in a guest slot.
            let mut carried = inventory.without_spill();
            if self.current(&carried) && self.grid == Some(grid) {
                return None;
            }
            self.captured = Some(carried.clone());
            self.grid = Some(grid);
            let mut shown_grid = grid;
            // Reserve the actual metadata and room for delimiters and the
            // optional fields a payload adds to the stack string.
            let metadata_bytes: usize = std::iter::once(carried.cursor)
                .chain(carried.slots)
                .chain(carried.armor)
                .chain(std::iter::once(carried.offhand))
                .chain(grid.cells)
                .map(|slot| stack_to_string(slot.map(|stack| Stack { payload: None, ..stack })).len())
                .sum();
            let mut budget = (MAX_INVENTORY_BYTES - 128 * MAX_FIELDS).saturating_sub(metadata_bytes);
            for slot in std::iter::once(&mut carried.cursor)
                .chain(&mut carried.slots)
                .chain(&mut carried.armor)
                .chain(std::iter::once(&mut carried.offhand))
                .chain(&mut shown_grid.cells)
            {
                displayed_stack(slot, &mut budget);
            }
            let cells = shown_grid.cells[..grid.size * grid.size]
                .iter()
                .map(|&stack| stack_to_string(stack))
                .collect::<Vec<_>>()
                .join(",");
            let text = format!("{}\n{},{cells}\n{}", stack_to_string(carried.cursor), grid.size, carried.serialize());
            let previous = std::mem::take(&mut self.large);
            let known: HashMap<_, _> = previous.iter().enumerate().map(|(i, s)| (s.as_ref(), i)).collect();
            let mut update = String::new();
            for (stack, separator) in fields(&text) {
                if let Some(&index) = known.get(stack) {
                    update.push('@');
                    update.push_str(&index.to_string());
                    self.large.push(previous[index].clone());
                } else {
                    update.push_str(stack);
                    if stack.len() > LARGE_STACK_BYTES {
                        self.large.push(Arc::from(stack));
                    }
                }
                update.push_str(separator);
            }
            if update.len() > INVENTORY_PART_BYTES {
                self.multipart_revision = Some(self.revision.wrapping_add(1));
            }
            self.remaining = update.into_bytes().into_iter();
        }
        let bytes = self.remaining.by_ref().take(INVENTORY_PART_BYTES).collect();
        let last = self.remaining.len() == 0;
        if last {
            self.remaining = Default::default();
            self.revision = self.revision.wrapping_add(1);
        }
        Some(Packet::InventoryPart { last, bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use components::Payload;

    #[test]
    fn held_book_waits_for_its_stack_but_not_unrelated_inventory_changes() {
        let mut sender = Sender::default();
        let mut inventory = Inventory::default();
        let book =
            Stack::new(Item::WRITABLE_BOOK, 1).with_payload(Payload::WritableBook(vec!["書".repeat(1024); 100]));
        inventory.slots[0] = Some(book);
        sender.next(&inventory, Grid::new(2)).unwrap();
        assert!(sender.awaiting_large_update(0));
        assert!(!sender.held_matches(0, &inventory));
        while sender.next(&inventory, Grid::new(2)).is_some() {}
        assert!(sender.awaiting_large_update(0));
        assert!(!sender.awaiting_large_update(sender.revision));
        inventory.slots[1] = Some(Stack::new(Item::DIAMOND, 1));
        assert!(sender.held_matches(0, &inventory));
        inventory.slots[0] = None;
        assert!(!sender.held_matches(0, &inventory));
    }

    #[test]
    fn oversized_storage_keeps_its_item_and_spill_stays_on_the_host() {
        let book =
            Stack::new(Item::WRITABLE_BOOK, 1).with_payload(Payload::WritableBook(vec!["書".repeat(1024); 100]));
        let box_item = voxelcraft::content::shulker_box::item(None);
        let filled = Stack::new(box_item, 1).with_payload(Payload::Container(vec![Some(book); 27]));
        assert!(filled.payload.unwrap().field().len() > components::MAX_FIELD);
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(filled);
        inventory.slots[1] = Some(book);
        inventory.spill_stack(book);
        let mut sender = Sender::default();
        let mut receiver = Receiver::default();
        while let Some(Packet::InventoryPart { last, bytes }) = sender.next(&inventory, Grid::new(2)) {
            receiver.push(last, &bytes).unwrap();
        }
        assert_eq!(receiver.current.slots[0], Some(Stack::new(box_item, 1)));
        assert_eq!(receiver.current.slots[1], Some(book));
        assert!(receiver.current.take_spill().is_empty());
        assert_eq!(inventory.take_spill(), [book]);
    }
}

#[derive(Default)]
pub(super) struct Receiver {
    pending: Vec<u8>,
    large: Vec<Arc<str>>,
    pub(super) current: Inventory,
    pub(super) grid: Option<Grid>,
    pub(super) revision: u32,
}

impl Receiver {
    pub(super) fn push(&mut self, last: bool, bytes: &[u8]) -> Option<()> {
        if self.pending.len() + bytes.len() > MAX_INVENTORY_BYTES {
            return None;
        }
        self.pending.extend_from_slice(bytes);
        if last {
            let pending = std::mem::take(&mut self.pending);
            let mut update = String::new();
            let mut large = Vec::new();
            for (index, (field, separator)) in fields(std::str::from_utf8(&pending).ok()?).enumerate() {
                if index >= MAX_FIELDS || separator == "|" {
                    return None;
                }
                let stack = if let Some(reference) = field.strip_prefix('@') {
                    let known = self.large.get(reference.parse::<usize>().ok()?)?;
                    large.push(known.clone());
                    known
                } else {
                    if field.len() > LARGE_STACK_BYTES {
                        large.push(Arc::from(field));
                    }
                    field
                };
                if update.len() + stack.len() + separator.len() > MAX_INVENTORY_BYTES {
                    return None;
                }
                update.push_str(stack);
                update.push_str(separator);
            }
            let mut lines = update.splitn(3, '\n');
            let cursor = stack_from_str(lines.next()?)?;
            let mut cells = lines.next()?.split(',');
            let size = cells.next()?.parse::<usize>().ok().filter(|s| matches!(s, 2 | 3))?;
            let mut grid = Grid::new(size);
            for cell in &mut grid.cells[..size * size] {
                *cell = stack_from_str(cells.next()?)?;
            }
            if cells.next().is_some() {
                return None;
            }
            let mut inventory = Inventory::deserialize(lines.next()?)?;
            inventory.cursor = cursor;
            self.current = inventory;
            self.grid = Some(grid);
            self.large = large;
            self.revision = self.revision.wrapping_add(1);
        }
        Some(())
    }
}
