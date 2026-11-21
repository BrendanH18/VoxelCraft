use super::*;
use crate::world::skulls::{self, Kind};
use std::collections::HashMap;

/// A sparse block map for pattern tests (unset cells are air).
fn grid(cells: &[(IVec3, Block)]) -> impl Fn(IVec3) -> Option<Block> + '_ {
    let map: HashMap<IVec3, Block> = cells.iter().copied().collect();
    move |p| Some(map.get(&p).copied().unwrap_or(Block::AIR))
}

/// The T of soul sand under three skulls, built along x at `o` (the lower
/// soul sand), standing up.
fn t_shape(o: IVec3, skull: Block) -> Vec<(IVec3, Block)> {
    let mut v = vec![(o, Block::SOUL_SAND)];
    for dx in -1..=1 {
        v.push((o + IVec3::new(dx, 1, 0), crate::world::nether_biome_blocks::SOUL_SOIL));
        v.push((o + IVec3::new(dx, 2, 0), skull));
    }
    v
}

#[test]
fn the_t_of_soul_sand_and_three_skulls_matches_from_any_skull() {
    let o = IVec3::new(10, 64, 10);
    let skull = skulls::floor(Kind::WitherSkeleton, 3);
    let cells = t_shape(o, skull);
    let get = grid(&cells);
    for dx in -1..=1 {
        let m = find_pattern(&get, o + IVec3::new(dx, 2, 0)).expect("a match from each skull");
        // The spawn cell is the lower soul sand, at its centre.
        let (feet, yrot) = spawn_point(&m);
        assert_eq!(feet, o.as_dvec3() + DVec3::new(0.5, 0.55, 0.5));
        assert_eq!(yrot, 90.0, "{m:?}");
        // Every pattern cell is one of ours.
        for x in 0..3 {
            for y in 0..3 {
                let c = m.cell(x, y, 0);
                assert!(
                    cells.iter().any(|&(p, _)| p == c) || c == o + IVec3::new(-1, 0, 0) || c == o + IVec3::new(1, 0, 0)
                );
            }
        }
    }
}

#[test]
fn the_pattern_needs_air_corners_skulls_and_soul_blocks() {
    let o = IVec3::new(0, 64, 0);
    let wither_wall = skulls::wall(Kind::WitherSkeleton, crate::world::block::Facing::North);
    let mut cells = t_shape(o, wither_wall);
    assert!(find_pattern(grid(&cells), o + IVec3::new(0, 2, 0)).is_some(), "wall skulls count");
    // A block in a lower corner spoils it (`~` must be air).
    cells.push((o + IVec3::X, Block::DIRT));
    assert!(find_pattern(grid(&cells), o + IVec3::new(0, 2, 0)).is_none());
    // A skeleton skull doesn't do.
    let mut cells = t_shape(o, skulls::floor(Kind::WitherSkeleton, 0));
    cells.retain(|&(p, _)| p != o + IVec3::new(1, 2, 0));
    cells.push((o + IVec3::new(1, 2, 0), skulls::floor(Kind::Skeleton, 0)));
    assert!(find_pattern(grid(&cells), o + IVec3::new(0, 2, 0)).is_none());
}

#[test]
fn the_pattern_matches_lying_down_and_upside_down() {
    let o = IVec3::new(0, 64, 0);
    let skull = skulls::floor(Kind::WitherSkeleton, 0);
    // Flat on the ground along z: skulls at z+2, the bar at z+1, the stem at z.
    let mut flat = vec![(o, Block::SOUL_SAND)];
    for dx in -1..=1 {
        flat.push((o + IVec3::new(dx, 0, 1), Block::SOUL_SAND));
        flat.push((o + IVec3::new(dx, 0, 2), skull));
    }
    let m = find_pattern(grid(&flat), o + IVec3::new(1, 0, 2)).expect("lying flat");
    assert_eq!(m.cell(1, 2, 0), o);
    assert_eq!(spawn_point(&m).1, 90.0);
    // Upside down along x: the skulls under the bar.
    let mut inverted = vec![(o, Block::SOUL_SAND)];
    for dz in -1..=1 {
        inverted.push((o + IVec3::new(0, -1, dz), Block::SOUL_SAND));
        inverted.push((o + IVec3::new(0, -2, dz), skull));
    }
    let m = find_pattern(grid(&inverted), o + IVec3::new(0, -2, -1)).expect("upside down");
    assert_eq!(m.cell(1, 2, 0), o);
    assert_eq!(spawn_point(&m).1, 0.0, "a pattern across z faces along x");
}

