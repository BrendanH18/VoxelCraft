//! v0.17 Track C2: Java 26.3's dispense table against
//! `fixtures/java/26.3/dispensing.json` (extracted by
//! `tools/extract_java_dispense_fixtures.py` from the jar's own
//! `DispenserBlock.getDispenseMethod`), both ways, and the behaviours
//! through real dispensers: equipment, shears, glass bottles, glowstone,
//! honeycomb, water bottles, flint and steel, boats, minecarts, armour
//! stands and thrown projectiles.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use voxelcraft::{
    entity::{Entities, MobKind, dispensed::DispenseTarget, equipment::Slot},
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Item},
    java_registry,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        dispensing::{self, Allowed, Behaviour},
        redstone_blocks as r,
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/dispensing.json")).unwrap()
}

fn registry() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap()
}

fn gaps() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_gaps.json")).unwrap()
}

/// Java items whose behaviour exists in the table but which VoxelCraft
/// lacks; each is a registry gap with an owner.
const ABSENT: &[(&str, &str)] = &[
    ("command_block_minecart", "command blocks are creative-only"),
    ("furnace_minecart", "Track D1 adds the furnace minecart"),
];

/// Every Java item's behaviour: the fixture's entry, else the plain drop.
fn java_behaviours() -> BTreeMap<String, Value> {
    fixture()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["item"].as_str().unwrap().to_string(), e.clone()))
        .collect()
}

fn allowed_json(a: Allowed) -> Value {
    match a {
        Allowed::Any => Value::Null,
        Allowed::Llamas => serde_json::json!(["llama", "trader_llama"]),
        Allowed::Harness => "#can_equip_harness".into(),
        Allowed::HorseArmor => "#can_wear_horse_armor".into(),
        Allowed::NautilusArmor => "#can_wear_nautilus_armor".into(),
        Allowed::Saddle => "#can_equip_saddle".into(),
        Allowed::Wolf => serde_json::json!(["wolf"]),
    }
}

fn slot_name(slot: Slot) -> &'static str {
    slot.name()
}

/// Java to VoxelCraft: every Java item that resolves gets Java's
/// behaviour, equipment slot and allowed wearers; every one that doesn't
/// and has a behaviour of its own is an owned registry gap.
#[test]
fn every_java_item_dispenses_with_its_java_behaviour() {
    let java = java_behaviours();
    let names = registry();
    let gaps = gaps();
    let mut wrong = Vec::new();
    let mut absent = BTreeSet::new();
    let mut matched = 0;
    for (id, english) in names["items"].as_object().unwrap() {
        let entry = java.get(id);
        let expected = entry.map_or("default", |e| e["behaviour"].as_str().unwrap());
        let Some(item) = java_registry::item(id, english.as_str().unwrap()) else {
            if expected != "default" {
                assert!(gaps["items"].get(id).is_some(), "{id}: missing item with behaviour {expected} has no owner");
                absent.insert(id.clone());
            }
            continue;
        };
        let got = dispensing::behaviour(item).name();
        if got != expected {
            wrong.push(format!("{id}: java {expected}, game {got}"));
            continue;
        }
        matched += 1;
        let equip = entry.map(|e| &e["equippable"]).filter(|v| !v.is_null());
        match (dispensing::equippable(item), equip) {
            (None, None) => {}
            (Some(e), Some(j)) => {
                assert_eq!(slot_name(e.slot), j["slot"], "{id} slot");
                assert_eq!(allowed_json(e.allowed), j.get("allowed").cloned().unwrap_or(Value::Null), "{id} wearers");
                assert_eq!(j["dispensable"], true, "{id}");
            }
            (e, j) => wrong.push(format!("{id}: equippable game {e:?}, java {j:?}")),
        }
    }
    assert!(wrong.is_empty(), "{} mismatches:\n{}", wrong.len(), wrong.join("\n"));
    // The items Java dispenses specially that VoxelCraft lacks: the spawn
    // eggs (v0.18) and the listed carts.
    for id in &absent {
        assert!(
            id.ends_with("_spawn_egg") || ABSENT.iter().any(|(a, _)| a == id),
            "{id}: unexpected missing dispensable item"
        );
    }
    println!("{matched} Java items dispense like Java; {} with behaviours absent", absent.len());
}

