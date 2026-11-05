//! The ordinary copper-golem/statue survival loop through shared player actions.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    entity::{Entities, PlayerId},
    inventory::{Stack, StackName},
    item::{Item, Tier, ToolKind},
    player::EYE_HEIGHT,
    rules::{GameMode, GameRules},
    session::{
        PlayerSession,
        interact::{UseContext, Used},
        mining::Outcome,
    },
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        copper::Stage,
        copper_golem_statue::{self as statue, Pose, Statue},
        storage::{LevelInfo, Storage},
        terrain::Generator,
    },
};

const P: IVec3 = IVec3::new(10, 140, 10);
fn aim(s: &mut PlayerSession, point: DVec3) {
    let eye = DVec3::new(10.5, 141.0, 13.0);
    let direction = (point - eye).normalize();
    s.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    s.player.yaw = direction.z.atan2(direction.x) as f32;
    s.player.pitch = direction.y.asin() as f32;
}
fn scene(block: Block) -> (World, PlayerSession, Entities) {
    let mut w = World::new_remote(Arc::new(Generator::new(263)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w.set_block(P - IVec3::Y, Block::STONE);
    w.set_block(P, block);
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
    aim(&mut s, P.as_dvec3() + DVec3::new(0.5, 0.5, 0.5));
    (w, s, Entities::new(263))
}

#[test]
fn statue_placement_pick_and_mining_keep_name_pose_and_water() {
    let input = Statue { pose: Pose::Star, ..Statue::new(Stage::Weathered, true) }
        .item_stack(StackName::new("Copper friend | ♥").unwrap());
    let (mut w, mut s, mut entities) = scene(Block::WATER);
    s.inventory.slots[0] = Some(input);
    aim(&mut s, P.as_dvec3() + DVec3::new(0.5, -0.01, 0.5));
    s.place_held(0, &mut w, &[]).unwrap();
    let placed = statue::decode(w.get_block(P).unwrap()).unwrap();
    assert_eq!(placed.pose, Pose::Star);
    assert_eq!(placed.facing, Facing::South);
    assert!(placed.waxed && placed.waterlogged);
    assert_eq!(placed.stage, Stage::Weathered);
    assert_eq!(w.copper_golem_statue_item(P), Some(input));
    assert!(s.inventory.get(0).is_none());

    aim(&mut s, P.as_dvec3() + DVec3::splat(0.5));
    s.mode = GameMode::Creative;
    s.pick_block(0, &w).unwrap();
    assert_eq!(s.inventory.get(0), Some(input));
    assert_eq!(s.pick_hotbar_slot(&w), Some(0));
    s.mode = GameMode::Survival;
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Diamond), 1));
    for _ in 0..200 {
        if matches!(s.tick_mining(0, &mut w, &mut entities, &GameRules::default()).unwrap(), Outcome::Broken { .. }) {
            break;
        }
    }
    assert_eq!(w.get_block(P), Some(Block::WATER));
    assert_eq!(w.drops, vec![(P, input)]);
    assert_eq!(w.copper_golem_statue_name(P), StackName::default());
}

#[test]
fn named_statue_saves_and_keeps_components_across_every_state_change() {
    let (mut w, _, _) = scene(statue::item_block(Stage::Unaffected, false));
    let name = StackName::new("A name with = | , and ♥").unwrap();
    w.name_copper_golem_statue(P, name);
    for id in statue::FIRST..statue::END {
        w.set_block(P, Block(id));
        assert_eq!(w.copper_golem_statue_name(P), name);
    }
    let expected = w.copper_golem_statue_item(P).unwrap();
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("copper-statue-save-{}", std::process::id()));
    let storage = Storage::new(&directory);
    let props = [("copper_golem_statues".into(), w.copper_golem_statues_to_string())].into_iter().collect();
    storage.save(&LevelInfo { seed: 263, player: None, props }, &w.modified_chunks()).unwrap();
    let mut loaded = World::new_remote(Arc::new(Generator::new(263)), 2);
    // Names can arrive before their chunks, both on disk load and over LAN.
    loaded.load_copper_golem_statues(&storage.load_level().unwrap().props["copper_golem_statues"]);
    for (pos, data) in storage.load_chunks().unwrap() {
        loaded.receive_chunk(pos, data);
    }
    assert_eq!(loaded.copper_golem_statue_item(P), Some(expected));
    loaded.detonate(&[P], 1.0, true, false, false);
    assert_eq!(loaded.drops, vec![(P, expected)]);
    assert_eq!(loaded.copper_golem_statue_name(P), StackName::default());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn statue_name_snapshot_and_no_drop_removal_do_not_leave_stale_names() {
    let b = statue::item_block(Stage::Oxidized, true);
    let (mut w, mut s, mut entities) = scene(b);
    let name = StackName::new("Snapshot name").unwrap();
    w.name_copper_golem_statue(P, name);
    let (mut remote, _, _) = scene(b);
    remote.replace_copper_golem_statues(&w.copper_golem_statues_near(P.as_dvec3()));
    assert_eq!(remote.copper_golem_statue_name(P), name);
    s.mode = GameMode::Creative;
    s.tick_mining(0, &mut w, &mut entities, &GameRules::default()).unwrap();
    assert!(w.drops.is_empty());
    assert_eq!(w.copper_golem_statue_name(P), StackName::default());
    w.set_block(P, b);
    remote.replace_copper_golem_statues(&w.copper_golem_statues_near(P.as_dvec3()));
    assert_eq!(remote.copper_golem_statue_name(P), StackName::default());
}

