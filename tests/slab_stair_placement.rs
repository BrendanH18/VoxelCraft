//! BLD-04a/b: slab types and stairs halves and corners through the shared
//! placement path (desktop, controller, LAN and CLI all call `place_held`),
//! neighbour updates, drops and saves (Java 26.3 `SlabBlock`, `StairBlock`).
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::Item,
    player::EYE_HEIGHT,
    session::{
        PlayerSession,
        placement::{Kind, Outcome},
    },
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        slab_stair_states::{self as ss, FAMILIES, Slab, SlabType, StairShape, Stairs},
        storage::Storage,
        terrain::Generator,
    },
};

const STONE_SLAB: Block = FAMILIES[0].slab;

fn scene(item: Item) -> (World, PlayerSession) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut session = PlayerSession::new(PlayerId::HOST, DVec3::new(4.5, 145., 4.5));
    session.inventory.slots[0] = Some(Stack::new(item, 64));
    (world, session)
}

/// Stands the player with their eye at `eye`, looking at `point`.
fn aim(session: &mut PlayerSession, eye: DVec3, point: DVec3) {
    let d = (point - eye).normalize();
    session.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    session.player.yaw = d.z.atan2(d.x) as f32;
    session.player.pitch = d.y.asin() as f32;
}

fn place(world: &mut World, session: &mut PlayerSession, eye: DVec3, point: DVec3) -> Result<Outcome, &'static str> {
    aim(session, eye, point);
    session.place_held(0, world, &[])
}

fn slab_at(world: &World, p: IVec3) -> Option<Slab> {
    ss::slab(world.get_block(p)?)
}

#[test]
fn slab_halves_follow_the_clicked_face_and_height() {
    let (mut world, mut s) = scene(Item::from_block(STONE_SLAB));
    let stone = IVec3::new(10, 140, 10);
    world.set_block(stone, Block::STONE);
    let eye = DVec3::new(10.5, 141.0, 7.0);
    // The north side above and below its middle.
    place(&mut world, &mut s, eye, DVec3::new(10.5, 140.75, 10.0)).unwrap();
    assert_eq!(slab_at(&world, stone - IVec3::Z).map(|s| s.kind), Some(SlabType::Top));
    world.set_block(stone - IVec3::Z, Block::AIR);
    place(&mut world, &mut s, eye, DVec3::new(10.5, 140.25, 10.0)).unwrap();
    assert_eq!(world.get_block(stone - IVec3::Z), Some(STONE_SLAB), "bottom slabs keep the existing id");
    world.set_block(stone - IVec3::Z, Block::AIR);
    // The top face: bottom, however high the click.
    place(&mut world, &mut s, DVec3::new(10.5, 142.6, 8.0), DVec3::new(10.5, 141.0, 10.9)).unwrap();
    assert_eq!(world.get_block(stone + IVec3::Y), Some(STONE_SLAB));
    // The underside: top.
    let ceiling = IVec3::new(14, 143, 10);
    world.set_block(ceiling, Block::STONE);
    place(&mut world, &mut s, DVec3::new(14.5, 141.6, 7.5), DVec3::new(14.5, 143.0, 10.5)).unwrap();
    assert_eq!(slab_at(&world, ceiling - IVec3::Y).map(|s| s.kind), Some(SlabType::Top));
    assert_eq!(s.inventory.get(0).unwrap().count, 60);
}

#[test]
fn slabs_merge_into_a_double_slab_from_the_open_side_or_into_their_cell() {
    let double = ss::slab_id(Slab { family: 0, kind: SlabType::Double, wet: false });
    let (mut world, mut s) = scene(Item::from_block(STONE_SLAB));
    let cell = IVec3::new(10, 140, 9);
    // A bottom slab clicked on its top.
    world.set_block(cell, STONE_SLAB);
    let out = place(&mut world, &mut s, DVec3::new(10.5, 142.5, 6.5), DVec3::new(10.5, 140.5, 9.5)).unwrap();
    assert!(matches!(out, Outcome::Placed { kind: Kind::CombinedSlab, cell: c, .. } if c == cell));
    assert_eq!(world.get_block(cell), Some(double));
    // A top slab clicked on its underside.
    let top = ss::slab_id(Slab { family: 0, kind: SlabType::Top, wet: false });
    world.set_block(cell, top);
    place(&mut world, &mut s, DVec3::new(10.5, 138.6, 7.0), DVec3::new(10.5, 140.5, 9.5)).unwrap();
    assert_eq!(world.get_block(cell), Some(double));
    // A bottom slab's side (all of it below the middle) places beside it.
    world.set_block(cell, STONE_SLAB);
    place(&mut world, &mut s, DVec3::new(10.5, 140.3, 7.0), DVec3::new(10.5, 140.3, 9.0)).unwrap();
    assert_eq!(world.get_block(cell), Some(STONE_SLAB));
    assert_eq!(world.get_block(cell - IVec3::Z), Some(STONE_SLAB));
    // Clicking a block beside a slab cell fills that cell (`canPlace` on the
    // relative position), whatever the click height.
    let wall = IVec3::new(16, 140, 10);
    world.set_block(wall, Block::STONE);
    world.set_block(wall - IVec3::Z, top);
    // Under the top slab, onto the wall's north face.
    place(&mut world, &mut s, DVec3::new(16.5, 140.2, 7.0), DVec3::new(16.5, 140.2, 10.0)).unwrap();
    assert_eq!(world.get_block(wall - IVec3::Z), Some(double));
    // Another kind of slab doesn't merge: it goes above.
    let (mut world, mut s) = scene(Item::from_block(FAMILIES[1].slab));
    world.set_block(cell, STONE_SLAB);
    place(&mut world, &mut s, DVec3::new(10.5, 142.5, 6.5), DVec3::new(10.5, 140.5, 9.5)).unwrap();
    assert_eq!(world.get_block(cell), Some(STONE_SLAB));
    assert_eq!(world.get_block(cell + IVec3::Y), Some(FAMILIES[1].slab));
}

