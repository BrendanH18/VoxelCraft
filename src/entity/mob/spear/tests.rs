//! v1.0 gap Track 2b: golden spears on piglins and zombified piglins, the
//! piglin's spear behaviours, mob stabs on mobs, knockback-only and
//! dismount contacts, and the look's turn limits, against Java 26.3.
use glam::DVec3;

use super::*;
use crate::entity::mob::chase::SpearCharge;
use crate::entity::mounts::MOB_VEHICLE;
use crate::entity::nether::Weapon;
use crate::entity::{Ctx, Entities, EntityEvent, PlayerId, Target};
use crate::item::Tier;
use crate::physics::test_util::Grid;
use crate::simulation::lighting::SpawnLight;
use crate::world::terrain::Dimension;

fn ctx(players: Vec<Target>, dimension: Dimension) -> Ctx {
    Ctx { players, daylight: 0.0, spawn_light: SpawnLight::NIGHT, spawning: false, raining: false, dimension }
}

fn iron_spear() -> crate::inventory::Stack {
    crate::inventory::Stack::new(crate::weapons::spear(Tier::Iron), 1)
}

/// A zombie at the origin facing +X with an iron spear lowered past its
/// 12-tick delay, charging at 4 blocks a second, its look on `at`.
fn charging_zombie(e: &mut Entities, at: DVec3) -> usize {
    let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5), 0.0);
    m.persistent = true;
    m.equipment.set(crate::entity::equipment::Slot::MainHand, Some(iron_spear()));
    m.vel = DVec3::new(4.0, 0.0, 0.0);
    m.spear_use = Some(SpearCharge { used: 12, stabbed: Vec::new() });
    m.set_look_at(at, SPEAR_GOAL_TURN, SPEAR_GOAL_TURN);
    m.look_tick();
    e.mobs.push(m);
    e.ensure_mob_ids();
    e.mobs.len() - 1
}

/// `Piglin.createSpawnWeapon`: half crossbows, and of the rest one in ten
/// golden spears; `ZombifiedPiglin.populateDefaultEquipmentSlots`: one
/// golden spear in twenty.
#[test]
fn piglins_and_zombified_piglins_roll_golden_spears_at_javas_odds() {
    let mut e = Entities::new(4);
    let (mut adults, mut crossbows, mut spears) = (0, 0, 0);
    for i in 0..4000 {
        e.spawn(MobKind::Piglin, DVec3::new(f64::from(i % 64) + 0.5, 0.0, f64::from(i / 64) + 0.5));
        let m = e.mobs.pop().unwrap();
        if m.baby {
            assert_eq!(m.nether.as_ref().unwrap().weapon, Weapon::None, "babies carry nothing");
            continue;
        }
        adults += 1;
        match m.nether.as_ref().unwrap().weapon {
            Weapon::Crossbow => crossbows += 1,
            Weapon::GoldenSpear => spears += 1,
            w => assert_eq!(w, Weapon::GoldenSword),
        }
    }
    let share = |n: i32| f64::from(n) / f64::from(adults);
    assert!((0.46..0.54).contains(&share(crossbows)), "crossbows {}", share(crossbows));
    assert!((0.035..0.065).contains(&share(spears)), "spears {}", share(spears));
    let mut spears = 0;
    for i in 0..4000 {
        e.spawn(MobKind::ZombifiedPiglin, DVec3::new(f64::from(i % 64) + 0.5, 0.0, f64::from(i / 64) + 0.5));
        let m = e.mobs.pop().unwrap();
        match m.held {
            Some(Weapon::GoldenSpear) => spears += 1,
            held => assert_eq!(held, Some(Weapon::GoldenSword)),
        }
    }
    assert!((0.035..0.065).contains(&(f64::from(spears) / 4000.0)), "zombified spears {spears}");
}

