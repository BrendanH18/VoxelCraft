//! Java 26.3 HoneyBlock, Foods/Consumables/Items and built-in recipe/loot data.
use glam::{DVec3, IVec3};
use std::{process::Command as Process, sync::Arc};
use voxelcraft::{
    agent::{Agent, Command},
    crafting::Grid,
    entity::{Entities, PlayerId},
    inventory::{Inventory, Stack},
    item::Item,
    player::{MoveInput, Player},
    rules::GameMode,
    session::PlayerSession,
    simulation::{self, effects::Effect, survival::Vitals},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        shape,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};
const P: IVec3 = IVec3::new(4, 145, 4);
fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w.receive_chunk(IVec3::new(0, 5, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}
fn session() -> PlayerSession {
    PlayerSession::new(PlayerId::default(), P.as_dvec3() + DVec3::splat(0.5))
}
fn drink(s: &mut PlayerSession, e: &mut Entities) {
    assert_eq!(s.consumption_duration(0), Ok(40));
    for _ in 0..39 {
        assert!(s.tick_consumption(0, true, e).unwrap().completed.is_none());
    }
    assert_eq!(s.inventory.get(0).unwrap().item, Item::HONEY_BOTTLE);
    assert_eq!(s.tick_consumption(0, true, e).unwrap().completed.unwrap().item, Item::HONEY_BOTTLE);
}
#[test]
fn honey_registration_collision_outline_loot_and_pick_block() {
    let b = Block::HONEY_BLOCK;
    assert_eq!(b.0, 8128);
    assert_eq!(Item::from_block(b), Item(1072));
    assert_eq!(Item::HONEY_BOTTLE.0, 1073);
    assert_eq!(Item::from_name("minecraft:honey_block"), Some(Item::HONEY_BLOCK));
    assert_eq!(Item::from_name("honey_bottle"), Some(Item::HONEY_BOTTLE));
    assert_eq!(b.hardness(), 0.0);
    assert_eq!(b.best_tool(), None);
    assert_eq!(b.harvest_level(), None);
    assert!(!b.is_opaque() && b.is_solid());
    assert_eq!(b.light_opacity(), 0);
    assert_eq!(b.drop(), Some(Item::HONEY_BLOCK));
    assert_eq!(Item::HONEY_BOTTLE.max_stack(), 16);
    assert_eq!(Item::HONEY_BOTTLE.food(), Some((6, 1.2)));
    assert!(Item::HONEY_BOTTLE.always_edible());
    for item in [Item::HONEY_BLOCK, Item::HONEY_BOTTLE] {
        assert!(Item::creative_palette().any(|i| i == item));
        assert!(item.matches_query("honey"));
    }
    assert_eq!(shape::outline(b, |_| Block::AIR, Block::AIR).as_slice(), &[shape::Box16::FULL]);
    let boxes = shape::collision_or_cube(b, |_| Block::AIR, Block::AIR, shape::Above::NONE);
    assert_eq!(boxes.as_slice(), &[shape::Box16 { min: [1, 0, 1], max: [15, 15, 15] }]);
    let mut w = world();
    w.set_block(P, b);
    w.spill_mined(P, b, Default::default());
    assert_eq!(w.drops, [(P, Stack::new(Item::HONEY_BLOCK, 1))]);
    let mut s = session();
    s.mode = GameMode::Creative;
    s.player.pos = P.as_dvec3() + DVec3::new(-2.0, -1.12, 0.5);
    assert!(s.pick_block(0, &w).is_some());
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::HONEY_BLOCK, 64)));
    assert_eq!(voxelcraft::session::interact::picked_item(b), Item::HONEY_BLOCK);
    s.set_mode(GameMode::Survival);
    s.inventory.slots[1] = Some(Stack::new(Item::HONEY_BLOCK, 1));
    s.place_held(1, &mut w, &[]).unwrap();
    assert_eq!(w.get_block(P - IVec3::X), Some(Block::HONEY_BLOCK));
    assert!(s.inventory.get(1).is_none());
}
#[test]
fn honey_drinking_removes_only_poison_and_returns_bottles_in_all_stack_cases() {
    let mut e = Entities::new(3);
    let mut s = session();
    s.inventory.slots[0] = Some(Stack::new(Item::HONEY_BOTTLE, 2));
    s.vitals.hunger.food = 10.0;
    s.vitals.hunger.saturation = 0.0;
    s.vitals.apply_effect(Effect::Poison, 0, 1200);
    s.vitals.apply_effect(Effect::Poison, 1, 200);
    s.vitals.apply_effect(Effect::Speed, 0, 1200);
    drink(&mut s, &mut e);
    assert_eq!(s.vitals.hunger.food, 16.0);
    assert_eq!(s.vitals.hunger.saturation, 1.2);
    assert!(!s.vitals.effects.has(Effect::Poison));
    assert!(s.vitals.effects.has(Effect::Speed));
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::HONEY_BOTTLE, 1)));
    assert_eq!(s.inventory.get(1), Some(Stack::new(Item::GLASS_BOTTLE, 1)));
    s.vitals.hunger.food = 20.0;
    drink(&mut s, &mut e);
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::GLASS_BOTTLE, 1)));
    s.inventory.slots.fill(Some(Stack::new(Item::from_block(Block::STONE), 64)));
    s.inventory.slots[0] = Some(Stack::new(Item::HONEY_BOTTLE, 2));
    drink(&mut s, &mut e);
    assert_eq!(e.items.len(), 1);
    assert_eq!(e.items[0].stack, Stack::new(Item::GLASS_BOTTLE, 1));
    s.mode = GameMode::Creative;
    s.vitals.apply_effect(Effect::Poison, 0, 600);
    drink(&mut s, &mut e);
    assert!(!s.vitals.effects.has(Effect::Poison));
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::HONEY_BOTTLE, 1)));
    assert_eq!(e.items.len(), 1, "creative leaves no extra bottle");
}
#[test]
fn honey_recipes_match_java_and_return_every_crafting_bottle() {
    let mut grid = Grid::new(2);
    grid.cells[..4].fill(Some(Stack::new(Item::HONEY_BOTTLE, 2)));
    assert_eq!(grid.result(), Some(Stack::new(Item::HONEY_BLOCK, 1)));
    assert_eq!(grid.consume_with_remainders(), vec![Stack::new(Item::GLASS_BOTTLE, 1); 4]);
    assert_eq!(grid.result(), Some(Stack::new(Item::HONEY_BLOCK, 1)));
    assert!(grid.consume_with_remainders().is_empty());
    assert!(grid.cells[..4].iter().all(|c| *c == Some(Stack::new(Item::GLASS_BOTTLE, 1))));
    let mut grid = Grid::new(3);
    for (i, item) in [Item::HONEY_BLOCK, Item::GLASS_BOTTLE, Item::GLASS_BOTTLE, Item::GLASS_BOTTLE, Item::GLASS_BOTTLE]
        .into_iter()
        .enumerate()
    {
        grid.cells[i] = Some(Stack::new(item, 1));
    }
    assert_eq!(grid.result(), Some(Stack::new(Item::HONEY_BOTTLE, 4)));
    assert!(grid.consume_with_remainders().is_empty());
    assert!(grid.cells.iter().all(Option::is_none));
    grid.cells[8] = Some(Stack::new(Item::HONEY_BOTTLE, 1));
    assert_eq!(grid.result(), Some(Stack::new(Item::SUGAR, 3)));
    grid.consume();
    assert_eq!(grid.cells[8], Some(Stack::new(Item::GLASS_BOTTLE, 1)));
}
#[test]
fn cli_give_drink_and_crafting_use_honey_registrations_and_duration() {
    let mut w = world();
    let mut e = Entities::new(2);
    w.set_block(P, Block::STONE);
    let mut a = Agent::new(P.as_dvec3() + DVec3::new(0.5, 1.0, 0.5));
    a.mode = GameMode::Creative;
    for name in ["honey_block", "honey_bottle"] {
        a.execute(Command::parse(&format!("give {name} 4")).unwrap(), &mut w, &mut e, &[]).unwrap();
    }
    assert!(a.inventory.slots.iter().flatten().any(|s| s.item == Item::HONEY_BLOCK));
    assert!(a.inventory.slots.iter().flatten().any(|s| s.item == Item::HONEY_BOTTLE));
    a.mode = GameMode::Survival;
    a.inventory = Inventory::default();
    a.inventory.slots[0] = Some(Stack::new(Item::HONEY_BOTTLE, 5));
    a.vitals.apply_effect(Effect::Poison, 0, 600);
    // The CLI's `eat` consumes whatever is held, drinks included; a honey
    // bottle is always edible, even at full hunger.
    a.execute(Command::parse("eat").unwrap(), &mut w, &mut e, &[]).unwrap();
    for _ in 0..39 {
        a.tick(&mut w, &mut e);
    }
    assert!(a.vitals.effects.has(Effect::Poison));
    a.tick(&mut w, &mut e);
    assert!(!a.vitals.effects.has(Effect::Poison));
    a.execute(Command::parse("craft honey_block").unwrap(), &mut w, &mut e, &[]).unwrap();
    assert!(a.inventory.slots.iter().flatten().any(|s| s.item == Item::HONEY_BLOCK));
    assert_eq!(
        a.inventory
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == Item::GLASS_BOTTLE)
            .map(|s| s.count as u32)
            .sum::<u32>(),
        5
    );
}
#[test]
fn cli_crafting_drops_glass_remainders_when_the_result_fits_but_bottles_do_not() {
    let mut w = world();
    let mut e = Entities::new(2);
    let mut a = Agent::new(P.as_dvec3());
    a.inventory.slots.fill(Some(Stack::new(Item::from_block(Block::STONE), 64)));
    a.inventory.slots[0] = Some(Stack::new(Item::HONEY_BOTTLE, 8));
    a.inventory.slots[1] = Some(Stack::new(Item::HONEY_BLOCK, 1));
    a.execute(Command::Craft(Item::HONEY_BLOCK), &mut w, &mut e, &[]).unwrap();
    assert_eq!(a.inventory.get(0), Some(Stack::new(Item::HONEY_BOTTLE, 4)));
    assert_eq!(a.inventory.get(1), Some(Stack::new(Item::HONEY_BLOCK, 2)));
    assert_eq!(
        e.items
            .iter()
            .map(|i| {
                assert_eq!(i.stack.item, Item::GLASS_BOTTLE);
                u32::from(i.stack.count)
            })
            .sum::<u32>(),
        4
    );
}

