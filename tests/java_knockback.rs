//! Sweep attacks and knockback against pinned Java 26.3 facts
//! (`fixtures/java/26.3/combat.json`, extracted from `Player.attack`, `doSweepAttack`,
//! `isSweepAttack`, `LivingEntity.knockback`, `Mob.doHurtTarget`, `AbstractArrow.doKnockback`,
//! `HoglinBase.throwTarget` and the enchantment data by `tools/extract_java_combat_fixtures.py`).
//!
//! The reference cases are computed by the extractor, not by VoxelCraft. The scenarios
//! below build an independent evaluator from the fixture's constants, check it against
//! those cases first, and then compare whole-game outcomes with it.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use voxelcraft::{
    agent::{Agent, Command},
    enchant::{Enchantment, Enchants},
    entity::{Ctx, Entities, EntityEvent, MobKind, PlayerId, PlayerMelee, Target as MobTarget},
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind},
    knockback::{self, AttackSound, Hit, Push, Throw},
    session::{
        PlayerSession,
        combat::{Outcome, Target},
    },
    simulation::{effects::Effect, lighting::SpawnLight},
    world::{World, block::Block, chunk::ChunkData, terrain::Dimension, terrain::Generator},
};

fn fixture() -> &'static Value {
    static F: OnceLock<Value> = OnceLock::new();
    F.get_or_init(|| serde_json::from_str(include_str!("fixtures/java/26.3/combat.json")).unwrap())
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn vec3(v: &Value) -> DVec3 {
    let a = v.as_array().unwrap();
    DVec3::new(num(&a[0]), num(&a[1]), num(&a[2]))
}

fn near(a: DVec3, b: DVec3, tolerance: f64, what: &str) {
    assert!((a - b).abs().max_element() <= tolerance, "{what}: {a:?} vs {b:?}");
}

fn no_jitter() -> impl FnMut() -> f64 {
    || panic!("a push with a direction needs no random jitter")
}

// --- Constants -------------------------------------------------------------------------

#[test]
fn constants_are_the_extracted_bytecode_values() {
    let f = fixture();
    let k = &f["living_entity_knockback"];
    assert_eq!(knockback::DEFAULT_STRENGTH, num(&k["default_strength"]));
    assert_eq!(knockback::DEFAULT_STRENGTH as f32, num(&k["default_strength_float"]) as f32);
    assert_eq!(knockback::MOTION_DIVISOR, num(&k["horizontal_motion_divisor"]));
    assert_eq!(knockback::VERTICAL_CAP, num(&k["vertical_cap"]));
    assert_eq!(knockback::ZERO_DIRECTION, num(&k["zero_direction_threshold"]));
    assert_eq!(knockback::JITTER_SCALE, num(&k["zero_direction_jitter"]));
    let a = &f["attack_knockback"];
    assert_eq!(f64::from(knockback::SPRINT_BONUS), num(&a["player_sprint_bonus"]));
    assert_eq!(knockback::ATTACKER_SLOWDOWN, num(&a["attacker_after"]["horizontal_motion_multiplier"]));
    assert_eq!(f64::from(knockback::DEGREES_TO_RADIANS), 0.017453292f32 as f64);
    let arrow = &f["arrow_knockback"];
    assert_eq!(knockback::ARROW_PUSH_PER_LEVEL, num(&arrow["per_level_push"]));
    assert_eq!(knockback::ARROW_PUSH_LIFT, num(&arrow["vertical_push"]));
    let sweep = &f["sweep_attack"];
    assert_eq!(knockback::SWEEP_INFLATE, vec3(&sweep["target_box"]["inflate"]));
    assert_eq!(knockback::SWEEP_RANGE_SQR, num(&sweep["max_distance_sqr"]));
    assert_eq!(knockback::SWEEP_SPEED_FACTOR, num(&sweep["requires"]["horizontal_movement_below_speed_times"]));
    assert_eq!(knockback::SWEEP_STRENGTH, num(&sweep["knockback"]["strength"]));
    let speed = &f["movement_speed"];
    assert_eq!(knockback::PLAYER_SPEED, num(&speed["player_base"]));
    assert_eq!(knockback::SPEED_PER_LEVEL, num(&speed["effects"]["speed"]["amount_per_level"]));
    assert_eq!(knockback::SLOWNESS_PER_LEVEL, num(&speed["effects"]["slowness"]["amount_per_level"]));
    assert_eq!(f["mob_melee"]["default_hurt_knockback"], k["default_strength"]);
}

#[test]
fn facing_vectors_use_the_java_sine_table() {
    for case in fixture()["cases"]["yaw_direction"].as_array().unwrap() {
        let yaw = num(&case["yaw_degrees"]) as f32;
        let (x, z) = knockback::facing_vector(yaw);
        assert_eq!(x, num(&case["push_x"]), "x at {yaw}");
        assert_eq!(z, num(&case["push_z"]), "z at {yaw}");
    }
    // VoxelCraft's yaw 0 looks along +X, which is Java yaw -90.
    assert_eq!(knockback::java_yaw_degrees(0.0), -90.0);
    assert!((knockback::java_yaw_degrees(std::f32::consts::FRAC_PI_2)).abs() < 1e-5);
}

// --- Pure rules against extracted cases -------------------------------------------------

#[test]
fn living_entity_knockback_matches_the_reference_cases() {
    let cases = fixture()["cases"]["knockback"].as_array().unwrap();
    assert!(cases.len() >= 12);
    for case in cases {
        let push = Push { strength: num(&case["strength"]), x: num(&case["x"]), z: num(&case["z"]) };
        let got = knockback::apply(
            vec3(&case["old_motion"]),
            push,
            num(&case["resistance"]),
            case["on_ground"].as_bool().unwrap(),
            &mut no_jitter(),
        );
        match case["new_motion"].as_array() {
            None => assert_eq!(got, None, "{case}"),
            Some(_) => {
                near(got.unwrap_or_else(|| panic!("{case}")), vec3(&case["new_motion"]), 1e-12, &case.to_string())
            }
        }
    }
}

