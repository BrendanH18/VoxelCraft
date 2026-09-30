//! Player inventory: 36 stack slots (the first 9 are the hotbar) plus the
//! stack held on the mouse cursor while the inventory screen is open.

use crate::world::block::Block;

pub const HOTBAR_SLOTS: usize = 9;
pub const SLOTS: usize = 36;
pub const MAX_STACK: u8 = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    pub block: Block,
    pub count: u8,
}

impl Stack {
    pub fn new(block: Block, count: u8) -> Self {
        Self { block, count }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub slots: [Option<Stack>; SLOTS],
    pub cursor: Option<Stack>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { slots: [None; SLOTS], cursor: None }
    }
}

impl Inventory {
    pub fn with_hotbar(blocks: &[Block]) -> Self {
        let mut inv = Self::default();
        for (slot, &b) in inv.slots.iter_mut().zip(blocks) {
            *slot = Some(Stack::new(b, MAX_STACK));
        }
        inv
    }

    pub fn get(&self, slot: usize) -> Option<Stack> {
        self.slots[slot]
    }

    /// Adds blocks, filling existing stacks first (hotbar before the main
    /// grid), then empty slots. Returns how many didn't fit.
    pub fn add(&mut self, block: Block, mut count: u8) -> u8 {
        for s in self.slots.iter_mut().flatten() {
            if s.block == block && s.count < MAX_STACK {
                let n = count.min(MAX_STACK - s.count);
                s.count += n;
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                let n = count.min(MAX_STACK);
                *slot = Some(Stack::new(block, n));
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        count
    }

    /// Removes one block from a slot, returning it.
    pub fn take_one(&mut self, slot: usize) -> Option<Block> {
        let s = self.slots[slot].as_mut()?;
        let block = s.block;
        s.count -= 1;
        if s.count == 0 {
            self.slots[slot] = None;
        }
        Some(block)
    }

    pub fn find(&self, block: Block) -> Option<usize> {
        self.slots.iter().position(|s| s.is_some_and(|s| s.block == block))
    }

    /// Minecraft-style slot click with the cursor stack. Left click picks
    /// up, places, merges or swaps whole stacks; right click picks up half
    /// or places a single item.
    pub fn click(&mut self, slot: usize, right: bool) {
        let s = &mut self.slots[slot];
        match (self.cursor, *s, right) {
            (None, None, _) => {}
            (None, Some(st), false) => {
                self.cursor = Some(st);
                *s = None;
            }
            (None, Some(st), true) => {
                let half = st.count.div_ceil(2);
                self.cursor = Some(Stack::new(st.block, half));
                *s = (st.count > half).then(|| Stack::new(st.block, st.count - half));
            }
            (Some(c), None, false) => {
                *s = Some(c);
                self.cursor = None;
            }
            (Some(c), None, true) => {
                *s = Some(Stack::new(c.block, 1));
                self.cursor = (c.count > 1).then(|| Stack::new(c.block, c.count - 1));
            }
            (Some(c), Some(st), _) if c.block == st.block => {
                let n = if right { 1.min(c.count) } else { c.count }.min(MAX_STACK - st.count);
                *s = Some(Stack::new(st.block, st.count + n));
                self.cursor = (c.count > n).then(|| Stack::new(c.block, c.count - n));
            }
            (Some(c), Some(st), _) => {
                *s = Some(c);
                self.cursor = Some(st);
            }
        }
    }

    /// Puts the cursor stack back into the inventory (when the screen
    /// closes). Anything that doesn't fit is lost.
    pub fn return_cursor(&mut self) {
        if let Some(c) = self.cursor.take() {
            self.add(c.block, c.count);
        }
    }

    /// `id:count` pairs, `-` for empty slots, comma separated.
    pub fn serialize(&self) -> String {
        self.slots
            .iter()
            .map(|s| s.map_or("-".to_string(), |s| format!("{}:{}", s.block.0, s.count)))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn deserialize(text: &str) -> Option<Self> {
        let mut inv = Self::default();
        let parts: Vec<&str> = text.trim().split(',').collect();
        if parts.len() != SLOTS {
            return None;
        }
        for (slot, part) in inv.slots.iter_mut().zip(parts) {
            if part != "-" {
                let (id, count) = part.split_once(':')?;
                let (id, count): (u8, u8) = (id.parse().ok()?, count.parse().ok()?);
                if count > 0 {
                    *slot = Some(Stack::new(Block(id), count.min(MAX_STACK)));
                }
            }
        }
        Some(inv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_fills_existing_stacks_then_empty_slots() {
        let mut inv = Inventory::default();
        assert_eq!(inv.add(Block::DIRT, 60), 0);
        assert_eq!(inv.add(Block::DIRT, 10), 0);
        assert_eq!(inv.get(0), Some(Stack::new(Block::DIRT, 64)));
        assert_eq!(inv.get(1), Some(Stack::new(Block::DIRT, 6)));
        inv.take_one(1);
        assert_eq!(inv.get(1).unwrap().count, 5);
    }

    #[test]
    fn full_inventory_reports_leftover() {
        let mut inv = Inventory::default();
        for _ in 0..SLOTS {
            inv.add(Block::STONE, 64);
        }
        assert_eq!(inv.add(Block::DIRT, 3), 3);
    }

    #[test]
    fn clicks_pick_place_merge_split_and_swap() {
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Block::DIRT, 10));
        inv.slots[1] = Some(Stack::new(Block::DIRT, 60));
        inv.slots[2] = Some(Stack::new(Block::STONE, 1));

        inv.click(0, true); // pick up half
        assert_eq!(inv.cursor, Some(Stack::new(Block::DIRT, 5)));
        assert_eq!(inv.get(0).unwrap().count, 5);

        inv.click(1, false); // merge into 60 -> 64, 1 left on cursor
        assert_eq!(inv.get(1).unwrap().count, 64);
        assert_eq!(inv.cursor, Some(Stack::new(Block::DIRT, 1)));

        inv.click(2, false); // swap with stone
        assert_eq!(inv.get(2), Some(Stack::new(Block::DIRT, 1)));
        assert_eq!(inv.cursor, Some(Stack::new(Block::STONE, 1)));

        inv.click(5, true); // place one into empty slot
        assert_eq!(inv.get(5), Some(Stack::new(Block::STONE, 1)));
        assert_eq!(inv.cursor, None);
    }

    #[test]
    fn serialization_roundtrip() {
        let mut inv = Inventory::default();
        inv.add(Block::PLANKS, 70);
        inv.slots[20] = Some(Stack::new(Block::GLASS, 3));
        assert_eq!(Inventory::deserialize(&inv.serialize()), Some(inv));
    }
}
