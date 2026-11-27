//! v1.0 gap Track 3 (FOO-04c, MOB-03c) through the real entity tick and the
//! shared session: camels' own ride, horses ridden in water, the nautilus
//! inventory screen, baby dolphins and guardians beaming squid and
//! axolotls (currents are unit-tested in `entity::mob::swim`). Sources and differences are in
//! `docs/verification/2026-11-27-v1.0-gap-track-03-mounts-water.md`.
use std::collections::HashMap;
use std::sync::Arc;

use glam::{DVec3, IVec3};
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, Target, mounts},
    inventory::{Inventory, Stack},
    item::Item,
    physics::BlockSource,
    player::{MoveInput, Player},
    session::{
        PlayerSession,
        interact::{UseContext, Used},
        storage::{Storage, StorageScreen, StorageSlot},
    },
    simulation::lighting::SpawnLight,
    world::{World, block::Block, chunk::ChunkData, terrain::Dimension, terrain::Generator},
};

/// Stone below y = 1; water up to y = 8 when `water`; explicit overrides.
#[derive(Default)]
struct Scene {
    blocks: HashMap<IVec3, Block>,
    water: bool,
}
impl BlockSource for Scene {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(self.blocks.get(&p).copied().unwrap_or(if p.y < 1 {
            Block::STONE
        } else if self.water && p.y < 9 {
            Block::WATER
        } else {
            Block::AIR
        }))
    }
}
impl MobWorld for Scene {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
}

/// No player anyone can target: a creative one far away.
fn quiet() -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        players: vec![Target::new(PlayerId::HOST, DVec3::new(500.0, 1.0, 500.0), false)],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

/// A saddled mount of `kind` at `pos` with the host riding it, and the host
/// as a player looking along +x (yaw 0).
fn ridden(e: &mut Entities, kind: MobKind, pos: DVec3) -> (usize, Player) {
    e.spawn(kind, pos);
    e.ensure_mob_ids();
    let i = e.mobs.len() - 1;
    let m = &mut e.mobs[i];
    m.baby = false;
    m.age = 0;
    m.on_ground = true;
    m.yaw = 0.0;
    let s = m.mount.as_mut().unwrap();
    s.tame = true;
    s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
    s.rider = Some(PlayerId::HOST);
    let mut p = Player::new(pos);
    p.yaw = 0.0;
    p.vehicle = Some(mounts::MOB_VEHICLE | m.uid);
    (i, p)
}

fn ride(e: &mut Entities, p: &Player, input: MoveInput, scene: &Scene) {
    e.mount_input(PlayerId::HOST, p, input, &Inventory::default(), 0);
    e.update(0.05, scene, &quiet());
}

fn forward() -> MoveInput {
    MoveInput { forward: 1.0, ..MoveInput::default() }
}

#[test]
fn a_seated_camel_stands_for_52_ticks_on_forward_input_then_walks() {
    let scene = Scene::default();
    let mut e = Entities::new(5);
    let (i, p) = ridden(&mut e, MobKind::Camel, DVec3::new(0.5, 1.0, 0.5));
    let s = e.mobs[i].mount.as_mut().unwrap();
    s.sit_down();
    s.pose_time = 400;
    // Standing still, a seated rider stays seated.
    for _ in 0..5 {
        ride(&mut e, &p, MoveInput::default(), &scene);
    }
    assert!(e.mobs[i].mount.as_ref().unwrap().camel_sitting);
    // `Camel.tickRidden`: forward input stands it up; it won't move until
    // the 52-tick transition is over (`refuseToMove`).
    let x0 = e.mobs[i].pos.x;
    ride(&mut e, &p, forward(), &scene);
    let s = e.mobs[i].mount.as_ref().unwrap();
    assert!(!s.camel_sitting && s.refuses_to_move(), "standing up");
    for _ in 0..50 {
        ride(&mut e, &p, forward(), &scene);
    }
    assert!((e.mobs[i].pos.x - x0).abs() < 1e-9, "still standing up: {}", e.mobs[i].pos.x - x0);
    for _ in 0..60 {
        ride(&mut e, &p, forward(), &scene);
    }
    assert!(!e.mobs[i].mount.as_ref().unwrap().refuses_to_move());
    // Ground speed 0.09 on stone: v = speed × 0.546 / (1 − 0.546) a tick.
    let top = f64::from(0.09_f32) * 0.546 / (1.0 - 0.546);
    assert!((e.mobs[i].vel.x / 20.0 - top).abs() < 1e-3, "{} vs {top}", e.mobs[i].vel.x / 20.0);
    // Sprinting with the dash ready adds 0.1 to the speed.
    for _ in 0..60 {
        ride(&mut e, &p, MoveInput { sprint: true, ..forward() }, &scene);
    }
    let sprint = f64::from(0.19_f32) * 0.546 / (1.0 - 0.546);
    assert!((e.mobs[i].vel.x / 20.0 - sprint).abs() < 1e-3, "{} vs {sprint}", e.mobs[i].vel.x / 20.0);
}

