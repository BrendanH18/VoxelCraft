//! Nautiluses and zombie nautiluses against `tests/fixtures/java/26.3/new_mobs.json`.
use super::*;
use crate::{
    entity::{Target, category::MobCategory},
    inventory::Inventory,
    physics::test_util::Grid,
    simulation::lighting::SpawnLight,
    world::{block::Block, terrain::Dimension},
};
use glam::IVec3;
use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/new_mobs.json")).unwrap()
}
fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}
fn names(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()
}
fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        players,
        daylight: 0.0,
        spawn_light: SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
/// A pool of water 121 blocks along X and 21 across and deep, above stone.
fn ocean() -> Grid {
    let mut g = Grid::flat(0);
    for x in -60..=60 {
        for y in 1..=21 {
            for z in -10..=10 {
                g.set(IVec3::new(x, y, z), Block::WATER);
            }
        }
    }
    g
}
fn tame(e: &mut Entities, kind: MobKind, pos: DVec3) -> usize {
    e.spawn(kind, pos);
    e.ensure_mob_ids();
    let i = e.mobs.len() - 1;
    let s = e.mobs[i].mount.as_mut().unwrap();
    s.tame = true;
    s.owner = Some(PlayerId::HOST);
    i
}

#[test]
fn registration_attributes_and_constants_match_java() {
    let f = facts();
    for (name, kind, category) in [
        ("nautilus", MobKind::Nautilus, MobCategory::WaterCreature),
        ("zombie_nautilus", MobKind::ZombieNautilus, MobCategory::Monster),
    ] {
        let t = &f["entity_types"][name];
        assert_eq!(MobKind::from_name(name), Some(kind));
        assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
        assert_eq!(kind.shape().half_width * 2.0, num(&t["width"]));
        assert_eq!(kind.shape().height, num(&t["height"]));
        assert_eq!(kind.category(), category);
        assert_eq!(t["category"], if category == MobCategory::Monster { "monster" } else { "water_creature" });
        assert_eq!(f64::from(kind.max_health()), num(&f["attributes"]["nautilus"]["max_health"]));
    }
    let a = &f["attributes"]["nautilus"];
    assert_eq!(f64::from(ATTACK_DAMAGE), num(&a["attack_damage"]));
    assert_eq!(KNOCKBACK_RESISTANCE, num(&a["knockback_resistance"]) as f32);
    assert_eq!(movement_speed(MobKind::Nautilus), num(&a["movement_speed"]));
    assert_eq!(
        movement_speed(MobKind::ZombieNautilus) as f32,
        num(&f["attributes"]["zombie_nautilus"]["movement_speed"]) as f32
    );
    let c = &f["constants"]["abstract_nautilus"];
    assert_eq!(IN_WATER_SPEED_MODIFIER as f32, num(&c["IN_WATER_SPEED_MODIFIER"]) as f32);
    assert_eq!(RIDDEN_SPEED_IN_WATER as f32, num(&c["RIDDEN_SPEED_MODIFIER_IN_WATER"]) as f32);
    assert_eq!(RIDDEN_SPEED_ON_LAND as f32, num(&c["RIDDEN_SPEED_MODIFIER_ON_LAND"]) as f32);
    assert_eq!(WATER_RESISTANCE, num(&c["NAUTILUS_WATER_RESISTANCE"]));
    assert_eq!(f64::from(DASH_COOLDOWN), num(&c["DASH_COOLDOWN_TICKS"]));
    assert_eq!(DASH_MOMENTUM_IN_WATER as f32, num(&c["DASH_MOMENTUM_IN_WATER"]) as f32);
    assert_eq!(DASH_MOMENTUM_ON_LAND as f32, num(&c["DASH_MOMENTUM_ON_LAND"]) as f32);
    assert_eq!(f64::from(EFFECT_DURATION), num(&c["EFFECT_DURATION"]));
    assert_eq!(f64::from(EFFECT_REFRESH_RATE), num(&c["EFFECT_REFRESH_RATE"]));
    assert_eq!(f64::from(SMALL_RESTRICTION_RADIUS), num(&c["SMALL_RESTRICTION_RADIUS"]));
    assert_eq!(f64::from(LARGE_RESTRICTION_RADIUS), num(&c["LARGE_RESTRICTION_RADIUS"]));
    assert_eq!(f64::from(RESTRICTION_RADIUS_BUFFER), num(&c["RESTRICTION_RADIUS_BUFFER"]));
    assert_eq!(f64::from(TOTAL_AIR), num(&f["constants"]["nautilus"]["NAUTILUS_TOTAL_AIR_SUPPLY"]));
    let ai = &f["constants"]["nautilus_ai"];
    assert_eq!(f64::from(ANGER_DURATION), num(&ai["ANGER_DURATION"]));
    assert_eq!(f64::from(TIME_BETWEEN_ATTACKS), num(&ai["TIME_BETWEEN_ATTACKS"]));
    assert_eq!(MAX_CHARGE_DISTANCE, num(&ai["MAX_CHARGE_DISTANCE"]));
    assert_eq!(MAX_TARGET_DETECTION_DISTANCE, num(&ai["MAX_TARGET_DETECTION_DISTANCE"]));
    assert_eq!(CHARGE_SPEED as f32, num(&ai["SPEED_WHEN_ATTACKING"]) as f32);
    assert_eq!(f64::from(CHARGE_KNOCKBACK), num(&ai["ATTACK_KNOCKBACK_FORCE"]));
    assert_eq!(TEMPT_SPEED as f32, num(&ai["SPEED_MULTIPLIER_WHEN_TEMPTED"]) as f32);
    assert_eq!(PANIC_SPEED as f32, num(&ai["SPEED_MULTIPLIER_WHEN_PANICKING"]) as f32);
    assert_eq!(LOVE_SPEED as f32, num(&ai["SPEED_MULTIPLIER_WHEN_MAKING_LOVE"]) as f32);
    let z = &f["constants"]["zombie_nautilus_ai"];
    assert_eq!(ZOMBIE_CHARGE_SPEED as f32, num(&z["SPEED_WHEN_ATTACKING"]) as f32);
    assert_eq!(ZOMBIE_TEMPT_SPEED as f32, num(&z["SPEED_MULTIPLIER_WHEN_TEMPTED"]) as f32);
    let d = &f["constants"]["drowned"];
    assert_eq!(
        crate::entity::camel_husk::ZOMBIE_NAUTILUS_JOCKEY_CHANCE,
        num(&d["ZOMBIE_NAUTILUS_JOCKEY_CHANCE"]) as f32
    );
}

