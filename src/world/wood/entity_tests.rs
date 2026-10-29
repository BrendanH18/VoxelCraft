//! Signs, shelves and the chiseled bookshelf in a world: states, support,
//! block entities, redstone and saving. Expected values come from Java
//! 26.3's `SignBlock` family, `ShelfBlock`, `SideChainPartBlock` and
//! `ChiseledBookShelfBlock` (see the verification record).
use std::sync::Arc;

use glam::{DVec3, IVec3, Vec3};

use super::shelves::{Chain, Shelf};
use super::signs::{Mount, Sign};
use super::*;
use crate::color::DyeColor;
use crate::content::fuel;
use crate::inventory::Stack;
use crate::session::placement::PlaceContext;
use crate::world::World;
use crate::world::chunk::ChunkData;
use crate::world::redstone_blocks as r;
use crate::world::shape;
use crate::world::terrain::Generator;

const AT: IVec3 = IVec3::new(8, 145, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w
}

fn all_signs() -> impl Iterator<Item = Sign> {
    Kind::ALL.into_iter().flat_map(|kind| {
        let mut mounts = Vec::new();
        for rotation in 0..16 {
            mounts.push(Mount::Standing { rotation });
            mounts.push(Mount::Hanging { rotation, attached: false });
            mounts.push(Mount::Hanging { rotation, attached: true });
        }
        for facing in Facing::ALL {
            mounts.push(Mount::Wall { facing });
            mounts.push(Mount::WallHanging { facing });
        }
        mounts.into_iter().flat_map(move |mount| [false, true].map(|wet| Sign { kind, mount, wet }))
    })
}

#[test]
fn every_sign_state_round_trips_with_java_properties() {
    let mut count = 0;
    for s in all_signs() {
        count += 1;
        let b = s.block();
        assert!(owns(b.0));
        assert_eq!(signs::sign(b), Some(s));
        assert_eq!(b.is_waterlogged(), s.wet, "{}", b.name());
        assert_eq!(b.with_water(!s.wet), Sign { wet: !s.wet, ..s }.block());
        assert_eq!(b.base(), s.item_block());
        assert_eq!(b.drop(), Some(Item::from_block(s.item_block())));
        assert_eq!(b.hardness(), 1.0);
        assert_eq!(b.best_tool(), Some(ToolKind::Axe));
        assert!(b.flat_icon(), "sign items are sprites");
        assert_eq!(b.kind(), RenderKind::Shaped);
        assert_eq!(b.fire_odds(), (0, 0), "FireBlock lists no signs");
        assert_eq!(b.ignited_by_lava(), s.kind.burns());
        let outline = shape::outline(b, |_| Block::AIR, Block::AIR);
        assert!(!outline.is_empty(), "{} can be targeted", b.name());
        let collision = shape::collision(b, |_| Block::AIR, Block::AIR);
        assert_eq!(collision.is_empty(), !matches!(s.mount, Mount::WallHanging { .. }), "{}", b.name());
        assert!(shape::shape(b, |_| Block::AIR, Block::AIR).is_empty(), "drawn outside the chunk mesh");
        assert_eq!(b.light_opacity(), if s.wet { 1 } else { 0 });
        let name = b.name();
        let expected = match s.mount {
            Mount::Standing { .. } => format!("{} sign", s.kind.name()),
            Mount::Wall { .. } => format!("{} wall sign", s.kind.name()),
            Mount::Hanging { .. } => format!("{} hanging sign", s.kind.name()),
            Mount::WallHanging { .. } => format!("{} wall hanging sign", s.kind.name()),
        };
        assert_eq!(name, expected);
    }
    assert_eq!(count, 12 * 112);
    // Fuel: 200 ticks for signs, 800 for hanging signs; crimson and warped don't burn.
    for kind in Kind::ALL {
        let standing = Sign { kind, mount: Mount::Standing { rotation: 0 }, wet: false }.block();
        let hanging = Sign { kind, mount: Mount::Hanging { rotation: 0, attached: false }, wet: false }.block();
        assert_eq!(fuel::ticks(Item::from_block(standing)), kind.burns().then_some(200));
        assert_eq!(fuel::ticks(Item::from_block(hanging)), kind.burns().then_some(800));
    }
    for name in ["oak_wall_sign", "warped_wall_hanging_sign", "bamboo_hanging_sign", "pale_oak_sign"] {
        assert!(Block::from_name(name).is_some(), "{name}");
    }
}

