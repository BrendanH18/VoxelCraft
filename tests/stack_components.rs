//! Java 26.3 item-stack components (ITEM-00-COMPONENTS): the trim, dyed
//! colour and interned payload slots against `stack_components.json`,
//! `ItemStack.isSameItemSameComponents` merging on every stack path, and the
//! save codec through every container, a second process and older strings.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::{collections::BTreeMap, process::Command, sync::Arc};
use voxelcraft::{
    color::DyeColor,
    enchant::{Enchantment, Enchants},
    entity::Entities,
    inventory::{
        Inventory, Lodestone, Stack, click_slot,
        components::{
            self as c, BannerLayer, BannerPattern, BannerPatterns, Payload, Trim, TrimMaterial, TrimPattern,
            WrittenBook,
        },
        move_into, stack_from_str, stack_to_string,
    },
    item::{ArmorMaterial, ArmorPiece, Item, Tier, ToolKind},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        redstone_blocks as r,
        storage::{LevelInfo, Storage},
        terrain::{Dimension, Generator},
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/stack_components.json")).unwrap()
}

fn names(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_owned()).collect()
}

fn banner() -> Item {
    Item::from_name("white banner").expect("white banner is registered")
}

fn layer(pattern: &str, color: DyeColor) -> BannerLayer {
    BannerLayer { pattern: BannerPattern::from_name(pattern).unwrap(), color }
}

fn banner_payload(base: Option<DyeColor>, layers: Vec<BannerLayer>) -> Payload {
    Payload::Banner(BannerPatterns { base, layers })
}

fn written(title: &str, generation: u8) -> Payload {
    Payload::WrittenBook(WrittenBook {
        title: title.into(),
        author: "Alex".into(),
        generation,
        pages: vec!["Once upon a time".into(), "The end \u{2728}".into()],
        resolved: true,
    })
}

fn bundle(items: &[(Item, u8)]) -> Payload {
    Payload::Bundle(items.iter().map(|&(i, n)| Stack::new(i, n)).collect())
}

fn trim(pattern: TrimPattern, material: TrimMaterial) -> Option<Trim> {
    Some(Trim { pattern, material })
}

fn helmet() -> Item {
    Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron)
}

/// One stack carrying every component kind, nested bundles included.
fn samples() -> Vec<Stack> {
    let trimmed = Stack { trim: trim(TrimPattern::Wayfinder, TrimMaterial::Resin), ..Stack::new(helmet(), 1) };
    let inner = Stack::new(Item::BOOK, 1).with_payload(bundle(&[(Item::DIAMOND, 2)]));
    vec![
        trimmed,
        c::apply_dyes(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Red, DyeColor::Blue]).unwrap(),
        Stack::new(banner(), 3).with_payload(banner_payload(
            None,
            vec![layer("creeper", DyeColor::Black), layer("border", DyeColor::Red)],
        )),
        Stack::new(Item::SHIELD, 1)
            .with_payload(banner_payload(Some(DyeColor::Lime), vec![layer("globe", DyeColor::Blue)])),
        Stack::new(Item::BOOK, 1).with_payload(Payload::WritableBook(vec![
            "first".into(),
            String::new(),
            "x".repeat(1024),
        ])),
        Stack::new(Item::PAPER, 16).with_payload(written("Tale: of, a; book|#~", 2)),
        Stack::new(Item::BOOK, 1).with_payload(Payload::Bundle(vec![trimmed, inner, Stack::new(Item::APPLE, 7)])),
        Stack {
            enchants: Enchants::NONE.with(Enchantment::Protection, 4),
            repair_cost: 3,
            trim: trim(TrimPattern::Silence, TrimMaterial::Netherite),
            ..Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Netherite), 1)
        }
        .with_name("Night")
        .unwrap(),
    ]
}

