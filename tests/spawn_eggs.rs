//! v0.18 Track 4: Java 26.3's spawn eggs against
//! `tests/fixtures/java/26.3/spawn_eggs.json` (`tools/extract_java_creative_fixtures.py`):
//! the registry, `SpawnEggItem.useOn` and `use` through the shared use chain,
//! spawners and trial spawners, babies from a matching mob, Peaceful, and
//! eggs surviving save and reload. Dispensing is in `tests/dispensing.rs`.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    content::spawn_egg::{self, Baby},
    entity::{Entities, MobKind, PlayerId},
    inventory::{Inventory, Stack},
    item::Item,
    java_registry,
    rules::GameMode,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
    },
    simulation::difficulty::Difficulty,
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/spawn_eggs.json")).unwrap()
}

fn gaps() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_gaps.json")).unwrap()
}

fn rgb(hex: &str) -> [u8; 3] {
    let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

#[test]
fn every_egg_for_a_mob_the_game_has_is_registered_in_java_order() {
    let fx = fixture();
    let eggs = fx["eggs"].as_array().unwrap();
    assert_eq!(eggs.len(), 88, "Java 26.3 has 88 spawn eggs");
    let mut k = 0;
    for e in eggs {
        let id = e["item"].as_str().unwrap();
        let english = e["name"].as_str().unwrap();
        let resolved = java_registry::item(id, english);
        let Some(kind) = MobKind::from_name(e["entity"].as_str().unwrap()) else {
            // Only the Ender Dragon's (v1.0 Track 7): the dragon isn't a
            // mob kind, so its egg is item 1937 outside `EGGS`.
            assert_eq!(id, "ender_dragon_spawn_egg");
            assert_eq!(resolved, Some(spawn_egg::ENDER_DRAGON));
            assert_eq!(spawn_egg::ENDER_DRAGON, Item(1937));
            assert_eq!(spawn_egg::ENDER_DRAGON.name(), english.to_lowercase());
            assert_eq!(u64::from(spawn_egg::ENDER_DRAGON.max_stack()), e["max_stack"].as_u64().unwrap());
            assert_eq!(spawn_egg::ENDER_DRAGON_BASE, rgb(e["base"].as_str().unwrap()));
            assert_eq!(spawn_egg::ENDER_DRAGON_SPOTS, rgb(e["spots"].as_str().unwrap()));
            assert_eq!(spawn_egg::ENDER_DRAGON_PEACEFUL, e["allowed_in_peaceful"].as_bool().unwrap());
            assert_eq!(e["in_tab"], false, "no creative tab lists it");
            assert_eq!(e["baby"], "none");
            assert!(gaps()["items"].get(id).is_none(), "{id} is registered, so not a gap");
            continue;
        };
        let item = Item(spawn_egg::ITEM_FIRST + k);
        let egg = &spawn_egg::EGGS[k as usize];
        assert_eq!(resolved, Some(item), "{id}");
        assert_eq!(item.name(), english.to_lowercase());
        assert_eq!(egg.kind, kind, "{id}");
        assert_eq!(spawn_egg::mob(item), Some(kind));
        assert_eq!(u64::from(item.max_stack()), e["max_stack"].as_u64().unwrap(), "{id}");
        assert_eq!(egg.base, rgb(e["base"].as_str().unwrap()), "{id} base colour");
        assert_eq!(egg.spots, rgb(e["spots"].as_str().unwrap()), "{id} spot colour");
        assert_eq!(egg.peaceful, e["allowed_in_peaceful"].as_bool().unwrap(), "{id}");
        let baby = match e["baby"].as_str().unwrap() {
            "breed" => Baby::Breed,
            "set_baby" => Baby::SetBaby,
            _ => Baby::None,
        };
        assert_eq!(egg.baby, baby, "{id}");
        assert!(gaps()["items"].get(id).is_none(), "{id} is registered, so not a gap");
        // Reported, not enforced: where Java's `isAllowedInPeaceful` and the
        // game's own Peaceful despawn rule (another track's) disagree.
        if egg.peaceful == kind.despawns_in_peaceful() {
            println!("peaceful rule differs for {id}: Java allows {}", egg.peaceful);
        }
        k += 1;
    }
    assert_eq!(k as usize, spawn_egg::EGGS.len());
    assert_eq!(spawn_egg::ITEM_END, 1937, "items 1850..=1936, inside Track 4's 1850-1949");
}

#[test]
fn eggs_are_creative_only_and_have_their_own_icons() {
    let recipes = voxelcraft::crafting::recipes();
    for item in spawn_egg::all() {
        assert!(recipes.iter().all(|r| r.result.item != item), "{} has no recipe", item.name());
        let layer = item.icon_layer().expect("an icon");
        assert!(voxelcraft::world::block::tex::item_index(layer).is_some());
    }
    // Every egg's icon is its own layer, the Ender Dragon's included.
    let layers: std::collections::BTreeSet<_> = spawn_egg::all().filter_map(|i| i.icon_layer()).collect();
    assert_eq!(layers.len(), spawn_egg::EGGS.len() + 1);
    assert_eq!(spawn_egg::all().last(), Some(spawn_egg::ENDER_DRAGON));
}

/// The block the player looks at, level with the eye, along +x.
const WALL: IVec3 = IVec3::new(7, 146, 4);

fn scene(egg: MobKind) -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    for x in 3..10 {
        world.set_block(IVec3::new(x, 144, 4), Block::STONE);
    }
    world.set_block(WALL, Block::STONE);
    let mut session = PlayerSession::new(PlayerId(6), DVec3::new(4.5, 145., 4.5));
    session.player.on_ground = true;
    session.inventory.slots[0] = Some(Stack::new(spawn_egg::egg(egg).unwrap(), 3));
    (world, session, Entities::new(7))
}

