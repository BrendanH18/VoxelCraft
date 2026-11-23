//! v0.17 Track C1: the crafter against Java 26.3 (`CrafterBlock`,
//! `CrafterBlockEntity`, `CrafterMenu`), with the facts in
//! `tests/fixtures/java/26.3/crafter.json` written by
//! `tools/extract_java_crafter_fixtures.py`. See
//! `docs/verification/2026-11-23-v0.17-crafter.md`.
use std::sync::Arc;

use glam::{DVec3, IVec3, Vec3};
use serde_json::Value;
use voxelcraft::{
    agent::{Agent, Command},
    crafting::Grid,
    entity::Entities,
    inventory::Stack,
    item::Item,
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        crafter::{self, Cue, State},
        redstone_blocks as r,
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/crafter.json")).unwrap()
}

const P: IVec3 = IVec3::new(8, 150, 8);

fn world() -> World {
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
    for _ in 0..10_000 {
        world.update(DVec3::new(8.0, 150.0, 8.0));
        if world.is_loaded(P) && world.is_block_ticking(P) {
            return world;
        }
        std::thread::yield_now();
    }
    panic!("world failed to load");
}

fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_redstone();
    }
}

fn dir(name: &str) -> IVec3 {
    match name {
        "down" => IVec3::NEG_Y,
        "up" => IVec3::Y,
        "north" => IVec3::NEG_Z,
        "south" => IVec3::Z,
        "west" => IVec3::NEG_X,
        "east" => IVec3::X,
        _ => panic!("not a direction: {name}"),
    }
}

/// An idle crafter whose front faces `front` (top up, or east when vertical).
fn crafter_facing(front: IVec3) -> Block {
    let top = if front.y == 0 { IVec3::Y } else { IVec3::X };
    crafter::encode(State {
        orientation: crafter::orientation_of(front, top).unwrap(),
        triggered: false,
        crafting: false,
    })
}

fn state(w: &World, p: IVec3) -> State {
    crafter::decode(w.get_block(p).unwrap()).expect("a crafter")
}

fn planks(n: u8) -> Option<Stack> {
    Some(Stack::new(Block::PLANKS, n))
}

/// Puts `stacks` into the crafter's slots in order.
fn fill(w: &mut World, p: IVec3, stacks: &[(usize, Option<Stack>)]) {
    for &(i, s) in stacks {
        assert!(w.set_chest_slot(p, i, s));
    }
}

/// Item entities the queued ejections became, in order.
fn ejected(w: &mut World) -> Vec<Stack> {
    let mut entities = Entities::new(1);
    w.tick_automation_entities(&mut entities);
    entities.items.iter().map(|e| e.stack).collect()
}