#[test]
fn modelled_components_and_registries_match_java() {
    let f = fixture();
    assert_eq!(names(&f["components"]), c::MODELLED.map(str::to_owned));
    // Trim patterns, materials and banner patterns in both directions.
    let patterns: Vec<String> = f["trim_patterns"].as_object().unwrap().keys().cloned().collect();
    assert_eq!(TrimPattern::ALL.map(|p| p.name().to_owned()).to_vec(), patterns);
    assert_eq!(TrimMaterial::ALL.map(|m| m.name().to_owned()).to_vec(), names(&f["trim_materials"]));
    assert_eq!(c::BANNER_PATTERNS.map(str::to_owned).to_vec(), names(&f["banner_patterns"]));
    for p in TrimPattern::ALL {
        assert_eq!(TrimPattern::from_name(p.name()), Some(p));
    }
    for m in TrimMaterial::ALL {
        assert_eq!(TrimMaterial::from_name(m.name()), Some(m));
    }
    assert_eq!(BannerPattern::all().count(), c::BANNER_PATTERNS.len());
    // The limits Java's book codecs enforce.
    assert_eq!(f["writable_book"]["max_pages"], c::WRITABLE_MAX_PAGES);
    assert_eq!(f["writable_book"]["page_edit_length"], c::WRITABLE_PAGE_LENGTH);
    assert_eq!(f["written_book"]["page_length"], c::WRITTEN_PAGE_LENGTH);
    assert_eq!(f["written_book"]["title_max_length"], c::WRITTEN_TITLE_MAX_LENGTH);
    assert_eq!(f["written_book"]["max_generation"], c::WRITTEN_MAX_GENERATION);
    // The dye colours `DyedItemColor` mixes.
    let colors = f["dye_colors"].as_object().unwrap();
    assert_eq!(colors.len(), DyeColor::ALL.len());
    for d in DyeColor::ALL {
        let [r, g, b] = d.rgb();
        let id = d.adjective().replace(' ', "_");
        assert_eq!(colors[&id], format!("#{r:02X}{g:02X}{b:02X}"), "{id}");
    }
}

/// `#cauldron_can_remove_dye` and `#trimmable_armor` against the registered
/// items, both ways; Java items the game lacks must be owned gaps.
#[test]
fn dyeable_and_trimmable_items_match_java_tags() {
    let f = fixture();
    let reference: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap();
    let gaps: Value = serde_json::from_str(include_str!("fixtures/java/26.3/registry_gaps.json")).unwrap();
    let mut java_name: BTreeMap<u16, String> = BTreeMap::new();
    for (id, english) in reference["items"].as_object().unwrap() {
        if let Some(item) = voxelcraft::java_registry::item(id, english.as_str().unwrap()) {
            java_name.entry(item.0).or_insert_with(|| id.clone());
        }
    }
    for (tag, predicate) in [("dyeable", c::dyeable as fn(Item) -> bool), ("trimmable_armor", c::trimmable)] {
        let members = names(&f[tag]);
        assert!(!members.is_empty());
        for id in &members {
            let english = reference["items"][id].as_str().unwrap();
            match voxelcraft::java_registry::item(id, english) {
                Some(item) => assert!(predicate(item), "{tag}: {id}"),
                None => assert!(gaps["items"].get(id).is_some(), "{tag}: {id} is neither registered nor a gap"),
            }
        }
        for item in Item::all_items().filter(|&i| predicate(i)) {
            let id = java_name.get(&item.0).unwrap_or_else(|| panic!("{tag}: {} has no Java name", item.name()));
            assert!(members.contains(id), "{tag}: {id} is not in Java's tag");
        }
    }
}

