use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::*;
use crate::item::Item;
use crate::physics::test_util::Grid;
use crate::world::chunk::{ChunkData, chunk_of};
use crate::world::terrain::Generator;

const P: IVec3 = IVec3::new(4, 145, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w
}

/// A pool: stone floor with `base` at `P` and `height` water sources above it.
fn pool(base: Block, height: i32) -> World {
    let mut w = world();
    for dx in -3..=3 {
        for dz in -3..=3 {
            w.edit(P + IVec3::new(dx, -1, dz), Block::STONE, false);
        }
    }
    w.edit(P, base, false);
    for y in 1..=height {
        w.edit(P + IVec3::Y * y, Block::WATER, false);
    }
    w
}

fn run(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.tick_fluids(0.25);
    }
}

#[test]
fn a_column_is_water_for_every_fluid_rule() {
    for b in [UP, DOWN] {
        assert!(b.is_water() && b.is_fluid() && !b.is_lava());
        assert_eq!(
            (b.fluid(), b.fluid_level(), b.water_level()),
            (Some(crate::world::block::Fluid::Water), Some(0), Some(0))
        );
        assert_eq!(b.fluid_drop(), Block::WATER.fluid_drop());
        assert!(b.holds_water() && b.is_replaceable() && !b.is_solid() && !b.is_targetable());
        assert_eq!(b.name(), "bubble column");
        assert_eq!(b.kind(), crate::world::block::RenderKind::Translucent);
    }
    assert_ne!(UP.info().tex[0], DOWN.info().tex[0]);
    assert!(drags(DOWN) && !drags(UP));
    assert_eq!(column_above(Block::SOUL_SAND), Some(UP));
    assert_eq!(column_above(Block::MAGMA), Some(DOWN));
    assert_eq!(column_above(Block::STONE), None);
}

#[test]
fn soul_sand_and_magma_start_columns_through_every_water_source_above() {
    for (base, column) in [(Block::SOUL_SAND, UP), (Block::MAGMA, DOWN)] {
        let mut w = pool(base, 5);
        w.edit(P + IVec3::Y * 6, Block::flowing_water(3), false);
        run(&mut w, 2);
        for y in 1..=5 {
            assert_eq!(w.get_block(P + IVec3::Y * y), Some(column), "{base:?} y+{y}");
        }
        assert_eq!(w.get_block(P + IVec3::Y * 6), Some(Block::flowing_water(3)), "flowing water is no column");
        assert_eq!(w.get_block(P), Some(base));
    }
}

#[test]
fn water_that_is_not_a_source_over_the_base_makes_no_column() {
    let mut w = pool(Block::SOUL_SAND, 0);
    w.edit(P + IVec3::Y, Block::flowing_water(2), false);
    w.edit(P + IVec3::Y * 2, Block::AIR, false);
    run(&mut w, 3);
    assert_eq!(w.get_block(P + IVec3::Y), Some(Block::flowing_water(2)));
}

#[test]
fn a_column_follows_its_base_and_the_water_above() {
    let mut w = pool(Block::SOUL_SAND, 3);
    run(&mut w, 2);
    assert_eq!(w.get_block(P + IVec3::Y * 3), Some(UP));
    // New water on top joins the column.
    w.edit(P + IVec3::Y * 4, Block::WATER, false);
    run(&mut w, 2);
    assert_eq!(w.get_block(P + IVec3::Y * 4), Some(UP));
    // Swapping the base reverses it for the whole column.
    w.edit(P, Block::MAGMA, false);
    run(&mut w, 2);
    assert!((1..=4).all(|y| w.get_block(P + IVec3::Y * y) == Some(DOWN)));
    // A block in the middle cuts the column above it back to water.
    w.edit(P + IVec3::Y * 2, Block::STONE, false);
    run(&mut w, 2);
    assert_eq!(w.get_block(P + IVec3::Y), Some(DOWN));
    assert_eq!(w.get_block(P + IVec3::Y * 3), Some(Block::WATER));
    assert_eq!(w.get_block(P + IVec3::Y * 4), Some(Block::WATER));
    // Breaking the base turns the rest back into water.
    w.edit(P + IVec3::Y * 2, Block::WATER, false);
    w.edit(P, Block::STONE, false);
    run(&mut w, 2);
    assert!((1..=4).all(|y| w.get_block(P + IVec3::Y * y) == Some(Block::WATER)));
    assert_eq!(w.active_fluids(), 0, "the work queue drains");
}

