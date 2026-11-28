//! v1.0 gap Track 4c: what each projectile does to the Ender Dragon and
//! End crystals (Java 26.3's `EnderDragonPart.hurtServer`,
//! `EnderDragon.hurt`, `AbstractDragonSittingPhase.onHurt` and each
//! projectile's `onHit`), explosions and wind pushing it, and
//! `checkWalls` under `mobGriefing`.

use glam::DVec3;

use super::{Blow, Dragon, MAX_HEALTH, Part, Phase};
use crate::entity::{Ctx, Entities, EntityEvent, PlayerId, Rng};
use crate::physics::test_util::Grid;
use crate::world::block::Block;

fn ctx() -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players: Vec::new(),
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn flying() -> Dragon {
    let mut d = Dragon::new(DVec3::new(0.5, 40.0, 0.5));
    d.phase = Phase::HoldingPattern;
    d
}

/// Lands `blow` on a fresh window; returns (landed, health lost).
fn strike(d: &mut Dragon, part: Part, blow: Blow) -> (bool, f32) {
    d.immune = (0, 0.0);
    let before = d.health;
    let landed = d.hurt(part, blow);
    (landed, before - d.health)
}

#[test]
fn each_blow_lands_as_javas_ender_dragon_hurt_rules() {
    let mut d = flying();
    // A player's hit: the head and (26.3) the neck take it all, the rest
    // a quarter plus one.
    assert_eq!(strike(&mut d, Part::Head, Blow::melee(8.0)), (true, 8.0));
    assert_eq!(strike(&mut d, Part::Neck, Blow::melee(8.0)), (true, 8.0));
    assert_eq!(strike(&mut d, Part::Body, Blow::melee(8.0)), (true, 3.0));
    // A snowball's or egg's `hurt(thrown, 0)`: refused, under 0.01.
    assert_eq!(strike(&mut d, Part::Body, Blow::projectile(0.0, true)), (false, 0.0));
    // A mob's projectile (a shulker's bullet, a skeleton's arrow) lands
    // and takes nothing; an explosion's always does.
    assert_eq!(strike(&mut d, Part::Body, Blow::projectile(4.0, false)), (true, 0.0));
    assert_eq!(strike(&mut d, Part::Wing(0), Blow::arrow(9.0, false)), (true, 0.0));
    assert_eq!(strike(&mut d, Part::Body, Blow::explosion(10.0)), (true, 3.5));
    // Wind charges: a player's does one point (a quarter plus one on the
    // body), a breeze's nothing.
    assert_eq!(strike(&mut d, Part::Body, Blow::wind_charge(false, true)), (true, 1.25));
    assert_eq!(strike(&mut d, Part::Head, Blow::wind_charge(false, true)), (true, 1.0));
    assert_eq!(strike(&mut d, Part::Body, Blow::wind_charge(true, false)), (true, 0.0));
    // Fireballs: fire immune.
    assert_eq!(strike(&mut d, Part::Head, Blow::fire(6.0, true)), (false, 0.0));
    // A firework's blast, whoever launched it (`#is_explosion`).
    assert_eq!(strike(&mut d, Part::Body, Blow::explosion(7.0)), (true, 2.75));

    // Perched, arrows, tridents and a player's wind charges do nothing and
    // bounce; a breeze's charge and anything else lands as before.
    for phase in [Phase::SittingScanning, Phase::SittingFlaming, Phase::SittingAttacking] {
        let mut d = flying();
        d.phase = phase;
        assert_eq!(strike(&mut d, Part::Head, Blow::arrow(9.0, true)), (false, 0.0));
        assert_eq!(strike(&mut d, Part::Head, Blow::wind_charge(false, true)), (false, 0.0));
        assert_eq!(strike(&mut d, Part::Head, Blow::wind_charge(true, false)), (true, 0.0));
        assert_eq!(strike(&mut d, Part::Head, Blow::melee(9.0)), (true, 9.0));
    }
    // Hovering counts as sitting but keeps the default `onHurt`.
    let mut d = flying();
    d.phase = Phase::Hovering;
    assert_eq!(strike(&mut d, Part::Head, Blow::arrow(9.0, true)), (true, 9.0));
    // Dying, nothing lands.
    let mut d = flying();
    d.hurt_by(Part::Head, 500.0, true);
    assert_eq!(d.phase, Phase::Dying);
    for blow in [Blow::melee(9.0), Blow::explosion(9.0), Blow::arrow(9.0, true)] {
        assert_eq!(strike(&mut d, Part::Head, blow), (false, 0.0));
    }
}

