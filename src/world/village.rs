//! Surface villages: terrain-projected, connector-driven pieces in five biome
//! palettes. Placement matches Java 1.21 random_spread; piece artwork is an
//! original compact pool rather than a copy of Minecraft's NBT templates.
use super::block::{Block, Facing};
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME};
use super::fortress::Feature;
use super::structure::{Bounds, Oriented, Paint, Rng};
use super::terrain::{Biome, Dimension, Generator, SEA_LEVEL};
use crate::enchant::JavaRandom;
use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

pub const SPACING: i32 = 34;
pub const SEPARATION: i32 = 8;
pub const SALT: u64 = 10_387_312;
const REGION: i32 = SPACING * 16;
const REACH: i32 = 80;
const EXTENT: i32 = REACH + 12;
const CACHE_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Style {
    Plains,
    Desert,
    Savanna,
    Taiga,
    Snowy,
}
impl Style {
    pub fn of(b: Biome) -> Option<Self> {
        Some(match b {
            Biome::Plains | Biome::Meadow => Self::Plains,
            Biome::Desert => Self::Desert,
            Biome::Savanna => Self::Savanna,
            Biome::Taiga => Self::Taiga,
            Biome::SnowyPlains => Self::Snowy,
            _ => return None,
        })
    }
    fn materials(self) -> (Block, Block, Block) {
        match self {
            Self::Desert => (Block::SANDSTONE, Block::SANDSTONE, Block::SANDSTONE),
            Self::Savanna => (Block::ACACIA_LOG, Block::ACACIA_PLANKS, Block::TERRACOTTA),
            Self::Taiga | Self::Snowy => (Block::SPRUCE_LOG, Block::SPRUCE_PLANKS, Block::COBBLESTONE),
            Self::Plains => (Block::LOG, Block::PLANKS, Block::COBBLESTONE),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Meeting,
    Street,
    House,
    Farm,
}
#[derive(Clone, Debug)]
pub struct Piece {
    pub kind: Kind,
    pub bounds: Bounds,
    pub facing: Facing,
    pub job: Block,
    seed: u64,
}
impl Oriented for Piece {
    fn facing(&self) -> Facing {
        self.facing
    }
    fn bounds(&self) -> &Bounds {
        &self.bounds
    }
}
pub struct Village {
    pub center: IVec3,
    pub style: Style,
    pub pieces: Vec<Piece>,
    pub bounds: Bounds,
}
struct Builder<'a> {
    generator: &'a Generator,
    center: IVec3,
    rng: Rng,
    pieces: Vec<Piece>,
}
// Available Java profession sites. Farm pieces add composters separately.
const JOBS: [Block; 12] = [
    Block::COMPOSTER,
    Block::BARREL,
    Block::SMOKER,
    Block::BLAST_FURNACE,
    Block::CARTOGRAPHY_TABLE,
    Block::FLETCHING_TABLE,
    Block::GRINDSTONE,
    Block::LECTERN,
    Block::LOOM,
    Block::STONECUTTER,
    Block::SMITHING_TABLE,
    Block::BREWING_STAND,
];
impl Builder<'_> {
    fn add(&mut self, kind: Kind, door: IVec3, facing: Facing) -> Option<usize> {
        if (door.x - self.center.x).abs() > REACH || (door.z - self.center.z).abs() > REACH {
            return None;
        }
        let (size, off) = match kind {
            Kind::Street => (IVec3::new(3, 5, 11), -1),
            Kind::House => (IVec3::new(7, 7, 7), -3),
            Kind::Farm => (IVec3::new(9, 3, 9), -4),
            Kind::Meeting => unreachable!(),
        };
        let mut bounds = Bounds::oriented(door, IVec3::new(off, 0, 0), size, facing);
        if self.pieces.iter().any(|p| horizontal_intersects(&p.bounds, &bounds)) {
            return None;
        }
        let middle = (bounds.min + bounds.max) / 2;
        let col = self.generator.column(middle.x, middle.z);
        if col.height <= SEA_LEVEL || col.biome.is_watery() || col.biome.is_swamp() {
            return None;
        }
        // Rigid buildings sit at their entrance level, terrain is filled below.
        // Reject cliffs rather than constructing inaccessible floating houses.
        let entry = self.generator.column(door.x, door.z).height;
        if kind != Kind::Street && (entry - door.y + 1).abs() > 1 {
            return None;
        }
        let y = entry + 1;
        let mut low = y;
        let mut high = y;
        for x in [bounds.min.x, bounds.max.x] {
            for z in [bounds.min.z, bounds.max.z] {
                let h = self.generator.column(x, z).height + 1;
                low = low.min(h);
                high = high.max(h);
            }
        }
        if high - low > if kind == Kind::Street { 6 } else { 3 } {
            return None;
        }
        bounds.min.y = if kind == Kind::Street { low } else { y };
        bounds.max.y = if kind == Kind::Street { high + 4 } else { y + size.y - 1 };
        let job = JOBS[self.rng.below(JOBS.len() as u32) as usize];
        self.pieces.push(Piece { kind, bounds, facing, job, seed: self.rng.next_u64() });
        Some(self.pieces.len() - 1)
    }
    fn grow(mut self, style: Style) -> Village {
        let mut pending = Vec::new();
        for f in Facing::ALL {
            let d = f.offset();
            pending.push((self.center + d * 5, f, 1));
        }
        while let Some((door, facing, depth)) = pending.pop() {
            let Some(i) = self.add(Kind::Street, door, facing) else { continue };
            let branch = (depth < 6 && self.rng.below(3) == 0).then(|| self.rng.below(2) == 0);
            for off in [2, 8] {
                for side in [true, false] {
                    if off == 8 && branch == Some(side) {
                        continue;
                    }
                    let (mut at, f) = self.pieces[i].side(side, 0, off);
                    // Terrain-projected road sets the connector's ground level.
                    at.y = self.generator.column(at.x - f.offset().x, at.z - f.offset().z).height + 1;
                    let kind = if self.rng.below(5) == 0 { Kind::Farm } else { Kind::House };
                    self.add(kind, at, f);
                }
            }
            if depth < 6 {
                let (mut at, f) = self.pieces[i].ahead(1, 0);
                at.y = self.generator.column(at.x, at.z).height + 1;
                if self.rng.below(4) != 0 {
                    pending.push((at, f, depth + 1));
                }
                if let Some(side) = branch {
                    let (mut at, f) = self.pieces[i].side(side, 0, 8);
                    at.y = self.generator.column(at.x, at.z).height + 1;
                    pending.push((at, f, depth + 1));
                }
            }
        }
        let bounds = self.pieces.iter().fold(self.pieces[0].bounds, |a, p| a.union(&p.bounds));
        Village { center: self.center, style, pieces: self.pieces, bounds }
    }
}
fn horizontal_intersects(a: &Bounds, b: &Bounds) -> bool {
    a.min.x <= b.max.x && a.max.x >= b.min.x && a.min.z <= b.max.z && a.max.z >= b.min.z
}
impl Village {
    fn generate(g: &Generator, center: IVec3, style: Style, seed: u64) -> Self {
        let bounds = Bounds { min: center - IVec3::new(4, 0, 4), max: center + IVec3::new(4, 6, 4) };
        let meeting = Piece { kind: Kind::Meeting, bounds, facing: Facing::South, job: Block::BELL, seed };
        Builder { generator: g, center, rng: Rng(seed), pieces: vec![meeting] }.grow(style)
    }
    pub fn homes(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.pieces.iter().filter(|p| p.kind == Kind::House).map(|p| p.world(1, 0, 4))
    }
    fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        let (log, planks, stone) = self.style.materials();
        for piece in &self.pieces {
            if !horizontal_intersects(&piece.bounds, &chunk) {
                continue;
            }
            if piece.kind == Kind::Street {
                for z in piece.bounds.min.z.max(chunk.min.z)..=piece.bounds.max.z.min(chunk.max.z) {
                    for x in piece.bounds.min.x.max(chunk.min.x)..=piece.bounds.max.x.min(chunk.max.x) {
                        let y = g.column(x, z).height;
                        for wy in y..=y + 3 {
                            if !(base.y..base.y + CHUNK_SIZE_I).contains(&wy) {
                                continue;
                            }
                            let i = super::chunk::index(
                                (x - base.x) as usize,
                                (wy - base.y) as usize,
                                (z - base.z) as usize,
                            );
                            blocks[i] = if wy == y { Block::DIRT_PATH } else { Block::AIR };
                        }
                    }
                }
                continue;
            }
            let ground =
                |x: i32, z: i32, from: i32, to: i32| from > g.column(x, z).height && to > g.column(x, z).height;
            let mut p = Paint { blocks, base, top: chunk.max, piece, open: &ground, pillar: stone };
            match piece.kind {
                Kind::Meeting => {
                    p.fill(0, 0, 0, 8, 30, 8, Block::AIR);
                    p.fill(0, -1, 0, 8, -1, 8, Block::DIRT_PATH);
                    // A covered well and bell at the gathering point.
                    p.fill(2, -1, 2, 6, -1, 6, stone);
                    p.fill(3, -1, 3, 5, -1, 5, Block::WATER);
                    for x in [2, 6] {
                        for z in [2, 6] {
                            p.fill(x, 0, z, x, 3, z, Block::OAK_FENCE);
                        }
                    }
                    p.fill(2, 4, 2, 6, 4, 6, planks);
                    p.set(1, 0, 4, Block::BELL);
                    for x in 0..=8 {
                        for z in 0..=8 {
                            p.column_down(x, -2, z);
                        }
                    }
                }
                Kind::Farm => {
                    p.fill(0, 0, 0, 8, 30, 8, Block::AIR);
                    p.fill(0, -1, 0, 8, -1, 8, log);
                    p.fill(1, -1, 1, 7, -1, 7, Block::WET_FARMLAND);
                    p.fill(4, -1, 1, 4, -1, 7, Block::WATER);
                    for x in 1..=7 {
                        for z in 1..=7 {
                            if x != 4 {
                                let age = (super::noise::hash3(x, 0, z, piece.seed) % 8) as u8;
                                let crop = match (piece.seed + x as u64) % 3 {
                                    0 => Block::wheat(age),
                                    1 => Block::crop(super::block::Crop::Carrot, age),
                                    _ => Block::crop(super::block::Crop::Potato, age),
                                };
                                p.set(x, 0, z, crop);
                            }
                        }
                    }
                    p.set(0, 0, 4, Block::COMPOSTER);
                    p.set(8, 0, 7, Block::HAY_BALE);
                    for x in 0..=8 {
                        for z in 0..=8 {
                            p.column_down(x, -2, z);
                        }
                    }
                }
                Kind::House => {
                    p.fill(0, 0, 0, 6, 30, 6, Block::AIR);
                    p.fill(0, -1, 0, 6, -1, 6, stone);
                    p.fill(0, 0, 0, 6, 3, 6, planks);
                    p.fill(1, 0, 1, 5, 3, 5, Block::AIR);
                    for x in [0, 6] {
                        for z in [0, 6] {
                            p.fill(x, 0, z, x, 3, z, log);
                        }
                    }
                    p.fill(0, 1, 2, 0, 2, 3, Block::GLASS_PANE);
                    p.fill(6, 1, 2, 6, 2, 3, Block::GLASS_PANE);
                    p.fill(2, 1, 6, 3, 2, 6, Block::GLASS_PANE);
                    p.set(3, 0, 0, Block::AIR);
                    p.set(3, 1, 0, Block::AIR);
                    let door = Block::door(piece.facing.opposite(), false, false);
                    p.set(3, 0, 0, door);
                    p.set(3, 1, 0, Block::door(piece.facing.opposite(), false, true));
                    if self.style == Style::Desert {
                        p.fill(0, 4, 0, 6, 4, 6, stone);
                    } else {
                        for x in 0..=6 {
                            let h = 4 + x.min(6 - x) / 2;
                            p.fill(x, h, 0, x, h, 6, if self.style == Style::Snowy { Block::SNOW } else { planks });
                            if x > 0 && x < 6 {
                                p.fill(x, 4, 0, x, h, 0, planks);
                                p.fill(x, 4, 6, x, h, 6, planks);
                            }
                        }
                    }
                    // Beds follow the piece's +z axis (head beyond foot).
                    p.set(1, 0, 3, Block::BED_FOOT);
                    p.set(1, 0, 4, Block::BED_HEAD);
                    p.set(5, 0, 4, p.facing(piece.job, Facing::West));
                    p.set(5, 0, 2, p.facing(Block::CHEST, Facing::North));
                    p.set(2, 0, 5, Block::TORCH);
                    // Lamp beside the doorway without intersecting the street.
                    p.fill(6, 0, 0, 6, 2, 0, Block::OAK_FENCE);
                    p.set(6, 3, 0, Block::GLOWSTONE);
                    for x in 0..=6 {
                        for z in 0..=6 {
                            p.column_down(x, -2, z);
                        }
                    }
                }
                Kind::Street => unreachable!(),
            }
        }
    }
}
pub struct Villages {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Option<Arc<Village>>>>,
}
impl Villages {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }
    /// Java WorldgenRandom.setLargeFeatureWithSalt + linear spread (16-block chunks).
    pub fn candidate(&self, region: IVec2) -> IVec2 {
        let seed = self
            .seed
            .wrapping_add((region.x as i64).wrapping_mul(341_873_128_712) as u64)
            .wrapping_add((region.y as i64).wrapping_mul(132_897_987_541) as u64)
            .wrapping_add(SALT);
        let mut r = JavaRandom::new(seed as i64);
        (region * SPACING + IVec2::new(r.next_bounded(SPACING - SEPARATION), r.next_bounded(SPACING - SEPARATION))) * 16
    }
    fn start(&self, g: &Generator, region: IVec2) -> Option<(IVec3, Style)> {
        if g.dimension != Dimension::Overworld {
            return None;
        }
        let at = self.candidate(region);
        let c = g.column(at.x, at.y);
        if c.height <= SEA_LEVEL {
            return None;
        }
        Some((IVec3::new(at.x, c.height + 1, at.y), Style::of(c.biome)?))
    }
    pub fn get(&self, g: &Generator, region: IVec2) -> Option<Arc<Village>> {
        if let Some(v) = self.cache.lock().unwrap().get(&region) {
            return v.clone();
        }
        let v = self.start(g, region).map(|(at, style)| {
            Arc::new(Village::generate(g, at, style, super::noise::hash3(region.x, 0, region.y, self.seed ^ SALT)))
        });
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        cache.entry(region).or_insert(v).clone()
    }
    fn visit(&self, g: &Generator, base: IVec3, mut f: impl FnMut(&Village)) {
        if g.dimension != Dimension::Overworld {
            return;
        }
        for z in (base.z - EXTENT).div_euclid(REGION)..=(base.z + CHUNK_SIZE_I - 1 + EXTENT).div_euclid(REGION) {
            for x in (base.x - EXTENT).div_euclid(REGION)..=(base.x + CHUNK_SIZE_I - 1 + EXTENT).div_euclid(REGION) {
                let region = IVec2::new(x, z);
                let at = self.candidate(region);
                if at.x + EXTENT < base.x
                    || at.x - EXTENT >= base.x + CHUNK_SIZE_I
                    || at.y + EXTENT < base.z
                    || at.y - EXTENT >= base.z + CHUNK_SIZE_I
                {
                    continue;
                }
                if let Some(v) = self.get(g, region) {
                    f(&v);
                }
            }
        }
    }
    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        self.visit(g, base, |v| v.paint(g, blocks, base));
    }
    pub fn features(&self, g: &Generator, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let bounds = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        let mut out = Vec::new();
        self.visit(g, base, |v| {
            for p in &v.pieces {
                if p.kind == Kind::House {
                    let home = p.world(1, 0, 4);
                    if bounds.contains(home) {
                        out.push((home, Feature::VillageHome(p.world(3, 0, 3))));
                    }
                    let chest = p.world(5, 0, 2);
                    if bounds.contains(chest) {
                        out.push((chest, Feature::VillageChest(p.seed, v.style)));
                    }
                    let job = p.world(5, 0, 4);
                    if bounds.contains(job) {
                        out.push((job, Feature::VillageWorkstation(p.job)));
                    }
                }
            }
        });
        out
    }
    pub fn nearest(&self, g: &Generator, from: IVec3) -> Option<IVec3> {
        if g.dimension != Dimension::Overworld {
            return None;
        }
        let region = IVec2::new(from.x.div_euclid(REGION), from.z.div_euclid(REGION));
        let mut best = None;
        let mut distance = i64::MAX;
        for ring in 0i32..=24 {
            for z in -ring..=ring {
                for x in -ring..=ring {
                    if x.abs() != ring && z.abs() != ring {
                        continue;
                    }
                    if let Some((at, _)) = self.start(g, region + IVec2::new(x, z)) {
                        let dx = at.x as i64 - from.x as i64;
                        let dz = at.z as i64 - from.z as i64;
                        let d = dx * dx + dz * dz;
                        if d < distance {
                            best = Some(at);
                            distance = d;
                        }
                    }
                }
            }
            // Every candidate beyond this ring is at least this far away.
            if ring > 0 && i64::from((ring - 1) * REGION).pow(2) > distance {
                break;
            }
        }
        best
    }
}

