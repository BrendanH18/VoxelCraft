//! Shared melee against pinned Java 26.3 facts (`fixtures/java/26.3/combat.json`).
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Entities, MobKind, PlayerId},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    mining,
    rules::GameMode,
    session::{
        PlayerSession,
        combat::{Outcome, Target},
    },
    simulation::{effects::Effect, survival::Env},
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/combat.json")).unwrap()
}

/// A player on a stone pad at x=4.5 looking along +x at an iron golem (no
/// armor, tall enough to meet a level look ray).
fn scene(golem_x: f64) -> (World, PlayerSession, Entities) {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w.set_block(IVec3::new(4, 144, 4), Block::STONE);
    let mut s = PlayerSession::new(PlayerId(3), DVec3::new(4.5, 145., 4.5));
    s.player.on_ground = true;
    s.player.yaw = 0.0;
    s.player.pitch = 0.0;
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1));
    let mut e = Entities::new(7);
    e.spawn(MobKind::IronGolem, DVec3::new(golem_x, 145., 4.5));
    (w, s, e)
}

fn charge(s: &mut PlayerSession, ticks: u32) {
    for _ in 0..ticks {
        s.tick_attack_strength(0);
    }
}

/// Java 26.3 `Player.attack` damage for a hit without enchantments.
fn reference_damage(base: f32, strength: f32, critical: bool) -> f32 {
    base * (0.2 + strength * strength * 0.8) * if critical { 1.5 } else { 1.0 }
}

