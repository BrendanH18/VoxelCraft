//! v1.0 Track 5: Soul Speed through the session, as every adapter runs it.
use super::*;
use crate::{
    enchant::Enchantment,
    entity::PlayerId,
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Item},
    particles::{Kind, Request},
    player::MoveInput,
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};
use std::sync::Arc;

fn soul_sand() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(42)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, 143, z), Block::SOUL_SAND);
        }
    }
    w
}

fn wearing(level: u8) -> PlayerSession {
    let mut s = PlayerSession::new(PlayerId(1), DVec3::new(16.5, 144.0, 16.5));
    let mut boots = Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Iron), 1);
    boots.enchants.set(Enchantment::SoulSpeed, level);
    s.inventory.armor[3] = Some(boots);
    s
}

#[test]
fn walking_on_soul_sand_sheds_souls_every_fifth_tick() {
    let mut w = soul_sand();
    let mut s = wearing(2);
    let input = MoveInput { forward: 1.0, ..Default::default() };
    let mut souls = 0;
    for _ in 0..40 {
        s.player.powder_snow_equipment(&s.inventory.armor, 0);
        s.player.tick(input, &w);
        s.tick_soul_speed(&mut w);
        souls += w.particles.drain().filter(|r| matches!(r, Request::Burst(b) if b.kind == Kind::Soul)).count();
    }
    assert!(s.player.soul.active);
    // Eight fifth ticks, less the first while it lands and starts moving.
    assert!((6..=8).contains(&souls), "{souls}");
    // Standing still sheds none.
    for _ in 0..40 {
        s.player.tick(MoveInput::default(), &w);
    }
    let _ = w.particles.drain().count();
    for _ in 0..20 {
        s.player.tick(MoveInput::default(), &w);
        s.tick_soul_speed(&mut w);
    }
    assert_eq!(w.particles.drain().count(), 0);
}

#[test]
fn rolled_wear_damages_the_boots_outside_creative_and_breaks_them() {
    let mut w = soul_sand();
    let mut s = wearing(3);
    s.player.soul.wear = 5;
    assert!(!s.tick_soul_speed(&mut w));
    assert_eq!(s.inventory.armor[3].unwrap().damage, 5, "no Unbreaking: every point");
    assert_eq!(s.player.soul.wear, 0, "taken");
    s.mode = GameMode::Creative;
    s.player.soul.wear = 5;
    s.tick_soul_speed(&mut w);
    assert_eq!(s.inventory.armor[3].unwrap().damage, 5, "none in Creative");
    s.mode = GameMode::Survival;
    let max = s.inventory.armor[3].unwrap().item.durability().unwrap();
    s.inventory.armor[3].as_mut().unwrap().damage = max - 1;
    s.player.soul.wear = 1;
    assert!(s.tick_soul_speed(&mut w), "the last point breaks them");
    assert!(s.inventory.armor[3].is_none());
}
