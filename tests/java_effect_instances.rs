//! Java 26.3's `MobEffectInstance` (infinite duration, the ambient,
//! show_particles and show_icon flags, `update`, `tickServer` and
//! `compareTo`) and `/effect give`'s full syntax, against the cases
//! `tools/extract_java_effect_instance_cases.py` generates from a model of
//! the decompiled source.
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    agent::{Agent, Command, EffectChange, command::effect_feedback, tab_complete},
    entity::{Entities, MobKind},
    simulation::{
        effects::{self, AMBIENT_PARTICLE_ALPHA, Active, Effect, Effects, INFINITE},
        survival::Vitals,
    },
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/effect_instances.json")).unwrap()
}

fn effect(id: &Value) -> Effect {
    Effect::from_id(id.as_str().unwrap()).unwrap()
}

fn ticks(duration: &Value) -> u32 {
    match duration.as_i64().unwrap() {
        -1 => INFINITE,
        d => u32::try_from(d).unwrap(),
    }
}

/// `[duration, amplifier, ambient, show_particles, show_icon]`.
fn instance(e: Effect, spec: &Value) -> Active {
    let b = |i: usize| spec[i].as_bool().unwrap();
    Active::new(e, spec[1].as_u64().unwrap() as u8, ticks(&spec[0])).with_flags(b(2), b(3), b(4))
}

fn spec(a: &Active) -> Value {
    serde_json::json!([a.java_duration(), a.amplifier, a.ambient, a.particles, a.show_icon])
}

fn cases(key: &str) -> Vec<Value> {
    fixture()[key].as_array().unwrap().clone()
}

/// `LivingEntity.addEffect` and `MobEffectInstance.update`: which adds
/// change the effect, and the resulting chain of the active instance and its
/// hidden ones, flags included. Infinite beats every finite duration.
#[test]
fn adding_instances_follows_mob_effect_instance_update() {
    let fixture = fixture();
    assert_eq!(fixture["infinite_duration"], -1);
    let all = cases("merge_cases");
    assert!(all.len() >= 300);
    for (n, case) in all.iter().enumerate() {
        let e = effect(&case["effect"]);
        let mut effects = Effects::default();
        let changed: Vec<bool> =
            case["adds"].as_array().unwrap().iter().map(|a| effects.add_instance(instance(e, a)).1).collect();
        let expected: Vec<bool> = case["changed"].as_array().unwrap().iter().map(|c| c.as_bool().unwrap()).collect();
        assert_eq!(changed, expected, "case {n}: {case}");
        let chain: Vec<Value> = effects.chain(e).iter().map(spec).collect();
        assert_eq!(Value::Array(chain), case["chain"], "case {n}: {case}");
    }
}

/// `MobEffectInstance.tickServer`: finite instances (and their hidden ones)
/// count down and hand over; an infinite one never counts down and times
/// its periodic ticks on the entity's tick count instead of its duration.
#[test]
fn ticking_follows_tick_server() {
    for (n, case) in cases("tick_cases").iter().enumerate() {
        let e = effect(&case["effect"]);
        let mut effects = Effects::default();
        for _ in 0..case["age"].as_u64().unwrap() {
            effects.tick(0.05);
        }
        for a in case["adds"].as_array().unwrap() {
            effects.add_instance(instance(e, a));
        }
        let mut applied = Vec::new();
        for t in 1..=case["ticks"].as_u64().unwrap() {
            let out = effects.tick(0.05);
            if out.heal + out.poison + out.wither > 0.0 {
                applied.push(t);
            }
            if !effects.has(e) {
                break;
            }
        }
        assert_eq!(serde_json::json!(applied), case["applied"], "case {n}: {case}");
        let chain: Vec<Value> = effects.chain(e).iter().map(spec).collect();
        assert_eq!(Value::Array(chain), case["chain"], "case {n}: {case}");
    }
}

/// An infinite effect never runs out, never reads as "about to expire"
/// (night vision stays full, the HUD icon never blinks) and shows `∞`.
#[test]
fn infinite_effects_never_expire() {
    let mut effects = Effects::default();
    effects.add(Effect::NightVision, 0, INFINITE);
    effects.add(Effect::Speed, 1, 100);
    effects.add(Effect::Speed, 0, INFINITE);
    for _ in 0..20_000 {
        effects.tick(0.05);
    }
    let night = effects.get(Effect::NightVision).unwrap();
    assert!(night.is_infinite() && !night.ends_within(200));
    assert_eq!(effects.night_vision(0.0), 1.0);
    assert_eq!(night.hud_alpha(), 1.0);
    assert_eq!(effects::duration_text(night.ticks), "\u{221e}");
    // Speed II ran out and the infinite Speed I it hid came back.
    assert_eq!(effects.get(Effect::Speed).map(|a| (a.amplifier, a.is_infinite())), Some((0, true)));
}

