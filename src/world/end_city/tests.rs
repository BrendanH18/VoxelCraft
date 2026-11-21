use super::*;
use crate::world::end::EndGen;
use crate::world::terrain::Generation;

/// Flat land everywhere at `y`, all of it End highlands.
struct Flat(i32);

impl Terrain for Flat {
    fn city_biome(&self, _: i32, _: i32) -> bool {
        true
    }
    fn first_free(&self, _: i32, _: i32) -> i32 {
        self.0
    }
}

#[test]
fn placement_follows_the_end_cities_structure_set() {
    for seed in [1u64, 42, 12345] {
        for rx in -3..3 {
            for rz in -3..3 {
                let c = start_chunk(seed, IVec2::new(rx, rz));
                let off = c - IVec2::new(rx, rz) * SPACING;
                assert!((0..SPACING - SEPARATION).contains(&off.x) && (0..SPACING - SEPARATION).contains(&off.y));
            }
        }
    }
    // Low land drops the start.
    assert!(start(7, &Flat(59), IVec2::new(3, 4)).is_none());
    let s = start(7, &Flat(60), IVec2::new(3, 4)).unwrap();
    assert_eq!(s.origin, IVec3::new(3 * 16 + 7, 60, 4 * 16 + 7));
}

#[test]
fn cities_assemble_from_javas_piece_rules() {
    let mut ships = 0;
    let mut kinds = std::collections::BTreeSet::new();
    for seed in 0..40u64 {
        let s = start(seed, &Flat(64), IVec2::new(10, -6)).unwrap();
        let ps = pieces(seed, &s);
        // `startHouseTower`: the first four pieces.
        let names: Vec<_> = ps.iter().take(4).map(Piece::name).collect();
        assert_eq!(names, ["base_floor", "second_floor_1", "third_floor_1", "third_roof"]);
        assert!(ps.iter().filter(|p| p.name() == "ship").count() <= 1);
        ships += ps.iter().filter(|p| p.name() == "ship").count();
        for p in &ps {
            kinds.insert(p.name());
            let c = s.origin;
            assert!(
                (p.bounds.min.x - c.x).abs() <= REACH
                    && (p.bounds.max.x - c.x).abs() <= REACH
                    && (p.bounds.min.z - c.z).abs() <= REACH
                    && (p.bounds.max.z - c.z).abs() <= REACH,
                "seed {seed}: {} reaches past {REACH}",
                p.name()
            );
        }
        // The same start always lays out the same city.
        assert_eq!(pieces(seed, &s), ps);
    }
    assert!(ships > 0, "some of forty cities have a ship");
    for kind in ["tower_base", "tower_piece", "tower_top", "bridge_end", "bridge_piece", "fat_tower_top"] {
        assert!(kinds.contains(kind), "{kind} never generated");
    }
}

#[test]
fn templates_match_javas_sizes_and_markers() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/end_cities.json")).unwrap();
    let java = f["templates"].as_object().unwrap();
    assert_eq!(java.len(), TEMPLATES.len());
    for t in &TEMPLATES {
        let j = &java[t.name];
        let size: Vec<i32> = j["size"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap() as i32).collect();
        assert_eq!(size, t.size, "{}", t.name);
        assert_eq!(j["filled"].as_bool(), Some(t.filled), "{}", t.name);
        let markers: Vec<(String, Vec<i32>)> = j["markers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["name"].as_str().unwrap().to_string(),
                    m["pos"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap() as i32).collect(),
                )
            })
            .collect();
        let ours: Vec<(String, Vec<i32>)> = t.markers.iter().map(|(m, p)| (m.java().to_string(), p.to_vec())).collect();
        assert_eq!(ours, markers, "{}", t.name);
        // Every stored cell resolves to a real block.
        let built = template(template_index(t.name).unwrap());
        for &(_, p) in built.markers() {
            assert_eq!(built.cell(p), None, "a marker's cell is never placed");
        }
    }
    let set = &f["structure_set"]["placement"];
    assert_eq!(set["spacing"], SPACING);
    assert_eq!(set["separation"], SEPARATION);
    assert_eq!(set["salt"], SALT);
    assert_eq!(set["spread_type"], "triangular");
}

#[test]
fn generation_5_end_has_reachable_cities_with_loot_and_shulkers() {
    let g = EndGen::with_generation(42, Generation::V5);
    // A city within reach of where each of the first gateways leads.
    for gateway in g.gateways().iter().take(4) {
        let exit = crate::world::end_portal::gateway_exit(&g, &|_| None, *gateway, 256).exit;
        let found = g.nearest_city(IVec2::new(exit.x, exit.z), 1200);
        assert!(found.is_some(), "no End city near the gateway exit {exit}");
    }
    let at = g.nearest_city(IVec2::new(1500, 0), 3000).expect("a city");
    let cities = g.cities_near(IVec2::new(at.x, at.z), IVec2::new(at.x, at.z));
    let city = cities.iter().find(|c| c.start.origin == at).expect("the located city is built");
    assert!(city.start.origin.y >= MIN_START_Y);
    assert!(!city.features.is_empty(), "loot chests");
    assert!(!city.sentries.is_empty(), "shulkers");
    // Its blocks reach the chunk holding the start.
    let cpos = at.div_euclid(IVec3::splat(32));
    let chunk = g.generate(cpos);
    let mut purpur = 0;
    chunk.for_each_block(|b| purpur += usize::from(crate::world::end_blocks::owns(b.0)));
    assert!(purpur > 0);
    // Generation 4 has none.
    assert!(EndGen::new(42).nearest_city(IVec2::new(1500, 0), 3000).is_none());
}