/// `ItemStack.isSameItemSameComponents`: two stacks of one item merge only
/// when every component is equal, through `stacks_with`, `Inventory::add_stack`,
/// `move_into`, `click_slot` and dropped-item merging.
#[test]
fn differing_components_never_stack_and_equal_ones_do() {
    type Make = fn() -> Stack;
    fn base() -> Stack {
        Stack::new(banner(), 1)
    }
    let cases: [(&str, Make, Make); 14] = [
        (
            "bundle_contents",
            || base().with_payload(bundle(&[(Item::DIAMOND, 1)])),
            || base().with_payload(bundle(&[(Item::DIAMOND, 2)])),
        ),
        (
            "bundle_contents order",
            || base().with_payload(bundle(&[(Item::DIAMOND, 1), (Item::APPLE, 1)])),
            || base().with_payload(bundle(&[(Item::APPLE, 1), (Item::DIAMOND, 1)])),
        ),
        ("bundle_contents vs none", || base().with_payload(bundle(&[(Item::DIAMOND, 1)])), base),
        (
            "writable_book_content",
            || base().with_payload(Payload::WritableBook(vec!["a".into()])),
            || base().with_payload(Payload::WritableBook(vec!["b".into()])),
        ),
        (
            "written_book_content title",
            || base().with_payload(written("Tale", 0)),
            || base().with_payload(written("Other", 0)),
        ),
        (
            "written_book_content generation",
            || base().with_payload(written("Tale", 0)),
            || base().with_payload(written("Tale", 1)),
        ),
        (
            "written vs writable",
            || base().with_payload(written("Tale", 0)),
            || base().with_payload(Payload::WritableBook(vec!["Once upon a time".into(), "The end \u{2728}".into()])),
        ),
        (
            "banner_patterns colour",
            || base().with_payload(banner_payload(None, vec![layer("creeper", DyeColor::Black)])),
            || base().with_payload(banner_payload(None, vec![layer("creeper", DyeColor::Red)])),
        ),
        (
            "banner_patterns pattern",
            || base().with_payload(banner_payload(None, vec![layer("creeper", DyeColor::Black)])),
            || base().with_payload(banner_payload(None, vec![layer("skull", DyeColor::Black)])),
        ),
        (
            "base_color",
            || base().with_payload(banner_payload(Some(DyeColor::Red), vec![])),
            || base().with_payload(banner_payload(Some(DyeColor::Blue), vec![])),
        ),
        (
            "trim pattern",
            || Stack { trim: trim(TrimPattern::Bolt, TrimMaterial::Iron), ..base() },
            || Stack { trim: trim(TrimPattern::Coast, TrimMaterial::Iron), ..base() },
        ),
        (
            "trim material",
            || Stack { trim: trim(TrimPattern::Bolt, TrimMaterial::Iron), ..base() },
            || Stack { trim: trim(TrimPattern::Bolt, TrimMaterial::Gold), ..base() },
        ),
        ("trim vs none", || Stack { trim: trim(TrimPattern::Bolt, TrimMaterial::Iron), ..base() }, base),
        (
            "dyed_color",
            || c::with_dyed_color(base(), Some([10, 20, 30])),
            || c::with_dyed_color(base(), Some([10, 20, 31])),
        ),
    ];
    for (name, a, b) in cases {
        let (x, y, z) = (a(), a(), b());
        assert_eq!(x, y, "{name}: built twice, the components are equal");
        assert!(x.stacks_with(&y), "{name}: equal components stack");
        assert!(!x.stacks_with(&z) && !z.stacks_with(&x), "{name}: different components never stack");
        let n = |s: Stack, count: u8| Stack { count, ..s };

        let mut inv = Inventory::default();
        assert_eq!(inv.add_stack(n(x, 2)), 0);
        assert_eq!(inv.add_stack(n(y, 3)), 0);
        assert_eq!(inv.add_stack(n(z, 1)), 0);
        assert_eq!((inv.get(0), inv.get(1)), (Some(n(x, 5)), Some(n(z, 1))), "{name}: add_stack");

        let mut slots = [Some(n(x, 2)), None];
        assert_eq!(move_into(n(z, 1), &mut slots, &[0, 1]), None);
        assert_eq!(move_into(n(y, 1), &mut slots, &[0, 1]), None);
        assert_eq!(slots, [Some(n(x, 3)), Some(n(z, 1))], "{name}: move_into");

        let (mut slot, mut cursor) = (Some(n(x, 2)), Some(n(z, 1)));
        click_slot(&mut slot, &mut cursor, false);
        assert_eq!((slot, cursor), (Some(n(z, 1)), Some(n(x, 2))), "{name}: different stacks swap");
        let (mut slot, mut cursor) = (Some(n(x, 2)), Some(n(y, 1)));
        click_slot(&mut slot, &mut cursor, false);
        assert_eq!((slot, cursor), (Some(n(x, 3)), None), "{name}: equal stacks merge");

        let mut e = Entities::new(3);
        for s in [n(x, 1), n(z, 1), n(y, 1)] {
            e.scatter(s, DVec3::new(0.5, 150.0, 0.5));
        }
        for item in &mut e.items {
            item.pos = DVec3::new(0.5, 150.0, 0.5);
        }
        voxelcraft::entity::item::merge(&mut e.items);
        let mut counts: Vec<(bool, u8)> = e.items.iter().map(|i| (i.stack.stacks_with(&x), i.stack.count)).collect();
        counts.sort();
        assert_eq!(counts, [(false, 1), (true, 2)], "{name}: dropped items");
    }
}