#[test]
fn a_push_without_a_direction_uses_the_reference_jitter() {
    let k = &fixture()["living_entity_knockback"];
    let (scale, tiny) = (num(&k["zero_direction_jitter"]), num(&k["zero_direction_threshold"]));
    // Draws are (nextDouble - nextDouble) * 0.01 for x, then z, repeated until the squared length reaches the threshold.
    let draws = [0.9, 0.4, 0.1, 0.35];
    let (x, z) = ((draws[0] - draws[1]) * scale, (draws[2] - draws[3]) * scale);
    assert!(x * x + z * z >= tiny);
    let mut next = draws.into_iter();
    let mut jitter = || next.next().expect("one round of draws");
    let got = knockback::apply(DVec3::ZERO, Push { strength: 0.4, x: 0.0, z: 0.0 }, 0.0, true, &mut jitter).unwrap();
    let away = DVec3::new(x, 0.0, z).normalize() * 0.4;
    near(got, DVec3::new(-away.x, 0.4, -away.z), 1e-12, "jittered direction");
    // Draws too small to give a direction are repeated.
    let mut calls = 0;
    let mut tiny_then_ok = || {
        calls += 1;
        [0.5, 0.5, 0.5, 0.5, 0.9, 0.4, 0.1, 0.35][calls - 1]
    };
    knockback::apply(DVec3::ZERO, Push { strength: 0.4, x: 0.0, z: 0.0 }, 0.0, true, &mut tiny_then_ok).unwrap();
    assert_eq!(calls, 8, "a second round of draws replaces a direction shorter than the threshold");
}

#[test]
fn attack_knockback_strength_matches_the_reference_cases() {
    let cases = fixture()["cases"]["attack_knockback"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let got = knockback::attack_strength(
            num(&case["attack_knockback_attribute"]),
            num(&case["knockback_level"]) as u32,
            case["sprint_bonus"].as_bool().unwrap(),
        );
        assert_eq!(got, num(&case["strength"]) as f32, "{case}");
    }
}

#[test]
fn arrow_punch_matches_the_reference_cases() {
    let cases = fixture()["cases"]["arrow_punch"].as_array().unwrap();
    assert_eq!(cases.len(), 140);
    for case in cases {
        let got = knockback::arrow_push(
            num(&case["punch_level"]) as u32,
            num(&case["resistance"]),
            vec3(&case["arrow_motion"]),
        );
        match case["push"].as_array() {
            None => assert_eq!(got, None, "{case}"),
            Some(_) => near(got.unwrap_or_else(|| panic!("{case}")), vec3(&case["push"]), 1e-12, &case.to_string()),
        }
    }
}

#[test]
fn hoglin_tusk_toss_matches_the_reference_cases() {
    let cases = fixture()["cases"]["hoglin_throw"].as_array().unwrap();
    assert_eq!(cases.len(), 240);
    for case in cases {
        let toss = Throw {
            strength: num(&case["attack_knockback"]),
            dx: num(&case["dx"]),
            dz: num(&case["dz"]),
            turn: num(&case["turn"]) as f32,
            reach: num(&case["reach_roll"]) as f32,
            lift: num(&case["lift_roll"]) as f32,
        };
        let got = toss.push(num(&case["resistance"]));
        match case["push"].as_array() {
            None => assert_eq!(got, None, "{case}"),
            Some(_) => near(got.unwrap_or_else(|| panic!("{case}")), vec3(&case["push"]), 1e-9, &case.to_string()),
        }
    }
}

#[test]
fn player_movement_speed_follows_the_reference_attribute_modifiers() {
    let speeds = &fixture()["cases"]["player_speed"];
    assert_eq!(knockback::movement_speed(0, 0, false), num(&speeds["walking"]) as f32);
    for level in 1..=3u32 {
        assert_eq!(knockback::movement_speed(level, 0, false), num(&speeds[format!("speed_{level}")]) as f32);
        assert_eq!(knockback::movement_speed(0, level, false), num(&speeds[format!("slowness_{level}")]) as f32);
    }
    // Sprinting multiplies by 1.3 (`add_multiplied_total`), after which the bonus effects multiply on.
    let m = &fixture()["movement_speed"];
    let sprint = num(&m["sprint_modifier"]);
    assert_eq!(m["sprint_operation"], "add_multiplied_total");
    assert_eq!(knockback::movement_speed(0, 0, true), (num(&m["player_base"]) * (1.0 + sprint)) as f32);
    // Effects combine by multiplication, not addition.
    let mut effects = voxelcraft::simulation::effects::Effects::default();
    effects.add(Effect::Speed, 1, 600);
    effects.add(Effect::Slowness, 0, 600);
    let expected = (1.0 + num(&m["effects"]["speed"]["amount_per_level"]) * 2.0)
        * (1.0 + num(&m["effects"]["slowness"]["amount_per_level"]));
    assert!((effects.speed_factor() - expected).abs() < 1e-12);
    assert_eq!(effects.movement_speed_levels(), (2, 1));
}

#[test]
fn sweep_eligibility_matches_the_reference_cases() {
    let cases = fixture()["cases"]["sweep_eligibility"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    for case in cases {
        let flag = |k: &str| case[k].as_bool().unwrap();
        let got = knockback::sweep_allowed(
            flag("strong"),
            flag("critical"),
            flag("sprint_knockback"),
            flag("on_ground"),
            num(&case["horizontal_distance_sqr"]),
            num(&case["speed"]) as f32,
            flag("sword"),
        );
        assert_eq!(got, flag("allowed"), "{case}");
    }
}

#[test]
fn the_movement_limit_is_a_strict_comparison() {
    for speed in [0.1f32, 0.12, 0.085] {
        let edge = (f64::from(speed) * 2.5) * (f64::from(speed) * 2.5);
        let allowed = |moved: f64| knockback::sweep_allowed(true, false, false, true, moved, speed, true);
        assert!(!allowed(edge), "equal to (speed x 2.5)^2 is not below it");
        assert!(allowed(f64::from_bits(edge.to_bits() - 1)));
        assert!(!allowed(f64::from_bits(edge.to_bits() + 1)));
    }
}

#[test]
fn sweeping_edge_and_sweep_damage_match_the_reference_cases() {
    let ratios = &fixture()["sweep_attack"]["sweeping_edge_ratio"];
    assert_eq!(knockback::sweeping_ratio(0), 0.0);
    for level in 1..=3u32 {
        assert_eq!(knockback::sweeping_ratio(level), num(&ratios[level.to_string()]) as f32, "level {level}");
    }
    let cases = fixture()["cases"]["sweep_damage"].as_array().unwrap();
    assert_eq!(cases.len(), 192);
    for case in cases {
        let got = knockback::sweep_damage(
            num(&case["scaled_attack_damage"]) as f32,
            num(&case["sweeping_edge_level"]) as u32,
            num(&case["victim_enchantment_bonus"]) as f32,
            num(&case["attack_strength"]) as f32,
        );
        assert!((f64::from(got) - num(&case["damage"])).abs() < 1e-6, "{case}: {got}");
    }
}

#[test]
fn sweep_victims_come_from_the_inflated_target_box_within_three_blocks() {
    let cases = fixture()["cases"]["sweep_reach"].as_array().unwrap();
    let target = (DVec3::new(-0.3, 0.0, -0.3), DVec3::new(0.3, 1.95, 0.3));
    for case in cases {
        let at = vec3(&case["victim_position"]);
        let (w, h) = (num(&case["victim_half_width"]), num(&case["victim_height"]));
        let victim = (at - DVec3::new(w, 0.0, w), at + DVec3::new(w, h, w));
        let in_box = knockback::in_sweep_box(target, victim);
        let distance = vec3(&case["attacker_position"]).distance_squared(at);
        assert_eq!(in_box, case["in_box"].as_bool().unwrap(), "{case}");
        assert!((distance - num(&case["distance_sqr"])).abs() < 1e-9);
        assert_eq!(in_box && distance < knockback::SWEEP_RANGE_SQR, case["swept"].as_bool().unwrap(), "{case}");
    }
}

fn sound_name(sound: AttackSound) -> &'static str {
    match sound {
        AttackSound::Knockback => "player.attack.knockback",
        AttackSound::Crit => "player.attack.crit",
        AttackSound::Sweep => "player.attack.sweep",
        AttackSound::Strong => "player.attack.strong",
        AttackSound::Weak => "player.attack.weak",
        AttackSound::NoDamage => "player.attack.nodamage",
    }
}