#[test]
fn skulls_accelerate_like_hurting_projectiles() {
    // `(m + m.normalize() * 0.1) * 0.95` from 0.1 a tick tends to 1.9.
    let mut k = Skull::new(DVec3::ZERO, DVec3::X, false, None);
    let ctx = Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        players: vec![],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    };
    let world = crate::physics::test_util::Grid::flat(-100);
    k.tick(&world, &ctx, &[]);
    assert!((k.motion.x - 0.19).abs() < 1e-6, "{}", k.motion.x);
    for _ in 0..200 {
        k.tick(&world, &ctx, &[]);
    }
    assert!((k.motion.x - 1.9).abs() < 1e-3, "{}", k.motion.x);
    // A dangerous skull's 0.73 inertia settles far slower: 0.27.
    let mut d = Skull::new(DVec3::ZERO, DVec3::X, true, None);
    for _ in 0..200 {
        d.tick(&world, &ctx, &[]);
    }
    assert!((d.motion.x - 0.073 / 0.27).abs() < 1e-3, "{}", d.motion.x);
}

#[test]
fn the_wither_is_registered_and_saves_its_charge() {
    assert_eq!(MobKind::from_name("wither"), Some(MobKind::Wither));
    assert_eq!(MobKind::Wither.max_health(), 300.0);
    assert_eq!((MobKind::Wither.shape().half_width, MobKind::Wither.shape().height), (0.45, 3.5));
    assert!(MobKind::Wither.fire_immune() && MobKind::Wither.is_hostile());
    assert!(!MobKind::Wither.spawns_in(crate::world::terrain::Dimension::Overworld));
    assert_eq!(MobKind::Wither.xp(&mut Rng::new(1)), 50);
    assert!(MobKind::Wither.loot().is_empty());
    let mut e = Entities::new(1);
    e.spawn_wither(DVec3::new(0.5, 70.55, 0.5), 90.0);
    // Explosions are refused while it charges too (`hurtServer`).
    assert!(!e.mobs[0].blast(50.0, DVec3::ZERO, &mut Rng::new(1)));
    let m = &e.mobs[0];
    assert_eq!(m.health, 100.0, "a third of its health");
    assert_eq!(m.wither.as_ref().unwrap().invul, INVULNERABLE_TICKS);
    assert_eq!(boss_progress(m), 0.0);
    assert!(m.refuses_hit(5.0), "invulnerable while charging");
    let saved = super::super::nether::save_mob(m);
    let back = super::super::nether::load_mob(&saved).unwrap();
    assert_eq!(back.kind, MobKind::Wither);
    assert_eq!(back.wither.as_ref().unwrap().invul, INVULNERABLE_TICKS);
    assert_eq!(back.health, 100.0);
}

#[test]
fn armor_turns_away_arrows_at_half_health() {
    let mut m = Mob::new(MobKind::Wither, DVec3::ZERO, 0.0);
    let mut rng = Rng::new(3);
    m.health = 200.0;
    m.hit_cause = Some(crate::simulation::damage::Cause::ARROW);
    assert!(!m.refuses_hit(5.0));
    m.health = 150.0;
    assert!(powered(&m));
    assert!(m.refuses_hit(5.0), "arrows bounce off the armor");
    m.hit_cause = None;
    assert!(!m.refuses_hit(5.0));
    // Armor 4 takes a little off an ordinary hit.
    let before = m.health;
    m.damage(10.0, None, &mut rng);
    assert!(before - m.health < 10.0 && before - m.health > 8.0, "{}", before - m.health);
}

