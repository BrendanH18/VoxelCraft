//! v1.0 final F7: mobs, dropped items and falling blocks meet scaffolding
//! and powder snow through Java 26.3's `EntityCollisionContext`
//! (`CollisionContext.of(entity)`, which `Entity.collide` builds for every
//! entity). The rules are `ScaffoldingBlock.getCollisionShape`,
//! `PowderSnowBlock.getCollisionShape` and `canEntityWalkOnPowderSnow`,
//! with the `powder_snow_walkable_mobs` tag (rabbit, endermite,
//! silverfish, fox).
use super::*;
use crate::{
    entity::{Mob, MobKind, Rng, equipment::Slot, item::ItemEntity},
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Item},
    physics::{self, CollisionContext, Shape},
    world::{chunk::ChunkData, terrain::Generator},
};
use glam::DVec3;
use std::sync::Arc;

const P: IVec3 = IVec3::new(8, 140, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w.set_block(P - IVec3::Y, Block::STONE);
    w
}

/// Falls a box from `feet` with `context` (taken afresh each step, as each
/// tick's `Entity.collide` does) until it lands; returns where.
fn drop_from(w: &World, feet: DVec3, mut context: impl FnMut(DVec3) -> CollisionContext) -> DVec3 {
    let shape = Shape::new(0.3, 1.8);
    let mut pos = feet;
    for _ in 0..200 {
        let (step, mut vel) = (DVec3::new(0.0, -0.3, 0.0), DVec3::ZERO);
        let context = Some(context(pos));
        let hit = physics::move_box_context(w, &mut pos, &mut vel, step, shape, context);
        if hit.on_ground {
            break;
        }
    }
    pos
}

#[test]
fn mobs_stand_on_scaffolding_from_above_and_fall_through_it_from_inside() {
    let mut w = world();
    for y in 0..3 {
        w.set_block(P + IVec3::Y * y, state(0, false, false));
    }
    let zombie = |pos: DVec3| Mob::new(MobKind::Zombie, pos, 0.0).collision_context();
    let centre = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    // Landing from above: on the top of the column.
    let top = drop_from(&w, centre + DVec3::Y * 4.0, zombie);
    assert!((top.y - (P.y as f64 + 3.0)).abs() < 1e-3, "{top}");
    // Inside the column (a zombie that climbed in or spawned there): the
    // block it is in has no shape for it (its feet aren't above the top),
    // so it sinks onto the top of the one below.
    let inside = drop_from(&w, centre + DVec3::Y * 2.5, zombie);
    assert!((inside.y - (P.y as f64 + 2.0)).abs() < 1e-3, "{inside}");
    // The old empty context popped it up onto the block it was in.
    let mut pos = centre + DVec3::Y * 2.5;
    let mut vel = DVec3::ZERO;
    physics::move_box(&w, &mut pos, &mut vel, DVec3::new(0.0, -0.3, 0.0), Shape::new(0.3, 1.8));
    assert!(pos.y > P.y as f64 + 2.5, "{pos}");
    // An unstable bottom block (distance > 0, nothing below) keeps its
    // 2-pixel rim for a mob inside it.
    w.set_block(P + IVec3::Y * 5, state(1, true, false));
    let rim = drop_from(&w, centre + DVec3::Y * 5.5, zombie);
    assert!((rim.y - (P.y as f64 + 5.125)).abs() < 1e-3, "{rim}");
}

#[test]
fn powder_snow_holds_walkers_and_falling_mobs_but_not_others() {
    let mut w = world();
    w.set_block(P, POWDER_SNOW);
    let centre = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let on_top = P.y as f64 + 1.0;
    let land = |m: &Mob| {
        drop_from(&w, centre + DVec3::Y * 1.0, |pos| CollisionContext { feet_y: pos.y, ..m.collision_context() }).y
    };
    for kind in [MobKind::Rabbit, MobKind::Fox, MobKind::Endermite, MobKind::Silverfish] {
        let m = Mob::new(kind, centre, 0.0);
        assert!(m.walks_on_powder_snow(), "{kind:?}");
        assert!((land(&m) - on_top).abs() < 1e-3, "{kind:?} stands on it");
    }
    let mut zombie = Mob::new(MobKind::Zombie, centre, 0.0);
    assert!(!zombie.walks_on_powder_snow());
    assert!((land(&zombie) - P.y as f64).abs() < 1e-3, "a zombie sinks to the stone");
    zombie.equipment.set(
        Slot::of_piece(ArmorPiece::Boots),
        Some(Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Leather), 1)),
    );
    assert!(zombie.walks_on_powder_snow(), "leather boots");
    assert!((land(&zombie) - on_top).abs() < 1e-3);
    zombie.equipment.set(
        Slot::of_piece(ArmorPiece::Boots),
        Some(Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Iron), 1)),
    );
    assert!(!zombie.walks_on_powder_snow(), "only leather");
    // `fallDistance > 2.5`: the 0.9-high falling shape catches anything.
    let fallen = drop_from(&w, centre + DVec3::Y * 1.5, |pos| CollisionContext::entity(pos.y, 3.0, false));
    assert!((fallen.y - (P.y as f64 + f64::from(0.9_f32))).abs() < 1e-3, "{fallen}");
    // A falling block always lands on it.
    let block = drop_from(&w, centre + DVec3::Y * 1.5, |pos| CollisionContext {
        falling_block: true,
        ..CollisionContext::entity(pos.y, 0.0, false)
    });
    assert!((block.y - on_top).abs() < 1e-3, "{block}");
}

#[test]
fn dropped_items_sink_through_powder_snow_and_fall_inside_scaffolding() {
    let mut rng = Rng::new(3);
    let settle = |w: &World, from: DVec3, rng: &mut Rng| {
        let mut item = ItemEntity::new(Stack::new(Item::from_block(Block::STONE), 1), from, DVec3::ZERO, 0.0, rng);
        for _ in 0..100 {
            item.update(0.05, w);
        }
        item.pos.y
    };
    let centre = P.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let mut w = world();
    w.set_block(P, POWDER_SNOW);
    assert!((settle(&w, centre + DVec3::Y * 1.5, &mut rng) - P.y as f64).abs() < 1e-3, "through the snow");
    for y in 0..3 {
        w.set_block(P + IVec3::Y * y, state(0, false, false));
    }
    let top = settle(&w, centre + DVec3::Y * 3.5, &mut rng);
    assert!((top - (P.y as f64 + 3.0)).abs() < 1e-3, "on the column: {top}");
    // Dropped inside the column it sinks onto the block below instead of
    // being pushed out of the top (`noCollision(this, box)` is the item's
    // own context).
    let inside = settle(&w, centre + DVec3::Y * 1.5, &mut rng);
    assert!((inside - (P.y as f64 + 1.0)).abs() < 1e-3, "inside the column: {inside}");
}
