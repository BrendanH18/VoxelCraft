//! Monster rooms, Java's `MonsterRoomFeature` terrain feature. Each 16x16
//! column makes ten attempts over this world's full height and four attempts
//! in Java's deep band (`above_bottom` 6 through absolute -1, that is
//! y = -58..=-1). Layouts are seeded
//! per column, validated against pristine terrain and painted in every
//! touching 32³ chunk, so loading order cannot cut a room.

use std::sync::{Arc, Mutex};

use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chest::{Chest, SLOTS, is_chest};
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, index};
use super::fortress::Feature;
use super::noise::hash3;
use super::structure::{Bounds, Rng};
use super::terrain::Generator;
use crate::entity::MobKind;
use crate::inventory::Stack;
use crate::item::Item;

const FULL_SALT: u64 = 0x004D_4F4E_5354_4552;
const DEEP_SALT: u64 = 0x004D_4F4E_4445_4550;
const CACHE_LIMIT: usize = 512;
const REACH: i32 = 4;
const MOBS: [MobKind; 4] = [MobKind::Skeleton, MobKind::Zombie, MobKind::Zombie, MobKind::Spider];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    pub center: IVec3,
    pub bounds: Bounds,
    rx: i32,
    rz: i32,
    seed: u64,
    mob: MobKind,
    chests: [Option<IVec3>; 2],
}

impl Room {
    fn new(center: IVec3, rng: &mut Rng) -> Self {
        let rx = rng.range(2, 3) as i32;
        let rz = rng.range(2, 3) as i32;
        let bounds =
            Bounds { min: center + IVec3::new(-rx - 1, -1, -rz - 1), max: center + IVec3::new(rx + 1, 4, rz + 1) };
        let seed = rng.next_u64();
        Self { center, bounds, rx, rz, seed, mob: MobKind::Zombie, chests: [None; 2] }
    }

    /// Java requires solid floor and roof at every cell and one to five
    /// two-block-high air openings on the perimeter.
    fn valid(&self, mut at: impl FnMut(IVec3) -> Block) -> bool {
        let (x0, x1, z0, z1) = (-self.rx - 1, self.rx + 1, -self.rz - 1, self.rz + 1);
        let mut openings = 0;
        for x in x0..=x1 {
            for z in z0..=z1 {
                if !at(self.center + IVec3::new(x, -1, z)).is_solid()
                    || !at(self.center + IVec3::new(x, 4, z)).is_solid()
                {
                    return false;
                }
                if (x == x0 || x == x1 || z == z0 || z == z1)
                    && at(self.center + IVec3::new(x, 0, z)) == Block::AIR
                    && at(self.center + IVec3::new(x, 1, z)) == Block::AIR
                {
                    openings += 1;
                    if openings > 5 {
                        return false;
                    }
                }
            }
        }
        openings >= 1
    }

    fn decorate(&mut self, mut rng: Rng, mut at: impl FnMut(IVec3) -> Block) {
        // Java makes two passes of three attempts each, choosing an empty
        // interior cell with exactly one solid horizontal neighbour.
        for slot in 0..self.chests.len() {
            for _ in 0..3 {
                let x = rng.range(0, (self.rx * 2) as u32) as i32 - self.rx;
                let z = rng.range(0, (self.rz * 2) as u32) as i32 - self.rz;
                let p = self.center + IVec3::new(x, 0, z);
                if self.chests.contains(&Some(p)) {
                    continue;
                }
                let walls = [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
                    .into_iter()
                    .filter(|&(nx, nz)| {
                        (nx.abs() == self.rx + 1 || nz.abs() == self.rz + 1)
                            && at(self.center + IVec3::new(nx, 0, nz)).is_solid()
                    })
                    .count();
                if walls == 1 {
                    self.chests[slot] = Some(p);
                    break;
                }
            }
        }
        self.mob = MOBS[rng.below(MOBS.len() as u32) as usize];
    }

    fn put(blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, p: IVec3, block: Block) {
        let l = p - base;
        if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
            blocks[index(l.x as usize, l.y as usize, l.z as usize)] = block;
        }
    }

