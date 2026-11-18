//! The illagers, vexes, ravagers and shared mob combat against
//! `tests/fixtures/java/26.3/illagers.json`, in both directions: each
//! constant is read back from the fixture, and behaviour is checked through
//! the entity update.
use super::*;
use crate::{
    enchant::{Enchantment, Enchants},
    entity::{PlayerId, Target, category::MobCategory, equipment::Slot, item::ItemEntity, mob::goals, mob_combat},
    inventory::{Stack, StackName},
    item::{ArmorMaterial, ArmorPiece},
    physics::test_util::Grid,
    simulation::{difficulty::Difficulty, effects::Effect, lighting::SpawnLight},
    world::terrain::Dimension,
};
use serde_json::Value;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/illagers.json")).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        players,
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

fn player(pos: DVec3) -> Target {
    Target::new(PlayerId::HOST, pos, true)
}

/// A mob of `kind` standing at `pos`, with its spawn equipment but no
/// enchantments.
fn mob(e: &mut Entities, kind: MobKind, pos: DVec3) -> usize {
    e.spawn(kind, pos);
    let m = e.mobs.last_mut().unwrap();
    m.on_ground = true;
    if let Some(s) = m.equipment.get_mut(Slot::MainHand) {
        s.enchants = Enchants::NONE;
    }
    e.ensure_mob_ids();
    e.mobs.len() - 1
}

const KINDS: [(&str, MobKind); 6] = [
    ("vindicator", MobKind::Vindicator),
    ("evoker", MobKind::Evoker),
    ("illusioner", MobKind::Illusioner),
    ("pillager", MobKind::Pillager),
    ("vex", MobKind::Vex),
    ("ravager", MobKind::Ravager),
];

#[test]
fn registration_attributes_and_experience_match_java() {
    let facts = facts();
    for (id, kind) in KINDS {
        let m = &facts["mobs"][id];
        assert_eq!(MobKind::from_name(id), Some(kind), "{id}");
        assert_eq!(MobKind::from_name(&format!("minecraft:{id}")), Some(kind));
        assert_eq!(kind.name(), id, "saved under Java's registry name");
        assert_eq!(MobKind::from_lan_index(kind.lan_index()), Some(kind));
        let t = &m["entity_type"];
        assert!((kind.shape().half_width * 2.0 - f(&t["width"])).abs() < 1e-6, "{id} width");
        assert!((kind.shape().height - f(&t["height"])).abs() < 1e-6, "{id} height");
        assert_eq!(t["category"], "monster");
        assert_eq!(kind.category(), MobCategory::Monster, "{id}");
        assert!(kind.is_hostile());
        assert_eq!(kind.despawns_in_peaceful(), !t["allowed_in_peaceful"].as_bool().unwrap(), "{id}");
        assert_eq!(kind.fire_immune(), t["fire_immune"].as_bool().unwrap(), "{id} fire immunity");
        assert!(!kind.spawns_in(Dimension::Overworld), "{id} has no natural spawn here");
        let a = &m["attributes"];
        assert_eq!(f64::from(kind.max_health()), f(&a["max_health"]), "{id} health");
        assert_eq!(kind.follow_range_base(), f(&a["follow_range"]), "{id} follow range");
        assert_eq!(follow_range(kind), f(&a["follow_range"]));
        // `ATTACK_DAMAGE` without a value is the registry default 2.
        let damage = a["attack_damage"].as_f64().unwrap_or(2.0);
        assert_eq!(f64::from(kind.melee().0), damage, "{id} attack damage");
        // `MOVEMENT_SPEED` without a value is the registry default 0.7.
        let speed = a.get("movement_speed").and_then(Value::as_f64).unwrap_or(0.7);
        assert_eq!(goals::movement_speed(kind), speed, "{id} speed");
        let mut rng = Rng::new(1);
        assert_eq!(u64::from(kind.xp(&mut rng)), m["experience"].as_u64().unwrap(), "{id} experience");
    }
    let r = &facts["mobs"]["ravager"]["attributes"];
    let m = Mob::new(MobKind::Ravager, DVec3::ZERO, 0.0);
    assert_eq!(m.knockback_resistance(), f(&r["knockback_resistance"]));
    assert_eq!(m.attack_knockback(), f(&r["attack_knockback"]));
    assert_eq!(MobKind::Ravager.nav_caps(1.95, 2.2).step, f(&r["step_height"]) as f32);
    assert_eq!(VEX_EYE, f(&facts["mobs"]["vex"]["entity_type"]["eye_height"]));
}

#[test]
fn illager_tags_are_exactly_javas() {
    let facts = facts();
    let tag = |name: &str| -> Vec<String> {
        facts["entity_type_tags"][name].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()
    };
    for kind in MobKind::ALL {
        let id = kind.name().replace(' ', "_");
        assert_eq!(tag("illager").contains(&id), is_illager(kind), "{id} in #illager");
        assert_eq!(tag("illager_friends").contains(&id), is_illager(kind), "{id} in #illager_friends");
        assert_eq!(tag("raiders").contains(&id), is_raider(kind), "{id} in #raiders");
    }
}