#[test]
fn heads_sit_beside_and_above_the_body() {
    let p = DVec3::new(0.0, 64.0, 0.0);
    assert_eq!(head_pos(p, 0.0, 0), DVec3::new(0.0, 67.0, 0.0));
    let left = head_pos(p, 0.0, 1);
    let right = head_pos(p, 0.0, 2);
    assert!((left.x - 1.3).abs() < 1e-6 && (left.y - 66.2).abs() < 1e-9);
    assert!((right.x + 1.3).abs() < 1e-6);
}

// ------------------------------------------------------- v0.16 Track H

fn ctx() -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        players: vec![],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

/// A charged Wither at `at`, simulated, with its id.
fn ready_wither(e: &mut Entities, at: DVec3) -> (usize, u32) {
    e.spawn_wither(at, 0.0);
    let i = e.mobs.len() - 1;
    e.mobs[i].wither.as_mut().unwrap().invul = 0;
    e.mobs[i].health = MAX_HEALTH;
    e.mobs[i].simulation_active = true;
    e.ensure_mob_ids();
    (i, e.mobs[i].uid)
}

fn add(e: &mut Entities, kind: MobKind, at: DVec3) -> u32 {
    let mut m = Mob::new(kind, at, 0.0);
    m.simulation_active = true;
    e.mobs.push(m);
    e.ensure_mob_ids();
    e.mobs.last().unwrap().uid
}

#[test]
fn skulls_in_flight_are_saved_with_dangerous() {
    let mut e = Entities::new(1);
    let mut k = Skull::new(DVec3::new(1.5, 70.0, -3.25), DVec3::X, true, Some(42));
    k.motion = DVec3::new(0.3, -0.1, 0.05);
    e.wither_skulls.push(k);
    e.wither_skulls.push(Skull::new(DVec3::new(0.0, 64.0, 0.0), DVec3::Z, false, None));
    let mut props = std::collections::BTreeMap::new();
    e.save_props(&mut props);
    let mut back = Entities::new(2);
    back.load_props(&props);
    assert_eq!(back.wither_skulls.len(), 2);
    let k2 = back.wither_skulls[0];
    assert_eq!((k2.pos, k2.motion, k2.dangerous, k2.owner), (k.pos, k.motion, true, Some(42)));
    assert!(!back.wither_skulls[1].dangerous && back.wither_skulls[1].owner.is_none());
}

/// `HurtByTargetGoal` turns the Wither on a mob that hurt it, on a goal
/// selection tick (every other tick, by `tickCount + id`); its friends'
/// hits are refused and change nothing.
#[test]
fn mobs_that_hurt_it_become_its_target_on_selection_ticks() {
    let world = crate::physics::test_util::Grid::flat(0);
    let ctx = ctx();
    let mut e = Entities::new(5);
    let (w, wid) = ready_wither(&mut e, DVec3::new(0.5, 8.0, 0.5));
    let pig = add(&mut e, MobKind::Pig, DVec3::new(4.5, 1.0, 0.5));
    let golem = add(&mut e, MobKind::IronGolem, DVec3::new(20.5, 1.0, 0.5));
    let skeleton = add(&mut e, MobKind::Skeleton, DVec3::new(-10.5, 1.0, 0.5));
    let mut events = Vec::new();
    e.wither_tick(0.05, &world, &ctx, &mut events);
    assert_eq!(e.mobs[w].wither.as_ref().unwrap().targets[0], Some(Foe::Mob(pig)), "the nearest first");
    // A friend's hit is refused: no damage, no new target.
    let health = e.mobs[w].health;
    let mut hits = vec![EntityEvent::MobHit { target: wid, attacker: skeleton, damage: 4.0, knockback: DVec3::ZERO }];
    e.resolve_mob_hits(&mut hits);
    assert_eq!(e.mobs[w].health, health);
    let mut hits = vec![EntityEvent::MobHit { target: wid, attacker: golem, damage: 7.0, knockback: DVec3::ZERO }];
    e.resolve_mob_hits(&mut hits);
    assert!(e.mobs[w].health < health);
    let mut switched = None;
    for _ in 0..4 {
        e.wither_tick(0.05, &world, &ctx, &mut events);
        let s = e.mobs[w].wither.as_ref().unwrap();
        if s.targets[0] == Some(Foe::Mob(golem)) {
            switched = Some(s.tick_count);
            break;
        }
        assert_eq!(s.targets[0], Some(Foe::Mob(pig)));
    }
    let tick = switched.expect("the golem becomes the target") as u32;
    assert!(tick.wrapping_add(wid).is_multiple_of(2), "chosen on a selection tick");
    // The hurt goal keeps its target while the pig stays nearer.
    for _ in 0..6 {
        e.wither_tick(0.05, &world, &ctx, &mut events);
    }
    assert_eq!(e.mobs[w].wither.as_ref().unwrap().targets[0], Some(Foe::Mob(golem)));
}

