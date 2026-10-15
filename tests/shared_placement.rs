//! Real CLI requests and the desktop's shared placement entry point.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::Item,
    rules::GameMode,
    session::{
        PlayerSession,
        placement::{Kind, Outcome},
    },
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        forms, nether_biome_blocks as nb, overworld_blocks as ob,
        terrain::{Dimension, Generator},
    },
};
const WALL: IVec3 = IVec3::new(7, 146, 4);
const AT: IVec3 = IVec3::new(6, 146, 4);

#[test]
fn slab_targeting_uses_the_half_height_and_the_actual_top_face() {
    let slab = Block::slab_of(Block::STONE).unwrap();
    let (mut world, _, _) = scene(Item::from_block(slab));
    world.set_block(WALL, slab);
    assert!(world.raycast(DVec3::new(4.5, 146.75, 4.5), DVec3::X, 4.5).is_none());
    let above = DVec3::new(7.5, 148., 4.5);
    assert!(world.raycast(above, DVec3::NEG_Y, 1.4).is_none());
    assert_eq!(world.raycast(above, DVec3::NEG_Y, 1.6), Some((WALL, IVec3::Y)));
    assert_eq!(world.raycast(DVec3::new(7., 148., 4.), DVec3::NEG_Y, 1.6), Some((WALL, IVec3::Y)));
    assert_eq!(world.raycast(DVec3::new(7., 148., 4.), DVec3::new(-0., -1., -0.), 1.6), Some((WALL, IVec3::Y)));
    assert!(world.raycast(above, DVec3::ZERO, 2.).is_none());
    assert!(world.raycast(above, DVec3::NEG_Y, f64::NAN).is_none());
}

#[test]
fn cross_plant_empty_corners_do_not_hide_blocks_from_either_adapter() {
    let (mut w, mut direct, _) = scene(Item::from_block(Block::DIRT));
    // A sapling occupies x/z=2..14, y=0..12 and has no movement collision.
    let plant = AT;
    w.set_block(plant - IVec3::Y, Block::DIRT);
    w.set_block(plant, Block::OAK_SAPLING);
    let eye = DVec3::new(4.5, 146.1, 4.05);
    direct.player.pos = eye - DVec3::Y * voxelcraft::player::EYE_HEIGHT;
    direct.player.yaw = 0.;
    direct.player.pitch = 0.;
    let mut cli = Agent::new(direct.player.pos);
    cli.session.player.yaw = 0.;
    cli.session.player.pitch = 0.;
    assert_eq!(w.raycast(eye, DVec3::X, 4.5), Some((WALL, IVec3::NEG_X)));
    assert_eq!(direct.block_target(&w), cli.session.block_target(&w));
    assert_eq!(direct.block_target(&w).unwrap().0, WALL);
    let centre = DVec3::new(4.5, 146.5, 4.5);
    assert_eq!(w.raycast(centre, DVec3::X, 4.5), Some((plant, IVec3::NEG_X)));
}

fn scene(item: Item) -> (World, PlayerSession, Entities) {
    scene_in(item, Dimension::Overworld)
}
fn scene_in(item: Item, dimension: Dimension) -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::for_dimension(7, dimension)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(WALL, Block::STONE);
    world.set_block(AT - IVec3::Y, Block::STONE);
    let mut session = PlayerSession::new(PlayerId::HOST, DVec3::new(4.5, 145., 4.5));
    session.inventory.slots[0] = Some(Stack::new(item, 1));
    (world, session, Entities::new(7))
}