#[test]
fn loot_follows_the_entity_tables() {
    let facts = facts();
    let pools = |id: &str| facts["mobs"][id]["loot_table"]["pools"].as_array().unwrap().clone();
    assert!(pools("vex").is_empty() && pools("illusioner").is_empty());
    let mut rng = Rng::new(7);
    for (id, kind) in KINDS {
        for player_kill in [false, true] {
            for looting in 0..4u8 {
                for _ in 0..200 {
                    let mut out = Vec::new();
                    loot(kind, player_kill, looting, &mut rng, &mut out);
                    for pool in pools(id) {
                        let conditions = pool["conditions"].as_array().unwrap();
                        let needs_player = conditions.iter().any(|c| c == "killed_by_player");
                        let captain = conditions.iter().any(|c| c == "raid_captain");
                        let entry = &pool["entries"][0];
                        let Some(item) = Item::from_name(entry["item"].as_str().unwrap()) else {
                            assert!(captain, "{id}: {entry} names an unregistered item");
                            continue;
                        };
                        let n = out.iter().filter(|(i, _)| *i == item).map(|(_, n)| u32::from(*n)).sum::<u32>();
                        let (lo, hi) = (entry["count"][0].as_u64().unwrap(), entry["count"][1].as_u64().unwrap());
                        let extra = entry["looting"][1].as_f64().map_or(0, |m| (m * f64::from(looting)) as u64);
                        if captain || needs_player && !player_kill {
                            assert_eq!(n, 0, "{id}: {item:?} needs a player kill or a captain");
                        } else {
                            assert!(u64::from(n) <= hi + extra, "{id}: {n} {item:?}");
                            assert!(lo == 0 || u64::from(n) >= lo, "{id}: {n} {item:?} below {lo}");
                        }
                    }
                }
            }
        }
    }
    // The totem always drops; the emerald's 0-1 roll with Looting III
    // reaches 4.
    let mut out = Vec::new();
    loot(MobKind::Evoker, false, 0, &mut rng, &mut out);
    assert_eq!(out, vec![(Item::TOTEM_OF_UNDYING, 1)]);
    let most = (0..500)
        .map(|_| {
            let mut out = Vec::new();
            loot(MobKind::Vindicator, true, 3, &mut rng, &mut out);
            out.first().map_or(0, |d| d.1)
        })
        .max()
        .unwrap();
    assert_eq!(most, 4);
}

#[test]
fn spell_goals_match_java() {
    let facts = facts();
    let s = &facts["spells"];
    assert_eq!(u64::from(DEFAULT_WARMUP), s["default_warmup"].as_u64().unwrap());
    for (key, goal) in [
        ("evoker_summon", SUMMON),
        ("evoker_fangs", FANGS),
        ("evoker_wololo", WOLOLO),
        ("illusioner_mirror", MIRROR),
        ("illusioner_blindness", BLINDNESS),
    ] {
        let j = &s[key];
        assert_eq!(u64::from(goal.warmup), j["warmup"].as_u64().unwrap(), "{key} warmup");
        assert_eq!(i64::from(goal.casting), j["casting_time"].as_i64().unwrap(), "{key} casting");
        assert_eq!(u64::from(goal.interval), j["interval"].as_u64().unwrap(), "{key} interval");
        let name = format!("{:?}", goal.spell).to_lowercase();
        let java = j["spell"].as_str().unwrap().replace('_', "");
        assert_eq!(name, java, "{key} spell");
    }
    let summon = &s["evoker_summon"];
    assert_eq!(u64::from(SUMMON_VEXES), summon["vexes"].as_u64().unwrap());
    assert_eq!(u64::from(SUMMON_ROLL), summon["roll"].as_u64().unwrap());
    assert_eq!(VEX_COUNT_RANGE, f(&summon["count_range"]));
    assert_eq!(i64::from(VEX_LIFE_MIN_SECONDS), summon["life_min_seconds"].as_i64().unwrap());
    assert_eq!(u64::from(VEX_LIFE_SPREAD_SECONDS), summon["life_spread_seconds"].as_u64().unwrap());
    let wololo = &s["evoker_wololo"];
    assert_eq!(WOLOLO_RANGE, f(&wololo["range"]));
    assert_eq!(WOLOLO_RANGE, f(&wololo["box_horizontal"]));
    assert_eq!(WOLOLO_BOX_VERTICAL, f(&wololo["box_vertical"]));
    assert_eq!(u64::from(INVISIBILITY_TICKS), s["illusioner_mirror"]["invisibility_ticks"].as_u64().unwrap());
    assert_eq!(u64::from(BLINDNESS_TICKS), s["illusioner_blindness"]["blindness_ticks"].as_u64().unwrap());
    assert_eq!(spell_goals(MobKind::Evoker), &[SUMMON, FANGS, WOLOLO]);
    assert_eq!(spell_goals(MobKind::Illusioner), &[MIRROR, BLINDNESS]);
    // Ids follow `IllagerSpell`'s order; unknown ids read as none.
    assert_eq!(Spell::from_id(3), Spell::Wololo);
    assert_eq!(Spell::from_id(99), Spell::None);
}

