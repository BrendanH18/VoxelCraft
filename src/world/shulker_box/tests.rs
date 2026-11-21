use super::*;
use crate::content::shulker_box as item;
use crate::item::Item;
use crate::world::terrain::Generator;
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w.set_tile_drops(true);
    w
}

const P: IVec3 = IVec3::new(3, 10, 3);

fn filled(colour: Option<DyeColor>) -> Stack {
    let mut slots = [None; SLOTS];
    slots[0] = Some(Stack::new(Item::DIAMOND, 12));
    slots[26] = Some(Stack::new(Item::APPLE, 3));
    item::with_contents(Stack::new(item::item(colour), 1), &slots)
}

#[test]
fn states_cover_seventeen_colours_and_six_facings() {
    let mut seen = std::collections::BTreeSet::new();
    for colour in std::iter::once(None).chain(DyeColor::ALL.map(Some)) {
        for facing in Dir::ALL {
            let b = block(State { colour, facing });
            assert_eq!(decode(b), Some(State { colour, facing }));
            assert!(seen.insert(b.0));
            assert_eq!(b.base(), item_state(colour));
            assert_eq!(b.drop(), Some(item::item(colour)));
            assert_eq!(b.hardness(), 2.0);
            assert!(b.is_solid());
            assert!(crate::world::chest::is_chest(b));
        }
    }
    assert_eq!(seen.len(), 102);
    assert_eq!((*seen.first().unwrap(), *seen.last().unwrap()), (FIRST, LAST));
    assert_eq!(Block::from_name("red shulker box"), Some(item_state(Some(DyeColor::Red))));
    assert_eq!(Block::from_name("shulker_box"), Some(item_state(None)));
    assert_eq!(Block::creative_palette().filter(|b| owns(b.0)).count(), 17);
    // Drawn as models only: the chunk shape is empty, the outline a cube.
    let b = item_state(None);
    assert!(crate::world::shape::shape(b, |_| Block::AIR, Block::AIR).is_empty());
    assert_eq!(crate::world::shape::outline(b, |_| Block::AIR, Block::AIR).as_slice().len(), 1);
}

#[test]
fn placed_boxes_face_away_from_the_clicked_face() {
    let raw = item_state(Some(DyeColor::Blue));
    assert_eq!(decode(placed(raw, IVec3::NEG_Z).unwrap()).unwrap().facing, Dir::North);
    assert_eq!(decode(placed(raw, IVec3::NEG_Y).unwrap()).unwrap().facing, Dir::Down);
    assert_eq!(decode(placed(raw, IVec3::X).unwrap()).unwrap().colour, Some(DyeColor::Blue));
    assert_eq!(placed(Block::STONE, IVec3::Y), None);
}

#[test]
fn breaking_keeps_contents_on_the_drop_instead_of_spilling() {
    let mut w = world();
    let stack = filled(Some(DyeColor::Lime));
    let b = item_state(Some(DyeColor::Lime));
    w.set_block(P, b);
    w.restore_shulker_box_item(P, &stack);
    assert_eq!(w.chest(P).unwrap().slots, item::contents(&stack));
    // Survival mining: removed, then its loot.
    w.drops.clear();
    w.set_block(P, Block::AIR);
    assert!(w.drops.is_empty(), "the contents never spill");
    w.spill_block(P, b);
    assert_eq!(w.drops, [(P, stack)]);
    // An explosion rolls the drop first and then removes the box.
    w.drops.clear();
    w.set_block(P, b);
    w.restore_shulker_box_item(P, &stack);
    let cells = [P];
    w.detonate(&cells, 4.0, true, true, false);
    assert_eq!(w.drops, [(P, stack)], "shulker box loot has no explosion condition");
    // An empty box drops a plain box.
    w.drops.clear();
    w.set_block(P, b);
    w.set_block(P, Block::AIR);
    w.spill_block(P, b);
    assert_eq!(w.drops, [(P, Stack::new(item::item(Some(DyeColor::Lime)), 1))]);
}

#[test]
fn creative_breaking_drops_only_a_box_that_holds_something() {
    let mut w = world();
    let b = item_state(None);
    w.set_block(P, b);
    w.drops.clear();
    w.set_block(P, Block::AIR);
    w.creative_shulker_box_drop(P, b);
    assert!(w.drops.is_empty());
    w.set_block(P, b);
    w.restore_shulker_box_item(P, &filled(None));
    w.set_block(P, Block::AIR);
    w.creative_shulker_box_drop(P, b);
    assert_eq!(w.drops, [(P, filled(None))]);
    // Unclaimed contents are forgotten next tick.
    w.set_block(P, b);
    w.restore_shulker_box_item(P, &filled(None));
    w.set_block(P, Block::AIR);
    w.clear_removed_shulker_boxes();
    w.drops.clear();
    w.spill_block(P, b);
    assert_eq!(w.drops, [(P, Stack::new(item::item(None), 1))]);
}

