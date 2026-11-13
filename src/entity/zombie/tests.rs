//! Seeded checks of the zombie family's spawn attributes and reinforcement
//! calls against the rules in `Zombie` and `Mob` (Java 26.3).
use super::*;
use crate::{
    entity::{Target, mob::Ai},
    physics::BlockSource,
    simulation::lighting::SpawnLight,
    world::block::Block,
};

/// Stone up to `floor`, water up to `water_top` (exclusive), air above;
/// every cell has skylight `sky` and no block light.
struct Flat {
    floor: i32,
    water_top: i32,
    sky: u8,
}

impl BlockSource for Flat {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(if p.y <= self.floor {
            Block::STONE
        } else if p.y < self.water_top {
            Block::WATER
        } else {
            Block::AIR
        })
    }
}

impl MobWorld for Flat {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(self.floor)
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(self.sky)
    }
}

const DARK: Flat = Flat { floor: 63, water_top: 0, sky: 0 };

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        players,
        daylight: 0.0,
        spawn_light: SpawnLight::NIGHT,
        spawning: true,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

fn player_at(x: f64, z: f64) -> Target {
    Target::new(PlayerId::HOST, DVec3::new(x, 64.0, z), true)
}

/// Entities on Hard with one `kind` at (0.5, y, 0.5) whose
/// `spawn_reinforcements` is `chance`.
fn caller(kind: MobKind, y: f64, chance: f64, seed: u64) -> Entities {
    let mut e = Entities::new(seed);
    e.difficulty = Difficulty::Hard;
    let mut m = Mob::new(kind, DVec3::new(0.5, y, 0.5), 0.0);
    m.attrs = Attrs::for_kind(kind);
    m.attrs.reinforcement_base = chance;
    e.mobs.push(m);
    e
}

fn hit(e: &mut Entities) {
    e.mobs[0].reinforce_call = Some(Some(PlayerId::HOST));
}

#[test]
fn handle_attributes_rolls_java_ranges_and_rates() {
    let mut rng = Rng::new(11);
    let n = 20_000;
    let (mut leaders, mut follow, mut base_sum) = (0, 0, 0.0);
    for _ in 0..n {
        let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
        m.health = 7.0;
        finalize_spawn(&mut m, 1.0, true, &mut rng);
        let a = m.attributes();
        assert_eq!(a.kind, Some(MobKind::Zombie));
        assert!((0.0..REINFORCEMENT_BASE_SCALE).contains(&a.reinforcement_base));
        assert!((0.0..KNOCKBACK_BONUS_SCALE).contains(&a.knockback_bonus));
        assert!(a.follow_spawn_bonus.abs() <= RANDOM_SPAWN_BONUS_SPREAD);
        base_sum += a.reinforcement_base;
        if a.zombie_follow_bonus != 0.0 {
            follow += 1;
            assert!(a.zombie_follow_bonus > 1.0 && a.zombie_follow_bonus <= 1.5);
        }
        if m.is_leader() {
            leaders += 1;
            assert!((0.5..0.75).contains(&a.leader_reinforcement));
            assert!(a.can_break_doors);
            // Max health x (2..5), and the leader is healed to it.
            let max = m.max_health();
            assert!((40.0..100.0).contains(&max), "{max}");
            assert_eq!(m.health, max);
            assert!(m.reinforcement_chance() >= 0.5);
        } else {
            assert_eq!(a.leader_reinforcement, 0.0);
            assert_eq!(m.max_health(), 20.0);
            assert_eq!(m.health, 7.0);
        }
    }
    let within = |count: usize, p: f64| {
        let se = (p * (1.0 - p) / n as f64).sqrt();
        ((count as f64 / n as f64) - p).abs() < 5.0 * se
    };
    // Leader with LEADER_CHANCE x special; follow bonus when nextDouble() x 1.5 > 1.
    assert!(within(leaders, f64::from(LEADER_CHANCE)), "{leaders}");
    assert!(within(follow, 1.0 / 3.0), "{follow}");
    // Base nextDouble() x 0.1F: mean 0.05, standard error 0.1 / sqrt(12 n).
    let se = REINFORCEMENT_BASE_SCALE / (12.0 * n as f64).sqrt();
    assert!((base_sum / n as f64 - REINFORCEMENT_BASE_SCALE / 2.0).abs() < 5.0 * se);
}

