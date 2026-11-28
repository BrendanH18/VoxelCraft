//! v1.0 gap Track 2c: Java's look and body controls and look goals.
use glam::DVec3;

use super::super::{Mob, MobKind};
use super::*;
use crate::entity::{Ctx, PlayerId, Rng, Target};
use crate::physics::test_util::Grid;

fn ctx(players: &[DVec3]) -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players: players.iter().enumerate().map(|(i, &p)| Target::new(PlayerId(i as u32 + 1), p, true)).collect(),
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn mob(kind: MobKind) -> Mob {
    let mut m = Mob::new(kind, DVec3::new(0.5, 64.0, 0.5), 0.0);
    m.on_ground = true;
    m
}

/// The head relative to the body, degrees.
fn rel(m: &Mob) -> f32 {
    wrap_degrees(m.look.head - m.body_yrot())
}

/// `LookControl.tick`: a request turns the head by its speed for two ticks,
/// then it turns back by 10° a tick; the pitch restarts level each tick.
#[test]
fn the_look_control_turns_by_its_limits_then_back_to_the_body() {
    let mut m = mob(MobKind::Cow);
    let body = m.yrot();
    m.look_ready();
    // A point 90° to the left at eye height.
    let side = super::super::spear::view_vector(0.0, body + 90.0);
    let at = m.look_eye() + side * 4.0;
    m.look_toward(at);
    m.look_tick();
    assert!((rel(&m) - 10.0).abs() < 1e-3, "{}", rel(&m));
    m.look_tick();
    assert!((rel(&m) - 20.0).abs() < 1e-3, "{}", rel(&m));
    // The request has run out: back toward the body.
    m.look_tick();
    assert!((rel(&m) - 10.0).abs() < 1e-3, "{}", rel(&m));
    // A frog's head turns 35° a tick, at most 5° off the body while it
    // walks a path; a cow's at most 75°.
    let mut f = mob(MobKind::Frog);
    f.look_ready();
    let fb = f.yrot();
    let at = f.look_eye() + super::super::spear::view_vector(0.0, fb + 90.0) * 4.0;
    f.look_toward(at);
    f.look_tick();
    assert!((rel(&f) - 35.0).abs() < 1e-3, "{}", rel(&f));
    // Pitch: a point 45° below, limited to 40° (`getMaxHeadXRot`).
    let mut c = mob(MobKind::Cow);
    let below = c.look_eye() + super::super::spear::view_vector(45.0, c.yrot()) * 3.0;
    c.look_toward(below);
    c.look_tick();
    assert!((c.look.pitch - 40.0).abs() < 1e-3, "{}", c.look.pitch);
    c.look_tick();
    c.look_tick();
    assert_eq!(c.look.pitch, 0.0, "level once the request ends");
}

/// `BodyRotationControl`: a still body keeps within 75° of a head that
/// moved, then turns to face it from the 11th to the 20th still tick; a
/// moving body keeps the head within 75° of its heading.
#[test]
fn the_body_follows_a_still_head_and_holds_it_while_moving() {
    let mut m = mob(MobKind::Zombie);
    m.look_ready();
    let body = m.body_yrot();
    let heading = m.yaw;
    m.head.last_stable = m.look.head;
    m.look.head = body + 100.0;
    m.body_tick(DVec3::ZERO);
    let turned = |m: &Mob| wrap_degrees(m.body_yrot() - body);
    assert!((turned(&m) - 25.0).abs() < 1e-3, "pulled within 75°: {}", turned(&m));
    for _ in 0..10 {
        m.body_tick(DVec3::ZERO);
    }
    assert!((turned(&m) - 25.0).abs() < 1e-3, "settling: {}", turned(&m));
    for _ in 0..10 {
        m.body_tick(DVec3::ZERO);
    }
    assert!(rel(&m).abs() < 1e-2, "faces the head: {}", rel(&m));
    // The drawn head follows; the heading movement follows (`yRot`) stays.
    assert!(m.head_yaw.abs() < 1e-3);
    assert_eq!(m.yaw, heading);
    assert_eq!(m.pose_values()[0], m.body_yaw);
    // Moving: the body takes the heading, the head within 75° of it.
    m.look.head = m.yrot() - 120.0;
    m.body_tick(DVec3::X * 0.2);
    assert!((rel(&m) + 75.0).abs() < 1e-3, "{}", rel(&m));
    // Carrying a mob passenger, a still body doesn't turn.
    let mut s = mob(MobKind::Strider);
    s.look_ready();
    let sb = s.body_yaw;
    s.head.carrying_mob = true;
    s.look.head = s.body_yrot() + 100.0;
    for _ in 0..30 {
        s.body_tick(DVec3::ZERO);
    }
    assert_eq!(s.body_yaw, sb);
}

