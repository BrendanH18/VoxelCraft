use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::{add_or_append, seen_percent};
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::{World, block::Block, chunk::ChunkData, terrain::Generator};

const FLOOR: i32 = 140;

/// A loaded, empty 3 x 3 column block with a stone floor at `FLOOR`.
fn world(seed: u64) -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(seed)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.insert_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
    }
    for x in -20..=20 {
        for z in -20..=20 {
            w.edit(IVec3::new(x, FLOOR, z), Block::STONE, false);
        }
    }
    w.set_tile_drops(true);
    w
}

fn fill(w: &mut World, min: IVec3, max: IVec3, b: Block) {
    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                w.edit(IVec3::new(x, y, z), b, false);
            }
        }
    }
}

#[test]
fn blast_resistance_is_java_s() {
    for (b, r) in [
        (Block::STONE, 6.0),
        (Block::DIRT, 0.5),
        (Block::OBSIDIAN, 1200.0),
        (Block::BEDROCK, 3_600_000.0),
        (Block::WATER, 100.0),
        (Block::TNT, 0.0),
    ] {
        assert_eq!(b.blast_resistance(), r, "{}", b.name());
    }
}

#[test]
fn rays_reach_at_most_1_3_times_power_over_0_225_steps_in_open_air() {
    let mut w = world(1);
    let center = DVec3::new(0.5, FLOOR as f64 + 8.5, 0.5);
    let cells = w.exploded_cells(center, 4.0);
    // 5.2 intensity lasts 23 steps of 0.3 blocks.
    let reach = (4.0f64 * 1.3 / 0.225).ceil() * 0.3;
    assert!(cells.iter().all(|c| (c.as_dvec3() + DVec3::splat(0.5)).distance(center) < reach + 1.0));
    assert!(cells.iter().any(|c| (c.as_dvec3() + DVec3::splat(0.5)).distance(center) > 4.0));
    assert!(cells.iter().all(|c| c.y > FLOOR), "8 blocks up, the floor is out of reach");
}

#[test]
fn stone_holds_a_tnt_blast_to_a_small_crater_and_obsidian_shields_what_it_hides() {
    let mut w = world(2);
    fill(&mut w, IVec3::new(-8, FLOOR + 1, -8), IVec3::new(8, FLOOR + 9, 8), Block::STONE);
    let center = IVec3::new(0, FLOOR + 5, 0);
    w.edit(center, Block::AIR, false);
    let removed = w.explode(center.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), 4.0);
    assert!(removed > 0, "the cells around the TNT break");
    let far = (-8..=8)
        .flat_map(|x| (-8..=8).map(move |z| IVec3::new(x, center.y, z)))
        .filter(|p| (*p - center).abs().max_element() >= 3)
        .filter(|&p| w.get_block(p) == Some(Block::AIR))
        .count();
    assert_eq!(far, 0, "stone stops the rays within two blocks");

    let mut w = world(3);
    let center = DVec3::new(0.5, FLOOR as f64 + 1.0, 0.5);
    fill(&mut w, IVec3::new(2, FLOOR + 1, -3), IVec3::new(2, FLOOR + 4, 3), Block::OBSIDIAN);
    fill(&mut w, IVec3::new(3, FLOOR + 1, -3), IVec3::new(3, FLOOR + 4, 3), Block::DIRT);
    fill(&mut w, IVec3::new(-3, FLOOR + 1, -3), IVec3::new(-3, FLOOR + 4, 3), Block::DIRT);
    w.explode(center, 4.0);
    let standing =
        |w: &World, x: i32| (-1..=1).filter(|&z| w.get_block(IVec3::new(x, FLOOR + 1, z)) == Some(Block::DIRT)).count();
    assert_eq!(standing(&w, 3), 3, "dirt behind obsidian is shielded");
    assert_eq!(standing(&w, -3), 0, "the same dirt in the open breaks");
    assert!((-3..=3).all(|z| w.get_block(IVec3::new(2, FLOOR + 1, z)) == Some(Block::OBSIDIAN)));
}

#[test]
fn water_shields_a_tnt_blast() {
    let mut w = world(4);
    fill(&mut w, IVec3::new(-4, FLOOR + 1, -4), IVec3::new(4, FLOOR + 1, 4), Block::DIRT);
    w.edit(IVec3::new(0, FLOOR + 2, 0), Block::WATER, false);
    let removed = w.explode(DVec3::new(0.5, FLOOR as f64 + 2.5, 0.5), 4.0);
    assert_eq!(removed, 0, "a blast inside water breaks nothing");
}

#[test]
fn exposure_is_the_share_of_the_box_that_sees_the_blast() {
    let mut w = world(5);
    let center = DVec3::new(0.5, FLOOR as f64 + 1.5, 0.5);
    let (min, max) = (DVec3::new(3.2, FLOOR as f64 + 1.0, 0.2), DVec3::new(3.8, FLOOR as f64 + 2.8, 0.8));
    assert_eq!(seen_percent(&w, center, min, max), 1.0);
    fill(&mut w, IVec3::new(2, FLOOR + 1, -1), IVec3::new(2, FLOOR + 1, 1), Block::STONE);
    let half = seen_percent(&w, center, min, max);
    // Only the top row of sample points clears the wall.
    assert!(half > 0.0 && half < 0.5, "a one-high wall hides the legs: {half}");
    fill(&mut w, IVec3::new(2, FLOOR + 2, -1), IVec3::new(2, FLOOR + 3, 1), Block::STONE);
    assert_eq!(seen_percent(&w, center, min, max), 0.0);
}

#[test]
fn drops_pile_up_to_sixteen_at_the_first_cell_that_dropped_them() {
    let (a, b) = (IVec3::ZERO, IVec3::X);
    let (cobble, dirt) = (Item::from_block(Block::COBBLESTONE), Item::from_block(Block::DIRT));
    let mut piles = Vec::new();
    for _ in 0..20 {
        add_or_append(&mut piles, Stack::new(cobble, 1), a);
    }
    add_or_append(&mut piles, Stack::new(dirt, 1), b);
    add_or_append(&mut piles, Stack::new(cobble, 1), b);
    let summary: Vec<_> = piles.iter().map(|(p, s)| (*p, s.item, s.count)).collect();
    assert_eq!(
        summary,
        [(a, cobble, 16), (a, cobble, 5), (b, dirt, 1)],
        "the 21st cobblestone tops up the second pile"
    );
    // `areMergable` refuses two stacks that wouldn't fit in one.
    let mut piles = vec![(a, Stack::new(Item::ENDER_PEARL, 10))];
    add_or_append(&mut piles, Stack::new(Item::ENDER_PEARL, 10), b);
    assert_eq!(piles.len(), 2);
}

#[test]
fn bed_and_anchor_blasts_light_fires_and_tnt_does_not() {
    for fire in [true, false] {
        let mut w = world(6);
        let center = DVec3::new(0.5, FLOOR as f64 + 1.0, 0.5);
        let cells = w.exploded_cells(center, 5.0);
        w.detonate(&cells, 5.0, true, true, fire);
        let lit = cells.iter().filter(|&&p| w.get_block(p).is_some_and(Block::is_fire)).count();
        assert_eq!(lit > 0, fire, "fire = {fire}: {lit} fires");
        assert!(
            cells
                .iter()
                .filter(|&&p| w.get_block(p).is_some_and(Block::is_fire))
                .all(|&p| w.get_block(p - IVec3::Y).is_some_and(Block::is_opaque)),
            "fires stand on solid blocks"
        );
    }
}
