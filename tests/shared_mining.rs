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
        overworld_blocks as ob,
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
    for _ in 0..2000 {
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
fn progress_follows_position_and_held_item_not_slot_or_block_state() {
    let rules = GameRules::default();
    let (mut w, mut s, mut e) = scene(Block::FURNACE, Dimension::Overworld);
    let pick = Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Wood), 1);
    let delta = |block| {
        let digger = voxelcraft::mining::Digger {
            held: Some(pick),
            helmet: Enchants::NONE,
            eyes_in_water: false,
            on_ground: true,
        };
        voxelcraft::mining::dig_progress(block, digger, 1.)
    };
    s.inventory.slots[0] = Some(pick);
    s.inventory.slots[1] = Some(pick);
    s.inventory.slots[2] = Some(Stack::new(Item::tool(ToolKind::Shovel, Tier::Wood), 1));
    for _ in 0..3 {
        s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    }
    let before = s.breaking(&w).unwrap().1;
    // The furnace lights at the same position: Java keeps the progress.
    w.set_block(AT, Block::LIT_FURNACE);
    assert_eq!(s.breaking(&w), Some((AT, before)));
    // An identical stack in another slot is the same destroying item.
    s.tick_mining(1, &mut w, &mut e, &rules).unwrap();
    let lit = delta(Block::LIT_FURNACE);
    assert_eq!(s.breaking(&w), Some((AT, before + lit)));
    // A different item restarts without progress, then accrues its own rate.
    assert!(matches!(s.tick_mining(2, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress: 0., .. }));
    assert!(s.breaking(&w).is_none());
    // The same slot holding a different stack (here, enchanted) also restarts.
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    assert_eq!(s.breaking(&w), Some((AT, lit)));
    s.inventory.slots[0].as_mut().unwrap().enchants = Enchants::NONE.with(Enchantment::Efficiency, 1);
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress: 0., .. }));
    s.inventory.slots[0] = Some(pick);
    s.tick_mining(1, &mut w, &mut e, &rules).unwrap();
    s.tick_mining(1, &mut w, &mut e, &rules).unwrap();
    w.set_block(AT, Block::STONE);
    s.tick_mining(1, &mut w, &mut e, &rules).unwrap();
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
fn creative_destroys_bedrock_and_the_dragon_egg_while_a_survival_attack_teleports_it() {
    let (mut w, mut s, mut e) = scene(Block::BEDROCK, Dimension::Overworld);
    for _ in 0..100 {
        assert!(matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::Hit { .. }));
    }
    assert_eq!(w.get_block(AT), Some(Block::BEDROCK));
    s.set_mode(GameMode::Creative);
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::Broken { .. }));
    assert_eq!(w.get_block(AT), Some(Block::AIR));
    assert!(w.drops.is_empty());
    // Java's instabuild branch destroys before `BlockState.attack`: no teleport.
    w.set_block(AT - IVec3::Y, Block::STONE);
    w.set_block(AT, Block::DRAGON_EGG);
    s.cancel_mining();
    let out = s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap();
    assert!(matches!(out, Outcome::Broken { block: Block::DRAGON_EGG, .. }), "{out:?}");
    assert_eq!(w.get_block(AT), Some(Block::AIR));
    assert!(w.drops.is_empty());
    // Survival start attacks the egg. Java 26.3 requires non-air below
    // the destination; provide a floor, rather than unsupported empty air.
    let (mut w, mut s, mut e) = scene(Block::AIR, Dimension::Overworld);
    for x in -8..=8 {
        for z in -8..=8 {
            w.set_block(AT + IVec3::new(x, -1, z), Block::STONE);
        }
    }
    w.set_block(AT - IVec3::Y, Block::STONE);
    w.set_block(AT, Block::DRAGON_EGG);
    let out = s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap();
    assert!(matches!(out, Outcome::TeleportedEgg { destination: Some(_), .. }), "{out:?}");
    assert_ne!(w.get_block(AT), Some(Block::DRAGON_EGG));
    assert!(w.drops.is_empty());
}

