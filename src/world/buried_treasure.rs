//! Generation 3 buried treasure: Java legacy_type_2 placement and beach burial.
use super::block::{Block, Facing};
use super::chunk::{CHUNK_SIZE_I, CHUNK_VOLUME, chunk_of, index};
use super::fortress::Feature;
use super::temples::LootTable;
use super::terrain::{Dimension, Generation, Generator};
use crate::enchant::JavaRandom;
use glam::{IVec2, IVec3};
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};

type Build = Arc<Vec<(IVec3, Block)>>;
pub struct BuriedTreasure {
    seed: u64,
    cache: Mutex<FxHashMap<IVec2, Build>>,
}
impl BuriedTreasure {
    pub fn new(seed: u64) -> Self {
        Self { seed, cache: Mutex::new(FxHashMap::default()) }
    }

    /// AbstractSpreadingStructurePlacement.legacyArbitrarySaltProbabilityReducer.
    fn candidate(&self, chunk: IVec2) -> bool {
        let seed = self
            .seed
            .wrapping_add((chunk.x as i64).wrapping_mul(341_873_128_712) as u64)
            .wrapping_add((chunk.y as i64).wrapping_mul(132_897_987_541) as u64)
            .wrapping_add(10_387_320);
        JavaRandom::new(seed as i64).next_float() < 0.01
    }

    fn valid(&self, g: &Generator, chunk: IVec2) -> bool {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld || !self.candidate(chunk) {
            return false;
        }
        // onTopOfChunkCenter(OCEAN_FLOOR_WG): Java checks the biome at +8,+8.
        let p = chunk * 16 + IVec2::splat(8);
        let floor = g.column(p.x, p.y).height;
        g.biome_at(IVec3::new(p.x, floor, p.y)).is_beach()
    }

    /// Java findNearestMapStructure's first valid start on the first occupied
    /// random-spread ring, x outer / z inner. Locate uses the saved (9,0,9)
    /// placement offset, not the eventual chest's depth. No generated chunks read.
    pub fn nearest_buried_treasure(&self, g: &Generator, from: IVec3, max_rings: i32) -> Option<IVec3> {
        if g.generation < Generation::V3 || g.dimension != Dimension::Overworld {
            return None;
        }
        let home = IVec2::new(from.x, from.z).div_euclid(IVec2::splat(16));
        for ring in 0..=max_rings {
            for x in -ring..=ring {
                for z in -ring..=ring {
                    if x.abs() != ring && z.abs() != ring {
                        continue;
                    }
                    let c = home + IVec2::new(x, z);
                    if self.valid(g, c) {
                        return Some(IVec3::new(c.x * 16 + 9, 0, c.y * 16 + 9));
                    }
                }
            }
        }
        None
    }

    /// The chest and six neighbour fills of BuriedTreasurePiece.postProcess.
    pub fn blocks(&self, g: &Generator, chunk: IVec2) -> Vec<(IVec3, Block)> {
        if !self.valid(g, chunk) {
            return Vec::new();
        }
        if let Some(built) = self.cache.lock().unwrap().get(&chunk) {
            return built.as_ref().clone();
        }
        let built = self.build(g, chunk);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 256
            && let Some(key) = cache.keys().next().copied()
        {
            cache.remove(&key);
        }
        cache.entry(chunk).or_insert_with(|| Arc::new(built)).as_ref().clone()
    }

