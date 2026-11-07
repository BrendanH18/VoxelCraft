use super::*;
use crate::entity::{Target, category::MobCategory};
use crate::item::Item;
use crate::simulation::difficulty::Difficulty;
use crate::world::block::Block;
use crate::world::{
    cave_blocks as cb,
    chunk::ChunkData,
    terrain::{Dimension, Generator},
};
use std::sync::Arc;

const FLOOR: i32 = 139;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.insert_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
    }
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, FLOOR, z), Block::STONE);
        }
    }
    w
}

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        players,
        daylight: 0.,
        spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

fn feet(x: f64, z: f64) -> DVec3 {
    DVec3::new(x, f64::from(FLOOR + 1), z)
}

/// A standing warden that has finished emerging.
fn warden(e: &mut Entities, at: DVec3) -> usize {
    e.spawn(MobKind::Warden, at);
    e.ensure_mob_ids();
    e.mobs[0].on_ground = true;
    e.mobs.len() - 1
}

fn run(e: &mut Entities, w: &mut World, c: &Ctx, ticks: u32) -> Vec<EntityEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        crate::simulation::tick_world_rules(w, c.players.first().map_or(DVec3::ZERO, |p| p.pos), false, 0);
        events.extend(e.update_world(0.05, w, c, Difficulty::Normal));
    }
    events
}

#[test]
fn attributes_and_registry_match_java() {
    assert_eq!(MobKind::Warden.lan_index(), 72);
    assert_eq!(MobKind::from_lan_index(72), Some(MobKind::Warden));
    assert_eq!(MobKind::from_name("minecraft:warden"), Some(MobKind::Warden));
    assert_eq!(MobKind::Warden.max_health(), 500.0);
    assert_eq!(MobKind::Warden.shape().height, 2.9);
    assert_eq!(MobKind::Warden.category(), MobCategory::Monster);
    assert!(MobKind::Warden.fire_immune() && MobKind::Warden.despawns_in_peaceful());
    assert!(!MobKind::Warden.spawns_in(Dimension::Overworld));
    assert_eq!(MobKind::Warden.loot(), &[(Item::from_block(cb::SCULK_CATALYST), 1, 1)]);
    assert_eq!(MobKind::Warden.xp(&mut Rng::new(1)), 5);
    assert_eq!(MobKind::Warden.melee().0, 30.0);
    let m = Mob::new(MobKind::Warden, DVec3::ZERO, 0.0);
    assert_eq!(m.knockback_resistance(), 1.0);
    assert_eq!(m.warden.as_ref().unwrap().dig_cooldown, DIG_COOLDOWN);
}

#[test]
fn anger_sorts_angry_then_players_then_most_and_decays() {
    let mut a = Anger::default();
    let player = EntityRef::Player(PlayerId(1));
    let zombie = EntityRef::Mob(7);
    let cow = EntityRef::Mob(8);
    assert_eq!(a.increase(zombie, 85), 85);
    assert_eq!(a.increase(player, 50), 50);
    assert_eq!(a.increase(cow, 60), 60);
    assert_eq!(a.suspects.iter().map(|s| s.0).collect::<Vec<_>>(), vec![zombie, player, cow]);
    assert_eq!(a.increase(player, 200), MAX_ANGER);
    assert_eq!(a.suspects[0].0, player, "an angry player comes first");
    assert_eq!(a.active(None), MAX_ANGER);
    assert_eq!(a.active(Some(cow)), 60);
    // Decay: valid suspects lose one, gone ones wait, invalid ones go.
    a.tick(|e| match e {
        EntityRef::Mob(7) => Presence::Invalid,
        EntityRef::Mob(8) => Presence::Gone,
        _ => Presence::Valid,
    });
    assert_eq!(a.of(player), MAX_ANGER - 1);
    assert_eq!(a.of(zombie), 0);
    assert_eq!(a.absent, vec![(cow, 59)]);
    // Coming back adds the remembered anger.
    assert_eq!(a.increase(cow, 10), 69);
    let back = Anger::load(&a.save());
    assert!(back.suspects.is_empty());
    assert_eq!(back.absent.len(), 2);
    assert_eq!(anger_level(39), 0);
    assert_eq!(anger_level(40), 1);
    assert_eq!(anger_level(80), 2);
}