#[test]
fn iron_doors_drop_only_with_a_pickaxe_from_either_half_and_never_in_creative() {
    let pick = Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Wood), 1));
    for (upper, held, mode, drops) in [
        (true, None, GameMode::Survival, 0),
        (true, pick, GameMode::Survival, 1),
        (false, None, GameMode::Survival, 0),
        (false, pick, GameMode::Survival, 1),
        (true, None, GameMode::Creative, 0),
        (true, pick, GameMode::Creative, 0),
    ] {
        let target = voxelcraft::world::redstone_blocks::iron_door(Facing::East, false, upper);
        let (mut w, mut s, mut e) = scene(target, Dimension::Overworld);
        let partner = if upper { AT - IVec3::Y } else { AT + IVec3::Y };
        w.set_block(partner, voxelcraft::world::redstone_blocks::iron_door(Facing::East, false, !upper));
        w.set_block(AT.min(partner) - IVec3::Y, Block::STONE);
        s.player.pitch = -0.05;
        s.set_mode(mode);
        s.inventory.slots[0] = held;
        assert_eq!(s.block_target(&w).map(|(p, _)| p), Some(AT), "{upper} {mode:?}");
        finish(&mut s, &mut w, &mut e, &GameRules::default());
        assert_eq!((w.get_block(AT), w.get_block(partner)), (Some(Block::AIR), Some(Block::AIR)), "{upper} {mode:?}");
        let dropped: u32 = w.drops.iter().map(|(_, s)| u32::from(s.count)).sum();
        assert_eq!(dropped, drops, "upper {upper}, pickaxe {}, {mode:?}", held.is_some());
        assert!(w.drops.iter().all(|(_, s)| s.item == Item::from_name("iron_door").unwrap()));
    }
}

#[test]
fn creative_upper_double_plant_removes_the_lower_half_without_a_drop() {
    for (mode, drops) in [(GameMode::Creative, 0), (GameMode::Survival, 1)] {
        let (mut w, mut s, mut e) = scene(Block(ob::SUNFLOWER.0 + 1), Dimension::Overworld);
        w.set_block(AT - IVec3::Y * 2, Block::DIRT);
        w.before_overworld_place(AT - IVec3::Y, ob::SUNFLOWER);
        w.set_block(AT - IVec3::Y, ob::SUNFLOWER);
        assert_eq!(w.get_block(AT), Some(Block(ob::SUNFLOWER.0 + 1)));
        s.player.pitch = -0.05;
        s.set_mode(mode);
        assert_eq!(s.block_target(&w).map(|(p, _)| p), Some(AT));
        finish(&mut s, &mut w, &mut e, &GameRules::default());
        assert_eq!((w.get_block(AT), w.get_block(AT - IVec3::Y)), (Some(Block::AIR), Some(Block::AIR)));
        assert_eq!(w.drops.iter().map(|(_, s)| u32::from(s.count)).sum::<u32>(), drops, "{mode:?}");
    }
}

#[test]
fn mining_exhaustion_needs_the_correct_tool_but_wear_does_not() {
    for (held, exhaustion, drops) in [
        (Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Wood), 1)), 0.005, true),
        (None, 0., false),
        (Some(Stack::new(Item::tool(ToolKind::Shovel, Tier::Wood), 1)), 0., false),
    ] {
        let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
        s.inventory.slots[0] = held;
        finish(&mut s, &mut w, &mut e, &GameRules::default());
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        assert_eq!(s.vitals.hunger.exhaustion, exhaustion);
        assert_eq!(!w.drops.is_empty(), drops);
        assert_eq!(s.inventory.get(0).map(|s| s.damage), held.map(|_| 1), "wear is unconditional");
    }
}

#[test]
fn creative_swords_cannot_break_blocks_but_other_items_can() {
    let sword = Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1);
    let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
    s.set_mode(GameMode::Creative);
    s.inventory.slots[0] = Some(sword);
    s.inventory.slots[1] = Some(Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Wood), 1));
    assert!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).is_err());
    assert_eq!(w.get_block(AT), Some(Block::STONE));
    assert!(matches!(s.tick_mining(1, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::Broken { .. }));
    // Survival swords still mine (and wear twice per block, Java's damage_per_block 2).
    let (mut w, mut s, mut e) = scene(Block::DIRT, Dimension::Overworld);
    s.inventory.slots[0] = Some(sword);
    finish(&mut s, &mut w, &mut e, &GameRules::default());
    assert_eq!(w.get_block(AT), Some(Block::AIR));
    assert_eq!(s.inventory.get(0).unwrap().damage, 2);
}

