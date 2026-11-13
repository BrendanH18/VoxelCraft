//! Java 26.3 armor equipment against `fixtures/java/26.3/equipment.json`
//! (`tools/extract_java_equipment_fixtures.py`): every humanoid, wolf, horse
//! and nautilus armor item's material and form both ways, the armorer's armor
//! offers, the nugget and netherite recipes that take the v0.17 armors, and
//! the chest tables that hold them.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    enchant,
    entity::{
        Entities,
        armor::ArmorKind,
        mounts,
        villager::{Profession, level_offers},
    },
    equipment::{self, BodyArmor},
    inventory::{Stack, stack_from_str, stack_to_string},
    item::{ArmorMaterial, ArmorPiece, Item},
    java_registry, smithing,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        furnace::{FurnaceKind, smelt_xp},
        loot,
        terrain::Generator,
    },
};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The registered item for a Java id: by id, or by its English name.
fn registered(names: &Value, id: &str) -> Option<Item> {
    let id = id.trim_start_matches("minecraft:");
    java_registry::item(id, names["items"][id].as_str().unwrap_or(""))
}

fn material(name: &str) -> Option<ArmorMaterial> {
    Some(match name {
        "leather" => ArmorMaterial::Leather,
        "copper" => ArmorMaterial::Copper,
        "chainmail" => ArmorMaterial::Chainmail,
        "iron" => ArmorMaterial::Iron,
        "gold" => ArmorMaterial::Gold,
        "diamond" => ArmorMaterial::Diamond,
        "netherite" => ArmorMaterial::Netherite,
        "turtle_scute" => ArmorMaterial::Turtle,
        // Wolf armor's material; the wolf armor item is checked on its own.
        "armadillo_scute" => return None,
        other => panic!("unknown reference armor material {other}"),
    })
}

fn piece(name: &str) -> ArmorPiece {
    match name {
        "helmet" => ArmorPiece::Helmet,
        "chestplate" => ArmorPiece::Chestplate,
        "leggings" => ArmorPiece::Leggings,
        "boots" => ArmorPiece::Boots,
        other => panic!("not a humanoid armor type {other}"),
    }
}

