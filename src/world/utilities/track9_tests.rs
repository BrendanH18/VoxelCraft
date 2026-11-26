//! v1.0 Track 9: the utility blocks' ticks Java posts as game events:
//! campfires finishing food, dripleaf tilting, dripstone drips, mud drying
//! to clay and cauldrons filling.

use std::sync::Arc;

use glam::IVec3;

use super::*;
use crate::item::Item;
use crate::world::World;
use crate::world::block::Facing;
use crate::world::chunk::ChunkData;
use crate::world::overworld_blocks as ob;
use crate::world::terrain::Generator;
use crate::world::vibration::{EventSource, GameEvent};

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(5)), Default::default(), 2);
    for y in w.generator.dimension.chunk_rows() {
        w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    w.set_entity_listeners(true);
    w.take_entity_game_events();
    w
}

fn events_at(w: &mut World, cell: IVec3) -> Vec<GameEvent> {
    w.take_entity_game_events().into_iter().filter(|p| p.pos.floor().as_ivec3() == cell).map(|p| p.event).collect()
}

#[test]
fn campfires_post_block_change_for_each_food_they_finish() {
    let mut w = world();
    let p = IVec3::new(3, 110, 3);
    w.set_block(p - IVec3::Y, Block::STONE);
    w.set_block(p, CAMPFIRE);
    assert!(w.campfire_place_food(p, Item::RAW_BEEF));
    assert!(w.campfire_place_food(p, Item::POTATO));
    w.take_entity_game_events();
    for _ in 0..599 {
        w.tick_utilities();
    }
    assert!(events_at(&mut w, p).is_empty());
    w.tick_utilities();
    assert_eq!(events_at(&mut w, p), [GameEvent::BlockChange, GameEvent::BlockChange]);
}

/// `BigDripleafBlock.setTilt`: unstable posts nothing; partial, full and
/// the reset to level each post `block_change` from no one.
#[test]
fn dripleaf_tilts_post_except_the_unstable_step() {
    let mut w = world();
    let p = IVec3::new(5, 110, 5);
    w.set_block(p - IVec3::Y, Block::STONE);
    w.set_block(p, big_dripleaf(Facing::North, Tilt::None, false));
    w.take_entity_game_events();
    w.dripleaf_step(p.as_dvec3() + glam::DVec3::new(0.5, 0.95, 0.5), true);
    assert_eq!(dripleaf_of(w.get_block(p).unwrap()).map(|d| d.1), Some(Tilt::Unstable));
    assert!(events_at(&mut w, p).is_empty(), "unstable doesn't cause a vibration");
    let mut seen = Vec::new();
    for _ in 0..130 {
        w.tick_utilities();
        seen.extend(w.take_entity_game_events().into_iter().filter(|e| e.pos.floor().as_ivec3() == p));
    }
    assert_eq!(seen.iter().map(|e| e.event).collect::<Vec<_>>(), [GameEvent::BlockChange; 3], "partial, full, none");
    assert!(seen.iter().all(|e| e.source == EventSource::NONE && e.affected.is_none()));
}

#[test]
fn drips_and_mud_post_block_change() {
    let mut w = world();
    let root = IVec3::new(8, 120, 8);
    w.set_block(root + IVec3::Y * 2, Block::WATER);
    w.set_block(root + IVec3::Y, Block::STONE);
    w.set_block(root, ob::pointed_dripstone(true, ob::Thickness::Base));
    w.set_block(root - IVec3::Y, ob::pointed_dripstone(true, ob::Thickness::Tip));
    let pot = root - IVec3::Y * 5;
    w.set_block(pot - IVec3::Y, Block::STONE);
    w.set_block(pot, CAULDRON);
    for _ in 0..200 {
        w.dripstone_transfer(root, w.get_block(root).unwrap());
        if !w.utilities.drips.is_empty() {
            break;
        }
    }
    w.take_entity_game_events();
    for _ in 0..54 {
        w.tick_utilities();
    }
    assert_eq!(w.get_block(pot), Some(water_cauldron(1)));
    assert_eq!(events_at(&mut w, pot), [GameEvent::BlockChange], "receiveStalactiteDrip");
    // Mud where the water was dries to clay, posting at the clay.
    let above = root + IVec3::Y * 2;
    w.set_block(above, ob::MUD);
    w.take_entity_game_events();
    for _ in 0..400 {
        w.dripstone_transfer(root, w.get_block(root).unwrap());
        if w.get_block(above) == Some(Block::CLAY) {
            break;
        }
    }
    assert_eq!(w.get_block(above), Some(Block::CLAY));
    assert_eq!(events_at(&mut w, above), [GameEvent::BlockChange]);
}
