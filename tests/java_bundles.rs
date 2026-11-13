//! Java 26.3 bundles against `fixtures/java/26.3/bundles.json` and the
//! bundle recipes in `recipes.json`: the seventeen items, the
//! `BundleContents` weight rule and insert/remove order, the container
//! click overrides on the shared click path, the plain recipe and the
//! sixteen component-keeping dye transmutes (grid and CLI), the chest loot
//! that names a bundle, and bundles with nested contents through saves.

#[path = "java_recipes/compare.rs"]
mod compare;

use glam::{DVec3, IVec3};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    color::DyeColor,
    content::bundle::{self, Click, Fraction},
    crafting::Grid,
    enchant::Enchantment,
    entity::{Entities, bee::Occupant},
    inventory::{Inventory, Stack, click_slot, components::Payload, stack_from_str, stack_to_string},
    item::{Item, Tier, ToolKind},
    world::{
        World,
        bees::HiveItem,
        block::Block,
        chunk::ChunkData,
        loot::{LootRandom, roll_items},
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/bundles.json")).unwrap()
}

fn names() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

/// The Java item `id` as the game resolves it.
fn resolve(id: &str) -> Item {
    let english = names()["items"][id].as_str().unwrap_or("").to_string();
    voxelcraft::java_registry::item(id, &english).unwrap_or_else(|| panic!("{id} is not registered"))
}

fn fraction(text: &str) -> Fraction {
    let (n, d) = text.split_once('/').unwrap();
    Fraction::new(n.parse().unwrap(), d.parse().unwrap())
}

/// A fixture stack: item, count, an optional name, bundle contents and
/// whether it is a hive holding bees.
fn stack(v: &Value) -> Option<Stack> {
    if v.is_null() {
        return None;
    }
    let mut s = Stack::new(resolve(v["item"].as_str().unwrap()), v["count"].as_u64().unwrap() as u8);
    if let Some(name) = v["name"].as_str() {
        s = s.with_name(name).unwrap();
    }
    if let Some(items) = v["contents"].as_array() {
        s = s.with_payload(Payload::Bundle(items.iter().map(|i| stack(i).unwrap()).collect()));
    }
    if v["bees"].as_bool() == Some(true) {
        s.hive = Some(HiveItem { bees: [Some(Occupant::natural(0)), None, None], honey: 0 }.intern());
    }
    Some(s)
}

fn stacks(v: &Value) -> Vec<Stack> {
    v.as_array().unwrap().iter().map(|s| stack(s).unwrap()).collect()
}

#[test]
fn the_seventeen_bundles_match_the_java_tag_both_ways() {
    let f = fixture();
    let java = strings(&f["items"]);
    assert_eq!(java.len(), 17);
    assert_eq!(java[0], "bundle");
    assert_eq!(resolve("bundle"), bundle::BUNDLE);
    assert_eq!(bundle::colour(bundle::BUNDLE), Some(None));
    for ((id, colour), c) in java[1..].iter().zip(strings(&f["colours"])).zip(DyeColor::ALL) {
        assert_eq!(id, &format!("{colour}_bundle"));
        let item = resolve(id);
        assert_eq!(item, bundle::dyed(c), "{id}");
        assert_eq!(bundle::colour(item), Some(Some(c)), "{id}");
    }
    let tag: Vec<Item> = java.iter().map(|id| resolve(id)).collect();
    assert_eq!(bundle::all().collect::<Vec<_>>(), tag, "the tag's order is the id order");
    for (id, &item) in java.iter().zip(&tag) {
        assert_eq!(item.name(), id.replace('_', " "), "{id}");
        assert!(Item::creative_palette().any(|i| i == item), "{id} is missing from the creative palette");
        assert!(item.icon_layer().is_some(), "{id} has no icon");
        assert_eq!(u64::from(item.max_stack()), f["stack_size"].as_u64().unwrap(), "{id}");
    }
    // Nothing else the game registers is a bundle.
    let ours: BTreeSet<u16> = Item::all_items().filter(|&i| bundle::is_bundle(i)).map(|i| i.0).collect();
    assert_eq!(ours, tag.iter().map(|i| i.0).collect::<BTreeSet<_>>());
    // The reserved ids, plain first.
    assert_eq!(bundle::BUNDLE, Item(1568));
    assert_eq!(bundle::dyed(DyeColor::Black), Item(1584));
}

