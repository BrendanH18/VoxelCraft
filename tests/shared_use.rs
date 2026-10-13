//! The shared right-click chain (`PlayerSession::use_held`) through the CLI adapter.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    rules::GameMode,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
    },
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

/// The block the player looks at, level with the eye, along +x.
const WALL: IVec3 = IVec3::new(7, 146, 4);

fn scene(item: Option<Item>) -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(IVec3::new(4, 144, 4), Block::STONE);
    world.set_block(WALL, Block::STONE);
    let mut session = PlayerSession::new(PlayerId(6), DVec3::new(4.5, 145., 4.5));
    session.player.on_ground = true;
    session.inventory.slots[0] = item.map(|i| Stack::new(i, 1));
    (world, session, Entities::new(7))
}

fn use_held(s: &mut PlayerSession, w: &mut World, e: &mut Entities, sneaking: bool) -> Option<Used> {
    s.use_held(0, w, e, &UseContext { sneaking, others: &[] }).unwrap().map(|u| u.used)
}

#[test]
fn containers_report_screens_and_sneaking_builds_against_them() {
    let (mut w, mut s, mut e) = scene(Some(Item::from_block(Block::STONE)));
    w.set_block(WALL, Block::CRAFTING_TABLE);
    assert_eq!(
        use_held(&mut s, &mut w, &mut e, false),
        Some(Used::Screen { cell: WALL, block: Block::CRAFTING_TABLE })
    );
    assert_eq!(s.inventory.get(0).map(|s| s.count), Some(1), "opening a screen spends nothing");
    assert!(matches!(use_held(&mut s, &mut w, &mut e, true), Some(Used::Placed(_))));
    assert_eq!(w.get_block(WALL - IVec3::X), Some(Block::STONE));
}

#[test]
fn doors_toggle_for_every_adapter_including_the_cli() {
    let door = Item::from_name("oak_door").unwrap();
    let (mut w, mut s, mut e) = scene(Some(door));
    // Stand the door on the floor in front of the player, then use it bare-handed.
    w.set_block(WALL, Block::AIR);
    for x in 4..9 {
        w.set_block(IVec3::new(x, 144, 4), Block::STONE);
    }
    s.player.pitch = -0.68;
    assert!(matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::Placed(_))));
    let placed = (5..8).map(|x| IVec3::new(x, 145, 4)).find(|c| w.get_block(*c).is_some_and(Block::is_door)).unwrap();
    s.player.pitch = 0.0;
    s.inventory.slots[0] = None;
    let closed = w.get_block(placed).unwrap();
    // The CLI opens it through the shared rule...
    let mut cli = Agent::new(s.player.pos);
    cli.session.player.yaw = s.player.yaw;
    cli.session.player.pitch = 0.0;
    cli.execute(Command::Place, &mut w, &mut e, &[]).unwrap();
    assert_ne!(w.get_block(placed), Some(closed));
    // ...and the session closes it again, aiming across the swung-open panel.
    let open = w.get_block(placed).unwrap();
    let base = s.player.yaw;
    let toggled = (0..25).find_map(|k| {
        s.player.yaw = base + (k as f32 - 12.0) * 0.05;
        use_held(&mut s, &mut w, &mut e, false).filter(|u| matches!(u, Used::Door { .. }))
    });
    assert_eq!(toggled.map(|u| matches!(u, Used::Door { open: Some(false), .. })), Some(true), "{open:?}");
    assert_eq!(w.get_block(placed), Some(closed));
}

#[test]
fn flint_and_steel_ignites_and_primes_tnt_spending_durability_in_survival() {
    let flint = Some(Item::FLINT_AND_STEEL);
    let (mut w, mut s, mut e) = scene(flint);
    s.player.pitch = -1.5;
    let down = s.block_target(&w).unwrap();
    assert_eq!(down, (IVec3::new(4, 144, 4), IVec3::Y));
    let lit = use_held(&mut s, &mut w, &mut e, false);
    assert_eq!(lit, Some(Used::Ignited { cell: IVec3::new(4, 145, 4), portal: false }));
    assert!(w.get_block(IVec3::new(4, 145, 4)).is_some_and(Block::is_fire));
    assert_eq!(s.inventory.get(0).unwrap().damage, 1);

    let (mut w, mut s, mut e) = scene(flint);
    w.set_block(WALL, Block::TNT);
    s.set_mode(GameMode::Creative);
    assert_eq!(use_held(&mut s, &mut w, &mut e, false), Some(Used::TntPrimed { cell: WALL }));
    assert_eq!(w.get_block(WALL), Some(Block::AIR));
    assert_eq!(s.inventory.get(0).unwrap().damage, 0, "Creative does not wear the tool");
}

