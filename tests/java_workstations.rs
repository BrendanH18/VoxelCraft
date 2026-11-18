//! v0.15 Track B against `tests/fixtures/java/26.3/workstations.json`
//! (made by `tools/extract_java_workstation_fixtures.py` from the official
//! server jar): banner patterns and the loom's lists, the pattern items,
//! spyglass and books, the ominous banner, the stonecutter's recipe order,
//! the lectern's comparator output, and the constants the game copies.
//! Each check goes both ways where the game has a list of its own.

use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use voxelcraft::color::DyeColor;
use voxelcraft::content::{banner_patterns as bp, books, spyglass};
use voxelcraft::crafting::stonecutting;
use voxelcraft::inventory::Stack;
use voxelcraft::inventory::components::{
    BANNER_PATTERNS, BannerPattern, WRITABLE_MAX_PAGES, WRITABLE_PAGE_LENGTH, WRITTEN_MAX_GENERATION,
    WRITTEN_TITLE_MAX_LENGTH,
};
use voxelcraft::item::Item;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/workstations.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_owned()).collect()
}

fn item(java: &str) -> Item {
    Item::from_name(java).unwrap_or_else(|| panic!("{java} is not registered"))
}

fn dye_id(c: DyeColor) -> String {
    c.adjective().replace(' ', "_")
}

#[test]
fn banner_patterns_and_loom_lists_match_java() {
    let f = fixture();
    let java: BTreeSet<String> = strings(&f["banner_patterns"]).into_iter().collect();
    let ours: BTreeSet<String> = BANNER_PATTERNS.iter().map(|s| s.to_string()).collect();
    assert_eq!(ours, java, "the banner pattern registry");
    assert_eq!(bp::NO_ITEM_REQUIRED.to_vec(), strings(&f["no_item_required"]), "#no_item_required, in order");
    let loom_list: Vec<String> = bp::selectable(None).iter().map(|p| p.name().to_string()).collect();
    assert_eq!(loom_list, strings(&f["no_item_required"]), "the loom's list without a pattern item");
    // `#loom_patterns` in order, each item providing its `pattern_item` tag.
    let loom_patterns = strings(&f["item_tags"]["loom_patterns"]);
    assert_eq!(bp::PATTERN_ITEMS.iter().map(|p| p.java).collect::<Vec<_>>(), loom_patterns);
    for (i, p) in bp::PATTERN_ITEMS.iter().enumerate() {
        assert_eq!(item(p.java), bp::item(i));
        let tag = p.java.strip_suffix("_banner_pattern").unwrap();
        let provided = strings(&f["pattern_item_tags"][tag]);
        assert_eq!(provided, [p.pattern], "{}", p.java);
        let s = Stack::new(bp::item(i), 1);
        assert_eq!(bp::selectable(Some(s)), [BannerPattern::from_name(p.pattern).unwrap()]);
    }
    let tags = f["pattern_item_tags"].as_object().unwrap();
    assert_eq!(tags.len(), bp::PATTERN_ITEMS.len(), "every pattern item tag has an item");
}

#[test]
fn items_have_java_stack_sizes_rarity_and_glint() {
    let f = fixture();
    for (java, facts) in f["items"].as_object().unwrap() {
        let it = item(java);
        assert_eq!(u64::from(it.max_stack()), facts["max_stack"].as_u64().unwrap(), "{java} stack size");
        let rarity = match (it.is_uncommon(), it.is_rare()) {
            (false, false) => "common",
            (true, false) => "uncommon",
            (false, true) => "rare",
            _ => panic!("{java} has two rarities"),
        };
        assert_eq!(rarity, facts["rarity"], "{java} rarity");
        assert_eq!(it.has_glint(), facts["glint"] == true, "{java} glint");
    }
    assert_eq!(item("spyglass"), Item::SPYGLASS);
    assert_eq!(item("writable_book"), Item::WRITABLE_BOOK);
    assert_eq!(item("written_book"), Item::WRITTEN_BOOK);
}

#[test]
fn item_tags_match_both_ways() {
    let f = fixture();
    let tag = |name: &str| -> HashSet<Item> { strings(&f["item_tags"][name]).iter().map(|j| item(j)).collect() };
    let ours: HashSet<Item> = Item::all_items().filter(|&i| books::lectern_book(i)).collect();
    assert_eq!(ours, tag("lectern_books"));
    assert_eq!(tag("book_cloning_target"), HashSet::from([Item::WRITABLE_BOOK]));
    let ours: HashSet<Item> = Item::all_items().filter(|&i| voxelcraft::world::maps::extendable(i)).collect();
    assert_eq!(ours, tag("extendable_maps"));
    let ours: HashSet<Item> = Item::all_items().filter(|&i| bp::is_pattern_item(i)).collect();
    assert_eq!(ours, tag("loom_patterns"));
}