#[test]
fn constants_states_and_tables_match_the_java_fixture() {
    let f = fixture();
    let block = &f["block"];
    assert_eq!(block["constants"]["CRAFTING_TICK_DELAY"], crafter::CRAFTING_TICK_DELAY);
    assert_eq!(block["neighbor_changed_delay"], crafter::CRAFTING_TICK_DELAY);
    assert_eq!(block["placed_by_delay"], crafter::CRAFTING_TICK_DELAY);
    assert_eq!(block["constants"]["MAX_CRAFTING_TICKS"], crafter::MAX_CRAFTING_TICKS);
    assert_eq!(block["crafting_ticks"], crafter::MAX_CRAFTING_TICKS);
    assert_eq!(block["eject_speed"], crafter::EJECT_SPEED);
    assert_eq!(block["eject_offset"], crafter::EJECT_OFFSET);
    for key in ["eject_into_container", "eject_one_at_a_time_into_crafters", "remainders_ejected", "analog_output"] {
        assert_eq!(block[key], true, "{key}");
    }
    assert_eq!(block["neighbor_changed_signal"], "hasNeighborSignal");
    assert_eq!(f["block_entity"]["constants"]["CONTAINER_SIZE"], crafter::SLOTS);
    assert_eq!(f["menu"]["constants"]["SLOT_COUNT"], crafter::SLOTS);
    for key in [
        "set_slot_state_requires_empty_slot",
        "can_place_checks_later_smaller_stacks",
        "signal_counts_filled_or_disabled",
    ] {
        assert_eq!(f["block_entity"][key], true, "{key}");
    }
    assert_eq!(f["slot_state_packet_calls"], "CrafterBlockEntity");
    assert_eq!(f["menu"]["slot_may_place_checks_disabled"], true);
    // Twelve orientations, `triggered` and `crafting`, in Java's order.
    let properties: Vec<&str> = block["properties"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(properties, ["orientation", "triggered", "crafting"]);
    let orientations = f["orientations"].as_array().unwrap();
    assert_eq!(crafter::STATES as usize, orientations.len() * 4);
    for (i, o) in orientations.iter().enumerate() {
        assert_eq!(crafter::ORIENTATIONS[i], o["name"].as_str().unwrap());
        let want = (dir(o["front"].as_str().unwrap()), dir(o["top"].as_str().unwrap()));
        assert_eq!(crafter::front_and_top(i as u8), want, "{}", o["name"]);
    }
    assert_eq!(crafter::ORIENTATIONS[crafter::NORTH_UP as usize], "north_up");
    for id in crafter::FIRST..=crafter::LAST {
        let b = Block(id);
        assert_eq!(b.name(), "crafter");
        assert_eq!(b.base(), crafter::CRAFTER);
        // `blocks/crafter`: itself (when it survives an explosion).
        assert_eq!(b.drop(), Some(Item::from_block(crafter::CRAFTER)));
        assert_eq!(b.hardness(), f["strength"]["hardness"].as_f64().unwrap() as f32);
        assert_eq!(b.blast_resistance(), f["strength"]["blast_resistance"].as_f64().unwrap() as f32);
        assert!(b.is_opaque() && r::conductor(b), "a full, conducting cube");
    }
    assert_eq!(f["strength"]["requires_correct_tool"], false);
    assert!(voxelcraft::mining::can_harvest(crafter::CRAFTER, None));
    assert!(f["tags"]["block"].as_array().unwrap().iter().any(|t| t == "mineable/pickaxe"));
    assert_eq!(crafter::CRAFTER.best_tool(), Some(voxelcraft::item::ToolKind::Pickaxe));
    assert_eq!(f["loot_table"]["pools"][0]["entries"][0]["name"], "minecraft:crafter");
    assert_eq!(f["sounds"]["craft"], "block.crafter.craft");
    assert_eq!(f["sounds"]["fail"], "block.crafter.fail");
}

#[test]
fn the_recipe_matches_java() {
    let recipe = &fixture()["recipe"];
    let item = |id: &str| match id {
        "minecraft:iron_ingot" => Item::IRON_INGOT,
        "minecraft:crafting_table" => Item::from_block(Block::CRAFTING_TABLE),
        "minecraft:dropper" => Item::from_block(r::DROPPER),
        "minecraft:redstone" => Item::REDSTONE,
        other => panic!("unexpected ingredient {other}"),
    };
    let mut grid = Grid::new(3);
    for (y, row) in recipe["pattern"].as_array().unwrap().iter().enumerate() {
        for (x, c) in row.as_str().unwrap().chars().enumerate() {
            let id = recipe["key"][c.to_string()].as_str().unwrap();
            grid.cells[y * 3 + x] = Some(Stack::new(item(id), 1));
        }
    }
    assert_eq!(grid.result(), Some(Stack::new(Item::from_block(crafter::CRAFTER), 1)));
}

#[test]
fn placement_follows_java_get_state_for_placement() {
    let f = fixture();
    let tops = &f["block"]["placement_top"];
    // Looking toward `-front` puts the front toward the player.
    for (front, look) in [
        ("down", Vec3::new(0.1, 0.95, 0.2)),
        ("up", Vec3::new(0.1, -0.95, 0.2)),
        ("north", Vec3::new(0.1, 0.2, 0.95)),
        ("south", Vec3::new(0.1, 0.2, -0.95)),
        ("west", Vec3::new(0.95, 0.2, 0.1)),
        ("east", Vec3::new(-0.95, 0.2, 0.1)),
    ] {
        for horizontal in Facing::ALL {
            for powered in [false, true] {
                let b = crafter::placed(crafter::CRAFTER, look, horizontal, powered).unwrap();
                let s = crafter::decode(b).unwrap();
                let (got_front, got_top) = crafter::front_and_top(s.orientation);
                assert_eq!(got_front, dir(front));
                let want_top = match tops[front].as_str().unwrap() {
                    "up" => IVec3::Y,
                    "horizontal" => horizontal.offset(),
                    "horizontal_opposite" => horizontal.opposite().offset(),
                    other => panic!("unknown top rule {other}"),
                };
                assert_eq!(got_top, want_top, "front {front} facing {horizontal:?}");
                assert_eq!(s.triggered, powered);
                assert!(!s.crafting);
            }
        }
    }
}

/// Every special crafting recipe type of Java 26.3 and whether the shared
/// matcher (and so the crafter, which accepts every crafting recipe) makes it.
const SPECIAL: &[(&str, bool, &str)] = &[
    ("crafting_dye", true, "leather armor, leather horse armor and wolf armor dyeing"),
    ("crafting_imbue", true, "tipped arrows"),
    ("crafting_transmute", true, "bundle and shulker box dyeing, map cloning"),
    ("crafting_special_bannerduplicate", true, "banner copying"),
    ("crafting_special_bookcloning", true, "written book copying"),
    ("crafting_special_firework_rocket", true, "rockets"),
    ("crafting_special_firework_star", true, "stars"),
    ("crafting_special_firework_star_fade", true, "star fades"),
    ("crafting_special_mapextending", true, "map zooming out (needs the world's maps)"),
    ("crafting_special_shielddecoration", true, "shield banners"),
    ("crafting_special_repairitem", false, "gap: the matcher has no item-repair recipe"),
    ("crafting_decorated_pot", false, "gap: only the plain brick pot; sherd faces aren't matched"),
];

#[test]
fn special_recipe_types_are_all_accounted_for() {
    let f = fixture();
    let types: std::collections::BTreeSet<&str> =
        f["special_recipes"].as_object().unwrap().values().map(|v| v.as_str().unwrap()).collect();
    let listed: std::collections::BTreeSet<&str> = SPECIAL.iter().map(|(t, ..)| *t).collect();
    assert_eq!(types, listed, "a new special recipe type needs a crafter support decision");
    // The documented gap: two damaged pickaxes don't combine.
    let mut grid = Grid::new(3);
    let mut worn = Stack::new(Item::tool(voxelcraft::item::ToolKind::Pickaxe, voxelcraft::item::Tier::Wood), 1);
    worn.damage = 30;
    grid.cells[0] = Some(worn);
    grid.cells[1] = Some(worn);
    assert_eq!(grid.result(), None, "RepairItemRecipe is listed as a gap");
}

#[test]
fn a_rising_edge_crafts_four_ticks_later_and_crafting_lasts_six_ticks() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::X));
    fill(&mut w, P, &[(0, planks(2)), (3, planks(1))]);
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    let mut seen = 0;
    while !state(&w, P).triggered {
        ticks(&mut w, 1);
        seen += 1;
        assert!(seen < 4, "the neighbour update arrives on the next tick");
    }
    for _ in 1..crafter::CRAFTING_TICK_DELAY {
        ticks(&mut w, 1);
        assert_eq!(w.chest_slot(P, 0), planks(2), "nothing crafts before the delay");
    }
    ticks(&mut w, 1);
    assert_eq!(w.chest_slot(P, 0), planks(1));
    assert_eq!(w.chest_slot(P, 3), None);
    let s = state(&w, P);
    assert!(s.crafting && s.triggered);
    assert_eq!(w.take_crafter_events(), vec![crafter::Event { pos: P, cue: Cue::Craft }]);
    assert_eq!(ejected(&mut w), vec![Stack::new(Item::STICK, 4)]);
    // `serverTick` counts 6 down from the craft's tick: off on the fifth after it.
    for _ in 1..crafter::MAX_CRAFTING_TICKS - 1 {
        ticks(&mut w, 1);
        assert!(state(&w, P).crafting);
    }
    ticks(&mut w, 1);
    assert!(!state(&w, P).crafting);
    assert!(state(&w, P).triggered);
    // Holding the power crafts nothing more.
    ticks(&mut w, 20);
    assert_eq!(w.chest_slot(P, 0), planks(1));
    // A falling edge clears `triggered`; the next rising edge crafts again.
    w.set_block(P - IVec3::X, Block::AIR);
    ticks(&mut w, 2);
    assert!(!state(&w, P).triggered);
    // A plank beside dirt makes nothing: the craft fails and nothing is used.
    fill(&mut w, P, &[(4, Some(Stack::new(Block::DIRT, 1)))]);
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 1 + crafter::CRAFTING_TICK_DELAY as usize);
    assert_eq!(w.chest_slot(P, 0), planks(1));
    assert_eq!(w.chest_slot(P, 4), Some(Stack::new(Block::DIRT, 1)));
    assert_eq!(w.take_crafter_events(), vec![crafter::Event { pos: P, cue: Cue::Fail }]);
    assert!(!state(&w, P).crafting, "a failed craft doesn't set `crafting`");
    assert!(ejected(&mut w).is_empty());
}