#[test]
fn a_double_slab_is_refused_inside_a_player_and_both_adapters_agree() {
    let double = ss::slab_id(Slab { family: 0, kind: SlabType::Double, wet: false });
    let setup = || {
        let (mut world, mut s) = scene(Item::from_block(STONE_SLAB));
        world.set_block(IVec3::new(7, 144, 4), Block::STONE);
        world.set_block(IVec3::new(7, 145, 4), STONE_SLAB);
        aim(&mut s, DVec3::new(5.0, 147.0, 4.5), DVec3::new(7.5, 145.5, 4.5));
        (world, s)
    };
    let (mut world, mut s) = setup();
    let (mut cli_world, cs) = setup();
    let mut cli = Agent::new(cs.player.pos);
    cli.session = cs;
    s.place_held(0, &mut world, &[]).unwrap();
    cli.execute(Command::Place, &mut cli_world, &mut Entities::new(7), &[]).unwrap();
    assert_eq!(world.get_block(IVec3::new(7, 145, 4)), Some(double));
    assert_eq!(cli_world.get_block(IVec3::new(7, 145, 4)), Some(double));
    // Someone standing on the slab blocks its upper half.
    let (mut world, mut s) = setup();
    let inventory = s.inventory.serialize();
    assert!(s.place_held(0, &mut world, &[DVec3::new(7.5, 145.5, 4.5)]).is_err());
    assert_eq!(world.get_block(IVec3::new(7, 145, 4)), Some(STONE_SLAB));
    assert_eq!(s.inventory.serialize(), inventory);
}

#[test]
fn stairs_face_the_player_take_the_clicked_half_and_turn_corners() {
    let stairs_item = Item::from_block(FAMILIES[2].stairs);
    let (mut world, mut s) = scene(stairs_item);
    for x in 8..14 {
        world.set_block(IVec3::new(x, 139, 12), Block::STONE);
    }
    // Looking north (-Z) onto the top face: Java facing north, our front south.
    place(&mut world, &mut s, DVec3::new(10.5, 141.5, 15.0), DVec3::new(10.5, 140.0, 12.5)).unwrap();
    let a = IVec3::new(10, 140, 12);
    let state = ss::stairs(world.get_block(a).unwrap()).unwrap();
    assert_eq!((state.facing, state.top, state.shape), (Facing::South, false, StairShape::Straight));
    assert_eq!(world.get_block(a), Some(FAMILIES[2].stairs.with_facing(Facing::South)), "existing id");
    // Upside down from a side clicked above its middle.
    world.set_block(IVec3::new(13, 140, 9), Block::STONE);
    place(&mut world, &mut s, DVec3::new(13.5, 141.0, 6.0), DVec3::new(13.5, 140.8, 9.0)).unwrap();
    let up = ss::stairs(world.get_block(IVec3::new(13, 140, 8)).unwrap()).unwrap();
    assert!(up.top);
    // A stairs north of `a` facing (Java) east: placing it makes `a` an outer corner.
    world.set_block(IVec3::new(10, 139, 11), Block::STONE);
    place(&mut world, &mut s, DVec3::new(7.0, 141.5, 11.5), DVec3::new(10.5, 140.0, 11.5)).unwrap();
    let b = IVec3::new(10, 140, 11);
    assert_eq!(ss::stairs(world.get_block(b).unwrap()).unwrap().facing, Facing::West);
    assert_eq!(ss::stairs(world.get_block(a).unwrap()).unwrap().shape, StairShape::OuterRight);
    // Breaking it straightens `a` again (`updateShape`).
    world.set_block(b, Block::AIR);
    assert_eq!(ss::stairs(world.get_block(a).unwrap()).unwrap().shape, StairShape::Straight);
    // Placed in front of an existing stairs, the new one takes the corner itself.
    world.set_block(IVec3::new(11, 140, 12), FAMILIES[2].stairs.with_facing(Facing::West));
    let c = ss::stairs(world.get_block(IVec3::new(11, 140, 12)).unwrap()).unwrap();
    assert_eq!(c.shape, StairShape::Straight, "a raw edit doesn't reshape the edited cell");
    assert_eq!(ss::stairs(world.get_block(a).unwrap()).unwrap().shape, StairShape::Straight);
}