fn use_egg(s: &mut PlayerSession, w: &mut World, e: &mut Entities) -> Result<Option<Used>, &'static str> {
    s.use_held(0, w, e, &UseContext { sneaking: false, others: &[] }).map(|u| u.map(|u| u.used))
}

fn count(s: &PlayerSession) -> u8 {
    s.inventory.get(0).map_or(0, |s| s.count)
}

#[test]
fn an_egg_on_a_side_face_spawns_in_front_dropping_at_most_a_block() {
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    let used = use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(e.mobs.len(), 1);
    let pig = &e.mobs[0];
    assert_eq!(pig.kind, MobKind::Pig);
    // In front of the clicked west face, at the cell's centre; nothing
    // below within a block, so it stays at the cell's floor.
    assert_eq!(pig.pos, DVec3::new(6.5, 146.0, 4.5));
    assert_eq!(used, Some(Used::SpawnedMob { at: pig.pos }));
    assert_eq!(count(&s), 2, "survival uses one");
}

#[test]
fn an_egg_on_a_top_face_stands_the_mob_on_the_block() {
    let (mut w, mut s, mut e) = scene(MobKind::Cow);
    w.set_block(WALL, Block::AIR);
    s.player.pitch = -0.9;
    use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(e.mobs.len(), 1);
    let cow = &e.mobs[0];
    assert_eq!(cow.pos.y, 145.0, "on the floor's top face");
    assert_eq!(cow.pos.x.fract(), 0.5);
}

#[test]
fn an_egg_on_a_plant_spawns_in_its_cell() {
    let (mut w, mut s, mut e) = scene(MobKind::Sheep);
    w.set_block(WALL, Block::AIR);
    let grass = Block::from_name("tall grass").unwrap();
    w.set_block(IVec3::new(6, 145, 4), grass);
    s.player.pitch = -0.6;
    use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(e.mobs.len(), 1);
    // `getCollisionShape(...).isEmpty()`: the plant's own cell.
    assert_eq!(e.mobs[0].pos, DVec3::new(6.5, 145.0, 4.5));
}

#[test]
fn creative_players_keep_their_eggs() {
    let (mut w, mut s, mut e) = scene(MobKind::Chicken);
    s.set_mode(GameMode::Creative);
    use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(e.mobs.len(), 1);
    assert_eq!(count(&s), 3);
}

#[test]
fn an_egg_into_water_spawns_in_the_source() {
    let (mut w, mut s, mut e) = scene(MobKind::Squid);
    w.set_block(WALL, Block::WATER);
    let used = use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(e.mobs.len(), 1);
    // `SpawnEggItem.use`: no moving down, at the source's own cell.
    assert_eq!(e.mobs[0].pos, DVec3::new(7.5, 146.0, 4.5));
    assert!(matches!(used, Some(Used::SpawnedMob { .. })));
}

