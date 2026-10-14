//! Java 26.3 fishing loot, driven by `fixtures/java/26.3/fishing.json`
//! (extracted by `tools/extract_java_fishing_fixtures.py` from the official
//! server artifact, not from VoxelCraft).
use serde_json::Value;
use voxelcraft::{
    content::fishing::{self, Category, Condition, Entry, Loot},
    enchant::Enchantment,
    entity::{CatchContext, Rng, roll_catch, set_damage},
    item::Item,
    potion::Potion,
    world::terrain::Biome,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/fishing.json")).unwrap()
}

fn reference_item(entry: &Value) -> Item {
    let name = entry["item"].as_str().unwrap().trim_start_matches("minecraft:");
    match (name, entry["potion"].as_str()) {
        ("potion", Some("minecraft:water")) => Item::potion(Potion::WATER),
        (_, None) => Item::from_name(name).unwrap_or_else(|| panic!("missing fixture item {name}")),
        other => panic!("unhandled fixture potion {other:?}"),
    }
}

fn condition(value: &Value) -> Condition {
    match value {
        Value::Null => Condition::Always,
        Value::String(s) if s == "in_open_water" => Condition::InOpenWater,
        Value::Object(o) => {
            let biomes: Vec<Biome> = o["biomes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| {
                    let name = b.as_str().unwrap().trim_start_matches("minecraft:");
                    Biome::ALL.into_iter().find(|x| x.name() == name).unwrap_or_else(|| panic!("biome {name}"))
                })
                .collect();
            let jungles: Vec<Biome> = Biome::ALL.into_iter().filter(|b| b.is_jungle()).collect();
            assert_eq!(biomes, jungles, "the only location check is the three jungles");
            Condition::JungleBiome
        }
        other => panic!("unhandled condition {other}"),
    }
}

fn check_table(name: &str, actual: &[Entry<Loot>], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{name} entry count");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        let context = format!("{name}[{i}] {}", e["item"]);
        assert_eq!(a.value.item, reference_item(e), "{context}");
        assert_eq!(a.weight, e["weight"].as_i64().unwrap() as i32, "{context}");
        assert_eq!(a.quality, e["quality"].as_i64().unwrap() as i32, "{context}");
        assert_eq!(a.condition, condition(&e["condition"]), "{context}");
        assert_eq!(i64::from(a.value.count), e["count"].as_i64().unwrap(), "{context}");
        let damage =
            e["damage"].as_object().map(|d| (d["min"].as_f64().unwrap() as f32, d["max"].as_f64().unwrap() as f32));
        assert_eq!(a.value.damage, damage, "{context}");
        assert_eq!(a.value.enchant_levels.map(i64::from), e["enchant_with_levels"].as_i64(), "{context}");
    }
}

#[test]
fn every_fishing_table_row_matches_the_reference() {
    let f = fixture();
    let categories = f["categories"].as_array().unwrap();
    assert_eq!(fishing::CATEGORIES.len(), categories.len());
    for (a, e) in fishing::CATEGORIES.iter().zip(categories) {
        let category = match e["table"].as_str().unwrap() {
            "junk" => Category::Junk,
            "treasure" => Category::Treasure,
            "fish" => Category::Fish,
            other => panic!("table {other}"),
        };
        assert_eq!(a.value, category);
        assert_eq!(a.weight, e["weight"].as_i64().unwrap() as i32);
        assert_eq!(a.quality, e["quality"].as_i64().unwrap() as i32);
        assert_eq!(a.condition, condition(&e["condition"]));
    }
    check_table("junk", &fishing::JUNK, &f["tables"]["junk"]);
    check_table("treasure", &fishing::TREASURE, &f["tables"]["treasure"]);
    check_table("fish", &fishing::FISH, &f["tables"]["fish"]);
}

#[test]
fn every_registered_enchantment_is_in_on_random_loot() {
    let names: Vec<Enchantment> = fixture()["on_random_loot"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| Enchantment::from_name(n.as_str().unwrap().trim_start_matches("minecraft:")))
        .collect();
    for e in Enchantment::ALL {
        assert!(names.contains(&e), "{e:?} is in #on_random_loot");
    }
}

fn is_in(table: &[Entry<Loot>], item: Item) -> bool {
    table.iter().any(|e| e.value.item == item)
}

