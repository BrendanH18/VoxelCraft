//! v1.0 gap Track 3b (FOO-04c, MOB-03c) through the real entity tick:
//! horses rearing and getting mad, the camel's pose and dash sounds and the
//! dash a release in the air still costs, and the mount screens' Java
//! layouts. Camel turning and looking, currents, lava, guardians and baby
//! dolphins are unit-tested beside their code. Sources and differences are
//! in `docs/verification/2026-11-27-v1.0-gap-track-03b-mounts-water.md`.
use std::collections::HashMap;

use glam::{DVec3, IVec3};
use voxelcraft::{
    entity::{
        Ctx, Entities, EntityEvent, MobKind, MobSound, MobWorld, PlayerId, Rng, Target,
        mounts::{self, Cue, MountArmor},
    },
    inventory::{Inventory, Stack},
    item::Item,
    physics::BlockSource,
    player::{MoveInput, Player},
    simulation::lighting::SpawnLight,
    world::{block::Block, terrain::Dimension},
};

/// Stone below y = 1, air above.
#[derive(Default)]
struct Scene {
    blocks: HashMap<IVec3, Block>,
}
impl BlockSource for Scene {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(self.blocks.get(&p).copied().unwrap_or(if p.y < 1 { Block::STONE } else { Block::AIR }))
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

fn mount_sounds(events: &[EntityEvent]) -> Vec<Cue> {
    events
        .iter()
        .filter_map(|e| match e {
            EntityEvent::Sound { sound: MobSound::Mount(cue), .. } => Some(*cue),
            _ => None,
        })
        .collect()
}

/// A saddled `kind` at `pos` with the host riding it, looking along +x.
fn ridden(e: &mut Entities, kind: MobKind, pos: DVec3) -> (usize, Player) {
    e.spawn(kind, pos);
    e.ensure_mob_ids();
    let i = e.mobs.len() - 1;
    let m = &mut e.mobs[i];
    m.baby = false;
    m.age = 0;
    m.persistent = true;
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

fn ride(e: &mut Entities, p: &Player, input: MoveInput, scene: &Scene) -> Vec<EntityEvent> {
    e.mount_input(PlayerId::HOST, p, input, &Inventory::default(), 0);
    e.update(0.05, scene, &quiet())
}

/// Holds jump for `ticks`, then lets go; the events of every tick.
fn charge_and_release(e: &mut Entities, p: &Player, ticks: usize, scene: &Scene) -> Vec<EntityEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(ride(e, p, MoveInput { jump: true, ..MoveInput::default() }, scene));
    }
    events.extend(ride(e, p, MoveInput::default(), scene));
    events
}

#[test]
fn horses_rear_when_hurt_one_time_in_three_and_on_every_jump() {
    // `AbstractHorse.hurtServer`: one hit in three rears.
    let mut rng = Rng::new(9);
    let mut reared = 0;
    for _ in 0..600 {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Horse, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[0].damage(1.0, None, &mut rng);
        reared += usize::from(e.mobs[0].mount.as_ref().unwrap().standing());
    }
    assert!((150..250).contains(&reared), "{reared} of 600");
    // Camels and llamas never rear.
    for kind in [MobKind::Camel, MobKind::Llama] {
        for _ in 0..30 {
            let mut e = Entities::new(1);
            e.spawn(kind, DVec3::new(0.5, 1.0, 0.5));
            e.mobs[0].damage(1.0, None, &mut rng);
            assert!(!e.mobs[0].mount.as_ref().unwrap().standing(), "{kind:?}");
        }
    }
    // `onPlayerJump` and `handleStartJump`: a released jump rears a saddled
    // horse, sliding allowed (its leap still carries it).
    let scene = Scene::default();
    let mut e = Entities::new(2);
    let (i, p) = ridden(&mut e, MobKind::Horse, DVec3::new(0.5, 1.0, 0.5));
    for _ in 0..3 {
        ride(&mut e, &p, MoveInput::default(), &scene);
    }
    charge_and_release(&mut e, &p, 6, &scene);
    let s = e.mobs[i].mount.as_ref().unwrap();
    assert!(s.standing() && s.stand_anim > 0.0, "the jump rears it");
    assert!(e.mobs[i].pos.y > 1.0, "and it leaps");
}

