//! Turtle equipment/progression expectations independently checked in Java 26.3.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    crafting::Grid,
    enchant::{self, Enchantment},
    entity::{Entities, MobKind},
    inventory::{Inventory, Stack},
    item::{ArmorMaterial, ArmorPiece, Item},
    potion::Potion,
    rules::GameMode,
    simulation::{
        effects::{Effect, Effects},
        survival::{Env, Vitals},
    },
    world::{World, block::Block, brewing, chunk::ChunkData, terrain::Generator},
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/turtle_integrations.json")).unwrap()
}
#[test]
fn helmet_crafting_equipment_stats_repair_and_legacy_ids() {
    let f = fixture();
    let mut grid = Grid::new(3);
    for (y, row) in f["helmet"]["pattern"].as_array().unwrap().iter().enumerate() {
        for (x, c) in row.as_str().unwrap().chars().enumerate() {
            if c != ' ' {
                grid.cells[y * 3 + x] = Some(Stack::new(Item::TURTLE_SCUTE, 1));
            }
        }
    }
    assert_eq!(grid.result(), Some(Stack::new(Item::TURTLE_HELMET, 1)));
    grid.consume();
    assert!(grid.cells.iter().all(Option::is_none));
    assert_eq!(Item::TURTLE_HELMET.durability(), Some(f["helmet"]["durability"].as_u64().unwrap() as u16));
    assert_eq!(Item::TURTLE_HELMET.as_armor(), Some((ArmorPiece::Helmet, ArmorMaterial::Turtle)));
    assert_eq!(ArmorMaterial::Turtle.defense(ArmorPiece::Helmet), f["helmet"]["defense"].as_u64().unwrap() as u8);
    assert_eq!(enchant::enchantability(Item::TURTLE_HELMET), f["helmet"]["enchantability"].as_u64().unwrap() as u32);
    assert!(Enchantment::Respiration.fits(Item::TURTLE_HELMET));
    assert!(enchant::repairs(Item::TURTLE_HELMET, Item::TURTLE_SCUTE));
    for index in 0..36 {
        let p = Potion(index);
        assert_eq!(Item::potion(p), Item(400 + u16::from(index)));
        assert_eq!(Item::splash_potion(p), Item(436 + u16::from(index)));
    }
    for p in Potion::all() {
        assert_eq!(Item::potion(p).as_potion(), Some(p));
        assert_eq!(Item::splash_potion(p).as_splash_potion(), Some(p));
        assert!(Item::splash_potion(p).as_potion().is_none());
    }
}
#[test]
fn turtle_master_brews_both_effects_in_all_variants_and_keeps_save_metadata() {
    let f = fixture();
    for case in f["brewing"].as_array().unwrap() {
        let input = Potion::from_id(case["input"].as_str().unwrap()).unwrap();
        let ingredient =
            Item::from_name(case["ingredient"].as_str().unwrap().trim_start_matches("minecraft:")).unwrap();
        let output = Potion::from_id(case["output"].as_str().unwrap()).unwrap();
        let bottle = if case["bottle"] == "splash_potion" { Item::splash_potion } else { Item::potion };
        assert_eq!(brewing::brew_item(bottle(input), ingredient), Some(bottle(output)));
        assert!(brewing::is_ingredient(ingredient));
    }
    for case in f["potions"].as_array().unwrap() {
        let p = Potion::from_id(case["id"].as_str().unwrap()).unwrap();
        let mut v = Vitals::default();
        assert_eq!(p.drink(&mut v), 0.0);
        for expected in case["effects"].as_array().unwrap() {
            let effect = Effect::from_id(expected["effect"].as_str().unwrap()).unwrap();
            let active = v.effects.get(effect).unwrap();
            assert_eq!(
                (active.amplifier, active.ticks),
                (expected["amplifier"].as_u64().unwrap() as u8, expected["ticks"].as_u64().unwrap() as u32)
            );
        }
        assert_eq!(Effects::deserialize(&v.effects.serialize()), v.effects);
        assert_eq!(p.describe_lines().len(), 2);
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(Item::potion(p), 1));
        inventory.slots[1] = Some(Stack::new(Item::splash_potion(p), 1));
        assert_eq!(Inventory::deserialize(&inventory.serialize()), Some(inventory));
        let taken = v.damage(10.0, "was slain", false);
        let amp = v.effects.get(Effect::Resistance).unwrap().amplifier;
        assert!((taken - 10.0 * (1.0 - 0.2 * (f32::from(amp) + 1.0))).abs() < 1e-5);
    }
}
#[test]
fn helmet_renews_on_dry_eyes_then_expires_underwater_without_shortening_a_potion() {
    let mut v = Vitals::default();
    let dry = Env { turtle_helmet: true, on_ground: true, ..Default::default() };
    for _ in 0..40 {
        v.tick(0.05, &dry, false);
        assert_eq!(v.effects.get(Effect::WaterBreathing).unwrap().ticks, 200);
    }
    let wet = Env { head_in_water: true, turtle_helmet: true, on_ground: true, ..Default::default() };
    let air = v.air;
    for _ in 0..199 {
        v.tick(0.05, &wet, false);
    }
    assert_eq!(v.air, air);
    assert_eq!(v.effects.get(Effect::WaterBreathing).unwrap().ticks, 1);
    v.tick(0.05, &wet, false);
    assert!(v.effects.get(Effect::WaterBreathing).is_none());
    assert!(v.air < air);
    v.effects.add(Effect::WaterBreathing, 0, 3600);
    v.tick(0.05, &dry, false);
    assert_eq!(v.effects.get(Effect::WaterBreathing).unwrap().ticks, 3599);
}
#[test]
fn cli_and_client_rule_equip_the_same_helmet_and_reject_spectators_without_consumption() {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut direct = Agent::new(DVec3::new(4.5, 145., 4.5));
    direct.inventory.slots[0] = Some(Stack::new(Item::TURTLE_HELMET, 1));
    let mut cli = Agent::new(direct.player.pos);
    cli.inventory = direct.inventory.clone();
    assert!(voxelcraft::survival_items::equip_armor(
        &direct.session.player,
        &mut direct.session.inventory,
        0,
        GameMode::Survival,
        &world
    ));
    cli.execute(Command::Place, &mut world, &mut Entities::new(7), &[]).unwrap();
    assert_eq!(direct.inventory, cli.inventory);
    cli.set_mode(GameMode::Spectator);
    cli.inventory.slots[0] = Some(Stack::new(Item::TURTLE_HELMET, 1));
    let saved = cli.inventory.clone();
    assert!(cli.execute(Command::Place, &mut world, &mut Entities::new(7), &[]).is_err());
    assert_eq!(cli.inventory, saved);
}
/// A usable block only takes the click within Java's block interaction range
/// (4.5 in Survival); a crafting table farther away lets the helmet equip.
#[test]
fn only_a_usable_block_within_block_reach_prevents_equipping() {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut player = voxelcraft::player::Player::new(DVec3::new(4.5, 145., 4.5));
    (player.yaw, player.pitch) = (0.0, 0.0);
    let eye = player.eye();
    for (x, blocked) in [(8, true), (10, false)] {
        let table = IVec3::new(x, eye.y.floor() as i32, 4);
        world.set_block(table, Block::CRAFTING_TABLE);
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(Item::TURTLE_HELMET, 1));
        let equipped = voxelcraft::survival_items::equip_armor(&player, &mut inventory, 0, GameMode::Survival, &world);
        assert_eq!(equipped, !blocked, "table {} blocks from the eye", x as f64 - eye.x);
        world.set_block(table, Block::AIR);
    }
}
#[test]
fn living_creatures_keep_turtle_master_effects_in_their_persistent_formats() {
    for kind in [MobKind::Horse, MobKind::Villager, MobKind::IronGolem] {
        let mut e = Entities::new(7);
        e.spawn(kind, DVec3::new(4.5, 145., 4.5));
        e.mobs[0].persistent = true;
        e.mobs[0].effects.add(Effect::Slowness, 3, 400);
        e.mobs[0].effects.add(Effect::Resistance, 2, 400);
        let mut restored = Entities::new(7);
        if matches!(kind, MobKind::Villager | MobKind::IronGolem) {
            restored.load_villagers(&e.villagers_to_string());
        } else {
            restored.load_nether_mobs(&e.nether_mobs_to_string());
        }
        assert_eq!(restored.mobs.len(), 1);
        assert_eq!(restored.mobs[0].effects, e.mobs[0].effects);
        assert!((restored.mobs[0].effects.speed_factor() - 0.4).abs() < 1e-6);
        let hp = restored.mobs[0].health;
        restored.mobs[0].damage(10.0, None, &mut voxelcraft::entity::Rng::new(7));
        assert!((hp - restored.mobs[0].health - 4.0).abs() < 1e-5);
    }
}

