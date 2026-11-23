//! v0.17 Track G: the core workstations against
//! `tests/fixtures/java/26.3/workstations.json` (made by
//! `tools/extract_java_workstation_fixtures.py` from the official server
//! jar, with `tools/java_workstation_models.py`'s independent models).
//! Furnaces, smokers and blast furnaces replay Java's tick scenarios;
//! brewing stands likewise; the enchanting table's offers, anvil and
//! grindstone results and falling-anvil wear are compared case by case;
//! comparator output, bookshelf positions and the smithing slots too. Lists
//! the game keeps (fuels, reagents, bottle inputs, smithing slots,
//! enchantability) are checked both ways.

use glam::IVec3;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, OnceLock};
use voxelcraft::enchant::{self, Enchantment, Enchants};
use voxelcraft::inventory::Stack;
use voxelcraft::item::Item;
use voxelcraft::potion::Potion;
use voxelcraft::world::{
    World,
    block::Block,
    brewing::{self, BrewingStand},
    chunk::ChunkData,
    furnace::{self, Furnace, FurnaceKind},
    terrain::Generator,
};
use voxelcraft::{grindstone, java_registry, smithing};

fn fixture() -> &'static Value {
    static F: OnceLock<Value> = OnceLock::new();
    F.get_or_init(|| serde_json::from_str(include_str!("fixtures/java/26.3/workstations.json")).unwrap())
}

fn registry() -> &'static Value {
    static R: OnceLock<Value> = OnceLock::new();
    R.get_or_init(|| serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap())
}

fn registered(id: &str) -> Option<Item> {
    java_registry::item(id, registry()["items"][id].as_str().unwrap_or(""))
}

fn item(id: &str) -> Item {
    registered(id).unwrap_or_else(|| panic!("{id} is not registered"))
}

/// Every registered Java item id with its item.
fn all_items() -> Vec<(String, Item)> {
    registry()["items"].as_object().unwrap().keys().filter_map(|id| Some((id.clone(), registered(id)?))).collect()
}

fn strings(v: &Value) -> BTreeSet<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_owned()).collect()
}

fn enchantment(id: &str) -> Enchantment {
    Enchantment::from_name(id).unwrap_or_else(|| panic!("no enchantment {id}"))
}

fn enchantments(v: &Value) -> Enchants {
    let mut out = Enchants::NONE;
    for (e, l) in v.as_object().unwrap() {
        out.set(enchantment(e), l.as_u64().unwrap() as u8);
    }
    out
}

/// A fixture stack: `{item, count, damage, enchantments, repair_cost, name}`.
fn stack(v: &Value) -> Option<Stack> {
    if v.is_null() {
        return None;
    }
    let mut s = Stack::new(item(v["item"].as_str().unwrap()), v["count"].as_u64().unwrap() as u8);
    s.damage = v["damage"].as_u64().unwrap() as u16;
    s.enchants = enchantments(&v["enchantments"]);
    s.repair_cost = v["repair_cost"].as_u64().unwrap() as u16;
    if let Some(name) = v["name"].as_str() {
        s = s.with_name(name).unwrap();
    }
    Some(s)
}

/// A fixture `[item, count]` pair.
fn pair(v: &Value) -> Option<Stack> {
    let a = v.as_array()?;
    Some(Stack::new(item(a[0].as_str().unwrap()), a[1].as_u64().unwrap() as u8))
}

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

const AT: IVec3 = IVec3::new(8, 140, 8);

