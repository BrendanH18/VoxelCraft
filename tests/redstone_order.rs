//! Java 26.3 redstone update order: the constants and orders extracted from
//! the pinned jar, and seven small contraptions whose per-tick states and
//! update logs an independent model of the Java classes produced
//! (`tools/extract_java_redstone_order_fixtures.py`). Each case is rebuilt
//! here through the game's own placement, use and tick entry points.
use glam::IVec3;
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::world::{
    World,
    block::{Block, Facing},
    chunk::ChunkData,
    redstone_blocks::{self as r, Component},
    redstone_order::{self as order, Traced},
    terrain::Generator,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/redstone_order.json")).unwrap()
}

const DIRECTION_NAMES: [(&str, IVec3); 6] = [
    ("down", IVec3::NEG_Y),
    ("up", IVec3::Y),
    ("north", IVec3::NEG_Z),
    ("south", IVec3::Z),
    ("west", IVec3::NEG_X),
    ("east", IVec3::X),
];

fn direction(name: &str) -> IVec3 {
    DIRECTION_NAMES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("direction {name}")).1
}

fn direction_name(d: IVec3) -> &'static str {
    DIRECTION_NAMES.iter().find(|(_, v)| *v == d).unwrap().0
}

fn facing(d: IVec3) -> Facing {
    Facing::from_offset(d).unwrap()
}

/// The six-way facing index `redstone_blocks::direction` reads.
fn six(d: IVec3) -> u8 {
    (0..6).find(|&f| r::direction(f) == d).unwrap()
}

/// Wall mounts 1-4 face `Facing::ALL[mount - 1]`, away from their support.
fn wall_mount(d: IVec3) -> u8 {
    Facing::ALL.iter().position(|&f| f.offset() == d).unwrap() as u8 + 1
}

/// Fixture state names use Java's property values: a diode's facing is the
/// side its input comes from, an observer's the side it watches.
fn parse(spec: &str) -> Block {
    let parts: Vec<&str> = spec.split('/').collect();
    let on = |s: &str| s == "1";
    match parts[0] {
        "air" => Block::AIR,
        "stone" => Block::STONE,
        "redstone_block" => r::REDSTONE_BLOCK,
        "wire" => r::wire(parts[1].parse().unwrap()),
        "lever" if parts[1] == "floor" => r::lever(0, on(parts[2])),
        "lever" => r::lever(wall_mount(direction(parts[2])), on(parts[3])),
        "repeater" => r::repeater(facing(-direction(parts[1])), parts[2].parse().unwrap(), on(parts[3])),
        "comparator" => r::comparator(facing(-direction(parts[1])), parts[2] == "subtract", on(parts[3])),
        "torch" => r::torch(0, on(parts[1])),
        "wall_torch" => r::torch(wall_mount(direction(parts[1])), on(parts[2])),
        "lamp" => {
            if on(parts[1]) {
                Block(r::LAMP.0 + 1)
            } else {
                r::LAMP
            }
        }
        "observer" => r::observer(six(direction(parts[1])), on(parts[2])),
        "piston" => r::piston(six(direction(parts[1])), false, on(parts[2])),
        other => panic!("unknown fixture state {other}"),
    }
}

fn name(b: Block) -> String {
    let bit = |on: bool| on as u8;
    match r::component(b) {
        _ if b == Block::AIR => "air".into(),
        _ if b == Block::STONE => "stone".into(),
        Some(Component::Source) => "redstone_block".into(),
        Some(Component::Wire(power)) => format!("wire/{power}"),
        Some(Component::Lever { on, .. }) => format!("lever/{}", bit(on)),
        Some(Component::Repeater { facing, delay, on }) => {
            format!("repeater/{}/{delay}/{}", direction_name(facing.opposite().offset()), bit(on))
        }
        Some(Component::Comparator { facing, subtract, on }) => format!(
            "comparator/{}/{}/{}",
            direction_name(facing.opposite().offset()),
            if subtract { "subtract" } else { "compare" },
            bit(on)
        ),
        Some(Component::Torch { mount: 0, lit }) => format!("torch/{}", bit(lit)),
        Some(Component::Torch { mount, lit }) => {
            format!("wall_torch/{}/{}", direction_name(-r::support(mount)), bit(lit))
        }
        Some(Component::Lamp(on)) => format!("lamp/{}", bit(on)),
        Some(Component::Observer { facing, on }) => {
            format!("observer/{}/{}", direction_name(r::direction(facing)), bit(on))
        }
        Some(Component::Piston { facing, extended, .. }) => {
            format!("piston/{}/{}", direction_name(r::direction(facing)), bit(extended))
        }
        _ => format!("other/{}", b.0),
    }
}