#[test]
fn item_and_entity_tags_match_java() {
    let f = facts();
    let tags = &f["tags"]["item"];
    let all = [
        (Item::COD, "cod"),
        (Item::COOKED_COD, "cooked_cod"),
        (Item::SALMON, "salmon"),
        (Item::COOKED_SALMON, "cooked_salmon"),
        (Item::PUFFERFISH, "pufferfish"),
        (Item::TROPICAL_FISH, "tropical_fish"),
        (Item::PUFFERFISH_BUCKET, "pufferfish_bucket"),
        (Item::COD_BUCKET, "cod_bucket"),
        (Item::SALMON_BUCKET, "salmon_bucket"),
        (Item::TROPICAL_FISH_BUCKET, "tropical_fish_bucket"),
        (Item::WATER_BUCKET, "water_bucket"),
        (Item::WHEAT, "wheat"),
    ];
    let fishes = names(&tags["fishes"]);
    let bucket = names(&tags["nautilus_bucket_food"]);
    let taming = names(&tags["nautilus_taming_items"]);
    assert_eq!(names(&tags["nautilus_food"]), ["#minecraft:fishes", "#minecraft:nautilus_bucket_food"]);
    for (item, n) in all {
        let n = n.to_owned();
        assert_eq!(bucket_food(item), bucket.contains(&n), "{n}");
        assert_eq!(taming_item(item), taming.contains(&n), "{n}");
        assert_eq!(food(item), fishes.contains(&n) || bucket.contains(&n), "{n}");
    }
    let e = &f["tags"]["entity_type"];
    for kind in [MobKind::Nautilus, MobKind::ZombieNautilus] {
        assert!(names(&e["can_wear_nautilus_armor"]).contains(&kind.name().to_owned()));
        assert!(names(&e["can_equip_saddle"]).contains(&kind.name().to_owned()));
        assert!(names(&e["aquatic"]).contains(&kind.name().to_owned()));
    }
    assert_eq!(names(&e["nautilus_hostiles"]), ["pufferfish"]);
    assert!(names(&e["burn_in_daylight"]).contains(&"zombie_nautilus".to_owned()));
    assert!(MobKind::ZombieNautilus.burns_in_sun() && !MobKind::Nautilus.burns_in_sun());
    assert!(names(&e["zombies"]).contains(&"zombie_nautilus".to_owned()));
    assert_eq!(MobKind::ZombieNautilus.creature(), crate::enchant::Creature::Undead);
    assert_eq!(names(&f["tags"]["biome"]["spawns_coral_variant_zombie_nautilus"]), ["warm_ocean"]);
    assert!(!MobKind::Nautilus.accepts_potion_effect(Effect::Poison));
    assert!(MobKind::Nautilus.accepts_potion_effect(Effect::Weakness));
}

