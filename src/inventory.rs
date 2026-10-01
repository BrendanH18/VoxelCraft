//! Player inventory: 36 stack slots (the first 9 are the hotbar) plus the
//! stack held on the mouse cursor while the inventory screen is open.

use crate::item::Item;

pub const HOTBAR_SLOTS: usize = 9;
pub const SLOTS: usize = 36;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    pub item: Item,
    pub count: u8,
    /// Uses taken off a tool's durability (0 for everything else).
    pub damage: u16,
}

impl Stack {
    pub fn new(item: impl Into<Item>, count: u8) -> Self {
        Self { item: item.into(), count, damage: 0 }
    }

    /// Whether `other` can merge into this stack (same item, same wear).
    pub fn stacks_with(&self, other: &Stack) -> bool {
        self.item == other.item && self.damage == other.damage
    }

    pub fn max(&self) -> u8 {
        self.item.max_stack()
    }

    /// Remaining durability as a fraction, for tools that have been used.
    pub fn wear(&self) -> Option<f32> {
        let max = self.item.durability()?;
        (self.damage > 0).then(|| 1.0 - self.damage as f32 / max as f32)
    }

    fn with_count(self, count: u8) -> Option<Stack> {
        (count > 0).then_some(Stack { count, ..self })
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
    pub fn with_hotbar(items: &[Item]) -> Self {
        let mut inv = Self::default();
        for (slot, &i) in inv.slots.iter_mut().zip(items) {
            *slot = Some(Stack::new(i, i.max_stack()));
        }
        inv
    }

    pub fn get(&self, slot: usize) -> Option<Stack> {
        self.slots[slot]
    }

    /// Adds new (unworn) items, filling existing stacks first (hotbar
    /// before the main grid), then empty slots. Returns how many didn't fit.
    pub fn add(&mut self, item: impl Into<Item>, count: u8) -> u8 {
        self.add_stack(Stack::new(item, count))
    }

    /// Adds a stack like [`Inventory::add`], keeping its wear. Returns how
    /// many didn't fit.
    pub fn add_stack(&mut self, stack: Stack) -> u8 {
        let max = stack.max();
        let mut count = stack.count;
        for s in self.slots.iter_mut().flatten() {
            if s.stacks_with(&stack) && s.count < max {
                let n = count.min(max - s.count);
                s.count += n;
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                let n = count.min(max);
                *slot = Some(Stack { count: n, ..stack });
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        count
    }

    /// Removes one item from a slot, returning it.
    pub fn take_one(&mut self, slot: usize) -> Option<Item> {
        let s = self.slots[slot]?;
        self.slots[slot] = s.with_count(s.count - 1);
        Some(s.item)
    }

    /// Wears the tool in `slot` by `uses`; returns `true` if that broke it
    /// (the slot is then empty). Non-tools are untouched.
    pub fn wear(&mut self, slot: usize, uses: u16) -> bool {
        let Some(s) = &mut self.slots[slot] else { return false };
        let Some(max) = s.item.durability() else { return false };
        s.damage = s.damage.saturating_add(uses);
        if s.damage >= max {
            self.slots[slot] = None;
            return true;
        }
        false
    }

    pub fn find(&self, item: impl Into<Item>) -> Option<usize> {
        let item = item.into();
        self.slots.iter().position(|s| s.is_some_and(|s| s.item == item))
    }

    /// Minecraft-style slot click with the cursor stack. Left click picks
    /// up, places, merges or swaps whole stacks; right click picks up half
    /// or places a single item.
    pub fn click(&mut self, slot: usize, right: bool) {
        click_slot(&mut self.slots[slot], &mut self.cursor, right);
    }

    /// Puts the cursor stack back into the inventory (when the screen
    /// closes). Anything that doesn't fit is lost.
    pub fn return_cursor(&mut self) {
        if let Some(c) = self.cursor.take() {
            self.add_stack(c);
        }
    }

    /// `id:count` (or `id:count:damage` for worn tools) per slot, `-` for
    /// empty slots, comma separated.
    pub fn serialize(&self) -> String {
        self.slots.iter().map(|&s| stack_to_string(s)).collect::<Vec<_>>().join(",")
    }

    pub fn deserialize(text: &str) -> Option<Self> {
        let mut inv = Self::default();
        let parts: Vec<&str> = text.trim().split(',').collect();
        if parts.len() != SLOTS {
            return None;
        }
        for (slot, part) in inv.slots.iter_mut().zip(parts) {
            *slot = stack_from_str(part)?;
        }
        Some(inv)
    }
}

/// `id:count` (or `id:count:damage` for worn tools), or `-` for nothing.
pub fn stack_to_string(stack: Option<Stack>) -> String {
    match stack {
        None => "-".to_string(),
        Some(s) if s.damage > 0 => format!("{}:{}:{}", s.item.0, s.count, s.damage),
        Some(s) => format!("{}:{}", s.item.0, s.count),
    }
}

/// Parses [`stack_to_string`]'s format; `None` if malformed. Unknown item
/// ids (e.g. from a newer version) read as an empty slot.
pub fn stack_from_str(text: &str) -> Option<Option<Stack>> {
    if text == "-" {
        return Some(None);
    }
    let mut fields = text.split(':');
    let id: u16 = fields.next()?.parse().ok()?;
    let count: u8 = fields.next()?.parse().ok()?;
    let damage: u16 = fields.next().map_or(Some(0), |d| d.parse().ok())?;
    let item = Item(id);
    Some((count > 0 && item.is_valid()).then(|| Stack { item, count: count.min(item.max_stack()), damage }))
}

/// Minecraft-style click on one slot with the cursor stack; shared by every
/// container screen. Left click picks up, places, merges or swaps whole
/// stacks; right click picks up half or places a single item.
pub fn click_slot(slot: &mut Option<Stack>, cursor: &mut Option<Stack>, right: bool) {
    match (*cursor, *slot, right) {
        (None, None, _) => {}
        (None, Some(st), false) => {
            *cursor = Some(st);
            *slot = None;
        }
        (None, Some(st), true) => {
            let half = st.count.div_ceil(2);
            *cursor = st.with_count(half);
            *slot = st.with_count(st.count - half);
        }
        (Some(c), None, false) => {
            *slot = Some(c);
            *cursor = None;
        }
        (Some(c), None, true) => {
            *slot = c.with_count(1);
            *cursor = c.with_count(c.count - 1);
        }
        (Some(c), Some(st), _) if c.stacks_with(&st) && st.max() > 1 => {
            let n = if right { 1.min(c.count) } else { c.count }.min(st.max().saturating_sub(st.count));
            *slot = st.with_count(st.count + n);
            *cursor = c.with_count(c.count - n);
        }
        (Some(c), Some(st), _) => {
            *slot = Some(c);
            *cursor = Some(st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Tier, ToolKind};
    use crate::world::block::Block;

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
        inv.slots[21] = Some(Stack::new(Item::COAL, 12));
        inv.slots[22] = Some(Stack { damage: 17, ..Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Iron), 1) });
        assert_eq!(Inventory::deserialize(&inv.serialize()), Some(inv));
    }

    #[test]
    fn old_block_only_saves_still_load() {
        let mut text = vec!["-"; SLOTS];
        text[0] = "3:64";
        text[5] = "9:2";
        let inv = Inventory::deserialize(&text.join(",")).unwrap();
        assert_eq!(inv.get(0), Some(Stack::new(Block::GRASS, 64)));
        assert_eq!(inv.get(5), Some(Stack::new(Block::COBBLESTONE, 2)));
    }

    #[test]
    fn tools_do_not_stack() {
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Stone);
        let mut inv = Inventory::default();
        assert_eq!(inv.add(pick, 2), 0);
        assert_eq!(inv.get(0), Some(Stack::new(pick, 1)));
        assert_eq!(inv.get(1), Some(Stack::new(pick, 1)));

        // Clicking one tool onto another swaps rather than merging.
        inv.click(0, false);
        inv.click(1, false);
        assert_eq!(inv.cursor, Some(Stack::new(pick, 1)));
        assert_eq!(inv.get(1), Some(Stack::new(pick, 1)));
    }

    #[test]
    fn worn_stacks_keep_their_damage() {
        let sword = Stack { damage: 30, ..Stack::new(Item::tool(ToolKind::Sword, Tier::Wood), 1) };
        let mut inv = Inventory::default();
        inv.add_stack(sword);
        assert_eq!(inv.get(0), Some(sword));
        assert!((sword.wear().unwrap() - (1.0 - 30.0 / 59.0)).abs() < 1e-6);
        assert_eq!(Stack::new(Item::COAL, 1).wear(), None);
    }

    #[test]
    fn tools_wear_out_and_break() {
        use crate::item::{Tier, ToolKind};
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Wood);
        let mut inv = Inventory::with_hotbar(&[pick, Item::STICK]);
        for _ in 0..58 {
            assert!(!inv.wear(0, 1));
        }
        assert_eq!(inv.get(0).unwrap().damage, 58);
        assert!(inv.wear(0, 1), "59 uses breaks a wooden pickaxe");
        assert_eq!(inv.get(0), None);
        assert!(!inv.wear(1, 1) && inv.get(1).unwrap().damage == 0, "sticks don't wear");
    }
}