#[test]
fn hoes_till_and_shovels_make_paths_under_open_air_only() {
    for (tool, made, used) in [
        (ToolKind::Hoe, Block::FARMLAND, Used::Tilled { cell: IVec3::new(4, 144, 4) }),
        (ToolKind::Shovel, Block::DIRT_PATH, Used::Path { cell: IVec3::new(4, 144, 4) }),
    ] {
        let (mut w, mut s, mut e) = scene(Some(Item::tool(tool, Tier::Iron)));
        w.set_block(IVec3::new(4, 144, 4), Block::GRASS);
        s.player.pos.x = 4.5;
        s.player.pitch = -1.5;
        assert_eq!(use_held(&mut s, &mut w, &mut e, false), Some(used));
        assert_eq!(w.get_block(IVec3::new(4, 144, 4)), Some(made));
        assert_eq!(s.inventory.get(0).unwrap().damage, 1);
    }
}

#[test]
fn the_cli_no_longer_places_against_screens_and_reports_why() {
    let (mut w, s, mut e) = scene(Some(Item::from_block(Block::STONE)));
    w.set_block(WALL, Block::CRAFTING_TABLE);
    let mut cli = Agent::new(s.player.pos);
    cli.session.inventory = s.inventory.clone();
    let err = cli.execute(Command::Place, &mut w, &mut e, &[]).unwrap_err();
    assert!(err.contains("screen"), "{err}");
    assert_eq!(w.get_block(WALL - IVec3::X), Some(Block::AIR));
}

#[test]
fn bows_draw_in_ticks_shoot_on_release_and_let_go_when_the_slot_changes() {
    use voxelcraft::session::ranged::{FULL_DRAW_TICKS, Ranged};
    let (w, mut s, mut e) = scene(Some(Item::BOW));
    s.inventory.slots[1] = Some(Stack::new(Item::ARROW, 2));
    s.inventory.slots[2] = Some(Stack::new(Item::BOW, 1));
    assert!(matches!(s.start_draw(0, &w, &mut e), Some(shot) if shot.ranged == Ranged::Drawing));
    for _ in 0..FULL_DRAW_TICKS {
        assert_eq!(s.tick_draw(0, true), None);
    }
    assert_eq!(s.draw_power(0), Some(1.0));
    let shot = s.release_draw(0, &mut e).unwrap();
    assert!(matches!(shot.ranged, Ranged::ArrowShot { power, .. } if power == 1.0));
    assert_eq!(s.inventory.get(1).unwrap().count, 1, "one arrow spent");
    assert_eq!(s.inventory.get(0).unwrap().damage, 1, "the bow wears");
    assert_eq!(e.arrows.len(), 1);

    // Selecting another bow lets the string go without shooting.
    s.start_draw(0, &w, &mut e).unwrap();
    for _ in 0..FULL_DRAW_TICKS {
        s.tick_draw(0, true);
    }
    assert_eq!(s.tick_draw(2, true), None);
    assert!(!s.drawing());
    assert_eq!(s.release_draw(2, &mut e), None);
    assert_eq!(e.arrows.len(), 1);
}

#[test]
fn crossbows_load_after_twenty_five_ticks_and_fire_on_the_next_use() {
    use voxelcraft::session::ranged::{CROSSBOW_CHARGE_TICKS, Ranged};
    let (w, mut s, mut e) = scene(Some(Item::CROSSBOW));
    s.inventory.slots[1] = Some(Stack::new(Item::ARROW, 1));
    s.start_draw(0, &w, &mut e).unwrap();
    for _ in 0..CROSSBOW_CHARGE_TICKS - 1 {
        assert_eq!(s.tick_draw(0, true), None);
    }
    let loaded = s.tick_draw(0, true).unwrap();
    assert!(matches!(loaded.ranged, Ranged::CrossbowLoaded { .. }));
    assert_eq!(s.inventory.get(0).unwrap().item, Item::CHARGED_CROSSBOW);
    assert_eq!(s.inventory.get(1), None, "loading spends the arrow");
    let fired = s.start_draw(0, &w, &mut e).unwrap();
    assert!(matches!(fired.ranged, Ranged::CrossbowFired { .. }));
    assert_eq!(s.inventory.get(0).unwrap().item, Item::CROSSBOW);
    assert_eq!(e.arrows.len(), 1);
}