#[test]
fn a_camel_dashes_on_a_released_jump_then_waits_55_ticks() {
    let scene = Scene::default();
    let mut e = Entities::new(6);
    let (i, p) = ridden(&mut e, MobKind::Camel, DVec3::new(0.5, 1.0, 0.5));
    // Settle onto the floor.
    for _ in 0..3 {
        ride(&mut e, &p, MoveInput::default(), &scene);
    }
    let jump = MoveInput { jump: true, ..MoveInput::default() };
    for _ in 0..10 {
        ride(&mut e, &p, jump, &scene);
    }
    assert_eq!(e.mobs[i].mount.as_ref().unwrap().jump_charge, 9.0 * 0.1);
    let x0 = e.mobs[i].pos.x;
    // Released: `onPlayerJump`, then the next ridden tick dashes:
    // 22.2222 × 1 × 0.09 along the look and 1.4285 × 0.42 up.
    ride(&mut e, &p, MoveInput::default(), &scene);
    let m = &e.mobs[i];
    let s = m.mount.as_ref().unwrap();
    assert!(s.dashing && s.dash_cooldown == mounts::DASH_COOLDOWN - 1, "{}", s.dash_cooldown);
    assert!(m.pos.x - x0 > 1.9, "dash moved {}", m.pos.x - x0);
    // `addDeltaMovement`: on top of a grounded mob's −0.08 × 0.98.
    let up = f64::from(1.4285_f32 * 0.42_f32) - 0.08 * 0.98;
    assert!((m.pos.y - 1.0 - up).abs() < 1e-3, "rose {} vs {up}", m.pos.y - 1.0);
    // While the dash cools down the rider's jump doesn't charge.
    for _ in 0..10 {
        ride(&mut e, &p, jump, &scene);
        assert_eq!(e.mobs[i].mount.as_ref().unwrap().jump_charge, 0.0);
    }
    for _ in 0..60 {
        ride(&mut e, &p, MoveInput::default(), &scene);
    }
    let s = e.mobs[i].mount.as_ref().unwrap();
    assert!(s.dash_cooldown == 0 && !s.dashing);
    // A seated camel can't charge a dash (`canJump`).
    e.mobs[i].mount.as_mut().unwrap().sit_down();
    for _ in 0..5 {
        ride(&mut e, &p, jump, &scene);
    }
    assert_eq!(e.mobs[i].mount.as_ref().unwrap().jump_charge, 0.0);
}

#[test]
fn idle_camels_sit_after_twenty_seconds_and_stand_again_after_twenty_more() {
    let scene = Scene::default();
    let mut e = Entities::new(7);
    e.spawn(MobKind::Camel, DVec3::new(0.5, 1.0, 0.5));
    let (mut sat, mut stood) = (None, None);
    for t in 0..6000 {
        e.update(0.05, &scene, &quiet());
        let s = e.mobs[0].mount.as_ref().unwrap();
        if sat.is_none() && s.camel_sitting {
            sat = Some(t);
        }
        if sat.is_some() && stood.is_none() && !s.camel_sitting {
            stood = Some(t);
        }
    }
    let (sat, stood) = (sat.expect("never sat"), stood.expect("never stood"));
    // A new camel stands fully (pose time 53): `RandomSitting` waits for 400.
    assert!(sat >= 400 - 53, "{sat}");
    assert!(stood - sat >= 400, "sat {sat}, stood {stood}");
}