#[test]
fn a_crafter_placed_powered_is_triggered_and_crafts_after_the_delay() {
    let mut w = world();
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    let look = Vec3::new(-0.95, 0.0, 0.1);
    let placed = crafter::placed(crafter::CRAFTER, look, Facing::West, w.redstone_power(P) > 0).unwrap();
    assert!(crafter::decode(placed).unwrap().triggered);
    w.set_block(P, placed);
    fill(&mut w, P, &[(0, planks(1)), (3, planks(1))]);
    ticks(&mut w, crafter::CRAFTING_TICK_DELAY as usize);
    assert_eq!(w.chest_slot(P, 0), None, "`setPlacedBy` scheduled the craft");
    assert_eq!(ejected(&mut w), vec![Stack::new(Item::STICK, 4)]);
}

#[test]
fn remainders_leave_after_the_result_and_every_slot_loses_one() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::Z));
    let milk = Some(Stack::new(Item::MILK_BUCKET, 1));
    let sugar = Some(Stack::new(Item::SUGAR, 2));
    let wheat = Some(Stack::new(Item::WHEAT, 1));
    fill(
        &mut w,
        P,
        &[
            (0, milk),
            (1, milk),
            (2, milk),
            (3, sugar),
            (4, Some(Stack::new(Item::EGG, 3))),
            (5, sugar),
            (6, wheat),
            (7, wheat),
            (8, wheat),
        ],
    );
    w.set_block(P + IVec3::NEG_Z, r::REDSTONE_BLOCK);
    ticks(&mut w, 1 + crafter::CRAFTING_TICK_DELAY as usize);
    let bucket = Stack::new(Item::BUCKET, 1);
    assert_eq!(ejected(&mut w), vec![Stack::new(Item::CAKE, 1), bucket, bucket, bucket]);
    for i in [0, 1, 2, 6, 7, 8] {
        assert_eq!(w.chest_slot(P, i), None, "slot {i}: the bucket left, it didn't stay");
    }
    assert_eq!(w.chest_slot(P, 3), Some(Stack::new(Item::SUGAR, 1)));
    assert_eq!(w.chest_slot(P, 4), Some(Stack::new(Item::EGG, 2)));
}

