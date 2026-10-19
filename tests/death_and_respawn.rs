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
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        terrain::Generator,
    },
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
    rules.set("keep_inventory", if keep_inventory { "true" } else { "false" }).unwrap();
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
        // `ServerPlayer.die` leaves effects alone; the respawned body has none.
        assert!(a.vitals.effects.has(Effect::Speed), "effects last until the respawn ({mode:?})");
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
fn absorption_hearts_die_with_the_player_and_do_not_survive_the_respawn() {
    let mut world = world();
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.vitals.apply_effect(Effect::Absorption, 3, 2400);
    assert_eq!(a.vitals.effects.absorption(), 16.0);
    // 100 damage: Java absorbs 16 and the other 84 kill.
    a.hurt(100.0, "was slain by a zombie", DVec3::ZERO, &mut entities);
    assert!(a.vitals.is_dead());
    assert_eq!(a.vitals.effects.absorption(), 0.0);
    a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
    assert!(!a.vitals.is_dead());
    assert_eq!((a.vitals.effects.absorption(), a.vitals.effects.max_absorption()), (0.0, 0.0));
    assert!(!a.vitals.effects.has(Effect::Absorption));
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

/// The bed's foot; its head is one block along the chosen direction.
const FOOT: IVec3 = IVec3::new(10, 150, 10);

/// A stone floor at y = 149 and clear air above it round the bed.
fn platform(world: &mut World) {
    for z in 4..=16 {
        for x in 4..=16 {
            world.set_block(IVec3::new(x, 149, z), Block::STONE);
            for y in 150..=155 {
                world.set_block(IVec3::new(x, y, z), Block::AIR);
            }
        }
    }
}

fn bed(world: &mut World, dir: IVec3) -> IVec3 {
    assert!(world.place_colored_bed(FOOT, dir, voxelcraft::color::DyeColor::Red));
    FOOT + dir
}

/// Fills the column at (x, z) with stone from y = 150 up, so nobody fits.
fn wall(world: &mut World, x: i32, z: i32) {
    world.set_block(IVec3::new(x, 150, z), Block::STONE);
    world.set_block(IVec3::new(x, 151, z), Block::STONE);
}

fn spawn(world: &World, foot: IVec3) -> voxelcraft::session::PersonalSpawn {
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::new(1.5, 150.0, 1.5));
    s.spawn_bed = Some(foot);
    let spawn = s.personal_spawn(world);
    assert_eq!(s.spawn_bed.is_none(), spawn.missing_bed, "a bed that cannot be used is forgotten");
    spawn
}

fn at(x: f64, y: f64, z: f64) -> Option<DVec3> {
    Some(DVec3::new(x, y, z))
}

/// `ServerPlayer.findRespawnAndUseSpawnBlock`: a bed respawn stands where
/// `AbstractBedBlock.findStandUpPosition` finds room beside the bed's head
/// (the player's stored angle, 0 for VoxelCraft saves, picks the side) and
/// looks from there at the head (`RespawnPosAngle.of`); a set spawn point or
/// the world spawn uses its stored angle, which defaults to 0 (south, +Z).
/// The pitch is level either way, whatever the player looked at when it died.
#[test]
fn respawns_stand_beside_the_bed_head_and_look_at_it_with_level_pitch() {
    let mut world = world();
    let mut entities = Entities::new(1);
    use std::f32::consts::{FRAC_PI_2, PI};
    for (dir, stand, yaw) in [
        // East's clockwise side faces south, which angle 0 steers away from.
        (IVec3::X, (11.5, 9.5), FRAC_PI_2),
        (IVec3::Z, (9.5, 11.5), 0.0),
        (IVec3::NEG_X, (9.5, 9.5), FRAC_PI_2),
        (IVec3::NEG_Z, (11.5, 9.5), PI),
    ] {
        platform(&mut world);
        let head = bed(&mut world, dir);
        let mut a = loaded(GameMode::Survival);
        a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
        a.spawn_bed = Some(FOOT);
        a.player.yaw = 1.0;
        a.player.pitch = -1.2;
        a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
        a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.spawn_bed, Some(FOOT), "the bed is still there");
        assert_eq!(a.player.pos, DVec3::new(stand.0, 150.0, stand.1), "{dir}: beside the head, on the floor");
        let to_head = (head.as_dvec3() + DVec3::new(0.5, 0.0, 0.5) - a.player.pos).normalize();
        assert!((a.player.forward().as_dvec3() - to_head).length() < 1e-5, "{dir}: looks at the head");
        assert!((a.player.yaw - yaw).abs() < 1e-5, "{dir}: yaw {}", a.player.yaw);
        assert_eq!(a.player.pitch, 0.0);
    }

    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.spawn_point = Some(IVec3::new(4, 150, 4));
    a.player.yaw = 2.5;
    a.player.pitch = 0.7;
    a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
    a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(a.player.yaw, voxelcraft::session::DEFAULT_SPAWN_YAW);
    assert_eq!(a.respawns, 1, "LAN tells clients about each respawn by this count");
    assert!((a.player.forward() - glam::Vec3::Z).length() < 1e-6, "Java yaw 0 faces south (+Z)");
    assert_eq!(a.player.pitch, 0.0);
}

