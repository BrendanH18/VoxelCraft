//! Camel husks, the parched and natural jockeys against
//! `tests/fixtures/java/26.3/new_mobs.json`.
use super::*;
use crate::{
    entity::{Ctx, PlayerId, Target, category::MobCategory, monster_spawns},
    inventory::Inventory,
    item::Item,
    physics::test_util::Grid,
    simulation::{effects::Effect, lighting::SpawnLight},
    world::terrain::Dimension,
};
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

#[test]
fn camel_husk_and_parched_registration_match_java() {
    let f = facts();
    for (name, kind) in [("camel_husk", MobKind::CamelHusk), ("parched", MobKind::Parched)] {
        let t = &f["entity_types"][name];
        assert_eq!(MobKind::from_name(name), Some(kind));
        assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
        assert_eq!(kind.shape().half_width * 2.0, num(&t["width"]));
        assert_eq!(kind.shape().height, num(&t["height"]));
        assert_eq!(t["category"], "monster");
        assert_eq!(kind.category(), MobCategory::Monster);
        assert_eq!(f64::from(kind.max_health()), num(&f["attributes"][name]["max_health"]));
    }
    let a = &f["attributes"]["camel_husk"];
    assert_eq!(crate::entity::mob::goals::movement_speed(MobKind::CamelHusk) as f32, num(&a["movement_speed"]) as f32);
    let s = crate::entity::mounts::State::new(MobKind::CamelHusk, &mut crate::entity::Rng::new(1));
    assert_eq!(s.speed as f32, num(&a["movement_speed"]) as f32);
    assert!(s.tame, "camel husks need no taming");
    assert_eq!(f["husk_camel_husk_chance"].as_f64().unwrap() as f32, CAMEL_HUSK_CHANCE);
    let p = &f["parched"];
    assert_eq!(p["arrow_weakness_ticks"].as_u64(), Some(u64::from(crate::entity::hostiles::PARCHED_WEAKNESS_TICKS)));
    assert_eq!(MobKind::Parched.arrow_effect(), Some((Effect::Weakness, 0, 600)));
    // Their intervals plus the 20-tick draw, as the other variants.
    assert_eq!(MobKind::Parched.bow_interval(true), (num(&p["hard_attack_interval"]) as f32 + 20.0) / 20.0);
    assert_eq!(MobKind::Parched.bow_interval(false), (num(&p["attack_interval"]) as f32 + 20.0) / 20.0);
    assert!(!MobKind::Parched.accepts_potion_effect(Effect::Weakness));
    assert!(MobKind::Parched.is_bow_skeleton() && MobKind::Parched.is_hostile());
    let e = &f["tags"]["entity_type"];
    assert!(names(&e["skeletons"]).contains(&"parched".to_owned()));
    assert!(names(&e["zombies"]).contains(&"camel_husk".to_owned()));
    assert!(names(&e["can_equip_saddle"]).contains(&"camel_husk".to_owned()));
    // Neither is in `#burn_in_daylight`.
    for kind in [MobKind::Parched, MobKind::CamelHusk] {
        assert!(!names(&e["burn_in_daylight"]).contains(&kind.name().to_owned()));
        assert!(!kind.burns_in_sun());
        assert_eq!(kind.creature(), crate::enchant::Creature::Undead);
    }
    assert_eq!(names(&f["tags"]["item"]["camel_husk_food"]), ["rabbit_foot"]);
}