#[test]
fn results_go_into_a_container_in_front_and_one_at_a_time_into_a_crafter() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::X));
    w.set_block(P + IVec3::X, Block::CHEST);
    fill(&mut w, P, &[(0, planks(1)), (3, planks(1))]);
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 1 + crafter::CRAFTING_TICK_DELAY as usize);
    assert_eq!(w.chest_slot(P + IVec3::X, 0), Some(Stack::new(Item::STICK, 4)));
    assert!(ejected(&mut w).is_empty());
    assert!(w.take_crafter_events().is_empty(), "the sound and smoke go with items left in the world");

    // Into another crafter: one stick per call, so its slot rule spreads them.
    let mut w = world();
    let q = P + IVec3::X;
    w.set_block(P, crafter_facing(IVec3::X));
    w.set_block(q, crafter_facing(IVec3::X));
    assert!(w.set_crafter_slot_enabled(q, 2, false));
    fill(&mut w, P, &[(0, planks(1)), (3, planks(1))]);
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 1 + crafter::CRAFTING_TICK_DELAY as usize);
    let stick = Some(Stack::new(Item::STICK, 1));
    let got: Vec<_> = (0..crafter::SLOTS).map(|i| w.chest_slot(q, i)).collect();
    assert_eq!(got, [stick, stick, None, stick, stick, None, None, None, None]);
}

