use super::*;
use crate::world::{
    block::Facing,
    chunk::ChunkData,
    overworld_blocks::AMETHYST_BLOCK,
    terrain::Generator,
    vibration::{EventSource, GameEvent},
};
use std::sync::Arc;

const P: IVec3 = IVec3::new(16, 140, 16);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.insert_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
    }
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, 139, z), Block::STONE);
        }
    }
    w
}

fn tick(w: &mut World, n: u32) {
    for _ in 0..n {
        w.tick_redstone();
        w.tick_sculk();
    }
}

fn center(p: IVec3) -> DVec3 {
    p.as_dvec3() + DVec3::splat(0.5)
}

fn player() -> EventSource {
    EventSource::player(PlayerId(3), false)
}

fn phase(w: &World, p: IVec3) -> Phase {
    cb::any_sensor(w.get_block(p).unwrap()).unwrap().0
}

#[test]
fn a_sensor_hears_a_placed_block_and_powers_by_distance() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    let at = P + IVec3::new(3, 0, 0);
    w.emit_game_event(center(at), GameEvent::BlockPlace, player(), Some(Block::STONE));
    // Chosen on the next tick, arriving after three ticks of travel (floor(3.0)).
    tick(&mut w, 2);
    assert_eq!(phase(&w, P), Phase::Inactive);
    tick(&mut w, 1);
    assert_eq!(phase(&w, P), Phase::Active);
    let e = w.sculk_entity(P).unwrap();
    // `15 - floor(15 / 8 * 3)`.
    assert_eq!(e.power, 10);
    assert_eq!(e.last_frequency, 13);
    assert_eq!(w.container_signal(P), Some(13));
    assert_eq!(w.redstone_power(P + IVec3::Z), 10, "weak power to the side");
    tick(&mut w, SENSOR_ACTIVE_TICKS as u32);
    assert_eq!(phase(&w, P), Phase::Cooldown);
    assert_eq!(w.sculk_entity(P).unwrap().power, 0);
    assert_eq!(w.container_signal(P), Some(0));
    tick(&mut w, SENSOR_COOLDOWN_TICKS as u32);
    assert_eq!(phase(&w, P), Phase::Inactive);
}

#[test]
fn wool_occludes_and_dampens_and_sneaking_steps_are_silent() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    // A wool wall between the sensor and the event.
    for y in -3..=3 {
        for z in -3..=3 {
            w.set_block(P + IVec3::new(2, y, z), Block::WOOL);
        }
    }
    w.emit_game_event(center(P + IVec3::new(4, 0, 0)), GameEvent::BlockPlace, player(), Some(Block::STONE));
    tick(&mut w, 10);
    assert_eq!(phase(&w, P), Phase::Inactive, "occluded by wool");
    // Breaking wool itself is dampened.
    w.emit_game_event(center(P + IVec3::new(0, 0, 3)), GameEvent::BlockDestroy, player(), Some(Block::WOOL));
    tick(&mut w, 10);
    assert_eq!(phase(&w, P), Phase::Inactive, "dampened by wool");
    let sneaking = EventSource::player(PlayerId(3), true);
    w.emit_game_event(center(P + IVec3::new(0, 0, 3)), GameEvent::Step, sneaking, Some(Block::STONE));
    tick(&mut w, 10);
    assert_eq!(phase(&w, P), Phase::Inactive, "sneaking steps make no vibration");
    w.emit_game_event(center(P + IVec3::new(0, 0, 3)), GameEvent::Step, player(), Some(Block::STONE));
    tick(&mut w, 5);
    assert_eq!(phase(&w, P), Phase::Active);
    assert_eq!(w.sculk_entity(P).unwrap().last_frequency, 1);
}

#[test]
fn beyond_the_radius_and_its_own_block_events_are_ignored() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    w.emit_game_event(center(P + IVec3::new(9, 0, 0)), GameEvent::Explode, player(), None);
    w.emit_game_event(center(P), GameEvent::BlockPlace, player(), Some(cb::SCULK_SENSOR));
    tick(&mut w, 15);
    assert_eq!(phase(&w, P), Phase::Inactive);
}

