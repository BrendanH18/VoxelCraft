//! Game rules and difficulty through the shared agent, which controller and
//! LAN players also run on. Rule semantics are Java 26.3's; see
//! `docs/game-rules.md` and `docs/verification/2026-10-17-game-rules-difficulty.md`.
use std::sync::Arc;

use glam::{DVec3, IVec3};
use voxelcraft::{
    agent::{Agent, Command},
    entity::Entities,
    rules::{GameMode, GameRules},
    simulation::{difficulty::Difficulty, survival::MAX_HEALTH},
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
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