#[test]
fn loot_matches_the_tables() {
    let f = facts();
    let shell = &f["loot_tables"]["entities/nautilus"]["pools"][0];
    assert_eq!(shell["entries"][0]["name"], "minecraft:nautilus_shell");
    let chance = &shell["condition"]["terms"][1];
    assert_eq!(num(&chance["unenchanted_chance"]), 0.05);
    let flesh = &f["loot_tables"]["entities/zombie_nautilus"]["pools"][0];
    assert_eq!(flesh["condition"]["type"], "minecraft:killed_by_player");
    assert_eq!(MobKind::ZombieNautilus.loot(), [(Item::ROTTEN_FLESH, 0, 3)]);
    // A nautilus's shell needs a player; about one kill in twenty.
    let mut shells = 0;
    for seed in 0..2000 {
        let mut e = Entities::new(seed);
        e.drop_loot_with_fire(MobKind::Nautilus, DVec3::new(0., 10., 0.), 0, false, true);
        shells += e.items.iter().filter(|i| i.stack.item == Item::NAUTILUS_SHELL).count();
        let mut e = Entities::new(seed);
        e.drop_loot_with_fire(MobKind::Nautilus, DVec3::new(0., 10., 0.), 0, false, false);
        e.drop_loot_with_fire(MobKind::ZombieNautilus, DVec3::new(0., 10., 0.), 0, false, false);
        assert!(e.items.is_empty());
    }
    assert!((60..=140).contains(&shells), "{shells}");
}

#[test]
fn pufferfish_tames_one_in_three_and_buckets_come_back() {
    let mut tamed = 0;
    for seed in 0..300 {
        let mut e = Entities::new(seed);
        e.spawn(MobKind::Nautilus, DVec3::new(0.5, 5.0, 0.5));
        assert_eq!(e.use_nautilus(0, Some(Stack::new(Item::COD, 1)), PlayerId::HOST), None, "cod doesn't tame");
        match e.use_nautilus(0, Some(Stack::new(Item::PUFFERFISH_BUCKET, 1)), PlayerId(4)) {
            Some(Use::Tamed { tamed: true, bucket: true }) => {
                tamed += 1;
                let s = e.mobs[0].mount.as_ref().unwrap();
                assert!(s.tame && s.owner == Some(PlayerId(4)));
            }
            Some(Use::Tamed { tamed: false, bucket: true }) => assert!(!e.mobs[0].mount.as_ref().unwrap().tame),
            other => panic!("{other:?}"),
        }
    }
    assert!((70..=130).contains(&tamed), "{tamed}");
}