#[test]
fn a_fourth_warning_summons_an_emerging_warden_and_darkness() {
    let mut w = world();
    let mut e = Entities::new(9);
    let shrieker = IVec3::new(16, FLOOR + 1, 16);
    w.set_block(shrieker, cb::sculk_shrieker(false, true, false));
    let near = Target::new(PlayerId::HOST, feet(20.5, 16.5), true);
    let c = ctx(vec![near]);
    w.sculk_step_on(shrieker, feet(16.5, 16.5), EventSource::player(PlayerId::HOST, false), false);
    let mut tracker = crate::world::sculk::WardenSpawnTracker { warning_level: 3, ..Default::default() };
    let mut players = [crate::world::sculk::ShriekPlayer {
        id: PlayerId::HOST,
        pos: near.pos,
        alive: true,
        spectator: false,
        untargetable: false,
        tracker: &mut tracker,
    }];
    w.resolve_shrieks(&mut players, |_| false);
    assert_eq!(tracker.warning_level, 4);
    let mut events = run(&mut e, &mut w, &c, 92);
    assert_eq!(e.count(MobKind::Warden), 1);
    let m = e.mobs.iter().find(|m| m.kind == MobKind::Warden).unwrap();
    assert!((m.pos - shrieker.as_dvec3()).abs().max_element() <= 6.5);
    let s = m.warden.as_ref().unwrap();
    assert_eq!(s.pose, EMERGING);
    assert!(m.refuses_hit(10.0), "invulnerable while emerging");
    assert!(
        events
            .iter()
            .any(|ev| matches!(ev, EntityEvent::PlayerEffect { effect: Effect::Darkness, ticks: DARKNESS_TICKS, .. }))
    );
    events.extend(run(&mut e, &mut w, &c, u32::from(EMERGE_TICKS) + 2));
    let m = e.mobs.iter().find(|m| m.kind == MobKind::Warden).unwrap();
    assert!(!m.warden.as_ref().unwrap().digging_or_emerging(), "done emerging");
    assert!(!m.refuses_hit(10.0));
    assert!(events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Warden(Cue::Emerge), .. })));
}

#[test]
fn vibrations_anger_it_until_it_roars_and_attacks() {
    let mut w = world();
    let mut e = Entities::new(11);
    let i = warden(&mut e, feet(10.5, 10.5));
    let uid = e.mobs[i].uid;
    let player = Target::new(PlayerId(5), feet(13.5, 10.5), true);
    let c = ctx(vec![player]);
    let source = EventSource::player(PlayerId(5), false);
    // Each vibration (40 ticks of cooldown apart) adds 35 anger.
    let mut events = run(&mut e, &mut w, &c, 1);
    for _ in 0..3 {
        w.emit_game_event(player.pos, crate::world::vibration::GameEvent::BlockPlace, source, Some(Block::STONE));
        events.extend(run(&mut e, &mut w, &c, 45));
    }
    let m = e.mobs.iter().find(|m| m.uid == uid).unwrap();
    let s = m.warden.as_ref().unwrap();
    assert!(s.anger.of(EntityRef::Player(PlayerId(5))) >= ANGRY);
    assert!(s.roar_target.is_some() || s.attack_target.is_some() || s.pose == ROARING);
    events.extend(run(&mut e, &mut w, &c, u32::from(ROAR_TICKS) + 40));
    let m = e.mobs.iter().find(|m| m.uid == uid).unwrap();
    assert_eq!(m.warden.as_ref().unwrap().attack_target, Some(EntityRef::Player(PlayerId(5))));
    assert!(events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Warden(Cue::Roar), .. })));
    assert!(events.iter().any(|ev| matches!(ev,
        EntityEvent::PlayerHit { player: PlayerId(5), damage, cause, .. } if *damage == MELEE_DAMAGE && *cause == CAUSE_MELEE)));
}

