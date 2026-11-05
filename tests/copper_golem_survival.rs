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
