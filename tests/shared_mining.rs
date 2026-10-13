//! Shared execution through client-entry rules and the real CLI adapter.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    enchant::{Enchantment, Enchants},
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::{Item, Tier, ToolKind},
    player::MoveInput,
    rules::{GameMode, GameRules},
    session::{PlayerSession, mining::Outcome},
    world::{
        World,
        block::{Block, Facing},
        chunk::ChunkData,
        terrain::{Dimension, Generator},
    },
};
const AT: IVec3 = IVec3::new(7, 146, 4);

#[test]
fn reference_block_reach_bounds_the_shared_action_and_cli_target() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/java/26.3/mining.json")).unwrap();
    let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
    w.set_block(AT, Block::AIR);
    let far = IVec3::new(9, 146, 4);
    w.set_block(far, Block::STONE);
    s.player.pos.x = 4.4;
    assert_eq!(s.block_reach(), fixture["block_reach"]["survival"].as_f64().unwrap());
    let mut cli = Agent::new(s.player.pos);
    assert!(cli.target(&w).is_none());
    assert!(s.tick_mining(0, &mut w, &mut e, &Default::default()).is_err());
    assert_eq!(w.get_block(far), Some(Block::STONE));
    s.set_mode(GameMode::Creative);
    cli.set_mode(GameMode::Creative);
    assert_eq!(s.block_reach(), fixture["block_reach"]["creative"].as_f64().unwrap());
    assert_eq!(cli.target(&w).map(|(p, _)| p), Some(far));
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &Default::default()).unwrap(), Outcome::Broken { .. }));
}

