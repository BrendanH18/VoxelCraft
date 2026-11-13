//! One scenario per neutral mob and rule, each with the rule on and off: the
//! host provokes the mob, then dies (`forgive_dead_players`) or a bystander
//! stands nearer (`universal_anger`).
use super::*;
use crate::{
    entity::{EntityEvent, Target, nether::NetherMob},
    physics::test_util::Grid,
    simulation::lighting::SpawnLight,
    world::terrain::Dimension,
};

const HOST: PlayerId = PlayerId::HOST;
const BYSTANDER: PlayerId = PlayerId(2);
/// The mob kinds whose anger `neutral` drives (polar bears run their own).
const DRIVEN: [MobKind; 5] =
    [MobKind::Bee, MobKind::IronGolem, MobKind::Wolf, MobKind::Enderman, MobKind::ZombifiedPiglin];

fn grid() -> Grid {
    Grid::flat(0)
}

/// The host 6 blocks east, the bystander 3 blocks west.
fn ctx(dimension: Dimension) -> Ctx {
    Ctx {
        players: vec![
            Target::new(HOST, DVec3::new(6.5, 1.0, 0.5), true),
            Target::new(BYSTANDER, DVec3::new(-2.5, 1.0, 0.5), true),
        ],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension,
        spawn_light: SpawnLight::NIGHT,
    }
}

fn dimension(kind: MobKind) -> Dimension {
    if matches!(kind, MobKind::ZombifiedPiglin | MobKind::Piglin) { Dimension::Nether } else { Dimension::Overworld }
}

/// `kind` at the origin (plus `extra` more of its kind nearby), hit once by
/// the host and given one update to take the hit in.
fn provoked(kind: MobKind, forgive: bool, universal: bool, extra: &[DVec3]) -> Entities {
    let mut e = Entities::new(41);
    e.neutral_forgive_dead_players = forgive;
    e.neutral_universal_anger = universal;
    for p in std::iter::once(DVec3::new(0.5, 1.0, 0.5)).chain(extra.iter().copied()) {
        e.spawn(kind, p);
        e.mobs.last_mut().unwrap().on_ground = true;
    }
    e.ensure_mob_ids();
    // The update runs as the first mob's off tick (`spawn_ticks + uid` odd),
    // so it takes the hit without its target selector picking a player yet.
    e.spawn_ticks = u64::from(e.mobs[0].uid);
    e.attack(0, DVec3::X, 0.5);
    e.update(0.05, &grid(), &ctx(dimension(kind)));
    e
}

#[test]
fn every_neutral_kind_holds_its_attacker_as_its_persistent_target() {
    for kind in KINDS {
        let e = provoked(kind, true, false, &[]);
        assert_eq!(e.mobs[0].anger_target(), Some(HOST), "{kind:?}");
        assert!(!e.mobs[0].angry_at_all_players(), "{kind:?}");
    }
}

/// `NeutralMob.playerDied`, with the rule on and off.
fn forgives(kind: MobKind) {
    // On: the host dies nearby and every neutral mob forgives it.
    let mut e = provoked(kind, true, false, &[]);
    e.player_died(HOST, DVec3::new(6.5, 1.0, 0.5));
    assert_eq!(e.mobs[0].anger_target(), None, "{kind:?} forgives");
    // Another player's death changes nothing.
    let mut e = provoked(kind, true, false, &[]);
    e.player_died(BYSTANDER, DVec3::new(-2.5, 1.0, 0.5));
    assert_eq!(e.mobs[0].anger_target(), Some(HOST), "{kind:?} only forgives the dead");
    // Off: the grudge outlives the death.
    let mut e = provoked(kind, false, false, &[]);
    e.player_died(HOST, DVec3::new(6.5, 1.0, 0.5));
    assert_eq!(e.mobs[0].anger_target(), Some(HOST), "{kind:?} holds the grudge");
}

#[test]
fn bee_forgives_a_dead_player_only_under_forgive_dead_players() {
    forgives(MobKind::Bee);
}

#[test]
fn iron_golem_forgives_a_dead_player_only_under_forgive_dead_players() {
    forgives(MobKind::IronGolem);
}

#[test]
fn wolf_forgives_a_dead_player_only_under_forgive_dead_players() {
    forgives(MobKind::Wolf);
}

#[test]
fn enderman_forgives_a_dead_player_only_under_forgive_dead_players() {
    forgives(MobKind::Enderman);
}

#[test]
fn zombified_piglin_forgives_a_dead_player_only_under_forgive_dead_players() {
    forgives(MobKind::ZombifiedPiglin);
}

#[test]
fn the_death_notice_reaches_polar_bears_too() {
    forgives(MobKind::PolarBear);
}

