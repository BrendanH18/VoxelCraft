use super::*;
use crate::world::World;
use crate::world::terrain::Generator;
use std::sync::Arc;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/wave1_leftovers.json")).unwrap()
}

/// A flat test column with air well above the ground at y 0.
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

fn run(w: &mut World, from: u64, ticks: u64) -> Vec<Event> {
    let mut out = Vec::new();
    for t in from..from + ticks {
        w.tick_potent_sulfur(t);
        out.extend(w.take_potent_sulfur_events());
    }
    out
}

/// A geyser at `at`: magma under potent sulfur under `water` water sources.
fn geyser(w: &mut World, at: IVec3, below: Block, water: i32) {
    w.set_block(at - IVec3::Y, below);
    w.set_block(at, g::potent(Potent::Dry));
    for i in 1..=water {
        w.set_block(at + IVec3::Y * i, Block::WATER);
    }
}

#[test]
fn constants_match_the_jar() {
    let f = &fixture()["potent_sulfur"];
    assert_eq!(f["allowed_water_blocks_above"], ALLOWED_WATER_ABOVE);
    assert_eq!(f["effect_application_frequency_ticks"], EFFECT_INTERVAL);
    assert_eq!(f["effect_duration_in_ticks"], EFFECT_TICKS);
    assert_eq!(f["effect_range"].as_f64().unwrap() as f32, EFFECT_RANGE as f32);
    assert_eq!(f["particle_frequency_ticks"], PARTICLE_INTERVAL);
    assert_eq!(f["sound_frequency_ticks"], SOUND_INTERVAL);
    assert_eq!(f["geyser_base_launch_speed"].as_f64().unwrap() as f32, BASE_LAUNCH_SPEED);
    assert_eq!(f["geyser_launch_force"].as_f64().unwrap() as f32, LAUNCH_FORCE);
    assert_eq!(f["geyser_salt"].as_i64().unwrap(), GEYSER_SALT);
    assert_eq!(f["continuous_tag"], serde_json::json!(["lava"]));
    assert_eq!(f["periodic_tag"], serde_json::json!(["magma_block"]));
    assert_eq!(f["not_affected_by_geysers"], serde_json::json!(["ender_dragon"]));
    let cues: Vec<&str> = Cue::ALL.iter().map(|c| c.key()).collect();
    for key in f["sounds"].as_array().unwrap() {
        assert!(cues.contains(&key.as_str().unwrap()), "{key}");
    }
}

/// `geyserPositional` against draws made by the jar's own
/// `XoroshiroRandomSource` (tools/JavaGeyserVectors.java).
#[test]
fn countdowns_follow_javas_positional_random() {
    let vectors = fixture()["geyser_countdowns"].as_array().unwrap().clone();
    assert!(!vectors.is_empty());
    for v in vectors {
        let seed = v["seed"].as_i64().unwrap() as u64;
        let p = IVec3::new(
            v["x"].as_i64().unwrap() as i32,
            v["y"].as_i64().unwrap() as i32,
            v["z"].as_i64().unwrap() as i32,
        );
        // One block of water: the dormant draw is 15..=30 itself, the
        // erupting one 1..=2.
        assert_eq!(countdown(seed, p, true, 1), v["dormant_draw"].as_i64().unwrap() as i32, "{v}");
        assert_eq!(countdown(seed, p, false, 1), v["erupting_draw"].as_i64().unwrap() as i32, "{v}");
        // Deeper water lengthens both.
        assert_eq!(countdown(seed, p, true, 3), 20 + v["dormant_draw"].as_i64().unwrap() as i32);
        assert_eq!(countdown(seed, p, false, 3), 2 + v["erupting_draw"].as_i64().unwrap() as i32);
    }
}

