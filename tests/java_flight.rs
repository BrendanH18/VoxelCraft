//! v0.16 Track B: shulker boxes, elytra and fireworks against
//! `tests/fixtures/java/26.3/flight.json`, written by
//! `tools/extract_java_flight_fixtures.py` from the pinned 26.3 server jar
//! (constants from the bytecode, and an independent model of the glide and
//! the rocket boost run over a table of cases).

use serde_json::Value;
use voxelcraft::{
    color::DyeColor,
    content::{elytra, fireworks as fw, shulker_box},
    entity::firework,
    inventory::Stack,
    item::{ArmorPiece, Item},
    session::ranged,
    world::{block::Block, chest, shulker_box as boxes},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/flight.json")).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn vec3(v: &Value) -> glam::DVec3 {
    let a = v.as_array().unwrap();
    glam::DVec3::new(f(&a[0]), f(&a[1]), f(&a[2]))
}

#[test]
fn rocket_numbers_match_java() {
    let r = &fixture()["rocket"];
    assert_eq!(f(&r["launch_drift"]), firework::LAUNCH_DRIFT);
    assert_eq!(f(&r["launch_speed"]), firework::LAUNCH_SPEED);
    assert_eq!(f(&r["horizontal_acceleration"]), firework::HORIZONTAL_ACCELERATION);
    assert_eq!(f(&r["vertical_acceleration"]), firework::VERTICAL_ACCELERATION);
    assert_eq!(f(&r["damage_base"]) as f32, firework::BASE_DAMAGE);
    assert_eq!(f(&r["damage_per_star"]) as f32, firework::DAMAGE_PER_STAR);
    assert_eq!(f(&r["damage_radius"]), firework::DAMAGE_RADIUS);
    let per = r["lifetime_per_flight"].as_u64().unwrap() as u32;
    let rolls: Vec<u32> = r["lifetime_rolls"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
    for flight in 0..=3u8 {
        assert_eq!(firework::lifetime(flight, 0, 0), per * (1 + u32::from(flight)));
        assert_eq!(firework::lifetime(flight, rolls[0] - 1, rolls[1] - 1), per * (1 + u32::from(flight)) + 11);
    }
    let item = &fixture()["rocket_item"];
    assert_eq!(f(&item["placement_offset"]), firework::PLACEMENT_OFFSET);
    assert_eq!(f(&item["dispense_power"]), firework::DISPENSER_POWER);
    let durations: Vec<u8> =
        item["craftable_durations"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect();
    assert_eq!(durations, fw::CRAFTABLE_DURATIONS);
    assert_eq!(item["default_flight"].as_u64(), Some(u64::from(fw::Fireworks::default().flight)));
    let crossbow = &fixture()["crossbow"];
    assert!((f(&crossbow["firework_power"]) - firework::CROSSBOW_POWER).abs() < 1e-6);
    assert_eq!(crossbow["firework_durability_use"].as_u64(), Some(u64::from(ranged::FIREWORK_DURABILITY_USE)));
}

#[test]
fn shapes_and_colours_match_java() {
    let d = fixture();
    let shapes: Vec<&str> = d["fireworks"]["shapes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(shapes, fw::Shape::ALL.map(fw::Shape::name));
    assert_eq!(d["fireworks"]["max_explosions"].as_u64(), Some(fw::MAX_EXPLOSIONS as u64));
    for c in DyeColor::ALL {
        let name = c.adjective().replace(' ', "_");
        assert_eq!(d["firework_colors"][&name].as_u64(), Some(u64::from(fw::firework_color(c))), "{name}");
    }
}

#[test]
fn elytra_matches_java() {
    let e = &fixture()["elytra"];
    assert_eq!(e["durability"].as_u64(), Some(u64::from(elytra::DURABILITY)));
    assert_eq!(elytra::ELYTRA.durability(), Some(elytra::DURABILITY));
    assert!(elytra::ELYTRA.is_epic());
    assert_eq!(elytra::ELYTRA.equip_piece(), Some(ArmorPiece::Chestplate));
    assert!(voxelcraft::enchant::repairs(elytra::ELYTRA, Item::PHANTOM_MEMBRANE));
    let drag: Vec<f32> = e["drag"].as_array().unwrap().iter().map(|v| f(v) as f32).collect();
    assert_eq!(drag, [elytra::HORIZONTAL_DRAG, elytra::VERTICAL_DRAG]);
    assert_eq!(e["ticks_per_free_fall_event"].as_u64(), Some(u64::from(elytra::TICKS_PER_FREE_FALL_EVENT)));
    assert_eq!(e["free_fall_events_per_break"].as_u64(), Some(u64::from(elytra::FREE_FALL_EVENTS_PER_BREAK)));
    let wall: Vec<f64> = e["wall_damage"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(elytra::wall_damage(1.0, 0.0), Some((wall[0] - wall[1]) as f32));
}

#[test]
fn the_glide_follows_java_tick_by_tick() {
    for case in fixture()["glide_cases"].as_array().unwrap() {
        let (x_rot, y_rot) = (f(&case["x_rot"]) as f32, f(&case["y_rot"]) as f32);
        let look = elytra::look_angle(y_rot, x_rot);
        let mut m = vec3(&case["start"]);
        for (tick, want) in case["ticks"].as_array().unwrap().iter().enumerate() {
            m = elytra::glide(m, look, x_rot, f(&case["gravity"]));
            let want = vec3(want);
            assert!((m - want).abs().max_element() < 1e-9, "pitch {x_rot} yaw {y_rot} tick {tick}: {m} != {want}");
        }
    }
}

#[test]
fn the_rocket_boost_follows_java() {
    for case in fixture()["boost_cases"].as_array().unwrap() {
        let look = elytra::look_angle(f(&case["y_rot"]) as f32, f(&case["x_rot"]) as f32);
        let got = elytra::rocket_boost(vec3(&case["start"]), look);
        assert!((got - vec3(&case["result"])).abs().max_element() < 1e-9, "{got}");
    }
}

#[test]
fn shulker_boxes_match_java() {
    let s = &fixture()["shulker_box"];
    assert_eq!(s["container_size"].as_u64(), Some(chest::SLOTS as u64));
    assert_eq!(s["opening_tick_length"].as_u64(), Some((1.0 / boxes::LID_STEP).round() as u64));
    assert_eq!(f(&s["max_lid_height"]) as f32, boxes::MAX_LID_HEIGHT);
    assert_eq!(f(&s["max_lid_rotation"]) as f32, boxes::MAX_LID_ROTATION);
    assert_eq!(f(&s["strength"]) as f32, boxes::HARDNESS);
    let names: Vec<&str> = s["blocks"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let ours: Vec<Item> = shulker_box::all().collect();
    assert_eq!(names.len(), ours.len());
    for (name, item) in names.iter().zip(&ours) {
        assert_eq!(Item::from_name(name), Some(*item), "{name}");
        assert_eq!(item.max_stack(), 1);
        let loot = &s["loot"][*name];
        assert_eq!(loot["item"].as_str(), Some(*name));
        assert!(loot["copies"].as_array().unwrap().iter().any(|c| c == "container"));
        assert!(loot["conditions"].as_array().unwrap().is_empty(), "drops whatever breaks it");
        assert_eq!(item.block().map(Block::hardness), Some(boxes::HARDNESS));
    }
    // `#shulker_boxes` in the same order.
    let d = fixture();
    let tag: Vec<&str> =
        d["tags"]["item/shulker_boxes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(tag, names);
}

#[test]
fn recipes_match_java() {
    let d = fixture();
    let r = &d["recipes"];
    assert_eq!(r["firework_rocket_simple"]["result"]["count"].as_u64(), Some(u64::from(fw::ROCKETS_PER_CRAFT)));
    assert_eq!(r["firework_rocket"]["fuel"], "minecraft:gunpowder");
    assert_eq!(r["firework_star"]["shapes"]["burst"], "minecraft:feather");
    assert_eq!(fw::shape_of(Item::FEATHER), Some(fw::Shape::Burst));
    assert_eq!(fw::shape_of(Item::FIRE_CHARGE), Some(fw::Shape::LargeBall));
    assert_eq!(fw::shape_of(Item::GOLD_NUGGET), Some(fw::Shape::Star));
    // Every `#skulls` item this build registers makes the creeper shape;
    // Track C's heads join when they register.
    for name in d["tags"]["item/skulls"].as_array().unwrap() {
        if let Some(item) = Item::from_name(name.as_str().unwrap()) {
            assert_eq!(fw::shape_of(item), Some(fw::Shape::Creeper), "{name}");
        }
    }
    for c in DyeColor::ALL {
        let name = format!("{}_shulker_box", c.adjective().replace(' ', "_"));
        assert_eq!(r[&name]["type"], "minecraft:crafting_transmute");
        let plain = Stack::new(shulker_box::item(None), 1);
        let dyed = shulker_box::transmute(&[plain, Stack::new(c.dye(), 1)]).flatten().unwrap();
        assert_eq!(dyed.item, shulker_box::item(Some(c)));
    }
    // `shulker_box.json` needs v0.16 Track A's shulker shell: when it is
    // registered, two shells around a chest make a plain box.
    assert_eq!(r["shulker_box"]["key"]["-"], "minecraft:shulker_shell");
    match voxelcraft::crafting::flight::shulker_shell() {
        Some(shell) => {
            let mut g = voxelcraft::crafting::Grid::new(3);
            g.cells[1] = Some(Stack::new(shell, 1));
            g.cells[4] = Some(Stack::new(Block::CHEST, 1));
            g.cells[7] = Some(Stack::new(shell, 1));
            assert_eq!(g.result(), Some(Stack::new(shulker_box::item(None), 1)));
        }
        None => assert!(
            !voxelcraft::crafting::recipes().iter().any(|r| r.result.item == shulker_box::item(None)),
            "no box recipe without the shell"
        ),
    }
}
