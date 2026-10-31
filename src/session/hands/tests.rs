use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::*;
use crate::{
    crafting::Grid,
    entity::{Entities, PlayerId},
    inventory::Inventory,
    rules::GameMode,
    session::interact::{UseContext, Used},
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

const P: IVec3 = IVec3::new(8, 140, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    for x in 0..16 {
        for z in 0..16 {
            w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
        }
    }
    w
}

/// A survival player at the chunk's middle, facing +X (yaw 0).
fn session() -> PlayerSession {
    let mut s = PlayerSession::new(PlayerId::HOST, P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
    s.mode = GameMode::Survival;
    s.player.yaw = 0.0;
    s.player.pitch = 0.0;
    s
}

fn raise(s: &mut PlayerSession, ticks: u32) {
    assert!(s.start_blocking(0));
    for _ in 0..ticks {
        s.tick_blocking(0, true);
    }
}

#[test]
fn the_shield_is_a_craftable_336_use_wooden_item_repaired_with_planks() {
    assert_eq!(Item::SHIELD.name(), "shield");
    assert_eq!(Item::SHIELD.max_stack(), 1);
    assert_eq!(Item::SHIELD.durability(), Some(336));
    assert!(crate::enchant::repairs(Item::SHIELD, Block::PLANKS.into()));
    assert!(!crate::enchant::repairs(Item::SHIELD, Item::IRON_INGOT));
    assert!(Item::creative_palette().any(|i| i == Item::SHIELD));
    // Six planks of any kind around an iron ingot.
    for planks in [Block::PLANKS, crate::world::block::Wood::ALL[3].planks()] {
        let mut g = Grid::new(3);
        for i in [0, 2, 3, 4, 5, 7] {
            g.cells[i] = Some(crate::inventory::Stack::new(planks, 1));
        }
        g.cells[1] = Some(crate::inventory::Stack::new(Item::IRON_INGOT, 1));
        assert_eq!(g.result().map(|s| s.item), Some(Item::SHIELD));
    }
    let mut g = Grid::new(3);
    for i in [0, 2, 3, 4, 5, 7] {
        g.cells[i] = Some(crate::inventory::Stack::new(Block::PLANKS, 1));
    }
    assert_eq!(g.result(), None, "no iron, no shield");
}

#[test]
fn swapping_hands_trades_the_stacks_and_saves() {
    let mut s = session();
    s.inventory.slots[2] = Some(Stack::new(Item::SHIELD, 1));
    s.inventory.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
    assert!(s.swap_hands(2));
    assert_eq!(s.inventory.slots[2].map(|x| x.item), Some(Item::TOTEM_OF_UNDYING));
    assert_eq!(s.inventory.offhand.map(|x| x.item), Some(Item::SHIELD));
    // An empty hand swaps with a full one, and back.
    s.inventory.slots[2] = None;
    assert!(s.swap_hands(2));
    assert_eq!(s.inventory.offhand, None);
    assert_eq!(s.inventory.slots[2].map(|x| x.item), Some(Item::SHIELD));

    let mut inv = Inventory::default();
    inv.offhand = Some(Stack::new(Item::SHIELD, 1));
    inv.armor[1] = Some(Stack::new(Item::IRON_INGOT, 1));
    let back = Inventory::deserialize(&inv.serialize()).unwrap();
    assert_eq!(back.offhand, inv.offhand);
    assert_eq!(back.armor, inv.armor);
    // Only an off-hand: the armor fields stay empty.
    let mut only = Inventory::default();
    only.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
    assert_eq!(Inventory::deserialize(&only.serialize()).unwrap().offhand, only.offhand);
    // A save from before the off-hand loads with it empty.
    let mut old = Inventory::default();
    old.armor[0] = Some(Stack::new(Item::IRON_INGOT, 1));
    assert_eq!(Inventory::deserialize(&old.serialize()).unwrap().offhand, None);
    // Dropped on death.
    assert!(inv.take_all().iter().any(|s| s.item == Item::SHIELD));
    assert_eq!(inv.offhand, None);
}

#[test]
fn a_raised_shield_blocks_only_after_the_five_tick_delay() {
    let mut s = session();
    s.inventory.slots[0] = Some(Stack::new(Item::SHIELD, 1));
    assert!(!s.shield_raised() && !s.blocking());
    raise(&mut s, 4);
    assert!(s.shield_raised() && !s.blocking(), "still raising");
    assert_eq!(s.shield_intercept(0, 2.0, DVec3::X), ShieldHit::default());
    s.tick_blocking(0, true);
    assert!(s.blocking());
    assert!(s.using_item(), "raising a shield slows movement like any held use");
    // Releasing lowers it at once.
    s.tick_blocking(0, false);
    assert!(!s.shield_raised());
}

#[test]
fn a_shield_stops_frontal_hits_and_wears_by_one_plus_the_damage() {
    let mut s = session();
    s.inventory.slots[0] = Some(Stack::new(Item::SHIELD, 1));
    raise(&mut s, 6);
    // From the front (+X) and the sides within 90 degrees: blocked.
    assert!(s.shield_intercept(0, 1.0, DVec3::X).blocked);
    assert!(s.shield_intercept(0, 1.0, DVec3::new(1.0, 0.0, 1.0)).blocked);
    // From behind: not.
    assert!(!s.shield_intercept(0, 4.0, DVec3::NEG_X).blocked);
    assert!(!s.shield_intercept(0, 4.0, DVec3::new(-1.0, 0.0, 2.0)).blocked);
    // Straight above or below: no horizontal source, not blocked.
    assert!(!s.shield_intercept(0, 4.0, DVec3::Y).blocked);
    // Below three half-hearts the shield takes no wear; at four it takes five.
    assert_eq!(s.inventory.slots[0].unwrap().damage, 0);
    assert!(s.shield_intercept(0, 4.0, DVec3::X).blocked);
    assert_eq!(s.inventory.slots[0].unwrap().damage, 5);
    // Arrows and blasts arrive with a direction too; creative players lose nothing.
    s.mode = GameMode::Creative;
    s.shield_intercept(0, 20.0, DVec3::X);
    assert_eq!(s.inventory.slots[0].unwrap().damage, 5);
}

#[test]
fn a_worn_out_shield_breaks_and_lowers() {
    let mut s = session();
    s.inventory.slots[0] = Some(Stack { damage: 334, ..Stack::new(Item::SHIELD, 1) });
    raise(&mut s, 6);
    let hit = s.shield_intercept(0, 3.0, DVec3::X);
    assert_eq!(hit, ShieldHit { blocked: true, broke: true });
    assert_eq!(s.inventory.slots[0], None);
    assert!(!s.shield_raised());
}

#[test]
fn an_axe_hit_disables_the_shield_for_five_seconds() {
    let mut s = session();
    s.inventory.slots[0] = Some(Stack::new(Item::SHIELD, 1));
    raise(&mut s, 6);
    assert!(s.disable_shield());
    assert!(!s.shield_raised());
    assert_eq!(s.shield_cooldown(), DISABLE_TICKS);
    assert!(!s.start_blocking(0), "cannot raise while disabled");
    for _ in 0..DISABLE_TICKS - 1 {
        s.tick_blocking(0, true);
    }
    assert!(!s.start_blocking(0));
    s.tick_blocking(0, true);
    assert_eq!(s.shield_cooldown(), 0);
    assert!(s.start_blocking(0));
    // A lowered shield is not disabled by an axe.
    s.stop_blocking();
    assert!(!s.disable_shield());
    assert_eq!(s.shield_cooldown(), 0);
}

#[test]
fn the_offhand_shield_rises_when_the_main_hand_passes_but_not_over_food() {
    let mut s = session();
    s.inventory.offhand = Some(Stack::new(Item::SHIELD, 1));
    // Empty hand, a sword, a block: the off-hand shield.
    for main in
        [None, Some(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Iron)), Some(Block::STONE.into())]
    {
        s.inventory.slots[0] = main.map(|i| Stack::new(i, 1));
        assert_eq!(s.shield_hand(0), Some(Hand::Off), "{main:?}");
    }
    // Hungry with bread in the main hand: eating wins.
    s.vitals.hunger.food = 10.0;
    s.inventory.slots[0] = Some(Stack::new(Item::BREAD, 1));
    assert_eq!(s.shield_hand(0), None);
    s.inventory.slots[0] = Some(Stack::new(Item::BOW, 1));
    assert_eq!(s.shield_hand(0), None);
    // A shield in the main hand is raised from there.
    s.inventory.slots[0] = Some(Stack::new(Item::SHIELD, 1));
    assert_eq!(s.shield_hand(0), Some(Hand::Main));

    // Raising the off-hand shield, then swapping it away lowers it.
    s.inventory.slots[0] = None;
    raise(&mut s, 6);
    assert!(s.blocking());
    s.inventory.offhand.as_mut().unwrap().damage = 0;
    assert!(s.swap_hands(0));
    assert!(!s.shield_raised());
}

#[test]
fn offhand_shield_wear_lands_on_the_offhand_stack() {
    let mut s = session();
    s.inventory.offhand = Some(Stack::new(Item::SHIELD, 1));
    raise(&mut s, 6);
    assert!(s.shield_intercept(0, 6.0, DVec3::X).blocked);
    assert_eq!(s.inventory.offhand.unwrap().damage, 7);
}

#[test]
fn use_falls_back_to_the_offhand_for_blocks_and_torches() {
    let mut w = world();
    let mut entities = Entities::new(7);
    let mut s = session();
    // Look down at the floor stone, empty main hand, torch in the off-hand.
    s.player.pitch = -std::f32::consts::FRAC_PI_2 + 0.01;
    s.inventory.offhand = Some(Stack::new(Block::TORCH, 3));
    let used = s.use_held(0, &mut w, &mut entities, &UseContext { sneaking: false, others: &[] }).unwrap();
    assert!(used.as_ref().is_some_and(|u| u.off_hand), "the use reports the off-hand");
    assert!(matches!(used.map(|u| u.used), Some(Used::Placed(_))));
    assert_eq!(s.inventory.offhand.map(|x| x.count), Some(2));
    assert_eq!(s.inventory.slots[0], None, "the main hand stayed empty");
    let torch =
        (0..16).flat_map(|x| (0..16).map(move |z| IVec3::new(x, P.y, z))).any(|p| w.get_block(p) == Some(Block::TORCH));
    assert!(torch);

    // A main-hand item that does something keeps the click.
    let mut w = world();
    let mut s = session();
    s.player.pitch = -0.7;
    s.inventory.slots[0] = Some(Stack::new(Block::STONE, 3));
    s.inventory.offhand = Some(Stack::new(Block::TORCH, 3));
    let before = s.inventory.offhand;
    let used = s.use_held(0, &mut w, &mut entities, &UseContext { sneaking: false, others: &[] }).unwrap();
    assert!(used.is_some_and(|u| !u.off_hand));
    assert_eq!(s.inventory.offhand, before);
    assert_eq!(s.inventory.slots[0].map(|x| x.count), Some(2));
}

#[test]
fn eating_works_from_the_offhand_and_leaves_the_main_hand_alone() {
    let mut s = session();
    let mut entities = Entities::new(7);
    s.vitals.hunger.food = 5.0;
    let sword = Stack::new(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Iron), 1);
    s.inventory.slots[0] = Some(sword);
    s.inventory.offhand = Some(Stack::new(Item::BREAD, 2));
    assert_eq!(s.consumable_in_hands(0), Some(Item::BREAD));
    let mut done = None;
    for _ in 0..40 {
        let step = s.tick_consumption_either(0, true, &mut entities).unwrap();
        if let Some(c) = step.completed {
            done = Some(c);
            break;
        }
    }
    assert_eq!(done.map(|c| c.item), Some(Item::BREAD));
    assert_eq!(s.inventory.slots[0], Some(sword), "the sword never left the main hand");
    assert_eq!(s.inventory.offhand.map(|x| x.count), Some(1));
    assert!(s.vitals.hunger.food > 5.0);
    assert!(!s.consuming());
}