#[test]
fn eggs_set_spawners_and_trial_spawners() {
    let (mut w, mut s, mut e) = scene(MobKind::Creeper);
    w.set_block(WALL, Block::SPAWNER);
    assert_eq!(use_egg(&mut s, &mut w, &mut e).unwrap(), Some(Used::SpawnerSet { cell: WALL }));
    assert_eq!(w.spawner(WALL), Some(MobKind::Creeper));
    assert!(e.mobs.is_empty(), "no mob of its own");
    assert_eq!(count(&s), 2);

    use voxelcraft::world::trial::{SpawnerState, spawner, spawner_block};
    let (mut w, mut s, mut e) = scene(MobKind::Blaze);
    w.set_block(WALL, spawner_block(SpawnerState::Active, true));
    assert!(w.trial_spawner(WALL).is_some());
    assert_eq!(use_egg(&mut s, &mut w, &mut e).unwrap(), Some(Used::SpawnerSet { cell: WALL }));
    // `overrideEntityToSpawn`: the blaze in both configurations, inactive, ominous kept.
    let t = w.trial_spawner(WALL).unwrap();
    assert_eq!(t.entity, Some(MobKind::Blaze));
    assert_eq!(t.potentials(false)[0].kind, MobKind::Blaze);
    assert_eq!(t.potentials(true).len(), 1);
    assert_eq!(w.get_block(WALL).and_then(spawner), Some((SpawnerState::Inactive, true)));
    // Saved and loaded with the world's trial data.
    let saved = w.trial_to_string();
    let mut fresh = World::new_remote(Arc::new(Generator::new(7)), 2);
    fresh.load_trial(&saved);
    assert_eq!(fresh.trial_spawner(WALL).and_then(|t| t.entity), Some(MobKind::Blaze));

    // `spawner_blocks_work` off: nothing changes.
    let (mut w, mut s, mut e) = scene(MobKind::Zombie);
    w.set_block(WALL, Block::SPAWNER);
    e.spawner_blocks = false;
    assert!(use_egg(&mut s, &mut w, &mut e).is_err());
    assert_eq!(count(&s), 3);
}

#[test]
fn eggs_on_their_own_mob_make_babies() {
    for (kind, baby) in [(MobKind::Cow, true), (MobKind::Zombie, true), (MobKind::Creeper, false)] {
        let (mut w, mut s, mut e) = scene(kind);
        e.spawn(kind, DVec3::new(6.0, 145.5, 4.5));
        e.mobs[0].baby = false;
        e.mobs[0].age = 0;
        let used = use_egg(&mut s, &mut w, &mut e).unwrap();
        if baby {
            assert_eq!(used, Some(Used::Mob), "{kind:?}");
            assert_eq!(e.mobs.len(), 2);
            let child = &e.mobs[1];
            assert!(child.baby && child.kind == kind);
            assert_eq!(child.pos, e.mobs[0].pos, "at the parent's feet");
            assert_eq!(child.yaw, 0.0);
            if kind == MobKind::Cow {
                assert_eq!(child.age, -24000, "AgeableMob.BABY_START_AGE");
            }
            assert_eq!(count(&s), 2);
        } else {
            // No offspring: the use passes, and the egg isn't used on the
            // block behind the creeper either.
            assert_eq!(used, None, "{kind:?}");
            assert_eq!(e.mobs.len(), 1);
            assert_eq!(count(&s), 3);
        }
    }
    // Another type's egg on a cow does nothing to it.
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    e.spawn(MobKind::Cow, DVec3::new(6.0, 145.5, 4.5));
    assert_eq!(use_egg(&mut s, &mut w, &mut e).unwrap(), None);
    assert_eq!(e.mobs.len(), 1);
}

#[test]
fn peaceful_refuses_monsters_but_not_animals() {
    let (mut w, mut s, mut e) = scene(MobKind::Creeper);
    e.difficulty = Difficulty::Peaceful;
    assert!(use_egg(&mut s, &mut w, &mut e).is_err());
    assert!(e.mobs.is_empty());
    assert_eq!(count(&s), 3);
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    e.difficulty = Difficulty::Peaceful;
    use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(e.mobs.len(), 1);
}

#[test]
fn eggs_keep_their_ids_through_save_and_reload() {
    let mut inv = Inventory::default();
    let first = Item(spawn_egg::ITEM_FIRST);
    let last = Item(spawn_egg::ITEM_END - 1);
    inv.slots[0] = Some(Stack::new(first, 64));
    inv.slots[20] = Some(Stack::new(last, 5));
    inv.cursor = None;
    let text = inv.serialize();
    let back = Inventory::deserialize(&text).unwrap();
    assert_eq!(back.slots[0], Some(Stack::new(first, 64)));
    assert_eq!(back.slots[20], Some(Stack::new(last, 5)));
    assert_eq!(voxelcraft::inventory::stack_to_string(Some(Stack::new(first, 2))), "1850:2");
    assert_eq!(spawn_egg::mob(last), Some(MobKind::Wither));
}

