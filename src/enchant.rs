//! Enchantments, following Java Edition 1.21: each one's levels, costs,
//! weights and the items it fits; what they do in combat, mining and
//! armor; the enchanting table's offers; and the anvil's combining rules.
//!
//! A stack's enchantments are levels packed three bits apiece into one
//! `u128` ([`Enchants`]), so stacks stay `Copy` and saves stay one line.

use crate::inventory::Stack;
use crate::item::{ArmorPiece, Item, ItemKind, ToolKind};
use crate::world::block::Block;

/// Every enchantment, in the order of their save bits. Append only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum Enchantment {
    Protection,
    FireProtection,
    FeatherFalling,
    BlastProtection,
    ProjectileProtection,
    Respiration,
    AquaAffinity,
    Thorns,
    DepthStrider,
    Sharpness,
    Smite,
    BaneOfArthropods,
    Knockback,
    FireAspect,
    Looting,
    SweepingEdge,
    Efficiency,
    SilkTouch,
    Unbreaking,
    Fortune,
    Power,
    Punch,
    Flame,
    Infinity,
    Mending,
    BindingCurse,
    VanishingCurse,
}

/// Which items an enchantment goes on (Java's item tags).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fits {
    Armor,
    Head,
    Chest,
    Feet,
    Sword,
    /// Swords and axes (sharpness, smite, bane of arthropods).
    Weapon,
    /// Pickaxes, shovels, axes and hoes.
    Mining,
    Bow,
    /// Anything that wears out.
    Durable,
}

impl Fits {
    fn accepts(self, item: Item) -> bool {
        match (self, item.info().kind) {
            (Fits::Armor, ItemKind::Armor(..)) => true,
            (Fits::Head, ItemKind::Armor(p, _)) => p == ArmorPiece::Helmet,
            (Fits::Chest, ItemKind::Armor(p, _)) => p == ArmorPiece::Chestplate,
            (Fits::Feet, ItemKind::Armor(p, _)) => p == ArmorPiece::Boots,
            (Fits::Sword, ItemKind::Tool(k, _)) => k == ToolKind::Sword,
            (Fits::Weapon, ItemKind::Tool(k, _)) => matches!(k, ToolKind::Sword | ToolKind::Axe),
            (Fits::Mining, ItemKind::Tool(k, _)) => k != ToolKind::Sword,
            (Fits::Bow, ItemKind::Bow) => true,
            (Fits::Durable, _) => item.durability().is_some(),
            _ => false,
        }
    }
}

/// Enchantments that exclude each other (Java's exclusive sets).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Group {
    None,
    Protection,
    Damage,
    Mining,
    Bow,
}

/// One enchantment's definition (Java's `Enchantment.definition`).
pub struct Def {
    pub name: &'static str,
    pub max_level: u8,
    /// Relative chance among the enchantments an offer can roll.
    weight: u32,
    /// Lowest and highest modified enchanting level that rolls level 1,
    /// and how far both rise per extra level.
    min_cost: (u32, u32),
    max_cost: (u32, u32),
    /// Levels per enchantment level an anvil charges to add it.
    pub anvil_cost: u32,
    /// Treasure enchantments never come from the enchanting table.
    pub treasure: bool,
    pub curse: bool,
    supported: Fits,
    /// What the table offers it on (books take everything).
    primary: Fits,
    group: Group,
}

const fn def(
    name: &'static str,
    max_level: u8,
    weight: u32,
    min_cost: (u32, u32),
    max_cost: (u32, u32),
    anvil_cost: u32,
    supported: Fits,
) -> Def {
    Def {
        name,
        max_level,
        weight,
        min_cost,
        max_cost,
        anvil_cost,
        treasure: false,
        curse: false,
        supported,
        primary: supported,
        group: Group::None,
    }
}

const fn in_group(d: Def, group: Group) -> Def {
    Def { group, ..d }
}

const fn primary(d: Def, primary: Fits) -> Def {
    Def { primary, ..d }
}

const fn treasure(d: Def) -> Def {
    Def { treasure: true, ..d }
}

const fn curse(d: Def) -> Def {
    Def { treasure: true, curse: true, ..d }
}