#[test]
fn furnace_constants_speeds_and_recipe_times_match_java() {
    let f = &fixture()["furnace"];
    assert_eq!(f["constants"]["BURN_TIME_STANDARD"], furnace::BURN_TIME_STANDARD);
    assert_eq!(f["constants"]["BURN_COOL_SPEED"], furnace::BURN_COOL_SPEED);
    assert_eq!(f["constants"]["DEFAULT_SPEED_MULTIPLIER"], Furnace::default().speed as f64);
    assert_eq!(f["speed_multipliers"]["fast"], furnace::FAST_SPEED_MULTIPLIER as f64);
    assert_eq!(f["speed_multipliers"]["normal"], FurnaceKind::Normal.speed_multiplier() as f64);
    assert_eq!(f["burn_time_reductions"]["fast"], furnace::FAST_BURN_REDUCTION);
    assert_eq!(f["burn_time_reductions"]["normal"], 1);
    let fast = strings(&f["fast_cooking_blocks"]);
    for (block, kind) in
        [("furnace", Block::FURNACE), ("smoker", Block::SMOKER), ("blast_furnace", Block::BLAST_FURNACE)]
    {
        assert_eq!(FurnaceKind::of(kind).fast(), fast.contains(block), "#block/fast_cooking: {block}");
    }
    for time in f["recipe_cooking_times"].as_array().unwrap() {
        assert_eq!(time[1], furnace::RECIPE_COOK_TICKS, "{time}");
    }
    assert_eq!(FurnaceKind::Normal.cook_ticks(), 200);
    assert_eq!(FurnaceKind::Smoker.cook_ticks(), 100);
}

#[test]
fn every_fuel_burns_its_java_time_in_each_furnace_both_ways() {
    let fuels = fixture()["furnace"]["fuels"].as_object().unwrap();
    let mut compared = 0;
    for (id, ticks) in fuels {
        let Some(i) = registered(id) else { continue };
        let ticks = ticks.as_u64().unwrap() as u32;
        assert_eq!(FurnaceKind::Normal.burn_ticks(i), Some(ticks), "{id}");
        assert_eq!(FurnaceKind::Smoker.burn_ticks(i), Some(ticks / 2), "{id} in a smoker");
        assert_eq!(FurnaceKind::Blast.burn_ticks(i), Some(ticks / 2), "{id} in a blast furnace");
        compared += 1;
    }
    for (id, i) in all_items() {
        if voxelcraft::content::fuel::ticks(i).is_some() {
            assert!(fuels.contains_key(&id), "{id} burns but isn't a Java 26.3 fuel");
        }
    }
    assert!(compared > 300, "{compared} fuels compared");
}

fn kind_of(block: &str) -> FurnaceKind {
    match block {
        "furnace" => FurnaceKind::Normal,
        "smoker" => FurnaceKind::Smoker,
        "blast_furnace" => FurnaceKind::Blast,
        _ => panic!("{block}"),
    }
}

#[test]
fn furnace_scenarios_match_java_tick_for_tick() {
    let f = &fixture()["furnace"];
    let xp = f["recipe_experience"].as_object().unwrap();
    for case in f["scenarios"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let mut furnace = Furnace { kind: kind_of(case["block"].as_str().unwrap()), ..Default::default() };
        let mut states = case["states"].as_array().unwrap().iter();
        for op in case["ops"].as_array().unwrap() {
            match op[0].as_str().unwrap() {
                "input" => furnace.set_input(pair(&Value::Array(op.as_array().unwrap()[1..].to_vec()))),
                "fuel" => furnace.fuel = pair(&Value::Array(op.as_array().unwrap()[1..].to_vec())),
                "take_output" => furnace.output = None,
                "tick" => {
                    for _ in 0..op[1].as_u64().unwrap() {
                        furnace.step();
                    }
                    let want = states.next().unwrap();
                    let at = format!("{name} after {op}");
                    assert_eq!(furnace.lit, want["lit"].as_u64().unwrap() as u32, "lit: {at}");
                    assert_eq!(furnace.lit_total, want["lit_total"].as_u64().unwrap() as u32, "lit total: {at}");
                    assert_eq!(furnace.cook, want["cook"].as_u64().unwrap() as u32, "cook: {at}");
                    assert_eq!(furnace.cook_total, want["cook_total"].as_u64().unwrap() as u32, "cook total: {at}");
                    assert_eq!(furnace.speed as f64, want["speed"].as_f64().unwrap(), "speed: {at}");
                    assert_eq!(furnace.input, pair(&want["input"]), "input: {at}");
                    assert_eq!(furnace.fuel, pair(&want["fuel"]), "fuel: {at}");
                    assert_eq!(furnace.output, pair(&want["output"]), "output: {at}");
                    // The tally: each Java recipe's count and experience.
                    let mut want_used: Vec<(u32, u32)> = want["recipes_used"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(id, n)| ((xp[id].as_f64().unwrap() as f32).to_bits(), n.as_u64().unwrap() as u32))
                        .collect();
                    let mut used: Vec<(u32, u32)> =
                        furnace.recipes_used.iter().map(|r| (r.xp.to_bits(), r.count)).collect();
                    want_used.sort();
                    used.sort();
                    assert_eq!(used, want_used, "recipes used: {at}");
                }
                other => panic!("unknown op {other}"),
            }
        }
    }
}

