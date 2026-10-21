//! BLD-04f rules in a world: placing into water, buckets, flow through
//! shapes, source conversion, explosions, coral and connections.
use super::*;
use crate::survival_items::bucket::{self, Effect};
use crate::world::{World, chunk::ChunkData, overworld_blocks as ob, shape, terrain::Generator};
use crate::{item::Item, world::block::Facing};
use glam::DVec3;
use std::sync::Arc;

const P: IVec3 = IVec3::new(8, 140, 8);

/// An empty chunk with a stone floor under `P`'s layer.
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    for x in 2..15 {
        for z in 2..15 {
            w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
        }
    }
    w
}

fn water_ticks(w: &mut World, n: usize) {
    for _ in 0..n {
        w.tick_fluids(0.25);
    }
}

#[test]
fn blocks_placed_into_water_hold_it_and_leave_it_when_broken() {
    for block in [Block::OAK_FENCE, Block::STONE_STAIRS, Block::slab_of(Block::STONE).unwrap(), Block::IRON_BARS] {
        let mut w = world();
        w.set_block(P, Block::WATER);
        w.set_block(P, block);
        assert_eq!(w.get_block(P), Some(block.with_water(true)), "{}", block.name());
        assert!(w.get_block(P).unwrap().is_waterlogged());
        w.set_block(P, Block::AIR);
        assert_eq!(w.get_block(P), Some(Block::WATER), "{}", block.name());
        // On dry ground it stays dry.
        w.set_block(P + IVec3::X * 3, block);
        assert_eq!(w.get_block(P + IVec3::X * 3), Some(block));
    }
}

#[test]
fn buckets_fill_and_drain_waterloggable_blocks_and_lava_does_neither() {
    let mut w = world();
    w.set_block(P, Block::OAK_FENCE);
    assert_eq!(bucket::empty_at(&mut w, P, Block::WATER, None), Some(Effect::Poured { at: P, fluid: Block::WATER }));
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE.with_water(true)));
    assert_eq!(bucket::pick_up(&mut w, P), Some((Item::WATER_BUCKET, Block::WATER)));
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE));
    assert_eq!(bucket::pick_up(&mut w, P), None, "a dry fence has nothing to give");
    assert_eq!(bucket::empty_at(&mut w, P, Block::LAVA, None), None);
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE));
    // `fill_waterlogged` and `drain_waterlogged` report whether anything changed.
    assert!(w.fill_waterlogged(P) && !w.fill_waterlogged(P));
    assert!(w.drain_waterlogged(P) && !w.drain_waterlogged(P));
}

#[test]
fn water_crosses_only_the_faces_a_shape_leaves_open() {
    // Stairs with their front (low step) south: Java facing north, so the
    // back and the bottom are whole faces and the front and top are not.
    let stairs = Block::STONE_STAIRS.with_facing(Facing::South);
    let get = |q: IVec3| Some(if q == P { stairs } else { Block::AIR });
    assert!(!flow_passes(get, P, IVec3::NEG_Z) && !flow_passes(get, P, IVec3::NEG_Y));
    assert!(flow_passes(get, P, IVec3::Z) && flow_passes(get, P, IVec3::Y));
    assert!(flow_passes(get, P, IVec3::X), "the sides are half open");
    // A fence post leaves every side open; a bottom slab closes only its underside.
    let get = |q: IVec3| Some(if q == P { Block::OAK_FENCE } else { Block::AIR });
    assert!(
        [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y, IVec3::NEG_Y]
            .iter()
            .all(|&d| flow_passes(get, P, d))
    );
    let slab = Block::slab_of(Block::STONE).unwrap();
    let get = |q: IVec3| Some(if q == P { slab } else { Block::AIR });
    assert!(!flow_passes(get, P, IVec3::NEG_Y) && flow_passes(get, P, IVec3::Z));
    // Two plain blocks are never judged here.
    assert!(flow_passes(|_| Some(Block::AIR), P, IVec3::X));
}

#[test]
fn waterlogged_stairs_spill_only_through_their_open_front() {
    // A channel one block wide, closed behind the stairs.
    let mut w = world();
    for z in -2..4 {
        for x in [-1, 1] {
            w.set_block(P + IVec3::new(x, 0, z), Block::STONE);
        }
    }
    w.set_block(P - IVec3::Z * 2, Block::STONE);
    w.set_block(P, Block::STONE_STAIRS.with_facing(Facing::South).with_water(true));
    water_ticks(&mut w, 12);
    assert_eq!(w.get_block(P - IVec3::Z), Some(Block::AIR), "the whole back face holds the water in");
    assert!(w.get_block(P + IVec3::Z).is_some_and(Block::is_water), "the front lets it out");
    assert!(w.get_block(P).unwrap().is_waterlogged(), "the stairs keep their own water");
}