#[test]
fn attack_sounds_match_the_reference_table() {
    let cases = fixture()["cases"]["attack_sounds"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    for case in cases {
        let flag = |k: &str| case[k].as_bool().unwrap();
        let got: Vec<_> = knockback::attack_sounds(
            flag("strong"),
            flag("sprinting"),
            flag("critical"),
            flag("sweep"),
            flag("primary_hit_landed"),
        )
        .into_iter()
        .map(sound_name)
        .collect();
        let expected: Vec<_> = case["events"].as_array().unwrap().iter().map(|e| e.as_str().unwrap()).collect();
        assert_eq!(got, expected, "{case}");
    }
}

// --- Whole-game scenarios ---------------------------------------------------------------

/// Java `LivingEntity.knockback` built from the fixture's constants, for motion in blocks per tick.
struct Reference {
    divisor: f64,
    cap: f64,
}

impl Reference {
    fn new() -> Self {
        let k = &fixture()["living_entity_knockback"];
        Self { divisor: num(&k["horizontal_motion_divisor"]), cap: num(&k["vertical_cap"]) }
    }

    fn knock(&self, old: DVec3, strength: f64, resistance: f64, x: f64, z: f64, on_ground: bool) -> DVec3 {
        let strength = strength * (1.0 - resistance);
        if strength <= 0.0 {
            return old;
        }
        let away = DVec3::new(x, 0.0, z).normalize() * strength;
        let y = if on_ground { self.cap.min(old.y / self.divisor + strength) } else { old.y };
        DVec3::new(old.x / self.divisor - away.x, y, old.z / self.divisor - away.z)
    }
}

#[test]
fn the_reference_evaluator_reproduces_the_fixture_cases() {
    let r = Reference::new();
    for case in fixture()["cases"]["knockback"].as_array().unwrap() {
        if case["new_motion"].is_null() {
            continue;
        }
        let got = r.knock(
            vec3(&case["old_motion"]),
            num(&case["strength"]),
            num(&case["resistance"]),
            num(&case["x"]),
            num(&case["z"]),
            case["on_ground"].as_bool().unwrap(),
        );
        near(got, vec3(&case["new_motion"]), 1e-12, &case.to_string());
    }
}

fn scene() -> (World, PlayerSession, Entities) {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w.set_block(IVec3::new(4, 144, 4), Block::STONE);
    let mut s = PlayerSession::new(PlayerId(3), DVec3::new(4.5, 145., 4.5));
    s.player.on_ground = true;
    s.player.yaw = 0.0;
    s.player.pitch = 0.0;
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1));
    (w, s, Entities::new(7))
}

fn charge(s: &mut PlayerSession, ticks: u32) {
    for _ in 0..ticks {
        s.tick_attack_strength(0);
    }
}

/// A standing mob (on the ground, at rest) at `x`, `z`.
fn spawn(e: &mut Entities, kind: MobKind, x: f64, z: f64) -> usize {
    e.spawn(kind, DVec3::new(x, 145.0, z));
    let i = e.mobs.len() - 1;
    e.mobs[i].on_ground = true;
    e.mobs[i].vel = DVec3::ZERO;
    i
}

/// Pitch the player's view at the middle of mob `i`; the yaw stays along +X.
fn aim(s: &mut PlayerSession, e: &Entities, i: usize) {
    let m = &e.mobs[i];
    let to = m.pos + DVec3::Y * (m.shape().height * 0.5) - s.player.eye();
    s.player.pitch = to.y.atan2(to.x.hypot(to.z)) as f32;
}

fn enchanted(item: Item, enchants: &[(Enchantment, u8)]) -> Stack {
    let mut stack = Stack::new(item, 1);
    stack.enchants = enchants.iter().fold(Enchants::NONE, |e, &(k, l)| e.with(k, l));
    stack
}

fn tool_damage(id: &str) -> f64 {
    let tool = fixture()["tools"].as_array().unwrap().iter().find(|t| t["id"] == id).unwrap();
    num(&fixture()["player"]["attack_damage"]) + num(&tool["attack_damage_with_material"])
}

/// Java yaw -90 (VoxelCraft yaw 0, facing +X) as the fixture's push vector.
fn facing_east() -> (f64, f64) {
    let case =
        fixture()["cases"]["yaw_direction"].as_array().unwrap().iter().find(|c| c["yaw_degrees"] == -90.0).unwrap();
    (num(&case["push_x"]), num(&case["push_z"]))
}

fn attack_strength_case(level: u32, sprint: bool) -> f64 {
    let case = fixture()["cases"]["attack_knockback"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["attack_knockback_attribute"] == 0.0 && c["knockback_level"] == level && c["sprint_bonus"] == sprint
        })
        .unwrap();
    num(&case["strength"])
}

fn mob_motion(e: &Entities, i: usize) -> DVec3 {
    e.mobs[i].vel / knockback::TICK_RATE
}

