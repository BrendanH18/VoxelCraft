//! The endermite against `tests/fixtures/java/26.3/small_mobs.json`.
use super::*;
use crate::{
    enchant::{Creature, Enchantment, Enchants},
    entity::{PlayerId, Target, category::MobCategory, mob::goals, path},
    inventory::Stack,
    item::{Item, ToolKind},
    physics::test_util::Grid,
    simulation::{difficulty::Difficulty, effects::Effect, lighting::SpawnLight},
    world::{block::Block, terrain::Dimension},
};
use glam::IVec3;
use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/small_mobs.json")).unwrap()
}

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::DAY,
        players,
        // Night: endermen in daylight wander off by teleporting.
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

/// A player that mobs ignore (creative), standing at `pos`.
fn bystander(pos: DVec3) -> Target {
    Target::new(PlayerId::HOST, pos, false)
}

fn mite(e: &mut Entities, pos: DVec3) -> usize {
    e.mobs.push(Mob::new(MobKind::Endermite, pos, 0.0));
    e.mobs.last_mut().unwrap().on_ground = true;
    e.ensure_mob_ids();
    e.mobs.len() - 1
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn endermite_registration_attributes_and_loot_match_java() {
    let facts = facts();
    let m = &facts["endermite"];
    let kind = MobKind::from_name(m["id"].as_str().unwrap()).expect("endermite is registered");
    assert_eq!(kind, MobKind::Endermite);
    assert_eq!(MobKind::from_name("minecraft:endermite"), Some(kind));
    assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
    let t = &m["entity_type"];
    assert_eq!(kind.shape().half_width * 2.0, f(&t["width"]));
    assert!((kind.shape().height - f(&t["height"])).abs() < 1e-6);
    assert!((EYE_HEIGHT - f(&t["eye_height"])).abs() < 1e-6);
    assert_eq!(t["category"], "monster");
    assert_eq!(kind.category(), MobCategory::Monster);
    assert!(kind.is_hostile());
    assert_eq!(kind.despawns_in_peaceful(), !t["allowed_in_peaceful"].as_bool().unwrap());
    let a = &m["attributes"];
    assert_eq!(f64::from(kind.max_health()), f(&a["max_health"]));
    assert_eq!(f64::from(MAX_HEALTH), f(&a["max_health"]));
    assert_eq!(goals::movement_speed(kind), f(&a["movement_speed"]));
    assert_eq!(f64::from(kind.melee().0), f(&a["attack_damage"]));
    assert_eq!(FOLLOW_RANGE, f(&a["follow_range"]));
    assert_eq!(kind.nav_caps(0.6, 1.8).max_path_length, f(&a["follow_range"]) as f32);
    let mut rng = super::super::Rng::new(5);
    for _ in 0..20 {
        assert_eq!(u64::from(kind.xp(&mut rng)), m["experience"].as_u64().unwrap());
    }
    // `entities/endermite` has no pools: nothing drops, with or without Looting.
    assert_eq!(m["loot_table"]["pools"], 0);
    assert!(kind.loot().is_empty());
    for looting in 0..4 {
        assert!(kind.drops(&mut rng, looting).is_empty());
    }
    assert_eq!(i64::from(MAX_LIFE), m["max_life"].as_i64().unwrap());
    assert_eq!(f64::from(PEARL_CHANCE), f(&facts["pearl_endermite"]["chance"]) as f32 as f64);
    assert_eq!(u64::from(PORTAL_PARTICLES), m["portal_particles_per_tick"].as_u64().unwrap());
    let hunt = &facts["enderman_targets_endermites"];
    assert_eq!(ENDERMAN_FOLLOW_RANGE, f(&hunt["follow_range"]));
    assert_eq!(f64::from(ENDERMAN_DAMAGE), f(&hunt["attack_damage"]));
    assert_eq!(f64::from(MobKind::Enderman.melee().0), f(&hunt["attack_damage"]));
    assert!(hunt["must_see"].as_bool().unwrap() && !hunt["must_reach"].as_bool().unwrap());
}

#[test]
fn endermite_goal_speeds_follow_the_java_goal_list() {
    let facts = facts();
    let goal = |name: &str| {
        facts["endermite"]["goals"].as_array().unwrap().iter().find(|g| g["goal"] == name).cloned().unwrap()
    };
    let speed = f(&facts["endermite"]["attributes"]["movement_speed"]);
    let m = Mob::new(MobKind::Endermite, DVec3::ZERO, 0.0);
    // `WaterAvoidingRandomStrollGoal(1.0)` and `MeleeAttackGoal(1.0, false)`.
    let stroll = f(&goal("WaterAvoidingRandomStrollGoal")["args"][0]);
    assert!((m.stroll_speed() - path::ground_speed(speed * stroll)).abs() < 1e-9);
    let melee = goal("MeleeAttackGoal");
    assert_eq!(melee["args"][1], 0, "doesn't follow unseen targets");
    assert!((m.goal_speed(f(&melee["args"][0])) - path::ground_speed(speed)).abs() < 1e-9);
    // `FloatGoal`: it swims at the surface.
    assert_eq!(goal("FloatGoal")["priority"], 1);
    assert!(MobKind::Endermite.floats());
    assert!(goal("HurtByTargetGoal")["alert_others"].as_bool().unwrap());
    assert_eq!(goal("NearestAttackableTargetGoal")["args"], serde_json::json!(["class:Player", 1]));
}

#[test]
fn bane_of_arthropods_covers_exactly_the_java_tag_with_its_damage_and_slowness() {
    let facts = facts();
    let tag: Vec<&str> = facts["entity_type_tags"]["sensitive_to_bane_of_arthropods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for kind in MobKind::ALL {
        let java_id = kind.name().replace(' ', "_");
        assert_eq!(
            tag.contains(&java_id.as_str()),
            kind.creature() == Creature::Arthropod,
            "{java_id} and #sensitive_to_bane_of_arthropods"
        );
    }
    for id in &tag {
        assert!(MobKind::from_name(id).is_some(), "{id} is a mob kind");
    }
    let bane = &facts["bane_of_arthropods"];
    for level in 1..=bane["max_level"].as_u64().unwrap() as u8 {
        let held = Enchants::NONE.with(Enchantment::BaneOfArthropods, level);
        let expected = f(&bane["damage_base"]) + f(&bane["damage_per_level_above_first"]) * f64::from(level - 1);
        assert_eq!(f64::from(crate::enchant::damage_bonus(held, Creature::Arthropod)), expected);
        assert_eq!(crate::enchant::damage_bonus(held, Creature::Other), 0.0);
    }
    // A Bane of Arthropods V hit: 2.5 * 5 more damage than a plain sword,
    // and Slowness IV for 1.5 to 3.5 seconds, spread over that range.
    let s = &bane["slowness"];
    assert_eq!(s["effect"], "slowness");
    let amplifier = f(&s["min_amplifier"]) as u8;
    assert_eq!(f(&s["max_amplifier"]) as u8, amplifier);
    let (lo, hi) = (
        f(&s["min_duration_seconds"]),
        f(&s["max_duration_base_seconds"]) + 4.0 * f(&s["max_duration_per_level_above_first"]),
    );
    let sword = Item::tool(ToolKind::Sword, crate::item::Tier::Diamond);
    let bane5 = Stack { enchants: Enchants::NONE.with(Enchantment::BaneOfArthropods, 5), ..Stack::new(sword, 1) };
    let (mut shortest, mut longest) = (u32::MAX, 0);
    for seed in 0..200 {
        let mut e = Entities::new(seed);
        let i = mite(&mut e, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[i].health = 1000.0;
        e.melee(i, DVec3::X, Some(Stack::new(sword, 1)), 0.0, false, None);
        let plain = 1000.0 - e.mobs[i].health;
        assert!(e.mobs[i].effects.get(Effect::Slowness).is_none(), "no slowness without the enchantment");
        e.mobs[i].health = 1000.0;
        e.mobs[i].hurt = 0.0;
        e.mobs[i].effects = Default::default();
        e.melee(i, DVec3::X, Some(bane5), 0.0, false, None);
        let enchanted = 1000.0 - e.mobs[i].health;
        assert!((enchanted - plain - 12.5).abs() < 1e-3, "{enchanted} vs {plain}");
        let slow = e.mobs[i].effects.get(Effect::Slowness).expect("slowed");
        assert_eq!(slow.amplifier, amplifier);
        assert!((lo * 20.0..=hi * 20.0).contains(&f64::from(slow.ticks)), "{} ticks", slow.ticks);
        shortest = shortest.min(slow.ticks);
        longest = longest.max(slow.ticks);
    }
    assert!(shortest <= 35 && longest >= 65, "durations {shortest}..{longest}");
    // Other mobs aren't slowed.
    let mut e = Entities::new(3);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5));
    e.melee(0, DVec3::X, Some(bane5), 0.0, false, None);
    assert!(e.mobs[0].effects.get(Effect::Slowness).is_none());
}