#[test]
fn haste_multiplies_dig_speed_before_fatigue_in_reference_float_order_and_persists() {
    use voxelcraft::simulation::effects::{Effect, Effects};
    let pick = Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Iron), 1);
    let digger =
        voxelcraft::mining::Digger { held: Some(pick), helmet: Enchants::NONE, eyes_in_water: false, on_ground: true };
    for amplifier in 0..=255u8 {
        let mut effects = Effects::default();
        effects.add(Effect::Haste, amplifier, 100);
        effects.add(Effect::MiningFatigue, 0, 100);
        // Player.getDestroySpeed: speed * (1.0f + (float)(amp + 1) * 0.2f), then * fatigue.
        let haste = 1.0f32 + (i32::from(amplifier) + 1) as f32 * 0.2f32;
        assert_eq!(effects.dig_speed_factor().to_bits(), haste.to_bits());
        let speed = 6.0f32 * haste * 0.3f64.powf(1.) as f32;
        let expected = speed / Block::STONE.hardness() / 30.0;
        let actual = voxelcraft::mining::dig_progress_with(
            Block::STONE,
            digger,
            effects.dig_speed_factor(),
            effects.mining_factor(),
        );
        assert_eq!(actual.to_bits(), expected.to_bits(), "amplifier {amplifier}");
        assert_eq!(Effects::deserialize(&effects.serialize()), effects);
    }
    assert_eq!(Effects::default().dig_speed_factor(), 1.0);
    assert_eq!(Effect::from_id("minecraft:haste"), Some(Effect::Haste));
    // Through the shared transaction: Haste II cuts iron-pickaxe stone from 8 ticks to 6.
    for (amplifier, ticks) in [(None, 8), (Some(1), 6)] {
        let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
        s.inventory.slots[0] = Some(pick);
        if let Some(a) = amplifier {
            s.vitals.effects.add(Effect::Haste, a, 1000);
        }
        let mut n = 0;
        while !matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), Outcome::Broken { .. }) {
            n += 1;
        }
        assert_eq!(n + 1, ticks, "{amplifier:?}");
    }
}

/// Ticks (0 = the press tick) at which a held attack destroys a block.
fn destroy_ticks(mut step: impl FnMut() -> Option<Outcome>, ticks: usize) -> Vec<usize> {
    (0..ticks).filter(|_| matches!(step(), Some(Outcome::Broken { .. }))).collect()
}
type TimingCase = (GameMode, [Block; 3], Option<Stack>, &'static [usize]);

/// Java `MultiPlayerGameMode` (client; the server jar does not contain it):
/// Creative destroys on the press, then every sixth tick after a five-tick
/// `destroyDelay` decremented in the same tick; Survival waits five delay
/// ticks after a completed timed break, restarts without progress for one
/// tick, and instant blocks break every tick with no delay.
#[test]
fn held_attack_timing_matches_java_destroy_delay_through_session_and_cli() {
    let pick = Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Iron), 1);
    let cases: [TimingCase; 4] = [
        (GameMode::Creative, [Block::STONE; 3], None, &[0, 5, 11]),
        (GameMode::Survival, [Block::TNT; 3], None, &[0, 1, 2]),
        (GameMode::Survival, [Block::STONE; 3], Some(pick), &[7, 21, 35]),
        (GameMode::Survival, [Block::STONE, Block::TNT, Block::TNT], Some(pick), &[7, 13, 14]),
    ];
    for (mode, blocks, held, expected) in cases {
        let setup = || {
            let (mut w, mut s, e) = scene(blocks[0], Dimension::Overworld);
            for (i, b) in blocks.into_iter().enumerate() {
                w.set_block(AT + IVec3::X * i as i32, b);
            }
            w.set_block(IVec3::new(5, 144, 4), Block::STONE);
            s.set_mode(mode);
            s.player.pos.x = 5.5;
            s.inventory.slots[0] = held;
            (w, s, e)
        };
        let cleared = |w: &World| (0..3).filter(|i| w.get_block(AT + IVec3::X * *i) == Some(Block::AIR)).count();
        let (mut w, mut s, mut e) = setup();
        // Like the adapters, an empty aim releases the input.
        let step = || match s.tick_mining(0, &mut w, &mut e, &GameRules::default()) {
            Ok(out) => Some(out),
            Err(_) => {
                s.cancel_mining();
                None
            }
        };
        let direct = destroy_ticks(step, 40);
        assert_eq!(direct, expected, "session {mode:?} {:?}", blocks.map(|b| b.name()));
        let (mut w, session, mut e) = setup();
        let mut cli = Agent::new(session.player.pos);
        cli.session = session;
        cli.set_mode(mode);
        let mut broke = Vec::new();
        for t in 0..40 {
            let before = cleared(&w);
            cli.hold(MoveInput::default(), true, false);
            cli.tick(&mut w, &mut e);
            if cleared(&w) > before {
                broke.push(t);
            }
        }
        assert_eq!(broke, expected, "CLI {mode:?} {:?}", blocks.map(|b| b.name()));
    }
}

