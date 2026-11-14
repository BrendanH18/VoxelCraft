//! Java 26.3's mob effect registry (`MobEffects`, `MobEffectCategory`,
//! en_us `effect.minecraft.*`) against [`Effect`], from the fixture
//! `tools/extract_java_effect_fixtures.py` extracts from the official jars.
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    agent::{Agent, Command, EffectChange, tab_complete},
    entity::{Entities, PlayerId, Target},
    simulation::effects::{self, Category, Effect, Effects, Operation},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/mob_effects.json")).unwrap()
}

fn java_effects() -> Vec<Value> {
    fixture()["effects"].as_array().unwrap().clone()
}

/// The effects v0.14 registered, looked up by Java id rather than position.
const REGISTERED_IN_V0_17: [&str; 11] = [
    "health_boost",
    "glowing",
    "levitation",
    "conduit_power",
    "hero_of_the_village",
    "raid_omen",
    "wind_charged",
    "weaving",
    "oozing",
    "infested",
    "breath_of_the_nautilus",
];

fn category(name: &str) -> Category {
    match name {
        "beneficial" => Category::Beneficial,
        "harmful" => Category::Harmful,
        "neutral" => Category::Neutral,
        other => panic!("unknown category {other}"),
    }
}

/// Every effect Java registers is an [`Effect`] with Java's id, English
/// name, colour, category, instantaneous flag and attribute modifiers, and
/// the game has no effect Java lacks.
#[test]
fn every_java_effect_is_registered_with_its_reference_facts() {
    let java = java_effects();
    assert_eq!(java.len(), 40, "Java 26.3 registers 40 mob effects");
    for e in &java {
        let id = e["id"].as_str().unwrap();
        let effect = Effect::from_id(id).unwrap_or_else(|| panic!("Java effect {id} has no Effect variant"));
        assert_eq!(Effect::from_id(&format!("minecraft:{id}")), Some(effect));
        assert_eq!(effect.id(), id);
        assert_eq!(effect.name(), e["name"].as_str().unwrap(), "{id} name");
        let [r, g, b] = effect.colour();
        assert_eq!(u32::from_be_bytes([0, r, g, b]), e["color"].as_u64().unwrap() as u32, "{id} colour");
        assert_eq!(effect.category(), category(e["category"].as_str().unwrap()), "{id} category");
        assert_eq!(effect.is_instantaneous(), e["instantaneous"].as_bool().unwrap(), "{id} isInstantaneous");
        let modifiers: Vec<_> = e["attribute_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                let operation = match m["operation"].as_str().unwrap() {
                    "add_value" => Operation::AddValue,
                    "add_multiplied_base" => Operation::AddMultipliedBase,
                    "add_multiplied_total" => Operation::AddMultipliedTotal,
                    other => panic!("unknown operation {other}"),
                };
                (m["attribute"].as_str().unwrap(), m["id"].as_str().unwrap(), m["amount"].as_f64().unwrap(), operation)
            })
            .collect();
        let ours: Vec<_> = effect.modifiers().iter().map(|m| (m.attribute, m.id, m.amount, m.operation)).collect();
        assert_eq!(ours, modifiers, "{id} attribute modifiers");
    }
    for effect in Effect::ALL {
        assert!(java.iter().any(|e| e["id"] == effect.id()), "{} is not a Java 26.3 effect", effect.id());
    }
    assert_eq!(Effect::ALL.len(), java.len());
    for id in REGISTERED_IN_V0_17 {
        assert!(Effect::from_id(id).is_some(), "{id}");
    }
}

/// `MobEffectCategory`: neutral effects (the omens and Glowing) colour
/// tooltips blue like beneficial ones, and only harmful ones are red.
#[test]
fn neutral_effects_have_their_own_category_and_blue_tooltips() {
    let formatting = &fixture()["category_tooltip_formatting"];
    for effect in Effect::ALL {
        let key = match effect.category() {
            Category::Beneficial => "beneficial",
            Category::Harmful => "harmful",
            Category::Neutral => "neutral",
        };
        assert_eq!(effect.is_harmful(), formatting[key] == "red", "{} tooltip colour", effect.id());
    }
    let neutral: Vec<_> =
        Effect::ALL.into_iter().filter(|e| e.category() == Category::Neutral).map(Effect::id).collect();
    assert_eq!(neutral, ["bad_omen", "trial_omen", "glowing", "raid_omen"]);
}