#[test]
fn half_special_multiplier_never_extends_follow_range_and_piglins_never_call() {
    let mut rng = Rng::new(3);
    for _ in 0..5000 {
        let mut m = Mob::new(MobKind::Husk, DVec3::ZERO, 0.0);
        finalize_spawn(&mut m, 0.5, true, &mut rng);
        // nextDouble() x 1.5 x 0.5 never exceeds 1.
        assert_eq!(m.attributes().zombie_follow_bonus, 0.0);
        let mut p = Mob::new(MobKind::ZombifiedPiglin, DVec3::ZERO, 0.0);
        finalize_spawn(&mut p, 0.0, true, &mut rng);
        // `ZombifiedPiglin.randomizeReinforcementsChance` sets 0, and with no
        // special multiplier there are no leaders.
        assert_eq!(p.reinforcement_chance(), 0.0);
        assert!(!p.is_leader());
    }
}

#[test]
fn every_spawned_mob_gets_the_follow_range_triangle() {
    let mut e = Entities::new(5);
    let n = 6000;
    for _ in 0..n {
        e.spawn(MobKind::Cow, DVec3::new(0.5, 64.0, 0.5));
    }
    let mut sum = 0.0;
    let mut sq = 0.0;
    for m in &e.mobs {
        let b = m.attributes().follow_spawn_bonus;
        assert!(b.abs() <= RANDOM_SPAWN_BONUS_SPREAD);
        // ADD_MULTIPLIED_BASE on the cow's 16.
        assert!((m.follow_range() - MOB_FOLLOW_RANGE * (1.0 + b)).abs() < 1e-9);
        assert_eq!(m.reinforcement_chance(), 0.0);
        sum += b;
        sq += b * b;
    }
    // triangle(0, s) = s (U1 - U2): mean 0, variance s^2 / 6.
    let var = RANDOM_SPAWN_BONUS_SPREAD.powi(2) / 6.0;
    assert!((sum / n as f64).abs() < 5.0 * (var / n as f64).sqrt());
    assert!((sq / n as f64 - var).abs() < 0.1 * var, "{}", sq / n as f64);
}

#[test]
fn computed_attributes_follow_java_operation_order() {
    let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    m.attrs = Attrs {
        kind: Some(MobKind::Zombie),
        follow_spawn_bonus: 0.1,
        zombie_follow_bonus: 1.2,
        knockback_bonus: 0.04,
        reinforcement_base: 0.08,
        caller_charge: -0.15,
        callee_charge: true,
        leader_reinforcement: 0.6,
        leader_health: 2.5,
        can_break_doors: true,
    };
    // (35 + 35 x 0.1) x (1 + 1.2).
    assert!((m.follow_range() - 35.0 * 1.1 * 2.2).abs() < 1e-9);
    assert!((m.knockback_resistance() - 0.04).abs() < 1e-12);
    assert!((m.reinforcement_chance() - (0.08 - 0.15 + CALLEE_CHARGE + 0.6)).abs() < 1e-12);
    assert_eq!(m.max_health(), 70.0);
    // Charges past the base clamp at 0.
    m.attrs.caller_charge = -5.0;
    assert_eq!(m.reinforcement_chance(), 0.0);
    // A conversion in place leaves the rolls stale: they read as none.
    m.kind = MobKind::Drowned;
    assert_eq!(m.attributes(), Attrs::default());
    assert_eq!(m.max_health(), 20.0);
    assert_eq!(m.follow_range(), ZOMBIE_FOLLOW_RANGE);
}