#[test]
fn tame_nautiluses_take_saddles_and_armor_and_carry_one_rider() {
    let mut e = Entities::new(2);
    let i = tame(&mut e, MobKind::Nautilus, DVec3::new(5.0, 140.0, 5.0));
    let eye = DVec3::new(5.0, 140.5, 3.0);
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::SADDLE, 1));
    inv.slots[1] = Some(Stack::new(crate::equipment::IRON_NAUTILUS_ARMOR, 1));
    inv.slots[2] = Some(Stack::new(Item::IRON_HORSE_ARMOR, 1));
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 0, false));
    assert!(e.mobs[i].mount.as_ref().unwrap().saddled());
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 1, false));
    assert_eq!(e.mobs[i].mount.as_ref().unwrap().slots[1].map(|s| s.item), Some(crate::equipment::IRON_NAUTILUS_ARMOR));
    // Horse armor doesn't fit.
    assert!(!e.mobs[i].mount.as_ref().unwrap().accepts(MobKind::Nautilus, 1, Item::IRON_HORSE_ARMOR));
    // Nautilus armor protects like the matching horse armor's `BODY` defense.
    assert_eq!(
        crate::entity::mounts::armor_points(crate::equipment::IRON_NAUTILUS_ARMOR),
        crate::entity::mounts::armor_points(Item::IRON_HORSE_ARMOR)
    );
    let before = e.mobs[i].health;
    e.mobs[i].damage(10.0, None, &mut Rng::new(1));
    assert!(before - e.mobs[i].health < 10.0);
    // An empty hand rides; a second player can't.
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 5, false));
    assert_eq!(e.mount_for_player(PlayerId::HOST), Some(crate::entity::mounts::MOB_VEHICLE | e.mobs[i].uid));
    assert!(!e.use_mount(eye, DVec3::Z, 5.0, PlayerId(2), &mut inv, 5, false));
    assert!(e.mobs[i].nautilus_controlled());
    let seat = e.mount_seat(crate::entity::mounts::MOB_VEHICLE | e.mobs[i].uid, PlayerId::HOST).unwrap();
    assert!((seat.y - (e.mobs[i].pos.y + PASSENGER_HEIGHT)).abs() < 1e-9);
}

#[test]
fn ridden_nautiluses_swim_dash_and_give_breath_of_the_nautilus() {
    let world = ocean();
    let mut e = Entities::new(3);
    let i = tame(&mut e, MobKind::Nautilus, DVec3::new(0.5, 10.0, 0.5));
    let s = e.mobs[i].mount.as_mut().unwrap();
    s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
    s.rider = Some(PlayerId::HOST);
    s.yaw = 0.0;
    let mut rider = Target::new(PlayerId::HOST, DVec3::new(0.5, 11.0, 0.5), true);
    rider.in_water = true;
    let c = ctx(vec![rider]);
    let mut breath = 0;
    let start = e.mobs[i].pos;
    for _ in 0..100 {
        e.mobs[i].mount.as_mut().unwrap().forward = 1.0;
        for ev in e.update(0.05, &world, &c) {
            if let EntityEvent::PlayerEffect { effect: Effect::BreathOfTheNautilus, ticks, amplifier: 0, .. } = ev {
                assert_eq!(ticks, EFFECT_DURATION);
                breath += 1;
            }
        }
    }
    // Refreshed when missing (the target here never has it) — every tick.
    assert!(breath >= 90, "{breath}");
    // Terminal `deltaMovement` in water: 0.0325 a tick against 0.9 drag.
    let terminal = WATER_RESISTANCE * RIDDEN_SPEED_IN_WATER / (1.0 - WATER_RESISTANCE);
    let speed = e.mobs[i].vel.length() * TICK;
    assert!((speed - terminal).abs() < terminal * 0.1, "{speed} vs {terminal}");
    assert!(e.mobs[i].pos.x - start.x > 5.0, "swims toward +X at yaw 0");
    // The dash: 1.2 × the movement speed along the look, then 40 ticks of cooldown.
    let v0 = e.mobs[i].vel;
    let n = e.mobs[i].nautilus.as_mut().unwrap();
    n.dash_cooldown = 0;
    let s = e.mobs[i].mount.as_mut().unwrap();
    s.pending_jump_for_test(1.0);
    let world2 = ocean();
    e.update(0.05, &world2, &c);
    assert!(e.mobs[i].vel.x * TICK > v0.x * TICK + 0.9, "{} {}", e.mobs[i].vel.x * TICK, v0.x * TICK);
    assert_eq!(e.mobs[i].nautilus.as_ref().unwrap().dash_cooldown, DASH_COOLDOWN - 1);
}

