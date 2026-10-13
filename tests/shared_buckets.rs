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
    survival_items::bucket::{self, Effect, Use},
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        composter, overworld_blocks, redstone_blocks,
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
    bucket::use_bucket(
        &agent.session.player,
        &mut agent.session.inventory,
        agent.selected,
        agent.session.mode,
        world,
        entities,
        6.,
    )
}

/// The ordering step the desktop/controller client runs after its entity interactions.
fn desktop_order(world: &mut World, agent: &mut Agent, entities: &mut Entities, sneaking: bool) -> Use {
    bucket::use_held(
        &agent.session.player,
        &mut agent.session.inventory,
        agent.selected,
        agent.session.mode,
        sneaking,
        world,
        entities,
        6.,
    )
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
    assert!(
        bucket::use_bucket(
            &agent.session.player,
            &mut agent.session.inventory,
            0,
            GameMode::Survival,
            &mut world,
            &mut entities,
            1.
        )
        .is_none()
    );
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

/// A lever on the near face of the stone, at the pour cell.
fn lever_on_target(world: &mut World) -> Block {
    let mount = 1 + Facing::ALL.iter().position(|f| -f.offset() == IVec3::Z).unwrap() as u8;
    let lever = redstone_blocks::lever(mount, false);
    assert!(world.set_block(POUR, lever));
    lever
}

#[test]
fn block_use_precedes_buckets_and_bottles_in_the_shared_order_and_on_the_cli() {
    for held in [Item::BUCKET, Item::WATER_BUCKET, Item::LAVA_BUCKET, Item::GLASS_BOTTLE, Item::COD_BUCKET] {
        let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
        let lever = lever_on_target(&mut world);
        agent.inventory.slots[0] = Some(Stack::new(held, 1));
        assert_eq!(desktop_order(&mut world, &mut agent, &mut entities, false), Use::Block(POUR), "{held:?}");
        assert_eq!(world.get_block(POUR), Some(lever));
        agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_ne!(world.get_block(POUR), Some(lever), "{held:?} should flip the lever");
        assert_eq!(agent.inventory.get(0), Some(Stack::new(held, 1)));
        assert!(entities.mobs.is_empty());
        assert!(agent.events.iter().all(|e| !matches!(e, Event::Bucket(_))));
    }
}

#[test]
fn sneaking_skips_block_use_and_only_a_full_composter_claims_a_bucket() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    lever_on_target(&mut world);
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    let face = POUR - IVec3::Z;
    assert_eq!(
        desktop_order(&mut world, &mut agent, &mut entities, true),
        Use::Item(Some(Effect::Poured { at: face, fluid: Block::WATER }))
    );

    for level in [3, 8] {
        let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
        world.set_block(HIT, composter::state(level));
        agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
        let order = desktop_order(&mut world, &mut agent, &mut entities, false);
        if level == 8 {
            assert_eq!(order, Use::Block(HIT));
            agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
            assert_eq!(world.get_block(HIT).and_then(composter::level), Some(0), "a full composter empties first");
            assert_eq!(world.drops.len() + entities.items.len(), 1);
            assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::WATER_BUCKET, 1)));
        } else {
            assert_eq!(order, Use::Item(Some(Effect::Poured { at: POUR, fluid: Block::WATER })));
        }
    }
}

#[test]
fn cli_buckets_and_bottles_mount_carts_before_their_own_use() {
    for held in [Item::BUCKET, Item::WATER_BUCKET, Item::LAVA_BUCKET, Item::GLASS_BOTTLE] {
        let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
        let kind = voxelcraft::entity::minecart::CartKind::from_item(Item::MINECART).unwrap();
        let cart = entities.spawn_cart(kind, DVec3::new(5.5, 102.2, 4.5));
        agent.inventory.slots[0] = Some(Stack::new(held, 1));
        agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(agent.player.vehicle, Some(cart), "{held:?}");
        assert_eq!(world.get_block(POUR), Some(Block::AIR));
        assert_eq!(agent.inventory.get(0), Some(Stack::new(held, 1)));
    }
}

