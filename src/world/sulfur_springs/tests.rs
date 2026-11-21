use super::*;
use crate::world::terrain::{Dimension, Generation};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/wave1_leftovers.json")).unwrap()
}

#[test]
fn placement_matches_the_java_features() {
    let f = fixture();
    let f = &f["sulfur_springs"];
    let selector = f["selector"].as_array().unwrap();
    assert_eq!(selector.len(), SIZES.len());
    for (java, &(weight, tries, spread, rise, names)) in selector.iter().zip(&SIZES) {
        assert_eq!(java["weight"], weight);
        assert_eq!(java["tuff"], "tuff");
        assert_eq!(java["tuff_count"], tries);
        assert_eq!(java["tuff_xz"], serde_json::json!([-spread, spread]));
        assert_eq!(java["tuff_y"], serde_json::json!([-rise, rise]));
        assert_eq!(java["tuff_scan"], TUFF_SCAN);
        assert_eq!(java["template_offset_y"], TEMPLATE_OFFSET_Y);
        assert_eq!(java["templates"], serde_json::json!(names));
    }
    for (name, size) in TEMPLATE_SIZES {
        assert_eq!(f["template_sizes"][name], serde_json::json!(size), "{name}");
    }
    let r = &f["root_system"];
    assert_eq!(r["required_vertical_space_for_tree"], REQUIRED_SPACE);
    assert_eq!(r["level_test_distance"], LEVEL_TEST_DISTANCE);
    assert_eq!(r["max_level_deviation"], MAX_LEVEL_DEVIATION);
    assert_eq!(r["root_radius"], ROOT_RADIUS);
    assert_eq!(r["root_placement_attempts"], ROOT_ATTEMPTS);
    assert_eq!(r["root_column_max_height"], ROOT_COLUMN_MAX);
    assert_eq!(r["hanging_root_radius"], 1);
    assert_eq!(r["hanging_roots_vertical_span"], 1);
    assert_eq!(r["hanging_root_placement_attempts"], 1);
    assert_eq!(r["allowed_vertical_water_for_tree"], 1);
    assert_eq!(r["root_replaceable"], "#minecraft:azalea_root_replaceable");
    assert_eq!(f["root_state"], "sulfur");
    assert_eq!(f["hanging_root_state"], "sulfur");
    assert_eq!(f["allowed_tree_position"], "air");
    let placement = f["placement"].as_array().unwrap();
    assert_eq!(placement[0]["count"]["min_inclusive"], COUNT.0);
    assert_eq!(placement[0]["count"]["max_inclusive"], COUNT.1);
    assert_eq!(placement[2]["height"]["max_inclusive"]["absolute"], MAX_Y);
    assert_eq!(placement[3]["max_steps"], CEILING_SCAN);
    assert_eq!(placement[4]["y"], -1);
    // The replaceable blocks this engine has.
    for name in f["base_stone_overworld"].as_array().unwrap() {
        let b = Block::from_name(name.as_str().unwrap()).unwrap();
        assert!(root_replaceable(b), "{name}");
    }
    assert!(!root_replaceable(g::SULFUR) && !root_replaceable(g::CINNABAR));
}

#[test]
fn templates_are_basins_over_dormant_vents() {
    for t in templates() {
        assert_eq!(t.cells.iter().map(|c| c.0).collect::<std::collections::HashSet<_>>().len(), t.cells.len());
        for &(p, _) in &t.cells {
            assert!(p.cmpge(IVec3::ZERO).all() && p.cmplt(t.size).all(), "{} {p}", t.name);
        }
        let at = |q: IVec3| t.cells.iter().find(|c| c.0 == q).map(|c| c.1);
        let vents: Vec<IVec3> = t.cells.iter().filter(|c| c.1 == g::potent(Potent::Dormant)).map(|c| c.0).collect();
        assert_eq!(vents.len(), if t.name.ends_with("extra_large_1") { 2 } else { 1 }, "{}", t.name);
        for v in vents {
            assert_eq!(at(v - IVec3::Y), Some(Block::MAGMA), "{}", t.name);
            assert_eq!(at(v + IVec3::Y), Some(Block::WATER), "{}", t.name);
            // The pool's surface is the top body layer, level with the floor.
            let surface = (1..=4).map(|k| v + IVec3::Y * k).take_while(|q| at(*q) == Some(Block::WATER)).last();
            assert_eq!(surface.map(|q| q.y), Some(6), "{}", t.name);
            assert_eq!(at(IVec3::new(v.x, 7, v.z)), Some(Block::AIR), "{}", t.name);
        }
        assert!(t.cells.iter().any(|c| g::spike_of(c.1).is_some()), "{} has spikes", t.name);
        assert!(t.cells.iter().any(|c| c.1 == Block::TUFF) && t.cells.iter().any(|c| c.1 == g::CINNABAR));
    }
}