/// `PatchedDataComponentMap.set`: a component set to the item default leaves
/// no patch, so an emptied bundle stacks with a fresh one.
#[test]
fn item_defaults_leave_the_payload_slot_empty() {
    let plain = Stack::new(banner(), 1);
    for p in [Payload::Bundle(vec![]), Payload::WritableBook(vec![]), banner_payload(None, vec![])] {
        assert!(p.is_default());
        let s = plain.with_payload(p);
        assert_eq!(s.payload, None);
        assert_eq!(s, plain);
        assert_eq!(stack_to_string(Some(s)), stack_to_string(Some(plain)));
    }
    assert!(!written("", 0).is_default(), "a written book has no default content");
    assert!(plain.bundle_contents().is_empty() && plain.writable_pages().is_empty());
    assert!(plain.written_book().is_none() && plain.banner_patterns().is_none());
    let s = plain.with_payload(bundle(&[(Item::APPLE, 2)]));
    assert_eq!(s.bundle_contents(), [Stack::new(Item::APPLE, 2)]);
    let s = plain.with_payload(written("Tale", 1));
    assert_eq!(s.written_book().map(|b| (b.title.as_str(), b.generation)), Some(("Tale", 1)));
}

/// Java's codec limits: `WritableBookContent` (100 pages of 1024 UTF-16
/// units), `WrittenBookContent` (a 32-unit title, generation 0-3, 32767-unit
/// pages) and `ItemStackTemplate` counts 1-99.
#[test]
fn payload_limits_follow_java_codecs() {
    let pages = |n: usize, page: &str| Payload::WritableBook(vec![page.to_owned(); n]);
    assert!(pages(100, &"x".repeat(1024)).is_valid());
    assert!(!pages(101, "x").is_valid());
    assert!(!pages(1, &"x".repeat(1025)).is_valid());
    // An emoji is two UTF-16 units, as Java's `String.length` counts.
    assert!(pages(1, &"\u{1F600}".repeat(512)).is_valid());
    assert!(!pages(1, &"\u{1F600}".repeat(513)).is_valid());
    let book = |title: &str, generation: u8, page: usize| {
        Payload::WrittenBook(WrittenBook {
            title: title.into(),
            generation,
            pages: vec!["y".repeat(page)],
            ..Default::default()
        })
    };
    assert!(book(&"t".repeat(32), 3, 32767).is_valid());
    assert!(!book(&"t".repeat(33), 0, 1).is_valid());
    assert!(!book("t", 4, 1).is_valid());
    assert!(!book("t", 0, 32768).is_valid());
    assert!(!Payload::Bundle(vec![Stack::new(Item::DIAMOND, 100)]).is_valid());
    assert!(Payload::Bundle(vec![Stack::new(Item::DIAMOND, 64)]).is_valid());
}

