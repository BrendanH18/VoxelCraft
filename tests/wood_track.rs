//! v0.12 wood track through the shared session paths every adapter uses
//! (`place_held`, `use_held`) and through a file save: log axes, stripping,
//! the new trapdoors, buttons and plates (Java 26.3 `RotatedPillarBlock`,
//! `block_transformer/axe.json`, `TrapDoorBlock`, `ButtonBlock`).
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    player::EYE_HEIGHT,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
    },
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        redstone_blocks::{self as r, Component},
        storage::{LevelInfo, Storage},
        terrain::Generator,
        wood::{self, Pillar, switches},
    },
};

fn scene(item: Item) -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut session = PlayerSession::new(PlayerId::HOST, DVec3::new(4.5, 145., 4.5));
    session.inventory.slots[0] = Some(Stack::new(item, item.info().max_stack));
    (world, session, Entities::new(7))
}

fn aim(session: &mut PlayerSession, eye: DVec3, point: DVec3) {
    let d = (point - eye).normalize();
    session.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    session.player.yaw = d.z.atan2(d.x) as f32;
    session.player.pitch = d.y.asin() as f32;
}

fn place(world: &mut World, session: &mut PlayerSession, eye: DVec3, point: DVec3) {
    aim(session, eye, point);
    session.place_held(0, world, &[]).unwrap();
}

fn use_held(s: &mut PlayerSession, w: &mut World, e: &mut Entities) -> Option<Used> {
    s.use_held(0, w, e, &UseContext { sneaking: false, others: &[] }).unwrap().map(|u| u.used)
}

/// A stone target with a free face toward each of +X, +Y and +Z, and the
/// eye and aim point that click each face.
const TARGET: IVec3 = IVec3::new(10, 141, 10);
fn clicks() -> [(IVec3, DVec3, DVec3, u16); 3] {
    let c = TARGET.as_dvec3() + DVec3::splat(0.5);
    [
        (IVec3::Y, c + DVec3::new(0.0, 2.0, -1.0), c + DVec3::new(0.0, 0.5, 0.0), 0),
        (IVec3::X, c + DVec3::new(2.5, 0.2, 0.0), c + DVec3::new(0.5, 0.0, 0.0), 1),
        (IVec3::Z, c + DVec3::new(0.0, 0.2, 2.5), c + DVec3::new(0.0, 0.0, 0.5), 2),
    ]
}

#[test]
fn logs_wood_and_stripped_blocks_take_the_clicked_axis_and_axes_strip_them() {
    let items = wood::LOGS
        .into_iter()
        .chain(
            (0..9)
                .flat_map(|w| [Pillar::Wood, Pillar::StrippedLog, Pillar::StrippedWood].map(|k| wood::pillar(w, k, 0))),
        )
        .chain([Block(wood::STRIPPED_BAMBOO)]);
    for block in items {
        for (face, eye, point, axis) in clicks() {
            let (mut w, mut s, mut e) = scene(Item::from_block(block));
            w.set_block(TARGET, Block::STONE);
            place(&mut w, &mut s, eye, point);
            let placed = w.get_block(TARGET + face).unwrap();
            assert_eq!((placed.base(), wood::axis(placed)), (block, Some(axis)), "{} on {face}", block.name());
            // An axe strips it in place, keeping the axis, and wears.
            s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1));
            aim(&mut s, eye + face.as_dvec3(), point + face.as_dvec3());
            let used = s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] });
            let used = used.ok().flatten().map(|u| u.used);
            let after = w.get_block(TARGET + face).unwrap();
            match wood::stripped(placed) {
                Some(stripped) => {
                    assert_eq!(used, Some(Used::Stripped { cell: TARGET + face }), "{}", block.name());
                    assert_eq!(after, stripped);
                    assert_eq!(wood::axis(after), Some(axis));
                    assert_eq!(s.inventory.get(0).unwrap().damage, 1);
                }
                None => {
                    assert_eq!(used, None, "an axe does nothing to a stripped block");
                    assert_eq!(after, placed);
                }
            }
        }
    }
}

