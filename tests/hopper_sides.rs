//! v0.17 Track C2: sided container access against
//! `fixtures/java/26.3/dispensing.json` (`WorldlyContainer.getSlotsForFace`,
//! `canPlaceItem`/`canPlaceItemThroughFace` and `canTakeItem`/
//! `canTakeItemThroughFace` of every 26.3 container block, read from the
//! jar's own block entities), and hoppers and droppers moving items
//! through those faces in a world, with Java's pickup box.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    entity::{Entities, item::ItemEntity},
    inventory::Stack,
    item::Item,
    java_registry,
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        container_sides::{self as sides, Kind},
        redstone_blocks as r,
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/dispensing.json")).unwrap()
}

const FACES: [(&str, IVec3); 6] = [
    ("down", IVec3::NEG_Y),
    ("up", IVec3::Y),
    ("north", IVec3::NEG_Z),
    ("south", IVec3::Z),
    ("west", IVec3::NEG_X),
    ("east", IVec3::X),
];

/// The VoxelCraft container kind for each Java container class, and what
/// isn't modelled.
fn kind_of(class: &str, size: u64) -> Result<Kind, &'static str> {
    Ok(match class {
        "ChestBlockEntity"
        | "TrappedChestBlockEntity"
        | "BarrelBlockEntity"
        | "DispenserBlockEntity"
        | "DropperBlockEntity"
        | "HopperBlockEntity" => Kind::Plain { slots: size as u8 },
        "ShulkerBoxBlockEntity" => Kind::ShulkerBox,
        "FurnaceBlockEntity" | "SmokerBlockEntity" | "BlastFurnaceBlockEntity" => Kind::Furnace,
        "BrewingStandBlockEntity" => Kind::BrewingStand,
        "InputContainer" => Kind::ComposterInput,
        "EmptyContainer" => Kind::ComposterFull,
        "OutputContainer" => Kind::ComposterOutput,
        "JukeboxBlockEntity" => Kind::Jukebox,
        "ChiseledBookShelfBlockEntity" => Kind::ChiseledBookshelf,
        "ShelfBlockEntity" => Kind::Shelf,
        "CrafterBlockEntity" => return Err("Track C1 owns the crafter's faces"),
        "DecoratedPotBlockEntity" => return Err("decorated pots hold no item yet"),
        _ => panic!("unknown container {class}"),
    })
}

fn probe(name: &str) -> Item {
    static NAMES: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let names =
        NAMES.get_or_init(|| serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap());
    java_registry::item(name, names["items"][name].as_str().unwrap()).unwrap_or_else(|| panic!("probe {name}"))
}

/// Every Java container class, every face: the same slots in the same
/// order, and the same probes accepted and given up through each.
#[test]
fn container_faces_match_java_both_ways() {
    let fx = fixture();
    let mut checked = 0;
    let mut skipped = Vec::new();
    for group in fx["containers"].as_array().unwrap() {
        let class = group["class"].as_str().unwrap();
        let kind = match kind_of(class, group["size"].as_u64().unwrap()) {
            Ok(kind) => kind,
            Err(why) => {
                skipped.push(format!("{class}: {why}"));
                continue;
            }
        };
        assert_eq!(sides::size(kind) as u64, group["size"].as_u64().unwrap(), "{class} size");
        assert_eq!(sides::worldly(kind), group["worldly"].as_bool().unwrap(), "{class} worldly");
        let level_bound = group.get("note").is_some();
        for (face, step) in FACES {
            let runs = group["faces"].get(face).unwrap_or_else(|| &group["faces"]["all"]);
            let java_slots: Vec<u8> = runs
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|run| run["slots"].as_array().unwrap().iter().map(|s| s.as_u64().unwrap() as u8))
                .collect();
            assert_eq!(sides::slots_for_face(kind, step), java_slots.as_slice(), "{class} {face} slots");
            for run in runs.as_array().unwrap() {
                let list = |key: &str| -> Vec<String> {
                    run[key].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
                };
                let (place, take) = (list("place"), list("take"));
                for slot in run["slots"].as_array().unwrap() {
                    let slot = slot.as_u64().unwrap() as usize;
                    for name in fx_probes() {
                        let item = probe(&name);
                        // The oracle's stand has no level, so it refuses
                        // every bottle and ingredient; Java's rule (from
                        // `canPlaceItem`) is checked below instead.
                        if !(level_bound && slot < 4) {
                            assert_eq!(
                                sides::can_place(kind, slot, item, None, Some(step)),
                                place.contains(&name),
                                "{class} {face} slot {slot} place {name}"
                            );
                        }
                        assert_eq!(
                            sides::can_take(kind, slot, item, step),
                            take.contains(&name),
                            "{class} {face} slot {slot} take {name}"
                        );
                    }
                }
            }
        }
        checked += 1;
    }
    println!("{checked} container rule sets match; skipped: {skipped:?}");
    assert_eq!(skipped.len(), 2, "{skipped:?}");
}