/// The saved `nether::Weapon` gains `golden_spear` at the end: a piglin's
/// and a zombified piglin's spear survive a save, and the names saves
/// already hold still load as before.
#[test]
fn golden_spears_save_and_reload() {
    let mut e = Entities::new(4);
    e.mobs.push(Mob::new(MobKind::Piglin, DVec3::new(0.5, 10.0, 0.5), 0.0));
    e.mobs[0].nether.as_mut().unwrap().weapon = Weapon::GoldenSpear;
    e.mobs[0].persistent = true;
    e.mobs.push(Mob::new(MobKind::ZombifiedPiglin, DVec3::new(3.5, 10.0, 0.5), 0.0));
    e.mobs[1].held = Some(Weapon::GoldenSpear);
    e.mobs[1].persistent = true;
    let text = e.nether_mobs_to_string();
    assert!(text.contains("golden_spear"));
    let mut back = Entities::new(4);
    back.load_nether_mobs(&text);
    let piglin = back.mobs.iter().find(|m| m.kind == MobKind::Piglin).unwrap();
    assert_eq!(piglin.nether.as_ref().unwrap().weapon, Weapon::GoldenSpear);
    assert_eq!(piglin.holds_spear(), Some(Tier::Gold));
    let zombified = back.mobs.iter().find(|m| m.kind == MobKind::ZombifiedPiglin).unwrap();
    assert_eq!(zombified.held, Some(Weapon::GoldenSpear));
    assert_eq!(zombified.holds_spear(), Some(Tier::Gold));
    assert_eq!(zombified.melee_attack().0, 5.0, "a golden spear adds no attack damage");
    for w in [Weapon::None, Weapon::GoldenSword, Weapon::Crossbow, Weapon::GoldenAxe, Weapon::GoldenSpear] {
        assert_eq!(Weapon::from_name(w.name()), w);
    }
}

/// `Mob.dropCustomDeathLoot`: a zombified piglin's main hand drops on a
/// player kill with the default 0.085 chance, randomly worn; never
/// otherwise.
#[test]
fn zombified_piglins_drop_their_weapon_by_the_default_chance() {
    let mut e = Entities::new(4);
    let pos = DVec3::new(0.5, 10.0, 0.5);
    let spear = crate::weapons::spear(Tier::Gold);
    for player_kill in [true, false] {
        e.items.clear();
        for _ in 0..4000 {
            let mut m = Mob::new(MobKind::ZombifiedPiglin, pos, 0.0);
            m.held = Some(Weapon::GoldenSpear);
            m.health = 0.0;
            m.dying = Some(0.0);
            e.mobs.push(m);
            e.mob_equipment_drops(MobKind::ZombifiedPiglin, pos, 0, player_kill);
            e.mobs.pop();
        }
        let drops: Vec<_> = e.items.iter().filter(|i| i.stack.item == spear).collect();
        if player_kill {
            let share = drops.len() as f64 / 4000.0;
            assert!((0.065..0.105).contains(&share), "{share}");
            assert!(drops.iter().any(|i| i.stack.damage > 0), "dropped worn");
        } else {
            assert!(drops.is_empty());
        }
    }
}

