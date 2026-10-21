//! BLD-04c/e through the shared placement path (desktop, controller, LAN and
//! CLI all call `place_held`): door hinges, floor and ceiling switches and
//! lanterns (Java 26.3 `DoorBlock.getHinge`,
//! `FaceAttachedHorizontalDirectionalBlock` and `LanternBlock`).
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    entity::PlayerId,
    inventory::Stack,
    item::Item,
    player::EYE_HEIGHT,
    session::PlayerSession,
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        mount_states::{self as ms, AttachFace},
        redstone_blocks::{self as r, Component},
        terrain::Generator,
    },
};

fn scene(item: Item) -> (World, PlayerSession) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut session = PlayerSession::new(PlayerId::HOST, DVec3::new(4.5, 145., 4.5));
    session.inventory.slots[0] = Some(Stack::new(item, 64));
    (world, session)
}

/// Places the held item with the eye at `eye`, looking at `point`.
fn place(world: &mut World, session: &mut PlayerSession, eye: DVec3, point: DVec3) {
    let d = (point - eye).normalize();
    session.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    session.player.yaw = d.z.atan2(d.x) as f32;
    session.player.pitch = d.y.asin() as f32;
    session.place_held(0, world, &[]).unwrap();
}

fn mount(world: &World, p: IVec3) -> (AttachFace, Facing) {
    match world.get_block(p).and_then(r::component) {
        Some(Component::Lever { mount, .. } | Component::Button { mount, .. }) => ms::mount_attach(mount),
        other => panic!("no switch at {p}: {other:?}"),
    }
}

#[test]
fn a_second_door_beside_the_first_hinges_away_from_it() {
    let (mut world, mut s) = scene(Item::OAK_DOOR);
    for x in 8..14 {
        world.set_block(IVec3::new(x, 139, 10), Block::STONE);
    }
    // Looking north onto the floor's middle: the click decides (left).
    let first = IVec3::new(10, 140, 10);
    place(&mut world, &mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140.0, 10.5));
    let lower = world.get_block(first).unwrap();
    assert!(lower.is_door() && !lower.is_door_upper() && !ms::is_right_hinge(lower));
    // East of it, the door on its left makes this one hinge on the right.
    let second = first + IVec3::X;
    place(&mut world, &mut s, DVec3::new(11.5, 141.6, 13.0), DVec3::new(11.5, 140.0, 10.5));
    let lower = world.get_block(second).unwrap();
    assert!(lower.is_door() && ms::is_right_hinge(lower));
    assert!(world.get_block(second + IVec3::Y).is_some_and(Block::is_door_upper));
    // Opening it keeps the hinge.
    let (_, facing) = lower.oriented().unwrap();
    assert!(ms::is_right_hinge(lower.toggled(facing)));
}

#[test]
fn floor_and_ceiling_switches_face_where_the_player_looks() {
    let (mut world, mut s) = scene(Item::from_block(r::STONE_BUTTON));
    let floor = IVec3::new(10, 139, 10);
    world.set_block(floor, Block::STONE);
    // Looking down and east onto the floor's top.
    place(&mut world, &mut s, DVec3::new(9.0, 141.5, 10.5), DVec3::new(10.5, 140.0, 10.5));
    assert_eq!(mount(&world, floor + IVec3::Y), (AttachFace::Floor, Facing::East));
    // A lever under a ceiling, looking up and south.
    let (mut world, mut s) = scene(Item::from_block(r::LEVER));
    let ceiling = IVec3::new(10, 143, 10);
    world.set_block(ceiling, Block::STONE);
    place(&mut world, &mut s, DVec3::new(10.5, 141.0, 8.5), DVec3::new(10.5, 143.0, 10.5));
    assert_eq!(mount(&world, ceiling - IVec3::Y), (AttachFace::Ceiling, Facing::South));
    // Looking mostly sideways at a wall still mounts it on the wall.
    let wall = IVec3::new(14, 141, 10);
    world.set_block(wall, Block::STONE);
    place(&mut world, &mut s, DVec3::new(14.5, 142.0, 7.0), DVec3::new(14.5, 141.6, 10.0));
    assert_eq!(mount(&world, wall - IVec3::Z), (AttachFace::Wall, Facing::North));
}

#[test]
fn lanterns_hang_from_a_ceiling_and_stand_on_a_floor() {
    let (mut world, mut s) = scene(Item::from_block(ms::LANTERN));
    let ceiling = IVec3::new(10, 143, 10);
    world.set_block(ceiling, Block::STONE);
    place(&mut world, &mut s, DVec3::new(10.5, 141.0, 8.5), DVec3::new(10.5, 143.0, 10.5));
    assert_eq!(world.get_block(ceiling - IVec3::Y), Some(ms::lantern_block(false, true, false)));
    let floor = IVec3::new(14, 139, 10);
    world.set_block(floor, Block::STONE);
    place(&mut world, &mut s, DVec3::new(14.5, 141.5, 8.0), DVec3::new(14.5, 140.0, 10.5));
    assert_eq!(world.get_block(floor + IVec3::Y), Some(ms::LANTERN));
    assert_eq!(s.inventory.get(0).unwrap().count, 62);
}