#[test]
fn loot_tables_match_java() {
    let f = facts();
    let husk = &f["loot_tables"]["entities/camel_husk"]["pools"][0]["entries"][0];
    assert_eq!(husk["name"], "minecraft:rotten_flesh");
    let count = &husk["modifier"][0]["count"];
    assert_eq!(MobKind::CamelHusk.loot(), [(Item::ROTTEN_FLESH, num(&count["min"]) as i8, num(&count["max"]) as u8)]);
    let parched = &f["loot_tables"]["entities/parched"]["pools"];
    assert_eq!(parched[0]["entries"][0]["name"], "minecraft:arrow");
    assert_eq!(parched[1]["entries"][0]["name"], "minecraft:bone");
    assert_eq!(parched[2]["entries"][0]["modifier"][2]["id"], "minecraft:weakness");
    assert_eq!(MobKind::Parched.arrow_potion(), crate::potion::Potion::from_id("weakness"));
    let mut tipped = 0;
    for seed in 0..200 {
        let mut e = Entities::new(seed);
        e.drop_loot_with_fire(MobKind::Parched, DVec3::new(0., 10., 0.), 0, false, true);
        for s in &e.items {
            if s.stack.item == Item::TIPPED_ARROW {
                assert_eq!(s.stack.arrow_potion(), crate::potion::Potion::from_id("weakness"));
                tipped += 1;
            }
        }
        let mut e = Entities::new(seed);
        e.drop_loot_with_fire(MobKind::CamelHusk, DVec3::new(0., 10., 0.), 0, false, false);
        let flesh: u32 =
            e.items.iter().filter(|i| i.stack.item == Item::ROTTEN_FLESH).map(|i| u32::from(i.stack.count)).sum();
        assert!((2..=3).contains(&flesh), "{flesh}");
    }
    assert!(tipped > 50);
}

#[test]
fn parched_spawn_in_desert_monster_lists_under_open_sky() {
    let desert = monster_spawns::monster_list(crate::world::terrain::Biome::Desert);
    let parched = desert.iter().find(|s| s.mob.kind() == Some(MobKind::Parched)).expect("desert lists parched");
    assert_eq!((parched.weight, parched.min, parched.max), (50, 4, 4));
    assert!(MobKind::Parched.spawns_in(Dimension::Overworld));
    assert!(!MobKind::CamelHusk.spawns_in(Dimension::Overworld));
}

#[test]
fn parched_shoot_weakness_arrows() {
    let mut e = Entities::new(2);
    e.spawn(MobKind::Parched, DVec3::new(0.5, 10., 0.5));
    let world = Grid::flat(10);
    let c = ctx(vec![Target::new(PlayerId(7), DVec3::new(9.5, 10., 0.5), true)]);
    let mut weakness = false;
    for _ in 0..200 {
        for ev in e.update_difficulty(0.05, &world, &c, crate::simulation::difficulty::Difficulty::Normal) {
            if matches!(
                ev,
                crate::entity::EntityEvent::PlayerEffect {
                    player: PlayerId(7),
                    effect: Effect::Weakness,
                    ticks: 600,
                    ..
                }
            ) {
                weakness = true;
            }
        }
    }
    assert!(weakness);
}

#[test]
fn natural_husks_bring_camel_husks_with_a_parched_and_spears() {
    let world = Grid::flat(0);
    let mut jockeys = 0;
    for seed in 0..400 {
        let mut e = Entities::new(seed);
        e.spawn(MobKind::Husk, DVec3::new(0.5, 1.0, 0.5));
        e.natural_jockeys(&world, crate::world::terrain::Biome::Desert);
        let Some(c) = e.mobs.iter().position(|m| m.kind == MobKind::CamelHusk) else {
            assert_eq!(e.mobs.len(), 1);
            continue;
        };
        jockeys += 1;
        let vehicle = MOB_VEHICLE | e.mobs[c].uid;
        let riders: Vec<MobKind> = e.mobs.iter().filter(|m| m.riding == Some(vehicle)).map(|m| m.kind).collect();
        assert_eq!(riders, [MobKind::Husk, MobKind::Parched]);
        let husk = &e.mobs[0];
        assert_eq!(husk.equipment.main_hand().map(|s| s.item), Some(crate::weapons::spear(crate::item::Tier::Iron)));
        assert!(!e.mobs[c].persistent);
        // Front and back seats, half a block either side of the camel's middle.
        let parched = e.mobs.iter().find(|m| m.kind == MobKind::Parched).unwrap();
        assert!((husk.pos.distance(parched.pos) - 1.0).abs() < 1e-6);
    }
    assert!((20..=65).contains(&jockeys), "{jockeys} of 400");
    // No room for a camel husk: no try.
    let mut walled = Grid::flat(0);
    walled.set(glam::IVec3::new(1, 2, 0), crate::world::block::Block::STONE);
    for seed in 0..100 {
        let mut e = Entities::new(seed);
        e.spawn(MobKind::Husk, DVec3::new(0.5, 1.0, 0.5));
        e.natural_jockeys(&walled, crate::world::terrain::Biome::Desert);
        assert_eq!(e.mobs.len(), 1);
    }
}