/// Java 1.21's values, in [`Enchantment`] order.
static DEFS: [Def; Enchantment::COUNT] = [
    in_group(def("protection", 4, 10, (1, 11), (12, 11), 1, Fits::Armor), Group::Protection),
    in_group(def("fire protection", 4, 5, (10, 8), (18, 8), 2, Fits::Armor), Group::Protection),
    def("feather falling", 4, 5, (5, 6), (11, 6), 2, Fits::Feet),
    in_group(def("blast protection", 4, 2, (5, 8), (13, 8), 4, Fits::Armor), Group::Protection),
    in_group(def("projectile protection", 4, 5, (3, 6), (9, 6), 2, Fits::Armor), Group::Protection),
    def("respiration", 3, 2, (10, 10), (40, 10), 4, Fits::Head),
    def("aqua affinity", 1, 2, (1, 0), (41, 0), 4, Fits::Head),
    primary(def("thorns", 3, 1, (10, 20), (60, 20), 8, Fits::Armor), Fits::Chest),
    def("depth strider", 3, 2, (10, 10), (25, 10), 4, Fits::Feet),
    in_group(primary(def("sharpness", 5, 10, (1, 11), (21, 11), 1, Fits::Weapon), Fits::Sword), Group::Damage),
    in_group(primary(def("smite", 5, 5, (5, 8), (25, 8), 2, Fits::Weapon), Fits::Sword), Group::Damage),
    in_group(primary(def("bane of arthropods", 5, 5, (5, 8), (25, 8), 2, Fits::Weapon), Fits::Sword), Group::Damage),
    def("knockback", 2, 5, (5, 20), (55, 20), 2, Fits::Sword),
    def("fire aspect", 2, 2, (10, 20), (60, 20), 4, Fits::Sword),
    def("looting", 3, 2, (15, 9), (65, 9), 4, Fits::Sword),
    def("sweeping edge", 3, 2, (5, 9), (20, 9), 4, Fits::Sword),
    def("efficiency", 5, 10, (1, 10), (51, 10), 1, Fits::Mining),
    in_group(def("silk touch", 1, 1, (15, 0), (65, 0), 8, Fits::Mining), Group::Mining),
    def("unbreaking", 3, 5, (5, 8), (55, 8), 2, Fits::Durable),
    in_group(def("fortune", 3, 2, (15, 9), (65, 9), 4, Fits::Mining), Group::Mining),
    def("power", 5, 10, (1, 10), (16, 10), 1, Fits::Bow),
    def("punch", 2, 2, (12, 20), (37, 20), 4, Fits::Bow),
    def("flame", 1, 2, (20, 0), (50, 0), 4, Fits::Bow),
    in_group(def("infinity", 1, 1, (20, 0), (50, 0), 8, Fits::Bow), Group::Bow),
    in_group(treasure(def("mending", 1, 2, (25, 25), (75, 25), 4, Fits::Durable)), Group::Bow),
    curse(def("curse of binding", 1, 1, (25, 0), (50, 0), 8, Fits::Armor)),
    curse(def("curse of vanishing", 1, 1, (25, 0), (50, 0), 8, Fits::Durable)),
];

impl Enchantment {
    pub const COUNT: usize = 27;
    pub const ALL: [Enchantment; Enchantment::COUNT] = {
        let mut all = [Enchantment::Protection; Enchantment::COUNT];
        let mut i = 0;
        while i < Enchantment::COUNT {
            // SAFETY: `Enchantment` is `repr(u8)` with variants 0..COUNT.
            all[i] = unsafe { std::mem::transmute::<u8, Enchantment>(i as u8) };
            i += 1;
        }
        all
    };