/// `LookAtPlayerGoal(Player, 6)` at the default 0.02 chance per full
/// update: a cow looks at a player 5 blocks off for 20 to 39 full updates,
/// and never at one 7 blocks off.
#[test]
fn a_cow_looks_at_a_nearby_player_for_javas_time() {
    let w = Grid::flat(64);
    // Off to the side (the cow faces +X).
    let near = ctx(&[DVec3::new(0.5, 64.0, 5.5)]);
    let far = ctx(&[DVec3::new(0.5, 64.0, 7.5)]);
    for seed in 0..10 {
        let mut rng = Rng::new(seed);
        let mut m = mob(MobKind::Cow);
        for _ in 0..400 {
            m.head_goals(&w, &far, &mut rng, true);
            assert!(!matches!(m.head.running, Some((_, Running::LookAt { .. }))), "too far");
        }
        let mut ticks = 0;
        while !matches!(m.head.running, Some((_, Running::LookAt { .. }))) {
            m.head_goals(&w, &near, &mut rng, true);
            ticks += 1;
            assert!(ticks < 5000, "seed {seed}");
        }
        assert!(m.look.looking(), "it asks the look control");
        let yaw = yaw_toward(DVec3::new(0.0, 0.0, 5.0)).unwrap();
        m.look_tick();
        assert!(wrap_degrees(m.look.head - yaw).abs() < wrap_degrees(m.body_yrot() - yaw).abs());
        let mut held = 0;
        while matches!(m.head.running, Some((_, Running::LookAt { .. }))) {
            m.head_goals(&w, &near, &mut rng, true);
            held += 1;
            assert!(held < 200);
        }
        // `adjustedTickDelay(40 + nextInt(40))` full updates, every other tick.
        assert!((38..=84).contains(&held), "seed {seed}: {held}");
    }
}

/// `RandomLookAroundGoal` holds MOVE: it never starts while a better goal
/// moves the mob, starts at 2% a full update when it stands, and an
/// attack goal (LOOK) stops it.
#[test]
fn random_looks_wait_for_a_still_mob_and_yield_to_attacks() {
    let w = Grid::flat(64);
    let none = ctx(&[]);
    let mut rng = Rng::new(3);
    let mut m = mob(MobKind::Sheep);
    for _ in 0..2000 {
        m.head_goals(&w, &none, &mut rng, true);
        assert!(m.head.running.is_none());
    }
    let mut ticks = 0;
    while !matches!(m.head.running, Some((_, Running::Random { .. }))) {
        m.head_goals(&w, &none, &mut rng, false);
        ticks += 1;
        assert!(ticks < 5000);
    }
    assert!(m.look.looking());
    // Something else asks the look control (an attack): the random look stops.
    let at = m.look_eye() + DVec3::X;
    m.set_look_at(at, ATTACK_LOOK, ATTACK_LOOK);
    m.head_goals(&w, &none, &mut rng, false);
    assert!(m.head.running.is_none());
}