#[test]
fn trident_drowned_ride_zombie_nautiluses_outside_rivers() {
    let world = Grid::flat(0);
    let mut jockeys = 0;
    for seed in 0..200 {
        for biome in [crate::world::terrain::Biome::WarmOcean, crate::world::terrain::Biome::River] {
            let mut e = Entities::new(seed);
            e.spawn(MobKind::Drowned, DVec3::new(0.5, 1.0, 0.5));
            e.mobs[0].baby = false;
            e.mobs[0].equipment.set(
                crate::entity::equipment::Slot::MainHand,
                Some(crate::inventory::Stack::new(crate::weapons::TRIDENT, 1)),
            );
            e.natural_jockeys(&world, biome);
            let z = e.mobs.iter().find(|m| m.kind == MobKind::ZombieNautilus);
            if biome.is_river() {
                assert!(z.is_none());
                continue;
            }
            if let Some(z) = z {
                jockeys += 1;
                assert_eq!(e.mobs[0].riding, Some(MOB_VEHICLE | z.uid));
                assert_eq!(z.mount.as_ref().unwrap().variant, 1, "warm oceans give the coral variant");
            }
        }
    }
    assert!((70..=130).contains(&jockeys), "{jockeys} of 200");
}

#[test]
fn camel_husks_ride_like_camels_and_eat_rabbit_feet() {
    let mut e = Entities::new(3);
    e.spawn(MobKind::CamelHusk, DVec3::new(5.0, 140.0, 5.0));
    e.mobs[0].health = 20.0;
    let eye = DVec3::new(5.0, 141.0, 2.0);
    let mut inv = Inventory::default();
    inv.slots[0] = Some(crate::inventory::Stack::new(Item::RABBIT_FOOT, 2));
    inv.slots[1] = Some(crate::inventory::Stack::new(Item::SADDLE, 1));
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 0, false));
    assert_eq!(e.mobs[0].health, 22.0);
    assert_eq!(inv.get(0).map(|s| s.count), Some(1));
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 1, false));
    assert!(e.mobs[0].mount.as_ref().unwrap().saddled());
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 5, false));
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId(2), &mut inv, 5, false), "a second seat");
    assert!(!MobKind::CamelHusk.is_breedable());
    // Mob riders keep players off; their pursuit steers the camel husk.
    let mut e = Entities::new(4);
    e.spawn(MobKind::Husk, DVec3::new(0.5, 1.0, 0.5));
    e.spawn_camel_husk_jockey(0);
    let c = e.mobs.iter().position(|m| m.kind == MobKind::CamelHusk).unwrap();
    let eye = e.mobs[c].pos + DVec3::new(0.0, 1.0, -3.0);
    assert!(!e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut Inventory::default(), 0, false));
    let world = Grid::flat(0);
    let cx = ctx(vec![Target::new(PlayerId(7), e.mobs[c].pos + DVec3::new(12.0, 0.0, 0.0), true)]);
    let start = e.mobs[c].pos;
    for _ in 0..60 {
        e.update(0.05, &world, &cx);
    }
    let c = e.mobs.iter().position(|m| m.kind == MobKind::CamelHusk).unwrap();
    assert!(e.mobs[c].pos.x > start.x + 1.0, "the husk drives it toward its target");
}

