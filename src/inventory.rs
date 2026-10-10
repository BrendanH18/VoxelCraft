//! Player inventory: 36 stack slots (the first 9 are the hotbar), four
//! armor slots, plus the stack held on the mouse cursor while the inventory
//! screen is open.

use crate::enchant::{self, Enchants};
use crate::item::{ArmorPiece, Item};

pub const HOTBAR_SLOTS: usize = 9;
pub const SLOTS: usize = 36;
/// Plain UTF-8 bytes kept inline so [`Stack`] remains cheap to copy.
pub const STACK_NAME_MAX: usize = 64;

/// A stack's custom display name. Java stores a rich text component; this
/// compact clone keeps up to 64 UTF-8 bytes and preserves them through saves
/// and item transforms.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StackName {
    len: u8,
    bytes: [u8; STACK_NAME_MAX],
}

impl Default for StackName {
    fn default() -> Self {
        Self { len: 0, bytes: [0; STACK_NAME_MAX] }
    }
}

impl StackName {
    pub fn new(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(Self::default());
        }
        if text.len() > STACK_NAME_MAX {
            return None;
        }
        let mut name = Self { len: text.len() as u8, ..Self::default() };
        name.bytes[..text.len()].copy_from_slice(text.as_bytes());
        Some(name)
    }

    pub fn as_str(&self) -> Option<&str> {
        (self.len > 0).then(|| std::str::from_utf8(&self.bytes[..self.len as usize]).expect("StackName is valid UTF-8"))
    }

    fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(self.len as usize * 2);
        for &byte in &self.bytes[..self.len as usize] {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 15) as usize] as char);
        }
        out
    }

    fn from_hex(text: &str) -> Option<Self> {
        if !text.len().is_multiple_of(2) || text.len() / 2 > STACK_NAME_MAX {
            return None;
        }
        let nibble = |b: u8| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        };
        let mut name = Self { len: (text.len() / 2) as u8, ..Self::default() };
        for i in 0..name.len as usize {
            name.bytes[i] = nibble(text.as_bytes()[i * 2])? << 4 | nibble(text.as_bytes()[i * 2 + 1])?;
        }
        std::str::from_utf8(&name.bytes[..name.len as usize]).ok()?;
        Some(name)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    pub item: Item,
    pub count: u8,
    /// Uses taken off a tool's durability (0 for everything else).
    pub damage: u16,
    /// Enchantments (an enchanted book's are the ones it stores).
    pub enchants: Enchants,
    /// Java's anvil prior-work penalty: levels added to the next anvil use.
    pub repair_cost: u16,
    /// A plain custom display name (Java uses a styled text component).
    pub name: StackName,
    /// Captured water creature variant and health; zero means an ordinary item.
    pub entity_data: u32,
}

impl Stack {
    pub fn new(item: impl Into<Item>, count: u8) -> Self {
        Self {
            item: item.into(),
            count,
            damage: 0,
            enchants: Enchants::NONE,
            repair_cost: 0,
            name: StackName::default(),
            entity_data: 0,
        }
    }

    /// Whether `other` can merge into this stack (same item, wear,
    /// enchantments and custom name, like Java's component check).
    pub fn stacks_with(&self, other: &Stack) -> bool {
        self.item == other.item
            && self.damage == other.damage
            && self.enchants == other.enchants
            && self.repair_cost == other.repair_cost
            && self.name == other.name
            && self.entity_data == other.entity_data
    }

    /// The enchantments that take effect when held or worn: a book's are
    /// only stored.
    pub fn active_enchants(&self) -> Enchants {
        if self.item == Item::ENCHANTED_BOOK { Enchants::NONE } else { self.enchants }
    }

    pub fn max(&self) -> u8 {
        self.item.max_stack()
    }

    pub fn with_name(mut self, name: &str) -> Option<Self> {
        self.name = StackName::new(name)?;
        Some(self)
    }

