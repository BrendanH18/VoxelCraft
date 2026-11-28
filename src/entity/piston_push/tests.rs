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

/// v1.0 final F9: thrown projectiles, fireballs, skulls, bullets and fangs
/// in the stone's way end just ahead of it too, each by its own box; a
/// shulker bullet (`noPhysics`) would pass through blocks, the others
/// stop against them.
#[test]
fn thrown_projectiles_skulls_bullets_and_fangs_move_ahead_of_the_pushed_stone() {
    use crate::entity::{Rng, illager::Fang, pearl::Pearl, wither::Skull};
    let mut w = world();
    let mut e = Entities::new(1);
    let mut rng = Rng::new(5);
    let at = |x: f64| O.as_dvec3() + DVec3::new(x, 0.25, 0.5);
    let mut pearl = Pearl::thrown(crate::entity::PlayerId::HOST, at(2.2), DVec3::X, DVec3::ZERO, &mut rng);
    pearl.pos = at(2.2);
    pearl.vel = DVec3::ZERO;
    e.pearls.push(pearl);
    let mut skull = Skull::new(at(2.3), DVec3::X, false, None);
    skull.motion = DVec3::ZERO;
    e.wither_skulls.push(skull);
    e.fangs.push(Fang::new(O.as_dvec3() + DVec3::new(2.5, 0.0, 0.5), 0.0, 10, 0));
    power_and_push(&mut w, &mut e, 3);
    assert_eq!(w.get_block(O + IVec3::X * 2), Some(Block::STONE));
    let face = O.x as f64 + 3.0 + crate::world::piston::PUSH_OFFSET;
    assert!(close(e.pearls[0].pos.x - 0.125, face), "pearl at {}", e.pearls[0].pos.x);
    assert!(close(e.wither_skulls[0].pos.x - 0.15625, face), "skull at {}", e.wither_skulls[0].pos.x);
    assert!(close(e.fangs[0].pos.x - 0.25, face), "fangs at {}", e.fangs[0].pos.x);
}

/// A slime block pushing sideways sets a projectile's speed on the push
/// axis (`moveCollidedEntities`' bounce), in each one's own units.
#[test]
fn a_slime_block_launches_a_pearl_along_the_push() {
    use crate::entity::{Rng, pearl::Pearl};
    let mut w = world();
    assert!(w.set_block(O + IVec3::X, crate::world::stone_blocks::SLIME_BLOCK));
    let mut e = Entities::new(1);
    let mut rng = Rng::new(5);
    let at = O.as_dvec3() + DVec3::new(2.2, 0.25, 0.5);
    let mut pearl = Pearl::thrown(crate::entity::PlayerId::HOST, at, DVec3::X, DVec3::ZERO, &mut rng);
    pearl.pos = at;
    pearl.vel = DVec3::new(0.0, -3.0, 0.0);
    e.pearls.push(pearl);
    power_and_push(&mut w, &mut e, 3);
    // One block a tick, in the pearl's blocks a second; y untouched.
    assert!(close(e.pearls[0].vel.x, 20.0), "{:?}", e.pearls[0].vel);
    assert!(close(e.pearls[0].vel.y, -3.0));
}

/// Item frames and paintings a piston breaks play their break sound
/// (`ItemFrame.dropItem` always, `Painting.dropItem` with entity drops).
#[test]
fn broken_frames_and_paintings_play_their_break_sound() {
    use crate::entity::{EntityEvent, MobSound, decor::Kind};
    for (item, drops, sound) in [
        (Item::ITEM_FRAME, true, true),
        (Item::ITEM_FRAME, false, true),
        (Item::PAINTING, true, true),
        (Item::PAINTING, false, false),
    ] {
        let mut w = world();
        let mut e = Entities::new(1);
        e.entity_drops = drops;
        assert!(w.set_block(O + IVec3::X * 3, Block::STONE));
        if item == Item::PAINTING {
            for d in [IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                assert!(w.set_block(O + IVec3::X * 2 + d, Block::STONE));
            }
        }
        e.place_decor(&w, item, O + IVec3::X * 3, IVec3::NEG_X, 90.0, &[]);
        power_and_push(&mut w, &mut e, 3);
        assert!(e.decor.is_empty(), "{item:?} broke");
        let played = e.pending_sounds.iter().any(|ev| {
            matches!(ev, EntityEvent::Sound { sound: MobSound::DecorBroken(Kind::Frame { .. } | Kind::Painting(_)), .. })
        });
        assert_eq!(played, sound, "{item:?} with drops {drops}");
    }
}