#[test]
fn weights_follow_bundle_contents() {
    let f = fixture();
    assert_eq!(bundle::BUNDLE_IN_BUNDLE_WEIGHT, fraction(f["weights"]["bundle_in_bundle"].as_str().unwrap()));
    assert_eq!(bundle::BEEHIVE_WEIGHT, fraction(f["weights"]["beehive"].as_str().unwrap()));
    // The case items stack as in Java, so `1 / max stack` is Java's weight.
    for (id, size) in f["max_stack"].as_object().unwrap() {
        assert_eq!(u64::from(resolve(id).max_stack()), size.as_u64().unwrap(), "{id}");
    }
    let one = |id: &str| Stack::new(resolve(id), 1);
    assert_eq!(bundle::weight(&one("stone")), Some(Fraction::new(1, 64)));
    assert_eq!(bundle::weight(&one("ender_pearl")), Some(Fraction::new(1, 16)));
    assert_eq!(bundle::weight(&one("saddle")), Some(Fraction::ONE));
    // An empty hive weighs as an ordinary block; one with bees weighs one.
    let hive = stack(&serde_json::json!({"item": "beehive", "count": 1, "bees": true})).unwrap();
    assert_eq!(bundle::weight(&one("beehive")), Some(Fraction::new(1, 64)));
    assert_eq!(bundle::weight(&hive), Some(Fraction::ONE));
    let honey_only = Stack { hive: Some(HiveItem { bees: [None, None, None], honey: 5 }.intern()), ..one("beehive") };
    assert_eq!(bundle::weight(&honey_only), Some(Fraction::new(1, 64)));
}

/// The fixture's insert and remove sequences, by the behaviour each shows.
const CAPACITY: &[&str] = &[
    "a full stack of a 64-stacking item fills a bundle",
    "partial stacks merge up to the weight limit",
    "sixteen of a 16-stacking item fill a bundle",
    "eight pearls and thirty-two stone fill a bundle exactly",
    "the remaining weight is floored",
    "an unstackable item weighs one",
    "an unstackable item needs an empty bundle",
];
const NESTED: &[&str] = &[
    "empty bundles are unstackable and weigh a sixteenth each",
    "a nested bundle weighs a sixteenth plus its contents",
    "bundles nest three deep",
];
const HIVES: &[&str] = &[
    "a hive holding bees weighs one",
    "a hive holding bees needs an empty bundle",
    "an empty hive weighs one sixty-fourth",
];
const ORDER: &[&str] = &["merging moves the stack to the front", "removing takes the most recent stack first"];

/// Replays the named sequences on the bundle module and compares every step
/// with the Java model: accepted counts, what is left of the offered stack,
/// the contents in order and their exact weight.
fn replay(names: &[&str]) {
    let f = fixture();
    for &wanted in names {
        let case = f["sequences"].as_array().unwrap().iter().find(|c| c["name"] == wanted).unwrap();
        let name = case["name"].as_str().unwrap();
        let mut contents = stacks(&case["start"]);
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            if step["insert"].is_object() {
                let offered = stack(&step["insert"]).unwrap();
                let mut left = Some(offered);
                let accepted = bundle::try_insert(&mut contents, &mut left);
                assert_eq!(u64::from(accepted), step["accepted"].as_u64().unwrap(), "{name} step {i}");
                let rest = offered.count - accepted;
                assert_eq!(left, (rest > 0).then_some(Stack { count: rest, ..offered }), "{name} step {i}");
            } else {
                assert_eq!(bundle::remove_one(&mut contents), stack(&step["removed"]), "{name} step {i}");
            }
            assert_eq!(contents, stacks(&step["contents"]), "{name} step {i}");
            let weight = fraction(step["weight"].as_str().unwrap());
            assert_eq!(bundle::contents_weight(&contents), Some(weight), "{name} step {i}");
        }
    }
}