/// VoxelCraft to Java: every item with a behaviour of its own maps back to
/// a Java item with that behaviour (potions to `potion`/`splash_potion`).
#[test]
fn every_special_game_item_is_in_the_java_table() {
    let java = java_behaviours();
    let names = registry();
    let mut back: HashMap<Item, Vec<String>> = HashMap::new();
    for (id, english) in names["items"].as_object().unwrap() {
        if let Some(item) = java_registry::item(id, english.as_str().unwrap()) {
            back.entry(item).or_default().push(id.clone());
        }
    }
    let mut wrong = Vec::new();
    for item in Item::creative_palette().chain(Item::all_items()) {
        let b = dispensing::behaviour(item);
        if b == Behaviour::Default {
            continue;
        }
        let ids = if item.as_potion().is_some() {
            vec!["potion".to_string()]
        } else if item.as_splash_potion().is_some() {
            vec!["splash_potion".to_string()]
        } else {
            back.get(&item).cloned().unwrap_or_default()
        };
        if ids.is_empty() {
            wrong.push(format!("{} ({}): no Java item", item.name(), b.name()));
        }
        for id in ids {
            let expected = java.get(&id).map_or("default", |e| e["behaviour"].as_str().unwrap());
            if expected != b.name() {
                wrong.push(format!("{} as {id}: java {expected}, game {}", item.name(), b.name()));
            }
        }
    }
    assert!(wrong.is_empty(), "{} mismatches:\n{}", wrong.len(), wrong.join("\n"));
}

/// `ProjectileItem.DispenseConfig` for every projectile: power,
/// uncertainty, level event and the spawn point for each facing.
#[test]
fn projectiles_use_java_dispense_configs() {
    let fx = fixture();
    let dirs = [
        ("down", IVec3::NEG_Y),
        ("up", IVec3::Y),
        ("north", IVec3::NEG_Z),
        ("south", IVec3::Z),
        ("west", IVec3::NEG_X),
        ("east", IVec3::X),
    ];
    let names = registry();
    for (id, config) in fx["projectiles"].as_object().unwrap() {
        let english = names["items"][id].as_str().unwrap();
        let item = java_registry::item(id, english).unwrap_or_else(|| panic!("{id} resolves"));
        let p = dispensing::projectile(item).unwrap_or_else(|| panic!("{id} shoots"));
        let close = |a: f64, b: &Value| (a - b.as_f64().unwrap()).abs() < 1e-6;
        assert!(close(p.power, &config["power"]), "{id} power");
        assert!(close(p.uncertainty, &config["uncertainty"]), "{id} uncertainty");
        assert_eq!(u64::from(p.event), config["event"].as_u64().unwrap(), "{id} event");
        for (name, step) in dirs {
            let got = dispensing::projectile_offset(item, step);
            let want = &config["offset"][name];
            for (axis, v) in [got.x, got.y, got.z].into_iter().enumerate() {
                assert!(close(v, &want[axis]), "{id} {name} offset {got}");
            }
        }
    }
}

/// `Equippable.allowedEntities` tags against the mob kinds the game admits.
#[test]
fn allowed_wearers_follow_the_java_entity_tags() {
    let fx = fixture();
    let kinds = [
        ("camel", MobKind::Camel),
        ("camel_husk", MobKind::CamelHusk),
        ("donkey", MobKind::Donkey),
        ("horse", MobKind::Horse),
        ("mule", MobKind::Mule),
        ("pig", MobKind::Pig),
        ("skeleton_horse", MobKind::SkeletonHorse),
        ("strider", MobKind::Strider),
        ("zombie_horse", MobKind::ZombieHorse),
        ("happy_ghast", MobKind::HappyGhast),
        ("llama", MobKind::Llama),
        ("wolf", MobKind::Wolf),
        ("cow", MobKind::Cow),
    ];
    for (tag, allowed) in [
        ("can_equip_saddle", Allowed::Saddle),
        ("can_equip_harness", Allowed::Harness),
        ("can_wear_horse_armor", Allowed::HorseArmor),
    ] {
        let members: BTreeSet<&str> =
            fx["entity_type_tags"][tag].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        for (name, kind) in kinds {
            assert_eq!(allowed.admits(kind), members.contains(name), "{tag}: {name}");
        }
    }
}

