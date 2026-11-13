//! Enchantment definitions against Java 26.3's `data/minecraft/enchantment`
//! files and the tags they name (`enchantments.json`): every definition,
//! every registered item against every enchantment's supported and primary
//! items, every pair's compatibility, the enchantment tags the game uses and
//! the tooltip order. Both directions: what Java has, the game has (Soul
//! Speed is the one listed gap), and the game has nothing Java lacks.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use voxelcraft::{
    enchant::{self, Enchantment, Enchants, ExclusiveSet, ItemSet, NON_TREASURE, ON_RANDOM_LOOT, TOOLTIP_ORDER},
    inventory::Stack,
    item::Item,
    world::block::{Block, STATE_CAPACITY},
};

/// Java definitions without an [`Enchantment`] variant. The save encoding
/// holds 42 enchantments in 126 of its 128 bits, so Soul Speed waits for a
/// wider encoding (EFX-ENCH-18).
const UNREGISTERED: [&str; 1] = ["minecraft:soul_speed"];

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

/// Every item a player can hold, block items included.
fn all_items() -> Vec<Item> {
    let blocks = (0..STATE_CAPACITY as u16).map(Block).filter(|b| b.base() == *b && !b.name().is_empty());
    let mut items: Vec<Item> = blocks.filter_map(|b| Item::from_name(b.name())).chain(Item::all_items()).collect();
    items.sort_by_key(|i| i.0);
    items.dedup();
    items
}

/// The registered items and the Java ids each one stands for.
struct Items {
    all: Vec<Item>,
    java: BTreeMap<u16, BTreeSet<String>>,
    names: Value,
    gaps: Value,
}

impl Items {
    fn new() -> Self {
        let names = fixture("registry_names.json");
        let mut java: BTreeMap<u16, BTreeSet<String>> = BTreeMap::new();
        for (id, english) in names["items"].as_object().unwrap() {
            if let Some(item) = voxelcraft::java_registry::item(id, english.as_str().unwrap()) {
                java.entry(item.0).or_default().insert(format!("minecraft:{id}"));
            }
        }
        // Java's crossbow holds its charge as a component; VoxelCraft gives
        // the charged one its own id.
        java.entry(Item::CHARGED_CROSSBOW.0).or_default().insert("minecraft:crossbow".into());
        Self { all: all_items(), java, names, gaps: fixture("registry_gaps.json") }
    }

    /// Whether Java's expanded item tag holds `item`.
    fn in_tag(&self, members: &BTreeSet<String>, item: Item) -> bool {
        self.java.get(&item.0).is_some_and(|ids| ids.iter().any(|id| members.contains(id)))
    }

    fn registered(&self, id: &str) -> Option<Item> {
        let short = id.trim_start_matches("minecraft:");
        voxelcraft::java_registry::item(short, self.names["items"][short].as_str().unwrap_or(""))
    }
}

fn item_tag(f: &Value, tag: &str) -> BTreeSet<String> {
    strings(&f["item_tags"][tag]).into_iter().collect()
}

fn enchantment_tag(f: &Value, tag: &str) -> Vec<String> {
    strings(&f["enchantment_tags"][tag])
}

/// The registered members of an enchantment tag, in its order.
fn registered_tag(f: &Value, tag: &str) -> Vec<Enchantment> {
    let ids = enchantment_tag(f, tag);
    ids.iter()
        .filter(|id| !UNREGISTERED.contains(&id.as_str()))
        .map(|id| Enchantment::from_name(id.trim_start_matches("minecraft:")).unwrap_or_else(|| panic!("{tag}: {id}")))
        .collect()
}

