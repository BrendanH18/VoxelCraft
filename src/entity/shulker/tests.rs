//! The shulker against `tests/fixtures/java/26.3/end_cities.json`.
use super::*;
use crate::{
    entity::{Target, mob::goals},
    physics::test_util::Grid,
    simulation::{effects::Effect, lighting::SpawnLight},
    world::terrain::Dimension,
};
use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/end_cities.json")).unwrap()
}

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        players,
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::End,
    }
}

/// A shulker on the floor of a flat grid (floor at y 10).
fn shulker(e: &mut Entities, x: f64, z: f64) -> usize {
    e.mobs.push(Mob::new(MobKind::Shulker, DVec3::new(x, 10.0, z), 0.0));
    e.mobs.last_mut().unwrap().persistent = true;
    e.ensure_mob_ids();
    e.mobs.len() - 1
}

fn run(e: &mut Entities, world: &Grid, c: &Ctx, ticks: u32) -> Vec<EntityEvent> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        out.extend(e.update(0.05, world, c));
    }
    out
}

#[test]
fn registration_constants_and_loot_match_java() {
    let f = facts();
    let c = &f["constants"]["shulker"];
    assert_eq!(f64::from(MAX_HEALTH), f["shulker"]["max_health"].as_f64().unwrap());
    assert_eq!(f64::from(COVERED_ARMOR), f["shulker"]["covered_armor"].as_f64().unwrap());
    assert!((f64::from(PEEK_PER_TICK) - c["PEEK_PER_TICK"].as_f64().unwrap()).abs() < 1e-6);
    assert_eq!(i64::from(MAX_TELEPORT_DISTANCE), c["MAX_TELEPORT_DISTANCE"].as_i64().unwrap());
    assert_eq!(OTHER_SHULKER_SCAN_RADIUS, c["OTHER_SHULKER_SCAN_RADIUS"].as_f64().unwrap());
    assert_eq!(f64::from(OTHER_SHULKER_LIMIT), c["OTHER_SHULKER_LIMIT"].as_f64().unwrap());
    assert_eq!(BULLET_SPEED, f["constants"]["shulker_bullet"]["SPEED"].as_f64().unwrap());
    assert_eq!(f64::from(BULLET_DAMAGE), f["shulker_bullet_hit"]["damage"].as_f64().unwrap());
    assert_eq!(u64::from(LEVITATION_TICKS), f["shulker_bullet_hit"]["levitation_ticks"].as_u64().unwrap());
    let kind = MobKind::from_name("minecraft:shulker").unwrap();
    assert_eq!(kind, MobKind::Shulker);
    assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
    assert_eq!(kind.max_health(), MAX_HEALTH);
    assert!(kind.is_hostile() && !kind.spawns_in(Dimension::End) && !kind.spawns_in(Dimension::Overworld));
    let mut rng = super::super::Rng::new(3);
    assert_eq!(kind.xp(&mut rng), EXPERIENCE);
    let _ = goals::movement_speed;
    // `entities/shulker`: one shell, 0.5 unenchanted.
    let pool = &f["loot_tables"]["entities/shulker"]["pools"][0];
    assert_eq!(pool["entries"][0]["name"], "shulker_shell");
    assert_eq!(pool["condition"]["unenchanted_chance"], 0.5);
    let mut e = Entities::new(9);
    let mut shells = 0;
    for _ in 0..4000 {
        e.items.clear();
        e.drop_loot_killed(MobKind::Shulker, DVec3::ZERO, 0, false);
        shells += e.items.iter().filter(|i| i.stack.item == crate::content::chorus::SHULKER_SHELL).count();
    }
    assert!((1800..2200).contains(&shells), "{shells} of 4000");
}

#[test]
fn a_shut_shulker_is_armored_and_shrugs_off_arrows() {
    let mut m = Mob::new(MobKind::Shulker, DVec3::new(0.5, 10.0, 0.5), 0.0);
    assert!(m.shulker().unwrap().closed());
    let mut rng = super::super::Rng::new(1);
    assert!(!m.damage_projectile(6.0, &[], &mut rng));
    assert_eq!(m.health, MAX_HEALTH);
    // Twenty armor points (`CombatRules`: 20 - 6 / 2 = 17 count) take 6
    // down to 6 * (1 - 17/25).
    m.damage(6.0, None, &mut rng);
    assert!((m.health - (MAX_HEALTH - 6.0 * (1.0 - 17.0 / 25.0))).abs() < 1e-4, "{}", m.health);
    m.shulker.as_mut().unwrap().peek = 100;
    let before = m.health;
    // v1.0 gap Track 8d: a later hit, past the first one's hurt cooldown.
    m.damage_cooldown = 0;
    m.damage(6.0, None, &mut rng);
    assert!((before - m.health - 6.0).abs() < 1e-4);
}