#[test]
fn hoppers_fill_enabled_slots_smallest_stack_first() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::X));
    for slot in [1, 4, 5, 6, 7, 8] {
        assert!(w.set_crafter_slot_enabled(P, slot, false));
    }
    // Enabled: 0, 2 and 3. Empty slots fill first, then the smallest stack.
    let above = P + IVec3::Y;
    w.set_block(above, r::hopper(4, false));
    assert!(w.set_chest_slot(above, 0, Some(Stack::new(Item::IRON_INGOT, 5))));
    assert!(w.set_chest_slot(above, 1, Some(Stack::new(Item::GOLD_INGOT, 1))));
    ticks(&mut w, 80);
    let iron = |n| Some(Stack::new(Item::IRON_INGOT, n));
    assert_eq!(w.chest_slot(P, 0), iron(2));
    assert_eq!(w.chest_slot(P, 2), iron(2));
    assert_eq!(w.chest_slot(P, 3), iron(1));
    // Gold has nowhere to go: every enabled slot holds iron, and the iron
    // stacks refuse it (`canMergeItems`).
    assert_eq!(w.chest_slot(above, 1), Some(Stack::new(Item::GOLD_INGOT, 1)));
    for slot in [1, 4, 5, 6, 7, 8] {
        assert_eq!(w.chest_slot(P, slot), None, "disabled slot {slot} took an item");
    }
    // `canPlaceItem` directly: slot 3 (1 iron) takes the next; slot 0 doesn't.
    assert!(!w.crafter_can_place(P, 0));
    assert!(w.crafter_can_place(P, 3));
    assert!(!w.crafter_can_place(P, 1));
}

#[test]
fn comparators_count_filled_or_disabled_slots_and_toggles_need_an_empty_slot() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::X));
    assert_eq!(w.container_signal(P), Some(0));
    fill(&mut w, P, &[(0, planks(64)), (4, planks(1))]);
    assert_eq!(w.container_signal(P), Some(2), "counts slots, not fullness");
    assert!(!w.set_crafter_slot_enabled(P, 0, false), "a filled slot can't be disabled");
    assert!(w.set_crafter_slot_enabled(P, 8, false));
    assert!(!w.set_crafter_slot_enabled(P, 8, false), "already disabled");
    assert_eq!(w.container_signal(P), Some(3));
    assert!(w.set_crafter_slot_enabled(P, 8, true));
    assert_eq!(w.container_signal(P), Some(2));
    // A comparator beside it reads the count.
    w.set_block(P + IVec3::X - IVec3::Y, Block::STONE);
    w.set_block(P + IVec3::X, r::comparator(Facing::East, false, false));
    ticks(&mut w, 6);
    assert_eq!(w.redstone_power(P + IVec3::X * 2), 2);
}

#[test]
fn slots_toggles_triggered_and_crafting_survive_save_and_reload() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::X));
    fill(&mut w, P, &[(0, planks(1)), (3, planks(1))]);
    assert!(w.set_crafter_slot_enabled(P, 7, false));
    w.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    // Save between the rising edge and the craft.
    ticks(&mut w, 2);
    assert!(state(&w, P).triggered);
    let (chests, crafters, redstone) = (w.chests_to_string(), w.crafters_to_string(), w.redstone_to_string());
    let block = w.get_block(P).unwrap();

    let mut v = world();
    v.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    v.set_block(P, block);
    v.load_chests(&chests);
    v.load_crafters(&crafters);
    v.load_redstone(&redstone);
    assert!(state(&v, P).triggered);
    assert!(v.crafter_slot_disabled(P, 7));
    assert_eq!(v.chest_slot(P, 0), planks(1));
    ticks(&mut v, crafter::CRAFTING_TICK_DELAY as usize);
    assert_eq!(v.chest_slot(P, 0), None, "the saved craft tick still fires");
    assert!(state(&v, P).crafting);
    // Mid-countdown: `crafting_ticks_remaining` comes back too.
    let left = v.crafter(P).unwrap().crafting_ticks;
    assert!((1..crafter::MAX_CRAFTING_TICKS).contains(&left));
    let (crafters, redstone) = (v.crafters_to_string(), v.redstone_to_string());
    let block = v.get_block(P).unwrap();
    let mut u = world();
    u.set_block(P - IVec3::X, r::REDSTONE_BLOCK);
    u.set_block(P, block);
    u.load_crafters(&crafters);
    u.load_redstone(&redstone);
    assert!(u.crafter_slot_disabled(P, 7));
    assert_eq!(u.crafter(P).unwrap().crafting_ticks, left);
    ticks(&mut u, left as usize - 1);
    assert!(state(&u, P).crafting);
    ticks(&mut u, 1);
    assert!(!state(&u, P).crafting, "the countdown resumes after loading");
}