#[test]
fn third_person_held_statues_draw_their_pose_and_high_texture_layers() {
    use voxelcraft::entity::model::{PlayerAppearance, build_player};
    let p = voxelcraft::player::Player::new(DVec3::ZERO);
    let mut positions = Vec::new();
    for pose in Pose::ALL {
        let block = statue::block(Statue { pose, ..Statue::new(Stage::Oxidized, true) });
        let look = PlayerAppearance {
            held: Some(Item::from_block(block.base())),
            held_statue: Some(block),
            alpha: 1.0,
            ..Default::default()
        };
        let mut vertices = Vec::new();
        build_player(&p, DVec3::ZERO, DVec3::ZERO, (1.0, 0.0), 0.0, look, &mut vertices);
        let held: Vec<_> = vertices.iter().filter(|v| v.torch[1] == 2).collect();
        assert_eq!(held.len(), 8 * 36, "statue must use the eight-part model, not one cube");
        for v in &held {
            let layer = v.torch[2] as u16 | ((v.torch[3] as u16) << 8);
            assert!((statue::TEXTURE_FIRST + 9..statue::TEXTURE_END).contains(&layer));
        }
        positions.push(held.iter().map(|v| v.pos).collect::<Vec<_>>());
    }
    for (i, pose) in positions.iter().enumerate() {
        for other in &positions[i + 1..] {
            assert_ne!(pose, other, "each pose must remain distinct in the hand");
        }
    }
}

#[test]
fn honeycomb_and_axe_change_only_wax_and_oxidation_and_charge_the_held_stack() {
    let before =
        Statue { pose: Pose::Running, facing: Facing::West, waterlogged: true, ..Statue::new(Stage::Weathered, false) };
    let (mut w, mut s, mut entities) = scene(statue::block(before));
    let name = StackName::new("Keep my name").unwrap();
    w.name_copper_golem_statue(P, name);
    let ctx = UseContext { sneaking: false, others: &[] };
    s.inventory.slots[0] = Some(Stack::new(Item::HONEYCOMB, 2));
    let used = s.use_held(0, &mut w, &mut entities, &ctx).unwrap().unwrap();
    assert_eq!(used.used, Used::Waxed { cell: P });
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    assert_eq!(statue::decode(w.get_block(P).unwrap()), Some(Statue { waxed: true, ..before }));
    let axe = Item::tool(ToolKind::Axe, Tier::Iron);
    s.inventory.slots[0] = Some(Stack::new(axe, 1));
    let used = s.use_held(0, &mut w, &mut entities, &ctx).unwrap().unwrap();
    assert_eq!(used.used, Used::Scraped { cell: P });
    assert_eq!(statue::decode(w.get_block(P).unwrap()), Some(before));
    assert_eq!(s.inventory.get(0).unwrap().damage, 1);
    s.use_held(0, &mut w, &mut entities, &ctx).unwrap().unwrap();
    assert_eq!(statue::decode(w.get_block(P).unwrap()), Some(Statue { stage: Stage::Exposed, ..before }));
    assert_eq!(s.inventory.get(0).unwrap().damage, 2);
    assert_eq!(w.copper_golem_statue_name(P), name);
}