#[test]
fn creative_and_sneaking_players_go_unheard() {
    let mut w = world();
    let mut e = Entities::new(12);
    let i = warden(&mut e, feet(10.5, 10.5));
    let uid = e.mobs[i].uid;
    let creative = Target::new(PlayerId(6), feet(14.5, 10.5), false);
    let c = ctx(vec![creative]);
    let source = EventSource { untargetable: true, ..EventSource::player(PlayerId(6), false) };
    w.emit_game_event(creative.pos, crate::world::vibration::GameEvent::BlockPlace, source, None);
    let sneaking = EventSource::player(PlayerId(6), true);
    w.emit_game_event(creative.pos, crate::world::vibration::GameEvent::Step, sneaking, Some(Block::STONE));
    run(&mut e, &mut w, &c, 20);
    let m = e.mobs.iter().find(|m| m.uid == uid).unwrap();
    assert!(m.warden.as_ref().unwrap().anger.suspects.is_empty());
}

#[test]
fn an_unreachable_target_gets_a_sonic_boom() {
    let mut w = world();
    let mut e = Entities::new(13);
    let i = warden(&mut e, feet(10.5, 10.5));
    let uid = e.mobs[i].uid;
    // A player on a pillar out of reach, eight blocks away.
    for y in FLOOR + 1..FLOOR + 6 {
        w.set_block(IVec3::new(18, y, 10), Block::STONE);
    }
    let player = Target::new(PlayerId(7), DVec3::new(18.5, f64::from(FLOOR + 6), 10.5), true);
    let c = ctx(vec![player]);
    {
        let s = e.mobs[i].warden.as_mut().unwrap();
        s.anger.increase(EntityRef::Player(PlayerId(7)), 100);
        Entities::warden_attack(s, EntityRef::Player(PlayerId(7)));
    }
    let events = run(&mut e, &mut w, &c, 200 + u32::from(SONIC_SOUND_AT) + 5);
    let boom = events
        .iter()
        .find(|ev| matches!(ev, EntityEvent::PlayerSonicBoom { player: PlayerId(7), .. }))
        .expect("a sonic boom");
    let EntityEvent::PlayerSonicBoom { damage, push, .. } = boom else { unreachable!() };
    assert_eq!(*damage, SONIC_DAMAGE);
    assert!(push.x > 0.0, "pushed away along the beam");
    assert!(e.mobs.iter().any(|m| m.uid == uid));
}

#[test]
fn a_minute_undisturbed_and_it_digs_away_without_loot() {
    let mut w = world();
    let mut e = Entities::new(14);
    let i = warden(&mut e, feet(10.5, 10.5));
    e.mobs[i].warden.as_mut().unwrap().dig_cooldown = 3;
    let c = ctx(Vec::new());
    for _ in 0..400 {
        run(&mut e, &mut w, &c, 1);
        if e.mobs[0].warden.as_ref().unwrap().pose == DIGGING {
            break;
        }
    }
    let s = e.mobs[0].warden.as_ref().unwrap();
    assert_eq!(s.pose, DIGGING);
    assert!(e.mobs[0].refuses_hit(10.0), "invulnerable while digging");
    run(&mut e, &mut w, &c, u32::from(DIG_TICKS) + 2);
    assert_eq!(e.count(MobKind::Warden), 0);
    assert!(e.items.is_empty() && e.orbs_to_string().len() <= 2, "no loot or experience");
}

#[test]
fn persistent_wardens_never_dig() {
    let mut w = world();
    let mut e = Entities::new(15);
    let i = warden(&mut e, feet(10.5, 10.5));
    e.mobs[i].persistent = true;
    let c = ctx(Vec::new());
    run(&mut e, &mut w, &c, 1300);
    assert_eq!(e.count(MobKind::Warden), 1);
}

