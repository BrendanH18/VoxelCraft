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