#[test]
fn placing_the_last_pumpkin_builds_a_golem_and_replaces_copper_with_a_chest() {
    use voxelcraft::{
        entity::golem::finish_golems,
        world::{copper, pumpkin_blocks},
    };
    let (mut w, mut s, mut e) = scene(Block::AIR);
    let source = copper::Copper { family: copper::Family::Block, stage: Stage::Weathered, local: 0, waxed: true };
    w.set_block(P - IVec3::Y, copper::encode(source));
    s.inventory.slots[0] = Some(Stack::new(pumpkin_blocks::CARVED, 1));
    aim(&mut s, P.as_dvec3() + DVec3::new(0.5, -0.01, 0.5));
    s.place_held(0, &mut w, &[]).unwrap();
    finish_golems(&mut w, &mut e);
    assert!(s.inventory.get(0).is_none());
    assert_eq!(w.get_block(P), Some(Block::AIR));
    let chest = copper::state(w.get_block(P - IVec3::Y).unwrap()).unwrap();
    assert_eq!((chest.family, chest.stage, chest.waxed), (copper::Family::Chest, Stage::Weathered, true));
    assert_eq!(e.mobs.len(), 1);
    let m = &e.mobs[0];
    let state = m.copper_golem.as_ref().unwrap();
    assert_eq!(state.stage, Stage::Weathered);
    assert!(!state.waxed());
    assert!(m.persistent && m.built && m.uid != 0);
    assert_eq!(m.pos, P.as_dvec3() + DVec3::new(0.5, 0.05, 0.5));
    assert!(w.drops.is_empty());
}

#[test]
fn both_heads_construct_all_copper_variants_in_each_two_cell_orientation() {
    use voxelcraft::{
        entity::golem::finish_golems,
        world::{copper, double_chest, pumpkin_blocks},
    };
    for stage in Stage::ALL {
        for waxed in [false, true] {
            for head in [pumpkin_blocks::CARVED, pumpkin_blocks::JACK_O_LANTERN] {
                for offset in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                    let (mut w, _, mut e) = scene(Block::AIR);
                    let at = P + offset;
                    w.set_block(
                        at,
                        copper::encode(copper::Copper { family: copper::Family::Block, stage, local: 0, waxed }),
                    );
                    w.set_block(P, head.with_facing(Facing::East));
                    finish_golems(&mut w, &mut e);
                    assert_eq!(e.mobs.len(), 1, "{stage:?} {waxed} {offset}");
                    let chest = double_chest::state(w.get_block(at).unwrap()).unwrap();
                    assert_eq!(chest.kind.copper(), Some((stage, waxed)));
                    assert_eq!(chest.facing, Facing::East);
                }
            }
        }
    }
}