    fn paint(&self, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3, mut natural: impl FnMut(IVec3) -> Block) {
        let top = base + IVec3::splat(CHUNK_SIZE_I - 1);
        if !self.bounds.intersects(&Bounds { min: base, max: top }) {
            return;
        }
        for x in -self.rx - 1..=self.rx + 1 {
            for z in -self.rz - 1..=self.rz + 1 {
                // Java works downward so support checks see the terrain,
                // not the floor this room just placed.
                for y in (-1..=3).rev() {
                    let p = self.center + IVec3::new(x, y, z);
                    let Some(current) = blocks_at(blocks, base, p) else { continue };
                    let shell = x.abs() == self.rx + 1 || z.abs() == self.rz + 1 || y == -1;
                    if !shell {
                        if !is_chest(current) && current != Block::SPAWNER {
                            Self::put(blocks, base, p, Block::AIR);
                        }
                        continue;
                    }
                    let below = blocks_at(blocks, base, p - IVec3::Y).unwrap_or_else(|| natural(p - IVec3::Y));
                    if !below.is_solid() {
                        Self::put(blocks, base, p, Block::AIR);
                    } else if current.is_solid() && !is_chest(current) {
                        let block = if y == -1 && !hash3(p.x, p.y, p.z, self.seed).is_multiple_of(4) {
                            Block::MOSSY_COBBLESTONE
                        } else {
                            Block::COBBLESTONE
                        };
                        Self::put(blocks, base, p, block);
                    }
                    // Existing cave air in the wall stays as an entrance.
                }
            }
        }
        Self::put(blocks, base, self.center, Block::SPAWNER);
        for p in self.chests.into_iter().flatten() {
            Self::put(blocks, base, p, Block::CHEST);
        }
    }

    fn features(&self, chunk: Bounds, out: &mut Vec<(IVec3, Feature)>) {
        if chunk.contains(self.center) {
            out.push((self.center, Feature::Spawner(self.mob)));
        }
        for (i, p) in self.chests.into_iter().enumerate() {
            if let Some(p) = p
                && chunk.contains(p)
            {
                out.push((p, Feature::DungeonChest(self.seed ^ (i as u64 + 1))));
            }
        }
    }
}

fn blocks_at(blocks: &[Block; CHUNK_VOLUME], base: IVec3, p: IVec3) -> Option<Block> {
    let l = p - base;
    (l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all())
        .then(|| blocks[index(l.x as usize, l.y as usize, l.z as usize)])
}

pub struct Dungeons {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Arc<Vec<Room>>>>,
}

impl Dungeons {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    fn source(&self, generator: &Generator, column: IVec2) -> Arc<Vec<Room>> {
        if let Some(found) = self.cache.lock().unwrap().get(&column) {
            return found.clone();
        }
        let mut rooms = Vec::new();
        let mut heights = FxHashMap::default();
        let mut nodes = FxHashMap::default();
        for sub_z in 0..2 {
            for sub_x in 0..2 {
                let chunk16 = column * 2 + IVec2::new(sub_x, sub_z);
                for (salt, attempts, deep) in [(FULL_SALT, 10, false), (DEEP_SALT, 4, true)] {
                    let mut rng = Rng(hash3(chunk16.x, 0, chunk16.y, self.seed ^ salt));
                    for _ in 0..attempts {
                        let x = chunk16.x * 16 + rng.below(16) as i32;
                        let z = chunk16.y * 16 + rng.below(16) as i32;
                        // Java `monster_room_deep`: uniform from above_bottom 6
                        // (y = -58) through absolute -1. The old `6..=63` was
                        // that range plus 64.
                        let y = if deep { rng.range(0, 57) as i32 - 58 } else { rng.below(320) as i32 };
                        let center = IVec3::new(x, y, z);
                        let room = Room::new(center, &mut rng);
                        // Most upper attempts are above the surface; reject
                        // before sampling any cave noise.
                        if center.y + 4 > generator.column(center.x, center.z).height {
                            continue;
                        }
                        let at = |p| generator.natural_block(p, &mut heights, &mut nodes);
                        if !room.valid(at) {
                            continue;
                        }
                        let mut room = room;
                        room.decorate(Rng(room.seed), |p| generator.natural_block(p, &mut heights, &mut nodes));
                        rooms.push(room);
                    }
                }
            }
        }
        let found = Arc::new(rooms);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= CACHE_LIMIT
            && let Some(k) = cache.keys().next().copied()
        {
            cache.remove(&k);
        }
        cache.entry(column).or_insert(found).clone()
    }

    fn near(&self, generator: &Generator, base: IVec3) -> Vec<Room> {
        let lo = IVec2::new(base.x - REACH, base.z - REACH).div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let hi = IVec2::new(base.x + CHUNK_SIZE_I - 1 + REACH, base.z + CHUNK_SIZE_I - 1 + REACH)
            .div_euclid(IVec2::splat(CHUNK_SIZE_I));
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        let mut out = Vec::new();
        for cz in lo.y..=hi.y {
            for cx in lo.x..=hi.x {
                out.extend(
                    self.source(generator, IVec2::new(cx, cz)).iter().copied().filter(|r| r.bounds.intersects(&chunk)),
                );
            }
        }
        out.sort_by_key(|r| (r.center.x, r.center.y, r.center.z));
        out
    }