/// `Hud.extractEffects` places an icon by `MobEffect.isBeneficial`: the
/// neutral omens and Glowing share the second row with harmful effects.
#[test]
fn neutral_effect_icons_sit_in_the_hud_row_below_beneficial_ones() {
    for e in java_effects() {
        let effect = Effect::from_id(e["id"].as_str().unwrap()).unwrap();
        assert_eq!(effects::hud_row(effect), u8::from(e["category"] != "beneficial"), "{}", effect.id());
    }
    assert_eq!(effects::hud_row(Effect::BadOmen), 1);
    assert_eq!(effects::hud_row(Effect::Speed), 0);
}

/// `MobEffects.WITHER` is 7561558 (0x736156), the tint of its swirls,
/// tipped arrows and lingering clouds.
#[test]
fn wither_uses_its_java_colour() {
    assert_eq!(Effect::Wither.colour(), [0x73, 0x61, 0x56]);
    let wither = java_effects().into_iter().find(|e| e["id"] == "wither").unwrap();
    assert_eq!(wither["color"], 7561558);
}

/// en_us `effect.minecraft.dolphins_grace` is "Dolphin's Grace", with a plain apostrophe.
#[test]
fn dolphins_grace_is_named_as_in_en_us() {
    assert_eq!(Effect::DolphinsGrace.name(), "Dolphin's Grace");
}

/// The behaviour the game implements reads the same numbers as Java's
/// modifiers: per level, `amount * (amplifier + 1)`.
#[test]
fn implemented_effect_modifiers_use_the_registered_amounts() {
    let amount = |e: Effect| e.modifiers()[0].amount;
    let with = |e: Effect, amplifier: u8| {
        let mut effects = Effects::default();
        effects.add(e, amplifier, 600);
        effects
    };
    assert_eq!(amount(Effect::Speed), voxelcraft::knockback::SPEED_PER_LEVEL);
    assert_eq!(amount(Effect::Slowness), voxelcraft::knockback::SLOWNESS_PER_LEVEL);
    assert_eq!(with(Effect::Speed, 1).speed_factor(), 1.0 + 2.0 * amount(Effect::Speed));
    assert_eq!(with(Effect::Haste, 2).attack_speed_factor(), 1.0 + 3.0 * amount(Effect::Haste));
    assert_eq!(with(Effect::MiningFatigue, 0).attack_speed_factor(), 1.0 + amount(Effect::MiningFatigue));
    assert_eq!(f64::from(with(Effect::Strength, 1).attack_bonus()), 2.0 * amount(Effect::Strength));
    assert_eq!(f64::from(with(Effect::Weakness, 0).attack_bonus()), amount(Effect::Weakness));
    assert_eq!(f64::from(with(Effect::Absorption, 3).max_absorption()), 4.0 * amount(Effect::Absorption));
    assert_eq!(f64::from(with(Effect::Luck, 2).luck()), 3.0 * amount(Effect::Luck));
    assert_eq!(f64::from(with(Effect::BadLuck, 0).luck()), amount(Effect::BadLuck));
}