/// `findStandUpPositionAtOffset` tries `bedStandUpOffsets` in order: the side
/// the angle chose, round the foot end, the far side, then the head end, and
/// finally on the bed itself; a spot needs a floor and 1.8 blocks of room.
#[test]
fn obstructed_sides_choose_the_next_offset_in_java_order() {
    let mut world = world();
    platform(&mut world);
    bed(&mut world, IVec3::X);
    // Head (11, 150, 10), facing east, side north; offsets (dx, dz) from it.
    let expected = [
        (11, 9),  // 0: beside the head
        (10, 9),  // 1: beside the foot
        (9, 9),   // 2: past the foot, diagonally
        (9, 10),  // 3: behind the foot
        (9, 11),  // 4
        (10, 11), // 5: beside the foot, other side
        (11, 11), // 6: beside the head, other side
        (12, 11), // 7: past the head, diagonally
        (12, 10), // 8: in front of the head
        (12, 9),  // 9
    ];
    for (i, &(x, z)) in expected.iter().enumerate() {
        let found = spawn(&world, FOOT);
        assert_eq!(found.position, at(x as f64 + 0.5, 150.0, z as f64 + 0.5), "offset {i}");
        assert!(!found.missing_bed);
        // The yaw looks at the head from wherever the player stands.
        let look = DVec3::new(11.5 - (x as f64 + 0.5), 0.0, 10.5 - (z as f64 + 0.5));
        assert!((found.yaw - look.z.atan2(look.x) as f32).abs() < 1e-6, "offset {i}: yaw {}", found.yaw);
        wall(&mut world, x, z);
    }
    // Everything round the bed is stone: only the bed itself is left, head
    // first, and that needs the room above it.
    let found = spawn(&world, FOOT);
    assert_eq!(found.position, at(11.5, 150.0 + Block::BED_HEAD.height(), 10.5));
    assert_eq!(found.yaw, 0.0, "directly above the head there is nothing to turn towards");
    // Standing on the head needs 1.8 blocks above 150.5625, so y = 152 is in the way.
    world.set_block(IVec3::new(11, 152, 10), Block::STONE);
    assert_eq!(spawn(&world, FOOT).position, at(10.5, 150.0 + Block::BED_FOOT.height(), 10.5), "the foot is next");
    world.set_block(IVec3::new(10, 152, 10), Block::STONE);
    assert!(spawn(&world, FOOT).missing_bed);
}

#[test]
fn a_dangerous_floor_is_only_used_when_nothing_safer_fits() {
    let mut world = world();
    platform(&mut world);
    bed(&mut world, IVec3::X);
    world.set_block(IVec3::new(11, 149, 9), Block::MAGMA);
    assert_eq!(spawn(&world, FOOT).position, at(10.5, 150.0, 9.5), "magma beside the head is skipped");
    // Leave only the magma spot and no room on the bed: it is taken in the
    // second pass.
    for z in 4..=16 {
        for x in 4..=16 {
            if (x, z) != (11, 9) && !matches!((x, z), (10, 10) | (11, 10)) {
                wall(&mut world, x, z);
            }
        }
    }
    world.set_block(IVec3::new(10, 151, 10), Block::STONE);
    world.set_block(IVec3::new(11, 151, 10), Block::STONE);
    assert_eq!(spawn(&world, FOOT).position, at(11.5, 150.0, 9.5));
}