#[test]
fn darkness_pulses_reach_survival_players_within_twenty() {
    let mut w = world();
    let mut e = Entities::new(16);
    warden(&mut e, feet(10.5, 10.5));
    let near = Target::new(PlayerId(1), feet(25.5, 10.5), true);
    let far = Target::new(PlayerId(2), feet(31.5, 31.5), true);
    let creative = Target::new(PlayerId(3), feet(12.5, 10.5), false);
    let renewing = Target {
        darkness_ticks: 250,
        effects: 1 << Effect::Darkness as u32,
        ..Target::new(PlayerId(4), feet(11.5, 11.5), true)
    };
    let c = ctx(vec![near, far, creative, renewing]);
    let events = run(&mut e, &mut w, &c, 121);
    let got: Vec<PlayerId> = events
        .iter()
        .filter_map(|ev| match ev {
            EntityEvent::PlayerEffect { player, effect: Effect::Darkness, .. } => Some(*player),
            _ => None,
        })
        .collect();
    assert!(got.contains(&PlayerId(1)));
    assert!(!got.contains(&PlayerId(2)) && !got.contains(&PlayerId(3)) && !got.contains(&PlayerId(4)));
}

#[test]
fn warden_state_saves_and_reloads() {
    let mut e = Entities::new(17);
    let i = warden(&mut e, feet(10.5, 10.5));
    {
        let s = e.mobs[i].warden.as_mut().unwrap();
        s.pose = ROARING;
        s.action = Some((Action::Roar, 12));
        s.anger.increase(EntityRef::Player(PlayerId(3)), 90);
        s.disturbance = Some((IVec3::new(4, 140, 5), 60));
        s.dig_cooldown = 700;
        s.roar_target = Some(EntityRef::Player(PlayerId(3)));
    }
    let v = crate::entity::nether::save_mob(&e.mobs[i]);
    let m = crate::entity::nether::load_mob(&v).unwrap();
    let s = m.warden.as_ref().unwrap();
    assert_eq!(m.kind, MobKind::Warden);
    assert_eq!(s.pose, ROARING);
    assert_eq!(s.action, Some((Action::Roar, 12)));
    assert_eq!(s.anger.absent, vec![(EntityRef::Player(PlayerId(3)), 90)]);
    assert_eq!(s.disturbance, Some((IVec3::new(4, 140, 5), 60)));
    assert_eq!(s.dig_cooldown, 700);
    assert_eq!(s.roar_target, Some(EntityRef::Player(PlayerId(3))));
    assert_eq!(s.activity, Activity::Roar);
    // LAN clients draw the pose, timers and anger.
    let mut look = State::default();
    look.apply_look(&s.look_state());
    assert_eq!((look.pose, look.action), (ROARING, Some((Action::Roar, 12))));
}

#[test]
fn a_kill_near_a_catalyst_feeds_it_instead_of_dropping_experience() {
    let mut w = world();
    let mut e = Entities::new(18);
    let catalyst = IVec3::new(16, FLOOR, 16);
    w.set_block(catalyst, cb::SCULK_CATALYST);
    let c = ctx(Vec::new());
    run(&mut e, &mut w, &c, 1);
    e.spawn(MobKind::Skeleton, feet(18.5, 16.5));
    let pos = e.mobs.last().unwrap().pos;
    e.mobs.last_mut().unwrap().dying = Some(0.0);
    e.drop_loot_with_fire(MobKind::Skeleton, pos, 0, false, true);
    assert!(e.orbs_to_string().len() <= 2, "no orbs");
    run(&mut e, &mut w, &c, 1);
    assert_eq!(w.get_block(catalyst), Some(cb::sculk_catalyst(true)));
    assert_eq!(w.sculk_entity(catalyst).unwrap().cursors.iter().map(|c| c.charge).sum::<i32>(), 5);
}