/// A hovering dragon no fight owns at `at`, in a world of air over a floor.
fn hovering(at: DVec3) -> (Entities, Grid) {
    let mut e = Entities::new(1);
    e.wild_dragons.spawn(at, 0.0);
    (e, Grid::flat(10))
}

fn health(e: &Entities) -> f32 {
    e.wild_dragons.dragons[0].dragon.health
}

/// The centre of the dragon's body as it stands.
fn body(e: &Entities) -> DVec3 {
    let (_, min, max) = e.wild_dragons.dragons[0].dragon.parts.iter().find(|p| p.0 == Part::Body).copied().unwrap();
    (min + max) / 2.0
}

fn run(e: &mut Entities, world: &Grid, ticks: u32) -> Vec<EntityEvent> {
    (0..ticks).flat_map(|_| e.update(0.05, world, &ctx())).collect()
}

#[test]
fn thrown_things_strike_the_dragons_parts() {
    use crate::entity::thrown::{Kind, Thrown};
    let at = DVec3::new(0.5, 40.0, 0.5);
    let mut rng = Rng::new(3);
    // A snowball and an egg stop on the dragon and do nothing to it.
    for kind in [Kind::Snowball, Kind::Egg] {
        let (mut e, world) = hovering(at);
        run(&mut e, &world, 1);
        let aim = body(&e);
        let from = aim - DVec3::new(8.0, -0.3, 0.0);
        e.thrown.push(Thrown::launch(kind, PlayerId::HOST, from, DVec3::X, DVec3::ZERO, &mut rng));
        run(&mut e, &world, 8);
        assert!(e.thrown.is_empty(), "{kind:?} stops on the dragon");
        assert_eq!(health(&e), MAX_HEALTH, "{kind:?}");
    }
    // A player's wind charge takes a point (a quarter plus one on the body)
    // and bursts; a breeze's bursts and takes nothing.
    for (breeze, lost) in [(false, 1.25), (true, 0.0)] {
        let (mut e, world) = hovering(at);
        run(&mut e, &world, 1);
        let from = body(&e) - DVec3::new(8.0, 0.0, 0.0);
        let charge = if breeze {
            Thrown::breeze_charge(from, DVec3::X, 0.0, 99, &mut rng)
        } else {
            Thrown::launch(Kind::WindCharge, PlayerId::HOST, from, DVec3::X, DVec3::ZERO, &mut rng)
        };
        e.thrown.push(charge);
        let events = run(&mut e, &world, 12);
        assert!(e.thrown.is_empty());
        assert!(events.iter().any(|ev| matches!(ev, EntityEvent::WindBurst(_))), "breeze {breeze}");
        assert_eq!(MAX_HEALTH - health(&e), lost, "breeze {breeze}");
    }
}

#[test]
fn a_pearl_on_the_dragon_takes_its_thrower_to_where_it_was() {
    use crate::entity::pearl::Pearl;
    let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
    run(&mut e, &world, 1);
    let aim = body(&e);
    let mut rng = Rng::new(4);
    e.pearls.push(Pearl::thrown(PlayerId(7), aim - DVec3::new(8.0, -0.3, 0.0), DVec3::X, DVec3::ZERO, &mut rng));
    let events = run(&mut e, &world, 8);
    let landed = events.iter().find_map(|ev| match *ev {
        EntityEvent::PearlLanded { owner, pos } => Some((owner, pos)),
        _ => None,
    });
    let (owner, pos) = landed.expect("the pearl lands on the dragon");
    assert_eq!(owner, PlayerId(7));
    // `oldPosition()`: outside the body, short of it.
    assert!(pos.x < aim.x - 2.5 && pos.x > aim.x - 8.0, "{pos}");
    assert_eq!(health(&e), MAX_HEALTH);
}