#[test]
fn nautiluses_dry_out_on_land_and_zombie_ones_do_not() {
    let world = Grid::flat(0);
    let mut e = Entities::new(4);
    e.spawn(MobKind::Nautilus, DVec3::new(0.5, 1.0, 0.5));
    e.spawn(MobKind::ZombieNautilus, DVec3::new(4.5, 1.0, 0.5));
    let c = ctx(Vec::new());
    for _ in 0..(TOTAL_AIR + 19) {
        e.nautilus_upkeep(0.05, &world, &c, &mut Vec::new());
    }
    assert_eq!(e.mobs[0].health, MAX_HEALTH);
    e.nautilus_upkeep(0.05, &world, &c, &mut Vec::new());
    assert_eq!(e.mobs[0].health, MAX_HEALTH - DRY_OUT_DAMAGE);
    assert_eq!(e.mobs[1].health, MAX_HEALTH);
}

#[test]
fn a_hurt_wild_nautilus_charges_its_attacker_in_water() {
    let world = ocean();
    let mut e = Entities::new(5);
    e.spawn(MobKind::Nautilus, DVec3::new(0.5, 10.0, 0.5));
    e.ensure_mob_ids();
    e.mobs[0].nautilus.as_mut().unwrap().anger = Some((PlayerId(7), ANGER_DURATION));
    let mut p = Target::new(PlayerId(7), DVec3::new(4.5, 10.0, 0.5), true);
    p.in_water = true;
    let c = ctx(vec![p]);
    let mut hit = None;
    for _ in 0..60 {
        for ev in e.update(0.05, &world, &c) {
            if let EntityEvent::PlayerHit { player: PlayerId(7), damage, .. } = ev {
                hit = Some(damage);
            }
        }
        if hit.is_some() {
            break;
        }
    }
    assert_eq!(hit, Some(ATTACK_DAMAGE));
    let n = e.mobs[0].nautilus.as_ref().unwrap();
    assert!(n.charge.is_none() && n.charge_cooldown > TIME_BETWEEN_ATTACKS - 5);
    // A tame one never charges.
    let mut e = Entities::new(6);
    let i = tame(&mut e, MobKind::Nautilus, DVec3::new(0.5, 10.0, 0.5));
    e.mobs[i].nautilus.as_mut().unwrap().anger = Some((PlayerId(7), ANGER_DURATION));
    for _ in 0..40 {
        assert!(!e.update(0.05, &world, &c).iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { .. })));
    }
}

#[test]
fn tame_nautiluses_breed_tame_babies() {
    let world = ocean();
    let mut e = Entities::new(7);
    let a = tame(&mut e, MobKind::Nautilus, DVec3::new(0.5, 10.0, 0.5));
    let b = tame(&mut e, MobKind::Nautilus, DVec3::new(1.5, 10.0, 0.5));
    for i in [a, b] {
        assert_eq!(e.use_nautilus(i, Some(Stack::new(Item::COD, 1)), PlayerId::HOST), Some(Use::Ate { bucket: false }));
    }
    let c = ctx(Vec::new());
    for _ in 0..200 {
        e.update(0.05, &world, &c);
    }
    let baby = e.mobs.iter().find(|m| m.kind == MobKind::Nautilus && m.baby).expect("a baby");
    assert!(baby.mount.as_ref().unwrap().tame);
    assert_eq!(baby.mount.as_ref().unwrap().owner, Some(PlayerId::HOST));
    // Zombie nautiluses never breed.
    let mut e = Entities::new(8);
    let z = tame(&mut e, MobKind::ZombieNautilus, DVec3::new(0.5, 10.0, 0.5));
    assert_eq!(e.use_nautilus(z, Some(Stack::new(Item::COD, 1)), PlayerId::HOST), None);
}