fn cell(v: &Value) -> IVec3 {
    IVec3::new(v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32, v[2].as_i64().unwrap() as i32)
}

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

fn log_entry(t: Traced) -> (&'static str, IVec3, Block) {
    match t {
        Traced::Neighbor(p, b) => ("n", p, b),
        Traced::Shape(p, b) => ("s", p, b),
        Traced::Tick(p, b) => ("t", p, b),
        Traced::Event(p, b) => ("e", p, b),
    }
}

/// Watched states per tick, and the filtered update log of each logged tick.
type Rebuilt = (Vec<Vec<String>>, Vec<(String, Vec<String>)>);

/// Rebuild one case: its per-tick watched states and its filtered logs.
fn run(case: &Value) -> Rebuilt {
    let mut w = world();
    for entry in case["setup"].as_array().unwrap() {
        assert!(w.set_block(cell(entry), parse(entry[3].as_str().unwrap())), "setup {entry}");
    }
    for _ in 0..case["settle"].as_u64().unwrap() {
        w.tick_redstone();
    }
    let compare: Vec<&str> =
        case["compare"].as_array().map_or(Vec::new(), |k| k.iter().map(|k| k.as_str().unwrap()).collect());
    let stop_at_event = case["stop_at_event"].as_bool().unwrap_or(false);
    let logged: Vec<u64> =
        case["log_ticks"].as_array().map_or(Vec::new(), |t| t.iter().map(|t| t.as_u64().unwrap()).collect());
    let watch: Vec<IVec3> = case["watch"].as_array().unwrap().iter().map(cell).collect();
    let (mut trace, mut logs) = (Vec::new(), Vec::new());
    for tick in 1..=case["ticks"].as_u64().unwrap() {
        let log = logged.contains(&tick);
        w.trace_redstone(log);
        for action in case["actions"].as_array().unwrap().iter().filter(|a| a[0].as_u64() == Some(tick)) {
            let action = &action[1];
            let p = IVec3::new(
                action[1].as_i64().unwrap() as i32,
                action[2].as_i64().unwrap() as i32,
                action[3].as_i64().unwrap() as i32,
            );
            match action[0].as_str().unwrap() {
                "place" => assert!(w.set_block(p, parse(action[4].as_str().unwrap())), "{action}"),
                "use" => assert!(w.use_redstone(p), "{action}"),
                other => panic!("unknown action {other}"),
            }
        }
        w.tick_redstone();
        if log {
            let mut kept = Vec::new();
            for (kind, p, b) in w.take_redstone_trace().into_iter().map(log_entry) {
                let state = name(b);
                if state == "air" || state == "stone" {
                    continue;
                }
                if compare.contains(&kind) {
                    kept.push(format!("{kind} {} {} {} {state}", p.x, p.y, p.z));
                }
                if kind == "e" && stop_at_event {
                    break;
                }
            }
            logs.push((tick.to_string(), kept));
        }
        w.trace_redstone(false);
        trace.push(watch.iter().map(|&p| name(w.get_block(p).unwrap())).collect());
    }
    (trace, logs)
}

fn expected_log(entries: &Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| format!("{} {} {} {} {}", e[0].as_str().unwrap(), e[1], e[2], e[3], e[4].as_str().unwrap()))
        .collect()
}