#[test]
fn contents_save_and_reload_with_the_chest_table() {
    let mut w = world();
    let stack = filled(Some(DyeColor::Purple));
    w.set_block(P, item_state(Some(DyeColor::Purple)));
    w.restore_shulker_box_item(P, &stack);
    let saved = w.chests_to_string();
    let mut other = world();
    other.set_block(P, item_state(Some(DyeColor::Purple)));
    other.load_chests(&saved);
    assert_eq!(other.chest(P).unwrap().slots, item::contents(&stack));
}

#[test]
fn shulker_boxes_take_no_shulker_box() {
    let mut w = world();
    w.set_block(P, item_state(None));
    w.set_block(P + IVec3::X, Block::CHEST);
    assert!(!w.container_accepts(P, &filled(None)));
    assert!(w.container_accepts(P, &Stack::new(Item::DIAMOND, 1)));
    assert!(w.container_accepts(P + IVec3::X, &filled(None)), "a chest takes a box");
    assert!(!crate::content::bundle::can_fit(&filled(None)));
}

#[test]
fn the_lid_needs_room_and_opens_over_ten_ticks() {
    let mut w = world();
    let up = item_state(None);
    w.set_block(P, up);
    assert!(w.shulker_box_opens(P));
    w.set_block(P + IVec3::Y, Block::STONE);
    assert!(!w.shulker_box_opens(P), "a block on the lid keeps it shut");
    // A box facing east opens past a block on top.
    w.set_block(P, block(State { colour: None, facing: Dir::East }));
    assert!(w.shulker_box_opens(P));
    // A bottom slab doesn't reach the half block above the lid.
    w.set_block(P, up);
    w.set_block(P + IVec3::Y, Block::slab_of(Block::STONE).unwrap());
    assert!(!w.shulker_box_opens(P), "the slab's lower half is in the way");
    w.set_block(P + IVec3::Y, Block::AIR);

    w.set_shulker_viewers(&[P]);
    assert_eq!(w.take_shulker_cues(), [Cue { pos: P, open: true }]);
    for i in 1..=10 {
        w.tick_shulker_boxes();
        assert!((w.shulker_lid(P, 1.0) - (i as f32 * 0.1).min(1.0)).abs() < 1e-5, "tick {i}");
    }
    assert_eq!(w.shulker_lids().next().unwrap().1.status, Status::Opened);
    // Still open with a second viewer; no new sound.
    w.set_shulker_viewers(&[P, P]);
    assert!(w.take_shulker_cues().is_empty());
    w.set_shulker_viewers(&[]);
    assert_eq!(w.take_shulker_cues(), [Cue { pos: P, open: false }]);
    for _ in 0..12 {
        w.tick_shulker_boxes();
    }
    assert_eq!(w.shulker_lid(P, 1.0), 0.0);
    assert_eq!(w.shulker_lids().count(), 0, "shut lids are forgotten");
}

#[test]
fn the_model_lifts_and_turns_its_lid() {
    let s = State { colour: Some(DyeColor::Red), facing: Dir::Up };
    let mut shut = Vec::new();
    quads(s, 0.0, |q| shut.push(q));
    assert_eq!(shut.len(), 12);
    let top = |qs: &[Quad]| qs.iter().flat_map(|q| q.corners).map(|c| c.y).fold(f32::MIN, f32::max);
    assert!((top(&shut) - 1.0).abs() < 1e-5);
    let mut open = Vec::new();
    quads(s, 1.0, |q| open.push(q));
    assert!((top(&open) - 1.5).abs() < 1e-5, "the lid rises half a block");
    // Facing north, the lid lifts toward -Z.
    let mut north = Vec::new();
    quads(State { facing: Dir::North, ..s }, 1.0, |q| north.push(q));
    let low_z = north.iter().flat_map(|q| q.corners).map(|c| c.z).fold(f32::MAX, f32::min);
    assert!((low_z + 0.5).abs() < 1e-5, "{low_z}");
    assert!(pixel(TEX, 4, 4)[3] == 255 && pixel(TEX_END - 1, 8, 8)[3] == 255);
}

#[test]
fn chunk_loads_index_boxes_for_the_renderer() {
    let mut w = world();
    w.set_block(P, item_state(None));
    assert_eq!(w.shulker_boxes_near(P.as_dvec3()).count(), 1);
    assert_eq!(w.shulker_boxes_near(P.as_dvec3() + DVec3::X * 100.0).count(), 0);
    w.unload_test_column(glam::IVec2::ZERO);
    assert_eq!(w.shulker_boxes_near(P.as_dvec3()).count(), 0);
    w.load_test_column(glam::IVec2::ZERO, 0);
    assert_eq!(w.shulker_boxes_near(P.as_dvec3()).count(), 1, "re-indexed from the saved chunk");
}