#[test]
fn the_death_notice_box_is_the_dead_players_block_inflated_32_10_32() {
    // The box is the dead player's block inflated by 32, 10 and 32: a mob
    // whose box reaches into it hears, one past it doesn't.
    // The mob stands at (0.5, 1, 0.5).
    for (at, heard) in [
        (DVec3::new(32.5, 1.0, 0.5), true),
        (DVec3::new(33.5, 1.0, 0.5), false),
        (DVec3::new(0.5, 1.0, -31.5), true),
        (DVec3::new(0.5, 1.0, -33.5), false),
        (DVec3::new(0.5, 11.5, 0.5), true),
        (DVec3::new(0.5, 14.5, 0.5), false),
        (DVec3::new(0.5, -8.5, 0.5), true),
        (DVec3::new(0.5, -10.5, 0.5), false),
    ] {
        let mut e = provoked(MobKind::ZombifiedPiglin, true, false, &[]);
        // Back where it started: its update may have stepped it toward the host.
        e.mobs[0].pos = DVec3::new(0.5, 1.0, 0.5);
        e.player_died(HOST, at);
        assert_eq!(e.mobs[0].anger_target().is_none(), heard, "{at}");
    }
}

#[test]
fn a_held_grudge_finds_the_respawned_player_and_a_forgiven_one_does_not() {
    // A zombified piglin: off, the host's respawned body is still its target.
    for forgive in [true, false] {
        let mut e = provoked(MobKind::ZombifiedPiglin, forgive, false, &[]);
        let mut c = ctx(Dimension::Nether);
        c.players[0].alive = false;
        c.players[0].targetable = false;
        e.player_died(HOST, c.players[0].pos);
        for _ in 0..20 {
            e.update(0.05, &grid(), &c);
        }
        // Respawned beside it; the bystander stays put.
        c.players[0] = Target::new(HOST, DVec3::new(2.0, 1.0, 0.5), true);
        let mut hit = Vec::new();
        for _ in 0..80 {
            for ev in e.update(0.05, &grid(), &c) {
                if let EntityEvent::PlayerHit { player, .. } = ev {
                    hit.push(player);
                }
            }
        }
        if forgive {
            assert!(hit.is_empty(), "a forgiven piglin leaves everyone alone: {hit:?}");
        } else {
            assert!(!hit.is_empty() && hit.iter().all(|p| *p == HOST), "{hit:?}");
        }
    }
}

#[test]
fn angry_endermen_and_zombified_piglins_go_for_their_attacker_not_the_nearest_player() {
    for kind in [MobKind::Enderman, MobKind::ZombifiedPiglin] {
        let mut e = provoked(kind, true, false, &[]);
        // The bystander now stands closest; the host is a little further off.
        let mut c = ctx(dimension(kind));
        c.players[0].pos = DVec3::new(3.0, 1.0, 0.5);
        c.players[1].pos = DVec3::new(-1.5, 1.0, 0.5);
        let mut hit = Vec::new();
        for _ in 0..100 {
            for ev in e.update(0.05, &grid(), &c) {
                if let EntityEvent::PlayerHit { player, .. } = ev {
                    hit.push(player);
                }
            }
        }
        assert!(hit.iter().all(|p| *p == HOST), "{kind:?}: {hit:?}");
        assert!(kind == MobKind::Enderman || !hit.is_empty(), "the zombified piglin fights back");
    }
}

/// `ResetUniversalAngerTargetGoal` and `isAngryAtAllPlayers`, with the rule on and off.
fn angry_at_everyone(kind: MobKind) {
    // Off: the attacker is the target; the bystander never is.
    let mut e = provoked(kind, true, false, &[]);
    let c = ctx(dimension(kind));
    for _ in 0..40 {
        e.neutral_sense(&grid(), &c);
        e.spawn_ticks += 1;
    }
    assert_eq!(e.mobs[0].anger_target(), Some(HOST), "{kind:?} off");
    // On: no persistent target at first, then the nearer bystander.
    let mut e = provoked(kind, true, true, &[]);
    assert!(e.mobs[0].angry_at_all_players(), "{kind:?}");
    assert_eq!(e.mobs[0].anger_target(), None, "{kind:?}");
    for _ in 0..200 {
        e.neutral_sense(&grid(), &c);
        e.spawn_ticks += 1;
        if e.mobs[0].anger_target().is_some() {
            break;
        }
    }
    assert_eq!(e.mobs[0].anger_target(), Some(BYSTANDER), "{kind:?} on");
    assert!(!e.mobs[0].angry_at_all_players(), "{kind:?} keeps the one it picked");
}

#[test]
fn bee_under_universal_anger_turns_on_the_nearest_player() {
    angry_at_everyone(MobKind::Bee);
}

#[test]
fn iron_golem_under_universal_anger_turns_on_the_nearest_player() {
    angry_at_everyone(MobKind::IronGolem);
}

