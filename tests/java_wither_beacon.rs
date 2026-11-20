//! v0.16 Track C: the Wither, its skulls, the wither rose, mob heads and
//! beacons against `fixtures/java/26.3/wither_beacon.json`, which
//! `tools/extract_java_wither_beacon_fixtures.py` reads from the pinned
//! server jar.

use serde_json::Value;
use voxelcraft::entity::MobKind;
use voxelcraft::entity::wither;
use voxelcraft::item::{ArmorPiece, Item};
use voxelcraft::java_registry;
use voxelcraft::simulation::effects::Effect;
use voxelcraft::world::beacon;
use voxelcraft::world::block::Block;
use voxelcraft::world::skulls::{self, Kind};
use voxelcraft::world::wither_rose;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/wither_beacon.json")).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn the_wither_matches_its_entity_type_and_attributes() {
    let fx = fixture();
    let t = &fx["entity_types"]["wither"];
    let shape = MobKind::Wither.shape();
    assert_eq!(shape.half_width * 2.0, f(&t["width"]));
    assert_eq!(shape.height, f(&t["height"]));
    assert_eq!(MobKind::Wither.fire_immune(), t["fire_immune"].as_bool().unwrap());
    assert!(MobKind::Wither.is_hostile() && t["category"] == "monster");
    let w = &fx["wither"];
    let a = &w["attributes"];
    assert_eq!(f64::from(MobKind::Wither.max_health()), f(&a["max_health"]));
    assert_eq!(f64::from(wither::ARMOR), f(&a["armor"]));
    assert_eq!(wither::FOLLOW_RANGE, f(&a["follow_range"]));
    assert_eq!(wither::FLYING_SPEED as f32, f(&a["flying_speed"]) as f32);
    assert_eq!(i64::from(wither::INVULNERABLE_TICKS), w["constants"]["INVULNERABLE_TICKS"].as_i64().unwrap());
    assert_eq!(u64::from(wither::EXPERIENCE), w["xp_reward"].as_u64().unwrap());
    let goal = w["ranged_attack_goal"].as_array().unwrap();
    assert_eq!(i64::from(wither::ATTACK_INTERVAL), goal[1].as_i64().unwrap());
    assert_eq!(wither::ATTACK_RADIUS, f(&goal[2]));
    assert_eq!(f64::from(wither::SPAWN_POWER), f(&w["spawn_explosion_power"]));
    assert_eq!(w["spawn_level_event"], 1023);
    assert_eq!(f(&w["charge_heal"]), 10.0);
    assert_eq!(f(&w["heal"]), 1.0);
    assert_eq!(f(&w["head_attack_range_sqr"]), 30.0 * 30.0);
    assert_eq!(wither::HEAD_RANGE, f(&w["targeting_range"]));
    let b = w["head_targeting_box"].as_array().unwrap();
    assert_eq!((f(&b[0]), f(&b[1]), f(&b[2])), (20.0, 8.0, 20.0));
    assert_eq!(f(&w["invulnerable_health_divisor"]), 3.0);
    assert_eq!(f(&w["powered_divisor"]), 2.0);
    // Head positions: the centre at 3, the sides 1.3 out at 2.2.
    let p = glam::DVec3::ZERO;
    let ys = w["head_y"].as_array().unwrap();
    assert_eq!(wither::head_pos(p, 0.0, 0).y as f32, f(&ys[0]) as f32);
    assert_eq!(wither::head_pos(p, 0.0, 1).y as f32, f(&ys[1]) as f32);
    assert!((wither::head_pos(p, 0.0, 1).x - f(&w["head_offset"])).abs() < 1e-6);
    assert_eq!(f(&w["dangerous_chance"]) as f32, 0.001);
}

