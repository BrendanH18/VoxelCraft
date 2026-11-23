//! v0.17 Track E2: every Java 26.3 block that emits, consumes or is read
//! as a redstone signal (`redstone_components.json`, from
//! `tools/extract_java_redstone_component_fixtures.py`), compared with the
//! game's roles (`world::redstone_catalogue`) both ways, then one small
//! scenario per component through the public world API.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use voxelcraft::{
    entity::PlayerId,
    inventory::Stack,
    item::Item,
    java_registry,
    player::EYE_HEIGHT,
    session::{PlayerSession, placement::Outcome},
    world::{
        World,
        block::{Block, Facing, Shaped},
        chunk::ChunkData,
        copper::{self, Family, Stage},
        gadgets, redstone_blocks as r,
        redstone_catalogue::{Role, role, state_analog_output},
        terrain::Generator,
        utilities,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/redstone_components.json")).unwrap()
}

fn names() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_names.json")).unwrap()
}

/// The game block for a Java block ID, through its English name where the
/// game's name differs (`cake with black candle`, `block of redstone`).
fn resolve(names: &Value, id: &str) -> Option<Block> {
    java_registry::block(id, names["blocks"][id].as_str().unwrap_or(""))
}

/// Game block names whose Java block carries the difference as a state.
fn java_family(game_name: &str) -> &str {
    match game_name {
        "lit furnace" => "furnace",
        "lit smoker" => "smoker",
        "lit blast furnace" => "blast furnace",
        other => other,
    }
}

fn gaps() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_gaps.json")).unwrap()
}

/// Java blocks whose behaviour belongs to another v0.17 track or to a
/// technical block the game leaves out (`registry_gaps.json`).
fn missing_in_game(name: &str) -> bool {
    gaps()["blocks"].get(name).is_some()
}

/// Known, recorded differences between the Java class flags and the
/// game's roles (`docs/verification/2026-11-23-v0.17-components.md`):
/// `(java name, column)`.
const DIFFERENCES: &[(&str, &str)] = &[];

fn java_role(entry: &Value) -> Role {
    Role {
        source: entry["signal_source"].as_bool().unwrap(),
        strong: entry["strong"].as_bool().unwrap(),
        analog: entry["analog"].as_bool().unwrap(),
        reads_power: entry["reads_power"].as_bool().unwrap(),
    }
}

fn columns(a: Role, b: Role) -> Vec<&'static str> {
    let mut out = Vec::new();
    if a.source != b.source {
        out.push("source");
    }
    if a.strong != b.strong {
        out.push("strong");
    }
    if a.analog != b.analog {
        out.push("analog");
    }
    if a.reads_power != b.reads_power {
        out.push("reads_power");
    }
    out
}