fn fx_probes() -> Vec<String> {
    // Every probe the oracle used appears in the hopper's "place" list.
    let fx = fixture();
    let hopper = fx["containers"].as_array().unwrap().iter().find(|g| g["class"] == "HopperBlockEntity").unwrap();
    hopper["faces"]["all"][0]["place"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

/// `BrewingStandBlockEntity.canPlaceItem`, which needs the level's recipes:
/// a potion or glass bottle into an empty bottle slot, a reagent as the
/// ingredient, blaze powder as fuel.
#[test]
fn brewing_stand_places_like_java() {
    let water = Item::potion(voxelcraft::potion::Potion::WATER);
    let bottle = Stack::new(water, 1);
    assert!(sides::can_place(Kind::BrewingStand, 0, water, None, Some(IVec3::X)));
    assert!(!sides::can_place(Kind::BrewingStand, 0, water, Some(&bottle), Some(IVec3::X)));
    assert!(sides::can_place(Kind::BrewingStand, 1, Item::GLASS_BOTTLE, None, Some(IVec3::NEG_Y)));
    assert!(sides::can_place(Kind::BrewingStand, 3, Item::NETHER_WART, None, Some(IVec3::Y)));
    assert!(!sides::can_place(Kind::BrewingStand, 3, Item::STICK, None, Some(IVec3::Y)));
    assert!(sides::can_place(Kind::BrewingStand, 4, Item::BLAZE_POWDER, None, Some(IVec3::X)));
}

/// `Hopper.SUCK_AABB` is what the hopper collects from.
#[test]
fn hopper_suck_box_is_javas() {
    let fx = fixture();
    let b: Vec<f64> = fx["hopper_suck_aabb"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(b, [0.0, 11.0 / 16.0, 0.0, 1.0, 2.0, 1.0]);
    // An item whose box tops out just under 11/16 stays; one reaching into
    // the box from beside the hopper is taken.
    for (pos, taken) in [
        (DVec3::new(4.5, 145.4374, 8.5), false),
        (DVec3::new(4.5, 145.44, 8.5), true),
        (DVec3::new(3.9, 146.5, 8.5), true),
        (DVec3::new(3.87, 146.5, 8.5), false),
        (DVec3::new(4.5, 147.0, 8.5), false),
    ] {
        let mut w = world();
        let mut e = Entities::new(1);
        w.set_block(HOPPER, r::hopper(4, false));
        e.items.push(ItemEntity::new(
            Stack::new(Item::STICK, 3),
            pos,
            DVec3::ZERO,
            0.0,
            &mut voxelcraft::entity::Rng::new(1),
        ));
        e.items.last_mut().unwrap().pos = pos;
        for _ in 0..3 {
            w.tick_redstone();
            w.tick_automation_entities(&mut e);
        }
        assert_eq!(w.chest(HOPPER).unwrap().slots[0].is_some(), taken, "{pos}");
    }
}

const HOPPER: IVec3 = IVec3::new(4, 145, 8);

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.receive_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)));
        }
    }
    w
}

fn run(w: &mut World, ticks: usize) {
    let mut e = Entities::new(1);
    for _ in 0..ticks {
        w.tick_redstone();
        w.tick_automation_entities(&mut e);
    }
}

/// A hopper under a furnace takes the output before an empty bucket in the
/// fuel slot (`SLOTS_FOR_DOWN = {2, 1}`), and leaves coal in the fuel slot
/// (`#furnace_fuel_bottom_takeable`).
#[test]
fn hoppers_under_furnaces_take_output_first_and_leave_fuel() {
    let mut w = world();
    let furnace = HOPPER + IVec3::Y;
    w.set_block(furnace, Block::FURNACE);
    w.set_block(HOPPER, r::hopper(4, false));
    w.set_block(HOPPER - IVec3::Y, Block::STONE);
    let f = w.furnace_mut(furnace).unwrap();
    f.fuel = Some(Stack::new(Item::BUCKET, 1));
    f.output = Some(Stack::new(Item::IRON_INGOT, 1));
    run(&mut w, 2);
    let slots = w.chest(HOPPER).unwrap().slots;
    assert_eq!(slots[0].map(|s| s.item), Some(Item::IRON_INGOT), "output first");
    run(&mut w, 9);
    assert_eq!(w.chest(HOPPER).unwrap().slots[1].map(|s| s.item), Some(Item::BUCKET), "then the bucket");

    let mut w = world();
    w.set_block(furnace, Block::FURNACE);
    w.set_block(HOPPER, r::hopper(4, false));
    w.furnace_mut(furnace).unwrap().fuel = Some(Stack::new(Item::COAL, 4));
    run(&mut w, 20);
    assert_eq!(w.chest(HOPPER).unwrap().slots[0], None, "coal stays in the fuel slot");
}