#[test]
fn reference_float_operation_order_and_all_fatigue_amplifiers_match() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/java/26.3/mining.json")).unwrap();
    for case in fixture["progress_cases"].as_array().unwrap() {
        let item = (case["tool"] != "hand").then(|| Item::from_name(case["tool"].as_str().unwrap()).unwrap());
        let block = Block::from_name(case["block"].as_str().unwrap()).unwrap();
        assert_eq!(block.hardness(), case["setup_hardness"].as_f64().unwrap() as f32);
        let digger = voxelcraft::mining::Digger {
            held: item.map(|i| Stack::new(i, 1)),
            helmet: Enchants::NONE,
            eyes_in_water: case["eyes_in_water"].as_bool().unwrap(),
            on_ground: case["on_ground"].as_bool().unwrap(),
        };
        let expected = case["delta"].as_f64().unwrap() as f32;
        assert_eq!(voxelcraft::mining::dig_progress(block, digger, 1.).to_bits(), expected.to_bits());
    }
    for case in fixture["fatigue_factors"].as_array().unwrap() {
        let mut effects = voxelcraft::simulation::effects::Effects::default();
        effects.add(
            voxelcraft::simulation::effects::Effect::MiningFatigue,
            case["amplifier"].as_u64().unwrap() as u8,
            100,
        );
        let expected = case["factor"].as_f64().unwrap() as f32;
        assert_eq!(effects.mining_factor().to_bits(), expected.to_bits(), "{case}");
    }
}
fn scene(block: Block, dimension: Dimension) -> (World, PlayerSession, Entities) {
    let mut w = World::new_remote(Arc::new(Generator::for_dimension(7, dimension)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w.set_block(IVec3::new(4, 144, 4), Block::STONE);
    w.set_block(AT, block);
    let mut s = PlayerSession::new(PlayerId(3), DVec3::new(4.5, 145., 4.5));
    s.player.on_ground = true;
    (w, s, Entities::new(7))
}
fn finish(s: &mut PlayerSession, w: &mut World, e: &mut Entities, rules: &GameRules) -> Outcome {
    for _ in 0..500 {
        let out = s.tick_mining(0, w, e, rules).unwrap();
        if !matches!(out, Outcome::Hit { .. }) {
            return out;
        }
    }
    panic!("dig did not finish");
}
#[test]
fn ice_support_matches_every_registered_reference_tag_member_and_reviewed_exceptions() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/java/26.3/mining.json")).unwrap();
    let mut checked = 0;
    for name in fixture["ice_support_blocks"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        if let Some(b) = Block::from_name(name) {
            assert!(voxelcraft::content::mining::ice_support(b), "{name}");
            checked += 1;
        }
    }
    assert!(checked > 300, "reference coverage unexpectedly dropped: {checked}");
    println!("checked {checked} registered reference ice-support names");
    for b in [Block::AIR, Block::TORCH, Block::LADDER, Block::TALL_GRASS] {
        assert!(!voxelcraft::content::mining::ice_support(b), "{}", b.name());
    }
    for b in [Block::SNOW, Block::SNOWY_GRASS, Block::WOOL, Block::GLASS, Block::WATER, Block::LAVA] {
        assert!(voxelcraft::content::mining::ice_support(b), "{}", b.name());
    }
    for facing in Facing::ALL {
        assert!(voxelcraft::content::mining::ice_support(Block::door(facing, true, true)));
    }
}

#[test]
fn replacing_the_target_and_switching_slots_restart_progress_but_idle_controller_rules_preserve_it() {
    let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
    let pick = Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Wood), 1);
    s.inventory.slots[0] = Some(pick);
    s.inventory.slots[1] = Some(pick);
    for _ in 0..5 {
        s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap();
    }
    let before = s.breaking(&w).unwrap().1;
    s.tick_mining(1, &mut w, &mut e, &GameRules::default()).unwrap();
    assert!(s.breaking(&w).unwrap().1 < before);
    w.set_block(AT, Block::COBBLESTONE);
    assert!(s.breaking(&w).is_none());
    s.tick_mining(1, &mut w, &mut e, &GameRules::default()).unwrap();
    let mut controller = Agent::new(s.player.pos);
    controller.session = s;
    controller.selected = 1;
    let progress = controller.breaking(&w).unwrap();
    controller.hold(MoveInput::default(), false, false);
    controller.tick(&mut w, &mut e);
    assert_eq!(controller.breaking(&w), Some(progress), "idle rules do not touch the hands' clock");
    controller.hold(MoveInput::default(), true, false);
    controller.tick(&mut w, &mut e);
    assert!(controller.breaking(&w).unwrap().1 > progress.1);
    controller.hold(MoveInput::default(), false, false);
    assert!(controller.breaking(&w).is_none(), "releasing CLI-held mining cancels its clock");
}
#[test]
fn ice_melts_only_over_reference_support_without_silk_touch_or_creative() {
    for (support, silk, mode, dimension, expected) in [
        (Block::STONE, false, GameMode::Survival, Dimension::Overworld, Block::WATER),
        (Block::TORCH, false, GameMode::Survival, Dimension::Overworld, Block::AIR),
        (Block::AIR, false, GameMode::Survival, Dimension::Overworld, Block::AIR),
        (Block::STONE, true, GameMode::Survival, Dimension::Overworld, Block::AIR),
        (Block::STONE, false, GameMode::Creative, Dimension::Overworld, Block::AIR),
        (Block::STONE, false, GameMode::Survival, Dimension::Nether, Block::AIR),
        (Block::STONE, false, GameMode::Survival, Dimension::End, Block::WATER),
    ] {
        let (mut w, mut s, mut e) = scene(Block::ICE, dimension);
        if support == Block::TORCH {
            w.set_block(AT - IVec3::Y * 2, Block::STONE);
        }
        w.set_block(AT - IVec3::Y, support);
        s.set_mode(mode);
        let mut pick = Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Iron), 1);
        if silk {
            pick.enchants = Enchants::NONE.with(Enchantment::SilkTouch, 1);
        }
        s.inventory.slots[0] = Some(pick);
        finish(&mut s, &mut w, &mut e, &GameRules::default());
        assert_eq!(w.get_block(AT), Some(expected));
        assert_eq!(w.drops.iter().any(|(_, stack)| stack.item == Item::from_block(Block::ICE)), silk);
    }
}
#[test]
fn client_entry_and_cli_digging_have_identical_timing_drops_and_wear() {
    for block in [Block::STONE, Block::DIRT, Block::ICE, Block::REDSTONE_ORE] {
        let (mut direct_world, mut direct, mut de) = scene(block, Dimension::Overworld);
        let (mut cli_world, session, mut ce) = scene(block, Dimension::Overworld);
        let pick = Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Iron), 1);
        direct.inventory.slots[0] = Some(pick);
        let mut cli = Agent::new(session.player.pos);
        cli.session = session;
        cli.inventory.slots[0] = Some(pick);
        for tick in 1..=200 {
            let out = direct.tick_mining(0, &mut direct_world, &mut de, &GameRules::default()).unwrap();
            cli.hold(MoveInput::default(), true, false);
            cli.tick(&mut cli_world, &mut ce);
            assert_eq!(direct_world.get_block(AT), cli_world.get_block(AT), "{} tick {tick}", block.name());
            assert_eq!(direct.inventory, cli.inventory);
            assert_eq!(direct_world.drops, cli_world.drops);
            if matches!(out, Outcome::Broken { .. }) {
                assert_eq!(direct.vitals.hunger.exhaustion, cli.vitals.hunger.exhaustion);
                break;
            }
            assert!(tick < 200);
        }
    }
}
#[test]
fn mining_upper_door_and_bed_head_removes_both_halves_and_drops_once() {
    for block in [Block::door(Facing::East, false, true), Block::BED_HEAD] {
        let (mut w, mut session, mut e) = scene(block, Dimension::Overworld);
        session.player.pitch = -0.05;
        let partner = if block.is_door() { AT - IVec3::Y } else { AT + IVec3::Z };
        if block.is_door() {
            w.set_block(partner - IVec3::Y, Block::STONE);
        }
        w.set_block(partner, if block.is_door() { Block::door(Facing::East, false, false) } else { Block::BED_FOOT });
        let mut cli = Agent::new(session.player.pos);
        cli.session = session;
        assert_eq!(cli.target(&w).map(|(pos, _)| pos), Some(AT));
        cli.execute(Command::Run(MoveInput::default(), 200, true), &mut w, &mut e, &[]).unwrap();
        for _ in 0..200 {
            cli.tick(&mut w, &mut e);
            if w.get_block(AT) == Some(Block::AIR) {
                break;
            }
        }
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        assert_eq!(w.get_block(partner), Some(Block::AIR));
        assert_eq!(w.drops.iter().map(|(_, s)| u32::from(s.count)).sum::<u32>(), 1);
    }
}
#[test]
fn invalid_actions_are_atomic_and_dropped_loot_obeys_the_gamerule() {
    let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
    for mode in [GameMode::Adventure, GameMode::Spectator] {
        s.set_mode(mode);
        assert!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).is_err());
        assert_eq!(w.get_block(AT), Some(Block::STONE));
        assert!(w.drops.is_empty());
        assert!(s.breaking(&w).is_none());
    }
    s.set_mode(GameMode::Survival);
    assert!(s.tick_mining(9, &mut w, &mut e, &GameRules::default()).is_err());
    s.inventory.slots[0] = Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Iron), 1));
    let mut rules = GameRules::default();
    rules.set("doTileDrops", "false").unwrap();
    finish(&mut s, &mut w, &mut e, &rules);
    assert!(w.drops.is_empty() && e.orbs.is_empty());
    assert_eq!(s.inventory.get(0).unwrap().damage, 1);
}
#[test]
fn creative_breaks_bedrock_and_dragon_egg_attack_teleports_without_a_drop() {
    let (mut w, mut s, mut e) = scene(Block::BEDROCK, Dimension::Overworld);
    for _ in 0..100 {
        assert!(matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::Hit { .. }));
    }
    assert_eq!(w.get_block(AT), Some(Block::BEDROCK));
    s.set_mode(GameMode::Creative);
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::Broken { .. }));
    assert_eq!(w.get_block(AT), Some(Block::AIR));
    assert!(w.drops.is_empty());
    w.set_block(AT - IVec3::Y, Block::STONE);
    w.set_block(AT, Block::DRAGON_EGG);
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::TeleportedEgg { .. }));
    assert!(w.drops.is_empty());
}