#[test]
fn furnace_experience_rounds_each_recipe_like_create_experience() {
    for case in fixture()["furnace"]["experience"].as_array().unwrap() {
        let count = case["count"].as_u64().unwrap() as u32;
        let xp = case["xp"].as_f64().unwrap() as f32;
        let whole = case["whole"].as_u64().unwrap() as u32;
        let fraction = case["fraction"].as_f64().unwrap() as f32;
        let up = (fraction > 0.0) as u32;
        assert_eq!(furnace::award(count, xp, 0.0), whole + up, "{case}");
        assert_eq!(furnace::award(count, xp, fraction), whole, "{case}: a roll equal to the fraction");
        if fraction > 0.0 {
            assert_eq!(furnace::award(count, xp, fraction - f32::EPSILON), whole + 1, "{case}");
        }
    }
}

#[test]
fn hoppers_take_the_output_without_experience_and_players_with_it() {
    let mut w = world();
    w.set_block(AT, Block::FURNACE);
    let f = w.furnace_mut(AT).unwrap();
    f.set_input(Some(Stack::new(Item::RAW_IRON, 2)));
    f.fuel = Some(Stack::new(Item::COAL, 1));
    for _ in 0..2 {
        w.furnace_mut(AT).unwrap().tick(10.0);
    }
    let f = w.furnace_mut(AT).unwrap();
    assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 2)));
    // A hopper's `removeItem` on the output: the tally stays.
    f.output = Some(Stack::new(Item::IRON_INGOT, 1));
    assert!((f.stored_xp() - 1.4).abs() < 1e-6);
    // A player's take of the last ingot pays all of it.
    let before = f.output.map_or(0, |s| s.count);
    f.output = None;
    assert_eq!(f.take_output_xp(before, || 0.5), Some(1));
    assert_eq!(f.stored_xp(), 0.0);
}

#[test]
fn brewing_constants_fuels_inputs_and_reagents_match_java_both_ways() {
    let b = &fixture()["brewing"];
    assert_eq!(b["constants"]["BREWING_TIME_SECONDS"].as_u64().unwrap() * 20, brewing::BREW_TICKS as u64);
    assert_eq!(b["constants"]["speed_default"], 1.0);
    let fuels = b["fuels"].as_object().unwrap();
    assert_eq!(fuels.len(), 1);
    assert_eq!(fuels["blaze_powder"], brewing::FUEL_USES);
    let inputs = strings(&b["potion_inputs"]);
    let reagents = strings(&b["reagents"]);
    for id in &inputs {
        assert!(brewing::fits_bottle_slot(item(id)), "{id} fits a bottle slot");
    }
    let mut compared = 0;
    for (id, i) in all_items() {
        if !inputs.contains(&id) && i.as_potion().is_none() && i.as_splash_potion().is_none() {
            assert!(!brewing::fits_bottle_slot(i), "{id} isn't a potion input");
        }
        assert_eq!(brewing::is_fuel(i), fuels.contains_key(&id), "brewing fuel {id}");
        if i.as_potion().is_none() && i.as_splash_potion().is_none() {
            assert_eq!(brewing::is_ingredient(i), reagents.contains(&id), "BREWING_REAGENTS: {id}");
            compared += 1;
        }
    }
    assert!(compared > 1000);
}