#[test]
fn every_registered_tool_has_reference_attack_attributes() {
    let f = fixture();
    let base_damage = f["player"]["attack_damage"].as_f64().unwrap() as f32;
    let base_speed = f["player"]["attack_speed"].as_f64().unwrap();
    assert_eq!(mining::attack_damage(None), base_damage);
    assert_eq!(mining::attack_speed(None), base_speed);
    let (mut checked, mut missing) = (0, Vec::new());
    for tool in f["tools"].as_array().unwrap() {
        let id = tool["id"].as_str().unwrap();
        let Some(item) = Item::from_name(id) else {
            missing.push(id);
            continue;
        };
        let damage = base_damage + tool["attack_damage_with_material"].as_f64().unwrap() as f32;
        // Java stores the float modifier in a double attribute.
        let speed = base_speed + f64::from(tool["attack_speed_modifier"].as_f64().unwrap() as f32);
        assert_eq!(mining::attack_damage(Some(item)), damage, "{id} damage");
        assert_eq!(mining::attack_speed(Some(item)), speed, "{id} speed");
        checked += 1;
    }
    assert_eq!(checked, 35, "every reference tool is registered; unregistered: {missing:?}");
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn entity_reach_is_the_reference_interaction_range() {
    let f = fixture();
    let survival = f["player"]["default_entity_interaction_range"].as_f64().unwrap();
    let creative = survival + f["player"]["creative_entity_interaction_range_modifier_value"].as_f64().unwrap();
    // Golem's near face at 3.65 blocks from the eye: inside Creative reach only.
    let (w, mut s, mut e) = scene(4.5 + 3.65 + 0.35);
    assert_eq!(s.entity_reach(), survival);
    charge(&mut s, 20);
    assert_eq!(s.attack(0, &w, &mut e, false), Ok(Outcome::Missed { player: PlayerId(3) }));
    assert_eq!(e.mobs[0].health, MobKind::IronGolem.max_health());
    s.set_mode(GameMode::Creative);
    assert_eq!(s.entity_reach(), creative);
    assert!(matches!(s.attack(0, &w, &mut e, false), Ok(Outcome::Hit { target: Target::Mob(0), .. })));
    assert!(e.mobs[0].health < MobKind::IronGolem.max_health());
}

#[test]
fn damage_scales_with_the_attack_strength_charge() {
    let sword = Some(Item::tool(ToolKind::Sword, Tier::Diamond));
    let delay = mining::attack_strength_delay(sword);
    assert_eq!(delay, (1.0 / mining::attack_speed(sword) * 20.0) as f32);
    for ticks in [1, 4, 8, 13, 40] {
        let (w, mut s, mut e) = scene(6.0);
        charge(&mut s, ticks);
        // The first tick sees the newly held sword and restarts the charge.
        let strength = ((ticks - 1) as f32 + 0.5) / delay;
        let strength = strength.clamp(0.0, 1.0);
        assert_eq!(s.attack_strength(0, 0.5), strength);
        let out = s.attack(0, &w, &mut e, false).unwrap();
        let Outcome::Hit { strength: used, critical, .. } = out else { panic!("{out:?}") };
        assert_eq!((used, critical), (strength, false));
        let expected = MobKind::IronGolem.max_health() - reference_damage(7.0, strength, false);
        assert!((e.mobs[0].health - expected).abs() < 1e-4, "{ticks} ticks: {} vs {expected}", e.mobs[0].health);
        // Attacking restarts the charge.
        assert_eq!(s.attack_strength(0, 0.0), 0.0);
    }
}

/// Drops the scene's player `blocks` onto the stone pad through the real vitals tracking.
fn fall(s: &mut PlayerSession, blocks: f64) {
    let ground = s.player.pos.y;
    let env = |y: f64, on_ground: bool, in_water: bool| Env { y, on_ground, in_water, ..Env::default() };
    s.vitals.tick(0.05, &env(ground + blocks, true, false), false);
    s.player.on_ground = false;
    s.vitals.tick(0.05, &env(ground, false, false), false);
}

#[test]
fn criticals_need_a_strong_falling_unsprinted_attack() {
    for (ticks, sprinting, expect_critical) in [(1, false, false), (20, false, true), (20, true, false)] {
        let (w, mut s, mut e) = scene(6.0);
        charge(&mut s, ticks);
        fall(&mut s, 2.0);
        let strength = s.attack_strength(0, 0.5);
        let Outcome::Hit { critical, .. } = s.attack(0, &w, &mut e, sprinting).unwrap() else { panic!() };
        assert_eq!(critical, expect_critical, "ticks {ticks} sprinting {sprinting}");
        let expected = MobKind::IronGolem.max_health() - reference_damage(7.0, strength, expect_critical);
        assert!((e.mobs[0].health - expected).abs() < 1e-4);
    }
}

/// Java 26.3 `Player.canCriticalAttack` reads `fallDistance`, not the vertical velocity.
#[test]
fn criticals_follow_fall_distance_and_not_velocity() {
    let critical = |setup: &dyn Fn(&mut PlayerSession)| {
        let (w, mut s, mut e) = scene(6.0);
        charge(&mut s, 20);
        setup(&mut s);
        let Outcome::Hit { critical, .. } = s.attack(0, &w, &mut e, false).unwrap() else { panic!() };
        critical
    };
    // Fallen two blocks and momentarily motionless (a bounce apex): still a critical.
    assert!(critical(&|s| {
        fall(s, 2.0);
        s.player.vel.y = 0.0;
    }));
    // Rising after a jump has no fall distance, whatever the player's pose.
    assert!(!critical(&|s| {
        s.player.on_ground = false;
        s.vitals.tick(0.05, &Env { y: 145.0, ..Env::default() }, false);
        s.player.vel.y = 6.0;
    }));
    // Fast downward velocity without a fall (teleported, flying) is no critical.
    assert!(!critical(&|s| {
        s.player.on_ground = false;
        s.vitals.reset_fall();
        s.player.vel.y = -8.0;
    }));
    // Landing in water, a ladder or flight clears the fall distance.
    assert!(!critical(&|s| {
        fall(s, 3.0);
        s.vitals.tick(0.05, &Env { y: 145.0, in_water: true, ..Env::default() }, false);
    }));
    assert!(!critical(&|s| {
        fall(s, 3.0);
        s.vitals.tick(0.05, &Env { y: 145.0, climbing: true, ..Env::default() }, false);
    }));
    assert!(!critical(&|s| {
        fall(s, 3.0);
        s.player.in_water = true;
    }));
    // Passengers never land criticals.
    assert!(!critical(&|s| {
        fall(s, 3.0);
        s.player.vehicle = Some(1);
    }));
    assert_eq!(fixture()["criticals"]["needs"][0], "fall_distance > 0");
}

#[test]
fn haste_and_mining_fatigue_scale_the_attack_speed_attribute() {
    let f = fixture();
    assert_eq!(f["attack_speed_enchantments"], serde_json::json!([]), "Java 26.3 defines none");
    let amount = |name: &str| f["attack_speed_effects"][name]["amount_per_level"].as_f64().unwrap();
    let [lo, hi] = [0, 1].map(|i| f["attack_speed_range"][i].as_f64().unwrap());
    let sword = Some(Item::tool(ToolKind::Sword, Tier::Diamond));
    let speed = mining::attack_speed(sword);
    let with = |effects: &[(Effect, u8)]| {
        let mut e = voxelcraft::simulation::effects::Effects::default();
        for &(effect, amplifier) in effects {
            e.add(effect, amplifier, 100);
        }
        mining::attack_speed_with(sword, &e)
    };
    assert_eq!(with(&[]), speed);
    for amplifier in 0..5u8 {
        let level = f64::from(amplifier) + 1.0;
        let haste = (speed * (1.0 + amount("haste") * level)).clamp(lo, hi);
        assert_eq!(with(&[(Effect::Haste, amplifier)]), haste, "Haste {level}");
        let fatigue = (speed * (1.0 + amount("mining_fatigue") * level)).clamp(lo, hi);
        assert_eq!(with(&[(Effect::MiningFatigue, amplifier)]), fatigue, "Mining Fatigue {level}");
    }
    // Opposite modifiers multiply: Haste II with Mining Fatigue I.
    let both = speed * (1.0 + amount("haste") * 2.0) * (1.0 + amount("mining_fatigue"));
    assert!((with(&[(Effect::Haste, 1), (Effect::MiningFatigue, 0)]) - both).abs() < 1e-12);
    // Ten levels of Mining Fatigue remove all of the speed; the attribute floors at zero.
    assert_eq!(with(&[(Effect::MiningFatigue, 9)]), 0.0);
    assert_eq!(with(&[(Effect::MiningFatigue, 200)]), 0.0);
    // The charge, its delay and the crosshair indicator all read the modified speed.
    let (_, mut s, _) = scene(6.0);
    charge(&mut s, 1);
    let plain = s.attack_strength_delay(0);
    s.vitals.effects.add(Effect::Haste, 1, 1000);
    let hasty = s.attack_strength_delay(0);
    assert_eq!(hasty, (1.0 / with(&[(Effect::Haste, 1)]) * 20.0) as f32);
    assert!(hasty < plain);
    charge(&mut s, 9);
    assert!(s.attack_strength(0, 0.5) > 0.9, "Haste II recharges a sword in about ten ticks");
    s.vitals.effects.add(Effect::MiningFatigue, 9, 1000);
    assert!(s.attack_strength_delay(0).is_infinite());
    assert_eq!(s.attack_strength(0, 0.5), 0.0, "a zero attack speed never charges");
}

#[test]
fn misses_and_different_items_restart_the_charge() {
    let (w, mut s, mut e) = scene(20.0);
    charge(&mut s, 20);
    assert!(s.attack_strength(0, 0.5) > 0.9);
    assert_eq!(s.attack(0, &w, &mut e, false), Ok(Outcome::Missed { player: PlayerId(3) }));
    assert_eq!(s.attack_strength(0, 0.0), 0.0);
    charge(&mut s, 20);
    // Another stack of the same item keeps the charge; a different item restarts it.
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1));
    s.tick_attack_strength(0);
    assert!(s.attack_strength(0, 0.5) > 0.9);
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Diamond), 1));
    s.tick_attack_strength(0);
    assert_eq!(s.attack_strength(0, 0.0), 0.0);
}