// ---- Behaviours through a dispenser ---------------------------------------

const P: IVec3 = IVec3::new(4, 145, 8);
/// Facing east (+X).
const EAST: u8 = 2;

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.receive_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)));
        }
    }
    for x in 0..12 {
        for z in 0..16 {
            w.set_block(IVec3::new(x, 144, z), Block::STONE);
        }
    }
    w
}

/// A dispenser at `P` facing east holding `stack`, powered, run until it
/// fires and the entity pass has run.
fn fire(w: &mut World, e: &mut Entities, stack: Stack, players: &mut [DispenseTarget<'_>]) {
    w.set_block(P, r::dispenser(EAST, false));
    w.chest_mut(P).unwrap().slots[0] = Some(stack);
    w.set_block(P - IVec3::Z, r::REDSTONE_BLOCK);
    for _ in 0..5 {
        w.tick_redstone();
        w.tick_automation_entities_with(e, players);
    }
    w.set_block(P - IVec3::Z, Block::AIR);
    w.tick_redstone();
}

fn held(w: &World) -> Option<Stack> {
    w.chest(P).unwrap().slots[0]
}

const FRONT: IVec3 = IVec3::new(5, 145, 8);

fn centre(cell: IVec3) -> DVec3 {
    cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
}

#[test]
fn equipment_goes_onto_stands_looting_mobs_and_players_or_pops_out() {
    // Nobody in front: the helmet pops out (`EquipmentDispenseItemBehavior`
    // falls back to the plain drop).
    let mut w = world();
    let mut e = Entities::new(1);
    fire(&mut w, &mut e, Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 2), &mut []);
    assert_eq!(held(&w).map(|s| s.count), Some(1));
    assert_eq!(e.items.len(), 1);

    // An armour stand in front takes it.
    let mut w = world();
    let mut e = Entities::new(1);
    e.decor.push(voxelcraft::entity::decor::Decor::dispensed_stand(FRONT, 0.0));
    fire(&mut w, &mut e, Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1), &mut []);
    assert_eq!(held(&w), None);
    assert_eq!(e.decor[0].armor[0].map(|s| s.item), Some(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron)));
    assert!(e.items.is_empty());

    // A zombie that can't pick up loot is passed over; one that can wears it.
    for (loot, worn) in [(false, false), (true, true)] {
        let mut w = world();
        let mut e = Entities::new(1);
        e.spawn(MobKind::Zombie, centre(FRONT));
        e.mobs[0].can_pick_up_loot = loot;
        fire(&mut w, &mut e, Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Iron), 1), &mut []);
        assert_eq!(e.mobs[0].equipment.get(Slot::Chest).is_some(), worn);
        assert_eq!(e.items.len(), usize::from(!worn), "the plate pops out when nobody takes it");
        if worn {
            assert!(e.mobs[0].persistent);
            assert!(e.mobs[0].equipment.drop_chance(Slot::Chest) > 1.0, "a guaranteed drop");
        }
    }

    // A player in front gets boots, and a shield in the off hand.
    let mut w = world();
    let mut e = Entities::new(1);
    let mut armor = [None; 4];
    let mut offhand = None;
    for item in [Item::armor(ArmorPiece::Boots, ArmorMaterial::Iron), Item::SHIELD] {
        let mut players = [DispenseTarget {
            pos: centre(FRONT),
            half_width: 0.3,
            height: 1.8,
            can_equip: true,
            armor: &mut armor,
            offhand: &mut offhand,
        }];
        fire(&mut w, &mut e, Stack::new(item, 1), &mut players);
    }
    assert_eq!(armor[3].map(|s| s.item), Some(Item::armor(ArmorPiece::Boots, ArmorMaterial::Iron)));
    assert_eq!(offhand.map(|s| s.item), Some(Item::SHIELD));
}