fn bottle(v: &Value) -> Option<Stack> {
    let a = v.as_array()?;
    let potion = a[1].as_str().map(|p| Potion::from_id(p).unwrap());
    Some(match a[0].as_str().unwrap() {
        "potion" => Stack::new(Item::potion(potion.unwrap()), 1),
        "splash_potion" => Stack::new(Item::splash_potion(potion.unwrap()), 1),
        "lingering_potion" => Stack::lingering_potion(potion.unwrap()),
        "glass_bottle" => Stack::new(Item::GLASS_BOTTLE, 1),
        other => panic!("{other}"),
    })
}

#[test]
fn brewing_scenarios_match_java_tick_for_tick() {
    for case in fixture()["brewing"]["scenarios"].as_array().unwrap() {
        let name = case["case"].as_str().unwrap();
        let mut s = BrewingStand::default();
        let (mut brews, mut dropped) = (0, Vec::new());
        let mut states = case["states"].as_array().unwrap().iter();
        for op in case["ops"].as_array().unwrap() {
            let rest = Value::Array(op.as_array().unwrap()[1..].to_vec());
            match op[0].as_str().unwrap() {
                "bottles" => {
                    for (i, b) in op[1].as_array().unwrap().iter().enumerate() {
                        s.bottles[i] = bottle(b);
                    }
                }
                "ingredient" => s.ingredient = pair(&rest),
                "fuel" => s.fuel = pair(&rest),
                "tick" => {
                    for _ in 0..op[1].as_u64().unwrap() {
                        if s.step() {
                            brews += 1;
                            // `World::tick_brewing` drops a remainder while
                            // some ingredient is left.
                            if let Some(rest) = s.ingredient.and_then(|i| i.item.remainder()) {
                                dropped.push(rest);
                            }
                        }
                    }
                    let want = states.next().unwrap();
                    let at = format!("{name} after {op}");
                    assert_eq!(s.brew_ticks as u64, want["brew_time"].as_u64().unwrap(), "brew time: {at}");
                    assert_eq!(s.fuel_left as u64, want["fuel"].as_u64().unwrap(), "fuel: {at}");
                    for (i, b) in want["bottles"].as_array().unwrap().iter().enumerate() {
                        assert_eq!(s.bottles[i], bottle(b), "bottle {i}: {at}");
                    }
                    assert_eq!(s.ingredient, pair(&want["ingredient"]), "ingredient: {at}");
                    assert_eq!(s.fuel, pair(&want["fuel_slot"]), "fuel slot: {at}");
                    assert_eq!(brews, want["brews"].as_u64().unwrap(), "brews: {at}");
                    let want_dropped: Vec<Item> =
                        want["dropped"].as_array().unwrap().iter().map(|d| item(d.as_str().unwrap())).collect();
                    assert_eq!(dropped, want_dropped, "dropped: {at}");
                }
                other => panic!("unknown op {other}"),
            }
        }
    }
}

#[test]
fn comparators_read_furnaces_and_brewing_stands_like_java() {
    let c = &fixture()["comparator"];
    for case in c["furnace"].as_array().unwrap() {
        let mut w = world();
        w.set_block(AT, Block::FURNACE);
        let slots: Vec<_> = case["slots"].as_array().unwrap().iter().map(pair).collect();
        let f = w.furnace_mut(AT).unwrap();
        (f.input, f.fuel, f.output) = (slots[0], slots[1], slots[2]);
        assert_eq!(w.container_signal(AT), Some(case["signal"].as_u64().unwrap() as u8), "{case}");
    }
    for case in c["brewing"].as_array().unwrap() {
        let mut w = world();
        w.set_block(AT, Block::BREWING_STAND);
        let slots: Vec<_> = case["slots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let a = v.as_array()?;
                // A water bottle stands in for the fixture's `potion`.
                Some(match a[0].as_str().unwrap() {
                    "potion" => Stack::new(Item::potion(Potion::WATER), a[1].as_u64().unwrap() as u8),
                    _ => pair(v)?,
                })
            })
            .collect();
        let b = w.brewing_stand_mut(AT).unwrap();
        b.bottles = [slots[0], slots[1], slots[2]];
        (b.ingredient, b.fuel) = (slots[3], slots[4]);
        assert_eq!(w.container_signal(AT), Some(case["signal"].as_u64().unwrap() as u8), "{case}");
    }
}