#[test]
fn stack_strings_round_trip_every_component() {
    for s in samples() {
        let text = stack_to_string(Some(s));
        assert!(!text.contains([',', ';', '|', '#', '=', ' ']), "{text}");
        let back = stack_from_str(&text).unwrap().unwrap();
        assert_eq!(back, s);
        assert_eq!(stack_to_string(Some(back)), text);
        assert_eq!(back.payload(), s.payload(), "the payload content survives");
    }
    // Trailing fields are written only as far as the last one present.
    let id = helmet().0;
    let trimmed = Stack { trim: trim(TrimPattern::Bolt, TrimMaterial::Iron), ..Stack::new(helmet(), 1) };
    assert_eq!(stack_to_string(Some(trimmed)), format!("{id}:1:0::0::0:~:~:~:bolt.iron"));
    let s = Stack { map_id: Some(4), ..trimmed };
    assert_eq!(stack_to_string(Some(s)), format!("{id}:1:0::0::0:~:4:~:bolt.iron"));
    let payload = Stack::new(Item::BOOK, 1).with_payload(written("T", 0));
    let field = payload.payload.unwrap().field();
    assert_eq!(stack_to_string(Some(payload)), format!("{}:1:0::0::0:~:~:~:~:{field}", Item::BOOK.0));
    // Malformed new fields are rejected like malformed old ones.
    for bad in ["bolt", "bolt.wood", "nope.iron"] {
        assert_eq!(stack_from_str(&format!("{id}:1:0::0::0:~:~:~:{bad}")), None, "{bad}");
    }
    assert_eq!(stack_from_str(&format!("{id}:1:0::0::0:~:~:~:~:not*base64")), None);
    assert_eq!(stack_from_str(&format!("{id}:1:0::0::0:~:~:~:~:~:extra")), None);
    // A saved payload past Java's limits is rejected.
    let json = serde_json::to_vec(&serde_json::json!({"writable_book": vec!["p"; 101]})).unwrap();
    assert_eq!(c::Payload::from_field(&base64(&json)), None);
}

/// Standard unpadded base64, written independently of the game's encoder.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = String::new();
    for b in bytes {
        bits.push_str(&format!("{b:08b}"));
    }
    bits.as_bytes()
        .chunks(6)
        .map(|c| A[usize::from_str_radix(&format!("{:0<6}", std::str::from_utf8(c).unwrap()), 2).unwrap()] as char)
        .collect()
}