#[test]
fn reinforcements_need_hard_spawning_monsters_and_a_target() {
    let world = DARK;
    let players = vec![player_at(2.5, 0.5)];
    // Normal: no call.
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 1);
    e.difficulty = Difficulty::Normal;
    hit(&mut e);
    e.call_reinforcements(&world, &ctx(players.clone()));
    assert_eq!(e.mobs.len(), 1);
    // spawn_monsters off, then spawn_mobs off.
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 1);
    e.monster_spawning = false;
    hit(&mut e);
    e.call_reinforcements(&world, &ctx(players.clone()));
    assert_eq!(e.mobs.len(), 1);
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 1);
    hit(&mut e);
    let mut off = ctx(players.clone());
    off.spawning = false;
    e.call_reinforcements(&world, &off);
    assert_eq!(e.mobs.len(), 1);
    // Hurt without a target or attacker (fall damage while idle): no call.
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 1);
    e.mobs[0].reinforce_call = Some(None);
    e.call_reinforcements(&world, &ctx(players.clone()));
    assert_eq!(e.mobs.len(), 1);
    // A zero chance never calls.
    let mut e = caller(MobKind::Zombie, 64.0, 0.0, 1);
    hit(&mut e);
    e.call_reinforcements(&world, &ctx(players.clone()));
    assert_eq!(e.mobs.len(), 1);
    // Hard, a player attacker, chance 1: one reinforcement of the same kind,
    // sent after the attacker, with the callee charge; the caller is charged.
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 1);
    hit(&mut e);
    e.call_reinforcements(&world, &ctx(players.clone()));
    assert_eq!(e.mobs.len(), 2);
    let r = &e.mobs[1];
    assert_eq!(r.kind, MobKind::Zombie);
    assert_eq!(r.reinforce_target, Some(PlayerId::HOST));
    assert!(r.attributes().callee_charge);
    assert!(!r.persistent);
    assert_eq!(e.mobs[0].attributes().caller_charge, CALLER_CHARGE);
}

#[test]
fn a_player_melee_hit_calls_reinforcements_on_the_next_update() {
    let world = DARK;
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 9);
    e.melee(0, DVec3::X, None, 0.0, false, None);
    assert_eq!(e.mobs[0].reinforce_call, Some(Some(PlayerId::HOST)));
    let mut c = ctx(vec![player_at(-1.0, 0.5)]);
    c.spawning = true;
    e.call_reinforcements(&world, &c);
    assert_eq!(e.mobs.len(), 2);
    assert_eq!(e.mobs[0].reinforce_call, None);
}

#[test]
fn reinforcements_land_7_to_40_blocks_out_on_valid_dry_ground_away_from_players() {
    let world = DARK;
    let far_player = player_at(20.5, 0.5);
    let mut seen = std::collections::BTreeSet::new();
    let mut near_far_player = [0, 0];
    for (with_far, seed) in [(false, 0u64), (true, 1_000)] {
        let mut players = vec![player_at(0.5, 2.5)];
        if with_far {
            players.push(far_player);
        }
        for s in 0..1500 {
            let mut e = caller(MobKind::Zombie, 64.0, 1.0, seed + s);
            hit(&mut e);
            e.call_reinforcements(&world, &ctx(players.clone()));
            assert_eq!(e.mobs.len(), 2, "a flat dark floor always has room");
            let pos = e.mobs[1].pos;
            assert_eq!(pos, pos.floor(), "Java places it on the block corner");
            let d = pos.as_ivec3();
            assert_eq!(d.y, 64, "only the floor's top is valid ground");
            for v in [d.x, d.z] {
                assert!(v == 0 || (7..=40).contains(&v.abs()), "{d}");
                seen.insert(v);
            }
            for t in &players {
                assert!(t.pos.distance(pos) >= REINFORCEMENT_PLAYER_CLEARANCE, "{d}");
            }
            if far_player.pos.distance(pos) < REINFORCEMENT_PLAYER_CLEARANCE {
                near_far_player[with_far as usize] += 1;
            }
        }
    }
    // Both signs and both ends of the range turn up.
    for v in [-40, -7, 0, 7, 40] {
        assert!(seen.contains(&v), "{v} never rolled");
    }
    // Without the far player some land where it would stand; with it, none.
    assert!(near_far_player[0] > 0);
    assert_eq!(near_far_player[1], 0);
    // Water or a blocked box is refused: no floor out of the water.
    let flooded = Flat { floor: 63, water_top: 70, sky: 0 };
    let mut e = caller(MobKind::Zombie, 64.0, 1.0, 4);
    hit(&mut e);
    e.call_reinforcements(&flooded, &ctx(vec![player_at(2.5, 0.5)]));
    assert_eq!(e.mobs.len(), 1);
}

