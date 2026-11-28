//! Java 26.3 archaeology: ocean ruins (v0.14) and, since v1.0 Track 5,
//! trail ruins' `archaeology/trail_ruins_common` and `_rare` tables and
//! their seven pottery sherds. Original art; no decorated pots.
use glam::IVec3;
use rustc_hash::FxHashMap;

use super::{
    World,
    block::{Block, RenderKind},
    loot::{self, Random},
};
use crate::{
    inventory::{Stack, stack_from_str, stack_to_string},
    item::{Item, ItemInfo, ItemKind, Sprite},
};

pub const FIRST: u16 = 15232;
pub const LAST: u16 = FIRST + 7;
pub const SAND: Block = Block(FIRST);
pub const GRAVEL: Block = Block(FIRST + 4);
pub const ITEM_FIRST: u16 = 1395;
pub const ITEM_END: u16 = ITEM_FIRST + 7;
/// v1.0 Track 5: the trail ruins' sherds, item ids 1950–1956 (the track's
/// reserved 1950–1959).
pub const TRAIL_ITEM_FIRST: u16 = 1950;
pub const TRAIL_ITEM_END: u16 = TRAIL_ITEM_FIRST + 7;
pub mod t {
    pub const BASE: u16 = 3504;
    pub const END: u16 = BASE + 8;
}
pub const SHERDS: [&str; 7] = [
    "angler pottery sherd",
    "shelter pottery sherd",
    "snort pottery sherd",
    "blade pottery sherd",
    "explorer pottery sherd",
    "mourner pottery sherd",
    "plenty pottery sherd",
];
/// v1.0 Track 5: `archaeology/trail_ruins_rare`'s sherds, in its order.
pub const TRAIL_SHERDS: [&str; 7] = [
    "burn pottery sherd",
    "danger pottery sherd",
    "friend pottery sherd",
    "heart pottery sherd",
    "heartbreak pottery sherd",
    "howl pottery sherd",
    "sheaf pottery sherd",
];
/// The warm table's sniffer egg (the sniffer track's registration).
pub fn sniffer_egg() -> Option<Item> {
    Some(Item::SNIFFER_EGG)
}
pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}
pub const fn state(gravel: bool, dust: u8) -> Block {
    Block(FIRST + gravel as u16 * 4 + if dust > 3 { 3 } else { dust as u16 })
}
pub fn decode(b: Block) -> Option<(bool, u8)> {
    owns(b.0).then(|| ((b.0 - FIRST) >= 4, ((b.0 - FIRST) % 4) as u8))
}
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    (
        if id < FIRST + 4 { "suspicious sand" } else { "suspicious gravel" },
        RenderKind::Opaque,
        [t::BASE + id - FIRST; 6],
    )
}
pub fn item_info(item: Item) -> Option<ItemInfo> {
    let (name, sprite) = if (TRAIL_ITEM_FIRST..TRAIL_ITEM_END).contains(&item.0) {
        let i = (item.0 - TRAIL_ITEM_FIRST) as usize;
        (TRAIL_SHERDS[i], SHERDS.len() + i)
    } else {
        let i = item.0.checked_sub(ITEM_FIRST)? as usize;
        (*SHERDS.get(i)?, i)
    };
    Some(ItemInfo { name, kind: ItemKind::Material, max_stack: 64, sprite: Sprite::PotterySherd(sprite as u8) })
}
/// A suspicious block's loot table. The discriminant is its save code
/// (append only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    Warm = 0,
    Cold = 1,
    /// v1.0 Track 5: `archaeology/trail_ruins_common`.
    TrailCommon = 2,
    /// v1.0 Track 5: `archaeology/trail_ruins_rare`.
    TrailRare = 3,
}
impl Table {
    pub const ALL: [Table; 4] = [Self::Warm, Self::Cold, Self::TrailCommon, Self::TrailRare];
    fn key(self) -> &'static str {
        match self {
            Self::Warm => "minecraft:archaeology/ocean_ruin_warm",
            Self::Cold => "minecraft:archaeology/ocean_ruin_cold",
            Self::TrailCommon => "minecraft:archaeology/trail_ruins_common",
            Self::TrailRare => "minecraft:archaeology/trail_ruins_rare",
        }
    }
    /// The trail ruins' tables.
    pub fn trail(self) -> bool {
        matches!(self, Self::TrailCommon | Self::TrailRare)
    }
    /// The table saved as `code`.
    pub fn from_code(code: u8) -> Option<Table> {
        Self::ALL.get(code as usize).copied()
    }
    /// The ocean tables have 15 total weight; all entries have count one.
    /// The trail tables roll Java's JSON (`loot_tables.json`), whose single
    /// pool yields one stack (`BrushableBlockEntity.unpackLootTable` keeps
    /// the first).
    fn roll(self, rng: &mut impl Random) -> Option<Stack> {
        if let Some(name) = self.key().strip_prefix("minecraft:").filter(|_| self.trail()) {
            return loot::roll_items(name, rng, |_| None).into_iter().next();
        }
        let n = rng.bounded(15);
        let item = match n {
            0..=2 if self == Self::Warm => Some(Item(ITEM_FIRST + n as u16)),
            3 if self == Self::Warm => sniffer_egg(),
            0..=3 => Some(Item(ITEM_FIRST + 3 + n as u16)),
            4 => Some(Item::tool(crate::item::ToolKind::Axe, crate::item::Tier::Iron)),
            5..=6 => Some(Item::EMERALD),
            7..=8 => Some(Item::WHEAT),
            9..=10 => Some(Item::tool(crate::item::ToolKind::Hoe, crate::item::Tier::Wood)),
            11..=12 => Some(Item::COAL),
            _ => Some(Item::GOLD_NUGGET),
        };
        item.map(|i| Stack::new(i, 1))
    }
}
#[derive(Clone, Debug, Default)]
pub struct Brushable {
    pub table: Option<Table>,
    pub seed: u64,
    /// A resolved reward survives reload, including the explicitly empty result.
    item: String,
    count: u8,
    reset_at: u64,
    cooldown: u64,
    face: Option<[i32; 3]>,
}
impl Brushable {
    pub(super) fn with_loot(table: Table, seed: u64) -> Self {
        Self { table: Some(table), seed, ..Default::default() }
    }
}
pub type State = FxHashMap<IVec3, Brushable>;
impl World {
    pub(super) fn register_archaeology_ticks(&mut self, chunk: IVec3) {
        let Some(slot) = self.chunks.get(&chunk) else {
            return;
        };
        // Palette fast path, as `register_beehives`: byte chunks can't hold
        // these IDs, so most loads never scan their blocks.
        let any = match &*slot.data {
            super::chunk::ChunkData::Uniform(b) => owns(b.0),
            super::chunk::ChunkData::Bytes(_) => false,
            super::chunk::ChunkData::Paletted { palette, len, .. } => {
                palette[..*len as usize].iter().any(|b| owns(b.0))
            }
            super::chunk::ChunkData::Dense(v) => v.iter().any(|b| owns(b.0)),
        };
        if !any {
            return;
        }
        let data = slot.data.clone();
        let mut i = 0;
        let mut pending = Vec::new();
        data.for_each_block(|b| {
            if owns(b.0) {
                pending.push(chunk * 32 + IVec3::new(i % 32, i / (32 * 32), i / 32 % 32));
            }
            i += 1;
        });
        for p in pending {
            self.archaeology.entry(p).or_default();
            self.schedule_redstone(p, 2, 0);
        }
    }
    pub fn set_archaeology_loot(&mut self, p: IVec3, table: Table, seed: u64) -> bool {
        if !self.get_block(p).is_some_and(|b| owns(b.0)) {
            return false;
        }
        self.archaeology.insert(p, Brushable { table: Some(table), seed, ..Default::default() });
        true
    }
    pub(super) fn track_archaeology(&mut self, p: IVec3, old: Block, new: Block) {
        if owns(new.0) {
            if !owns(old.0) || old.base() != new.base() {
                self.archaeology.insert(p, Brushable::default());
            }
            self.schedule_redstone(p, 2, 0);
        } else {
            self.archaeology.remove(&p);
        }
        for q in [p + IVec3::Y, p - IVec3::Y, p + IVec3::X, p - IVec3::X, p + IVec3::Z, p - IVec3::Z] {
            if self.get_block(q).is_some_and(|b| owns(b.0)) {
                self.schedule_redstone(q, 2, 0);
            }
        }
    }
    /// An accepted stroke; true only on completion (one point of brush wear).
    pub fn brush_block(&mut self, p: IVec3, face: IVec3) -> bool {
        let Some((gravel, _)) = self.get_block(p).and_then(decode) else {
            return false;
        };
        let now = self.maps.game_ticks;
        let b = self.archaeology.entry(p).or_default();
        b.face.get_or_insert(face.to_array());
        b.reset_at = now + 40;
        if now < b.cooldown {
            return false;
        }
        b.cooldown = now + 10;
        if let Some(table) = b.table.take() {
            let item = if b.seed != 0 {
                table.roll(&mut loot::LootRandom::new(b.seed))
            } else {
                table.roll(self.loot_sequences.get(self.generator.seed, table.key()))
            };
            b.item = stack_to_string(item);
        }
        b.count += 1;
        if b.count >= 10 {
            let item = stack_from_str(&b.item).flatten();
            let face = IVec3::from_array(b.face.unwrap_or([0, 1, 0]));
            if let Some(stack) = item {
                self.drops.push((p + face, stack));
            }
            // Level event 3008: the suspicious block's destroy particles.
            if let Some(block) = self.get_block(p) {
                self.particles.push(crate::particles::Request::Break { cell: p, block });
            }
            self.set_block(p, if gravel { Block::GRAVEL } else { Block::SAND });
            true
        } else {
            let dust = dust(b.count);
            self.edit(p, state(gravel, dust), true);
            self.schedule_redstone(p, 2, 0);
            false
        }
    }
    /// Partly brushed blocks with a resolved reward: cell, reward and first
    /// face, for Java's `BrushableBlockRenderer` (the block's dust stage sets
    /// how far the reward emerges).
    pub fn emerging_items(&self) -> impl Iterator<Item = (IVec3, Stack, IVec3)> + '_ {
        self.archaeology.iter().filter_map(|(p, b)| {
            let face = IVec3::from_array(b.face?);
            Some((*p, stack_from_str(&b.item).flatten()?, face))
        })
    }
    /// v1.0 final Track F4: the rewards rising out of partly brushed blocks
    /// within `radius` of `at`, for LAN guests (`BrushableBlockEntity`'s
    /// item and hit direction, which Java syncs to clients in
    /// `getUpdateTag`): `[x, y, z, face x, face y, face z, stack]`.
    pub fn emerging_near(&self, at: glam::DVec3, radius: f64) -> Vec<serde_json::Value> {
        self.emerging_items()
            .filter(|(p, ..)| (p.as_dvec3() + glam::DVec3::splat(0.5)).distance_squared(at) <= radius * radius)
            .take(128)
            .map(|(p, stack, face)| {
                serde_json::json!([p.x, p.y, p.z, face.x, face.y, face.z, stack_to_string(Some(stack))])
            })
            .collect()
    }

    /// v1.0 final Track F4: applies [`World::emerging_near`] on a LAN guest,
    /// whose own archaeology state never resolves a reward, so the find it
    /// draws is the host's.
    pub fn apply_emerging(&mut self, list: &serde_json::Value) {
        let Some(list) = list.as_array() else { return };
        self.archaeology.retain(|_, b| b.face.is_none() && b.item.is_empty());
        for v in list {
            let Some(a) = v.as_array() else { continue };
            let n = |i: usize| a.get(i).and_then(serde_json::Value::as_i64).map(|n| n as i32);
            let (Some(x), Some(y), Some(z), Some(fx), Some(fy), Some(fz)) = (n(0), n(1), n(2), n(3), n(4), n(5)) else {
                continue;
            };
            let Some(item) = a.get(6).and_then(serde_json::Value::as_str) else { continue };
            if stack_from_str(item).flatten().is_none() {
                continue;
            }
            let b = self.archaeology.entry(IVec3::new(x, y, z)).or_default();
            b.item = item.to_string();
            b.face = Some([fx, fy, fz]);
        }
    }

    /// Scripted scenes (`--brush`): `strokes` accepted strokes from `face`,
    /// as if ten ticks apart. A block without loot gets its ocean-ruin table.
    /// The dust stage is held (no 40-tick reset) so a capture taken after
    /// the scene settles still shows it.
    pub fn brush_scripted(&mut self, p: IVec3, face: IVec3, strokes: u8) -> bool {
        let Some((gravel, _)) = self.get_block(p).and_then(decode) else {
            return false;
        };
        let b = self.archaeology.entry(p).or_default();
        if b.table.is_none() && b.item.is_empty() {
            b.table = Some(if gravel { Table::Cold } else { Table::Warm });
            b.seed = super::noise::hash3(p.x, p.y, p.z, 0x5EED) | 1;
        }
        for _ in 0..strokes {
            if let Some(b) = self.archaeology.get_mut(&p) {
                b.cooldown = 0;
            }
            if self.brush_block(p, face) {
                return true;
            }
        }
        if let Some(b) = self.archaeology.get_mut(&p) {
            b.reset_at = u64::MAX;
        }
        true
    }
    pub(super) fn archaeology_tick(&mut self, p: IVec3) {
        let Some((gravel, _)) = self.get_block(p).and_then(decode) else {
            return;
        };
        let now = self.maps.game_ticks;
        let b = self.archaeology.entry(p).or_default();
        if b.count > 0 && now >= b.reset_at {
            b.count = b.count.saturating_sub(2);
            b.reset_at = now + 4;
            if b.count == 0 {
                b.face = None;
                b.cooldown = 0;
                b.reset_at = 0;
            }
            let dust = dust(b.count);
            self.edit(p, state(gravel, dust), true);
        }
        if self.archaeology.get(&p).is_some_and(|b| b.count > 0) {
            self.schedule_redstone(p, 2, 0);
        }
        if self.get_block(p - IVec3::Y).is_some_and(super::falling::can_fall_into) {
            let block = self.get_block(p).unwrap();
            self.edit(p, Block::AIR, true);
            self.falling.push(super::falling::FallingBlock::new(block, p.as_dvec3(), 0.0));
            // Sand or gravel resting on it loses its support too.
            self.settle(p);
        }
    }
    pub fn archaeology_to_string(&self) -> String {
        let mut entries: Vec<_> = self
            .archaeology
            .iter()
            .map(|(p, b)| {
                (
                    p.to_array(),
                    b.table.map(|t| t as u8),
                    b.seed,
                    b.item.as_str(),
                    b.count,
                    b.reset_at,
                    b.cooldown,
                    b.face,
                )
            })
            .collect();
        entries.sort_by_key(|e| e.0);
        serde_json::to_string(&entries).expect("archaeology serialization")
    }
    pub fn load_archaeology(&mut self, text: &str) {
        type Entry = ([i32; 3], Option<u8>, u64, String, u8, u64, u64, Option<[i32; 3]>);
        if let Ok(entries) = serde_json::from_str::<Vec<Entry>>(text) {
            self.archaeology = entries
                .into_iter()
                .filter(|e| {
                    e.4 < 10
                        && e.1.is_none_or(|t| Table::from_code(t).is_some())
                        && e.7.is_none_or(|f| f.iter().map(|n| (*n as i64).abs()).sum::<i64>() == 1)
                })
                .map(|(p, t, seed, item, count, reset_at, cooldown, face)| {
                    (
                        IVec3::from_array(p),
                        Brushable { table: t.and_then(Table::from_code), seed, item, count, reset_at, cooldown, face },
                    )
                })
                .collect();
        }
    }
}
fn dust(count: u8) -> u8 {
    match count {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        chunk::{ChunkData, chunk_of},
        terrain::Generator,
    };
    use std::sync::Arc;
    const AT: IVec3 = IVec3::new(8, 145, 8);
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
        w.insert_chunk(chunk_of(AT), Arc::new(ChunkData::Uniform(Block::AIR)), true);
        w.set_block(AT - IVec3::Y, Block::STONE);
        w.set_block(AT, SAND);
        w.set_archaeology_loot(AT, Table::Warm, 42);
        w
    }
    fn tick(w: &mut World) {
        w.maps.game_ticks += 1;
        w.tick_redstone();
    }
    #[test]
    fn java_strokes_cooldown_dust_decay_and_one_extraction() {
        let mut w = world();
        for (i, expected) in [1, 1, 2, 2, 2, 3, 3, 3, 3].into_iter().enumerate() {
            w.maps.game_ticks = i as u64 * 10;
            assert!(!w.brush_block(AT, IVec3::X));
            assert_eq!(decode(w.get_block(AT).unwrap()), Some((false, expected)));
            assert!(!w.brush_block(AT, IVec3::NEG_X), "two players cannot double the rate");
        }
        w.maps.game_ticks = 90;
        assert!(w.brush_block(AT, IVec3::NEG_X));
        assert_eq!(w.get_block(AT), Some(Block::SAND));
        assert_eq!(w.drops.len(), 1);
        assert_eq!(w.drops[0].0, AT + IVec3::X, "first face persists");
        assert!(!w.brush_block(AT, IVec3::X));
        let mut w = world();
        for i in 0..6 {
            w.maps.game_ticks = i * 10;
            w.brush_block(AT, IVec3::X);
        }
        for _ in 0..39 {
            tick(&mut w);
        }
        assert_eq!(decode(w.get_block(AT).unwrap()), Some((false, 3)));
        for _ in 0..10 {
            tick(&mut w);
        }
        assert_eq!(decode(w.get_block(AT).unwrap()), Some((false, 0)));
        assert!(w.drops.is_empty());
    }
    #[test]
    fn unbrushed_and_resolved_reward_survive_saved_chunk_reload_without_rerolls() {
        for strokes in [0, 4] {
            let mut w = world();
            for i in 0..strokes {
                w.maps.game_ticks = i * 10;
                w.brush_block(AT, IVec3::X);
            }
            let saved = w.archaeology_to_string();
            let data = w.chunks[&chunk_of(AT)].data.clone();
            let mut loaded = world();
            loaded.insert_chunk(chunk_of(AT), data, true);
            loaded.load_archaeology(&saved);
            assert_eq!(loaded.archaeology_to_string(), saved);
            for i in strokes..10 {
                for world in [&mut w, &mut loaded] {
                    world.maps.game_ticks = i * 10;
                    world.brush_block(AT, IVec3::X);
                }
            }
            assert_eq!(w.drops, loaded.drops);
            assert_eq!(loaded.get_block(AT), Some(Block::SAND));
            assert_eq!(loaded.archaeology_to_string(), "[]");
        }
    }
    #[test]
    fn sand_on_a_falling_suspicious_block_falls_with_it() {
        let mut w = world();
        w.set_block(AT + IVec3::Y, Block::SAND);
        w.set_block(AT - IVec3::Y, Block::AIR);
        w.set_block(AT - IVec3::Y * 3, Block::STONE);
        tick(&mut w);
        tick(&mut w);
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        assert_eq!(w.get_block(AT + IVec3::Y), Some(Block::AIR), "the sand above lost its support");
        assert_eq!(w.falling.len(), 2);
    }
    #[test]
    fn unsupported_blocks_wait_two_ticks_then_fall_and_break_with_no_loot() {
        let mut w = world();
        w.set_block(AT - IVec3::Y, Block::AIR);
        w.set_block(AT - IVec3::Y * 3, Block::STONE);
        assert!(w.falling.is_empty());
        tick(&mut w);
        assert_eq!(w.get_block(AT), Some(SAND));
        tick(&mut w);
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        assert_eq!(w.falling.len(), 1);
        let saved = w.falling_to_string();
        w.load_falling(&saved);
        for _ in 0..80 {
            w.tick_falling(0.05);
        }
        assert!(w.falling.is_empty());
        assert_eq!(w.get_block(AT - IVec3::Y * 2), Some(Block::AIR));
        assert!(w.drops.is_empty());
        assert_eq!(w.archaeology_to_string(), "[]");
        for b in [SAND, GRAVEL, state(false, 3), state(true, 3)] {
            assert_eq!(b.drop(), None);
            assert_eq!(crate::mining::silk_drop(b), None);
        }
    }
    #[test]
    fn scripted_brushing_holds_its_dust_stage_and_resolves_loot() {
        let mut w = world();
        w.set_block(AT + IVec3::X * 2 - IVec3::Y, Block::STONE);
        let bare = AT + IVec3::X * 2;
        w.set_block(bare, GRAVEL);
        assert!(w.brush_scripted(bare, IVec3::Z, 6));
        assert!(!w.brush_scripted(AT - IVec3::Y, IVec3::Z, 1), "only suspicious blocks");
        for _ in 0..100 {
            tick(&mut w);
        }
        assert_eq!(decode(w.get_block(bare).unwrap()), Some((true, 3)));
        assert_eq!(w.emerging_items().filter(|(p, ..)| *p == bare).count(), 1, "cold-table reward");
    }
    /// v1.0 Track 5: the trail tables roll Java's entries, and every one
    /// resolves (the seven sherds are registered at 1950–1956).
    #[test]
    fn trail_tables_roll_javas_entries_and_every_item_resolves() {
        use std::collections::BTreeMap;
        for (table, entries, total) in [(Table::TrailCommon, 31, 45), (Table::TrailRare, 12, 12)] {
            let mut seen: BTreeMap<String, u32> = BTreeMap::new();
            let mut rng = loot::LootRandom::new(77);
            let rolls = 20_000;
            for _ in 0..rolls {
                let stack = table.roll(&mut rng).expect("one stack");
                assert_eq!(stack.count, 1);
                *seen.entry(stack.item.name().to_string()).or_default() += 1;
            }
            assert_eq!(seen.len(), entries, "{table:?}: {seen:?}");
            let per = rolls as f64 / total as f64;
            for (name, n) in &seen {
                assert!((*n as f64) > per * 0.6, "{table:?} {name}: {n}");
            }
        }
        let rare: Vec<Option<Item>> = [
            "wayfinder armor trim smithing template",
            "raiser armor trim smithing template",
            "shaper armor trim smithing template",
            "host armor trim smithing template",
            "music disc relic",
        ]
        .into_iter()
        .chain(TRAIL_SHERDS)
        .map(Item::from_name)
        .collect();
        assert!(rare.iter().all(Option::is_some), "{rare:?}");
        for (i, name) in TRAIL_SHERDS.iter().enumerate() {
            let item = Item(TRAIL_ITEM_FIRST + i as u16);
            assert_eq!(item.name(), *name);
            assert_eq!(item.max_stack(), 64);
            assert_eq!(item.info().sprite, Sprite::PotterySherd(7 + i as u8));
            assert_eq!(loot::java_item(&name.replace(' ', "_")), Some(item));
        }
    }

    /// v1.0 Track 5: suspicious gravel with a trail table saves its table
    /// code (2 and 3, appended) and reloads it, before and after a stroke.
    #[test]
    fn trail_tables_survive_save_and_reload() {
        for table in [Table::TrailCommon, Table::TrailRare] {
            let mut w = world();
            w.set_block(AT, GRAVEL);
            assert!(w.set_archaeology_loot(AT, table, 0x5EED));
            let saved = w.archaeology_to_string();
            assert!(saved.contains(&format!(",{},", table as u8)), "{saved}");
            let data = w.chunks[&chunk_of(AT)].data.clone();
            let mut loaded = world();
            loaded.insert_chunk(chunk_of(AT), data, true);
            loaded.load_archaeology(&saved);
            assert_eq!(loaded.archaeology_to_string(), saved);
            assert_eq!(loaded.archaeology[&AT].table, Some(table));
            for i in 0..10 {
                for world in [&mut w, &mut loaded] {
                    world.maps.game_ticks = i * 10;
                    world.brush_block(AT, IVec3::Y);
                }
            }
            assert_eq!(w.drops.len(), 1);
            assert_eq!(w.drops, loaded.drops, "the same seeded reward");
        }
        // An unknown code (a newer save) is dropped, not misread.
        let mut w = world();
        w.load_archaeology("[[[8,145,8],9,1,\"\",0,0,0,null]]");
        assert_eq!(w.archaeology_to_string(), "[]");
    }
    struct Choice(u32);
    impl Random for Choice {
        fn bounded(&mut self, n: u32) -> u32 {
            assert_eq!(n, 15);
            self.0
        }
        fn boolean(&mut self) -> bool {
            unreachable!()
        }
        fn float(&mut self) -> f32 {
            unreachable!()
        }
    }
    #[test]
    fn exact_java_table_weights_and_original_item_registrations() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/ocean_archaeology.json")).unwrap();
        for (table, key) in [(Table::Warm, "warm"), (Table::Cold, "cold")] {
            let entries = fixture[key]["pools"][0]["entries"].as_array().unwrap();
            let mut slot = 0;
            for entry in entries {
                let name = entry["name"].as_str().unwrap().trim_start_matches("minecraft:");
                let expected = Item::from_name(name);
                assert!(expected.is_some(), "missing archaeology item {name}");
                for _ in 0..entry["weight"].as_u64().unwrap_or(1) {
                    assert_eq!(
                        table.roll(&mut Choice(slot)),
                        expected.map(|i| Stack::new(i, 1)),
                        "{name} weighted slot {slot}"
                    );
                    slot += 1;
                }
            }
            assert_eq!(slot, 15);
        }
        for i in ITEM_FIRST..ITEM_END {
            let item = Item(i);
            assert!(item.is_valid());
            assert_eq!(Item::from_name(SHERDS[(i - ITEM_FIRST) as usize]), Some(item));
            assert_eq!(item.max_stack(), 64);
        }
    }
}
