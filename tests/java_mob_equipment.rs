//! Mob equipment against Java 26.3 (`EquipmentSlot`, `DropChances`,
//! `Mob.dropCustomDeathLoot`, `populateDefaultEquipmentSlots`, conversions):
//! the facts come from `fixtures/java/26.3/mob_equipment.json`, extracted by
//! `tools/extract_java_mob_equipment_fixtures.py`.
use glam::DVec3;
use serde_json::Value;
use voxelcraft::{
    enchant::{Enchantment, Enchants},
    entity::{
        Entities, Mob, MobKind,
        armor::{self, ArmorKind},
        equipment::{self, Equipment, Slot},
    },
    inventory::{Stack, stack_to_string},
    item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/mob_equipment.json")).unwrap()
}

fn num(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

#[test]
fn slots_and_drop_chance_constants_match_java() {
    let f = fixture();
    let slots = f["slots"].as_array().unwrap();
    // Both directions: the same slots, in `EquipmentSlot.VALUES` order.
    assert_eq!(slots.len(), Slot::ALL.len());
    for (slot, java) in Slot::ALL.into_iter().zip(slots) {
        assert_eq!(slot.name(), java["name"].as_str().unwrap());
        assert_eq!(u64::from(slot.count_limit()), java["count_limit"].as_u64().unwrap(), "{slot:?}");
        assert_eq!(slot.index() as u64, java["index"].as_u64().unwrap(), "{slot:?}");
        let kind = java["type"].as_str().unwrap();
        assert_eq!(slot.is_armor(), kind.ends_with("_armor"), "{slot:?}");
        assert_eq!(slot.piece().is_some(), kind == "humanoid_armor", "{slot:?}");
        assert_eq!(slot.can_increase_experience(), kind != "saddle", "{slot:?}");
    }
    let chances = &f["drop_chances"];
    assert_eq!(equipment::DEFAULT_DROP_CHANCE, num(&chances["default"]));
    assert_eq!(equipment::PRESERVE_THRESHOLD, num(&chances["preserve_threshold"]));
    assert_eq!(equipment::GUARANTEED_DROP_CHANCE, num(&chances["guaranteed"]));
    let fresh = Equipment::default();
    assert!(Slot::ALL.iter().all(|&s| fresh.drop_chance(s) == equipment::DEFAULT_DROP_CHANCE));
    let mut e = Equipment::default();
    e.set_drop_chance(Slot::Head, 1.0);
    assert!(!e.is_preserved(Slot::Head), "1.0 is not above the threshold");
    e.set_guaranteed_drop(Slot::Head);
    assert!(e.is_preserved(Slot::Head));
}

#[test]
fn looting_and_spawn_constants_match_java() {
    let f = fixture();
    let looting = &f["looting_equipment_drops"];
    assert_eq!(looting["effect"], "add");
    assert_eq!(looting["attacker_must_be"], "minecraft:player");
    let (base, per) = (num(&looting["base"]), num(&looting["per_level_above_first"]));
    assert_eq!((equipment::LOOTING_BASE, equipment::LOOTING_PER_LEVEL_ABOVE_FIRST), (base, per));
    assert_eq!(equipment::looting_chance(0.085, 0), 0.085);
    for level in 1..=3u8 {
        let java = 0.085 + (base + per * f32::from(level - 1));
        assert_eq!(equipment::looting_chance(0.085, level), java);
    }
    assert_eq!(f["curses"]["vanishing_curse"][0], f["death_drop"]["prevent_drop_effect"]);
    let spawn = &f["spawn"];
    assert_eq!(equipment::MAX_ENCHANTED_ARMOR_CHANCE, num(&spawn["max_enchanted_armor_chance"]));
    assert_eq!(equipment::MAX_ENCHANTED_WEAPON_CHANCE, num(&spawn["max_enchanted_weapon_chance"]));
    let order: Vec<Slot> = spawn["population_order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Slot::from_name(s.as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(armor::POPULATION_ORDER.map(Slot::of_piece).to_vec(), order);
    let tiers: Vec<ArmorKind> = spawn["armor_tiers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| ArmorKind::from_name(t.as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(armor::MONSTER_TIERS.to_vec(), tiers);
    assert_eq!(f["trial_spawner_slot_drop_chances"], serde_json::json!([0.0]));
}

/// A partial monster set starts at the helmet (`EQUIPMENT_POPULATION_ORDER`).
#[test]
fn natural_armor_fills_the_head_first() {
    let mut partial = 0;
    for seed in 0..4000 {
        let worn = armor::roll_monster_armor(&mut voxelcraft::entity::Rng::new(seed), false, 1.0);
        let count = worn.iter().flatten().count();
        if (1..4).contains(&count) {
            partial += 1;
            assert!(worn[ArmorPiece::Helmet as usize].is_some(), "{worn:?}");
            assert!(worn[ArmorPiece::Boots as usize].is_none(), "{worn:?}");
        }
    }
    assert!(partial > 100, "{partial}");
}

/// `Drowned.populateDefaultEquipmentSlots` replaces Mob's: no armor.
#[test]
fn drowned_spawn_without_armor() {
    assert_eq!(fixture()["drowned_skips_armor"], true);
    let mut e = Entities::new(5);
    e.difficulty = voxelcraft::simulation::difficulty::Difficulty::Hard;
    e.clock_time = 10_000_000;
    e.moon_brightness = 1.0;
    let mut zombies_armored = 0;
    for _ in 0..3000 {
        e.spawn(MobKind::Drowned, DVec3::new(0.5, 10.0, 0.5));
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
    }
    for m in &e.mobs {
        let armored = m.equipment.worn().iter().any(Option::is_some);
        match m.kind {
            MobKind::Drowned => assert!(!armored),
            _ => zombies_armored += usize::from(armored),
        }
    }
    assert!(zombies_armored > 100, "{zombies_armored}");
}

fn iron(piece: ArmorPiece) -> Stack {
    Stack::new(Item::armor(piece, ArmorMaterial::Iron), 1)
}

/// A zombie wearing an iron helmet and boots and holding an iron sword,
/// killed `trials` times; the drops of each item counted.
fn kill_many(
    trials: u32,
    looting: u8,
    player_kill: bool,
    gear: impl Fn(&mut Equipment),
) -> (Vec<Stack>, std::collections::HashMap<Item, u32>) {
    let mut e = Entities::new(77);
    let mut drops = Vec::new();
    let mut counts = std::collections::HashMap::new();
    for _ in 0..trials {
        let mut m = Mob::new(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5), 0.0);
        gear(&mut m.equipment);
        m.dying = Some(0.0);
        e.mobs.push(m);
        e.drop_loot_killed(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5), looting, player_kill);
        for item in e.items.drain(..) {
            if item.stack.item.as_armor().is_some() || item.stack.item.as_tool().is_some() {
                *counts.entry(item.stack.item).or_insert(0) += 1;
                drops.push(item.stack);
            }
        }
        e.mobs.clear();
        e.orbs.clear();
    }
    (drops, counts)
}

fn within_five_se(hits: u32, trials: u32, p: f32) {
    let (n, p) = (f64::from(trials), f64::from(p));
    let observed = f64::from(hits) / n;
    let se = (p * (1.0 - p) / n).sqrt();
    assert!((observed - p).abs() <= 5.0 * se, "observed {observed}, expected {p} ± {}", 5.0 * se);
}

fn sword() -> Stack {
    Stack::new(Item::tool(ToolKind::Sword, Tier::Iron), 1)
}

fn standard_gear(e: &mut Equipment) {
    e.set(Slot::Head, Some(iron(ArmorPiece::Helmet)));
    e.set(Slot::Feet, Some(iron(ArmorPiece::Boots)));
    e.set(Slot::MainHand, Some(sword()));
}

#[test]
fn player_kills_drop_each_slot_at_the_default_chance_raised_by_looting() {
    const N: u32 = 6000;
    for looting in [0u8, 3] {
        let (_, counts) = kill_many(N, looting, true, standard_gear);
        let p = equipment::looting_chance(0.085, looting);
        for item in [iron(ArmorPiece::Helmet).item, iron(ArmorPiece::Boots).item, sword().item] {
            within_five_se(counts.get(&item).copied().unwrap_or(0), N, p);
        }
    }
}

#[test]
fn deaths_without_player_credit_drop_only_preserved_gear() {
    const N: u32 = 2000;
    // Killed by another mob or the environment: nothing, even with Looting.
    let (drops, _) = kill_many(N, 3, false, standard_gear);
    assert!(drops.is_empty(), "{drops:?}");
    // A guaranteed slot drops on every death, its wear untouched.
    let mut worn = sword();
    worn.damage = 40;
    let (drops, counts) = kill_many(200, 0, false, |e| {
        standard_gear(e);
        e.set(Slot::MainHand, Some(worn));
        e.set_guaranteed_drop(Slot::MainHand);
    });
    assert_eq!(counts.get(&sword().item), Some(&200));
    assert!(drops.iter().all(|s| *s == worn), "preserved drops keep their stack");
}

#[test]
fn zero_chance_and_curse_of_vanishing_never_drop() {
    let (drops, _) = kill_many(1000, 3, true, |e| {
        standard_gear(e);
        e.set_drop_chance(Slot::Head, 0.0);
        e.set_guaranteed_drop(Slot::Head);
        e.set_drop_chance(Slot::Head, 0.0);
        e.get_mut(Slot::Feet).unwrap().enchants = Enchants::NONE.with(Enchantment::VanishingCurse, 1);
        e.set_guaranteed_drop(Slot::Feet);
    });
    assert!(drops.iter().all(|s| s.item == sword().item), "{:?}", drops.iter().map(|s| s.item).collect::<Vec<_>>());
}

/// `maxDamage - nextInt(1 + nextInt(max(maxDamage - 3, 1)))`: with inner
/// `k` uniform on `0..M` and the reroll `r` uniform on `0..=k`,
/// `P(r = j) = (1/M) * sum(k = j..M) 1/(k + 1)` and the mean of `r` is
/// `(M - 1) / 4`.
#[test]
fn dropped_gear_wear_follows_the_java_reroll() {
    const N: u32 = 8000;
    let helmet = Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Leather), 1);
    let max = helmet.item.durability().unwrap();
    let (drops, _) = kill_many(N, 0, true, |e| {
        e.set(Slot::Head, Some(helmet));
        // 1.0 always drops but is not preserved, so the wear is rerolled.
        e.set_drop_chance(Slot::Head, 1.0);
    });
    assert_eq!(drops.len() as u32, N);
    let m = f64::from(max.saturating_sub(3).max(1));
    let rerolls: Vec<f64> = drops.iter().map(|s| f64::from(max - s.damage)).collect();
    assert!(rerolls.iter().all(|&r| r >= 0.0 && r < m));
    let mean = rerolls.iter().sum::<f64>() / f64::from(N);
    // Var(r) from the model, for the standard error of the mean.
    let p = |j: f64| (j as usize..m as usize).map(|k| 1.0 / (k as f64 + 1.0)).sum::<f64>() / m;
    let expected_mean = (m - 1.0) / 4.0;
    let var = (0..m as usize).map(|j| p(j as f64) * (j as f64 - expected_mean).powi(2)).sum::<f64>();
    assert!((mean - expected_mean).abs() <= 5.0 * (var / f64::from(N)).sqrt(), "{mean} vs {expected_mean}");
    let zero = rerolls.iter().filter(|&&r| r == 0.0).count() as u32;
    within_five_se(zero, N, p(0.0) as f32);
}