#[test]
fn missed_rays_preserve_held_input_and_reaiming_a_new_target_starts_at_zero() {
    let (mut w, mut s, mut e) = scene(Block::STONE, Dimension::Overworld);
    let rules = GameRules::default();
    s.player.yaw = std::f32::consts::PI;
    assert!(s.tick_mining(0, &mut w, &mut e, &rules).is_err());
    s.player.yaw = 0.;
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress: 0., .. }));
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    let before = s.breaking(&w).unwrap();
    s.player.yaw = std::f32::consts::PI;
    assert!(s.tick_mining(0, &mut w, &mut e, &rules).is_err());
    assert_eq!(s.breaking(&w), Some(before));
    s.player.yaw = 0.;
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    assert!(s.breaking(&w).unwrap().1 > before.1);
    w.set_block(AT - IVec3::X, Block::STONE);
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress: 0., .. }));
    s.cancel_mining();
    assert!(
        matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress, .. } if progress > 0.)
    );
}

#[test]
fn progress_survives_dropping_part_of_the_stack_a_mode_change_and_picking_the_same_item() {
    use voxelcraft::session::interact::Pick;
    let rules = GameRules::default();
    let (mut w, mut s, mut e) = scene(Block::FURNACE, Dimension::Overworld);
    let pick = Item::tool(ToolKind::Pickaxe, Tier::Wood);
    s.inventory.slots[0] = Some(Stack::new(Item::from_block(Block::STONE), 4));
    for _ in 0..3 {
        s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    }
    let before = s.breaking(&w).unwrap().1;
    // Dropping one of four keeps the same item and components.
    s.inventory.slots[0] = Some(Stack::new(Item::from_block(Block::STONE), 3));
    s.set_mode(GameMode::Survival);
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    assert!(s.breaking(&w).unwrap().1 > before);
    // Picking the block replaces the held slot with a different item: restart.
    s.inventory.slots[0] = Some(Stack::new(pick, 1));
    s.inventory.slots[12] = Some(Stack::new(Item::from_block(Block::FURNACE), 1));
    assert_eq!(s.pick_block(0, &w), Some(Pick::Replaced));
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress: 0., .. }));
    assert_eq!(s.breaking(&w), None);
}

#[test]
fn continued_mining_does_not_relight_ore_after_a_random_tick() {
    let (mut w, mut s, mut e) = scene(Block::REDSTONE_ORE, Dimension::Overworld);
    let rules = GameRules::default();
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    assert_eq!(w.get_block(AT), Some(Block(1435)));
    // Java's random tick can extinguish the ore during a held attack.
    w.set_block(AT, Block::REDSTONE_ORE);
    s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
    assert_eq!(w.get_block(AT), Some(Block::REDSTONE_ORE));
    assert!(s.breaking(&w).is_some());
}