    pub fn def(self) -> &'static Def {
        &DEFS[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Enchantment> {
        let name = name.to_lowercase().replace('_', " ");
        // Java's ids name the curses "binding curse" and "vanishing curse".
        let name = match name.as_str() {
            "binding curse" => "curse of binding",
            "vanishing curse" => "curse of vanishing",
            n => n,
        };
        Enchantment::ALL.into_iter().find(|e| e.def().name == name)
    }

    /// Whether this can go on `item` at all (anvils, `/enchant`).
    pub fn fits(self, item: Item) -> bool {
        item == Item::ENCHANTED_BOOK || item == Item::BOOK || self.def().supported.accepts(item)
    }

    /// Whether `self` and `other` may share an item.
    pub fn compatible(self, other: Enchantment) -> bool {
        let (a, b) = (self.def().group, other.def().group);
        self != other && (a == Group::None || a != b)
    }

    /// Modified enchanting level range that rolls `level` of this.
    fn cost_range(self, level: u8) -> (u32, u32) {
        let d = self.def();
        let extra = level.saturating_sub(1) as u32;
        (d.min_cost.0 + d.min_cost.1 * extra, d.max_cost.0 + d.max_cost.1 * extra)
    }

    /// `Sharpness V`, `Mending` (single-level ones drop the numeral).
    pub fn describe(self, level: u8) -> String {
        let name = capitalize_words(self.def().name);
        if self.def().max_level == 1 && level == 1 { name } else { format!("{name} {}", roman(level)) }
    }
}

fn capitalize_words(text: &str) -> String {
    text.split(' ')
        .map(|w| match w {
            "of" => w.to_string(),
            _ => w[..1].to_uppercase() + &w[1..],
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn roman(level: u8) -> String {
    const NUMERALS: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    NUMERALS.get(level.wrapping_sub(1) as usize).map_or_else(|| level.to_string(), |s| s.to_string())
}

/// A stack's enchantment levels, three bits each by [`Enchantment`] index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Enchants(pub u128);

const BITS: u32 = 3;
const LEVEL_MASK: u128 = (1 << BITS) - 1;

impl Enchants {
    pub const NONE: Enchants = Enchants(0);

    pub fn level(self, e: Enchantment) -> u8 {
        ((self.0 >> (e as u32 * BITS)) & LEVEL_MASK) as u8
    }

    pub fn has(self, e: Enchantment) -> bool {
        self.level(e) > 0
    }

    /// Sets a level (0 removes it; levels saturate at 7).
    pub fn set(&mut self, e: Enchantment, level: u8) {
        let shift = e as u32 * BITS;
        self.0 = (self.0 & !(LEVEL_MASK << shift)) | ((level.min(7) as u128) << shift);
    }

    pub fn with(mut self, e: Enchantment, level: u8) -> Self {
        self.set(e, level);
        self
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Each enchantment present and its level, in definition order.
    pub fn iter(self) -> impl Iterator<Item = (Enchantment, u8)> {
        Enchantment::ALL.into_iter().map(move |e| (e, self.level(e))).filter(|&(_, l)| l > 0)
    }

    /// Lowercase hex, for saves (empty when there are none).
    pub fn to_hex(self) -> String {
        if self.is_empty() { String::new() } else { format!("{:x}", self.0) }
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(Self::NONE);
        }
        // Bits past the last known enchantment (a newer save) are dropped.
        let known = (1u128 << (Enchantment::COUNT as u32 * BITS)) - 1;
        u128::from_str_radix(text, 16).ok().map(|v| Enchants(v & known))
    }

    /// Tooltip lines: `Sharpness V`, and whether each is a curse (red).
    pub fn lines(self) -> Vec<(String, bool)> {
        self.iter().map(|(e, l)| (e.describe(l), e.def().curse)).collect()
    }
}

/// Java's `java.util.Random` (and Minecraft's legacy random source): the
/// enchanting table's offers come from it, seeded per player.
pub struct JavaRandom(u64);

const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const MASK: u64 = (1 << 48) - 1;

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        Self((seed as u64 ^ MULTIPLIER) & MASK)
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(MULTIPLIER).wrapping_add(0xB)) & MASK;
        (self.0 >> (48 - bits)) as u32 as i32
    }

    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// Uniform in `0..bound` (`bound` > 0).
    pub fn next_bounded(&mut self, bound: i32) -> i32 {
        if (bound as u32).is_power_of_two() {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits.wrapping_sub(val).wrapping_add(bound - 1) >= 0 {
                return val;
            }
        }
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }
}

/// A uniform roll in `0..1` for enchantment chances (unbreaking, mending)
/// that don't need a seeded source.
pub fn roll() -> f32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let mut z = STATE.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// How readily an item takes enchantments at the table (Java's
/// `enchantable` values); 0 means it can't be enchanted there.
pub fn enchantability(item: Item) -> u32 {
    use crate::item::{ArmorMaterial, Tier};
    match item.info().kind {
        ItemKind::Tool(_, tier) => match tier {
            Tier::Wood => 15,
            Tier::Stone => 5,
            Tier::Iron => 14,
            Tier::Gold => 22,
            Tier::Diamond => 10,
        },
        ItemKind::Armor(_, material) => match material {
            ArmorMaterial::Leather => 15,
            ArmorMaterial::Iron => 9,
            ArmorMaterial::Gold => 25,
            ArmorMaterial::Diamond => 10,
        },
        ItemKind::Bow => 1,
        _ if item == Item::BOOK => 1,
        _ => 0,
    }
}

/// Java's `EnchantingTableBlock.isValidBookShelf`: bookshelves on the ring
/// two blocks out, at the table's level or one up, with nothing solid in
/// between (at the same height). At most 15 count.
pub fn bookshelves(world: &crate::world::World, table: glam::IVec3) -> u32 {
    let mut n = 0;
    for y in 0..=1 {
        for z in -2..=2i32 {
            for x in -2..=2i32 {
                if x.abs() != 2 && z.abs() != 2 {
                    continue;
                }
                let shelf = world.get_block(table + glam::IVec3::new(x, y, z)) == Some(Block::BOOKSHELF);
                let between = world.get_block(table + glam::IVec3::new(x / 2, y, z / 2));
                if shelf && between.is_some_and(Block::is_replaceable) {
                    n += 1;
                }
            }
        }
    }
    n.min(15)
}

/// Whether the enchanting table takes this stack (one unenchanted item).
pub fn table_accepts(stack: Stack) -> bool {
    enchantability(stack.item) > 0 && stack.enchants.is_empty()
}

/// Java's `getEnchantmentCost`: the level cost of offer `slot` (0..3) with
/// `bookshelves` around the table.
fn offer_cost(rng: &mut JavaRandom, slot: usize, bookshelves: u32, item: Item) -> u32 {
    if enchantability(item) == 0 {
        return 0;
    }
    let shelves = bookshelves.min(15) as i32;
    let base = rng.next_bounded(8) + 1 + (shelves >> 1) + rng.next_bounded(shelves + 1);
    let cost = match slot {
        0 => (base / 3).max(1),
        1 => base * 2 / 3 + 1,
        _ => base.max(shelves * 2),
    };
    cost as u32
}

/// Java's `getAvailableEnchantmentResults`: the best level of each
/// table enchantment that a modified level of `power` can roll on `item`.
fn available(power: u32, item: Item) -> Vec<(Enchantment, u8)> {
    let book = item == Item::BOOK;
    let mut out = Vec::new();
    for e in Enchantment::ALL {
        let d = e.def();
        if d.treasure || !(book || d.primary.accepts(item)) {
            continue;
        }
        if let Some(level) = (1..=d.max_level).rev().find(|&l| {
            let (lo, hi) = e.cost_range(l);
            (lo..=hi).contains(&power)
        }) {
            out.push((e, level));
        }
    }
    out
}

/// Java's `WeightedRandom.getRandomItem`.
fn weighted_pick(rng: &mut JavaRandom, list: &[(Enchantment, u8)]) -> Option<(Enchantment, u8)> {
    let total: u32 = list.iter().map(|(e, _)| e.def().weight).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.next_bounded(total as i32);
    for &(e, l) in list {
        roll -= e.def().weight as i32;
        if roll < 0 {
            return Some((e, l));
        }
    }
    None
}

/// Java's `selectEnchantment`: rolls the enchantments for an offer of
/// `cost` levels.
fn select(rng: &mut JavaRandom, item: Item, cost: u32) -> Vec<(Enchantment, u8)> {
    let ench = enchantability(item) as i32;
    let mut picked = Vec::new();
    if ench == 0 {
        return picked;
    }
    let mut level = cost as i32 + 1 + rng.next_bounded(ench / 4 + 1) + rng.next_bounded(ench / 4 + 1);
    let f = (rng.next_float() + rng.next_float() - 1.0) * 0.15;
    level = ((level as f32 + level as f32 * f + 0.5).floor() as i32).max(1);
    let mut options = available(level as u32, item);
    if let Some(first) = weighted_pick(rng, &options) {
        picked.push(first);
        while rng.next_bounded(50) <= level {
            let (last, _) = *picked.last().expect("picked one");
            options.retain(|&(e, _)| last.compatible(e));
            match weighted_pick(rng, &options) {
                Some(next) => picked.push(next),
                None => break,
            }
            level /= 2;
        }
    }
    picked
}

/// Java's `getEnchantmentList`: what offer `slot` will apply, seeded by the
/// player's enchantment seed. A book loses one of several at random.
pub fn offer_enchants(seed: i32, item: Item, slot: usize, cost: u32) -> Vec<(Enchantment, u8)> {
    let mut rng = JavaRandom::new(seed.wrapping_add(slot as i32) as i64);
    offer_list(&mut rng, item, cost)
}

fn offer_list(rng: &mut JavaRandom, item: Item, cost: u32) -> Vec<(Enchantment, u8)> {
    let mut list = select(rng, item, cost);
    if item == Item::BOOK && list.len() > 1 {
        list.remove(rng.next_bounded(list.len() as i32) as usize);
    }
    list
}

/// The table's three offers: level cost (0 = none) and the one
/// enchantment it shows as a clue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Offer {
    pub cost: u32,
    pub clue: Option<(Enchantment, u8)>,
}