#[test]
fn the_nearest_candidate_of_a_tick_wins() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    w.emit_game_event(center(P + IVec3::new(6, 0, 0)), GameEvent::Explode, player(), None);
    w.emit_game_event(center(P + IVec3::new(2, 0, 0)), GameEvent::Step, player(), Some(Block::STONE));
    tick(&mut w, 4);
    assert_eq!(phase(&w, P), Phase::Active);
    assert_eq!(w.sculk_entity(P).unwrap().last_frequency, 1);
}

#[test]
fn calibrated_sensors_filter_by_their_input_and_hear_further() {
    let mut w = world();
    let s = cb::calibrated_sensor(Facing::North, Phase::Inactive, false);
    w.set_block(P, s);
    // The input is the side opposite the facing: south of a north-facing sensor.
    w.set_block(P + IVec3::Z, crate::world::redstone_blocks::REDSTONE_BLOCK);
    assert_eq!(w.calibrated_input(P, s), 15);
    w.emit_game_event(center(P + IVec3::new(12, 0, 0)), GameEvent::BlockPlace, player(), Some(Block::STONE));
    tick(&mut w, 20);
    assert_eq!(phase(&w, P), Phase::Inactive, "frequency 13 filtered out by input 15");
    w.emit_game_event(center(P + IVec3::new(12, 0, 0)), GameEvent::Explode, player(), None);
    tick(&mut w, 14);
    assert_eq!(phase(&w, P), Phase::Active, "frequency 15 passes, 12 blocks out");
    let power = w.sculk_entity(P).unwrap().power;
    assert_eq!(power, redstone_strength(12.0, 16));
    assert_eq!(w.redstone_power(P + IVec3::X), power);
    // Calibrated sensors stay active 10 ticks.
    tick(&mut w, CALIBRATED_ACTIVE_TICKS as u32);
    assert_eq!(phase(&w, P), Phase::Cooldown);
}

#[test]
fn amethyst_resonates_the_frequency_to_a_second_sensor() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    w.set_block(P + IVec3::X, AMETHYST_BLOCK);
    let far = P + IVec3::new(8, 0, 0);
    w.set_block(far, cb::SCULK_SENSOR);
    w.emit_game_event(center(P - IVec3::new(2, 0, 0)), GameEvent::EntityDamage, player(), None);
    tick(&mut w, 4);
    assert_eq!(phase(&w, P), Phase::Active);
    assert!(w.take_sculk_sounds().iter().any(|(_, s)| matches!(s, SculkSound::Resonate(_))));
    tick(&mut w, 10);
    assert_eq!(phase(&w, far), Phase::Active);
    assert_eq!(w.sculk_entity(far).unwrap().last_frequency, 7, "resonate_7 carries entity_damage's frequency");
}

#[test]
fn a_shrieker_that_cannot_summon_shrieks_for_a_player_and_answers_nothing() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    let shrieker = P + IVec3::new(3, 0, 0);
    w.set_block(shrieker, cb::sculk_shrieker(false, false, false));
    w.emit_game_event(center(P + IVec3::new(0, 0, 2)), GameEvent::BlockPlace, player(), Some(Block::STONE));
    tick(&mut w, 3);
    assert_eq!(phase(&w, P), Phase::Active);
    // The tendril clicks reach the shrieker three blocks away.
    tick(&mut w, 5);
    assert_eq!(cb::shrieker_of(w.get_block(shrieker).unwrap()), Some((true, false, false)));
    tick(&mut w, SHRIEKING_TICKS as u32);
    assert_eq!(cb::shrieker_of(w.get_block(shrieker).unwrap()), Some((false, false, false)));
    assert!(w.take_shrieker_responses().is_empty());
}