#[test]
fn untamed_horses_and_llamas_get_mad_at_items_that_arent_food() {
    let scene = Scene::default();
    for (kind, cue, rears) in [
        (MobKind::Horse, Cue::HorseAngry, true),
        (MobKind::Donkey, Cue::DonkeyAngry, true),
        (MobKind::Mule, Cue::MuleAngry, true),
        (MobKind::ZombieHorse, Cue::ZombieHorseAngry, true),
        (MobKind::Llama, Cue::LlamaAngry, false),
    ] {
        let mut e = Entities::new(3);
        e.spawn(kind, DVec3::new(2.5, 1.0, 0.5));
        e.ensure_mob_ids();
        e.mobs[0].persistent = true;
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::STICK, 1));
        let used = e.use_mount(DVec3::new(0.5, 1.6, 0.5), DVec3::X, 4.0, PlayerId::HOST, &mut inv, 0, false);
        assert!(used, "{kind:?}");
        let s = e.mobs[0].mount.as_ref().unwrap();
        assert!(s.rider.is_none(), "{kind:?}: nobody got on");
        assert_eq!(s.standing(), rears, "{kind:?}");
        assert_eq!(inv.slots[0].map(|s| s.count), Some(1), "{kind:?}: the stick stays");
        let events = e.update(0.05, &scene, &quiet());
        assert_eq!(mount_sounds(&events), [cue], "{kind:?}");
    }
}

