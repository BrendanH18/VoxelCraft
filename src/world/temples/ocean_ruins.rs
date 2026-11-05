//! Generation 3 ocean ruins on original text templates. Legacy layouts stay pinned.
use super::*;
use crate::world::{
    archaeology::{self, Table},
    jigsaw::{self, Palette},
};
use std::sync::OnceLock;
fn library() -> &'static jigsaw::Library {
    static LIB: OnceLock<jigsaw::Library> = OnceLock::new();
    LIB.get_or_init(|| jigsaw::Library::parse(include_str!("../jigsaw/ocean_ruins.txt")).expect("original ocean ruins"))
}
struct Materials {
    warm: bool,
    material: Block,
    integrity: f32,
}
impl Palette for Materials {
    fn role(&self, r: &str) -> Option<Block> {
        match r {
            "masonry" => Some(self.material),
            "sediment" => Some(if self.warm { Block::SAND } else { Block::GRAVEL }),
            _ => None,
        }
    }
    fn process(&self, b: Block, at: IVec3, _: &jigsaw::Placed) -> Option<Block> {
        (jigsaw::processor_random(at).next_float() <= self.integrity).then_some(b)
    }
    fn clear_above(&self) -> i32 {
        0
    }
}
fn long(r: &mut JavaRandom) -> u64 {
    ((r.next_int() as i64) << 32).wrapping_add(r.next_int() as i64) as u64
}
pub(super) fn structure_seed(g: &Generator, centre: IVec2) -> u64 {
    let mut r = JavaRandom::new(g.seed as i64);
    let (a, b) = (long(&mut r), long(&mut r));
    (centre.x.div_euclid(16) as u64).wrapping_mul(a) ^ (centre.y.div_euclid(16) as u64).wrapping_mul(b) ^ g.seed
}
fn position_seed(at: IVec3) -> u64 {
    let s = i64::from(at.x.wrapping_mul(3_129_871)) ^ i64::from(at.z).wrapping_mul(116_129_781) ^ i64::from(at.y);
    (s.wrapping_mul(s).wrapping_mul(42_317_861).wrapping_add(s.wrapping_mul(11)) >> 16) as u64
}
fn range(r: &mut JavaRandom, lo: i32, hi: i32) -> i32 {
    lo + r.next_bounded(hi - lo + 1)
}
pub(super) fn warm(b: Biome) -> bool {
    matches!(b, Biome::WarmOcean | Biome::LukewarmOcean | Biome::DeepLukewarmOcean)
}
/// Complete layouts are baked before clipping; caps never depend on a chunk's
/// generation order, and surviving loot blocks retain their positional seeds.
pub(super) fn build(g: &Generator, centre: IVec2, seed: u64) -> Built {
    let mut r = JavaRandom::new(seed as i64);
    let warm = warm(g.column(centre.x, centre.y).biome);
    let rot = r.next_bounded(4) as u8;
    let big = r.next_float() <= 0.3;
    let mut result = Builder::new(IVec3::ZERO, IVec2::ONE, 0);
    let at = IVec3::new(centre.x - 8, 0, centre.y - 8);
    piece(g, &mut result, at, rot, big, warm, &mut r);
    if big && r.next_float() <= 0.9 {
        let rotated = jigsaw::rotate(IVec3::new(15, 0, 15), rot);
        let main = Bounds { min: at.min(at + rotated), max: at.max(at + rotated) };
        let home = main.min;
        let mut positions = vec![
            home + IVec3::new(-16 + range(&mut r, 1, 8), 0, 16 + range(&mut r, 1, 7)),
            home + IVec3::new(-16 + range(&mut r, 1, 8), 0, range(&mut r, 1, 7)),
            home + IVec3::new(-16 + range(&mut r, 1, 8), 0, -16 + range(&mut r, 4, 8)),
            home + IVec3::new(range(&mut r, 1, 7), 0, 16 + range(&mut r, 1, 7)),
            home + IVec3::new(range(&mut r, 1, 7), 0, -16 + range(&mut r, 4, 6)),
            home + IVec3::new(16 + range(&mut r, 1, 7), 0, 16 + range(&mut r, 3, 8)),
            home + IVec3::new(16 + range(&mut r, 1, 7), 0, range(&mut r, 1, 7)),
            home + IVec3::new(16 + range(&mut r, 1, 7), 0, -16 + range(&mut r, 4, 8)),
        ];
        for _ in 0..range(&mut r, 4, 8) {
            let i = r.next_bounded(positions.len() as i32) as usize;
            let p = positions.remove(i);
            let rot = r.next_bounded(4) as u8;
            let end = p + jigsaw::rotate(IVec3::new(5, 0, 6), rot);
            if !main.intersects(&Bounds { min: p.min(end), max: p.max(end) }) {
                piece(g, &mut result, p, rot, false, warm, &mut r);
            }
        }
    }
    result.finish(Kind::OceanRuin, 6)
}
/// Java's footprint lowering rule, sampling pristine terrain rather than
/// previously painted structures or partially generated neighbouring chunks.
fn height(g: &Generator, placed: &jigsaw::Placed) -> i32 {
    let initial = g.column(placed.origin.x, placed.origin.z).height + 1;
    let mut min = 512;
    let mut deep = 0;
    let (mut columns, mut nodes) = (FxHashMap::default(), FxHashMap::default());
    for z in placed.bounds.min.z..=placed.bounds.max.z {
        for x in placed.bounds.min.x..=placed.bounds.max.x {
            let mut y = initial - 1;
            while y > super::super::chunk::WORLD_MIN_Y + 1 {
                let b = g.natural_block(IVec3::new(x, y, z), &mut columns, &mut nodes);
                if b != Block::AIR
                    && !b.is_water()
                    && b != Block::ICE
                    && b != super::super::gadgets::PACKED_ICE
                    && b != ob::BLUE_ICE
                {
                    break;
                }
                y -= 1;
            }
            min = min.min(y);
            deep += i32::from(y < initial - 3);
        }
    }
    let width = placed.bounds.max.x - placed.bounds.min.x;
    if initial - 1 - min > 2 && deep > width - 2 { min + 1 } else { initial }
}
fn piece(g: &Generator, out: &mut Builder, at: IVec3, rot: u8, big: bool, warm: bool, r: &mut JavaRandom) {
    let name = format!("ocean/{}/{}", if big { "big" } else { "small" }, r.next_bounded(if big { 4 } else { 8 }));
    let lib = library();
    let mut p = jigsaw::single(lib, &name, at, rot, 0);
    let y = height(g, &p);
    p = jigsaw::single(lib, &name, IVec3::new(at.x, y, at.z), rot, p.seed);
    let integrity = if big { 0.9 } else { 0.8 };
    let mut material =
        Materials { warm, material: if warm { Block::SANDSTONE } else { Block::STONE_BRICKS }, integrity };
    let mut cells = super::super::relics::bake(lib, std::slice::from_ref(&p), &material);
    // CappedProcessor shuffles the complete processed piece, changes at most
    // five matching sediment blocks, then attaches the archaeology loot.
    cells.sort_by_key(|(at, _)| {
        let local = jigsaw::rotate(*at - p.origin, (4 - rot) % 4);
        (local.y, local.x, local.z)
    });
    let mut order: Vec<_> = (0..cells.len()).collect();
    let mut world_random = JavaRandom::new(g.seed as i64);
    let mut cap = super::super::loot::LootRandom::new(long(&mut world_random) ^ position_seed(p.origin));
    super::super::loot::shuffle(&mut order, &mut cap);
    let soil = if warm { Block::SAND } else { Block::GRAVEL };
    let mut count = 0;
    for i in order {
        if cells[i].1 == soil {
            cells[i].1 = archaeology::state(!warm, 0);
            let pos = cells[i].0;
            out.features.push((
                pos,
                Feature::Archaeology(
                    long(&mut jigsaw::processor_random(pos)),
                    if warm { Table::Warm } else { Table::Cold },
                ),
            ));
            count += 1;
            if count == 5 {
                break;
            }
        }
    }
    for (at, b) in cells {
        out.blocks.insert(at, b);
    }
    if !warm {
        // Original cracked and mossy overlays use the same masonry motif;
        // sediment belongs only to the primary template, so it isn't replaced.
        for (block, integrity) in [(Block::CRACKED_STONE_BRICKS, 0.7), (Block::MOSSY_STONE_BRICKS, 0.5)] {
            material.material = block;
            material.integrity = integrity;
            for (at, b) in super::super::relics::bake(lib, std::slice::from_ref(&p), &material) {
                if b == block {
                    out.blocks.insert(at, b);
                }
            }
        }
    }
    let t = &lib.templates[p.template as usize];
    let cx = t.size.x / 2;
    let cz = t.size.z / 2;
    let chest = p.world(IVec3::new(cx, 1, cz));
    out.blocks.insert(
        chest,
        Block::CHEST.with_facing(jigsaw::rotate_facing(Facing::South, rot)).with_water(chest.y < SEA_LEVEL),
    );
    out.features.push((
        chest,
        Feature::TempleChest(
            hash3(chest.x, chest.y, chest.z, g.seed),
            if big { LootTable::RuinBig } else { LootTable::RuinSmall },
        ),
    ));
    let resident = p.world(IVec3::new(cx - 1, 1, cz));
    out.blocks.insert(resident, if resident.y < SEA_LEVEL { Block::WATER } else { Block::AIR });
    out.blocks.insert(resident + IVec3::Y, if resident.y + 1 < SEA_LEVEL { Block::WATER } else { Block::AIR });
    out.residents.push((crate::entity::MobKind::Drowned, resident));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{World, chunk::chunk_of, terrain::Generation};
    #[test]
    fn variants_caps_chests_residents_and_rebuilds_are_deterministic() {
        let g = Generator::with_generation(12345, Dimension::Overworld, Generation::V3);
        let at = g.temples.nearest(&g, Kind::OceanRuin, IVec2::ZERO, 6400).unwrap();
        let centre = IVec2::new(at.x, at.z);
        let mut small = false;
        let mut cluster = false;
        for seed in 0..24 {
            let a = build(&g, centre, seed);
            let b = build(&g, centre, seed);
            assert_eq!(a.blocks, b.blocks);
            assert_eq!(a.features, b.features);
            let chests = a.features.iter().filter(|(_, f)| matches!(f, Feature::TempleChest(..))).count();
            small |= chests == 1;
            cluster |= chests >= 4;
            let archaeology = a.features.iter().filter(|(_, f)| matches!(f, Feature::Archaeology(..))).count();
            assert!(archaeology > 0 && archaeology <= chests * 5);
            for (kind, p) in &a.residents {
                assert_eq!(*kind, crate::entity::MobKind::Drowned);
                assert!(a.blocks.iter().find(|(at, _)| at == p).is_some_and(|(_, b)| !b.is_solid()));
            }
        }
        assert!(small && cluster);
        let data = g.generate(chunk_of(at));
        assert_ne!(data.uniform(), Some(Block::AIR));
    }
    #[test]
    fn generated_chests_stay_looted_and_sediment_keeps_loot_after_reload() {
        let g = Arc::new(Generator::with_generation(12345, Dimension::Overworld, Generation::V3));
        let at = g.temples.nearest(&g, Kind::OceanRuin, IVec2::ZERO, 6400).unwrap();
        let mut w = World::new_headless(g.clone(), Default::default(), 2);
        let c = chunk_of(at);
        let mut found = false;
        for dz in -2..=2 {
            for dx in -2..=2 {
                let p = c + IVec3::new(dx, 0, dz);
                w.insert_chunk(p, Arc::new(g.generate(p)), false);
                found |= g.structure_features(p).iter().any(|(_, f)| matches!(f, Feature::Archaeology(..)));
            }
        }
        assert!(found);
        let Some((&pos, _)) = w.chests.iter().next() else {
            panic!("ruin chest");
        };
        w.chests.insert(pos, Chest::default());
        let saved = w.chests_to_string();
        let archaeology = w.archaeology_to_string();
        let mut loaded = World::new_headless(g.clone(), Default::default(), 2);
        loaded.load_chests(&saved);
        loaded.load_archaeology(&archaeology);
        let p = chunk_of(pos);
        loaded.insert_chunk(p, Arc::new(g.generate(p)), false);
        assert_eq!(loaded.chests[&pos], Chest::default());
        assert_eq!(loaded.archaeology_to_string(), archaeology);
    }
}
