//! Happy ghasts and dried ghasts against `tests/fixtures/java/26.3/new_mobs.json`.
use super::*;
use crate::{
    color::DyeColor,
    content::harness::harness,
    entity::{Target, category::MobCategory},
    inventory::Inventory,
    physics::test_util::Grid,
    simulation::lighting::SpawnLight,
    world::{block::Block, dried_ghast, terrain::Dimension},
};
use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/new_mobs.json")).unwrap()
}
fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}
fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        players,
        daylight: 1.0,
        spawn_light: SpawnLight::DAY,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn adult(e: &mut Entities, pos: DVec3) -> usize {
    e.spawn(MobKind::HappyGhast, pos);
    e.ensure_mob_ids();
    e.mobs.len() - 1
}
fn harnessed(e: &mut Entities, pos: DVec3) -> usize {
    let i = adult(e, pos);
    assert_eq!(e.use_happy_ghast(i, Some(Stack::new(harness(DyeColor::Red), 1))), Some(Use::Harnessed));
    i
}

#[test]
fn registration_attributes_and_constants_match_java() {
    let f = facts();
    let t = &f["entity_types"]["happy_ghast"];
    let kind = MobKind::from_name("minecraft:happy_ghast").unwrap();
    assert_eq!(kind, MobKind::HappyGhast);
    assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
    assert_eq!(kind.shape().half_width * 2.0, num(&t["width"]));
    assert_eq!(kind.shape().height, num(&t["height"]));
    assert_eq!(t["category"], "creature");
    assert_eq!(kind.category(), MobCategory::Creature);
    let a = &f["attributes"]["happy_ghast"];
    assert_eq!(f64::from(MAX_HEALTH), num(&a["max_health"]));
    assert_eq!(f64::from(kind.max_health()), num(&a["max_health"]));
    assert_eq!(FLYING_SPEED, num(&a["flying_speed"]));
    assert_eq!(MOVEMENT_SPEED, num(&a["movement_speed"]));
    assert_eq!(TEMPT_RANGE, num(&a["tempt_range"]));
    assert_eq!(FOLLOW_RANGE, num(&a["follow_range"]));
    let c = &f["constants"]["happy_ghast"];
    assert_eq!(BABY_SCALE as f32, num(&c["BABY_SCALE"]) as f32);
    assert_eq!(MAX_PASSENGERS as f64, num(&c["MAX_PASSANGERS"]));
    assert_eq!(f64::from(SMALL_RESTRICTION_RADIUS), num(&c["SMALL_RESTRICTION_RADIUS"]));
    assert_eq!(f64::from(LARGE_RESTRICTION_RADIUS), num(&c["LARGE_RESTRICTION_RADIUS"]));
    assert_eq!(f64::from(RESTRICTION_RADIUS_BUFFER), num(&c["RESTRICTION_RADIUS_BUFFER"]));
    assert_eq!(f64::from(FAST_HEALING_TICKS), num(&c["FAST_HEALING_TICKS"]));
    assert_eq!(f64::from(SLOW_HEALING_TICKS), num(&c["SLOW_HEALING_TICKS"]));
    assert_eq!(f64::from(STILL_TIMEOUT_GRACE), num(&c["STILL_TIMEOUT_ON_LOAD_GRACE_PERIOD"]));
    assert_eq!(f64::from(MAX_STILL_TIMEOUT), num(&c["MAX_STILL_TIMEOUT"]));
    assert_eq!(BABY_PANIC_SPEED, num(&c["SPEED_MULTIPLIER_WHEN_PANICKING"]));
    assert_eq!(f64::from(FLOAT_DISTANCE_TO_BLOCKS), num(&c["WANDER_GROUND_DISTANCE"]));
    let ai = &f["constants"]["happy_ghast_ai"];
    assert_eq!(BABY_TEMPT_SPEED as f32, num(&ai["SPEED_MULTIPLIER_WHEN_TEMPTED"]) as f32);
    assert_eq!(BABY_FOLLOW_SPEED as f32, num(&ai["SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT"]) as f32);
    assert_eq!(BABY_CLOSE_ENOUGH, num(&ai["BABY_GHAST_CLOSE_ENOUGH_DIST"]));
    // The baby box is the adult's scaled by `BABY_SCALE`.
    let mut baby = Mob::new(MobKind::HappyGhast, DVec3::ZERO, 0.0);
    baby.baby = true;
    assert!((baby.shape().height - 4.0 * BABY_SCALE).abs() < 1e-9);
    // `entities/happy_ghast` has no pools; the harness drops as equipment.
    assert!(f["loot_tables"]["entities/happy_ghast"].get("pools").is_none());
    assert!(MobKind::HappyGhast.loot().is_empty());
    assert!(f["tags"]["entity_type"]["fall_damage_immune"].as_array().unwrap().contains(&"happy_ghast".into()));
}