#[test]
fn jockeys_save_reload_and_replicate() {
    let mut e = Entities::new(5);
    e.spawn(MobKind::Husk, DVec3::new(0.5, 1.0, 0.5));
    e.spawn_camel_husk_jockey(0);
    let c = e.mobs.iter().position(|m| m.kind == MobKind::CamelHusk).unwrap();
    e.mobs[c].mount.as_mut().unwrap().slots[0] = Some(crate::inventory::Stack::new(Item::SADDLE, 1));
    let mut restored = Entities::new(6);
    restored.load_nether_mobs(&e.nether_mobs_to_string());
    let kinds: Vec<MobKind> = restored.mobs.iter().map(|m| m.kind).collect();
    assert!(kinds.contains(&MobKind::CamelHusk) && kinds.contains(&MobKind::Parched) && kinds.contains(&MobKind::Husk));
    let cr = restored.mobs.iter().find(|m| m.kind == MobKind::CamelHusk).unwrap();
    assert!(cr.mount.as_ref().unwrap().saddled());
    let vehicle = MOB_VEHICLE | cr.uid;
    assert_eq!(restored.mobs.iter().filter(|m| m.riding == Some(vehicle)).count(), 2);
    let parched = restored.mobs.iter().find(|m| m.kind == MobKind::Parched).unwrap();
    assert_eq!(parched.equipment.main_hand().map(|s| s.item), Some(Item::BOW));
    // LAN replicas: the saddle and the camel pose.
    let mut replica = Mob::new(MobKind::CamelHusk, DVec3::ZERO, 0.0);
    replica.apply_look(&e.mobs[c].look());
    assert!(replica.mount.as_ref().unwrap().saddled());
    let pi = e.mobs.iter().position(|m| m.kind == MobKind::Parched).unwrap();
    let mut pr = Mob::new(MobKind::Parched, DVec3::ZERO, 0.0);
    pr.apply_look(&e.mobs[pi].look());
    assert_eq!(pr.equipment.main_hand().map(|s| s.item), Some(Item::BOW));
}

/// v1.0 Track 7: `Strider.finalizeSpawn` for an adult: a saddled strider
/// under a zombified piglin with a warped fungus on a stick one time in 30
/// (only while monsters spawn), else a baby strider rider one in ten. The
/// riders and the saddle survive a save.
#[test]
fn natural_striders_carry_piglin_and_baby_strider_jockeys() {
    let world = Grid::flat(0);
    let (mut piglins, mut babies) = (0, 0);
    for seed in 0..1500 {
        let mut e = Entities::new(seed);
        e.monster_spawning = true;
        e.difficulty = crate::simulation::difficulty::Difficulty::Normal;
        e.spawn(MobKind::Strider, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[0].baby = false;
        e.natural_jockeys(&world, crate::world::terrain::Biome::Plains);
        if e.mobs.len() == 1 {
            continue;
        }
        let vehicle = MOB_VEHICLE | e.mobs[0].uid;
        let rider = &e.mobs[1];
        assert_eq!(rider.riding, Some(vehicle));
        match rider.kind {
            MobKind::ZombifiedPiglin => {
                piglins += 1;
                assert_eq!(rider.equipment.main_hand().map(|s| s.item), Some(Item::WARPED_FUNGUS_ON_A_STICK));
                assert!(e.mobs[0].mount.as_ref().unwrap().saddled(), "the strider is saddled");
                // `ridingOffset(-0.7)` on the strider's 1.7-block attachment.
                assert!((rider.pos.y - (e.mobs[0].pos.y + MobKind::Strider.shape().height - 0.7)).abs() < 1e-6);
                let mut back = Entities::new(seed);
                back.load_nether_mobs(&e.nether_mobs_to_string());
                let strider = back.mobs.iter().find(|m| m.kind == MobKind::Strider).unwrap();
                assert!(strider.mount.as_ref().unwrap().saddled());
                let piglin = back.mobs.iter().find(|m| m.kind == MobKind::ZombifiedPiglin).unwrap();
                assert_eq!(piglin.riding, Some(MOB_VEHICLE | strider.uid));
            }
            MobKind::Strider => {
                babies += 1;
                assert!(rider.baby && rider.age == -24000);
            }
            k => panic!("unexpected rider {k:?}"),
        }
    }
    // 1/30 of 1500 is 50; then a tenth of the rest is about 145.
    assert!((25..=80).contains(&piglins), "{piglins}");
    assert!((100..=195).contains(&babies), "{babies}");
    // No monster spawning: never a piglin.
    for seed in 0..300 {
        let mut e = Entities::new(seed);
        e.monster_spawning = false;
        e.spawn(MobKind::Strider, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[0].baby = false;
        e.natural_jockeys(&world, crate::world::terrain::Biome::Plains);
        assert!(e.mobs.iter().all(|m| m.kind != MobKind::ZombifiedPiglin));
    }
}