#[test]
fn saved_payload_fields_are_base64_json_with_names() {
    let s = Stack::new(Item::SHIELD, 1)
        .with_payload(banner_payload(Some(DyeColor::LightBlue), vec![layer("creeper", DyeColor::Black)]));
    let expected = base64(br#"{"banner":{"base":"light_blue","layers":[["creeper","black"]]}}"#);
    assert_eq!(s.payload.unwrap().field(), expected);
    let parsed = c::Payload::from_field(&expected).unwrap().unwrap();
    assert_eq!(Some(parsed), s.payload);
}

/// Stack strings from before v0.17, frozen as literals, load unchanged and
/// write back the same way.
#[test]
fn pre_v017_stack_strings_load_unchanged() {
    let hive = Item::from_block(voxelcraft::world::bees::NEST).0;
    let pick = Item::tool(ToolKind::Pickaxe, Tier::Diamond).0;
    let efficiency = Enchants::NONE.with(Enchantment::Efficiency, 5);
    let cases: Vec<(String, Option<Stack>)> = vec![
        ("-".into(), None),
        ("304:12".into(), Some(Stack::new(Item::BOOK, 12))),
        (format!("{pick}:1:17"), Some(Stack { damage: 17, ..Stack::new(Item(pick), 1) })),
        (
            format!("{pick}:1:0:{}:3:4d696e6572:0", efficiency.to_hex()),
            Some(
                Stack { enchants: efficiency, repair_cost: 3, ..Stack::new(Item(pick), 1) }.with_name("Miner").unwrap(),
            ),
        ),
        // Dyed wolf armor: DYED | 0xB02E26, named "Cedar".
        (
            "1304:1:5::0:4365646172:28323366".into(),
            Some(
                c::with_dyed_color(Stack { damage: 5, ..Stack::new(Item::WOLF_ARMOR, 1) }, Some([0xB0, 0x2E, 0x26]))
                    .with_name("Cedar")
                    .unwrap(),
            ),
        ),
        (
            "518:1:0::0::0:10_64_-3_nether".into(),
            Some(Stack {
                lodestone: Some(Lodestone { target: Some((IVec3::new(10, 64, -3), Dimension::Nether)) }),
                ..Stack::new(Item::COMPASS, 1)
            }),
        ),
        (
            "518:1:0::0::0:-".into(),
            Some(Stack { lodestone: Some(Lodestone { target: None }), ..Stack::new(Item::COMPASS, 1) }),
        ),
        ("1256:1:0::0::0:~:7".into(), Some(Stack { map_id: Some(7), ..Stack::new(Item::FILLED_MAP, 1) })),
    ];
    for (text, expected) in &cases {
        assert_eq!(stack_from_str(text), Some(*expected), "{text}");
        assert_eq!(&stack_to_string(*expected), text);
    }
    // A hive item: `{"bees":[null,null,null],"honey":3}` in hex.
    let text = format!("{hive}:1:0::0::0:~:~:7b2262656573223a5b6e756c6c2c6e756c6c2c6e756c6c5d2c22686f6e6579223a337d");
    let s = stack_from_str(&text).unwrap().unwrap();
    assert_eq!((s.item.0, s.hive.map(|h| h.honey), s.trim, s.payload), (hive, Some(3), None, None));
    assert_eq!(stack_from_str(&stack_to_string(Some(s))), Some(Some(s)));
    // A whole pre-v0.17 inventory, armor and off hand included.
    let mut slots = vec!["-"; 36];
    slots[0] = "304:12";
    slots[3] = "1304:1:5::0:4365646172:28323366";
    let inv = Inventory::deserialize(&format!("{}#-,-,-,-,1256:1:0::0::0:~:7", slots.join(","))).unwrap();
    assert_eq!(inv.get(0), Some(Stack::new(Item::BOOK, 12)));
    assert_eq!(c::dyed_color(&inv.get(3).unwrap()), Some([0xB0, 0x2E, 0x26]));
    assert_eq!(inv.offhand.and_then(|s| s.map_id), Some(7));
}

/// The shared dyed-colour helper: only dyeable items read `entity_data` as a
/// colour, mixing keeps the rest of the stack, and washing removes it.
#[test]
fn dyed_colour_helper_is_shared_and_gated_by_the_dyeable_tag() {
    let boots = Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather);
    for item in [boots, Item::LEATHER_HORSE_ARMOR, Item::WOLF_ARMOR] {
        let named = Stack::new(item, 1).with_name("Kit").unwrap();
        let dyed = c::apply_dyes(named, &[DyeColor::Red, DyeColor::Blue]).unwrap();
        assert_eq!(c::dyed_color(&dyed), Some([173, 83, 152]), "{}", item.name());
        assert_eq!(dyed.name, named.name);
        let twice = c::apply_dyes(dyed, &[DyeColor::Yellow]).unwrap();
        assert_eq!(c::dyed_color(&twice), c::mix_dyes(Some([173, 83, 152]), &[DyeColor::Yellow]));
        assert_eq!(c::remove_dye(dyed), Some(named));
        assert_eq!(c::remove_dye(named), None);
        assert!(!dyed.stacks_with(&named));
    }
    assert_eq!(c::apply_dyes(Stack::new(Item::WOLF_ARMOR, 1), &[]), None);
    // A tipped arrow's `entity_data` is a potion, never a colour.
    let arrow = Stack { entity_data: u32::MAX, ..Stack::new(Item::TIPPED_ARROW, 1) };
    assert_eq!(c::dyed_color(&arrow), None);
    assert_eq!(c::apply_dyes(Stack::new(Item::DIAMOND, 1), &[DyeColor::Red]), None);
    // Wolf armor's own helpers are the shared ones.
    let wolf = c::apply_dyes(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Cyan]).unwrap();
    assert_eq!(voxelcraft::entity::wolf_armor::dye(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Cyan]), Some(wolf));
    assert_eq!(voxelcraft::entity::wolf_armor::color(wolf), c::dyed_color(&wolf));
}

