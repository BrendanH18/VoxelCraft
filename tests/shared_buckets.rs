//! Fluid actions use the same device-free entry point as the desktop/controller client.
//! Reference: Java 26.3 BucketItem/ItemUtils; provenance in fixtures/java/26.3/README.md.
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use glam::{DVec3, IVec3};
use voxelcraft::{
    agent::{Agent, Command, Event},
    entity::{Entities, MobKind},
    inventory::{Inventory, Stack},
    item::Item,
    rules::GameMode,
    survival_items::bucket::{self, Effect},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        terrain::{Dimension, Generator},
    },
};

const HIT: IVec3 = IVec3::new(5, 102, 5);
const POUR: IVec3 = IVec3::new(5, 102, 4);

fn load(dimension: Dimension, saved: rustc_hash::FxHashMap<IVec3, Arc<ChunkData>>) -> World {
    let mut world = World::new_headless(Arc::new(Generator::for_dimension(1, dimension)), saved, 1);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !world.is_loaded(HIT) {
        world.update(DVec3::new(5.5, 101., 2.5));
        assert!(Instant::now() < deadline, "fixture chunk didn't load");
        std::thread::yield_now();
    }
    world
}

fn scene(dimension: Dimension) -> (World, Agent, Entities) {
    let saved = [(IVec3::new(0, 3, 0), Arc::new(ChunkData::Uniform(Block::AIR)))].into_iter().collect();
    let mut world = load(dimension, saved);
    world.set_block(HIT, Block::STONE);
    let mut agent = Agent::new(DVec3::new(5.5, 101., 2.5));
    agent.player.yaw = std::f32::consts::FRAC_PI_2;
    (world, agent, Entities::new(1))
}

fn direct(world: &mut World, agent: &mut Agent, entities: &mut Entities) -> Option<Effect> {
    bucket::use_bucket(&agent.player, &mut agent.inventory, agent.selected, agent.creative, world, entities, 6.)
}

#[test]
fn cli_pours_water_and_lava_with_the_same_result_as_the_shared_client_action() {
    for mode in [GameMode::Survival, GameMode::Creative] {
        for (held, fluid) in [(Item::WATER_BUCKET, Block::WATER), (Item::LAVA_BUCKET, Block::LAVA)] {
            let (mut w, mut a, mut e) = scene(Dimension::Overworld);
            a.set_mode(mode);
            a.inventory.slots[0] = Some(Stack::new(held, 1));
            assert_eq!(direct(&mut w, &mut a, &mut e), Some(Effect::Poured { at: POUR, fluid }));
            let inventory = a.inventory.serialize();
            let (mut cli_world, mut cli, mut cli_entities) = scene(Dimension::Overworld);
            cli.set_mode(mode);
            cli.inventory.slots[0] = Some(Stack::new(held, 1));
            cli.execute(Command::Place, &mut cli_world, &mut cli_entities, &[]).unwrap();
            assert_eq!(cli_world.get_block(POUR), Some(fluid));
            assert_eq!(cli.inventory.serialize(), inventory);
            assert_eq!(cli.events, [Event::Bucket(Effect::Poured { at: POUR, fluid })]);
            assert_eq!(cli.inventory.get(0).unwrap().item, if mode.is_creative() { held } else { Item::BUCKET });
        }
    }
}

#[test]
fn fluid_and_inventory_survive_reload_after_pouring_then_pickup() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    let saved = world.modified_chunks().into_iter().collect();
    let mut restored_world = load(Dimension::Overworld, saved);
    agent.inventory = Inventory::deserialize(&agent.inventory.serialize()).unwrap();
    agent.execute(Command::Place, &mut restored_world, &mut entities, &[]).unwrap();
    assert_eq!(restored_world.get_block(POUR), Some(Block::AIR));
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::WATER_BUCKET, 1)));
}

#[test]
fn stacked_empty_buckets_drop_overflow_without_losing_the_remaining_bucket() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    world.set_block(HIT, Block::WATER);
    agent.inventory.slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
    agent.inventory.slots[0] = Some(Stack::new(Item::BUCKET, 2));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(world.get_block(HIT), Some(Block::AIR));
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::BUCKET, 1)));
    assert_eq!(entities.items.len(), 1);
    assert_eq!(entities.items[0].stack, Stack::new(Item::WATER_BUCKET, 1));
}

#[test]
fn bottles_leave_the_source_and_creative_filling_does_not_duplicate_results() {
    for mode in [GameMode::Survival, GameMode::Creative] {
        let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
        world.set_block(HIT, Block::WATER);
        agent.set_mode(mode);
        agent.inventory.slots[0] = Some(Stack::new(Item::GLASS_BOTTLE, 2));
        agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(world.get_block(HIT), Some(Block::WATER));
        assert_eq!(agent.inventory.get(0).unwrap().count, if mode.is_creative() { 2 } else { 1 });
        if mode.is_creative() {
            let before = agent.inventory.serialize();
            agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
            assert_eq!(agent.inventory.serialize(), before);
        }
    }
}

#[test]
fn water_evaporates_in_the_nether_but_lava_pours() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Nether);
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    assert_eq!(direct(&mut world, &mut agent, &mut entities), Some(Effect::Evaporated { at: POUR }));
    assert_eq!(world.get_block(POUR), Some(Block::AIR));
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::BUCKET, 1)));
    agent.inventory.slots[0] = Some(Stack::new(Item::LAVA_BUCKET, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(world.get_block(POUR), Some(Block::LAVA));
}

#[test]
fn misses_out_of_reach_and_spectator_actions_leave_inventory_and_world_unchanged() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    for mode in [GameMode::Survival, GameMode::Spectator] {
        agent.set_mode(mode);
        if mode == GameMode::Survival {
            agent.player.yaw = -std::f32::consts::FRAC_PI_2;
        } else {
            agent.player.yaw = std::f32::consts::FRAC_PI_2;
        }
        let before = agent.inventory.serialize();
        assert!(agent.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(agent.inventory.serialize(), before);
        assert_eq!(world.get_block(POUR), Some(Block::AIR));
    }
    agent.set_mode(GameMode::Survival);
    assert!(bucket::use_bucket(&agent.player, &mut agent.inventory, 0, false, &mut world, &mut entities, 1.).is_none());
    assert_eq!(world.get_block(POUR), Some(Block::AIR));
}

#[test]
fn creature_capture_still_precedes_pouring_and_preserves_saved_payload() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    entities.spawn(MobKind::Cod, DVec3::new(5.5, 102.5, 4.));
    entities.mobs[0].health = 1.5;
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert!(entities.mobs.is_empty());
    assert_eq!(agent.inventory.get(0).unwrap().item, Item::COD_BUCKET);
    assert_eq!(world.get_block(POUR), Some(Block::AIR));
    agent.inventory = Inventory::deserialize(&agent.inventory.serialize()).unwrap();
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(entities.mobs.len(), 1);
    assert_eq!(entities.mobs[0].health, 1.5);
    assert!(entities.mobs[0].persistent);
    assert_eq!(world.get_block(POUR), Some(Block::WATER));
    assert_eq!(agent.inventory.get(0).unwrap().item, Item::BUCKET);
}

#[test]
fn cli_empty_buckets_still_milk_cows() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    world.set_block(HIT, Block::AIR);
    agent.player.pitch = -0.2;
    entities.spawn(MobKind::Cow, DVec3::new(5.5, 101., 4.));
    agent.inventory.slots[0] = Some(Stack::new(Item::BUCKET, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(agent.inventory.get(0).unwrap().item, Item::MILK_BUCKET);
}