#[test]
fn a_hit_pushes_from_the_attacker_position_not_along_the_view() {
    let r = Reference::new();
    let (w, mut s, mut e) = scene();
    // The mob's near edge is on the view ray, its feet a quarter block to one side.
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.75);
    aim(&mut s, &e, i);
    charge(&mut s, 20);
    let out = s.attack(0, &w, &mut e, false).unwrap();
    assert!(matches!(out, Outcome::Hit { target: Target::Mob(0), strong: true, sprint_knockback: false, .. }));
    // Only `hurtServer`'s 0.4 push, toward the attacker's feet: no Knockback, no sprint.
    let expected = r.knock(DVec3::ZERO, knockback::DEFAULT_STRENGTH, 0.0, 4.5 - 6.5, 4.5 - 4.75, true);
    near(mob_motion(&e, i), expected, 1e-9, "hurt knockback");
    assert!(expected.x > 0.0 && expected.z > 0.0 && (expected.z / expected.x - 0.125).abs() < 1e-9);
    assert_eq!(s.player.vel, DVec3::ZERO, "no extra knockback, no slowdown");
    assert!(matches!(out, Outcome::Hit { sprint_cancelled: false, .. }));
}

#[test]
fn sprint_and_the_knockback_enchantment_add_a_second_push_along_the_facing() {
    let r = Reference::new();
    let (fx, fz) = facing_east();
    for (sprint, level) in [(false, 1u8), (false, 2), (true, 0), (true, 1), (true, 2)] {
        let (w, mut s, mut e) = scene();
        s.inventory.slots[0] =
            Some(enchanted(Item::tool(ToolKind::Sword, Tier::Diamond), &[(Enchantment::Knockback, level)]));
        let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.75);
        aim(&mut s, &e, i);
        charge(&mut s, 20);
        s.player.vel = DVec3::new(4.0, 0.0, 2.0);
        let out = s.attack(0, &w, &mut e, sprint).unwrap();
        let strength = attack_strength_case(u32::from(level), sprint);
        let mut expected = r.knock(DVec3::ZERO, knockback::DEFAULT_STRENGTH, 0.0, 4.5 - 6.5, 4.5 - 4.75, true);
        expected = r.knock(expected, strength, 0.0, fx, fz, true);
        near(mob_motion(&e, i), expected, 1e-9, &format!("sprint {sprint} knockback {level}"));
        let Outcome::Hit { sprint_knockback, sprint_cancelled, .. } = out else { panic!("{out:?}") };
        assert_eq!(sprint_knockback, sprint, "only a sprinting strong attack gets the sprint bonus");
        assert!(sprint_cancelled, "causeExtraKnockback stops the sprint");
        // The attacker's horizontal motion is multiplied by 0.6.
        near(s.player.vel, DVec3::new(2.4, 0.0, 1.2), 1e-12, "attacker slowed");
    }
}

#[test]
fn a_hit_without_extra_knockback_leaves_the_attacker_moving() {
    let (w, mut s, mut e) = scene();
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    aim(&mut s, &e, i);
    charge(&mut s, 20);
    s.player.vel = DVec3::new(1.0, 0.0, 0.0);
    let out = s.attack(0, &w, &mut e, false).unwrap();
    assert!(matches!(out, Outcome::Hit { sprint_cancelled: false, .. }));
    assert_eq!(s.player.vel, DVec3::new(1.0, 0.0, 0.0));
}

#[test]
fn a_weak_sprinting_hit_gets_no_sprint_bonus_but_enchantment_knockback_applies() {
    let r = Reference::new();
    let (fx, fz) = facing_east();
    let (w, mut s, mut e) = scene();
    s.inventory.slots[0] = Some(enchanted(Item::tool(ToolKind::Sword, Tier::Diamond), &[(Enchantment::Knockback, 2)]));
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    aim(&mut s, &e, i);
    charge(&mut s, 2);
    assert!(s.attack_strength(0, 0.5) < 0.9);
    let out = s.attack(0, &w, &mut e, true).unwrap();
    let Outcome::Hit { strong, sprint_knockback, .. } = out else { panic!("{out:?}") };
    assert!(!strong && !sprint_knockback);
    let hurt = r.knock(DVec3::ZERO, knockback::DEFAULT_STRENGTH, 0.0, -2.0, 0.0, true);
    let expected = r.knock(hurt, attack_strength_case(2, false), 0.0, fx, fz, true);
    near(mob_motion(&e, i), expected, 1e-9, "low charge still knocks back");
}

#[test]
fn existing_motion_is_halved_by_each_push() {
    let r = Reference::new();
    let (fx, fz) = facing_east();
    let (w, mut s, mut e) = scene();
    s.inventory.slots[0] = Some(enchanted(Item::tool(ToolKind::Sword, Tier::Diamond), &[(Enchantment::Knockback, 1)]));
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    let old = DVec3::new(4.0, 2.0, -3.0);
    e.mobs[i].vel = old;
    aim(&mut s, &e, i);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    let hurt = r.knock(old / knockback::TICK_RATE, knockback::DEFAULT_STRENGTH, 0.0, -2.0, 0.0, true);
    let expected = r.knock(hurt, attack_strength_case(1, false), 0.0, fx, fz, true);
    near(mob_motion(&e, i), expected, 1e-9, "both pushes halve the motion before them");
}

#[test]
fn an_airborne_mob_keeps_its_vertical_motion() {
    let (w, mut s, mut e) = scene();
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    e.mobs[i].on_ground = false;
    e.mobs[i].vel = DVec3::new(0.0, 3.0, 0.0);
    aim(&mut s, &e, i);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    assert_eq!(e.mobs[i].vel.y, 3.0, "knockback leaves airborne vertical motion alone");
    assert!(e.mobs[i].vel.x > 0.0);
}

#[test]
fn knockback_resistant_mobs_take_the_reference_share() {
    let r = Reference::new();
    let attributes = &fixture()["mob_melee"]["attributes"];
    let golem = num(&attributes["iron_golem"]["knockback_resistance"]);
    let hoglin = num(&attributes["hoglin"]["knockback_resistance"]);
    assert_eq!(golem, 1.0);
    for (kind, resistance) in [(MobKind::IronGolem, golem), (MobKind::Hoglin, hoglin), (MobKind::Zoglin, hoglin)] {
        let (w, mut s, mut e) = scene();
        let i = spawn(&mut e, kind, 6.5, 4.5);
        aim(&mut s, &e, i);
        charge(&mut s, 20);
        let out = s.attack(0, &w, &mut e, false).unwrap();
        assert!(matches!(out, Outcome::Hit { target: Target::Mob(0), .. }), "{kind:?}: {out:?}");
        let expected = r.knock(DVec3::ZERO, knockback::DEFAULT_STRENGTH, resistance, -2.0, 0.0, true);
        near(mob_motion(&e, i), expected, 1e-9, &format!("{kind:?}"));
        if kind == MobKind::IronGolem {
            assert_eq!(e.mobs[i].vel, DVec3::ZERO, "a resistance of 1 cancels every push");
        }
    }
}

