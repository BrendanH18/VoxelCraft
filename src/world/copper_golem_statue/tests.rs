//! Expected facts from the pinned Java 26.3 bytecode/data, not a model dump.
use super::*;
use crate::{
    crafting::Grid,
    inventory::Inventory,
    item::{Item, Tier, ToolKind},
    mining,
    world::{
        chunk::ChunkData,
        redstone_blocks as r, shape,
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};
use std::sync::Arc;
const P: IVec3 = IVec3::new(8, 140, 8);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
    for y in 4..7 {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w
}
#[test]
fn reserved_layout_names_items_and_all_256_properties() {
    assert_eq!((FIRST, END), (14336, 14592));
    let palette: Vec<_> = Block::creative_palette().collect();
    for id in FIRST..END {
        let b = Block(id);
        let s = decode(b).unwrap();
        assert_eq!(block(s), b);
        assert_eq!(b.name(), name(s));
        assert_eq!(b.shaped(), Some(super::super::block::Shaped::Custom));
        assert_eq!(b.is_waterlogged(), s.waterlogged);
        assert!(b.waterloggable());
        assert_eq!(b.with_water(!s.waterlogged), block(Statue { waterlogged: !s.waterlogged, ..s }));
        assert_eq!(b.with_facing(Facing::West), block(Statue { facing: Facing::West, ..s }));
        assert_eq!(b.base(), item_block(s.stage, s.waxed));
        assert_eq!(b.oriented().map(|(_, f)| f), Some(s.facing));
        let item = Item::from_block(b.base());
        assert!(item.is_valid());
        assert_eq!(Item::from_name(&b.name().replace(' ', "_")), Some(item));
        assert_eq!(Block::from_name(b.name()), Some(b.base()));
        assert_eq!(palette.iter().filter(|&&p| p == b.base()).count(), 1);
        assert_eq!(crate::session::interact::picked_item(b), item);
        assert_eq!(b.drop(), Some(item));
        assert_eq!(b.hardness(), 3.0);
        assert_eq!(b.best_tool(), Some(ToolKind::Pickaxe));
        assert_eq!(b.harvest_level(), None); // Java does not require correct tool.
        assert!(mining::can_harvest(b, None));
        assert_eq!(super::super::explosion::resistance(b), 6.0);
        assert!(b.is_solid() && b.is_targetable() && !b.is_opaque());
        assert_eq!((b.emission(), b.light_opacity()), (0, u8::from(s.waterlogged)));
        assert!(!b.uses_shape_for_light_occlusion());
    }
    assert!(decode(Block(FIRST - 1)).is_none() && decode(Block(END)).is_none());
    // The separate reserved item slots are deliberately unused.
    assert!((1360..1368).all(|id| !Item(id).is_valid()));
}
#[test]
fn pose_outputs_and_pure_player_transition_contract() {
    assert_eq!(Pose::ALL.map(Pose::name), ["standing", "sitting", "running", "star"]);
    assert_eq!(Pose::ALL.map(Pose::comparator_output), [1, 2, 3, 4]);
    assert_eq!(Pose::from_id(4), Pose::Standing);
    for id in FIRST..END {
        let s = decode(Block(id)).unwrap();
        assert_eq!(s.cycle_pose().cycle_pose().cycle_pose().cycle_pose(), s);
        assert_eq!(s.cycle_pose().pose as u8, (s.pose as u8 + 1) % 4);
        if s.waxed {
            assert!(s.wax().is_none());
            assert_eq!(s.scrape(), Some(Statue { waxed: false, ..s }));
        } else {
            assert_eq!(s.wax(), Some(Statue { waxed: true, ..s }));
            assert_eq!(s.scrape().map(|s| s.stage), s.stage.previous());
        }
    }
}
#[test]
fn fixed_java_shape_and_original_pose_models() {
    for id in FIRST..END {
        let b = Block(id);
        let s = decode(b).unwrap();
        assert_eq!(
            shape::collision(b, |_| Block::AIR, Block::AIR).as_slice(),
            &[Box16 { min: [3, 0, 3], max: [13, 14, 13] }]
        );
        assert_eq!(shape::outline(b, |_| Block::AIR, Block::AIR).as_slice(), collision().as_slice());
        let model = shape::shape(b, |_| Block::AIR, Block::AIR);
        assert_eq!(model.as_slice(), mesh(s).as_slice());
        assert_eq!(model.as_slice().len(), 8);
        for bx in model.as_slice() {
            assert!((0..3).all(|i| bx.min[i] < bx.max[i] && bx.max[i] <= 16));
        }
    }
    let models = Pose::ALL.map(|pose| mesh(Statue { pose, ..Statue::new(Stage::Oxidized, false) }));
    for a in 0..4 {
        for b in a + 1..4 {
            assert_ne!(models[a].as_slice(), models[b].as_slice());
        }
    }
}
#[test]
fn waxing_reference_recipes_and_pose_bearing_loot() {
    let mut w = world();
    for stage in Stage::ALL {
        let plain = item_block(stage, false);
        let waxed = item_block(stage, true);
        let mut grid = Grid::new(2);
        grid.cells[0] = Some(Stack::new(plain, 1));
        grid.cells[3] = Some(Stack::new(Item::HONEYCOMB, 1));
        assert_eq!(grid.result(), Some(Stack::new(waxed, 1)));
        grid.cells[0] = Some(Stack::new(waxed, 1));
        assert_eq!(grid.result(), None);
    }
    for id in FIRST..END {
        let b = Block(id);
        let s = decode(b).unwrap();
        for held in [None, Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Diamond), 1))] {
            w.drops.clear();
            w.spill_with_item(P, b, held);
            assert_eq!(w.drops, [(P, s.item_stack(Default::default()))]);
        }
    }
}
#[test]
fn block_and_item_components_save_reload_all_states() {
    let mut w = world();
    let pos = |i: u16| P + IVec3::new((i % 16) as i32, 0, (i / 16) as i32);
    for id in FIRST..END {
        w.edit_exact(pos(id - FIRST), Block(id), false);
    }
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("target/statue-save-{}", std::process::id()));
    let storage = Storage::new(&dir);
    storage.save(&LevelInfo { seed: 263, player: None, props: Default::default() }, &w.modified_chunks()).unwrap();
    let mut loaded = world();
    for (p, data) in storage.load_chunks().unwrap() {
        loaded.insert_chunk(p, data, true);
    }
    for id in FIRST..END {
        assert_eq!(loaded.get_block(pos(id - FIRST)), Some(Block(id)));
    }
    std::fs::remove_dir_all(dir).unwrap();
    let mut inv = Inventory::default();
    for pose in Pose::ALL {
        let s = Statue { pose, ..Statue::new(Stage::Oxidized, true) };
        let mut stack = s.item_stack(Default::default());
        stack.name = StackName::new("Copper friend").unwrap();
        inv.slots[pose as usize] = Some(stack);
        assert_eq!(item_pose(stack), Some(pose));
    }
    assert_eq!(Inventory::deserialize(&inv.serialize()).unwrap().slots, inv.slots);
}
#[test]
fn mixed_copper_neighborhood_oxidation_and_wax_immunity() {
    use super::super::copper::{self, Family};
    let mut w = world();
    let s =
        Statue { facing: Facing::East, pose: Pose::Running, waterlogged: true, ..Statue::new(Stage::Exposed, false) };
    w.set_block(P, block(s));
    w.set_block(P + IVec3::X * 4, Block::COPPER_BLOCK);
    for _ in 0..3000 {
        w.random_tick(P);
    }
    assert_eq!(w.get_block(P), Some(block(s)), "younger copper at Manhattan 4 inhibits statue aging");
    w.set_block(P + IVec3::X * 4, Block::AIR);
    for _ in 0..3000 {
        w.random_tick(P);
    }
    assert_eq!(w.get_block(P), Some(block(Statue { stage: Stage::Oxidized, ..s })));
    // Reciprocal inhibition: a younger statue prevents ordinary copper aging.
    w.set_block(P, copper::block(Family::Cut, Stage::Exposed, 0));
    let young = Statue::new(Stage::Unaffected, false);
    w.set_block(P + IVec3::X, block(young));
    for _ in 0..3000 {
        w.random_tick(P);
    }
    assert_eq!(w.get_block(P), Some(copper::block(Family::Cut, Stage::Exposed, 0)));
    w.set_block(P + IVec3::X, block(Statue { waxed: true, ..young }));
    for _ in 0..3000 {
        w.random_tick(P);
    }
    assert_eq!(w.get_block(P), Some(copper::block(Family::Cut, Stage::Oxidized, 0)));
    w.set_block(P, block(Statue { waxed: true, ..s }));
    for _ in 0..3000 {
        w.random_tick(P);
    }
    assert_eq!(w.get_block(P), Some(block(Statue { waxed: true, ..s })));
}
#[test]
fn lightning_cleans_statues_and_walks_across_mixed_families() {
    use super::super::copper::{self, Family};
    let mut w = world();
    let s = Statue { pose: Pose::Star, waterlogged: true, ..Statue::new(Stage::Oxidized, false) };
    w.set_block(P, block(s));
    w.clean_lightning_copper(P);
    assert_eq!(w.get_block(P), Some(block(Statue { stage: Stage::Unaffected, ..s })));
    // Dense neighborhood ensures a deterministic walk meets unwaxed statues;
    // center is waxed, which starts walks but is itself immune in Java 26.3.
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                w.set_block(P + IVec3::new(dx, dy, dz), block(s));
            }
        }
    }
    let waxed = block(Statue { waxed: true, ..s });
    w.set_block(P, waxed);
    w.clean_lightning_copper(P);
    assert_eq!(w.get_block(P), Some(waxed));
    assert!((-1..=1).any(|dx| (-1..=1).any(|dy| (-1..=1).any(|dz| {
        w.get_block(P + IVec3::new(dx, dy, dz)).and_then(decode).is_some_and(|n| !n.waxed && n.stage != Stage::Oxidized)
    }))));
    w.set_block(P, copper::block(Family::Cut, Stage::Oxidized, 0));
    w.clean_lightning_copper(P);
    assert_eq!(w.get_block(P), Some(copper::block(Family::Cut, Stage::Unaffected, 0)));
}
#[test]
fn live_comparator_updates_after_every_pose_change_and_removal() {
    let mut w = world();
    let comparator = P + IVec3::X;
    w.set_block(comparator - IVec3::Y, Block::STONE);
    w.set_block(comparator, r::comparator(Facing::East, false, false));
    for pose in Pose::ALL {
        w.set_block(P, block(Statue { pose, ..Statue::new(Stage::Oxidized, true) }));
        for _ in 0..4 {
            w.tick_redstone();
        }
        assert_eq!(w.redstone_power(comparator + IVec3::X), pose.comparator_output());
    }
    w.set_block(P, Block::AIR);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert_eq!(w.redstone_power(comparator + IVec3::X), 0);
}