#[test]
fn breaking_a_crafter_spills_its_slots_and_drops_itself() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::X));
    fill(&mut w, P, &[(0, planks(3)), (8, Some(Stack::new(Item::STICK, 2)))]);
    assert!(w.set_crafter_slot_enabled(P, 4, false));
    w.drops.clear();
    w.set_block(P, Block::AIR);
    let spilled: Vec<Stack> = w.drops.iter().map(|&(_, s)| s).collect();
    assert_eq!(spilled, vec![Stack::new(Block::PLANKS, 3), Stack::new(Item::STICK, 2)]);
    assert!(w.crafter(P).is_none());
    w.set_block(P, crafter_facing(IVec3::X));
    assert!(!w.crafter_slot_disabled(P, 4), "a new crafter starts with every slot enabled");
}

#[test]
fn cli_agents_toggle_slots_and_cannot_put_into_disabled_ones() {
    let mut w = world();
    let mut entities = Entities::new(1);
    let pos = IVec3::new(11, 151, 8);
    w.set_block(pos, crafter_facing(IVec3::NEG_X));
    let mut a = Agent::new(DVec3::new(8.5, 150.0, 8.5));
    a.inventory.add(Block::PLANKS, 4);
    a.execute(Command::parse("crafter toggle 3").unwrap(), &mut w, &mut entities, &[]).unwrap();
    assert!(w.crafter_slot_disabled(pos, 3));
    assert!(a.execute(Command::Chest(false, 3), &mut w, &mut entities, &[]).is_err());
    a.execute(Command::Chest(false, 0), &mut w, &mut entities, &[]).unwrap();
    assert_eq!(w.chest_slot(pos, 0), planks(4));
    assert!(
        a.execute(Command::parse("crafter toggle 0").unwrap(), &mut w, &mut entities, &[]).is_err(),
        "a filled slot can't be toggled"
    );
    let seen = a.observe(&w, 0);
    assert_eq!(seen["target"]["crafter"]["disabled"], serde_json::json!([3]));
    assert_eq!(seen["target"]["crafter"]["triggered"], false);
    a.execute(Command::parse("crafter toggle 3").unwrap(), &mut w, &mut entities, &[]).unwrap();
    assert!(!w.crafter_slot_disabled(pos, 3));
    assert!(Command::parse("crafter toggle 9").is_err());
}

#[test]
fn a_firework_rocket_is_a_special_recipe_the_crafter_makes() {
    let mut w = world();
    w.set_block(P, crafter_facing(IVec3::Y));
    fill(&mut w, P, &[(0, Some(Stack::new(Item::PAPER, 1))), (1, Some(Stack::new(Item::GUNPOWDER, 1)))]);
    let preview = w.crafter_result(P).expect("the screen previews the rocket");
    assert_eq!(preview.item, voxelcraft::content::fireworks::FIREWORK_ROCKET);
    w.set_block(P - IVec3::Y, r::REDSTONE_BLOCK);
    ticks(&mut w, 1 + crafter::CRAFTING_TICK_DELAY as usize);
    let out = ejected(&mut w);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].item, voxelcraft::content::fireworks::FIREWORK_ROCKET);
    assert_eq!(out[0].count, preview.count);
}

#[test]
fn idle_crafters_do_no_per_tick_work() {
    let mut w = world();
    for i in 0..64 {
        let p = IVec3::new(2 + i % 8, 148, 2 + i / 8);
        w.set_block(p, crafter_facing(IVec3::Y));
    }
    ticks(&mut w, 3);
    let before = w.crafters_to_string();
    ticks(&mut w, 40);
    assert_eq!(w.crafters_to_string(), before);
    assert!(w.take_crafter_events().is_empty());
    let saved: Value = serde_json::from_str(&before).unwrap();
    assert_eq!(saved["crafters"], serde_json::json!([]), "idle default crafters aren't even saved");
}