// --- Sweeping ---------------------------------------------------------------------------

/// A target at x = 6.5 with neighbours: swept (`inside`), too far from the box, too far from the player, above.
struct Crowd {
    target: usize,
    beside: usize,
    outside_box: usize,
    too_far: usize,
    just_above: usize,
    too_high: usize,
}

fn crowd(e: &mut Entities) -> Crowd {
    let target = spawn(e, MobKind::Zombie, 6.5, 4.5);
    let beside = spawn(e, MobKind::Zombie, 6.5, 5.5);
    let outside_box = spawn(e, MobKind::Zombie, 6.5, 6.2);
    let too_far = spawn(e, MobKind::Zombie, 8.3, 4.5);
    let just_above = spawn(e, MobKind::Zombie, 6.5, 4.5);
    e.mobs[just_above].pos.y = 147.1;
    let too_high = spawn(e, MobKind::Zombie, 5.5, 3.5);
    e.mobs[too_high].pos.y = 147.3;
    Crowd { target, beside, outside_box, too_far, just_above, too_high }
}

fn lost(e: &Entities, i: usize) -> f32 {
    MobKind::Zombie.max_health() - e.mobs[i].health
}

#[test]
fn a_sweep_hits_the_reference_victims_for_the_reference_damage() {
    let r = Reference::new();
    let base = tool_damage("diamond_sword");
    assert_eq!(base, 7.0);
    for level in [0u8, 1, 3] {
        let (w, mut s, mut e) = scene();
        s.inventory.slots[0] =
            Some(enchanted(Item::tool(ToolKind::Sword, Tier::Diamond), &[(Enchantment::SweepingEdge, level)]));
        let c = crowd(&mut e);
        aim(&mut s, &e, c.target);
        charge(&mut s, 20);
        let out = s.attack(0, &w, &mut e, false).unwrap();
        let Outcome::Hit { target: Target::Mob(t), sweep, strong, .. } = out else { panic!("{out:?}") };
        assert_eq!(t, c.target);
        assert!(sweep && strong);
        let swept = f64::from(knockback::sweep_damage(base as f32, u32::from(level), 0.0, 1.0));
        let ratio =
            if level == 0 { 0.0 } else { num(&fixture()["sweep_attack"]["sweeping_edge_ratio"][level.to_string()]) };
        assert!((swept - (1.0 + ratio * base)).abs() < 1e-5, "1 + ratio x damage");
        assert!((f64::from(lost(&e, c.target)) - base).abs() < 1e-4, "the target takes the full hit");
        assert!((f64::from(lost(&e, c.beside)) - swept).abs() < 1e-4, "level {level}: {}", lost(&e, c.beside));
        for (name, i) in
            [("outside the box", c.outside_box), ("beyond three blocks", c.too_far), ("above the box", c.too_high)]
        {
            assert_eq!(lost(&e, i), 0.0, "{name} is not swept");
        }
        assert!((f64::from(lost(&e, c.just_above)) - swept).abs() < 1e-4, "the box reaches a quarter block higher");
        // Each victim is pushed away from the attacker, then along its facing.
        let (fx, fz) = facing_east();
        for i in [c.beside, c.just_above] {
            let hurt = r.knock(
                DVec3::ZERO,
                knockback::DEFAULT_STRENGTH,
                0.0,
                4.5 - e.mobs[i].pos.x,
                4.5 - e.mobs[i].pos.z,
                true,
            );
            let expected = r.knock(hurt, num(&fixture()["sweep_attack"]["knockback"]["strength"]), 0.0, fx, fz, true);
            near(mob_motion(&e, i), expected, 1e-9, "swept knockback");
        }
        assert_eq!(e.mobs[c.too_far].vel, DVec3::ZERO);
    }
}

#[test]
fn swept_victims_take_their_own_enchantment_bonus_scaled_by_the_charge() {
    let (w, mut s, mut e) = scene();
    s.inventory.slots[0] = Some(enchanted(
        Item::tool(ToolKind::Sword, Tier::Diamond),
        &[(Enchantment::Smite, 5), (Enchantment::SweepingEdge, 2)],
    ));
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    let strength = s.attack_strength(0, 0.5);
    s.attack(0, &w, &mut e, false).unwrap();
    let smite = voxelcraft::enchant::damage_bonus(
        Enchants::NONE.with(Enchantment::Smite, 5),
        voxelcraft::entity::MobKind::Zombie.creature(),
    );
    assert!(smite > 0.0);
    let expected = knockback::sweep_damage(7.0, 2, smite, strength);
    assert!((lost(&e, c.beside) - expected).abs() < 1e-4, "{} vs {expected}", lost(&e, c.beside));
}

#[test]
fn sweeping_needs_a_strong_grounded_unsprinted_slow_sword_attack() {
    let sweeps = |setup: &dyn Fn(&mut PlayerSession), sprint: bool, ticks: u32| {
        let (w, mut s, mut e) = scene();
        let c = crowd(&mut e);
        aim(&mut s, &e, c.target);
        charge(&mut s, ticks);
        setup(&mut s);
        let out = s.attack(0, &w, &mut e, sprint).unwrap();
        let Outcome::Hit { sweep, .. } = out else { panic!("{out:?}") };
        assert_eq!(sweep, lost(&e, c.beside) > 0.0, "the outcome reports the sweep that happened");
        sweep
    };
    let nothing: &dyn Fn(&mut PlayerSession) = &|_| {};
    assert!(sweeps(nothing, false, 20), "baseline");
    assert!(!sweeps(nothing, false, 3), "a weak attack does not sweep");
    assert!(!sweeps(nothing, true, 20), "a sprint knockback attack does not sweep");
    assert!(!sweeps(&|s| s.player.on_ground = false, false, 20), "airborne");
    // Horizontal movement under 2.5 x speed (0.1 blocks per tick = 5 blocks per second) still sweeps.
    assert!(sweeps(&|s| s.player.vel = DVec3::new(4.9, 3.0, 0.0), false, 20), "vertical speed is not counted");
    assert!(!sweeps(&|s| s.player.vel = DVec3::new(5.1, 0.0, 0.0), false, 20), "moving faster than 2.5 x speed");
    // Speed raises the limit; Slowness lowers it.
    assert!(
        sweeps(
            &|s| {
                s.vitals.effects.add(Effect::Speed, 0, 600);
                s.player.vel = DVec3::new(5.5, 0.0, 0.0)
            },
            false,
            20
        ),
        "Speed I: limit 0.3 blocks per tick"
    );
    assert!(
        !sweeps(
            &|s| {
                s.vitals.effects.add(Effect::Slowness, 1, 600);
                s.player.vel = DVec3::new(4.0, 0.0, 0.0)
            },
            false,
            20
        ),
        "Slowness II: limit 0.175 blocks per tick"
    );
}