#[test]
fn statue_chunk_mesh_preserves_high_texture_bits_and_bounded_geometry() {
    let mut w = world();
    for id in FIRST..END {
        let i = id - FIRST;
        w.edit_exact(IVec3::new(2 * (i % 16) as i32, 140, 2 * (i / 16) as i32), Block(id), false);
    }
    w.update_block_light();
    let input = w.mesh_input(IVec3::new(0, 4, 0));
    let mut region = crate::mesh::Region::default();
    let start = std::time::Instant::now();
    let mut quads = 0;
    for _ in 0..16 {
        let mesh = crate::mesh::build(&input, &mut region);
        quads = mesh.quads.len();
        assert!(quads > 256 * 12 && quads <= 256 * 8 * 6 + 256 * 6);
        for q in mesh.quads.iter().filter(|q| q[1] >> 31 != 0) {
            let layer = (q[1] & 255)
                | ((q[0] >> 5 & 1) << 8)
                | ((q[0] >> 11 & 1) << 9)
                | ((q[0] >> 17 & 1) << 10)
                | ((q[0] >> 30 & 1) << 11);
            assert!((u32::from(TEXTURE_FIRST)..u32::from(TEXTURE_END)).contains(&layer));
        }
    }
    eprintln!("statue mesh: 256 states, {quads} quads, 16 builds in {:?}", start.elapsed());
}