#[test]
fn enchanting_tables_count_java_bookshelves_through_power_transmitters() {
    let e = &fixture()["enchanting"];
    assert_eq!(strings(&e["power_providers"]), BTreeSet::from(["bookshelf".to_string()]));
    // `#enchantment_power_transmitter` (`#replaceable`), both ways.
    let transmitters = strings(&e["power_transmitters"]);
    let mut wrong = Vec::new();
    for (id, english) in registry()["blocks"].as_object().unwrap() {
        let Some(b) = java_registry::block(id, english.as_str().unwrap()) else { continue };
        if enchant::transmits_power(b) != transmitters.contains(id) {
            wrong.push(id.clone());
        }
    }
    assert!(wrong.is_empty(), "power transmitters that differ from Java: {wrong:?}");
    let offsets = e["bookshelf_offsets"].as_array().unwrap();
    assert_eq!(offsets.len(), 32);
    let v = |o: &Value| {
        IVec3::new(o[0].as_i64().unwrap() as i32, o[1].as_i64().unwrap() as i32, o[2].as_i64().unwrap() as i32)
    };
    let mut w = world();
    w.set_block(AT, Block::ENCHANTING_TABLE);
    for (n, o) in offsets.iter().enumerate() {
        w.set_block(AT + v(&o["offset"]), Block::BOOKSHELF);
        assert_eq!(enchant::bookshelves(&w, AT), (n as u32 + 1).min(15), "{n} shelves");
    }
    // Something solid between the table and a shelf blocks it.
    let between = v(&offsets[0]["between"]);
    w.set_block(AT + between, Block::STONE);
    let blocked = offsets.iter().filter(|o| v(&o["between"]) == between).count();
    assert!(blocked >= 1);
    for o in offsets {
        w.set_block(AT + v(&o["offset"]), Block::AIR);
    }
    for o in offsets.iter().filter(|o| v(&o["between"]) != between).take(5) {
        w.set_block(AT + v(&o["offset"]), Block::BOOKSHELF);
    }
    assert_eq!(enchant::bookshelves(&w, AT), 5);
    w.set_block(AT + v(&offsets[0]["offset"]), Block::BOOKSHELF);
    assert_eq!(enchant::bookshelves(&w, AT), 5, "the blocked shelf doesn't count");
}

#[test]
fn enchanting_offers_match_java_for_seeds_shelves_and_items() {
    let e = &fixture()["enchanting"];
    let table: Vec<Enchantment> =
        e["in_enchanting_table"].as_array().unwrap().iter().map(|s| enchantment(s.as_str().unwrap())).collect();
    assert_eq!(table, enchant::NON_TREASURE.to_vec(), "#in_enchanting_table in Java's order");
    for (id, value) in e["items"].as_object().unwrap() {
        let Some(i) = registered(id) else { continue };
        assert_eq!(enchant::enchantability(i) as u64, value.as_u64().unwrap(), "{id} enchantability");
    }
    let entry = |v: &Value| (enchantment(v[0].as_str().unwrap()), v[1].as_u64().unwrap() as u8);
    let mut compared = 0;
    for case in e["offers"].as_array().unwrap() {
        let Some(i) = registered(case["item"].as_str().unwrap()) else { continue };
        let seed = case["seed"].as_i64().unwrap() as i32;
        let shelves = case["bookshelves"].as_u64().unwrap() as u32;
        let offers = enchant::offers(seed, i, shelves.min(15));
        for slot in 0..3 {
            let cost = case["costs"][slot].as_u64().unwrap() as u32;
            assert_eq!(offers[slot].cost, cost, "cost {slot}: {case}");
            let clue = (!case["clues"][slot].is_null()).then(|| entry(&case["clues"][slot]));
            assert_eq!(offers[slot].clue, clue, "clue {slot}: {case}");
            let list: Vec<_> = case["enchantments"][slot].as_array().unwrap().iter().map(entry).collect();
            if cost > 0 {
                assert_eq!(enchant::offer_enchants(seed, i, slot, cost), list, "roll {slot}: {case}");
            }
        }
        compared += 1;
    }
    assert!(compared >= 500, "{compared} offer cases compared");
}

