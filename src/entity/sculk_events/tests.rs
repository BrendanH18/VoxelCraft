//! v1.0 Track 9: the mob behaviours Java posts as game events
//! (`docs/verification/2026-11-25-v1.0-track-09-controls-sculk.md`).

use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::super::{Entities, MobKind};
use crate::entity::equipment::Slot;
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::World;
use crate::world::block::Block;
use crate::world::chunk::ChunkData;
use crate::world::terrain::Generator;
use crate::world::vibration::{EntityRef, GameEvent};

const FLOOR: i32 = 140;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, FLOOR - 1, z), Block::STONE);
        }
    }
    w.set_entity_listeners(true);
    w.take_entity_game_events();
    w
}

/// One mob of `kind` on the floor, seen once so later changes post.
fn one(kind: MobKind) -> (Entities, World) {
    let mut e = Entities::new(5);
    let mut w = world();
    e.spawn(kind, DVec3::new(8.5, FLOOR as f64, 8.5));
    e.ensure_mob_ids();
    e.mobs[0].on_ground = true;
    e.mobs[0].simulation_active = true;
    e.mob_game_events(&mut w);
    w.take_entity_game_events();
    (e, w)
}

/// The events the next comparison posts, and that each came from the mob.
fn next(e: &mut Entities, w: &mut World) -> Vec<GameEvent> {
    e.mob_game_events(w);
    let uid = e.mobs[0].uid;
    let posted = w.take_entity_game_events();
    for p in &posted {
        assert_eq!(p.source.entity, Some(EntityRef::Mob(uid)), "{:?} names the mob", p.event);
    }
    posted.iter().map(|p| p.event).collect()
}

#[test]
fn armadillos_post_rolling_up_and_out_but_not_peeking() {
    use crate::entity::armadillo::{IDLE, ROLLING, SCARED, UNROLLING};
    let (mut e, mut w) = one(MobKind::Armadillo);
    let pose = |e: &mut Entities, p| e.mobs[0].armadillo.as_mut().unwrap().pose = p;
    pose(&mut e, ROLLING);
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "rollUp");
    pose(&mut e, SCARED);
    assert!(next(&mut e, &mut w).is_empty(), "rolled up: no event");
    pose(&mut e, UNROLLING);
    assert!(next(&mut e, &mut w).is_empty());
    pose(&mut e, IDLE);
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "rollOut");
}

#[test]
fn camels_post_sitting_standing_and_dashing() {
    let (mut e, mut w) = one(MobKind::Camel);
    e.mobs[0].mount.as_mut().unwrap().sit_ticks = 600;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "sitDown");
    e.mobs[0].mount.as_mut().unwrap().sit_ticks = 0;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "standUp");
    e.mobs[0].mount.as_mut().unwrap().dash_cooldown = crate::entity::mounts::DASH_COOLDOWN;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "handleStartJump");
    e.mobs[0].mount.as_mut().unwrap().dash_cooldown -= 1;
    assert!(next(&mut e, &mut w).is_empty(), "the cooldown running down posts nothing");
}

#[test]
fn nautiluses_dash_sulfur_cubes_prime_and_creakings_change_state() {
    let (mut e, mut w) = one(MobKind::Nautilus);
    e.mobs[0].nautilus.as_mut().unwrap().dashing = true;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction]);
    assert!(next(&mut e, &mut w).is_empty());

    let (mut e, mut w) = one(MobKind::SulfurCube);
    e.mobs[0].sulfur_cube.as_mut().unwrap().fuse = 80;
    assert_eq!(next(&mut e, &mut w), [GameEvent::PrimeFuse]);

    let (mut e, mut w) = one(MobKind::Creaking);
    e.mobs[0].creaking.as_mut().unwrap().active = true;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "activate");
    e.mobs[0].creaking.as_mut().unwrap().can_move = false;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "can no longer move");
    e.mobs[0].creaking.as_mut().unwrap().hit_cooldown = 0.4;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "a protected creaking twitches");
    e.mobs[0].creaking.as_mut().unwrap().hit_cooldown = 0.3;
    assert!(next(&mut e, &mut w).is_empty());
    e.mobs[0].creaking.as_mut().unwrap().active = false;
    assert_eq!(next(&mut e, &mut w), [GameEvent::EntityAction], "deactivate");
}

