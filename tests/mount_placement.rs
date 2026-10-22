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

#[test]
fn every_door_opens_immediately_when_either_half_is_placed_beside_power() {
    use voxelcraft::world::{block::Shaped, forms};
    let items = [Item::OAK_DOOR, Item::from_block(r::IRON_DOOR)]
        .into_iter()
        .chain((0..11).map(|i| Item::from_block(forms::wood_id(i, 14))));
    for item in items {
        for power_y in [None, Some(0), Some(1)] {
            let (mut world, mut s) = scene(item);
            let lower = IVec3::new(10, 140, 10);
            world.set_block(lower - IVec3::Y, Block::STONE);
            if let Some(y) = power_y {
                world.set_block(lower + IVec3::X + IVec3::Y * y, r::REDSTONE_BLOCK);
            }
            place(&mut world, &mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140.0, 10.5));
            let expected = power_y.is_some();
            let open = |world: &World| match world.get_block(lower).unwrap().shaped() {
                Some(Shaped::Door { open, upper: false, .. }) => open,
                other => panic!("no door for {}: {other:?}", item.name()),
            };
            assert_eq!(open(&world), expected, "{} / power at {power_y:?}, before ticks", item.name());
            assert!(world.get_block(lower + IVec3::Y).unwrap().is_door_upper());
            if let Some(Component::IronDoor { open, .. }) = world.get_block(lower + IVec3::Y).and_then(r::component) {
                assert_eq!(open, expected, "iron upper half");
            }
            for _ in 0..5 {
                world.tick_redstone();
            }
            assert_eq!(open(&world), expected, "after ticks");
            if let Some(y) = power_y {
                world.set_block(lower + IVec3::X + IVec3::Y * y, Block::AIR);
                for _ in 0..6 {
                    world.tick_redstone();
                }
                assert!(!open(&world), "removing either half's power closes the door");
                world.set_block(lower + IVec3::X + IVec3::Y * y, r::REDSTONE_BLOCK);
                for _ in 0..6 {
                    world.tick_redstone();
                }
                assert!(open(&world), "restoring power opens the door");
            }
            // Removing power above closes an open door for every family; then
            // a powered wooden door closed by hand stays closed (POWERED is
            // remembered, so the next update isn't a new rising edge).
            if expected && !matches!(world.get_block(lower).and_then(r::component), Some(Component::IronDoor { .. })) {
                voxelcraft::session::interact::toggle_door(&mut world, lower, s.player.forward());
                for _ in 0..5 {
                    world.tick_redstone();
                }
                assert!(!open(&world), "manual close while powered");
            }
        }
    }
}

#[test]
fn powered_placement_keeps_a_right_hinge_and_the_saved_powered_edge() {
    use voxelcraft::world::block::Shaped;
    let (mut world, mut s) = scene(Item::OAK_DOOR);
    let lower = IVec3::new(10, 140, 10);
    world.set_block(lower - IVec3::Y, Block::STONE);
    world.set_block(lower - IVec3::X - IVec3::Y, Block::STONE);
    world.set_block(lower - IVec3::X, Block::door(Facing::North, false, false));
    world.set_block(lower - IVec3::X + IVec3::Y, Block::door(Facing::North, false, true));
    world.set_block(lower + IVec3::X, r::REDSTONE_BLOCK);
    place(&mut world, &mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140.0, 10.5));
    assert!(ms::is_right_hinge(world.get_block(lower).unwrap()));
    assert!(matches!(world.get_block(lower).unwrap().shaped(), Some(Shaped::Door { open: true, .. })));
    // Close before the first redstone tick; a spurious rising edge would
    // reopen it. The placement POWERED edge also survives serialization.
    voxelcraft::session::interact::toggle_door(&mut world, lower, s.player.forward());
    let redstone = world.redstone_to_string();
    let mut replica = World::new_remote(Arc::new(Generator::new(7)), 2);
    for (pos, data) in world.modified_chunks() {
        replica.receive_chunk(pos, data);
    }
    replica.load_redstone(&redstone);
    for _ in 0..6 {
        replica.tick_redstone();
    }
    assert!(matches!(replica.get_block(lower).unwrap().shaped(), Some(Shaped::Door { open: false, .. })));
    assert!(ms::is_right_hinge(replica.get_block(lower).unwrap()));
}

#[test]
fn every_wall_places_with_the_java_post_and_keeps_its_water_and_item() {
    use voxelcraft::{session::placement::Outcome, world::wall_states};
    for wall in wall_states::FAMILIES {
        for wet in [false, true] {
            let item = Item::from_block(wall);
            let (mut world, mut s) = scene(item);
            let cell = IVec3::new(10, 140, 10);
            world.set_block(cell - IVec3::Y, Block::STONE);
            world.set_block(cell - IVec3::X, wall);
            world.set_block(cell + IVec3::X, wall);
            if wet {
                world.set_block(cell, Block::WATER);
            }
            let eye = DVec3::new(10.5, 141.6, 13.0);
            let point = DVec3::new(10.5, 140.0, 10.5);
            let d = (point - eye).normalize();
            s.player.pos = eye - DVec3::Y * EYE_HEIGHT;
            s.player.yaw = d.z.atan2(d.x) as f32;
            s.player.pitch = d.y.asin() as f32;
            let Outcome::Placed { block, cell: placed, .. } = s.place_held(0, &mut world, &[]).unwrap() else {
                panic!("wall not placed")
            };
            assert_eq!(placed, cell);
            assert_eq!(world.get_block(cell), Some(block));
            assert_eq!(wall_states::up(block), Some(false));
            assert_eq!(block.is_waterlogged(), wet);
            assert_eq!(voxelcraft::session::interact::picked_item(block), item);
            assert_eq!(block.drop(), Some(item));
            assert_eq!(s.inventory.get(0).unwrap().count, 63);
        }
    }
}

#[test]
fn legacy_iron_door_helper_also_samples_power_at_either_half() {
    use voxelcraft::world::block::Shaped;
    for y in [0, 1] {
        let (mut world, _) = scene(Item::OAK_DOOR);
        let p = IVec3::new(10, 140, 10);
        world.set_block(p - IVec3::Y, Block::STONE);
        world.set_block(p + IVec3::X + IVec3::Y * y, r::REDSTONE_BLOCK);
        assert!(world.place_iron_door(p, Facing::North));
        for cell in [p, p + IVec3::Y] {
            assert!(matches!(world.get_block(cell).unwrap().shaped(), Some(Shaped::Door { open: true, .. })));
        }
    }
}