#[test]
fn cli_tropical_fish_bucket_feeds_an_axolotl_instead_of_releasing_the_fish() {
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    // Level with the eye in front of the stone; entities are not ticked here.
    entities.spawn(MobKind::Axolotl, DVec3::new(5.5, 102.3, 4.));
    agent.inventory.slots[0] = Some(Stack::new(Item::TROPICAL_FISH_BUCKET, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(entities.mobs.len(), 1, "no tropical fish was released");
    assert_eq!(agent.inventory.get(0).unwrap().item, Item::WATER_BUCKET);
    assert!(agent.events.iter().all(|e| !matches!(e, Event::Bucket(_))));
}

#[test]
fn adventure_fails_fluid_buckets_but_fills_bottles_and_captures_creatures() {
    for held in [Item::WATER_BUCKET, Item::LAVA_BUCKET, Item::COD_BUCKET] {
        let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
        agent.set_mode(GameMode::Adventure);
        agent.inventory.slots[0] = Some(Stack::new(held, 1));
        assert_eq!(direct(&mut world, &mut agent, &mut entities), None);
        assert!(agent.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(world.get_block(POUR), Some(Block::AIR));
        assert!(entities.mobs.is_empty());
        assert_eq!(agent.inventory.get(0), Some(Stack::new(held, 1)));
    }
    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    world.set_block(HIT, Block::WATER);
    agent.set_mode(GameMode::Adventure);
    agent.inventory.slots[0] = Some(Stack::new(Item::BUCKET, 1));
    assert!(agent.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
    assert_eq!(world.get_block(HIT), Some(Block::WATER));
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::BUCKET, 1)));
    agent.inventory.slots[0] = Some(Stack::new(Item::GLASS_BOTTLE, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(agent.inventory.get(0).unwrap().item, Item::potion(voxelcraft::potion::Potion::WATER));

    let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
    agent.set_mode(GameMode::Adventure);
    entities.spawn(MobKind::Cod, DVec3::new(5.5, 102.5, 4.));
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    agent.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(agent.inventory.get(0).unwrap().item, Item::COD_BUCKET);
}

#[test]
fn buckets_pour_on_the_hit_face_of_targeted_grass_and_ferns() {
    for plant in [Block::TALL_GRASS, Block::FERN] {
        for (held, fluid) in [(Item::WATER_BUCKET, Block::WATER), (Item::LAVA_BUCKET, Block::LAVA)] {
            let (mut world, mut agent, mut entities) = scene(Dimension::Overworld);
            world.set_block(HIT - IVec3::Y, Block::DIRT);
            world.set_block(HIT, plant);
            agent.inventory.slots[0] = Some(Stack::new(held, 1));
            assert_eq!(direct(&mut world, &mut agent, &mut entities), Some(Effect::Poured { at: POUR, fluid }));
            assert_eq!(world.get_block(HIT), Some(plant), "the targeted plant stays");
            assert_eq!(world.get_block(POUR), Some(fluid));
        }
    }
}

#[test]
fn emptying_into_a_plant_cell_destroys_it_with_its_drops() {
    for (plant, fluid) in
        [(Block::DANDELION, Block::WATER), (Block::FERN, Block::LAVA), (Block::TALL_GRASS, Block::WATER)]
    {
        let (mut world, _, _) = scene(Dimension::Overworld);
        world.set_block(POUR - IVec3::Y, Block::DIRT);
        world.set_block(POUR, plant);
        world.drops.clear();
        assert_eq!(
            bucket::empty_contents(&mut world, HIT, -IVec3::Z, fluid, false),
            Some(Effect::Poured { at: POUR, fluid })
        );
        assert_eq!(world.get_block(POUR), Some(fluid));
        assert!(world.drops.iter().all(|(at, _)| *at == POUR));
        if plant == Block::DANDELION {
            assert_eq!(world.drops, [(POUR, Stack::new(Item::from_block(Block::DANDELION), 1))]);
        }
    }
}

#[test]
fn water_fills_water_holding_plants_in_place_but_lava_and_sneaking_use_the_face() {
    let seagrass = overworld_blocks::SEAGRASS;
    let setup = || {
        let (mut world, agent, entities) = scene(Dimension::Overworld);
        world.set_block(HIT - IVec3::Y, Block::STONE);
        world.set_block(HIT, seagrass);
        (world, agent, entities)
    };
    // Java targets the LiquidBlockContainer itself; seagrass already holds water, so the bucket just empties.
    let (mut world, mut agent, mut entities) = setup();
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    assert_eq!(direct(&mut world, &mut agent, &mut entities), Some(Effect::Poured { at: HIT, fluid: Block::WATER }));
    assert_eq!(world.get_block(HIT), Some(seagrass));
    assert_eq!(world.get_block(POUR), Some(Block::AIR));
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::BUCKET, 1)));

    let (mut world, mut agent, mut entities) = setup();
    agent.inventory.slots[0] = Some(Stack::new(Item::COD_BUCKET, 1));
    assert_eq!(direct(&mut world, &mut agent, &mut entities), Some(Effect::Poured { at: HIT, fluid: Block::WATER }));
    assert_eq!(world.get_block(HIT), Some(seagrass));
    assert_eq!(entities.mobs.len(), 1);
    assert_eq!(entities.mobs[0].pos.floor().as_ivec3(), HIT);

    let (mut world, mut agent, mut entities) = setup();
    agent.inventory.slots[0] = Some(Stack::new(Item::LAVA_BUCKET, 1));
    assert_eq!(direct(&mut world, &mut agent, &mut entities), Some(Effect::Poured { at: POUR, fluid: Block::LAVA }));
    assert_eq!(world.get_block(HIT), Some(seagrass));

    let (mut world, mut agent, mut entities) = setup();
    agent.player.sneaking = true;
    agent.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    assert_eq!(direct(&mut world, &mut agent, &mut entities), Some(Effect::Poured { at: POUR, fluid: Block::WATER }));
    assert_eq!(world.get_block(POUR), Some(Block::WATER));
}