#[test]
fn buckets_take_a_column_cell_as_water_and_the_rest_reverts() {
    let mut w = pool(Block::SOUL_SAND, 3);
    run(&mut w, 2);
    let taken = crate::survival_items::bucket::pick_up(&mut w, P + IVec3::Y).unwrap();
    assert_eq!(taken, (Item::WATER_BUCKET, Block::WATER));
    assert_eq!(w.get_block(P + IVec3::Y), Some(Block::AIR));
    run(&mut w, 2);
    assert_eq!(w.get_block(P + IVec3::Y * 2), Some(Block::WATER), "no base, no column");
}

#[test]
fn flow_treats_a_column_cell_as_a_source() {
    let mut w = pool(Block::SOUL_SAND, 1);
    run(&mut w, 2);
    assert_eq!(w.get_block(P + IVec3::Y), Some(UP));
    let beside = P + IVec3::Y + IVec3::X;
    w.edit(beside - IVec3::Y, Block::STONE, false);
    w.wake_fluids(P + IVec3::Y);
    run(&mut w, 3);
    assert_eq!(w.get_block(beside), Some(Block::flowing_water(1)), "a column spreads like a source");
    assert_eq!(w.get_block(P + IVec3::Y), Some(UP), "and stays a column");
}

#[test]
fn lava_hardens_beside_a_column_like_any_water() {
    let mut w = pool(Block::SOUL_SAND, 1);
    run(&mut w, 2);
    w.edit(P + IVec3::Y + IVec3::X, Block::LAVA, false);
    w.wake_fluids(P + IVec3::Y + IVec3::X);
    run(&mut w, 8);
    assert_eq!(w.get_block(P + IVec3::Y + IVec3::X), Some(Block::OBSIDIAN));
}

#[test]
fn a_block_set_over_a_column_leaves_no_water() {
    let mut w = pool(Block::SOUL_SAND, 2);
    run(&mut w, 2);
    w.set_block(P + IVec3::Y, Block::STONE);
    assert_eq!(w.get_block(P + IVec3::Y), Some(Block::STONE));
    // A waterloggable block keeps the column's water (`waterlog::carried`).
    w.set_block(P + IVec3::Y, Block::AIR);
    w.edit(P + IVec3::Y, UP, false);
    let fence = Block::from_name("oak_fence").unwrap();
    w.edit(P + IVec3::Y, fence, false);
    assert!(w.get_block(P + IVec3::Y).unwrap().is_waterlogged());
}

#[test]
fn entities_are_lifted_and_dragged_at_the_familiar_speeds() {
    assert_eq!(push(0.0, false, false), 0.06);
    assert_eq!(push(0.69, false, false), 0.7);
    assert_eq!(push(0.0, false, true), 0.1);
    assert_eq!(push(1.75, false, true), 1.8);
    assert_eq!(push(0.0, true, false), -0.03);
    assert_eq!(push(-0.29, true, false), -0.3);
    assert_eq!(push(-0.88, true, true), -0.9);

    let mut g = Grid::flat(0);
    g.set(IVec3::new(0, 0, 0), UP);
    g.set(IVec3::new(0, 1, 0), UP);
    let (min, max) = (DVec3::new(0.2, 0.0, 0.2), DVec3::new(0.8, 1.8, 0.8));
    // Two cells overlap the box; the top one has air above it.
    let dy = apply(|p| crate::physics::BlockSource::block(&g, p), min, max, 0.0).unwrap();
    assert!((dy - (0.06 + 0.1)).abs() < 1e-12);
    // Outside the column nothing happens.
    let (min, max) = (DVec3::new(3.2, 0.0, 0.2), DVec3::new(3.8, 1.8, 0.8));
    assert_eq!(apply(|p| crate::physics::BlockSource::block(&g, p), min, max, 0.0), None);
}

#[test]
fn a_player_rises_in_an_upward_column_and_sinks_in_a_downward_one() {
    for (column, rises) in [(UP, true), (DOWN, false)] {
        let mut g = Grid::flat(0);
        for y in -3..8 {
            g.set(IVec3::new(0, y, 0), column);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                g.set(IVec3::new(dx, y, dz), Block::WATER);
            }
        }
        let mut player = crate::player::Player::new(DVec3::new(0.5, 2.0, 0.5));
        player.flying = false;
        let start = player.pos.y;
        for _ in 0..20 {
            player.tick(crate::player::MoveInput::default(), &g);
        }
        assert_eq!(player.pos.y > start, rises, "{column:?}: {} from {start}", player.pos.y);
    }
}