/// A bed with no room round it or above it is the missing-bed outcome
/// (`TeleportTransition.missingRespawnBlock`): the player spawns at the world
/// spawn, the bed is forgotten and "Your home bed was missing" shows.
#[test]
fn a_boxed_in_bed_is_a_missing_bed() {
    let mut world = world();
    platform(&mut world);
    bed(&mut world, IVec3::X);
    for z in 8..=12 {
        for x in 8..=13 {
            if z != 10 || !(10..=11).contains(&x) {
                wall(&mut world, x, z);
            }
        }
    }
    world.set_block(IVec3::new(10, 151, 10), Block::STONE);
    world.set_block(IVec3::new(11, 151, 10), Block::STONE);
    let found = spawn(&world, FOOT);
    assert!(found.missing_bed && found.position.is_none());
    assert_eq!(found.yaw, voxelcraft::session::DEFAULT_SPAWN_YAW);

    // Through the CLI respawn: the world spawn, bed forgotten, same facing.
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.spawn_bed = Some(FOOT);
    a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
    a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
    assert_eq!(a.spawn_bed, None);
    assert!((a.player.pos - DVec3::new(11.5, 150.0, 9.5)).length() > 1.0, "not at the bed");
}

#[test]
fn a_bunk_bed_respawns_beside_either_bed_before_standing_on_the_top() {
    let mut world = world();
    platform(&mut world);
    bed(&mut world, IVec3::X);
    let upper = FOOT + IVec3::Y;
    // Placement wants solid ground; a bunk bed rests on the bed below.
    let red = voxelcraft::color::DyeColor::Red;
    world.set_block(upper, Block::colored_bed(red, false));
    world.set_block(upper + IVec3::X, Block::colored_bed(red, true));
    // Nothing beside the top bed has a floor; the lower bed's north side does.
    let found = spawn(&world, upper);
    assert_eq!(found.position, at(11.5, 150.0, 9.5));
    assert!((found.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6, "looks at the head");
    // Block the floor-level sides: the top of the upper head is next.
    for z in 4..=16 {
        for x in 4..=16 {
            if !matches!((x, z), (10, 10) | (11, 10)) {
                wall(&mut world, x, z);
            }
        }
    }
    let found = spawn(&world, upper);
    assert_eq!(found.position, at(11.5, 151.0 + Block::BED_HEAD.height(), 10.5));
}

/// Java reads the selected bed's own `FACING`; a neighbouring bed that
/// happens to touch the head must not change which way the stored bed faces.
#[test]
fn an_adjacent_bed_does_not_change_the_stored_beds_facing() {
    let mut world = world();
    platform(&mut world);
    bed(&mut world, IVec3::X);
    let red = voxelcraft::color::DyeColor::Red;
    // A second east-facing bed whose foot is south of the first one's head.
    world.set_block(IVec3::new(11, 150, 11), Block::colored_bed(red, false));
    world.set_block(IVec3::new(12, 150, 11), Block::colored_bed(red, true));
    assert_eq!(spawn(&world, FOOT).position, at(11.5, 150.0, 9.5), "north of the first bed's head");
}

/// `LivingEntity.stopSleeping`: waking puts the player at the bed's stand-up
/// position, looking at the head, level; their own yaw picks the side.
#[test]
fn waking_stands_the_player_up_beside_the_bed() {
    let mut world = world();
    let mut entities = Entities::new(1);
    platform(&mut world);
    let head = bed(&mut world, IVec3::X);
    let lie = DVec3::new(10.5, 150.0 + Block::BED_FOOT.height(), 10.5);
    let mut s = PlayerSession::new(PlayerId::HOST, lie);
    // VoxelCraft yaw pi/2 is Java yaw 0: facing south, so the north side.
    for (yaw, stand) in [(std::f32::consts::FRAC_PI_2, (11.5, 9.5)), (3.0 * std::f32::consts::FRAC_PI_2, (11.5, 11.5))]
    {
        s.player.yaw = yaw;
        s.player.pitch = 0.9;
        s.begin_sleep(Some((head, Facing::East)), lie);
        assert_eq!(s.sleeping, Some(0.0));
        s.wake(&world);
        assert_eq!(s.sleeping, None);
        assert_eq!(s.player.pos, DVec3::new(stand.0, 150.0, stand.1), "yaw {yaw}");
        let to_head = (head.as_dvec3() + DVec3::new(0.5, 0.0, 0.5) - s.player.pos).normalize();
        assert!((s.player.forward().as_dvec3() - to_head).length() < 1e-5, "looks at the head");
        assert_eq!(s.player.pitch, 0.0);
    }

    // Any other wake-up (being hurt, a command) settles on the next tick, and
    // only a player still lying where they lay down moves.
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.player.yaw = std::f32::consts::FRAC_PI_2;
    a.begin_sleep(Some((head, Facing::East)), lie);
    a.hurt(1.0, "test", DVec3::ZERO, &mut entities);
    assert_eq!(a.sleeping, None);
    assert_eq!(a.player.pos, lie, "still on the bed until the next tick");
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    assert!((a.player.pos - DVec3::new(11.5, 150.0, 9.5)).length() < 1e-3, "{:?}", a.player.pos);
    a.begin_sleep(Some((head, Facing::East)), lie);
    a.sleeping = None;
    a.player.pos = DVec3::new(14.5, 150.0, 14.5);
    a.settle_wake(&world);
    assert_eq!(a.player.pos, DVec3::new(14.5, 150.0, 14.5), "teleported away: left alone");

    // Physics settles a sleeper a hair above the bed; that still counts as
    // lying there.
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.player.yaw = std::f32::consts::FRAC_PI_2;
    a.player.pitch = 0.4;
    a.begin_sleep(Some((head, Facing::East)), lie);
    for _ in 0..5 {
        a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    }
    assert_ne!(a.player.pos, lie, "the collision epsilon moved the sleeper");
    a.wake(&world);
    assert!((a.player.pos - DVec3::new(11.5, 150.0, 9.5)).length() < 1e-3, "{:?}", a.player.pos);
    assert_eq!(a.player.pitch, 0.0);

    // A bed that is gone leaves the sleeper where they are.
    s.begin_sleep(Some((head, Facing::East)), lie);
    world.set_block(FOOT, Block::AIR);
    world.set_block(head, Block::AIR);
    s.wake(&world);
    assert_eq!(s.player.pos, lie);

    // A boxed-in bed puts them just above the head (`stopSleeping`'s fallback).
    platform(&mut world);
    let head = bed(&mut world, IVec3::X);
    for z in 8..=12 {
        for x in 8..=13 {
            if z != 10 || !(10..=11).contains(&x) {
                wall(&mut world, x, z);
            }
        }
    }
    world.set_block(IVec3::new(10, 151, 10), Block::STONE);
    world.set_block(IVec3::new(11, 151, 10), Block::STONE);
    s.begin_sleep(Some((head, Facing::East)), lie);
    s.wake(&world);
    assert_eq!(s.player.pos, DVec3::new(11.5, 151.1, 10.5));
}

#[test]
fn the_death_screen_score_resets_on_respawn_unless_experience_is_kept() {
    let mut world = world();
    for keep in [false, true] {
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.tick_rules(&mut world, &mut entities, Default::default(), &rules(keep));
        a.vitals.xp.add_points(100);
        a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.vitals.xp.total, 100, "the death screen shows the score");
        a.execute(Command::Respawn, &mut world, &mut entities, &[]).unwrap();
        // `restoreFrom` copies the score only with the inventory.
        assert_eq!(a.vitals.xp.total, if keep { 100 } else { 0 }, "keepInventory {keep}");
    }
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

/// `LivingEntity.baseTick` keeps ticking effects on the death screen, so a
/// short effect runs out there instead of lasting until the respawn, and a
/// dead body takes no harm or healing from them.
#[test]
fn effects_keep_counting_down_on_the_death_screen_without_acting() {
    let mut world = world();
    let mut entities = Entities::new(1);
    let mut a = loaded(GameMode::Survival);
    a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    a.vitals.effects.add(Effect::Blindness, 0, 10);
    a.vitals.effects.add(Effect::Regeneration, 4, 200);
    a.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
    let health = a.vitals.health;
    for _ in 0..20 {
        a.tick_rules(&mut world, &mut entities, Default::default(), &rules(false));
    }
    assert!(a.vitals.is_dead() && a.vitals.health == health, "a dead body doesn't regenerate");
    assert!(!a.vitals.effects.has(Effect::Blindness), "ten ticks ran out on the death screen");
    assert!(a.vitals.effects.get(Effect::Regeneration).is_some_and(|e| e.ticks <= 180));

    let mut v = Vitals::default();
    v.effects.add(Effect::Blindness, 0, 10);
    v.damage(100.0, "test", false);
    for _ in 0..20 {
        v.tick(0.05, &Default::default(), false);
    }
    assert!(!v.effects.has(Effect::Blindness), "the desktop's vitals tick does the same");
}