#[test]
fn every_existing_door_family_and_ladders_place_through_both_adapters() {
    let names = [
        "oak_door",
        "spruce_door",
        "birch_door",
        "jungle_door",
        "acacia_door",
        "dark_oak_door",
        "mangrove_door",
        "cherry_door",
        "crimson_door",
        "warped_door",
        "pale_oak_door",
        "bamboo_door",
        "iron_door",
        "ladder",
    ];
    for name in names {
        let item = Item::from_name(name).unwrap();
        let (mut client_world, mut client, _) = scene(item);
        let (mut cli_world, session, mut entities) = scene(item);
        let mut cli = Agent::new(session.player.pos);
        cli.session = session;
        let result = client.place_held(0, &mut client_world, &[]).unwrap();
        assert!(matches!(result, Outcome::Placed { kind: Kind::Door | Kind::Ladder, .. }), "{name}: {result:?}");
        cli.execute(Command::Place, &mut cli_world, &mut entities, &[]).unwrap();
        for cell in [AT, AT + IVec3::Y, WALL] {
            assert_eq!(client_world.get_block(cell), cli_world.get_block(cell), "{name} {cell}");
        }
        assert_eq!(client.inventory.get(0), None);
        assert_eq!(cli.inventory.get(0), None);
        if name != "ladder" {
            assert!(client_world.get_block(AT).unwrap().is_door());
            assert!(client_world.get_block(AT + IVec3::Y).unwrap().is_door_upper());
        }
    }
}

#[test]
fn replacements_plant_orientation_and_support_are_consistent() {
    for item in [
        Item::from_block(Block::DIRT),
        Item::from_block(ob::PALE_OAK_PLANKS),
        Item::from_block(forms::wood_id(10, 0)),
        Item::COCOA_BEANS,
    ] {
        let (mut client_world, mut client, _) = scene(item);
        let (mut cli_world, session, mut entities) = scene(item);
        if item == Item::COCOA_BEANS {
            for world in [&mut client_world, &mut cli_world] {
                world.set_block(WALL, Block::JUNGLE_LOG);
            }
        }
        let mut cli = Agent::new(session.player.pos);
        cli.session = session;
        let direct = client.place_held(0, &mut client_world, &[]);
        let indirect = cli.execute(Command::Place, &mut cli_world, &mut entities, &[]);
        assert!(direct.is_ok(), "{}: {direct:?}", item.name());
        assert!(indirect.is_ok(), "{}: {indirect:?}", item.name());
        assert_eq!(client_world.get_block(AT), cli_world.get_block(AT));
        assert_eq!(client.inventory.get(0), cli.inventory.get(0));
    }
    // Replacing vegetation uses the clicked cell rather than placing against it.
    let (mut world, mut session, _) = scene(Item::from_block(Block::DIRT));
    world.set_block(AT - IVec3::Y, Block::GRASS);
    world.set_block(AT, Block::TALL_GRASS);
    assert!(matches!(session.place_held(0,&mut world,&[]).unwrap(),Outcome::Placed{cell,..} if cell==AT));
    assert_eq!(world.get_block(AT), Some(Block::DIRT));
    assert_eq!(world.get_block(WALL), Some(Block::STONE));
}

#[test]
fn invalid_partner_cells_modes_stacks_and_other_players_do_not_spend_or_place() {
    for problem in 0..6 {
        let (mut world, mut session, _) = scene(Item::OAK_DOOR);
        let mut others = Vec::new();
        match problem {
            0 => {
                world.set_block(AT + IVec3::Y, Block::STONE);
            }
            1 => session.set_mode(GameMode::Adventure),
            2 => {
                session.inventory.slots[0].as_mut().unwrap().count = 0;
            }
            3 => others.push(AT.as_dvec3() + DVec3::new(0.5, 0., 0.5)),
            4 => {
                session.sleeping = Some(0.);
            }
            5 => {
                session.player.pos.x = f64::NAN;
            }
            _ => unreachable!(),
        }
        let inventory = session.inventory.serialize();
        let before = [world.get_block(AT), world.get_block(AT + IVec3::Y), world.get_block(WALL)];
        assert!(session.place_held(0, &mut world, &others).is_err(), "case {problem}");
        assert_eq!(session.inventory.serialize(), inventory, "case {problem}");
        assert_eq!([world.get_block(AT), world.get_block(AT + IVec3::Y), world.get_block(WALL)], before);
    }
    let (mut world, mut session, _) = scene(Item::OAK_DOOR);
    assert!(session.place_held(9, &mut world, &[]).is_err());
    assert_eq!(session.inventory.get(0), Some(Stack::new(Item::OAK_DOOR, 1)));
}