#[test]
fn fang_layouts_and_bites_match_java() {
    let facts = facts();
    let s = &facts["spells"]["evoker_fangs"];
    assert_eq!(FANG_CLOSE_SQR, f(&s["close_distance_sqr"]));
    assert_eq!(u64::from(FANG_INNER.0), s["inner_count"].as_u64().unwrap());
    assert_eq!(FANG_INNER.1, f(&s["inner_radius"]));
    assert_eq!(u64::from(FANG_OUTER.0), s["outer_count"].as_u64().unwrap());
    assert_eq!(FANG_OUTER.1, f(&s["outer_radius"]));
    assert_eq!(i64::from(FANG_OUTER.2), s["outer_delay"].as_i64().unwrap());
    assert_eq!(u64::from(FANG_LINE.0), s["line_count"].as_u64().unwrap());
    assert_eq!(FANG_LINE.1, f(&s["line_spacing"]));
    // Close: five at 1.5 blocks 0.4π apart, then eight at 2.5 blocks.
    let close = fang_layout(0.0, 4.0);
    assert_eq!(close.len(), 13);
    assert!(close[..5].iter().all(|&(x, z, _, d)| ((x * x + z * z).sqrt() - 1.5).abs() < 1e-6 && d == 0));
    assert!(close[5..].iter().all(|&(x, z, _, d)| ((x * x + z * z).sqrt() - 2.5).abs() < 1e-6 && d == 3));
    assert!((close[1].2 - std::f32::consts::PI * 0.4).abs() < 1e-6);
    assert!((close[5].2 - f(&s["outer_offset"]) as f32).abs() < 1e-6);
    // Far: sixteen along the line, a tick apart.
    let line = fang_layout(std::f32::consts::FRAC_PI_2, 100.0);
    assert_eq!(line.len(), 16);
    for (i, &(x, z, _, d)) in line.iter().enumerate() {
        assert!(x.abs() < 1e-6 && (z - 1.25 * (i + 1) as f64).abs() < 1e-6 && d == i as i32);
    }
    let fangs = &facts["fangs"];
    assert_eq!(i64::from(FANG_LIFE), fangs["life_ticks"].as_i64().unwrap());
    assert_eq!(i64::from(FANG_BITE_AT), fangs["attack_at_warmup"].as_i64().unwrap());
    assert_eq!(f64::from(FANG_DAMAGE), f(&fangs["damage"]));
    assert_eq!(FANG_INFLATE, f(&fangs["inflate"][0]));
    assert_eq!(FANG_SHAPE.half_width * 2.0, f(&fangs["entity_type"]["width"]));
    assert!((FANG_SHAPE.height - f(&fangs["entity_type"]["height"])).abs() < 1e-6);
    // The ground search stands a fang on the first sturdy top below.
    let world = Grid::flat(1);
    assert_eq!(fang_ground(&world, 0.5, 0.5, 1.0, 3.0), Some(1.0));
    assert_eq!(fang_ground(&world, 0.5, 0.5, 5.0, 6.0), None, "no ground within reach");
}

