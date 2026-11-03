//! Original generation 3 bastions. Legacy geometry and saved loot stay intact.
use super::*;
use crate::entity::{MobKind, nether::Weapon};
use crate::world::jigsaw::{self, Marker, Palette};

const SETTINGS: jigsaw::Settings = jigsaw::Settings { start_pool: "bastion/starts", max_depth: 6, max_distance: 80 };

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resident {
    pub at: IVec3,
    pub kind: MobKind,
    pub weapon: Weapon,
}

struct Degradation;
impl Palette for Degradation {
    fn role(&self, _role: &str) -> Option<Block> {
        None
    }
    fn clear_above(&self) -> i32 {
        0
    }
    fn process(&self, block: Block, at: IVec3, _piece: &jigsaw::Placed) -> Option<Block> {
        if !matches!(block, BRICKS | BLACK | Block::GOLD_BLOCK | Block::GILDED_BLACKSTONE) {
            return Some(block);
        }
        // RuleProcessor seeds random per world position, independently of paint
        // order. Each predicate takes a separate draw, in the JSON rule order.
        let seed =
            i64::from(at.x.wrapping_mul(3_129_871)) ^ i64::from(at.z).wrapping_mul(116_129_781) ^ i64::from(at.y);
        let seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861).wrapping_add(seed.wrapping_mul(11)) >> 16;
        let mut rng = crate::enchant::JavaRandom::new(seed);
        for (input, probability, output) in [
            (BRICKS, 0.3, Block::CRACKED_POLISHED_BLACKSTONE_BRICKS),
            (BLACK, 0.0001, AIR),
            (Block::GOLD_BLOCK, 0.3, Block::CRACKED_POLISHED_BLACKSTONE_BRICKS),
            (Block::GILDED_BLACKSTONE, 0.5, BLACK),
            (BLACK, 0.01, Block::GILDED_BLACKSTONE),
        ] {
            if block == input && rng.next_float() < probability {
                return Some(output);
            }
        }
        Some(block)
    }
}

fn resident(lib: &jigsaw::Library, at: IVec3, pool: &str, seed: u64) -> Option<Resident> {
    let pool = &lib.pools[pool];
    let mut roll = Rng(seed).below(pool.elements.iter().map(|e| e.1).sum());
    let template = pool.elements.iter().find_map(|&(template, weight)| {
        if roll < weight {
            Some(template)
        } else {
            roll -= weight;
            None
        }
    })??;
    let name = lib.templates[template as usize].mobs.first()?.1.as_str();
    let (kind, weapon) = match name {
        "piglin_brute" => (MobKind::PiglinBrute, Weapon::GoldenAxe),
        "piglin_sword" => (MobKind::Piglin, Weapon::GoldenSword),
        "piglin_crossbow" => (MobKind::Piglin, Weapon::Crossbow),
        "hoglin" => (MobKind::Hoglin, Weapon::None),
        _ => panic!("unsupported bastion resident {name}"),
    };
    Some(Resident { at, kind, weapon })
}

pub(super) fn generate(seed: u64, start: IVec2) -> Bastion {
    let lib = jigsaw::bastions();
    let placed = jigsaw::assemble(lib, &SETTINGS, start, &|_, _| 33, seed);
    let kind = match lib.templates[placed[0].template as usize].name.as_str() {
        "bastion/units/air_base" => Kind::Housing,
        "bastion/hoglin_stable/air_base" => Kind::Stables,
        "bastion/treasure/big_air_full" => Kind::Treasure,
        "bastion/bridge/starting_pieces/entrance_base" => Kind::Bridge,
        _ => unreachable!("bastion start pool"),
    };
    let bounds = placed.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b)).expect("bastion start");
    let residents = jigsaw::markers(lib, &placed, &|_, _| 33)
        .into_iter()
        .filter_map(|m| match m {
            Marker::Mob(at, pool) => resident(lib, at, pool, hash3(at.x, at.y, at.z, seed)),
            _ => None,
        })
        .collect();
    Bastion { kind, bounds, pieces: Vec::new(), placed, template_residents: Some(residents) }
}

pub(super) fn paint(b: &Bastion, blocks: &mut [Block], base: IVec3, top: IVec3) {
    jigsaw::paint_canvas(
        jigsaw::bastions(),
        &b.placed,
        &Degradation,
        &jigsaw::Ground { surface: &|_, _| 33, solid: &|_, _| 33 },
        &mut jigsaw::Canvas { blocks, base, top },
    );
}