/// `Mob.updateControlFlags`: a strider carrying a zombified piglin loses
/// LOOK, so its look goals stop; the rider keeps its own.
#[test]
fn a_mob_steered_strider_stops_its_look_goals_and_the_rider_keeps_its_own() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(3.5, 64.0, 0.5)]);
    let mut rng = Rng::new(9);
    let mut s = mob(MobKind::Strider);
    s.head.carrying_mob = true;
    let mut rider = mob(MobKind::ZombifiedPiglin);
    rider.riding = Some(s.uid | crate::entity::mounts::MOB_VEHICLE);
    let mut looked = false;
    for _ in 0..3000 {
        s.head_goals(&w, &near, &mut rng, false);
        assert!(s.head.running.is_none());
        rider.head_goals(&w, &near, &mut rng, false);
        looked |= rider.head.running.is_some();
    }
    assert!(looked, "the rider's LookAtPlayerGoal and RandomLookAroundGoal run");
}

/// `SetEntityLookTargetSometimes(PLAYER, 6, 30..60)` and
/// `LookAtTargetSink(45, 90)`: a goat looks at a player within 6 blocks
/// once its ticker comes round, for 46 to 91 ticks.
#[test]
fn a_goat_looks_at_a_player_sometimes_for_the_sinks_time() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(4.5, 64.0, 0.5)]);
    for seed in 0..10 {
        let mut rng = Rng::new(seed);
        let mut m = mob(MobKind::Goat);
        let mut ticks = 0;
        while m.head.target.is_none() {
            m.head_goals(&w, &near, &mut rng, false);
            ticks += 1;
            assert!(ticks <= 60, "seed {seed}");
        }
        assert!(ticks >= 30, "seed {seed}: {ticks}");
        let mut held = 0;
        while m.head.target.is_some() {
            m.head_goals(&w, &near, &mut rng, false);
            held += 1;
            assert!(held < 200);
        }
        assert!((46..=92).contains(&held), "seed {seed}: {held}");
        // Goats turn their heads at most 15° off the body while walking.
        assert_eq!(m.max_head_y_rot(), 15.0);
    }
}

/// The riding baby piglin (`initRideHoglinActivity`): each tick a shuffled
/// one-shot picks a player, a piglin or anything within 8 blocks, or
/// nothing; an idle piglin's `RunOne` may instead do nothing for 30 to 60.
#[test]
fn a_riding_baby_piglin_looks_around_from_its_hoglin() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(4.5, 64.0, 0.5)]);
    let mut rng = Rng::new(5);
    let mut p = mob(MobKind::Piglin);
    p.baby = true;
    let hoglin = mob(MobKind::Hoglin);
    p.riding = Some(hoglin.uid | crate::entity::mounts::MOB_VEHICLE);
    let mut ticks = 0;
    while p.head.target.is_none() {
        p.head_goals(&w, &near, &mut rng, false);
        ticks += 1;
        assert!(ticks < 20, "a one-shot fires most ticks: {ticks}");
    }
    assert!(matches!(p.head.target, Some(Tracked::Player(_))));
}

/// A villager's full look behaviour weighs cats 8 to players' 2: with a
/// cat in sight it mostly looks at the cat.
#[test]
fn villagers_mostly_look_at_cats() {
    let w = Grid::flat(64);
    // Beyond `SetLookAndInteract`'s 4 blocks (Track 2d), which would make a
    // closer player the idle villager's look target first.
    let near = ctx(&[DVec3::new(6.5, 64.0, 0.5)]);
    let (mut cats, mut players) = (0, 0);
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut v = mob(MobKind::Villager);
        let cat = mob(MobKind::Cat);
        v.head.seen = vec![Seen {
            uid: 77,
            kind: MobKind::Cat,
            eye: cat.look_eye() + DVec3::X * 2.0,
            pos: DVec3::new(2.5, 64.0, 0.5),
            breedable: false,
            baby: false,
            chasing: None,
            farmer: false,
            wants_food: false,
        }];
        let mut ticks = 0;
        while v.head.target.is_none() && ticks < 200 {
            v.head_goals(&w, &near, &mut rng, false);
            ticks += 1;
        }
        match v.head.target {
            Some(Tracked::Mob(77, _)) => cats += 1,
            Some(Tracked::Player(_)) => players += 1,
            _ => {}
        }
    }
    assert!(cats > players * 2, "cats {cats}, players {players}");
}