#[test]
fn vex_ravager_and_crossbow_constants_match_java() {
    let facts = facts();
    let v = &facts["vex"];
    assert_eq!(i64::from(VEX_STARVE_INTERVAL), v["starve_interval"].as_i64().unwrap());
    assert_eq!(f64::from(VEX_STARVE_DAMAGE), f(&v["starve_damage"]));
    assert_eq!(u64::from(VEX_CHARGE_ROLL), v["charge"]["roll"].as_u64().unwrap());
    assert_eq!(VEX_CHARGE_MIN_SQR, f(&v["charge"]["min_distance_sqr"]));
    assert_eq!(VEX_REAIM_SQR, f(&v["charge"]["reaim_distance_sqr"]));
    assert_eq!(VEX_WANDER_SPEED, f(&v["wander"]["speed"]));
    assert_eq!(VEX_ACCELERATION, f(&v["move"]["acceleration"]));
    assert_eq!(VEX_ARRIVE_DAMPING, f(&v["move"]["arrive_damping"]));
    let r = &facts["ravager"];
    assert_eq!(i64::from(RAVAGER_ATTACK_TICKS), r["attack_ticks"].as_i64().unwrap());
    assert_eq!(i64::from(RAVAGER_STUN_TICKS), r["stun_ticks"].as_i64().unwrap());
    assert_eq!(i64::from(RAVAGER_ROAR_TICKS), r["roar_ticks"].as_i64().unwrap());
    assert_eq!(i64::from(RAVAGER_ROAR_AT), r["roar_at"].as_i64().unwrap());
    assert_eq!(f64::from(RAVAGER_ROAR_DAMAGE), f(&r["roar_damage"]));
    assert_eq!(RAVAGER_ROAR_RADIUS, f(&r["roar_radius"]));
    assert_eq!(RAVAGER_PUSH, f(&r["knockback"]["push"]));
    assert_eq!(RAVAGER_PUSH_UP, f(&r["knockback"]["push_up"]));
    assert_eq!(RAVAGER_PUSH_MIN_SQR, f(&r["knockback"]["min_distance_sqr"]));
    assert_eq!(RAVAGER_STUN_CHANCE, f(&r["stun_chance"]));
    assert_eq!(RAVAGER_BASE_SPEED, f(&r["base_speed"]));
    assert_eq!(RAVAGER_ATTACK_SPEED, f(&r["attack_speed"]));
    assert_eq!(RAVAGER_SPEED_LERP, f(&r["speed_lerp"]));
    assert_eq!(RAVAGER_GRIEF_INFLATE, f(&r["grief_inflate"]));
    assert_eq!(RAVAGER_ATTACK_DEFLATE, f(&r["attack_box_deflate"]));
    // `strongKnockback` divides by the squared distance: 4 blocks a tick at
    // one block, 2 at two.
    let p = ravager_push(DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0));
    assert!((p.x - 2.0).abs() < 1e-9 && p.y == RAVAGER_PUSH_UP);
    let c = &facts["crossbow"];
    assert_eq!(charge_ticks(0), (f(&c["charge_seconds"]) * 20.0) as u32);
    assert_eq!((charge_ticks(1), charge_ticks(3)), (20, 10), "Quick Charge takes 5 ticks a level");
    assert_eq!(MOB_ARROW_POWER, f(&c["mob_arrow_power"]) as f32 as f64);
    assert_eq!(CROSSBOW_RADIUS, f(&c["attack_radius"]));
    assert_eq!(i64::from(CROSSBOW_SEE_TIME), c["see_time_min"].as_i64().unwrap());
    assert_eq!(u64::from(CROSSBOW_DELAY_BASE), c["attack_delay_base"].as_u64().unwrap());
    assert_eq!(u64::from(CROSSBOW_DELAY_SPREAD), c["attack_delay_spread"].as_u64().unwrap());
    assert_eq!(u64::from(CROSSBOW_PATH_DELAY.0), c["path_delay_min_seconds"].as_u64().unwrap() * 20);
    assert_eq!(u64::from(CROSSBOW_PATH_DELAY.1), c["path_delay_max_seconds"].as_u64().unwrap() * 20);
    // `direction` starts at 1 and flips after each, so the second arrow
    // goes to -10.
    assert_eq!(multishot_angles(3, MULTISHOT_SPREAD), vec![0.0, -10.0, 10.0]);
    assert_eq!(multishot_angles(1, MULTISHOT_SPREAD), vec![0.0]);
    assert_eq!(uncertainty(Difficulty::Normal), 6.0);
    let d = &facts["vindicator"];
    assert_eq!(JOHNNY, d["johnny_name"].as_str().unwrap());
    assert_eq!(u64::from(DOOR_BREAK_TIME), d["door_break_time"].as_u64().unwrap());
    assert!(breaks_doors(Difficulty::Hard, true) && breaks_doors(Difficulty::Normal, true));
    assert!(!breaks_doors(Difficulty::Easy, true) && !breaks_doors(Difficulty::Hard, false), "raids only");
}

#[test]
fn shared_mob_combat_constants_match_java() {
    let facts = facts();
    let c = &facts["mob_combat"];
    assert_eq!(mob_combat::PICKUP_REACH, [0, 1, 2].map(|i| f(&c["pickup_reach"][i])));
    // Java `float`s, widened as the game reads them.
    let java_float = |v: &Value| f64::from(f(v) as f32);
    assert_eq!(f64::from(mob_combat::CAN_PICK_UP_LOOT_CHANCE), java_float(&c["zombie_can_pick_up_loot_chance"]));
    assert_eq!(f64::from(mob_combat::CAN_PICK_UP_LOOT_CHANCE), java_float(&c["skeleton_can_pick_up_loot_chance"]));
    assert_eq!(f64::from(mob_combat::REPLACE_DROP_OFFSET), f(&c["replace_drop_offset"]) as f32 as f64);
    let reach = f(&c["attack_reach"]["sqrt_of"]).sqrt() - f(&c["attack_reach"]["minus"]);
    assert_eq!(crate::entity::endermite::ATTACK_REACH, reach);
}