/// Effects save as `id:amplifier:ticks` Java ids: a pre-v0.14 save still loads,
/// and every effect registered in v0.14 survives a save and reload, for
/// players (level props) and mobs alike.
#[test]
fn registered_effects_survive_a_save_and_old_saves_still_load() {
    let old = Effects::deserialize("speed:0:600|bad_omen:0:120000|invisibility:0:3600|absorption:1:2400:5.5");
    assert_eq!(old.iter().count(), 4);
    assert_eq!(old.get(Effect::BadOmen).map(|a| a.ticks), Some(120000));
    assert_eq!(old.absorption(), 5.5);
    let mut all = Effects::default();
    for (i, id) in REGISTERED_IN_V0_17.iter().enumerate() {
        all.add(Effect::from_id(id).unwrap(), (i % 3) as u8, 100 + i as u32);
    }
    let text = all.serialize();
    for id in REGISTERED_IN_V0_17 {
        assert!(text.split('|').any(|entry| entry.starts_with(&format!("{id}:"))), "{id} is saved by its Java id");
    }
    let dir = std::env::temp_dir().join(format!(
        "voxelcraft-java-effects-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let storage = Storage::new(&dir);
    let mut mobs = Entities::new(7);
    mobs.spawn(voxelcraft::entity::MobKind::Donkey, DVec3::new(0.5, 70.0, 0.5));
    mobs.mobs[0].effects = all.clone();
    let props = [("effects", text.clone()), ("mobs", mobs.nether_mobs_to_string())]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    storage.save(&LevelInfo { seed: 7, player: None, props }, &[]).unwrap();
    let level = Storage::new(&dir).load_level().unwrap();
    assert_eq!(Effects::deserialize(&level.props["effects"]), all);
    let mut back = Entities::new(7);
    back.load_nether_mobs(&level.props["mobs"]);
    assert_eq!(back.mobs[0].effects, all);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The in-memory effect bit set has room for every effect, so mobs see a
/// player's late-registered effects.
#[test]
fn the_target_effect_mask_holds_every_effect() {
    let mut all = Effects::default();
    for effect in Effect::ALL.into_iter().filter(|e| !e.is_instant()) {
        all.add(effect, 0, 600);
    }
    let mut target = Target::new(PlayerId(1), DVec3::ZERO, true);
    target.effects = all.mask();
    for effect in Effect::ALL.into_iter().filter(|e| !e.is_instant()) {
        assert!(target.has_effect(effect), "{}", effect.id());
    }
    assert_eq!(target.effects.count_ones() as usize, Effect::ALL.len() - 2);
}

/// `EffectCommands.computeDurationInTicks`: seconds for lasting effects (30
/// by default), ticks for instantaneous ones (one by default).
#[test]
fn effect_command_durations_follow_java() {
    for e in java_effects() {
        let effect = Effect::from_id(e["id"].as_str().unwrap()).unwrap();
        let (default, five) = if e["instantaneous"] == true { (1, 5) } else { (600, 100) };
        assert_eq!(effect.command_duration_ticks(None), default, "{}", effect.id());
        assert_eq!(effect.command_duration_ticks(Some(5)), five, "{}", effect.id());
    }
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

/// `/effect give` and `/effect clear` reach every registered effect through
/// the command path the console, CLI agents, controller and LAN players
/// share, and the console completes the new ids.
#[test]
fn every_effect_can_be_given_and_cleared_by_command() {
    let (mut world, mut entities) = (world(), Entities::new(1));
    let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    for effect in Effect::ALL.into_iter().filter(|e| !e.is_instant()) {
        let line = format!("/effect give @s minecraft:{} 10 1", effect.id());
        let command = Command::parse(&line).unwrap();
        let ticks = if effect.is_instantaneous() { 10 } else { 200 };
        assert!(
            matches!(command, Command::Effect(EffectChange::Give(e, t, 1, true)) if e == effect && t == ticks),
            "{line}"
        );
        agent.execute(command, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(agent.vitals.effects.get(effect).map(|a| (a.amplifier, a.ticks)), Some((1, ticks)), "{line}");
    }
    for id in REGISTERED_IN_V0_17 {
        let effect = Effect::from_id(id).unwrap();
        agent.execute(Command::parse(&format!("effect clear {id}")).unwrap(), &mut world, &mut entities, &[]).unwrap();
        assert!(!agent.vitals.effects.has(effect), "{id} cleared");
    }
    assert_eq!(tab_complete("/effect give @s breath").as_deref(), Some("/effect give @s breath_of_the_nautilus"));
    assert_eq!(tab_complete("/effect give hero").as_deref(), Some("/effect give hero_of_the_village"));
    assert_eq!(tab_complete("/effect clear wi"), None, "wind_charged and wither share only `wi`");
    assert_eq!(tab_complete("/effect clear win").as_deref(), Some("/effect clear wind_charged"));
    assert_eq!(tab_complete("/effect give minecraft:oo").as_deref(), Some("/effect give oozing"));
}