#[test]
fn ominous_banner_has_raid_layers() {
    let f = fixture();
    let banner = bp::ominous_banner();
    assert_eq!(bp::banner_color(banner.item).map(dye_id).as_deref(), f["ominous_banner"]["base"].as_str());
    let layers: Vec<(String, String)> =
        bp::patterns(&banner).layers.iter().map(|l| (l.pattern.name().to_string(), dye_id(l.color))).collect();
    let java: Vec<(String, String)> = f["ominous_banner"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["pattern"].as_str().unwrap().to_string(), l["color"].as_str().unwrap().to_string()))
        .collect();
    assert_eq!(layers, java);
    assert!(bp::is_ominous(&banner));
    assert_eq!(banner.display_name(), bp::OMINOUS_NAME);
    assert!(layers.len() > bp::MAX_LAYERS, "more layers than the loom allows");
}

#[test]
fn constants_match_java() {
    let c = &fixture()["constants"];
    let int = |k: &str| c[k].as_u64().unwrap_or_else(|| panic!("{k}"));
    assert_eq!(u64::from(spyglass::USE_DURATION), int("SpyglassItem.USE_DURATION"));
    assert_eq!(spyglass::ZOOM_FOV_MODIFIER, c["SpyglassItem.ZOOM_FOV_MODIFIER"].as_f64().unwrap() as f32);
    assert_eq!(bp::MAX_LAYERS as u64, int("BannerBlockEntity.MAX_PATTERNS"));
    assert_eq!(voxelcraft::world::workstations::PAGE_PULSE_TICKS, int("LecternBlock.PAGE_CHANGE_IMPULSE_TICKS"));
    assert_eq!(WRITABLE_MAX_PAGES as u64, int("WritableBookContent.MAX_PAGES"));
    assert_eq!(WRITABLE_PAGE_LENGTH as u64, int("WritableBookContent.PAGE_EDIT_LENGTH"));
    assert_eq!(WRITTEN_TITLE_MAX_LENGTH as u64, int("WrittenBookContent.TITLE_MAX_LENGTH"));
    assert_eq!(u64::from(WRITTEN_MAX_GENERATION), int("WrittenBookContent.MAX_GENERATION"));
}

#[test]
fn lectern_signal_follows_the_java_model() {
    for case in fixture()["lectern_signal"].as_array().unwrap() {
        let pages = case["pages"].as_u64().unwrap() as usize;
        let page = case["page"].as_i64().unwrap();
        let clamped = books::clamp_page(page, pages);
        assert_eq!(clamped as u64, case["clamped"].as_u64().unwrap(), "{case}");
        assert_eq!(u64::from(books::lectern_signal(clamped, pages)), case["signal"].as_u64().unwrap(), "{case}");
    }
}

#[test]
fn stonecutter_lists_recipes_in_java_id_order() {
    let f = fixture();
    let mut compared = 0;
    for (input, entries) in f["stonecutter_menus"].as_object().unwrap() {
        let Some(input_item) = Item::from_name(input) else { continue };
        let java: Vec<(Item, u8)> = entries
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| {
                let result = Item::from_name(e["result"].as_str()?)?;
                Some((result, e["count"].as_u64()? as u8))
            })
            .collect();
        assert_eq!(stonecutting::menu(input_item), java.as_slice(), "{input}");
        for e in entries.as_array().unwrap() {
            if let Some(result) = Item::from_name(e["result"].as_str().unwrap()) {
                let id = format!(
                    "{}_from_{}_stonecutting",
                    stonecutting::java_id(result),
                    stonecutting::java_id(input_item)
                );
                assert_eq!(id, e["id"].as_str().unwrap(), "the recipe id the order sorts by");
            }
        }
        compared += 1;
    }
    assert!(compared > 50, "only {compared} stonecutter inputs compared");
}

#[test]
fn loot_and_trades_name_registered_items() {
    let f = fixture();
    for (table, entries) in f["loot"].as_object().unwrap() {
        for e in entries.as_array().unwrap() {
            item(e["item"].as_str().unwrap());
            assert!(e["weight"].as_u64().unwrap() > 0, "{table}");
        }
    }
    let game: Value = serde_json::from_str(include_str!("../src/world/loot_tables.json")).unwrap();
    for table in ["bastion_other", "trial_chambers/reward_unique", "trial_chambers/reward_ominous_unique"] {
        let java = &f["loot"][format!("chests/{table}")];
        let text = game["tables"][table].to_string();
        for e in java.as_array().unwrap() {
            assert!(text.contains(&format!("minecraft:{}", e["item"].as_str().unwrap())), "{table}");
        }
    }
    for t in f["trades"].as_array().unwrap() {
        item(t["gives"].as_str().unwrap());
        item(t["wants"].as_str().unwrap());
    }
}