/// Java's `EnchantmentMenu.slotsChanged`: the offers for `item` with
/// `bookshelves` around the table.
pub fn offers(seed: i32, item: Item, bookshelves: u32) -> [Offer; 3] {
    let mut rng = JavaRandom::new(seed as i64);
    let mut out = [Offer::default(); 3];
    for (slot, o) in out.iter_mut().enumerate() {
        o.cost = offer_cost(&mut rng, slot, bookshelves, item);
        if o.cost < slot as u32 + 1 {
            o.cost = 0;
        }
    }
    for (slot, o) in out.iter_mut().enumerate() {
        if o.cost > 0 {
            // Java reseeds this same source for the roll, then draws the
            // clue from its remaining state (not the cost-generation state).
            let mut rng = JavaRandom::new(seed.wrapping_add(slot as i32) as i64);
            let list = offer_list(&mut rng, item, o.cost);
            if !list.is_empty() {
                o.clue = Some(list[rng.next_bounded(list.len() as i32) as usize]);
            }
        }
    }
    out
}

/// The anvil's result for `left` combined with `right` (Java's
/// `AnvilMenu.createResult`, without renaming): the new stack, its level
/// cost, and how many of `right` it uses up (`None` = all).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AnvilResult {
    pub output: Stack,
    pub cost: u32,
    pub uses: Option<u8>,
}