#[test]
fn food_and_tempt_items_follow_the_tags() {
    let f = facts();
    let food: Vec<&str> =
        f["tags"]["item"]["happy_ghast_food"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(food, ["snowball"]);
    for item in [Item::SNOWBALL, Item::SLIME_BALL, harness(DyeColor::Blue)] {
        assert_eq!(is_food(item), food.contains(&item.name()), "{}", item.name());
    }
    let tempt = &f["tags"]["item"]["happy_ghast_tempt_items"];
    assert_eq!(tempt, &serde_json::json!(["#minecraft:happy_ghast_food", "#minecraft:harnesses"]));
    let mut m = Mob::new(MobKind::HappyGhast, DVec3::ZERO, 0.0);
    assert!(tempted_by(&m, harness(DyeColor::Lime)));
    assert!(tempted_by(&m, Item::SNOWBALL));
    // Harnessed adults and ghastlings follow food only.
    m.mount.as_mut().unwrap().slots[1] = Some(Stack::new(harness(DyeColor::Lime), 1));
    assert!(!tempted_by(&m, harness(DyeColor::Lime)));
    assert!(tempted_by(&m, Item::SNOWBALL));
    let avoids: Vec<&str> =
        f["tags"]["block"]["happy_ghast_avoids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(avoids.contains(&"cactus") && avoids.contains(&"magma_block"));
    assert!(avoids::avoids_all(&[Block::CACTUS, Block::MAGMA]));
}

mod avoids {
    pub(super) fn avoids_all(blocks: &[crate::world::block::Block]) -> bool {
        blocks.iter().all(|&b| super::avoids(b))
    }
}

#[test]
fn ghastlings_grow_up_and_snowballs_hurry_them() {
    let world = Grid::flat(0);
    let mut e = Entities::new(3);
    e.spawn_ghastling(IVec3::new(0, 40, 0), 0.0);
    assert!(e.mobs[0].baby && e.mobs[0].age == crate::entity::animals::BABY_AGE);
    // `getSpeedUpSecondsWhenFeeding`: a tenth of the 1200 seconds left.
    assert_eq!(e.use_happy_ghast(0, Some(Stack::new(Item::SNOWBALL, 1))), Some(Use::Fed));
    assert_eq!(e.mobs[0].age, -24000 + 120 * 20);
    let c = ctx(Vec::new());
    for _ in 0..(24000 - 120 * 20) {
        e.happy_ghast_upkeep(0.05, &world, &c);
    }
    assert!(!e.mobs[0].baby && e.mobs[0].age == 0, "{}", e.mobs[0].age);
    // Adults don't breed (`canFallInLove` is false).
    assert_eq!(e.use_happy_ghast(0, Some(Stack::new(Item::SNOWBALL, 1))), None);
    assert!(!MobKind::HappyGhast.is_breedable());
}

#[test]
fn harness_shears_and_four_seats() {
    let mut e = Entities::new(4);
    let i = harnessed(&mut e, DVec3::new(0.5, 80.0, 0.5));
    // A second harness doesn't go on over the first.
    assert_eq!(e.use_happy_ghast(i, Some(Stack::new(harness(DyeColor::Blue), 1))), None);
    for p in 1..=4 {
        assert!(e.board_happy_ghast(i, PlayerId(p)), "seat {p}");
    }
    assert!(!e.board_happy_ghast(i, PlayerId(5)), "MAX_PASSANGERS is four");
    let seats: Vec<DVec3> = (1..=4).map(|p| e.mount_seat(MOB_VEHICLE_OF(&e, i), PlayerId(p)).unwrap()).collect();
    for (a, s) in seats.iter().enumerate() {
        assert!((s.y - (e.mobs[i].pos.y + 4.0)).abs() < 1e-9);
        assert!((((*s - e.mobs[i].pos) * DVec3::new(1.0, 0.0, 1.0)).length() - 1.7).abs() < 1e-6);
        assert!(seats[..a].iter().all(|o| o.distance(*s) > 1.0));
    }
    assert_eq!(e.mount_for_player(PlayerId(3)), Some(MOB_VEHICLE_OF(&e, i)));
    // No shearing while anyone rides; the first rider controls.
    assert_eq!(e.use_happy_ghast(i, Some(Stack::new(Item::SHEARS, 1))), None);
    assert!(e.mobs[i].happy_ghast_controlled());
    // Getting off lands on top and starts a ten-tick still timeout;
    // the next passenger moves up to the controlling seat.
    let world = Grid::flat(0);
    let top = e.dismount_animal(&world, PlayerId(1)).unwrap();
    assert!((top.y - (e.mobs[i].pos.y + 4.0)).abs() < 1e-9);
    assert_eq!(e.mobs[i].happy_ghast.as_ref().unwrap().still_timeout, MAX_STILL_TIMEOUT);
    assert_eq!(e.mobs[i].mount.as_ref().unwrap().rider, Some(PlayerId(2)));
    for p in 2..=4 {
        e.dismount_animal(&world, PlayerId(p));
    }
    assert_eq!(e.use_happy_ghast(i, Some(Stack::new(Item::SHEARS, 1))), Some(Use::Sheared));
    assert!(e.items.iter().any(|it| it.stack.item == harness(DyeColor::Red)));
    assert!(!e.board_happy_ghast(i, PlayerId(1)), "no harness, no ride");
}

#[allow(non_snake_case)]
fn MOB_VEHICLE_OF(e: &Entities, i: usize) -> u32 {
    crate::entity::mounts::MOB_VEHICLE | e.mobs[i].uid
}

#[test]
fn ridden_input_matches_get_ridden_input() {
    let k = RIDDEN_INPUT_SCALE * FLYING_SPEED;
    // Level forward: all forward.
    let v = ridden_input(1.0, 0.0, 0.0, false);
    assert!((v - DVec3::new(0.0, 0.0, k)).length() < 1e-9);
    // Looking 30° up climbs by sin 30°.
    let v = ridden_input(1.0, 0.0, 30f32.to_radians(), false);
    assert!((v.y - 0.5 * k).abs() < 1e-6 && (v.z - 30f64.to_radians().cos() * k).abs() < 1e-6);
    // Backwards is half speed, reversed; jumping adds half a unit up.
    let v = ridden_input(-1.0, 0.0, 0.0, true);
    assert!((v - DVec3::new(0.0, 0.5 * k, -0.5 * k)).length() < 1e-9);
    // Strafing right is Java's negative `xxa`.
    assert!(ridden_input(0.0, 1.0, 0.0, false).x < 0.0);
}

#[test]
fn riders_steer_at_javas_flying_speed_and_a_player_on_top_stops_it() {
    let world = Grid::flat(0);
    let mut e = Entities::new(5);
    let i = harnessed(&mut e, DVec3::new(0.5, 80.0, 0.5));
    e.mobs[i].happy_ghast.as_mut().unwrap().ticks = 100;
    assert!(e.board_happy_ghast(i, PlayerId::HOST));
    let yaw = e.mobs[i].yaw;
    let s = e.mobs[i].mount.as_mut().unwrap();
    s.forward = 1.0;
    s.yaw = yaw;
    let c = ctx(vec![Target::new(PlayerId(9), DVec3::new(50.0, 10.0, 50.0), true)]);
    let start = e.mobs[i].pos;
    for _ in 0..200 {
        e.update(0.05, &world, &c);
        let s = e.mobs[i].mount.as_mut().unwrap();
        s.forward = 1.0;
    }
    // `travelFlying`: accel 3.9 × 0.05 × (0.05 × 5/3) a tick against 0.91
    // drag; `deltaMovement` settles at the drag times the accel over 0.09.
    let terminal = 0.91 * RIDDEN_INPUT_SCALE * FLYING_SPEED * (FLYING_SPEED * 5.0 / 3.0) / (1.0 - 0.91);
    let speed = e.mobs[i].vel.length() * TICK;
    assert!((speed - terminal).abs() < terminal * 0.05, "{speed} vs {terminal}");
    assert!(e.mobs[i].pos.distance(start) > 20.0 * terminal * 0.8 * 5.0);
    // Someone standing on its back puts it on the still timeout.
    let top = e.mobs[i].pos + DVec3::Y * 4.0;
    let c = ctx(vec![Target::new(PlayerId(9), top, true)]);
    e.update(0.05, &world, &c);
    assert_eq!(e.mobs[i].happy_ghast.as_ref().unwrap().still_timeout, MAX_STILL_TIMEOUT);
    assert!(!e.mobs[i].happy_ghast_controlled());
    for _ in 0..3 {
        e.update(0.05, &world, &c);
    }
    assert!(e.mobs[i].vel.length() < 1e-9, "stopInPlace");
}

#[test]
fn healing_and_home_follow_the_harness() {
    let world = Grid::flat(0);
    let mut e = Entities::new(6);
    let i = adult(&mut e, DVec3::new(0.5, 80.0, 0.5));
    e.mobs[i].health = 10.0;
    let c = ctx(Vec::new());
    for _ in 0..SLOW_HEALING_TICKS {
        e.happy_ghast_upkeep(0.05, &world, &c);
    }
    assert_eq!(e.mobs[i].health, 11.0);
    assert_eq!(e.mobs[i].restrict.map(|r| r.1), Some(LARGE_RESTRICTION_RADIUS));
    e.use_happy_ghast(i, Some(Stack::new(harness(DyeColor::Cyan), 1)));
    e.happy_ghast_upkeep(0.05, &world, &c);
    assert_eq!(e.mobs[i].restrict.map(|r| r.1), Some(SMALL_RESTRICTION_RADIUS));
}

#[test]
fn harness_riders_still_timeout_and_age_survive_save_and_lan() {
    let mut e = Entities::new(7);
    let i = harnessed(&mut e, DVec3::new(0.5, 80.0, 0.5));
    assert!(e.board_happy_ghast(i, PlayerId(2)));
    assert!(e.board_happy_ghast(i, PlayerId(3)));
    assert!(e.board_happy_ghast(i, PlayerId(4)));
    e.mobs[i].happy_ghast.as_mut().unwrap().still_timeout = 7;
    e.spawn_ghastling(IVec3::new(5, 70, 5), 1.0);
    e.mobs.last_mut().unwrap().age = -1234;
    let mut restored = Entities::new(8);
    restored.load_nether_mobs(&e.nether_mobs_to_string());
    let g = restored.mobs.iter().find(|m| m.kind == MobKind::HappyGhast && !m.baby).unwrap();
    let s = g.mount.as_ref().unwrap();
    assert_eq!(s.slots[1], Some(Stack::new(harness(DyeColor::Red), 1)));
    assert_eq!(passengers(s).collect::<Vec<_>>(), [PlayerId(2), PlayerId(3), PlayerId(4)]);
    assert_eq!(g.happy_ghast.as_ref().unwrap().still_timeout, 7);
    let baby = restored.mobs.iter().find(|m| m.kind == MobKind::HappyGhast && m.baby).unwrap();
    assert_eq!(baby.age, -1234);
    // A LAN replica draws the harness, the goggles and the still flag.
    e.happy_ghast_upkeep(0.05, &Grid::flat(0), &ctx(Vec::new()));
    let mut replica = Mob::new(MobKind::HappyGhast, DVec3::ZERO, 0.0);
    replica.apply_look(&e.mobs[i].look());
    assert_eq!(replica.mount.as_ref().unwrap().slots[1], Some(Stack::new(harness(DyeColor::Red), 1)));
    assert!(replica.happy_ghast.as_ref().unwrap().goggles_down);
    assert!(replica.happy_ghast.as_ref().unwrap().still());
}

#[test]
fn ordinary_use_harnesses_and_mounts_through_the_shared_paths() {
    let mut e = Entities::new(9);
    let i = adult(&mut e, DVec3::new(5.0, 140.0, 5.0));
    let eye = DVec3::new(5.0, 141.0, 1.0);
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(harness(DyeColor::Pink), 1));
    // Without a harness `use_mount` refuses; the harness goes on through `use_happy_ghast`.
    assert!(!e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 0, false));
    let (hit, _) = e.raycast(eye, DVec3::Z, 5.0).unwrap();
    assert_eq!(hit, i);
    assert_eq!(e.use_happy_ghast(hit, inv.get(0)), Some(Use::Harnessed));
    inv.take_one(0);
    assert!(e.use_mount(eye, DVec3::Z, 5.0, PlayerId::HOST, &mut inv, 0, false));
    assert!(e.mount_for_player(PlayerId::HOST).is_some());
}

