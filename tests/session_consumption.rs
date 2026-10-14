//! Independent Java timing expectations and shared actor/input-path regressions.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::Item,
    player::MoveInput,
    potion::Potion,
    rules::GameMode,
    session::PlayerSession,
    simulation::effects::Effect,
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};
fn world() -> World {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(IVec3::new(4, 144, 4), Block::STONE);
    world
}
fn session(item: Item) -> PlayerSession {
    let mut s = PlayerSession::new(PlayerId(3), DVec3::new(4.5, 145., 4.5));
    s.inventory.slots[0] = Some(Stack::new(item, 1));
    s.vitals.hunger.food = 8.;
    s
}
#[test]
fn reference_durations_complete_at_the_same_tick_through_cli_and_shared_execution() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/java/26.3/consumption.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let item = if case["item"] == "potion" {
            Item::potion(Potion::from_id("turtle_master").unwrap())
        } else {
            Item::from_name(case["item"].as_str().unwrap()).unwrap()
        };
        let ticks = case["ticks"].as_u64().unwrap() as u32;
        let mut direct = session(item);
        let mut cli = Agent::new(direct.player.pos);
        cli.session = session(item);
        let mut world = world();
        let mut direct_entities = Entities::new(7);
        let mut cli_entities = Entities::new(7);
        cli.execute(Command::Eat, &mut world, &mut cli_entities, &[]).unwrap();
        for tick in 1..=ticks {
            let step = direct.tick_consumption(0, true, &mut direct_entities).unwrap();
            cli.tick(&mut world, &mut cli_entities);
            assert_eq!(step.completed.is_some(), tick == ticks, "{} tick {tick}", item.name());
            assert_eq!(direct.inventory, cli.inventory);
            if tick < ticks {
                assert_eq!(direct.inventory.get(0), Some(Stack::new(item, 1)));
            } else {
                assert_eq!(step.completed.unwrap().player, PlayerId(3));
                assert_eq!(direct.vitals.hunger.food, cli.vitals.hunger.food);
                assert_eq!(direct.vitals.effects, cli.vitals.effects);
            }
        }
        assert_eq!(cli.remaining, 0);
    }
}
#[test]
fn interrupted_or_replaced_food_requires_a_whole_new_bite() {
    let mut s = session(Item::APPLE);
    let mut e = Entities::new(7);
    for _ in 0..31 {
        assert!(s.tick_consumption(0, true, &mut e).unwrap().completed.is_none());
    }
    s.inventory.slots[0] = Some(Stack::new(Item::BREAD, 1));
    assert!(s.tick_consumption(0, true, &mut e).unwrap().completed.is_none());
    assert_eq!(s.consumption_elapsed(), 0.05);
    s.tick_consumption(0, false, &mut e).unwrap();
    for _ in 0..31 {
        assert!(s.tick_consumption(0, true, &mut e).unwrap().completed.is_none());
    }
    assert!(s.tick_consumption(0, true, &mut e).unwrap().completed.is_some());
    assert_eq!(s.vitals.hunger.food, 13.);
    assert!(s.inventory.get(0).is_none());
}
#[test]
fn cli_wait_cancels_a_bite_but_idle_controller_rules_do_not_cancel_or_double_tick_it() {
    let mut cli = Agent::new(DVec3::new(4.5, 145., 4.5));
    cli.session = session(Item::APPLE);
    let mut w = world();
    let mut e = Entities::new(7);
    cli.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
    for _ in 0..20 {
        cli.tick(&mut w, &mut e);
    }
    cli.execute(Command::Run(MoveInput::default(), 1, false), &mut w, &mut e, &[]).unwrap();
    assert_eq!(cli.session.consumption_progress(), 0.);
    for tick in 1..=32 {
        // The controller's hands advance use; its agent then advances ordinary rules.
        cli.hold(MoveInput::default(), false, false);
        let step = cli.session.tick_consumption(0, true, &mut e).unwrap();
        cli.tick(&mut w, &mut e);
        assert_eq!(step.completed.is_some(), tick == 32);
        if tick < 32 {
            assert_eq!(cli.session.consumption_elapsed(), f64::from(tick) * 0.05);
            assert_eq!(cli.inventory.get(0), Some(Stack::new(Item::APPLE, 1)));
        }
    }
    assert!(cli.inventory.get(0).is_none());
}