#[test]
fn turned_templates_stay_centred() {
    let size = IVec3::new(9, 11, 8);
    let origin = IVec3::new(100, 20, -50);
    for turns in 0..4 {
        let mut lo = IVec3::splat(i32::MAX);
        let mut hi = IVec3::splat(i32::MIN);
        for x in 0..size.x {
            for z in 0..size.z {
                let p = placed_cell(origin, size, turns, IVec3::new(x, 0, z));
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        let span = hi - lo + IVec3::ONE;
        let expect = if turns % 2 == 0 { (size.x, size.z) } else { (size.z, size.x) };
        assert_eq!((span.x, span.z), expect, "turns {turns}");
        assert!((lo.x..=hi.x).contains(&origin.x) && (lo.z..=hi.z).contains(&origin.z), "turns {turns}");
    }
}

/// On seed 12345 generation 5's sulfur caves hold springs: a template, a
/// dormant vent over magma in the generated chunk; generation 4 has none.
/// Sulfur cave regions are small, so several are searched.
#[test]
fn generation_5_grows_sulfur_springs() {
    let generator = Generator::with_generation(12345, Dimension::Overworld, Generation::V5);
    let mut why: std::collections::BTreeMap<String, usize> = Default::default();
    let mut sites = Vec::new();
    let mut found = None;
    let per = CHUNK_SIZE_I / JAVA_CHUNK;
    'search: for oz in -3i32..=3 {
        for ox in -3i32..=3 {
            let Some(at) = generator.nearest_biome(IVec2::new(ox, oz) * 1500, Biome::SulfurCaves, 1500) else {
                continue;
            };
            let c = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
            if sites.contains(&(c.x, c.z)) {
                continue;
            }
            sites.push((c.x, c.z));
            for dz in -4..=4 {
                for dx in -4..=4 {
                    for sub in 0..per * per {
                        let col = IVec2::new(c.x + dx, c.z + dz) * per + IVec2::new(sub % per, sub / per);
                        for (start, rng) in generator.sulfur_springs.candidates(col) {
                            match generator.sulfur_springs.try_build(&generator, start, rng) {
                                Ok(s) => {
                                    found = Some(s);
                                    break 'search;
                                }
                                Err(r) => *why.entry(format!("{r:?}")).or_default() += 1,
                            }
                        }
                    }
                }
            }
        }
    }
    let spring = found.unwrap_or_else(|| panic!("no sulfur spring at {} sites: {why:?}", sites.len()));
    assert!(spring.spring.y > spring.origin.y);
    assert_eq!(generator.biome_at(spring.origin), Biome::SulfurCaves);
    // The cached layout agrees, and the generated chunk holds the vent.
    let column = IVec2::new(spring.origin.x, spring.origin.z).div_euclid(IVec2::splat(CHUNK_SIZE_I));
    assert!(generator.sulfur_springs.source(&generator, column).iter().any(|s| s.origin == spring.origin));
    let vent = spring.cells.iter().find(|c| c.1 == g::potent(Potent::Dormant)).expect("a vent").0;
    let chunk = vent.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    let data = generator.generate(chunk);
    let l = vent - chunk * CHUNK_SIZE_I;
    assert_eq!(data.get(l.x as usize, l.y as usize, l.z as usize), g::potent(Potent::Dormant));
    let old = Generator::with_generation(12345, Dimension::Overworld, Generation::V4);
    let mut any = false;
    old.generate(chunk).for_each_block(|b| any |= g::potent_state(b).is_some());
    assert!(!any, "generation 4 stays without springs");
}