#[test]
fn conversions_keep_or_drop_equipment_as_java_says() {
    let f = fixture();
    assert_eq!(f["conversions"]["zombie_to_drowned_husk_to_zombie_skeleton_to_stray"]["keep_equipment"], true);
    assert_eq!(f["conversions"]["piglin_to_zombified_piglin"]["keep_equipment"], true);
    assert_eq!(f["conversions"]["zombie_villager_cure"]["keep_equipment"], false);
    assert_eq!(f["conversions"]["villager_to_witch"]["keep_equipment"], false);
    // Carrying keeps each stack and its chance.
    let mut from = Equipment::default();
    standard_gear(&mut from);
    from.set_drop_chance(Slot::Head, 0.0);
    from.set_guaranteed_drop(Slot::MainHand);
    from.wear_chainmail(ArmorPiece::Chestplate, true);
    let before = from.clone();
    let mut to = Equipment::default();
    to.carry_from(&mut from);
    assert_eq!(to, before);
    assert!(from.is_empty());
}

/// A record saved before v0.14, with its `armor`, `armor_materials`,
/// `armor_glint`, `skeleton_bow`, `main_hand` and `keeps_equipment` keys.
const LEGACY_NETHER_MOBS: &str = r#"{"bastions":[],"generation":[],"mobs":[
{"admire":0.0,"age":0,"animal":null,"armor":[false,false,false,false],"armor_materials":[1,2,null,null],"armor_glint":2,"baby":false,"cannot_hunt":false,"celebrate":null,"color":0,"effects":"","egg_timer":0.0,"grow":0.0,"health":20.0,"held":null,"home":null,"identity":{"leash":null,"name":null},"immune":false,"kind":"skeleton","mount":null,"offhand":null,"persistence_required":true,"pocket":"","pos":[10.5,70.0,0.5],"riding":null,"sheared":false,"uid":11,"variant":null,"weapon":"none","yaw":0.0,"zombify":0.0,"skeleton_bow":"BOW","keeps_equipment":true},
{"admire":0.0,"age":0,"animal":null,"armor":[true,false,false,true],"baby":false,"cannot_hunt":false,"celebrate":null,"color":0,"effects":"","egg_timer":0.0,"grow":0.0,"health":20.0,"held":null,"home":null,"identity":{"leash":null,"name":null},"immune":false,"kind":"zombie","mount":null,"offhand":null,"persistence_required":true,"pocket":"","pos":[20.5,70.0,0.5],"riding":null,"sheared":false,"uid":12,"variant":null,"weapon":"none","yaw":0.0,"zombify":0.0,"armor_glint":1},
{"admire":0.0,"age":0,"animal":null,"armor":[false,false,false,false],"baby":false,"cannot_hunt":false,"celebrate":null,"color":0,"effects":"","egg_timer":0.0,"grow":0.0,"health":20.0,"held":null,"home":null,"identity":{"leash":null,"name":null},"immune":false,"kind":"skeleton","mount":null,"offhand":null,"persistence_required":true,"pocket":"","pos":[30.5,70.0,0.5],"riding":null,"sheared":false,"uid":13,"variant":null,"weapon":"none","yaw":0.0,"zombify":0.0}
],"spawn_ticks":0,"structures":[]}"#;