/// `PiglinAi.initFightActivity`: a piglin with a golden spear approaches to
/// 10 blocks (`SpearApproach`), lowers the spear for `computeDamageUseDuration`
/// = 14 + 275 ticks and charges, stabbing the player (`SpearAttack`), then
/// raises it and retreats 9–11 blocks for up to 100 ticks (`SpearRetreat`).
/// It never swings it in melee.
#[test]
fn piglins_fight_with_golden_spears() {
    let world = Grid::flat(0);
    let mut e = Entities::new(4);
    e.mobs.push(Mob::new(MobKind::Piglin, DVec3::new(0.5, 0.0, 0.5), 0.0));
    e.mobs[0].persistent = true;
    e.mobs[0].nether.as_mut().unwrap().weapon = Weapon::GoldenSpear;
    let player = DVec3::new(14.5, 0.0, 0.5);
    let c = ctx(vec![Target::new(PlayerId(1), player, true)], Dimension::Nether);
    let mut seen = Vec::new();
    let mut lowered_at = None;
    let mut lowered_ticks = 0;
    let mut hits = Vec::new();
    for tick in 0..900 {
        for ev in e.update(0.05, &world, &c) {
            if let EntityEvent::PlayerHit { damage, .. } = ev {
                hits.push((tick, damage));
            }
        }
        let m = &e.mobs[0];
        let s = m.nether.as_ref().unwrap().spear;
        if seen.last() != Some(&s.status) {
            seen.push(s.status);
        }
        if m.spear_use.is_some() {
            lowered_ticks += 1;
            lowered_at.get_or_insert(m.pos.distance(player));
        }
    }
    let at = lowered_at.expect("the spear was lowered");
    assert!(at <= PIGLIN_APPROACH + 0.5, "lowered within 10 blocks: {at}");
    let s = Some;
    let order = [s(SpearStatus::Approach), s(SpearStatus::Charging), s(SpearStatus::Retreat)];
    let starts = seen.windows(3).position(|w| w == order);
    assert!(starts.is_some(), "approach, charge, retreat: {seen:?}");
    let gold = spear::kinetic(Tier::Gold);
    assert_eq!(gold.damage_use_duration(), 289);
    assert!(lowered_ticks >= 289, "held lowered for the whole charge: {lowered_ticks}");
    assert!(!hits.is_empty(), "the charge stabs the player");
    // The piglin's base 5 plus `floor(relative speed × 0.7)`; no 5 + 3
    // golden-sword swing can be told apart, but no hit lands outside a charge.
    assert!(hits.iter().all(|&(_, d)| d >= 5.0), "{hits:?}");
}

/// A zombified piglin's golden spear is a `KineticWeapon`: once angry it
/// runs `SpearUseGoal` and lowers it.
#[test]
fn zombified_piglins_charge_with_golden_spears() {
    let world = Grid::flat(0);
    let mut e = Entities::new(4);
    e.mobs.push(Mob::new(MobKind::ZombifiedPiglin, DVec3::new(0.5, 0.0, 0.5), 0.0));
    e.mobs[0].persistent = true;
    e.mobs[0].held = Some(Weapon::GoldenSpear);
    e.mobs[0].anger(30.0);
    let c = ctx(vec![Target::new(PlayerId(1), DVec3::new(12.5, 0.0, 0.5), true)], Dimension::Nether);
    let lowered = (0..300).any(|_| {
        e.update(0.05, &world, &c);
        e.mobs[0].spear_use.is_some()
    });
    assert!(lowered, "the spear goal lowered the golden spear");
}

/// `KineticWeapon.damageEntities` for a mob: the ray stabs the mob in the
/// way (a skeleton, for the zombie's base 3 plus `floor(4 × 0.95)`), never the spear's own
/// mount, and the spear doesn't wear (`LivingEntity.stabAttack` never calls
/// `postHurtEnemy`).
#[test]
fn mob_spears_stab_mobs_in_the_way_but_not_their_mount() {
    let world = Grid::flat(0);
    let c = ctx(Vec::new(), Dimension::Overworld);
    let mut e = Entities::new(4);
    let victim_at = DVec3::new(2.3, 0.0, 0.5);
    let z = charging_zombie(&mut e, victim_at + DVec3::Y * 1.6);
    e.mobs.push(Mob::new(MobKind::Skeleton, victim_at, 0.0));
    e.mobs[1].persistent = true;
    e.ensure_mob_ids();
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    let stab = events.iter().find_map(|ev| match ev {
        EntityEvent::MobStab(s) => Some(*s),
        _ => None,
    });
    let stab = stab.expect("the skeleton in the way is stabbed");
    assert_eq!((stab.target, stab.damage, stab.knockback), (e.mobs[1].uid, Some(6.0), true));
    e.resolve_mob_hits(&mut events);
    assert_eq!(e.mobs[1].health, 14.0);
    assert_eq!(e.mobs[z].main_hand().unwrap().damage, 0, "a mob's stab doesn't wear the spear");
    // Within the 10-tick contact cooldown nothing more lands.
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::MobStab(_))));

    // A rider's own mount in front of it is never stabbed (`isPassengerOfSameVehicle`).
    let mut e = Entities::new(4);
    let z = charging_zombie(&mut e, victim_at + DVec3::Y * 1.6);
    e.mobs.push(Mob::new(MobKind::ZombieHorse, victim_at, 0.0));
    e.ensure_mob_ids();
    e.mobs[z].riding = Some(MOB_VEHICLE | e.mobs[1].uid);
    e.mobs[1].vel = DVec3::new(4.0, 0.0, 0.0);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::MobStab(_))), "{events:?}");
}

