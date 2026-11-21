use super::*;
use crate::world::World;
use crate::world::overworld_blocks as ob;
use crate::world::terrain::Generator;
use glam::IVec3;
use std::sync::Arc;

/// A flat test column with air well above the ground.
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

/// A conduit at `p` in a 5 × 5 × 5 pool, with `frame` of its 42 frame
/// cells filled with prismarine family blocks.
fn pool(w: &mut World, p: IVec3, frame: usize) {
    for x in -2..=2 {
        for y in -2..=2 {
            for z in -2..=2 {
                w.set_block(p + IVec3::new(x, y, z), Block::WATER);
            }
        }
    }
    let kinds = [ob::PRISMARINE, ob::PRISMARINE_BRICKS, ob::SEA_LANTERN, ob::DARK_PRISMARINE];
    for (i, o) in frame_offsets().take(frame).enumerate() {
        w.set_block(p + o, kinds[i % 4]);
    }
    w.set_block(p, WATERLOGGED);
}

fn run(w: &mut World, from: u64, to: u64) -> Vec<Event> {
    let mut out = Vec::new();
    for t in from..=to {
        w.tick_conduits(t, || 0);
        out.extend(w.take_conduit_events());
    }
    out
}

#[test]
fn the_frame_is_javas_42_cells() {
    let cells: Vec<IVec3> = frame_offsets().collect();
    assert_eq!(cells.len(), 42);
    for c in &cells {
        assert!(c.abs().max_element() == 2, "{c}");
        assert_eq!((c.x == 0) as u8 + (c.y == 0) as u8 + (c.z == 0) as u8 >= 1, true, "{c}");
    }
    let mut sorted = cells.clone();
    sorted.sort_by_key(|c| c.to_array());
    sorted.dedup();
    assert_eq!(sorted.len(), 42);
    // `activeSize / 7 * 16`.
    assert_eq!([16, 20, 21, 28, 35, 41, 42].map(range), [32, 32, 48, 64, 80, 80, 96]);
}

#[test]
fn the_block_matches_java() {
    assert_eq!(CONDUIT.name(), "conduit");
    assert_eq!(WATERLOGGED.canonical(), CONDUIT);
    assert!(WATERLOGGED.is_waterlogged() && !CONDUIT.is_waterlogged());
    assert_eq!((CONDUIT.emission(), WATERLOGGED.emission()), (15, 15));
    assert_eq!(CONDUIT.hardness(), 3.0);
    assert_eq!(WATERLOGGED.drop(), Some(crate::item::Item::from_block(CONDUIT)));
    assert_eq!(boxes().as_slice(), &[Box16 { min: [5, 5, 5], max: [11, 11, 11] }]);
    // Placed into a water source it holds the water.
    assert_eq!(crate::world::waterlog::carried(Block::WATER, CONDUIT), WATERLOGGED);
    assert_eq!(crate::world::waterlog::carried(Block::AIR, CONDUIT), CONDUIT);
}

#[test]
fn a_full_frame_activates_hunts_and_powers_every_40_ticks() {
    let mut w = world();
    let p = IVec3::new(8, 100, 8);
    pool(&mut w, p, 42);
    let events = run(&mut w, 1, 40);
    assert!(events.contains(&Event::Sound(Cue::Activate, p)));
    assert!(events.contains(&Event::Power { pos: p, range: 96 }));
    assert!(events.contains(&Event::Hunt { pos: p, hunting: true }));
    let c = w.conduit(p).unwrap();
    assert!(c.active && c.hunting && c.frame.len() == 42);
    // Nothing more until the next pulse, but the short ambient.
    let between = run(&mut w, 41, 79);
    assert!(!between.iter().any(|e| matches!(e, Event::Power { .. } | Event::Hunt { .. })));
    let pulse = run(&mut w, 80, 80);
    assert!(pulse.contains(&Event::Sound(Cue::Ambient, p)));
    assert!(pulse.contains(&Event::Power { pos: p, range: 96 }));
    assert_eq!(w.conduit(p).unwrap().rotation, 41.0 + 39.0);
}

#[test]
fn small_frames_and_dry_cores_change_the_conduit() {
    let mut w = world();
    let p = IVec3::new(8, 100, 8);
    pool(&mut w, p, 15);
    assert!(!run(&mut w, 1, 40).iter().any(|e| matches!(e, Event::Power { .. })));
    assert!(!w.conduit(p).unwrap().active);
    pool(&mut w, p, 16);
    let events = run(&mut w, 41, 80);
    assert!(events.contains(&Event::Power { pos: p, range: 32 }));
    assert!(events.contains(&Event::Hunt { pos: p, hunting: false }));
    // A dry cell in the core switches it off.
    w.set_block(p + IVec3::new(1, 1, 1), Block::GLASS);
    let events = run(&mut w, 81, 120);
    assert!(events.contains(&Event::Sound(Cue::Deactivate, p)));
    assert!(!w.conduit(p).unwrap().active);
}

#[test]
fn conduits_save_and_reload() {
    let mut w = world();
    let p = IVec3::new(8, 100, 8);
    pool(&mut w, p, 42);
    let saved = w.conduits_to_string();
    let mut v = world();
    v.load_conduits(&saved);
    assert!(v.conduit(p).is_some());
    // Breaking it forgets the block entity.
    w.set_block(p, Block::WATER);
    assert!(w.conduit(p).is_none());
}