/// Every Java armor item is registered with its material's numbers and its
/// form's components, and the game has no armor Java lacks.
#[test]
fn every_armor_item_matches_its_java_material_and_form() {
    let f = fixture("equipment.json");
    let names = fixture("registry_names.json");
    let gaps = fixture("registry_gaps.json");
    let mut seen = BTreeSet::new();
    let mut missing = Vec::new();
    for (id, facts) in f["items"].as_object().unwrap() {
        let Some(item) = registered(&names, id) else {
            assert!(gaps["items"].get(id).is_some(), "{id} is neither registered nor a listed gap");
            missing.push(id.as_str());
            continue;
        };
        assert!(seen.insert(item.0), "{id} shares an item");
        let name = facts["material"].as_str().unwrap();
        let m = &f["materials"][name];
        let form = facts["form"].as_str().unwrap();
        let rules = &f["forms"][form];
        let ty = facts["type"].as_str().unwrap();
        let defense = m["defense"][ty].as_u64().unwrap();
        let units = f["armor_types"][ty]["durability_unit"].as_u64().unwrap();
        let durability =
            rules["durability"].as_bool().unwrap().then(|| m["durability_multiplier"].as_u64().unwrap() * units);
        let repair_items: Vec<Item> = m["repair_items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| registered(&names, r.as_str().unwrap()))
            .collect();
        assert_eq!(item.durability().map(u64::from), durability, "{id} durability");
        assert_eq!(u64::from(item.max_stack()), rules["stacks_to"].as_u64().unwrap_or(1), "{id} stack");
        assert_eq!(item.fire_resistant(), facts["fire_resistant"].as_bool().unwrap(), "{id} fire resistance");
        assert_eq!(item.is_uncommon(), facts["rarity"] == "uncommon", "{id} rarity");
        assert_eq!(item.is_rare(), facts["rarity"] == "rare", "{id} rarity");
        let enchantable = rules["enchantable"].as_bool().unwrap().then(|| m["enchantability"].as_u64().unwrap());
        assert_eq!(enchant::enchantability(item), enchantable.unwrap_or(0) as u32, "{id} enchantability");
        for &r in &repair_items {
            assert_eq!(enchant::repairs(item, r), rules["repairable"].as_bool().unwrap(), "{id} with {}", r.name());
        }
        match form {
            "humanoid" => {
                let material = material(name).unwrap();
                let piece = piece(ty);
                assert_eq!(item.as_armor(), Some((piece, material)), "{id}");
                assert_eq!(Item::armor(piece, material), item, "{id}");
                assert_eq!(u64::from(material.defense(piece)), defense, "{id} defense");
                assert_eq!(f64::from(material.toughness()), m["toughness"].as_f64().unwrap(), "{id} toughness");
                let kb = m["knockback_resistance"].as_f64().unwrap() as f32;
                assert_eq!(material.knockback_resistance(), kb, "{id} knockback resistance");
                assert!(item.body_armor().is_none());
            }
            "horse" | "nautilus" => {
                let fits = if form == "horse" { BodyArmor::Horse } else { BodyArmor::Nautilus };
                let material = material(name).unwrap();
                assert_eq!(item.body_armor(), Some((fits, material)), "{id}");
                assert_eq!(item.as_armor(), None, "{id} is not humanoid armor");
                assert_eq!(u64::from(material.body_defense()), defense, "{id} body defense");
                let horse = if fits == BodyArmor::Horse { defense } else { 0 };
                assert_eq!(u64::from(mounts::armor_points(item)), horse, "{id} on a horse");
                if fits == BodyArmor::Horse {
                    assert_eq!(f64::from(mounts::armor_toughness(item)), m["toughness"].as_f64().unwrap());
                    let kb = m["knockback_resistance"].as_f64().unwrap() as f32;
                    assert_eq!(mounts::armor_knockback_resistance(item), kb, "{id}");
                }
            }
            "wolf" => assert_eq!(item, Item::WOLF_ARMOR),
            other => panic!("unknown armor form {other}"),
        }
        assert!(Item::creative_palette().any(|i| i == item), "{id} in the creative palette");
        assert!(item.icon_layer().is_some(), "{id} icon");
    }
    assert!(missing.is_empty(), "unregistered armor: {missing:?}");
    for item in Item::all_items() {
        if item.as_armor().is_some() || item.body_armor().is_some() || item == Item::WOLF_ARMOR {
            assert!(seen.contains(&item.0), "{} is armor Java does not register", item.name());
        }
    }
    // Every humanoid material the game has is a Java material with all four
    // pieces (turtle has only its helmet).
    for m in ArmorMaterial::ALL {
        for p in ArmorPiece::ALL {
            assert!(seen.contains(&Item::armor(p, m).0), "{m:?} {p:?}");
        }
    }
}

/// Chainmail is worn like any humanoid armor: a CLI player equips it from
/// the hand, the HUD counts Java's 12 points for the set, the player model
/// draws the chain sheet, and worn and stored stacks keep damage and
/// enchantments across a save.
#[test]
fn chainmail_is_equipped_worn_and_saved() {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut entities = Entities::new(1);
    let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    for (slot, piece) in ArmorPiece::ALL.into_iter().enumerate() {
        a.session.inventory.slots[slot] = Some(Stack::new(Item::armor(piece, ArmorMaterial::Chainmail), 1));
    }
    for slot in 0..4 {
        a.selected = slot;
        a.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.session.inventory.get(slot), None, "slot {slot} equipped");
    }
    let f = fixture("equipment.json");
    let set: u64 = ["helmet", "chestplate", "leggings", "boots"]
        .iter()
        .map(|t| f["materials"]["chainmail"]["defense"][t].as_u64().unwrap())
        .sum();
    assert_eq!(u64::from(a.session.inventory.armor_points()), set);
    assert_eq!(a.session.inventory.armor_toughness(), 0.0);
    let worn = voxelcraft::entity::armor::from_stacks(&a.session.inventory.armor);
    assert!(worn.iter().all(|w| w.is_some_and(|w| w.kind == ArmorKind::Chain)));
    let enchants = enchant::Enchants::NONE.with(enchant::Enchantment::Protection, 3);
    let chest = Some(Stack {
        damage: 40,
        enchants,
        repair_cost: 1,
        ..Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Chainmail), 1)
    });
    assert_eq!(stack_from_str(&stack_to_string(chest)), Some(chest));
}