#[test]
fn mobs_post_equipment_changes_mounting_and_bounces() {
    let (mut e, mut w) = one(MobKind::Zombie);
    let pumpkin = Item::from_block(Block::CARVED_PUMPKIN);
    e.mobs[0].equipment.set(Slot::Head, Some(Stack::new(pumpkin, 1)));
    assert_eq!(next(&mut e, &mut w), [GameEvent::Equip], "an equippable item");
    e.mobs[0].equipment.set(Slot::MainHand, Some(Stack::new(Item::STICK, 1)));
    assert_eq!(next(&mut e, &mut w), [GameEvent::Unequip], "a stick has no EQUIPPABLE");
    e.mobs[0].equipment.set(Slot::Head, None);
    assert_eq!(next(&mut e, &mut w), [GameEvent::Unequip]);
    // `startRiding`: at the vehicle.
    let cart = e.spawn_cart(crate::entity::minecart::CartKind::Rideable, DVec3::new(12.5, FLOOR as f64, 8.5));
    e.mobs[0].riding = Some(cart);
    e.mob_game_events(&mut w);
    let posted = w.take_entity_game_events();
    assert_eq!(posted.iter().map(|p| p.event).collect::<Vec<_>>(), [GameEvent::EntityMount]);
    assert_eq!(posted[0].pos, DVec3::new(12.5, FLOOR as f64, 8.5));
    e.mobs[0].riding = None;
    e.mob_game_events(&mut w);
    w.take_entity_game_events();
    e.mobs[0].bounces += 1;
    assert_eq!(next(&mut e, &mut w), [GameEvent::Bounce]);
}

/// `processFlappingMovement` over air: a bat awake flaps every 10 ticks
/// (asleep, never); a chicken's flap follows the distance it flies.
#[test]
fn flyers_flap_over_air_at_their_own_rhythm() {
    let air = DVec3::new(8.5, FLOOR as f64 + 6.0, 8.5);
    let (mut e, mut w) = one(MobKind::Bat);
    e.mobs[0].pos = air;
    e.mobs[0].on_ground = false;
    e.mobs[0].bat.as_mut().unwrap().resting = false;
    let mut flaps = 0;
    for _ in 0..40 {
        e.mobs[0].previous_pos = e.mobs[0].pos;
        flaps += next(&mut e, &mut w).iter().filter(|&&k| k == GameEvent::Flap).count();
    }
    assert_eq!(flaps, 4, "every tenth tick");
    e.mobs[0].bat.as_mut().unwrap().resting = true;
    for _ in 0..20 {
        assert!(!next(&mut e, &mut w).contains(&GameEvent::Flap), "a resting bat");
    }

    let (mut e, mut w) = one(MobKind::Chicken);
    e.mobs[0].on_ground = false;
    let mut flaps = 0;
    for i in 0..20 {
        e.mobs[0].previous_pos = air + DVec3::X * (i as f64 * 0.2);
        e.mobs[0].pos = air + DVec3::X * ((i + 1) as f64 * 0.2);
        flaps += next(&mut e, &mut w).iter().filter(|&&k| k == GameEvent::Flap).count();
    }
    // 0.12 a tick flown (0.2 * 0.6), a flap each half block once flapSpeed is 1.
    assert!((4..=6).contains(&flaps), "{flaps}");
    // On the ground, nothing flaps.
    let (mut e, mut w) = one(MobKind::Bee);
    for _ in 0..10 {
        assert!(!next(&mut e, &mut w).contains(&GameEvent::Flap));
    }
}