#[test]
fn server_rejects_a_stop_destroy_below_the_tolerance_and_destroys_when_its_own_progress_reaches_one() {
    let rules = GameRules::default();
    let (mut w, mut s, mut e) = scene(Block::DIRT, Dimension::Overworld);
    let mut shovel = Stack::new(Item::tool(ToolKind::Shovel, Tier::Diamond), 1);
    shovel.enchants = Enchants::NONE.with(Enchantment::Efficiency, 2);
    s.inventory.slots[0] = Some(shovel);
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { .. }));
    // The player leaves the ground: the client still completes (0.87 + 0.17) but the
    // server's `rate * (ticks + 1)` is 0.35, below `STOP_DESTROY_BLOCK`'s 0.7.
    s.player.on_ground = false;
    assert!(matches!(s.tick_mining(0, &mut w, &mut e, &rules).unwrap(), Outcome::Hit { progress: 0., .. }));
    assert_eq!(w.get_block(AT), Some(Block::DIRT));
    // Java's delayed destroy fires once `0.17 * (elapsed + 1) >= 1`.
    let mut ticks = 0;
    let broken = loop {
        ticks += 1;
        let out = s.tick_mining(0, &mut w, &mut e, &rules).unwrap();
        if matches!(out, Outcome::Broken { .. }) || ticks > 20 {
            break out;
        }
    };
    assert!(matches!(broken, Outcome::Broken { cell: AT, .. }), "{broken:?}");
    assert_eq!(ticks, 4);
    assert_eq!(w.get_block(AT), Some(Block::AIR));
    // A steady rate always passes: 1.0 >= 0.7.
    let (mut w, mut s, mut e) = scene(Block::DIRT, Dimension::Overworld);
    s.inventory.slots[0] = Some(shovel);
    assert!(matches!(finish(&mut s, &mut w, &mut e, &rules), Outcome::Broken { .. }));
}

/// Engine labels for reference names that differ.
fn engine_name(reference: &str) -> String {
    match reference {
        "short grass" => "tall grass".into(),
        "tall grass" => "tall grass plant".into(),
        "vine" => "vines".into(),
        name => name.into(),
    }
}

#[test]
fn tool_rules_follow_the_reference_mineable_tags_for_every_registered_name() {
    use voxelcraft::content::mining::{mineable_by, sword_speed};
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/java/26.3/mining.json")).unwrap();
    let rules = &fixture["tool_rules"];
    let kinds =
        [("pickaxe", ToolKind::Pickaxe), ("axe", ToolKind::Axe), ("shovel", ToolKind::Shovel), ("hoe", ToolKind::Hoe)];
    let name_set = |v: &serde_json::Value| -> Vec<String> {
        v.as_array().unwrap().iter().map(|n| n.as_str().unwrap().replace('_', " ")).collect()
    };
    let mut checked = 0;
    for (tag, kind) in kinds {
        for name in name_set(&rules["mineable"][tag]) {
            if let Some(b) = Block::from_name(&engine_name(&name)) {
                assert!(mineable_by(b, kind), "{name} should be mineable by {tag}");
                checked += 1;
            }
        }
    }
    assert!(checked > 250, "reference coverage unexpectedly dropped: {checked}");
    // Every other kind is rejected for names that exist in the reference.
    let mut rejected = 0;
    for name in name_set(&rules["all_blocks"]) {
        let Some(b) = Block::from_name(&engine_name(&name)) else { continue };
        for (tag, kind) in kinds {
            if !name_set(&rules["mineable"][tag]).contains(&name) {
                assert!(!mineable_by(b, kind), "{name} must not be mineable by {tag}");
                rejected += 1;
            }
        }
    }
    assert!(rejected > 1000, "negative coverage unexpectedly dropped: {rejected}");
    // Rules the old engine table lacked: a hoe digs leaves, a pickaxe digs
    // stone buttons and dead coral, an axe digs wooden buttons.
    for (name, kind) in [
        ("oak leaves", ToolKind::Hoe),
        ("spruce leaves", ToolKind::Hoe),
        ("stone button", ToolKind::Pickaxe),
        ("dead tube coral", ToolKind::Pickaxe),
        ("dead tube coral fan", ToolKind::Pickaxe),
        ("oak button", ToolKind::Axe),
        ("warped button", ToolKind::Axe),
    ] {
        let b = Block::from_name(name).unwrap_or_else(|| panic!("{name}"));
        assert!(mineable_by(b, kind), "{name}");
    }
    // Sword rules: cobweb 15, instant bamboo, 1.5 for the efficient tag.
    assert_eq!(sword_speed(Block::COBWEB), Some(15.0));
    assert_eq!(sword_speed(Block::from_name("oak leaves").unwrap()), Some(1.5));
    assert_eq!(sword_speed(Block::from_name("melon").unwrap()), Some(1.5));
    assert_eq!(sword_speed(Block::STONE), None);
}