#[test]
fn pre_v017_saves_load_into_the_new_slots_with_the_same_drop_rule() {
    let mut power_bow = Stack::new(Item::BOW, 1);
    power_bow.enchants = Enchants::NONE.with(Enchantment::Power, 3);
    let legacy = LEGACY_NETHER_MOBS.replace("BOW", &stack_to_string(Some(power_bow)));
    let mut e = Entities::new(1);
    e.load_nether_mobs(&legacy);
    assert_eq!(e.mobs.len(), 3);
    let at = |x: f64| e.mobs.iter().find(|m| m.pos.x == x).unwrap();
    // A trial skeleton: chainmail helmet (drawn, no item yet), an enchanted
    // iron chestplate and its bow; `keeps_equipment` meant nothing dropped.
    let trial = at(10.5);
    assert_eq!(trial.equipment.main_hand(), Some(&power_bow));
    assert_eq!(trial.equipment.armor_kind(ArmorPiece::Helmet), Some(ArmorKind::Chain));
    let plate = trial.equipment.get(Slot::Chest).unwrap();
    assert_eq!(plate.item, Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Iron));
    assert_eq!(plate.enchants, equipment::trial_armor_enchants());
    assert!(Slot::ALL.iter().all(|&s| trial.equipment.drop_chance(s) == 0.0));
    // A natural zombie in gold: the glinted helmet gets a stand-in
    // enchantment; chances stay at the default.
    let zombie = at(20.5);
    let helmet = zombie.equipment.get(Slot::Head).unwrap();
    assert_eq!(helmet.item, Item::armor(ArmorPiece::Helmet, ArmorMaterial::Gold));
    assert!(!helmet.enchants.is_empty());
    let boots = zombie.equipment.get(Slot::Feet).unwrap();
    assert_eq!((boots.item, boots.enchants), (Item::armor(ArmorPiece::Boots, ArmorMaterial::Gold), Enchants::NONE));
    assert_eq!(zombie.equipment.drop_chance(Slot::Feet), equipment::DEFAULT_DROP_CHANCE);
    // A skeleton saved before `skeleton_bow` keeps its default bow.
    assert_eq!(at(30.5).equipment.main_hand().map(|s| s.item), Some(Item::BOW));
    // Saved again, it round-trips through the new keys only.
    let text = e.nether_mobs_to_string();
    for key in ["armor_materials", "armor_glint", "skeleton_bow", "keeps_equipment", "\"armor\""] {
        assert!(!text.contains(key), "{key} is retired");
    }
    let mut back = Entities::new(2);
    back.load_nether_mobs(&text);
    for m in &e.mobs {
        let copy = back.mobs.iter().find(|b| b.uid == m.uid).unwrap();
        assert_eq!(copy.equipment, m.equipment, "{:?}", m.kind);
    }
}

