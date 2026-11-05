//! Bee nest/hive states, bounded occupant storage and survival transactions.
use super::{
    World,
    block::{Block, Facing, RenderKind},
    chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData},
};
use crate::{
    entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, bee::Occupant},
    inventory::Stack,
};
use glam::IVec3;
use serde_json::{Value, json};
pub const FIRST: u16 = 10844;
pub const LAST: u16 = FIRST + 48;
pub const NEST: Block = Block(FIRST);
pub const HIVE: Block = Block(FIRST + 24);
pub const HONEYCOMB_BLOCK: Block = Block(LAST);
pub mod t {
    pub const BASE: u16 = 3008;
    pub const NEST_SIDE: u16 = BASE;
    pub const NEST_TOP: u16 = BASE + 1;
    pub const NEST_FRONT: u16 = BASE + 2;
    pub const HIVE_SIDE: u16 = BASE + 4;
    pub const HIVE_TOP: u16 = BASE + 5;
    pub const HIVE_FRONT: u16 = BASE + 6;
    pub const COMB: u16 = BASE + 8;
    pub const END: u16 = BASE + 9;
}
pub const fn block(nest: bool, f: Facing, honey: u8) -> Block {
    Block(FIRST + if nest { 0 } else { 24 } + honey as u16 * 4 + f as u16)
}
pub fn hive(b: Block) -> Option<(bool, Facing, u8)> {
    let r = b.0.checked_sub(FIRST)?;
    if r >= 48 {
        return None;
    }
    Some((r < 24, Facing::ALL[(r % 4) as usize], ((r % 24) / 4) as u8))
}
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    if id == LAST {
        return ("honeycomb block", RenderKind::Opaque, [t::COMB; 6]);
    }
    let r = id - FIRST;
    let nest = r < 24;
    let side = if nest { t::NEST_SIDE } else { t::HIVE_SIDE };
    let top = side + 1;
    let front = side + 2 + ((r % 24) / 4 == 5) as u16;
    let mut tex = [side; 6];
    tex[2] = top;
    tex[3] = top;
    let face = match r % 4 {
        0 => 4,
        1 => 5,
        2 => 0,
        _ => 1,
    };
    tex[face] = front;
    (if nest { "bee nest" } else { "beehive" }, RenderKind::Opaque, tex)
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HiveItem {
    pub bees: [Option<Occupant>; 3],
    pub honey: u8,
}
impl HiveItem {
    /// Stacks are Copy throughout the engine. Immutable hive components are
    /// interned at acquisition/load, so ordinary stacks retain pointer-sized
    /// optional data instead of copying three bees on every inventory move.
    pub fn intern(self) -> &'static Self {
        static VALUES: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, &'static HiveItem>>> =
            std::sync::OnceLock::new();
        let mut values = VALUES.get_or_init(Default::default).lock().unwrap();
        let key = self.to_hex();
        values.entry(key).or_insert_with(|| Box::leak(Box::new(self)))
    }

    pub fn save(&self) -> Value {
        json!({"honey":self.honey,"bees":self.bees.each_ref().map(|o|o.as_ref().map(Occupant::save))})
    }
    pub fn load(v: &Value) -> Option<Self> {
        let honey = v["honey"].as_u64()?.try_into().ok()?;
        if honey > 5 {
            return None;
        }
        let array = v["bees"].as_array()?;
        if array.len() != 3 {
            return None;
        }
        let mut bees = [None, None, None];
        for (i, v) in array.iter().enumerate() {
            if !v.is_null() {
                bees[i] = Some(Occupant::load(v)?);
            }
        }
        Some(Self { bees, honey })
    }
    pub fn to_hex(&self) -> String {
        serde_json::to_vec(&self.save()).unwrap().iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() > 131072 || !s.len().is_multiple_of(2) {
            return None;
        }
        let bytes = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
            .collect::<Option<Vec<_>>>()?;
        Self::load(&serde_json::from_slice::<Value>(&bytes).ok()?)
    }
}
#[derive(Default)]
pub(crate) struct State {
    pub active: std::collections::BTreeSet<[i32; 3]>,
    pub hives: std::collections::BTreeMap<[i32; 3], HiveItem>,
    pub released: Vec<(IVec3, Occupant)>,
    pub time: f64,
    pub night: Option<bool>,
}
/// The pinned overworld day timeline's discrete bees_stay_in_hive track.
pub fn rest_time(dimension: super::terrain::Dimension, ticks: u32) -> bool {
    dimension == super::terrain::Dimension::Overworld && (12542..23460).contains(&(ticks % 24000))
}
impl World {
    /// Adapters with an authoritative clock supply it before entity updates.
    pub fn set_bee_clock(&mut self, dimension: super::terrain::Dimension, ticks: u32) {
        self.beehives.night = Some(rest_time(dimension, ticks));
    }
    pub fn beehives_to_string(&self) -> String {
        let values: Vec<_> = self.beehives.hives.iter().map(|(p, h)| json!([p, h.save()])).collect();
        let released: Vec<_> = self.beehives.released.iter().map(|(p, o)| json!([p.to_array(), o.save()])).collect();
        json!({"hives":values,"released":released}).to_string()
    }
    pub fn load_beehives(&mut self, text: &str) {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return;
        };
        self.beehives.released.clear();
        let entries = value.as_array().or_else(|| value["hives"].as_array());
        if let Some(values) = entries {
            for v in values {
                if let (Ok(p), Some(h)) = (serde_json::from_value::<[i32; 3]>(v[0].clone()), HiveItem::load(&v[1])) {
                    self.beehives.hives.insert(p, h);
                }
            }
        }
        if let Some(values) = value["released"].as_array() {
            for v in values {
                if let (Ok(p), Some(o)) = (serde_json::from_value::<[i32; 3]>(v[0].clone()), Occupant::load(&v[1])) {
                    self.beehives.released.push((IVec3::from_array(p), o));
                }
            }
        }
    }
    pub fn beehive(&self, p: IVec3) -> Option<&HiveItem> {
        self.beehives.hives.get(&p.to_array())
    }
    pub(in crate::world) fn unload_beehives(&mut self, pos: IVec3) {
        self.beehives.active.retain(|p| super::chunk::chunk_of(IVec3::from_array(*p)) != pos);
    }
    pub(in crate::world) fn register_beehives(&mut self, pos: IVec3, data: &ChunkData, modified: bool) {
        let any = match data {
            ChunkData::Uniform(b) => hive(*b).is_some(),
            ChunkData::Bytes(_) => false,
            ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|&b| hive(b).is_some()),
            ChunkData::Dense(v) => v.iter().any(|&b| hive(b).is_some()),
        };
        if !any {
            return;
        }
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let b = data.get(x, y, z);
                    if let Some((nest, _, honey)) = hive(b) {
                        let p = pos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32);
                        self.beehives.active.insert(p.to_array());
                        self.beehives.hives.entry(p.to_array()).or_insert_with(|| {
                            let mut h = HiveItem { honey, ..Default::default() };
                            if nest && !modified {
                                let mut n = super::noise::hash3(p.x, p.y, p.z, self.generator.seed);
                                for i in 0..2 + (n % 2) as usize {
                                    let ticks = (super::noise::splitmix64(&mut n) % 599) as u32;
                                    h.bees[i] = Some(Occupant::natural(ticks));
                                }
                            }
                            h
                        });
                    }
                }
            }
        }
    }
    pub(in crate::world) fn track_beehives(&mut self, p: IVec3, old: Block, new: Block) {
        if hive(old).is_some() && hive(new).is_none() {
            self.beehives.active.remove(&p.to_array());
            if let Some(h) = self.beehives.hives.remove(&p.to_array()) {
                self.beehives.released.extend(h.bees.into_iter().flatten().map(|b| (p, b)));
            }
        } else if let Some((_, _, honey)) = hive(new) {
            self.beehives.active.insert(p.to_array());
            self.beehives.hives.entry(p.to_array()).or_default().honey = honey;
        }
    }
    pub fn restore_hive_item(&mut self, p: IVec3, item: Option<&'static HiveItem>) {
        if let Some(h) = item
            && let Some((nest, f, _)) = self.get_block(p).and_then(hive)
        {
            self.set_block(p, block(nest, f, h.honey));
            self.beehives.hives.insert(p.to_array(), h.clone());
        }
    }
    pub(super) fn populate_bee_nest(&mut self, p: IVec3) {
        let count = 2 + (self.roll() % 2) as usize;
        let mut h = HiveItem::default();
        for i in 0..count {
            h.bees[i] = Some(Occupant::natural((self.roll() % 599) as u32));
        }
        self.beehives.hives.insert(p.to_array(), h);
    }
    pub fn bee_enter(&mut self, p: IVec3, bee: Occupant) -> bool {
        if self.get_block(p).and_then(hive).is_none() {
            return false;
        }
        let h = self.beehives.hives.entry(p.to_array()).or_default();
        if let Some(slot) = h.bees.iter_mut().find(|o| o.is_none()) {
            *slot = Some(bee);
            true
        } else {
            false
        }
    }
    fn release_bee(&mut self, entities: &mut Entities, p: IVec3, o: Occupant, emergency: bool) {
        let f = self.get_block(p).and_then(hive).map_or(Facing::South, |(_, f, _)| f);
        let Some(mut restored) = o.mob() else {
            return;
        };
        let age = if restored.age < 0 {
            restored.age.saturating_add(o.ticks.min(i32::MAX as u32) as i32).min(0)
        } else {
            (restored.age - o.ticks.min(i32::MAX as u32) as i32).max(0)
        };
        let scale = if age < 0 { 0.5 } else { 1. };
        let blocked = self.bee_exit_blocked(p + f.offset());
        let dist = if emergency && blocked { 0. } else { 0.55 + 0.35 * scale };
        let at = p.as_dvec3() + glam::DVec3::new(0.5, 0.5 - 0.3 * scale, 0.5) + f.offset().as_dvec3() * dist;
        entities.spawn(MobKind::Bee, at);
        let uid = entities.mobs.last().unwrap().uid;
        restored.uid = uid;
        restored.pos = at;
        restored.previous_pos = at;
        restored.age = age;
        restored.baby = age < 0;
        restored.persistent = true;
        if let Some(a) = &mut restored.animal {
            a.love = (a.love - o.ticks as f32 / 20.).max(0.);
            a.courtship = 0.;
            a.partner = None;
        }
        *entities.mobs.last_mut().unwrap() = restored;
        let s = entities.mobs.last_mut().unwrap().bee.as_mut().unwrap();
        s.hive = Some(p.to_array());
        s.nectar = o.nectar && emergency;
        s.crops = 0;
        s.since_pollination = 0;
        s.pollination = 0;
        s.anger = 0;
        s.target = None;
        s.cannot_enter = if emergency { 400 } else { 0 };
    }
    pub fn tick_beehives(&mut self, entities: &mut Entities, dt: f64, ctx: &Ctx) {
        for (p, o) in std::mem::take(&mut self.beehives.released) {
            self.release_bee(entities, p, o, true);
        }
        self.beehives.time += dt;
        let ticks = (self.beehives.time * 20.).floor() as u32;
        self.beehives.time -= ticks as f64 / 20.;
        if ticks == 0 {
            return;
        }
        let keys: Vec<_> =
            self.beehives.active.iter().copied().filter(|p| self.entity_ticking(IVec3::from_array(*p))).collect();
        for key in keys {
            let p = IVec3::from_array(key);
            let Some((nest, f, mut honey)) = self.get_block(p).and_then(hive) else {
                continue;
            };
            let blocked = self.bee_exit_blocked(p + f.offset());
            let emergency = crate::entity::bee::near_fire(self, p);
            let mut h = self.beehives.hives.remove(&key).unwrap();
            for slot in &mut h.bees {
                let Some(o) = slot.as_mut() else {
                    continue;
                };
                o.ticks = o.ticks.saturating_add(ticks);
                if emergency
                    || (!ctx.raining
                        && !self.bee_night(ctx.daylight)
                        && !blocked
                        && o.ticks > if o.nectar { 2400 } else { 600 })
                {
                    let o = slot.take().unwrap();
                    if o.nectar && !emergency && honey < 5 {
                        honey = (honey + if self.one_in(100) { 2 } else { 1 }).min(5);
                    }
                    self.release_bee(entities, p, o, emergency);
                }
            }
            h.honey = honey;
            self.beehives.hives.insert(key, h);
            if self.get_block(p) != Some(block(nest, f, honey)) {
                self.set_block(p, block(nest, f, honey));
            }
        }
    }
    /// Campfire smoke: five cells below, stopped by a full collision face;
    /// a campfire directly below that blocking cell can still smoke through.
    fn bee_collision(&self, p: IVec3) -> super::shape::Boxes {
        let get = |q| self.get_block(q).unwrap_or(Block::STONE);
        super::shape::collision_or_cube(get(p), |f| get(p + f.offset()), get(p - IVec3::Y), super::shape::Above::NONE)
    }
    fn bee_exit_blocked(&self, p: IVec3) -> bool {
        !self.bee_collision(p).is_empty()
    }
    pub fn hive_smoked(&self, p: IVec3) -> bool {
        for dy in 1..=5 {
            let q = p - IVec3::Y * dy;
            let Some(b) = self.get_block(q) else {
                return false;
            };
            if super::utilities::campfire_of(b).is_some_and(|c| c.lit) {
                return true;
            }
            if self
                .bee_collision(q)
                .as_slice()
                .iter()
                .any(|b| b.min[0] < 10 && b.max[0] > 6 && b.min[2] < 10 && b.max[2] > 6)
            {
                return self
                    .get_block(q - IVec3::Y)
                    .is_some_and(|b| super::utilities::campfire_of(b).is_some_and(|c| c.lit));
            }
        }
        false
    }
    pub fn drop_honeycomb(&mut self, p: IVec3) {
        if self.tile_drops {
            self.drops.push((p, Stack::new(crate::item::Item::HONEYCOMB, 3)));
        }
    }
    pub fn harvest_beehive(&mut self, entities: &mut Entities, p: IVec3, player: PlayerId) -> bool {
        let Some((nest, f, 5)) = self.get_block(p).and_then(hive) else {
            return false;
        };
        let smoked = self.hive_smoked(p);
        if !smoked {
            let mut h = self.beehives.hives.remove(&p.to_array()).unwrap_or_default();
            let occupied = h.bees.iter().any(Option::is_some);
            for o in h.bees.iter_mut().filter_map(Option::take) {
                self.release_bee(entities, p, o, true);
            }
            self.beehives.hives.insert(p.to_array(), h);
            if occupied {
                entities.anger_bees(p, player);
            }
        }
        self.set_block(p, block(nest, f, 0));
        true
    }
    pub fn mine_beehive(
        &mut self,
        entities: &mut Entities,
        p: IVec3,
        b: Block,
        held: Option<Stack>,
        player: PlayerId,
        drops: bool,
    ) -> bool {
        let Some((nest, _, honey)) = hive(b) else {
            return false;
        };
        let mut h = self.beehives.hives.remove(&p.to_array()).unwrap_or_default();
        h.honey = honey;
        let silk = held.is_some_and(|s| s.active_enchants().has(crate::enchant::Enchantment::SilkTouch));
        if silk {
            if drops {
                let mut s = Stack::new(if nest { NEST } else { HIVE }, 1);
                s.hive = Some(h.intern());
                self.drops.push((p, s));
            }
        } else {
            for o in h.bees.into_iter().flatten() {
                self.release_bee(entities, p, o, true);
            }
            // playerDestroy always alerts nearby bees without Silk Touch;
            // smoke only protects honey harvesting, not mining.
            entities.anger_bees(p, player);
            if drops && !nest {
                self.drops.push((p, Stack::new(HIVE, 1)));
            }
        }
        true
    }
    pub fn bee_grow_crop(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else {
            return false;
        };
        let next = if let Some(age) = super::sniffer_plants::torch_age(b) {
            Some(if age == 0 { Block(b.0 + 1) } else { super::sniffer_plants::TORCHFLOWER })
        } else if let Some((crop, age)) = b.as_crop() {
            (age < crop.max_age()).then(|| Block::crop(crop, age + 1))
        } else if let Some((f, age, None)) = super::crops::stem(b) {
            (age < 7).then(|| super::crops::stem_block(f, age + 1))
        } else if let Some(age) = super::overworld_blocks::berry_age(b) {
            (age < 3).then(|| super::overworld_blocks::berry_bush(age + 1))
        } else {
            None
        };
        if matches!(b.name(), "cave vines" | "cave vines plant") && b.emission() == 0 {
            return self.apply_bone_meal(p);
        }
        next.is_some_and(|b| self.set_block(p, b))
    }
}