#[test]
fn honey_landing_damage_uses_java_floor_after_the_point_two_multiplier() {
    let mut w = world();
    w.set_block(P, Block::HONEY_BLOCK);
    let mut p = Player::new(P.as_dvec3() + DVec3::new(0.5, 21.0, 0.5));
    let mut v = Vitals::default();
    let start = p.pos.y;
    for _ in 0..120 {
        let step = simulation::tick_player(&mut p, &mut w, &mut v, &[None; 4], MoveInput::default(), false);
        if p.on_ground {
            let distance = start - p.pos.y;
            assert_eq!(step.hurts.fall, ((distance + 1e-6 - 3.0) * f64::from(0.2_f32)).floor() as f32);
            assert_eq!(step.hurts.fall, 3.0);
            assert!(simulation::player_environment(&p, &w, 0.0).on_honey);
            assert!(p.honey_landed);
            voxelcraft::particles::movement_effects(&p, p.pos, &mut w);
            assert!(
                w.particles
                    .drain()
                    .any(|r| matches!(r, voxelcraft::particles::Request::Honey { count: 10, sound: true, .. }))
            );
            p.tick(MoveInput::default(), &w);
            assert!(!p.honey_landed, "standing still does not repeat landing feedback");
            return;
        }
    }
    panic!("no landing");
}
#[test]
fn honey_block_and_items_survive_separate_process_save_reload() {
    let path = std::env::temp_dir().join(format!(
        "voxelcraft-honey-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for phase in ["write", "read"] {
        let output = Process::new(std::env::current_exe().unwrap())
            .args(["--exact", "honey_persistence_worker", "--nocapture"])
            .env("VOXELCRAFT_HONEY_SAVE", &path)
            .env("VOXELCRAFT_HONEY_PHASE", phase)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{phase}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn honey_persistence_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_HONEY_SAVE") else { return };
    let storage = Storage::new(path);
    let _lock = storage.lock().unwrap();
    let mut w = world();
    if std::env::var("VOXELCRAFT_HONEY_PHASE").unwrap() == "write" {
        w.set_block(P, Block::HONEY_BLOCK);
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(Item::HONEY_BLOCK, 64));
        inventory.slots[1] = Some(Stack::new(Item::HONEY_BOTTLE, 16));
        storage
            .save(
                &LevelInfo { seed: 7, player: None, props: [("inventory".into(), inventory.serialize())].into() },
                &w.modified_chunks(),
            )
            .unwrap();
    } else {
        let level = storage.load_level().unwrap();
        let inventory = Inventory::deserialize(&level.props["inventory"]).unwrap();
        for (pos, data) in storage.load_chunks().unwrap() {
            w.receive_chunk(pos, data);
        }
        assert_eq!(w.get_block(P), Some(Block::HONEY_BLOCK));
        assert_eq!(inventory.get(0), Some(Stack::new(Item::HONEY_BLOCK, 64)));
        assert_eq!(inventory.get(1), Some(Stack::new(Item::HONEY_BOTTLE, 16)));
        assert_eq!(inventory.get(0).unwrap().item.places(), Some(Block::HONEY_BLOCK));
        assert_eq!(inventory.get(1).unwrap().item.max_stack(), 16);
    }
}