#[test]
fn hits_spend_reference_exhaustion_and_durability_in_survival_only() {
    let f = fixture();
    let exhaustion = f["attack_strength"]["food_exhaustion"].as_f64().unwrap() as f32;
    for mode in [GameMode::Survival, GameMode::Creative] {
        let (w, mut s, mut e) = scene(6.0);
        s.set_mode(mode);
        charge(&mut s, 20);
        let before = s.vitals.hunger.exhaustion;
        s.attack(0, &w, &mut e, false).unwrap();
        let survival = mode == GameMode::Survival;
        assert_eq!(s.vitals.hunger.exhaustion - before, if survival { exhaustion } else { 0.0 });
        let worn = s.inventory.get(0).unwrap().damage > 0;
        assert_eq!(worn, survival);
    }
}

#[test]
fn the_cli_adapter_uses_the_same_charged_transaction() {
    let (mut w, mut s, mut e) = scene(6.0);
    charge(&mut s, 20);
    s.attack(0, &w, &mut e, false).unwrap();
    let session_health = e.mobs[0].health;

    let (_, _, mut e2) = scene(6.0);
    let mut cli = Agent::new(DVec3::new(4.5, 145., 4.5));
    cli.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1));
    let uncharged = cli.execute(Command::Attack, &mut w, &mut e2, &[]);
    assert!(uncharged.is_ok());
    assert!(e2.mobs[0].health > session_health, "an uncharged CLI hit is weaker");

    let (_, _, mut e3) = scene(6.0);
    for _ in 0..20 {
        cli.tick(&mut w, &mut Entities::new(1));
    }
    cli.player.pos = DVec3::new(4.5, 145., 4.5);
    cli.player.vel = DVec3::ZERO;
    cli.player.on_ground = true;
    cli.player.yaw = 0.0;
    cli.player.pitch = 0.0;
    cli.execute(Command::Attack, &mut w, &mut e3, &[]).unwrap();
    assert_eq!(e3.mobs[0].health, session_health);
}

#[test]
fn the_crosshair_indicator_charges_then_marks_a_full_strike_on_a_living_target() {
    use voxelcraft::session::combat::AttackIndicator;
    let (_, mut s, _) = scene(6.0);
    charge(&mut s, 1);
    assert_eq!(s.attack_indicator(0, true), Some(AttackIndicator::Charging(0.0)));
    charge(&mut s, 6);
    let Some(AttackIndicator::Charging(scale)) = s.attack_indicator(0, false) else { panic!() };
    assert!(scale > 0.0 && scale < 1.0);
    charge(&mut s, 20);
    assert_eq!(s.attack_indicator(0, false), None, "a full charge shows nothing without a target");
    assert_eq!(s.attack_indicator(0, true), Some(AttackIndicator::Full));
    // Fists recharge in 5 ticks, which Java treats as too fast for the full mark.
    s.inventory.slots[0] = None;
    charge(&mut s, 20);
    assert_eq!(mining::attack_strength_delay(None), 5.0);
    assert_eq!(s.attack_indicator(0, true), None);
}