#[test]
fn drowned_call_drowned_in_dark_water_and_husks_call_husks_under_the_sky() {
    let water = Flat { floor: 40, water_top: 62, sky: 0 };
    let mut e = caller(MobKind::Drowned, 50.0, 1.0, 2);
    hit(&mut e);
    e.call_reinforcements(&water, &ctx(vec![Target::new(PlayerId::HOST, DVec3::new(2.5, 50.0, 0.5), true)]));
    assert_eq!(e.mobs.len(), 2);
    assert_eq!(e.mobs[1].kind, MobKind::Drowned);
    let y = e.mobs[1].pos.y as i32;
    assert!(y > 41 && y < 62, "{y}");
    // Husks need the sky (15); under a roof (0) they find no spot.
    let open = Flat { floor: 63, water_top: 0, sky: 15 };
    let mut called = 0;
    for s in 0..20 {
        let mut e = caller(MobKind::Husk, 64.0, 1.0, 50 + s);
        hit(&mut e);
        e.call_reinforcements(&open, &ctx(vec![player_at(2.5, 0.5)]));
        called += e.mobs.len() - 1;
        assert!(e.mobs.iter().all(|m| m.kind == MobKind::Husk));
    }
    assert!(called > 0);
    let mut e = caller(MobKind::Husk, 64.0, 1.0, 6);
    hit(&mut e);
    e.call_reinforcements(&DARK, &ctx(vec![player_at(2.5, 0.5)]));
    assert_eq!(e.mobs.len(), 1);
}

/// Exact distribution of the number of reinforcements after `hits` calls
/// at starting chance `p`, each success charging the caller 0.05.
fn reinforcement_model(p: f64, hits: usize) -> (f64, f64) {
    let mut dist = vec![1.0];
    for _ in 0..hits {
        let mut next = vec![0.0; dist.len() + 1];
        for (k, &q) in dist.iter().enumerate() {
            let chance = (p + CALLER_CHARGE * k as f64).clamp(0.0, 1.0);
            next[k + 1] += q * chance;
            next[k] += q * (1.0 - chance);
        }
        dist = next;
    }
    let mean: f64 = dist.iter().enumerate().map(|(k, q)| k as f64 * q).sum();
    let var: f64 = dist.iter().enumerate().map(|(k, q)| (k as f64 - mean).powi(2) * q).sum();
    (mean, var)
}

#[test]
fn caller_charges_cut_the_expected_reinforcement_count() {
    let world = DARK;
    let (p, hits, trials) = (0.6, 20, 300);
    let (mean, var) = reinforcement_model(p, hits);
    let mut total = 0.0;
    for t in 0..trials {
        let mut e = caller(MobKind::Zombie, 64.0, p, 77 + t);
        for _ in 0..hits {
            hit(&mut e);
            e.call_reinforcements(&world, &ctx(vec![player_at(2.5, 0.5)]));
        }
        let count = e.mobs.len() - 1;
        assert!((e.mobs[0].attributes().caller_charge - CALLER_CHARGE * count as f64).abs() < 1e-9);
        assert!(e.mobs[1..].iter().all(|r| r.attributes().callee_charge));
        total += count as f64;
    }
    let se = (var / trials as f64).sqrt();
    assert!((total / trials as f64 - mean).abs() < 5.0 * se, "{} vs {mean}", total / trials as f64);
}

#[test]
fn reinforcements_hunt_their_target_out_to_follow_range() {
    // A zombie chases within its follow range (35), past the 24 other
    // monsters use, but not beyond it.
    for (x, chases) in [(30.5, true), (36.5, false)] {
        let mut e = Entities::new(3);
        e.mobs.push(Mob::new(MobKind::Zombie, DVec3::new(0.5, 64.0, 0.5), 0.0));
        let mut c = ctx(vec![player_at(x, 0.5)]);
        c.spawning = false;
        e.update_difficulty(0.05, &DARK, &c, Difficulty::Hard);
        assert_eq!(e.mobs[0].ai == Ai::Chase, chases, "{x}");
    }
    // Sent after a player, it keeps that target up to follow range even
    // where it would not have noticed them.
    let mut e = Entities::new(3);
    let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 64.0, 0.5), 0.0);
    m.reinforce_target = Some(PlayerId::HOST);
    e.mobs.push(m);
    let mut sneaky = player_at(33.5, 0.5);
    sneaky.sneaking = true;
    let mut c = ctx(vec![sneaky]);
    c.spawning = false;
    e.update_difficulty(0.05, &DARK, &c, Difficulty::Hard);
    assert_eq!(e.mobs[0].ai, Ai::Chase);
}