#[test]
fn one_landed_pearl_in_twenty_leaves_an_endermite_where_the_thrower_stood() {
    let thrower = {
        let mut t = Target::new(PlayerId::HOST, DVec3::new(3.5, 10.0, -2.5), true);
        t.look = DVec3::new(0.0, 0.0, 1.0);
        t
    };
    let landed = [EntityEvent::PearlLanded { owner: PlayerId::HOST, pos: DVec3::new(30.5, 10.0, 0.5) }];
    let roll = |spawning: bool, monsters: bool, difficulty: Difficulty| {
        let mut e = Entities::new(1234);
        e.monster_spawning = monsters;
        e.difficulty = difficulty;
        let c = Ctx { spawning, ..ctx(vec![thrower]) };
        for _ in 0..20_000 {
            e.pearl_endermites(&landed, &c);
        }
        e
    };
    let e = roll(true, true, Difficulty::Normal);
    let n = e.mobs.len() as f64;
    // Binomial(20000, 0.05): 1000 expected, standard deviation about 31.
    assert!((n - 1000.0).abs() < 5.0 * (20_000.0f64 * 0.05 * 0.95).sqrt(), "{n} endermites");
    assert!(e.mobs.iter().all(|m| m.kind == MobKind::Endermite && m.pos == thrower.pos));
    assert!(e.mobs.iter().all(|m| (m.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6), "faces as the thrower did");
    assert_eq!(roll(true, true, Difficulty::Peaceful).mobs.len(), 0, "never on Peaceful");
    assert_eq!(roll(false, true, Difficulty::Hard).mobs.len(), 0, "spawn_mobs off");
    assert_eq!(roll(true, false, Difficulty::Hard).mobs.len(), 0, "spawn_monsters off");
    // A dead thrower teleports nowhere and leaves nothing.
    let mut e = Entities::new(9);
    let dead = Target { alive: false, ..thrower };
    for _ in 0..2_000 {
        e.pearl_endermites(&landed, &ctx(vec![dead]));
    }
    assert!(e.mobs.is_empty());
}

#[test]
fn thrown_pearls_land_and_sometimes_leave_endermites_through_the_entity_update() {
    let world = Grid::flat(10);
    let start = DVec3::new(0.5, 10.0, 0.5);
    let (mut landings, mut mites) = (0, 0);
    for seed in 0..300 {
        let mut e = Entities::new(seed);
        let mut t = Target::new(PlayerId::HOST, start, false);
        t.look = DVec3::X;
        let c = Ctx { spawning: true, ..ctx(vec![t]) };
        e.throw_pearl(PlayerId::HOST, start + DVec3::Y * 1.62, DVec3::new(1.0, 0.3, 0.0), DVec3::ZERO);
        for _ in 0..200 {
            let events = e.update(0.05, &world, &c);
            if events.iter().any(|ev| matches!(ev, EntityEvent::PearlLanded { .. })) {
                landings += 1;
                break;
            }
        }
        for m in e.mobs.iter().filter(|m| m.kind == MobKind::Endermite) {
            assert_eq!(m.pos, start, "at the thrower's feet before the teleport");
            mites += 1;
        }
    }
    assert_eq!(landings, 300);
    // 15 expected; 3 standard deviations either side.
    assert!((4..=27).contains(&mites), "{mites} endermites from 300 landings");
}

#[test]
fn endermites_vanish_after_two_minutes_unless_persistent() {
    let world = Grid::flat(1);
    // `ctx` turns natural spawning off, so the endermites are the only mobs.
    let c = ctx(vec![bystander(DVec3::new(4.5, 1.0, 0.5))]);
    let mut e = Entities::new(17);
    mite(&mut e, DVec3::new(0.5, 1.0, 0.5));
    let named = mite(&mut e, DVec3::new(-2.5, 1.0, 0.5));
    let tag = Stack::new(Item::NAME_TAG, 1).with_name("Mitey").unwrap();
    assert_eq!(e.use_lead_or_name(named, Some(tag), PlayerId::HOST), Some(true));
    for tick in 1..MAX_LIFE {
        e.update(0.05, &world, &c);
        assert_eq!(
            e.count(MobKind::Endermite),
            2,
            "both alive at tick {tick}: {:?}",
            e.mobs.iter().map(|m| (m.kind, m.pos, m.health, m.dying, m.persistent)).collect::<Vec<_>>()
        );
    }
    let plain = e.mobs.iter().find(|m| !m.persistent).unwrap();
    assert_eq!(plain.endermite.as_ref().unwrap().life, MAX_LIFE - 1);
    e.update(0.05, &world, &c);
    assert_eq!(e.count(MobKind::Endermite), 1, "the untouched one is discarded at 2400");
    assert!(e.items.is_empty() && e.orbs.is_empty(), "a discard drops nothing");
    for _ in 0..600 {
        e.update(0.05, &world, &c);
    }
    assert_eq!(e.mobs.len(), 1);
    let kept = &e.mobs[0];
    assert!(kept.persistent && kept.name.as_str() == Some("Mitey"));
    assert_eq!(kept.endermite.as_ref().unwrap().life, 0, "a persistent endermite's life doesn't count");
    // `aiStep` still discards a persistent one whose saved life has run out.
    e.mobs[0].endermite.as_mut().unwrap().life = MAX_LIFE;
    e.update(0.05, &world, &c);
    assert_eq!(e.count(MobKind::Endermite), 0);
}

#[test]
fn endermite_lifetime_survives_saving_and_old_records_start_at_zero() {
    let mut e = Entities::new(4);
    let i = mite(&mut e, DVec3::new(2.5, 64.0, -1.5));
    e.mobs[i].endermite.as_mut().unwrap().life = 1234;
    let named = mite(&mut e, DVec3::new(5.5, 64.0, -1.5));
    e.mobs[named].persistent = true;
    e.mobs[named].endermite.as_mut().unwrap().life = 7;
    let text = e.nether_mobs_to_string();
    let mut loaded = Entities::new(5);
    loaded.load_nether_mobs(&text);
    let mites: Vec<_> = loaded.mobs.iter().filter(|m| m.kind == MobKind::Endermite).collect();
    assert_eq!(mites.len(), 2, "natural endermites are saved so their lifetime keeps counting");
    let life =
        |persistent: bool| mites.iter().find(|m| m.persistent == persistent).unwrap().endermite.as_ref().unwrap().life;
    assert_eq!((life(false), life(true)), (1234, 7));
    assert_eq!(mites.iter().find(|m| !m.persistent).unwrap().pos, DVec3::new(2.5, 64.0, -1.5));
    // A record without `lifetime` (as Java's `getIntOr("Lifetime", 0)`).
    let mut v = super::super::nether::save_mob(&e.mobs[i]);
    v.as_object_mut().unwrap().remove("lifetime");
    let old = super::super::nether::load_mob(&v).unwrap();
    assert_eq!(old.endermite.as_ref().unwrap().life, 0);
    // Other mobs don't gain the key.
    let zombie = super::super::nether::save_mob(&Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0));
    assert!(zombie.get("lifetime").is_none());
}