#[test]
fn it_clings_peeks_and_saves() {
    let world = Grid::flat(10);
    let mut e = Entities::new(4);
    let i = shulker(&mut e, 3.3, 7.9);
    let c = ctx(vec![]);
    run(&mut e, &world, &c, 400);
    let m = &e.mobs[i];
    assert_eq!(m.pos, DVec3::new(3.5, 10.0, 7.5), "snapped to its block");
    assert_eq!(m.shulker().unwrap().attach, Dir::Down);
    // Without a target it peeks now and then.
    let mut peeked = false;
    for _ in 0..800 {
        e.update(0.05, &world, &c);
        peeked |= e.mobs[i].shulker().unwrap().peek == 30;
    }
    assert!(peeked);
    // Save and reload.
    e.mobs[i].shulker.as_mut().unwrap().color = Some(DyeColor::Lime);
    e.mobs[i].shulker.as_mut().unwrap().attach = Dir::North;
    let saved = crate::entity::nether::save_mob(&e.mobs[i]);
    let back = crate::entity::nether::load_mob(&saved).unwrap();
    let s = back.shulker().unwrap();
    assert_eq!((s.attach, s.color), (Dir::North, Some(DyeColor::Lime)));
    // And its LAN look.
    let mut copy = Mob::new(MobKind::Shulker, DVec3::ZERO, 0.0);
    copy.apply_look(&e.mobs[i].look());
    assert_eq!(copy.shulker().unwrap().color, Some(DyeColor::Lime));
    assert_eq!(copy.shulker().unwrap().attach, Dir::North);
}

#[test]
fn it_finds_a_new_face_or_teleports_when_its_support_goes() {
    let mut world = Grid::flat(0);
    // Floating, with a wall to the east.
    world.set(IVec3::new(4, 20, 3), Block::STONE);
    let mut e = Entities::new(4);
    e.mobs.push(Mob::new(MobKind::Shulker, DVec3::new(3.5, 20.0, 3.5), 0.0));
    e.mobs[0].persistent = true;
    e.ensure_mob_ids();
    run(&mut e, &world, &ctx(vec![]), 2);
    assert_eq!(e.mobs[0].shulker().unwrap().attach, Dir::East);
    // Nothing to cling to but the floor up to eight blocks below: it
    // teleports onto it.
    let world = Grid::flat(24);
    let mut e = Entities::new(5);
    e.mobs.push(Mob::new(MobKind::Shulker, DVec3::new(0.5, 30.0, 0.5), 0.0));
    e.mobs[0].persistent = true;
    e.ensure_mob_ids();
    run(&mut e, &world, &ctx(vec![]), 40);
    let at = cell(e.mobs[0].pos);
    assert!(can_stay_at(&world, at, e.mobs[0].shulker().unwrap().attach), "{at}");
}

#[test]
fn it_shoots_homing_bullets_that_levitate_players() {
    let world = Grid::flat(10);
    let mut e = Entities::new(7);
    let i = shulker(&mut e, 0.5, 0.5);
    let player = Target::new(PlayerId::HOST, DVec3::new(6.5, 10.0, 0.5), true);
    let c = ctx(vec![player]);
    let mut events = Vec::new();
    let mut shot = false;
    for _ in 0..600 {
        let ev = e.update(0.05, &world, &c);
        shot |= !e.shulker_bullets.is_empty();
        events.extend(ev);
        if events.iter().any(|ev| matches!(ev, EntityEvent::PlayerEffect { effect: Effect::Levitation, .. })) {
            break;
        }
    }
    assert!(shot, "it opened and fired");
    assert_eq!(e.mobs[i].shulker().unwrap().target, Some(PlayerId::HOST));
    assert!(
        events.iter().any(|ev| matches!(
            ev,
            EntityEvent::PlayerEffect { effect: Effect::Levitation, ticks: LEVITATION_TICKS, .. }
        ))
    );
    assert!(events.iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { damage, .. } if *damage == BULLET_DAMAGE)));
    // A creative player is never targeted.
    let mut e = Entities::new(7);
    shulker(&mut e, 0.5, 0.5);
    let c = ctx(vec![Target::new(PlayerId::HOST, DVec3::new(6.5, 10.0, 0.5), false)]);
    run(&mut e, &world, &c, 200);
    assert!(e.shulker_bullets.is_empty());
}

#[test]
fn an_open_shulker_hit_by_a_bullet_duplicates() {
    let world = Grid::flat(10);
    let mut spawned = 0;
    for seed in 0..10 {
        let mut e = Entities::new(seed);
        let i = shulker(&mut e, 0.5, 0.5);
        e.mobs[i].shulker.as_mut().unwrap().peek = 100;
        e.mobs[i].health = MAX_HEALTH;
        let b = Bullet::replica(DVec3::new(0.5, 10.5, 0.5), DVec3::new(0.5, 10.5, 0.5));
        e.bullet_hit(&b, BulletHit::Mob(i), &mut Vec::new());
        assert!(e.mobs[i].effects.has(Effect::Levitation));
        let mut ev = Vec::new();
        e.shulker_tick(i, &world, &ctx(vec![]), &mut ev);
        spawned += e.mobs.iter().filter(|m| m.kind == MobKind::Shulker).count() - 1;
    }
    // Alone, a shulker that teleports always leaves a new one (failure 0).
    assert!(spawned > 0);
}