#[test]
fn eyes_in_a_column_keep_their_air() {
    let mut g = Grid::flat(0);
    for y in 0..6 {
        g.set(IVec3::new(0, y, 0), UP);
    }
    let player = crate::player::Player::new(DVec3::new(0.5, 1.0, 0.5));
    assert!(player.head_in_water(&g), "still under water");
    assert!(player.eye_in_bubble_column(&g));
    let mut g = Grid::flat(0);
    for y in 0..6 {
        g.set(IVec3::new(0, y, 0), Block::WATER);
    }
    assert!(!player.eye_in_bubble_column(&g));
}

#[test]
fn generated_water_over_magma_and_soul_sand_seeds_columns() {
    let mut blocks = ChunkData::new_dense(Block::STONE);
    let at = |x: usize, y: usize, z: usize| crate::world::chunk::index(x, y, z);
    blocks[at(5, 10, 6)] = Block::MAGMA;
    blocks[at(5, 11, 6)] = Block::WATER;
    blocks[at(5, 12, 6)] = Block::WATER;
    blocks[at(9, 3, 9)] = Block::SOUL_SAND;
    blocks[at(9, 4, 9)] = Block::AIR;
    blocks[at(2, 20, 2)] = Block::MAGMA;
    blocks[at(2, 21, 2)] = Block::LAVA;
    let data = ChunkData::from_dense(blocks);
    let origin = IVec3::new(32, 64, 0);
    assert_eq!(seeds(&data, origin), vec![origin + IVec3::new(5, 11, 6)]);
    assert!(seeds(&ChunkData::Uniform(Block::WATER), origin).is_empty());

    // Installing the chunk queues the seed; the first tick builds the column.
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    let cpos = chunk_of(origin);
    w.insert_chunk(cpos, Arc::new(data), false);
    assert!(w.active_fluids() > 0);
    run(&mut w, 2);
    assert_eq!(w.get_block(origin + IVec3::new(5, 11, 6)), Some(DOWN));
    assert_eq!(w.get_block(origin + IVec3::new(5, 12, 6)), Some(DOWN));
}

#[test]
fn columns_save_and_reload() {
    let mut w = pool(Block::SOUL_SAND, 2);
    w.edit(P + IVec3::X * 2, Block::MAGMA, false);
    w.edit(P + IVec3::X * 2 + IVec3::Y, Block::WATER, false);
    run(&mut w, 2);
    let dir = std::env::temp_dir().join(format!("voxelcraft-bubble-{}", std::process::id()));
    let store = crate::world::storage::Storage::new(&dir);
    let cpos = chunk_of(P);
    let data = w.chunks.get(&cpos).unwrap().data.clone();
    store
        .save(&crate::world::storage::LevelInfo { seed: 7, player: None, props: Default::default() }, &[(cpos, data)])
        .unwrap();
    let loaded = store.load_chunks().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let l = crate::world::chunk::local_of(P);
    let get = |d: &ChunkData, dx: i32, dy: i32| d.get((l.x + dx) as usize, (l.y + dy) as usize, l.z as usize);
    let d = &loaded[&cpos];
    assert_eq!((get(d, 0, 1), get(d, 0, 2)), (UP, UP));
    assert_eq!(get(d, 2, 1), DOWN);
}

/// `cargo test --release --lib bubble_seed_scan_cost -- --ignored --nocapture`:
/// what the seed scan adds to generating real chunks.
#[test]
#[ignore]
fn bubble_seed_scan_cost() {
    let generator = Generator::new(11);
    let mut generated = Vec::new();
    let t = std::time::Instant::now();
    for cx in -4..4 {
        for cz in -4..4 {
            for cy in -2..5 {
                let pos = IVec3::new(cx, cy, cz);
                generated.push((pos, generator.generate(pos)));
            }
        }
    }
    let gen_time = t.elapsed();
    let t = std::time::Instant::now();
    let mut found = 0;
    for (pos, data) in &generated {
        found += seeds(data, *pos * crate::world::chunk::CHUNK_SIZE_I).len();
    }
    let scan = t.elapsed();
    println!(
        "BUBBLE chunks={} generate={:?} scan={:?} ({:.1}% of generation) seeds={found}",
        generated.len(),
        gen_time,
        scan,
        100.0 * scan.as_secs_f64() / gen_time.as_secs_f64()
    );
}