pub(super) fn features(b: &Bastion) -> impl Iterator<Item = (IVec3, Feature)> + '_ {
    jigsaw::markers(jigsaw::bastions(), &b.placed, &|_, _| 33).into_iter().filter_map(|m| match m {
        Marker::Loot(at, name, seed) => {
            let kind = match name {
                "bastion_treasure" => ChestKind::Treasure,
                "bastion_bridge" => ChestKind::Bridge,
                "bastion_hoglin_stable" => ChestKind::HoglinStable,
                "bastion_other" => ChestKind::Other,
                _ => panic!("unsupported bastion loot {name}"),
            };
            Some((at, Feature::BastionChest(seed, kind)))
        }
        Marker::Spawner(at, "magma_cube") => Some((at, Feature::Spawner(MobKind::MagmaCube))),
        Marker::Spawner(_, name) => panic!("unsupported bastion spawner {name}"),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        World,
        chunk::{chunk_of, index},
        terrain::{Dimension, Generation, Generator},
    };

    /// Fixture produced with java.util.Random and Java 26.3 Mth.getSeed /
    /// RandomBlockMatchTest over 20,480 block-position pairs.
    #[test]
    fn degradation_matches_java_position_random_and_rule_order() {
        let piece = jigsaw::single(jigsaw::bastions(), "bastion/units/air_base", IVec3::ZERO, 0, 0);
        let mut hash = 0xcbf29ce484222325u64;
        let mut counts = [0; 5];
        for block in [BRICKS, BLACK, Block::GOLD_BLOCK, Block::GILDED_BLACKSTONE, Block::BASALT] {
            for x in -32..32 {
                for z in -32..32 {
                    let result = Degradation.process(block, IVec3::new(x, 33, z), &piece).unwrap();
                    let code = if result == block {
                        0
                    } else if result == Block::CRACKED_POLISHED_BLACKSTONE_BRICKS {
                        1
                    } else if result == AIR {
                        2
                    } else if result == BLACK {
                        3
                    } else if result == Block::GILDED_BLACKSTONE {
                        4
                    } else {
                        panic!("unexpected processor output")
                    };
                    counts[code] += 1;
                    hash = (hash ^ code as u64).wrapping_mul(0x100000001b3);
                }
            }
        }
        assert_eq!(hash, 0xebdf26e37d2d287e);
        assert_eq!(counts, [16010, 2410, 0, 2031, 29]);
    }

    #[test]
    fn mob_pools_keep_the_java_26_3_weights() {
        let lib = jigsaw::bastions();
        for (pool, expected) in [
            (
                "bastion/mobs/piglin",
                vec![("melee_piglin", 1), ("sword_piglin", 4), ("crossbow_piglin", 4), ("empty", 1)],
            ),
            ("bastion/mobs/hoglin", vec![("hoglin", 2), ("empty", 1)]),
            ("bastion/mobs/piglin_melee", vec![("melee_piglin_always", 1), ("melee_piglin", 5), ("sword_piglin", 1)]),
        ] {
            let got: Vec<_> = lib.pools[pool]
                .elements
                .iter()
                .map(|&(t, w)| (lib.templates[t.unwrap() as usize].name.rsplit('/').next().unwrap(), w))
                .collect();
            assert_eq!(got, expected);
        }
        assert_eq!(lib.pools["bastion/starts"].elements.iter().map(|e| e.1).collect::<Vec<_>>(), vec![1, 1, 1, 1]);
    }

    #[test]
    fn all_four_starts_are_deterministic_with_nonoverlapping_walkable_pieces() {
        let mut kinds = [0; 4];
        let mut hoglins = 0;
        for seed in 0..120 {
            let a = generate(seed, IVec2::new(-29, 31));
            let b = generate(seed, IVec2::new(-29, 31));
            assert_eq!(format!("{:?}", a.placed), format!("{:?}", b.placed));
            assert_eq!(a.template_residents, b.template_residents);
            kinds[a.kind as usize] += 1;
            assert!(a.placed.len() >= 5, "{:?}: {}", a.kind, a.placed.len());
            for (i, p) in a.placed.iter().enumerate() {
                for q in &a.placed[..i] {
                    assert!(!p.bounds.intersects(&q.bounds));
                }
            }
            for r in a.template_residents.as_ref().unwrap() {
                hoglins += usize::from(r.kind == MobKind::Hoglin);
                let base = chunk_of(r.at - IVec3::Y) * 32;
                let mut blocks = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                paint(&a, &mut *blocks, base, base + IVec3::splat(31));
                let l = r.at - IVec3::Y - base;
                assert!(blocks[index(l.x as usize, l.y as usize, l.z as usize)].is_solid(), "{:?} {r:?}", a.kind);
                for dy in [0, 1] {
                    let at = r.at + IVec3::Y * dy;
                    let base = chunk_of(at) * 32;
                    let mut blocks = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                    paint(&a, &mut *blocks, base, base + IVec3::splat(31));
                    let l = at - base;
                    assert_eq!(blocks[index(l.x as usize, l.y as usize, l.z as usize)], AIR, "{:?} {r:?}", a.kind);
                }
            }
        }
        assert!(kinds.iter().all(|&n| n > 10), "{kinds:?}");
        assert!(hoglins > 50);
    }

    #[test]
    fn columns_and_shifted_chunks_paint_identically_and_markers_match_blocks() {
        let mut kinds = [false; 4];
        for seed in 0..40 {
            let b = generate(seed, IVec2::new(29, 29));
            if kinds[b.kind as usize] {
                continue;
            }
            kinds[b.kind as usize] = true;
            let lo = chunk_of(b.bounds.min);
            let hi = chunk_of(b.bounds.max);
            for cx in lo.x..=hi.x {
                for cz in lo.z..=hi.z {
                    for cy in lo.y..=hi.y {
                        let base = IVec3::new(cx, cy, cz) * 32;
                        let draw = |base| {
                            let mut a = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                            paint(&b, &mut *a, base, base + IVec3::splat(31));
                            a
                        };
                        let a = draw(base);
                        let shifted = draw(base + IVec3::ONE);
                        for x in 1..32 {
                            for z in 1..32 {
                                for y in 1..32 {
                                    assert_eq!(a[index(x, y, z)], shifted[index(x - 1, y - 1, z - 1)]);
                                }
                            }
                        }
                        for x in [0, 7, 15, 24, 31] {
                            for z in [0, 7, 15, 24, 31] {
                                let p = base + IVec3::new(x, 0, z);
                                let mut column = [Block::NETHERRACK; CHUNK_SIZE];
                                paint(&b, &mut column, p, p + IVec3::Y * 31);
                                for y in 0..32 {
                                    assert_eq!(column[y], a[index(x as usize, y, z as usize)]);
                                }
                            }
                        }
                    }
                }
            }
            let mut spawners = 0;
            for (p, f) in features(&b) {
                let base = chunk_of(p) * 32;
                let mut a = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                paint(&b, &mut *a, base, base + IVec3::splat(31));
                let l = p - base;
                let block = a[index(l.x as usize, l.y as usize, l.z as usize)].base();
                match f {
                    Feature::BastionChest(..) => assert_eq!(block, Block::CHEST),
                    Feature::Spawner(MobKind::MagmaCube) => {
                        spawners += 1;
                        assert_eq!(block, Block::SPAWNER);
                    }
                    _ => unreachable!(),
                }
            }
            assert_eq!(spawners, usize::from(b.kind == Kind::Treasure));
        }
        assert!(kinds.into_iter().all(|v| v));
    }

    #[test]
    fn public_column_and_chunk_paths_agree_for_every_start_kind() {
        let mut checked = [false; 4];
        for seed in 0..100 {
            let bastions = Bastions::new(seed);
            let Some(b) = bastions.get(IVec2::ZERO) else { continue };
            if checked[b.kind as usize] {
                continue;
            }
            checked[b.kind as usize] = true;
            let lo = chunk_of(b.bounds.min);
            let hi = chunk_of(b.bounds.max);
            for x in lo.x..=hi.x {
                for z in lo.z..=hi.z {
                    for y in lo.y..=hi.y {
                        let base = IVec3::new(x, y, z) * 32;
                        let mut blocks = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                        bastions.paint(&mut blocks, base);
                        for x in [0, 15, 31] {
                            for z in [0, 15, 31] {
                                let col =
                                    bastions.column_at(base + IVec3::new(x, 0, z), [Block::NETHERRACK; CHUNK_SIZE]);
                                for (y, &block) in col.iter().enumerate() {
                                    assert_eq!(block, blocks[index(x as usize, y, z as usize)]);
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(checked.into_iter().all(|v| v));
    }

    #[test]
    fn entrances_bridges_stairs_and_all_chests_have_a_walkable_route() {
        use rustc_hash::FxHashSet;
        use std::collections::VecDeque;
        let mut checked = [false; 4];
        for seed in 0..40 {
            let b = generate(seed, IVec2::new(29, 29));
            if checked[b.kind as usize] {
                continue;
            }
            checked[b.kind as usize] = true;
            let lo = chunk_of(b.bounds.min);
            let hi = chunk_of(b.bounds.max);
            let mut chunks = FxHashMap::default();
            for x in lo.x..=hi.x {
                for z in lo.z..=hi.z {
                    for y in lo.y..=hi.y {
                        let c = IVec3::new(x, y, z);
                        let base = c * 32;
                        let mut blocks = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                        paint(&b, &mut *blocks, base, base + IVec3::splat(31));
                        chunks.insert(c, blocks);
                    }
                }
            }
            let block = |p: IVec3| {
                let l = p.rem_euclid(IVec3::splat(32));
                chunks
                    .get(&chunk_of(p))
                    .map_or(Block::NETHERRACK, |c| c[index(l.x as usize, l.y as usize, l.z as usize)])
            };
            if b.kind == Kind::Treasure {
                let room = b.placed[0].bounds;
                let mut lava = 0;
                for x in room.min.x..=room.max.x {
                    for z in room.min.z..=room.max.z {
                        for y in room.min.y..=room.max.y {
                            let at = IVec3::new(x, y, z);
                            if !block(at).is_lava() {
                                continue;
                            }
                            lava += 1;
                            for d in [IVec3::X, -IVec3::X, IVec3::Z, -IVec3::Z, -IVec3::Y] {
                                assert!(
                                    block(at + d).is_solid() || block(at + d).is_lava(),
                                    "treasure lava can escape from {at} toward {d}"
                                );
                            }
                        }
                    }
                }
                assert!(lava > 100);
            }
            let can_stand = |p| {
                block(p) == AIR
                    && block(p + IVec3::Y) == AIR
                    && block(p - IVec3::Y).is_solid()
                    && block(p - IVec3::Y) != Block::MAGMA
            };
            let start = b.placed[0].world(jigsaw::bastions().templates[b.placed[0].template as usize].jigsaws[0].pos);
            assert!(can_stand(start));
            let mut reached = FxHashSet::default();
            reached.insert(start);
            let mut queue = VecDeque::from([start]);
            while let Some(p) = queue.pop_front() {
                for d in [IVec3::X, -IVec3::X, IVec3::Z, -IVec3::Z] {
                    for dy in -1..=1 {
                        let to = p + d + IVec3::Y * dy;
                        // A player can walk or jump a one-block rise with headroom.
                        if can_stand(to) && block(p + IVec3::Y * (1 + dy.max(0))) == AIR && reached.insert(to) {
                            queue.push_back(to);
                        }
                    }
                }
            }
            for (at, f) in features(&b) {
                if matches!(f, Feature::BastionChest(..)) {
                    assert!(
                        reached.iter().any(|p| (p.x - at.x).abs() + (p.z - at.z).abs() <= 1 && (p.y - at.y).abs() <= 1),
                        "{:?} seed{seed} unreachable chest {at}",
                        b.kind
                    );
                }
            }
        }
        assert!(checked.into_iter().all(|v| v));
    }

    #[test]
    fn generated_treasure_registers_spawner_and_saved_empty_chests() {
        let seed =
            (0..100).find(|&s| Bastions::new(s).get(IVec2::ZERO).is_some_and(|b| b.kind == Kind::Treasure)).unwrap();
        let generator = Arc::new(Generator::with_generation(seed, Dimension::Nether, Generation::V3));
        let b = Bastions::new(seed).get(IVec2::ZERO).unwrap();
        let mut world = World::new_headless(generator.clone(), Default::default(), 2);
        let all: Vec<_> = features(&b).collect();
        for &(at, _) in &all {
            let c = chunk_of(at);
            let data = generator.generate(c);
            world.register_structure_features(c, &data);
        }
        let chest = all.iter().find(|(_, f)| matches!(f, Feature::BastionChest(_, ChestKind::Treasure))).unwrap().0;
        assert!(world.chests[&chest].slots.iter().flatten().any(|s| s.item == Item::NETHERITE_UPGRADE));
        let spawner = all.iter().find(|(_, f)| matches!(f, Feature::Spawner(MobKind::MagmaCube))).unwrap().0;
        assert_eq!(world.spawners.get(&spawner), Some(&MobKind::MagmaCube));
        world.chests.insert(chest, Chest::default());
        let saved = world.chests_to_string();
        let mut loaded = World::new_headless(generator.clone(), Default::default(), 2);
        loaded.load_chests(&saved);
        let c = chunk_of(chest);
        loaded.register_structure_features(c, &generator.generate(c));
        assert!(loaded.chests[&chest].slots.iter().all(Option::is_none));
        eprintln!("generation 3 treasure seed{seed}, bounds {:?}", b.bounds);
    }
}