#[test]
fn only_swords_sweep_and_every_reference_sword_does() {
    let swords = fixture()["sweep_attack"]["sword_items"].as_array().unwrap();
    assert_eq!(swords.len(), 7);
    let sweeps = |item: Item| {
        let (w, mut s, mut e) = scene();
        s.inventory.slots[0] = Some(Stack::new(item, 1));
        let c = crowd(&mut e);
        aim(&mut s, &e, c.target);
        charge(&mut s, 40);
        s.attack(0, &w, &mut e, false).unwrap();
        lost(&e, c.beside) > 0.0
    };
    for id in swords {
        let name = id.as_str().unwrap().trim_start_matches("minecraft:");
        let item = Item::from_name(name).unwrap_or_else(|| panic!("{name} is registered"));
        assert!(sweeps(item), "{name} sweeps");
    }
    for tool in [ToolKind::Axe, ToolKind::Pickaxe, ToolKind::Shovel, ToolKind::Hoe] {
        assert!(!sweeps(Item::tool(tool, Tier::Diamond)), "{tool:?} does not sweep");
    }
    let (w, mut s, mut e) = scene();
    s.inventory.slots[0] = None;
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 40);
    s.attack(0, &w, &mut e, false).unwrap();
    assert_eq!(lost(&e, c.beside), 0.0, "a bare hand does not sweep");
}

#[test]
fn a_sweep_angers_zombified_piglins_like_any_player_hit() {
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    // A piglin the sword sweeps, and one far outside the sweep but within 32 blocks.
    let swept = spawn(&mut e, MobKind::ZombifiedPiglin, 6.5, 5.3);
    let far = spawn(&mut e, MobKind::ZombifiedPiglin, 20.5, 14.5);
    assert!(!e.mobs[far].is_provoked());
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    assert!(lost_kind(&e, swept) > 0.0, "the piglin was swept");
    assert!(e.mobs[far].is_provoked(), "a swept zombified piglin angers its neighbours");
}

#[test]
fn a_swept_iron_golem_turns_on_the_attacker_not_the_host() {
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    let golem = spawn(&mut e, MobKind::IronGolem, 6.5, 5.3);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    assert!(lost_kind(&e, golem) > 0.0, "the golem was swept");
    assert_eq!(e.mobs[golem].angry_at(), Some(PlayerId(3)));
}

#[test]
fn a_swept_wild_wolf_turns_on_the_attacker() {
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    let wolf = spawn(&mut e, MobKind::Wolf, 6.5, 5.3);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    assert!(lost_kind(&e, wolf) > 0.0, "the wolf was swept");
    assert_eq!(e.mobs[wolf].angry_at(), Some(PlayerId(3)), "sweep damage carries the player's damage source");
}

#[test]
fn full_knockback_resistance_leaves_an_iron_golems_motion_alone() {
    let mut e = Entities::new(5);
    let i = spawn(&mut e, MobKind::IronGolem, 6.5, 4.5);
    e.mobs[i].vel = DVec3::new(2.0, -3.0, 1.0);
    let mut rng = voxelcraft::entity::Rng::new(1);
    e.mobs[i].damage(1.0, Some(DVec3::new(4.0, 3.0, 0.0)), &mut rng);
    // `LivingEntity.knockback` returns before touching the motion when resistance cancels the strength.
    assert_eq!(e.mobs[i].vel, DVec3::new(2.0, -3.0, 1.0));
}

fn lost_kind(e: &Entities, i: usize) -> f32 {
    e.mobs[i].kind.max_health() - e.mobs[i].health
}

#[test]
fn the_sweep_particle_appears_ahead_of_the_attacker() {
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    // Java: (x - sin(yaw), y + 0.5 * height, z + cos(yaw)); facing +X is Java yaw -90.
    let (sin, cos) = {
        let case =
            fixture()["cases"]["yaw_direction"].as_array().unwrap().iter().find(|c| c["yaw_degrees"] == -90.0).unwrap();
        (num(&case["sin"]), num(&case["cos"]))
    };
    let expected = DVec3::new(4.5 - sin, 145.0 + 0.5 * s.player.collision_shape().height, 4.5 + cos);
    assert!(expected.x > 5.4, "ahead of the attacker, not behind");
    let found = e.particles.drain().any(|request| {
        matches!(&request, voxelcraft::particles::Request::Burst(b)
            if b.kind == voxelcraft::particles::Kind::Poof && (b.pos - expected).abs().max_element() < 1e-6)
    });
    assert!(found, "no sweep particle at {expected:?}");
}

#[test]
fn a_hit_with_extra_knockback_stops_an_agent_sprinting() {
    use voxelcraft::player::MoveInput;
    let (mut w, _, mut e) = scene();
    let mut a = Agent::new(DVec3::new(4.5, 145., 4.5));
    a.player.on_ground = true;
    a.player.yaw = 0.0;
    a.session.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1));
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    aim(&mut a.session, &e, i);
    charge(&mut a.session, 20);
    a.hold(MoveInput { forward: 1.0, sprint: true, ..Default::default() }, false, false);
    assert!(a.movement_input().sprint);
    a.execute(Command::Attack, &mut w, &mut e, &[]).unwrap();
    assert!(!a.movement_input().sprint, "sprint knockback stops the sprint (setSprinting(false))");
}