#[test]
fn dried_ghast_recipe_loot_and_hydration_match_java() {
    let f = facts();
    let c = &f["constants"]["dried_ghast_block"];
    assert_eq!(f64::from(dried_ghast::MAX_HYDRATION), num(&c["MAX_HYDRATION_LEVEL"]));
    assert_eq!(dried_ghast::HYDRATION_TICK_DELAY as f64, num(&c["HYDRATION_TICK_DELAY"]));
    let r = &f["dried_ghast_recipe"];
    assert_eq!(r["pattern"], serde_json::json!(["###", "#X#", "###"]));
    assert_eq!(r["key"]["#"], "ghast_tear");
    assert_eq!(r["key"]["X"], "soul_sand");
    let mut g = crate::crafting::Grid::new(3);
    for (n, cell) in g.cells.iter_mut().enumerate() {
        *cell = Some(Stack::new(if n == 4 { Item::from_block(Block::SOUL_SAND) } else { Item::GHAST_TEAR }, 1));
    }
    assert_eq!(g.result(), Some(Stack::new(Item::from_block(dried_ghast::DRIED_GHAST), 1)));
    // `blocks/dried_ghast`: itself, every state.
    let loot = &f["loot_tables"]["blocks/dried_ghast"]["pools"][0];
    assert_eq!(loot["entries"][0]["name"], "minecraft:dried_ghast");
    for id in dried_ghast::FIRST..=dried_ghast::LAST {
        assert_eq!(Block(id).drop(), Some(Item::from_block(dried_ghast::DRIED_GHAST)));
        assert_eq!(Block(id).base(), dried_ghast::DRIED_GHAST);
        assert_eq!(Block(id).hardness(), 0.0);
    }
    assert_eq!(Item::from_name("dried_ghast"), Some(Item::from_block(dried_ghast::DRIED_GHAST)));
}