/// `HoverRandomPos`: a spot one to three blocks over solid ground below
/// the Wither (`moveUpToAboveSolid`).
#[test]
fn idle_flight_hovers_over_the_ground() {
    let world = crate::physics::test_util::Grid::flat(0);
    let ctx = ctx();
    let m = Mob::new(MobKind::Wither, DVec3::new(0.5, 4.0, 0.5), 0.0);
    let mut rng = Rng::new(9);
    let mut hovered = 0;
    for _ in 0..40 {
        let Some(p) = m.hover_spot(&world, &ctx, &mut rng) else { continue };
        assert!(p.y >= 1, "{p}");
        hovered += usize::from((2..=4).contains(&p.y));
        assert!(p.x.abs() <= 9 && p.z.abs() <= 9, "{p}");
    }
    assert!(hovered >= 30, "{hovered} of 40 hover over the ground");
}

#[test]
fn goal_numbers_match_the_jar() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/wave1_leftovers.json")).unwrap();
    let w = &f["wither"];
    assert_eq!(w["hover_random_pos"][0], HOVER.0);
    assert_eq!(w["hover_random_pos"][1], HOVER.1);
    assert_eq!(w["hover_random_pos"][3], HOVER.2);
    assert_eq!(w["hover_random_pos"][4], HOVER.3);
    assert_eq!(w["air_and_water_random_pos"][0], AIR_AND_WATER.0);
    assert_eq!(w["air_and_water_random_pos"][1], AIR_AND_WATER.1);
    assert_eq!(w["air_and_water_random_pos"][2], AIR_AND_WATER.2);
    let interval = w["random_stroll_interval"].as_u64().unwrap() as u32;
    let divisor = w["reduced_tick_delay_divisor"].as_u64().unwrap() as u32;
    assert_eq!(interval.div_ceil(divisor), STROLL_INTERVAL);
    assert_eq!(w["selector_cadence"], 2);
    assert_eq!(w["skull_patterns"]["base"], serde_json::json!(["###", "~#~"]));
    assert_eq!(w["can_spawn_mob_min_y_offset"], 2);
    let goals: Vec<(String, i64)> = w["goals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["goal"].as_str().unwrap().to_string(), g["priority"].as_i64().unwrap()))
        .collect();
    assert!(goals.contains(&("HurtByTargetGoal".into(), 1)));
    assert!(goals.contains(&("NearestAttackableTargetGoal".into(), 2)));
    assert!(goals.contains(&("RangedAttackGoal".into(), 2)));
    assert!(goals.contains(&("WaterAvoidingRandomFlyingGoal".into(), 5)));
}

/// `WitherSkullBlock.canSpawnMob`'s base: the T and its air corners, with
/// the skull row free.
#[test]
fn the_base_pattern_needs_only_the_t() {
    let o = IVec3::new(0, 64, 0);
    let mut cells = t_shape(o, Block::AIR);
    cells.retain(|&(_, b)| b != Block::AIR);
    let get = grid(&cells);
    assert!(find_base(&get, o + IVec3::new(1, 2, 0)).is_some());
    assert!(can_spawn_mob(&get, o + IVec3::new(1, 2, 0), -64, false));
    assert!(!can_spawn_mob(&get, o + IVec3::new(1, 2, 0), -64, true), "never in Peaceful");
    assert!(!can_spawn_mob(&get, o + IVec3::new(1, 2, 0), 65, false), "two above the bottom");
    assert!(find_base(grid(&[]), o).is_none());
}