#[test]
fn two_sources_waterlog_a_block_and_flowing_water_never_does() {
    let mut w = world();
    w.set_block(P, Block::OAK_FENCE);
    w.set_block(P + IVec3::X, Block::WATER);
    water_ticks(&mut w, 6);
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE), "one source's flow doesn't fill it");
    w.set_block(P - IVec3::X, Block::WATER);
    water_ticks(&mut w, 6);
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE.with_water(true)), "a new source forms in it");
    // Without source conversion it stays dry.
    let mut w = world();
    w.rules.water_source_conversion = false;
    w.set_block(P, Block::OAK_FENCE);
    w.set_block(P + IVec3::X, Block::WATER);
    w.set_block(P - IVec3::X, Block::WATER);
    water_ticks(&mut w, 6);
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE));
}

#[test]
fn explosions_spare_blocks_that_hold_water() {
    let mut w = world();
    w.set_block(P, Block::OAK_FENCE.with_water(true));
    w.set_block(P + IVec3::X, Block::OAK_FENCE);
    w.explode_with_drops(P.as_dvec3() + DVec3::splat(0.5), 2.0, false);
    assert_eq!(w.get_block(P), Some(Block::OAK_FENCE.with_water(true)));
    assert_eq!(w.get_block(P + IVec3::X), Some(Block::AIR));
}

#[test]
fn dry_live_coral_dies_unless_water_reaches_it() {
    let fan = ob::coral(2, 0, false);
    let mut w = world();
    w.set_block(P, fan.with_water(false));
    w.set_block(P + IVec3::X * 4, fan.with_water(false));
    w.set_block(P + IVec3::X * 4 + IVec3::Z, Block::WATER);
    for _ in 0..110 {
        w.tick_redstone();
    }
    assert_eq!(w.get_block(P), Some(ob::coral(2, 0, true)));
    assert_eq!(w.get_block(P + IVec3::X * 4), Some(fan.with_water(false)), "water beside it keeps it alive");
}

#[test]
fn a_waterlogged_partner_acts_as_its_original_except_for_fire_and_light() {
    let wet = Block::OAK_FENCE.with_water(true);
    assert_ne!(wet, Block::OAK_FENCE);
    assert_eq!(wet.canonical(), Block::OAK_FENCE);
    assert_eq!(
        (wet.name(), wet.drop(), wet.hardness()),
        (Block::OAK_FENCE.name(), Block::OAK_FENCE.drop(), Block::OAK_FENCE.hardness())
    );
    assert_eq!(wet.shaped(), Block::OAK_FENCE.shaped());
    // `FireBlock.getBurnOdds` and `propagatesSkylightDown` see the water.
    assert_eq!(wet.fire_odds(), (0, 0));
    assert_ne!(Block::OAK_FENCE.fire_odds(), (0, 0));
    assert_eq!((Block::OAK_FENCE.light_opacity(), wet.light_opacity()), (0, 1));
}

#[test]
fn fences_and_walls_join_sturdy_faces_but_not_leaves() {
    let fence = Block::OAK_FENCE;
    // Stairs north of the fence: their back (Java facing south, our front
    // north) is whole; their front is not.
    assert!(shape::fence_connects(fence, Block::STONE_STAIRS.with_facing(Facing::North), Facing::North));
    assert!(!shape::fence_connects(fence, Block::STONE_STAIRS.with_facing(Facing::South), Facing::North));
    assert!(shape::fence_connects(fence, Block::STONE, Facing::East));
    assert!(!shape::fence_connects(fence, Block::LEAVES, Facing::East), "`isExceptionForConnection`");
    assert!(shape::fence_connects(fence, fence.with_water(true), Facing::East), "a wet fence is a fence");
    // `WallBlock.shouldRaisePost`: a wall in a straight run has no post in
    // the open; a fence post over its middle raises one; under a full
    // block both sides are tall and the post stays down.
    let wall = Block::COBBLESTONE_WALL;
    let run = |f: Facing| if f.along_x() { Block::AIR } else { wall };
    let post = shape::Box16 { min: [4, 0, 4], max: [12, 16, 12] };
    let open = shape::shape_with(wall, run, Block::AIR, shape::Above::NONE);
    assert!(!open.as_slice().contains(&post));
    assert!(open.as_slice().iter().all(|b| b.max[1] == 14), "low sides in the open");
    let fence_above = shape::Above { block: Block::OAK_FENCE, sides: [Block::AIR; 4] };
    let raised = shape::shape_with(wall, run, Block::AIR, fence_above);
    assert!(raised.as_slice().contains(&post));
    let roofed =
        shape::shape_with(wall, run, Block::AIR, shape::Above { block: Block::STONE, sides: [Block::STONE; 4] });
    assert!(!roofed.as_slice().contains(&post));
    assert!(roofed.as_slice().iter().all(|b| b.max[1] == 16), "tall sides under a full block");
}