/// `MobEffectInstance.compareTo`: the inventory list's natural order and
/// the HUD's reverse order.
#[test]
fn effects_sort_by_compare_to() {
    let fixture = fixture();
    assert_eq!(fixture["compare_cutoff"], 32147);
    for (n, case) in cases("order_cases").iter().enumerate() {
        let mut effects = Effects::default();
        for e in case["effects"].as_array().unwrap() {
            let a = Active::new(effect(&e[0]), 0, ticks(&e[1])).with_flags(e[2].as_bool().unwrap(), true, true);
            assert!(effects.add_instance(a).1);
        }
        let ids = |list: Vec<Active>| Value::from(list.iter().map(|a| a.effect.id()).collect::<Vec<_>>());
        assert_eq!(ids(effects.sorted()), case["natural"], "case {n}: {case}");
        let hud: Vec<Active> = effects::hud_layout(&effects).into_iter().map(|(a, _, _)| a).collect();
        assert_eq!(ids(hud), case["reverse"], "case {n}: {case}");
    }
}

/// `Hud.extractEffects`: a hidden icon takes no slot, and each row counts
/// its own columns from the right edge.
#[test]
fn hidden_icons_take_no_hud_slot() {
    let mut effects = Effects::default();
    // Hero of the Village is given with particles off and the icon on.
    effects.add_instance(Active::new(Effect::HeroOfTheVillage, 0, 48_000).with_flags(false, false, true));
    effects.add_instance(Active::new(Effect::Speed, 0, 600).with_flags(false, false, false));
    effects.add_instance(Active::new(Effect::Poison, 0, 600));
    let layout: Vec<(Effect, u32, u8)> =
        effects::hud_layout(&effects).into_iter().map(|(a, column, row)| (a.effect, column, row)).collect();
    assert_eq!(layout, [(Effect::HeroOfTheVillage, 1, 0), (Effect::Poison, 1, 1)]);
    // The inventory list still shows every effect.
    assert_eq!(effects.sorted().len(), 3);
}

/// `Hud.extractEffects`' icon alpha: steady when ambient or with over 200
/// ticks left, else Java's blink, in single precision with `Mth.cos`.
#[test]
fn hud_icons_blink_as_in_java() {
    assert_eq!(fixture()["hud_blink_ticks"], 200);
    for case in cases("hud_alpha_cases") {
        let a = Active::new(Effect::Speed, 0, ticks(&case["duration"])).with_flags(
            case["ambient"].as_bool().unwrap(),
            true,
            true,
        );
        let expected = case["alpha"].as_f64().unwrap() as f32;
        assert!((a.hud_alpha() - expected).abs() <= 1e-6, "{case}: {}", a.hud_alpha());
    }
}

/// `MobEffectUtil.formatDuration` and `EffectsInInventory.getEffectName`.
#[test]
fn durations_and_levels_read_as_in_java() {
    for case in cases("duration_text_cases") {
        assert_eq!(effects::duration_text(ticks(&case["duration"])), case["text"].as_str().unwrap(), "{case}");
    }
    for case in cases("inventory_level_cases") {
        let amplifier = case["amplifier"].as_u64().unwrap() as u8;
        let label = effects::list_label(&Active::new(Effect::Speed, amplifier, 600));
        assert_eq!(label != "Speed", case["level_text"].as_bool().unwrap(), "{case}: {label}");
    }
    assert_eq!(effects::list_label(&Active::new(Effect::Speed, 2, 600)), "Speed III");
}

/// `LivingEntity.tickEffects` and `updateSynchronizedMobEffectParticles`:
/// swirls come only from effects with particles, an ambient one's colour has
/// alpha 38, and when every visible effect is ambient they are five times
/// rarer (`areAllEffectsAmbient`).
#[test]
fn particle_flags_follow_java() {
    let fixture = fixture();
    assert_eq!(fixture["ambient_particle_alpha"].as_u64(), Some(u64::from(AMBIENT_PARTICLE_ALPHA)));
    assert_eq!(fixture["all_ambient_factor"], 5);
    assert_eq!(fixture["particle_bound"]["visible"], 4);
    let mut effects = Effects::default();
    effects.add_instance(Active::new(Effect::Speed, 0, 600).with_flags(false, false, true));
    assert_eq!(effects.with_particles().count(), 0);
    assert!(effects.all_ambient(), "no visible effect, so none is non-ambient");
    effects.add_instance(Active::new(Effect::Haste, 0, 600).with_flags(true, true, true));
    assert!(effects.all_ambient());
    assert_eq!(effects.get(Effect::Haste).unwrap().particle_colour()[3], AMBIENT_PARTICLE_ALPHA);
    effects.add(Effect::Poison, 0, 600);
    assert!(!effects.all_ambient());
    assert_eq!(effects.get(Effect::Poison).unwrap().particle_colour()[3], 255);
    assert_eq!(effects.with_particles().map(|a| a.effect).collect::<Vec<_>>(), [Effect::Poison, Effect::Haste]);
}