#[test]
fn saddles_need_a_tamed_horse_but_not_a_tamed_pig() {
    for (kind, tame, saddled) in
        [(MobKind::Horse, false, false), (MobKind::Horse, true, true), (MobKind::Pig, false, true)]
    {
        let mut w = world();
        let mut e = Entities::new(1);
        e.spawn(kind, centre(FRONT));
        let m = &mut e.mobs[0];
        m.baby = false;
        m.age = 0;
        m.mount.as_mut().unwrap().tame = tame || kind == MobKind::Pig;
        fire(&mut w, &mut e, Stack::new(Item::SADDLE, 1), &mut []);
        assert_eq!(e.mobs[0].mount.as_ref().unwrap().saddled(), saddled, "{kind:?} tame {tame}");
    }
}

#[test]
fn shears_take_wool_and_wear_glass_bottles_fill_from_water() {
    let mut w = world();
    let mut e = Entities::new(1);
    e.spawn(MobKind::Sheep, centre(FRONT));
    e.mobs[0].baby = false;
    e.mobs[0].age = 0;
    fire(&mut w, &mut e, Stack::new(Item::SHEARS, 1), &mut []);
    assert!(e.mobs[0].sheared);
    assert_eq!(held(&w).map(|s| s.damage), Some(1), "shears wear by one");
    assert!(e.items.iter().any(|i| i.stack.item.block().is_some()), "wool dropped");

    // Nothing to shear: the shears stay and don't wear.
    let mut w = world();
    let mut e = Entities::new(1);
    fire(&mut w, &mut e, Stack::new(Item::SHEARS, 1), &mut []);
    assert_eq!(held(&w).map(|s| s.damage), Some(0));
    assert!(e.items.is_empty());

    // A glass bottle in front of water becomes a water bottle in the same slot.
    let mut w = world();
    let mut e = Entities::new(1);
    w.set_block(FRONT, Block::WATER);
    fire(&mut w, &mut e, Stack::new(Item::GLASS_BOTTLE, 1), &mut []);
    assert_eq!(held(&w).map(|s| s.item), Some(Item::potion(voxelcraft::potion::Potion::WATER)));
}

#[test]
fn glowstone_charges_anchors_honeycomb_waxes_and_water_makes_mud() {
    use voxelcraft::world::utilities::{anchor_charges, respawn_anchor};
    let mut w = world();
    let mut e = Entities::new(1);
    w.set_block(FRONT, respawn_anchor(3));
    fire(&mut w, &mut e, Stack::new(Item::from_block(Block::GLOWSTONE), 2), &mut []);
    assert_eq!(w.get_block(FRONT).and_then(anchor_charges), Some(4));
    assert_eq!(held(&w).map(|s| s.count), Some(1));
    // A full anchor keeps the glowstone (`setSuccess(false)`).
    fire(&mut w, &mut e, Stack::new(Item::from_block(Block::GLOWSTONE), 1), &mut []);
    assert_eq!(held(&w).map(|s| s.count), Some(1));
    assert!(e.items.is_empty());

    let mut w = world();
    let mut e = Entities::new(1);
    w.set_block(FRONT, Block::DIRT);
    fire(&mut w, &mut e, Stack::new(Item::potion(voxelcraft::potion::Potion::WATER), 1), &mut []);
    assert_eq!(w.get_block(FRONT), Some(voxelcraft::world::overworld_blocks::MUD));
    assert_eq!(held(&w).map(|s| s.item), Some(Item::GLASS_BOTTLE));

    let mut w = world();
    let mut e = Entities::new(1);
    let copper = voxelcraft::world::copper::encode(voxelcraft::world::copper::Copper {
        family: voxelcraft::world::copper::Family::Block,
        stage: voxelcraft::world::copper::Stage::Unaffected,
        local: 0,
        waxed: false,
    });
    w.set_block(FRONT, copper);
    fire(&mut w, &mut e, Stack::new(Item::HONEYCOMB, 1), &mut []);
    assert!(w.get_block(FRONT).and_then(voxelcraft::world::copper::state).is_some_and(|c| c.waxed));
    assert_eq!(held(&w), None);
}