#[test]
fn empty_hand_returns_full_cargo_and_poppy_shears_wax_and_axe_use_the_same_entity() {
    let (mut w, mut s, mut e) = scene(Block::AIR);
    let pos = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    e.spawn_copper_golem(pos, Stage::Weathered, false);
    let mut cargo = Statue { pose: Pose::Star, ..Statue::new(Stage::Exposed, true) }
        .item_stack(StackName::new("Return this whole stack").unwrap());
    cargo.count = 7;
    e.mobs[0].copper_golem.as_mut().unwrap().cargo = Some(cargo);
    let ctx = UseContext { sneaking: false, others: &[] };
    assert_eq!(s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap().used, Used::Mob);
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().cargo.is_none());
    assert_eq!(e.items[0].stack, cargo);
    assert!((e.items[0].pos.y - (pos.y + 0.8125 - f64::from(0.3f32))).abs() < 1e-9);
    assert!((e.items[0].vel - (s.player.pos - pos).normalize() * (f64::from(0.3f32) * 20.0)).length() < 1e-9);
    assert_eq!(e.items[0].pickup_delay, 0.5);
    e.mobs[0].copper_golem.as_mut().unwrap().interaction_ticks = 12;
    assert!(s.use_held(0, &mut w, &mut e, &ctx).unwrap().is_none());
    assert_eq!(
        e.mobs[0].copper_golem.as_ref().unwrap().interaction_ticks,
        12,
        "an empty-hand click on an empty golem passes without interrupting its inspection"
    );

    let mut poppy = Stack::new(Block::POPPY, 2);
    poppy.name = StackName::new("A flower").unwrap();
    s.inventory.slots[0] = Some(poppy);
    s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap();
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    assert_eq!(e.mobs[0].copper_golem.as_ref().unwrap().antenna, Some(Stack { count: 1, ..poppy }));
    s.inventory.slots[0] = Some(Stack::new(Item::SHEARS, 1));
    s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap();
    assert_eq!(s.inventory.get(0).unwrap().damage, 1);
    assert_eq!(e.items[1].stack, Stack { count: 1, ..poppy });
    assert_eq!(e.items[1].pos, pos + DVec3::Y * 1.5);
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().antenna.is_none());

    s.inventory.slots[0] = Some(Stack::new(Item::HONEYCOMB, 2));
    assert_eq!(s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap().used, Used::Waxed { cell: P });
    assert_eq!(s.inventory.get(0).unwrap().count, 1);
    assert!(e.mobs[0].copper_golem.as_ref().unwrap().waxed());
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1));
    for stage in [Stage::Weathered, Stage::Exposed, Stage::Unaffected] {
        assert_eq!(s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap().used, Used::Scraped { cell: P });
        let state = e.mobs[0].copper_golem.as_ref().unwrap();
        assert_eq!(state.stage, stage);
        assert!(!state.waxed());
    }
    assert_eq!(s.inventory.get(0).unwrap().damage, 3);
    assert_eq!(w.particles.drain().count(), 4, "waxing and each scraping action have visible feedback");
}

#[test]
fn named_golem_converts_drops_equipment_saves_and_revives_as_a_fresh_golem() {
    let (mut w, mut s, mut e) = scene(Block::AIR);
    let pos = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let name = StackName::new("Sorter ♥").unwrap();
    let cargo = Statue { pose: Pose::Running, ..Statue::new(Stage::Weathered, true) }.item_stack(name);
    e.spawn_copper_golem(pos, Stage::Oxidized, false);
    let m = &mut e.mobs[0];
    m.name = name;
    m.yaw = std::f32::consts::PI;
    m.health = 3.0;
    let state = m.copper_golem.as_mut().unwrap();
    state.statue_ready = true;
    state.cargo = Some(cargo);
    state.antenna = Some(Stack::new(Block::POPPY, 1));
    e.entity_drops = false;
    e.finish_copper_statues(&mut w);
    assert!(e.mobs.is_empty());
    assert_eq!(e.items.iter().map(|i| i.stack).collect::<Vec<_>>(), vec![cargo, Stack::new(Block::POPPY, 1)]);
    let statue = statue::decode(w.get_block(P).unwrap()).unwrap();
    assert_eq!((statue.stage, statue.facing, statue.waxed), (Stage::Oxidized, Facing::West, false));
    assert_eq!(w.copper_golem_statue_name(P), name);
    let (mut loaded, _, _) = scene(statue::block(statue));
    loaded.load_copper_golem_statues(&w.copper_golem_statues_to_string());
    e.load_nether_mobs(&e.nether_mobs_to_string());
    let ctx = UseContext { sneaking: false, others: &[] };
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1));
    for stage in [Stage::Weathered, Stage::Exposed, Stage::Unaffected] {
        s.use_held(0, &mut loaded, &mut e, &ctx).unwrap().unwrap();
        assert_eq!(statue::decode(loaded.get_block(P).unwrap()).unwrap().stage, stage);
    }
    assert_eq!(s.use_held(0, &mut loaded, &mut e, &ctx).unwrap().unwrap().used, Used::Statue { cell: P });
    assert_eq!(s.inventory.get(0).unwrap().damage, 4);
    assert_eq!(loaded.get_block(P), Some(Block::AIR));
    assert_eq!(loaded.copper_golem_statue_name(P), StackName::default());
    assert_eq!(e.mobs.len(), 1);
    let m = &e.mobs[0];
    assert_eq!((m.name, m.health, m.pos), (name, 12.0, pos));
    assert!((m.yaw - std::f32::consts::PI).abs() < 1e-6);
    assert_eq!(m.copper_golem.as_ref().unwrap().stage, Stage::Unaffected);
    assert!(m.copper_golem.as_ref().unwrap().cargo.is_none());
    assert!(m.copper_golem.as_ref().unwrap().antenna.is_none());
}

