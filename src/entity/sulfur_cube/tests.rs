//! The sulfur cube against `tests/fixtures/java/26.3/sulfur_cube.json`.
use super::*;
use crate::{
    entity::{Rng, Target, category::MobCategory},
    physics::test_util::Grid,
    simulation::{damage::DamageType, lighting::SpawnLight},
    world::{block::Block, terrain::Dimension},
};
use glam::IVec3;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/sulfur_cube.json")).unwrap()
}

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::DAY,
        players,
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn cube(e: &mut Entities, pos: DVec3) -> usize {
    e.spawn(MobKind::SulfurCube, pos);
    let i = e.mobs.len() - 1;
    e.mobs[i].on_ground = true;
    e.ensure_mob_ids();
    i
}

fn item(name: &str) -> Item {
    let english = Archetype::ALL.iter().flat_map(|a| a.item_names()).find(|(n, _)| *n == name).map_or("", |(_, e)| e);
    resolve(name, english).unwrap_or_else(|| panic!("{name} resolves"))
}

#[test]
fn registration_size_health_and_constants_match_java() {
    let facts = facts();
    let kind = MobKind::from_name("minecraft:sulfur_cube").expect("registered");
    assert_eq!(kind, MobKind::SulfurCube);
    assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
    assert_eq!(kind.category(), MobCategory::Monster);
    assert!(!kind.despawns_in_peaceful(), "not `notInPeaceful`");
    assert!(!kind.fire_immune());
    let t = &facts["entity_type"];
    assert_eq!(t["category"], "monster");
    assert_eq!(t["fire_immune"], false);
    assert!((BASE_SIZE - f(&t["width"])).abs() < 1e-6 && (BASE_SIZE - f(&t["height"])).abs() < 1e-6);
    assert!((EYE_HEIGHT - f(&t["eye_height"])).abs() < 1e-6);
    assert_eq!(f(&facts["attributes"]["tempt_range"]), TEMPT_RANGE);
    let c = &facts["constants"]["sulfur_cube"];
    assert_eq!(c["MIN_SIZE"], u64::from(BABY_SIZE));
    assert_eq!(c["MAX_SIZE"], u64::from(ADULT_SIZE));
    assert_eq!(c["PICKUP_TIMER_DURATION"], PICKUP_TIMER as u64);
    assert_eq!(c["SPLIT_COUNT"], 2);
    assert_eq!(f(&c["PUSH_DISTANCE_THRESHOLD"]), PUSH_DISTANCE);
    assert_eq!(f(&c["MAX_PLAYER_PUSH_SPEED"]), MAX_PLAYER_PUSH_SPEED);
    for (name, value) in [
        ("PLAYER_PUSH_SPEED_SCALE_MULTIPLIER", PLAYER_PUSH_SCALE),
        ("VEHICLE_PUSH_SPEED_SCALE_MULTIPLIER", VEHICLE_PUSH_SCALE),
        ("VERTICAL_PUSH_MULTIPLIER", VERTICAL_PUSH),
        ("HORIZONTAL_HIT_ANGLE_SCALE", HORIZONTAL_HIT_ANGLE_SCALE),
        ("VERTICAL_HIT_ANGLE_SCALE", VERTICAL_HIT_ANGLE_SCALE),
        ("VERTICAL_POSITION_ANGLE_SCALE", VERTICAL_POSITION_ANGLE_SCALE),
    ] {
        assert_eq!(f(&c[name]) as f32, value, "{name}");
    }
    assert_eq!(facts["bucket_item_class"], "MobBucketItem");
    assert_eq!(BUCKET.max_stack(), 1);
    assert_eq!(item("sulfur_cube_bucket"), BUCKET);
    // `entities/sulfur_cube` has no pools: only the held block drops.
    assert!(facts["loot_table"].get("pools").is_none());
    assert!(MobKind::SulfurCube.loot().is_empty());

    let mut e = Entities::new(1);
    let i = cube(&mut e, DVec3::new(0.5, 1.0, 0.5));
    let m = &e.mobs[i];
    assert_eq!((m.size, m.baby, m.health, m.max_health()), (ADULT_SIZE, false, 8.0, 8.0));
    assert!((m.shape().height - 2.0 * BASE_SIZE).abs() < 1e-9);
    let mut baby = Mob::new(MobKind::SulfurCube, DVec3::ZERO, 0.0);
    baby.set_sulfur_size(BABY_SIZE);
    assert_eq!((baby.size, baby.baby, baby.age, baby.health), (1, true, BABY_AGE, 4.0));
}