/// A zombie villager record from before v0.14 (`armor` materials, `glint`).
const LEGACY_VILLAGERS: &str = r#"{"births":[],"mobs":[{"age":0,"armor":[2,null,null,4],"bell_hide":0.0,"built":false,"convert":0.0,"convert_by":null,"convert_tick":0.0,"day":-1,"effects":"","food":["-","-","-","-","-","-","-","-"],"food_level":0,"glint":8,"golem_seen":0.0,"gossip":{"day":0,"entries":[]},"health":20.0,"home":null,"id":1,"identity":{"leash":null,"name":null},"job":null,"kind":"zombie villager","last":0,"level":1,"offers":[null,null,null,null,null,null,null,null,null,null],"p":[1000.5,70.0,1000.5],"profession":3,"promote":0.0,"promote_pending":false,"reputation":0,"restocks":0,"seed":1865834515,"slept":null,"trader":null,"weakness":0.0,"xp":0,"yaw":0.0}],"next":2,"trader_spawner":[1200.0,25]}"#;

#[test]
fn pre_v017_zombie_villagers_keep_their_armor() {
    let mut e = Entities::new(3);
    e.load_villagers(LEGACY_VILLAGERS);
    let m = e.mobs.iter().find(|m| m.kind == MobKind::ZombieVillager).expect("zombie villager");
    assert_eq!(m.equipment.get(Slot::Head).map(|s| s.item), Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron)));
    let boots = m.equipment.get(Slot::Feet).unwrap();
    assert_eq!(boots.item, Item::armor(ArmorPiece::Boots, ArmorMaterial::Diamond));
    assert!(!boots.enchants.is_empty(), "glint bit 3 is the boots");
    let text = e.villagers_to_string();
    let mut back = Entities::new(4);
    back.load_villagers(&text);
    assert_eq!(back.mobs[0].equipment, m.equipment);
}

