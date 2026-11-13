//! Game rules and difficulty through the shared agent, which controller and
//! LAN players also run on. Rule semantics are Java 26.3's; see
//! `docs/game-rules.md` and `docs/verification/2026-10-17-game-rules-difficulty.md`.
use std::sync::Arc;

use glam::{DVec3, IVec3};
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Ctx, Entities, MobKind, PlayerId, Target, neutral},
    rules::{GameMode, GameRules},
    simulation::{difficulty::Difficulty, lighting::SpawnLight, survival::MAX_HEALTH},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        terrain::{Dimension, Generator},
    },
};

fn world() -> World {
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
    for _ in 0..10_000 {
        world.update(DVec3::new(1.0, 150.0, 1.0));
        if world.is_loaded(IVec3::new(1, 150, 1)) {
            // A floor to stand on, so ticks do not turn into a fall.
            for x in -3..=5 {
                for z in -3..=5 {
                    world.set_block(IVec3::new(x, 149, z), Block::STONE);
                }
            }
            return world;
        }
        std::thread::yield_now();
    }
    panic!("world failed to load");
}

fn survivor(world: &mut World, entities: &mut Entities, rules: &GameRules) -> Agent {
    let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    agent.set_mode(GameMode::Survival);
    // The first tick hands the agent the world's rules, as the host does every tick.
    agent.tick_rules(world, entities, Difficulty::Normal, rules);
    agent.vitals.health = MAX_HEALTH;
    agent
}

#[test]
fn fall_fire_and_drowning_rules_make_the_player_immune_to_that_damage_type() {
    let mut world = world();
    let mut entities = Entities::new(1);
    for (rule, cause) in [
        ("fall_damage", "fell from a high place"),
        ("fall_damage", "hit the ground too hard"),
        ("fire_damage", "burned to death"),
        ("fire_damage", "tried to swim in lava"),
        ("fire_damage", "was fireballed by a blaze"),
        ("drowning_damage", "drowned"),
    ] {
        let hurt = |rules: &GameRules, entities: &mut Entities, world: &mut World| {
            let mut agent = survivor(world, entities, rules);
            agent.hurt(4.0, cause, DVec3::ZERO, entities);
            MAX_HEALTH - agent.vitals.health
        };
        assert!(hurt(&GameRules::default(), &mut entities, &mut world) > 0.0, "{rule}: {cause} hurts by default");
        let mut off = GameRules::default();
        off.set(rule, "false").unwrap();
        assert_eq!(hurt(&off, &mut entities, &mut world), 0.0, "{rule} = false makes {cause} harmless");
        // Mobs and other damage types are unaffected by the three rules.
        let mut all_off = GameRules::default();
        for r in ["fall_damage", "fire_damage", "drowning_damage"] {
            all_off.set(r, "false").unwrap();
        }
        let mut agent = survivor(&mut world, &mut entities, &all_off);
        assert!(agent.hurt(4.0, "was slain by a zombie", DVec3::ZERO, &mut entities) > 0.0);
    }
}

#[test]
fn mounted_falls_honour_fall_damage_too() {
    let mut world = world();
    let mut entities = Entities::new(1);
    for (works, expected) in [("true", true), ("false", false)] {
        let mut rules = GameRules::default();
        rules.set("fall_damage", works).unwrap();
        let mut agent = survivor(&mut world, &mut entities, &rules);
        agent.hurt_fall(5.0, &mut entities);
        assert_eq!(agent.vitals.health < MAX_HEALTH, expected, "fall_damage = {works}");
    }
}

#[test]
fn natural_regeneration_and_difficulty_decide_how_an_agent_heals() {
    let mut world = world();
    let mut entities = Entities::new(1);
    let heal = |difficulty: Difficulty, regeneration: &str, world: &mut World, entities: &mut Entities| {
        let mut rules = GameRules::default();
        rules.set("natural_health_regeneration", regeneration).unwrap();
        let mut agent = survivor(world, entities, &rules);
        agent.vitals.health = 10.0;
        agent.vitals.hunger = voxelcraft::simulation::survival::Hunger::restore(10.0, 0.0, 0.0);
        for _ in 0..40 {
            agent.tick_rules(world, entities, difficulty, &rules);
        }
        (agent.vitals.health, agent.vitals.hunger.food)
    };
    // Peaceful: +1 health a second and +1 food every half second.
    assert_eq!(heal(Difficulty::Peaceful, "true", &mut world, &mut entities), (12.0, 14.0));
    // The rule gates both, and food-based healing needs 18 food anyway.
    assert_eq!(heal(Difficulty::Peaceful, "false", &mut world, &mut entities), (10.0, 10.0));
    assert_eq!(heal(Difficulty::Normal, "true", &mut world, &mut entities), (10.0, 10.0));
}