    pub fn display_name(&self) -> &str {
        self.name.as_str().unwrap_or_else(|| self.item.name())
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
    /// Worn armor, indexed by [`ArmorPiece`].
    pub armor: [Option<Stack>; 4],
    /// Stacks that didn't fit back (container leftovers, the cursor): the
    /// game drops them in the world. Saved with the inventory, so a save
    /// taken with a full inventory and a crafting screen open loses nothing.
    spill: Vec<Stack>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { slots: [None; SLOTS], cursor: None, armor: [None; 4], spill: Vec::new() }
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

    /// Wears the tool in `slot` by `uses` (unbreaking skips some); returns
    /// `true` if that broke it (the slot is then empty). Non-tools are
    /// untouched.
    pub fn wear(&mut self, slot: usize, uses: u16) -> bool {
        let Some(s) = &mut self.slots[slot] else { return false };
        let Some(max) = s.item.durability() else { return false };
        let uses = (0..uses).filter(|_| enchant::wears(s.enchants, false, enchant::roll())).count() as u16;
        s.damage = s.damage.saturating_add(uses);
        if s.damage >= max {
            self.slots[slot] = None;
            return true;
        }
        false
    }

    /// Mending: experience `points` picked up repair a random damaged
    /// mending item held in `selected` or worn, two durability per point.
    /// Returns the points left for the experience bar (Java 1.21).
    pub fn mend(&mut self, mut points: u32, selected: usize) -> u32 {
        let mendable = |s: &Option<Stack>| {
            s.is_some_and(|s| {
                s.damage > 0 && s.enchants.has(enchant::Enchantment::Mending) && s.item != Item::ENCHANTED_BOOK
            })
        };
        let mut cells: Vec<&mut Option<Stack>> = self.armor.iter_mut().filter(|s| mendable(s)).collect();
        if mendable(&self.slots[selected]) {
            cells.push(&mut self.slots[selected]);
        }
        while !cells.is_empty() && points > 0 {
            let pick = ((enchant::roll() * cells.len() as f32) as usize).min(cells.len() - 1);
            let Some(stack) = cells.swap_remove(pick) else { continue };
            let repair = points.saturating_mul(2).min(stack.damage as u32);
            stack.damage -= repair as u16;
            // An odd durability point costs no XP after integer rounding.
            // Continue with other pieces even if the XP count stayed the same.
            points -= repair / 2;
        }
        points
    }

    /// Total armor points worn (0..=20).
    pub fn armor_points(&self) -> u32 {
        self.armor.iter().flatten().filter_map(|s| s.item.as_armor()).map(|(p, m)| m.defense(p) as u32).sum()
    }

    /// Total armor toughness worn (2 per diamond piece, 3 per Netherite).
    pub fn armor_toughness(&self) -> f32 {
        self.armor.iter().flatten().filter_map(|s| s.item.as_armor()).map(|(_, m)| m.toughness()).sum()
    }

    /// Share of knockback the worn armor cancels (0.1 per Netherite piece).
    pub fn knockback_resistance(&self) -> f32 {
        self.armor.iter().flatten().filter_map(|s| s.item.as_armor()).map(|(_, m)| m.knockback_resistance()).sum()
    }

    /// Puts on the armor in `slot`, swapping out whatever piece was worn
    /// there. Returns `false` if the slot holds no armor, or the worn piece
    /// is bound.
    pub fn equip(&mut self, slot: usize) -> bool {
        let Some((piece, _)) = self.slots[slot].and_then(|s| s.item.as_armor()) else { return false };
        if !self.can_unequip(piece, false) {
            return false;
        }
        std::mem::swap(&mut self.slots[slot], &mut self.armor[piece as usize]);
        true
    }

    /// Whether the worn piece can come off: curse of binding keeps it on
    /// outside creative.
    pub fn can_unequip(&self, piece: ArmorPiece, creative: bool) -> bool {
        creative || self.armor[piece as usize].is_none_or(|s| !s.enchants.has(enchant::Enchantment::BindingCurse))
    }

    /// Clicks an armor slot: only the matching piece goes in, and a bound
    /// piece stays on.
    pub fn click_armor(&mut self, piece: ArmorPiece, right: bool, creative: bool) {
        let fits = self.cursor.is_none_or(|c| c.item.as_armor().is_some_and(|(p, _)| p == piece));
        if fits && self.can_unequip(piece, creative) {
            click_slot(&mut self.armor[piece as usize], &mut self.cursor, right);
        }
    }

    /// Wears every armor piece for a hit of `damage` half hearts (a quarter
    /// of it, at least one use; unbreaking skips some). Returns the pieces
    /// that broke.
    pub fn wear_armor(&mut self, damage: f32) -> Vec<Item> {
        let uses = ((damage / 4.0) as u16).max(1);
        let mut broken = Vec::new();
        for slot in &mut self.armor {
            let Some(s) = slot else { continue };
            let Some(max) = s.item.durability() else { continue };
            let worn = (0..uses).filter(|_| enchant::wears(s.enchants, true, enchant::roll())).count() as u16;
            s.damage = s.damage.saturating_add(worn);
            if s.damage >= max {
                broken.push(s.item);
                *slot = None;
            }
        }
        broken
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

    /// Puts the cursor and container leftovers back in the inventory;
    /// whatever doesn't fit goes to the spill (see [`Inventory::take_spill`]).
    pub fn return_stacks(&mut self, stacks: impl IntoIterator<Item = Stack>) {
        for stack in self.cursor.take().into_iter().chain(stacks) {
            let left = self.add_stack(stack);
            if left > 0 {
                self.spill.push(Stack { count: left, ..stack });
            }
        }
    }

    /// Takes the stacks that didn't fit, for the game to drop.
    pub fn take_spill(&mut self) -> Vec<Stack> {
        std::mem::take(&mut self.spill)
    }

    /// Empties every slot and the cursor (a dying player drops it all).
    /// Items with curse of vanishing are destroyed instead.
    pub fn take_all(&mut self) -> Vec<Stack> {
        let mut all: Vec<Stack> = self.slots.iter_mut().chain(&mut self.armor).filter_map(Option::take).collect();
        all.extend(self.cursor.take());
        all.append(&mut self.spill);
        all.retain(|s| !s.active_enchants().has(enchant::Enchantment::VanishingCurse));
        all
    }

    /// `id:count` (or `id:count:damage` for worn tools) per slot, `-` for
    /// empty slots, comma separated. An optional `|` suffix holds the spill
    /// (older versions kept container leftovers there); saves without it
    /// still load. Worn armor follows a `#`, when there is any.
    pub fn serialize(&self) -> String {
        let mut text = self.slots.iter().map(|&s| stack_to_string(s)).collect::<Vec<_>>().join(",");
        if !self.spill.is_empty() {
            text.push('|');
            text.push_str(&self.spill.iter().map(|&s| stack_to_string(Some(s))).collect::<Vec<_>>().join(","));
        }
        if self.armor.iter().any(Option::is_some) {
            text.push('#');
            text.push_str(&self.armor.iter().map(|&s| stack_to_string(s)).collect::<Vec<_>>().join(","));
        }
        text
    }

    pub fn deserialize(text: &str) -> Option<Self> {
        let mut inv = Self::default();
        let (text, armor) = text.trim().split_once('#').unwrap_or((text.trim(), ""));
        for (slot, part) in inv.armor.iter_mut().zip(armor.split(',').filter(|s| !s.is_empty())) {
            *slot = stack_from_str(part)?;
        }
        let (slots, returns) = text.split_once('|').unwrap_or((text, ""));
        let parts: Vec<&str> = slots.split(',').collect();
        if parts.len() != SLOTS {
            return None;
        }
        for (slot, part) in inv.slots.iter_mut().zip(parts) {
            *slot = stack_from_str(part)?;
        }
        for part in returns.split(',').filter(|s| !s.is_empty()) {
            if let Some(stack) = stack_from_str(part)? {
                inv.spill.push(stack);
            }
        }
        Some(inv)
    }
}

/// `id:count` (or `id:count:damage` for worn tools, and
/// `id:count:damage:enchantments:repair cost:name` for enchanted,
/// anvil-worked or named ones; enchantments and UTF-8 name bytes are hex),
/// or `-` for nothing.
pub fn stack_to_string(stack: Option<Stack>) -> String {
    match stack {
        None => "-".to_string(),
        Some(s) if !s.enchants.is_empty() || s.repair_cost > 0 || s.name.as_str().is_some() || s.entity_data > 0 => {
            format!(
                "{}:{}:{}:{}:{}:{}:{}",
                s.item.0,
                s.count,
                s.damage,
                s.enchants.to_hex(),
                s.repair_cost,
                s.name.to_hex(),
                s.entity_data
            )
        }
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
    let enchants = fields.next().map_or(Some(Enchants::NONE), Enchants::from_hex)?;
    let repair_cost: u16 = fields.next().map_or(Some(0), |d| d.parse().ok())?;
    let name = fields.next().map_or(Some(StackName::default()), StackName::from_hex)?;
    let entity_data = fields.next().map_or(Some(0), |d| d.parse().ok())?;
    if fields.next().is_some() {
        return None;
    }
    let mut item = Item(id);
    // v0.4.0's axis-less bone block item is the axis bone block now.
    if item.block() == Some(crate::world::gadgets::LEGACY_BONE_BLOCK) {
        item = Item::from_block(crate::world::gadgets::BONE_BLOCK);
    }
    Some((count > 0 && item.is_valid()).then(|| Stack {
        item,
        count: count.min(item.max_stack()),
        damage,
        enchants,
        repair_cost,
        name,
        entity_data,
    }))
}

/// Moves as much of `stack` as fits into `slots`, visiting them in
/// `order`: onto matching stacks first, then into empty slots (shift-click).
/// Returns what's left.
pub fn move_into(stack: Stack, slots: &mut [Option<Stack>], order: &[usize]) -> Option<Stack> {
    let mut count = stack.count;
    for &i in order {
        if let Some(s) = &mut slots[i]
            && s.stacks_with(&stack)
        {
            let n = count.min(s.max().saturating_sub(s.count));
            s.count += n;
            count -= n;
        }
    }
    for &i in order {
        if count > 0 && slots[i].is_none() {
            let n = count.min(stack.max());
            slots[i] = Some(Stack { count: n, ..stack });
            count -= n;
        }
    }
    stack.with_count(count)
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

    #[test]
    fn v0_4_bone_block_items_load_as_the_axis_bone_block() {
        let legacy = Item::from_block(crate::world::gadgets::LEGACY_BONE_BLOCK);
        let stack = stack_from_str(&format!("{}:12", legacy.0)).unwrap().unwrap();
        assert_eq!(stack.item, Item::from_block(crate::world::gadgets::BONE_BLOCK));
        assert_eq!(stack.count, 12);
    }
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
        let enchants = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Efficiency, 5);
        inv.slots[23] =
            Some(Stack { enchants, repair_cost: 3, ..Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Diamond), 1) });
        inv.slots[24] = Stack::new(Item::DIAMOND, 1).with_name("Miner's \u{2728}");
        assert_eq!(inv.slots[24].unwrap().display_name(), "Miner's \u{2728}");
        assert_eq!(Inventory::deserialize(&inv.serialize()), Some(inv));
        assert!(Stack::new(Item::DIAMOND, 1).with_name(&"x".repeat(STACK_NAME_MAX + 1)).is_none());
    }

    #[test]
    fn armor_equips_wears_and_saves() {
        use crate::item::ArmorMaterial;
        let helmet = Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron);
        let boots = Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather);
        let mut inv = Inventory::with_hotbar(&[helmet, Item::STICK, boots]);
        assert!(inv.equip(0) && inv.equip(2) && !inv.equip(1));
        assert_eq!(inv.get(0), None);
        assert_eq!(inv.armor_points(), 3);

        // Only the matching piece fits a slot.
        inv.cursor = Some(Stack::new(Item::STICK, 1));
        inv.click_armor(ArmorPiece::Chestplate, false, false);
        assert_eq!(inv.armor[1], None);
        inv.cursor = None;

        let restored = Inventory::deserialize(&inv.serialize()).unwrap();
        assert_eq!(restored, inv);

        // Leather boots last 65 hits.
        for _ in 0..64 {
            assert!(inv.wear_armor(2.0).is_empty());
        }
        assert_eq!(inv.wear_armor(2.0), vec![boots]);
        assert_eq!(inv.armor_points(), 2);
        assert_eq!(inv.take_all(), vec![Stack::new(Item::STICK, 64), Stack { damage: 65, ..Stack::new(helmet, 1) }]);
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

    #[test]
    fn leftovers_that_do_not_fit_spill_and_survive_a_save() {
        let mut inv = Inventory::default();
        inv.slots.fill(Some(Stack::new(Block::STONE, 64)));
        inv.slots[0] = Some(Stack::new(Block::LOG, 63));
        inv.cursor = Some(Stack::new(Block::LOG, 10));
        let sword = Stack { damage: 30, ..Stack::new(Item::tool(ToolKind::Sword, Tier::Wood), 1) };
        inv.return_stacks([sword]);
        assert!(inv.cursor.is_none());
        assert_eq!(inv.get(0), Some(Stack::new(Block::LOG, 64)));
        assert_eq!(inv.spill, vec![Stack::new(Block::LOG, 9), sword]);

        let mut restored = Inventory::deserialize(&inv.serialize()).unwrap();
        assert_eq!(restored, inv);
        assert_eq!(restored.take_spill(), vec![Stack::new(Block::LOG, 9), sword]);
        assert!(restored.take_spill().is_empty());
        assert!(!restored.serialize().contains('|'));
    }

    #[test]
    fn move_into_tops_up_stacks_before_filling_gaps_in_order() {
        let mut slots = [None, Some(Stack::new(Block::DIRT, 60)), None, Some(Stack::new(Block::DIRT, 10))];
        let left = move_into(Stack::new(Block::DIRT, 64), &mut slots, &[3, 2, 1, 0]);
        assert_eq!(left, None);
        assert_eq!(slots.map(|s| s.map_or(0, |s| s.count)), [0, 64, 6, 64]);
        let left = move_into(Stack::new(Block::DIRT, 64), &mut slots, &[1, 3]);
        assert_eq!(left, Some(Stack::new(Block::DIRT, 64)), "nowhere to go");
    }

    #[test]
    fn mending_uses_leftover_xp_after_repairing_one_damage() {
        use crate::enchant::Enchantment;
        let mut inv = Inventory::default();
        let enchants = Enchants::NONE.with(Enchantment::Mending, 1);
        let helmet = Item::armor(ArmorPiece::Helmet, crate::item::ArmorMaterial::Iron);
        inv.armor[0] = Some(Stack { damage: 1, enchants, ..Stack::new(helmet, 1) });
        inv.slots[0] = Some(Stack { damage: 1, enchants, ..Stack::new(helmet, 1) });
        assert_eq!(inv.mend(1, 0), 1, "each odd point rounds down to zero XP used");
        assert_eq!(inv.armor[0].unwrap().damage, 0);
        assert_eq!(inv.get(0).unwrap().damage, 0, "the same orb repairs both pieces");
        inv.slots[0].as_mut().unwrap().damage = 5;
        assert_eq!(inv.mend(u32::MAX, 0), u32::MAX - 2, "large XP awards cannot overflow");
    }

    #[test]
    fn mending_vanishing_and_binding() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::item::ArmorMaterial;
        let mending = Enchants::NONE.with(Enchantment::Mending, 1);
        let sword = Item::tool(ToolKind::Sword, Tier::Iron);
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack { damage: 5, enchants: mending, ..Stack::new(sword, 1) });
        // 3 points repair 5 damage (two per point) and leave 1 point over.
        assert_eq!(inv.mend(3, 0), 1);
        assert_eq!(inv.get(0).unwrap().damage, 0);
        assert_eq!(inv.mend(3, 0), 3, "nothing left to mend");
        assert_eq!(inv.mend(3, 1), 3, "only the held slot counts");

        let helmet = Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron);
        let bound = Stack { enchants: Enchants::NONE.with(Enchantment::BindingCurse, 1), ..Stack::new(helmet, 1) };
        inv.armor[0] = Some(bound);
        inv.click_armor(ArmorPiece::Helmet, false, false);
        assert_eq!((inv.cursor, inv.armor[0]), (None, Some(bound)), "bound armor stays on");
        inv.slots[1] = Some(Stack::new(helmet, 1));
        assert!(!inv.equip(1));
        inv.click_armor(ArmorPiece::Helmet, false, true);
        assert_eq!(inv.cursor, Some(bound), "creative takes it off");

        inv.slots[2] =
            Some(Stack { enchants: Enchants::NONE.with(Enchantment::VanishingCurse, 1), ..Stack::new(sword, 1) });
        let dropped = inv.take_all();
        assert!(dropped.iter().all(|s| !s.enchants.has(Enchantment::VanishingCurse)));
        assert_eq!(dropped.len(), 3, "the vanishing sword is gone");
    }

    #[test]
    fn take_all_empties_slots_cursor_and_spill() {
        let mut inv = Inventory::with_hotbar(&[Item::STICK, Item::COAL]);
        inv.cursor = Some(Stack::new(Block::DIRT, 3));
        inv.spill.push(Stack::new(Block::SAND, 1));
        assert_eq!(inv.take_all().len(), 4);
        assert_eq!(inv, Inventory::default());
    }
}