#[test]
fn stacks_stay_small_to_copy() {
    let size = std::mem::size_of::<Stack>();
    assert!(size <= 160, "Stack is {size} bytes");
    assert_eq!(std::mem::size_of::<Option<c::StackPayload>>(), std::mem::size_of::<usize>());
}

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

fn decor_text(stacks: &[Stack]) -> String {
    let s = |i: usize| stack_to_string(stacks.get(i).copied());
    format!("F,0,4,145,4,1,0,0,3,{};S,8.500,145.000,8.500,90,{},{},{},{}", s(0), s(1), s(2), s(3), s(4))
}

/// Component stacks keep every component through the inventory, a chest, a
/// hopper moving them between chests, dropped items, an item frame and an
/// armor stand, each saved and loaded again.
#[test]
fn component_stacks_survive_every_container() {
    let samples = samples();
    // Player inventory: slots, armor, off hand and spill.
    let mut inv = Inventory::default();
    for (i, &s) in samples.iter().enumerate() {
        inv.slots[i * 3] = Some(s);
    }
    inv.armor[0] = Some(samples[0]);
    inv.offhand = Some(samples[2]);
    inv.slots.iter_mut().filter(|s| s.is_none()).for_each(|s| *s = Some(Stack::new(Block::STONE, 64)));
    inv.cursor = Some(samples[5]);
    inv.return_stacks([samples[6]]);
    assert_eq!(Inventory::deserialize(&inv.serialize()), Some(inv.clone()));

    // A chest's save, and a hopper carrying each stack one item at a time.
    let (top, hopper, bottom) = (IVec3::new(4, 147, 4), IVec3::new(4, 146, 4), IVec3::new(4, 145, 4));
    let mut w = world();
    w.set_block(top, Block::CHEST);
    w.set_block(hopper, r::hopper(4, false));
    w.set_block(bottom, Block::CHEST);
    for (i, &s) in samples.iter().enumerate() {
        w.chest_mut(top).unwrap().slots[i] = Some(Stack { count: 1, ..s });
    }
    let saved = w.chests_to_string();
    let mut loaded = world();
    loaded.load_chests(&saved);
    assert_eq!(loaded.chest(top), w.chest(top));
    for _ in 0..200 {
        w.tick_redstone();
    }
    let mut moved: Vec<Stack> = w.chest(bottom).unwrap().slots.iter().flatten().copied().collect();
    let mut expected: Vec<Stack> = samples.iter().map(|&s| Stack { count: 1, ..s }).collect();
    let key = |s: &Stack| stack_to_string(Some(*s));
    moved.sort_by_key(key);
    expected.sort_by_key(key);
    assert_eq!(moved, expected, "the hopper moved every stack whole");

    // Dropped items.
    let mut e = Entities::new(5);
    for (i, &s) in samples.iter().enumerate() {
        e.scatter(s, DVec3::new(i as f64 * 4.0, 150.0, 0.5));
    }
    let mut e2 = Entities::new(5);
    e2.load_items(&e.items_to_string());
    assert_eq!(e2.items.iter().map(|i| i.stack).collect::<Vec<_>>(), samples);

    // An item frame's item and an armor stand's armor.
    let mut e = Entities::new(5);
    e.load_decor(&decor_text(&samples));
    assert_eq!(e.decor.len(), 2);
    assert_eq!(e.decor[0].item, Some(samples[0]));
    let mut e2 = Entities::new(5);
    e2.load_decor(&e.decor_to_string());
    assert_eq!(e2.decor, e.decor);
    assert_eq!(e2.decor[1].armor[..], samples[1..5].iter().map(|&s| Some(s)).collect::<Vec<_>>()[..]);
}