#[test]
fn held_weapons_add_their_damage_and_enchantments_to_mob_hits() {
    let sword = |tier, enchants| Stack { enchants, ..Stack::new(Item::tool(ToolKind::Sword, tier), 1) };
    let mut z = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    assert_eq!(z.melee_attack().0, 3.0);
    z.equipment.set(Slot::MainHand, Some(sword(Tier::Iron, Enchants::NONE)));
    assert_eq!(z.melee_attack().0, 8.0, "an iron sword adds 5");
    z.equipment.set(Slot::MainHand, Some(sword(Tier::Iron, Enchants::NONE.with(Enchantment::Sharpness, 2))));
    assert_eq!(z.melee_attack().0, 9.5, "Sharpness II adds 1.5");
    z.equipment.set(Slot::MainHand, Some(sword(Tier::Iron, Enchants::NONE.with(Enchantment::Knockback, 2))));
    assert_eq!(z.attack_knockback(), 2.0);
    z.equipment.set(Slot::MainHand, Some(sword(Tier::Iron, Enchants::NONE.with(Enchantment::FireAspect, 2))));
    assert_eq!(z.weapon_fire_seconds(), 8.0);
    // Java's vindicator with its iron axe (5 + 8) and vex with its iron
    // sword (4 + 5).
    let mut e = Entities::new(1);
    let v = mob(&mut e, MobKind::Vindicator, DVec3::ZERO);
    assert_eq!(e.mobs[v].main_hand().map(|s| s.item), Some(Item::tool(ToolKind::Axe, Tier::Iron)));
    assert_eq!(e.mobs[v].melee_attack().0, 13.0);
    let x = mob(&mut e, MobKind::Vex, DVec3::ZERO);
    assert_eq!(e.mobs[x].melee_attack().0, 9.0);
    assert_eq!(e.mobs[x].equipment.drop_chance(Slot::MainHand), 0.0, "the vex's sword never drops");
    let p = mob(&mut e, MobKind::Pillager, DVec3::ZERO);
    assert_eq!(e.mobs[p].main_hand().map(|s| s.item), Some(Item::CROSSBOW));
    let i = mob(&mut e, MobKind::Illusioner, DVec3::ZERO);
    assert_eq!(e.mobs[i].main_hand().map(|s| s.item), Some(Item::BOW));
    let ev = mob(&mut e, MobKind::Evoker, DVec3::ZERO);
    assert!(e.mobs[ev].main_hand().is_none());
    assert!(e.mobs[ev].illager.as_ref().unwrap().raider.can_join_raid, "raiders may join raids");
}

#[test]
fn worn_mob_armor_and_its_protection_reduce_attacks() {
    let mut rng = Rng::new(3);
    let armored = |enchants: Enchants| {
        let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
        m.health = 100.0;
        for piece in ArmorPiece::ALL {
            let mut s = Stack::new(Item::armor(piece, ArmorMaterial::Iron), 1);
            s.enchants = enchants;
            m.equipment.set(Slot::of_piece(piece), Some(s));
        }
        m
    };
    let mut m = armored(Enchants::NONE);
    assert_eq!(m.worn_armor(), (15, 0.0));
    m.damage(10.0, Some(DVec3::X), &mut rng);
    let expected = crate::simulation::survival::armor_reduce(10.0, 15, 0.0);
    assert!((100.0 - m.health - expected).abs() < 1e-4, "{} taken", 100.0 - m.health);
    // Fire and other unattributed damage go straight through.
    let mut m = armored(Enchants::NONE);
    m.damage(4.0, None, &mut rng);
    assert_eq!(m.health, 96.0);
    // Protection I on four pieces: 4 points, 16% less after the armor.
    let mut m = armored(Enchants::NONE.with(Enchantment::Protection, 1));
    m.damage(10.0, Some(DVec3::X), &mut rng);
    assert!((100.0 - m.health - expected * (1.0 - 4.0 / 25.0)).abs() < 1e-4);
    // Projectile Protection counts against arrows only.
    let mut m = armored(Enchants::NONE.with(Enchantment::ProjectileProtection, 1));
    m.damage_projectile(10.0, &[], &mut rng);
    assert!((100.0 - m.health - expected * (1.0 - 8.0 / 25.0)).abs() < 1e-4);
    // Netherite resists knockback, 0.1 a piece.
    let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    m.equipment.wear_set(super::super::armor::ArmorKind::Netherite, false);
    assert!((m.knockback_resistance() - 0.4).abs() < 1e-6);
}

