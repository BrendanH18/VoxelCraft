//! Double chest states, pairing, the shared container and copper pairs
//! against Java 26.3's `ChestBlock`, `CopperChestBlock` and
//! `DoubleBlockCombiner`.
use super::*;
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::{block::RenderKind, chunk::ChunkData, redstone_blocks as r, terrain::Generator};
use glam::DVec3;
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(5)), Default::default(), 2);
    for y in w.generator.dimension.chunk_rows() {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w
}

const KINDS: [Kind; 6] = [
    Kind::Plain,
    Kind::Trapped,
    Kind::Copper(Stage::Unaffected),
    Kind::Copper(Stage::Exposed),
    Kind::Copper(Stage::Weathered),
    Kind::Copper(Stage::Oxidized),
];

fn all_states() -> impl Iterator<Item = ChestState> {
    KINDS.into_iter().flat_map(|kind| {
        Facing::ALL.into_iter().flat_map(move |facing| {
            ChestType::ALL.into_iter().flat_map(move |ty| [false, true].map(|wet| ChestState { kind, facing, ty, wet }))
        })
    })
}

fn chest(kind: Kind, facing: Facing, ty: ChestType) -> Block {
    block(ChestState { kind, facing, ty, wet: false })
}

/// Java's `BlockPlaceContext` for placing `raw` at `at` against the face
/// `face` of the block behind it, the player facing `look`.
fn ctx(at: IVec3, face: IVec3, look: Facing, sneaking: bool) -> PlaceContext {
    PlaceContext {
        clicked: at - face,
        clicked_block: Block::AIR,
        face,
        click: DVec3::splat(0.5),
        at,
        facing: look,
        look: look.offset().as_vec3(),
        sneaking,
    }
}

/// Places `raw` the way the shared placement does: its state, then the edit.
fn place(w: &mut World, raw: Block, c: PlaceContext) -> Block {
    let b = placed(raw, &c, |q| w.get_block(q)).expect("a chest");
    assert!(w.set_block(c.at, b));
    w.get_block(c.at).unwrap()
}

#[test]
fn every_chest_state_has_its_own_registered_id() {
    let mut seen = std::collections::HashSet::new();
    for s in all_states() {
        let b = block(s);
        assert_eq!(state(b), Some(s), "{s:?} round-trips");
        assert!(seen.insert(b.0), "{s:?} has its own id");
        assert_ne!(b.kind(), RenderKind::Invisible, "{s:?} is registered");
        assert!(crate::world::chest::is_chest(b), "{s:?} stores items");
        assert_eq!(b.is_waterlogged(), s.wet, "{s:?}");
        assert_eq!(state(b.with_water(!s.wet)), Some(ChestState { wet: !s.wet, ..s }), "{s:?} waterlogs");
        // Every half answers like its single chest: name, item, mining, drops.
        let single = block(ChestState { ty: ChestType::Single, wet: false, ..s });
        assert_eq!(b.name(), single.name());
        assert_eq!(b.base(), single.base(), "{s:?}");
        assert_eq!(b.hardness(), single.hardness(), "{s:?}");
        assert_eq!(b.with_facing(Facing::East), block(ChestState { facing: Facing::East, ..s }));
    }
    // Ender chests and barrels never pair.
    assert_eq!(state(crate::world::utilities::ENDER_CHEST), None);
    assert_eq!(state(Block::BARREL), None);
    assert_eq!(plain_single(PLAIN_DOUBLE), Some(Block::CHEST.with_facing(Facing::North)));
}

#[test]
fn halves_point_at_each_other_and_reach_the_shared_side() {
    // Java: a left half's partner is clockwise of its facing.
    assert_eq!(connected(Facing::North, ChestType::Left), Facing::East);
    assert_eq!(connected(Facing::North, ChestType::Right), Facing::West);
    for s in all_states().filter(|s| s.ty != ChestType::Single) {
        let partner = ChestState { ty: s.ty.opposite(), ..s };
        assert_eq!(connected(s.facing, s.ty).opposite(), connected(partner.facing, partner.ty));
        let shape = shape(block(s)).unwrap();
        let d = connected(s.facing, s.ty).offset();
        let reaches = |axis: usize, v: i32| match v {
            1 => shape.max[axis] == 16,
            -1 => shape.min[axis] == 0,
            _ => shape.min[axis] == 1 && shape.max[axis] == 15,
        };
        assert!(reaches(0, d.x) && reaches(2, d.z) && shape.max[1] == 14, "{s:?}");
    }
    assert_eq!(shape(Block::CHEST), Some(Box16 { min: [1, 0, 1], max: [15, 14, 15] }));
}