fn ctx(at: IVec3, clicked: IVec3, face: IVec3, look: Vec3) -> PlaceContext {
    PlaceContext {
        clicked,
        clicked_block: Block::STONE,
        face,
        click: DVec3::splat(0.5),
        at,
        facing: Facing::toward(Vec3::new(look.x, 0.0, look.z)).opposite(),
        look: look.normalize(),
        sneaking: false,
    }
}

#[test]
fn signs_place_standing_on_floors_and_on_walls_facing_the_player() {
    let mut w = world();
    let oak = Sign { kind: Kind::Oak, mount: Mount::Standing { rotation: 0 }, wet: false }.block();
    w.set_block(AT - IVec3::Y, Block::STONE);
    // Looking down and north at a floor: a standing sign facing south (rotation 0).
    let c = ctx(AT, AT - IVec3::Y, IVec3::Y, Vec3::new(0.0, -0.9, -0.4));
    let placed = signs::placed(oak, &c, &|p| w.get_block(p)).unwrap().unwrap();
    assert_eq!(signs::sign(placed).unwrap().mount, Mount::Standing { rotation: 0 });
    // Looking east (rotation 4 is west-facing... the sign faces the player: west).
    let c = ctx(AT, AT - IVec3::Y, IVec3::Y, Vec3::new(1.0, -0.9, 0.0));
    let placed = signs::placed(oak, &c, &|p| w.get_block(p)).unwrap().unwrap();
    assert_eq!(signs::sign(placed).unwrap().mount, Mount::Standing { rotation: 4 });
    // Looking north at a wall: a wall sign facing south.
    let wall = AT + IVec3::new(4, 0, 0);
    w.set_block(wall - IVec3::Z, Block::STONE);
    let c = ctx(wall, wall - IVec3::Z, IVec3::Z, Vec3::new(0.0, 0.0, -1.0));
    let placed = signs::placed(oak, &c, &|p| w.get_block(p)).unwrap().unwrap();
    assert_eq!(signs::sign(placed).unwrap().mount, Mount::Wall { facing: Facing::South });
    // In mid-air nothing holds it.
    let air = AT + IVec3::new(0, 6, 6);
    let c = ctx(air, air - IVec3::Z, IVec3::Z, Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(signs::placed(oak, &c, &|p| w.get_block(p)), Some(None));
    // Hanging signs: under a ceiling, or between two walls on a wall.
    let hanging =
        Sign { kind: Kind::Cherry, mount: Mount::Hanging { rotation: 0, attached: false }, wet: false }.block();
    let under = AT + IVec3::new(-4, 0, 0);
    w.set_block(under + IVec3::Y, Block::STONE);
    let c = ctx(under, under + IVec3::Y, IVec3::NEG_Y, Vec3::new(0.0, 0.9, -0.4));
    let placed = signs::placed(hanging, &c, &|p| w.get_block(p)).unwrap().unwrap();
    assert_eq!(signs::sign(placed).unwrap().mount, Mount::Hanging { rotation: 0, attached: false });
    // Clicking the west face of a block: the plank runs along x, so the
    // sign faces north or south (the first of them the player looks along).
    let side = AT + IVec3::new(0, 0, -5);
    w.set_block(side + IVec3::X, Block::STONE);
    let c = ctx(side, side + IVec3::X, IVec3::NEG_X, Vec3::new(1.0, 0.0, -0.3));
    let placed = signs::placed(hanging, &c, &|p| w.get_block(p)).unwrap().unwrap();
    assert_eq!(signs::sign(placed).unwrap().mount, Mount::WallHanging { facing: Facing::South });
}

#[test]
fn signs_break_when_their_support_goes_and_keep_text_until_then() {
    let mut w = world();
    w.set_block(AT - IVec3::Y, Block::STONE);
    let standing = Sign { kind: Kind::Spruce, mount: Mount::Standing { rotation: 3 }, wet: false };
    w.set_block(AT, standing.block());
    let wall_at = AT + IVec3::X * 3;
    w.set_block(wall_at - IVec3::Z, Block::STONE);
    let wall = Sign { kind: Kind::Spruce, mount: Mount::Wall { facing: Facing::South }, wet: false };
    w.set_block(wall_at, wall.block());
    w.set_sign_text(AT, true, ["Hello".into(), String::new(), "World!".into(), String::new()]).unwrap();
    assert_eq!(w.sign_at(AT).unwrap().1.front.lines[2], "World!");
    w.drops.clear();
    w.set_block(AT - IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(AT), Some(Block::AIR), "standing sign without a floor");
    assert!(w.sign_at(AT).is_none() && w.wood_entities().sign(AT).is_none());
    assert!(w.drops.iter().any(|(_, s)| s.item == Item::from_block(standing.item_block())));
    w.set_block(wall_at - IVec3::Z, Block::AIR);
    assert_eq!(w.get_block(wall_at), Some(Block::AIR), "wall sign without its wall");
}

#[test]
fn signs_resting_on_signs_break_together_and_hanging_signs_hold_hanging_signs() {
    let mut w = world();
    // A standing sign on a standing sign on stone: removing the stone drops both.
    let standing = Sign { kind: Kind::Oak, mount: Mount::Standing { rotation: 0 }, wet: false }.block();
    w.set_block(AT - IVec3::Y, Block::STONE);
    w.set_block(AT, standing);
    w.set_block(AT + IVec3::Y, standing);
    w.set_block(AT - IVec3::Y, Block::AIR);
    assert_eq!(w.get_block(AT), Some(Block::AIR));
    assert_eq!(w.get_block(AT + IVec3::Y), Some(Block::AIR), "the upper sign loses its support too");
    // `getBlockSupportShape` is the outline: a hanging sign hangs under another.
    let hanging =
        Sign { kind: Kind::Cherry, mount: Mount::Hanging { rotation: 0, attached: false }, wet: false }.block();
    let top = AT + IVec3::new(-4, 2, 0);
    w.set_block(top + IVec3::Y, Block::STONE);
    w.set_block(top, hanging);
    let c = ctx(top - IVec3::Y, top, IVec3::NEG_Y, Vec3::new(0.0, 0.9, -0.4));
    assert!(signs::placed(hanging, &c, &|p| w.get_block(p)).unwrap().is_some());
    // A wall hanging sign stays once placed, even when its side goes
    // (`WallHangingSignBlock` only checks its sides in `canPlace`).
    let side = AT + IVec3::new(0, 0, -5);
    w.set_block(side + IVec3::X, Block::STONE);
    let wall_hanging = Sign { kind: Kind::Oak, mount: Mount::WallHanging { facing: Facing::South }, wet: false };
    w.set_block(side, wall_hanging.block());
    w.set_block(side + IVec3::X, Block::AIR);
    assert_eq!(w.get_block(side), Some(wall_hanging.block()));
    // Signs stack to 16 (`stacksTo(16)`).
    assert_eq!(Item::from_block(standing).info().max_stack, 16);
    assert_eq!(Item::from_block(hanging).info().max_stack, 16);
    assert_eq!(Item::from_block(shelf(Kind::Oak, Facing::South)).info().max_stack, 64);
}

#[test]
fn shelves_let_held_items_act_unless_aimed_at_their_front() {
    let mut w = world();
    w.set_block(AT, shelf(Kind::Oak, Facing::South));
    let pearl = Some(Item::ENDER_PEARL);
    let centre = AT.as_dvec3() + DVec3::splat(0.5);
    // Looking north at the front (south face): the shelf acts first.
    let front = (centre + DVec3::new(0.0, 0.0, 3.0), DVec3::NEG_Z);
    assert!(crate::content::interaction::block_use_precedes_item(&w, AT, pearl, false, front));
    // Looking south at its back: `PASS`, so the pearl is thrown.
    let back = (centre + DVec3::new(0.0, 0.0, -3.0), DVec3::Z);
    assert!(!crate::content::interaction::block_use_precedes_item(&w, AT, pearl, false, back));
}

#[test]
fn sign_text_saves_and_loads_any_printable_line() {
    let mut w = world();
    w.set_block(AT - IVec3::Y, Block::STONE);
    let s = Sign { kind: Kind::Warped, mount: Mount::Standing { rotation: 9 }, wet: false };
    w.set_block(AT, s.block());
    let tricky = "a|b=c;d:e.f,g\\h".to_string();
    w.set_sign_text(AT, true, [tricky.clone(), "é dropped".into(), "x".repeat(500), String::new()]).unwrap();
    w.set_sign_text(AT, false, ["back".into(), String::new(), String::new(), String::new()]).unwrap();
    assert!(w.update_sign(AT, true, |t| t.color = DyeColor::Red));
    assert!(w.update_sign(AT, false, |t| t.glowing = true));
    assert!(!w.update_sign(AT, false, |t| t.glowing = true), "no change, no update");
    let saved = w.wood_to_string();
    let mut other = world();
    other.set_block(AT - IVec3::Y, Block::STONE);
    other.set_block(AT, s.block());
    other.load_wood(&saved);
    let (_, e) = other.sign_at(AT).unwrap();
    assert_eq!(e.front.lines[0], tricky);
    assert_eq!(e.front.lines[1], " dropped", "unprintable characters are dropped");
    assert_eq!(e.front.lines[2].len(), entities::MAX_LINE_CHARS);
    assert_eq!((e.front.color, e.front.glowing), (DyeColor::Red, false));
    assert_eq!((e.back.lines[0].as_str(), e.back.color, e.back.glowing), ("back", DyeColor::Black, true));
    other.load_wood("s:1,2=garbage|h:x=1|b:1,2,3=;");
}

#[test]
fn signs_face_front_toward_the_side_the_player_stands_on() {
    let s = Sign { kind: Kind::Oak, mount: Mount::Standing { rotation: 0 }, wet: false };
    assert!(s.faces_front(AT, AT.as_dvec3() + DVec3::new(0.5, 0.0, 3.0)), "south of a south-facing sign");
    assert!(!s.faces_front(AT, AT.as_dvec3() + DVec3::new(0.5, 0.0, -3.0)));
    let w = Sign { mount: Mount::Wall { facing: Facing::East }, ..s };
    assert!(w.faces_front(AT, AT.as_dvec3() + DVec3::new(3.0, 0.0, 0.5)));
    assert!(!w.faces_front(AT, AT.as_dvec3() + DVec3::new(-3.0, 0.0, 0.5)));
}

// ---- Shelves --------------------------------------------------------------

fn shelf(kind: Kind, facing: Facing) -> Block {
    Shelf { kind, facing, powered: false, chain: Chain::Unconnected, wet: false }.block()
}

#[test]
fn shelf_states_round_trip_with_java_properties() {
    for kind in Kind::ALL {
        for facing in Facing::ALL {
            for powered in [false, true] {
                for chain in [Chain::Unconnected, Chain::Right, Chain::Centre, Chain::Left] {
                    for wet in [false, true] {
                        let s = Shelf { kind, facing, powered, chain, wet };
                        let b = s.block();
                        assert_eq!(shelves::shelf(b), Some(s));
                        assert_eq!(b.is_waterlogged(), wet);
                        assert_eq!(b.base(), shelf(kind, Facing::North));
                        assert_eq!(b.hardness(), 2.0);
                        assert_eq!(b.best_tool(), Some(ToolKind::Axe));
                        assert_eq!(b.fire_odds(), if kind.burns() && !wet { (30, 20) } else { (0, 0) });
                        assert!(b.uses_shape_for_light_occlusion(), "Java's useShapeForLightOcclusion");
                        assert_eq!(b.with_facing(Facing::West), Shelf { facing: Facing::West, ..s }.block());
                    }
                }
            }
        }
        assert_eq!(fuel::ticks(Item::from_block(shelf(kind, Facing::North))), kind.burns().then_some(300));
    }
    // The back board fills the face behind the front.
    let boxes = shape::shape(shelf(Kind::Oak, Facing::South), |_| Block::AIR, Block::AIR);
    assert!(boxes.as_slice().contains(&shape::Box16 { min: [0, 0, 0], max: [16, 16, 3] }));
}

#[test]
fn shelves_take_and_give_one_stack_per_slot_and_spill_when_broken() {
    let mut w = world();
    w.set_block(AT, shelf(Kind::Birch, Facing::South));
    let front = AT.as_dvec3() + DVec3::new(0.1, 0.5, 1.0);
    let mut hotbar = [None; 9];
    hotbar[0] = Some(Stack::new(Item::DIAMOND, 5));
    assert_eq!(w.use_shelf(AT, IVec3::Z, front, &mut hotbar, 0, false), Some(ShelfUse::Placed));
    assert_eq!(hotbar[0], None);
    assert_eq!(w.wood_entities().shelf(AT).unwrap()[0], Some(Stack::new(Item::DIAMOND, 5)));
    // The middle slot is empty: an empty hand passes.
    let middle = AT.as_dvec3() + DVec3::new(0.5, 0.5, 1.0);
    assert_eq!(w.use_shelf(AT, IVec3::Z, middle, &mut hotbar, 0, false), None);
    // Clicking another face passes.
    assert_eq!(w.use_shelf(AT, IVec3::Y, front, &mut hotbar, 0, false), None);
    // Taking it back with an empty hand.
    assert_eq!(w.use_shelf(AT, IVec3::Z, front, &mut hotbar, 0, false), Some(ShelfUse::Swapped { took: true }));
    assert_eq!(hotbar[0], Some(Stack::new(Item::DIAMOND, 5)));
    // Creative players keep what they put down.
    assert_eq!(w.use_shelf(AT, IVec3::Z, front, &mut hotbar, 0, true), Some(ShelfUse::Placed));
    assert_eq!(hotbar[0], Some(Stack::new(Item::DIAMOND, 5)));
    // Comparator reading from behind only: bit 0 for the left slot.
    assert_eq!(w.wood_comparator_signal(AT, IVec3::NEG_Z), Some(1));
    assert_eq!(w.wood_comparator_signal(AT, IVec3::X), Some(0));
    let saved = w.wood_to_string();
    let mut other = world();
    other.set_block(AT, shelf(Kind::Birch, Facing::South));
    other.load_wood(&saved);
    assert_eq!(other.wood_entities().shelf(AT).unwrap()[0], Some(Stack::new(Item::DIAMOND, 5)));
    w.drops.clear();
    w.set_block(AT, Block::AIR);
    assert!(w.drops.iter().any(|(_, s)| *s == Stack::new(Item::DIAMOND, 5)), "contents spill");
}

#[test]
fn powered_shelves_chain_up_to_three_and_swap_with_the_hotbar() {
    let mut w = world();
    let row: Vec<IVec3> = (0..4).map(|i| AT + IVec3::X * i).collect();
    for &p in &row {
        w.set_block(p, shelf(Kind::Oak, Facing::South));
    }
    // A redstone block under each powers it; Java's chains stop at three.
    for &p in &row {
        w.set_block(p - IVec3::Y, r::REDSTONE_BLOCK);
        for _ in 0..4 {
            w.tick_redstone();
        }
    }
    let state = |w: &World, p: IVec3| shelves::shelf(w.get_block(p).unwrap()).unwrap();
    assert!(row.iter().all(|&p| state(&w, p).powered));
    // Facing south, the shelf's left (from its front) is west: lower x.
    assert_eq!(state(&w, row[0]).chain, Chain::Left);
    assert_eq!(state(&w, row[1]).chain, Chain::Centre);
    assert_eq!(state(&w, row[2]).chain, Chain::Right);
    assert_eq!(state(&w, row[3]).chain, Chain::Unconnected);
    assert_eq!(w.shelf_chain(row[1]), row[..3].to_vec());
    // Hotbar slots 0..9: the chain of three takes all nine.
    let mut hotbar: [Option<Stack>; 9] = std::array::from_fn(|i| Some(Stack::new(Item::STICK, i as u8 + 1)));
    let middle = row[1].as_dvec3() + DVec3::new(0.5, 0.5, 1.0);
    assert_eq!(w.use_shelf(row[1], IVec3::Z, middle, &mut hotbar, 0, false), Some(ShelfUse::Hotbar));
    assert!(hotbar.iter().all(Option::is_none));
    assert_eq!(w.wood_entities().shelf(row[2]).unwrap()[2], Some(Stack::new(Item::STICK, 9)));
    // Losing power unchains a shelf and its neighbours.
    w.set_block(row[1] - IVec3::Y, Block::STONE);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert!(!state(&w, row[1]).powered);
    assert_eq!(state(&w, row[1]).chain, Chain::Unconnected);
    assert_eq!(state(&w, row[0]).chain, Chain::Unconnected);
    assert_eq!(state(&w, row[2]).chain, Chain::Unconnected);
}

// ---- Chiseled bookshelf ---------------------------------------------------

#[test]
fn chiseled_bookshelves_hold_six_books_and_report_the_last_slot() {
    let mut w = world();
    w.set_block(AT, bookshelf::state(Facing::South, 0));
    let book = Some(Stack::new(Item::BOOK, 3));
    // Slot 4: bottom middle, seen from the front (south).
    let hit = AT.as_dvec3() + DVec3::new(0.5, 0.25, 1.0);
    let (used, out) = w.use_bookshelf(AT, IVec3::Z, hit, book).unwrap();
    assert_eq!((used, out), (ShelfUse::BookIn { enchanted: false }, None));
    assert_eq!(bookshelf::slots(w.get_block(AT).unwrap()), Some(1 << 4));
    assert_eq!(w.wood_comparator_signal(AT, IVec3::X), Some(5));
    // Clicking it again with a book takes nothing more; an empty hand takes it out.
    let (used, out) = w.use_bookshelf(AT, IVec3::Z, hit, None).unwrap();
    assert_eq!((used, out), (ShelfUse::BookOut { enchanted: false }, Some(Stack::new(Item::BOOK, 1))));
    assert_eq!(bookshelf::slots(w.get_block(AT).unwrap()), Some(0));
    // Top left from the front of a north-facing shelf is at the east end.
    w.set_block(AT + IVec3::X * 3, bookshelf::state(Facing::North, 0));
    let p = AT + IVec3::X * 3;
    let hit = p.as_dvec3() + DVec3::new(0.9, 0.9, 0.0);
    w.use_bookshelf(p, IVec3::NEG_Z, hit, Some(Stack::new(Item::ENCHANTED_BOOK, 1))).unwrap();
    assert_eq!(bookshelf::slots(w.get_block(p).unwrap()), Some(1));
    // Silk touch only; the books spill.
    let b = w.get_block(p).unwrap();
    assert_eq!(b.drop(), None);
    assert_eq!(crate::mining::silk_drop(b), Some(Item::from_block(bookshelf::CHISELED_BOOKSHELF)));
    assert_eq!(b.hardness(), 1.5);
    w.drops.clear();
    w.set_block(p, Block::AIR);
    assert!(w.drops.iter().any(|(_, s)| s.item == Item::ENCHANTED_BOOK));
    assert_eq!(fuel::ticks(Item::from_block(bookshelf::CHISELED_BOOKSHELF)), Some(300));
}

#[test]
fn lan_sign_contents_go_nearest_first_within_their_byte_budget() {
    let mut w = world();
    let sign = Sign { kind: Kind::Oak, mount: Mount::Standing { rotation: 0 }, wet: false }.block();
    let long = || std::array::from_fn(|_| "x".repeat(super::entities::MAX_LINE_CHARS));
    for dx in [0, 4] {
        w.set_block(AT + IVec3::X * dx, sign);
        w.set_sign_text(AT + IVec3::X * dx, true, long()).unwrap();
    }
    let at = AT.as_dvec3();
    let both = w.wood_near(at, 160.0, usize::MAX);
    assert_eq!(both.split('|').count(), 2);
    // Room for one: the nearer sign.
    let one = w.wood_near(at, 160.0, both.len() / 2 + 8);
    assert_eq!(one.split('|').count(), 1);
    assert!(one.starts_with(&format!("s:{},{},{}=", AT.x, AT.y, AT.z)), "{}", &one[..30]);
    let mut client = world();
    client.set_block(AT, sign);
    client.replace_wood(&one);
    assert_eq!(client.sign_at(AT).unwrap().1.side(true).lines, long());
}