#[test]
fn states_follow_valid_block_state() {
    use Potent::*;
    let water = Block::WATER;
    assert_eq!(valid_state(Wet, Block::AIR, Block::MAGMA), (Dry, false));
    assert_eq!(valid_state(Dry, water, Block::STONE), (Wet, false));
    assert_eq!(valid_state(Dry, water, Block::LAVA), (Continuous, false));
    // Flowing lava isn't a source.
    assert_eq!(valid_state(Dry, water, Block(38)), (Wet, false));
    assert_eq!(valid_state(Dry, water, Block::MAGMA), (Dormant, true));
    assert_eq!(valid_state(Wet, water, Block::MAGMA), (Dormant, true));
    assert_eq!(valid_state(Dormant, water, Block::MAGMA), (Dormant, false));
    assert_eq!(valid_state(Erupting, water, Block::MAGMA), (Erupting, false));
    // Flowing water above isn't a source either.
    assert_eq!(valid_state(Wet, Block(25), Block::STONE), (Dry, false));
}

#[test]
fn the_gas_source_is_the_first_open_cell_over_the_water() {
    let at = IVec3::new(0, 10, 0);
    let column = |water: i32, top: Block| {
        move |p: IVec3| {
            Some(match p.y - at.y {
                d if d >= 1 && d <= water => Block::WATER,
                d if d == water + 1 => top,
                _ => Block::AIR,
            })
        }
    };
    assert_eq!(gas_source(at, column(1, Block::AIR)), Some(at + IVec3::Y * 2));
    assert_eq!(gas_source(at, column(4, Block::AIR)), Some(at + IVec3::Y * 5));
    // Five blocks of water is too deep; a lid stops the gas.
    assert_eq!(gas_source(at, column(5, Block::AIR)), None);
    assert_eq!(gas_source(at, column(2, Block::STONE)), None);
    assert_eq!(unobstructed(at + IVec3::Y, 2, column(2, Block::AIR)), 12);
    assert_eq!(unobstructed(at + IVec3::Y, 2, column(2, Block::STONE)), 2);
    assert_eq!(launch_box(at, 12), (DVec3::new(0.0, 11.0, 0.0), DVec3::new(1.0, 23.0, 1.0)));
    assert_eq!(launch_box(at, 0), (DVec3::new(0.0, 10.0, 0.0), DVec3::new(1.0, 12.0, 1.0)));
}

#[test]
fn placed_and_neighbour_changes_settle_the_state() {
    let mut w = world();
    let at = IVec3::new(4, 10, 4);
    geyser(&mut w, at, Block::MAGMA, 2);
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Dormant)));
    w.set_block(at - IVec3::Y, Block::STONE);
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Wet)));
    w.set_block(at - IVec3::Y, Block::LAVA);
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Continuous)));
    w.set_block(at + IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Dry)));
    w.set_block(at, Block::AIR);
    assert!(w.potent_sulfur(at).is_none());
}

#[test]
fn a_dormant_geyser_erupts_and_settles_again() {
    let mut w = world();
    let at = IVec3::new(4, 10, 4);
    geyser(&mut w, at, Block::MAGMA, 1);
    let dormant = countdown(w.generator.seed, at, true, 1);
    // The first countdown tick draws the wait and counts it once.
    let events = run(&mut w, 1, 20);
    assert!(events.iter().all(|e| !matches!(e, Event::Launch { .. })));
    assert_eq!(w.potent_sulfur(at).unwrap().countdown, dormant - 1);
    let mut t = 21;
    while w.get_block(at) == Some(g::potent(Potent::Dormant)) {
        run(&mut w, t, 1);
        t += 1;
        assert!(t < 20 * 40, "never erupted");
    }
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Erupting)));
    // It erupted on the countdown tick that reached zero.
    assert_eq!((t - 1) % COUNTDOWN_INTERVAL, 0);
    assert_eq!((t - 1) / COUNTDOWN_INTERVAL, dormant as u64);
    let events = run(&mut w, t, 1);
    let lift = events.iter().find_map(|e| match e {
        Event::Launch { min, max, limit } => Some((*min, *max, *limit)),
        _ => None,
    });
    let (min, max, limit) = lift.expect("an erupting geyser lifts");
    assert_eq!(min, (at + IVec3::Y).as_dvec3());
    // One block of water and six clear cells for it.
    assert_eq!(max.y, f64::from(at.y) + 1.0 + 6.0);
    assert!((limit - 0.4).abs() < 1e-6);
    let erupt = countdown(w.generator.seed, at, false, 1);
    run(&mut w, t + 1, 20 * (erupt as u64 + 1));
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Dormant)));
}