#[test]
fn summoning_shriekers_raise_warning_levels_with_cooldowns_and_answer_at_four() {
    let mut w = world();
    let s = P + IVec3::new(0, 0, 0);
    w.set_block(s, cb::sculk_shrieker(false, true, false));
    let mut tracker = WardenSpawnTracker::default();
    let mut other = WardenSpawnTracker { warning_level: 2, ..Default::default() };
    let shriek = |w: &mut World, t: &mut WardenSpawnTracker, o: &mut WardenSpawnTracker| {
        w.sculk_step_on(s, center(s) + DVec3::Y * 0.5, player(), false);
        let mut players = [
            ShriekPlayer {
                id: PlayerId(3),
                pos: center(s),
                alive: true,
                spectator: false,
                untargetable: false,
                tracker: t,
            },
            ShriekPlayer {
                id: PlayerId(4),
                pos: center(s) + DVec3::X * 5.0,
                alive: true,
                spectator: false,
                untargetable: false,
                tracker: o,
            },
        ];
        w.resolve_shrieks(&mut players, |_| false);
    };
    shriek(&mut w, &mut tracker, &mut other);
    // The highest level among nearby players rises and both take it.
    assert_eq!((tracker.warning_level, other.warning_level), (3, 3));
    assert!(cb::shrieker_of(w.get_block(s).unwrap()).unwrap().0, "shrieking");
    tick(&mut w, SHRIEKING_TICKS as u32);
    assert_eq!(w.take_shrieker_responses(), vec![ShriekerResponse { pos: s, warning: 3 }]);
    // On cooldown: no warning, no shriek.
    shriek(&mut w, &mut tracker, &mut other);
    assert!(!cb::shrieker_of(w.get_block(s).unwrap()).unwrap().0);
    for _ in 0..200 {
        tracker.tick();
        other.tick();
    }
    shriek(&mut w, &mut tracker, &mut other);
    assert_eq!(tracker.warning_level, 4);
    tick(&mut w, SHRIEKING_TICKS as u32);
    assert_eq!(w.take_shrieker_responses(), vec![ShriekerResponse { pos: s, warning: 4 }]);
    // A warden nearby stops warnings.
    for _ in 0..200 {
        tracker.tick();
    }
    let before = tracker;
    w.sculk_step_on(s, center(s), player(), false);
    let mut players = [ShriekPlayer {
        id: PlayerId(3),
        pos: center(s),
        alive: true,
        spectator: false,
        untargetable: false,
        tracker: &mut tracker,
    }];
    w.resolve_shrieks(&mut players, |_| true);
    assert_eq!(tracker, before);
    // Peaceful or the gamerule off: summoning shriekers shriek without warnings.
    w.rules.spawn_wardens = false;
    tick(&mut w, 5);
    w.sculk_step_on(s, center(s), player(), false);
    assert!(cb::shrieker_of(w.get_block(s).unwrap()).unwrap().0);
}

#[test]
fn warning_levels_decay_every_twelve_thousand_ticks() {
    let mut t = WardenSpawnTracker { warning_level: 3, ..Default::default() };
    for _ in 0..12000 {
        t.tick();
    }
    assert_eq!(t.warning_level, 3);
    t.tick();
    assert_eq!(t.warning_level, 2);
    let back = WardenSpawnTracker::load(&t.save());
    assert_eq!(back, t);
}

#[test]
fn a_catalyst_takes_a_deaths_experience_blooms_and_spreads_sculk() {
    let mut w = world();
    let catalyst = IVec3::new(16, 139, 16);
    w.set_block(catalyst, cb::SCULK_CATALYST);
    assert!(w.sculk_catalyst_near(center(catalyst) + DVec3::X * 6.0));
    assert!(!w.sculk_catalyst_near(center(catalyst) + DVec3::X * 12.0));
    let death = DVec3::new(22.5, 140.0, 16.5);
    assert!(w.sculk_catalyst_death(death, 30));
    assert_eq!(w.get_block(catalyst), Some(cb::sculk_catalyst(true)));
    assert!(w.take_sculk_sounds().iter().any(|(_, s)| *s == SculkSound::Bloom));
    assert_eq!(w.sculk_entity(catalyst).unwrap().cursors.iter().map(|c| c.charge).sum::<i32>(), 30);
    tick(&mut w, BLOOM_TICKS as u32);
    assert_eq!(w.get_block(catalyst), Some(cb::sculk_catalyst(false)));
    let count = |w: &World| {
        let mut n = 0;
        for x in 0..32 {
            for z in 0..32 {
                n += (w.get_block(IVec3::new(x, 139, z)) == Some(cb::SCULK)) as u32;
            }
        }
        n
    };
    tick(&mut w, 400);
    assert!(count(&w) > 0, "charge turned stone into sculk");
    // Far deaths go unheard.
    assert!(!w.sculk_catalyst_death(DVec3::new(30.5, 140.0, 30.5), 5));
}

