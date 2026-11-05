//! Hive acquisition through the same transactions used by CLI/controllers.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    enchant::{Enchantment, Enchants},
    entity::{Entities, PlayerId, bee::Occupant},
    inventory::{Stack, StackName},
    item::{Item, Tier, ToolKind},
    player::EYE_HEIGHT,
    rules::GameRules,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
        mining,
    },
    world::{
        World, bees,
        block::{Block, Facing},
        chunk::ChunkData,
        terrain::Generator,
    },
};
const P: IVec3 = IVec3::new(10, 140, 10);
fn scene(item: Item, count: u8) -> (World, PlayerSession, Entities) {
    let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::new(10.5, 140., 13.));
    let eye = DVec3::new(10.5, 141., 13.);
    let point = P.as_dvec3() + DVec3::new(0.5, 0.5, 0.9);
    let d = (point - eye).normalize();
    s.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    s.player.yaw = d.z.atan2(d.x) as f32;
    s.player.pitch = d.y.asin() as f32;
    s.inventory.slots[0] = Some(Stack::new(item, count));
    w.set_block(P, bees::block(false, Facing::South, 5));
    (w, s, Entities::new(4))
}
#[test]
fn shears_and_stacked_bottles_harvest_full_hives_once() {
    let (mut w, mut s, mut e) = scene(Item::SHEARS, 1);
    assert!(matches!(
        s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).unwrap().map(|u| u.used),
        Some(Used::Harvested { cell: P })
    ));
    assert_eq!(s.inventory.slots[0].unwrap().damage, 1);
    assert_eq!(w.drops, vec![(P, Stack::new(Item::HONEYCOMB, 3))]);
    assert_eq!(bees::hive(w.get_block(P).unwrap()).unwrap().2, 0);
    let (mut w, mut s, mut e) = scene(Item::GLASS_BOTTLE, 3);
    s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).unwrap();
    assert_eq!(s.inventory.slots[0], Some(Stack::new(Item::GLASS_BOTTLE, 2)));
    assert!(s.inventory.slots.iter().flatten().any(|s| s.item == Item::HONEY_BOTTLE && s.count == 1));
}
#[test]
fn silk_mining_keeps_occupants_and_placing_restores_them() {
    let (mut w, mut s, mut e) = scene(Item::tool(ToolKind::Axe, Tier::Diamond), 1);
    s.inventory.slots[0].as_mut().unwrap().enchants = Enchants::NONE.with(Enchantment::SilkTouch, 1);
    let mut bee = voxelcraft::entity::Mob::new(voxelcraft::entity::MobKind::Bee, P.as_dvec3(), 0.);
    bee.name = StackName::new("Hive friend").unwrap();
    bee.bee.as_mut().unwrap().nectar = true;
    let o = Occupant { ticks: 700, ..Occupant::from_mob(&bee) };
    assert!(w.bee_enter(P, o.clone()));
    let mut broken = false;
    for _ in 0..80 {
        if matches!(s.tick_mining(0, &mut w, &mut e, &GameRules::default()).unwrap(), mining::Outcome::Broken { .. }) {
            broken = true;
            break;
        }
    }
    assert!(broken);
    assert!(e.mobs.is_empty());
    assert_eq!(w.drops.len(), 1);
    let stack = w.drops[0].1;
    assert_eq!(stack.hive.unwrap().bees[0].as_ref(), Some(&o));
    w.set_block(P - IVec3::Y, Block::STONE);
    s.inventory.slots[0] = Some(stack);
    let eye = s.player.eye();
    let d = (P.as_dvec3() + DVec3::new(0.5, 0., 0.5) - eye).normalize();
    s.player.yaw = d.z.atan2(d.x) as f32;
    s.player.pitch = d.y.asin() as f32;
    s.place_held(0, &mut w, &[]).unwrap();
    assert_eq!(w.beehive(P).unwrap().bees[0].as_ref(), Some(&o));
    assert_eq!(bees::hive(w.get_block(P).unwrap()).unwrap().2, 5);
}

#[test]
fn creative_bottles_still_shrink_and_broken_shears_report_the_break() {
    let (mut w, mut s, mut e) = scene(Item::GLASS_BOTTLE, 3);
    s.mode = voxelcraft::rules::GameMode::Creative;
    s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).unwrap();
    assert_eq!(s.inventory.slots[0], Some(Stack::new(Item::GLASS_BOTTLE, 2)));
    let (mut w, mut s, mut e) = scene(Item::SHEARS, 1);
    s.inventory.slots[0].as_mut().unwrap().damage = Item::SHEARS.durability().unwrap() - 1;
    let result = s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).unwrap().unwrap();
    assert_eq!(result.tool_broken, Some(Item::SHEARS));
    assert!(s.inventory.slots[0].is_none());
}
