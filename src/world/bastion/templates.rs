//! Original generation 3 bastions. Legacy geometry and saved loot stay intact.
use super::*;
use crate::entity::{MobKind, nether::Weapon};
use crate::world::jigsaw::{self, Marker, Palette};

pub(crate) const SETTINGS: jigsaw::Settings =
    jigsaw::Settings { start_pool: "bastion/starts", max_depth: 6, max_distance: 80 };

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
        let mut rng = jigsaw::processor_random(at);
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
    Bastion { kind, bounds, pieces: Vec::new(), placed, template_residents: Some(residents), catalogue: false }
}

/// Generation 6's palette: each piece's Java processor list (bastion
/// degradation, the high rampart's falling-off with height), measured from
/// the piece's own position as Java's `RuleProcessor` does.
struct CataloguePalette;

fn catalogue_processors() -> &'static [Option<&'static jigsaw::rules::Compiled>] {
    static LISTS: std::sync::OnceLock<Vec<Option<&'static jigsaw::rules::Compiled>>> = std::sync::OnceLock::new();
    LISTS.get_or_init(|| super::catalogue::library().processor_lists.iter().map(|n| jigsaw::rules::list(n)).collect())
}

impl Palette for CataloguePalette {
    fn role(&self, _role: &str) -> Option<Block> {
        None
    }
    fn clear_above(&self) -> i32 {
        0
    }
    fn process(&self, block: Block, at: IVec3, piece: &jigsaw::Placed) -> Option<Block> {
        self.process_at(block, at, piece, Block::AIR)
    }
    fn process_at(&self, block: Block, at: IVec3, piece: &jigsaw::Placed, existing: Block) -> Option<Block> {
        let t = &super::catalogue::library().templates[piece.template as usize];
        match t.processors.and_then(|i| catalogue_processors()[i as usize]) {
            Some(list) => list.apply_in(block, at, existing, piece.origin),
            None => Some(block),
        }
    }
}

/// A resident of one of Java's `bastion/mobs/*` templates.
fn catalogue_resident(template: &str, at: IVec3) -> Option<Resident> {
    let (kind, weapon) = match template.rsplit('/').next()? {
        "melee_piglin" | "melee_piglin_always" => (MobKind::PiglinBrute, Weapon::GoldenAxe),
        "sword_piglin" => (MobKind::Piglin, Weapon::GoldenSword),
        "crossbow_piglin" => (MobKind::Piglin, Weapon::Crossbow),
        "hoglin" => (MobKind::Hoglin, Weapon::None),
        _ => return None,
    };
    Some(Resident { at, kind, weapon })
}

/// Generation 6: Java's full pool catalogue, assembled by the jigsaw engine
/// with Java's settings (start pool `bastion/starts`, size 6, distance 80,
/// start height 33).
pub(super) fn generate_catalogue(seed: u64, start: IVec2) -> Bastion {
    let lib = super::catalogue::library();
    let placed = jigsaw::assemble(lib, &SETTINGS, start, &|_, _| 33, seed);
    let kind = match lib.templates[placed[0].template as usize].name.as_str() {
        "bastion/units/air_base" => Kind::Housing,
        "bastion/hoglin_stable/air_base" => Kind::Stables,
        "bastion/treasure/big_air_full" => Kind::Treasure,
        "bastion/bridge/starting_pieces/entrance_base" => Kind::Bridge,
        _ => unreachable!("bastion start pool"),
    };
    let bounds = placed.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b)).expect("bastion start");
    let residents = placed
        .iter()
        .flat_map(|p| {
            let t = &lib.templates[p.template as usize];
            t.mobs.iter().filter_map(|(c, _)| catalogue_resident(&t.name, p.world(*c)))
        })
        .collect();
    Bastion { kind, bounds, pieces: Vec::new(), placed, template_residents: Some(residents), catalogue: true }
}