#[test]
fn a_chest_beside_one_facing_the_same_way_pairs_with_it() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    // Both placed by a player looking south: they face north.
    let first = place(&mut w, Block::CHEST, ctx(p, IVec3::Y, Facing::South, false));
    assert_eq!(state(first).unwrap().ty, ChestType::Single);
    let second = place(&mut w, Block::CHEST, ctx(p + IVec3::X, IVec3::Y, Facing::South, false));
    // Facing north, east is clockwise: the new chest's partner is west, so it is right.
    assert_eq!(second, chest(Kind::Plain, Facing::North, ChestType::Right));
    assert_eq!(w.get_block(p), Some(chest(Kind::Plain, Facing::North, ChestType::Left)));
    assert_eq!(w.chest_partner(p), Some(p + IVec3::X));
    assert_eq!(w.chest_partner(p + IVec3::X), Some(p));
    assert_eq!(w.container_slots(p), 54);
    // A third chest finds both neighbours taken.
    let third = place(&mut w, Block::CHEST, ctx(p + 2 * IVec3::X, IVec3::Y, Facing::South, false));
    assert_eq!(third, chest(Kind::Plain, Facing::North, ChestType::Single));
    // A chest facing another way stays single.
    let turned = place(&mut w, Block::CHEST, ctx(p - IVec3::X, IVec3::Y, Facing::East, false));
    assert_eq!(state(turned).unwrap().ty, ChestType::Single);
    // Plain and trapped chests don't pair.
    let mut w = world();
    place(&mut w, Block::CHEST, ctx(p, IVec3::Y, Facing::South, false));
    let trapped =
        place(&mut w, crate::world::utilities::TRAPPED_CHEST, ctx(p + IVec3::X, IVec3::Y, Facing::South, false));
    assert_eq!(state(trapped).unwrap().ty, ChestType::Single);
}

#[test]
fn sneaking_keeps_a_chest_single_unless_placed_against_a_chests_side() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    place(&mut w, Block::CHEST, ctx(p, IVec3::Y, Facing::South, false));
    // Sneaking onto the floor beside it: no pairing.
    let alone = place(&mut w, Block::CHEST, ctx(p + IVec3::X, IVec3::Y, Facing::South, true));
    assert_eq!(state(alone).unwrap().ty, ChestType::Single);
    // Sneaking against the side of a chest facing north joins it, taking
    // its facing even though the player looks east.
    let mut w = world();
    place(&mut w, Block::CHEST, ctx(p, IVec3::Y, Facing::South, false));
    let joined = place(&mut w, Block::CHEST, ctx(p - IVec3::X, IVec3::NEG_X, Facing::East, true));
    let s = state(joined).unwrap();
    assert_eq!((s.facing, s.ty), (Facing::North, ChestType::Left));
    assert_eq!(w.chest_partner(p - IVec3::X), Some(p));
    // Against its front (the facing's axis) nothing joins.
    let mut w = world();
    place(&mut w, Block::CHEST, ctx(p, IVec3::Y, Facing::South, false));
    let front = place(&mut w, Block::CHEST, ctx(p - IVec3::Z, IVec3::NEG_Z, Facing::South, true));
    assert_eq!(state(front).unwrap().ty, ChestType::Single);
}

#[test]
fn breaking_a_half_leaves_the_other_single_with_its_items() {
    let mut w = world();
    w.tile_drops = true;
    let p = IVec3::new(4, 100, 4);
    place(&mut w, Block::CHEST, ctx(p, IVec3::Y, Facing::South, false));
    place(&mut w, Block::CHEST, ctx(p + IVec3::X, IVec3::Y, Facing::South, false));
    // Slots 0..27 are the right half's (the second chest), 27..54 the left's.
    assert!(w.set_chest_slot(p, 0, Some(Stack::new(Item::DIAMOND, 3))));
    assert!(w.set_chest_slot(p, 30, Some(Stack::new(Item::STICK, 5))));
    assert_eq!(w.chest(p + IVec3::X).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 3)));
    assert_eq!(w.chest(p).unwrap().slots[3], Some(Stack::new(Item::STICK, 5)));
    assert_eq!(w.chest_slot(p + IVec3::X, 30), Some(Stack::new(Item::STICK, 5)), "either half reads both");
    w.drops.clear();
    w.set_block(p + IVec3::X, Block::AIR);
    assert_eq!(w.get_block(p), Some(chest(Kind::Plain, Facing::North, ChestType::Single)));
    assert_eq!(w.container_slots(p), 27);
    assert_eq!(w.chest(p).unwrap().slots[3], Some(Stack::new(Item::STICK, 5)), "the kept half keeps its own");
    assert_eq!(w.drops, vec![(p + IVec3::X, Stack::new(Item::DIAMOND, 3))]);
}

