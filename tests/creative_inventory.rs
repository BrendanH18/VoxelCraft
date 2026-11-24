//! v0.18 Track 4: the creative tabs against Java 26.3's own
//! (`tests/fixtures/java/26.3/creative_tabs.json`, built by running
//! `CreativeModeTabs` from the pinned jar), and the grid and scrolling
//! against `creative_layout.json`.
//!
//! Each tab must list, in Java's order, every stack whose item the game
//! registers. Entries are resolved here independently of the game's
//! loader (by Java id and English name, as `tests/java_registry.rs` does),
//! and every entry left out must be a listed registry gap; the stack
//! variants the game can't express are counted and printed.
use serde_json::Value;
use std::collections::BTreeSet;
use voxelcraft::{
    creative::{self, Kind, Row, scroll},
    inventory::Stack,
    item::Item,
    java_registry,
    potion::Potion,
};

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn tabs_fixture() -> Value {
    json(include_str!("fixtures/java/26.3/creative_tabs.json"))
}

fn english(id: &str) -> String {
    let names = json(include_str!("fixtures/java/26.3/registry_names.json"));
    names["items"][id].as_str().or_else(|| names["blocks"][id].as_str()).unwrap_or_default().to_string()
}

/// The player's contents of a fixture tab.
fn contents(tab: &Value) -> &Value {
    if tab.get("items").is_some() { &tab["items"] } else { &tab["player"] }
}

/// What a fixture entry should become, worked out without the game's loader.
enum Expect {
    Stack(Stack),
    /// A stack variant the game has no component for: its plain item.
    Merged(Stack),
    /// Not in the game: a registry gap (or the ominous banner).
    Missing,
}

fn expect(entry: &Value) -> Expect {
    let id = entry["item"].as_str().unwrap();
    let Some(item) = java_registry::item(id, &english(id)) else { return Expect::Missing };
    let c = &entry["components"];
    if let Some(p) = c["potion_contents"]["potion"].as_str() {
        let Some(p) = Potion::from_id(p) else { return Expect::Missing };
        return Expect::Stack(match id {
            "potion" => Stack::new(Item::potion(p), 1),
            "splash_potion" => Stack::new(Item::splash_potion(p), 1),
            "lingering_potion" => Stack::lingering_potion(p),
            "tipped_arrow" => Stack::tipped_arrow(p, 1),
            _ => panic!("potion contents on {id}"),
        });
    }
    if let Some(books) = c["stored_enchantments"].as_object() {
        let mut s = Stack::new(item, 1);
        for (name, level) in books {
            let Some(e) = voxelcraft::enchant::Enchantment::from_name(name) else {
                println!("no enchantment {name}: its book is left out");
                return Expect::Missing;
            };
            s.enchants.set(e, level.as_u64().unwrap() as u8);
        }
        return Expect::Stack(s);
    }
    if let Some(flight) = c["fireworks"]["flight_duration"].as_u64() {
        let f = voxelcraft::content::fireworks::Fireworks { flight: flight as u8, explosions: Vec::new() };
        return Expect::Stack(voxelcraft::content::fireworks::rocket(f, 1));
    }
    if let Some(a) = c["ominous_bottle_amplifier"].as_u64() {
        return Expect::Stack(Stack::new(voxelcraft::world::trial::ominous_bottle(a as u8), 1));
    }
    if let Some(effects) = c["suspicious_stew_effects"].as_array() {
        let e = &effects[0];
        let effect = voxelcraft::simulation::effects::Effect::from_id(e["id"].as_str().unwrap()).unwrap();
        let ticks = e["duration"].as_u64().unwrap() as u32;
        let code = (1..=14).find(|&c| voxelcraft::entity::mooshroom::effect(c) == Some((effect, ticks))).unwrap();
        return Expect::Stack(Stack { entity_data: code, ..Stack::new(Item::SUSPICIOUS_STEW, 1) });
    }
    if c.get("banner_patterns").is_some() {
        assert_eq!(c["item_name"]["translate"], "block.minecraft.ominous_banner");
        return Expect::Stack(voxelcraft::content::banner_patterns::ominous_banner());
    }
    let plain = Stack::new(item, 1);
    if c.is_object() { Expect::Merged(plain) } else { Expect::Stack(plain) }
}