/// A dropper under a furnace pushes fuel in through the bottom face, and a
/// second bucket can't join a bucket already in the fuel slot.
#[test]
fn droppers_fuel_furnaces_from_below() {
    let mut w = world();
    let furnace = HOPPER + IVec3::Y;
    w.set_block(furnace, Block::FURNACE);
    w.set_block(HOPPER, r::dispenser(4, true));
    w.chest_mut(HOPPER).unwrap().slots[0] = Some(Stack::new(Item::COAL, 2));
    w.set_block(HOPPER - IVec3::Z, r::REDSTONE_BLOCK);
    run(&mut w, 6);
    assert_eq!(w.furnace(furnace).unwrap().fuel.map(|s| s.count), Some(1));

    let mut w = world();
    w.set_block(furnace, Block::FURNACE);
    w.furnace_mut(furnace).unwrap().fuel = Some(Stack::new(Item::BUCKET, 1));
    w.set_block(HOPPER, r::dispenser(4, true));
    w.chest_mut(HOPPER).unwrap().slots[0] = Some(Stack::new(Item::BUCKET, 1));
    w.set_block(HOPPER - IVec3::Z, r::REDSTONE_BLOCK);
    run(&mut w, 6);
    assert_eq!(w.furnace(furnace).unwrap().fuel.map(|s| s.count), Some(1));
    assert_eq!(w.chest(HOPPER).unwrap().slots[0].map(|s| s.count), Some(1), "the dropper keeps its bucket");
}

/// Java 26.3 shelves and chiseled bookshelves are hopper containers: a
/// shelf takes anything, a chiseled bookshelf one book a slot (and shows it).
#[test]
fn hoppers_fill_shelves_and_chiseled_bookshelves() {
    use voxelcraft::world::wood::{bookshelf, shelves};
    let mut w = world();
    let below = HOPPER - IVec3::Y;
    let shelf = shelves::palette_ids().map(Block).find(|&b| shelves::shelf(b).is_some()).unwrap();
    w.set_block(below, shelf);
    w.set_block(HOPPER, r::hopper(4, false));
    w.chest_mut(HOPPER).unwrap().slots[0] = Some(Stack::new(Item::STICK, 2));
    run(&mut w, 10);
    assert_eq!(w.wood_entities().shelf(below).unwrap()[0].map(|s| s.count), Some(2));

    let mut w = world();
    let case = bookshelf::state(voxelcraft::world::block::Facing::North, 0);
    w.set_block(below, case);
    w.set_block(HOPPER, r::hopper(4, false));
    w.chest_mut(HOPPER).unwrap().slots[0] = Some(Stack::new(Item::BOOK, 2));
    w.chest_mut(HOPPER).unwrap().slots[1] = Some(Stack::new(Item::STICK, 1));
    run(&mut w, 20);
    let books = w.wood_entities().bookshelf(below).unwrap().books;
    assert_eq!(books.iter().flatten().count(), 2, "one book in each of two slots");
    assert_eq!(bookshelf::slots(w.get_block(below).unwrap()), Some(0b11));
    assert_eq!(w.chest(HOPPER).unwrap().slots[1].map(|s| s.item), Some(Item::STICK), "no stick goes in");
}

/// A full block above stops item collection, unless it is a beehive.
#[test]
fn a_full_block_above_stops_collection() {
    for (above, taken) in [(Block::STONE, false), (Block::AIR, true)] {
        let mut w = world();
        let mut e = Entities::new(1);
        w.set_block(HOPPER, r::hopper(4, false));
        w.set_block(HOPPER + IVec3::Y, above);
        let pos = DVec3::new(4.5, 145.8, 8.5);
        e.items.push(ItemEntity::new(
            Stack::new(Item::STICK, 1),
            pos,
            DVec3::ZERO,
            0.0,
            &mut voxelcraft::entity::Rng::new(1),
        ));
        e.items.last_mut().unwrap().pos = pos;
        for _ in 0..3 {
            w.tick_redstone();
            w.tick_automation_entities(&mut e);
        }
        assert_eq!(w.chest(HOPPER).unwrap().slots[0].is_some(), taken, "{above:?}");
    }
}