#[test]
fn endermites_hunt_the_nearest_visible_player_and_bite_for_two() {
    let world = Grid::flat(1);
    let player = Target::new(PlayerId::HOST, DVec3::new(4.5, 1.0, 0.5), true);
    let c = ctx(vec![player]);
    let mut e = Entities::new(21);
    mite(&mut e, DVec3::new(0.5, 1.0, 0.5));
    let mut bites = Vec::new();
    for _ in 0..200 {
        for ev in e.update(0.05, &world, &c) {
            if let EntityEvent::PlayerHit { damage, cause, .. } = ev {
                bites.push((damage, cause));
            }
        }
    }
    assert!(bites.len() >= 3, "{} bites", bites.len());
    // `MeleeAttackGoal`: one bite every 20 ticks at most.
    assert!(bites.len() <= 10, "{} bites", bites.len());
    assert!(bites.iter().all(|(d, cause)| *d == ATTACK_DAMAGE && cause.text.contains("endermite")));
    assert!(e.mobs[0].head_yaw.abs() < 1e-6, "no separate head");

    // A pen's wall between them hides the player from `NearestAttackableTargetGoal`.
    let mut walled = Grid::flat(1);
    for y in 1..3 {
        for a in -1..3 {
            for cell in [IVec3::new(a, y, -1), IVec3::new(a, y, 2), IVec3::new(-1, y, a), IVec3::new(2, y, a)] {
                walled.set(cell, Block::STONE);
            }
        }
    }
    let mut e = Entities::new(22);
    mite(&mut e, DVec3::new(0.5, 1.0, 0.5));
    for _ in 0..200 {
        let events = e.update(0.05, &walled, &c);
        assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { .. })));
    }
    assert!(e.mobs[0].move_ctl.target.is_none());
}