/// `SmoothSwimmingLookControl(10)`: a dolphin whose head is more than 10°
/// off its body swings the body 4° a tick toward it.
#[test]
fn a_smooth_swimmer_swings_its_body_toward_its_head() {
    let mut d = mob(MobKind::Dolphin);
    d.look_ready();
    let body = d.body_yrot();
    d.look.head = body + 30.0;
    d.look_tick();
    assert!((wrap_degrees(d.body_yrot() - body) - 4.0).abs() < 1e-3, "{}", d.body_yrot() - body);
    // Its heading (`yRot`) isn't turned by the look.
    assert_eq!(d.yrot(), body);
}

/// v1.0 gap Track 2d: each kind's own eye (`EntityTypes`' `eyeHeight`, the
/// baby dimensions and the poses), where 2c took 0.85 of the box.
#[test]
fn mobs_look_from_their_own_eye_heights() {
    let close = |a: f64, b: f32| (a - f64::from(b)).abs() < 1e-6;
    assert!(close(mob(MobKind::Zombie).eye_height(), 1.74));
    assert!(close(mob(MobKind::Spider).eye_height(), 0.65));
    assert!(close(mob(MobKind::Enderman).eye_height(), 2.55));
    let mut v = mob(MobKind::Villager);
    assert!(close(v.eye_height(), 1.62));
    v.baby = true;
    v.age = -24000;
    assert!(close(v.eye_height(), 0.63), "Villager.BABY_DIMENSIONS");
    v.villager.as_mut().unwrap().sleeping = true;
    assert!(close(v.eye_height(), 0.2), "SLEEPING_DIMENSIONS");
    let mut slime = mob(MobKind::Slime);
    slime.set_size(4);
    assert!(close(slime.eye_height(), 1.3), "0.325 a size");
    assert!((slime.look_eye().y - (64.0 + f64::from(0.325_f32 * 4.0))).abs() < 1e-6);
}

/// v1.0 gap Track 2d: `GoalSelector`'s strict priorities. The parrot's
/// `LookAtPlayerGoal` (1) outranks `FollowOwnerGoal` (2, MOVE and LOOK),
/// so while it runs the owner goal can't start; the witch's random look (3)
/// holds MOVE against its patrol and village goals (4) but not the banner
/// goal (1).
#[test]
fn parrot_and_witch_looks_take_flags_by_priority() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(3.5, 64.0, 0.5)]);
    let mut rng = Rng::new(2);
    let mut p = mob(MobKind::Parrot);
    // The owner goal holding LOOK at 2 doesn't stop the look goal at 1.
    p.goals.running = Some(1);
    assert!(!p.look_held(1), "FollowOwnerGoal is worse than the look");
    let mut ticks = 0;
    while p.head.running.is_none() {
        p.head_goals(&w, &near, &mut rng, false);
        ticks += 1;
        assert!(ticks < 5000);
    }
    assert!(p.goals.running.is_none(), "the look interrupted FollowOwnerGoal");
    assert!(!p.idle_look_allows(2, true), "and it can't start while the look runs");
    let mut witch = mob(MobKind::Witch);
    witch.head.running = Some((1, Running::Random { rel: (1.0, 0.0), left: 30 }));
    assert_eq!(witch.idle_look_holds(), Some((3, true)));
    assert!(!witch.idle_look_allows(4, false), "LongDistancePatrolGoal waits");
    assert!(!witch.idle_look_allows(3, false), "PathfindToRaidGoal can't replace an equal priority");
    assert!(witch.idle_look_allows(1, false), "ObtainRaidLeaderBannerGoal outranks it");
}