#[test]
fn ridden_horses_swim_by_travel_in_water_and_skeleton_horses_sink() {
    // Under a ceiling, fully under water: `travelInWater`'s 0.02 a tick
    // against 0.8 drag, 0.08 a tick.
    let mut scene = Scene { water: true, ..Scene::default() };
    for x in -4..=40 {
        for z in -3..=3 {
            scene.blocks.insert(IVec3::new(x, 3, z), Block::STONE);
        }
    }
    let mut e = Entities::new(8);
    let (i, p) = ridden(&mut e, MobKind::Horse, DVec3::new(0.5, 1.0, 0.5));
    for _ in 0..100 {
        ride(&mut e, &p, forward(), &scene);
    }
    let m = &e.mobs[i];
    assert!((m.vel.x / 20.0 - 0.08).abs() < 1e-3, "{}", m.vel.x / 20.0);
    assert!(m.pos.y < 2.0 && m.in_water);
    // In open water `#can_float_while_ridden` lifts it to the surface.
    let scene = Scene { water: true, ..Scene::default() };
    let mut e = Entities::new(8);
    let (i, p) = ridden(&mut e, MobKind::Horse, DVec3::new(0.5, 2.0, 0.5));
    for _ in 0..200 {
        ride(&mut e, &p, forward(), &scene);
    }
    assert!(e.mobs[i].pos.y > 7.0, "floats up: {}", e.mobs[i].pos.y);
    // A skeleton horse has no float goal and isn't tagged: it walks the
    // bottom, with its 0.96 water slow-down.
    let mut e = Entities::new(9);
    let (i, p) = ridden(&mut e, MobKind::SkeletonHorse, DVec3::new(0.5, 1.0, 0.5));
    for _ in 0..120 {
        ride(&mut e, &p, forward(), &scene);
    }
    let m = &e.mobs[i];
    let top = 0.02 * 0.96 / (1.0 - 0.96);
    assert!((m.vel.x / 20.0 - top).abs() < 0.02, "{} vs {top}", m.vel.x / 20.0);
    assert!(m.pos.y < 2.0, "{}", m.pos.y);
}

#[test]
fn guardians_beam_squid_and_axolotls() {
    let scene = Scene { water: true, ..Scene::default() };
    for prey in [MobKind::Squid, MobKind::Axolotl] {
        let mut e = Entities::new(11);
        e.spawn(MobKind::Guardian, DVec3::new(0.5, 4.0, 0.5));
        e.spawn(prey, DVec3::new(6.5, 4.0, 0.5));
        e.ensure_mob_ids();
        for m in &mut e.mobs {
            m.persistent = true;
        }
        let health = e.mobs[1].health;
        let mut aimed = false;
        for _ in 0..200 {
            e.update(0.05, &scene, &quiet());
            aimed |= e.mobs[0].aquatic.as_ref().unwrap().beam.is_some();
            if e.mobs.len() < 2 || e.mobs[1].health < health {
                break;
            }
        }
        assert!(aimed, "{prey:?}: the guardian never aimed");
        // An axolotl hunts the guardian back and may close within 3
        // blocks, where a guardian lets go; a squid takes the beam.
        if prey == MobKind::Squid {
            assert!(e.mobs.len() < 2 || e.mobs[1].health < health, "{prey:?} unhurt");
        }
    }
}

#[test]
fn baby_dolphins_grow_with_time_and_fish_and_age_locks_hold_them() {
    let mut e = Entities::new(12);
    e.spawn(MobKind::Dolphin, DVec3::new(0.5, 4.0, 0.5));
    let egg = voxelcraft::content::spawn_egg::egg(MobKind::Dolphin).unwrap();
    let b = e.spawn_baby_from_egg(0, egg).unwrap();
    for m in &mut e.mobs {
        m.persistent = true;
    }
    assert!(e.mobs[b].baby && e.mobs[b].age == -24000);
    // `Dolphin.BABY_SCALE`: the box is 0.65 of the adult's.
    let adult = e.mobs[0].shape();
    assert!((e.mobs[b].shape().height - adult.height * 0.65).abs() < 1e-9);
    let scene = Scene { water: true, ..Scene::default() };
    for _ in 0..20 {
        e.update(0.05, &scene, &quiet());
    }
    let b = e.mobs.iter().position(|m| m.baby).unwrap();
    assert_eq!(e.mobs[b].age, -24000 + 20);
    // A fish: a tenth of the 1199 seconds left, in whole seconds.
    let age = e.mobs[b].age;
    assert_eq!(e.use_animal(b, Some(Stack::new(Item::COD, 1)), PlayerId::HOST), Some(true));
    assert_eq!(e.mobs[b].age, age + (((-age) / 20) as f32 * 0.1) as i32 * 20);
    // A golden dandelion locks the age (dolphins aren't `#cannot_be_age_locked`).
    let dandelion = Stack::new(voxelcraft::world::gen5_blocks::GOLDEN_DANDELION, 1);
    assert_eq!(e.use_animal(b, Some(dandelion), PlayerId::HOST), Some(true));
    assert_eq!(e.mobs[b].age, -24000);
    for _ in 0..20 {
        e.update(0.05, &scene, &quiet());
    }
    let b = e.mobs.iter().position(|m| m.baby).unwrap();
    assert_eq!(e.mobs[b].age, -24000);
}