#[test]
fn releasing_held_cli_use_requires_a_full_bite_when_pressed_again() {
    let mut cli = Agent::new(DVec3::new(4.5, 145., 4.5));
    cli.session = session(Item::APPLE);
    let mut w = world();
    let mut e = Entities::new(7);
    for _ in 0..31 {
        cli.hold(MoveInput::default(), false, true);
        cli.tick(&mut w, &mut e);
    }
    cli.hold(MoveInput::default(), false, false);
    cli.tick(&mut w, &mut e);
    for _ in 0..31 {
        cli.hold(MoveInput::default(), false, true);
        cli.tick(&mut w, &mut e);
        assert_eq!(cli.inventory.get(0), Some(Stack::new(Item::APPLE, 1)));
    }
    cli.hold(MoveInput::default(), false, true);
    cli.tick(&mut w, &mut e);
    assert!(cli.inventory.get(0).is_none());
}
#[test]
fn invalid_use_does_not_mutate_inventory_effects_clock_or_world_rng() {
    for invalid in 0..8 {
        let mut s = session(Item::APPLE);
        let mut e = Entities::new(7);
        let mut control = Entities::new(7);
        s.tick_consumption(0, true, &mut e).unwrap();
        let slot = match invalid {
            0 => 9,
            1 => {
                s.inventory.slots[0] = None;
                0
            }
            2 => {
                s.inventory.slots[0] = Some(Stack::new(Item::APPLE, 0));
                0
            }
            3 => {
                s.inventory.slots[0] = Some(Stack::new(Item::APPLE, 65));
                0
            }
            4 => {
                s.inventory.slots[0] = Some(Stack::new(Item::STICK, 1));
                0
            }
            5 => {
                s.vitals.hunger.food = 20.;
                0
            }
            6 => {
                s.vitals.damage(100., "test", false);
                0
            }
            _ => {
                s.sleeping = Some(0.);
                0
            }
        };
        let inventory = s.inventory.clone();
        let effects = s.vitals.effects.clone();
        let food = s.vitals.hunger.food;
        let elapsed = s.consumption_elapsed();
        assert!(s.tick_consumption(slot, true, &mut e).is_err(), "invalid case {invalid}");
        assert_eq!(s.inventory, inventory);
        assert_eq!(s.vitals.effects, effects);
        assert_eq!(s.vitals.hunger.food, food);
        assert_eq!(s.consumption_elapsed(), elapsed);
        assert_eq!(e.roll(), control.roll());
        assert!(e.items.is_empty());
    }
}
#[test]
fn distinct_players_keep_their_progress_effects_remainders_and_modes() {
    let potion = Item::potion(Potion::from_id("strong_turtle_master").unwrap());
    let mut s = session(potion);
    let mut other = session(Item::MILK_BUCKET);
    other.id = PlayerId(4);
    other.set_mode(GameMode::Creative);
    other.vitals.apply_effect(Effect::Poison, 1, 600);
    let mut e = Entities::new(7);
    for _ in 0..16 {
        s.tick_consumption(0, true, &mut e).unwrap();
        other.tick_consumption(0, true, &mut e).unwrap();
    }
    other.set_mode(GameMode::Survival);
    for _ in 0..16 {
        s.tick_consumption(0, true, &mut e).unwrap();
        assert!(other.tick_consumption(0, true, &mut e).unwrap().completed.is_none());
    }
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::GLASS_BOTTLE, 1)));
    assert_eq!(s.vitals.effects.get(Effect::Slowness).unwrap().amplifier, 5);
    assert_eq!(s.vitals.effects.get(Effect::Resistance).unwrap().amplifier, 3);
    assert!(other.vitals.effects.has(Effect::Poison));
    for _ in 0..16 {
        other.tick_consumption(0, true, &mut e).unwrap();
    }
    assert!(other.vitals.effects.is_empty());
    assert_eq!(other.inventory.get(0), Some(Stack::new(Item::BUCKET, 1)));
    assert!(s.vitals.effects.has(Effect::Resistance));
    s.inventory.slots[0] = Some(Stack::new(potion, 1));
    s.set_mode(GameMode::Spectator);
    assert!(s.tick_consumption(0, true, &mut e).is_err());
    s.set_mode(GameMode::Creative);
    for _ in 0..32 {
        s.tick_consumption(0, true, &mut e).unwrap();
    }
    assert_eq!(s.inventory.get(0), Some(Stack::new(potion, 1)));
}

#[test]
fn harming_finishes_the_inventory_exchange_before_the_existing_cli_death_route() {
    let item = Item::potion(Potion::from_id("harming").unwrap());
    let mut direct = session(item);
    direct.vitals.health = 1.;
    let mut cli = Agent::new(direct.player.pos);
    cli.session = session(item);
    cli.vitals.health = 1.;
    let mut w = world();
    let mut e = Entities::new(7);
    cli.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
    for _ in 0..31 {
        direct.tick_consumption(0, true, &mut e).unwrap();
        cli.tick(&mut w, &mut e);
    }
    let finished = direct.tick_consumption(0, true, &mut e).unwrap().completed.unwrap();
    assert_eq!(finished.magic_damage, 6.);
    assert_eq!(direct.inventory.get(0), Some(Stack::new(Item::GLASS_BOTTLE, 1)));
    assert!(!direct.vitals.is_dead(), "the adapter applies damage after the exchange");
    cli.tick(&mut w, &mut e);
    assert!(cli.vitals.is_dead());
    assert!(cli.inventory.get(0).is_none());
    assert!(e.items.iter().any(|item| item.stack.item == Item::GLASS_BOTTLE));
}