#[test]
fn mobs_that_can_pick_up_loot_equip_better_items() {
    let world = Grid::flat(1);
    let iron = Stack::new(Item::tool(ToolKind::Sword, Tier::Iron), 1);
    let setup = |griefing: bool, held: Option<Stack>| {
        let mut e = Entities::new(5);
        e.villager_griefing = griefing;
        let i = mob(&mut e, MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[i].can_pick_up_loot = true;
        e.mobs[i].equipment = Default::default();
        e.mobs[i].equipment.set(Slot::MainHand, held);
        let mut rng = Rng::new(1);
        let mut item = ItemEntity::new(iron, DVec3::new(1.2, 1.0, 0.5), DVec3::ZERO, 0.0, &mut rng);
        item.pickup_delay = 0.0;
        e.items.push(item);
        e
    };
    let mut e = setup(true, None);
    e.update(0.05, &world, &ctx(vec![]));
    let z = e.mobs.iter().find(|m| m.kind == MobKind::Zombie).unwrap();
    assert_eq!(z.main_hand(), Some(iron));
    assert!(z.equipment.is_preserved(Slot::MainHand), "picked-up gear always drops");
    assert!(z.persistent);
    assert!(e.items.is_empty());
    // No `mobGriefing`, no pickup.
    let mut e = setup(false, None);
    e.update(0.05, &world, &ctx(vec![]));
    assert!(e.mobs[0].main_hand().is_none());
    // A diamond sword stays over an iron one.
    let diamond = Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1);
    let mut e = setup(true, Some(diamond));
    e.update(0.05, &world, &ctx(vec![]));
    assert_eq!(e.mobs[0].main_hand(), Some(diamond));
    assert_eq!(e.items.len(), 1);
    // `canReplaceCurrentItem`: preferred weapons, armor, then the tiebreak.
    use mob_combat::can_replace;
    let stone = Stack::new(Item::tool(ToolKind::Sword, Tier::Stone), 1);
    assert!(can_replace(MobKind::Zombie, &iron, Some(&stone), Slot::MainHand));
    assert!(!can_replace(MobKind::Zombie, &stone, Some(&iron), Slot::MainHand));
    let bow = Stack::new(Item::BOW, 1);
    assert!(!can_replace(MobKind::Skeleton, &iron, Some(&bow), Slot::MainHand), "skeletons keep bows");
    assert!(can_replace(MobKind::Skeleton, &bow, Some(&iron), Slot::MainHand));
    let helmet = |m| Stack::new(Item::armor(ArmorPiece::Helmet, m), 1);
    assert!(can_replace(
        MobKind::Zombie,
        &helmet(ArmorMaterial::Iron),
        Some(&helmet(ArmorMaterial::Leather)),
        Slot::Head
    ));
    let enchanted = Stack { enchants: Enchants::NONE.with(Enchantment::Unbreaking, 1), ..helmet(ArmorMaterial::Iron) };
    assert!(can_replace(MobKind::Zombie, &enchanted, Some(&helmet(ArmorMaterial::Iron)), Slot::Head));
    let worn = Stack { damage: 5, ..helmet(ArmorMaterial::Iron) };
    assert!(can_replace(MobKind::Zombie, &helmet(ArmorMaterial::Iron), Some(&worn), Slot::Head));
    assert!(!mob_combat::wants(MobKind::Zombie, Item::GLOW_INK_SAC, false, false));
    assert!(!mob_combat::wants(MobKind::Skeleton, crate::weapons::spear(Tier::Iron), false, false));
    // Zombies roll `CanPickUpLoot` at 0.55 x the special multiplier.
    let mut e = Entities::new(11);
    e.difficulty = Difficulty::Hard;
    let mut picks = 0;
    for _ in 0..400 {
        e.spawn(MobKind::Zombie, DVec3::ZERO);
        picks += usize::from(e.mobs.last().unwrap().can_pick_up_loot);
    }
    assert!(picks < 400, "{picks} of 400");
}

#[test]
fn a_vindicator_swings_its_axe_at_a_player_and_johnny_attacks_anything() {
    let world = Grid::flat(1);
    let mut e = Entities::new(21);
    mob(&mut e, MobKind::Vindicator, DVec3::new(0.5, 1.0, 0.5));
    let c = ctx(vec![player(DVec3::new(1.9, 1.0, 0.5))]);
    let mut hits = Vec::new();
    for _ in 0..120 {
        for ev in e.update(0.05, &world, &c) {
            if let EntityEvent::PlayerHit { damage, .. } = ev {
                hits.push(damage);
            }
        }
    }
    assert!(!hits.is_empty(), "the vindicator attacks");
    assert!(hits.iter().all(|&d| d == 13.0), "{hits:?}");
    assert!(hits.len() <= 7, "one swing every 20 ticks: {}", hits.len());
    // Named Johnny, it goes for a cow; plain, it doesn't.
    for johnny in [false, true] {
        let mut e = Entities::new(22);
        let v = mob(&mut e, MobKind::Vindicator, DVec3::new(0.5, 1.0, 0.5));
        if johnny {
            e.mobs[v].name = StackName::new(JOHNNY).unwrap();
            named(&mut e.mobs[v]);
        }
        mob(&mut e, MobKind::Cow, DVec3::new(2.5, 1.0, 0.5));
        let mut hurt = false;
        for _ in 0..300 {
            e.update(0.05, &world, &ctx(vec![]));
            hurt |= e.mobs.iter().any(|m| m.kind == MobKind::Cow && m.health < 10.0);
        }
        assert_eq!(hurt, johnny, "johnny {johnny}");
    }
}