    pub fn paint(&self, generator: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        let mut heights = FxHashMap::default();
        let mut nodes = FxHashMap::default();
        for room in self.near(generator, base) {
            room.paint(blocks, base, |p| generator.natural_block(p, &mut heights, &mut nodes));
        }
    }

    pub fn features(&self, generator: &Generator, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let chunk = Bounds { min: base, max: base + IVec3::splat(CHUNK_SIZE_I - 1) };
        let mut out = Vec::new();
        for room in self.near(generator, base) {
            room.features(chunk, &mut out);
        }
        out
    }
}

// Java's simple_dungeon has three independent pools: 1-3, 1-4, and 3
// rolls. Entries with no usable VoxelCraft counterpart remain weighted
// blanks, preserving the relative odds of the items we can award. Omitted:
// golden apples, three music discs and name tags,
// redstone and melon, pumpkin and beetroot seeds.
type Entry = (Option<Item>, u32, u8, u8);
const RARE: &[Entry] = &[
    (Some(Item::SADDLE), 20, 1, 1),             // saddle
    (None, 15, 1, 1),                           // golden apple
    (None, 2, 1, 1),                            // enchanted golden apple
    (None, 2, 1, 1),                            // music disc otherside
    (None, 15, 1, 1),                           // music disc 13
    (None, 15, 1, 1),                           // music disc cat
    (None, 20, 1, 1),                           // name tag
    (Some(Item::GOLDEN_HORSE_ARMOR), 10, 1, 1), // golden horse armor
    (Some(Item::IRON_HORSE_ARMOR), 15, 1, 1),   // iron horse armor
    (Some(Item::DIAMOND_HORSE_ARMOR), 5, 1, 1), // diamond horse armor
    (Some(Item::ENCHANTED_BOOK), 10, 1, 1),
];
const COMMON: &[Entry] = &[
    (Some(Item::IRON_INGOT), 10, 1, 4),
    (Some(Item::GOLD_INGOT), 5, 1, 4),
    (Some(Item::BREAD), 20, 1, 1),
    (Some(Item::WHEAT), 20, 1, 4),
    (Some(Item::BUCKET), 10, 1, 1),
    (None, 15, 1, 4), // redstone
    (Some(Item::COAL), 15, 1, 4),
    (None, 10, 2, 4), // melon seeds
    (None, 10, 2, 4), // pumpkin seeds
    (None, 10, 2, 4), // beetroot seeds
];
const JUNK: &[Entry] = &[
    (Some(Item::BONE), 10, 1, 8),
    (Some(Item::GUNPOWDER), 10, 1, 8),
    (Some(Item::ROTTEN_FLESH), 10, 1, 8),
    (Some(Item::STRING), 10, 1, 8),
];

fn roll_pool(chest: &mut Chest, rng: &mut Rng, table: &[Entry], rolls: u32) {
    let total: u32 = table.iter().map(|e| e.1).sum();
    for _ in 0..rolls {
        let mut choice = rng.below(total);
        for &(item, weight, lo, hi) in table {
            if choice < weight {
                if let Some(item) = item {
                    let count = rng.range(lo as u32, hi as u32) as u8;
                    let empty: Vec<usize> = (0..SLOTS).filter(|&i| chest.slots[i].is_none()).collect();
                    if let Some(&slot) = empty.get(rng.below(empty.len() as u32) as usize) {
                        chest.slots[slot] = Some(Stack::new(item, count));
                    }
                }
                break;
            }
            choice -= weight;
        }
    }
}

