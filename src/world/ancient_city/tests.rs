use super::super::chunk::chunk_of;
use super::*;

const SEED: u64 = 12345;

#[test]
fn placement_matches_independent_java_random_starts() {
    // Independently calculated with a Python 48-bit java.util.Random
    // (the same oracle reproduces the trail ruins' vectors): seed 12345,
    // spacing 24, separation 8, salt 20083232.
    let cities = AncientCities::new(SEED);
    for (r, c) in [((0, 0), (11, 15)), ((-1, -1), (-18, -18)), ((2, -3), (59, -60)), ((-17, 23), (-402, 553))] {
        assert_eq!(cities.start_chunk(IVec2::new(r.0, r.1)), IVec2::new(c.0, c.1));
    }
}

fn city(g: &Generator) -> Arc<City> {
    let at = g.ancient_cities.nearest(g, IVec2::ZERO, 8000).expect("an ancient city within 8000 blocks");
    eprintln!("ancient_city: {at}");
    let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * SPACING));
    let city = g.ancient_cities.get(g, region).expect("the located city");
    assert_eq!(city.at, at);
    city
}

#[test]
fn cities_stand_in_the_deep_dark_round_their_portal() {
    let g = Generator::new(SEED);
    let city = city(&g);
    let lib = jigsaw::ancient_cities();
    assert!(allowed(g.biome_at(city.at)));
    assert_eq!(city.at.y, START_Y);
    let start = city.pieces[0];
    assert!(lib.templates[start.template as usize].name.starts_with("ancient_city/city_center/city_center_"));
    assert_eq!(start.origin.y, FLOOR_Y);
    assert_eq!(((start.bounds.min + start.bounds.max) / 2).with_y(START_Y), city.at);
    assert!(city.pieces.len() >= 12, "{} pieces", city.pieces.len());
    for (i, a) in city.pieces.iter().enumerate() {
        assert!(a.depth <= MAX_DEPTH + 1);
        let d = (a.bounds.max - city.at).abs().max((a.bounds.min - city.at).abs());
        assert!(d.x <= MAX_DISTANCE + 1 && d.z <= MAX_DISTANCE + 1, "{a:?}");
        for b in &city.pieces[i + 1..] {
            assert!(!a.bounds.intersects(&b.bounds), "{a:?} overlaps {b:?}");
        }
    }
    let has = |pred: &dyn Fn(Block) -> bool| city.cells.values().flatten().any(|&(_, b)| pred(b));
    assert!(has(&|b| b == super::super::jukebox::REINFORCED_DEEPSLATE));
    assert!(has(&|b| {
        b == super::super::nether_biome_blocks::SOUL_FIRE
            || super::super::mount_states::lantern_of(b).is_some_and(|l| l.soul)
    }));
    assert!(has(&|b| cb::sensor_of(b).is_some()));
    let summoning = cb::sculk_shrieker(false, true, false);
    let placed = city.ops.values().flatten().any(|&(_, op)| op == PatchOp::Place(summoning));
    assert!(has(&|b| b == summoning) || placed, "summoning shriekers");
    assert!(!has(&|b| cb::shrieker_of(b).is_some_and(|(_, summon, _)| !summon)), "every city shrieker can summon");
    assert!(city.ops.values().flatten().any(|&(_, op)| op == PatchOp::Sculk), "sculk patches");
    assert!(!city.chests.is_empty());
    assert!(g.in_ancient_city(city.at.with_y(FLOOR_Y + 2)));
    assert!(!g.in_ancient_city(city.at + IVec3::new(2000, 0, 0)));
    // Only generation 3 has cities.
    let old = Generator::with_generation(SEED, Dimension::Overworld, Generation::V2);
    assert!(old.ancient_cities.nearest(&old, IVec2::ZERO, 8000).is_none());
    assert!(!old.in_ancient_city(city.at));
}