fn check_stack(got: Stack, want: &Value, at: &str) {
    assert_eq!(got.item, item(want["item"].as_str().unwrap()), "item: {at}");
    assert_eq!(got.count as u64, want["count"].as_u64().unwrap(), "count: {at}");
    assert_eq!(got.damage as u64, want["damage"].as_u64().unwrap(), "damage: {at}");
    assert_eq!(got.enchants, enchantments(&want["enchantments"]), "enchantments: {at}");
    assert_eq!(got.repair_cost as u64, want["repair_cost"].as_u64().unwrap(), "repair cost: {at}");
    assert_eq!(got.name.as_str(), want["name"].as_str(), "name: {at}");
}

#[test]
fn anvil_items_and_constants_match_java() {
    let a = &fixture()["anvil"];
    assert_eq!(a["constants"]["MAX_NAME_LENGTH"], enchant::ANVIL_MAX_NAME_LENGTH);
    assert_eq!(a["constants"]["too_expensive"], enchant::TOO_EXPENSIVE);
    assert_eq!(a["constants"]["wear_chance"].as_f64().unwrap() as f32, enchant::ANVIL_WEAR_CHANCE);
    assert_eq!(
        a["constants"]["falling_damage_per_block"].as_f64().unwrap() as f32,
        enchant::FALLING_ANVIL_DAMAGE_PER_BLOCK
    );
    assert_eq!(a["constants"]["falling_damage_max"].as_f64().unwrap() as f32, enchant::FALLING_ANVIL_MAX_DAMAGE);
    for id in strings(&a["anvils"]) {
        let b = java_registry::block(&id, "").unwrap_or_else(|| panic!("{id}"));
        assert!(b.is_anvil(), "{id}");
    }
    let items = a["items"].as_object().unwrap();
    for (id, info) in items {
        let i = item(id);
        assert_eq!(i.durability().map(u64::from), info["max_damage"].as_u64(), "{id} durability");
        let repairs = strings(&info["repairs"]);
        for other in items.keys() {
            assert_eq!(enchant::repairs(i, item(other)), repairs.contains(other), "{other} repairs {id}");
        }
    }
    assert_eq!(enchant::valid_anvil_name(&"a".repeat(50)).map(|s| s.len()), Some(50));
    assert_eq!(enchant::valid_anvil_name(&"a".repeat(51)), None);
    assert_eq!(enchant::valid_anvil_name("A\u{a7}cB\n").as_deref(), Some("AcB"), "StringUtil.filterText");
}

#[test]
fn anvil_results_and_costs_match_java_case_by_case() {
    for case in fixture()["anvil"]["cases"].as_array().unwrap() {
        let at = case["case"].as_str().unwrap();
        let left = stack(&case["left"]).unwrap();
        let right = stack(&case["right"]);
        let creative = case["creative"].as_bool().unwrap();
        let got = enchant::anvil_menu(left, right, case["name"].as_str(), creative);
        let want = &case["result"];
        let Some(got) = got else {
            assert!(want.is_null(), "{at}: no result, Java {want}");
            continue;
        };
        assert!(!want.is_null(), "{at}: {got:?}, Java has no result");
        check_stack(got.output, &want["output"], at);
        assert_eq!(got.cost as u64, want["cost"].as_u64().unwrap(), "cost: {at}");
        assert_eq!(got.uses.map(u64::from), want["uses"].as_u64(), "uses: {at}");
        let too_expensive = !creative && got.cost >= enchant::TOO_EXPENSIVE;
        assert_eq!(too_expensive, want["too_expensive"].as_bool().unwrap(), "too expensive: {at}");
        if case["name"].is_null() {
            assert_eq!(enchant::anvil(left, right, creative).is_none(), too_expensive, "{at}");
        }
    }
}