#[test]
fn vine_age_comes_from_shared_world_rng_and_failed_requests_do_not_advance_it() {
    let item = Item::from_block(nb::TWISTING_VINES);
    let (mut control, mut a, _) = scene(item);
    let (mut invalid, mut b, _) = scene(item);
    assert!(b.place_held(9, &mut invalid, &[]).is_err());
    // A late rejection: slot, stack, target and destination pass; support fails.
    invalid.set_block(AT - IVec3::Y, Block::AIR);
    assert_eq!(b.place_held(0, &mut invalid, &[]), Err("block cannot survive here"));
    assert_eq!(invalid.get_block(AT), Some(Block::AIR));
    assert_eq!(b.inventory.get(0), Some(Stack::new(item, 1)));
    invalid.set_block(AT - IVec3::Y, Block::STONE);
    a.place_held(0, &mut control, &[]).unwrap();
    b.place_held(0, &mut invalid, &[]).unwrap();
    let expected = control.get_block(AT).unwrap();
    assert!(matches!(nb::Vine::of(expected), Some((nb::Vine::Twisting, Some(0..=24)))));
    assert_eq!(invalid.get_block(AT), Some(expected));
}

#[test]
fn creative_preserves_the_stack_and_nether_water_does_not_place_or_spend() {
    let item = Item::from_block(Block::DIRT);
    let (mut world, mut session, _) = scene(item);
    session.set_mode(GameMode::Creative);
    session.place_held(0, &mut world, &[]).unwrap();
    assert_eq!(world.get_block(AT), Some(Block::DIRT));
    assert_eq!(session.inventory.get(0), Some(Stack::new(item, 1)));
    let water = Item::from_block(Block::WATER);
    let (mut world, mut session, _) = scene_in(water, Dimension::Nether);
    assert!(matches!(session.place_held(0,&mut world,&[]).unwrap(),Outcome::WaterEvaporated{cell,..} if cell==AT));
    assert_eq!(world.get_block(AT), Some(Block::AIR));
    assert_eq!(session.inventory.get(0), Some(Stack::new(water, 1)));
}

#[test]
fn colored_beds_place_both_halves_once_and_blocked_heads_are_atomic() {
    for color in voxelcraft::color::DyeColor::ALL {
        let item = color.bed();
        let setup = || {
            let (mut world, mut session, entities) = scene(item);
            world.set_block(WALL, Block::AIR);
            for x in [7, 8] {
                world.set_block(IVec3::new(x, 145, 4), Block::STONE);
            }
            session.player.pitch = -0.2;
            (world, session, entities)
        };
        let foot = IVec3::new(7, 146, 4);
        let head = foot + IVec3::X;
        let (mut world, mut session, _) = setup();
        let (mut cli_world, s, mut entities) = setup();
        let mut cli = Agent::new(s.player.pos);
        cli.session = s;
        assert!(
            matches!(session.place_held(0,&mut world,&[]).unwrap(),Outcome::Placed{cell,kind:Kind::Bed,..} if cell==foot)
        );
        cli.execute(Command::Place, &mut cli_world, &mut entities, &[]).unwrap();
        assert_eq!(world.get_block(foot), Some(Block::colored_bed(color, false)));
        assert_eq!(world.get_block(head), Some(Block::colored_bed(color, true)));
        assert_eq!(world.get_block(foot), cli_world.get_block(foot));
        assert_eq!(world.get_block(head), cli_world.get_block(head));
        assert_eq!(session.inventory.get(0), None);
        assert_eq!(cli.inventory.get(0), None);
        let (mut world, mut session, _) = setup();
        world.set_block(head, Block::STONE);
        let inventory = session.inventory.serialize();
        assert!(session.place_held(0, &mut world, &[]).is_err());
        assert_eq!(world.get_block(foot), Some(Block::AIR));
        assert_eq!(world.get_block(head), Some(Block::STONE));
        assert_eq!(session.inventory.serialize(), inventory);
    }
}