#[test]
fn generated_chunks_carry_the_city_and_its_chests() {
    let g = Arc::new(Generator::new(SEED));
    let city = city(&g);
    let start = city.pieces[0];
    // The portal frame's west pillar, and open air over the plaza's corner.
    let frame = start.world(IVec3::new(13, 10, 21));
    let open = start.world(IVec3::new(4, 12, 4));
    for p in [frame, open] {
        let data = g.generate(chunk_of(p));
        let l = p - chunk_of(p) * CHUNK_SIZE_I;
        let b = data.get(l.x as usize, l.y as usize, l.z as usize);
        if p == frame {
            assert_eq!(b, super::super::jukebox::REINFORCED_DEEPSLATE);
        } else {
            assert_eq!(b, Block::AIR, "carved over the plaza at {p}");
        }
    }
    // Painting is independent of the chunk grid's offset (GEN-07).
    let mut a = Box::new([Block::STONE; CHUNK_VOLUME]);
    let base = chunk_of(frame) * CHUNK_SIZE_I;
    g.ancient_cities.paint(&g, &mut a, base);
    g.ancient_cities.cache.lock().unwrap().clear();
    let mut b = Box::new([Block::STONE; CHUNK_VOLUME]);
    g.ancient_cities.paint(&g, &mut b, base);
    assert!(a.iter().zip(b.iter()).all(|(x, y)| x == y));
    // Chests register their tables when their chunk loads.
    let (chest, _, ice) = city.chests[0];
    let cpos = chunk_of(chest);
    let data = g.generate(cpos);
    assert!(g.structure_features(cpos).iter().any(|(p, f)| {
        *p == chest && matches!(f, super::super::fortress::Feature::AncientCityChest(_, i) if *i == ice)
    }));
    let mut world = super::super::World::new_headless(g.clone(), Default::default(), 1);
    world.register_structure_features(cpos, &data);
    let filled = world.chests.get(&chest).expect("a filled chest");
    assert!(filled.slots.iter().flatten().count() > 0);
}

#[test]
fn processors_follow_the_java_lists() {
    let lib = jigsaw::ancient_cities();
    let piece = |name: &str| jigsaw::single(lib, name, IVec3::ZERO, 0, 7);
    let generic = piece("ancient_city/structures/barracks");
    let start = piece("ancient_city/city_center/city_center_1");
    let walls = piece("ancient_city/walls/intact_horizontal_wall_1");
    let ice = piece("ancient_city/structures/ice_box_1");
    let slab = Block::from_name("deepslate tile slab").unwrap();
    let (mut cracked, mut gone, mut start_cracked, mut start_gone, mut slabs_gone) = (0, 0, 0, 0, 0);
    let n = 40_000;
    for i in 0..n {
        let at = IVec3::new(i % 200, -50, i / 200);
        match CityPalette.process(sb::DEEPSLATE_BRICKS, at, &generic) {
            None => gone += 1,
            Some(b) if b == sb::CRACKED_DEEPSLATE_BRICKS => cracked += 1,
            _ => {}
        }
        match CityPalette.process(sb::DEEPSLATE_BRICKS, at, &start) {
            None => start_gone += 1,
            Some(b) if b == sb::CRACKED_DEEPSLATE_BRICKS => start_cracked += 1,
            _ => {}
        }
        if CityPalette.process(slab, at, &walls) != Some(slab) {
            slabs_gone += 1;
        }
        assert_eq!(CityPalette.process(sb::DEEPSLATE_BRICKS, at, &ice), Some(sb::DEEPSLATE_BRICKS));
    }
    // One position-seeded float decides both: rot above 0.95, cracking below 0.3.
    let near = |count: i32, share: f64| (count as f64 / n as f64 - share).abs() < 0.01;
    assert!(near(gone, 0.05) && near(cracked, 0.3), "{gone} {cracked}");
    assert!(start_gone == 0 && near(start_cracked, 0.3), "{start_gone} {start_cracked}");
    // Walls: rot, then the slab rule.
    assert!(near(slabs_gone, 0.05 + 0.3), "{slabs_gone}");
    assert_eq!(CityPalette.role("shrieker"), Some(cb::sculk_shrieker(false, true, false)));
}