/// Netherite (and any other armor with an item) drops as the worn stack;
/// before v0.14 a mob's netherite mapped to no item.
#[test]
fn netherite_armor_drops_its_stack() {
    let mut plate = Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Netherite), 1);
    plate.enchants = Enchants::NONE.with(Enchantment::Protection, 2);
    let (drops, _) = kill_many(50, 0, true, |e| {
        e.set(Slot::Chest, Some(plate));
        e.set_drop_chance(Slot::Chest, 1.0);
    });
    assert_eq!(drops.len(), 50);
    assert!(drops.iter().all(|s| s.item == plate.item && s.enchants == plate.enchants));
}

/// `populateDefaultEquipmentEnchantments`: the hand at 0.25 × special, each
/// worn piece at 0.5 × special, with real `mob_spawn_equipment` enchantments.
#[test]
fn spawned_gear_gets_real_enchantments_at_java_rates() {
    const N: u32 = 4000;
    let special = 0.8;
    let (mut hands, mut pieces) = (0u32, 0u32);
    let mut rng = voxelcraft::entity::Rng::new(12);
    for _ in 0..N {
        let mut e = Equipment::default();
        e.set(Slot::MainHand, Some(Stack::new(Item::BOW, 1)));
        e.set(Slot::Head, Some(iron(ArmorPiece::Helmet)));
        e.enchant_spawned(&mut rng, special);
        hands += u32::from(!e.main_hand().unwrap().enchants.is_empty());
        pieces += u32::from(!e.get(Slot::Head).unwrap().enchants.is_empty());
    }
    within_five_se(hands, N, equipment::MAX_ENCHANTED_WEAPON_CHANCE * special);
    within_five_se(pieces, N, equipment::MAX_ENCHANTED_ARMOR_CHANCE * special);
}