#[test]
fn evokers_summon_vexes_then_raise_fangs() {
    let world = Grid::flat(1);
    let mut e = Entities::new(31);
    let ev = mob(&mut e, MobKind::Evoker, DVec3::new(0.5, 1.0, 0.5));
    let owner = e.mobs[ev].uid;
    // Six blocks along +x: a line of fangs reaches it.
    let c = ctx(vec![player(DVec3::new(6.75, 1.0, 0.5))]);
    let mut fang_hits = 0;
    let mut saw_fangs = false;
    // Under the 340-tick summon interval, so one summon at most.
    for _ in 0..300 {
        // Keep the evoker in place so the line keeps its geometry.
        let m = e.mobs.iter_mut().find(|m| m.uid == owner).unwrap();
        m.pos = DVec3::new(0.5, 1.0, 0.5);
        for ev in e.update(0.05, &world, &c) {
            if let EntityEvent::PlayerHit { damage, cause, .. } = ev
                && cause == FANGS_CAUSE
            {
                assert_eq!(damage, FANG_DAMAGE);
                fang_hits += 1;
            }
        }
        saw_fangs |= !e.fangs.is_empty();
    }
    let vexes: Vec<_> = e.mobs.iter().filter(|m| m.kind == MobKind::Vex).collect();
    assert_eq!(vexes.len(), 3, "one summon of three");
    for v in &vexes {
        let s = v.illager.as_ref().unwrap();
        assert_eq!(s.owner, Some(owner));
        assert!(s.bound.is_some());
        assert!(s.life.is_some_and(|l| l <= 20 * (30 + 89)), "{:?}", s.life);
    }
    assert!(saw_fangs, "fangs rise");
    assert!(fang_hits >= 1, "a fang bites the player");
}

#[test]
fn wololo_turns_a_blue_sheep_red_only_with_mob_griefing() {
    let world = Grid::flat(1);
    for griefing in [true, false] {
        let mut e = Entities::new(41);
        e.villager_griefing = griefing;
        mob(&mut e, MobKind::Evoker, DVec3::new(0.5, 1.0, 0.5));
        let s = mob(&mut e, MobKind::Sheep, DVec3::new(4.5, 1.0, 0.5));
        e.mobs[s].wool_color = crate::color::DyeColor::Blue;
        let uid = e.mobs[s].uid;
        for _ in 0..200 {
            e.update(0.05, &world, &ctx(vec![]));
        }
        let sheep = e.mobs.iter().find(|m| m.uid == uid).unwrap();
        let expected = if griefing { crate::color::DyeColor::Red } else { crate::color::DyeColor::Blue };
        assert_eq!(sheep.wool_color, expected, "griefing {griefing}");
    }
}

#[test]
fn vexes_fly_through_walls_and_starve_when_their_life_runs_out() {
    // A solid block between the vex and where it wants to be.
    let mut world = Grid::flat(1);
    for y in 1..6 {
        world.set(IVec3::new(2, y, 0), Block::STONE);
    }
    let mut m = Mob::new(MobKind::Vex, DVec3::new(0.5, 2.0, 0.5), 0.0);
    m.illager.as_mut().unwrap().wanted = Some((DVec3::new(4.5, 2.0, 0.5), 1.0));
    for _ in 0..80 {
        vex_travel(&mut m, TICK);
    }
    assert!(m.pos.x > 3.5, "flew through the wall to {}", m.pos.x);
    let _ = &world;
    let mut e = Entities::new(51);
    let v = mob(&mut e, MobKind::Vex, DVec3::new(0.5, 3.0, 0.5));
    e.mobs[v].illager.as_mut().unwrap().life = Some(1);
    let uid = e.mobs[v].uid;
    let health = |e: &Entities| e.mobs.iter().find(|m| m.uid == uid).map_or(0.0, |m| m.health);
    e.update(0.05, &world, &ctx(vec![]));
    assert_eq!(health(&e), 13.0);
    for _ in 0..20 {
        e.update(0.05, &world, &ctx(vec![]));
    }
    assert_eq!(health(&e), 12.0, "again 20 ticks later");
}

#[test]
fn a_stunned_ravager_roars_and_spares_illagers() {
    let world = Grid::flat(1);
    let mut e = Entities::new(61);
    let r = mob(&mut e, MobKind::Ravager, DVec3::new(0.5, 1.0, 0.5));
    mob(&mut e, MobKind::Cow, DVec3::new(3.0, 1.0, 0.5));
    mob(&mut e, MobKind::Vindicator, DVec3::new(-2.5, 1.0, 0.5));
    e.mobs[r].illager.as_mut().unwrap().stun_tick = 1;
    for _ in 0..12 {
        e.update(0.05, &world, &ctx(vec![]));
    }
    let cow = e.mobs.iter().find(|m| m.kind == MobKind::Cow).unwrap();
    assert_eq!(cow.health, 10.0 - RAVAGER_ROAR_DAMAGE, "the roar hurts the cow");
    let v = e.mobs.iter().find(|m| m.kind == MobKind::Vindicator).unwrap();
    assert_eq!(v.health, 24.0, "illagers are spared");
}

#[test]
fn a_ravager_tramples_crops_it_walks_through() {
    let mut world = Grid::flat(1);
    let wheat = Block::crop(crate::world::block::Crop::Wheat, 7);
    world.set(IVec3::new(0, 1, 0), wheat);
    let mut e = Entities::new(71);
    mob(&mut e, MobKind::Ravager, DVec3::new(0.5, 1.0, 0.5));
    let events = e.update(0.05, &world, &ctx(vec![]));
    assert!(
        events
            .iter()
            .any(|ev| matches!(ev, EntityEvent::MobGrief { cell, drops: true, .. } if *cell == IVec3::new(0, 1, 0))),
        "{events:?}"
    );
}

