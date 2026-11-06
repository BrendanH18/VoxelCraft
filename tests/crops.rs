//! New crops through the shared player transaction used by keyboard,
//! controller and CLI players, with food consumption and actual chest loot.
use glam::{DVec3, IVec3};
use std::sync::Arc;
use voxelcraft::{
    entity::{Entities, PlayerId},
    inventory::Stack,
    item::Item,
    player::EYE_HEIGHT,
    session::{PlayerSession, placement::Outcome},
    world::{
        World,
        block::{Block, Crop},
        chunk::ChunkData,
        crops::{self, Fruit},
        loot,
        terrain::Generator,
    },
};
fn scene() -> (World, PlayerSession) {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    (w, PlayerSession::new(PlayerId::HOST, DVec3::new(4.5, 145., 4.5)))
}
fn aim(s: &mut PlayerSession, eye: DVec3, point: DVec3) {
    let d = (point - eye).normalize();
    s.player.pos = eye - DVec3::Y * EYE_HEIGHT;
    s.player.yaw = d.z.atan2(d.x) as f32;
    s.player.pitch = d.y.asin() as f32;
}
#[test]
fn seeds_plant_only_on_farmland_and_spend_one_through_shared_placement() {
    for (item, block) in [
        (Item::MELON_SEEDS, crops::stem_block(Fruit::Melon, 0)),
        (Item::PUMPKIN_SEEDS, crops::stem_block(Fruit::Pumpkin, 0)),
        (Item::BEETROOT_SEEDS, Block::crop(Crop::Beetroot, 0)),
        (Item::TORCHFLOWER_SEEDS, voxelcraft::world::sniffer_plants::TORCH_CROP),
        (Item::PITCHER_POD, voxelcraft::world::sniffer_plants::PITCHER_CROP),
    ] {
        let (mut w, mut s) = scene();
        let floor = IVec3::new(10, 139, 10);
        s.inventory.slots[0] = Some(Stack::new(item, 3));
        w.set_block(floor, Block::DIRT);
        aim(&mut s, DVec3::new(10.5, 141.6, 13.0), DVec3::new(10.5, 140., 10.5));
        assert!(!matches!(s.place_held(0, &mut w, &[]), Ok(Outcome::Placed { .. })));
        assert_eq!(s.inventory.slots[0], Some(Stack::new(item, 3)));
        w.set_block(floor, Block::WET_FARMLAND);
        assert!(matches!(s.place_held(0,&mut w,&[]),Ok(Outcome::Placed {block:b,..}) if b==block));
        assert_eq!(w.get_block(floor + IVec3::Y), Some(block));
        assert_eq!(s.inventory.slots[0], Some(Stack::new(item, 2)));
        w.set_block(floor, Block::DIRT);
        assert_eq!(w.get_block(floor + IVec3::Y), Some(Block::AIR));
    }
}
#[test]
fn beetroot_soup_eats_in_32_ticks_and_returns_the_bowl() {
    let (_, mut s) = scene();
    let mut e = Entities::new(1);
    s.inventory.slots[0] = Some(Stack::new(Item::BEETROOT_SOUP, 1));
    s.vitals.hunger.food = 8.;
    s.vitals.hunger.saturation = 0.;
    assert_eq!(s.consumption_duration(0), Ok(32));
    for _ in 0..31 {
        assert!(s.tick_consumption(0, true, &mut e).unwrap().completed.is_none());
    }
    assert!(s.tick_consumption(0, true, &mut e).unwrap().completed.is_some());
    assert_eq!(s.inventory.slots[0], Some(Stack::new(Item::BOWL, 1)));
    assert_eq!(s.vitals.hunger.food, 14.);
    assert!((s.vitals.hunger.saturation - 7.2).abs() < 0.001);
}
#[test]
fn existing_java_loot_tables_supply_every_seed_and_snowy_soup() {
    for (table, item) in [
        ("abandoned_mineshaft", Item::MELON_SEEDS),
        ("simple_dungeon", Item::PUMPKIN_SEEDS),
        ("simple_dungeon", Item::BEETROOT_SEEDS),
        ("village/village_snowy_house", Item::BEETROOT_SOUP),
    ] {
        assert!(
            (0..400).any(|seed| loot::table(seed, table).slots.iter().flatten().any(|s| s.item == item)),
            "{table} never provided {}",
            item.name()
        );
    }
}

#[test]
fn pitcher_flower_shared_placement_checks_both_cells_and_spends_one() {
    use voxelcraft::world::sniffer_plants as sp;
    let (mut w, mut s) = scene();
    let floor = IVec3::new(10, 139, 10);
    w.set_block(floor, Block::DIRT);
    w.set_block(floor + IVec3::Y * 2, Block::STONE);
    s.inventory.slots[0] = Some(Stack::new(sp::PITCHER_PLANT, 2));
    aim(&mut s, DVec3::new(10.5, 141.6, 13.), DVec3::new(10.5, 140., 10.5));
    assert!(s.place_held(0, &mut w, &[]).is_err());
    assert_eq!(s.inventory.slots[0].unwrap().count, 2);
    w.set_block(floor + IVec3::Y * 2, Block::AIR);
    assert!(s.place_held(0, &mut w, &[]).is_ok());
    assert_eq!(w.get_block(floor + IVec3::Y), Some(sp::PITCHER_PLANT));
    assert_eq!(w.get_block(floor + IVec3::Y * 2), Some(sp::partner(sp::PITCHER_PLANT)));
    assert_eq!(s.inventory.slots[0].unwrap().count, 1);
}