#[test]
fn wolf_under_universal_anger_turns_on_the_nearest_player() {
    angry_at_everyone(MobKind::Wolf);
}

#[test]
fn enderman_under_universal_anger_turns_on_the_nearest_player() {
    angry_at_everyone(MobKind::Enderman);
}

#[test]
fn zombified_piglin_under_universal_anger_turns_on_the_nearest_player() {
    angry_at_everyone(MobKind::ZombifiedPiglin);
}

#[test]
fn universal_anger_needs_a_valid_visible_player_in_follow_range() {
    for kind in DRIVEN {
        let mut e = provoked(kind, true, true, &[]);
        let mut c = ctx(dimension(kind));
        let range = follow_range(kind).unwrap();
        c.players[0].pos = DVec3::new(range + 2.0, 1.0, 0.5);
        c.players[1].targetable = false;
        for _ in 0..200 {
            e.neutral_sense(&grid(), &c);
            e.spawn_ticks += 1;
        }
        assert_eq!(e.mobs[0].anger_target(), None, "{kind:?}: nobody in range is valid");
        assert!(e.mobs[0].angry_at_all_players(), "{kind:?}");
    }
    // Tame wolves can't attack their owner (`TamableAnimal.canAttack`).
    let mut e = provoked(MobKind::Wolf, true, true, &[]);
    e.mobs[0].animal.as_mut().unwrap().owner = Some(BYSTANDER);
    let mut c = ctx(Dimension::Overworld);
    c.players[0].pos = DVec3::new(30.5, 1.0, 0.5);
    for _ in 0..200 {
        e.neutral_sense(&grid(), &c);
        e.spawn_ticks += 1;
    }
    assert_eq!(e.mobs[0].anger_target(), None);
}

/// `TargetingConditions.forCombat()` checks the line of sight for every
/// kind, endermen included (`EndermanLookForPlayerGoal`'s start conditions),
/// and `LivingEntity.canAttack` refuses players on peaceful.
#[test]
fn universal_anger_needs_a_line_of_sight_and_a_difficulty_above_peaceful() {
    let mut walled = grid();
    for x in [-1, 3] {
        for y in 0..8 {
            for z in -4..=4 {
                walled.set(glam::IVec3::new(x, y, z), crate::world::block::Block::STONE);
            }
        }
    }
    for kind in DRIVEN {
        let c = ctx(dimension(kind));
        let mut e = provoked(kind, true, true, &[]);
        for _ in 0..200 {
            e.neutral_sense(&walled, &c);
            e.spawn_ticks += 1;
        }
        assert_eq!(e.mobs[0].anger_target(), None, "{kind:?} sees nobody through the walls");
        let mut e = provoked(kind, true, true, &[]);
        e.difficulty = crate::simulation::difficulty::Difficulty::Peaceful;
        for _ in 0..200 {
            e.neutral_sense(&grid(), &c);
            e.spawn_ticks += 1;
        }
        assert_eq!(e.mobs[0].anger_target(), None, "{kind:?} on peaceful");
    }
}

#[test]
fn universal_anger_alerts_bees_wolves_and_zombified_piglins_of_the_same_kind_in_follow_range() {
    for kind in DRIVEN {
        let r = follow_range(kind).unwrap();
        let near = DVec3::new(r - 1.0, 1.0, 0.5);
        let far = DVec3::new(r + 3.0, 1.0, 0.5);
        let e = provoked(kind, true, true, &[near, far]);
        let alerted = alerts_same_type(kind);
        assert!(e.mobs[0].angry_at_all_players(), "{kind:?}");
        // An alerted mob's first sense tick may already have picked the
        // nearest player it can see (the seeded roll decides), which ends
        // its universal anger the way `NearestAttackableTargetGoal` does.
        let woke = |m: &Mob| m.angry_at_all_players() || m.anger_target().is_some();
        assert_eq!(woke(&e.mobs[1]), alerted, "{kind:?} in range");
        assert!(!woke(&e.mobs[2]), "{kind:?} out of range");
    }
}

#[test]
fn universal_anger_ignores_built_golems_and_lapses_with_the_timer() {
    let mut e = provoked(MobKind::IronGolem, true, true, &[]);
    e.mobs[0].built = true;
    e.neutral_sense(&grid(), &ctx(Dimension::Overworld));
    assert!(!e.mobs[0].angry_at_all_players(), "player-made golems never attack players");
    let mut e = provoked(MobKind::Wolf, true, true, &[]);
    e.mobs[0].provoked = 0.0;
    e.neutral_sense(&grid(), &ctx(Dimension::Overworld));
    assert!(!e.mobs[0].angry_at_all_players() && !e.mobs[0].neutral_universal);
}