#[test]
fn hoppers_fill_the_first_half_then_the_second_and_comparators_read_both() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    let (left, right) =
        (chest(Kind::Plain, Facing::North, ChestType::Left), chest(Kind::Plain, Facing::North, ChestType::Right));
    w.set_block(p, left);
    w.set_block(p + IVec3::X, right);
    assert_eq!(w.chest_halves(p), (p + IVec3::X, Some(p)));
    // With the right half's 27 slots (the first) full, a hopper above the
    // left half puts its item into slot 27: the left half's first.
    for i in 0..27 {
        w.set_chest_slot(p, i, Some(Stack::new(Item::DIAMOND, 64)));
    }
    assert_eq!(w.container_signal(p), w.container_signal(p + IVec3::X));
    // 27 of 54 slots full: floor(14 * 0.5) + 1 = 8.
    assert_eq!(w.container_signal(p), Some(8));
    w.set_block(p + IVec3::Y, r::hopper(4, false));
    w.with_chest_slots(p + IVec3::Y, |s| s[0] = Some(Stack::new(Item::STICK, 1)));
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert_eq!(w.chest(p).unwrap().slots[0], Some(Stack::new(Item::STICK, 1)), "the left half's first slot");
    // A conductor on either half keeps the chest shut: comparators read 0.
    assert!(w.chest_opens(p));
    w.set_block(p + IVec3::X + IVec3::Y, Block::STONE);
    assert!(!w.chest_opens(p) && !w.chest_opens(p + IVec3::X));
    assert_eq!(w.container_signal(p), Some(0));
    w.set_block(p + IVec3::X + IVec3::Y, Block::GLASS);
    assert!(w.chest_opens(p), "glass isn't a conductor");
}

#[test]
fn a_shared_screen_moves_items_across_both_halves() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    w.set_block(p, chest(Kind::Plain, Facing::South, ChestType::Left));
    w.set_block(p - IVec3::X, chest(Kind::Plain, Facing::South, ChestType::Right));
    let n = w.with_chest_slots(p, |slots| {
        slots[53] = Some(Stack::new(Item::DIAMOND, 1));
        slots.len()
    });
    assert_eq!(n, Some(54));
    assert_eq!(w.chest(p).unwrap().slots[26], Some(Stack::new(Item::DIAMOND, 1)));
    assert_eq!(w.with_chest_slots(IVec3::new(9, 100, 9), |s| s.len()), None, "no chest there");
}

#[test]
fn trapped_halves_both_signal_their_viewers() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    let left = chest(Kind::Trapped, Facing::North, ChestType::Left);
    let right = chest(Kind::Trapped, Facing::North, ChestType::Right);
    w.set_block(p, left);
    w.set_block(p + IVec3::X, right);
    // Two players have it open, one through each half.
    w.set_chest_viewers([p, p + IVec3::X]);
    assert_eq!(w.trapped_signal(p, left, IVec3::X, false), Some(2));
    assert_eq!(w.trapped_signal(p + IVec3::X, right, IVec3::X, false), Some(2));
    w.set_chest_viewers([]);
    assert_eq!(w.chest_viewers(p), 0);
    assert_eq!(w.chest_viewers(p + IVec3::X), 0);
}

#[test]
fn copper_pairs_take_the_younger_stage_and_age_together() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    let item = |s| chest(Kind::Copper(s), Facing::North, ChestType::Single);
    place(&mut w, item(Stage::Weathered), ctx(p, IVec3::Y, Facing::South, false));
    // An exposed chest joins: both become exposed.
    let placed = place(&mut w, item(Stage::Exposed), ctx(p + IVec3::X, IVec3::Y, Facing::South, false));
    assert_eq!(placed, chest(Kind::Copper(Stage::Exposed), Facing::North, ChestType::Right));
    assert_eq!(w.get_block(p), Some(chest(Kind::Copper(Stage::Exposed), Facing::North, ChestType::Left)));
    // An oxidized chest joining an unaffected one becomes unaffected.
    let mut w2 = world();
    place(&mut w2, item(Stage::Unaffected), ctx(p, IVec3::Y, Facing::South, false));
    let young = place(&mut w2, item(Stage::Oxidized), ctx(p + IVec3::X, IVec3::Y, Facing::South, false));
    assert_eq!(state(young).unwrap().kind, Kind::Copper(Stage::Unaffected));
    // Scraping one half scrapes the pair; contents stay.
    w.set_chest_slot(p, 40, Some(Stack::new(Item::DIAMOND, 2)));
    assert!(w.scrape_copper(p + IVec3::X));
    assert_eq!(w.get_block(p), Some(chest(Kind::Copper(Stage::Unaffected), Facing::North, ChestType::Left)));
    assert_eq!(w.chest_slot(p, 40), Some(Stack::new(Item::DIAMOND, 2)));
    // The pair ages through its left half and never splits.
    let (l, r) = (p, p + IVec3::X);
    for _ in 0..20_000 {
        for q in [l, r] {
            let b = w.get_block(q).unwrap();
            w.tick_copper(q, b);
        }
        let (a, b) = (state(w.get_block(l).unwrap()).unwrap(), state(w.get_block(r).unwrap()).unwrap());
        assert_eq!(a.kind, b.kind, "the halves age together");
        if a.kind != Kind::Copper(Stage::Unaffected) {
            break;
        }
    }
    assert_eq!(state(w.get_block(l).unwrap()).unwrap().kind, Kind::Copper(Stage::Exposed));
    assert_eq!(w.chest_partner(l), Some(r));
}