    fn build(&self, g: &Generator, chunk: IVec2) -> Vec<(IVec3, Block)> {
        // This snapshot excludes treasure itself, so it cannot recurse into
        // this layout cache. All lateral neighbours remain in this Java cell.
        let terrain = std::cell::RefCell::new(FxHashMap::default());
        let block = |p: IVec3| {
            let c = chunk_of(p);
            let mut terrain = terrain.borrow_mut();
            let data = terrain.entry(c).or_insert_with(|| g.treasure_terrain(c));
            let l = p - c * CHUNK_SIZE_I;
            data.get(l.x as usize, l.y as usize, l.z as usize)
        };
        let x = chunk.x * 16 + 9;
        let z = chunk.y * 16 + 9;
        let col = g.structure_column(x, z);
        for y in (col.min_y + 1..=col.top(true) + 1).rev() {
            let below = block(IVec3::new(x, y - 1, z));
            if !matches!(below, Block::SANDSTONE | Block::STONE | Block::ANDESITE | Block::GRANITE | Block::DIORITE) {
                continue;
            }
            let at = IVec3::new(x, y, z);
            let current = block(at);
            let soft = if open(current) { Block::SAND } else { current };
            let mut out = Vec::with_capacity(7);
            for d in [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X] {
                let p = at + d;
                if open(block(p)) {
                    let fill = if d != IVec3::Y && open(block(p - IVec3::Y)) { below } else { soft };
                    out.push((p, fill));
                }
            }
            // StructurePiece.reorient: a unique solid side faces away; an
            // enclosed chest follows Java's north/south/west/east fallback.
            let neighbour = |f: Facing| {
                let p = at + f.offset();
                out.iter().find(|(q, _)| *q == p).map_or_else(|| block(p), |(_, b)| *b)
            };
            let chest_beside = Facing::ALL.iter().any(|&f| neighbour(f).base() == Block::CHEST);
            let solid = Facing::ALL.map(|f| (f, neighbour(f).is_opaque()));
            let walls: Vec<_> = solid.iter().filter(|(_, solid)| *solid).map(|(f, _)| *f).collect();
            let mut facing = if !chest_beside && walls.len() == 1 { walls[0].opposite() } else { Facing::North };
            if !chest_beside && walls.len() != 1 {
                let opaque = |f| solid.iter().any(|&(face, solid)| face == f && solid);
                if opaque(facing) {
                    facing = facing.opposite();
                }
                if opaque(facing) {
                    facing = match facing {
                        Facing::North => Facing::East,
                        Facing::East => Facing::South,
                        Facing::South => Facing::West,
                        Facing::West => Facing::North,
                    };
                }
                if opaque(facing) {
                    facing = facing.opposite();
                }
            }
            out.push((at, Block::CHEST.with_facing(facing)));
            return out;
        }
        Vec::new()
    }

