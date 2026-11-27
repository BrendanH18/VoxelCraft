//! Reliable inventory updates, paced independently of movement snapshots.

use std::collections::HashMap;
use std::sync::Arc;

use crate::crafting::Grid;
use crate::inventory::{Inventory, Stack, stack_from_str, stack_to_string};
use voxelcraft::lan::{INVENTORY_PART_BYTES, Packet};

// A full inventory of maximum-length books fits, including base64 encoding.
const MAX_INVENTORY_BYTES: usize = 32 * 1024 * 1024;
const LARGE_STACK_BYTES: usize = 4096;
// The existing inventory serialization separates stacks with these characters.
const SEPARATORS: [char; 4] = [',', '|', '#', '\n'];

fn fields(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split_inclusive(SEPARATORS).map(|part| {
        let len = part.trim_end_matches(SEPARATORS).len();
        // split_inclusive leaves exactly one separator even for an empty field.
        part.split_at(len)
    })
}

#[derive(Default)]
pub(super) struct Sender {
    captured: Option<Inventory>,
    grid: Option<Grid>,
    remaining: std::vec::IntoIter<u8>,
    large: Vec<Arc<str>>,
    pub(super) revision: u32,
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

    /// Finish the previous update before starting another so the receiver
    /// can replace its inventory atomically, even if slots change mid-send.
    pub(super) fn next(&mut self, inventory: &Inventory, grid: Grid) -> Option<Packet> {
        if self.remaining.len() == 0 {
            if self.current(inventory) && self.grid == Some(grid) {
                return None;
            }
            let cells = grid.cells[..grid.size * grid.size]
                .iter()
                .map(|&stack| stack_to_string(stack))
                .collect::<Vec<_>>()
                .join(",");
            let text =
                format!("{}\n{},{cells}\n{}", stack_to_string(inventory.cursor), grid.size, inventory.serialize());
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
            self.remaining = update.into_bytes().into_iter();
            self.captured = Some(inventory.clone());
            self.grid = Some(grid);
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
            for (field, separator) in fields(std::str::from_utf8(&pending).ok()?) {
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