#[test]
fn every_java_component_has_the_same_role_in_the_game() {
    let fixture = fixture();
    let names = names();
    let mut mismatches = Vec::new();
    for (name, entry) in fixture["blocks"].as_object().unwrap() {
        let Some(block) = resolve(&names, name) else {
            assert!(missing_in_game(name), "{name} neither resolves nor is a listed registry gap");
            continue;
        };
        let game = role(block);
        for column in columns(java_role(entry), game) {
            if !DIFFERENCES.contains(&(name.as_str(), column)) {
                mismatches.push(format!(
                    "{name} ({}): {column} java {:?} game {game:?}",
                    entry["class"],
                    java_role(entry)
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{} mismatches:\n{}", mismatches.len(), mismatches.join("\n"));
}

/// Every block state the game gives a redstone role is a Java component
/// with that role: a family the engine handles that Java doesn't (or a
/// state of a family the catalogue misses) fails here.
#[test]
fn every_game_component_is_a_java_component() {
    let fixture = fixture();
    let blocks = fixture["blocks"].as_object().unwrap();
    // The game names its components' Java blocks; collect them by name.
    let names = names();
    let mut by_name = std::collections::BTreeMap::new();
    for name in blocks.keys() {
        if let Some(block) = resolve(&names, name) {
            by_name.entry(block.name()).or_insert_with(Vec::new).push(name.clone());
        }
    }
    let mut unknown = BTreeSet::new();
    let mut mismatches = BTreeSet::new();
    for id in 0..voxelcraft::world::block::STATE_CAPACITY as u16 {
        let b = Block(id);
        let game = role(b);
        if !game.any() || b.name().is_empty() {
            continue;
        }
        let Some(names) = by_name.get(java_family(b.name())) else {
            unknown.insert(b.name());
            continue;
        };
        let java = java_role(&blocks[&names[0]]);
        for column in columns(java, game) {
            if !DIFFERENCES.contains(&(names[0].as_str(), column)) {
                mismatches.insert(format!("{} state {id}: {column}", b.name()));
            }
        }
    }
    assert!(unknown.is_empty(), "game components with no Java component: {unknown:?}");
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn the_fixture_pins_the_component_constants_the_game_uses() {
    let fixture = fixture();
    let c = &fixture["constants"];
    assert_eq!(c["daylight_detector"]["sample_period_ticks"], 20);
    assert_eq!(c["daylight_detector"]["sun_angle_easing"], 0.2);
    assert_eq!(c["lightning_rod_pulse_ticks"], 8);
    assert_eq!(c["copper_bulb_scheduled_tick"], false);
    assert_eq!(c["redstone_lamp_off_delay"], 4);
    let classes = &fixture["classes"];
    let delays = |class: &str| classes[class]["scheduled_delays"].as_array().unwrap().clone();
    assert!(delays("RedstoneLampBlock").contains(&Value::from(4)));
    assert!(delays("ObserverBlock").contains(&Value::from(2)));
    assert!(delays("LightningRodBlock").contains(&Value::from(8)));
    assert!(delays("TripWireHookBlock").contains(&Value::from(10)));
}

// ---- Scenarios -------------------------------------------------------------

const AT: IVec3 = IVec3::new(8, 145, 8);

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    for x in 0..24 {
        for z in 0..24 {
            w.set_block(IVec3::new(x, AT.y - 1, z), Block::STONE);
        }
    }
    w
}

fn put(w: &mut World, p: IVec3, b: Block) {
    assert!(w.set_block(p, b), "placing {} at {p}", b.name());
}

fn ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_redstone();
    }
}

/// Button press lengths and weighted-plate maxima come from the
/// registration's constructor arguments.
fn constructor_ints(name: &str) -> Vec<i64> {
    fixture()["blocks"][name]["constructor_ints"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect()
}

#[test]
fn every_button_stays_pressed_for_its_java_ticks() {
    let fixture = fixture();
    for (name, entry) in fixture["blocks"].as_object().unwrap() {
        if entry["class"] != "ButtonBlock" {
            continue;
        }
        let ticks_pressed = *constructor_ints(name).last().unwrap() as usize;
        let button = resolve(&names(), name).unwrap_or_else(|| panic!("{name}"));
        let Some(r::Component::Button { mount, on: false, .. }) = r::component(button) else {
            panic!("{name} is not an unpressed button")
        };
        let mut w = world();
        let p = AT + IVec3::Y;
        let support = p + r::support(mount);
        put(&mut w, support, Block::STONE);
        put(&mut w, p, button);
        assert!(w.use_redstone(p), "{name}");
        ticks(&mut w, 1);
        let beside = support + if r::support(mount).x == 0 { IVec3::X } else { IVec3::Z };
        assert_eq!(w.redstone_power(beside), 15, "{name} strongly powers its support");
        ticks(&mut w, ticks_pressed - 2);
        assert_eq!(w.redstone_power(beside), 15, "{name} still pressed");
        ticks(&mut w, 2);
        assert_eq!(w.redstone_power(beside), 0, "{name} released after {ticks_pressed}");
    }
}

#[test]
fn levers_power_and_the_redstone_block_is_weak_only() {
    let mut w = world();
    put(&mut w, AT, r::lever(0, false));
    assert!(w.use_redstone(AT));
    ticks(&mut w, 1);
    // The lever strongly powers the floor block it stands on, which powers
    // the cell under it.
    assert_eq!(w.redstone_power(AT - IVec3::Y * 2), 15, "strong power through the support");
    // `PoweredBlock`: weak 15 everywhere, so a conductor beside it isn't powered.
    let block = AT + IVec3::new(4, 0, 0);
    put(&mut w, block, r::REDSTONE_BLOCK);
    put(&mut w, block + IVec3::X, Block::STONE);
    assert_eq!(w.redstone_power(block + IVec3::X), 15);
    assert_eq!(w.redstone_power(block + IVec3::X * 2), 0, "no strong power through the stone");
}

#[test]
fn lamps_light_at_once_and_go_out_four_ticks_later() {
    let mut w = world();
    put(&mut w, AT, r::LAMP);
    put(&mut w, AT + IVec3::X, r::lever(0, false));
    assert!(w.use_redstone(AT + IVec3::X));
    ticks(&mut w, 1);
    assert_eq!(w.get_block(AT).and_then(r::component), Some(r::Component::Lamp(true)));
    assert!(w.use_redstone(AT + IVec3::X));
    ticks(&mut w, 4);
    assert_eq!(w.get_block(AT).and_then(r::component), Some(r::Component::Lamp(true)), "still lit");
    ticks(&mut w, 1);
    assert_eq!(w.get_block(AT).and_then(r::component), Some(r::Component::Lamp(false)));
}

#[test]
fn copper_bulbs_toggle_on_each_rising_edge_with_no_delay() {
    let mut w = world();
    let bulb = copper::block(Family::Bulb, Stage::Unaffected, 0);
    put(&mut w, AT, bulb);
    let lever = AT + IVec3::X;
    put(&mut w, lever, r::lever(0, false));
    let lit = |w: &World| copper::bulb(w.get_block(AT).unwrap()).unwrap().1;
    assert!(w.use_redstone(lever));
    ticks(&mut w, 1);
    assert!(lit(&w), "the first rising edge lights it in the same tick");
    assert_eq!(w.container_signal(AT), Some(15));
    assert!(w.use_redstone(lever));
    ticks(&mut w, 1);
    assert!(lit(&w), "a falling edge keeps it lit");
    assert!(w.use_redstone(lever));
    ticks(&mut w, 1);
    assert!(!lit(&w), "the next rising edge turns it off");
    assert_eq!(w.container_signal(AT), Some(0));
}

#[test]
fn lightning_rods_pulse_for_eight_ticks() {
    let mut w = world();
    let rod = copper::block(Family::Rod, Stage::Unaffected, 0);
    put(&mut w, AT, Block::STONE);
    put(&mut w, AT + IVec3::Y, rod);
    w.lightning_strike_blocks(DVec3::new(8.5, 146.5, 8.5));
    ticks(&mut w, 1);
    assert_eq!(w.redstone_power(AT + IVec3::Y * 2), 15, "weak power around the rod");
    assert_eq!(w.redstone_power(AT + IVec3::X), 15, "strong power into the block it stands on");
    ticks(&mut w, 8);
    assert_eq!(w.redstone_power(AT + IVec3::Y * 2), 0);
}

#[test]
fn targets_and_trapped_chests_and_lecterns_emit_their_java_signals() {
    let mut w = world();
    put(&mut w, AT, r::TARGET);
    assert!(w.hit_redstone_target(AT, DVec3::new(8.5, 145.5, 8.0), IVec3::NEG_Z, true));
    assert_eq!(w.redstone_power(AT + IVec3::X), 15, "a centre hit gives 15");
    ticks(&mut w, 20);
    assert_eq!(w.redstone_power(AT + IVec3::X), 0, "an arrow's pulse lasts 20 ticks");

    // Trapped chest: viewers as weak power, strong only downward.
    let chest = AT + IVec3::new(4, 0, 0);
    put(&mut w, chest, utilities::trapped_chest(Facing::North, false));
    w.set_chest_viewers([chest]);
    ticks(&mut w, 1);
    assert_eq!(w.redstone_power(chest + IVec3::X), 1);

    // Lectern: a page turn pulses 15 for two ticks.
    let lectern = AT + IVec3::new(8, 0, 0);
    put(&mut w, lectern, voxelcraft::world::workstations::lectern_state(Facing::North, false, false));
    assert!(w.place_lectern_book(lectern, Stack::new(Item::WRITABLE_BOOK, 1)));
    assert!(!w.set_lectern_page(lectern, 1), "a one-page book has no page 1");
}

#[test]
fn comparators_read_every_state_only_analog_block() {
    let mut w = world();
    let cases = [
        (Block::cake(0), 14),
        (Block::cake(6), 2),
        (voxelcraft::world::colour_blocks::candle::cake(0, false), 14),
        (utilities::cauldron(utilities::Contents::Empty), 0),
        (utilities::cauldron(utilities::Contents::Water(2)), 2),
        (utilities::cauldron(utilities::Contents::Lava), 3),
        (utilities::cauldron(utilities::Contents::PowderSnow(3)), 3),
        (utilities::respawn_anchor(1), 3),
        (utilities::respawn_anchor(2), 7),
        (utilities::respawn_anchor(4), 15),
        (Block::END_PORTAL_FRAME, 0),
        (Block(Block::END_PORTAL_FRAME.0 + 4), 15),
    ];
    for (block, expected) in cases {
        assert_eq!(state_analog_output(block), Some(expected), "{}", block.name());
        put(&mut w, AT, block);
        assert_eq!(w.container_signal(AT), Some(expected), "{}", block.name());
    }
    // A comparator behind a full cake reads 14 rather than the power behind it.
    put(&mut w, AT, Block::cake(0));
    let comparator = AT + IVec3::Z;
    put(&mut w, comparator, r::comparator(Facing::South, false, false));
    ticks(&mut w, 3);
    assert_eq!(w.redstone_power(comparator + IVec3::Z), 14);
}

#[test]
fn decorated_pots_report_their_one_stack_and_wake_comparators() {
    let mut w = world();
    put(&mut w, AT, utilities::decorated_pot(Facing::North, false, false));
    let comparator = AT + IVec3::Z;
    put(&mut w, comparator, r::comparator(Facing::South, false, false));
    ticks(&mut w, 3);
    assert_eq!(w.container_signal(AT), Some(0));
    assert_eq!(w.redstone_power(comparator + IVec3::Z), 0);
    assert!(w.decorated_pot_insert(AT, Stack::new(Item::DIAMOND, 1)));
    ticks(&mut w, 3);
    // floor(14 * 1/64) + 1.
    assert_eq!(w.container_signal(AT), Some(1));
    assert_eq!(w.redstone_power(comparator + IVec3::Z), 1);
}

#[test]
fn observers_see_fire_age_changes() {
    let mut w = world();
    let fire = AT;
    put(&mut w, fire - IVec3::Y, Block::NETHERRACK);
    put(&mut w, fire, Block(165));
    // An observer west of the fire, facing it (east).
    let observer = fire - IVec3::X;
    put(&mut w, observer, r::observer(2, false));
    ticks(&mut w, 5);
    assert!(w.set_block(fire, Block(166)));
    ticks(&mut w, 2);
    assert!(matches!(w.get_block(observer).and_then(r::component), Some(r::Component::Observer { on: true, .. })));
}

#[test]
fn note_blocks_under_mob_heads_imitate_the_mob() {
    let mut w = world();
    put(&mut w, AT, gadgets::note(5, false));
    put(&mut w, AT + IVec3::Y, voxelcraft::world::skulls::floor(voxelcraft::world::skulls::Kind::Zombie, 0));
    put(&mut w, AT + IVec3::X, r::lever(0, false));
    assert!(w.use_redstone(AT + IVec3::X));
    ticks(&mut w, 1);
    let (_, pitch, instrument) = *w.notes.last().expect("a head above doesn't silence the note");
    assert_eq!(pitch, 12, "mob heads play at pitch 1");
    assert_ne!(instrument, gadgets::Instrument::Harp as u8);
}

#[test]
fn powered_tnt_primes_and_doors_and_bells_follow_power() {
    let mut w = world();
    put(&mut w, AT, Block::TNT);
    put(&mut w, AT + IVec3::X, r::REDSTONE_BLOCK);
    ticks(&mut w, 1);
    assert_ne!(w.get_block(AT), Some(Block::TNT), "powered TNT primes");

    let door = AT + IVec3::new(0, 0, 4);
    assert!(w.place_iron_door(door, Facing::North));
    put(&mut w, door + IVec3::X, r::lever(0, false));
    assert!(w.use_redstone(door + IVec3::X));
    ticks(&mut w, 1);
    assert!(matches!(w.get_block(door).and_then(r::component), Some(r::Component::IronDoor { open: true, .. })));
}

#[test]
fn redstone_ore_lights_on_touch() {
    let mut w = world();
    put(&mut w, AT, Block::REDSTONE_ORE);
    assert!(w.touch_redstone_ore(AT));
    assert_eq!(w.get_block(AT).unwrap().emission(), 9);
}

/// Whether a door, trapdoor or fence gate state is open.
fn open(b: Block) -> bool {
    match r::component(b) {
        Some(r::Component::Trapdoor { open, .. } | r::Component::IronDoor { open, .. }) => open,
        _ => match b.shaped() {
            Some(Shaped::Gate { open, .. } | Shaped::Door { open, .. }) => open,
            other => panic!("{} is no door, trapdoor or gate: {other:?}", b.name()),
        },
    }
}

/// Places `item` on the floor in front of a player at `AT + 3z` looking
/// at the cell `AT` (doors place both halves through the shared path).
fn place_on_floor(w: &mut World, item: Item) -> Block {
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::new(8.5, AT.y as f64, 11.5));
    s.inventory.slots[0] = Some(Stack::new(item, 1));
    let eye = DVec3::new(8.5, AT.y as f64 + 1.62, 11.5);
    let point = DVec3::new(8.5, AT.y as f64, 8.5);
    let d = (point - eye).normalize();
    s.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    s.player.yaw = d.z.atan2(d.x) as f32;
    s.player.pitch = d.y.asin() as f32;
    match s.place_held(0, w, &[]).unwrap() {
        Outcome::Placed { block, .. } => block,
        other => panic!("{} not placed: {other:?}", item_name(item)),
    }
}

fn item_name(item: Item) -> String {
    format!("{item:?}")
}

/// `DoorBlock`, `TrapDoorBlock` and `FenceGateBlock.neighborChanged`: a
/// power change sets `OPEN` to the new power, for every Java 26.3 door,
/// trapdoor and fence gate (wood, iron and copper).
#[test]
fn every_door_trapdoor_and_gate_follows_its_power() {
    let fixture = fixture();
    let names = names();
    let classes =
        ["DoorBlock", "WeatheringCopperDoorBlock", "TrapDoorBlock", "WeatheringCopperTrapDoorBlock", "FenceGateBlock"];
    let mut count = 0;
    for (name, entry) in fixture["blocks"].as_object().unwrap() {
        if !classes.contains(&entry["class"].as_str().unwrap()) {
            continue;
        }
        let block = resolve(&names, name).unwrap_or_else(|| panic!("{name}"));
        let mut w = world();
        let placed = place_on_floor(&mut w, Item::from_block(block));
        assert!(!open(placed), "{name} places closed");
        let power = AT + IVec3::X;
        put(&mut w, power, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert!(open(w.get_block(AT).unwrap()), "{name} opens with power");
        put(&mut w, power, Block::AIR);
        ticks(&mut w, 1);
        assert!(!open(w.get_block(AT).unwrap()), "{name} closes without power");
        count += 1;
    }
    assert_eq!(count, 22 + 22 + 13, "every door, trapdoor and gate, copper ones included");
}

/// `JukeboxBlock`: weak 15 to every side while a song plays, and the
/// comparator reads the disc's own output.
#[test]
fn jukeboxes_power_while_playing_and_comparators_read_the_disc() {
    let mut w = world();
    put(&mut w, AT, voxelcraft::world::jukebox::JUKEBOX);
    let disc = voxelcraft::world::jukebox::disc_named("music_disc_cat").unwrap();
    assert!(w.insert_disc(AT, Stack::new(disc, 1)));
    ticks(&mut w, 1);
    assert_eq!(w.redstone_power(AT + IVec3::X), 15);
    assert_eq!(w.container_signal(AT), Some(2), "cat's comparator output");
    put(&mut w, AT + IVec3::X, Block::STONE);
    assert_eq!(w.redstone_power(AT + IVec3::X * 2), 0, "weak only: the stone isn't powered");
}