#[test]
fn falling_anvils_wear_with_javas_chance() {
    for case in fixture()["anvil"]["falling"].as_array().unwrap() {
        let distance = case["distance"].as_f64().unwrap();
        let damage = case["damage"].as_f64().unwrap() as f32;
        let chance = case["wear_chance"].as_f64().unwrap() as f32;
        let (got, _) = enchant::falling_anvil_landing(Block::ANVIL, distance, 0.0);
        assert_eq!(got, damage, "{case}");
        if chance > 0.0 && chance < 1.0 {
            let below = enchant::falling_anvil_landing(Block::ANVIL, distance, chance - 1e-6).1;
            assert_eq!(below, Some(Block::ANVIL.anvil_damaged()), "{case}");
            assert_eq!(enchant::falling_anvil_landing(Block::ANVIL, distance, chance).1, None, "{case}");
        }
        if chance == 0.0 {
            assert_eq!(enchant::falling_anvil_landing(Block::ANVIL, distance, 0.0).1, None, "{case}");
        }
        if chance >= 1.0 {
            assert!(enchant::falling_anvil_landing(Block::ANVIL, distance, 0.999).1.is_some(), "{case}");
        }
    }
    let damaged = Block::ANVIL.anvil_damaged().unwrap().anvil_damaged().unwrap();
    assert_eq!(enchant::falling_anvil_landing(damaged, 10.0, 0.0).1, Some(None), "a damaged anvil breaks");
}

#[test]
fn grindstone_results_and_experience_match_java_case_by_case() {
    let g = &fixture()["grindstone"];
    let curses: HashSet<Enchantment> =
        g["curses"].as_array().unwrap().iter().map(|c| enchantment(c.as_str().unwrap())).collect();
    for e in Enchantment::ALL {
        assert_eq!(e.def().curse, curses.contains(&e), "#curse: {e:?}");
    }
    for case in g["cases"].as_array().unwrap() {
        let at = case["case"].as_str().unwrap();
        let (a, b) = (stack(&case["first"]), stack(&case["second"]));
        let got = grindstone::result(a, b);
        let want = &case["result"];
        let Some(got) = got else {
            assert!(want.is_null(), "{at}: no result, Java {want}");
            continue;
        };
        assert!(!want.is_null(), "{at}: {got:?}, Java has no result");
        check_stack(got.output, want, at);
        let xp = case["xp"].as_array().unwrap();
        assert_eq!(grindstone::xp(a, b, 0.0) as u64, xp[0].as_u64().unwrap(), "least xp: {at}");
        assert_eq!(grindstone::xp(a, b, 0.999_999) as u64, xp[1].as_u64().unwrap(), "most xp: {at}");
    }
}

#[test]
fn smithing_slots_take_exactly_javas_recipe_items() {
    let s = &fixture()["smithing"];
    let (templates, bases, additions) = (strings(&s["template"]), strings(&s["base"]), strings(&s["addition"]));
    let mut seen = BTreeMap::new();
    for (id, i) in all_items() {
        assert_eq!(smithing::is_template(i), templates.contains(&id), "template {id}");
        assert_eq!(smithing::is_base(i), bases.contains(&id), "base {id}");
        assert_eq!(smithing::is_addition(i), additions.contains(&id), "addition {id}");
        seen.insert(id, ());
    }
    for id in templates.iter().chain(&bases).chain(&additions) {
        assert!(seen.contains_key(id) || registered(id).is_none(), "{id}");
    }
}