fn block_world() -> crate::world::World {
    use crate::world::{World, chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w
}

#[test]
fn waterlogged_dried_ghasts_hydrate_and_hatch_a_ghastling_facing_their_way() {
    const P: IVec3 = IVec3::new(12, 145, 12);
    let mut w = block_world();
    let wet = dried_ghast::block(dried_ghast::State {
        hydration: dried_ghast::MAX_HYDRATION,
        facing: crate::world::block::Facing::West,
        waterlogged: true,
    });
    w.set_block(P, wet);
    // A pending tick saved at hydration 3 hatches when it comes due.
    w.load_redstone(&format!("v1,0,1|t,12,145,12,1,0,0,{}", dried_ghast::DRIED_GHAST.0));
    w.tick_redstone();
    w.tick_redstone();
    assert_eq!(w.get_block(P), Some(Block::WATER), "the water it held stays");
    let mut e = Entities::new(1);
    e.hatch_dried_ghasts(&mut w);
    let g = &e.mobs[0];
    assert_eq!(g.kind, MobKind::HappyGhast);
    assert!(g.baby && g.persistent);
    assert_eq!(g.pos, DVec3::new(12.5, 145.0, 12.5));
    // `Direction.getYRot(WEST)` is 90°: facing -X.
    assert!((g.yaw.cos() + 1.0).abs() < 1e-5, "{}", g.yaw);
    // Lower levels step up one at a time.
    w.set_block(
        P,
        dried_ghast::block(dried_ghast::State { hydration: 1, waterlogged: true, ..dried_ghast::decode(wet).unwrap() }),
    );
    w.load_redstone(&format!("v1,0,1|t,12,145,12,1,0,0,{}", dried_ghast::DRIED_GHAST.0));
    w.tick_redstone();
    w.tick_redstone();
    assert_eq!(w.get_block(P).and_then(dried_ghast::decode).map(|s| s.hydration), Some(2));
    // Out of water it dries a level per tick.
    let dry = dried_ghast::block(dried_ghast::State {
        hydration: 2,
        waterlogged: false,
        ..dried_ghast::decode(wet).unwrap()
    });
    // A dry cell two blocks over (the first still holds the hatchling's water).
    let q = P + IVec3::X * 2;
    w.set_block(q, dry);
    w.load_redstone(&format!("v1,0,1|t,14,145,12,1,0,0,{}", dried_ghast::DRIED_GHAST.0));
    w.tick_redstone();
    w.tick_redstone();
    assert_eq!(w.get_block(q).and_then(dried_ghast::decode).map(|s| s.hydration), Some(1));
}
