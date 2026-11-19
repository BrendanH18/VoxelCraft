use super::*;
use glam::{DVec3, IVec2};

fn world() -> World {
    let g = std::sync::Arc::new(crate::world::terrain::Generator::new(4));
    let mut w = World::new_headless(g, Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.load_test_column(IVec2::new(x, z), 63);
        }
    }
    w
}

/// A still pool on the ground's surface, one block deep.
fn pool(w: &mut World) {
    for x in -2..=12 {
        for z in -2..=12 {
            w.set_block(IVec3::new(x, 63, z), Block::WATER);
        }
    }
}

#[test]
fn states_are_ages_without_items_or_drops() {
    for a in 0..=MAX_AGE {
        let b = frosted_ice(a);
        assert_eq!(b.0, 12256 + u16::from(a));
        assert_eq!(age(b), Some(a));
        assert_eq!(b.name(), "frosted ice");
        assert_eq!(b.drop(), None, "blocks/frosted_ice is an empty table");
        assert_eq!(b.base(), FIRST);
        assert_eq!(b.hardness(), 0.5);
        assert_eq!(b.kind(), RenderKind::Translucent);
    }
    assert_eq!(age(Block::ICE), None);
    assert!(!Block::creative_palette().any(|b| owns(b.0)), "Java has no frosted ice item");
}

/// `frost_walker.json`: radius `3 + (level - 1)`, horizontal distance from
/// the feet below the radius, still water with air above only.
#[test]
fn frost_walker_freezes_a_disk_of_still_water() {
    assert_eq!((frost_walker_radius(1), frost_walker_radius(2)), (3, 4));
    let mut w = world();
    pool(&mut w);
    let feet = DVec3::new(5.5, 64.0, 5.5);
    let placed = w.frost_walk(feet, 1, |_| false);
    let mut frozen = Vec::new();
    for x in -2..=12 {
        for z in -2..=12 {
            if w.get_block(IVec3::new(x, 63, z)).is_some_and(|b| owns(b.0)) {
                frozen.push((x, z));
            }
        }
    }
    assert_eq!(placed, frozen.len());
    for x in -2..=12 {
        for z in -2..=12 {
            let (dx, dz) = (f64::from(x) + 0.5 - feet.x, f64::from(z) + 0.5 - feet.z);
            assert_eq!(frozen.contains(&(x, z)), dx * dx + dz * dz < 9.0, "{x},{z}");
        }
    }
    assert_eq!(w.get_block(IVec3::new(5, 63, 5)), Some(frosted_ice(0)));
    // Level 2 reaches one block further.
    let mut w = world();
    pool(&mut w);
    assert!(w.frost_walk(feet, 2, |_| false) > placed);
    // Water with something above, flowing water and occupied cells stay.
    let mut w = world();
    pool(&mut w);
    w.set_block(IVec3::new(6, 64, 5), Block::STONE);
    w.set_block(IVec3::new(4, 63, 5), Block(24));
    w.frost_walk(feet, 1, |p| p == IVec3::new(5, 63, 6));
    assert_eq!(w.get_block(IVec3::new(6, 63, 5)), Some(Block::WATER));
    assert!(!w.get_block(IVec3::new(4, 63, 5)).is_some_and(|b| owns(b.0)));
    assert_eq!(w.get_block(IVec3::new(5, 63, 6)), Some(Block::WATER));
}

/// In full light an isolated block ages on every tick (fewer than four
/// frosted neighbours) and melts back to water after age 3; a dark one
/// keeps rescheduling and stays.
#[test]
fn frosted_ice_ages_in_light_and_melts_to_water() {
    let mut w = world();
    let lit = IVec3::new(5, 64, 5);
    w.set_block(lit, frosted_ice(0));
    // A sealed pocket inside the ground: no light at all.
    let dark = IVec3::new(9, 55, 9);
    w.set_block(dark, frosted_ice(0));
    let mut ages = vec![0];
    for _ in 0..2000 {
        w.tick_redstone();
        match w.get_block(lit).and_then(age) {
            Some(a) if ages.last() != Some(&a) => ages.push(a),
            Some(_) => {}
            None => break,
        }
    }
    assert_eq!(ages, [0, 1, 2, 3]);
    assert_eq!(w.get_block(lit), Some(Block::WATER));
    assert_eq!(w.get_block(dark), Some(frosted_ice(0)), "too dark to age");
}

/// `neighborChanged`: frosted ice left with fewer than two frosted
/// neighbours when a frosted neighbour goes melts at once.
#[test]
fn losing_a_frosted_neighbour_melts_the_ice_beside_it() {
    let mut w = world();
    for x in 2..=4 {
        w.set_block(IVec3::new(x, 64, 5), frosted_ice(0));
    }
    // Each end has one frosted neighbour; removing the middle melts both.
    w.set_block(IVec3::new(3, 64, 5), Block::AIR);
    assert_eq!(w.get_block(IVec3::new(2, 64, 5)), Some(Block::WATER));
    assert_eq!(w.get_block(IVec3::new(4, 64, 5)), Some(Block::WATER));
    // Ageing a neighbour is no change of block and melts nothing.
    let mut w = world();
    for x in 2..=3 {
        w.set_block(IVec3::new(x, 64, 5), frosted_ice(0));
    }
    w.set_block(IVec3::new(2, 64, 5), frosted_ice(1));
    assert_eq!(w.get_block(IVec3::new(3, 64, 5)), Some(frosted_ice(0)));
}