/// `data/minecraft/villager_trade/armorer/*`: the plain armor offers the
/// armorer rolls at each level, with Java's price, uses, xp and discount.
#[test]
fn armorer_armor_offers_match_java() {
    let f = fixture("equipment.json");
    let names = fixture("registry_names.json");
    let mut chainmail = 0;
    for t in f["trades"].as_array().unwrap() {
        // Enchanted and dyed offers price their extras separately.
        if t["enchanted"] == true || t["profession"] != "armorer" {
            continue;
        }
        let level = t["level"].as_u64().unwrap() as u8;
        let gives = registered(&names, t["gives"]["id"].as_str().unwrap()).unwrap();
        let wants = registered(&names, t["wants"]["id"].as_str().unwrap()).unwrap();
        let offers = level_offers(Profession::Armorer, level);
        let offer = offers.iter().find(|o| o.output.item == gives).unwrap_or_else(|| panic!("{}", t["path"]));
        assert_eq!(offer.cost, Stack::new(wants, t["wants"]["count"].as_u64().unwrap() as u8), "{}", t["path"]);
        assert_eq!(offer.second, None);
        assert_eq!(u64::from(offer.output.count), t["gives"]["count"].as_u64().unwrap());
        assert_eq!(u64::from(offer.max_uses), t["max_uses"].as_u64().unwrap(), "{}", t["path"]);
        assert_eq!(u64::from(offer.xp), t["xp"].as_u64().unwrap(), "{}", t["path"]);
        assert_eq!(offer.multiplier, t["reputation_discount"].as_f64().unwrap() as f32, "{}", t["path"]);
        if gives.as_armor().is_some_and(|(_, m)| m == ArmorMaterial::Chainmail) {
            chainmail += 1;
        }
    }
    assert_eq!(chainmail, 4);
}

fn recipe<'a>(recipes: &'a Value, kind: &str, id: &str) -> &'a Value {
    let r = &recipes["recipes"][kind][id];
    assert!(r.is_object(), "{kind}/{id}");
    r
}

fn ingredients(v: &Value) -> Vec<&str> {
    match v {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().map(|s| s.as_str().unwrap()).collect(),
        other => panic!("ingredient {other}"),
    }
}

/// `recipe/{iron,gold,copper}_nugget_from_{smelting,blasting}.json`: the
/// v0.17 armors among the ingredients melt to one nugget in a furnace and a
/// blast furnace, never in a smoker; the rest of the v0.17 armors don't melt.
#[test]
fn chainmail_and_metal_body_armor_melt_to_nuggets() {
    let recipes = fixture("recipes.json");
    let names = fixture("registry_names.json");
    let new: BTreeSet<u16> = equipment::all().map(|i| i.0).collect();
    let mut melted = BTreeSet::new();
    for metal in ["iron", "gold", "copper"] {
        for (kind, furnace) in [("smelting", FurnaceKind::Normal), ("blasting", FurnaceKind::Blast)] {
            let r = recipe(&recipes, kind, &format!("{metal}_nugget_from_{kind}"));
            let result = registered(&names, r["result"].as_str().unwrap()).unwrap();
            assert_eq!(smelt_xp(result), r["experience"].as_f64().unwrap() as f32);
            for id in ingredients(&r["ingredient"]) {
                let item = registered(&names, id).unwrap_or_else(|| panic!("{id}"));
                if new.contains(&item.0) {
                    assert_eq!(furnace.smelt(item), Some(result), "{kind}: {id}");
                    assert_eq!(FurnaceKind::Smoker.smelt(item), None, "smoking {id}");
                    melted.insert(item.0);
                }
            }
        }
    }
    // Four chainmail pieces, copper horse armor and three nautilus armors.
    assert_eq!(melted.len(), 8);
    for item in equipment::all().filter(|i| !melted.contains(&i.0)) {
        assert_eq!(FurnaceKind::Normal.smelt(item), None, "{}", item.name());
        assert_eq!(FurnaceKind::Blast.smelt(item), None, "{}", item.name());
    }
}

