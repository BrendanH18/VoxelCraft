use super::*;
use crate::world::terrain::Generator;
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

const P: IVec3 = IVec3::new(3, 10, 3);

/// The pinned Java facts these lids follow (`sounds.json`'s `lids`).
fn fixture() -> serde_json::Value {
    let path = format!("{}/tests/fixtures/java/26.3/sounds.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_lid_moves_by_javas_step_and_eases() {
    let java = fixture();
    assert_eq!(java["lids"]["chest_lid_step_per_tick"].as_f64(), Some(f64::from(LID_STEP) as f32 as f64));
    let mut lid = Lid { should_be_open: true, ..Default::default() };
    for i in 1..=12 {
        lid.tick();
        assert!((lid.openness - (i as f32 * 0.1).min(1.0)).abs() < 1e-5, "tick {i}");
    }
    assert!((lid.at(0.5) - 1.0).abs() < 1e-6);
    lid.should_be_open = false;
    lid.tick();
    assert!((lid.at(0.5) - 0.95).abs() < 1e-5, "partial ticks interpolate");
    for _ in 0..10 {
        lid.tick();
    }
    assert_eq!(lid.openness, 0.0);
    // `ChestRenderer`: 1 - (1 - t)^3.
    assert_eq!(eased(0.0), 0.0);
    assert!((eased(0.5) - 0.875).abs() < 1e-6);
    assert_eq!(eased(1.0), 1.0);
}

#[test]
fn a_chest_opens_for_its_first_viewer_and_shuts_after_its_last() {
    let mut w = world();
    w.set_block(P, Block::CHEST);
    w.set_container_viewers(&[P]);
    let cues = w.take_lid_cues();
    assert_eq!(cues, [Cue { pos: P.as_dvec3() + DVec3::splat(0.5), container: Container::Chest, open: true }]);
    assert_eq!(w.lidded_in_chunk(crate::world::chunk::chunk_of(P)), [P], "the mesh leaves it to the model");
    for _ in 0..10 {
        w.tick_lids();
    }
    assert_eq!(w.chest_lid(P, 1.0), 1.0);
    // A second viewer: no new sound.
    w.set_container_viewers(&[P, P]);
    assert!(w.take_lid_cues().is_empty());
    w.set_container_viewers(&[]);
    assert_eq!(w.take_lid_cues().len(), 1);
    // Ten ticks down, then one more with the lid at rest before it's forgotten.
    let mut ticks = 0;
    while w.lidded().count() > 0 && ticks < 20 {
        w.tick_lids();
        ticks += 1;
    }
    assert!((10..=12).contains(&ticks), "{ticks} ticks to shut");
    assert_eq!(w.chest_lid(P, 1.0), 0.0);
    assert!(w.lidded_in_chunk(crate::world::chunk::chunk_of(P)).is_empty());
}

#[test]
fn a_double_chest_sounds_once_between_its_halves_and_lifts_both_lids() {
    use crate::world::double_chest::PLAIN_DOUBLE;
    let mut w = world();
    // Facing north: the left half's partner is east of it.
    w.set_block(P, Block(PLAIN_DOUBLE));
    w.set_block(P + IVec3::X, Block(PLAIN_DOUBLE + 4));
    assert_eq!(w.chest_halves(P).1, Some(P + IVec3::X), "the halves pair");
    w.set_container_viewers(&[P + IVec3::X]);
    let cues = w.take_lid_cues();
    assert_eq!(cues.len(), 1, "{cues:?}");
    assert_eq!(cues[0].pos, P.as_dvec3() + DVec3::new(1.0, 0.5, 0.5), "between the halves");
    w.tick_lids();
    assert!(w.chest_lid(P, 1.0) > 0.0 && w.chest_lid(P + IVec3::X, 1.0) > 0.0);
}

#[test]
fn ender_chests_and_barrels_open_and_barrels_show_their_open_top() {
    let mut w = world();
    let ender = crate::world::utilities::ender_chest(Facing::South, false);
    let barrel = crate::world::village_blocks::BARREL_UP;
    let b = P + IVec3::new(4, 0, 0);
    w.set_block(P, ender);
    w.set_block(b, barrel);
    w.set_container_viewers(&[P, b]);
    let mut cues = w.take_lid_cues();
    cues.sort_by(|a, b| a.pos.x.total_cmp(&b.pos.x));
    assert_eq!(cues[0].container, Container::EnderChest);
    assert_eq!(
        cues[1],
        Cue { pos: b.as_dvec3() + DVec3::new(0.5, 1.0, 0.5), container: Container::Barrel, open: true }
    );
    assert!(w.barrel_open(b));
    let faces = barrel_faces(barrel).unwrap();
    assert_eq!(faces[2], BARREL_OPEN_TEX, "the open end faces up");
    w.set_container_viewers(&[]);
    assert!(!w.barrel_open(b));
    assert_eq!(w.take_lid_cues().iter().filter(|c| !c.open).count(), 2);
}

#[test]
fn lan_clients_get_the_hosts_lids() {
    let mut host = world();
    host.set_block(P, Block::CHEST);
    host.set_container_viewers(&[P]);
    for _ in 0..3 {
        host.tick_lids();
    }
    let sent = serde_json::json!(host.lids_near(P.as_dvec3(), 16.0));
    let mut client = world();
    client.set_block(P, Block::CHEST);
    client.apply_lids(&sent);
    assert!((client.chest_lid(P, 1.0) - 0.3).abs() < 1e-5);
    assert!((client.chest_lid(P, 0.0) - 0.2).abs() < 1e-5);
    assert_eq!(client.lidded().count(), 1);
    client.apply_lids(&serde_json::json!([]));
    assert_eq!(client.lidded().count(), 0);
}

#[test]
fn the_lid_turns_up_about_its_back_edge() {
    let tex = Block::CHEST.info().tex;
    let (facing, bounds) = chest_model(Block::CHEST).unwrap();
    let top = |openness: f32| {
        let mut highest = f32::MIN;
        chest_quads(facing, bounds, tex, openness, |q| {
            for c in q.corners {
                highest = highest.max(c.y);
            }
        });
        highest
    };
    assert!((top(0.0) - 14.0 / 16.0).abs() < 1e-5, "shut: the chest's own height");
    assert!(top(1.0) > 0.95, "open: the lid stands up from its hinge");
}
