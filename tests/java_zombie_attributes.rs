//! Zombie reinforcements, leaders and random spawn attributes against Java
//! 26.3 (`Zombie.hurtServer`, `Zombie.handleAttributes`,
//! `randomizeReinforcementsChance`, `Mob.finalizeSpawn`): the facts come from
//! `fixtures/java/26.3/zombie_attributes.json`, extracted from the bytecode by
//! `tools/extract_java_zombie_attribute_fixtures.py`. The seeded behaviour
//! checks live in `src/entity/zombie/tests.rs`.
use glam::DVec3;
use serde_json::Value;
use voxelcraft::entity::{
    Entities, Mob, MobKind,
    zombie::{self, Attrs},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/zombie_attributes.json")).unwrap()
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn java_id(kind: MobKind) -> String {
    kind.name().replace(' ', "_")
}

#[test]
fn reinforcement_carriers_match_java_in_both_directions() {
    let f = fixture();
    let carriers = f["spawn_reinforcements_carriers"].as_object().unwrap();
    for kind in MobKind::ALL {
        let java = carriers.get(&java_id(kind));
        assert_eq!(zombie::calls_reinforcements(kind), java.is_some(), "{kind:?}");
        // Their base before `randomizeReinforcementsChance` is 0.
        if let Some(base) = java {
            assert_eq!(num(base), 0.0);
        }
    }
    for id in carriers.keys() {
        assert!(MobKind::ALL.iter().any(|&k| &java_id(k) == id), "Java's {id} has no kind");
    }
    let liquids: Vec<_> = f["reinforcements"]["can_spawn_in_liquids"].as_array().unwrap().clone();
    assert_eq!(liquids, vec![Value::from(java_id(MobKind::Drowned))]);
}

#[test]
fn constants_match_the_bytecode() {
    let f = fixture();
    let r = &f["reinforcements"];
    assert_eq!(r["difficulty"], "hard");
    assert_eq!(r["needs_spawning_monsters"], true);
    assert_eq!(r["spawn_reason"], "reinforcement");
    assert_eq!(r["attempts"].as_u64().unwrap(), u64::from(zombie::REINFORCEMENT_ATTEMPTS));
    assert_eq!(r["range_min"].as_i64().unwrap(), i64::from(zombie::REINFORCEMENT_RANGE_MIN));
    assert_eq!(r["range_max"].as_i64().unwrap(), i64::from(zombie::REINFORCEMENT_RANGE_MAX));
    assert_eq!((r["sign_min"].as_i64(), r["sign_max"].as_i64()), (Some(-1), Some(1)));
    assert_eq!(num(&r["player_clearance"]), zombie::REINFORCEMENT_PLAYER_CLEARANCE);
    assert_eq!(num(&r["caller_charge"]), zombie::CALLER_CHARGE);
    assert_eq!(num(&r["callee_charge"]), zombie::CALLEE_CHARGE);
    assert_eq!(r["charge_operation"], "add_value");

    let h = &f["handle_attributes"];
    assert_eq!(num(&h["reinforcement_base_scale"]), zombie::REINFORCEMENT_BASE_SCALE);
    assert_eq!(num(&h["zombified_piglin_reinforcement_base"]), 0.0);
    assert_eq!(num(&h["knockback_bonus_scale"]), zombie::KNOCKBACK_BONUS_SCALE);
    assert_eq!(h["knockback_operation"], "add_value");
    assert_eq!(num(&h["follow_bonus_scale"]), zombie::FOLLOW_BONUS_SCALE);
    assert_eq!(num(&h["follow_bonus_threshold"]), zombie::FOLLOW_BONUS_THRESHOLD);
    assert_eq!(h["follow_operation"], "add_multiplied_total");
    assert_eq!(num(&h["leader_chance"]) as f32, zombie::LEADER_CHANCE);
    assert_eq!(num(&h["leader_reinforcement_scale"]), zombie::LEADER_REINFORCEMENT_SCALE);
    assert_eq!(num(&h["leader_reinforcement_offset"]), zombie::LEADER_REINFORCEMENT_OFFSET);
    assert_eq!(h["leader_reinforcement_operation"], "add_value");
    assert_eq!(num(&h["leader_health_scale"]), zombie::LEADER_HEALTH_SCALE);
    assert_eq!(num(&h["leader_health_offset"]), zombie::LEADER_HEALTH_OFFSET);
    assert_eq!(h["leader_health_operation"], "add_multiplied_total");
    assert_eq!(h["leader_breaks_doors"], true);
    assert_eq!(num(&h["break_door_chance"]) as f32, zombie::BREAK_DOOR_CHANCE);

    let b = &f["random_spawn_bonus"];
    assert_eq!(b["attribute"], "follow_range");
    assert_eq!(b["operation"], "add_multiplied_base");
    assert_eq!(num(&b["triangle_mean"]), 0.0);
    assert_eq!(num(&b["triangle_spread"]), zombie::RANDOM_SPAWN_BONUS_SPREAD);

    assert_eq!(num(&f["follow_range"]["mob"]), zombie::MOB_FOLLOW_RANGE);
    assert_eq!(num(&f["follow_range"]["zombie"]), zombie::ZOMBIE_FOLLOW_RANGE);
    let a = &f["attributes"];
    let range = |name: &str| (num(&a[name]["min"]), num(&a[name]["max"]));
    assert_eq!(range("spawn_reinforcements"), zombie::SPAWN_REINFORCEMENTS_RANGE);
    assert_eq!(num(&a["spawn_reinforcements"]["default"]), 0.0);
    assert_eq!(range("max_health"), zombie::MAX_HEALTH_RANGE);
    assert_eq!(range("follow_range"), zombie::FOLLOW_RANGE_RANGE);
    assert_eq!(range("knockback_resistance"), zombie::KNOCKBACK_RESISTANCE_RANGE);
}

/// Every zombie-family kind follows from 35 (`Zombie.createAttributes`).
#[test]
fn zombie_family_follow_range_is_javas() {
    let f = fixture();
    let zombie = num(&f["follow_range"]["zombie"]);
    for kind in MobKind::ALL.into_iter().filter(|&k| zombie::calls_reinforcements(k)) {
        assert_eq!(kind.follow_range_base(), zombie, "{kind:?}");
        assert_eq!(Mob::new(kind, DVec3::ZERO, 0.0).follow_range(), zombie, "{kind:?}");
    }
    assert_eq!(MobKind::Cow.follow_range_base(), num(&f["follow_range"]["mob"]));
}

/// Hard spawns through the shared `Entities::spawn` path roll the leader
/// rate from Java's chance times the local special multiplier, and a
/// leader's max health lies in Java's 2x to 5x range.
#[test]
fn spawned_zombies_roll_leaders_at_javas_rate() {
    let f = fixture();
    let h = &f["handle_attributes"];
    let mut e = Entities::new(21);
    e.difficulty = voxelcraft::simulation::difficulty::Difficulty::Hard;
    e.clock_time = 10_000_000;
    e.moon_brightness = 1.0;
    let special = voxelcraft::simulation::difficulty::special_multiplier(e.difficulty.effective(
        e.clock_time,
        0,
        e.moon_brightness,
    ));
    let n = 20_000;
    for _ in 0..n {
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 64.0, 0.5));
    }
    let leaders: Vec<&Mob> = e.mobs.iter().filter(|m| m.is_leader()).collect();
    let p = f64::from(special * num(&h["leader_chance"]) as f32);
    let se = (p * (1.0 - p) / n as f64).sqrt();
    assert!(((leaders.len() as f64 / n as f64) - p).abs() < 5.0 * se, "{} leaders", leaders.len());
    let lo = 20.0 * (1.0 + num(&h["leader_health_offset"]));
    let hi = 20.0 * (1.0 + num(&h["leader_health_offset"]) + num(&h["leader_health_scale"]));
    for m in leaders {
        let max = f64::from(m.max_health());
        assert!(max >= lo && max < hi, "{max}");
        assert_eq!(m.health, m.max_health(), "spawned leaders are healed");
    }
    assert!(e.mobs.iter().all(|m| m.attributes() != Attrs::default()));
}