pub fn loot(seed: u64) -> Chest {
    let mut rng = Rng(seed ^ 0x7369_6D70_6C65);
    let mut chest = Chest::default();
    let rare_rolls = rng.range(1, 3);
    roll_pool(&mut chest, &mut rng, RARE, rare_rolls);
    let common_rolls = rng.range(1, 4);
    roll_pool(&mut chest, &mut rng, COMMON, common_rolls);
    roll_pool(&mut chest, &mut rng, JUNK, 3);
    chest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_opening_and_support_rules() {
        let room = Room::new(IVec3::new(0, 30, 0), &mut Rng(3));
        let doorway = room.center + IVec3::new(room.rx + 1, 0, 0);
        let solid = |p: IVec3| {
            if p == doorway || p == doorway + IVec3::Y { Block::AIR } else { Block::STONE }
        };
        assert!(room.valid(solid));
        assert!(!room.valid(|_| Block::STONE));
        assert!(!room.valid(|p| if p == doorway - IVec3::Y { Block::AIR } else { solid(p) }));
        assert!(!room.valid(|p| if p == doorway + IVec3::Y * 4 { Block::AIR } else { solid(p) }));
        assert!(
            !room.valid(|p| {
                if p.y == room.center.y || p.y == room.center.y + 1 { Block::AIR } else { Block::STONE }
            })
        );
    }

    #[test]
    fn weighted_loot_pools_keep_java_totals_and_are_deterministic() {
        assert_eq!(RARE.iter().map(|e| e.1).sum::<u32>(), 129);
        assert_eq!(COMMON.iter().map(|e| e.1).sum::<u32>(), 125);
        assert_eq!(JUNK.iter().map(|e| e.1).sum::<u32>(), 40);
        assert_eq!(MOBS.iter().filter(|&&mob| mob == MobKind::Zombie).count(), 2);
        assert_eq!(MOBS.iter().filter(|&&mob| mob == MobKind::Skeleton).count(), 1);
        assert_eq!(MOBS.iter().filter(|&&mob| mob == MobKind::Spider).count(), 1);
        assert_eq!(loot(17).serialize(), loot(17).serialize());
        for seed in 0..100 {
            let chest = loot(seed);
            assert!(chest.slots.iter().flatten().count() <= 10);
        }
    }

    #[test]
    fn seeded_layouts_repeat_and_cross_chunk_border() {
        let generator = Generator::new(248);
        let a = generator.dungeons.source(&generator, IVec2::ZERO);
        let b = generator.dungeons.source(&generator, IVec2::ZERO);
        assert_eq!(a.as_ref(), b.as_ref());
        let found = (-4..=4)
            .flat_map(|z| (-4..=4).map(move |x| IVec2::new(x, z)))
            .find_map(|source| {
                generator
                    .dungeons
                    .source(&generator, source)
                    .iter()
                    .find(|room| {
                        let cpos = room.center.div_euclid(IVec3::splat(CHUNK_SIZE_I));
                        room.chests.iter().flatten().any(|p| p.div_euclid(IVec3::splat(CHUNK_SIZE_I)) == cpos)
                    })
                    .copied()
            })
            .expect("seed should place a monster room with a chest near the origin");
        eprintln!("dungeon at {:?}, mob {:?}, chests {:?}", found.center, found.mob, found.chests);
        let cpos = found.center.div_euclid(IVec3::splat(CHUNK_SIZE_I));
        let generated = generator.generate(cpos);
        let local = found.center - cpos * CHUNK_SIZE_I;
        assert_eq!(generated.get(local.x as usize, local.y as usize, local.z as usize), Block::SPAWNER);
        assert!(generator.dungeons.features(&generator, cpos).contains(&(found.center, Feature::Spawner(found.mob))));
        let mut world = super::super::World::new_headless(Arc::new(Generator::new(248)), Default::default(), 1);
        world.register_structure_features(cpos, &generated);
        assert_eq!(world.spawner(found.center), Some(found.mob));
        for (i, chest) in found.chests.into_iter().enumerate() {
            if let Some(chest) = chest {
                let local = chest - cpos * CHUNK_SIZE_I;
                if local.cmpge(IVec3::ZERO).all() && local.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                    assert_eq!(generated.get(local.x as usize, local.y as usize, local.z as usize), Block::CHEST);
                    assert!(
                        generator
                            .dungeons
                            .features(&generator, cpos)
                            .contains(&(chest, Feature::DungeonChest(found.seed ^ (i as u64 + 1)),))
                    );
                    assert!(world.chest(chest).is_some_and(|loot| loot.slots.iter().any(Option::is_some)));
                    world.chest_mut(chest).unwrap().slots = [None; SLOTS];
                    world.register_structure_features(cpos, &generated);
                    assert!(world.chest(chest).unwrap().slots.iter().all(Option::is_none));
                }
            }
        }
        let mut rng = Rng(9);
        let room = Room::new(IVec3::new(31, 30, 15), &mut rng);
        let mut left = [Block::STONE; CHUNK_VOLUME];
        let mut right = [Block::STONE; CHUNK_VOLUME];
        room.paint(&mut left, IVec3::new(0, 0, 0), |_| Block::STONE);
        room.paint(&mut right, IVec3::new(32, 0, 0), |_| Block::STONE);
        assert_eq!(left[index(31, 30, 15)], Block::SPAWNER);
        assert_eq!(right[index(0, 30, 15)], Block::AIR);
    }
}