#[test]
fn a_main_hand_shield_or_bow_keeps_the_use_from_offhand_food() {
    for main in [Item::SHIELD, Item::BOW] {
        let mut s = session();
        s.vitals.hunger.food = 5.0;
        s.inventory.slots[0] = Some(Stack::new(main, 1));
        s.inventory.offhand = Some(Stack::new(Item::BREAD, 2));
        assert_eq!(s.consumable_in_hands(0), None, "{main:?}");
        assert!(s.consumption_duration_either(0).is_err(), "{main:?}");
    }
    // An interrupted off-hand meal doesn't keep its claim on the next click.
    let mut s = session();
    let mut entities = Entities::new(7);
    s.vitals.hunger.food = 5.0;
    s.inventory.offhand = Some(Stack::new(Item::BREAD, 2));
    s.tick_consumption_either(0, true, &mut entities).unwrap();
    assert!(s.consuming());
    s.cancel_consumption();
    s.inventory.slots[0] = Some(Stack::new(Item::SHIELD, 1));
    assert_eq!(s.consumable_in_hands(0), None);
}

#[test]
fn a_totem_in_either_hand_undoes_a_lethal_hit_once() {
    for in_off in [false, true] {
        let mut s = session();
        if in_off {
            s.inventory.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
        } else {
            s.inventory.slots[0] = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
        }
        s.vitals.damage(100.0, "was slain by a zombie", false);
        assert!(s.vitals.is_dead());
        assert!(s.totem_saves(0));
        assert!(!s.vitals.is_dead());
        assert_eq!(s.vitals.health, 1.0);
        assert!(s.vitals.effects.has(Effect::Regeneration));
        assert!(s.vitals.effects.has(Effect::Absorption));
        assert!(s.vitals.effects.has(Effect::FireResistance));
        assert_eq!(s.inventory.slots[0], None);
        assert_eq!(s.inventory.offhand, None);
    }
    // No totem: death stands. The void ignores one.
    let mut s = session();
    s.vitals.damage(100.0, "was slain by a zombie", false);
    assert!(!s.totem_saves(0));
    let mut s = session();
    s.inventory.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
    s.vitals.damage(100.0, "fell out of the world", false);
    assert!(!s.totem_saves(0));
    assert!(s.inventory.offhand.is_some());
    // `/kill` bypasses it; magic (harming) is an ordinary lethal hit.
    s.vitals.death = Some("was killed".into());
    assert!(!s.totem_saves(0));
    let mut s = session();
    s.inventory.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
    s.vitals.damage(100.0, crate::simulation::survival::CAUSE_MAGIC, false);
    assert!(s.totem_saves(0));
}

#[test]
fn swapping_hands_ends_a_draw() {
    let w = world();
    let mut entities = Entities::new(7);
    let mut s = session();
    s.inventory.slots[0] = Some(Stack::new(Item::BOW, 1));
    s.inventory.offhand = Some(Stack::new(Item::BOW, 1));
    s.inventory.slots[1] = Some(Stack::new(Item::ARROW, 8));
    s.start_draw(0, &w, &mut entities);
    assert!(s.drawing());
    assert!(s.swap_hands(0));
    assert!(!s.drawing(), "the incoming bow starts undrawn");
}