pub(super) fn sapling_flower(b: Block) -> bool {
    crate::entity::bee::flower(b) || matches!(b.name(), "closed eyeblossom" | "golden dandelion")
}

/// BeehiveDecorator: one SOUTH-facing nest beside a log at the first leaf
/// layer minus one (at least the second log), with an unobstructed exit.
pub fn decorate_tree(biome: super::terrain::Biome, tree: &[(IVec3, Block)], seed: u64) -> Option<(IVec3, Block)> {
    use super::terrain::Biome;
    if !tree.iter().any(|(_, b)| matches!(*b, Block::LOG | Block::BIRCH_LOG | Block::CHERRY_LOG | Block::MANGROVE_LOG))
    {
        return None;
    }
    let chance = match biome {
        Biome::Meadow => 1.,
        Biome::CherryGrove => 0.05,
        Biome::MangroveSwamp => 0.01,
        Biome::Plains | Biome::SunflowerPlains => 0.05,
        Biome::FlowerForest => 0.02,
        Biome::Forest | Biome::BirchForest | Biome::OldGrowthBirchForest => 0.002,
        _ => return None,
    };
    decorate_tree_with_chance(tree, seed, chance)
}

pub(super) fn decorate_tree_with_chance(tree: &[(IVec3, Block)], seed: u64, chance: f32) -> Option<(IVec3, Block)> {
    let mut rng = seed;
    let roll = (super::noise::splitmix64(&mut rng) >> 40) as f32 / (1u64 << 24) as f32;
    if roll >= chance {
        return None;
    }
    let bottom = tree.iter().filter(|(_, b)| b.is_log()).map(|(p, _)| p.y).min()?;
    let top = tree.iter().filter(|(_, b)| b.is_log()).map(|(p, _)| p.y).max()?;
    let y = tree
        .iter()
        .filter(|(_, b)| b.is_leaves())
        .map(|(p, _)| p.y)
        .min()
        .map_or_else(|| (bottom + 1 + (rng % 3) as i32).min(top), |y| (y - 1).max(bottom + 1));
    let mut candidates: Vec<_> = tree
        .iter()
        .filter(|(p, b)| p.y == y && b.is_log())
        .flat_map(|(p, _)| [IVec3::X, IVec3::NEG_X, IVec3::Z].map(|d| *p + d))
        .collect();
    for i in (1..candidates.len()).rev() {
        let j = (super::noise::splitmix64(&mut rng) % (i as u64 + 1)) as usize;
        candidates.swap(i, j);
    }
    candidates
        .into_iter()
        .find(|p| !tree.iter().any(|(q, _)| *q == *p || *q == *p + IVec3::Z))
        .map(|p| (p, block(true, Facing::South, 0)))
}

#[cfg(test)]
mod tests;