#[test]
fn a_waterlogged_amethyst_bud_keeps_growing_and_keeps_its_water() {
    let mut w = world();
    let budding = IVec3::new(8, 145, 8);
    w.set_block(budding, Block(1925));
    for face in [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X] {
        w.set_block(budding + face, Block::WATER);
        w.set_block(budding + face, ob::amethyst_bud_facing(0, face));
        assert!(w.get_block(budding + face).unwrap().is_waterlogged());
    }
    for _ in 0..2000 {
        w.tick_overworld_flora(budding, Block(1925));
    }
    // Every bud has reached a cluster and is still wet.
    for face in [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X] {
        let bud = w.get_block(budding + face).unwrap();
        assert_eq!(bud, ob::amethyst_bud_facing(3, face).with_water(true), "{face}");
    }
}

#[test]
fn a_waterlogged_chest_lets_its_water_out_through_its_inset_sides() {
    let chest = Block::CHEST;
    let get = |q: IVec3| Some(if q == P { chest.with_water(true) } else { Block::AIR });
    assert!(flow_passes(get, P, IVec3::X) && flow_passes(get, P, IVec3::Y));
    let mut w = world();
    w.set_block(P, Block::WATER);
    w.set_block(P, chest);
    water_ticks(&mut w, 6);
    assert!(w.get_block(P + IVec3::X).is_some_and(Block::is_water));
}

#[test]
fn a_bucket_emptied_into_a_wet_block_changes_nothing_like_java() {
    let mut w = world();
    let wet = Block::OAK_FENCE.with_water(true);
    w.set_block(P, wet);
    // `canPlaceLiquid` ignores the waterlogged state and `placeLiquid` then
    // does nothing; `BucketItem.emptyContents` still reports success.
    assert_eq!(
        bucket::empty_contents(&mut w, P, IVec3::Y, Block::WATER, false),
        Some(Effect::Poured { at: P, fluid: Block::WATER })
    );
    assert_eq!(w.get_block(P), Some(wet));
    assert_eq!(w.get_block(P + IVec3::Y), Some(Block::AIR));
}

#[test]
fn a_wet_rail_keeps_its_water_through_shape_changes() {
    use crate::world::block::RailShape;
    let wet = Block::rail(RailShape::NorthSouth).with_water(true);
    assert_eq!(crate::world::rails::with_shape(wet, RailShape::NorthSouth), wet);
    assert_eq!(
        crate::world::rails::with_shape(wet, RailShape::EastWest),
        Block::rail(RailShape::EastWest).with_water(true)
    );
}

#[test]
fn flowing_fluids_leave_dry_waterloggable_plants_alone() {
    let dead_fan = ob::coral(2, 0, true);
    assert!(dead_fan.kind() == crate::world::block::RenderKind::Cross && dead_fan.waterloggable());
    for fluid in [Block::WATER, Block::LAVA] {
        let mut w = world();
        w.set_block(P + IVec3::X, dead_fan);
        w.set_block(P, fluid);
        water_ticks(&mut w, 40);
        assert_eq!(w.get_block(P + IVec3::X), Some(dead_fan), "{}", fluid.name());
    }
}

#[test]
fn a_pending_coral_death_survives_save_and_reload() {
    let dry = ob::coral(2, 0, false).with_water(false);
    let mut w = world();
    w.set_block(P, dry);
    let saved = w.redstone_to_string();
    w.load_redstone(&saved);
    for _ in 0..110 {
        w.tick_redstone();
    }
    assert_eq!(w.get_block(P), Some(ob::coral(2, 0, true)));
}

#[test]
fn water_beside_dry_hanging_roots_still_spreads_the_other_ways() {
    // A source on stone, dry hanging roots east over a hole: Java's
    // `getSpread` can't route through the roots, so the other sides fill.
    let roots = Block(1949);
    assert!(roots.waterloggable() && !roots.is_waterlogged() && roots.is_replaceable());
    let mut w = world();
    let east = P + IVec3::X;
    w.set_block(east + IVec3::Y, Block::STONE);
    w.set_block(east, roots);
    w.set_block(east - IVec3::Y, Block::AIR);
    w.set_block(P, Block::WATER);
    water_ticks(&mut w, 8);
    assert_eq!(w.get_block(east), Some(roots));
    for d in [IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
        assert!(w.get_block(P + d).is_some_and(Block::is_water), "{d}");
    }
}