/// LAN replicas draw what the host's slots hold.
#[test]
fn replicas_draw_the_slots() {
    let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    m.equipment.wear_set(ArmorKind::Diamond, true);
    m.equipment.wear_chainmail(ArmorPiece::Leggings, false);
    let mut replica = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    replica.apply_look(&m.look());
    assert_eq!(replica.equipment.worn(), m.equipment.worn());
}

/// `Zombie.populateDefaultEquipmentSlots`: a main-hand weapon 5% of the
/// time on Hard (1% otherwise), sword, spear or shovel by `nextInt(6)`.
#[test]
fn zombies_spawn_with_java_weapons() {
    let f = fixture();
    let w = &f["zombie_weapon"];
    assert_eq!(w["by_roll"]["1"], "iron_spear");
    for (difficulty, chance, n) in [
        (voxelcraft::simulation::difficulty::Difficulty::Hard, num(&w["chance_hard"]), 8000u32),
        (voxelcraft::simulation::difficulty::Difficulty::Normal, num(&w["chance_other"]), 20000),
    ] {
        let mut e = Entities::new(9);
        e.difficulty = difficulty;
        let mut held = std::collections::HashMap::new();
        let mut zombies = 0;
        for _ in 0..n {
            e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
            let m = e.mobs.pop().unwrap();
            e.mobs.clear();
            if m.kind != MobKind::Zombie {
                continue;
            }
            zombies += 1;
            if let Some(stack) = m.equipment.main_hand() {
                *held.entry(stack.item).or_insert(0u32) += 1;
            }
        }
        let total: u32 = held.values().sum();
        within_five_se(total, zombies, chance);
        let sword = Item::tool(ToolKind::Sword, Tier::Iron);
        let spear = voxelcraft::weapons::spear(Tier::Iron);
        let shovel = Item::tool(ToolKind::Shovel, Tier::Iron);
        assert_eq!(held.keys().filter(|i| ![sword, spear, shovel].contains(i)).count(), 0, "{held:?}");
        within_five_se(held.get(&shovel).copied().unwrap_or(0), total, 4.0 / 6.0);
    }
}

/// `Mob.getBaseExperienceReward` runs after the drops: a skeleton killed by
/// a player gives 5, plus 1–3 if its bow did not drop.
#[test]
fn filled_slots_add_experience_after_the_drops() {
    let f = fixture();
    assert_eq!(f["experience"]["per_filled_slot"], "1 + nextInt(3)");
    assert_eq!(f["death_drop"]["before_experience"], true);
    let mut e = Entities::new(31);
    let mut seen = [0u32; 9];
    for _ in 0..3000 {
        let mut m = Mob::new(MobKind::Skeleton, DVec3::new(0.5, 10.0, 0.5), 0.0);
        m.dying = Some(0.0);
        e.mobs.push(m);
        e.drop_loot_killed(MobKind::Skeleton, DVec3::new(0.5, 10.0, 0.5), 0, true);
        let xp: u32 = e.orbs.iter().map(|o| o.value * o.count).sum();
        let bow_dropped = e.items.iter().any(|i| i.stack.item == Item::BOW);
        if bow_dropped {
            assert_eq!(xp, 5);
        } else {
            assert!((6..=8).contains(&xp), "{xp}");
        }
        seen[xp as usize] += 1;
        e.mobs.clear();
        e.items.clear();
        e.orbs.clear();
    }
    assert!(seen[5] > 0 && seen[6] > 0 && seen[7] > 0 && seen[8] > 0, "{seen:?}");
    // A preserved slot adds nothing.
    let mut m = Mob::new(MobKind::Skeleton, DVec3::ZERO, 0.0);
    m.equipment.set_drop_chance(Slot::MainHand, 0.0);
    assert!((1..=3).contains(&m.equipment.experience_bonus(&mut voxelcraft::entity::Rng::new(1))));
    m.equipment.set_guaranteed_drop(Slot::MainHand);
    assert_eq!(m.equipment.experience_bonus(&mut voxelcraft::entity::Rng::new(1)), 0);
}