// Java 1.21 chests/village/village_*_house.json weights/counts.
// Missing items remain weighted empty rolls; supported items are not inflated.
type Loot = (&'static str, u32, u8, u8);
const PLAINS_LOOT: &[Loot] = &[
    ("gold_nugget", 1, 1, 3),
    ("dandelion", 2, 1, 1),
    ("poppy", 1, 1, 1),
    ("potato", 10, 1, 7),
    ("bread", 10, 1, 4),
    ("apple", 10, 1, 5),
    ("book", 1, 1, 1),
    ("feather", 1, 1, 1),
    ("emerald", 2, 1, 4),
    ("oak_sapling", 5, 1, 2),
];
const DESERT_LOOT: &[Loot] = &[
    ("clay_ball", 1, 1, 1),
    ("green_dye", 1, 1, 1),
    ("cactus", 10, 1, 4),
    ("wheat", 10, 1, 7),
    ("bread", 10, 1, 4),
    ("book", 1, 1, 1),
    ("dead_bush", 2, 1, 3),
    ("emerald", 1, 1, 3),
];
const SAVANNA_LOOT: &[Loot] = &[
    ("gold_nugget", 1, 1, 3),
    ("short_grass", 5, 1, 1),
    ("tall_grass", 5, 1, 1),
    ("bread", 10, 1, 4),
    ("wheat_seeds", 10, 1, 5),
    ("emerald", 2, 1, 4),
    ("acacia_sapling", 10, 1, 2),
    ("saddle", 1, 1, 1),
    ("torch", 1, 1, 2),
    ("bucket", 1, 1, 1),
];
const TAIGA_LOOT: &[Loot] = &[
    ("iron_nugget", 1, 1, 5),
    ("fern", 2, 1, 1),
    ("large_fern", 2, 1, 1),
    ("potato", 10, 1, 7),
    ("sweet_berries", 5, 1, 7),
    ("bread", 10, 1, 4),
    ("pumpkin_seeds", 5, 1, 5),
    ("pumpkin_pie", 1, 1, 1),
    ("emerald", 2, 1, 4),
    ("spruce_sapling", 5, 1, 5),
    ("spruce_sign", 1, 1, 1),
    ("spruce_log", 10, 1, 5),
];
const SNOWY_LOOT: &[Loot] = &[
    ("blue_ice", 1, 1, 1),
    ("snow_block", 4, 1, 1),
    ("potato", 10, 1, 7),
    ("bread", 10, 1, 4),
    ("beetroot_seeds", 10, 1, 5),
    ("beetroot_soup", 1, 1, 1),
    ("furnace", 1, 1, 1),
    ("emerald", 1, 1, 4),
    ("snowball", 10, 1, 7),
    ("coal", 5, 1, 4),
];
pub fn loot(seed: u64, style: Style) -> super::chest::Chest {
    let table = match style {
        Style::Plains => PLAINS_LOOT,
        Style::Desert => DESERT_LOOT,
        Style::Savanna => SAVANNA_LOOT,
        Style::Taiga => TAIGA_LOOT,
        Style::Snowy => SNOWY_LOOT,
    };
    let mut chest = super::chest::Chest::default();
    let mut rng = Rng(seed);
    let total = table.iter().map(|e| e.1).sum();
    for _ in 0..rng.range(3, 8) {
        let mut roll = rng.below(total);
        for &(name, w, lo, hi) in table {
            if roll >= w {
                roll -= w;
                continue;
            }
            let count = rng.range(lo as u32, hi as u32) as u8;
            let name = match name {
                "short_grass" => "tall_grass",
                "snow_block" => "snow",
                _ => name,
            };
            if let Some(item) = crate::item::Item::from_name(name) {
                let mut slot = rng.below(super::chest::SLOTS as u32) as usize;
                while chest.slots[slot].is_some() {
                    slot = (slot + 1) % super::chest::SLOTS;
                }
                chest.slots[slot] = Some(crate::inventory::Stack::new(item, count));
            }
            break;
        }
    }
    chest
}

#[cfg(test)]
mod tests {
    use super::super::chunk::index;
    use super::*;
    #[test]
    fn placement_uses_java_spacing_separation_and_signed_regions() {
        let v = Villages::new(42);
        for z in -4..=4 {
            for x in -4..=4 {
                let at = v.candidate(IVec2::new(x, z)) / 16 - IVec2::new(x, z) * 34;
                assert!(at.cmpge(IVec2::ZERO).all() && at.cmplt(IVec2::splat(26)).all());
            }
        }
        assert_eq!(v.candidate(IVec2::ZERO), IVec2::new(144, 176));
    }
    #[test]
    fn all_five_biomes_have_connected_non_overlapping_villages() {
        let g = Generator::new(42);
        let mut found = [false; 5];
        for z in -12..=12 {
            for x in -12..=12 {
                if let Some(v) = g.villages.get(&g, IVec2::new(x, z)) {
                    let first = !found[v.style as usize];
                    found[v.style as usize] = true;
                    for (i, a) in v.pieces.iter().enumerate() {
                        for b in &v.pieces[i + 1..] {
                            assert!(!horizontal_intersects(&a.bounds, &b.bounds));
                        }
                    }
                    assert!(v.pieces.iter().all(|p| (p.bounds.min.x - v.center.x).abs() <= EXTENT
                        && (p.bounds.min.z - v.center.z).abs() <= EXTENT));
                    if first {
                        println!(
                            "village {:?}: {} {} {}, {} houses",
                            v.style,
                            v.center.x,
                            v.center.y,
                            v.center.z,
                            v.homes().count()
                        );
                    }
                }
            }
        }
        assert!(found.into_iter().all(|v| v));
        assert_eq!(g.villages.nearest(&Generator::for_dimension(42, Dimension::Nether), IVec3::ZERO), None);
        let at = g.villages.nearest(&g, IVec3::ZERO).unwrap();
        assert!(Style::of(g.column(at.x, at.z).biome).is_some());
    }
    fn ground(g: &Generator, base: IVec3) -> Box<[Block; CHUNK_VOLUME]> {
        let mut b = super::super::chunk::ChunkData::new_dense(Block::AIR);
        for z in 0..32 {
            for x in 0..32 {
                let h = g.column(base.x + x, base.z + z).height;
                for y in 0..32 {
                    if base.y + y <= h {
                        b[index(x as usize, y as usize, z as usize)] = Block::STONE;
                    }
                }
            }
        }
        b
    }
    #[test]
    fn village_painting_agrees_in_overlapping_chunks_and_preserves_features() {
        let g = Generator::new(42);
        let at = g.villages.nearest(&g, IVec3::ZERO).unwrap();
        let region = IVec2::new(at.x.div_euclid(REGION), at.z.div_euclid(REGION));
        let v = g.villages.get(&g, region).unwrap();
        let base = v.center - IVec3::new(12, 8, 12);
        let next = base + IVec3::new(1, 1, 1);
        let mut a = ground(&g, base);
        let mut b = ground(&g, next);
        v.paint(&g, &mut a, base);
        v.paint(&g, &mut b, next);
        for y in 1..32 {
            for z in 1..32 {
                for x in 1..32 {
                    assert_eq!(a[index(x, y, z)], b[index(x - 1, y - 1, z - 1)], "{x} {y} {z}");
                }
            }
        }
        for p in v.pieces.iter().filter(|p| p.kind == Kind::House) {
            for (at, want) in [(p.world(1, 0, 4), Block::BED_HEAD), (p.world(5, 0, 2), Block::CHEST)] {
                let c = super::super::chunk::chunk_of(at);
                let data = g.generate(c);
                let l = super::super::chunk::local_of(at);
                assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize).base(), want);
            }
        }
        let before = v.pieces.len();
        g.villages.cache.lock().unwrap().clear();
        assert_eq!(g.villages.get(&g, region).unwrap().pieces.len(), before);
    }
    #[test]
    fn village_loot_is_seeded_and_follows_java_counts() {
        for style in [Style::Plains, Style::Desert, Style::Savanna, Style::Taiga, Style::Snowy] {
            for seed in 0..100 {
                let a = loot(seed, style);
                assert_eq!(a, loot(seed, style));
                assert!(a.slots.iter().flatten().all(|s| s.count > 0 && s.count <= 7));
            }
        }
    }
}