/// Levels at or above which survival anvils refuse ("Too Expensive!").
pub const TOO_EXPENSIVE: u32 = 40;

pub fn anvil(left: Stack, right: Option<Stack>, creative: bool) -> Option<AnvilResult> {
    anvil_any_cost(left, right, creative).filter(|r| creative || r.cost < TOO_EXPENSIVE)
}

/// [`anvil`] without the survival cost limit, for showing "Too Expensive!".
pub fn anvil_any_cost(left: Stack, right: Option<Stack>, creative: bool) -> Option<AnvilResult> {
    let right = right?;
    let mut out = left;
    let base = left.repair_cost as u32 + right.repair_cost as u32;
    let mut cost = 0u32;
    let mut uses = None;
    let book = right.item == Item::ENCHANTED_BOOK && !right.enchants.is_empty();
    let max = left.item.durability();
    if let Some(max) = max
        && repairs(left.item, right.item)
    {
        let mut fix = out.damage.min(max / 4);
        if fix == 0 {
            return None;
        }
        let mut used = 0;
        while fix > 0 && used < right.count {
            out.damage -= fix;
            cost += 1;
            used += 1;
            fix = out.damage.min(max / 4);
        }
        uses = Some(used);
    } else {
        if !(book || (left.item == right.item && max.is_some()) || left.item == Item::ENCHANTED_BOOK && book) {
            return None;
        }
        if let Some(max) = max
            && !book
        {
            let left_rest = max - left.damage.min(max);
            let right_rest = max - right.damage.min(max);
            let rest = left_rest as u32 + right_rest as u32 + max as u32 * 12 / 100;
            let damage = (max as u32).saturating_sub(rest) as u16;
            if damage < out.damage {
                out.damage = damage;
                cost += 2;
            }
        }
        let (mut any_fit, mut any_clash) = (false, false);
        for (e, level) in right.enchants.iter() {
            let have = out.enchants.level(e);
            let mut new = if have == level { level + 1 } else { have.max(level) };
            let mut fits = e.def().supported.accepts(left.item) || creative || left.item == Item::ENCHANTED_BOOK;
            for (other, _) in out.enchants.iter() {
                if other != e && !e.compatible(other) {
                    fits = false;
                    cost += 1;
                }
            }
            if !fits {
                any_clash = true;
                continue;
            }
            any_fit = true;
            new = new.min(e.def().max_level);
            out.enchants.set(e, new);
            let per = if book { (e.def().anvil_cost / 2).max(1) } else { e.def().anvil_cost };
            cost += per * new as u32;
            if left.count > 1 {
                cost = TOO_EXPENSIVE;
            }
        }
        if any_clash && !any_fit {
            return None;
        }
    }
    if cost == 0 {
        return None;
    }
    let total = base + cost;
    let prior = left.repair_cost.max(right.repair_cost) as u32;
    out.repair_cost = (prior * 2 + 1).min(u16::MAX as u32) as u16;
    Some(AnvilResult { output: out, cost: total, uses })
}

/// Java's anvil wear after a use: a 12% chance it turns chipped, then
/// damaged, then breaks. Returns `Some(broke)` if it changed.
pub fn wear_anvil(world: &mut crate::world::World, pos: glam::IVec3) -> Option<bool> {
    let anvil = world.get_block(pos).filter(|b| b.is_anvil())?;
    if roll() >= 0.12 {
        return None;
    }
    let next = anvil.anvil_damaged();
    world.set_block(pos, next.unwrap_or(Block::AIR));
    Some(next.is_none())
}

/// Whether `material` repairs `item` on an anvil (Java's repair tags).
pub fn repairs(item: Item, material: Item) -> bool {
    use crate::item::{ArmorMaterial, Tier};
    match item.info().kind {
        ItemKind::Tool(_, tier) => match tier {
            Tier::Wood => material.block().is_some_and(|b| b.is_planks()),
            Tier::Stone => material == Item::from(Block::COBBLESTONE),
            Tier::Iron => material == Item::IRON_INGOT,
            Tier::Gold => material == Item::GOLD_INGOT,
            Tier::Diamond => material == Item::DIAMOND,
        },
        ItemKind::Armor(_, m) => match m {
            ArmorMaterial::Leather => material == Item::LEATHER,
            ArmorMaterial::Iron => material == Item::IRON_INGOT,
            ArmorMaterial::Gold => material == Item::GOLD_INGOT,
            ArmorMaterial::Diamond => material == Item::DIAMOND,
        },
        _ => false,
    }
}