/// A contact whose relative speed is too low to hurt but whose attacker is
/// fast enough to knock back and dismount: no damage, the 0.4 push along
/// the mob's facing, and the target off its vehicle (a player gets
/// `PlayerDismount` and `PlayerShove`; a mob is unseated and pushed).
#[test]
fn knockback_only_and_dismount_contacts_apply() {
    let world = Grid::flat(0);
    let mut e = Entities::new(4);
    let at = DVec3::new(2.3, 0.0, 0.5);
    let mut rider = Target::new(PlayerId(1), at, true);
    rider.passenger = true;
    rider.vel = DVec3::new(4.0, 0.0, 0.0);
    let c = ctx(vec![rider], Dimension::Overworld);
    charging_zombie(&mut e, at + DVec3::Y * 1.6);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert!(events.iter().any(|ev| matches!(ev, EntityEvent::PlayerDismount { player: PlayerId(1) })));
    assert!(
        events.iter().any(|ev| matches!(ev, EntityEvent::PlayerShove { player: PlayerId(1), pushes: [Some(_), None] }))
    );
    assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { .. })), "no damage at no relative speed");

    // A skeleton on a spider moving with the charge.
    let c = ctx(Vec::new(), Dimension::Overworld);
    let mut e = Entities::new(4);
    charging_zombie(&mut e, at + DVec3::Y * 1.6);
    e.mobs.push(Mob::new(MobKind::Spider, at, 0.0));
    e.mobs.push(Mob::new(MobKind::Skeleton, at + DVec3::Y * 0.6, 0.0));
    e.ensure_mob_ids();
    e.mobs[1].vel = DVec3::new(4.0, 0.0, 0.0);
    e.mobs[2].riding = Some(MOB_VEHICLE | e.mobs[1].uid);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    let stab = events.iter().find_map(|ev| match ev {
        EntityEvent::MobStab(s) => Some(*s),
        _ => None,
    });
    let stab = stab.expect("the skeleton is struck");
    assert_eq!((stab.target, stab.damage, stab.knockback, stab.dismount), (e.mobs[2].uid, None, true, true));
    let health = e.mobs[2].health;
    e.resolve_mob_hits(&mut events);
    assert_eq!(e.mobs[2].riding, None, "unseated");
    assert_eq!(e.mobs[2].health, health, "unhurt");
    assert!(e.mobs[2].vel.x > 1.0, "pushed along the zombie's facing: {:?}", e.mobs[2].vel);
    assert!(e.mobs[0].vel.x < 4.0 * 0.61, "the attacker slows (`causeExtraKnockback`)");
}