/// `/effect give <targets> <effect> [<seconds>|infinite] [<amplifier>]
/// [<hideParticles>]`, Brigadier's integer and boolean syntax included.
#[test]
fn effect_give_parses_every_java_form() {
    let fixture = fixture();
    for case in fixture["command"]["cases"].as_array().unwrap() {
        let e = effect(&case["effect"]);
        let args: Vec<&str> = case["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        let line = format!("/effect give @s minecraft:{} {}", e.id(), args.join(" "));
        let parsed = Command::parse(&line);
        match &case["result"] {
            Value::Null => assert!(parsed.is_err(), "{line} must fail"),
            r => {
                let want = EffectChange::Give(
                    e,
                    ticks(&r["ticks"]),
                    r["amplifier"].as_u64().unwrap() as u8,
                    r["particles"].as_bool().unwrap(),
                );
                assert!(matches!(parsed, Ok(Command::Effect(got)) if got == want), "{line}: {:?}", parsed.err());
            }
        }
    }
    assert_eq!(tab_complete("/effect give @s speed inf").as_deref(), Some("/effect give @s speed infinite"));
    assert_eq!(tab_complete("/effect give speed 10 1 t").as_deref(), Some("/effect give speed 10 1 true"));
}

/// `EffectCommands`' single-target feedback and its three failures, from
/// en_us `commands.effect.*`.
#[test]
fn effect_commands_report_java_feedback() {
    let fixture = fixture();
    let lang = |key: &str| fixture["feedback"][key].as_str().unwrap().to_string();
    assert_eq!(effect_feedback::GIVE_SUCCESS, lang("commands.effect.give.success.single"));
    assert_eq!(effect_feedback::GIVE_FAILED, lang("commands.effect.give.failed"));
    assert_eq!(effect_feedback::CLEAR_EVERYTHING_SUCCESS, lang("commands.effect.clear.everything.success.single"));
    assert_eq!(effect_feedback::CLEAR_EVERYTHING_FAILED, lang("commands.effect.clear.everything.failed"));
    assert_eq!(effect_feedback::CLEAR_SPECIFIC_SUCCESS, lang("commands.effect.clear.specific.success.single"));
    assert_eq!(effect_feedback::CLEAR_SPECIFIC_FAILED, lang("commands.effect.clear.specific.failed"));
    let mut vitals = Vitals::default();
    let run = |vitals: &mut Vitals, line: &str| match Command::parse(line).unwrap() {
        Command::Effect(change) => change.apply(vitals, "Steve").map(|(_, text)| text),
        _ => unreachable!(),
    };
    assert_eq!(run(&mut vitals, "effect clear @s").unwrap_err(), lang("commands.effect.clear.everything.failed"));
    assert_eq!(run(&mut vitals, "effect give @s speed infinite 1 true").unwrap(), "Applied effect Speed to Steve");
    let speed = *vitals.effects.get(Effect::Speed).unwrap();
    assert!(speed.is_infinite() && !speed.particles && !speed.show_icon && !speed.ambient);
    // Nothing stronger, longer or differently flagged: `update` changes nothing.
    assert_eq!(run(&mut vitals, "effect give @s speed 30 1 true").unwrap_err(), lang("commands.effect.give.failed"));
    // Particles back on is a change.
    assert!(run(&mut vitals, "effect give @s speed 30 1").is_ok());
    assert!(vitals.effects.get(Effect::Speed).unwrap().particles);
    assert_eq!(run(&mut vitals, "effect clear @s haste").unwrap_err(), lang("commands.effect.clear.specific.failed"));
    assert_eq!(run(&mut vitals, "effect clear @s speed").unwrap(), "Removed effect Speed from Steve");
    run(&mut vitals, "effect give @s haste").unwrap();
    assert_eq!(run(&mut vitals, "effect clear @s").unwrap(), "Removed every effect from Steve");
}

fn world() -> World {
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
    for _ in 0..10_000 {
        world.update(DVec3::new(1.0, 150.0, 1.0));
        if world.is_loaded(IVec3::new(1, 150, 1)) {
            return world;
        }
        std::thread::yield_now();
    }
    panic!("world failed to load");
}

/// The command path CLI agents, the controller and LAN players share gives
/// infinite and particle-free effects, and a failed `/effect` is an error.
#[test]
fn agents_give_infinite_hidden_effects_by_command() {
    let (mut world, mut entities) = (world(), Entities::new(1));
    let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    let give = Command::parse("/effect give @s minecraft:regeneration infinite 0 true").unwrap();
    agent.execute(give, &mut world, &mut entities, &[]).unwrap();
    let regeneration = *agent.vitals.effects.get(Effect::Regeneration).unwrap();
    assert!(regeneration.is_infinite() && !regeneration.particles && !regeneration.show_icon);
    let again = Command::parse("/effect give @s regeneration infinite 0 true").unwrap();
    assert!(agent.execute(again, &mut world, &mut entities, &[]).is_err(), "nothing changes: Java fails");
    agent.execute(Command::parse("effect clear").unwrap(), &mut world, &mut entities, &[]).unwrap();
    assert!(agent.execute(Command::parse("effect clear").unwrap(), &mut world, &mut entities, &[]).is_err());
}

/// Saves keep infinite durations and every flag, on the active instance and
/// on hidden ones, for players (level props) and mobs; older save strings
/// still load with Java's defaults.
#[test]
fn flags_and_infinite_durations_survive_a_save() {
    let mut effects = Effects::default();
    effects.add_instance(Active::new(Effect::Haste, 1, INFINITE).with_flags(true, true, true));
    effects.add_instance(Active::new(Effect::HeroOfTheVillage, 0, 48_000).with_flags(false, false, true));
    effects.add_instance(Active::new(Effect::Speed, 0, 6000).with_flags(false, false, false));
    effects.add_instance(Active::new(Effect::Speed, 2, 200).with_flags(false, true, true));
    effects.add_instance(Active::new(Effect::Absorption, 1, INFINITE));
    effects.absorb(3.0);
    let text = effects.serialize();
    assert!(text.contains("haste:1:-1:a"), "{text}");
    assert!(text.contains("hero_of_the_village:0:48000:p0"), "{text}");
    assert!(text.contains("speed:0:6000:p0:i0"), "{text}");
    assert!(text.contains("absorption:1:-1:5"), "{text}");
    assert_eq!(Effects::deserialize(&text), effects);
    assert_eq!(Effects::deserialize(&text).chain(Effect::Speed), effects.chain(Effect::Speed));

    let dir = std::env::temp_dir().join(format!(
        "voxelcraft-effect-instances-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let storage = Storage::new(&dir);
    let mut mobs = Entities::new(7);
    mobs.spawn(MobKind::Donkey, DVec3::new(0.5, 70.0, 0.5));
    mobs.mobs[0].effects = effects.clone();
    let props = [("effects", text.clone()), ("mobs", mobs.nether_mobs_to_string())]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    storage.save(&LevelInfo { seed: 7, player: None, props }, &[]).unwrap();
    let level = Storage::new(&dir).load_level().unwrap();
    assert_eq!(Effects::deserialize(&level.props["effects"]), effects);
    let mut back = Entities::new(7);
    back.load_nether_mobs(&level.props["mobs"]);
    assert_eq!(back.mobs[0].effects, effects);
    std::fs::remove_dir_all(dir).unwrap();

    // A pre-v0.14 string: no flags, finite durations, Java's defaults.
    let old = Effects::deserialize("speed:1:100|speed:0:600|absorption:1:2400:5.5");
    assert_eq!(old.chain(Effect::Speed), [Active::new(Effect::Speed, 1, 100), Active::new(Effect::Speed, 0, 600)]);
    assert_eq!(old.absorption(), 5.5);
    assert!(old.iter().all(|a| !a.ambient && a.particles && a.show_icon));
}

/// The `--effect` start option for scripted scenes takes `infinite` and the
/// three flags.
#[test]
fn the_start_option_takes_infinite_and_flags() {
    let a = Active::parse_option("conduit_power,infinite,0,ambient").unwrap();
    assert_eq!(a, Active::new(Effect::ConduitPower, 0, INFINITE).with_flags(true, true, true));
    let a = Active::parse_option("hero_of_the_village,60,1,hideparticles").unwrap();
    assert_eq!(a, Active::new(Effect::HeroOfTheVillage, 1, 1200).with_flags(false, false, true));
    assert_eq!(Active::parse_option("speed").unwrap(), Active::new(Effect::Speed, 0, 600));
    assert!(Active::parse_option("speed,10,0,sparkly").is_err());
}