/// Java's `/enchant` on the held stack: the level can't pass the
/// enchantment's maximum, the item must take it and it must not clash with
/// what's already there. A plain book becomes an enchanted book holding it.
pub fn command(held: Option<Stack>, e: Enchantment, level: u8) -> Result<Stack, String> {
    let mut stack = held.ok_or("no item held")?;
    let name = e.describe(level);
    if level > e.def().max_level {
        return Err(format!("{level} is higher than the maximum level of {} for {}", e.def().max_level, e.def().name));
    }
    if stack.item == Item::BOOK && stack.count == 1 {
        stack.item = Item::ENCHANTED_BOOK;
    } else if stack.item != Item::ENCHANTED_BOOK && !e.fits(stack.item) || stack.item == Item::BOOK {
        return Err(format!("{} cannot support {name}", stack.item.name()));
    }
    if let Some((other, _)) = stack.enchants.iter().find(|&(o, _)| !e.compatible(o) && o != e) {
        return Err(format!("{name} conflicts with {}", other.describe(stack.enchants.level(other))));
    }
    stack.enchants.set(e, level);
    Ok(stack)
}

/// Mobs that smite and bane of arthropods hit harder.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Creature {
    Undead,
    Arthropod,
    Other,
}

/// Extra melee damage from the held weapon's enchantments (Java 1.21:
/// sharpness 0.5 per level + 0.5; smite and bane 2.5 per level).
pub fn damage_bonus(held: Enchants, target: Creature) -> f32 {
    let sharp = held.level(Enchantment::Sharpness);
    let mut bonus = if sharp > 0 { 0.5 * sharp as f32 + 0.5 } else { 0.0 };
    bonus += match target {
        Creature::Undead => 2.5 * held.level(Enchantment::Smite) as f32,
        Creature::Arthropod => 2.5 * held.level(Enchantment::BaneOfArthropods) as f32,
        Creature::Other => 0.0,
    };
    bonus
}

/// Extra mining speed from efficiency (Java: level² + 1, on the right tool).
pub fn efficiency_bonus(held: Enchants) -> f32 {
    let level = held.level(Enchantment::Efficiency) as f32;
    if level > 0.0 { level * level + 1.0 } else { 0.0 }
}

/// Whether a use wears the item: unbreaking skips most of them (tools and
/// weapons wear 1 in (level + 1) times; armor 60% + 40% / (level + 1)).
pub fn wears(enchants: Enchants, armor: bool, roll: f32) -> bool {
    let level = enchants.level(Enchantment::Unbreaking) as f32;
    if level == 0.0 {
        return true;
    }
    let chance = if armor { 0.6 + 0.4 / (level + 1.0) } else { 1.0 / (level + 1.0) };
    roll < chance
}

/// The kind of damage, for the protection enchantments, read from the
/// death message the damage carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageKind {
    Fall,
    Fire,
    Explosion,
    Projectile,
    /// A fireball: both fire and a projectile.
    Fireball,
    Other,
}

impl DamageKind {
    pub fn of_cause(cause: &str) -> DamageKind {
        match cause {
            c if c.starts_with("fell") || c.starts_with("hit the ground") => DamageKind::Fall,
            c if c.contains("fireball") => DamageKind::Fireball,
            c if c.starts_with("burned") || c.contains("lava") => DamageKind::Fire,
            c if c.contains("blown up") || c.contains("Intentional Game Design") => DamageKind::Explosion,
            c if c.starts_with("was shot") => DamageKind::Projectile,
            _ => DamageKind::Other,
        }
    }
}

/// Java's enchantment protection factor of the worn armor against `kind`:
/// protection 1 per level, the specific kinds 2, feather falling 3.
pub fn protection_factor(armor: &[Option<Stack>; 4], kind: DamageKind) -> u32 {
    use Enchantment as E;
    let mut epf = 0;
    for e in armor.iter().flatten().map(|s| s.enchants) {
        epf += e.level(E::Protection) as u32;
        let fire = matches!(kind, DamageKind::Fire | DamageKind::Fireball);
        let projectile = matches!(kind, DamageKind::Projectile | DamageKind::Fireball);
        epf += 2 * e.level(E::FireProtection) as u32 * fire as u32;
        epf += 2 * e.level(E::BlastProtection) as u32 * (kind == DamageKind::Explosion) as u32;
        epf += 2 * e.level(E::ProjectileProtection) as u32 * projectile as u32;
        epf += 3 * e.level(E::FeatherFalling) as u32 * (kind == DamageKind::Fall) as u32;
    }
    epf
}