#[cfg(test)]
mod registration_tests {
    use super::*;
    #[test]
    fn generated_containers_work_and_saved_empty_chests_never_refill() {
        let g = Arc::new(Generator::new(42));
        let at = g.villages.nearest(&g, IVec3::ZERO).unwrap();
        let v = g.villages.get(&g, IVec2::new(at.x.div_euclid(REGION), at.z.div_euclid(REGION))).unwrap();
        let mut world = super::super::World::new_headless(Arc::clone(&g), Default::default(), 1);
        for p in v.pieces.iter().filter(|p| p.kind == Kind::House) {
            let chest = p.world(5, 0, 2);
            let c = super::super::chunk::chunk_of(chest);
            world.register_structure_features(c, &g.generate(c));
            assert!(world.chest(chest).is_some());
            world.chest_mut(chest).unwrap().slots.fill(None);
            let saved = world.chests_to_string();
            let mut other = super::super::World::new_headless(Arc::clone(&g), Default::default(), 1);
            other.load_chests(&saved);
            other.register_structure_features(c, &g.generate(c));
            assert!(other.chest(chest).unwrap().slots.iter().all(Option::is_none));
            let at = p.world(5, 0, 4);
            let c = super::super::chunk::chunk_of(at);
            world.register_structure_features(c, &g.generate(c));
            if super::super::furnace::is_furnace(p.job) {
                assert_eq!(world.furnace(at).unwrap().kind, super::super::furnace::FurnaceKind::of(p.job));
            }
            if p.job == Block::BARREL {
                assert!(world.chest(at).is_some());
            }
        }
    }
}