#[test]
fn switches_place_toggle_and_press_through_the_session() {
    for f in 0..switches::FAMILIES {
        // A button on the side of a block, pressed by hand.
        let (mut w, mut s, mut e) = scene(Item::from_block(switches::button(f)));
        w.set_block(TARGET, Block::STONE);
        let (face, eye, point, _) = clicks()[1];
        place(&mut w, &mut s, eye, point);
        let button = w.get_block(TARGET + face).unwrap();
        assert_eq!(switches::family(button.0), Some(f));
        assert!(matches!(r::component(button), Some(Component::Button { on: false, .. })));
        s.inventory.slots[0] = None;
        aim(&mut s, eye, TARGET.as_dvec3() + DVec3::new(1.05, 0.5, 0.5));
        assert_eq!(use_held(&mut s, &mut w, &mut e), Some(Used::Redstone { cell: TARGET + face }));
        let pressed = w.get_block(TARGET + face).unwrap();
        assert_eq!(switches::family(pressed.0), Some(f), "pressing keeps the family");
        assert!(matches!(r::component(pressed), Some(Component::Button { on: true, .. })));
        if f == 7 {
            continue;
        }
        // A trapdoor on the floor opens by hand and stays the same wood.
        let (mut w, mut s, mut e) = scene(Item::from_block(switches::trapdoor(f)));
        w.set_block(TARGET, Block::STONE);
        let (face, eye, point, _) = clicks()[0];
        place(&mut w, &mut s, eye, point);
        let at = TARGET + face;
        let closed = w.get_block(at).unwrap();
        assert!(matches!(r::component(closed), Some(Component::Trapdoor { open: false, top: false, .. })));
        s.inventory.slots[0] = None;
        aim(&mut s, eye, at.as_dvec3() + DVec3::new(0.5, 0.1, 0.5));
        assert_eq!(use_held(&mut s, &mut w, &mut e), Some(Used::Redstone { cell: at }));
        let open = w.get_block(at).unwrap();
        assert_eq!(switches::family(open.0), Some(f));
        assert!(matches!(r::component(open), Some(Component::Trapdoor { open: true, .. })));
    }
}

#[test]
fn new_states_survive_a_file_save_and_reload() {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut placed = Vec::new();
    let mut p = IVec3::new(1, 129, 1);
    let mut put = |w: &mut World, b: Block| {
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p, b);
        placed.push((p, b));
        p.x += 2;
        if p.x > 30 {
            p = IVec3::new(1, p.y, p.z + 2);
            if p.z > 30 {
                p = IVec3::new(1, p.y + 2, 1);
            }
        }
    };
    for id in wood::FIRST..=wood::LAST {
        let b = Block(id);
        // Every registered pillar and switch state (signs, shelves and the
        // bookshelf have their own save tests).
        let standing = match r::component(b) {
            Some(Component::Button { mount, .. }) => matches!(mount, 0 | 6..=8),
            Some(_) => true,
            None => false,
        };
        if wood::axis(b).is_some() || (switches::family(id).is_some() && standing) {
            put(&mut world, b);
        }
    }
    let dir = std::env::temp_dir().join(format!("voxelcraft-wood-track-{}", std::process::id()));
    let storage = Storage::new(&dir);
    storage.save(&LevelInfo { seed: 7, player: None, props: Default::default() }, &world.modified_chunks()).unwrap();
    let mut loaded = World::new_remote(Arc::new(Generator::new(7)), 2);
    for (pos, data) in storage.load_chunks().unwrap() {
        loaded.receive_chunk(pos, data);
    }
    for (p, b) in placed {
        assert_eq!(loaded.get_block(p), Some(b), "{} at {p}", b.name());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn signs_place_take_text_and_ink_through_the_session() {
    use voxelcraft::color::DyeColor;
    use voxelcraft::world::wood::signs::{self, Mount};
    let item = Item::from_block(
        signs::Sign { kind: wood::Kind::Oak, mount: Mount::Standing { rotation: 0 }, wet: false }.block(),
    );
    let (mut w, mut s, mut e) = scene(item);
    w.set_block(TARGET, Block::STONE);
    // Looking south and down onto the floor: a standing sign facing back north (rotation 8).
    let (face, eye, point, _) = clicks()[0];
    place(&mut w, &mut s, eye, point);
    let at = TARGET + face;
    let sign = signs::sign(w.get_block(at).unwrap()).unwrap();
    assert_eq!(sign.mount, Mount::Standing { rotation: 8 });
    assert_eq!(s.inventory.get(0).unwrap().count, 15, "signs stack to 16");
    // The edit screen's answer, then a black dye (no change), red dye and glow ink.
    let lines = ["Welcome".to_string(), "to".into(), "VoxelCraft".into(), String::new()];
    s.edit_sign(&mut w, at, true, lines).unwrap();
    assert_eq!(w.sign_at(at).unwrap().1.front.lines[2], "VoxelCraft");
    aim(&mut s, eye, at.as_dvec3() + DVec3::new(0.5, 0.8, 0.55));
    s.inventory.slots[0] = Some(Stack::new(DyeColor::Black.dye(), 2));
    assert_eq!(
        use_held(&mut s, &mut w, &mut e),
        Some(Used::EditSign { cell: at, front: true }),
        "black is the default"
    );
    s.inventory.slots[0] = Some(Stack::new(DyeColor::Red.dye(), 2));
    assert_eq!(use_held(&mut s, &mut w, &mut e), Some(Used::SignInked { cell: at, item: DyeColor::Red.dye() }));
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    s.inventory.slots[0] = Some(Stack::new(Item::GLOW_INK_SAC, 1));
    assert!(matches!(use_held(&mut s, &mut w, &mut e), Some(Used::SignInked { .. })));
    let front = &w.sign_at(at).unwrap().1.front;
    assert_eq!((front.color, front.glowing), (DyeColor::Red, true));
    // A bare hand opens the editor for the side faced; the back is blank, so dye does nothing there.
    s.inventory.slots[0] = None;
    assert_eq!(use_held(&mut s, &mut w, &mut e), Some(Used::EditSign { cell: at, front: true }));
    // Too far away, the edit is refused.
    s.player.pos += DVec3::new(0.0, 0.0, 20.0);
    assert!(s.edit_sign(&mut w, at, true, Default::default()).is_err());
}

#[test]
fn shelves_and_chiseled_bookshelves_trade_items_through_the_session() {
    use voxelcraft::world::block::Facing;
    use voxelcraft::world::wood::{ShelfUse, bookshelf, shelves};
    let shelf = shelves::Shelf {
        kind: wood::Kind::Jungle,
        facing: Facing::South,
        powered: false,
        chain: shelves::Chain::Unconnected,
        wet: false,
    };
    let (mut w, mut s, mut e) = scene(Item::from_block(shelf.block()));
    w.set_block(TARGET, Block::STONE);
    // Placed by a player north of it looking south, it faces north.
    let (face, eye, point, _) = clicks()[0];
    place(&mut w, &mut s, eye, point);
    let at = TARGET + face;
    assert_eq!(shelves::shelf(w.get_block(at).unwrap()).map(|s| s.facing), Some(Facing::North));
    s.inventory.slots[0] = Some(Stack::new(Item::APPLE, 3));
    // Seen from the north, the right slot is at the west end.
    let front = at.as_dvec3() + DVec3::new(0.15, 0.5, 0.0);
    aim(&mut s, front + DVec3::new(0.0, 0.6, -2.0), front);
    assert_eq!(use_held(&mut s, &mut w, &mut e), Some(Used::Shelf { cell: at, used: ShelfUse::Placed }));
    assert_eq!(w.wood_entities().shelf(at).unwrap()[2], Some(Stack::new(Item::APPLE, 3)), "the right slot");
    assert_eq!(
        use_held(&mut s, &mut w, &mut e),
        Some(Used::Shelf { cell: at, used: ShelfUse::Swapped { took: true } })
    );
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::APPLE, 3)));

    let (mut w, mut s, mut e) = scene(Item::BOOK);
    let at = TARGET;
    w.set_block(at, bookshelf::state(Facing::South, 0));
    let slot0 = at.as_dvec3() + DVec3::new(0.1, 0.9, 1.0);
    aim(&mut s, slot0 + DVec3::new(0.0, 0.3, 2.0), slot0);
    assert!(matches!(use_held(&mut s, &mut w, &mut e), Some(Used::Shelf { used: ShelfUse::BookIn { .. }, .. })));
    assert_eq!(bookshelf::slots(w.get_block(at).unwrap()), Some(1));
    assert_eq!(s.inventory.get(0).unwrap().count, 63);
    s.inventory.slots[0] = None;
    assert!(matches!(use_held(&mut s, &mut w, &mut e), Some(Used::Shelf { used: ShelfUse::BookOut { .. }, .. })));
    assert_eq!(s.inventory.get(0), Some(Stack::new(Item::BOOK, 1)));
}