#[test]
fn looking_straight_down_does_not_reverse_bed_direction() {
    let color = voxelcraft::color::DyeColor::Blue;
    let (mut world, mut session, _) = scene(color.bed());
    world.set_block(WALL, Block::AIR);
    for x in [7, 8] {
        world.set_block(IVec3::new(x, 145, 4), Block::STONE);
    }
    session.player.pos = DVec3::new(7.5, 148., 4.5);
    session.player.pitch = -std::f32::consts::FRAC_PI_2;
    session.place_held(0, &mut world, &[]).unwrap();
    assert_eq!(world.get_block(IVec3::new(7, 146, 4)), Some(Block::colored_bed(color, false)));
    assert_eq!(world.get_block(IVec3::new(8, 146, 4)), Some(Block::colored_bed(color, true)));
}

#[test]
fn combining_slabs_and_blocked_double_plants_preserve_resource_accounting() {
    let slab = Block::slab_of(Block::STONE).unwrap();
    let setup = || {
        let (mut world, mut session, entities) = scene(Item::from_block(slab));
        world.set_block(WALL, Block::AIR);
        world.set_block(AT - IVec3::Y, Block::AIR);
        world.set_block(IVec3::new(7, 145, 4), slab);
        session.player.pitch = -0.4;
        (world, session, entities)
    };
    let (mut world, mut session, _) = setup();
    let (mut cli_world, s, mut entities) = setup();
    let mut cli = Agent::new(s.player.pos);
    cli.session = s;
    assert!(matches!(
        session.place_held(0, &mut world, &[]).unwrap(),
        Outcome::Placed { kind: Kind::CombinedSlab, .. }
    ));
    cli.execute(Command::Place, &mut cli_world, &mut entities, &[]).unwrap();
    assert_eq!(world.get_block(IVec3::new(7, 145, 4)), Some(Block::STONE));
    assert_eq!(world.get_block(IVec3::new(7, 145, 4)), cli_world.get_block(IVec3::new(7, 145, 4)));
    assert_eq!(session.inventory.get(0), None);
    assert_eq!(cli.inventory.get(0), None);
    let item = Item::from_name("sunflower").unwrap();
    let (mut world, mut session, _) = scene(item);
    world.set_block(AT - IVec3::Y, Block::GRASS);
    world.set_block(AT + IVec3::Y, Block::STONE);
    let inventory = session.inventory.serialize();
    assert!(session.place_held(0, &mut world, &[]).is_err());
    assert_eq!(world.get_block(AT), Some(Block::AIR));
    assert_eq!(world.get_block(AT + IVec3::Y), Some(Block::STONE));
    assert_eq!(session.inventory.serialize(), inventory);
}

/// Java `SnowLayerBlock`: one to eight layers of two sixteenths each, collision
/// one layer lower than the outline, `canBeReplaced` stacking from the top face,
/// loot of one snowball (or silk-touched layer) per layer, and a melt above
/// block light 11. Stages append at 2516..=2522.
#[test]
fn snow_layers_stack_to_eight_and_have_java_heights() {
    let layer = Item::from_block(ob::SNOW_LAYER);
    for n in 1..=8u8 {
        let b = ob::snow_layer(n);
        assert_eq!(ob::snow_layers(b), Some(n));
        assert_eq!((b.base(), b.name(), b.top_drop()), (ob::SNOW_LAYER, "snow layer", 16 - 2 * n));
        let collision = if n == 1 { 0.0 } else { f64::from(n - 1) / 8.0 };
        assert_eq!(b.height(), collision, "{n} layers");
        assert_eq!(b.drop(), None, "snowballs come from the layer count");
    }
    let (mut world, mut session, _) = scene(layer);
    world.set_block(WALL, Block::AIR);
    world.set_block(WALL - IVec3::Y, Block::STONE);
    session.player.pos = DVec3::new(7.5, 148., 4.5);
    session.player.pitch = -std::f32::consts::FRAC_PI_2;
    session.inventory.slots[0] = Some(Stack::new(Item::from_block(ob::SNOW_LAYER), 20));
    for n in 1..=8u8 {
        let outcome = session.place_held(0, &mut world, &[]).unwrap();
        assert!(matches!(outcome, Outcome::Placed { cell: WALL, .. }), "{outcome:?}");
        assert_eq!(world.get_block(WALL), Some(ob::snow_layer(n)));
    }
    assert_eq!(session.inventory.get(0).map(|s| s.count), Some(12));
    // A full eight-layer snow block is solid ground: the ninth layer starts a new block above.
    session.place_held(0, &mut world, &[]).unwrap();
    assert_eq!(world.get_block(WALL), Some(ob::snow_layer(8)));
    assert_eq!(world.get_block(WALL + IVec3::Y), Some(ob::SNOW_LAYER));
    // Clicking the side of a snow block places beside it instead of stacking.
    let (mut world, mut session, _) = scene(layer);
    world.set_block(WALL, ob::snow_layer(3));
    world.set_block(WALL - IVec3::Y, Block::STONE);
    session.player.pos = DVec3::new(4.5, 144.5, 4.5);
    session.player.yaw = 0.0;
    session.player.pitch = 0.0;
    world.set_block(AT - IVec3::Y, Block::STONE);
    session.place_held(0, &mut world, &[]).unwrap();
    assert_eq!(world.get_block(WALL), Some(ob::snow_layer(3)), "side clicks do not stack");
    assert_eq!(world.get_block(AT), Some(ob::SNOW_LAYER));
}

