//! v1.0 final F7: Java 26.3's `moveCollidedEntities` on the entities
//! v0.17 left alone. Every one is `PushReaction.PUSH_PULL`
//! (`Entity.getPistonPushReaction`); `BlockAttachedEntity.move` breaks item
//! frames and paintings instead of moving them.
use glam::{DVec3, IVec3};
use std::sync::Arc;

use crate::entity::{Entities, projectile::Arrow};
use crate::item::Item;
use crate::world::{World, block::Block, chunk::ChunkData, redstone_blocks as r, terrain::Generator};

const O: IVec3 = IVec3::new(8, 140, 8);
const EAST: u8 = 2;

/// A piston at `O` facing east with stone in front, powered: the stone
/// moves into `O + 2X` over two ticks.
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    for c in [IVec3::new(0, 4, 0), IVec3::new(0, 3, 0)] {
        w.insert_chunk(c, Arc::new(ChunkData::Uniform(Block::AIR)), false);
    }
    assert!(w.set_block(O, r::piston(EAST, false, false)));
    assert!(w.set_block(O + IVec3::X, Block::STONE));
    w
}

fn power_and_push(w: &mut World, e: &mut Entities, ticks: usize) {
    assert!(w.set_block(O - IVec3::Z, r::REDSTONE_BLOCK));
    for _ in 0..ticks {
        w.tick_redstone();
        e.apply_piston_pushes(w);
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn arrows_crystals_and_armor_stands_end_just_ahead_of_the_pushed_stone() {
    let mut w = world();
    let mut e = Entities::new(1);
    // An arrow in front of the stone (its 0.5 box from x + 2.05).
    let mut a = Arrow::shot(DVec3::ZERO, DVec3::X, 0.0, false);
    a.pos = O.as_dvec3() + DVec3::new(2.3, 0.25, 0.5);
    a.vel = DVec3::ZERO;
    e.arrows.push(a);
    // An End crystal (2 wide) with its west side at x + 2.05.
    e.crystals.spawn(O.as_dvec3() + DVec3::new(3.05, 0.0, 0.5), None, false, false);
    // An armor stand standing in the cell the stone moves into.
    e.decor.push(crate::entity::decor::Decor::dispensed_stand(O + IVec3::X * 2, 0.0));
    let stand_x = e.decor[0].pos.x;
    power_and_push(&mut w, &mut e, 3);
    assert_eq!(w.get_block(O + IVec3::X * 2), Some(Block::STONE));
    let face = O.x as f64 + 3.0 + crate::world::piston::PUSH_OFFSET;
    assert!(close(e.arrows[0].pos.x - 0.25, face), "arrow at {}", e.arrows[0].pos.x);
    assert!(close(e.crystals.list[0].pos.x - 1.0, face), "crystal at {}", e.crystals.list[0].pos.x);
    assert!(close(e.decor[0].pos.x - 0.25, face), "stand from {stand_x} to {}", e.decor[0].pos.x);
}

#[test]
fn item_frames_and_paintings_in_the_way_break_and_drop() {
    for item in [Item::ITEM_FRAME, Item::PAINTING] {
        let mut w = world();
        let mut e = Entities::new(1);
        // Hung on the west face of a stone the moving stone runs into.
        assert!(w.set_block(O + IVec3::X * 3, Block::STONE));
        if item == Item::PAINTING {
            // Room for nothing but a 1×1 painting.
            for d in [IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                assert!(w.set_block(O + IVec3::X * 2 + d, Block::STONE));
            }
        }
        e.place_decor(&w, item, O + IVec3::X * 3, IVec3::NEG_X, 90.0, &[]);
        assert_eq!(e.decor.len(), 1, "{item:?} hangs");
        power_and_push(&mut w, &mut e, 3);
        assert!(e.decor.is_empty(), "{item:?} broke");
        assert!(e.items.iter().any(|i| i.stack.item == item), "{item:?} dropped");
    }
}

#[test]
fn a_frame_beside_the_path_stays() {
    let mut w = world();
    let mut e = Entities::new(1);
    // On top of a stone south of the stone's destination: the moving box
    // never sweeps it.
    assert!(w.set_block(O + IVec3::X * 2 + IVec3::Z, Block::STONE));
    e.place_decor(&w, Item::ITEM_FRAME, O + IVec3::X * 2 + IVec3::Z, IVec3::Y, 0.0, &[]);
    assert_eq!(e.decor.len(), 1);
    power_and_push(&mut w, &mut e, 3);
    assert_eq!(e.decor.len(), 1);
}