#[test]
fn drowning_conversion_rerolls_without_the_spawn_bonus() {
    // Through `Entities::update`: a zombie whose conversion ends this tick,
    // with its eyes under water, becomes a drowned.
    let water = Flat { floor: 40, water_top: 62, sky: 0 };
    for (from, to) in [(MobKind::Zombie, MobKind::Drowned), (MobKind::Husk, MobKind::Zombie)] {
        let mut e = Entities::new(8);
        let mut m = Mob::new(from, DVec3::new(0.5, 50.0, 0.5), 0.0);
        let mut rng = Rng::new(8);
        finalize_spawn(&mut m, 1.0, true, &mut rng);
        m.attrs.leader_health = 3.0;
        m.attrs.follow_spawn_bonus = 0.1;
        m.attrs.can_break_doors = true;
        m.water_conversion.exposure = 600;
        m.water_conversion.remaining = 0;
        e.mobs.push(m);
        let mut c = ctx(vec![Target::new(PlayerId::HOST, DVec3::new(30.5, 50.0, 0.5), true)]);
        c.spawning = false;
        e.update_difficulty(0.05, &water, &c, Difficulty::Hard);
        let m = &e.mobs[0];
        assert_eq!(m.kind, to);
        let a = m.attributes();
        assert_eq!(a.kind, Some(to));
        // `handleAttributes(CONVERSION)` only: no `Mob.finalizeSpawn` bonus,
        // the old leader bonus is gone unless re-rolled, and a re-rolled
        // leader is not healed.
        assert_eq!(a.follow_spawn_bonus, 0.0);
        assert!(a.reinforcement_base > 0.0 && a.knockback_bonus > 0.0);
        assert!((0.0..REINFORCEMENT_BASE_SCALE).contains(&a.reinforcement_base));
        assert!(a.leader_health == 0.0 || (1.0..4.0).contains(&a.leader_health));
        assert!(m.health <= 20.0);
        // `ConversionType.convertCommon` keeps `CanBreakDoors`.
        assert!(a.can_break_doors);
    }
}

#[test]
fn rolled_attributes_and_charges_survive_save_and_reload() {
    let mut m = Mob::new(MobKind::Husk, DVec3::new(1.5, 64.0, 2.5), 0.0);
    m.attrs = Attrs {
        kind: Some(MobKind::Husk),
        follow_spawn_bonus: -0.05,
        zombie_follow_bonus: 1.25,
        knockback_bonus: 0.03,
        reinforcement_base: 0.07,
        caller_charge: -0.1,
        callee_charge: true,
        leader_reinforcement: 0.6,
        leader_health: 3.5,
        can_break_doors: true,
    };
    m.health = 85.0;
    let saved = crate::entity::nether::save_mob(&m);
    let back = crate::entity::nether::load_mob(&saved).unwrap();
    assert_eq!(back.attributes(), m.attributes());
    assert_eq!(back.health, 85.0, "the leader's health is above the kind's 20");
    assert_eq!(back.max_health(), 90.0);
    assert_eq!(back.follow_range(), m.follow_range());
    assert_eq!(back.reinforcement_chance(), m.reinforcement_chance());
    // A pre-v0.17 record has no attributes: base values, health clamped to 20.
    let mut old = saved.clone();
    old.as_object_mut().unwrap().remove("attributes");
    let back = crate::entity::nether::load_mob(&old).unwrap();
    assert_eq!(back.attributes(), Attrs::default());
    assert_eq!(back.health, 20.0);
    assert_eq!(back.follow_range(), ZOMBIE_FOLLOW_RANGE);
    // Out-of-range values read as none.
    let mut bad = saved;
    bad["attributes"]["leader_health"] = serde_json::json!(40.0);
    bad["attributes"]["reinforcement_base"] = serde_json::json!(f64::NAN.to_string());
    let back = crate::entity::nether::load_mob(&bad).unwrap();
    assert_eq!(back.attributes().leader_health, 0.0);
    assert_eq!(back.attributes().reinforcement_base, 0.0);
    // Mobs with nothing rolled save no record.
    assert!(crate::entity::nether::save_mob(&Mob::new(MobKind::Cow, DVec3::ZERO, 0.0)).get("attributes").is_none());
}