#[test]
fn archetypes_match_the_registry_data_and_their_tags() {
    let facts = facts();
    let archetypes = facts["archetypes"].as_object().unwrap();
    assert_eq!(archetypes.len(), Archetype::ALL.len());
    let gaps: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/registry_gaps.json")).unwrap();
    for a in Archetype::ALL {
        let data = &archetypes[a.name()];
        let names: Vec<&str> = data["items"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let ours: Vec<&str> = a.item_names().iter().map(|(n, _)| *n).collect();
        assert_eq!(ours, names, "{} items", a.name());
        let p = a.physics();
        for m in data["attribute_modifiers"].as_array().unwrap() {
            let amount = f(&m["amount"]);
            match m["attribute"].as_str().unwrap() {
                "knockback_resistance" => assert_eq!(p.knockback_resistance, amount.clamp(-2.0, 1.0)),
                "explosion_knockback_resistance" => {
                    assert_eq!(p.explosion_knockback_resistance, amount.clamp(0.0, 1.0))
                }
                "bounciness" => assert_eq!(p.bounciness, amount.clamp(0.0, 1.0)),
                "friction_modifier" => assert!((f64::from(p.friction) - (1.0 + amount)).abs() < 1e-6),
                "air_drag_modifier" => assert!((f64::from(p.air_drag) - (1.0 + amount)).abs() < 1e-6),
                other => panic!("unexpected attribute {other}"),
            }
        }
        assert_eq!(p.buoyant, data["buoyant"] == true, "{}", a.name());
        let kb = &data["knockback_modifiers"];
        assert_eq!(p.knockback, (f(&kb["horizontal_power"]) as f32, f(&kb["vertical_power"]) as f32));
        let s = &data["sound_settings"];
        assert_eq!(p.push_sound, (f(&s["push_sound_impulse_threshold"]) as f32, f(&s["push_sound_cooldown"]) as f32));
        match (&p.explosion, data.get("explosion")) {
            (Some(x), Some(j)) => {
                assert_eq!((x.power, x.causes_fire, x.fuse), (f(&j["power"]) as f32, j["causes_fire"] == true, 120));
                assert_eq!(j["fuse"], 120);
            }
            (None, None) => {}
            other => panic!("{}: explosion {other:?}", a.name()),
        }
        assert_eq!(p.contact_damage.map(f64::from), data.get("contact_damage").map(|c| f(&c["amount"])));
        // Each name resolves to the archetype, or the block is a listed gap.
        for (name, english) in a.item_names() {
            match resolve(name, english) {
                Some(it) => assert_eq!(Archetype::of(it), Some(a), "{name}"),
                None => assert!(gaps["items"].get(*name).is_some(), "{name} is neither registered nor a gap"),
            }
        }
    }
    let swallowable_names: Vec<&str> =
        facts["tags"]["swallowable"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(swallowable_names.len(), Archetype::ALL.iter().map(|a| a.item_names().len()).sum::<usize>());
    assert_eq!(facts["tags"]["food"], serde_json::json!(["slime_ball"]));
    assert!(is_food(Item::SLIME_BALL) && !swallowable(Item::SLIME_BALL));
    for name in facts["tags"]["immune_with_block"].as_array().unwrap() {
        let t = DamageType::from_id(name.as_str().unwrap()).expect("a known damage type");
        assert!(immune_with_block(t), "{t:?}");
    }
    assert!(!immune_with_block(DamageType::OnFire) && !immune_with_block(DamageType::Magic));
    assert_eq!(facts["damage_type_hot"]["message_id"], "sulfurCubeHot");
}

#[test]
fn natural_entry_and_spawn_rules_follow_the_sulfur_caves() {
    let facts = facts();
    let entry = &facts["sulfur_caves_monster"][0];
    assert_eq!((entry["weight"].as_u64(), entry["count"]["min_inclusive"].as_u64()), (Some(100), Some(2)));
    assert_eq!(super::super::monster_spawns::JavaMonster::SulfurCube.kind(), Some(MobKind::SulfurCube));
}

#[test]
fn adults_swallow_blocks_shears_eject_them_and_babies_eat_slime_balls() {
    let mut e = Entities::new(2);
    let i = cube(&mut e, DVec3::new(0.5, 1.0, 0.5));
    let planks = Stack::new(item("oak_planks"), 5);
    assert_eq!(e.use_sulfur_cube(i, Some(planks)), Some(Use::Swallowed));
    assert_eq!(e.mobs[i].sulfur_body().map(|s| (s.item, s.count)), Some((planks.item, 1)));
    assert_eq!(e.mobs[i].sulfur_physics(), Some(Archetype::Bouncy.physics()));
    // The same block again is refused; a different one swaps it out.
    assert_eq!(e.use_sulfur_cube(i, Some(planks)), None);
    let stone = Stack::new(Block::STONE, 1);
    assert_eq!(e.use_sulfur_cube(i, Some(stone)), Some(Use::Swallowed));
    assert!(e.items.iter().any(|it| it.stack.item == planks.item), "the old block pops out");
    assert_eq!(e.use_sulfur_cube(i, Some(Stack::new(Item::SHEARS, 1))), Some(Use::Sheared));
    assert!(e.mobs[i].sulfur_body().is_none());
    assert_eq!(e.mobs[i].sulfur_cube.as_ref().unwrap().pickup_timer, PICKUP_TIMER);
    assert!(e.items.iter().any(|it| it.stack.item == Item::from_block(Block::STONE)));
    // Something it can't swallow does nothing; babies refuse blocks and eat slime balls.
    assert_eq!(e.use_sulfur_cube(i, Some(Stack::new(Item::STICK, 1))), None);
    let b = cube(&mut e, DVec3::new(4.5, 1.0, 0.5));
    e.mobs[b].set_sulfur_size(BABY_SIZE);
    assert_eq!(e.use_sulfur_cube(b, Some(stone)), None);
    assert!(!e.sulfur_swallow(b, stone));
    assert_eq!(e.use_sulfur_cube(b, Some(Stack::new(Item::SLIME_BALL, 1))), Some(Use::Fed));
    // `getSpeedUpSecondsWhenFeeding(24000)`: 120 seconds, 2400 ticks.
    assert_eq!(e.mobs[b].age, BABY_AGE + 2400);
    // A golden dandelion locks its age (the shared AgeableMob path).
    let dandelion = Stack::new(crate::world::gen5_blocks::GOLDEN_DANDELION, 1);
    assert_eq!(e.use_animal(b, Some(dandelion), PlayerId::HOST), Some(true));
    assert!(e.mobs[b].age_locked);
}

#[test]
fn babies_grow_up_into_size_two_unless_age_locked() {
    let mut e = Entities::new(3);
    let w = Grid::flat(0);
    let i = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
    e.mobs[i].set_sulfur_size(BABY_SIZE);
    e.mobs[i].age = -3;
    let c = ctx(vec![]);
    for _ in 0..4 {
        e.update(0.05, &w, &c);
    }
    assert_eq!((e.mobs[i].baby, e.mobs[i].size, e.mobs[i].health), (false, ADULT_SIZE, 8.0));
    let j = cube(&mut e, DVec3::new(4.5, 0.0, 0.5));
    e.mobs[j].set_sulfur_size(BABY_SIZE);
    e.mobs[j].age = -3;
    e.mobs[j].age_locked = true;
    for _ in 0..4 {
        e.update(0.05, &w, &c);
    }
    assert!(e.mobs[j].baby, "a locked baby stays one");
}

#[test]
fn a_block_turns_off_melee_and_projectile_damage_but_not_fire() {
    let mut e = Entities::new(4);
    let i = cube(&mut e, DVec3::new(0.5, 1.0, 0.5));
    // Without a block it is hurt like any mob.
    e.mobs[i].damage(2.0, Some(DVec3::X), &mut Rng::new(1));
    assert_eq!(e.mobs[i].health, 6.0);
    assert!(e.sulfur_swallow(i, Stack::new(Block::STONE, 1)));
    let mut rng = Rng::new(2);
    assert!(!e.mobs[i].damage(5.0, Some(DVec3::X), &mut rng));
    assert!(!e.mobs[i].damage_projectile(5.0, &[crate::knockback::Push::projectile(DVec3::X)], &mut rng));
    assert!(!e.mobs[i].blast(5.0, DVec3::X, &mut rng));
    assert_eq!(e.mobs[i].health, 6.0, "immune with a block");
    assert!(e.mobs[i].sulfur_shrugs_off(DamageType::PlayerAttack));
    assert!(!e.mobs[i].sulfur_shrugs_off(DamageType::OnFire));
}

#[test]
fn a_players_hit_sends_a_cube_with_a_block_flying_along_the_aim() {
    let mut e = Entities::new(5);
    let i = cube(&mut e, DVec3::new(0.5, 1.0, 0.5));
    assert!(e.sulfur_swallow(i, Stack::new(item("oak_planks"), 1)));
    let hit = crate::entity::PlayerMelee {
        held: None,
        bonus: 0.0,
        strength: 1.0,
        critical: false,
        sprint_knockback: false,
        origin: DVec3::new(-1.5, 1.0, 0.5),
        // Facing +X (Java yaw -90) and level.
        yaw: -90.0,
        sweep: false,
        owner: PlayerId::HOST,
        smash: 0.0,
        armor_effectiveness: 0.0,
        pitch: 0.0,
    };
    assert_eq!(e.player_melee(i, hit), None);
    let m = &e.mobs[i];
    assert_eq!(m.health, 8.0, "no damage");
    assert!(m.vel.x > 1.0 && m.vel.z.abs() < 1e-6, "pushed away along +X: {:?}", m.vel);
    assert!(m.vel.y > 0.0, "and up");
    // Bouncy: (1 - (-2)) x 0.4125 x sqrt(1) x 0.4 a tick.
    let expect = 3.0 * f64::from(0.4125_f32 * 0.4) * 20.0;
    assert!((m.vel.x - expect).abs() < 0.2, "{} vs {expect}", m.vel.x);
}

#[test]
fn ball_physics_bounces_slides_and_sticks_by_archetype() {
    // Dropped from 4 blocks: bouncy planks come back up, honeycomb sticks.
    let drop = |name: &str| {
        let mut e = Entities::new(6);
        let i = cube(&mut e, DVec3::new(0.5, 4.0, 0.5));
        e.mobs[i].on_ground = false;
        assert!(e.sulfur_swallow(i, Stack::new(item(name), 1)));
        let w = Grid::flat(0);
        let mut peak_after_landing: f64 = 0.0;
        let mut landed = false;
        for _ in 0..60 {
            e.mobs[i].sulfur_travel(0.05, &w);
            if e.mobs[i].on_ground {
                landed = true;
            } else if landed {
                peak_after_landing = peak_after_landing.max(e.mobs[i].pos.y);
            }
        }
        peak_after_landing
    };
    assert!(drop("oak_planks") > 0.5, "bouncy rebounds");
    assert_eq!(drop("honeycomb_block"), 0.0, "sticky doesn't");
    // Pushed along the ground: fast sliding (packed ice) outruns sticky.
    let slide = |name: &str| {
        let mut e = Entities::new(7);
        let i = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
        assert!(e.sulfur_swallow(i, Stack::new(item(name), 1)));
        e.mobs[i].vel = DVec3::new(10.0, 0.0, 0.0);
        let w = Grid::flat(0);
        for _ in 0..40 {
            e.mobs[i].sulfur_travel(0.05, &w);
        }
        e.mobs[i].pos.x
    };
    assert!(slide("packed_ice") > slide("honeycomb_block") + 1.0);
    // `computeModifiedFriction`.
    assert!((modified_friction(0.6, 0.05) - (1.0 - 0.4 * 0.05)).abs() < 1e-6);
    assert!((modified_friction(0.6, 2.0) - 0.2).abs() < 1e-6);
    assert_eq!(modified_friction(0.6, 3.0), 0.0);
}

#[test]
fn players_walking_into_a_cube_push_it_and_hot_cubes_burn_them() {
    let mut e = Entities::new(8);
    let i = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
    assert!(e.sulfur_swallow(i, Stack::new(item("magma_block"), 1)));
    let w = Grid::flat(0);
    let mut player = Target::new(PlayerId::HOST, DVec3::new(-0.3, 0.0, 0.5), true);
    e.update(0.05, &w, &ctx(vec![player]));
    player.pos.x += 0.2;
    let events = e.update(0.05, &w, &ctx(vec![player]));
    assert!(e.mobs[i].vel.x > 0.0, "pushed along +X: {:?}", e.mobs[i].vel);
    assert!(events.iter().any(|ev| matches!(
        ev,
        EntityEvent::PlayerHit { damage, cause, .. } if *damage == 1.0 && cause.kind == DamageType::SulfurCubeHot
    )));
}

#[test]
fn explosive_cubes_prime_burn_down_and_explode_without_splitting() {
    let mut e = Entities::new(9);
    let i = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
    assert!(e.sulfur_swallow(i, Stack::new(Block::TNT, 1)));
    assert_eq!(e.use_sulfur_cube(i, Some(Stack::new(Item::FLINT_AND_STEEL, 1))), Some(Use::Lit { charge: false }));
    assert_eq!(e.mobs[i].sulfur_cube.as_ref().unwrap().fuse, 120);
    assert!(e.mobs[i].refuses_hit(100.0), "primed cubes are invulnerable");
    assert_eq!(e.use_sulfur_cube(i, Some(Stack::new(Item::FLINT_AND_STEEL, 1))), None);
    let w = Grid::flat(0);
    let c = ctx(vec![]);
    let mut boom = None;
    for _ in 0..130 {
        for ev in e.update(0.05, &w, &c) {
            if let EntityEvent::Explosion { power, .. } = ev {
                boom = Some(power);
            }
        }
    }
    assert_eq!(boom, Some(3.0));
    assert_eq!(e.count(MobKind::SulfurCube), 0, "gone, with no babies");
    // With `tnt_explodes` off the lighting fails.
    let j = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
    assert!(e.sulfur_swallow(j, Stack::new(Block::TNT, 1)));
    e.tnt_explodes = false;
    assert_eq!(e.use_sulfur_cube(j, Some(Stack::new(Item::FIRE_CHARGE, 1))), Some(Use::Disabled));
    // An explosion primes it on a short fuse: nextInt(30) + 15.
    e.tnt_explodes = true;
    assert!(e.mobs[j].prime_sulfur_cube(true, true, &mut Rng::new(3)));
    assert!((15..45).contains(&e.mobs[j].sulfur_cube.as_ref().unwrap().fuse));
}

#[test]
fn a_dead_adult_splits_into_two_babies_and_drops_its_block() {
    let mut e = Entities::new(10);
    let i = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
    assert!(e.sulfur_swallow(i, Stack::new(item("oak_planks"), 1)));
    e.shear_sulfur_cube(i);
    assert!(e.sulfur_swallow(i, Stack::new(Block::STONE, 1)));
    e.items.clear();
    e.mobs[i].health = 0.0;
    e.mobs[i].dying = Some(0.0);
    let w = Grid::flat(0);
    let c = ctx(vec![]);
    for _ in 0..30 {
        e.update(0.05, &w, &c);
    }
    let babies: Vec<_> = e.mobs.iter().filter(|m| m.kind == MobKind::SulfurCube).collect();
    assert_eq!(babies.len(), 2);
    assert!(babies.iter().all(|m| m.baby && m.size == 1 && m.health == 4.0));
}

#[test]
fn buckets_scoop_adults_with_their_block_and_release_them() {
    let mut e = Entities::new(11);
    let i = cube(&mut e, DVec3::new(0.5, 1.0, 0.5));
    assert!(e.sulfur_swallow(i, Stack::new(item("oak_planks"), 1)));
    e.mobs[i].health = 6.5;
    let Some(Use::Bucketed(filled)) = e.use_sulfur_cube(i, Some(Stack::new(Item::BUCKET, 1))) else {
        panic!("an empty bucket scoops the cube");
    };
    assert_eq!(filled.item, BUCKET);
    assert_eq!(e.count(MobKind::SulfurCube), 0);
    // The payload survives the stack's save format.
    let text = crate::inventory::stack_to_string(Some(filled));
    let filled = crate::inventory::stack_from_str(&text).unwrap().unwrap();
    assert!(e.release_sulfur_cube(filled, DVec3::new(3.5, 1.0, 0.5)));
    let m = e.mobs.last().unwrap();
    assert_eq!(m.kind, MobKind::SulfurCube);
    assert_eq!(m.sulfur_body().map(|s| s.item), Some(item("oak_planks")));
    assert_eq!(m.health, 6.5);
    assert!(m.persistent && m.sulfur_cube.as_ref().unwrap().from_bucket);
    // Babies can't be scooped (`isBaby` takes the feeding branch).
    let b = cube(&mut e, DVec3::new(6.5, 1.0, 0.5));
    e.mobs[b].set_sulfur_size(BABY_SIZE);
    assert_eq!(e.use_sulfur_cube(b, Some(Stack::new(Item::BUCKET, 1))), None);
}

#[test]
fn sulfur_cubes_save_their_block_size_fuse_and_bucket_flag() {
    let mut e = Entities::new(12);
    let i = cube(&mut e, DVec3::new(0.5, 1.0, 0.5));
    assert!(e.sulfur_swallow(i, Stack::new(Block::TNT, 1)));
    assert!(e.mobs[i].prime_sulfur_cube(false, true, &mut Rng::new(1)));
    e.mobs[i].sulfur_cube.as_mut().unwrap().from_bucket = true;
    e.mobs[i].sulfur_cube.as_mut().unwrap().pickup_timer = 37;
    let b = cube(&mut e, DVec3::new(3.5, 1.0, 0.5));
    e.mobs[b].set_sulfur_size(BABY_SIZE);
    e.mobs[b].age = -1234;
    let mut restored = Entities::new(13);
    restored.load_nether_mobs(&e.nether_mobs_to_string());
    assert_eq!(restored.count(MobKind::SulfurCube), 2, "natural sulfur cubes are saved");
    let a = restored.mobs.iter().find(|m| !m.baby).unwrap();
    assert_eq!(a.sulfur_body().map(|s| s.item), Some(Item::from_block(Block::TNT)));
    let s = a.sulfur_cube.as_ref().unwrap();
    assert_eq!((s.fuse, s.from_bucket, s.pickup_timer, a.size), (120, true, 37, ADULT_SIZE));
    let baby = restored.mobs.iter().find(|m| m.baby).unwrap();
    assert_eq!((baby.size, baby.age, baby.health), (BABY_SIZE, -1234, 4.0));
    // The LAN look carries the block (equipment), the fuse and the size.
    let mut remote = Mob::new(MobKind::SulfurCube, DVec3::ZERO, 0.0);
    remote.apply_look(&e.mobs[i].look());
    assert_eq!(remote.sulfur_body().map(|s| s.item), Some(Item::from_block(Block::TNT)));
    assert_eq!(remote.sulfur_cube.as_ref().unwrap().fuse, 120);
}

#[test]
fn adults_hop_to_swallowable_items_and_take_them() {
    let mut e = Entities::new(14);
    let w = Grid::flat(0);
    let i = cube(&mut e, DVec3::new(0.5, 0.0, 0.5));
    e.drop_from_block(Stack::new(Block::STONE, 3), IVec3::new(4, 0, 0));
    for it in &mut e.items {
        it.pickup_delay = 0.0;
    }
    let c = ctx(vec![]);
    for _ in 0..400 {
        e.update(0.05, &w, &c);
        if e.mobs[i].sulfur_has_block() {
            break;
        }
    }
    assert_eq!(e.mobs[i].sulfur_body().map(|s| (s.item, s.count)), Some((Item::from_block(Block::STONE), 1)));
    assert_eq!(e.items.iter().map(|it| u32::from(it.stack.count)).sum::<u32>(), 2, "one taken");
    assert!(e.mobs[i].persistent);
}

/// Generation 5, seed 12345: natural spawning in a real sulfur cave
/// (generated chunks around the nearest one) makes sulfur cubes from the
/// biome's monster list, in the dark, whatever the light.
#[test]
fn generation_5_sulfur_caves_spawn_sulfur_cubes() {
    use crate::world::chunk::CHUNK_SIZE_I;
    use crate::world::terrain::{Biome, Generation, Generator};
    use std::sync::Arc;
    let g = Arc::new(Generator::with_generation(12345, Dimension::Overworld, Generation::V5));
    let at = g.nearest_biome(glam::IVec2::ZERO, Biome::SulfurCaves, 6000).expect("a sulfur cave");
    let mut world = crate::world::World::new_headless(g.clone(), Default::default(), 2);
    let column = glam::IVec2::new(at.x.div_euclid(CHUNK_SIZE_I), at.z.div_euclid(CHUNK_SIZE_I));
    for dz in -1..=1 {
        for dx in -1..=1 {
            for cy in Dimension::Overworld.chunk_rows() {
                let pos = IVec3::new(column.x + dx, cy, column.y + dz);
                world.insert_chunk(pos, Arc::new(g.generate(pos)), false);
            }
        }
    }
    // Open floors in the cave around the found spot.
    let floor = |p: IVec3| {
        world.get_block(p).is_some_and(|b| b == Block::AIR)
            && world.get_block(p + IVec3::Y).is_some_and(|b| b == Block::AIR)
            && world.get_block(p - IVec3::Y).is_some_and(|b| b.is_solid())
            && world.cave_biome(p) == Biome::SulfurCaves
    };
    let mut starts = Vec::new();
    for dx in -12..=12 {
        for dz in -12..=12 {
            for dy in -24..=24 {
                let p = IVec3::new(at.x + dx, at.y + dy, at.z + dz);
                if floor(p) {
                    starts.push(p);
                }
            }
        }
    }
    assert!(!starts.is_empty(), "no open floor in the sulfur cave at {at}");
    // A player 30 blocks above, far enough to allow spawning.
    let eye = starts[0].as_dvec3() + DVec3::new(0.5, 30.0, 0.5);
    let c = Ctx { daylight: 1.0, spawning: true, ..ctx(vec![Target::new(PlayerId::HOST, eye, true)]) };
    let rules =
        super::super::monster_spawns::SpawnRules { dimension: Dimension::Overworld, light: SpawnLight::DAY, moon: 1.0 };
    let mut e = Entities::new(21);
    let mut state = super::super::category::SpawnState::new(&[eye], std::iter::empty(), |_| true);
    for round in 0..200 {
        let start = starts[round % starts.len()];
        e.category_packs(&world, &c, start, rules, &mut state, MobCategory::Monster, &[]);
        if e.count(MobKind::SulfurCube) > 0 {
            break;
        }
    }
    let cubes: Vec<_> = e.mobs.iter().filter(|m| m.kind == MobKind::SulfurCube).collect();
    assert!(!cubes.is_empty(), "sulfur cubes spawn in the sulfur cave at {at} ({} starts)", starts.len());
    assert!(cubes.iter().all(|m| m.size == ADULT_SIZE || m.baby));
}