#[test]
fn flint_and_steel_lights_fire_and_wears() {
    let mut w = world();
    let mut e = Entities::new(1);
    w.set_block(FRONT - IVec3::Y, Block::NETHERRACK);
    fire(&mut w, &mut e, Stack::new(Item::FLINT_AND_STEEL, 1), &mut []);
    assert!(w.get_block(FRONT).is_some_and(|b| b.is_fire()));
    assert_eq!(held(&w).map(|s| s.damage), Some(1));
}

#[test]
fn boats_float_on_water_and_carts_take_rails() {
    let mut w = world();
    let mut e = Entities::new(1);
    w.set_block(FRONT, Block::WATER);
    fire(&mut w, &mut e, Stack::new(Item::boat(0, false), 1), &mut []);
    assert_eq!(e.minecarts.len(), 1);
    assert_eq!(held(&w), None);
    // `0.5625 + 1.375 / 2` out, a block up for water in front.
    let pos = e.minecarts[0].pos;
    assert!((pos.x - (4.5 + 1.25)).abs() < 1e-9 && (pos.y - 146.5).abs() < 1e-9, "{pos}");

    // A boat with neither water nor air over water in front pops out.
    let mut w = world();
    let mut e = Entities::new(1);
    fire(&mut w, &mut e, Stack::new(Item::boat(0, false), 1), &mut []);
    assert!(e.minecarts.is_empty());
    assert_eq!(e.items.len(), 1);

    let mut w = world();
    let mut e = Entities::new(1);
    w.set_block(FRONT, Block::RAIL);
    fire(&mut w, &mut e, Stack::new(Item::MINECART, 1), &mut []);
    assert_eq!(e.minecarts.len(), 1);
    assert!((e.minecarts[0].pos.y - 145.1).abs() < 1e-9, "a flat rail: 0.1 up");
}

#[test]
fn armour_stands_and_thrown_items_leave_the_dispenser() {
    let mut w = world();
    let mut e = Entities::new(1);
    fire(&mut w, &mut e, Stack::new(Item::ARMOR_STAND, 1), &mut []);
    assert_eq!(e.decor.len(), 1);
    assert_eq!(e.decor[0].pos, centre(FRONT));
    assert_eq!(e.decor[0].yaw, 270.0, "east is Java's 270 degrees");

    for item in [Item::SNOWBALL, Item::EGG, voxelcraft::world::trial::WIND_CHARGE] {
        let mut w = world();
        let mut e = Entities::new(1);
        fire(&mut w, &mut e, Stack::new(item, 1), &mut []);
        assert_eq!(e.thrown.len(), 1, "{}", item.name());
        assert!(e.thrown[0].velocity().x > 0.0);
    }
    let mut w = world();
    let mut e = Entities::new(1);
    let splash = Item::splash_potion(voxelcraft::potion::Potion::from_id("healing").unwrap());
    fire(&mut w, &mut e, Stack::new(splash, 1), &mut []);
    assert_eq!(e.potions.len(), 1);
    // `power 1.375` blocks a tick east, give or take the noise.
    let v = e.potions[0].vel / 20.0;
    assert!((v.length() - 1.375).abs() < 0.3, "{v}");
}

#[test]
fn queued_dispenses_survive_save_and_reload() {
    let mut w = world();
    w.set_block(P, r::dispenser(EAST, false));
    w.chest_mut(P).unwrap().slots[3] = Some(Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1));
    w.set_block(P - IVec3::Z, r::REDSTONE_BLOCK);
    for _ in 0..5 {
        w.tick_redstone();
    }
    assert!(w.has_deferred_dispenses());
    let saved = w.automation_to_string();
    assert!(saved.contains("|d,4,145,8,2,3,"), "{saved}");
    let mut loaded = world();
    loaded.set_block(P, r::dispenser(EAST, false));
    loaded.chest_mut(P).unwrap().slots[3] = Some(Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1));
    loaded.load_automation(&saved);
    assert!(loaded.has_deferred_dispenses());
    let mut e = Entities::new(1);
    loaded.tick_automation_entities(&mut e);
    assert_eq!(e.items.len(), 1, "nobody in front: the helmet pops out once");
    assert_eq!(loaded.chest(P).unwrap().slots[3], None);
    loaded.tick_automation_entities(&mut e);
    assert_eq!(e.items.len(), 1);
}