fn check(name: &str) {
    let fixture = fixture();
    let case = fixture["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap();
    let (trace, logs) = run(case);
    for (tick, (got, want)) in trace.iter().zip(case["trace"].as_array().unwrap()).enumerate() {
        let want: Vec<&str> = want.as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
        assert_eq!(got, &want, "{name}: states after tick {}", tick + 1);
    }
    for (tick, got) in logs {
        let want = expected_log(&case["logs"][&tick]);
        if got != want {
            let first = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
            panic!(
                "{name}: update log of tick {tick} differs at entry {first} ({} entries, Java {}):\n  got  {:?}\n  java {:?}",
                got.len(),
                want.len(),
                &got[first..got.len().min(first + 8)],
                &want[first..want.len().min(first + 8)]
            );
        }
    }
}

#[test]
fn extracted_orders_and_constants_match_the_engine() {
    let f = &fixture()["facts"];
    let names = |key: &str| -> Vec<IVec3> {
        f[key].as_array().unwrap().iter().map(|n| direction(n.as_str().unwrap())).collect()
    };
    assert_eq!(names("update_order"), order::UPDATE_ORDER);
    assert_eq!(names("update_shape_order"), order::UPDATE_SHAPE_ORDER);
    assert_eq!(names("direction_values"), order::DIRECTIONS);
    assert_eq!(names("horizontal"), order::HORIZONTAL);
    assert_eq!(names("vertical"), order::VERTICAL);
    assert_eq!(f["max_chained_neighbor_updates"], order::MAX_CHAINED_NEIGHBOR_UPDATES as u64);
    assert_eq!(f["max_scheduled_ticks"], order::MAX_SCHEDULED_TICKS as u64);
    let priority = &f["tick_priority"];
    assert_eq!(priority["EXTREMELY_HIGH"], order::priority::EXTREMELY_HIGH as i64);
    assert_eq!(priority["VERY_HIGH"], order::priority::VERY_HIGH as i64);
    assert_eq!(priority["HIGH"], order::priority::HIGH as i64);
    assert_eq!(priority["NORMAL"], order::priority::NORMAL as i64);
}

#[test]
fn wire_updates_follow_the_java_hash_set_order() {
    // `DefaultRedstoneWireEvaluator` collects the wire and its six
    // neighbours (inserted in `Direction.values()` order) in a `HashSet`,
    // which iterates by the bucket of `Vec3i.hashCode`, so the order
    // depends on where the wire is.
    let set = |p: IVec3| {
        order::java_hash_set_order([
            p,
            p - IVec3::Y,
            p + IVec3::Y,
            p - IVec3::Z,
            p + IVec3::Z,
            p - IVec3::X,
            p + IVec3::X,
        ])
    };
    let p = IVec3::ZERO;
    assert_eq!(set(p), [p, p - IVec3::Z, p - IVec3::X, p + IVec3::Z, p + IVec3::X, p - IVec3::Y, p + IVec3::Y]);
    let p = IVec3::new(4, 145, 4);
    assert_eq!(set(p), [p + IVec3::Y, p - IVec3::Z, p - IVec3::X, p, p - IVec3::Y, p + IVec3::Z, p + IVec3::X]);
    check("wire_line_order");
}

#[test]
fn locking_repeater_wins_its_race_on_extremely_high_priority() {
    check("repeater_lock_race");
}

#[test]
fn torch_burnout_waits_for_its_160_tick_tick() {
    check("torch_burnout");
}

#[test]
fn quasi_connected_piston_waits_for_a_block_update_and_extends_on_its_block_event() {
    check("piston_qc_bud");
}

#[test]
fn facing_observers_form_a_clock_through_shape_updates() {
    check("observer_clock");
}

#[test]
fn comparator_compare_and_subtract_modes_follow_a_pulse() {
    check("comparator_modes");
}

#[test]
fn one_pulse_runs_ticks_in_priority_then_sub_tick_order() {
    check("priority_race");
}

#[test]
fn block_events_deduplicate_and_wait_for_the_end_of_the_tick() {
    let mut w = world();
    let p = IVec3::new(8, 145, 4);
    w.set_block(p - IVec3::Y, Block::STONE);
    w.set_block(p, r::piston(1, false, false));
    w.set_block(p + IVec3::Y + IVec3::X, r::REDSTONE_BLOCK);
    // Two block updates queue one event (`ObjectLinkedOpenHashSet`).
    w.set_block(p - IVec3::X, Block::STONE);
    w.set_block(p + IVec3::X, Block::STONE);
    assert_eq!(w.block_events().count(), 1);
    assert_eq!(w.get_block(p), Some(r::piston(1, false, false)), "events run at the end of a tick");
    w.tick_block_ticks();
    assert_eq!(w.get_block(p), Some(r::piston(1, false, false)), "not during the block ticks");
    w.run_block_events();
    assert_eq!(w.get_block(p), Some(r::piston(1, false, true)));
    assert_eq!(w.block_events().count(), 0);
}