/// Damage left after enchantment protection (capped at 80%).
pub fn protect(amount: f32, armor: &[Option<Stack>; 4], cause: &str) -> f32 {
    let epf = protection_factor(armor, DamageKind::of_cause(cause)).min(20);
    amount * (1.0 - epf as f32 / 25.0)
}

/// The highest level of `e` across the worn armor.
pub fn armor_level(armor: &[Option<Stack>; 4], e: Enchantment) -> u8 {
    armor.iter().flatten().map(|s| s.enchants.level(e)).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ArmorMaterial, Tier};

    fn sword() -> Item {
        Item::tool(ToolKind::Sword, Tier::Diamond)
    }

    #[test]
    fn levels_pack_and_round_trip() {
        let mut e = Enchants::NONE.with(Enchantment::Sharpness, 5).with(Enchantment::VanishingCurse, 1);
        assert_eq!(e.level(Enchantment::Sharpness), 5);
        assert_eq!(e.level(Enchantment::Smite), 0);
        e.set(Enchantment::Sharpness, 3);
        assert_eq!(e.iter().collect::<Vec<_>>(), vec![(Enchantment::Sharpness, 3), (Enchantment::VanishingCurse, 1)]);
        assert_eq!(Enchants::from_hex(&e.to_hex()), Some(e));
        assert_eq!(Enchants::from_hex(""), Some(Enchants::NONE));
        assert_eq!(Enchants::from_hex("zz"), None);
        assert_eq!(Enchantment::ALL[26], Enchantment::VanishingCurse);
    }

    #[test]
    fn names_and_numerals() {
        assert_eq!(Enchantment::Sharpness.describe(5), "Sharpness V");
        assert_eq!(Enchantment::Mending.describe(1), "Mending");
        assert_eq!(Enchantment::BaneOfArthropods.describe(2), "Bane of Arthropods II");
        assert_eq!(Enchantment::from_name("silk_touch"), Some(Enchantment::SilkTouch));
        assert_eq!(Enchantment::from_name("binding_curse"), Some(Enchantment::BindingCurse));
    }

    #[test]
    fn java_random_matches_the_jdk() {
        // new java.util.Random(42): nextInt() = -1170105035,
        // nextInt(10) = 3 (second call), nextFloat() = 0.6832234 (third).
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(), -1170105035);
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_bounded(10), 0);
        assert_eq!(r.next_bounded(10), 3);
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_int(), -1155484576);
    }

    #[test]
    fn exclusive_sets() {
        use Enchantment as E;
        assert!(!E::Sharpness.compatible(E::Smite));
        assert!(!E::Protection.compatible(E::BlastProtection));
        assert!(!E::SilkTouch.compatible(E::Fortune));
        assert!(!E::Infinity.compatible(E::Mending));
        assert!(E::Sharpness.compatible(E::Looting));
        assert!(!E::Sharpness.compatible(E::Sharpness));
    }

    #[test]
    fn clues_continue_the_offer_roll_and_seeds_wrap_as_java_ints() {
        use Enchantment as E;
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Iron);
        // EnchantmentMenu reseeds for each roll and uses its next random
        // draw for the clue, after selection has consumed its draws.
        assert_eq!(
            offers(0, pick, 15),
            [
                Offer { cost: 8, clue: Some((E::Unbreaking, 2)) },
                Offer { cost: 13, clue: Some((E::Unbreaking, 2)) },
                Offer { cost: 30, clue: Some((E::Efficiency, 4)) },
            ]
        );
        assert_eq!(offer_enchants(i32::MAX, pick, 1, 30), offer_enchants(i32::MIN, pick, 0, 30));
    }

    #[test]
    fn table_offers_follow_java_costs() {
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Iron);
        for seed in 0..200 {
            let o = offers(seed, pick, 15);
            // With 15 shelves the bottom offer is always 30.
            assert_eq!(o[2].cost, 30);
            assert!((2..=10).contains(&o[0].cost), "{o:?}");
            for (slot, offer) in o.iter().enumerate() {
                let list = offer_enchants(seed, pick, slot, offer.cost);
                assert!(!list.is_empty());
                assert!(list.contains(&offer.clue.unwrap()), "the clue is one of them");
                for (e, _) in &list {
                    assert!(e.def().primary.accepts(pick) && !e.def().treasure, "{e:?}");
                }
                for (i, (a, _)) in list.iter().enumerate() {
                    assert!(list[i + 1..].iter().all(|(b, _)| a.compatible(*b)));
                }
            }
        }
        // No shelves: costs stay low.
        assert!(offers(7, pick, 0).iter().all(|o| o.cost <= 8));
        // Unenchantable things get no offers.
        assert!(offers(7, Item::STICK, 15).iter().all(|o| o.cost == 0 && o.clue.is_none()));
    }

    #[test]
    fn level_thirty_books_can_roll_high_levels() {
        let mut best = 0;
        for seed in 0..400 {
            for (e, l) in offer_enchants(seed, Item::BOOK, 2, 30) {
                if e == Enchantment::Sharpness {
                    best = best.max(l);
                }
            }
        }
        assert_eq!(best, 4, "sharpness IV is the most a 30-level table gives");
    }

    #[test]
    fn anvil_combines_and_repairs() {
        let sharp3 = Stack { enchants: Enchants::NONE.with(Enchantment::Sharpness, 3), ..Stack::new(sword(), 1) };
        let r = anvil(sharp3, Some(sharp3), false).unwrap();
        assert_eq!(r.output.enchants.level(Enchantment::Sharpness), 4);
        assert_eq!(r.cost, 4, "sharpness costs 1 per level");
        assert_eq!(r.output.repair_cost, 1);

        // A book halves the per-level price (min 1); levels cap at the max.
        let book =
            Stack { enchants: Enchants::NONE.with(Enchantment::Looting, 3), ..Stack::new(Item::ENCHANTED_BOOK, 1) };
        let r = anvil(Stack::new(sword(), 1), Some(book), false).unwrap();
        assert_eq!((r.output.enchants.level(Enchantment::Looting), r.cost), (3, 6));
        assert_eq!(
            anvil(Stack::new(Item::BOOK, 1), Some(book), false),
            None,
            "plain books do not store enchantments on an anvil"
        );
        assert!(anvil(Stack::new(Item::ENCHANTED_BOOK, 1), Some(book), false).is_some());

        // Conflicts cost one each and add nothing.
        let smite =
            Stack { enchants: Enchants::NONE.with(Enchantment::Smite, 1), ..Stack::new(Item::ENCHANTED_BOOK, 1) };
        assert_eq!(anvil(sharp3, Some(smite), false), None);

        // Material repair: a quarter of the durability per ingot.
        let worn = Stack { damage: 1000, ..Stack::new(sword(), 1) };
        let r = anvil(worn, Some(Stack::new(Item::DIAMOND, 5)), false).unwrap();
        assert_eq!((r.output.damage, r.cost, r.uses), (1000 - 390 * 2 - 220, 3, Some(3)));
        assert_eq!(anvil(Stack::new(sword(), 1), Some(Stack::new(Item::DIAMOND, 1)), false), None);

        // Two worn swords: both remainders plus 12%.
        let a = Stack { damage: 1500, ..Stack::new(sword(), 1) };
        let r = anvil(a, Some(a), false).unwrap();
        assert_eq!((r.output.damage, r.cost), (1561 - (61 + 61 + 187), 2));

        // Prior work makes it too expensive.
        let used = Stack { repair_cost: 39, ..sharp3 };
        assert_eq!(anvil(used, Some(sharp3), false), None);
        assert!(anvil(used, Some(sharp3), true).is_some(), "creative ignores the limit");

        // Unrelated items don't combine.
        let boots = Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Iron), 1);
        assert_eq!(anvil(boots, Some(Stack::new(sword(), 1)), false), None);
    }

    #[test]
    fn protection_and_damage_math() {
        let mut armor = [None; 4];
        let prot4 = Enchants::NONE.with(Enchantment::Protection, 4);
        let boots = Item::armor(ArmorPiece::Boots, ArmorMaterial::Diamond);
        armor[3] = Some(Stack { enchants: prot4.with(Enchantment::FeatherFalling, 4), ..Stack::new(boots, 1) });
        assert_eq!(protection_factor(&armor, DamageKind::Fall), 16);
        assert_eq!(protection_factor(&armor, DamageKind::Other), 4);
        assert!((protect(10.0, &armor, "fell from a high place") - 3.6).abs() < 1e-5);
        assert_eq!(DamageKind::of_cause("was fireballed by a blaze"), DamageKind::Fireball);
        assert_eq!(DamageKind::of_cause("was blown up by a creeper"), DamageKind::Explosion);
        let held = Enchants::NONE.with(Enchantment::Sharpness, 5);
        assert_eq!(damage_bonus(held, Creature::Other), 3.0);
        assert_eq!(damage_bonus(Enchants::NONE.with(Enchantment::Smite, 5), Creature::Undead), 12.5);
        assert_eq!(efficiency_bonus(Enchants::NONE.with(Enchantment::Efficiency, 5)), 26.0);
        assert!(wears(Enchants::NONE, false, 0.99));
        assert!(!wears(Enchants::NONE.with(Enchantment::Unbreaking, 3), false, 0.3));
    }
}
