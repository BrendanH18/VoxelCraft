//! LootTable.fill ordering and Java's persistent named random sequences.
use super::chest::Chest;
use crate::{enchant::JavaRandom, inventory::Stack};
use std::collections::BTreeMap;

pub struct LootRandom(pub JavaRandom);
impl LootRandom {
    pub fn new(seed: u64) -> Self {
        Self(JavaRandom::new(seed as i64))
    }
    pub fn below(&mut self, n: u32) -> u32 {
        self.0.next_bounded(n as i32) as u32
    }
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if lo >= hi { lo } else { lo + self.below(hi - lo + 1) }
    }
    pub fn unit(&mut self) -> f64 {
        self.0.next_float() as f64
    }
    pub fn boolean(&mut self) -> bool {
        self.0.next_boolean()
    }
}
pub trait Random {
    fn bounded(&mut self, n: u32) -> u32;
    fn boolean(&mut self) -> bool;
    fn float(&mut self) -> f32;
}
impl Random for LootRandom {
    fn bounded(&mut self, n: u32) -> u32 {
        self.below(n)
    }
    fn boolean(&mut self) -> bool {
        self.boolean()
    }
    fn float(&mut self) -> f32 {
        self.0.next_float()
    }
}
pub fn shuffle<T>(items: &mut [T], rng: &mut impl Random) {
    for n in (2..=items.len()).rev() {
        let i = rng.bounded(n as u32) as usize;
        items.swap(n - 1, i);
    }
}
#[derive(Default)]
pub struct LootItems {
    pub slots: Vec<Option<Stack>>,
}
impl LootItems {
    pub fn push(&mut self, stack: Stack) {
        self.slots.push(Some(stack));
    }
    pub fn finish(self, rng: &mut impl Random, size: usize) -> Chest {
        let mut chest = Chest::default();
        fill(&mut chest, self.slots.into_iter().flatten().collect(), rng, size);
        chest
    }
}
/// Java shuffles available slots first, then splits stacks, then shuffles
/// the result; placement pops slots from the end. Existing slots survive.
pub fn fill(chest: &mut Chest, mut items: Vec<Stack>, rng: &mut impl Random, size: usize) {
    let mut slots: Vec<_> = (0..size.min(chest.slots.len())).filter(|&i| chest.slots[i].is_none()).collect();
    shuffle(&mut slots, rng);
    items.retain(|s| s.count > 0);
    let mut split = Vec::new();
    let mut singles = Vec::new();
    for s in items {
        if s.count > 1 {
            split.push(s);
        } else {
            singles.push(s);
        }
    }
    while slots.len() > singles.len() + split.len() && !split.is_empty() {
        // Mth.nextInt skips the draw when the inclusive bounds are equal.
        let i = if split.len() == 1 { 0 } else { rng.bounded(split.len() as u32) as usize };
        let mut s = split.remove(i);
        let half = s.count / 2;
        let count = if half == 1 { 1 } else { 1 + rng.bounded(half as u32) as u8 };
        s.count -= count;
        let copy = Stack { count, ..s };
        if s.count > 1 && rng.boolean() {
            split.push(s);
        } else {
            singles.push(s);
        }
        if copy.count > 1 && rng.boolean() {
            split.push(copy);
        } else {
            singles.push(copy);
        }
    }
    singles.extend(split);
    shuffle(&mut singles, rng);
    for stack in singles {
        let Some(i) = slots.pop() else { break };
        chest.slots[i] = Some(stack);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Xoroshiro {
    pub lo: u64,
    pub hi: u64,
}
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
impl Xoroshiro {
    pub fn from_state(lo: u64, hi: u64) -> Self {
        if lo | hi == 0 { Self { lo: 0x9e3779b97f4a7c15, hi: 0x6a09e667f3bcc909 } } else { Self { lo, hi } }
    }
    pub fn sequence(seed: u64, key: &str) -> Self {
        let digest = md5(key.as_bytes());
        let lo = seed ^ 0x6a09e667f3bcc909;
        Self::from_state(
            mix(lo ^ u64::from_be_bytes(digest[..8].try_into().unwrap())),
            mix(lo.wrapping_add(0x9e3779b97f4a7c15) ^ u64::from_be_bytes(digest[8..].try_into().unwrap())),
        )
    }
    pub fn next_long(&mut self) -> u64 {
        let a = self.lo;
        let mut b = self.hi;
        let result = a.wrapping_add(b).rotate_left(17).wrapping_add(a);
        b ^= a;
        self.lo = a.rotate_left(49) ^ b ^ (b << 21);
        self.hi = b.rotate_left(28);
        result
    }
    pub fn next_float(&mut self) -> f32 {
        (self.next_long() >> 40) as f32 / (1u32 << 24) as f32
    }
}
impl Random for Xoroshiro {
    fn bounded(&mut self, n: u32) -> u32 {
        assert!(n > 0);
        loop {
            let product = (self.next_long() as u32 as u64) * n as u64;
            let low = product as u32;
            if low >= n || low >= n.wrapping_neg() % n {
                return (product >> 32) as u32;
            }
        }
    }
    fn boolean(&mut self) -> bool {
        self.next_long() & 1 != 0
    }
    fn float(&mut self) -> f32 {
        self.next_float()
    }
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct RandomSequences {
    pub sequences: BTreeMap<String, Xoroshiro>,
}
impl RandomSequences {
    pub fn get(&mut self, seed: u64, key: &str) -> &mut Xoroshiro {
        self.sequences.entry(key.into()).or_insert_with(|| Xoroshiro::sequence(seed, key))
    }
    pub fn serialize(&self) -> String {
        serde_json::Value::Object(
            self.sequences.iter().map(|(k, s)| (k.clone(), serde_json::json!([s.lo, s.hi]))).collect(),
        )
        .to_string()
    }
    pub fn load(text: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let mut sequences = BTreeMap::new();
        for (k, v) in v.as_object()? {
            sequences.insert(k.clone(), Xoroshiro::from_state(v[0].as_u64()?, v[1].as_u64()?));
        }
        Some(Self { sequences })
    }
}
// RFC 1321 digest; Java RandomSupport uses MD5 bytes in big-endian longs.
fn md5(bytes: &[u8]) -> [u8; 16] {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14,
        20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6,
        10, 15, 21,
    ];
    let mut data = bytes.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&((bytes.len() as u64) * 8).to_le_bytes());
    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    for chunk in data.as_chunks::<64>().0 {
        let words: [u32; 16] = std::array::from_fn(|i| u32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap()));
        let [mut a, mut b, mut c, mut d] = state;
        for (i, &shift) in SHIFTS.iter().enumerate() {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let k = (((i + 1) as f64).sin().abs() * 4294967296.0).floor() as u32;
            let next = b.wrapping_add(a.wrapping_add(f).wrapping_add(k).wrapping_add(words[g]).rotate_left(shift));
            a = d;
            d = c;
            c = b;
            b = next;
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }
    let mut out = [0; 16];
    for (c, s) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        c.copy_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn md5_matches_rfc1321() {
        assert_eq!(
            md5(b"abc"),
            [0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1, 0x7f, 0x72]
        );
        assert_eq!(
            md5(b""),
            [0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8, 0x42, 0x7e]
        );
    }
    #[test]
    fn xoroshiro_first_step_is_derived_from_one_and_two() {
        let mut r = Xoroshiro::from_state(1, 2);
        // rotateLeft(1+2,17)+1 = 393217; xor state = 3.
        assert_eq!(r.next_long(), 393217);
        assert_eq!(r.lo, (1u64 << 49) ^ 3 ^ (3u64 << 21));
        assert_eq!(r.hi, 3u64 << 28);
    }
    #[test]
    fn java_lcg_seed_zero_matches_known_java_random_values() {
        let mut r = LootRandom::new(0);
        assert_eq!(r.0.next_int(), -1155484576);
        assert_eq!(r.0.next_int(), -723955400);
        let mut r = LootRandom::new(0);
        assert_eq!(r.below(100), 60);
        assert_eq!(r.below(100), 48);
    }
    #[test]
    fn persistent_sequences_resume_the_exact_next_draw_and_are_independent() {
        let mut s = RandomSequences::default();
        let key = "minecraft:chests/shipwreck_map";
        let first = s.get(42, key).next_long();
        let saved = s.serialize();
        let next = s.get(42, key).next_long();
        let mut loaded = RandomSequences::load(&saved).unwrap();
        assert_eq!(loaded.get(42, key).next_long(), next);
        assert_ne!(first, next);
        assert_eq!(
            s.get(42, "minecraft:chests/igloo_chest").next_long(),
            Xoroshiro::sequence(42, "minecraft:chests/igloo_chest").next_long()
        );
    }
    #[test]
    fn fill_conserves_items_keeps_occupied_slots_and_fills_visible_slots_only() {
        let mut c = Chest::default();
        c.slots[0] = Some(Stack::new(crate::item::Item::DIAMOND, 1));
        fill(&mut c, vec![Stack::new(crate::item::Item::ARROW, 14)], &mut LootRandom::new(42), 9);
        assert_eq!(c.slots[0], Some(Stack::new(crate::item::Item::DIAMOND, 1)));
        assert!(c.slots[9..].iter().all(Option::is_none));
        assert_eq!(
            c.slots
                .iter()
                .flatten()
                .filter(|s| s.item == crate::item::Item::ARROW)
                .map(|s| s.count as u32)
                .sum::<u32>(),
            14
        );
        assert!(c.slots[..9].iter().flatten().count() > 2);
    }
}

/// Resolve Java registry names without changing established player-facing names.
/// The cache keeps item/block name scans out of chunk registration.
pub fn java_item(name: &str) -> Option<crate::item::Item> {
    use super::block::Block;
    use crate::item::{ArmorMaterial, ArmorPiece, Item};
    static ITEMS: std::sync::OnceLock<BTreeMap<String, Option<Item>>> = std::sync::OnceLock::new();
    let items = ITEMS.get_or_init(|| {
        let mut out = BTreeMap::new();
        // Every entry name, including those of inline nested tables.
        fn names<'a>(v: &'a serde_json::Value, found: &mut Vec<&'a str>) {
            match v {
                serde_json::Value::Object(o) => {
                    if let Some(entries) = o.get("entries").and_then(|e| e.as_array()) {
                        found.extend(entries.iter().filter_map(|e| e["name"].as_str()));
                    }
                    o.values().for_each(|v| names(v, found));
                }
                serde_json::Value::Array(a) => a.iter().for_each(|v| names(v, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        names(&facts()["tables"], &mut found);
        {
            for name in found.into_iter().map(|s| s.trim_start_matches("minecraft:")) {
                {
                    out.entry(name.into()).or_insert_with(|| {
                        if let Some(item) = super::trial::java_item(name) {
                            return Some(item);
                        }
                        let block = match name {
                            "gold_block" => Some(Block::GOLD_BLOCK),
                            "iron_block" => Some(Block::IRON_BLOCK),
                            "diamond_block" => Some(super::stone_blocks::DIAMOND_BLOCK),
                            "emerald_block" => Some(Block::EMERALD_BLOCK),
                            "lapis_block" => Some(Block::LAPIS_BLOCK),
                            "coal_block" => Some(super::stone_blocks::COAL_BLOCK),
                            "iron_chain" => Some(Block::CHAIN),
                            "bone_block" => Some(super::gadgets::BONE_BLOCK),
                            "short_grass" => Some(Block::TALL_GRASS),
                            "snow_block" => Some(Block::SNOW),
                            "tnt" => Some(Block::TNT),
                            "pumpkin" => Some(Block::PUMPKIN),
                            _ => None,
                        };
                        block
                            .map(Item::from_block)
                            .or_else(|| match name {
                                "goat_horn" => Some(Item::GOAT_HORN),
                                "potion" => Some(Item::potion(crate::potion::Potion::WATER)),
                                "porkchop" => Some(Item::RAW_PORKCHOP),
                                "beef" => Some(Item::RAW_BEEF),
                                "chicken" => Some(Item::RAW_CHICKEN),
                                "leather_helmet" => Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather)),
                                "leather_chestplate" => {
                                    Some(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Leather))
                                }
                                "leather_leggings" => Some(Item::armor(ArmorPiece::Leggings, ArmorMaterial::Leather)),
                                "splash_potion" => Some(Item::splash_potion(crate::potion::Potion::WATER)),
                                "lingering_potion" => Some(Item::LINGERING_POTION),
                                "tipped_arrow" => Some(Item::TIPPED_ARROW),
                                _ => Item::from_name(name),
                            })
                            // Last, so a name the chain above already resolves keeps its item.
                            .or_else(|| crate::java_registry::item(name, ""))
                    });
                }
            }
        }
        out
    });
    items.get(name).copied().flatten()
}

pub fn prepare() {
    let _ = java_item("map");
}

fn facts() -> &'static serde_json::Value {
    static DATA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("loot_tables.json")).expect("structure loot facts"))
}
fn uniform(v: &serde_json::Value, rng: &mut impl Random) -> u32 {
    if let Some(n) = v.as_u64() {
        n as u32
    } else {
        let lo = v["min"].as_f64().unwrap_or(0.0).floor() as u32;
        let hi = v["max"].as_f64().unwrap_or(lo as f64).floor() as u32;
        if lo >= hi { lo } else { lo + rng.bounded(hi - lo + 1) }
    }
}
fn uniform_float(v: &serde_json::Value, rng: &mut impl Random) -> f32 {
    if let Some(n) = v.as_f64() {
        n as f32
    } else {
        let lo = v["min"].as_f64().unwrap_or(0.0) as f32;
        let hi = v["max"].as_f64().unwrap_or(lo as f64) as f32;
        lo + rng.float() * (hi - lo)
    }
}
fn enchant_options(v: &serde_json::Value) -> Vec<&str> {
    if let Some(options) = v.as_array() {
        options.iter().filter_map(|e| e.as_str().map(|s| s.trim_start_matches("minecraft:"))).collect()
    } else if let Some(single) = v.as_str().filter(|s| !s.starts_with('#')) {
        vec![single.trim_start_matches("minecraft:")]
    } else {
        facts()["enchantment_order"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect()
    }
}
fn contains(v: &serde_json::Value, name: &str) -> bool {
    v.as_array().is_some_and(|a| a.iter().any(|s| s.as_str() == Some(name)))
}
/// Enchantments the game lacks (Soul Speed) leave the item as it was, so a
/// book stays a plain book rather than an empty enchanted one.
fn apply_enchantment(stack: &mut Stack, name: &str, level: u8) {
    if let Some(e) = crate::enchant::Enchantment::from_name(name) {
        if stack.item == crate::item::Item::BOOK {
            stack.item = crate::item::Item::ENCHANTED_BOOK;
        }
        stack.enchants.set(e, level);
    }
}
fn enchant_random(stack: &mut Stack, name: &str, modifier: &serde_json::Value, rng: &mut impl Random) {
    let options: Vec<_> = enchant_options(&modifier["options"])
        .into_iter()
        .filter(|e| {
            name == "book"
                || modifier["only_compatible"].as_bool() == Some(false)
                || contains(&facts()["enchantments"][e]["supported"], name)
        })
        .collect();
    if options.is_empty() {
        return;
    }
    let e = options[rng.bounded(options.len() as u32) as usize];
    let max = facts()["enchantments"][e]["max_level"].as_u64().unwrap() as u32;
    let level = if max <= 1 { 1 } else { 1 + rng.bounded(max) };
    apply_enchantment(stack, e, level as u8);
}
fn enchant_levels(stack: &mut Stack, name: &str, modifier: &serde_json::Value, rng: &mut impl Random) {
    let mut enchantability = crate::enchant::enchantability(stack.item);
    if enchantability == 0 && (name.ends_with("_spear") || name.ends_with("_sword")) {
        enchantability = match name.split('_').next().unwrap_or("") {
            "wooden" => 15,
            "stone" => 5,
            "iron" => 14,
            "golden" => 22,
            "diamond" => 10,
            "netherite" => 15,
            "copper" => 13,
            _ => 0,
        };
    }
    let mut cost = uniform(&modifier["levels"], rng);
    if enchantability == 0 {
        return;
    }
    cost += 1 + rng.bounded(enchantability / 4 + 1) + rng.bounded(enchantability / 4 + 1);
    let span = (rng.float() + rng.float() - 1.0) * 0.15;
    cost = ((cost as f32 + cost as f32 * span + 0.5).floor() as u32).max(1);
    let mut available = Vec::new();
    for e in enchant_options(&modifier["options"]) {
        let data = &facts()["enchantments"][e];
        if name != "book" && !contains(&data["primary"], name) {
            continue;
        }
        for level in (1..=data["max_level"].as_u64().unwrap() as u8).rev() {
            let at = |key: &str| {
                data[key]["base"].as_u64().unwrap() as u32
                    + (level as u32 - 1) * data[key]["per_level_above_first"].as_u64().unwrap() as u32
            };
            if (at("min_cost")..=at("max_cost")).contains(&cost) {
                available.push((e, level, data["weight"].as_u64().unwrap() as u32));
                break;
            }
        }
    }
    let mut last = None;
    while !available.is_empty() {
        let total = available.iter().map(|e| e.2).sum();
        let mut roll = rng.bounded(total);
        for &(e, l, w) in &available {
            if roll < w {
                apply_enchantment(stack, e, l);
                last = Some(e);
                break;
            }
            roll -= w;
        }
        if rng.bounded(50) > cost {
            break;
        }
        if let Some(last) = last {
            available.retain(|(e, _, _)| {
                *e != last
                    && !contains(&facts()["enchantments"][last]["exclusive"], e)
                    && !contains(&facts()["enchantments"][e]["exclusive"], last)
            });
        }
        cost /= 2;
    }
    if stack.item == crate::item::Item::BOOK {
        stack.item = crate::item::Item::ENCHANTED_BOOK;
    }
}
/// Roll the normalized Java pools in declaration order. The resolver runs
/// exploration modifiers before the fill shuffles, so discarded maps do not
/// consume a slot or a stack-splitting draw.
pub fn table_with_random(name: &str, rng: &mut impl Random, resolve: impl FnMut(Stack) -> Option<Stack>) -> Chest {
    let items = roll_items(name, rng, resolve);
    let mut chest = Chest::default();
    let size = if name == "jungle_temple_dispenser" || name.starts_with("dispensers/") { 9 } else { 27 };
    fill(&mut chest, items, rng, size);
    chest
}

/// `LootTable.getRandomItems`: every stack a named table yields, in roll
/// order, before any container fill. Unknown tables yield nothing.
pub fn roll_items(name: &str, rng: &mut impl Random, mut resolve: impl FnMut(Stack) -> Option<Stack>) -> Vec<Stack> {
    let mut items = Vec::new();
    let table = &facts()["tables"][name];
    if table.is_object() {
        roll_table(table, rng, &mut resolve, &mut items, 0);
    }
    items
}

/// One table's pools; `loot_table` entries recurse with the same random,
/// as a nested `NestedLootTable` shares its parent's context.
fn roll_table(
    table: &serde_json::Value,
    rng: &mut impl Random,
    resolve: &mut impl FnMut(Stack) -> Option<Stack>,
    items: &mut Vec<Stack>,
    depth: u32,
) {
    for pool in table["pools"].as_array().into_iter().flatten() {
        // Pool conditions this corpus uses: `random_chance` draws a float;
        // `killed_by_player` holds for the callers that roll entity tables.
        if let Some(c) = pool.get("condition") {
            match c["type"].as_str().unwrap_or("").trim_start_matches("minecraft:") {
                "random_chance" => {
                    if rng.float() >= c["chance"].as_f64().unwrap_or(1.0) as f32 {
                        continue;
                    }
                }
                "killed_by_player" => {}
                _ => {}
            }
        }
        // The only conditional entry in this corpus requires sulfur_caves,
        // a biome this generator cannot produce. Java excludes it before
        // summing weights, rather than rolling it as an empty item.
        let entries: Vec<_> =
            pool["entries"].as_array().unwrap().iter().filter(|e| e.get("condition").is_none()).collect();
        if entries.is_empty() {
            continue;
        }
        let weight: u32 = entries.iter().map(|e| e["weight"].as_u64().unwrap_or(1) as u32).sum();
        for _ in 0..uniform(&pool["rolls"], rng) {
            let mut roll = if entries.len() == 1 { 0 } else { rng.bounded(weight) };
            let entry = entries
                .iter()
                .find(|e| {
                    let w = e["weight"].as_u64().unwrap_or(1) as u32;
                    if roll < w {
                        true
                    } else {
                        roll -= w;
                        false
                    }
                })
                .unwrap();
            if entry["type"].as_str() == Some("minecraft:loot_table") {
                if depth < 4 {
                    match &entry["value"] {
                        serde_json::Value::String(n) => {
                            let n = n.trim_start_matches("minecraft:");
                            let nested = &facts()["tables"][n.strip_prefix("chests/").unwrap_or(n)];
                            if nested.is_object() {
                                roll_table(nested, rng, resolve, items, depth + 1);
                            }
                        }
                        inline @ serde_json::Value::Object(_) => roll_table(inline, rng, resolve, items, depth + 1),
                        _ => {}
                    }
                }
                continue;
            }
            let Some(name) = entry["name"].as_str().map(|s| s.trim_start_matches("minecraft:")) else { continue };
            // Unsupported entries execute their modifiers and retain RNG draws,
            // then discard their result. Names supply compatibility even when
            // no inventory item exists.
            let item = java_item(name);
            let mut stack = Stack::new(item.unwrap_or(crate::item::Item::STICK), 1);
            let mut mods = Vec::new();
            for list in [&entry["modifier"], &pool["modifier"], &table["modifier"]] {
                match list {
                    serde_json::Value::Array(a) => mods.extend(a.iter()),
                    serde_json::Value::Object(_) => mods.push(list),
                    _ => {}
                }
            }
            let mut discard = false;
            for m in mods {
                if m.get("condition").is_some() {
                    continue;
                } // sulfur-cave-only stew effect; stew is unsupported.
                match m["type"].as_str().unwrap_or("").trim_start_matches("minecraft:") {
                    "set_count" => stack.count = uniform(&m["count"], rng).min(255) as u8,
                    "set_damage" => {
                        let remaining = uniform_float(&m["damage"], rng);
                        if let Some(max) = stack.item.durability() {
                            stack.damage = ((1.0 - remaining) * max as f32).floor() as u16;
                        }
                    }
                    "enchant_randomly" => enchant_random(&mut stack, name, m, rng),
                    "enchant_with_levels" => enchant_levels(&mut stack, name, m, rng),
                    // Fixed levels; an enchantment another track has not
                    // added yet leaves this entry blank (integration hook).
                    "set_enchantments" => {
                        for (e, level) in m["enchantments"].as_object().into_iter().flatten() {
                            let e = e.trim_start_matches("minecraft:");
                            if crate::enchant::Enchantment::from_name(e).is_none() {
                                discard = true;
                            }
                            apply_enchantment(&mut stack, e, level.as_u64().unwrap_or(1).min(255) as u8);
                        }
                    }
                    "set_potion" => {
                        if let Some(potion) = m["id"].as_str().and_then(crate::potion::Potion::from_id) {
                            match name {
                                "lingering_potion" | "tipped_arrow" => stack.entity_data = u32::from(potion.0) + 1,
                                "splash_potion" => stack.item = crate::item::Item::splash_potion(potion),
                                _ => stack.item = crate::item::Item::potion(potion),
                            }
                        } else if matches!(name, "potion" | "splash_potion" | "lingering_potion" | "tipped_arrow") {
                            // A potion this build lacks (wind charged, oozing,
                            // weaving, infested) stays blank.
                            discard = true;
                        }
                    }
                    "set_ominous_bottle_amplifier" => {
                        let amplifier = uniform(&m["amplifier"], rng).min(4) as u8;
                        stack.item = super::trial::ominous_bottle(amplifier);
                    }
                    "set_instrument" => {
                        stack.item = crate::item::Item(crate::item::Item::GOAT_HORN.0 + rng.bounded(4) as u16);
                    }
                    "exploration_map" => {
                        if let Some(mapped) = resolve(stack) {
                            stack = mapped;
                        } else {
                            discard = true;
                        }
                    }
                    "filtered"
                        if m["item_filter"]["predicates"].get("minecraft:map_id").is_some()
                            && stack.map_id.is_none() =>
                    {
                        discard = true
                    }
                    "set_stew_effect" => {
                        if let Some(effects) = m["effects"].as_array() {
                            let e = &effects[rng.bounded(effects.len() as u32) as usize];
                            let _ = uniform(&e["duration"], rng);
                        }
                    }
                    _ => {}
                }
            }
            if item.is_some() && !discard && stack.count > 0 {
                let max = stack.item.max_stack();
                while stack.count > max {
                    items.push(Stack { count: max, ..stack });
                    stack.count -= max;
                }
                items.push(stack);
            }
        }
    }
}
pub fn table(seed: u64, name: &str) -> Chest {
    table_with_random(name, &mut LootRandom::new(seed), |_| None)
}

impl super::World {
    /// A zero/absent loot seed follows the table's persistent world sequence,
    /// as LootParams.Builder.withOptionalRandomSeed does in Java.
    pub fn structure_loot(&mut self, name: &str, seed: Option<u64>, origin: glam::IVec3) -> Chest {
        if let Some(seed) = seed.filter(|&s| s != 0) {
            table_with_random(name, &mut LootRandom::new(seed), |s| {
                self.maps.explorer(&self.generator, origin, s.item, 50)
            })
        } else {
            let key = sequence_key(name);
            let mut sequence = *self.loot_sequences.get(self.generator.seed, &key);
            let chest =
                table_with_random(name, &mut sequence, |s| self.maps.explorer(&self.generator, origin, s.item, 50));
            *self.loot_sequences.get(self.generator.seed, &key) = sequence;
            chest
        }
    }

    /// `LootTable.getRandomItems` with the table's persistent named random
    /// sequence (vault rewards, trial spawner ejections).
    pub fn loot_stacks(&mut self, name: &str, origin: glam::IVec3) -> Vec<Stack> {
        let key = sequence_key(name);
        let mut sequence = *self.loot_sequences.get(self.generator.seed, &key);
        let items = roll_items(name, &mut sequence, |s| self.maps.explorer(&self.generator, origin, s.item, 50));
        *self.loot_sequences.get(self.generator.seed, &key) = sequence;
        items
    }
}

/// A table's `random_sequence`: chest tables are named without their
/// `chests/` folder here; spawner, dispenser, equipment and entity tables
/// keep theirs.
pub fn sequence_key(name: &str) -> String {
    if ["spawners/", "dispensers/", "equipment/", "entities/"].iter().any(|f| name.starts_with(f)) {
        format!("minecraft:{name}")
    } else {
        format!("minecraft:chests/{name}")
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use crate::item::Item;
    #[test]
    fn named_sequence_matches_the_independent_jdk_md5_oracle() {
        let mut r = Xoroshiro::sequence(42, "minecraft:chests/shipwreck_map");
        assert_eq!((r.lo, r.hi), (14474646034769971954, 11881435880415453716));
        for expected in [187261483733214841, 6670933311659465707, 11535633699340623698, 9528855927551693442] {
            assert_eq!(r.next_long(), expected);
        }
    }
    #[test]
    fn cartographer_pool_and_slot_order_match_jdk_random_and_collections() {
        // Reproduce: java tools/JavaLootVectors.java. This independent oracle
        // uses JDK Random, exact set_count ranges and Collections.shuffle.
        for (seed, expected) in [
            (0, vec![(26, Item::STICK, 1)]),
            (42, vec![(9, Item::PAPER, 2), (17, Item::PAPER, 2)]),
            (12345, vec![(2, Item::BREAD, 1), (13, Item::BREAD, 1), (25, Item::BREAD, 2), (26, Item::COMPASS, 1)]),
        ] {
            let c = table(seed, "village/village_cartographer");
            let observed: Vec<_> =
                c.slots.iter().enumerate().filter_map(|(i, s)| s.map(|s| (i, s.item, s.count))).collect();
            assert_eq!(observed, expected);
        }
    }
    #[test]
    fn stack_splitting_matches_independent_jdk_random_oracle() {
        let mut chest = Chest::default();
        fill(
            &mut chest,
            vec![Stack::new(Item::ARROW, 14), Stack::new(Item::PAPER, 3), Stack::new(Item::DIAMOND, 1)],
            &mut LootRandom::new(42),
            9,
        );
        let expected = [
            Some(Stack::new(Item::DIAMOND, 1)),
            None,
            Some(Stack::new(Item::PAPER, 2)),
            None,
            None,
            None,
            Some(Stack::new(Item::ARROW, 4)),
            Some(Stack::new(Item::PAPER, 1)),
            Some(Stack::new(Item::ARROW, 10)),
        ];
        assert_eq!(&chest.slots[..9], &expected);
        assert!(chest.slots[9..].iter().all(Option::is_none));
    }
    #[test]
    fn java_registry_aliases_keep_existing_loot_items() {
        assert_eq!(java_item("gold_block"), Some(Item::from_block(super::super::block::Block::GOLD_BLOCK)));
        assert_eq!(java_item("porkchop"), Some(Item::RAW_PORKCHOP));
        assert_eq!(java_item("iron_chain"), Some(Item::from_block(super::super::block::Block::CHAIN)));
        // Java's `wheat` is both the crop block and the item; loot means the item.
        assert_eq!(java_item("wheat"), Some(Item::WHEAT));
        assert_eq!(crate::java_registry::item("wheat", ""), Some(Item::WHEAT));
    }
    #[test]
    fn unseeded_tables_resume_after_world_save() {
        let g = std::sync::Arc::new(super::super::terrain::Generator::new(42));
        let mut w = super::super::World::new_headless(g.clone(), Default::default(), 1);
        let _ = w.structure_loot("village/village_cartographer", None, glam::IVec3::ZERO);
        let saved = w.loot_sequences.serialize();
        let expected = w.structure_loot("village/village_cartographer", None, glam::IVec3::ZERO);
        let mut reloaded = super::super::World::new_headless(g, Default::default(), 1);
        reloaded.loot_sequences = RandomSequences::load(&saved).unwrap();
        assert_eq!(reloaded.structure_loot("village/village_cartographer", None, glam::IVec3::ZERO), expected);
        assert_eq!(reloaded.loot_sequences, w.loot_sequences);
    }
    #[test]
    fn data_tables_have_supported_modifiers_and_ordered_pools() {
        for (name, t) in facts()["tables"].as_object().unwrap() {
            assert_eq!(t["random_sequence"], sequence_key(name));
            for p in t["pools"].as_array().unwrap() {
                for e in p["entries"].as_array().unwrap() {
                    let mut modifiers = Vec::new();
                    for list in [&e["modifier"], &p["modifier"], &t["modifier"]] {
                        match list {
                            serde_json::Value::Array(a) => modifiers.extend(a.iter()),
                            serde_json::Value::Object(_) => modifiers.push(list),
                            _ => {}
                        }
                    }
                    for m in modifiers {
                        assert!(matches!(
                            m["type"].as_str().unwrap(),
                            "minecraft:set_count"
                                | "minecraft:set_damage"
                                | "minecraft:enchant_randomly"
                                | "minecraft:enchant_with_levels"
                                | "minecraft:exploration_map"
                                | "minecraft:filtered"
                                | "minecraft:set_instrument"
                                | "minecraft:set_potion"
                                | "minecraft:set_stew_effect"
                                | "minecraft:set_ominous_bottle_amplifier"
                                | "minecraft:set_enchantments"
                        ));
                    }
                }
            }
            let chest = table(17, name);
            assert!(
                chest.slots.iter().flatten().all(|s| s.item.is_valid() && s.count > 0 && s.count <= s.item.max_stack()),
                "{name}"
            );
        }
    }
}

#[cfg(test)]
mod pool_modifier_tests {
    use super::*;
    #[test]
    fn buried_treasure_pool_modifier_makes_water_breathing_potions() {
        let potion = crate::potion::Potion::from_id("water_breathing").unwrap();
        let chest = table(42, "buried_treasure");
        assert!(chest.slots.iter().flatten().any(|s| s.item == crate::item::Item::potion(potion)));
    }
    #[test]
    fn a_single_enchantment_option_is_not_the_random_loot_tag() {
        assert_eq!(enchant_options(&serde_json::json!("minecraft:soul_speed")), ["soul_speed"]);
        assert!(enchant_options(&serde_json::json!("#minecraft:on_random_loot")).len() > 1);
        let mut book = Stack::new(crate::item::Item::BOOK, 1);
        apply_enchantment(&mut book, "soul_speed", 1);
        assert_eq!(book.item, crate::item::Item::BOOK);
    }
}