#[test]
fn tabs_are_javas_in_order_with_their_titles_places_and_icons() {
    let fx = tabs_fixture();
    let c = creative::catalogue();
    let tabs = fx["tabs"].as_array().unwrap();
    assert_eq!(c.tabs.len(), tabs.len());
    for (t, f) in c.tabs.iter().zip(tabs) {
        assert_eq!(t.id, f["id"].as_str().unwrap());
        assert_eq!(t.title, f["title"].as_str().unwrap());
        assert_eq!(t.row == Row::Top, f["row"] == "top", "{}", t.id);
        assert_eq!(u64::from(t.column), f["column"].as_u64().unwrap(), "{}", t.id);
        assert_eq!(t.aligned_right, f["aligned_right"].as_bool().unwrap(), "{}", t.id);
        assert_eq!(t.show_title, f["show_title"].as_bool().unwrap(), "{}", t.id);
        assert_eq!(t.can_scroll, f["can_scroll"].as_bool().unwrap(), "{}", t.id);
        let kind = match f["type"].as_str().unwrap() {
            "hotbar" => Kind::Hotbar,
            "search" => Kind::Search,
            "inventory" => Kind::Inventory,
            _ => Kind::Category,
        };
        assert_eq!(t.kind, kind, "{}", t.id);
        let icon = f["icon"].as_str().unwrap();
        let expected = java_registry::item(icon, &english(icon)).unwrap_or(Item::STICK);
        assert_eq!(t.icon, expected, "{} icon {icon}", t.id);
    }
    // Java's shown tabs without operator permissions: all but Operator Utilities.
    let shown: Vec<&str> = c.shown().map(|i| c.tabs[i].id.as_str()).collect();
    assert_eq!(
        shown,
        [
            "building_blocks",
            "colored_blocks",
            "natural_blocks",
            "functional_blocks",
            "redstone_blocks",
            "hotbar",
            "search",
            "tools_and_utilities",
            "combat",
            "food_and_drinks",
            "ingredients",
            "spawn_eggs",
            "inventory"
        ]
    );
    assert_eq!(c.tabs[c.default_tab()].id, "building_blocks");
}

#[test]
fn each_tab_lists_javas_stacks_in_order_skipping_only_registry_gaps() {
    let fx = tabs_fixture();
    let gaps = json(include_str!("fixtures/java/26.3/registry_gaps.json"));
    let c = creative::catalogue();
    let mut missing = BTreeSet::new();
    let mut merged = BTreeSet::new();
    let mut total = 0;
    for (t, f) in c.tabs.iter().zip(fx["tabs"].as_array().unwrap()) {
        if t.kind != Kind::Category {
            assert!(t.items.is_empty());
            continue;
        }
        let mut list = |key: &str| -> Vec<Stack> {
            let mut out: Vec<Stack> = Vec::new();
            for e in contents(f)[key].as_array().into_iter().flatten() {
                let id = e["item"].as_str().unwrap();
                let stack = match expect(e) {
                    Expect::Stack(s) => s,
                    Expect::Merged(s) => {
                        merged.insert(id.to_string());
                        s
                    }
                    Expect::Missing => {
                        missing.insert((t.id.clone(), id.to_string()));
                        continue;
                    }
                };
                if !out.contains(&stack) {
                    out.push(stack);
                }
            }
            out
        };
        let display = list("display");
        if t.items != display {
            let names = |v: &[Stack]| v.iter().map(|s| s.display_name().to_string()).collect::<Vec<_>>();
            let (game, java) = (names(&t.items), names(&display));
            let first = game.iter().zip(&java).position(|(a, b)| a != b);
            panic!("{}: first difference at {first:?}; game {} stacks, expected {}", t.id, game.len(), java.len());
        }
        if contents(f).get("search").is_some() {
            assert_eq!(t.search, list("search"), "{}: search stacks", t.id);
        } else {
            assert_eq!(t.search, t.items, "{}", t.id);
        }
        total += t.items.len();
        println!("{}: {} of {} Java stacks", t.id, t.items.len(), contents(f)["display"].as_array().unwrap().len());
    }
    // Everything left out is a listed gap, or a variant of a potion or an
    // enchantment the game lacks (printed above).
    for (tab, id) in &missing {
        let gap = gaps["items"].get(id).is_some() || gaps["blocks"].get(id).is_some();
        let variant = id.ends_with("potion") || id == "tipped_arrow" || id == "enchanted_book";
        assert!(gap || variant, "{tab}: {id}");
    }
    let skipped: BTreeSet<_> = c.skipped.iter().cloned().collect();
    assert_eq!(skipped, missing, "the game skips exactly what has no stack");
    println!("{total} stacks across the item tabs; merged variants: {merged:?}");
    println!("skipped: {missing:?}");
    // The stack variants that collapse to one plain item.
    assert!(merged.iter().all(|id| ["painting", "goat_horn"].contains(&id.as_str())), "{merged:?}");
    // The spawn eggs tab holds the 87 registered eggs, in Java's order.
    let eggs = &c.tabs[c.index("spawn_eggs").unwrap()].items;
    let in_tab: Vec<_> =
        eggs.iter().filter(|s| voxelcraft::content::spawn_egg::is_spawn_egg(s.item)).map(|s| s.item).collect();
    assert_eq!(in_tab.len(), 86, "every registered egg but the Wither's, which no tab lists");
    assert!(in_tab.windows(2).all(|w| w[0].0 < w[1].0), "ids follow the tab order");
}