#[test]
fn gear_air_anger_and_variant_survive_save_and_lan() {
    let mut e = Entities::new(9);
    let i = tame(&mut e, MobKind::Nautilus, DVec3::new(0.5, 10.0, 0.5));
    let s = e.mobs[i].mount.as_mut().unwrap();
    s.slots[0] = Some(Stack::new(Item::SADDLE, 1));
    s.slots[1] = Some(Stack::new(crate::equipment::GOLDEN_NAUTILUS_ARMOR, 1));
    let n = e.mobs[i].nautilus.as_mut().unwrap();
    n.air = 123;
    n.anger = Some((PlayerId(3), 77));
    e.spawn(MobKind::ZombieNautilus, DVec3::new(3.5, 10.0, 0.5));
    e.nautilus_on_spawn(true);
    let mut restored = Entities::new(10);
    restored.load_nether_mobs(&e.nether_mobs_to_string());
    let n = restored.mobs.iter().find(|m| m.kind == MobKind::Nautilus).unwrap();
    let s = n.mount.as_ref().unwrap();
    assert!(s.tame && s.saddled());
    assert_eq!(s.slots[1].map(|s| s.item), Some(crate::equipment::GOLDEN_NAUTILUS_ARMOR));
    assert_eq!(n.nautilus.as_ref().unwrap().air, 123);
    assert_eq!(n.nautilus.as_ref().unwrap().anger, Some((PlayerId(3), 77)));
    let z = restored.mobs.iter().find(|m| m.kind == MobKind::ZombieNautilus).unwrap();
    assert_eq!(z.mount.as_ref().unwrap().variant, 1, "the warm coral variant");
    // LAN replicas draw the saddle, armor, variant and dash.
    e.mobs[i].nautilus.as_mut().unwrap().dashing = true;
    let mut replica = Mob::new(MobKind::Nautilus, DVec3::ZERO, 0.0);
    replica.apply_look(&e.mobs[i].look());
    let s = replica.mount.as_ref().unwrap();
    assert!(s.saddled());
    assert_eq!(s.slots[1].map(|s| s.item), Some(crate::equipment::GOLDEN_NAUTILUS_ARMOR));
    assert!(replica.nautilus.as_ref().unwrap().dashing);
    let zi = e.mobs.iter().position(|m| m.kind == MobKind::ZombieNautilus).unwrap();
    let mut zr = Mob::new(MobKind::ZombieNautilus, DVec3::ZERO, 0.0);
    zr.apply_look(&e.mobs[zi].look());
    assert_eq!(zr.mount.as_ref().unwrap().variant, 1);
}

#[test]
fn nautiluses_spawn_in_javas_ocean_lists_and_depths() {
    use crate::world::terrain::SEA_LEVEL;
    let water = Block::WATER;
    assert!(spawn_ok(water, water, water, SEA_LEVEL - 5));
    assert!(spawn_ok(water, water, water, SEA_LEVEL - 25));
    assert!(!spawn_ok(water, water, water, SEA_LEVEL - 4));
    assert!(!spawn_ok(water, water, water, SEA_LEVEL - 26));
    assert!(!spawn_ok(water, Block::SAND, water, SEA_LEVEL - 10));
    assert!(MobKind::Nautilus.spawns_in(Dimension::Overworld));
    assert!(!MobKind::ZombieNautilus.spawns_in(Dimension::Overworld));
}