#[test]
fn a_copper_pair_ages_only_through_its_left_half_and_not_while_open() {
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    w.set_block(p, chest(Kind::Copper(Stage::Unaffected), Facing::North, ChestType::Left));
    w.set_block(p + IVec3::X, chest(Kind::Copper(Stage::Unaffected), Facing::North, ChestType::Right));
    let right = w.get_block(p + IVec3::X).unwrap();
    for _ in 0..100_000 {
        w.tick_copper(p + IVec3::X, right);
    }
    assert_eq!(w.get_block(p + IVec3::X), Some(right));
    w.set_chest_viewers([p]);
    let left = w.get_block(p).unwrap();
    for _ in 0..100_000 {
        w.tick_copper(p, left);
    }
    assert_eq!(w.get_block(p), Some(left), "an open chest doesn't age");
}

#[test]
fn half_textures_copy_the_single_chest_with_the_joint_opened() {
    for layer in tex::BASE..tex::BASE + tex::COUNT {
        let (single, _, _) = texture(layer).unwrap();
        assert!(texture(single).is_none(), "{layer} copies a single texture");
    }
    assert!(texture(tex::BASE + tex::COUNT).is_none());
    // The outer edge stays, the joint pixel is a latch column.
    assert_eq!(joint_source(0, true), 0);
    assert_eq!(joint_source(15, true), 7);
    assert_eq!(joint_source(15, false), 15);
    assert_eq!(joint_source(0, false), 8);
    for i in 0..16 {
        // The joint's frame column is never copied, and only the joint
        // pixel copies a latch column.
        let (hi, lo) = (joint_source(i, true), joint_source(i, false));
        assert!(hi < 15 && (hi != 7 || i == 15) && hi != 8, "{i} high: {hi}");
        assert!(lo > 0 && (lo != 8 || i == 0) && lo != 7, "{i} low: {lo}");
    }
    // A half's front shows its own front layer, its back and top the half layers.
    let left = state(chest(Kind::Plain, Facing::North, ChestType::Left)).unwrap();
    let f = faces(left);
    assert_eq!(texture(f[Facing::North.face()]).map(|t| t.0), Some(crate::world::block::tex::CHEST_FRONT));
    assert_eq!(texture(f[2]), Some((crate::world::block::tex::CHEST_TOP, false, true)), "partner east: high x");
}

#[test]
fn double_chests_survive_save_and_reload() {
    use crate::world::storage::{LevelInfo, Storage};
    let dir = std::env::temp_dir().join(format!("voxelcraft-double-chest-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let mut chunk = ChunkData::Uniform(Block::AIR);
    let states: Vec<Block> = all_states().map(block).collect();
    for (i, b) in states.iter().enumerate() {
        chunk.set(i % 32, i / 32, 5, *b);
    }
    let pos = IVec3::new(1, -3, 2);
    storage.save(&LevelInfo { seed: 1, player: None, props: Default::default() }, &[(pos, Arc::new(chunk))]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    for (i, b) in states.iter().enumerate() {
        assert_eq!(loaded[&pos].get(i % 32, i / 32, 5), *b);
    }
    std::fs::remove_dir_all(dir).unwrap();
    // Each half's contents save by position, so a reloaded pair reads as one.
    let mut w = world();
    let p = IVec3::new(4, 100, 4);
    w.set_block(p, chest(Kind::Plain, Facing::North, ChestType::Left));
    w.set_block(p + IVec3::X, chest(Kind::Plain, Facing::North, ChestType::Right));
    w.set_chest_slot(p, 50, Some(Stack::new(Item::DIAMOND, 7)));
    let saved = w.chests_to_string();
    let mut other = world();
    other.set_block(p, chest(Kind::Plain, Facing::North, ChestType::Left));
    other.set_block(p + IVec3::X, chest(Kind::Plain, Facing::North, ChestType::Right));
    other.load_chests(&saved);
    assert_eq!(other.chest_slot(p + IVec3::X, 50), Some(Stack::new(Item::DIAMOND, 7)));
}
