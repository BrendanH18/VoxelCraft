//! Java 26.3 death and respawn (`ServerPlayer.die`, `Player.dropEquipment`,
//! `getBaseExperienceReward`, `restoreFrom`) through the shared
//! [`PlayerSession`] step that the desktop, controller, LAN and CLI adapters
//! all take, here driven through the CLI agent that controller and LAN
//! players also run on.
use std::sync::Arc;

use glam::{DVec3, IVec3};
use voxelcraft::{
    agent::{Agent, Command},
    enchant::{Enchantment, Enchants},
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    rules::{GameMode, GameRules},
    session::PlayerSession,
    simulation::{effects::Effect, survival::Vitals},
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

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

fn rules(keep_inventory: bool) -> GameRules {
    let mut rules = GameRules::default();
    rules.set("keepInventory", if keep_inventory { "true" } else { "false" }).unwrap();
    rules
}

/// A carrying player: diamonds, a cursed sword, a work input and ten levels.
fn loaded(mode: GameMode) -> Agent {
    let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    a.set_mode(mode);
    a.inventory.add(Item::DIAMOND, 5);
    a.inventory.slots[1] = Some(Stack {
        enchants: Enchants::NONE.with(Enchantment::VanishingCurse, 1),
        ..Stack::new(Item::tool(ToolKind::Sword, Tier::Iron), 1)
    });
    a.work[0] = Some(Stack::new(Item::LAPIS_LAZULI, 3));
    a.vitals.xp.add_levels(10);
    a.vitals.effects.add(Effect::Speed, 0, 600);
    a
}

fn dropped(entities: &Entities, item: Item) -> u32 {
    entities.items.iter().filter(|i| i.stack.item == item).map(|i| i.stack.count as u32).sum()
}

fn carried(session: &PlayerSession, item: Item) -> u32 {
    session.inventory.slots.iter().flatten().filter(|s| s.item == item).map(|s| s.count as u32).sum()
}

fn orbs(entities: &Entities) -> u32 {
    entities.orbs.iter().map(|o| o.value).sum()
}

#[test]
fn deaths_drop_everything_but_vanishing_items_in_every_mode_but_spectator() {
    let mut world = world();
    for mode in [GameMode::Survival, GameMode::Adventure, GameMode::Creative] {
        let mut entities = Entities::new(1);
        let mut a = loaded(mode);
        a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
        a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
        assert!(a.vitals.is_dead(), "{mode:?}");
        assert!(a.inventory.slots.iter().all(Option::is_none) && a.work == [None; 3], "{mode:?}");
        assert_eq!(dropped(&entities, Item::DIAMOND), 5, "{mode:?}");
        assert_eq!(dropped(&entities, Item::LAPIS_LAZULI), 3, "work inputs drop too ({mode:?})");
        assert_eq!(entities.items.len(), 2, "Curse of Vanishing destroys the sword ({mode:?})");
        // `getBaseExperienceReward`: 7 per level, at most 100.
        assert_eq!(orbs(&entities), 70, "{mode:?}");
        assert_eq!(a.vitals.xp.level, 0);
        assert!(a.vitals.effects.is_empty(), "effects end with the death");
    }
    // `ServerPlayer.die` skips the loot for spectators.
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Spectator);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
    assert!(a.vitals.is_dead());
    assert_eq!((entities.items.len(), orbs(&entities)), (0, 0));
    assert_eq!(a.vitals.xp.level, 10);
}

#[test]
fn keep_inventory_keeps_items_and_experience_through_the_cli_respawn() {
    let mut world = world();
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(true));
    let xp = a.vitals.xp;
    a.hurt(100.0, "was slain by a zombie", DVec3::ZERO, &mut entities);
    assert!(a.vitals.is_dead());
    assert!(entities.items.is_empty() && entities.orbs.is_empty());
    // Before the shared step, the CLI respawn reset experience even with
    // keepInventory (the controller and LAN adapters patched it back).
    a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
    assert!(!a.vitals.is_dead());
    assert_eq!(a.vitals.xp, xp, "keepInventory keeps experience");
    assert_eq!(carried(&a, Item::DIAMOND), 5);
    assert_eq!(a.work[0], Some(Stack::new(Item::LAPIS_LAZULI, 3)));
}

#[test]
fn respawn_is_a_fresh_body_that_keeps_the_enchantment_seed_and_spawn() {
    let mut world = world();
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.vitals.xp.seed = 12345;
    a.spawn_point = Some(IVec3::new(4, 150, 4));
    a.player.vel = DVec3::new(3.0, -20.0, 0.0);
    a.player.sneaking = true;
    a.vitals.hunger.food = 3.0;
    a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
    assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err(), "the dead only respawn");
    a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
    let fresh = Vitals::default();
    assert_eq!((a.vitals.health, a.vitals.hunger.food, a.vitals.air), (fresh.health, fresh.hunger.food, fresh.air));
    assert!(a.vitals.effects.is_empty() && a.vitals.death.is_none());
    // `restoreFrom` always copies the enchantment seed; experience only with
    // the inventory.
    assert_eq!((a.vitals.xp.level, a.vitals.xp.seed), (0, 12345));
    assert_eq!(a.player.pos, DVec3::new(4.5, 150.0, 4.5), "at the spawn point");
    assert_eq!(a.spawn_point, Some(IVec3::new(4, 150, 4)));
    assert_eq!(a.player.vel, DVec3::ZERO);
    assert!(!a.player.sneaking && !a.player.flying && a.player.vehicle.is_none());
}

#[test]
fn the_desktop_crafting_grid_drops_or_returns_with_the_inventory() {
    let mut entities = Entities::new(1);
    let grid = vec![Stack::new(Item::STICK, 2), Stack::new(Item::COAL, 1)];
    let mut dying = PlayerSession::new(PlayerId::HOST, DVec3::new(0.5, 70.0, 0.5));
    dying.vitals.damage(100.0, "fell out of the world", false);
    dying.die(&mut entities, false, grid.clone());
    assert_eq!((dropped(&entities, Item::STICK), dropped(&entities, Item::COAL)), (2, 1));

    let mut entities = Entities::new(1);
    let mut kept = PlayerSession::new(PlayerId::HOST, DVec3::new(0.5, 70.0, 0.5));
    kept.vitals.damage(100.0, "fell out of the world", false);
    kept.die(&mut entities, true, grid);
    assert!(entities.items.is_empty());
    assert_eq!((carried(&kept, Item::STICK), carried(&kept, Item::COAL)), (2, 1));
}

#[test]
fn a_player_saved_dead_loads_dead_and_dies_only_once() {
    let mut world = world();
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
    let items = entities.items.len();
    let xp = orbs(&entities);
    // The level file keeps health, air and the death cause.
    let saved = (a.vitals.health, a.vitals.air, a.vitals.death.clone());
    let mut b = Agent::new(a.player.pos);
    b.vitals = Vitals::restore(saved.0, saved.1, saved.2);
    assert!(b.vitals.is_dead(), "loads on the death screen");
    // The desktop runs the death step again on load; it has nothing left
    // to drop, so nothing is duplicated.
    b.die(&mut entities, false, Vec::new());
    assert_eq!((entities.items.len(), orbs(&entities)), (items, xp));
    b.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
    assert!(!b.vitals.is_dead());
}