    pub fn paint(&self, g: &Generator, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
        // The chest is at +9,+9 and its neighbours never leave that 16-block
        // cell. A VoxelCraft chunk holds two Java cells on each horizontal axis.
        for x in 0..2 {
            for z in 0..2 {
                for (p, b) in self.blocks(g, IVec2::new(base.x / 16 + x, base.z / 16 + z)) {
                    let l = p - base;
                    if l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(CHUNK_SIZE_I)).all() {
                        blocks[index(l.x as usize, l.y as usize, l.z as usize)] = b;
                    }
                }
            }
        }
    }

    pub fn features(&self, g: &Generator, cpos: IVec3) -> Vec<(IVec3, Feature)> {
        let base = cpos * CHUNK_SIZE_I;
        let mut out = Vec::new();
        for x in 0..2 {
            for z in 0..2 {
                // The final block is the treasure chest; neighbour fills can
                // copy a chest-shaped soft block but do not receive loot.
                if let Some(&(p, b)) = self.blocks(g, IVec2::new(base.x / 16 + x, base.z / 16 + z)).last()
                    && b.base() == Block::CHEST
                    && chunk_of(p) == cpos
                {
                    out.push((
                        p,
                        Feature::TempleChest(super::noise::hash3(p.x, p.y, p.z, self.seed), LootTable::BuriedTreasure),
                    ));
                }
            }
        }
        out
    }
}
fn open(b: Block) -> bool {
    b == Block::AIR || b.is_lava() || (b.is_water() && !super::bubble::is_column(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_type_two_matches_independent_java_lcg() {
        for (seed, want) in [
            (0, vec![(-28, -22), (-27, -9), (-25, -20), (-25, 21), (-23, -4), (-17, -10), (-17, 19), (-15, 7)]),
            (42, vec![(-28, -22), (-27, -9), (-27, 9), (-25, -20), (-25, 21), (-23, -1), (-20, -21), (-17, -10)]),
            (12345, vec![(-30, 20), (-27, 21), (-26, -27), (-24, -3), (-24, 29), (-23, -26), (-23, 29), (-21, 20)]),
        ] {
            let t = BuriedTreasure::new(seed);
            let actual: Vec<_> = (-30..=30)
                .flat_map(|x| (-30..=30).map(move |z| (x, z)))
                .filter(|&(x, z)| t.candidate(IVec2::new(x, z)))
                .take(8)
                .collect();
            assert_eq!(actual, want);
        }
        let t = BuriedTreasure::new(42);
        let count =
            (-300..300).flat_map(|x| (-300..300).map(move |z| IVec2::new(x, z))).filter(|&p| t.candidate(p)).count();
        assert!((3300..3900).contains(&count), "{count}");
    }
    #[test]
    fn buried_chests_are_below_beaches_on_java_stone_and_have_loot() {
        for seed in [0, 1, 42, 12345] {
            let g = Generator::new(seed);
            let locate = g.buried_treasure.nearest_buried_treasure(&g, IVec3::ZERO, 400).unwrap();
            assert_eq!((locate.x.rem_euclid(16), locate.y, locate.z.rem_euclid(16)), (9, 0, 9));
            let chunk = IVec2::new(locate.x.div_euclid(16), locate.z.div_euclid(16));
            let blocks = g.buried_treasure.blocks(&g, chunk);
            let (at, b) = *blocks.last().unwrap();
            assert_eq!(b.base(), Block::CHEST);
            let col = g.structure_column(at.x, at.z);
            assert!(matches!(
                col.get(at.y - 1),
                Block::STONE | Block::SANDSTONE | Block::ANDESITE | Block::GRANITE | Block::DIORITE
            ));
            assert!(at.y <= col.top(true));
            assert!(g.column(chunk.x * 16 + 8, chunk.y * 16 + 8).biome.is_beach());
            let c = chunk_of(at);
            let data = g.generate(c);
            let l = at - c * CHUNK_SIZE_I;
            assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize).base(), Block::CHEST);
            let below = at - IVec3::Y;
            let below_chunk = chunk_of(below);
            let l = below - below_chunk * CHUNK_SIZE_I;
            let support = g.generate(below_chunk).get(l.x as usize, l.y as usize, l.z as usize);
            assert!(
                matches!(support, Block::STONE | Block::SANDSTONE | Block::ANDESITE | Block::GRANITE | Block::DIORITE),
                "seed {seed} chest {at} rests on {support:?}"
            );
            let features = g.buried_treasure.features(&g, c);
            let (_, Feature::TempleChest(loot_seed, LootTable::BuriedTreasure)) =
                features.iter().find(|(p, _)| *p == at).unwrap()
            else {
                panic!("missing chest")
            };
            let loot = super::super::temples::loot(*loot_seed, LootTable::BuriedTreasure);
            assert!(loot.slots.iter().flatten().any(|s| s.item == crate::item::Item::HEART_OF_THE_SEA));
            let fresh = Generator::new(seed);
            assert_eq!(blocks, fresh.buried_treasure.blocks(&fresh, chunk));
            eprintln!("seed {seed}: treasure {at}, locate {locate}");
        }
    }
    #[test]
    fn ring_search_uses_java_iteration_order() {
        let g = Generator::new(42);
        let from = IVec3::new(-300, 80, 500);
        let actual = g.buried_treasure.nearest_buried_treasure(&g, from, 400).unwrap();
        let home = IVec2::new(from.x.div_euclid(16), from.z.div_euclid(16));
        let got = IVec2::new(actual.x.div_euclid(16), actual.z.div_euclid(16));
        let d = got - home;
        let ring = d.x.abs().max(d.y.abs());
        assert!(g.buried_treasure.nearest_buried_treasure(&g, from, ring - 1).is_none());
        let mut starts = Vec::new();
        for x in -ring..=ring {
            for z in -ring..=ring {
                let p = home + IVec2::new(x, z);
                if (x.abs() == ring || z.abs() == ring) && g.buried_treasure.valid(&g, p) {
                    starts.push(p);
                }
            }
        }
        assert_eq!(starts[0], got);
    }
}