#[test]
fn continuous_geysers_lift_every_tick_and_wet_ones_gas() {
    let mut w = world();
    let hot = IVec3::new(2, 10, 2);
    geyser(&mut w, hot, Block::LAVA, 2);
    let wet = IVec3::new(8, 10, 8);
    geyser(&mut w, wet, Block::STONE, 1);
    let events = run(&mut w, 1, 20);
    let lifts = events.iter().filter(|e| matches!(e, Event::Launch { .. })).count();
    assert_eq!(lifts, 20);
    let gas: Vec<IVec3> = events
        .iter()
        .filter_map(|e| match e {
            Event::Nausea { source } => Some(*source),
            _ => None,
        })
        .collect();
    assert_eq!(gas, vec![wet + IVec3::Y * 2; 2]);
    // An eye just over the water, within three blocks, breathes the gas.
    let eye = DVec3::new(8.5, 12.2, 9.5);
    assert!(w.noxious_gas_reaches(wet + IVec3::Y * 2, eye));
    assert!(!w.noxious_gas_reaches(wet + IVec3::Y * 2, DVec3::new(8.5, 13.2, 9.5)));
    assert!(!w.noxious_gas_reaches(wet + IVec3::Y * 2, DVec3::new(12.5, 12.2, 8.5)));
}

#[test]
fn launches_lift_mobs_and_items() {
    let mut w = world();
    let at = IVec3::new(4, 10, 4);
    geyser(&mut w, at, Block::LAVA, 1);
    let mut entities = crate::entity::Entities::new(1);
    let mut m = crate::entity::Mob::new(crate::entity::MobKind::Pig, DVec3::new(4.5, 11.0, 4.5), 0.0);
    m.vel = DVec3::ZERO;
    entities.mobs.push(m);
    for e in run(&mut w, 1, 1) {
        entities.potent_sulfur(&w, &e);
    }
    let v = entities.mobs[0].vel.y * crate::simulation::TICK_SECONDS;
    assert!((v - f64::from(LAUNCH_FORCE)).abs() < 1e-9, "{v}");
    // Rising faster than the limit, nothing more is added.
    entities.mobs[0].vel.y = 0.5 / crate::simulation::TICK_SECONDS;
    for e in run(&mut w, 2, 1) {
        entities.potent_sulfur(&w, &e);
    }
    assert!((entities.mobs[0].vel.y * crate::simulation::TICK_SECONDS - 0.5).abs() < 1e-9);
}

#[test]
fn countdowns_survive_saving() {
    let mut w = world();
    let at = IVec3::new(4, 10, 4);
    geyser(&mut w, at, Block::MAGMA, 1);
    run(&mut w, 1, 40);
    let left = w.potent_sulfur(at).unwrap().countdown;
    assert!(left > 0);
    let saved = w.potent_sulfur_to_string();
    let mut again = world();
    again.load_potent_sulfur(&saved);
    assert_eq!(again.potent_sulfur(at).map(|g| g.countdown), Some(left));
}

/// Generated potent sulfur settles on its first tick: a template's dry
/// block over magma under water turns dormant.
#[test]
fn loaded_potent_sulfur_settles() {
    use crate::world::chunk::ChunkData;
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    let mut data = ChunkData::new_dense(Block::AIR);
    let at = IVec3::new(5, 5, 5);
    let i = |p: IVec3| crate::world::chunk::index(p.x as usize, p.y as usize, p.z as usize);
    data[i(at - IVec3::Y)] = Block::MAGMA;
    data[i(at)] = g::potent(Potent::Dry);
    data[i(at + IVec3::Y)] = Block::WATER;
    for y in w.generator.dimension.chunk_rows() {
        let chunk = if y == 0 { ChunkData::from_dense(data.clone()) } else { ChunkData::Uniform(Block::AIR) };
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(chunk), true);
    }
    assert!(w.potent_sulfur(at).is_some());
    run(&mut w, 1, 1);
    assert_eq!(w.get_block(at), Some(g::potent(Potent::Dormant)));
}
