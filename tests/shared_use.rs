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

/// Solid neighbors on every side of `WALL` but the player's, so a mounted block stays attached.
fn support_all_but_the_viewer_side(w: &mut World) {
    for d in [IVec3::X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
        w.set_block(WALL + d, Block::STONE);
    }
}

#[test]
fn adventure_uses_redstone_and_composters_without_build_permission() {
    use voxelcraft::world::{composter, redstone_blocks as r};
    let mount = (1..7).find(|m| r::support(*m) == IVec3::X).unwrap();
    for block in [r::lever(mount, false), r::button(mount, false, false), composter::state(8)] {
        let (mut w, mut s, mut e) = scene(None);
        support_all_but_the_viewer_side(&mut w);
        w.set_block(WALL, block);
        s.set_mode(GameMode::Adventure);
        assert!(
            matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::Redstone { .. } | Used::Composter { .. })),
            "{block:?}"
        );
    }
}

#[test]
fn blocks_claim_throwables_and_bow_use_but_full_cake_passes_to_items() {
    use voxelcraft::world::redstone_blocks as r;
    let (mut w, mut s, mut e) = scene(Some(Item::SNOWBALL));
    support_all_but_the_viewer_side(&mut w);
    let mount = (1..7).find(|m| r::support(*m) == IVec3::X).unwrap();
    w.set_block(WALL, r::lever(mount, false));
    assert!(matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::Redstone { .. })));
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    s.inventory.slots[0] = Some(Stack::new(Item::BOW, 1));
    s.inventory.slots[1] = Some(Stack::new(Item::ARROW, 1));
    assert!(s.start_draw(0, &w, &mut e).is_none());
    s.player.sneaking = true;
    assert!(s.start_draw(0, &w, &mut e).is_some());
    s.cancel_draw();
    s.player.sneaking = false;
    w.set_block(WALL - IVec3::Y, Block::STONE);
    w.set_block(WALL, Block::cake(0));
    s.player.pos.y = 144.5;
    s.inventory.slots[0] = Some(Stack::new(Item::SNOWBALL, 2));
    s.vitals.hunger.food = 18.;
    assert!(matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::CakeBitten { .. })));
    assert_eq!(s.inventory.get(0).unwrap().count, 2);
    assert!(matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::ProjectileThrown { .. })));
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    assert_eq!(w.get_block(WALL), Some(Block::cake(1)));
}

/// Java `BellBlock.isProperHit`: no ring from the top or bottom face, nor from
/// the beam above 0.8124; the click then passes on to the held item.
#[test]
fn bells_ring_only_for_proper_hits_and_other_clicks_pass_to_the_held_item() {
    let (mut w, mut s, mut e) = scene(Some(Item::from_block(Block::STONE)));
    w.set_block(WALL, Block::BELL);
    // Level with the bell body (eye 0.62 into the cell).
    assert_eq!(use_held(&mut s, &mut w, &mut e, false), Some(Used::Redstone { cell: WALL }));
    // Eye at 0.92 of the cell: the beam, so the stone is placed instead.
    s.player.pos.y += 0.3;
    assert!(matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::Placed(_))));
    assert_eq!(w.get_block(WALL), Some(Block::BELL));
    // Looking down onto the top face.
    let (mut w, mut s, mut e) = scene(Some(Item::from_block(Block::STONE)));
    w.set_block(WALL, Block::BELL);
    s.player.pos = DVec3::new(7.5, 148., 4.5);
    s.player.pitch = -std::f32::consts::FRAC_PI_2 + 0.01;
    s.player.on_ground = true;
    assert!(matches!(use_held(&mut s, &mut w, &mut e, false), Some(Used::Placed(_))));
    assert_eq!(w.get_block(WALL), Some(Block::BELL));
}

/// Entity clicks use the entity interaction range (3, or 5 in Creative), not the
/// block range, and a nearer block still hides the animal.
#[test]
fn animal_clicks_use_the_entity_interaction_range() {
    use voxelcraft::entity::MobKind;
    for (mode, x, fed) in [
        (GameMode::Survival, 7.5, true),
        (GameMode::Survival, 9.5, false),
        (GameMode::Creative, 9.5, true),
        (GameMode::Creative, 11.5, false),
    ] {
        let (mut w, mut s, mut e) = scene(Some(Item::WHEAT));
        w.set_block(WALL, Block::AIR);
        s.set_mode(mode);
        e.spawn(MobKind::Cow, DVec3::new(x, 145.5, 4.5));
        let used =
            s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).ok().flatten().map(|u| u.used);
        assert_eq!(used == Some(Used::Animal), fed, "{mode:?} at {x}: {used:?}");
    }
    // A wall closer than the cow stops the click.
    let (mut w, mut s, mut e) = scene(Some(Item::WHEAT));
    e.spawn(MobKind::Cow, DVec3::new(7.5, 145.5, 4.5));
    w.set_block(WALL - IVec3::X * 2, Block::STONE);
    w.set_block(WALL - IVec3::X * 2 - IVec3::Y, Block::STONE);
    let used =
        s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).ok().flatten().map(|u| u.used);
    assert_ne!(used, Some(Used::Animal));
}
