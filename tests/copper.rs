//! v0.12 copper (BLD-01c) through the shared session paths every input uses
//! (`place_held` and `use_held`): Java 26.3's placement rules for torches,
//! lanterns, lightning rods, chains, chests, doors, stairs and trapdoors,
//! hand use of doors, and axe scraping.
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
        block::{Block, Facing, Shaped},
        chunk::ChunkData,
        copper::{self, Family, Stage},
        mount_states as ms,
        redstone_blocks::{self as r, Component},
        slab_stair_states as ss,
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

fn aim(session: &mut PlayerSession, eye: DVec3, point: DVec3) {
    let d = (point - eye).normalize();
    session.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    session.player.yaw = d.z.atan2(d.x) as f32;
    session.player.pitch = d.y.asin() as f32;
}

/// Places the held item with the eye at `eye`, looking at `point`.
fn place(world: &mut World, session: &mut PlayerSession, eye: DVec3, point: DVec3) -> Block {
    aim(session, eye, point);
    match session.place_held(0, world, &[]).unwrap() {
        voxelcraft::session::placement::Outcome::Placed { block, .. } => block,
        other => panic!("not placed: {other:?}"),
    }
}

fn item(family: Family, stage: Stage) -> Item {
    Item::from_block(copper::block(family, stage, 0))
}

#[test]
fn torches_stand_on_floors_and_hang_on_walls() {
    let (mut world, mut s) = scene(item(Family::Torch, Stage::Unaffected));
    let floor = IVec3::new(10, 139, 10);
    world.set_block(floor, Block::STONE);
    let standing = place(&mut world, &mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140.0, 10.5));
    assert_eq!(standing, copper::block(Family::Torch, Stage::Unaffected, 0));
    assert_eq!(standing.kind(), voxelcraft::world::block::RenderKind::Cross);
    // On the south face of a wall: a wall torch facing south.
    let wall = IVec3::new(14, 141, 10);
    world.set_block(wall, Block::STONE);
    let on_wall = place(&mut world, &mut s, DVec3::new(14.5, 141.6, 13.0), DVec3::new(14.5, 141.5, 11.0));
    assert_eq!(world.get_block(wall + IVec3::Z), Some(on_wall));
    assert_eq!(on_wall, copper::block(Family::Torch, Stage::Unaffected, 1 + Facing::South as u16));
    assert_eq!(on_wall.drop(), Some(item(Family::Torch, Stage::Unaffected)));
    assert_eq!(on_wall.emission(), 14);
}

#[test]
fn lanterns_hang_or_stand_and_take_the_water_they_are_placed_in() {
    let (mut world, mut s) = scene(item(Family::Lantern, Stage::Unaffected));
    let ceiling = IVec3::new(10, 143, 10);
    world.set_block(ceiling, Block::STONE);
    world.set_block(ceiling - IVec3::Y, Block::WATER);
    let hanging = place(&mut world, &mut s, DVec3::new(10.5, 141.0, 8.5), DVec3::new(10.5, 143.0, 10.5));
    assert_eq!(world.get_block(ceiling - IVec3::Y), Some(hanging));
    assert_eq!(copper::state(hanging).map(|c| c.local), Some(3), "hanging and waterlogged");
    let floor = IVec3::new(14, 139, 10);
    world.set_block(floor, Block::STONE);
    let standing = place(&mut world, &mut s, DVec3::new(14.5, 141.5, 8.0), DVec3::new(14.5, 140.0, 10.5));
    assert_eq!(standing, copper::block(Family::Lantern, Stage::Unaffected, 0));
}

#[test]
fn lightning_rods_point_out_of_every_clicked_face_and_waterlog() {
    let base = IVec3::new(10, 141, 10);
    let cases = [
        (DVec3::new(10.5, 145.5, 13.0), DVec3::new(10.5, 142.0, 10.5), IVec3::Y, 0),
        (DVec3::new(10.5, 137.5, 13.0), DVec3::new(10.5, 141.0, 10.5), IVec3::NEG_Y, 1),
        (DVec3::new(10.5, 141.5, 13.0), DVec3::new(10.5, 141.5, 11.0), IVec3::Z, 2 + Facing::South as u16),
        (DVec3::new(10.5, 141.5, 8.0), DVec3::new(10.5, 141.5, 10.0), IVec3::NEG_Z, 2 + Facing::North as u16),
        (DVec3::new(13.0, 141.5, 10.5), DVec3::new(11.0, 141.5, 10.5), IVec3::X, 2 + Facing::East as u16),
        (DVec3::new(8.0, 141.5, 10.5), DVec3::new(10.0, 141.5, 10.5), IVec3::NEG_X, 2 + Facing::West as u16),
    ];
    for (eye, point, normal, local) in cases {
        for wet in [false, true] {
            let (mut world, mut s) = scene(item(Family::Rod, Stage::Unaffected));
            world.set_block(base, Block::STONE);
            if wet {
                world.set_block(base + normal, Block::WATER);
            }
            let rod = place(&mut world, &mut s, eye, point);
            assert_eq!(world.get_block(base + normal), Some(rod));
            assert_eq!(copper::state(rod).map(|c| c.local), Some(local + 6 * wet as u16), "{normal} wet {wet}");
            assert_eq!(rod.is_waterlogged(), wet);
            assert_eq!(rod.drop(), Some(item(Family::Rod, Stage::Unaffected)));
        }
    }
}

