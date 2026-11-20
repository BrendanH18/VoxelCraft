//! Abandoned camps against Java 26.3's structure set, pools and loot.
use super::*;
use crate::world::terrain::Generation;

fn jar_pool(name: &str) -> &'static Pool {
    &jigsaw::abandoned_camps().pools[name]
}
use crate::world::jigsaw::Pool;

#[test]
fn the_catalogue_covers_javas_pools_and_piece_kinds() {
    let lib = jigsaw::abandoned_camps();
    for c in &CAMPS {
        let name = c.biome.name();
        let tents = jar_pool(&format!("abandoned_camp/tent/{name}"));
        assert!(tents.elements.len() >= 3, "{name}: tents");
        for (t, w) in &tents.elements {
            let t = &lib.templates[t.expect("no empty tents") as usize];
            assert_eq!(*w, 1, "every tent element weighs 1, as Java's");
            assert!(matches!((t.size.x, t.size.z), (8, 8) | (6, 8)), "{}: Java's tents are 8x8 or 6x8", t.name);
            assert!(t.jigsaws.iter().any(|j| j.name == "connector" && j.target == "end_connector"));
        }
        // The default chest, barrel and special campsites weigh 15 each (Java's 15
        // of each kind) against the biome's own campsites' 4.
        let camps = jar_pool(&format!("abandoned_camp/camp/{name}"));
        let weights: Vec<u32> = camps.elements.iter().map(|e| e.1).collect();
        assert_eq!(weights, [15, 15, 15, 4], "{name}");
        for (t, _) in &camps.elements {
            let t = &lib.templates[t.unwrap() as usize];
            assert!(t.jigsaws.iter().any(|j| j.name == "end_connector" && j.target == "connector"));
            assert!(t.size.x == 10 && t.size.z == 10, "{}: Java's campsites are 10 wide", t.name);
        }
        for tree in c.tent_trees.iter().chain(c.camp_trees) {
            assert!(TREE_POOLS.contains(tree), "{tree}");
        }
    }
    for tree in TREE_POOLS {
        assert!(lib.pools.contains_key(&format!("abandoned_camp/trees/{tree}")));
        assert!(tree_blocks(tree, IVec3::new(0, 64, 0), 7).len() > 4, "{tree} grows");
    }
    // Every container is filled from one of Java's three camp tables.
    let mut tables: Vec<&str> = lib.loot.iter().map(String::as_str).collect();
    tables.sort_unstable();
    assert_eq!(tables, ["abandoned_camp_common_chest", "abandoned_camp_secret_chest", "barrels/abandoned_camp_barrel"]);
    for t in &tables {
        assert!(!crate::world::loot::roll_items(t, &mut crate::world::loot::LootRandom::new(5), |_| None).is_empty());
    }
}

#[test]
fn placement_is_javas_random_spread_and_the_camp_set() {
    // `worldgen/structure_set/abandoned_camp.json`.
    assert_eq!((SPACING, SEPARATION, SALT), (37, 8, 91_231_127));
    let t = Temples::new(12345);
    for r in [IVec2::new(0, 0), IVec2::new(-1, -1), IVec2::new(3, -2)] {
        assert_eq!(
            t.start_chunk(Kind::AbandonedCamp, r),
            crate::world::trail_ruins::spread(12345, r, SPACING, SEPARATION, SALT)
        );
    }
    assert_eq!(Kind::from_name("minecraft:abandoned_camp_forest"), Some(Kind::AbandonedCamp));
    assert_eq!(Kind::from_name("abandoned_camp"), Some(Kind::AbandonedCamp));
    let biomes: Vec<&str> = CAMPS.iter().map(|c| c.biome.name()).collect();
    assert_eq!(
        biomes,
        [
            "bamboo_jungle",
            "birch_forest",
            "cherry_grove",
            "dappled_forest",
            "flower_forest",
            "forest",
            "meadow",
            "old_growth_pine_taiga",
            "old_growth_birch_forest",
            "old_growth_spruce_taiga",
            "pale_garden",
            "savanna",
            "snowy_taiga",
            "sparse_jungle",
            "swamp",
            "taiga",
            "windswept_forest",
            "wooded_badlands",
        ]
    );
    assert!(Kind::AbandonedCamp.allowed(Biome::Forest) && !Kind::AbandonedCamp.allowed(Biome::Plains));
}