#[test]
fn a_critical_hit_never_sweeps() {
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    // Falling makes the hit critical and takes the player off the ground.
    s.player.on_ground = false;
    s.vitals.tick(
        0.05,
        &voxelcraft::simulation::survival::Env { y: 147.0, on_ground: true, ..Default::default() },
        false,
    );
    s.vitals.tick(
        0.05,
        &voxelcraft::simulation::survival::Env { y: 145.0, on_ground: false, ..Default::default() },
        false,
    );
    let Outcome::Hit { critical, sweep, .. } = s.attack(0, &w, &mut e, false).unwrap() else { panic!() };
    assert!(critical && !sweep);
    assert_eq!(lost(&e, c.beside), 0.0);
}

#[test]
fn outcome_sounds_follow_the_reference_table() {
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    let out = s.attack(0, &w, &mut e, false).unwrap();
    let names: Vec<_> = out.sounds().into_iter().map(sound_name).collect();
    assert_eq!(names, ["player.attack.sweep"], "a sweep replaces the strong or weak sound");
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 20);
    let names: Vec<_> = s.attack(0, &w, &mut e, true).unwrap().sounds().into_iter().map(sound_name).collect();
    assert_eq!(names, ["player.attack.knockback", "player.attack.strong"]);
    let (w, mut s, mut e) = scene();
    let c = crowd(&mut e);
    aim(&mut s, &e, c.target);
    charge(&mut s, 2);
    let names: Vec<_> = s.attack(0, &w, &mut e, true).unwrap().sounds().into_iter().map(sound_name).collect();
    assert_eq!(names, ["player.attack.weak"], "a weak sprinting hit has no knockback sound");
}

#[test]
fn the_cli_agent_attacks_through_the_same_knockback_path() {
    let r = Reference::new();
    let (fx, fz) = facing_east();
    let (mut w, _, mut e) = scene();
    let mut a = Agent::new(DVec3::new(4.5, 145., 4.5));
    a.player.on_ground = true;
    a.player.yaw = 0.0;
    a.session.inventory.slots[0] =
        Some(enchanted(Item::tool(ToolKind::Sword, Tier::Diamond), &[(Enchantment::Knockback, 1)]));
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    aim(&mut a.session, &e, i);
    charge(&mut a.session, 20);
    a.execute(Command::Attack, &mut w, &mut e, &[]).unwrap();
    let hurt = r.knock(DVec3::ZERO, knockback::DEFAULT_STRENGTH, 0.0, -2.0, 0.0, true);
    let expected = r.knock(hurt, attack_strength_case(1, false), 0.0, fx, fz, true);
    near(mob_motion(&e, i), expected, 1e-9, "agent attack");
}

#[test]
fn an_agent_hit_harder_inside_its_immunity_window_keeps_its_motion_and_hurt_direction() {
    let (_, _, mut e) = scene();
    let mut a = Agent::new(DVec3::new(4.5, 145., 4.5));
    a.player.on_ground = true;
    a.player.yaw = 0.0;
    let east = DVec3::new(10.5, 145.0, 4.5);
    const CAUSE: &str = "was slain by a zombie";
    assert_eq!(a.hurt_knocked(2.0, CAUSE, &Hit::melee(east, a.player.pos, -90.0, 0.0), &mut e), 2.0);
    let direction = a.player.animation.hurt_direction;
    assert_ne!(direction, 0.0, "pushed from the east");
    let moving = DVec3::new(3.0, 0.0, -1.0);
    a.player.vel = moving;
    assert_eq!(a.hurt_knocked(4.0, CAUSE, &Hit::melee(east, a.player.pos, -90.0, 0.0), &mut e), 2.0);
    near(a.player.vel, moving, 1e-12, "no default push inside the window");
    assert_eq!(a.player.animation.hurt_direction, direction, "Java leaves the hurt direction alone");
}

// --- Mobs and projectiles hitting players -----------------------------------------------

fn night(player: DVec3) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        players: vec![MobTarget::new(PlayerId::HOST, player, true)],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

struct Flat;
impl voxelcraft::physics::BlockSource for Flat {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(if p.y < 1 { Block::STONE } else { Block::AIR })
    }
}
impl voxelcraft::entity::MobWorld for Flat {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
}

fn first_hit(e: &mut Entities, ctx: &Ctx) -> (f32, Option<Hit>, DVec3) {
    for _ in 0..200 {
        let events = e.update(0.05, &Flat, ctx);
        for event in events {
            if let EntityEvent::PlayerHit { java, damage, .. } = event {
                return (damage, java, e.mobs[0].pos);
            }
        }
    }
    panic!("the mob never hit");
}

#[test]
fn a_mob_melee_hit_is_the_default_push_away_from_the_mob() {
    let target = DVec3::new(1.5, 1.0, 0.5);
    let mut e = Entities::new(3);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5));
    let (_, java, _) = first_hit(&mut e, &night(target));
    let hit = java.expect("mob melee delivers Java knockback");
    let push = hit.pushes[0].expect("hurtServer's push");
    assert_eq!(push.strength, knockback::DEFAULT_STRENGTH);
    // The vector points from the player toward the zombie, so the player moves away from it.
    assert!(push.x < 0.0 && push.z.abs() < 0.2, "{push:?}");
    assert_eq!(hit.pushes[1], None, "a zombie's attack_knockback attribute is 0");
    assert_eq!((hit.punch, hit.throw), (None, None));
}

#[test]
fn an_angry_wolf_bites_with_the_mob_melee_push() {
    let target = DVec3::new(1.5, 1.0, 0.5);
    let mut e = Entities::new(3);
    e.spawn(MobKind::Wolf, DVec3::new(0.5, 1.0, 0.5));
    let punch = PlayerMelee {
        held: None,
        bonus: 0.0,
        strength: 1.0,
        critical: false,
        sprint_knockback: false,
        origin: target,
        yaw: 90.0,
        sweep: false,
        owner: PlayerId::HOST,
    };
    e.player_melee(0, punch);
    assert_eq!(e.mobs[0].angry_at(), Some(PlayerId::HOST), "a hit wild wolf turns on the player");
    let (_, java, _) = first_hit(&mut e, &night(target));
    // `Wolf` has no `doHurtTarget` of its own, so it bites like any mob.
    let hit = java.expect("a wolf bite delivers Java knockback");
    let push = hit.pushes[0].expect("hurtServer's push");
    assert_eq!(push.strength, knockback::DEFAULT_STRENGTH);
    assert!(push.x < 0.0 && push.z.abs() < 0.2, "{push:?}");
    assert_eq!(hit.pushes[1], None, "a wolf's attack_knockback attribute is 0");
}