#[test]
fn a_hurt_endermite_alerts_the_endermites_around_it() {
    let c = ctx(vec![Target::new(PlayerId::HOST, DVec3::new(40.5, 1.0, 0.5), true)]);
    let mut e = Entities::new(31);
    mite(&mut e, DVec3::new(0.5, 1.0, 0.5));
    mite(&mut e, DVec3::new(12.5, 1.0, 0.5));
    mite(&mut e, DVec3::new(0.5, 13.0, 0.5));
    mite(&mut e, DVec3::new(-20.5, 1.0, 0.5));
    e.hurt_by_player_target(0, PlayerId::HOST);
    e.resolve_hurt_by(&c);
    let targets: Vec<_> = e.mobs.iter().map(|m| m.move_ctl.target).collect();
    assert_eq!(targets, [Some(PlayerId::HOST), Some(PlayerId::HOST), None, None]);
    // A creative player's hit turns nobody (`canAttack` refuses it).
    let mut e = Entities::new(32);
    mite(&mut e, DVec3::new(0.5, 1.0, 0.5));
    e.hurt_by_player_target(0, PlayerId::HOST);
    e.resolve_hurt_by(&ctx(vec![bystander(DVec3::new(2.5, 1.0, 0.5))]));
    assert!(e.mobs[0].move_ctl.target.is_none());
}