/// `LookControl.tick`: the head turns toward the wanted point by the
/// requested yaw limit while the request lasts (two ticks), then back
/// toward the body by 10°; the pitch restarts level each tick and turns by
/// at most its limit; the stab follows the head.
#[test]
fn the_spear_look_turns_by_javas_limits() {
    let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 0.0, 0.5), 0.0);
    assert_eq!(m.yrot(), -90.0, "facing +X");
    // Aiming (the look stays Java's while a spear is lowered).
    m.spear_use = Some(SpearCharge::default());
    let behind_above = DVec3::new(-10.5, 30.0, 0.5);
    m.set_look_at(behind_above, SPEAR_GOAL_TURN, SPEAR_GOAL_TURN);
    m.look_tick();
    assert!((m.look.head - -120.0).abs() < 1e-3, "{}", m.look.head);
    assert!((m.look.pitch - -30.0).abs() < 1e-3, "pitch held to 30°: {}", m.look.pitch);
    m.look_tick();
    assert!((m.look.head - -150.0).abs() < 1e-3, "{}", m.look.head);
    m.look_tick();
    assert!((m.look.head - -140.0).abs() < 1e-3, "back toward the body: {}", m.look.head);
    assert_eq!(m.look.pitch, 0.0, "level once the request ends");
    // The piglin's `LookAtTargetSink` asks for 10° a tick and 40° of pitch.
    let mut p = Mob::new(MobKind::Piglin, DVec3::new(0.5, 0.0, 0.5), 0.0);
    p.set_look_at(behind_above, HEAD_ROT_SPEED, MAX_HEAD_X_ROT);
    p.look_tick();
    assert!((p.look.head - -100.0).abs() < 1e-3, "{}", p.look.head);
    assert!((p.look.pitch - -40.0).abs() < 1e-3, "{}", p.look.pitch);
    // `calculateViewVector`: yaw -90 faces +X, 0 faces +Z, pitch -90 up.
    assert!(view_vector(0.0, -90.0).abs_diff_eq(DVec3::X, 1e-6));
    assert!(view_vector(0.0, 0.0).abs_diff_eq(DVec3::Z, 1e-6));
    assert!(view_vector(-90.0, 0.0).abs_diff_eq(DVec3::Y, 1e-6));
}

/// A stab follows the head: a zombie whose head hasn't turned to a player
/// beside it doesn't stab them.
#[test]
fn the_stab_follows_the_head_not_the_target() {
    let world = Grid::flat(0);
    let mut e = Entities::new(4);
    // Looking ahead (+X) while the player stands to the side (+Z).
    charging_zombie(&mut e, DVec3::new(10.5, 1.6, 0.5));
    let side = Target::new(PlayerId(1), DVec3::new(0.5, 0.0, 2.3), true);
    let c = ctx(vec![side], Dimension::Overworld);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert!(events.is_empty(), "{events:?}");
}

/// v1.0 gap Track 2c: `PiercingWeapon.canHitEntity` takes every pickable
/// entity: a minecart in the way takes ten times the stab's 6 (past 40 it
/// breaks into its item), an End crystal is set off, and the mob's own
/// cart is never stabbed (`isPassengerOfSameVehicle`).
#[test]
fn mob_spears_stab_carts_and_crystals_but_not_their_own_cart() {
    use crate::entity::minecart::CartKind;
    let world = Grid::flat(0);
    let c = ctx(Vec::new(), Dimension::Overworld);
    let at = DVec3::new(2.3, 0.0, 0.5);
    let mut e = Entities::new(4);
    charging_zombie(&mut e, at + DVec3::Y * 0.4);
    e.spawn_cart(CartKind::Rideable, at);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert!(e.minecarts.is_empty(), "60 damage breaks the cart");
    assert!(e.items.iter().any(|i| i.stack.item == crate::item::Item::MINECART));

    let mut e = Entities::new(4);
    charging_zombie(&mut e, at + DVec3::Y * 1.0);
    e.crystals.spawn(at, None, false, false);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert!(e.crystals.hits_pending(), "the crystal is set off");

    // Riding a cart, its own cart ahead isn't stabbed.
    let mut e = Entities::new(4);
    let z = charging_zombie(&mut e, at + DVec3::Y * 0.4);
    let id = e.spawn_cart(CartKind::Rideable, at);
    e.mobs[z].riding = Some(id);
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    assert_eq!(e.minecarts.len(), 1);
    assert!(e.items.is_empty());
}