#[test]
fn chains_follow_the_clicked_axis_and_chests_face_the_placer() {
    let (mut world, mut s) = scene(item(Family::Chain, Stage::Unaffected));
    let wall = IVec3::new(10, 141, 10);
    world.set_block(wall, Block::STONE);
    let side = place(&mut world, &mut s, DVec3::new(13.0, 141.5, 10.5), DVec3::new(11.0, 141.5, 10.5));
    assert_eq!(side.shaped(), Some(Shaped::Chain(1)), "along x");
    let top = place(&mut world, &mut s, DVec3::new(10.5, 145.5, 13.0), DVec3::new(10.5, 142.0, 10.4));
    assert_eq!(top.shaped(), Some(Shaped::Chain(0)), "along y");
    let (mut world, mut s) = scene(item(Family::Chest, Stage::Unaffected));
    world.set_block(IVec3::new(10, 139, 10), Block::STONE);
    // Looking north: the chest's front faces back south at the player.
    let chest = place(&mut world, &mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140.0, 10.5));
    assert_eq!(chest.oriented(), Some((copper::block(Family::Chest, Stage::Unaffected, 0), Facing::South)));
    assert!(world.chest(IVec3::new(10, 140, 10)).is_some(), "it holds 27 slots");
}

#[test]
fn doors_hinge_open_by_hand_and_stairs_take_their_half() {
    let (mut world, mut s) = scene(item(Family::Door, Stage::Exposed));
    for x in 8..14 {
        world.set_block(IVec3::new(x, 139, 10), Block::STONE);
    }
    let first = IVec3::new(10, 140, 10);
    place(&mut world, &mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140.0, 10.5));
    place(&mut world, &mut s, DVec3::new(11.5, 141.6, 13.0), DVec3::new(11.5, 140.0, 10.5));
    let second = world.get_block(first + IVec3::X).unwrap();
    assert!(second.is_door() && ms::is_right_hinge(second), "the second door hinges away from the first");
    assert!(world.get_block(first + IVec3::new(1, 1, 0)).is_some_and(Block::is_door_upper));
    // Bare hands open it (`BlockSetType.COPPER` opens by hand).
    s.inventory.slots[0] = None;
    aim(&mut s, DVec3::new(11.5, 141.6, 13.0), DVec3::new(11.5, 141.0, 10.9));
    let mut entities = Entities::new(7);
    let used = s.use_held(0, &mut world, &mut entities, &UseContext { sneaking: false, others: &[] }).unwrap();
    assert_eq!(used.map(|u| u.used), Some(Used::Door { cell: first + IVec3::new(1, 1, 0), open: Some(true) }));
    assert!(matches!(world.get_block(first + IVec3::X).unwrap().shaped(), Some(Shaped::Door { open: true, .. })));
    // Stairs: clicking a side above its middle gives the upside-down half.
    let (mut world, mut s) = scene(item(Family::Stairs, Stage::Oxidized));
    let wall = IVec3::new(10, 141, 10);
    world.set_block(wall, Block::STONE);
    let stairs = place(&mut world, &mut s, DVec3::new(10.5, 141.9, 13.0), DVec3::new(10.5, 141.8, 11.0));
    let st = ss::stairs(stairs).unwrap();
    assert!(st.top);
    assert_eq!(copper::state(stairs).map(|c| c.stage), Some(Stage::Oxidized));
    // Trapdoors on a ceiling's underside are top halves.
    let (mut world, mut s) = scene(item(Family::Trapdoor, Stage::Weathered));
    let ceiling = IVec3::new(10, 143, 10);
    world.set_block(ceiling, Block::STONE);
    let trapdoor = place(&mut world, &mut s, DVec3::new(10.5, 141.0, 8.5), DVec3::new(10.5, 143.0, 10.5));
    assert!(matches!(r::component(trapdoor), Some(Component::Trapdoor { top: true, iron: false, .. })));
    assert_eq!(copper::state(trapdoor).map(|c| c.stage), Some(Stage::Weathered));
}

#[test]
fn a_sneaking_axe_scrapes_one_stage_and_wears() {
    let (mut world, mut s) = scene(Item::tool(ToolKind::Axe, Tier::Iron));
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1));
    let target = IVec3::new(7, 146, 4);
    world.set_block(target, copper::block(Family::Cut, Stage::Oxidized, 0));
    aim(&mut s, DVec3::new(4.5, 146.5, 4.5), DVec3::new(7.0, 146.5, 4.5));
    let mut entities = Entities::new(7);
    let ctx = UseContext { sneaking: true, others: &[] };
    assert_eq!(s.block_target(&world).map(|(p, _)| p), Some(target));
    for stage in [Stage::Weathered, Stage::Exposed, Stage::Unaffected] {
        let used = s.use_held(0, &mut world, &mut entities, &ctx).unwrap().map(|u| u.used);
        assert_eq!(used, Some(Used::Scraped { cell: target }));
        assert_eq!(world.get_block(target), Some(copper::block(Family::Cut, stage, 0)));
    }
    assert_eq!(s.inventory.get(0).map(|st| st.damage), Some(3), "one durability per scrape");
    // Nothing left to scrape: the use falls through to placing, which an axe can't.
    assert!(s.use_held(0, &mut world, &mut entities, &ctx).is_err());
    assert_eq!(world.get_block(target), Some(copper::block(Family::Cut, Stage::Unaffected, 0)));
}