#[test]
fn creative_use_copies_armor_and_can_replace_binding_but_spectators_cannot() {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let player = voxelcraft::player::Player::new(DVec3::new(4.5, 145., 4.5));
    let mut inventory = Inventory::default();
    let helmet = Stack::new(Item::TURTLE_HELMET, 1);
    inventory.slots[0] = Some(helmet);
    assert!(voxelcraft::survival_items::equip_armor(&player, &mut inventory, 0, GameMode::Creative, &world));
    assert_eq!((inventory.slots[0], inventory.armor[0]), (Some(helmet), Some(helmet)));
    assert!(!voxelcraft::survival_items::equip_armor(&player, &mut inventory, 0, GameMode::Creative, &world));
    let mut bound = Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1);
    bound.enchants = bound.enchants.with(Enchantment::BindingCurse, 1);
    inventory.armor[0] = Some(bound);
    assert!(!voxelcraft::survival_items::equip_armor(&player, &mut inventory, 0, GameMode::Survival, &world));
    assert!(voxelcraft::survival_items::equip_armor(&player, &mut inventory, 0, GameMode::Creative, &world));
    assert_eq!((inventory.slots[0], inventory.armor[0]), (Some(bound), Some(helmet)));
    assert!(!voxelcraft::survival_items::equip_armor(&player, &mut inventory, 0, GameMode::Spectator, &world));
}