#[test]
fn listener_state_saves_and_reloads() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    let shrieker = P + IVec3::new(4, 0, 0);
    w.set_block(shrieker, cb::sculk_shrieker(false, true, false));
    w.emit_game_event(center(P + IVec3::new(0, 0, 5)), GameEvent::Eat, player(), None);
    tick(&mut w, 2);
    w.sculk_entity_mut(shrieker).warning = 2;
    let catalyst = P + IVec3::new(-5, -1, 0);
    w.set_block(catalyst, cb::SCULK_CATALYST);
    w.sculk_catalyst_death(center(catalyst) + DVec3::Y, 12);
    let text = w.sculk_to_string();
    let mut other = world();
    other.load_sculk(&text);
    let saved = w.sculk_entity(P).unwrap().vibration.clone();
    assert!(saved.current.is_some(), "a vibration in flight");
    assert_eq!(other.sculk.entities[&P.to_array()].vibration, saved);
    assert_eq!(other.sculk.entities[&shrieker.to_array()].warning, 2);
    assert_eq!(other.sculk.entities[&catalyst.to_array()].cursors.len(), 1);
    assert_eq!(SculkEntity::load(&w.sculk_entity(P).unwrap().save()).vibration, saved);
}

#[test]
fn walking_players_step_on_sensors_and_shriekers_unless_sneaking() {
    let mut w = world();
    w.set_block(P, cb::SCULK_SENSOR);
    let mut p = crate::player::Player::new(DVec3::new(20.5, 140.0, 16.5));
    p.on_ground = true;
    // Walking four blocks posts steps (every 1/0.6 blocks); sneaking ones are silent.
    p.sneaking = true;
    for _ in 0..16 {
        let before = p.pos;
        p.pos.x += 0.25;
        w.player_moved(PlayerId(3), &p, before, false, false);
    }
    tick(&mut w, 10);
    assert_eq!(phase(&w, P), Phase::Inactive);
    p.sneaking = false;
    for _ in 0..16 {
        let before = p.pos;
        p.pos.x -= 0.25;
        w.player_moved(PlayerId(3), &p, before, false, false);
    }
    tick(&mut w, 10);
    assert_eq!(phase(&w, P), Phase::Active);
    // Standing on a sensor triggers it even while sneaking (`stepOn`).
    tick(&mut w, 45);
    assert_eq!(phase(&w, P), Phase::Inactive);
    p.sneaking = true;
    p.pos = DVec3::new(16.5, 140.5, 16.5);
    w.player_moved(PlayerId(3), &p, p.pos, false, false);
    tick(&mut w, 3);
    assert_eq!(phase(&w, P), Phase::Active);
    // Spectators make no events.
    tick(&mut w, 45);
    let s = P + IVec3::new(0, 0, 4);
    w.set_block(s, cb::sculk_shrieker(false, false, false));
    p.pos = DVec3::new(16.5, 140.5, 20.5);
    w.player_moved(PlayerId(3), &p, p.pos, true, false);
    assert!(!cb::shrieker_of(w.get_block(s).unwrap()).unwrap().0);
    w.player_moved(PlayerId(3), &p, p.pos, false, false);
    assert!(cb::shrieker_of(w.get_block(s).unwrap()).unwrap().0, "a player on a shrieker");
}