/// Java's `useItemOn` runs the block's own use before the item's `useOn`:
/// an egg on a ripe sweet berry bush picks the berries
/// (`SweetBerryBushBlock.useWithoutItem`) and spawns nothing, while a chest
/// still opens and a spawner still takes the egg (above).
#[test]
fn blocks_that_take_the_use_come_before_the_egg() {
    use voxelcraft::world::overworld_blocks::berry_bush;
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    w.set_block(WALL, Block::AIR);
    let bush = IVec3::new(6, 145, 4);
    w.set_block(bush, berry_bush(3));
    s.player.pitch = -0.6;
    let used = use_egg(&mut s, &mut w, &mut e).unwrap();
    assert_eq!(used, Some(Used::Harvested { cell: bush }));
    assert!(e.mobs.is_empty(), "no pig in the bush");
    assert_eq!(count(&s), 3);

    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    w.set_block(WALL, Block::CHEST);
    assert!(matches!(use_egg(&mut s, &mut w, &mut e).unwrap(), Some(Used::Screen { .. })));
    assert!(e.mobs.is_empty());

    // Blocks whose use passes let the egg act: `CakeBlock.eat` for a full
    // survival player, and `DoorBlock.useWithoutItem` for an iron door.
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    w.set_block(WALL, Block::AIR);
    let cake = IVec3::new(6, 145, 4);
    w.set_block(cake, Block::cake(0));
    s.vitals.hunger.food = 20.0;
    assert!(!s.vitals.hunger.can_eat());
    s.player.pitch = -0.6;
    let used = use_egg(&mut s, &mut w, &mut e).unwrap();
    assert!(matches!(used, Some(Used::SpawnedMob { .. })), "{used:?}");
    assert_eq!(w.get_block(cake), Some(Block::cake(0)), "the cake is untouched");
    assert_eq!(e.mobs.len(), 1);
    assert!(e.mobs[0].pos.y > 145.0, "on top of the cake");

    use voxelcraft::world::{block::Facing, redstone_blocks::iron_door};
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    w.set_block(WALL - IVec3::Y, iron_door(Facing::East, false, false));
    w.set_block(WALL, iron_door(Facing::East, false, true));
    let used = use_egg(&mut s, &mut w, &mut e).unwrap();
    assert!(matches!(used, Some(Used::SpawnedMob { .. })), "{used:?}");
    assert_eq!(w.get_block(WALL), Some(iron_door(Facing::East, false, true)), "still shut");
    assert_eq!(e.mobs.len(), 1);
}

/// v1.0 Track 7: the Ender Dragon's egg makes a dragon no fight owns. Java
/// 26.3 starts it `HOVERING` where it appeared (`EnderDragonPhaseManager`),
/// with no boss bar (`EnderDragonFight` owns that), and keeps it through a
/// save (`EnderDragon.addAdditionalSaveData`).
#[test]
fn the_ender_dragon_egg_spawns_a_hovering_dragon_that_saves() {
    use voxelcraft::entity::dragon::Phase;
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    s.inventory.slots[0] = Some(Stack::new(spawn_egg::ENDER_DRAGON, 3));
    let used = use_egg(&mut s, &mut w, &mut e).unwrap();
    let Some(Used::SpawnedMob { at }) = used else { panic!("{used:?}") };
    assert!(e.mobs.is_empty(), "the dragon isn't a mob");
    assert_eq!(e.wild_dragons.dragons.len(), 1);
    let d = &e.wild_dragons.dragons[0].dragon;
    assert_eq!(d.pos, at);
    // In front of the clicked face (+x of the wall's west face).
    assert_eq!((at.x, at.z), (6.5, 4.5));
    assert_eq!(d.phase, Phase::Hovering);
    assert_eq!(d.health, voxelcraft::entity::dragon::MAX_HEALTH);
    assert!(e.fight.is_none(), "no fight, so no boss bar");
    // Creative keeps the egg; survival would use one.
    assert_eq!(count(&s), 3 - u8::from(!s.mode.is_creative()));

    let mut props = std::collections::BTreeMap::new();
    e.save_props(&mut props);
    let mut back = Entities::new(7);
    back.load_props(&props);
    assert_eq!(back.wild_dragons.dragons.len(), 1);
    let b = &back.wild_dragons.dragons[0].dragon;
    assert_eq!((b.pos, b.yaw, b.health, b.phase), (d.pos, d.yaw, d.health, d.phase));

    // Peaceful still allows it (`isAllowedInPeaceful` is true for the dragon).
    let (mut w, mut s, mut e) = scene(MobKind::Pig);
    e.difficulty = Difficulty::Peaceful;
    s.inventory.slots[0] = Some(Stack::new(spawn_egg::ENDER_DRAGON, 1));
    assert!(matches!(use_egg(&mut s, &mut w, &mut e).unwrap(), Some(Used::SpawnedMob { .. })));
    assert_eq!(e.wild_dragons.dragons.len(), 1);
}