#[test]
fn endermen_in_sight_hunt_endermites_and_the_endermites_fight_back() {
    let world = Grid::flat(1);
    let c = ctx(vec![bystander(DVec3::new(0.5, 1.0, 20.5))]);
    let mut e = Entities::new(41);
    e.spawn(MobKind::Enderman, DVec3::new(0.5, 1.0, 0.5));
    e.mobs[0].on_ground = true;
    mite(&mut e, DVec3::new(8.5, 1.0, 0.5));
    let mut endermite_hit_back = false;
    let mut killed = false;
    for _ in 0..400 {
        e.update(0.05, &world, &c);
        let alive = e.mobs.iter().find(|m| m.kind == MobKind::Endermite && m.alive());
        if let Some(m) = alive {
            endermite_hit_back |= m.endermite.as_ref().unwrap().foe.is_some();
        } else {
            killed = true;
            break;
        }
    }
    assert!(killed, "the enderman killed the endermite");
    assert!(endermite_hit_back, "the bitten endermite turned on the enderman");
    assert!(e.mobs.iter().any(|m| m.kind == MobKind::Enderman && m.health > 0.0));

    // `mustSee`: behind a wall it doesn't notice the endermite.
    let mut walled = Grid::flat(1);
    for y in 1..6 {
        for z in -8..9 {
            walled.set(IVec3::new(4, y, z), Block::STONE);
        }
    }
    let mut e = Entities::new(42);
    e.spawn(MobKind::Enderman, DVec3::new(0.5, 1.0, 0.5));
    mite(&mut e, DVec3::new(8.5, 1.0, 0.5));
    e.update(0.05, &walled, &c);
    assert!(e.mobs[0].hunt.is_none());
    // An enderman angry at a player keeps after the player.
    let mut e = Entities::new(43);
    e.spawn(MobKind::Enderman, DVec3::new(0.5, 1.0, 0.5));
    mite(&mut e, DVec3::new(4.5, 1.0, 0.5));
    e.mobs[0].anger(30.0);
    e.endermite_upkeep(0.05, &world);
    assert!(e.mobs[0].hunt.is_none());
}
