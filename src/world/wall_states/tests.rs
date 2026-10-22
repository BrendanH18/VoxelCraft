//! Fixtures derived from the Java 26.3 WallBlock methods listed in the
//! verification record; expected posts and arm heights are explicit.
use super::*;
use crate::world::{
    block::Shaped,
    chunk::ChunkData,
    mount_states as ms, redstone_blocks as r,
    storage::{LevelInfo, Storage},
    terrain::Generator,
};
use crate::{item::Item, session::interact::picked_item};
use std::sync::Arc;

const P: IVec3 = IVec3::new(8, 140, 8);
const POST: shape::Box16 = shape::Box16 { min: [4, 0, 4], max: [12, 16, 12] };
const WALL: Block = Block::COBBLESTONE_WALL;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    for y in 4..7 {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w
}
fn boxes(w: &World, p: IVec3, collision: bool) -> shape::Boxes {
    let block = w.get_block(p).unwrap();
    let side = |f: Facing| w.get_block(p + f.offset()).unwrap_or(Block::AIR);
    let above = shape::Above::read(block, |d| w.get_block(p + d).unwrap_or(Block::AIR));
    if collision {
        shape::collision_with(block, side, Block::AIR, above)
    } else {
        shape::outline_with(block, side, Block::AIR, above)
    }
}
fn stack(w: &mut World, height: i32, wet: bool) {
    for y in 0..height {
        let p = P + IVec3::Y * y;
        w.set_block(p - IVec3::Z, WALL);
        w.set_block(p + IVec3::Z, WALL);
        if wet {
            w.set_block(p, Block::WATER);
        }
        w.set_block(p, WALL);
    }
}
fn posts(w: &World, height: i32, expected: bool) {
    for y in 0..height {
        let p = P + IVec3::Y * y;
        assert_eq!(up(w.get_block(p).unwrap()), Some(expected), "UP at {p}");
        assert_eq!(boxes(w, p, false).as_slice().contains(&POST), expected, "outline at {p}");
        let collision_post = shape::Box16 { min: [4, 0, 4], max: [12, 24, 12] };
        assert_eq!(boxes(w, p, true).as_slice().contains(&collision_post), expected, "collision at {p}");
    }
}

#[test]
fn append_only_post_states_preserve_material_items_water_and_light() {
    for (i, &old) in FAMILIES.iter().enumerate() {
        assert_eq!(up(old), Some(true));
        for wet in [false, true] {
            let old = old.with_water(wet);
            let new = with_up(old, false);
            assert_eq!(new.0, FIRST + i as u16 * 2 + wet as u16);
            assert_eq!(up(new), Some(false));
            assert_eq!(with_up(new, true), old);
            assert_eq!(new.with_water(!wet), with_up(old.with_water(!wet), false));
            assert_eq!(new.is_waterlogged(), wet);
            assert_eq!(new.name(), old.name());
            assert_eq!(new.shaped(), Some(Shaped::Wall));
            assert_eq!(new.kind(), old.kind());
            assert_eq!(new.info().tex, old.info().tex);
            assert_eq!(new.wall_base(), old.wall_base());
            assert_eq!(new.drop(), Some(Item::from_block(old.with_water(false))));
            assert_eq!(new.drop(), old.drop());
            assert_eq!(picked_item(new), picked_item(old));
            assert_eq!(new.hardness(), old.hardness());
            assert_eq!(new.best_tool(), old.best_tool());
            assert_eq!(new.harvest_level(), old.harvest_level());
            assert_eq!(new.light_opacity(), if wet { 1 } else { 0 });
            assert_eq!(new.uses_shape_for_light_occlusion(), old.uses_shape_for_light_occlusion());
            assert_eq!(new.emission(), 0);
            assert!(ms::supports_center(new, IVec3::Y));
        }
    }
    for id in END..=8127 {
        assert_eq!(Block(id).name(), "unknown");
    }
}

