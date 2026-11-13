//! Java 26.3 harnesses (item side) against `fixtures/java/26.3/harnesses.json`
//! and the harness recipes in `recipes.json`: the sixteen items, their stack
//! size and `Equippable.harness` data, both recipe families through the
//! crafting grid and the CLI's `craft`, and saving.

#[path = "java_recipes/compare.rs"]
mod compare;

use glam::{DVec3, IVec3};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    color::DyeColor,
    content::harness,
    crafting::Grid,
    entity::Entities,
    inventory::{Inventory, Stack, stack_from_str, stack_to_string},
    item::Item,
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/harnesses.json")).unwrap()
}

fn names() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

/// The Java item `id` as the game resolves it.
fn resolve(id: &str) -> Option<Item> {
    let english = names()["items"][id].as_str().unwrap_or("").to_string();
    voxelcraft::java_registry::item(id, &english)
}

#[test]
fn the_sixteen_harnesses_match_the_java_tag_both_ways() {
    let f = fixture();
    let java = strings(&f["items"]);
    assert_eq!(java.len(), 16);
    // The tag's order is DyeColor order, which the item ids follow.
    for ((id, colour), c) in java.iter().zip(strings(&f["colours"])).zip(DyeColor::ALL) {
        assert_eq!(id, &format!("{colour}_harness"));
        let item = resolve(id).unwrap_or_else(|| panic!("{id} is not registered"));
        assert_eq!(item, harness::harness(c), "{id}");
        assert_eq!(harness::colour(item), Some(c), "{id}");
        assert_eq!(item.name(), id.replace('_', " "), "{id}");
        assert!(Item::creative_palette().any(|i| i == item), "{id} is missing from the creative palette");
    }
    // Nothing the game calls a harness is missing from Java's tag.
    let ours: BTreeSet<String> =
        Item::all_items().filter(|&i| harness::colour(i).is_some()).map(|i| i.name().replace(' ', "_")).collect();
    assert_eq!(ours, java.iter().cloned().collect::<BTreeSet<_>>());
    assert_eq!(harness::all().count(), 16);
}

#[test]
fn harnesses_stack_to_one_and_carry_the_java_equippable_data() {
    let f = fixture();
    for item in harness::all() {
        assert_eq!(u64::from(item.max_stack()), f["stack_size"].as_u64().unwrap(), "{}", item.name());
        assert!(item.icon_layer().is_some(), "{} has no icon", item.name());
        // A stack never grows past one.
        let mut inventory = Inventory::default();
        assert_eq!(inventory.add(item, 3), 0);
        assert_eq!(inventory.slots.iter().flatten().filter(|s| s.item == item && s.count == 1).count(), 3);
    }
    let e = &f["equippable"];
    assert_eq!(harness::SLOT.name(), e["slot"].as_str().unwrap());
    assert_eq!(harness::EQUIP_SOUND, e["equip_sound"].as_str().unwrap());
    assert_eq!(harness::SHEARING_SOUND, e["shearing_sound"].as_str().unwrap());
    assert_eq!(harness::ALLOWED_ENTITIES, strings(&e["allowed_entities"]));
    assert_eq!(harness::EQUIP_ON_INTERACT, e["equip_on_interact"].as_bool().unwrap());
    assert_eq!(harness::CAN_BE_SHEARED, e["can_be_sheared"].as_bool().unwrap());
    assert!(harness::can_equip("minecraft:happy_ghast"));
    assert!(!harness::can_equip("player") && !harness::can_equip("ghast") && !harness::can_equip("horse"));
}

#[test]
fn a_player_cannot_wear_a_harness() {
    // `Equippable.swapWithEquipmentSlot` fails for entities outside
    // `can_equip_harness`, so the stack stays in the hand.
    let red = harness::harness(DyeColor::Red);
    let mut inventory = Inventory::with_hotbar(&[red]);
    assert!(!inventory.equip(0));
    assert!(!inventory.equip_in_mode(0, true));
    assert_eq!(inventory.get(0), Some(Stack::new(red, 1)));
    assert!(inventory.armor.iter().all(Option::is_none));
}

#[test]
fn every_harness_recipe_is_registered_and_matches_java() {
    let f = fixture();
    let keys: BTreeSet<&str> = f["recipes"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys.len(), 32);
    let recipes: Value = serde_json::from_str(compare::FIXTURE).unwrap();
    for (kind, group) in [("crafting_shaped", "harness"), ("crafting_shapeless", "harness_dye")] {
        let ids: BTreeSet<&str> = recipes["recipes"][kind]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .filter(|id| id.ends_with("_harness"))
            .collect();
        assert_eq!(ids.len(), 16, "{kind}");
        for id in ids {
            assert!(keys.contains(id), "{id}");
            assert_eq!(f["recipes"][id]["type"], kind, "{id}");
            assert_eq!(f["recipes"][id]["group"], group, "{id}");
        }
    }
    // Both ways: every Java harness recipe is compared and crafts its grids;
    // the game has no crafting recipe Java lacks.
    let section = compare::compare_crafting(&compare::Reference::load());
    let unregistered: Vec<&String> = section.unregistered.keys().filter(|k| k.contains("harness")).collect();
    assert!(unregistered.is_empty(), "harness recipes still unregistered: {unregistered:?}");
    assert!(section.failures.is_empty(), "{}", section.failures.join("\n"));
    let ours = voxelcraft::crafting::recipes().iter().filter(|r| harness::colour(r.result.item).is_some()).count();
    assert_eq!(ours, 32);
}