#[test]
fn a_stronger_hit_inside_the_immunity_window_skips_the_default_push() {
    let r = Reference::new();
    let (_, mut s, _) = scene();
    let source = DVec3::new(10.5, 145.0, 4.5);
    const CAUSE: &str = "was slain by a zombie";
    assert_eq!(s.vitals.damage(2.0, CAUSE, false), 2.0);
    s.knocked_back(&Hit::melee(source, s.player.pos, -90.0, 0.0), &mut no_jitter());
    assert!(s.player.vel.x < 0.0, "the first hit pushes the player away from the source");
    // `hurtServer`: a stronger hit inside the window deals the difference and skips the default push...
    let moving = DVec3::new(3.0, 0.0, -1.0);
    s.player.vel = moving;
    assert_eq!(s.vitals.damage(4.0, CAUSE, false), 2.0);
    s.knocked_back(&Hit::melee(source, s.player.pos, -90.0, 0.0), &mut no_jitter());
    near(s.player.vel, moving, 1e-12, "no default push inside the window");
    assert_ne!(s.player.animation.hurt_direction, 0.0, "the first hit's hurt direction stays");
    // ...while the attacker's extra knockback, which follows any accepted hurt, still lands.
    assert_eq!(s.vitals.damage(6.0, CAUSE, false), 2.0);
    s.knocked_back(&Hit::melee(source, s.player.pos, -90.0, 1.0), &mut no_jitter());
    let (x, z) = facing_east();
    let expected = r.knock(moving / knockback::TICK_RATE, 1.0, 0.0, x, z, true);
    near(s.player.vel / knockback::TICK_RATE, expected, 1e-6, "only the extra push");
}

#[test]
fn a_hoglin_hit_adds_the_reference_toss_and_no_extra_knockback() {
    let target = DVec3::new(2.0, 1.0, 0.5);
    let mut e = Entities::new(11);
    e.spawn(MobKind::Hoglin, DVec3::new(0.5, 1.0, 0.5));
    let (_, java, _) = first_hit(&mut e, &night(target));
    let hit = java.expect("Java knockback");
    assert_eq!(hit.pushes[1], None, "HoglinBase.hurtAndThrowTarget has no causeExtraKnockback");
    let toss = hit.throw.expect("the tusk toss");
    assert_eq!(toss.strength, num(&fixture()["mob_melee"]["attributes"]["hoglin"]["attack_knockback"]));
    assert!((-10.0..=10.0).contains(&toss.turn) && toss.turn.fract() == 0.0, "nextInt(21) - 10: {}", toss.turn);
    assert!(toss.dx > 0.0);
}

#[test]
fn a_player_hit_by_a_mob_moves_by_the_reference_formula_with_armor_resistance() {
    let r = Reference::new();
    let netherite = num(&fixture()["armor_materials"]["netherite"]["knockback_resistance"]);
    assert!((netherite - 0.1).abs() < 1e-7);
    for pieces in [0usize, 4] {
        let (_, mut s, _) = scene();
        for (i, piece) in ArmorPiece::ALL.into_iter().enumerate().take(pieces) {
            s.inventory.armor[i] = Some(Stack::new(Item::armor(piece, ArmorMaterial::Netherite), 1));
        }
        let resistance = netherite * pieces as f64;
        s.player.vel = DVec3::new(3.0, 0.0, -1.0);
        let hit = Hit::melee(DVec3::new(10.5, 145.0, 4.5), s.player.pos, -90.0, 0.0);
        s.knocked_back(&hit, &mut no_jitter());
        let (x, z) = (10.5 - 4.5, 0.0);
        let expected = r.knock(DVec3::new(0.15, 0.0, -0.05), knockback::DEFAULT_STRENGTH, resistance, x, z, true);
        near(s.player.vel / knockback::TICK_RATE, expected, 1e-6, &format!("{pieces} netherite pieces"));
    }
}

#[test]
fn arrow_punch_adds_its_shove_after_the_default_push() {
    let r = Reference::new();
    let arrow = DVec3::new(1.0, -0.5, 0.0);
    let cases = fixture()["cases"]["arrow_punch"].as_array().unwrap();
    let case = cases
        .iter()
        .find(|c| c["punch_level"] == 2 && c["resistance"] == 0.0 && vec3(&c["arrow_motion"]) == arrow)
        .unwrap();
    let shove = vec3(&case["push"]);
    // A mob.
    let mut e = Entities::new(5);
    let i = spawn(&mut e, MobKind::Zombie, 6.5, 4.5);
    let mut rng = voxelcraft::entity::Rng::new(1);
    e.mobs[i].damage_pushed(1.0, &[Push::projectile(arrow)], &mut rng);
    e.mobs[i].shove_punch(2, arrow);
    let expected = r.knock(DVec3::ZERO, knockback::DEFAULT_STRENGTH, 0.0, -1.0, 0.0, true) + shove;
    near(mob_motion(&e, i), expected, 1e-9, "mob");
    // A player, whose armor resistance scales only the shove it adds to (and the default push).
    let (_, mut s, _) = scene();
    s.knocked_back(&Hit::arrow(arrow, 2), &mut no_jitter());
    near(s.player.vel / knockback::TICK_RATE, expected, 1e-9, "player");
}

#[test]
fn punch_arrows_throw_mobs_further_than_plain_arrows() {
    let shoot = |punch: u8| {
        let mut e = Entities::new(9);
        e.spawn(MobKind::Zombie, DVec3::new(8.5, 1.0, 0.5));
        e.mobs[0].on_ground = true;
        e.mobs[0].persistent = true;
        let enchants = Enchants::NONE.with(Enchantment::Punch, punch);
        e.shoot_enchanted_for(DVec3::new(0.5, 2.0, 0.5), DVec3::X, 1.0, false, enchants, PlayerId::HOST);
        let mut vel = DVec3::ZERO;
        for _ in 0..40 {
            let hit = e.update(0.05, &Flat, &night(DVec3::new(100.0, 1.0, 100.0)));
            if hit.iter().any(|ev| matches!(ev, EntityEvent::MobShot { .. })) {
                vel = e.mobs[0].vel;
                break;
            }
        }
        vel
    };
    let (plain, punched) = (shoot(0), shoot(2));
    assert!(plain.x > 0.0, "the default push moves the mob along the arrow: {plain:?}");
    // Punch II adds 2 x 0.6 blocks per tick = 24 blocks per second.
    assert!(punched.x - plain.x > 20.0, "{plain:?} {punched:?}");
}