/// Category frequencies follow `max(weight + quality * luck, 0)`, with
/// treasure only in open water. Expected shares: closed water 10/95 junk,
/// 85/95 fish; open water with Luck of the Sea III 4/97 junk, 11/97
/// treasure, 82/97 fish.
#[test]
fn category_shares_follow_reference_weights_luck_and_open_water() {
    const N: usize = 40_000;
    for (luck, open_water, junk, treasure) in [(0, false, 10.0 / 95.0, 0.0), (3, true, 4.0 / 97.0, 11.0 / 97.0)] {
        let mut rng = Rng::new(17 + luck as u64);
        let ctx = CatchContext { open_water, jungle: false };
        let (mut j, mut t) = (0usize, 0usize);
        for _ in 0..N {
            let stack = roll_catch(&mut rng, luck, ctx).unwrap();
            let book = stack.item == Item::ENCHANTED_BOOK;
            let enchanted = !stack.enchants.is_empty();
            if book || (is_in(&fishing::TREASURE, stack.item) && (enchanted || !is_in(&fishing::JUNK, stack.item))) {
                t += 1;
            } else if is_in(&fishing::JUNK, stack.item) {
                j += 1;
            } else {
                assert!(is_in(&fishing::FISH, stack.item), "{}", stack.item.name());
            }
        }
        let (j, t) = (j as f64 / N as f64, t as f64 / N as f64);
        assert!((j - junk).abs() < 0.01, "luck {luck}: junk share {j} vs {junk}");
        assert!((t - treasure).abs() < 0.01, "luck {luck}: treasure share {t} vs {treasure}");
    }
    // Java gives nothing when every eligible weight is zero.
    assert_eq!(roll_catch(&mut Rng::new(1), 85, CatchContext::default()), None);
    assert!(roll_catch(&mut Rng::new(1), 85, CatchContext { open_water: true, jungle: false }).is_some());
}

#[test]
fn junk_counts_damage_and_jungle_bamboo_follow_the_table() {
    let bamboo = fishing::JUNK.iter().find(|e| e.condition == Condition::JungleBiome).unwrap().value.item;
    let boots = fishing::JUNK[1].value.item;
    assert_eq!(boots.durability(), Some(65), "leather boots: 13 * 5");
    for jungle in [false, true] {
        let mut rng = Rng::new(3);
        let mut seen_bamboo = false;
        for _ in 0..30_000 {
            let stack = roll_catch(&mut rng, 0, CatchContext { open_water: false, jungle }).unwrap();
            seen_bamboo |= stack.item == bamboo;
            if stack.item == Item::INK_SAC {
                assert_eq!(stack.count, 10);
            } else {
                assert_eq!(stack.count, 1);
            }
            if stack.item == boots || stack.item == Item::FISHING_ROD {
                // set_damage uniform 0..0.9 of durability *remaining*.
                let max = stack.item.durability().unwrap();
                assert!((set_damage(0.9, max)..=max).contains(&stack.damage), "{} {}", stack.item.name(), stack.damage);
            }
        }
        assert_eq!(seen_bamboo, jungle);
    }
}

#[test]
fn treasure_gear_is_worn_and_enchanted_at_thirty_levels_including_curses() {
    let mut rng = Rng::new(11);
    let mut curses = 0;
    let mut treasure_only = 0;
    let mut books = 0;
    for _ in 0..60_000 {
        let stack = roll_catch(&mut rng, 3, CatchContext { open_water: true, jungle: false }).unwrap();
        assert_ne!(stack.item, Item::BOOK, "enchant_with_levels turns books into enchanted books");
        let gear = matches!(stack.item, Item::BOW | Item::FISHING_ROD) && !stack.enchants.is_empty();
        if gear {
            let max = stack.item.durability().unwrap();
            assert!((set_damage(0.25, max)..=max).contains(&stack.damage), "{} {}", stack.item.name(), stack.damage);
        }
        if stack.item == Item::ENCHANTED_BOOK {
            books += 1;
            assert!(!stack.enchants.is_empty());
        }
        if gear || stack.item == Item::ENCHANTED_BOOK {
            for (e, level) in stack.enchants.iter() {
                assert!(level >= 1 && level <= e.def().max_level);
                curses += usize::from(e.def().curse);
                treasure_only += usize::from(e.def().treasure);
            }
        }
    }
    assert!(books > 100, "books {books}");
    assert!(curses > 0, "#on_random_loot includes curses");
    assert!(treasure_only > curses, "mending/frost walker also roll");
}