#[test]
fn camels_sound_as_they_sit_stand_dash_and_get_their_dash_back() {
    let scene = Scene::default();
    for (kind, sit, stand, dash, ready) in [
        (MobKind::Camel, Cue::CamelSit, Cue::CamelStand, Cue::CamelDash, Cue::CamelDashReady),
        (MobKind::CamelHusk, Cue::CamelHuskSit, Cue::CamelHuskStand, Cue::CamelHuskDash, Cue::CamelHuskDashReady),
    ] {
        let mut e = Entities::new(4);
        let (i, p) = ridden(&mut e, kind, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[i].mount.as_mut().unwrap().sit_down();
        let events = ride(&mut e, &p, MoveInput::default(), &scene);
        assert_eq!(mount_sounds(&events), [sit], "{kind:?}");
        e.mobs[i].mount.as_mut().unwrap().pose_time = 400;
        let events = ride(&mut e, &p, MoveInput { forward: 1.0, ..MoveInput::default() }, &scene);
        assert_eq!(mount_sounds(&events), [stand], "{kind:?}");
        let mut events = Vec::new();
        for _ in 0..60 {
            events.extend(ride(&mut e, &p, MoveInput::default(), &scene));
        }
        assert!(mount_sounds(&events).is_empty(), "{kind:?}: {:?}", mount_sounds(&events));
        // The dash: its sound on release, then the ready sound 55 ticks on.
        let events = charge_and_release(&mut e, &p, 6, &scene);
        assert_eq!(mount_sounds(&events), [dash], "{kind:?}");
        let s = e.mobs[i].mount.as_ref().unwrap();
        assert!(s.dashing && s.dash_cooldown > 50, "{kind:?}");
        let mut ticks = 0;
        let mut events = Vec::new();
        while !mount_sounds(&events).contains(&ready) {
            events = ride(&mut e, &p, MoveInput::default(), &scene);
            ticks += 1;
            assert!(ticks < 60, "{kind:?}");
        }
        assert_eq!(e.mobs[i].mount.as_ref().unwrap().dash_cooldown, 0);
    }
}

#[test]
fn a_dash_let_go_of_in_the_air_still_costs_the_cooldown() {
    // `handleStartJump` sets `DASH`, whose change starts a 55-tick cooldown
    // (`onSyncedDataUpdated`), though `onPlayerJump` refused the dash off
    // the ground.
    let scene = Scene::default();
    let mut e = Entities::new(5);
    let (i, p) = ridden(&mut e, MobKind::Camel, DVec3::new(0.5, 1.0, 0.5));
    for _ in 0..3 {
        ride(&mut e, &p, MoveInput::default(), &scene);
    }
    // Lift it into the air for the release.
    e.mobs[i].pos.y = 6.0;
    e.mobs[i].on_ground = false;
    let events = charge_and_release(&mut e, &p, 6, &scene);
    assert_eq!(mount_sounds(&events), [Cue::CamelDash]);
    let m = &e.mobs[i];
    let s = m.mount.as_ref().unwrap();
    assert!(s.dash_cooldown > 50, "{}", s.dash_cooldown);
    assert!(m.vel.x.abs() < 1.0, "no dash: {}", m.vel.x);
}

#[test]
fn mount_screens_follow_javas_layouts() {
    let mut e = Entities::new(6);
    let kinds = [
        MobKind::Horse,
        MobKind::Donkey,
        MobKind::Llama,
        MobKind::Camel,
        MobKind::SkeletonHorse,
        MobKind::Nautilus,
        MobKind::Pig,
    ];
    for (n, kind) in kinds.into_iter().enumerate() {
        e.spawn(kind, DVec3::new(n as f64 * 4.0, 1.0, 0.0));
    }
    e.ensure_mob_ids();
    e.mobs[1].mount.as_mut().unwrap().chest = true;
    let llama = e.mobs[2].mount.as_mut().unwrap();
    llama.chest = true;
    llama.strength = 2;
    let screen = |e: &Entities, i: usize| e.mount_screen(mounts::MOB_VEHICLE | e.mobs[i].uid);
    // `HorseInventoryMenu`: saddle (8, 18), body (8, 36).
    let horse = screen(&e, 0).unwrap();
    assert_eq!((horse.saddle, horse.armor, horse.columns), (true, Some(MountArmor::Horse), 0));
    assert_eq!(horse.slots(), [(0, 8.0, 18.0), (1, 8.0, 36.0)]);
    // A donkey's chest: five columns of three from (80, 18), no body slot.
    let donkey = screen(&e, 1).unwrap();
    assert_eq!((donkey.armor, donkey.columns), (None, 5));
    let slots = donkey.slots();
    assert_eq!(slots.len(), 1 + 15);
    assert_eq!(slots[1], (2, 80.0, 18.0));
    assert_eq!(slots[5], (6, 152.0, 18.0));
    assert_eq!(slots[15], (16, 152.0, 54.0));
    // A llama: no saddle slot, its carpet, three slots a strength point.
    let llama = screen(&e, 2).unwrap();
    assert_eq!((llama.saddle, llama.armor, llama.columns), (false, Some(MountArmor::Llama), 2));
    assert_eq!(llama.slots()[0], (1, 8.0, 36.0));
    assert_eq!(llama.slots().last(), Some(&(7, 98.0, 54.0)));
    // A camel and a skeleton horse: a saddle only.
    for i in [3, 4] {
        let s = screen(&e, i).unwrap();
        assert_eq!(s.slots(), [(0, 8.0, 18.0)], "{:?}", e.mobs[i].kind);
    }
    // `NautilusInventoryMenu`: saddle and nautilus armor.
    assert_eq!(screen(&e, 5).unwrap().armor, Some(MountArmor::Nautilus));
    assert!(screen(&e, 6).is_none(), "a pig has no screen");
    // The title is the mount's name (`getDisplayName`).
    assert_eq!(e.mount_name(mounts::MOB_VEHICLE | e.mobs[4].uid).as_deref(), Some("Skeleton Horse"));
}