#[test]
fn snow_layers_drop_by_count_melt_in_light_and_keep_every_layer_with_silk_touch() {
    use voxelcraft::enchant::{Enchantment, Enchants};
    let (mut world, _, _) = scene(Item::SNOWBALL);
    for n in 1..=8u8 {
        world.drops.clear();
        world.spill_mined(AT, ob::snow_layer(n), Enchants::NONE);
        assert_eq!(
            world.drops.iter().map(|(_, s)| (s.item, s.count)).collect::<Vec<_>>(),
            [(Item::SNOWBALL, n)],
            "{n}"
        );
        world.drops.clear();
        let mut silk = Enchants::NONE;
        silk.set(Enchantment::SilkTouch, 1);
        world.spill_mined(AT, ob::snow_layer(n), silk);
        assert_eq!(
            world.drops.iter().map(|(_, s)| (s.item, s.count)).collect::<Vec<_>>(),
            [(Item::from_block(ob::SNOW_LAYER), n)],
            "{n}"
        );
    }
}

/// Java's barrel faces `getNearestLookingDirection().getOpposite()`, so looking
/// down opens it upward and looking up opens it downward (`BarrelBlock.FACING`).
#[test]
fn barrels_open_vertically_when_placed_looking_up_or_down() {
    use voxelcraft::world::village_blocks as vb;
    let barrel = Item::from_block(Block::BARREL);
    for (pitch, expected) in [
        (-std::f32::consts::FRAC_PI_2, Some(vb::BARREL_UP)),
        (std::f32::consts::FRAC_PI_2, Some(vb::BARREL_DOWN)),
        (-0.05, None),
    ] {
        let (mut world, mut session, _) = scene(barrel);
        world.set_block(WALL, Block::AIR);
        // Aim down at a floor from above, up at a ceiling from below, or level at a wall.
        if pitch < -1.0 {
            world.set_block(IVec3::new(7, 145, 4), Block::STONE);
            session.player.pos = DVec3::new(7.5, 148., 4.5);
        } else if pitch > 1.0 {
            world.set_block(IVec3::new(7, 148, 4), Block::STONE);
            session.player.pos = DVec3::new(7.5, 145., 4.5);
        } else {
            world.set_block(WALL, Block::STONE);
        }
        session.player.pitch = pitch;
        let Outcome::Placed { cell, block, .. } = session.place_held(0, &mut world, &[]).unwrap() else { panic!() };
        assert_eq!(world.get_block(cell), Some(block));
        match expected {
            Some(vertical) => {
                assert_eq!(block, vertical);
                assert_eq!((block.base(), block.name()), (Block::BARREL, "barrel"));
                assert_eq!(block.drop(), Some(barrel));
                assert!(voxelcraft::world::chest::is_chest(block), "vertical barrels still hold items");
                let tex = block.info().tex;
                let open = if vertical == vb::BARREL_UP { 2 } else { 3 };
                assert!(tex.iter().enumerate().all(|(face, &t)| (t == tex[open]) == (face == open)), "{tex:?}");
            }
            None => assert!(block.oriented().is_some(), "level looks keep the horizontal facing"),
        }
    }
}