pub(super) fn paint(b: &Bastion, blocks: &mut [Block], base: IVec3, top: IVec3) {
    let (lib, palette): (&jigsaw::Library, &dyn Palette) =
        if b.catalogue { (super::catalogue::library(), &CataloguePalette) } else { (jigsaw::bastions(), &Degradation) };
    jigsaw::paint_canvas(
        lib,
        &b.placed,
        palette,
        &jigsaw::Ground { surface: &|_, _| 33, solid: &|_, _| 33 },
        &mut jigsaw::Canvas { blocks, base, top },
    );
}

pub(super) fn features(b: &Bastion) -> impl Iterator<Item = (IVec3, Feature)> + '_ {
    let lib = if b.catalogue { super::catalogue::library() } else { jigsaw::bastions() };
    jigsaw::markers(lib, &b.placed, &|_, _| 33).into_iter().filter_map(|m| match m {
        Marker::Loot(at, name, seed) => {
            // The catalogue names Java's tables in full (`chests/...`).
            let kind = match name.strip_prefix("chests/").unwrap_or(name) {
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
        let v3 = |s| Bastions::with_generation(s, Generation::V3);
        let seed = (0..100).find(|&s| v3(s).get(IVec2::ZERO).is_some_and(|b| b.kind == Kind::Treasure)).unwrap();
        let generator = Arc::new(Generator::with_generation(seed, Dimension::Nether, Generation::V3));
        let b = v3(seed).get(IVec2::ZERO).unwrap();
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

    /// Generation 6 (v1.0 Track 8, GEN-05g): Java's full pool catalogue
    /// assembles all four kinds, deterministically, into non-overlapping
    /// pieces within Java's 80-block distance, with residents drawn from the
    /// mob pools and the treasure chests, spawner and gold of the catalogue.
    #[test]
    fn generation_6_bastions_assemble_javas_full_catalogue() {
        let lib = super::super::catalogue::library();
        let mut kinds = [0; 4];
        let mut used = std::collections::BTreeSet::new();
        let (mut brutes, mut hoglins, mut crossbows) = (0, 0, 0);
        let start = IVec2::new(-29, 31);
        for seed in 0..80 {
            let a = generate_catalogue(seed, start);
            let b = generate_catalogue(seed, start);
            assert_eq!(format!("{:?}", a.placed), format!("{:?}", b.placed));
            assert_eq!(a.template_residents, b.template_residents);
            kinds[a.kind as usize] += 1;
            assert!(a.placed.len() >= 8, "{:?} seed {seed}: {} pieces", a.kind, a.placed.len());
            for (i, p) in a.placed.iter().enumerate() {
                used.insert(lib.templates[p.template as usize].name.clone());
                // Java's `max_distance_from_center` bounds every piece.
                let far = (p.bounds.min.x - start.x).abs().max((p.bounds.max.x - start.x).abs());
                let far_z = (p.bounds.min.z - start.y).abs().max((p.bounds.max.z - start.y).abs());
                assert!(
                    far.max(far_z) <= SETTINGS.max_distance + 1,
                    "{:?} seed {seed}: piece {far},{far_z} out",
                    a.kind
                );
                // A connector pointing into its own piece places the child in
                // that piece's interior (Java's air bases hold the units), so
                // pieces either nest or keep apart.
                let contains = |a: &Bounds, b: &Bounds| a.min.cmple(b.min).all() && a.max.cmpge(b.max).all();
                for q in &a.placed[..i] {
                    assert!(
                        !p.bounds.intersects(&q.bounds)
                            || contains(&p.bounds, &q.bounds)
                            || contains(&q.bounds, &p.bounds),
                        "{:?} seed {seed}: overlapping pieces {:?} {:?}",
                        a.kind,
                        lib.templates[p.template as usize].name,
                        lib.templates[q.template as usize].name
                    );
                }
            }
            for r in a.template_residents.as_ref().unwrap() {
                brutes += usize::from(r.kind == MobKind::PiglinBrute);
                hoglins += usize::from(r.kind == MobKind::Hoglin);
                crossbows += usize::from(r.weapon == Weapon::Crossbow);
            }
        }
        assert!(kinds.iter().all(|&n| n > 8), "{kinds:?}");
        assert!(
            brutes > 20 && hoglins > 10 && crossbows > 20,
            "{brutes} brutes, {hoglins} hoglins, {crossbows} crossbows"
        );
        // Most of the catalogue appears within 80 bastions.
        assert!(used.len() > 100, "{} of {} templates used", used.len(), lib.templates.len());
    }

    #[test]
    fn generation_6_treasure_keeps_its_spawner_and_saved_empty_chests() {
        let v6 = |s| Bastions::with_generation(s, Generation::V6);
        let seed = (0..200).find(|&s| v6(s).get(IVec2::ZERO).is_some_and(|b| b.kind == Kind::Treasure)).unwrap();
        let generator = Arc::new(Generator::with_generation(seed, Dimension::Nether, Generation::V6));
        let b = v6(seed).get(IVec2::ZERO).unwrap();
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
    }

    /// The high rampart's processor removes blocks more often the higher
    /// they stand above the piece (Java's `axis_aligned_linear_pos`, 0 to
    /// 5% over 100 blocks), and the gold block piece paints gold.
    #[test]
    fn generation_6_processors_and_loose_blocks_follow_java() {
        let list = jigsaw::rules::list("high_rampart").unwrap();
        let origin = IVec3::new(0, 33, 0);
        let mut removed = [0; 2];
        for (i, dy) in [0, 100].into_iter().enumerate() {
            for x in 0..100 {
                for z in 0..100 {
                    let at = IVec3::new(x, 33 + dy, z);
                    if list.apply_in(BRICKS, at, Block::AIR, origin) == Some(AIR) {
                        removed[i] += 1;
                    }
                }
            }
        }
        assert_eq!(removed[0], 0, "nothing falls at the piece's own height");
        assert!((300..700).contains(&removed[1]), "about 5% at 100 blocks: {}", removed[1]);
        let lib = super::super::catalogue::library();
        let gold = lib.template("bastion/blocks/gold").unwrap();
        assert!(gold.cells.iter().any(|c| c.spec.base == jigsaw::Base::Block(Block::GOLD_BLOCK)));
    }

    /// Generation 6's pieces reach below y 33 (bridge legs) and above 70
    /// (the treasure court, ramparts): chunks there paint them too.
    #[test]
    fn generation_6_bastions_paint_outside_the_old_height_band() {
        let mut low = false;
        let mut high = false;
        for seed in 0..60 {
            let bastions = Bastions::with_generation(seed, Generation::V6);
            let Some(b) = bastions.get(IVec2::ZERO) else { continue };
            for (y, cy) in
                [(b.bounds.min.y, b.bounds.min.y.div_euclid(32)), (b.bounds.max.y, b.bounds.max.y.div_euclid(32))]
            {
                if (33..=70).contains(&y) || (low && y < 33) || (high && y > 70) {
                    continue;
                }
                let lo = chunk_of(b.bounds.min);
                let hi = chunk_of(b.bounds.max);
                let painted = (lo.x..=hi.x).flat_map(|x| (lo.z..=hi.z).map(move |z| (x, z))).any(|(x, z)| {
                    let base = IVec3::new(x, cy, z) * 32;
                    let mut blocks = Box::new([Block::NETHERRACK; CHUNK_VOLUME]);
                    bastions.paint(&mut blocks, base);
                    blocks.iter().any(|&b| b != Block::NETHERRACK)
                });
                assert!(painted, "seed {seed}: nothing painted at y {y}");
                if y < 33 {
                    low = true;
                } else {
                    high = true;
                }
            }
        }
        assert!(low || high, "no generation 6 bastion left the old band");
    }
}