#[test]
fn harming_reaches_the_dragon_and_healing_mends_it() {
    use crate::entity::lingering::Cloud;
    use crate::entity::potion::ThrownPotion;
    use crate::potion::Potion;
    let harming = Potion::from_id("harming").unwrap();
    let healing = Potion::from_id("healing").unwrap();
    // A player's splash of Harming at the body: 6, through the body.
    let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
    run(&mut e, &world, 1);
    let from = body(&e) - DVec3::new(6.0, -1.0, 0.0);
    e.potions.push(ThrownPotion::new(harming, Some(PlayerId::HOST), from, DVec3::X * 20.0));
    let events = run(&mut e, &world, 10);
    assert!(e.potions.is_empty());
    assert!(events.iter().any(|ev| matches!(ev, EntityEvent::PotionSplashed { .. })));
    assert_eq!(MAX_HEALTH - health(&e), 6.0 / 4.0 + 1.0);
    // A witch's does nothing to it; Healing mends it.
    e.wild_dragons.dragons[0].dragon.immune = (0, 0.0);
    e.wild_dragons.dragons[0].dragon.health = 150.0;
    let from = body(&e) - DVec3::new(6.0, -1.0, 0.0);
    e.potions.push(ThrownPotion::new(harming, None, from, DVec3::X * 20.0));
    run(&mut e, &world, 10);
    assert_eq!(health(&e), 150.0);
    let from = body(&e) - DVec3::new(6.0, -1.0, 0.0);
    e.potions.push(ThrownPotion::new(healing, None, from, DVec3::X * 20.0));
    run(&mut e, &world, 10);
    assert_eq!(health(&e), 154.0);
    // A player's lingering Harming at its feet: three a dose.
    let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
    run(&mut e, &world, 1);
    let feet = e.wild_dragons.dragons[0].dragon.pos;
    e.clouds.push(Cloud::new(feet, harming, Some(PlayerId::HOST)));
    run(&mut e, &world, 12);
    assert_eq!(MAX_HEALTH - health(&e), 3.0 / 4.0 + 1.0);
}

#[test]
fn thrown_things_break_end_crystals_but_wind_charges_and_bottles_pass_or_burst() {
    use crate::entity::thrown::{Kind, Thrown};
    let world = Grid::flat(10);
    let mut rng = Rng::new(5);
    for (kind, breaks) in [(Kind::Snowball, true), (Kind::Egg, true), (Kind::ExperienceBottle, false)] {
        let mut e = Entities::new(1);
        let id = e.crystals.place(DVec3::new(0.5, 11.0, 0.5));
        let from = DVec3::new(-6.0, 12.1, 0.5);
        let mut t = Thrown::launch(kind, PlayerId::HOST, from, DVec3::X, DVec3::ZERO, &mut rng);
        if kind == Kind::ExperienceBottle {
            // Flat at the crystal, not lobbed over it.
            t = Thrown::dispensed(kind, from, DVec3::X, 1.5, 0.0, &mut rng);
        }
        e.thrown.push(t);
        run(&mut e, &world, 10);
        assert!(e.thrown.is_empty(), "{kind:?}");
        assert_eq!(e.crystals.get(id).is_none(), breaks, "{kind:?}");
    }
    // `AbstractWindCharge.canHitEntity` passes crystals by.
    let mut e = Entities::new(1);
    let id = e.crystals.place(DVec3::new(0.5, 11.0, 0.5));
    e.thrown.push(Thrown::launch(
        Kind::WindCharge,
        PlayerId::HOST,
        DVec3::new(-6.0, 12.1, 0.5),
        DVec3::X,
        DVec3::ZERO,
        &mut rng,
    ));
    run(&mut e, &world, 3);
    assert!(e.crystals.get(id).is_some());
}

#[test]
fn wind_and_blasts_push_the_dragon() {
    let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
    let feet = e.wild_dragons.dragons[0].dragon.pos;
    let burst = crate::entity::wind::WindBurst::charge(feet + DVec3::new(1.0, 0.0, 0.0), Some(PlayerId::HOST));
    e.wind_blast(&world, &burst);
    let v = e.wild_dragons.dragons[0].dragon.vel;
    // Away from the burst toward its eye (6.8 up): (1 - 1/2.4) * 1.22.
    let push = (1.0 - 1.0 / f64::from(2.4f32)) * f64::from(1.22f32);
    let dir = DVec3::new(-1.0, 6.8, 0.0).normalize();
    assert!((v - dir * push).length() < 1e-9, "{v}");
    // Out of reach: nothing.
    let far = crate::entity::wind::WindBurst::charge(feet + DVec3::new(3.0, 0.0, 0.0), None);
    e.wind_blast(&world, &far);
    assert!((e.wild_dragons.dragons[0].dragon.vel - v).length() < 1e-12);
    // A blast measures every part and the dragon from its feet: under the
    // body, the dragon itself takes the most, through its body.
    let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
    let feet = e.wild_dragons.dragons[0].dragon.pos;
    e.wild_dragons.explode(&world, feet - DVec3::Y, 4.0);
    let d = &e.wild_dragons.dragons[0].dragon;
    assert!(d.health < MAX_HEALTH);
    assert!(d.vel.y > 0.0, "pushed up: {}", d.vel);
}