#[test]
fn personal_respawn_uses_each_players_point_and_forgets_only_their_broken_bed() {
    let mut w = world();
    let mut host = session(Item::APPLE);
    host.id = PlayerId::HOST;
    host.spawn_point = Some(IVec3::new(2, 145, 2));
    let mut cli = Agent::new(host.player.pos);
    cli.session = session(Item::APPLE);
    cli.spawn_point = Some(IVec3::new(8, 145, 8));
    cli.spawn_bed = Some(IVec3::new(10, 145, 10));
    let result = cli.session.personal_spawn(&w);
    assert!(result.missing_bed);
    assert_eq!(result.position, Some(DVec3::new(8.5, 145., 8.5)));
    cli.vitals.damage(100., "test", false);
    cli.execute(Command::Respawn, &mut w, &mut Entities::new(7), &[]).unwrap();
    assert_eq!(cli.player.pos, result.position.unwrap());
    assert_eq!(host.personal_spawn(&w).position, Some(DVec3::new(2.5, 145., 2.5)));
    assert_eq!(host.spawn_point, Some(IVec3::new(2, 145, 2)));
}
#[test]
fn food_effects_chances_and_always_edible_foods_match_the_reference() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/java/26.3/consumption.json")).unwrap();
    for (name, case) in fixture["food_effects"].as_object().unwrap() {
        let item = Item::from_name(name).unwrap();
        assert_eq!(item.food_effect_chance(), case["chance"].as_f64().unwrap() as f32, "{name}");
        // The game has no Absorption hearts, so that reference effect is the only one left out.
        let expected: Vec<_> = case["effects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["effect"] != "absorption")
            .map(|e| (e["effect"].as_str().unwrap(), e["amplifier"].as_u64().unwrap(), e["ticks"].as_u64().unwrap()))
            .collect();
        let actual: Vec<_> =
            item.food_effects().iter().map(|&(e, a, t)| (e.id(), u64::from(a), u64::from(t))).collect();
        assert_eq!(actual, expected, "{name}");
    }
    // Only registered foods can be checked; the others remain unregistered.
    let registered: Vec<_> = fixture["always_edible"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| Item::from_name(n.as_str().unwrap()))
        .collect();
    assert_eq!(registered, vec![Item::GOLDEN_APPLE]);
    assert!(registered.iter().all(|i| i.always_edible()));
    assert!(!Item::APPLE.always_edible());
}
#[test]
fn full_players_eat_golden_apples_and_creative_players_eat_anything_without_using_it_up() {
    let mut e = Entities::new(7);
    let mut golden = session(Item::GOLDEN_APPLE);
    golden.vitals.hunger.food = 20.;
    for _ in 0..32 {
        golden.tick_consumption(0, true, &mut e).unwrap();
    }
    assert!(golden.inventory.get(0).is_none());
    assert_eq!(golden.vitals.effects.get(Effect::Regeneration).map(|a| (a.amplifier, a.ticks)), Some((1, 100)));

    let mut full = session(Item::BREAD);
    full.vitals.hunger.food = 20.;
    assert!(full.tick_consumption(0, true, &mut e).is_err(), "full survival players cannot eat bread");
    full.set_mode(GameMode::Adventure);
    assert!(full.tick_consumption(0, true, &mut e).is_err(), "Adventure still needs hunger");
    full.vitals.hunger.food = 8.;
    assert!(full.tick_consumption(0, true, &mut e).is_ok(), "Adventure players eat when hungry");

    let mut creative = session(Item::PUFFERFISH);
    creative.set_mode(GameMode::Creative);
    creative.vitals.hunger.food = 20.;
    for _ in 0..32 {
        creative.tick_consumption(0, true, &mut e).unwrap();
    }
    assert_eq!(creative.inventory.get(0), Some(Stack::new(Item::PUFFERFISH, 1)), "infinite materials");
    for (effect, amplifier) in [(Effect::Poison, 1), (Effect::Hunger, 2), (Effect::Nausea, 0)] {
        assert_eq!(creative.vitals.effects.get(effect).map(|a| a.amplifier), Some(amplifier), "{effect:?}");
    }
    let mut stew = session(Item::MUSHROOM_STEW);
    stew.set_mode(GameMode::Creative);
    for _ in 0..32 {
        stew.tick_consumption(0, true, &mut e).unwrap();
    }
    assert_eq!(stew.inventory.get(0), Some(Stack::new(Item::MUSHROOM_STEW, 1)), "no bowl exchange either");
}