/// v1.0 gap Track 2d: the merchant's customer (`tradingPlayer`), not the
/// nearest player, is who the trading sink and the trader's look follow.
#[test]
fn merchants_look_at_their_customer() {
    let w = Grid::flat(64);
    let players = ctx(&[DVec3::new(1.5, 64.0, 0.5), DVec3::new(-2.5, 64.0, 0.5)]);
    let mut rng = Rng::new(4);
    let mut v = mob(MobKind::Villager);
    v.hurt_age = 100;
    let s = v.villager.as_mut().unwrap();
    // Resting: no interaction looks, only the trading sink writes.
    s.activity = crate::entity::villager::Activity::Rest;
    s.trading = true;
    s.customer = Some(PlayerId(2));
    v.head_goals(&w, &players, &mut rng, false);
    assert_eq!(v.head.target, Some(Tracked::Player(PlayerId(2))));
    let mut t = mob(MobKind::WanderingTrader);
    let s = t.villager.as_mut().unwrap();
    s.trading = true;
    s.customer = Some(PlayerId(2));
    t.head_goals(&w, &players, &mut rng, false);
    t.head_goals(&w, &players, &mut rng, false);
    assert!(matches!(t.head.running, Some((0, Running::LookAt { at: Tracked::Player(PlayerId(2)), .. }))));
}

/// v1.0 gap Track 2d: an idle adult villager sets a player within 4 blocks
/// as `INTERACTION_TARGET` and looks at it (`SetLookAndInteract`); then
/// `ShowTradesToPlayer` keeps looking for 40 ticks with nothing to show,
/// erases the interaction, and the next tick it starts over.
#[test]
fn villagers_show_trades_to_players_close_by() {
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(2.5, 64.0, 0.5)]);
    let mut rng = Rng::new(6);
    let mut v = mob(MobKind::Villager);
    v.head_goals(&w, &near, &mut rng, false);
    assert_eq!(v.head.interaction, Some(PlayerId(1)));
    assert_eq!(v.head.target, Some(Tracked::Player(PlayerId(1))));
    let mut ticks = 1;
    while v.head.interaction.is_some() {
        v.head_goals(&w, &near, &mut rng, false);
        assert_eq!(v.head.target, Some(Tracked::Player(PlayerId(1))), "it keeps looking");
        ticks += 1;
        assert!(ticks < 100);
    }
    assert!((40..=42).contains(&ticks), "lookTime 40: {ticks}");
    v.head_goals(&w, &near, &mut rng, false);
    assert_eq!(v.head.interaction, Some(PlayerId(1)), "SetLookAndInteract again");
    // A player 6 blocks off is out of its 4-block reach.
    let far = ctx(&[DVec3::new(6.5, 64.0, 0.5)]);
    let mut v = mob(MobKind::Villager);
    v.head_goals(&w, &far, &mut rng, false);
    assert_eq!(v.head.interaction, None);
}

/// v1.0 gap Track 2e: `ShowTradesToPlayer` holds up the results of the
/// offers the player's item pays for (`displayAsHeldItem`, never dropped),
/// cycling every 40 ticks, keeps looking for 900, and clears the hand once
/// the player's hand changes to something it doesn't take.
#[test]
fn villagers_hold_up_the_trades_a_players_item_pays_for() {
    use crate::entity::equipment::{DEFAULT_DROP_CHANCE, Slot};
    use crate::entity::villager::Offer;
    use crate::inventory::Stack;
    use crate::item::Item;
    let w = Grid::flat(64);
    let mut near = ctx(&[DVec3::new(2.5, 64.0, 0.5)]);
    near.players[0].held_item = Some(Item::EMERALD);
    let mut rng = Rng::new(6);
    let mut v = mob(MobKind::Villager);
    let offer = |output: Item| Offer {
        cost: Stack::new(Item::EMERALD, 1),
        second: None,
        output: Stack::new(output, 1),
        uses: 0,
        max_uses: 12,
        xp: 1,
        demand: 0,
        multiplier: 0.05,
    };
    let offers = &mut v.villager.as_mut().unwrap().offers;
    offers[0] = Some(offer(Item::BREAD));
    offers[1] = Some(offer(Item::APPLE));
    v.head_goals(&w, &near, &mut rng, false);
    assert_eq!(v.shown_trade_item().map(|s| s.item), Some(Item::BREAD));
    assert_eq!(v.equipment.drop_chance(Slot::MainHand), 0.0);
    let mut shown = vec![Item::BREAD];
    for _ in 0..100 {
        v.head_goals(&w, &near, &mut rng, false);
        let item = v.shown_trade_item().unwrap().item;
        if shown.last() != Some(&item) {
            shown.push(item);
        }
    }
    assert_eq!(shown, [Item::BREAD, Item::APPLE, Item::BREAD], "cycles every 40 ticks");
    // Still looking well past 40 ticks: `lookTime` is 900.
    assert_eq!(v.head.interaction, Some(PlayerId(1)));
    near.players[0].held_item = Some(Item::BONE);
    v.head_goals(&w, &near, &mut rng, false);
    assert_eq!(v.shown_trade_item(), None);
    assert_eq!(v.equipment.drop_chance(Slot::MainHand), DEFAULT_DROP_CHANCE);
}