/// `checkWalls` with and without `mobGriefing`.
#[test]
fn with_mob_griefing_off_every_block_in_its_way_is_a_wall() {
    let at = DVec3::new(0.5, 40.0, 0.5);
    for (block, griefing, breaks, wall) in [
        (Block::STONE, true, true, false),
        (Block::STONE, false, false, true),
        (Block::END_STONE, true, false, true),
        (Block::OBSIDIAN, true, false, true),
        (Block::WATER, true, false, false),
        (Block::WATER, false, false, true),
        (Block::FIRE, false, false, false),
        (Block::NETHER_PORTAL, true, true, false),
    ] {
        let mut world = Grid::flat(0);
        let mut d = Dragon::new(at);
        // A block in the middle of the body.
        let (part, min, max) = d.parts[0];
        assert_eq!(part, Part::Body);
        let cell = ((min + max) / 2.0).floor().as_ivec3();
        world.set(cell, block);
        let mut events = Vec::new();
        d.check_walls(&world, griefing, &mut events);
        let broke = events.iter().any(|e| matches!(e, EntityEvent::BreakBlock { cell: c } if *c == cell));
        assert_eq!((broke, d.in_wall), (breaks, wall), "{} griefing {griefing}", block.name());
    }
}

/// `flapTime`: 0.1 a tick perched; in flight `0.2 / (10 v + 1) * 2^vy`,
/// halved in a wall.
#[test]
fn the_wings_beat_at_javas_rate() {
    let end = crate::world::end::EndGen::new(5);
    let mut fight = super::Fight::new(&end);
    let mut crystals = crate::entity::crystal::Crystals::default();
    let d = fight.dragon.as_mut().unwrap();
    d.phase = Phase::SittingScanning;
    let before = d.flap;
    let mut events = Vec::new();
    fight.update(super::TICK, &Grid::flat(0), &ctx(), &mut Rng::new(1), &mut crystals, &mut events);
    let d = fight.dragon.as_mut().unwrap();
    assert!((d.flap - before - 0.1).abs() < 1e-6, "{}", d.flap - before);
}

/// A ghast's fireball explodes on a dragon part (its own fire can't hurt
/// the dragon); a blaze's is simply gone.
#[test]
fn fireballs_burst_on_the_dragon() {
    use crate::entity::fireball::Fireball;
    for large in [true, false] {
        let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
        run(&mut e, &world, 1);
        let from = body(&e) - DVec3::new(6.0, 0.0, 0.0);
        e.fireballs.push(if large { Fireball::large(from, DVec3::X) } else { Fireball::new(from, DVec3::X) });
        let events = run(&mut e, &world, 30);
        assert!(e.fireballs.is_empty(), "large {large}");
        let blast = events
            .iter()
            .any(|ev| matches!(ev, EntityEvent::Explosion { center, .. } if center.distance(body(&e)) < 4.0));
        assert_eq!(blast, large);
        assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::IgniteBlock { .. })));
        assert_eq!(health(&e), MAX_HEALTH, "the fire itself does nothing");
    }
}

/// `FishingHook.onHitEntity` hooks a dragon part; reeling it in wears the
/// rod by five, as for any hooked entity.
#[test]
fn a_fishing_hook_catches_on_the_dragon() {
    use crate::entity::bobber::Bobber;
    let (mut e, world) = hovering(DVec3::new(0.5, 40.0, 0.5));
    run(&mut e, &world, 1);
    let aim = body(&e);
    let player = crate::entity::Target::new(PlayerId::HOST, aim - DVec3::new(10.0, 1.0, 0.0), true);
    let c = Ctx { players: vec![player], ..ctx() };
    let mut rng = Rng::new(6);
    e.bobbers.push(Bobber::cast(PlayerId::HOST, aim - DVec3::new(6.0, 0.5, 0.0), DVec3::X, 0, 0, &mut rng));
    for _ in 0..10 {
        e.update(0.05, &world, &c);
    }
    let b = &e.bobbers[0];
    assert!(b.pos.distance(aim) < 3.0, "riding the body: {} vs {aim}", b.pos);
    assert_eq!(b.retrieve(&mut rng, 0, false).wear, 5);
}
