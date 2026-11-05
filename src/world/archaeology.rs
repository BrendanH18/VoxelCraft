//! Java 26.3 ocean-ruin archaeology. Original art; no decorated pots.
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
/// Track C supplies this registration. Keeping its Java name avoids reserving
/// another track's numeric identifier or substituting a different reward.
pub const SNIFFER_EGG_JAVA_NAME: &str = "sniffer_egg";
pub fn sniffer_egg() -> Option<Item> {
    Item::from_name(SNIFFER_EGG_JAVA_NAME)
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
    let i = item.0.checked_sub(ITEM_FIRST)? as usize;
    SHERDS.get(i).map(|&name| ItemInfo {
        name,
        kind: ItemKind::Material,
        max_stack: 64,
        sprite: Sprite::PotterySherd(i as u8),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    Warm,
    Cold,
}
impl Table {
    fn key(self) -> &'static str {
        match self {
            Self::Warm => "minecraft:archaeology/ocean_ruin_warm",
            Self::Cold => "minecraft:archaeology/ocean_ruin_cold",
        }
    }
    /// The table has 15 total weight; absent eggs keep their weight and yield
    /// nothing until track C is integrated. All entries have count one.
    fn roll(self, rng: &mut impl Random) -> Option<Stack> {
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
        if slot.data.uniform().is_some_and(|b| !owns(b.0)) {
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
            self.set_block(p, if gravel { Block::GRAVEL } else { Block::SAND });
            true
        } else {
            let dust = dust(b.count);
            self.edit(p, state(gravel, dust), true);
            self.schedule_redstone(p, 2, 0);
            false
        }
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
                        && e.1.is_none_or(|t| t <= 1)
                        && e.7.is_none_or(|f| f.iter().map(|n| (*n as i64).abs()).sum::<i64>() == 1)
                })
                .map(|(p, t, seed, item, count, reset_at, cooldown, face)| {
                    (
                        IVec3::from_array(p),
                        Brushable {
                            table: t.map(|v| if v == 0 { Table::Warm } else { Table::Cold }),
                            seed,
                            item,
                            count,
                            reset_at,
                            cooldown,
                            face,
                        },
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
                assert!(expected.is_some() || name == SNIFFER_EGG_JAVA_NAME, "missing archaeology item {name}");
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