#[test]
fn java_low_wall_connection_masks_and_post_override_fixtures() {
    // Bits follow Facing::ALL: south, north, east, west. With LOW arms,
    // only straight pairs and the four-way cross omit the post.
    let expected = [true, true, true, false, true, true, true, true, true, true, true, true, false, true, true, false];
    for (mask, expected) in expected.into_iter().enumerate() {
        let next = reshaped(WALL, |d| {
            Facing::from_offset(d).filter(|f| mask & (1 << *f as usize) != 0).map_or(Block::AIR, |_| WALL)
        });
        assert_eq!(up(next), Some(expected), "connection mask {mask:04b}");
    }
    // An open straight run, standing torches and pressure plates, standing
    // and hanging lanterns, a full cube, fence, and both upper-wall posts.
    for (above, post, tall) in [
        (Block::AIR, false, false),
        (Block::TORCH, true, false),
        (crate::world::nether_biome_blocks::SOUL_TORCH, true, false),
        (r::torch(0, true), true, false),
        (r::torch(1, true), false, false),
        (r::plate(0, 0), true, false),
        (ms::LANTERN, true, false),
        (ms::lantern_block(false, true, false), false, false),
        (Block::STONE, false, true),
        (Block::OAK_FENCE, true, false),
        (WALL, true, false),
        (with_up(WALL, false), false, false),
    ] {
        let side = |f: Facing| if f.along_x() { Block::AIR } else { WALL };
        let upper = shape::Above { block: above, sides: [Block::AIR; 4] };
        let (_, raised) = shape::wall_state(&side, upper);
        assert_eq!(raised, post, "above {} ({})", above.name(), above.0);
        let placed = with_up(WALL, raised);
        let shape = shape::shape_with(placed, side, Block::AIR, upper);
        assert_eq!(shape.as_slice().contains(&POST), post);
        assert!(shape.as_slice().iter().filter(|&&b| b != POST).all(|b| b.max[1] == if tall { 16 } else { 14 }));
    }
    // A wall with UP=true wins even when both lower arms are TALL.
    let run = |f: Facing| if f.along_x() { Block::AIR } else { WALL };
    let upper = shape::Above { block: WALL, sides: Facing::ALL.map(run) };
    let (sides, raised) = shape::wall_state(&run, upper);
    assert!(raised);
    assert_eq!(sides, [Some(true), Some(true), None, None]);
}

#[test]
fn dry_and_waterlogged_wall_posts_propagate_down_across_chunk_boundaries() {
    for wet in [false, true] {
        let mut w = world();
        stack(&mut w, 40, wet);
        posts(&w, 40, false);
        let cap = P + IVec3::Y * 40;
        w.set_block(cap, Block::TORCH);
        posts(&w, 40, true);
        w.set_block(cap, Block::AIR);
        posts(&w, 40, false);
        w.set_block(cap, ms::LANTERN);
        posts(&w, 40, true);
        w.set_block(cap, Block::AIR);
        posts(&w, 40, false);
        // An asymmetric upper wall raises its post and every lower post.
        w.set_block(cap - IVec3::Y + IVec3::X, Block::STONE);
        posts(&w, 40, true);
        w.set_block(cap - IVec3::Y + IVec3::X, Block::AIR);
        posts(&w, 40, false);
        for y in 0..40 {
            assert_eq!(w.get_block(P + IVec3::Y * y).unwrap().is_waterlogged(), wet);
        }
    }
}

#[test]
fn wall_connections_distinguish_fences_panes_gates_and_exceptions() {
    for (neighbour, connected) in [
        (Block::OAK_FENCE, false),
        (Block::NETHER_BRICK_FENCE, false),
        (Block::IRON_BARS, true),
        (Block(640), true),
        (Block::STONE, true),
        (Block::LEAVES, false),
        (Block::PUMPKIN, false),
        (Block::MELON, false),
        (Block::FENCE_GATE.with_facing(Facing::East), false),
        (Block::FENCE_GATE.with_facing(Facing::North), true),
    ] {
        let (sides, _) =
            shape::wall_state(&|f| if f == Facing::East { neighbour } else { Block::AIR }, shape::Above::NONE);
        assert_eq!(sides[Facing::East as usize].is_some(), connected, "{} {}", neighbour.name(), neighbour.0);
    }
    assert!(!shape::fence_connects(Block::OAK_FENCE, WALL, Facing::East));
    assert!(shape::pane_connects_on(with_up(WALL.with_water(true), false), Facing::East));
}

#[test]
fn wall_stack_persists_and_keeps_updating_after_chunk_reload() {
    let mut w = world();
    stack(&mut w, 24, true);
    w.set_block(P + IVec3::Y * 24, Block::TORCH);
    posts(&w, 24, true);
    let dir = std::env::temp_dir().join(format!("voxelcraft-wall-posts-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let save = |w: &World| {
        storage.save(&LevelInfo { seed: 7, player: None, props: Default::default() }, &w.modified_chunks()).unwrap()
    };
    save(&w);
    let load = || {
        let chunks = storage.load_chunks().unwrap();
        let mut loaded = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        for (pos, data) in chunks {
            loaded.insert_chunk(pos, data, true);
        }
        loaded
    };
    let mut loaded = load();
    posts(&loaded, 24, true);
    loaded.set_block(P + IVec3::Y * 24, Block::AIR);
    posts(&loaded, 24, false);
    save(&loaded);
    let mut loaded = load();
    posts(&loaded, 24, false);
    assert!(loaded.get_block(P).unwrap().is_waterlogged());
    loaded.set_block(P + IVec3::Y * 24, Block::TORCH);
    posts(&loaded, 24, true);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "manual wall update timing; run without concurrent builds"]
fn wall_stack_update_timing() {
    let mut w = world();
    stack(&mut w, 40, false);
    let cap = P + IVec3::Y * 40;
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        w.set_block(cap, Block::TORCH);
        w.set_block(cap, Block::AIR);
    }
    println!("40-wall stack: {:.3} ms/cap edit (2,000 edits)", start.elapsed().as_secs_f64() * 1000. / 2000.);
    posts(&w, 40, false);
}
