//! Generation 3 outposts, with original templates and Java pool weights.
use super::*;
use crate::world::jigsaw::{self, Marker, Palette};

const SETTINGS: jigsaw::Settings =
    jigsaw::Settings { start_pool: "pillager_outpost/base_plates", max_depth: 7, max_distance: 80 };

struct OutpostPalette;
impl Palette for OutpostPalette {
    fn role(&self, role: &str) -> Option<Block> {
        (role == "foundation").then_some(Block::COBBLESTONE)
    }
}

fn surface(g: &Generator, x: i32, z: i32) -> i32 {
    g.column(x, z).height.max(SEA_LEVEL - 1)
}

pub(super) fn build(g: &Generator, centre: IVec2, seed: u64) -> Built {
    let lib = jigsaw::outposts();
    let surface = |x, z| surface(g, x, z);
    let placed = jigsaw::assemble(lib, &SETTINGS, centre, &surface, seed);
    let bounds = placed.iter().map(|p| p.bounds).reduce(|a, b| a.union(&b)).expect("outpost start");
    let mut features = Vec::new();
    let mut residents = Vec::new();
    for marker in jigsaw::markers(lib, &placed, &surface) {
        match marker {
            Marker::Loot(at, "pillager_outpost", seed) => {
                features.push((at, Feature::TempleChest(seed, LootTable::Outpost)))
            }
            Marker::Mob(at, name) => {
                residents.push((crate::entity::MobKind::from_name(name).expect("outpost resident"), at))
            }
            _ => panic!("unsupported outpost marker {marker:?}"),
        }
    }
    Built { kind: Kind::Outpost, bounds, blocks: Vec::new(), features, spawns: bounds, residents, placed }
}

pub(super) fn paint(g: &Generator, built: &Built, blocks: &mut [Block; CHUNK_VOLUME], base: IVec3) {
    jigsaw::paint(
        jigsaw::outposts(),
        &built.placed,
        &OutpostPalette,
        &jigsaw::Ground { surface: &|x, z| surface(g, x, z), solid: &|x, z| g.column(x, z).height },
        blocks,
        base,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        World,
        chest::Chest,
        chunk::{chunk_of, index},
        terrain::Generation,
    };
    use std::collections::BTreeSet;

    #[test]
    fn pools_cover_every_feature_and_layouts_are_repeatable_without_overlaps() {
        let lib = jigsaw::outposts();
        let mut seen = BTreeSet::new();
        for seed in 0..160 {
            let a = jigsaw::assemble(lib, &SETTINGS, IVec2::new(-13, 27), &|_, _| 70, seed);
            let b = jigsaw::assemble(lib, &SETTINGS, IVec2::new(-13, 27), &|_, _| 70, seed);
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
            assert_eq!(jigsaw::markers(lib, &a, &|_, _| 70), jigsaw::markers(lib, &b, &|_, _| 70));
            assert!(a.iter().any(|p| lib.templates[p.template as usize].name.ends_with("watchtower")));
            for (i, p) in a.iter().enumerate() {
                seen.insert(lib.templates[p.template as usize].name.as_str());
                for q in &a[..i] {
                    assert!(!p.bounds.intersects(&q.bounds));
                }
            }
        }
        for t in &lib.templates {
            assert!(seen.contains(t.name.as_str()), "missing {}", t.name);
        }
        let features = &lib.pools["pillager_outpost/features"].elements;
        assert_eq!(features.iter().filter(|e| e.0.is_some()).count(), 7);
        assert_eq!(features.iter().find(|e| e.0.is_none()).unwrap().1, 6);
    }

    #[test]
    fn shifted_chunks_agree_and_all_residents_have_clear_supported_cells() {
        let lib = jigsaw::outposts();
        for seed in 0..12 {
            let pieces = jigsaw::assemble(lib, &SETTINGS, IVec2::new(29, -3), &|_, _| 70, seed);
            let ground = jigsaw::Ground { surface: &|_, _| 70, solid: &|_, _| 70 };
            let draw = |base| {
                let mut b = Box::new([Block::AIR; CHUNK_VOLUME]);
                jigsaw::paint(lib, &pieces, &OutpostPalette, &ground, &mut b, base);
                b
            };
            for p in &pieces {
                let base = chunk_of(p.bounds.min) * CHUNK_SIZE_I;
                let a = draw(base);
                let b = draw(base + IVec3::ONE);
                for x in 1..32 {
                    for z in 1..32 {
                        for y in 1..32 {
                            assert_eq!(a[index(x, y, z)], b[index(x - 1, y - 1, z - 1)]);
                        }
                    }
                }
            }
            for marker in jigsaw::markers(lib, &pieces, &|_, _| 70) {
                let (at, block) = match marker {
                    Marker::Mob(at, _) => (at, Block::AIR),
                    Marker::Loot(at, _, _) => (at, Block::CHEST),
                    _ => unreachable!(),
                };
                let base = chunk_of(at - IVec3::Y) * CHUNK_SIZE_I;
                let b = draw(base);
                let at_block = |pos: IVec3| {
                    let c = draw(chunk_of(pos) * CHUNK_SIZE_I);
                    let p = pos.rem_euclid(IVec3::splat(32));
                    c[index(p.x as usize, p.y as usize, p.z as usize)]
                };
                assert_eq!(at_block(at).base(), block);
                if block == Block::AIR {
                    assert_eq!(at_block(at + IVec3::Y), Block::AIR);
                    let l = (at - IVec3::Y) - base;
                    assert!(b[index(l.x as usize, l.y as usize, l.z as usize)].is_solid());
                }
            }
        }
    }

    #[test]
    fn generated_outpost_chest_registers_and_saved_empty_inventory_wins() {
        let generator = Arc::new(Generator::with_generation(42, Dimension::Overworld, Generation::V3));
        let at = generator.temples.nearest(&generator, Kind::Outpost, IVec2::ZERO, 20_000).expect("outpost");
        let b = generator.temples.near(&generator, at, 96).into_iter().find(|b| b.kind == Kind::Outpost).unwrap();
        assert!(!b.placed.is_empty());
        assert!(b.residents.iter().filter(|r| r.0 == crate::entity::MobKind::Pillager).count() >= 3);
        let chest = b.features[0].0;
        let cpos = chunk_of(chest);
        let data = generator.generate(cpos);
        let mut world = World::new_headless(generator.clone(), Default::default(), 2);
        world.register_structure_features(cpos, &data);
        assert!(world.chests[&chest].slots.iter().any(Option::is_some));
        world.chests.insert(chest, Chest::default());
        let saved = world.chests_to_string();
        let mut loaded = World::new_headless(generator, Default::default(), 2);
        loaded.load_chests(&saved);
        loaded.register_structure_features(cpos, &data);
        assert!(loaded.chests[&chest].slots.iter().all(Option::is_none));
        eprintln!("generation 3 outpost seed42 locate {at}, chest {chest}");
    }
}