fn definition(f: &Value, e: Enchantment) -> &Value {
    let mut found = f["enchantments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| Enchantment::from_name(d["id"].as_str().unwrap().trim_start_matches("minecraft:")) == Some(e));
    let d = found.next().unwrap_or_else(|| panic!("{e:?} is not a Java 26.3 enchantment"));
    assert!(found.next().is_none(), "{e:?} matches two Java ids");
    d
}

fn cost(v: &Value) -> (u32, u32) {
    (v["base"].as_u64().unwrap() as u32, v["per_level_above_first"].as_u64().unwrap() as u32)
}

#[test]
fn every_java_definition_is_registered_with_its_numbers_and_names() {
    let f = fixture("enchantments.json");
    let defs = f["enchantments"].as_array().unwrap();
    assert_eq!(defs.len(), 43);
    let mut missing = Vec::new();
    for d in defs {
        let id = d["id"].as_str().unwrap();
        let Some(e) = Enchantment::from_name(id.trim_start_matches("minecraft:")) else {
            missing.push(id);
            continue;
        };
        let def = e.def();
        assert_eq!(e.display_name(), d["name"].as_str().unwrap(), "{id}: en_us name");
        assert_eq!(def.max_level as u64, d["max_level"].as_u64().unwrap(), "{id}: max_level");
        assert_eq!(def.weight as u64, d["weight"].as_u64().unwrap(), "{id}: weight");
        assert_eq!(def.min_cost, cost(&d["min_cost"]), "{id}: min_cost");
        assert_eq!(def.max_cost, cost(&d["max_cost"]), "{id}: max_cost");
        assert_eq!(def.anvil_cost as u64, d["anvil_cost"].as_u64().unwrap(), "{id}: anvil_cost");
        assert_eq!(def.supported.tag(), d["supported_items"].as_str().unwrap(), "{id}: supported_items");
        assert_eq!(def.primary.map(ItemSet::tag), d["primary_items"].as_str(), "{id}: primary_items");
        assert_eq!(def.exclusive_set.map(ExclusiveSet::tag), d["exclusive_set"].as_str(), "{id}: exclusive_set");
        // Java's getMinCost/getMaxCost: base + per_level_above_first * (level - 1).
        for level in 1..=def.max_level {
            let (lo, hi) = (cost(&d["min_cost"]), cost(&d["max_cost"]));
            let extra = level as u32 - 1;
            assert_eq!(e.cost_range(level), (lo.0 + lo.1 * extra, hi.0 + hi.1 * extra), "{id} {level}");
        }
    }
    assert_eq!(missing, UNREGISTERED, "Java definitions without a variant");
    for e in Enchantment::ALL {
        definition(&f, e);
    }
}

/// Each `ItemSet` is its `#enchantable/*` tag, over every registered item,
/// both ways; Java items the game lacks are owned registry gaps.
#[test]
fn item_sets_match_the_enchantable_tags_for_every_registered_item() {
    let f = fixture("enchantments.json");
    let items = Items::new();
    let tags: BTreeSet<&str> = f["item_tags"].as_object().unwrap().keys().map(String::as_str).collect();
    let sets: BTreeSet<&str> = ItemSet::ALL.iter().map(|s| s.tag()).collect();
    assert_eq!(sets, tags, "one ItemSet per #enchantable tag");
    let mut failures = Vec::new();
    let mut unregistered = BTreeSet::new();
    for set in ItemSet::ALL {
        let members = item_tag(&f, set.tag());
        for id in &members {
            match items.registered(id) {
                Some(item) => {
                    if !set.contains(item) {
                        failures.push(format!("{}: {id} is missing", set.tag()));
                    }
                }
                None if items.gaps["items"].get(id.trim_start_matches("minecraft:")).is_some() => {
                    unregistered.insert(id.clone());
                }
                None => failures.push(format!("{}: {id} is neither registered nor a gap", set.tag())),
            }
        }
        for &item in &items.all {
            if set.contains(item) && !items.in_tag(&members, item) {
                failures.push(format!("{}: {} is not in Java's tag", set.tag(), item.name()));
            }
        }
    }
    println!("unregistered tag members (owned gaps): {unregistered:?}");
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}

/// Java's `isSupportedItem`, `isPrimaryItem` and `canEnchant` for every
/// registered item and every enchantment.
#[test]
fn every_item_takes_exactly_the_enchantments_java_allows() {
    let f = fixture("enchantments.json");
    let items = Items::new();
    let mut failures = Vec::new();
    let mut pairs = 0;
    for e in Enchantment::ALL {
        let d = definition(&f, e);
        let supported = item_tag(&f, d["supported_items"].as_str().unwrap());
        let primary = d["primary_items"].as_str().map(|t| item_tag(&f, t));
        for &item in &items.all {
            if item == Item::BOOK || item == Item::ENCHANTED_BOOK {
                continue;
            }
            pairs += 1;
            let want_supported = items.in_tag(&supported, item);
            let want_primary = want_supported && primary.as_ref().is_none_or(|p| items.in_tag(p, item));
            if e.is_supported_item(item) != want_supported || e.fits(item) != want_supported {
                failures.push(format!("{e:?} on {}: supported should be {want_supported}", item.name()));
            }
            if e.is_primary_item(item) != want_primary {
                failures.push(format!("{e:?} on {}: primary should be {want_primary}", item.name()));
            }
        }
        // Books take every enchantment: the table's `isBook`, the anvil's
        // enchanted book and `enchant_randomly`'s book case.
        assert!(e.fits(Item::BOOK) && e.fits(Item::ENCHANTED_BOOK));
    }
    println!("compared {pairs} item x enchantment pairs");
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}

/// Java's `areCompatible` over every ordered pair, from each definition's
/// exclusive set; and each `ExclusiveSet` is its tag, in order.
#[test]
fn exclusive_sets_and_compatibility_match_for_every_pair() {
    let f = fixture("enchantments.json");
    let mut tags: Vec<String> = f["enchantment_tags"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|t| t.starts_with("exclusive_set/"))
        .cloned()
        .collect();
    tags.sort();
    let mut sets: Vec<String> = ExclusiveSet::ALL.iter().map(|s| s.tag().to_string()).collect();
    sets.sort();
    assert_eq!(sets, tags);
    for set in ExclusiveSet::ALL {
        assert_eq!(set.members(), registered_tag(&f, set.tag()).as_slice(), "{}", set.tag());
    }
    let set_of = |e: Enchantment| -> Vec<Enchantment> {
        definition(&f, e)["exclusive_set"].as_str().map(|t| registered_tag(&f, t)).unwrap_or_default()
    };
    for a in Enchantment::ALL {
        for b in Enchantment::ALL {
            let want = a != b && !set_of(a).contains(&b) && !set_of(b).contains(&a);
            assert_eq!(a.compatible(b), want, "{a:?} with {b:?}");
        }
    }
}

/// The enchantment tags, both ways: every Java tag is checked here, and each
/// one the game uses holds exactly the registered enchantments it should,
/// in Java's order where the game walks it.
#[test]
fn enchantment_tags_match_and_keep_java_order() {
    let f = fixture("enchantments.json");
    let known = [
        "curse",
        "double_trade_price",
        "exclusive_set/armor",
        "exclusive_set/boots",
        "exclusive_set/bow",
        "exclusive_set/crossbow",
        "exclusive_set/damage",
        "exclusive_set/mining",
        "exclusive_set/riptide",
        "in_enchanting_table",
        "non_treasure",
        "on_mob_spawn_equipment",
        "on_random_loot",
        "on_traded_equipment",
        "prevents_bee_spawns_when_mining",
        "prevents_decorated_pot_shattering",
        "prevents_ice_melting",
        "prevents_infested_spawns",
        "smelts_loot",
        "tooltip_order",
        "tradeable",
        "treasure",
    ];
    let tags: Vec<&str> = f["enchantment_tags"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(tags, known, "a new or removed enchantment tag needs a decision here");

    let set = |list: Vec<Enchantment>| list.into_iter().map(|e| e as u8).collect::<BTreeSet<_>>();
    let flagged = |p: fn(Enchantment) -> bool| {
        Enchantment::ALL.into_iter().filter(|&e| p(e)).map(|e| e as u8).collect::<BTreeSet<_>>()
    };
    assert_eq!(set(registered_tag(&f, "treasure")), flagged(|e| e.def().treasure));
    assert_eq!(set(registered_tag(&f, "curse")), flagged(|e| e.def().curse));
    // Villagers double a book's price on the treasure flag.
    assert_eq!(set(registered_tag(&f, "double_trade_price")), flagged(|e| e.def().treasure));
    // The table, mob spawn equipment and traded equipment all roll from
    // `NON_TREASURE`, in its order.
    assert_eq!(registered_tag(&f, "non_treasure"), NON_TREASURE);
    for tag in ["in_enchanting_table", "on_mob_spawn_equipment", "on_traded_equipment"] {
        assert_eq!(registered_tag(&f, tag), NON_TREASURE, "{tag}");
    }
    assert_eq!(set(NON_TREASURE.to_vec()), flagged(|e| !e.def().treasure));
    assert_eq!(registered_tag(&f, "on_random_loot"), ON_RANDOM_LOOT);
    assert_eq!(set(ON_RANDOM_LOOT.to_vec()), flagged(Enchantment::on_random_loot));
    assert_eq!(registered_tag(&f, "tradeable"), Enchantment::tradeable());
    assert_eq!(registered_tag(&f, "tooltip_order"), TOOLTIP_ORDER);
    // Their consumers name the single member directly (mining, bees,
    // pots, ice, infested blocks and fire aspect's smelted drops).
    for tag in [
        "prevents_bee_spawns_when_mining",
        "prevents_decorated_pot_shattering",
        "prevents_ice_melting",
        "prevents_infested_spawns",
    ] {
        assert_eq!(registered_tag(&f, tag), [Enchantment::SilkTouch], "{tag}");
    }
    assert_eq!(registered_tag(&f, "smelts_loot"), [Enchantment::FireAspect]);
    // Soul Speed is in #treasure and #tooltip_order only.
    for (tag, ids) in f["enchantment_tags"].as_object().unwrap() {
        let has = strings(ids).contains(&UNREGISTERED[0].to_string());
        assert_eq!(has, tag == "treasure" || tag == "tooltip_order", "{tag}");
    }
}

/// `ItemEnchantments.addToTooltip`: lines follow `#tooltip_order`, curses
/// first, whatever order the save bits are in.
#[test]
fn tooltips_list_enchantments_in_tooltip_order() {
    let f = fixture("enchantments.json");
    let all = Enchantment::ALL.into_iter().fold(Enchants::NONE, |a, e| a.with(e, 1));
    let want: Vec<String> = registered_tag(&f, "tooltip_order").into_iter().map(|e| e.describe(1)).collect();
    let got: Vec<String> = all.lines().into_iter().map(|(line, _)| line).collect();
    assert_eq!(got, want);
    let sword = Enchants::NONE
        .with(Enchantment::Unbreaking, 3)
        .with(Enchantment::Mending, 1)
        .with(Enchantment::Sharpness, 5)
        .with(Enchantment::VanishingCurse, 1)
        .with(Enchantment::Looting, 2);
    assert_eq!(
        sword.lines(),
        [
            ("Curse of Vanishing".to_string(), true),
            ("Sharpness V".to_string(), false),
            ("Looting II".to_string(), false),
            ("Unbreaking III".to_string(), false),
            ("Mending".to_string(), false),
        ]
    );
    assert_eq!(Enchants::NONE.with(Enchantment::LuckOfTheSea, 3).lines()[0].0, "Luck of the Sea III");
}

fn book(e: Enchantment, level: u8) -> Stack {
    Stack { enchants: Enchants::NONE.with(e, level), ..Stack::new(Item::ENCHANTED_BOOK, 1) }
}

/// The tag fixes reach the anvil: Efficiency on shears (`#enchantable/mining`),
/// Curse of Binding on a carved pumpkin (`#enchantable/equippable`) and Curse
/// of Vanishing on a compass and a carved pumpkin (`#enchantable/vanishing`).
/// None of them is enchantable at the table, and Silk Touch and Fortune stay
/// off shears (`#enchantable/mining_loot`).
#[test]
fn anvil_books_reach_shears_pumpkins_and_compasses() {
    let pumpkin = Item::from_block(voxelcraft::world::pumpkin_blocks::CARVED);
    for (item, e, level) in [
        (Item::SHEARS, Enchantment::Efficiency, 5),
        (pumpkin, Enchantment::BindingCurse, 1),
        (pumpkin, Enchantment::VanishingCurse, 1),
        (Item::COMPASS, Enchantment::VanishingCurse, 1),
    ] {
        let result = enchant::anvil(Stack::new(item, 1), Some(book(e, level)), false)
            .unwrap_or_else(|| panic!("{e:?} on {}", item.name()));
        assert_eq!(result.output.item, item);
        assert_eq!(result.output.enchants.level(e), level);
        // An enchanted book halves the anvil cost per level (minimum 1).
        assert_eq!(result.cost, (e.def().anvil_cost / 2).max(1) * level as u32);
        assert!(!enchant::table_accepts(Stack::new(item, 1)), "{} at the table", item.name());
        assert!(enchant::command(Some(Stack::new(item, 1)), e, level).is_ok(), "/enchant {e:?}");
    }
    for e in [Enchantment::SilkTouch, Enchantment::Fortune] {
        assert_eq!(enchant::anvil(Stack::new(Item::SHEARS, 1), Some(book(e, 1)), false), None, "{e:?}");
    }
    assert_eq!(enchant::anvil(Stack::new(Item::COMPASS, 1), Some(book(Enchantment::BindingCurse, 1)), false), None);
}