#[test]
fn skulls_match_wither_skull() {
    let fx = fixture();
    let s = &fx["wither_skull"];
    assert_eq!(f64::from(wither::SKULL_DAMAGE), f(&s["owner_damage"]));
    assert_eq!(f64::from(wither::SKULL_MAGIC_DAMAGE), f(&s["ownerless_damage"]));
    assert_eq!(f64::from(wither::SKULL_HEAL), f(&s["kill_heal"]));
    let secs: Vec<u64> = s["wither_seconds"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(secs, wither::SKULL_WITHER_SECONDS.map(u64::from));
    assert_eq!(s["wither_amplifier"], 1);
    assert_eq!(wither::DANGEROUS_INERTIA, f(&s["dangerous_inertia"]) as f32);
    assert_eq!(wither::SKULL_INERTIA, f(&s["inertia"]) as f32);
    assert_eq!(wither::LIQUID_INERTIA, f(&s["liquid_inertia"]) as f32);
    assert_eq!(wither::ACCELERATION_POWER, f(&s["acceleration_power"]));
    assert_eq!(f64::from(wither::SKULL_POWER), f(&s["explosion_power"]));
    assert_eq!(wither::DANGEROUS_RESISTANCE, f(&s["dangerous_resistance_cap"]) as f32);
}

#[test]
fn the_wither_never_breaks_its_immune_blocks() {
    let fx = fixture();
    for name in strings(&fx["tags"]["block"]["wither_immune"]) {
        if let Some(b) = java_registry::block(&name, "") {
            assert!(!wither::can_destroy(b), "{name}");
        }
    }
    assert!(wither::can_destroy(Block::OBSIDIAN) && wither::can_destroy(Block::STONE));
    assert!(!wither::can_destroy(Block::AIR));
    // `#wither_friends` (`#undead`): the Wither leaves the undead alone.
    assert_eq!(strings(&fx["tags"]["entity_type"]["wither_friends"]), ["#minecraft:undead"]);
    for kind in [MobKind::Zombie, MobKind::Skeleton, MobKind::WitherSkeleton, MobKind::Phantom, MobKind::Wither] {
        assert!(wither::friend(kind), "{kind:?}");
    }
    assert!(!wither::friend(MobKind::Cow) && !wither::friend(MobKind::IronGolem));
    assert_eq!(strings(&fx["tags"]["block"]["wither_summon_base_blocks"]), ["soul_sand", "soul_soil"]);
}

#[test]
fn heads_match_their_items_and_blocks() {
    let fx = fixture();
    for kind in Kind::ALL {
        let name = kind.java_name();
        let facts = &fx["items"][name];
        let item = java_registry::item(name, "").unwrap_or_else(|| panic!("{name} is registered"));
        assert_eq!(item, skulls::item(kind), "{name}");
        assert_eq!(java_registry::block(kind.java_wall_name(), "").map(|b| b.base()), Some(skulls::item_block(kind)));
        let rarity = facts["rarity"].as_str().unwrap();
        assert_eq!(item.is_uncommon(), rarity == "uncommon", "{name}");
        assert_eq!(item.is_rare(), rarity == "rare", "{name}");
        assert_eq!(item.is_epic(), rarity == "epic", "{name}");
        assert_eq!(item.wear_slot() == Some(ArmorPiece::Helmet), facts["equips_head"].as_bool().unwrap(), "{name}");
        // `loweredMobVisibility`: exactly the listed mobs see it at half range.
        let hidden: Vec<String> = facts["lowered_mob_visibility"]
            .as_array()
            .map_or(Vec::new(), |a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect());
        for mob in MobKind::ALL {
            let java = mob.name().replace(' ', "_");
            assert_eq!(kind.disguises_from(mob), hidden.contains(&java), "{name} against {java}");
        }
        let strength = f(&fx["block_strength"][name]) as f32;
        assert_eq!(skulls::item_block(kind).hardness(), strength, "{name}");
        assert_eq!(skulls::wall(kind, voxelcraft::world::block::Facing::North).hardness(), strength);
        assert_eq!(skulls::item_block(kind).drop(), Some(item));
    }
    // `#noteblock_top_instruments` and the instruments the heads set.
    let tops = strings(&fx["tags"]["item"]["noteblock_top_instruments"]);
    assert_eq!(tops.len(), Kind::ALL.len());
    let instruments = strings(&fx["note_block_instruments"]);
    for kind in Kind::ALL {
        assert!(tops.contains(&kind.java_name().to_string()));
        let set = skulls::note_instrument(skulls::item_block(kind)).unwrap();
        // The player head is `custom_head`; the rest name their mob.
        assert_eq!(set.is_none(), kind == Kind::Player);
    }
    for mob in ["zombie", "skeleton", "creeper", "dragon", "wither_skeleton", "piglin", "custom_head"] {
        assert!(instruments.contains(&mob.to_string()));
    }
}

#[test]
fn the_nether_star_beacon_and_rose_match_items() {
    let fx = fixture();
    let star = &fx["items"]["nether_star"];
    assert_eq!(java_registry::item("nether_star", ""), Some(wither::NETHER_STAR));
    assert!(wither::NETHER_STAR.is_rare() && star["rarity"] == "rare");
    assert_eq!(wither::NETHER_STAR.has_glint(), star["glint"].as_bool().unwrap());
    assert_eq!(wither::NETHER_STAR.explosion_proof(), star["damage_resistant"].as_bool().unwrap());
    let beacon_item = Item::from_block(beacon::BEACON);
    assert_eq!(java_registry::item("beacon", ""), Some(beacon_item));
    assert!(beacon_item.is_rare() && fx["items"]["beacon"]["rarity"] == "rare");
    assert_eq!(beacon::BEACON.hardness(), f(&fx["block_strength"]["beacon"]) as f32);
    assert_eq!(beacon::BEACON.emission(), 15);
    let rose = Item::from_block(wither_rose::WITHER_ROSE);
    assert_eq!(java_registry::item("wither_rose", ""), Some(rose));
    assert!(!rose.is_rare() && !rose.is_uncommon());
    assert_eq!(wither_rose::WITHER_ROSE.hardness(), f(&fx["block_strength"]["wither_rose"]) as f32);
    assert_eq!(u64::from(wither_rose::CONTACT_TICKS), fx["wither_rose_contact_ticks"].as_u64().unwrap());
    // `#supports_wither_rose`.
    let soils = strings(&fx["tags"]["block"]["supports_wither_rose"]);
    for name in soils.iter().filter(|n| !n.starts_with('#')) {
        let b = java_registry::block(name, "").unwrap();
        assert!(wither_rose::supports(b), "{name}");
    }
    assert!(wither_rose::supports(Block::GRASS));
    assert_eq!(java_registry::block("potted_wither_rose", ""), Some(wither_rose::POTTED_WITHER_ROSE));
}

#[test]
fn beacons_match_beacon_block_entity() {
    let fx = fixture();
    let b = &fx["beacon"];
    let powers: Vec<Vec<String>> = b["powers"].as_array().unwrap().iter().map(strings).collect();
    let ours: Vec<Vec<String>> =
        beacon::POWERS.iter().map(|tier| tier.iter().map(|e| e.id().to_string()).collect()).collect();
    assert_eq!(ours, powers);
    assert_eq!(i64::from(beacon::MAX_LEVELS), b["constants"]["MAX_LEVELS"].as_i64().unwrap());
    assert_eq!(i64::from(beacon::CHECKS_PER_TICK), b["constants"]["BLOCKS_CHECK_PER_TICK"].as_i64().unwrap());
    assert_eq!(beacon::UPDATE_INTERVAL, b["update_interval"].as_u64().unwrap());
    // `applyEffects`: `levels * 10 + 10` blocks for `(9 + levels * 2) * 20` ticks.
    let n: Vec<f64> = b["apply_effects_numbers"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(&n[..2], &[10.0, 10.0]);
    assert_eq!(&n[5..8], &[9.0, 2.0, 20.0]);
    for levels in 1..=4u8 {
        assert_eq!(beacon::range(levels), f64::from(levels) * n[0] + n[1]);
        assert_eq!(f64::from(beacon::duration(levels)), (n[5] + f64::from(levels) * n[6]) * n[7]);
    }
    assert_eq!(b["payment_slot_max"], 1);
    for name in strings(&fx["tags"]["block"]["beacon_base_blocks"]) {
        // Registered as "block of iron" and so on.
        let english = format!("block of {}", name.trim_end_matches("_block"));
        let block = java_registry::block(&name, &english).unwrap();
        assert!(beacon::is_base(block), "{name}");
    }
    assert!(!beacon::is_base(Block::STONE));
    for name in strings(&fx["tags"]["item"]["beacon_payment_items"]) {
        let item = java_registry::item(&name, &name.replace('_', " ")).unwrap();
        assert!(beacon::is_payment(item), "{name}");
    }
    assert!(!beacon::is_payment(Item::COAL));
    // The secondary is level-4 only, and so is Regeneration.
    assert!(beacon::valid_powers(Some(Effect::Speed), Some(Effect::Regeneration), 4));
    assert!(!beacon::valid_powers(Some(Effect::Speed), Some(Effect::Regeneration), 3));
}

#[test]
fn loot_tables_name_the_heads_and_star() {
    let fx = fixture();
    let loot = &fx["loot_tables"];
    // `entities/wither` has no pools: the star is `dropCustomDeathLoot`.
    assert!(loot["entities/wither"]["pools"].is_null());
    // Charged creepers knock off the five mob heads.
    for (table, kind, mob) in [
        ("charged_creeper/creeper", Kind::Creeper, MobKind::Creeper),
        ("charged_creeper/piglin", Kind::Piglin, MobKind::Piglin),
        ("charged_creeper/skeleton", Kind::Skeleton, MobKind::Skeleton),
        ("charged_creeper/wither_skeleton", Kind::WitherSkeleton, MobKind::WitherSkeleton),
        ("charged_creeper/zombie", Kind::Zombie, MobKind::Zombie),
    ] {
        let name = loot[table]["pools"][0]["entries"][0]["name"].as_str().unwrap();
        assert_eq!(name.trim_start_matches("minecraft:"), kind.java_name());
        assert_eq!(Kind::of_mob(mob), Some(kind));
    }
    assert_eq!(Kind::of_mob(MobKind::Husk), None);
    // The wither skeleton's skull pool: 2.5%, plus 1% per Looting level.
    let pools = loot["entities/wither_skeleton"]["pools"].as_array().unwrap();
    let skull = pools.last().unwrap();
    assert_eq!(skull["entries"][0]["name"], "minecraft:wither_skeleton_skull");
    let chance = &skull["condition"]["terms"][1];
    assert_eq!(f(&chance["unenchanted_chance"]), 0.025);
    assert_eq!(f(&chance["enchanted_chance"]["base"]), 0.035);
    assert_eq!(f(&chance["enchanted_chance"]["per_level_above_first"]), 0.01);
    // Every head block drops its item.
    for kind in Kind::ALL {
        let table = &loot[format!("blocks/{}", kind.java_name())];
        let name = table["pools"][0]["entries"][0]["name"].as_str().unwrap();
        assert_eq!(name.trim_start_matches("minecraft:"), kind.java_name());
    }
    assert_eq!(loot["blocks/beacon"]["pools"][0]["entries"][0]["name"], "minecraft:beacon");
}
