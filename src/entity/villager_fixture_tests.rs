//! v0.15 Track H: the villager tables and constants against Java 26.3's
//! data, extracted by `tools/extract_java_villager_fixtures.py` into
//! `tests/fixtures/java/26.3/villagers.json`. Each comparison runs both
//! ways: every Java entry must be in the game, and every game entry in Java.
use super::trades::{self, Modifier, TradeDef};
use super::*;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/villagers.json")).unwrap()
}

fn java_profession(p: Profession) -> String {
    p.name().to_lowercase()
}

/// The fixture's view of one table entry, built from the game's.
fn as_java(d: &TradeDef) -> Value {
    let cost = |(item, count): (&str, u8)| json!({"item": item, "count": count});
    json!({
        "wants": cost(d.wants),
        "additional_wants": d.also.map(cost),
        "gives": cost(d.gives),
        "max_uses": d.max_uses,
        "xp": d.xp,
        "reputation_discount": d.mult,
        "villager_types": (!d.types.is_empty()).then_some(d.types),
    })
}

/// The modifier the fixture's `given_item_modifier` list describes.
fn java_modifier(t: &Value) -> Modifier {
    let mut out = Modifier::None;
    for m in t["modifiers"].as_array().unwrap() {
        let found = match m["type"].as_str().unwrap() {
            "filtered" => continue,
            "enchant_with_levels" => {
                assert_eq!(m["options"], "#on_traded_equipment");
                assert_eq!(m["include_additional_cost_component"], true);
                assert_eq!(m["levels"]["type"], "uniform");
                Modifier::Enchant {
                    min: m["levels"]["min"].as_u64().unwrap() as u8,
                    max: m["levels"]["max"].as_u64().unwrap() as u8,
                }
            }
            "enchant_randomly" => {
                assert_eq!(m["options"], "#tradeable");
                assert_eq!(m["include_additional_cost_component"], true);
                assert_eq!(t["double_trade_price_enchantments"], "#double_trade_price");
                Modifier::Book
            }
            "exploration_map" => {
                let dest = m["destination"].as_str().unwrap().trim_start_matches('#');
                Modifier::Map(
                    trades::TRADES
                        .iter()
                        .find_map(|(_, _, d)| match d.modifier {
                            Modifier::Map(x) if x == dest => Some(x),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("no map trade to {dest}")),
                )
            }
            "set_random_dyes" => {
                // `1 + binomial(2, 0.75)`.
                assert_eq!(
                    m["number_of_dyes"],
                    json!({"type": "add", "inputs": [1, {"type": "binomial", "n": 2, "p": 0.75}]})
                );
                Modifier::Dyes
            }
            "set_potion" => {
                let id = m["id"].as_str().unwrap();
                Modifier::Potion(
                    trades::TRADES
                        .iter()
                        .map(|(_, _, d)| d)
                        .chain(trades::WANDERING.iter().flat_map(|(_, _, s)| s.iter()))
                        .find_map(|d| match d.modifier {
                            Modifier::Potion(p) if p == id => Some(p),
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("no potion trade {id}")),
                )
            }
            "set_random_potion" => {
                assert_eq!(m["options"], "#tradeable");
                Modifier::RandomPotion
            }
            "set_stew_effect" => Modifier::Stew,
            other => panic!("unknown modifier {other}"),
        };
        assert_eq!(out, Modifier::None, "one modifier per trade");
        out = found;
    }
    out
}

fn check_entry(d: &TradeDef, t: &Value) {
    let mut java = t.clone();
    let java = java.as_object_mut().unwrap();
    // Components on a cost (the wandering trader's water bottle) are the
    // item's own: `potion` resolves to a water bottle.
    if let Some(components) = java["wants"].as_object_mut().unwrap().remove("components") {
        assert_eq!(components, json!({"potion_contents": {"potion": "water"}}), "{}", d.id);
        assert_eq!(trades::item(d.wants.0), Some(Item::potion(crate::potion::Potion::WATER)));
    }
    let game = as_java(d);
    for key in ["wants", "additional_wants", "gives", "max_uses", "xp", "villager_types"] {
        assert_eq!(game[key], java[key], "{} {key}", d.id);
    }
    assert_eq!(d.mult, java["reputation_discount"].as_f64().unwrap() as f32, "{}", d.id);
    assert_eq!(d.modifier, java_modifier(t), "{}", d.id);
}

#[test]
fn profession_trade_tables_match_java_both_ways() {
    let f = fixture();
    let sets = f["trade_sets"].as_object().unwrap();
    let professions: Vec<_> =
        Profession::ALL.into_iter().filter(|p| !matches!(p, Profession::None | Profession::Nitwit)).collect();
    assert_eq!(professions.len(), sets.len());
    let mut checked = 0;
    for p in professions {
        let levels = &sets[&java_profession(p)];
        for level in 1..=5u8 {
            let set = &levels[level.to_string()];
            let java: Vec<&str> = set["trades"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
            let game: Vec<&TradeDef> =
                trades::TRADES.iter().filter(|(q, l, _)| *q == p && *l == level).map(|(_, _, d)| d).collect();
            assert_eq!(game.iter().map(|d| d.id).collect::<Vec<_>>(), java, "{p:?} level {level}");
            assert_eq!(trades::amount(p, level) as u64, set["amount"].as_u64().unwrap(), "{p:?} {level}");
            assert_eq!(set["allow_duplicates"], false);
            for d in game {
                check_entry(d, &f["trades"][d.id]);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, trades::TRADES.len(), "every game entry belongs to a Java level");
}

#[test]
fn wandering_trader_sets_match_java_both_ways() {
    let f = fixture();
    let w = &f["wandering_trader"];
    let order: Vec<&str> = w["order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(trades::WANDERING.map(|(name, _, _)| name).to_vec(), order);
    for (name, amount, set) in trades::WANDERING {
        let java = &w["sets"][name];
        assert_eq!(amount as u64, java["amount"].as_u64().unwrap(), "{name}");
        let ids: Vec<&str> = java["trades"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(set.iter().map(|d| d.id).collect::<Vec<_>>(), ids, "{name}");
        for d in set {
            check_entry(d, &f["trades"][d.id]);
        }
    }
}

/// An offer is only dropped for an item the game really lacks: every name
/// Java's registry resolves (with its English name) resolves at runtime.
#[test]
fn trade_items_resolve_like_the_registry() {
    let names: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/registry_names.json")).unwrap();
    let all = trades::TRADES.iter().map(|(_, _, d)| d).chain(trades::WANDERING.iter().flat_map(|(_, _, s)| s.iter()));
    let mut missing = std::collections::BTreeSet::new();
    let mut differ = std::collections::BTreeSet::new();
    for d in all {
        for name in [Some(d.wants.0), d.also.map(|a| a.0), Some(d.gives.0)].into_iter().flatten() {
            let english = names["items"][name].as_str().unwrap_or("");
            let registry = crate::java_registry::item(name, english);
            if trades::item(name) != registry {
                differ.insert(format!("{name} ({english}): {registry:?}"));
            }
            if trades::item(name).is_none() {
                missing.insert(name);
            }
        }
    }
    assert!(differ.is_empty(), "resolve differently at runtime: {differ:#?}");
    println!("trade items the game lacks: {missing:?}");
}

#[test]
fn enchantment_potion_and_stew_tags_match_java() {
    let f = fixture();
    let names = |v: &Value| -> std::collections::BTreeSet<String> {
        v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
    };
    let id = |e: crate::enchant::Enchantment| {
        e.def()
            .name
            .replace("curse of binding", "binding curse")
            .replace("curse of vanishing", "vanishing curse")
            .replace(' ', "_")
    };
    let tradeable: std::collections::BTreeSet<String> =
        crate::enchant::Enchantment::tradeable().iter().map(|&e| id(e)).collect();
    assert_eq!(tradeable, names(&f["enchantment_tags"]["tradeable"]));
    let double: std::collections::BTreeSet<String> =
        crate::enchant::Enchantment::ALL.into_iter().filter(|&e| trades::double_trade_price(e)).map(id).collect();
    // Soul speed isn't in the game yet; the rest of the tag must match.
    let java_double: std::collections::BTreeSet<String> = names(&f["enchantment_tags"]["double_trade_price"])
        .into_iter()
        .filter(|n| crate::enchant::Enchantment::from_name(n).is_some())
        .collect();
    assert_eq!(double, java_double);
    let potions: Vec<&str> =
        f["potion_tags"]["tradeable"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(trades::TRADEABLE_POTIONS.to_vec(), potions);
    for p in potions {
        assert!(crate::potion::Potion::from_id(p).is_some(), "{p}");
    }
    // The farmer's stew: seconds times 20, except the instant saturation.
    let stew = f["trades"]["farmer/4/emerald_suspicious_stew"]["modifiers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["type"] == "set_stew_effect")
        .unwrap()["effects"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(stew.len(), trades::STEW_EFFECTS.len());
    for (java, (effect, ticks, code)) in stew.iter().zip(trades::STEW_EFFECTS) {
        assert_eq!(java["type"], effect);
        let seconds = java["duration"].as_u64().unwrap() as u32;
        assert_eq!(ticks, if effect == "saturation" { seconds } else { seconds * 20 });
        let (e, t) = super::super::mooshroom::effect(code).unwrap();
        assert_eq!((e.id(), t), (effect, ticks), "stew code {code}");
    }
}

#[test]
fn offers_follow_get_offer_rules() {
    // Biome-only trades: a plains villager never offers the taiga boat.
    let boat = trades::TRADES.iter().map(|(_, _, d)| d).find(|d| d.types == ["taiga", "snow"]).unwrap();
    assert!(boat.offer(1, "plains").is_none());
    assert!(boat.offer(1, "taiga").is_some() || !boat.available());
    // Librarian books: price is 2 + nextInt(5 + 10L) + 3L, doubled for
    // treasure, plus the base 0, clamped to a stack.
    let book = trades::TRADES
        .iter()
        .map(|(_, _, d)| d)
        .find(|d| d.id == "librarian/1/emerald_and_book_enchanted_book")
        .unwrap();
    for seed in 0..200 {
        let o = book.offer(seed, "plains").unwrap();
        let (e, level) = o.output.enchants.iter().next().unwrap();
        let level = level as u32;
        let lo = 2 + 3 * level;
        let hi = lo + 4 + 10 * level;
        let (lo, hi) = if trades::double_trade_price(e) { (2 * lo, 2 * hi) } else { (lo, hi) };
        assert!((lo.min(64)..=hi.min(64)).contains(&(o.cost.count as u32)), "{e:?} {level} {}", o.cost.count);
        assert_eq!(o.second, Some(Stack::new(Item::BOOK, 1)));
        assert_eq!((o.max_uses, o.multiplier), (12, 0.2));
    }
    // Dyed leather has one to three dyes mixed in.
    let leather = trades::TRADES.iter().map(|(_, _, d)| d).find(|d| d.modifier == Modifier::Dyes).unwrap();
    for seed in 0..50 {
        let o = leather.offer(seed, "plains").unwrap();
        assert!(crate::inventory::components::dyed_color(&o.output).is_some());
    }
    // The fletcher's tipped arrows carry a tradeable potion.
    let arrows = trades::TRADES.iter().map(|(_, _, d)| d).find(|d| d.modifier == Modifier::RandomPotion).unwrap();
    for seed in 0..50 {
        let o = arrows.offer(seed, "plains").unwrap();
        assert_eq!((o.output.item, o.output.count), (Item::TIPPED_ARROW, 5));
        assert!(o.output.entity_data > 0);
        assert_eq!(o.second, Some(Stack::new(Item::ARROW, 5)));
    }
}

#[test]
fn schedules_match_the_villager_timeline() {
    let f = fixture();
    assert_eq!(f["schedule"]["period_ticks"], 24000);
    let name = |a: Activity| format!("{a:?}").to_lowercase();
    for (track, game) in [("villager_activity", &ADULT_SCHEDULE), ("baby_villager_activity", &BABY_SCHEDULE)] {
        let java: Vec<(u64, String)> = f["schedule"]["tracks"][track]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| (k["tick"].as_u64().unwrap(), k["activity"].as_str().unwrap().to_string()))
            .collect();
        let game: Vec<(u64, String)> = game.iter().map(|&(t, a)| (t as u64, name(a))).collect();
        assert_eq!(game, java, "{track}");
    }
    assert_eq!(schedule_activity(5, false), Activity::Rest, "the night's rest runs past midnight");
    assert_eq!(schedule_activity(1999, false), Activity::Idle);
    assert_eq!(schedule_activity(2000, false), Activity::Work);
    assert_eq!(schedule_activity(9500, false), Activity::Meet);
    assert_eq!(schedule_activity(11500, false), Activity::Idle);
    assert_eq!(schedule_activity(3000, true), Activity::Play);
    assert_eq!(schedule_activity(7000, true), Activity::Idle);
}

#[test]
fn villager_constants_match_java() {
    let f = fixture();
    let c = &f["constants"];
    let int = |k: &str| c[k].as_i64().unwrap_or_else(|| panic!("{k}"));
    let levels: Vec<u64> = f["level_xp_thresholds"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(LEVEL_XP.map(u64::from).to_vec(), levels);
    // Gossip types, in the game's saved order.
    let gossip = f["gossip_types"].as_array().unwrap();
    assert_eq!(gossip.len(), super::super::villager_gossip::TYPES.len());
    for (id, weight, max, per_day, per_transfer) in super::super::villager_gossip::TYPES {
        let java = gossip.iter().find(|g| g["id"] == id).unwrap();
        assert_eq!(java["weight"].as_i64().unwrap(), weight as i64, "{id}");
        assert_eq!(java["max"].as_u64().unwrap(), max as u64, "{id}");
        assert_eq!(java["decay_per_day"].as_u64().unwrap(), per_day as u64, "{id}");
        assert_eq!(java["decay_per_transfer"].as_u64().unwrap(), per_transfer as u64, "{id}");
    }
    use super::super::villager_gossip as g;
    assert_eq!(int("Villager.GOSSIP_DECAY_INTERVAL"), g::DECAY_INTERVAL);
    assert_eq!(int("Villager.GOSSIP_COOLDOWN"), g::GOSSIP_COOLDOWN);
    assert_eq!(int("Villager.MAX_GOSSIP_TOPICS"), g::MAX_TOPICS as i64);
    assert_eq!(int("GossipContainer.DISCARD_THRESHOLD"), 2);
    assert_eq!(int("GossipType.REPUTATION_CHANGE_PER_TRADE"), 2);
    assert_eq!(int("Villager.BREEDING_FOOD_THRESHOLD"), 12);
    assert_eq!(int("Villager.TIME_SINCE_SLEEPING_FOR_GOLEM_SPAWNING"), super::super::golem::GOLEM_SLEEP_TICKS);
    assert_eq!(int("Villager.HOW_MANY_VILLAGERS_NEED_TO_AGREE_TO_SPAWN_A_GOLEM"), 5);
    assert_eq!(int("Villager.HOW_FAR_AWAY_TO_TALK_TO_OTHER_VILLAGERS_ABOUT_GOLEMS"), 10);
    assert_eq!(int("GolemSensor.MEMORY_TIME_TO_LIVE") + 1, 600, "the 30 s golem memory");
    assert_eq!(int("AcquirePoi.SCAN_RANGE"), 48);
    assert_eq!(int("WorkAtPoi.CHECK_COOLDOWN"), WORK_CHECK_COOLDOWN);
    assert_eq!(c["WorkAtPoi.DISTANCE"].as_f64().unwrap(), WORK_DISTANCE);
    assert_eq!(f["poi_types"]["meeting"]["max_tickets"].as_u64().unwrap(), MEETING_TICKETS as u64);
    for job in ["armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "home"] {
        assert_eq!(f["poi_types"][job]["max_tickets"], 1, "{job}");
    }
    use super::super::zombie_villager as z;
    assert_eq!(int("ZombieVillager.VILLAGER_CONVERSION_WAIT_MIN"), z::CONVERSION_MIN as i64);
    assert_eq!(int("ZombieVillager.VILLAGER_CONVERSION_WAIT_MAX"), z::CONVERSION_MAX as i64);
    assert_eq!(int("ZombieVillager.MAX_SPECIAL_BLOCKS_COUNT"), z::SPECIAL_BLOCKS as i64);
    assert_eq!(int("ZombieVillager.SPECIAL_BLOCK_RADIUS"), z::SPECIAL_RADIUS as i64);
    use super::super::wandering_trader as w;
    // Java steps the wait 1200 ticks at a time; the game counts it whole.
    assert_eq!(int("WanderingTraderSpawner.DEFAULT_SPAWN_DELAY") % int("WanderingTraderSpawner.DEFAULT_TICK_DELAY"), 0);
    assert_eq!(int("WanderingTraderSpawner.DEFAULT_SPAWN_DELAY"), w::SPAWN_DELAY as i64);
    assert_eq!(int("WanderingTraderSpawner.MIN_SPAWN_CHANCE"), w::MIN_CHANCE as i64);
    assert_eq!(int("WanderingTraderSpawner.MAX_SPAWN_CHANCE"), w::MAX_CHANCE as i64);
    assert_eq!(int("WanderingTraderSpawner.SPAWN_CHANCE_INCREASE"), w::CHANCE_STEP as i64);
    assert_eq!(int("WanderingTraderSpawner.SPAWN_ONE_IN_X_CHANCE"), w::ONE_IN as i64);
    assert_eq!(int("WanderingTraderSpawner.NUMBER_OF_SPAWN_ATTEMPTS"), w::SPAWN_ATTEMPTS as i64);
    // Villager food, both ways over the items the game has.
    let food = f["villager_food"].as_object().unwrap();
    for (name, points) in food {
        let item = trades::item(name).unwrap();
        assert_eq!(super::super::villager_breeding::food_points(item) as u64, points.as_u64().unwrap(), "{name}");
    }
    for item in Item::all_items() {
        if super::super::villager_breeding::food_points(item) > 0 {
            assert!(food.keys().any(|n| trades::item(n) == Some(item)), "{item:?} is not villager food in Java");
        }
    }
}

/// Track A's flee distances, reviewed by Track H: the villager hostile
/// sensor and the wandering trader's avoid goals.
#[test]
fn villagers_and_wandering_traders_flee_at_java_distances() {
    let f = fixture();
    let java = f["villager_hostile_distances"].as_object().unwrap();
    for kind in MobKind::ALL {
        let expected = java.get(&kind.name().replace(' ', "_")).map(|d| d.as_f64().unwrap());
        assert_eq!(super::super::illager::villager_fear(kind), expected, "{kind:?}");
    }
    for name in java.keys() {
        assert!(MobKind::from_name(name).is_some(), "{name} is not a mob yet");
    }
    use super::super::mob::goals::{Avoided, Goal, avoided_kind, table};
    let class = |a: Avoided| match a {
        Avoided::Zombie => "Zombie",
        Avoided::Evoker => "Evoker",
        Avoided::Vindicator => "Vindicator",
        Avoided::Vex => "Vex",
        Avoided::Pillager => "Pillager",
        Avoided::Illusioner => "Illusioner",
        Avoided::Zoglin => "Zoglin",
        other => panic!("unexpected {other:?}"),
    };
    let game: Vec<Value> = table(MobKind::WanderingTrader)
        .iter()
        .filter_map(|e| match e.goal {
            Goal::Avoid(a) => {
                Some(json!({"class": class(a.what), "distance": a.dist, "walk": a.walk, "sprint": a.sprint}))
            }
            _ => None,
        })
        .collect();
    assert_eq!(Value::Array(game), f["wandering_trader_avoids"]);
    // `Zombie.class` covers its subclasses, zombified piglins among them.
    for kind in [MobKind::Zombie, MobKind::Husk, MobKind::Drowned, MobKind::ZombieVillager, MobKind::ZombifiedPiglin] {
        assert!(avoided_kind(Avoided::Zombie, kind), "{kind:?}");
    }
}