/// Generation 5, seed 12345: a camp near spawn with its tent, campsite,
/// loot containers and trees, painted into the chunks it crosses; none in
/// generation 4.
#[test]
fn generation_5_finds_and_builds_an_abandoned_camp() {
    let g = Generator::with_generation(12345, Dimension::Overworld, Generation::V5);
    let at = g.temples.nearest(&g, Kind::AbandonedCamp, IVec2::ZERO, 12000).expect("a camp within 12000 blocks");
    eprintln!("abandoned camp: {at}");
    let start = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16));
    let region = start.div_euclid(IVec2::splat(SPACING));
    let built = g.temples.get(&g, Kind::AbandonedCamp, region).expect("built");
    assert_eq!(built.placed.len(), 2, "a tent and its campsite (size 2)");
    let lib = jigsaw::abandoned_camps();
    let names: Vec<&str> = built.placed.iter().map(|p| lib.templates[p.template as usize].name.as_str()).collect();
    assert!(names[0].starts_with("abandoned_camp/tent/") && names[1].starts_with("abandoned_camp/camp/"), "{names:?}");
    let camp = start_biome(&g, start).expect("a camp biome");
    assert!(names[0].contains(camp.biome.name()));
    let loot: Vec<&str> = built
        .features
        .iter()
        .filter_map(|(_, f)| match f {
            Feature::StructureLoot(_, t) => Some(*t),
            _ => None,
        })
        .collect();
    assert!(loot.len() >= 2, "{loot:?}");
    // Painted: the tent's wool and the campfire, in whichever chunks hold them.
    let bounds = built.bounds;
    let (lo, hi) =
        (bounds.min.div_euclid(IVec3::splat(CHUNK_SIZE_I)), bounds.max.div_euclid(IVec3::splat(CHUNK_SIZE_I)));
    let (mut wool, mut fire, mut chests) = (0, 0, 0);
    for x in lo.x..=hi.x {
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                let data = g.generate(IVec3::new(x, y, z));
                data.for_each_block(|b| {
                    wool += usize::from(b.name() == "white wool stairs" || b.name() == "white wool");
                    fire += usize::from(b.name() == "campfire");
                    chests += usize::from(crate::world::chest::is_chest(b));
                });
            }
        }
    }
    assert!(wool > 0 && fire > 0 && chests >= 2, "{wool} wool, {fire} campfires, {chests} containers at {at}");
    // `/locate` finds it; generation 4 has none.
    let old = Generator::with_generation(12345, Dimension::Overworld, Generation::V4);
    assert!(old.temples.nearest(&old, Kind::AbandonedCamp, IVec2::ZERO, 12000).is_none());
}

#[test]
fn common_chests_give_camp_maps_for_other_biomes_only() {
    use crate::world::camp_maps::{CAMP_MAP, DESTINATIONS};
    use crate::world::loot::{LootRandom, roll_items_at};
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..400 {
        let items = roll_items_at("abandoned_camp_common_chest", &mut LootRandom::new(seed), Some("swamp"), |s| {
            Some(crate::inventory::Stack { map_id: Some(1), ..s })
        });
        for s in items.iter().filter(|s| s.item == CAMP_MAP) {
            let i = s.entity_data as usize - 1;
            assert_ne!(DESTINATIONS[i].0, Biome::Swamp, "never a map to the swamp camp from a swamp");
            assert_eq!(s.name.as_str(), Some(DESTINATIONS[i].1));
            seen.insert(i);
        }
    }
    assert_eq!(seen.len(), DESTINATIONS.len() - 1, "the other seven destinations");
    // Without a resolver (no world) the map is discarded, like other explorer maps.
    let items = roll_items_at("abandoned_camp_common_chest", &mut LootRandom::new(1), Some("swamp"), |_| None);
    assert!(items.iter().all(|s| s.item != CAMP_MAP));
}

#[test]
fn camp_maps_point_at_a_generated_camp_of_their_biome() {
    use crate::world::camp_maps::{CAMP_MAP, DESTINATIONS};
    let g = Generator::with_generation(12345, Dimension::Overworld, Generation::V5);
    let mut found = 0;
    for (i, (biome, _)) in DESTINATIONS.iter().enumerate() {
        let stack = crate::inventory::Stack { entity_data: i as u32 + 1, ..crate::inventory::Stack::new(CAMP_MAP, 1) };
        if let Some(data) = crate::world::maps::explorer_map_stack(&g, IVec3::ZERO, stack, 50) {
            let d = data.decorations.iter().find(|d| d.kind == "abandoned_camp").expect("the camp decoration");
            let start = IVec2::new(d.x as i32, d.z as i32).div_euclid(IVec2::splat(16));
            assert_eq!(start_biome(&g, start).map(|c| c.biome), Some(*biome));
            found += 1;
        }
    }
    assert!(found >= 4, "most camp kinds within 50 rings: {found}");
}