#[test]
fn sign_text_and_shelf_items_survive_a_level_save() {
    use voxelcraft::world::block::Facing;
    use voxelcraft::world::wood::{shelves, signs};
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(TARGET - IVec3::Y, Block::STONE);
    let sign = signs::Sign { kind: wood::Kind::Mangrove, mount: signs::Mount::Standing { rotation: 6 }, wet: false };
    world.set_block(TARGET, sign.block());
    world.set_sign_text(TARGET, false, ["back".into(), String::new(), String::new(), "side".into()]).unwrap();
    let shelf_at = TARGET + IVec3::X * 2;
    let shelf = shelves::Shelf {
        kind: wood::Kind::Crimson,
        facing: Facing::East,
        powered: false,
        chain: shelves::Chain::Unconnected,
        wet: false,
    };
    world.set_block(shelf_at, shelf.block());
    let mut hotbar = [Some(Stack::new(Item::EMERALD, 7)), None, None, None, None, None, None, None, None];
    world
        .use_shelf(shelf_at, IVec3::X, shelf_at.as_dvec3() + DVec3::new(1.0, 0.5, 0.5), &mut hotbar, 0, false)
        .unwrap();
    let dir = std::env::temp_dir().join(format!("voxelcraft-wood-entities-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let props = [("wood".to_string(), world.wood_to_string())].into_iter().collect();
    storage.save(&LevelInfo { seed: 7, player: None, props }, &world.modified_chunks()).unwrap();
    let mut loaded = World::new_remote(Arc::new(Generator::new(7)), 2);
    for (pos, data) in storage.load_chunks().unwrap() {
        loaded.receive_chunk(pos, data);
    }
    loaded.load_wood(&storage.load_level().unwrap().props["wood"]);
    assert_eq!(loaded.get_block(TARGET), Some(sign.block()));
    assert_eq!(loaded.sign_at(TARGET).unwrap().1.back.lines[3], "side");
    assert_eq!(loaded.wood_entities().shelf(shelf_at).unwrap()[1], Some(Stack::new(Item::EMERALD, 7)));
    std::fs::remove_dir_all(dir).unwrap();
}