fn grid(size: usize, cells: &[(usize, Item)]) -> Grid {
    let mut g = Grid::new(size);
    for &(i, item) in cells {
        g.cells[i] = Some(Stack::new(item, 1));
    }
    g
}

#[test]
fn harnesses_craft_from_leather_glass_and_wool_at_a_crafting_table() {
    let (leather, glass) = (Item::LEATHER, Item::from_block(Block::GLASS));
    for c in DyeColor::ALL {
        let wool = Item::from_block(Block::wool(c));
        // "LLL" over "G#G", in either pair of rows.
        for top in [0, 3] {
            let mut cells: Vec<(usize, Item)> = (0..3).map(|x| (top + x, leather)).collect();
            cells.extend([(top + 3, glass), (top + 4, wool), (top + 5, glass)]);
            let mut g = grid(3, &cells);
            assert_eq!(g.result(), Some(Stack::new(harness::harness(c), 1)), "{c:?}");
            g.consume();
            assert!(g.cells.iter().all(Option::is_none), "crafting spends every ingredient");
        }
    }
    // Upside down or with a pane of the wrong block, nothing.
    let wool = Item::from_block(Block::wool(DyeColor::Red));
    let flipped = grid(3, &[(0, glass), (1, wool), (2, glass), (3, leather), (4, leather), (5, leather)]);
    assert_eq!(flipped.result(), None);
    let stone = Item::from_block(Block::STONE);
    let wrong = grid(3, &[(0, leather), (1, leather), (2, leather), (3, stone), (4, wool), (5, stone)]);
    assert_eq!(wrong.result(), None);
}

#[test]
fn any_other_harness_and_a_dye_make_that_colour_in_the_inventory_grid() {
    for target in DyeColor::ALL {
        for from in DyeColor::ALL {
            let g = grid(2, &[(0, harness::harness(from)), (3, target.dye())]);
            let expected = (from != target).then(|| Stack::new(harness::harness(target), 1));
            // Java's `dye_<colour>_harness` leaves out the result's colour.
            assert_eq!(g.result(), expected, "{from:?} + {target:?} dye");
        }
    }
    let (blue, red_dye) = (harness::harness(DyeColor::Blue), DyeColor::Red.dye());
    assert_eq!(grid(2, &[(0, blue), (1, blue), (2, red_dye)]).result(), None, "two harnesses");
    assert_eq!(grid(2, &[(0, blue), (1, red_dye), (2, red_dye)]).result(), None, "two dyes");
}

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

#[test]
fn the_cli_crafts_and_redyes_harnesses_on_the_shared_path() {
    let mut w = world();
    let mut e = Entities::new(3);
    let table = IVec3::new(4, 140, 4);
    w.set_block(table, Block::CRAFTING_TABLE);
    let mut a = Agent::new(table.as_dvec3() + DVec3::new(0.5, 1.0, 0.5));
    // `look` takes VoxelCraft degrees: a negative pitch looks down at the table.
    a.execute(Command::parse("look 0 -90").unwrap(), &mut w, &mut e, &[]).unwrap();
    a.inventory.slots[0] = Some(Stack::new(Item::LEATHER, 3));
    a.inventory.slots[1] = Some(Stack::new(Item::from_block(Block::GLASS), 2));
    a.inventory.slots[2] = Some(Stack::new(Item::from_block(Block::wool(DyeColor::Lime)), 1));
    a.execute(Command::parse("craft lime_harness").unwrap(), &mut w, &mut e, &[]).unwrap();
    let lime = harness::harness(DyeColor::Lime);
    assert_eq!(a.inventory.slots.iter().flatten().copied().collect::<Vec<_>>(), [Stack::new(lime, 1)]);
    // Re-dyeing fits the inventory's 2x2 grid; no table needed.
    a.inventory.slots[5] = Some(Stack::new(DyeColor::Black.dye(), 2));
    a.execute(Command::parse("look 0 90").unwrap(), &mut w, &mut e, &[]).unwrap();
    a.execute(Command::Craft(harness::harness(DyeColor::Black)), &mut w, &mut e, &[]).unwrap();
    let left: Vec<Stack> = a.inventory.slots.iter().flatten().copied().collect();
    assert!(left.contains(&Stack::new(harness::harness(DyeColor::Black), 1)), "{left:?}");
    assert!(left.contains(&Stack::new(DyeColor::Black.dye(), 1)), "{left:?}");
    assert!(!left.iter().any(|s| s.item == lime), "{left:?}");
    // The same colour again is not a recipe.
    let before = a.inventory.clone();
    assert!(a.execute(Command::Craft(harness::harness(DyeColor::Black)), &mut w, &mut e, &[]).is_err());
    assert_eq!(a.inventory, before);
}

#[test]
fn harness_stacks_save_and_load() {
    let mut inventory = Inventory::default();
    for (slot, item) in harness::all().enumerate() {
        inventory.slots[slot] = Some(Stack::new(item, 1));
        let text = stack_to_string(Some(Stack::new(item, 1)));
        assert_eq!(text, format!("{}:1", item.0));
        assert_eq!(stack_from_str(&text), Some(Some(Stack::new(item, 1))));
    }
    let loaded = Inventory::deserialize(&inventory.serialize()).unwrap();
    assert_eq!(loaded, inventory);
    // The ids are the reserved 1585..=1600, white first.
    assert_eq!(harness::harness(DyeColor::White), Item(1585));
    assert_eq!(harness::harness(DyeColor::Black), Item(1600));
}