#[test]
fn explosions_and_other_edits_update_neighbouring_corners() {
    let (mut world, _) = scene(Item::from_block(FAMILIES[2].stairs));
    let a = IVec3::new(10, 140, 12);
    let state =
        |family, facing, top| ss::stairs_id(Stairs { family, facing, top, shape: StairShape::Straight, wet: false });
    // Upside-down blackstone stairs, Java north (front south), at `a`; oak
    // stairs Java east (front west) north of it.
    world.set_block(a, state(36, Facing::South, true));
    world.set_block(a - IVec3::Z, state(2, Facing::West, false));
    // Different halves never join.
    assert_eq!(ss::stairs(world.get_block(a).unwrap()).unwrap().shape, StairShape::Straight);
    world.set_block(a - IVec3::Z, state(2, Facing::West, true));
    assert_eq!(ss::stairs(world.get_block(a).unwrap()).unwrap().shape, StairShape::OuterRight);
    // An explosion too small to reach `a` (radius 0.95 from the oak
    // stairs' centre) takes the oak stairs, and `a` straightens.
    world.explode_with_drops((a - IVec3::Z).as_dvec3() + DVec3::splat(0.5), 0.5, false);
    assert_eq!(world.get_block(a - IVec3::Z), Some(Block::AIR));
    let left = ss::stairs(world.get_block(a).unwrap()).unwrap();
    assert_eq!((left.top, left.shape), (true, StairShape::Straight));
}

#[test]
fn double_slabs_drop_two_and_every_state_drops_its_item() {
    let (mut world, _) = scene(Item::from_block(STONE_SLAB));
    world.set_tile_drops(true);
    let p = IVec3::new(9, 140, 9);
    for (block, count) in [
        (ss::slab_id(Slab { family: 0, kind: SlabType::Double, wet: false }), 2),
        (ss::slab_id(Slab { family: 0, kind: SlabType::Top, wet: true }), 1),
        (STONE_SLAB, 1),
    ] {
        world.drops.clear();
        world.spill_block(p, block);
        assert_eq!(world.drops, vec![(p, Stack::new(Item::from_block(STONE_SLAB), count))], "{block:?}");
    }
    world.drops.clear();
    let corner =
        ss::stairs_id(Stairs { family: 5, facing: Facing::East, top: true, shape: StairShape::InnerLeft, wet: true });
    world.spill_block(p, corner);
    assert_eq!(world.drops, vec![(p, Stack::new(Item::from_block(FAMILIES[5].stairs), 1))]);
}

#[test]
fn new_states_survive_save_and_reload() {
    let dir = std::env::temp_dir().join(format!("voxelcraft-slab-stair-states-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let storage = Storage::new(&dir);
    let mut data = ChunkData::Uniform(Block::AIR);
    let states: Vec<Block> = (ss::STAIRS_ORIGIN..ss::END).map(Block).chain([STONE_SLAB, FAMILIES[2].stairs]).collect();
    for (i, &b) in states.iter().enumerate() {
        data.set(i % 32, i / 1024, i / 32 % 32, b);
    }
    let chunk = Arc::new(data);
    let level = voxelcraft::world::storage::LevelInfo { seed: 7, player: None, props: Default::default() };
    storage.save(&level, &[(IVec3::new(0, 4, 0), chunk.clone())]).unwrap();
    let loaded = storage.load_chunks().unwrap();
    let back = &loaded[&IVec3::new(0, 4, 0)];
    for (i, &b) in states.iter().enumerate() {
        assert_eq!(back.get(i % 32, i / 1024, i / 32 % 32), b);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_creative_palette_lists_each_stairs_and_slab_item_once() {
    let palette: Vec<Block> = Block::creative_palette().collect();
    assert!(palette.iter().all(|b| b.0 < ss::STAIRS_ORIGIN || b.0 > ss::RESERVED_LAST));
    for family in FAMILIES {
        let stairs = palette.iter().filter(|b| b.stairs_base() == Some(family.base)).count();
        let slabs = palette.iter().filter(|b| b.slab_base() == Some(family.base)).count();
        assert_eq!((stairs, slabs), (1, 1), "{}", family.stairs.name());
        assert!(palette.contains(&family.stairs) && palette.contains(&family.slab));
    }
}