/// v1.0 gap Track 2c: `getManyEntityHitResult`'s margin. A ray passing
/// 0.1 beside a box (inside its 0.125 margin) hits it when nothing stands
/// between the margin and the box's centre, and misses when a block does;
/// a ray starting inside the margin but outside the box misses.
#[test]
fn the_margin_hit_checks_blocks_toward_the_box() {
    use crate::weapons::spear::stab_hits;
    let mut world = Grid::flat(-10);
    let (min, max) = (DVec3::new(3.0, 0.0, 0.0), DVec3::new(4.0, 1.0, 1.0));
    // Along +X at z = -0.1, just outside the box's z = 0 face.
    let (from, span) = (DVec3::new(0.5, 0.5, -0.1), DVec3::new(5.0, 0.0, 0.0));
    assert!(stab_hits(&world, from, span, min, max, 0.125));
    assert!(!stab_hits(&world, from, span, min, max, 0.0), "no margin, no hit");
    // Farther than the margin beside it: a miss.
    assert!(!stab_hits(&world, DVec3::new(0.5, 0.5, -0.2), span, min, max, 0.125));
    // A block in the corner between the margin crossing (2.875, 0.5, -0.1)
    // and the box's centre stops the clip short of the box: a miss, though
    // the ray itself passes clear of it.
    world.set(glam::IVec3::new(2, 0, 0), crate::world::block::Block::STONE);
    assert!(!stab_hits(&world, from, span, min, max, 0.125), "a block toward the centre");
    // Starting within the margin (but outside the box): `AABB.clip` finds no entry.
    assert!(!stab_hits(&Grid::flat(-10), DVec3::new(2.95, 0.5, 0.5), DVec3::new(-1.0, 0.0, 0.0), min, max, 0.125));
}

/// v1.0 gap Track 2d: a mob's spear reaches the dragon's parts
/// (`EnderDragonPart` is pickable). The stab lands and is remembered for
/// the contact cooldown, but takes no health: `EnderDragon.hurt` hurts only
/// for players and explosions.
#[test]
fn mob_spears_reach_dragon_parts_and_do_nothing() {
    use crate::entity::dragon::Hit;
    let world = Grid::flat(-64);
    let c = ctx(Vec::new(), Dimension::Overworld);
    let mut e = Entities::new(4);
    let id = e.wild_dragons.spawn(DVec3::new(0.5, 20.0, 0.5), 0.0);
    let (_, min, max) = e.wild_dragons.dragons[0].dragon.parts()[0];
    let centre = (min + max) * 0.5;
    let mut m = Mob::new(MobKind::Zombie, centre - DVec3::new(2.0, 1.74, 0.0), 0.0);
    m.persistent = true;
    m.equipment.set(crate::entity::equipment::Slot::MainHand, Some(iron_spear()));
    m.vel = DVec3::new(4.0, 0.0, 0.0);
    m.spear_use = Some(SpearCharge { used: 12, stabbed: Vec::new() });
    m.set_look_at(centre, SPEAR_GOAL_TURN, SPEAR_GOAL_TURN);
    m.look_tick();
    e.mobs.push(m);
    e.ensure_mob_ids();
    let health = e.wild_dragons.dragons[0].dragon.health;
    let mut events = Vec::new();
    e.mob_spear_stabs(&world, &c, &mut events);
    let charge = e.mobs[0].spear_use.as_ref().unwrap();
    assert!(
        charge.stabbed.iter().any(|s| matches!(s.0, Stabbed::Dragon(Hit::Wild(d, _)) if d == id)),
        "{:?}",
        charge.stabbed
    );
    assert_eq!(e.wild_dragons.dragons[0].dragon.health, health, "a mob's stab takes no health");
}