/// The world and session `tests/shared_use.rs` uses: the player at
/// (4.5, 145, 4.5) looking along +x.
fn session_scene() -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(IVec3::new(4, 144, 4), Block::STONE);
    let mut session = PlayerSession::new(PlayerId(6), DVec3::new(4.5, 145., 4.5));
    session.player.on_ground = true;
    (world, session, Entities::new(7))
}

fn use_on(s: &mut PlayerSession, w: &mut World, e: &mut Entities, sneaking: bool) -> Option<Used> {
    s.use_held(0, w, e, &UseContext { sneaking, others: &[] }).ok().flatten().map(|u| u.used)
}

#[test]
fn the_nautilus_inventory_opens_like_a_horses_on_every_path() {
    let (mut w, mut s, mut e) = session_scene();
    e.spawn(MobKind::Nautilus, DVec3::new(6.0, 146.0, 4.5));
    e.ensure_mob_ids();
    let id = mounts::MOB_VEHICLE | e.mobs[0].uid;
    // Wild: sneak-use doesn't open anything.
    assert_ne!(use_on(&mut s, &mut w, &mut e, true), Some(Used::OpenVehicle(id)));
    assert!(e.vehicle_slots(id).is_none());
    e.mobs[0].mount.as_mut().unwrap().tame = true;
    // `AbstractNautilus.mobInteract`: a tame adult opens on sneak-use.
    assert_eq!(use_on(&mut s, &mut w, &mut e, true), Some(Used::OpenVehicle(id)));
    assert_eq!(e.vehicle_title(id), "Nautilus (saddle, armor)");
    // The screen's slots: saddle, then body armor (shift-clicks place them;
    // anything else moves between the hotbar and the main grid).
    s.inventory.slots[0] = Some(Stack::new(Item::SADDLE, 1));
    s.inventory.slots[1] = Some(Stack::new(voxelcraft::equipment::IRON_NAUTILUS_ARMOR, 1));
    s.inventory.slots[2] = Some(Stack::new(Item::IRON_HORSE_ARMOR, 1));
    {
        let mut screen =
            StorageScreen { session: &mut s, world: &mut w, entities: &mut e, storage: Storage::Vehicle(id) };
        assert_eq!(screen.container_len(), 2);
        screen.quick_move(StorageSlot::Inventory(0));
        screen.quick_move(StorageSlot::Inventory(1));
        screen.quick_move(StorageSlot::Inventory(2));
    }
    let slots = e.vehicle_slots(id).unwrap();
    assert_eq!(slots[0].map(|s| s.item), Some(Item::SADDLE));
    assert_eq!(slots[1].map(|s| s.item), Some(voxelcraft::equipment::IRON_NAUTILUS_ARMOR));
    assert!(s.inventory.slots[2].is_none(), "horse armor left the hotbar");
    assert!(s.inventory.slots[9..].iter().any(|s| s.is_some_and(|s| s.item == Item::IRON_HORSE_ARMOR)));
    // A rider opens it from the saddle (the inventory key and use go
    // through the same vehicle storage).
    e.mobs[0].mount.as_mut().unwrap().rider = Some(PlayerId(6));
    s.player.vehicle = Some(id);
    assert_eq!(use_on(&mut s, &mut w, &mut e, false), Some(Used::OpenVehicle(id)));
    // Someone else riding it: not for others (`openCustomInventoryScreen`).
    e.mobs[0].mount.as_mut().unwrap().rider = Some(PlayerId(9));
    s.player.vehicle = None;
    assert_ne!(use_on(&mut s, &mut w, &mut e, true), Some(Used::OpenVehicle(id)));
    e.mobs[0].mount.as_mut().unwrap().rider = None;
    // Babies have none.
    e.mobs[0].baby = true;
    assert!(e.vehicle_slots(id).is_none());
    e.mobs[0].baby = false;
    // The CLI's `chest take` reads the same slots.
    let mut agent = Agent::new(s.player.pos);
    agent.session.player.on_ground = true;
    agent.execute(Command::Chest(true, 0), &mut w, &mut e, &[]).unwrap();
    assert!(agent.session.inventory.slots.iter().any(|s| s.is_some_and(|s| s.item == Item::SADDLE)));
    assert!(e.vehicle_slots(id).unwrap()[0].is_none());
}