#[test]
fn pillagers_charge_hold_and_fire_their_crossbows() {
    let world = Grid::flat(1);
    for multishot in [0u8, 1] {
        let mut e = Entities::new(81);
        let p = mob(&mut e, MobKind::Pillager, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[p].equipment.get_mut(Slot::MainHand).unwrap().enchants =
            Enchants::NONE.with(Enchantment::Multishot, multishot);
        let c = ctx(vec![player(DVec3::new(6.5, 1.0, 0.5))]);
        let mut volleys: Vec<(u32, usize)> = Vec::new();
        let shooter = e.mobs[p].uid;
        for tick in 0..300 {
            // `MobArrow` events become arrows inside the update.
            let before = e.arrows.len();
            e.update(0.05, &world, &c);
            let new = &e.arrows[before.min(e.arrows.len())..];
            assert!(new.iter().all(|a| a.shooter == Some(shooter) && a.java_base == Some(2.0)));
            if !new.is_empty() {
                volleys.push((tick, new.len()));
            }
        }
        assert!(volleys.len() >= 2, "{volleys:?}");
        assert!(volleys.iter().all(|&(_, n)| n == 1 + 2 * multishot as usize), "{volleys:?}");
        // Charge (25) plus a 20-39 tick hold between volleys.
        let gap = volleys[1].0 - volleys[0].0;
        assert!((45..=70).contains(&gap), "{gap} ticks between volleys");
    }
}

#[test]
fn illusioners_blind_their_target_on_hard_and_turn_invisible() {
    let world = Grid::flat(1);
    let mut e = Entities::new(91);
    mob(&mut e, MobKind::Illusioner, DVec3::new(0.5, 1.0, 0.5));
    let c = ctx(vec![player(DVec3::new(6.5, 1.0, 0.5))]);
    let mut blinded = false;
    for _ in 0..300 {
        for ev in e.update_difficulty(0.05, &world, &c, Difficulty::Hard) {
            if let EntityEvent::PlayerEffect { effect: Effect::Blindness, ticks, .. } = ev {
                assert_eq!(ticks, BLINDNESS_TICKS);
                blinded = true;
            }
        }
    }
    let m = e.mobs.iter().find(|m| m.kind == MobKind::Illusioner).unwrap();
    assert!(m.effects.has(Effect::Invisibility), "the mirror spell");
    assert!(blinded, "blindness on Hard");
}

#[test]
fn illagers_survive_save_and_reload() {
    let mut e = Entities::new(101);
    for (_, kind) in KINDS {
        mob(&mut e, kind, DVec3::new(0.5, 1.0, 0.5));
    }
    let r = e.mobs.iter().position(|m| m.kind == MobKind::Ravager).unwrap();
    let ravager = e.mobs[r].uid;
    let v = e.mobs.iter().position(|m| m.kind == MobKind::Vindicator).unwrap();
    assert!(e.mount_ravager(e.mobs[v].uid, ravager), "an illager rides the ravager");
    {
        let s = e.mobs[v].illager.as_mut().unwrap();
        s.johnny = true;
        s.raider.wave = 3;
        s.raider.raid = Some(7);
        s.raider.patrol_leader = true;
    }
    let x = e.mobs.iter().position(|m| m.kind == MobKind::Vex).unwrap();
    {
        let s = e.mobs[x].illager.as_mut().unwrap();
        s.owner = Some(ravager);
        s.bound = Some(IVec3::new(1, 2, 3));
        s.life = Some(345);
    }
    e.mobs[r].illager.as_mut().unwrap().stun_tick = 17;
    let ev = e.mobs.iter().position(|m| m.kind == MobKind::Evoker).unwrap();
    e.mobs[ev].illager.as_mut().unwrap().spell_ticks = 33;
    let saved = e.nether_mobs_to_string();
    let mut back = Entities::new(102);
    back.load_nether_mobs(&saved);
    assert_eq!(back.mobs.len(), e.mobs.len(), "every illager is saved");
    for before in &e.mobs {
        let after = back.mobs.iter().find(|m| m.uid == before.uid).unwrap();
        assert_eq!(after.kind, before.kind);
        assert_eq!(after.main_hand(), before.main_hand(), "{:?} keeps its weapon", before.kind);
        assert_eq!(after.equipment.drop_chance(Slot::MainHand), before.equipment.drop_chance(Slot::MainHand));
        assert_eq!(after.riding, before.riding);
        let (a, b) = (after.illager.as_ref().unwrap(), before.illager.as_ref().unwrap());
        assert_eq!(a.raider, b.raider);
        assert_eq!((a.johnny, a.owner, a.bound, a.life), (b.johnny, b.owner, b.bound, b.life));
        assert_eq!((a.stun_tick, a.spell_ticks), (b.stun_tick, b.spell_ticks));
    }
    // `Pillager.readAdditionalSaveData` turns `CanPickUpLoot` on.
    assert!(back.mobs.iter().find(|m| m.kind == MobKind::Pillager).unwrap().can_pick_up_loot);
    assert!(!back.mobs.iter().find(|m| m.kind == MobKind::Vindicator).unwrap().can_pick_up_loot);
}