#[test]
fn statue_click_priority_handles_wax_sneak_offhand_and_waterlogged_reactivation() {
    let original = Statue { waterlogged: true, ..Statue::new(Stage::Unaffected, true) };
    let (mut w, mut s, mut e) = scene(statue::block(original));
    let ctx = UseContext { sneaking: false, others: &[] };
    s.inventory.slots[0] = Some(Stack::new(Item::HONEYCOMB, 2));
    s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap();
    assert_eq!(statue::decode(w.get_block(P).unwrap()).unwrap().pose, Pose::Sitting);
    assert_eq!(s.inventory.get(0).unwrap().count, 2);
    s.inventory.slots[0] = None;
    s.inventory.offhand = Some(Stack::new(Item::STICK, 1));
    let sneak = UseContext { sneaking: true, others: &[] };
    // A passed action reaches the existing placement adapter, which can report
    // an empty/non-placeable held slot. Neither hand may cycle this statue.
    assert!(!matches!(s.use_held(0, &mut w, &mut e, &sneak), Ok(Some(_))));
    assert_eq!(statue::decode(w.get_block(P).unwrap()).unwrap().pose, Pose::Sitting);
    s.inventory.offhand = None;
    s.use_held(0, &mut w, &mut e, &sneak).unwrap().unwrap();
    assert_eq!(statue::decode(w.get_block(P).unwrap()).unwrap().pose, Pose::Running);
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1));
    s.use_held(0, &mut w, &mut e, &sneak).unwrap().unwrap();
    assert!(!statue::decode(w.get_block(P).unwrap()).unwrap().waxed);
    assert!(!matches!(s.use_held(0, &mut w, &mut e, &sneak), Ok(Some(_))));
    assert!(e.mobs.is_empty(), "sneaking with an axe bypasses reactivation");
    s.use_held(0, &mut w, &mut e, &ctx).unwrap().unwrap();
    assert_eq!(w.get_block(P), Some(Block::WATER));
    assert_eq!(e.mobs.len(), 1);
    assert_eq!(s.inventory.get(0).unwrap().damage, 2);
}

#[test]
fn a_golem_carries_each_statue_pose_as_original_geometry() {
    let (_, _, mut e) = scene(Block::AIR);
    e.spawn_copper_golem(DVec3::ZERO, Stage::Unaffected, false);
    let mut poses = Vec::new();
    for pose in Pose::ALL {
        e.mobs[0].copper_golem.as_mut().unwrap().cargo =
            Some(Statue { pose, ..Statue::new(Stage::Weathered, false) }.item_stack(StackName::default()));
        let mut verts = Vec::new();
        voxelcraft::entity::model::build(&e.mobs, DVec3::ZERO, glam::Vec3::Z, 100.0, 0.0, 1.0, &mut verts);
        let cargo: Vec<_> = verts.iter().filter(|v| v.torch[1] == 2).collect();
        assert_eq!(cargo.len(), 8 * 36);
        poses.push(cargo.into_iter().map(|v| v.pos).collect::<Vec<_>>());
    }
    for (i, pose) in poses.iter().enumerate() {
        assert!(poses[i + 1..].iter().all(|other| other != pose));
    }
}

#[test]
fn weathering_uses_saved_simulation_age_even_when_day_time_is_changed_or_frozen() {
    let (mut w, _, mut e) = scene(Block::AIR);
    e.spawn_copper_golem(P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), Stage::Unaffected, false);
    e.mobs[0].copper_golem.as_mut().unwrap().next_weather_age = 600_000;
    w.maps.game_ticks = 599_999;
    e.clock_time = i64::MAX;
    e.tick_copper_golems(0.05, &mut w);
    assert_eq!(e.mobs[0].copper_golem.as_ref().unwrap().stage, Stage::Unaffected);
    let saved_age = w.maps.serialize();
    w.maps = voxelcraft::world::maps::MapStorage::load(&saved_age).unwrap();
    e.clock_time = 0;
    w.maps.game_ticks += 1;
    e.tick_copper_golems(0.05, &mut w);
    assert_eq!(e.mobs[0].copper_golem.as_ref().unwrap().stage, Stage::Exposed);
    assert!((1_104_000..=1_152_000).contains(&e.mobs[0].copper_golem.as_ref().unwrap().next_weather_age));
}
