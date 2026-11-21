use super::*;
use crate::world::World;
use crate::world::terrain::Generator;
use glam::IVec3;
use std::sync::Arc;

/// A flat test column with air well above the ground.
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

fn pyramid(w: &mut World, top: IVec3, levels: i32, base: Block) {
    for step in 1..=levels {
        for x in -step..=step {
            for z in -step..=step {
                w.set_block(top + IVec3::new(x, -step, z), base);
            }
        }
    }
}

fn run(w: &mut World, ticks: u64) {
    for t in 1..=ticks {
        w.tick_beacons(t);
    }
}

#[test]
fn powers_follow_javas_level_gating() {
    use Effect::*;
    assert_eq!(required_levels(Speed), Some(1));
    assert_eq!(required_levels(Haste), Some(1));
    assert_eq!(required_levels(Resistance), Some(2));
    assert_eq!(required_levels(JumpBoost), Some(2));
    assert_eq!(required_levels(Strength), Some(3));
    assert_eq!(required_levels(Regeneration), Some(4));
    assert_eq!(required_levels(Poison), None);
    assert!(valid_powers(Some(Speed), None, 1));
    assert!(!valid_powers(Some(Resistance), None, 1));
    assert!(valid_powers(Some(Strength), None, 3));
    // Regeneration is only ever secondary, and a secondary needs four levels.
    assert!(!valid_powers(Some(Regeneration), None, 4));
    assert!(valid_powers(Some(Speed), Some(Regeneration), 4));
    assert!(!valid_powers(Some(Speed), Some(Regeneration), 3));
    // The secondary may repeat the primary (level II) but not name another tier's power.
    assert!(valid_powers(Some(Haste), Some(Haste), 4));
    assert!(!valid_powers(Some(Haste), Some(Speed), 4));
    assert!(valid_powers(None, None, 0));
    assert_eq!((range(1), range(4)), (20.0, 50.0));
    assert_eq!((duration(1), duration(4)), (220, 340));
}

#[test]
fn pyramids_count_full_layers_of_base_blocks() {
    let mut w = world();
    let top = IVec3::new(8, 100, 8);
    w.set_block(top, BEACON);
    assert_eq!(w.beacon_levels(top), 0);
    pyramid(&mut w, top, 2, Block::IRON_BLOCK);
    assert_eq!(w.beacon_levels(top), 2);
    // Mixed base blocks count; a missing corner stops the count.
    pyramid(&mut w, top, 4, Block::GOLD_BLOCK);
    w.set_block(top + IVec3::new(-3, -3, 3), Block::EMERALD_BLOCK);
    assert_eq!(w.beacon_levels(top), 4);
    w.set_block(top + IVec3::new(3, -3, -3), Block::STONE);
    assert_eq!(w.beacon_levels(top), 2);
}

#[test]
fn the_beam_climbs_ten_cells_a_tick_and_takes_glass_colours() {
    let mut w = world();
    let top = IVec3::new(8, 100, 8);
    pyramid(&mut w, top, 1, Block::IRON_BLOCK);
    w.set_block(top, BEACON);
    let red = Block::stained_glass(crate::color::DyeColor::Red);
    let blue = Block::stained_glass(crate::color::DyeColor::Blue);
    w.set_block(top + IVec3::Y * 3, red);
    w.set_block(top + IVec3::Y * 5, blue);
    // Nothing until the climb finishes and an 80-tick update sees the pyramid.
    run(&mut w, 79);
    assert_eq!(w.beacon(top).unwrap().levels, 0);
    run(&mut w, 1000);
    let b = w.beacon(top).unwrap();
    assert_eq!(b.levels, 1);
    let beam = b.beam();
    // White from the beacon, then red, then red and blue mixed.
    assert_eq!(beam[0].color, crate::color::DyeColor::White.rgb());
    assert_eq!(beam[1].color, crate::color::DyeColor::Red.rgb());
    assert_eq!(beam[2].color, average(crate::color::DyeColor::Red.rgb(), crate::color::DyeColor::Blue.rgb()));
    // An opaque block above stops the beam (bedrock doesn't).
    w.set_block(top + IVec3::Y * 8, Block::STONE);
    run(&mut w, 2000);
    assert!(w.beacon(top).unwrap().beam().is_empty());
    w.set_block(top + IVec3::Y * 8, Block::BEDROCK);
    run(&mut w, 2000);
    assert!(!w.beacon(top).unwrap().beam().is_empty());
}

#[test]
fn powers_pulse_every_eighty_ticks_and_save() {
    let mut w = world();
    let top = IVec3::new(8, 100, 8);
    pyramid(&mut w, top, 4, crate::world::stone_blocks::DIAMOND_BLOCK);
    w.set_block(top, BEACON);
    run(&mut w, 2000);
    assert_eq!(w.beacon(top).unwrap().levels, 4);
    assert!(w.set_beacon_powers(top, Some(Effect::Haste), Some(Effect::Haste)));
    assert!(!w.set_beacon_powers(top, Some(Effect::Regeneration), None));
    w.take_beacon_events();
    for t in 2001..=2080 {
        w.tick_beacons(t);
    }
    let events = w.take_beacon_events();
    let pulse = events.iter().find_map(|e| match *e {
        Event::Powers { primary, amplifier, secondary, ticks, levels, .. } => {
            Some((primary, amplifier, secondary, ticks, levels))
        }
        _ => None,
    });
    assert_eq!(pulse, Some((Effect::Haste, 1, None, 340, 4)));
    let saved = w.beacons_to_string();
    let mut other = world();
    other.load_beacons(&saved);
    let b = other.beacon(top).unwrap();
    assert_eq!((b.levels, b.primary, b.secondary), (4, Some(Effect::Haste), Some(Effect::Haste)));
    // Breaking the beacon forgets it and plays the deactivation.
    w.set_block(top, Block::AIR);
    assert!(w.beacon(top).is_none());
    assert!(w.take_beacon_events().contains(&Event::Deactivate(top)));
}

#[test]
fn the_box_reaches_up_to_the_sky() {
    let pos = IVec3::new(0, 64, 0);
    let player = |p: glam::DVec3| (p, p + glam::DVec3::new(0.6, 1.8, 0.6));
    let (lo, hi) = player(glam::DVec3::new(20.5, 64.0, 0.5));
    assert!(reaches(pos, 1, 384.0, lo, hi));
    let (lo, hi) = player(glam::DVec3::new(21.5, 64.0, 0.5));
    assert!(!reaches(pos, 1, 384.0, lo, hi));
    let (lo, hi) = player(glam::DVec3::new(0.5, 300.0, 0.5));
    assert!(reaches(pos, 1, 384.0, lo, hi));
    let (lo, hi) = player(glam::DVec3::new(0.5, 40.0, 0.5));
    assert!(!reaches(pos, 1, 384.0, lo, hi));
}