#[test]
fn hoes_swords_and_shears_dig_at_their_reference_rule_speeds() {
    use voxelcraft::mining::break_time;
    let leaves = Block::from_name("oak leaves").unwrap();
    let hand = break_time(leaves, None);
    let iron_hoe = break_time(leaves, Some(Item::tool(ToolKind::Hoe, Tier::Iron)));
    assert!((hand / iron_hoe - 6.0).abs() < 1e-4, "iron hoe speed 6 on leaves: {hand} {iron_hoe}");
    let sword = break_time(leaves, Some(Item::tool(ToolKind::Sword, Tier::Iron)));
    assert!((hand / sword - 1.5).abs() < 1e-4, "sword_efficient is 1.5: {hand} {sword}");
    assert_eq!(break_time(leaves, Some(Item::SHEARS)), hand / 15.0);
    // Shears: vines and glow lichen dig at 2, wool at 5, carpets are not wool.
    let vines = Block::from_name("vines").unwrap();
    assert_eq!(break_time(vines, Some(Item::SHEARS)), break_time(vines, None) / 2.0);
    let wool = Block::from_name("white wool").unwrap();
    assert_eq!(break_time(wool, Some(Item::SHEARS)), break_time(wool, None) / 5.0);
    // A hand cannot harvest stone and a stone button needs no tool for drops.
    assert!(!voxelcraft::mining::can_harvest(Block::STONE, None));
    let button = Block::from_name("stone button").unwrap();
    assert!(voxelcraft::mining::can_harvest(button, None));
    assert!(break_time(button, Some(Item::tool(ToolKind::Pickaxe, Tier::Iron))) < break_time(button, None));
}

#[test]
fn shear_drops_match_the_reference_loot_tables() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/java/26.3/mining.json")).unwrap();
    let mut w = World::new_remote(Arc::new(Generator::for_dimension(7, Dimension::Overworld)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut checked = 0;
    for (name, entries) in fixture["tool_rules"]["shear_loot"].as_object().unwrap() {
        let entry = &entries[0];
        let Some(count) = entry["count"].as_u64() else { continue };
        let reference = name.replace('_', " ");
        let item = engine_name(&entry["item"].as_str().unwrap().replace('_', " "));
        for id in 0..voxelcraft::world::block::STATE_CAPACITY as u16 {
            let b = Block(id);
            if b.base() != b || b.name() != engine_name(&reference) {
                continue;
            }
            w.drops.clear();
            w.spill_with_item(AT, b, Some(Stack::new(Item::SHEARS, 1)));
            let got: Vec<_> = w.drops.iter().map(|(_, s)| (s.item.name().to_string(), u64::from(s.count))).collect();
            assert_eq!(got, vec![(item.clone(), count)], "{name} (block {id})");
            checked += 1;
        }
    }
    assert!(checked >= 20, "shear coverage unexpectedly dropped: {checked}");
}

#[test]
fn a_player_breaking_either_half_of_tall_grass_with_shears_gets_two_blades() {
    for upper in [false, true] {
        let (mut w, mut s, mut e) = scene(Block(ob::DOUBLE_TALL_GRASS.0 + upper as u16), Dimension::Overworld);
        let low = if upper { AT - IVec3::Y } else { AT };
        w.set_block(low - IVec3::Y, Block::DIRT);
        w.set_block(low + IVec3::Y, Block(ob::DOUBLE_TALL_GRASS.0 + 1));
        w.before_overworld_place(low, ob::DOUBLE_TALL_GRASS);
        w.set_block(low, ob::DOUBLE_TALL_GRASS);
        s.inventory.slots[0] = Some(Stack::new(Item::SHEARS, 1));
        s.player.pitch = -0.05;
        s.set_mode(GameMode::Survival);
        finish(&mut s, &mut w, &mut e, &GameRules::default());
        let got: Vec<_> = w.drops.iter().map(|(_, s)| (s.item.name().to_string(), s.count)).collect();
        assert_eq!(got, vec![("tall grass".to_string(), 2)], "upper half: {upper}");
    }
}
