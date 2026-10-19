//! Block-level effects of the game rules in `docs/game-rules.md`.

use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::{World, WorldRules, block::Block, chunk::ChunkData, terrain::Generator};

const FLOOR: i32 = 140;

/// A loaded, empty 3 x 3 column block with a stone floor at `FLOOR`.
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
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
    w
}

fn settle(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.tick_fluids(0.25);
    }
}

/// Two sources with one empty cell between them, beside a solid floor.
fn between_two_sources(w: &mut World, fluid: Block) -> IVec3 {
    let gap = IVec3::new(0, FLOOR + 1, 0);
    w.set_block(gap - IVec3::X * 1, fluid);
    w.set_block(gap + IVec3::X * 1, fluid);
    gap
}

#[test]
fn the_defaults_match_java_26_3() {
    let rules = WorldRules::default();
    assert!(rules.tnt_explodes && rules.water_source_conversion);
    assert!(!rules.lava_source_conversion, "lava_source_conversion defaults to false");
    assert_eq!(rules.fire_spread_radius, 128);
}

#[test]
fn water_source_conversion_makes_infinite_water_unless_switched_off() {
    for (rule, expect_source) in [(true, true), (false, false)] {
        let mut w = world();
        w.rules.water_source_conversion = rule;
        let gap = between_two_sources(&mut w, Block::WATER);
        settle(&mut w, 40);
        assert_eq!(w.get_block(gap) == Some(Block::WATER), expect_source, "water_source_conversion = {rule}");
        assert!(w.get_block(gap).is_some_and(|b| b.is_fluid()), "the gap is wet either way");
    }
}

#[test]
fn lava_source_conversion_is_off_by_default_and_matches_water_when_on() {
    for (rule, expect_source) in [(false, false), (true, true)] {
        let mut w = world();
        w.rules.lava_source_conversion = rule;
        let gap = between_two_sources(&mut w, Block::LAVA);
        settle(&mut w, 120);
        assert_eq!(w.get_block(gap) == Some(Block::LAVA), expect_source, "lava_source_conversion = {rule}");
        assert!(w.get_block(gap).is_some_and(|b| b.is_lava()), "the gap is lava either way");
    }
}

#[test]
fn explosion_drops_decay_by_one_over_power_unless_the_source_never_decays() {
    let blast = |decay: bool, drops: bool| {
        let mut w = world();
        w.set_tile_drops(true);
        for x in -8..=8 {
            for z in -8..=8 {
                for y in FLOOR - 4..=FLOOR {
                    w.edit(IVec3::new(x, y, z), Block::STONE, false);
                }
            }
        }
        let removed = w.explode_with_decay(DVec3::new(0.5, FLOOR as f64 + 1.0, 0.5), 4.0, drops, decay);
        (removed, w.drops.len())
    };
    let (removed, kept) = blast(false, true);
    assert!(removed > 40);
    assert_eq!(kept, removed, "TNT (no decay) drops every destroyed block");
    let (removed, kept) = blast(true, true);
    assert!(kept > 0 && kept * 2 < removed, "decay keeps about 1/4 at power 4: {kept} of {removed}");
    assert_eq!(blast(false, false).1, 0, "block_drops off silences even undecayed blasts");
}

#[test]
fn tnt_in_a_blast_chains_only_with_tnt_explodes() {
    for explodes in [true, false] {
        let mut w = world();
        w.set_tile_drops(true);
        w.rules.tnt_explodes = explodes;
        let tnt = IVec3::new(2, FLOOR + 1, 0);
        w.set_block(tnt, Block::TNT);
        w.explode(DVec3::new(0.5, FLOOR as f64 + 1.5, 0.5), 4.0);
        assert_eq!(w.get_block(tnt), Some(Block::AIR), "the blast destroys the block");
        assert_eq!(w.primed_tnt.len(), usize::from(explodes), "tnt_explodes = {explodes}");
        // `TntBlock.dropFromExplosion` is false: a blasted TNT block never drops.
        assert!(
            w.drops.iter().all(|(_, s)| s.item != crate::item::Item::from_block(Block::TNT)),
            "tnt_explodes = {explodes}"
        );
    }
}

#[test]
fn random_tick_speed_zero_stops_random_ticks_and_higher_speeds_run_them() {
    // Dry farmland with no crop dries to dirt on a random tick.
    let field = |w: &mut World| {
        for x in 0..4 {
            for z in 0..4 {
                w.set_block(IVec3::new(x, FLOOR + 1, z), Block::FARMLAND);
            }
        }
    };
    let dried = |w: &World| {
        (0..4)
            .flat_map(|x| (0..4).map(move |z| IVec3::new(x, FLOOR + 1, z)))
            .filter(|&p| w.get_block(p) == Some(Block::DIRT))
            .count()
    };
    let mut still = world();
    field(&mut still);
    for _ in 0..40 {
        still.tick_random_rules(0.05, DVec3::new(0.5, 142.0, 0.5), 0, true);
    }
    assert_eq!(dried(&still), 0, "random_tick_speed 0");
    let mut fast = world();
    field(&mut fast);
    for _ in 0..40 {
        fast.tick_random_rules(0.05, DVec3::new(0.5, 142.0, 0.5), 3000, true);
    }
    assert_eq!(dried(&fast), 16, "a high random_tick_speed ticks every cell");
}