#[test]
fn the_search_tab_is_every_tabs_search_stacks_once() {
    let c = creative::catalogue();
    let mut union: Vec<Stack> = Vec::new();
    for t in &c.tabs {
        for s in &t.search {
            if !union.contains(s) {
                union.push(*s);
            }
        }
    }
    assert_eq!(c.search, union);
    let search = c.index("search").unwrap();
    assert_eq!(c.items(search), c.search.as_slice());
    let fx = tabs_fixture();
    let java = fx["tabs"].as_array().unwrap().iter().find(|t| t["id"] == "search").unwrap();
    println!("search: {} stacks (Java {})", c.search.len(), java["player"]["count"]);
    // Searching by words finds enchanted books by their enchantment.
    let sharp: Vec<_> = c.search.iter().filter(|s| creative::matches(s, "sharpness")).collect();
    assert!(sharp.iter().any(|s| s.item == Item::ENCHANTED_BOOK), "{sharp:?}");
    assert!(c.search.iter().any(|s| creative::matches(s, "creeper spawn")));
}

#[test]
fn grid_and_scrolling_follow_the_layout_fixture() {
    let l = json(include_str!("fixtures/java/26.3/creative_layout.json"));
    let cr = &l["creative"];
    assert_eq!(cr["size"], serde_json::json!([195, 136]));
    assert_eq!(cr["grid"]["columns"], scroll::COLS);
    assert_eq!(cr["grid"]["rows"], scroll::ROWS);
    assert_eq!(cr["scroll"]["scrolls_past"], scroll::ROWS * scroll::COLS);
    assert!(scroll::can_scroll(46) && !scroll::can_scroll(45));
    // The thumb runs the bar's height less 17 (`(yscr2 - yscr - 17) * scrollOffs`).
    assert_eq!(cr["scrollbar"]["travel"].as_f64().unwrap() as f32, scroll::thumb(1.0));
    // Dragging puts the thumb's centre under the pointer over 112 - 15.
    let centre = cr["scrollbar"]["drag_centre"].as_f64().unwrap() as f32;
    let span =
        cr["scrollbar"]["size"][1].as_f64().unwrap() as f32 - cr["scrollbar"]["thumb"][1].as_f64().unwrap() as f32;
    assert_eq!(scroll::drag(centre), 0.0);
    assert_eq!(scroll::drag(centre + span), 1.0);
    assert_eq!(scroll::drag(centre + span / 2.0), 0.5);
    // Rows show from `(int)(offs * hidden + 0.5)`; every tab reaches its last row.
    let c = creative::catalogue();
    for i in c.shown() {
        let n = c.items(i).len();
        if n > 45 {
            let last = scroll::at(n, 1.0, 44).or(scroll::at(n, 1.0, 36)).unwrap();
            assert!(last >= n - 9, "{}: the last row is reachable", c.tabs[i].id);
            assert_eq!(scroll::first_row(n, 1.0), n.div_ceil(9) - 5);
        }
    }
    // Saved hotbars: nine rows of nine, and the keys Java binds.
    let h = &l["hotbars"];
    assert_eq!(h["groups"], 9);
    assert_eq!(h["keys"]["key.saveToolbarActivator"]["default"], "C");
    assert_eq!(h["keys"]["key.loadToolbarActivator"]["default"], "X");
    assert_eq!(creative::Hotbars::default().cells().len(), 81);
    // Java's panel at the supported GUI scales of the two reference sizes.
    for size in ["1920x1080", "1280x720"] {
        for row in l["gui_scales"][size].as_array().unwrap() {
            let gui = &row["gui"];
            let (w, h) = (gui[0].as_i64().unwrap(), gui[1].as_i64().unwrap());
            assert_eq!(row["left"].as_i64().unwrap(), (w - 195) / 2);
            assert_eq!(row["top"].as_i64().unwrap(), (h - 136) / 2);
            // Tabs above and below stay on screen at every supported scale.
            assert!(row["top"].as_i64().unwrap() >= 32 && h - (row["top"].as_i64().unwrap() + 136) >= 32, "{size}");
        }
    }
}

#[test]
fn creative_cells_follow_javas_click_rules() {
    let stone = Stack::new(Item::from_block(voxelcraft::world::block::Block::STONE), 1);
    let mut cursor = None;
    creative::click(&mut cursor, Some(stone), false, true);
    assert_eq!(cursor.map(|c| c.count), Some(64), "shift-click takes a full stack");
    creative::click(&mut cursor, None, true, false);
    assert_eq!(cursor.map(|c| c.count), Some(63), "right on an empty cell drops one");
    creative::click(&mut cursor, None, false, false);
    assert_eq!(cursor, None, "left on an empty cell deletes the held stack");
}

/// Headless costs the creative screen adds (reported, not enforced): the
/// one-time catalogue build and a search's filtering, which the screen
/// caches per query.
#[test]
fn creative_costs_are_reported() {
    let start = std::time::Instant::now();
    let c = creative::catalogue();
    println!(
        "catalogue: {} tabs, {} search stacks, built in {:.1} ms",
        c.tabs.len(),
        c.search.len(),
        start.elapsed().as_secs_f64() * 1e3
    );
    let start = std::time::Instant::now();
    let runs = 200;
    let mut hits = 0;
    for _ in 0..runs {
        hits = c.search_hits("oak").len();
    }
    println!("search \"oak\": {hits} hits, {:.1} us per filter", start.elapsed().as_secs_f64() / f64::from(runs) * 1e6);
    assert!(hits > 5);
}