/// `recipe/netherite_{horse,nautilus}_armor_smithing.json` through the
/// shared smithing rule, as the CLI agent runs it at a smithing table.
#[test]
fn netherite_body_armor_is_smithed_from_diamond() {
    let recipes = fixture("recipes.json");
    let names = fixture("registry_names.json");
    for id in ["netherite_horse_armor_smithing", "netherite_nautilus_armor_smithing"] {
        let r = recipe(&recipes, "smithing_transform", id);
        let item = |key: &str| registered(&names, r[key].as_str().unwrap()).unwrap();
        let (template, base, result) = (item("template"), item("base"), item("result"));
        assert_eq!(r["addition"], "#netherite_tool_materials");
        let addition = Item::NETHERITE_INGOT;
        assert!(smithing::is_base(base) && smithing::is_template(template), "{id}");
        let one = |i| Stack::new(i, 1);
        assert_eq!(smithing::upgrade(one(template), one(base), one(addition)), Some(one(result)), "{id}");
        assert_eq!(smithing::upgrade(one(template), one(result), one(addition)), None, "{id} again");
        assert!(!smithing::is_base(result));

        let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
        world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        world.set_block(IVec3::new(4, 151, 1), Block::SMITHING_TABLE);
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.session.inventory.slots[0] = Some(one(base));
        a.session.inventory.slots[1] = Some(one(template));
        a.session.inventory.slots[2] = Some(one(addition));
        a.execute(Command::Smithing, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.session.inventory.get(0), Some(one(result)), "{id} at a table");
        assert_eq!((a.session.inventory.get(1), a.session.inventory.get(2)), (None, None));
    }
}

/// Expected stacks of `name` per roll of `table`: for each pool, the mean
/// roll count times the entry's share of the pool's weight.
fn expected_per_chest(tables: &Value, table: &str, name: &str) -> f64 {
    let mut expected = 0.0;
    for pool in tables[table]["pools"].as_array().unwrap() {
        let rolls = &pool["rolls"];
        let mean =
            rolls.as_f64().unwrap_or_else(|| (rolls["min"].as_f64().unwrap() + rolls["max"].as_f64().unwrap()) / 2.0);
        let entries = pool["entries"].as_array().unwrap();
        let weight = |e: &Value| e["weight"].as_f64().unwrap_or(1.0);
        let total: f64 = entries.iter().map(weight).sum();
        for e in entries.iter().filter(|e| e["name"] == name) {
            assert!(e.get("conditions").is_none(), "{table}: conditional {name}");
            expected += mean * weight(e) / total;
        }
    }
    expected
}

/// The jar's chest tables that hold copper horse armor or the nautilus
/// armor pool now award them at their weights (they rolled blank before).
#[test]
fn chest_loot_awards_the_new_armor_at_java_weights() {
    let tables: Value = serde_json::from_str(include_str!("../src/world/loot_tables.json")).unwrap();
    let tables = &tables["tables"];
    let horse = ["desert_pyramid", "jungle_temple", "nether_bridge", "simple_dungeon", "stronghold_corridor"];
    let ocean = [
        "buried_treasure",
        "shipwreck_map",
        "shipwreck_supply",
        "shipwreck_treasure",
        "underwater_ruin_big",
        "underwater_ruin_small",
    ];
    let nautilus = ["copper", "iron", "golden", "diamond"].map(|m| format!("minecraft:{m}_nautilus_armor"));
    let mut cases: Vec<(&str, String)> =
        horse.iter().map(|t| (*t, "minecraft:copper_horse_armor".to_string())).collect();
    for t in ocean {
        cases.extend(nautilus.iter().map(|n| (t, n.clone())));
    }
    const ROLLS: u64 = 4000;
    for table in horse.iter().chain(&ocean) {
        let mut counts = std::collections::BTreeMap::new();
        for seed in 0..ROLLS {
            for stack in loot::roll_items(table, &mut loot::LootRandom::new(seed * 7919 + 13), Some) {
                *counts.entry(stack.item.0).or_insert(0u64) += u64::from(stack.count);
            }
        }
        for (_, name) in cases.iter().filter(|(t, _)| t == table) {
            let item = loot::java_item(name.trim_start_matches("minecraft:")).unwrap();
            assert!(new_item(item), "{name}");
            let expected = expected_per_chest(tables, table, name) * ROLLS as f64;
            assert!(expected > 0.0, "{table}: {name} not in the table");
            let got = counts.get(&item.0).copied().unwrap_or(0) as f64;
            assert!((got - expected).abs() <= 5.0 * expected.sqrt() + 2.0, "{table}: {name} {got} of ~{expected:.0}");
        }
    }
}

fn new_item(item: Item) -> bool {
    equipment::all().any(|i| i == item)
}