/// v1.0 gap Track 2d: `ShulkerLookControl` turns the head in the shell's
/// frame (a floor shulker's is the world's), never pitches it, and the
/// shell's `yBodyRot` stays 0; its `LookAtPlayerGoal` (1) takes LOOK from
/// a running `ShulkerAttackGoal` (4).
#[test]
fn shulkers_turn_their_heads_in_their_shells_frame() {
    use crate::world::end_blocks::Dir;
    let d = DVec3::new(-3.0, 1.0, 0.0);
    assert!((shulker_yaw(Dir::Down, d).unwrap() - yaw_toward(d).unwrap()).abs() < 1e-3);
    // On a wall facing east (clinging to its west face), "forward" is down.
    assert!((shulker_yaw(Dir::West, DVec3::new(0.0, -2.0, 0.0)).unwrap()).abs() < 1e-3);
    let mut s = mob(MobKind::Shulker);
    s.look_ready();
    let at = s.look_eye() + DVec3::new(0.0, 3.0, 4.0);
    s.goal_look_at(at, 180.0, 180.0);
    s.look_tick();
    s.body_tick(DVec3::ZERO);
    assert!(s.look.head.abs() < 1e-3, "south, in Java's degrees: {}", s.look.head);
    assert_eq!(s.look.pitch, 0.0);
    assert_eq!(s.head_pitch, 0.0);
    assert!(s.body_yrot().abs() < 1e-3);
    // Looking at a player interrupts the attack (priority 4 < 1 fails).
    let w = Grid::flat(64);
    let near = ctx(&[DVec3::new(3.5, 64.0, 0.5)]);
    let mut rng = Rng::new(1);
    assert_eq!(s.table_holder(), None);
    let mut ticks = 0;
    while !matches!(s.head.running, Some((0, _))) {
        s.head_goals(&w, &near, &mut rng, false);
        ticks += 1;
        assert!(ticks < 5000);
    }
    assert!(!s.idle_look_allows(4, true), "ShulkerAttackGoal waits for it");
}

/// v1.0 gap Track 2d: a pollinating bee keeps its pitch (`BeeLookControl`'s
/// `resetXRotOnTick`); the warden's `LOOK_TARGET` turns its head through the
/// sink at the default speeds.
#[test]
fn bees_and_wardens_keep_their_own_look_rules() {
    let mut b = mob(MobKind::Bee);
    b.look_ready();
    b.look.pitch = 30.0;
    b.look_tick();
    assert_eq!(b.look.pitch, 0.0, "not pollinating: level each tick");
    b.bee.as_mut().unwrap().flower = Some([0, 63, 0]);
    b.look.pitch = 30.0;
    b.look_tick();
    assert_eq!(b.look.pitch, 30.0, "pollinating: held");
    let w = Grid::flat(64);
    let none = ctx(&[]);
    let mut rng = Rng::new(3);
    let mut wd = mob(MobKind::Warden);
    wd.look_ready();
    let body = wd.body_yrot();
    let side = super::super::spear::view_vector(0.0, body + 90.0);
    wd.head.target = Some(Tracked::Point(wd.look_eye() + side * 5.0));
    wd.head_goals(&w, &none, &mut rng, false);
    wd.look_tick();
    assert!((rel(&wd) - 10.0).abs() < 1e-3, "{}", rel(&wd));
}