/// Two processes: one saves component stacks to disk, the next loads them
/// with an empty intern table and finds equal stacks.
#[test]
fn component_stacks_survive_a_process_restart() {
    let path = std::env::temp_dir().join(format!(
        "voxelcraft-stack-components-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for phase in ["write", "read"] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "component_persistence_worker", "--nocapture"])
            .env("VOXELCRAFT_COMPONENT_DIR", &path)
            .env("VOXELCRAFT_COMPONENT_PHASE", phase)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{phase}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains(&format!("components {phase} ok")));
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn component_persistence_worker() {
    let Ok(path) = std::env::var("VOXELCRAFT_COMPONENT_DIR") else {
        return;
    };
    let phase = std::env::var("VOXELCRAFT_COMPONENT_PHASE").unwrap();
    let storage = Storage::new(&path);
    let _lock = storage.lock().unwrap();
    let samples = samples();
    let chest = IVec3::new(6, 145, 6);
    let mut w = world();
    if phase == "write" {
        w.set_block(chest, Block::CHEST);
        for (i, &s) in samples.iter().enumerate() {
            w.chest_mut(chest).unwrap().slots[i] = Some(s);
        }
        let mut inv = Inventory::default();
        for (i, &s) in samples.iter().enumerate() {
            inv.slots[i] = Some(s);
        }
        inv.armor[0] = Some(samples[0]);
        let mut e = Entities::new(9);
        e.load_decor(&decor_text(&samples));
        for &s in &samples {
            e.scatter(s, DVec3::new(2.5, 150.0, 2.5));
        }
        let mut props = BTreeMap::new();
        props.insert("inventory".into(), inv.serialize());
        props.insert("chests".into(), w.chests_to_string());
        e.save_props(&mut props);
        storage.save(&LevelInfo { seed: 7, player: None, props }, &w.modified_chunks()).unwrap();
    } else {
        let level = storage.load_level().unwrap();
        let inv = Inventory::deserialize(&level.props["inventory"]).unwrap();
        assert_eq!(inv.slots[..samples.len()], samples.iter().map(|&s| Some(s)).collect::<Vec<_>>()[..]);
        assert_eq!(inv.armor[0], Some(samples[0]));
        w.load_chests(&level.props["chests"]);
        assert_eq!(w.chest(chest).unwrap().slots[..samples.len()], inv.slots[..samples.len()]);
        let mut e = Entities::new(9);
        e.load_props(&level.props);
        assert_eq!(e.items.iter().map(|i| i.stack).collect::<Vec<_>>(), samples);
        assert_eq!(e.decor[0].item, Some(samples[0]));
        assert_eq!(e.decor[1].armor[0], Some(samples[1]));
        let book = inv.slots[5].unwrap().written_book().unwrap();
        assert_eq!((book.title.as_str(), book.generation), ("Tale: of, a; book|#~", 2));
        assert_eq!(inv.slots[6].unwrap().bundle_contents()[1].bundle_contents(), [Stack::new(Item::DIAMOND, 2)]);
    }
    println!("components {phase} ok");
}