/// `ThrownEnderpearl.tick` discards a dead owner's pearls. An immediate
/// respawn must not outrun that, so the pearl goes with the death itself.
#[test]
fn a_dead_players_pearls_vanish_even_with_immediate_respawn() {
    let mut world = world();
    for vanish in [true, false] {
        let mut entities = Entities::new(1);
        let mut rules = GameRules::default();
        rules.set("ender_pearls_vanish_on_death", if vanish { "true" } else { "false" }).unwrap();
        rules.set("immediate_respawn", "true").unwrap();
        let mut agent = survivor(&mut world, &mut entities, &rules);
        entities.throw_pearl(agent.session.id, DVec3::new(1.5, 151.6, 1.5), DVec3::X, DVec3::ZERO);
        agent.hurt(1000.0, "was killed", DVec3::ZERO, &mut entities);
        assert!(agent.vitals.is_dead());
        // The host respawns an agent at once under `immediate_respawn`.
        agent.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
        assert!(!agent.vitals.is_dead());
        assert_eq!(entities.pearls.len(), usize::from(!vanish), "ender_pearls_vanish_on_death = {vanish}");
    }
}

/// A zombified piglin the agent hit, given one update to take the hit in.
fn angry_piglin(world: &World, entities: &mut Entities, agent: &Agent) {
    entities.spawn(MobKind::ZombifiedPiglin, DVec3::new(3.5, 150.0, 1.5));
    entities.attack(0, DVec3::X, 0.5);
    let ctx = Ctx {
        players: vec![Target::new(agent.session.id, agent.session.player.pos, true)],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Nether,
        spawn_light: SpawnLight::NIGHT,
    };
    entities.update(0.05, world, &ctx);
}

#[test]
fn forgive_dead_players_reaches_neutral_mobs_through_the_shared_death_step() {
    let mut world = world();
    for forgive in [true, false] {
        let mut entities = Entities::new(1);
        let mut rules = GameRules::default();
        rules.set("forgive_dead_players", if forgive { "true" } else { "false" }).unwrap();
        let mut agent = survivor(&mut world, &mut entities, &rules);
        // `Entities::attack` hits as the host, which this agent is.
        assert_eq!(agent.session.id, PlayerId::HOST);
        angry_piglin(&world, &mut entities, &agent);
        assert_eq!(entities.mobs[0].anger_target(), Some(agent.session.id));
        agent.hurt(1000.0, "was killed", DVec3::ZERO, &mut entities);
        assert!(agent.vitals.is_dead());
        let want = (!forgive).then_some(agent.session.id);
        assert_eq!(entities.mobs[0].anger_target(), want, "forgive_dead_players = {forgive}");
    }
}

#[test]
fn universal_anger_reaches_neutral_mobs_through_the_agents_rules() {
    let mut world = world();
    for universal in [true, false] {
        let mut entities = Entities::new(1);
        let mut rules = GameRules::default();
        rules.set("universal_anger", if universal { "true" } else { "false" }).unwrap();
        let agent = survivor(&mut world, &mut entities, &rules);
        angry_piglin(&world, &mut entities, &agent);
        let m = &entities.mobs[0];
        assert_eq!(m.angry_at_all_players(), universal);
        assert_eq!(m.anger_target(), (!universal).then_some(agent.session.id));
    }
}

/// `tests/fixtures/java/26.3/neutral_mobs.json`, from
/// `tools/extract_java_neutral_mob_fixtures.py`, against the game's tables.
#[test]
fn neutral_mob_rules_match_the_java_fixture() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/java/26.3/neutral_mobs.json")).expect("fixture parses");
    let java = fixture["neutral_mobs"].as_object().unwrap();
    // Both directions: every Java implementer is neutral here, and nothing else is.
    let mut names: Vec<&str> = neutral::KINDS.iter().map(|k| k.name()).collect();
    names.sort_unstable();
    let mut want: Vec<&str> = java.keys().map(String::as_str).collect();
    want.sort_unstable();
    assert_eq!(names, want);
    for kind in MobKind::ALL {
        assert_eq!(neutral::is_neutral(kind), java.contains_key(kind.name()), "{kind:?}");
    }
    // Kinds whose anger timer is still the game's own (see the EFX-RULE-21 record).
    let own_timer = [MobKind::IronGolem, MobKind::Wolf, MobKind::Enderman, MobKind::ZombifiedPiglin];
    for kind in neutral::KINDS {
        let j = &java[kind.name()];
        assert_eq!(neutral::alerts_same_type(kind), j["alert_others_of_same_type"] == true, "{kind:?}");
        assert_eq!(neutral::follow_range(kind), j["follow_range"].as_f64(), "{kind:?}");
        let secs = j["persistent_anger_seconds"].as_array().unwrap();
        let ticks = (secs[0].as_u64().unwrap() as u32 * 20, secs[1].as_u64().unwrap() as u32 * 20);
        match neutral::persistent_anger_ticks(kind) {
            Some(t) => assert_eq!(t, ticks, "{kind:?}"),
            None => assert!(own_timer.contains(&kind), "{kind:?} lacks Java's anger timer"),
        }
    }
    let inflate: Vec<f64> =
        fixture["death_notice"]["inflate_xyz"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(neutral::DEATH_NOTICE.to_vec(), inflate);
    assert_eq!(fixture["death_notice"]["only_with_forgive_dead_players"], true);
    assert_eq!(u64::from(neutral::PIGLIN_UNIVERSAL_ANGER_TICKS), fixture["piglin"]["universal_anger_ticks"]);
    assert_eq!(u64::from(neutral::PIGLIN_UNIVERSAL_ANGER_TICKS), fixture["piglin"]["angry_at_ticks"]);
}