#[test]
fn every_fixture_sequence_is_replayed() {
    let f = fixture();
    let all: BTreeSet<&str> = f["sequences"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    let claimed: BTreeSet<&str> = [CAPACITY, NESTED, HIVES, ORDER].concat().into_iter().collect();
    assert_eq!(all, claimed);
    assert_eq!(all.len(), 15);
}

#[test]
fn bundles_hold_at_most_one_unit_of_weight() {
    replay(CAPACITY);
}

#[test]
fn a_nested_bundle_weighs_a_sixteenth_plus_its_contents() {
    replay(NESTED);
}

#[test]
fn a_hive_holding_bees_weighs_one() {
    replay(HIVES);
}

#[test]
fn inserting_merges_to_the_front_and_removing_takes_the_front() {
    replay(ORDER);
}

#[test]
fn the_fullness_bar_follows_bundle_item() {
    let filled = |items: Vec<Stack>| Stack::new(bundle::BUNDLE, 1).with_payload(Payload::Bundle(items));
    // `isBarVisible`: only with something inside.
    assert_eq!(bundle::bar_width(&Stack::new(bundle::BUNDLE, 1)), None);
    assert_eq!(bundle::bar_width(&Stack::new(Block::DIRT, 1)), None);
    // `min(1 + mulAndTruncate(weight, 12), 13)`.
    for (count, width) in [(1, 1), (5, 1), (6, 2), (32, 7), (63, 12), (64, 13)] {
        let b = filled(vec![Stack::new(Block::DIRT, count)]);
        assert_eq!(bundle::bar_width(&b), Some(width), "{count} dirt");
        let full = count == 64;
        assert_eq!(bundle::bar_rgb(&b), if full { [1.0, 0.33, 0.33] } else { [0.44, 0.53, 1.0] }, "{count} dirt");
    }
    // A dyed bundle and a nested bundle count the same way: 1/16 + 16/64.
    let inner = filled(vec![Stack::new(Block::DIRT, 16)]);
    let outer = Stack::new(bundle::dyed(DyeColor::Red), 1).with_payload(Payload::Bundle(vec![inner]));
    assert_eq!(bundle::fullness(&outer), Fraction::new(5, 16));
    assert_eq!(bundle::bar_width(&outer), Some(4));
}

#[test]
fn container_clicks_match_the_java_overrides_on_the_shared_click_path() {
    let f = fixture();
    let clicks = f["clicks"].as_array().unwrap();
    assert_eq!(clicks.len(), 19);
    for case in clicks {
        let name = case["name"].as_str().unwrap();
        let right = case["button"] == "right";
        let (carried, slot) = (stack(&case["carried"]), stack(&case["slot"]));
        // The bundle rule on its own: whether it takes the click, and what
        // feedback it gives.
        let (mut c, mut s) = (carried, slot);
        let outcome = bundle::click(&mut s, &mut c, right);
        if case["overridden"] == true {
            let expected = match case["outcome"].as_str().unwrap() {
                "inserted" => Click::Inserted,
                "insert_failed" => Click::InsertFailed,
                "removed" => Click::Removed,
                "nothing" => Click::Nothing,
                other => panic!("{other}"),
            };
            assert_eq!(outcome, Some(expected), "{name}");
            assert_eq!(c, stack(&case["carried_after"]), "{name}: held");
            assert_eq!(s, stack(&case["slot_after"]), "{name}: slot");
        } else {
            assert_eq!(outcome, None, "{name}");
            assert_eq!((c, s), (carried, slot), "{name}: untouched");
        }
        // The click every container screen and adapter shares: the
        // inventory, a chest and the crafting grid all use `click_slot`.
        let (mut c2, mut s2) = (carried, slot);
        click_slot(&mut s2, &mut c2, right);
        if case["overridden"] == true {
            assert_eq!((c2, s2), (c, s), "{name}: click_slot");
        }
        let mut inventory = Inventory::default();
        inventory.cursor = carried;
        inventory.slots[9] = slot;
        inventory.click(9, right);
        assert_eq!((inventory.cursor, inventory.slots[9]), (c2, s2), "{name}: inventory click");
    }
    // The ordinary rules still apply where no bundle overrides: an empty
    // hand picks a bundle up, and a held stack swaps with one on right click.
    let filled = Stack::new(bundle::BUNDLE, 1).with_payload(Payload::Bundle(vec![Stack::new(Block::STONE, 2)]));
    let (mut c, mut s) = (None, Some(filled));
    click_slot(&mut s, &mut c, false);
    assert_eq!((c, s), (Some(filled), None));
    let (mut c, mut s) = (Some(Stack::new(Block::STONE, 3)), Some(filled));
    click_slot(&mut s, &mut c, true);
    assert_eq!((c, s), (Some(filled), Some(Stack::new(Block::STONE, 3))));
}

#[test]
fn filling_and_emptying_through_the_inventory_keeps_every_item() {
    // Survival play: craft-free fill and empty, one click at a time.
    let mut inv = Inventory::default();
    let sword = {
        let mut s = Stack::new(Item::tool(ToolKind::Sword, Tier::Iron), 1);
        s.damage = 40;
        s.enchants.set(Enchantment::Sharpness, 3);
        s
    };
    inv.slots[0] = Some(Stack::new(bundle::BUNDLE, 1));
    inv.slots[1] = Some(Stack::new(Block::DIRT, 20));
    inv.slots[2] = Some(Stack::new(Item::ENDER_PEARL, 16));
    inv.slots[3] = Some(sword);
    // Pick the bundle up and sweep it over the dirt and the pearls.
    inv.click(0, false);
    inv.click(1, false);
    inv.click(2, false);
    assert_eq!(inv.slots[1], None);
    // 20/64 of dirt leaves 44/64: eleven pearls fit.
    assert_eq!(inv.slots[2], Some(Stack::new(Item::ENDER_PEARL, 5)));
    let held = inv.cursor.unwrap();
    assert_eq!(held.bundle_contents(), [Stack::new(Item::ENDER_PEARL, 11), Stack::new(Block::DIRT, 20)]);
    assert_eq!(bundle::bar_width(&held), Some(13), "full: 1 + 12");
    assert_eq!(bundle::bar_rgb(&held), [1.0, 0.33, 0.33]);
    // A full bundle refuses the sword.
    inv.click(3, false);
    assert_eq!(inv.slots[3], Some(sword));
    // Put it down, then right-click it to take the front stack out.
    inv.click(0, false);
    inv.click(0, true);
    assert_eq!(inv.cursor, Some(Stack::new(Item::ENDER_PEARL, 11)));
    let left = inv.slots[0].unwrap();
    assert_eq!(left.bundle_contents(), [Stack::new(Block::DIRT, 20)]);
    // 20/64: 1 + floor(20 * 12 / 64) = 4 pixels, blue.
    assert_eq!(bundle::bar_width(&left), Some(4));
    assert_eq!(bundle::bar_rgb(&left), [0.44, 0.53, 1.0]);
    // Drop the pearls back on their stack, then empty the dirt.
    inv.click(2, false);
    inv.click(0, true);
    assert_eq!(inv.cursor, Some(Stack::new(Block::DIRT, 20)));
    assert_eq!(inv.slots[0], Some(Stack::new(bundle::BUNDLE, 1)), "an emptied bundle is a fresh one");
    assert_eq!(bundle::bar_width(&inv.slots[0].unwrap()), None);
    // A damaged, enchanted sword goes in and comes out unchanged.
    inv.cursor = None;
    inv.click(3, false);
    inv.click(0, false);
    assert_eq!(inv.cursor, None);
    inv.click(0, true);
    assert_eq!(inv.cursor, Some(sword));
}

#[test]
fn the_bundle_recipe_and_its_dye_transmutes_match_java() {
    let f = fixture();
    let plain = &f["recipes"]["bundle"];
    assert_eq!(plain["type"], "crafting_shaped");
    assert_eq!(strings(&plain["pattern"]), ["-", "#"]);
    assert_eq!(plain["key"]["-"], "string");
    assert_eq!(plain["key"]["#"], "leather");
    for (id, recipe) in f["recipes"].as_object().unwrap().iter().filter(|(id, _)| *id != "bundle") {
        assert_eq!(recipe["type"], "crafting_transmute", "{id}");
        assert_eq!(recipe["input"], "#minecraft:bundles", "{id}");
        assert!(recipe["material_count"].is_null() && recipe["add_material_count_to_result"] == false, "{id}");
        assert_eq!(resolve(recipe["result"].as_str().unwrap()), resolve(id), "{id}");
        assert_eq!(resolve(recipe["material"].as_str().unwrap()).dye_color(), bundle::colour(resolve(id)).unwrap());
    }
    // Both ways, through the shared comparison: all 17 bundle recipes are
    // compared and craft their grids, and the game has none Java lacks.
    let section = compare::compare_crafting(&compare::Reference::load());
    let unregistered: Vec<&String> = section.unregistered.keys().filter(|k| k.contains("bundle")).collect();
    assert!(unregistered.is_empty(), "bundle recipes still unregistered: {unregistered:?}");
    assert!(section.failures.is_empty(), "{}", section.failures.join("\n"));

    // String over leather, in any column of either grid; nothing else.
    for size in [2, 3] {
        for x in 0..size {
            let mut g = Grid::new(size);
            g.cells[x] = Some(Stack::new(Item::STRING, 1));
            g.cells[size + x] = Some(Stack::new(Item::LEATHER, 1));
            assert_eq!(g.result(), Some(Stack::new(bundle::BUNDLE, 1)), "size {size} column {x}");
            g.cells.swap(x, size + x);
            assert_eq!(g.result(), None, "leather over string");
        }
    }
}

#[test]
fn dyeing_any_bundle_keeps_its_contents_and_name() {
    let inner =
        Stack::new(bundle::dyed(DyeColor::Lime), 1).with_payload(Payload::Bundle(vec![Stack::new(Item::APPLE, 3)]));
    let contents = vec![Stack::new(Block::DIRT, 32), inner];
    for from in bundle::all() {
        let input = Stack::new(from, 1).with_payload(Payload::Bundle(contents.clone())).with_name("Kit").unwrap();
        for c in DyeColor::ALL {
            for (a, b) in [(0, 1), (3, 0)] {
                let mut g = Grid::new(2);
                g.cells[a] = Some(input);
                g.cells[b] = Some(Stack::new(c.dye(), 4));
                let expected = (from != bundle::dyed(c)).then_some(Stack { item: bundle::dyed(c), ..input });
                assert_eq!(g.result(), expected, "{} + {}", from.name(), c.dye().name());
                if let Some(out) = expected {
                    assert_eq!(out.bundle_contents(), contents);
                    assert_eq!(out.display_name(), "Kit");
                }
            }
        }
    }
    // One bundle and exactly one dye.
    let mut g = Grid::new(3);
    g.cells[0] = Some(Stack::new(bundle::BUNDLE, 1));
    g.cells[1] = Some(Stack::new(DyeColor::Red.dye(), 1));
    g.cells[2] = Some(Stack::new(DyeColor::Red.dye(), 1));
    assert_eq!(g.result(), None, "two dyes");
    g.cells[2] = Some(Stack::new(bundle::BUNDLE, 1));
    assert_eq!(g.result(), None, "two bundles");
    g.cells[2] = Some(Stack::new(Item::STRING, 1));
    assert_eq!(g.result(), None, "something else");
    // Consuming the grid takes one dye and the bundle.
    let mut g = Grid::new(2);
    g.cells[0] = Some(Stack::new(bundle::BUNDLE, 1));
    g.cells[3] = Some(Stack::new(DyeColor::Cyan.dye(), 4));
    assert_eq!(g.result(), Some(Stack::new(bundle::dyed(DyeColor::Cyan), 1)));
    g.consume();
    assert_eq!(g.cells[0], None);
    assert_eq!(g.cells[3], Some(Stack::new(DyeColor::Cyan.dye(), 3)));
}

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

#[test]
fn the_cli_crafts_and_dyes_a_filled_bundle_on_the_shared_path() {
    let mut w = world();
    let mut e = Entities::new(3);
    let mut a = Agent::new(DVec3::new(4.5, 141.0, 4.5));
    a.inventory.slots[0] = Some(Stack::new(Item::STRING, 1));
    a.inventory.slots[1] = Some(Stack::new(Item::LEATHER, 2));
    // String over leather fits the inventory's 2x2 grid; no table needed.
    a.execute(Command::parse("craft bundle").unwrap(), &mut w, &mut e, &[]).unwrap();
    let crafted: Vec<Stack> = a.inventory.slots.iter().flatten().copied().collect();
    assert!(crafted.contains(&Stack::new(bundle::BUNDLE, 1)), "{crafted:?}");
    assert!(crafted.contains(&Stack::new(Item::LEATHER, 1)), "{crafted:?}");
    // Fill it and dye it: the contents survive.
    let slot = a.inventory.find(bundle::BUNDLE).unwrap();
    let filled = Stack::new(bundle::BUNDLE, 1).with_payload(Payload::Bundle(vec![Stack::new(Block::DIRT, 9)]));
    a.inventory.slots[slot] = Some(filled);
    a.inventory.slots[8] = Some(Stack::new(DyeColor::Purple.dye(), 2));
    a.execute(Command::parse("craft purple_bundle").unwrap(), &mut w, &mut e, &[]).unwrap();
    let purple = a.inventory.find(bundle::dyed(DyeColor::Purple)).map(|i| a.inventory.get(i).unwrap());
    assert_eq!(purple, Some(Stack { item: bundle::dyed(DyeColor::Purple), ..filled }));
    assert_eq!(a.inventory.find(bundle::BUNDLE), None);
    // Its own colour again is no recipe.
    let before = a.inventory.clone();
    assert!(a.execute(Command::parse("craft purple_bundle").unwrap(), &mut w, &mut e, &[]).is_err());
    assert_eq!(a.inventory, before);
}

/// Java loot table paths and the game's table names.
fn game_table(java: &str) -> String {
    java.strip_prefix("chests/").unwrap_or(java).to_string()
}

/// Java tables that name a bundle but that the game doesn't roll yet:
/// villages (tannery, weaponsmith) and abandoned camps (v0.18) own them.
const TABLE_GAPS: &[&str] = &[
    "barrels/abandoned_camp_barrel",
    "chests/abandoned_camp_common_chest",
    "chests/village/village_tannery",
    "chests/village/village_weaponsmith",
];

#[test]
fn village_chests_hold_a_bundle_one_time_in_three() {
    let f = fixture();
    let loot = f["loot"].as_object().unwrap();
    assert_eq!(loot.len(), 10);
    let mut live = Vec::new();
    for (java, pools) in loot {
        let name = game_table(java);
        let rolls: usize = (0..64).map(|seed| roll_items(&name, &mut LootRandom::new(seed), Some).len()).sum();
        if TABLE_GAPS.contains(&java.as_str()) {
            assert_eq!(rolls, 0, "{java} is rolled now: take it off the gap list and check its bundle pool");
            continue;
        }
        assert!(rolls > 0, "{java} is not a table the game rolls");
        // Each live table has one pool for the bundle: weight 1 against an
        // empty entry of weight 2, one roll, one bundle.
        let [pool] = pools.as_array().unwrap().as_slice() else { panic!("{java}") };
        assert_eq!(pool["rolls"], 1, "{java}");
        let entries = pool["entries"].as_array().unwrap();
        let bundle_entry = entries.iter().find(|e| e["name"] == "bundle").unwrap();
        assert_eq!((bundle_entry["weight"].as_u64(), bundle_entry["count"].as_u64()), (Some(1), Some(1)), "{java}");
        let total: u64 = entries.iter().map(|e| e["weight"].as_u64().unwrap()).sum();
        assert_eq!(total, 3, "{java}");
        const N: u64 = 6000;
        let mut with_bundle = 0;
        for seed in 0..N {
            let items = roll_items(&name, &mut LootRandom::new(seed * 7919 + 13), Some);
            let bundles: Vec<&Stack> = items.iter().filter(|s| bundle::is_bundle(s.item)).collect();
            assert!(bundles.len() <= 1, "{java}: {bundles:?}");
            if let [b] = bundles.as_slice() {
                assert_eq!(**b, Stack::new(bundle::BUNDLE, 1), "{java}");
                with_bundle += 1;
            }
        }
        // Binomial(6000, 1/3): mean 2000, sd 36.5; allow five sd.
        assert!((with_bundle as i64 - 2000).abs() <= 183, "{java}: {with_bundle} of {N}");
        live.push(java.as_str());
    }
    assert_eq!(live.len(), 6, "{live:?}");
}

#[test]
fn bundles_with_nested_contents_save_and_load() {
    let mut sword = Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1);
    sword.damage = 100;
    sword.enchants.set(Enchantment::Sharpness, 2);
    let hive = Stack {
        hive: Some(HiveItem { bees: [Some(Occupant::natural(30)), None, None], honey: 2 }.intern()),
        ..Stack::new(resolve("bee_nest"), 1)
    };
    let inner = Stack::new(bundle::dyed(DyeColor::Pink), 1)
        .with_payload(Payload::Bundle(vec![Stack::new(Block::DIRT, 3).with_name("Soil").unwrap()]));
    let outer = Stack::new(bundle::dyed(DyeColor::Orange), 1)
        .with_payload(Payload::Bundle(vec![inner, sword, Stack::new(Item::ENDER_PEARL, 4)]))
        .with_name("Travel kit")
        .unwrap();
    let with_hive = Stack::new(bundle::BUNDLE, 1).with_payload(Payload::Bundle(vec![hive]));
    for s in [outer, with_hive, Stack::new(bundle::dyed(DyeColor::Black), 1)] {
        let text = stack_to_string(Some(s));
        assert_eq!(stack_from_str(&text), Some(Some(s)), "{text}");
    }
    // A plain bundle saves as `id:count`, as every older stack does.
    assert_eq!(stack_to_string(Some(Stack::new(bundle::BUNDLE, 1))), "1568:1");
    assert_eq!(stack_from_str("1584:1"), Some(Some(Stack::new(bundle::dyed(DyeColor::Black), 1))));
    // The inventory, a chest and a dropped item.
    let mut inv = Inventory::default();
    inv.slots[0] = Some(outer);
    inv.slots[5] = Some(with_hive);
    inv.offhand = Some(inner);
    let loaded = Inventory::deserialize(&inv.serialize()).unwrap();
    assert_eq!(loaded, inv);
    assert_eq!(loaded.slots[0].unwrap().bundle_contents()[0].bundle_contents()[0].display_name(), "Soil");
    let chest = IVec3::new(4, 145, 4);
    let mut w = world();
    w.set_block(chest, Block::CHEST);
    w.chest_mut(chest).unwrap().slots[3] = Some(outer);
    let mut reloaded = world();
    reloaded.load_chests(&w.chests_to_string());
    assert_eq!(reloaded.chest(chest).unwrap().slots[3], Some(outer));
    let mut e = Entities::new(5);
    e.scatter(outer, DVec3::new(0.5, 150.0, 0.5));
    let mut e2 = Entities::new(5);
    e2.load_items(&e.items_to_string());
    assert_eq!(e2.items[0].stack, outer);
}