#[test]
fn anger_survives_a_save_mid_grudge_for_every_kind() {
    for kind in DRIVEN {
        for universal in [false, true] {
            let mut e = provoked(kind, true, universal, &[]);
            e.mobs[0].persistent = true;
            let (target, all) = (e.mobs[0].anger_target(), e.mobs[0].angry_at_all_players());
            let mut loaded = Entities::new(1);
            if kind == MobKind::IronGolem {
                loaded.load_villagers(&e.villagers_to_string());
            } else {
                loaded.load_nether_mobs(&e.nether_mobs_to_string());
            }
            let m = &loaded.mobs[0];
            assert_eq!(m.kind, kind);
            assert_eq!((m.anger_target(), m.angry_at_all_players()), (target, all), "{kind:?} {universal}");
            assert!(m.neutral_angry(), "{kind:?} keeps its timer");
        }
    }
    // Records written before this change load calm, as they did.
    let e = provoked(MobKind::Wolf, true, false, &[]);
    let mut v = crate::entity::nether::save_mob(&e.mobs[0]);
    assert!(v["neutral_anger"].is_object());
    v.as_object_mut().unwrap().remove("neutral_anger");
    let m = crate::entity::nether::load_mob(&v).unwrap();
    assert_eq!((m.anger_target(), m.angry_at_all_players()), (None, false));
}

fn piglins(universal: bool, gold_bystander: bool) -> (Entities, Ctx) {
    let mut e = Entities::new(77);
    e.neutral_universal_anger = universal;
    for x in [0.5, 4.5] {
        e.spawn(MobKind::Piglin, DVec3::new(x, 1.0, 0.5));
        let m = e.mobs.last_mut().unwrap();
        m.on_ground = true;
        m.nether.as_mut().unwrap().immune = true;
    }
    e.ensure_mob_ids();
    let mut c = ctx(Dimension::Nether);
    c.players[1].gold_armor = gold_bystander;
    c.players[0].gold_armor = true;
    (e, c)
}

fn foe(n: &NetherMob) -> Option<Foe> {
    n.foe
}

fn look(e: &mut Entities, c: &Ctx) {
    for m in &mut e.mobs {
        m.nether.as_mut().unwrap().sense_timer = 0.0;
    }
    e.nether_sense(0.05, &grid(), c);
}

#[test]
fn piglins_under_universal_anger_turn_on_the_nearest_player_and_rally_the_others() {
    for universal in [false, true] {
        let (mut e, c) = piglins(universal, true);
        e.nether_hurt(0, Foe::Player(HOST));
        look(&mut e, &c);
        let n0 = e.mobs[0].nether.as_deref().unwrap();
        let n1 = e.mobs[1].nether.as_deref().unwrap();
        if universal {
            // The bystander stands nearest the first piglin, gold armour or not.
            assert_eq!(foe(n0), Some(Foe::Player(BYSTANDER)));
            // The second piglin picks the player nearest to it: the host.
            assert_eq!(foe(n1), Some(Foe::Player(HOST)));
            assert!(n0.universal > 0.0 && n1.universal > 0.0);
        } else {
            assert_eq!(foe(n0), Some(Foe::Player(HOST)));
            assert_eq!(foe(n1), Some(Foe::Player(HOST)), "broadcastAngerTarget");
            assert_eq!(n0.universal, 0.0);
        }
    }
    // With nobody to see, the attacker stays the target.
    let (mut e, mut c) = piglins(true, false);
    c.players[1].targetable = false;
    c.players[0].pos = DVec3::new(0.5, 1.0, 40.5);
    e.nether_hurt(0, Foe::Player(HOST));
    look(&mut e, &c);
    assert!(!e.mobs[0].nether.as_deref().unwrap().universal_pick);
    assert!(e.mobs[0].nether.as_deref().unwrap().universal > 0.0);
}

#[test]
fn a_piglin_with_universal_anger_attacks_gold_wearing_players() {
    let (mut e, c) = piglins(false, true);
    look(&mut e, &c);
    assert!(
        !matches!(e.mobs[0].nether.as_deref().unwrap().goal, crate::entity::nether::Goal::Attack { .. }),
        "gold calms a piglin"
    );
    e.mobs[0].nether.as_mut().unwrap().universal = 10.0;
    look(&mut e, &c);
    assert!(matches!(
        e.mobs[0].nether.as_deref().unwrap().goal,
        crate::entity::nether::Goal::Attack { foe: Foe::Player(BYSTANDER), .. }
    ));
}

#[test]
fn chest_angered_piglins_pick_the_nearest_player_under_universal_anger() {
    for universal in [false, true] {
        let (mut e, c) = piglins(universal, false);
        e.piglins_notice(HOST, DVec3::new(6.5, 1.0, 0.5), false, &grid());
        look(&mut e, &c);
        let want = if universal { BYSTANDER } else { HOST };
        assert_eq!(foe(e.mobs[0].nether.as_deref().unwrap()), Some(Foe::Player(want)), "{universal}");
    }
}
